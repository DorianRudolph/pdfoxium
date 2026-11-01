#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <functional>
#include <string>
#include <utility>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using RGB = std::array<int, 3>;
using Frames = std::vector<Raster>;
using Inspector = std::function<void(const Frames&)>;
const RGB red{255,0,0}, green{0,128,0}, blue{0,0,255}, magenta{255,0,255}, cyan{0,255,255};

std::string Block(const std::string& name, RGB color, int width, int height,
                  const std::string& attributes = "") {
  return "<draw name=\"" + name + "\" w=\"" + std::to_string(width) + "pt\" h=\"" +
      std::to_string(height) + "pt\" " + attributes + "><value><rectangle><fill><color value=\"" +
      std::to_string(color[0]) + "," + std::to_string(color[1]) + "," + std::to_string(color[2]) +
      "\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
}
std::string Cell(const std::string& name, RGB color, int height,
                 const std::string& properties = "", const std::string& attributes = "") {
  return "<subform name=\"" + name + "\" layout=\"position\" w=\"60pt\" h=\"" +
      std::to_string(height) + "pt\" " + attributes + ">" + properties + Block("paint",color,60,height) + "</subform>";
}
std::string Occur(int minimum=1, int maximum=-1) {
  return "<occur min=\"" + std::to_string(minimum) + "\" max=\"" + std::to_string(maximum) + "\"/>";
}
std::string Escape(const std::string& value) {
  std::string out;
  for(char c:value) {
    if(c=='&') out += "&amp;";
    else if(c=='<') out += "&lt;";
    else if(c=='>') out += "&gt;";
    else out += c;
  }
  return out;
}
std::string Event(const std::string& script) {
  return "<event activity=\"click\"><script contentType=\"application/x-javascript\">" + Escape(script) + "</script></event>";
}
std::string Master(RGB color) {
  return Block("stationery",color,10,10,"x=\"220pt\" y=\"10pt\"");
}
std::string Area(const std::string& name, const std::string& content,
                 const std::string& occurrence=Occur(), const std::string& attributes="") {
  return "<pageArea name=\""+name+"\" id=\""+name+"\" "+attributes+">"+occurrence+
      "<medium short=\"240pt\" long=\"240pt\"/>"+content+"</pageArea>";
}
std::string Pages(const std::string& areas="<contentArea w=\"180pt\" h=\"100pt\"/>") {
  return "<pageSet name=\"pages\">"+Area("page",areas)+"</pageSet>";
}
std::vector<uint8_t> Fixture(const std::string& body, const std::string& pages) {
  // Template/page-set nodes have no public constructors; every merge, event,
  // layout, page callback, paint and save below runs through the public API.
  const auto xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
      "<subform name=\"root\" layout=\"tb\" restoreState=\"auto\">"+pages+body+"</subform></template>";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 240] >>",
      "<< /Fields [] /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("",xml),
      PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
      PdfStream("","</xdp:xdp>")});
}
struct Loaded {
  forms::XfaCallbacks callbacks;
  Session s;
  Loaded(const Library& library,const std::vector<uint8_t>& bytes)
      :s(library.api(),bytes,true,true,2,[this](forms::FormHost& host){callbacks.Configure(host);}) {}
};
Frames Render(Session& s) {
  Frames images;const int count=s.api.FPDF_GetPageCount(s.document);
  EXPECT_GT(count,0);EXPECT_LE(count,16);
  if(count<1||count>16)return images;
  for(int p=0;p<count;++p)images.push_back(s.Draw(240,240,0,0,0,0,0,0,p));
  return images;
}
size_t Count(const Frames& images,RGB color) {
  size_t n=0;for(const auto& image:images)for(size_t at=0;at<image.pixels.size();at+=4)
    n+=image.pixels[at]==color[2]&&image.pixels[at+1]==color[1]&&image.pixels[at+2]==color[0];
  return n;
}
void Pixel(const Frames& images,int page,int x,int y,RGB color) {
  ASSERT_GE(page,0);ASSERT_LT(static_cast<size_t>(page),images.size());const auto& image=images[page];
  const size_t at=(static_cast<size_t>(y)*image.width+x)*4;
  EXPECT_EQ(image.pixels[at],color[2]);EXPECT_EQ(image.pixels[at+1],color[1]);EXPECT_EQ(image.pixels[at+2],color[0]);
}
void Compare(const Frames& a,const Frames& b,Artifacts& artifacts,const std::string& label) {
  ASSERT_EQ(a.size(),b.size());
  for(size_t p=0;p<a.size();++p)CompareRaster(a[p],b[p],artifacts,label+"-"+std::to_string(p));
}
void FourWay(const Library& writer,Session& s,Artifacts& artifacts,const std::string& label,
             const Inspector& inspect,const Frames& original) {
  const auto saved=SaveDocument(s.api,s.document,artifacts,writer.name()+label);ASSERT_FALSE(saved.empty());
  for(const auto* reader:Implementations()) {
    SCOPED_TRACE(writer.name()+" -> "+reader->name());Loaded reopened(*reader,saved);
    ASSERT_TRUE(reopened.s.valid());ASSERT_TRUE(reopened.s.xfa_loaded);
    const auto image=Render(reopened.s);
    ASSERT_NO_FATAL_FAILURE(inspect(image));
    Compare(original,image,artifacts,writer.name()+reader->name()+label);
    EXPECT_TRUE(reopened.s.host->callbacks_valid);EXPECT_FALSE(reopened.s.host->callback_error);
  }
}
void Scenario(const std::string& label,const std::string& body,const std::string& pages,const Inspector& inspect) {
  Artifacts artifacts;const auto bytes=Fixture(body,pages);artifacts.Pdf(label+"-input",bytes);
  std::array<Frames,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Loaded loaded(*writer,bytes);ASSERT_TRUE(loaded.s.valid());ASSERT_TRUE(loaded.s.xfa_loaded);
    originals[backend]=Render(loaded.s);
    ASSERT_NO_FATAL_FAILURE(inspect(originals[backend]));
    Compare(originals[backend],Render(loaded.s),artifacts,writer->name()+label+"-repeat");
    FourWay(*writer,loaded.s,artifacts,label,inspect,originals[backend]);++backend;
  }
  Compare(originals[0],originals[1],artifacts,label+"-engines");
}
Inspector Payload(int rows,int height=60,int pages=-1) {
  return [=](const Frames& images){
    if(pages>=0) { ASSERT_EQ(images.size(),static_cast<size_t>(pages)); }
    EXPECT_EQ(Count(images,green),static_cast<size_t>(rows*60*height));
  };
}
std::string Rows(int count,int height=60,const std::string& properties="") {
  std::string body;for(int i=0;i<count;++i)body+=Cell("row"+std::to_string(i),green,height,properties);
  return body;
}

// DoLayoutFlowedContainer's no-split/no-larger-area fallback must place an
// oversized atomic child once instead of retrying the same page indefinitely.
TEST(CoverageXfaLayoutRecovery, OversizedAtomicChildrenMakeProgressWhenNoContentAreaCanFitThem) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int height:{110,150,190})Scenario("oversized-first-"+std::to_string(height),
      Cell("oversized",green,height,"<keep intact=\"contentArea\"/>"),Pages(),Payload(1,height,1));
}
TEST(CoverageXfaLayoutRecovery, OversizedChildFollowingUsedSpaceMovesToAFreshPageBeforeItsForcedPlacement) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const std::string intact:{"contentArea","pageArea"})
    Scenario("oversized-after-prefix-"+intact,Cell("prefix",red,30)+Cell("oversized",green,150,
      "<keep intact=\""+intact+"\"/>"),Pages(),[](const Frames& images){
      ASSERT_EQ(images.size(),2u);EXPECT_EQ(Count(images,red),1800u);EXPECT_EQ(Count(images,green),9000u);
      Pixel(images,0,5,5,red);Pixel(images,1,5,5,green);
    });
}
TEST(CoverageXfaLayoutRecovery, OversizedChildrenUseImmediateAreaLookaheadWithoutSearchingPastAnotherShortArea) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int height:{110,150})Scenario("skip-short-areas-"+std::to_string(height),
      Cell("payload",green,height,"<keep intact=\"contentArea\"/>"),
      Pages("<contentArea w=\"60pt\" h=\"30pt\"/><contentArea x=\"70pt\" w=\"60pt\" h=\"60pt\"/>"
            "<contentArea x=\"140pt\" w=\"60pt\" h=\"180pt\"/>"),[=](const Frames& images){
      ASSERT_EQ(images.size(),1u);EXPECT_EQ(Count(images,green),static_cast<size_t>(60*height));
      // GetNextAvailContentHeight compares only the immediately following area.
      // Neither short area can fit, so the first area forces placement once.
      Pixel(images,0,5,5,green);Pixel(images,0,75,5,{255,255,255});Pixel(images,0,145,5,{255,255,255});
    });
}
TEST(CoverageXfaLayoutRecovery, ExactHeightAtomicRowsDoNotCreateEmptyContinuationPages) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int height:{25,50,100})for(int count:{1,2,4})
    Scenario("exact-fit-"+std::to_string(height)+"-"+std::to_string(count),Rows(count,height),Pages(),
      Payload(count,height,(count*height+99)/100));
}
TEST(CoverageXfaLayoutRecovery, UnusedRequiredMasterOccurrencesRetainStationeryWithoutDuplicatingBody) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int minimum:{2,3})Scenario("required-repeat-"+std::to_string(minimum),Cell("payload",green,20),
    "<pageSet relation=\"orderedOccurrence\">"+Area("page","<contentArea w=\"180pt\" h=\"100pt\"/>"+Master(cyan),Occur(minimum,minimum))+"</pageSet>",
    [=](const Frames& images){ASSERT_EQ(images.size(),static_cast<size_t>(minimum));
      EXPECT_EQ(Count(images,green),1200u);EXPECT_EQ(Count(images,cyan),static_cast<size_t>(minimum*100));Pixel(images,0,5,5,green);
      for(int i=1;i<minimum;++i)Pixel(images,i,5,5,{255,255,255});});
}
TEST(CoverageXfaLayoutRecovery, NestedTrailingPageSetMinimumCreatesAllRequiredStationeryOccurrences) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int minimum:{2,3})Scenario("nested-required-tail-"+std::to_string(minimum),Cell("payload",green,20),
    "<pageSet relation=\"orderedOccurrence\">"+Area("body","<contentArea w=\"180pt\" h=\"100pt\"/>"+Master(red),Occur(1,1))+
      "<pageSet name=\"tail\" relation=\"orderedOccurrence\">"+Occur(minimum,minimum)+
      Area("stationery","<contentArea w=\"180pt\" h=\"100pt\"/>"+Master(cyan),Occur(1,1))+"</pageSet></pageSet>",
    [=](const Frames& images){ASSERT_EQ(images.size(),static_cast<size_t>(minimum+1));EXPECT_EQ(Count(images,green),1200u);
      EXPECT_EQ(Count(images,red),100u);EXPECT_EQ(Count(images,cyan),static_cast<size_t>(minimum*100));
      Pixel(images,0,5,5,green);for(int i=1;i<=minimum;++i)Pixel(images,i,5,5,{255,255,255});});
}
TEST(CoverageXfaLayoutRecovery, RootOnlyMasterReplacesNestedFirstStationeryForOneBodyPage) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto content="<contentArea w=\"180pt\" h=\"100pt\"/>";
  Scenario("nested-simplex-only",Cell("payload",green,20),
    "<pageSet relation=\"simplexPaginated\"><pageSet name=\"nested\" relation=\"simplexPaginated\">"+
      Area("first",content+Master(red),Occur(),"pagePosition=\"first\"")+
      Area("rest",content+Master(blue),Occur(),"pagePosition=\"rest\"")+"</pageSet>"+
      Area("only",content+Master(cyan),Occur(),"pagePosition=\"only\"")+"</pageSet>",
    [](const Frames& images){ASSERT_EQ(images.size(),1u);EXPECT_EQ(Count(images,green),1200u);
      EXPECT_EQ(Count(images,red),0u);EXPECT_EQ(Count(images,cyan),100u);});
}
TEST(CoverageXfaLayoutRecovery, NestedSimplexLastMastersReplaceOnlyTheLastCompletedRestPage) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto content="<contentArea w=\"180pt\" h=\"100pt\"/>";
  Scenario("nested-simplex-last",Rows(3,60),
    "<pageSet relation=\"simplexPaginated\"><pageSet name=\"nested\" relation=\"simplexPaginated\">"+
      Area("first",content+Master(red),Occur(),"pagePosition=\"first\"")+
      Area("rest",content+Master(blue),Occur(),"pagePosition=\"rest\"")+
      Area("last",content+Master(cyan),Occur(),"pagePosition=\"last\"")+"</pageSet></pageSet>",
    [](const Frames& images){ASSERT_EQ(images.size(),3u);EXPECT_EQ(Count(images,green),10800u);
      EXPECT_EQ(Count(images,red),100u);EXPECT_EQ(Count(images,blue),100u);EXPECT_EQ(Count(images,cyan),100u);
      Pixel(images,0,225,15,red);Pixel(images,1,225,15,blue);Pixel(images,2,225,15,cyan);});
}
TEST(CoverageXfaLayoutRecovery, OnlyMasterWithSeveralContentAreasPreservesAllBodyFragmentsAndOffsets) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto areas="<contentArea w=\"60pt\" h=\"100pt\"/><contentArea x=\"100pt\" w=\"60pt\" h=\"100pt\"/>";
  Scenario("simplex-only-multiple-areas",Rows(2,60),
      "<pageSet relation=\"simplexPaginated\">"+Area("first",areas+Master(red),Occur(),"pagePosition=\"first\"")+
      Area("only",areas+Master(cyan),Occur(),"pagePosition=\"only\"")+"</pageSet>",
      [](const Frames& images){ASSERT_EQ(images.size(),1u);EXPECT_EQ(Count(images,green),7200u);
        EXPECT_EQ(Count(images,cyan),100u);Pixel(images,0,5,5,green);Pixel(images,0,105,5,green);});
}
TEST(CoverageXfaLayoutRecovery, AutomaticPaginationSkipsContentlessStationeryWithoutAnExplicitBreakTarget) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Scenario("required-blank-between",Rows(2,70),
      "<pageSet relation=\"orderedOccurrence\">"+
      Area("first","<contentArea w=\"180pt\" h=\"100pt\"/>"+Master(red),Occur(1,1))+
      Area("blank",Master(blue),Occur(1,1))+
      Area("last","<contentArea w=\"180pt\" h=\"100pt\"/>"+Master(cyan),Occur(1,1))+"</pageSet>",
      [](const Frames& images){ASSERT_EQ(images.size(),2u);EXPECT_EQ(Count(images,green),8400u);
        Pixel(images,0,5,5,green);Pixel(images,1,5,5,green);EXPECT_EQ(Count(images,blue),0u);
        Pixel(images,0,225,15,red);Pixel(images,1,225,15,cyan);});
}
TEST(CoverageXfaLayoutRecovery, KeepChainsContainingExplicitBreaksPreserveEveryMemberAcrossRestartedLayoutStages) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(bool before:{false,true})for(const std::string keep:{"contentArea","pageArea"}) {
    const auto breaker="<"+std::string(before?"breakBefore":"breakAfter")+" targetType=\"pageArea\" startNew=\"1\"/>";
    Scenario("keep-with-break-"+std::to_string(before)+keep,Cell("prefix",red,60)+
      Cell("head",green,20,"<keep next=\""+keep+"\"/>"+(before?"":breaker))+
      Cell("tail",blue,20,"<keep previous=\""+keep+"\"/>"+(before?breaker:"")),Pages(),
      [](const Frames& images){EXPECT_GE(images.size(),2u);EXPECT_LE(images.size(),4u);
        EXPECT_EQ(Count(images,red),3600u);EXPECT_EQ(Count(images,green),1200u);EXPECT_EQ(Count(images,blue),1200u);});
  }
}
TEST(CoverageXfaLayoutRecovery, InitialNonPagingBreakBeforeEmitsOneLeaderWhileFollowingAutoInstructionsRemainNonpaging) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto decoration=[](const char* name,RGB c){return "<subform name=\""+std::string(name)+"\" id=\""+name+"\" layout=\"position\" w=\"20pt\" h=\"5pt\">"+Occur(0,-1)+Block("paint",c,20,5)+"</subform>";};
  const auto proto="<proto>"+decoration("head",magenta)+decoration("tail",cyan)+"</proto>";
  Scenario("multiple-before-auto",proto+Cell("first",green,20,
    "<breakBefore targetType=\"auto\" leader=\"#head\" trailer=\"#tail\"/><breakBefore targetType=\"auto\" leader=\"#head\" trailer=\"#tail\"/>"),Pages(),
    [](const Frames& images){ASSERT_EQ(images.size(),1u);EXPECT_EQ(Count(images,green),1200u);
      // Only the initial first-container break receives the initial-page
      // override. Later auto breaks do not create pages or add decorations.
      EXPECT_EQ(Count(images,magenta),100u);EXPECT_EQ(Count(images,cyan),0u);});
}

struct Step { std::string program; Inspector inspect; std::u16string native_notice{}; };
std::string Probe(const std::string& program,int index) {
  const auto checked="function eq(a,b){if(a!==b)throw 'expected '+b+' got '+a;}"
      "try{var r=xfa.form.root,l=xfa.layout;"+program+";xfa.host.messageBox('PASS');}"
      "catch(e){xfa.host.messageBox('FAIL:'+String(e));}";
  // Captionless buttons give a public input route into scripts while colored
  // body rectangles independently establish the resulting visible state.
  return "<field name=\"action"+std::to_string(index)+"\" x=\""+std::to_string(190+index*10)+
      "pt\" y=\"0pt\" w=\"8pt\" h=\"12pt\"><bind match=\"none\"/><ui><button/></ui>"+
      "<border><edge presence=\"hidden\"/></border>"+Event(checked)+"</field>";
}
void Dynamic(const std::string& label,const std::string& body,const Inspector& initial,
             const std::vector<Step>& steps) {
  ASSERT_LE(steps.size(),5u);std::string controls;
  for(size_t i=0;i<steps.size();++i)controls+=Probe(steps[i].program,static_cast<int>(i));
  const auto pages=Pages("<contentArea w=\"240pt\" h=\"100pt\"/>");
  // A normal positioned body control row supplies real input on the first
  // page. Colored payloads occupy the next flow rows, separately from these
  // captionless buttons, so model and paint assertions remain independent.
  const auto controller="<subform name=\"controls\" layout=\"position\" w=\"240pt\" h=\"20pt\">"+controls+"</subform>";
  const auto bytes=Fixture(controller+body,pages);Artifacts artifacts;artifacts.Pdf(label+"-input",bytes);
  std::array<std::vector<Frames>,2> observations;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());Loaded loaded(*library,bytes);auto& s=loaded.s;
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    auto image=Render(s);
    ASSERT_NO_FATAL_FAILURE(initial(image));observations[backend].push_back(image);
    for(size_t i=0;i<steps.size();++i) {
      SCOPED_TRACE(i);s.host->alerts.clear();s.api.FORM_ForceToKillFocus(s.form);
      const double x=194+10*i;
      ASSERT_EQ(s.api.FPDFPage_HasFormFieldAtPoint(s.form,s.page,x,5),FPDF_FORMFIELD_XFA_PUSHBUTTON);
      s.api.FORM_OnMouseMove(s.form,s.page,0,x,5);
      ASSERT_TRUE(s.api.FORM_OnLButtonDown(s.form,s.page,0,x,5));
      ASSERT_TRUE(s.api.FORM_OnLButtonUp(s.form,s.page,0,x,5));
      // Relayout may replace page-view identities. Release the old handles
      // through their owner, then obtain the new page from the public host.
      for(size_t p=0;p<s.host->pages.size();++p)s.ClosePage(static_cast<int>(p));
      s.page=s.host->Page(0);ASSERT_NE(s.page,nullptr);
      if(library->name()=="native" && !steps[i].native_notice.empty()) {
        ASSERT_EQ(s.host->alerts.size(),2u);
        EXPECT_EQ(std::get<0>(s.host->alerts[0]),steps[i].native_notice);
      } else { ASSERT_EQ(s.host->alerts.size(),1u); }
      EXPECT_EQ(std::get<0>(s.host->alerts.back()),u"PASS");
      s.api.FORM_ForceToKillFocus(s.form);s.api.FORM_OnMouseMove(s.form,s.page,0,230,180);
      image=Render(s);
    ASSERT_NO_FATAL_FAILURE(steps[i].inspect(image));observations[backend].push_back(image);
      EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
    }
    // Each workflow restores its initial geometry after exercising intermediate
    // mutations. Both readers must reproduce that state from both writers.
    ASSERT_NO_FATAL_FAILURE(initial(image));FourWay(*library,s,artifacts,label,initial,image);++backend;
  }
  ASSERT_EQ(observations[0].size(),observations[1].size());
  for(size_t i=0;i<observations[0].size();++i)Compare(observations[0][i],observations[1][i],artifacts,label+"-state"+std::to_string(i));
}
std::string Repeated(int initial=2,int height=60,const std::string& attrs="",const std::string& properties="") {
  return "<subform name=\"row\" layout=\"position\" w=\"60pt\" h=\""+std::to_string(height)+"pt\" "+attrs+">"
      "<bind match=\"none\"/><occur min=\"0\" max=\"5\" initial=\""+std::to_string(initial)+"\"/>"+
      properties+Block("paint",green,60,height)+"</subform>";
}
TEST(CoverageXfaLayoutRecovery, ShrinkingAndGrowingInstancesReusesAndReleasesRealPageViews) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Dynamic("instances-shrink-grow",Repeated(),Payload(2,60,2),{
    {"r._row.count=1;eq(r._row.count,1);l.relayout();",Payload(1,60,1)},
    {"r._row.count=4;eq(r._row.count,4);l.relayout();",Payload(4,60,4)},
    {"r._row.count=2;eq(r._row.count,2);l.relayout();",Payload(2,60,2)}});
}
TEST(CoverageXfaLayoutRecovery, RemovingAllInstancesReleasesOldPagesAndAddingAgainRebuildsTheirContent) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Dynamic("instances-zero-recreate",Repeated(),Payload(2,60,2),{
    {"r._row.count=0;eq(r._row.count,0);l.relayout();",Payload(0,60,1)},
    {// Native InsertItem asks for item[-1] in an empty group and returns
     // without inserting its newly created node. Rust must already attach it;
     // a native-only diagnostic makes falling back in Rust fail the test.
     "var n=r._row.addInstance(false);eq(n!==null,true);"
     "if(n.parent===null){eq(r._row.count,0);xfa.host.messageBox('DETACHED');r.nodes.insert(n,null);}"
     "eq(r._row.count,1);l.relayout();",Payload(1,60,1),u"DETACHED"},
    {"r._row.count=2;l.relayout();",Payload(2,60,2)}});
}
TEST(CoverageXfaLayoutRecovery, RemovingTheFirstInstanceRebindsTheExistingPageToItsNewFirstSubform) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Dynamic("remove-first-page-owner",Repeated(),Payload(2,60,2),{
    {"r._row.removeInstance(0);eq(r._row.count,1);l.relayout();",Payload(1,60,1)},
    {"r._row.insertInstance(0,false);eq(r._row.count,2);l.relayout();",Payload(2,60,2)}});
}
TEST(CoverageXfaLayoutRecovery, RemovingTheLastInstanceAndAppendingAgainPreservesTheFirstPageOwner) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Dynamic("remove-last-page-owner",Repeated(),Payload(2,60,2),{
    {"r._row.removeInstance(1);eq(r._row.count,1);l.relayout();",Payload(1,60,1)},
    {"r._row.addInstance(false);eq(r._row.count,2);l.relayout();",Payload(2,60,2)}});
}
TEST(CoverageXfaLayoutRecovery, MovingInstancesAcrossExistingPagesKeepsTheirLogicalIndicesAndInk) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Dynamic("move-page-owners",Repeated(3),Payload(3,60,3),{
    {"r._row.moveInstance(2,0);eq(r.resolveNode('row[0]').instanceIndex,0);l.relayout();",Payload(3,60,3)},
    {"r._row.moveInstance(0,2);eq(r.resolveNode('row[2]').instanceIndex,2);l.relayout();",Payload(3,60,3)}});
}
TEST(CoverageXfaLayoutRecovery, RepeatedFullRelayoutDoesNotDuplicateMasterOrBodyLayoutItems) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Dynamic("repeated-relayout",Repeated(3),Payload(3,60,3),{
    {"l.relayout();l.relayout();",Payload(3,60,3)},
    {"l.relayout();l.relayout();l.relayout();",Payload(3,60,3)}});
}
TEST(CoverageXfaLayoutRecovery, HiddenInstancesReleaseSpaceAndVisibleRestorationCreatesTheirPageAgain) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Dynamic("hidden-flow-instance",Repeated(),Payload(2,60,2),{
    {"r.resolveNode('row[0]').presence='hidden';l.relayout();",Payload(1,60,1)},
    {"r.resolveNode('row[0]').presence='visible';l.relayout();",Payload(2,60,2)}});
}
TEST(CoverageXfaLayoutRecovery, InvisibleInstancesRetainTheirPageSpaceWhileSuppressingTheirDrawnChildren) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Dynamic("invisible-flow-instance",Repeated(),Payload(2,60,2),{
    {"r.resolveNode('row[0]').presence='invisible';l.relayout();",Payload(1,60,2)},
    {"r.resolveNode('row[0]').presence='visible';l.relayout();",Payload(2,60,2)}});
}
TEST(CoverageXfaLayoutRecovery, HiddenAndInvisibleStatesAlternateAcrossRemovedAndReusedPageRecords) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Dynamic("alternate-presence-relayout",Repeated(),Payload(2,60,2),{
    {"r.resolveNode('row[0]').presence='hidden';l.relayout();",Payload(1,60,1)},
    {"r.resolveNode('row[0]').presence='invisible';l.relayout();",Payload(1,60,2)},
    {"r.resolveNode('row[0]').presence='visible';l.relayout();",Payload(2,60,2)}});
}
TEST(CoverageXfaLayoutRecovery, ScriptedContainerAndDrawHeightChangesRepaginateAndRestoreTheOriginalGeometry) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto resize=[](int h){return "for(var i=0;i<2;i++){var n=r.resolveNode('row['+i+']');n.h='"+
      std::to_string(h)+"pt';n.paint.h='"+std::to_string(h)+"pt';}l.relayout();";};
  Dynamic("resize-flow-instances",Repeated(),Payload(2,60,2),{
    {resize(20),Payload(2,20,1)}, {resize(40),Payload(2,40,1)}, {resize(60),Payload(2,60,2)}});
}
TEST(CoverageXfaLayoutRecovery, SubformKeepChangesMoveTheWholePairAndRestoreSplitPagePlacement) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto initial=[](const Frames& images){ASSERT_EQ(images.size(),2u);EXPECT_EQ(Count(images,red),3000u);
    EXPECT_EQ(Count(images,green),1200u);EXPECT_EQ(Count(images,blue),1200u);Pixel(images,0,5,75,green);Pixel(images,1,5,5,blue);};
  const auto kept=[](const Frames& images){ASSERT_EQ(images.size(),2u);EXPECT_EQ(Count(images,green),1200u);
    EXPECT_EQ(Count(images,blue),1200u);Pixel(images,1,5,5,green);Pixel(images,1,5,25,blue);};
  Dynamic("live-keep-chain",Cell("prefix",red,50)+Cell("head",green,20)+Cell("tail",blue,20),initial,{
    {"r.head.keep.next='contentArea';r.tail.keep.previous='contentArea';l.relayout();",kept},
    {"r.head.keep.next='none';r.tail.keep.previous='none';l.relayout();",initial}});
}
TEST(CoverageXfaLayoutRecovery, RemovedMiddleInstancesAndReinsertedInstancesClearUnusedLayoutChildren) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Dynamic("middle-instance-retirement",Repeated(4),Payload(4,60,4),{
    {"r._row.removeInstance(1);r._row.removeInstance(1);eq(r._row.count,2);l.relayout();",Payload(2,60,2)},
    {"r._row.insertInstance(1,false);r._row.insertInstance(2,false);eq(r._row.count,4);l.relayout();",Payload(4,60,4)}});
}
}  // namespace
}  // namespace pdfoxium::integration
