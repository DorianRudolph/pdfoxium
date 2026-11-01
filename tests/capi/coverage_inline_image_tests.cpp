#include "coverage_editing_fixture.h"

#include <fstream>
#include <iterator>

namespace pdfoxium::integration {
namespace {
using namespace coverage_editing;
constexpr std::array<uint8_t,4> kInlineWhite{255,255,255,255};

// Test-only literal encoders make the expected image samples independent of
// either PDF engine. Stored DEFLATE blocks need no compression dependency.
std::string InlineZlib(const std::string& input) {
  std::string out("\x78\x01\x01",3); const unsigned n=input.size();
  out.push_back(static_cast<char>(n)); out.push_back(static_cast<char>(n>>8));
  out.push_back(static_cast<char>(~n)); out.push_back(static_cast<char>((~n)>>8)); out+=input;
  uint32_t a=1,b=0; for(unsigned char c:input){a=(a+c)%65521;b=(b+a)%65521;}
  const uint32_t sum=(b<<16)|a;for(int shift:{24,16,8,0})out.push_back(static_cast<char>(sum>>shift)); return out;
}
std::string InlineHex(const std::string& data) {std::string out;const char* hex="0123456789ABCDEF";for(unsigned char c:data){out+=hex[c>>4];out+=hex[c&15];}return out+">";}
std::string Inline85(const std::string& input) {
  std::string out;
  for(size_t offset=0;offset<input.size();offset+=4) {const size_t n=std::min<size_t>(4,input.size()-offset);uint32_t value=0;for(size_t i=0;i<4;++i)value=(value<<8)|(i<n?static_cast<uint8_t>(input[offset+i]):0);
    char bytes[5];for(int i=4;i>=0;--i){bytes[i]=static_cast<char>('!'+value%85);value/=85;}out.append(bytes,n+1);}
  return out+"~>";
}
std::string InlineLzw(const std::string& data) {
  std::string out;uint32_t pending=0;int bits=0;
  auto code=[&](unsigned value){pending=(pending<<9)|value;bits+=9;while(bits>=8){bits-=8;out.push_back(static_cast<char>(pending>>bits));}};
  code(256);for(unsigned char c:data)code(c);code(257);if(bits)out.push_back(static_cast<char>(pending<<(8-bits)));return out;
}
std::string InlineRead(const std::filesystem::path& path) {std::ifstream in(path,std::ios::binary);EXPECT_TRUE(in.good());return {std::istreambuf_iterator<char>(in),std::istreambuf_iterator<char>()};}
std::vector<uint8_t> InlinePdf(const std::string& dictionary,const std::string& payload,const std::string& resources="") {
  return editing::SceneFixture("q 48 0 0 48 16 16 cm 0 1 0 rg BI "+dictionary+" ID\n"+payload+"\nEI Q\n0 0 1 rg 96 96 16 16 re f\n",resources);
}
FPDF_PAGEOBJECT InlineObject(const Api& api,FPDF_PAGE page) {
  FPDF_PAGEOBJECT found=nullptr;
  for(int i=0;i<api.FPDFPage_CountObjects(page);++i){auto obj=api.FPDFPage_GetObject(page,i);if(api.FPDFPageObj_GetType(obj)==FPDF_PAGEOBJ_IMAGE){EXPECT_EQ(found,nullptr);found=obj;}}
  EXPECT_NE(found,nullptr);return found;
}
std::string InlineDecoded(const Api& api,FPDF_PAGEOBJECT object) {
  const auto n=api.FPDFImageObj_GetImageDataDecoded(object,nullptr,0); std::string data(n,'\0');
  EXPECT_EQ(api.FPDFImageObj_GetImageDataDecoded(object,data.data(),n),n); return data;
}
void InlineSentinel(const Api& api,FPDF_PAGE page) {
  EXPECT_EQ(api.FPDFPage_CountObjects(page),2); const auto image=Render(api,page,128,128);
  EXPECT_EQ(Pixel(image,104,24),(std::array<uint8_t,4>{255,0,0,255})); EXPECT_EQ(Pixel(image,8,120),kInlineWhite);
}

// Native ProcessImage returns immediately for BI images, including an inline
// image replaced through SetBitmap. Its regenerated stream therefore contains
// only our blue sentinel. Require that known observation for both readers of
// native's saved bytes, while retaining the correct Rust image assertions.
bool NativeInlineWasDropped(const Library& writer, const Api& api, FPDF_PAGE page) {
  if (writer.name() != "native") return false;
  EXPECT_EQ(api.FPDFPage_CountObjects(page), 1);
  const auto raster = Render(api, page, 128, 128);
  size_t wrong = 0;
  for (int y = 0; y < 128; ++y)
    for (int x = 0; x < 128; ++x) {
      const auto expected = x >= 96 && x < 112 && y >= 16 && y < 32
          ? std::array<uint8_t, 4>{255, 0, 0, 255} : kInlineWhite;
      wrong += Pixel(raster, x, y) != expected;
    }
  EXPECT_EQ(wrong, 0u) << "native regenerated BI output contains only the sentinel";
  return true;
}
void CompareOutsideInlineImage(const Raster& native, const Raster& rust,
                               Artifacts& artifacts, const std::string& label) {
  // Exclude only the union of the original and translated image rectangles;
  // no reference pixels are synthesized or accepted inside that rectangle.
  // The Rust output is asserted independently and compared with both readers.
  for (const auto& area : std::array<std::array<int, 4>, 4>{{
           {0, 0, 128, 64}, {0, 112, 128, 16}, {0, 64, 16, 48}, {80, 64, 48, 48}}})
    Exact(CropRaster(native, area[0], area[1], area[2], area[3]),
          CropRaster(rust, area[0], area[1], area[2], area[3]), artifacts, label);
}
void CompareOutsideResourceImage(const Raster& original, const Raster& current,
                                 Artifacts& artifacts, const std::string& label) {
  // Keep the same standard-font edge budget as the complete resource scene.
  // Only the existing glyph interior uses it; all other unaffected pixels are
  // strict, including the solid blue graphics state control.
  TextRegions(CropRaster(original, 0, 0, 128, 80),
              CropRaster(current, 0, 0, 128, 80), artifacts, label + "-header",
              {{12, 18, 22, 25}});
  for (const auto& area : std::array<std::array<int, 4>, 3>{{
           {0, 112, 128, 16}, {0, 80, 16, 32}, {48, 80, 80, 32}}})
    Exact(CropRaster(original, area[0], area[1], area[2], area[3]),
          CropRaster(current, area[0], area[1], area[2], area[3]), artifacts, label);
}

TEST(CoverageInlineImages, RawAbbreviationsNamedIndexedAndMaskSamplesBecomeEditableObjects) {
  struct Case {std::string dictionary,data,resources;std::array<uint8_t,4> expected;};
  const std::vector<Case> cases={
    {"/W 1 /H 1 /BPC 8 /CS /G",std::string(1,'\x40'),"",{64,64,64,255}},
    {"/W 1 /H 1 /BPC 8 /CS /RGB",std::string("\xff\x20\x00",3),"",{0,32,255,255}},
    // Stock Adobe CMYK's table/interpolation maps K=255 to RGB(35,31,32),
    // rather than the idealized subtractive black of a standard conversion.
    {"/W 1 /H 1 /BPC 8 /CS /CMYK",std::string("\x00\x00\x00\xff",4),"",{32,31,35,255}},
    {"/W 1 /H 1 /BPC 8 /CS /IndexedColors",std::string(1,'\x01'),"/ColorSpace << /IndexedColors [/Indexed /DeviceRGB 1 <FF000000FF00>] >>",{0,255,0,255}},
    {"/W 1 /H 1 /BPC 1 /CS [/I /RGB 1 <FF000000FF00>]",std::string(1,'\x80'),"",{0,255,0,255}},
    {"/W 1 /H 1 /IM true /D [1 0]",std::string(1,'\x80'),"",{0,255,0,255}},
    {"/W 1 /H 1 /IM true /D [0 1]",std::string(1,'\x80'),"",kInlineWhite}};
  Artifacts artifacts;
  for(size_t c=0;c<cases.size();++c){std::vector<Raster> scenes;for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());
    SCOPED_TRACE(c);
    const auto& api=writer->api();const auto bytes=InlinePdf(cases[c].dictionary,cases[c].data,cases[c].resources);
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    {Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);InlineSentinel(api,page.get());auto image=InlineObject(api,page.get());ASSERT_NE(image,nullptr);
      unsigned width=0,height=0;ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(image,&width,&height));EXPECT_EQ(width,1u);EXPECT_EQ(height,1u);
      EXPECT_EQ(InlineDecoded(api,image),cases[c].data);EXPECT_EQ(Pixel(Render(api,page.get(),128,128),40,88),cases[c].expected);
      ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image,48,0,0,48,32,16));ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));}
    const auto observed=RoundTrip(*writer,doc.get(),{{}},artifacts,"inline-colors-"+std::to_string(c),[&](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int){
      if (NativeInlineWasDropped(*writer, read, page)) return;
      InlineSentinel(read,page);auto image=InlineObject(read,page);editing::ExpectBounds(read,image,{32,16,80,64});
      EXPECT_EQ(Pixel(Render(read,page,128,128),56,88),cases[c].expected);EXPECT_EQ(Pixel(Render(read,page,128,128),24,88),kInlineWhite);
    });scenes.push_back(observed[0]);}
    CompareOutsideInlineImage(scenes[0],scenes[1],artifacts,"inline-colors-backends-"+std::to_string(c));}
}

TEST(CoverageInlineImages, FilterAliasesArraysAndParametersConsumeExactlyTheirEncodedPayload) {
  const std::string pixels("\xff\x00\x00\x00\xff\x00\x00\x00\xff\xff\xff\x00",12);
  struct Encoding{std::string filter,data;};
  const std::vector<Encoding> encodings={
    {"/F /AHx",InlineHex(pixels)}, {"/F /A85",Inline85(pixels)},
    {"/F /RL",std::string(1,'\x0b')+pixels+std::string(1,'\x80')},
    {"/F /Fl /DP << /Predictor 1 /Columns 2 /Colors 3 /BitsPerComponent 8 >>",InlineZlib(pixels)},
    {"/F /LZW /DP << /EarlyChange 0 >>",InlineLzw(pixels)},
    {"/F [/AHx /Fl] /DP [null << /Predictor 1 >>]",InlineHex(InlineZlib(pixels))}};
  Artifacts artifacts;
  for(size_t c=0;c<encodings.size();++c){std::vector<Raster> scenes;for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());
    SCOPED_TRACE(c);
    const auto& api=writer->api();const auto bytes=InlinePdf("/W 2 /H 2 /BPC 8 /CS /RGB "+encodings[c].filter,encodings[c].data);
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    {Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);InlineSentinel(api,page.get());auto image=InlineObject(api,page.get());ASSERT_NE(image,nullptr);
      EXPECT_EQ(InlineDecoded(api,image),pixels);EXPECT_GE(api.FPDFImageObj_GetImageFilterCount(image),1);
      ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image,48,0,0,48,32,16));ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));}
    const auto observed=RoundTrip(*writer,doc.get(),{{}},artifacts,"inline-filter-"+std::to_string(c),[&](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int){
      if (NativeInlineWasDropped(*writer, read, page)) return;
      InlineSentinel(read,page);EXPECT_EQ(InlineDecoded(read,InlineObject(read,page)),pixels);
      const auto image=Render(read,page,128,128);EXPECT_EQ(Pixel(image,40,72),(std::array<uint8_t,4>{0,0,255,255}));EXPECT_EQ(Pixel(image,72,72),(std::array<uint8_t,4>{0,255,0,255}));
      EXPECT_EQ(Pixel(image,40,104),(std::array<uint8_t,4>{255,0,0,255}));EXPECT_EQ(Pixel(image,72,104),(std::array<uint8_t,4>{0,255,255,255}));
    });scenes.push_back(observed[0]);}
    CompareOutsideInlineImage(scenes[0],scenes[1],artifacts,"inline-filter-backends-"+std::to_string(c));}
}

TEST(CoverageInlineImages, BinaryEiBytesAreSamplesRatherThanAnEarlyTerminatorAndBitmapReplacementWorks) {
  const std::string pixels(" EI \x00\xff\x7f\x20",8); const auto bytes=InlinePdf("/W 8 /H 1 /BPC 8 /CS /G",pixels);
  Artifacts artifacts;std::vector<Raster> scenes;
  for(const auto* writer:Implementations()) {const auto& api=writer->api();Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    {Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);InlineSentinel(api,page.get());auto image=InlineObject(api,page.get());ASSERT_NE(image,nullptr);EXPECT_EQ(InlineDecoded(api,image),pixels);
      const auto raster=Render(api,page.get(),128,128);EXPECT_EQ(Pixel(raster,19,88),(std::array<uint8_t,4>{32,32,32,255}));EXPECT_EQ(Pixel(raster,43,88),(std::array<uint8_t,4>{0,0,0,255}));
      Handle bitmap(api.FPDFBitmap_Create(2,2,1),api.FPDFBitmap_Destroy);ASSERT_NE(bitmap.get(),nullptr);ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,2,2,0xffe020a0));
      FPDF_PAGE pages[]={page.get()};ASSERT_TRUE(api.FPDFImageObj_SetBitmap(pages,1,image,bitmap.get()));ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));}
    const auto observed=RoundTrip(*writer,doc.get(),{{}},artifacts,"inline-EI-replacement",[&](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int){if (NativeInlineWasDropped(*writer, read, page)) return;InlineSentinel(read,page);unsigned w=0,h=0;ASSERT_TRUE(read.FPDFImageObj_GetImagePixelSize(InlineObject(read,page),&w,&h));EXPECT_EQ(w,2u);EXPECT_EQ(h,2u);EXPECT_EQ(Pixel(Render(read,page,128,128),40,88),(std::array<uint8_t,4>{160,32,224,255}));});scenes.push_back(observed[0]);}
  CompareOutsideInlineImage(scenes[0],scenes[1],artifacts,"inline-EI-backends");
}

TEST(CoverageInlineImages, InlineDctAndFaxBoundariesMatchAnIndependentXobjectControl) {
  struct Case{const char* file;int width,height;std::string inline_dict,image_dict;bool jpeg;};
  const std::vector<Case> cases={
    {"crates/pdfoxium-core/tests/fixtures/gray.jpg",16,1,"/BPC 8 /CS /G /F /DCT","/BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /DCTDecode",true},
    {"upstream/pdfium/testing/resources/mona_lisa.jpg",120,120,"/BPC 8 /CS /RGB /F /DCT","/BitsPerComponent 8 /ColorSpace /DeviceRGB /Filter /DCTDecode",true},
    {"crates/pdfoxium-core/tests/fixtures/cmyk.jpg",16,1,"/BPC 8 /CS /CMYK /F /DCT","/BitsPerComponent 8 /ColorSpace /DeviceCMYK /Filter /DCTDecode",true},
    {"crates/pdfoxium-core/tests/fixtures/fax_g3.bin",16,2,"/IM true /F /CCF /DP << /K 0 /Columns 16 /Rows 2 /EndOfBlock false /BlackIs1 true >>","/ImageMask true /BitsPerComponent 1 /Filter /CCITTFaxDecode /DecodeParms << /K 0 /Columns 16 /Rows 2 /EndOfBlock false /BlackIs1 true >>",false},
    {"crates/pdfoxium-core/tests/fixtures/fax_g4.bin",16,2,"/IM true /F /CCF /DP << /K -1 /Columns 16 /Rows 2 /EndOfBlock false /BlackIs1 true >>","/ImageMask true /BitsPerComponent 1 /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 16 /Rows 2 /EndOfBlock false /BlackIs1 true >>",false}};
  Artifacts artifacts;
  for(size_t c=0;c<cases.size();++c) {const auto& item=cases[c];const auto payload=InlineRead(TestRoot()/item.file);ASSERT_FALSE(payload.empty());
    const std::string dimensions="/Width "+std::to_string(item.width)+" /Height "+std::to_string(item.height);
    const auto bytes=InlinePdf("/W "+std::to_string(item.width)+" /H "+std::to_string(item.height)+" "+item.inline_dict,payload);
    const auto control=editing::SceneFixture("q 48 0 0 48 16 16 cm 0 1 0 rg /I Do Q 0 0 1 rg 96 96 16 16 re f","/XObject << /I 5 0 R >>",{PdfStream("/Type /XObject /Subtype /Image "+dimensions+" "+item.image_dict,payload)});
    std::vector<Raster> scenes;const auto compare=[&](const Raster&a,const Raster&b,Artifacts&out,const std::string&label){CompareRaster(a,b,out,label,item.jpeg?2:1);};
    for(const auto* writer:Implementations()) {SCOPED_TRACE(writer->name());
      SCOPED_TRACE(c);
      const auto& api=writer->api();Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
      Handle control_doc(api.FPDF_LoadMemDocument64(control.data(),control.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(control_doc.get(),nullptr);
      {Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);Handle expected(api.FPDF_LoadPage(control_doc.get(),0),api.FPDF_ClosePage);InlineSentinel(api,page.get());
        compare(Render(api,expected.get(),128,128),Render(api,page.get(),128,128),artifacts,writer->name()+"-inline-control-"+std::to_string(c));
        auto image=InlineObject(api,page.get());unsigned w=0,h=0;ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(image,&w,&h));EXPECT_EQ(w,static_cast<unsigned>(item.width));EXPECT_EQ(h,static_cast<unsigned>(item.height));
        if(!item.jpeg) {
          // The first decoded row is the upper image row in raster space.
          const auto picture=Render(api,page.get(),128,128);
          EXPECT_EQ(Pixel(picture,19,72),(std::array<uint8_t,4>{0,255,0,255}));
          EXPECT_EQ(Pixel(picture,61,72),kInlineWhite);
          EXPECT_EQ(Pixel(picture,19,104),kInlineWhite);
          EXPECT_EQ(Pixel(picture,61,104),(std::array<uint8_t,4>{0,255,0,255}));
        }
        ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image,48,0,0,48,32,16));ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));}
      const auto pictures=RoundTrip(*writer,doc.get(),{{}},artifacts,"inline-codec-"+std::to_string(c),[&](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int){if (NativeInlineWasDropped(*writer, read, page)) return;InlineSentinel(read,page);auto image=InlineObject(read,page);editing::ExpectBounds(read,image,{32,16,80,64});unsigned w=0,h=0;ASSERT_TRUE(read.FPDFImageObj_GetImagePixelSize(image,&w,&h));EXPECT_EQ(w,static_cast<unsigned>(item.width));EXPECT_EQ(h,static_cast<unsigned>(item.height));},compare);scenes.push_back(pictures[0]);
    }
    CompareOutsideInlineImage(scenes[0],scenes[1],artifacts,"inline-codec-backends-"+std::to_string(c));
  }
}

std::vector<uint8_t> SharedResourceFixture(bool inherited) {
  const std::string resources=inherited ? "" : " /Resources 5 0 R";
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2"+std::string(inherited?" /Resources 5 0 R":"")+" >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Contents 12 0 R"+resources+" >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Contents 13 0 R"+resources+" >>",
    "<< /XObject 6 0 R /ExtGState 7 0 R /Font 8 0 R >>","<< /FXI1 9 0 R >>","<< /FXE1 10 0 R >>","<< /FXF1 11 0 R >>",
    PdfStream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceRGB",std::string("\xff\x00\x00",3)),
    "<< /Type /ExtGState /ca .5 /CA .5 >>","<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    PdfStream("","q 32 0 0 32 16 16 cm /FXI1 Do Q q /FXE1 gs 0 0 1 rg 64 16 32 32 re f Q BT /FXF1 14 Tf 1 0 0 1 16 90 Tm (A) Tj ET"),
    PdfStream("","q 32 0 0 32 16 16 cm /FXI1 Do Q q /FXE1 gs 0 0 1 rg 64 16 32 32 re f Q BT /FXF1 14 Tf 1 0 0 1 16 90 Tm (A) Tj ET")});
}

TEST(CoverageInlineImages, SharedResourceCategoriesSurvivePruningRestorationAndConflictingNewNames) {
  Artifacts artifacts;const auto compare=[](const Raster&a,const Raster&b,Artifacts&out,const std::string&label){TextRegions(a,b,out,label,{{12,18,22,25}});};
  for(bool inherited:{false,true}) {std::vector<std::vector<Raster>> scenes;
    for(const auto* writer:Implementations()) {SCOPED_TRACE(writer->name());
      SCOPED_TRACE(inherited);
      const bool native_shared_bug = inherited && writer->name() == "native";
      const auto& api=writer->api();const auto bytes=SharedResourceFixture(inherited);Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
      Raster untouched;
      {Handle other(api.FPDF_LoadPage(doc.get(),1),api.FPDF_ClosePage);untouched=Render(api,other.get(),128,128);}
      {Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_EQ(api.FPDFPage_CountObjects(page.get()),3);auto image=InlineObject(api,page.get());FPDF_PAGEOBJECT text=nullptr;
        for(int i=0;i<3;++i){auto obj=api.FPDFPage_GetObject(page.get(),i);if(api.FPDFPageObj_GetType(obj)==FPDF_PAGEOBJ_TEXT)text=obj;}ASSERT_NE(text,nullptr);
        ASSERT_TRUE(api.FPDFPageObj_SetIsActive(image,0));ASSERT_TRUE(api.FPDFPageObj_SetIsActive(text,0));ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
        const auto inactive=Render(api,page.get(),128,128);EXPECT_EQ(Pixel(inactive,32,96),kInlineWhite);EXPECT_EQ(Pixel(inactive,21,32),kInlineWhite);
        {Handle other(api.FPDF_LoadPage(doc.get(),1),api.FPDF_ClosePage);
          const auto after = Render(api,other.get(),128,128);
          if (native_shared_bug) {
            EXPECT_EQ(api.FPDFPage_CountObjects(other.get()),2);
            EXPECT_EQ(Pixel(after,32,96),kInlineWhite);
            CompareOutsideResourceImage(untouched,after,artifacts,"native-inherited-resource-prune");
          } else Exact(untouched,after,artifacts,writer->name()+"-resource-prune-other-page");}
        ASSERT_TRUE(api.FPDFPageObj_SetIsActive(image,1));ASSERT_TRUE(api.FPDFPageObj_SetIsActive(text,1));ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
        compare(untouched,Render(api,page.get(),128,128),artifacts,writer->name()+"-resource-restore");
        Handle bitmap(api.FPDFBitmap_Create(1,1,1),api.FPDFBitmap_Destroy);ASSERT_NE(bitmap.get(),nullptr);ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,1,1,0xff00ff00));
        Handle added(api.FPDFPageObj_NewImageObj(doc.get()),api.FPDFPageObj_Destroy);ASSERT_NE(added.get(),nullptr);FPDF_PAGE pages[]={page.get()};ASSERT_TRUE(api.FPDFImageObj_SetBitmap(pages,1,added.get(),bitmap.get()));ASSERT_TRUE(api.FPDFImageObj_SetMatrix(added.get(),24,0,0,24,96,64));ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),added.release()));
        ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));}
      const auto pictures=RoundTrip(*writer,doc.get(),{{},{}},artifacts,"shared-resources-"+std::to_string(inherited),[&](const Api& read,FPDF_DOCUMENT,FPDF_PAGE page,int index){
        const bool lost = index && native_shared_bug;
        EXPECT_EQ(read.FPDFPage_CountObjects(page),index?(lost?2:3):4);const auto image=Render(read,page,128,128);EXPECT_EQ(Pixel(image,32,96),lost?kInlineWhite:(std::array<uint8_t,4>{0,0,255,255}));EXPECT_EQ(Pixel(image,80,96),(std::array<uint8_t,4>{255,128,128,255}));EXPECT_EQ(Pixel(image,108,52),index?kInlineWhite:(std::array<uint8_t,4>{0,255,0,255}));Ink(image,14,20,20,22);
      },compare);
      if (native_shared_bug) CompareOutsideResourceImage(untouched,pictures[1],artifacts,"native-inherited-resource-persisted");
      else Exact(untouched,pictures[1],artifacts,writer->name()+"-resource-other-persisted");
      scenes.push_back(pictures);
    }
    for(size_t p=0;p<2;++p) {
      const auto label="shared-resource-backends-"+std::to_string(inherited)+std::to_string(p);
      if (inherited && p==1) CompareOutsideResourceImage(scenes[0][p],scenes[1][p],artifacts,label);
      else compare(scenes[0][p],scenes[1][p],artifacts,label);
    }
  }
}

TEST(CoverageInlineImages, ImportDropsDanglingOptionalReferencesAndRemapsSelfDestinationsAndPageTree) {
  // Optional dictionary values and arrays contain missing objects, while the
  // actual page paint, link, popup and appearance form are all complete.
  const auto source_bytes=PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << >> /Annots [4 0 R] /Contents 6 0 R >>",
    "<< /Type /Annot /Subtype /Link /Rect [24 24 64 64] /Border [0 0 0] /F 4 /Popup 5 0 R /Tree 2 0 R /Bad 99 0 R /BrokenArray [100 0 R] /A << /S /GoTo /D [3 0 R /XYZ 12 34 0] >> /AP << /N 7 0 R >> >>",
    "<< /Type /Annot /Subtype /Popup /Parent 4 0 R /Rect [72 72 112 112] /Contents (retained popup) /F 2 >>",
    PdfStream("","0 0 1 rg 8 8 8 8 re f"),
    PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 40 40] /Resources << /Unused 101 0 R >>","0 1 0 rg 0 0 40 40 re f")});
  Artifacts artifacts;std::vector<std::vector<Raster>> scenes;
  for(const auto* writer:Implementations()) {SCOPED_TRACE(writer->name());const auto& api=writer->api();
    Handle source(api.FPDF_LoadMemDocument64(source_bytes.data(),source_bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(source.get(),nullptr);
    Handle target(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);ASSERT_NE(target.get(),nullptr);
    for(int index=0;index<2;++index) {Handle page(api.FPDFPage_New(target.get(),index,128,128),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),editing::Rectangle(api,8,8,16,16,index?128:255,128,128)));ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));}
    const int selected=0;ASSERT_TRUE(api.FPDF_ImportPagesByIndex(target.get(),source.get(),&selected,1,1));
    // Native skips Popup.Parent during remapping and keeps a raw pointer to the
    // source holder. An isolated debugger confirmed its post-close UAF here.
    // Keep native's source alive; Rust must safely own the imported relation.
    if (writer->name() != "native") source.reset();
    {Handle imported(api.FPDF_LoadPage(target.get(),1),api.FPDF_ClosePage);ASSERT_EQ(api.FPDFPage_CountObjects(imported.get()),1);ASSERT_TRUE(api.FPDFPageObj_SetFillColor(api.FPDFPage_GetObject(imported.get(),0),255,0,0,255));ASSERT_TRUE(api.FPDFPage_GenerateContent(imported.get()));}
    const auto pictures=RoundTrip(*writer,target.get(),{{128,128,FPDF_ANNOT},{128,128,FPDF_ANNOT},{128,128,FPDF_ANNOT}},artifacts,"import-optional-references",[&](const Api& read,FPDF_DOCUMENT doc,FPDF_PAGE page,int index){
      EXPECT_EQ(read.FPDFPage_CountObjects(page),1);const auto image=Render(read,page,128,128,FPDF_ANNOT);
      if(index!=1) {EXPECT_EQ(read.FPDFPage_GetAnnotCount(page),0);EXPECT_EQ(Pixel(image,16,112),(std::array<uint8_t,4>{128,128,static_cast<uint8_t>(index?128:255),255}));return;}
      ASSERT_EQ(read.FPDFPage_GetAnnotCount(page),1);Handle annot(read.FPDFPage_GetAnnot(page,0),read.FPDFPage_CloseAnnot);ASSERT_NE(annot.get(),nullptr);
      EXPECT_FALSE(read.FPDFAnnot_HasKey(annot.get(),"Bad"));EXPECT_FALSE(read.FPDFAnnot_HasKey(annot.get(),"BrokenArray"));EXPECT_EQ(read.FPDFAnnot_GetValueType(annot.get(),"Tree"),FPDF_OBJECT_REFERENCE);
      Handle popup(read.FPDFAnnot_GetLinkedAnnot(annot.get(),"Popup"),read.FPDFPage_CloseAnnot);ASSERT_NE(popup.get(),nullptr);EXPECT_EQ(read.FPDFAnnot_GetSubtype(popup.get()),FPDF_ANNOT_POPUP);
      if (writer->name() != "native") {
        Handle parent(read.FPDFAnnot_GetLinkedAnnot(popup.get(),"Parent"),read.FPDFPage_CloseAnnot);
        ASSERT_NE(parent.get(),nullptr);EXPECT_EQ(read.FPDFAnnot_GetSubtype(parent.get()),FPDF_ANNOT_LINK);
        EXPECT_EQ(read.FPDFAnnot_GetValueType(parent.get(),"Popup"),FPDF_OBJECT_REFERENCE);
      }
      auto link=read.FPDFAnnot_GetLink(annot.get());ASSERT_NE(link,nullptr);auto dest=read.FPDFLink_GetDest(doc,link);ASSERT_NE(dest,nullptr);EXPECT_EQ(read.FPDFDest_GetDestPageIndex(doc,dest),1);
      FPDF_BOOL has_x=0,has_y=0,has_zoom=0;float x=0,y=0,zoom=0;ASSERT_TRUE(read.FPDFDest_GetLocationInPage(dest,&has_x,&has_y,&has_zoom,&x,&y,&zoom));EXPECT_TRUE(has_x);EXPECT_TRUE(has_y);EXPECT_EQ(x,12);EXPECT_EQ(y,34);
      EXPECT_EQ(Pixel(image,12,116),(std::array<uint8_t,4>{0,0,255,255}));EXPECT_EQ(Pixel(image,40,88),(std::array<uint8_t,4>{0,255,0,255}));EXPECT_EQ(Pixel(image,100,28),kInlineWhite);
    });scenes.push_back(pictures);
  }
  for(size_t p=0;p<3;++p)Exact(scenes[0][p],scenes[1][p],artifacts,"import-optional-backends-"+std::to_string(p));
}

}  // namespace
}  // namespace pdfoxium::integration
