#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;

// Source: CJX_HostPseudoModel -> CXFA_FFHost -> CPDFXFA_DocEnvironment and
// CPDFXFA_Context. Only public form input executes the scripts below. UI, print,
// URL and file effects terminate in the harness's recording callbacks.
std::string TextField(const std::string& name,const std::string& value,const std::string& properties="") {
  return "<field name=\""+name+"\" w=\"180pt\" h=\"20pt\"><bind match=\"none\"/>"
      "<ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"12pt\"/>"
      "<value><text>"+value+"</text></value>"+properties+"</field>";
}
std::string CheckScript(const std::string& body) {
  return "var h=xfa.host,r=xfa.form.root,n=0;"
      "function eq(a,b){++n;if(a!==b)throw 'check '+n+': '+String(a)+' != '+String(b);}"
      "function yes(a){++n;if(!a)throw 'check '+n;}"
      "function raises(f){var caught=false;try{f();}catch(e){caught=true;}yes(caught);}"
      "try{this.caption.value.text.value='Run';"+body+
      ";r.report.rawValue='PASS';}catch(e){r.report.rawValue='FAIL';h.messageBox('FAIL:'+String(e));}";
}
std::vector<uint8_t> HostFixture(const std::string& body,const std::string& model="",int pages=1) {
  std::string contents="<draw w=\"30pt\" h=\"20pt\"><value><rectangle><fill><color value=\"0,160,0\"/></fill>"
      "<edge presence=\"hidden\"/></rectangle></value></draw>"+TextField("report","Ready")+
      "<field name=\"run\" w=\"80pt\" h=\"20pt\"><bind match=\"none\"/><ui><button/></ui>"
      "<font typeface=\"Helvetica\" size=\"10pt\"/><caption><value><text>Run</text></value></caption>"+
      forms::Event(CheckScript(body),"click")+"</field>"+model;
  if(pages>1) {
    contents="<subform name=\"first\" layout=\"tb\" w=\"200pt\" h=\"280pt\"><keep intact=\"contentArea\"/>"+contents+"</subform>";
    // Scripts use r.report/r.run in the ordinary one-page form. The navigation
    // scenario explicitly selects first.report and receives the same checked body.
    for(int page=1;page<pages;++page)
      contents+="<subform name=\"later"+std::to_string(page)+"\" layout=\"tb\" w=\"200pt\" h=\"280pt\">"
          "<breakBefore targetType=\"pageArea\" startNew=\"1\"/><keep intact=\"contentArea\"/>"+TextField("pageLabel","Page "+std::to_string(page+1))+"</subform>";
  }
  const std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
      "<subform name=\"root\" layout=\"tb\" locale=\"en_US\" restoreState=\"auto\">"
      "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/>"
      "<medium short=\"200pt\" long=\"300pt\"/><contentArea w=\"200pt\" h=\"300pt\"/></pageArea></pageSet>"+
      contents+"</subform></template>";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Resources << /Font << /Helv 10 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("",xml),
      PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
      PdfStream("","</xdp:xdp>"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
      "<< /Title (Original title) >>"},"/Info 11 0 R");
}
std::u16string MetaTitle(Session& s) {
  const auto size=s.api.FPDF_GetMetaText(s.document,"Title",nullptr,0);
  EXPECT_GE(size,2u);EXPECT_EQ(size%2,0u);if(size<2||size%2)return {};
  std::vector<uint16_t> text(size/2+1);EXPECT_EQ(s.api.FPDF_GetMetaText(s.document,"Title",text.data(),size),size);
  return {text.begin(),text.begin()+size/2-1};
}
struct HostRecording : forms::XfaCallbacks {
  bool native=false;
  int export_failure=0; // 1: open, 2: write, 3: flush; all remain mock-only.
  void Configure(forms::FormHost& host) {
    forms::XfaCallbacks::Configure(host);
    host.FFI_OpenFile=[](FPDF_FORMFILLINFO* p,int flag,FPDF_WIDESTRING path,const char* mode)->FPDF_FILEHANDLER* {
      auto& state=static_cast<HostRecording&>(forms::XfaCallbacks::Self(p));
      try {
        state.opened.emplace_back(flag,forms::Wide(path),mode?mode:"");
        if(state.export_failure==1)return nullptr;
        auto* file=state.File();
        if(state.export_failure==2)file->WriteBlock=[](void* p,FPDF_DWORD,const void*,FPDF_DWORD)->FPDF_RESULT {
          ++static_cast<forms::MemoryFile*>(p)->writes;return 1;
        };
        if(state.export_failure==3)file->Flush=[](void* p)->FPDF_RESULT {
          ++static_cast<forms::MemoryFile*>(p)->flushes;return 1;
        };
        return file;
      }catch(...){state.failed=true;return nullptr;}
    };
  }
};
using Configure=std::function<void(forms::FormHost&,HostRecording&)>;
using Inspect=std::function<void(Session&,HostRecording&)>;
void CompareHost(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& name,bool landmark=true) {
  CompareRaster(a,b,artifacts,name,RasterTolerance{24,0,.2});
  CompareRaster(CropRaster(a,0,20,180,20),CropRaster(b,0,20,180,20),artifacts,name+"-report",RasterTolerance{24,0,1});
  // Only page zero contains the actual green landmark. Later pages have
  // antialiased text here and retain the full-scene text tolerance above.
  if(landmark){CompareRaster(CropRaster(a,0,0,30,20),CropRaster(b,0,0,30,20),artifacts,name+"-landmark");}
}
void ExecuteHost(Session& session) {
  // Draw first to finish lazy widget loading before callbacks execute.
  (void)session.Draw(200,300);session.host->alerts.clear();session.Click(8,50);
  ASSERT_TRUE(session.Focus(8,25));EXPECT_EQ(session.Text(),u"PASS");
  for(const auto& alert:session.host->alerts)
    EXPECT_NE(std::get<0>(alert).substr(0,5),u"FAIL:")<<::testing::PrintToString(std::get<0>(alert));
  session.api.FORM_ForceToKillFocus(session.form);
  EXPECT_TRUE(session.host->callbacks_valid);EXPECT_FALSE(session.host->callback_error);
}
void RunHost(const std::string& script,const std::string& model="",const Configure& configure={},
             const Inspect& inspect={},bool save=true,int pages=1) {
  const auto fixture=HostFixture(script,model,pages);Artifacts artifacts;
  std::array<std::vector<Raster>,2> images;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());HostRecording callbacks;callbacks.native=writer->name()=="native";
    Session session(writer->api(),fixture,true,true,2,[&](forms::FormHost& host){callbacks.Configure(host);if(configure)configure(host,callbacks);});
    ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);ASSERT_EQ(session.api.FPDF_GetPageCount(session.document),pages);
    const auto before=session.Draw(200,300);ExecuteHost(session);ASSERT_FALSE(::testing::Test::HasFailure());
    if(inspect){inspect(session,callbacks);}
    EXPECT_FALSE(callbacks.failed);
    for(int page=0;page<pages;++page)images[backend].push_back(session.Draw(200,300,0,0,0,0,0,0,page));
    forms::ExpectInk(images[backend][0],0,20,180,40);
    EXPECT_NE(CropRaster(before,0,20,180,20).pixels,CropRaster(images[backend][0],0,20,180,20).pixels);
    if(save) {
      const auto bytes=SaveDocument(session.api,session.document,artifacts,writer->name()+"-host");ASSERT_FALSE(bytes.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());HostRecording reloaded_callbacks;reloaded_callbacks.native=reader->name()=="native";
        Session reloaded(reader->api(),bytes,true,true,2,[&](forms::FormHost& host){reloaded_callbacks.Configure(host);if(configure)configure(host,reloaded_callbacks);});
        ASSERT_TRUE(reloaded.valid());ASSERT_TRUE(reloaded.xfa_loaded);ASSERT_EQ(reloaded.api.FPDF_GetPageCount(reloaded.document),pages);
        EXPECT_EQ(MetaTitle(reloaded),MetaTitle(session));
        ExecuteHost(reloaded);ASSERT_FALSE(::testing::Test::HasFailure());if(inspect)inspect(reloaded,reloaded_callbacks);
        EXPECT_FALSE(reloaded_callbacks.failed);
        for(int page=0;page<pages;++page)CompareHost(images[backend][page],reloaded.Draw(200,300,0,0,0,0,0,0,page),artifacts,writer->name()+"-"+reader->name()+"-host-"+std::to_string(page),page==0);
      }
    }
    ++backend;
  }
  for(int page=0;page<pages;++page)CompareHost(images[0][page],images[1][page],artifacts,"host-backends-"+std::to_string(page),page==0);
}

TEST(CoverageXfaHostCallbacks, ReadonlyPropertiesAndUnicodeTitleReachRealDocumentMetadata) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunHost(R"JS(
    eq(h.appType,'Exchange');eq(h.name,'Acrobat');eq(h.version,'11');eq(h.variation,'Full');
    eq(h.platform,'Linux');eq(h.language,'en-US');eq(h.numPages,1);eq(h.currentPage,0);
    ['appType','name','version','variation','language','platform','numPages'].forEach(function(key){
      var original=h[key];raises(function(){h[key]='replacement';});eq(h[key],original);
    });
    ['ASCII title',17,true,null,''].forEach(function(value){h.title=value;eq(h.title,String(value));});
    var title='Title \u00e9 \u03a9';h.title=title;
    // Native GetTitle byte-decodes a UTF16BE PDF string as DefANSI. Preserve
    // that exact legacy observation separately from correct Unicode metadata.
    var legacy=String.fromCharCode(254,255);
    for(var i=0;i<title.length;++i){var code=title.charCodeAt(i);legacy+=String.fromCharCode(code>>8,code&255);}
    if(h.title===title)h.messageBox('unicode-title');
    else{eq(h.title,legacy);h.messageBox('legacy-title');}
  )JS","",{},[](Session& s,HostRecording& c){
    EXPECT_EQ(MetaTitle(s),u"Title \u00e9 \u03a9");EXPECT_GT(c.platform_queries,0);EXPECT_GT(c.language_queries,0);
    ASSERT_EQ(s.host->alerts.size(),1u);EXPECT_EQ(std::get<0>(s.host->alerts[0]),c.native?u"legacy-title":u"unicode-title");
  });
}

TEST(CoverageXfaHostCallbacks, MessageBoxDefaultsNormalizationUnicodeAndReturnValuesUseCallbacks) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::array<int,6> values={-1,0,1,2,3,7};
  std::string script="eq(h.messageBox('one'),4);eq(h.messageBox('two','title'),4);eq(h.messageBox('three','T',2),4);";
  for(int value:values)script+="eq(h.messageBox('message \\u00e9 \\u03a9','Title',"+std::to_string(value)+","+std::to_string(value)+"),4);";
  script+="raises(function(){h.messageBox();});raises(function(){h.messageBox(1,2,3,4,5);});";
  RunHost(script,"",[](forms::FormHost& h,HostRecording&){h.alert_result=4;},[=](Session& s,HostRecording&){
    ASSERT_EQ(s.host->alerts.size(),9u);
    EXPECT_EQ(s.host->alerts[0],(std::make_tuple(u"one",u"",0,0)));
    EXPECT_EQ(s.host->alerts[1],(std::make_tuple(u"two",u"title",0,0)));
    EXPECT_EQ(s.host->alerts[2],(std::make_tuple(u"three",u"T",0,2)));
    for(size_t i=0;i<values.size();++i){const int normalized=values[i]<0||values[i]>3?0:values[i];
      EXPECT_EQ(s.host->alerts[i+3],(std::make_tuple(u"message \u00e9 \u03a9",u"Title",normalized,normalized)));}
  });
}

TEST(CoverageXfaHostCallbacks, ResponseDefaultsMaskingUnicodeAndBufferLimitsPreserveCallbackPayloads) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int length:{0,5,1100}) {
    SCOPED_TRACE(length);const std::u16string response=length==5?u"r\u00e9\u03a9ly":std::u16string(length,u'x');
    const std::string expected=length==5?"'r\\u00e9\\u03a9ly'":("Array("+std::to_string(std::min(length,1024)+1)+").join('x')");
    RunHost(R"JS(
      var replies=[h.response('Q'),h.response('Q','T'),h.response('Q','T','D'),
        h.response('Q','T','D',1),h.response('Q','T','D',-1),h.response('Q','T','D',0),h.response('Q','T','D','2'),
        h.response('Q\u00e9','T\u03a9','D\u00e9',true)];
      replies.forEach(function(value){eq(value.length,)JS"+std::to_string(std::min(length,1024))+R"JS();});
      eq(replies[0],replies[6]);eq(replies[0],)JS"+expected+R"JS();
      raises(function(){h.response();});raises(function(){h.response(1,2,3,4,5);});
    )JS","",[=](forms::FormHost& h,HostRecording&){h.response=response;},[=](Session& s,HostRecording&){
      ASSERT_EQ(s.host->responses.size(),8u);EXPECT_EQ(s.host->responses[0],(std::array<std::u16string,4>{u"Q",u"",u"",u""}));
      EXPECT_EQ(s.host->responses[1],(std::array<std::u16string,4>{u"Q",u"T",u"",u""}));
      for(size_t i=2;i<7;++i)EXPECT_EQ(s.host->responses[i],(std::array<std::u16string,4>{u"Q",u"T",u"D",u""}));
      EXPECT_EQ(s.host->responses[7],(std::array<std::u16string,4>{u"Q\u00e9",u"T\u03a9",u"D\u00e9",u""}));
      EXPECT_EQ(s.host->response_passwords,(std::vector<int>{0,0,0,1,1,0,1,1}));
      EXPECT_EQ(s.host->response_capacities,std::vector<int>(8,2048));
    });
  }
}

TEST(CoverageXfaHostCallbacks, BeepUrlBatchAndArityErrorsLeaveFollowingCallbacksUsable) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunHost(R"JS(
    h.beep();[0,1,2,3,4,5,-1,1.9,'2',null].forEach(function(v){h.beep(v);});
    h.gotoURL('https://example.invalid/\u00e9');h.gotoURL('');h.gotoURL(17);
    eq(h.documentCountInBatch(),0);eq(h.documentCountInBatch(1,2),0);eq(h.documentInBatch(),0);eq(h.documentInBatch('anything'),0);
    eq(h.importData('unused.xml'),undefined);
    [function(){h.beep(1,2);},function(){h.gotoURL();},function(){h.gotoURL(1,2);},
     function(){h.exportData();},function(){h.exportData(1,2,3);},function(){h.importData();},function(){h.importData(1,2);},
     function(){h.setFocus();},function(){h.setFocus(1,2);},function(){h.openList();},function(){h.openList(1,2);},
     function(){h.resetData(1,2);},function(){h.print();},function(){h.print(1,2,3,4,5,6,7,8,9);}
    ].forEach(function(f){raises(f);});
    h.beep(3);h.gotoURL('https://example.invalid/recovered');
  )JS","",{},[](Session& s,HostRecording& c){
    EXPECT_EQ(s.host->beeps,(std::vector<int>{4,0,1,2,3,4,5,-1,1,2,0,3}));
    EXPECT_EQ(c.urls,(std::vector<std::u16string>{u"https://example.invalid/\u00e9",u"",u"17",u"https://example.invalid/recovered"}));
    EXPECT_TRUE(c.opened.empty());EXPECT_TRUE(s.host->alerts.empty());
  });
}

TEST(CoverageXfaHostCallbacks, PrintFlagsAndPageRangeConversionsReachMockWithoutPrinting) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::string script;std::vector<std::array<int,8>> expected;
  for(int bits=0;bits<64;++bits) {
    const std::array<int,8> row={bits&1,1,2,(bits>>1)&1,(bits>>2)&1,(bits>>3)&1,(bits>>4)&1,(bits>>5)&1};
    script+="h.print(";for(size_t i=0;i<row.size();++i){if(i)script+=",";script+=std::to_string(row[i]);}script+=");";expected.push_back(row);
  }
  script+="h.print(false,-2,100,false,false,false,false,false);h.print(true,'2.9',null,1,'',{},[],null);";
  expected.push_back({0,-2,100,0,0,0,0,0});expected.push_back({1,2,0,1,0,1,1,0});
  RunHost(script,"",{},[=](Session& s,HostRecording&){EXPECT_EQ(s.host->prints,expected);EXPECT_TRUE(s.host->alerts.empty());});
}

TEST(CoverageXfaHostCallbacks, CurrentPageAssignmentAndNavigationRespectRealThreePageBounds) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // The check wrapper's report lives in first for this flowed multi-page form.
  const std::string script=R"JS(
    var root=r;r=r.first;eq(h.numPages,3);eq(h.currentPage,0);
    h.currentPage=1;eq(h.currentPage,1);h.pageDown();eq(h.currentPage,2);
    h.pageDown();eq(h.currentPage,2);h.currentPage=-1;eq(h.currentPage,2);
    h.currentPage=3;eq(h.currentPage,2);h.currentPage=999;eq(h.currentPage,2);
    h.pageUp();eq(h.currentPage,1);h.currentPage='2.9';eq(h.currentPage,2);
    h.currentPage=null;eq(h.currentPage,0);h.currentPage=0;eq(h.currentPage,0);
  )JS";
  RunHost(script,"",{},[](Session& s,HostRecording&){
    EXPECT_EQ(s.host->current_page,0);EXPECT_EQ(s.host->goto_pages,(std::vector<int>{1,2,1,2,0,0}));
  },true,3);
}

TEST(CoverageXfaHostCallbacks, FocusObjectsSomMissingAndNullPreserveRealControlIdentity) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunHost(R"JS(
    h.setFocus(r.input);eq(h.getFocus().name,'input');r.input.rawValue='Target';
    h.setFocus('other');eq(h.getFocus().name,'other');r.other.rawValue='Other';
    h.setFocus('missing');eq(h.getFocus().name,'other');
    h.setFocus(null);eq(h.getFocus(),undefined);
    h.setFocus('input');eq(h.getFocus().name,'input');h.setFocus(17);eq(h.getFocus(),undefined);
    h.setFocus(r.other);eq(h.getFocus().name,'other');h.setFocus(this);eq(h.getFocus().name,'run');
    h.openList(r.input);h.openList('missing');h.openList(null);h.openList(17);
  )JS",TextField("input","Initial",forms::Event("xfa.host.messageBox('input-enter');","enter")+forms::Event("xfa.host.messageBox('input-exit');","exit"))+
      TextField("other","Before",forms::Event("xfa.host.messageBox('other-enter');","enter")+forms::Event("xfa.host.messageBox('other-exit');","exit")),{},[](Session& s,HostRecording&){
    ASSERT_TRUE(s.Focus(8,65));EXPECT_EQ(s.Text(),u"Target");s.api.FORM_ForceToKillFocus(s.form);
    ASSERT_TRUE(s.Focus(8,85));EXPECT_EQ(s.Text(),u"Other");s.api.FORM_ForceToKillFocus(s.form);
    for(const auto& expected:std::array<std::u16string,4>{u"input-enter",u"input-exit",u"other-enter",u"other-exit"}) {
      bool observed=false;for(const auto& alert:s.host->alerts)observed|=std::get<0>(alert)==expected;
      EXPECT_TRUE(observed)<<::testing::PrintToString(expected);
    }
    // FFI_SetTextFieldFocus is an AcroForm notification. Actual XFA focus is
    // checked through getFocus, public focused text and real enter/exit events.
    EXPECT_TRUE(s.host->focus_values.empty());
  });
}

TEST(CoverageXfaHostCallbacks, CalculationEnableCoercionsSuspendAndResumeRealDependentFields) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::string computed=TextField("computed","6",
      "<calculate><script contentType=\"application/x-javascript\"><![CDATA[Number(xfa.form.root.input.rawValue)*3;]]></script></calculate>");
  const std::string enable="<field name=\"enable\" w=\"80pt\" h=\"20pt\"><bind match=\"none\"/><ui><button/></ui>"
      "<font typeface=\"Helvetica\" size=\"10pt\"/><caption><value><text>Enable</text></value></caption>"+
      forms::Event("this.caption.value.text.value='Enable';xfa.host.calculationsEnabled=true;xfa.form.root.input.rawValue='9';","click")+"</field>";
  RunHost(R"JS(
    [false,0,'',null,true,1,'yes',{}].forEach(function(v){h.calculationsEnabled=v;eq(h.calculationsEnabled,Boolean(v));});
    h.calculationsEnabled=true;r.input.rawValue='2';r.computed.execCalculate();eq(Number(r.computed.rawValue),6);
    h.calculationsEnabled=false;eq(h.calculationsEnabled,false);r.input.rawValue='7';
  )JS",TextField("input","2")+computed+enable,{},[](Session& s,HostRecording&){
    ASSERT_TRUE(s.Focus(8,85));EXPECT_EQ(s.Text(),u"6");s.api.FORM_ForceToKillFocus(s.form);
    ASSERT_TRUE(s.Focus(8,65));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"8");
    s.api.FORM_ForceToKillFocus(s.form);ASSERT_TRUE(s.Focus(8,85));EXPECT_EQ(s.Text(),u"6");
    s.api.FORM_ForceToKillFocus(s.form);s.Click(8,110);
    ASSERT_TRUE(s.Focus(8,85));EXPECT_EQ(s.Text(),u"27");s.api.FORM_ForceToKillFocus(s.form);
    ASSERT_TRUE(s.Focus(8,65));EXPECT_EQ(s.Text(),u"9");s.api.FORM_ForceToKillFocus(s.form);
  });
}

TEST(CoverageXfaHostCallbacks, OpenListObjectAndSomArgumentsDrawPopupAndCommitKeyboardSelection) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // Normalize the known native-writer prefix during initialization, before an
  // initial closed-control render can visit the saved popup's item text. The
  // native text-out CHECKs on LF+Red in a single 12 pt row; the debugger confirms a healthy 94 pt width.
  const std::string clean_items=R"JS(
    for(var g=0;g<this.nodes.length;++g){var group=this.nodes.item(g);if(group.className!=='items')continue;
      for(var i=0;i<group.nodes.length;++i){var item=group.nodes.item(i),value=String(item.value);
        if(value.charCodeAt(0)===10)item.value=value.slice(1);}}
  )JS";
  const std::string choice="<field name=\"choice\" w=\"100pt\" h=\"20pt\"><bind match=\"none\"/>"
      "<ui><choiceList open=\"userControl\"/></ui><font typeface=\"Helvetica\" size=\"12pt\"/>"
      "<items><text>Red</text><text>Green</text><text>Blue</text></items>"
      "<items save=\"1\"><text>r</text><text>g</text><text>b</text></items><value><text>r</text></value>"+
      forms::Event(clean_items,"initialize")+"</field>";
  for(bool object:{false,true}) {
    SCOPED_TRACE(object);const auto fixture=HostFixture(R"JS(
      // Native-writer XML can prefix item content with one LF. Retain every
      // real item/mapping and remove only that documented serialization prefix.
      for(var g=0;g<r.choice.nodes.length;++g){var group=r.choice.nodes.item(g);if(group.className!=='items')continue;
        for(var i=0;i<group.nodes.length;++i){var item=group.nodes.item(i),value=String(item.value);
          if(value.charCodeAt(0)===10)item.value=value.slice(1);}}
      ['Red','Green','Blue'].forEach(function(value,i){eq(r.choice.getDisplayItem(i),value);});
      ['r','g','b'].forEach(function(value,i){eq(r.choice.getSaveItem(i),value);});
      r.choice.rawValue='r';h.setFocus(r.choice);h.openList()JS"+
        std::string(object?"r.choice":"'choice'")+");h.messageBox('popup-ready');",choice);
    Artifacts artifacts;std::array<Raster,2> opened;size_t backend=0;
    const auto open_and_select=[&](Session& s,HostRecording& callbacks,Raster& popup){
      const auto closed=s.Draw(200,300);s.host->alerts.clear();s.Click(8,50);
      ASSERT_FALSE(s.host->alerts.empty());
      ASSERT_EQ(std::get<0>(s.host->alerts.back()),u"popup-ready")<<::testing::PrintToString(std::get<0>(s.host->alerts.back()));
      popup=s.Draw(200,300);
      EXPECT_NE(closed.pixels,popup.pixels);EXPECT_GT(callbacks.page_rect_queries,0);
      forms::ExpectInk(popup,0,80,90,125);
      ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Down,0));
      ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Down,0));
      ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Return,0));EXPECT_EQ(s.Text(),u"b");
      s.api.FORM_ForceToKillFocus(s.form);ASSERT_TRUE(s.Focus(8,25));EXPECT_EQ(s.Text(),u"PASS");
      s.api.FORM_ForceToKillFocus(s.form);ASSERT_EQ(s.host->alerts.size(),1u);
      EXPECT_EQ(std::get<0>(s.host->alerts[0]),u"popup-ready");
      EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);EXPECT_FALSE(callbacks.failed);
    };
    const auto compare_popup=[&](const Raster& a,const Raster& b,const std::string& label){
      // Native's documented composite-widget double paint affects only the
      // closed edit text. Preserve both full scenes as artifacts and compare
      // the actual popup rows and the unaffected report/landmark separately.
      artifacts.Image(label+"-a",a);artifacts.Image(label+"-b",b);
      CompareRaster(CropRaster(a,0,80,100,70),CropRaster(b,0,80,100,70),artifacts,label+"-popup",RasterTolerance{24,0,.5});
      CompareRaster(CropRaster(a,0,0,200,60),CropRaster(b,0,0,200,60),artifacts,label+"-top",RasterTolerance{24,0,.2});
      CompareRaster(CropRaster(a,100,60,100,240),CropRaster(b,100,60,100,240),artifacts,label+"-right");
      CompareRaster(CropRaster(a,0,150,100,150),CropRaster(b,0,150,100,150),artifacts,label+"-below");
    };
    for(const auto* writer:Implementations()) {
      SCOPED_TRACE(writer->name());HostRecording callbacks;callbacks.native=writer->name()=="native";
      Session s(writer->api(),fixture,true,true,2,[&](forms::FormHost& host){callbacks.Configure(host);});
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);open_and_select(s,callbacks,opened[backend]);
      ASSERT_FALSE(::testing::Test::HasFailure());const auto bytes=SaveDocument(s.api,s.document,artifacts,writer->name()+"-open-list");ASSERT_FALSE(bytes.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());HostRecording reload_callbacks;
        Session reloaded(reader->api(),bytes,true,true,2,[&](forms::FormHost& host){reload_callbacks.Configure(host);});
        ASSERT_TRUE(reloaded.valid());ASSERT_TRUE(reloaded.xfa_loaded);Raster popup;open_and_select(reloaded,reload_callbacks,popup);
        compare_popup(opened[backend],popup,writer->name()+"-"+reader->name()+"-open-list-reload");
      }
      ++backend;
    }
    compare_popup(opened[0],opened[1],"open-list-backends");
  }
}

TEST(CoverageXfaHostCallbacks, ExportBrowseAndFileFailuresReleaseOwnershipAndPermitHealthyRetry) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(bool xdp:{false,true})for(int failure=0;failure<4;++failure) {
    SCOPED_TRACE(xdp);
      SCOPED_TRACE(failure);
    const std::string script="h.exportData('',"+std::string(xdp?"true":"false")+");h.beep(2);";
    RunHost(script,"",[=](forms::FormHost& h,HostRecording& c){h.browse="chosen-export.xml";c.export_failure=failure;},
      [=](Session& s,HostRecording& c){
        EXPECT_GT(s.host->browse_queries,0);ASSERT_EQ(c.opened.size(),1u);
        EXPECT_EQ(c.opened[0],(std::make_tuple(xdp?FXFA_SAVEAS_XDP:FXFA_SAVEAS_XML,u"chosen-export.xml","wb")));
        EXPECT_EQ(s.host->beeps,(std::vector<int>{2}));
        if(failure==1){EXPECT_TRUE(c.files.empty());}
        else {ASSERT_EQ(c.files.size(),1u);EXPECT_GT(c.files[0]->writes,0);EXPECT_EQ(c.files[0]->releases,1);
          if(failure!=2){EXPECT_GT(c.files[0]->flushes,0);}
          if(!failure){EXPECT_FALSE(c.files[0]->data.empty());}}
        // Repeat the same public event with a healthy fresh file callback; the
        // failed stream must not poison the document or retain ownership.
        c.export_failure=0;const auto opened=c.opened.size();s.Click(8,50);ASSERT_EQ(c.opened.size(),opened+1);
        ASSERT_FALSE(c.files.empty());const auto& recovered=*c.files.back();EXPECT_GT(recovered.writes,0);
        EXPECT_GT(recovered.flushes,0);EXPECT_EQ(recovered.releases,1);EXPECT_FALSE(recovered.data.empty());
        ASSERT_TRUE(s.Focus(8,25));EXPECT_EQ(s.Text(),u"PASS");s.api.FORM_ForceToKillFocus(s.form);
      });
  }
}

} // namespace
} // namespace pdfoxium::integration
