#include "runtime_fixture.h"

#include <algorithm>
#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Color = std::array<int, 3>;
constexpr int kSize = 128;
constexpr int kFlags = FPDF_RENDER_NO_SMOOTHIMAGE | FPDF_RENDER_NO_SMOOTHPATH;
constexpr std::array<int, 4> kFormats{FPDFBitmap_Gray, FPDFBitmap_BGR, FPDFBitmap_BGRx, FPDFBitmap_BGRA};

// Each packed row is padded independently, including the odd-width 1/2/4-bit
// cases. This literal encoder deliberately does not use an engine's image API.
std::string Samples(const std::vector<unsigned>& row, int bits, int height = 3) {
  std::string packed((row.size() * bits + 7) / 8, '\0');
  size_t bit = 0;
  for (unsigned value : row) for (int shift = bits - 1; shift >= 0; --shift, ++bit)
    packed[bit / 8] = static_cast<char>(static_cast<unsigned char>(packed[bit / 8]) |
                                      (((value >> shift) & 1) << (7 - bit % 8)));
  std::string result;
  for (int y = 0; y < height; ++y) result += packed;
  return result;
}
std::vector<uint8_t> ImagePdf(int bits, const std::string& space,
                            const std::string& samples, const std::string& extra = "",
                            const std::vector<std::string>& trailing = {},
                            const std::string& matrix = "100 0 0 48 14 32",
                            const std::string& prefix = "") {
  // Images and masks have no public dictionary constructor. All other scene
  // geometry is a crisp rectangle with known sample centers and white guards.
  std::vector<std::string> objects{
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << /XObject << /Im 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", prefix + "q " + matrix + " cm /Im Do Q\n"),
      PdfStream("/Type /XObject /Subtype /Image /Width 5 /Height 3 /BitsPerComponent " +
                    std::to_string(bits) + " /ColorSpace " + space + " " + extra, samples)};
  objects.insert(objects.end(), trailing.begin(), trailing.end());
  return PdfFixture(objects);
}
Raster SurfaceRender(const Api& api, FPDF_PAGE page, int format, int offset = 0) {
  // External backing has guard bytes after every odd-width row as well as
  // before/after the bitmap. PDFium accepts custom positive row strides.
  const int step = format == FPDFBitmap_Gray ? 1 : format == FPDFBitmap_BGR ? 3 : 4;
  const int stride = ((kSize * step + 3) & ~3) + 12;
  std::vector<uint8_t> storage(stride * kSize + 32, 0xa7);
  Handle bitmap(api.FPDFBitmap_CreateEx(kSize, kSize, format, storage.data() + 16, stride), api.FPDFBitmap_Destroy);
  if (!bitmap.get()) { ADD_FAILURE(); return {}; }
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, kSize, kSize, 0xffffffff));
  api.FPDF_RenderPageBitmap(bitmap.get(), page, offset, 0, kSize, kSize, 0, kFlags | FPDF_NO_NATIVETEXT);
  Raster result{kSize, kSize, {}};
  for (int y = 0; y < kSize; ++y) {
    const auto* row = storage.data() + 16 + y * stride;
    for (int x = 0; x < kSize; ++x) {
      const auto* p = row + x * step;
      if (step == 1) result.pixels.insert(result.pixels.end(), {p[0], p[0], p[0], 255});
      else result.pixels.insert(result.pixels.end(), {p[0], p[1], p[2], static_cast<uint8_t>(format == FPDFBitmap_BGRA ? p[3] : 255)});
    }
    for (int x = kSize * step; x < stride; ++x) EXPECT_EQ(row[x], 0xa7) << "padding row=" << y;
  }
  for (int i = 0; i < 16; ++i) {
    EXPECT_EQ(storage[i], 0xa7);
    EXPECT_EQ(storage[16 + stride * kSize + i], 0xa7);
  }
  return result;
}
void Sample(const Raster& raster, int x, int y, Color rgb, int format, int tolerance = 1) {
  if (format == FPDFBitmap_Gray) {
    const int gray = (rgb[0] * 30 + rgb[1] * 59 + rgb[2] * 11) / 100;
    rgb = {gray, gray, gray};
  }
  const size_t at = ((kSize - 1 - y) * kSize + x) * 4;
  for (int c = 0; c < 3; ++c) EXPECT_NEAR(raster.pixels[at + 2 - c], rgb[c], tolerance) << x << ',' << y << " channel=" << c;
  EXPECT_EQ(raster.pixels[at + 3], 255);
}
void Row(const Raster& raster, const std::vector<Color>& colors, int format, int offset = 0) {
  ASSERT_EQ(raster.pixels.size(), kSize * kSize * 4u);
  for (size_t x = 0; x < colors.size(); ++x) Sample(raster, 24 + 20 * x + offset, 56, colors[x], format);
  Sample(raster, 2, 110, {255, 255, 255}, format, 0);
  Sample(raster, 124, 12, {255, 255, 255}, format, 0);
}
void Workflow(const std::vector<uint8_t>& fixture, const std::vector<Color>& expected,
              const std::string& name, int format = FPDFBitmap_BGRA, int offset = 0) {
  Artifacts artifacts;
  std::array<Raster, 2> originals;
  size_t index = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(name + " writer=" + writer->name() + " format=" + std::to_string(format));
    const auto& api = writer->api();
    Handle document(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    originals[index] = SurfaceRender(api, page.get(), format, offset);
    ASSERT_NO_FATAL_FAILURE(Row(originals[index], expected, format, offset));
    // Reusing a live page exercises the decoded-image cache, not merely a
    // second parser. Pixel identity also detects stale scanline state.
    CompareRaster(originals[index], SurfaceRender(api, page.get(), format, offset), artifacts, name + "-cached");
    const auto saved = SaveDocument(api, document.get(), artifacts, name + "-" + writer->name());
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE("reader=" + reader->name());
      const auto& read = reader->api();
      Handle reload(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reload.get(), nullptr);
      Handle reload_page(read.FPDF_LoadPage(reload.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(reload_page.get(), nullptr);
      const auto raster = SurfaceRender(read, reload_page.get(), format, offset);
      ASSERT_NO_FATAL_FAILURE(Row(raster, expected, format, offset));
      CompareRaster(originals[index], raster, artifacts, name + "-reload-" + reader->name(), 1);
    }
    ++index;
  }
  CompareRaster(originals[0], originals[1], artifacts, name + "-native-rust", 1);
}

TEST(CoverageImagePipeline, OddWidthPackedRgbExpandsEveryComponentAcrossSurfaceFormats) {
  for (int bits : {1, 2, 4}) {
    const unsigned maximum = (1u << bits) - 1;
    const std::vector<unsigned> samples{maximum, 0, 0, 0, maximum, 0, 0, 0, maximum,
                                        maximum, maximum, maximum, 0, 0, 0};
    for (int format : kFormats)
      Workflow(ImagePdf(bits, "/DeviceRGB", Samples(samples, bits)),
               {{255,0,0},{0,255,0},{0,0,255},{255,255,255},{0,0,0}}, "packed-rgb-" + std::to_string(bits), format);
  }
}
TEST(CoverageImagePipeline, GrayColorKeyUsesRawSamplesForSetAndResetBitsAndByteRanges) {
  for (int bits : {1, 2, 4, 8}) {
    const unsigned maximum = (1u << bits) - 1;
    const std::vector<unsigned> samples{0, maximum, maximum / 2, maximum, 0};
    for (unsigned key : {0u, maximum}) {
      std::vector<Color> expected;
      for (unsigned value : samples) {
        const int gray = value == key ? 255 : value * 255 / maximum;
        expected.push_back({gray, gray, gray});
      }
      for (int format : kFormats)
        Workflow(ImagePdf(bits, "/DeviceGray", Samples(samples, bits), "/Mask [" + std::to_string(key) + " " + std::to_string(key) + "]"),
                 expected, "gray-key-" + std::to_string(bits) + "-" + std::to_string(key), format);
    }
  }
}
TEST(CoverageImagePipeline, IndexedColorKeysKeepPaletteColorsAndOnlyHideMatchingIndices) {
  for (int bits : {1, 2, 4, 8}) {
    const std::vector<unsigned> samples{0,1,0,1,0};
    for (int format : kFormats)
      Workflow(ImagePdf(bits, "[/Indexed /DeviceRGB 1 <E0204000C080>]", Samples(samples, bits), "/Mask [1 1]"),
               {{224,32,64},{255,255,255},{224,32,64},{255,255,255},{224,32,64}}, "indexed-key-" + std::to_string(bits), format);
  }
}
TEST(CoverageImagePipeline, RgbColorKeyRequiresEveryComponentInsideItsIndependentRange) {
  for (int bits : {1, 2, 4, 8}) {
    const unsigned maximum = (1u << bits) - 1;
    const std::vector<unsigned> samples{0,0,0, maximum,0,0, 0,maximum,0, 0,0,maximum, maximum,maximum,maximum};
    const std::string mask = "/Mask [0 0 0 0 0 0]";
    for (int format : kFormats)
      Workflow(ImagePdf(bits, "/DeviceRGB", Samples(samples, bits), mask),
               {{255,255,255},{255,0,0},{0,255,0},{0,0,255},{255,255,255}}, "rgb-key-" + std::to_string(bits), format);
  }
}
TEST(CoverageImagePipeline, SixteenBitGrayDecodeAndRgbHighBytesReadOddRowsWithoutBleeding) {
  for (bool invert : {false, true}) {
    const std::vector<unsigned> gray{0, 0x4040, 0x8080, 0xc0c0, 0xffff};
    std::vector<Color> expected;
    for (unsigned value : gray) {
      const int g = invert ? 255 - value / 257 : value / 257;
      expected.push_back({g,g,g});
    }
    for (int format : kFormats)
      Workflow(ImagePdf(16, "/DeviceGray", Samples(gray, 16), invert ? "/Decode [1 0]" : "/Decode [0 1]"), expected,
               "gray16-" + std::to_string(invert), format);
  }
  const std::vector<unsigned> rgb{0xffff,0,0, 0,0xffff,0, 0,0,0xffff, 0x4040,0x8080,0xc0c0, 0xffff,0xffff,0xffff};
  for (int format : kFormats)
    Workflow(ImagePdf(16, "/DeviceRGB", Samples(rgb,16)), {{255,0,0},{0,255,0},{0,0,255},{64,128,192},{255,255,255}}, "rgb16", format);
}
TEST(CoverageImagePipeline, DecodeIsAppliedBeforeRgbCompositingAndColorKeysRemainInSampleSpace) {
  const std::vector<unsigned> samples{0,0,0, 255,0,0, 0,255,0, 0,0,255, 255,255,255};
  for (bool key : {false,true}) for (int format : kFormats)
    Workflow(ImagePdf(8, "/DeviceRGB", Samples(samples,8), "/Decode [1 0 0 1 1 0] " + std::string(key ? "/Mask [0 0 0 0 0 0]" : "")),
             {key ? Color{255,255,255} : Color{255,0,255}, {0,0,255},{255,255,255},{255,0,0},{0,255,0}}, "decode-rgb-" + std::to_string(key), format);
}
TEST(CoverageImagePipeline, SoftMaskMatteUnpremultipliesColorBeforeCompositingAndSurvivesReload) {
  // Red premultiplied against a white Matte: (1,1-a,1-a), with a=128/255.
  // Removing that Matte and painting onto white must reproduce the samples.
  const std::vector<unsigned> rgb{255,127,127, 255,127,127, 255,127,127, 255,127,127, 255,127,127};
  const auto mask = PdfStream("/Type /XObject /Subtype /Image /Width 5 /Height 3 /BitsPerComponent 8 /ColorSpace /DeviceGray /Matte [1 1 1]", Samples({128,128,128,128,128},8));
  for (int format : kFormats)
    Workflow(ImagePdf(8,"/DeviceRGB",Samples(rgb,8),"/SMask 6 0 R",{mask}),
             std::vector<Color>(5,Color{255,127,127}), "matte-white",format);
}
TEST(CoverageImagePipeline, ReflectedImagesAndNegativeDeviceOriginKeepPackedRowsAndGuardsIntact) {
  for (int bits : {1,4,8}) {
    const unsigned maximum=(1u<<bits)-1;
    const std::vector<unsigned> rgb{maximum,0,0, 0,maximum,0, 0,0,maximum, maximum,maximum,maximum, 0,0,0};
    for (int format : {FPDFBitmap_BGR,FPDFBitmap_BGRA})
      Workflow(ImagePdf(bits,"/DeviceRGB",Samples(rgb,bits),"",{},"-100 0 0 -48 114 80"),
               {{0,0,0},{255,255,255},{0,0,255},{0,255,0},{255,0,0}},"reflected-clipped-"+std::to_string(bits),format,-7);
  }
}
}  // namespace
}  // namespace pdfoxium::integration
