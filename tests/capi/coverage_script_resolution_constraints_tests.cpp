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
void ResolutionScenario(const std::string& program, const std::string& model = Field("target", "Seed", 100),
         const Inspect& inspect = {}, int answer = JSPLATFORM_ALERT_RETURN_YES,
         const std::string& language = "application/x-javascript", const std::u16string& expected = u"PASS",
         const std::string& version = "3.3", const Inspect& initial = {}) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts; std::array<Raster, 2> originals; size_t index = 0;
  const auto fixture = Fixture(program, model, language, version);
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    Session s(writer->api(), fixture, true, true, 2, [&](forms::FormHost& h) { Configure(h, answer); });
    ASSERT_TRUE(s.valid()); ASSERT_TRUE(s.xfa_loaded);
    if (initial) { initial(s); }
    ASSERT_NO_FATAL_FAILURE(Execute(s, expected, inspect)); originals[index] = s.DrawForComparison(260, 320);
    forms::ExpectInk(originals[index], 8, 8, 228, 32);
    const auto saved = SaveDocument(s.api, s.document, artifacts, writer->name() + "-events"); ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name()); Session loaded(reader->api(), saved, true, true, 2,
          [&](forms::FormHost& h) { Configure(h, answer); });
      ASSERT_TRUE(loaded.valid()); ASSERT_TRUE(loaded.xfa_loaded);
      if (initial) { initial(loaded); }
      ASSERT_NO_FATAL_FAILURE(Execute(loaded, expected, inspect));
      Compare(originals[index], loaded.DrawForComparison(260, 320), artifacts, writer->name() + "-" + reader->name());
    }
    ++index;
  }
  Compare(originals[0], originals[1], artifacts, "event-recovery-engines");
}

// Exercise the object-valued branch of SOM rather than manufacturing parser
// nodes for host/event/layout services. The variables scripts also run in their
// own realm, so SDK fallback and FormCalc accessors cross the real C API hosts.
std::string ResolutionModel() {
  return Field("amount", "17", 100) +
      "<subform name=\"numbers\" presence=\"hidden\">"
      "<field name=\"item\"><bind match=\"none\"/><value><integer>1</integer></value></field>"
      "<field name=\"item\"><bind match=\"none\"/><value><integer>2</integer></value></field>"
      "<field name=\"item\"><bind match=\"none\"/><value><integer>3</integer></value></field>"
      "</subform><subform name=\"branch\" presence=\"hidden\">"
      "<field name=\"leaf\"><bind match=\"none\"/><value><integer>9</integer></value></field>"
      "</subform><field name=\"dot.name\" presence=\"hidden\"><bind match=\"none\"/>"
      "<value><integer>4</integer></value></field>"
      "<variables><script name=\"ops\" contentType=\"application/x-javascript\"><![CDATA["
      "var visits=0;let privateOffset=10;"
      "function reset(){visits=0;}"
      "function next(){visits++;return visits+privateOffset;}"
      "function viewer(){return app.viewerType;}"
      "function format(v){return util.printf('%04d',v);}"
      "function readAmount(){return amount.rawValue;}"
      "function writeAmount(v){amount.rawValue=v;return amount.rawValue;}"
      "function unknown(){return typeof capiMissingResolutionName;}"
      "function readGlobal(){return global.capiResolution;}"
      "function writeGlobal(v){global.capiResolution=v;return global.capiResolution;}"
      "function clearGlobal(){delete global.capiResolution;}"
      "function fail(){throw {tag:'resolution',visits:visits};}"
      "function combine(v){return format(v)+':'+viewer();}"
      "function formName(){return xfa.form.root.name;}"
      "function absent(){return typeof capiAbsentSomName;}"
      "]]></script></variables>";
}

void ScriptResolution(const std::string& program) {
  ResolutionScenario(program + ";r.audit.rawValue='Complete';", ResolutionModel(),
      [](Session& session) {
        ASSERT_TRUE(session.Focus(12, 170));
        EXPECT_EQ(session.Text(), u"Complete");
      });
}

void FormCalcResolution(const std::string& expression,
                        const std::u16string& expected,
                        const std::string& declarations = "") {
  const std::string program = declarations + "\nroot.audit.rawValue=" + expression +
      "\nroot.report.rawValue=\"PASS\"";
  ResolutionScenario(program, ResolutionModel(), [&](Session& session) {
    ASSERT_TRUE(session.Focus(12, 170));
    EXPECT_EQ(session.Text(), expected);
  }, JSPLATFORM_ALERT_RETURN_YES, "application/x-formcalc");
}

TEST(CoverageScriptResolutionConstraints, HostPseudoModelAbsoluteBindingsRespectTheActiveEventScope) {
  ScriptResolution(R"JS(
    var object=xfa['host'];
    yes(object!==null);
    eq(xfa.resolveNode('$host'),object);
    eq(xfa.resolveNode('$xfa.host'),object);
    eq(xfa.resolveNode('host'),null);
    eq(xfa.resolveNodes('$host').length,0);
    eq(xfa['host.className'],object.className);
  )JS");
}

TEST(CoverageScriptResolutionConstraints, EventPseudoModelAbsoluteBindingsRespectTheActiveEventScope) {
  ScriptResolution(R"JS(
    var object=xfa['event'];
    yes(object!==null);
    eq(xfa.resolveNode('$event'),object);
    eq(xfa.resolveNode('$xfa.event'),object);
    eq(xfa.resolveNode('event'),null);
    eq(xfa.resolveNodes('$event').length,0);
    eq(xfa['event.className'],object.className);
  )JS");
}

TEST(CoverageScriptResolutionConstraints, DataWindowPseudoModelAbsoluteBindingsRespectTheActiveEventScope) {
  ScriptResolution(R"JS(
    var object=xfa['dataWindow'];
    yes(object!==null);
    eq(xfa.resolveNode('$dataWindow'),object);
    eq(xfa.resolveNode('$xfa.dataWindow'),object);
    eq(xfa.resolveNode('dataWindow'),null);
    eq(xfa.resolveNodes('$dataWindow').length,0);
    eq(xfa['dataWindow.className'],object.className);
  )JS");
}

TEST(CoverageScriptResolutionConstraints, LogPseudoModelAbsoluteBindingsRespectTheActiveEventScope) {
  ScriptResolution(R"JS(
    var object=xfa['log'];
    yes(object!==null);
    eq(xfa.resolveNode('$log'),object);
    eq(xfa.resolveNode('$xfa.log'),object);
    eq(xfa.resolveNode('log'),null);
    eq(xfa.resolveNodes('$log').length,0);
    eq(xfa['log.className'],object.className);
  )JS");
}

TEST(CoverageScriptResolutionConstraints, SignaturePseudoModelAbsoluteBindingsRespectTheActiveEventScope) {
  ScriptResolution(R"JS(
    var object=xfa['signature'];
    yes(object!==null);
    eq(xfa.resolveNode('$signature'),object);
    eq(xfa.resolveNode('$xfa.signature'),object);
    eq(xfa.resolveNode('signature'),null);
    eq(xfa.resolveNodes('$signature').length,0);
    eq(xfa['signature.className'],object.className);
  )JS");
}

TEST(CoverageScriptResolutionConstraints, LayoutPseudoModelAbsoluteBindingsRespectTheActiveEventScope) {
  ScriptResolution(R"JS(
    var object=xfa['layout'];
    yes(object!==null);
    eq(xfa.resolveNode('$layout'),object);
    eq(xfa.resolveNode('$xfa.layout'),object);
    eq(xfa.resolveNode('layout'),null);
    eq(xfa.resolveNodes('$layout').length,0);
    eq(xfa['layout.className'],object.className);
  )JS");
}

TEST(CoverageScriptResolutionConstraints, HostPseudoModelConditionsFilterOneObjectWithoutCreatingParserNodes) {
  ScriptResolution(R"JS(
    var object=xfa['host'];
    eq(xfa.resolveNode('$host[0]'),object);
    eq(xfa.resolveNode('$host[*]'),object);
    eq(xfa.resolveNode('$host[1]'),null);
    eq(xfa.resolveNode('$host[+0]'),object);
    eq(xfa.resolveNode('$host[+1]'),null);
    eq(xfa.resolveNode('$host[-1]'),null);
  )JS");
}

TEST(CoverageScriptResolutionConstraints, EventPseudoModelConditionsFilterOneObjectWithoutCreatingParserNodes) {
  ScriptResolution(R"JS(
    var object=xfa['event'];
    eq(xfa.resolveNode('$event[0]'),object);
    eq(xfa.resolveNode('$event[*]'),object);
    eq(xfa.resolveNode('$event[1]'),null);
    eq(xfa.resolveNode('$event[+0]'),object);
    eq(xfa.resolveNode('$event[+1]'),null);
    eq(xfa.resolveNode('$event[-1]'),null);
  )JS");
}

TEST(CoverageScriptResolutionConstraints, DataWindowPseudoModelConditionsFilterOneObjectWithoutCreatingParserNodes) {
  ScriptResolution(R"JS(
    var object=xfa['dataWindow'];
    eq(xfa.resolveNode('$dataWindow[0]'),object);
    eq(xfa.resolveNode('$dataWindow[*]'),object);
    eq(xfa.resolveNode('$dataWindow[1]'),null);
    eq(xfa.resolveNode('$dataWindow[+0]'),object);
    eq(xfa.resolveNode('$dataWindow[+1]'),null);
    eq(xfa.resolveNode('$dataWindow[-1]'),null);
  )JS");
}

TEST(CoverageScriptResolutionConstraints, LogPseudoModelConditionsFilterOneObjectWithoutCreatingParserNodes) {
  ScriptResolution(R"JS(
    var object=xfa['log'];
    eq(xfa.resolveNode('$log[0]'),object);
    eq(xfa.resolveNode('$log[*]'),object);
    eq(xfa.resolveNode('$log[1]'),null);
    eq(xfa.resolveNode('$log[+0]'),object);
    eq(xfa.resolveNode('$log[+1]'),null);
    eq(xfa.resolveNode('$log[-1]'),null);
  )JS");
}

TEST(CoverageScriptResolutionConstraints, SignaturePseudoModelConditionsFilterOneObjectWithoutCreatingParserNodes) {
  ScriptResolution(R"JS(
    var object=xfa['signature'];
    eq(xfa.resolveNode('$signature[0]'),object);
    eq(xfa.resolveNode('$signature[*]'),object);
    eq(xfa.resolveNode('$signature[1]'),null);
    eq(xfa.resolveNode('$signature[+0]'),object);
    eq(xfa.resolveNode('$signature[+1]'),null);
    eq(xfa.resolveNode('$signature[-1]'),null);
  )JS");
}

TEST(CoverageScriptResolutionConstraints, LayoutPseudoModelConditionsFilterOneObjectWithoutCreatingParserNodes) {
  ScriptResolution(R"JS(
    var object=xfa['layout'];
    eq(xfa.resolveNode('$layout[0]'),object);
    eq(xfa.resolveNode('$layout[*]'),object);
    eq(xfa.resolveNode('$layout[1]'),null);
    eq(xfa.resolveNode('$layout[+0]'),object);
    eq(xfa.resolveNode('$layout[+1]'),null);
    eq(xfa.resolveNode('$layout[-1]'),null);
  )JS");
}

TEST(CoverageScriptResolutionConstraints, JavascriptPseudoPredicatesUseTheResolvedObjectAsThis) {
  ScriptResolution(R"JS(
    eq(xfa.resolveNode('$host.(this.version == "11")'),xfa.host);
    eq(xfa.resolveNode('$host.(this.version == "missing")'),null);
    eq(xfa.host.version,'11');
    eq(xfa.this.name,'run');
)JS");
}

TEST(CoverageScriptResolutionConstraints, FormCalcPseudoPredicatesRetainTruthfulAndRejectFalseResults) {
  ScriptResolution(R"JS(
    eq(xfa.resolveNode('$host.[1]'),xfa.host);
    eq(xfa.resolveNode('$host.[0]'),null);
    eq(xfa.resolveNode('$host.[1+1]'),xfa.host);
    eq(xfa.this.name,'run');
)JS");
}

TEST(CoverageScriptResolutionConstraints, ThrowingPseudoPredicateRestoresTheClickEventOwner) {
  ScriptResolution(R"JS(
    eq(xfa.resolveNode('$host.(throw 17)'),null);
    eq(xfa.this.name,'run');
    eq(xfa.resolveNode('$host'),xfa.host);
    eq(xfa.host.version,'11');
)JS");
}

TEST(CoverageScriptResolutionConstraints, QualifiedHostAttributesAreValuesRatherThanResolveNodeResults) {
  ScriptResolution(R"JS(
    eq(xfa['host.version'],'11');
    eq(xfa.resolveNode('host.version'),null);
    eq(xfa.resolveNode('host.version.missing'),null);
    eq(xfa['host.numPages'],1);
    eq(xfa['host.currentPage'],0);
)JS");
}

TEST(CoverageScriptResolutionConstraints, QualifiedHostCalculationReadsFollowResolvedHostWrites) {
  ScriptResolution(R"JS(
    // NormalPropertySetter takes an attribute name, not a SOM expression.
    // Resolve the host object for writes; qualified getters use the SOM path.
    var old=xfa.host.calculationsEnabled;
    try {
      xfa.resolveNode('$host').calculationsEnabled=false;
      eq(xfa['host.calculationsEnabled'],false);
      xfa.resolveNode('$host').calculationsEnabled=true;
      eq(xfa['host.calculationsEnabled'],true);
    } finally {xfa.host.calculationsEnabled=old;}
)JS");
}

TEST(CoverageScriptResolutionConstraints, QualifiedHostValidationReadsFollowResolvedHostWrites) {
  ScriptResolution(R"JS(
    var old=xfa.host.validationsEnabled;
    try {
      xfa.resolveNode('$host').validationsEnabled=false;
      eq(xfa['host.validationsEnabled'],false);
      xfa.resolveNode('$host').validationsEnabled=true;
      eq(xfa['host.validationsEnabled'],true);
    } finally {xfa.host.validationsEnabled=old;}
)JS");
}

TEST(CoverageScriptResolutionConstraints, VariablesRealmResolvesViewerServicesFromTheSdkGlobal) {
  ScriptResolution(R"JS(eq(r.ops.viewer(),'pdfium');eq(r.ops.viewer(),'pdfium');)JS");
}

TEST(CoverageScriptResolutionConstraints, VariablesRealmCallsSdkFormattingWithArgumentsAndCoercion) {
  ScriptResolution(R"JS(eq(r.ops.format(7),'0007');eq(r.ops.format('23'),'0023');eq(r.ops.format(-2),'-002');)JS");
}

TEST(CoverageScriptResolutionConstraints, VariablesRealmKeepsLexicalBindingsAcrossRepeatedExportedCalls) {
  ScriptResolution(R"JS(r.ops.reset();eq(r.ops.next(),11);eq(r.ops.next(),12);eq(r.ops.next(),13);)JS");
}

TEST(CoverageScriptResolutionConstraints, ExtractedVariablesFunctionsRetainTheirLexicalRealm) {
  ScriptResolution(R"JS(r.ops.reset();var next=r.ops.next;eq(next(),11);eq(next.call({}),12);eq(r.ops.next(),13);)JS");
}

TEST(CoverageScriptResolutionConstraints, VariablesRealmResolvesAParentSiblingFieldThroughSom) {
  ScriptResolution(R"JS(r.amount.rawValue='17';eq(clean(r.ops.readAmount()),'17');eq(clean(r.ops.writeAmount('31')),'31');eq(clean(r.amount.rawValue),'31');r.amount.rawValue='17';)JS");
}

TEST(CoverageScriptResolutionConstraints, VariablesRealmFallsBackToUndefinedForUnknownNames) {
  ScriptResolution(R"JS(eq(r.ops.unknown(),'undefined');eq(r.ops.absent(),'undefined');eq(r.ops.viewer(),'pdfium');)JS");
}

TEST(CoverageScriptResolutionConstraints, VariablesRealmSharesSdkGlobalStateAcrossExportedCalls) {
  ScriptResolution(R"JS(try{eq(r.ops.writeGlobal(71),71);eq(global.capiResolution,71);global.capiResolution=23;eq(r.ops.readGlobal(),23);}finally{r.ops.clearGlobal();})JS");
}

TEST(CoverageScriptResolutionConstraints, VariablesRealmGlobalObjectValuesRetainTheirJavaScriptType) {
  ScriptResolution(R"JS(try{eq(r.ops.writeGlobal(true),true);eq(r.ops.readGlobal(),true);eq(r.ops.writeGlobal('text'),'text');eq(r.ops.readGlobal(),'text');}finally{r.ops.clearGlobal();})JS");
}

TEST(CoverageScriptResolutionConstraints, ExceptionFromVariablesRealmLeavesSdkAndSomResolutionUsable) {
  ScriptResolution(R"JS(r.ops.reset();eq(r.ops.next(),11);var caught=false;try{r.ops.fail();}catch(e){eq(e.tag,'resolution');eq(e.visits,1);caught=true;}yes(caught);eq(r.ops.viewer(),'pdfium');eq(r.ops.next(),12);eq(r.ops.formName(),'root');)JS");
}

TEST(CoverageScriptResolutionConstraints, NestedExportedCallsKeepTheVariablesRealmAndSdkFallback) {
  ScriptResolution(R"JS(eq(r.ops.combine(12),'0012:pdfium');eq(r.ops.combine(9),'0009:pdfium');eq(r.ops.formName(),'root');)JS");
}

TEST(CoverageScriptResolutionConstraints, FormCalcHashClassAccessorSelectsAnIndexedField) {
  FormCalcResolution(R"FC(root.numbers.#field[1])FC", u"2");
}

TEST(CoverageScriptResolutionConstraints, FormCalcHashClassWildcardPassesEveryFieldToSum) {
  FormCalcResolution(R"FC(Sum(root.numbers.#field[*]))FC", u"6");
}

TEST(CoverageScriptResolutionConstraints, FormCalcHashClassAccessorWithoutAnIndexUsesTheFirstField) {
  FormCalcResolution(R"FC(root.numbers.#field)FC", u"1");
}

TEST(CoverageScriptResolutionConstraints, FormCalcDescendantAccessorAcceptsAnAbsoluteIndex) {
  FormCalcResolution(R"FC(root..item[1])FC", u"2");
}

TEST(CoverageScriptResolutionConstraints, FormCalcDescendantWildcardPassesAllMatchingValuesToSum) {
  FormCalcResolution(R"FC(Sum(root..item[*]))FC", u"6");
}

// An identifier followed directly by brackets is a SOM name lookup, not a
// lexical-variable array index. Method results retain the actual node reference
// for the unnamed accessor path, including relative and computed indices.
TEST(CoverageScriptResolutionConstraints, FormCalcMethodReferenceAcceptsAComputedSiblingIndex) {
  FormCalcResolution(R"FC(root.numbers.resolveNode("item[0]")[offset])FC", u"3",
      R"FC(var offset=2)FC");
}

TEST(CoverageScriptResolutionConstraints, FormCalcPositiveRelativeMethodIndexUsesItsOriginalPosition) {
  FormCalcResolution(R"FC(root.numbers.resolveNode("item[1]")[+1])FC", u"3");
}

TEST(CoverageScriptResolutionConstraints, FormCalcNegativeRelativeMethodIndexUsesItsOriginalPosition) {
  FormCalcResolution(R"FC(root.numbers.resolveNode("item[1]")[-1])FC", u"1");
}

TEST(CoverageScriptResolutionConstraints, FormCalcMethodResultCanBeIndexedBeforeDefaultValueExtraction) {
  FormCalcResolution(R"FC(root.numbers.resolveNode("item[0]")[1])FC", u"2");
}

// Native ResolveObjects concatenates the raw node name before its unnamed
// index, turning dot.name[0] into two SOM components. Rust keeps that original
// name escaped. Check the original outcome before a healthy independent event
// restores a common visible scene for raster and writer/reader comparisons.
void DottedReferenceEvents(Session& session, const Library& reader,
                           Artifacts& artifacts, const std::string& label) {
  const bool native = reader.name() == "native";
  ASSERT_TRUE(session.Focus(12, 170));
  ASSERT_TRUE(session.api.FORM_SelectAllText(session.form, session.page));
  session.Replace(u"Ready");
  Blur(session);
  std::u16string original_audit;
  ASSERT_NO_FATAL_FAILURE(Execute(session, native ? u"Ready" : u"PASS",
      [&](Session& current) {
        ASSERT_TRUE(current.Focus(12, 170));
        original_audit = current.Text();
        EXPECT_EQ(original_audit, native ? u"Ready" : u"4");
      }));
  ASSERT_TRUE(session.Focus(12, 18));
  const auto original_report = session.Text();
  artifacts.Text(label + "-original-dotted-scalars",
      "report=" + std::string(original_report.begin(), original_report.end()) +
      "\naudit=" + std::string(original_audit.begin(), original_audit.end()) + "\n");
  Blur(session);
  artifacts.Image(label + "-original-dotted-reference", session.DrawForComparison(260, 320));

  // The direct escaped lookup remains a valid native oracle. It does not take
  // the broken unnamed-index path and must produce the same independent value.
  session.Click(12, 220);
  ASSERT_TRUE(session.Focus(12, 170));
  EXPECT_EQ(session.Text(), u"4");
  ASSERT_TRUE(session.Focus(12, 18));
  EXPECT_EQ(session.Text(), u"PASS");
  ASSERT_TRUE(session.api.FORM_SelectAllText(session.form, session.page));
  session.Replace(u"PASS");
  Blur(session);
  EXPECT_TRUE(session.host->callbacks_valid);
  EXPECT_FALSE(session.host->callback_error);
}

TEST(CoverageScriptResolutionConstraints, FormCalcIndexedMethodResultEscapesDotsInTheOriginalNodeName) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::string original = R"FC(
    root.audit.rawValue=root.resolveNode("dot\\.name")[0]
    root.report.rawValue="PASS"
  )FC";
  const std::string control = R"FC(
    root.audit.rawValue=root.resolveNode("dot\\.name").rawValue
    root.report.rawValue="PASS"
  )FC";
  const std::string model = ResolutionModel() +
      Field("control", "", 210,
          "<caption><value><text>Check</text></value></caption>" +
          forms::Event(control, "click", "application/x-formcalc") +
          forms::Event("this.caption.value.text.value='Check';", "initialize"),
          "<button/>");
  const auto fixture = Fixture(original, model, "application/x-formcalc");
  Artifacts artifacts;
  std::array<Raster, 2> originals;
  size_t index = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    Session session(writer->api(), fixture, true, true, 2,
        [&](forms::FormHost& host) { Configure(host, JSPLATFORM_ALERT_RETURN_YES); });
    ASSERT_TRUE(session.valid());
    ASSERT_TRUE(session.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(DottedReferenceEvents(session, *writer, artifacts, writer->name()));
    originals[index] = session.DrawForComparison(260, 320);
    const auto saved = SaveDocument(session.api, session.document, artifacts, writer->name() + "-dotted-reference");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      Session loaded(reader->api(), saved, true, true, 2,
          [&](forms::FormHost& host) { Configure(host, JSPLATFORM_ALERT_RETURN_YES); });
      ASSERT_TRUE(loaded.valid());
      ASSERT_TRUE(loaded.xfa_loaded);
      const std::string label = writer->name() + "-" + reader->name();
      ASSERT_NO_FATAL_FAILURE(DottedReferenceEvents(loaded, *reader, artifacts, label));
      Compare(originals[index], loaded.DrawForComparison(260, 320), artifacts, label);
    }
    ++index;
  }
  Compare(originals[0], originals[1], artifacts, "dotted-reference-engines");
}

TEST(CoverageScriptResolutionConstraints, FormCalcChildWildcardPassesOnlyActualChildrenToSum) {
  FormCalcResolution(R"FC(Sum(root.numbers.*))FC", u"6");
}

TEST(CoverageScriptResolutionConstraints, FormCalcPacketAndPseudoRootsResolveFromTheActiveEventRealm) {
  FormCalcResolution(R"FC($host.version)FC", u"11");
}

}  // namespace
}  // namespace pdfoxium::integration
