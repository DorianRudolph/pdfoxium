#include "xfa_form_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <functional>
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

struct LocaleOptions {
  std::string root_locale = "en_US";
  std::string field_locale;
  std::string config_locale;
};

std::string NodeLocale(const std::string& name, const std::string& decimal) {
  std::string calendar = "<calendarSymbols name=\"gregorian\">";
  for (bool abbreviated : {false, true}) {
    calendar += "<monthNames abbr=\"" + std::string(abbreviated ? "1" : "0") + "\">";
    for (int month = 1; month <= 12; ++month) {
      calendar += "<month>" + std::string(abbreviated ? "M" : "Month") +
          std::to_string(month) + "</month>";
    }
    calendar += "</monthNames><dayNames abbr=\"" + std::string(abbreviated ? "1" : "0") + "\">";
    for (int day = 0; day < 7; ++day) {
      calendar += "<day>" + std::string(abbreviated ? "D" : "Day") +
          std::to_string(day) + "</day>";
    }
    calendar += "</dayNames>";
  }
  calendar += "<meridiemNames><meridiem>dawn</meridiem><meridiem>dusk</meridiem></meridiemNames>"
      "<eraNames><era>Before</era><era>After</era></eraNames></calendarSymbols>";
  return "<locale name=\"" + name + "\">" + calendar +
      "<numberSymbols><numberSymbol name=\"decimal\">" + decimal + "</numberSymbol>"
      "<numberSymbol name=\"grouping\">_</numberSymbol><numberSymbol name=\"minus\">~</numberSymbol>"
      "<numberSymbol name=\"percent\">p</numberSymbol><numberSymbol name=\"zero\">0</numberSymbol></numberSymbols>"
      "<currencySymbols><currencySymbol name=\"symbol\">@</currencySymbol>"
      "<currencySymbol name=\"isoname\">CST</currencySymbol><currencySymbol name=\"decimal\">" + decimal +
      "</currencySymbol></currencySymbols>"
      "<numberPatterns><numberPattern name=\"numeric\">9</numberPattern>"
      "<numberPattern name=\"currency\">9</numberPattern><numberPattern name=\"percent\">9</numberPattern></numberPatterns>"
      "<datePatterns><datePattern name=\"short\">YYYY/MM/DD</datePattern>"
      "<datePattern name=\"med\">DD-MMM-YYYY</datePattern><datePattern name=\"long\">D MMMM YYYY</datePattern>"
      "<datePattern name=\"full\">EEEE D MMMM YYYY</datePattern></datePatterns>"
      "<timePatterns><timePattern name=\"short\">h:MM A</timePattern><timePattern name=\"med\">h:MM:SS A</timePattern>"
      "<timePattern name=\"long\">HH:MM:SS</timePattern><timePattern name=\"full\">HH:MM:SS</timePattern></timePatterns>"
      "<dateTimeSymbols>GyMdkHmsSEDFwWahKzZ</dateTimeSymbols></locale>";
}

std::string LocalePacket() {
  // Matching a built-in name must select this node-backed locale, while ambient
  // retains the host's pre-existing built-in default. Patterns are source-defined
  // by XFA_PatternToString; numberPattern payloads do not replace that algorithm.
  return "<localeSet xmlns=\"http://www.xfa.org/schema/xfa-locale-set/2.7/\">" +
      NodeLocale("en_US", ",") + NodeLocale("zz_XF", ":") + "</localeSet>";
}

std::vector<uint8_t> Fixture(const std::string& program, const std::string& model,
                           const std::string& language = "application/x-javascript",
                           const std::string& version = "3.3",
                           const LocaleOptions& options = {}) {
  const std::string preamble = "<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
  std::string config = "<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
  if (!options.config_locale.empty()) {
    const auto where = config.find("</acrobat>");
    config.insert(where, "<common><locale>" + options.config_locale + "</locale></common>");
  }
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
  const std::string root_attribute = " locale=\"en_US\"";
  const auto root_where = xml.find(root_attribute);
  xml.replace(root_where, root_attribute.size(), options.root_locale.empty() ? "" :
      " locale=\"" + options.root_locale + "\"");
  if (!options.field_locale.empty()) {
    const auto field_where = xml.find("<field name=\"run\"");
    xml.insert(field_where + std::string("<field name=\"run\"").size(),
        " locale=\"" + options.field_locale + "\"");
  }
  AlignXml(xml, preamble.size() + config.size());
  std::vector<std::string> objects{
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 260 320] /Resources << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (localeSet) 10 0 R (postamble) 9 0 R] >>",
      PdfStream("", preamble), PdfStream("", config), PdfStream("", xml),
      PdfStream("", "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root><group flag=\"old\"><item>A</item><item>B</item></group></root></xfa:data></xfa:datasets>"),
      PdfStream("", "</xdp:xdp>"),
      PdfStream("", LocalePacket())};  // Pinned font objects remain 11..13.
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
void Execute(Session& s, const std::u16string& expected, const Inspect& inspect) {
  s.host->alerts.clear();
  ASSERT_TRUE(s.Focus(12, 18)); ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page)); s.Replace(u"Ready");
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

void ExecuteLocale(Session& session, const std::u16string& expected) {
  ASSERT_TRUE(session.Focus(12, 170));
  ASSERT_TRUE(session.api.FORM_SelectAllText(session.form, session.page));
  session.Replace(u"Ready");
  Blur(session);
  ASSERT_NO_FATAL_FAILURE(Execute(session, u"PASS", [&](Session& current) {
    ASSERT_TRUE(current.Focus(12, 170));
    EXPECT_EQ(current.Text(), expected);
  }));
}

void CustomLocaleScenario(const std::string& expression,
                          const std::u16string& expected,
                          const LocaleOptions& options = {}) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::string program = "root.audit.rawValue=" + expression +
      "\nroot.report.rawValue=\"PASS\"";
  const auto bytes = Fixture(program, "", "application/x-formcalc", "3.3", options);
  Artifacts artifacts;
  std::array<Raster, 2> originals;
  size_t index = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    Session session(writer->api(), bytes, true, true, 2,
        [&](forms::FormHost& host) { Configure(host, JSPLATFORM_ALERT_RETURN_YES); });
    ASSERT_TRUE(session.valid());
    ASSERT_TRUE(session.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(ExecuteLocale(session, expected));
    originals[index] = session.DrawForComparison(260, 320);
    forms::ExpectInk(originals[index], 8, 160, 228, 184);
    const auto saved = SaveDocument(session.api, session.document, artifacts, writer->name() + "-custom-locale");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      Session loaded(reader->api(), saved, true, true, 2,
          [&](forms::FormHost& host) { Configure(host, JSPLATFORM_ALERT_RETURN_YES); });
      ASSERT_TRUE(loaded.valid());
      ASSERT_TRUE(loaded.xfa_loaded);
      ASSERT_NO_FATAL_FAILURE(ExecuteLocale(loaded, expected));
      Compare(originals[index], loaded.DrawForComparison(260, 320), artifacts,
          writer->name() + "-" + reader->name());
    }
    ++index;
  }
  Compare(originals[0], originals[1], artifacts, "custom-locale-engines");
}

TEST(CoverageXfaCustomLocale, DocumentLocaleOverridesBuiltinDecimalSymbol) {
  CustomLocaleScenario(R"FC(Format("num{9.99}",7.5))FC", u"7,50");
}

TEST(CoverageXfaCustomLocale, DocumentLocaleOverridesGroupingWithoutPictureQuoteAmbiguity) {
  CustomLocaleScenario(R"FC(Format("num{z,zz9.99}",1234.5))FC", u"1_234,50");
}

TEST(CoverageXfaCustomLocale, DocumentLocaleOverridesMinusSymbol) {
  CustomLocaleScenario(R"FC(Format("num{9.99}",-7.5))FC", u"~7,50");
}

TEST(CoverageXfaCustomLocale, DocumentLocaleOverridesCurrencySymbol) {
  CustomLocaleScenario(R"FC(Format("num{$9.99}",7.5))FC", u"@7,50");
}

TEST(CoverageXfaCustomLocale, DocumentLocaleOverridesPercentSymbol) {
  CustomLocaleScenario(R"FC(Format("num{zz9.9%}",0.125))FC", u"12,5p");
}

TEST(CoverageXfaCustomLocale, DocumentGroupingParsesBackToCanonicalDecimal) {
  CustomLocaleScenario(R"FC(Parse("num{z,zz9.99}","1_234,50"))FC", u"1234.50");
}

TEST(CoverageXfaCustomLocale, DocumentMinusParsesBackToCanonicalSign) {
  CustomLocaleScenario(R"FC(Parse("num{S9.99}","~7,50"))FC", u"-7.50");
}

TEST(CoverageXfaCustomLocale, DocumentCurrencyParsesBackToCanonicalNumber) {
  CustomLocaleScenario(R"FC(Parse("num{$9.99}","@7,50"))FC", u"7.50");
}

TEST(CoverageXfaCustomLocale, NamedCurrencyUsesStandardNumericPatternWithCustomSymbols) {
  CustomLocaleScenario(R"FC(Format("num.currency",1234.5))FC", u"@1_234,50");
}

TEST(CoverageXfaCustomLocale, NamedDecimalUsesStandardOptionalFractionPattern) {
  CustomLocaleScenario(R"FC(Format("num.decimal",1234.5))FC", u"1_234,5");
}

TEST(CoverageXfaCustomLocale, NamedIntegerUsesStandardGroupingPattern) {
  CustomLocaleScenario(R"FC(Format("num.integer",1234))FC", u"1_234");
}

TEST(CoverageXfaCustomLocale, NamedPercentUsesStandardScaleWithCustomPercentSymbol) {
  CustomLocaleScenario(R"FC(Format("num.percent",0.25))FC", u"25p");
}

TEST(CoverageXfaCustomLocale, ExplicitPictureLocaleSelectsTheCustomBuiltinName) {
  CustomLocaleScenario(R"FC(Format("num(en_US){9.99}",7.5))FC", u"7,50");
}

TEST(CoverageXfaCustomLocale, ExplicitPictureLocaleSelectsASecondNodeLocale) {
  CustomLocaleScenario(R"FC(Format("num(zz_XF){9.99}",7.5))FC", u"7:50");
}

TEST(CoverageXfaCustomLocale, AlternatingPictureLocalesDoNotLeakTheSelectedDefault) {
  CustomLocaleScenario(R"FC(Concat(Format("num(en_US){9.9}",7.5),"/",Format("num(zz_XF){9.9}",7.5),"/",Format("num{9.9}",7.5)))FC", u"7,5/7:5/7,5");
}

TEST(CoverageXfaCustomLocale, FieldLocaleOverridesItsTopLevelSubform) {
  CustomLocaleScenario(R"FC(Format("num{9.99}",7.5))FC", u"7:50",
      LocaleOptions{"en_US", "zz_XF", ""});
}

TEST(CoverageXfaCustomLocale, ConfigLocaleOverridesTheTopLevelSubformForAnInheritedField) {
  CustomLocaleScenario(R"FC(Format("num{9.99}",7.5))FC", u"7:50",
      LocaleOptions{"en_US", "", "zz_XF"});
}

TEST(CoverageXfaCustomLocale, ExplicitFieldLocaleOverridesConfigSelection) {
  CustomLocaleScenario(R"FC(Format("num{9.99}",7.5))FC", u"7,50",
      LocaleOptions{"zz_XF", "en_US", "zz_XF"});
}

TEST(CoverageXfaCustomLocale, AmbientFieldKeepsTheHostDefaultRatherThanTheCustomNameOverride) {
  CustomLocaleScenario(R"FC(Format("num{9.99}",7.5))FC", u"7.50",
      LocaleOptions{"zz_XF", "ambient", ""});
}

TEST(CoverageXfaCustomLocale, CustomLocalDatePatternUsesItsDateTimeSymbolAlphabet) {
  CustomLocaleScenario(R"FC(LocalDateFmt(1,"en_US"))FC", u"yyyy/MM/dd");
}

TEST(CoverageXfaCustomLocale, CustomLocalTimePatternUsesItsDateTimeSymbolAlphabet) {
  CustomLocaleScenario(R"FC(LocalTimeFmt(2,"en_US"))FC", u"h:mm:ss a");
}

TEST(CoverageXfaCustomLocale, CustomMonthNameOverridesTheCachedBuiltinLocale) {
  CustomLocaleScenario(R"FC(Num2Date(1,"DD MMMM YYYY","en_US"))FC", u"01 Month1 1900");
}

TEST(CoverageXfaCustomLocale, CustomDayNameOverridesTheCachedBuiltinLocale) {
  CustomLocaleScenario(R"FC(Num2Date(1,"EEEE","en_US"))FC", u"Day1");
}

TEST(CoverageXfaCustomLocale, CustomMeridiemsFormatBothHalvesOfTheDay) {
  // Keep each calendar result in its own field scene. Their concatenation
  // crosses native's separately documented 100pt text-layout wrap boundary.
  CustomLocaleScenario(R"FC(Format("time{h:MM A}","01:14:00"))FC", u"1:14 dawn");
  CustomLocaleScenario(R"FC(Format("time{h:MM A}","13:14:00"))FC", u"1:14 dusk");
}

TEST(CoverageXfaCustomLocale, CustomEraNameOverridesTheBuiltinCalendar) {
  CustomLocaleScenario(R"FC(Format("date{G YYYY}","1900-01-01"))FC", u"After 1900");
}

TEST(CoverageXfaCustomLocale, CustomMeridiemParsesToCanonicalTime) {
  CustomLocaleScenario(R"FC(Parse("time{h:MM A}","1:14 dusk"))FC", u"13:14:00");
}

}  // namespace
}  // namespace pdfoxium::integration
