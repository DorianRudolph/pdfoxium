#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;

std::string MergeField(const std::string& name,const std::string& initial="Seed",
                       const std::string& binding="<bind match=\"once\"/>",
                       const std::string& value_kind="text",const std::string& ui="<textEdit/>",
                       const std::string& value_attributes="") {
  return "<field name=\""+name+"\" w=\"180pt\" h=\"20pt\">"+binding+"<ui>"+ui+
      "</ui><font typeface=\"Helvetica\" size=\"10pt\"/><value><"+value_kind+value_attributes+">"+initial+
      "</"+value_kind+"></value>"+
      // Native XML saving adds a known leading LF to text-bearing elements.
      // These fixtures have no intentional leading LF; normalize only that one
      // prefix through the real field setter before its edit widget is cached.
      forms::Event("if(typeof this.rawValue==='string'&&this.rawValue.charAt(0)==='\\n')this.rawValue=this.rawValue.substring(1);","initialize")+
      "</field>";
}
std::string MergeContainer(const std::string& name,const std::string& fields,
                           const std::string& binding="<bind match=\"none\"/>",
                           const std::string& properties="") {
  return "<subform name=\""+name+"\" layout=\"tb\" w=\"180pt\">"+binding+properties+fields+"</subform>";
}
std::string MergeChecked(const std::string& body) {
  return "function eq(a,b){if(a===b)return;if(Array.isArray(a)&&Array.isArray(b)&&JSON.stringify(a)===JSON.stringify(b))return;throw 'expected '+String(b)+' got '+String(a);}"
      "function yes(v){if(!v)throw 'assertion';}function text(n){return n.rawValue===null?'':String(n.rawValue);}"
      "function near(a,b){if(!isFinite(a)||!isFinite(b)||Math.abs(a-b)>0.00001)throw 'expected '+b+' got '+a;}"
      "function clean(v){return v===null?'':String(v).replace(/^\\n/,'');}"
      "function same(a,b){yes(a&&b);eq(a.className,b.className);eq(a.somExpression,b.somExpression);}"
      "try{this.caption.value.text.value='Run';var r=xfa.form.root,b=r.body,d=xfa.datasets.data;"
      "var firstLoad=String(r.report.rawValue).indexOf('new:')===0,rustBackend=String(r.report.rawValue).indexOf('rust')>=0;"+body+
      ";r.report.rawValue='PASS';}catch(e){xfa.form.root.report.rawValue='FAIL';xfa.host.messageBox(String(e));}";
}
void MergeAlignCdata(std::string& xml,size_t prefix) {
  size_t at=0;
  while((at=xml.find("<![CDATA[",at))!=std::string::npos) {
    if((prefix+at)%1024>1015){xml.insert(at,"        ");at+=8;}
    auto end=xml.find("]]>",at+9);if(end==std::string::npos)break;
    while((prefix+end)%1024>1021){xml.insert(end," ");++end;}
    at=end+3;
  }
}
std::vector<uint8_t> MergeFixture(const std::string& fields,const std::string& data,const std::string& program,
                                 const std::string& description="",int datasets_mode=0) {
  const std::string preamble="<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
  const std::string config="<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender>"
      "</acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
  std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"tb\" locale=\"en_US\" restoreState=\"auto\">"
      "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"200pt\" long=\"280pt\"/>"
      "<contentArea w=\"200pt\" h=\"280pt\"/></pageArea></pageSet>"
      "<draw w=\"40pt\" h=\"20pt\"><value><rectangle><fill><color value=\"255,0,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>"+
      MergeField("report","Ready","<bind match=\"none\"/>")+
      "<field name=\"run\" w=\"80pt\" h=\"20pt\"><bind match=\"none\"/><ui><button/></ui>"
      "<font typeface=\"Helvetica\" size=\"10pt\"/><caption><value><text>Run</text></value></caption>"+
      forms::Event(MergeChecked(program),"click")+"</field>"+MergeContainer("body",fields)+
      "<draw name=\"end\" w=\"40pt\" h=\"10pt\"><value><rectangle><fill><color value=\"0,128,0\"/></fill>"
      "<edge presence=\"hidden\"/></rectangle></value></draw></subform></template>";
  MergeAlignCdata(xml,preamble.size()+config.size());
  const std::string datasets="<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">"+
      description+(datasets_mode==2?"":"<xfa:data xfa:dataNode=\"dataGroup\">"+data+"</xfa:data>")+"</xfa:datasets>";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 280] /Resources << /Font << /Helv 10 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 10 Tf 0 g) /DR << /Font << /Helv 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R "+
          std::string(datasets_mode==1?"":"(datasets) 8 0 R ")+"(postamble) 9 0 R] >>",
      PdfStream("",preamble),PdfStream("",config),PdfStream("",xml),PdfStream("",datasets),PdfStream("","</xdp:xdp>"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"});
}
void ExecuteMerge(Session& s,const Library& owner,bool first,const std::u16string& first_value,bool inspect_first) {
  const std::string seed=std::string(first?"new:":"")+owner.name();
  ASSERT_TRUE(s.Focus(8,30));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(std::u16string(seed.begin(),seed.end()));
  s.api.FORM_ForceToKillFocus(s.form);s.host->alerts.clear();s.Click(8,50);s.api.FORM_ForceToKillFocus(s.form);
  ASSERT_TRUE(s.Focus(8,30));std::string errors;
  for(const auto& alert:s.host->alerts)errors+=std::string(std::get<0>(alert).begin(),std::get<0>(alert).end())+"\n";
  EXPECT_EQ(s.Text(),u"PASS")<<errors;EXPECT_TRUE(s.host->alerts.empty())<<errors;s.api.FORM_ForceToKillFocus(s.form);
  if(inspect_first){ASSERT_TRUE(s.Focus(8,70));EXPECT_EQ(s.Text(),first_value);s.api.FORM_ForceToKillFocus(s.form);}
  EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
}
void InspectMergeRaster(const Raster& image) {
  size_t red=0,green=0;
  for(size_t at=0;at<image.pixels.size();at+=4) {
    if(image.pixels[at]==0&&image.pixels[at+1]==0&&image.pixels[at+2]==255)++red;
    if(image.pixels[at]==0&&image.pixels[at+1]==128&&image.pixels[at+2]==0)++green;
  }
  EXPECT_EQ(red,800u);EXPECT_EQ(green,400u);forms::ExpectInk(image,0,20,180,40);
}
void CompareMerge(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,RasterTolerance{24,0,.25});
}
void MergeScenario(const std::string& fields,const std::string& data,const std::string& program,
                   const std::u16string& first_value=u"Changed",bool inspect_first=true,
                   const std::string& description="",int mode=0) {
  Artifacts artifacts;const auto bytes=MergeFixture(fields,data,program,description,mode);std::array<Raster,2> scenes;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Session s(writer->api(),bytes,true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    ASSERT_EQ(s.api.FPDF_GetPageCount(s.document),1);const auto before=s.Draw(200,280);
    ASSERT_NO_FATAL_FAILURE(ExecuteMerge(s,*writer,true,first_value,inspect_first));scenes[backend]=s.Draw(200,280);InspectMergeRaster(scenes[backend]);
    EXPECT_NE(CropRaster(before,0,20,180,20).pixels,CropRaster(scenes[backend],0,20,180,20).pixels);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-data-merge");ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Session reload(reader->api(),saved,true,true);ASSERT_TRUE(reload.valid());ASSERT_TRUE(reload.xfa_loaded);
      ASSERT_EQ(reload.api.FPDF_GetPageCount(reload.document),1);(void)reload.Draw(200,280);
      ASSERT_NO_FATAL_FAILURE(ExecuteMerge(reload,*reader,false,first_value,inspect_first));const auto image=reload.Draw(200,280);InspectMergeRaster(image);
      CompareMerge(scenes[backend],image,artifacts,writer->name()+reader->name()+"-data-merge");
    }
    ++backend;
  }
  CompareMerge(scenes[0],scenes[1],artifacts,"data-merge-engines");
}

TEST(CoverageXfaDataMerge, MissingDatasetsOrDataPacketsAreSynthesizedFromRealTemplateDefaults) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int mode=0;mode<3;++mode) {
    SCOPED_TRACE(mode);
    MergeScenario(MergeField("created","Seed"),"",R"JS(
      yes(d);yes(d.root);yes(b.created.dataNode);eq(b.created.dataNode.name,'created');
      if(firstLoad){eq(text(b.created),'Seed');eq(clean(b.created.dataNode.value),'Seed');}
      b.created.rawValue='Changed';eq(clean(b.created.dataNode.value),'Changed');
    )JS",u"Changed",true,"",mode);
  }
}

TEST(CoverageXfaDataMerge, GlobalBindingsFindDirectAndNestedRecordValuesInDepthFirstOrder) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Row {const char* data;const char* initial;};
  for(const auto& row:std::array<Row,3>{{
    {"<root shared=\"Direct\"><branch shared=\"Nested\"><anchor/></branch></root>","Direct"},
    {"<root><branch shared=\"Nested\"><anchor/></branch></root>","Nested"},
    {"<root><first><deep shared=\"First\"><anchor/></deep></first><later shared=\"Later\"><anchor/></later></root>","First"}}}) {
    SCOPED_TRACE(row.initial);
    const auto fields=MergeContainer("left",MergeField("shared","Seed","<bind match=\"global\"/>"))+
        MergeContainer("right",MergeField("shared","Seed","<bind match=\"global\"/>"));
    MergeScenario(fields,row.data,"if(firstLoad){eq(text(b.left.shared),'"+std::string(row.initial)+"');eq(text(b.right.shared),'"+row.initial+"');}"
      "same(b.left.shared.dataNode,b.right.shared.dataNode);b.left.shared.rawValue='Changed';eq(text(b.right.shared),'Changed');"
      "eq(clean(b.left.shared.dataNode.value),'Changed');");
  }
}

TEST(CoverageXfaDataMerge, MissingGlobalValuesCreateOneSharedRecordNodeRatherThanContainerLocalCopies) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto fields=MergeContainer("left",MergeField("shared","Seed","<bind match=\"global\"/>"))+
      MergeContainer("right",MergeField("shared","Seed","<bind match=\"global\"/>"));
  MergeScenario(fields,"<root><anchor/></root>",R"JS(
    yes(b.left.shared.dataNode);same(b.left.shared.dataNode,b.right.shared.dataNode);
    eq(b.left.shared.dataNode.parent.name,'root');eq(d.root.resolveNodes('shared[*]').length,1);
    if(firstLoad){eq(text(b.left.shared),'Seed');eq(text(b.right.shared),'Seed');}
    b.right.shared.rawValue='Changed';eq(text(b.left.shared),'Changed');eq(clean(d.root.shared.value),'Changed');
  )JS");
}

TEST(CoverageXfaDataMerge, OnceBindingsConsumeDistinctSameNamedValuesWithoutStealingTheirSiblings) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int count:{2,3}) {
    SCOPED_TRACE(count);std::string fields,data="<root>";
    for(int i=0;i<count;++i){fields+=MergeField("item","Seed");data+="<item>"+std::string(1,static_cast<char>('A'+i))+"</item>";}
    data+="</root>";
    MergeScenario(fields,data,"var nodes=b.resolveNodes('item[*]');eq(nodes.length,"+std::to_string(count)+
      ");if(firstLoad){for(var i=0;i<nodes.length;++i)eq(text(nodes.item(i)),String.fromCharCode(65+i));}"
      "yes(nodes.item(0).dataNode.somExpression!==nodes.item(1).dataNode.somExpression);"
      "nodes.item(0).rawValue='Changed';eq(text(nodes.item(1)),'B');eq(clean(nodes.item(0).dataNode.value),'Changed');");
  }
}

TEST(CoverageXfaDataMerge, ExplicitDataReferencesBindMismatchedFieldNamesToTheRequestedMetadata) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const auto* prefix:{"$record.values.","$data.root.values.","xfa.datasets.data.root.values."}) {
    SCOPED_TRACE(prefix);
    const auto fields=MergeField("display","Seed","<bind match=\"dataRef\" ref=\""+std::string(prefix)+"first\"/>")+
        MergeField("other","Seed","<bind match=\"dataRef\" ref=\""+std::string(prefix)+"second\"/>");
    MergeScenario(fields,"<root><values first=\"One\" second=\"Two\"><anchor/></values></root>",R"JS(
      if(firstLoad){eq(text(b.display),'One');eq(text(b.other),'Two');}
      same(b.display.dataNode,d.root.values.first);same(b.other.dataNode,d.root.values.second);
      b.display.rawValue='Changed';eq(text(b.other),'Two');eq(clean(d.root.values.first.value),'Changed');
    )JS");
  }
}

TEST(CoverageXfaDataMerge, RepeatingDataReferenceContainersBindChildValuesAndKeepUnboundDefaultsLocal) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int count:{2,3}) {
    SCOPED_TRACE(count);std::string data="<root><rows>";
    for(int i=0;i<count;++i)data+="<item label=\""+std::string(1,static_cast<char>('A'+i))+"\"><anchor/></item>";
    data+="</rows></root>";
    const auto row=MergeContainer("row",MergeField("label")+MergeField("local","Local","<bind match=\"none\"/>"),
      "<bind match=\"dataRef\" ref=\"$record.rows.item[*]\"/>","<occur min=\"1\" max=\"3\" initial=\"1\"/>");
    MergeScenario(row,data,"var rows=b.resolveNodes('row[*]');eq(rows.length,"+std::to_string(count)+
      ");for(var i=0;i<rows.length;++i){var row=rows.item(i);if(firstLoad)eq(text(row.label),String.fromCharCode(65+i));"
      "eq(row.local.dataNode,null);eq(text(row.local),'Local');eq(row.dataNode.name,'item');"
      "row.label.rawValue='R'+i;eq(clean(row.dataNode.label.value),'R'+i);}",u"R0");
  }
}

TEST(CoverageXfaDataMerge, ContainerOccurrenceBoundsHandleZeroMinimumPaddingAndMaximumTruncation) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Row {int data,min,initial,max,expected;};
  for(const auto& test:std::array<Row,4>{{{0,0,0,0,0},{1,2,2,4,2},{3,1,1,2,2},{3,0,0,4,3}}}) {
    SCOPED_TRACE("data="+std::to_string(test.data)+" min="+std::to_string(test.min)+" max="+std::to_string(test.max));
    std::string data="<root><rows xfa:dataNode=\"dataGroup\">";
    for(int i=0;i<test.data;++i)data+="<item label=\""+std::string(1,static_cast<char>('A'+i))+"\"><anchor/></item>";
    data+="</rows></root>";
    const auto field=MergeContainer("row",MergeField("label","Seed"),"<bind match=\"dataRef\" ref=\"$record.rows.item[*]\"/>",
      "<occur min=\""+std::to_string(test.min)+"\" initial=\""+std::to_string(test.initial)+"\" max=\""+std::to_string(test.max)+"\"/>");
    MergeScenario(field,data,"var rows=b.resolveNodes('row[*]');eq(rows.length,"+std::to_string(test.expected)+
      ");for(var i=0;i<rows.length;++i){rows.item(i).label.rawValue='R'+i;eq(clean(rows.item(i).label.dataNode.value),'R'+i);}",
      u"R0",test.expected!=0);
  }
}

TEST(CoverageXfaDataMerge, TemplatePrototypeMergesPreserveLocalValuesAndIndependentFormChildren) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const auto* reference:{"use=\"#basePrototype\"","usehref=\".#basePrototype\"","use=\"baseField\""}) {
    SCOPED_TRACE(reference);
    const std::string base="<field name=\"baseField\" id=\"basePrototype\" w=\"140pt\" h=\"20pt\"><bind match=\"none\"/>"
        "<ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"10pt\"/><value><text>Prototype</text></value></field>";
    const auto fields=base+"<field name=\"inherited\" "+reference+"/><field name=\"localOverride\" "+reference+
        " w=\"90pt\"><value><text>Local</text></value></field>";
    MergeScenario(fields,"<root><anchor/></root>",R"JS(
      if(firstLoad){eq(text(b.baseField),'Prototype');eq(text(b.inherited),'Prototype');eq(text(b.localOverride),'Local');}
      eq(b.inherited.w,'140pt');eq(b.localOverride.w,'90pt');yes(b.inherited.ui.textEdit);yes(b.localOverride.ui.textEdit);
      b.baseField.rawValue='Changed';eq(text(b.inherited),'Prototype');eq(text(b.localOverride),'Local');
      yes(b.baseField.value.somExpression!==b.inherited.value.somExpression);
    )JS");
  }
}

TEST(CoverageXfaDataMerge, NumericTemplateDefaultsExportThroughTheActualDecimalOrFloatBindingPath) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Row {const char* input;const char* kind;const char* attributes;const char* normalized;};
  for(const auto& row:std::array<Row,4>{{
    {"00012.3400","decimal"," fracDigits=\"2\"","12.34"},{"000.000","decimal"," fracDigits=\"2\"","0"},
    {".1200","decimal"," fracDigits=\"2\"","0.12"},{"00012.3400","float","","12.3400"}}}) {
    SCOPED_TRACE(row.input);
    MergeScenario(MergeField("amount",row.input,"<bind match=\"once\"/>",row.kind,"<numericEdit/>",row.attributes),
      "<root><anchor/></root>","if(firstLoad){yes(b.amount.rawValue!==null);near(Number(b.amount.rawValue),Number('"+std::string(row.normalized)+
      "'));eq(clean(b.amount.dataNode.value),'"+row.normalized+"');}b.amount.rawValue=123.4;near(Number(b.amount.rawValue),123.4);"
      "near(Number(b.amount.dataNode.value),123.4);",u"123.4");
  }
}

TEST(CoverageXfaDataMerge, ExclusionGroupDefaultExportsSelectOneMemberAndClearItsSiblings) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const auto* selected:{"A","B"}) {
    SCOPED_TRACE(selected);
    const auto member=[&](const char* name,const char* on,int x){return "<field name=\""+std::string(name)+"\" x=\""+std::to_string(x)+
      "pt\" y=\"0pt\" w=\"80pt\" h=\"20pt\"><ui><checkButton shape=\"round\"/></ui>"
      "<items><text>"+on+"</text><text>Off</text></items><value><text>"+(std::string(on)==selected?on:"Off")+"</text></value>"+
      forms::Event("for(var g=0;g<this.nodes.length;++g){var group=this.nodes.item(g);if(group.className!=='items')continue;"
        "for(var i=0;i<group.nodes.length;++i){var item=group.nodes.item(i),value=String(item.value);if(value.charCodeAt(0)===10)item.value=value.slice(1);}}"
        "if(typeof this.rawValue==='string'&&this.rawValue.charAt(0)==='\\n')this.rawValue=this.rawValue.substring(1);","initialize")+"</field>";};
    const auto fields="<exclGroup name=\"choice\" w=\"180pt\" h=\"20pt\"><bind match=\"once\"/>"+
        member("a","A",0)+member("b","B",90)+"</exclGroup>";
    MergeScenario(fields,"<root><anchor/></root>","var g=b.choice;if(firstLoad){eq(String(g.rawValue),'"+std::string(selected)+
      "');eq(clean(g.dataNode.value),'"+selected+"');}g.rawValue='B';eq(g.selectedMember().name,'b');"
      "eq(String(g.a.rawValue),'Off');eq(String(g.b.rawValue),'B');eq(clean(g.dataNode.value),'B');",u"",false);
  }
}

TEST(CoverageXfaDataMerge, ExplicitDataKindsNilAndCompoundTextAreRecognizedDuringInitialPacketLoading) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto fields=MergeField("nil","Fallback")+MergeField("mixed","Fallback")+MergeField("rich","Fallback");
  MergeScenario(fields,"<root><nil xsi:nil=\"true\">Ignored</nil><group xfa:dataNode=\"dataGroup\"/>"
      "<mixed xfa:dataNode=\"dataValue\">A<child>B</child>C</mixed>"
      "<rich xfa:dataNode=\"dataValue\"><html:body xmlns:html=\"http://www.w3.org/1999/xhtml\"><html:span>Rich</html:span></html:body></rich></root>",R"JS(
    eq(d.root.group.className,'dataGroup');eq(b.nil.dataNode.className,'dataValue');
    if(firstLoad){eq(text(b.nil),'');eq(text(b.mixed),'ABC');eq(clean(b.rich.rawValue).replace(/\n/g,''),'Rich');
      eq(d.root.mixed.nodes.length,3);eq(d.root.mixed.nodes.item(1).name,'child');eq(d.root.saveXML().indexOf('xsi:nil='),-1);}
    b.nil.rawValue='Changed';b.mixed.rawValue='ABC';b.rich.rawValue='Rich';
    eq(clean(b.nil.dataNode.value),'Changed');eq(text(b.mixed),'ABC');eq(text(b.rich),'Rich');
  )JS");
}

TEST(CoverageXfaDataMerge, NamespacedMetadataBindingsKeepQualifiedNamesAndNamespaceIdentityDistinct) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  MergeScenario(MergeField("code")+MergeField("code"),
      "<root xmlns:a=\"urn:merge:a\" xmlns:b=\"urn:merge:b\" a:code=\"A\" b:code=\"B\"><anchor/></root>",R"JS(
    var fields=b.resolveNodes('code[*]');eq(fields.length,2);var a=fields.item(0),other=fields.item(1);
    if(firstLoad){eq(text(a),'A');eq(text(other),'B');}
    eq(a.dataNode.getAttribute('qualifiedName'),'a:code');eq(other.dataNode.getAttribute('qualifiedName'),'b:code');
    // Native passes its entire qualified attribute name to the namespace
    // qualifier resolver. Rust resolves the actual prefix; both retain the
    // same qualified names and must update the correct XML attributes below.
    eq(a.dataNode.ns,rustBackend?'urn:merge:a':'');eq(other.dataNode.ns,rustBackend?'urn:merge:b':'');
    a.rawValue='Changed';other.rawValue='Other';
    yes(d.root.saveXML().indexOf('a:code="Changed"')>=0);yes(d.root.saveXML().indexOf('b:code="Other"')>=0);
    eq(text(a),'Changed');eq(text(other),'Other');
  )JS");
}

TEST(CoverageXfaDataMerge, NestedDataDescriptionsPermitDeclaredLeavesAndRejectUnrelatedNamespaceGroups) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(bool unrelated:{false,true}) {
    SCOPED_TRACE(unrelated);
    const std::string description="<dd:dataDescription xmlns:dd=\"http://ns.adobe.com/data-description/\" xmlns:other=\"urn:merge:unrelated\">"
        "<root><dd:group><dd:group><allowed/></dd:group><nested><leaf/></nested></dd:group>"+
        std::string(unrelated?"<other:group><blocked/></other:group>":"")+"</root></dd:dataDescription>";
    const auto fields=MergeField("allowed","Seed")+MergeField("blocked","Local")+
        MergeContainer("nested",MergeField("leaf","Leaf"),"<bind match=\"once\"/>");
    MergeScenario(fields,"<root><nested xfa:dataNode=\"dataGroup\"/></root>",R"JS(
      yes(b.allowed.dataNode);eq(b.blocked.dataNode,null);yes(b.nested.dataNode);yes(b.nested.leaf.dataNode);
      if(firstLoad){eq(text(b.allowed),'Seed');eq(text(b.blocked),'Local');eq(text(b.nested.leaf),'Leaf');}
      eq(d.root.resolveNode('blocked'),null);same(b.nested.dataNode,d.root.nested);
      b.allowed.rawValue='Changed';eq(clean(d.root.allowed.value),'Changed');
      eq(text(b.blocked),'Local');eq(clean(b.nested.leaf.dataNode.value),'Leaf');
    )JS",u"Changed",true,description);
  }
}

}  // namespace
}  // namespace pdfoxium::integration
