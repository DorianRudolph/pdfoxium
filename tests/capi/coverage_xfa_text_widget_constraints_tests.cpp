#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <cmath>
#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using Action = std::function<void(Session&)>;
using ImageCheck = std::function<void(const Raster&,const Raster&,Artifacts&,const std::string&)>;
bool IsNative(const Session& s){for(const auto* lib:Implementations())if(&lib->api()==&s.api)return lib->name()=="native";return false;}

// Source paths: FFTextEdit::UpdateWidgetProperty, FDETextEditEngine::AdjustGap/
// ReplaceSelectedText, FWLListBox::GetItem/OnVK/ScrollToVisible, and the rich
// TextLayout block loader. All edits and observations use the exported C API.
std::string CleanInitialize() {
  return forms::Event("if(typeof this.rawValue==='string')this.rawValue=this.rawValue.replace(/^\\n/,'');"
      "for(var j=0;j<this.nodes.length;j++){var n=this.nodes.item(j);if(n.className!=='items')continue;"
      "for(var k=0;k<n.nodes.length;k++){var v=n.nodes.item(k);v.value=String(v.value).replace(/^\\n/,'');}}", "initialize");
}
std::string Text(const std::string& ui="<textEdit multiLine=\"1\"/>",
                 const std::string& attributes="", const std::string& para="", int width=280, int height=70) {
  return "<field name=\"input\" x=\"8pt\" y=\"8pt\" w=\""+std::to_string(width)+"pt\" h=\""+std::to_string(height)+
      "pt\"><bind match=\"once\"/><ui>"+ui+"</ui><font typeface=\"Helvetica\" size=\"10pt\"/>"+para+
      "<value><text "+attributes+">seed</text></value>"+CleanInitialize()+"</field>";
}
std::vector<uint8_t> Fixture(const std::string& body) {
  const std::string preamble="<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
  const std::string config="<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
  std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" locale=\"en_US\" restoreState=\"auto\"><pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"320pt\" long=\"300pt\"/><contentArea w=\"320pt\" h=\"300pt\"/></pageArea></pageSet>"+body+
      "<field name=\"sink\" x=\"8pt\" y=\"260pt\" w=\"280pt\" h=\"22pt\"><bind match=\"none\"/><ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"10pt\"/><value><text>sink</text></value>"+CleanInitialize()+"</field>"
      "<draw x=\"300pt\" y=\"280pt\" w=\"10pt\" h=\"10pt\"><value><rectangle><fill><color value=\"0,128,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw></subform></template>";
  // Keep the native oracle's independent CDATA buffer-boundary defect outside
  // these widget scenarios, using the fixture convention shared by XFA suites.
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
    PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
    PdfStream("","</xdp:xdp>"),"null"};
  auto font=forms::PinnedXfaArimoFontObjects();for(auto& object:font)objects.push_back(std::move(object));
  return PdfFixture(objects);
}
void Put(Session& s,const std::u16string& text) {
  ASSERT_TRUE(s.Focus(12,16));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(text);
}
void Settle(Session& s) {
  ASSERT_TRUE(s.Focus(12,268));s.api.FORM_ForceToKillFocus(s.form);
  s.Click(315,295);s.api.FORM_OnMouseMove(s.form,s.page,0,315,295);
}
void Key(Session& s,int key,int flags=0){EXPECT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,key,flags));}
// Native's date-picker composite double-paints its edit text. Preserve the
// same explicit ink-geometry comparison used by CoverageForms::CompareDate.
std::array<int,5> InkBounds(const Raster& raster) {
  std::array<int,5> b{raster.width,raster.height,-1,-1,0};
  for(int y=0;y<raster.height;++y)for(int x=0;x<raster.width;++x){const auto at=(size_t(y)*raster.width+x)*4;
    if(raster.pixels[at]<180&&raster.pixels[at+1]<180&&raster.pixels[at+2]<180){b[0]=std::min(b[0],x);b[1]=std::min(b[1],y);b[2]=std::max(b[2],x);b[3]=std::max(b[3],y);++b[4];}}
  return b;
}
void ClearLogical(Raster& image,int x,int y,int width,int height){
  const int left=int(std::lround(x*image.render_scale)),top=int(std::lround(y*image.render_scale));
  const int right=int(std::lround((x+width)*image.render_scale)),bottom=int(std::lround((y+height)*image.render_scale));
  for(int py=top;py<bottom;++py)for(int px=left;px<right;++px)for(int c=0;c<4;++c)image.pixels[(size_t(py)*image.width+px)*4+c]=255;
}
void DateImages(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label){
  const auto first=InkBounds(CropLogicalRaster(a,8,8,280,60)),second=InkBounds(CropLogicalRaster(b,8,8,280,60));
  EXPECT_GT(first[4],20);EXPECT_GT(second[4],20);
  for(size_t i=0;i<4;++i)EXPECT_NEAR(first[i],second[i],std::ceil(a.render_scale));
  auto outside_a=a,outside_b=b;ClearLogical(outside_a,8,8,280,60);ClearLogical(outside_b,8,8,280,60);
  CompareRaster(outside_a,outside_b,artifacts,label,RasterTolerance::ConnectedError(1.0));
}
std::vector<std::array<int,2>> Columns(const Raster& image){
  std::vector<std::array<int,2>> columns;
  for(int x=0;x<image.width;++x){bool ink=false;for(int y=0;y<image.height&&!ink;++y){const auto at=(size_t(y)*image.width+x)*4;ink=image.pixels[at]<180&&image.pixels[at+1]<180&&image.pixels[at+2]<180;}
    if(ink){if(!columns.empty()&&columns.back()[1]+1==x)columns.back()[1]=x;else columns.push_back({x,x});}}
  return columns;
}
void TabPositions(Session& s){
  const auto image=CropLogicalRaster(s.DrawForComparison(320,300),8,100,280,140);const auto columns=Columns(image);ASSERT_EQ(columns.size(),3u);
  // The native initial tab-index -1 guard suppresses positioned stops. Rust
  // aligns each 12pt Cousine advance to the declared 80pt/180pt right edge.
  for(int i=1;i<3;++i)EXPECT_NEAR((columns[i][0]-columns[0][0])/image.render_scale,IsNative(s)?12*i:(i==1?68:168),1.0);
}
void TabImages(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label){
  const auto acrop=CropLogicalRaster(a,8,100,280,140),bcrop=CropLogicalRaster(b,8,100,280,140);
  const auto ac=Columns(acrop),bc=Columns(bcrop);ASSERT_EQ(ac.size(),3u);ASSERT_EQ(bc.size(),3u);
  for(size_t i=0;i<3;++i){EXPECT_NEAR(ac[i][1]-ac[i][0],bc[i][1]-bc[i][0],1);
    const int width=std::max(ac[i][1]-ac[i][0],bc[i][1]-bc[i][0])+3;
    ASSERT_GT(ac[i][0],0);ASSERT_GT(bc[i][0],0);
    CompareRaster(CropRaster(acrop,ac[i][0]-1,0,width,acrop.height),CropRaster(bcrop,bc[i][0]-1,0,width,bcrop.height),artifacts,label+"-glyph-"+std::to_string(i),RasterTolerance::ConnectedError(1.0));}
  auto outside_a=a,outside_b=b;ClearLogical(outside_a,8,100,280,140);ClearLogical(outside_b,8,100,280,140);
  CompareRaster(outside_a,outside_b,artifacts,label+"-outside",RasterTolerance::ConnectedError(1.0));
}
void Exercise(const std::string& body,const Action& action,const Action& verify,const ImageCheck& image_check={},const Action& prepare_image={}) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);Artifacts artifacts;std::array<Raster,2> images;size_t n=0;
  const auto fixture=Fixture(body);
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Session s(writer->api(),fixture,true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(action(s));
    ASSERT_NO_FATAL_FAILURE(verify(s));Settle(s);
    if(prepare_image){ASSERT_NO_FATAL_FAILURE(prepare_image(s));}
    images[n]=s.DrawForComparison(320,300);forms::ExpectInk(images[n],8,8,288,248);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-constraints");ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Session loaded(reader->api(),saved,true,true);ASSERT_TRUE(loaded.valid());ASSERT_TRUE(loaded.xfa_loaded);
      ASSERT_NO_FATAL_FAILURE(verify(loaded));Settle(loaded);
      if(prepare_image){ASSERT_NO_FATAL_FAILURE(prepare_image(loaded));}
      EXPECT_TRUE(loaded.host->callbacks_valid);EXPECT_FALSE(loaded.host->callback_error);
      const auto after=loaded.DrawForComparison(320,300);
      if(image_check)image_check(images[n],after,artifacts,writer->name()+"-"+reader->name());
      else CompareRaster(images[n],after,artifacts,writer->name()+"-"+reader->name(),RasterTolerance::ConnectedError(1.0));
    }
    ++n;
  }
  if(image_check)image_check(images[0],images[1],artifacts,"engines");
  else CompareRaster(images[0],images[1],artifacts,"engines",RasterTolerance::ConnectedError(1.0));
  CompareRaster(CropLogicalRaster(images[0],300,280,10,10),CropLogicalRaster(images[1],300,280,10,10),artifacts,"landmark");
}

class CoverageXfaTextConstraints : public testing::TestWithParam<int> {};
TEST_P(CoverageXfaTextConstraints, TruncatedReplacementRespectsLimitAndRestoresCompleteHistory) {
  const int limit=GetParam();const std::u16string request(limit+7,u'W'),expected(limit,u'W');
  const auto body=Text("<textEdit multiLine=\"1\"/>","maxChars=\""+std::to_string(limit)+"\"");
  Exercise(body,[&](Session& s){
    Put(s,request);EXPECT_EQ(s.Text(),expected);ASSERT_TRUE(s.api.FORM_CanUndo(s.form,s.page));
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),u"seed");
    ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),expected);
    Key(s,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey);s.api.FORM_OnChar(s.form,s.page,'X',0);EXPECT_EQ(s.Text(),expected);
    Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
  },[&](Session& s){ASSERT_TRUE(s.Focus(12,16));EXPECT_EQ(s.Text(),expected);Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);});
}
INSTANTIATE_TEST_SUITE_P(CharacterBoundarySizes,CoverageXfaTextConstraints,testing::Values(5,7,15,31,63,64,65,127));

struct AreaCase {const char* name;int width;int height;bool multiline;};
class CoverageXfaAreaConstraints : public testing::TestWithParam<AreaCase> {};
TEST_P(CoverageXfaAreaConstraints, OverflowPolicyThenShortReplacementRecoversWithoutLosingEditability) {
  const auto item=GetParam();std::u16string accepted;
  auto body=Text(item.multiline?"<textEdit multiLine=\"1\" vScrollPolicy=\"off\"/>":"<textEdit hScrollPolicy=\"off\"/>",
      "","<para lineHeight=\"12pt\"/>",item.width,item.height);
  const auto close=body.rfind("</field>");
  body.insert(close,forms::Event("xfa.host.messageBox('full');","full"));
  const std::u16string request(500,u'W');
  Exercise(body,[&](Session& s){
    s.host->alerts.clear();Put(s,request);accepted=s.Text();
    ASSERT_FALSE(accepted.empty());
    const bool native_vertical_bug=item.multiline&&IsNative(s);
    if(native_vertical_bug)EXPECT_EQ(accepted,request);else EXPECT_LT(accepted.size(),request.size());
    EXPECT_EQ(accepted,request.substr(0,accepted.size()));
    bool full=false;for(const auto& alert:s.host->alerts)full|=std::get<0>(alert)==u"full";EXPECT_EQ(full,!native_vertical_bug);
    // Native breaks when width fits before checking vertical overflow. Check
    // its original observation above, then exercise recovery via normal input.
    Put(s,u"fit");EXPECT_EQ(s.Text(),u"fit");
    Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
  },[&](Session& s){ASSERT_TRUE(s.Focus(12,16));EXPECT_EQ(s.Text(),u"fit");Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);});
}
INSTANTIATE_TEST_SUITE_P(VisibleAreaLimits,CoverageXfaAreaConstraints,testing::Values(
 AreaCase{"Single40",40,24,false},AreaCase{"Single80",80,24,false},AreaCase{"Single120",120,24,false},AreaCase{"Single160",160,24,false},
 AreaCase{"OneLine",120,12,true},AreaCase{"TwoLines",120,24,true},AreaCase{"ThreeLines",120,36,true},AreaCase{"FourLines",120,48,true}),
 [](const testing::TestParamInfo<AreaCase>& info){return info.param.name;});

class CoverageXfaGapHistory : public testing::TestWithParam<int> {};
TEST_P(CoverageXfaGapHistory, LongReplacementThenMidBufferInsertionPreservesGapAndBranchesHistory) {
  const int length=GetParam();std::u16string seed;for(int i=0;i<length;++i)seed+=char16_t(u'A'+i%26);
  const auto expected=seed.substr(0,length/2)+u"middle"+seed.substr(length/2);
  Exercise(Text(),[&](Session& s){
    Put(s,seed);EXPECT_EQ(s.Text(),seed);Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
    for(int i=0;i<length/2;++i)Key(s,FWL_VKEY_Right);
    s.Replace(u"middle");EXPECT_EQ(s.Text(),expected);ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),seed);
    ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),expected);
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));Put(s,u"branch");EXPECT_EQ(s.Text(),u"branch");
    EXPECT_FALSE(s.api.FORM_CanRedo(s.form,s.page));Put(s,expected);Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
  },[&](Session& s){ASSERT_TRUE(s.Focus(12,16));EXPECT_EQ(s.Text(),expected);Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);});
}
INSTANTIATE_TEST_SUITE_P(GapAllocationSizes,CoverageXfaGapHistory,testing::Values(63,64,65,127,128,129,511,1025));

struct RichCase {const char* name;const char* html;bool tabs=false;};
class CoverageXfaRichBlockConstraints : public testing::TestWithParam<RichCase> {};
TEST_P(CoverageXfaRichBlockConstraints, RichBlockGeometrySurvivesIndependentPublicInputAndCrossSaving) {
  const auto item=GetParam();
  const std::string rich="<draw name=\"rich\" x=\"8pt\" y=\"100pt\" w=\"280pt\" h=\"140pt\"><font typeface=\"Helvetica\" size=\"10pt\"/><value><exData contentType=\"text/html\"><body xmlns=\"http://www.w3.org/1999/xhtml\">"+std::string(item.html)+"</body></exData></value></draw>";
  Exercise(Text()+rich,[&](Session& s){
    const auto before=CropLogicalRaster(s.DrawForComparison(320,300),8,100,280,140);forms::ExpectInk(before,0,0,280,140);
    if(item.tabs)TabPositions(s);
    Put(s,u"independent input");EXPECT_EQ(s.Text(),u"independent input");Settle(s);
    Artifacts artifacts;
    CompareRaster(before,CropLogicalRaster(s.DrawForComparison(320,300),8,100,280,140),artifacts,"rich-unmodified");
  },[&](Session& s){ASSERT_TRUE(s.Focus(12,16));EXPECT_EQ(s.Text(),u"independent input");if(item.tabs)TabPositions(s);},item.tabs?ImageCheck(TabImages):ImageCheck{});
}
INSTANTIATE_TEST_SUITE_P(BlockStyles,CoverageXfaRichBlockConstraints,testing::Values(
 RichCase{"NestedLists","<ul><li>Outer<ol><li>Inner first</li><li>Inner second</li></ol></li><li>Last</li></ul>"},
 RichCase{"ParagraphSpacing","<p style=\"margin-top:8pt;margin-bottom:12pt\">First</p><p style=\"margin-top:4pt\">Second</p>"},
 RichCase{"HangingIndent","<p style=\"margin-left:24pt;text-indent:-12pt\">First second third fourth fifth sixth seventh eighth ninth tenth eleventh twelfth thirteenth</p>"},
 RichCase{"PreserveWhitespace","<p style=\"white-space:pre\">  Alpha   Beta<br/>  Gamma   Delta</p>"},
 RichCase{"MixedFontRuns","<p>Normal <span style=\"font-size:16pt;font-weight:bold\">Large</span> small<br/>Another line</p>"},
 RichCase{"EmptyBlocks","<p/><p>Visible first</p><p/><p>Visible last</p>"},
 RichCase{"RightTabs","<p style=\"font-family:Cousine;font-size:20pt;tab-stops:right 80pt right 180pt\">I<span style=\"xfa-tab-count:1\"/>I<span style=\"xfa-tab-count:1\"/>I</p>",true},
 RichCase{"DecimalTabs","<p style=\"font-family:Cousine;font-size:20pt;tab-stops:decimal 80pt decimal 180pt\">I<span style=\"xfa-tab-count:1\"/>I<span style=\"xfa-tab-count:1\"/>I</p>",true}),
 [](const testing::TestParamInfo<RichCase>& info){return info.param.name;});

struct ListCase {const char* name;int count;int width;int height;int downs;bool from_end;};
class CoverageXfaLongListConstraints : public testing::TestWithParam<ListCase> {};
TEST_P(CoverageXfaLongListConstraints, KeyboardMovesAcrossClippedItemsAndPersistsTheExactExport) {
  const auto item=GetParam();std::string labels,values;
  for(int i=0;i<item.count;++i){labels+="<text>Item "+std::to_string(i)+" long label for horizontal clipping</text>";values+="<text>v"+std::to_string(i)+"</text>";}
  const std::string body="<field name=\"choice\" x=\"8.5pt\" y=\"8.5pt\" w=\""+std::to_string(item.width)+"pt\" h=\""+std::to_string(item.height)+"pt\"><bind match=\"once\"/><ui><choiceList open=\"always\"/></ui><font typeface=\"Helvetica\" size=\"10pt\"/><items>"+labels+"</items><items save=\"1\">"+values+"</items><value><text>v0</text></value>"+CleanInitialize()+"</field>"+
    "<field name=\"check\" x=\"200pt\" y=\"200pt\" w=\"80pt\" h=\"24pt\"><ui><button/></ui>"+
    forms::Event("var observed=String(xfa.form.root.choice.selectedIndex)+'|'+String(xfa.form.root.choice.rawValue);xfa.form.root.sink.rawValue=observed;xfa.host.messageBox(observed);","click")+"</field>";
  const int selected=item.from_end?item.count-1-item.downs:item.downs;
  const auto observe=[&](Session& s){
    // Require a fresh public callback without changing focus or editing the
    // sink before the choice is observed. Saved sink text alone is not proof
    // that the check action ran with the reloaded selection.
    s.host->alerts.clear();s.Click(220,210);
    const auto ascii=std::to_string(selected)+"|v"+std::to_string(selected);
    const std::u16string expected(ascii.begin(),ascii.end());
    ASSERT_EQ(s.host->alerts.size(),1u);EXPECT_EQ(std::get<0>(s.host->alerts.front()),expected);
    ASSERT_TRUE(s.Focus(12,268));EXPECT_EQ(s.Text(),expected);
  };
  const auto verify=[&](Session& s){
    // Selection persists; viewport scrolling does not. Observe the saved
    // export before replaying the same navigation to normalize only scrolling.
    ASSERT_NO_FATAL_FAILURE(observe(s));ASSERT_TRUE(s.Focus(12,16));
    Key(s,item.from_end?FWL_VKEY_End:FWL_VKEY_Home);
    for(int i=0;i<item.downs;++i)Key(s,item.from_end?FWL_VKEY_Up:FWL_VKEY_Down);
    ASSERT_NO_FATAL_FAILURE(observe(s));
  };
  const auto focused_image=[&](Session& s){
    // Native UpdateFWLData mistakes an item index for a selected-item ordinal
    // and loses the highlight after blur. Render the actual focused selection
    // through identical public input, retaining the strict full-scene metric.
    const auto blue_pixels=[&](const Raster& image){
      const auto region=CropLogicalRaster(image,8,8,item.width,item.height);size_t count=0;
      for(size_t i=0;i<region.pixels.size();i+=4)
        count+=region.pixels[i]>region.pixels[i+2]+20&&region.pixels[i+1]>region.pixels[i+2]+5;
      return count;
    };
    const auto blurred=s.DrawForComparison(320,300);const auto before_blue=blue_pixels(blurred);
    if(IsNative(s)&&selected>0)EXPECT_EQ(before_blue,0u);else EXPECT_GT(before_blue,20u);
    if(testing::Test::HasFailure()){Artifacts artifacts;artifacts.Image(IsNative(s)?"native-blurred-selection":"rust-blurred-selection",blurred);}
    ASSERT_TRUE(s.Focus(12,16));Key(s,item.from_end?FWL_VKEY_End:FWL_VKEY_Home);
    for(int i=0;i<item.downs;++i)Key(s,item.from_end?FWL_VKEY_Up:FWL_VKEY_Down);
    s.api.FORM_OnMouseMove(s.form,s.page,0,315,295);
    const auto image=s.DrawForComparison(320,300);EXPECT_GT(blue_pixels(image),20u);
    if(testing::Test::HasFailure()){Artifacts artifacts;artifacts.Image(IsNative(s)?"native-focused-selection":"rust-focused-selection",image);}
  };
  Exercise(body,[&](Session& s){
    ASSERT_TRUE(s.Focus(12,16));Key(s,item.from_end?FWL_VKEY_End:FWL_VKEY_Home);
    for(int i=0;i<item.downs;++i)Key(s,item.from_end?FWL_VKEY_Up:FWL_VKEY_Down);
    // Draw while focused to exercise visible-item clipping after scroll-to-item.
    forms::ExpectInk(s.DrawForComparison(320,300),8,8,item.width+8,item.height+8);
  },verify,{},focused_image);
}
INSTANTIATE_TEST_SUITE_P(ClippedItemWindows,CoverageXfaLongListConstraints,testing::Values(
 ListCase{"ShortWindowDown",20,120,32,7,false},ListCase{"ShortWindowUp",20,120,32,7,true},
 ListCase{"NarrowWindowDown",24,48,48,15,false},ListCase{"NarrowWindowUp",24,48,48,15,true},
 ListCase{"TallWindowDown",32,160,120,24,false},ListCase{"TallWindowUp",32,160,120,24,true},
 ListCase{"LastItem",40,180,60,0,true},ListCase{"FirstItem",40,180,60,0,false}),
 [](const testing::TestParamInfo<ListCase>& info){return info.param.name;});

struct DateCase {const char* name;const char* horizontal;const char* vertical;const char* picture;const char16_t* input;};
class CoverageXfaDateEditConstraints : public testing::TestWithParam<DateCase> {};
TEST_P(CoverageXfaDateEditConstraints, AlignedDateEditCommitsItsPictureThroughPublicInputAndReloads) {
  const auto item=GetParam();
  const std::string body="<field name=\"input\" x=\"8pt\" y=\"8pt\" w=\"280pt\" h=\"60pt\"><bind match=\"once\"/><ui><dateTimeEdit hScrollPolicy=\"off\"/><picture>"+std::string(item.picture)+"</picture></ui><font typeface=\"Helvetica\" size=\"10pt\"/><para hAlign=\""+item.horizontal+"\" vAlign=\""+item.vertical+"\"/><value><date>2004-02-29</date></value><format><picture>"+item.picture+"</picture></format>"+CleanInitialize()+forms::Event("this.ui.picture.value=String(this.ui.picture.value).replace(/^\\n/,\"\");this.format.picture.value=String(this.format.picture.value).replace(/^\\n/,\"\");", "initialize")+"</field>";
  Exercise(body,[&](Session& s){Put(s,item.input);EXPECT_EQ(s.Text(),item.input);Settle(s);},
    [&](Session& s){ASSERT_TRUE(s.Focus(12,16));EXPECT_EQ(s.Text(),item.input);Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);},DateImages);
}
INSTANTIATE_TEST_SUITE_P(EditPictureAlignment,CoverageXfaDateEditConstraints,testing::Values(
 DateCase{"LeftTop","left","top","date{YYYY-MM-DD}",u"2000-02-29"},
 DateCase{"CenterTop","center","top","date{YYYY-MM-DD}",u"2001-12-31"},
 DateCase{"RightTop","right","top","date{YYYY-MM-DD}",u"2004-01-01"},
 DateCase{"LeftMiddle","left","middle","date{MM/DD/YYYY}",u"02/29/2004"},
 DateCase{"CenterMiddle","center","middle","date{DD.MM.YYYY}",u"31.12.2000"},
 DateCase{"RightMiddle","right","middle","date{YYYY/MM/DD}",u"2004/03/01"},
 DateCase{"LeftBottom","left","bottom","date{MM-DD-YYYY}",u"12-31-2004"},
 DateCase{"RightBottom","right","bottom","date{DD/MM/YYYY}",u"29/02/2000"}),
 [](const testing::TestParamInfo<DateCase>& info){return info.param.name;});

}  // namespace
}  // namespace pdfoxium::integration
