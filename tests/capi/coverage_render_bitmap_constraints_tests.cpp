// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "runtime_fixture.h"

#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using RGB = std::array<int, 3>;
constexpr int kSide = 96;
constexpr int kFlags = FPDF_RENDER_NO_SMOOTHIMAGE | FPDF_RENDER_NO_SMOOTHPATH;
Bytes ImageDocument(const std::string& dictionary, const std::string& samples) {
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Resources << /XObject << /I 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", "q 54 0 0 48 16 16 cm /I Do Q 0 1 0 rg 76 8 8 8 re f"),
      PdfStream("/Type /XObject /Subtype /Image " + dictionary, samples)});
}
RGB At(const Raster& raster, int x, int y) {
  const auto index = (size_t(kSide - y - 1) * raster.width + x) * 4;
  return {raster.pixels.at(index + 2), raster.pixels.at(index + 1), raster.pixels.at(index)};
}
void Pixel(const Raster& raster, int x, int y, RGB expected, int tolerance = 0) {
  ASSERT_EQ(raster.width, kSide); ASSERT_EQ(raster.height, kSide);
  const auto actual = At(raster, x, y);
  for (int channel = 0; channel < 3; ++channel) {
    EXPECT_NEAR(actual[channel], expected[channel], tolerance) << x << ',' << y;
  }
}
RGB OnSurface(RGB color, int format) {
  if (format == FPDFBitmap_Gray) {
    const int gray = (30 * color[0] + 59 * color[1] + 11 * color[2]) / 100;
    return {gray, gray, gray};
  }
  return color;
}
Raster RenderBorrowed(const Api& api, FPDF_PAGE page, int format) {
  const int step = format == FPDFBitmap_Gray ? 1 : format == FPDFBitmap_BGR ? 3 : 4;
  // Native 32bpp FillRect uses aligned uint32_t scanlines. Keep that layout
  // aligned; byte-based Gray/BGR formats also exercise an unaligned base and
  // odd strides. Every format retains padding and outer sentinels.
  const int prefix = step == 4 ? 4 : 1;
  const int stride = kSide * step + (step == 4 ? 8 : 7);
  std::vector<uint8_t> storage(size_t(stride) * kSide + prefix + 1, 0xa7);
  Handle bitmap(api.FPDFBitmap_CreateEx(kSide, kSide, format, storage.data() + prefix, stride), api.FPDFBitmap_Destroy);
  if (!bitmap.get()) { ADD_FAILURE() << "Borrowed bitmap allocation failed"; return {}; }
  EXPECT_EQ(api.FPDFBitmap_GetBuffer(bitmap.get()), storage.data() + prefix);
  EXPECT_EQ(api.FPDFBitmap_GetStride(bitmap.get()), stride);
  EXPECT_EQ(api.FPDFBitmap_GetFormat(bitmap.get()), format);
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, kSide, kSide, 0xffffffff));
  api.FPDF_RenderPageBitmap(bitmap.get(), page, 0, 0, kSide, kSide, 0, kFlags);
  EXPECT_EQ(storage.front(), 0xa7); EXPECT_EQ(storage.back(), 0xa7);
  Raster out{kSide, kSide, {}};
  for (int row = 0; row < kSide; ++row) {
    const auto* source = storage.data() + prefix + row * stride;
    for (int column = kSide * step; column < stride; ++column) {
      EXPECT_EQ(source[column], 0xa7) << "row padding " << row << ':' << column;
    }
    for (int x = 0; x < kSide; ++x) {
      const auto* pixel = source + x * step;
      if (step == 1) out.pixels.insert(out.pixels.end(), {pixel[0], pixel[0], pixel[0], 255});
      else out.pixels.insert(out.pixels.end(), {pixel[0], pixel[1], pixel[2], 255});
    }
  }
  return out;
}
using Check = std::function<void(const Raster&, int)>;
using Inspect = std::function<void(const Api&, FPDF_PAGE)>;
void ExerciseBitmapConstraints(const Bytes& input, const std::string& label,
                               const Check& check, const Inspect& inspect = {}) {
  Artifacts artifacts;
  for (const int format : {FPDFBitmap_BGRA, FPDFBitmap_Gray}) {
    const auto scenario = label + "-format-" + std::to_string(format);
    std::array<Raster, 2> original; size_t index = 0;
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(scenario + "-" + writer->name()); const auto& api = writer->api();
      Handle document(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
      if (inspect) { ASSERT_NO_FATAL_FAILURE(inspect(api, page.get())); }
      original[index] = RenderBorrowed(api, page.get(), format);
      ASSERT_NO_FATAL_FAILURE(check(original[index], format));
      const auto saved = SaveDocument(api, document.get(), artifacts, scenario + "-" + writer->name());
      ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        SCOPED_TRACE(reader->name()); const auto& read = reader->api();
        Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
        ASSERT_NE(reopened.get(), nullptr);
        Handle loaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage); ASSERT_NE(loaded.get(), nullptr);
        if (inspect) { ASSERT_NO_FATAL_FAILURE(inspect(read, loaded.get())); }
        const auto after = RenderBorrowed(read, loaded.get(), format);
        ASSERT_NO_FATAL_FAILURE(check(after, format));
        CompareRaster(original[index], after, artifacts, scenario + "-" + writer->name() + "-" + reader->name(), 1);
      }
      ++index;
    }
    CompareRaster(original[0], original[1], artifacts, scenario + "-engines", 1);
  }
}
void Guards(const Raster& raster, int format) {
  Pixel(raster, 3, 3, {255, 255, 255});
  Pixel(raster, 90, 90, {255, 255, 255});
  // A later independent object must still render even if image decoding fails.
  Pixel(raster, 80, 12, OnSurface({0, 255, 0}, format));
}
enum class Tail { Missing, Partial, ReversedDecode };
struct SamplesCase { int bits; bool gray; Tail tail; };
std::string CaseName(const SamplesCase& c) {
  return std::string(c.gray ? "Gray" : "Rgb") + std::to_string(c.bits) +
      (c.tail == Tail::Missing ? "MissingRow" : c.tail == Tail::Partial ? "PartialRow" : "ReverseDecode");
}
std::string Samples(const SamplesCase& c) {
  const unsigned maximum = (1u << c.bits) - 1;
  const int components = c.gray ? 1 : 3;
  const int pitch = (9 * components * c.bits + 7) / 8;
  std::string out(size_t(pitch) * 3, '\0');
  for (int row = 0; row < 3; ++row) {
    size_t bit = size_t(row) * pitch * 8;
    for (int x = 0; x < 9; ++x) {
      for (int component = 0; component < components; ++component) {
        const unsigned value = c.gray ? (row == 0 ? 0 : row == 1 ? maximum / 2 : maximum)
                                      : (component == row ? maximum : 0);
        for (int shift = c.bits - 1; shift >= 0; --shift, ++bit) {
          if ((value >> shift) & 1) out[bit / 8] = char(uint8_t(out[bit / 8]) | (0x80 >> (bit % 8)));
        }
      }
    }
  }
  if (c.tail == Tail::Missing) out.resize(size_t(pitch) * 2);
  if (c.tail == Tail::Partial) out.resize(size_t(pitch) * 2 + (4 * components * c.bits + 7) / 8);
  return out;
}
class CoverageRenderBitmapSamples : public testing::TestWithParam<SamplesCase> {};
TEST_P(CoverageRenderBitmapSamples, ShortRowsAndDecodeRangesRemainBoundedAndSurviveBothSavers) {
  const auto c = GetParam(); const auto name = CaseName(c);
  const std::string decode = c.tail != Tail::ReversedDecode ? "" :
      c.gray ? " /Decode [1 0]" : " /Decode [1 0 1 0 1 0]";
  const auto input = ImageDocument("/Width 9 /Height 3 /ColorSpace " +
      std::string(c.gray ? "/DeviceGray" : "/DeviceRGB") + " /BitsPerComponent " + std::to_string(c.bits) + decode, Samples(c));
  const auto inspect = [](const Api& api, FPDF_PAGE page) {
    const auto image = api.FPDFPage_GetObject(page, 0); ASSERT_NE(image, nullptr);
    EXPECT_EQ(api.FPDFPageObj_GetType(image), FPDF_PAGEOBJ_IMAGE);
    Handle decoded(api.FPDFImageObj_GetBitmap(image), api.FPDFBitmap_Destroy);
    ASSERT_NE(decoded.get(), nullptr);
    EXPECT_EQ(api.FPDFBitmap_GetWidth(decoded.get()), 9); EXPECT_EQ(api.FPDFBitmap_GetHeight(decoded.get()), 3);
    EXPECT_GT(api.FPDFBitmap_GetStride(decoded.get()), 0);
    EXPECT_NE(api.FPDFBitmap_GetBuffer(decoded.get()), nullptr);
  };
  ExerciseBitmapConstraints(input, name, [c](const Raster& raster, int format) {
    Guards(raster, format);
    const unsigned maximum = (1u << c.bits) - 1;
    for (int row = 0; row < 3; ++row) {
      RGB expected = c.gray ? RGB{0, 0, 0} : RGB{row == 0 ? 255 : 0, row == 1 ? 255 : 0, row == 2 ? 255 : 0};
      if (c.gray) expected.fill(row == 0 ? 0 : row == 1 ? int((maximum / 2) * 255 / maximum) : 255);
      if (c.tail == Tail::ReversedDecode) for (auto& channel : expected) channel = 255 - channel;
      if (row == 2 && c.tail == Tail::Missing) expected = {0, 0, 0};
      Pixel(raster, 19, 56 - row * 16, OnSurface(expected, format), 1);
      const RGB right = row == 2 && c.tail == Tail::Partial ? RGB{0, 0, 0} : expected;
      Pixel(raster, 67, 56 - row * 16, OnSurface(right, format), 1);
    }
  }, inspect);
}
std::vector<SamplesCase> SampleCases() {
  std::vector<SamplesCase> cases;
  for (const int bits : {1, 2, 4, 8, 16}) for (const bool gray : {false, true})
    for (const auto tail : {Tail::Missing, Tail::Partial, Tail::ReversedDecode}) cases.push_back({bits, gray, tail});
  return cases;
}
INSTANTIATE_TEST_SUITE_P(PackedImageRows, CoverageRenderBitmapSamples, testing::ValuesIn(SampleCases()),
                        [](const testing::TestParamInfo<SamplesCase>& info) { return CaseName(info.param); });

struct RejectedImage { const char* name; int width, height, bits; bool empty = false; };
class CoverageRenderBitmapRejectedImages : public testing::TestWithParam<RejectedImage> {};
TEST_P(CoverageRenderBitmapRejectedImages, FailedImageLeavesIndependentPageContentRenderableAfterSave) {
  const auto c = GetParam();
  // An EOD-only RunLength program advertises zero decoded samples and fails
  // the decoder's required-size check before and after save. A zero-byte raw
  // stream instead becomes a nonempty Flate program in both savers, changing
  // its missing-row fallback from an absent image to black decoded samples.
  const auto input = ImageDocument("/Width " + std::to_string(c.width) + " /Height " + std::to_string(c.height) +
      " /BitsPerComponent " + std::to_string(c.bits) + " /ColorSpace /DeviceRGB" +
      (c.empty ? " /Filter /RunLengthDecode" : ""), c.empty ? std::string(1, char(128)) : std::string(64, '\xff'));
  ExerciseBitmapConstraints(input, c.name, [](const Raster& raster, int format) {
    Guards(raster, format); Pixel(raster, 30, 30, {255, 255, 255}); Pixel(raster, 60, 55, {255, 255, 255});
  });
}
INSTANTIATE_TEST_SUITE_P(ImageHeaderBounds, CoverageRenderBitmapRejectedImages, testing::Values(
    RejectedImage{"ZeroWidth", 0, 3, 8}, RejectedImage{"NegativeWidth", -1, 3, 8},
    RejectedImage{"WidthBeyondLimit", 131072, 3, 8}, RejectedImage{"ZeroHeight", 9, 0, 8},
    RejectedImage{"NegativeHeight", 9, -1, 8}, RejectedImage{"HeightBeyondLimit", 9, 131072, 8},
    RejectedImage{"ZeroBits", 9, 3, 0}, RejectedImage{"UnsupportedThreeBits", 9, 3, 3},
    RejectedImage{"BitsBeyondLimit", 9, 3, 17}, RejectedImage{"EmptyRunLengthSampleProgram", 9, 3, 8, true}),
    [](const testing::TestParamInfo<RejectedImage>& info) { return info.param.name; });

TEST(CoverageRenderBitmapConstraints, CroppedLargeRunLengthImageKeepsTheRetainedDecoderPathBounded) {
  // The public page cache retains lazy CPDF_DIB scanlines at 60,000,000 bytes.
  // This 61,440,000-byte image is only 960,001 encoded bytes; its decoder owns
  // one 512-byte row. Translation clips away the first 119,920 rows before the
  // stretch engine asks the retained source to SkipToScanline().
  std::string samples;
  samples.reserve(960001);
  for (int run = 0; run < 480000; ++run) { samples.push_back(char(129)); samples.push_back(0); }
  samples.push_back(char(128));
  const auto input = PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Resources << /XObject << /I 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", "q 54 0 0 120000 16 16 cm /I Do Q 0 1 0 rg 76 8 8 8 re f"),
      PdfStream("/Type /XObject /Subtype /Image /Width 512 /Height 120000 /BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /RunLengthDecode", samples)});
  ExerciseBitmapConstraints(input, "cropped-large-runlength", [](const Raster& raster, int format) {
    Guards(raster, format);
    Pixel(raster, 20, 20, {0, 0, 0}); Pixel(raster, 60, 90, {0, 0, 0});
    Pixel(raster, 10, 50, {255, 255, 255}); Pixel(raster, 30, 10, {255, 255, 255});
  });
}

class CoverageRenderBitmapBorrowedSource : public testing::TestWithParam<int> {};
TEST_P(CoverageRenderBitmapBorrowedSource, ImageOwnsSamplesAfterPaddedSourceStorageAndHandleAreDestroyed) {
  const int format = GetParam(); Artifacts artifacts;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument); ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDFPage_New(document.get(), 0, kSide, kSide), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
    const auto image = api.FPDFPageObj_NewImageObj(document.get()); ASSERT_NE(image, nullptr);
    const int step = format == FPDFBitmap_Gray ? 1 : format == FPDFBitmap_BGR ? 3 : 4;
    const int prefix = step == 4 ? 4 : 1;
    const int stride = 9 * step + (step == 4 ? 8 : 5);
    {
      std::vector<uint8_t> storage(size_t(stride) * 3 + prefix + 1, 0x95);
      Handle source(api.FPDFBitmap_CreateEx(9, 3, format, storage.data() + prefix, stride), api.FPDFBitmap_Destroy);
      ASSERT_NE(source.get(), nullptr);
      ASSERT_TRUE(api.FPDFBitmap_FillRect(source.get(), -3, -1, 15, 5, 0xffff0000));
      EXPECT_EQ(storage.front(), 0x95); EXPECT_EQ(storage.back(), 0x95);
      for (int row = 0; row < 3; ++row) for (int x = 9 * step; x < stride; ++x) {
        EXPECT_EQ(storage[prefix + row * stride + x], 0x95);
      }
      ASSERT_TRUE(api.FPDFImageObj_SetBitmap(nullptr, 0, image, source.get()));
    }
    ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image, 54, 0, 0, 48, 16, 16));
    api.FPDFPage_InsertObject(page.get(), image); ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    const auto saved = SaveDocument(api, document.get(), artifacts, "borrowed-" + std::to_string(format) + "-" + writer->name());
    ASSERT_FALSE(saved.empty());
    const RGB color = format == FPDFBitmap_Gray ? RGB{76, 76, 76} : RGB{255, 0, 0};
    for (const int destination : {FPDFBitmap_BGRA, FPDFBitmap_Gray}) {
      const auto before = RenderBorrowed(api, page.get(), destination);
      Pixel(before, 30, 30, OnSurface(color, destination)); Pixel(before, 3, 3, {255, 255, 255});
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api();
        Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
        ASSERT_NE(reopened.get(), nullptr);
        Handle loaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage); ASSERT_NE(loaded.get(), nullptr);
        const auto after = RenderBorrowed(read, loaded.get(), destination);
        Pixel(after, 30, 30, OnSurface(color, destination)); Pixel(after, 3, 3, {255, 255, 255});
        CompareRaster(before, after, artifacts, "borrowed-" + std::to_string(format) + "-" + std::to_string(destination) + "-" + writer->name() + "-" + reader->name());
      }
    }
  }
}
INSTANTIATE_TEST_SUITE_P(SourceFormats, CoverageRenderBitmapBorrowedSource,
                        testing::Values(FPDFBitmap_Gray, FPDFBitmap_BGR, FPDFBitmap_BGRx, FPDFBitmap_BGRA),
                        [](const testing::TestParamInfo<int>& info) { return "Format" + std::to_string(info.param); });
}  // namespace
}  // namespace pdfoxium::integration
