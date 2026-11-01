#include "form_fixture.h"

#include <algorithm>
#include <array>
#include <set>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;

std::string Literal(const std::string& value) {
  std::string out="(";
  for(char ch:value) {
    if(ch=='\r') out+="\\r";
    else if(ch=='\n') out+="\\n";
    else {if(ch=='('||ch==')'||ch=='\\')out+='\\';out+=ch;}
  }
  return out+")";
}
std::u16string Canonical(const std::string& value) {
  std::u16string out;
  for(char ch:value) {if(ch=='\r')out+=u"\r\n";else out+=char16_t(ch);}
  return out;
}
std::vector<uint8_t> Fixture(const std::string& initial="",int list_height=160) {
  std::string options;
  for(int i=0;i<48;++i) {
    const auto number=(i<10?"0":"")+std::to_string(i);
    options+="[(v"+number+") (Item "+number+")] ";
  }
  // The C API has no AcroForm constructor. Supply only its field dictionaries;
  // focus, editing, scroll capture, timers, appearance and saving are public API.
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Annots [6 0 R 7 0 R] /Contents 8 0 R /Resources << /Font << /Cour 5 0 R >> >> >>",
    "<< /Fields [6 0 R 7 0 R] /NeedAppearances true /DA (/Cour 12 Tf 0 g) /DR << /Font << /Cour 5 0 R >> >> >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>",
    "<< /Type /Annot /Subtype /Widget /FT /Ch /T (list) /P 3 0 R /Rect [20 180 220 "+std::to_string(180+list_height)+"] /F 4 /Ff 2097152 /BS << /W 1 >> /MK << /BC [0 0 0] /BG [1 1 1] >> /Opt ["+options+"] /I [0] /V (v00) >>",
    "<< /Type /Annot /Subtype /Widget /FT /Tx /T (text) /P 3 0 R /Rect [20 20 220 140] /F 4 /Ff 4096 /BS << /W 1 >> /MK << /BC [0 0 0] /BG [1 1 1] >> /V "+Literal(initial)+" >>",
    PdfStream("","0.94 g 0 0 400 400 re f 0 0.7 0 rg 260 180 40 160 re f\n")});
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  // Same Courier font budget used by existing form scenarios. Local crops keep
  // a small broken control from disappearing in the page's background average.
  CompareRaster(a,b,artifacts,label,RasterTolerance{18,0,0.08});
  CompareRaster(CropRaster(a,20,60,200,160),CropRaster(b,20,60,200,160),artifacts,label+"-list",RasterTolerance{18,0,0.4});
  CompareRaster(CropRaster(a,20,260,200,120),CropRaster(b,20,260,200,120),artifacts,label+"-text",RasterTolerance{18,0,0.4});
}
void Frames(const std::array<std::vector<Raster>,2>& frames,Artifacts& artifacts,const std::string& label) {
  ASSERT_EQ(frames[0].size(),frames[1].size());
  for(size_t i=0;i<frames[0].size();++i)Compare(frames[0][i],frames[1][i],artifacts,label+std::to_string(i));
}
void Key(Session& s,int key,int flags=0) {EXPECT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,key,flags));}
void Char(Session& s,int ch) {EXPECT_TRUE(s.api.FORM_OnChar(s.form,s.page,ch,0));}
void Selected(Session& s,const std::set<int>& expected) {
  for(int i=0;i<48;++i)EXPECT_EQ(bool(s.api.FORM_IsIndexSelected(s.form,s.page,i)),expected.count(i)!=0)<<i;
}
void Replay(const Library& writer,Session& s,Artifacts& artifacts,const std::string& label,
            const std::u16string& expected,int annotation=1) {
  ASSERT_TRUE(s.api.FORM_ForceToKillFocus(s.form));
  EXPECT_EQ(s.Field(annotation),expected);const auto before=s.Draw();
  const auto saved=SaveDocument(s.api,s.document,artifacts,writer.name()+label);ASSERT_FALSE(saved.empty());
  for(const auto* reader:Implementations()) {
    SCOPED_TRACE(reader->name());Session loaded(reader->api(),saved);ASSERT_TRUE(loaded.valid());
    EXPECT_EQ(loaded.Field(annotation),expected);
    Compare(before,loaded.Draw(),artifacts,writer.name()+reader->name()+label);
  }
}
void ArrowHold(Session& s,bool down,int ticks,int height=160,bool release_outside=false) {
  // CPWL_Wnd places the 12pt scrollbar inside the 1pt field border; its
  // min/max child buttons occupy the first/last 9pt. The list has no caret
  // timer, so these callbacks belong solely to the pressed scrollbar arrow.
  const double y=down?185:180+height-5;
  const auto old=s.host->timers;
  EXPECT_TRUE(s.api.FORM_OnLButtonDown(s.form,s.page,0,212,y));
  std::vector<int> created;
  for(const auto& [id,callback]:s.host->timers)if(!old.count(id))created.push_back(id);
  ASSERT_EQ(created.size(),1u);
  for(int tick=0;tick<ticks;++tick)s.host->Tick(created.front());
  EXPECT_TRUE(s.api.FORM_OnLButtonUp(s.form,s.page,0,release_outside?230:212,release_outside?170:y));
  EXPECT_FALSE(s.host->timers.count(created.front()));
  const auto stable=s.Draw();s.host->Tick(created.front());
  EXPECT_EQ(s.Draw().pixels,stable.pixels)<<"Released timers must not scroll again";
}

TEST(CoveragePwlHistoryScroll, ScrollbarArrowRepeatPreservesSelectionAndClampsAtBothLimits) {
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());Session s(library->api(),Fixture());ASSERT_TRUE(s.valid());
    ASSERT_TRUE(s.Focus(30,330));Selected(s,{0});frames[backend].push_back(s.Draw());
    ArrowHold(s,true,2,160,true);Selected(s,{0});frames[backend].push_back(s.Draw());
    EXPECT_NE(CropRaster(frames[backend][0],22,62,180,150).pixels,CropRaster(frames[backend][1],22,62,180,150).pixels);
    // One initial step plus two repeats advances three complete item heights.
    s.Click(30,334);Selected(s,{3});
    ArrowHold(s,true,80);frames[backend].push_back(s.Draw());
    ArrowHold(s,true,2);EXPECT_EQ(s.Draw().pixels,frames[backend].back().pixels);
    s.Click(30,184);Selected(s,{47});
    ArrowHold(s,false,80);frames[backend].push_back(s.Draw());
    ArrowHold(s,false,2);EXPECT_EQ(s.Draw().pixels,frames[backend].back().pixels);
    s.Click(30,334);Selected(s,{0});
    Replay(*library,s,artifacts,"-arrow-repeat",u"v00",0);
    EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);++backend;
  }
  Frames(frames,artifacts,"arrow-repeat-");
}

TEST(CoveragePwlHistoryScroll, CapturedThumbDragClampsOutsideTrackAndReleaseEndsDragging) {
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());Session s(library->api(),Fixture());ASSERT_TRUE(s.valid());
    ASSERT_TRUE(s.Focus(30,330));frames[backend].push_back(s.Draw());
    // Initial thumb starts immediately below the top arrow; y323 is inside
    // its first six points independently of the total 48-item content height.
    ASSERT_TRUE(s.api.FORM_OnLButtonDown(s.form,s.page,0,212,323));
    const auto pressed=s.Draw();
    s.api.FORM_OnMouseMove(s.form,s.page,FWL_EVENTFLAG_LeftButtonDown,212,322.5);
    EXPECT_EQ(s.Draw().pixels,pressed.pixels)<<"Sub-point movement is ignored";
    // Stay inside the PDF widget while crossing the scrollbar's track limits;
    // PageView routes motion by widget hit, then PWL routes its captured child.
    s.api.FORM_OnMouseMove(s.form,s.page,FWL_EVENTFLAG_LeftButtonDown,212,181);
    frames[backend].push_back(s.Draw());Selected(s,{0});
    EXPECT_NE(frames[backend][0].pixels,frames[backend][1].pixels);
    s.api.FORM_OnMouseMove(s.form,s.page,FWL_EVENTFLAG_LeftButtonDown,212,339);
    frames[backend].push_back(s.Draw());
    EXPECT_EQ(CropRaster(frames[backend][0],22,62,180,150).pixels,CropRaster(frames[backend][2],22,62,180,150).pixels);
    ASSERT_TRUE(s.api.FORM_OnLButtonUp(s.form,s.page,0,212,350));
    const auto released=s.Draw();s.api.FORM_OnMouseMove(s.form,s.page,0,212,200);
    EXPECT_EQ(s.Draw().pixels,released.pixels);
    s.Click(30,334);Selected(s,{0});Replay(*library,s,artifacts,"-thumb-capture",u"v00",0);++backend;
  }
  Frames(frames,artifacts,"thumb-capture-");
}

TEST(CoveragePwlHistoryScroll, TrackPagingAndShortScrollbarButtonsRetainValuesAndDrawTheirGeometry) {
  Artifacts artifacts;
  for(int height:{18,24,36,160}) {
    SCOPED_TRACE(height);std::array<std::vector<Raster>,2> frames;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),Fixture("",height));ASSERT_TRUE(s.valid());
      ASSERT_TRUE(s.Focus(30,180+height-5));frames[backend].push_back(s.Draw());Selected(s,{0});
      // A point near the lower track pages away from the initial top thumb.
      // Small windows instead exercise compressed arrow/hidden thumb layout.
      s.Click(212,180+height/3.0);Selected(s,{0});frames[backend].push_back(s.Draw());
      if(height==160){EXPECT_NE(frames[backend][0].pixels,frames[backend][1].pixels);}
      s.Click(212,180+height-5);Selected(s,{0});frames[backend].push_back(s.Draw());
      ASSERT_TRUE(s.api.FORM_ForceToKillFocus(s.form));EXPECT_EQ(s.Field(0),u"v00");
      EXPECT_TRUE(s.host->timers.empty());++backend;
    }
    Frames(frames,artifacts,"track-height"+std::to_string(height)+"-");
  }
}

enum class Edit {InsertReturn,BackspaceWord,DeleteWord,BackspaceReturn,DeleteReturn,Clear,ReplaceWord,ReplaceReturn};
TEST(CoveragePwlHistoryScroll, TypedWordsReturnsAndDeletionEachUndoAndRedoAsOneMeaningfulOperation) {
  struct Row {const char* name;const char* initial;Edit edit;std::u16string expected;};
  const std::array<Row,8> rows={{{"insert-return","ABCD",Edit::InsertReturn,u"AB\r\nCD"},
    {"backspace-word","ABCD",Edit::BackspaceWord,u"ACD"},
    {"delete-word","ABCD",Edit::DeleteWord,u"ABD"},
    {"backspace-return","AB\rCD",Edit::BackspaceReturn,u"ABCD"},
    {"delete-return","AB\rCD",Edit::DeleteReturn,u"ABCD"},
    {"clear-selected","AB\rCD",Edit::Clear,u""},
    {"replace-selected-word","AB\rCD",Edit::ReplaceWord,u"Z"},
    {"replace-selected-return","AB\rCD",Edit::ReplaceReturn,u"\r\n"}}};
  Artifacts artifacts;
  for(const auto& row:rows) {
    SCOPED_TRACE(row.name);std::array<std::vector<Raster>,2> frames;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),Fixture(row.initial));ASSERT_TRUE(s.valid());
      ASSERT_TRUE(s.Focus(30,125));const auto initial=Canonical(row.initial);EXPECT_EQ(s.Text(),initial);
      EXPECT_FALSE(s.api.FORM_CanUndo(s.form,s.page));
      Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
      if(row.edit==Edit::BackspaceReturn){Key(s,FWL_VKEY_Down);Key(s,FWL_VKEY_Home);}
      else if(row.edit==Edit::DeleteReturn)Key(s,FWL_VKEY_End);
      else if(row.edit==Edit::Clear||row.edit==Edit::ReplaceWord||row.edit==Edit::ReplaceReturn)s.api.FORM_SelectAllText(s.form,s.page);
      else {Key(s,FWL_VKEY_Right);Key(s,FWL_VKEY_Right);}
      frames[backend].push_back(s.Draw());
      switch(row.edit) {
        case Edit::InsertReturn:case Edit::ReplaceReturn:Char(s,13);break;
        case Edit::BackspaceWord:case Edit::BackspaceReturn:case Edit::Clear:Char(s,8);break;
        case Edit::DeleteWord:case Edit::DeleteReturn:Key(s,FWL_VKEY_Delete);break;
        case Edit::ReplaceWord:Char(s,'Z');break;
      }
      EXPECT_EQ(s.Text(),row.expected);frames[backend].push_back(s.Draw());
      ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),initial);
      EXPECT_FALSE(s.api.FORM_CanUndo(s.form,s.page));frames[backend].push_back(s.Draw());
      ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),row.expected);
      EXPECT_FALSE(s.api.FORM_CanRedo(s.form,s.page));frames[backend].push_back(s.Draw());
      Replay(*library,s,artifacts,row.name,row.expected);++backend;
    }
    Frames(frames,artifacts,std::string(row.name)+"-");
  }
}

TEST(CoveragePwlHistoryScroll, MultilineTypingHistoryBranchesAfterUndoAndFiltersControlCharacters) {
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t backend=0;
  const std::vector<std::u16string> states={u"",u"A",u"AB",u"AB\r\n",u"AB\r\nC",u"AB\r\nCD",u"AB\r\nCD\r\n",u"AB\r\nCD\r\nE"};
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());Session s(library->api(),Fixture());ASSERT_TRUE(s.valid());ASSERT_TRUE(s.Focus(30,125));
    size_t i=0;for(int ch:std::array<int,7>{'A','B',13,'C','D',13,'E'}){Char(s,ch);EXPECT_EQ(s.Text(),states[++i]);}
    for(size_t step=states.size()-1;step>0;--step){ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),states[step-1]);}
    EXPECT_FALSE(s.api.FORM_Undo(s.form,s.page));
    for(size_t step=1;step<states.size();++step){ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),states[step]);}
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));
    Char(s,'Z');EXPECT_EQ(s.Text(),u"AB\r\nCDZ");EXPECT_FALSE(s.api.FORM_CanRedo(s.form,s.page));
    // LF/Delete reach CPWL_Edit's character filter. Escape is intercepted by
    // CFFL_TextField first and destroys its uncommitted editing window instead.
    for(int ch:{10,127}){s.api.FORM_OnChar(s.form,s.page,ch,0);EXPECT_EQ(s.Text(),u"AB\r\nCDZ");}
    frames[backend].push_back(s.Draw());Replay(*library,s,artifacts,"-branched-typing",u"AB\r\nCDZ");
    s.Click(30,125);ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"pending\rchange");
    EXPECT_EQ(s.Text(),u"pending\r\nchange");Char(s,27);
    EXPECT_EQ(s.Field(1),u"AB\r\nCDZ");frames[backend].push_back(s.Draw());
    s.Click(30,125);EXPECT_EQ(s.Text(),u"AB\r\nCDZ");
    EXPECT_FALSE(s.api.FORM_CanUndo(s.form,s.page));
    ASSERT_TRUE(s.api.FORM_ForceToKillFocus(s.form));++backend;
  }
  Frames(frames,artifacts,"branched-typing-");
}

TEST(CoveragePwlHistoryScroll, MultilineShiftNavigationAndGroupedReplacementPreserveWholeLineSelections) {
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t backend=0;
  const std::u16string initial=u"ABCD\r\nEFGH\r\nIJKL";
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());Session s(library->api(),Fixture("ABCD\rEFGH\rIJKL"));ASSERT_TRUE(s.valid());ASSERT_TRUE(s.Focus(30,125));
    Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
    Key(s,FWL_VKEY_Down,FWL_EVENTFLAG_ShiftKey);EXPECT_EQ(s.Text(true),u"ABCD\r\n");
    Key(s,FWL_VKEY_Down,FWL_EVENTFLAG_ShiftKey);EXPECT_EQ(s.Text(true),u"ABCD\r\nEFGH\r\n");
    frames[backend].push_back(s.Draw());
    Key(s,FWL_VKEY_Up,FWL_EVENTFLAG_ShiftKey);EXPECT_EQ(s.Text(true),u"ABCD\r\n");
    s.Replace(u"One\rTwo\r");EXPECT_EQ(s.Text(),u"One\r\nTwo\r\nEFGH\r\nIJKL");
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),initial);
    ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),u"One\r\nTwo\r\nEFGH\r\nIJKL");
    Key(s,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey);Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ShiftKey);
    EXPECT_EQ(s.Text(true),u"IJKL");frames[backend].push_back(s.Draw());
    s.Replace(u"Last");EXPECT_EQ(s.Text(),u"One\r\nTwo\r\nEFGH\r\nLast");
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),u"One\r\nTwo\r\nEFGH\r\nIJKL");
    ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));
    Replay(*library,s,artifacts,"-line-selection",u"One\r\nTwo\r\nEFGH\r\nLast");++backend;
  }
  Frames(frames,artifacts,"line-selection-");
}
}  // namespace
}  // namespace pdfoxium::integration
