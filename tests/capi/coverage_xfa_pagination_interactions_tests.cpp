#include "xfa_form_fixture.h"
#include "pinned_test_fonts.h"

#include <algorithm>
#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using RGB = std::array<int,3>;
using Frames = std::vector<Raster>;
using Inspect = std::function<void(const Frames&)>;
const RGB red{255,0,0},green{0,128,0},blue{0,0,255},orange{255,128,0};
const RGB magenta{255,0,255},yellow{255,255,0},cyan{0,255,255};
std::string Paint(const std::string& name,RGB color,int width,int height,const std::string& attrs="") {
  return "<draw name=\""+name+"\" w=\""+std::to_string(width)+"pt\" h=\""+std::to_string(height)+"pt\" "+attrs+"><value><rectangle><fill><color value=\""+
    std::to_string(color[0])+","+std::to_string(color[1])+","+std::to_string(color[2])+"\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
}
std::string Cell(const std::string& name,RGB color,int height,const std::string& props="",const std::string& attrs="") {
  return "<subform name=\""+name+"\" layout=\"position\" w=\"60pt\" h=\""+std::to_string(height)+"pt\" "+attrs+">"+props+Paint("paint",color,60,height)+"</subform>";
}
std::string Area(const std::string& areas="<contentArea name=\"body\" id=\"body\" w=\"240pt\" h=\"100pt\"/>") {
  return "<pageSet><pageArea name=\"page\" id=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"240pt\" long=\"240pt\"/>"+areas+
    Paint("stationery",cyan,8,8,"x=\"220pt\" y=\"220pt\"")+"</pageArea></pageSet>";
}
std::string Escape(const std::string& text) {
  std::string out;for(char c:text){if(c=='&')out+="&amp;";else if(c=='<')out+="&lt;";else if(c=='>')out+="&gt;";else out+=c;}return out;
}
std::vector<uint8_t> Fixture(const std::string& body,const std::string& pages) {
  // Pagination instructions, generated nodes and script actions have no public
  // constructors. All loading/input/rendering/saving below uses the public ABI.
  const auto xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"tb\" restoreState=\"auto\">"+pages+body+"</subform></template>";
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 240] /Resources << >> >>",
    "<< /Fields [] /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
    PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
    PdfStream("",xml),PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),PdfStream("","</xdp:xdp>")});
}
struct Loaded {
  forms::XfaCallbacks callbacks;
  Session s;
  Loaded(const Library& owner,const std::vector<uint8_t>& bytes):s(owner.api(),bytes,true,true,2,[this](forms::FormHost& h){callbacks.Configure(h);}){}
};
Frames Render(Session& s) {
  Frames out;const int count=s.api.FPDF_GetPageCount(s.document);EXPECT_GT(count,0);EXPECT_LE(count,16);
  if(count<1||count>16)return out;
  for(int page=0;page<count;++page)out.push_back(s.Draw(240,240,0,0,0,0,0,0,page));
  return out;
}
size_t Count(const Frames& images,RGB color) {
  size_t count=0;
  for(const auto& image:images){for(size_t at=0;at<image.pixels.size();at+=4){
    count+=image.pixels[at]==color[2]&&image.pixels[at+1]==color[1]&&image.pixels[at+2]==color[0];}}
  return count;
}
size_t First(const Frames& images,RGB color) {
  size_t offset=0;
  for(const auto& image:images){
    for(size_t at=0;at<image.pixels.size();at+=4){
      if(image.pixels[at]==color[2]&&image.pixels[at+1]==color[1]&&image.pixels[at+2]==color[0])return offset+at/4;
    }
    offset+=image.pixels.size()/4;
  }
  return offset;
}
void Compare(const Frames& expected,const Frames& actual,Artifacts& artifacts,const std::string& name) {
  ASSERT_EQ(expected.size(),actual.size());for(size_t page=0;page<actual.size();++page)CompareRaster(expected[page],actual[page],artifacts,name+"-"+std::to_string(page));
}
Inspect Counts(int r,int g,int b,int o,int pages=-1,int decorations=0,bool order=false) {
  return [=](const Frames& images){
    if(pages>=0){ASSERT_EQ(images.size(),static_cast<size_t>(pages));}
    EXPECT_EQ(Count(images,red),static_cast<size_t>(r));EXPECT_EQ(Count(images,green),static_cast<size_t>(g));
    EXPECT_EQ(Count(images,blue),static_cast<size_t>(b));EXPECT_EQ(Count(images,orange),static_cast<size_t>(o));
    EXPECT_EQ(Count(images,cyan),64u*images.size());
    if(decorations==0){EXPECT_EQ(Count(images,magenta),0u);EXPECT_EQ(Count(images,yellow),0u);}
    else if(decorations>0){EXPECT_GT(Count(images,magenta),0u);EXPECT_GT(Count(images,yellow),0u);}
    if(order){size_t previous=0;bool first=true;for(const auto& entry:std::array<std::pair<RGB,int>,4>{{{red,r},{green,g},{blue,b},{orange,o}}}){
      if(entry.second==0){continue;}
      const auto at=First(images,entry.first);if(!first){EXPECT_GT(at,previous);}first=false;previous=at;}}
  };
}
void SaveBothModes(const Library& writer,Session& s,const Frames& original,const Inspect& inspect,Artifacts& artifacts,const std::string& label) {
  for(FPDF_DWORD flags:{FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}){
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer.name()+label+std::to_string(flags),flags);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()){
      SCOPED_TRACE(reader->name());Loaded reopened(*reader,saved);ASSERT_TRUE(reopened.s.valid());ASSERT_TRUE(reopened.s.xfa_loaded);
      const auto image=Render(reopened.s);
    ASSERT_NO_FATAL_FAILURE(inspect(image));Compare(original,image,artifacts,writer.name()+reader->name()+label+std::to_string(flags));
      EXPECT_TRUE(reopened.s.host->callbacks_valid);EXPECT_FALSE(reopened.s.host->callback_error);
    }
  }
}
void Scenario(const std::string& name,const std::string& body,const Inspect& inspect,const std::string& pages=Area()) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);Artifacts artifacts;const auto bytes=Fixture(body,pages);std::array<Frames,2> images;size_t index=0;
  for(const auto* writer:Implementations()){
    SCOPED_TRACE(writer->name());Loaded loaded(*writer,bytes);ASSERT_TRUE(loaded.s.valid());ASSERT_TRUE(loaded.s.xfa_loaded);
    images[index]=Render(loaded.s);
    ASSERT_NO_FATAL_FAILURE(inspect(images[index]));Compare(images[index],Render(loaded.s),artifacts,name+writer->name()+"-repeat");
    ASSERT_NO_FATAL_FAILURE(SaveBothModes(*writer,loaded.s,images[index],inspect,artifacts,name));++index;
  }
  Compare(images[0],images[1],artifacts,name+"-backends");
}
std::string Keep(const std::string& scope,bool previous,bool next) {
  return "<keep intact=\"contentArea\" previous=\""+(previous?scope:"none")+"\" next=\""+(next?scope:"none")+"\"/>";
}
std::string Chain(const std::string& scope,const std::string& middle_attributes="",const std::string& middle_properties="") {
  return Cell("prefix",red,70)+Cell("head",green,20,Keep(scope,false,true))+Cell("middle",blue,20,Keep(scope,true,true)+middle_properties,middle_attributes)+Cell("tail",orange,20,Keep(scope,true,false));
}
TEST(CoverageXfaPaginationInteractions, ThreeMemberContentAreaKeepChainMovesAsOneOrderedGroup) {
  Scenario("three-content-keeps",Chain("contentArea"),Counts(4200,1200,1200,1200,2,0,true));
}
TEST(CoverageXfaPaginationInteractions, ThreeMemberPageAreaKeepChainPreservesAllMemberColors) {
  Scenario("three-page-keeps",Chain("pageArea"),Counts(4200,1200,1200,1200,2,0,true));
}
TEST(CoverageXfaPaginationInteractions, PreviousOnlyKeepLinksStillJoinThePrecedingAtomicMembers) {
  Scenario("previous-only",Cell("prefix",red,70)+Cell("head",green,20)+Cell("middle",blue,20,Keep("contentArea",true,false))+Cell("tail",orange,20,Keep("contentArea",true,false)),Counts(4200,1200,1200,1200,2,0,true));
}
TEST(CoverageXfaPaginationInteractions, HiddenMiddleKeepMemberCannotCreateABlankContinuationPage) {
  Scenario("hidden-middle",Chain("contentArea","presence=\"hidden\""),Counts(4200,1200,0,1200,2,0,true));
}
TEST(CoverageXfaPaginationInteractions, InvisibleMiddleKeepMemberRetainsSpaceWithoutPaintingItsChild) {
  Scenario("invisible-middle",Chain("contentArea","presence=\"invisible\""),Counts(4200,1200,0,1200,2,0,true));
}
TEST(CoverageXfaPaginationInteractions, KeepChainUsesTheNextContentAreaWithoutInventingAnotherMaster) {
  Scenario("chain-two-areas",Chain("contentArea"),Counts(4200,1200,1200,1200,1,0,true),Area("<contentArea name=\"left\" w=\"100pt\" h=\"100pt\"/><contentArea name=\"right\" x=\"120pt\" w=\"100pt\" h=\"100pt\"/>"));
}
TEST(CoverageXfaPaginationInteractions, OversizedKeepChainMakesProgressAndRetainsEveryAtomicMember) {
  Scenario("long-kept-chain",Cell("prefix",red,10)+Cell("head",green,60,Keep("contentArea",false,true))+Cell("middle",blue,60,Keep("contentArea",true,true))+Cell("tail",orange,60,Keep("contentArea",true,false)),Counts(600,3600,3600,3600,-1,0,true));
}
TEST(CoverageXfaPaginationInteractions, BreakAfterInsideAThreeMemberKeepSequenceDoesNotDropTheTail) {
  Scenario("keep-break-after",Chain("contentArea","","<breakAfter targetType=\"pageArea\" startNew=\"1\"/>"),Counts(4200,1200,1200,1200,-1,0,true));
}
std::string AlternatingRows(int count,int height=30) {
  std::string out;for(int i=0;i<count;++i)out+=Cell("row"+std::to_string(i),i%2?blue:green,height,"<keep intact=\"contentArea\"/>");return out;
}
TEST(CoverageXfaPaginationInteractions, NestedFlowTopInsetSurvivesTheContinuationParentAdjustment) {
  Scenario("nested-top-inset","<subform name=\"outer\" layout=\"tb\" w=\"80pt\"><keep intact=\"none\"/><margin topInset=\"8pt\"/>"+AlternatingRows(5)+"</subform>",Counts(0,5400,3600,0));
}
TEST(CoverageXfaPaginationInteractions, NestedFlowBottomInsetDoesNotConsumeOrDuplicateColoredRows) {
  Scenario("nested-bottom-inset","<subform name=\"outer\" layout=\"tb\" w=\"80pt\"><keep intact=\"none\"/><margin bottomInset=\"12pt\"/>"+AlternatingRows(5)+"</subform>",Counts(0,5400,3600,0));
}
TEST(CoverageXfaPaginationInteractions, TwoNestedSplitLevelsKeepTheirIndependentInsetsAndPayload) {
  Scenario("two-split-levels","<subform name=\"outer\" layout=\"tb\" w=\"90pt\"><keep intact=\"none\"/><margin topInset=\"8pt\" bottomInset=\"4pt\"/><subform name=\"inner\" layout=\"tb\" w=\"80pt\"><keep intact=\"none\"/><margin topInset=\"4pt\" bottomInset=\"8pt\"/>"+AlternatingRows(6)+"</subform></subform>",Counts(0,5400,5400,0));
}
TEST(CoverageXfaPaginationInteractions, NestedFlowContinuesAcrossUnequalContentAreaHeights) {
  Scenario("unequal-content-areas","<subform name=\"outer\" layout=\"tb\" w=\"60pt\"><keep intact=\"none\"/>"+AlternatingRows(5,40)+"</subform>",Counts(0,7200,4800,0),Area("<contentArea name=\"short\" w=\"100pt\" h=\"60pt\"/><contentArea name=\"tall\" x=\"120pt\" w=\"100pt\" h=\"100pt\"/>"));
}
std::string Decoration(const std::string& name,RGB color) {
  return "<subform name=\""+name+"\" id=\""+name+"\" layout=\"position\" w=\"60pt\" h=\"8pt\"><occur min=\"0\" max=\"-1\"/>"+Paint("paint",color,60,8)+"</subform>";
}
std::string OverflowBody() {
  return "<proto>"+Decoration("head",magenta)+Decoration("tail",yellow)+"</proto><subform name=\"stream\" layout=\"tb\" w=\"60pt\"><keep intact=\"none\"/><overflow leader=\"#head\" trailer=\"#tail\"/><subform name=\"row\" layout=\"position\" w=\"60pt\" h=\"60pt\"><bind match=\"none\"/><occur min=\"0\" max=\"5\" initial=\"3\"/><keep intact=\"none\"/>"+Paint("upper",green,60,30)+Paint("lower",blue,60,30,"y=\"30pt\"")+"</subform></subform>";
}
struct Step {std::string script;Inspect inspect;};
void Trigger(Session& s,size_t index) {
  s.host->alerts.clear();s.api.FORM_ForceToKillFocus(s.form);const double x=194+10*index;
  FPDF_PAGE control=nullptr;
  for(int page=0;page<s.api.FPDF_GetPageCount(s.document);++page){
    auto candidate=s.host->Page(page);
    if(candidate&&s.api.FPDFPage_HasFormFieldAtPoint(s.form,candidate,x,5)==FPDF_FORMFIELD_XFA_PUSHBUTTON){control=candidate;break;}
  }
  ASSERT_NE(control,nullptr);s.page=control;
  s.api.FORM_OnMouseMove(s.form,s.page,0,x,5);
  ASSERT_TRUE(s.api.FORM_OnLButtonDown(s.form,s.page,0,x,5));
  ASSERT_TRUE(s.api.FORM_OnLButtonUp(s.form,s.page,0,x,5));
  for(size_t page=0;page<s.host->pages.size();++page){s.ClosePage(static_cast<int>(page));}
  s.page=s.host->Page(0);ASSERT_NE(s.page,nullptr);
  ASSERT_EQ(s.host->alerts.size(),1u);EXPECT_EQ(std::get<0>(s.host->alerts[0]),u"PASS");
  s.api.FORM_ForceToKillFocus(s.form);s.api.FORM_OnMouseMove(s.form,s.page,0,230,180);
}
void NativeStableMaster(const Frames& images) {
  // Native incremental relayout can retain blank pages and lose a continuation
  // band in this genuine split fixture. Its unmodified master and right-hand
  // white gutter remain independent, trustworthy paint controls.
  ASSERT_FALSE(images.empty());EXPECT_EQ(Count(images,cyan),64u*images.size());
  for(const auto& image:images){const size_t at=(180*image.width+230)*4;
    EXPECT_EQ(image.pixels[at],255);EXPECT_EQ(image.pixels[at+1],255);EXPECT_EQ(image.pixels[at+2],255);}
}
void Dynamic(const std::string& label,const std::vector<Step>& steps) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);ASSERT_LE(steps.size(),4u);std::string controls;
  for(size_t i=0;i<=steps.size();++i){
    const std::string action=i<steps.size()?steps[i].script:
      "xfa.form.remerge();eq(xfa.form.root.stream._row.count,3);";
    const auto script="function eq(a,b){if(a!==b)throw 'expected '+b+' got '+a;}try{var r=xfa.form.root,l=xfa.layout;"+action+";xfa.host.messageBox('PASS');}catch(e){xfa.host.messageBox('FAIL:'+String(e));}";
    controls+="<field name=\"step"+std::to_string(i)+"\" x=\""+std::to_string(190+10*i)+"pt\" y=\"0pt\" w=\"8pt\" h=\"12pt\"><bind match=\"none\"/><ui><button/></ui><border><edge presence=\"hidden\"/></border><event activity=\"click\"><script contentType=\"application/x-javascript\">"+Escape(script)+"</script></event></field>";
  }
  // Native can retain blank pages after relayout. Trigger searches all pages
  // for the real body control instead of assuming its current view is page0.
  const auto bytes=Fixture("<subform name=\"controls\" layout=\"position\" w=\"240pt\" h=\"20pt\">"+controls+"</subform>"+OverflowBody(),Area());
  Artifacts artifacts;const auto initial=Counts(0,5400,5400,0,-1,1);
  std::array<Frames,2> first,last;size_t backend=0;
  for(const auto* writer:Implementations()){
    SCOPED_TRACE(writer->name());Loaded loaded(*writer,bytes);auto& s=loaded.s;ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    auto image=Render(s);ASSERT_NO_FATAL_FAILURE(initial(image));first[backend]=image;
    for(size_t i=0;i<steps.size();++i){
      ASSERT_NO_FATAL_FAILURE(Trigger(s,i));image=Render(s);
      if(writer->name()=="native"){
        ASSERT_NO_FATAL_FAILURE(NativeStableMaster(image));
        for(size_t page=0;page<image.size();++page)artifacts.Image(label+"-native-step"+std::to_string(i)+"-page"+std::to_string(page),image[page]);
      }else{
        ASSERT_NO_FATAL_FAILURE(steps[i].inspect(image));
        if(i+1==steps.size()||label=="overflow-relayout-repeat"){
          ASSERT_EQ(image.size(),first[backend].size());
        }else{EXPECT_LE(image.size(),first[backend].size());}
      }
      EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
    }
    // Rebuild through a fresh public event before using the native raster as
    // a shared oracle again. Rust must already have passed every intermediate
    // correct payload/decoration assertion; remerge is not its recovery path.
    ASSERT_NO_FATAL_FAILURE(Trigger(s,steps.size()));image=Render(s);
    ASSERT_NO_FATAL_FAILURE(initial(image));last[backend]=image;
    Compare(first[backend],last[backend],artifacts,label+writer->name()+"-remerge-control");
    ASSERT_NO_FATAL_FAILURE(SaveBothModes(*writer,s,image,initial,artifacts,label));++backend;
  }
  Compare(first[0],first[1],artifacts,label+"-initial-backends");
  Compare(last[0],last[1],artifacts,label+"-rebuilt-backends");
}
const std::string Shrink="for(var i=0;i<3;i++){var n=r.stream.resolveNode('row['+i+']');n.h='20pt';n.upper.h='10pt';n.lower.y='10pt';n.lower.h='10pt';eq(n.h,'20pt');eq(n.upper.h,'10pt');eq(n.lower.h,'10pt');}eq(r.stream._row.count,3);l.relayout();";
const std::string Grow="for(var i=0;i<3;i++){var n=r.stream.resolveNode('row['+i+']');n.h='60pt';n.upper.h='30pt';n.lower.y='30pt';n.lower.h='30pt';eq(n.h,'60pt');eq(n.upper.h,'30pt');eq(n.lower.h,'30pt');}eq(r.stream._row.count,3);l.relayout();";
TEST(CoverageXfaPaginationInteractions, ShrinkingOverflowContentRetiresDecorationsAndGrowingRecreatesOnlyNeededCopies) {
  Dynamic("overflow-shrink-grow",{{Shrink,Counts(0,1800,1800,0,1)}, {Grow,Counts(0,5400,5400,0,-1,1)}});
}
TEST(CoverageXfaPaginationInteractions, ClearingAndRestoringOverflowReferencesDoesNotLeaveGeneratedInk) {
  Dynamic("overflow-clear-restore",{{"r.stream.overflow.leader='';r.stream.overflow.trailer='';eq(r.stream.overflow.leader,'');eq(r.stream.overflow.trailer,'');l.relayout();",Counts(0,5400,5400,0)}, {"r.stream.overflow.leader='#head';r.stream.overflow.trailer='#tail';eq(r.stream.overflow.leader,'#head');eq(r.stream.overflow.trailer,'#tail');l.relayout();",Counts(0,5400,5400,0,-1,1)}});
}
TEST(CoverageXfaPaginationInteractions, HidingAndRestoringTheOverflowBodyReleasesItsGeneratedPages) {
  Dynamic("overflow-hide-restore",{{"r.stream.presence='hidden';eq(r.stream.presence,'hidden');l.relayout();",Counts(0,0,0,0,1)}, {"r.stream.presence='visible';eq(r.stream.presence,'visible');l.relayout();",Counts(0,5400,5400,0,-1,1)}});
}
TEST(CoverageXfaPaginationInteractions, RemovingAndReinsertingAMiddleOverflowInstanceRetiresOnlyItsPayload) {
  Dynamic("overflow-remove-middle",{{"r.stream._row.removeInstance(1);eq(r.stream._row.count,2);l.relayout();",Counts(0,3600,3600,0,-1,-1)}, {"r.stream._row.insertInstance(1,false);eq(r.stream._row.count,3);l.relayout();",Counts(0,5400,5400,0,-1,1)}});
}
TEST(CoverageXfaPaginationInteractions, RepeatedOverflowRelayoutDoesNotAccumulateLeaderOrTrailerPaint) {
  Dynamic("overflow-relayout-repeat",{{"eq(r.stream._row.count,3);l.relayout();l.relayout();eq(r.stream._row.count,3);",Counts(0,5400,5400,0,-1,1)}, {"eq(r.stream._row.count,3);l.relayout();l.relayout();l.relayout();eq(r.stream._row.count,3);",Counts(0,5400,5400,0,-1,1)}});
}
TEST(CoverageXfaPaginationInteractions, RemergingTheTemplateAfterOverflowMutationsRebuildsTheOriginalPageTree) {
  Dynamic("overflow-remerge",{{Shrink,Counts(0,1800,1800,0,1)}, {"xfa.form.remerge();eq(xfa.form.root.stream._row.count,3);",Counts(0,5400,5400,0,-1,1)}});
}
} // namespace
} // namespace pdfoxium::integration
