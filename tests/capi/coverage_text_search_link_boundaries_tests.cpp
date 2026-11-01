#include "text_fixture.h"

#include <array>
#include <functional>
#include <iomanip>
#include <sstream>
#include <string>
#include <utility>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using Match = std::pair<int, int>;
// Ahem A's glyf box ends at 800/1000 em. CFX_Face::GetCharBBox adds
// integer top/64 to an ordinary glyph's reported character box (800+12),
// independently of its unchanged painted outline. At 10pt and y=160:
constexpr double kCharacterBoxTop = 160.0 + 10.0 * (800 + 800 / 64) / 1000.0;
uint16_t Read16(const Bytes& bytes, size_t at) {
  return at + 1 < bytes.size() ? (uint16_t(bytes[at]) << 8) | bytes[at + 1] : 0;
}
uint32_t Read32(const Bytes& bytes, size_t at) { return (uint32_t(Read16(bytes, at)) << 16) | Read16(bytes, at + 2); }
std::string Hex(unsigned value, int digits = 4) {
  std::ostringstream text; text << std::hex << std::uppercase << std::setw(digits) << std::setfill('0') << value; return text.str();
}
uint16_t FontGlyph(const Bytes& bytes, uint16_t character) {
  for (unsigned i = 0; i < Read16(bytes, 4); ++i) {
    const size_t record = 12 + i * 16;
    if (Read32(bytes, record) != 0x636d6170) continue;
    const size_t cmap = Read32(bytes, record + 8);
    for (unsigned j = 0; j < Read16(bytes, cmap + 2); ++j) {
      const size_t sub = cmap + Read32(bytes, cmap + 8 + j * 8);
      if (Read16(bytes, sub) != 4) continue;
      const unsigned count = Read16(bytes, sub + 6) / 2;
      const size_t ends = sub + 14, starts = ends + count * 2 + 2;
      const size_t deltas = starts + count * 2, ranges = deltas + count * 2;
      for (unsigned k = 0; k < count; ++k) {
        if (character < Read16(bytes, starts + k * 2) || character > Read16(bytes, ends + k * 2)) continue;
        const auto delta = Read16(bytes, deltas + k * 2), range = Read16(bytes, ranges + k * 2);
        if (!range) return static_cast<uint16_t>(character + delta);
        const auto glyph = Read16(bytes, ranges + k * 2 + range + (character - Read16(bytes, starts + k * 2)) * 2);
        return glyph ? static_cast<uint16_t>(glyph + delta) : 0;
      }
    }
  }
  ADD_FAILURE() << "Pinned Ahem lacks expected character"; return 0;
}
struct LogicalLine { std::u16string text; int x = 20, y = 160; };
Bytes LogicalFixture(const std::vector<LogicalLine>& lines, bool duplicate = false, bool zero_width = false) {
  // The public APIs cannot assign ToUnicode. Each code independently maps to
  // its logical character while all printable codes use Ahem's solid square.
  // Space uses the real blank glyph. This isolates search/link semantics from
  // Unicode font fallback and gives exact integral glyph geometry.
  static const auto font = text_fixture::Resource("fonts/ahem/Ahem.ttf");
  const auto square = FontGlyph(font, 'A'), blank = FontGlyph(font, ' ');
  std::string glyphs(2, '\0'), content, mappings;
  unsigned code = 1;
  for (const auto& line : lines) {
    std::string codes, pairs;
    for (const auto character : line.text) {
      codes += Hex(code);
      pairs += "<" + Hex(code++) + "> <" + Hex(character) + ">\n";
      const auto glyph = character == ' ' ? blank : square;
      glyphs.push_back(static_cast<char>(glyph >> 8)); glyphs.push_back(static_cast<char>(glyph));
    }
    mappings += std::to_string(line.text.size()) + " beginbfchar\n" + pairs + "endbfchar\n";
    const auto object = "BT /F 10 Tf 1 0 0 1 " + std::to_string(line.x) + " " + std::to_string(line.y) + " Tm <" + codes + "> Tj ET\n";
    content += object; if (duplicate) content += object;
  }
  const std::string cmap = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n"
      "/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n"
      "/CMapName /SearchLogical def /CMapType 2 def\n1 begincodespacerange <0000> <FFFF> endcodespacerange\n" +
      mappings + "endcmap CMapName currentdict /CMap defineresource pop end end";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 240] /Resources << /Font << /F 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", content),
      "<< /Type /Font /Subtype /Type0 /BaseFont /Ahem /Encoding /Identity-H /DescendantFonts [6 0 R] /ToUnicode 7 0 R >>",
      "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Ahem /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 8 0 R /DW " + std::string(zero_width ? "0" : "1000") + " /CIDToGIDMap 10 0 R >>",
      PdfStream("", cmap),
      "<< /Type /FontDescriptor /FontName /Ahem /Flags 32 /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /CapHeight 800 /StemV 80 /FontFile2 9 0 R >>",
      PdfStream("/Length1 " + std::to_string(font.size()), std::string(font.begin(), font.end())),
      PdfStream("", glyphs)});
}
using Inspect = std::function<void(const Library&, FPDF_PAGE, FPDF_TEXTPAGE)>;
void TextWorkflow(const Bytes& input, const std::u16string& expected, const Inspect& inspect) {
  Artifacts artifacts; artifacts.Pdf("input", input);
  std::array<Raster, 2> originals; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument); ASSERT_NE(doc.get(), nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
    Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage); ASSERT_NE(text.get(), nullptr);
    EXPECT_EQ(text_fixture::Extract(api, text.get()), expected);
    ASSERT_NO_FATAL_FAILURE(inspect(*writer, page.get(), text.get()));
    originals[backend] = Render(api, page.get(), 400, 240);
    const auto ink = std::count(originals[backend].pixels.begin(), originals[backend].pixels.end(), uint8_t(0));
    EXPECT_GT(ink, 100);
    text.reset(); page.reset();
    const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-saved"); ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name()); const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument); ASSERT_NE(reopened.get(), nullptr);
      Handle loaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage); ASSERT_NE(loaded.get(), nullptr);
      Handle extracted(read.FPDFText_LoadPage(loaded.get()), read.FPDFText_ClosePage); ASSERT_NE(extracted.get(), nullptr);
      EXPECT_EQ(text_fixture::Extract(read, extracted.get()), expected);
      ASSERT_NO_FATAL_FAILURE(inspect(*reader, loaded.get(), extracted.get()));
      CompareRaster(originals[backend], Render(read, loaded.get(), 400, 240), artifacts, writer->name() + "-to-" + reader->name());
    }
    ++backend;
  }
  CompareRaster(originals[0], originals[1], artifacts, "original-engines");
}
void SearchCase(const std::u16string& source, const std::u16string& needle, const std::vector<Match>& expected,
                unsigned long flags = FPDF_MATCHCASE, bool backwards = true, const std::u16string& extracted = u"") {
  SCOPED_TRACE(std::string(needle.begin(), needle.end()));
  TextWorkflow(LogicalFixture({{source}}), extracted.empty() ? source : extracted, [=](const Library& library, FPDF_PAGE, FPDF_TEXTPAGE text) {
    const auto& api = library.api(); const auto query = text_fixture::Wide(needle);
    Handle search(api.FPDFText_FindStart(text, query.data(), flags, 0), api.FPDFText_FindClose); ASSERT_NE(search.get(), nullptr);
    std::vector<Match> matches;
    for (size_t limit = 0; limit < expected.size() + 2 && api.FPDFText_FindNext(search.get()); ++limit) {
      const int start = api.FPDFText_GetSchResultIndex(search.get()), count = api.FPDFText_GetSchCount(search.get());
      EXPECT_GE(start, 0); EXPECT_GT(count, 0); EXPECT_LE(start + count, api.FPDFText_CountChars(text));
      matches.emplace_back(start, count);
      if (needle.find_first_not_of(u' ') != std::u16string::npos) { EXPECT_GT(api.FPDFText_CountRects(text, start, count), 0); }
    }
    EXPECT_EQ(matches, expected); EXPECT_FALSE(api.FPDFText_FindNext(search.get()));
    if (backwards) {
      Handle reverse(api.FPDFText_FindStart(text, query.data(), flags, -1), api.FPDFText_FindClose); ASSERT_NE(reverse.get(), nullptr);
      std::vector<Match> found;
      for (size_t limit = 0; limit < expected.size() + 2 && api.FPDFText_FindPrev(reverse.get()); ++limit) {
        found.emplace_back(api.FPDFText_GetSchResultIndex(reverse.get()), api.FPDFText_GetSchCount(reverse.get()));
      }
      auto reversed = expected; std::reverse(reversed.begin(), reversed.end()); EXPECT_EQ(found, reversed);
    }
  });
}
TEST(CoverageTextSearchLinkBoundaries, TrailingQuerySpaceRetriesAnOtherwiseMatchingUnseparatedWord) { SearchCase(u"catX cat ", u"cat ", {{5, 4}}); }
TEST(CoverageTextSearchLinkBoundaries, LeadingQuerySpaceSkipsAWordWithoutAnActualPrecedingGap) { SearchCase(u"Xcat cat!", u" cat", {{4, 4}}); }
TEST(CoverageTextSearchLinkBoundaries, TrailingQuerySpaceFailsWhenEveryCandidateHasASuffix) { SearchCase(u"catX catY", u"cat ", {}); }
TEST(CoverageTextSearchLinkBoundaries, NonWhitespaceBetweenWordsRetriesAtTheNextCompletePhrase) { SearchCase(u"aaXbb aa bb", u"aa bb", {{6, 5}}); }
TEST(CoverageTextSearchLinkBoundaries, RepeatedQuerySpacesRemainOneWordSeparator) { SearchCase(u"aa bb", u"aa   bb", {{0, 5}}); }
TEST(CoverageTextSearchLinkBoundaries, AllSpaceQueriesUseTheCompactedPageWhitespace) { SearchCase(u"aa  bb   cc", u" ", {{2, 1}, {5, 1}}, FPDF_MATCHCASE, true, u"aa bb cc"); }
TEST(CoverageTextSearchLinkBoundaries, ConsecutiveLiteralDoubleSpaceQueriesDoNotInventRemovedPageSpaces) { SearchCase(u"aa  bb   cc", u"  ", {}, FPDF_MATCHCASE | FPDF_CONSECUTIVE, true, u"aa bb cc"); }
TEST(CoverageTextSearchLinkBoundaries, WholeWordSearchTreatsAdjacentDecimalDigitsAsPartOfTheWord) { SearchCase(u"cat1 cat cat2", u"cat", {{5, 3}}, FPDF_MATCHCASE | FPDF_MATCHWHOLEWORD); }
TEST(CoverageTextSearchLinkBoundaries, SingleCjkWholeWordSearchMatchesInsideAnAdjacentLatinRun) { SearchCase(u"A\u4e2dB \u4e2d", u"\u4e2d", {{1, 1}, {4, 1}}, FPDF_MATCHCASE | FPDF_MATCHWHOLEWORD); }
TEST(CoverageTextSearchLinkBoundaries, CjkSearchWordsAcceptBothOmittedAndPresentWhitespace) { SearchCase(u"\u4e2d \u6587 \u4e2d\u6587", u"\u4e2d\u6587", {{0, 3}, {4, 2}}); }
TEST(CoverageTextSearchLinkBoundaries, CjkWordMatchingRejectsInterveningVisiblePunctuation) { SearchCase(u"\u4e2d_\u6587 \u4e2d \u6587", u"\u4e2d\u6587", {{4, 3}}); }
TEST(CoverageTextSearchLinkBoundaries, CrossLineSearchCountsBothGeneratedBreakCharactersInItsResult) {
  TextWorkflow(LogicalFixture({{u"Alpha", 20, 160}, {u"Beta", 20, 130}}), u"Alpha\r\nBeta", [](const Library& library, FPDF_PAGE, FPDF_TEXTPAGE text) {
    const auto& api = library.api(); const auto needle = text_fixture::Wide(u"Alpha Beta");
    Handle search(api.FPDFText_FindStart(text, needle.data(), FPDF_MATCHCASE, 0), api.FPDFText_FindClose);
    ASSERT_TRUE(api.FPDFText_FindNext(search.get())); EXPECT_EQ(api.FPDFText_GetSchResultIndex(search.get()), 0); EXPECT_EQ(api.FPDFText_GetSchCount(search.get()), 11);
    EXPECT_EQ(api.FPDFText_CountRects(text, 0, 11), 2); EXPECT_FALSE(api.FPDFText_FindNext(search.get()));
    double left, top, right, bottom; ASSERT_TRUE(api.FPDFText_GetRect(text, 0, &left, &top, &right, &bottom));
    EXPECT_NEAR(left, 20, .01); EXPECT_NEAR(right, 70, .01); EXPECT_GT(top, bottom);
    ASSERT_TRUE(api.FPDFText_GetRect(text, 1, &left, &top, &right, &bottom)); EXPECT_NEAR(left, 20, .01); EXPECT_NEAR(right, 60, .01); EXPECT_GT(top, bottom);
  });
}

struct LinkSpec {
  std::u16string input, url;
  int start = 0, count = 0;
  bool native_candidate_range = false;
};
void LinkCase(const LinkSpec& spec) {
  TextWorkflow(LogicalFixture({{spec.input}}), spec.input, [=](const Library& library, FPDF_PAGE, FPDF_TEXTPAGE text) {
    const auto& api = library.api(); Handle links(api.FPDFLink_LoadWebLinks(text), api.FPDFLink_CloseWebLinks); ASSERT_NE(links.get(), nullptr);
    ASSERT_EQ(api.FPDFLink_CountWebLinks(links.get()), spec.url.empty() ? 0 : 1);
    if (spec.url.empty()) return;
    const int required = api.FPDFLink_GetURL(links.get(), 0, nullptr, 0); ASSERT_EQ(required, static_cast<int>(spec.url.size() + 1));
    std::vector<unsigned short> buffer(required + 1, 0xa5a5); ASSERT_EQ(api.FPDFLink_GetURL(links.get(), 0, buffer.data(), required), required);
    EXPECT_EQ(std::u16string(buffer.begin(), buffer.begin() + required - 1), spec.url); EXPECT_EQ(buffer[required - 1], 0); EXPECT_EQ(buffer[required], 0xa5a5);
    int start = -1, count = -1; ASSERT_TRUE(api.FPDFLink_GetTextRange(links.get(), 0, &start, &count));
    // Native keeps the pre-trim mail candidate range after rewriting its URL.
    // Rust must select just the actual address; both use the same URL and ink.
    const bool original = spec.native_candidate_range && library.name() == "native";
    const int expected_start = original ? 0 : spec.start, expected_count = original ? static_cast<int>(spec.input.size()) : spec.count;
    EXPECT_EQ(start, expected_start); EXPECT_EQ(count, expected_count);
    ASSERT_EQ(api.FPDFLink_CountRects(links.get(), 0), 1);
    double left, top, right, bottom; ASSERT_TRUE(api.FPDFLink_GetRect(links.get(), 0, 0, &left, &top, &right, &bottom));
    EXPECT_NEAR(left, 20 + expected_start * 10, .01); EXPECT_NEAR(right, 20 + (expected_start + expected_count) * 10, .01);
    EXPECT_NEAR(top, kCharacterBoxTop, .01); EXPECT_NEAR(bottom, 158, .01);
  });
}
TEST(CoverageTextSearchLinkBoundaries, Ipv6HostWithoutPathUsesTheClosingAddressBracket) { const std::u16string url = u"http://[2001:db8::1]"; LinkCase({url, url, 0, static_cast<int>(url.size())}); }
TEST(CoverageTextSearchLinkBoundaries, Ipv6PortWithoutPathConsumesTheCompleteDecimalPort) { const std::u16string url = u"http://[2001:db8::1]:8080"; LinkCase({url, url, 0, static_cast<int>(url.size())}); }
TEST(CoverageTextSearchLinkBoundaries, Ipv6PortStopsAtTheFirstNondecimalCharacter) { const std::u16string url = u"http://[2001:db8::1]:443"; LinkCase({url + u"tail", url, 0, static_cast<int>(url.size())}); }
TEST(CoverageTextSearchLinkBoundaries, QuotedWebAddressExcludesBothDoubleQuotesFromItsRange) { const std::u16string url = u"https://example.org/a"; LinkCase({u"\"" + url + u"\"", url, 1, static_cast<int>(url.size())}); }
TEST(CoverageTextSearchLinkBoundaries, QuotedWebAddressExcludesBothSingleQuotesFromItsRange) { const std::u16string url = u"https://example.org/a"; LinkCase({u"'" + url + u"'", url, 1, static_cast<int>(url.size())}); }
TEST(CoverageTextSearchLinkBoundaries, AngleDelimiterEndsAWebAddressBeforeFollowingVisibleText) { const std::u16string url = u"https://example.org/a"; LinkCase({u"<" + url + u">suffix", url, 1, static_cast<int>(url.size())}); }
TEST(CoverageTextSearchLinkBoundaries, EmailLocalPartEndingInAPeriodIsRejected) { LinkCase({u"a.@example.org", u""}); }
TEST(CoverageTextSearchLinkBoundaries, EmailWithNoLocalPartIsRejected) { LinkCase({u"@example.org", u""}); }
TEST(CoverageTextSearchLinkBoundaries, EmailDomainCannotStartWithAPeriod) { LinkCase({u"a@.example.org", u""}); }
TEST(CoverageTextSearchLinkBoundaries, EmailDomainRejectsUnderscoresBeforeItsFirstDot) { LinkCase({u"a@exam_ple.org", u""}); }
TEST(CoverageTextSearchLinkBoundaries, EmailDomainMustRetainARealSuffixBeforeAnInvalidCharacter) { LinkCase({u"a@x.!tail", u""}); }
TEST(CoverageTextSearchLinkBoundaries, EmailTrailingDoublePeriodKeepsOnlyTheCompleteAddress) { const std::u16string address = u"a@example.org"; LinkCase({address + u"..tail", u"mailto:" + address, 0, static_cast<int>(address.size()), true}); }
TEST(CoverageTextSearchLinkBoundaries, EmailConsecutiveLocalDotsLeaveTheValidAddressSuffix) { const std::u16string address = u"b@example.org"; LinkCase({u"a.." + address, u"mailto:" + address, 3, static_cast<int>(address.size()), true}); }
TEST(CoverageTextSearchLinkBoundaries, EmailMailtoPrefixDoesNotBecomePartOfTheSelectedAddress) { const std::u16string address = u"a@example.org"; LinkCase({u"mailto:" + address, u"mailto:" + address, 7, static_cast<int>(address.size()), true}); }
TEST(CoverageTextSearchLinkBoundaries, DuplicateZeroAdvanceTextUsesFontOutlineWidthForOverlapSuppression) {
  TextWorkflow(LogicalFixture({{u"A"}}, true, true), u"A", [](const Library& library, FPDF_PAGE page, FPDF_TEXTPAGE text) {
    const auto& api = library.api(); EXPECT_EQ(api.FPDFPage_CountObjects(page), 2); EXPECT_EQ(api.FPDFText_CountChars(text), 1);
    double left, right, bottom, top; ASSERT_TRUE(api.FPDFText_GetCharBox(text, 0, &left, &right, &bottom, &top));
    EXPECT_NEAR(left, 20, .01); EXPECT_NEAR(right, 30, .01); EXPECT_NEAR(top, kCharacterBoxTop, .01); EXPECT_NEAR(bottom, 158, .01);
  });
}

}  // namespace
}  // namespace pdfoxium::integration
