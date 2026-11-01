// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "runtime_fixture.h"

#include <gtest/gtest.h>
#include <algorithm>
#include <array>
#include <cmath>
#include <iomanip>
#include <sstream>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using RGB=std::array<int,3>;
struct AlphaCase {
  std::string name,mode="Normal";
  int back_alpha=255,source_alpha=255;
  RGB back{255,0,0},source{0,0,255};
  bool clipped=false,reverse=false,gray=false;
};
std::string Opacity(int alpha) {
  std::ostringstream out;out<<std::setprecision(17)<<double(alpha)/255.;return out.str();
}
std::string Color(RGB c) {
  return Opacity(c[0])+" "+Opacity(c[1])+" "+Opacity(c[2])+" rg ";
}
std::vector<uint8_t> Fixture(const AlphaCase& spec) {
  std::string samples;for(int i=0;i<3;++i)for(int c:spec.source)samples.push_back(char(c));
  const auto clip=spec.clipped?"12 36 60 24 re W n ":"";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Contents 4 0 R /Resources << /XObject << /Im 5 0 R >> /ExtGState << /B 7 0 R /F 8 0 R >> >> >>",
    PdfStream("","q /B gs "+Color(spec.back)+"12 12 60 48 re f Q q /F gs "+clip+"60 0 0 48 12 12 cm /Im Do Q"),
    PdfStream("/Type /XObject /Subtype /Image /Width 3 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceRGB /SMask 6 0 R",samples),
    PdfStream("/Type /XObject /Subtype /Image /Width 3 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray",std::string("\0\x80\xff",3)),
    "<< /Type /ExtGState /ca "+Opacity(spec.back_alpha)+" >>",
    "<< /Type /ExtGState /ca "+Opacity(spec.source_alpha)+" /BM /"+spec.mode+" >>"});
}
void AddLandmark(const Api& api,FPDF_PAGE page) {
  // A real public edit proves the saved PDF contains the current page state.
  auto object=api.FPDFPageObj_CreateNewRect(84,84,4,4);ASSERT_NE(object,nullptr);
  ASSERT_TRUE(api.FPDFPageObj_SetFillColor(object,0,255,0,255));
  ASSERT_TRUE(api.FPDFPath_SetDrawMode(object,FPDF_FILLMODE_WINDING,0));
  api.FPDFPage_InsertObject(page,object);ASSERT_TRUE(api.FPDFPage_GenerateContent(page));
}
Raster Paint(const Api& api,FPDF_PAGE page,const AlphaCase& spec,bool matrix) {
  const int format=spec.gray?FPDFBitmap_Gray:FPDFBitmap_BGRA;
  Handle bitmap(api.FPDFBitmap_CreateEx(96,96,format,nullptr,0),api.FPDFBitmap_Destroy);
  if(!bitmap.get()){ADD_FAILURE()<<"No destination bitmap";return {};}
  api.FPDFBitmap_FillRect(bitmap.get(),0,0,96,96,spec.gray?0xffffffff:0);
  const int flags=FPDF_RENDER_NO_SMOOTHIMAGE|FPDF_RENDER_NO_SMOOTHPATH|(spec.reverse?FPDF_REVERSE_BYTE_ORDER:0);
  if(matrix) {
    // The API composes this matrix after its own page display transform.
    // A second PDF-to-device Y flip would move the page outside the target.
    const FS_MATRIX transform{1,0,0,1,0,0};const FS_RECTF clip{0,0,96,96};
    api.FPDF_RenderPageBitmapWithMatrix(bitmap.get(),page,&transform,&clip,flags);
  } else {api.FPDF_RenderPageBitmap(bitmap.get(),page,0,0,96,96,0,flags);}
  Raster result;
  if(spec.gray) {
    // runtime_test::Bitmap is explicitly a four-byte bitmap reader. Expand
    // the public Gray destination bytes ourselves without changing their values.
    const int stride=api.FPDFBitmap_GetStride(bitmap.get());
    const auto* data=static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
    if(api.FPDFBitmap_GetFormat(bitmap.get())!=FPDFBitmap_Gray||!data||stride<96) {
      ADD_FAILURE()<<"Expected a live Gray destination";return {};
    }
    result.width=96;result.height=96;result.pixels.resize(96*96*4);
    for(int y=0;y<96;++y)for(int x=0;x<96;++x) {
      const auto gray=data[size_t(y)*stride+x];const auto at=(size_t(y)*96+x)*4;
      result.pixels[at]=result.pixels[at+1]=result.pixels[at+2]=gray;result.pixels[at+3]=255;
    }
  } else {result=runtime_test::Bitmap(api,bitmap.get());}
  // This is the specified destination channel order, not a visual tolerance.
  if(spec.reverse)for(size_t i=0;i<result.pixels.size();i+=4)std::swap(result.pixels[i],result.pixels[i+2]);
  return result;
}
std::array<double,4> Expected(const AlphaCase& spec,int mask) {
  const double a=double(spec.source_alpha*mask/255)/255.;
  const double b=double(spec.back_alpha)/255.;const double out=a+b-a*b;
  std::array<double,4> result{0,0,0,out*255.};if(out==0)return result;
  for(int i=0;i<3;++i) {
    const double source=double(spec.source[i])/255.,back=double(spec.back[i])/255.;
    const double blend=spec.mode=="Normal"?source:spec.mode=="Multiply"?source*back:
        spec.mode=="Screen"?source+back-source*back:spec.mode=="Luminosity"?source:back;
    result[i]=255.*((1-a)*b*back+(1-b)*a*source+a*b*blend)/out;
  }
  return result;
}
void Pixel(const Raster& image,int x,int y,const std::array<double,4>& expected) {
  ASSERT_EQ(image.width,96);ASSERT_EQ(image.height,96);
  const size_t at=(size_t(95-y)*96+x)*4;
  EXPECT_NEAR(image.pixels.at(at+3),expected[3],1.01)<<x<<','<<y;
  if(expected[3]>0)for(int c=0;c<3;++c) {
    // At most two integer quantization levels differ from the floating PDF
    // source-over reference. Native/Rust still use the unchanged shared metric.
    EXPECT_NEAR(image.pixels.at(at+2-c),expected[c],2.01)<<x<<','<<y<<" channel"<<c;
  }
}
void Inspect(const Raster& image,const AlphaCase& spec) {
  const std::array<double,4> outside=spec.gray?std::array<double,4>{255,255,255,255}:std::array<double,4>{0,0,0,0};
  Pixel(image,4,4,outside);Pixel(image,90,90,outside);Pixel(image,76,28,outside);
  Pixel(image,86,86,spec.gray?std::array<double,4>{150,150,150,255}:std::array<double,4>{0,255,0,255});
  for(int col=0;col<3;++col)for(int y:{24,48}) {
    const int mask=spec.clipped&&y==24?0:std::array<int,3>{0,128,255}[col];
    Pixel(image,22+20*col,y,Expected(spec,mask));
  }
}
void Observe(const Api& api,FPDF_PAGE page,const AlphaCase& spec,Artifacts& artifacts,Raster* result) {
  *result=Paint(api,page,spec,false);
    ASSERT_NO_FATAL_FAILURE(Inspect(*result,spec));
  const auto matrix=Paint(api,page,spec,true);
    ASSERT_NO_FATAL_FAILURE(Inspect(matrix,spec));
  CompareRaster(*result,matrix,artifacts,"ordinary-matrix");
}
void Exercise(const AlphaCase& spec) {
  const auto input=Fixture(spec);Artifacts artifacts;std::array<Raster,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(spec.name+" writer="+writer->name());const auto& api=writer->api();
    Handle doc(api.FPDF_LoadMemDocument64(input.data(),input.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    ASSERT_NO_FATAL_FAILURE(AddLandmark(api,page.get()));
    ASSERT_NO_FATAL_FAILURE(Observe(api,page.get(),spec,artifacts,&originals[backend]));page.reset();
    for(FPDF_DWORD flags:{FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}) {
      const auto saved=SaveDocument(api,doc.get(),artifacts,spec.name+"-"+writer->name()+"-"+std::to_string(flags),flags);ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());const auto& read=reader->api();
        Handle loaded(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(loaded.get(),nullptr);
        Handle reopened(read.FPDF_LoadPage(loaded.get(),0),read.FPDF_ClosePage);ASSERT_NE(reopened.get(),nullptr);Raster after;
        ASSERT_NO_FATAL_FAILURE(Observe(read,reopened.get(),spec,artifacts,&after));
        CompareRaster(originals[backend],after,artifacts,writer->name()+"-"+reader->name()+"-save",RasterTolerance::ConnectedError(1));
      }
    }
    ++backend;
  }
  CompareRaster(originals[0],originals[1],artifacts,"native-rust",RasterTolerance::ConnectedError(1));
}
std::vector<AlphaCase> Cases() {
  std::vector<AlphaCase> out;
  auto add=[&](const char* name,const char* mode,int back,int source) -> AlphaCase& {AlphaCase s;s.name=name;s.mode=mode;s.back_alpha=back;s.source_alpha=source;out.push_back(s);return out.back();};
  add("NormalTransparentBackdrop","Normal",0,255);
  add("NormalHalfBackdrop","Normal",128,255);
  add("NormalOpaqueBackdrop","Normal",255,255);
  add("NormalHalfSource","Normal",255,128);
  add("NormalHalfSourceAndBackdrop","Normal",128,128);
  add("MultiplyTransparentBackdrop","Multiply",0,255);
  add("MultiplyHalfBackdrop","Multiply",128,255);
  add("MultiplyOpaqueBackdrop","Multiply",255,255);
  add("MultiplyClippedHalfSource","Multiply",255,128).clipped=true;
  add("ScreenHalfSourceAndBackdrop","Screen",128,128);
  for(const auto& mode:{"Hue","Saturation","Color","Luminosity"}) {
    auto& s=add((std::string(mode)+"OpaqueGrayBackdrop").c_str(),mode,255,255);s.back={160,160,160};s.source={32,32,32};
  }
  add("NormalRgbDestinationOrder","Normal",128,128).reverse=true;
  add("ScreenRgbDestinationOrder","Screen",128,128).reverse=true;
  for(const auto& mode:{"Color","Luminosity"}) {
    auto& s=add((std::string(mode)+"GrayDestination").c_str(),mode,255,255);s.back={160,160,160};s.source={32,32,32};s.gray=true;
  }
  return out;
}
class CoverageAlphaClipBlend:public testing::TestWithParam<AlphaCase> {};
TEST_P(CoverageAlphaClipBlend, IndependentAlphaAndColorSurviveBothRenderPathsAndFourWaySaving) {Exercise(GetParam());}
INSTANTIATE_TEST_SUITE_P(AlphaBoundaries,CoverageAlphaClipBlend,testing::ValuesIn(Cases()),[](const testing::TestParamInfo<AlphaCase>& info){return info.param.name;});
}  // namespace
}  // namespace pdfoxium::integration
