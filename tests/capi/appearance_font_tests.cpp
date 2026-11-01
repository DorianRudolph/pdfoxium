// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

#include "pinned_test_fonts.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

// AcroForm field trees and missing annotation appearances have no public
// constructor. Both fixtures reference nonembedded Helvetica through /Helv;
// the installed public provider must supply Arimo during generation AND reload.
enum class AppearanceKind { FreeText, Widget, FreeTextColor };
constexpr int kWidth = 240, kHeight = 100;
// At 4x the 18pt glyphs use the portable outline path in both engines, keeping
// the font-resolution regression independent of small bitmap-hinting deltas.
constexpr int kRasterWidth = kWidth * 4, kRasterHeight = kHeight * 4;

std::vector<uint8_t> Fixture(AppearanceKind kind) {
  const bool widget = kind == AppearanceKind::Widget;
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 5 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 100] /Resources <<>> /Annots [4 0 R] >>",
      widget
          ? "<< /Type /Annot /Subtype /Widget /P 3 0 R /Rect [12 20 228 80] /F 4 /FT /Tx /T (field) /V (Appearance font) /DA (/Helv 18 Tf 0 g) /BS << /W 0 >> >>"
          : "<< /Type /Annot /Subtype /FreeText /P 3 0 R /Rect [12 20 228 80] /F 4 /Contents (Appearance font) /DA (/Helv 18 Tf 0 g) /C [1 1 1] /BS << /W 0 >> >>",
      widget
          ? "<< /Fields [4 0 R] /NeedAppearances true /DA (/Helv 18 Tf 0 g) /DR << /Font << /Helv 6 0 R >> >> >>"
          : "<< /Fields [] /DA (/Helv 18 Tf 0 g) /DR << /Font << /Helv 6 0 R >> >> >>",
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"});
}

struct FormInfo : FPDF_FORMFILLINFO {
  FormInfo() : FPDF_FORMFILLINFO{} { version = 2; xfa_disabled = 1; }
};

struct FontPlacement {
  FS_MATRIX matrix{};
  float size = 0;
};

std::u16string Appearance(const Api& api, FPDF_ANNOTATION annotation) {
  const auto bytes = api.FPDFAnnot_GetAP(annotation, FPDF_ANNOT_APPEARANCEMODE_NORMAL,
                                        nullptr, 0);
  EXPECT_GE(bytes, 2u);
  EXPECT_EQ(bytes % 2, 0u);
  if (bytes < 2 || bytes % 2) return {};
  std::vector<FPDF_WCHAR> units(bytes / 2);
  EXPECT_EQ(api.FPDFAnnot_GetAP(annotation, FPDF_ANNOT_APPEARANCEMODE_NORMAL,
                              units.data(), bytes), bytes);
  EXPECT_EQ(units.back(), 0);
  return {units.begin(), units.end() - 1};
}

void InspectAppearance(const Api& api, FPDF_ANNOTATION annotation,
                       const std::vector<uint8_t>& expected_font,
                       std::vector<FontPlacement>* placements) {
  placements->clear();
  const int count = api.FPDFAnnot_GetObjectCount(annotation);
  ASSERT_GT(count, 0);
  for (int i = 0; i < count; ++i) {
    const auto object = api.FPDFAnnot_GetObject(annotation, i);
    ASSERT_NE(object, nullptr);
    if (api.FPDFPageObj_GetType(object) != FPDF_PAGEOBJ_TEXT) continue;
    const auto font = api.FPDFTextObj_GetFont(object);
    ASSERT_NE(font, nullptr);
    size_t size = 0;
    ASSERT_TRUE(api.FPDFFont_GetFontData(font, nullptr, 0, &size));
    ASSERT_EQ(size, expected_font.size())
        << "AP generation must use the configured provider, not bundled Helvetica";
    std::vector<uint8_t> actual(size);
    ASSERT_TRUE(api.FPDFFont_GetFontData(font, actual.data(), actual.size(), &size));
    EXPECT_EQ(actual, expected_font);
    EXPECT_EQ(api.FPDFFont_GetIsEmbedded(font), 0);
    FontPlacement placement;
    ASSERT_TRUE(api.FPDFPageObj_GetMatrix(object, &placement.matrix));
    ASSERT_TRUE(api.FPDFTextObj_GetFontSize(object, &placement.size));
    EXPECT_FLOAT_EQ(placement.size, 18);
    placements->push_back(placement);
  }
  ASSERT_FALSE(placements->empty());
  const auto appearance = Appearance(api, annotation);
  EXPECT_NE(appearance.find(u"Tf"), std::u16string::npos);
  EXPECT_NE(appearance.find(u"Tj"), std::u16string::npos);
}

void SamePlacement(const std::vector<FontPlacement>& expected,
                   const std::vector<FontPlacement>& actual) {
  ASSERT_EQ(actual.size(), expected.size());
  for (size_t i = 0; i < expected.size(); ++i) {
    EXPECT_FLOAT_EQ(actual[i].size, expected[i].size);
    const auto& a = expected[i].matrix;
    const auto& b = actual[i].matrix;
    for (const auto pair : {std::array<float, 2>{a.a, b.a}, {a.b, b.b},
                           {a.c, b.c}, {a.d, b.d}, {a.e, b.e}, {a.f, b.f}}) {
      EXPECT_NEAR(pair[0], pair[1], 0.0001f);
    }
  }
}

void ExpectTextInk(const Raster& image, bool blue) {
  ASSERT_EQ(image.width, kRasterWidth);
  ASSERT_EQ(image.height, kRasterHeight);
  size_t ink = 0;
  for (size_t at = 0; at < image.pixels.size(); at += 4) {
    if (blue) {
      ink += image.pixels[at] > 180 && image.pixels[at + 1] < 140 &&
             image.pixels[at + 2] < 100;
    } else {
      ink += image.pixels[at] < 128 && image.pixels[at + 1] < 128 &&
             image.pixels[at + 2] < 128;
    }
  }
  EXPECT_GT(ink, 100u) << "The generated appearance must visibly draw its text";
}

void RunAppearance(AppearanceKind kind) {
  ScopedPinnedTestFonts providers;
  const auto expected_font = text_fixture::Read(
      TestRoot() / "tests/capi/resources/test_fonts/Arimo-Regular.ttf");
  ASSERT_FALSE(expected_font.empty());
  const auto fixture = Fixture(kind);
  Artifacts artifacts;
  std::array<std::vector<FontPlacement>, 2> original_placements;
  std::array<Raster, 2> originals;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    const auto& api = writer->api();
    Handle document(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr),
                    api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    Handle annotation(api.FPDFPage_GetAnnot(page.get(), 0), api.FPDFPage_CloseAnnot);
    ASSERT_NE(annotation.get(), nullptr);
    const auto before = Render(api, page.get(), kRasterWidth, kRasterHeight, FPDF_ANNOT);
    // Core rendering generates a NeedAppearances widget AP without drawing
    // widgets. Do not initialize an SDK form environment: that would exercise
    // a different appearance generator and could prime the document font cache.
    if (kind != AppearanceKind::Widget) ExpectTextInk(before, false);
    std::vector<FontPlacement> initial;
    ASSERT_NO_FATAL_FAILURE(InspectAppearance(api, annotation.get(), expected_font, &initial));
    if (kind == AppearanceKind::FreeTextColor) {
      FormInfo info;
      Handle form(api.FPDFDOC_InitFormFillEnvironment(document.get(), &info),
                  api.FPDFDOC_ExitFormFillEnvironment);
      ASSERT_NE(form.get(), nullptr);
      ASSERT_TRUE(api.FPDFAnnot_SetFontColor(form.get(), annotation.get(), 32, 96, 224));
      originals[backend] = Render(api, page.get(), kRasterWidth, kRasterHeight, FPDF_ANNOT);
      ExpectTextInk(originals[backend], true);
      EXPECT_NE(before.pixels, originals[backend].pixels);
    } else {
      originals[backend] = before;
    }
    ASSERT_NO_FATAL_FAILURE(InspectAppearance(api, annotation.get(), expected_font,
                                             &original_placements[backend]));
    SamePlacement(initial, original_placements[backend]);
    const auto ap = Appearance(api, annotation.get());
    annotation.reset();
    page.reset();
    const auto saved = SaveDocument(api, document.get(), artifacts,
                                    writer->name() + "-configured-appearance-font");
    ASSERT_FALSE(saved.empty());
    if (kind == AppearanceKind::Widget) {
      Handle flatten(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(flatten.get(), nullptr);
      ASSERT_EQ(api.FPDFPage_Flatten(flatten.get(), FLAT_NORMALDISPLAY), FLATTEN_SUCCESS);
      // Flatten updates /Contents; reload to parse the replacement page content.
      flatten.reset();
      flatten = Handle(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(flatten.get(), nullptr);
      originals[backend] = Render(api, flatten.get(), kRasterWidth, kRasterHeight);
      ExpectTextInk(originals[backend], false);
    }
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr),
                      read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      Handle loaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(loaded.get(), nullptr);
      // Trigger core annotation generation before inspecting the AP.
      auto image = Render(read, loaded.get(), kRasterWidth, kRasterHeight, FPDF_ANNOT);
      Handle annot(read.FPDFPage_GetAnnot(loaded.get(), 0), read.FPDFPage_CloseAnnot);
      ASSERT_NE(annot.get(), nullptr);
      std::vector<FontPlacement> placements;
      ASSERT_NO_FATAL_FAILURE(InspectAppearance(read, annot.get(), expected_font, &placements));
      SamePlacement(original_placements[backend], placements);
      // Saving an existing authored AP must preserve its decoded contents;
      // no PDF serialization/object-number equality is required.
      EXPECT_EQ(Appearance(read, annot.get()), ap);
      annot.reset();
      if (kind == AppearanceKind::Widget) {
        ASSERT_EQ(read.FPDFPage_Flatten(loaded.get(), FLAT_NORMALDISPLAY), FLATTEN_SUCCESS);
        loaded.reset();
        loaded = Handle(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
        ASSERT_NE(loaded.get(), nullptr);
        image = Render(read, loaded.get(), kRasterWidth, kRasterHeight);
      }
      ExpectTextInk(image, kind == AppearanceKind::FreeTextColor);
      const auto label = writer->name() + "-" + reader->name() + "-configured-font";
      if (writer == reader) {
        CompareRaster(originals[backend], image, artifacts, label);
      } else {
        text_fixture::CompareTextRaster(originals[backend], image, artifacts, label);
      }
    }
    ++backend;
  }
  SamePlacement(original_placements[0], original_placements[1]);
  text_fixture::CompareTextRaster(originals[0], originals[1], artifacts,
                                  "configured-font-backends");
  providers.ExpectMapped();
}

TEST(AppearanceFonts, FreeTextGenerationUsesConfiguredFaceBeforeAndAfterReload) {
  RunAppearance(AppearanceKind::FreeText);
}

TEST(AppearanceFonts, NeedAppearancesWidgetUsesConfiguredFaceBeforeAndAfterReload) {
  RunAppearance(AppearanceKind::Widget);
}

TEST(AppearanceFonts, FreeTextColorRegenerationRetainsConfiguredFaceAndPlacement) {
  RunAppearance(AppearanceKind::FreeTextColor);
}

// Declining the font intentionally exercises the public callback and ordinary
// builtin fallback, without depending on this machine's installed fonts.
struct ReentrantFontInfo : FPDF_SYSFONTINFO {
  explicit ReentrantFontInfo(const Library& owner) : library(owner) {
    version = 2;
    Release = [](FPDF_SYSFONTINFO*) {};
    EnumFonts = [](FPDF_SYSFONTINFO* info, void* mapper) {
      auto& self = *static_cast<ReentrantFontInfo*>(info);
      self.library.api().FPDF_AddInstalledFont(mapper, "Arial", FXFONT_ANSI_CHARSET);
    };
    MapFont = [](FPDF_SYSFONTINFO* info, int, FPDF_BOOL, int, int,
                 const char*, FPDF_BOOL* exact) -> void* {
      auto& self = *static_cast<ReentrantFontInfo*>(info);
      if (exact) *exact = 0;
      if (self.annotation) {
        ++self.reads;
        const auto& api = self.library.api();
        FS_RECTF rect{};
        self.valid &= api.FPDFAnnot_GetFlags(self.annotation) == FPDF_ANNOT_FLAG_PRINT;
        self.valid &= !!api.FPDFAnnot_GetRect(self.annotation, &rect);
        self.valid &= rect.left == 12 && rect.bottom == 20 &&
                      rect.right == 228 && rect.top == 80;
        self.valid &= api.FPDFAnnot_GetSubtype(self.annotation) == FPDF_ANNOT_FREETEXT;
      }
      return nullptr;
    };
    GetFont = [](FPDF_SYSFONTINFO*, const char*) -> void* { return nullptr; };
    GetFontData = [](FPDF_SYSFONTINFO*, void*, unsigned int, unsigned char*,
                     unsigned long) -> unsigned long { return 0; };
    GetFaceName = [](FPDF_SYSFONTINFO*, void*, char*, unsigned long) -> unsigned long { return 0; };
    GetFontCharset = [](FPDF_SYSFONTINFO*, void*) -> int { return FXFONT_ANSI_CHARSET; };
    DeleteFont = [](FPDF_SYSFONTINFO*, void*) {};
    library.api().FPDF_SetSystemFontInfo(this);
  }
  ~ReentrantFontInfo() { library.restore_default_font_info(); }
  const Library& library;
  FPDF_ANNOTATION annotation = nullptr;
  size_t reads = 0;
  bool valid = true;
};

TEST(AppearanceFonts, InitialFreeTextGenerationAllowsFontCallbackToReadAnnotation) {
  const auto fixture = Fixture(AppearanceKind::FreeText);
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    ReentrantFontInfo provider(*library);
    Handle document(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr),
                    api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    Handle annotation(api.FPDFPage_GetAnnot(page.get(), 0), api.FPDFPage_CloseAnnot);
    ASSERT_NE(annotation.get(), nullptr);
    EXPECT_TRUE(Appearance(api, annotation.get()).empty());
    provider.annotation = annotation.get();
    const auto image = Render(api, page.get(), kRasterWidth, kRasterHeight, FPDF_ANNOT);
    provider.annotation = nullptr;
    EXPECT_GT(provider.reads, 0u);
    EXPECT_TRUE(provider.valid);
    ExpectTextInk(image, false);
    EXPECT_NE(Appearance(api, annotation.get()).find(u"Tj"), std::u16string::npos);
  }
}

}  // namespace
}  // namespace pdfoxium::integration
