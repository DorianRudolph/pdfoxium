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

// These fixtures supply event/validation definitions that the public C API
// cannot construct. Every invocation, edit, repaint and reload uses that API.
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
void ExerciseEvents(const std::string& program, const std::string& model = Field("target", "Seed", 100),
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
void HasMessage(Session& s, const std::u16string& text, int icon = -1, int buttons = -1) {
  bool found = false;
  for (const auto& alert : s.host->alerts) {
    if (std::get<0>(alert).find(text) == std::u16string::npos) continue;
    found = true;
    if (icon >= 0) { EXPECT_EQ(std::get<3>(alert), icon); }
    if (buttons >= 0) { EXPECT_EQ(std::get<2>(alert), buttons); }
  }
  EXPECT_TRUE(found) << "missing expected validation alert";
}
std::string Validate(const std::string& attributes, const std::string& children = "") {
  return "<validate " + attributes + ">" + children + "</validate>";
}

TEST(CoverageXfaEventRecovery, FieldExecEventRoutesEveryDirectActivityToItsActualOwner) {
  for (const std::string event : {"enter", "exit", "change", "full", "click", "mouseEnter", "mouseExit", "mouseDown", "mouseUp"}) {
    SCOPED_TRACE(event);
    ExerciseEvents("r.audit.rawValue='before';eq(r.target.execEvent('" + event + "'),undefined);eq(clean(r.audit.rawValue),'" + event + "');r.audit.rawValue='Complete';",
        Field("target", "Seed", 100, forms::Event("xfa.form.root.audit.rawValue='" + event + "';", event)));
  }
}
TEST(CoverageXfaEventRecovery, UnknownAndInapplicableEventsHaveNoSideEffects) {
  ExerciseEvents("var names=['unknown','indexChange','preOpen','postOpen','preSign','postSign'];for(var i=0;i<names.length;i++){r.audit.rawValue='before';eq(r.target.execEvent(names[i]),undefined);eq(clean(r.audit.rawValue),'before');}r.audit.rawValue='Complete';",
      Field("target", "Seed", 100));
}
TEST(CoverageXfaEventRecovery, SignatureActivitiesRequireTheSignatureUi) {
  for (const std::string event : {"preSign", "postSign"}) {
    SCOPED_TRACE(event);
    ExerciseEvents("r.audit.rawValue='before';r.target.execEvent('" + event + "');eq(clean(r.audit.rawValue),'signed');r.audit.rawValue='Complete';",
        Field("target", "", 100, forms::Event("xfa.form.root.audit.rawValue='signed';", event), "<signature/>"));
  }
}
TEST(CoverageXfaEventRecovery, ScriptRecursionLimitUnwindsAndPermitsTheNextIndependentEvent) {
  ExerciseEvents("r.audit.rawValue='0';r.target.execEvent('click');eq(Number(r.audit.rawValue),3);r.target.execEvent('enter');eq(clean(r.audit.rawValue),'Recovered');",
      Field("target", "Seed", 100,
          forms::Event("xfa.form.root.audit.rawValue=String(Number(xfa.form.root.audit.rawValue)+1);this.execEvent('click');", "click") +
          forms::Event("xfa.form.root.audit.rawValue='Recovered';", "enter")));
}
TEST(CoverageXfaEventRecovery, ServerAndUnknownScriptKindsDeclineExecutionWithoutReplacingExistingData) {
  for (const std::string attributes : {"runAt=\"server\" contentType=\"application/x-javascript\"", "contentType=\"application/unknown\""}) {
    SCOPED_TRACE(attributes);
    ExerciseEvents("r.audit.rawValue='before';r.target.execEvent('click');eq(clean(r.audit.rawValue),'before');r.audit.rawValue='Complete';",
        Field("target", "Seed", 100, "<event activity=\"click\"><script " + attributes + "><![CDATA[xfa.form.root.audit.rawValue='BAD';]]></script></event>"));
  }
}
TEST(CoverageXfaEventRecovery, EmptyAndThrowingEventScriptsLeaveTheDocumentUsable) {
  for (const std::string script : {"", "throw new Error('deliberate event failure');"}) {
    SCOPED_TRACE(script);
    ExerciseEvents("r.target.rawValue='before';r.target.execEvent('click');eq(clean(r.target.rawValue),'before');r.target.rawValue='Recovered';",
        Field("target", "Seed", 100, forms::Event(script, "click")));
  }
}
TEST(CoverageXfaEventRecovery, EventMethodsRejectInvalidArityBeforeAnyMutation) {
  ExerciseEvents("var f=r.target;throws(function(){f.execEvent();});throws(function(){f.execEvent('click',1);});"
      "throws(function(){f.execInitialize(1);});throws(function(){f.execCalculate(1);});throws(function(){f.execValidate(1);});"
      "eq(clean(f.rawValue),'Seed');f.rawValue='Recovered';");
}
TEST(CoverageXfaEventRecovery, RecursiveInitializeAndCalculateUseRealSubformChildren) {
  ExerciseEvents("r.audit.rawValue='before';r.group.execInitialize();eq(clean(r.audit.rawValue),'initialized');"
      "r.group.execCalculate();eq(Number(r.group.child.rawValue),42);r.audit.rawValue='Complete';",
      "<subform name=\"group\" x=\"8pt\" y=\"100pt\" w=\"220pt\" h=\"24pt\">" +
      Field("child", "Seed", 0, forms::Event("xfa.form.root.audit.rawValue='initialized';", "initialize") +
        "<calculate><script contentType=\"application/x-javascript\">42;</script></calculate>") + "</subform>");
}
TEST(CoverageXfaEventRecovery, SubformIndexChangeDoesNotDispatchMouseOnlyFieldActivities) {
  ExerciseEvents("r.audit.rawValue='before';r.group.execEvent('indexChange');eq(clean(r.audit.rawValue),'index');"
      "r.audit.rawValue='before';r.group.execEvent('mouseDown');eq(clean(r.audit.rawValue),'before');r.audit.rawValue='Complete';",
      "<subform name=\"group\" x=\"8pt\" y=\"100pt\" w=\"220pt\" h=\"24pt\">" +
      forms::Event("xfa.form.root.audit.rawValue='index';", "indexChange") + Field("child", "Seed", 0) + "</subform>");
}
TEST(CoverageXfaEventRecovery, FieldMessagePropertiesKeepIndependentNullAndFormatText) {
  ExerciseEvents("var f=r.target;f.mandatoryMessage='Required';f.formatMessage='Format';f.validationMessage='Script';"
      "eq(f.mandatoryMessage,'Required');eq(f.formatMessage,'Format');eq(f.validationMessage,'Script');"
      "f.mandatoryMessage='';eq(f.mandatoryMessage,'');eq(f.formatMessage,'Format');eq(f.validationMessage,'Script');");
}
TEST(CoverageXfaEventRecovery, DefaultNumericValidationMessagesPreferCaptionTextAndFallBackToName) {
  for (bool caption : {false, true}) for (const std::string mode : {"error", "warning"}) {
    SCOPED_TRACE(mode + (caption ? "-caption" : "-name"));
    const auto cap = caption ? "<caption placement=\"left\"><value><text>Account</text></value></caption>" : "";
    const auto name = caption ? u"Account" : u"target";
    const std::u16string expected = std::u16string(u"The value you entered for ") + name + u" is invalid.";
    ExerciseEvents("var f=r.target;" + std::string(caption ? "f.caption.value.text.value='Account';" : "") +
        "f.rawValue='bad';eq(f.execValidate(),false);f.rawValue='12';eq(f.execValidate(),true);",
        Field("target", "12", 100, cap + Validate("nullTest=\"disabled\" formatTest=\"" + mode + "\"", "<picture>num{99}</picture>")),
        [=](Session& s) { HasMessage(s, expected); });
  }
}
// execValidate returns the validation event status. After layout a false
// script value alone is not a format/null error; the field remains editable.
TEST(CoverageXfaEventRecovery, EmptyScriptValidationDefersAlertsAndAllowsExplicitRecovery) {
  for (const std::string version : {"2.6", "3.3"}) for (const std::string mode : {"error", "warning"})
      for (bool caption : {false, true}) for (int answer : {JSPLATFORM_ALERT_RETURN_YES, JSPLATFORM_ALERT_RETURN_NO}) {
    SCOPED_TRACE(version + ":" + mode + (caption ? ":caption" : ":name") + ":" + std::to_string(answer));
    const auto model = Field("target", "", 100,
        std::string(caption ? "<caption placement=\"left\"><value><text>Account</text></value></caption>" : "") +
        Validate("scriptTest=\"" + mode + "\" nullTest=\"disabled\"",
            "<script contentType=\"application/x-javascript\">false;</script>") +
        (caption ? forms::Event("this.caption.value.text.value='Account';", "initialize") : ""));
    ExerciseEvents("r.target.rawValue=null;eq(r.target.execValidate(),true);r.target.rawValue='Recovered';r.audit.rawValue='Complete';",
        model, {}, answer, "application/x-javascript", u"PASS", version,
        [](Session& s) { EXPECT_TRUE(s.host->alerts.empty()); });
  }
}
TEST(CoverageXfaEventRecovery, ExplicitFormatWarningsExerciseBothAcceptAndRejectReplies) {
  for (int answer : {JSPLATFORM_ALERT_RETURN_YES, JSPLATFORM_ALERT_RETURN_NO}) {
    SCOPED_TRACE(answer);
    ExerciseEvents("var f=r.target;f.rawValue='bad';eq(f.execValidate(),false);f.rawValue='34';eq(f.execValidate(),true);",
        Field("target", "12", 100, Validate("nullTest=\"disabled\" formatTest=\"warning\"",
          "<picture>num{99}</picture><message><text name=\"formatTest\">Two digits</text></message>")),
        [](Session& s) { HasMessage(s, u"Two digits"); }, answer);
  }
}
TEST(CoverageXfaEventRecovery, EmptyNumericFormatInputRecoversWithoutTreatingItAsAValidNumber) {
  ExerciseEvents("var f=r.target;f.rawValue=null;eq(f.execValidate(),false);f.rawValue='12';eq(f.execValidate(),true);",
      Field("target", "12", 100, Validate("nullTest=\"disabled\" formatTest=\"error\"", "<picture>num{99}</picture>")));
}
TEST(CoverageXfaEventRecovery, NullWarningAcceptanceAndRejectionPreserveRecoverableModelState) {
  for (int answer : {JSPLATFORM_ALERT_RETURN_YES, JSPLATFORM_ALERT_RETURN_NO}) {
    SCOPED_TRACE(answer);
    ExerciseEvents("var f=r.target;f.rawValue='nonempty';f.rawValue=null;eq(f.execValidate(),false);f.rawValue='Recovered';eq(f.execValidate(),true);",
        Field("target", "Seed", 100, Validate("nullTest=\"warning\"")),
        [](Session& s) {
          // execValidate deliberately batches null tests without opening an
          // alert (flags=1). A real edit commit enters the interactive path.
          ASSERT_TRUE(s.Focus(12, 108));
          const auto text = s.Text();
          ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form, s.page, FWL_VKEY_Home, FWL_EVENTFLAG_ControlKey));
          for (size_t i = 0; i < text.size() && !s.Text().empty(); ++i) {
            ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form, s.page, FWL_VKEY_Delete, 0));
          }
          EXPECT_EQ(s.Text(), u""); Blur(s); HasMessage(s, u"cannot be blank");
          ASSERT_TRUE(s.Focus(12, 108));
          for (char16_t ch : std::u16string(u"Recovered")) {
            ASSERT_TRUE(s.api.FORM_OnChar(s.form, s.page, ch, 0));
          }
          EXPECT_EQ(s.Text(), u"Recovered"); Blur(s);
        }, answer);
  }
}
TEST(CoverageXfaEventRecovery, NumericFieldEditAndDisplayPicturesRemainSeparateFromRawValue) {
  ExerciseEvents("var f=r.target;f.format.picture.value='num{z,zz9.99}';f.ui.picture.value='num{zzz9.99}';"
      "f.rawValue='1234.5';eq(Number(f.rawValue),1234.5);eq(f.formattedValue,'1,234.50');"
      "f.editValue='42.25';eq(Number(f.rawValue),42.25);f.formattedValue='34.50';eq(Number(f.rawValue),34.5);",
      Field("target", "12", 100, "<value><decimal>12</decimal></value><format><picture>num{z,zz9.99}</picture></format>", "<numericEdit/><picture>num{zzz9.99}</picture>"));
}
TEST(CoverageXfaEventRecovery, ChoiceMethodsRejectWrongArgumentsAndKeepTheVisibleTextFieldIntact) {
  ExerciseEvents("var f=r.target;throws(function(){f.addItem();});throws(function(){f.addItem('a','b','c');});"
      "throws(function(){f.deleteItem();});throws(function(){f.boundItem();});throws(function(){f.getItemState();});"
      "throws(function(){f.setItemState(1);});throws(function(){f.getDisplayItem();});throws(function(){f.getSaveItem(0,1);});"
      "eq(f.parentSubform,null);eq(f.getDisplayItem(-1),null);eq(f.getSaveItem(999),null);eq(clean(f.rawValue),'Seed');");
}
TEST(CoverageXfaEventRecovery, NodeModelReferencesAndReadOnlyPropertiesRemainAttachedToTheirDocument) {
  ExerciseEvents("var f=r.target;eq(f.model.className,'form');eq(f.parent.className,'subform');eq(f.model.name,'form');"
      "throws(function(){f.model=r;});throws(function(){f.parent=r;});yes(f.somExpression.indexOf('target')>=0);");
}
TEST(CoverageXfaEventRecovery, NodePropertyLookupRejectsUnknownTypesAndNegativeIndicesWithoutMutation) {
  ExerciseEvents("var f=r.target;eq(f.getElement('notAnElement'),null);eq(f.getElement('caption',-1),null);"
      "eq(f.isPropertySpecified('notAnElement'),false);f.setAttribute('target','name');eq(f.isPropertySpecified('name',false),true);"
      "throws(function(){f.getElement();});throws(function(){f.getElement('caption',0,1);});"
      "throws(function(){f.isPropertySpecified();});throws(function(){f.isPropertySpecified('name',true,0,1);});"
      "eq(clean(f.rawValue),'Seed');");
}
TEST(CoverageXfaEventRecovery, SettingElementsAndAttributesRespectsTheirDifferentArgumentContracts) {
  ExerciseEvents("var f=r.target;throws(function(){f.setElement();});throws(function(){f.setElement('caption',0,1);});"
      "eq(f.setElement('caption'),undefined);eq(f.setElement('caption',0),undefined);"
      "f.setAttribute('right','hAlign');eq(f.getAttribute('hAlign'),'right');f.setAttribute('left','hAlign');"
      "throws(function(){f.getAttribute();});throws(function(){f.setAttribute('name');});");
}
TEST(CoverageXfaEventRecovery, XmlAppendAndOverwriteKeepNamedDatasetsAndSerializeEscapedContent) {
  ExerciseEvents("var d=xfa.datasets,p=d.data.root,g=d.createNode('dataGroup','scratch');p.nodes.append(g);g.loadXML('<group><item>A</item><item>B</item></group>',true,true);"
      "g.loadXML('<item>C &amp; D</item>',false,false);eq(clean(g.resolveNode('item[2]').value),'C & D');"
      "yes(g.saveXML().indexOf('C &amp; D')>=0);"
      "g.loadXML('<group flag=\"new\"><item>Changed</item></group>',true,true);eq(clean(g.item.value),'Changed');"
      "eq(g.resolveNode('item[1]'),null);yes(g.saveXML('pretty').indexOf('Changed')>=0);p.nodes.remove(g);");
}
TEST(CoverageXfaEventRecovery, XmlProcessingInstructionRichTextAndIgnoredRootKeepDocumentUsable) {
  ExerciseEvents("var d=xfa.datasets,p=d.data.root,g=d.createNode('dataGroup','scratch');p.nodes.append(g);g.loadXML('<?xml version=\"1.0\"?><replacement><item>One</item><item>Two</item></replacement>',true,true);"
      "eq(clean(g.item.value),'One');eq(clean(g.resolveNode('item[1]').value),'Two');"
      "var before=g.saveXML();g.loadXML('<broken a=unquoted />',true,true);eq(g.saveXML(),before);"
      "g.loadXML('<replacement><item>Again</item></replacement>',true,true);eq(clean(g.item.value),'Again');p.nodes.remove(g);");
}
TEST(CoverageXfaEventRecovery, DatasetCreateNodeNamespacesAndWrongArityHaveIndependentContracts) {
  ExerciseEvents("var d=xfa.datasets;throws(function(){d.createNode();});throws(function(){d.isCompatibleNS();});"
      "eq(d.isCompatibleNS('urn:missing'),false);eq(d.isCompatibleNS('http://www.xfa.org/schema/xfa-data/1.0/'),true);"
      "var n=d.createNode('dataValue','newValue');eq(n.className,'dataValue');eq(n.name,'newValue');"
      "n.value='Created';eq(n.value,'Created');");
}
TEST(CoverageXfaEventRecovery, IntegerValueChildWritesRetainTheirTypedRawValue) {
  ExerciseEvents("var v=r.target.value;v.oneOfChild.value='9';eq(Number(r.target.rawValue),9);"
      "v.oneOfChild.value='0';eq(Number(r.target.rawValue),0);",
      Field("target", "", 100, "<value><integer>3</integer></value>", "<numericEdit/>"));
}
TEST(CoverageXfaEventRecovery, DrawDefaultValuesIgnoreNumericWritesAndAcceptTextWrites) {
  ExerciseEvents("r.label.rawValue='Label';r.label.rawValue=12;eq(clean(r.label.rawValue),'Label');r.label.rawValue='Changed';eq(clean(r.label.rawValue),'Changed');",
      "<draw name=\"label\" x=\"8pt\" y=\"100pt\" w=\"220pt\" h=\"24pt\"><font typeface=\"Helvetica\" size=\"12pt\"/><value><text>Label</text></value></draw>");
}
TEST(CoverageXfaEventRecovery, LocaleWritesAndOccurrenceMutationsUseTheirExistingModelProperties) {
  ExerciseEvents("var f=r.target;f.locale='en_US';eq(f.locale,'en_US');f.locale='de_DE';eq(f.locale,'de_DE');f.locale='en_US';"
      "var o=r.group.occur;o.min=0;o.max=3;eq(o.min,0);eq(o.max,3);o.min=1;o.max=1;eq(o.min,1);eq(o.max,1);",
      Field("target", "Seed", 100) + "<subform name=\"group\" x=\"8pt\" y=\"210pt\" w=\"220pt\" h=\"24pt\"><occur min=\"1\" max=\"1\"/>" + Field("child", "Child", 0) + "</subform>");
}

// Model entrypoints execute against the document's existing FF service. The
// fixtures have no remerge or geometry-changing handlers, keeping traversal
// bounded while checking the real public script-to-host call paths.
TEST(CoverageXfaEventRecovery, FormNodesChecksTheDataNodeAndPersistsIndependentEmptyLists) {
  ExerciseEvents("var f=xfa.form;throws(function(){f.formNodes();});throws(function(){f.formNodes(r,1);});"
      "[null,undefined,{},12,'bad'].forEach(function(v){throws(function(){f.formNodes(v);});});"
      "var a=f.formNodes(xfa.datasets.data.root),b=f.formNodes(xfa.datasets.data.root);"
      "eq(a.length,0);eq(b.length,0);yes(a!==b);throws(function(){a.item(0);});"
      "eq(clean(r.target.rawValue),'Seed');");
}
TEST(CoverageXfaEventRecovery, FormEventEntryPointsTraverseTheirOwnInitializedChildrenAndRejectExtraArguments) {
  ExerciseEvents("var f=xfa.form;r.audit.rawValue='before';eq(f.execInitialize(),undefined);"
      "eq(clean(r.audit.rawValue),'Initialized');eq(f.execCalculate(),undefined);eq(f.execValidate(),true);"
      "throws(function(){f.execInitialize(1);});throws(function(){f.execCalculate(1);});"
      "throws(function(){f.execValidate(1);});throws(function(){f.remerge(1);});r.audit.rawValue='Complete';",
      Field("target", "Seed", 100, forms::Event("xfa.form.root.audit.rawValue='Initialized';", "initialize")));
}
TEST(CoverageXfaEventRecovery, FormRecalculateChecksArityAndConvertsBothZeroAndNonzeroModesOnce) {
  ExerciseEvents("var f=xfa.form;throws(function(){f.recalculate();});throws(function(){f.recalculate(0,1);});"
      "var calls=0;eq(f.recalculate({valueOf:function(){calls++;return 1;}}),undefined);eq(calls,1);"
      "eq(f.recalculate({valueOf:function(){calls++;return 0;}}),undefined);eq(calls,2);"
      "eq(clean(r.target.rawValue),'Seed');r.audit.rawValue='Complete';");
}
TEST(CoverageXfaEventRecovery, FormChecksumUsesStringCoercionWithoutChangingTheVisibleForm) {
  ExerciseEvents("var f=xfa.form,calls=0;f.checksum={toString:function(){calls++;return 'temporary-checksum';}};"
      "eq(calls,1);eq(f.checksum,'temporary-checksum');f.checksum='';eq(f.checksum,'');"
      "eq(clean(r.target.rawValue),'Seed');r.audit.rawValue='Complete';");
}
TEST(CoverageXfaEventRecovery, TemplateReadinessAndBooleanModelMethodsUseTheirDocumentDefinition) {
  ExerciseEvents("var t=xfa.template;eq(t.execInitialize(),false);eq(t.execCalculate(),false);eq(t.execValidate(),false);"
      "throws(function(){t.execInitialize(1);});throws(function(){t.execCalculate(1);});throws(function(){t.execValidate(1);});"
      "var poison={toString:function(){throw 'unused';},valueOf:function(){throw 'unused';}};"
      "eq(t.formNodes(poison),true);eq(t.recalculate(poison),true);"
      "throws(function(){t.formNodes();});throws(function(){t.formNodes(r,1);});"
      "throws(function(){t.recalculate();});throws(function(){t.recalculate(0,1);});"
      "eq(clean(r.target.rawValue),'Seed');");
}
TEST(CoverageXfaEventRecovery, TemplateXmlAndDetachedDatasetXmlUseTheirPacketSpecificSerializationRules) {
  ExerciseEvents("eq(xfa.template.saveXML(),'');eq(xfa.config.saveXML(),'');"
      "var value=xfa.datasets.createNode('dataValue');"
      "yes(value.saveXML().indexOf('<?xml')===0);"
      "throws(function(){r.target.saveXML('invalid');});throws(function(){r.target.saveXML('pretty',1);});"
      "eq(clean(r.target.rawValue),'Seed');");
}

// Parser failures are entered through real FormCalc event scripts. After the
// failed event, public edits and another independent script must still work.
void BadFormCalc(const std::vector<std::string>& scripts) {
  for (const auto& script : scripts) {
    SCOPED_TRACE(script);
    ExerciseEvents("root.report.rawValue = \"BAD\"\n" + script, Field("target", "Seed", 100), [](Session& s) {
      ASSERT_TRUE(s.Focus(12, 108)); ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page)); s.Replace(u"Recovered");
      EXPECT_EQ(s.Text(), u"Recovered");
    }, JSPLATFORM_ALERT_RETURN_YES, "application/x-formcalc", u"Ready");
  }
}
TEST(CoverageXfaEventRecovery, FormCalcMalformedDeclarationsFunctionsAndReturnsRecoverForPublicEdits) {
  BadFormCalc({"var", "var = 2", "var x =", "func", "func f", "func f(", "func f(a,", "func f(a) do", "func f(a) do 1 endfunc )", "return ("});
}
TEST(CoverageXfaEventRecovery, FormCalcMalformedConditionsAndBlocksRecoverForPublicEdits) {
  BadFormCalc({"if", "if (", "if (1", "if (1) then", "if (1) then 2 elseif", "if (1) then 2 else", "if (1) then 2 endif )", "do", "do 1", "do 1 end )"});
}
TEST(CoverageXfaEventRecovery, FormCalcMalformedLoopsAndForeachRecoverForPublicEdits) {
  BadFormCalc({"while", "while (1", "while (1) do", "for", "for i", "for i=1 upto", "for i=1 upto 3 step", "for i=1 upto 3 do", "foreach", "foreach i in (", "foreach i in (1,2) do"});
}
TEST(CoverageXfaEventRecovery, FormCalcMalformedAccessorsAndArgumentListsRecoverForPublicEdits) {
  BadFormCalc({"root.", "root..", "root.item[", "root.item[+", "root.item[1", "root.item[*", "root.item.(", "root.item(1,", "Abs(", "Abs(1,", "(1+2", "1+", "1 or", "not", "1 eq"});
}
TEST(CoverageXfaEventRecovery, FormCalcMalformedStringsAndLexicalTokensRecoverForPublicEdits) {
  BadFormCalc({"\"unterminated", "@", "#", "1e+", "[1]", "1 ? 2"});
}
TEST(CoverageXfaEventRecovery, FormCalcAdjacentLiteralsAreSeparateStatementsRatherThanParseFailures) {
  for (const auto& expression : std::vector<std::string>{"var v=\"a\" \"b\"\nif(v==\"a\")then", "var v=1.2 .3\nif(Abs(v-1.2)<0.00001)then"}) {
    SCOPED_TRACE(expression);
    ExerciseEvents(expression + " root.report.rawValue=\"PASS\" else root.report.rawValue=\"FAIL\" endif",
        Field("target", "Seed", 100), {}, JSPLATFORM_ALERT_RETURN_YES, "application/x-formcalc");
  }
}
TEST(CoverageXfaEventRecovery, FormCalcIndexedNodeAliasesResolveTheirDefaultValues) {
  ExerciseEvents("root.target.rawValue=\"Seed\"\nvar a=root.target[0]\nroot.audit.rawValue=Concat(a,\"!\")\nif(root.audit.rawValue==\"Seed!\")then root.report.rawValue=\"PASS\" else root.report.rawValue=\"FAIL\" endif",
      Field("target", "Seed", 100), {}, JSPLATFORM_ALERT_RETURN_YES, "application/x-formcalc");
}
TEST(CoverageXfaEventRecovery, NullCalculationClearsItsValueWhileUndefinedCalculationPreservesExistingValue) {
  for (const std::string calculation : {"null;", "undefined;"}) {
    SCOPED_TRACE(calculation);
    ExerciseEvents("r.target.rawValue='Seed';r.target.execCalculate();eq(clean(r.target.rawValue),'" + std::string(calculation == "null;" ? "" : "Seed") + "');",
        Field("target", "Seed", 100, "<calculate><script contentType=\"application/x-javascript\">" + calculation + "</script></calculate>"));
  }
}
TEST(CoverageXfaEventRecovery, FormatAndNullValidationCanRecoverAfterAnEventException) {
  ExerciseEvents("r.target.rawValue='12';r.target.execEvent('click');r.target.rawValue='bad';eq(r.target.execValidate(),false);"
      "r.target.rawValue='56';eq(r.target.execValidate(),true);",
      Field("target", "12", 100, forms::Event("throw 'deliberate';", "click") + Validate("formatTest=\"error\" nullTest=\"disabled\"", "<picture>num{99}</picture>")),
      [](Session& s) { HasMessage(s, u"invalid"); });
}

}  // namespace
}  // namespace pdfoxium::integration
