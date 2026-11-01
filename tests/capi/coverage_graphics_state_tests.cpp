#include "runtime_fixture.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using RGB = std::array<int, 3>;
using RGBA = std::array<int, 4>;
constexpr int kSide = 100;
constexpr int kCrisp = FPDF_RENDER_NO_SMOOTHIMAGE | FPDF_RENDER_NO_SMOOTHPATH;

RGBA At(const Raster& raster, int x, int pdf_y) {
  const size_t offset = (static_cast<size_t>(raster.height - 1 - pdf_y) * raster.width + x) * 4;
  return {raster.pixels[offset + 2], raster.pixels[offset + 1], raster.pixels[offset], raster.pixels[offset + 3]};
}
void Pixel(const Raster& raster, int x, int y, RGBA expected, int tolerance = 0) {
  const auto actual = At(raster, x, y);
  for (size_t channel = 0; channel < 4; ++channel)
    EXPECT_NEAR(actual[channel], expected[channel], tolerance) << "pixel " << x << ',' << y << " channel " << channel;
}
std::vector<uint8_t> Scene(const std::string& content, const std::string& resources,
                           const std::vector<std::string>& extra = {}, const std::string& catalog = "",
                           const std::string& page = "") {
  std::vector<std::string> objects = {
      "<< /Type /Catalog /Pages 2 0 R " + catalog + " >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << " + resources +
          " >> /Contents 4 0 R " + page + " >>", PdfStream("", content)};
  objects.insert(objects.end(), extra.begin(), extra.end());
  return PdfFixture(objects);
}
struct Surface {
  int format = FPDFBitmap_BGRA;
  int flags = kCrisp;
  uint32_t background = 0xffffffff;
};
Raster Paint(const Api& api, FPDF_PAGE page, Surface surface = {}) {
  Handle bitmap(api.FPDFBitmap_CreateEx(kSide, kSide, surface.format, nullptr, 0), api.FPDFBitmap_Destroy);
  if (!bitmap.get()) { ADD_FAILURE() << "Surface allocation failed"; return {}; }
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, kSide, kSide, surface.background));
  api.FPDF_RenderPageBitmap(bitmap.get(), page, 0, 0, kSide, kSide, 0, surface.flags | FPDF_NO_NATIVETEXT);
  const int stride = api.FPDFBitmap_GetStride(bitmap.get());
  const auto* bytes = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
  Raster raster{kSide, kSide, {}};
  raster.pixels.reserve(kSide * kSide * 4);
  const int step = surface.format == FPDFBitmap_Gray ? 1 : surface.format == FPDFBitmap_BGR ? 3 : 4;
  const bool reverse = (surface.flags & FPDF_REVERSE_BYTE_ORDER) != 0;
  for (int y = 0; y < kSide; ++y) for (int x = 0; x < kSide; ++x) {
    const auto* p = bytes + y * stride + x * step;
    if (step == 1) raster.pixels.insert(raster.pixels.end(), {p[0], p[0], p[0], 255});
    else raster.pixels.insert(raster.pixels.end(), {
        p[reverse ? 2 : 0], p[1], p[reverse ? 0 : 2],
        static_cast<uint8_t>(surface.format == FPDFBitmap_BGRA ? p[3] : 255)});
  }
  return raster;
}
void WorkflowReaders(const std::vector<uint8_t>& bytes, Artifacts& artifacts, const std::string& name,
              const std::function<void(const Raster&, bool)>& inspect, Surface surface = {}, int delta = 0,
              bool native_exception = false,
              const std::function<void(const Raster&, const Raster&)>& compare_exception = {}) {
  std::array<Raster, 2> originals;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(name + " writer=" + writer->name());
    const auto& api = writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    originals[backend] = Paint(api, page.get(), surface);
    ASSERT_EQ(originals[backend].pixels.size(), static_cast<size_t>(kSide * kSide * 4));
    ASSERT_NO_FATAL_FAILURE(inspect(originals[backend], writer->name() == "rust"));
    const auto saved = SaveDocument(api, document.get(), artifacts, name + "-" + writer->name());
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      const auto& read = reader->api();
      Handle reloaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reloaded.get(), nullptr);
      Handle loaded(read.FPDF_LoadPage(reloaded.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(loaded.get(), nullptr);
      const auto after = Paint(read, loaded.get(), surface);
      ASSERT_EQ(after.pixels.size(), static_cast<size_t>(kSide * kSide * 4));
      ASSERT_NO_FATAL_FAILURE(inspect(after, reader->name() == "rust"));
      if (native_exception && reader != writer) {
        Handle source(read.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), read.FPDF_CloseDocument);
        ASSERT_NE(source.get(), nullptr);
        Handle original(read.FPDF_LoadPage(source.get(), 0), read.FPDF_ClosePage);
        ASSERT_NE(original.get(), nullptr);
        const auto reference = Paint(read, original.get(), surface);
        ASSERT_NO_FATAL_FAILURE(inspect(reference, reader->name() == "rust"));
        CompareRaster(reference, after, artifacts, name + "-" + writer->name() + "-" + reader->name(), delta);
      } else {
        CompareRaster(originals[backend], after, artifacts, name + "-" + writer->name() + "-" + reader->name(), delta);
      }
    }
    ++backend;
  }
  if (!native_exception) CompareRaster(originals[0], originals[1], artifacts, name + "-backends", delta);
  else if (compare_exception) compare_exception(originals[0], originals[1]);
}
void Workflow(const std::vector<uint8_t>& bytes, Artifacts& artifacts, const std::string& name,
              const std::function<void(const Raster&)>& inspect, Surface surface = {}, int delta = 0) {
  WorkflowReaders(bytes, artifacts, name, [&](const Raster& raster, bool) { inspect(raster); }, surface, delta);
}
void LayerPixels(const Raster& image, bool visible) {
  Pixel(image, 50, 50, visible ? RGBA{255, 0, 0, 255} : RGBA{255, 255, 255, 255});
  Pixel(image, 5, 50, {255, 255, 255, 255});
  Pixel(image, 4, 4, {0, 0, 255, 255});
}
std::vector<uint8_t> Layer(const std::string& group, const std::string& properties) {
  return Scene("/OC /Layer BDC 1 0 0 rg 10 10 80 80 re f EMC 0 0 1 rg 2 2 4 4 re f",
      "/Properties << /Layer 5 0 R >>", {"<< /Type /OCG /Name (Layer) " + group + " >>"}, properties);
}

TEST(CoverageGraphicsState, LayerConfigurationsIntentsUsageAndApplicationStatesControlViewAndPrint) {
  struct Case { const char* name; std::string group, properties; bool view, print; };
  const std::string groups = "/OCProperties << /OCGs [5 0 R] ";
  const std::vector<Case> cases = {
      {"no-config", "", "", true, true},
      {"not-listed", "", "/OCProperties << /OCGs [] /D << /BaseState /OFF >> >>", true, true},
      {"base-off", "", groups + "/D << /BaseState /OFF >> >>", false, false},
      {"base-default", "", groups + "/D << >> >>", true, true},
      {"on-override", "", groups + "/D << /BaseState /OFF /ON [5 0 R] >> >>", true, true},
      {"off-override", "", groups + "/D << /OFF [5 0 R] >> >>", false, false},
      {"off-after-on", "", groups + "/D << /BaseState /OFF /ON [5 0 R] /OFF [5 0 R] >> >>", false, false},
      {"irrelevant-design", "/Intent /Design", groups + "/D << /BaseState /OFF >> >>", true, true},
      {"array-intent", "/Intent [/Design /View]", groups + "/D << /BaseState /OFF >> >>", false, false},
      {"all-intent", "/Intent /All", groups + "/D << /BaseState /OFF >> >>", false, false},
      {"array-all", "/Intent [/All]", groups + "/D << /BaseState /OFF >> >>", false, false},
      {"empty-intent", "/Intent []", groups + "/D << /BaseState /OFF >> >>", true, true},
      {"matching-config", "", groups + "/D << /BaseState /ON >> /Configs [<< /Intent /Design /BaseState /ON >> << /Intent [/View] /BaseState /OFF >>] >>", false, false},
      {"all-config", "", groups + "/D << /BaseState /ON >> /Configs [<< /Intent /All /BaseState /OFF >>] >>", false, false},
      {"default-config-fallback", "", groups + "/D << /BaseState /OFF >> /Configs [null << /BaseState /ON >> << /Intent /Design /BaseState /ON >>] >>", false, false},
      {"usage-precedence", "/Usage << /View << /ViewState /ON >> /Print << /PrintState /OFF >> >>", groups + "/D << /BaseState /OFF >> >>", true, false},
      {"print-usage", "/Usage << /View << /ViewState /OFF >> /Print << /PrintState /ON >> >>", "", false, true},
      {"print-view-fallback", "/Usage << /View << /ViewState /OFF >> /Print << >> >>", "", false, false},
      {"missing-usage-state", "/Usage << /View << >> >>", groups + "/D << /BaseState /OFF >> >>", false, false},
      // AS entries include the source-recognized state dictionary. Keep its
      // state equal to the declared base while testing relevant/ignored
      // categories and entries; unrelated state must never change a layer.
      {"application-states", "", groups + "/D << /BaseState /ON /AS [null << /Event /Export /OCGs [5 0 R] /View << /ViewState /OFF >> >> << /Event /View /OCGs [] /View << /ViewState /OFF >> >> << /Event /View /OCGs [5 0 R] /Category [/View] /View << /ViewState /ON >> >> << /Event /Print /OCGs [5 0 R] /Category [/Print] /Print << /PrintState /ON >> >>] >> >>", true, true},
      {"application-off", "", groups + "/D << /BaseState /OFF /AS [<< /OCGs [5 0 R] /Category [/View] /View << /ViewState /OFF >> >> << /Event /Print /OCGs [5 0 R] /Category [/Print] /Print << /PrintState /OFF >> >>] >> >>", false, false},
      {"application-missing-data", "", groups + "/D << /BaseState /OFF /AS [<< /Event /View >> << /Event /View /OCGs [5 0 R] >>] >> >>", false, false}};
  Artifacts artifacts;
  for (const auto& item : cases) for (bool printing : {false, true}) {
    SCOPED_TRACE(item.name);
    const auto bytes = Layer(item.group, item.properties);
    // PDF 1.7 §4.10.3: /D is the opening configuration; /Configs are
    // explicit alternatives. Usage metadata requires an /AS application.
    const std::string name = item.name;
    const bool alternate = name == "matching-config" || name == "all-config";
    const bool metadata_only = name == "usage-precedence";
    const bool expected = alternate ? true : metadata_only ? false : (printing ? item.print : item.view);
    const bool exception = expected != (printing ? item.print : item.view);
    WorkflowReaders(bytes, artifacts, name + (printing ? "-print" : "-view"),
        [&](const Raster& raster, bool rust) {
          LayerPixels(raster, rust ? expected : (printing ? item.print : item.view));
        }, {FPDFBitmap_BGRA, kCrisp | (printing ? FPDF_PRINTING : 0), 0xffffffff}, 0, exception);
  }
}

TEST(CoverageGraphicsState, MembershipPoliciesAndNestedVisibilityExpressionsUseBothLayerStates) {
  Artifacts artifacts;
  struct Policy { const char* name; std::function<bool(bool, bool)> value; };
  const std::array<Policy, 4> policies = {{{"AnyOn", [](bool a, bool b) { return a || b; }},
      {"AnyOff", [](bool a, bool b) { return !a || !b; }},
      {"AllOn", [](bool a, bool b) { return a && b; }},
      {"AllOff", [](bool a, bool b) { return !a && !b; }}}};
  auto check = [&](const std::string& membership, bool a, bool b, bool expected, const std::string& name, bool native_single_off = false) {
    const auto bytes = Scene("/OC /Layer BDC 1 0 0 rg 10 10 80 80 re f EMC 0 0 1 rg 2 2 4 4 re f",
        "/Properties << /Layer 7 0 R >>",
        {std::string("<< /Type /OCG /Name (A) /Usage << /View << /ViewState /") + (a ? "ON" : "OFF") + " >> >> >>",
         std::string("<< /Type /OCG /Name (B) /Usage << /View << /ViewState /") + (b ? "ON" : "OFF") + " >> >> >>",
         "<< /Type /OCMD " + membership + " >>"});
    WorkflowReaders(bytes, artifacts, name, [&](const Raster& raster, bool rust) {
      LayerPixels(raster, !rust && native_single_off ? !expected : expected);
    }, {}, 0, native_single_off);
  };
  for (bool a : {false, true}) for (bool b : {false, true}) {
    for (const auto& policy : policies) {
      check(std::string("/P /") + policy.name + " /OCGs [5 0 R 6 0 R]", a, b,
          policy.value(a, b), std::string(policy.name) + std::to_string(a) + std::to_string(b));
    }
    check("/VE [/And 5 0 R [/Not 6 0 R]]", a, b, a && !b, "and-not" + std::to_string(a) + std::to_string(b));
    check("/VE [/Or [/Not 5 0 R] 6 0 R]", a, b, !a || b, "or-not" + std::to_string(a) + std::to_string(b));
    check("/VE [/Not [/Or 5 0 R 6 0 R]]", a, b, !(a || b), "not-or" + std::to_string(a) + std::to_string(b));
    check("/VE [/And 5 0 R 6 0 R] /P /AnyOff /OCGs [5 0 R]", a, b, a && b, "expression-precedes-policy" + std::to_string(a) + std::to_string(b));
  }
  for (const auto& policy : policies)
    check(std::string("/P /") + policy.name + " /OCGs [null 3 false]", false, false, true, std::string(policy.name) + "-no-valid-groups");
  check("", false, false, true, "missing-groups");
  check("/OCGs 5 0 R", true, false, true, "single-group-on");
  check("/OCGs 5 0 R", false, true, false, "single-group-off");
  for (bool on : {false, true}) {
    check("/OCGs 5 0 R /P /AnyOff", on, false, !on, "single-anyoff" + std::to_string(on), true);
    check("/OCGs 5 0 R /P /AllOff", on, false, !on, "single-alloff" + std::to_string(on), true);
  }
  check("/OCGs 12", false, false, true, "non-list-groups");
  size_t malformed = 0;
  for (const std::string expression : {"[/Not]", "[/Not 7]", "[/And]", "[/Or 7]", "[/Unknown 5 0 R]"})
    check("/VE " + expression, true, false, false, "bounded-invalid-" + std::to_string(malformed++));
}

TEST(CoverageGraphicsState, NestedMarksAndOptionalFormImageAndAppearanceObjectsKeepTheirOwnVisibility) {
  Artifacts artifacts;
  for (bool on : {false, true}) for (bool printing : {false, true}) {
    const auto bytes = Scene(
        "/OC /On BDC /OC /Layer BDC 1 0 0 rg 5 10 20 20 re f EMC EMC "
        "q 1 0 0 1 30 10 cm /F Do Q q 20 0 0 20 55 10 cm /I Do Q "
        "/OC /Layer BDC 0 0 1 rg 5 50 20 20 re f EMC /OC /Layer BDC 0 0 1 rg 30 50 20 20 re f EMC "
        "0 0 1 rg 2 2 2 2 re f",
        "/Properties << /Layer 5 0 R /On 6 0 R >> /XObject << /F 7 0 R /I 8 0 R >>",
        {std::string("<< /Type /OCG /Name (Main) /Usage << /View << /ViewState /") + (on ? "ON" : "OFF") +
             " >> /Print << /PrintState /" + (on ? "OFF" : "ON") + " >> >> >>",
         "<< /Type /OCG /Name (Always) /Usage << /View << /ViewState /ON >> >> >>",
         PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 20 20] /Resources << >> /OC 5 0 R", "1 0 0 rg 0 0 20 20 re f"),
         PdfStream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceRGB /OC 5 0 R", std::string("\xff\0\0", 3)),
         "<< /Type /Annot /Subtype /Square /Rect [80 10 100 30] /F 4 /AP << /N 10 0 R >> >>",
         PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 20 20] /Resources << /Properties << /Layer 5 0 R >> >>",
             "/OC /Layer BDC 1 0 0 rg 0 0 20 20 re f EMC")},
        "", "/Annots [9 0 R]");
    const bool visible = printing ? !on : on;
    Workflow(bytes, artifacts, "optional-wrappers-" + std::to_string(on) + std::to_string(printing),
        [&](const Raster& image) {
          for (int x : {15, 40, 65, 90})
            Pixel(image, x, 20, visible ? RGBA{255, 0, 0, 255} : RGBA{255, 255, 255, 255});
          for (int x : {15, 40})
            Pixel(image, x, 60, visible ? RGBA{0, 0, 255, 255} : RGBA{255, 255, 255, 255});
          Pixel(image, 3, 3, {0, 0, 255, 255});
        }, {FPDFBitmap_BGRA, kCrisp | FPDF_ANNOT | (printing ? FPDF_PRINTING : 0), 0xffffffff});
  }
}

using Color = std::array<double, 3>;
double Lum(const Color& color) { return .30 * color[0] + .59 * color[1] + .11 * color[2]; }
Color SetLum(Color color, double luminance) {
  const double offset = luminance - Lum(color);
  for (double& channel : color) channel += offset;
  const double level = Lum(color), low = *std::min_element(color.begin(), color.end());
  const double high = *std::max_element(color.begin(), color.end());
  if (low < 0) for (double& channel : color) channel = level + (channel - level) * level / (level - low);
  if (high > 1) for (double& channel : color) channel = level + (channel - level) * (1 - level) / (high - level);
  return color;
}
Color Blend(const std::string& mode, Color source, const Color& back) {
  if (mode == "Color") return SetLum(source, Lum(back));
  if (mode == "Luminosity") return SetLum(back, Lum(source));
  for (size_t channel = 0; channel < 3; ++channel) {
    if (mode == "Multiply") source[channel] *= back[channel];
    else if (mode == "Screen") source[channel] = 1 - (1 - source[channel]) * (1 - back[channel]);
  }
  return source;
}
RGBA Composite(const std::string& mode, RGB source, int source_alpha, RGB back, int back_alpha) {
  Color src{}, dst{};
  for (size_t channel = 0; channel < 3; ++channel) { src[channel] = source[channel] / 255.; dst[channel] = back[channel] / 255.; }
  const auto blend = Blend(mode, src, dst);
  const double sa = source_alpha / 255., ba = back_alpha / 255., alpha = sa + ba * (1 - sa);
  RGBA result{};
  for (size_t channel = 0; channel < 3; ++channel)
    result[channel] = static_cast<int>(std::lround((sa * (1 - ba) * src[channel] + sa * ba * blend[channel] +
        (1 - sa) * ba * dst[channel]) / alpha * 255));
  result[3] = static_cast<int>(std::lround(alpha * 255));
  return result;
}

TEST(CoverageGraphicsState, TranslucentImageBlendModesRespectRealSurfaceAlphaGrayAndByteOrder) {
  Artifacts artifacts;
  for (int format : {FPDFBitmap_Gray, FPDFBitmap_BGR, FPDFBitmap_BGRx, FPDFBitmap_BGRA})
    for (bool reverse : {false, true}) for (int alpha : {0, 128, 255}) {
      if ((format == FPDFBitmap_Gray && reverse) || (format != FPDFBitmap_BGRA && alpha != 255)) continue;
      for (const std::string mode : {"Normal", "Multiply", "Screen", "Color", "Luminosity"}) {
        const bool gray = format == FPDFBitmap_Gray;
        const RGB color = gray ? RGB{192, 192, 192} : RGB{255, 0, 160};
        std::string samples;
        for (int channel : color) samples.push_back(static_cast<char>(channel));
        // Opaque outputs re-render the PDF page's isolated backdrop when
        // blending; a caller-supplied bitmap fill is not that PDF backdrop.
        // Supply the gray rectangle in the PDF for these rows. BGRA rows keep
        // their genuine external alpha0/128/255 backdrop to test alpha union.
        const std::string background = format == FPDFBitmap_BGRA ? "" :
            "0.1254902 g 0 0 100 100 re f ";
        const auto bytes = Scene(background + "q /G gs 80 0 0 80 10 10 cm /I Do Q",
            "/ExtGState << /G << /BM /" + mode + " >> >> /XObject << /I 5 0 R >>",
            {PdfStream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceRGB /SMask 6 0 R", samples),
             PdfStream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray", std::string(1, '\x80'))});
        const auto expected = Composite(mode, color, 128, {32, 32, 32}, alpha);
        const auto name = "alpha-surface-" + std::to_string(format) + std::to_string(reverse) + std::to_string(alpha) + mode;
        Workflow(bytes, artifacts, name, [&](const Raster& raster) {
          Pixel(raster, 50, 50, expected, 2);
          Pixel(raster, 5, 5, {32, 32, 32, alpha});
        }, {format, kCrisp | (reverse ? FPDF_REVERSE_BYTE_ORDER : 0), (static_cast<uint32_t>(alpha) << 24) | 0x202020}, 2);
      }
    }
}

TEST(CoverageGraphicsState, IndexedImagesAndStencilsUsePaletteAndCoverageUnderCurvedClips) {
  Artifacts artifacts;
  const std::string clip = "50 10 m 72.091 10 90 27.909 90 50 c 90 72.091 72.091 90 50 90 c "
      "27.909 90 10 72.091 10 50 c 10 27.909 27.909 10 50 10 c h W n ";
  for (int bits : {1, 8}) for (bool mask : {false, true}) {
    if (mask && bits != 1) continue;
    const auto payload = bits == 1 ? std::string("\x40\x80", 2) : std::string("\0\1\1\0", 4);
    const std::string dictionary = "/Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent " + std::to_string(bits) +
        (mask ? " /ImageMask true" : " /ColorSpace [/Indexed /DeviceRGB 1 <FF000000FF00>]");
    const auto bytes = Scene("q " + clip + "0.2 0.4 0.8 rg 80 0 0 80 10 10 cm /I Do Q",
        "/XObject << /I 5 0 R >>", {PdfStream(dictionary, payload)});
    for (int format : {FPDFBitmap_Gray, FPDFBitmap_BGR, FPDFBitmap_BGRx, FPDFBitmap_BGRA}) for (bool reverse : {false, true}) {
      if (format == FPDFBitmap_Gray && reverse) continue;
      auto gray = [format](RGBA value) {
        if (format == FPDFBitmap_Gray) value[0] = value[1] = value[2] = (value[0] * 30 + value[1] * 59 + value[2] * 11) / 100;
        return value;
      };
      Workflow(bytes, artifacts, "clip-indexed-" + std::to_string(bits) + std::to_string(mask) + std::to_string(format) + std::to_string(reverse),
          [&](const Raster& raster) {
            const auto zero = gray(mask ? RGBA{51, 102, 204, 255} : RGBA{255, 0, 0, 255});
            const auto one = gray(mask ? RGBA{32, 32, 32, 255} : RGBA{0, 255, 0, 255});
            Pixel(raster, 30, 70, zero, 1); Pixel(raster, 70, 70, one, 1);
            Pixel(raster, 30, 30, one, 1); Pixel(raster, 70, 30, zero, 1);
            Pixel(raster, 11, 11, {32, 32, 32, 255});
          }, {format, kCrisp | (reverse ? FPDF_REVERSE_BYTE_ORDER : 0), 0xff202020}, 1);
    }
  }
}

TEST(CoverageGraphicsState, Type3BitmapBatchesAndTranslucentColoredFormsPreserveGlyphCells) {
  Artifacts artifacts;
  struct Matrix { const char* value; std::vector<std::array<int, 2>> centers; std::array<int, 4> bounds; };
  const std::vector<Matrix> matrices = {
      {"1 0 0 1 10 30", {{20, 40}, {40, 40}, {60, 40}, {80, 40}}, {9, 29, 91, 51}},
      {"0 1 -1 0 80 5", {{70, 15}, {70, 35}, {70, 55}, {70, 75}}, {59, 4, 81, 86}},
      {"1 .2 .25 1 5 20", {{18, 32}, {38, 36}, {58, 40}, {78, 44}}, {4, 19, 91, 57}}};
  for (int kind : {0, 1, 2}) for (bool translucent : {false, true}) for (const auto& matrix : matrices) {
    const std::string image_glyph = "1000 0 0 0 1000 1000 d1 q 1000 0 0 1000 0 0 cm /Im Do Q";
    const std::string plain_glyph = "1000 0 0 0 1000 1000 d1 0 0 1000 1000 re f";
    const std::string colored_glyph = "1000 0 d0 1 0 0 rg 0 0 1000 1000 re f 0 0 1 rg 0 0 1000 200 re f";
    const auto bytes = Scene("q /G gs .2 .4 .8 rg BT /F 20 Tf " + std::string(matrix.value) + " Tm (ABBA) Tj ET Q",
        "/ExtGState << /G << /ca " + std::string(translucent ? ".5" : "1") + " >> >> /Font << /F 5 0 R >>",
        {"<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [.001 0 0 .001 0 0] "
             "/FirstChar 65 /LastChar 66 /Widths [1000 1000] /Encoding << /Differences [65 /A /B] >> "
             "/CharProcs << /A 6 0 R /B 7 0 R >> /Resources << /XObject << /Im 8 0 R >> >> >>",
         PdfStream("", kind == 2 ? colored_glyph : image_glyph),
         PdfStream("", kind == 0 ? image_glyph : kind == 1 ? plain_glyph : colored_glyph),
         PdfStream("/Type /XObject /Subtype /Image /Width 2 /Height 2 /ImageMask true /BitsPerComponent 1", std::string(2, '\0'))});
    const auto name = "type3-batch-" + std::to_string(kind) + std::to_string(translucent) + matrix.value;
    const bool exception = translucent && kind != 1;
    WorkflowReaders(bytes, artifacts, name, [&](const Raster& raster, bool rust) {
      const RGBA solid = kind == 2 ? RGBA{255, 0, 0, 255} : RGBA{51, 102, 204, 255};
      RGBA expected = translucent ? RGBA{(solid[0] + 255) / 2, (solid[1] + 255) / 2, (solid[2] + 255) / 2, 255} : solid;
      if (exception && !rust) {
        // Native applies text alpha twice to a combined bitmap-glyph mask,
        // while colored form glyphs omit that text alpha entirely. Rust must
        // apply it once for each kind. These affect both writers' readers.
        expected = kind == 0 ? RGBA{204, 217, 242, 255} : solid;
      }
      for (const auto& center : matrix.centers) Pixel(raster, center[0], center[1], expected, 2);
      Pixel(raster, 4, 4, {255, 255, 255, 255});
    }, {}, 2, exception, [&](const Raster& native, const Raster& rust) {
      // Limit the oracle exception to the glyph bounds (plus one AA pixel).
      // Opaque rows above already compare the entire glyph shape directly.
      const int left = matrix.bounds[0], top = kSide - matrix.bounds[3];
      const int right = matrix.bounds[2], bottom = kSide - matrix.bounds[1];
      for (const auto& rectangle : std::vector<std::array<int, 4>>{
          {0, 0, kSide, top}, {0, bottom, kSide, kSide - bottom},
          {0, top, left, bottom - top}, {right, top, kSide - right, bottom - top}}) {
        if (rectangle[2] <= 0 || rectangle[3] <= 0) continue;
        CompareRaster(CropRaster(native, rectangle[0], rectangle[1], rectangle[2], rectangle[3]),
                      CropRaster(rust, rectangle[0], rectangle[1], rectangle[2], rectangle[3]),
                      artifacts, name + "-outside-glyphs-" + std::to_string(rectangle[0]) + std::to_string(rectangle[1]));
      }
    });
  }
}

}  // namespace
}  // namespace pdfoxium::integration
