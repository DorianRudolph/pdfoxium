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
using RGB = std::array<int, 3>;
constexpr int kWidth = 160, kHeight = 120;
constexpr int kCrisp = FPDF_RENDER_NO_SMOOTHIMAGE | FPDF_RENDER_NO_SMOOTHPATH;

Bytes Scene(const std::string& content, const std::string& resources = "",
            const std::vector<std::string>& extra = {}) {
  std::vector<std::string> objects{
      "<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 160 120] /Resources << " + resources +
          " >> /Contents 4 0 R >>", PdfStream("", content)};
  objects.insert(objects.end(), extra.begin(), extra.end());
  return PdfFixture(objects);
}
RGB At(const Raster& r, int x, int y) {
  const auto p = (size_t(kHeight - y - 1) * r.width + x) * 4;
  return {r.pixels.at(p + 2), r.pixels.at(p + 1), r.pixels.at(p)};
}
void Pixel(const Raster& r, int x, int y, RGB expected, int delta = 0) {
  ASSERT_EQ(r.width, kWidth); ASSERT_EQ(r.height, kHeight);
  const auto actual = At(r, x, y);
  for (int c = 0; c < 3; ++c) EXPECT_NEAR(actual[c], expected[c], delta) << x << ',' << y;
}
void Guards(const Raster& r) {
  Pixel(r, 3, 3, {255, 255, 255}); Pixel(r, 155, 115, {255, 255, 255});
}
Raster Paint(const Api& api, FPDF_PAGE page, int flags, int format) {
  Handle bitmap(api.FPDFBitmap_CreateEx(kWidth, kHeight, format, nullptr, 0), api.FPDFBitmap_Destroy);
  if (!bitmap.get()) { ADD_FAILURE() << "Bitmap allocation failed"; return {}; }
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, kWidth, kHeight, 0xffffffff));
  api.FPDF_RenderPageBitmap(bitmap.get(), page, 0, 0, kWidth, kHeight, 0, flags | FPDF_NO_NATIVETEXT);
  return text_fixture::Bitmap(api, bitmap.get());
}
using Semantic = std::function<void(const Api&, FPDF_DOCUMENT, FPDF_PAGE)>;
void Workflow(const Bytes& bytes, const std::string& name,
              const std::function<void(const Raster&)>& inspect,
              const Semantic& semantic = {}, bool text = false, int delta = 0,
              int flags = kCrisp, int format = FPDFBitmap_BGRA) {
  Artifacts artifacts; std::array<Raster, 2> originals; size_t backend = 0;
  const auto compare = [&](const Raster& a, const Raster& b, const std::string& label) {
    if (text) text_fixture::CompareTextRaster(a, b, artifacts, label);
    else CompareRaster(a, b, artifacts, label, delta);
  };
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(name + "-" + writer->name()); const auto& api = writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
    if (semantic) { ASSERT_NO_FATAL_FAILURE(semantic(api, document.get(), page.get())); }
    originals[backend] = Paint(api, page.get(), flags, format);
    ASSERT_NO_FATAL_FAILURE(inspect(originals[backend]));
    const auto saved = SaveDocument(api, document.get(), artifacts, name + "-" + writer->name());
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api();
     SCOPED_TRACE(reader->name());
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      Handle loaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage); ASSERT_NE(loaded.get(), nullptr);
      if (semantic) { ASSERT_NO_FATAL_FAILURE(semantic(read, reopened.get(), loaded.get())); }
      const auto after = Paint(read, loaded.get(), flags, format);
      ASSERT_NO_FATAL_FAILURE(inspect(after));
      if (writer == reader) CompareRaster(originals[backend], after, artifacts, name + "-same-reader");
      else compare(originals[backend], after, name + "-" + writer->name() + "-" + reader->name());
    }
    ++backend;
  }
  compare(originals[0], originals[1], name + "-engines");
}
void VisibleTextAtLeast(const Raster& r, size_t minimum) {
  Guards(r); size_t ink = 0;
  for (int y = 10; y < 100; ++y) for (int x = 10; x < 140; ++x) {
    const auto c = At(r, x, y); ink += std::min({c[0], c[1], c[2]}) < 100;
  }
  EXPECT_GT(ink, minimum); EXPECT_LT(ink, 3000u);
}
void VisibleText(const Raster& r) { VisibleTextAtLeast(r, 12); }
Semantic Text(const std::u16string& expected, float size) {
  return [=](const Api& api, FPDF_DOCUMENT, FPDF_PAGE page) {
    Handle text(api.FPDFText_LoadPage(page), api.FPDFText_ClosePage); ASSERT_NE(text.get(), nullptr);
    EXPECT_EQ(text_fixture::Extract(api, text.get()), expected);
    EXPECT_NEAR(api.FPDFText_GetFontSize(text.get(), 0), size, .001);
    const auto object = api.FPDFPage_GetObject(page, 0); ASSERT_NE(object, nullptr);
    EXPECT_EQ(api.FPDFPageObj_GetType(object), FPDF_PAGEOBJ_TEXT);
    float actual_size = 0; ASSERT_TRUE(api.FPDFTextObj_GetFontSize(object, &actual_size)); EXPECT_NEAR(actual_size, size, .001);
    const auto font = api.FPDFTextObj_GetFont(object); ASSERT_NE(font, nullptr);
    float width = 0; ASSERT_TRUE(api.FPDFFont_GetGlyphWidth(font, expected.front(), size, &width));
    EXPECT_GT(width, 0); EXPECT_TRUE(std::isfinite(width));
  };
}
Bytes Standard(const std::string& font, const std::string& descriptor = "",
               const std::string& content = "BT /F 24 Tf 20 40 Td (H) Tj ET", bool truetype = false) {
  return Scene(content, "/Font << /F 5 0 R >>", {
      "<< /Type /Font /Subtype /" + std::string(truetype ? "TrueType" : "Type1") +
      " /BaseFont /" + font + " /Encoding /WinAnsiEncoding " +
      (descriptor.empty() ? "" : "/FontDescriptor 6 0 R ") + ">>",
      "<< /Type /FontDescriptor /FontName /" + font + (descriptor.find("/Flags") == std::string::npos ? " /Flags 32 " : " ") + " /FontBBox [-200 -250 1200 1000] "
      "/Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 " + descriptor + " >>"});
}
void FontNames(const std::vector<std::string>& names, const std::string& label,
               const std::string& descriptor = "", bool truetype = false) {
  for (const auto& name : names) {
    SCOPED_TRACE(name); Workflow(Standard(name, descriptor, "BT /F 24 Tf 20 40 Td (H) Tj ET", truetype),
        label + name, VisibleText, Text(u"H", 24), true);
  }
}

// Missing-family classification needs an unavailable system font, rather than
// whichever default face the host provider chooses after earlier suites have
// populated its inventory. Keep one callback owner per library alive until all
// documents and all four save/reload combinations have closed.
struct ScopedUnavailableSystemFonts : FPDF_SYSFONTINFO {
  explicit ScopedUnavailableSystemFonts(const Library& owner)
      : FPDF_SYSFONTINFO{}, library(owner) {
    version = 2;
    MapFont = [](FPDF_SYSFONTINFO*, int, FPDF_BOOL, int, int, const char*, FPDF_BOOL* exact) -> void* {
      if (exact) *exact = 0;
      return nullptr;
    };
    GetFont = [](FPDF_SYSFONTINFO*, const char*) -> void* { return nullptr; };
    library.api().FPDF_SetSystemFontInfo(this);
  }
  ~ScopedUnavailableSystemFonts() { library.restore_default_font_info(); }
  ScopedUnavailableSystemFonts(const ScopedUnavailableSystemFonts&) = delete;
  ScopedUnavailableSystemFonts& operator=(const ScopedUnavailableSystemFonts&) = delete;
  const Library& library;
};

TEST(CoverageRenderFontRecovery, ArialAndTimesPostscriptAliasesSelectUsableSubstituteFaces) {
  FontNames({"ArialMT", "Arial,Bold", "Arial,Italic", "TimesNewRomanPSMT", "TimesNewRomanPS-BoldMT"}, "postscript-alias-");
}
TEST(CoverageRenderFontRecovery, CourierAliasesAndSubsetPrefixesRetainTheRequestedLogicalText) {
  FontNames({"CourierNew", "CourierNewPSMT", "CourierNew,Bold", "ABCDEF+Courier", "UVWXYZ+Helvetica"}, "courier-alias-");
}
TEST(CoverageRenderFontRecovery, MissingSerifAndSansFamiliesUseTheDescriptorClassification) {
  const auto libraries = Implementations();
  ScopedUnavailableSystemFonts first(*libraries[0]), second(*libraries[1]);
  // Non-Base14 simple fonts provide PDF widths. Warm the shared generic
  // face at a different width to keep classification independent of native's
  // mutable MM instance when its missing-width fallback reads glyph metrics.
  const auto fixture = [](const std::string& family, int flags, int width) {
    return Scene("BT /F 24 Tf 20 40 Td (H) Tj ET", "/Font << /F 5 0 R >>", {
        "<< /Type /Font /Subtype /Type1 /BaseFont /" + family +
            " /Encoding /WinAnsiEncoding /FirstChar 72 /LastChar 72 /Widths [" + std::to_string(width) + "] /FontDescriptor 6 0 R >>",
        "<< /Type /FontDescriptor /FontName /" + family + " /Flags " + std::to_string(flags) +
            " /FontBBox [-200 -250 1200 1000] /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 >>"});
  };
  const auto warm = fixture("WarmRecoverySans", 32, 1000);
  for (const auto* library : libraries) {
    const auto& api = library->api();
    Handle document(api.FPDF_LoadMemDocument64(warm.data(), warm.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(VisibleText(Paint(api, page.get(), kCrisp, FPDFBitmap_BGRA)));
  }
  for (const bool serif : {false, true}) {
    const std::string family = serif ? "MissingRecoverySerif" : "MissingRecoverySans";
    const std::string expected = serif ? "Chrome Serif MM" : "Chrome Sans MM";
    const Semantic classified = [expected](const Api& api, FPDF_DOCUMENT doc, FPDF_PAGE page) {
      ASSERT_NO_FATAL_FAILURE(Text(u"H", 24)(api, doc, page));
      const auto font = api.FPDFTextObj_GetFont(api.FPDFPage_GetObject(page, 0));
      ASSERT_NE(font, nullptr);
      EXPECT_EQ(api.FPDFFont_GetIsEmbedded(font), 0);
      std::array<char, 128> actual{};
      ASSERT_EQ(api.FPDFFont_GetFamilyName(font, actual.data(), actual.size()), expected.size() + 1);
      EXPECT_EQ(std::string(actual.data()), expected);
    };
    Workflow(fixture(family, serif ? 34 : 32, 600),
             std::string(serif ? "serif-default-" : "sans-default-") + family,
             VisibleText, classified, true);
  }
}
TEST(CoverageRenderFontRecovery, ForcedBoldAndItalicDescriptorsPreserveNonemptyGlyphGeometry) {
  FontNames({"Courier", "Helvetica", "Times-Roman"}, "forced-bold-", "/Flags 262176 /ItalicAngle -12 /StemV 120");
}
TEST(CoverageRenderFontRecovery, NonembeddedTrueTypeDescriptorsExerciseExplicitWeightAndAngle) {
  FontNames({"Arial", "CourierNew", "TimesNewRoman"}, "tt-substitute-", "/FontWeight 700 /ItalicAngle -10", true);
}
TEST(CoverageRenderFontRecovery, SymbolAndDingbatFontsKeepTheirNativeEncodingAndMappedGlyphPaths) {
  // PDF character 0x48 maps to Greek capital Eta in Symbol and the black
  // star in ZapfDingbats. GetGlyphPath accepts Unicode rather than PDF bytes.
  struct Case { const char* font; unsigned unicode; };
  for (const auto& item : {Case{"Symbol", 0x0397}, Case{"ZapfDingbats", 0x2605}}) {
    const auto bytes = Scene("BT /F 24 Tf 20 40 Td (H) Tj ET", "/Font << /F 5 0 R >>",
        {std::string("<< /Type /Font /Subtype /Type1 /BaseFont /") + item.font + " >>"});
    Workflow(bytes, std::string("symbol-face-") + item.font, VisibleText,
        [=](const Api& api, FPDF_DOCUMENT, FPDF_PAGE page) {
          Handle text(api.FPDFText_LoadPage(page), api.FPDFText_ClosePage); ASSERT_NE(text.get(), nullptr);
          EXPECT_EQ(text_fixture::Extract(api, text.get()), std::u16string(1, char16_t(item.unicode)));
          const auto object = api.FPDFPage_GetObject(page, 0); ASSERT_NE(object, nullptr);
          const auto f = api.FPDFTextObj_GetFont(object); ASSERT_NE(f, nullptr);
          const auto path = api.FPDFFont_GetGlyphPath(f, item.unicode, 24); ASSERT_NE(path, nullptr);
          EXPECT_GT(api.FPDFGlyphPath_CountGlyphSegments(path), 0);
        }, true);
  }
}

TEST(CoverageRenderFontRecovery, ExternalGraphicsFontEntrySetsResourceFontAndSizeInsideTextObjects) {
  for (const int size : {12, 24, 36}) {
    const auto fixture = Scene("BT /G gs 20 40 Td (H) Tj ET",
        "/Font << /F 5 0 R >> /ExtGState << /G << /Font [/F " + std::to_string(size) + "] >> >>",
        {"<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>"});
    Workflow(fixture, "ext-font-" + std::to_string(size),
        [=](const Raster& r) { VisibleTextAtLeast(r, size == 12 ? 8 : 12); }, Text(u"H", size), true);
  }
}
TEST(CoverageRenderFontRecovery, ExternalGraphicsFontOverridesEarlierTfAndIsRestoredByQ) {
  const auto fixture = Scene("q BT /F 12 Tf /G gs 20 40 Td (H) Tj ET Q",
      "/Font << /F 5 0 R /Other 6 0 R >> /ExtGState << /G << /Font [/Other 28] >> >>",
      {"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>", "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>"});
  Workflow(fixture, "ext-font-override", VisibleText, Text(u"H", 28), true);
}
TEST(CoverageRenderFontRecovery, TextWidthsAndHorizontalScaleRemainObservableThroughSubstitution) {
  for (const int scale : {50, 100, 150}) {
    const std::string content = "BT /F 24 Tf " + std::to_string(scale) + " Tz 20 40 Td (HH) Tj ET";
    Workflow(Standard("Courier", "/MissingWidth 600", content), "text-scale-" + std::to_string(scale),
        VisibleText, Text(u"HH", 24), true);
  }
}

void Path(const std::string& content, const std::string& resources,
          const std::vector<std::string>& extra, const std::string& name,
          const std::function<void(const Raster&)>& inspect, int delta = 0) {
  Workflow(Scene(content, resources, extra), name, [=](const Raster& r) { Guards(r); inspect(r); }, {}, false, delta);
}
TEST(CoverageRenderFontRecovery, ExternalDashArrayAndPhaseProduceKnownSolidAndGapIntervals) {
  for (const int phase : {0, 4}) {
    const auto resources = "/ExtGState << /G << /LW 4 /LC 0 /LJ 0 /ML 8 /D [[8 8] " + std::to_string(phase) + "] >> >>";
    Path("/G gs 1 0 0 RG 20 60 m 140 60 l S", resources, {}, "ext-dash-" + std::to_string(phase),
        [=](const Raster& r) { Pixel(r, phase ? 21 : 24, 60, {255, 0, 0}); Pixel(r, phase ? 26 : 32, 60, {255, 255, 255}); });
  }
}
TEST(CoverageRenderFontRecovery, ExternalLineCapJoinAndMiterSettingsSurviveAllFourSaveReaders) {
  for (const int cap : {0, 1, 2}) for (const int join : {0, 1, 2}) {
    Path("/G gs 0 0 1 RG 30 30 m 80 80 l 130 30 l S", "/ExtGState << /G << /LW 8 /LC " +
        std::to_string(cap) + " /LJ " + std::to_string(join) + " /ML 1.5 >> >>", {},
        "external-join-" + std::to_string(cap) + std::to_string(join),
        [](const Raster& r) { Pixel(r, 55, 55, {0, 0, 255}); Pixel(r, 105, 55, {0, 0, 255}); Pixel(r, 80, 20, {255, 255, 255}); });
  }
}
TEST(CoverageRenderFontRecovery, ShorthandVAndYBezierOperatorsPaintTheSpecifiedClosedRegion) {
  for (const auto& curve : {"20 90 140 90 v", "20 90 140 90 y"}) {
    Path(std::string("0 0 1 rg 20 30 m ") + curve + " 140 30 l h f", "", {}, std::string("bezier-") + curve,
        [](const Raster& r) { Pixel(r, 80, 40, {0, 0, 255}); Pixel(r, 80, 100, {255, 255, 255}); });
  }
}
TEST(CoverageRenderFontRecovery, DeviceDefaultRgbAliasIsUsedForImagesAndPaths) {
  Path("/DeviceRGB cs 1 0 0 sc 20 30 100 50 re f", "/ColorSpace << /DefaultRGB /DeviceRGB >>", {},
       "default-rgb", [](const Raster& r) { Pixel(r, 50, 50, {255, 0, 0}); });
}
TEST(CoverageRenderFontRecovery, DeviceDefaultGrayAliasUsesItsIndependentBlackAndWhiteEndpoints) {
  Path("/DeviceGray cs 0 sc 20 30 50 50 re f 1 sc 80 30 50 50 re f",
      "/ColorSpace << /DefaultGray /DeviceGray >>", {}, "default-gray", [](const Raster& r) {
        Pixel(r, 40, 50, {0, 0, 0}); Pixel(r, 100, 50, {255, 255, 255}); });
}
TEST(CoverageRenderFontRecovery, LegacyHalftoneTransferUndercolorAndBlackGenerationEntriesPreserveSolidColor) {
  const auto fn = "<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >>";
  Path("/G gs 1 0 0 rg 20 30 100 50 re f", std::string("/ExtGState << /G << /HT /Default /BG ") + fn +
      " /BG2 /Default /UCR " + fn + " /UCR2 /Default /FL 1 /SM 0.1 /SA true /AIS false /TK false /OP false /OPM 1 >> >>", {},
      "legacy-graphic-state", [](const Raster& r) { Pixel(r, 50, 50, {255, 0, 0}); });
}
TEST(CoverageRenderFontRecovery, OpacityClampingAndFillStrokeSeparationUseKnownInteriorSamples) {
  for (const auto& alpha : {"-1", "0", "1", "2"}) {
    const bool visible = std::string(alpha) == "1" || std::string(alpha) == "2";
    Path("/G gs 1 0 0 rg 0 0 1 RG 4 w 20 30 100 50 re B",
         std::string("/ExtGState << /G << /ca ") + alpha + " /CA 1 >> >>", {}, "clamped-opacity-" + std::string(alpha),
         [=](const Raster& r) { Pixel(r, 50, 50, visible ? RGB{255, 0, 0} : RGB{255, 255, 255}); Pixel(r, 20, 50, {0, 0, 255}); });
  }
}

std::string Packed(const std::vector<unsigned>& values, int bits, int copies = 2) {
  std::string row((values.size() * bits + 7) / 8, '\0'); size_t at = 0;
  for (auto value : values) for (int shift = bits - 1; shift >= 0; --shift, ++at)
    row[at / 8] = char(uint8_t(row[at / 8]) | (((value >> shift) & 1) << (7 - at % 8)));
  std::string result; for (int i = 0; i < copies; ++i) result += row; return result;
}
Bytes Image(const std::string& space, int bits, const std::string& data,
            const std::string& dictionary = "", const std::string& matrix = "80 0 0 40 20 40",
            const std::string& prefix = "") {
  return Scene(prefix + "q " + matrix + " cm /Im Do Q", "/XObject << /Im 5 0 R >>", {
      PdfStream("/Type /XObject /Subtype /Image /Width 8 /Height 2 /BitsPerComponent " +
                std::to_string(bits) + " /ColorSpace " + space + " " + dictionary, data)});
}
void GrayEndpoints(const Raster& r) { Guards(r); Pixel(r, 30, 60, {0, 0, 0}, 1); Pixel(r, 90, 60, {255, 255, 255}, 1); }
void CopyBitmap(int bits, bool indexed) {
  const unsigned maximum = indexed ? 1 : (1u << bits) - 1;
  const std::vector<unsigned> values{0, 0, 0, 0, maximum, maximum, maximum, maximum};
  const std::string space = indexed ? "[/Indexed /DeviceRGB 1 <000000ffffff>]" : "/DeviceGray";
  const auto input = Image(space, bits, Packed(values, bits)); Artifacts artifacts;
  std::array<Raster, 2> results; size_t backend = 0;
  const auto inspect = [=](const Api& api, FPDF_PAGE page) {
    ASSERT_EQ(api.FPDFPage_CountObjects(page), 1);
    const auto image = api.FPDFPage_GetObject(page, 0); ASSERT_NE(image, nullptr);
    EXPECT_EQ(api.FPDFPageObj_GetType(image), FPDF_PAGEOBJ_IMAGE);
    FPDF_IMAGEOBJ_METADATA metadata{}; ASSERT_TRUE(api.FPDFImageObj_GetImageMetadata(image, page, &metadata));
    EXPECT_EQ(metadata.width, 8u); EXPECT_EQ(metadata.height, 2u);
  };
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    Handle original(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(original.get(), nullptr); Handle loaded(api.FPDF_LoadPage(original.get(), 0), api.FPDF_ClosePage); ASSERT_NE(loaded.get(), nullptr);
    const auto source = api.FPDFPage_GetObject(loaded.get(), 0); ASSERT_NE(source, nullptr);
    Handle bitmap(api.FPDFImageObj_GetBitmap(source), api.FPDFBitmap_Destroy); ASSERT_NE(bitmap.get(), nullptr);
    ASSERT_EQ(api.FPDFBitmap_GetWidth(bitmap.get()), 8); ASSERT_EQ(api.FPDFBitmap_GetHeight(bitmap.get()), 2);
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument); ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDFPage_New(document.get(), 0, kWidth, kHeight), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
    Handle image(api.FPDFPageObj_NewImageObj(document.get()), api.FPDFPageObj_Destroy); ASSERT_NE(image.get(), nullptr);
    ASSERT_TRUE(api.FPDFImageObj_SetBitmap(nullptr, 0, image.get(), bitmap.get()));
    ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image.get(), 80, 0, 0, 40, 20, 40));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), image.release()));
    // Returned decoded bitmaps are normalized public surfaces. Re-embedding
    // exercises the actual palette/Gray/BGR conversion and serialization path.
    bitmap.reset(); loaded.reset(); original.reset();
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
     ASSERT_NO_FATAL_FAILURE(inspect(api, page.get()));
    results[backend++] = Render(api, page.get(), kWidth, kHeight, kCrisp); GrayEndpoints(results[backend - 1]);
    const auto saved = SaveDocument(api, document.get(), artifacts, "decoded-bitmap-copy"); ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api(); Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument); ASSERT_NE(reopened.get(), nullptr);
      Handle reloaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage); ASSERT_NE(reloaded.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(inspect(read, reloaded.get())); const auto after = Render(read, reloaded.get(), kWidth, kHeight, kCrisp); GrayEndpoints(after);
      CompareRaster(results[backend - 1], after, artifacts, writer->name() + "-" + reader->name());
    }
  }
  CompareRaster(results[0], results[1], artifacts, "decoded-copy-engines");
}
TEST(CoverageRenderFontRecovery, DecodedOneBitGrayCanBeReembeddedAfterItsSourceDocumentCloses) { CopyBitmap(1, false); }
TEST(CoverageRenderFontRecovery, DecodedTwoBitGrayCanBeReembeddedAfterItsSourceDocumentCloses) { CopyBitmap(2, false); }
TEST(CoverageRenderFontRecovery, DecodedFourBitGrayCanBeReembeddedAfterItsSourceDocumentCloses) { CopyBitmap(4, false); }
TEST(CoverageRenderFontRecovery, DecodedEightBitGrayCanBeReembeddedAfterItsSourceDocumentCloses) { CopyBitmap(8, false); }
TEST(CoverageRenderFontRecovery, DecodedOneBitPaletteCanBeReembeddedAfterItsSourceDocumentCloses) { CopyBitmap(1, true); }
TEST(CoverageRenderFontRecovery, DecodedTwoBitPaletteCanBeReembeddedAfterItsSourceDocumentCloses) { CopyBitmap(2, true); }
TEST(CoverageRenderFontRecovery, DecodedFourBitPaletteCanBeReembeddedAfterItsSourceDocumentCloses) { CopyBitmap(4, true); }
TEST(CoverageRenderFontRecovery, DecodedEightBitPaletteCanBeReembeddedAfterItsSourceDocumentCloses) { CopyBitmap(8, true); }

TEST(CoverageRenderFontRecovery, CalGrayImageTranslationKeepsBlackAndWhiteAcrossBitDepthsAndGamma) {
  for (const int bits : {1, 8, 16}) for (const auto& gamma : {"1", "2.2"}) {
    const unsigned maximum = (1u << bits) - 1;
    Workflow(Image(std::string("[/CalGray << /WhitePoint [0.9505 1 1.089] /Gamma ") + gamma + " >>]", bits,
        Packed({0, 0, 0, 0, maximum, maximum, maximum, maximum}, bits)), "calgray-image-" + std::to_string(bits) + gamma, GrayEndpoints, {}, false, 1);
  }
}
TEST(CoverageRenderFontRecovery, CalRgbImageTranslationKeepsNeutralEndpointsAcrossBitDepths) {
  // The diagonal matrix maps sample [1,1,1] to the declared XYZ white point.
  // Packed one-bit palette conversion and byte/word scanlines share endpoints.
  for (const int bits : {1, 8, 16}) {
    const unsigned maximum = (1u << bits) - 1; std::vector<unsigned> samples;
    for (int i = 0; i < 8; ++i) for (int c = 0; c < 3; ++c) samples.push_back(i < 4 ? 0 : maximum);
    Workflow(Image("[/CalRGB << /WhitePoint [0.9505 1 1.089] /Gamma [2.2 2.2 2.2] /Matrix [0.9505 0 0 0 1 0 0 0 1.089] >>]", bits, Packed(samples, bits)),
        "calrgb-image-" + std::to_string(bits), GrayEndpoints, {}, false, 1);
  }
}
TEST(CoverageRenderFontRecovery, IndexedImagesRotateVerticallyThroughAllBitmapDestinationFormats) {
  const auto data = Packed({0, 0, 0, 0, 1, 1, 1, 1}, 1);
  // Swapped axes run the indexed vertical bitmap-composer path. The left half
  // becomes a lower black band while the upper half remains palette white.
  for (const int format : {FPDFBitmap_Gray, FPDFBitmap_BGR, FPDFBitmap_BGRx, FPDFBitmap_BGRA}) {
    Workflow(Image("[/Indexed /DeviceRGB 1 <000000ffffff>]", 1, data, "", "0 80 -40 0 100 20"),
        "vertical-indexed-" + std::to_string(format), [](const Raster& r) { Guards(r); Pixel(r, 80, 30, {0, 0, 0}); Pixel(r, 80, 90, {255, 255, 255}); }, {}, false, 0, kCrisp, format);
  }
}
TEST(CoverageRenderFontRecovery, VerticalImageCompositingCombinesARealClipMaskAndConstantAlpha) {
  const auto data = Packed({0, 0, 0, 0, 1, 1, 1, 1}, 1);
  const auto bytes = Scene("20 20 m 140 20 l 80 110 l h W n /G gs q 0 80 -40 0 100 20 cm /Im Do Q",
      "/ExtGState << /G << /ca 0.5 >> >> /XObject << /Im 5 0 R >>", {
        PdfStream("/Type /XObject /Subtype /Image /Width 8 /Height 2 /BitsPerComponent 1 /ColorSpace [/Indexed /DeviceRGB 1 <ff00000000ff>]", data)});
  Workflow(bytes, "vertical-indexed-clip",
      [](const Raster& r) { Guards(r); Pixel(r, 80, 30, {255, 128, 128}, 1); Pixel(r, 80, 85, {128, 128, 255}, 1); Pixel(r, 65, 95, {255, 255, 255}); }, {}, false, 1);
}
TEST(CoverageRenderFontRecovery, ReversedImageDecodeKeepsItsDirectionAcrossReflectedMatrices) {
  const auto data = Packed({0, 0, 0, 0, 1, 1, 1, 1}, 1);
  Workflow(Image("/DeviceGray", 1, data, "/Decode [1 0]", "-80 0 0 40 100 40"),
      "reflected-image-decode", GrayEndpoints);
}
TEST(CoverageRenderFontRecovery, IndexedPaletteLookupCanBeStoredInAnIndirectStream) {
  const auto data = Packed({0, 0, 0, 0, 1, 1, 1, 1}, 1);
  const auto fixture = Scene("q 80 0 0 40 20 40 cm /Im Do Q", "/XObject << /Im 5 0 R >>", {
      PdfStream("/Type /XObject /Subtype /Image /Width 8 /Height 2 /BitsPerComponent 1 /ColorSpace [/Indexed /DeviceRGB 1 6 0 R]", data),
      PdfStream("", std::string("\0\0\0\xff\xff\xff", 6))});
  Workflow(fixture, "stream-palette", GrayEndpoints);
}

}  // namespace
}  // namespace pdfoxium::integration
