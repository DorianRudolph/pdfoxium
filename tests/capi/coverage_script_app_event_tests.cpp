#include "pinned_test_fonts.h"
#include "form_fixture.h"

#include <array>
#include <cstring>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using HostCheck=std::function<void(forms::FormHost&)>;
using Configure=std::function<void(forms::FormHost&)>;

std::string CheckedApp(const std::string& body) {
  return "function eq(a,b){if(JSON.stringify(a)!==JSON.stringify(b))throw 'expected '+JSON.stringify(b)+' got '+JSON.stringify(a);}"
      "function yes(v){if(!v)throw 'assertion';}"
      "function throws(f){var caught=false;try{f();}catch(e){caught=true;}if(!caught)throw 'expected exception';}"
      "try{var nativeEngine=this.getField('input').value==='native';"+body+";this.getField('input').value='Ready';this.getField('second').value='PASS';}"
      "catch(e){this.getField('second').value='FAIL: '+String(e);}";
}
void CompareApp(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,RasterTolerance{18,0,.25});
}
void ExecuteApp(Session& s,const Library& owner,const HostCheck& inspect) {
  ASSERT_TRUE(s.Focus(30,355));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
  s.Replace(owner.name()=="native"?u"native":u"rust");s.api.FORM_ForceToKillFocus(s.form);
  s.host->file_queries=0;s.host->browse_queries=0;
  s.api.FORM_DoDocumentJSAction(s.form);
  EXPECT_EQ(s.Field(1),u"PASS");EXPECT_EQ(s.Field(0),u"Ready");
  if(inspect)inspect(*s.host);
  EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
}
void AppScenario(const std::string& body,const HostCheck& inspect={},const Configure& configure={},const HostCheck& native_inspect={}) {
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  forms::Actions actions;actions.document=CheckedApp(body);
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Session s(writer->api(),forms::Fixture(actions),true,false,2,configure);
    ASSERT_TRUE(s.valid());ASSERT_NO_FATAL_FAILURE(ExecuteApp(s,*writer,writer->name()=="native"&&native_inspect?native_inspect:inspect));
    images[backend]=s.Draw();forms::ExpectInk(images[backend],20,30,180,60);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-app");ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Session reload(reader->api(),saved,true,false,2,configure);
      ASSERT_TRUE(reload.valid());ASSERT_NO_FATAL_FAILURE(ExecuteApp(reload,*reader,reader->name()=="native"&&native_inspect?native_inspect:inspect));
      CompareApp(images[backend],reload.Draw(),artifacts,writer->name()+reader->name()+"-app");
    }
    ++backend;
  }
  CompareApp(images[0],images[1],artifacts,"app-engines");
}

TEST(CoverageScriptAppEvents, ApplicationIdentityAndHighlightStateUseTheDocumentRuntime) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  AppScenario(R"JS(
    eq(app.activeDocs.length,1);yes(app.activeDocs[0]===this);
    eq(app.activeDocs[0].getField('input').name,'input');
    eq(app.formsVersion,7);eq(app.viewerType,'pdfium');eq(app.viewerVariation,'Full');
    eq(app.viewerVersion,8);eq(app.platform,'Linux');eq(app.language,'en-US');
    var original=app.runtimeHighlight;
    [false,true,0,1,'', 'enabled',null,{},[]].forEach(function(v){app.runtimeHighlight=v;eq(app.runtimeHighlight,!!v);});
    app.runtimeHighlight={valueOf:function(){throw 'must not coerce';}};eq(app.runtimeHighlight,true);
    app.runtimeHighlight=original;eq(app.runtimeHighlight,original);
    throws(function(){app.activeDocs=[];});
  )JS");
}

TEST(CoverageScriptAppEvents, TwoLiveDocumentsKeepApplicationFlagsIndependent) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());forms::Actions a,b;
    a.document=CheckedApp("app.runtimeHighlight=true;app.calculate=false;eq(app.runtimeHighlight,true);eq(app.calculate,false);");
    b.document=CheckedApp("eq(app.runtimeHighlight,false);eq(app.calculate,true);");
    Session first(library->api(),forms::Fixture(a),true),second(library->api(),forms::Fixture(b),true);
    ASSERT_TRUE(first.valid());ASSERT_TRUE(second.valid());
    ExecuteApp(first,*library,{});ExecuteApp(second,*library,{});ExecuteApp(first,*library,{});ExecuteApp(second,*library,{});
    images[backend]=first.Draw();CompareApp(images[backend],second.Draw(),artifacts,"isolated-documents");
    const auto saved=SaveDocument(first.api,first.document,artifacts,library->name()+"-app-isolation");ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      Session reload(reader->api(),saved,true);ASSERT_TRUE(reload.valid());ExecuteApp(reload,*reader,{});
      CompareApp(images[backend],reload.Draw(),artifacts,library->name()+reader->name()+"-isolation");
    }
    ++backend;
  }
  CompareApp(images[0],images[1],artifacts,"isolation-engines");
}

TEST(CoverageScriptAppEvents, PlatformAndLanguageCallbacksUseBoundedUtf16AndFallbackOnFailure) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // These callbacks provide text only; the host does not consult the OS.
  for(int mode=0;mode<4;++mode) {
    SCOPED_TRACE(mode);
    const auto configure=[=](forms::FormHost& h){
      h.response=mode==0?u"TST \u03a9":mode==1?u"":mode==2?u"zero":u"oversized";
      h.FFI_GetPlatform=[](FPDF_FORMFILLINFO* p,void* output,int capacity)->int {
        auto& host=forms::FormHost::Self(p);++host.file_queries;
        if(host.response==u"zero")return 0;
        if(!output){EXPECT_EQ(capacity,0);return static_cast<int>(host.response.size()*2);}
        EXPECT_EQ(capacity,static_cast<int>(host.response.size()*2));
        if(host.response==u"oversized")return capacity+2;
        std::memcpy(output,host.response.data(),static_cast<size_t>(capacity));return capacity;
      };
      h.FFI_GetLanguage=[](FPDF_FORMFILLINFO* p,void* output,int capacity)->int {
        auto& host=forms::FormHost::Self(p);++host.browse_queries;
        const std::u16string value=u"FRA";const int bytes=static_cast<int>(value.size()*2);
        if(!output){EXPECT_EQ(capacity,0);return bytes;}
        EXPECT_EQ(capacity,bytes);std::memcpy(output,value.data(),value.size()*2);return bytes;
      };
    };
    AppScenario("eq(app.platform,"+std::string(mode==0?"'TST \\u03a9'":"'WIN'")+");eq(app.language,'FRA');",
      [=](forms::FormHost& h){EXPECT_EQ(h.file_queries,mode==0||mode==3?2:1);EXPECT_EQ(h.browse_queries,2);},configure);
  }
}

TEST(CoverageScriptAppEvents, AlertArraysRetainSparseNestedAndReentrantStringConversion) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Row {const char* source;std::u16string expected;};
  const std::vector<Row> rows={
    {"[]",u"[]"},{"['A',42,true,null]",u"[A, 42, true, null]"},
    {"[,,'last']",u"[undefined, undefined, last]"},
    {"[['A','B'],['C']]",u"[A,B, C]"},{"['caf\\u00e9','\\u03a9']",u"[caf\u00e9, \u03a9]"},
    {"[{toString:function(){return 'converted';}}]",u"[converted]"}};
  for(const auto& row:rows)for(bool keyword:{false,true}) {
    SCOPED_TRACE(row.source);
    SCOPED_TRACE(keyword);
    const auto call=keyword?"app.alert({cMsg:"+std::string(row.source)+",nIcon:2,nType:3,cTitle:'Array'})":
        "app.alert("+std::string(row.source)+",2,3,'Array')";
    AppScenario("eq("+call+",4);",[=](forms::FormHost& h){
      ASSERT_EQ(h.alerts.size(),1u);EXPECT_EQ(h.alerts[0],std::make_tuple(row.expected,std::u16string(u"Array"),3,2));
    },[](forms::FormHost& h){h.alert_result=4;});
  }
  AppScenario(R"JS(
    var a=[{toString:function(){a.push('added');return 'first';}}];eq(app.alert(a),4);
    if(nativeEngine)eq(app.alert([{toString:function(){throw 'conversion';}}]),4);
    else throws(function(){app.alert([{toString:function(){throw 'conversion';}}]);});
    eq(app.alert('recovered',0,0,'After'),4);
  )JS",[](forms::FormHost& h){
    ASSERT_EQ(h.alerts.size(),2u);EXPECT_EQ(std::get<0>(h.alerts[0]),u"[first, added]");
    EXPECT_EQ(std::get<1>(h.alerts[0]),u"Alert");EXPECT_EQ(std::get<0>(h.alerts[1]),u"recovered");
  },[](forms::FormHost& h){h.alert_result=4;},[](forms::FormHost& h){
    ASSERT_EQ(h.alerts.size(),3u);EXPECT_EQ(std::get<0>(h.alerts[0]),u"[first, added]");
    EXPECT_EQ(std::get<0>(h.alerts[1]),u"[]");EXPECT_EQ(std::get<0>(h.alerts[2]),u"recovered");
  });
}

TEST(CoverageScriptAppEvents, ResponseDefaultsAndKeywordFormsPreserveTheMockHostPayload) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(bool keyword:{false,true}) {
    SCOPED_TRACE(keyword);
    AppScenario(keyword?"eq(app.response({cQuestion:'Question'}),'reply');":"eq(app.response('Question'),'reply');",
      [](forms::FormHost& h){ASSERT_EQ(h.responses.size(),1u);EXPECT_EQ(h.responses[0],(std::array<std::u16string,4>{u"Question",u"PDF",u"",u""}));
        EXPECT_EQ(h.response_capacities,(std::vector<int>{2048}));EXPECT_EQ(h.response_passwords,(std::vector<int>{0}));});
  }
  AppScenario("eq(app.response({cQuestion:'\\u03a9',cTitle:'Title',cDefault:'start',bPassword:true,cLabel:'Label'}),'caf\\u00e9');",
    [](forms::FormHost& h){ASSERT_EQ(h.responses.size(),1u);EXPECT_EQ(h.responses[0],(std::array<std::u16string,4>{u"\u03a9",u"Title",u"start",u"Label"}));
      EXPECT_EQ(h.response_passwords,(std::vector<int>{1}));},[](forms::FormHost& h){h.response=u"caf\u00e9";});
}

TEST(CoverageScriptAppEvents, MailMessageParametersOnlyReachTheRecordingHost) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Row {const char* arguments;bool ui;std::array<std::u16string,5> payload;};
  // Doc_mail order is to, subject, cc, bcc, body, distinct from mailMsg args.
  const std::vector<Row> rows={
    {"true",true,{u"",u"",u"",u"",u""}},
    {"false,'to@example.invalid'",false,{u"to@example.invalid",u"",u"",u"",u""}},
    {"false,'to','cc','bcc','subject','body'",false,{u"to",u"subject",u"cc",u"bcc",u"body"}},
    {"{bUI:true,cSubject:'Subject',cMsg:'Body'}",true,{u"",u"Subject",u"",u"",u"Body"}},
    {"{bUI:false,cTo:'to',cCc:'cc',cBcc:'bcc',cSubject:'\\u03a9',cMsg:'caf\\u00e9'}",false,{u"to",u"\u03a9",u"cc",u"bcc",u"caf\u00e9"}},
    {"{bUI:1,cTo:17,cSubject:{toString:function(){return 'object';}},cMsg:true}",true,{u"17",u"object",u"",u"",u"true"}}};
  for(const auto& row:rows) {
    SCOPED_TRACE(row.arguments);
    AppScenario("app.mailMsg("+std::string(row.arguments)+");",[=](forms::FormHost& h){
      ASSERT_EQ(h.mails.size(),1u);EXPECT_EQ(h.mails[0],row.payload);EXPECT_EQ(h.mail_ui,(std::vector<int>{row.ui?1:0}));
      ASSERT_EQ(h.mail_packets.size(),1u);EXPECT_TRUE(h.mail_packets[0].empty());EXPECT_TRUE(h.submissions.empty());EXPECT_TRUE(h.uris.empty());
    });
  }
  AppScenario(R"JS(
    throws(function(){app.mailMsg();});throws(function(){app.mailMsg(false);});
    throws(function(){app.mailMsg({bUI:false});});
    if(nativeEngine)app.mailMsg(true,{toString:function(){throw 'recipient';}});
    else throws(function(){app.mailMsg(true,{toString:function(){throw 'recipient';}});});
    app.mailMsg(true,'recovered');
  )JS",[](forms::FormHost& h){ASSERT_EQ(h.mails.size(),1u);EXPECT_EQ(h.mails[0][0],u"recovered");EXPECT_TRUE(h.mail_packets[0].empty());},
  {},[](forms::FormHost& h){ASSERT_EQ(h.mails.size(),2u);EXPECT_EQ(h.mails[0][0],u"");EXPECT_EQ(h.mails[1][0],u"recovered");
    ASSERT_EQ(h.mail_packets.size(),2u);EXPECT_TRUE(h.mail_packets[0].empty());EXPECT_TRUE(h.mail_packets[1].empty());});
}

// Real Fo/Bl actions are absent from the shared AcroForm fixture. This minimal
// field tree supplies those constructors and records events without alerting or
// changing a field while an event is active. The public push action flushes logs.
std::vector<uint8_t> EventFixture(const std::string& key_extra="",int max_length=8) {
  const std::string snapshot=R"JS(
    var log=[],failures=[];
    function snapshot(){
      try{
        var key=event.name==='Keystroke';
        var selection=[event.selStart,event.selEnd];
        if(!key){var poison={valueOf:function(){throw 'selection coerced outside key event';}};event.selStart=poison;event.selEnd=poison;}
        var full=key?event.fieldFull:null;
        var value=event.name.indexOf('Mouse')===0?null:event.value;
        log.push([event.name,event.type,event.targetName,event.target.name,event.change,event.changeEx,
          event.keyDown,event.modifier,event.shift,event.willCommit,selection[0],selection[1],event.commitKey,full,value]);
      }catch(e){failures.push(String(e));}
    }
    this.getField('input').value='ABCD';log=[];
  )JS";
  const std::string guarded="if(typeof snapshot==='function')snapshot();";
  const auto js=[&](const std::string& extra=""){return forms::Js(guarded+extra);};
  const std::string widget="/Type /Annot /Subtype /Widget /F 4 /BS << /W 1 /S /S >> /MK << /BG [1 1 1] /BC [0 0 0] >> ";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /Names << /JavaScript << /Names [(events) "+forms::Js(snapshot)+"] >> >> >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << /Font << /Helv 5 0 R >> >> /Annots [6 0 R 7 0 R 8 0 R 9 0 R] /Contents 10 0 R >>",
    "<< /Fields [6 0 R 7 0 R 8 0 R 9 0 R] /CO [8 0 R] /NeedAppearances true /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    "<< "+widget+" /P 3 0 R /FT /Tx /T (input) /Rect [20 340 180 370] /V (ABCD) /MaxLen "+std::to_string(max_length)+
      " /AA << /Fo "+js()+" /Bl "+js()+" /E "+js()+" /X "+js()+" /D "+js()+" /U "+js()+" /K "+js(key_extra)+" /V "+js()+" /F "+js()+" >> >>",
    "<< "+widget+" /P 3 0 R /FT /Tx /T (second) /Rect [220 340 380 370] /V () >>",
    "<< "+widget+" /P 3 0 R /FT /Tx /T (calc) /Ff 1 /Rect [20 260 180 290] /V (0) /AA << /C "+forms::Js(
      "if(typeof snapshot==='function'){try{snapshot();log.push(['Source',event.source.name,event.target.name]);event.value='7';}catch(e){failures.push(String(e));}}")+" >> >>",
    "<< "+widget+" /P 3 0 R /FT /Btn /T (flush) /Ff 65536 /Rect [220 180 380 210] /MK << /CA (Flush) >> /A "+forms::Js(
      "this.getField('second').value=JSON.stringify(log)+'|errors='+JSON.stringify(failures);")+" >>",
    PdfStream("","0.94 g 0 0 400 400 re f")});
}
using EventOperation=std::function<void(Session&)>;
using EventCheck=std::function<void(const std::u16string&)>;
std::u16string WideApp(const std::string& text){return {text.begin(),text.end()};}
void ContainsEvent(const std::u16string& log,const std::string& expected) {
  EXPECT_NE(log.find(WideApp(expected)),std::u16string::npos)<<expected;
}
void EventScenario(const std::vector<uint8_t>& bytes,const EventOperation& operation,const EventCheck& inspect,
                   const std::u16string& expected) {
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  const auto exercise=[&](Session& s){
    s.api.FORM_DoDocumentJSAction(s.form);operation(s);s.api.FORM_ForceToKillFocus(s.form);
    s.Click(250,195);s.api.FORM_ForceToKillFocus(s.form);
    const auto report=s.Field(1);inspect(report);ContainsEvent(report,"|errors=[]");
    EXPECT_EQ(s.Field(0),expected);
    // Preserve the complete semantic report, then use real public input to
    // normalize only that long diagnostic field before strict painting.
    std::string ascii;for(char16_t ch:report)ascii.push_back(ch<128?static_cast<char>(ch):'?');
    artifacts.Text("events-"+std::to_string(backend),ascii);
    ASSERT_TRUE(s.Focus(250,355));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"PASS");
    s.api.FORM_ForceToKillFocus(s.form);EXPECT_EQ(s.Field(1),u"PASS");
  };
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Session s(writer->api(),bytes,true);ASSERT_TRUE(s.valid());
    ASSERT_NO_FATAL_FAILURE(exercise(s));images[backend]=s.Draw();forms::ExpectInk(images[backend],20,30,180,60);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-events");ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Session reload(reader->api(),saved,true);ASSERT_TRUE(reload.valid());
      ASSERT_NO_FATAL_FAILURE(exercise(reload));CompareApp(images[backend],reload.Draw(),artifacts,writer->name()+reader->name()+"-events");
    }
    ++backend;
  }
  CompareApp(images[0],images[1],artifacts,"event-engines");
}

TEST(CoverageScriptAppEvents, FocusMouseAndBlurExposeTheirActualFieldEventChannels) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  EventScenario(EventFixture(),[](Session& s){
    s.Click(30,355);s.Click(200,300);
  },[](const std::u16string& log){
    for(const auto* name:{"Focus","Blur","Mouse Enter","Mouse Exit","Mouse Down","Mouse Up"})
      ContainsEvent(log,"[\""+std::string(name)+"\",\"Field\",\"input\",\"input\"");
  },u"ABCD");
}

TEST(CoverageScriptAppEvents, KeystrokeSelectionsModifiersAndFullStateDescribeActualPublicInput) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(bool selected:{false,true})for(int modifiers:std::array<int,2>{0,FWL_EVENTFLAG_ShiftKey}) {
    SCOPED_TRACE(selected);
    SCOPED_TRACE(modifiers);
    EventScenario(EventFixture(),[=](Session& s){
      ASSERT_TRUE(s.Focus(30,355));
      ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Home,0));
      if(selected) {
        for(int i=0;i<2;++i) {
          ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey));
        }
      }
      ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,'Z',modifiers));
      EXPECT_EQ(s.Text(),selected?u"ZCD":u"ZABCD");
    },[=](const std::u16string& log){
      ContainsEvent(log,"[\"Keystroke\",\"Field\",\"input\",\"input\",\"Z\",\"\",true,"+
        std::string(modifiers&FWL_EVENTFLAG_ControlKey?"true":"false")+","+
        (modifiers&FWL_EVENTFLAG_ShiftKey?"true":"false")+",false,0,"+(selected?"2":"0")+",0,false,\"ABCD\"]");
      ContainsEvent(log,"[\"Validate\",\"Field\",\"input\",\"input\"");
      ContainsEvent(log,"[\"Format\",\"Field\",\"input\",\"input\"");
    },selected?u"ZCD":u"ZABCD");
  }
}

TEST(CoverageScriptAppEvents, DeleteKeyCarriesControlAndShiftModifiersThroughTheRealKeystroke) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // Ctrl-character input is a platform shortcut and does not dispatch the
  // ordinary typed-character event. Delete has a source-defined keydown path
  // that delivers modifiers to the event before editing in both engines.
  for(int modifiers:std::array<int,4>{0,FWL_EVENTFLAG_ShiftKey,FWL_EVENTFLAG_ControlKey,FWL_EVENTFLAG_ShiftKey|FWL_EVENTFLAG_ControlKey}) {
    SCOPED_TRACE(modifiers);
    EventScenario(EventFixture(),[=](Session& s){
      ASSERT_TRUE(s.Focus(30,355));ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Home,0));
      // OnBeforeKeyStroke applies a scripted edit through SetActionData and
      // returns rc=false to stop a second default deletion. Its public boolean
      // can therefore be false after the correct edit has already occurred.
      const auto returned=s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Delete,modifiers);
      EXPECT_EQ(s.Text(),u"BCD")<<"Delete return="<<returned;
      EXPECT_EQ(s.Text(true),u"")<<"Delete must leave a collapsed selection";
      int page_index=-1;FPDF_ANNOTATION focused=nullptr;
      ASSERT_TRUE(s.api.FORM_GetFocusedAnnot(s.form,&page_index,&focused));
      Handle annotation(focused,s.api.FPDFPage_CloseAnnot);ASSERT_NE(annotation.get(),nullptr);
      EXPECT_EQ(page_index,0);EXPECT_EQ(s.api.FPDFPage_GetAnnotIndex(s.page,annotation.get()),0);
    },[=](const std::u16string& log){
      const auto expected=WideApp("[\"Keystroke\",\"Field\",\"input\",\"input\",\"\",\"\",true,"+
        std::string(modifiers&FWL_EVENTFLAG_ControlKey?"true":"false")+","+
        (modifiers&FWL_EVENTFLAG_ShiftKey?"true":"false")+",false,0,1,0,false,\"ABCD\"]");
      const auto at=log.find(expected);ASSERT_NE(at,std::u16string::npos)<<::testing::PrintToString(expected);
      EXPECT_EQ(log.find(expected,at+1),std::u16string::npos)<<"Delete keystroke must execute exactly once";
    },u"BCD");
  }
}

TEST(CoverageScriptAppEvents, CalculationSourceAndValueMutationsRemainAttachedToTheRealFields) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  EventScenario(EventFixture(),[](Session& s){
    ASSERT_TRUE(s.Focus(30,355));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"12");
    s.api.FORM_ForceToKillFocus(s.form);EXPECT_EQ(s.Field(2),u"7");
  },[](const std::u16string& log){
    ContainsEvent(log,"[\"Calculate\",\"Field\",\"calc\",\"calc\"");
    ContainsEvent(log,"[\"Source\",\"input\",\"calc\"]");
  },u"12");
}

TEST(CoverageScriptAppEvents, FullFieldKeystrokesReportTheLimitWithoutLosingExistingText) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  EventScenario(EventFixture("",4),[](Session& s){
    ASSERT_TRUE(s.Focus(30,355));ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_End,0));
    s.api.FORM_OnChar(s.form,s.page,'Z',0);EXPECT_EQ(s.Text(),u"ABCD");
  },[](const std::u16string& log){
    ContainsEvent(log,"[\"Keystroke\",\"Field\",\"input\",\"input\"");
    ContainsEvent(log,",0,true,\"ABCD\"]");
  },u"ABCD");
}

TEST(CoverageScriptAppEvents, EventReadonlyErrorsAndSelectionSettersRecoverWithinARealKeystroke) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::string extra=R"JS(
    if(!event.willCommit){
      var properties=['changeEx','commitKey','fieldFull','keyDown','modifier','name','shift','source','target','targetName','type','willCommit'];
      for(var i=0;i<properties.length;++i){var caught=false;try{event[properties[i]]=99;}catch(e){caught=true;}if(!caught)failures.push('readonly:'+properties[i]);}
      var before=event.change;event.change={toString:function(){throw 'must not coerce';}};
      if(event.change!==before)failures.push('change type');
      event.selStart={valueOf:function(){return 0;}};event.selEnd='2';
      if(event.selStart!==0||event.selEnd!==2)failures.push('selection setters');
      var current=event.value;for(var j=0;j<3;++j){var caught=false;try{event.value=[null,true,undefined][j];}catch(e){caught=true;}if(!caught)failures.push('value rejected');}
      event.value={toString:function(){return current;}};
    }
  )JS";
  EventScenario(EventFixture(extra),[](Session& s){
    ASSERT_TRUE(s.Focus(30,355));ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Home,0));
    for(int i=0;i<2;++i)ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey));
    ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,'Z',0));
  },[](const std::u16string& log){ContainsEvent(log,"[\"Keystroke\",\"Field\",\"input\",\"input\",\"Z\"");},u"ZCD");
}

}  // namespace
}  // namespace pdfoxium::integration
