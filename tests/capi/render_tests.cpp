#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

constexpr int kPageWidth = 120, kPageHeight = 100;
constexpr int kCanvasWidth = 240, kCanvasHeight = 220;
constexpr int kCrisp = FPDF_RENDER_NO_SMOOTHIMAGE | FPDF_RENDER_NO_SMOOTHPATH;
using Pixel = std::array<uint8_t, 4>;
constexpr Pixel kWhite{255,255,255,255};

Pixel GetPixel(const Raster& raster, int x, int y) {
  EXPECT_GE(x,0);EXPECT_LT(x,raster.width);EXPECT_GE(y,0);EXPECT_LT(y,raster.height);
  if(x<0 || y<0 || x>=raster.width || y>=raster.height) return {};
  const auto offset=(static_cast<size_t>(y)*raster.width+x)*4;
  return {raster.pixels[offset],raster.pixels[offset+1],raster.pixels[offset+2],raster.pixels[offset+3]};
}

Raster CopyBitmap(const Api& api, FPDF_BITMAP bitmap) {
  const int width=api.FPDFBitmap_GetWidth(bitmap),height=api.FPDFBitmap_GetHeight(bitmap);
  const int format=api.FPDFBitmap_GetFormat(bitmap),stride=api.FPDFBitmap_GetStride(bitmap);
  const int bpp=format==FPDFBitmap_Gray?1:format==FPDFBitmap_BGR?3:4;
  const auto* data=static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap));
  EXPECT_NE(data,nullptr);EXPECT_GT(width,0);EXPECT_GT(height,0);EXPECT_GE(stride,width*bpp);
  if(!data || width<=0 || height<=0 || stride<width*bpp) return {};
  Raster raster{width,height,{}};raster.pixels.reserve(static_cast<size_t>(width)*height*4);
  for(int y=0;y<height;++y) for(int x=0;x<width;++x) {
    const auto* p=data+static_cast<size_t>(y)*stride+x*bpp;
    if(format==FPDFBitmap_Gray) raster.pixels.insert(raster.pixels.end(),{p[0],p[0],p[0],255});
    else raster.pixels.insert(raster.pixels.end(),{p[0],p[1],p[2],static_cast<uint8_t>(format==FPDFBitmap_BGRA?p[3]:255)});
  }
  return raster;
}

void AddRectangle(const Api& api,FPDF_PAGE page,float x,float y,float width,float height,
                  unsigned int red,unsigned int green,unsigned int blue,unsigned int alpha=255) {
  Handle object(api.FPDFPageObj_CreateNewRect(x,y,width,height),api.FPDFPageObj_Destroy);
  ASSERT_NE(object.get(),nullptr);
  ASSERT_TRUE(api.FPDFPageObj_SetFillColor(object.get(),red,green,blue,alpha));
  ASSERT_TRUE(api.FPDFPath_SetDrawMode(object.get(),FPDF_FILLMODE_WINDING,0));
  api.FPDFPage_InsertObject(page,object.release());
}

void BuildScene(const Api& api,FPDF_DOCUMENT document,FPDF_PAGE page) {
  ASSERT_NO_FATAL_FAILURE(AddRectangle(api,page,10,10,24,20,255,0,0));
  ASSERT_NO_FATAL_FAILURE(AddRectangle(api,page,50,20,20,24,0,255,0));
  ASSERT_NO_FATAL_FAILURE(AddRectangle(api,page,20,60,24,20,0,0,255));
  ASSERT_NO_FATAL_FAILURE(AddRectangle(api,page,40,64,20,20,255,128,0,128));
  // A fractional triangle makes the path smoothing switch observable.
  Handle triangle(api.FPDFPageObj_CreateNewPath(72.25f,10.25f),api.FPDFPageObj_Destroy);
  ASSERT_NE(triangle.get(),nullptr);
  ASSERT_TRUE(api.FPDFPath_LineTo(triangle.get(),85.75f,26.5f));
  ASSERT_TRUE(api.FPDFPath_LineTo(triangle.get(),71.25f,27.75f));
  ASSERT_TRUE(api.FPDFPath_Close(triangle.get()));
  ASSERT_TRUE(api.FPDFPageObj_SetFillColor(triangle.get(),255,128,0,255));
  ASSERT_TRUE(api.FPDFPath_SetDrawMode(triangle.get(),FPDF_FILLMODE_WINDING,0));
  api.FPDFPage_InsertObject(page,triangle.release());

  Handle bitmap(api.FPDFBitmap_Create(4,4,1),api.FPDFBitmap_Destroy);
  ASSERT_NE(bitmap.get(),nullptr);
  ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,2,2,0xff00ffff));
  ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),2,0,2,2,0xffff00ff));
  ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,2,2,2,0xffffff00));
  ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),2,2,2,2,0xff000000));
  Handle image(api.FPDFPageObj_NewImageObj(document),api.FPDFPageObj_Destroy);
  ASSERT_NE(image.get(),nullptr);
  ASSERT_TRUE(api.FPDFImageObj_SetBitmap(nullptr,0,image.get(),bitmap.get()));
  ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image.get(),20,0,0,20,80,60));
  api.FPDFPage_InsertObject(page,image.release());

  // Public constructors support ordinary stamps and popups. The latter must
  // remain excluded from page rendering even when FPDF_ANNOT is enabled.
  for(int subtype:{FPDF_ANNOT_STAMP,FPDF_ANNOT_POPUP}) {
    Handle annotation(api.FPDFPage_CreateAnnot(page,subtype),api.FPDFPage_CloseAnnot);
    ASSERT_NE(annotation.get(),nullptr);
    const FS_RECTF rect=subtype==FPDF_ANNOT_STAMP?FS_RECTF{92,45,112,30}:FS_RECTF{90,18,110,6};
    ASSERT_TRUE(api.FPDFAnnot_SetRect(annotation.get(),&rect));
    ASSERT_TRUE(api.FPDFAnnot_SetFlags(annotation.get(),FPDF_ANNOT_FLAG_PRINT));
    const std::u16string appearance=subtype==FPDF_ANNOT_STAMP?
        u"1 0 1 rg 0 0 120 100 re f":u"0 1 1 rg 0 0 120 100 re f";
    ASSERT_TRUE(api.FPDFAnnot_SetAP(annotation.get(),FPDF_ANNOT_APPEARANCEMODE_NORMAL,
        reinterpret_cast<const FPDF_WCHAR*>(appearance.c_str())));
  }
  ASSERT_TRUE(api.FPDFPage_GenerateContent(page));
}

struct MatrixCase {
  const char* name;
  FS_MATRIX matrix;
  // Equivalent FPDF_RenderPageBitmap viewport, when one exists.
  bool ordinary;
  int left,top,width,height,rotation;
};
const std::array<MatrixCase,11> kMatrices={{
    {"identity",{1,0,0,1,0,0},true,0,0,120,100,0},
    {"translation",{1,0,0,1,30,40},true,30,40,120,100,0},
    {"scale",{1.5f,0,0,1.5f,4,6},true,4,6,180,150,0},
    {"nonuniform-scale",{0.5f,0,0,1.75f,8,8},true,8,8,60,175,0},
    {"negative-translation",{1,0,0,1,-15,-8},true,-15,-8,120,100,0},
    {"shear",{1,0.25f,0.25f,1,8,9},false,0,0,0,0,0},
    {"rotation90",{0,1,-1,0,160,12},true,60,12,100,120,1},
    {"rotation180",{-1,0,0,-1,180,150},true,60,50,120,100,2},
    {"rotation270",{0,-1,1,0,12,170},true,12,50,100,120,3},
    // cos(37 degrees), sin(37 degrees); translation keeps the page on canvas.
    {"rotation37",{0.79863551f,0.60181502f,-0.60181502f,0.79863551f,80,12},false,0,0,0,0,0},
    {"reflection",{-1,0,0,1,180,10},false,0,0,0,0,0}}};

Raster RenderMatrix(const Api& api,FPDF_PAGE page,const FS_MATRIX& matrix,
                    const FS_RECTF& clip,int flags=kCrisp,FPDF_DWORD background=0xffffffff) {
  Handle bitmap(api.FPDFBitmap_Create(kCanvasWidth,kCanvasHeight,1),api.FPDFBitmap_Destroy);
  EXPECT_NE(bitmap.get(),nullptr);if(!bitmap.get()) return {};
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,kCanvasWidth,kCanvasHeight,background));
  api.FPDF_RenderPageBitmapWithMatrix(bitmap.get(),page,&matrix,&clip,flags | FPDF_NO_NATIVETEXT);
  return CopyBitmap(api,bitmap.get());
}
const FS_RECTF kFullClip{0,kCanvasHeight,kCanvasWidth,0};

// Public matrix rendering first applies the page's normal display matrix
// (x,y)->(x,page_height-y), then applies the caller's matrix in device space.
void ExpectLandmarks(const Raster& raster,const FS_MATRIX& m,bool gray=false,bool reverse=false) {
  struct Landmark {float x,y;Pixel color;};
  const std::array<Landmark,7> landmarks={{{22,20,{0,0,255,255}},
      {60,32,{0,255,0,255}},{30,70,{255,0,0,255}},
      {85,75,{255,255,0,255}},{95,75,{255,0,255,255}},
      {85,65,{0,255,255,255}},{95,65,{0,0,0,255}}}};
  for(const auto& landmark:landmarks) {
    const float dx=landmark.x,dy=kPageHeight-landmark.y;
    const int x=static_cast<int>(std::floor(m.a*dx+m.c*dy+m.e));
    const int y=static_cast<int>(std::floor(m.b*dx+m.d*dy+m.f));
    Pixel expected=landmark.color;
    if(gray) {
      const auto value=static_cast<uint8_t>((expected[0]*11+expected[1]*59+expected[2]*30)/100);
      expected={value,value,value,255};
    }
    if(reverse) std::swap(expected[0],expected[2]);
    const auto actual=GetPixel(raster,x,y);
    if(landmark.x>=80 && (m.a*m.b!=0 || m.c*m.d!=0)) {
      // The affine image resampler quantizes its 8-bit interpolation weights.
      // Both engines give 253/2 for these nominal 255/0 interiors under shear.
      // This local color assertion allows that two-level quantization; all
      // full-scene cross-engine and saved/reloaded raster comparisons are exact.
      for(size_t channel=0;channel<3;++channel)
        EXPECT_NEAR(actual[channel],expected[channel],2)<<"image landmark "<<landmark.x<<","<<landmark.y;
      EXPECT_EQ(actual[3],expected[3]);
    } else {
      EXPECT_EQ(actual,expected)<<"landmark "<<landmark.x<<","<<landmark.y;
    }
  }
  EXPECT_EQ(GetPixel(raster,kCanvasWidth-1,kCanvasHeight-1),kWhite);
}

TEST(Rendering, MatrixGeometryMatchesOrdinaryViewportsLandmarksAndBothSavedReaders) {
  Artifacts artifacts;std::array<std::vector<Raster>,2> scenes;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());const auto& api=writer->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);
    Handle page(api.FPDFPage_New(document.get(),0,kPageWidth,kPageHeight),api.FPDF_ClosePage);
    ASSERT_NE(page.get(),nullptr);ASSERT_NO_FATAL_FAILURE(BuildScene(api,document.get(),page.get()));
    const auto bytes=SaveDocument(api,document.get(),artifacts,"matrix-"+writer->name());
    ASSERT_FALSE(bytes.empty());
    for(const auto& item:kMatrices) {
      SCOPED_TRACE(item.name);
      ASSERT_NE(item.matrix.a*item.matrix.d-item.matrix.b*item.matrix.c,0);
      const auto raster=RenderMatrix(api,page.get(),item.matrix,kFullClip);
      ASSERT_NO_FATAL_FAILURE(ExpectLandmarks(raster,item.matrix));
      if(item.ordinary) {
        Handle normal(api.FPDFBitmap_Create(kCanvasWidth,kCanvasHeight,1),api.FPDFBitmap_Destroy);
        ASSERT_NE(normal.get(),nullptr);
        ASSERT_TRUE(api.FPDFBitmap_FillRect(normal.get(),0,0,kCanvasWidth,kCanvasHeight,0xffffffff));
        api.FPDF_RenderPageBitmap(normal.get(),page.get(),item.left,item.top,item.width,item.height,item.rotation,kCrisp | FPDF_NO_NATIVETEXT);
        CompareRaster(CopyBitmap(api,normal.get()),raster,artifacts,writer->name()+"-equivalent-"+item.name);
      }
      scenes[backend].push_back(raster);
    }
    for(const auto* reader:Implementations()) {
      const auto& read=reader->api();
      Handle loaded(read.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),read.FPDF_CloseDocument);
      ASSERT_NE(loaded.get(),nullptr);
      Handle reopened(read.FPDF_LoadPage(loaded.get(),0),read.FPDF_ClosePage);ASSERT_NE(reopened.get(),nullptr);
      for(size_t i=0;i<kMatrices.size();++i) {
        const auto raster=RenderMatrix(read,reopened.get(),kMatrices[i].matrix,kFullClip);
        ASSERT_NO_FATAL_FAILURE(ExpectLandmarks(raster,kMatrices[i].matrix));
        CompareRaster(scenes[backend][i],raster,artifacts,
            "matrix-"+writer->name()+"-"+reader->name()+"-"+kMatrices[i].name);
      }
    }
    ++backend;
  }
  for(size_t i=0;i<kMatrices.size();++i)
    CompareRaster(scenes[0][i],scenes[1][i],artifacts,std::string("matrix-backends-")+kMatrices[i].name);
}

TEST(Rendering, DeviceClipsTruncateNormalizeAndPreserveAllOutsidePixels) {
  Artifacts artifacts;std::array<std::vector<Raster>,2> scenes;size_t backend=0;
  const FS_MATRIX matrix{1.5f,0,0,1.5f,4,6};
  const Pixel untouched{236,232,228,255};
  const std::array<FS_RECTF,6> clips={{{0,220,240,0},{40.8f,139.8f,119.8f,28.8f},
      {119.8f,28.8f,40.8f,139.8f},{-30,260,280,-20},{40,100,40,0},{260,250,300,230}}};
  for(const auto* library:Implementations()) {
    const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,kPageWidth,kPageHeight),api.FPDF_ClosePage);
    ASSERT_NE(page.get(),nullptr);ASSERT_NO_FATAL_FAILURE(BuildScene(api,document.get(),page.get()));
    const auto bytes=SaveDocument(api,document.get(),artifacts,"device-clips-"+library->name());
    ASSERT_FALSE(bytes.empty());
    const auto full=RenderMatrix(api,page.get(),matrix,kFullClip,kCrisp,0xffe4e8ec);
    for(size_t i=0;i<clips.size();++i) {
      const auto& clip=clips[i];
      auto expected=full;
      // ToFxRect truncates each float toward zero. The rectangular clipping
      // path then normalizes the opposing corners in device coordinates.
      const int left=std::min(static_cast<int>(clip.left),static_cast<int>(clip.right));
      const int right=std::max(static_cast<int>(clip.left),static_cast<int>(clip.right));
      const int top=std::min(static_cast<int>(clip.top),static_cast<int>(clip.bottom));
      const int bottom=std::max(static_cast<int>(clip.top),static_cast<int>(clip.bottom));
      for(int y=0;y<kCanvasHeight;++y) for(int x=0;x<kCanvasWidth;++x)
        if(x<left || x>=right || y<top || y>=bottom)
          std::copy(untouched.begin(),untouched.end(),expected.pixels.begin()+(static_cast<size_t>(y)*kCanvasWidth+x)*4);
      const auto raster=RenderMatrix(api,page.get(),matrix,clip,kCrisp,0xffe4e8ec);
      CompareRaster(expected,raster,artifacts,library->name()+"-clip-"+std::to_string(i));
      scenes[backend].push_back(raster);
    }
    for(const auto* reader:Implementations()) {
      const auto& read=reader->api();
      Handle loaded(read.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),read.FPDF_CloseDocument);
      ASSERT_NE(loaded.get(),nullptr);
      Handle reopened(read.FPDF_LoadPage(loaded.get(),0),read.FPDF_ClosePage);ASSERT_NE(reopened.get(),nullptr);
      for(size_t i=0;i<clips.size();++i)
        CompareRaster(scenes[backend][i],RenderMatrix(read,reopened.get(),matrix,clips[i],kCrisp,0xffe4e8ec),
            artifacts,"clip-saved-"+library->name()+"-"+reader->name()+"-"+std::to_string(i));
    }
    ++backend;
  }
  for(size_t i=0;i<clips.size();++i)
    CompareRaster(scenes[0][i],scenes[1][i],artifacts,"clip-backends-"+std::to_string(i));
}

TEST(Rendering, BitmapFormatsFlagsAnnotationExclusionAndImageOwnedTargets) {
  Artifacts artifacts;std::array<std::vector<Raster>,2> scenes;size_t backend=0;
  const FS_MATRIX matrix{1.5f,0,0,1.5f,7,11};
  const std::vector<int> flags={0,FPDF_GRAYSCALE,FPDF_ANNOT,FPDF_ANNOT|FPDF_PRINTING,
      FPDF_REVERSE_BYTE_ORDER,FPDF_RENDER_LIMITEDIMAGECACHE,FPDF_RENDER_FORCEHALFTONE,
      FPDF_RENDER_NO_SMOOTHIMAGE,FPDF_RENDER_NO_SMOOTHPATH,kCrisp,
      FPDF_DEBUG_INFO|FPDF_NO_CATCH};
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,kPageWidth,kPageHeight),api.FPDF_ClosePage);
    ASSERT_NE(page.get(),nullptr);ASSERT_NO_FATAL_FAILURE(BuildScene(api,document.get(),page.get()));
    for(int format:{FPDFBitmap_Gray,FPDFBitmap_BGR,FPDFBitmap_BGRx,FPDFBitmap_BGRA}) {
      std::vector<Raster> results;
      const int bpp=format==FPDFBitmap_Gray?1:format==FPDFBitmap_BGR?3:4;
      constexpr size_t guard=32;
      const int stride=kCanvasWidth*bpp+16;
      for(int value:flags) {
        // Reversing three color channels has no meaning for a gray target.
        if(format==FPDFBitmap_Gray && value==FPDF_REVERSE_BYTE_ORDER) continue;
        SCOPED_TRACE("format="+std::to_string(format)+" flags="+std::to_string(value));
        std::vector<uint8_t> storage(guard+static_cast<size_t>(stride)*kCanvasHeight+guard,0xa5);
        Handle bitmap(api.FPDFBitmap_CreateEx(kCanvasWidth,kCanvasHeight,format,storage.data()+guard,stride),api.FPDFBitmap_Destroy);
        ASSERT_NE(bitmap.get(),nullptr);
        ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,kCanvasWidth,kCanvasHeight,0xffffffff));
        api.FPDF_RenderPageBitmap(bitmap.get(),page.get(),7,11,180,150,0,value | FPDF_NO_NATIVETEXT);
        auto raster=CopyBitmap(api,bitmap.get());
        if(format==FPDFBitmap_BGRA)
          CompareRaster(raster,RenderMatrix(api,page.get(),matrix,kFullClip,value),artifacts,
              library->name()+"-matrix-flags-"+std::to_string(value));
        const bool gray=format==FPDFBitmap_Gray || (value&FPDF_GRAYSCALE);
        ASSERT_NO_FATAL_FAILURE(ExpectLandmarks(raster,matrix,gray,value==FPDF_REVERSE_BYTE_ORDER));
        const Pixel stamp=gray?Pixel{104,104,104,255}:Pixel{255,0,255,255};
        EXPECT_EQ(GetPixel(raster,160,105),(value&FPDF_ANNOT)?stamp:kWhite);
        EXPECT_EQ(GetPixel(raster,157,143),kWhite);  // Popup is never rendered.
        for(int y=0;y<kCanvasHeight;++y) {
          const auto first=storage.begin()+guard+static_cast<size_t>(y)*stride+kCanvasWidth*bpp;
          EXPECT_TRUE(std::all_of(first,first+16,[](uint8_t b){return b==0xa5;}));
        }
        EXPECT_TRUE(std::all_of(storage.begin(),storage.begin()+guard,[](uint8_t b){return b==0xa5;}));
        EXPECT_TRUE(std::all_of(storage.end()-guard,storage.end(),[](uint8_t b){return b==0xa5;}));
        if(value==(FPDF_DEBUG_INFO|FPDF_NO_CATCH))
          CompareRaster(results.front(),raster,artifacts,library->name()+"-obsolete-"+std::to_string(format));
        results.push_back(raster);scenes[backend].push_back(std::move(raster));
      }
      // The fractional triangle has edge coverage with default antialiasing.
      const size_t no_path_index=format==FPDFBitmap_Gray?7:8;
      EXPECT_NE(results[0].pixels,results[no_path_index].pixels);
    }
    // FPDFImageObj_GetBitmap returns an independently owned bitmap, also a
    // documented render target. Keep its image alive while rendering to it.
    Handle source(api.FPDFBitmap_Create(kCanvasWidth,kCanvasHeight,1),api.FPDFBitmap_Destroy);
    ASSERT_NE(source.get(),nullptr);
    ASSERT_TRUE(api.FPDFBitmap_FillRect(source.get(),0,0,kCanvasWidth,kCanvasHeight,0xffffffff));
    Handle image(api.FPDFPageObj_NewImageObj(document.get()),api.FPDFPageObj_Destroy);ASSERT_NE(image.get(),nullptr);
    ASSERT_TRUE(api.FPDFImageObj_SetBitmap(nullptr,0,image.get(),source.get()));
    Handle target(api.FPDFImageObj_GetBitmap(image.get()),api.FPDFBitmap_Destroy);ASSERT_NE(target.get(),nullptr);
    api.FPDF_RenderPageBitmap(target.get(),page.get(),0,0,kPageWidth,kPageHeight,0,kCrisp | FPDF_NO_NATIVETEXT);
    const auto raster=CopyBitmap(api,target.get());
    ASSERT_NO_FATAL_FAILURE(ExpectLandmarks(raster,FS_MATRIX{1,0,0,1,0,0}));
    CompareRaster(RenderMatrix(api,page.get(),FS_MATRIX{1,0,0,1,0,0},kFullClip),raster,artifacts,library->name()+"-image-owned-target");
    ++backend;
  }
  ASSERT_EQ(scenes[0].size(),scenes[1].size());
  for(size_t i=0;i<scenes[0].size();++i)
    CompareRaster(scenes[0][i],scenes[1][i],artifacts,"format-flags-backends-"+std::to_string(i));
}

TEST(Rendering, NoSmoothTextOverridesLcdAndObsoleteFlagsWithoutSuppressingText) {
  Artifacts artifacts;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,kPageWidth,kPageHeight),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    Handle text(api.FPDFPageObj_NewTextObj(document.get(),"Helvetica",19),api.FPDFPageObj_Destroy);ASSERT_NE(text.get(),nullptr);
    const std::u16string value=u"Port 123";
    ASSERT_TRUE(api.FPDFText_SetText(text.get(),reinterpret_cast<const FPDF_WCHAR*>(value.c_str())));
    ASSERT_TRUE(api.FPDFPageObj_SetFillColor(text.get(),0,0,0,255));
    api.FPDFPageObj_Transform(text.get(),1,0,0,1,8,36);
    api.FPDFPage_InsertObject(page.get(),text.release());ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    const auto normal=Render(api,page.get(),kPageWidth,kPageHeight);
    const auto lcd=Render(api,page.get(),kPageWidth,kPageHeight,FPDF_LCD_TEXT);
    const auto crisp=Render(api,page.get(),kPageWidth,kPageHeight,FPDF_RENDER_NO_SMOOTHTEXT);
    const auto both=Render(api,page.get(),kPageWidth,kPageHeight,FPDF_LCD_TEXT|FPDF_RENDER_NO_SMOOTHTEXT);
    CompareRaster(crisp,both,artifacts,library->name()+"-no-text-smoothing-disables-lcd");
    CompareRaster(normal,Render(api,page.get(),kPageWidth,kPageHeight,FPDF_DEBUG_INFO|FPDF_NO_CATCH),artifacts,library->name()+"-text-obsolete-flags");
    if (library->name() == "rust") {
      CompareRaster(normal,RenderWithExactFlags(api,page.get(),kPageWidth,kPageHeight,0),artifacts,library->name()+"-portable-text-output");
    }
    const auto nonwhite=[](const Raster& raster) {
      size_t count=0;
      for(size_t offset=0;offset<raster.pixels.size();offset+=4)
        if(raster.pixels[offset]!=255 || raster.pixels[offset+1]!=255 || raster.pixels[offset+2]!=255) ++count;
      return count;
    };
    EXPECT_GT(nonwhite(normal),100u);EXPECT_GT(nonwhite(lcd),100u);EXPECT_GT(nonwhite(crisp),100u);
    EXPECT_NE(normal.pixels,crisp.pixels);
    artifacts.Image(library->name()+"-normal-text",normal);
    artifacts.Image(library->name()+"-lcd-text",lcd);
    artifacts.Image(library->name()+"-no-smoothing-text",crisp);
  }
}

}  // namespace
}  // namespace pdfoxium::integration
