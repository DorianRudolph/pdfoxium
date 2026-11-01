#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

// The public API has no constructors for XDP packets or field/data trees.
// The fixture contains only this envelope; actual resolving, scripting, model
// mutations, layout, input, saving and rendering belong to each library.
std::string Field(const std::string& name, const std::string& value) {
  return "<field name=\""+name+"\" w=\"180pt\" h=\"20pt\"><bind match=\"none\"/>"
      "<ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"12pt\"/>"
      "<value>"+value+"</value></field>";
}
const std::string kModel =
    "<subform name=\"model\" layout=\"tb\">"+
    Field("item","<integer>1</integer>")+Field("item","<integer>2</integer>")+
    Field("item","<integer>3</integer>")+Field("a.b","<text>dot</text>")+
    "<subform name=\"branch\" layout=\"tb\">"+Field("leaf","<integer>9</integer>")+
    "</subform></subform>";

std::string Checked(const std::string& body) {
  return "var rustBackend=xfa.form.root.report.rawValue==='rust',checkIndex=0;"
      "function show(v){return v!==null&&typeof v==='object'&&!Array.isArray(v)?'[host object]':JSON.stringify(v);}"
      "function eq(a,b){++checkIndex;if(a===b)return;if(Array.isArray(a)&&Array.isArray(b)&&JSON.stringify(a)===JSON.stringify(b))return;throw 'check '+checkIndex+': expected '+show(b)+' got '+show(a);}"
      "function yes(v,label){++checkIndex;if(!v)throw 'check '+checkIndex+': '+(label||'assertion');}"
      "function sameNode(a,b){yes(a!==null&&b!==null);eq(a.className,b.className);eq(a.somExpression,b.somExpression);if(rustBackend)yes(a===b,'canonical Rust node identity');}"
      "function throws(f){var caught=false;try{f();}catch(e){caught=true;}if(!caught)throw 'expected exception';}"
      "function names(list){var a=[];for(var i=0;i<list.length;++i)a.push(list.item(i).name);return a;}"
      "try{this.caption.value.text.value='Run';"+body+";xfa.form.root.report.rawValue='PASS';}"
      "catch(e){xfa.form.root.report.rawValue='FAIL';xfa.host.messageBox(String(e));}";
}

std::vector<uint8_t> Fixture(const std::string& body, bool formcalc=false,
                            const std::string& model=kModel) {
  const std::string program=formcalc?body:Checked(body);
  const std::string button="<field name=\"run\" w=\"80pt\" h=\"20pt\"><bind match=\"none\"/>"
      "<ui><button/></ui><font typeface=\"Helvetica\" size=\"10pt\"/>"
      "<caption><value><text>Run</text></value></caption>"+
      forms::Event(program,"click",formcalc?"application/x-formcalc":"application/x-javascript")+"</field>";
  const std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
      "<subform name=\"root\" layout=\"tb\" locale=\"en_US\" restoreState=\"auto\">"
      "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/>"
      "<medium short=\"200pt\" long=\"300pt\"/><contentArea w=\"200pt\" h=\"300pt\"/></pageArea></pageSet>"
      "<draw w=\"40pt\" h=\"20pt\"><value><rectangle><fill><color value=\"255,0,0\"/>"
      "</fill><edge presence=\"hidden\"/></rectangle></value></draw>"+
      Field("report","<text>Ready</text>")+button+model+"</subform></template>";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Resources << /Font << /Helv 11 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (custom) 9 0 R (postamble) 10 0 R] >>",
      PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("",xml),
      PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root><item>10</item><item>20</item><item>30</item><group><leaf>9</leaf></group><a.b>dot</a.b></root></xfa:data></xfa:datasets>"),
      PdfStream("","<custom xmlns=\"urn:capi:packet\" old=\"original\">A<child>B</child><![CDATA[C]]><?acrobat data="
          "\"packet instruction\"?></custom>"),
      PdfStream("","</xdp:xdp>"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"});
}

void Execute(forms::Session& session,const Library& library) {
  // A normal public edit supplies the reader's identity to the script so
  // narrow source-confirmed native exceptions still work after cross-reload.
  // It is test data, not an engine hook or a value saved as a PDF contract.
  ASSERT_TRUE(session.Focus(8,25));
  ASSERT_TRUE(session.api.FORM_SelectAllText(session.form,session.page));
  session.Replace(library.name()=="rust"?u"rust":u"native");
  session.api.FORM_ForceToKillFocus(session.form);
  session.Click(8,50);
  ASSERT_TRUE(session.Focus(8,25));
  std::string errors;
  for(const auto& alert:session.host->alerts)
    errors+=std::string(std::get<0>(alert).begin(),std::get<0>(alert).end())+"\n";
  EXPECT_EQ(session.Text(),u"PASS")<<errors;
  EXPECT_TRUE(session.host->alerts.empty())<<errors;
  session.api.FORM_ForceToKillFocus(session.form);
  EXPECT_TRUE(session.host->callbacks_valid);
  EXPECT_FALSE(session.host->callback_error);
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,RasterTolerance{24,0,.2});
  CompareRaster(CropRaster(a,0,20,180,20),CropRaster(b,0,20,180,20),artifacts,
                label+"-visible-report",RasterTolerance{24,0,1});
  CompareRaster(CropRaster(a,0,0,200,20),CropRaster(b,0,0,200,20),artifacts,label+"-red-landmark");
}
void RunSomScenario(const std::string& program,bool save=false,bool formcalc=false,
         const std::string& model=kModel) {
  Artifacts artifacts;
  const auto bytes=Fixture(program,formcalc,model);
  std::array<Raster,2> images;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());
    forms::Session session(writer->api(),bytes,true,true);
    ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);
    ASSERT_TRUE(session.Focus(8,25));EXPECT_EQ(session.Text(),u"Ready");
    session.api.FORM_ForceToKillFocus(session.form);
    const auto before=session.Draw(200,300);
    Execute(session,*writer);images[backend]=session.Draw(200,300);
    forms::ExpectInk(images[backend],0,20,180,40);
    EXPECT_NE(CropRaster(before,0,20,180,20).pixels,CropRaster(images[backend],0,20,180,20).pixels);
    if(save) {
      const auto saved=SaveDocument(session.api,session.document,artifacts,writer->name()+"-som-xml");
      ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());forms::Session loaded(reader->api(),saved,true,true);
        ASSERT_TRUE(loaded.valid());ASSERT_TRUE(loaded.xfa_loaded);
        // Reexecute read-only/idempotent programs with each reader. This tests
        // the saved script/model, not arbitrary custom-packet mutation policy.
        Execute(loaded,*reader);Compare(images[backend],loaded.Draw(200,300),artifacts,
                                writer->name()+"-"+reader->name()+"-saved-program");
      }
    }
    ++backend;
  }
  Compare(images[0],images[1],artifacts,"som-xml-backends");
}

TEST(CoverageSomXml, AbsoluteEscapedClassWildcardAndRelativePathsResolveRealNodeIdentity) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSomScenario(R"JS(
    var r=xfa.form.root,m=r.model,items=m.resolveNodes('item[*]');
    eq(items.length,3);eq(names(items),['item','item','item']);
    eq(m.resolveNode('item').rawValue,1);eq(m.resolveNode('item[1]').rawValue,2);
    eq(m.resolveNode('item[2]').rawValue,3);eq(m.resolveNode('item[99]'),null);
    eq(m.resolveNode('item[-1]'),null);
    eq(m.resolveNode('a\\.b').rawValue,'dot');eq(m.resolveNode('branch.leaf').rawValue,9);
    eq(m.resolveNode('..leaf').rawValue,9);eq(m.resolveNode('#subform.leaf').rawValue,9);
    eq(names(m.resolveNodes('#field[*]')),['item','item','item','a.b']);
    var children=m.resolveNodes('*'),payload=[];
    for(var i=0;i<children.length;++i) {
      var child=children.item(i);
      if(child.className==='field'||child.className==='subform')payload.push(child.name);
    }
    eq(payload,['item','item','item','a.b','branch']);
    sameNode(m.resolveNode('$'),m);sameNode(m.resolveNode('parent'),r);
    sameNode(m.resolveNode('parent.model'),m);
    eq(m.resolveNode('$form.root.model.item[1]').rawValue,2);
    eq(m.resolveNode('$xfa.form.root.model.item[2]').rawValue,3);
    eq(m.resolveNode('$template.root.model.item[0]').name,'item');
    // Reopened native-written datasets can have the documented leading LF;
    // ignore only that serialization prefix, never internal value whitespace.
    eq(String(m.resolveNode('$record.item[1]').value).replace(/^\n/,''),'20');
    eq(String(m.resolveNode('!data.root.item[2]').value).replace(/^\n/,''),'30');
    var second=items.item(1);
    // Native CJX_List::item deliberately calls NewNormalXFAObject and
    // overwrites its binding cache. Rust retains canonical wrappers. Require
    // both aliases to identify the same actual node, never JSON {} equality.
    eq(second===items.item(1),rustBackend);
    sameNode(second.resolveNode('item[-1]'),items.item(0));
    sameNode(second.resolveNode('item[+0]'),second);sameNode(second.resolveNode('item[+1]'),items.item(2));
    eq(second.all.length,3);eq(second.classAll.length,4);
    sameNode(second.all.item(1),second);sameNode(second.classAll.item(3),m.resolveNode('a\\.b'));
  )JS",true);
}

TEST(CoverageSomXml, PredicatesUseRealJavaScriptFormCalcAndRestoreNestedResolutionContext) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSomScenario(R"JS(
    var m=xfa.form.root.model,self=this;
    var filtered=m.resolveNodes('item.(index == 1)').length;
    if(filtered!==1)throw 'predicate count '+filtered+' direct '+m.resolveNode('item[0]').index+','+
        m.resolveNode('item[1]').index+','+m.resolveNode('item[2]').index+
        ' explicit '+m.resolveNodes('item.(this.index == 1)').length;
    eq(filtered,1);
    eq(m.resolveNode('item.(index == 1)').rawValue,2);
    eq(m.resolveNodes('item.(rawValue >= 2)').length,2);
    eq(m.resolveNodes('item.(false)').length,0);eq(m.resolveNodes('item.(true)').length,3);
    eq(m.resolveNodes('item.[1]').length,3);eq(m.resolveNodes('item.[0]').length,0);
    eq(m.resolveNodes('item.[rawValue >= 2]').length,2);
    eq(m.resolveNode('item.(xfa.form.root.model.resolveNodes("item[*]").length == 3 && index == 2)').rawValue,3);
    eq(this.name,'run');yes(this===self);
    eq(m.resolveNodes('item.(throw new Error("predicate failure"))').length,0);
    eq(m.resolveNode('item[0]').rawValue,1);eq(this.name,'run');
    ['item[','item.(true','item.[1'].forEach(function(expression){
      var failed=false,result;try{result=m.resolveNode(expression);}catch(e){failed=true;}
      if(rustBackend)yes(failed,'malformed SOM must be rejected');
      else {yes(!failed);sameNode(result,m);}
      eq(m.resolveNode('item[1]').rawValue,2);eq(self.name,'run');
    });
    throws(function(){m.resolveNode();});throws(function(){m.resolveNodes('item','extra');});
  )JS",true);
}

TEST(CoverageSomXml, FormCalcDotDotdotAttributeListsAndAssignmentsUseTheRealSomTree) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSomScenario(R"FM(
    root.run.caption.value.text.value = "Run"
    $record.item[0] = 10
    $record.item[1] = 20
    $record.item[2] = 30
    root.model.item[0] = 1
    root.model.item[1] = 2
    root.model.item[2] = 3
    var total = Sum(root.model.item[*])
    var count = Count(root.model.item[*].name)
    var deep = Sum(root.model..leaf)
    var record = Sum($record.item[*])
    if (total == 6 and count == 3 and deep == 9 and record == 60) then
      root.model.item[1] = 12
      if (Sum(root.model.item[*]) == 16 and root.model.item[1] == 12) then
        root.model.item[1] = 2
        root.report.rawValue = "PASS"
      else
        root.report.rawValue = "FAIL"
      endif
    else
      root.report.rawValue = "FAIL"
    endif
  )FM",true,true);
}

TEST(CoverageSomXml, CustomPacketAttributesMixedTextCdataAndCoercionsMutateMappedXml) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSomScenario(R"JS(
    var p=xfa.resolveNode('custom');yes(p!==null);eq(p.className,'packet');
    eq(p.getAttribute('old'),'original');eq(p.getAttribute('missing'),'');eq(p.content,'AC');
    eq(p.setAttribute('value & <tag>','new'),null);eq(p.getAttribute('new'),'value & <tag>');
    eq(p.removeAttribute('old'),null);eq(p.getAttribute('old'),'');
    p.content=' D & < >';p.content=' E';eq(p.content,'AC D & < > E');
    var order=[];
    p.setAttribute({toString:function(){order.push('value');p.setAttribute('nested','inner');return 'outer';}},
                   {toString:function(){order.push('name');return 'new';}});
    // C++ argument evaluation order is not a JavaScript API guarantee.
    eq(order.sort(),['name','value']);eq(p.getAttribute('inner'),'nested');eq(p.getAttribute('new'),'outer');
    p.content={toString:function(){p.content='inner';return 'outer';}};
    eq(p.content,'AC D & < > Einnerouter');
    throws(function(){p.getAttribute();});throws(function(){p.getAttribute('a','b');});
    throws(function(){p.setAttribute('a');});throws(function(){p.removeAttribute();});
    var caught=false;try{p.setAttribute({toString:function(){throw 'stop';}},'never');}catch(e){caught=true;}
    // Native ReentrantToWideStringHelper squashes conversion exceptions;
    // Rust must propagate them and avoid the outer attribute write.
    eq(caught,rustBackend);eq(p.getAttribute('never'),'');eq(p.getAttribute('new'),'outer');
  )JS");
}

TEST(CoverageSomXml, DatasetXmlLoadsEntitiesNamespacesCdataInstructionsAndRejectsMalformedReplacement) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSomScenario(R"JS(
    var d=xfa.datasets,g=d.createNode('dataGroup','scratch');d.data.root.nodes.append(g);
    var xml='<person id="A"><name>Jane &amp; Doe</name><note><![CDATA[<raw>'+']'+']></note><?acrobat data="kept"?></person>';
    g.loadXML(xml,true,true);eq(g.resolveNode('name').value,'Jane & Doe');eq(g.resolveNode('note').value,'<raw>');
    var output=g.saveXML();yes(output.indexOf('Jane &amp; Doe')>=0);yes(output.indexOf('&lt;raw&gt;')>=0||output.indexOf('<![CDATA[')>=0);
    yes(output.indexOf('<?xml')==0);yes(output.indexOf('scratch')>=0);
    var copy=d.createNode('dataGroup','copy');d.data.root.nodes.append(copy);
    copy.loadXML(output,false,true);
    // Native's XML serializer can prepend a newline to a text element. Keep
    // exact input values above and remove only that known writer-added prefix
    // when checking a reparse of the writer's own XML.
    function text(n){return String(n.value).replace(/^\n/,'');}
    eq(text(copy.resolveNode('scratch.name')),'Jane & Doe');eq(text(copy.resolveNode('scratch.note')),'<raw>');
    var before=g.saveXML();g.loadXML('<broken a=unquoted />',true,true);eq(g.saveXML(),before);
    g.loadXML('',true,true);eq(g.saveXML(),before);
    g.loadXML('<next><value>Append</value></next>',false,false);
    eq(g.resolveNode('next.value').value,'Append');
    throws(function(){g.loadXML();});throws(function(){g.loadXML('x',true,true,1);});
    throws(function(){g.saveXML('unknown');});throws(function(){g.saveXML('pretty','extra');});
    yes(g.saveXML('pretty').indexOf('Append')>=0);
    d.data.root.nodes.remove(copy);d.data.root.nodes.remove(g);
  )JS",true);
}

TEST(CoverageSomXml, ModelCreationTypedDefaultsNamespacesAndReentrantCoercionKeepIdentity) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSomScenario(R"JS(
    var d=xfa.datasets,order=[];
    yes(d.isCompatibleNS('http://www.xfa.org/schema/xfa-data/1.0/'));eq(d.isCompatibleNS('urn:wrong'),false);
    var group=d.createNode({toString:function(){order.push('tag');return 'dataGroup';}},
        {toString:function(){order.push('name');return 'created';}},
        {toString:function(){order.push('namespace');return 'urn:ignored';}});
    eq(order,['name','namespace','tag']);eq(group.name,'created');eq(group.parent,null);
    d.data.root.nodes.append(group);sameNode(group.parent,d.data.root);
    var value=d.createNode('dataValue','value');group.nodes.append(value);
    value.value='12';eq(value.value,'12');value['{default}']='changed';eq(value.value,'changed');
    value.value=null;eq(value.value,null);value.value='restored';eq(value.value,'restored');
    var primitive=[['boolean',0,false],['boolean',2,true],['integer','12.75',12],
                   ['float',1.25,1.25],['decimal',1234.5,1234.5],['text','typed','typed']];
    primitive.forEach(function(row){var n=xfa.form.createNode(row[0]);yes(n!==null);n.value=row[1];eq(n.value,row[2]);});
    eq(d.createNode('field'),null);eq(d.createNode('notAnElement'),null);
    throws(function(){d.createNode();});throws(function(){d.createNode('dataValue','x','y','z');});
    throws(function(){d.isCompatibleNS();});throws(function(){xfa.template.createNode('ui','bad');});
    var poison={toString:function(){throw 'must not coerce unused argument';}};
    d.clearErrorList(poison,poison);d.context=poison;d.aliasNode=poison;
    eq(d.context,undefined);eq(d.aliasNode,undefined);
    var inner,outer=d.createNode('dataGroup',{toString:function(){inner=d.createNode('dataValue','inner');group.nodes.append(inner);return 'outer';}});
    group.nodes.append(outer);sameNode(group.nodes.namedItem('inner'),inner);sameNode(group.nodes.namedItem('outer'),outer);
    d.data.root.nodes.remove(group);
  )JS",true);
}

TEST(CoverageSomXml, AttachedAndSnapshotListsInsertMoveRemoveAndValidateWithoutChangingUnrelatedNodes) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSomScenario(R"JS(
    var d=xfa.datasets,root=d.data.root,g=d.createNode('dataGroup','listGroup');root.nodes.append(g);
    var list=g.nodes,a=d.createNode('dataValue','a'),b=d.createNode('dataValue','b'),c=d.createNode('dataValue','c');
    eq(list.length,0);list.append(a);list.append(c);list.insert(b,c);eq(names(list),['a','b','c']);
    var alias=list.namedItem('b');sameNode(alias,b);eq(list.namedItem('missing'),undefined);
    b.value='shared alias';eq(alias.value,'shared alias');
    list.insert(c,a);eq(names(list),['c','a','b']);list.remove(a);eq(names(list),['c','b']);eq(a.parent,null);
    list.insert(a,null);eq(names(list),['c','b','a']);sameNode(a.parent,g);
    throws(function(){list.item(-1);});throws(function(){list.item(99);});throws(function(){list.item();});
    throws(function(){list.append();});throws(function(){list.append({});});throws(function(){list.insert(a);});
    throws(function(){list.namedItem();});throws(function(){list.remove();});
    throws(function(){list.length=0;});throws(function(){list.className='fake';});
    var snapshot=xfa.form.root.model.resolveNodes('item[*]');eq(snapshot.length,3);
    var first=snapshot.item(0),middle=snapshot.item(1);snapshot.remove(middle);eq(snapshot.length,2);
    eq(xfa.form.root.model.resolveNodes('item[*]').length,3);
    snapshot.append(first);eq(snapshot.length,3);sameNode(snapshot.item(2),first);
    eq(first.parent.name,'model');eq(names(g.nodes),['c','b','a']);root.nodes.remove(g);
  )JS",true);
}

TEST(CoverageSomXml, ReflectedDeclaredAttributesRemainLiveAndNondeletableWhileCustomPropertiesAreOrdinary) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunSomScenario(R"JS(
    var node=xfa.form.createNode('subform','reflection'),original=node.name;
    yes('name' in node);var desc=Object.getOwnPropertyDescriptor(node,'name');yes(desc!==undefined);
    eq(desc.configurable,false);yes(desc.enumerable);
    eq(Reflect.deleteProperty(node,'name'),!rustBackend);eq(node.name,original);
    node.name='changed';eq(node.name,'changed');
    if(typeof desc.get==='function')eq(desc.get.call(node),'changed');
    else eq(Object.getOwnPropertyDescriptor(node,'name').value,'changed');
    // Native's virtual-property query reports DontDelete, so V8 rejects
    // replacing the declared attribute even though its absent deleter
    // reports success above. Rust consistently rejects both operations.
    eq(Reflect.defineProperty(node,'name',{value:'fake',writable:true,configurable:true}),false);
    eq(node.name,'changed');eq(node.getAttribute('name'),'changed');
    yes(Reflect.defineProperty(node,'custom',{value:17,writable:true,configurable:true,enumerable:true}));
    // Native's named interceptor reports define success but neither exposes
    // nor materializes this unknown property. Rust keeps ordinary JS expando
    // reads, descriptors, writes and enumeration as independent assertions.
    var custom=Object.getOwnPropertyDescriptor(node,'custom');
    if(rustBackend)eq(custom.value,17);else eq(custom,undefined);
    eq(node.custom,rustBackend?17:undefined);node.custom=23;
    eq(node.custom,rustBackend?23:undefined);
    custom=Object.getOwnPropertyDescriptor(node,'custom');
    if(rustBackend)eq(custom.value,23);else eq(custom,undefined);
    eq(Object.keys(node).indexOf('custom')>=0,rustBackend);yes(Reflect.deleteProperty(node,'custom'));
    eq(typeof node.custom,'undefined');eq(Object.getOwnPropertyDescriptor(node,'notDeclared'),undefined);
    var marker={},caught=false;
    try{node.name={toString:function(){node.name='nested';throw marker;}};}catch(e){caught=true;}
    eq(caught,rustBackend);eq(node.name,rustBackend?'nested':'');eq(node.className,'subform');
    throws(function(){node.className='field';});
  )JS");
}

}  // namespace
}  // namespace pdfoxium::integration
