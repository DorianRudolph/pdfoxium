#include "text_fixture.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <functional>
#include <iomanip>
#include <sstream>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using namespace text_fixture;

std::string Hex(uint32_t value, int digits) {
  std::ostringstream out;
  out << std::hex << std::uppercase << std::setw(digits) << std::setfill('0') << value;
  return out.str();
}

uint16_t Be16(const std::vector<uint8_t>& data, size_t at) {
  EXPECT_LT(at + 1, data.size());
  return at + 1 < data.size() ? (uint16_t(data[at]) << 8) | data[at + 1] : 0;
}
uint32_t Be32(const std::vector<uint8_t>& data, size_t at) {
  return (uint32_t(Be16(data, at)) << 16) | Be16(data, at + 2);
}

uint16_t AhemGlyph(const std::vector<uint8_t>& bytes) {
  // Read the fixture's SFNT format-4 cmap to locate its known rectangular A.
  // Mapping every test CID to this glyph removes system font substitution
  // from the character-code/CID decoding comparisons.
  size_t cmap = 0;
  for (uint16_t i = 0; i < Be16(bytes, 4); ++i) {
    const size_t row = 12 + i * 16;
    if (Be32(bytes, row) == 0x636d6170) cmap = Be32(bytes, row + 8);
  }
  EXPECT_NE(cmap, 0u);
  if (!cmap) return 0;
  for (uint16_t i = 0; i < Be16(bytes, cmap + 2); ++i) {
    const size_t sub = cmap + Be32(bytes, cmap + 8 + i * 8);
    if (Be16(bytes, sub) != 4) continue;
    const auto count = Be16(bytes, sub + 6) / 2;
    const size_t ends = sub + 14, starts = ends + 2 * count + 2;
    const size_t deltas = starts + 2 * count, ranges = deltas + 2 * count;
    for (int j = 0; j < count; ++j) {
      if ('A' < Be16(bytes, starts + 2 * j) || 'A' > Be16(bytes, ends + 2 * j)) continue;
      const auto delta = Be16(bytes, deltas + 2 * j), range = Be16(bytes, ranges + 2 * j);
      if (!range) return static_cast<uint16_t>('A' + delta);
      const auto glyph = Be16(bytes, ranges + 2 * j + range + 2 * ('A' - Be16(bytes, starts + 2 * j)));
      return glyph ? static_cast<uint16_t>(glyph + delta) : 0;
    }
  }
  ADD_FAILURE() << "Ahem lacks an ASCII format-4 cmap";
  return 0;
}

std::string UnicodeCmap(const std::string& mappings, const std::string& spaces =
    "1 begincodespacerange <0000> <FFFF> endcodespacerange") {
  return "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n"
      "/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n"
      "/CMapName /CoverageUnicode def /CMapType 2 def\n" + spaces + "\n" + mappings +
      "\nendcmap CMapName currentdict /CMap defineresource pop end end";
}

std::vector<uint8_t> CidFixture(const std::string& encoding, const std::string& unicode,
                              const std::string& text_hex, bool stream_encoding,
                              const std::string& ordering = "Identity", bool vertical = false) {
  const auto font = Resource("fonts/ahem/Ahem.ttf");
  const uint16_t glyph = AhemGlyph(font);
  EXPECT_NE(glyph, 0);
  // All 65,536 CIDs use the same embedded outline, including predefined
  // encodings whose CID assignments differ across Adobe character sets.
  std::string gid_map(65536 * 2, '\0');
  for (size_t i = 0; i < gid_map.size(); i += 2) {
    gid_map[i] = static_cast<char>(glyph >> 8);
    gid_map[i + 1] = static_cast<char>(glyph);
  }
  const std::string font_bytes(font.begin(), font.end());
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 240] /Resources << /Font << /F 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type0 /BaseFont /Ahem /Encoding " +
      (stream_encoding ? std::string("11 0 R") : "/" + encoding) +
      " /DescendantFonts [6 0 R] /ToUnicode 7 0 R >>",
    PdfStream("", "BT /F 20 Tf 1 0 0 1 80 170 Tm <" + text_hex + "> Tj ET"),
    "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Ahem /CIDSystemInfo << /Registry (Adobe) /Ordering (" + ordering +
      ") /Supplement 0 >> /FontDescriptor 8 0 R /CIDToGIDMap 10 0 R /DW 1000" +
      (vertical ? std::string(" /DW2 [880 -1000]") : "") + " >>",
    PdfStream("", unicode),
    "<< /Type /FontDescriptor /FontName /Ahem /Flags 32 /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /CapHeight 800 /ItalicAngle 0 /StemV 80 /FontFile2 9 0 R >>",
    PdfStream("/Length1 " + std::to_string(font.size()), font_bytes),
    PdfStream("", gid_map),
    stream_encoding ? PdfStream("", encoding) : "null"});
}

size_t DarkPixels(const Raster& image) {
  size_t ink = 0;
  for (size_t i = 0; i < image.pixels.size(); i += 4)
    ink += std::min({image.pixels[i], image.pixels[i + 1], image.pixels[i + 2]}) < 128;
  return ink;
}

void CharacterObservations(const Api& api, FPDF_PAGE page, FPDF_TEXTPAGE text,
                           const std::u16string& expected) {
  ASSERT_EQ(Extract(api, text), expected);
  ASSERT_EQ(api.FPDFText_CountChars(text), static_cast<int>(expected.size()));
  const int rectangles = api.FPDFText_CountRects(text, 0, expected.size());
  EXPECT_GT(rectangles, 0);
  for (int i = 0; i < static_cast<int>(expected.size()); ++i) {
    EXPECT_EQ(api.FPDFText_GetUnicode(text, i), static_cast<unsigned int>(expected[i]));
    EXPECT_EQ(api.FPDFText_HasUnicodeMapError(text, i), 0);
    EXPECT_EQ(api.FPDFText_GetTextIndexFromCharIndex(text, i), i);
    EXPECT_EQ(api.FPDFText_GetCharIndexFromTextIndex(text, i), i);
    double left, right, bottom, top, x, y;
    ASSERT_TRUE(api.FPDFText_GetCharBox(text, i, &left, &right, &bottom, &top));
    ASSERT_TRUE(api.FPDFText_GetCharOrigin(text, i, &x, &y));
    EXPECT_LE(left, right); EXPECT_LE(bottom, top);
    EXPECT_TRUE(std::isfinite(x)); EXPECT_TRUE(std::isfinite(y));
    if (!api.FPDFText_IsGenerated(text, i)) {
      EXPECT_NE(api.FPDFText_GetTextObject(text, i), nullptr);
    }
  }
  EXPECT_GT(api.FPDFPage_CountObjects(page), 0);
}

void CompareFixture(const std::vector<uint8_t>& bytes, Artifacts& artifacts,
                    const std::string& label, const std::u16string& expected,
                    const std::function<void(const Api&, FPDF_PAGE, FPDF_TEXTPAGE)>& extra = {}) {
  std::array<Raster, 2> images;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name() + "/" + label);
    const auto& api = library->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
    ASSERT_NE(text.get(), nullptr);
    CharacterObservations(api, page.get(), text.get(), expected);
    if (extra) extra(api, page.get(), text.get());
    images[backend++] = Render(api, page.get(), 400, 240);
    EXPECT_GT(DarkPixels(images[backend - 1]), 20u);
    text.reset();
    // Save the unchanged document: preservation of source CMap/font resources
    // is distinct from GenerateContent's handling of edited page objects.
    page.reset();
    const auto saved = SaveDocument(api, document.get(), artifacts, label + "-" + library->name());
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      Handle reloaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(reloaded.get(), nullptr);
      Handle snapshot(read.FPDFText_LoadPage(reloaded.get()), read.FPDFText_ClosePage);
      ASSERT_NE(snapshot.get(), nullptr);
      CharacterObservations(read, reloaded.get(), snapshot.get(), expected);
      if (extra) extra(read, reloaded.get(), snapshot.get());
      const auto after = Render(read, reloaded.get(), 400, 240);
      if (library->name() == reader->name()) CompareRaster(images[backend - 1], after, artifacts, label + "-same-writer");
      else CompareTextRaster(images[backend - 1], after, artifacts, label + "-cross-reader");
    }
  }
  CompareTextRaster(images[0], images[1], artifacts, label + "-backends");
}

TEST(CoverageTextFonts, EmbeddedOneTwoAndFourByteCMapsDecodeRangesAndPersist) {
  Artifacts artifacts;
  struct EncodingCase { const char* name; const char* spaces; const char* cids; const char* unicode; const char* bytes; };
  const std::array<EncodingCase, 4> cases = {{
    {"one-byte", "1 begincodespacerange <00> <FF> endcodespacerange",
      "1 begincidrange <41> <46> 1 endcidrange", "1 beginbfrange <41> <46> <0041> endbfrange", "414243444546"},
    {"two-byte", "1 begincodespacerange <0000> <FFFF> endcodespacerange",
      "2 begincidchar <0101> 1 <0102> 2 endcidchar 1 begincidrange <0103> <0106> 3 endcidrange",
      "1 beginbfrange <0101> <0106> <0041> endbfrange", "010101020103010401050106"},
    // Leading zero bytes give distinct 3/4-byte code spaces while keeping
    // numeric source values inside PDFium's 16-bit ToUnicode parser limit.
    {"mixed-three-byte", "3 begincodespacerange <40> <7F> <8100> <81FF> <008200> <0082FF> endcodespacerange",
      "4 begincidchar <41> 1 <8140> 2 <42> 5 <8141> 6 endcidchar 1 begincidrange <008243> <008244> 3 endcidrange",
      "4 beginbfchar <41> <0041> <8140> <0042> <42> <0045> <8141> <0046> endbfchar 1 beginbfrange <008243> <008244> <0043> endbfrange",
      "418140008243008244428141"},
    {"mixed-four-byte", "3 begincodespacerange <40> <7F> <8100> <81FF> <00009000> <000090FF> endcodespacerange",
      "4 begincidchar <41> 1 <8140> 2 <42> 5 <8141> 6 endcidchar 1 begincidrange <00009043> <00009044> 3 endcidrange",
      "4 beginbfchar <41> <0041> <8140> <0042> <42> <0045> <8141> <0046> endbfchar 1 beginbfrange <00009043> <00009044> [<0043> <0044>] endbfrange",
      "4181400000904300009044428141"}
  }};
  for (const auto& item : cases) {
    SCOPED_TRACE(item.name);
    const std::string encoding = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n"
      "/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> def\n"
      "/CMapName /CoverageEncoding def /CMapType 1 def /WMode 0 def\n" +
      std::string(item.spaces) + "\n" + item.cids + "\nendcmap CMapName currentdict /CMap defineresource pop end end";
    CompareFixture(CidFixture(encoding, UnicodeCmap(item.unicode, item.spaces), item.bytes, true),
      artifacts, item.name, u"ABCDEF", [](const Api& api, FPDF_PAGE, FPDF_TEXTPAGE text) {
        for (int i = 0; i < 6; ++i) {
          double x, y;
          ASSERT_TRUE(api.FPDFText_GetCharOrigin(text, i, &x, &y));
          EXPECT_NEAR(x, 80 + i * 20, .001); EXPECT_NEAR(y, 170, .001);
        }
        const auto query = Wide(u"CDE");
        Handle search(api.FPDFText_FindStart(text, query.data(), FPDF_MATCHCASE, 0), api.FPDFText_FindClose);
        ASSERT_NE(search.get(), nullptr); ASSERT_TRUE(api.FPDFText_FindNext(search.get()));
        EXPECT_EQ(api.FPDFText_GetSchResultIndex(search.get()), 2); EXPECT_EQ(api.FPDFText_GetSchCount(search.get()), 3);
      });
  }
}

TEST(CoverageTextFonts, SharedLeadingZeroCMapPrefixesDoNotConsumeTheFollowingCharacter) {
  Artifacts artifacts;
  const std::string spaces = "4 begincodespacerange <40> <7F> <8100> <81FF> <008200> <0082FF> <00009000> <000090FF> endcodespacerange";
  const auto bytes = CidFixture("/WMode 0 def " + spaces +
    " 2 begincidchar <41> 1 <8140> 2 endcidchar 1 begincidrange <008243> <008244> 3 endcidrange "
    "1 begincidrange <00009045> <00009046> 5 endcidrange", UnicodeCmap(
    "2 beginbfchar <41> <0041> <8140> <0042> endbfchar "
    "2 beginbfrange <008243> <008244> <0043> <00009045> <00009046> [<0045> <0046>] endbfrange", spaces),
    "4181400082430082440000904500009046", true);
  // The malformed native decoding also creates an extra painted CID0, even
  // though that character is suppressed from extraction. Build independent
  // ordinary one-byte controls for six correct and seven native glyphs rather
  // than treating the native rendering defect as an AA difference.
  std::array<std::array<Raster,2>,2> controls;
  for (int extra = 0; extra < 2; ++extra) {
    const auto control_bytes = CidFixture(
      "1 begincodespacerange <00> <FF> endcodespacerange 1 begincidrange <41> <47> 1 endcidrange",
      UnicodeCmap("1 beginbfrange <41> <47> <0041> endbfrange",
        "1 begincodespacerange <00> <FF> endcodespacerange"),
      extra ? "41424344454647" : "414243444546", true);
    size_t engine = 0;
    for (const auto* library : Implementations()) {
      const auto& api = library->api();
      Handle document(api.FPDF_LoadMemDocument64(control_bytes.data(), control_bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
      ASSERT_NE(text.get(), nullptr);
      CharacterObservations(api, page.get(), text.get(), extra ? u"ABCDEFG" : u"ABCDEF");
      controls[extra][engine++] = Render(api, page.get(), 400, 240);
      EXPECT_EQ(DarkPixels(controls[extra][engine - 1]), extra ? 2800u : 2400u);
    }
    CompareTextRaster(controls[extra][0], controls[extra][1], artifacts,
      "shared-cmap-prefix-valid-control-" + std::to_string(extra));
  }
  const auto inspect = [](const Library& library, FPDF_PAGE page) {
    const auto& api = library.api();
    Handle text(api.FPDFText_LoadPage(page), api.FPDFText_ClosePage);
    ASSERT_NE(text.get(), nullptr);
    if (library.name() == "rust") {
      CharacterObservations(api, page, text.get(), u"ABCDEF");
      const auto query = Wide(u"CDE");
      Handle search(api.FPDFText_FindStart(text.get(), query.data(), FPDF_MATCHCASE, 0), api.FPDFText_FindClose);
      ASSERT_TRUE(api.FPDFText_FindNext(search.get()));
      EXPECT_EQ(api.FPDFText_GetSchResultIndex(search.get()), 2);
      EXPECT_EQ(api.FPDFText_GetSchCount(search.get()), 3);
    } else {
      // Pinned CheckFourByteCodeRange accepts a partial prefix mismatch:
      // 00 82 43 00 is consumed as four bytes instead of the valid 00 82 43.
      // Its missing ToUnicode fallback exposes the low UTF-16 word4300.
      EXPECT_EQ(Extract(api, text.get()), u"AB\u4300DEF");
    }
  };
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr); inspect(*writer, page.get());
    const bool native_writer = writer->name() == "native";
    CompareRaster(controls[native_writer ? 1 : 0][native_writer ? 0 : 1],
      Render(api, page.get(), 400, 240), artifacts, writer->name() + "-shared-prefix-control");
    page.reset();
    const auto saved = SaveDocument(api, document.get(), artifacts, writer->name() + "-shared-cmap-prefix");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      Handle reloaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(reloaded.get(), nullptr); inspect(*reader, reloaded.get());
      const bool native_reader = reader->name() == "native";
      CompareRaster(controls[native_reader ? 1 : 0][native_reader ? 0 : 1],
        Render(read, reloaded.get(), 400, 240), artifacts,
        writer->name() + "-to-" + reader->name() + "-shared-cmap-prefix");
    }
  }
}

TEST(CoverageTextFonts, PredefinedAdobeCMapsSpanEveryCharacterCollectionAndWritingMode) {
  Artifacts artifacts;
  struct NamedMap { const char* name; const char* ordering; const char* bytes; const char* mappings; };
  // Embedded outlines and explicit ToUnicode keep these independent of the
  // host's CJK font installation. Each source includes a two-byte code and,
  // for mixed encodings, ASCII bytes on both sides of that code.
  const std::array<NamedMap, 18> maps = {{
    {"GB-EUC", "GB1", "41A1A142", "3 beginbfchar <41> <0041> <A1A1> <0042> <42> <0043> endbfchar"},
    {"GBpc-EUC", "GB1", "41A1A142", "3 beginbfchar <41> <0041> <A1A1> <0042> <42> <0043> endbfchar"},
    {"GBK-EUC", "GB1", "41814042", "3 beginbfchar <41> <0041> <8140> <0042> <42> <0043> endbfchar"},
    {"GBKp-EUC", "GB1", "41814042", "3 beginbfchar <41> <0041> <8140> <0042> <42> <0043> endbfchar"},
    {"GBK2K", "GB1", "41814042", "3 beginbfchar <41> <0041> <8140> <0042> <42> <0043> endbfchar"},
    {"UniGB-UCS2", "GB1", "00414E2D0042", "3 beginbfchar <0041> <0041> <4E2D> <0042> <0042> <0043> endbfchar"},
    {"UniGB-UTF16", "GB1", "00414E2D0042", "3 beginbfchar <0041> <0041> <4E2D> <0042> <0042> <0043> endbfchar"},
    {"B5pc", "CNS1", "41A14042", "3 beginbfchar <41> <0041> <A140> <0042> <42> <0043> endbfchar"},
    {"ETen-B5", "CNS1", "41A14042", "3 beginbfchar <41> <0041> <A140> <0042> <42> <0043> endbfchar"},
    {"HKscs-B5", "CNS1", "41A14042", "3 beginbfchar <41> <0041> <A140> <0042> <42> <0043> endbfchar"},
    {"UniCNS-UCS2", "CNS1", "00414E2D0042", "3 beginbfchar <0041> <0041> <4E2D> <0042> <0042> <0043> endbfchar"},
    {"90ms-RKSJ", "Japan1", "41814042", "3 beginbfchar <41> <0041> <8140> <0042> <42> <0043> endbfchar"},
    {"EUC", "Japan1", "41A1A142", "3 beginbfchar <41> <0041> <A1A1> <0042> <42> <0043> endbfchar"},
    {"UniJIS-UCS2", "Japan1", "004130420042", "3 beginbfchar <0041> <0041> <3042> <0042> <0042> <0043> endbfchar"},
    {"KSC-EUC", "Korea1", "41A1A142", "3 beginbfchar <41> <0041> <A1A1> <0042> <42> <0043> endbfchar"},
    {"KSCms-UHC", "Korea1", "41814142", "3 beginbfchar <41> <0041> <8141> <0042> <42> <0043> endbfchar"},
    {"KSCpc-EUC", "Korea1", "41A1A142", "3 beginbfchar <41> <0041> <A1A1> <0042> <42> <0043> endbfchar"},
    {"UniKS-UCS2", "Korea1", "0041AC000042", "3 beginbfchar <0041> <0041> <AC00> <0042> <0042> <0043> endbfchar"}
  }};
  for (const auto& map : maps) for (bool vertical : {false, true}) {
    const auto name = std::string(map.name) + (vertical ? "-V" : "-H");
    CompareFixture(CidFixture(name, UnicodeCmap(map.mappings), map.bytes, false, map.ordering, vertical),
      artifacts, name, u"ABC", [vertical](const Api& api, FPDF_PAGE, FPDF_TEXTPAGE text) {
        std::array<double,3> x{},y{};
        for (int i = 0; i < 3; ++i) ASSERT_TRUE(api.FPDFText_GetCharOrigin(text, i, &x[i], &y[i]));
        if (vertical) {
          EXPECT_NEAR(x[0], x[1], .001); EXPECT_NEAR(x[1], x[2], .001);
          EXPECT_NEAR(y[0] - y[1], 20, .001); EXPECT_NEAR(y[1] - y[2], 20, .001);
        } else {
          EXPECT_NEAR(x[1] - x[0], 20, .001); EXPECT_NEAR(x[2] - x[1], 20, .001);
          EXPECT_NEAR(y[0], y[2], .001);
        }
      });
  }
}

TEST(CoverageTextFonts, ToUnicodeAcceptsFullWidthType0SourceCodesAndPreservesEditedMappings) {
  Artifacts artifacts;
  // PDF32000-1 sections9.7.6.2 and9.10.3 permit up to four-byte source
  // character codes and require ToUnicode to use the font's code spaces.
  // These source values exceed16bits, while their descendant CIDs remain1..6.
  const std::string spaces = "4 begincodespacerange <00> <7F> <8100> <81FF> <820000> <82FFFF> <90000000> <90FFFFFF> endcodespacerange";
  const auto bytes = CidFixture("/WMode 0 def " + spaces +
    " 2 begincidchar <41> 1 <8140> 2 endcidchar "
    "2 begincidrange <824100> <824101> 3 <90410000> <90410001> 5 endcidrange", UnicodeCmap(
    "2 beginbfchar <41> <0041> <8140> <0042> endbfchar "
    "1 beginbfchar <824100> <0043> endbfchar "
    "2 beginbfrange <824101> <824101> <0044> <90410000> <90410001> [<0045> <0046>] endbfrange", spaces),
    "4181408241008241019041000090410001", true);
  const auto inspect = [](const Library& library, FPDF_PAGE page, bool edited) {
    const auto& api = library.api();
    Handle text(api.FPDFText_LoadPage(page), api.FPDFText_ClosePage);
    ASSERT_NE(text.get(), nullptr);
    if (library.name() == "rust") {
      CharacterObservations(api, page, text.get(), edited ? u"CDEFAB" : u"ABCDEF");
      const auto query = Wide(edited ? u"DEF" : u"CDE");
      Handle search(api.FPDFText_FindStart(text.get(), query.data(), FPDF_MATCHCASE, 0), api.FPDFText_FindClose);
      ASSERT_TRUE(api.FPDFText_FindNext(search.get()));
      EXPECT_EQ(api.FPDFText_GetSchResultIndex(search.get()), edited ? 1 : 2);
      EXPECT_EQ(api.FPDFText_GetSchCount(search.get()), 3);
      for (int i = 0; i < 6; ++i) {
        double x, y; ASSERT_TRUE(api.FPDFText_GetCharOrigin(text.get(), i, &x, &y));
        EXPECT_NEAR(x, 80 + i * 20, .001); EXPECT_NEAR(y, 170, .001);
      }
    } else {
      // Native incorrectly applies a16-bit CID bound to source character
      // codes. The rejected sections fall back to raw charcodes, whose low
      // words become4100,4101,0000,0001 in GetText's UTF16 output.
      const std::u16string fallback = edited
        ? std::u16string{u'\u4100',u'\u4101',u'\0',u'\1',u'A',u'B'}
        : std::u16string{u'A',u'B',u'\u4100',u'\u4101',u'\0',u'\1'};
      EXPECT_EQ(Extract(api, text.get()), fallback);
    }
  };
  std::array<Raster,2> images; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api(); SCOPED_TRACE(writer->name());
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr); inspect(*writer, page.get(), false);
    images[backend++] = Render(api, page.get(), 400, 240);
    EXPECT_EQ(DarkPixels(images[backend - 1]), 2400u); page.reset();
    const auto saved = SaveDocument(api, document.get(), artifacts, writer->name() + "-full-width-tounicode");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api(); SCOPED_TRACE(reader->name());
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      Handle reloaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(reloaded.get(), nullptr); inspect(*reader, reloaded.get(), false);
      CompareTextRaster(images[backend - 1], Render(read, reloaded.get(), 400, 240), artifacts,
        writer->name() + "-to-" + reader->name() + "-full-width-tounicode");
    }
    if (writer->name() != "rust") continue;
    // Exercise reverse Unicode lookup and3/4-byte content serialization too.
    Handle edited(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(edited.get(), nullptr);
    auto object = api.FPDFPage_GetObject(edited.get(), 0); ASSERT_NE(object, nullptr);
    ASSERT_TRUE(api.FPDFText_SetText(object, Wide(u"CDEFAB").data()));
    ASSERT_TRUE(api.FPDFPage_GenerateContent(edited.get()));
    inspect(*writer, edited.get(), true);
    CompareRaster(images[backend - 1], Render(api, edited.get(), 400, 240), artifacts,
      "full-width-tounicode-edited-glyphs");
    edited.reset();
    const auto changed = SaveDocument(api, document.get(), artifacts, "rust-full-width-tounicode-edited");
    ASSERT_FALSE(changed.empty());
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(changed.data(), changed.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      Handle reloaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(reloaded.get(), nullptr); inspect(*reader, reloaded.get(), true);
      CompareTextRaster(images[backend - 1], Render(read, reloaded.get(), 400, 240), artifacts,
        reader->name() + "-full-width-tounicode-edited");
    }
  }
  CompareTextRaster(images[0], images[1], artifacts, "full-width-tounicode-backends");
}

TEST(CoverageTextFonts, UnicodeRangeArraysIncrementingSequencesAndDuplicateCodes) {
  Artifacts artifacts;
  const std::string spaces = "1 begincodespacerange <00> <FF> endcodespacerange";
  const std::string encoding = "/WMode 0 def " + spaces +
    " 1 begincidrange <41> <48> 1 endcidrange";
  // Exercise scalar ranges, array destinations, and incrementing strings.
  // One source glyph can produce multiple searchable/extractable characters.
  const auto unicode = UnicodeCmap(
    "3 beginbfrange <41> <42> <0061> <43> <44> [<03A9> <00660069>] <45> <46> <00780031> endbfrange\n"
    "2 beginbfchar <47> <005A> <48> <0021> endbfchar", spaces);
  const auto bytes = CidFixture(encoding, unicode, "4142434445464748", true);
  CompareFixture(bytes, artifacts, "unicode-range-sequences", u"ab\u03a9fix1x2Z!",
    [](const Api& api, FPDF_PAGE, FPDF_TEXTPAGE text) {
      const auto query = Wide(u"fix1x2");
      Handle search(api.FPDFText_FindStart(text, query.data(), 0, 0), api.FPDFText_FindClose);
      ASSERT_TRUE(api.FPDFText_FindNext(search.get()));
      EXPECT_EQ(api.FPDFText_GetSchResultIndex(search.get()), 3);
      EXPECT_EQ(api.FPDFText_GetSchCount(search.get()), 6);
      double fi_x, fi_y, i_x, i_y;
      ASSERT_TRUE(api.FPDFText_GetCharOrigin(text, 3, &fi_x, &fi_y));
      ASSERT_TRUE(api.FPDFText_GetCharOrigin(text, 4, &i_x, &i_y));
      EXPECT_NEAR(fi_x, i_x, .001); EXPECT_NEAR(fi_y, i_y, .001);
    });
}

std::vector<uint8_t> Type3Fixture(bool colored, bool image, bool missing_width) {
  const auto unicode = UnicodeCmap("2 beginbfchar <41> <0041> <42> <0042> endbfchar",
      "1 begincodespacerange <00> <FF> endcodespacerange");
  // d0/d1 glyph programs cannot be constructed through the public text API.
  // B intentionally has no explicit ToUnicode-free Adobe glyph name fallback.
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 240] /Resources << /Font << /F 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type3 /Name /F /BaseFont /CoverageType3 /FontBBox [0 0 600 700] /FontMatrix [.001 0 0 .001 0 0] "
      "/CharProcs << /A 6 0 R /B 7 0 R >> /Encoding << /Type /Encoding /Differences [65 /A /B] >> /FirstChar 65 /LastChar 66 " +
      (missing_width ? std::string("") : "/Widths [600 600] ") + "/ToUnicode 8 0 R /Resources << /XObject << /Im 9 0 R >> >> >>",
    PdfStream("", "q 0 .3 .8 rg BT /F 40 Tf 1 0 0 1 40.25 100.25 Tm (ABA) Tj ET Q"),
    PdfStream("", (colored ? std::string("600 0 d0 1 0 0 rg ") : "600 0 0 0 600 700 d1 ") +
      (image ? "q 600 0 0 700 0 0 cm /Im Do Q" : "0 0 600 700 re f")),
    PdfStream("", "600 0 0 0 600 700 d1 0 0 m 600 0 l 300 700 l h f"),
    PdfStream("", unicode),
    PdfStream("/Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 1 /ImageMask true", std::string("\xc0\xc0",2))});
}

TEST(CoverageTextFonts, Type3ColoredStencilAndImageGlyphsExposeTextMetricsAndSave) {
  Artifacts artifacts;
  for (bool colored : {false, true}) for (bool image : {false, true}) {
    const auto label = std::string("type3-") + (colored ? "colored" : "stencil") + (image ? "-image" : "-path");
    CompareFixture(Type3Fixture(colored, image, false), artifacts, label, u"ABA",
      [](const Api& api, FPDF_PAGE, FPDF_TEXTPAGE text) {
        for (int i = 0; i < 3; ++i) {
          double x, y, left, right, bottom, top;
          ASSERT_TRUE(api.FPDFText_GetCharOrigin(text, i, &x, &y));
          EXPECT_NEAR(x, 40.25 + i * 24, .001); EXPECT_NEAR(y, 100.25, .001);
          ASSERT_TRUE(api.FPDFText_GetCharBox(text, i, &left, &right, &bottom, &top));
          EXPECT_NEAR(right - left, 24, .01); EXPECT_NEAR(top - bottom, 28, .01);
          FS_RECTF loose{}; ASSERT_TRUE(api.FPDFText_GetLooseCharBox(text, i, &loose));
          EXPECT_GT(loose.right, loose.left); EXPECT_GT(loose.top, loose.bottom);
        }
      });
  }
}

TEST(CoverageTextFonts, BidiArabicNormalizationAndMirroredHebrewRetainSelection) {
  Artifacts artifacts;
  const std::string spaces = "1 begincodespacerange <00> <FF> endcodespacerange";
  // The pinned arabic.pdf intentionally lacks an embedded font and renders
  // blank under this harness's deterministic no-system-font configuration.
  // Keep its presentation-form input/mappings, but embed Ahem so both engines
  // must visibly render the decoded glyphs while normalizing the same text.
  const auto arabic = CidFixture("/WMode 0 def " + spaces +
    " 1 begincidrange <01> <0A> 1 endcidrange", UnicodeCmap(
    "10 beginbfchar <01> <0020> <02> <FE84> <03> <FE8D> <04> <FE92> <05> <FEA4> "
    "<06> <FEAD> <07> <FEAE> <08> <FEAF> <09> <FED5> <0A> <FEDF> endbfchar", spaces),
    "090608020A03010705040A03", true);
  struct BidiCase {std::string name;std::vector<uint8_t> bytes;std::u16string expected;};
  const std::array<BidiCase,2> fixtures = {{
    {"embedded-arabic",arabic, u"\u0627\u0644\u0628\u062d\u0631 \u0627\u0644\u0623\u0632\u0631\u0642"},
    {"hebrew_mirrored.pdf",Resource("hebrew_mirrored.pdf"), u"\u05d1\u05e0\u05d9\u05de\u05d9\u05df\r\n\u05df\u05d1"}
  }};
  for (const auto& fixture : fixtures) {
    CompareFixture(fixture.bytes, artifacts, fixture.name, fixture.expected,
      [](const Api& api, FPDF_PAGE, FPDF_TEXTPAGE text) {
        for (int i = 0; i < api.FPDFText_CountChars(text); ++i) {
          if (api.FPDFText_IsGenerated(text, i)) continue;
          double left, right, bottom, top;
          ASSERT_TRUE(api.FPDFText_GetCharBox(text, i, &left, &right, &bottom, &top));
          EXPECT_GT(right - left, 0); EXPECT_GT(top - bottom, 0);
        }
        EXPECT_GT(api.FPDFText_CountRects(text, 0, -1), 0);
      });
  }
}

TEST(CoverageTextFonts, ActualTextBidirectionalSegmentsPreserveLogicalSearchAndSourceGeometry) {
  Artifacts artifacts;
  const auto bytes = Resource("actual_text_rtl.pdf");
  const std::u16string shalom = u"\u05e9\u05dc\u05d5\u05dd", water = u"\u05de\u05d9\u05dd";
  // Expected logical order comes from pinned FPDFTextEmbedderTest.ActualTextRtl.
  // Its predominantly RTL line retains the native ordering of two runs; the
  // rest independently checks extraction, mirrored punctuation and searching.
  const std::u16string expected = u"Hello is " + shalom + u"\r\nWater is water " + water +
    u"\r\nMirrored \u05dd](\r\n" + shalom + u"\r\n" + water + u" Ha " + shalom +
    u"\r\nHe " + shalom + u"\r\nPure pdf\r\nHi " + shalom + u" hi\r\nHo ho " + shalom +
    u"\r\nHu " + shalom + u" Ha " + water + u"\r\nNa Hi " + shalom + u" Go\r\n" +
    shalom + u" Ne LTR " + water + u"\r\n" + shalom + u"\u05d1\u05df";
  CompareFixture(bytes, artifacts, "actualtext-bidi", expected,
    [shalom](const Api& api, FPDF_PAGE, FPDF_TEXTPAGE text) {
      const auto query = Wide(shalom);
      Handle search(api.FPDFText_FindStart(text, query.data(), FPDF_MATCHCASE, 0), api.FPDFText_FindClose);
      int occurrences = 0;
      while (api.FPDFText_FindNext(search.get())) {
        EXPECT_EQ(api.FPDFText_GetSchCount(search.get()), 4);
        EXPECT_GE(api.FPDFText_GetSchResultIndex(search.get()), 0);
        EXPECT_GT(api.FPDFText_CountRects(text, api.FPDFText_GetSchResultIndex(search.get()), 4), 0);
        ASSERT_LT(++occurrences, 20);
      }
      EXPECT_EQ(occurrences, 10);
    });
}

TEST(CoverageTextFonts, AllLatinLigaturesNormalizeAndRemainSearchableAcrossGlyphExpansion) {
  Artifacts artifacts;
  const auto unicode = UnicodeCmap(
    "1 beginbfrange <41> <47> <FB00> endbfrange", "1 begincodespacerange <00> <FF> endcodespacerange");
  const auto encoding = std::string("1 begincodespacerange <00> <FF> endcodespacerange ") +
    "1 begincidrange <41> <47> 1 endcidrange";
  CompareFixture(CidFixture(encoding, unicode, "41424344454647", true), artifacts,
    // FB05 expands to LONG S + t; FB06 expands to ordinary s + t.
    "normalized-latin-ligatures", u"fffiflffiffl\u017ftst", [](const Api& api, FPDF_PAGE, FPDF_TEXTPAGE text) {
      const auto query = Wide(u"ffi");
      Handle search(api.FPDFText_FindStart(text, query.data(), FPDF_MATCHCASE | FPDF_CONSECUTIVE, 0), api.FPDFText_FindClose);
      ASSERT_TRUE(api.FPDFText_FindNext(search.get()));
      EXPECT_EQ(api.FPDFText_GetSchCount(search.get()), 3);
      EXPECT_EQ(api.FPDFText_GetSchResultIndex(search.get()), 1);
    });
}

TEST(CoverageTextFonts, IPv6PortsBalancedDelimitersAndEmailTerminationUseExactLinkRanges) {
  Artifacts artifacts;
  const std::array<std::pair<std::u16string,std::u16string>,8> examples = {{
    {u"http://[2001:db8::1]:8080/path",u"http://[2001:db8::1]:8080/path"},
    {u"(https://example.org/path)",u"https://example.org/path"},
    {u"[https://example.org/path]",u"https://example.org/path"},
    {u"{https://example.org/path}",u"https://example.org/path"},
    {u"https://example.org/path,",u"https://example.org/path"},
    {u"www.example.org/test.",u"http://www.example.org/test"},
    // This is the source extractor's conservative address syntax: '+'
    // terminates the local-part prefix, leaving a valid tag@example.org span.
    {u"person+tag@example.org;",u"mailto:tag@example.org"},
    {u"<person@example.org>",u"mailto:person@example.org"}
  }};
  for (size_t scenario = 0; scenario < examples.size(); ++scenario) {
    std::array<Raster,2> images; size_t backend = 0;
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name() + "/" + std::to_string(scenario)); const auto& api = library->api();
      Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      Handle page(api.FPDFPage_New(document.get(), 0, 400, 240), api.FPDF_ClosePage);
      ASSERT_NE(Add(api, document.get(), page.get(), examples[scenario].first, 15, 120, 12), nullptr);
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      images[backend++] = Render(api, page.get(), 400, 240); page.reset();
      CrossReload(*library, document.get(), artifacts, "link-boundary-" + std::to_string(scenario),
        [&,scenario](const Api& read, FPDF_PAGE, FPDF_TEXTPAGE text) {
          Handle links(read.FPDFLink_LoadWebLinks(text), read.FPDFLink_CloseWebLinks);
          ASSERT_EQ(read.FPDFLink_CountWebLinks(links.get()), 1);
          const int count = read.FPDFLink_GetURL(links.get(), 0, nullptr, 0);
          ASSERT_EQ(count, static_cast<int>(examples[scenario].second.size()) + 1);
          std::vector<unsigned short> units(count + 1, 0xaaaa);
          EXPECT_EQ(read.FPDFLink_GetURL(links.get(), 0, units.data(), count), count);
          EXPECT_EQ(std::u16string(units.begin(), units.begin() + count - 1), examples[scenario].second);
          EXPECT_EQ(units[count - 1], 0); EXPECT_EQ(units[count], 0xaaaa);
          int start = -1, length = -1;
          ASSERT_TRUE(read.FPDFLink_GetTextRange(links.get(), 0, &start, &length));
          EXPECT_GE(start, 0); EXPECT_GT(length, 0);
          EXPECT_LE(start + length, read.FPDFText_CountChars(text));
          EXPECT_EQ(read.FPDFLink_CountRects(links.get(), 0), 1);
        }, FPDF_NO_INCREMENTAL, RasterTolerance{18,0,.8});
    }
    // Investigated these Courier URL edges: max15 and mean .753 in the
    // painted ROI. Tighten the channel cap to18 while allowing mean .8.
    CompareTextRaster(images[0], images[1], artifacts, "link-boundary-" + std::to_string(scenario), RasterTolerance{18,0,.8});
  }
}

std::string Type1Encrypt(const std::vector<uint8_t>& input, int len_iv) {
  if (len_iv < 0) return std::string(input.begin(), input.end());
  std::vector<uint8_t> plain(static_cast<size_t>(len_iv), 0);
  plain.insert(plain.end(), input.begin(), input.end());
  uint16_t key = 4330;
  std::string output;
  for (const auto byte : plain) {
    const auto encrypted = static_cast<uint8_t>(byte ^ (key >> 8));
    output.push_back(static_cast<char>(encrypted));
    key = static_cast<uint16_t>((uint32_t(encrypted) + key) * 52845 + 22719);
  }
  return output;
}

std::vector<uint8_t> CidType1Fixture(int len_iv, bool hex) {
  // Original Adobe #5014 CID-keyed Type1 control, with two font dictionaries,
  // a local subroutine, empty CID0 and independently transformed CID1/CID2.
  // Type1 charstring bytes encode 0 500 hsbw, 0 callsubr, endchar; the
  // subroutine draws a 500x700 triangle. There is no external font program.
  const auto glyph = Type1Encrypt({139,248,136,13,139,10,14}, len_iv);
  const auto subr = Type1Encrypt({139,139,21,248,136,139,5,252,136,249,80,5,9,11}, len_iv);
  const size_t first = 12, second = first + glyph.size(), subr_map = second + glyph.size();
  const size_t second_subr_map = subr_map + 4 + subr.size();
  std::string binary;
  const auto put16 = [&binary](size_t value) {
    binary.push_back(static_cast<char>(value >> 8)); binary.push_back(static_cast<char>(value));
  };
  for (const auto& entry : std::array<std::pair<int,size_t>,4>{{{0,first},{0,first},{1,second},{0,subr_map}}}) {
    binary.push_back(static_cast<char>(entry.first)); put16(entry.second);
  }
  binary += glyph; binary += glyph;
  for (const auto map : {subr_map, second_subr_map}) {
    put16(map + 4); put16(map + 4 + subr.size()); binary += subr;
  }
  std::string font = "%!PS-Adobe-3.0 Resource-CIDFont\n"
    "/CIDFontName /CoverageCID def /CIDFontVersion 1 def /CIDFontType 0 def\n"
    "/CIDSystemInfo 3 dict dup begin /Registry (Adobe) def /Ordering (Identity) def /Supplement 0 def end def\n"
    "/FontInfo 3 dict dup begin /FamilyName (Coverage CID) def /FullName (Coverage CID) def /Weight (Regular) def end def\n"
    "/FontBBox [0 0 1000 700] def /CIDMapOffset 0 def /FDBytes 1 def /GDBytes 2 def /CIDCount 3 def\n"
    "/FDArray 2 array\n%ADOBeginFontDict\n"
    "/FontMatrix [.001 0 0 .001 0 0] def /Private 8 dict dup begin /lenIV " + std::to_string(len_iv) +
    " def /SubrMapOffset " + std::to_string(subr_map) + " def /SDBytes 2 def /SubrCount 1 def end def\n"
    "%ADOEndFontDict\n%ADOBeginFontDict\n"
    "/FontMatrix [.002 0 0 .001 0 0] def /Private 8 dict dup begin /lenIV " + std::to_string(len_iv) +
    " def /SubrMapOffset " + std::to_string(second_subr_map) + " def /SDBytes 2 def /SubrCount 1 def end def\n"
    "%ADOEndFontDict\n(" + (hex ? std::string("Hex") : "Binary") + ") " + std::to_string(binary.size()) + " StartData ";
  if (hex) {
    for (const auto byte : binary) font += Hex(static_cast<uint8_t>(byte), 2) + " ";
  } else font += binary;
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 240] /Resources << /Font << /F 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type0 /BaseFont /CoverageCID /Encoding /Identity-H /DescendantFonts [6 0 R] /ToUnicode 7 0 R >>",
    PdfStream("", "BT /F 60 Tf 1 0 0 1 40.25 90.25 Tm <00010002> Tj ET"),
    "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /CoverageCID /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 8 0 R /DW 500 /W [2 [1000]] >>",
    PdfStream("", UnicodeCmap("2 beginbfchar <0001> <0041> <0002> <0042> endbfchar")),
    "<< /Type /FontDescriptor /FontName /CoverageCID /Flags 4 /FontBBox [0 0 1000 700] /Ascent 700 /Descent 0 /CapHeight 700 /ItalicAngle 0 /StemV 80 /FontFile3 9 0 R >>",
    PdfStream("/Subtype /CIDFontType0 /Length1 " + std::to_string(font.size()), font)});
}

TEST(CoverageTextFonts, CidKeyedType1BinaryHexEncryptionSubroutinesAndFdMatrices) {
  Artifacts artifacts;
  for (const int len_iv : {-1,0,1,4}) for (bool hex : {false,true}) {
    const auto label = std::string("cid-type1-") + (hex ? "hex-" : "binary-") + std::to_string(len_iv);
    CompareFixture(CidType1Fixture(len_iv, hex), artifacts, label, u"AB",
      [](const Api& api, FPDF_PAGE, FPDF_TEXTPAGE text) {
        double first_x, first_y, second_x, second_y;
        ASSERT_TRUE(api.FPDFText_GetCharOrigin(text, 0, &first_x, &first_y));
        ASSERT_TRUE(api.FPDFText_GetCharOrigin(text, 1, &second_x, &second_y));
        EXPECT_NEAR(first_x, 40.25, .001); EXPECT_NEAR(second_x - first_x, 30, .001);
        EXPECT_NEAR(first_y, second_y, .001);
        double l1,r1,b1,t1,l2,r2,b2,t2;
        ASSERT_TRUE(api.FPDFText_GetCharBox(text, 0, &l1,&r1,&b1,&t1));
        ASSERT_TRUE(api.FPDFText_GetCharBox(text, 1, &l2,&r2,&b2,&t2));
        EXPECT_NEAR(r1-l1, 30, .01); EXPECT_NEAR(r2-l2, 60, .01);
        // Public character boxes include PDFium's non-tricky-font top/64
        // allowance: 700 + floor(700/64) =710 font units, at size60.
        EXPECT_NEAR(t1-b1, 42.6, .01); EXPECT_NEAR(t2-b2, 42.6, .01);
      });
  }
}

}  // namespace
}  // namespace pdfoxium::integration
