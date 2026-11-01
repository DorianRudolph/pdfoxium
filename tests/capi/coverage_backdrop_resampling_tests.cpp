#include "runtime_fixture.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <functional>
#include <sstream>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
constexpr int kSide=100;
using RGB=std::array<int,3>;
using RGBA=std::array<int,4>;
struct Matrix {
  double a=64,b=0,c=0,d=64,e=18,f=18;
  std::string Pdf() const {std::ostringstream s;s<<a<<' '<<b<<' '<<c<<' '<<d<<' '<<e<<' '<<f;return s.str();}
  std::array<double,2> Map(double u,double v) const{return {a*u+c*v+e,b*u+d*v+f};}
};
const std::array<Matrix,6> kMatrices{{{}, {-64,0,0,64,82,18}, {0,64,-64,0,82,18},
  {48,16,-16,48,34,18},{64,0,12,48,12,24},{80.5,0,0,47.25,8.25,20.75}}};
struct Surface {int format=FPDFBitmap_BGRA;bool transparent=false;bool reverse=false;int flags=0;int x=0,y=0;};
const std::array<Surface,5> kSurfaces{{{FPDFBitmap_Gray},{FPDFBitmap_BGR},{FPDFBitmap_BGRx},
  {FPDFBitmap_BGRA},{FPDFBitmap_BGRA,true}}};
RGBA At(const Raster& image,int x,int pdf_y) {
  const auto p=(static_cast<size_t>(kSide-1-pdf_y)*kSide+x)*4;
  return {image.pixels.at(p+2),image.pixels.at(p+1),image.pixels.at(p),image.pixels.at(p+3)};
}
void Pixel(const Raster& image,int x,int y,RGBA expected,int delta=2) {
  const auto actual=At(image,x,y);
  for(size_t channel=0;channel<4;++channel) {
    if(channel<3&&expected[3]==0)continue;  // Straight RGB is undefined at alpha0.
    EXPECT_NEAR(actual[channel],expected[channel],delta)<<"pixel "<<x<<','<<y<<" channel "<<channel;
  }
}
std::vector<uint8_t> Scene(const std::string& content,const std::string& resources,
    const std::vector<std::string>& assets) {
  std::vector<std::string> objects{
    "<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << "+resources+" >> /Contents 4 0 R >>",
    PdfStream("",content)};
  objects.insert(objects.end(),assets.begin(),assets.end());return PdfFixture(objects);
}
Raster Paint(const Api& api,FPDF_PAGE page,Surface surface) {
  Handle bitmap(api.FPDFBitmap_CreateEx(kSide,kSide,surface.format,nullptr,0),api.FPDFBitmap_Destroy);
  if(!bitmap.get()){ADD_FAILURE()<<"bitmap allocation";return {};}
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,kSide,kSide,surface.transparent?0:0xffffffff));
  api.FPDF_RenderPageBitmap(bitmap.get(),page,surface.x,surface.y,kSide,kSide,0,
      (surface.flags|(surface.reverse?FPDF_REVERSE_BYTE_ORDER:0)) | FPDF_NO_NATIVETEXT);
  const auto* data=static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
  if(!data){ADD_FAILURE()<<"bitmap buffer";return {};}
  const int stride=api.FPDFBitmap_GetStride(bitmap.get());
  const int step=surface.format==FPDFBitmap_Gray?1:surface.format==FPDFBitmap_BGR?3:4;
  Raster image{kSide,kSide,{}};
  for(int y=0;y<kSide;++y)for(int x=0;x<kSide;++x) {
    const auto* p=data+y*stride+x*step;
    if(step==1)image.pixels.insert(image.pixels.end(),{p[0],p[0],p[0],255});
    else image.pixels.insert(image.pixels.end(),{p[surface.reverse?2:0],p[1],p[surface.reverse?0:2],
        static_cast<uint8_t>(surface.format==FPDFBitmap_BGRA?p[3]:255)});
  }
  return image;
}
using Inspect=std::function<void(const Raster&)>;
void Workflow(const std::vector<uint8_t>& bytes,const std::string& label,Surface surface,
    const Inspect& inspect,RasterTolerance tolerance={3,0,.25}) {
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(label+" writer="+writer->name());const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
    ASSERT_NE(page.get(),nullptr);images[backend]=Paint(api,page.get(),surface);
    ASSERT_EQ(images[backend].pixels.size(),static_cast<size_t>(kSide*kSide*4));
    ASSERT_NO_FATAL_FAILURE(inspect(images[backend]));
    const auto saved=SaveDocument(api,document.get(),artifacts,label+writer->name());ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE("reader="+reader->name());const auto& read=reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(),nullptr);Handle after(read.FPDF_LoadPage(reopened.get(),0),read.FPDF_ClosePage);
      ASSERT_NE(after.get(),nullptr);const auto image=Paint(read,after.get(),surface);
      ASSERT_EQ(image.pixels.size(),static_cast<size_t>(kSide*kSide*4));
      ASSERT_NO_FATAL_FAILURE(inspect(image));
      CompareRaster(images[backend],image,artifacts,label+writer->name()+reader->name(),tolerance);
    }
    ++backend;
  }
  CompareRaster(images[0],images[1],artifacts,label+"-engines",tolerance);
}
std::string Group(const std::string& content,const std::string& resources,bool isolated,
    const std::string& bbox="[0 0 80 80]") {
  return PdfStream("/Type /XObject /Subtype /Form /BBox "+bbox+
      " /Group << /S /Transparency /CS /DeviceRGB /I "+std::string(isolated?"true":"false")+
      " >> /Resources << "+resources+" >>",content);
}
double BlendGray(const std::string& mode,double source,double back) {
  if(mode=="Normal")return source;
  if(mode=="Multiply")return source*back;
  if(mode=="Screen")return source+back-source*back;
  if(mode=="Difference")return std::abs(source-back);
  if(mode=="Exclusion")return source+back-2*source*back;
  ADD_FAILURE()<<mode;return 0;
}
RGBA GrayOver(int gray,double alpha,Surface surface,int back=255) {
  if(surface.transparent)return {gray,gray,gray,static_cast<int>(std::lround(alpha*255))};
  const auto value=static_cast<int>(std::lround(gray*alpha+back*(1-alpha)));
  return {value,value,value,255};
}

TEST(CoverageBackdropResampling, NestedNonisolatedGroupsReadBackCurrentForegroundAndBackdrop) {
  // An outer /ca group installs an AGG backdrop. Its nested nonnormal blend
  // enters ProcessTransparency again: get_dibits must clip the backdrop and
  // composite the current group plane, rather than reading either plane alone.
  for(const auto& surface:kSurfaces)for(bool isolated:{false,true})
    for(const std::string mode:{"Normal","Multiply","Screen","Difference","Exclusion"}) {
      const auto bytes=Scene(".5019608 g 0 0 100 100 re f q /A gs 1 0 0 1 10 10 cm /Outer Do Q",
        "/ExtGState << /A << /ca .5 >> >> /XObject << /Outer 5 0 R >>",
        {Group(".7529412 g 0 0 80 80 re f q /B gs /Inner Do Q",
          "/ExtGState << /B << /BM /"+mode+" >> >> /XObject << /Inner 6 0 R >>",isolated),
         Group(".2509804 g 20 20 40 40 re f","",false,"[20 20 60 60]")});
      const int blended=static_cast<int>(std::lround(BlendGray(mode,64./255,192./255)*255));
      const auto label="nested-backdrop-"+std::to_string(surface.format)+std::to_string(surface.transparent)+
          std::to_string(isolated)+mode;
      Workflow(bytes,label,surface,[=](const Raster& image){
        // The page itself supplies an opaque backdrop, including on BGRA0.
        Pixel(image,50,50,{(blended+128)/2,(blended+128)/2,(blended+128)/2,255},3);
        Pixel(image,20,20,{160,160,160,255},2);Pixel(image,95,95,{128,128,128,255},1);
      });
    }
}

TEST(CoverageBackdropResampling, OffsetNestedBlendKeepsSpatiallyDifferentBackdropRegions) {
  for(const std::string mode:{"Multiply","Screen","Difference"})for(bool reflected:{false,true}) {
    const auto bytes=Scene(".5019608 g 0 0 100 100 re f q /A gs "+
      std::string(reflected?"-1 0 0 1 90 10":"1 0 0 1 10 10")+" cm /Outer Do Q",
      "/ExtGState << /A << /ca .5 >> >> /XObject << /Outer 5 0 R >>",
      {Group(".2509804 g 0 0 40 80 re f .7529412 g 40 0 40 80 re f q /B gs /Inner Do Q",
        "/ExtGState << /B << /BM /"+mode+" >> >> /XObject << /Inner 6 0 R >>",false),
       Group(".2509804 g 45 20 25 40 re f","",false,"[45 20 70 60]")});
    const int inner=static_cast<int>(std::lround((BlendGray(mode,64./255,192./255)*255+128)/2));
    Workflow(bytes,"offset-backdrop-"+mode+std::to_string(reflected),{},[=](const Raster& image){
      const auto x=[=](int local){return reflected?89-local:10+local;};
      Pixel(image,x(55),50,{inner,inner,inner,255},3);
      Pixel(image,x(15),50,{96,96,96,255},2);Pixel(image,x(75),50,{160,160,160,255},2);
      Pixel(image,95,95,{128,128,128,255},1);
    });
  }
}

TEST(CoverageBackdropResampling, CombinedTranslucentFillStrokeKnocksOutFillAtStrokeInterior) {
  // B/B* routes translucent fill+stroke through DrawFillStrokePath's backdrop
  // snapshot and knockout renderer, independently of a PDF Group /K entry.
  for(const auto& surface:kSurfaces)for(const std::string op:{"B","B*"})for(int origin:{-8,0,12}) {
    const auto bytes=Scene(".5019608 g 0 0 100 100 re f q 1 0 0 1 "+std::to_string(origin)+
      " 0 cm /A gs 1 g 0 G 12 w 20 20 60 60 re "+op+" Q",
      "/ExtGState << /A << /ca .5 /CA .5 >> >>",{});
    Workflow(bytes,"fill-stroke-knockout-"+std::to_string(surface.format)+std::to_string(surface.transparent)+op+
      std::to_string(origin),surface,[=](const Raster& image){
      Pixel(image,50+origin,50,{191,191,191,255},2);
      Pixel(image,22+origin,50,{64,64,64,255},2);
      Pixel(image,16+origin,50,{64,64,64,255},2);
      Pixel(image,95,95,{128,128,128,255},1);
    });
  }
}

struct Samples {
  std::string name,space;int bits=8,components=1;bool mask=false;
};
const std::array<Samples,8> kSamples{{
  {"gray8","/DeviceGray"},{"rgb8","/DeviceRGB",8,3},
  {"indexed2","[/Indexed /DeviceRGB 3 <202020606060A0A0A0E0E0E0>]",2},
  {"indexed4","[/Indexed /DeviceRGB 3 <202020606060A0A0A0E0E0E0>]",4},
  {"gray16","/DeviceGray",16},{"rgb16","/DeviceRGB",16,3},
  {"rgb1","/DeviceRGB",1,3},{"stencil1","",1,1,true}}};
constexpr std::array<int,4> kGray{{32,96,160,224}};
int Quadrant(int x,int y,int width,int height){return (y>=height/2?2:0)+(x>=width/2?1:0);}
int Level(const Samples& sample,int quadrant) {
  return sample.bits==1?((quadrant==0||quadrant==3)?0:255):kGray[quadrant];
}
std::string Packed(const Samples& sample,int width,int height) {
  std::string out;
  for(int y=0;y<height;++y) {
    unsigned packed=0;int used=0;
    for(int x=0;x<width;++x) {
      const int quadrant=Quadrant(x,y,width,height);
      int value=sample.space.find("/Indexed")!=std::string::npos?quadrant:Level(sample,quadrant);
      if(sample.bits==1)value=value?1:0;
      if(sample.bits==16)value*=257;
      for(int channel=0;channel<sample.components;++channel)for(int bit=sample.bits-1;bit>=0;--bit) {
        packed=(packed<<1)|((value>>bit)&1);if(++used==8){out.push_back(static_cast<char>(packed));packed=0;used=0;}
      }
    }
    if(used)out.push_back(static_cast<char>(packed<<(8-used)));
  }
  return out;
}
std::string Image(const Samples& sample,int width,int height,bool interpolate,bool alpha=false) {
  return PdfStream("/Type /XObject /Subtype /Image /Width "+std::to_string(width)+" /Height "+
    std::to_string(height)+" /BitsPerComponent "+std::to_string(sample.bits)+
    (sample.mask?" /ImageMask true":" /ColorSpace "+sample.space)+
    " /Interpolate "+std::string(interpolate?"true":"false")+(alpha?" /SMask 6 0 R":""),Packed(sample,width,height));
}
std::vector<uint8_t> ImageScene(const Samples& sample,int width,int height,const Matrix& matrix,
    bool interpolate,bool clipped=false,bool alpha=false) {
  std::vector<std::string> assets{Image(sample,width,height,interpolate,alpha)};
  if(alpha) {
    Samples mask{"alpha","/DeviceGray"};
    assets.push_back(PdfStream("/Type /XObject /Subtype /Image /Width "+std::to_string(width)+" /Height "+
        std::to_string(height)+" /BitsPerComponent 8 /ColorSpace /DeviceGray",Packed(mask,width,height)));
  }
  return Scene("q "+std::string(clipped?"50 5 m 95 50 l 50 95 l 5 50 l h W n ":"")+"0 g "+matrix.Pdf()+" cm /Im Do Q",
      "/XObject << /Im 5 0 R >>",assets);
}
void Quadrants(const Raster& image,const Samples& sample,const Matrix& matrix,Surface surface,bool alpha=false) {
  for(double v:{.25,.75})for(double u:{.25,.75}) {
    const int quadrant=(v<.5?2:0)+(u>.5?1:0); // Image samples start at the top.
    const auto point=matrix.Map(u,v);
    const int x=static_cast<int>(std::floor(point[0]))+surface.x;
    const int y=static_cast<int>(std::floor(point[1]))-surface.y;
    if(x<0||x>=kSide||y<0||y>=kSide)continue;
    const auto expected=sample.mask?
      GrayOver(0,Level(sample,quadrant)==0?1:0,surface):
      GrayOver(Level(sample,quadrant),alpha?kGray[quadrant]/255.:1,surface);
    Pixel(image,x,y,expected,3);
  }
  Pixel(image,98,98,surface.transparent?RGBA{0,0,0,0}:RGBA{255,255,255,255},0);
}

TEST(CoverageBackdropResampling, OddPackedRowsResampleAcrossAxisAffineAndReflectedImageTransforms) {
  for(const auto& sample:kSamples)for(size_t transform=0;transform<kMatrices.size();++transform) {
    const auto matrix=kMatrices[transform];const bool interpolate=transform%2==0;
    Surface surface;surface.flags=transform%3==0?FPDF_RENDER_FORCEHALFTONE:
      transform%3==1?FPDF_RENDER_NO_SMOOTHIMAGE:0;
    Workflow(ImageScene(sample,17,15,matrix,interpolate),"resample-"+sample.name+std::to_string(transform),surface,
      [=](const Raster& image){Quadrants(image,sample,matrix,surface);});
  }
}

TEST(CoverageBackdropResampling, AntialiasedClipsAndNegativeViewportOriginsKeepResampledMasksAligned) {
  for(size_t kind:{size_t(0),size_t(2),size_t(3),size_t(7)})for(const auto& output:kSurfaces)
    for(bool offset:{false,true}) {
      const auto sample=kSamples[kind];Surface surface=output;
      if(offset){surface.x=-9;surface.y=-7;}
      const Matrix matrix=kMatrices[3];
      Workflow(ImageScene(sample,17,15,matrix,true,true),"clipped-resample-"+sample.name+
        std::to_string(surface.format)+std::to_string(surface.transparent)+std::to_string(offset),surface,
        [=](const Raster& image){Quadrants(image,sample,matrix,surface);});
    }
}

TEST(CoverageBackdropResampling, SmoothLargeDownsamplesPreserveAverageAndWideSourceContributions) {
  for(size_t kind:{size_t(0),size_t(1),size_t(2),size_t(4),size_t(5)})for(int size:{1,12})
    for(int flags:{0,FPDF_RENDER_FORCEHALFTONE}) {
      const auto sample=kSamples[kind];Surface surface;surface.flags=flags;
      Matrix matrix;matrix.a=size;matrix.d=size;matrix.e=50;matrix.f=50;
      Workflow(ImageScene(sample,128,128,matrix,false),"downsample-"+sample.name+
        std::to_string(size)+std::to_string(flags),surface,[=](const Raster& image){
          if(size==1)Pixel(image,50,50,{128,128,128,255},3);
          else Quadrants(image,sample,matrix,surface);
          Pixel(image,45,45,{255,255,255,255},0);
        });
    }
}

TEST(CoverageBackdropResampling, ResampledSoftMasksPreserveStraightColorsAndOutputAlphaAcrossSurfaces) {
  for(size_t kind:{size_t(0),size_t(1),size_t(2),size_t(5)})for(const auto& surface:kSurfaces) {
    const auto sample=kSamples[kind];const auto matrix=kMatrices[4];
    Workflow(ImageScene(sample,17,15,matrix,true,true,true),"soft-resample-"+sample.name+
      std::to_string(surface.format)+std::to_string(surface.transparent),surface,
      [=](const Raster& image){Quadrants(image,sample,matrix,surface,true);});
  }
}

TEST(CoverageBackdropResampling, InterpolatedColorImagesBlendThroughSoftMasksAndReverseByteDestinations) {
  const std::array<RGB,4> colors{{{192,64,128},{32,192,96},{64,96,224},{224,160,32}}};
  std::string samples;
  for(int y=0;y<15;++y)for(int x=0;x<17;++x)
    for(const int channel:colors[Quadrant(x,y,17,15)])samples.push_back(static_cast<char>(channel));
  const auto matrix=kMatrices[3];
  for(int format:{FPDFBitmap_BGR,FPDFBitmap_BGRx,FPDFBitmap_BGRA})for(bool reverse:{false,true})
    for(const std::string mode:{"Multiply","Screen","Difference","Exclusion"}) {
      Surface surface;surface.format=format;surface.reverse=reverse;
      const auto bytes=Scene(".5019608 g 0 0 100 100 re f q /A gs "+matrix.Pdf()+" cm /Im Do Q",
        "/ExtGState << /A << /ca .5 /BM /"+mode+" >> >> /XObject << /Im 5 0 R >>",
        {PdfStream("/Type /XObject /Subtype /Image /Width 17 /Height 15 /BitsPerComponent 8 /ColorSpace /DeviceRGB /Interpolate true /SMask 6 0 R",samples),
         PdfStream("/Type /XObject /Subtype /Image /Width 17 /Height 15 /BitsPerComponent 8 /ColorSpace /DeviceGray",std::string(17*15,'\x80'))});
      Workflow(bytes,"color-blend-resample-"+std::to_string(format)+std::to_string(reverse)+mode,surface,
        [=](const Raster& image){
          for(double v:{.25,.75})for(double u:{.25,.75}) {
            const auto point=matrix.Map(u,v);const auto& color=colors[(v<.5?2:0)+(u>.5?1:0)];
            RGBA expected{0,0,0,255};
            // The 128/255 soft mask and constant ca=.5 produce source alpha
            // 64/255 over the actual opaque PDF backdrop, before blending.
            for(size_t channel=0;channel<3;++channel)expected[channel]=static_cast<int>(std::lround(
              BlendGray(mode,color[channel]/255.,128./255)*64+128*(1-64./255)));
            Pixel(image,static_cast<int>(point[0]),static_cast<int>(point[1]),expected,3);
          }
          Pixel(image,95,95,{128,128,128,255},1);
        });
    }
}
}  // namespace
}  // namespace pdfoxium::integration
