#include "runtime_fixture.h"
#include "text_fixture.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <functional>
#include <sstream>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
constexpr int kSide=96;
constexpr uint8_t kCanary=0xa7;
constexpr size_t kGuard=16;
using Color=std::array<int,4>;  // RGB, alpha.
struct Surface {
  int format=FPDFBitmap_Gray;
  int flags=0;
  int back=255;
  int alpha=255;
  // WithMatrix already includes the page display transform.
  FS_MATRIX matrix{1,0,0,1,0,0};
  FS_RECTF clip{0,0,kSide,kSide};
};
int Step(int format){return format==FPDFBitmap_Gray?1:format==FPDFBitmap_BGR?3:4;}
Color At(const Raster& image,int x,int pdf_y) {
  const auto p=(static_cast<size_t>(image.height-1-pdf_y)*image.width+x)*4;
  return {image.pixels.at(p+2),image.pixels.at(p+1),image.pixels.at(p),image.pixels.at(p+3)};
}
void Pixel(const Raster& image,int x,int y,Color expected,int delta=0) {
  const auto actual=At(image,x,y);
  for(size_t c=0;c<4;++c) {
    if(c<3&&expected[3]==0)continue;
    EXPECT_NEAR(actual[c],expected[c],delta)<<"pixel "<<x<<','<<y<<" channel "<<c;
  }
}
std::vector<uint8_t> Scene(const std::string& content,const std::string& resources="",
    const std::vector<std::string>& assets={}) {
  std::vector<std::string> objects{
    "<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Resources << "+resources+" >> /Contents 4 0 R >>",
    PdfStream("",content)};
  objects.insert(objects.end(),assets.begin(),assets.end());return PdfFixture(objects);
}
// Independent healthy sibling of the required-hint fixture, matching the
// face-path campaign's original600x700 unhinted rectangle. Patch only the
// existing glyf/loca/head bytes and repair SFNT checksums; no renderer data.
std::vector<uint8_t> HealthyRequiredFont() {
  auto data=text_fixture::Read(TestRoot()/"crates/pdfoxium-core/tests/fixtures/hint-optional.ttf");
  const auto u16=[&](size_t p){return (unsigned(data.at(p))<<8)|data.at(p+1);};
  const auto u32=[&](size_t p){return (uint32_t(u16(p))<<16)|u16(p+2);};
  const auto w16=[&](size_t p,uint16_t x){data.at(p)=x>>8;data.at(p+1)=x;};
  const auto w32=[&](size_t p,uint32_t x){w16(p,x>>16);w16(p+2,x);};
  const auto sum=[&](size_t start,size_t length){
    uint32_t out=0;
    for(size_t p=0;p<length;p+=4){
      uint32_t word=0;
      for(size_t k=0;k<4;++k){word=(word<<8)|(p+k<length?data.at(start+p+k):0);}
      out+=word;
    }
    return out;
  };
  size_t head=0,glyf=0,loca=0,glyf_length=0;
  const unsigned count=u16(4);
  for(unsigned i=0;i<count;++i){const size_t row=12+16*i;const auto tag=u32(row),offset=u32(row+8);
    if(tag==0x68656164)head=offset;
    if(tag==0x676c7966){glyf=offset;glyf_length=u32(row+12);}
    if(tag==0x6c6f6361)loca=offset;
  }
  EXPECT_GT(head,0u);EXPECT_GT(glyf,0u);EXPECT_GT(loca,0u);EXPECT_GE(glyf_length,34u);
  if(!head||!glyf||!loca||glyf_length<34)return {};
  std::fill_n(data.begin()+glyf,glyf_length,0);
  w16(glyf,1);w16(glyf+6,600);w16(glyf+8,700);w16(glyf+10,3);
  for(size_t i=14;i<18;++i)data[glyf+i]=1;
  w16(glyf+20,600);w16(glyf+24,uint16_t(-600));w16(glyf+30,700);
  w16(loca+4,17);w16(head+40,600);w32(head+8,0);
  for(unsigned i=0;i<count;++i){const size_t row=12+16*i;w32(row+4,sum(u32(row+8),u32(row+12)));}
  w32(head+8,0xb1b0afba-sum(0,data.size()));return data;
}
std::vector<uint8_t> TextScene(const std::string& content,const std::string& font="ahem",
    const std::string& resources="",const std::vector<uint8_t>& replacement={}) {
  const auto data=!replacement.empty()?replacement:font=="ahem"?text_fixture::Resource("fonts/ahem/Ahem.ttf"):
      text_fixture::Read(TestRoot()/"crates/pdfoxium-core/tests/fixtures"/(font+".ttf"));
  const int width=font=="ahem"?1000:700;
  std::string widths;for(int c=32;c<=126;++c)widths+=std::to_string(width)+" ";
  return Scene(content,"/Font << /F 5 0 R >> "+resources,{
    "<< /Type /Font /Subtype /TrueType /BaseFont /"+std::string(font=="ahem"?"Ahem":"HintDeviceFixture")+
      " /FirstChar 32 /LastChar 126 /Widths ["+widths+"] /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R >>",
    "<< /Type /FontDescriptor /FontName /"+std::string(font=="ahem"?"Ahem":"HintDeviceFixture")+
      " /Flags 32 /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /CapHeight 800 /ItalicAngle 0 /StemV 80 /FontFile2 7 0 R >>",
    PdfStream("/Length1 "+std::to_string(data.size()),std::string(data.begin(),data.end()))});
}
Raster Copy(const Api& api,FPDF_BITMAP bitmap,bool reverse=false) {
  Raster image;
  if(!bitmap){ADD_FAILURE()<<"missing bitmap";return image;}
  image.width=api.FPDFBitmap_GetWidth(bitmap);image.height=api.FPDFBitmap_GetHeight(bitmap);
  const int format=api.FPDFBitmap_GetFormat(bitmap),step=Step(format),stride=api.FPDFBitmap_GetStride(bitmap);
  const auto* data=static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap));
  EXPECT_GE(stride,image.width*step);
  if(!data||image.width<=0||image.height<=0){ADD_FAILURE()<<"invalid bitmap";return image;}
  image.pixels.reserve(static_cast<size_t>(image.width)*image.height*4);
  for(int y=0;y<image.height;++y)for(int x=0;x<image.width;++x) {
    const auto* p=data+static_cast<size_t>(y)*stride+x*step;
    if(step==1)image.pixels.insert(image.pixels.end(),{p[0],p[0],p[0],255});
    else image.pixels.insert(image.pixels.end(),{p[reverse?2:0],p[1],p[reverse?0:2],
      static_cast<uint8_t>(format==FPDFBitmap_BGRA?p[3]:255)});
  }
  return image;
}
Raster Paint(const Api& api,FPDF_PAGE page,const Surface& surface) {
  const int step=Step(surface.format),stride=((kSide*step+3)&~3)+12;
  std::vector<uint8_t> bytes(kGuard+static_cast<size_t>(stride)*kSide+kGuard,kCanary);
  auto* data=bytes.data()+kGuard;
  for(int y=0;y<kSide;++y)for(int x=0;x<kSide;++x) {
    auto* p=data+y*stride+x*step;
    std::fill_n(p,step,static_cast<uint8_t>(surface.back));
    if(step==4)p[3]=surface.format==FPDFBitmap_BGRA?surface.alpha:255;
  }
  Handle bitmap(api.FPDFBitmap_CreateEx(kSide,kSide,surface.format,data,stride),api.FPDFBitmap_Destroy);
  if(!bitmap.get()){ADD_FAILURE()<<"external bitmap creation";return {};}
  EXPECT_EQ(api.FPDFBitmap_GetBuffer(bitmap.get()),data);
  EXPECT_EQ(api.FPDFBitmap_GetStride(bitmap.get()),stride);
  EXPECT_EQ(api.FPDFBitmap_GetFormat(bitmap.get()),surface.format);
  api.FPDF_RenderPageBitmapWithMatrix(bitmap.get(),page,&surface.matrix,&surface.clip,surface.flags);
  for(size_t i=0;i<kGuard;++i){EXPECT_EQ(bytes[i],kCanary);EXPECT_EQ(bytes[bytes.size()-1-i],kCanary);}
  for(int y=0;y<kSide;++y)for(int x=kSide*step;x<stride;++x)
    EXPECT_EQ(data[y*stride+x],kCanary)<<"padding row "<<y<<" byte "<<x;
  const auto image=Copy(api,bitmap.get(),surface.flags&FPDF_REVERSE_BYTE_ORDER);
  // Integer external clips are a device contract, independent of PDF content.
  for(int y=0;y<kSide;++y)for(int x=0;x<kSide;++x)
    if(x<surface.clip.left||x>=surface.clip.right||y<surface.clip.top||y>=surface.clip.bottom) {
      const auto p=(static_cast<size_t>(y)*kSide+x)*4;
      for(int c=0;c<3;++c)EXPECT_EQ(image.pixels[p+c],surface.back)<<x<<','<<y;
      EXPECT_EQ(image.pixels[p+3],surface.format==FPDFBitmap_BGRA?surface.alpha:255);
    }
  return image;
}
using Inspect=std::function<void(const Api&,FPDF_DOCUMENT,FPDF_PAGE,const Raster&)>;
void Workflow(const std::vector<uint8_t>& bytes,const std::string& label,Surface surface,
    const Inspect& inspect,RasterTolerance tolerance={2,0,.15},bool text=false,
    const std::vector<uint8_t>& healthy={}) {
  Artifacts artifacts;std::array<Raster,2> images,controls;size_t backend=0;
  auto compare=[&](const Raster& a,const Raster& b,const std::string& name,bool exact){
    if(exact)CompareRaster(a,b,artifacts,name);
    else if(text)text_fixture::CompareTextRaster(a,b,artifacts,name,tolerance);
    else CompareRaster(a,b,artifacts,name,tolerance);
  };
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(label+" writer="+writer->name());const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
    ASSERT_NE(page.get(),nullptr);images[backend]=Paint(api,page.get(),surface);
    ASSERT_EQ(images[backend].pixels.size(),static_cast<size_t>(kSide*kSide*4));
    ASSERT_NO_FATAL_FAILURE(inspect(api,document.get(),page.get(),images[backend]));
    if(!healthy.empty()) {
      Handle control(api.FPDF_LoadMemDocument64(healthy.data(),healthy.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(control.get(),nullptr);Handle cp(api.FPDF_LoadPage(control.get(),0),api.FPDF_ClosePage);
      ASSERT_NE(cp.get(),nullptr);controls[backend]=Paint(api,cp.get(),surface);
    }
    const auto saved=SaveDocument(api,document.get(),artifacts,label+writer->name());ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE("reader="+reader->name());const auto& read=reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(),nullptr);Handle after(read.FPDF_LoadPage(reopened.get(),0),read.FPDF_ClosePage);
      ASSERT_NE(after.get(),nullptr);const auto image=Paint(read,after.get(),surface);
      ASSERT_EQ(image.pixels.size(),static_cast<size_t>(kSide*kSide*4));
      ASSERT_NO_FATAL_FAILURE(inspect(read,reopened.get(),after.get(),image));
      if(healthy.empty())compare(images[backend],image,label+writer->name()+reader->name(),reader==writer);
      else {
        // Two explicitly observed native defects below retain each reader's
        // own original appearance through both writers. Correct Rust output
        // is separately compared with an independent healthy common control.
        Handle original(read.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),read.FPDF_CloseDocument);
        ASSERT_NE(original.get(),nullptr);Handle op(read.FPDF_LoadPage(original.get(),0),read.FPDF_ClosePage);
        ASSERT_NE(op.get(),nullptr);
        compare(Paint(read,op.get(),surface),image,label+writer->name()+reader->name()+"-reader-original",true);
      }
    }
    ++backend;
  }
  if(healthy.empty())compare(images[0],images[1],label+"-engines",false);
  else {
    EXPECT_NE(images[0].pixels,controls[0].pixels)<<"pinned native defect unexpectedly changed";
    compare(controls[0],controls[1],label+"-healthy-engines",false);
    compare(controls[0],images[1],label+"-correct-rust-control",false);
  }
}
void GrayInk(const Raster& image,size_t minimum) {
  size_t ink=0;
  for(size_t p=0;p<image.pixels.size();p+=4) {
    EXPECT_EQ(image.pixels[p],image.pixels[p+1]);EXPECT_EQ(image.pixels[p],image.pixels[p+2]);
    EXPECT_EQ(image.pixels[p+3],255);ink+=image.pixels[p]<200;
  }
  EXPECT_GE(ink,minimum);
}
std::string Number(double x){std::ostringstream s;s<<x;return s.str();}
std::string Glyph(double size,double x,double y,const std::string& body="(A) Tj") {
  return "BT /F "+Number(size)+" Tf 1 0 0 1 "+Number(x)+" "+Number(y)+" Tm "+body+" ET ";
}

// CFX_RenderDevice::DrawNormalText chooses kNormal on a smooth <16bpp
// display destination. These are real Gray8 targets, not FPDF_GRAYSCALE on BGRA.
// Ahem has an independent 1000/em square [0,-200,1000,800] for A/B and a blank
// 1000/em space. Integer grid rows are exact; fractional square edges permit
// small coverage differences only. Source/raster analysis bounds these square
// edges separately; none of these allowances permit moving or missing cells.
// With the correct page display orientation, fractional squares differ by1–2
// at gray edges (max cropped mean .110947). The12/16pt runs have a3-level
// fringe difference, exact dark bounds and under.1% ink-mass difference.
TEST(CoverageBitmapDevicePaths, GrayNormalGlyphMasksFillKnownAhemCellsOnPaddedExternalRows) {
  for(int back:{255,192,64})for(int flags:{0,FPDF_LCD_TEXT,FPDF_NO_NATIVETEXT}) {
    Surface surface;surface.back=back;surface.flags=flags;
    Workflow(TextScene(Glyph(20,8,32,"(A B) Tj")),"gray-cells-"+std::to_string(back)+std::to_string(flags),surface,
      [=](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page,const Raster& image){
        GrayInk(image,700);Pixel(image,18,38,{0,0,0,255});Pixel(image,58,38,{0,0,0,255});
        Pixel(image,38,38,{back,back,back,255});Pixel(image,4,4,{back,back,back,255});
        Handle text(api.FPDFText_LoadPage(page),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
        EXPECT_EQ(text_fixture::Extract(api,text.get()),u"A B");
      },{0,0,0},true);
  }
}

TEST(CoverageBitmapDevicePaths, GrayNormalFractionalSquaresKeepCoverageConfinedToAnalyticalEdges) {
  for(double size:{12.5,18.75,25.25}) {
    const double x=8.25,y=32.25;
    Workflow(TextScene(Glyph(size,x,y)),"gray-fractional-"+Number(size),{},
      [=](const Api&,FPDF_DOCUMENT,FPDF_PAGE,const Raster& image){
        GrayInk(image,80);size_t fringe=0;uint64_t mass=0;
        for(int row=0;row<kSide;++row)for(int col=0;col<kSide;++col) {
          const int value=image.pixels[(static_cast<size_t>(row)*kSide+col)*4];mass+=255-value;
          if(value<255) {
            EXPECT_GE(col,x-1);EXPECT_LE(col,x+size+1);
            const double py=kSide-1-row;EXPECT_GE(py,y-.2*size-2);EXPECT_LE(py,y+.8*size+1);
          }
          fringe+=value>0&&value<255;
        }
        EXPECT_GT(fringe,0u);EXPECT_NEAR(double(mass)/255,size*size,2*size);
        Pixel(image,int(x+size*.5),int(y+size*.3),{0,0,0,255});
      },{2,0,.12},true);
  }
}

TEST(CoverageBitmapDevicePaths, GrayColoredGlyphMasksApplyLuminanceAndConstantOpacityToInterior) {
  for(const std::array<int,3> color:{std::array<int,3>{255,0,0},{0,255,0},{0,0,255},{64,128,192}})
    for(bool translucent:{false,true}) {
      const int gray=(30*color[0]+59*color[1]+11*color[2])/100;
      const int expected=translucent?(gray*127+255*128)/255:gray;
      const std::string rgb=Number(color[0]/255.)+" "+Number(color[1]/255.)+" "+Number(color[2]/255.)+" rg ";
      const auto bytes=TextScene("q /G gs "+rgb+Glyph(20,8,32)+"Q","ahem",
        "/ExtGState << /G << /ca "+std::string(translucent?".5":"1")+" >> >>");
      // Pinned native applies ca127 in the temporary coverage mask and again
      // at SetBitMask: effective63. Keep that observation, while Rust must
      // match an opaque Ahem square in the independently computed final gray.
      const auto healthy=translucent?TextScene(Number(expected/255.)+" g "+Glyph(20,8,32)):std::vector<uint8_t>{};
      Workflow(bytes,"gray-color-"+std::to_string(gray)+std::to_string(translucent),{},
        [=](const Api& api,FPDF_DOCUMENT,FPDF_PAGE,const Raster& image){
          const bool native=&api==&Implementations()[0]->api();
          const int result=translucent&&native?(gray*63+255*192)/255:expected;
          Pixel(image,18,38,{result,result,result,255},1);Pixel(image,4,4,{255,255,255,255});
        },{2,0,.15},true,healthy);
    }
}

TEST(CoverageBitmapDevicePaths, GrayTextTjNegativeAdvancesUnionOverlappingGlyphMasksWithoutLosingGaps) {
  // A advances16, TJ+500 backs up8, and TJ-1000 adds16 after the
  // second glyph. The square union is8..32 then a disjoint48..64 cell.
  Workflow(TextScene(Glyph(16,8,32,"[(A) 500 (A) -1000 (B)] TJ")),"gray-tj-union",{},
    [](const Api&,FPDF_DOCUMENT,FPDF_PAGE,const Raster& image){
      GrayInk(image,600);Pixel(image,12,36,{0,0,0,255});Pixel(image,24,36,{0,0,0,255});
      Pixel(image,40,36,{255,255,255,255});Pixel(image,56,36,{0,0,0,255});
      Pixel(image,72,36,{255,255,255,255});
    },{3,0,.1},true);
}

TEST(CoverageBitmapDevicePaths, GrayRotatedAndReflectedNormalGlyphMasksKeepTransformedCellCenters) {
  struct Row{const char* matrix;int x,y;};
  for(const auto row:{Row{"0 1 -1 0 50 12",44,22},Row{"-1 0 0 1 70 32",60,38},Row{"1 0 0 -1 8 60",18,54}}) {
    Workflow(TextScene("BT /F 20 Tf "+std::string(row.matrix)+" Tm (A) Tj ET"),
      "gray-transform-"+std::to_string(row.x)+std::to_string(row.y),{},
      [=](const Api&,FPDF_DOCUMENT,FPDF_PAGE,const Raster& image){
        GrayInk(image,350);Pixel(image,row.x,row.y,{0,0,0,255});Pixel(image,4,4,{255,255,255,255});
      },{0,0,0},true);
  }
}

TEST(CoverageBitmapDevicePaths, GrayNormalGlyphRunBoundsIntersectDeviceClipAndNegativeOrigins) {
  for(int offset:{-12,0,12}) {
    Surface surface;surface.clip={16,16,64,64};surface.matrix.e=offset;
    Workflow(TextScene(Glyph(12,2,40,"(AAAAAAAA) Tj")),"gray-clipped-"+std::to_string(offset),surface,
      [](const Api&,FPDF_DOCUMENT,FPDF_PAGE,const Raster& image){
        GrayInk(image,400);Pixel(image,32,44,{0,0,0,255});Pixel(image,15,44,{255,255,255,255});
        Pixel(image,64,44,{255,255,255,255});Pixel(image,32,20,{255,255,255,255});
      },{3,0,.18},true);
  }
}

TEST(CoverageBitmapDevicePaths, GrayNormalHintRequiredAndOptionalOutlinesUseDistinctAnalyticalWidths) {
  // Single13.5pt destination case: font-face point-size tables live elsewhere.
  // Required SHPIX reconstructs600x700 at Rust's fixed1000ppem; native runs
  // this adversarial synthetic program at64ppem and expands its right edge.
  // An unhinted sibling supplies the independent600x700 healthy glyph control.
  for(const std::string font:{"hint-optional","hint-reliant"}) {
    constexpr double size=13.5;const bool required=font=="hint-reliant";
    const double width=size*(required?.6:.4);
    const auto healthy=required?TextScene(Glyph(size,8,32),"hint-optional","",HealthyRequiredFont()):std::vector<uint8_t>{};
    Workflow(TextScene(Glyph(size,8,32),font),"gray-"+font+Number(size),{},
      [=](const Api& api,FPDF_DOCUMENT,FPDF_PAGE,const Raster& image){
        const bool native=&api==&Implementations()[0]->api();
        GrayInk(image,12);Pixel(image,9,34,{0,0,0,255},1);
        uint64_t mass=0;int left=kSide,right=-1;
        for(int y=0;y<kSide;++y)for(int x=0;x<kSide;++x) {
          const int gray=image.pixels[(static_cast<size_t>(y)*kSide+x)*4];mass+=255-gray;
          if(gray<180){left=std::min(left,x);right=std::max(right,x);}
        }
        EXPECT_EQ(left,8);
        if(required&&native){EXPECT_GT(right-left+1,40);EXPECT_LT(right-left+1,52);}
        else {
          Pixel(image,int(8+width+3),34,{255,255,255,255});
          EXPECT_NEAR(right-left+1,width,1);EXPECT_NEAR(double(mass)/255,width*size*.7,2*size);
          if(required)Pixel(image,int(8+size*.5),int(32+size*.3),{0,0,0,255},1);
        }
      },{3,0,.5},true,healthy);
  }
}

TEST(CoverageBitmapDevicePaths, GrayBlankZeroSizeOffscreenAndWhollyClippedRunsLeaveVisibleAnchorIntact) {
  Surface surface;surface.clip={8,8,80,80};
  const auto bytes=TextScene(Glyph(12,12,28)+Glyph(12,40,28,"(   ) Tj")+
    Glyph(0,24,50)+Glyph(12,-200,28)+Glyph(12,12,120)+Glyph(12,84,28));
  Workflow(bytes,"gray-empty-and-clipped",surface,
    [](const Api&,FPDF_DOCUMENT,FPDF_PAGE,const Raster& image){
      GrayInk(image,120);Pixel(image,18,32,{0,0,0,255});Pixel(image,46,32,{255,255,255,255});
      Pixel(image,30,52,{255,255,255,255});Pixel(image,84,32,{255,255,255,255});
      size_t dark=0;for(size_t i=0;i<image.pixels.size();i+=4)dark+=image.pixels[i]<180;
      EXPECT_GE(dark,120u);EXPECT_LE(dark,170u);
    },{3,0,.18},true);
}

std::string Image(int width,int height,const std::string& space,int bits,const std::string& data,
    const std::string& extra="") {
  return PdfStream("/Type /XObject /Subtype /Image /Width "+std::to_string(width)+" /Height "+
    std::to_string(height)+" /BitsPerComponent "+std::to_string(bits)+" /ColorSpace "+space+" "+extra,data);
}
void RawImage(const Api& api,FPDF_PAGE page,int width,int height,const std::function<void(const Raster&)>& inspect) {
  ASSERT_EQ(api.FPDFPage_CountObjects(page),1);const auto object=api.FPDFPage_GetObject(page,0);
  ASSERT_NE(object,nullptr);ASSERT_EQ(api.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_IMAGE);
  Handle bitmap(api.FPDFImageObj_GetBitmap(object),api.FPDFBitmap_Destroy);ASSERT_NE(bitmap.get(),nullptr);
  const auto image=Copy(api,bitmap.get());ASSERT_EQ(image.width,width);ASSERT_EQ(image.height,height);
  ASSERT_NO_FATAL_FAILURE(inspect(image));
}

TEST(CoverageBitmapDevicePaths, GrayIndexedOneAndEightBitRowsConvertPaletteWithoutLeakingPackedTailBits) {
  for(int bits:{1,8}) {
    // Odd13px rows distinguish sample count from packed/aligned row pitch.
    std::string data;
    for(int y=0;y<5;++y) {
      if(bits==1){data.push_back(char(0x55));data.push_back(char(0x57));}
      else for(int x=0;x<13;++x)data.push_back(char(x%2));
    }
    const auto bytes=Scene("q 52 0 0 20 12 36 cm /I Do Q","/XObject << /I 5 0 R >>",
      {Image(13,5,"[/Indexed /DeviceRGB 1 <FF000000FF00>]",bits,data)});
    Surface surface;surface.flags=FPDF_RENDER_NO_SMOOTHIMAGE;
    Workflow(bytes,"gray-packed-palette-"+std::to_string(bits),surface,
      [](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page,const Raster& image){
        Pixel(image,14,46,{76,76,76,255},1);Pixel(image,18,46,{150,150,150,255},1);
        Pixel(image,62,46,{76,76,76,255},1);Pixel(image,68,46,{255,255,255,255});
        RawImage(api,page,13,5,[](const Raster& raw){
          for(int y=0;y<5;++y)for(int x=0;x<13;++x)
            Pixel(raw,x,y,x%2?Color{0,255,0,255}:Color{255,0,0,255});
        });
      },{1,0,.1});
  }
}

TEST(CoverageBitmapDevicePaths, GrayNonseparableImageBlendModesKeepIndependentSourceAndBackdropLuminance) {
  // Gray compositor Color/Hue/Saturation retain the gray backdrop;
  // Luminosity takes the source. Colored channels are deliberately unequal.
  for(const std::string mode:{"Hue","Saturation","Color","Luminosity"}) {
    const auto bytes=Scene(".3764706 g 0 0 96 96 re f q /G gs 48 0 0 48 16 24 cm /I Do Q",
      "/ExtGState << /G << /BM /"+mode+" >> >> /XObject << /I 5 0 R >>",
      {Image(1,1,"/DeviceRGB",8,std::string("\xe0\x40\xa0",3))});
    Workflow(bytes,"gray-nonseparable-"+mode,{},
      [=](const Api&,FPDF_DOCUMENT,FPDF_PAGE,const Raster& image){
        const int gray=mode=="Luminosity"?(224*30+64*59+160*11)/100:96;
        Pixel(image,40,48,{gray,gray,gray,255},2);Pixel(image,4,4,{96,96,96,255},1);
      },{2,0,.15});
  }
}

TEST(CoverageBitmapDevicePaths, GrayAnisotropicRgbDownsampleUsesEverySourceStripeAndRawExtractionKeepsColor) {
  // Prior downsample rows use square gray quadrants. Here a513x3 color source
  // collapses each complete171px stripe to8px, with independentRGB extraction.
  const std::array<std::array<int,3>,3> colors{{{255,0,0},{0,255,0},{0,0,255}}};
  std::string data;for(int y=0;y<3;++y)for(int x=0;x<513;++x)
    for(int c:colors[x/171])data.push_back(char(c));
  const auto bytes=Scene("q 24 0 0 48 24 24 cm /I Do Q","/XObject << /I 5 0 R >>",
    {Image(513,3,"/DeviceRGB",8,data)});
  for(int flags:{0,FPDF_RENDER_FORCEHALFTONE}) {
    Surface surface;surface.flags=flags;
    Workflow(bytes,"gray-anisotropic-"+std::to_string(flags),surface,
      [=](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page,const Raster& image){
        Pixel(image,27,48,{76,76,76,255},1);Pixel(image,35,48,{150,150,150,255},1);
        Pixel(image,43,48,{28,28,28,255},1);Pixel(image,52,48,{255,255,255,255});
        RawImage(api,page,513,3,[=](const Raster& raw){
          for(int stripe=0;stripe<3;++stripe){const auto c=colors[stripe];Pixel(raw,stripe*171+85,1,{c[0],c[1],c[2],255});}
        });
      },{2,0,.15});
  }
}

TEST(CoverageBitmapDevicePaths, ClippedSoftMaskBlendsIntoPartialAlphaAndReverseOrderDestinations) {
  // Unlike the existing uniform-alpha surface table, the image is also under
  // a curved AA clip. Its interior has an independent Porter-Duff result,
  // and the untouched outside retains the caller's genuine partial alpha.
  const std::string clip="48 12 m 67.882 12 84 28.118 84 48 c 84 67.882 67.882 84 48 84 c "
    "28.118 84 12 67.882 12 48 c 12 28.118 28.118 12 48 12 c h W n ";
  for(bool reverse:{false,true})for(int back_alpha:{0,96,255}) {
    Surface surface;surface.format=FPDFBitmap_BGRA;surface.back=64;surface.alpha=back_alpha;
    surface.flags=reverse?FPDF_REVERSE_BYTE_ORDER:0;
    const auto bytes=Scene("q "+clip+"72 0 0 72 12 12 cm /I Do Q","/XObject << /I 5 0 R >>",
      {Image(1,1,"/DeviceRGB",8,std::string("\xc0\x40\x80",3),"/SMask 6 0 R"),
       Image(1,1,"/DeviceGray",8,std::string(1,char(128)))});
    Workflow(bytes,"clip-alpha-conversion-"+std::to_string(reverse)+std::to_string(back_alpha),surface,
      [=](const Api&,FPDF_DOCUMENT,FPDF_PAGE,const Raster& image){
        const double sa=128./255,ba=back_alpha/255.,alpha=sa+ba*(1-sa);Color expected{};
        for(int c=0;c<3;++c){const int source=std::array<int,3>{192,64,128}[c];
          expected[c]=int(std::lround((source*sa+64*ba*(1-sa))/alpha));}
        expected[3]=int(std::lround(alpha*255));Pixel(image,48,48,expected,2);
        Pixel(image,4,4,{64,64,64,back_alpha});
        Pixel(image,13,13,{64,64,64,back_alpha});
      },{2,0,.15});
  }
}

TEST(CoverageBitmapDevicePaths, FilledTwoPointAndRetracedZeroAreaPathsRemainVisibleAcrossPixelFormats) {
  // Source DrawPath has a special two-point cosmetic-line fallback; the
  // longer palindrome goes through GetZeroAreaPath instead of polygon fill.
  for(const std::string path:{"12 28 m 76 28 l","12 28 m 76 28 l 12 28 l",
      "12 28 m 44 28 l 76 28 l 44 28 l 12 28 l"})
    for(int format:{FPDFBitmap_Gray,FPDFBitmap_BGR,FPDFBitmap_BGRA}) {
      Surface surface;surface.format=format;surface.flags=FPDF_RENDER_NO_SMOOTHPATH;
      Workflow(Scene("0 g "+path+" f"),"zeroarea-"+std::to_string(path.size())+std::to_string(format),surface,
        [](const Api&,FPDF_DOCUMENT,FPDF_PAGE,const Raster& image){
          size_t ink=0;for(int y=24;y<=31;++y)for(int x=10;x<80;++x) {
            const auto pixel=At(image,x,y);if(pixel[0]<255){++ink;EXPECT_LE(std::abs(y-28),2);}
          }
          EXPECT_GT(ink,40u);EXPECT_LT(ink,180u);Pixel(image,40,40,{255,255,255,255});
        },{0,0,0});
    }
}
}  // namespace
}  // namespace pdfoxium::integration
