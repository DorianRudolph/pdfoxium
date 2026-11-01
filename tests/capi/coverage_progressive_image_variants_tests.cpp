// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "runtime_fixture.h"

#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using RGB = std::array<int, 3>;
constexpr int kSide = 96;
constexpr int kCrisp = FPDF_RENDER_NO_SMOOTHIMAGE | FPDF_RENDER_NO_SMOOTHPATH;
const std::string kIdentity = "<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >>";
const std::string kInvert = "<< /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0] /N 1 >>";
struct Spec {
  std::string name, image = "/Width 4 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceRGB";
  std::string samples = std::string("\xff\0\0\0\xff\0\0\0\xff\x80\x80\x80", 12);
  std::string state, extra7 = "null", extra8 = "null", prefix, paint = "/G gs ";
  std::string matrix = "64 0 0 48 16 16";
  std::array<RGB, 4> colors{{{255, 0, 0}, {0, 255, 0}, {0, 0, 255}, {128, 128, 128}}};
  int flags = kCrisp;
  bool pattern = false, uncolored = false, reflected = false, clipped = false, require_pause = false;
};
Bytes Fixture(const Spec& s) {
  const std::string pattern_resources = s.pattern ? "/Pattern << /P 7 0 R >> /ColorSpace << /PCS [/Pattern /DeviceRGB] >> " : "";
  const std::string clip = s.clipped ? "16 16 64 24 re W n " : "";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Resources << /XObject << /I 5 0 R >> /ExtGState << /G 6 0 R >> " + pattern_resources + " >> /Contents 4 0 R >>",
      PdfStream("", s.prefix + "q " + clip + s.paint + s.matrix + " cm /I Do Q 0 1 0 rg 86 86 4 4 re f"),
      PdfStream("/Type /XObject /Subtype /Image " + s.image, s.samples),
      "<< /Type /ExtGState " + s.state + " >>", s.extra7, s.extra8});
}
RGB Gray(RGB color, int flags) {
  if (flags & FPDF_GRAYSCALE) {
    const int gray = (30 * color[0] + 59 * color[1] + 11 * color[2]) / 100;
    return {gray, gray, gray};
  }
  return color;
}
void Pixel(const Raster& r, int x, int y, RGB expected, int delta = 0) {
  ASSERT_EQ(r.width, kSide); ASSERT_EQ(r.height, kSide);
  const auto at = (size_t(kSide - y - 1) * kSide + x) * 4;
  EXPECT_NEAR(r.pixels.at(at), expected[2], delta) << x << ',' << y;
  EXPECT_NEAR(r.pixels.at(at + 1), expected[1], delta) << x << ',' << y;
  EXPECT_NEAR(r.pixels.at(at + 2), expected[0], delta) << x << ',' << y;
  EXPECT_EQ(r.pixels.at(at + 3), 255);
}
void Inspect(const Raster& r, const Spec& s) {
  Pixel(r, 3, 3, {255, 255, 255}); Pixel(r, 92, 92, {255, 255, 255});
  Pixel(r, 88, 88, Gray({0, 255, 0}, s.flags));
  if (!s.pattern) {
    for (int column = 0; column < 4; ++column) {
      const auto expected = s.colors[s.reflected ? 3 - column : column];
      Pixel(r, 24 + column * 16, 28, Gray(expected, s.flags), 1);
      if (!s.clipped) Pixel(r, 24 + column * 16, 52, Gray(expected, s.flags), 1);
    }
  } else {
    for (int column = 0; column < 4; ++column) {
      const bool visible = ((s.reflected ? 3 - column : column) % 2) == 0;
      for (int half = 0; half < 2; ++half) {
        const RGB paint = s.uncolored ? RGB{51, 102, 153} : half == 0 ? RGB{255, 0, 0} : RGB{0, 0, 255};
        Pixel(r, 20 + column * 16 + half * 8, 28, visible ? Gray(paint, s.flags) : RGB{255, 255, 255}, 1);
      }
    }
  }
  if (s.clipped) Pixel(r, 24, 52, {255, 255, 255});
}
struct Pause : IFSDK_PAUSE {
  Pause() { version = 1; user = this; NeedToPauseNow = Need; }
  static FPDF_BOOL Need(IFSDK_PAUSE* p) { return ++static_cast<Pause*>(p)->calls <= 4; }
  int calls = 0;
};
struct Job {
  Job(const Api& api, FPDF_PAGE page) : api(api), page(page) {}
  ~Job() { api.FPDF_RenderPage_Close(page); }
  const Api& api; FPDF_PAGE page;
};
void PaintProgressive(const Api& api, FPDF_PAGE page, const Spec& s, Raster* result) {
  Handle bitmap(api.FPDFBitmap_Create(kSide, kSide, 1), api.FPDFBitmap_Destroy); ASSERT_NE(bitmap.get(), nullptr);
  ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, kSide, kSide, 0xffffffff));
  Pause pause; Job job(api, page);
  int status = api.FPDF_RenderPageBitmap_Start(bitmap.get(), page, 0, 0, kSide, kSide, 0, s.flags, &pause);
  int continuations = 0;
  while (status == FPDF_RENDER_TOBECONTINUED && continuations++ < 32)
    status = api.FPDF_RenderPage_Continue(page, &pause);
  ASSERT_EQ(status, FPDF_RENDER_DONE); EXPECT_LE(continuations, 32);
  if (s.require_pause) { EXPECT_GT(pause.calls, 0); EXPECT_GT(continuations, 0); }
  *result = runtime_test::Bitmap(api, bitmap.get());
}
void ObservePage(const Api& api, FPDF_PAGE page, const Spec& s, Artifacts& artifacts,
                 const std::string& label, Raster* result) {
  // Progressive first: an ordinary render must not warm the initial image job.
  ASSERT_NO_FATAL_FAILURE(PaintProgressive(api, page, s, result));
  ASSERT_NO_FATAL_FAILURE(Inspect(*result, s));
  const auto ordinary = RenderWithExactFlags(api, page, kSide, kSide, s.flags);
  ASSERT_NO_FATAL_FAILURE(Inspect(ordinary, s));
  CompareRaster(ordinary, *result, artifacts, label + "-ordinary-progressive", 1);
}
void ExerciseProgressiveVariant(const Spec& s) {
  const auto bytes = Fixture(s); Artifacts artifacts; std::array<Raster, 2> originals; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(s.name + "-" + writer->name()); const auto& api = writer->api();
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument); ASSERT_NE(doc.get(), nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(ObservePage(api, page.get(), s, artifacts, writer->name(), &originals[backend]));
    page.reset();
    for (FPDF_DWORD flags : {FPDF_NO_INCREMENTAL, FPDF_INCREMENTAL}) {
      const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-" + std::to_string(flags), flags);
      ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        SCOPED_TRACE(reader->name()); const auto& read = reader->api();
        Handle loaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument); ASSERT_NE(loaded.get(), nullptr);
        Handle reopened(read.FPDF_LoadPage(loaded.get(), 0), read.FPDF_ClosePage); ASSERT_NE(reopened.get(), nullptr);
        Raster after;
        ASSERT_NO_FATAL_FAILURE(ObservePage(read, reopened.get(), s, artifacts, writer->name() + "-" + reader->name() + "-" + std::to_string(flags), &after));
        CompareRaster(originals[backend], after, artifacts, writer->name() + "-" + reader->name() + "-saved-" + std::to_string(flags), 1);
      }
    }
    ++backend;
  }
  CompareRaster(originals[0], originals[1], artifacts, "native-rust", 1);
}
Spec Masked(Spec s) {
  s.image += " /SMask 8 0 R";
  s.extra8 = PdfStream("/Type /XObject /Subtype /Image /Width 4 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray", std::string("\0\x55\xaa\xff", 4));
  for (int i = 0; i < 4; ++i) for (auto& channel : s.colors[i]) channel = (channel * (i * 85) + 255 * (255 - i * 85)) / 255;
  return s;
}
std::vector<Spec> Cases() {
  std::vector<Spec> cases;
  Spec rgb; rgb.name = "GrayRgb8"; rgb.flags |= FPDF_GRAYSCALE; cases.push_back(rgb);
  Spec indexed = rgb; indexed.name = "GrayIndexed2";
  indexed.image = "/Width 4 /Height 1 /BitsPerComponent 2 /ColorSpace [/Indexed /DeviceRGB 3 <ff000000ff000000ff808080>]";
  indexed.samples = std::string(1, char(0x1b)); cases.push_back(indexed);
  Spec wide = rgb; wide.name = "GrayRgb16"; wide.image = "/Width 4 /Height 1 /BitsPerComponent 16 /ColorSpace /DeviceRGB"; wide.samples.clear();
  for (const auto& color : wide.colors) for (const auto value : color) { wide.samples.push_back(char(value)); wide.samples.push_back(char(value)); }
  cases.push_back(wide);
  Spec reflected = rgb; reflected.name = "GrayReflectedRgb"; reflected.matrix = "-64 0 0 48 80 16"; reflected.reflected = true; cases.push_back(reflected);
  Spec cropped = rgb; cropped.name = "GrayClippedRgb"; cropped.clipped = true; cases.push_back(cropped);
  Spec opaque_mask; opaque_mask.name = "RgbSoftMask"; cases.push_back(Masked(opaque_mask));
  Spec gray_mask = rgb; gray_mask.name = "GrayRgbSoftMask"; cases.push_back(Masked(gray_mask));
  Spec inverse; inverse.name = "InvertExponentialTransfer"; inverse.state = "/TR " + kInvert;
  for (auto& color : inverse.colors) for (auto& channel : color) channel = 255 - channel;
  cases.push_back(inverse);
  Spec gray_inverse = inverse; gray_inverse.name = "GrayAfterTransfer"; gray_inverse.flags |= FPDF_GRAYSCALE; cases.push_back(gray_inverse);
  Spec inverse_mask = inverse; inverse_mask.name = "TransferPreservesSoftMask"; cases.push_back(Masked(inverse_mask));
  Spec sampled = inverse; sampled.name = "SampledTransfer"; sampled.state = "/TR 7 0 R";
  sampled.extra7 = PdfStream("/FunctionType 0 /Domain [0 1] /Range [0 1] /Size [2] /BitsPerSample 8", std::string("\xff\0", 2)); cases.push_back(sampled);
  Spec calculator = inverse; calculator.name = "CalculatorTransfer"; calculator.state = "/TR 7 0 R";
  calculator.extra7 = PdfStream("/FunctionType 4 /Domain [0 1] /Range [0 1]", "{ 1 exch sub }"); cases.push_back(calculator);
  Spec channels; channels.name = "IndependentChannelTransfers";
  channels.state = "/TR [" + kInvert + " " + kIdentity + " << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [0] /N 1 >> " + kIdentity + "]";
  for (auto& color : channels.colors) { color[0] = 255 - color[0]; color[2] = 0; } cases.push_back(channels);
  Spec identity; identity.name = "ExplicitIdentityTransfer"; identity.state = "/TR [" + kIdentity + " " + kIdentity + " " + kIdentity + " " + kIdentity + "]"; cases.push_back(identity);
  for (int variant = 0; variant < 5; ++variant) {
    Spec pattern; pattern.pattern = true; pattern.samples = std::string(1, char(0x50));
    pattern.image = "/Width 4 /Height 1 /BitsPerComponent 1 /ImageMask true /Decode [0 1]";
    pattern.name = variant == 0 ? "ColoredPatternStencil" : variant == 1 ? "UncoloredPatternStencil" : variant == 2 ? "ReflectedPatternStencil" : variant == 3 ? "GrayPatternStencil" : "ClippedPatternStencil";
    pattern.uncolored = variant == 1; pattern.reflected = variant == 2; pattern.clipped = variant == 4;
    if (pattern.reflected) pattern.matrix = "-64 0 0 48 80 16";
    if (variant == 3) pattern.flags |= FPDF_GRAYSCALE;
    pattern.paint = pattern.uncolored ? "/G gs /PCS cs .2 .4 .6 /P scn " : "/G gs /Pattern cs /P scn ";
    pattern.extra7 = PdfStream("/Type /Pattern /PatternType 1 /PaintType " + std::string(pattern.uncolored ? "2" : "1") + " /TilingType 1 /BBox [0 0 16 16] /XStep 16 /YStep 16 /Resources << >>",
        pattern.uncolored ? "0 0 16 16 re f" : "1 0 0 rg 0 0 8 16 re f 0 0 1 rg 8 0 8 16 re f");
    cases.push_back(pattern);
  }
  Spec alpha; alpha.name = "ConstantOpacityRgb"; alpha.state = "/ca .5";
  for (auto& color : alpha.colors) for (auto& channel : color) channel = (channel + 255) / 2;
  cases.push_back(alpha);
  Spec multiply; multiply.name = "MultiplyAgainstKnownBackdrop"; multiply.state = "/BM /Multiply";
  multiply.prefix = ".5 .75 1 rg 16 16 64 48 re f ";
  for (auto& color : multiply.colors) { color[0] = color[0] * 127 / 255; color[1] = color[1] * 191 / 255; }
  cases.push_back(multiply);
  Spec large; large.name = "GrayLargeImageActuallyContinues"; large.require_pause = true; large.flags |= FPDF_GRAYSCALE;
  large.image = "/Width 1024 /Height 1024 /BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /RunLengthDecode";
  large.samples.clear(); for (int i = 0; i < 8192; ++i) { large.samples.push_back(char(129)); large.samples.push_back(char(64)); }
  large.samples.push_back(char(128)); for (auto& color : large.colors) color = {64, 64, 64}; cases.push_back(large);
  return cases;
}
class CoverageProgressiveImageVariants : public testing::TestWithParam<Spec> {};
TEST_P(CoverageProgressiveImageVariants, IndependentPixelsAndStaticRenderingSurviveProgressiveSaveReload) {
  ExerciseProgressiveVariant(GetParam());
}
INSTANTIATE_TEST_SUITE_P(ImageBranches, CoverageProgressiveImageVariants, testing::ValuesIn(Cases()),
                        [](const testing::TestParamInfo<Spec>& info) { return info.param.name; });
}  // namespace
}  // namespace pdfoxium::integration
