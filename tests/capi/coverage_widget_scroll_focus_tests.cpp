#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <functional>
#include <set>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using Bytes = std::vector<uint8_t>;
constexpr int kMulti = 2097152;
constexpr int kEditable = 393216;
struct ChoiceSpec {
  int list_flags = 0, combo_flags = 131072;
  std::string list_actions, combo_actions, text_actions;
  std::string initial_text = "ABCDEF";
};
std::string Actions(const std::string& keys) { return keys.empty() ? "" : " /AA << " + keys + " >>"; }
std::string Action(const std::string& key, const std::string& code) { return "/" + key + " " + forms::Js(code) + " "; }
Bytes ChoiceFixture(const ChoiceSpec& spec) {
  const std::string options = "[[(e0) (Alpha)] [(e1) (Beta)] [(e2) (Bravo)] [(e3) (Charlie)] [(e4) (Delta)] [(e5) (Echo)] [(e6) (Zulu)]]";
  const std::string widget = "/Type /Annot /Subtype /Widget /P 3 0 R /F 4 /BS << /W 1 >> /MK << /BC [0 0 0] /BG [1 1 1] >> ";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Annots [6 0 R 7 0 R 8 0 R 9 0 R] /Resources << /Font << /Cour 5 0 R >> >> /Contents 10 0 R >>",
    "<< /Fields [6 0 R 7 0 R 8 0 R 9 0 R] /NeedAppearances true /DA (/Cour 12 Tf 0 g) /DR << /Font << /Cour 5 0 R >> >> >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>",
    "<< " + widget + "/FT /Ch /T (list) /Rect [20 190 200 350] /Opt " + options + " /V (e0) /I [0] /Ff " + std::to_string(spec.list_flags) + Actions(spec.list_actions) + " >>",
    "<< " + widget + "/FT /Ch /T (combo) /Rect [220 320 380 350] /Opt " + options + " /V (e0) /I [0] /Ff " + std::to_string(spec.combo_flags) + Actions(spec.combo_actions) + " >>",
    "<< " + widget + "/FT /Tx /T (text) /Rect [20 70 200 150] /Ff 4096 /V (" + spec.initial_text + ")" + Actions(spec.text_actions) + " >>",
    "<< " + widget + "/FT /Tx /T (report) /Rect [20 20 380 50] /V () >>",
    PdfStream("", "0.94 g 0 0 400 400 re f 0 0.7 0 rg 220 190 40 80 re f\n")});
}
bool Native(const Session& s) { return &s.api == &Implementations()[0]->api(); }
void Key(Session& s, int key, int flags = 0) { s.api.FORM_OnKeyDown(s.form,s.page,key,flags); }
void Char(Session& s, int character, int flags = 0) { s.api.FORM_OnChar(s.form,s.page,character,flags); }
void Type(Session& s,const std::u16string& text) { for(char16_t c:text) Char(s,c); }
void ListFocus(Session& s) { ASSERT_TRUE(s.Focus(30,344)); }
void ComboFocus(Session& s) { ASSERT_TRUE(s.Focus(230,335)); }
void Selection(Session& s, const std::set<int>& expected) {
  for (int i=0;i<7;++i) EXPECT_EQ(bool(s.api.FORM_IsIndexSelected(s.form,s.page,i)),expected.count(i)!=0)<<i;
}
void AnnotSelection(Session& s, int annotation, const std::set<int>& expected) {
  Handle a(s.api.FPDFPage_GetAnnot(s.page,annotation),s.api.FPDFPage_CloseAnnot);ASSERT_NE(a.get(),nullptr);
  EXPECT_EQ(s.api.FPDFAnnot_GetOptionCount(s.form,a.get()),7);
  for (int i=0;i<7;++i) EXPECT_EQ(bool(s.api.FPDFAnnot_IsOptionSelected(s.form,a.get(),i)),expected.count(i)!=0)<<i;
}
struct Expected {
  std::set<int> list{0}, combo{0};
  std::u16string text=u"ABCDEF", combo_value=u"e0", report;
};
void Model(Session& s,const Expected& e) {
  AnnotSelection(s,0,e.list);AnnotSelection(s,1,e.combo);
  EXPECT_EQ(s.Field(1),e.combo_value);EXPECT_EQ(s.Field(2),e.text);EXPECT_EQ(s.Field(3),e.report);
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,{18,0,0.08});
  for(const auto& box:std::array<std::array<int,4>,3>{{{20,50,180,160},{220,50,160,30},{20,250,180,80}}})
    CompareRaster(CropRaster(a,box[0],box[1],box[2],box[3]),CropRaster(b,box[0],box[1],box[2],box[3]),artifacts,label+std::to_string(box[0])+"-"+std::to_string(box[1]),{18,0,0.4});
}
void Exercise(const ChoiceSpec& spec,const std::string& label,const std::function<void(Session&,Expected&,std::vector<Raster>&)>& run) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<std::vector<Raster>,2> images;size_t n=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Session s(writer->api(),ChoiceFixture(spec),true);ASSERT_TRUE(s.valid());Expected expected;
    ASSERT_NO_FATAL_FAILURE(run(s,expected,images[n]));
    s.api.FORM_ForceToKillFocus(s.form);Model(s,expected);const auto final=s.Draw();images[n].push_back(final);
    forms::ExpectInk(final,20,250,200,330);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+label);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Session loaded(reader->api(),saved,true);ASSERT_TRUE(loaded.valid());Model(loaded,expected);
      Compare(final,loaded.Draw(),artifacts,writer->name()+reader->name()+label);
    }
    EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);++n;
  }
  ASSERT_EQ(images[0].size(),images[1].size());
  for(size_t i=0;i<images[0].size();++i)Compare(images[0][i],images[1][i],artifacts,label+std::to_string(i));
}
void Frame(Session& s,std::vector<Raster>& frames) { frames.push_back(s.Draw()); }
// Native's ordinary OnAAction reports false after running K scripts. Retain
// its original-input result and the correct Rust rewrite, then use real public
// input with the guarded script disabled for the shared raster/reload control.
void RecoverCombo(Session& s,const std::u16string& value) {
  ASSERT_TRUE(s.Focus(30,35));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"done");
  s.api.FORM_ForceToKillFocus(s.form);ComboFocus(s);ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));Type(s,value);EXPECT_EQ(s.Text(),value);
}
double ListY(Session& s,int index) {
  // CPWL_ListCtrl lays out each item using the editor's ascent minus descent.
  // Pinned Cousine has a different line box from the built-in substitute; use
  // the same public font metrics rather than assuming twelve-point rows.
  Handle font(s.api.FPDFText_LoadStandardFont(s.document,"Courier"),s.api.FPDFFont_Close);
  EXPECT_NE(font.get(),nullptr);float ascent=0,descent=0;
  EXPECT_TRUE(s.api.FPDFFont_GetAscent(font.get(),12,&ascent));
  EXPECT_TRUE(s.api.FPDFFont_GetDescent(font.get(),12,&descent));
  const double height=ascent-descent;EXPECT_GT(height,0);EXPECT_LT(height,24);
  return 349.-(index+.5)*height;
}
void ListClick(Session& s,int index,int flags=0) { s.Click(30,ListY(s,index),flags); }
void Drag(Session& s,int begin,int end,int modifiers=0) {
  const auto y=[&s](int i){return ListY(s,i);};
  s.api.FORM_OnLButtonDown(s.form,s.page,modifiers,30,y(begin));
  s.api.FORM_OnMouseMove(s.form,s.page,modifiers|FWL_EVENTFLAG_LeftButtonDown,30,y(end));
  s.api.FORM_OnLButtonUp(s.form,s.page,modifiers,30,y(end));
}

TEST(CoverageWidgetScrollFocus, SingleListTypeaheadCyclesMatchingInitialsAndWrapsCaseInsensitively) {
  Exercise({},"list-typeahead",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);
    for(int index:{1,2,1}){Char(s,'b');Selection(s,{index});Frame(s,f);e.list={index};}
    Char(s,'Z');Selection(s,{6});Char(s,'a');Selection(s,{0});e.list={0};
  });
}
TEST(CoverageWidgetScrollFocus, UnmatchedAndControlCharactersDoNotDestroyTheExistingListSelection) {
  Exercise({},"list-unmatched",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);Char(s,'D');Selection(s,{4});
    for(int character:{int('?'),0,8,10,127}){Char(s,character);Selection(s,{4});}e.list={4};Frame(s,f);
  });
}
TEST(CoverageWidgetScrollFocus, SingleListLeftAndRightReachOppositeEndsAndClampRepeatedMovement) {
  Exercise({},"list-left-right",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);
    for(int key:{FWL_VKEY_Right,FWL_VKEY_Right,FWL_VKEY_Down}){Key(s,key);Selection(s,{6});}Frame(s,f);
    for(int key:{FWL_VKEY_Left,FWL_VKEY_Left,FWL_VKEY_Up}){Key(s,key);Selection(s,{0});}e.list={0};
  });
}
TEST(CoverageWidgetScrollFocus, ShiftKeyboardRangesUseTheOriginalListAnchorInBothDirections) {
  ChoiceSpec spec;spec.list_flags=kMulti;
  Exercise(spec,"list-shift",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);ListClick(s,3);Selection(s,{3});
    Key(s,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey);Selection(s,{3,4,5,6});Frame(s,f);
    Key(s,FWL_VKEY_Left,FWL_EVENTFLAG_ShiftKey);Selection(s,{0,1,2,3});Frame(s,f);e.list={0,1,2,3};
  });
}
TEST(CoverageWidgetScrollFocus, ControlMovementChangesCaretWhileLeavingTheListSelectionUntouched) {
  ChoiceSpec spec;spec.list_flags=kMulti;
  Exercise(spec,"list-control",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);ListClick(s,2);Selection(s,{2});
    for(int key:{FWL_VKEY_Right,FWL_VKEY_Left,FWL_VKEY_End,FWL_VKEY_Home}){Key(s,key,FWL_EVENTFLAG_ControlKey);Selection(s,{2});Frame(s,f);}e.list={2};
  });
}
TEST(CoverageWidgetScrollFocus, ControlClicksToggleOnlyTheHitAcroFormListItem) {
  ChoiceSpec spec;spec.list_flags=kMulti;
  Exercise(spec,"list-toggle",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);ListClick(s,2,FWL_EVENTFLAG_ControlKey);Selection(s,{0,2});
    ListClick(s,0,FWL_EVENTFLAG_ControlKey);Selection(s,{2});Frame(s,f);ListClick(s,4,FWL_EVENTFLAG_ControlKey);Selection(s,{2,4});e.list={2,4};
  });
}
TEST(CoverageWidgetScrollFocus, ControlDragAddsAnInclusiveRangeWithoutClearingOtherListItems) {
  ChoiceSpec spec;spec.list_flags=kMulti;
  Exercise(spec,"list-add-drag",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);Drag(s,2,4,FWL_EVENTFLAG_ControlKey);Selection(s,{0,2,3,4});Frame(s,f);e.list={0,2,3,4};});
}
TEST(CoverageWidgetScrollFocus, ControlDragSubtractsAnInclusiveRangeFromTheExistingSelection) {
  ChoiceSpec spec;spec.list_flags=kMulti;
  Exercise(spec,"list-subtract-drag",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);Drag(s,0,6);Selection(s,{0,1,2,3,4,5,6});
    Drag(s,2,4,FWL_EVENTFLAG_ControlKey);Selection(s,{0,1,5,6});Frame(s,f);e.list={0,1,5,6};});
}
TEST(CoverageWidgetScrollFocus, OrdinaryReverseDragReplacesAFormerMultiSelection) {
  ChoiceSpec spec;spec.list_flags=kMulti;
  Exercise(spec,"list-reverse-drag",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);Drag(s,0,6);Drag(s,4,2);Selection(s,{2,3,4});Frame(s,f);e.list={2,3,4};});
}
TEST(CoverageWidgetScrollFocus, TypeaheadInMultiSelectListsReplacesTheSelectionAndUpdatesItsAnchor) {
  ChoiceSpec spec;spec.list_flags=kMulti;
  Exercise(spec,"list-multi-typeahead",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);Drag(s,0,3);Char(s,'E');Selection(s,{5});
    Key(s,FWL_VKEY_Up,FWL_EVENTFLAG_ShiftKey);Selection(s,{4,5});Frame(s,f);e.list={4,5};});
}
TEST(CoverageWidgetScrollFocus, CommitOnSelectionListActionsKeepDisplayChangeSeparateFromSavedExport) {
  ChoiceSpec spec;spec.list_flags=67108864;spec.list_actions=Action("K","if(!event.willCommit){this.getField('report').value=event.changeEx+':'+event.change;}");
  Exercise(spec,"list-change-export",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);Char(s,'D');Selection(s,{4});e.list={4};e.report=u":Delta";Frame(s,f);});
}
TEST(CoverageWidgetScrollFocus, SingleListValidationReceivesItsDisplayValueAndCanRejectCommit) {
  ChoiceSpec spec;spec.list_actions=Action("V","this.getField('report').value=event.value;event.rc=false;");
  Exercise(spec,"list-reject",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);Char(s,'D');Selection(s,{4});Frame(s,f);s.api.FORM_ForceToKillFocus(s.form);e.list={0};e.report=u"Delta";});
}
TEST(CoverageWidgetScrollFocus, NonEditableComboTypeaheadUsesTheListRatherThanCreatingCustomText) {
  Exercise({},"combo-typeahead",[](Session& s,Expected& e,std::vector<Raster>& f){ComboFocus(s);Char(s,'b');Selection(s,{1});Char(s,'B');Selection(s,{2});Frame(s,f);e.combo={2};e.combo_value=u"e2";});
}
TEST(CoverageWidgetScrollFocus, SpaceOpensANonEditableComboAndEscapeLeavesItsSelectionUnchanged) {
  Exercise({},"combo-space",[](Session& s,Expected& e,std::vector<Raster>& f){ComboFocus(s);Char(s,' ');Frame(s,f);Key(s,FWL_VKEY_Escape);e.combo={0};});
}
TEST(CoverageWidgetScrollFocus, ReturnTogglesTheComboPopupAndActualMouseReleaseSelectsTheHitItem) {
  Exercise({},"combo-popup-click",[](Session& s,Expected& e,std::vector<Raster>& f){ComboFocus(s);Char(s,13);Frame(s,f);
    s.Click(230,313);Frame(s,f);e.combo={0};
  });
}
TEST(CoverageWidgetScrollFocus, EditableComboCustomTextSupportsUndoRedoAndInvalidatesTheSelectedIndex) {
  ChoiceSpec spec;spec.combo_flags=kEditable;
  Exercise(spec,"combo-history",[](Session& s,Expected& e,std::vector<Raster>& f){ComboFocus(s);ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));Char(s,'L');EXPECT_EQ(s.Text(),u"L");
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),u"Alpha");ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),u"L");Frame(s,f);e.combo={};e.combo_value=u"L";});
}
TEST(CoverageWidgetScrollFocus, EditableComboKeystrokeChangesAreAppliedThroughTheExistingSelection) {
  ChoiceSpec spec;spec.combo_flags=kEditable;spec.combo_actions=Action("K","if(!event.willCommit&&this.getField('report').value!=='done'){event.change=event.change.toUpperCase();this.getField('report').value='changed';}");
  Exercise(spec,"combo-change",[](Session& s,Expected& e,std::vector<Raster>& f){ComboFocus(s);ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));Type(s,u"mixed");EXPECT_EQ(s.Text(),Native(s)?u"mixed":u"MIXED");EXPECT_EQ(s.Field(3),u"changed");RecoverCombo(s,u"MIXED");Frame(s,f);e.combo={};e.combo_value=u"MIXED";e.report=u"done";});
}
TEST(CoverageWidgetScrollFocus, EditableComboKeystrokeMayReplaceTheRangeWithoutLosingFollowingInput) {
  ChoiceSpec spec;spec.combo_flags=kEditable;spec.combo_actions=Action("K","if(!event.willCommit&&this.getField('report').value!=='done'){event.selStart=0;event.selEnd=event.value.length;event.change='L';this.getField('report').value='changed';}");
  Exercise(spec,"combo-change-range",[](Session& s,Expected& e,std::vector<Raster>& f){ComboFocus(s);Key(s,FWL_VKEY_End);Char(s,'x');EXPECT_EQ(s.Text(),Native(s)?u"Alphax":u"L");EXPECT_EQ(s.Field(3),u"changed");RecoverCombo(s,u"L");Frame(s,f);e.combo={};e.combo_value=u"L";e.report=u"done";});
}
TEST(CoverageWidgetScrollFocus, ComboValidationRejectionRestoresItsOriginalExportAndFollowingFocusStillWorks) {
  ChoiceSpec spec;spec.combo_flags=kEditable;spec.combo_actions=Action("V","this.getField('report').value=event.value;event.rc=false;");
  Exercise(spec,"combo-reject",[](Session& s,Expected& e,std::vector<Raster>& f){ComboFocus(s);ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));Type(s,u"Rejected");Frame(s,f);s.api.FORM_ForceToKillFocus(s.form);e.report=u"Rejected";e.combo={0};});
}
// Pinned Field.fillColor only validates its argument; Field.textSize has no
// setter. These callbacks do not change appearance age. Actual recreation is
// exercised by CoverageFormWindowRecreation.
TEST(CoverageWidgetScrollFocus, ComboIgnoredFillColorSetterDuringKeystrokePreservesPendingTextAndItsSelection) {
  ChoiceSpec spec;spec.combo_flags=kEditable;spec.combo_actions=Action("K","if(!event.willCommit&&this.getField('report').value!=='once'){this.getField('report').value='once';this.getField('combo').fillColor=['RGB',0.8,0.9,1];}");
  Exercise(spec,"combo-recreate",[](Session& s,Expected& e,std::vector<Raster>& f){ComboFocus(s);ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));Type(s,u"Recreated");EXPECT_EQ(s.Text(),u"Recreated");Frame(s,f);e.combo={};e.combo_value=u"Recreated";e.report=u"once";});
}
TEST(CoverageWidgetScrollFocus, ListTextSizeAssignmentDuringKeystrokePreservesItsSelectedItem) {
  ChoiceSpec spec;spec.list_actions=Action("K","if(!event.willCommit&&this.getField('report').value!=='once'){this.getField('report').value='once';this.getField('list').textSize=10;}");
  Exercise(spec,"list-recreate",[](Session& s,Expected& e,std::vector<Raster>& f){ListFocus(s);Char(s,'D');Selection(s,{4});Frame(s,f);e.list={4};e.report=u"once";});
}
TEST(CoverageWidgetScrollFocus, ComboFocusAndBlurActionsObserveCommittedExportsSeparatelyFromPendingText) {
  ChoiceSpec spec;spec.combo_flags=kEditable;spec.combo_actions=Action("Fo","this.getField('report').value='focus:'+event.value;")+Action("Bl","this.getField('report').value+=';blur:'+event.value;");
  Exercise(spec,"combo-focus-values",[](Session& s,Expected& e,std::vector<Raster>& f){ComboFocus(s);ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));Type(s,u"Fresh");Frame(s,f);s.api.FORM_ForceToKillFocus(s.form);e.combo={};e.combo_value=u"Fresh";e.report=u"focus:e0;blur:Fresh";});
}
TEST(CoverageWidgetScrollFocus, CommittedComboValueStartsFreshHistoryWhenItsWindowIsRecreatedOnReentry) {
  ChoiceSpec spec;spec.combo_flags=kEditable;
  Exercise(spec,"combo-refocus",[](Session& s,Expected& e,std::vector<Raster>& f){ComboFocus(s);ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));Type(s,u"Saved");s.api.FORM_ForceToKillFocus(s.form);ComboFocus(s);EXPECT_EQ(s.Text(),u"Saved");EXPECT_FALSE(s.api.FORM_CanUndo(s.form,s.page));
    ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));Type(s,u"Final");EXPECT_TRUE(s.api.FORM_CanUndo(s.form,s.page));ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),u"Fina");ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),u"Final");Frame(s,f);e.combo={};e.combo_value=u"Final";});
}
TEST(CoverageWidgetScrollFocus, MultilineDeletionRefreshesEverySelectedLineBeforeUndoAndRedo) {
  ChoiceSpec spec;spec.initial_text="First\\rSecond\\rThird\\rFourth";
  Exercise(spec,"multiline-refresh",[](Session& s,Expected& e,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(30,140));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));Frame(s,f);s.Replace(u"Short");EXPECT_EQ(s.Text(),u"Short");
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),u"First\r\nSecond\r\nThird\r\nFourth");Frame(s,f);ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));e.text=u"Short";});
}
TEST(CoverageWidgetScrollFocus, MultilineSelectionDrawingEmitsOnlyFiniteRectanglesWithinItsField) {
  ChoiceSpec spec;spec.initial_text="First\\rSecond\\rThird\\rFourth";
  Exercise(spec,"selection-rectangles",[](Session& s,Expected& e,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(30,140));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));Frame(s,f);EXPECT_FALSE(s.host->selections.empty());
    for(const auto& rect:s.host->selections) {
      for(double value:rect) EXPECT_TRUE(std::isfinite(value));
    }
    s.Replace(u"Visible");e.text=u"Visible";});
}

// Foreground XFA keeps the PDF SDK widgets and routes their actions and value
// synchronization into the already loaded XFA model. Full XFA tests exercise a
// different dispatcher, so these fixtures deliberately use NeedsRendering false.
// A real PDF parent field produces root.amount as its full name. Bracketed SOM
// spellings fail the source GetWidgetByName XML-name guard before resolution.
enum class ForegroundKind { Text, Combo, List, Check };
Bytes ForegroundFixture(ForegroundKind kind,const std::string& events="",int max_chars=0) {
  const bool choice=kind==ForegroundKind::Combo||kind==ForegroundKind::List;
  const bool check=kind==ForegroundKind::Check;
  const std::string initial=choice?"b":check?"Off":"old";
  const std::string items="<items><text>Alpha</text><text>Beta</text><text>Charlie</text><text>Delta</text></items><items save=\"1\"><text>a</text><text>b</text><text>c</text><text>d</text></items>";
  const std::string ui=choice?"<choiceList open=\""+std::string(kind==ForegroundKind::Combo?"userControl":"always")+"\"/>":check?"<checkButton size=\"20pt\" shape=\"square\"/>":"<textEdit/>";
  const std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" locale=\"en_US\"><pageSet><pageArea><medium short=\"400pt\" long=\"400pt\"/><contentArea w=\"400pt\" h=\"400pt\"/></pageArea></pageSet>"
      "<field name=\"amount\" x=\"20pt\" y=\"70pt\" w=\"160pt\" h=\"80pt\"><bind match=\"dataRef\" ref=\"$record.amount\"/><font typeface=\"Helvetica\" size=\"12pt\"/><ui>"+ui+"</ui>"+
      (choice?items:check?"<items><text>Yes</text><text>Off</text></items>":"")+"<value><text"+(max_chars?" maxChars=\""+std::to_string(max_chars)+"\"":"")+">"+initial+"</text></value>"+events+"</field></subform></template>";
  const std::string field=choice?"/FT /Ch /Ff "+std::string(kind==ForegroundKind::Combo?"131072":"0")+" /Opt [[(a) (Alpha)] [(b) (Beta)] [(c) (Charlie)] [(d) (Delta)]] /V (b) /I [1]":check?"/FT /Btn /V /Off /AS /Off /AP << /N << /Off 13 0 R /Yes 14 0 R >> >>":std::string("/FT /Tx /V (old)")+(max_chars?" /MaxLen "+std::to_string(max_chars):"");
  auto font=forms::PinnedXfaArimoFontObjects();
  font[0].replace(font[0].find("/FontDescriptor 12 0 R"),std::string("/FontDescriptor 12 0 R").size(),"/FontDescriptor 16 0 R");
  font[1].replace(font[1].find("/FontFile2 13 0 R"),std::string("/FontFile2 13 0 R").size(),"/FontFile2 17 0 R");
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering false >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Annots [10 0 R] /Resources << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> /Contents 12 0 R >>",
    "<< /Fields [15 0 R] /NeedAppearances true /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
    PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>forbidden</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
    PdfStream("",xml),
    PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root><amount>"+initial+"</amount></root></xfa:data></xfa:datasets>"),
    PdfStream("","</xdp:xdp>"),
    "<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /Parent 15 0 R /T (amount) /Rect [20 250 180 330] /BS << /W 1 >> /MK << /BC [0 0 0] /BG [1 1 1] >> "+field+Actions(Action("K","")+Action("V","")+Action("Fo","")+Action("Bl","")+Action("E","")+Action("X","")+Action("D","")+Action("U",""))+" >>",
    std::move(font[0]),
    PdfStream("","0.94 g 0 0 400 400 re f 0 0.7 0 rg 220 250 40 80 re f"),
    PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 160 80]","1 g 0 0 160 80 re f 0 G 1 w .5 .5 159 79 re S"),
    PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 160 80]","1 g 0 0 160 80 re f 0 G 1 w .5 .5 159 79 re S 0 g 8 8 20 20 re f"),
    "<< /T (root) /Kids [10 0 R] >>",std::move(font[1]),std::move(font[2])});
}
void Foreground(const Bytes& bytes,const std::string& label,const std::u16string& expected,const std::function<void(Session&,std::vector<Raster>&)>& interact) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<std::vector<Raster>,2> images;size_t n=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Session s(writer->api(),bytes,true,true,2,{},true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);EXPECT_EQ(s.api.FPDF_GetFormType(s.document),FORMTYPE_XFA_FOREGROUND);
    ASSERT_NO_FATAL_FAILURE(interact(s,images[n]));s.api.FORM_ForceToKillFocus(s.form);EXPECT_EQ(s.Field(0),expected);const auto final=s.Draw();images[n].push_back(final);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+label);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Session loaded(reader->api(),saved,true,true,2,{},true);ASSERT_TRUE(loaded.valid());ASSERT_TRUE(loaded.xfa_loaded);EXPECT_EQ(loaded.Field(0),expected);
      CompareRaster(final,loaded.Draw(),artifacts,writer->name()+reader->name()+label,{32,0,0.15});
    }
    EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);++n;
  }
  ASSERT_EQ(images[0].size(),images[1].size());for(size_t i=0;i<images[0].size();++i)CompareRaster(images[0][i],images[1][i],artifacts,label+std::to_string(i),{32,0,0.15});
}
std::vector<std::u16string> Messages(Session& s) { std::vector<std::u16string> out;for(const auto& a:s.host->alerts)out.push_back(std::get<0>(a));return out; }
TEST(CoverageWidgetScrollFocus, ForegroundXfaChangeActionsObserveTheActualSdkReplacementAndPreviousText) {
  const auto bytes=ForegroundFixture(ForegroundKind::Text,forms::Event("xfa.host.messageBox(xfa.event.change+':'+xfa.event.prevText);","change"));
  Foreground(bytes,"foreground-change",u"mixed",[](Session& s,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(30,300));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));Type(s,u"mixed");EXPECT_EQ(s.Text(),u"mixed");const auto messages=Messages(s);
    EXPECT_NE(std::find(messages.begin(),messages.end(),u"m:old"),messages.end());
    EXPECT_NE(std::find(messages.begin(),messages.end(),u"d:mixe"),messages.end());Frame(s,f);});
}
TEST(CoverageWidgetScrollFocus, ForegroundXfaMouseActionsRouteThroughTheSdkHitAndCapturePaths) {
  const auto bytes=ForegroundFixture(ForegroundKind::Text,
      forms::Event("xfa.host.messageBox('mouseEnter');","mouseEnter")+
      forms::Event("xfa.host.messageBox('mouseExit');","mouseExit")+
      forms::Event("xfa.host.messageBox('mouseDown');","mouseDown")+
      forms::Event("xfa.host.messageBox('mouseUp');","mouseUp"));
  Foreground(bytes,"foreground-mouse",u"old",[](Session& s,std::vector<Raster>& f){s.Click(30,300);Frame(s,f);s.api.FORM_OnMouseMove(s.form,s.page,0,390,390);const auto messages=Messages(s);
    for(const auto* message:{u"mouseEnter",u"mouseExit",u"mouseDown",u"mouseUp"})EXPECT_NE(std::find(messages.begin(),messages.end(),message),messages.end());
  });
}
TEST(CoverageWidgetScrollFocus, ForegroundXfaEnterAndExitActionsRouteThroughTheActualPdfWidget) {
  const auto bytes=ForegroundFixture(ForegroundKind::Text,forms::Event("xfa.host.messageBox('enter');","enter")+forms::Event("xfa.host.messageBox('exit');","exit"));
  Foreground(bytes,"foreground-focus",u"changed",[](Session& s,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(30,300));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"changed");Frame(s,f);s.api.FORM_ForceToKillFocus(s.form);const auto messages=Messages(s);EXPECT_NE(std::find(messages.begin(),messages.end(),u"enter"),messages.end());EXPECT_NE(std::find(messages.begin(),messages.end(),u"exit"),messages.end());});
}
TEST(CoverageWidgetScrollFocus, ForegroundXfaComboIndexChangesSynchronizeDisplayAndExportValues) {
  Foreground(ForegroundFixture(ForegroundKind::Combo),"foreground-combo",u"d",[](Session& s,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(30,300));ASSERT_TRUE(s.api.FORM_SetIndexSelected(s.form,s.page,3,true));EXPECT_TRUE(s.api.FORM_IsIndexSelected(s.form,s.page,3));Frame(s,f);});
}
TEST(CoverageWidgetScrollFocus, ForegroundXfaListIndexChangesSynchronizeTheExistingModel) {
  Foreground(ForegroundFixture(ForegroundKind::List),"foreground-list",u"d",[](Session& s,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(30,320));ASSERT_TRUE(s.api.FORM_SetIndexSelected(s.form,s.page,3,true));EXPECT_TRUE(s.api.FORM_IsIndexSelected(s.form,s.page,3));Frame(s,f);});
}
TEST(CoverageWidgetScrollFocus, ForegroundXfaCheckKeyboardAndMouseUpdatesPersistTheirOnValue) {
  Foreground(ForegroundFixture(ForegroundKind::Check),"foreground-check",u"Yes",[](Session& s,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(30,300));Char(s,' ');Frame(s,f);s.Click(30,300);s.Click(30,300);Frame(s,f);});
}
TEST(CoverageWidgetScrollFocus, ForegroundXfaFullEventLeavesTheLimitedTextAndFollowingEditsUsable) {
  const auto bytes=ForegroundFixture(ForegroundKind::Text,forms::Event("xfa.host.messageBox('full');","full"),3);
  Foreground(bytes,"foreground-full",u"NEW",[](Session& s,std::vector<Raster>& f){ASSERT_TRUE(s.Focus(30,300));Key(s,FWL_VKEY_End);Char(s,'!');EXPECT_EQ(s.Text(),u"old");const auto messages=Messages(s);EXPECT_NE(std::find(messages.begin(),messages.end(),u"full"),messages.end());ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"NEW");EXPECT_EQ(s.Text(),u"NEW");Frame(s,f);});
}

}  // namespace
}  // namespace pdfoxium::integration
