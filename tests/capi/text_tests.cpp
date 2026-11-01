#include "pinned_test_fonts.h"
#include "text_fixture.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cctype>
#include <cmath>
#include <limits>
#include <string>
#include <tuple>
#include <vector>

namespace pdfoxium::integration {
namespace {
using namespace text_fixture;

TEST(Text, IndependentSnapshotsEmptyPagesAndReloadedEdits) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle other_document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr); ASSERT_NE(other_document.get(), nullptr);
    Handle page(api.FPDFPage_New(document.get(), 0, 400, 240), api.FPDF_ClosePage);
    Handle other(api.FPDFPage_New(other_document.get(), 0, 400, 240), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr); ASSERT_NE(other.get(), nullptr);
    {
      Handle empty(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
      ASSERT_NE(empty.get(), nullptr); EXPECT_EQ(api.FPDFText_CountChars(empty.get()), 0);
      auto word = Add(api, other_document.get(), other.get(), u"Independent", 20, 150);
      ASSERT_NE(word, nullptr);
      Handle live(api.FPDFText_LoadPage(other.get()), api.FPDFText_ClosePage);
      ASSERT_NE(live.get(), nullptr);
      EXPECT_EQ(Extract(api, live.get()), u"Independent");
      for (int repeat = 0; repeat < 3; ++repeat) {
        Handle a(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
        Handle b(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
        ASSERT_NE(a.get(), nullptr); ASSERT_NE(b.get(), nullptr);
        EXPECT_NE(a.get(), b.get());
        EXPECT_EQ(api.FPDFText_CountChars(a.get()), 0);
        EXPECT_EQ(Extract(api, live.get()), u"Independent");
      }
    }
    auto text = Add(api, document.get(), page.get(), u"Before", 20, 150);
    ASSERT_NE(text, nullptr);
    {
      Handle snapshot(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
      EXPECT_EQ(Extract(api, snapshot.get()), u"Before");
    }
    auto replacement = Wide(u"After edit");
    ASSERT_TRUE(api.FPDFText_SetText(text, replacement.data()));
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
    CrossReload(*library, document.get(), artifacts, "snapshot",
                [](const Api& read, FPDF_PAGE, FPDF_TEXTPAGE snapshot) {
                  EXPECT_EQ(Extract(read, snapshot), u"After edit");
                });
    EXPECT_EQ(api.FPDFText_LoadPage(nullptr), nullptr);
    api.FPDFText_ClosePage(nullptr);
  }
}

TEST(Text, CharacterOwnershipGeneratedSeparatorsAndCropIndependentExtraction) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<std::vector<unsigned int>, 2> observations;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, 400, 240), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    auto first = Add(api, document.get(), page.get(), u"Alpha", 20, 180);
    auto second = Add(api, document.get(), page.get(), u"Beta", 100, 180, 24, "Courier-Bold");
    auto third = Add(api, document.get(), page.get(), u"Gamma", 20, 130);
    auto outside = Add(api, document.get(), page.get(), u"Outside", 280, 40);
    ASSERT_NE(first, nullptr); ASSERT_NE(second, nullptr);
    ASSERT_NE(third, nullptr); ASSERT_NE(outside, nullptr);
    api.FPDFPage_SetCropBox(page.get(), 0, 70, 250, 240);
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
    ASSERT_NE(text.get(), nullptr);
    const auto extracted = Extract(api, text.get());
    EXPECT_NE(extracted.find(u"Alpha"), std::u16string::npos);
    EXPECT_NE(extracted.find(u"Beta"), std::u16string::npos);
    EXPECT_NE(extracted.find(u"Gamma"), std::u16string::npos);
    EXPECT_NE(extracted.find(u"Outside"), std::u16string::npos);
    bool generated = false, explicit_source = false, first_seen = false, second_seen = false;
    for (int i = 0; i < api.FPDFText_CountChars(text.get()); ++i) {
      const auto value = api.FPDFText_GetUnicode(text.get(), i);
      observations[backend].push_back(value);
      const auto object = api.FPDFText_GetTextObject(text.get(), i);
      const int is_generated = api.FPDFText_IsGenerated(text.get(), i);
      EXPECT_TRUE(is_generated == 0 || is_generated == 1);
      generated |= is_generated == 1;
      explicit_source |= is_generated == 0;
      if (!object) {
        int flags=91;
        EXPECT_DOUBLE_EQ(api.FPDFText_GetFontSize(text.get(),i),1);
        EXPECT_EQ(api.FPDFText_GetFontInfo(text.get(),i,nullptr,0,&flags),0u);
        EXPECT_EQ(flags,91);
        EXPECT_EQ(api.FPDFText_GetFontWeight(text.get(),i),-1);
      }
      if (object) {
        EXPECT_EQ(api.FPDFPageObj_GetType(object), FPDF_PAGEOBJ_TEXT);
        EXPECT_TRUE(object == first || object == second || object == third || object == outside);
        first_seen |= object == first; second_seen |= object == second;
        EXPECT_GT(api.FPDFText_GetFontSize(text.get(), i), 0);
      }
      EXPECT_EQ(api.FPDFText_HasUnicodeMapError(text.get(), i), 0);
    }
    EXPECT_TRUE(generated); EXPECT_TRUE(explicit_source);
    EXPECT_TRUE(first_seen); EXPECT_TRUE(second_seen);
    const int bounded=api.FPDFText_GetBoundedText(text.get(),0,240,250,70,nullptr,0);ASSERT_GT(bounded,0);
    std::vector<unsigned short> crop_text(static_cast<size_t>(bounded)+1);
    EXPECT_EQ(api.FPDFText_GetBoundedText(text.get(),0,240,250,70,crop_text.data(),bounded+1),bounded+1);
    const std::u16string cropped(crop_text.begin(),crop_text.end()-1);
    EXPECT_NE(cropped.find(u"Alpha"),std::u16string::npos);EXPECT_EQ(cropped.find(u"Outside"),std::u16string::npos);
    text.reset(); ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
    // Inspected Courier-Bold 't' in Beta: slightly thinner Rust stroke leaves
    // connected mass 2.19 (about 1.6% less glyph coverage). Limit this exception
    // to this scene's cross-engine comparisons; same-engine reload stays exact.
    CrossReload(*library, document.get(), artifacts, "layout",
                [](const Api& read, FPDF_PAGE, FPDF_TEXTPAGE snapshot) {
                  const auto value = Extract(read, snapshot);
                  EXPECT_NE(value.find(u"Outside"), std::u16string::npos);
                  EXPECT_NE(value.find(u"Alpha"), std::u16string::npos);
                }, FPDF_NO_INCREMENTAL, RasterTolerance::ConnectedError(2.5));
    ++backend;
  }
  EXPECT_EQ(observations[0], observations[1]);
}

TEST(Text, CharacterFontColorGeometryAndHitTestingUseKnownObjectState) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<std::vector<double>, 2> observations;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    const auto& api = library->api(); SCOPED_TRACE(library->name());
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, 400, 240), api.FPDF_ClosePage);
    auto object = Add(api, document.get(), page.get(), u"Ab c", 20, 100, 20);
    ASSERT_NE(object, nullptr);
    ASSERT_TRUE(api.FPDFPageObj_SetFillColor(object, 192, 64, 32, 127));
    ASSERT_TRUE(api.FPDFPageObj_SetStrokeColor(object, 16, 96, 208, 254));
    ASSERT_TRUE(api.FPDFTextObj_SetTextRenderMode(object, FPDF_TEXTRENDERMODE_FILL_STROKE));
    Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
    ASSERT_EQ(api.FPDFText_CountChars(text.get()), 4);
    EXPECT_EQ(Extract(api, text.get()), u"Ab c");
    for (int i = 0; i < 4; ++i) {
      EXPECT_EQ(api.FPDFText_GetUnicode(text.get(), i), static_cast<unsigned int>(u"Ab c"[i]));
      EXPECT_EQ(api.FPDFText_GetTextObject(text.get(), i), object);
      EXPECT_EQ(api.FPDFText_IsGenerated(text.get(), i), 0);
      EXPECT_EQ(api.FPDFText_IsHyphen(text.get(), i), 0);
      EXPECT_DOUBLE_EQ(api.FPDFText_GetFontSize(text.get(), i), 20);
      EXPECT_NEAR(api.FPDFText_GetCharAngle(text.get(), i), 0, 1e-6);
      double x = -1, y = -1;
      ASSERT_TRUE(api.FPDFText_GetCharOrigin(text.get(), i, &x, &y));
      EXPECT_NEAR(x, 20 + i * 12, 1e-4); EXPECT_NEAR(y, 100, 1e-4);
      FS_MATRIX matrix{};
      ASSERT_TRUE(api.FPDFText_GetMatrix(text.get(), i, &matrix));
      EXPECT_FLOAT_EQ(matrix.a, 1); EXPECT_FLOAT_EQ(matrix.b, 0);
      EXPECT_FLOAT_EQ(matrix.c, 0); EXPECT_FLOAT_EQ(matrix.d, 1);
      EXPECT_FLOAT_EQ(matrix.e, 20); EXPECT_FLOAT_EQ(matrix.f, 100);
      double left, right, bottom, top;
      ASSERT_TRUE(api.FPDFText_GetCharBox(text.get(), i, &left, &right, &bottom, &top));
      EXPECT_LE(left, right); EXPECT_LE(bottom, top);
      FS_RECTF loose{};
      ASSERT_TRUE(api.FPDFText_GetLooseCharBox(text.get(), i, &loose));
      EXPECT_LE(loose.left, loose.right); EXPECT_LE(loose.bottom, loose.top);
      observations[backend].insert(observations[backend].end(),{loose.left,loose.right,loose.bottom,loose.top});
      for (const auto tolerance : {std::array<double, 2>{0,0}, {1,0}, {0,1}, {1,1}}) {
        if (i != 2) {
          EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(text.get(), (left + right) / 2,
                                                  (bottom + top) / 2,
                                                  tolerance[0], tolerance[1]), i);
        }
      }
      std::array<unsigned int, 4> fill{}, stroke{};
      ASSERT_TRUE(api.FPDFText_GetFillColor(text.get(), i, &fill[0], &fill[1], &fill[2], &fill[3]));
      ASSERT_TRUE(api.FPDFText_GetStrokeColor(text.get(), i, &stroke[0], &stroke[1], &stroke[2], &stroke[3]));
      EXPECT_EQ(fill, (std::array<unsigned int, 4>{192,64,32,127}));
      EXPECT_EQ(stroke, (std::array<unsigned int, 4>{16,96,208,254}));
      observations[backend].insert(observations[backend].end(), {x,y,left,right,bottom,top});
      int flags = -1;
      const auto needed = api.FPDFText_GetFontInfo(text.get(), i, nullptr, 0, &flags);
      EXPECT_EQ(needed, 8u);  // Courier plus terminator.
      EXPECT_EQ(flags,api.FPDFFont_GetFlags(api.FPDFTextObj_GetFont(object)));
      for (size_t capacity : {needed - 1, needed, needed + 1}) {
        std::vector<char> name(capacity + 2, '~');
        EXPECT_EQ(api.FPDFText_GetFontInfo(text.get(), i, name.data(), capacity, nullptr), needed);
        if (capacity >= needed) { EXPECT_EQ(std::string(name.data()), "Courier"); }
        else { EXPECT_TRUE(std::all_of(name.begin(), name.end(), [](char c){return c == '~';})); }
        EXPECT_EQ(name[capacity], '~'); EXPECT_EQ(name[capacity + 1], '~');
      }
    }
    EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(text.get(), 10000, 10000, 1, 1), -1);
    EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(text.get(), 10000, 10000, -1, -1), -1);
    text.reset(); ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
    CrossReload(*library, document.get(), artifacts, "font-color-geometry",
                [](const Api& read, FPDF_PAGE, FPDF_TEXTPAGE snapshot) {
                  EXPECT_EQ(Extract(read, snapshot), u"Ab c");
                });
    ++backend;
  }
  ASSERT_EQ(observations[0].size(), observations[1].size());
  for (size_t i = 0; i < observations[0].size(); ++i)
    EXPECT_NEAR(observations[0][i], observations[1][i], 0.05) << i;
}

TEST(Text, MatrixAnglesAndOriginsFollowRotationSkewAndReflection) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  const std::array<FS_MATRIX, 6> transforms = {{{1,0,0,1,40,40}, {0,1,-1,0,100,40},
      {-1,0,0,-1,160,70}, {0,-1,1,0,220,100}, {1,.25f,.5f,1,260,60},
      {-1,0,0,1,350,80}}};
  std::array<Raster, 2> raster;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name()); const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, 400, 240), api.FPDF_ClosePage);
    std::array<FPDF_PAGEOBJECT, 6> objects{};
    for (size_t i = 0; i < transforms.size(); ++i) {
      objects[i] = Add(api, document.get(), page.get(), u"A", 0, 0, 20);
      ASSERT_NE(objects[i], nullptr);
      ASSERT_TRUE(api.FPDFPageObj_SetMatrix(objects[i], &transforms[i]));
    }
    Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
    int seen = 0;
    for (int index = 0; index < api.FPDFText_CountChars(text.get()); ++index) {
      if (api.FPDFText_IsGenerated(text.get(), index)) continue;
      const auto object = api.FPDFText_GetTextObject(text.get(), index);
      const auto found = std::find(objects.begin(), objects.end(), object);
      ASSERT_NE(found, objects.end()); const auto i = found - objects.begin(); ++seen;
      FS_MATRIX matrix{}; ASSERT_TRUE(api.FPDFText_GetMatrix(text.get(), index, &matrix));
      EXPECT_NEAR(matrix.a, transforms[i].a, 1e-5); EXPECT_NEAR(matrix.b, transforms[i].b, 1e-5);
      EXPECT_NEAR(matrix.c, transforms[i].c, 1e-5); EXPECT_NEAR(matrix.d, transforms[i].d, 1e-5);
      EXPECT_NEAR(matrix.e, transforms[i].e, 1e-5); EXPECT_NEAR(matrix.f, transforms[i].f, 1e-5);
      double x,y; ASSERT_TRUE(api.FPDFText_GetCharOrigin(text.get(), index, &x,&y));
      EXPECT_NEAR(x, transforms[i].e, 1e-4); EXPECT_NEAR(y, transforms[i].f, 1e-4);
      // Public angle follows the transformed x axis in PDFium's matrix
      // coordinate convention (c,a), yielding nonnegative radians.
      double expected = std::atan2(transforms[i].c, transforms[i].a);
      if (expected < 0) expected += 2 * 3.14159265358979323846;
      EXPECT_NEAR(api.FPDFText_GetCharAngle(text.get(), index), expected, 1e-4);
      EXPECT_DOUBLE_EQ(api.FPDFText_GetFontSize(text.get(),index),20);
      double left,right,bottom,top;ASSERT_TRUE(api.FPDFText_GetCharBox(text.get(),index,&left,&right,&bottom,&top));
      EXPECT_LE(left,right);EXPECT_LE(bottom,top);
      EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(text.get(),(left+right)/2,(bottom+top)/2,0,0),index);
    }
    EXPECT_EQ(seen, 6);
    raster[backend++] = Render(api, page.get(), 400, 240);
    text.reset(); ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
    CrossReload(*library, document.get(), artifacts, "matrices",
                [](const Api&, FPDF_PAGE, FPDF_TEXTPAGE) {});
  }
  CompareTextRaster(raster[0], raster[1], artifacts, "matrix-backends");
}

TEST(Text, GetTextRangesUseUnitsTerminatorsAndBoundedCanaries) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  constexpr char16_t kExpected[] = u"Ab c";
  for (const auto* library : Implementations()) {
    const auto& api = library->api(); SCOPED_TRACE(library->name());
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, 400, 240), api.FPDF_ClosePage);
    ASSERT_NE(Add(api, document.get(), page.get(), kExpected, 20, 100, 20), nullptr);
    Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
    for (auto range : {std::array<int,2>{0,0}, {0,1}, {0,4}, {3,1}, {1,4}, {0,99}}) {
      const int actual_count = std::min(range[1], 4 - range[0]);
      std::vector<unsigned short> output(static_cast<size_t>(range[1]) + 3, 0xaaaa);
      EXPECT_EQ(api.FPDFText_GetText(text.get(), range[0], range[1], output.data()), actual_count + 1);
      EXPECT_EQ(std::u16string(output.begin(), output.begin() + actual_count),
                std::u16string(kExpected + range[0], kExpected + range[0] + actual_count));
      EXPECT_EQ(output[actual_count], 0); EXPECT_EQ(output[range[1] + 1], 0xaaaa);
    }
    for (auto range : {std::array<int,2>{-1,1}, {0,-1}, {4,1}, {99,1}}) {
      std::array<unsigned short, 8> output; output.fill(0xaaaa);
      EXPECT_EQ(api.FPDFText_GetText(text.get(), range[0], range[1], output.data()), 0);
      EXPECT_TRUE(std::all_of(output.begin(), output.end(), [](auto c){return c == 0xaaaa;}));
    }
    EXPECT_EQ(api.FPDFText_GetText(text.get(), 0, 4, nullptr), 0);
    EXPECT_EQ(api.FPDFText_GetText(nullptr, 0, 4, nullptr), 0);
  }
}

std::vector<uint8_t> SupplementaryMathFixture(bool paired) {
  // The same mathematical glyph reaches the text page as either one scalar
  // (glyph-name fallback) or two units (ToUnicode). GetText's legacy UCS-2
  // contract skips the former and preserves the latter, even partial pairs.
    return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Resources << /Font << /F 6 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", "BT /F 12 Tf 20 50 Td (ABCD) Tj ET"),
      "null",
      std::string("<< /Type /Font /Subtype /Type1 /BaseFont /Courier ") +
        "/FirstChar 65 /LastChar 68 /Widths [600 600 600 600] "
        "/Encoding << /Type /Encoding /Differences [65 /A /u1D45E /B /C] >> " +
        (paired ? "/ToUnicode 7 0 R " : "") + ">>",
      PdfStream("", "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n"
        "/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n"
        "/CMapName /Math def /CMapType 2 def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n"
        "1 beginbfchar\n<42> <D835DC5E>\nendbfchar\nendcmap CMapName currentdict /CMap defineresource pop end end")});
}

TEST(Text, GetTextUcs2SkipsSupplementaryScalarsWithoutTruncatingTheTail) {
  for (bool paired : {false, true}) {
    const auto bytes = SupplementaryMathFixture(paired);
    const std::vector<unsigned> expected = paired
        ? std::vector<unsigned>{'A', 0xd835, 0xdc5e, 'B', 'C'}
        : std::vector<unsigned>{'A', 0x1d45e, 'B', 'C'};
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name());
      SCOPED_TRACE(paired);
      const auto& api = library->api();
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
      ASSERT_NE(text.get(), nullptr);
      ASSERT_EQ(api.FPDFText_CountChars(text.get()), expected.size());
      for (size_t index = 0; index < expected.size(); ++index)
        ASSERT_EQ(api.FPDFText_GetUnicode(text.get(), index), expected[index]);
      for (int start = 0; start < static_cast<int>(expected.size()); ++start) {
        for (int count = 0; count <= static_cast<int>(expected.size()) + 2; ++count) {
          SCOPED_TRACE(start);
          SCOPED_TRACE(count);
          std::vector<unsigned short> wanted;
          const int end = std::min(start + count, static_cast<int>(expected.size()));
          for (int index = start; index < end; ++index)
            if (expected[index] <= 0xffff) wanted.push_back(expected[index]);
          wanted.push_back(0);
          std::vector<unsigned short> output(count + 3, 0xaaaa);
          const int written = api.FPDFText_GetText(text.get(), start, count, output.data() + 1);
          ASSERT_EQ(written, wanted.size());
          EXPECT_TRUE(std::equal(wanted.begin(), wanted.end(), output.begin() + 1));
          EXPECT_EQ(output.front(), 0xaaaa);
          EXPECT_TRUE(std::all_of(output.begin() + 1 + written, output.end(), [](auto c) { return c == 0xaaaa; }));
        }
      }
      std::array<unsigned short, 16> full; full.fill(0xaaaa);
      const int written = api.FPDFText_GetBoundedText(text.get(), 0, 100, 200, 0, full.data(), full.size());
      ASSERT_EQ(written, 6);
      EXPECT_EQ(std::u16string(full.begin(), full.begin() + 5), u"A\U0001d45eBC");
      EXPECT_EQ(full[5], 0); EXPECT_EQ(full[6], 0xaaaa);
    }
  }
}

TEST(Text, MathematicalScalarAndPairedSearchPreservesSourceRangesAndStarts) {
  for (bool paired : {false, true}) {
    const auto bytes = SupplementaryMathFixture(paired);
    const int length = paired ? 2 : 1;
    const int characters = paired ? 5 : 4;
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name());
      SCOPED_TRACE(paired);
      const auto& api = library->api();
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
      ASSERT_NE(text.get(), nullptr);
      ASSERT_EQ(api.FPDFText_CountChars(text.get()), characters);
      const auto query = Wide(u"\U0001d45e");
      for (unsigned flags : {0u, 1u, 2u, 4u, 7u}) {
        for (int start = -1; start <= characters + 1; ++start) {
          for (bool backwards : {false, true}) {
            SCOPED_TRACE(flags);
            SCOPED_TRACE(start);
            SCOPED_TRACE(backwards);
            Handle search(api.FPDFText_FindStart(text.get(), query.data(), flags, start), api.FPDFText_FindClose);
            ASSERT_NE(search.get(), nullptr);
            // Native decodes the pattern to a scalar but does not normalize
            // paired text. Rust must find either representation with its exact
            // original span, including starts at/between the pair's units.
            const bool representable = !(paired && library->name() == "native");
            const bool expected = representable && (backwards ? start == -1 || start >= length : start >= 0 && start <= 1);
            EXPECT_EQ(backwards ? api.FPDFText_FindPrev(search.get()) : api.FPDFText_FindNext(search.get()), expected);
            if (expected) {
              EXPECT_EQ(api.FPDFText_GetSchResultIndex(search.get()), 1);
              EXPECT_EQ(api.FPDFText_GetSchCount(search.get()), length);
              EXPECT_FALSE(backwards ? api.FPDFText_FindPrev(search.get()) : api.FPDFText_FindNext(search.get()));
            }
          }
        }
      }
    }
  }
}

TEST(Text, BoundedTextAndCachedRectanglesHonorEachBufferContract) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name()); const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, 400, 240), api.FPDF_ClosePage);
    ASSERT_NE(Add(api, document.get(), page.get(), u"Ab c", 20, 100, 20), nullptr);
    Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
    double left=1,top=2,right=3,bottom=4;
    EXPECT_FALSE(api.FPDFText_GetRect(text.get(), 0, &left,&top,&right,&bottom));
    EXPECT_EQ(left, 0); EXPECT_EQ(top, 0); EXPECT_EQ(right, 0); EXPECT_EQ(bottom, 0);
    for (auto range : {std::array<int,2>{0,-1}, {0,1}, {1,2}, {0,99}}) {
      EXPECT_EQ(api.FPDFText_CountRects(text.get(), range[0], range[1]), 1);
      ASSERT_TRUE(api.FPDFText_GetRect(text.get(), 0, &left,&top,&right,&bottom));
      EXPECT_LE(left, right); EXPECT_LE(bottom, top);
      for (int index : {-1,1}) {
        left=1;top=2;right=3;bottom=4;
        EXPECT_FALSE(api.FPDFText_GetRect(text.get(), index, &left,&top,&right,&bottom));
        EXPECT_EQ(left,0); EXPECT_EQ(top,0); EXPECT_EQ(right,0); EXPECT_EQ(bottom,0);
      }
    }
    EXPECT_EQ(api.FPDFText_CountRects(text.get(), 0, 0), 0);
    EXPECT_EQ(api.FPDFText_CountRects(text.get(), -1, 2), -1);
    EXPECT_EQ(api.FPDFText_CountRects(nullptr, 0, 1), 0);
    const int required = api.FPDFText_GetBoundedText(text.get(), 0,240,400,0,nullptr,0);
    ASSERT_EQ(required, 4);
    for (int capacity : {0,1,required-1,required,required+1}) {
      std::vector<unsigned short> output(static_cast<size_t>(capacity) + 2, 0xaaaa);
      const int result = api.FPDFText_GetBoundedText(text.get(),0,240,400,0,output.data(),capacity);
      if (!capacity) { EXPECT_EQ(result, required); }
      else {
        EXPECT_EQ(result,std::min(capacity,required+1));
        const auto expected=Wide(u"Ab c");
        EXPECT_TRUE(std::equal(output.begin(),output.begin()+result,expected.begin()));
      }
      EXPECT_EQ(output[capacity],0xaaaa); EXPECT_EQ(output[capacity+1],0xaaaa);
    }
    std::array<unsigned short,8> output;output.fill(0xaaaa);
    EXPECT_EQ(api.FPDFText_GetBoundedText(text.get(),0,240,400,0,output.data(),-1),required);
    EXPECT_EQ(output[0],0xaaaa);
    EXPECT_EQ(api.FPDFText_GetBoundedText(text.get(),300,230,350,200,nullptr,0),0);
    EXPECT_EQ(api.FPDFText_GetBoundedText(nullptr,0,240,400,0,output.data(),8),0);
  }
}

TEST(Text, SelectionBoundsHitTolerancesAndCrossLineSearchUseCharacterGeometry) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<std::vector<int>,2> observations;size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
    ASSERT_NE(Add(api,document.get(),page.get(),u"Ag i",20,140,20),nullptr);
    ASSERT_NE(Add(api,document.get(),page.get(),u"two",20,100,20),nullptr);
    Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
    EXPECT_EQ(Extract(api,text.get()),u"Ag i\r\ntwo");
    double left,right,bottom,top;ASSERT_TRUE(api.FPDFText_GetCharBox(text.get(),0,&left,&right,&bottom,&top));
    const double x=(left+right)/2,y=(bottom+top)/2;
    EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(text.get(),x,y,0,0),0);
    EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(text.get(),left-.5,y,0,0),-1);
    EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(text.get(),left-.5,y,1,0),0);
    EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(text.get(),left-.5,y,0,1),-1);
    EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(text.get(),x,top+.5,0,1),0);
    EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(text.get(),x,top+.5,1,0),-1);
    EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(text.get(),x,y,-1,-1),0);
    FS_RECTF loose;ASSERT_TRUE(api.FPDFText_GetLooseCharBox(text.get(),0,&loose));
    EXPECT_LE(loose.left,left+.01);EXPECT_GE(loose.right,right-.01);
    EXPECT_LE(loose.bottom,bottom+.01);EXPECT_GE(loose.top,top-.01);
    for(int index:{1,3}) {
      double l,r,b,t;ASSERT_TRUE(api.FPDFText_GetCharBox(text.get(),index,&l,&r,&b,&t));
      FS_RECTF metrics;ASSERT_TRUE(api.FPDFText_GetLooseCharBox(text.get(),index,&metrics));
      EXPECT_LE(metrics.bottom,b+.01);EXPECT_GE(metrics.top,t-.01);
      EXPECT_GT(metrics.top-metrics.bottom,t-b);
    }
    for(const auto& bounds:{std::array<double,4>{left,top,right,bottom},
                            {x,top,right,bottom},{0,240,400,0},{0,120,400,80},
                            {300,240,320,200},{left,top,left,bottom},{right,bottom,left,top}}) {
      const int required=api.FPDFText_GetBoundedText(text.get(),bounds[0],bounds[1],bounds[2],bounds[3],nullptr,0);
      ASSERT_GE(required,0);observations[backend].push_back(required);
      std::vector<unsigned short> buffer(static_cast<size_t>(required)+2,0xaaaa);
      const auto written=api.FPDFText_GetBoundedText(text.get(),bounds[0],bounds[1],bounds[2],bounds[3],buffer.data(),required+1);
      EXPECT_EQ(written,required+1);EXPECT_EQ(buffer[required],0);EXPECT_EQ(buffer[required+1],0xaaaa);
      for(int index=0;index<required;++index)observations[backend].push_back(buffer[index]);
    }
    EXPECT_EQ(api.FPDFText_GetBoundedText(text.get(),left,top,right,bottom,nullptr,0),1);
    EXPECT_EQ(api.FPDFText_GetBoundedText(text.get(),0,120,400,80,nullptr,0),3);
    const auto pattern=Wide(u"i two");
    Handle search(api.FPDFText_FindStart(text.get(),pattern.data(),0,0),api.FPDFText_FindClose);
    ASSERT_NE(search.get(),nullptr);ASSERT_TRUE(api.FPDFText_FindNext(search.get()));
    EXPECT_EQ(api.FPDFText_GetSchResultIndex(search.get()),3);EXPECT_EQ(api.FPDFText_GetSchCount(search.get()),6);
    EXPECT_FALSE(api.FPDFText_FindNext(search.get()));
    EXPECT_EQ(api.FPDFText_CountRects(text.get(),0,-1),2);
    EXPECT_EQ(api.FPDFText_CountRects(text.get(),3,6),2);
    {
      Handle overlap_page(api.FPDFPage_New(document.get(),1,400,240),api.FPDF_ClosePage);
      ASSERT_NE(overlap_page.get(),nullptr);
      ASSERT_NE(Add(api,document.get(),overlap_page.get(),u"A",80,100,20),nullptr);
      ASSERT_NE(Add(api,document.get(),overlap_page.get(),u"B",80,100,20),nullptr);
      Handle overlap(api.FPDFText_LoadPage(overlap_page.get()),api.FPDFText_ClosePage);
      ASSERT_EQ(api.FPDFText_CountChars(overlap.get()),2);
      double l1,r1,b1,t1,l2,r2,b2,t2;
      ASSERT_TRUE(api.FPDFText_GetCharBox(overlap.get(),0,&l1,&r1,&b1,&t1));
      ASSERT_TRUE(api.FPDFText_GetCharBox(overlap.get(),1,&l2,&r2,&b2,&t2));
      ASSERT_LT(std::max(l1,l2),std::min(r1,r2));ASSERT_LT(std::max(b1,b2),std::min(t1,t2));
      EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(overlap.get(),(std::max(l1,l2)+std::min(r1,r2))/2,
                (std::max(b1,b2)+std::min(t1,t2))/2,0,0),0);
      // Another live page's extraction/search remains independent.
      EXPECT_EQ(Extract(api,text.get()),u"Ag i\r\ntwo");
    }
    search.reset();text.reset();ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));page.reset();
    CrossReload(*library,document.get(),artifacts,"cross-line-selection",[](const Api& read,FPDF_PAGE,FPDF_TEXTPAGE text) {
      EXPECT_EQ(Extract(read,text),u"Ag i\r\ntwo");
      EXPECT_EQ(read.FPDFText_CountRects(text,0,-1),2);
    });++backend;
  }
  EXPECT_EQ(observations[0],observations[1]);
}

std::vector<int> Matches(std::string source, std::string needle, unsigned long flags,
                          int start, bool backward, bool native_partition = false) {
  if (!(flags & FPDF_MATCHCASE)) {
    for (auto& c : source) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    for (auto& c : needle) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
  }
  std::vector<int> result;
  if (needle.empty()) return result;
  const auto matches_at = [&](int index) {
    const size_t at=static_cast<size_t>(index);
    if(source.compare(at,needle.size(),needle)!=0) return false;
    return !(flags & FPDF_MATCHWHOLEWORD) ||
      ((at==0 || !std::isalnum(static_cast<unsigned char>(source[at-1]))) &&
       (at+needle.size()==source.size() || !std::isalnum(static_cast<unsigned char>(source[at+needle.size()]))));
  };
  if(backward && !native_partition) {
    const int last=std::min(static_cast<int>(source.size())-static_cast<int>(needle.size()),
                            start<0 ? static_cast<int>(source.size()) : start-static_cast<int>(needle.size())+1);
    for(int index=last;index>=0;) {
      if(matches_at(index)) { result.push_back(index);index-=(flags&FPDF_CONSECUTIVE)?1:static_cast<int>(needle.size()); }
      else --index;
    }
  } else if(backward) {
    // The native reverse iterator enumerates the non-overlapping forward
    // match intervals, then visits them in reverse order. The consecutive
    // flag makes that interval set include overlaps in either direction.
    std::vector<int> intervals;
    for(int index=0;index+needle.size()<=source.size();) {
      if(matches_at(index)) { intervals.push_back(index);index+=(flags&FPDF_CONSECUTIVE)?1:static_cast<int>(needle.size()); }
      else ++index;
    }
    for(auto it=intervals.rbegin();it!=intervals.rend();++it)
      if(start<0 || *it+static_cast<int>(needle.size())-1<=start)result.push_back(*it);
  } else if(start>=0) {
    for(int index=start;index+needle.size()<=source.size();) {
      if(matches_at(index)) { result.push_back(index);index+=(flags&FPDF_CONSECUTIVE)?1:static_cast<int>(needle.size()); }
      else ++index;
    }
  }
  return result;
}

TEST(Text, SearchFlagsStartPositionsDirectionsAndIndependentOwners) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  constexpr char source[]="Banana banana! banana banana.";
  std::array<std::vector<int>,2> observations;size_t backend=0;
  for (const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
    ASSERT_NE(Add(api,document.get(),page.get(),u"Banana banana! banana banana.",20,100,12),nullptr);
    Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
    ASSERT_EQ(Extract(api,text.get()),u"Banana banana! banana banana.");
    const int count=api.FPDFText_CountChars(text.get());
    {
      Handle null_pattern(api.FPDFText_FindStart(text.get(),nullptr,0,0),api.FPDFText_FindClose);
      if(library->name()=="rust") { EXPECT_EQ(null_pattern.get(),nullptr); }
      // Native treats a null pattern as empty and returns an owned search;
      // advancing that native handle has the same empty-pattern UB below.
    }
    for(unsigned long flags=0;flags<8;++flags) for(const std::string pattern:{"banana","Banana","ana","absent",""}) {
      std::u16string pattern16(pattern.begin(),pattern.end());const auto units=Wide(pattern16);
      for(int start:{0,2,count-1,-1,count,count+100}) for(bool backward:{false,true}) {
        SCOPED_TRACE("flags="+std::to_string(flags)+" pattern="+pattern+" start="+std::to_string(start)+" backward="+std::to_string(backward));
        Handle search(api.FPDFText_FindStart(text.get(),units.data(),flags,start),api.FPDFText_FindClose);
        if(pattern.empty()) {
          // Empty-pattern advancement is unsafe in the pinned native finder:
          // Create ignores FindFirst(false), then FindNext dereferences back().
          // Construction/owned cleanup are covered without reproducing UB.
          if(library->name()=="rust") {
            EXPECT_FALSE(api.FPDFText_FindNext(search.get()));
            EXPECT_FALSE(api.FPDFText_FindPrev(search.get()));
          }
          continue;
        }
        ASSERT_NE(search.get(),nullptr);
        EXPECT_EQ(api.FPDFText_GetSchResultIndex(search.get()),0);
        EXPECT_EQ(api.FPDFText_GetSchCount(search.get()),0);
        const auto expected=Matches(source,pattern,flags,start,backward,library->name()=="native");
        std::vector<int> actual;
        for(size_t limit=0;limit<static_cast<size_t>(count)+1;++limit) {
          const auto found=backward?api.FPDFText_FindPrev(search.get()):api.FPDFText_FindNext(search.get());
          if(!found)break;
          actual.push_back(api.FPDFText_GetSchResultIndex(search.get()));
          EXPECT_EQ(api.FPDFText_GetSchCount(search.get()),static_cast<int>(pattern.size()));
        }
        EXPECT_EQ(actual,expected);
        EXPECT_FALSE(backward?api.FPDFText_FindPrev(search.get()):api.FPDFText_FindNext(search.get()));
        // Native FindPrev reuses a left-greedy forward partition for overlapping
        // matches; pdfoxium correctly starts at the rightmost backwards match.
        // All unambiguous result lists remain direct cross-engine comparisons.
        if(!(backward && pattern=="ana" && !(flags&FPDF_CONSECUTIVE)))
          observations[backend].insert(observations[backend].end(),actual.begin(),actual.end());
      }
    }
    const auto pattern=Wide(u"banana");
    Handle first(api.FPDFText_FindStart(text.get(),pattern.data(),0,0),api.FPDFText_FindClose);
    Handle second(api.FPDFText_FindStart(text.get(),pattern.data(),FPDF_MATCHCASE,-1),api.FPDFText_FindClose);
    ASSERT_TRUE(api.FPDFText_FindNext(first.get()));
    ASSERT_TRUE(api.FPDFText_FindPrev(second.get()));
    EXPECT_EQ(api.FPDFText_GetSchResultIndex(first.get()),0);
    EXPECT_EQ(api.FPDFText_GetSchResultIndex(second.get()),22);
    ASSERT_TRUE(api.FPDFText_FindNext(first.get()));
    EXPECT_EQ(api.FPDFText_GetSchResultIndex(first.get()),7);
    ASSERT_TRUE(api.FPDFText_FindPrev(first.get()));
    EXPECT_EQ(api.FPDFText_GetSchResultIndex(first.get()),0);
    second.reset();
    EXPECT_EQ(Extract(api,text.get()),u"Banana banana! banana banana.");
    api.FPDFText_FindClose(nullptr);++backend;
  }
  EXPECT_EQ(observations[0],observations[1]);
}

TEST(Text, SearchIndexMappingsHandleGeneratedAndActualTextExpansion) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // ActualText cannot be assigned by the public text constructors. The first
  // marked source glyph expands to three Unicode characters; another glyph is
  // separate in the content stream to force generated separators.
  const auto bytes=PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 240] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    PdfStream("", "BT /F1 20 Tf 20 160 Td /Span << /ActualText <FEFF006600660069> >> BDC (X) Tj EMC 80 0 Td (A) Tj ET")});
  Artifacts artifacts;artifacts.Pdf("actualtext",bytes);
  std::array<std::vector<int>,2> observations;size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);
    Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
    Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
    const auto extracted=Extract(api,text.get());
    EXPECT_NE(extracted.find(u"ffi"),std::u16string::npos);
    EXPECT_NE(extracted.find(u'A'),std::u16string::npos);
    const int count=api.FPDFText_CountChars(text.get());
    for(int index=0;index<static_cast<int>(extracted.size());++index) {
      const int character=api.FPDFText_GetCharIndexFromTextIndex(text.get(),index);
      EXPECT_GE(character,0);EXPECT_LT(character,count);
      observations[backend].push_back(character);
      if(character>=0 && character<count) {
        const int inverse=api.FPDFText_GetTextIndexFromCharIndex(text.get(),character);
        EXPECT_GE(inverse,0);EXPECT_LE(inverse,index);
      }
    }
    for(int index=0;index<count;++index) {
      const int unit=api.FPDFText_GetTextIndexFromCharIndex(text.get(),index);
      EXPECT_TRUE(unit==-1 || (unit>=0 && unit<static_cast<int>(extracted.size())));
      observations[backend].push_back(unit);
    }
    for(int index:{-1,count,count+1,100000}) EXPECT_EQ(api.FPDFText_GetTextIndexFromCharIndex(text.get(),index),-1);
    for(int index:{-1,static_cast<int>(extracted.size()),static_cast<int>(extracted.size()+1),100000})
      EXPECT_EQ(api.FPDFText_GetCharIndexFromTextIndex(text.get(),index),-1);
    EXPECT_EQ(api.FPDFText_GetTextIndexFromCharIndex(nullptr,0),-1);
    EXPECT_EQ(api.FPDFText_GetCharIndexFromTextIndex(nullptr,0),-1);
    const auto pattern=Wide(u"ffi");
    Handle search(api.FPDFText_FindStart(text.get(),pattern.data(),0,0),api.FPDFText_FindClose);
    ASSERT_NE(search.get(),nullptr);EXPECT_TRUE(api.FPDFText_FindNext(search.get()));
    EXPECT_EQ(api.FPDFText_GetSchResultIndex(search.get()),0);
    EXPECT_EQ(api.FPDFText_GetSchCount(search.get()),3);
    search.reset();text.reset();ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
    CrossReload(*library,document.get(),artifacts,"actualtext",[](const Api& read,FPDF_PAGE,FPDF_TEXTPAGE value){
      EXPECT_NE(Extract(read,value).find(u"ffi"),std::u16string::npos);
    });++backend;
  }
  EXPECT_EQ(observations[0],observations[1]);
}

TEST(Text, HyphenUnicodeMapAndNestedVerticalFixturesRetainSemanticsAndPixels) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  for(const std::string resource:{"bug_781804.pdf","bug_1388_2.pdf","text_form_multiple.pdf","vertical_text.pdf","actual_text_rtl.pdf","cropped_text.pdf"}) {
    const auto bytes=Resource(resource);ASSERT_FALSE(bytes.empty());
    artifacts.Pdf(resource,bytes);
    std::array<Raster,2> rasters;std::array<std::vector<unsigned int>,2> values;size_t backend=0;
    for(const auto* library:Implementations()) {
      const auto& api=library->api();SCOPED_TRACE(library->name()+"/"+resource);
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);
      Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
      ASSERT_GT(api.FPDFText_CountChars(text.get()),0);
      if(resource=="bug_781804.pdf") {
        EXPECT_EQ(api.FPDFText_GetUnicode(text.get(),0),'V');EXPECT_EQ(api.FPDFText_IsHyphen(text.get(),0),0);
        EXPECT_EQ(api.FPDFText_IsHyphen(text.get(),6),1);
        EXPECT_EQ(api.FPDFText_GetUnicode(text.get(),18),0x2010u);EXPECT_EQ(api.FPDFText_IsHyphen(text.get(),18),0);
      }
      if(resource=="bug_1388_2.pdf") {
        EXPECT_EQ(api.FPDFText_CountChars(text.get()),5);
        EXPECT_EQ(api.FPDFText_GetUnicode(text.get(),0),'X');
        EXPECT_EQ(api.FPDFText_HasUnicodeMapError(text.get(),0),0);
        EXPECT_EQ(api.FPDFText_HasUnicodeMapError(text.get(),2),1);
      }
      for(int index=0;index<api.FPDFText_CountChars(text.get());++index) {
        values[backend].push_back(api.FPDFText_GetUnicode(text.get(),index));
        values[backend].push_back(api.FPDFText_IsHyphen(text.get(),index));
        values[backend].push_back(api.FPDFText_HasUnicodeMapError(text.get(),index));
        const auto object=api.FPDFText_GetTextObject(text.get(),index);
        if(object) { EXPECT_EQ(api.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_TEXT); }
        double x,y;EXPECT_TRUE(api.FPDFText_GetCharOrigin(text.get(),index,&x,&y));
      }
      rasters[backend]=Render(api,page.get(),400,240);++backend;
    }
    EXPECT_EQ(values[0],values[1]);CompareTextRaster(rasters[0],rasters[1],artifacts,resource+"-backends");
  }
}

TEST(Text, InvalidIndexesAndNullHandlesPreserveAllRequiredOutputs) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
    ASSERT_NE(Add(api,document.get(),page.get(),u"A",20,100),nullptr);
    Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
    for(auto handle:{FPDF_TEXTPAGE{},text.get()}) for(int index:{-1,1,100000}) {
      EXPECT_EQ(api.FPDFText_GetUnicode(handle,index),0u);
      EXPECT_EQ(api.FPDFText_GetTextObject(handle,index),nullptr);
      EXPECT_EQ(api.FPDFText_IsGenerated(handle,index),-1);
      EXPECT_EQ(api.FPDFText_IsHyphen(handle,index),-1);
      EXPECT_EQ(api.FPDFText_HasUnicodeMapError(handle,index),-1);
      EXPECT_EQ(api.FPDFText_GetFontSize(handle,index),0);
      EXPECT_EQ(api.FPDFText_GetFontWeight(handle,index),-1);
      EXPECT_EQ(api.FPDFText_GetCharAngle(handle,index),-1);
      std::array<unsigned int,4> rgba{91,92,93,94};
      EXPECT_FALSE(api.FPDFText_GetFillColor(handle,index,&rgba[0],&rgba[1],&rgba[2],&rgba[3]));
      EXPECT_EQ(rgba,(std::array<unsigned int,4>{91,92,93,94}));
      EXPECT_FALSE(api.FPDFText_GetStrokeColor(handle,index,&rgba[0],&rgba[1],&rgba[2],&rgba[3]));
      EXPECT_EQ(rgba,(std::array<unsigned int,4>{91,92,93,94}));
      double l=91,r=92,b=93,t=94;
      EXPECT_FALSE(api.FPDFText_GetCharBox(handle,index,&l,&r,&b,&t));
      EXPECT_EQ(l,91);EXPECT_EQ(r,92);EXPECT_EQ(b,93);EXPECT_EQ(t,94);
      FS_RECTF rect{91,92,93,94};EXPECT_FALSE(api.FPDFText_GetLooseCharBox(handle,index,&rect));
      EXPECT_EQ(rect.left,91);EXPECT_EQ(rect.top,92);EXPECT_EQ(rect.right,93);EXPECT_EQ(rect.bottom,94);
      FS_MATRIX matrix{91,92,93,94,95,96};EXPECT_FALSE(api.FPDFText_GetMatrix(handle,index,&matrix));
      EXPECT_EQ(matrix.a,91);EXPECT_EQ(matrix.f,96);
      double x=91,y=92;EXPECT_FALSE(api.FPDFText_GetCharOrigin(handle,index,&x,&y));EXPECT_EQ(x,91);EXPECT_EQ(y,92);
      int flags=91;std::array<char,16> buffer;buffer.fill('~');
      EXPECT_EQ(api.FPDFText_GetFontInfo(handle,index,buffer.data(),buffer.size(),&flags),0u);
      EXPECT_EQ(flags,91);EXPECT_EQ(buffer[0],'~');
    }
    EXPECT_EQ(api.FPDFText_CountChars(nullptr),-1);
    EXPECT_EQ(api.FPDFText_GetCharIndexAtPos(nullptr,0,0,0,0),-3);
    EXPECT_FALSE(api.FPDFText_GetMatrix(text.get(),0,nullptr));
    EXPECT_EQ(api.FPDFText_FindStart(nullptr,Wide(u"A").data(),0,0),nullptr);
    EXPECT_FALSE(api.FPDFText_FindNext(nullptr));EXPECT_FALSE(api.FPDFText_FindPrev(nullptr));
    EXPECT_EQ(api.FPDFText_GetSchResultIndex(nullptr),0);EXPECT_EQ(api.FPDFText_GetSchCount(nullptr),0);
    double l=91,t=92,r=93,b=94;EXPECT_FALSE(api.FPDFText_GetRect(nullptr,0,&l,&t,&r,&b));
    EXPECT_EQ(l,91);EXPECT_EQ(t,92);EXPECT_EQ(r,93);EXPECT_EQ(b,94);
  }
}

std::u16string Url(const Api& api,FPDF_PAGELINK links,int index,int capacity) {
  std::vector<unsigned short> output(static_cast<size_t>(capacity)+2,0xaaaa);
  const int result=api.FPDFLink_GetURL(links,index,output.data(),capacity);
  EXPECT_GE(result,0);EXPECT_LE(result,capacity);
  EXPECT_EQ(output[capacity],0xaaaa);EXPECT_EQ(output[capacity+1],0xaaaa);
  if(result<=0)return {};
  const bool terminated=output[result-1]==0;
  return std::u16string(output.begin(),output.begin()+result-(terminated?1:0));
}

TEST(TextLinks, UrlsEmailsBufferSizesRangesAndRectangles) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<std::vector<std::u16string>,2> observations;std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,600,300),api.FPDF_ClosePage);
    ASSERT_NE(Add(api,document.get(),page.get(),u"https://example.org/a,",20,250,12),nullptr);
    ASSERT_NE(Add(api,document.get(),page.get(),u"http://example.net",20,210,12),nullptr);
    ASSERT_NE(Add(api,document.get(),page.get(),u"www.example.com",20,170,12),nullptr);
    ASSERT_NE(Add(api,document.get(),page.get(),u"test@example.org",20,130,12),nullptr);
    ASSERT_NE(Add(api,document.get(),page.get(),u"https://example.org/a",20,90,12),nullptr);
    Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
    Handle links(api.FPDFLink_LoadWebLinks(text.get()),api.FPDFLink_CloseWebLinks);
    ASSERT_NE(links.get(),nullptr);ASSERT_EQ(api.FPDFLink_CountWebLinks(links.get()),5);
    {
      Handle independent(api.FPDFLink_LoadWebLinks(text.get()),api.FPDFLink_CloseWebLinks);
      ASSERT_NE(independent.get(),nullptr);EXPECT_NE(independent.get(),links.get());
      EXPECT_EQ(api.FPDFLink_CountWebLinks(independent.get()),5);
    }
    EXPECT_EQ(api.FPDFLink_CountWebLinks(links.get()),5);
    const std::array<std::u16string,5> expected={u"https://example.org/a",u"http://example.net",u"http://www.example.com",u"mailto:test@example.org",u"https://example.org/a"};
    for(int index=0;index<5;++index) {
      SCOPED_TRACE(index);
      const int needed=api.FPDFLink_GetURL(links.get(),index,nullptr,0);
      ASSERT_EQ(needed,static_cast<int>(expected[index].size()+1));
      for(int capacity:{1,needed-1,needed,needed+1}) {
        const auto actual=Url(api,links.get(),index,capacity);
        EXPECT_EQ(actual,expected[index].substr(0,static_cast<size_t>(std::min(needed-1,capacity))));
      }
      observations[backend].push_back(Url(api,links.get(),index,needed));
      int start=-1,count=-1;ASSERT_TRUE(api.FPDFLink_GetTextRange(links.get(),index,&start,&count));
      EXPECT_GE(start,0);EXPECT_GT(count,0);EXPECT_LE(start+count,api.FPDFText_CountChars(text.get()));
      const int rects=api.FPDFLink_CountRects(links.get(),index);ASSERT_GT(rects,0);
      for(int rect=0;rect<rects;++rect) {
        double l,t,r,b;ASSERT_TRUE(api.FPDFLink_GetRect(links.get(),index,rect,&l,&t,&r,&b));
        EXPECT_LE(l,r);EXPECT_LE(b,t);
      }
      for(int invalid:{-1,rects}) {
        double l=91,t=92,r=93,b=94;
        EXPECT_FALSE(api.FPDFLink_GetRect(links.get(),index,invalid,&l,&t,&r,&b));
      }
    }
    for(int index:{-1,5,10000}) {
      EXPECT_EQ(Url(api,links.get(),index,4),u"");EXPECT_EQ(api.FPDFLink_CountRects(links.get(),index),0);
      int start=91,count=92;EXPECT_FALSE(api.FPDFLink_GetTextRange(links.get(),index,&start,&count));EXPECT_EQ(start,91);EXPECT_EQ(count,92);
      double l=91,t=92,r=93,b=94;EXPECT_FALSE(api.FPDFLink_GetRect(links.get(),index,0,&l,&t,&r,&b));
      EXPECT_EQ(l,91);EXPECT_EQ(t,92);EXPECT_EQ(r,93);EXPECT_EQ(b,94);
    }
    images[backend]=Render(api,page.get(),600,300);
    links.reset();
    for(int repeat=0;repeat<3;++repeat) {
      Handle again(api.FPDFLink_LoadWebLinks(text.get()),api.FPDFLink_CloseWebLinks);
      EXPECT_EQ(api.FPDFLink_CountWebLinks(again.get()),5);
    }
    ++backend;
  }
  EXPECT_EQ(observations[0],observations[1]);CompareTextRaster(images[0],images[1],artifacts,"web-links");
}

TEST(TextLinks, EmptyAndAnnotationOnlyPagesDoNotInventTextLinks) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
    Handle annotation(api.FPDFPage_CreateAnnot(page.get(),FPDF_ANNOT_LINK),api.FPDFPage_CloseAnnot);
    ASSERT_NE(annotation.get(),nullptr);ASSERT_TRUE(api.FPDFAnnot_SetURI(annotation.get(),"https://example.org/"));
    Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
    Handle links(api.FPDFLink_LoadWebLinks(text.get()),api.FPDFLink_CloseWebLinks);
    ASSERT_NE(links.get(),nullptr);EXPECT_EQ(api.FPDFLink_CountWebLinks(links.get()),0);
    EXPECT_EQ(api.FPDFLink_LoadWebLinks(nullptr),nullptr);EXPECT_EQ(api.FPDFLink_CountWebLinks(nullptr),0);
    EXPECT_EQ(api.FPDFLink_CountRects(nullptr,0),0);
    std::array<unsigned short,4> output;output.fill(0xaaaa);
    EXPECT_EQ(api.FPDFLink_GetURL(nullptr,0,output.data(),4),1);EXPECT_EQ(output[0],0);
    EXPECT_EQ(api.FPDFLink_GetURL(links.get(),0,output.data(),-1),1);
    int start=91,count=92;EXPECT_FALSE(api.FPDFLink_GetTextRange(nullptr,0,&start,&count));EXPECT_EQ(start,91);EXPECT_EQ(count,92);
    double l=91,t=92,r=93,b=94;EXPECT_FALSE(api.FPDFLink_GetRect(nullptr,0,0,&l,&t,&r,&b));EXPECT_EQ(l,91);EXPECT_EQ(t,92);EXPECT_EQ(r,93);EXPECT_EQ(b,94);
    api.FPDFLink_CloseWebLinks(nullptr);
  }
}

TEST(TextLinks, WrappedAndCrossObjectUrlsUseSourceRangesAndRenderAfterSave) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  // This pinned minimal fixture encodes explicit line splits and retained
  // hyphens that the public text builder cannot tag as source line wrapping.
  const auto bytes=Resource("weblinks_across_lines.pdf");ASSERT_FALSE(bytes.empty());
  const std::array<std::u16string,6> expected={u"http://example.com",u"http://example.com/",
      u"http://example.com/test-foo",u"http://abc.com/test-foo",u"http://example.com/",u"http://www.abc.com"};
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);
    CrossReload(*library,document.get(),artifacts,"wrapped-links",[&](const Api& read,FPDF_PAGE,FPDF_TEXTPAGE text) {
      Handle links(read.FPDFLink_LoadWebLinks(text),read.FPDFLink_CloseWebLinks);
      ASSERT_NE(links.get(),nullptr);ASSERT_EQ(read.FPDFLink_CountWebLinks(links.get()),6);
      bool multiple_rectangles=false;
      for(int index=0;index<6;++index) {
        const auto length=read.FPDFLink_GetURL(links.get(),index,nullptr,0);
        EXPECT_EQ(Url(read,links.get(),index,length),expected[index]);
        int start=-1,count=-1;ASSERT_TRUE(read.FPDFLink_GetTextRange(links.get(),index,&start,&count));
        EXPECT_GE(start,0);EXPECT_GT(count,0);EXPECT_LE(start+count,read.FPDFText_CountChars(text));
        const int rectangles=read.FPDFLink_CountRects(links.get(),index);ASSERT_GT(rectangles,0);
        multiple_rectangles|=rectangles>1;
        for(int rectangle=0;rectangle<rectangles;++rectangle) {
          double l,t,r,b;ASSERT_TRUE(read.FPDFLink_GetRect(links.get(),index,rectangle,&l,&t,&r,&b));
          EXPECT_LE(l,r);EXPECT_LE(b,t);
        }
      }
      EXPECT_TRUE(multiple_rectangles);
    });
    Handle split(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(split.get(),0,400,240),api.FPDF_ClosePage);
    const std::u16string prefix=u"https://example.";
    ASSERT_NE(Add(api,split.get(),page.get(),prefix,20,180,12),nullptr);
    ASSERT_NE(Add(api,split.get(),page.get(),u"org/path",20+static_cast<float>(prefix.size())*7.2f,180,12),nullptr);
    ASSERT_NE(Add(api,split.get(),page.get(),u"htp://invalid www. not@ valid",20,100,12),nullptr);
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
    CrossReload(*library,split.get(),artifacts,"split-link",[](const Api& read,FPDF_PAGE,FPDF_TEXTPAGE text) {
      Handle links(read.FPDFLink_LoadWebLinks(text),read.FPDFLink_CloseWebLinks);
      ASSERT_NE(links.get(),nullptr);ASSERT_EQ(read.FPDFLink_CountWebLinks(links.get()),1);
      const auto length=read.FPDFLink_GetURL(links.get(),0,nullptr,0);
      EXPECT_EQ(Url(read,links.get(),0,length),u"https://example.org/path");
      int start=-1,count=-1;ASSERT_TRUE(read.FPDFLink_GetTextRange(links.get(),0,&start,&count));
      EXPECT_EQ(start,0);EXPECT_EQ(count,static_cast<int>(std::u16string(u"https://example.org/path").size()));
    });
  }
}

TEST(Text, NestedFormUnicodeAndSearchOperateOnEffectiveCharacterTransforms) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  // Public APIs can import Form XObjects, but cannot assign their source
  // resource/content dictionaries. This fixture isolates nested text transforms.
  const auto bytes=PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 240] /Resources << /XObject << /Outer 4 0 R >> >> /Contents 7 0 R >>",
    PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 300 180] /Matrix [1 0 0 1 20 30] /Resources << /XObject << /Inner 5 0 R >> >>","q 1 0 0 1 10 20 cm /Inner Do Q"),
    PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 200 120] /Matrix [1 0 0 1 5 10] /Resources << /Font << /F1 6 0 R >> >>","BT /F1 20 Tf 1 0 0 1 10 20 Tm (Nested form text) Tj ET"),
    "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>",
    PdfStream("","q /Outer Do Q")});
  std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);
    Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
    Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
    EXPECT_EQ(Extract(api,text.get()),u"Nested form text");
    ASSERT_EQ(api.FPDFText_CountChars(text.get()),16);
    const auto borrowed=api.FPDFText_GetTextObject(text.get(),0);ASSERT_NE(borrowed,nullptr);
    EXPECT_EQ(api.FPDFPageObj_GetType(borrowed),FPDF_PAGEOBJ_TEXT);
    double x,y;ASSERT_TRUE(api.FPDFText_GetCharOrigin(text.get(),0,&x,&y));
    EXPECT_NEAR(x,45,.001);EXPECT_NEAR(y,80,.001);
    FS_MATRIX matrix{};ASSERT_TRUE(api.FPDFText_GetMatrix(text.get(),0,&matrix));
    EXPECT_FLOAT_EQ(matrix.a,1);EXPECT_FLOAT_EQ(matrix.d,1);
    EXPECT_NEAR(matrix.e,45,.001);EXPECT_NEAR(matrix.f,80,.001);
    const auto pattern=Wide(u"form text");
    Handle search(api.FPDFText_FindStart(text.get(),pattern.data(),FPDF_MATCHCASE,0),api.FPDFText_FindClose);
    ASSERT_TRUE(api.FPDFText_FindNext(search.get()));EXPECT_EQ(api.FPDFText_GetSchResultIndex(search.get()),7);
    EXPECT_EQ(api.FPDFText_GetSchCount(search.get()),9);
    images[backend++]=Render(api,page.get(),400,240);
    search.reset();text.reset();ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
    CrossReload(*library,document.get(),artifacts,"nested",[](const Api& read,FPDF_PAGE,FPDF_TEXTPAGE snapshot){
      EXPECT_EQ(Extract(read,snapshot),u"Nested form text");
    });
  }
  CompareTextRaster(images[0],images[1],artifacts,"nested-backends");
}

TEST(TextLinks, RotatedNestedFormLinksRetainSourceRangesAndUserSpaceRectangles) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // Public constructors cannot create a form XObject resource dictionary.
  const auto bytes=PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 240] /Resources << /XObject << /Nested 5 0 R >> >> /Contents 4 0 R >>",
    PdfStream("","q /Nested Do Q"),
    PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 300 200] /Matrix [0 1 -1 0 200 30] /Resources << /Font << /F1 6 0 R >> >>",
              "BT /F1 12 Tf 10 10 Td (https://nested.example/path) Tj ET"),
    "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>"});
  Artifacts artifacts;artifacts.Pdf("nested-url",bytes);std::array<Raster,2> images;size_t backend=0;
  const auto inspect=[](const Api& read,FPDF_PAGE,FPDF_TEXTPAGE text) {
    const std::u16string expected=u"https://nested.example/path";
    EXPECT_EQ(Extract(read,text),expected);
    Handle links(read.FPDFLink_LoadWebLinks(text),read.FPDFLink_CloseWebLinks);ASSERT_NE(links.get(),nullptr);
    ASSERT_EQ(read.FPDFLink_CountWebLinks(links.get()),1);const int length=read.FPDFLink_GetURL(links.get(),0,nullptr,0);
    EXPECT_EQ(Url(read,links.get(),0,length),expected);
    int start=-1,count=-1;ASSERT_TRUE(read.FPDFLink_GetTextRange(links.get(),0,&start,&count));
    EXPECT_EQ(start,0);EXPECT_EQ(count,static_cast<int>(expected.size()));
    const int rectangles=read.FPDFLink_CountRects(links.get(),0);ASSERT_GT(rectangles,0);
    for(int index=0;index<rectangles;++index) {
      double l,t,r,b;ASSERT_TRUE(read.FPDFLink_GetRect(links.get(),0,index,&l,&t,&r,&b));
      EXPECT_LT(l,r);EXPECT_LT(b,t);EXPECT_LT(r-l,t-b);
      EXPECT_GE(l,175);EXPECT_LE(r,205);EXPECT_GE(b,35);EXPECT_LE(t,240);
    }
    for(int index=0;index<read.FPDFText_CountChars(text);++index) {
      const auto object=read.FPDFText_GetTextObject(text,index);ASSERT_NE(object,nullptr);
      EXPECT_EQ(read.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_TEXT);
      FS_MATRIX matrix{};ASSERT_TRUE(read.FPDFText_GetMatrix(text,index,&matrix));
      EXPECT_NEAR(matrix.a,0,.0001);EXPECT_NEAR(matrix.b,1,.0001);
      EXPECT_NEAR(matrix.c,-1,.0001);EXPECT_NEAR(matrix.d,0,.0001);
    }
  };
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    {
      Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);inspect(api,page.get(),text.get());
    }
    images[backend++]=Render(api,page.get(),400,240);page.reset();
    CrossReload(*library,document.get(),artifacts,"nested-url",inspect);
  }
  CompareTextRaster(images[0],images[1],artifacts,"nested-url-backends");
}

}  // namespace
}  // namespace pdfoxium::integration
