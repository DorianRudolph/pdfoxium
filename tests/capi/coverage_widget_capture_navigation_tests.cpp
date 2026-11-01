#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"
#include "text_fixture.h"

#include <array>
#include <functional>
#include <set>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using Bytes=std::vector<uint8_t>;
std::string Escape(const std::string& text) {
  std::string out;for(char c:text){if(c=='&')out+="&amp;";else if(c=='<')out+="&lt;";else if(c=='\n')out+="&#10;";else if(c=='\r')out+="&#13;";else out+=c;}return out;
}
std::string Quote(const std::string& text) {
  std::string out="'";for(char c:text){if(c=='\\'||c=='\'')out+='\\';if(c=='\n')out+="\\n";else if(c=='\r')out+="\\r";else out+=c;}return out+"'";
}
const std::string kClean="var v=this.rawValue;if(typeof v==='string'&&v.charCodeAt(0)===10)this.rawValue=v.slice(1);"
    "for(var g=0;g<this.nodes.length;g++){var items=this.nodes.item(g);if(items.className!=='items')continue;for(var i=0;i<items.nodes.length;i++){var item=items.nodes.item(i),s=String(item.value);if(s.charCodeAt(0)===10)item.value=s.slice(1);}}";
struct Spec {
  std::string input="ABCDEF",input_ui="<textEdit/>",input_attributes;
  std::string check_attributes,check_ui="<checkButton size=\"24pt\" shape=\"square\"/>",check_events;
  std::string push_attributes,push_events,action;
  int list_height=96,list_count=40,audit=0;
  std::string expected_input="ABCDEF",expected_check="Off";
};
std::string Field(const std::string& name,const std::string& ui,const std::string& value,
                  const std::string& xy,int width,int height,const std::string& attributes="",const std::string& extra="") {
  return "<field name=\""+name+"\" "+xy+" w=\""+std::to_string(width)+"pt\" h=\""+std::to_string(height)+"pt\" "+attributes+"><bind match=\"once\"/><font typeface=\"Ahem\" size=\"12pt\"/><ui>"+ui+"</ui><value><text>"+Escape(value)+"</text></value><para vAlign=\"top\" hAlign=\"left\" lineHeight=\"12pt\"/>"+extra+forms::Event(kClean,"initialize")+"</field>";
}
std::string Button(const std::string& name,int x,int y,const std::string& code) {
  return "<field name=\""+name+"\" x=\""+std::to_string(x)+"pt\" y=\""+std::to_string(y)+"pt\" w=\"100pt\" h=\"24pt\"><bind match=\"none\"/><font typeface=\"Ahem\" size=\"10pt\"/><ui><button/></ui><caption><value><text>GO</text></value></caption>"+forms::Event("this.caption.value.text.value='GO';","initialize")+forms::Event(code,"click")+"</field>";
}
Bytes Fixture(const Spec& spec) {
  static const auto font=text_fixture::Read(TestRoot()/"upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
  std::string items="<items>",saved="<items save=\"1\">";
  for(int i=0;i<spec.list_count;++i){items+="<text>"+std::string(i%7+1,char('A'+i%7))+"</text>";saved+="<text>v"+std::to_string(i)+"</text>";}
  const auto verify="try{var r=xfa.form.root;function clean(v){return String(v===null?'':v).replace(/^\\n/,'').replace(/\\r\\n/g,'\\n');}"
      "if(r.list.selectedIndex!==0)throw 'list:'+String(r.list.selectedIndex);if(clean(r.check.rawValue)!=="+Quote(spec.expected_check)+")throw 'check:'+String(r.check.rawValue);"
      "if(Number(r.audit.rawValue)!=="+std::to_string(spec.audit)+")throw 'audit:'+String(r.audit.rawValue);if(clean(r.input.rawValue)!=="+Quote(spec.expected_input)+")throw 'input:'+String(r.input.rawValue);r.report.rawValue='PASS';}catch(e){xfa.form.root.report.rawValue='FAIL';xfa.host.messageBox('FAIL:'+String(e));}";
  const std::string preamble="<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
  const std::string config="<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
  std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" locale=\"en_US\"><pageSet><pageArea><occur min=\"1\" max=\"-1\"/><medium short=\"400pt\" long=\"400pt\"/><contentArea w=\"400pt\" h=\"400pt\"/></pageArea></pageSet>"+
      Field("input",spec.input_ui,spec.input,"x=\"20pt\" y=\"20pt\"",100,40,spec.input_attributes)+
      Field("list","<choiceList open=\"always\"/>","v0","x=\"20.5pt\" y=\"100.5pt\"",180,spec.list_height,"",items+"</items>"+saved+"</items>")+
      Field("check",spec.check_ui,"Off","x=\"240pt\" y=\"80pt\"",32,32,spec.check_attributes,"<items><text>Yes</text><text>Off</text><text>Maybe</text></items>"+spec.check_events)+
      "<field name=\"push\" x=\"240pt\" y=\"140pt\" w=\"100pt\" h=\"30pt\" "+spec.push_attributes+"><bind match=\"none\"/><ui><button highlight=\"inverted\"/></ui><font typeface=\"Ahem\" size=\"12pt\"/><para vAlign=\"top\" hAlign=\"left\" lineHeight=\"12pt\"/><caption><value><text>GO</text></value></caption>"+
      forms::Event("this.caption.value.text.value='GO';","initialize")+forms::Event("var r=xfa.form.root;r.audit.rawValue=Number(r.audit.rawValue)+1;","click")+spec.push_events+"</field>"+
      Field("audit","<textEdit/>","0","x=\"20pt\" y=\"340pt\"",150,24)+Field("report","<textEdit/>","READY","x=\"210pt\" y=\"340pt\"",160,24)+
      Button("action",20,280,"var r=xfa.form.root;"+spec.action)+Button("verify",240,280,verify)+"</subform></template>";
  // Keep CDATA delimiters inside their native 1024-byte XML read plane. Their
  // separate boundary regression already covers the pinned native parser bug.
  for(size_t i=0;i<xml.size();){const size_t n=xml.compare(i,9,"<![CDATA[")==0?9:xml.compare(i,3,"]]>")==0?3:0;if(!n){++i;continue;}const size_t at=preamble.size()+config.size()+i;if(at/1024!=(at+n-1)/1024){const size_t pad=1024-at%1024;xml.insert(i,pad,' ');i+=pad;}i+=n;}
  std::string widths;for(int i=0;i<256;++i)widths+="1000 ";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << /Font << /Ahem 10 0 R >> >> >>",
      "<< /Fields [] /DR << /Font << /Ahem 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("",preamble),PdfStream("",config),PdfStream("",xml),PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root xfa:dataNode=\"dataGroup\"/></xfa:data></xfa:datasets>"),PdfStream("","</xdp:xdp>"),
      "<< /Type /Font /Subtype /TrueType /BaseFont /Ahem /FirstChar 0 /LastChar 255 /Widths ["+widths+"] /Encoding /WinAnsiEncoding /FontDescriptor 11 0 R >>",
      "<< /Type /FontDescriptor /FontName /Ahem /Flags 32 /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /CapHeight 800 /ItalicAngle 0 /StemV 80 /FontFile2 12 0 R >>",
      PdfStream("/Length1 "+std::to_string(font.size()),std::string(font.begin(),font.end()))});
}
bool Native(const Session& s){return &s.api==&Implementations()[0]->api();}
void Key(Session& s,int key,int flags=0){s.api.FORM_OnKeyDown(s.form,s.page,key,flags);}
void Char(Session& s,int c,int flags=0){s.api.FORM_OnChar(s.form,s.page,c,flags);}
void Move(Session& s,double x,double y,int flags=0){s.api.FORM_OnMouseMove(s.form,s.page,flags,x,y);}
void Down(Session& s,double x,double y){s.api.FORM_OnLButtonDown(s.form,s.page,0,x,y);}
void Up(Session& s,double x,double y){s.api.FORM_OnLButtonUp(s.form,s.page,0,x,y);}
void Frame(Session& s,std::vector<Raster>& frames){frames.push_back(s.Draw());}
void Verify(Session& s){s.Click(250,290);ASSERT_TRUE(s.Focus(220,350));for(const auto& a:s.host->alerts) {const auto& message=std::get<0>(a);EXPECT_NE(message.substr(0,5),u"FAIL:")<<std::string(message.begin(),message.end());}ASSERT_EQ(s.Text(),u"PASS");}
void Blur(Session& s){s.api.FORM_ForceToKillFocus(s.form);s.Click(390,390);s.api.FORM_ForceToKillFocus(s.form);Move(s,390,390);}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label){
  // Embedded Ahem has the existing Fontations/FreeType partial-coverage budget;
  // per-widget crops prevent a broken small control hiding in page averages.
  CompareRaster(a,b,artifacts,label,{40,0,0.5});
  for(const auto& r:std::array<std::array<int,4>,4>{{{20,20,102,42},{20,100,182,98},{239,79,35,35},{239,139,103,33}}})
    CompareRaster(CropRaster(a,r[0],r[1],r[2],r[3]),CropRaster(b,r[0],r[1],r[2],r[3]),artifacts,label+std::to_string(r[0])+"-"+std::to_string(r[1]),{40,0,0.5});
}
// Scroll offsets and caret/hover states are transient window state, not PDF
// data. Return through actual input to the common top/left view before saving.
void Settle(Session& s){
  s.Click(30,106);Key(s,FWL_VKEY_Home);
  s.Click(22,26);Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
}
void Exercise(const Spec& spec,const std::string& label,const std::function<void(Session&,std::vector<Raster>&)>& interact){
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t n=0;
  for(const auto* writer:Implementations()){
    SCOPED_TRACE(writer->name());Session s(writer->api(),Fixture(spec),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(interact(s,frames[n]));
    ASSERT_NO_FATAL_FAILURE(Verify(s));Settle(s);
    ASSERT_NO_FATAL_FAILURE(Verify(s));Blur(s);const auto final=s.Draw();frames[n].push_back(final);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+label);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()){
      SCOPED_TRACE(reader->name());Session loaded(reader->api(),saved,true,true);ASSERT_TRUE(loaded.valid());ASSERT_TRUE(loaded.xfa_loaded);
      ASSERT_NO_FATAL_FAILURE(Verify(loaded));Settle(loaded);
    ASSERT_NO_FATAL_FAILURE(Verify(loaded));Blur(loaded);Compare(final,loaded.Draw(),artifacts,writer->name()+reader->name()+label);
    }
    EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);++n;
  }
  ASSERT_EQ(frames[0].size(),frames[1].size());for(size_t i=0;i<frames[0].size();++i)Compare(frames[0][i],frames[1][i],artifacts,label+std::to_string(i));
}
void Wheel(Session& s,int delta,double x=50,double y=130){const FS_POINTF p{float(x),float(y)};s.api.FORM_OnMouseWheel(s.form,s.page,0,&p,0,delta);}
void ListFocus(Session& s){ASSERT_TRUE(s.Focus(30,106));}
std::set<int> TimerIds(Session& s){std::set<int> ids;for(const auto& entry:s.host->timers)ids.insert(entry.first);return ids;}
void TickNew(Session& s,const std::set<int>& previous,int repeats){
  for(int i=0;i<repeats;++i){std::vector<int> ids;for(const auto& entry:s.host->timers)if(!previous.count(entry.first))ids.push_back(entry.first);ASSERT_FALSE(ids.empty());for(int id:ids)s.host->Tick(id);}
}

TEST(CoverageWidgetCaptureNavigation, CheckboxSpaceKeyActivatesTheActualFwlControl){Spec spec;spec.expected_check="Yes";Exercise(spec,"check-space",[](Session& s,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(250,90));Key(s,FWL_VKEY_Space);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, CheckboxReturnKeyActivatesTheActualFwlControl){Spec spec;spec.expected_check="Yes";Exercise(spec,"check-return",[](Session& s,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(250,90));Key(s,FWL_VKEY_Return);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, CheckboxNavigationAndCharMessagesDoNotPretendToBeActivationKeys){Exercise({},"check-nonactivation",[](Session& s,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(250,90));for(int key:{FWL_VKEY_Left,FWL_VKEY_Right,FWL_VKEY_Home,FWL_VKEY_End})Key(s,key);Char(s,'x');Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, CheckboxCaptureOutsideReleaseLeavesTheOriginalValue){Exercise({},"check-release-out",[](Session& s,std::vector<Raster>& f){Down(s,250,90);Move(s,310,120,FWL_EVENTFLAG_LeftButtonDown);Frame(s,f);Up(s,310,120);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, CheckboxCaptureReturningInsideRestoresPressedStateAndActivatesOnce){Spec spec;spec.expected_check="Yes";Exercise(spec,"check-reenter",[](Session& s,std::vector<Raster>& f){Down(s,250,90);Move(s,310,120,FWL_EVENTFLAG_LeftButtonDown);Move(s,250,90,FWL_EVENTFLAG_LeftButtonDown);Frame(s,f);Up(s,250,90);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, CheckboxUnpairedReleaseDoesNotToggleItsModel){Exercise({},"check-unpaired",[](Session& s,std::vector<Raster>& f){Up(s,250,90);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, ThreeStateCheckboxKeyboardCyclesThroughNeutralAndBackToOff){Spec spec;spec.check_ui="<checkButton size=\"24pt\" allowNeutral=\"1\"/>";Exercise(spec,"check-three-state",[](Session& s,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(250,90));for(int i=0;i<3;++i){Key(s,FWL_VKEY_Space);Frame(s,f);}});}
TEST(CoverageWidgetCaptureNavigation, ReadOnlyCheckboxFiltersMouseAndKeyboardMutation){Spec spec;spec.check_attributes="access=\"readOnly\"";Exercise(spec,"check-readonly",[](Session& s,std::vector<Raster>& f){s.Click(250,90);Key(s,FWL_VKEY_Space);Key(s,FWL_VKEY_Return);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, PushButtonReturnDispatchesOneClickThroughTheFwlMouseEventAdapter){Spec spec;spec.audit=1;Exercise(spec,"push-return",[](Session& s,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(250,150));Key(s,FWL_VKEY_Return);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, PushButtonSpaceDoesNotSynthesizeAReturnClick){Exercise({},"push-space",[](Session& s,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(250,150));Key(s,FWL_VKEY_Space);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, PushButtonCaptureRestoresItsPressedAppearanceWhenThePointerReturns){Spec spec;spec.audit=1;Exercise(spec,"push-reenter",[](Session& s,std::vector<Raster>& f){Down(s,250,150);Move(s,370,190,FWL_EVENTFLAG_LeftButtonDown);Frame(s,f);Move(s,250,150,FWL_EVENTFLAG_LeftButtonDown);Frame(s,f);Up(s,250,150);});}
TEST(CoverageWidgetCaptureNavigation, PushButtonReleaseOutsideCancelsClickAndClearsPressedPainting){Exercise({},"push-outside",[](Session& s,std::vector<Raster>& f){Down(s,250,150);Move(s,370,190,FWL_EVENTFLAG_LeftButtonDown);Up(s,370,190);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, PushButtonMouseLeaveClearsHoverWithoutCommittingAValue){Exercise({},"push-leave",[](Session& s,std::vector<Raster>& f){Move(s,250,150);Frame(s,f);Move(s,390,200);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, PushButtonReadOnlyAccessRejectsBothActivationChannels){Spec spec;spec.push_attributes="access=\"readOnly\"";Exercise(spec,"push-readonly",[](Session& s,std::vector<Raster>& f){s.Click(250,150);Key(s,FWL_VKEY_Return);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, AccessChangesDuringMouseDownPreserveTheAlreadyAcceptedButtonGesture){Spec spec;spec.audit=1;spec.push_events=forms::Event("this.access='readOnly';","mouseDown");spec.action="r.push.access='open';";Exercise(spec,"push-disable-capture",[](Session& s,std::vector<Raster>& f){Down(s,250,150);Move(s,252,152,FWL_EVENTFLAG_LeftButtonDown);Up(s,252,152);Frame(s,f);s.Click(30,290);});}
TEST(CoverageWidgetCaptureNavigation, ListWheelMovesTheViewportWithoutChangingItsSelectedValue){Exercise({},"list-wheel",[](Session& s,std::vector<Raster>& f){ListFocus(s);Frame(s,f);Wheel(s,-120);Frame(s,f);EXPECT_NE(CropRaster(f[0],21,101,168,78).pixels,CropRaster(f[1],21,101,168,78).pixels);Wheel(s,120);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, ListWheelClampsAtTheBottomAndReturnsToItsOriginalTopViewport){Exercise({},"list-wheel-clamp",[](Session& s,std::vector<Raster>& f){ListFocus(s);const auto top=s.Draw();for(int i=0;i<80;++i)Wheel(s,-120);const auto bottom=s.Draw();Wheel(s,-120);EXPECT_EQ(s.Draw().pixels,bottom.pixels);Frame(s,f);for(int i=0;i<80;++i)Wheel(s,120);EXPECT_EQ(CropRaster(top,21,101,168,78).pixels,CropRaster(s.Draw(),21,101,168,78).pixels);Frame(s,f);});}
TEST(CoverageWidgetCaptureNavigation, ListThumbCaptureTracksFractionalMotionAndClampsBothEnds){Exercise({},"list-thumb",[](Session& s,std::vector<Raster>& f){ListFocus(s);Down(s,196,114);Move(s,196,166.5,FWL_EVENTFLAG_LeftButtonDown);Frame(s,f);Move(s,196,195,FWL_EVENTFLAG_LeftButtonDown);Frame(s,f);Move(s,196,101,FWL_EVENTFLAG_LeftButtonDown);Frame(s,f);Up(s,196,101);});}
TEST(CoverageWidgetCaptureNavigation, ListScrollbarArrowTimerRepeatsAndReleaseCancelsItsOwner){Exercise({},"list-arrow-repeat",[](Session& s,std::vector<Raster>& f){ListFocus(s);const auto previous=TimerIds(s);Down(s,196,191);TickNew(s,previous,3);Frame(s,f);Up(s,196,191);const auto stopped=s.Draw();for(const auto& entry:s.host->timers)EXPECT_TRUE(previous.count(entry.first));for(int id:previous)s.host->Tick(id);EXPECT_EQ(s.Draw().pixels,stopped.pixels);});}
TEST(CoverageWidgetCaptureNavigation, ListLowerTrackPagesAndRepeatsWithoutSelectingTheRowUnderIt){Exercise({},"list-page-forward",[](Session& s,std::vector<Raster>& f){ListFocus(s);const auto previous=TimerIds(s);Down(s,196,155);TickNew(s,previous,1);Frame(s,f);Up(s,196,155);});}
TEST(CoverageWidgetCaptureNavigation, ListUpperTrackPagesBackFromAScrolledViewport){Exercise({},"list-page-backward",[](Session& s,std::vector<Raster>& f){ListFocus(s);for(int i=0;i<12;++i)Wheel(s,-120);Frame(s,f);Down(s,196,113);Frame(s,f);Up(s,196,113);});}
TEST(CoverageWidgetCaptureNavigation, ShortListScrollbarCompressesItsButtonsWithoutChangingTheValue){Spec spec;spec.list_height=18;Exercise(spec,"list-short-scrollbar",[](Session& s,std::vector<Raster>& f){ListFocus(s);s.Click(196,104);s.Click(196,114);Frame(s,f);});}
// FFTextEdit constructs right-down with a null FWL destination. WidgetMgr
// discards that message; right-up opens the host menu without moving the edit
// caret. Clicking beyond the text must therefore preserve our explicit Home.
TEST(CoverageWidgetCaptureNavigation, RightMouseMenuPreservesTheExistingLogicalInsertionPoint){Spec spec;spec.expected_input="XABCDEF";Exercise(spec,"edit-right-caret",[](Session& s,std::vector<Raster>& f){s.Click(22,26);ASSERT_TRUE(s.Focus(30,30));Key(s,FWL_VKEY_Home);s.api.FORM_OnRButtonDown(s.form,s.page,0,110,26);s.api.FORM_OnRButtonUp(s.form,s.page,0,110,26);Char(s,'X');EXPECT_EQ(s.Text(),u"XABCDEF");Frame(s,f);});}
// Native CFWL_Edit::OnChar inserts at cursor_position_ and Delete removes one
// character, even when the edit engine has an existing selection. Rust applies
// the selected range. Preserve both original observations and undo/redo, then
// use the supported public selection replacement for the common saved scene.
TEST(CoverageWidgetCaptureNavigation, ActualSelectedCharacterInputUsesTheEditReplacementRecordPath){
  Spec spec;spec.expected_input="Z";
  Exercise(spec,"edit-char-selected",[](Session& s,std::vector<Raster>& f){
    Artifacts original;ASSERT_TRUE(s.Focus(30,30));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
    Char(s,'Z');original.Image(Native(s)?"native-original":"rust-original",s.Draw());
    EXPECT_EQ(s.Text(),Native(s)?u"ZABCDEF":u"Z");
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),u"ABCDEF");
    ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),Native(s)?u"ZABCDEF":u"Z");
    ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"Z");EXPECT_EQ(s.Text(),u"Z");Frame(s,f);
  });
}
TEST(CoverageWidgetCaptureNavigation, DeleteKeyRemovesTheWholeSelectedEditRangeAndRetainsUndo){
  Spec spec;spec.expected_input="";
  Exercise(spec,"edit-delete-selected",[](Session& s,std::vector<Raster>& f){
    Artifacts original;ASSERT_TRUE(s.Focus(30,30));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
    Key(s,FWL_VKEY_Delete);original.Image(Native(s)?"native-original":"rust-original",s.Draw());
    EXPECT_EQ(s.Text(),Native(s)?u"BCDEF":u"");
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),u"ABCDEF");
    ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),Native(s)?u"BCDEF":u"");
    if(!s.Text().empty()){ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"");}EXPECT_EQ(s.Text(),u"");Blur(s);Frame(s,f);
  });
}
TEST(CoverageWidgetCaptureNavigation, DisabledHorizontalAutoScrollClipsTheCaretAndKeepsTheLogicalEndSelection){
  Spec spec;spec.input=std::string(24,'W');spec.expected_input="WWW";spec.input_ui="<textEdit hScrollPolicy=\"off\"/>";
  Exercise(spec,"edit-no-hscroll",[](Session& s,std::vector<Raster>& f){
    Artifacts original;s.Click(22,26);ASSERT_TRUE(s.Focus(30,30));
    Key(s,FWL_VKEY_End);Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ShiftKey);
    EXPECT_EQ(s.Text(),std::u16string(24,u'W'));EXPECT_EQ(s.Text(true),std::u16string(24,u'W'));
    const auto image=s.Draw();original.Image(Native(s)?"native-original":"rust-original",image);
    size_t lower_ink=0;for(int y=34;y<58;++y)for(int x=21;x<119;++x){const auto at=(size_t(y)*image.width+x)*4;lower_ink+=std::min({image.pixels[at],image.pixels[at+1],image.pixels[at+2]})<150;}
    // Native's unchanged false/no-wrap setters leave the breaker's initial
    // 100-point wrap active. Rust preserves a real single line. This boundary
    // is also covered by the existing FormCalc/picture input scenarios.
    if(Native(s))EXPECT_GT(lower_ink,500u);else EXPECT_EQ(lower_ink,0u);
    ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"WWW");EXPECT_EQ(s.Text(),u"WWW");
    // A fitting real replacement supplies the shared native/Rust raster and
    // writer/reader control without requiring native's wrapping bug in Rust.
    Blur(s);Frame(s,f);
  });
}

}  // namespace
}  // namespace pdfoxium::integration
