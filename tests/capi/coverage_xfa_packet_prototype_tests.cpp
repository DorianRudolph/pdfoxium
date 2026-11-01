#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
std::string Escape(const std::string& s) {
  std::string result;for(char c:s) {if(c=='&')result+="&amp;";else if(c=='<')result+="&lt;";else if(c=='>')result+="&gt;";else result+=c;}return result;
}
struct Spec {std::string packets,program,body,preamble_attributes,template_instructions;};
std::vector<uint8_t> Fixture(const Spec& spec) {
  const auto program="function eq(a,b){if(a!==b)throw 'expected '+String(b)+' got '+String(a);}"
      "function yes(v){if(!v)throw 'assertion';}"
      "function clean(v){return v===null?'':String(v).replace(/^\\n/,'');}"
      "function packet(name,index){var count=0;for(var i=0;i<xfa.nodes.length;++i){var p=xfa.nodes.item(i);"
      "if(p.name===name){if(count===(index||0))return p;++count;}}throw 'missing packet '+name;}"
      "try{var r=xfa.form.root,s=r.body.resolveNode('sample');"+spec.program+";xfa.form.remerge();xfa.host.messageBox('PASS');}catch(e){xfa.host.messageBox('FAIL:'+String(e));}";
  const auto xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"+spec.template_instructions+
      "<subform name=\"root\" layout=\"tb\" restoreState=\"auto\">"
      "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"240pt\" long=\"240pt\"/>"
      "<contentArea w=\"240pt\" h=\"200pt\"/></pageArea></pageSet>"
      "<subform name=\"controls\" layout=\"position\" w=\"240pt\" h=\"20pt\"><field name=\"run\" x=\"190pt\" w=\"20pt\" h=\"12pt\">"
      "<bind match=\"none\"/><ui><button/></ui><border><edge presence=\"hidden\"/></border>"
      "<event activity=\"click\"><script contentType=\"application/x-javascript\">"+Escape(program)+"</script></event></field></subform>"
      "<subform name=\"body\" layout=\"position\" w=\"180pt\" h=\"40pt\"><bind match=\"none\"/>"+spec.body+
      "<draw name=\"paint\" w=\"60pt\" h=\"20pt\"><value><rectangle><fill><color value=\"0,128,0\"/></fill>"
      "<edge presence=\"hidden\"/></rectangle></value></draw></subform></subform></template>";
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>","<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 240] >>",
      "<< /Fields [] /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\" "+spec.preamble_attributes+">"+spec.packets),
      PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("",xml),PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
      PdfStream("","</xdp:xdp>")});
}
struct Loaded {forms::XfaCallbacks callbacks;Session s;
  Loaded(const Library& library,const std::vector<uint8_t>& bytes):s(library.api(),bytes,true,true,2,[this](forms::FormHost& h){callbacks.Configure(h);}) {}
};
void Execute(Session& s) {
  s.host->alerts.clear();s.api.FORM_ForceToKillFocus(s.form);
  ASSERT_EQ(s.api.FPDFPage_HasFormFieldAtPoint(s.form,s.page,195,5),FPDF_FORMFIELD_XFA_PUSHBUTTON);
  s.Click(195,5);for(size_t i=0;i<s.host->pages.size();++i)s.ClosePage(static_cast<int>(i));
  s.page=s.host->Page(0);ASSERT_NE(s.page,nullptr);
  ASSERT_EQ(s.host->alerts.size(),1u);
  EXPECT_EQ(std::get<0>(s.host->alerts[0]),u"PASS");
  s.api.FORM_ForceToKillFocus(s.form);s.api.FORM_OnMouseMove(s.form,s.page,0,230,180);
  EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
}
Raster Render(Session& s) {
  EXPECT_EQ(s.api.FPDF_GetPageCount(s.document),1);auto image=s.Draw(240,240);
  size_t green=0;for(size_t i=0;i<image.pixels.size();i+=4)green+=image.pixels[i]==0&&image.pixels[i+1]==128&&image.pixels[i+2]==0;
  EXPECT_EQ(green,1200u);return image;
}
void Scenario(const std::string& label,const Spec& spec) {
  const auto bytes=Fixture(spec);Artifacts artifacts;artifacts.Pdf(label+"-input",bytes);
  std::array<Raster,2> originals;size_t index=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Loaded loaded(*writer,bytes);auto& s=loaded.s;
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(Execute(s));originals[index]=Render(s);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+label);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Loaded reopened(*reader,saved);ASSERT_TRUE(reopened.s.valid());ASSERT_TRUE(reopened.s.xfa_loaded);
      ASSERT_NO_FATAL_FAILURE(Execute(reopened.s));CompareRaster(originals[index],Render(reopened.s),artifacts,writer->name()+reader->name()+label);
    }++index;
  }CompareRaster(originals[0],originals[1],artifacts,label+"-engines");
}
// Opaque packets retain their XML mapping and attributes without interpreting
// arbitrary XML children as XFA model nodes. CJX_Packet::content reads only
// immediate XML text children (CFX_XMLElement::GetTextData). No resource is fetched.
void Opaque(const std::string& label,const std::string& packets,const std::string& name,const std::string& value="Seed") {
  Spec s;s.packets=packets;s.program="var p=packet('"+name+"');eq(p.className,'packet');eq(p.nodes.length,0);eq(p.content,'"+value+"');"
      "eq(p.getAttribute('marker'),'original');p.setAttribute('changed','marker');eq(p.getAttribute('marker'),'changed');"
      "p.removeAttribute('marker');eq(p.getAttribute('marker'),'');p.setAttribute('original','marker');";
  Scenario(label,s);
}

TEST(CoverageXfaPacketPrototype, CustomPacketDoesNotFlattenNestedXmlIntoItsScalarContent) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Opaque("CustomPacketDoesNotFlattenNestedXmlIntoItsScalarContent","<custom xmlns=\"urn:pdfoxium:custom\" marker=\"original\"><payload>Seed</payload></custom>","custom","");
}

TEST(CoverageXfaPacketPrototype, PrefixedCustomPacketUsesItsLocalName) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Opaque("PrefixedCustomPacketUsesItsLocalName","<p:custom xmlns:p=\"urn:pdfoxium:custom\" marker=\"original\">Seed</p:custom>","custom","Seed");
}

TEST(CoverageXfaPacketPrototype, CustomPacketPreservesCharacterData) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Opaque("CustomPacketPreservesCharacterData","<custom xmlns=\"urn:pdfoxium:custom\" marker=\"original\"><![CDATA[Seed]]></custom>","custom","Seed");
}

TEST(CoverageXfaPacketPrototype, XmpMetadataPacketRemainsOpaque) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Opaque("XmpMetadataPacketRemainsOpaque","<xmpmeta xmlns=\"http://ns.adobe.com/xmpmeta/\" marker=\"original\">Seed<payload>Nested content</payload></xmpmeta>","xmpmeta","Seed");
}

TEST(CoverageXfaPacketPrototype, StylesheetPacketRemainsOpaque) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Opaque("StylesheetPacketRemainsOpaque","<stylesheet xmlns=\"http://www.w3.org/1999/XSL/Transform\" marker=\"original\">Seed<payload>Nested content</payload></stylesheet>","stylesheet","Seed");
}

TEST(CoverageXfaPacketPrototype, SignaturePacketRemainsOpaque) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Opaque("SignaturePacketRemainsOpaque","<signature xmlns=\"http://www.w3.org/2000/09/xmldsig#\" marker=\"original\">Seed<payload>Nested content</payload></signature>","signature","Seed");
}

TEST(CoverageXfaPacketPrototype, XfdfPacketRemainsOpaque) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Opaque("XfdfPacketRemainsOpaque","<xfdf xmlns=\"http://ns.adobe.com/xfdf/\" marker=\"original\">Seed<payload>Nested content</payload></xfdf>","xfdf","Seed");
}

TEST(CoverageXfaPacketPrototype, PdfPacketWithAnotherNamespaceIsClassifiedAsAnOpaqueUserPacket) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Opaque("PdfPacketWithAnotherNamespaceIsClassifiedAsAnOpaqueUserPacket","<pdf xmlns=\"urn:pdfoxium:custom\" marker=\"original\">Seed</pdf>","pdf","Seed");
}

TEST(CoverageXfaPacketPrototype, MultipleXmpMetadataPacketsRetainSeparateXmlMappings) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.packets="<xmpmeta marker=\"first\">One</xmpmeta><xmpmeta marker=\"second\">Two</xmpmeta>";
  s.program="var a=packet('xmpmeta',0),b=packet('xmpmeta',1);yes(a!==b);eq(a.className,'packet');eq(b.className,'packet');"
      "eq(a.content,'One');eq(b.content,'Two');eq(a.getAttribute('marker'),'first');eq(b.getAttribute('marker'),'second');";
  Scenario("two-xmp-packets",s);
}

TEST(CoverageXfaPacketPrototype, XdcPacketLoadsAlongsideTheTemplateWithoutInterpretation) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.packets="<xdc xmlns=\"http://www.xfa.org/schema/xdc/1.0/\"><payload>Opaque</payload></xdc>";
  s.program="var p=packet('xdc');eq(p.className,'xdc');eq(p.name,'xdc');";Scenario("xdc-packet",s);
}

TEST(CoverageXfaPacketPrototype, ConnectionSetPacketBuildsNamedConnectionsWithoutOpeningThem) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.packets="<connectionSet xmlns=\"http://www.xfa.org/schema/xfa-connection-set/2.8/\"><xmlConnection name=\"offline\"><uri>urn:pdfoxium:offline</uri></xmlConnection></connectionSet>";
  s.program="var p=packet('connectionSet');eq(p.className,'connectionSet');eq(p.nodes.length,1);eq(p.nodes.item(0).className,'xmlConnection');eq(p.nodes.item(0).name,'offline');";
  Scenario("connection-packet",s);
}

TEST(CoverageXfaPacketPrototype, SourceSetPacketLoadsAnEmptyLocalSourceModel) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.packets="<sourceSet xmlns=\"http://www.xfa.org/schema/xfa-source-set/2.8/\"/>";
  s.program="var p=packet('sourceSet');eq(p.className,'sourceSet');eq(p.name,'sourceSet');eq(p.nodes.length,0);";Scenario("source-packet",s);
}

TEST(CoverageXfaPacketPrototype, XdpUuidAndTimeStampAreRecognizedRootAttributes) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.preamble_attributes="uuid=\"8f09f225-35ad-45ec-8d6f-62ee8ed0c001\" timeStamp=\"2026-01-02T03:04:05Z\"";
  s.program="eq(xfa.getAttribute('uuid'),'8f09f225-35ad-45ec-8d6f-62ee8ed0c001');eq(xfa.getAttribute('timeStamp'),'2026-01-02T03:04:05Z');";
  Scenario("xdp-root-attributes",s);
}

TEST(CoverageXfaPacketPrototype, NestedXdpPacketDoesNotReplaceTheOuterDocumentTemplate) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.packets="<xdp:xdp><ignored>Seed</ignored></xdp:xdp>";
  s.program="eq(xfa.template.root.className,'subform');eq(xfa.template.root.name,'root');";Scenario("nested-xdp-ignored",s);
}

TEST(CoverageXfaPacketPrototype, OriginalVersionInstructionAllowsTheExistingJavaScriptEngine) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.template_instructions="<?originalXFAVersion http://www.xfa.org/schema/xfa-template/2.7 v2.7-scripting:1 ?>";
  s.program="eq(6*7,42);eq(xfa.template.root.name,'root');";Scenario("original-version-instruction",s);
}

TEST(CoverageXfaPacketPrototype, AcrobatStrictScopingInstructionPreservesExplicitModelReferences) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.template_instructions="<?acrobat JavaScript strictScoping ?>";
  s.program="eq(xfa.form.root.name,'root');eq(xfa.template.root.name,'root');";Scenario("strict-scoping-instruction",s);
}

// Property nodes merge by element and name; local attributes/children remain
// local while absent properties are inherited through the prototype link.
void Prototype(const std::string& label,const std::string& base_properties,const std::string& local_properties,const std::string& program) {
  Spec s;s.body="<proto><field id=\"baseField\" name=\"base\" presence=\"invisible\" w=\"60pt\" h=\"20pt\"><bind match=\"none\"/>"
      "<ui><textEdit/></ui>"+base_properties+"</field></proto><field name=\"sample\" use=\"#baseField\" presence=\"invisible\">"+local_properties+"</field>";
  s.program="yes(s!==null);eq(s.className,'field');"+program;Scenario(label,s);
}

TEST(CoverageXfaPacketPrototype, PrototypeFontAttributesAreInheritedIndependently) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Prototype("PrototypeFontAttributesAreInheritedIndependently","<font typeface=\"Courier\" size=\"9pt\" weight=\"bold\"/>","","eq(s.font.typeface,'Courier');eq(s.font.weight,'bold');eq(s.font.size,'9pt');");
}

TEST(CoverageXfaPacketPrototype, LocalFontSizeRetainsThePrototypeTypeface) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Prototype("LocalFontSizeRetainsThePrototypeTypeface","<font typeface=\"Courier\" size=\"9pt\"/>","<font size=\"16pt\"/>","eq(s.font.typeface,'Courier');eq(s.font.size,'16pt');");
}

TEST(CoverageXfaPacketPrototype, PrototypeParagraphAttributesReachTheMergedField) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Prototype("PrototypeParagraphAttributesReachTheMergedField","<para hAlign=\"right\" vAlign=\"middle\"/>","","eq(s.para.hAlign,'right');eq(s.para.vAlign,'middle');");
}

TEST(CoverageXfaPacketPrototype, LocalParagraphAlignmentRetainsTheOtherPrototypeAttributes) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Prototype("LocalParagraphAlignmentRetainsTheOtherPrototypeAttributes","<para hAlign=\"right\" vAlign=\"middle\"/>","<para hAlign=\"center\"/>","eq(s.para.hAlign,'center');eq(s.para.vAlign,'middle');");
}

TEST(CoverageXfaPacketPrototype, PrototypeMarginInsetsReachTheMergedField) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Prototype("PrototypeMarginInsetsReachTheMergedField","<margin leftInset=\"3pt\" topInset=\"4pt\" rightInset=\"5pt\" bottomInset=\"6pt\"/>","","eq(s.margin.leftInset,'3pt');eq(s.margin.bottomInset,'6pt');");
}

TEST(CoverageXfaPacketPrototype, PrototypeDefaultValueIsAvailableWithoutDatasetBinding) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Prototype("PrototypeDefaultValueIsAvailableWithoutDatasetBinding","<value><text>Seed</text></value>","","eq(clean(s.rawValue),'Seed');eq(s.dataNode,null);");
}

TEST(CoverageXfaPacketPrototype, LocalValueOverridesThePrototypeValue) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Prototype("LocalValueOverridesThePrototypeValue","<value><text>Seed</text></value>","<value><text>Local</text></value>","eq(clean(s.rawValue),'Local');eq(s.dataNode,null);");
}

TEST(CoverageXfaPacketPrototype, PrototypeAssistPropertiesRemainIndependentOfTheLocalTooltip) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Prototype("PrototypeAssistPropertiesRemainIndependentOfTheLocalTooltip","<assist><toolTip>Base tip</toolTip><speak>Base speech</speak></assist>","<assist><toolTip>Local tip</toolTip></assist>","eq(clean(s.assist.toolTip.value),'Local tip');eq(clean(s.assist.speak.value),'Base speech');");
}

// DataValue.loadXML selects DocumentBuilder's content-value construction path,
// unlike the ordinary data-group imports already exercised by row merging.
// Empty input creates the child without a Value attribute, so its JS value is null.
void ImportValue(const std::string& label,const std::string& xml,bool ignore_root,const std::string& tag,const std::string& expected) {
  Spec s;s.program="var d=xfa.datasets,parent=d.data.root,n=d.createNode('dataValue','scratch');parent.nodes.append(n);"
      "n.loadXML('"+xml+"',"+(ignore_root?"true":"false")+",true);eq(n.nodes.length,1);"
      "eq(n.nodes.item(0).name,'"+tag+"');"+
      (expected.empty()?"eq(n.nodes.item(0).value,null);":"eq(String(n.nodes.item(0).value),'"+expected+"');")+"parent.nodes.remove(n);";
  Scenario(label,s);
}
TEST(CoverageXfaPacketPrototype, ScalarLoadXmlConvertsNestedXmlTextIntoOneValueChild) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ImportValue("scalar-nested-xml","<payload><line>One</line><line>Two</line></payload>",false,"payload","OneTwo");
}
TEST(CoverageXfaPacketPrototype, ScalarLoadXmlPreservesRichTextLineBreaks) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ImportValue("scalar-rich-break","<body xmlns=\"http://www.w3.org/1999/xhtml\">One<br/>Two</body>",false,"body","One\\nTwo");
}
TEST(CoverageXfaPacketPrototype, ScalarLoadXmlSkipsProcessingInstructionsBeforeTheValueElement) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ImportValue("scalar-instruction","<holder><?acrobat JavaScript strictScoping ?><payload>Seed</payload></holder>",true,"payload","Seed");
}
TEST(CoverageXfaPacketPrototype, ScalarLoadXmlKeepsAnEmptyRichTextElementAsAnEmptyValue) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ImportValue("scalar-empty-rich","<body xmlns=\"http://www.w3.org/1999/xhtml\"/>",false,"body","");
}

}  // namespace
}  // namespace pdfoxium::integration
