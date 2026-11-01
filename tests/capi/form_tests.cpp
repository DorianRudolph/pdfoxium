#include "pinned_test_fonts.h"
#include "form_fixture.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <fstream>
#include <iterator>
#include <set>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Actions;
using forms::Fixture;
using forms::Session;
using forms::Terminated;

void CompareFormRaster(const Raster& expected,const Raster& actual,Artifacts& artifacts,
                       const std::string& label) {
  // Observed standard-font AA coverage differences are mostly 1–3 levels,
  // max17, with mean <=0.31 per 64px tile and <=0.162 per widget. Require
  // every pixel within18 levels and bound error on each field independently.
  // Selection colors, missing widgets and displaced borders exceed this bound.
  CompareRaster(expected,actual,artifacts,label,RasterTolerance{18,0,0.06});
  if(expected.width==400 && expected.height==400) {
    const std::array<std::array<int,4>,13> fields={{{20,30,160,30},{220,30,160,30},
      {20,80,30,30},{70,80,30,30},{120,80,80,30},{20,130,160,30},
      {220,110,160,110},{20,230,160,30},{20,280,160,100},{220,230,160,30},
      {220,270,160,30},{220,350,160,30},{20,190,160,30}}};
    for(size_t i=0;i<fields.size();++i) {
      const auto& r=fields[i];
      CompareRaster(CropRaster(expected,r[0],r[1],r[2],r[3]),
                    CropRaster(actual,r[0],r[1],r[2],r[3]),artifacts,
                    label+"-field"+std::to_string(i),RasterTolerance{18,0,0.25});
    }
  } else {
    for(int y=0;y<expected.height;y+=64) for(int x=0;x<expected.width;x+=64) {
      const int width=std::min(64,expected.width-x),height=std::min(64,expected.height-y);
      CompareRaster(CropRaster(expected,x,y,width,height),CropRaster(actual,x,y,width,height),
                    artifacts,label+"-region"+std::to_string(x)+"x"+std::to_string(y),
                    RasterTolerance{18,0,0.4});
    }
  }
}
void CompareForms(const std::array<Raster,2>& images, Artifacts& artifacts,
                   const std::string& label) {
  CompareFormRaster(images[0],images[1],artifacts,label);
}

void CheckTextBuffers(Session& session, bool selected, const std::u16string& text,
                       int page_index = 0) {
  const auto get = selected ? session.api.FORM_GetSelectedText : session.api.FORM_GetFocusedText;
  auto page = session.host->Page(page_index);
  const auto required = static_cast<unsigned long>((text.size()+1)*2);
  EXPECT_EQ(get(session.form,page,nullptr,0),required);
  for (unsigned long capacity : {0ul,1ul,required-1,required,required+8}) {
    std::vector<uint16_t> storage((required+16)/2+2,0xa55a);
    const auto before = storage;
    EXPECT_EQ(get(session.form,page,storage.data()+1,capacity),required);
    if(capacity<required) { EXPECT_EQ(storage,before); }
    else {
      EXPECT_EQ(std::vector<uint16_t>(storage.begin()+1,storage.begin()+1+text.size()),
                std::vector<uint16_t>(text.begin(),text.end()));
      EXPECT_EQ(storage[text.size()+1],0);
      EXPECT_EQ(storage.front(),0xa55a);
      EXPECT_TRUE(std::all_of(storage.begin()+text.size()+2,storage.end(),
                              [](uint16_t v){return v==0xa55a;}));
    }
  }
}

void CheckSavedField(const Library& writer, Session& session, Artifacts& artifacts,
                     const std::string& label, const std::u16string& expected,
                     int annotation = 0) {
  session.api.FORM_ForceToKillFocus(session.form);
  int focused_page=-1; FPDF_ANNOTATION focused=nullptr;
  ASSERT_TRUE(session.api.FORM_GetFocusedAnnot(session.form,&focused_page,&focused));
  EXPECT_EQ(focused_page,-1); EXPECT_EQ(focused,nullptr);
  const auto before = session.Draw();
  const auto bytes = SaveDocument(writer.api(),session.document,artifacts,label);
  ASSERT_FALSE(bytes.empty());
  std::array<Raster,2> images;
  size_t index=0;
  for(const auto* reader:Implementations()) {
    SCOPED_TRACE(label+" reader="+reader->name());
    Session reopened(reader->api(),bytes);
    ASSERT_TRUE(reopened.valid());
    EXPECT_EQ(reopened.Field(annotation),expected);
    images[index]=reopened.Draw();
    CompareFormRaster(before,images[index],artifacts,label+"-to-"+reader->name());
    ++index;
  }
  CompareForms(images,artifacts,label+"-readers");
}

TEST(Forms, EnvironmentOwnershipVersionsDisabledJavaScriptAndReentrantCallbacks) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    Session plain(api,PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] >>"}));
    ASSERT_TRUE(plain.valid());EXPECT_EQ(api.FPDF_GetFormType(plain.document),FORMTYPE_NONE);
    EXPECT_FALSE(plain.Focus(30,355));EXPECT_FALSE(api.FORM_ForceToKillFocus(plain.form));
    api.FORM_DoDocumentJSAction(plain.form);api.FORM_DoDocumentOpenAction(plain.form);
    api.FORM_DoDocumentAAction(plain.form,FPDFDOC_AACTION_WS);
    api.FORM_DoPageAAction(plain.page,plain.form,FPDFPAGE_AACTION_OPEN);
    EXPECT_EQ(api.FPDFPage_HasFormFieldAtPoint(plain.form,plain.page,30,355),-1);
    plain.Close();EXPECT_EQ(plain.host->releases,1);
    Actions actions; actions.document="this.getField('input').value='executed';";
    Session disabled(api,Fixture(actions));
    Session enabled(api,Fixture(actions),true);
    ASSERT_TRUE(disabled.valid()); ASSERT_TRUE(enabled.valid());
    EXPECT_NE(disabled.document,enabled.document); EXPECT_NE(disabled.form,enabled.form);
    EXPECT_EQ(api.FPDF_GetFormType(disabled.document),FORMTYPE_ACRO_FORM);
    api.FORM_DoDocumentJSAction(disabled.form);
    EXPECT_EQ(disabled.Field(0),u"");
    api.FORM_DoDocumentJSAction(enabled.form);
    EXPECT_EQ(enabled.Field(0),u"executed");
    Handle independent(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    ASSERT_NE(independent.get(),nullptr);
    disabled.host->reenter=true; disabled.host->independent=independent.get();
    ASSERT_TRUE(disabled.Focus(30,355));
    ASSERT_TRUE(api.FORM_OnChar(disabled.form,disabled.page,'A',0));
    EXPECT_EQ(disabled.Text(),u"A");
    ASSERT_TRUE(api.FORM_ForceToKillFocus(disabled.form));
    EXPECT_TRUE(disabled.host->callbacks_valid); EXPECT_FALSE(disabled.host->callback_error);
    EXPECT_GT(disabled.host->invalidations,0);
    EXPECT_GT(disabled.host->focus_values.size(),0u);
    images[backend++]=disabled.Draw();
    disabled.Close();
    EXPECT_EQ(disabled.host->releases,1);
    EXPECT_TRUE(disabled.host->timers.empty());
    EXPECT_EQ(enabled.Field(0),u"executed");
    api.FPDFDOC_ExitFormFillEnvironment(nullptr);
    Session another(api,Fixture()); ASSERT_TRUE(another.valid());
    for(int version:{0,1,3}) {
      auto info=std::make_unique<forms::FormHost>(api); info->version=version;
      const auto bytes=Fixture();
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);
      auto form=api.FPDFDOC_InitFormFillEnvironment(document.get(),info.get());
      // The pinned XFA-enabled public ABI requires interface version2.
      EXPECT_EQ(form,nullptr);
      if(form) api.FPDFDOC_ExitFormFillEnvironment(form);
    }
  }
  CompareForms(images,artifacts,"environment-reentry");
}

TEST(Forms, PageNotificationsCommitAndReloadWithoutStaleCallbacks) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture()); ASSERT_TRUE(session.valid());
    auto later=session.host->Page(1); ASSERT_NE(later,nullptr);
    ASSERT_TRUE(session.Focus(30,355));
    session.Replace(u"committed");
    EXPECT_EQ(session.Text(),u"committed");
    session.ClosePage(0);
    EXPECT_EQ(session.host->pages[0],nullptr);
    EXPECT_EQ(session.Field(0,1),u"later");
    session.page=session.host->Page(0); ASSERT_NE(session.page,nullptr);
    EXPECT_EQ(session.Field(0),u"committed");
    ASSERT_TRUE(session.Focus(30,355)); EXPECT_EQ(session.Text(),u"committed");
    ASSERT_TRUE(session.api.FORM_ForceToKillFocus(session.form));
    images[backend++]=session.Draw();
    CheckSavedField(*library,session,artifacts,library->name()+"-page-reload",u"committed");
  }
  CompareForms(images,artifacts,"page-reload");
}

TEST(Forms, PointFocusOwnedAnnotationIdentityAndFieldHitTesting) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  const std::array<std::tuple<double,double,int>,7> points={{{30,355,FPDF_FORMFIELD_TEXTFIELD},
      {30,305,FPDF_FORMFIELD_CHECKBOX},{80,305,FPDF_FORMFIELD_RADIOBUTTON},
      {140,305,FPDF_FORMFIELD_PUSHBUTTON},{30,255,FPDF_FORMFIELD_COMBOBOX},
      {230,260,FPDF_FORMFIELD_LISTBOX},{390,390,-1}}};
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture()); ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    int page_index=99; FPDF_ANNOTATION focused=nullptr;
    EXPECT_FALSE(api.FORM_GetFocusedAnnot(nullptr,&page_index,&focused));
    EXPECT_EQ(page_index,99);EXPECT_EQ(focused,nullptr);
    ASSERT_TRUE(api.FORM_GetFocusedAnnot(session.form,&page_index,&focused));
    EXPECT_EQ(page_index,-1); EXPECT_EQ(focused,nullptr);
    for(const auto& [x,y,type]:points) {
      EXPECT_EQ(api.FPDFPage_HasFormFieldAtPoint(session.form,session.page,x,y),type);
      const int order=api.FPDFPage_FormFieldZOrderAtPoint(session.form,session.page,x,y);
      if(type==-1) { EXPECT_EQ(order,-1); } else { EXPECT_GE(order,0); }
    }
    EXPECT_EQ(api.FPDFPage_HasFormFieldAtPoint(session.form,session.page,230,75),FPDF_FORMFIELD_TEXTFIELD);
    Handle annotation(api.FPDFPage_GetAnnot(session.page,0),api.FPDFPage_CloseAnnot);
    ASSERT_NE(annotation.get(),nullptr);
    ASSERT_TRUE(api.FORM_SetFocusedAnnot(session.form,annotation.get()));
    ASSERT_TRUE(api.FORM_SetFocusedAnnot(session.form,annotation.get()));
    ASSERT_TRUE(api.FORM_GetFocusedAnnot(session.form,&page_index,&focused));
    EXPECT_EQ(page_index,0); ASSERT_NE(focused,nullptr);
    Handle owned(focused,api.FPDFPage_CloseAnnot);
    EXPECT_EQ(api.FPDFPage_GetAnnotIndex(session.page,owned.get()),0);
    EXPECT_EQ(api.FPDFAnnot_GetSubtype(owned.get()),FPDF_ANNOT_WIDGET);
    ASSERT_TRUE(session.Focus(230,355));
    EXPECT_EQ(session.Text(),u"");
    ASSERT_TRUE(session.Focus(30,355,1));
    ASSERT_TRUE(api.FORM_GetFocusedAnnot(session.form,&page_index,&focused));
    EXPECT_EQ(page_index,1); ASSERT_NE(focused,nullptr);
    api.FPDFPage_CloseAnnot(focused);
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    api.FORM_GetFocusedAnnot(session.form,&page_index,&focused);
    EXPECT_EQ(page_index,-1); EXPECT_EQ(focused,nullptr);
    images[backend++]=session.Draw();
    EXPECT_TRUE(session.host->callbacks_valid);
  }
  CompareForms(images,artifacts,"focus-hits");
}

TEST(Forms, TextReadbackAllBufferSizesAndNoFocusStates) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture()); ASSERT_TRUE(session.valid());
    EXPECT_EQ(session.api.FORM_GetFocusedText(session.form,session.page,nullptr,0),2u);
    EXPECT_EQ(session.api.FORM_GetSelectedText(session.form,session.page,nullptr,0),2u);
    EXPECT_FALSE(session.api.FORM_SelectAllText(session.form,session.page));
    EXPECT_FALSE(session.api.FORM_Undo(session.form,session.page));
    EXPECT_FALSE(session.api.FORM_Redo(session.form,session.page));
    ASSERT_TRUE(session.Focus(30,355));
    CheckTextBuffers(session,false,u""); CheckTextBuffers(session,true,u"");
    session.Replace(u"Ab\u00e9\u03a9");
    CheckTextBuffers(session,false,u"Ab\u00e9\u03a9"); CheckTextBuffers(session,true,u"");
    ASSERT_TRUE(session.api.FORM_SelectAllText(session.form,session.page));
    CheckTextBuffers(session,true,u"Ab\u00e9\u03a9");
    auto another=session.host->Page(1);
    EXPECT_EQ(session.api.FORM_GetFocusedText(session.form,another,nullptr,0),2u);
    ASSERT_TRUE(session.api.FORM_ForceToKillFocus(session.form));
    session.Click(30,305);
    EXPECT_EQ(session.api.FORM_GetFocusedText(session.form,session.page,nullptr,0),2u);
  }
}

TEST(Forms, ReplacementsSelectionsCaretPositionsAndEmptyStringsRenderAndPersist) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture()); ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    ASSERT_TRUE(session.Focus(30,355));
    EXPECT_FALSE(api.FORM_CanUndo(session.form,session.page));
    EXPECT_FALSE(api.FORM_CanRedo(session.form,session.page));
    for(char character:std::string("ABCDE")) ASSERT_TRUE(api.FORM_OnChar(session.form,session.page,character,0));
    EXPECT_EQ(session.Text(),u"ABCDE");
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));
    session.Replace(u"\u00e9Z",true);
    EXPECT_EQ(session.Text(),u"\u00e9Z"); EXPECT_EQ(session.Text(true),u"\u00e9Z");
    session.Replace(u"",true);
    EXPECT_EQ(session.Text(),u""); EXPECT_EQ(session.Text(true),u"");
    session.Replace(u"middle");
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Home,0);
    session.Replace(u"[");
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_End,0);
    session.Replace(u"]");
    EXPECT_EQ(session.Text(),u"[middle]");
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Home,0);
    for(int i=0;i<3;++i) api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey);
    EXPECT_EQ(session.Text(true),u"[mi");
    session.Replace(u"X");
    EXPECT_EQ(session.Text(),u"Xddle]"); EXPECT_EQ(session.Text(true),u"");
    images[backend++]=session.Draw();
    forms::ExpectInk(images[backend-1],25,32,175,56);
    CheckSavedField(*library,session,artifacts,library->name()+"-replace",u"Xddle]");
  }
  CompareForms(images,artifacts,"replacements-focused");
}

TEST(Forms, UndoRedoHistoryBranchesAndIndependentWidgets) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture()); ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    ASSERT_TRUE(session.Focus(30,355));
    for(char c:std::string("ABC")) ASSERT_TRUE(api.FORM_OnChar(session.form,session.page,c,0));
    EXPECT_TRUE(api.FORM_CanUndo(session.form,session.page));
    for(const auto& expected:{u"AB",u"A",u""}) {
      ASSERT_TRUE(api.FORM_Undo(session.form,session.page)); EXPECT_EQ(session.Text(),expected);
      EXPECT_TRUE(api.FORM_CanRedo(session.form,session.page));
    }
    EXPECT_FALSE(api.FORM_Undo(session.form,session.page));
    EXPECT_FALSE(api.FORM_CanUndo(session.form,session.page));
    for(const auto& expected:{u"A",u"AB",u"ABC"}) { ASSERT_TRUE(api.FORM_Redo(session.form,session.page)); EXPECT_EQ(session.Text(),expected); }
    EXPECT_FALSE(api.FORM_Redo(session.form,session.page)); EXPECT_FALSE(api.FORM_CanRedo(session.form,session.page));
    ASSERT_TRUE(api.FORM_Undo(session.form,session.page));
    ASSERT_TRUE(api.FORM_OnChar(session.form,session.page,'Z',0));
    EXPECT_EQ(session.Text(),u"ABZ"); EXPECT_FALSE(api.FORM_CanRedo(session.form,session.page));
    ASSERT_TRUE(session.Focus(230,355));
    EXPECT_FALSE(api.FORM_CanUndo(session.form,session.page));
    ASSERT_TRUE(api.FORM_OnChar(session.form,session.page,'Q',0));
    ASSERT_TRUE(api.FORM_Undo(session.form,session.page)); EXPECT_EQ(session.Text(),u"");
    ASSERT_TRUE(api.FORM_Redo(session.form,session.page)); EXPECT_EQ(session.Text(),u"Q");
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    EXPECT_EQ(session.Field(0),u"ABZ"); EXPECT_EQ(session.Field(1),u"Q");
    EXPECT_FALSE(api.FORM_Redo(session.form,session.page));
    images[backend++]=session.Draw();
  }
  CompareForms(images,artifacts,"history-committed");
}

TEST(Forms, KeyboardModifiersNavigationReservedKeyUpAndNoFocus) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture({},"Alpha beta")); ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    EXPECT_FALSE(api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Delete,0));
    ASSERT_TRUE(session.Focus(30,355));
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Home,0);
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey);
    EXPECT_EQ(session.Text(true),u"A");
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_End,FWL_EVENTFLAG_ShiftKey);
    EXPECT_EQ(session.Text(true),u"Alpha beta");
    for(int flags:std::array<int,3>{0,FWL_EVENTFLAG_ShiftKey,FWL_EVENTFLAG_ControlKey|FWL_EVENTFLAG_KeyPad|FWL_EVENTFLAG_AutoRepeat}) {
      const auto before=session.Text();
      EXPECT_FALSE(api.FORM_OnKeyUp(session.form,session.page,FWL_VKEY_Left,flags));
      EXPECT_EQ(session.Text(),before);
    }
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_End,0);
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Back,0);
    api.FORM_OnChar(session.form,session.page,'\b',0);
    EXPECT_EQ(session.Text(),u"Alpha bet");
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Home,0);
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Delete,0);
    EXPECT_EQ(session.Text(),u"lpha bet");
    ASSERT_TRUE(api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Tab,0));
    EXPECT_EQ(session.Text(),u"");
    ASSERT_TRUE(api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Tab,FWL_EVENTFLAG_ShiftKey));
    EXPECT_EQ(session.Text(),u"lpha bet");
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    images[backend++]=session.Draw();
  }
  CompareForms(images,artifacts,"keyboard-navigation");
}

TEST(Forms, SingleLineReturnCommitsAndEscapeCancelsPendingInput) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> cancelled,committed;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture({},"saved"));ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    session.Click(30,355);
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));session.Replace(u"pending");
    EXPECT_EQ(session.Text(),u"pending");
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Escape,0);
    api.FORM_OnChar(session.form,session.page,FWL_VKEY_Escape,0);
    EXPECT_EQ(session.Field(0),u"saved");
    cancelled[backend]=session.Draw();
    session.Click(30,355);
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));session.Replace(u"committed");
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Return,0);
    api.FORM_OnChar(session.form,session.page,FWL_VKEY_Return,0);
    EXPECT_EQ(session.Field(0),u"committed");
    api.FORM_ForceToKillFocus(session.form);committed[backend++]=session.Draw();
    CheckSavedField(*library,session,artifacts,library->name()+"-return-commit",u"committed");
  }
  CompareForms(cancelled,artifacts,"escape-cancels");CompareForms(committed,artifacts,"return-commits");
}

TEST(Forms, ReadonlyMultilinePasswordCombAndMaximumLengthRespectLimits) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<std::vector<Raster>,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture()); ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    session.Focus(30,155);
    EXPECT_EQ(session.Field(7),u"Fixed");
    api.FORM_OnChar(session.form,session.page,'X',0);
    EXPECT_EQ(session.Field(7),u"Fixed");
    api.FORM_SelectAllText(session.form,session.page);
    session.Replace(u"Changed"); EXPECT_EQ(session.Field(7),u"Fixed");
    session.Replace(u"Changed",true); EXPECT_EQ(session.Field(7),u"Fixed");
    ASSERT_TRUE(session.Focus(30,90));
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));
    session.Replace(u"One\rTwo\rThree",true);
    EXPECT_EQ(session.Text(),u"One\r\nTwo\r\nThree");
    EXPECT_EQ(session.Text(true),session.Text());
    api.FORM_SelectAllText(session.form,session.page);
    EXPECT_EQ(session.Text(true),session.Text());
    images[backend].push_back(session.Draw());
    ASSERT_TRUE(session.Focus(230,155));
    EXPECT_EQ(session.Text(),u"secret");
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));
    session.Replace(u"123456789",true); EXPECT_EQ(session.Text(),u"123456");
    EXPECT_EQ(session.Text(true),u"123456");
    // Native ReplaceAndKeepSelection updates sel_state_ after InsertText but
    // omits SetCaretInfo; refresh through the public selection API before draw.
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));
    images[backend].push_back(session.Draw());
    ASSERT_TRUE(session.Focus(230,115));
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));
    session.Replace(u"ABCDEF123",true); EXPECT_EQ(session.Text(),u"ABCDEF");
    EXPECT_EQ(session.Text(true),u"ABCDEF");
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));
    images[backend].push_back(session.Draw());
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    ++backend;
  }
  for(size_t i=0;i<images[0].size();++i) CompareFormRaster(images[0][i],images[1][i],artifacts,"special-text-"+std::to_string(i));
}

TEST(Forms, MouseDragDoubleClickButtonPressReleaseAndUriFallback) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<std::vector<Raster>,2> images;
  std::array<std::vector<int>,2> values;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture({},"Alpha beta")); ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    EXPECT_TRUE(api.FORM_OnMouseMove(session.form,session.page,0,30,355));
    session.Click(30,355);
    api.FORM_OnMouseMove(session.form,session.page,0,42,355);
    api.FORM_OnLButtonDoubleClick(session.form,session.page,0,42,355);
    EXPECT_FALSE(session.Text(true).empty());
    const auto selected=session.Text(true);
    EXPECT_NE(std::u16string(u"Alpha beta").find(selected),std::u16string::npos);
    ASSERT_TRUE(api.FORM_OnLButtonDown(session.form,session.page,0,25,355));
    api.FORM_OnMouseMove(session.form,session.page,FWL_EVENTFLAG_LeftButtonDown,90,355);
    api.FORM_OnLButtonUp(session.form,session.page,0,90,355);
    EXPECT_FALSE(session.Text(true).empty());
    images[backend].push_back(session.Draw());
    session.Click(30,305);
    values[backend].push_back(session.Field(2)==u"Yes");
    EXPECT_EQ(session.Field(2),u"Yes");
    images[backend].push_back(session.Draw());
    session.Click(80,305); EXPECT_EQ(session.Field(3),u"Yes");
    images[backend].push_back(session.Draw());
    api.FORM_OnLButtonDown(session.form,session.page,0,140,305);
    images[backend].push_back(session.Draw());
    api.FORM_OnLButtonUp(session.form,session.page,0,140,305);
    images[backend].push_back(session.Draw());
    const int modifiers=FWL_EVENTFLAG_ControlKey|FWL_EVENTFLAG_ShiftKey;
    session.Click(250,35,modifiers);
    ASSERT_FALSE(session.host->uris.empty()); EXPECT_EQ(session.host->uris.back(),"https://example.invalid/form");
    // Widget action dispatch has no keyboard-modifier payload. The v2
    // callback takes precedence; the legacy callback is exercised below.
    EXPECT_EQ(session.host->uri_modifiers.back(),0);
    session.host->FFI_DoURIActionWithKeyboardModifier=nullptr;
    session.Click(250,35,modifiers);
    EXPECT_EQ(session.host->uri_modifiers.back(),0);
    const auto before_right=session.Field(0);
    const auto menus=session.host->popup_events;
    api.FORM_OnRButtonDown(session.form,session.page,0,30,355);
    api.FORM_OnRButtonUp(session.form,session.page,0,30,355);
    EXPECT_EQ(session.Field(0),before_right);
    EXPECT_EQ(session.host->popup_events,menus);
    ASSERT_TRUE(session.Focus(30,355));
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));
    const std::u16string unicode=u"caf\u00e9, na\u00efve";
    session.Replace(unicode);
    for(int flags:std::array<int,3>{0,FWL_EVENTFLAG_ShiftKey,FWL_EVENTFLAG_ControlKey}) {
      ASSERT_TRUE(session.Focus(30,355));
      api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_End,0);
      api.FORM_OnLButtonDoubleClick(session.form,session.page,flags,40,355);
      EXPECT_FALSE(session.Text(true).empty());
      EXPECT_NE(unicode.find(session.Text(true)),std::u16string::npos);
      EXPECT_EQ(session.Text(),unicode);
      api.FORM_OnLButtonDoubleClick(session.form,session.page,flags,20,355);
      EXPECT_EQ(session.Text(),unicode);
      EXPECT_FALSE(api.FORM_OnLButtonDoubleClick(session.form,session.page,flags,390,390));
      EXPECT_EQ(session.Text(),u"");
      EXPECT_EQ(session.Field(0),unicode);
    }
    EXPECT_FALSE(api.FORM_ForceToKillFocus(session.form));
    images[backend].push_back(session.Draw());
    EXPECT_TRUE(session.host->callbacks_valid);
    ++backend;
  }
  EXPECT_EQ(values[0],values[1]);
  for(size_t i=0;i<images[0].size();++i) CompareFormRaster(images[0][i],images[1][i],artifacts,"mouse-state-"+std::to_string(i));
}

TEST(Forms, ChoiceSelectionExportsScrollWheelAndPersistedValues) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  std::array<std::vector<Raster>,2> wheel_images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture()); ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    ASSERT_TRUE(session.Focus(230,260));
    for(int index:{0,4,9}) {
      ASSERT_TRUE(api.FORM_SetIndexSelected(session.form,session.page,index,1));
      EXPECT_TRUE(api.FORM_IsIndexSelected(session.form,session.page,index));
    }
    ASSERT_TRUE(api.FORM_SetIndexSelected(session.form,session.page,4,0));
    EXPECT_FALSE(api.FORM_IsIndexSelected(session.form,session.page,4));
    EXPECT_TRUE(api.FORM_IsIndexSelected(session.form,session.page,0));
    EXPECT_TRUE(api.FORM_IsIndexSelected(session.form,session.page,9));
    for(int index:{-1,10,99}) {
      EXPECT_FALSE(api.FORM_SetIndexSelected(session.form,session.page,index,1));
      EXPECT_FALSE(api.FORM_IsIndexSelected(session.form,session.page,index));
    }
    FS_POINTF inside{230,230}, outside{390,390};
    wheel_images[backend].push_back(session.Draw());
    int caret=4,anchor=-1;
    std::set<int> expected_selected{0,9};
    const auto wheel=[&](int flags,int dx,int dy) {
      api.FORM_OnMouseWheel(session.form,session.page,flags,&inside,dx,dy);
      // CPWL_ListBox maps the normalized wheel sign to list keyboard
      // navigation; Control moves only the caret and Shift extends its anchor.
      const int next=caret+(dy<0?1:-1);
      if(next>=0 && next<10) {
        if(!(flags&FWL_EVENTFLAG_ControlKey)) {
          expected_selected.clear();
          if(flags&FWL_EVENTFLAG_ShiftKey) {
            for(int i=std::min(anchor,next);i<=std::max(anchor,next);++i)
              if(i>=0) expected_selected.insert(i);
          } else { expected_selected.insert(next);anchor=next; }
        }
        caret=next;
      }
      for(int i=0;i<10;++i)
        EXPECT_EQ(!!api.FORM_IsIndexSelected(session.form,session.page,i),expected_selected.count(i)!=0);
    };
    for(int flags:std::array<int,3>{0,FWL_EVENTFLAG_ControlKey,FWL_EVENTFLAG_ShiftKey}) {
      for(const auto delta:{std::array<int,2>{0,0},{0,1},{0,-1},{1,0},{-1,0},{0,4},{0,-4}}) {
        wheel(flags,delta[0],delta[1]);
        wheel_images[backend].push_back(session.Draw());
      }
    }
    api.FORM_OnMouseWheel(session.form,session.page,0,&outside,0,-1);
    for(int delta:{400,-400}) {
      for(int step=0;step<12;++step) wheel(0,0,delta);
      auto limit=session.Draw();
      wheel(0,0,delta);
      EXPECT_EQ(session.Draw().pixels,limit.pixels)<<"Further wheel movement must clamp at the list limit";
      wheel_images[backend].push_back(std::move(limit));
    }
    EXPECT_TRUE(std::any_of(wheel_images[backend].begin()+1,wheel_images[backend].end(),[&](const Raster& frame) {
      return frame.pixels!=wheel_images[backend].front().pixels;
    }))<<"The long list must actually scroll";
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    ASSERT_TRUE(session.Focus(30,255));
    for(int index:{0,1,2}) {
      ASSERT_TRUE(api.FORM_SetIndexSelected(session.form,session.page,index,1));
      EXPECT_TRUE(api.FORM_IsIndexSelected(session.form,session.page,index));
      EXPECT_EQ(session.Text(),index==0?u"Red":index==1?u"Green":u"Blue");
    }
    EXPECT_FALSE(api.FORM_SetIndexSelected(session.form,session.page,2,0));
    EXPECT_TRUE(api.FORM_IsIndexSelected(session.form,session.page,2));
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    EXPECT_EQ(session.Field(5),u"b");
    images[backend++]=session.Draw();
    CheckSavedField(*library,session,artifacts,library->name()+"-choice",u"b",5);
    ASSERT_TRUE(session.Focus(30,355));
    EXPECT_FALSE(api.FORM_IsIndexSelected(session.form,session.page,0));
    EXPECT_FALSE(api.FORM_SetIndexSelected(session.form,session.page,0,1));
  }
  CompareForms(images,artifacts,"choices");
  ASSERT_EQ(wheel_images[0].size(),wheel_images[1].size());
  for(size_t i=0;i<wheel_images[0].size();++i)
    CompareFormRaster(wheel_images[0][i],wheel_images[1][i],artifacts,"choice-wheel-"+std::to_string(i));
}

TEST(Forms, HighlightColorAlphaRemovalAndEnvironmentIsolationActuallyRedraw) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<std::vector<Raster>,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture()); ASSERT_TRUE(session.valid());
    Session independent(library->api(),Fixture()); ASSERT_TRUE(independent.valid());
    const auto& api=session.api;
    const auto baseline=session.Draw();
    const auto other=independent.Draw();
    for(int alpha:{0,96,255}) {
      api.FPDF_SetFormFieldHighlightColor(session.form,FPDF_FORMFIELD_UNKNOWN,0x0030b060);
      api.FPDF_SetFormFieldHighlightAlpha(session.form,static_cast<unsigned char>(alpha));
      const auto image=session.Draw();
      images[backend].push_back(image);
      if(alpha==0) { EXPECT_EQ(image.pixels,baseline.pixels); }
      else { EXPECT_NE(image.pixels,baseline.pixels); }
      EXPECT_EQ(independent.Draw().pixels,other.pixels);
      // An unrelated page corner must not be recolored with widget interiors.
      EXPECT_EQ(std::vector<uint8_t>(image.pixels.begin(),image.pixels.begin()+4),
                std::vector<uint8_t>(baseline.pixels.begin(),baseline.pixels.begin()+4));
    }
    api.FPDF_SetFormFieldHighlightColor(session.form,FPDF_FORMFIELD_TEXTFIELD,0x00e04020);
    images[backend].push_back(session.Draw());
    api.FPDF_RemoveFormFieldHighlight(session.form);
    EXPECT_EQ(session.Draw().pixels,baseline.pixels);
    EXPECT_EQ(session.Draw().pixels,baseline.pixels);
    api.FPDF_SetFormFieldHighlightColor(session.form,FPDF_FORMFIELD_TEXTFIELD,0x0030b060);
    api.FPDF_SetFormFieldHighlightAlpha(session.form,96);
    EXPECT_NE(session.Draw().pixels,baseline.pixels);
    ++backend;
  }
  for(size_t i=0;i<images[0].size();++i) CompareFormRaster(images[0][i],images[1][i],artifacts,"highlight-"+std::to_string(i));
}

TEST(Forms, OverlayDrawingRotationsViewportsClippingAndFlags) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<std::vector<Raster>,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture({},"Visible")); ASSERT_TRUE(session.valid());
    ASSERT_TRUE(session.Focus(30,355));
    ASSERT_TRUE(session.api.FORM_SelectAllText(session.form,session.page));
    for(int rotation=0;rotation<4;++rotation) {
      for(int flags:{0,FPDF_ANNOT,FPDF_GRAYSCALE,FPDF_PRINTING,FPDF_REVERSE_BYTE_ORDER}) {
        images[backend].push_back(session.Draw(300,280,rotation,flags,12,8,260,250));
        EXPECT_EQ(std::vector<uint8_t>(images[backend].back().pixels.begin(),images[backend].back().pixels.begin()+4),
                  (std::vector<uint8_t>{255,255,255,255}));
      }
      images[backend].push_back(session.Draw(220,180,rotation,0,-20,-10,350,300));
    }
    EXPECT_GT(session.host->focus_values.size(),0u);
    ++backend;
  }
  ASSERT_EQ(images[0].size(),images[1].size());
  for(size_t i=0;i<images[0].size();++i) {
    const auto label="viewport-"+std::to_string(i);
    if(i>=18 && i<=22) {
      // At rotation3/fractional scale, three pixels at the V's tip have
      // hint/coverage deltas96,106,31. The enlarged25x14 ROI confirms the
      // same glyph contour/position. Confine this exception to that glyph;
      // every other pixel retains the ordinary no-outlier AA bound.
      const auto& expected=images[0][i];const auto& actual=images[1][i];
      const std::array<std::array<int,4>,4> surround={{{0,0,300,238},{0,252,300,28},{0,238,30,14},{55,238,245,14}}};
      for(size_t r=0;r<surround.size();++r) {
        const auto& box=surround[r];
        CompareRaster(CropRaster(expected,box[0],box[1],box[2],box[3]),
                      CropRaster(actual,box[0],box[1],box[2],box[3]),artifacts,
                      label+"-surround"+std::to_string(r),RasterTolerance{18,0,0.08});
      }
      CompareRaster(CropRaster(expected,30,238,25,14),CropRaster(actual,30,238,25,14),
                    artifacts,label+"-glyph-tip",RasterTolerance{18,3.0/(25*14),0.6});
    } else CompareFormRaster(images[0][i],images[1][i],artifacts,label);
  }
}

TEST(Forms, TextDirectionValidEnumRenderingAndNonpersistentSettings) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<std::vector<Raster>,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture()); ASSERT_TRUE(session.valid());
    Handle annotation(session.api.FPDFPage_GetAnnot(session.page,0),session.api.FPDFPage_CloseAnnot);
    ASSERT_NE(annotation.get(),nullptr);
    EXPECT_EQ(session.api.FORM_GetTextDirection(session.form,annotation.get()),FPDF_TEXTDIR_AUTO);
    ASSERT_TRUE(session.Focus(30,355));
    session.Replace(u"abc \u05d0\u05d1\u05d2");
    for(auto direction:{FPDF_TEXTDIR_AUTO,FPDF_TEXTDIR_LTR,FPDF_TEXTDIR_RTL,FPDF_TEXTDIR_AUTO}) {
      ASSERT_TRUE(session.api.FORM_SetTextDirection(session.form,annotation.get(),direction));
      EXPECT_EQ(session.api.FORM_GetTextDirection(session.form,annotation.get()),direction);
      images[backend].push_back(session.Draw());
    }
    EXPECT_FALSE(session.api.FORM_SetTextDirection(session.form,annotation.get(),FPDF_TEXTDIR_UNKNOWN));
    EXPECT_EQ(session.api.FORM_GetTextDirection(session.form,annotation.get()),FPDF_TEXTDIR_AUTO);
    EXPECT_EQ(session.api.FORM_GetTextDirection(nullptr,annotation.get()),FPDF_TEXTDIR_UNKNOWN);
    EXPECT_EQ(session.api.FORM_GetTextDirection(session.form,nullptr),FPDF_TEXTDIR_UNKNOWN);
    ASSERT_TRUE(session.api.FORM_SetTextDirection(session.form,annotation.get(),FPDF_TEXTDIR_RTL));
    ASSERT_TRUE(session.api.FORM_ForceToKillFocus(session.form));
    const auto bytes=SaveDocument(session.api,session.document,artifacts,library->name()+"-direction");
    for(const auto* reader:Implementations()) {
      Session reopened(reader->api(),bytes); ASSERT_TRUE(reopened.valid());
      Handle item(reopened.api.FPDFPage_GetAnnot(reopened.page,0),reopened.api.FPDFPage_CloseAnnot);
      ASSERT_NE(item.get(),nullptr);
      EXPECT_EQ(reopened.api.FORM_GetTextDirection(reopened.form,item.get()),FPDF_TEXTDIR_AUTO);
      EXPECT_EQ(reopened.Field(0),u"abc \u05d0\u05d1\u05d2");
    }
    ++backend;
  }
  for(size_t i=0;i<images[0].size();++i) CompareFormRaster(images[0][i],images[1][i],artifacts,"direction-"+std::to_string(i));
}

TEST(Forms, OverlappingWidgetsZOrderTargetingAndReloadAgreeWithDrawing) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> initial,edited;
  size_t backend=0;
  const auto fixture=PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << /Font << /Helv 5 0 R >> >> /Annots [6 0 R 7 0 R] >>",
    "<< /Fields [6 0 R 7 0 R] /NeedAppearances true /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    "<< /Type /Annot /Subtype /Widget /P 3 0 R /FT /Tx /T (back) /V (Back) /Rect [20 340 180 370] /MK << /BG [1 0.8 0.8] /BC [0 0 0] >> /F 4 >>",
    "<< /Type /Annot /Subtype /Widget /P 3 0 R /FT /Tx /T (front) /V (Front) /Rect [100 340 260 370] /MK << /BG [0.8 1 0.8] /BC [0 0 0] >> /F 4 >>"});
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),fixture);ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    const int back=api.FPDFPage_FormFieldZOrderAtPoint(session.form,session.page,50,355);
    const int front=api.FPDFPage_FormFieldZOrderAtPoint(session.form,session.page,230,355);
    EXPECT_GE(back,0);EXPECT_GT(front,back);
    EXPECT_EQ(api.FPDFPage_FormFieldZOrderAtPoint(session.form,session.page,150,355),front);
    EXPECT_EQ(api.FPDFPage_FormFieldZOrderAtPoint(session.form,session.page,390,390),-1);
    initial[backend]=session.Draw();
    // Native input iteration uses drawing order and targets the covered back
    // widget. Rust must agree with the visible/front z-order query instead.
    session.Click(150,355);
    EXPECT_EQ(session.Text(),library->name()=="native"?u"Back":u"Front");
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    // An exclusive front-widget point makes the subsequent edit identical
    // for both writers, retaining full visual and four-way save comparisons.
    session.Click(230,355);EXPECT_EQ(session.Text(),u"Front");
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));session.Replace(u"Target");
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    EXPECT_EQ(session.Field(0),u"Back");EXPECT_EQ(session.Field(1),u"Target");
    edited[backend]=session.Draw();
    CheckSavedField(*library,session,artifacts,library->name()+"-overlap",u"Target",1);
    session.ClosePage(0);session.page=session.host->Page(0);ASSERT_NE(session.page,nullptr);
    EXPECT_EQ(api.FPDFPage_FormFieldZOrderAtPoint(session.form,session.page,150,355),front);
    ++backend;
  }
  CompareForms(initial,artifacts,"overlap-before");CompareForms(edited,artifacts,"overlap-targeted");
}

TEST(Forms, EditableComboUnicodeBuffersSelectionHistoryAndMultilineSpans) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture());ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    ASSERT_TRUE(session.Focus(30,255));
    CheckTextBuffers(session,false,u"Green");
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Home,0);
    for(int i=0;i<3;++i) api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey);
    CheckTextBuffers(session,true,u"Gre");
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Right,0);
    for(int i=0;i<2;++i) api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Left,FWL_EVENTFLAG_ShiftKey);
    CheckTextBuffers(session,true,u"re");
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));CheckTextBuffers(session,true,u"Green");
    session.Replace(u"caf\u00e9",true);CheckTextBuffers(session,false,u"caf\u00e9");CheckTextBuffers(session,true,u"caf\u00e9");
    session.Replace(u"Z");EXPECT_EQ(session.Text(),u"Z");EXPECT_EQ(session.Text(true),u"");
    ASSERT_TRUE(api.FORM_CanUndo(session.form,session.page));ASSERT_TRUE(api.FORM_Undo(session.form,session.page));
    EXPECT_EQ(session.Text(),u"caf\u00e9");ASSERT_TRUE(api.FORM_Redo(session.form,session.page));EXPECT_EQ(session.Text(),u"Z");
    api.FORM_OnChar(session.form,session.page,0x00e9,0);EXPECT_EQ(session.Text(),u"Z\u00e9");
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));EXPECT_EQ(session.Field(5),u"Z\u00e9");
    ASSERT_TRUE(session.Focus(30,90));api.FORM_SelectAllText(session.form,session.page);
    session.Replace(u"One\rTwo\rThree");api.FORM_SelectAllText(session.form,session.page);
    CheckTextBuffers(session,false,u"One\r\nTwo\r\nThree");CheckTextBuffers(session,true,u"One\r\nTwo\r\nThree");
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey|FWL_EVENTFLAG_ShiftKey);
    EXPECT_EQ(session.Text(true),session.Text());
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    images[backend++]=session.Draw();
    CheckSavedField(*library,session,artifacts,library->name()+"-combo-unicode",u"Z\u00e9",5);
  }
  CompareForms(images,artifacts,"combo-unicode-multiline");
}

TEST(FormActions, PointerEnterExitPressReleaseAndKeyboardButtonsHaveObservableEffects) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Actions actions;
    actions.enter="this.getField('second').value+='E';";
    actions.exit="this.getField('second').value+='X';";
    actions.down="this.getField('second').value+='D';";
    actions.up="this.getField('second').value+='U';";
    actions.button="this.getField('second').value+='B';";
    Session session(library->api(),Fixture(actions),true);ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    api.FORM_OnMouseMove(session.form,session.page,0,30,355);EXPECT_EQ(session.Field(1),u"E");
    api.FORM_OnMouseMove(session.form,session.page,0,40,355);EXPECT_EQ(session.Field(1),u"E");
    api.FORM_OnLButtonDown(session.form,session.page,0,40,355);EXPECT_EQ(session.Field(1),u"ED");
    api.FORM_OnLButtonUp(session.form,session.page,0,40,355);EXPECT_EQ(session.Field(1),u"EDU");
    api.FORM_OnMouseMove(session.form,session.page,0,390,390);EXPECT_EQ(session.Field(1),u"EDUX");
    api.FORM_ForceToKillFocus(session.form);
    api.FORM_OnMouseMove(session.form,session.page,0,140,305);
    api.FORM_OnLButtonDown(session.form,session.page,0,140,305);EXPECT_EQ(session.Field(1),u"EDUX");
    api.FORM_OnLButtonUp(session.form,session.page,0,140,305);EXPECT_EQ(session.Field(1),u"EDUXB");
    ASSERT_TRUE(session.Focus(30,305));api.FORM_OnChar(session.form,session.page,' ',0);EXPECT_EQ(session.Field(2),u"Yes");
    api.FORM_OnChar(session.form,session.page,' ',0);EXPECT_EQ(session.Field(2),u"Off");
    ASSERT_TRUE(session.Focus(80,305));api.FORM_OnChar(session.form,session.page,' ',0);EXPECT_EQ(session.Field(3),u"Yes");
    api.FORM_ForceToKillFocus(session.form);images[backend++]=session.Draw();
    EXPECT_GT(session.host->changes,0);EXPECT_GT(session.host->cursors.size(),0u);
  }
  CompareForms(images,artifacts,"mouse-action-keyboard-state");
}

TEST(FormActions, OrderedDocumentScriptsErrorsOpenAndPageActionsRemainUsable) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Actions actions;
    actions.catalog="/Names << /JavaScript << /Names [(a) "+forms::Js("var order='A';this.getField('input').value=order;")+" (b) "+
        forms::Js("order+='B';throw 'deliberate script error';")+" (c) "+
        forms::Js("order+='C';this.getField('input').value=order;")+"] >> >>";
    actions.open="order+='O';this.getField('input').value=order;";
    actions.page_open="this.pageNum=1;this.getField('later').value='opened';this.pageNum=0;order+='P';this.getField('input').value=order;";
    actions.page_close="order+='C';this.getField('input').value=order;";
    Session session(library->api(),Fixture(actions),true); ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    EXPECT_EQ(session.Field(0),u"");
    api.FORM_DoDocumentJSAction(session.form);
    EXPECT_EQ(session.Field(0),u"ABC");
    api.FORM_DoDocumentOpenAction(session.form);
    EXPECT_EQ(session.Field(0),u"ABCO");
    api.FORM_DoPageAAction(session.page,session.form,FPDFPAGE_AACTION_OPEN);
    EXPECT_EQ(session.Field(0),u"ABCOP");
    EXPECT_EQ(session.Field(0,1),u"opened");
    EXPECT_NE(session.host->pages[1],nullptr);
    api.FORM_DoPageAAction(session.page,session.form,FPDFPAGE_AACTION_CLOSE);
    EXPECT_EQ(session.Field(0),u"ABCOPC");
    images[backend++]=session.Draw();
    Session disabled(api,Fixture(actions),false); ASSERT_TRUE(disabled.valid());
    api.FORM_DoDocumentJSAction(disabled.form); api.FORM_DoDocumentOpenAction(disabled.form);
    api.FORM_DoPageAAction(disabled.page,disabled.form,FPDFPAGE_AACTION_OPEN);
    EXPECT_EQ(disabled.Field(0),u"");
  }
  CompareForms(images,artifacts,"script-action-order");
}

TEST(FormActions, EachDocumentAdditionalActionIsExplicitAndSaveKeepsResult) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Actions actions;
    actions.catalog="/AA << ";
    const std::array<std::pair<const char*,int>,5> kinds={{{"WC",FPDFDOC_AACTION_WC},
        {"WS",FPDFDOC_AACTION_WS},{"DS",FPDFDOC_AACTION_DS},{"WP",FPDFDOC_AACTION_WP},
        {"DP",FPDFDOC_AACTION_DP}}};
    for(const auto& [name,kind]:kinds) {
      (void)kind;
      actions.catalog+="/"+std::string(name)+" "+forms::Js("this.getField('input').value+= '"+std::string(name)+"';")+" ";
    }
    actions.catalog+=" >>";
    Session session(library->api(),Fixture(actions),true); ASSERT_TRUE(session.valid());
    std::u16string expected;
    for(const auto& [name,kind]:kinds) {
      session.api.FORM_DoDocumentAAction(session.form,kind);
      expected.push_back(name[0]);expected.push_back(name[1]);
      EXPECT_EQ(session.Field(0),expected);
      if(kind==FPDFDOC_AACTION_WS) {
        const auto bytes=SaveDocument(session.api,session.document,artifacts,library->name()+"-between-save-actions");
        EXPECT_EQ(session.Field(0),expected); // Save does not invent automatic action dispatch.
        for(const auto* reader:Implementations()) {
          Session reopened(reader->api(),bytes); ASSERT_TRUE(reopened.valid());
          EXPECT_EQ(reopened.Field(0),expected);
        }
      }
    }
    EXPECT_EQ(expected,u"WCWSDSWPDP");
    CheckSavedField(*library,session,artifacts,library->name()+"-additional-actions",expected);
    Session absent(library->api(),Fixture(),true); ASSERT_TRUE(absent.valid());
    for(const auto& [name,kind]:kinds) { (void)name; absent.api.FORM_DoDocumentAAction(absent.form,kind); }
    EXPECT_EQ(absent.Field(0),u"");
  }
}

TEST(FormActions, DestinationNamedAndUriOpenActionsUseHostPayloads) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::array<std::vector<int>,2> observations;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Actions destination;destination.catalog="/OpenAction << /S /GoTo /D [6 0 R /XYZ 12 25 1.5] >>";
    Session session(library->api(),Fixture(destination),true); ASSERT_TRUE(session.valid());
    session.api.FORM_DoDocumentOpenAction(session.form);
    ASSERT_EQ(session.host->destinations.size(),1u);
    const auto& [page,mode,position]=session.host->destinations.front();
    EXPECT_EQ(page,1);EXPECT_EQ(mode,1);
    EXPECT_EQ(position,(std::vector<float>{12,25,1.5f}));
    observations[backend].push_back(page);observations[backend].push_back(mode);
    Actions chain;chain.catalog="/OpenAction << /S /Named /N /NextPage /Next << /S /URI /URI (https://example.invalid/open) >> >>";
    Session chained(library->api(),Fixture(chain),true); ASSERT_TRUE(chained.valid());
    chained.api.FORM_DoDocumentOpenAction(chained.form);
    EXPECT_EQ(chained.host->named_actions,(std::vector<std::string>{"NextPage"}));
    // URI actions require a user-input action context; document-open
    // traversal executes Named and deliberately ignores the chained URI.
    EXPECT_TRUE(chained.host->uris.empty());
    Session none(library->api(),Fixture(),true); ASSERT_TRUE(none.valid());
    none.api.FORM_DoDocumentOpenAction(none.form);
    EXPECT_TRUE(none.host->destinations.empty());EXPECT_TRUE(none.host->named_actions.empty());
    ++backend;
  }
  EXPECT_EQ(observations[0],observations[1]);
}

TEST(FormActions, AlertResponseBeepUnicodeAndResponseCapacityUseRecordedCallbacks) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Actions actions;actions.document=R"(
      var answer=app.alert({cMsg:'Message\u00e9',cTitle:'Title\u03a9',nType:3,nIcon:2});
      var response;try{response=app.response({cQuestion:'Question\u00e9',cTitle:'Title',cDefault:'default',cLabel:'Label',bPassword:true});this.getField('input').value=String(response.length);}catch(e){this.getField('input').value='overflow';}
      this.getField('second').value=String(answer);app.beep(3);
    )";
    for(size_t reply_length:{size_t{0},size_t{5},size_t{1024},size_t{1025}}) {
      Session session(library->api(),Fixture(actions),true);ASSERT_TRUE(session.valid());
      session.host->response.assign(reply_length,u'\u00e9');
      session.host->reenter=true;
      session.api.FORM_DoDocumentJSAction(session.form);
      const auto digits=std::to_string(reply_length);
      const std::u16string expected=reply_length<=1024?std::u16string(digits.begin(),digits.end()):u"overflow";
      EXPECT_EQ(session.Field(0),expected);
      EXPECT_EQ(session.Field(1),u"4");
      ASSERT_EQ(session.host->alerts.size(),1u);
      EXPECT_EQ(session.host->alerts[0],(std::tuple<std::u16string,std::u16string,int,int>{u"Message\u00e9",u"Title\u03a9",3,2}));
      ASSERT_EQ(session.host->responses.size(),1u);
      EXPECT_EQ(session.host->responses[0],(std::array<std::u16string,4>{u"Question\u00e9",u"Title",u"default",u"Label"}));
      EXPECT_EQ(session.host->response_capacities,(std::vector<int>{2048}));
      EXPECT_EQ(session.host->response_passwords,(std::vector<int>{1}));
      EXPECT_EQ(session.host->beeps,(std::vector<int>{3}));
      EXPECT_TRUE(session.host->callbacks_valid);
      if(reply_length==5) images[backend]=session.Draw();
    }
    ++backend;
  }
  CompareForms(images,artifacts,"host-dialog-values");
}

TEST(FormActions, TimersFireRecordedCallbacksAndCancelBeforeHostStorageRelease) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Actions actions;
    actions.document=R"(var ticks=0;var repeating=app.setInterval("ticks++;this.getField('input').value=String(ticks);",20);var once=app.setTimeOut("ticks+=10;this.getField('second').value=String(ticks);",40);)";
    actions.open=R"(app.clearInterval(repeating);var pending=app.setInterval("this.getField('input').value='late';",100);)";
    actions.page_close="app.clearInterval(pending);";
    Session session(library->api(),Fixture(actions),true);ASSERT_TRUE(session.valid());
    session.api.FORM_DoDocumentJSAction(session.form);
    ASSERT_EQ(session.host->timers.size(),2u);
    const auto first=session.host->timers.begin()->first;
    const auto last=session.host->timers.rbegin()->first;
    session.host->Tick(first);session.host->Tick(first);
    EXPECT_EQ(session.Field(0),u"2");
    session.host->Tick(last);EXPECT_EQ(session.Field(1),u"12");
    session.host->Tick(last);EXPECT_EQ(session.Field(1),u"12");
    EXPECT_EQ(session.host->timers.size(),1u);
    images[backend++]=session.Draw();
    session.api.FORM_DoDocumentOpenAction(session.form);
    EXPECT_EQ(session.host->timers.size(),1u);
    EXPECT_NE(std::find(session.host->killed.begin(),session.host->killed.end(),first),session.host->killed.end());
    session.api.FORM_DoPageAAction(session.page,session.form,FPDFPAGE_AACTION_CLOSE);
    EXPECT_TRUE(session.host->timers.empty());
    session.Close();
    EXPECT_TRUE(session.host->timers.empty());EXPECT_EQ(session.host->releases,1);
  }
  CompareForms(images,artifacts,"timer-values");
}

TEST(FormActions, BrowserPathMailPrintSubmitAndPageNavigationStayInsideHost) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Actions actions;
    actions.catalog="/AA << /WP "+forms::Js("this.getField('second').value='before-print';")+
      " /DP "+forms::Js("this.getField('second').value+='-after-print';")+" >>";
    actions.document=R"(
      var savedPathForTest=this.path;this.getField('input').browseForFileToSubmit();
      this.getField('input').value=this.getField('input').value+'|'+savedPathForTest;
      this.pageNum=1;this.pageNum=0;
    )";
    actions.button=R"(
      this.print({bUI:false,nStart:0,nEnd:1,bSilent:true,bShrinkToFit:true,bPrintAsImage:false,bReverse:true,bAnnotations:true});
      this.mailDoc({bUI:false,cTo:'to@example.invalid',cCc:'cc',cBcc:'bcc',cSubject:'Subject',cMsg:'Message'});
      this.mailForm({bUI:false,cTo:'form@example.invalid',cCc:'',cBcc:'',cSubject:'Form',cMsg:'Data'});
      this.submitForm({cURL:'https://example.invalid/submit',bFDF:true,bEmpty:true});
    )";
    Session session(library->api(),Fixture(actions,"",1<<20),true);ASSERT_TRUE(session.valid());
    session.host->filepath="virtual/form.pdf";
    session.api.FORM_DoDocumentJSAction(session.form);
    session.api.FORM_DoDocumentAAction(session.form,FPDFDOC_AACTION_WP);
    EXPECT_EQ(session.Field(1),u"before-print");
    session.Click(140,305);
    EXPECT_EQ(session.Field(1),u"before-print")<<"The host dispatches print lifecycle actions explicitly";
    session.api.FORM_DoDocumentAAction(session.form,FPDFDOC_AACTION_DP);
    EXPECT_EQ(session.Field(1),u"before-print-after-print");
    EXPECT_EQ(session.Field(0),u"chosen.pdf|/virtual/form.pdf");
    EXPECT_GE(session.host->file_queries,1);EXPECT_GE(session.host->browse_queries,1);
    EXPECT_EQ(session.host->goto_pages,(std::vector<int>{1,0}));
    EXPECT_EQ(session.host->prints,(std::vector<std::array<int,8>>{{0,0,1,1,1,0,1,1}}));
    ASSERT_EQ(session.host->mails.size(),2u);
    EXPECT_EQ(session.host->mail_ui,(std::vector<int>{0,0}));
    EXPECT_EQ(session.host->mails[0],(std::array<std::u16string,5>{u"to@example.invalid",u"Subject",u"cc",u"bcc",u"Message"}));
    EXPECT_EQ(session.host->mails[1],(std::array<std::u16string,5>{u"form@example.invalid",u"Form",u"",u"",u"Data"}));
    ASSERT_EQ(session.host->mail_packets.size(),2u);
    EXPECT_TRUE(session.host->mail_packets[0].empty());
    EXPECT_FALSE(session.host->mail_packets[1].empty());
    const std::string attachment(session.host->mail_packets[1].begin(),session.host->mail_packets[1].end());
    EXPECT_NE(attachment.find("chosen.pdf"),std::string::npos);
    ASSERT_EQ(session.host->submit_urls.size(),1u);
    EXPECT_EQ(session.host->submit_urls[0],u"https://example.invalid/submit");
    ASSERT_EQ(session.host->submissions.size(),1u);EXPECT_FALSE(session.host->submissions[0].empty());
    const std::string payload(session.host->submissions[0].begin(),session.host->submissions[0].end());
    EXPECT_NE(payload.find("chosen.pdf"),std::string::npos);
    images[backend++]=session.Draw();
  }
  CompareForms(images,artifacts,"host-document-actions");
}

TEST(FormActions, KeystrokeCancellationValidationFormattingAndCalculationCommit) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Actions actions;
    actions.keystroke="this.getField('second').value=event.change;if(event.change==='x')event.rc=false;if(event.change==='a')event.change='A';";
    actions.validate="if(event.value==='bad')event.rc=false;";
    actions.format="event.value='['+event.value+']';";
    actions.calculate="event.value=Number(this.getField('input').value)*2;";
    Session session(library->api(),Fixture(actions,"",0,0,false),true);ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    ASSERT_TRUE(session.Focus(30,355));
    api.FORM_OnChar(session.form,session.page,'a',0);
    EXPECT_EQ(session.Field(1),u"a");
    // The pinned native CPDFSDK_Widget::OnAAction returns false after
    // executing ordinary AcroForm actions. Its caller then ignores rewritten
    // change/rc data. Keep asserting the correct edit semantics in Rust.
    const bool native=library->name()=="native";
    EXPECT_EQ(session.Text(),native?u"a":u"A");
    api.FORM_OnChar(session.form,session.page,'x',0);
    EXPECT_EQ(session.Field(1),u"x");
    EXPECT_EQ(session.Text(),native?u"ax":u"A");
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));
    session.Replace(u"bad");
    api.FORM_ForceToKillFocus(session.form);
    EXPECT_NE(session.Field(0),u"bad");
    ASSERT_TRUE(session.Focus(30,355));
    api.FORM_SelectAllText(session.form,session.page);session.Replace(u"7");
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    EXPECT_EQ(session.Field(0),u"7");EXPECT_EQ(session.Field(13),u"14");
    images[backend++]=session.Draw();
    CheckSavedField(*library,session,artifacts,library->name()+"-validated",u"7");
  }
  CompareForms(images,artifacts,"validated-formatted");
}

TEST(Forms, ReversedAndOutsideDragsAndOutsideButtonReleaseHaveRealEffects) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<std::vector<Raster>,2> images;
  std::array<std::vector<std::u16string>,2> selections;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Actions actions;actions.button="this.getField('second').value+='activated';";
    Session session(library->api(),Fixture(actions,"Alpha beta"),true);ASSERT_TRUE(session.valid());
    const auto& api=session.api;
    for(const auto& drag:std::array<std::array<double,2>,3>{{{25,175},{175,25},{25,390}}}) {
      SCOPED_TRACE(drag[1]);
      ASSERT_TRUE(session.Focus(30,355));
      api.FORM_OnMouseMove(session.form,session.page,0,drag[0],355);
      ASSERT_TRUE(api.FORM_OnLButtonDown(session.form,session.page,0,drag[0],355));
      api.FORM_OnMouseMove(session.form,session.page,FWL_EVENTFLAG_LeftButtonDown,drag[1],355);
      api.FORM_OnLButtonUp(session.form,session.page,0,drag[1],355);
      EXPECT_EQ(session.Text(),u"Alpha beta");
      // PageView dispatches movement to the widget under the pointer. A
      // direct jump outside supplies no intervening in-field drag positions.
      EXPECT_EQ(session.Text(true),drag[1]>180?u"":u"Alpha beta");
      selections[backend].push_back(session.Text(true));
      images[backend].push_back(session.Draw());
    }
    std::array<std::u16string,2> partial;
    for(size_t i=0;i<2;++i) {
      const double start=i?65:30,end=i?30:65;
      ASSERT_TRUE(api.FORM_OnLButtonDown(session.form,session.page,0,start,355));
      api.FORM_OnMouseMove(session.form,session.page,FWL_EVENTFLAG_LeftButtonDown,end,355);
      api.FORM_OnLButtonUp(session.form,session.page,0,end,355);
      partial[i]=session.Text(true);
      EXPECT_FALSE(partial[i].empty());EXPECT_LT(partial[i].size(),session.Text().size());
      selections[backend].push_back(partial[i]);
    }
    EXPECT_EQ(partial[0],partial[1]);
    ASSERT_FALSE(session.host->selections.empty());
    for(const auto& rect:session.host->selections) {
      EXPECT_TRUE(std::isfinite(rect[0]));EXPECT_TRUE(std::isfinite(rect[1]));
      EXPECT_TRUE(std::isfinite(rect[2]));EXPECT_TRUE(std::isfinite(rect[3]));
      EXPECT_GE(rect[0],20);EXPECT_LE(rect[2],180);
      EXPECT_GE(rect[3],340);EXPECT_LE(rect[1],370);
      EXPECT_LT(rect[0],rect[2]);EXPECT_GT(rect[1],rect[3]);
    }
    api.FORM_OnMouseMove(session.form,session.page,0,140,305);
    api.FORM_OnLButtonDown(session.form,session.page,0,140,305);
    api.FORM_OnMouseMove(session.form,session.page,FWL_EVENTFLAG_LeftButtonDown,390,390);
    api.FORM_OnLButtonUp(session.form,session.page,0,390,390);
    // Native dispatches the focused widget's primary action outside. Rust
    // requires release inside the active area, after cleaning up pressed state.
    EXPECT_EQ(session.Field(1),library->name()=="native"?u"activated":u"");
    images[backend].push_back(CropRaster(session.Draw(),110,70,100,50));
    session.Focus(230,355);api.FORM_SelectAllText(session.form,session.page);session.Replace(u"");
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    session.Click(140,305);EXPECT_EQ(session.Field(1),u"activated");
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    images[backend].push_back(session.Draw());
    Actions up_action;up_action.button_up="this.getField('second').value+='up';";
    Session explicit_up(api,Fixture(up_action),true);ASSERT_TRUE(explicit_up.valid());
    explicit_up.Click(140,305);EXPECT_EQ(explicit_up.Field(1),u"up");
    explicit_up.Focus(230,355);api.FORM_SelectAllText(explicit_up.form,explicit_up.page);explicit_up.Replace(u"");
    api.FORM_ForceToKillFocus(explicit_up.form);
    api.FORM_OnMouseMove(explicit_up.form,explicit_up.page,0,140,305);
    api.FORM_OnLButtonDown(explicit_up.form,explicit_up.page,0,140,305);
    api.FORM_OnMouseMove(explicit_up.form,explicit_up.page,FWL_EVENTFLAG_LeftButtonDown,390,390);
    api.FORM_OnLButtonUp(explicit_up.form,explicit_up.page,0,390,390);
    EXPECT_EQ(explicit_up.Field(1),library->name()=="native"?u"up":u"");
    images[backend++].push_back(CropRaster(explicit_up.Draw(),110,70,100,50));
  }
  EXPECT_EQ(selections[0],selections[1]);
  for(size_t i=0;i<images[0].size();++i)
    CompareFormRaster(images[0][i],images[1][i],artifacts,"outside-drag-release-"+std::to_string(i));
}

TEST(Forms, RotatedScaledPublicCoordinateConversionTargetsTheSameWidget) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<std::vector<Raster>,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture());ASSERT_TRUE(session.valid());const auto& api=session.api;
    api.FPDFPage_SetRotation(session.page,1);
    EXPECT_EQ(api.FPDFPage_GetRotation(session.page),1);
    for(const auto& edge:std::array<std::array<double,2>,4>{{{20,355},{180,355},{30,340},{30,370}}})
      EXPECT_EQ(api.FPDFPage_HasFormFieldAtPoint(session.form,session.page,edge[0],edge[1]),FPDF_FORMFIELD_TEXTFIELD);
    for(int rotation=0;rotation<4;++rotation) {
      SCOPED_TRACE(rotation);
      int device_x=-1,device_y=-1;double page_x=-1,page_y=-1;
      ASSERT_TRUE(api.FPDF_PageToDevice(session.page,13,17,800,800,rotation,30,355,&device_x,&device_y));
      ASSERT_TRUE(api.FPDF_DeviceToPage(session.page,13,17,800,800,rotation,device_x,device_y,&page_x,&page_y));
      EXPECT_NEAR(page_x,30,0.001);EXPECT_NEAR(page_y,355,0.001);
      EXPECT_EQ(api.FPDFPage_HasFormFieldAtPoint(session.form,session.page,page_x,page_y),FPDF_FORMFIELD_TEXTFIELD);
      session.Click(page_x,page_y);EXPECT_TRUE(api.FORM_SelectAllText(session.form,session.page));
      session.Replace(u"Rotated "+std::u16string(1,static_cast<char16_t>('0'+rotation)));
      EXPECT_EQ(session.Text(),u"Rotated "+std::u16string(1,static_cast<char16_t>('0'+rotation)));
      ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
      EXPECT_EQ(session.Field(0),u"Rotated "+std::u16string(1,static_cast<char16_t>('0'+rotation)));
      images[backend].push_back(session.Draw(820,840,rotation,0,13,17,800,800));
    }
    CheckSavedField(*library,session,artifacts,library->name()+"-converted-input",u"Rotated 3");
    ++backend;
  }
  // The 2x scenes have 4–13 isolated font-edge differences above18,
  // max36 (whole-image mean0.0194). The glyphs retain matching contours;
  // geometry, hit coordinates and field values are asserted independently.
  for(size_t i=0;i<images[0].size();++i) {
    const auto label="converted-input-"+std::to_string(i);
    CompareRaster(images[0][i],images[1][i],artifacts,label,RasterTolerance{40,0,0.025});
    for(int y=0;y<840;y+=64) for(int x=0;x<820;x+=64) {
      const int w=std::min(64,820-x),h=std::min(64,840-y);
      CompareRaster(CropRaster(images[0][i],x,y,w,h),CropRaster(images[1][i],x,y,w,h),
        artifacts,label+"-tile"+std::to_string(x)+"x"+std::to_string(y),RasterTolerance{40,0,0.4});
    }
  }
}

TEST(Forms, PlatformShortcutAndOtherModifierControlCharactersPreserveSelection) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),Fixture({},"Keep selection"));ASSERT_TRUE(session.valid());
    const auto& api=session.api;ASSERT_TRUE(session.Focus(30,355));
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));
#ifdef __APPLE__
    const int shortcut=FWL_EVENTFLAG_MetaKey,other=FWL_EVENTFLAG_ControlKey;
#else
    const int shortcut=FWL_EVENTFLAG_ControlKey,other=FWL_EVENTFLAG_MetaKey;
#endif
    // CPWL_Edit::OnCharInternal filters printable platform shortcuts and
    // control characters bearing either modifier, without replacing selection.
    api.FORM_OnChar(session.form,session.page,'A',shortcut);
    EXPECT_EQ(session.Text(),u"Keep selection");EXPECT_EQ(session.Text(true),u"Keep selection");
    for(int control:{1,3,22,24}) {
      api.FORM_OnChar(session.form,session.page,control,other);
      EXPECT_EQ(session.Text(),u"Keep selection");EXPECT_EQ(session.Text(true),u"Keep selection");
    }
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Right,0);
    EXPECT_EQ(session.Text(true),u"");
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_A,shortcut);
    EXPECT_EQ(session.Text(true),u"Keep selection");
    api.FORM_OnChar(session.form,session.page,'M',other);
    EXPECT_EQ(session.Text(),u"M");EXPECT_EQ(session.Text(true),u"");
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));
    session.Replace(u"Replacement");EXPECT_EQ(session.Text(),u"Replacement");
    ASSERT_TRUE(api.FORM_Undo(session.form,session.page));EXPECT_EQ(session.Text(),u"M");
    ASSERT_TRUE(api.FORM_Redo(session.form,session.page));EXPECT_EQ(session.Text(),u"Replacement");
    ASSERT_TRUE(api.FORM_SelectAllText(session.form,session.page));
    api.FORM_OnKeyDown(session.form,session.page,FWL_VKEY_Delete,0);EXPECT_EQ(session.Text(),u"");
    ASSERT_TRUE(api.FORM_Undo(session.form,session.page));EXPECT_EQ(session.Text(),u"Replacement");
    ASSERT_TRUE(api.FORM_Redo(session.form,session.page));EXPECT_EQ(session.Text(),u"");
    ASSERT_TRUE(api.FORM_Undo(session.form,session.page));EXPECT_EQ(session.Text(),u"Replacement");
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    images[backend++]=session.Draw();
  }
  CompareForms(images,artifacts,"platform-shortcut");
}

TEST(Forms, HoveredNonWidgetPopupRequiresAnnotationOverlayFlag) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<std::vector<Raster>,2> images;size_t backend=0;
  // Public setters cannot link a note to its popup. CPDF_AnnotList skips
  // stored Popup annotations and creates a 200x200 yellow popup from Contents;
  // this fixture deliberately includes a different stored popup to verify that
  // interaction paints the generated popup rather than the serialized one.
  const auto fixture=PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Annots [4 0 R 5 0 R 8 0 R] >>",
    "<< /Type /Annot /Subtype /Text /Rect [20 300 50 330] /Contents (Hover note) /Popup 5 0 R /AS /On /AP << /N << /On 6 0 R >> >> >>",
    "<< /Type /Annot /Subtype /Popup /Rect [100 200 200 280] /Parent 4 0 R /AS /On /AP << /N << /On 7 0 R >> >> >>",
    PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 30 30]","0 0 1 rg 0 0 30 30 re f"),
    PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 100 80]","0 1 0 rg 0 0 100 80 re f"),
    "<< /Type /Annot /Subtype /Link /Rect [250 300 350 330] /A << /S /URI /URI (https://example.invalid/hover) >> >>"});
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session session(library->api(),fixture);ASSERT_TRUE(session.valid());const auto& api=session.api;
    Handle note(api.FPDFPage_GetAnnot(session.page,0),api.FPDFPage_CloseAnnot);
    ASSERT_NE(note.get(),nullptr);EXPECT_FALSE(api.FORM_SetFocusedAnnot(session.form,note.get()));
    auto closed=session.Draw(400,400,0,FPDF_ANNOT);
    for(double x:{260.,280.,390.}) api.FORM_OnMouseMove(session.form,session.page,0,x,315);
    EXPECT_TRUE(session.host->uris.empty());
    EXPECT_EQ(session.Draw(400,400,0,FPDF_ANNOT).pixels,closed.pixels);
    const auto white=CropRaster(closed,110,130,80,60);
    for(uint8_t channel:white.pixels) EXPECT_EQ(channel,255);
    const auto invalidations=session.host->invalidations;
    api.FORM_OnMouseMove(session.form,session.page,0,30,315);
    EXPECT_GT(session.host->invalidations,invalidations);
    auto hidden=session.Draw(400,400,0,0);
    EXPECT_EQ(CropRaster(hidden,110,130,80,60).pixels,white.pixels);
    auto opened=session.Draw(400,400,0,FPDF_ANNOT);
    const auto green=CropRaster(opened,110,130,80,60);
    for(size_t i=0;i<green.pixels.size();i+=4) {
      EXPECT_EQ(green.pixels[i],0);EXPECT_EQ(green.pixels[i+1],255);
      EXPECT_EQ(green.pixels[i+2],255);EXPECT_EQ(green.pixels[i+3],255);
    }
    api.FORM_OnMouseMove(session.form,session.page,0,390,390);
    auto exited=session.Draw(400,400,0,FPDF_ANNOT);
    EXPECT_EQ(exited.pixels,closed.pixels);
    images[backend++]={std::move(closed),std::move(hidden),std::move(opened),std::move(exited)};
    EXPECT_TRUE(session.host->callbacks_valid);
  }
  for(size_t i=0;i<images[0].size();++i) {
    const auto label="hover-popup-"+std::to_string(i);
    if(i==2) {
      // Only the generated Contents glyph edges differ (240pixels, max11,
      // mean0.00181 full scene); solid popup/background geometry is identical.
      CompareRaster(images[0][i],images[1][i],artifacts,label,RasterTolerance{12,0,0.003});
      CompareRaster(CropRaster(images[0][i],20,100,200,200),CropRaster(images[1][i],20,100,200,200),
        artifacts,label+"-popup",RasterTolerance{12,0,0.01});
    } else CompareRaster(images[0][i],images[1][i],artifacts,label);
  }
}


TEST(FormActions, DistinctPageActionsAndScriptedFocusRespectPageLifecycle) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Actions actions;
    actions.page_open="this.getField('input').value='first-open';";
    actions.page_close="this.getField('input').value+='-close';";
    actions.later_page_open="this.getField('later').value='later-open';";
    actions.later_page_close="this.getField('later').value+='-close';";
    actions.open="this.getField('second').setFocus();";
    Session session(library->api(),Fixture(actions),true);ASSERT_TRUE(session.valid());const auto& api=session.api;
    auto later=session.host->Page(1);ASSERT_NE(later,nullptr);
    api.FORM_DoPageAAction(session.page,session.form,FPDFPAGE_AACTION_OPEN);
    EXPECT_EQ(session.Field(0),u"first-open");EXPECT_EQ(session.Field(0,1),u"later");
    api.FORM_DoPageAAction(later,session.form,FPDFPAGE_AACTION_OPEN);
    EXPECT_EQ(session.Field(0),u"first-open");EXPECT_EQ(session.Field(0,1),u"later-open");
    api.FORM_DoDocumentOpenAction(session.form);
    int page=-1;FPDF_ANNOTATION focused=nullptr;
    ASSERT_TRUE(api.FORM_GetFocusedAnnot(session.form,&page,&focused));ASSERT_NE(focused,nullptr);
    Handle annotation(focused,api.FPDFPage_CloseAnnot);EXPECT_EQ(page,0);
    EXPECT_EQ(api.FPDFPage_GetAnnotIndex(session.page,annotation.get()),1);
    session.Replace(u"Focused by script");EXPECT_EQ(session.Text(),u"Focused by script");
    ASSERT_TRUE(api.FORM_ForceToKillFocus(session.form));
    api.FORM_DoPageAAction(later,session.form,FPDFPAGE_AACTION_CLOSE);
    EXPECT_EQ(session.Field(0,1),u"later-open-close");
    session.ClosePage(1);
    api.FORM_DoPageAAction(session.page,session.form,FPDFPAGE_AACTION_CLOSE);
    EXPECT_EQ(session.Field(0),u"first-open-close");
    EXPECT_EQ(session.Field(1),u"Focused by script");
    EXPECT_FALSE(session.Focus(230,75))<<"The geometrically hit-testable hidden field must not accept input focus";
    images[backend++]=session.Draw();
    CheckSavedField(*library,session,artifacts,library->name()+"-cross-page-actions",u"first-open-close");
    EXPECT_TRUE(session.host->callbacks_valid);
  }
  CompareForms(images,artifacts,"cross-page-actions");
}

TEST(FormActions, EmbeddedWaveDemoAnimatesAndPausesThroughRealTimerCallbacks) {
  ScopedPinnedTestFonts fonts;
  std::ifstream file(TestRoot()/"crates/pdfoxium-viewer/assets/demos/animation-demo.pdf",std::ios::binary);
  ASSERT_TRUE(file);
  const std::vector<uint8_t> bytes((std::istreambuf_iterator<char>(file)),{});
  Artifacts artifacts;
  std::array<Raster,2> initial, advanced;
  size_t index=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name()); const auto& api=library->api();
    Session session(api,bytes,true); ASSERT_TRUE(session.valid());
    api.FORM_DoDocumentJSAction(session.form);
    initial[index]=session.DrawForComparison(612,792);
    session.Click(306,420);
    ASSERT_EQ(session.host->timers.size(),1u);
    const int timer=session.host->timers.begin()->first;
    session.host->Tick(timer);session.host->Tick(timer);session.host->Tick(timer);
    // The native hit can focus a frame which becomes hidden, while the Rust
    // hit focuses the persistent controller. Compare equally unfocused frames.
    api.FORM_ForceToKillFocus(session.form);
    advanced[index]=session.DrawForComparison(612,792);
    const auto before=CropLogicalRaster(initial[index],56,252,500,240);
    const auto after=CropLogicalRaster(advanced[index],56,252,500,240);
    EXPECT_NE(before.pixels,after.pixels);
    session.Click(136,266);EXPECT_TRUE(session.host->timers.empty());
    const auto paused=session.DrawForComparison(612,792);
    session.host->Tick(timer);
    EXPECT_EQ(CropLogicalRaster(paused,56,252,500,240).pixels,
              CropLogicalRaster(session.DrawForComparison(612,792),56,252,500,240).pixels);
    artifacts.Image(std::string(library->name())+"-wave-initial",initial[index]);
    artifacts.Image(std::string(library->name())+"-wave-advanced",advanced[index]);
    ++index;
  }
  CompareRaster(CropLogicalRaster(initial[0],56,252,500,240),CropLogicalRaster(initial[1],56,252,500,240),artifacts,"wave-initial",RasterTolerance{16,0,.04});
  CompareRaster(CropLogicalRaster(advanced[0],56,252,500,240),CropLogicalRaster(advanced[1],56,252,500,240),artifacts,"wave-advanced",RasterTolerance{16,0,.04});
}

}  // namespace
}  // namespace pdfoxium::integration
