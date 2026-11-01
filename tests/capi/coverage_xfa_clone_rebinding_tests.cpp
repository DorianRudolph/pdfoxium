#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using Frames=std::vector<Raster>;
using RGB=std::array<int,3>;
const RGB green{0,128,0},cyan{0,255,255},orange{255,128,0},magenta{255,0,255};
struct Spec {
  std::string name,program;
  std::vector<int> values;
  int shared=10;
  bool alternative_master=false;
};
std::string Paint(const std::string& name,RGB color,int width,int height,const std::string& attrs=""){
  return "<draw name=\""+name+"\" w=\""+std::to_string(width)+"pt\" h=\""+std::to_string(height)+"pt\" "+attrs+"><value><rectangle><fill><color value=\""+
    std::to_string(color[0])+","+std::to_string(color[1])+","+std::to_string(color[2])+"\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
}
std::string Field(const std::string& name,const std::string& value,const std::string& bind,const std::string& extra=""){
  return "<field name=\""+name+"\" presence=\"invisible\" w=\"60pt\" h=\"12pt\">"+bind+"<ui><textEdit/></ui><value><text>"+value+"</text></value>"+extra+"</field>";
}
std::string Prelude(bool native_control){
  // Native DoDataRemerge retains strong old binding edges; FindOnceDataNode
  // then skips those same records. Its healthy public control refreshes only
  // child identities, retaining the record root. Resolve that root through
  // the cached resolver: CJX_List::item intentionally creates a NEW wrapper
  // and replaces the binding-map entry, invalidating JS === identity probes
  // despite retaining the same underlying node. Rust keeps same-data remerge.
  const std::string fresh_children=native_control?
    "var record=d.resolveNode('#dataGroup'),old=[];for(var i=0;i<record.nodes.length;i++)old.push(record.nodes.item(i));"
    "for(var i=0;i<old.length;i++){var copy=old[i].clone(true);record.nodes.insert(copy,old[i]);record.nodes.remove(old[i]);}":"";
  return "function eq(a,b){if(a!==b)throw 'expected '+String(b)+' got '+String(a);}function yes(a,label){if(!a)throw 'assertion '+(label||'');}"
    "function clean(a){return a===null?'':String(a).replace(/^\\n/,'');}function root(){return xfa.form.resolveNode('#subform');}"
    "function currentRow(previous,index){return "+std::string(native_control?"d.root.resolveNode('row['+index+']')":"previous")+";}"
    "function refresh(){"+fresh_children+"xfa.form.remerge();r=root();b=r.body;d=xfa.datasets.data;xfa.form.recalculate(0);}"
    "function verify(values,shared){eq(b._row.count,values.length);for(var i=0;i<values.length;i++){var f=b.resolveNode('row['+i+']'),n=d.resolveNode(r.name+'.row['+i+']');"
    "yes(f.dataNode===n,'row data '+i);yes(f.amount.dataNode===n.amount,'amount data '+i);yes(f.shared.dataNode===d.resolveNode(r.name+'.shared'),'global data '+i);eq(Number(clean(f.amount.rawValue)),values[i]);eq(Number(clean(f.shared.rawValue)),shared);"
    "eq(Number(clean(f.total.rawValue)),values[i]*2+shared);eq(clean(f.local.rawValue),'Seed');}}";
}
std::string Checked(const std::string& program,bool native_control){
  return Prelude(native_control)+"try{var r=root(),b=r.body,d=xfa.datasets.data;"+program+";xfa.host.messageBox('PASS');}catch(e){xfa.host.messageBox('FAIL:'+String(e));}";
}
std::string List(const std::vector<int>& values){
  std::string out="[";for(size_t i=0;i<values.size();++i){if(i)out+=",";out+=std::to_string(values[i]);}return out+"]";
}
std::string RootTemplate(const std::string& name,const Spec& spec,RGB master,bool native_control){
  const auto check="eq(r.name,'"+std::string(spec.alternative_master?"other":"root")+"');xfa.form.recalculate(0);verify("+List(spec.values)+","+std::to_string(spec.shared)+");";
  std::string controls;
  for(int action=0;action<2;++action)controls+="<field name=\"action"+std::to_string(action)+"\" x=\""+std::to_string(190+20*action)+"pt\" w=\"16pt\" h=\"12pt\"><bind match=\"none\"/><ui><button/></ui><border><edge presence=\"hidden\"/></border>"+
    forms::Event(Checked(action==0?spec.program:check,native_control),"click")+"</field>";
  const auto row="<subform name=\"row\" layout=\"position\" w=\"60pt\" h=\"30pt\"><occur min=\"0\" initial=\"0\" max=\"8\"/><keep intact=\"contentArea\"/>"+
    Field("amount","0","<bind match=\"once\"/>")+Field("shared","10","<bind match=\"global\"/>")+
    Field("local","Seed","<bind match=\"none\"/>")+
    Field("total","0","<bind match=\"none\"/>","<calculate><script contentType=\"application/x-javascript\">Number(this.parent.amount.rawValue)*2+Number(this.parent.shared.rawValue);</script></calculate>")+
    Paint("payload",green,60,30)+"</subform>";
  return "<subform name=\""+name+"\" layout=\"tb\" locale=\"en_US\" restoreState=\"auto\"><pageSet name=\"pages\"><pageArea name=\"master\"><occur min=\"1\" max=\"-1\"/><medium short=\"240pt\" long=\"240pt\"/><contentArea w=\"180pt\" h=\"100pt\"/>"+
    Paint("stationery",master,8,8,"x=\"220pt\" y=\"220pt\"")+"</pageArea></pageSet><subform name=\"controls\" layout=\"position\" w=\"240pt\" h=\"20pt\">"+controls+
    "</subform><subform name=\"body\" layout=\"tb\" w=\"60pt\"><bind match=\"none\"/><keep intact=\"none\"/>"+row+"</subform>"+Paint("footer",magenta,60,10)+"</subform>";
}
std::vector<uint8_t> Fixture(const Spec& spec,bool native_control){
  auto xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"+RootTemplate("root",spec,cyan,native_control);
  if(spec.alternative_master)xml+=RootTemplate("other",spec,orange,native_control);
  xml+="</template>";
  // Template/dataset constructors are unavailable through the public C API.
  // Real public input dispatches clone/remerge/recalculate and current views.
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>","<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 240] >>",
    "<< /Fields [] /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
    PdfStream("",xml),PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root><shared>10</shared><row><amount>1</amount></row><row><amount>2</amount></row><row><amount>3</amount></row></root></xfa:data></xfa:datasets>"),PdfStream("","</xdp:xdp>")});
}
struct Loaded {
  forms::XfaCallbacks callbacks;Session s;
  Loaded(const Library& owner,const std::vector<uint8_t>& bytes):s(owner.api(),bytes,true,true,2,[this](forms::FormHost& host){callbacks.Configure(host);}){}
};
void Execute(Session& s,int action){
  s.host->alerts.clear();s.api.FORM_ForceToKillFocus(s.form);
  const int x=195+20*action;
  ASSERT_EQ(s.api.FPDFPage_HasFormFieldAtPoint(s.form,s.page,x,5),FPDF_FORMFIELD_XFA_PUSHBUTTON);
  // Remerge can retire the dispatching button while its event is running.
  // Its fresh host callback and reacquired page handles establish completion.
  s.Click(x,5);
  for(size_t page=0;page<s.host->pages.size();++page)s.ClosePage(static_cast<int>(page));
  s.page=s.host->Page(0);ASSERT_NE(s.page,nullptr);
  ASSERT_EQ(s.host->alerts.size(),1u);EXPECT_EQ(std::get<0>(s.host->alerts.front()),u"PASS");
  s.api.FORM_ForceToKillFocus(s.form);s.api.FORM_OnMouseMove(s.form,s.page,0,235,180);
  EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
}
Frames Render(Session& s){
  Frames frames;const int pages=s.api.FPDF_GetPageCount(s.document);EXPECT_GE(pages,1);EXPECT_LE(pages,8);
  if(pages<1||pages>8)return frames;
  for(int page=0;page<pages;++page){frames.push_back(s.Draw(240,240,0,0,0,0,0,0,page));}
  return frames;
}
size_t Count(const Frames& images,RGB color){
  size_t count=0;for(const auto& image:images)for(size_t at=0;at<image.pixels.size();at+=4)
    count+=image.pixels[at]==color[2]&&image.pixels[at+1]==color[1]&&image.pixels[at+2]==color[0];
  return count;
}
void Pixel(const Frames& images,int page,int x,int y,RGB color){
  ASSERT_LT(static_cast<size_t>(page),images.size());const auto& image=images[page];const auto at=(y*image.width+x)*4;
  EXPECT_EQ(image.pixels[at],color[2])<<page<<":"<<x<<","<<y;EXPECT_EQ(image.pixels[at+1],color[1]);EXPECT_EQ(image.pixels[at+2],color[0]);
}
void Inspect(const Frames& images,size_t rows,bool alternative){
  // First master fits two30pt rows after the20pt controller; later masters
  // fit three. The10pt footer also proves traversal reached the final row.
  int pages=1;int used=20;
  for(size_t i=0;i<rows;++i){if(used+30>100){++pages;used=0;}used+=30;}
  if(used+10>100)++pages;
  ASSERT_EQ(images.size(),static_cast<size_t>(pages));EXPECT_EQ(Count(images,green),rows*1800u);EXPECT_EQ(Count(images,magenta),600u);
  EXPECT_EQ(Count(images,alternative?orange:cyan),64u*images.size());EXPECT_EQ(Count(images,alternative?cyan:orange),0u);
  int page=0,y=20;for(size_t i=0;i<rows;++i){if(y+30>100){++page;y=0;}Pixel(images,page,5,y+5,green);Pixel(images,page,65,y+5,{255,255,255});y+=30;}
  Pixel(images,page,5,y+5,magenta);
}
void Compare(const Frames& a,const Frames& b,Artifacts& artifacts,const std::string& label){
  ASSERT_EQ(a.size(),b.size());for(size_t page=0;page<a.size();++page)CompareRaster(a[page],b[page],artifacts,label+std::to_string(page));
}
void RunScenario(const Spec& spec){
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);Artifacts artifacts;
  std::array<Frames,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()){
    const auto bytes=Fixture(spec,writer->name()=="native");artifacts.Pdf(spec.name+writer->name()+"-input",bytes);
    SCOPED_TRACE(writer->name());Loaded loaded(*writer,bytes);ASSERT_TRUE(loaded.s.valid());ASSERT_TRUE(loaded.s.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(Inspect(Render(loaded.s),3,false));
    ASSERT_NO_FATAL_FAILURE(Execute(loaded.s,0));
    ASSERT_NO_FATAL_FAILURE(Execute(loaded.s,1));
    originals[backend]=Render(loaded.s);
    ASSERT_NO_FATAL_FAILURE(Inspect(originals[backend],spec.values.size(),spec.alternative_master));
    for(size_t page=0;page<originals[backend].size();++page)artifacts.Image(spec.name+writer->name()+"-live-"+std::to_string(page),originals[backend][page]);
    for(FPDF_DWORD flags:{FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}){
      const auto saved=SaveDocument(loaded.s.api,loaded.s.document,artifacts,spec.name+writer->name()+std::to_string(flags),flags);ASSERT_FALSE(saved.empty());
      std::array<Frames,2> reloaded;size_t reader_index=0;
      for(const auto* reader:Implementations()){
        SCOPED_TRACE(reader->name());Loaded current(*reader,saved);ASSERT_TRUE(current.s.valid());ASSERT_TRUE(current.s.xfa_loaded);
        ASSERT_NO_FATAL_FAILURE(Execute(current.s,1));reloaded[reader_index]=Render(current.s);
        ASSERT_NO_FATAL_FAILURE(Inspect(reloaded[reader_index],spec.values.size(),spec.alternative_master));
        Compare(originals[backend],reloaded[reader_index],artifacts,spec.name+writer->name()+reader->name()+std::to_string(flags));++reader_index;
      }
      Compare(reloaded[0],reloaded[1],artifacts,spec.name+writer->name()+"-readers"+std::to_string(flags));
    }
    ++backend;
  }
  Compare(originals[0],originals[1],artifacts,spec.name+"-backends");
}
TEST(CoverageXfaCloneRebinding, AppendedDeepCloneBindsIndependentFieldsAndCalculationInputs) {
  RunScenario({"append-row-clone",
    "verify([1,2,3],10);var original=d.root.row,c=original.clone(true);yes(c!==original);eq(c.parent,null);c.amount.value='8';d.root.nodes.insert(c,null);"
    "var retired=b.row;refresh();verify([1,2,3,8],10);yes(b.row!==retired);eq(clean(original.amount.value),'1');",{1,2,3,8}});
}
TEST(CoverageXfaCloneRebinding, PrependingAClonedRecordRebuildsLogicalIndicesWithoutSharingValues) {
  RunScenario({"prepend-row-clone",
    "var original=d.root.row,c=original.clone(true);c.amount.value='9';d.root.nodes.insert(c,original);refresh();verify([9,1,2,3],10);"
    "b.row.amount.rawValue='7';xfa.form.recalculate(0);verify([7,1,2,3],10);eq(clean(original.amount.value),'1');b.row.amount.rawValue='9';xfa.form.recalculate(0);",{9,1,2,3}});
}
TEST(CoverageXfaCloneRebinding, ReplacingAClonedMiddleRecordDisconnectsTheDetachedDatasetsValue) {
  RunScenario({"replace-middle-clone",
    "var old=d.root.resolveNode('row[1]'),c=old.clone(true);c.amount.value='7';d.root.nodes.insert(c,old);d.root.nodes.remove(old);refresh();verify([1,7,3],10);"
    "old.amount.value='99';xfa.form.recalculate(0);verify([1,7,3],10);eq(clean(c.amount.value),'7');",{1,7,3}});
}
TEST(CoverageXfaCloneRebinding, ReplacingTheWholeClonedRecordRebindsEveryCurrentFieldToNewDataIdentity) {
  RunScenario({"replace-root-clone",
    "var old=d.root,c=old.clone(true);c.row.amount.value='6';c.shared.value='20';d.nodes.insert(c,old);d.nodes.remove(old);refresh();verify([6,2,3],20);"
    "old.row.amount.value='99';old.shared.value='90';xfa.form.recalculate(0);verify([6,2,3],20);"
    "yes(c.parent===d,'cloned record parent');eq(d.resolveNodes('root[*]').length,1);yes(d.resolveNode('root')===c,'same cloned record resolved root');yes(d.root===c,'same cloned record root');",{6,2,3},20});
}
TEST(CoverageXfaCloneRebinding, DeepCloneKeepsNestedMetadataIndependentAcrossLaterRebinding) {
  RunScenario({"nested-record-clone",
    "d.root.row.loadXML('<row><amount>4</amount><meta><code>A</code></meta></row>',true,true);refresh();verify([4,2,3],10);"
    "var original=d.root.row,c=original.clone(true);c.amount.value='5';c.meta.code.value='B';d.root.nodes.insert(c,null);refresh();verify([4,2,3,5],10);"
    "eq(clean(original.meta.code.value),'A');eq(clean(c.meta.code.value),'B');yes(original.meta!==c.meta);",{4,2,3,5}});
}
TEST(CoverageXfaCloneRebinding, RepeatedDatasetReorderingReacquiresCurrentFormInstancesAndRecordScopes) {
  RunScenario({"reorder-record-generations",
    "var first=d.root.row,last=d.root.resolveNode('row[2]');d.root.nodes.insert(last,first);refresh();verify([3,1,2],10);"
    "last=currentRow(last,0);d.root.nodes.insert(last,null);refresh();verify([1,2,3],10);first=currentRow(first,0);d.root.nodes.insert(first,null);refresh();verify([2,3,1],10);",{2,3,1}});
}
TEST(CoverageXfaCloneRebinding, RepeatedXmlRecordReplacementRetiresObsoleteGenerationsBeforeCalculatingAgain) {
  RunScenario({"replace-record-generations",
    "var old=d.root.row;d.root.loadXML('<root><shared>10</shared><row><amount>4</amount></row></root>',true,true);refresh();verify([4],10);"
    "d.root.loadXML('<root><shared>10</shared><row><amount>5</amount></row><row><amount>6</amount></row><row><amount>7</amount></row><row><amount>8</amount></row></root>',true,true);refresh();verify([5,6,7,8],10);"
    "d.root.loadXML('<root><shared>10</shared><row><amount>9</amount></row><row><amount>10</amount></row></root>',true,true);refresh();old.amount.value='99';xfa.form.recalculate(0);verify([9,10],10);",{9,10}});
}
TEST(CoverageXfaCloneRebinding, EmptyDatasetCanBeRepopulatedWithDetachedRecordClonesAndReboundAgain) {
  RunScenario({"empty-clone-repopulation",
    "var a=d.root.row.clone(true),c=d.root.resolveNode('row[2]').clone(true);d.root.loadXML('<root><shared>10</shared></root>',true,true);refresh();verify([],10);"
    "a.amount.value='4';c.amount.value='6';d.root.nodes.insert(a,null);d.root.nodes.insert(c,null);refresh();verify([4,6],10);",{4,6}});
}
TEST(CoverageXfaCloneRebinding, GlobalMirrorsAndCalculationDependenciesFollowTheReplacementRecord) {
  RunScenario({"global-cloned-record",
    "var old=d.root,c=old.clone(true);c.shared.value='20';d.nodes.insert(c,old);d.nodes.remove(old);refresh();verify([1,2,3],20);"
    "b.row.shared.rawValue='30';xfa.form.recalculate(0);verify([1,2,3],30);eq(clean(c.shared.value),'30');"
    "old.shared.value='99';xfa.form.recalculate(0);verify([1,2,3],30);",{1,2,3},30});
}
TEST(CoverageXfaCloneRebinding, UnboundEditsSurviveBoundValueChangesAndResetOnlyWhenFormsAreRemerged) {
  RunScenario({"unbound-live-and-remerge",
    "b.row.local.rawValue='Local';b.row.amount.rawValue='5';xfa.form.recalculate(0);eq(clean(b.row.local.rawValue),'Local');"
    "eq(Number(clean(b.row.total.rawValue)),20);var old=b.row;refresh();verify([5,2,3],10);yes(b.row!==old);"
    "b.row.local.rawValue='Second';eq(clean(b.row.local.rawValue),'Second');refresh();verify([5,2,3],10);",{5,2,3}});
}
TEST(CoverageXfaCloneRebinding, RemovingTheFirstBoundInstanceThenReorderingDataReplacesCurrentPageOwners) {
  RunScenario({"remove-first-rebind",
    // RemoveItem(true) detaches the row and each separately bound child data
    // node. Retain the actual leaf before removal rather than looking it up
    // through a container that correctly no longer owns that child.
    "var old=b.row,oldData=old.dataNode,oldAmount=old.amount.dataNode;b._row.removeInstance(0);xfa.layout.relayout();eq(b._row.count,2);eq(oldData.resolveNode('amount'),null);"
    "refresh();verify([2,3],10);var c=d.root.row.clone(true);c.amount.value='8';d.root.nodes.insert(c,d.root.row);refresh();verify([8,2,3],10);"
    "oldAmount.value='99';xfa.form.recalculate(0);verify([8,2,3],10);",{8,2,3}});
}
TEST(CoverageXfaCloneRebinding, ClonedChildrenInANewRecordSelectTheAlternativeRootAndItsMasterPage) {
  RunScenario({"replace-root-master",
    // createNode supplies a real <other> XML mapping. Merely changing the
    // Name of a <root> clone changes SOM but does not rename its saved tag.
    "var old=d.root,c=xfa.datasets.createNode('dataGroup','other');for(var i=0;i<old.nodes.length;i++)c.nodes.insert(old.nodes.item(i).clone(true),null);"
    "d.nodes.insert(c,old);d.nodes.remove(old);refresh();eq(r.name,'other');verify([1,2,3],10);"
    "old.row.amount.value='99';xfa.form.recalculate(0);verify([1,2,3],10);",{1,2,3},10,true});
}
}  // namespace
}  // namespace pdfoxium::integration
