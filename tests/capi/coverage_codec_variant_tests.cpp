#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <cstdint>
#include <cmath>
#include <map>
#include <string>
#include <utility>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using Pixel = std::array<int, 4>;  // Blue, green, red, alpha on the white page.

void Put(Bytes& output, uint64_t value, size_t size, bool little = true) {
  for (size_t i = 0; i < size; ++i)
    output.push_back(static_cast<uint8_t>(value >> (8 * (little ? i : size - 1 - i))));
}
void At(Bytes& output, size_t offset, uint64_t value, size_t size) {
  for (size_t i = 0; i < size; ++i) output.at(offset + i) = static_cast<uint8_t>(value >> (8 * i));
}
std::string Base64(const Bytes& bytes) {
  constexpr char alphabet[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  std::string result;
  for (size_t i = 0; i < bytes.size(); i += 3) {
    const uint32_t word = static_cast<uint32_t>(bytes[i]) << 16 |
        (i + 1 < bytes.size() ? static_cast<uint32_t>(bytes[i + 1]) << 8 : 0) |
        (i + 2 < bytes.size() ? bytes[i + 2] : 0);
    result += alphabet[(word >> 18) & 63]; result += alphabet[(word >> 12) & 63];
    result += i + 1 < bytes.size() ? alphabet[(word >> 6) & 63] : '=';
    result += i + 2 < bytes.size() ? alphabet[word & 63] : '=';
  }
  return result;
}
Bytes StoredZlib(const Bytes& source) {
  EXPECT_LT(source.size(), 65536u);
  Bytes result{0x78, 0x01, 0x01};
  Put(result, source.size(), 2); Put(result, ~source.size(), 2);
  result.insert(result.end(), source.begin(), source.end());
  uint32_t a = 1, b = 0;
  for (const auto value : source) { a = (a + value) % 65521; b = (b + a) % 65521; }
  Put(result, b << 16 | a, 4, false);
  return result;
}
Pixel Gray(int value) { return {value, value, value, 255}; }
Pixel Rgb(int r, int g, int b) { return {b, g, r, 255}; }
std::vector<Pixel> Grays(std::initializer_list<int> values) {
  std::vector<Pixel> result; for (int value : values) result.push_back(Gray(value)); return result;
}

// Tiny real IFDs, adapted from the codec's source-derived tiff_tests.rs. These
// serialize the format only; neither implementation supplies decoded samples.
struct TiffField { uint16_t type; std::vector<uint64_t> values; };
using Tags = std::map<uint16_t, TiffField>;
TiffField S(std::initializer_list<uint64_t> values) { return {3, values}; }
TiffField L(std::initializer_list<uint64_t> values) { return {4, values}; }
Bytes FieldBytes(const TiffField& field, bool little) {
  Bytes result; for (auto value : field.values) Put(result, value, field.type == 3 ? 2 : 4, little); return result;
}
Bytes Tiff(int width, int height, int bits, int samples, int photo,
           Tags extra, const std::vector<Bytes>& chunks, bool little = true, bool big = false) {
  Tags tags{{256, L({static_cast<uint64_t>(width)})}, {257, L({static_cast<uint64_t>(height)})},
      {258, {3, std::vector<uint64_t>(samples, bits)}}, {259, S({1})},
      {277, S({static_cast<uint64_t>(samples)})}, {278, L({static_cast<uint64_t>(height)})}, {284, S({1})}};
  if (photo >= 0) tags[262] = S({static_cast<uint64_t>(photo)});
  for (const auto& entry : extra) tags[entry.first] = entry.second;
  const bool tiled = tags.count(322); if (tiled) tags.erase(278);
  const int offset_tag = tiled ? 324 : 273, count_tag = tiled ? 325 : 279;
  tags[offset_tag] = {4, std::vector<uint64_t>(chunks.size(), 0)};
  std::vector<uint64_t> lengths; for (const auto& chunk : chunks) lengths.push_back(chunk.size());
  tags[count_tag] = {4, lengths};
  const size_t header = big ? 16 : 8, count_size = big ? 8 : 2,
      entry_size = big ? 20 : 12, pointer = big ? 8 : 4;
  const size_t table_end = header + count_size + entry_size * tags.size() + pointer;
  size_t data_start = table_end;
  for (const auto& entry : tags) {
    const auto size = FieldBytes(entry.second, little).size(); if (size > pointer) data_start += size;
  }
  std::vector<uint64_t> offsets; size_t offset = data_start;
  for (const auto& chunk : chunks) { offsets.push_back(offset); offset += chunk.size(); }
  tags[offset_tag] = {4, offsets};
  Bytes result{static_cast<uint8_t>(little ? 'I' : 'M'), static_cast<uint8_t>(little ? 'I' : 'M')};
  Put(result, big ? 43 : 42, 2, little);
  if (big) { Put(result, 8, 2, little); Put(result, 0, 2, little); }
  Put(result, header, pointer, little); Put(result, tags.size(), count_size, little);
  Bytes tail;
  for (const auto& entry : tags) {
    Put(result, entry.first, 2, little); Put(result, entry.second.type, 2, little);
    Put(result, entry.second.values.size(), pointer, little);
    const auto bytes = FieldBytes(entry.second, little);
    if (bytes.size() <= pointer) {
      result.insert(result.end(), bytes.begin(), bytes.end()); result.resize(result.size() + pointer - bytes.size(), 0);
    } else {
      Put(result, table_end + tail.size(), pointer, little); tail.insert(tail.end(), bytes.begin(), bytes.end());
    }
  }
  Put(result, 0, pointer, little); result.insert(result.end(), tail.begin(), tail.end());
  EXPECT_EQ(result.size(), data_start);
  for (const auto& chunk : chunks) result.insert(result.end(), chunk.begin(), chunk.end());
  return result;
}

struct ImageCase {
  std::string label, mime;
  Bytes data;
  int width, height;
  std::vector<Pixel> pixels;
  bool reload = false;
  int pixel_delta = 1;
  std::vector<Pixel> native_pixels = {};
  std::string native_difference = {};
};
ImageCase NativeObservation(ImageCase item, std::vector<Pixel> expected, const std::string& reason) {
  item.native_pixels = std::move(expected); item.native_difference = reason; return item;
}
std::vector<uint8_t> ImageDocument(const ImageCase& item) {
  const auto image = "<image contentType=\"" + item.mime + "\" transferEncoding=\"base64\" aspect=\"none\">" + Base64(item.data) + "</image>";
  const auto extra = "<draw w=\"80pt\" h=\"80pt\"><value>" + image + "</value></draw>"
      "<field name=\"picture\" w=\"80pt\" h=\"80pt\"><bind match=\"none\"/><ui><imageEdit/></ui><value>" + image + "</value></field>"
      "<draw w=\"20pt\" h=\"12pt\"><value><rectangle><fill><color value=\"0,255,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
  return forms::XfaFixture(true, "", extra);
}
Pixel AtPixel(const Raster& image, int x, int y) {
  const size_t index = (static_cast<size_t>(y) * image.width + x) * 4;
  return {image.pixels.at(index), image.pixels.at(index + 1), image.pixels.at(index + 2), image.pixels.at(index + 3)};
}
void CheckSamples(const Raster& image, const ImageCase& item, bool native = false) {
  const auto& pixels = native && !item.native_pixels.empty() ? item.native_pixels : item.pixels;
  ASSERT_EQ(pixels.size(), static_cast<size_t>(item.width * item.height));
  for (int top : {40, 120}) {
    for (int y = 0; y < item.height; ++y) for (int x = 0; x < item.width; ++x) {
      const int px = static_cast<int>((x + 0.5) * 80 / item.width),
                py = top + static_cast<int>((y + 0.5) * 80 / item.height);
      const auto actual = AtPixel(image, px, py), expected = pixels[y * item.width + x];
      for (size_t channel = 0; channel < 4; ++channel) {
        // XFA always uses bilinear image interpolation. Rounding a source
        // cell's center to a device pixel moves it by at most half a device
        // pixel. Bound the resulting error from the independently known local
        // sample contrasts, rather than widening the cross-engine comparison.
        int horizontal = 0, vertical = 0;
        for (int next : {x - 1, x + 1}) if (next >= 0 && next < item.width)
          horizontal = std::max(horizontal, std::abs(expected[channel] - pixels[y * item.width + next][channel]));
        for (int next : {y - 1, y + 1}) if (next >= 0 && next < item.height)
          vertical = std::max(vertical, std::abs(expected[channel] - pixels[next * item.width + x][channel]));
        const int phase_error = static_cast<int>(std::ceil((horizontal * item.width + vertical * item.height) / 160.0));
        EXPECT_NEAR(actual[channel], expected[channel], item.pixel_delta + phase_error)
            << item.label << " cell=" << x << "," << y << " field-top=" << top << " channel=" << channel;
      }
    }
  }
  EXPECT_EQ(AtPixel(image, 8, 205), Rgb(0, 255, 0));
  EXPECT_EQ(AtPixel(image, 150, 205), Gray(255));
}
void CompareImages(const Raster& native, const Raster& rust, Artifacts& artifacts, const std::string& label) {
  // The image and sentinel contain no glyphs; color conversion/resampling has
  // a two-level rounding budget, with no exceptional pixels accepted.
  CompareRaster(CropRaster(native, 0, 40, 80, 172), CropRaster(rust, 0, 40, 80, 172),
                artifacts, label, RasterTolerance{2, 0, 0.2});
}
void CompareKnownImageDifference(const Raster& expected, const Raster& actual,
                                 Artifacts& artifacts, const ImageCase& item,
                                 const std::string& label, bool different_reader) {
  if (item.native_pixels.empty() || !different_reader) {
    CompareImages(expected, actual, artifacts, label); return;
  }
  SCOPED_TRACE(item.native_difference);
  artifacts.Image(label + "-original-expected", expected);
  artifacts.Image(label + "-original-actual", actual);
  // Only the two image rectangles differ. The explicit sample tables still
  // assert each reader's original output, and the independent Rust result is
  // retained. Compare all unaffected paint without synthesizing a reference.
  for (const auto& area : std::array<std::array<int, 4>, 3>{{
           {0,0,200,20}, {80,40,120,160}, {0,200,200,100}}}) {
    CompareRaster(CropRaster(expected,area[0],area[1],area[2],area[3]),
                  CropRaster(actual,area[0],area[1],area[2],area[3]),artifacts,label,1);
  }
}
void RunImages(const std::vector<ImageCase>& cases) {
  Artifacts artifacts;
  for (const auto& item : cases) {
    SCOPED_TRACE(item.label);
    const auto bytes = ImageDocument(item); artifacts.Pdf(item.label + "-input", bytes);
    std::array<Raster, 2> scenes; size_t backend = 0;
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name());
      forms::Session session(writer->api(), bytes, true, true);
      ASSERT_TRUE(session.valid()); ASSERT_TRUE(session.xfa_loaded);
      EXPECT_EQ(session.api.FPDFPage_HasFormFieldAtPoint(session.form, session.page, 10, 130), FPDF_FORMFIELD_XFA_IMAGEFIELD);
      scenes[backend] = session.Draw(200, 300, 0, FPDF_RENDER_NO_SMOOTHIMAGE);
      CheckSamples(scenes[backend], item, writer->name() == "native");
      if (item.reload) {
        const auto saved = SaveDocument(session.api, session.document, artifacts, writer->name() + "-" + item.label);
        ASSERT_FALSE(saved.empty());
        for (const auto* reader : Implementations()) {
          SCOPED_TRACE(reader->name()); forms::Session reopened(reader->api(), saved, true, true);
          ASSERT_TRUE(reopened.valid()); ASSERT_TRUE(reopened.xfa_loaded);
          const auto after = reopened.Draw(200, 300, 0, FPDF_RENDER_NO_SMOOTHIMAGE);
          CheckSamples(after, item, reader->name() == "native");
          CompareKnownImageDifference(scenes[backend], after, artifacts, item,
              writer->name() + "-" + item.label + "-to-" + reader->name(), writer->name() != reader->name());
        }
      }
      EXPECT_TRUE(session.host->callbacks_valid); EXPECT_FALSE(session.host->callback_error); ++backend;
    }
    CompareKnownImageDifference(scenes[0], scenes[1], artifacts, item, item.label + "-backends", true);
  }
}
ImageCase TiffCase(const std::string& label, Bytes data, int w, int h,
                   std::vector<Pixel> expected, bool reload = false) {
  return {label, "image/tiff", std::move(data), w, h, std::move(expected), reload};
}

TEST(CoverageCodecVariants, TiffPackedPalettesEndianBigDirectoriesAndSixteenBitSamples) {
  std::vector<ImageCase> cases;
  for (int bits : {1, 2, 4, 8}) {
    const int colors = 1 << bits;
    std::vector<uint64_t> palette;
    for (int c = 0; c < 3; ++c) for (int i = 0; i < colors; ++i) palette.push_back(i * 65535 / (colors - 1));
    for (bool little : {false, true}) for (bool big : {false, true}) {
      const Bytes samples = bits == 8 ? Bytes{0, 255} : Bytes{static_cast<uint8_t>((colors - 1) << (8 - 2 * bits))};
      const std::string name = "tiff-palette-" + std::to_string(bits) + (little ? "-le" : "-be") + (big ? "-big" : "-classic");
      cases.push_back(TiffCase(name, Tiff(2, 1, bits, 1, 3, {{320, {3, palette}}}, {samples}, little, big), 2, 1, Grays({0, 255}), bits == 4 && little && big));
    }
  }
  cases.push_back(TiffCase("tiff-old-eight-bit-colormap", Tiff(2, 1, 1, 1, 3, {{320, S({10,20,30,40,50,60})}}, {{0x40}}), 2, 1, {Rgb(10,30,50), Rgb(20,40,60)}));
  cases.push_back(TiffCase("tiff-fill-order-two", Tiff(3, 1, 1, 1, 1, {{266, S({2})}}, {{5}}), 3, 1, Grays({255,0,255})));
  cases.push_back(TiffCase("tiff-rgb-sixteen-rounding", Tiff(1, 1, 16, 3, 2, {}, {{0,1,255,0,128,128}}), 1, 1, {Rgb(1,1,128)}, true));
  cases.push_back(TiffCase("tiff-gray-sixteen-high-byte", Tiff(1, 1, 16, 1, 1, {}, {{255,0}}), 1, 1, Grays({0})));
  for (int orientation : {1, 2, 3, 4, 5, 6, 7, 8})
    cases.push_back(TiffCase("tiff-orientation-" + std::to_string(orientation), Tiff(2, 2, 8, 1, 1, {{274,S({static_cast<uint64_t>(orientation)})}}, {{10,20,30,40}}), 2, 2, Grays({10,20,30,40})));
  RunImages(cases);
}

Bytes Clumps(int width, int height, int horizontal, int vertical, const Bytes& values) {
  Bytes result;
  for (int row = 0; row < height; row += vertical) for (int column = 0; column < width; column += horizontal) {
    for (int y = 0; y < vertical; ++y) for (int x = 0; x < horizontal; ++x)
      result.push_back(row + y < height && column + x < width ? values[(row+y)*width+column+x] : 0);
    result.insert(result.end(), {128,128});
  }
  return result;
}
TEST(CoverageCodecVariants, TiffPlanarColorExtraChannelsClumpsAndTiledEdges) {
  std::vector<ImageCase> cases;
  cases.push_back(TiffCase("tiff-planar-rgb", Tiff(2, 1, 8, 3, 2, {{284,S({2})}}, {{10,40},{20,50},{30,60}}, false), 2, 1, {Rgb(10,20,30),Rgb(40,50,60)}, true));
  // Black with fractional alpha has an unambiguous white-page result for both
  // associated and unassociated storage and exercises all extra-sample paths.
  for (int alpha : {0,1,2})
    cases.push_back(TiffCase("tiff-planar-alpha-"+std::to_string(alpha), Tiff(1,1,8,4,2,{{284,S({2})},{338,S({static_cast<uint64_t>(alpha)})}},{{0},{0},{0},{128}},true,true),1,1,Grays({127})));
  cases.push_back(TiffCase("tiff-ignored-fifth-channel",Tiff(1,1,8,5,2,{{338,S({0,0})}},{{0,0,0,128,99}}),1,1,Grays({127})));
  cases.push_back(TiffCase("tiff-gray-alpha-white-is-zero",Tiff(1,1,8,2,0,{{338,S({2})}},{{255,128}}),1,1,Grays({127})));
  cases.push_back(TiffCase("tiff-explicit-rgb",Tiff(1,1,8,3,2,{},{{200,60,20}}),1,1,{Rgb(200,60,20)}));
  // Photometric is normally required. Record each reader's permissive recovery
  // separately, not as a claim that native must infer RGB from a malformed IFD.
  cases.push_back(NativeObservation(TiffCase("tiff-permissive-missing-photometric",Tiff(1,1,8,3,-1,{},{{200,60,20}}),1,1,{Rgb(200,60,20)}),
      Grays({200}), "missing Photometric recovery: native gray, Rust infers three color channels"));
  cases.push_back(TiffCase("tiff-subtractive-cmyk",Tiff(2,1,8,4,5,{},{{0,255,255,0,255,0,255,0}}),2,1,{Rgb(255,0,0),Rgb(0,255,0)}));
  const Bytes values{10,20,30,40,50,60,70,80,90,100,110,120,130,140,150};
  std::vector<Pixel> gray; for (auto value : values) gray.push_back(Gray(value));
  for (const auto& sub : std::vector<std::pair<int,int>>{{1,1},{1,2},{2,1},{2,2},{4,1},{4,2},{4,4}}) {
    auto item = TiffCase("tiff-ycbcr-"+std::to_string(sub.first)+"x"+std::to_string(sub.second),Tiff(5,3,8,3,6,{{530,S({static_cast<uint64_t>(sub.first),static_cast<uint64_t>(sub.second)})}},{Clumps(5,3,sub.first,sub.second,values)}),5,3,gray,sub.first==2&&sub.second==2);
    if (sub.second == 4) item = NativeObservation(std::move(item), std::vector<Pixel>(15,Gray(255)),
        "libtiff PickContigCase rejects image height smaller than vertical subsampling");
    cases.push_back(std::move(item));
  }
  const Bytes taller{10,20,30,40,50,60,70,80,90,100,110,120,130,140,150,160,170,180,190,200};
  std::vector<Pixel> taller_expected; for (auto value : taller) taller_expected.push_back(Gray(value));
  cases.push_back(TiffCase("tiff-ycbcr-4x4-full-height",Tiff(5,4,8,3,6,{{530,S({4,4})}},{Clumps(5,4,4,4,taller)}),5,4,taller_expected));
  cases.push_back(TiffCase("tiff-packbits-ycbcr-strips",Tiff(2,4,8,3,6,{{259,S({32773})},{278,L({2})},{530,S({2,2})}},{{5,10,20,30,40,128,128},{5,50,60,70,80,128,128}}),2,4,Grays({10,20,30,40,50,60,70,80})));
  // Native libtiff requires multiples-of-16 tile dimensions. Use valid 16x16
  // tiles with a 17x17 image so three edge tiles have padding to discard.
  std::vector<Bytes> tiles; std::vector<Pixel> expected;
  for (int tile = 0; tile < 4; ++tile) tiles.push_back(Clumps(16,16,2,2,Bytes(256, static_cast<uint8_t>(30+tile*40))));
  for (int y=0;y<17;++y)for(int x=0;x<17;++x)expected.push_back(Gray(30+(y/16*2+x/16)*40));
  // The edge-padded BigTIFF opens independently with the intended 17x17
  // samples, but the pinned native XFA RGBA path fails to decode it. Preserve
  // the correct Rust samples and keep the native failure observation explicit.
  cases.push_back(NativeObservation(TiffCase("tiff-ycbcr-tiled-padding",Tiff(17,17,8,3,6,{{322,L({16})},{323,L({16})},{530,S({2,2})}},tiles,true,true),17,17,expected),
      std::vector<Pixel>(289,Gray(255)), "pinned native XFA TIFF RGBA path rejects this valid edge-tiled BigTIFF"));
  RunImages(cases);
}

Bytes BitString(const std::string& bits) {
  Bytes result((bits.size()+7)/8,0);for(size_t i=0;i<bits.size();++i)if(bits[i]=='1')result[i/8]|=128>>(i%8);return result;
}
uint8_t Reverse(uint8_t value) { uint8_t out=0;for(int i=0;i<8;++i){out=static_cast<uint8_t>((out<<1)|(value&1));value>>=1;}return out; }
TEST(CoverageCodecVariants, TiffLegacyCompressionCodesPreserveLiteralRunsDeltasAndLogSamples) {
  std::vector<ImageCase> cases;
  cases.push_back(TiffCase("tiff-next-literal-span-runs",Tiff(8,3,2,1,1,{{259,S({32766})}},{{0,0x1b,0xe4,0x40,0,1,0,1,0,0x42,0x82,0xc4}}),8,3,Grays({0,85,170,255,255,170,85,0,255,255,255,255,0,0,0,0,85,85,170,170,255,255,255,255}),true));
  cases.push_back(TiffCase("tiff-thunder-raw-runs-two-deltas",Tiff(7,1,4,1,1,{{259,S({32809})}},{{0xc3,2,0x5b,0x96}}),7,1,Grays({51,51,51,68,51,85,51})));
  for(const auto& codec:std::vector<std::pair<int,std::string>>{{2,"10011"},{3,"00000000000110011"},{4,"1"}})for(int fill:{1,2}){
    auto bytes=BitString(codec.second);if(fill==2)for(auto& value:bytes)value=Reverse(value);
    cases.push_back(TiffCase("tiff-fax-"+std::to_string(codec.first)+"-fill"+std::to_string(fill),Tiff(8,1,1,1,0,{{259,S({static_cast<uint64_t>(codec.first)})},{266,S({static_cast<uint64_t>(fill)})}},{bytes}),8,1,std::vector<Pixel>(8,Gray(255))));
  }
  cases.push_back(TiffCase("tiff-word-aligned-fax",Tiff(8,2,1,1,0,{{259,S({32771})}},{{0x98,0,0x98,0}}),8,2,std::vector<Pixel>(16,Gray(255))));
  cases.push_back(TiffCase("tiff-reversed-packbits-payload",Tiff(2,1,8,1,1,{{259,S({32773})},{266,S({2})}},{{Reverse(1),Reverse(17),Reverse(238)}}),2,1,Grays({17,238})));
  cases.push_back(TiffCase("tiff-sgi-log-gray",Tiff(3,1,16,1,32844,{{259,S({34676})}},{{3,0,0x3f,0x40,3,0,0,0}}),3,1,Grays({0,181,255}),true));
  cases.push_back(TiffCase("tiff-sgi-log-color",Tiff(2,1,16,3,32845,{{259,S({34676})}},{{2,0,0x40,2,0,0,2,0,86,2,0,194}},false,true),2,1,Grays({0,255})));
  cases.push_back(TiffCase("tiff-sgi-log-color24",Tiff(1,1,16,3,32845,{{259,S({34677})}},{{0,0,0}}),1,1,Grays({0})));
  for(bool little:{false,true})for(int depth:{8,16}){
    Bytes words;for(uint16_t value:{uint16_t(0),uint16_t(1000),uint16_t(1250),uint16_t(1250),uint16_t(0-1000),uint16_t(0)})Put(words,value,2,little);
    cases.push_back(TiffCase("tiff-pixar-log-"+std::to_string(depth)+(little?"-le":"-be"),Tiff(2,1,depth,3,2,{{259,S({32909})}},{StoredZlib(words)},little),2,1,{Rgb(0,94,255),Rgb(255,0,255)}));
  }
  RunImages(cases);
}

Bytes Bmp(int bits,int compression,int width,int height,const std::vector<uint32_t>& palette,const Bytes& pixels) {
  Bytes result(54,0);result[0]='B';result[1]='M';At(result,14,40,4);At(result,18,width,4);At(result,22,static_cast<uint32_t>(height),4);At(result,26,1,2);At(result,28,bits,2);At(result,30,compression,4);At(result,46,palette.size(),4);
  for(auto color:palette) { Put(result,color,4); }
  At(result,10,result.size(),4);At(result,34,pixels.size(),4);
  result.insert(result.end(),pixels.begin(),pixels.end());At(result,2,result.size(),4);return result;
}
std::vector<Pixel> Indexed(const std::vector<uint32_t>& palette,std::initializer_list<int> indexes) {
  std::vector<Pixel> result;for(int index:indexes){const auto rgb=palette.at(index);result.push_back(Rgb(rgb>>16&255,rgb>>8&255,rgb&255));}return result;
}
TEST(CoverageCodecVariants, BmpRlePalettesTopDownAndExtendedBitfieldsRenderRealSamples) {
  const std::vector<uint32_t> colors{0,0xff0000,0x00ff00,0x0000ff};std::vector<ImageCase> cases;
  Bytes rle4{6,0x12,0,0,0,2,2,0,0,4,0x32,0x13,0,0,6,0x23,0,1};
  const auto expected4 = Indexed(colors,{2,3,2,3,2,3,0,0,3,2,1,3,1,2,1,2,1,2});
  // The native detector requires encoded bytes >= uncompressed pitched rows,
  // even for RLE. Its valid compact programs fail before DecodeRLE4/8 runs.
  cases.push_back(NativeObservation({"bmp-rle4-runs-absolute-delta","image/bmp",Bmp(4,2,6,3,colors,rle4),6,3,expected4,true},
      std::vector<Pixel>(18,Gray(255)), "native BMP detector compares compact RLE bytes to the uncompressed footprint"));
  // EOI terminates decoding; legal trailing file padding supplies an oracle
  // control reaching native's RLE commands without changing the pixel program.
  rle4.resize(24,0);
  cases.push_back({"bmp-rle4-padded-native-control","image/bmp",Bmp(4,2,6,3,colors,rle4),6,3,expected4});
  Bytes rle8{0,3,1,2,3,0,0,0,0,2,1,1,4,2,0,1};
  const auto expected8 = Indexed(colors,{0,2,2,2,2,0,0,0,0,0,1,2,3,0,0});
  cases.push_back(NativeObservation({"bmp-rle8-bottom-up-delta","image/bmp",Bmp(8,1,5,3,colors,rle8),5,3,expected8},
      std::vector<Pixel>(15,Gray(255)), "native BMP detector applies its uncompressed-length guard to RLE8"));
  rle8.resize(24,0);
  cases.push_back({"bmp-rle8-padded-native-control","image/bmp",Bmp(8,1,5,3,colors,rle8),5,3,expected8});
  cases.push_back({"bmp-top-down-rgb24","image/bmp",Bmp(24,0,2,-2,{}, {0,0,255,0,255,0,0,0,255,0,0,255,255,255,0,0}),2,2,{Rgb(255,0,0),Rgb(0,255,0),Rgb(0,0,255),Rgb(255,255,255)},true});
  cases.push_back({"bmp-four-bit-packed-palette","image/bmp",Bmp(4,0,4,1,colors,{0x12,0x30,0,0}),4,1,Indexed(colors,{1,2,3,0})});
  cases.push_back({"bmp-one-bit-packed-palette","image/bmp",Bmp(1,0,4,1,{0,0xffffff},{0xa0,0,0,0}),4,1,Grays({255,0,255,0})});
  for(int size:{52,56,108,124}){
    auto bytes=Bmp(32,3,1,-1,{},{});bytes.resize(14+size,0);At(bytes,14,size,4);
    At(bytes,54,0xff0000,4);At(bytes,58,0xff00,4);At(bytes,62,0xff,4);
    // An opaque explicit alpha mask exercises header variants without relying
    // on native's known XFA Bgrx shortcut for nonopaque BMP alpha.
    if(size>=56) { At(bytes,66,0xff000000,4); }
    At(bytes,10,bytes.size(),4);bytes.insert(bytes.end(),{20,40,60,255});At(bytes,2,bytes.size(),4);At(bytes,34,4,4);
    // The pinned legacy BMP reader explicitly rejects compression != BI_RGB
    // for every header larger than 40 bytes, including valid embedded masks.
    cases.push_back(NativeObservation({"bmp-bitfields-header"+std::to_string(size),"image/bmp",bytes,1,1,{Rgb(60,40,20)}},
        Grays({255}), "native ReadBmpHeaderIfh rejects extended-header BI_BITFIELDS"));
  }
  auto mask16=Bmp(16,3,1,1,{},{});Put(mask16,0xf800,4);Put(mask16,0x7e0,4);Put(mask16,0x1f,4);At(mask16,10,66,4);mask16.insert(mask16.end(),{0,0xf8,0,0});At(mask16,2,mask16.size(),4);At(mask16,34,4,4);
  cases.push_back({"bmp-sixteen-bit-565","image/bmp",mask16,1,1,{Rgb(248,0,0)}});
  // OS/2's 12-byte core header stores three-byte palette entries.
  Bytes os2{'B','M'};os2.resize(26,0);At(os2,14,12,4);At(os2,18,2,2);At(os2,20,1,2);At(os2,22,1,2);At(os2,24,1,2);os2.insert(os2.end(),{0,0,0,255,255,255});At(os2,10,32,4);os2.insert(os2.end(),{0x40,0,0,0});At(os2,2,os2.size(),4);
  cases.push_back({"bmp-os2-core-palette","image/bmp",os2,2,1,Grays({0,255})});
  RunImages(cases);
}

void PngChunk(Bytes& out,const char* kind,const Bytes& payload) {
  Put(out,payload.size(),4,false);const size_t start=out.size();for(int i=0;i<4;++i)out.push_back(kind[i]);out.insert(out.end(),payload.begin(),payload.end());uint32_t crc=0xffffffff;
  for(size_t i=start;i<out.size();++i){crc^=out[i];for(int b=0;b<8;++b)crc=crc&1?(crc>>1)^0xedb88320:crc>>1;}Put(out,~crc,4,false);
}
Bytes Png(int width,int height,int depth,int color,const Bytes& filtered,const Bytes& palette={},const Bytes& transparency={}) {
  Bytes result{137,80,78,71,13,10,26,10},header;Put(header,width,4,false);Put(header,height,4,false);header.insert(header.end(),{static_cast<uint8_t>(depth),static_cast<uint8_t>(color),0,0,0});PngChunk(result,"IHDR",header);
  if(!palette.empty()) { PngChunk(result,"PLTE",palette); }
  if(!transparency.empty()) { PngChunk(result,"tRNS",transparency); }
  PngChunk(result,"IDAT",StoredZlib(filtered));PngChunk(result,"IEND",{});return result;
}
Bytes Gif() {
  // Pinned GIF frame vector: comment, GCE transparency/delay, local palette,
  // interlace, two LZW-coded pixels. The global palette is deliberately red,
  // while the local frame's visible pixel is green.
  Bytes out{'G','I','F','8','9','a',1,0,2,0,0x80,0,0,0,0,0,255,0,0};
  const Bytes rest{0x21,0xfe,3,'a','b','c',0,0x21,0xf9,4,1,5,0,0,0,0x2c,0,0,0,0,1,0,2,0,0xc0,0,0,0,0,255,0,2,2,0x44,0x0a,0,0x3b};out.insert(out.end(),rest.begin(),rest.end());return out;
}
TEST(CoverageCodecVariants, GifLocalInterlacedTransparencyAndPngColorDepthsPaintTheirSamples) {
  std::vector<ImageCase> cases;
  // Native moves only one interlace pass after row0, then attempts row4
  // in this two-row image. Rust skips empty passes and paints the second row.
  cases.push_back(NativeObservation({"gif-local-interlace-transparent","image/gif",Gif(),1,2,{Gray(255),Rgb(0,255,0)},true},
      Grays({255,255}), "native CFX_GifContext skips no empty interlace passes for a short image"));
  auto animated=Gif();animated.pop_back();const Bytes next{0x2c,0,0,0,0,1,0,1,0,0,2,2,0x44,1,0,0x3b};animated.insert(animated.end(),next.begin(),next.end());
  cases.push_back(NativeObservation({"gif-multiple-frame-first-only","image/gif",animated,1,2,{Gray(255),Rgb(0,255,0)}},
      Grays({255,255}), "native short-image interlace pass limitation, first animation frame"));
  cases.push_back({"png-gray-eight","image/png",Png(2,1,8,0,{0,30,200}),2,1,Grays({30,200})});
  cases.push_back({"png-gray-sixteen-strip-low-byte","image/png",Png(2,1,16,0,{0,0x12,0xff,0xab,0x00}),2,1,Grays({0x12,0xab})});
  cases.push_back({"png-gray-alpha","image/png",Png(1,1,8,4,{0,0,128}),1,1,Grays({127}),true});
  cases.push_back({"png-rgba","image/png",Png(1,1,8,6,{0,0,0,0,128}),1,1,Grays({127})});
  cases.push_back({"png-rgb-sixteen","image/png",Png(1,1,16,2,{0,255,0,128,0,0,255}),1,1,{Rgb(255,128,0)}});
  cases.push_back({"png-indexed-alpha","image/png",Png(2,1,1,3,{0,0x40},{255,0,0,0,255,0},{255,0}),2,1,{Rgb(255,0,0),Gray(255)},true});
  // Mismatched/unknown content types should still use the image signature.
  cases.push_back({"png-fallback-from-jpeg-mime","image/jpeg",Png(1,1,8,2,{0,0,255,0}),1,1,{Rgb(0,255,0)}});
  cases.push_back({"png-fallback-from-unknown-mime","application/octet-stream",Png(1,1,8,2,{0,255,0,0}),1,1,{Rgb(255,0,0)}});
  RunImages(cases);
}

TEST(CoverageCodecVariants, BoundedMalformedImagePacketsLeaveTheDocumentAndInputUsable) {
  const auto valid_tiff=Tiff(1,1,8,1,1,{},{{20}});
  auto unknown_codec=Tiff(1,1,8,1,1,{{259,S({65000})}},{{0}});
  auto bad_palette=Tiff(1,1,1,1,3,{{320,S({1,2})}},{{0}});
  auto bad_bmp=Bmp(8,1,5,2,{0,0xff0000},{0,3,1,1});
  auto bad_masks=Bmp(32,3,1,1,{},{});Put(bad_masks,0xff,4);Put(bad_masks,0xff,4);Put(bad_masks,0xff0000,4);bad_masks.insert(bad_masks.end(),{1,2,3,4});At(bad_masks,10,66,4);At(bad_masks,2,bad_masks.size(),4);
  const std::vector<std::pair<std::string,Bytes>> bad={
      {"image/tiff",Bytes(valid_tiff.begin(),valid_tiff.begin()+7)},
      {"image/tiff",unknown_codec},{"image/tiff",bad_palette},
      {"image/tiff",Tiff(8,1,2,1,1,{{259,S({32766})}},{{0,0x1b}})},
      {"image/tiff",Tiff(2,1,4,1,1,{{259,S({32809})}},{{0xc3,3}})},
      {"image/bmp",Bytes{'B','M',0,0,0}},{"image/bmp",bad_bmp},{"image/bmp",bad_masks},
      {"image/gif",Bytes{'G','I','F','8','9','a',1,0}},
      {"image/png",Bytes{137,80,78,71,13,10,26,10}},
      {"image/jpeg",Bytes{0xff,0xd8,0xff,0xe0,0,20,1,2,3}}};
  Artifacts artifacts;size_t index=0;
  for(const auto& item:bad){
    SCOPED_TRACE(index);std::array<Raster,2> scenes;size_t backend=0;
    const ImageCase image{"bad-image-"+std::to_string(index),item.first,item.second,1,1,{Gray(255)}};
    for(const auto* library:Implementations()){
      SCOPED_TRACE(library->name());forms::Session session(library->api(),ImageDocument(image),true,true);ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);
      scenes[backend]=session.Draw(200,300);CheckSamples(scenes[backend],image);
      ASSERT_TRUE(session.Focus(5,25));ASSERT_TRUE(session.api.FORM_SelectAllText(session.form,session.page));session.Replace(u"Recovered");EXPECT_EQ(session.Text(),u"Recovered");session.api.FORM_ForceToKillFocus(session.form);
      EXPECT_EQ(AtPixel(session.Draw(200,300),8,205),Rgb(0,255,0));EXPECT_TRUE(session.host->callbacks_valid);++backend;
    }
    CompareImages(scenes[0],scenes[1],artifacts,"bad-image-"+std::to_string(index++));
  }
}

}  // namespace
}  // namespace pdfoxium::integration
