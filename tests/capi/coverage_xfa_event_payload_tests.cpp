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
                           const std::string& version = "3.3", const std::string& data = "<root><anchor/></root>") {
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
      PdfStream("", "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data>"+data+"</xfa:data></xfa:datasets>"),
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
void PayloadScenario(const std::string& program, const std::string& model = Field("target", "Seed", 100),
         const Inspect& inspect = {}, int answer = JSPLATFORM_ALERT_RETURN_YES,
         const std::string& language = "application/x-javascript", const std::u16string& expected = u"PASS",
         const std::string& version = "3.3", const Inspect& initial = {}, const std::string& data = "<root><anchor/></root>") {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts; std::array<Raster, 2> originals; size_t index = 0;
  const auto fixture = Fixture(program, model, language, version, data);
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
  Compare(originals[0], originals[1], artifacts, "payload-engines");
}

std::string Group(const std::string& events="",const std::string& first_events="") {
  const auto member=[&](const char* name,const char* on,int x,const std::string& handlers){
    return "<field name=\""+std::string(name)+"\" x=\""+std::to_string(x)+"pt\" w=\"80pt\" h=\"22pt\"><ui><checkButton shape=\"round\"/></ui>"
      "<items><text>"+on+"</text><text>Off</text></items><value><text>Off</text></value>"+
      forms::Event("for(var i=0;i<this.items.nodes.length;++i){var n=this.items.nodes.item(i);n.value=String(n.value).replace(/^\\n/,'');}if(typeof this.rawValue==='string')this.rawValue=this.rawValue.replace(/^\\n/,'');","initialize")+handlers+"</field>";};
  return "<exclGroup name=\"choice\" x=\"8pt\" y=\"100pt\" w=\"220pt\" h=\"24pt\"><bind match=\"none\"/>"+
      member("a","A",0,first_events)+member("b","B",100,"")+events+"</exclGroup>";
}

TEST(CoverageXfaEventPayloads, ClickPayloadExposesTheActualDefaultScalarFields) {
  PayloadScenario("var e=xfa.event;eq(e.commitKey,0);eq(e.fullText,'');eq(e.keyDown,false);eq(e.modifier,false);"
    "eq(e.newContentType,'');eq(e.prevContentType,'');eq(e.prevText,'');eq(e.reenter,false);eq(e.shift,false);"
    "eq(e.soapFaultCode,'');eq(e.soapFaultString,'');eq(e.target,undefined);eq(e.cancelAction,false);eq(e.className,'eventPseudoModel');");
}
TEST(CoverageXfaEventPayloads, IgnoredPayloadSettersNeverCoerceTheirSuppliedObjects) {
  PayloadScenario("var e=xfa.event,hits=0,bad={toString:function(){hits++;throw 'coercion';},valueOf:function(){hits++;throw 'coercion';}};"
    "var names=['commitKey','fullText','keyDown','modifier','newContentType','newText','prevContentType','prevText','reenter','shift','soapFaultCode','soapFaultString','target'];"
    "for(var i=0;i<names.length;++i){var before=e[names[i]];e[names[i]]=bad;eq(e[names[i]],before);}eq(hits,0);");
}
TEST(CoverageXfaEventPayloads, ResetClearsMutableEventStateWithoutReplacingTheEventFacade) {
  PayloadScenario("var e=xfa.event;e.change='replacement';e.cancelAction=true;eq(e.change,'replacement');eq(e.cancelAction,true);"
    "e.reset();eq(xfa.event,e);eq(e.change,'');eq(e.cancelAction,false);eq(e.prevText,'');eq(e.selStart,0);eq(e.selEnd,0);eq(e.newText,'');");
}
TEST(CoverageXfaEventPayloads, EmitDispatchesTheCurrentClickWithExplicitBoundedReentry) {
  PayloadScenario("if(clean(r.audit.rawValue)!=='1'){r.audit.rawValue='1';xfa.event.emit();eq(clean(r.audit.rawValue),'2');}else{r.audit.rawValue='2';}");
}
TEST(CoverageXfaEventPayloads, ClassNameIsReadOnlyAndLeavesTheCurrentEventUsable) {
  PayloadScenario("eq(xfa.event.className,'eventPseudoModel');throws(function(){xfa.event.className='other';});"
    "eq(xfa.event.className,'eventPseudoModel');xfa.event.change='usable';eq(xfa.event.change,'usable');");
}
TEST(CoverageXfaEventPayloads, ExtractedMethodsRejectObjectsWithoutAnEventBinding) {
  PayloadScenario("var emit=xfa.event.emit,reset=xfa.event.reset;throws(function(){emit.call({});});throws(function(){reset.call({});});"
    "eq(xfa.event.change,'');xfa.event.reset();eq(xfa.event.change,'');");
}
TEST(CoverageXfaEventPayloads, RealChangeInputClampsEndpointsAndRetainsTheOriginalCandidate) {
  const auto change=forms::Event("if(xfa.event.change==='@'){var e=xfa.event;e.selStart=-9;e.selEnd=999;var a=e.selStart,b=e.selEnd;"
    "e.selEnd=-1;var c=e.selStart,d=e.selEnd;e.selStart=999;var f=e.selStart,g=e.selEnd;"
    "xfa.form.root.audit.rawValue=[a,b,c,d,f,g,e.prevText,e.newText].join('|');e.cancelAction=true;}","change");
  const auto input=[](Session& s){ASSERT_TRUE(s.Focus(12,108));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"abcd");
    EXPECT_EQ(s.Text(),u"abcd");EXPECT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey));
    s.api.FORM_OnChar(s.form,s.page,'@',0);EXPECT_EQ(s.Text(),u"abcd");};
  PayloadScenario("eq(clean(r.audit.rawValue),'0|4|0|0|4|4|abcd|abcd@');eq(clean(r.target.rawValue),'abcd');r.audit.rawValue='Complete';",
    Field("target","Seed",100,change),{},JSPLATFORM_ALERT_RETURN_YES,"application/x-javascript",u"PASS","3.3",input);
}
TEST(CoverageXfaEventPayloads, ExclusionGroupInitializeTraversesItsRealMembers) {
  PayloadScenario("r.audit.rawValue='before';r.choice.execInitialize();eq(clean(r.audit.rawValue),'initialized');r.audit.rawValue='Complete';",
    Group("",forms::Event("xfa.form.root.audit.rawValue='initialized';","initialize")));
}
TEST(CoverageXfaEventPayloads, ExclusionGroupCalculateRunsWithoutTreatingTheContainerAsAnEditWidget) {
  // ProcessCalculate executes the script, but SetValue has no UI child on an
  // exclusion container. Member selection uses the group's rawValue setter.
  PayloadScenario("r.choice.rawValue='A';r.audit.rawValue='before';r.choice.execCalculate();"
    "eq(clean(r.audit.rawValue),'calculated');eq(clean(r.choice.rawValue),'A');eq(r.choice.selectedMember().name,'a');r.audit.rawValue='Complete';",
    Group("<calculate><script contentType=\"application/x-javascript\">xfa.form.root.audit.rawValue='calculated';'B';</script></calculate>"));
}
TEST(CoverageXfaEventPayloads, ExclusionGroupValidateUsesItsOwnValidationScript) {
  PayloadScenario("eq(r.choice.execValidate(),true);r.choice.rawValue='A';eq(r.choice.execValidate(),true);eq(r.choice.selectedMember().name,'a');",
    Group("<validate nullTest=\"disabled\" scriptTest=\"error\"><script contentType=\"application/x-javascript\">true;</script></validate>"));
}
TEST(CoverageXfaEventPayloads, ExclusionGroupExecEventRunsOnlyTheNamedActivity) {
  PayloadScenario("r.audit.rawValue='before';r.choice.execEvent('change');eq(clean(r.audit.rawValue),'changed');"
    "r.audit.rawValue='before';r.choice.execEvent('unknownEvent');eq(clean(r.audit.rawValue),'before');r.audit.rawValue='Complete';",
    Group(forms::Event("xfa.form.root.audit.rawValue='changed';","change")));
}
TEST(CoverageXfaEventPayloads, ExclusionGroupMethodsCheckArityBeforeCoercionOrMutation) {
  PayloadScenario("var g=r.choice,hits=0,bad={toString:function(){hits++;throw 'coercion';}};"
    "throws(function(){g.execEvent();});throws(function(){g.execEvent(bad,1);});throws(function(){g.execInitialize(1);});"
    "throws(function(){g.execCalculate(1);});throws(function(){g.execValidate(1);});throws(function(){g.selectedMember(bad);});eq(hits,0);"
    "g.rawValue='B';eq(g.selectedMember().name,'b');",Group());
}
TEST(CoverageXfaEventPayloads, EmptyExclusionSelectionReturnsNullAndCanBeSelectedAgain) {
  PayloadScenario("var g=r.choice;g.rawValue='A';eq(g.selectedMember().name,'a');g.rawValue=null;eq(g.selectedMember(),null);"
    "eq(g.rawValue,null);g.rawValue='B';eq(g.selectedMember().name,'b');",Group());
}
TEST(CoverageXfaEventPayloads, TemplateExclusionGroupsHaveNoReadyWidgetOrSelectedMember) {
  PayloadScenario("var g=xfa.template.resolveNode('root.choice');yes(g!==null);eq(g.selectedMember(),null);eq(g.rawValue,undefined);"
    "var hits=0;g.rawValue={toString:function(){hits++;throw 'coercion';}};eq(hits,0);eq(g.selectedMember(),null);"
    "r.choice.rawValue='A';eq(r.choice.selectedMember().name,'a');",Group());
}
TEST(CoverageXfaEventPayloads, ExclusionTransientIgnoresWritesWhileErrorTextRejectsThem) {
  PayloadScenario("var g=r.choice;eq(g.transient,undefined);eq(g.errorText,undefined);var hits=0;"
    "var bad={toString:function(){hits++;throw 'coercion';}};g.transient=bad;eq(g.transient,undefined);"
    "throws(function(){g.errorText=bad;});eq(hits,0);g.rawValue='A';eq(g.selectedMember().name,'a');",Group());
}
TEST(CoverageXfaEventPayloads, ImageDefaultValueIsReadOnlyWithoutAttemptingToLoadAnExternalImage) {
  PayloadScenario("var n=r.picture.value.image;eq(n.value,null);throws(function(){n.value='data';});"
    "eq(n.value,null);eq(n.className,'image');",
    Field("picture","",100,"<value><image/></value>","<imageEdit/>"));
}
TEST(CoverageXfaEventPayloads, InstanceIndexMovesBoundRowsAndRetainsEachDatasetIdentity) {
  const std::string rows="<subform name=\"holder\" layout=\"tb\" x=\"8pt\" y=\"100pt\" w=\"220pt\"><bind match=\"none\"/>"
    "<subform name=\"row\" layout=\"position\" w=\"220pt\" h=\"24pt\"><occur min=\"1\" max=\"3\"/><bind match=\"dataRef\" ref=\"$record.rows.row[*]\"/>"
    "<field name=\"value\" w=\"220pt\" h=\"22pt\"><bind match=\"once\"/><ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"12pt\"/>"
    "<value><text>Seed</text></value>"+forms::Event("if(typeof this.rawValue==='string')this.rawValue=this.rawValue.replace(/^\\n/,'');","initialize")+
    "</field></subform></subform>";
  PayloadScenario("var rows=r.holder.resolveNodes('row[*]');eq(rows.length,2);var a=rows.item(0),b=rows.item(1),d=a.value.dataNode;"
    "eq(clean(a.value.rawValue),'A');eq(clean(b.value.rawValue),'B');a.instanceIndex=1;eq(a.instanceIndex,1);eq(a.value.dataNode,d);"
    "eq(clean(a.value.rawValue),'A');eq(r.holder.resolveNodes('row[*]').item(0).value.rawValue,'B');"
    "a.instanceIndex=0;eq(a.instanceIndex,0);eq(a.value.dataNode,d);",
    rows,{},JSPLATFORM_ALERT_RETURN_YES,"application/x-javascript",u"PASS","3.3",{},
    "<root><rows><row><value>A</value></row><row><value>B</value></row></rows></root>");
}
TEST(CoverageXfaEventPayloads, DescriptionUseWithoutAPrototypeRootPreservesExistingContent) {
  PayloadScenario("var d=r.target.desc;eq(clean(d.text.value),'Description');d.use='#missing';eq(clean(d.text.value),'Description');",
    Field("target","Seed",100,"<desc><text>Description</text></desc>"));
}
TEST(CoverageXfaEventPayloads, DescriptionMissingPrototypeReferencesPreserveExistingContent) {
  PayloadScenario("var d=r.target.desc;d.use='#missing';eq(clean(d.text.value),'Description');d.use='unknown';"
    "eq(clean(d.text.value),'Description');d.use='';eq(clean(d.text.value),'Description');",
    "<proto><desc id=\"available\"><text>Prototype</text></desc></proto>"+Field("target","Seed",100,"<desc><text>Description</text></desc>"));
}
TEST(CoverageXfaEventPayloads, BooleanValueCoercionUsesNumericContentAndClearsNullOrUndefined) {
  PayloadScenario("var b=r.target.value.boolean;b.value=0;eq(b.value,false);b.value=-9;eq(b.value,true);"
    "b.value='17';eq(b.value,true);b.value=null;eq(b.value,false);b.value=1;eq(b.value,true);b.value=undefined;eq(b.value,false);",
    Field("target","",100,"<value><boolean>1</boolean></value>"));
}
TEST(CoverageXfaEventPayloads, ModelContextAndAliasNodeIgnoreCoercionWithoutChangingData) {
  PayloadScenario("var m=xfa.datasets,hits=0,bad={toString:function(){hits++;throw 'coercion';}};"
    "eq(m.context,undefined);eq(m.aliasNode,undefined);m.context=bad;m.aliasNode=bad;eq(hits,0);"
    "eq(m.context,undefined);eq(m.aliasNode,undefined);r.target.rawValue='Usable';eq(clean(r.target.rawValue),'Usable');");
}

}  // namespace
}  // namespace pdfoxium::integration
