// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "runtime_fixture.h"

#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using RGB=std::array<int,3>;
constexpr int kSide=96,kRows=3,kCell=8;
const std::array<std::string,3> kBits{"010110101","101001010","110010101"};
struct Spec {int width=1,kind=0;std::string name;};
std::string Samples(const Spec& s) {
  const size_t pitch=(s.width+7)/8;std::string bytes(pitch*kRows,char(255));
  for(int y=0;y<kRows;++y)for(int x=0;x<s.width;++x) {
    auto& byte=bytes[y*pitch+x/8];const auto mask=static_cast<unsigned char>(0x80>>(x%8));
    if(kBits[y][x]=='1')byte=char(static_cast<unsigned char>(byte)|mask);else byte=char(static_cast<unsigned char>(byte)&~mask);
  }
  return bytes;
}
std::vector<uint8_t> Fixture(const Spec& s) {
  // Public bitmap constructors expose Gray/BGR/BGRx/BGRA, not packed1bpp.
  // These literal PDF streams exercise reading/editing/saving one-bit images;
  // the private1bpp SetBitmap writer is covered by the separate Rust units.
  const std::string color=s.kind==0?"/ColorSpace [/Indexed /DeviceRGB 1 <e020301464dc>]":"/ImageMask true";
  const std::string decode=s.kind==2?"/Decode [1 0]":"";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Resources << /XObject << /I 5 0 R >> >> /Contents 4 0 R >>",
    PdfStream("","q 0.12549019607843137 0.5019607843137255 0.25098039215686274 rg "+std::to_string(s.width*kCell)+" 0 0 24 12 12 cm /I Do Q 0 0 1 rg 84 84 4 4 re f"),
    PdfStream("/Type /XObject /Subtype /Image /Width "+std::to_string(s.width)+" /Height 3 /BitsPerComponent 1 "+color+" "+decode,Samples(s))});
}
void Pixel(const Raster& image,int x,int y,RGB expected) {
  ASSERT_EQ(image.width,kSide);ASSERT_EQ(image.height,kSide);const auto at=(size_t(kSide-1-y)*kSide+x)*4;
  for(int c=0;c<3;++c)EXPECT_NEAR(image.pixels.at(at+2-c),expected[c],1)<<x<<','<<y;
  EXPECT_EQ(image.pixels.at(at+3),255);
}
void InspectPixels(const Raster& image,const Spec& s,bool edited) {
  const int dx=edited?4:0,dy=edited?8:0;const RGB paint=edited?RGB{16,64,224}:RGB{32,128,64};
  Pixel(image,4,4,{255,255,255});Pixel(image,90,90,{255,255,255});Pixel(image,86,86,{0,0,255});
  Pixel(image,12+dx+s.width*kCell+2,24+dy,{255,255,255});
  if(edited)Pixel(image,14,14,{255,255,255});
  for(int y=0;y<kRows;++y)for(int x=0;x<s.width;++x) {
    const bool bit=kBits[y][x]=='1';const bool painted=s.kind==2?bit:!bit;
    const RGB expected=s.kind==0?(bit?RGB{20,100,220}:RGB{224,32,48}):(painted?paint:RGB{255,255,255});
    Pixel(image,12+dx+x*kCell+4,12+dy+(kRows-1-y)*kCell+4,expected);
  }
}
void InspectSamples(const Api& api,FPDF_PAGE page,FPDF_PAGEOBJECT object,const Spec& s,bool raw) {
  ASSERT_EQ(api.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_IMAGE);unsigned width=0,height=0;
  ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(object,&width,&height));EXPECT_EQ(width,unsigned(s.width));EXPECT_EQ(height,3u);
  FPDF_IMAGEOBJ_METADATA metadata{};ASSERT_TRUE(api.FPDFImageObj_GetImageMetadata(object,page,&metadata));EXPECT_EQ(metadata.width,unsigned(s.width));EXPECT_EQ(metadata.height,3u);EXPECT_EQ(metadata.bits_per_pixel,1u);
  const auto samples=Samples(s);const std::vector<uint8_t> expected(samples.begin(),samples.end());
  const auto get=raw?api.FPDFImageObj_GetImageDataRaw:api.FPDFImageObj_GetImageDataDecoded;
  const auto n=get(object,nullptr,0);ASSERT_EQ(n,expected.size());std::vector<uint8_t> data(n+1,0xa5);
  ASSERT_EQ(get(object,data.data(),n),n);EXPECT_EQ(data.back(),0xa5);data.resize(n);EXPECT_EQ(data,expected);
}
void Observe(const Api& api,FPDF_PAGE page,const Spec& s,bool edited,Artifacts& artifacts,const std::string& label,Raster* image) {
  ASSERT_EQ(api.FPDFPage_CountObjects(page),2);const auto object=api.FPDFPage_GetObject(page,0);ASSERT_NE(object,nullptr);
  ASSERT_NO_FATAL_FAILURE(InspectSamples(api,page,object,s,false));
  FS_MATRIX matrix{};ASSERT_TRUE(api.FPDFPageObj_GetMatrix(object,&matrix));EXPECT_FLOAT_EQ(matrix.a,s.width*kCell);EXPECT_FLOAT_EQ(matrix.d,24);EXPECT_FLOAT_EQ(matrix.e,edited?16:12);EXPECT_FLOAT_EQ(matrix.f,edited?20:12);
  *image=RenderWithExactFlags(api,page,kSide,kSide,FPDF_RENDER_NO_SMOOTHIMAGE|FPDF_RENDER_NO_SMOOTHPATH);
  ASSERT_NO_FATAL_FAILURE(InspectPixels(*image,s,edited));
  CompareRaster(*image,RenderWithExactFlags(api,page,kSide,kSide,FPDF_RENDER_NO_SMOOTHIMAGE|FPDF_RENDER_NO_SMOOTHPATH),artifacts,label+"-repeat");
}
void Exercise(const Spec& s) {
  const auto input=Fixture(s);Artifacts artifacts;std::array<Raster,2> initial,edited;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(s.name+" writer="+writer->name());const auto& api=writer->api();
    Handle doc(api.FPDF_LoadMemDocument64(input.data(),input.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    ASSERT_NO_FATAL_FAILURE(Observe(api,page.get(),s,false,artifacts,writer->name(),&initial[backend]));
    const auto image=api.FPDFPage_GetObject(page.get(),0);ASSERT_NE(image,nullptr);ASSERT_NO_FATAL_FAILURE(InspectSamples(api,page.get(),image,s,true));
    // This edits the existing one-bit PDF image object; it does not pretend a
    // public8bit bitmap constructor can reach the private1bit embedding branch.
    ASSERT_TRUE(api.FPDFPageObj_SetFillColor(image,16,64,224,255));api.FPDFPageObj_Transform(image,1,0,0,1,4,8);
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    ASSERT_NO_FATAL_FAILURE(Observe(api,page.get(),s,true,artifacts,writer->name()+"-edited",&edited[backend]));EXPECT_NE(initial[backend].pixels,edited[backend].pixels);page.reset();
    for(FPDF_DWORD flags:{FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}) {
      const auto saved=SaveDocument(api,doc.get(),artifacts,s.name+"-"+writer->name()+"-"+std::to_string(flags),flags);ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());const auto& read=reader->api();
        Handle loaded(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(loaded.get(),nullptr);
        Handle reopened(read.FPDF_LoadPage(loaded.get(),0),read.FPDF_ClosePage);ASSERT_NE(reopened.get(),nullptr);Raster after;
        ASSERT_NO_FATAL_FAILURE(Observe(read,reopened.get(),s,true,artifacts,writer->name()+"-"+reader->name(),&after));CompareRaster(edited[backend],after,artifacts,"saved-reader",RasterTolerance::ConnectedError(1));
      }
    }
    ++backend;
  }
  CompareRaster(initial[0],initial[1],artifacts,"original-native-rust",RasterTolerance::ConnectedError(1));CompareRaster(edited[0],edited[1],artifacts,"edited-native-rust",RasterTolerance::ConnectedError(1));
}
std::vector<Spec> Cases() {
  std::vector<Spec> cases;for(int kind=0;kind<3;++kind)for(int width:{1,7,8,9})cases.push_back({width,kind,std::string(kind==0?"Indexed":kind==1?"ZeroPaintStencil":"OnePaintStencil")+"Width"+std::to_string(width)});return cases;
}
class CoverageOneBitImageRoundtrip:public testing::TestWithParam<Spec>{};
TEST_P(CoverageOneBitImageRoundtrip, PackedRowsColorsAndEditedStencilStateSurviveBothWritersAndReaders){Exercise(GetParam());}
INSTANTIATE_TEST_SUITE_P(PackedRowBoundaries,CoverageOneBitImageRoundtrip,testing::ValuesIn(Cases()),[](const testing::TestParamInfo<Spec>& p){return p.param.name;});
}  // namespace
}  // namespace pdfoxium::integration
