#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <cmath>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;

// Validation/event nodes and dataset binding declarations have no public
// constructors. Public input below executes, lays out, edits and saves them.
std::string Text(const std::string& name,const std::string& value,int x,int y,
                 const std::string& children="",const std::string& binding="<bind match=\"none\"/>",int width=100) {
  return "<field name=\""+name+"\" x=\""+std::to_string(x)+"pt\" y=\""+std::to_string(y)+
      "pt\" w=\""+std::to_string(width)+"pt\" h=\"24pt\"><ui><textEdit multiLine=\"0\"/></ui>"+binding+
      "<font typeface=\"Helvetica\" size=\"12pt\"/><value><text>"+value+"</text></value>"+children+"</field>";
}
std::string Script(const std::string& body) {
  return "function eq(a,b){if(a!==b)throw 'expected '+String(b)+' got '+String(a);}"
      "function yes(v,label){if(!v)throw 'assert '+label;}"
      "function clean(v){return String(v===null?'':v).replace(/^\\n/,'');}"
      "function cleanItems(f){for(var i=0;i<f.nodes.length;i++){var group=f.nodes.item(i);if(group.className!=='items')continue;for(var j=0;j<group.nodes.length;j++){var item=group.nodes.item(j),value=String(item.value);if(value.charCodeAt(0)===10)item.value=value.slice(1);}}}"
      "function throws(f){var caught=false;try{f();}catch(e){caught=true;}yes(caught,'exception');}"
      "try{var r=xfa.form.root;this.caption.value.text.value='Run';var action=r.resolveNode('action');if(action)action.caption.value.text.value='Run';"+body+";r.report.rawValue='PASS';xfa.host.setFocus(r.report);}"
      "catch(e){xfa.form.root.report.rawValue='FAIL';xfa.host.messageBox('FAIL:'+String(e));}";
}
std::string Button(const std::string& name,int x,const std::string& body) {
  return "<field name=\""+name+"\" x=\""+std::to_string(x)+"pt\" y=\"50pt\" w=\"80pt\" h=\"24pt\">"
      "<bind match=\"none\"/><ui><button/></ui><font typeface=\"Helvetica\" size=\"10pt\"/>"
      "<caption><value><text>Run</text></value></caption>"+forms::Event(body,"click")+"</field>";
}
void AlignNativeXmlDelimiters(std::string& xml,size_t prefix) {
  // The pinned parser misses delimiters split across its 1024-character plane.
  // That defect has dedicated original-input regression coverage. Keep these
  // semantic fixtures healthy for the native oracle using harmless script
  // whitespace, preserving every attribute (including explicitly empty refs).
  for(size_t i=0;i<xml.size();) {
    const size_t length=xml.compare(i,9,"<![CDATA[")==0?9:xml.compare(i,3,"]]>")==0?3:0;
    if(!length){++i;continue;}
    const size_t offset=prefix+i;
    if(offset/1024!=(offset+length-1)/1024) {
      const size_t padding=1024-offset%1024;xml.insert(i,padding,' ');i+=padding;
    }
    i+=length;
  }
}
std::vector<uint8_t> Fixture(const std::string& model,const std::string& probe,
                           const std::string& data="<root/>",const std::string& action="") {
  const std::string preamble="<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
  const std::string config="<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
  std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
      "<subform name=\"root\" layout=\"position\" locale=\"en_US\" restoreState=\"auto\">"
      "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"240pt\" long=\"240pt\"/>"
      "<contentArea w=\"240pt\" h=\"240pt\"/></pageArea></pageSet>"
      "<draw x=\"200pt\" y=\"200pt\" w=\"20pt\" h=\"20pt\"><value><rectangle><fill><color value=\"0,255,0\"/></fill>"
      "<edge presence=\"hidden\"/></rectangle></value></draw>"+Text("report","Ready",0,20)+
      Button("probe",0,Script(probe))+(action.empty()?"":Button("action",120,action))+model+"</subform></template>";
  AlignNativeXmlDelimiters(xml,preamble.size()+config.size());
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 240] /Resources << /Font << /Helv 10 0 R >> >> >>",
    "<< /Fields [] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    PdfStream("",preamble),
    PdfStream("",config),
    PdfStream("",xml),
    PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data>"+data+"</xfa:data></xfa:datasets>"),
    PdfStream("","</xdp:xdp>"),
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"});
}
void Configure(forms::FormHost& h,int answer=JSPLATFORM_ALERT_RETURN_YES) {
  h.alert_result=answer;
  h.FFI_GetPageViewRect=[](FPDF_FORMFILLINFO* p,FPDF_PAGE page,double* left,double* top,double* right,double* bottom) {
    auto& host=forms::FormHost::Self(p);host.CheckPage(page);
    *left=0;*top=0;*right=host.api.FPDF_GetPageWidthF(page);*bottom=host.api.FPDF_GetPageHeightF(page);
  };
}
void Execute(Session& s);
Raster Paint(Session& s,const Library& library,bool choice,Artifacts& artifacts,const std::string& label,bool focused_choice=false) {
  artifacts.Image(label+"-before-settle",s.Draw(240,240));
  if(choice) {
    // Native only updates its combo button state on mouse-up, and rejects a
    // null successor before hiding its child caret. Establish the same real
    // pointer-release state, then transfer focus through the probe's host
    // call with a non-null report widget. Escape cancels the opened popup.
    EXPECT_TRUE(s.api.FORM_OnMouseMove(s.form,s.page,0,95,110));
    EXPECT_TRUE(s.api.FORM_OnLButtonDown(s.form,s.page,0,95,110));
    EXPECT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Escape,0));
    EXPECT_TRUE(s.api.FORM_OnLButtonUp(s.form,s.page,0,95,110));
    Execute(s);
    if(focused_choice) {
      // Native ComboEdit's KillFocus bypasses Edit::HideCaret. Keep this case
      // in an explicit focused Home state in both originals and reloads;
      // End/Home forces a cursor update even when its initial position is 0.
      EXPECT_TRUE(s.Focus(8,110));
      EXPECT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey));
      EXPECT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey));
      EXPECT_EQ(s.Text(true),u"");EXPECT_EQ(s.Text(),u"b");
    }
    int page=-2;FPDF_ANNOTATION focused=nullptr;
    EXPECT_TRUE(s.api.FORM_GetFocusedAnnot(s.form,&page,&focused));
    // Pinned GetFocusedAnnot explicitly returns its no-focus sentinel for
    // XFA (crbug.com/42270486); Rust supplies an owned live XFA handle.
    const bool owned_focus=focused_choice&&library.name()!="native";
    EXPECT_EQ(page,owned_focus?0:-1);
    if(owned_focus){EXPECT_NE(focused,nullptr);}else{EXPECT_EQ(focused,nullptr);}
    if(focused){EXPECT_EQ(s.api.FPDFAnnot_GetSubtype(focused),FPDF_ANNOT_XFAWIDGET);s.api.FPDFPage_CloseAnnot(focused);}
  }
  auto image=s.Draw(240,240);artifacts.Image(label+"-original",image);
  if(choice&&library.name()=="native") {
    // The pinned FWL manager paints the combo edit twice. Reuse the existing
    // source-proven single-paint comparison (xfa_form_tests.cpp), solely in
    // this black-on-white combo's edit rectangle; preserve the original above.
    for(int y=100;y<124;++y)for(int x=0;x<85;++x) {
      const auto offset=(static_cast<size_t>(y)*image.width+x)*4;
      for(size_t c=0;c<3;++c)image.pixels[offset+c]=static_cast<uint8_t>(std::lround(std::sqrt(image.pixels[offset+c]*255.0)));
    }
  }
  return image;
}
void Blur(Session& s) {
  // A real focus transfer also clears the combo's child-edit caret and hover;
  // forcing SDK focus off alone can leave those FWL states visible in native.
  s.api.FORM_ForceToKillFocus(s.form);s.Click(8,30);s.api.FORM_ForceToKillFocus(s.form);
  s.Click(230,230);s.api.FORM_ForceToKillFocus(s.form);
}
void Execute(Session& s) {
  Blur(s);(void)s.Draw(240,240);s.host->alerts.clear();s.Click(8,60);
  ASSERT_TRUE(s.Focus(8,30));
  std::string errors;for(const auto& a:s.host->alerts)errors+=std::string(std::get<0>(a).begin(),std::get<0>(a).end())+"\n";
  EXPECT_EQ(s.Text(),u"PASS")<<errors;Blur(s);
  EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,RasterTolerance{24,0,.25});
  CompareRaster(CropRaster(a,200,200,20,20),CropRaster(b,200,200,20,20),artifacts,label+"-landmark");
  for(int y=20;y<=150;y+=40)CompareRaster(CropRaster(a,0,y,240,30),CropRaster(b,0,y,240,30),
      artifacts,label+"-region"+std::to_string(y),RasterTolerance{24,0,1});
}
using Inspect=std::function<void(Session&)>;
void RunValidationBinding(const std::vector<uint8_t>& bytes,const Inspect& inspect={},int answer=JSPLATFORM_ALERT_RETURN_YES,
         bool save=true,const Inspect& interact={}) {
  Artifacts artifacts;std::array<Raster,2> originals;size_t backend=0;
  const bool choice=std::string(bytes.begin(),bytes.end()).find("<choiceList")!=std::string::npos;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Session s(writer->api(),bytes,true,true,2,[&](forms::FormHost& h){Configure(h,answer);});
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);const auto before=s.Draw(240,240);
    Execute(s);ASSERT_FALSE(::testing::Test::HasFailure());
    if(interact){interact(s);}
    if(inspect){inspect(s);}
    originals[backend]=Paint(s,*writer,choice,artifacts,writer->name()+"-paint");EXPECT_NE(before.pixels,originals[backend].pixels);
    forms::ExpectInk(originals[backend],0,20,100,24);
    if(save) {
      const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-validation-binding");ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());Session reopened(reader->api(),saved,true,true,2,[&](forms::FormHost& h){Configure(h,answer);});
        ASSERT_TRUE(reopened.valid());ASSERT_TRUE(reopened.xfa_loaded);Execute(reopened);ASSERT_FALSE(::testing::Test::HasFailure());
        if(interact){interact(reopened);}
        if(inspect){inspect(reopened);}
        Compare(originals[backend],Paint(reopened,*reader,choice,artifacts,writer->name()+"-"+reader->name()+"-paint"),artifacts,writer->name()+"-"+reader->name()+"-reload");
      }
    }
    ++backend;
  }
  Compare(originals[0],originals[1],artifacts,"validation-binding-backends");
}
void EmptyAndRecover(Session& s) {
  ASSERT_TRUE(s.Focus(8,110));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
  const auto text=s.Text();EXPECT_EQ(s.Text(true),text);
  // Native OnKeyDown(Delete) ignores selection and deletes one character;
  // Rust correctly deletes the selection (already tested by TextInput). Use
  // actual bounded single-character edits for this validation-lifecycle case.
  ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey));
  for(size_t i=0;i<text.size()&&!s.Text().empty();++i){ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Delete,0));}
  EXPECT_EQ(s.Text(),u"");Blur(s);
  ASSERT_TRUE(s.Focus(8,110));for(char16_t ch:std::u16string(u"Recovered")){ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,ch,0));}Blur(s);
}
void Message(Session& s,const std::u16string& expected) {
  std::string observed;for(const auto& a:s.host->alerts)observed+=std::string(std::get<0>(a).begin(),std::get<0>(a).end())+"\n";
  EXPECT_TRUE(std::any_of(s.host->alerts.begin(),s.host->alerts.end(),[&](const auto& a){return std::get<0>(a)==expected;}))<<observed;
  for(const auto& a:s.host->alerts) {
    EXPECT_NE(std::get<0>(a).substr(0,5),u"FAIL:");EXPECT_GE(std::get<2>(a),0);EXPECT_LE(std::get<2>(a),3);
    EXPECT_GE(std::get<3>(a),0);EXPECT_LE(std::get<3>(a),4);
  }
}
std::string Validate(const std::string& attributes,const std::string& children="") {
  return "<validate "+attributes+">"+children+"</validate>";
}
std::string Choice(const std::string& bind,const std::string& value="g") {
  return "<field name=\"choice\" x=\"0pt\" y=\"100pt\" w=\"100pt\" h=\"24pt\"><bind match=\"once\"/>"
      "<ui><choiceList open=\"userControl\"/></ui><font typeface=\"Helvetica\" size=\"12pt\"/>"
      "<value><text>"+value+"</text></value>"+bind+
      // Normalize the documented native-writer LF before the FWL edit caches
      // labels during its first render; probe-time cleanup is too late.
      forms::Event("for(var g=0;g<this.nodes.length;g++){var group=this.nodes.item(g);if(group.className!=='items')continue;"
          "for(var i=0;i<group.nodes.length;i++){var item=group.nodes.item(i),v=String(item.value);if(v.charCodeAt(0)===10)item.value=v.slice(1);}}var raw=this.rawValue;if(typeof raw==='string'&&raw.charCodeAt(0)===10)this.rawValue=raw.slice(1);","initialize")+"</field>";
}
std::string Items(const std::string& ref,const std::string& value_ref,const std::string& label_ref) {
  return "<bindItems ref=\""+ref+"\" valueRef=\""+value_ref+"\" labelRef=\""+label_ref+"\"/>";
}
const std::string kOptions="<root><options><row><code>r</code><label>Red</label></row>"
    "<row><code>g</code><label>Green</label></row><row><code>b</code><label>Blue</label></row></options></root>";

// parser/cxfa_node.cpp ProcessNullTestValidate and CJX_Field::execValidate.
TEST(CoverageXfaValidationBinding, NullTestsDistinguishDisabledWarningAndErrorAndRecoverAfterPublicExecution) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const std::string mode:{"disabled","warning","error"})for(bool custom:{false,true}) {
    SCOPED_TRACE(mode+(custom?"-custom":"-caption"));
    const auto message=custom?"<message><text name=\"nullTest\">Required explicit</text></message>":"";
    const auto target=Text("target","Seed",0,100,
        "<caption reserve=\"50pt\"><value><text>Account</text></value></caption>"+Validate("nullTest=\""+mode+"\"",message),"<bind match=\"none\"/>",160);
    const std::string script="var f=r.target;f.caption.value.text.value='Account';"+std::string(custom?"f.mandatoryMessage='Required explicit';":"")+"f.rawValue='Seed';f.rawValue=null;var result=f.execValidate();"
        "eq(typeof result,'boolean');"+std::string(mode=="disabled"?"eq(result,true);":"eq(result,false);")+
        "f.rawValue='Recovered';eq(f.execValidate(),true);eq(clean(f.rawValue),'Recovered');";
    RunValidationBinding(Fixture(target,script),[=](Session& s){
      if(custom&&mode!="disabled")Message(s,u"Required explicit\n");
      if(mode=="disabled"){EXPECT_TRUE(s.host->alerts.empty());}
      ASSERT_TRUE(s.Focus(8,110));EXPECT_EQ(s.Text(),u"Recovered");Blur(s);
    },JSPLATFORM_ALERT_RETURN_YES,true,custom&&mode!="disabled"?Inspect(EmptyAndRecover):Inspect{});
  }
}

TEST(CoverageXfaValidationBinding, WarningRepliesAllowRecoveryAndErrorMessagesRemainAttachedToTheField) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int reply:{JSPLATFORM_ALERT_RETURN_YES,JSPLATFORM_ALERT_RETURN_NO}) {
    SCOPED_TRACE(reply);
    const auto target=Text("target","Seed",0,100,Validate("nullTest=\"warning\"",
        "<message><text name=\"nullTest\">Missing account</text></message>")+forms::Event("xfa.host.messageBox('exit:'+String(this.rawValue===null?'':this.rawValue).replace(/^\\n/,''));","exit"));
    RunValidationBinding(Fixture(target,"var f=r.target;f.mandatoryMessage='Missing account';f.rawValue='Recovered';"
        "eq(f.execValidate(),true);eq(f.mandatoryMessage,'Missing account');"),
        [](Session& s){Message(s,u"Missing account");Message(s,u"exit:");Message(s,u"exit:Recovered");},reply,true,EmptyAndRecover);
  }
}

// ProcessFormatTestValidate uses the document locale and validate picture.
TEST(CoverageXfaValidationBinding, NumericValidationPicturesCheckBothValidAndInvalidCanonicalValues) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const std::string mode:{"error","warning"}) {
    const auto validation=Validate("nullTest=\"disabled\" formatTest=\""+mode+"\"",
        "<picture>num{99}</picture><message><text name=\"formatTest\">Two digits required</text></message>");
    const auto target=Text("target","12",0,100,validation);
    RunValidationBinding(Fixture(target,"var f=r.target;f.formatMessage='Two digits required';f.rawValue='12';eq(f.execValidate(),true);"
        "f.rawValue='bad';eq(f.execValidate(),false);f.rawValue='34';eq(f.execValidate(),true);"
        "eq(f.formatMessage,'Two digits required');"),[](Session& s){Message(s,u"Two digits required");});
  }
}

TEST(CoverageXfaValidationBinding, ValidationScriptsAndExplicitInitializeEventsRunAgainstTheirActualOwner) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const std::string language:{"application/x-javascript","application/x-formcalc"})for(bool truth:{false,true}) {
    SCOPED_TRACE(language+(truth?"-true":"-false"));
    const std::string expression=language=="application/x-javascript"?
        "xfa.form.root.audit.rawValue='Validated';"+std::string(truth?"true;":"false;"):
        "root.audit.rawValue = \"Validated\"\n"+std::string(truth?"1 eq 1":"1 eq 2");
    const auto target=Text("target","Seed",0,100,Validate("nullTest=\"disabled\" scriptTest=\"error\"",
        "<script contentType=\""+language+"\"><![CDATA["+expression+"]]></script>")+
        forms::Event("xfa.form.root.audit.rawValue='Initialized';","initialize"));
    RunValidationBinding(Fixture(target+Text("audit","Ready",120,100),
        "r.audit.rawValue='Ready';r.target.execValidate();eq(clean(r.audit.rawValue),'Validated');"
        "r.target.execInitialize();eq(clean(r.audit.rawValue),'Initialized');"
        "throws(function(){r.target.execValidate(1);});throws(function(){r.target.execInitialize(1);});"));
  }
}

// CJX_HostPseudoModel::ResetData splits comma-separated SOM expressions and
// falls back to the root when none resolve. FFDocView restores template defaults.
TEST(CoverageXfaValidationBinding, ScopedCommaSeparatedAndWholeDocumentResetRestoreDefaultsWithoutLeakingSiblingChanges) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto model=Text("target","First",0,100)+Text("other","Second",120,100)+Text("third","Third",0,140);
  for(const std::string filter:{"root.target","root.target, root.other","missing",""}) {
    SCOPED_TRACE(filter);
    std::string body="r.target.rawValue='Changed';r.other.rawValue='Changed';r.third.rawValue='Changed';";
    body+="xfa.host.resetData('"+filter+"');";
    body+="eq(clean(r.target.rawValue),'First');";
    body+="eq(clean(r.other.rawValue),'"+std::string(filter=="root.target"?"Changed":"Second")+"');";
    body+="eq(clean(r.third.rawValue),'"+std::string(filter=="root.target"||filter=="root.target, root.other"?"Changed":"Third")+"');";
    RunValidationBinding(Fixture(model,body));
  }
}

// CXFA_FFDocView::RunBindItems: content refs, equal refs, named child refs and
// absent value-child fallback. Empty exported content defaults to its label
// when the choice-list item is inserted (CXFA_Node::InsertItem).
TEST(CoverageXfaValidationBinding, BindItemsResolvesIndependentDisplayAndExportReferencesAndContentFallbacks) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Row{std::string label_ref,value_ref;std::vector<std::string> labels,values;};
  const std::vector<Row> rows={
    {"label","code",{"Red","Green","Blue"},{"r","g","b"}},
    {"code","code",{"r","g","b"},{"r","g","b"}},
    {"","code",{"r","g","b"},{"r","g","b"}},
    {"$","$",{"","",""},{"","",""}},
    {"label","missing",{"Red","Green","Blue"},{"Red","Green","Blue"}}};
  for(const auto& row:rows) {
    SCOPED_TRACE(row.label_ref+":"+row.value_ref);
    std::string script="var f=r.choice;eq(f.length,3);";
    for(size_t i=0;i<3;++i)script+="eq(clean(f.getDisplayItem("+std::to_string(i)+")),'"+row.labels[i]+"');eq(clean(f.getSaveItem("+std::to_string(i)+")),'"+row.values[i]+"');";
    script+="eq(f.getDisplayItem(-1),null);eq(f.getSaveItem(99),null);";
    script+="cleanItems(f);f.rawValue='"+row.values[1]+"';";
    if(row.values[1]=="g")script+="f.rawValue='r';f.rawValue='g';yes(f.getItemState(1),'selected exported value');eq(clean(f.rawValue),'g');";
    RunValidationBinding(Fixture(Choice(Items("$record.options.row[*]",row.value_ref,row.label_ref)),script,kOptions));
  }
}

TEST(CoverageXfaValidationBinding, MissingAndEmptyBindItemsClearTemplateItemsWithoutInventingOptions) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const std::string ref:{"$record.missing[*]","$record.options.row[*]"}) {
    SCOPED_TRACE(ref);
    const auto choice=Choice("<items><text>Obsolete</text></items><items save=\"1\"><text>old</text></items>"+
        Items(ref,"code","label"),"");
    RunValidationBinding(Fixture(choice,"eq(r.choice.length,0);eq(r.choice.getDisplayItem(0),null);eq(r.choice.getSaveItem(0),null);",
        "<root><options/></root>"));
  }
}

TEST(CoverageXfaValidationBinding, BoundChoiceSelectionUsesRealKeyboardAndPersistsItsExportedValueAcrossReaders) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto bytes=Fixture(Choice(Items("$record.options.row[*]","code","label")),
      "eq(r.choice.length,3);eq(clean(r.choice.getDisplayItem(0)),'Red');eq(clean(r.choice.getSaveItem(2)),'b');cleanItems(r.choice);",kOptions);
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Session s(writer->api(),bytes,true,true,2,[](forms::FormHost& h){Configure(h);});ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    Execute(s);ASSERT_FALSE(::testing::Test::HasFailure());const auto before=s.Draw(240,240);
    ASSERT_TRUE(s.Focus(8,110));s.Click(95,110);
    ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Down,0));
    ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Return,0));
    EXPECT_EQ(s.Text(),u"b");Blur(s);images[backend]=Paint(s,*writer,true,artifacts,writer->name()+"-choice-paint",true);
    // End must clamp at the final option already reached by Down.
    ASSERT_TRUE(s.Focus(8,110));s.Click(95,110);ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_End,0));
    ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Return,0));
    EXPECT_EQ(s.Text(),u"b");Blur(s);images[backend]=Paint(s,*writer,true,artifacts,writer->name()+"-choice-paint",true);EXPECT_NE(before.pixels,images[backend].pixels);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-bound-choice");ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Session loaded(reader->api(),saved,true,true,2,[](forms::FormHost& h){Configure(h);});ASSERT_TRUE(loaded.valid());ASSERT_TRUE(loaded.xfa_loaded);
      Execute(loaded);ASSERT_FALSE(::testing::Test::HasFailure());ASSERT_TRUE(loaded.Focus(8,110));EXPECT_EQ(loaded.Text(),u"b");Blur(loaded);
      Compare(images[backend],Paint(loaded,*reader,true,artifacts,writer->name()+"-"+reader->name()+"-choice-paint",true),artifacts,writer->name()+"-"+reader->name()+"-choice");
    }
    ++backend;
  }
  Compare(images[0],images[1],artifacts,"bound-choice-backends");
}

TEST(CoverageXfaValidationBinding, DatasetReplacementAndRemergeRegenerateChoiceLabelsAndValuesInDatasetOrder) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::string updated="<root><options><row><code>b</code><label>Blue</label></row>"
      "<row><code>a</code><label>Amber</label></row></options></root>";
  const std::string mutation="xfa.datasets.data.root.options.loadXML('<options><row><code>b</code><label>Blue</label></row>"
      "<row><code>a</code><label>Amber</label></row></options>',true,true);xfa.form.remerge();";
  const auto probe=[](bool native_remerge){return std::string(
      "var f=r.choice,rows=xfa.datasets.data.root.options.resolveNodes('row[*]');")+
      (native_remerge?"if(rows.length===2){eq(f.length,0);}else{":"{")+
      "eq(f.length,rows.length);for(var i=0;i<rows.length;i++){eq(clean(f.getSaveItem(i)),clean(rows.item(i).code.value));"
      "eq(clean(f.getDisplayItem(i)),clean(rows.item(i).label.value));}}"
      "yes(rows.length===2||rows.length===3,'known dataset cardinality');cleanItems(f);";};
  const auto choice=Choice(Items("xfa.datasets.data.root.options.row[*]","code","label"));
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  const auto libraries=Implementations();
  for(const auto* writer:{libraries[1],libraries[0]}) {
    const bool native=writer->name()=="native";SCOPED_TRACE(writer->name());
    Session s(writer->api(),Fixture(choice,probe(native),kOptions,mutation),true,true,2,[](forms::FormHost& h){Configure(h);});
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);Execute(s);ASSERT_FALSE(::testing::Test::HasFailure());
    const auto before=s.Draw(240,240);s.Click(128,60);s.api.FORM_ForceToKillFocus(s.form);
    for(size_t i=0;i<s.host->pages.size();++i){s.ClosePage(static_cast<int>(i));}s.page=s.host->Page(0);
    Execute(s);ASSERT_FALSE(::testing::Test::HasFailure());
    std::unique_ptr<Session> control;Session* active=&s;
    if(native) {
      // Pinned UpdateDocView never drains the BindItems queue populated by
      // remerge. Retain its independently asserted zero-item state and actual
      // output, then compare corrected Rust with an identical healthy scene
      // initialized directly from the same final two dataset rows.
      artifacts.Image("native-unprocessed-remerge",s.Draw(240,240));
      ASSERT_FALSE(SaveDocument(s.api,s.document,artifacts,"native-unprocessed-remerge").empty());
      control=std::make_unique<Session>(writer->api(),Fixture(choice,probe(false),updated,mutation),true,true,2,
          [](forms::FormHost& h){Configure(h);});
      ASSERT_TRUE(control->valid());ASSERT_TRUE(control->xfa_loaded);Execute(*control);ASSERT_FALSE(::testing::Test::HasFailure());active=control.get();
    }
    ASSERT_TRUE(active->Focus(8,110));active->Click(95,110);
    ASSERT_TRUE(active->api.FORM_OnKeyDown(active->form,active->page,FWL_VKEY_Down,0));
    ASSERT_TRUE(active->api.FORM_OnKeyDown(active->form,active->page,FWL_VKEY_Down,0));
    ASSERT_TRUE(active->api.FORM_OnKeyDown(active->form,active->page,FWL_VKEY_Return,0));
    EXPECT_EQ(active->Text(),u"a");Blur(*active);
    images[backend]=Paint(*active,*writer,true,artifacts,writer->name()+"-choice-paint");EXPECT_NE(before.pixels,images[backend].pixels);
    const auto saved=SaveDocument(active->api,active->document,artifacts,writer->name()+"-remerge-choice");ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Session loaded(reader->api(),saved,true,true,2,[](forms::FormHost& h){Configure(h);});
      ASSERT_TRUE(loaded.valid());ASSERT_TRUE(loaded.xfa_loaded);Execute(loaded);ASSERT_FALSE(::testing::Test::HasFailure());
      ASSERT_TRUE(loaded.Focus(8,110));EXPECT_EQ(loaded.Text(),u"a");Blur(loaded);
      Compare(images[backend],Paint(loaded,*reader,true,artifacts,writer->name()+"-"+reader->name()+"-choice-paint"),artifacts,
          writer->name()+"-"+reader->name()+"-remerge");
    }
    ++backend;
  }
  Compare(images[0],images[1],artifacts,"remerge-backends");
}

}  // namespace
}  // namespace pdfoxium::integration
