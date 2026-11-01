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
      "<subform name=\"root\" layout=\"tb\" w=\"180pt\" restoreState=\"auto\">"+pages+body+"</subform></template>";
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


// These scenarios follow CalculateContainerSpecifiedSize/ComponentSize and
// DoLayoutFlowedContainer, rather than treating maxW/maxH as CSS constraints.
// Every check is repeated after both writers and both independent readers.
std::string Box(const std::string& attributes,const std::string& properties="",
                const std::string& paint=Block("paint",green,20,20)) {
  return "<subform name=\"box\" layout=\"position\" "+attributes+">"+properties+paint+"</subform>";
}
void Geometry(const std::string& label,const std::string& box,int x,int y,int next_y,
              size_t green_pixels=400) {
  Scenario(label,box+Block("following",blue,20,10),Pages("<contentArea w=\"180pt\" h=\"220pt\"/>"),
    [=](const Frames& images){ASSERT_EQ(images.size(),1u);
      EXPECT_EQ(Count(images,green),green_pixels);EXPECT_EQ(Count(images,blue),200u);
      Pixel(images,0,x+5,y+5,green);Pixel(images,0,5,next_y+5,blue);
    });
}

TEST(CoverageXfaLayoutConstraints, AutoWidthUsesTheVisibleChildExtentForCenteredPlacement) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("auto-centered",Box("hAlign=\"center\""),80,0,20);
}

TEST(CoverageXfaLayoutConstraints, MaxWidthProvidesTheContainerWidthWhenExplicitWidthIsAbsent) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("max-width-center",Box("maxW=\"80pt\" hAlign=\"center\""),50,0,20);
}

TEST(CoverageXfaLayoutConstraints, MaxWidthAlsoControlsRightAlignedContainerPlacement) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("max-width-right",Box("maxW=\"80pt\" hAlign=\"right\""),100,0,20);
}

TEST(CoverageXfaLayoutConstraints, ExplicitWidthTakesPrecedenceOverTheOptionalMaximumWidth) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("explicit-width",Box("w=\"60pt\" maxW=\"80pt\" hAlign=\"center\""),60,0,20);
}

TEST(CoverageXfaLayoutConstraints, ZeroExplicitWidthLeavesTheMaximumWidthAvailable) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("zero-width-max",Box("w=\"0pt\" maxW=\"80pt\" hAlign=\"center\""),50,0,20);
}

TEST(CoverageXfaLayoutConstraints, ZeroMaximumWidthLeavesTheContentWidthAutomatic) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("zero-max-width",Box("maxW=\"0pt\" hAlign=\"center\""),80,0,20);
}

TEST(CoverageXfaLayoutConstraints, MaximumHeightReservesFlowSpaceForAnOtherwiseAutomaticContainer) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("max-height",Box("maxH=\"60pt\""),0,0,60);
}

TEST(CoverageXfaLayoutConstraints, MaximumWidthAndHeightCanSpecifyBothAutomaticDimensions) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("max-both",Box("maxW=\"80pt\" maxH=\"60pt\" hAlign=\"center\""),50,0,60);
}

TEST(CoverageXfaLayoutConstraints, ZeroExplicitHeightLeavesAutomaticContentHeight) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("zero-height",Box("h=\"0pt\""),0,0,20);
}

TEST(CoverageXfaLayoutConstraints, ZeroMaximumHeightLeavesAutomaticContentHeight) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("zero-max-height",Box("maxH=\"0pt\""),0,0,20);
}

TEST(CoverageXfaLayoutConstraints, ExplicitContainerHeightReservesBlankFlowSpace) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("fixed-height",Box("w=\"60pt\" h=\"60pt\""),0,0,60);
}

TEST(CoverageXfaLayoutConstraints, AutomaticWidthIncludesBothHorizontalMarginInsets) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("auto-margin-width",Box("hAlign=\"center\"","<margin leftInset=\"10pt\" rightInset=\"10pt\"/>"),80,0,20);
}

TEST(CoverageXfaLayoutConstraints, AutomaticHeightIncludesBothVerticalMarginInsets) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("auto-margin-height",Box("","<margin topInset=\"5pt\" bottomInset=\"7pt\"/>"),0,5,32);
}

TEST(CoverageXfaLayoutConstraints, SpecifiedMaximumWidthStillPositionsItsChildInsideTheMargin) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("max-margin-width",Box("maxW=\"80pt\" hAlign=\"center\"","<margin leftInset=\"10pt\" rightInset=\"10pt\"/>"),60,0,20);
}

TEST(CoverageXfaLayoutConstraints, SpecifiedHeightContainsItsMarginsWithoutAddingFlowHeight) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Geometry("fixed-margin-height",Box("w=\"60pt\" h=\"60pt\"","<margin topInset=\"5pt\" bottomInset=\"7pt\"/>"),0,5,60);
}

std::string ReferenceProperties() {
  return "<proto><draw name=\"propertyOwner\"><value><text id=\"notContainer\">property</text></value></draw></proto>";
}
void Recovery(const std::string& label,const std::string& properties) {
  Scenario(label,ReferenceProperties()+Cell("payload",green,20,properties)+Cell("following",blue,20),Pages(),
    [](const Frames& images){ASSERT_EQ(images.size(),1u);EXPECT_EQ(Count(images,green),1200u);
      EXPECT_EQ(Count(images,blue),1200u);Pixel(images,0,5,5,green);Pixel(images,0,5,25,blue);});
}

TEST(CoverageXfaLayoutConstraints, MissingBookendLeaderPreservesTheBody) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Recovery("MissingBookendLeaderPreservesTheBody","<bookend leader=\"#absent\"/>");
}

TEST(CoverageXfaLayoutConstraints, MissingBookendTrailerPreservesTheBody) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Recovery("MissingBookendTrailerPreservesTheBody","<bookend trailer=\"#absent\"/>");
}

TEST(CoverageXfaLayoutConstraints, NonContainerBookendLeaderIsIgnored) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Recovery("NonContainerBookendLeaderIsIgnored","<bookend leader=\"#notContainer\"/>");
}

TEST(CoverageXfaLayoutConstraints, NonContainerBookendTrailerIsIgnored) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Recovery("NonContainerBookendTrailerIsIgnored","<bookend trailer=\"#notContainer\"/>");
}

TEST(CoverageXfaLayoutConstraints, LegacyBookendLeaderResolvesNonContainerSafely) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Recovery("LegacyBookendLeaderResolvesNonContainerSafely","<break bookendLeader=\"#notContainer\"/>");
}

TEST(CoverageXfaLayoutConstraints, LegacyBookendTrailerResolvesNonContainerSafely) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Recovery("LegacyBookendTrailerResolvesNonContainerSafely","<break bookendTrailer=\"#notContainer\"/>");
}

TEST(CoverageXfaLayoutConstraints, NonContainerAutoBreakLeaderDoesNotRemoveTheFollowingBody) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Recovery("NonContainerAutoBreakLeaderDoesNotRemoveTheFollowingBody","<breakAfter targetType=\"auto\" leader=\"#notContainer\"/>");
}

TEST(CoverageXfaLayoutConstraints, NonContainerAutoBreakTrailerDoesNotRemoveTheFollowingBody) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Recovery("NonContainerAutoBreakTrailerDoesNotRemoveTheFollowingBody","<breakAfter targetType=\"auto\" trailer=\"#notContainer\"/>");
}

TEST(CoverageXfaLayoutConstraints, MissingAutoBreakDecorationsDoNotCreateBlankPages) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Recovery("MissingAutoBreakDecorationsDoNotCreateBlankPages","<breakAfter targetType=\"auto\" leader=\"#absent\" trailer=\"#absent\"/>");
}

TEST(CoverageXfaLayoutConstraints, EmptyLegacyOverflowPropertiesDoNotCreateDecorationItems) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Recovery("EmptyLegacyOverflowPropertiesDoNotCreateDecorationItems","<break overflowLeader=\"\" overflowTrailer=\"\" overflowTarget=\"\"/>");
}

void OverflowRecovery(const std::string& label,const std::string& properties,bool inherited) {
  const auto rows=Rows(3,60);
  const auto content=inherited?"<subform name=\"outer\" layout=\"tb\">"+properties+rows+"</subform>":Rows(3,60,properties);
  Scenario(label,ReferenceProperties()+content,Pages(),Payload(3,60,3));
}

TEST(CoverageXfaLayoutConstraints, MissingOverflowTargetUsesNormalAutomaticPagination) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  OverflowRecovery("MissingOverflowTargetUsesNormalAutomaticPagination","<overflow target=\"#absent\"/>",false);
}

TEST(CoverageXfaLayoutConstraints, NonPageOverflowTargetUsesNormalAutomaticPagination) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  OverflowRecovery("NonPageOverflowTargetUsesNormalAutomaticPagination","<overflow target=\"#notContainer\"/>",false);
}

TEST(CoverageXfaLayoutConstraints, InheritedMissingOverflowTargetUsesNormalAutomaticPagination) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  OverflowRecovery("InheritedMissingOverflowTargetUsesNormalAutomaticPagination","<overflow target=\"#absent\"/>",true);
}

TEST(CoverageXfaLayoutConstraints, InheritedNonPageOverflowTargetUsesNormalAutomaticPagination) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  OverflowRecovery("InheritedNonPageOverflowTargetUsesNormalAutomaticPagination","<overflow target=\"#notContainer\"/>",true);
}

TEST(CoverageXfaLayoutConstraints, UnusedBoundLeaderIsDiscardedWhenTheBodyFits) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto decoration="<subform name=\"decoration\" id=\"decoration\" layout=\"position\" w=\"60pt\" h=\"10pt\">"
      "<occur min=\"0\" max=\"-1\"/><field name=\"bound\" presence=\"invisible\" w=\"10pt\" h=\"10pt\">"
      "<ui><textEdit/></ui><value><text>Seed</text></value></field>"+Block("paint",magenta,60,10)+"</subform>";
  Scenario("unused-bound-leader","<proto>"+decoration+"</proto>"+
    Cell("payload",green,40,"<overflow leader=\"#decoration\"/>"),Pages(),
    [](const Frames& images){ASSERT_EQ(images.size(),1u);EXPECT_EQ(Count(images,green),2400u);
      EXPECT_EQ(Count(images,magenta),0u);Pixel(images,0,5,5,green);});
}

TEST(CoverageXfaLayoutConstraints, UnusedBoundTrailerIsDiscardedWhenTheBodyFits) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto decoration="<subform name=\"decoration\" id=\"decoration\" layout=\"position\" w=\"60pt\" h=\"10pt\">"
      "<occur min=\"0\" max=\"-1\"/><field name=\"bound\" presence=\"invisible\" w=\"10pt\" h=\"10pt\">"
      "<ui><textEdit/></ui><value><text>Seed</text></value></field>"+Block("paint",magenta,60,10)+"</subform>";
  Scenario("unused-bound-trailer","<proto>"+decoration+"</proto>"+
    Cell("payload",green,40,"<overflow trailer=\"#decoration\"/>"),Pages(),
    [](const Frames& images){ASSERT_EQ(images.size(),1u);EXPECT_EQ(Count(images,green),2400u);
      EXPECT_EQ(Count(images,magenta),0u);Pixel(images,0,5,5,green);});
}

void NextMaster(const std::string& label,int next_height,int child_height,bool fits) {
  const auto content=[](int h){return "<contentArea w=\"180pt\" h=\""+std::to_string(h)+"pt\"/>";};
  Scenario(label,Cell("payload",green,child_height,"<keep intact=\"contentArea\"/>"),
    "<pageSet relation=\"orderedOccurrence\">"+Area("first",content(100)+Master(red),Occur(1,1))+
    Area("second",content(next_height)+Master(cyan),Occur(1,1))+
    Area("last",content(220)+Master(blue),Occur(0,-1))+"</pageSet>",
    [=](const Frames& images){EXPECT_EQ(Count(images,green),static_cast<size_t>(60*child_height));
      EXPECT_EQ(Count(images,red),100u);
      if(fits){ASSERT_GE(images.size(),2u);Pixel(images,0,5,5,{255,255,255});Pixel(images,1,5,5,green);}
      else {ASSERT_GE(images.size(),1u);Pixel(images,0,5,5,green);}
    });
}

TEST(CoverageXfaLayoutConstraints, OccurrenceMaximumLookaheadRollsBackItsSpeculativeNextPage) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  NextMaster("next-master-tall",180,150,true);
}

TEST(CoverageXfaLayoutConstraints, OccurrenceMaximumLookaheadRejectsAShortImmediateNextMaster) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  NextMaster("next-master-short",80,150,false);
}

TEST(CoverageXfaLayoutConstraints, ZeroHeightContinuationAreaUsesTheRemainingPhysicalPage) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Scenario("zero-continuation-60",Cell("prefix",red,20)+Cell("payload",green,100)+Cell("tail",blue,40),
    Pages("<contentArea w=\"60pt\" h=\"20pt\"/><contentArea x=\"60pt\" w=\"60pt\" h=\"0pt\"/>"),
    [](const Frames& images){ASSERT_EQ(images.size(),1u);EXPECT_EQ(Count(images,red),1200u);
      EXPECT_EQ(Count(images,green),6000u);EXPECT_EQ(Count(images,blue),2400u);
      Pixel(images,0,5,5,red);Pixel(images,0,65,5,green);Pixel(images,0,65,105,blue);});
}

TEST(CoverageXfaLayoutConstraints, ZeroHeightContinuationAreaPreservesItsHorizontalOffset) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Scenario("zero-continuation-120",Cell("prefix",red,20)+Cell("payload",green,100)+Cell("tail",blue,40),
    Pages("<contentArea w=\"60pt\" h=\"20pt\"/><contentArea x=\"120pt\" w=\"60pt\" h=\"0pt\"/>"),
    [](const Frames& images){ASSERT_EQ(images.size(),1u);EXPECT_EQ(Count(images,red),1200u);
      EXPECT_EQ(Count(images,green),6000u);EXPECT_EQ(Count(images,blue),2400u);
      Pixel(images,0,5,5,red);Pixel(images,0,125,5,green);Pixel(images,0,125,105,blue);});
}

}  // namespace
}  // namespace pdfoxium::integration
