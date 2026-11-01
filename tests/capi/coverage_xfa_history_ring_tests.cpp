#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using States=std::vector<std::u16string>;
using Frames=std::vector<Raster>;
using Action=std::function<void(Session&,States&,Frames&)>;
constexpr size_t kStoredOperations=128;
constexpr size_t kRetainedOperations=kStoredOperations-1;
// FDE uses 128 slots with the next insertion slot as a sentinel. Therefore a
// full ring exposes 127 undoable records, independently of text/gap capacity.
std::string Xml(const std::u16string& text) {
  std::string out;
  for(char16_t c:text) {
    if(c=='&')out+="&amp;";
    else if(c=='<')out+="&lt;";
    else if(c=='\n')out+="&#10;";
    else if(c=='\r')out+="&#13;";
    else out+=char(c);
  }
  return out;
}
std::vector<uint8_t> Fixture(const std::u16string& initial) {
  const std::string clean=forms::Event("if(typeof this.rawValue==='string')this.rawValue=this.rawValue.replace(/^\\n/,'');","initialize");
  const std::string preamble="<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
  const std::string config="<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
  std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" locale=\"en_US\" restoreState=\"auto\"><pageSet><pageArea><occur min=\"1\" max=\"-1\"/><medium short=\"320pt\" long=\"300pt\"/><contentArea w=\"320pt\" h=\"300pt\"/></pageArea></pageSet>"
    "<field name=\"input\" x=\"8pt\" y=\"8pt\" w=\"280pt\" h=\"210pt\"><bind match=\"once\"/><ui><textEdit multiLine=\"1\"/></ui><font typeface=\"Helvetica\" size=\"10pt\"/><para hAlign=\"left\" vAlign=\"top\" lineHeight=\"12pt\"/><value><text>"+Xml(initial)+"</text></value>"+clean+"</field>"
    "<field name=\"sink\" x=\"8pt\" y=\"260pt\" w=\"280pt\" h=\"22pt\"><bind match=\"none\"/><ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"10pt\"/><value><text>sink</text></value>"+clean+"</field>"
    "<draw x=\"300pt\" y=\"280pt\" w=\"10pt\" h=\"10pt\"><value><rectangle><fill><color value=\"0,128,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw></subform></template>";
  // Match the established XFA fixture convention for the oracle's XML reader.
  for(size_t i=0;i<xml.size();) {
    const size_t n=xml.compare(i,9,"<![CDATA[")==0?9:xml.compare(i,3,"]]>")==0?3:0;
    if(!n){++i;continue;}
    const auto offset=preamble.size()+config.size()+i;
    if(offset/1024!=(offset+n-1)/1024){const auto pad=1024-offset%1024;xml.insert(i,pad,' ');i+=pad;}
    i+=n;
  }
  std::vector<std::string> objects={
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 300] /Resources << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> >>",
    "<< /Fields [] /DR << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    PdfStream("",preamble),PdfStream("",config),PdfStream("",xml),
    PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root xfa:dataNode=\"dataGroup\"/></xfa:data></xfa:datasets>"),
    PdfStream("","</xdp:xdp>"),"null"};
  auto font=forms::PinnedXfaArimoFontObjects();
  for(auto& object:font)objects.push_back(std::move(object));
  return PdfFixture(objects);
}
std::u16string Text(Session& s) {
  const auto value=s.Text();std::u16string out;
  for(size_t i=0;i<value.size();++i) {
    if(value[i]=='\r'&&i+1<value.size()&&value[i+1]=='\n')continue;
    out+=value[i];
  }
  return out;
}
void Key(Session& s,int code,int flags=0) {s.api.FORM_OnKeyDown(s.form,s.page,code,flags);}
void End(Session& s){Key(s,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey);}
void Home(Session& s){Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);}
void Char(Session& s,char16_t c) {ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,c,0));}
void Snapshot(Session& s,Frames& frames) {
  // Put the transient caret/scroll viewport at the same logical position;
  // focus and history stay alive throughout all undo/redo assertions.
  Home(s);
  const auto text=Text(s);
  const bool undo=s.api.FORM_CanUndo(s.form,s.page),redo=s.api.FORM_CanRedo(s.form,s.page);
  // Manual host scheduling lets both live edits paint in the same caret blink
  // phase. Undo retains the native last-glyph caret height, while Rust updates
  // it immediately; that transient caret geometry is independent of history.
  // This fixture has no script timers or scrollbar capture timers.
  ASSERT_EQ(s.host->timers.size(),1u);
  const int caret_timer=s.host->timers.begin()->first;
  s.host->Tick(caret_timer);
  frames.push_back(s.DrawForComparison(320,300));
  ASSERT_TRUE(s.host->timers.count(caret_timer));
  s.host->Tick(caret_timer);
  EXPECT_EQ(Text(s),text);
  EXPECT_EQ(bool(s.api.FORM_CanUndo(s.form,s.page)),undo);
  EXPECT_EQ(bool(s.api.FORM_CanRedo(s.form,s.page)),redo);
}
void Insert(Session& s,States& states,size_t count,bool newline=false) {
  End(s);
  for(size_t i=0;i<count;++i) {
    const char16_t c=newline&&i%16==15?u'\n':char16_t(u'A'+i%26);
    ASSERT_NO_FATAL_FAILURE(Char(s,c=='\n'?u'\r':c));
    states.push_back(states.back()+c);
    ASSERT_EQ(Text(s),states.back())<<"insert "<<i;
  }
}
void UndoSteps(Session& s,const States& states,size_t& position,size_t count) {
  ASSERT_LE(count,position);
  for(size_t i=0;i<count;++i) {
    ASSERT_TRUE(s.api.FORM_CanUndo(s.form,s.page));
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));
    --position;
    ASSERT_EQ(Text(s),states[position])<<"undo "<<i;
  }
}
void RedoSteps(Session& s,const States& states,size_t& position,size_t count) {
  ASSERT_LT(position+count,states.size());
  for(size_t i=0;i<count;++i) {
    ASSERT_TRUE(s.api.FORM_CanRedo(s.form,s.page));
    ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));
    ++position;
    ASSERT_EQ(Text(s),states[position])<<"redo "<<i;
  }
}
void RoundTripHistory(Session& s,const States& states,size_t earliest,Frames& frames) {
  size_t position=states.size()-1;
  ASSERT_LE(earliest,position);
  const auto count=position-earliest;
  ASSERT_LE(count,kRetainedOperations);
  EXPECT_FALSE(s.api.FORM_CanRedo(s.form,s.page));
  Snapshot(s,frames);
  ASSERT_NO_FATAL_FAILURE(UndoSteps(s,states,position,count));
  EXPECT_EQ(position,earliest);
  EXPECT_FALSE(s.api.FORM_CanUndo(s.form,s.page));
  EXPECT_FALSE(s.api.FORM_Undo(s.form,s.page));
  EXPECT_EQ(Text(s),states[earliest]);
  Snapshot(s,frames);
  ASSERT_NO_FATAL_FAILURE(RedoSteps(s,states,position,count));
  EXPECT_FALSE(s.api.FORM_CanRedo(s.form,s.page));
  EXPECT_FALSE(s.api.FORM_Redo(s.form,s.page));
  EXPECT_EQ(Text(s),states.back());
  Snapshot(s,frames);
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,RasterTolerance::ConnectedError(1.0));
}
void Settle(Session& s,const std::u16string& expected) {
  ASSERT_TRUE(s.Focus(12,16));
  EXPECT_EQ(Text(s),expected);
  Home(s);
  ASSERT_TRUE(s.Focus(12,268));
  s.api.FORM_ForceToKillFocus(s.form);
  s.Click(315,295);s.api.FORM_OnMouseMove(s.form,s.page,0,315,295);
}
void Workflow(const std::string& name,const std::u16string& initial,const Action& action) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<Frames,2> frames;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(name+" writer="+writer->name());
    Session s(writer->api(),Fixture(initial),true,true);
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    ASSERT_TRUE(s.Focus(12,16));ASSERT_EQ(Text(s),initial);
    States states{initial};
    ASSERT_NO_FATAL_FAILURE(action(s,states,frames[backend]));
    ASSERT_EQ(Text(s),states.back());
    // Every exhausted/wrapped history remains usable for one more real edit.
    End(s);ASSERT_NO_FATAL_FAILURE(Char(s,u'Z'));
    const auto final=states.back()+u'Z';
    ASSERT_EQ(Text(s),final);
    ASSERT_NO_FATAL_FAILURE(Settle(s,final));
    const auto stable=s.DrawForComparison(320,300);
    const auto input_image=CropLogicalRaster(stable,8,8,280,210);
    forms::ExpectInk(input_image,0,0,input_image.width,input_image.height);
    frames[backend].push_back(stable);
    const auto saved=SaveDocument(s.api,s.document,artifacts,name+"-"+writer->name());
    ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE("reader="+reader->name());
      Session reloaded(reader->api(),saved,true,true);
      ASSERT_TRUE(reloaded.valid());ASSERT_TRUE(reloaded.xfa_loaded);
      ASSERT_NO_FATAL_FAILURE(Settle(reloaded,final));
      EXPECT_TRUE(reloaded.host->callbacks_valid);EXPECT_FALSE(reloaded.host->callback_error);
      Compare(stable,reloaded.DrawForComparison(320,300),artifacts,name+"-reload-"+writer->name()+"-"+reader->name());
    }
    EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
    ++backend;
  }
  ASSERT_EQ(frames[0].size(),frames[1].size());
  for(size_t i=0;i<frames[0].size();++i)Compare(frames[0][i],frames[1][i],artifacts,name+"-stage-"+std::to_string(i));
}
void Inserts(const std::string& name,size_t count,bool newline=false) {
  Workflow(name,u"",[=](Session& s,States& states,Frames& frames){
    ASSERT_NO_FATAL_FAILURE(Insert(s,states,count,newline));
    ASSERT_NO_FATAL_FAILURE(RoundTripHistory(s,states,count>kRetainedOperations?count-kRetainedOperations:0,frames));
  });
}
void Branch(const std::string& name,size_t count,size_t undo,size_t redo,bool replace=false) {
  Workflow(name,u"",[=](Session& s,States& states,Frames& frames){
    ASSERT_NO_FATAL_FAILURE(Insert(s,states,count));
    size_t position=count;
    ASSERT_NO_FATAL_FAILURE(UndoSteps(s,states,position,undo));
    ASSERT_NO_FATAL_FAILURE(RedoSteps(s,states,position,redo));
    Snapshot(s,frames);
    std::u16string branch;
    if(replace) {
      ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
      EXPECT_EQ(s.Text(true),states[position]);
      s.Replace(u"branched");branch=u"branched";
    } else {
      End(s);ASSERT_NO_FATAL_FAILURE(Char(s,u'Q'));branch=states[position]+u'Q';
    }
    states.resize(position+1);states.push_back(branch);
    ASSERT_EQ(Text(s),branch);
    EXPECT_FALSE(s.api.FORM_CanRedo(s.form,s.page));
    // All 128 stored records survive until overwritten or discarded as redo.
    // Moving the insertion sentinel can make the previously blocked oldest
    // record available again. Bound the new path independently by both the
    // original stored prefix and the 127-operation public undo capacity.
    const size_t stored_begin=count>kStoredOperations?count-kStoredOperations:0;
    const size_t path_length=states.size()-1;
    const size_t visible_begin=path_length>kRetainedOperations?path_length-kRetainedOperations:0;
    ASSERT_NO_FATAL_FAILURE(RoundTripHistory(s,states,std::max(stored_begin,visible_begin),frames));
  });
}

TEST(CoverageXfaHistoryRing, OneHundredTwentySevenSeparateCharactersCanAllBeUndone) {
  Inserts("OneHundredTwentySevenSeparateCharactersCanAllBeUndone",127,false);
}

TEST(CoverageXfaHistoryRing, TheOneHundredTwentyEighthEditRetainsTheFirstCharacterAtTheUndoCutoff) {
  Inserts("TheOneHundredTwentyEighthEditRetainsTheFirstCharacterAtTheUndoCutoff",128,false);
}

TEST(CoverageXfaHistoryRing, OneHundredTwentyNineEditsRetainExactlyTwoOldCharacters) {
  Inserts("OneHundredTwentyNineEditsRetainExactlyTwoOldCharacters",129,false);
}

TEST(CoverageXfaHistoryRing, TwoCompleteRingWrapsStillExposeExactlyTheLastOneHundredTwentySevenEdits) {
  Inserts("TwoCompleteRingWrapsStillExposeExactlyTheLastOneHundredTwentySevenEdits",257,false);
}

TEST(CoverageXfaHistoryRing, NewlineCharacterRecordsCrossTheRingWithoutLosingParagraphBoundaries) {
  Inserts("NewlineCharacterRecordsCrossTheRingWithoutLosingParagraphBoundaries",160,true);
}

TEST(CoverageXfaHistoryRing, BranchingWithinTheTailDropsOnlyTheOutstandingRedoRecords) {
  Branch("BranchingWithinTheTailDropsOnlyTheOutstandingRedoRecords",160,16,0,false);
}

TEST(CoverageXfaHistoryRing, BranchingAcrossTheWrappedHeadClearsBothDiscardedHistorySegments) {
  Branch("BranchingAcrossTheWrappedHeadClearsBothDiscardedHistorySegments",140,32,0,false);
}

TEST(CoverageXfaHistoryRing, BranchingAtTheEndOfTheSecondRingRetainsItsUndoCutoff) {
  Branch("BranchingAtTheEndOfTheSecondRingRetainsItsUndoCutoff",256,1,0,false);
}

TEST(CoverageXfaHistoryRing, BranchingAfterTheUndoCutoffRetainsTheOldestStoredPrefix) {
  Branch("BranchingAfterTheUndoCutoffRetainsTheOldestStoredPrefix",257,127,0,false);
}

TEST(CoverageXfaHistoryRing, RedoingPartOfTheWrappedHistoryBeforeBranchingRetainsOnlyTheNewPath) {
  Branch("RedoingPartOfTheWrappedHistoryBeforeBranchingRetainsOnlyTheNewPath",160,100,40,false);
}

TEST(CoverageXfaHistoryRing, SelectionReplacementAfterAWrappedUndoCreatesOneNewBranchRecord) {
  Branch("SelectionReplacementAfterAWrappedUndoCreatesOneNewBranchRecord",140,40,0,true);
}

TEST(CoverageXfaHistoryRing, RepeatedBackspaceRecordsRestoreOnlyTheRetainedDeletionSuffix) {
  std::u16string initial;for(int i=0;i<260;++i)initial+=char16_t(u'A'+i%26);
  Workflow("backspace-ring",initial,[](Session& s,States& states,Frames& frames){
    End(s);
    for(size_t i=0;i<160;++i) {
      ASSERT_NO_FATAL_FAILURE(Char(s,u'\b'));
      states.push_back(states.back().substr(0,states.back().size()-1));
      ASSERT_EQ(Text(s),states.back());
    }
    ASSERT_NO_FATAL_FAILURE(RoundTripHistory(s,states,33,frames));
  });
}
TEST(CoverageXfaHistoryRing, RepeatedForwardDeleteRecordsRestoreTheRetainedPrefixInOrder) {
  std::u16string initial;for(int i=0;i<260;++i)initial+=char16_t(u'A'+i%26);
  Workflow("forward-delete-ring",initial,[](Session& s,States& states,Frames& frames){
    Home(s);
    for(size_t i=0;i<160;++i) {
      Key(s,FWL_VKEY_Delete);
      states.push_back(states.back().substr(1));
      ASSERT_EQ(Text(s),states.back());
    }
    ASSERT_NO_FATAL_FAILURE(RoundTripHistory(s,states,33,frames));
  });
}
TEST(CoverageXfaHistoryRing, AlternatingInsertAndDeleteRecordsRemainSeparateAcrossTheRing) {
  Workflow("mixed-ring",u"seed",[](Session& s,States& states,Frames& frames){
    End(s);
    for(int i=0;i<90;++i) {
      const auto c=char16_t(u'A'+i%26);
      ASSERT_NO_FATAL_FAILURE(Char(s,c));states.push_back(states.back()+c);ASSERT_EQ(Text(s),states.back());
      ASSERT_NO_FATAL_FAILURE(Char(s,u'\b'));states.push_back(states.back().substr(0,states.back().size()-1));ASSERT_EQ(Text(s),states.back());
    }
    ASSERT_NO_FATAL_FAILURE(RoundTripHistory(s,states,53,frames));
  });
}
void Replacements(const std::string& name,bool partial,bool multiline) {
  Workflow(name,u"seed",[=](Session& s,States& states,Frames& frames){
    for(int i=0;i<140;++i) {
      std::u16string replacement;
      if(partial) {
        Home(s);Key(s,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey);
        EXPECT_EQ(s.Text(true),states.back().substr(0,1));
        replacement=std::u16string(1,char16_t(u'A'+i%26));
        const auto expected=replacement+states.back().substr(1);
        s.Replace(replacement);states.push_back(expected);
      } else {
        ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
        EXPECT_EQ(s.Text(true),states.back());
        replacement=(multiline?u"Top\n":u"word")+std::u16string(1,char16_t(u'A'+i%26));
        s.Replace(replacement);states.push_back(replacement);
      }
      ASSERT_EQ(Text(s),states.back());
    }
    // Each selected replacement is one ReplaceOperation, not independent
    // delete and insert records: 127 undos must land on replacement number13.
    ASSERT_NO_FATAL_FAILURE(RoundTripHistory(s,states,13,frames));
  });
}
TEST(CoverageXfaHistoryRing, WholeSelectionReplacementIsOneRecordEvenAfterTheRingWraps) {
  Replacements("whole-replacement-ring",false,false);
}
TEST(CoverageXfaHistoryRing, PartialSelectionReplacementPreservesTheUnselectedSuffixThroughHistoryWrap) {
  Replacements("partial-replacement-ring",true,false);
}
TEST(CoverageXfaHistoryRing, MultilineSelectedReplacementsRestoreBothParagraphsAsOneRecord) {
  Replacements("multiline-replacement-ring",false,true);
}
}  // namespace
}  // namespace pdfoxium::integration
