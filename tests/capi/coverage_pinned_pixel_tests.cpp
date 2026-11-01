// Copyright 2026 The pdfoxium Authors
// Pinned fixtures and expected rasters are from PDFium; see the resource README.
#include "pinned_fixture_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>

#ifndef PDFOXIUM_PINNED_PIXEL_DIR
#error PDFOXIUM_PINNED_PIXEL_DIR must identify the build-generated pinned PDF fixtures
#endif

namespace pdfoxium::integration {
namespace {

// These are the generic AGG pixel-test expectations, not snapshots from either
// implementation under test. Dimensions and /Pages Count1 come from the pinned
// .in sources and PNG IHDR. Raw RGBA provenance and hashes are in manifest.json.
std::vector<uint8_t> Bytes(const std::filesystem::path& path) {
  std::ifstream file(path,std::ios::binary);
  if(!file){ADD_FAILURE()<<"Cannot read pinned fixture "<<path;return {};}
  return {std::istreambuf_iterator<char>(file),std::istreambuf_iterator<char>()};
}
struct Pause : IFSDK_PAUSE {
  int calls=0;
  Pause() : IFSDK_PAUSE{} {
    version=1;NeedToPauseNow=[](IFSDK_PAUSE* pause)->FPDF_BOOL {
      ++static_cast<Pause*>(pause)->calls;return 1;
    };
  }
};
Raster Paint(const Api& api,FPDF_PAGE page,int width,int height,bool progressive) {
  // Match testing/pdfium_test/pdfium_test.cc:232,1094: scale1, rotate0,
  // FPDF_ANNOT, straight BGRA/transparent clear for transparent pages, otherwise
  // BGRx/white. Progressive rendering always asks to pause, as its test runner.
  const bool alpha=!!api.FPDFPage_HasTransparency(page);
  const int format=alpha?FPDFBitmap_BGRA:FPDFBitmap_BGRx;
  Handle bitmap(api.FPDFBitmap_CreateEx(width,height,format,nullptr,width*4),api.FPDFBitmap_Destroy);
  if(!bitmap.get()){ADD_FAILURE()<<"Pinned raster allocation failed";return {};}
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,width,height,alpha?0:0xffffffff));
  if(progressive) {
    struct Close {
      const Api& api;FPDF_PAGE page;
      ~Close(){api.FPDF_RenderPage_Close(page);}
    } close{api,page};
    Pause pause;
    int status=api.FPDF_RenderPageBitmapWithColorScheme_Start(bitmap.get(),page,0,0,width,height,0,FPDF_ANNOT | FPDF_NO_NATIVETEXT,nullptr,&pause);
    int continuations=0;
    while(status==FPDF_RENDER_TOBECONTINUED&&continuations<10000) {
      ++continuations;status=api.FPDF_RenderPage_Continue(page,&pause);
    }
    EXPECT_EQ(status,FPDF_RENDER_DONE)<<"continuations="<<continuations<<" pause callbacks="<<pause.calls;
    if(status!=FPDF_RENDER_DONE)return {};
  } else api.FPDF_RenderPageBitmap(bitmap.get(),page,0,0,width,height,0,FPDF_ANNOT | FPDF_NO_NATIVETEXT);
  const auto* pixels=static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
  const int stride=api.FPDFBitmap_GetStride(bitmap.get());
  if(!pixels||stride<width*4){ADD_FAILURE()<<"Invalid pinned bitmap storage";return {};}
  Raster result{width,height,std::vector<uint8_t>(static_cast<size_t>(width)*height*4)};
  for(int y=0;y<height;++y)std::copy_n(pixels+static_cast<size_t>(y)*stride,width*4,result.pixels.begin()+static_cast<size_t>(y)*width*4);
  // BGRx's fourth component is unused, not alpha; the PNG writer records opaque
  // pixels for this format. Actual BGRA alpha is retained exactly.
  if(!alpha)for(size_t i=3;i<result.pixels.size();i+=4)result.pixels[i]=255;
  return result;
}
bool MissingColorspaceCase(const std::string& name) {
  return name.rfind("jpxdecode_without_colorspace",0)==0;
}
std::vector<uint8_t> InlineAlphaControl(bool legacy) {
  // Exact 4x4 samples in the pinned gray/RGB/CMYK JPEG2000 inputs. These raw
  // image/SMask controls use the public PDF model instead of a second decoder.
  // Each row is sampled at the original 32pt square transform over gray128.
  const std::array<int,16> gray={0,0,255,255,0,0,255,255,255,255,0,0,255,255,0,0};
  const std::array<int,16> colors={0,0,1,1,0,0,1,1,1,1,2,2,1,1,2,2};
  const std::array<std::array<int,3>,3> rgb={{{255,0,0},{0,255,0},{0,0,255}}};
  const std::array<std::array<int,4>,4> cmyk={{{255,0,0,0},{0,255,0,0},{0,0,255,0},{0,0,0,255}}};
  const std::array<int,16> inks={0,0,1,1,0,0,1,1,2,2,3,3,2,2,3,3};
  const std::array<int,16> alpha={255,255,255,255,255,128,128,255,255,128,128,255,255,255,255,255};
  std::string gray_data,rgb_data,cmyk_data,mask_data,legacy_rgb;
  for(size_t i=0;i<16;++i) {
    gray_data.push_back(static_cast<char>(gray[i]));mask_data.push_back(static_cast<char>(alpha[i]));
    for(int channel:rgb[colors[i]]) {
      rgb_data.push_back(static_cast<char>(channel));
      legacy_rgb.push_back(static_cast<char>((channel*alpha[i]+255*(255-alpha[i]))/255));
    }
    for(int channel:cmyk[inks[i]])cmyk_data.push_back(static_cast<char>(channel));
  }
  const std::string image="/Type /XObject /Subtype /Image /Width 4 /Height 4 /BitsPerComponent 8 /ColorSpace ";
  std::vector<std::string> objects={
      "<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 88 128] /Contents 4 0 R /Resources << /XObject << /G 5 0 R /GA 6 0 R /R 7 0 R /RA 8 0 R /C 9 0 R /CA 10 0 R >> >> >>",
      PdfStream("","0.5 0.5 0.5 rg 0 0 88 128 re f q 32 0 0 32 8 88 cm /G Do Q q 32 0 0 32 48 88 cm /GA Do Q q 32 0 0 32 8 48 cm /R Do Q q 32 0 0 32 48 48 cm /RA Do Q q 32 0 0 32 8 8 cm /C Do Q q 32 0 0 32 48 8 cm /CA Do Q"),
      PdfStream(image+"/DeviceGray",gray_data),
      PdfStream(image+"/DeviceGray"+(legacy?"":" /SMask 11 0 R"),gray_data),
      PdfStream(image+"/DeviceRGB",rgb_data),
      PdfStream(image+"/DeviceRGB /SMask 11 0 R",legacy?legacy_rgb:rgb_data),
      PdfStream(image+"/DeviceCMYK",cmyk_data),
      PdfStream(image+"/DeviceCMYK"+(legacy?"":" /SMask 11 0 R"),cmyk_data),
      PdfStream(image+"/DeviceGray",mask_data)};
  return PdfFixture(objects);
}
Raster AlphaControl(const Api& api,bool legacy) {
  const auto bytes=InlineAlphaControl(legacy);
  Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
  if(!document.get()){ADD_FAILURE()<<"alpha control load";return {};}
  Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
  if(!page.get()){ADD_FAILURE()<<"alpha control page";return {};}
  return Paint(api,page.get(),88,128,false);
}
void InspectInlineAlpha(const Raster& image,bool legacy) {
  // DeviceCMYK's established Adobe colors for the four pinned ink samples.
  const std::array<std::array<int,3>,4> cmyk_rgb={{{0,174,239},{237,2,140},{255,241,1},{35,31,32}}};
  const std::array<std::array<int,3>,3> rgb={{{255,0,0},{0,255,0},{0,0,255}}};
  for(int row=0;row<4;++row)for(int column=0;column<4;++column)for(int family=0;family<3;++family) {
    const int alpha=(row==1||row==2)&&(column==1||column==2)?128:255;
    std::array<int,3> source;
    if(family==0){const int gray=(row<2)==(column<2)?0:255;source={gray,gray,gray};}
    else if(family==1)source=rgb[row<2?(column<2?0:1):(column<2?1:2)];
    else source=cmyk_rgb[row<2?(column<2?0:1):(column<2?2:3)];
    int applied_alpha=alpha;
    if(legacy&&family!=1)applied_alpha=255;
    if(legacy&&family==1)for(int& value:source)value=(value*alpha+255*(255-alpha))/255;
    const int x=48+column*8+4,y=8+family*40+row*8+4;
    const auto at=(static_cast<size_t>(y)*image.width+x)*4;
    for(size_t channel=0;channel<3;++channel)
      EXPECT_NEAR(image.pixels.at(at+2-channel),(source[channel]*applied_alpha+128*(255-applied_alpha))/255,1)
          <<"family="<<family<<" cell="<<column<<","<<row<<" channel="<<channel;
    EXPECT_EQ(image.pixels.at(at+3),255);
  }
}
void CheckInlineAlphaBoundary(const Library& owner,const Raster& expected,const Raster& actual,
                              Artifacts& artifacts,const std::string& name) {
  // The old committed PNG applies gray alpha (which native drops), white-
  // premattes RGB (which Rust correctly avoids), and has a shifted CMYK mask.
  // Preserve the original image and compare every unaffected pixel to it.
  artifacts.Image(name+"-unmodified-pinned-golden",expected);
  artifacts.Image(name+"-original-inline-jpx",actual);
  const int delta=owner.name()=="native"?0:1;
  // The old CMYK tile spans x49..80 instead of the declared x48..79.
  // Its single shifted right-edge column is also part of the golden defect;
  // every other margin pixel continues to compare unchanged.
  for(const auto& rect:std::array<std::array<int,4>,8>{{{0,0,48,128},{80,0,8,88},{81,88,7,32},{80,120,8,8},{48,0,32,8},{48,40,32,8},{48,80,32,8},{48,120,32,8}}})
    CompareRaster(CropRaster(expected,rect[0],rect[1],rect[2],rect[3]),CropRaster(actual,rect[0],rect[1],rect[2],rect[3]),artifacts,name+"-unchanged-"+std::to_string(rect[0])+"-"+std::to_string(rect[1]),delta);
  for(int y=88;y<120;++y) {
    const std::array<int,3> old_right=y<104?std::array<int,3>{237,2,140}:std::array<int,3>{35,31,32};
    const std::array<int,3> correct_left=y<104?std::array<int,3>{0,174,239}:std::array<int,3>{255,241,1};
    const auto right=(static_cast<size_t>(y)*expected.width+80)*4;
    const auto left=(static_cast<size_t>(y)*expected.width+48)*4;
    for(size_t channel=0;channel<3;++channel) {
      EXPECT_EQ(expected.pixels[right+2-channel],old_right[channel]);
      EXPECT_EQ(expected.pixels[left+2-channel],128);
      EXPECT_EQ(actual.pixels[right+2-channel],128);
      EXPECT_EQ(actual.pixels[left+2-channel],correct_left[channel]);
    }
    EXPECT_EQ(expected.pixels[right+3],255);EXPECT_EQ(actual.pixels[right+3],255);
  }
  const bool legacy=owner.name()=="native";
  InspectInlineAlpha(actual,legacy);
  const auto control=AlphaControl(owner.api(),legacy);
  CompareRaster(control,actual,artifacts,name+"-explicit-source-and-mask-control",delta);
  // Correct Rust grayscale opacity also agrees with the original golden's
  // grayscale tile; RGB/CMYK use explicit source samples and separate masks.
  if(!legacy)CompareRaster(CropRaster(expected,48,8,32,32),CropRaster(actual,48,8,32,32),artifacts,name+"-gray-golden",1);
}
void CheckGolden(const Library& owner,const Raster& expected,const Raster& actual,
                 Artifacts& artifacts,const std::string& name) {
  if(MissingColorspaceCase(name)){CheckInlineAlphaBoundary(owner,expected,actual,artifacts,name);return;}
  // Native establishes the exact pinned AGG control. Rust's existing AGG
  // arithmetic can differ by one byte; every pixel/channel still has this
  // bound, with no fraction of arbitrary exceptional pixels accepted.
  CompareRaster(expected,actual,artifacts,name,owner.name()=="native"?0:1);
  if(expected.width==88&&expected.height==128) {
    // Inspect each of the six original JPX tiles independently as well as the
    // whole page, so a missing alpha/colorspace tile cannot be diluted by the
    // surrounding gray/black backdrop or another successful image.
    for(int y:{8,48,88})for(int x:{8,48})
      CompareRaster(CropRaster(expected,x,y,32,32),CropRaster(actual,x,y,32,32),artifacts,
                    name+"-tile-"+std::to_string(x)+"-"+std::to_string(y),owner.name()=="native"?0:1);
  }
}
void RunPinned(const std::string& name,int width,int height) {
  const auto bytes=Bytes(std::filesystem::path(PDFOXIUM_PINNED_PIXEL_DIR)/(name+".pdf"));
  ASSERT_FALSE(bytes.empty());const auto expected=LoadPinnedGolden("pixel/"+name+"_expected.pdf.0.png");
  ASSERT_EQ(expected.width,width);ASSERT_EQ(expected.height,height);
  ASSERT_EQ(expected.pixels.size(),static_cast<size_t>(width)*height*4);
  Artifacts artifacts;artifacts.Pdf(name+"-pinned-input",bytes);
  std::array<Raster,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());const auto& api=writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr)<<"load error "<<api.FPDF_GetLastError();
    ASSERT_EQ(api.FPDF_GetPageCount(document.get()),1);
    Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    EXPECT_FLOAT_EQ(api.FPDF_GetPageWidthF(page.get()),width);EXPECT_FLOAT_EQ(api.FPDF_GetPageHeightF(page.get()),height);
    originals[backend]=Paint(api,page.get(),width,height,true);
    ASSERT_EQ(originals[backend].pixels.size(),expected.pixels.size());
    CheckGolden(*writer,expected,originals[backend],artifacts,name+"-"+writer->name()+"-golden");
    const auto one_shot=Paint(api,page.get(),width,height,false);
    CompareRaster(originals[backend],one_shot,artifacts,name+"-"+writer->name()+"-progressive-static");
    CompareRaster(originals[backend],Paint(api,page.get(),width,height,true),artifacts,name+"-"+writer->name()+"-cached");
    const auto saved=SaveDocument(api,document.get(),artifacts,name+"-"+writer->name());ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());const auto& read=reader->api();
      Handle reloaded(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reloaded.get(),nullptr);
      ASSERT_EQ(read.FPDF_GetPageCount(reloaded.get()),1);
      Handle loaded_page(read.FPDF_LoadPage(reloaded.get(),0),read.FPDF_ClosePage);ASSERT_NE(loaded_page.get(),nullptr);
      EXPECT_FLOAT_EQ(read.FPDF_GetPageWidthF(loaded_page.get()),width);EXPECT_FLOAT_EQ(read.FPDF_GetPageHeightF(loaded_page.get()),height);
      const auto raster=Paint(read,loaded_page.get(),width,height,true);
      ASSERT_EQ(raster.pixels.size(),expected.pixels.size());
      const auto label=name+"-"+writer->name()+"-"+reader->name()+"-reload";
      CheckGolden(*reader,expected,raster,artifacts,label+"-golden");
      if(MissingColorspaceCase(name)) {
        // Reader semantics differ only in the documented native opacity paths.
        // Both writers must preserve the original reader-specific appearance.
        const auto reader_original=reader==writer?originals[backend]:AlphaControl(read,reader->name()=="native");
        CompareRaster(reader_original,raster,artifacts,label,1);
      } else CompareRaster(originals[backend],raster,artifacts,label,1);
      CompareRaster(raster,Paint(read,loaded_page.get(),width,height,false),artifacts,label+"-static");
    }
    ++backend;
  }
  if(MissingColorspaceCase(name)) {
    const auto implementations=Implementations();
    const auto native_control=AlphaControl(implementations[0]->api(),false);
    const auto rust_control=AlphaControl(implementations[1]->api(),false);
    CompareRaster(native_control,rust_control,artifacts,name+"-shared-correct-explicit-mask-control",1);
    CompareRaster(native_control,originals[1],artifacts,name+"-correct-rust-inline-jpx",1);
  } else CompareRaster(originals[0],originals[1],artifacts,name+"-native-rust",1);
}

TEST(CoveragePinnedPixels, Type3CharProgramsRetainPinnedGlyphAppearance) {RunPinned("type3",200,150);}
TEST(CoveragePinnedPixels, Type3ImageXObjectUsesTheGlyphResourceDictionary) {RunPinned("type3_xobject",200,200);}
TEST(CoveragePinnedPixels, AllShadingTypesAndFunctionArraysRetainPinnedClippedCells) {RunPinned("shade",400,900);}
TEST(CoveragePinnedPixels, TensorAndCoonsMeshPatchBoundariesRetainPinnedAppearance) {RunPinned("shade-tensor",200,500);}
TEST(CoveragePinnedPixels, AffineIndexedGrayColorAndMaskedImagesRetainPinnedSamples) {RunPinned("image_transformer_other",200,200);}
TEST(CoveragePinnedPixels, IccSrgbSignatureWithWrongComponentCountUsesPinnedRecovery) {RunPinned("icc_profile_bad_component",100,100);}
TEST(CoveragePinnedPixels, IccHugeColorOperandUsesPinnedNumericRecovery) {RunPinned("icc_profile_bad_value",100,100);}
TEST(CoveragePinnedPixels, JpxMissingPdfColorspaceInfersGrayRgbCmykAndAlphaTiles) {RunPinned("jpxdecode_without_colorspace",88,128);}
TEST(CoveragePinnedPixels, JpxMismatchedPdfColorspacesRecoverEveryOriginalTile) {RunPinned("jpxdecode_with_mismatch_colorspace",88,128);}
TEST(CoveragePinnedPixels, JpxPremultipliedEmbeddedOpacityRetainsAllSixPinnedTiles) {RunPinned("jpxdecode_smask_in_data_2",88,128);}

} // namespace
} // namespace pdfoxium::integration
