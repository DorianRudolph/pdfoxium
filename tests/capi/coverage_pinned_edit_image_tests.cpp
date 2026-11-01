#include "test_support.h"
#include "pinned_fixture_support.h"
#include "pinned_test_fonts.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <fstream>
#include <functional>
#include <iterator>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
// Contracts from pinned fpdf_edit/fpdf_view/fpdf_render_pattern embedder tests.
// The committed upstream PNG is an independent oracle, never learned from Rust.
std::vector<uint8_t> Pdf(const std::string& name) {
  std::ifstream input(TestRoot()/"upstream/pdfium/testing/resources"/(name+".pdf"),std::ios::binary);
  EXPECT_TRUE(input.good())<<name;return {std::istreambuf_iterator<char>(input),std::istreambuf_iterator<char>()};
}
Raster Bitmap(const Api& api,FPDF_BITMAP bitmap) {
  if(!bitmap){ADD_FAILURE()<<"Missing extracted bitmap";return {};}
  const int width=api.FPDFBitmap_GetWidth(bitmap),height=api.FPDFBitmap_GetHeight(bitmap);
  const int format=api.FPDFBitmap_GetFormat(bitmap),stride=api.FPDFBitmap_GetStride(bitmap);
  const auto* buffer=static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap));
  const int bytes=format==FPDFBitmap_Gray?1:format==FPDFBitmap_BGR?3:4;
  if(width<=0||height<=0||!buffer||stride<width*bytes){ADD_FAILURE()<<"Invalid extracted storage";return {};}
  EXPECT_TRUE(format==FPDFBitmap_Gray||format==FPDFBitmap_BGR||format==FPDFBitmap_BGRx||format==FPDFBitmap_BGRA);
  Raster image{width,height,std::vector<uint8_t>(static_cast<size_t>(width)*height*4)};
  for(int y=0;y<height;++y)for(int x=0;x<width;++x){
    const auto* source=buffer+static_cast<size_t>(y)*stride+x*bytes;
    auto* destination=image.pixels.data()+(static_cast<size_t>(y)*width+x)*4;
    for(int c=0;c<3;++c)destination[c]=source[format==FPDFBitmap_Gray?0:c];
    destination[3]=format==FPDFBitmap_BGRA?source[3]:255;
  }
  return image;
}
// Match FPDFView's opaque-white bitmap caller. Its reverse-byte-order golden
// intentionally records swapped buffer channels rather than normalizing them.
Raster OpaquePage(const Api& api,FPDF_PAGE page,int width,int height,int flags) {
  Handle bitmap(api.FPDFBitmap_Create(width,height,0),api.FPDFBitmap_Destroy);
  if(!bitmap.get()){ADD_FAILURE()<<"Missing page bitmap";return {};}
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,width,height,0xffffffff));
  api.FPDF_RenderPageBitmap(bitmap.get(),page,0,0,width,height,0,flags | FPDF_NO_NATIVETEXT);
  return Bitmap(api,bitmap.get());
}
struct DifferencePolicy {int delta=0;double mse=0;int window=0;double window_mse=0;};
const DifferencePolicy fontations{12,.20,8,15};
const DifferencePolicy jpeg37{3,.05,8,15};
const DifferencePolicy jpeg38{3,.30,0,0};
void ComparePixels(const Raster& expected,const Raster& actual,Artifacts& artifacts,
                   const std::string& label,DifferencePolicy policy={}) {
  ASSERT_EQ(expected.width,actual.width);ASSERT_EQ(expected.height,actual.height);
  ASSERT_EQ(expected.pixels.size(),actual.pixels.size());ASSERT_FALSE(expected.pixels.empty());
  if(expected.pixels==actual.pixels)return;
  artifacts.Image(label+"-expected",expected);artifacts.Image(label+"-actual",actual);
  int maximum=0;double squared=0;
  const int width=expected.width,height=expected.height;
  const size_t sat_stride=static_cast<size_t>(width)+1;
  std::vector<uint64_t> sat(policy.window?sat_stride*(height+1):0);
  for(int y=0;y<height;++y)for(int x=0;x<width;++x){
    uint64_t pixel_error=0;const auto at=(static_cast<size_t>(y)*width+x)*4;
    for(int channel=0;channel<4;++channel){const int delta=int(expected.pixels[at+channel])-int(actual.pixels[at+channel]);
      maximum=std::max(maximum,std::abs(delta));if(channel<3)pixel_error+=uint64_t(delta*delta);}
    squared+=pixel_error;
    if(policy.window)sat[(y+1)*sat_stride+x+1]=pixel_error+sat[y*sat_stride+x+1]+sat[(y+1)*sat_stride+x]-sat[y*sat_stride+x];
  }
  // Upstream's pixel_diff_util.cpp measures RGB MSE, while maximum channel
  // delta also checks alpha. Its local limit examines every overlapping window.
  const double mse=squared/(static_cast<double>(width)*height*3);
  artifacts.Text(label+"-metrics","maximum channel delta: "+std::to_string(maximum)+"\nmean squared error: "+std::to_string(mse));
  EXPECT_LE(maximum,policy.delta)<<label;EXPECT_LE(mse,policy.mse)<<label;
  if(policy.window){
    if(width<policy.window||height<policy.window){EXPECT_LE(mse,policy.window_mse)<<label;}
    else {double maximum_window=0;
      for(int y=0;y+policy.window<=height;++y)for(int x=0;x+policy.window<=width;++x){
        const int right=x+policy.window,bottom=y+policy.window;
        const auto error=sat[bottom*sat_stride+right]-sat[y*sat_stride+right]-sat[bottom*sat_stride+x]+sat[y*sat_stride+x];
        maximum_window=std::max(maximum_window,double(error)/(3*policy.window*policy.window));
      }
      EXPECT_LE(maximum_window,policy.window_mse)<<label;
    }
  }
}
void Golden(const Raster& image,const std::string& basename,Artifacts& artifacts,DifferencePolicy policy={}) {
  ComparePixels(LoadPinnedGolden("embedder_tests/"+basename+".png"),image,artifacts,basename,policy);
}
using PageComparison=std::function<void(const Raster&,const Raster&,Artifacts&,const std::string&)>;
void CompareGlyphCoverage(const Raster& expected,const Raster& actual,Artifacts& artifacts,
                          const std::string& label,bool arial) {
  ASSERT_EQ(expected.width,actual.width);ASSERT_EQ(expected.height,actual.height);
  ASSERT_EQ(expected.pixels.size(),actual.pixels.size());
  std::array<std::array<int,4>,2> bounds{{{expected.width,expected.height,-1,-1},{actual.width,actual.height,-1,-1}}};
  std::array<uint64_t,2> mass{};
  for(int y=0;y<expected.height;++y)for(int x=0;x<expected.width;++x){
    const auto at=(static_cast<size_t>(y)*expected.width+x)*4;
    ASSERT_EQ(expected.pixels[at+3],actual.pixels[at+3]);ASSERT_EQ(actual.pixels[at+3],255);
    const int a=expected.pixels[at],b=actual.pixels[at];
    const bool gray=expected.pixels[at+1]==a&&expected.pixels[at+2]==a&&actual.pixels[at+1]==b&&actual.pixels[at+2]==b;
    if(!gray){
      for(int channel=0;channel<3;++channel){ASSERT_EQ(expected.pixels[at+channel],actual.pixels[at+channel]);}
      continue;
    }
    if((a==0||a==255)&&(b==0||b==255)){ASSERT_EQ(a,b);}
    if(a==255||b==255){ASSERT_LE(std::abs(a-b),arial?28:4);}
    if(a==0||b==0){ASSERT_LE(std::abs(a-b),4);}
    const std::array<int,2> samples{a,b};
    for(size_t owner=0;owner<2;++owner){mass[owner]+=255-samples[owner];
      if(samples[owner]<200){auto& box=bounds[owner];box[0]=std::min(box[0],x);box[1]=std::min(box[1],y);box[2]=std::max(box[2],x);box[3]=std::max(box[3],y);}}
  }
  EXPECT_EQ(bounds[0],bounds[1]);EXPECT_GT(mass[0],1000u);EXPECT_GT(mass[1],1000u);
  const auto difference=mass[0]>mass[1]?mass[0]-mass[1]:mass[1]-mass[0];
  EXPECT_LE(difference,std::max(mass[0],mass[1])/(arial?50:100));
  // Native is PNG-exact with pinned fonts. Fontations outlines rasterized by
  // AGG differ from FreeType only in these investigated grayscale glyph edges.
  // This local check uses mean absolute BGRA delta over the painted union,
  // unlike ComparePixels' source JPEG/Fontations RGB mean squared error.
  text_fixture::CompareTextRaster(expected,actual,artifacts,label,
                                 RasterTolerance{arial?64:32,0,arial?3.:1.6});
}
void CompareTimesCoverage(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareGlyphCoverage(a,b,artifacts,label,false);
}
void CompareArialCoverage(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareGlyphCoverage(a,b,artifacts,label,true);
}
using Verify=std::function<void(const Api&,FPDF_DOCUMENT,FPDF_PAGE,Artifacts&)>;
using PaintVerify=std::function<void(const Raster&)>;
void Exercise(const std::string& fixture,int width,int height,int flags,const std::string& golden,
              const Verify& verify={},const Verify& edit={},DifferencePolicy policy={},
              const PaintVerify& inspect={},bool opaque=false,bool original_matches=true,
              const PageComparison& glyph_coverage={}) {
  Artifacts artifacts;const auto bytes=Pdf(fixture);artifacts.Pdf(fixture+"-input",bytes);
  std::array<Raster,2> images;size_t backend=0;
  const auto check_golden=[&](const Raster& image,const Library& owner){
    if(golden.empty())return;
    const auto expected=LoadPinnedGolden("embedder_tests/"+golden+".png");
    if(glyph_coverage&&owner.name()=="rust")glyph_coverage(expected,image,artifacts,golden+"-rust-coverage");
    else ComparePixels(expected,image,artifacts,golden+"-"+owner.name(),glyph_coverage?DifferencePolicy{}:policy);
  };
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    ASSERT_EQ(api.FPDF_GetPageCount(document.get()),1);
    Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    EXPECT_DOUBLE_EQ(api.FPDF_GetPageWidth(page.get()),width);EXPECT_DOUBLE_EQ(api.FPDF_GetPageHeight(page.get()),height);
    const auto draw=[&](const Api& owner,FPDF_PAGE current){return opaque?OpaquePage(owner,current,width,height,flags):Render(owner,current,width,height,flags);};
    Raster before_edit;
    if(edit){before_edit=draw(api,page.get());artifacts.Image(writer->name()+"-original-"+fixture,before_edit);
      if(original_matches)check_golden(before_edit,*writer);
      ASSERT_NO_FATAL_FAILURE(edit(api,document.get(),page.get(),artifacts));}
    if(verify){ASSERT_NO_FATAL_FAILURE(verify(api,document.get(),page.get(),artifacts));}
    const auto original=draw(api,page.get());images[backend++]=original;
    check_golden(original,*writer);
    if(glyph_coverage&&edit&&original_matches)ComparePixels(before_edit,original,artifacts,writer->name()+"-unchanged-edit-"+fixture);
    if(inspect){ASSERT_NO_FATAL_FAILURE(inspect(original));}
    const auto saved=SaveDocument(api,document.get(),artifacts,writer->name()+"-"+fixture);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());const auto& read=reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);
      Handle loaded(read.FPDF_LoadPage(reopened.get(),0),read.FPDF_ClosePage);ASSERT_NE(loaded.get(),nullptr);
      if(verify){ASSERT_NO_FATAL_FAILURE(verify(read,reopened.get(),loaded.get(),artifacts));}
      const auto after=draw(read,loaded.get());check_golden(after,*reader);
      if(inspect){ASSERT_NO_FATAL_FAILURE(inspect(after));}
      const auto label=writer->name()+"-"+reader->name()+"-"+fixture;
      if(glyph_coverage&&writer!=reader)glyph_coverage(original,after,artifacts,label);
      else ComparePixels(original,after,artifacts,label,glyph_coverage?DifferencePolicy{}:policy);
    }
  }
  if(glyph_coverage)glyph_coverage(images[0],images[1],artifacts,"native-rust-"+fixture);
  else ComparePixels(images[0],images[1],artifacts,"native-rust-"+fixture,policy);
}
std::u16string ObjectText(const Api& api,FPDF_PAGEOBJECT object,FPDF_TEXTPAGE text) {
  const auto size=api.FPDFTextObj_GetText(object,text,nullptr,0);EXPECT_GE(size,2u);
  if(size<2||size%2){ADD_FAILURE()<<"Invalid UTF16 text byte size";return {};}
  std::vector<FPDF_WCHAR> value(size/2);EXPECT_EQ(api.FPDFTextObj_GetText(object,text,value.data(),size),size);
  EXPECT_EQ(value.back(),0);return {value.begin(),value.end()-1};
}
void Matrix(const Api& api,FPDF_PAGEOBJECT object,const FS_MATRIX& expected) {
  FS_MATRIX actual{};ASSERT_TRUE(api.FPDFPageObj_GetMatrix(object,&actual));
  EXPECT_FLOAT_EQ(actual.a,expected.a);EXPECT_FLOAT_EQ(actual.b,expected.b);EXPECT_FLOAT_EQ(actual.c,expected.c);
  EXPECT_FLOAT_EQ(actual.d,expected.d);EXPECT_FLOAT_EQ(actual.e,expected.e);EXPECT_FLOAT_EQ(actual.f,expected.f);
}

TEST(CoveragePinnedEditImage, JpxLzwFiltersRenderIdenticallyUnderAllSourceFlagsAndExposeMetadata) {
  const std::array<int,11> flags={0,FPDF_ANNOT,FPDF_LCD_TEXT,FPDF_NO_NATIVETEXT,FPDF_GRAYSCALE,
    FPDF_RENDER_LIMITEDIMAGECACHE,FPDF_RENDER_FORCEHALFTONE,FPDF_PRINTING,
    FPDF_RENDER_NO_SMOOTHTEXT,FPDF_RENDER_NO_SMOOTHIMAGE,FPDF_RENDER_NO_SMOOTHPATH};
  for(int flag:flags){SCOPED_TRACE(flag);
    Exercise("jpx_lzw",612,792,flag,flag==FPDF_GRAYSCALE?"jpx_lzw_grayscale":"jpx_lzw",
      [](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page,Artifacts&){
        const auto object=api.FPDFPage_GetObject(page,0);ASSERT_EQ(api.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_IMAGE);
        FPDF_IMAGEOBJ_METADATA metadata{};ASSERT_TRUE(api.FPDFImageObj_GetImageMetadata(object,page,&metadata));
        EXPECT_EQ(metadata.width,612u);EXPECT_EQ(metadata.height,792u);EXPECT_FLOAT_EQ(metadata.horizontal_dpi,72);
        EXPECT_FLOAT_EQ(metadata.vertical_dpi,72);EXPECT_EQ(metadata.bits_per_pixel,24u);
        EXPECT_EQ(metadata.colorspace,FPDF_COLORSPACE_UNKNOWN);EXPECT_EQ(metadata.marked_content_id,-1);
      },{}, {},[=](const Raster& image){
        const std::array<uint8_t,4> expected=flag==FPDF_GRAYSCALE
            ?std::array<uint8_t,4>{76,76,76,255}:std::array<uint8_t,4>{0,0,255,255};
        for(size_t at=0;at<image.pixels.size();at+=4)
          if(!std::equal(expected.begin(),expected.end(),image.pixels.begin()+at)) {
            ADD_FAILURE()<<"Unexpected solid-fill pixel at "<<at/4;
            return;
          }
      },true);
  }
}

void EmbeddedImages(const Api& api,FPDF_DOCUMENT,FPDF_PAGE page,Artifacts& artifacts) {
  ASSERT_EQ(api.FPDFPage_CountObjects(page),39);
  const auto nonimage=api.FPDFPage_GetObject(page,32);ASSERT_NE(api.FPDFPageObj_GetType(nonimage),FPDF_PAGEOBJ_IMAGE);
  EXPECT_EQ(api.FPDFImageObj_GetBitmap(nonimage),nullptr);EXPECT_EQ(api.FPDFImageObj_GetImageFilterCount(nonimage),0);
  const std::array<std::array<int,2>,6> sizes={{{109,88},{103,75},{92,68},{79,60},{126,106},{194,119}}};
  for(int index=33;index<=38;++index){SCOPED_TRACE(index);const auto object=api.FPDFPage_GetObject(page,index);
    ASSERT_EQ(api.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_IMAGE);
    Handle bitmap(api.FPDFImageObj_GetBitmap(object),api.FPDFBitmap_Destroy);ASSERT_NE(bitmap.get(),nullptr);
    EXPECT_EQ(api.FPDFBitmap_GetFormat(bitmap.get()),FPDFBitmap_BGR);
    const auto image=Bitmap(api,bitmap.get());EXPECT_EQ(image.width,sizes[index-33][0]);EXPECT_EQ(image.height,sizes[index-33][1]);
    Golden(image,"embedded_images_"+std::to_string(index),artifacts,index==37?jpeg37:index==38?jpeg38:DifferencePolicy{});
  }
  const auto image35=api.FPDFPage_GetObject(page,35);FPDF_IMAGEOBJ_METADATA metadata{};
  ASSERT_TRUE(api.FPDFImageObj_GetImageMetadata(image35,nullptr,&metadata));EXPECT_EQ(metadata.width,92u);EXPECT_EQ(metadata.height,68u);
  EXPECT_FLOAT_EQ(metadata.horizontal_dpi,96);EXPECT_FLOAT_EQ(metadata.vertical_dpi,96);
  EXPECT_EQ(metadata.marked_content_id,7);EXPECT_EQ(metadata.bits_per_pixel,0u);EXPECT_EQ(metadata.colorspace,FPDF_COLORSPACE_UNKNOWN);
  ASSERT_TRUE(api.FPDFImageObj_GetImageMetadata(image35,page,&metadata));EXPECT_EQ(metadata.bits_per_pixel,1u);EXPECT_EQ(metadata.colorspace,FPDF_COLORSPACE_INDEXED);
  const auto image37=api.FPDFPage_GetObject(page,37);ASSERT_TRUE(api.FPDFImageObj_GetImageMetadata(image37,page,&metadata));
  EXPECT_EQ(metadata.width,126u);EXPECT_EQ(metadata.height,106u);EXPECT_FLOAT_EQ(metadata.horizontal_dpi,162.173752f);
  EXPECT_FLOAT_EQ(metadata.vertical_dpi,162.555878f);EXPECT_EQ(metadata.bits_per_pixel,24u);EXPECT_EQ(metadata.colorspace,FPDF_COLORSPACE_DEVICERGB);EXPECT_EQ(metadata.marked_content_id,9);
  const auto filter=[&](int index,const std::vector<std::string>& expected){const auto object=api.FPDFPage_GetObject(page,index);
    ASSERT_EQ(api.FPDFImageObj_GetImageFilterCount(object),static_cast<int>(expected.size()));
    for(size_t i=0;i<expected.size();++i){const auto size=api.FPDFImageObj_GetImageFilter(object,static_cast<int>(i),nullptr,0);
      ASSERT_EQ(size,expected[i].size()+1);std::vector<char> value(size);
      ASSERT_EQ(api.FPDFImageObj_GetImageFilter(object,static_cast<int>(i),value.data(),size),size);EXPECT_STREQ(value.data(),expected[i].c_str());}
  };
  filter(33,{"FlateDecode"});filter(38,{"ASCIIHexDecode","DCTDecode"});
  EXPECT_EQ(api.FPDFImageObj_GetImageDataDecoded(api.FPDFPage_GetObject(page,33),nullptr,0),28776u);
}
TEST(CoveragePinnedEditImage, AllSixOriginalEmbeddedImagesKeepRawPixelsFiltersDimensionsAndMetadataAcrossSaving) {
  Exercise("embedded_images",612,792,0,"",EmbeddedImages,
    [](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page,Artifacts&){
      // These lengths describe the pinned original compressed streams; save
      // writers may choose different valid compression for the same pixels.
      EXPECT_EQ(api.FPDFImageObj_GetImageDataRaw(api.FPDFPage_GetObject(page,33),nullptr,0),4091u);
      EXPECT_EQ(api.FPDFImageObj_GetImageDataRaw(api.FPDFPage_GetObject(page,37),nullptr,0),4370u);
    },fontations);
}

TEST(CoveragePinnedEditImage, ChangingImageMatrixChangesRenderedExtractionButLeavesRawBitmapUntouched) {
  Exercise("embedded_images",612,792,0,"",
    [](const Api& api,FPDF_DOCUMENT document,FPDF_PAGE page,Artifacts& artifacts){
      ASSERT_EQ(api.FPDFPage_CountObjects(page),39);const auto object=api.FPDFPage_GetObject(page,33);
      Matrix(api,object,{120,0,0,43,72,646.510009765625f});
      Handle raw(api.FPDFImageObj_GetBitmap(object),api.FPDFBitmap_Destroy);ASSERT_NE(raw.get(),nullptr);
      Golden(Bitmap(api,raw.get()),"embedded_images_33",artifacts);
      Handle rendered(api.FPDFImageObj_GetRenderedBitmap(document,page,object),api.FPDFBitmap_Destroy);ASSERT_NE(rendered.get(),nullptr);
      EXPECT_EQ(api.FPDFBitmap_GetFormat(rendered.get()),FPDFBitmap_BGRA);
      EXPECT_EQ(api.FPDFBitmap_GetWidth(rendered.get()),120);EXPECT_EQ(api.FPDFBitmap_GetHeight(rendered.get()),43);
      Golden(Bitmap(api,rendered.get()),"embedded_images_33_rendered_transformed_agg",artifacts);
    },
    [](const Api& api,FPDF_DOCUMENT document,FPDF_PAGE page,Artifacts& artifacts){
      ASSERT_EQ(api.FPDFPage_CountObjects(page),39);const auto object=api.FPDFPage_GetObject(page,33);
      Matrix(api,object,{53,0,0,43,72,646.510009765625f});
      Handle raw(api.FPDFImageObj_GetBitmap(object),api.FPDFBitmap_Destroy);ASSERT_NE(raw.get(),nullptr);
      Golden(Bitmap(api,raw.get()),"embedded_images_33",artifacts);
      Handle rendered(api.FPDFImageObj_GetRenderedBitmap(document,page,object),api.FPDFBitmap_Destroy);ASSERT_NE(rendered.get(),nullptr);
      EXPECT_EQ(api.FPDFBitmap_GetWidth(rendered.get()),53);EXPECT_EQ(api.FPDFBitmap_GetHeight(rendered.get()),43);
      Golden(Bitmap(api,rendered.get()),"embedded_images_33_rendered_agg",artifacts);
      const FS_MATRIX matrix{120,0,0,43,72,646.510009765625f};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object,&matrix));
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page));
    },fontations);
}

TEST(CoveragePinnedEditImage, ExtractedImageBitmapRetainsTheOriginalClipAndTransparentOutsidePixels) {
  Exercise("image_clipped",100,100,0,"",
    [](const Api& api,FPDF_DOCUMENT document,FPDF_PAGE page,Artifacts& artifacts){
      ASSERT_EQ(api.FPDFPage_CountObjects(page),1);const auto object=api.FPDFPage_GetObject(page,0);
      ASSERT_EQ(api.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_IMAGE);
      Handle bitmap(api.FPDFImageObj_GetRenderedBitmap(document,page,object),api.FPDFBitmap_Destroy);ASSERT_NE(bitmap.get(),nullptr);
      const auto image=Bitmap(api,bitmap.get());ASSERT_EQ(image.width,4);ASSERT_EQ(image.height,4);Golden(image,"image_clipped",artifacts);
      for(int y=0;y<4;++y)for(int x=0;x<4;++x){const auto at=(size_t(y)*4+x)*4;
        EXPECT_EQ(image.pixels[at+3],x<2?255:0);
        if(x<2){EXPECT_EQ(image.pixels[at],0);EXPECT_EQ(image.pixels[at+1],y<2?0:255);EXPECT_EQ(image.pixels[at+2],y<2?255:0);}}
    },{}, {},[](const Raster& image){
      for(int y=0;y<100;++y)for(int x=0;x<100;++x){const auto at=(size_t(y)*100+x)*4;
        const bool inside=x<2&&y>=96;const int red_channel=inside&&y>=98?0:255;
        EXPECT_EQ(image.pixels[at],inside?0:255);EXPECT_EQ(image.pixels[at+1],inside&&y<98?0:255);EXPECT_EQ(image.pixels[at+2],red_channel);}
    });
}

TEST(CoveragePinnedEditImage, JpxChannelOrderFlagsSwapOriginalBlueAndRedImageBytes) {
  for(bool reverse:{false,true}){SCOPED_TRACE(reverse);
    Exercise("bug_1396264",100,200,reverse?FPDF_REVERSE_BYTE_ORDER:0,
      reverse?"bug_1396264_reverse_byte":"bug_1396264",{}, {},{},[=](const Raster& image){
        for(int y=0;y<200;++y)for(int x=0;x<100;++x){const auto at=(size_t(y)*100+x)*4;const bool top=y<100;
          EXPECT_EQ(image.pixels[at],top==reverse?255:0);EXPECT_EQ(image.pixels[at+1],0);EXPECT_EQ(image.pixels[at+2],top==reverse?0:255);}
      },true);
  }
}

TEST(CoveragePinnedEditImage, ResettingOriginalTextRetainsImportedClipPathsDuringContentRegeneration) {
  ScopedPinnedTestFonts fonts;
  for(bool second:{false,true}){SCOPED_TRACE(second);
    const auto verify=[=](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page,Artifacts&){
      ASSERT_EQ(api.FPDFPage_CountObjects(page),2);Handle text(api.FPDFText_LoadPage(page),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
      for(int index=second?1:0;index<2;++index){const auto object=api.FPDFPage_GetObject(page,index);ASSERT_EQ(api.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_TEXT);
        EXPECT_EQ(ObjectText(api,object,text.get()),second?u"Text":index?u"Unclipped Text":u"Clipped Text");
        const auto clip=api.FPDFPageObj_GetClipPath(object);ASSERT_NE(clip,nullptr);
        // CheckClip removes a rectangular clip that already contains the entire
        // object. Only bug1558's first text lies outside it and must retain it.
        const bool retained=!second&&index==0;
        EXPECT_EQ(api.FPDFClipPath_CountPaths(clip),retained?1:-1);
        float left=0,bottom=0,right=0,top=0;ASSERT_TRUE(api.FPDFPageObj_GetBounds(object,&left,&bottom,&right,&top));
        if(retained){EXPECT_GT(bottom,70);EXPECT_EQ(api.FPDFClipPath_CountPathSegments(clip,0),5);}
        else {EXPECT_GE(left,second?0:12);EXPECT_LE(right,second?50:112);EXPECT_GE(bottom,second?0:40);EXPECT_LE(top,second?75:70);}
      }
    };
    Exercise(second?"bug_1574":"bug_1558",200,second?300:200,0,second?"bug_1574_agg":"bug_1558_agg",verify,
      [=](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page,Artifacts&){
        ASSERT_EQ(api.FPDFPage_CountObjects(page),2);Handle text(api.FPDFText_LoadPage(page),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
        for(int index=second?1:0;index<2;++index){const auto object=api.FPDFPage_GetObject(page,index);const auto value=ObjectText(api,object,text.get());
          std::vector<FPDF_WCHAR> terminated(value.begin(),value.end());terminated.push_back(0);ASSERT_TRUE(api.FPDFText_SetText(object,terminated.data()));}
        ASSERT_TRUE(api.FPDFPage_GenerateContent(page));
      },fontations,{},false,true,CompareTimesCoverage);
  }
  fonts.ExpectMapped();
}

TEST(CoveragePinnedEditImage, OriginalHighlightPaintsOnScreenAndDisappearsForPrinting) {
  for(bool printing:{false,true}){SCOPED_TRACE(printing);
    Exercise("bug_1658",400,400,FPDF_ANNOT|(printing?FPDF_PRINTING:0),printing?"bug_1658_print":"bug_1658_annot_agg",
      [](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page,Artifacts&){EXPECT_GT(api.FPDFPage_GetAnnotCount(page),0);},{},{},
      [=](const Raster& image){size_t yellow=0;
        for(size_t at=0;at<image.pixels.size();at+=4){
          yellow+=image.pixels[at]<100&&image.pixels[at+1]>220&&image.pixels[at+2]>220;
        }
        EXPECT_EQ(yellow,printing?0u:1024u);},true);
  }
}

TEST(CoveragePinnedEditImage, InvalidShadingDictionaryAndMultipleImageFiltersRecoverToExactBlankPages) {
  for(const std::string fixture:{"bug_547706","bug_552046"}){SCOPED_TRACE(fixture);
    Exercise(fixture,612,792,0,"blank_612x792",{},{},{},[](const Raster& image){
      for(uint8_t channel:image.pixels)EXPECT_EQ(channel,255);
    });
  }
}
TEST(CoveragePinnedEditImage, MalformedRunLengthWithHugeDeviceNScanlineRetainsTheExpectedLastRow) {
  Exercise("bug_555784",612,792,0,"bug_555784",{},{},{},[](const Raster& image){
    for(int y=0;y<792;++y)for(int x=0;x<612;++x){const auto at=(size_t(y)*612+x)*4;
      for(int c=0;c<3;++c){EXPECT_EQ(image.pixels[at+c],y==791?0:255);}
      EXPECT_EQ(image.pixels[at+3],255);
    }
  });
}
TEST(CoveragePinnedEditImage, RepairedCrossReferenceArialRenderingIsIndependentOfNativeTextFlagOnAgg) {
  ScopedPinnedTestFonts fonts;
  for(bool no_native:{false,true}){SCOPED_TRACE(no_native);
    Exercise("bug_664284",612,792,no_native?FPDF_NO_NATIVETEXT:0,no_native?"bug_664284_no_native_text":"bug_664284_agg",
      {},[](const Api& api,FPDF_DOCUMENT document,FPDF_PAGE,Artifacts&){EXPECT_FALSE(api.FPDF_DocumentHasValidCrossReferenceTable(document));},fontations,{},true,true,CompareArialCoverage);
  }
  fonts.ExpectMapped();
}
TEST(CoveragePinnedEditImage, DeletingSecondOriginalTextObjectPreservesTheFirstObjectsResourcesAndSpacing) {
  ScopedPinnedTestFonts fonts;
  Exercise("bug_410996566",200,200,0,"bug_410996566",
    [](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page,Artifacts&){
      ASSERT_EQ(api.FPDFPage_CountObjects(page),1);const auto object=api.FPDFPage_GetObject(page,0);ASSERT_EQ(api.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_TEXT);
      Handle text(api.FPDFText_LoadPage(page),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
      auto value=ObjectText(api,object,text.get());
      // Original TJ kerning can create extraction-only space characters;
      // all source letters and punctuation must remain in their original order.
      value.erase(std::remove_if(value.begin(),value.end(),[](char16_t ch){return ch==u' '||ch==u'\r'||ch==u'\n';}),value.end());
      EXPECT_EQ(value,u"Hello,world!");
    },[](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page,Artifacts&){
      ASSERT_EQ(api.FPDFPage_CountObjects(page),2);Handle object(api.FPDFPage_GetObject(page,1),api.FPDFPageObj_Destroy);
      ASSERT_NE(object.get(),nullptr);ASSERT_TRUE(api.FPDFPage_RemoveObject(page,object.get()));object.reset();ASSERT_TRUE(api.FPDFPage_GenerateContent(page));
    },fontations,{},false,false,CompareTimesCoverage);
  fonts.ExpectMapped();
}
TEST(CoveragePinnedEditImage, OriginalColoredAndUncoloredPatternObjectsExtractTheirActualCells) {
  for(bool stroke:{false,true})for(bool uncolored:{false,true}){
    SCOPED_TRACE(stroke);
    SCOPED_TRACE(uncolored);
    const auto fixture=std::string("pattern_")+(stroke?"stroke":"fill")+(uncolored?"_uncolored":"");
    Exercise(fixture,200,300,0,"",[=](const Api& api,FPDF_DOCUMENT document,FPDF_PAGE page,Artifacts& artifacts){
      const auto object=api.FPDFPage_GetObject(page,0);ASSERT_EQ(api.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_PATH);
      Handle bitmap(stroke?api.FPDFPageObj_GetRenderedStrokePattern(document,object):api.FPDFPageObj_GetRenderedFillPattern(document,object),api.FPDFBitmap_Destroy);
      ASSERT_NE(bitmap.get(),nullptr);const auto image=Bitmap(api,bitmap.get());EXPECT_EQ(image.width,stroke?72:36);EXPECT_EQ(image.height,stroke?72:36);
      Golden(image,fixture,artifacts);
      if(uncolored){EXPECT_EQ(image.pixels[0],255);EXPECT_EQ(image.pixels[1],0);EXPECT_EQ(image.pixels[2],0);}
      else {EXPECT_EQ(image.pixels[0],44);EXPECT_EQ(image.pixels[1],27);EXPECT_EQ(image.pixels[2],238);}
      EXPECT_EQ(image.pixels[3],255);
    });
  }
}
}  // namespace
}  // namespace pdfoxium::integration
