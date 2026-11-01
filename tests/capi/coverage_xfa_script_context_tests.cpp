#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;

std::string ContextField(const std::string& name,const std::string& value,int y,int width=180) {
  return "<field name=\""+name+"\" x=\"0pt\" y=\""+std::to_string(y)+"pt\" w=\""+std::to_string(width)+
      "pt\" h=\"20pt\"><bind match=\"none\"/><ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"12pt\"/>"
      "<value><text>"+value+"</text></value>"+
      forms::Event("if(this.rawValue!==null)this.rawValue=String(this.rawValue).replace(/^\\n/,'');","initialize")+"</field>";
}
std::string VariableScript(const std::string& name,const std::string& source) {
  return "<script name=\""+name+"\" contentType=\"application/x-javascript\"><![CDATA["+source+"]]></script>";
}
std::string CheckedContext(const std::string& body) {
  return "function eq(a,b){if(a===b)return;if(Array.isArray(a)&&Array.isArray(b)&&JSON.stringify(a)===JSON.stringify(b))return;throw 'expected '+String(b)+' got '+String(a);}"
      "function yes(v){if(!v)throw 'assertion';}function throws(f){var caught=false;try{f();}catch(e){caught=true;}if(!caught)throw 'expected exception';}"
      "try{this.caption.value.text.value='Run';var root=xfa.form.root;var rustBackend=root.report.rawValue==='rust';"+body+
      ";root.report.rawValue='PASS';}catch(e){xfa.form.root.report.rawValue='FAIL';xfa.host.messageBox(String(e));}";
}
void AlignContextCdata(std::string& xml,size_t preceding) {
  // The dedicated original-input tests cover native's split-delimiter bug.
  // Keep this independent realm/SourceSet comparison away from those planes.
  size_t from=0;
  while((from=xml.find("<![CDATA[",from))!=std::string::npos) {
    if((preceding+from)%1024>1015){xml.insert(from,"        ");from+=8;}
    const auto close=xml.find("]]>",from+9);
    if(close==std::string::npos)break;
    size_t end=close;
    while((preceding+end)%1024>1021){xml.insert(end," ");++end;}
    from=end+3;
  }
}
std::vector<uint8_t> ContextFixture(const std::string& body,int box_x=0) {
  const std::string preamble="<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
  const std::string config="<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender>"
      "</acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
  const std::string sources="<sourceSet xmlns=\"http://www.xfa.org/schema/xfa-source-set/2.8/\">"
      "<source name=\"primary\" id=\"primarySource\"><connect connection=\"local\" ref=\"rows\" timeout=\"15\"/></source>"
      "<source name=\"secondary\"><connect connection=\"backup\" ref=\"archive\" timeout=\"30\"/></source></sourceSet>";
  const auto alpha=VariableScript("alpha",R"JS(
    var counter=1,seedRuns=1,state={history:[]};let privateOffset=10;
    function alphaReset(){counter=1;state.history=[];}
    function alphaIncrement(n){this.counter+=n;state.history.push(counter);return counter+privateOffset;}
    function lexicalIncrement(n){counter+=n;state.history.push(counter);return counter+privateOffset;}
    function setCounter(n){counter=n;return counter;}
    function owner(){return this.name;}
    function amountValue(){return amount.rawValue;}
    function writeAmount(v){amount.rawValue=v;return amount.rawValue;}
    function fail(){counter++;throw {tag:'expected',value:counter};}
    function chooseSource(){return xfa.resolveNode('sourceSet.primary').connect.getAttribute('connection');}
  )JS");
  const auto beta=VariableScript("beta","var counter=100;function betaReset(){counter=100;}function betaIncrement(n){counter+=n;return counter;}");
  const auto box=VariableScript("localOps",R"JS(
    var calls=0;
    function ownValue(){calls++;return local.rawValue;}
    function parentValue(){return amount.rawValue;}
    function writeLocal(v){local.rawValue=v;return local.rawValue;}
    function move(){x='20pt';return x;}
    function restore(){x='0pt';}
    function setLocale(v){locale=v;return locale;}
  )JS");
  std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" locale=\"en_US\" restoreState=\"auto\">"
      "<variables>"+alpha+beta+"</variables>"
      "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"200pt\" long=\"220pt\"/>"
      "<contentArea w=\"200pt\" h=\"220pt\"/></pageArea></pageSet>"
      "<draw x=\"0pt\" y=\"0pt\" w=\"40pt\" h=\"20pt\"><value><rectangle><fill><color value=\"255,0,0\"/></fill>"
      "<edge presence=\"hidden\"/></rectangle></value></draw>"+
      ContextField("report","Ready",20)+
      "<field name=\"run\" x=\"0pt\" y=\"40pt\" w=\"80pt\" h=\"20pt\"><bind match=\"none\"/><ui><button/>"
      "</ui><font typeface=\"Helvetica\" size=\"10pt\"/><caption><value><text>Run</text></value></caption>"+
      forms::Event(CheckedContext(body),"click")+"</field>"+ContextField("amount","Initial",70)+
      "<subform name=\"box\" layout=\"position\" x=\""+std::to_string(box_x)+"pt\" y=\"110pt\" w=\"180pt\" h=\"20pt\" locale=\"en_US\">"
      "<variables>"+box+"</variables>"+ContextField("local","Local",0)+"</subform></subform></template>";
  AlignContextCdata(xml,preamble.size()+config.size()+sources.size());
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 220] /Resources << /Font << /Helv 11 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (sourceSet) 7 0 R (template) 8 0 R (datasets) 9 0 R (postamble) 10 0 R] >>",
      PdfStream("",preamble),PdfStream("",config),PdfStream("",sources),PdfStream("",xml),
      PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
      PdfStream("","</xdp:xdp>"),"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"});
}
void ExecuteContext(Session& s,const Library& owner,const std::u16string& expected,int local_x=8) {
  ASSERT_TRUE(s.Focus(8,30));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
  s.Replace(owner.name()=="rust"?u"rust":u"native");s.api.FORM_ForceToKillFocus(s.form);
  s.host->alerts.clear();s.Click(8,50);s.api.FORM_ForceToKillFocus(s.form);
  ASSERT_TRUE(s.Focus(8,30));
  std::string errors;for(const auto& alert:s.host->alerts)errors+=std::string(std::get<0>(alert).begin(),std::get<0>(alert).end())+"\n";
  EXPECT_EQ(s.Text(),u"PASS")<<errors;EXPECT_TRUE(s.host->alerts.empty())<<errors;
  s.api.FORM_ForceToKillFocus(s.form);ASSERT_TRUE(s.Focus(8,80));EXPECT_EQ(s.Text(),expected);
  s.api.FORM_ForceToKillFocus(s.form);ASSERT_TRUE(s.Focus(local_x,120));EXPECT_EQ(s.Text(),u"Local");
  s.api.FORM_ForceToKillFocus(s.form);EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
}
void CompareContext(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,RasterTolerance{24,0,.2});
  CompareRaster(CropRaster(a,0,0,200,20),CropRaster(b,0,0,200,20),artifacts,label+"-landmark");
}
int LocalInkLeft(const Raster& image) {
  int left=image.width;
  for(int y=110;y<130;++y)for(int x=0;x<image.width;++x) {
    const auto pixel=(static_cast<size_t>(y)*image.width+x)*4;
    if(image.pixels[pixel]<128&&image.pixels[pixel+1]<128&&image.pixels[pixel+2]<128)left=std::min(left,x);
  }
  return left;
}
void ContextScenario(const std::string& body,const std::u16string& expected=u"Ready",int local_x=8) {
  Artifacts artifacts;const auto bytes=ContextFixture(body);std::array<Raster,2> images;size_t backend=0;
  const bool moved=local_x==28;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Session s(writer->api(),bytes,true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    const auto before=s.Draw(200,220);ASSERT_NO_FATAL_FAILURE(ExecuteContext(s,*writer,expected,local_x));
    // Repeat the same public event: a cached variables realm must remain usable
    // without rerunning its declarations or borrowing a different document's VM.
    ASSERT_NO_FATAL_FAILURE(ExecuteContext(s,*writer,expected,local_x));images[backend]=s.Draw(200,220);
    forms::ExpectInk(images[backend],0,20,180,40);EXPECT_NE(CropRaster(before,0,20,180,20).pixels,CropRaster(images[backend],0,20,180,20).pixels);
    Raster healthy;
    if(moved) {
      // Native changes the attribute while retaining the old live layout even
      // after an explicit relayout. Preserve that original paint; Rust must
      // already agree with a healthy fixture laid out at its final position.
      artifacts.Image(writer->name()+"-original-owner-attribute",images[backend]);
      EXPECT_EQ(LocalInkLeft(images[backend]),writer->name()=="native"?1:21);
      Session control(writer->api(),ContextFixture("root.amount.rawValue='Ready';",20),true,true);
      ASSERT_TRUE(control.valid());ASSERT_TRUE(control.xfa_loaded);(void)control.Draw(200,220);
      ASSERT_NO_FATAL_FAILURE(ExecuteContext(control,*writer,expected,local_x));healthy=control.Draw(200,220);
      EXPECT_EQ(LocalInkLeft(healthy),21);
      if(writer->name()=="rust")CompareContext(images[backend],healthy,artifacts,"original-rust-owner-attribute");
      // The healthy native scene has the same final x20 attributes but obtains
      // its layout directly on load. Use it for the shared visible reference;
      // saves below still come from the original mutated document. Both saved
      // readers must paint the final position without altering their input.
      images[backend]=healthy;
    }
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-contexts");ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Session reload(reader->api(),saved,true,true);ASSERT_TRUE(reload.valid());ASSERT_TRUE(reload.xfa_loaded);
      (void)reload.Draw(200,220);ASSERT_NO_FATAL_FAILURE(ExecuteContext(reload,*reader,expected,local_x));
      if(moved) {
        const auto original=reload.Draw(200,220);artifacts.Image(writer->name()+reader->name()+"-original-owner-reload",original);
        EXPECT_EQ(LocalInkLeft(original),21);
        if(reader->name()=="rust")CompareContext(original,healthy,artifacts,writer->name()+"-original-rust-owner-reload");
      }
      CompareContext(images[backend],reload.Draw(200,220),artifacts,writer->name()+reader->name()+"-contexts");
    }
    ++backend;
  }
  CompareContext(images[0],images[1],artifacts,"contexts-engines");
}

TEST(CoverageXfaScriptContexts, NamedVariablesRealmsKeepCountersIsolatedAndInitializeOnlyOnce) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ContextScenario(R"JS(
    var a=root.resolveNode('variables.alpha'),b=root.resolveNode('variables.beta');
    a.alphaReset();b.betaReset();eq(a.counter,1);eq(b.counter,100);eq(a.seedRuns,1);
    eq(a.alphaIncrement(2),rustBackend?13:11);eq(a.counter,rustBackend?3:1);
    a.alphaReset();eq(a.lexicalIncrement(2),13);eq(b.betaIncrement(4),104);eq(a.counter,3);
    eq(root.resolveNode('variables.alpha').counter,3);eq(a.seedRuns,1);eq(a.state.history,[3]);
    eq(a.writeAmount('Ready'),'Ready');
  )JS");
}

TEST(CoverageXfaScriptContexts, ExportedFunctionsRetainTheirVariablesGlobalWhenExtractedOrRebound) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ContextScenario(R"JS(
    var a=root.resolveNode('variables.alpha');a.alphaReset();var original=a.alphaIncrement;
    eq(original.call({counter:900},2),rustBackend?13:11);a.alphaReset();
    var f=rustBackend?a.alphaIncrement:a.lexicalIncrement;
    eq(f.call({counter:900},2),13);eq(a.counter,3);
    eq(f.apply(null,[1]),14);eq(a.counter,4);eq(f.bind({counter:700})(3),17);
    eq(a.state.history,[3,4,7]);eq(a.privateOffset,undefined);
    var write=a.writeAmount;eq(write.call({},'Ready'),'Ready');eq(a.amountValue(),'Ready');
  )JS");
}

TEST(CoverageXfaScriptContexts, ExportAssignmentsPreserveVarDeclarationsAndNewCallableProperties) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ContextScenario(R"JS(
    var a=root.resolveNode('variables.alpha');a.alphaReset();a.counter=20;eq(a.counter,rustBackend?20:1);
    if(!rustBackend)eq(a.setCounter(20),20);
    var increment=rustBackend?a.alphaIncrement:a.lexicalIncrement;eq(increment(1),31);
    a.exportedValue='added';eq(a.exportedValue,'added');a.exportedValue=42;eq(a.exportedValue,42);
    a.exportedValue=undefined;eq(a.exportedValue,rustBackend?undefined:42);
    a.exportedValue='cleared';eq(a.exportedValue,'cleared');
    a.callable=function(){return this.counter;};eq(a.callable(),21);
    eq(a.writeAmount('Ready'),'Ready');
  )JS");
}

TEST(CoverageXfaScriptContexts, NestedVariablesFindTheirOwnChildrenAndParentSiblings) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ContextScenario(R"JS(
    var a=root.resolveNode('variables.alpha'),inner=root.box.resolveNode('variables.localOps');
    inner.restore();root.box.local.rawValue='Local';eq(inner.ownValue(),'Local');
    a.writeAmount('Parent');eq(inner.parentValue(),'Parent');
    eq(inner.writeLocal('Changed'),'Changed');eq(root.box.local.rawValue,'Changed');
    root.box.local.rawValue='Local';eq(a.owner(),rustBackend?'root':'alpha');eq(a.writeAmount('Ready'),'Ready');
  )JS");
}

TEST(CoverageXfaScriptContexts, ThrowingExportedFunctionsRestoreTheCallingEventAndRealm) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ContextScenario(R"JS(
    var a=root.resolveNode('variables.alpha');a.alphaReset();var caught=false;
    try{a.fail();}catch(e){caught=true;eq(e.tag,'expected');eq(e.value,2);}yes(caught);eq(a.counter,2);
    eq(xfa.event.target,undefined);eq(this.name,'run');eq(a.alphaIncrement(1),rustBackend?13:12);
    a.setCounter(2);eq(a.lexicalIncrement(1),13);
    eq(a.writeAmount('Ready'),'Ready');eq(root.amount.rawValue,'Ready');
  )JS");
}

TEST(CoverageXfaScriptContexts, BareGlobalAttributeWritesRelayoutOnlyTheirVariablesOwner) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ContextScenario(R"JS(
    var inner=root.box.resolveNode('variables.localOps');inner.restore();
    eq(inner.setLocale('fr_CA'),'fr_CA');eq(root.box.locale,'fr_CA');inner.setLocale('en_US');
    eq(inner.move(),'20pt');eq(root.box.x,'20pt');eq(root.run.x,'0pt');
    eq(inner.ownValue(),'Local');root.amount.rawValue='Ready';
  )JS",u"Ready",28);
}

TEST(CoverageXfaScriptContexts, SourceSetPacketQueriesRetainTypedConnectPropertiesAndScopedLookups) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ContextScenario(R"JS(
    var set=xfa.resolveNode('sourceSet');eq(set.className,'sourceSet');eq(set.nodes.length,2);
    var a=set.resolveNode('primary'),b=set.resolveNode('secondary');eq(a.className,'source');eq(a.name,'primary');eq(b.name,'secondary');
    eq(a.connect.getAttribute('timeout'),'15');eq(b.connect.getAttribute('timeout'),'30');
    eq(a.connect.getAttribute('ref'),'rows');eq(b.connect.getAttribute('connection'),'backup');
    eq(root.resolveNode('variables.alpha').chooseSource(),'local');
    eq(set.resolveNodes('#source[*]').length,2);eq(a.parent.className,'sourceSet');
    root.amount.rawValue='Ready';
  )JS");
}

TEST(CoverageXfaScriptContexts, SourceMethodsKeepTheirPublishedNoopAndArityContractsWithoutDatabaseAccess) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ContextScenario(R"JS(
    var source=xfa.resolveNode('sourceSet.primary');
    var methods=['addNew','cancel','cancelBatch','close','delete','first','hasDataChanged','isBOF','isEOF','last','next','open','previous','requery','resync','update','updateBatch'];
    var poison={toString:function(){throw 'must not coerce';},valueOf:function(){throw 'must not coerce';}};
    for(var i=0;i<methods.length;++i){var name=methods[i];eq(typeof source[name],'function');eq(source[name](),undefined);throws(function(){source[name](poison);});}
    eq(source.db,undefined);source.db=poison;eq(source.db,undefined);
    eq(source.connect.getAttribute('connection'),'local');eq(source.parent.nodes.length,2);
    root.amount.rawValue='Ready';
  )JS");
}

}  // namespace
}  // namespace pdfoxium::integration
