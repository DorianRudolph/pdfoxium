#include "runtime_fixture.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <fstream>
#include <functional>
#include <iterator>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

using RGB = std::array<int, 3>;
constexpr int kSide = 100;
constexpr int kFlags = FPDF_RENDER_NO_SMOOTHIMAGE;

std::string Bytes(const std::filesystem::path& path) {
  std::ifstream input(path, std::ios::binary);
  EXPECT_TRUE(input.good()) << path.u8string();
  return std::string(std::istreambuf_iterator<char>(input), {});
}
RGB Pixel(const Raster& image, int x, int pdf_y) {
  const auto offset = (static_cast<size_t>(image.height - 1 - pdf_y) * image.width + x) * 4;
  return {image.pixels[offset + 2], image.pixels[offset + 1], image.pixels[offset]};
}
void Color(const Raster& image, int x, int y, RGB expected, int delta = 0) {
  const auto actual = Pixel(image, x, y);
  for (size_t channel = 0; channel < 3; ++channel)
    EXPECT_NEAR(actual[channel], expected[channel], delta) << "at " << x << "," << y << " channel " << channel;
}
std::vector<uint8_t> Scene(const std::string& content, const std::string& resources,
                           const std::vector<std::string>& extra) {
  std::vector<std::string> objects = {
      "<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << " + resources + " >> /Contents 4 0 R >>",
      PdfStream("", content)};
  objects.insert(objects.end(), extra.begin(), extra.end());
  return PdfFixture(objects);
}
// Saving the original streams retains source-only graphics state constructs
// such as patterns and transparency groups; no content regeneration is needed.
void CompareReaders(const std::vector<uint8_t>& bytes, Artifacts& artifacts, const std::string& name,
             const std::function<void(const Raster&, bool)>& inspect, int delta = 0,
             int flags = kFlags, bool native_exception = false) {
  std::array<Raster, 2> originals;
  size_t index = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(name + " writer=" + writer->name());
    const auto& api = writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    originals[index] = Render(api, page.get(), kSide, kSide, flags);
    ASSERT_NO_FATAL_FAILURE(inspect(originals[index], writer->name() == "rust"));
    const auto saved = SaveDocument(api, document.get(), artifacts, name + "-" + writer->name());
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE("reader=" + reader->name());
      const auto& read = reader->api();
      Handle reloaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reloaded.get(), nullptr);
      Handle after(read.FPDF_LoadPage(reloaded.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(after.get(), nullptr);
      const auto raster = Render(read, after.get(), kSide, kSide, flags);
      ASSERT_NO_FATAL_FAILURE(inspect(raster, reader->name() == "rust"));
      if (native_exception && reader != writer) {
        // The reader's renderer owns the documented deviation, irrespective
        // of which backend saved the source streams. Compare its reload with
        // its own rendering of the original fixture, checked independently.
        Handle source(read.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), read.FPDF_CloseDocument);
        ASSERT_NE(source.get(), nullptr);
        Handle original(read.FPDF_LoadPage(source.get(), 0), read.FPDF_ClosePage);
        ASSERT_NE(original.get(), nullptr);
        const auto reference = Render(read, original.get(), kSide, kSide, flags);
        ASSERT_NO_FATAL_FAILURE(inspect(reference, reader->name() == "rust"));
        CompareRaster(reference, raster, artifacts, name + "-" + writer->name() + "-" + reader->name(), delta);
      } else {
        CompareRaster(originals[index], raster, artifacts, name + "-" + writer->name() + "-" + reader->name(), delta);
      }
    }
    ++index;
  }
  if (!native_exception) CompareRaster(originals[0], originals[1], artifacts, name + "-backends", delta);
}
void Compare(const std::vector<uint8_t>& bytes, Artifacts& artifacts, const std::string& name,
             const std::function<void(const Raster&)>& inspect, int delta = 0,
             int flags = kFlags) {
  CompareReaders(bytes, artifacts, name, [&](const Raster& raster, bool) { inspect(raster); }, delta, flags);
}
std::string Pattern(const std::string& content, bool colored = true,
                    const std::string& geometry = "/BBox [0 0 20 20] /XStep 20 /YStep 20") {
  return PdfStream("/Type /Pattern /PatternType 1 /PaintType " + std::string(colored ? "1" : "2") +
      " /TilingType 1 " + geometry + " /Resources << >>", content);
}

TEST(CoverageCompositing, TextFillAndStrokePatternsPaintActualEmbeddedGlyphOutlines) {
  Artifacts artifacts;
  const auto font = Bytes(TestRoot() / "upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
  ASSERT_FALSE(font.empty());
  for (int mode : {0, 1, 2}) {
    const auto bytes = Scene(
        "/Pattern cs /Blue scn /Pattern CS /Red SCN 4 w BT /F 40 Tf " + std::to_string(mode) +
        " Tr 20.25 50.25 Td (A) Tj ET",
        "/Font << /F 5 0 R >> /Pattern << /Blue 8 0 R /Red 9 0 R >>",
        {"<< /Type /Font /Subtype /TrueType /BaseFont /Ahem /FirstChar 65 /LastChar 65 /Widths [1000] "
         "/Encoding /WinAnsiEncoding /FontDescriptor 6 0 R >>",
         "<< /Type /FontDescriptor /FontName /Ahem /Flags 32 /FontBBox [0 -200 1000 800] /ItalicAngle 0 "
         "/Ascent 800 /Descent -200 /StemV 80 /FontFile2 7 0 R >>",
         PdfStream("/Length1 " + std::to_string(font.size()), font),
         Pattern("0 0 1 rg 0 0 10 20 re f"), Pattern("1 0 0 rg 0 0 20 20 re f")});
    ASSERT_NO_FATAL_FAILURE(Compare(bytes, artifacts, "pattern-text-mode" + std::to_string(mode), [mode](const Raster& raster) {
      Color(raster, 25, 60, mode == 1 ? RGB{255, 255, 255} : RGB{0, 0, 255});
      Color(raster, 35, 60, {255, 255, 255});
      Color(raster, 19, 60, mode == 0 ? RGB{255, 255, 255} : RGB{255, 0, 0});
      Color(raster, 80, 60, {255, 255, 255});
    }, 1));
  }
}

TEST(CoverageCompositing, ColoredAndUncoloredPatternsAreMultipliedByRealImageStencils) {
  Artifacts artifacts;
  for (bool uncolored : {false, true}) for (bool reverse : {false, true}) {
    const auto bytes = Scene(
        std::string(uncolored ? "/PC cs 0 0 1 /P scn " : "/Pattern cs /P scn ") + "q 80 0 0 80 10 10 cm /I Do Q",
        "/XObject << /I 5 0 R >> /Pattern << /P 6 0 R >> /ColorSpace << /PC [/Pattern /DeviceRGB] >>",
        {PdfStream("/Type /XObject /Subtype /Image /Width 8 /Height 4 /ImageMask true /BitsPerComponent 1 " +
             std::string(reverse ? "/Decode [1 0]" : ""), std::string("\x0f\x0f\xf0\xf0", 4)),
         Pattern(uncolored ? "0 0 10 20 re f" : "0 0 1 rg 0 0 10 20 re f", !uncolored)});
    ASSERT_NO_FATAL_FAILURE(Compare(bytes, artifacts, "pattern-stencil-" + std::to_string(uncolored) + std::to_string(reverse),
        [reverse](const Raster& raster) {
          Color(raster, 25, 70, reverse ? RGB{255, 255, 255} : RGB{0, 0, 255});
          Color(raster, 65, 70, reverse ? RGB{0, 0, 255} : RGB{255, 255, 255});
          Color(raster, 25, 30, reverse ? RGB{0, 0, 255} : RGB{255, 255, 255});
          Color(raster, 65, 30, reverse ? RGB{255, 255, 255} : RGB{0, 0, 255});
          Color(raster, 35, 70, {255, 255, 255});
          Color(raster, 1, 1, {255, 255, 255});
        }));
  }
}

TEST(CoverageCompositing, OversizedTilingCellsRenderDirectlyInsideSmallClips) {
  Artifacts artifacts;
  for (bool uncolored : {false, true}) for (bool shifted : {false, true}) {
    const auto bytes = Scene("q 10 10 80 80 re W n " +
        std::string(uncolored ? "/PC cs 0 0 1 /P scn " : "/Pattern cs /P scn ") + "0 0 100 100 re f Q",
        "/Pattern << /P 5 0 R >> /ColorSpace << /PC [/Pattern /DeviceRGB] >>",
        {Pattern(std::string(uncolored ? "" : "0 0 1 rg ") + "0 0 50 200 re f", !uncolored,
            "/BBox [0 0 200 200] /XStep 200 /YStep 200 " +
            std::string(shifted ? "/Matrix [1 0 0 1 20 0]" : ""))});
    ASSERT_NO_FATAL_FAILURE(Compare(bytes, artifacts, "oversized-pattern-" + std::to_string(uncolored) + std::to_string(shifted),
        [shifted](const Raster& raster) {
          Color(raster, 30, 50, {0, 0, 255});
          Color(raster, 60, 50, shifted ? RGB{0, 0, 255} : RGB{255, 255, 255});
          Color(raster, 80, 50, {255, 255, 255});
          Color(raster, 5, 50, {255, 255, 255});
          Color(raster, 30, 5, {255, 255, 255});
        }));
  }
}

using ColorF = std::array<double, 3>;
// PDF 32000-1, tables 136/137: independent scalar blend equations, evaluated
// from the fixture's declared colors rather than reading rendered pixels.
double Lum(ColorF c) { return .3 * c[0] + .59 * c[1] + .11 * c[2]; }
double Sat(ColorF c) { return *std::max_element(c.begin(), c.end()) - *std::min_element(c.begin(), c.end()); }
ColorF SetLum(ColorF c, double target) {
  const double adjustment = target - Lum(c);
  for (auto& v : c) v += adjustment;
  const double low = *std::min_element(c.begin(), c.end());
  const double high = *std::max_element(c.begin(), c.end());
  if (low < 0) for (auto& v : c) v = target + (v - target) * target / (target - low);
  if (high > 1) for (auto& v : c) v = target + (v - target) * (1 - target) / (high - target);
  return c;
}
ColorF SetSat(ColorF c, double saturation) {
  std::array<size_t, 3> order{0, 1, 2};
  std::sort(order.begin(), order.end(), [&](size_t a, size_t b) { return c[a] < c[b]; });
  const auto lo = order[0], mid = order[1], hi = order[2];
  if (c[hi] > c[lo]) {
    c[mid] = (c[mid] - c[lo]) * saturation / (c[hi] - c[lo]);
    c[hi] = saturation;
  } else c[mid] = c[hi] = 0;
  c[lo] = 0;
  return c;
}
ColorF Blend(const std::string& mode, ColorF source, ColorF back) {
  if (mode == "Hue") return SetLum(SetSat(source, Sat(back)), Lum(back));
  if (mode == "Saturation") return SetLum(SetSat(back, Sat(source)), Lum(back));
  if (mode == "Color") return SetLum(source, Lum(back));
  if (mode == "Luminosity") return SetLum(back, Lum(source));
  ColorF result{};
  for (size_t i = 0; i < 3; ++i) {
    const double s = source[i], b = back[i];
    if (mode == "Normal") result[i] = s;
    else if (mode == "Multiply") result[i] = b * s;
    else if (mode == "Screen") result[i] = b + s - b * s;
    else if (mode == "Overlay") result[i] = b <= .5 ? 2 * b * s : 1 - 2 * (1 - b) * (1 - s);
    else if (mode == "Darken") result[i] = std::min(b, s);
    else if (mode == "Lighten") result[i] = std::max(b, s);
    else if (mode == "ColorDodge") result[i] = s == 1 ? 1 : std::min(1., b / (1 - s));
    else if (mode == "ColorBurn") result[i] = s == 0 ? 0 : 1 - std::min(1., (1 - b) / s);
    else if (mode == "HardLight") result[i] = s <= .5 ? 2 * b * s : 1 - 2 * (1 - b) * (1 - s);
    else if (mode == "SoftLight") {
      const double d = b <= .25 ? ((16 * b - 12) * b + 4) * b : std::sqrt(b);
      result[i] = s <= .5 ? b - (1 - 2 * s) * b * (1 - b) : b + (2 * s - 1) * (d - b);
    } else if (mode == "Difference") result[i] = std::abs(b - s);
    else if (mode == "Exclusion") result[i] = b + s - 2 * b * s;
    else ADD_FAILURE() << "unknown blend mode " << mode;
  }
  return result;
}
RGB Rounded(ColorF color) {
  RGB result{};
  for (size_t i = 0; i < 3; ++i) result[i] = static_cast<int>(std::lround(std::clamp(color[i], 0., 1.) * 255));
  return result;
}

TEST(CoverageCompositing, SeparableAndNonseparableBlendModesHaveAnalyticalOverlapColors) {
  Artifacts artifacts;
  const ColorF back{.85, .2, .5}, source{.2, .7, .4};
  for (const std::string mode : {"Normal", "Multiply", "Screen", "Overlay", "Darken", "Lighten", "ColorDodge", "ColorBurn",
                                "HardLight", "SoftLight", "Difference", "Exclusion", "Hue", "Saturation", "Color", "Luminosity"}) {
    // Both a path and an image force the chosen blend through different
    // compositor inputs. The overlap and each non-overlap color are observed.
    for (bool image : {false, true}) {
      const auto content = std::string(".85 .2 .5 rg 10 10 60 80 re f q /G gs ") +
          (image ? "40 0 0 60 50 20 cm /I Do" : ".2 .7 .4 rg 50 20 40 60 re f") + " Q";
      const auto bytes = Scene(content, "/ExtGState << /G << /BM /" + mode + " >> >> /XObject << /I 5 0 R >>",
          {PdfStream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8",
              std::string("\x33\xb3\x66", 3))});
      const auto actual_source = image ? ColorF{51. / 255, 179. / 255, 102. / 255} : source;
      const auto expected = Rounded(Blend(mode, actual_source, back));
      const auto over_white = Rounded(Blend(mode, actual_source, {1, 1, 1}));
      ASSERT_NO_FATAL_FAILURE(Compare(bytes, artifacts, "blend-" + mode + (image ? "-image" : "-path"),
          [&](const Raster& raster) {
            Color(raster, 30, 50, Rounded(back), 1);
            Color(raster, 60, 50, expected, 2);
            Color(raster, 80, 50, over_white, 2);
            Color(raster, 1, 1, {255, 255, 255});
          }, 1));
    }
  }
}

TEST(CoverageCompositing, NestedTransparencyGroupOpacityIsAppliedOnceAndIsolationChangesBlending) {
  Artifacts artifacts;
  for (bool isolated : {false, true}) for (bool nested : {false, true}) {
    // Two red rectangles overlap inside a half-opacity group. Their union
    // must remain purple over blue; painting them at half alpha separately
    // would incorrectly darken the overlap. Nested group alpha multiplies.
    const auto child = PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 80 80] "
        "/Group << /S /Transparency /I " + std::string(isolated ? "true" : "false") + " >> /Resources << >>",
        "1 0 0 rg 0 0 50 80 re f 30 0 50 80 re f");
    std::vector<std::string> extra{child};
    if (nested) extra.push_back(PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 80 80] "
        "/Group << /S /Transparency /I true >> /Resources << /XObject << /Inner 5 0 R >> "
        "/ExtGState << /Half << /ca .5 >> >> >>", "/Half gs /Inner Do"));
    const auto bytes = Scene("0 0 1 rg 0 0 100 100 re f q /Half gs 1 0 0 1 10 10 cm /F Do Q",
        "/ExtGState << /Half << /ca .5 >> >> /XObject << /F " + std::string(nested ? "6" : "5") + " 0 R >>", extra);
    const auto expected = nested ? RGB{64, 0, 191} : RGB{128, 0, 127};
    ASSERT_NO_FATAL_FAILURE(Compare(bytes, artifacts, "group-opacity-" + std::to_string(isolated) + std::to_string(nested),
        [&](const Raster& raster) {
          Color(raster, 25, 50, expected, 2);
          Color(raster, 50, 50, expected, 2);
          Color(raster, 75, 50, expected, 2);
          Color(raster, 5, 50, {0, 0, 255});
        }, 1));
  }
  for (bool isolated : {false, true}) {
    const auto bytes = Scene("0 0 1 rg 0 0 100 100 re f q 1 0 0 1 10 10 cm /F Do Q",
        "/XObject << /F 5 0 R >>", {PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 80 80] "
            "/Group << /S /Transparency /I " + std::string(isolated ? "true" : "false") + " >> "
            "/Resources << /ExtGState << /M << /BM /Multiply >> >> >>",
            "/M gs 1 0 0 rg 0 0 80 80 re f")});
    ASSERT_NO_FATAL_FAILURE(Compare(bytes, artifacts, "isolated-multiply-" + std::to_string(isolated),
        [isolated](const Raster& raster) {
          Color(raster, 50, 50, isolated ? RGB{255, 0, 0} : RGB{0, 0, 0});
          Color(raster, 5, 50, {0, 0, 255});
        }, 1));
  }
}

TEST(CoverageCompositing, SpotAndCmykImageOverprintUsesDarkenOnlyUnderItsRequiredState) {
  Artifacts artifacts;
  struct State { const char* name; const char* dictionary; bool darken; bool half; };
  const std::array<State, 5> states{{
      {"disabled", "/op false /OPM 0", false, false},
      {"enabled", "/op true /OPM 0", true, false},
      {"mode-one", "/op true /OPM 1", false, false},
      {"fill-alpha", "/op true /OPM 0 /ca .5", false, true},
      {"stroke-alpha", "/op true /OPM 0 /CA .5", false, false}}};
  for (int family = 0; family < 3; ++family) for (const auto& state : states) {
    const std::string space = family == 0 ? "/DeviceCMYK" : family == 1 ?
        "[/Separation /Yellow /DeviceRGB 6 0 R]" : "[/DeviceN [/Yellow /Aux] /DeviceRGB 6 0 R]";
    const std::string samples = family == 0 ? std::string("\0\0\xff\0", 4) : family == 1 ?
        std::string("\xff", 1) : std::string("\xff\xff", 2);
    const std::string function = family == 2 ?
        PdfStream("/FunctionType 4 /Domain [0 1 0 1] /Range [0 1 0 1 0 1]", "{ pop pop 1 1 0 }") :
        "<< /FunctionType 2 /Domain [0 1] /C0 [1 1 1] /C1 [1 1 0] /N 1 >>";
    const auto bytes = Scene("0 0 1 rg 0 0 100 100 re f q /G gs 80 0 0 80 10 10 cm /I Do Q",
        "/ExtGState << /G << " + std::string(state.dictionary) + " >> >> /XObject << /I 5 0 R >>",
        {PdfStream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace " + space, samples), function});
    ASSERT_NO_FATAL_FAILURE(CompareReaders(bytes, artifacts, "overprint-" + std::to_string(family) + "-" + state.name,
        [&](const Raster& raster, bool rust) {
          Color(raster, 5, 50, {0, 0, 255});
          if (state.darken) {
            // Native's CFX_AggDeviceDriver::StartDIBitsWithBlend discards its
            // blend argument, so overprint's Darken never reaches AGG. Rust
            // retains it. Validate the complete independent expected frame
            // for both readers rather than accepting native's yellow in Rust.
            for (int y = 0; y < kSide; ++y) for (int x = 0; x < kSide; ++x) {
              const bool inside = x >= 10 && x < 90 && y >= 10 && y < 90;
              const RGB source = family == 0 ? RGB{255, 241, 0} : RGB{255, 255, 0};
              Color(raster, x, y, inside ? (rust ? RGB{0, 0, 0} : source) : RGB{0, 0, 255}, 1);
            }
          }
          else if (family != 0) Color(raster, 50, 50, state.half ? RGB{127, 127, 128} : RGB{255, 255, 0}, 1);
          else {
            // Stock CMYK conversion uses its color table rather than the
            // idealized subtractive equation. Both outcomes must be yellow
            // (or yellow mixed halfway with the blue backdrop).
            const auto color = Pixel(raster, 50, 50);
            EXPECT_GT(color[0], state.half ? 100 : 200);
            EXPECT_GT(color[1], state.half ? 90 : 180);
            EXPECT_NEAR(color[2], state.half ? 128 : 0, 2);
          }
        }, 1, kFlags, state.darken));
  }
}

struct CodecPause : IFSDK_PAUSE {
  CodecPause(const Api& api, FPDF_PAGE page) : api(api), page(page) {
    version = 1;
    user = this;
    NeedToPauseNow = Pause;
  }
  static FPDF_BOOL Pause(IFSDK_PAUSE* descriptor) noexcept {
    auto* self = static_cast<CodecPause*>(descriptor);
    ++self->calls;
    self->valid &= descriptor->user == self && self->api.FPDF_GetPageWidthF(self->page) > 0;
    return self->pause;
  }
  const Api& api;
  FPDF_PAGE page;
  size_t calls = 0;
  bool pause = true;
  bool valid = true;
};
struct CodecJob {
  const Api& api;
  FPDF_PAGE page;
  ~CodecJob() { api.FPDF_RenderPage_Close(page); }
};
void ProgressiveJbig(const Library& implementation, FPDF_PAGE page, Artifacts& artifacts,
                     const std::string& name, Raster* completed) {
  const auto& api = implementation.api();
  // Do not perform synchronous rendering first: it would populate the image
  // cache and bypass the real JBIG2 decoder's progressive state machine.
  for (bool cancel : {true, false}) {
    Handle bitmap(api.FPDFBitmap_CreateEx(691, 432, FPDFBitmap_BGRA, nullptr, 0), api.FPDFBitmap_Destroy);
    ASSERT_NE(bitmap.get(), nullptr);
    ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, 691, 432, 0xffffffff));
    CodecPause pause(api, page);
    CodecJob job{api, page};
    int status = api.FPDF_RenderPageBitmap_Start(bitmap.get(), page, 0, 0, 691, 432, 0, kFlags | FPDF_NO_NATIVETEXT, &pause);
    if (cancel) {
      api.FPDF_RenderPage_Close(page);
      ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, 691, 432, 0xffffffff));
      status = api.FPDF_RenderPageBitmap_Start(bitmap.get(), page, 0, 0, 691, 432, 0, kFlags | FPDF_NO_NATIVETEXT, &pause);
    }
    size_t continuations = 0;
    while (status == FPDF_RENDER_TOBECONTINUED && continuations < 4096) {
      // Repeated callbacks drive actual row boundaries, then finish without
      // pausing. The public contract permits distinct backend pause cadence.
      if (continuations == 8) pause.pause = false;
      status = api.FPDF_RenderPage_Continue(page, &pause);
      ++continuations;
    }
    ASSERT_EQ(status, FPDF_RENDER_DONE);
    EXPECT_GT(pause.calls, 0u);
    EXPECT_TRUE(pause.valid);
    const auto raster = runtime_test::Bitmap(api, bitmap.get());
    size_t ink = 0;
    for (int y = 30; y < 120; ++y) for (int x = 30; x < 220; ++x)
      ink += Pixel(raster, x, raster.height - 1 - y) != RGB{255, 255, 255};
    EXPECT_GT(ink, 100u);
    EXPECT_LT(ink, 2000u);
    Color(raster, 300, 200, {255, 255, 255});
    api.FPDF_RenderPage_Close(page);
    const auto synchronous = Render(api, page, 691, 432, kFlags);
    CompareRaster(synchronous, raster, artifacts, name + (cancel ? "-cancel-restart" : "-cached"));
    if (!cancel) *completed = raster;
  }
}

TEST(CoverageCompositing, ProgressiveJbig2GlobalsSurviveCancellationCacheReuseAndCrossReload) {
  Artifacts artifacts;
  const auto data = Bytes(TestRoot() / "upstream/pdfium/testing/resources/bug_631912.pdf");
  ASSERT_FALSE(data.empty());
  std::array<Raster, 2> originals;
  size_t index = 0;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    Handle document(api.FPDF_LoadMemDocument64(data.data(), data.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(ProgressiveJbig(*writer, page.get(), artifacts, writer->name(), &originals[index]));
    // A second live page handle shares its document's image/global-symbol
    // cache while owning a separate progressive renderer and cancellation.
    {
      Handle second(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(second.get(), nullptr);
      Raster cached;
      ASSERT_NO_FATAL_FAILURE(ProgressiveJbig(*writer, second.get(), artifacts, writer->name() + "-second-page", &cached));
      CompareRaster(originals[index], cached, artifacts, writer->name() + "-shared-global-cache");
    }
    const auto saved = SaveDocument(api, document.get(), artifacts, writer->name() + "-progressive-jbig2");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      Handle after(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(after.get(), nullptr);
      Raster raster;
      ASSERT_NO_FATAL_FAILURE(ProgressiveJbig(*reader, after.get(), artifacts, writer->name() + "-" + reader->name(), &raster));
      CompareRaster(originals[index], raster, artifacts, writer->name() + "-" + reader->name() + "-jbig2-reload");
    }
    ++index;
  }
  CompareRaster(originals[0], originals[1], artifacts, "progressive-jbig2-backends");
}

// Independent masks decoded with jbig2dec: hash visible coverage bytes
// (0/255), excluding bitmap stride.
// This retains exact positions as well as ink counts; native's omitted dark
// masks are the documented oracle defect, never the expected Rust result.
struct PrivacyMaskExpectation { uint64_t hash; size_t ink; };
constexpr PrivacyMaskExpectation kPrivacyMasks[2][6] = {
    {{0x4e2e83414a2332a6ULL, 77179}, {0xc3e3835ee4353519ULL, 7178},
     {0xdaf6a425f7eca222ULL, 25175}, {0x5e7c272c09713fd0ULL, 25785},
     {0xace261e1fdb6f3faULL, 8149}, {0x837c36b508d01e16ULL, 9577}},
    {{0x9cf4bcfe89994b82ULL, 97015}, {0x17197c150d35d551ULL, 14844},
     {0xfced19fcc8ebcf02ULL, 36525}, {0x68f3eb1d1de2f52cULL, 36245},
     {0xd805080f7c22904dULL, 16104}, {0xf086d3e1f8a66988ULL, 20247}},
};

TEST(CoverageCompositing, Jbig2ShiftedRefinementsKeepAllPrivacyMasksAndProgressivePixels) {
  Artifacts artifacts;
  const auto bytes = Bytes(TestRoot() /
      "tests/render_corpus/pdfs/right_to_privacy_first_two_pages.pdf");
  ASSERT_FALSE(bytes.empty());
  for (const auto* implementation : Implementations()) {
    const auto& api = implementation->api();
    const bool rust = implementation->name() == "rust";
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    ASSERT_EQ(api.FPDF_GetPageCount(document.get()), 2);
    for (int index = 0; index < 2; ++index) {
      SCOPED_TRACE(implementation->name() + " page " + std::to_string(index));
      Handle page(api.FPDF_LoadPage(document.get(), index), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      // Render cold before any bitmap query can populate the decode cache.
      Raster progressive;
      for (bool cancel : {true, false}) {
        Handle bitmap(api.FPDFBitmap_CreateEx(390, 619, FPDFBitmap_BGRA, nullptr, 0), api.FPDFBitmap_Destroy);
        ASSERT_NE(bitmap.get(), nullptr);
        ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, 390, 619, 0xffffffff));
        CodecPause pause(api, page.get());
        CodecJob job{api, page.get()};
        int status = api.FPDF_RenderPageBitmap_Start(bitmap.get(), page.get(), 0, 0, 390, 619,
            0, FPDF_NO_NATIVETEXT, &pause);
        ASSERT_EQ(status, FPDF_RENDER_TOBECONTINUED);
        if (cancel) {
          api.FPDF_RenderPage_Close(page.get());
          continue;
        }
        size_t continuations = 0;
        while (status == FPDF_RENDER_TOBECONTINUED && continuations < 4096) {
          if (continuations == 8) pause.pause = false;
          status = api.FPDF_RenderPage_Continue(page.get(), &pause);
          ++continuations;
        }
        ASSERT_EQ(status, FPDF_RENDER_DONE);
        EXPECT_GT(continuations, 0u);
        EXPECT_GT(pause.calls, 0u);
        EXPECT_TRUE(pause.valid);
        progressive = runtime_test::Bitmap(api, bitmap.get());
      }
      size_t dark = 0;
      for (size_t i = 0; i < progressive.pixels.size(); i += 4)
        dark += progressive.pixels[i] < 128;
      if (rust) {
        EXPECT_GT(dark, 4000u);
      } else {
        EXPECT_EQ(dark, 0u);  // Native's surviving masks are gray 0.8/1.0.
      }
      const auto synchronous = Render(api, page.get(), 390, 619);
      CompareRaster(synchronous, progressive, artifacts,
          implementation->name() + "-privacy-progressive-" + std::to_string(index));

      int mask = 0;
      for (int object_index = 0; object_index < api.FPDFPage_CountObjects(page.get()); ++object_index) {
        auto object = api.FPDFPage_GetObject(page.get(), object_index);
        if (api.FPDFPageObj_GetType(object) != FPDF_PAGEOBJ_IMAGE) continue;
        ASSERT_LT(mask, 6);
        Handle bitmap(api.FPDFImageObj_GetBitmap(object), api.FPDFBitmap_Destroy);
        const bool native_decodes = mask == 4 || (index == 0 && mask == 5);
        if (!rust && !native_decodes) {
          EXPECT_EQ(bitmap.get(), nullptr);
        } else {
          ASSERT_NE(bitmap.get(), nullptr);
          ASSERT_EQ(api.FPDFBitmap_GetWidth(bitmap.get()), 1083);
          ASSERT_EQ(api.FPDFBitmap_GetHeight(bitmap.get()), 1717);
          ASSERT_EQ(api.FPDFBitmap_GetFormat(bitmap.get()), FPDFBitmap_Gray);
          const auto* data = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
          const int stride = api.FPDFBitmap_GetStride(bitmap.get());
          ASSERT_NE(data, nullptr);
          ASSERT_GE(stride, 1083);
          uint64_t hash = 14695981039346656037ULL;
          size_t ink = 0;
          for (int y = 0; y < 1717; ++y) for (int x = 0; x < 1083; ++x) {
            const auto value = data[static_cast<size_t>(y) * stride + x];
            hash = (hash ^ value) * 1099511628211ULL;
            ink += value != 0;
          }
          EXPECT_EQ(hash, kPrivacyMasks[index][mask].hash) << "mask " << mask;
          EXPECT_EQ(ink, kPrivacyMasks[index][mask].ink) << "mask " << mask;
        }
        ++mask;
      }
      EXPECT_EQ(mask, 6);
    }
  }
}

}  // namespace
}  // namespace pdfoxium::integration
