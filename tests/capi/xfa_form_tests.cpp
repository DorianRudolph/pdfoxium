#include "xfa_form_fixture.h"
#include "pinned_test_fonts.h"

#include <gtest/gtest.h>

#include <array>
#include <fstream>
#include <iterator>
#include <cmath>
#include <algorithm>
#include <string>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using forms::XfaCallbacks;
using forms::XfaFixture;

void CompareXfa(const Raster& expected,const Raster& actual,Artifacts& artifacts,const std::string& label) {
  if(!raster_comparison_settings().enabled)return;
  ASSERT_EQ(expected.render_scale,actual.render_scale);
  CompareRaster(expected,actual,artifacts,label,RasterTolerance{16,0,0.04});
  const int width=static_cast<int>(std::lround(expected.width/expected.render_scale));
  const int height=static_cast<int>(std::lround(expected.height/expected.render_scale));
  for(int y=0;y<height;y+=20)for(int x=0;x<width;x+=100) {
    const int w=std::min(100,width-x),h=std::min(20,height-y);
    CompareRaster(CropLogicalRaster(expected,x,y,w,h),CropLogicalRaster(actual,x,y,w,h),artifacts,
                  label+"-region"+std::to_string(x)+"x"+std::to_string(y),RasterTolerance{16,0,0.5});
  }
}

// Full XFA exposes layout coordinates through form input. Discover the point
// with the public hit-test API rather than borrowing either engine's internals.
std::pair<double,double> AmountPoint(Session& session) {
  for(int y=5;y<300;y+=10) for(int x=5;x<100;x+=10) {
    if(session.api.FPDFPage_HasFormFieldAtPoint(session.form,session.page,x,y)==FPDF_FORMFIELD_XFA_TEXTFIELD)
      return {x,y};
  }
  ADD_FAILURE()<<"The loaded XFA text field must be hit-testable";
  return {-1,-1};
}
void FocusAmount(Session& session) {
  const auto [x,y]=AmountPoint(session);
  ASSERT_GE(x,0);ASSERT_TRUE(session.Focus(x,y));
}
void ReplaceAmount(Session& session,const std::u16string& text) {
  FocusAmount(session);
  const bool nonempty=!session.Text().empty();
  const bool selected=!!session.api.FORM_SelectAllText(session.form,session.page);
  if(nonempty) { EXPECT_TRUE(selected); }
  session.Replace(text);
  EXPECT_EQ(session.Text(),text);
}
void ExpectXfaInk(const Raster& image) {
  ASSERT_EQ(image.pixels.size(),200u*300*4);
  size_t red=0,ink=0;
  for(size_t i=0;i<image.pixels.size();i+=4) {
    red+=image.pixels[i+2]>230 && image.pixels[i+1]<30 && image.pixels[i]<30;
    ink+=image.pixels[i+2]<180 && image.pixels[i+1]<180 && image.pixels[i]<180;
  }
  EXPECT_GT(red,300u)<<"XFA's generated rectangle must actually render";
  EXPECT_GT(ink,5u)<<"The bound XFA value must actually render";
}

void ExpectDefaultSingleLinePaint(const Raster& image,int top,int width,bool native_wrap) {
  // Long controls retain the native default-100pt-breaker observation. The
  // pinned Arimo validation label fits that width, so its caller requires
  // one line from both libraries instead of demanding a native wrapping bug.
  size_t first_line=0,second_line=0;
  for(int y=top;y<top+20;++y)for(int x=0;x<width;++x) {
    const auto at=(static_cast<size_t>(y)*image.width+x)*4;
    const bool dark=image.pixels[at]<180&&image.pixels[at+1]<180&&image.pixels[at+2]<180;
    if(y<top+13)first_line+=dark;
    if(y>=top+15)second_line+=dark;
  }
  EXPECT_GT(first_line,100u);
  if(native_wrap) { EXPECT_GT(second_line,10u); }
  else { EXPECT_EQ(second_line,0u); }
}

TEST(XfaForms, ClassificationLoadingFailureAndDisabledContextsRemainUsable) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> full_images,foreground_images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    EXPECT_EQ(api.FPDF_GetFormType(nullptr),FORMTYPE_NONE);
    Handle empty(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    EXPECT_EQ(api.FPDF_GetFormType(empty.get()),FORMTYPE_NONE);
    Session acro(api,forms::Fixture());ASSERT_TRUE(acro.valid());
    EXPECT_EQ(api.FPDF_GetFormType(acro.document),FORMTYPE_ACRO_FORM);
    EXPECT_FALSE(api.FPDF_LoadXFA(acro.document));
    for(bool full:{false,true}) {
      SCOPED_TRACE(full);
      const auto bytes=XfaFixture(full);
      Handle classified(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(classified.get(),nullptr);
      const int type=full?FORMTYPE_XFA_FULL:FORMTYPE_XFA_FOREGROUND;
      EXPECT_EQ(api.FPDF_GetFormType(classified.get()),type);
      Session disabled(api,bytes,true,false);ASSERT_TRUE(disabled.valid());
      EXPECT_FALSE(api.FPDF_LoadXFA(disabled.document));
      EXPECT_EQ(api.FPDF_GetPageCount(disabled.document),1);
      EXPECT_EQ(api.FPDF_GetFormType(disabled.document),type);
      Session loaded(api,bytes,true,true,2,{},full);ASSERT_TRUE(loaded.valid());
      ASSERT_TRUE(loaded.xfa_loaded);
      EXPECT_EQ(api.FPDF_GetFormType(loaded.document),type);
      EXPECT_EQ(api.FPDF_GetPageCount(loaded.document),1);
      EXPECT_FLOAT_EQ(api.FPDF_GetPageWidthF(loaded.page),200);
      EXPECT_FLOAT_EQ(api.FPDF_GetPageHeightF(loaded.page),300);
      const auto image=loaded.Draw(200,300);
      if(full) { ExpectXfaInk(image);full_images[backend]=image; }
      else {
        forms::ExpectInk(image,0,20,100,40);
        ASSERT_TRUE(loaded.Focus(10,270));EXPECT_EQ(loaded.Text(),u"old");
        ASSERT_TRUE(api.FORM_SelectAllText(loaded.form,loaded.page));
        loaded.Replace(u"foreground");EXPECT_EQ(loaded.Text(),u"foreground");
        ASSERT_TRUE(api.FORM_ForceToKillFocus(loaded.form));
        EXPECT_EQ(loaded.Field(0),u"foreground");
        foreground_images[backend]=loaded.Draw(200,300);
      }
    }
    Session malformed(api,XfaFixture(true,"","",true),true,true);
    ASSERT_TRUE(malformed.valid());EXPECT_FALSE(malformed.xfa_loaded);
    EXPECT_EQ(api.FPDF_GetPageCount(malformed.document),1);
    EXPECT_GT(api.FPDF_GetPageWidthF(malformed.page),0);
    ++backend;
  }
  CompareXfa(full_images[0],full_images[1],artifacts,"full-xfa-initial");
  CompareXfa(foreground_images[0],foreground_images[1],artifacts,"foreground-xfa-initial");
}

TEST(XfaForms, TextEditingHistoryRightClickWheelAndSaveReloadActuallyRender) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> before,changed;
  size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());
    const auto& api=writer->api();
    Session session(api,XfaFixture(),true,true);ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);
    before[backend]=session.Draw(200,300);ExpectXfaInk(before[backend]);
    FocusAmount(session);EXPECT_EQ(session.Text(),u"old");
    int page_index=99;FPDF_ANNOTATION focused=nullptr;
    ASSERT_TRUE(api.FORM_GetFocusedAnnot(session.form,&page_index,&focused));
    if(writer->name()=="native") {
      // The pinned wrapper explicitly leaves XFA focus as a TODO.
      EXPECT_EQ(page_index,-1);EXPECT_EQ(focused,nullptr);
    } else {
      EXPECT_EQ(page_index,0);ASSERT_NE(focused,nullptr);
      Handle owned(focused,api.FPDFPage_CloseAnnot);
      EXPECT_EQ(api.FPDFAnnot_GetSubtype(owned.get()),FPDF_ANNOT_XFAWIDGET);
      EXPECT_EQ(api.FPDFAnnot_GetFormFieldType(session.form,owned.get()),FPDF_FORMFIELD_XFA_TEXTFIELD);
      ASSERT_TRUE(api.FORM_SetFocusedAnnot(session.form,owned.get()));
    }
    EXPECT_FALSE(api.FORM_SetIndexSelected(session.form,session.page,0,true));
    EXPECT_FALSE(api.FORM_IsIndexSelected(session.form,session.page,0));
    EXPECT_FALSE(api.FORM_CanUndo(session.form,session.page));
    ReplaceAmount(session,u"changed");
    ASSERT_TRUE(api.FORM_CanUndo(session.form,session.page));
    ASSERT_TRUE(api.FORM_Undo(session.form,session.page));EXPECT_EQ(session.Text(),u"old");
    ASSERT_TRUE(api.FORM_CanRedo(session.form,session.page));
    ASSERT_TRUE(api.FORM_Redo(session.form,session.page));EXPECT_EQ(session.Text(),u"changed");
    const auto point=AmountPoint(session);
    const auto x=point.first,y=point.second;
    const auto menu=[&]() {
      const auto before=session.host->popup_events;
      api.FORM_OnRButtonDown(session.form,session.page,0,x,y);
      api.FORM_OnRButtonUp(session.form,session.page,0,x,y);
      EXPECT_GT(session.host->popup_events,before);
      EXPECT_FLOAT_EQ(session.host->popup_points.back()[0],static_cast<float>(x));
      EXPECT_FLOAT_EQ(session.host->popup_points.back()[1],static_cast<float>(y));
      return session.host->popup_flags.back();
    };
    const int plain_menu=menu();
    EXPECT_EQ(plain_menu&(FXFA_MENU_COPY|FXFA_MENU_CUT|FXFA_MENU_REDO),0);
    EXPECT_EQ(plain_menu&(FXFA_MENU_SELECTALL|FXFA_MENU_UNDO|FXFA_MENU_PASTE),
              FXFA_MENU_SELECTALL|FXFA_MENU_UNDO|FXFA_MENU_PASTE);
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));
    EXPECT_EQ(session.Text(true),u"changed");
    const int selected_menu=menu();
    EXPECT_EQ(selected_menu&(FXFA_MENU_COPY|FXFA_MENU_CUT),FXFA_MENU_COPY|FXFA_MENU_CUT);
    ASSERT_TRUE(api.FORM_Undo(session.form,session.page));EXPECT_EQ(session.Text(),u"old");
    EXPECT_NE(menu()&FXFA_MENU_REDO,0);
    ASSERT_TRUE(api.FORM_Redo(session.form,session.page));EXPECT_EQ(session.Text(),u"changed");
    for(int modifiers:{0,FWL_EVENTFLAG_ControlKey|FWL_EVENTFLAG_RightButtonDown}) {
      api.FORM_OnRButtonDown(session.form,session.page,modifiers,x,y);
      api.FORM_OnRButtonUp(session.form,session.page,modifiers,x,y);
      EXPECT_EQ(session.Text(),u"changed");
    }
    ASSERT_GT(session.host->popup_events,0);
    ASSERT_FALSE(session.host->popup_flags.empty());
    EXPECT_NE(session.host->popup_flags.back()&FXFA_MENU_SELECTALL,0);
    session.host->popup_success=false;
    api.FORM_OnRButtonDown(session.form,session.page,0,x,y);
    api.FORM_OnRButtonUp(session.form,session.page,0,x,y);
    EXPECT_EQ(session.Text(),u"changed");
    const auto menu_count=session.host->popup_events;
    api.FORM_OnRButtonDown(session.form,session.page,0,190,290);
    api.FORM_OnRButtonUp(session.form,session.page,0,190,290);
    EXPECT_EQ(session.host->popup_events,menu_count);
    FocusAmount(session);
    for(int delta:{120,-120,0,360,-360}) {
      const FS_POINTF point{static_cast<float>(x),static_cast<float>(y)};
      api.FORM_OnMouseWheel(session.form,session.page,FWL_EVENTFLAG_ShiftKey,&point,delta,delta);
      EXPECT_EQ(session.Text(),u"changed");
    }
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    changed[backend]=session.Draw(200,300);ExpectXfaInk(changed[backend]);
    EXPECT_NE(before[backend].pixels,changed[backend].pixels);
    // CFWL_Edit owns and paints its internal caret. The optional host
    // DisplayCaret fallback is not selected by this portable widget.
    EXPECT_EQ(session.host->caret_events,0);
    EXPECT_TRUE(session.host->callbacks_valid);
    const auto bytes=SaveDocument(api,session.document,artifacts,writer->name()+"-xfa-edited");
    ASSERT_FALSE(bytes.empty());
    std::array<Raster,2> roundtrips;size_t reader_index=0;
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());
      Session reopened(reader->api(),bytes,true,true);ASSERT_TRUE(reopened.valid());ASSERT_TRUE(reopened.xfa_loaded);
      FocusAmount(reopened);
      // Native XML serialization injects a newline after every start tag,
      // including mixed/text-only data elements. Both readers expose it.
      EXPECT_EQ(reopened.Text(),writer->name()=="native"?u"\nchanged":u"changed");
      reopened.api.FORM_ForceToKillFocus(reopened.form);
      roundtrips[reader_index]=reopened.Draw(200,300);
      if(writer->name()!="native") CompareXfa(changed[backend],roundtrips[reader_index],artifacts,writer->name()+"-xfa-to-"+reader->name());
      ++reader_index;
    }
    CompareXfa(roundtrips[0],roundtrips[1],artifacts,writer->name()+"-xfa-readers");
    session.Close();EXPECT_EQ(session.host->releases,1);EXPECT_TRUE(session.host->timers.empty());
    ++backend;
  }
  CompareXfa(before[0],before[1],artifacts,"xfa-before-input");
  CompareXfa(changed[0],changed[1],artifacts,"xfa-after-input");
}

TEST(XfaForms, FormCalcNetworkCallbacksUseOwnedMemoryFilesAndReturnedData) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  const auto script=forms::Event(R"($.rawValue = Concat(Get("https://example.invalid/get"), Post("https://example.invalid/post", "payload", "text/plain", "UTF-8", "X-Test: value"))
Put("https://example.invalid/put", "body", "UTF-8"))","exit","application/x-formcalc");
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    XfaCallbacks callbacks;
    Session session(library->api(),XfaFixture(true,script),true,true,2,[&](forms::FormHost& h){callbacks.Configure(h);});
    ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);
    ReplaceAmount(session,u"trigger");session.api.FORM_ForceToKillFocus(session.form);
    EXPECT_EQ(callbacks.downloads,(std::vector<std::u16string>{u"https://example.invalid/get"}));
    EXPECT_EQ(callbacks.posts,(std::vector<std::array<std::u16string,5>>{{u"https://example.invalid/post",u"payload",u"text/plain",u"UTF-8",u"X-Test: value"}}));
    EXPECT_EQ(callbacks.puts,(std::vector<std::array<std::u16string,3>>{{u"https://example.invalid/put",u"body",u"UTF-8"}}));
    FocusAmount(session);EXPECT_EQ(session.Text(),u"download-responsepost-response");
    ASSERT_EQ(callbacks.files.size(),1u);EXPECT_GT(callbacks.files[0]->reads,0);EXPECT_EQ(callbacks.files[0]->releases,1);
    EXPECT_FALSE(callbacks.failed);EXPECT_FALSE(callbacks.files[0]->failed);
    const auto response=session.Draw(200,300);
    artifacts.Image(library->name()+"-original-network-response",response);
    ExpectDefaultSingleLinePaint(response,20,100,library->name()=="native");
    // Keep the full response assertions above, then compare common visible
    // content through real input without re-running the field's exit request.
    ReplaceAmount(session,u"received");
    ASSERT_TRUE(session.api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey));
    images[backend++]=session.Draw(200,300);
  }
  CompareXfa(images[0],images[1],artifacts,"xfa-formcalc-response");
}

TEST(XfaForms, FailedFormCalcRequestsLeaveTheDocumentUsable) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const auto* library:Implementations()) for(int request=0;request<3;++request) {
    SCOPED_TRACE(library->name());
    SCOPED_TRACE(request);
    const std::array<std::string,3> scripts={
      "$.rawValue = Get(\"https://example.invalid/fail-get\")",
      "$.rawValue = Post(\"https://example.invalid/fail-post\",\"payload\",\"text/plain\",\"UTF-8\",\"\")",
      "Put(\"https://example.invalid/fail-put\",\"payload\",\"UTF-8\")"};
    XfaCallbacks callbacks;callbacks.success=false;
    Session session(library->api(),XfaFixture(true,forms::Event(scripts[request],"exit","application/x-formcalc")),true,true,2,[&](forms::FormHost& h){callbacks.Configure(h);});
    ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);
    ReplaceAmount(session,u"trigger");session.api.FORM_ForceToKillFocus(session.form);
    EXPECT_EQ(callbacks.downloads.size(),request==0?1u:0u);
    EXPECT_EQ(callbacks.posts.size(),request==1?1u:0u);
    EXPECT_EQ(callbacks.puts.size(),request==2?1u:0u);
    EXPECT_TRUE(callbacks.files.empty());EXPECT_FALSE(callbacks.failed);
    ReplaceAmount(session,u"usable");
    ExpectXfaInk(session.Draw(200,300));
  }
}

TEST(XfaForms, HostNavigationPlatformLanguageAndXmlXdpExportUseCallbacks) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    XfaCallbacks callbacks;
    const auto script=forms::Event(R"(
      this.rawValue=xfa.host.platform+'|'+xfa.host.language+'|'+xfa.host.currentPage;
      xfa.host.currentPage=0;
      xfa.host.gotoURL('https://example.invalid/xfa');
      xfa.host.exportData('memory.xml',false);
      xfa.host.exportData('memory.xdp',true);
    )");
    Session session(library->api(),XfaFixture(true,script),true,true,2,[&](forms::FormHost& h){callbacks.Configure(h);});
    ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);
    ReplaceAmount(session,u"trigger");session.api.FORM_ForceToKillFocus(session.form);
    EXPECT_GT(callbacks.platform_queries,0);EXPECT_GT(callbacks.language_queries,0);
    EXPECT_EQ(callbacks.urls,(std::vector<std::u16string>{u"https://example.invalid/xfa"}));
    FocusAmount(session);EXPECT_EQ(session.Text(),u"Linux|en-US|0");
    EXPECT_EQ(callbacks.opened,(std::vector<std::tuple<int,std::u16string,std::string>>{{FXFA_SAVEAS_XML,u"memory.xml","wb"},{FXFA_SAVEAS_XDP,u"memory.xdp","wb"}}));
    ASSERT_EQ(callbacks.files.size(),2u);
    for(const auto& file:callbacks.files) {
      EXPECT_GT(file->writes,0);EXPECT_EQ(file->releases,1);EXPECT_FALSE(file->failed);
      const std::string data(file->data.begin(),file->data.end());
      EXPECT_NE(data.find("amount"),std::string::npos);
      EXPECT_NE(data.find("Linux"),std::string::npos);
    }
    EXPECT_FALSE(callbacks.failed);EXPECT_TRUE(session.host->callbacks_valid);
  }
}
TEST(XfaForms, ChoicePopupUsesPageViewportAndKeyboardSelectionChangesRenderedValue) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> closed,opened,selected;
  size_t backend=0;
  const auto choice=R"(<field name="choice" w="100pt" h="20pt"><ui><choiceList open="userControl"/></ui><font typeface="Helvetica" size="12pt"/><items><text>Red</text><text>Green</text><text>Blue</text></items><items save="1"><text>r</text><text>g</text><text>b</text></items><value><text>r</text></value></field>)";
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    XfaCallbacks callbacks;
    Session session(library->api(),XfaFixture(true,"",choice),true,true,2,[&](forms::FormHost& h){callbacks.Configure(h);});
    ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);
    const auto& api=session.api;
    std::pair<int,int> point{-1,-1};
    for(int y=5;y<300&&point.first<0;y+=10) for(int x=5;x<100;x+=10)
      if(api.FPDFPage_HasFormFieldAtPoint(session.form,session.page,x,y)==FPDF_FORMFIELD_XFA_COMBOBOX) {point={x,y};break;}
    ASSERT_GE(point.first,0);
    closed[backend]=session.Draw(200,300);
    ASSERT_TRUE(session.Focus(point.first,point.second));EXPECT_EQ(session.Text(),u"r");
    EXPECT_FALSE(api.FORM_SetIndexSelected(session.form,session.page,1,true));
    EXPECT_FALSE(api.FORM_IsIndexSelected(session.form,session.page,1));
    session.Click(95,point.second);
    opened[backend]=session.Draw(200,300);
    EXPECT_NE(opened[backend].pixels,closed[backend].pixels);
    EXPECT_GT(callbacks.page_rect_queries,0);
    forms::ExpectInk(opened[backend],0,60,90,105);
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Down,0);
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Down,0);
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Return,0);
    EXPECT_EQ(session.Text(),u"b");
    FocusAmount(session);ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    selected[backend++]=session.Draw(200,300);
    EXPECT_TRUE(session.host->callbacks_valid);
  }
  const auto single_font_paint=[](Raster image) {
    // Native's manager draws the combo edit a second time after its composite
    // owner already drew it. For black text on white, native=A*A/255 where
    // A is single-paint coverage. Undo only that source-proven duplication;
    // preserve geometry and independently compare the whole widget/page.
    for(int y=40;y<60;++y) for(int x=0;x<85;++x) {
      const auto i=(static_cast<size_t>(y)*image.width+x)*4;
      for(size_t c=0;c<3;++c) image.pixels[i+c]=static_cast<uint8_t>(std::lround(std::sqrt(image.pixels[i+c]*255.0)));
    }
    return image;
  };
  CompareXfa(single_font_paint(closed[0]),closed[1],artifacts,"xfa-choice-closed");
  CompareXfa(opened[0],opened[1],artifacts,"xfa-choice-popup");
  CompareXfa(single_font_paint(selected[0]),selected[1],artifacts,"xfa-choice-selected");
}

TEST(XfaForms, InitializationCalculationAndValidationEventsUpdateVisibleRelatedFields) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> initialized,committed;
  size_t backend=0;
  const auto events=forms::Event("this.rawValue='initialized';","initialize")+
    R"(<validate><script contentType="application/x-javascript"><![CDATA[xfa.form.root.audit.rawValue='validated:'+this.rawValue;true;]]></script></validate>)";
  const std::string related=R"(<field name="computed" w="150pt" h="20pt"><ui><textEdit/></ui><font typeface="Helvetica" size="12pt"/><calculate><script contentType="application/x-javascript"><![CDATA[this.rawValue=xfa.form.root.amount.rawValue+'!';]]></script></calculate></field><field name="audit" w="150pt" h="20pt"><ui><textEdit/></ui><font typeface="Helvetica" size="12pt"/></field>)";
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),XfaFixture(true,events,related,false,true),true,true);
    ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);
    const auto [x,y]=AmountPoint(session);
    FocusAmount(session);EXPECT_EQ(session.Text(),u"initialized");
    session.api.FORM_ForceToKillFocus(session.form);
    ASSERT_TRUE(session.Focus(x,y+20));EXPECT_EQ(session.Text(),u"initialized!");
    session.api.FORM_ForceToKillFocus(session.form);
    const auto original=session.Draw(200,300);
    artifacts.Image(library->name()+"-original-initialization",original);
    ExpectDefaultSingleLinePaint(original,60,150,false);
    ASSERT_TRUE(session.Focus(x,y+40));EXPECT_EQ(session.Text(),u"validated:initialized");
    // The long validation value has been checked in both data and paint.
    // Normalize this field through public input for a full-page comparison;
    // the amount and calculated initialization values stay visible unchanged.
    ASSERT_TRUE(session.api.FORM_SelectAllText(session.form,session.page));
    session.Replace(u"initialized");EXPECT_EQ(session.Text(),u"initialized");
    session.api.FORM_ForceToKillFocus(session.form);initialized[backend]=session.DrawForComparison(200,300);
    ReplaceAmount(session,u"good");ASSERT_TRUE(session.api.FORM_ForceToKillFocus(session.form));
    // XFA queues calculation/validation until an input UpdateScope ends.
    session.Click(x,y);
    ASSERT_TRUE(session.Focus(x,y+20));EXPECT_EQ(session.Text(),u"good!");
    ASSERT_TRUE(session.Focus(x,y+40));EXPECT_EQ(session.Text(),u"validated:good");
    session.api.FORM_ForceToKillFocus(session.form);committed[backend++]=session.DrawForComparison(200,300);
    EXPECT_TRUE(session.host->callbacks_valid);
  }
  CompareXfa(initialized[0],initialized[1],artifacts,"xfa-initialized-calculated");
  CompareXfa(committed[0],committed[1],artifacts,"xfa-validated-calculated");
  fonts.ExpectMapped();
}

TEST(XfaForms, DynamicRepeatedInstancesPaginateAndReportAddedRemovedPages) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  for(bool intact:{false,true}) {
  SCOPED_TRACE(intact?"atomic instances":"splittable instances");
  std::array<Raster,2> initial,grown,shrunk;
  size_t backend=0;
  const std::string repeated=std::string(R"(<subform name="item" layout="tb" w="100pt" h="160pt">)")+
      (intact?"<keep intact=\"contentArea\"/>":"")+R"(<occur min="1" max="4"/><draw w="80pt" h="150pt"><value><rectangle><fill><color value="0,80,220"/></fill><edge presence="hidden"/></rectangle></value></draw></subform>)";
  const auto events=forms::Event(R"(
    if(this.rawValue==='grow') {
      xfa.form.root._item.addInstance(1);xfa.form.root._item.addInstance(1);
      this.rawValue='grown';xfa.layout.relayout();
    } else if(this.rawValue==='shrink') {
      while(xfa.form.root._item.count>1) xfa.form.root._item.removeInstance(xfa.form.root._item.count-1);
      this.rawValue='shrunk';xfa.layout.relayout();
    }
  )");
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),XfaFixture(true,events,repeated),true,true);
    ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);
    const auto& api=session.api;
    const int start=api.FPDF_GetPageCount(session.document);ASSERT_EQ(start,1);
    initial[backend]=session.Draw(200,300);
    session.host->page_events.clear();
    ReplaceAmount(session,u"grow");api.FORM_ForceToKillFocus(session.form);
    const auto amount=AmountPoint(session);session.Click(amount.first,amount.second);
    // Pagination removes SDK page views. Rebuild the embedder page lifecycle
    // before interacting with the replacement layout widgets.
    session.ClosePage(0);session.page=session.host->Page(0);ASSERT_NE(session.page,nullptr);
    FocusAmount(session);EXPECT_EQ(session.Text(),u"grown");
    const int expanded=api.FPDF_GetPageCount(session.document);ASSERT_GT(expanded,start);
    const auto added=std::find_if(session.host->page_events.begin(),session.host->page_events.end(),[](const auto& value){return value.second==FXFA_PAGEVIEWEVENT_POSTADDED;});
    ASSERT_NE(added,session.host->page_events.end());EXPECT_EQ(added->first,expanded-start);
    auto later=session.host->Page(expanded-1);ASSERT_NE(later,nullptr);
    grown[backend]=session.Draw(200,300,0,0,0,0,0,0,expanded-1);
    EXPECT_NE(initial[backend].pixels,grown[backend].pixels);
    session.ClosePage(expanded-1);
    ReplaceAmount(session,u"shrink");api.FORM_ForceToKillFocus(session.form);
    session.Click(amount.first,amount.second);
    session.ClosePage(0);session.page=session.host->Page(0);ASSERT_NE(session.page,nullptr);
    FocusAmount(session);EXPECT_EQ(session.Text(),u"shrunk");
    EXPECT_EQ(api.FPDF_GetPageCount(session.document),start);
    const auto removed=std::find_if(session.host->page_events.begin(),session.host->page_events.end(),[](const auto& value){return value.second==FXFA_PAGEVIEWEVENT_POSTREMOVED;});
    ASSERT_NE(removed,session.host->page_events.end());EXPECT_EQ(removed->first,expanded-start);
    FocusAmount(session);EXPECT_EQ(session.Text(),u"shrunk");api.FORM_ForceToKillFocus(session.form);
    shrunk[backend++]=session.Draw(200,300);
    EXPECT_TRUE(session.host->callbacks_valid);
  }
  const std::string label=intact?"xfa-repeat-atomic":"xfa-repeat-splittable";
  CompareXfa(initial[0],initial[1],artifacts,label+"-initial");
  CompareXfa(shrunk[0],shrunk[1],artifacts,label+"-removed-page");
  if(intact) {
    CompareRaster(grown[0],grown[1],artifacts,label+"-added-page");
  } else {
    // Native retains a stale20pt row offset for a detached nested fragment.
    // The independent core orphan-split regression requires Rust's y0.
    // Assert every background/block pixel independently before comparing
    // the element itself, so this never becomes a page-wide pixel allowance.
    const auto expected_block=[](int top) {
      Raster image{200,300,std::vector<uint8_t>(200u*300*4,255)};
      for(int y=top;y<top+150;++y) for(int x=0;x<80;++x) {
        const auto i=(static_cast<size_t>(y)*200+x)*4;
        image.pixels[i]=220;image.pixels[i+1]=80;image.pixels[i+2]=0;
      }
      return image;
    };
    CompareRaster(expected_block(20),grown[0],artifacts,label+"-native-geometry");
    CompareRaster(expected_block(0),grown[1],artifacts,label+"-rust-geometry");
    CompareRaster(CropRaster(grown[0],0,20,80,150),CropRaster(grown[1],0,0,80,150),
                  artifacts,label+"-added-element");
  }
  }
}

TEST(XfaForms, DisabledSubmitEventsDoNotInventEmailOrUploadServices) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // CXFA_Node::ProcessEvent explicitly returns kDisabled for Submit in the
  // pinned portable implementation. FFI_EmailTo/FFI_UploadTo have no production
  // callers. Exercise a real submit control and retain a usable form afterward.
  for(const auto* library:Implementations()) for(const std::string target:{"mailto:recipient@example.invalid","https://example.invalid/upload"}) {
    SCOPED_TRACE(library->name());
    SCOPED_TRACE(target);
    const auto button="<field name=\"send\" w=\"100pt\" h=\"20pt\"><ui><button/></ui><caption><value><text>Send</text></value></caption><event activity=\"click\"><submit format=\"xml\" target=\""+target+"\"/></event></field>";
    XfaCallbacks callbacks;
    Session session(library->api(),XfaFixture(true,"",button),true,true,2,[&](forms::FormHost& h){callbacks.Configure(h);});
    ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);
    std::pair<int,int> point{-1,-1};
    for(int y=5;y<300&&point.first<0;y+=10) for(int x=5;x<100;x+=10)
      if(session.api.FPDFPage_HasFormFieldAtPoint(session.form,session.page,x,y)==FPDF_FORMFIELD_XFA_PUSHBUTTON) {point={x,y};break;}
    ASSERT_GE(point.first,0);session.Click(point.first,point.second);
    EXPECT_TRUE(callbacks.emails.empty());EXPECT_TRUE(callbacks.uploads.empty());EXPECT_TRUE(callbacks.opened.empty());
    ReplaceAmount(session,u"usable");EXPECT_EQ(session.Text(),u"usable");
    EXPECT_FALSE(callbacks.failed);EXPECT_TRUE(session.host->callbacks_valid);
  }
}

// Original demo fixture: use the same public form input and drawing APIs in
// both libraries, including the XFA template rather than its PDF fallback.
TEST(XfaForms, ViewerDemoInputPaintAndSaveReopen) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::ifstream file(TestRoot()/"crates/pdfoxium-viewer/assets/demos/xfa-demo.pdf", std::ios::binary);
  ASSERT_TRUE(file);
  const std::vector<uint8_t> bytes((std::istreambuf_iterator<char>(file)), {});
  Artifacts artifacts;
  std::array<Raster,2> before, after, selected;
  std::array<std::vector<uint8_t>,2> saved;
  size_t index=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    XfaCallbacks callbacks;
    Session session(api,bytes,true,true,2,[&](forms::FormHost& host){callbacks.Configure(host);});
    ASSERT_TRUE(session.valid()); ASSERT_TRUE(session.xfa_loaded);
    EXPECT_EQ(api.FPDF_GetPageCount(session.document),1);
    EXPECT_EQ(api.FPDFPage_HasFormFieldAtPoint(session.form,session.page,200,146),FPDF_FORMFIELD_XFA_TEXTFIELD);
    before[index]=session.DrawForComparison(612,792);
    session.Click(443,285);
    const auto popup=session.DrawForComparison(612,792);
    EXPECT_NE(CropLogicalRaster(before[index],226,311,220,36).pixels,
              CropLogicalRaster(popup,226,311,220,36).pixels);
    artifacts.Image(std::string(library->name())+"-demo-popup",popup);
    // Native requires Shift during drag; Rust accepts ordinary primary-button
    // selection. Use the native-compatible gesture for the visual comparison.
    session.Click(38,146);
    api.FORM_OnLButtonDown(session.form,session.page,0,38,146);
    api.FORM_OnMouseMove(session.form,session.page,64,130,146);
    api.FORM_OnLButtonUp(session.form,session.page,0,130,146);
    EXPECT_EQ(session.Text(true).empty(),index==0);
    api.FORM_ForceToKillFocus(session.form);
    session.Click(38,146);
    api.FORM_OnLButtonDown(session.form,session.page,0,38,146);
    api.FORM_OnMouseMove(session.form,session.page,65,130,146);
    api.FORM_OnLButtonUp(session.form,session.page,0,130,146);
    EXPECT_EQ(session.Text(true),u"Ada Lovelace");
    selected[index]=session.DrawForComparison(612,792);
    EXPECT_NE(CropLogicalRaster(before[index],36,121,360,50).pixels,
              CropLogicalRaster(selected[index],36,121,360,50).pixels);
    artifacts.Image(std::string(library->name())+"-demo-selection",selected[index]);
    // Switching out of a combo edit must stop its child caret timer.
    session.Click(300,285);
    session.Click(200,146);
    if(index==1) EXPECT_LE(session.host->timers.size(),1u);
    api.FORM_ForceToKillFocus(session.form);
    if(index==1) {
      EXPECT_TRUE(session.host->timers.empty());
      const auto blurred=session.DrawForComparison(612,792);
      artifacts.Image("rust-demo-blurred",blurred);
      // The arrow retains its separate hover/pressed paint state. Compare
      // the edit body, where the old caret must disappear, independently.
      EXPECT_EQ(CropLogicalRaster(before[index],226,261,210,50).pixels,
                CropLogicalRaster(blurred,226,261,210,50).pixels);
    }
    session.Click(200,146);
    EXPECT_TRUE(api.FORM_SelectAllText(session.form,session.page));
    session.Replace(u"Grace Hopper");
    EXPECT_EQ(session.Text(),u"Grace Hopper");
    EXPECT_TRUE(api.FORM_ForceToKillFocus(session.form));
    after[index]=session.DrawForComparison(612,792);
    EXPECT_NE(before[index].pixels,after[index].pixels);
    artifacts.Image(std::string(library->name())+"-demo-before",before[index]);
    artifacts.Image(std::string(library->name())+"-demo-after",after[index]);
    MemoryWriter writer;
    ASSERT_TRUE(api.FPDF_SaveAsCopy(session.document,&writer,0));
    saved[index]=std::move(writer.bytes);
    ++index;
  }
  for(size_t writer=0; writer<saved.size(); ++writer) {
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());
      SCOPED_TRACE(writer);
      XfaCallbacks callbacks;
      Session reopened(library->api(),saved[writer],true,true,2,[&](forms::FormHost& host){callbacks.Configure(host);});
      ASSERT_TRUE(reopened.valid()); ASSERT_TRUE(reopened.xfa_loaded);
      reopened.Click(200,146);
      // Native XML serialization adds one LF to text-bearing elements;
      // docs/capi-known-native-differences.md records this writer defect.
      EXPECT_EQ(reopened.Text(),writer==0 ? u"\nGrace Hopper" : u"Grace Hopper");
    }
  }
  // This regression covers the edited text control. Preserve full-page artifacts
  // for inspecting unrelated baseline choice-control font differences.
  CompareRaster(CropLogicalRaster(before[0],36,121,360,50),CropLogicalRaster(before[1],36,121,360,50),artifacts,"viewer-demo-before",RasterTolerance{16,0,0.04});
  CompareRaster(CropLogicalRaster(selected[0],36,121,360,50),CropLogicalRaster(selected[1],36,121,360,50),artifacts,"viewer-demo-selection",RasterTolerance{16,0,0.04});
  CompareRaster(CropLogicalRaster(after[0],36,121,360,50),CropLogicalRaster(after[1],36,121,360,50),artifacts,"viewer-demo-after",RasterTolerance{16,0,0.04});
}

TEST(XfaForms, DynamicViewerDemoAddsRemovesAndSavesRows) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::ifstream file(TestRoot()/"crates/pdfoxium-viewer/assets/demos/dynamic-xfa-demo.pdf",std::ios::binary);
  ASSERT_TRUE(file);
  const std::vector<uint8_t> bytes((std::istreambuf_iterator<char>(file)),{});
  Artifacts artifacts;
  std::array<Raster,2> initial,grown,second,shrunk;
  size_t index=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name()); const auto& api=library->api();
    Session session(api,bytes,true,true);
    ASSERT_TRUE(session.valid()); ASSERT_TRUE(session.xfa_loaded);
    ASSERT_EQ(api.FPDF_GetPageCount(session.document),1);
    initial[index]=session.DrawForComparison(612,792);
    session.Click(111,125); session.Click(111,125);
    ASSERT_EQ(api.FPDF_GetPageCount(session.document),2);
    api.FORM_ForceToKillFocus(session.form);
    grown[index]=session.DrawForComparison(612,792);
    second[index]=session.DrawForComparison(612,792,0,0,0,0,0,0,1);
    EXPECT_NE(initial[index].pixels,grown[index].pixels);
    MemoryWriter writer; ASSERT_TRUE(api.FPDF_SaveAsCopy(session.document,&writer,0));
    for(const auto* reader:Implementations()) {
      Session reopened(reader->api(),writer.bytes,true,true);
      ASSERT_TRUE(reopened.valid()); ASSERT_TRUE(reopened.xfa_loaded);
      EXPECT_EQ(reader->api().FPDF_GetPageCount(reopened.document),2);
    }
    session.ClosePage(1);
    session.Click(296,125); session.Click(296,125);
    ASSERT_EQ(api.FPDF_GetPageCount(session.document),1);
    api.FORM_ForceToKillFocus(session.form);
    shrunk[index]=session.DrawForComparison(612,792);
    EXPECT_EQ(initial[index].pixels,shrunk[index].pixels);
    artifacts.Image(std::string(library->name())+"-dynamic-grown",grown[index]);
    artifacts.Image(std::string(library->name())+"-dynamic-second",second[index]);
    ++index;
  }
  CompareRaster(initial[0],initial[1],artifacts,"dynamic-demo-initial",RasterTolerance{16,0,0.04});
  CompareRaster(grown[0],grown[1],artifacts,"dynamic-demo-grown",RasterTolerance{16,0,0.04});
  CompareRaster(second[0],second[1],artifacts,"dynamic-demo-second",RasterTolerance{16,0,0.04});
  CompareRaster(shrunk[0],shrunk[1],artifacts,"dynamic-demo-shrunk",RasterTolerance{16,0,0.04});
}

}  // namespace
}  // namespace pdfoxium::integration
