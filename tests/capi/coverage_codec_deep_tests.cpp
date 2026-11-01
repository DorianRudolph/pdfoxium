#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <fstream>
#include <functional>
#include <iterator>
#include <map>
#include <string>
#include <utility>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using Color = std::array<int, 3>;  // RGB, independently expected display values.

void Put(Bytes& out, uint64_t value, size_t count, bool little = false) {
  for (size_t i = 0; i < count; ++i)
    out.push_back(static_cast<uint8_t>(value >> (8 * (little ? i : count - 1 - i))));
}
uint32_t Be32(const Bytes& data, size_t offset) {
  return static_cast<uint32_t>(data.at(offset)) << 24 | static_cast<uint32_t>(data.at(offset + 1)) << 16 |
      static_cast<uint32_t>(data.at(offset + 2)) << 8 | data.at(offset + 3);
}
Bytes Read(const std::string& path) {
  std::ifstream input(TestRoot() / path, std::ios::binary);
  EXPECT_TRUE(input.good()) << path;
  return {std::istreambuf_iterator<char>(input), std::istreambuf_iterator<char>()};
}
std::string Raw(const Bytes& bytes) { return {bytes.begin(), bytes.end()}; }
Bytes Zlib(const Bytes& bytes) {
  EXPECT_LT(bytes.size(), 65536u);
  Bytes out{0x78,0x01,0x01};Put(out,bytes.size(),2,true);Put(out,~bytes.size(),2,true);
  out.insert(out.end(),bytes.begin(),bytes.end());uint32_t a=1,b=0;
  for (auto value:bytes) { a=(a+value)%65521;b=(b+a)%65521; }
  Put(out,b<<16|a,4);return out;
}
struct Bits {
  Bytes out; uint8_t value=0; int count=0; bool jpeg=false;
  void Write(unsigned code,int width) {
    for(int bit=width-1;bit>=0;--bit) {
      value=static_cast<uint8_t>((value<<1)|((code>>bit)&1));
      if(++count==8) { out.push_back(value);if(jpeg&&value==255)out.push_back(0);count=0;value=0; }
    }
  }
  Bytes Finish(bool ones=false) {
    if(count)Write(ones?((1u<<(8-count))-1):0,8-count);
    return out;
  }
};
Color Pixel(const Raster& image,int x,int y) {
  const auto at=(static_cast<size_t>(y)*image.width+x)*4;
  return {image.pixels.at(at+2),image.pixels.at(at+1),image.pixels.at(at)};
}
void ExpectPixel(const Raster& image,int x,int y,Color expected,int delta=1) {
  const auto actual=Pixel(image,x,y);
  for(size_t c=0;c<3;++c)EXPECT_NEAR(actual[c],expected[c],delta)<<"pixel="<<x<<","<<y<<" channel="<<c;
}
void ExpectUniform(const Raster& image,Color expected,int delta=1) {
  int max_error=0;
  for(int y=0;y<image.height;++y)for(int x=0;x<image.width;++x) {
    const auto color=Pixel(image,x,y);
    for(size_t c=0;c<3;++c)max_error=std::max(max_error,std::abs(color[c]-expected[c]));
  }
  EXPECT_LE(max_error,delta)<<"independent uniform image color";
}
std::vector<uint8_t> ImagePdf(int width,int height,const std::string& attributes,const Bytes& bytes,
                              int declared_width=0,int declared_height=0,int page_width=0) {
  const auto w=std::to_string(width),h=std::to_string(height);
  const auto canvas=std::to_string(page_width?page_width:width);
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 "+canvas+" "+h+"] /Resources << /XObject << /I 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("","0.5 g 0 0 "+canvas+" "+h+" re f q "+w+" 0 0 "+h+" 0 0 cm /I Do Q"),
      PdfStream("/Type /XObject /Subtype /Image /Width "+std::to_string(declared_width?declared_width:width)+
          " /Height "+std::to_string(declared_height?declared_height:height)+" "+attributes,Raw(bytes))});
}
FPDF_PAGEOBJECT ImageObject(const Api& api,FPDF_PAGE page) {
  EXPECT_EQ(api.FPDFPage_CountObjects(page),2);
  const auto image=api.FPDFPage_GetObject(page,1);
  EXPECT_EQ(api.FPDFPageObj_GetType(image),FPDF_PAGEOBJ_IMAGE);return image;
}
Bytes Decoded(const Api& api,FPDF_PAGEOBJECT image) {
  const auto size=api.FPDFImageObj_GetImageDataDecoded(image,nullptr,0);Bytes result(size);
  EXPECT_EQ(api.FPDFImageObj_GetImageDataDecoded(image,result.data(),size),size);return result;
}
bool IsNative(const Api& api) {
  for (const auto* library : Implementations()) {
    if (&library->api() == &api) return library->name() == "native";
  }
  ADD_FAILURE() << "unregistered API";
  return false;
}
using Inspect = std::function<void(const Api&,FPDF_PAGE,const Raster&)>;
void ComparePdf(const std::string& label,const std::vector<uint8_t>& bytes,int width,int height,
                const Inspect& inspect,bool save=false,int channel_delta=1,
                std::array<int,4> compare_area={0,0,0,0}) {
  Artifacts artifacts;artifacts.Pdf(label+"-input",bytes);std::array<Raster,2> originals;size_t backend=0;
  const auto compare = [&](const Raster& before, const Raster& after, const std::string& name) {
    if (compare_area[2] > 0) {
      const auto [left, top, w, h] = compare_area;
      CompareRaster(CropRaster(before,left,top,w,h),CropRaster(after,left,top,w,h),artifacts,name,channel_delta);
    } else {
      CompareRaster(before,after,artifacts,name,channel_delta);
    }
  };
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());const auto& api=writer->api();
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    {Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      originals[backend]=Render(api,page.get(),width,height,FPDF_RENDER_NO_SMOOTHIMAGE);
      ASSERT_NO_FATAL_FAILURE(inspect(api,page.get(),originals[backend]));}
    if(save) {
      const auto saved=SaveDocument(api,doc.get(),artifacts,writer->name()+"-"+label);ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());const auto& read=reader->api();
        Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);
        Handle page(read.FPDF_LoadPage(reopened.get(),0),read.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
        const auto after=Render(read,page.get(),width,height,FPDF_RENDER_NO_SMOOTHIMAGE);
        ASSERT_NO_FATAL_FAILURE(inspect(read,page.get(),after));
        compare(originals[backend],after,writer->name()+"-"+label+"-to-"+reader->name());
      }
    }
    ++backend;
  }
  compare(originals[0],originals[1],label+"-backends");
}

void Segment(Bytes& out,uint8_t marker,const Bytes& payload) {
  out.insert(out.end(),{255,marker});Put(out,payload.size()+2,2);out.insert(out.end(),payload.begin(),payload.end());
}
// Test-only baseline JPEG serialization: constant 8x8 DCT blocks, quantizer1,
// DC categories0/7/11 and AC EOB. No external encoder or private engine call.
// All 128x8 components are MCU-aligned, so 2x/4x/8x decoder reductions are
// enabled. Constant DCT samples avoid IDCT-vs-box thumbnail quality ambiguity.
Bytes ConstantJpeg(int width,int height,int components) {
  Bytes out{255,216};
  if(components==3)Segment(out,0xee,Bytes{'A','d','o','b','e',0,100,0,0,0,0,0});
  Bytes quant(65,1);quant[0]=0;Segment(out,0xdb,quant);
  Bytes frame{8};Put(frame,height,2);Put(frame,width,2);frame.push_back(components);
  for(int c=0;c<components;++c)frame.insert(frame.end(),{static_cast<uint8_t>(c+1),0x11,0});
  Segment(out,0xc0,frame);
  Bytes huffman{0,1,1,1};huffman.resize(17,0);huffman.insert(huffman.end(),{0,7,11,0x10,1});
  huffman.resize(37,0);huffman.push_back(0);Segment(out,0xc4,huffman);
  Bytes scan{static_cast<uint8_t>(components)};
  for(int c=0;c<components;++c)scan.insert(scan.end(),{static_cast<uint8_t>(c+1),0});
  scan.insert(scan.end(),{0,63,0});Segment(out,0xda,scan);
  Bits bits;bits.jpeg=true;std::vector<int> previous(components,0);
  for(int block=0;block<((width+7)/8)*((height+7)/8);++block)for(int c=0;c<components;++c) {
    const int coefficient=components==4?-1024:(components==1?64:(c==0?64:(c==2?-64:0)));
    const int delta=coefficient-previous[c];previous[c]=coefficient;
    const int category=delta==0?0:(std::abs(delta)<=127?7:11);
    if(category==0)bits.Write(0,1);else if(category==7)bits.Write(2,2);else bits.Write(6,3);
    if(category)bits.Write(delta>=0?delta:((1<<category)-1+delta),category);
    bits.Write(0,1); // EOB; all 63 AC coefficients are zero.
  }
  const auto entropy=bits.Finish(true);out.insert(out.end(),entropy.begin(),entropy.end());out.insert(out.end(),{255,217});return out;
}
Color JpegColor(int components) { return components==1?Color{136,136,136}:components==3?Color{136,128,120}:Color{255,255,255}; }
std::string JpegAttributes(int components) {
  return "/BitsPerComponent 8 /ColorSpace /"+std::string(components==1?"DeviceGray":components==3?"DeviceRGB":"DeviceCMYK")+" /Filter /DCTDecode";
}
TEST(CoverageCodecDeep, JpegAlignedOneThreeAndFourComponentReductionsKeepKnownDctColors) {
  for(int components:{1,3,4})for(int scale:{1,2,4,8}) {
    SCOPED_TRACE(components);
    SCOPED_TRACE(scale);
    const auto payload=ConstantJpeg(128,8,components);const auto bytes=ImagePdf(128,8,JpegAttributes(components),payload);
    ComparePdf("jpeg-constant-"+std::to_string(components)+"-scale"+std::to_string(scale),bytes,128/scale,8/scale,
      [=](const Api& api,FPDF_PAGE page,const Raster& raster) {
        const auto image=ImageObject(api,page);unsigned w=0,h=0;ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(image,&w,&h));EXPECT_EQ(w,128u);EXPECT_EQ(h,8u);
        ExpectUniform(raster,JpegColor(components));
      },components==3&&scale==4);
  }
  // A partial MCU disables reduction, despite the small requested canvas.
  for(int components:{1,3,4}) {
    ComparePdf("jpeg-padded-mcu-"+std::to_string(components),ImagePdf(127,7,JpegAttributes(components),ConstantJpeg(127,7,components)),16,1,
      [=](const Api& api,FPDF_PAGE page,const Raster& raster){ImageObject(api,page);ExpectUniform(raster,JpegColor(components));});
  }
}

TEST(CoverageCodecDeep, TwoComponentDeviceNJpegRetainsRawTintsThroughRenderAndSave) {
  // The two DC-only JPEG components yield136/255 and128/255. The Type4
  // function appends zero, so the independent DeviceRGB result is 136,128,0.
  // Treating this payload as RGB, or omitting an unsupported image, cannot
  // satisfy that color. Both the declared components and JPEG SOF count are 2.
  const auto payload = ConstantJpeg(128, 8, 2);
  const auto bytes = PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 8] "
      "/Resources << /XObject << /I 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", "0.5 g 0 0 128 8 re f q 128 0 0 8 0 0 cm /I Do Q"),
      PdfStream("/Type /XObject /Subtype /Image /Width 128 /Height 8 "
                "/BitsPerComponent 8 /Filter /DCTDecode "
                "/ColorSpace [/DeviceN [/SpotA /SpotB] /DeviceRGB 6 0 R]",
                Raw(payload)),
      PdfStream("/FunctionType 4 /Domain [0 1 0 1] /Range [0 1 0 1 0 1]", "{ 0 }")});
  for (int scale : {1, 2, 4, 8}) {
    SCOPED_TRACE(scale);
    ComparePdf("jpeg-two-component-devicen-scale" + std::to_string(scale),
               bytes, 128 / scale, 8 / scale,
               [](const Api& api, FPDF_PAGE page, const Raster& raster) {
                 const auto image = ImageObject(api, page);
                 unsigned width = 0, height = 0;
                 ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(image, &width, &height));
                 EXPECT_EQ(width, 128u);
                 EXPECT_EQ(height, 8u);
                 ExpectUniform(raster, {136, 128, 0});
               }, true);
  }
}

TEST(CoverageCodecDeep, JpegPrefixTrailerAndDeclaredSizeRecoveryPreserveTheImage) {
  const auto original=ConstantJpeg(128,8,3);std::vector<std::pair<std::string,Bytes>> cases{{"complete",original}};
  auto prefixed=original;prefixed.insert(prefixed.begin(),{'j','u','n','k',0,0});cases.push_back({"leading-soi-scan",prefixed});
  auto no_eoi=original;no_eoi.resize(no_eoi.size()-2);auto bad_trailer=no_eoi;bad_trailer.insert(bad_trailer.end(),{0,0});
  cases.push_back({"replace-non-eoi-trailer",bad_trailer});
  for(const auto& item:cases) {
    SCOPED_TRACE(item.first);
    ComparePdf("jpeg-repair-"+item.first,ImagePdf(128,8,JpegAttributes(3),item.second),128,8,
      [](const Api& api,FPDF_PAGE page,const Raster& raster){ImageObject(api,page);ExpectUniform(raster,JpegColor(3));},true);
  }
  // LibjpegScanlineDecoder::PatchUpTrailer replaces the last two source
  // bytes instead of appending EOI. Here those bytes contain the final two
  // constant MCUs, which native libjpeg leaves neutral after replacement.
  // Rust's permissive decoder recovers those MCUs. This malformed stream has
  // no common complete-image oracle; all earlier MCUs must agree exactly.
  ComparePdf("jpeg-missing-eoi-entropy-recovery",ImagePdf(128,8,JpegAttributes(3),no_eoi),128,8,
    [](const Api& api,FPDF_PAGE page,const Raster& raster) {
      ImageObject(api,page);
      ExpectUniform(CropRaster(raster,0,0,112,8),JpegColor(3));
      ExpectUniform(CropRaster(raster,112,0,16,8),IsNative(api)?Color{128,128,128}:JpegColor(3));
    },true,1,{0,0,112,8});
  for(int declared:{64,256}) {
    SCOPED_TRACE(declared);
    ComparePdf("jpeg-dictionary-width-"+std::to_string(declared),ImagePdf(128,8,JpegAttributes(3),original,declared,8),128,8,
      [=](const Api& api,FPDF_PAGE page,const Raster& raster){const auto image=ImageObject(api,page);unsigned w=0,h=0;ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(image,&w,&h));EXPECT_EQ(w,static_cast<unsigned>(declared));EXPECT_EQ(h,8u);ExpectUniform(raster,JpegColor(3));});
  }
  auto truncated=original;truncated.resize(20);
  ComparePdf("jpeg-short-payload-isolated",ImagePdf(128,8,JpegAttributes(3),truncated),128,8,
      [](const Api& api,FPDF_PAGE page,const Raster& raster){ImageObject(api,page);ExpectUniform(raster,{128,128,128});});
}

Bytes Box(const char* type,const Bytes& payload,bool extended=false) {
  Bytes out;Put(out,extended?1:payload.size()+8,4);for(int i=0;i<4;++i)out.push_back(type[i]);
  if (extended) {
    Put(out,payload.size()+16,8);
  }
  out.insert(out.end(),payload.begin(),payload.end());
  return out;
}
void AppendBox(Bytes& out,const char* type,const Bytes& payload) {const auto box=Box(type,payload);out.insert(out.end(),box.begin(),box.end());}
enum class PaletteMode { Rgb, Reorder, Rgba, DirectFirst, Indexed, SignedBlue, Extended, EmptyPalette, BadComponent, IncompleteDefinition, TruncatedPalette };
Bytes PaletteJp2(PaletteMode mode) {
  const auto source=Read("upstream/pdfium/testing/resources/gray.jp2");Bytes result;
  for(size_t offset=0;offset+8<=source.size();) {
    const size_t length=Be32(source,offset);if(length<8||length>source.size()-offset){ADD_FAILURE()<<"bounded original JP2 box";return {};}
    const std::string type(source.begin()+offset+4,source.begin()+offset+8);
    if(type!="jp2h")result.insert(result.end(),source.begin()+offset,source.begin()+offset+length);
    else {
      Bytes header;
      for(size_t inner=offset+8;inner+8<=offset+length;) {
        const size_t count=Be32(source,inner);if(count<8||count>offset+length-inner){ADD_FAILURE()<<"bounded original JP2 image header";return {};}
        const std::string name(source.begin()+inner+4,source.begin()+inner+8);
        if (name != "colr") {
          header.insert(header.end(),source.begin()+inner,source.begin()+inner+count);
        }
        inner += count;
      }
      Bytes color{1,0,0};Put(color,16,4);AppendBox(header,"colr",color);
      const int channels=mode==PaletteMode::Rgba?4:3;
      Bytes palette;Put(palette,mode==PaletteMode::EmptyPalette?0:256,2);palette.push_back(channels);
      for(int c=0;c<channels;++c)palette.push_back(mode==PaletteMode::SignedBlue&&c==2?0x87:7);
      for(int index=0;index<256;++index) {
        palette.push_back(index);palette.push_back(255-index);palette.push_back(index/2);
        if(channels==4)palette.push_back(index==0?255:128);
      }
      if(mode==PaletteMode::TruncatedPalette)palette.resize(8);
      AppendBox(header,"pclr",palette);Bytes mapping;
      for(int c=0;c<channels;++c) {
        Put(mapping,mode==PaletteMode::BadComponent?99:0,2);
        mapping.push_back(mode==PaletteMode::DirectFirst&&c==0?0:1);mapping.push_back(mode==PaletteMode::DirectFirst&&c==0?0:c);
      }
      AppendBox(header,"cmap",mapping);
      if(mode==PaletteMode::Reorder||mode==PaletteMode::Rgba||mode==PaletteMode::IncompleteDefinition) {
        Bytes definitions;Put(definitions,mode==PaletteMode::IncompleteDefinition?1:channels,2);
        const int count=mode==PaletteMode::IncompleteDefinition?1:channels;
        for(int c=0;c<count;++c) {
          Put(definitions,mode==PaletteMode::Reorder?(c==0?2:c==1?0:1):c,2);
          Put(definitions,c==3?1:0,2);Put(definitions,c==3?0:c+1,2);
        }
        AppendBox(header,"cdef",definitions);
      }
      const auto box=Box("jp2h",header,mode==PaletteMode::Extended);result.insert(result.end(),box.begin(),box.end());
    }
    offset+=length;
  }
  return result;
}
Color PaletteColor(PaletteMode mode,int value,bool native=false) {
  if(mode==PaletteMode::Indexed)return value==0?Color{0,0,255}:Color{255,255,0};
  Color color{value,255-value,value/2};
  if(mode==PaletteMode::Reorder)color={color[2],color[0],color[1]};
  if(mode==PaletteMode::SignedBlue)color[2]+=128;
  if(mode==PaletteMode::Rgba&&value!=0)for(auto& channel:color) {
    // SMaskInData1 is unassociated RGB and opacity: Rust retains straight
    // RGB and blends it once. Native ConvertArgbJpxBitmapToRgb first blends
    // it over white, then applies the same inline opacity again; LoadMask's
    // disabled Matte sentinel does not undo that native white premultiplication.
    const int rgb=native?(channel*128+255*127)/255:channel;
    channel=(rgb*128+128*127)/255;
  }
  return color;
}
std::string IndexedPalette() {
  constexpr char hex[]="0123456789ABCDEF";std::string result;
  for(int i=0;i<256;++i)for(int value:{i,i,255-i}){result+=hex[value>>4];result+=hex[value&15];}
  return " /ColorSpace [/Indexed /DeviceRGB 255 <"+result+">]";
}
TEST(CoverageCodecDeep, Jp2PaletteMappingsChannelDefinitionsAndIndexedBypassRenderKnownColors) {
  for(auto mode:{PaletteMode::Rgb,PaletteMode::Reorder,PaletteMode::Rgba,PaletteMode::DirectFirst,PaletteMode::Indexed,PaletteMode::SignedBlue,PaletteMode::Extended}) {
    const auto label="jp2-palette-mode"+std::to_string(static_cast<int>(mode));SCOPED_TRACE(label);
    const auto attributes="/BitsPerComponent 8 /Filter /JPXDecode"+std::string(mode==PaletteMode::Rgba?" /SMaskInData 1":"")+(mode==PaletteMode::Indexed?IndexedPalette():"");
    ComparePdf(label,ImagePdf(4,4,attributes,PaletteJp2(mode)),80,80,
      [=](const Api& api,FPDF_PAGE page,const Raster& raster) {
        const auto image=ImageObject(api,page);unsigned w=0,h=0;ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(image,&w,&h));EXPECT_EQ(w,4u);EXPECT_EQ(h,4u);
        for(int row=0;row<4;++row)for(int col=0;col<4;++col)
          ExpectUniform(CropRaster(raster,col*20+2,row*20+2,16,16),
              PaletteColor(mode,((col<2)==(row<2))?0:255,IsNative(api)),1);
      },mode==PaletteMode::Rgba||mode==PaletteMode::Reorder,1,
      // The opaque quadrant retains a common native/Rust raster oracle;
      // all translucent interiors use the independent per-reader formulas.
      mode==PaletteMode::Rgba?std::array<int,4>{0,0,40,40}:std::array<int,4>{0,0,0,0});
  }
}
TEST(CoverageCodecDeep, Jp2InvalidPaletteAndDefinitionPacketsFailWithinTheirImage) {
  for(auto mode:{PaletteMode::EmptyPalette,PaletteMode::BadComponent,PaletteMode::IncompleteDefinition,PaletteMode::TruncatedPalette}) {
    const auto label="jp2-bounded-metadata-"+std::to_string(static_cast<int>(mode));SCOPED_TRACE(label);
    if (mode == PaletteMode::BadComponent) {
      // CMAP component99 is invalid for the one-component codestream. Rust
      // rejects it. The packaged native decoder permissively recovers the
      // original palette, despite pinned OpenJPEG check_color rejecting it.
      // Malformed recovery is not a normative native correctness oracle:
      // accept either bounded palette recovery or omission, verify the Rust
      // omission, and compare a separate gray control strip in all readers.
      ComparePdf(label,ImagePdf(4,4,"/BitsPerComponent 8 /Filter /JPXDecode",PaletteJp2(mode),0,0,6),60,40,
        [](const Api& api,FPDF_PAGE page,const Raster& raster) {
          ImageObject(api,page);
          ExpectUniform(CropRaster(raster,40,0,20,40),{128,128,128});
          if (!IsNative(api)) {
            ExpectUniform(CropRaster(raster,0,0,40,40),{128,128,128});
          } else {
            const bool omitted=Pixel(raster,5,5)==Color{128,128,128};
            for(int row=0;row<4;++row)for(int col=0;col<4;++col)
              ExpectPixel(raster,col*10+5,row*10+5,omitted?Color{128,128,128}:
                  PaletteColor(PaletteMode::Rgb,((col<2)==(row<2))?0:255));
          }
        },true,1,{40,0,20,40});
    } else {
      ComparePdf(label,ImagePdf(4,4,"/BitsPerComponent 8 /Filter /JPXDecode",PaletteJp2(mode)),40,40,
        [](const Api& api,FPDF_PAGE page,const Raster& raster){ImageObject(api,page);ExpectUniform(raster,{128,128,128});});
    }
  }
}

// PDF LZW bit packing follows the wire-format next-code boundary; expected
// output is specified separately. In particular, a clear resets code width.
Bytes Lzw(const std::vector<unsigned>& codes,bool early) {
  Bits bits;unsigned next=258;int width=9;bool previous=false;
  for(auto code:codes) {
    bits.Write(code,width);
    if(code==256){next=258;width=9;previous=false;continue;}
    if(code==257)break;
    if(previous&&next<4096){++next;if(width<12&&next+unsigned(early)==(1u<<width))++width;}
    previous=true;
  }
  return bits.Finish();
}
Bytes LzwLiterals(const Bytes& bytes,bool early,int clear_every=0) {
  std::vector<unsigned> codes{256};for(size_t i=0;i<bytes.size();++i){if(clear_every&&i&&i%clear_every==0)codes.push_back(256);codes.push_back(bytes[i]);}codes.push_back(257);return Lzw(codes,early);
}
void ExpectGraySamples(const Raster& raster,const Bytes& expected,int width,int height) {
  ASSERT_EQ(expected.size(),static_cast<size_t>(width*height));ASSERT_EQ(raster.width,width);ASSERT_EQ(raster.height,height);
  int difference=0;for(int y=0;y<height;++y)for(int x=0;x<width;++x)for(auto color:Pixel(raster,x,y))difference=std::max(difference,std::abs(color-int(expected[y*width+x])));
  EXPECT_LE(difference,1)<<"decoded grayscale image samples";
}
TEST(CoverageCodecDeep, LzwReferencesKwkwkGrowthThroughTwelveBitsAndClearResetDecodeExactly) {
  for(bool early:{false,true}) {
    std::vector<std::pair<std::vector<unsigned>,Bytes>> short_cases{
      {{256,42,258,259,260,261,262,263,257},Bytes(28,42)},
      {{256,65,66,258,260,259,257},Bytes{'A','B','A','B','A','B','A','B','A'}}};
    for(size_t i=0;i<short_cases.size();++i) {
      const auto& item=short_cases[i];const int width=i?3:7,height=i?3:4;
      const auto label="lzw-references-"+std::to_string(early)+"-"+std::to_string(i);SCOPED_TRACE(label);
      ComparePdf(label,ImagePdf(width,height,"/BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /LZWDecode /DecodeParms << /EarlyChange "+std::to_string(early)+" >>",Lzw(item.first,early)),width,height,
        [=](const Api& api,FPDF_PAGE page,const Raster& raster){EXPECT_EQ(Decoded(api,ImageObject(api,page)),item.second);ExpectGraySamples(raster,item.second,width,height);},true);
    }
    for(int clear_every:{0,2048}) {
      Bytes expected(8192);for(size_t i=0;i<expected.size();++i)expected[i]=static_cast<uint8_t>(i*37+i/128*11);
      const auto label="lzw-wide-codes-"+std::to_string(early)+"-clear"+std::to_string(clear_every);SCOPED_TRACE(label);
      ComparePdf(label,ImagePdf(128,64,"/BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /LZWDecode /DecodeParms << /EarlyChange "+std::to_string(early)+" >>",LzwLiterals(expected,early,clear_every)),128,64,
        [=](const Api& api,FPDF_PAGE page,const Raster& raster){EXPECT_EQ(Decoded(api,ImageObject(api,page)),expected);ExpectGraySamples(raster,expected,128,64);},clear_every==2048);
    }
  }
}

TEST(CoverageCodecDeep, PredictorRowsWithDifferentImageWidthsCarryLeftoverSamplesAcrossScanlines) {
  for(bool lzw:{false,true})for(bool tiff:{false,true})for(bool wider:{false,true}) {
    const int width=wider?2:3,height=wider?3:2,columns=wider?3:1;
    const Bytes expected=wider?Bytes{1,2,3,2,4,6}:Bytes{1,2,3,4,5,6};
    const Bytes raw=tiff?(wider?Bytes{1,1,1,2,2,2}:expected):
        (wider?Bytes{1,1,1,1,1,2,2,2}:Bytes{0,1,1,2,2,1,3,3,4,1,0,6});
    const auto encoded=lzw?LzwLiterals(raw,true):Zlib(raw);
    const auto parameters=" /DecodeParms << /Predictor "+std::to_string(tiff?2:15)+" /Columns "+std::to_string(columns)+" /Colors 1 /BitsPerComponent 8 >>";
    const auto label="predictor-leftovers-"+std::to_string(lzw)+std::to_string(tiff)+std::to_string(wider);SCOPED_TRACE(label);
    ComparePdf(label,ImagePdf(width,height,"/BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /"+std::string(lzw?"LZWDecode":"FlateDecode")+parameters,encoded),width,height,
      [=](const Api& api,FPDF_PAGE page,const Raster& raster){EXPECT_EQ(Decoded(api,ImageObject(api,page)),expected);ExpectGraySamples(raster,expected,width,height);},!lzw&&!tiff);
  }
}

TEST(CoverageCodecDeep, RunLengthPacketsCrossRowsAndBoundTruncatedLiteralPadding) {
  struct Case {const char* name;int width,height;Bytes encoded,decoded,visible;};
  Bytes literal{127};for(int i=0;i<128;++i)literal.push_back(i);literal.push_back(128);
  Bytes all(128);for(int i=0;i<128;++i)all[i]=i;
  const std::vector<Case> cases{
      {"mixed-packets",5,2,{254,50,4,10,20,30,40,50,255,90,128},{50,50,50,10,20,30,40,50,90,90},{50,50,50,10,20,30,40,50,90,90}},
      {"repeat-across-rows-no-eoi",3,2,{251,42},Bytes(6,42),Bytes(6,42)},
      {"maximum-repeat",32,4,{129,100,128},Bytes(128,100),Bytes(128,100)},
      {"maximum-literal-and-unused-tail",31,4,literal,all,Bytes(all.begin(),all.begin()+124)},
      {"truncated-literal",3,2,{5,10,20,30},{10,20,30,0,0,0},{10,20,30,0,0,0}}};
  for(const auto& item:cases) {
    SCOPED_TRACE(item.name);
    ComparePdf(std::string("run-length-")+item.name,ImagePdf(item.width,item.height,"/BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /RunLengthDecode",item.encoded),item.width,item.height,
      [=](const Api& api,FPDF_PAGE page,const Raster& raster){EXPECT_EQ(Decoded(api,ImageObject(api,page)),item.decoded);ExpectGraySamples(raster,item.visible,item.width,item.height);},true);
  }
}

// Minimal classic TIFF writer with SHORT/LONG/RATIONAL tags. Independent tiny
// source-derived color samples exercise the XFA image adapter, not a private API.
struct Tag {uint16_t kind;std::vector<uint32_t> values;};
Tag Short(std::initializer_list<uint32_t> values){return {3,values};}
Tag Long(std::initializer_list<uint32_t> values){return {4,values};}
Tag Rational(std::initializer_list<uint32_t> pairs){return {5,pairs};}
Bytes TagData(const Tag& tag){Bytes out;for(auto value:tag.values)Put(out,value,tag.kind==3?2:4,true);return out;}
Bytes TiffColor(int bits,int samples,int photo,const Bytes& pixels,std::map<uint16_t,Tag> extra={}) {
  std::map<uint16_t,Tag> tags{{256,Long({1})},{257,Long({1})},{258,{3,std::vector<uint32_t>(samples,bits)}},{259,Short({1})},{262,Short({static_cast<uint32_t>(photo)})},{273,Long({0})},{277,Short({static_cast<uint32_t>(samples)})},{278,Long({1})},{279,Long({static_cast<uint32_t>(pixels.size())})},{284,Short({1})}};
  for(const auto& entry:extra)tags[entry.first]=entry.second;
  const size_t table=8+2+12*tags.size()+4;size_t start=table;for(const auto& entry:tags){const auto n=TagData(entry.second).size();if(n>4)start+=n;}tags[273]=Long({static_cast<uint32_t>(start)});
  Bytes out{'I','I',42,0,8,0,0,0};Put(out,tags.size(),2,true);Bytes tail;
  for(const auto& entry:tags){Put(out,entry.first,2,true);Put(out,entry.second.kind,2,true);Put(out,entry.second.values.size()/(entry.second.kind==5?2:1),4,true);const auto bytes=TagData(entry.second);
    if(bytes.size()<=4){out.insert(out.end(),bytes.begin(),bytes.end());out.resize(out.size()+4-bytes.size(),0);}else{Put(out,table+tail.size(),4,true);tail.insert(tail.end(),bytes.begin(),bytes.end());}}
  Put(out,0,4,true);out.insert(out.end(),tail.begin(),tail.end());out.insert(out.end(),pixels.begin(),pixels.end());return out;
}
std::string Base64(const Bytes& bytes) {
  constexpr char table[]="ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";std::string out;
  for(size_t i=0;i<bytes.size();i+=3){const uint32_t value=uint32_t(bytes[i])<<16|(i+1<bytes.size()?uint32_t(bytes[i+1])<<8:0)|(i+2<bytes.size()?bytes[i+2]:0);out+=table[value>>18&63];out+=table[value>>12&63];out+=i+1<bytes.size()?table[value>>6&63]:'=';out+=i+2<bytes.size()?table[value&63]:'=';}return out;
}
struct TiffCase {std::string name;Bytes data;Color low,high;bool save=false;bool native_unsupported=false;};
void RunTiff(const TiffCase& item) {
  const auto image="<image contentType=\"image/tiff\" transferEncoding=\"base64\" aspect=\"none\">"+Base64(item.data)+"</image>";
  const auto body="<draw w=\"80pt\" h=\"80pt\"><value>"+image+"</value></draw><field name=\"picture\" w=\"80pt\" h=\"80pt\"><ui><imageEdit/></ui><value>"+image+"</value></field>";
  const auto bytes=forms::XfaFixture(true,"",body);Artifacts artifacts;std::array<Raster,2> scenes;size_t backend=0;
  const auto inspect=[&](const Raster& raster,bool native) {
    for(int top:{40,120}) {
      if (native && item.native_unsupported) {
        ExpectUniform(CropRaster(raster,2,top+2,76,76),{255,255,255});
      } else {
        int outliers=0;
        for(int y=top+2;y<top+78;++y)for(int x=2;x<78;++x) {
          const auto color=Pixel(raster,x,y);
          for(size_t c=0;c<3;++c)if(color[c]<item.low[c]||color[c]>item.high[c])++outliers;
        }
        EXPECT_EQ(outliers,0)<<"independently expected TIFF interior";
      }
    }
  };
  const auto compare=[&](const Raster& expected,const Raster& actual,const std::string& name,bool different_reader) {
    if(item.native_unsupported && different_reader) {
      // Native libtiff PickContigCase only installs a separated-CMYK converter
      // for8bit samples. Assert each reader's image independently and keep
      // strict native/Rust comparisons for the live form and control area.
      // Rows0..19 contain only the solid red/white control. Rows20..39
      // contain the unrelated amount text, whose font edge raster is not a
      // codec oracle; its actual value/focus behavior is asserted separately.
      CompareRaster(CropRaster(expected,0,0,200,20),CropRaster(actual,0,0,200,20),artifacts,name+"-control",2);
      CompareRaster(CropRaster(expected,80,40,120,160),CropRaster(actual,80,40,120,160),artifacts,name+"-outside-images",2);
      CompareRaster(CropRaster(expected,0,200,200,100),CropRaster(actual,0,200,200,100),artifacts,name+"-below-images",2);
    } else {
      CompareRaster(CropRaster(expected,0,40,80,160),CropRaster(actual,0,40,80,160),artifacts,name,2);
    }
  };
  const auto inspect_control=[](forms::Session& session,bool native_writer=false) {
    ASSERT_TRUE(session.Focus(5,25));
    // Native XFA serialization prefixes dataset values with one LF. Keep
    // the serialized-value boundary explicit; originals/Rust saves stay exact.
    EXPECT_EQ(session.Text(),native_writer?u"\nold":u"old");
    session.api.FORM_ForceToKillFocus(session.form);
  };
  for(const auto* writer:Implementations()){
    SCOPED_TRACE(writer->name());forms::Session session(writer->api(),bytes,true,true);ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(inspect_control(session));
    scenes[backend]=session.Draw(200,300);inspect(scenes[backend],IsNative(writer->api()));
    if(item.save){const auto saved=SaveDocument(session.api,session.document,artifacts,writer->name()+"-"+item.name);for(const auto* reader:Implementations()){
      SCOPED_TRACE(reader->name());forms::Session reopened(reader->api(),saved,true,true);ASSERT_TRUE(reopened.valid());ASSERT_TRUE(reopened.xfa_loaded);
      ASSERT_NO_FATAL_FAILURE(inspect_control(reopened,IsNative(writer->api())));
      const auto after=reopened.Draw(200,300);inspect(after,IsNative(reader->api()));
      compare(scenes[backend],after,writer->name()+"-"+item.name+"-to-"+reader->name(),writer!=reader);}}
    ++backend;
  }
  compare(scenes[0],scenes[1],item.name+"-backends",true);
}
TEST(CoverageCodecDeep, TiffCieLabSixteenBitCmykAndRationalColorMetadataRenderIndependently) {
  const auto neutral=Rational({299,1000,587,1000,114,1000});
  const auto limited=Rational({16,1,235,1,128,1,240,1,128,1,240,1});
  std::vector<TiffCase> cases;
  for(int bits:{8,16}){
    const Bytes black(bits==8?3:6,0),white=bits==8?Bytes{255,0,0}:Bytes{255,255,0,0,0,0};
    cases.push_back({"tiff-lab-black"+std::to_string(bits),TiffColor(bits,3,8,black),{0,0,0},{0,0,0}});
    // The native display matrix uses D50 white: red/green clip near255 while
    // blue remains216..230. This independent range distinguishes D50 from an
    // unconverted L-byte or a missing/white image.
    cases.push_back({"tiff-lab-d50-white"+std::to_string(bits),TiffColor(bits,3,8,white),{246,246,216},{255,255,230},bits==16});
    cases.push_back({"tiff-lab-d65-white"+std::to_string(bits),TiffColor(bits,3,8,white,{{318,Rational({3127,10000,3290,10000})}}),{245,245,245},{255,255,255}});
  }
  cases.push_back({"tiff-cmyk8-red-control",TiffColor(8,4,5,{0,255,255,0}),{255,0,0},{255,0,0},true});
  cases.push_back({"tiff-cmyk8-black-control",TiffColor(8,4,5,{0,0,0,255}),{0,0,0},{0,0,0}});
  cases.push_back({"tiff-cmyk16-red",TiffColor(16,4,5,{0,0,255,255,255,255,0,0}),{255,0,0},{255,0,0},true,true});
  cases.push_back({"tiff-cmyk16-black",TiffColor(16,4,5,{0,0,0,0,0,0,255,255}),{0,0,0},{0,0,0},true,true});
  cases.push_back({"tiff-ycc-limited-white",TiffColor(8,3,6,{235,128,128},{{530,Short({1,1})},{529,neutral},{532,limited}}),{255,255,255},{255,255,255},true});
  cases.push_back({"tiff-ycc-limited-black",TiffColor(8,3,6,{16,128,128},{{530,Short({1,1})},{529,neutral},{532,limited}}),{0,0,0},{0,0,0}});
  for(const auto& item:cases){SCOPED_TRACE(item.name);RunTiff(item);}
}

}  // namespace
}  // namespace pdfoxium::integration
