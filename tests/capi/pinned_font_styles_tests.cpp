// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

#include "pinned_test_fonts.h"

namespace pdfoxium::integration {
namespace {

TEST(PinnedTestFonts, Base14FamiliesAndStylesUseTheExactRequestedFontFiles) {
  ScopedPinnedTestFonts fonts;
  struct Case { const char* requested; const char* file; };
  const Case cases[] = {
      {"Helvetica", "Arimo-Regular.ttf"},
      {"Helvetica-Bold", "Arimo-Bold.ttf"},
      {"Helvetica-Oblique", "Arimo-Italic.ttf"},
      {"Helvetica-BoldOblique", "Arimo-BoldItalic.ttf"},
      {"Times-Roman", "Tinos-Regular.ttf"},
      {"Times-Bold", "Tinos-Bold.ttf"},
      {"Times-Italic", "Tinos-Italic.ttf"},
      {"Times-BoldItalic", "Tinos-BoldItalic.ttf"},
      {"Courier", "Cousine-Regular.ttf"},
      {"Courier-Bold", "Cousine-Bold.ttf"},
      {"Courier-Oblique", "Cousine-Italic.ttf"},
      {"Courier-BoldOblique", "Cousine-BoldItalic.ttf"},
  };
  for (const auto& item : cases) {
    SCOPED_TRACE(item.requested);
    const auto expected = text_fixture::Read(
        TestRoot() / "tests/capi/resources/test_fonts" / item.file);
    ASSERT_FALSE(expected.empty());
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name());
      const auto& api = library->api();
      Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      Handle font(api.FPDFText_LoadStandardFont(document.get(), item.requested),
                  api.FPDFFont_Close);
      ASSERT_NE(font.get(), nullptr);
      size_t size = 0;
      ASSERT_TRUE(api.FPDFFont_GetFontData(font.get(), nullptr, 0, &size));
      ASSERT_EQ(size, expected.size())
          << "The requested face must not fall back to a regular or bundled font";
      std::vector<uint8_t> actual(size);
      ASSERT_TRUE(api.FPDFFont_GetFontData(font.get(), actual.data(), actual.size(), &size));
      EXPECT_EQ(size, expected.size());
      EXPECT_TRUE(actual == expected) << "Public font data must match " << item.file;
    }
  }
  fonts.ExpectMapped();
}

}  // namespace
}  // namespace pdfoxium::integration
