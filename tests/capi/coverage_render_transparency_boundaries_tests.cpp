// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "runtime_fixture.h"
#include "text_fixture.h"

#include <array>
#include <cmath>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using RGBA = std::array<int, 4>;
constexpr int kSide = 128;
constexpr int kCrisp = FPDF_RENDER_NO_SMOOTHIMAGE | FPDF_RENDER_NO_SMOOTHPATH;
struct Surface {
  int format = FPDFBitmap_BGRA;
  int flags = kCrisp;
  unsigned background = 0xffffffff;
};
Bytes Scene(const std::string& content, const std::string& resources = "",
            const std::vector<std::string>& extra = {}) {
  std::vector<std::string> objects{
      "<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << " + resources +
          " >> /Contents 4 0 R >>", PdfStream("", content)};
  objects.insert(objects.end(), extra.begin(), extra.end()); return PdfFixture(objects);
}
RGBA At(const Raster& r, int x, int y) {
  const auto p = (size_t(kSide - y - 1) * r.width + x) * 4;
  return {r.pixels.at(p + 2), r.pixels.at(p + 1), r.pixels.at(p), r.pixels.at(p + 3)};
}
void Pixel(const Raster& r, int x, int y, RGBA expected, int tolerance = 0) {
  ASSERT_EQ(r.width, kSide); ASSERT_EQ(r.height, kSide); const auto actual = At(r, x, y);
  for (int c = 0; c < 4; ++c) EXPECT_NEAR(actual[c], expected[c], tolerance) << x << ',' << y << ':' << c;
}
void Guard(const Raster& r) { Pixel(r, 2, 2, {255, 255, 255, 255}); Pixel(r, 125, 125, {255, 255, 255, 255}); }
RGBA Gray(RGBA rgb, int format) {
  if (format == FPDFBitmap_Gray) { const auto g = (30 * rgb[0] + 59 * rgb[1] + 11 * rgb[2]) / 100; rgb[0] = rgb[1] = rgb[2] = g; }
  return rgb;
}
struct Pause : IFSDK_PAUSE {
  Pause() { version = 1; user = this; NeedToPauseNow = Need; }
  static FPDF_BOOL Need(IFSDK_PAUSE* value) { ++static_cast<Pause*>(value)->calls; return 1; }
  int calls = 0;
};
Raster Paint(const Api& api, FPDF_PAGE page, Surface surface, bool progressive = false) {
  Handle bitmap(api.FPDFBitmap_CreateEx(kSide, kSide, surface.format, nullptr, 0), api.FPDFBitmap_Destroy);
  if (!bitmap.get()) { ADD_FAILURE() << "Bitmap allocation failed"; return {}; }
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, kSide, kSide, surface.background));
  if (progressive) {
    Pause pause;
    int status = api.FPDF_RenderPageBitmap_Start(bitmap.get(), page, 0, 0, kSide, kSide, 0, surface.flags | FPDF_NO_NATIVETEXT, &pause);
    int continuations = 0;
    while (status == FPDF_RENDER_TOBECONTINUED && continuations++ < 1000) status = api.FPDF_RenderPage_Continue(page, &pause);
    EXPECT_EQ(status, FPDF_RENDER_DONE); EXPECT_LT(continuations, 1000);
    api.FPDF_RenderPage_Close(page);
  } else {
    api.FPDF_RenderPageBitmap(bitmap.get(), page, 0, 0, kSide, kSide, 0, surface.flags | FPDF_NO_NATIVETEXT);
  }
  const auto* data = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
  const int stride = api.FPDFBitmap_GetStride(bitmap.get());
  const int step = surface.format == FPDFBitmap_Gray ? 1 : surface.format == FPDFBitmap_BGR ? 3 : 4;
  const bool reverse = (surface.flags & FPDF_REVERSE_BYTE_ORDER) != 0;
  Raster out{kSide, kSide, {}}; out.pixels.reserve(kSide * kSide * 4);
  for (int y = 0; y < kSide; ++y) for (int x = 0; x < kSide; ++x) {
    const auto* p = data + y * stride + x * step;
    if (step == 1) out.pixels.insert(out.pixels.end(), {p[0], p[0], p[0], 255});
    else {
      const unsigned alpha = surface.format == FPDFBitmap_BGRA ? p[3] : 255;
      // Premultiplied observations preserve actual color/coverage while RGB
      // in fully transparent pixels remains outside the observable contract.
      out.pixels.insert(out.pixels.end(), {
          uint8_t((unsigned(p[reverse ? 2 : 0]) * alpha + 127) / 255),
          uint8_t((unsigned(p[1]) * alpha + 127) / 255),
          uint8_t((unsigned(p[reverse ? 0 : 2]) * alpha + 127) / 255), uint8_t(alpha)});
    }
  }
  return out;
}
using Semantic = std::function<void(const Api&, FPDF_PAGE)>;
void ExerciseTransparency(const Bytes& input, const std::string& raw_label, const std::function<void(const Raster&)>& inspect,
         Surface surface = {}, int tolerance = 0, const Semantic& semantic = {}, bool progressive = false, bool text = false,
         const std::function<void(const Raster&)>& native_inspect = {}, const Bytes& healthy_control = {}) {
  // Resource dictionaries are useful trace labels, but are not file names.
  // Keep saved artifacts portable and below common component-length limits.
  std::string label; uint32_t hash = 2166136261u;
  for (const unsigned char c : raw_label) {
    hash = (hash ^ c) * 16777619u;
    if (label.size() < 70) label.push_back((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
        (c >= '0' && c <= '9') || c == '-' || c == '_' ? char(c) : '_');
  }
  label += "-" + std::to_string(hash);
  Artifacts artifacts; std::array<Raster, 2> originals; size_t index = 0;
  auto compare = [&](const Raster& a, const Raster& b, const std::string& name) {
    if (text) text_fixture::CompareTextRaster(a, b, artifacts, name);
    else CompareRaster(a, b, artifacts, name, tolerance);
  };
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(label + "-" + writer->name()); const auto& api = writer->api();
    Handle document(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument); ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
    if (semantic) { ASSERT_NO_FATAL_FAILURE(semantic(api, page.get())); }
    const auto& check_writer = native_inspect && writer->name() == "native" ? native_inspect : inspect;
    originals[index] = Paint(api, page.get(), surface);
    ASSERT_NO_FATAL_FAILURE(check_writer(originals[index]));
    artifacts.Image(label + "-original-" + writer->name(), originals[index]);
    if (progressive) { const auto after = Paint(api, page.get(), surface, true);
     ASSERT_NO_FATAL_FAILURE(inspect(after)); compare(originals[index], after, label + "-progressive"); }
    const auto saved = SaveDocument(api, document.get(), artifacts, label + writer->name()); ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name()); const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument); ASSERT_NE(reopened.get(), nullptr);
      Handle loaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage); ASSERT_NE(loaded.get(), nullptr);
      if (semantic) { ASSERT_NO_FATAL_FAILURE(semantic(read, loaded.get())); }
      const auto after = Paint(read, loaded.get(), surface);
      const auto& check_reader = native_inspect && reader->name() == "native" ? native_inspect : inspect;
      ASSERT_NO_FATAL_FAILURE(check_reader(after));
      if (writer == reader) CompareRaster(originals[index], after, artifacts, label + "-same-reader");
      else if (native_inspect) {
        // Preserve each engine's original appearance independently of which
        // writer serialized it. A native defect belongs to the reader.
        Handle source(read.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), read.FPDF_CloseDocument); ASSERT_NE(source.get(), nullptr);
        Handle original(read.FPDF_LoadPage(source.get(), 0), read.FPDF_ClosePage); ASSERT_NE(original.get(), nullptr);
        const auto expected = Paint(read, original.get(), surface);
     ASSERT_NO_FATAL_FAILURE(check_reader(expected));
        CompareRaster(expected, after, artifacts, label + writer->name() + reader->name());
      } else compare(originals[index], after, label + writer->name() + reader->name());
    }
    ++index;
  }
  if (!native_inspect) compare(originals[0], originals[1], label + "-engines");
  else {
    ASSERT_FALSE(healthy_control.empty());
    // A separate, valid PDF expresses the same expected drawing without the
    // defective native path. Rust's original must match both healthy readers.
    for (const auto* control_reader : Implementations()) {
      const auto& read = control_reader->api();
      Handle control(read.FPDF_LoadMemDocument64(healthy_control.data(), healthy_control.size(), nullptr), read.FPDF_CloseDocument); ASSERT_NE(control.get(), nullptr);
      Handle page(read.FPDF_LoadPage(control.get(), 0), read.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
      if (semantic) { ASSERT_NO_FATAL_FAILURE(semantic(read, page.get())); }
      const auto expected = Paint(read, page.get(), surface);
     ASSERT_NO_FATAL_FAILURE(inspect(expected));
      compare(expected, originals[1], label + "-healthy-" + control_reader->name());
    }
  }
}

void FillStroke(int format, bool reverse, bool fractional, bool transparent = false) {
  const auto p = fractional ? "20.25 25.25 80 70 re B" : "20 25 80 70 re B";
  Surface surface{format, (reverse ? FPDF_REVERSE_BYTE_ORDER : 0), transparent ? 0u : 0xffffffffu};
  ExerciseTransparency(Scene(std::string("/G gs 1 0 0 rg 0 0 1 RG 12 w ") + p,
      "/ExtGState << /G << /ca .5 /CA .5 >> >>"), "fill-stroke-" + std::to_string(format) + std::to_string(reverse) + std::to_string(fractional) + std::to_string(transparent),
      [=](const Raster& r) {
        if (transparent) {
          Pixel(r, 50, 50, {128, 0, 0, 128}, 1); Pixel(r, 20, 60, {0, 0, 128, 128}, 1); Pixel(r, 2, 2, {0, 0, 0, 0});
        } else {
          Guard(r); Pixel(r, 50, 50, Gray({255, 128, 128, 255}, format), 1);
          // The stroke replaces the fill contribution at this overlap.
          Pixel(r, 20, 60, Gray({128, 128, 255, 255}, format), 1);
        }
      }, surface, fractional ? 2 : 1);
}
TEST(CoverageRenderTransparencyBoundaries, TranslucentFillStrokeKnockoutUsesOpaqueBgrReadback) { for (bool r : {false, true}) for (bool f : {false, true}) FillStroke(FPDFBitmap_BGR, r, f); }
TEST(CoverageRenderTransparencyBoundaries, TranslucentFillStrokeKnockoutUsesOpaqueBgrxReadback) { for (bool r : {false, true}) for (bool f : {false, true}) FillStroke(FPDFBitmap_BGRx, r, f); }
TEST(CoverageRenderTransparencyBoundaries, TranslucentFillStrokeKnockoutUsesGrayReadback) { for (bool f : {false, true}) FillStroke(FPDFBitmap_Gray, false, f); }
TEST(CoverageRenderTransparencyBoundaries, TranslucentFillStrokeKnockoutPreservesAlphaOnTransparentBgra) { for (bool r : {false, true}) FillStroke(FPDFBitmap_BGRA, r, true, true); }
TEST(CoverageRenderTransparencyBoundaries, FractionalCurvedFillStrokeRetainsTheInteriorAndAnUntouchedExterior) {
  for (const int format : {FPDFBitmap_BGR, FPDFBitmap_BGRx, FPDFBitmap_BGRA}) for (const bool reverse : {false, true}) {
    ExerciseTransparency(Scene("/G gs 1 0 0 rg 0 0 1 RG 8 w 20.25 30.25 m 20.25 100.25 100.25 100.25 100.25 30.25 c h B",
        "/ExtGState << /G << /ca .5 /CA .5 >> >>"), "curved-knockout-" + std::to_string(format) + std::to_string(reverse),
        [](const Raster& r) { Guard(r); Pixel(r, 60, 45, {255, 128, 128, 255}, 1); Pixel(r, 60, 110, {255, 255, 255, 255}); },
        {format, reverse ? FPDF_REVERSE_BYTE_ORDER : 0, 0xffffffff}, 2);
  }
}

std::string Tiling(int size, bool colored, const std::string& matrix = "1 0 0 1 0 0", const std::string& body = "") {
  const auto n = std::to_string(size);
  return PdfStream("/Type /Pattern /PatternType 1 /PaintType " + std::string(colored ? "1" : "2") +
      " /TilingType 1 /BBox [0 0 " + n + " " + n + "] /XStep " + n + " /YStep " + n + " /Matrix [" + matrix + "] /Resources << >>",
      body.empty() ? (colored ? std::string("0 0 1 rg ") : "") + "0 0 " + n + " " + n + " re f" : body);
}
void Tiny(int size, bool colored, bool reverse, bool rotate, bool gray, bool progressive = false) {
  const auto matrix = rotate ? "0 1 -1 0 8 8" : "1 0 0 1 8 8";
  const auto prefix = colored ? "/Pattern cs /P scn " : "/PC cs 0 0 1 /P scn ";
  std::string commands = std::string(prefix) + "20 20 80 80 re f";
  if (progressive) for (int i = 0; i < 150; ++i) commands += " 0 0 1 rg " + std::to_string(4 + i % 120) + " 112 1 1 re f";
  const auto resources = "/Pattern << /P 5 0 R >> /ColorSpace << /PC [/Pattern /DeviceRGB] >>";
  ExerciseTransparency(Scene(commands, resources, {Tiling(size, colored, matrix)}), "tiny-cell-" + std::to_string(size) + std::to_string(colored) + std::to_string(reverse) + std::to_string(rotate) + std::to_string(gray),
      [=](const Raster& r) { Guard(r); Pixel(r, 30, 30, gray ? RGBA{28, 28, 28, 255} : RGBA{0, 0, 255, 255}, gray ? 1 : 0); Pixel(r, 70, 70, gray ? RGBA{28, 28, 28, 255} : RGBA{0, 0, 255, 255}, gray ? 1 : 0); },
      {gray ? FPDFBitmap_Gray : FPDFBitmap_BGRx, kCrisp | (reverse ? FPDF_REVERSE_BYTE_ORDER : 0), 0xffffffff}, 1, {}, progressive);
}
TEST(CoverageRenderTransparencyBoundaries, OnePixelColoredTilingCellsKeepEveryPaintedPixelAndPhase) { for (bool r : {false, true}) Tiny(1, true, r, false, false); }
TEST(CoverageRenderTransparencyBoundaries, OnePixelUncoloredTilingCellsApplyTheCallerColor) { for (bool r : {false, true}) Tiny(1, false, r, false, false); }
TEST(CoverageRenderTransparencyBoundaries, TwoPixelColoredCellsUseTheEnlargedResamplingPath) { for (bool r : {false, true}) Tiny(2, true, r, false, false); }
TEST(CoverageRenderTransparencyBoundaries, ThreePixelUncoloredCellsUseTheEnlargedResamplingPath) { for (bool r : {false, true}) Tiny(3, false, r, false, false); }
TEST(CoverageRenderTransparencyBoundaries, QuarterTurnTinyCellsKeepTheirAlignedGridAndNegativeColumns) { for (int n : {1, 2, 3}) for (bool c : {false, true}) Tiny(n, c, false, true, false); }
TEST(CoverageRenderTransparencyBoundaries, TinyPatternColorConversionUsesTheGrayDestinationLuminance) { for (int n : {1, 2, 3}) for (bool c : {false, true}) Tiny(n, c, false, false, true); }
TEST(CoverageRenderTransparencyBoundaries, ProgressiveTinyPatternsFinishWithTheSamePixelsAsStaticRendering) { for (bool c : {false, true}) Tiny(1, c, false, true, false, true); }

Bytes Type3(const std::string& glyph, const std::vector<std::string>& resources = {}, bool zero_bbox = false,
            bool zero_width = false, const std::string& font_resources = "", const std::string& text = "AA") {
  std::vector<std::string> objects{
      "<< /Type /Font /Subtype /Type3 /Name /F /FontBBox " + std::string(zero_bbox ? "[0 0 0 0]" : "[0 0 500 700]") +
      " /FontMatrix [.001 0 0 .001 0 0] /FirstChar 65 /LastChar 65 /Widths [" + std::string(zero_width ? "0" : "500") +
      "] /Encoding << /Differences [65 /A] >> /CharProcs << /A 6 0 R >> /Resources << " + font_resources + " >> >>",
      PdfStream("", glyph)};
  objects.insert(objects.end(), resources.begin(), resources.end());
  return Scene("0 0 1 rg BT /F 40 Tf 20 30 Td (" + text + ") Tj ET", "/Font << /F 5 0 R >>", objects);
}
void Type3Text(const Api& api, FPDF_PAGE page) {
  Handle text(api.FPDFText_LoadPage(page), api.FPDFText_ClosePage); ASSERT_NE(text.get(), nullptr);
  EXPECT_EQ(text_fixture::Extract(api, text.get()), u"AA");
  EXPECT_EQ(api.FPDFText_GetUnicode(text.get(), 0), unsigned('A'));
  double x = 0, y = 0; ASSERT_TRUE(api.FPDFText_GetCharOrigin(text.get(), 1, &x, &y)); EXPECT_NEAR(x, 40, .05); EXPECT_NEAR(y, 30, .05);
}
void Type3Blue(const Raster& r) { Guard(r); Pixel(r, 25, 40, {0, 0, 255, 255}); Pixel(r, 45, 40, {0, 0, 255, 255}); Pixel(r, 80, 40, {255, 255, 255, 255}); }
TEST(CoverageRenderTransparencyBoundaries, Type3ZeroFontBoundsAreRebuiltFromActualGlyphGeometry) {
  ExerciseTransparency(Type3("500 0 0 0 500 700 d1 0 0 500 700 re f", {}, true), "type3-metrics-bbox", Type3Blue, {}, 0, Type3Text);
}
TEST(CoverageRenderTransparencyBoundaries, Type3ZeroWidthsAreRecoveredFromTheDeclaredD1Advance) {
  ExerciseTransparency(Type3("500 0 0 0 500 700 d1 0 0 500 700 re f", {}, false, true), "type3-metrics-width", Type3Blue, {}, 0, Type3Text);
}
TEST(CoverageRenderTransparencyBoundaries, Type3ColoredD0BoundsAndAdvanceComeFromThePaintedForm) {
  ExerciseTransparency(Type3("500 0 d0 1 0 0 rg 0 0 500 700 re f", {}, true, true), "type3-colored-metrics",
      [](const Raster& r) { Guard(r); Pixel(r, 25, 40, {255, 0, 0, 255}); Pixel(r, 45, 40, {255, 0, 0, 255}); }, {}, 0, Type3Text);
}
TEST(CoverageRenderTransparencyBoundaries, NestedType3FontsUseSeparateActiveFontIdentitiesAndInheritedColor) {
  const auto child = "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 500 700] /FontMatrix [.001 0 0 .001 0 0] /FirstChar 66 /LastChar 66 /Widths [500] /Encoding << /Differences [66 /B] >> /CharProcs << /B 8 0 R >> /Resources << >> >>";
  const std::vector<std::string> children{child, PdfStream("", "500 0 0 0 500 700 d1 0 0 500 700 re f")};
  const auto input = Type3("500 0 0 0 500 700 d1 BT /Child 1000 Tf 0 0 Td (B) Tj ET", children,
      false, false, "/Font << /Child 7 0 R >>");
  // A colored outer glyph may explicitly set its child's text color. This
  // valid control avoids native's default-black reset at an uncolored nesting.
  const auto control = Type3("500 0 d0 0 0 1 rg BT /Child 1000 Tf 0 0 Td (B) Tj ET", children,
      false, false, "/Font << /Child 7 0 R >>");
  ExerciseTransparency(input, "type3-nested-identity", Type3Blue, {}, 0, Type3Text, false, false,
      [](const Raster& r) { Guard(r); Pixel(r, 25, 40, {0, 0, 0, 255}); Pixel(r, 45, 40, {0, 0, 0, 255}); Pixel(r, 80, 40, {255, 255, 255, 255}); }, control);
}
TEST(CoverageRenderTransparencyBoundaries, MirroredType3ImageGlyphsRetainTheOddTrailingMaskBit) {
  for (bool flip_x : {false, true}) for (bool flip_y : {false, true}) {
    const auto matrix = std::string(flip_x ? "-500" : "500") + " 0 0 " + (flip_y ? "-700" : "700") + " " + (flip_x ? "500" : "0") + " " + (flip_y ? "700" : "0");
    const std::string mask("\xff\x00\xff\x00\xff\x00", 6);
    const auto input = Type3("500 0 0 0 500 700 d1 q " + matrix + " cm /I Do Q", {
      PdfStream("/Type /XObject /Subtype /Image /ImageMask true /BitsPerComponent 1 /Width 9 /Height 3", mask)}, false, false, "/XObject << /I 7 0 R >>");
    ExerciseTransparency(input, "type3-flipped-bitmap-" + std::to_string(flip_x) + std::to_string(flip_y), [=](const Raster& r) {
      Guard(r); Pixel(r, flip_x ? 21 : 39, 40, {0, 0, 255, 255}); Pixel(r, 30, 40, {255, 255, 255, 255});
    }, {}, 1, Type3Text);
  }
}
TEST(CoverageRenderTransparencyBoundaries, EmptyType3BitmapGlyphsKeepTextPositionsWithoutInventingInk) {
  const auto input = Type3("500 0 0 0 500 700 d1 q 500 0 0 700 0 0 cm /I Do Q", {
      PdfStream("/Type /XObject /Subtype /Image /ImageMask true /BitsPerComponent 1 /Width 9 /Height 3", std::string(6, '\xff'))},
      false, false, "/XObject << /I 7 0 R >>");
  ExerciseTransparency(input, "type3-empty-bitmap", [](const Raster& r) { for (auto byte : r.pixels) EXPECT_EQ(byte, 255); }, {}, 0, Type3Text);
}

void ShadingStroke(bool text, bool half) {
  const auto font = text_fixture::Resource("fonts/ahem/Ahem.ttf"); const std::string data(font.begin(), font.end());
  const auto commands = std::string("/G gs /Pattern CS /P SCN 8 w ") + (text ? "BT /F 40 Tf 1 Tr 25 40 Td (A) Tj ET" : "20 30 m 100 30 l S");
  const auto resources = "/Pattern << /P 5 0 R >> /ExtGState << /G << /ca 1 /CA " + std::string(half ? ".5" : "1") + " >> >> /Font << /F 8 0 R >>";
  std::vector<std::string> objects{
        "<< /Type /Pattern /PatternType 2 /Shading 6 0 R >>",
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 128 0] /Function 7 0 R /Extend [true true] >>",
        "<< /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [1 0 0] /N 1 >>",
        "<< /Type /Font /Subtype /TrueType /BaseFont /Ahem /FirstChar 65 /LastChar 65 /Widths [1000] /Encoding /WinAnsiEncoding /FontDescriptor 9 0 R >>",
        "<< /Type /FontDescriptor /FontName /Ahem /Flags 32 /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /StemV 80 /FontFile2 10 0 R >>", PdfStream("/Length1 " + std::to_string(data.size()), data)};
  const auto input = Scene(commands, resources, objects);
  const auto inspect = [=](const Raster& r) {
    Guard(r); Pixel(r, text ? 25 : 50, text ? 55 : 30, half ? RGBA{255, 128, 128, 255} : RGBA{255, 0, 0, 255}, 1);
    Pixel(r, text ? 45 : 50, text ? 55 : 50, {255, 255, 255, 255});
  };
  if (!(text && half)) { ExerciseTransparency(input, std::string("shading-stroke-") + std::to_string(text) + std::to_string(half), inspect, {}, 1); return; }
  // Native DrawTextPathWithPattern omits the text object's general state when
  // reconstructing a stroked glyph path, so it loses CA. A half-opacity group
  // around the opaque glyph supplies an independent healthy alpha control.
  const auto opaque_resources = "/Pattern << /P 5 0 R >> /ExtGState << /G << /ca 1 /CA 1 >> >> /Font << /F 8 0 R >>";
  objects.push_back(PdfStream(std::string("/Type /XObject /Subtype /Form /BBox [0 0 128 128] /Group << /S /Transparency /I true >> /Resources << ") + opaque_resources + " >>", commands));
  const auto control = Scene("/Half gs /Control Do", "/ExtGState << /Half << /ca .5 >> >> /XObject << /Control 11 0 R >>", objects);
  ExerciseTransparency(input, "shading-stroke-11", inspect, {}, 1, {}, false, false,
      [](const Raster& r) { Guard(r); Pixel(r, 25, 55, {255, 0, 0, 255}); Pixel(r, 45, 55, {255, 255, 255, 255}); }, control);
}
TEST(CoverageRenderTransparencyBoundaries, ShadingPatternStrokeUsesStrokeOpacityRatherThanFillOpacity) { ShadingStroke(false, true); }
TEST(CoverageRenderTransparencyBoundaries, ShadingPatternTextStrokeClipsToTheActualEmbeddedGlyphOutline) { for (bool half : {false, true}) ShadingStroke(true, half); }
TEST(CoverageRenderTransparencyBoundaries, NonuniformStrokeTextCtmKeepsLogicalTextAndVisiblePathGeometry) {
  for (int mode : {1, 2, 5, 6}) {
    const auto input = Scene("q 1.5 .1 .25 .75 0 0 cm 1 0 0 RG 0 0 1 rg 1 w BT /F 40 Tf " + std::to_string(mode) +
        " Tr 20 45 Td (H) Tj ET Q", "/Font << /F 5 0 R >>", {"<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>"});
    ExerciseTransparency(input, "stroke-text-ctm-" + std::to_string(mode), [](const Raster& r) {
      Guard(r); size_t ink = 0; for (int y = 10; y < 100; ++y) for (int x = 10; x < 115; ++x) {
        const auto c = At(r, x, y); ink += std::min({c[0], c[1], c[2]}) < 160;
      } EXPECT_GT(ink, 20u); EXPECT_LT(ink, 1500u);
    }, {FPDFBitmap_BGRA, FPDF_RENDER_NO_SMOOTHIMAGE, 0xffffffff}, 0,
    [](const Api& api, FPDF_PAGE page) { Handle text(api.FPDFText_LoadPage(page), api.FPDFText_ClosePage); ASSERT_NE(text.get(), nullptr); EXPECT_EQ(text_fixture::Extract(api, text.get()), u"H"); }, false, true);
  }
}

void MaskBackground(const std::string& group, const std::string& bc, bool visible, const std::string& mask_extra = "", bool alpha = false) {
  const auto input = Scene("/G gs 1 0 0 rg 20 20 80 80 re f",
      "/ExtGState << /G << /SMask << /S /" + std::string(alpha ? "Alpha" : "Luminosity") + " /G 5 0 R /BC " + bc + " " + mask_extra + " >> >> >>",
      {PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 128 128] " + group + " /Resources << >>", "")});
  ExerciseTransparency(input, "mask-background-" + bc + group + mask_extra, [=](const Raster& r) { Guard(r); Pixel(r, 50, 50, visible ? RGBA{255, 0, 0, 255} : RGBA{255, 255, 255, 255}); });
}
TEST(CoverageRenderTransparencyBoundaries, SoftMaskBackgroundFallsBackWhenTheGroupColorSpaceIsAbsent) { for (const auto& g : {"", "/Group << /S /Transparency >>", "/Group << /S /Transparency /CS /UnknownRecoverySpace >>"}) MaskBackground(g, "[1 1 1]", false); }
TEST(CoverageRenderTransparencyBoundaries, SoftMaskBackgroundRejectsNonArrayBcWithoutInventingCoverage) { MaskBackground("/Group << /S /Transparency /CS /DeviceRGB >>", "/NotAnArray", false); }
TEST(CoverageRenderTransparencyBoundaries, SoftMaskLabAndPatternBackgroundsUseTheUnsupportedSpaceFallback) {
  for (const auto& cs : {"[/Lab << /WhitePoint [0.9505 1 1.089] >>]", "[/Pattern /DeviceRGB]"}) MaskBackground(std::string("/Group << /S /Transparency /CS ") + cs + " >>", "[1 1 1]", false);
}
TEST(CoverageRenderTransparencyBoundaries, SoftMaskGrayRgbAndCmykWhiteBackgroundsRevealTheActualSourceColor) {
  MaskBackground("/Group << /S /Transparency /CS /DeviceGray >>", "[1]", true);
  MaskBackground("/Group << /S /Transparency /CS /DeviceRGB >>", "[1 1 1]", true);
  MaskBackground("/Group << /S /Transparency /CS /DeviceCMYK >>", "[0 0 0 0]", true);
}
TEST(CoverageRenderTransparencyBoundaries, AlphaSoftMaskIgnoresTheLuminosityBackgroundColor) { MaskBackground("/Group << /S /Transparency /CS /DeviceRGB >>", "[1 1 1]", false, "", true); }
TEST(CoverageRenderTransparencyBoundaries, NonFunctionSoftMaskTransferEntriesKeepIdentityCoverage) { for (const auto& tr : {"/Identity", "null", "<< /FunctionType 99 >>"}) MaskBackground("/Group << /S /Transparency /CS /DeviceGray >>", "[1]", true, std::string("/TR ") + tr); }

void RejectedShadings(const std::vector<std::string>& entries, const std::string& name) {
  for (const auto& entry : entries) {
    SCOPED_TRACE(entry); const auto input = Scene("/S sh 0 1 0 rg 10 10 8 8 re f", "/Shading << /S 5 0 R >>", {entry});
    ExerciseTransparency(input, name + entry, [](const Raster& r) { Guard(r); Pixel(r, 14, 14, {0, 255, 0, 255}); Pixel(r, 60, 60, {255, 255, 255, 255}); });
  }
}
TEST(CoverageRenderTransparencyBoundaries, ShadingsWithoutUsableColorSpacesLeaveSubsequentContentUsable) { RejectedShadings({"<< /ShadingType 2 >>", "<< /ShadingType 2 /ColorSpace /UnknownRecoverySpace >>", "<< /ShadingType 2 /ColorSpace /Pattern >>"}, "shading-space"); }
TEST(CoverageRenderTransparencyBoundaries, InvalidShadingTypesAndNonStreamMeshesAreSafelySkipped) { RejectedShadings({"<< /ShadingType 0 /ColorSpace /DeviceRGB >>", "<< /ShadingType 8 /ColorSpace /DeviceRGB >>", "<< /ShadingType 4 /ColorSpace /DeviceRGB >>", "<< /ShadingType 5 /ColorSpace /DeviceRGB >>", "<< /ShadingType 6 /ColorSpace /DeviceRGB >>", "<< /ShadingType 7 /ColorSpace /DeviceRGB >>"}, "shading-type"); }
TEST(CoverageRenderTransparencyBoundaries, AxialShadingRejectsMissingInvalidAndDimensionallyWrongFunctions) {
  const std::string begin = "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 128 0] /Extend [true true] ";
  RejectedShadings({begin + ">>", begin + "/Function [null] >>", begin + "/Function << /FunctionType 99 >> >>", begin + "/Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >>", begin + "/Function [<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> null null] >>"}, "shading-function");
}

}  // namespace
}  // namespace pdfoxium::integration
