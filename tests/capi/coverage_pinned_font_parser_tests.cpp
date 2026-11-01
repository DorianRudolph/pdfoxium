#include "pinned_fixture_support.h"
#include "pinned_test_fonts.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
constexpr char16_t kHelloGoodbye[]=u"Hello, world!\r\nGoodbye, world!";
constexpr char16_t kHelloWorld[]=u"Hello, world!";
using Inspect=std::function<void(const Api&,FPDF_PAGE,FPDF_TEXTPAGE)>;
using InspectRaster=std::function<void(const Raster&,Artifacts&,const std::string&,const Library&)>;
using ComparePixels=std::function<void(const Raster&,const Raster&,Artifacts&,const std::string&)>;

// Expected text and geometry below come from the pinned embedder regressions,
// not from either renderer. These original resource graphs complement synthetic
// cmap fixtures. Saving without GenerateContent preserves imported structures.
void ExactText(const Api& api,FPDF_TEXTPAGE text,const std::u16string& expected,
    int requested=128,int start=0) {
  std::vector<unsigned short> buffer(static_cast<size_t>(requested)+2,0xbdbd);
  ASSERT_EQ(api.FPDFText_GetText(text,start,requested,buffer.data()),expected.size()+1);
  for(size_t i=0;i<expected.size();++i)EXPECT_EQ(buffer[i],expected[i])<<i;
  EXPECT_EQ(buffer[expected.size()],0);
  EXPECT_EQ(buffer[requested+1],0xbdbd);
}
void UnicodeRange(const Api& api,FPDF_TEXTPAGE text,int start,
    const std::vector<unsigned int>& expected,bool get_text=true) {
  ASSERT_GE(api.FPDFText_CountChars(text),start+static_cast<int>(expected.size()));
  std::u16string units;
  for(size_t i=0;i<expected.size();++i) {
    EXPECT_EQ(api.FPDFText_GetUnicode(text,start+i),expected[i])<<i;
    units.push_back(static_cast<char16_t>(expected[i]));
  }
  if(get_text)ExactText(api,text,units,expected.size(),start);
}
struct Observations {int count=0;float width=0,height=0;std::vector<unsigned int> unicode;std::u16string text;};
Observations Observe(const Api& api,FPDF_PAGE page,FPDF_TEXTPAGE text) {
  Observations result;result.count=api.FPDFText_CountChars(text);
  result.width=api.FPDF_GetPageWidthF(page);result.height=api.FPDF_GetPageHeightF(page);
  for(int i=0;i<result.count;++i)result.unicode.push_back(api.FPDFText_GetUnicode(text,i));
  result.text=text_fixture::Extract(api,text);return result;
}
void SameObservations(const Observations& expected,const Observations& actual) {
  EXPECT_EQ(actual.count,expected.count);EXPECT_EQ(actual.unicode,expected.unicode);EXPECT_EQ(actual.text,expected.text);
  EXPECT_FLOAT_EQ(actual.width,expected.width);EXPECT_FLOAT_EQ(actual.height,expected.height);
}
void FixtureWorkflow(const std::string& file,int width,int height,const Inspect& inspect,
    const InspectRaster& inspect_raster={},const ComparePixels& compare_pixels={}) {
  const auto bytes=text_fixture::Resource(file);ASSERT_FALSE(bytes.empty());
  Artifacts artifacts;std::array<Raster,2> images;std::array<Observations,2> observations;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE("writer="+writer->name());const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);ASSERT_EQ(api.FPDF_GetPageCount(document.get()),1);
    Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
    ASSERT_NO_FATAL_FAILURE(inspect(api,page.get(),text.get()));
    observations[backend]=Observe(api,page.get(),text.get());
    if(width>0){EXPECT_FLOAT_EQ(observations[backend].width,width);}
    if(height>0){EXPECT_FLOAT_EQ(observations[backend].height,height);}
    ASSERT_TRUE(std::isfinite(observations[backend].width));ASSERT_TRUE(std::isfinite(observations[backend].height));
    ASSERT_GT(observations[backend].width,0);ASSERT_GT(observations[backend].height,0);
    const int render_width=static_cast<int>(std::ceil(observations[backend].width));
    const int render_height=static_cast<int>(std::ceil(observations[backend].height));
    ASSERT_LE(render_width,1000);ASSERT_LE(render_height,1000);
    images[backend]=Render(api,page.get(),render_width,render_height);
    ASSERT_EQ(images[backend].pixels.size(),static_cast<size_t>(render_width*render_height*4));
    if(inspect_raster){ASSERT_NO_FATAL_FAILURE(inspect_raster(images[backend],artifacts,file+writer->name(),*writer));}
    const auto saved=SaveDocument(api,document.get(),artifacts,file+writer->name());ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE("reader="+reader->name());const auto& read=reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(),nullptr);ASSERT_EQ(read.FPDF_GetPageCount(reopened.get()),1);
      Handle after(read.FPDF_LoadPage(reopened.get(),0),read.FPDF_ClosePage);ASSERT_NE(after.get(),nullptr);
      Handle extracted(read.FPDFText_LoadPage(after.get()),read.FPDFText_ClosePage);ASSERT_NE(extracted.get(),nullptr);
      ASSERT_NO_FATAL_FAILURE(inspect(read,after.get(),extracted.get()));
      SameObservations(observations[backend],Observe(read,after.get(),extracted.get()));
      const auto image=Render(read,after.get(),render_width,render_height);
      const auto label=file+writer->name()+reader->name();
      if(inspect_raster){ASSERT_NO_FATAL_FAILURE(inspect_raster(image,artifacts,label,*reader));}
      if(writer==reader)CompareRaster(images[backend],image,artifacts,label);
      else if(compare_pixels)compare_pixels(images[backend],image,artifacts,label);
      else text_fixture::CompareTextRaster(images[backend],image,artifacts,label);
    }
    ++backend;
  }
  SameObservations(observations[0],observations[1]);
  if(compare_pixels)compare_pixels(images[0],images[1],artifacts,file+"-engines");
  else text_fixture::CompareTextRaster(images[0],images[1],artifacts,file+"-engines");
}

void CompareCoverageStructure(const Raster& expected,const Raster& actual) {
  ASSERT_EQ(expected.width,actual.width);ASSERT_EQ(expected.height,actual.height);
  ASSERT_EQ(expected.pixels.size(),actual.pixels.size());
  std::array<std::array<int,4>,2> bounds{{{expected.width,expected.height,-1,-1},{actual.width,actual.height,-1,-1}}};
  std::array<uint64_t,2> mass{};const std::array<const Raster*,2> images{{&expected,&actual}};
  for(size_t owner=0;owner<images.size();++owner) {
    const auto& image=*images[owner];
    for(int y=0;y<image.height;++y)for(int x=0;x<image.width;++x) {
      const auto p=(static_cast<size_t>(y)*image.width+x)*4;const auto gray=image.pixels[p];
      ASSERT_EQ(image.pixels[p+1],gray);ASSERT_EQ(image.pixels[p+2],gray);ASSERT_EQ(image.pixels[p+3],255);
      mass[owner]+=255-gray;
      if(gray!=255){auto& box=bounds[owner];box[0]=std::min(box[0],x);box[1]=std::min(box[1],y);box[2]=std::max(box[2],x);box[3]=std::max(box[3],y);}
    }
  }
  EXPECT_EQ(bounds[0],bounds[1]);EXPECT_GT(mass[0],1000u);EXPECT_GT(mass[1],1000u);
  EXPECT_LE(mass[0]>mass[1]?mass[0]-mass[1]:mass[1]-mass[0],std::max(mass[0],mass[1])/100);
  for(size_t p=0;p<expected.pixels.size();p+=4) {
    const int a=expected.pixels[p],b=actual.pixels[p];
    // A changed pixel must touch an intermediate antialias coverage value;
    // solid white/black pixels cannot become their opposite.
    if((a==0||a==255)&&(b==0||b==255)){EXPECT_EQ(a,b);}
  }
}
void CompareSmallCidCoverage(const Raster& expected,const Raster& actual,Artifacts& artifacts,const std::string& label) {
  ASSERT_NO_FATAL_FAILURE(CompareCoverageStructure(expected,actual));
  // This 12pt embedded TimesNewRomanBold fixture has identical placement and
  // ink bounds, with 0.11% observed ink-mass difference, but FreeType and the
  // Rust AGG outline rasterizer differ by up to52 at seven edge pixels.
  // Keep the bound local and retain exact extraction/enclosure and reloads.
  text_fixture::CompareTextRaster(expected,actual,artifacts,label,{56,0,3.5});
}
void CompareHelloWorldCoverage(const Raster& expected,const Raster& actual,Artifacts& artifacts,const std::string& label) {
  ASSERT_NO_FATAL_FAILURE(CompareCoverageStructure(expected,actual));
  // With pinned Tinos/Arimo, native matches the committed PNG exactly. Rust's
  // AGG glyph edges differ by up to38, union mean0.677, and ink mass0.941%.
  // Keep native's golden exact and this bound local to the Rust reader.
  text_fixture::CompareTextRaster(expected,actual,artifacts,label,{40,0,.7});
}

TEST(CoveragePinnedFontParser, OriginalFontMatricesAndTightLooseBoxesSurviveBothWriters) {
  ScopedPinnedTestFonts fonts;
  // fpdf_text_embeddertest.cpp: GetMatrix and CharBox, complete matrix table.
  FixtureWorkflow("font_matrix.pdf",100,200,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    ASSERT_EQ(api.FPDFText_CountChars(text),10);ExactText(api,text,u"A1\r\nA2\r\nA3",10);
    const std::array<FS_MATRIX,10> matrices{{
      {12,0,0,10,66,90},{12,0,0,10,66,90},{1,0,0,1,0,0},{1,0,0,1,0,0},
      {12,0,0,10,38,60},{12,0,0,10,38,60},{1,0,0,1,0,0},{1,0,0,1,0,0},
      {1,0,0,.833333f,60,130},{1,0,0,.833333f,60,130}}};
    for(size_t i=0;i<matrices.size();++i) {
      SCOPED_TRACE(i);FS_MATRIX actual{};ASSERT_TRUE(api.FPDFText_GetMatrix(text,i,&actual));const auto& expected=matrices[i];
      EXPECT_FLOAT_EQ(actual.a,expected.a);EXPECT_FLOAT_EQ(actual.b,expected.b);EXPECT_FLOAT_EQ(actual.c,expected.c);
      EXPECT_FLOAT_EQ(actual.d,expected.d);EXPECT_FLOAT_EQ(actual.e,expected.e);EXPECT_FLOAT_EQ(actual.f,expected.f);
    }
    for(int index:{0,4,8}) {
      SCOPED_TRACE(index);EXPECT_EQ(api.FPDFText_GetUnicode(text,index),'A');
      double left=0,right=0,bottom=0,top=0;ASSERT_TRUE(api.FPDFText_GetCharBox(text,index,&left,&right,&bottom,&top));
      EXPECT_NEAR(right-left,8.460,.001);EXPECT_NEAR(top-bottom,6.600,.001);
      FS_RECTF loose{};ASSERT_TRUE(api.FPDFText_GetLooseCharBox(text,index,&loose));
      EXPECT_NEAR(loose.right-loose.left,8.664,.001);EXPECT_NEAR(loose.top-loose.bottom,11.060005,.001);
    }
    FS_MATRIX unused{};EXPECT_FALSE(api.FPDFText_GetMatrix(nullptr,0,&unused));
    EXPECT_FALSE(api.FPDFText_GetMatrix(text,10,&unused));EXPECT_FALSE(api.FPDFText_GetMatrix(text,-1,&unused));
    EXPECT_FALSE(api.FPDFText_GetMatrix(text,0,nullptr));
  });
  fonts.ExpectMapped();
}

TEST(CoveragePinnedFontParser, StemVEstimatedAndExplicitFontWeightsAgreeWithDescriptorContracts) {
  FixtureWorkflow("font_weight.pdf",200,200,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    ASSERT_EQ(api.FPDFText_CountChars(text),2);
    EXPECT_EQ(api.FPDFText_GetFontWeight(text,0),400);  // StemV80 * 5.
    EXPECT_EQ(api.FPDFText_GetFontWeight(text,1),400);  // Explicit400, not StemV82 * 5.
    EXPECT_EQ(api.FPDFText_GetFontWeight(nullptr,0),-1);EXPECT_EQ(api.FPDFText_GetFontWeight(text,-1),-1);
    EXPECT_EQ(api.FPDFText_GetFontWeight(text,314),-1);
  });
}

TEST(CoveragePinnedFontParser, InvalidType3UnicodeRemainsAnExplicitZeroCharacter) {
  FixtureWorkflow("bug_583.pdf",48,48,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    ASSERT_EQ(api.FPDFText_CountChars(text),1);EXPECT_EQ(api.FPDFText_GetUnicode(text,0),0u);
  });
}

TEST(CoveragePinnedFontParser, OriginalCyrillicCidMappingRetainsTheCompleteExpectedRange) {
  FixtureWorkflow("bug_921.pdf",612,792,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    ASSERT_EQ(api.FPDFText_CountChars(text),268);
    UnicodeRange(api,text,238,{1095,1077,1083,1086,1074,1077,1095,1077,1089,1082,1086,1077,
      32,1089,1090,1088,1072,1076,1072,1085,1080,1077,46,32});
  });
}

TEST(CoveragePinnedFontParser, NonAsciiHyphenAndSplitFiLigatureKeepAllFiftySixCodepoints) {
  // The 612x792 MediaBox lives in this fixture's compressed ObjStm.
  FixtureWorkflow("bug_1029.pdf",612,792,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    ASSERT_GT(api.FPDFText_CountChars(text),171+56);
    UnicodeRange(api,text,171,{
      0x004d,0x0045,0x0054,0x0041,0x0044,0x0041,0x0054,0x0041,
      0x0020,0x0074,0x0061,0x0062,0x006c,0x0065,0x002e,0x0020,
      0x0057,0x0068,0x0065,0x006e,0x0020,0x0074,0x0068,0x0065,
      0x0020,0x0073,0x0070,0x006c,0x0069,0x0074,0x0020,0x0068,
      0x0061,0x0073,0x0020,0x0063,0x006f,0x006d,0x006d,0x0069,
      0x0074,0x0074,0x0065,0x0064,0x002c,0x0020,0x0069,0x0074,
      0x0020,0x006e,0x006f,0x0074,0x0069,0x0002,0x0066,0x0069},false);
  });
}

TEST(CoveragePinnedFontParser, SmallType3GlyphsAndGeneratedSpacesKeepAllFiveExactBoxes) {
  FixtureWorkflow("bug_1591.pdf",160,400,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    ASSERT_EQ(api.FPDFText_CountChars(text),5);UnicodeRange(api,text,0,{'1',' ','2',' ','1'});
    // Source tables are {left,top,right,bottom}, including zero-size spaces.
    const std::array<std::array<double,4>,5> boxes{{
      {63.439998626708984,61.520000457763672,65.360000610351562,50},
      {62.007999420166016,50,62.007999420166016,50},
      {86,50.240001678466797,88.400001525878906,50},
      {86.010002136230469,50,86.010002136230469,50},
      {99.44000244140625,61.520000457763672,101.36000061035156,50}}};
    for(size_t i=0;i<boxes.size();++i) {
      SCOPED_TRACE(i);double left=0,right=0,bottom=0,top=0;
      ASSERT_TRUE(api.FPDFText_GetCharBox(text,i,&left,&right,&bottom,&top));
      EXPECT_DOUBLE_EQ(left,boxes[i][0]);EXPECT_DOUBLE_EQ(top,boxes[i][1]);
      EXPECT_DOUBLE_EQ(right,boxes[i][2]);EXPECT_DOUBLE_EQ(bottom,boxes[i][3]);
    }
  });
}

TEST(CoveragePinnedFontParser, IdentityHCidLooseMetricsEncloseEveryTightBox) {
  FixtureWorkflow("bug_402562387.pdf",200,200,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    ASSERT_EQ(api.FPDFText_CountChars(text),4);
    for(int i=0;i<4;++i) {
      SCOPED_TRACE(i);double left=0,right=0,bottom=0,top=0;
      ASSERT_TRUE(api.FPDFText_GetCharBox(text,i,&left,&right,&bottom,&top));
      FS_RECTF loose{};ASSERT_TRUE(api.FPDFText_GetLooseCharBox(text,i,&loose));
      EXPECT_LE(loose.left,left);EXPECT_GE(loose.right,right);EXPECT_LE(loose.bottom,bottom);EXPECT_GE(loose.top,top);
      EXPECT_GE(loose.right-loose.left,right-left);EXPECT_GE(loose.top-loose.bottom,top-bottom);
    }
  },{},CompareSmallCidCoverage);
}

TEST(CoveragePinnedFontParser, ZeroWidthCidGlyphDoesNotCorruptAdjacentHelloWorldText) {
  FixtureWorkflow("bug_491516663.pdf",200,200,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    ExactText(api,text,kHelloWorld);
  });
}

TEST(CoveragePinnedFontParser, LaterToUnicodeRangesOverrideCatchAllEntries) {
  FixtureWorkflow("bug_554790604.pdf",300,120,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    const std::u16string expected={0x00f3,0x00ba,0x20ac,0x00ed,0x00ba,'X','A','B',0x00e9,0x00ea};
    ExactText(api,text,expected);
  });
}

TEST(CoveragePinnedFontParser, Type3NontrivialPositioningRetainsAbcdExtraction) {
  FixtureWorkflow("bug_642.pdf",200,200,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    ASSERT_EQ(api.FPDFText_CountChars(text),4);ExactText(api,text,u"ABCD",4);
  });
}

TEST(CoveragePinnedFontParser, LeadingControlAffectsInternalCountWithoutEnteringExtractedOutput) {
  FixtureWorkflow("bug_1139.pdf",200,200,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    EXPECT_EQ(api.FPDFText_CountChars(text),std::size(kHelloGoodbye));
    ExactText(api,text,kHelloGoodbye);
  });
}

TEST(CoveragePinnedFontParser, StreamLengthPastEofRecoversTheOriginalThirteenCharacters) {
  FixtureWorkflow("bug_57.pdf",200,200,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    ASSERT_EQ(api.FPDFText_CountChars(text),13);ExactText(api,text,kHelloWorld);
  });
}

TEST(CoveragePinnedFontParser, MismatchedWellFormedXrefEntriesRecoverRealNonblankPageText) {
  // Two incremental page dictionaries have different MediaBoxes. The pinned
  // regression requires loading and real text, not a guessed repair choice.
  FixtureWorkflow("bug_602650.pdf",0,0,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    ASSERT_GT(api.FPDFText_CountChars(text),0);ExactText(api,text,kHelloGoodbye);
  });
}

TEST(CoveragePinnedFontParser, HighObjectNumbersPreserveHelloWorldPageContent) {
  ScopedPinnedTestFonts fonts;
  const auto golden=LoadPinnedGolden("embedder_tests/hello_world.png");
  ASSERT_EQ(golden.width,200);ASSERT_EQ(golden.height,200);
  FixtureWorkflow("bug_455199.pdf",200,200,[](const Api& api,FPDF_PAGE,FPDF_TEXTPAGE text){
    ExactText(api,text,kHelloGoodbye);
  },[&](const Raster& image,Artifacts& artifacts,const std::string& label,const Library& reader){
    if(reader.name()=="native")CompareRaster(golden,image,artifacts,label+"-pinned-hello-world");
    else CompareHelloWorldCoverage(golden,image,artifacts,label+"-pinned-hello-world");
  },CompareHelloWorldCoverage);
  fonts.ExpectMapped();
}
}  // namespace
}  // namespace pdfoxium::integration
