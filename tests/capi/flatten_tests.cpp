#include "form_fixture.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

// Explicit appearance streams distinguish normal, print-only and hidden
// annotations, plus a widget whose existing appearance must also be flattened.
std::vector<uint8_t> FlattenFixture() {
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Annots [5 0 R 6 0 R 7 0 R 8 0 R] /Contents 13 0 R >>",
      "<< /Fields [8 0 R] >>",
      "<< /Type /Annot /Subtype /Square /Rect [20 20 60 60] /F 0 /AP << /N 9 0 R >> >>",
      "<< /Type /Annot /Subtype /Square /Rect [80 20 120 60] /F 36 /AP << /N 10 0 R >> >>",
      "<< /Type /Annot /Subtype /Square /Rect [140 20 180 60] /F 2 /AP << /N 11 0 R >> >>",
      "<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /T (button) /Rect [20 100 60 140] /P 3 0 R /F 4 /AP << /N 12 0 R >> >>",
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 40 40]", "1 0 0 rg 0 0 40 40 re f"),
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 40 40]", "0 1 0 rg 0 0 40 40 re f"),
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 40 40]", "1 0 1 rg 0 0 40 40 re f"),
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 40 40]", "0 0 1 rg 0 0 40 40 re f"),
      PdfStream("", "1 g 0 0 200 200 re f")});
}

std::array<uint8_t,4> At(const Raster& raster,int x,int y) {
  const auto i=(static_cast<size_t>(y)*raster.width+x)*4;
  return {raster.pixels.at(i),raster.pixels.at(i+1),raster.pixels.at(i+2),raster.pixels.at(i+3)};
}

TEST(Flatten, DisplayAndPrintPreserveSelectedAppearanceThenSaveAcrossBothReaders) {
  Artifacts artifacts;
  std::array<std::vector<Raster>,2> observations;
  size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());
    const auto& api=writer->api();
    for(int usage:{FLAT_NORMALDISPLAY,FLAT_PRINT}) {
      SCOPED_TRACE(usage);
      const auto fixture=FlattenFixture();
      Raster before;
      {
        forms::Session session(api,fixture);
        ASSERT_TRUE(session.valid());
        before=session.Draw(200,200,0,FPDF_ANNOT|(usage==FLAT_PRINT?FPDF_PRINTING:0));
        ASSERT_EQ(before.pixels.size(),200u*200*4);
        EXPECT_EQ(At(before,40,80),(std::array<uint8_t,4>{255,0,0,255}));
        EXPECT_EQ(At(before,160,160),(std::array<uint8_t,4>{255,255,255,255}));
        if(usage==FLAT_PRINT) {
          EXPECT_EQ(At(before,40,160),(std::array<uint8_t,4>{255,255,255,255}));
          EXPECT_EQ(At(before,100,160),(std::array<uint8_t,4>{0,255,0,255}));
        } else {
          EXPECT_EQ(At(before,40,160),(std::array<uint8_t,4>{0,0,255,255}));
          EXPECT_EQ(At(before,100,160),(std::array<uint8_t,4>{255,255,255,255}));
        }
      }
      Handle document(api.FPDF_LoadMemDocument64(fixture.data(),fixture.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);
      Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
      ASSERT_NE(page.get(),nullptr);
      EXPECT_EQ(api.FPDFPage_GetAnnotCount(page.get()),4);
      EXPECT_EQ(api.FPDFPage_Flatten(page.get(),usage),FLATTEN_SUCCESS);
      EXPECT_EQ(api.FPDFPage_GetAnnotCount(page.get()),0);
      EXPECT_GE(api.FPDFPage_CountObjects(page.get()),1);
      EXPECT_EQ(api.FPDFPage_Flatten(page.get(),usage),FLATTEN_NOTHINGTODO);
      // Reload the page after flattening changes its content streams; do not
      // depend on a parser's already-cached pre-flatten object list.
      page.reset();
      Handle changed(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
      ASSERT_NE(changed.get(),nullptr);
      const auto after=Render(api,changed.get(),200,200);
      const auto label=writer->name()+"-usage"+std::to_string(usage);
      auto expected=before;
      if(writer->name()=="native" && usage==FLAT_NORMALDISPLAY) {
        // Native ParserAnnots ignores NoView when flattening for normal
        // display. Record its print-only square explicitly; Rust must keep
        // that square invisible, preserving the original screen appearance.
        for(int y=140;y<180;++y) for(int x=80;x<120;++x) {
          const auto i=(static_cast<size_t>(y)*200+x)*4;
          const std::array<uint8_t,4> green{0,255,0,255};
          std::copy(green.begin(),green.end(),expected.pixels.begin()+i);
        }
      }
      CompareRaster(expected,after,artifacts,label+"-flattened");
      observations[backend].push_back(after);
      changed.reset();
      const auto bytes=SaveDocument(api,document.get(),artifacts,label);
      std::array<Raster,2> reopened;
      size_t reader_index=0;
      for(const auto* reader:Implementations()) {
        const auto& read=reader->api();
        Handle loaded(read.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),read.FPDF_CloseDocument);
        ASSERT_NE(loaded.get(),nullptr);
        Handle loaded_page(read.FPDF_LoadPage(loaded.get(),0),read.FPDF_ClosePage);
        ASSERT_NE(loaded_page.get(),nullptr);
        EXPECT_EQ(read.FPDFPage_GetAnnotCount(loaded_page.get()),0);
        EXPECT_EQ(read.FPDFPage_Flatten(loaded_page.get(),usage),FLATTEN_NOTHINGTODO);
        reopened[reader_index]=Render(read,loaded_page.get(),200,200);
        CompareRaster(after,reopened[reader_index],artifacts,label+"-to-"+reader->name());
        ++reader_index;
      }
      CompareRaster(reopened[0],reopened[1],artifacts,label+"-readers");
    }
    ++backend;
  }
  for(size_t index=0;index<observations[0].size();++index) {
    auto native=observations[0][index];
    if(index==0) for(int y=140;y<180;++y) for(int x=80;x<120;++x) {
      const auto i=(static_cast<size_t>(y)*200+x)*4;
      std::fill_n(native.pixels.begin()+i,4,255);
    }
    CompareRaster(native,observations[1][index],artifacts,"usage-"+std::to_string(index));
  }
}

TEST(Flatten, EmptyPageAndNullPageHaveDistinctResults) {
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    EXPECT_EQ(api.FPDFPage_Flatten(nullptr,FLAT_NORMALDISPLAY),FLATTEN_FAIL);
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);
    Handle page(api.FPDFPage_New(document.get(),0,100,100),api.FPDF_ClosePage);
    ASSERT_NE(page.get(),nullptr);
    EXPECT_EQ(api.FPDFPage_Flatten(page.get(),FLAT_NORMALDISPLAY),FLATTEN_NOTHINGTODO);
    EXPECT_EQ(api.FPDFPage_Flatten(page.get(),FLAT_PRINT),FLATTEN_NOTHINGTODO);
  }
}

TEST(DocumentExtensions, CatalogPageModesIncludeAbsentAndUnknownValues) {
  const std::array<std::pair<std::string,int>,8> modes={{{"",PAGEMODE_USENONE},
      {"/UseNone",PAGEMODE_USENONE},{"/UseOutlines",PAGEMODE_USEOUTLINES},
      {"/UseThumbs",PAGEMODE_USETHUMBS},{"/FullScreen",PAGEMODE_FULLSCREEN},
      {"/UseOC",PAGEMODE_USEOC},{"/UseAttachments",PAGEMODE_USEATTACHMENTS},
      {"/UnknownMode",PAGEMODE_UNKNOWN}}};
  std::array<std::vector<int>,2> actual;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();
    EXPECT_EQ(api.FPDFDoc_GetPageMode(nullptr),PAGEMODE_UNKNOWN);
    for(const auto& [name,expected]:modes) {
      const auto bytes=PdfFixture({"<< /Type /Catalog /Pages 2 0 R "+(name.empty()?"":"/PageMode "+name)+" >>",
          "<< /Type /Pages /Kids [3 0 R] /Count 1 >>","<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>"});
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);
      const auto result=api.FPDFDoc_GetPageMode(document.get());
      EXPECT_EQ(result,expected)<<name;
      actual[backend].push_back(result);
    }
    ++backend;
  }
  EXPECT_EQ(actual[0],actual[1]);
}

struct UnsupportedHost : UNSUPPORT_INFO {
  UnsupportedHost() : UNSUPPORT_INFO{} {
    version=1;
    FSDK_UnSupport_Handler=[](UNSUPPORT_INFO* p,int kind) {
      auto& host=*static_cast<UnsupportedHost*>(p);
      try { host.types.push_back(kind); } catch(...) { host.failed=true; }
    };
  }
  std::vector<int> types;
  bool failed=false;
};

TEST(DocumentExtensions, UnsupportedHandlersReceiveRealDocumentAndAnnotationReports) {
  // Registrations are process-wide and the API has no unregister operation.
  // Retain every installed callback descriptor until normal process exit.
  static std::array<UnsupportedHost,4> hosts;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    auto& first=hosts[backend*2]; auto& replacement=hosts[backend*2+1];
    first.types.clear();replacement.types.clear();
    EXPECT_FALSE(api.FSDK_SetUnSpObjProcessHandler(nullptr));
    UNSUPPORT_INFO wrong{};wrong.version=2;
    EXPECT_FALSE(api.FSDK_SetUnSpObjProcessHandler(&wrong));
    ASSERT_TRUE(api.FSDK_SetUnSpObjProcessHandler(&first));
    const auto collection=PdfFixture({"<< /Type /Catalog /Pages 2 0 R /Collection << >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>","<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>"});
    {
      Handle document(api.FPDF_LoadMemDocument64(collection.data(),collection.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);
      EXPECT_NE(std::find(first.types.begin(),first.types.end(),FPDF_UNSP_DOC_PORTABLECOLLECTION),first.types.end());
    }
    ASSERT_TRUE(api.FSDK_SetUnSpObjProcessHandler(&replacement));
    const auto count=first.types.size();
    const auto annotations=PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Annots [4 0 R] >>",
        "<< /Type /Annot /Subtype /Movie /Rect [10 10 90 90] >>"});
    {
      // Unsupported annotation reports belong to the SDK page-view loading
      // lifecycle, reached by FORM_OnAfterLoadPage in Session's constructor.
      forms::Session session(api,annotations);
      ASSERT_TRUE(session.valid());
      session.Draw(100,100,0,FPDF_ANNOT);
      EXPECT_NE(std::find(replacement.types.begin(),replacement.types.end(),FPDF_UNSP_ANNOT_MOVIE),replacement.types.end());
    }
    EXPECT_EQ(first.types.size(),count);
    replacement.types.clear();
    const auto normal=PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>","<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>"});
    {
      Handle document(api.FPDF_LoadMemDocument64(normal.data(),normal.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);
      Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
      ASSERT_NE(page.get(),nullptr);
    }
    EXPECT_TRUE(replacement.types.empty());
    EXPECT_FALSE(first.failed); EXPECT_FALSE(replacement.failed);
    ++backend;
  }
}

}  // namespace
}  // namespace pdfoxium::integration
