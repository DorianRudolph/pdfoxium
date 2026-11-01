#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <functional>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using RGB=std::array<int,3>;
using Frames=std::vector<Raster>;
const RGB green{0,128,0},red{255,0,0},blue{0,0,255},magenta{255,0,255};
std::string Escape(const std::string& value) {
  std::string result;for(char c:value) {
    if(c=='&')result+="&amp;";else if(c=='<')result+="&lt;";else if(c=='>')result+="&gt;";else result+=c;
  }return result;
}
std::string Event(const std::string& program,const std::string& activity="click") {
  return "<event activity=\""+activity+"\"><script contentType=\"application/x-javascript\">"+Escape(program)+"</script></event>";
}
std::string Block(const std::string& name,RGB color,int height=20) {
  return "<draw name=\""+name+"\" w=\"60pt\" h=\""+std::to_string(height)+"pt\"><value><rectangle><fill><color value=\""+
    std::to_string(color[0])+","+std::to_string(color[1])+","+std::to_string(color[2])+"\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
}
std::string Field(const std::string& name,const std::string& value="Seed",const std::string& bind="<bind match=\"once\"/>",
                  const std::string& kind="text",const std::string& ui="textEdit",const std::string& value_attributes="") {
  // These fields exercise model binding, not text painting. Colored companion
  // draws independently expose the count/order/geometry of generated instances.
  return "<field name=\""+name+"\" presence=\"invisible\" w=\"60pt\" h=\"20pt\">"+bind+"<ui><"+ui+
      "/></ui><value><"+kind+value_attributes+">"+Escape(value)+"</"+kind+"></value></field>";
}
std::string Row(const std::string& name,const std::string& properties="",const std::string& bind="<bind match=\"once\"/>",
                RGB color=green,const std::string& field_bind="<bind match=\"once\"/>") {
  return "<subform name=\""+name+"\" layout=\"position\" w=\"60pt\" h=\"20pt\">"+bind+properties+
      Field("value","Seed",field_bind)+Block("paint",color)+"</subform>";
}
std::string Occur(int minimum,int initial,int maximum) {
  return "<occur min=\""+std::to_string(minimum)+"\" initial=\""+std::to_string(initial)+"\" max=\""+std::to_string(maximum)+"\"/>";
}
std::string Checked(const std::string& program) {
  return "function eq(a,b){if(a!==b)throw 'expected '+String(b)+' got '+String(a);}"
      "function yes(a){if(!a)throw 'assertion';}"
      "function near(a,b){if(!isFinite(a)||Math.abs(a-b)>.00001)throw 'expected '+b+' got '+a;}"
      "function clean(a){return a===null?'':String(a).replace(/^\\n/,'');}"
      "function names(n,kind){var a=[];for(var i=0;i<n.nodes.length;i++){var c=n.nodes.item(i);if(c.className===kind)a.push(c.name);}return a.join(',');}"
      "function root(){return xfa.form.resolveNode('#subform');}"
      "try{var r=root(),b=r.body,d=xfa.datasets.data;"+program+";xfa.host.messageBox('PASS');}"
      "catch(e){xfa.host.messageBox('FAIL:'+String(e));}";
}
struct Spec {
  std::string body,data="<root><anchor/></root>",program,description;
  std::string root_name="root",body_bind="<bind match=\"none\"/>",prefix_template;
  int content_height=200;
  bool default_namespace=false;
  bool inspect_unbound_choice_reload=false;
  std::string picture_raw,picture_xml;
  std::function<void(const Frames&)> inspect;
  std::function<void(const Frames&)> inspect_initial;
};
std::string RootTemplate(const std::string& name,const std::string& body,const std::string& program,int height,
                         const std::string& binding="<bind match=\"none\"/>") {
  return "<subform "+(name.empty()?std::string():"name=\""+name+"\" ")+"layout=\"tb\" locale=\"en_US\" restoreState=\"auto\">"
      "<pageSet name=\"pages\"><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"240pt\" long=\"240pt\"/>"
      "<contentArea w=\"240pt\" h=\""+std::to_string(height)+"pt\"/></pageArea></pageSet>"
      "<subform name=\"controls\" layout=\"position\" w=\"240pt\" h=\"20pt\"><field name=\"run\" x=\"190pt\" w=\"20pt\" h=\"12pt\">"
      "<bind match=\"none\"/><ui><button/></ui><border><edge presence=\"hidden\"/></border>"+Event(Checked(program))+"</field></subform>"
      "<subform name=\"body\" layout=\"tb\" w=\"180pt\">"+binding+body+"</subform>"+
      Block("footer",magenta,10)+"</subform>";
}
std::vector<uint8_t> Fixture(const Spec& spec) {
  auto program=spec.program;
  if(!spec.picture_raw.empty()) {
    // The pinned native writer inserts a leading LF into the FORM packet's
    // picture content (its original TEMPLATE and metadata input are intact).
    // Observe that malformed picture and unnormalized imported dataset value,
    // then establish a healthy public-input control. Rust-written saves are
    // required to avoid this path by the host notice assertions below.
    program="var f=b.value,p=String(f.bind.picture.value);if(p.charCodeAt(0)===10){"
      "eq(clean(f.dataNode.value),'"+spec.picture_xml+"');f.bind.picture.value=p.slice(1);"
      "xfa.host.messageBox('PICTURE_LF');f.rawValue='"+spec.picture_raw+"';}"+program;
  }
  const auto xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"+spec.prefix_template+
      RootTemplate(spec.root_name,spec.body,program,spec.content_height,spec.body_bind)+"</template>";
  const auto datasets=spec.default_namespace
      ?"<datasets xmlns=\"http://www.xfa.org/schema/xfa-data/1.0/\"><data xfa:dataNode=\"dataGroup\" xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\">"+spec.data+"</data>"+spec.description+"</datasets>"
      :"<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data xfa:dataNode=\"dataGroup\">"+spec.data+"</xfa:data>"+spec.description+"</xfa:datasets>";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 240] >>",
      "<< /Fields [] /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("",xml),PdfStream("",datasets),PdfStream("","</xdp:xdp>")});
}
struct Loaded {
  forms::XfaCallbacks callbacks;Session s;
  Loaded(const Library& library,const std::vector<uint8_t>& bytes)
      :s(library.api(),bytes,true,true,2,[this](forms::FormHost& h){callbacks.Configure(h);}) {}
};
Frames Render(Session& s) {
  Frames images;const int count=s.api.FPDF_GetPageCount(s.document);EXPECT_GE(count,1);EXPECT_LE(count,16);
  if(count<1||count>16)return images;
  for(int i=0;i<count;++i) { images.push_back(s.Draw(240,240,0,0,0,0,0,0,i)); }
  return images;
}
size_t Count(const Frames& images,RGB color) {
  size_t n=0;for(const auto& image:images)for(size_t at=0;at<image.pixels.size();at+=4)
    n+=image.pixels[at]==color[2]&&image.pixels[at+1]==color[1]&&image.pixels[at+2]==color[0];
  return n;
}
void Compare(const Frames& a,const Frames& b,Artifacts& artifacts,const std::string& label) {
  ASSERT_EQ(a.size(),b.size());for(size_t p=0;p<a.size();++p)CompareRaster(a[p],b[p],artifacts,label+std::to_string(p));
}
void Execute(Session& s,bool native_picture_lf=false) {
  s.host->alerts.clear();s.api.FORM_ForceToKillFocus(s.form);
  ASSERT_EQ(s.api.FPDFPage_HasFormFieldAtPoint(s.form,s.page,195,5),FPDF_FORMFIELD_XFA_PUSHBUTTON);
  // A remerge may retire this button while handling its click. Its host result
  // and new page handles, rather than the old button's return value, establish
  // that the script completed and produced a valid current document.
  s.Click(195,5);
  for(size_t i=0;i<s.host->pages.size();++i)s.ClosePage(static_cast<int>(i));
  s.page=s.host->Page(0);ASSERT_NE(s.page,nullptr);
  if(native_picture_lf) {
    ASSERT_EQ(s.host->alerts.size(),2u);EXPECT_EQ(std::get<0>(s.host->alerts[0]),u"PICTURE_LF");
  } else { ASSERT_EQ(s.host->alerts.size(),1u); }
  EXPECT_EQ(std::get<0>(s.host->alerts.back()),u"PASS");
  s.api.FORM_ForceToKillFocus(s.form);s.api.FORM_OnMouseMove(s.form,s.page,0,230,180);
  EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
}
void Inspect(const Spec& spec,const Frames& images) {
  EXPECT_EQ(Count(images,magenta),600u); // A real footer proves complete flow.
  if(spec.inspect)spec.inspect(images);
}
void Scenario(const std::string& label,const Spec& spec) {
  const auto bytes=Fixture(spec);Artifacts artifacts;artifacts.Pdf(label+"-input",bytes);
  std::array<Frames,2> scenes;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Loaded loaded(*writer,bytes);auto& s=loaded.s;
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    if(spec.inspect_initial){const auto before=Render(s);
    ASSERT_NO_FATAL_FAILURE(spec.inspect_initial(before));}
    ASSERT_NO_FATAL_FAILURE(Execute(s));scenes[backend]=Render(s);
    ASSERT_NO_FATAL_FAILURE(Inspect(spec,scenes[backend]));
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+label);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Loaded reopened(*reader,saved);ASSERT_TRUE(reopened.s.valid());ASSERT_TRUE(reopened.s.xfa_loaded);
      if(spec.inspect_unbound_choice_reload) {
        // Check the direct reload before the healthy-control remerge. Native
        // revives the inactive unbound choice branch; Rust must preserve the
        // first-only choice selected by the original template/form state.
        const auto direct=Render(reopened.s);
        EXPECT_EQ(Count(direct,red),1200u);
        EXPECT_EQ(Count(direct,blue),reader->name()=="native"?1200u:0u);
        for(size_t p=0;p<direct.size();++p)artifacts.Image(
            writer->name()+reader->name()+label+"-direct-choice-reload-"+std::to_string(p),direct[p]);
      }
      ASSERT_NO_FATAL_FAILURE(Execute(reopened.s,writer->name()=="native"&&!spec.picture_raw.empty()));const auto after=Render(reopened.s);
    ASSERT_NO_FATAL_FAILURE(Inspect(spec,after));
      Compare(scenes[backend],after,artifacts,writer->name()+reader->name()+label);
    }++backend;
  }Compare(scenes[0],scenes[1],artifacts,label+"-engines");
}
std::string RowData(const std::vector<std::string>& values,const std::string& name="row") {
  std::string data="<root>";for(const auto& value:values)data+="<"+name+"><value>"+value+"</value></"+name+">";return data+"</root>";
}
std::string Group(const std::string& name,const std::string& children,const std::string& attributes="",
                   const std::string& binding="<bind match=\"none\"/>") {
  return "<subform name=\""+name+"\" layout=\"tb\" w=\"180pt\" "+attributes+">"+binding+children+"</subform>";
}
void ExpectGreen(Spec& spec,size_t rows) {
  spec.inspect=[=](const Frames& images){EXPECT_EQ(Count(images,green),rows*1200);};
}

TEST(CoverageXfaMergePageRecovery, MissingDataReferencePathsCreateNestedGroupsAndLeafBindings) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const std::string ref:{"nested.value","$record.nested.value","$data.root.nested.value"}) {
    Spec s;s.body=Field("display","Seed","<bind match=\"dataRef\" ref=\""+ref+"\"/>");
    s.program="eq(clean(b.display.rawValue),'Seed');eq(b.display.dataNode.name,'value');eq(b.display.dataNode.parent.name,'nested');"
      "eq(clean(d.root.nested.resolveNode('value').value),'Seed');b.display.rawValue='Changed';eq(clean(d.root.nested.resolveNode('value').value),'Changed');b.display.rawValue='Seed';";
    Scenario("missing-ref-"+ref,s);
  }
}
TEST(CoverageXfaMergePageRecovery, IndexedDataReferencesExpandExistingScalarSiblingsAndBindTheRequestedIndex) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Field("display","Seed","<bind match=\"dataRef\" ref=\"$record.rows.item[2]\"/>");
  s.data="<root xfa:dataNode=\"dataGroup\"><rows><item>A</item></rows></root>";
  s.program="eq(d.root.rows.resolveNodes('item[*]').length,3);eq(clean(b.display.rawValue),'Seed');"
    "eq(b.display.dataNode.somExpression,d.root.rows.resolveNode('item[2]').somExpression);eq(b.display.dataNode.name,'item');"
    "eq(clean(d.root.rows.resolveNode('item[0]').value),'A');"
    "b.display.rawValue='Changed';eq(clean(b.display.dataNode.value),'Changed');b.display.rawValue='Seed';";
  Scenario("indexed-missing-ref",s);
}
TEST(CoverageXfaMergePageRecovery, WildcardDataReferencesConsumeDistinctUnboundScalarNodes) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.data="<root><items><item>A</item><item>B</item><item>C</item></items></root>";
  for(int i=0;i<3;++i)s.body+=Field("v"+std::to_string(i),"Seed","<bind match=\"dataRef\" ref=\"$record.items.item[*]\"/>");
  s.program="for(var i=0;i<3;i++){var f=b.resolveNode('v'+i);eq(clean(f.rawValue),String.fromCharCode(65+i));"
    "eq(f.dataNode.somExpression,d.root.items.resolveNode('item['+i+']').somExpression);"
    "f.rawValue='Changed'+i;eq(clean(f.dataNode.value),'Changed'+i);f.rawValue=String.fromCharCode(65+i);}";
  Scenario("wildcard-scalar-binding",s);
}
TEST(CoverageXfaMergePageRecovery, OnceBindingsMoveTheirAncestorMatchIntoTheBoundContainersLocalScope) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Group("box",Field("value"),"","<bind match=\"once\"/>");
  s.data="<root><value>Parent</value><box><anchor/></box></root>";
  s.program="eq(b.box.dataNode.name,'box');eq(clean(b.box.value.rawValue),'Parent');eq(b.box.value.dataNode.parent.name,'box');"
    "eq(d.root.resolveNode('value'),null);b.box.value.rawValue='Changed';eq(clean(d.root.box.resolveNode('value').value),'Changed');b.box.value.rawValue='Parent';";
  Scenario("once-upward-scope",s);
}
TEST(CoverageXfaMergePageRecovery, TransparentScopeContainersCreateTheirLeavesInTheNearestBoundRecord) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Group("transparent",Field("value"),"scope=\"none\"","<bind match=\"none\"/>");
  s.program="eq(b.transparent.dataNode,null);eq(b.transparent.value.dataNode.parent.name,'root');"
    "eq(d.root.resolveNode('transparent'),null);eq(clean(b.transparent.value.rawValue),'Seed');"
    "b.transparent.value.rawValue='Changed';eq(clean(d.root.value.value),'Changed');b.transparent.value.rawValue='Seed';";
  Scenario("transparent-scope-leaf",s);
}
TEST(CoverageXfaMergePageRecovery, DataDescriptionsRejectNewScalarLeavesWhoseDeclarationIsAGroup) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Field("branch","Local");
  s.description="<dd:dataDescription xmlns:dd=\"http://ns.adobe.com/data-description/\"><root><branch><leaf/></branch></root></dd:dataDescription>";
  s.program="eq(b.branch.dataNode,null);eq(clean(b.branch.rawValue),'Local');eq(d.root.resolveNode('branch'),null);"
    "b.branch.rawValue='Changed';eq(d.root.resolveNode('branch'),null);b.branch.rawValue='Local';";
  Scenario("description-wrong-node-kind",s);
}
TEST(CoverageXfaMergePageRecovery, GlobalScalarBindingsSkipSameNamedGroupsWhileFindingNestedValues) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Field("shared","Seed","<bind match=\"global\"/>")+Field("shared","Seed","<bind match=\"global\"/>");
  s.data="<root><shared><anchor/></shared><branch><shared>Nested</shared></branch></root>";
  s.program="var a=b.resolveNode('shared[0]'),c=b.resolveNode('shared[1]');eq(clean(a.rawValue),'Nested');eq(clean(c.rawValue),'Nested');"
    "eq(a.dataNode.somExpression,c.dataNode.somExpression);eq(a.dataNode.parent.name,'branch');"
    "a.rawValue='Changed';eq(clean(c.rawValue),'Changed');a.rawValue='Nested';";
  Scenario("global-skip-group",s);
}
TEST(CoverageXfaMergePageRecovery, GlobalBindingCachesRetainTheFirstNestedScalarAcrossDifferentFormScopes) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Group("inside",Field("shared","Seed","<bind match=\"global\"/>"),"","<bind match=\"dataRef\" ref=\"$record.branch\"/>")+
    Group("outside",Field("shared","Seed","<bind match=\"global\"/>"));
  s.data="<root><branch><shared>Inside</shared></branch><other><shared>Later</shared></other></root>";
  s.program="eq(clean(b.inside.shared.rawValue),'Inside');eq(clean(b.outside.shared.rawValue),'Inside');"
    "eq(b.inside.shared.dataNode.somExpression,b.outside.shared.dataNode.somExpression);"
    "b.outside.shared.rawValue='Changed';eq(clean(b.inside.shared.rawValue),'Changed');b.inside.shared.rawValue='Inside';";
  Scenario("global-cached-scopes",s);
}
TEST(CoverageXfaMergePageRecovery, ChoiceSetsSelectTheFirstMatchingNestedGroupInDatasetOrder) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(bool reverse:{false,true}) {
    Spec s;s.body="<subformSet name=\"set\" relation=\"choice\">"+Row("a","","<bind match=\"once\"/>",red)+Row("b","","<bind match=\"once\"/>",blue)+"</subformSet>";
    const std::string first=reverse?"a":"b";s.data=reverse?RowData({"A"},"a"):RowData({"B"},"b");
    s.program="eq(names(b.set,'subform'),'"+first+"');var n=b.set.resolveNode('"+first+"');eq(n.dataNode.name,'"+first+
      "');eq(clean(n.value.rawValue),'"+(reverse?"A":"B")+"');";
    s.inspect=[=](const Frames& images){EXPECT_EQ(Count(images,reverse?red:blue),1200u);EXPECT_EQ(Count(images,reverse?blue:red),0u);};
    Scenario("choice-nested-group-"+first,s);
  }
}
TEST(CoverageXfaMergePageRecovery, UnorderedNestedGroupsFollowDataOrderBeforeUnmatchedTemplateDefaults) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body="<subformSet name=\"set\" relation=\"unordered\">"+Row("a","","<bind match=\"once\"/>",red)+
    Row("b","","<bind match=\"once\"/>",blue)+Row("c","","<bind match=\"none\"/>",green,"<bind match=\"none\"/>")+"</subformSet>";
  s.data="<root><b><value>B</value></b><a><value>A</value></a></root>";
  s.program="eq(names(b.set,'subform'),'b,a,c');eq(clean(b.set.b.value.rawValue),'B');eq(clean(b.set.a.value.rawValue),'A');"
    "eq(b.set.c.dataNode,null);eq(clean(b.set.c.value.rawValue),'Seed');";
  s.inspect=[](const Frames& images){for(RGB c:{red,blue,green})EXPECT_EQ(Count(images,c),1200u);};Scenario("unordered-nested-groups",s);
}
TEST(CoverageXfaMergePageRecovery, RemergingChoiceSetsWithoutMatchingDataCopiesOnlyTheirFirstLocalDefaultContainer) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body="<subformSet name=\"set\" relation=\"choice\">"+Row("a","","<bind match=\"none\"/>",red,"<bind match=\"none\"/>")+
    Row("b","","<bind match=\"none\"/>",blue,"<bind match=\"none\"/>")+"</subformSet>";
  s.inspect_unbound_choice_reload=true;
  // Recompute this unbound choice from the dataset on every reader. Saved
  // transient form children are not the same operation as a fresh data merge.
  s.program="xfa.form.remerge();r=root();b=r.body;eq(names(b.set,'subform'),'a');eq(clean(b.set.a.value.rawValue),'Seed');eq(b.set.a.dataNode,null);eq(b.set.resolveNode('b'),null);";
  s.inspect=[](const Frames& images){EXPECT_EQ(Count(images,red),1200u);EXPECT_EQ(Count(images,blue),0u);};Scenario("choice-local-default",s);
}
TEST(CoverageXfaMergePageRecovery, UnorderedSetsWithoutMatchingDataRetainEveryLocalDefaultContainer) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body="<subformSet name=\"set\" relation=\"unordered\">"+Row("a","","<bind match=\"none\"/>",red,"<bind match=\"none\"/>")+
    Row("b","","<bind match=\"none\"/>",blue,"<bind match=\"none\"/>")+"</subformSet>";
  s.program="eq(names(b.set,'subform'),'a,b');eq(clean(b.set.a.value.rawValue),'Seed');eq(clean(b.set.b.value.rawValue),'Seed');";
  s.inspect=[](const Frames& images){EXPECT_EQ(Count(images,red),1200u);EXPECT_EQ(Count(images,blue),1200u);};Scenario("unordered-local-defaults",s);
}
TEST(CoverageXfaMergePageRecovery, AnonymousUnboundContainersHonorInitialPaddingAndOwnIndependentChildren) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int count:{0,2,3}) {
    Spec s;s.body="<subform layout=\"position\" w=\"60pt\" h=\"20pt\"><bind match=\"none\"/>"+Occur(0,count,3)+
      Field("value","Seed","<bind match=\"none\"/>")+Block("paint",green)+"</subform>";
    s.program="var nodes=[];for(var i=0;i<b.nodes.length;i++){var n=b.nodes.item(i);if(n.className==='subform')nodes.push(n);}"
      "eq(nodes.length,"+std::to_string(count)+");for(var i=0;i<nodes.length;i++){eq(nodes[i].dataNode,null);eq(clean(nodes[i].value.rawValue),'Seed');}"
      "if(nodes.length>1){nodes[0].value.rawValue='Changed';eq(clean(nodes[1].value.rawValue),'Seed');nodes[0].value.rawValue='Seed';}";
    ExpectGreen(s,count);Scenario("anonymous-initial-"+std::to_string(count),s);
  }
}
TEST(CoverageXfaMergePageRecovery, AreaChildrenRepeatAgainstRecordsWithoutCreatingAnInstanceManager) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body="<area name=\"region\">"+Row("row",Occur(1,1,3))+"</area>";s.data=RowData({"A","B"});
  s.program="eq(b.region.resolveNode('_row'),null);var rows=b.region.resolveNodes('row[*]');eq(rows.length,2);"
    "eq(clean(rows.item(0).value.rawValue),'A');eq(clean(rows.item(1).value.rawValue),'B');";
  ExpectGreen(s,1);Scenario("area-no-instance-manager",s);
}
TEST(CoverageXfaMergePageRecovery, ConsecutiveSameNamedInstanceManagersConsumeOnlyTheirOwnRecordLimits) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Row("row",Occur(1,1,2))+Row("row",Occur(1,1,1));s.data=RowData({"A","B","C","D"});
  s.program="var rows=b.resolveNodes('row[*]');eq(rows.length,3);eq(b.resolveNode('_row[0]').count,2);eq(b.resolveNode('_row[1]').count,1);"
    "for(var i=0;i<3;i++)eq(clean(rows.item(i).value.rawValue),String.fromCharCode(65+i));eq(d.root.resolveNodes('row[*]').length,4);";
  ExpectGreen(s,3);Scenario("adjacent-instance-manager-limits",s);
}
TEST(CoverageXfaMergePageRecovery, ExplicitNameReferencesResolveWithinTheirBoundRecordsWithoutParentLookup) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Row("row",Occur(1,1,3),"<bind match=\"dataRef\" ref=\"$record.items.item[*]\"/>",green,"<bind match=\"dataRef\" ref=\"name\"/>");
  s.data="<root><name>Wrong</name><items><item><name>A</name></item><item><name>B</name></item></items></root>";
  s.program="var rows=b.resolveNodes('row[*]');eq(rows.length,2);for(var i=0;i<2;i++){var f=rows.item(i).value;"
    "eq(clean(f.rawValue),String.fromCharCode(65+i));eq(f.dataNode.parent.getAttribute('name'),'item');eq(f.dataNode.name,'name');}";
  ExpectGreen(s,2);Scenario("local-name-reference",s);
}
TEST(CoverageXfaMergePageRecovery, ChildDataReferencesUnderRepeatedParentReferencesKeepTheirIndependentScopes) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Row("row",Occur(1,1,3),"<bind match=\"dataRef\" ref=\"$record.items.item[*]\"/>",green,"<bind match=\"dataRef\" ref=\"local.value\"/>");
  s.data="<root><items><item><local><value>A</value></local></item><item><local><value>B</value></local></item></items></root>";
  s.program="var rows=b.resolveNodes('row[*]');eq(rows.length,2);for(var i=0;i<2;i++){var f=rows.item(i).value;"
    "eq(clean(f.rawValue),String.fromCharCode(65+i));eq(f.dataNode.parent.name,'local');eq(f.dataNode.parent.parent.name,'item');"
    "f.rawValue='Changed'+i;eq(clean(f.dataNode.value),'Changed'+i);f.rawValue=String.fromCharCode(65+i);}";
  ExpectGreen(s,2);Scenario("nested-parent-child-reference",s);
}
TEST(CoverageXfaMergePageRecovery, SomUseHrefReferencesInheritPrototypeChildrenWithoutSharingTheirValues) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const std::string href:{"#som(base)",".#som(base)"}) {
    Spec s;s.body="<subform name=\"base\" id=\"prototype\" presence=\"hidden\" layout=\"position\" w=\"60pt\" h=\"20pt\"><bind match=\"none\"/>"+
      Field("value","Seed","<bind match=\"none\"/>")+Block("paint",green)+"</subform>"
      "<subform name=\"inherited\" usehref=\""+href+"\" presence=\"visible\"/>";
    s.program="eq(b.inherited.w,'60pt');eq(b.inherited.h,'20pt');eq(clean(b.inherited.value.rawValue),'Seed');"
      "b.inherited.value.rawValue='Changed';eq(clean(b.base.value.rawValue),'Seed');b.inherited.value.rawValue='Seed';";
    ExpectGreen(s,1);Scenario("som-prototype-"+href,s);
  }
}
TEST(CoverageXfaMergePageRecovery, PrototypeMergingPreservesLocalNestedValuesWhileAddingMissingSiblings) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body="<subform name=\"base\" id=\"prototype\" presence=\"hidden\" layout=\"tb\" w=\"60pt\"><bind match=\"none\"/>"+
    Group("inner",Field("value","Prototype","<bind match=\"none\"/>")+Field("extra","Inherited","<bind match=\"none\"/>"))+"</subform>"
    "<subform name=\"inherited\" use=\"#prototype\" presence=\"visible\">"+
    Group("inner",Field("value","Local","<bind match=\"none\"/>"))+"</subform>";
  s.program="eq(clean(b.inherited.inner.value.rawValue),'Local');eq(clean(b.inherited.inner.extra.rawValue),'Inherited');"
    "b.inherited.inner.extra.rawValue='Changed';eq(clean(b.base.inner.extra.rawValue),'Inherited');b.inherited.inner.extra.rawValue='Inherited';";
  Scenario("nested-prototype-local-values",s);
}
TEST(CoverageXfaMergePageRecovery, ExternalPrototypeUrisAndUnknownFragmentsPreserveLocalTemplateDefaults) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const std::string href:{"external.xdp#prototype","external.xdp",".#absent","#absent"}) {
    Spec s;s.body=Field("local","Local","<bind match=\"none\"/>");
    const auto at=s.body.find(" presence=");s.body.insert(at," usehref=\""+href+"\"");
    s.program="eq(clean(b.local.rawValue),'Local');eq(b.local.dataNode,null);";Scenario("unavailable-prototype-"+href,s);
  }
}
TEST(CoverageXfaMergePageRecovery, DatasetMetadataClonesBecomeIndependentElementsWhenAttached) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.data="<root code=\"A\"><anchor/></root>";
  s.program="var n=d.root.code.clone(false);eq(n.parent,null);eq(clean(n.value),'A');n.name='copy';d.root.nodes.insert(n,null);"
    "n.value='B';eq(clean(d.root.code.value),'A');eq(clean(d.root.copy.value),'B');"
    // Dataset name changes update SOM identity without renaming its existing XML tag.
    "yes(d.root.saveXML().indexOf('<code')>=0);d.root.nodes.remove(n);eq(d.root.resolveNode('copy'),null);";
  Scenario("metadata-clone-element",s);
}
TEST(CoverageXfaMergePageRecovery, DatasetDeepClonesPreserveNestedXmlWhileValueMutationsRemainIndependent) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.data="<root><branch code=\"A\"><leaf>B</leaf></branch></root>";
  s.program="var n=d.root.branch.clone(true);eq(n.parent,null);n.name='copy';d.root.nodes.insert(n,null);"
    "eq(clean(n.code.value),'A');eq(clean(n.leaf.value),'B');n.leaf.value='Changed';eq(clean(d.root.branch.leaf.value),'B');"
    "eq(d.root.saveXML().split('<branch').length,3);d.root.nodes.remove(n);eq(d.root.resolveNode('copy'),null);";
  Scenario("deep-dataset-clone",s);
}
TEST(CoverageXfaMergePageRecovery, DatasetShallowClonesHaveNoModelChildrenAndCanBeAttachedAndRemoved) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.data="<root><branch><leaf>B</leaf></branch></root>";
  s.program="var n=d.root.branch.clone(false);eq(n.parent,null);eq(n.className,'dataGroup');eq(n.nodes.length,0);"
    "n.name='copy';d.root.nodes.insert(n,null);eq(d.root.copy.nodes.length,0);d.root.nodes.remove(n);eq(d.root.resolveNode('copy'),null);";
  Scenario("shallow-dataset-clone",s);
}
TEST(CoverageXfaMergePageRecovery, DatasetLeafClonesRetainSelfContainedNamespacesAcrossReattachment) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.data="<root><p:leaf xmlns:p=\"urn:clone:leaf\" xfa:dataNode=\"dataValue\">Leaf</p:leaf></root>";
  s.program="var n=d.root.leaf.clone(true);eq(clean(n.value),'Leaf');eq(n.ns,'urn:clone:leaf');"
    "n.name='copy';d.root.nodes.insert(n,null);eq(clean(n.value),'Leaf');n.value='Changed';eq(clean(d.root.leaf.value),'Leaf');"
    "d.root.nodes.remove(n);eq(d.root.resolveNode('copy'),null);";
  Scenario("namespaced-dataset-clone",s);
}
TEST(CoverageXfaMergePageRecovery, SameDataRemergeRebindsExplicitReferencesAndRebuildsIndependentFormNodes) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Row("row","","<bind match=\"dataRef\" ref=\"$record.row\"/>",green,"<bind match=\"dataRef\" ref=\"$record.row.value\"/>");s.data=RowData({"A"});
  s.program="var old=b.row,oldData=old.dataNode;xfa.form.remerge();r=root();b=r.body;"
    "eq(b.row===old,false);eq(clean(b.row.value.rawValue),'A');eq(b.row.dataNode.somExpression,oldData.somExpression);"
    "b.row.value.rawValue='Changed';eq(clean(b.row.dataNode.resolveNode('value').value),'Changed');b.row.value.rawValue='A';";
  ExpectGreen(s,1);Scenario("same-data-remerge",s);
}
TEST(CoverageXfaMergePageRecovery, RemergeChoiceAfterDatasetReorderingReplacesItsVisibleSelectedBranch) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body="<subformSet name=\"set\" relation=\"choice\">"+Row("a","","<bind match=\"once\"/>",red)+
    Row("b","","<bind match=\"once\"/>",blue)+"</subformSet>";
  s.data="<root><a><value>A</value></a><b><value>B</value></b></root>";
  s.program="if(b.set.resolveNode('a')!==null){d.root.nodes.insert(d.root.b,d.root.a);xfa.form.remerge();r=root();b=r.body;}"
    "eq(names(b.set,'subform'),'b');eq(clean(b.set.b.value.rawValue),'B');";
  s.inspect_initial=[](const Frames& images){EXPECT_EQ(Count(images,red),1200u);EXPECT_EQ(Count(images,blue),0u);};
  s.inspect=[](const Frames& images){EXPECT_EQ(Count(images,blue),1200u);EXPECT_EQ(Count(images,red),0u);};Scenario("remerge-choice-order",s);
}
TEST(CoverageXfaMergePageRecovery, ReplacingDatasetRecordsAndRemergingUpdatesGeneratedInstancesAndTheirPaint) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Row("row",Occur(1,1,4));s.data=RowData({"A","B"});
  s.program="d.root.loadXML('<root><row><value>C</value></row><row><value>D</value></row><row><value>E</value></row></root>',true,true);"
    "xfa.form.remerge();r=root();b=r.body;var rows=b.resolveNodes('row[*]');eq(rows.length,3);"
    "for(var i=0;i<3;i++)eq(clean(rows.item(i).value.rawValue),String.fromCharCode(67+i));";
  s.inspect_initial=[](const Frames& images){EXPECT_EQ(Count(images,green),2400u);};ExpectGreen(s,3);Scenario("remerge-replaced-records",s);
}
TEST(CoverageXfaMergePageRecovery, GlobalBindingsAfterRemergeUseTheReplacementDatasetInsteadOfRetiredCachedNodes) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Field("shared","Seed","<bind match=\"global\"/>")+Field("shared","Seed","<bind match=\"global\"/>");
  s.data="<root><shared>A</shared></root>";
  s.program="d.root.loadXML('<root><shared>B</shared></root>',true,true);xfa.form.remerge();r=root();b=r.body;d=xfa.datasets.data;"
    "var a=b.resolveNode('shared[0]'),c=b.resolveNode('shared[1]');eq(clean(a.rawValue),'B');eq(clean(c.rawValue),'B');"
    "eq(a.dataNode.somExpression,d.root.shared.somExpression);eq(a.dataNode.somExpression,c.dataNode.somExpression);";
  Scenario("remerge-global-cache",s);
}
TEST(CoverageXfaMergePageRecovery, RootTemplateSelectionMatchesTheDatasetRecordNameInsteadOfTemplateDocumentOrder) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Field("value");s.data="<root><value>A</value></root>";
  s.prefix_template=RootTemplate("other",Block("wrong",blue),"eq(r.name,'other');",200);
  s.program="eq(r.name,'root');eq(r.dataNode.name,'root');eq(clean(b.value.rawValue),'A');";
  s.inspect=[](const Frames& images){EXPECT_EQ(Count(images,blue),0u);};Scenario("record-root-selection",s);
}
TEST(CoverageXfaMergePageRecovery, UnmatchedRecordNamesFallBackToTheFirstTemplateSubformAndRetainItsDataScope) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Field("value");s.data="<unmatched><value>A</value></unmatched>";
  s.program="eq(r.name,'root');eq(r.dataNode.name,'unmatched');eq(clean(b.value.rawValue),'A');"
    "eq(b.value.dataNode.parent.name,'unmatched');";Scenario("unmatched-record-root",s);
}
TEST(CoverageXfaMergePageRecovery, AnonymousTemplateRootsCreateANamedDatasetRecordForTheirDefaults) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.root_name="";s.data="";s.body=Field("value");
  s.program="eq(r.name,'');eq(r.dataNode.name,'form');eq(clean(b.value.rawValue),'Seed');eq(clean(d.form.resolveNode('value').value),'Seed');";
  Scenario("anonymous-record-root",s);
}
TEST(CoverageXfaMergePageRecovery, DefaultNamespaceDatasetsExposeTheirDataPacketAndUnqualifiedRecordBindings) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.default_namespace=true;s.body=Field("value");s.data="<root xmlns=\"\"><value>A</value></root>";
  s.program="eq(d.className,'dataGroup');eq(r.dataNode.name,'root');eq(clean(b.value.rawValue),'A');"
    "eq(b.value.dataNode.parent.name,'root');b.value.rawValue='Changed';eq(clean(d.root.value.value),'Changed');b.value.rawValue='A';";
  Scenario("default-namespace-datasets",s);
}
TEST(CoverageXfaMergePageRecovery, BoundRepeatedInstancesReportTheirRealPageIndicesAndPerPageContentNodes) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.content_height=60;s.body=Row("row",Occur(1,1,4));s.data=RowData({"A","B","C","D"});
  s.program="var rows=b.resolveNodes('row[*]'),l=xfa.layout;eq(rows.length,4);eq(l.pageCount(),2);"
    "for(var i=0;i<4;i++){var n=rows.item(i),p=i<2?0:1;eq(l.absPage(n),p);eq(l.page(n),p+1);eq(l.pageSpan(n),1);eq(l.absPageSpan(n),1);}"
    "for(var p=0;p<2;p++){eq(l.pageContent(p,'pageArea').length,1);eq(l.pageContent(p,'contentArea').length,1);"
    "eq(l.pageContent(p,'field',true).length,0);yes(l.pageContent(p,'draw').length>=2);}";
  ExpectGreen(s,4);Scenario("bound-page-content-indices",s);
}

// Metadata values exercise the same scalar binding normalization without the
// pinned native writer's separately tested leading-LF text-node defect.
std::string PictureBinding(const std::string& picture) {
  return "<bind match=\"once\"><picture>"+Escape(picture)+"</picture></bind>";
}
TEST(CoverageXfaMergePageRecovery, NumericBindingPicturesNormalizeImportedTextAndFormatRawValueExports) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Field("value","0",PictureBinding("num{z,zz9.99}"),"decimal","numericEdit"," fracDigits=\"2\"");
  s.data="<root value=\"1,234.50\"><anchor/></root>";
  s.program="near(Number(b.value.rawValue),1234.5);b.value.rawValue=123.4;near(Number(b.value.dataNode.value),123.4);yes(b.value.dataNode.saveXML().indexOf('123.40')>=0);"
    "b.value.rawValue=1234.5;near(Number(b.value.dataNode.value),1234.5);yes(b.value.dataNode.saveXML().indexOf('1,234.50')>=0);";
  Scenario("numeric-binding-picture",s);
}
TEST(CoverageXfaMergePageRecovery, TextBindingPicturesSeparateCanonicalValuesFromSerializedPunctuation) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Field("value","",PictureBinding("text{AAA-999}"));s.data="<root value=\"ABC-123\"><anchor/></root>";s.picture_raw="ABC123";s.picture_xml="ABC-123";
  s.program="eq(clean(b.value.rawValue),'ABC123');b.value.rawValue='XYZ987';eq(clean(b.value.dataNode.value),'XYZ987');yes(b.value.dataNode.saveXML().indexOf('XYZ-987')>=0);"
    "b.value.rawValue='ABC123';eq(clean(b.value.dataNode.value),'ABC123');yes(b.value.dataNode.saveXML().indexOf('ABC-123')>=0);";
  Scenario("text-binding-picture",s);
}
TEST(CoverageXfaMergePageRecovery, DateAndTimeBindingPicturesNormalizeDatasetValuesAndRetainCanonicalRawValues) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Case {const char* kind;const char* picture;const char* data;const char* raw;const char* next;const char* formatted;};
  for(const auto& c:std::array<Case,2>{{
    {"date","date{YYYY/MM/DD}","2004/02/29","2004-02-29","2008-12-31","2008/12/31"},
    {"time","time{hh:MM:SS A}","01:14:15 PM","13:14:15","02:03:04","02:03:04 AM"}}}) {
    Spec s;s.body=Field("value","",PictureBinding(c.picture),c.kind,"dateTimeEdit");s.data="<root value=\""+std::string(c.data)+"\"><anchor/></root>";s.picture_raw=c.raw;s.picture_xml=c.data;
    s.program="eq(clean(b.value.rawValue),'"+std::string(c.raw)+"');b.value.rawValue='"+c.next+
      "';eq(clean(b.value.dataNode.value),'"+c.next+"');yes(b.value.dataNode.saveXML().indexOf('"+c.formatted+"')>=0);b.value.rawValue='"+c.raw+"';";
    Scenario("date-time-bind-"+std::string(c.kind),s);
  }
}
TEST(CoverageXfaMergePageRecovery, DateAndTimeBindingExportsProjectTheirPartFromCombinedDateTimeRawValues) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Case {const char* kind;const char* picture;const char* initial;const char* canonical;const char* combined;};
  for(const auto& c:std::array<Case,2>{{
    {"date","date{MM/DD/YYYY}","02/29/2004","2004-02-29","2004-02-29T13:14:15"},
    {"time","time{HH:MM:SS}","13:14:15","13:14:15","2004-02-29 13:14:15"}}}) {
    Spec s;s.body=Field("value","",PictureBinding(c.picture),c.kind,"dateTimeEdit");s.data="<root value=\""+std::string(c.initial)+"\"><anchor/></root>";s.picture_raw=c.canonical;s.picture_xml=c.initial;
    s.program="eq(clean(b.value.rawValue),'"+std::string(c.canonical)+"');b.value.rawValue='"+c.combined+
      "';eq(clean(b.value.dataNode.value),'"+c.combined+"');yes(b.value.dataNode.saveXML().indexOf('"+c.initial+"')>=0);b.value.rawValue='"+c.canonical+"';";
    Scenario("combined-binding-projection-"+std::string(c.kind),s);
  }
}
TEST(CoverageXfaMergePageRecovery, InvalidBindingPicturesRetainUnparseableValuesThroughBothDirections) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Field("value","",PictureBinding("date{YYYY/MM/DD}"),"date","dateTimeEdit");s.data="<root value=\"not-a-date\"><anchor/></root>";
  s.program="eq(clean(b.value.rawValue),'not-a-date');b.value.rawValue='still-invalid';eq(clean(b.value.dataNode.value),'still-invalid');b.value.rawValue='not-a-date';";
  Scenario("binding-picture-invalid-fallback",s);
}
TEST(CoverageXfaMergePageRecovery, AlternativeBindingPicturesParseTheMatchingInputAndExportThePreferredPattern) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.body=Field("value","",PictureBinding("date{YYYY/MM/DD}|date{MM-DD-YYYY}"),"date","dateTimeEdit");s.data="<root value=\"02-29-2004\"><anchor/></root>";s.picture_raw="2004-02-29";s.picture_xml="2004/02/29";
  s.program="eq(clean(b.value.rawValue),'2004-02-29');b.value.rawValue='2008-12-31';eq(clean(b.value.dataNode.value),'2008-12-31');yes(b.value.dataNode.saveXML().indexOf('2008/12/31')>=0);"
    "b.value.rawValue='2004-02-29';eq(clean(b.value.dataNode.value),'2004-02-29');yes(b.value.dataNode.saveXML().indexOf('2004/02/29')>=0);";
  Scenario("alternative-binding-picture",s);
}
TEST(CoverageXfaMergePageRecovery, TemplateImageDefaultsExportInlineBytesAndContentTypeToTheirCreatedBinding) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::ifstream file(TestRoot()/"upstream/pdfium/testing/resources/pixel/xfa_specific/xfa_png_image.in",std::ios::binary);
  ASSERT_TRUE(file.good());const std::string source((std::istreambuf_iterator<char>(file)),{});
  const auto start=source.find("<image "),content=source.find('>',start),end=source.find("</image>",content);
  ASSERT_NE(start,std::string::npos);ASSERT_NE(content,std::string::npos);ASSERT_NE(end,std::string::npos);
  const auto encoded=source.substr(content+1,end-content-1);
  Spec s;s.body=Field("picture",encoded,"<bind match=\"once\"/>","image","imageEdit"," contentType=\"image/png\" transferEncoding=\"base64\"");
  s.program="eq(b.picture.value.image.contentType,'image/png');eq(b.picture.value.image.transferEncoding,'base64');"
    "yes(clean(b.picture.rawValue).length>100);yes(clean(b.picture.dataNode.value).length>100);"
    "eq(b.picture.dataNode.getAttribute('contentType'),'image/png');yes(d.root.saveXML().indexOf('xfa:contentType=')>=0);";
  Scenario("template-image-binding-export",s);
}
}  // namespace
}  // namespace pdfoxium::integration
