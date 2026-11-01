#include "xfa_form_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <ctime>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
struct PictureRow { const char* expression; const char16_t* value; bool javascript = false; bool numeric = false; const char16_t* alert = nullptr; const char16_t* native_value = nullptr; };

std::string PictureField(const std::string& name, int y, int height, const std::string& value) {
  return "<field name=\"" + name + "\" x=\"8pt\" y=\"" + std::to_string(y) +
      "pt\" w=\"300pt\" h=\"" + std::to_string(height) + "pt\"><bind match=\"none\"/>"
      "<font typeface=\"Helvetica\" size=\"10pt\"/><ui><textEdit multiLine=\"1\"/></ui>"
      "<value><text>" + value + "</text></value></field>";
}
std::vector<uint8_t> PictureModelFixture(const PictureRow& row) {
  const std::string expression = row.expression;
  const std::string program = row.javascript ? "var r=xfa.form.root;r.result.rawValue='['+(function(){" + expression + "})()+']';" :
      "root.result.rawValue=Concat(\"[\"," + expression + ",\"]\")";
  const std::string preamble = "<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
  const std::string config = "<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
  std::string xml = "<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" locale=\"en_US\" restoreState=\"auto\">"
      "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"320pt\" long=\"240pt\"/><contentArea w=\"320pt\" h=\"240pt\"/></pageArea></pageSet>" +
      PictureField("result", 8, 44, "Ready") +
      (row.numeric ? "<field name=\"input\" x=\"8pt\" y=\"68pt\" w=\"300pt\" h=\"24pt\"><bind match=\"none\"/><font typeface=\"Helvetica\" size=\"10pt\"/><ui><numericEdit/><picture>num{zz9.99}</picture></ui><value><decimal>12</decimal></value></field>" : PictureField("input", 68, 24, "Usable")) +
      "<field name=\"run\" x=\"8pt\" y=\"110pt\" w=\"80pt\" h=\"24pt\"><bind match=\"none\"/><ui><button/></ui><font typeface=\"Helvetica\" size=\"10pt\"/>"
      "<caption><value><text>Run</text></value></caption>" +
      forms::Event("this.caption.value.text.value='Run';", "initialize") +
      forms::Event(program, "click", row.javascript ? "application/x-javascript" : "application/x-formcalc") +
      "</field><draw x=\"280pt\" y=\"200pt\" w=\"20pt\" h=\"20pt\"><value><rectangle><fill><color value=\"0,255,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>"
      "</subform></template>";
  // Preserve the dedicated native CDATA read-plane regression; keep these
  // picture grammar programs within complete native delimiter planes.
  for (size_t i = 0; i < xml.size();) {
    const size_t n = xml.compare(i, 9, "<![CDATA[") == 0 ? 9 : xml.compare(i, 3, "]]>") == 0 ? 3 : 0;
    if (!n) { ++i; continue; }
    const size_t offset = preamble.size() + config.size() + i;
    if (offset / 1024 != (offset + n - 1) / 1024) {
      const size_t padding = 1024 - offset % 1024; xml.insert(i, padding, ' '); i += padding;
    }
    i += n;
  }
  std::vector<std::string> objects{
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 240] /Resources << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 10 Tf 0 g) /DR << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("", preamble), PdfStream("", config), PdfStream("", xml),
      PdfStream("", "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
      PdfStream("", "</xdp:xdp>"),
      "null"};  // Object 10 is reserved; pinned font objects are 11..13.
  auto font = forms::PinnedXfaArimoFontObjects();
  objects.push_back(std::move(font[0]));
  objects.push_back(std::move(font[1]));
  objects.push_back(std::move(font[2]));
  return PdfFixture(objects);
}
void PictureBlur(Session& s) {
  ASSERT_TRUE(s.Focus(12, 76)); s.api.FORM_ForceToKillFocus(s.form);
  s.Click(310, 230); s.api.FORM_ForceToKillFocus(s.form);
  s.api.FORM_OnMouseMove(s.form, s.page, 0, 310, 230);
}
void PictureExecute(Session& s, const std::u16string& expected, const PictureRow& row) {
  ASSERT_TRUE(s.Focus(12, 76)); ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page)); s.Replace(row.numeric ? u"12" : u"Usable");
  s.api.FORM_ForceToKillFocus(s.form); s.Click(12, 120);
  ASSERT_TRUE(s.Focus(12, 16)); EXPECT_EQ(s.Text(), expected);
  if (row.native_value) {
    // Assert each reader's original scalar before giving both readers the
    // same visible value through the public editing API, including reloads.
    ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page));
    s.Replace(std::u16string(u"[") + row.value + u"]");
    EXPECT_EQ(s.Text(), std::u16string(u"[") + row.value + u"]");
  }
  PictureBlur(s);
  if (row.alert) {
    bool found = false;
    for (const auto& alert : s.host->alerts) {
      found = found || std::get<0>(alert).find(row.alert) != std::u16string::npos;
    }
    EXPECT_TRUE(found);
  } else { EXPECT_TRUE(s.host->alerts.empty()); }
  EXPECT_TRUE(s.host->callbacks_valid); EXPECT_FALSE(s.host->callback_error);
}
void ComparePictureModel(const Raster& a, const Raster& b, Artifacts& artifacts, const std::string& label) {
  CompareRaster(a, b, artifacts, label, RasterTolerance::ConnectedError(1.0));
  CompareRaster(CropLogicalRaster(a, 8, 8, 300, 44), CropLogicalRaster(b, 8, 8, 300, 44), artifacts, label + "-value", RasterTolerance::ConnectedError(1.0));
  CompareRaster(CropLogicalRaster(a, 280, 200, 20, 20), CropLogicalRaster(b, 280, 200, 20, 20), artifacts, label + "-landmark");
}
void PictureRows(const std::vector<PictureRow>& rows) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  size_t case_index = 0;
  for (const auto& row : rows) {
    const std::string label = std::to_string(case_index++);
    SCOPED_TRACE(row.expression); Artifacts artifacts; std::array<Raster, 2> originals; size_t index = 0;
    const auto fixture = PictureModelFixture(row);
    const auto expected = [&](const Library& reader) {
      const auto value = row.native_value && reader.name() == "native" ? row.native_value : row.value;
      return std::u16string(u"[") + value + u"]";
    };
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name()); Session s(writer->api(), fixture, true, true);
      ASSERT_TRUE(s.valid()); ASSERT_TRUE(s.xfa_loaded);
     ASSERT_NO_FATAL_FAILURE(PictureExecute(s, expected(*writer), row));
      originals[index] = s.DrawForComparison(320, 240); forms::ExpectInk(originals[index], 8, 8, 308, 52);
      const auto saved = SaveDocument(s.api, s.document, artifacts, writer->name() + "-picture-model-" + label); ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        SCOPED_TRACE(reader->name()); Session loaded(reader->api(), saved, true, true);
        ASSERT_TRUE(loaded.valid()); ASSERT_TRUE(loaded.xfa_loaded);
        ASSERT_NO_FATAL_FAILURE(PictureExecute(loaded, expected(*reader), row));
        ComparePictureModel(originals[index], loaded.DrawForComparison(320, 240), artifacts, writer->name() + "-" + reader->name() + "-" + label);
      }
      ++index;
    }
    ComparePictureModel(originals[0], originals[1], artifacts, "picture-model-engines-" + label);
  }
}

// The numeric rows extend the pinned CFGAS_StringFormatter NumParse/NumFormat
// cases into an actual FormCalc/FF document. Expected values are independent
// decimal quantities, not another call to the implementation under test.
TEST(CoverageXfaPictureModels, TrailingSignsDistinguishMinusPlusAndBlankSlots) {
  PictureRows({{R"PICTURE(Parse("ZZ9.ZZZS","123.456-"))PICTURE", u"-123.456"},
    {R"PICTURE(Parse("ZZ9.ZZZS","123.456+"))PICTURE", u"123.456"},
    {R"PICTURE(Parse("ZZ9.ZZZS","123.456 "))PICTURE", u"123.456"},
    {R"PICTURE(Parse("ZZ9.ZZZS","123.456x"))PICTURE", u""}});
}
TEST(CoverageXfaPictureModels, ParenthesisSignSlotsRecognizeBothSidesAndPositivePadding) {
  PictureRows({{R"PICTURE(Parse("(zzz","(123"))PICTURE", u"-123"}, {R"PICTURE(Parse("zzz)","123)"))PICTURE", u"-123"},
    {R"PICTURE(Parse("(zzz)","(123)"))PICTURE", u"-123"}, {R"PICTURE(Parse("(zzz)"," 123 "))PICTURE", u"123"},
    {R"PICTURE(Parse("zzz.z(","123.5("))PICTURE", u"-123.5"}, {R"PICTURE(Parse("zzz.z)","123.5 "))PICTURE", u"123.5"}});
}
TEST(CoverageXfaPictureModels, CreditAndDebitSuffixCaseVariantsParseNegativeAndPositiveAmounts) {
  PictureRows({{R"PICTURE(Parse("999CR","123CR"))PICTURE", u"-123"}, {R"PICTURE(Parse("999CR","123  "))PICTURE", u"123"},
    {R"PICTURE(Parse("999db","123DB"))PICTURE", u"-123"}, {R"PICTURE(Parse("999db","123"))PICTURE", u"123"},
    {R"PICTURE(Parse("999.9db","123.5DB"))PICTURE", u"-123.5"}, {R"PICTURE(Parse("999.9cr","123.5"))PICTURE", u"123.5"}});
}
TEST(CoverageXfaPictureModels, DecimalGroupingAndUnlimitedFractionDigitsKeepTheDecimalQuantity) {
  PictureRows({{R"PICTURE(Parse("zzz,zz9.88888888","12,345.6789"))PICTURE", u"12345.6789"},
    {R"PICTURE(Parse("zzz,zz9.8","12,345.0000"))PICTURE", u"12345"},
    {R"PICTURE(Parse("zzz.zzz,z","123.545,4"))PICTURE", u"123.5454"},
    {R"PICTURE(Parse("zzz,zz9.8888","123,456.0"))PICTURE", u"123456"}});
}
TEST(CoverageXfaPictureModels, PercentAndCurrencyDecorationsAreConsumedOnlyAtTheirPicturePosition) {
  PictureRows({{R"PICTURE(Parse("zz%","12%"))PICTURE", u"0.12"}, {R"PICTURE(Parse("zzz.zz$","150.50$"))PICTURE", u"150.5"},
    {R"PICTURE(Parse("$zz9.99","$12.50"))PICTURE", u"12.5"}, {R"PICTURE(Parse("$zz9.99","12.50$"))PICTURE", u""},
    {R"PICTURE(Parse("zz%","%12"))PICTURE", u""}});
}
TEST(CoverageXfaPictureModels, MissingMandatoryDigitsAndMismatchedLiteralsRejectTheFormattedInput) {
  PictureRows({{R"PICTURE(Parse("num{999}","1x2"))PICTURE", u""}, {R"PICTURE(Parse("num{99.99}","12.3x"))PICTURE", u""},
    {R"PICTURE(Parse("num{'USD '999}","EUR 123"))PICTURE", u""}, {R"PICTURE(Parse("num{S999}","x123"))PICTURE", u""},
    {R"PICTURE(Parse("num{999CR}","123XY"))PICTURE", u""}, {R"PICTURE(Parse("num{999DB}","123XX"))PICTURE", u""}});
}
TEST(CoverageXfaPictureModels, EmptyPicturesAndExplicitZeroExponentsUseTheirDeclaredGrammar) {
  PictureRows({{R"PICTURE(Parse("num{}","abc"))PICTURE", u""}, {R"PICTURE(Parse("num{99E}","12E+0"))PICTURE", u"12"},
    {R"PICTURE(Parse("num{99.9}",""))PICTURE", u""}, {R"PICTURE(Parse("num{'tag'99}","xx12"))PICTURE", u""}});
}
TEST(CoverageXfaPictureModels, LowercaseCreditAndDebitFormattingAndParenthesesKeepSignSemantics) {
  PictureRows({{R"PICTURE(Format("num{zz9cr}",-12))PICTURE", u"12CR"}, {R"PICTURE(Format("num{zz9db}",-12))PICTURE", u"12db"},
    {R"PICTURE(Format("num{zz9CR}",12))PICTURE", u"12  "}, {R"PICTURE(Format("num{zz9DB}",12))PICTURE", u"12  "},
    {R"PICTURE(Format("num{(zz9)}",-12))PICTURE", u"(12)"}, {R"PICTURE(Format("num{(zz9)}",12))PICTURE", u" 12 "}});
}
TEST(CoverageXfaPictureModels, QuotedNumericPrefixesSuffixesAndBarsRemainLiteral) {
  PictureRows({{R"PICTURE(Format("num{'USD 'zz9.99}",12.5))PICTURE", u"USD 12.50"},
    {R"PICTURE(Format("num{zz9.99' net'}",12.5))PICTURE", u"12.50 net"},
    {R"PICTURE(Format("num{'a|b 'zz9}",12))PICTURE", u"a|b 12"},
    {R"PICTURE(Parse("num{'a|b 'zz9}","a|b 12"))PICTURE", u"12"}});
}
TEST(CoverageXfaPictureModels, UnicodeEscapedLiteralDigitsLettersAndSymbolsAreDecodedByThePictureGrammar) {
  PictureRows({{R"PICTURE(Format("text{'\\u0041'9}","5"))PICTURE", u"A5"},
    {R"PICTURE(Format("text{'\\u0061'9}","5"))PICTURE", u"a5"},
    {R"PICTURE(Format("text{'\\u00A9'9}","5"))PICTURE", u"\u00a95"},
    {R"PICTURE(Parse("text{'\\u0041'9}","A5"))PICTURE", u"5"}});
}
TEST(CoverageXfaPictureModels, TextMaskFailuresDistinguishLettersDigitsAlphanumericsAndExactLength) {
  PictureRows({{R"PICTURE(Format("text{AA}","A1"))PICTURE", u""}, {R"PICTURE(Format("text{99}","1A"))PICTURE", u""},
    {R"PICTURE(Format("text{OO}","A#"))PICTURE", u""}, {R"PICTURE(Format("text{AA}","ABC"))PICTURE", u""},
    {R"PICTURE(Format("text{AAA}","AB"))PICTURE", u""}, {R"PICTURE(Parse("text{A'-'9}","A+1"))PICTURE", u""}});
}
TEST(CoverageXfaPictureModels, TextMaskQuotedPrefixesSuffixesAndSeparatorsAreRemovedOnParse) {
  PictureRows({{R"PICTURE(Parse("text{'ID:'AA-99}","ID:AB-12"))PICTURE", u"AB12"},
    {R"PICTURE(Format("text{'ID:'AA-99}","AB12"))PICTURE", u"ID:AB-12"},
    {R"PICTURE(Parse("text{XX'|end'}","a#|end"))PICTURE", u"a#"},
    {R"PICTURE(Format("text{XX'|end'}","a#"))PICTURE", u"a#|end"}});
}
TEST(CoverageXfaPictureModels, NullAndZeroAlternativesUseTheirOwnLiteralClauses) {
  PictureRows({{R"PICTURE(Format("num{}|zero{'ZERO'}","ZERO"))PICTURE", u"ZERO"},
    {R"PICTURE(Format("num{}|null{'EMPTY'}","EMPTY"))PICTURE", u"EMPTY"},
    {R"PICTURE(Parse("num{}|zero{'ZERO'}","ZERO"))PICTURE", u"0"},
    {R"PICTURE(Parse("num{}|null{'EMPTY'}","EMPTY"))PICTURE", u""}});
}
TEST(CoverageXfaPictureModels, LiteralAlternativeMismatchesAndQuotedBarsRetainTheirExactCategory) {
  PictureRows({{R"PICTURE(Format("num{}|zero{'No|value'}","No|value"))PICTURE", u"No|value"},
    {R"PICTURE(Parse("num{}|zero{'ZERO'}","ZERX"))PICTURE", u""},
    {R"PICTURE(Parse("num{}|null{'EMPTY'}","EMPTX"))PICTURE", u""},
    {R"PICTURE(Format("num{}|null{'No|data'}","No|data"))PICTURE", u"No|data"}});
}
TEST(CoverageXfaPictureModels, DateParsingConsumesWeekdayNamesWithoutChangingTheSpecifiedDate) {
  PictureRows({{R"PICTURE(Parse("date{YYYY-MM-DD EEE}","2000-01-01 Sat"))PICTURE", u"2000-01-01"},
    {R"PICTURE(Parse("date{EEEE, MMMM D, YYYY}","Saturday, January 1, 2000"))PICTURE", u"2000-01-01"},
    {R"PICTURE(Parse("date{YYYY-MM-DD EEEE}","2000-01-01 Noday"))PICTURE", u""}});
}
TEST(CoverageXfaPictureModels, DateParsingConsumesEraAndOrdinalFieldsAlongsideTheActualCalendarDate) {
  PictureRows({{R"PICTURE(Parse("date{YYYY-MM-DD G}","2000-02-29 AD"))PICTURE", u"2000-02-29"},
    {R"PICTURE(Parse("date{YYYY-MM-DD JJJ}","2000-02-29 060"))PICTURE", u"2000-02-29"},
    {R"PICTURE(Parse("date{YYYY-MM-DD J}","2000-12-31 366"))PICTURE", u"2000-12-31"},
    {R"PICTURE(Parse("date{YYYY-MM-DD G}","1900-01-01 AD"))PICTURE", u"1900-01-01"}});
}
TEST(CoverageXfaPictureModels, DateParsingRejectsMissingDigitsBadNamesAndLiteralMismatches) {
  PictureRows({{R"PICTURE(Parse("date{YYYY-MM-DD}","20-01-01"))PICTURE", u""},
    {R"PICTURE(Parse("date{YYYY-MM-DD}","2000-0x-01"))PICTURE", u""},
    {R"PICTURE(Parse("date{YYYY MMMM DD}","2000 Notamonth 01"))PICTURE", u""},
    {R"PICTURE(Parse("date{YYYY' year 'MM' month 'DD}","2000 year 02 wrong 29"))PICTURE", u""}});
}
TEST(CoverageXfaPictureModels, TwoDigitDateYearsUseTheDocumentCenturyWindow) {
  PictureRows({{R"PICTURE(Parse("date{MM/DD/YY}","01/02/00"))PICTURE", u"2000-01-02"},
    {R"PICTURE(Parse("date{MM/DD/YY}","01/02/29"))PICTURE", u"2029-01-02"},
    {R"PICTURE(Parse("date{MM/DD/YY}","01/02/30"))PICTURE", u"1930-01-02"},
    {R"PICTURE(Parse("date{MM/DD/YY}","01/02/99"))PICTURE", u"1999-01-02"}});
}
TEST(CoverageXfaPictureModels, TimeParsingHandlesNoonMidnightAndExplicitAmPmNames) {
  PictureRows({{R"PICTURE(Parse("time{h:MM A}","12:30 AM"))PICTURE", u"00:30:00"},
    {R"PICTURE(Parse("time{h:MM A}","12:30 PM"))PICTURE", u"12:30:00"},
    {R"PICTURE(Parse("time{hh:MM:SS A}","01:02:03 PM"))PICTURE", u"13:02:03"},
    {R"PICTURE(Parse("time{h:MM A}","1:30 AM"))PICTURE", u"01:30:00"}});
}
TEST(CoverageXfaPictureModels, TimeParsingUsesQuotedLiteralsAndTheTwentyFourHourToken) {
  PictureRows({{R"PICTURE(Parse("time{HH:MM:SS}","23:59:58"))PICTURE", u"23:59:58"},
    {R"PICTURE(Parse("time{K:MM:SS}","24:00:00"))PICTURE", u"00:00:00"},
    {R"PICTURE(Parse("time{H.MM 'Uhr'}","12.59 Uhr"))PICTURE", u"12:59:00"},
    {R"PICTURE(Parse("time{HH:MM' end'}","12:30 wrong"))PICTURE", u""}});
}
TEST(CoverageXfaPictureModels, TimeParsingRejectsTruncatedAndNonNumericClockFields) {
  PictureRows({{R"PICTURE(Parse("time{HH:MM:SS}","12:3"))PICTURE", u""},
    {R"PICTURE(Parse("time{HH:MM:SS}","12:30:xx"))PICTURE", u""},
    {R"PICTURE(Parse("time{HH:MM Z}","xx:30 GMT"))PICTURE", u""},
    {R"PICTURE(Parse("time{HH:MM z}","12:xx +0200"))PICTURE", u""}});
}
// Time2Num returns an absolute millisecond-of-day value. Explicit input zones
// cancel the existing host-local conversion, so these expectations hold across
// native and Boa local timezones without replacing either clock or zone logic.
TEST(CoverageXfaPictureModels, ExplicitNumericZoneHoursAndMinutesConvertToTheExpectedUtcTimeNumber) {
  PictureRows({{R"PICTURE(Time2Num("13:00:00 GMT+02:30","HH:MM:SS Z"))PICTURE", u"37800001"},
    {R"PICTURE(Time2Num("01:00:00 GMT-0530","HH:MM:SS Z"))PICTURE", u"23400001"},
    {R"PICTURE(Time2Num("13:00:00 GMT+02","HH:MM:SS Z"))PICTURE", u"39600001"},
    {R"PICTURE(Time2Num("01:00:00 GMT+23:00","HH:MM:SS Z"))PICTURE", u"7200001"}});
}
TEST(CoverageXfaPictureModels, CanonicalZoneLetterAndSignedOffsetsUseTheSharedUtcTimeNumber) {
  // Native ParseLocaleTime consumes canonical Z without ResolveZone, then
  // Time2Num subtracts the CRT standard local offset. Rust correctly treats
  // Z as UTC. Compute the native defect independently from the host CRT;
  // explicit numeric zones below remain UTC and shared by both engines.
#if defined(_WIN32)
  _tzset();
  const long seconds_west = _timezone;
#else
  tzset();
  const long seconds_west = timezone;
#endif
  const long minutes = ((13 * 60 + seconds_west / 60) % 1440 + 1440) % 1440;
  const auto native_ascii = std::to_string(minutes * 60000 + 1);
  const std::u16string native_value(native_ascii.begin(), native_ascii.end());
  PictureRows({{R"PICTURE(Time2Num("13:00:00 Z","HH:MM:SS z"))PICTURE", u"46800001", false, false, nullptr, native_value.c_str()},
    {R"PICTURE(Time2Num("13:00:00 +02:30","HH:MM:SS z"))PICTURE", u"37800001"},
    {R"PICTURE(Time2Num("13:00:00 -05:30","HH:MM:SS z"))PICTURE", u"66600001"},
    {R"PICTURE(Time2Num("13:00:00 +0200","HH:MM:SS z"))PICTURE", u"39600001"}});
}
TEST(CoverageXfaPictureModels, GmtTimeFormattingPreservesWholeSecondsAcrossTheDay) {
  PictureRows({{R"PICTURE(Num2GMTime(1,"HH:MM:SS","en_US"))PICTURE", u"00:00:00"},
    {R"PICTURE(Num2GMTime(3600001,"HH:MM:SS","en_US"))PICTURE", u"01:00:00"},
    {R"PICTURE(Num2GMTime(43200001,"HH:MM:SS","en_US"))PICTURE", u"12:00:00"},
    {R"PICTURE(Num2GMTime(86399001,"HH:MM:SS","en_US"))PICTURE", u"23:59:59"}});
}
TEST(CoverageXfaPictureModels, TimeNumberOptionalNullArgumentsAndLocaleFallbacksRemainDistinct) {
  PictureRows({{R"PICTURE(HasValue(Time2Num("12:00",null)))PICTURE", u"0"},
    {R"PICTURE(HasValue(Time2Num("12:00","HH:MM",null)))PICTURE", u"0"},
    {R"PICTURE(HasValue(Num2GMTime(null)))PICTURE", u"0"}, {R"PICTURE(HasValue(Num2GMTime(0)))PICTURE", u"0"},
    {R"PICTURE(Num2GMTime(1,"HH:MM","zz_missing"))PICTURE", u"00:00"},
    {R"PICTURE(Num2GMTime(1,"HH:MM","x"))PICTURE", u""}});
}
TEST(CoverageXfaPictureModels, DateNumberOptionalNullArgumentsAndInvalidOrdinalsDoNotBecomeDates) {
  PictureRows({{R"PICTURE(HasValue(Date2Num("1/1/1900",null)))PICTURE", u"0"},
    {R"PICTURE(HasValue(Date2Num("1/1/1900","D/M/YYYY",null)))PICTURE", u"0"},
    {R"PICTURE(HasValue(Num2Date(1,null)))PICTURE", u"0"}, {R"PICTURE(HasValue(Num2Date(1,"YYYY",null)))PICTURE", u"0"},
    {R"PICTURE(HasValue(Num2Date(0)))PICTURE", u"0"}, {R"PICTURE(Num2Date(1,"YYYY","zz_missing"))PICTURE", u"1900"},
    {R"PICTURE(Num2Date(1,"YYYY","x"))PICTURE", u""}});
}
TEST(CoverageXfaPictureModels, LocalizedTimePictureServicesExposeTheExistingLocaleSymbolAlphabet) {
  PictureRows({{R"PICTURE(LocalTimeFmt(1,"de_DE"))PICTURE", u"hh:mm"},
    {R"PICTURE(LocalTimeFmt(2,"fr_CA"))PICTURE", u"hh:mm:ss"},
    {R"PICTURE(LocalTimeFmt(3,"de_CH"))PICTURE", u"hh:mm:ss z"},
    {R"PICTURE(LocalTimeFmt(4,"fr_FR"))PICTURE", u"hh h mm z"}});
}
TEST(CoverageXfaPictureModels, PictureStyleOutOfRangeValuesFallBackToTheLocaleDefault) {
  PictureRows({{R"PICTURE(DateFmt(-1,"en_US"))PICTURE", u"MMM D, YYYY"},
    {R"PICTURE(DateFmt(5,"en_US"))PICTURE", u"MMM D, YYYY"},
    {R"PICTURE(TimeFmt(-1,"en_US"))PICTURE", u"h:MM:SS A"},
    {R"PICTURE(TimeFmt(5,"en_US"))PICTURE", u"h:MM:SS A"}});
}
TEST(CoverageXfaPictureModels, LocalizedPictureServicesDistinguishNullFallbackAndShortLocaleNames) {
  PictureRows({{R"PICTURE(HasValue(LocalTimeFmt(null)))PICTURE", u"0"},
    {R"PICTURE(HasValue(LocalTimeFmt(1,null)))PICTURE", u"0"},
    {R"PICTURE(LocalTimeFmt(2,"zz_missing"))PICTURE", u"h:mm:ss a"},
    {R"PICTURE(LocalTimeFmt(2,"x"))PICTURE", u""},
    {R"PICTURE(HasValue(TimeFmt(1,null)))PICTURE", u"0"}});
}
TEST(CoverageXfaPictureModels, ExplicitTimeThenDatePicturesParseBothClockAndCalendarComponents) {
  PictureRows({{R"PICTURE(Parse("time{HH:MM:SS} date{YYYY-MM-DD}","13:04:05 2000-02-29"))PICTURE", u"2000-02-29T13:04:05"},
    {R"PICTURE(Parse("time{HH:MM:SS}'T'date{YYYY-MM-DD}","13:04:05T2000-02-29"))PICTURE", u"2000-02-29T13:04:05"},
    {R"PICTURE(Parse("time{HH:MM:SS} date{YYYY-MM-DD}","xx:04:05 2000-02-29"))PICTURE", u""}});
}

// These cases exercise actual field pictures, their script getters/setters,
// widget focus transitions and validation in addition to scalar FormCalc.
TEST(CoverageXfaPictureModels, NumericFieldDisplayAndEditPicturesRemainIndependentOfCanonicalRawValue) {
  PictureRows({{R"PICTURE(var f=r.input;f.format.picture.value="num{$z,zz9.99}";f.ui.picture.value="num{zzzz9.99}";
    f.rawValue="1234.5";return f.formattedValue+"|"+f.editValue+"|"+Number(f.rawValue);)PICTURE",
    u"$1,234.50|1234.50|1234.5", true, true}});
}
TEST(CoverageXfaPictureModels, NumericPictureSettersParseTheirOwnDecoratedValuesBeforeRendering) {
  PictureRows({{R"PICTURE(var f=r.input;f.format.picture.value="num{$z,zz9.99}";f.ui.picture.value="num{zzzz9.99}";
    f.editValue="42.25";var first=Number(f.rawValue);f.formattedValue="$34.50";
    return first+"|"+Number(f.rawValue)+"|"+f.formattedValue;)PICTURE", u"42.25|34.5|$34.50", true, true}});
}
TEST(CoverageXfaPictureModels, TextFieldDisplayMasksDoNotChangeTheEditOrRawValue) {
  PictureRows({{R"PICTURE(var f=r.input;f.format.picture.value="text{AA-99}";f.ui.picture.value="text{AA99}";
    f.rawValue="AB12";return f.formattedValue+"|"+f.editValue+"|"+f.rawValue;)PICTURE",
    u"AB-12|AB12|AB12", true}});
}
TEST(CoverageXfaPictureModels, TextFieldEditAndDisplayAssignmentsRemoveOnlyTheirOwnMaskLiterals) {
  PictureRows({{R"PICTURE(var f=r.input;f.format.picture.value="text{'ID 'AA99}";f.ui.picture.value="text{AA'-'99}";
    f.editValue="AB-12";var first=f.rawValue;f.formattedValue="ID CD34";
    return first+"|"+f.rawValue+"|"+f.formattedValue;)PICTURE", u"AB12|CD34|ID CD34", true}});
}
TEST(CoverageXfaPictureModels, ActualFieldValidationReportsThePictureFailureAndAcceptsCorrectedRawValue) {
  PictureRows({{R"PICTURE(var f=r.input;f.formatMessage="Two digits";f.validate.formatTest="error";
    f.validate.nullTest="disabled";f.validate.picture.value="num{99}";
    f.rawValue="12";var initial=f.execValidate();
    f.rawValue="bad";var bad=f.execValidate();f.rawValue="12";var good=f.execValidate();
    return String(initial)+"|"+String(bad)+"|"+String(good)+"|"+Number(f.rawValue);)PICTURE", u"true|false|true|12", true, true, u"Two digits"}});
}

}  // namespace
}  // namespace pdfoxium::integration
