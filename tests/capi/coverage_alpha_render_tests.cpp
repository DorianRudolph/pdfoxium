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
constexpr int kFlags=FPDF_RENDER_NO_SMOOTHIMAGE;
using RGB=std::array<int,3>;
using RGBA=std::array<int,4>;
struct Surface {int format=FPDFBitmap_BGRA; bool transparent=false;bool progressive=false;};
const std::array<Surface,5> kSurfaces{{{FPDFBitmap_Gray,false},{FPDFBitmap_BGR,false},
    {FPDFBitmap_BGRx,false},{FPDFBitmap_BGRA,false},{FPDFBitmap_BGRA,true}}};
struct Matrix {
  double a=60,b=0,c=0,d=60,e=20,f=20;
  std::string Pdf() const {std::ostringstream out;out<<a<<' '<<b<<' '<<c<<' '<<d<<' '<<e<<' '<<f;return out.str();}
  std::array<double,2> Inverse(double x,double y) const {
    const double determinant=a*d-b*c;
    return {(d*(x-e)-c*(y-f))/determinant,(-b*(x-e)+a*(y-f))/determinant};
  }
};
const std::array<Matrix,5> kMatrices{{{}, {-60,0,0,60,80,20},{0,60,-60,0,80,20},
    {48,16,-16,48,34,18},{48,0,16,48,18,24}}};
std::string AlphaState(double alpha) {return "<< /Type /ExtGState /ca "+std::to_string(alpha)+" /CA "+std::to_string(alpha)+" >>";}
std::string Image(const std::string& dictionary,const std::string& samples) {
  return PdfStream("/Type /XObject /Subtype /Image /Width 4 /Height 1 "+dictionary,samples);
}
std::string UnitImage(const Matrix& matrix) {return "q "+matrix.Pdf()+" cm /Im Do Q\n";}
std::string UnitRects(const Matrix& matrix,bool striped) {
  return "q "+matrix.Pdf()+" cm "+(striped?"0 0 .25 1 re .5 0 .25 1 re":"0 0 1 1 re")+" f Q\n";
}
std::vector<uint8_t> MaskPdf(const std::string& content,const std::string& resources,
    const std::vector<std::string>& assets, bool transparent=false,
    const std::string& transfer="",double paint_alpha=1) {
  // XFA-free minimal PDF structures: public C API has no constructor for
  // transparency-group soft masks or image color-key dictionaries.
  std::vector<std::string> objects{
    "<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /ExtGState << /Mask 5 0 R >> >> /Contents 4 0 R >>",
    PdfStream("",std::string(transparent?"":".25 .5 .75 rg 0 0 100 100 re f\n")+
        "q /Mask gs 1 0 0 rg 10 10 80 80 re f Q\n0 0 1 rg 2 2 4 4 re f\n"),
    "<< /Type /ExtGState /ca "+std::to_string(paint_alpha)+" /SMask << /S /Alpha /G 6 0 R "+transfer+" >> >>",
    PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Group << /S /Transparency /CS /DeviceRGB /I true >> /Resources << "+resources+" >>",content)};
  objects.insert(objects.end(),assets.begin(),assets.end());return PdfFixture(objects);
}
Raster Paint(const Api& api,FPDF_PAGE page,Surface surface) {
  Handle bitmap(api.FPDFBitmap_CreateEx(kSide,kSide,surface.format,nullptr,0),api.FPDFBitmap_Destroy);
  if(!bitmap.get()){ADD_FAILURE();return {};}
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,kSide,kSide,surface.transparent?0:0xffffffff));
  if(surface.progressive){
    IFSDK_PAUSE pause{};pause.version=1;pause.NeedToPauseNow=[](IFSDK_PAUSE*)->FPDF_BOOL{return 1;};
    int status=api.FPDF_RenderPageBitmap_Start(bitmap.get(),page,0,0,kSide,kSide,0,kFlags | FPDF_NO_NATIVETEXT,&pause);
    int iterations=0;
    while(status==FPDF_RENDER_TOBECONTINUED&&iterations++<1000)status=api.FPDF_RenderPage_Continue(page,&pause);
    EXPECT_EQ(status,FPDF_RENDER_DONE);api.FPDF_RenderPage_Close(page);
  }else api.FPDF_RenderPageBitmap(bitmap.get(),page,0,0,kSide,kSide,0,kFlags | FPDF_NO_NATIVETEXT);
  const int stride=api.FPDFBitmap_GetStride(bitmap.get());
  const auto* buffer=static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
  const int step=surface.format==FPDFBitmap_Gray?1:surface.format==FPDFBitmap_BGR?3:4;
  Raster image{kSide,kSide,{}};
  for(int y=0;y<kSide;++y)for(int x=0;x<kSide;++x){const auto* p=buffer+y*stride+x*step;
    if(step==1)image.pixels.insert(image.pixels.end(),{p[0],p[0],p[0],255});
    else image.pixels.insert(image.pixels.end(),{p[0],p[1],p[2],static_cast<uint8_t>(surface.format==FPDFBitmap_BGRA?p[3]:255)});
  }
  return image;
}
RGBA At(const Raster& image,int x,int y){const size_t p=((kSide-1-y)*kSide+x)*4;return {image.pixels[p+2],image.pixels[p+1],image.pixels[p],image.pixels[p+3]};}
RGBA Expected(double coverage,Surface surface) {
  const int alpha=std::clamp(static_cast<int>(std::lround(coverage*255)),0,255);
  if(surface.transparent)return alpha?RGBA{255,0,0,alpha}:RGBA{0,0,0,0};
  RGB backdrop{64,128,191},source{255,0,0};
  if(surface.format==FPDFBitmap_Gray){const int gray=(64*30+128*59+191*11)/100;backdrop={gray,gray,gray};source={76,76,76};}
  RGBA result{};for(int c=0;c<3;++c)result[c]=(backdrop[c]*(255-alpha)+source[c]*alpha)/255;result[3]=255;return result;
}
int PixelError(const Raster& image,int x,int y,RGBA expected){
  const auto actual=At(image,x,y);int error=std::abs(actual[3]-expected[3]);
  // Straight RGB is unspecified when alpha is zero. Its alpha still must be
  // zero, and complete native/reader raster comparisons remain independent.
  if(expected[3])for(int c=0;c<3;++c)error=std::max(error,std::abs(actual[c]-expected[c]));
  return error;
}
void Pixel(const Raster& image,int x,int y,RGBA expected,int tolerance=2){EXPECT_LE(PixelError(image,x,y,expected),tolerance)<<x<<','<<y;}
using Inspect=std::function<void(const Raster&,Surface)>;
void Guards(const Raster& image,Surface surface){Pixel(image,3,3,surface.format==FPDFBitmap_Gray?RGBA{28,28,28,255}:RGBA{0,0,255,255},1);Pixel(image,96,96,Expected(0,surface),1);}
Raster RenderBytes(const Api& api,const std::vector<uint8_t>& bytes,Surface surface){
  Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
  if(!doc.get()){ADD_FAILURE();return {};}
  Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);if(!page.get()){ADD_FAILURE();return {};}
  return Paint(api,page.get(),surface);
}
// Native StartBitmapAlpha encodes constant opacity in RGB with alpha255;
// EightBppMask consumes alpha, making direct images opaque. Its cached
// uncolored vector-pattern path instead applies inherited opacity twice.
// native_inspect independently predicts those exact observations. Rust keeps
// the correct opacity/product/union assertions and equivalent vector controls;
// reloads compare each reader to that reader's original, and pixels outside
// the affected paint rectangle remain strict native/Rust comparisons.
void Workflow(const std::vector<uint8_t>& bytes,const std::vector<uint8_t>& vector_control,
    const std::string& name,Surface surface,const Inspect& inspect,bool save=true,const Inspect& native_inspect={}) {
  Artifacts artifacts;std::array<Raster,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()){
    SCOPED_TRACE(name+" writer="+writer->name()+" format="+std::to_string(surface.format)+" transparent="+std::to_string(surface.transparent));
    const auto& api=writer->api();Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    originals[backend]=Paint(api,page.get(),surface);Guards(originals[backend],surface);
    (writer->name()=="native"&&native_inspect?native_inspect:inspect)(originals[backend],surface);
    if(!vector_control.empty()){
      SCOPED_TRACE("equivalent vector control");const auto control=RenderBytes(api,vector_control,surface);Guards(control,surface);inspect(control,surface);
      // The same independent sample points verify image and vector coverage.
      // Affine mask interpolation and vector edge coverage are different
      // algorithms, so edge pixels are compared across engines separately.
    }
    CompareRaster(originals[backend],Paint(api,page.get(),surface),artifacts,name+"-cached",0);
    if(save){const auto saved=SaveDocument(api,doc.get(),artifacts,name+"-"+writer->name());ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()){SCOPED_TRACE("reader="+reader->name());const auto raster=RenderBytes(reader->api(),saved,surface);Guards(raster,surface);
        (reader->name()=="native"&&native_inspect?native_inspect:inspect)(raster,surface);
        const auto reference=native_inspect&&reader!=writer?RenderBytes(reader->api(),bytes,surface):originals[backend];
        CompareRaster(reference,raster,artifacts,name+"-reload-"+reader->name(),1);}}
    ++backend;
  }
  if(!native_inspect)CompareRaster(originals[0],originals[1],artifacts,name+"-native-rust",1);
  else for(const auto& rect:std::array<std::array<int,4>,4>{{{0,0,100,10},{0,90,100,10},{0,10,10,80},{90,10,10,80}}})
    CompareRaster(CropRaster(originals[0],rect[0],rect[1],rect[2],rect[3]),CropRaster(originals[1],rect[0],rect[1],rect[2],rect[3]),artifacts,name+"-outside-alpha-paint",1);
}
Inspect Geometry(Matrix matrix,bool striped,double alpha,double paint_alpha=1){return [=](const Raster& image,Surface surface){
  int worst=0,wrong=0;
  for(int y=12;y<89;y+=7)for(int x=12;x<89;x+=7){const auto uv=matrix.Inverse(x+.5,y+.5);
    const double scale=std::min(std::hypot(matrix.a,matrix.b),std::hypot(matrix.c,matrix.d));
    bool edge=std::min({std::abs(uv[0]),std::abs(uv[0]-1),std::abs(uv[1]),std::abs(uv[1]-1)})*scale<3;
    if(striped)for(double u:{.25,.5,.75})edge|=std::abs(uv[0]-u)*scale<3;
    if(edge)continue;
    const bool inside=uv[0]>0&&uv[0]<1&&uv[1]>0&&uv[1]<1;
    const bool painted=inside&&(!striped||uv[0]<.25||(uv[0]>.5&&uv[0]<.75));
    const int error=PixelError(image,x,y,Expected(painted?alpha*paint_alpha:0,surface));
    worst=std::max(worst,error);wrong+=error>2;
  }
  EXPECT_LE(worst,2)<<wrong<<" independent geometric sample points differ";
};}
const std::string kImageResources="/XObject << /Im 7 0 R >> /ExtGState << /A 8 0 R >>";

TEST(CoverageAlphaRender, OpaqueImageColorIsIgnoredWhileMaskAndPaintingAlphaRemainIndependent) {
  const std::array<std::pair<std::string,std::string>,3> images{{
    {"/BitsPerComponent 8 /ColorSpace /DeviceRGB",std::string("\xff\x00\x00\x00\xff\x00\x00\x00\xff\x00\x00\x00",12)},
    {"/BitsPerComponent 8 /ColorSpace /DeviceGray",std::string("\x00\x40\x80\xff",4)},
    {"/BitsPerComponent 2 /ColorSpace [/Indexed /DeviceRGB 3 <000000FF000000FF000000FF>]",std::string(1,'\x1b')}}};
  for(size_t source=0;source<images.size();++source)for(double alpha:{0.,.5,1.})for(double paint:{.5,1.}){
    const std::vector<std::string> assets{Image(images[source].first,images[source].second),AlphaState(alpha)};
    for(Surface surface:{Surface{},Surface{FPDFBitmap_BGRA,true}}){
      const auto bytes=MaskPdf("/A gs "+UnitImage({}),kImageResources,assets,surface.transparent,"",paint);
      const auto control=MaskPdf("/A gs "+UnitRects({},false),kImageResources,assets,surface.transparent,"",paint);
      Workflow(bytes,control,"opaque-alpha-"+std::to_string(source)+"-"+std::to_string(alpha)+"-"+std::to_string(paint),surface,Geometry({},false,alpha,paint),source==0,alpha==1?Inspect{}:Geometry({},false,1,paint));
    }
  }
}
TEST(CoverageAlphaRender, BitMaskDecodeAndAxisReflectedRotatedAndShearedTransformsMatchVectorCoverage) {
  for(size_t transform=0;transform<kMatrices.size();++transform)for(bool invert:{false,true})for(double alpha:{.5,1.}){
    const auto matrix=kMatrices[transform];
    const std::vector<std::string> assets{Image("/ImageMask true /BitsPerComponent 1 /Decode "+std::string(invert?"[1 0]":"[0 1]"),std::string(1,invert?'\xa0':'\x50')),AlphaState(alpha)};
    for(Surface surface:{Surface{},Surface{FPDFBitmap_Gray,false}}){
      const auto bytes=MaskPdf("/A gs "+UnitImage(matrix),kImageResources,assets,surface.transparent);
      const auto control=MaskPdf("/A gs "+UnitRects(matrix,true),kImageResources,assets,surface.transparent);
      Workflow(bytes,control,"bit-alpha-"+std::to_string(transform)+"-"+std::to_string(invert)+"-"+std::to_string(alpha),surface,Geometry(matrix,true,alpha),transform<2,alpha==1?Inspect{}:Geometry(matrix,true,1));
    }
  }
}
TEST(CoverageAlphaRender, IndexedAndRgbColorKeysSupplyExtractedAlphaInsteadOfTheirColorChannels) {
  struct Source{std::string dictionary,samples;};
  const std::vector<Source> sources{
    {"/BitsPerComponent 1 /ColorSpace [/Indexed /DeviceRGB 1 <00FF00FF0000>] /Mask [1 1]",std::string(1,'\x50')},
    {"/BitsPerComponent 2 /ColorSpace [/Indexed /DeviceRGB 3 <00FF00FF00000000FFFFFFFF>] /Mask [1 3]",std::string(1,'\x11')},
    {"/BitsPerComponent 8 /ColorSpace /DeviceGray /Mask [255 255]",std::string("\x00\xff\x00\xff",4)},
    {"/BitsPerComponent 8 /ColorSpace /DeviceRGB /Mask [255 255 0 0 0 0]",std::string("\x00\xff\x00\xff\x00\x00\x00\xff\x00\xff\x00\x00",12)}};
  for(size_t source=0;source<sources.size();++source)for(size_t transform:{size_t{0},size_t{3}}){
    const auto matrix=kMatrices[transform];const std::vector<std::string> assets{Image(sources[source].dictionary,sources[source].samples),AlphaState(.5)};
    for(Surface surface:{Surface{},Surface{FPDFBitmap_BGRA,true}})
      Workflow(MaskPdf("/A gs "+UnitImage(matrix),kImageResources,assets,surface.transparent),MaskPdf("/A gs "+UnitRects(matrix,true),kImageResources,assets,surface.transparent),
        "color-key-alpha-"+std::to_string(source)+"-"+std::to_string(transform),surface,Geometry(matrix,true,.5),transform==0,Geometry(matrix,true,1));
  }
}
TEST(CoverageAlphaRender, OverlappingImageMaskObjectsAccumulateCoverageAndKeepNonoverlapDistinct) {
  for(double alpha:{.5,1.})for(Surface surface:kSurfaces){
    const Matrix first{40,0,0,60,20,20},second{40,0,0,60,40,20};
    const std::vector<std::string> assets{Image("/BitsPerComponent 8 /ColorSpace /DeviceRGB",std::string(12,'\0')),AlphaState(alpha)};
    const auto inspect=[=](const Raster& image,Surface output){
      Pixel(image,10,50,Expected(0,output));Pixel(image,30,50,Expected(alpha,output));
      Pixel(image,50,50,Expected(alpha+alpha-alpha*alpha,output));Pixel(image,70,50,Expected(alpha,output));};
    Workflow(MaskPdf("/A gs "+UnitImage(first)+UnitImage(second),kImageResources,assets,surface.transparent),
      MaskPdf("/A gs "+UnitRects(first,false)+UnitRects(second,false),kImageResources,assets,surface.transparent),"overlap-alpha-"+std::to_string(alpha),surface,inspect,true,alpha==1?Inspect{}:Inspect{[](const Raster& image,Surface output){
        Pixel(image,10,50,Expected(0,output));for(int x:{30,50,70})Pixel(image,x,50,Expected(1,output));}});
  }
}
TEST(CoverageAlphaRender, AxialAndRadialShadingContributeAlphaIndependentOfTheirRgbGradient) {
  for(bool radial:{false,true})for(double alpha:{.5,1.})for(Surface surface:kSurfaces){
    const std::string shader="<< /ShadingType "+std::to_string(radial?3:2)+" /ColorSpace /DeviceRGB /Coords "+(radial?"[50 50 0 50 50 30]":"[20 20 80 20]")+" /Extend [true true] /Function 9 0 R >>";
    const std::vector<std::string> assets{shader,AlphaState(alpha),"<< /FunctionType 2 /Domain [0 1] /C0 [0 0 0] /C1 [0 1 0] /N 1 >>"};
    const std::string resources="/Shading << /Sh 7 0 R >> /ExtGState << /A 8 0 R >>";
    Workflow(MaskPdf("/A gs 20 20 60 60 re W n /Sh sh",resources,assets,surface.transparent),
      MaskPdf("/A gs "+UnitRects({},false),resources,assets,surface.transparent),"shading-alpha-"+std::to_string(radial)+"-"+std::to_string(alpha),surface,Geometry({},false,alpha));
  }
}
TEST(CoverageAlphaRender, TransferFunctionsMapMaskCoverageIncludingTransparentBackgroundSamples) {
  for(bool invert:{false,true})for(double alpha:{0.,.5,1.}){
    const std::vector<std::string> assets{Image("/BitsPerComponent 8 /ColorSpace /DeviceRGB",std::string(12,'\0')),AlphaState(alpha)};
    const std::string transfer="/TR << /FunctionType 2 /Domain [0 1] /C0 ["+std::to_string(invert?1:0)+"] /C1 ["+std::to_string(invert?0:1)+"] /N "+std::string(invert?"1":"2")+" >>";
    const double inside=invert?1-alpha:alpha*alpha,outside=invert?1:0;
    const auto inspect=[=](const Raster& image,Surface surface){Pixel(image,30,50,Expected(inside,surface));Pixel(image,15,50,Expected(outside,surface));Pixel(image,85,50,Expected(outside,surface));};
    Workflow(MaskPdf("/A gs "+UnitImage({}),kImageResources,assets,false,transfer),
      MaskPdf("/A gs "+UnitRects({},false),kImageResources,assets,false,transfer),"transfer-alpha-"+std::to_string(invert)+"-"+std::to_string(alpha),{},inspect,true,alpha==1?Inspect{}:Inspect{[=](const Raster& image,Surface output){
        Pixel(image,30,50,Expected(invert?0:1,output));for(int x:{15,85})Pixel(image,x,50,Expected(outside,output));}});
  }
}
TEST(CoverageAlphaRender, NestedFormImagesPreserveAlphaModeAcrossResourcesAndGroupIsolation) {
  for(bool isolated:{false,true})for(double alpha:{.5,1.}){
    const std::string nested=PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 1 1] /Group << /S /Transparency /CS /DeviceRGB /I "+std::string(isolated?"true":"false")+" >> /Resources << /XObject << /Leaf 9 0 R >> >>","/Leaf Do");
    const std::vector<std::string> assets{nested,AlphaState(alpha),Image("/ImageMask true /BitsPerComponent 1",std::string(1,'\x50'))};
    Workflow(MaskPdf("/A gs "+UnitImage({}),kImageResources,assets),MaskPdf("/A gs "+UnitRects({},true),kImageResources,assets),
      "nested-alpha-"+std::to_string(isolated)+"-"+std::to_string(alpha),{},Geometry({},true,alpha));
  }
}
TEST(CoverageAlphaRender, UncoloredTilingCellsUseImageMaskCoverageAndCallerColorWithoutInventingCellColors) {
  for(bool vector:{false,true})for(bool stroke:{false,true})for(int tile:{10,120})for(bool unequal:{false,true})for(double alpha:{.5,1.}){
    const double ca=unequal?(alpha==.5?.25:.75):alpha;
    const double cap=unequal?(alpha==.5?.75:.25):alpha;
    const double effective=stroke?cap:ca;
    const std::string size=std::to_string(tile),quarter=std::to_string(tile/4.),half=std::to_string(tile/2.);
    const std::string cell=vector?"0 0 "+quarter+" "+size+" re "+half+" 0 "+quarter+" "+size+" re f":
        "q "+size+" 0 0 "+size+" 0 0 cm /Bits Do Q";
    const auto bytes=PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /Pattern << /Tile 7 0 R >> /ColorSpace << /PCS [/Pattern /DeviceRGB] >> /ExtGState << /A 8 0 R >> >> /Contents 4 0 R >>",
      PdfStream("",".25 .5 .75 rg 0 0 100 100 re f q /A gs "+std::string(stroke?
        "/PCS CS 1 0 0 /Tile SCN 60 w 20 50 m 80 50 l S":"/PCS cs 1 0 0 /Tile scn 20 20 60 60 re f")+" Q 0 0 1 rg 2 2 4 4 re f"),
      "null","null",PdfStream("/Type /Pattern /PatternType 1 /PaintType 2 /TilingType 1 /BBox [0 0 "+size+" "+size+"] /XStep "+size+" /YStep "+size+" /Resources << /XObject << /Bits 9 0 R >> >>",cell),
      "<< /Type /ExtGState /ca "+std::to_string(ca)+" /CA "+std::to_string(cap)+" >>",Image("/ImageMask true /BitsPerComponent 1",std::string(1,'\x50'))});
    const auto samples=[=](double opacity){return Inspect{[=](const Raster& image,Surface surface){
      int worst=0;
      for(int y:{25,45,65})for(int x:{21,23,26,28,31,33,36,38,71,73,76,78}){
        const double phase=std::fmod(x+.5,tile)/tile;
        const double coverage=phase<.25||(phase>=.5&&phase<.75)?opacity:0;
        worst=std::max(worst,PixelError(image,x,y,Expected(coverage,surface)));
      }EXPECT_LE(worst,2);
    }};};
    Inspect native;
    if(tile==10&&stroke)native=samples(0); // Native colors cached strokes with the existing fill/backdrop color.
    else if(tile==10&&vector&&ca!=1)native=samples(ca*ca);
    else if(tile==120&&!vector&&ca!=1)native=samples(ca*ca);
    else if(tile==120&&stroke&&unequal)native=samples(ca);
    Workflow(bytes,{},"tiling-mask-alpha-"+std::to_string(vector)+"-"+std::to_string(stroke)+"-"+size+"-"+std::to_string(unequal)+"-"+std::to_string(alpha),{},samples(effective),true,native);
  }
}
TEST(CoverageAlphaRender, StandaloneStencilOpacityIsAppliedOnceInStaticAndProgressiveRendering) {
  for(double alpha:{.25,.5,.75,1.})for(bool progressive:{false,true})for(bool transparent:{false,true}){
    const auto scene=[&](bool vector){return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /XObject << /Im 5 0 R >> /ExtGState << /A 6 0 R >> >> /Contents 4 0 R >>",
      PdfStream("",std::string(transparent?"":".25 .5 .75 rg 0 0 100 100 re f")+
        " q /A gs 1 0 0 rg "+(vector?UnitRects({},true):UnitImage({}))+" Q 0 0 1 rg 2 2 4 4 re f"),
      Image("/ImageMask true /BitsPerComponent 1",std::string(1,'\x50')),AlphaState(alpha)});};
    Workflow(scene(false),scene(true),"standalone-stencil-"+std::to_string(alpha)+"-"+std::to_string(progressive)+"-"+std::to_string(transparent),
      {FPDFBitmap_BGRA,transparent,progressive},Geometry({},true,alpha),true,alpha==1?Inspect{}:Geometry({},true,alpha*alpha));
  }
}
TEST(CoverageAlphaRender, OneUncoloredPatternIsReusedAtDifferentOpacitiesWithoutRetainingAnotherObjectsAlpha) {
  for(bool vector:{false,true}){
    const auto bytes=PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /Pattern << /Tile 7 0 R >> /ColorSpace << /PCS [/Pattern /DeviceRGB] >> /ExtGState << /A 8 0 R /B 10 0 R >> >> /Contents 4 0 R >>",
      PdfStream("",".25 .5 .75 rg 0 0 100 100 re f /PCS cs 1 0 0 /Tile scn q /A gs 20 20 25 60 re f Q q /B gs 55 20 25 60 re f Q 0 0 1 rg 2 2 4 4 re f"),
      "null","null",PdfStream("/Type /Pattern /PatternType 1 /PaintType 2 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10 /Resources << /XObject << /Bits 9 0 R >> >>",
        vector?"0 0 2.5 10 re 5 0 2.5 10 re f":"q 10 0 0 10 0 0 cm /Bits Do Q"),
      AlphaState(.25),Image("/ImageMask true /BitsPerComponent 1",std::string(1,'\x50')),AlphaState(.75)});
    const auto inspect=[](double first,double second){return Inspect{[=](const Raster& image,Surface surface){
      for(int y:{25,45,65}){for(int x:{21,26,31,36})Pixel(image,x,y,Expected(first,surface));for(int x:{56,61,66,71,76})Pixel(image,x,y,Expected(second,surface));}
    }};};
    Workflow(bytes,{},"reuse-pattern-opacity-"+std::to_string(vector),{},inspect(.25,.75),true,vector?inspect(.25*.25,.75*.75):Inspect{});
  }
}
}  // namespace
}  // namespace pdfoxium::integration
