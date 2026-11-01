#include "document_fixture.h"
#include "text_fixture.h"

#include <array>
#include <string>

namespace pdfoxium::integration {
namespace {
using namespace document_test;
using Bytes = std::vector<uint8_t>;
Bytes SemanticFixture(const std::string& operations, const std::string& properties = "") {
  // Ahem is embedded: physical A/B/C outlines are deterministic squares.
  // ActualText and named property dictionaries have no public constructor.
  static const auto font = text_fixture::Resource("fonts/ahem/Ahem.ttf");
  std::string widths;
  for (int code = 32; code <= 90; ++code) widths += "1000 ";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F 5 0 R >> /Properties << " + properties + " >> >> /Contents 4 0 R >>",
      PdfStream("", "BT /F 10 Tf 1 0 0 1 20 160 Tm " + operations + " ET\n"),
      "<< /Type /Font /Subtype /TrueType /BaseFont /Ahem /Encoding /WinAnsiEncoding /FirstChar 32 /LastChar 90 /Widths [" + widths + "] /FontDescriptor 6 0 R >>",
      "<< /Type /FontDescriptor /FontName /Ahem /Flags 32 /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /CapHeight 800 /StemV 80 /FontFile2 7 0 R >>",
      PdfStream("/Length1 " + std::to_string(font.size()), std::string(font.begin(), font.end()))});
}
void InspectSemantic(const Api& api, FPDF_PAGE page, const std::u16string& expected) {
  Handle text(api.FPDFText_LoadPage(page), api.FPDFText_ClosePage); ASSERT_NE(text.get(), nullptr);
  EXPECT_EQ(text_fixture::Extract(api, text.get()), expected);
  EXPECT_EQ(api.FPDFText_CountChars(text.get()), expected.size());
  // These BMP cases each have one exposed character per extracted code unit,
  // including generated spaces. Expansion never aliases a source-glyph index.
  for (int index = 0; index < static_cast<int>(expected.size()); ++index) {
    EXPECT_EQ(api.FPDFText_GetUnicode(text.get(), index), static_cast<unsigned>(expected[index]));
    EXPECT_EQ(api.FPDFText_GetTextIndexFromCharIndex(text.get(), index), index);
    EXPECT_EQ(api.FPDFText_GetCharIndexFromTextIndex(text.get(), index), index);
    std::array<unsigned short, 3> one{{0xbdbd, 0xbdbd, 0xbdbd}};
    EXPECT_EQ(api.FPDFText_GetText(text.get(), index, 1, one.data()), 2);
    EXPECT_EQ(one[0], expected[index]); EXPECT_EQ(one[1], 0); EXPECT_EQ(one[2], 0xbdbd);
  }
  EXPECT_EQ(api.FPDFText_GetTextIndexFromCharIndex(text.get(), expected.size()), -1);
  EXPECT_EQ(api.FPDFText_GetCharIndexFromTextIndex(text.get(), expected.size()), -1);
  const int rectangles = api.FPDFText_CountRects(text.get(), 0, -1);
  if (expected.empty()) { EXPECT_EQ(rectangles, 0); }
  else {
    EXPECT_GT(rectangles, 0);
    for (int index = 0; index < rectangles; ++index) {
      double left = 0, top = 0, right = 0, bottom = 0;
      ASSERT_TRUE(api.FPDFText_GetRect(text.get(), index, &left, &top, &right, &bottom));
      EXPECT_GE(left, 19.9); EXPECT_LE(right, 100.1); EXPECT_GT(top, bottom); EXPECT_GE(right, left);
    }
  }
}
void SemanticWorkflow(const std::string& operations, const std::u16string& expected, const std::string& properties = "") {
  const auto input = SemanticFixture(operations, properties);
  Artifacts artifacts; artifacts.Pdf("text-semantics-input", input); std::array<Raster, 2> images; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api(); SCOPED_TRACE(writer->name());
    Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument); ASSERT_NE(doc.get(), nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(InspectSemantic(api, page.get(), expected));
    images[backend] = Render(api, page.get(), 200, 200);
    EXPECT_GT(std::count(images[backend].pixels.begin(), images[backend].pixels.end(), uint8_t(0)), 50);
    page.reset();
    for (FPDF_DWORD flags : {FPDF_NO_INCREMENTAL, FPDF_INCREMENTAL}) {
      const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-text-semantics-" + std::to_string(flags), flags); ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api(); SCOPED_TRACE(reader->name());
        Handle loaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument); ASSERT_NE(loaded.get(), nullptr);
        Handle reopened(read.FPDF_LoadPage(loaded.get(), 0), read.FPDF_ClosePage); ASSERT_NE(reopened.get(), nullptr);
        ASSERT_NO_FATAL_FAILURE(InspectSemantic(read, reopened.get(), expected));
        CompareRaster(images[backend], Render(read, reopened.get(), 200, 200), artifacts, writer->name() + "-" + reader->name() + "-semantic-save");
      }
    }
    ++backend;
  }
  CompareRaster(images[0], images[1], artifacts, "text-semantic-engines");
}
void Replacement(const std::string& value, const std::u16string& expected) {
  SemanticWorkflow("/Span << /ActualText " + value + " >> BDC (A) Tj EMC", expected);
}
TEST(CoverageTextSemanticConstraints, EmptyActualTextPreservesPhysicalCharacter) { Replacement("<FEFF>", u"A"); }
TEST(CoverageTextSemanticConstraints, IntegerActualTextPreservesPhysicalCharacter) { Replacement("17", u"A"); }
TEST(CoverageTextSemanticConstraints, NameActualTextPreservesPhysicalCharacter) { Replacement("/Replacement", u"A"); }
TEST(CoverageTextSemanticConstraints, NullActualTextPreservesPhysicalCharacter) { Replacement("null", u"A"); }
TEST(CoverageTextSemanticConstraints, NulOnlyActualTextSuppressesPhysicalCharacter) { Replacement("<FEFF0000>", u""); }
TEST(CoverageTextSemanticConstraints, TabAndNewlineOnlyActualTextSuppressesPhysicalCharacter) { Replacement("<FEFF0009000A>", u""); }
TEST(CoverageTextSemanticConstraints, DeleteOnlyActualTextSuppressesPhysicalCharacter) { Replacement("<FEFF007F>", u""); }
TEST(CoverageTextSemanticConstraints, ReplacementOnlyActualTextSuppressesPhysicalCharacter) { Replacement("<FEFFFFFD>", u""); }
TEST(CoverageTextSemanticConstraints, NoncharacterOnlyActualTextSuppressesPhysicalCharacter) { Replacement("<FEFFFFFF>", u""); }
TEST(CoverageTextSemanticConstraints, TabInsideActualTextBecomesSingleSpace) { Replacement("<FEFF004C00090052>", u"L R"); }
TEST(CoverageTextSemanticConstraints, ConsecutiveActualTextControlsCollapseToOneSpace) { Replacement("<FEFF004C00000009000A0052>", u"L R"); }
TEST(CoverageTextSemanticConstraints, ReplacementCodepointInsideActualTextIsOmitted) { Replacement("<FEFF004CFFFD0052>", u"LR"); }
TEST(CoverageTextSemanticConstraints, NoncharacterInsideActualTextIsOmitted) { Replacement("<FEFF004CFFFF0052>", u"LR"); }
TEST(CoverageTextSemanticConstraints, ParameterlessNestedMarkRetainsOuterActualText) {
  SemanticWorkflow("/Span << /ActualText (outer) >> BDC /Artifact BMC (A) Tj EMC EMC", u"outer");
}
TEST(CoverageTextSemanticConstraints, InnerActualTextOverridesOuterReplacement) {
  SemanticWorkflow("/Span << /ActualText (outer) >> BDC /Span << /ActualText (inner) >> BDC (A) Tj EMC EMC", u"inner");
}
TEST(CoverageTextSemanticConstraints, SharedPropertyDictionaryReplacesConsecutiveObjectsOnce) {
  SemanticWorkflow("/Span /Shared BDC (A) Tj (B) Tj EMC", u"word", "/Shared << /ActualText (word) >>");
}
TEST(CoverageTextSemanticConstraints, SharedPropertyUnderParameterlessMarkReplacesConsecutiveObjectsOnce) {
  SemanticWorkflow("/Span /Shared BDC /Artifact BMC (A) Tj (B) Tj EMC EMC", u"word", "/Shared << /ActualText (word) >>");
}
TEST(CoverageTextSemanticConstraints, EqualReplacementStringsInDistinctDictionariesRemainDistinct) {
  SemanticWorkflow("/Span << /ActualText (x) >> BDC (A) Tj EMC /Span << /ActualText (x) >> BDC (B) Tj EMC", u"xx");
}
TEST(CoverageTextSemanticConstraints, PositiveCharacterSpacingKeepsSourceWordTogether) { SemanticWorkflow("2 Tc (ABC) Tj", u"ABC"); }
TEST(CoverageTextSemanticConstraints, NegativeCharacterSpacingKeepsSourceWordTogether) { SemanticWorkflow("-2 Tc (ABC) Tj", u"ABC"); }
TEST(CoverageTextSemanticConstraints, TighteningKerningKeepsSourceWordTogether) { SemanticWorkflow("2 Tc [(A) 100 (B) 100 (C)] TJ", u"ABC"); }
TEST(CoverageTextSemanticConstraints, ExpandingKerningCreatesSearchableSpaces) { SemanticWorkflow("[(A) -600 (B) -600 (C)] TJ", u"A B C"); }
TEST(CoverageTextSemanticConstraints, MixedKerningSubtractsSharedCharacterSpacing) { SemanticWorkflow("2 Tc [(A) -600 (B) 100 (C)] TJ", u"A BC"); }
TEST(CoverageTextSemanticConstraints, TwoCharacterKerningUsesItsOwnSpacingBaseline) { SemanticWorkflow("2 Tc [(A) -600 (B)] TJ", u"A B"); }
}  // namespace
}  // namespace pdfoxium::integration
