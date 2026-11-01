// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <memory>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using Frames=std::vector<Raster>;
using RGB=std::array<int,3>;
const RGB green{0,128,0},blue{0,0,255},cyan{0,255,255},white{255,255,255};
struct State {int rows=6,height=40,pages=2;bool visible=true,paired=false;};
struct Step {std::string script;State state;std::u16string edit{};};
struct Spec {std::string name;std::vector<int> areas{100,100};int nesting=0;std::vector<Step> steps;};
std::string Paint(const std::string& name,RGB color,int width,int height,const std::string& attrs="") {
  return "<draw name=\""+name+"\" w=\""+std::to_string(width)+"pt\" h=\""+std::to_string(height)+"pt\" "+attrs+"><value><rectangle><fill><color value=\""+
    std::to_string(color[0])+","+std::to_string(color[1])+","+std::to_string(color[2])+"\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
}
std::vector<uint8_t> Fixture(const Spec& spec) {
  std::string areas;
  for(size_t i=0;i<spec.areas.size();++i)areas+="<contentArea name=\"area"+std::to_string(i)+"\" x=\""+std::to_string(10+120*i)+"pt\" y=\"10pt\" w=\"100pt\" h=\""+std::to_string(spec.areas[i])+"pt\"/>";
  const auto pages="<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"360pt\" long=\"240pt\"/>"+areas+
    Paint("stationery",cyan,8,8,"x=\"340pt\" y=\"220pt\"")+"</pageArea></pageSet>";
  std::string controls="<subform name=\"controls\" layout=\"position\" w=\"100pt\" h=\"20pt\"><field name=\"note\" x=\"0pt\" y=\"0pt\" w=\"60pt\" h=\"18pt\"><bind match=\"none\"/><ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"10pt\"/><value><text>Seed</text></value></field>";
  for(size_t i=0;i<spec.steps.size();++i){
    const auto script="function eq(a,b){if(a!==b)throw 'expected '+b+' got '+a;}try{var r=xfa.form.root,l=xfa.layout,m=r.stream._row;"+spec.steps[i].script+";xfa.host.messageBox('PASS');}catch(e){xfa.host.messageBox('FAIL:'+String(e));}";
    controls+="<field name=\"action"+std::to_string(i)+"\" x=\""+std::to_string(64+7*i)+"pt\" y=\"0pt\" w=\"6pt\" h=\"14pt\"><bind match=\"none\"/><ui><button/></ui><border><edge presence=\"hidden\"/></border>"+forms::Event(script,"click")+"</field>";
  }
  controls+="</subform>";
  std::string rows="<subform name=\"row\" layout=\"position\" w=\"60pt\" h=\"40pt\"><bind match=\"none\"/><occur min=\"0\" max=\"8\" initial=\"6\"/><keep intact=\"contentArea\"/>"+
    Paint("upper",green,60,20)+Paint("lower",blue,60,20,"y=\"20pt\"")+"</subform>";
  // Atomic children inside splittable wrappers exercise continuation reuse
  // without depending on native's corrupt parentless inner-split raster.
  std::string body="<subform name=\"stream\" layout=\"tb\" w=\"60pt\"><keep intact=\"none\"/>";
  body+=rows+"</subform>";
  for(int level=0;level<spec.nesting;++level)body="<subform name=\"outer"+std::to_string(level)+"\" layout=\"tb\" w=\"60pt\"><keep intact=\"none\"/>"+body+"</subform>";
  // Explicit SOM alias avoids changing scripts when an extra flow level is
  // added. Resolve stream recursively through the named wrapper chain.
  std::string prefix;
  for(int level=spec.nesting-1;level>=0;--level)prefix+="outer"+std::to_string(level)+".";
  if(spec.nesting){
    const std::string from="r.stream";
    const std::string to="r."+prefix+"stream";
    for(size_t at=0;(at=controls.find(from,at))!=std::string::npos;at+=to.size())controls.replace(at,from.size(),to);
  }
  const auto xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"tb\" restoreState=\"auto\" locale=\"en_US\">"+pages+controls+body+"</subform></template>";
  // Public C API has no XFA-template constructor; input, callbacks, layout,
  // rendering and both save modes below all use the public ABI.
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 360 240] >>","<< /Fields [] /DA (/Helv 10 Tf 0 g) /DR << /Font << /Helv 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
    PdfStream("",xml),PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),PdfStream("","</xdp:xdp>"),"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"});
}
struct Loaded {
  forms::XfaCallbacks callbacks;Session s;
  Loaded(const Library& library,const std::vector<uint8_t>& bytes):s(library.api(),bytes,true,true,2,[this](forms::FormHost& h){callbacks.Configure(h);}){}
};
void Refresh(Session& s){
  for(size_t i=0;i<s.host->pages.size();++i)s.ClosePage(static_cast<int>(i));
  s.page=s.host->Page(0);ASSERT_NE(s.page,nullptr);
}
void Value(Session& s,const std::u16string& expected){
  ASSERT_TRUE(s.Focus(20,18));EXPECT_EQ(s.Text(),expected);
  s.api.FORM_ForceToKillFocus(s.form);
  s.api.FORM_OnMouseMove(s.form,s.page,0,355,180);
}
void Edit(Session& s,const std::u16string& value){
  ASSERT_TRUE(s.Focus(20,18));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
  s.Replace(value);EXPECT_EQ(s.Text(),value);s.api.FORM_ForceToKillFocus(s.form);
  ASSERT_NO_FATAL_FAILURE(Refresh(s));
  ASSERT_NO_FATAL_FAILURE(Value(s,value));
}
void Trigger(Session& s,size_t index){
  s.api.FORM_ForceToKillFocus(s.form);s.host->alerts.clear();
  const double x=77+7*index;
  ASSERT_EQ(s.api.FPDFPage_HasFormFieldAtPoint(s.form,s.page,x,15),FPDF_FORMFIELD_XFA_PUSHBUTTON);
  s.api.FORM_OnMouseMove(s.form,s.page,0,x,15);
  ASSERT_TRUE(s.api.FORM_OnLButtonDown(s.form,s.page,0,x,15));
  ASSERT_TRUE(s.api.FORM_OnLButtonUp(s.form,s.page,0,x,15));
  ASSERT_NO_FATAL_FAILURE(Refresh(s));
  ASSERT_EQ(s.host->alerts.size(),1u);EXPECT_EQ(std::get<0>(s.host->alerts.front()),u"PASS");
  EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
}
Frames Render(Session& s){
  Frames out;const int pages=s.api.FPDF_GetPageCount(s.document);EXPECT_GT(pages,0);EXPECT_LE(pages,8);
  if(pages<1||pages>8)return out;
  for(int page=0;page<pages;++page)out.push_back(s.Draw(360,240,0,0,0,0,0,0,page));
  return out;
}
size_t Count(const Frames& images,RGB color){
  size_t count=0;for(const auto& image:images)for(size_t at=0;at<image.pixels.size();at+=4)
    count+=image.pixels[at]==color[2]&&image.pixels[at+1]==color[1]&&image.pixels[at+2]==color[0];
  return count;
}
void Pixel(const Frames& images,int page,int x,int y,RGB color){
  ASSERT_LT(static_cast<size_t>(page),images.size());const auto& image=images[page];
  ASSERT_GE(x,0);ASSERT_LT(x,image.width);ASSERT_GE(y,0);ASSERT_LT(y,image.height);
  const auto at=(static_cast<size_t>(y)*image.width+x)*4;
  EXPECT_EQ(image.pixels[at],color[2])<<page<<":"<<x<<","<<y;
  EXPECT_EQ(image.pixels[at+1],color[1])<<page<<":"<<x<<","<<y;
  EXPECT_EQ(image.pixels[at+2],color[0])<<page<<":"<<x<<","<<y;
}
void Inspect(const Frames& images,const Spec& spec,State state){
  ASSERT_EQ(images.size(),static_cast<size_t>(state.pages));
  const auto note=CropRaster(images[0],10,10,60,18);
  size_t text_ink=0;
  for(size_t at=0;at<note.pixels.size();at+=4)
    text_ink+=note.pixels[at]<200&&note.pixels[at+1]<200&&note.pixels[at+2]<200;
  EXPECT_GT(text_ink,20u)<<"The edited unbound value must remain visible";
  const auto ink=state.visible?static_cast<size_t>(state.rows*60*state.height/2):0u;
  EXPECT_EQ(Count(images,green),ink);EXPECT_EQ(Count(images,blue),ink);
  EXPECT_EQ(Count(images,cyan),64u*images.size());
  for(int page=0;page<state.pages;++page){Pixel(images,page,344,224,cyan);Pixel(images,page,355,180,white);}
  if(!state.visible)return;
  int page=0,area=0,y=20;
  for(int row=0;row<state.rows;){
    const int group=state.paired?2:1;
    if(y+group*state.height>spec.areas[area]){
      y=0;if(++area==static_cast<int>(spec.areas.size())){area=0;++page;}
      continue;
    }
    for(int n=0;n<group&&row<state.rows;++n,++row){
      Pixel(images,page,15+120*area,10+y+state.height/4,green);
      Pixel(images,page,15+120*area,10+y+3*state.height/4,blue);
      // The right edge and gutter establish that rows did not overlap a
      // neighboring content area while their exact color mass was retained.
      Pixel(images,page,75+120*area,10+y+state.height/4,white);
      y+=state.height;
    }
  }
}
void Compare(const Frames& a,const Frames& b,Artifacts& artifacts,const std::string& label,bool different_note=false){
  ASSERT_EQ(a.size(),b.size());
  for(size_t page=0;page<a.size();++page){
    auto expected=a[page],actual=b[page];
    if(page==0&&different_note){
      artifacts.Image(label+"-unmasked-live",expected);artifacts.Image(label+"-unmasked-saved",actual);
      for(int y=10;y<28;++y)for(int x=10;x<70;++x)for(int channel=0;channel<3;++channel){
        expected.pixels[(y*360+x)*4+channel]=255;
        actual.pixels[(y*360+x)*4+channel]=255;
      }
    }
    CompareRaster(expected,actual,artifacts,label+"-"+std::to_string(page),RasterTolerance{18,0,.15});
  }
}
void SaveBoth(const Library& writer,Session& s,const Spec& spec,State state,const std::u16string& text,const Frames& original,Artifacts& artifacts,const std::string& label){
  for(FPDF_DWORD flags:{FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}){
    ASSERT_NO_FATAL_FAILURE(Value(s,text));
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer.name()+label+std::to_string(flags),flags);ASSERT_FALSE(saved.empty());
    ASSERT_NO_FATAL_FAILURE(Value(s,text));
    std::array<Frames,2> frames;size_t backend=0;
    for(const auto* reader:Implementations()){
      SCOPED_TRACE(writer.name()+" -> "+reader->name());Loaded loaded(*reader,saved);
      ASSERT_TRUE(loaded.s.valid());ASSERT_TRUE(loaded.s.xfa_loaded);
      // Native ContentNodeNeedtoExport explicitly excludes unbound field
      // values from the changed-form packet. Saving leaves the live edit
      // intact; a fresh merge intentionally restores the template default.
      ASSERT_NO_FATAL_FAILURE(Value(loaded.s,u"Seed"));frames[backend]=Render(loaded.s);
      ASSERT_NO_FATAL_FAILURE(Inspect(frames[backend],spec,state));
      Compare(original,frames[backend],artifacts,label+writer.name()+reader->name()+std::to_string(flags),text!=u"Seed");++backend;
    }
    Compare(frames[0],frames[1],artifacts,label+writer.name()+"-readers"+std::to_string(flags));
  }
}
void CanonicalizeNative(std::unique_ptr<Loaded>& loaded,const Library& owner,
    Artifacts& artifacts,const std::string& label){
  // Pinned native's retained view drifts after editing the unbound field:
  // the first content area becomes empty and later fragments move. Preserve
  // those raw frames, then obtain its oracle from the bytes it really saves.
  // Nothing reseeds text, row attributes, or instance state after reopening.
  const auto transient=Render(loaded->s);
  EXPECT_EQ(Count(transient,cyan),64u*transient.size());
  for(size_t page=0;page<transient.size();++page)
    artifacts.Image(label+"-native-live-"+std::to_string(page),transient[page]);
  EXPECT_TRUE(loaded->s.host->callbacks_valid);EXPECT_FALSE(loaded->s.host->callback_error);
  const auto bytes=SaveDocument(loaded->s.api,loaded->s.document,artifacts,label+"-native-canonical",FPDF_NO_INCREMENTAL);
  ASSERT_FALSE(bytes.empty());
  loaded.reset();
  loaded=std::make_unique<Loaded>(owner,bytes);
  ASSERT_TRUE(loaded->s.valid());ASSERT_TRUE(loaded->s.xfa_loaded);
}
void RunScenario(const Spec& spec){
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);ASSERT_LE(spec.steps.size(),5u);
  Artifacts artifacts;const auto bytes=Fixture(spec);artifacts.Pdf(spec.name+"-input",bytes);
  std::array<std::vector<Frames>,2> states;size_t backend=0;
  for(const auto* writer:Implementations()){
    SCOPED_TRACE(writer->name());auto loaded=std::make_unique<Loaded>(*writer,bytes);
    ASSERT_TRUE(loaded->s.valid());ASSERT_TRUE(loaded->s.xfa_loaded);
    EXPECT_FLOAT_EQ(loaded->s.api.FPDF_GetPageWidthF(loaded->s.page),360);
    EXPECT_FLOAT_EQ(loaded->s.api.FPDF_GetPageHeightF(loaded->s.page),240);
    ASSERT_NO_FATAL_FAILURE(Inspect(Render(loaded->s),spec,State{}));
    std::u16string text=u"Edited";
    ASSERT_NO_FATAL_FAILURE(Edit(loaded->s,text));
    if(writer->name()=="native"){
      ASSERT_NO_FATAL_FAILURE(CanonicalizeNative(loaded,*writer,artifacts,spec.name+"-initial-edited"));
    }
    ASSERT_NO_FATAL_FAILURE(Value(loaded->s,writer->name()=="native"?u"Seed":text));
    auto frames=Render(loaded->s);
    ASSERT_NO_FATAL_FAILURE(Inspect(frames,spec,State{}));states[backend].push_back(frames);
    ASSERT_NO_FATAL_FAILURE(SaveBoth(*writer,loaded->s,spec,State{},writer->name()=="native"?u"Seed":text,frames,artifacts,spec.name+"-initial-edited"));
    for(size_t i=0;i<spec.steps.size();++i){
      SCOPED_TRACE(i);
      if(!spec.steps[i].edit.empty()){
        text=spec.steps[i].edit;
        ASSERT_NO_FATAL_FAILURE(Edit(loaded->s,text));
        if(writer->name()=="native"){
          ASSERT_NO_FATAL_FAILURE(CanonicalizeNative(loaded,*writer,artifacts,spec.name+"-second-edit"));
          ASSERT_NO_FATAL_FAILURE(Value(loaded->s,u"Seed"));
          ASSERT_NO_FATAL_FAILURE(Inspect(Render(loaded->s),spec,i?spec.steps[i-1].state:State{}));
        }
      }
      ASSERT_NO_FATAL_FAILURE(Trigger(loaded->s,i));
      if(writer->name()=="native"){
        ASSERT_NO_FATAL_FAILURE(CanonicalizeNative(loaded,*writer,artifacts,spec.name+"-step"+std::to_string(i)));
      }
      ASSERT_NO_FATAL_FAILURE(Value(loaded->s,writer->name()=="native"?u"Seed":text));
      frames=Render(loaded->s);
      ASSERT_NO_FATAL_FAILURE(Inspect(frames,spec,spec.steps[i].state));
      for(size_t page=0;page<frames.size();++page)artifacts.Image(spec.name+writer->name()+"-step"+std::to_string(i)+"-page"+std::to_string(page),frames[page]);
      states[backend].push_back(frames);
      Compare(frames,Render(loaded->s),artifacts,spec.name+writer->name()+"-stable-step"+std::to_string(i));
      ASSERT_NO_FATAL_FAILURE(SaveBoth(*writer,loaded->s,spec,spec.steps[i].state,writer->name()=="native"?u"Seed":text,frames,artifacts,spec.name+"-step"+std::to_string(i)));
    }
    EXPECT_TRUE(loaded->s.host->callbacks_valid);EXPECT_FALSE(loaded->s.host->callback_error);++backend;
  }
  ASSERT_EQ(states[0].size(),states[1].size());
  for(size_t i=0;i<states[0].size();++i)Compare(states[0][i],states[1][i],artifacts,spec.name+"-backends"+std::to_string(i),true);
}
std::string Resize(int height){
  return "for(var i=0;i<m.count;i++){var n=r.stream.resolveNode('row['+i+']');n.h='"+std::to_string(height)+"pt';n.upper.h='"+std::to_string(height/2)+"pt';n.lower.y='"+std::to_string(height/2)+"pt';n.lower.h='"+std::to_string(height/2)+"pt';}eq(m.count,6);l.relayout();";
}
TEST(CoverageXfaRelayoutSequences, TwoAreaShrinkAndGrowRetainTheEditedUnboundField) {
  RunScenario({"two-area-shrink",{100,100},0,{{Resize(20),{6,20,1}},{Resize(40),{}}}});
}
TEST(CoverageXfaRelayoutSequences, HidingAndRestoringTheFlowPreservesTheEditedController) {
  RunScenario({"two-area-hide",{100,100},0,{
    {"r.stream.presence='hidden';eq(r.stream.presence,'hidden');l.relayout();",{0,40,1,false}},
    {"r.stream.presence='visible';eq(r.stream.presence,'visible');l.relayout();",{}}}});
}
TEST(CoverageXfaRelayoutSequences, RemovingAndReinsertingMiddleRowsRetiresTheTrailingPage) {
  RunScenario({"middle-rows",{100,100},0,{
    {"m.removeInstance(2);m.removeInstance(2);eq(m.count,4);l.relayout();",{4,40,1}},
    {"m.insertInstance(2,false);m.insertInstance(3,false);eq(m.count,6);l.relayout();",{}}}});
}
TEST(CoverageXfaRelayoutSequences, RepeatedShrinkGrowCyclesDoNotAccumulatePagesOrLoseTheEdit) {
  RunScenario({"repeated-shrink",{100,100},0,{
    {Resize(20),{6,20,1}},{Resize(40),{}},{Resize(20),{6,20,1}},{Resize(40),{}}}});
}
TEST(CoverageXfaRelayoutSequences, HiddenFlowCanRemoveAndReinsertRowsBeforeItsRestoration) {
  RunScenario({"hidden-instance-edits",{100,100},0,{
    {"r.stream.presence='hidden';l.relayout();",{0,40,1,false}},
    {"m.removeInstance(1);m.removeInstance(3);eq(m.count,4);l.relayout();",{0,40,1,false}},
    {"m.insertInstance(1,false);m.insertInstance(4,false);eq(m.count,6);r.stream.presence='visible';l.relayout();",{}}}});
}
TEST(CoverageXfaRelayoutSequences, NestedSplittableParentReusesContinuationsAfterResize) {
  RunScenario({"nested-resize",{100,100},1,{{Resize(20),{6,20,1}},{Resize(40),{}}}});
}
TEST(CoverageXfaRelayoutSequences, TwoNestedParentsReleaseHiddenContinuationsAndRestoreTheEdit) {
  RunScenario({"two-nested-hide",{100,100},2,{
    {"r.stream.presence='hidden';l.relayout();",{0,40,1,false}},
    {"r.stream.presence='visible';l.relayout();",{}}}});
}
TEST(CoverageXfaRelayoutSequences, KeepPairsMoveTogetherWhenTheFirstAreaCannotFitTheirCombinedHeight) {
  const auto paired="for(var i=0;i<m.count;i++){var n=r.stream.resolveNode('row['+i+']');n.keep.next=i%2===0?'contentArea':'none';n.keep.previous=i%2===1?'contentArea':'none';}l.relayout();";
  const auto clear="for(var i=0;i<m.count;i++){var n=r.stream.resolveNode('row['+i+']');n.keep.next='none';n.keep.previous='none';}l.relayout();";
  RunScenario({"keep-pairs",{90,90},0,{{paired,{6,40,2,true,true}},{clear,{}}}});
}
TEST(CoverageXfaRelayoutSequences, UnequalContentAreasRepackRowsAfterShrinkingWithoutChangingTheEdit) {
  RunScenario({"unequal-resize",{80,120},0,{{Resize(20),{6,20,1}},{Resize(40),{}}}});
}
TEST(CoverageXfaRelayoutSequences, ThreeContentAreasReuseTheNextMasterOnlyWhenTheSixthRowNeedsIt) {
  RunScenario({"three-area-resize",{80,80,80},0,{{Resize(20),{6,20,1}},{Resize(40),{}}}});
}
TEST(CoverageXfaRelayoutSequences, InvisibleFlowRetainsItsGeometryWhileTheUnboundFieldStaysEditable) {
  RunScenario({"invisible-edit",{100,100},0,{
    {"r.stream.presence='invisible';eq(r.stream.presence,'invisible');l.relayout();",{6,40,2,false}},
    {"r.stream.presence='visible';l.relayout();",{},u"Second"}}});
}
TEST(CoverageXfaRelayoutSequences, EditingAgainWhileShrunkSurvivesGrowthAndFirstLastRowReplacement) {
  RunScenario({"second-edit-replace",{100,100},0,{
    {Resize(20),{6,20,1}},
    {Resize(40),{},u"Second"},
    {"m.removeInstance(5);m.removeInstance(0);eq(m.count,4);l.relayout();",{4,40,1}},
    {"m.insertInstance(0,false);m.addInstance(false);eq(m.count,6);l.relayout();",{}}}});
}
}  // namespace
}  // namespace pdfoxium::integration
