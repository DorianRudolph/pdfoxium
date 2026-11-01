#include "xfa_form_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <functional>
#include <initializer_list>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using Inspect = std::function<void(Session&)>;

// XFA script definitions are embedded because the public API cannot create
// them. Execution, focus, state inspection, rendering and saving use that API.
std::string Field(const std::string& name, const std::string& value, int y,
                  const std::string& children = "", const std::string& ui = "<textEdit/>") {
  return "<field name=\"" + name + "\" x=\"8pt\" y=\"" + std::to_string(y) +
      "pt\" w=\"220pt\" h=\"24pt\"><bind match=\"none\"/><font typeface=\"Helvetica\" size=\"12pt\"/>"
      "<ui>" + ui + "</ui>" +
      (children.find("<value>") == 0 ? "" : "<value><text>" + value + "</text></value>") + children +
      forms::Event("var v=this.rawValue;if(typeof v==='string'&&v.charCodeAt(0)===10)this.rawValue=v.slice(1);", "initialize") + "</field>";
}
std::string Checked(const std::string& body) {
  return "function eq(a,b){if(a!==b)throw 'expected '+String(b)+' got '+String(a);}"
      "function yes(a){if(!a)throw 'assertion';}"
      "function clean(a){return String(a===null?'':a).replace(/^\\n/,'');}"
      "function throws(f){var caught=false;try{f();}catch(e){caught=true;}yes(caught);}"
      "this.caption.value.text.value='Run';try{var r=xfa.form.root;" + body + ";r.report.rawValue='PASS';}"
      "catch(e){xfa.form.root.report.rawValue='FAIL';xfa.host.messageBox('FAIL:'+String(e));}";
}
void AlignXml(std::string& xml, size_t prefix) {
  // Keep the separately tested native 1024-character CDATA-plane defect out
  // of semantic event fixtures without altering their scripts or attributes.
  for (size_t i = 0; i < xml.size();) {
    const size_t n = xml.compare(i, 9, "<![CDATA[") == 0 ? 9 : xml.compare(i, 3, "]]>") == 0 ? 3 : 0;
    if (!n) { ++i; continue; }
    const auto offset = prefix + i;
    if (offset / 1024 != (offset + n - 1) / 1024) {
      const size_t pad = 1024 - offset % 1024; xml.insert(i, pad, ' '); i += pad;
    }
    i += n;
  }
}
std::vector<uint8_t> Fixture(const std::string& program, const std::string& model,
                           const std::string& language = "application/x-javascript",
                           const std::string& version = "3.3") {
  const std::string preamble = "<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
  const std::string config = "<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
  std::string xml = "<template xmlns=\"http://www.xfa.org/schema/xfa-template/" + version + "/\">"
      "<subform name=\"root\" layout=\"position\" locale=\"en_US\" restoreState=\"auto\">"
      "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"260pt\" long=\"320pt\"/><contentArea w=\"260pt\" h=\"320pt\"/></pageArea></pageSet>" +
      Field("report", "Ready", 8) + Field("audit", "Ready", 160) +
      "<field name=\"run\" x=\"8pt\" y=\"48pt\" w=\"80pt\" h=\"24pt\"><bind match=\"none\"/><ui><button/></ui>"
      "<font typeface=\"Helvetica\" size=\"10pt\"/><caption><value><text>Run</text></value></caption>" +
      forms::Event(language == "application/x-javascript" ? Checked(program) : program, "click", language) +
      forms::Event("this.caption.value.text.value='Run';", "initialize") + "</field>" + model +
      "<draw x=\"232pt\" y=\"280pt\" w=\"20pt\" h=\"20pt\"><value><rectangle><fill><color value=\"0,255,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>"
      "</subform></template>";
  AlignXml(xml, preamble.size() + config.size());
  std::vector<std::string> objects{
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 260 320] /Resources << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("", preamble), PdfStream("", config), PdfStream("", xml),
      PdfStream("", "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root><group flag=\"old\"><item>A</item><item>B</item></group></root></xfa:data></xfa:datasets>"),
      PdfStream("", "</xdp:xdp>"),
      "null"};  // Object 10 is reserved; pinned font objects are 11..13.
  auto font = forms::PinnedXfaArimoFontObjects();
  objects.push_back(std::move(font[0]));
  objects.push_back(std::move(font[1]));
  objects.push_back(std::move(font[2]));
  return PdfFixture(objects);
}
void Configure(forms::FormHost& h, int answer) {
  h.alert_result = answer;
  h.FFI_GetPageViewRect = [](FPDF_FORMFILLINFO* info, FPDF_PAGE page, double* l, double* t, double* r, double* b) {
    auto& host = forms::FormHost::Self(info); host.CheckPage(page);
    *l = 0; *t = 0; *r = host.api.FPDF_GetPageWidthF(page); *b = host.api.FPDF_GetPageHeightF(page);
  };
}
void Blur(Session& s) {
  s.api.FORM_ForceToKillFocus(s.form); s.Click(12, 18);
  s.api.FORM_ForceToKillFocus(s.form); s.Click(250, 310);
  s.api.FORM_ForceToKillFocus(s.form); s.api.FORM_OnMouseMove(s.form, s.page, 0, 250, 310);
}
void Execute(Session& s, const std::u16string& expected, const Inspect& inspect,
             const std::u16string& marker = u"Ready") {
  s.host->alerts.clear();
  ASSERT_TRUE(s.Focus(12, 18)); ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page)); s.Replace(marker);
  Blur(s); s.Click(12, 58); ASSERT_TRUE(s.Focus(12, 18)); EXPECT_EQ(s.Text(), expected);
  for (const auto& alert : s.host->alerts) {
    EXPECT_NE(std::get<0>(alert).substr(0, 5), u"FAIL:")
        << std::string(std::get<0>(alert).begin(), std::get<0>(alert).end());
  }
  if (inspect) { inspect(s); }
  Blur(s); EXPECT_TRUE(s.host->callbacks_valid); EXPECT_FALSE(s.host->callback_error);
}
void Compare(const Raster& a, const Raster& b, Artifacts& artifacts, const std::string& label) {
  CompareRaster(a, b, artifacts, label, RasterTolerance::ConnectedError(1.0));
  CompareRaster(CropLogicalRaster(a, 232, 280, 20, 20), CropLogicalRaster(b, 232, 280, 20, 20), artifacts, label + "-landmark");
  CompareRaster(CropLogicalRaster(a, 8, 8, 220, 24), CropLogicalRaster(b, 8, 8, 220, 24), artifacts, label + "-report", RasterTolerance::ConnectedError(1.0));
}

struct ScalarRow {
  const char* expression;
  const char* expected;
  const char* native_expected = nullptr;
};

// These tables execute ordinary, published FormCalc functions. The compiler's
// private expansion helpers are reached only by real node/attribute arguments.
void ScalarProgram(const std::string& program, const std::u16string& audit,
                   const std::u16string& report = u"PASS",
                   bool distinguish_reader = false) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto bytes = Fixture(program,
      Field("amount", "5", 100, "<value><integer>5</integer></value>"),
      "application/x-formcalc");
  Artifacts artifacts;
  std::array<Raster, 2> originals;
  size_t index = 0;
  auto execute = [&](Session& session, const Library& reader) {
    ASSERT_TRUE(session.Focus(12, 170));
    ASSERT_TRUE(session.api.FORM_SelectAllText(session.form, session.page));
    session.Replace(u"Ready");
    Blur(session);
    ASSERT_NO_FATAL_FAILURE(Execute(session, report, [&](Session& current) {
      ASSERT_TRUE(current.Focus(12, 170));
      EXPECT_EQ(current.Text(), audit);
    }, distinguish_reader ? (reader.name() == "native" ? u"native" : u"rust") : u"Ready"));
  };
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    Session session(writer->api(), bytes, true, true, 2,
        [&](forms::FormHost& host) { Configure(host, JSPLATFORM_ALERT_RETURN_YES); });
    ASSERT_TRUE(session.valid());
    ASSERT_TRUE(session.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(execute(session, *writer));
    originals[index] = session.DrawForComparison(260, 320);
    forms::ExpectInk(originals[index], 8, 160, 228, 184);
    const auto saved = SaveDocument(session.api, session.document, artifacts, writer->name() + "-scalar-constraints");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      Session loaded(reader->api(), saved, true, true, 2,
          [&](forms::FormHost& host) { Configure(host, JSPLATFORM_ALERT_RETURN_YES); });
      ASSERT_TRUE(loaded.valid());
      ASSERT_TRUE(loaded.xfa_loaded);
      ASSERT_NO_FATAL_FAILURE(execute(loaded, *reader));
      Compare(originals[index], loaded.DrawForComparison(260, 320), artifacts,
          writer->name() + "-" + reader->name());
    }
    ++index;
  }
  Compare(originals[0], originals[1], artifacts, "scalar-constraint-engines");
}

void ScalarRows(std::initializer_list<ScalarRow> rows) {
  std::string program = "var passed=0\n";
  bool distinguish_reader = false;
  for (const auto& row : rows) {
    auto check = [&](const char* expected) {
      return "if ((" + std::string(row.expression) + ") eq (" + expected +
          ")) then passed=passed+1 endif\n";
    };
    if (row.native_expected) {
      distinguish_reader = true;
      program += "if(root.report.rawValue eq \"native\")then\n" + check(row.native_expected) +
          "else\n" + check(row.expected) + "endif\n";
    } else {
      program += check(row.expected);
    }
  }
  program += "root.audit.rawValue=passed\nif(passed eq " + std::to_string(rows.size()) +
      ")then root.report.rawValue=\"PASS\" else root.report.rawValue=\"FAIL\" endif";
  const auto count = std::to_string(rows.size());
  ScalarProgram(program, std::u16string(count.begin(), count.end()), u"PASS", distinguish_reader);
}

TEST(CoverageFormCalcScalarConstraints, EmptyNeedlesAndNullConcatenandsKeepDistinctResults) {
  ScalarRows({
      {R"FC(At("abc",""))FC", R"FC(1)FC"},
      {R"FC(At("",""))FC", R"FC(1)FC"},
      {R"FC(Concat(null,null))FC", R"FC(null)FC"},
      {R"FC(Concat(null,"x",null))FC", R"FC("x")FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, NullStringSourcesPropagateThroughTextOperations) {
  ScalarRows({
      {R"FC(At(null,"x"))FC", R"FC(null)FC"},
      {R"FC(Len(null))FC", R"FC(null)FC"},
      {R"FC(Left(null,2))FC", R"FC(null)FC"},
      {R"FC(Right(null,2))FC", R"FC(null)FC"},
      {R"FC(Lower(null))FC", R"FC(null)FC"},
      {R"FC(Upper(null))FC", R"FC(null)FC"},
      {R"FC(Ltrim(null))FC", R"FC(null)FC"},
      {R"FC(Rtrim(null))FC", R"FC(null)FC"},
      {R"FC(Space(null))FC", R"FC(null)FC"},
      {R"FC(Stuff(null,1,1))FC", R"FC(null)FC"},
      {R"FC(Substr(null,1,1))FC", R"FC(null)FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, OptionalNullParametersRetainTheirDocumentedResult) {
  ScalarRows({
      {R"FC(WordNum(1,null))FC", R"FC(null)FC"},
      {R"FC(WordNum(1,0,null))FC", R"FC(null)FC"},
      {R"FC(Stuff("abc",null,1))FC", R"FC(null)FC"},
      {R"FC(Substr("abc",1,null))FC", R"FC(null)FC"},
      {R"FC(Right("abc",null))FC", R"FC(null)FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, ReplaceNullAndEmptyNeedlesDoNotInventReplacementText) {
  ScalarRows({
      {R"FC(Replace("abc",""))FC", R"FC("abc")FC"},
      {R"FC(Replace(null,"a"))FC", R"FC("")FC"},
      {R"FC(Replace("abc",null))FC", R"FC("")FC"},
      {R"FC(Replace("aba","a"))FC", R"FC("b")FC"},
      {R"FC(Replace("aba","a",null))FC", R"FC("b")FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, SubstringIndicesAcceptNumericTextAndRejectOutOfRangePositions) {
  ScalarRows({
      {R"FC(Substr("abcdef","2","3"))FC", R"FC("bcd")FC"},
      {R"FC(Left("abcdef","2"))FC", R"FC("ab")FC"},
      {R"FC(Right("abcdef","2"))FC", R"FC("ef")FC"},
      {R"FC(Substr("",1,3))FC", R"FC("")FC"},
      {R"FC(Substr("abc",8,2))FC", R"FC("")FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, LatinExtendedCasePairsUseTheExistingCharacterMapping) {
  ScalarRows({
      {R"FC(Upper("āăą"))FC", R"FC("ĀĂĄ")FC"},
      {R"FC(Lower("ĀĂĄ"))FC", R"FC("āăą")FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, UrlEncodingPreservesTheDocumentedWideCodepointBytePairs) {
  ScalarRows({
      {R"FC(Encode("Ā","url"))FC", R"FC("%01%00")FC"},
      {R"FC(Encode("一","url"))FC", R"FC("%4e%00")FC"},
      {R"FC(Encode("é","url"))FC", R"FC("%e9")FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, XmlEncodingUsesNamedQuotesAndWideNumericEntities) {
  ScalarRows({
      {R"FC(Encode("""","xml"))FC", R"FC("&quot;")FC"},
      {R"FC(Encode("'","xml"))FC", R"FC("&apos;")FC"},
      {R"FC(Encode("Ā","xml"))FC", R"FC("&#x0100;")FC"},
      {R"FC(Encode("一","xml"))FC", R"FC("&#x4e00;")FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, StrPadsNonFiniteFloatConversionsWithoutASecondDecimalPass) {
  ScalarRows({
      {R"FC(Str(1e39,8,0))FC", R"FC("     inf")FC"},
      {R"FC(Str(-1e39,8,0))FC", R"FC("    -inf")FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, StrWidthStillBoundsNonFiniteFloatText) {
  ScalarRows({
      {R"FC(Str(1e39,2,0))FC", R"FC("**")FC"},
      {R"FC(Str(-1e39,3,0))FC", R"FC("***")FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, WordNumKeepsEveryGroupAcrossTheTrillionBoundary) {
  ScalarRows({
      {R"FC(WordNum(1099511627776))FC", R"FC("One Trillion Ninety-nine Billion Five Hundred Eleven Million Six Hundred Twenty-seven Thousand Seven Hundred Seventy-six")FC"},
      {R"FC(WordNum(4294967296))FC", R"FC("Four Billion Two Hundred Ninety-four Million Nine Hundred Sixty-seven Thousand Two Hundred Ninety-six")FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, EmptyAggregatesAndNullNumericInputsRemainNull) {
  ScalarRows({
      {R"FC(Max(null))FC", R"FC(null)FC"},
      {R"FC(Min(null))FC", R"FC(null)FC"},
      {R"FC(Sum(null))FC", R"FC(null)FC"},
      {R"FC(Ceil(null))FC", R"FC(null)FC"},
      {R"FC(Floor(null))FC", R"FC(null)FC"},
      {R"FC(Mod(null,2))FC", R"FC(null)FC"},
      {R"FC(Round(1,null))FC", R"FC(null)FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, FinancialNullInputsDoNotEnterIterativeCalculations) {
  ScalarRows({
      {R"FC(Apr(null,1,2))FC", R"FC(null)FC"},
      {R"FC(CTerm(null,2,1))FC", R"FC(null)FC"},
      {R"FC(FV(null,0.1,2))FC", R"FC(null)FC"},
      {R"FC(IPmt(null,0.1,1,1,1))FC", R"FC(null)FC"},
      {R"FC(PPmt(null,0.1,1,1,1))FC", R"FC(null)FC"},
      {R"FC(NPV(null,1))FC", R"FC(null)FC"},
      {R"FC(NPV(0.1,null))FC", R"FC(null)FC"},
      {R"FC(Pmt(null,0.1,2))FC", R"FC(null)FC"},
      {R"FC(PV(null,0.1,2))FC", R"FC(null)FC"},
      {R"FC(Rate(null,1,2))FC", R"FC(null)FC"},
      {R"FC(Term(null,0.1,2))FC", R"FC(null)FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, FutureValueZeroRateUsesTheUndiscountedPaymentTotal) {
  ScalarRows({
      {R"FC(FV(100,0,12))FC", R"FC(1200)FC"},
      {R"FC(FV(17,0,3))FC", R"FC(51)FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, OrderedStringsAndNullOperandsRetainComparisonRules) {
  ScalarRows({
      {R"FC("a" le "b")FC", R"FC(1)FC"},
      {R"FC("b" gt "a")FC", R"FC(1)FC"},
      {R"FC("b" ge "b")FC", R"FC(1)FC"},
      {R"FC(null le null)FC", R"FC(1)FC"},
      {R"FC(null ge null)FC", R"FC(1)FC"},
      {R"FC(null lt null)FC", R"FC(0)FC"},
      {R"FC(null gt 1)FC", R"FC(0)FC"},
      {R"FC(+null)FC", R"FC(null)FC"},
      {R"FC(-null)FC", R"FC(null)FC"},
      {R"FC(not null)FC", R"FC(null)FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, NamedAttributeExpansionsFeedModuloAndRounding) {
  ScalarRows({
      {R"FC(Mod(root.amount.rawValue,2))FC", R"FC(1)FC"},
      {R"FC(Round(root.amount.rawValue,0))FC", R"FC(5)FC"},
      {R"FC(Mod(root.amount,2))FC", R"FC(1)FC"},
      {R"FC(Round(root.amount,0))FC", R"FC(5)FC"},
  });
}

// Native skips the separator check before the day pair, accepts trailing text
// after Z, and accepts terminal clock/zone colons. Its scalar1 observations are
// explicit boundaries; Rust must reject each malformed ISO value with scalar0.
// A public field marker identifies the actual reader on every reload, before
// both implementations paint the same independently verified pass count.
TEST(CoverageFormCalcScalarConstraints, IsoCalendarRejectsInvalidDigitsAndSeparatorShapes) {
  ScalarRows({
      {R"FC(IsoDate2Num("1900-0x-01"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-00-01"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-13-01"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-01/01"))FC", R"FC(0)FC", R"FC(1)FC"},
      {R"FC(IsoDate2Num("1900-01-00"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900010x"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-01-010"))FC", R"FC(0)FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, IsoCalendarValidatesNonLeapAndThirtyDayMonthEnds) {
  ScalarRows({
      {R"FC(IsoDate2Num("1900-02-28"))FC", R"FC(59)FC"},
      {R"FC(IsoDate2Num("1900-02-29"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-04-30"))FC", R"FC(120)FC"},
      {R"FC(IsoDate2Num("1900-04-31"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-09-30"))FC", R"FC(273)FC"},
      {R"FC(IsoDate2Num("1900-09-31"))FC", R"FC(0)FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, IsoDateTimeAcceptsReducedClocksAndSignedZoneForms) {
  ScalarRows({
      {R"FC(IsoDate2Num("1900-01-01T12"))FC", R"FC(1)FC"},
      {R"FC(IsoDate2Num("1900-01-01T1234"))FC", R"FC(1)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34"))FC", R"FC(1)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:56.123Z"))FC", R"FC(1)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:56+05"))FC", R"FC(1)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:56-0530"))FC", R"FC(1)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:56+05:30"))FC", R"FC(1)FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, IsoDateTimeRejectsTruncatedOrNonNumericFractions) {
  ScalarRows({
      {R"FC(IsoDate2Num("1900-01-01T12:34:56.1"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:56.12"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:56.abc"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:56.1234"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:56Zx"))FC", R"FC(0)FC", R"FC(1)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:"))FC", R"FC(0)FC", R"FC(1)FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, IsoDateTimeRejectsOutOfRangeClockAndZoneComponents) {
  ScalarRows({
      {R"FC(IsoDate2Num("1900-01-01T24:00:00"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:60:00"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:61"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:56+24:00"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:56+05:60"))FC", R"FC(0)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:56+05:"))FC", R"FC(0)FC", R"FC(1)FC"},
      {R"FC(IsoDate2Num("1900-01-01T12:34:56+0x"))FC", R"FC(0)FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, OneofComparesStringAndBooleanValuesBeforeNumericCoercion) {
  ScalarRows({
      {R"FC(Oneof("alpha","beta","alpha"))FC", R"FC(1)FC"},
      {R"FC(Oneof("alpha","beta","gamma"))FC", R"FC(0)FC"},
      {R"FC(Oneof("a" lt "b",1))FC", R"FC(1)FC"},
      {R"FC(Oneof("b" lt "a",0))FC", R"FC(1)FC"},
  });
}

TEST(CoverageFormCalcScalarConstraints, SpaceRejectsARequestBeyondItsPublishedCharacterLimit) {
  ScalarProgram(R"FC(root.audit.rawValue="Entered"
Space(15654909)
root.report.rawValue="PASS")FC", u"Entered", u"Ready");
}

TEST(CoverageFormCalcScalarConstraints, ModRejectsZeroDivisorAfterExtractingARealNodeValue) {
  ScalarProgram(R"FC(root.audit.rawValue="Entered"
Mod(root.amount.rawValue,0)
root.report.rawValue="PASS")FC", u"Entered", u"Ready");
}

}  // namespace
}  // namespace pdfoxium::integration
