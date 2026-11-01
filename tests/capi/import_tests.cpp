#include "document_fixture.h"

#include <cmath>
#include <limits>

namespace pdfoxium::integration {
namespace {
using namespace document_test;

// Native FreeType and Rust Fontations differ only at digit/glyph AA edges in
// these scenes: observed at most16/9216 pixels, max8/255 and mean0.0078/255,
// all localized to the glyph bounds. Keep a tight scene-specific pixel/mean
// budget; independent text, dimensions and colored landmarks still must agree.
constexpr RasterTolerance kImportTextTolerance{2,0.002,0.02};

// The third 3x1 background has a fractional, centered height at96px. Native
// adds the device translation before trimming; Rust snaps before it to keep
// tiled crops stable. Assert BOTH complete boundary rows independently before
// comparing the remaining pixels. No new pixel tolerance or unchecked mask.
Raster CheckNupSnapBoundary(Raster raster, const Library& renderer) {
  EXPECT_EQ(raster.width,96);EXPECT_EQ(raster.height,96);
  if(raster.width!=96||raster.height!=96)return raster;
  for(int y:{32,63})for(int x=64;x<96;++x) {
    const bool colored=(y==32)==(renderer.name()=="native");
    const std::array<uint8_t,4> expected=colored?
        std::array<uint8_t,4>{120,96,72,255}:std::array<uint8_t,4>{255,255,255,255};
    const auto p=(size_t(y)*96+x)*4;
    for(size_t c=0;c<4;++c)EXPECT_EQ(raster.pixels[p+c],expected[c])<<renderer.name()<<" "<<x<<","<<y;
    std::fill_n(raster.pixels.begin()+p,4,255);
  }
  return raster;
}

void MakeImportSource(const Api& api, FPDF_DOCUMENT document, int count=7) {
  Handle font(api.FPDFText_LoadStandardFont(document,"Helvetica"),api.FPDFFont_Close);
  ASSERT_NE(font.get(),nullptr);
  for(int index=0;index<count;++index) {
    Handle page(api.FPDFPage_New(document,index,96+index*2,96),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    Handle background(api.FPDFPageObj_CreateNewRect(0,0,96+index*2,96),api.FPDFPageObj_Destroy);ASSERT_NE(background.get(),nullptr);
    ASSERT_TRUE(api.FPDFPageObj_SetFillColor(background.get(),32+index*20,64+index*16,96+index*12,255));
    ASSERT_TRUE(api.FPDFPath_SetDrawMode(background.get(),FPDF_FILLMODE_WINDING,0));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),background.release()));
    Handle text(api.FPDFPageObj_CreateTextObj(document,font.get(),12),api.FPDFPageObj_Destroy);ASSERT_NE(text.get(),nullptr);
    const auto label=Wide(std::u16string(1,static_cast<char16_t>('1'+index)));
    ASSERT_TRUE(api.FPDFText_SetText(text.get(),label.data()));
    const FS_MATRIX matrix={1,0,0,1,64,60};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(text.get(),&matrix));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),text.release()));
    Handle bitmap(api.FPDFBitmap_Create(2,2,1),api.FPDFBitmap_Destroy);ASSERT_NE(bitmap.get(),nullptr);
    ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,2,2,0xffe0b020));
    Handle image(api.FPDFPageObj_NewImageObj(document),api.FPDFPageObj_Destroy);ASSERT_NE(image.get(),nullptr);
    FPDF_PAGE pages[]={page.get()};ASSERT_TRUE(api.FPDFImageObj_SetBitmap(pages,1,image.get(),bitmap.get()));
    ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image.get(),16,0,0,16,12,12));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),image.release()));
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
  }
}

void CheckImportedPages(const Api& api,FPDF_DOCUMENT document,
                         const std::vector<int>& labels,std::vector<Raster>* images) {
  ASSERT_EQ(api.FPDF_GetPageCount(document),labels.size());images->clear();
  for(size_t index=0;index<labels.size();++index) {
    Handle page(api.FPDF_LoadPage(document,static_cast<int>(index)),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    const int label=labels[index];
    EXPECT_DOUBLE_EQ(api.FPDF_GetPageWidth(page.get()),96+label*2);
    EXPECT_DOUBLE_EQ(api.FPDF_GetPageHeight(page.get()),96);
    EXPECT_EQ(api.FPDFPage_CountObjects(page.get()),3);
    Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
    ASSERT_EQ(api.FPDFText_CountChars(text.get()),1);
    EXPECT_EQ(api.FPDFText_GetUnicode(text.get(),0),static_cast<unsigned int>('1'+label));
    auto image=Render(api,page.get(),96,96);
    ASSERT_EQ(image.pixels.size(),96u*96*4);
    EXPECT_EQ(image.pixels[0],96+label*12);
    EXPECT_EQ(image.pixels[1],64+label*16);
    EXPECT_EQ(image.pixels[2],32+label*20);
    images->push_back(std::move(image));
  }
}

void ImportRoundTrip(const Library& writer,FPDF_DOCUMENT document,
                      const std::vector<int>& labels,Artifacts& artifacts,const std::string& name) {
  std::vector<Raster> before;
  ASSERT_NO_FATAL_FAILURE(CheckImportedPages(writer.api(),document,labels,&before));
  const auto saved=SaveDocument(writer.api(),document,artifacts,writer.name()+"-"+name);
  ASSERT_FALSE(saved.empty());
  for(const auto* reader:Implementations()) {
    const auto& api=reader->api();Handle doc(api.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    std::vector<Raster> after;ASSERT_NO_FATAL_FAILURE(CheckImportedPages(api,doc.get(),labels,&after));
    for(size_t index=0;index<before.size();++index) {
      CompareRaster(before[index],after[index],artifacts,writer.name()+"-"+name+"-"+reader->name()+"-page"+std::to_string(index),kImportTextTolerance);
    }
  }
}

TEST(Imports, IndexedAllReorderedRepeatedAndInsertionPositions) {
  Artifacts artifacts;
  struct Case{std::vector<int> selection;bool all;int position;std::vector<int> expected;int initial_pages=1;};
  const std::vector<Case> cases={{{},true,0,{0,1,2,3,4,5,6,0}},{{2,0,2},false,0,{2,0,2,0}},{{1,3},false,1,{0,1,3}},{{4,5},false,1,{0,4,5,1,2},3}};
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();int case_index=0;
    for(const auto& item:cases) {
      Handle source(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);Handle target(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
      ASSERT_NO_FATAL_FAILURE(MakeImportSource(api,source.get()));
      ASSERT_NO_FATAL_FAILURE(MakeImportSource(api,target.get(),item.initial_pages));
      const int placeholder=0;
      const int* selected=item.all?nullptr:item.selection.empty()?&placeholder:item.selection.data();
      ASSERT_TRUE(api.FPDF_ImportPagesByIndex(target.get(),source.get(),selected,item.selection.size(),item.position));
      EXPECT_EQ(api.FPDF_GetPageCount(source.get()),7);
      source.reset();
      ASSERT_NO_FATAL_FAILURE(ImportRoundTrip(*library,target.get(),item.expected,artifacts,"indexed"+std::to_string(case_index++)));
    }
  }
}

TEST(Imports, OneBasedRangesNullAllAndMalformedRanges) {
  Artifacts artifacts;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    for(const auto& item:std::vector<std::pair<const char*,std::vector<int>>>{{nullptr,{0,1,2,3,4,5,6}},{"",{0,1,2,3,4,5,6}},{"1,3,5-7",{0,2,4,5,6}},{"3,1,3",{2,0,2}}}) {
      Handle source(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);Handle target(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
      ASSERT_NO_FATAL_FAILURE(MakeImportSource(api,source.get()));
      ASSERT_TRUE(api.FPDF_ImportPages(target.get(),source.get(),item.first,0));source.reset();
      ASSERT_NO_FATAL_FAILURE(ImportRoundTrip(*library,target.get(),item.second,artifacts,item.first?item.first:"null-all"));
    }
    for(const char* invalid:{"0","8","1-8","abc","1,,2","3-1","-1"}) {
      Handle source(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);Handle target(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
      ASSERT_NO_FATAL_FAILURE(MakeImportSource(api,source.get()));
      EXPECT_FALSE(api.FPDF_ImportPages(target.get(),source.get(),invalid,0))<<invalid;
      EXPECT_EQ(api.FPDF_GetPageCount(source.get()),7);
      Handle page(api.FPDF_LoadPage(source.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      EXPECT_EQ(Render(api,page.get(),96,96).pixels.size(),96u*96*4);
    }
  }
}

TEST(Imports, IndexedFailuresKeepSourceUsable) {
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    for(const auto& selection:std::vector<std::vector<int>>{{-1},{7},{1,99}}) {
      Handle source(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);Handle target(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
      ASSERT_NO_FATAL_FAILURE(MakeImportSource(api,source.get()));
      EXPECT_FALSE(api.FPDF_ImportPagesByIndex(target.get(),source.get(),selection.data(),selection.size(),0));
      const int first = 0;
      EXPECT_FALSE(api.FPDF_ImportPagesByIndex(target.get(),source.get(),&first,0,0));
      EXPECT_EQ(api.FPDF_GetPageCount(source.get()),7);
    }
    for(int index:{-1,2}) {
      Handle source(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);Handle target(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
      ASSERT_NO_FATAL_FAILURE(MakeImportSource(api,source.get(),1));
      const int selected=0;
      EXPECT_FALSE(api.FPDF_ImportPagesByIndex(target.get(),source.get(),&selected,1,index));
    }
  }
}

TEST(Imports, RangeInsertionPositionsAndInheritedResourcesAreIndependent) {
  Artifacts artifacts;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    for(int position:{0,1,2}) {
      Handle source(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);Handle target(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
      ASSERT_NO_FATAL_FAILURE(MakeImportSource(api,source.get(),3));
      ASSERT_NO_FATAL_FAILURE(MakeImportSource(api,target.get(),2));
      ASSERT_TRUE(api.FPDF_ImportPages(target.get(),source.get(),"3",position));source.reset();
      std::vector<int> expected={0,1};expected.insert(expected.begin()+position,2);
      ASSERT_NO_FATAL_FAILURE(ImportRoundTrip(*library,target.get(),expected,artifacts,"range-position"+std::to_string(position)));
    }
    const auto bytes=PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 96 96] /Rotate 90 /Resources << /Font << /F1 5 0 R >> >> >>",
      "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>",
      PdfStream("","BT /F1 12 Tf 20 30 Td (A) Tj ET"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"});
    Handle source(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(source.get(),nullptr);
    Handle target(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);ASSERT_NE(target.get(),nullptr);
    ASSERT_TRUE(api.FPDF_ImportPages(target.get(),source.get(),nullptr,0));source.reset();
    CrossRead(*library,target.get(),artifacts,"inherited-page-resources",[&](const Api& read,FPDF_DOCUMENT doc){
      ASSERT_EQ(read.FPDF_GetPageCount(doc),1);Handle page(read.FPDF_LoadPage(doc,0),read.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      EXPECT_EQ(read.FPDFPage_GetRotation(page.get()),1);EXPECT_DOUBLE_EQ(read.FPDF_GetPageWidth(page.get()),96);EXPECT_DOUBLE_EQ(read.FPDF_GetPageHeight(page.get()),96);
      Handle text(read.FPDFText_LoadPage(page.get()),read.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);ASSERT_EQ(read.FPDFText_CountChars(text.get()),1);EXPECT_EQ(read.FPDFText_GetUnicode(text.get(),0),'A');
    },kImportTextTolerance);
  }
}

TEST(Imports, NupGridsHaveExpectedSheetSizesAndCellColors) {
  Artifacts artifacts;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    for(const auto grid:{std::array<size_t,2>{1,1},{2,2},{3,1}}) {
      const bool identity_grid=grid[0]==1&&grid[1]==1;
      const int source_count=identity_grid?1:5;
      const int output_extent=identity_grid?96:192;
      Handle source(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);ASSERT_NO_FATAL_FAILURE(MakeImportSource(api,source.get(),source_count));
      const size_t per=grid[0]*grid[1];
      Handle nup(api.FPDF_ImportNPagesToOne(source.get(),static_cast<float>(output_extent),static_cast<float>(output_extent),grid[0],grid[1]),api.FPDF_CloseDocument);ASSERT_NE(nup.get(),nullptr);
      EXPECT_EQ(api.FPDF_GetPageCount(nup.get()),(source_count+per-1)/per);
      source.reset();
      const auto inspect=[&](const Api& read,FPDF_DOCUMENT doc) {
        EXPECT_EQ(read.FPDF_GetPageCount(doc),(source_count+per-1)/per);
        for(size_t sheet=0;sheet<(source_count+per-1)/per;++sheet) {
          Handle page(read.FPDF_LoadPage(doc,static_cast<int>(sheet)),read.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
          EXPECT_DOUBLE_EQ(read.FPDF_GetPageWidth(page.get()),output_extent);EXPECT_DOUBLE_EQ(read.FPDF_GetPageHeight(page.get()),output_extent);
          auto raster=Render(read,page.get(),output_extent,output_extent);ASSERT_EQ(raster.pixels.size(),static_cast<size_t>(output_extent)*output_extent*4);
          artifacts.Image(library->name()+"-nup-sheet"+std::to_string(sheet),raster);
          // Native layout fills left-to-right and top-to-bottom, letterboxing
          // each source into its cell. Check a colored point away from text.
          for(size_t cell=0;cell<per;++cell) {
            size_t x=(cell%grid[0])*output_extent/grid[0]+output_extent/grid[0]/2;
            size_t y=(cell/grid[0])*output_extent/grid[1]+output_extent/grid[1]/2;
            const size_t offset=(y*output_extent+x)*4;
            const int label=static_cast<int>(sheet*per+cell);
            if(label<source_count) {
              EXPECT_EQ(raster.pixels[offset],96+label*12);EXPECT_EQ(raster.pixels[offset+1],64+label*16);EXPECT_EQ(raster.pixels[offset+2],32+label*20);
            } else {EXPECT_EQ(raster.pixels[offset],255);EXPECT_EQ(raster.pixels[offset+1],255);EXPECT_EQ(raster.pixels[offset+2],255);}
          }
        }
      };
      const auto name="nup"+std::to_string(grid[0])+"x"+std::to_string(grid[1]);
      if(grid[0]!=3) {
        CrossRead(*library,nup.get(),artifacts,name,inspect,kImportTextTolerance);
      } else {
        ASSERT_NO_FATAL_FAILURE(inspect(api,nup.get()));
        Handle page(api.FPDF_LoadPage(nup.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
        const auto before=CheckNupSnapBoundary(Render(api,page.get(),96,96),*library);
        const auto bytes=SaveDocument(api,nup.get(),artifacts,library->name()+"-"+name);ASSERT_FALSE(bytes.empty());
        for(const auto* reader:Implementations()) {
          SCOPED_TRACE(library->name()+"->"+reader->name());const auto& read=reader->api();
          Handle loaded(read.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(loaded.get(),nullptr);
          ASSERT_NO_FATAL_FAILURE(inspect(read,loaded.get()));
          Handle reopened(read.FPDF_LoadPage(loaded.get(),0),read.FPDF_ClosePage);ASSERT_NE(reopened.get(),nullptr);
          const auto after=CheckNupSnapBoundary(Render(read,reopened.get(),96,96),*reader);
          CompareRaster(before,after,artifacts,library->name()+name+reader->name(),kImportTextTolerance);
        }
      }
    }
    Handle source(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);ASSERT_NO_FATAL_FAILURE(MakeImportSource(api,source.get(),1));
    for(const auto grid:{std::array<size_t,2>{0,1},{1,0}}) {
      Handle result(api.FPDF_ImportNPagesToOne(source.get(),100,100,grid[0],grid[1]),api.FPDF_CloseDocument);EXPECT_EQ(result.get(),nullptr);
    }
    for(float extent:{0.0f,-1.0f}) {
      Handle result(api.FPDF_ImportNPagesToOne(source.get(),extent,100,1,1),api.FPDF_CloseDocument);EXPECT_EQ(result.get(),nullptr);
    }
  }
}

TEST(Imports, TemplatesOutliveSourcesAndYieldIndependentFormObjects) {
  Artifacts artifacts;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    for(int source_index:{0,2}) {
      Handle source(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);Handle destination(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
      ASSERT_NO_FATAL_FAILURE(MakeImportSource(api,source.get(),3));
      EXPECT_EQ(api.FPDF_NewXObjectFromPage(destination.get(),source.get(),-1),nullptr);
      EXPECT_EQ(api.FPDF_NewXObjectFromPage(destination.get(),source.get(),3),nullptr);
      Handle object(api.FPDF_NewXObjectFromPage(destination.get(),source.get(),source_index),api.FPDF_CloseXObject);ASSERT_NE(object.get(),nullptr);
      source.reset();
      Handle page(api.FPDFPage_New(destination.get(),0,220,120),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      {
        Handle unused(api.FPDF_NewFormObjectFromXObject(object.get()),api.FPDFPageObj_Destroy);ASSERT_NE(unused.get(),nullptr);
      }
      for(float left:{4.0f,114.0f}) {
        Handle form(api.FPDF_NewFormObjectFromXObject(object.get()),api.FPDFPageObj_Destroy);ASSERT_NE(form.get(),nullptr);
        EXPECT_EQ(api.FPDFPageObj_GetType(form.get()),FPDF_PAGEOBJ_FORM);
        const FS_MATRIX matrix={1,0,0,1,left,12};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(form.get(),&matrix));
        ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),form.release()));
      }
      object.reset();
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      const auto before=Render(api,page.get(),220,120);page.reset();
      const auto saved=SaveDocument(api,destination.get(),artifacts,library->name()+"-form-template");
      for(const auto* reader:Implementations()) {
        const auto& read=reader->api();Handle doc(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
        Handle reloaded(read.FPDF_LoadPage(doc.get(),0),read.FPDF_ClosePage);ASSERT_NE(reloaded.get(),nullptr);
        EXPECT_EQ(read.FPDFPage_CountObjects(reloaded.get()),2);
        CompareRaster(before,Render(read,reloaded.get(),220,120),artifacts,library->name()+"-forms-"+reader->name(),kImportTextTolerance);
      }
    }
  }
}

TEST(Imports, NonfiniteGeometryIsRejectedBeforeCreatingTemplates) {
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();Handle source(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    ASSERT_NO_FATAL_FAILURE(MakeImportSource(api,source.get(),1));
    std::vector<float> rejected={-std::numeric_limits<float>::infinity()};
    if(library->name()=="rust") {
      // The pinned native guard only checks <=0, leaving +Inf/NaN to flow into
      // layout arithmetic. Do not invoke that unvalidated path in this shared
      // process; assert correct Rust boundary handling instead.
      rejected.push_back(std::numeric_limits<float>::infinity());
      rejected.push_back(std::numeric_limits<float>::quiet_NaN());
    }
    for(float extent:rejected) {
      Handle invalid_width(api.FPDF_ImportNPagesToOne(source.get(),extent,100,2,1),api.FPDF_CloseDocument);
      Handle invalid_height(api.FPDF_ImportNPagesToOne(source.get(),100,extent,2,1),api.FPDF_CloseDocument);
      EXPECT_EQ(invalid_width.get(),nullptr);EXPECT_EQ(invalid_height.get(),nullptr);
    }
    EXPECT_EQ(api.FPDF_GetPageCount(source.get()),1);
  }
}

TEST(Imports, ViewerPreferencesCopyOverwriteAndSurviveSourceClosure) {
  Artifacts artifacts;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    const auto source_bytes=OnePage("/ViewerPreferences << /PrintScaling /None /NumCopies 4 /Duplex /DuplexFlipLongEdge /PrintPageRange [0 1 3 4] /Direction /R2L >>");
    const auto target_bytes=OnePage("/ViewerPreferences << /PrintScaling /AppDefault /NumCopies 2 >>");
    Handle source(api.FPDF_LoadMemDocument64(source_bytes.data(),source_bytes.size(),nullptr),api.FPDF_CloseDocument);
    Handle target(api.FPDF_LoadMemDocument64(target_bytes.data(),target_bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(source.get(),nullptr);ASSERT_NE(target.get(),nullptr);
    ASSERT_TRUE(api.FPDF_CopyViewerPreferences(target.get(),source.get()));source.reset();
    CrossRead(*library,target.get(),artifacts,"viewer-preferences",[&](const Api& read,FPDF_DOCUMENT doc){
      EXPECT_FALSE(read.FPDF_VIEWERREF_GetPrintScaling(doc));EXPECT_EQ(read.FPDF_VIEWERREF_GetNumCopies(doc),4);
      EXPECT_EQ(read.FPDF_VIEWERREF_GetDuplex(doc),DuplexFlipLongEdge);
      auto range=read.FPDF_VIEWERREF_GetPrintPageRange(doc);ASSERT_NE(range,nullptr);ASSERT_EQ(read.FPDF_VIEWERREF_GetPrintPageRangeCount(range),4u);
      for(size_t i=0;i<4;++i)EXPECT_EQ(read.FPDF_VIEWERREF_GetPrintPageRangeElement(range,i),(std::array<int,4>{0,1,3,4})[i]);
    });
    Handle absent(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    EXPECT_FALSE(api.FPDF_CopyViewerPreferences(target.get(),absent.get()));
    EXPECT_EQ(api.FPDF_VIEWERREF_GetNumCopies(target.get()),4);
  }
}

}  // namespace
}  // namespace pdfoxium::integration
