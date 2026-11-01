#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
struct Bounds {int left=240,top=300,right=-1,bottom=-1,count=0;};
using Inspect=std::function<void(const Raster&)>;
std::vector<Bounds> Lines(const Raster& image,int y0,int height) {
  std::vector<Bounds> lines;
  for(int y=y0;y<y0+height;++y){Bounds row;for(int x=0;x<220;++x){const auto i=(size_t(y)*image.width+x)*4;
    if(image.pixels[i]<128&&image.pixels[i+1]<128&&image.pixels[i+2]<128){row.left=std::min(row.left,x);row.right=std::max(row.right,x);++row.count;}}
    if(!row.count)continue;
    if(lines.empty()||lines.back().bottom!=y-1){row.top=row.bottom=y;lines.push_back(row);}
    else {auto& line=lines.back();line.left=std::min(line.left,row.left);line.right=std::max(line.right,row.right);line.bottom=y;line.count+=row.count;}
  }return lines;
}
struct Spec {std::string para,control_para="lineHeight=\"24pt\"",text="III",control_text;bool auto_height=false;};
std::string TextDraw(const std::string& name,int y,const std::string& text,const std::string& para,bool automatic) {
  return "<draw name=\""+name+"\" "+(automatic?"":"x=\"10pt\" y=\""+std::to_string(y)+"pt\" ")+"w=\"180pt\" "+(automatic?"":"h=\"110pt\" ")+">"
      "<font typeface=\"Cousine\" size=\"20pt\"/><para "+para+"/><value><text>"+text+"</text></value></draw>";
}
std::string Flow(const std::string& name,int y,const std::string& text,const std::string& para,const char* color) {
  return "<subform name=\""+name+"Flow\" layout=\"tb\" x=\"10pt\" y=\""+std::to_string(y)+"pt\" w=\"180pt\">"+
      TextDraw(name,0,text,para,true)+"<draw w=\"60pt\" h=\"4pt\"><value><rectangle><fill><color value=\""+color+"\"/></fill>"
      "<edge presence=\"hidden\"/></rectangle></value></draw></subform>";
}
std::vector<uint8_t> Fixture(const Spec& s) {
  const auto control=s.control_text.empty()?s.text:s.control_text;
  const auto body=s.auto_height?Flow("control",10,control,s.control_para,"0,0,255")+Flow("sample",140,s.text,s.para,"0,128,0"):
      TextDraw("control",10,control,s.control_para,false)+TextDraw("sample",140,s.text,s.para,false);
  const auto xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" restoreState=\"auto\">"
      "<pageSet><pageArea><occur min=\"1\" max=\"-1\"/><medium short=\"240pt\" long=\"300pt\"/><contentArea w=\"240pt\" h=\"300pt\"/></pageArea></pageSet>"+
      body+"<draw x=\"220pt\" y=\"280pt\" w=\"10pt\" h=\"10pt\"><value><rectangle><fill><color value=\"255,0,255\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw></subform></template>";
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 300] >>",
      "<< /Fields [] /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("",xml),PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),PdfStream("","</xdp:xdp>")});
}
struct Loaded {forms::XfaCallbacks callbacks;forms::Session s;
  Loaded(const Library& library,const std::vector<uint8_t>& bytes):s(library.api(),bytes,true,true,2,[this](forms::FormHost& h){callbacks.Configure(h);}){}
};
void Landmark(const Raster& image) {
  ASSERT_EQ(image.width,240);ASSERT_EQ(image.height,300);const auto i=(size_t(285)*image.width+225)*4;
  EXPECT_EQ(image.pixels[i],255);EXPECT_EQ(image.pixels[i+1],0);EXPECT_EQ(image.pixels[i+2],255);
}
void Scenario(const std::string& label,const Spec& spec,const Inspect& inspect) {
  Artifacts artifacts;const auto bytes=Fixture(spec);artifacts.Pdf(label+"-input",bytes);std::array<Raster,2> images;size_t index=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Loaded loaded(*writer,bytes);ASSERT_TRUE(loaded.s.valid());ASSERT_TRUE(loaded.s.xfa_loaded);ASSERT_EQ(loaded.s.api.FPDF_GetPageCount(loaded.s.document),1);
    images[index]=loaded.s.Draw(240,300);Landmark(images[index]);ASSERT_NO_FATAL_FAILURE(inspect(images[index]));
    CompareRaster(images[index],loaded.s.Draw(240,300),artifacts,writer->name()+label+"-repeat",RasterTolerance::ConnectedError(1.0));
    const auto saved=SaveDocument(loaded.s.api,loaded.s.document,artifacts,writer->name()+label);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()){SCOPED_TRACE(reader->name());Loaded reopened(*reader,saved);ASSERT_TRUE(reopened.s.valid());ASSERT_TRUE(reopened.s.xfa_loaded);
      const auto after=reopened.s.Draw(240,300);Landmark(after);ASSERT_NO_FATAL_FAILURE(inspect(after));
      CompareRaster(images[index],after,artifacts,writer->name()+reader->name()+label,RasterTolerance::ConnectedError(1.0));
      EXPECT_TRUE(reopened.s.host->callbacks_valid);EXPECT_FALSE(reopened.s.host->callback_error);}
    EXPECT_TRUE(loaded.s.host->callbacks_valid);EXPECT_FALSE(loaded.s.host->callback_error);++index;
  }CompareRaster(images[0],images[1],artifacts,label+"-engines",RasterTolerance::ConnectedError(1.0));
}
// Cousine's authored 600/1000 advance gives12pt at20pt. Expectations use that
// fixed font metric and known container geometry, never a native output oracle.
void Single(const std::string& label,const std::string& para,int shift,int right_delta=0,const std::string& text="III",const std::string& control="") {
  Spec s;s.para=para;s.text=text;s.control_text=control;
  Scenario(label,s,[=](const Raster& image){const auto a=Lines(image,0,130),b=Lines(image,130,140);ASSERT_EQ(a.size(),1u);ASSERT_EQ(b.size(),1u);
    EXPECT_GT(a[0].count,20);EXPECT_GT(b[0].count,20);EXPECT_NEAR(b[0].left-a[0].left,shift,1);
    EXPECT_NEAR(b[0].right-a[0].right,shift+right_delta,1);EXPECT_NEAR(b[0].top-a[0].top,130,1);});
}
void Spacing(const std::string& label,const std::string& para,int lines,int initial_shift,int subsequent_shift) {
  Spec s;s.para=para;s.text="III";for(int i=1;i<lines;++i)s.text+="&#xA;III";
  Scenario(label,s,[=](const Raster& image){const auto a=Lines(image,0,130),b=Lines(image,130,140);ASSERT_EQ(a.size(),size_t(lines));ASSERT_EQ(b.size(),size_t(lines));
    for(int i=0;i<lines;++i){EXPECT_NEAR(b[i].left-a[i].left,0,1);EXPECT_NEAR(b[i].top-a[i].top,130+initial_shift+i*subsequent_shift,1);}});
}
int ColorTop(const Raster& image,int green) {
  for(int y=0;y<300;++y)for(int x=10;x<70;++x){const auto i=(size_t(y)*image.width+x)*4;
    if(image.pixels[i]==(green?0:255)&&image.pixels[i+1]==(green?128:0)&&image.pixels[i+2]==0)return y;}
  ADD_FAILURE()<<"missing flowed color marker";return -1;
}
void FlowSpacing(const std::string& label,const std::string& para,int added) {
  Spec s;s.auto_height=true;s.para=para;
  Scenario(label,s,[=](const Raster& image){const int control=ColorTop(image,0),sample=ColorTop(image,1);ASSERT_GT(control,10);ASSERT_GT(sample,140);
    EXPECT_NEAR(sample-control,130+added,1);});
}

TEST(CoverageXfaPlainParagraph, LeftMarginMovesThePlainTextOrigin) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Single("left-margin","marginLeft=\"12pt\"",12);
}

TEST(CoverageXfaPlainParagraph, CenteredParagraphUsesTheWholeSpecifiedDrawWidth) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Single("center-paragraph","hAlign=\"center\"",72);
}

TEST(CoverageXfaPlainParagraph, RightAlignedParagraphUsesTheWholeSpecifiedDrawWidth) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Single("right-paragraph","hAlign=\"right\"",144);
}

TEST(CoverageXfaPlainParagraph, CenteredParagraphRespectsBothAsymmetricMargins) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Single("center-margins","hAlign=\"center\" marginLeft=\"12pt\" marginRight=\"24pt\"",66);
}

TEST(CoverageXfaPlainParagraph, RightAlignedParagraphRespectsTheRightMargin) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Single("right-margin","hAlign=\"right\" marginLeft=\"12pt\" marginRight=\"24pt\"",120);
}

TEST(CoverageXfaPlainParagraph, ExplicitFirstLineIndentMovesThePlainTextOrigin) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Single("positive-indent","textIndent=\"24pt\"",24);
}

TEST(CoverageXfaPlainParagraph, FirstLineIndentIsAddedToTheLeftMargin) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Single("margin-indent","marginLeft=\"12pt\" textIndent=\"24pt\"",36);
}

TEST(CoverageXfaPlainParagraph, NegativeIndentDoesNotPlaceTheFirstGlyphBeforeTheLeftMargin) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Single("negative-indent","marginLeft=\"12pt\" textIndent=\"-24pt\"",12);
}

TEST(CoverageXfaPlainParagraph, JustifiedFinalParagraphLineRetainsItsNaturalWordSpacing) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Single("justify-final","hAlign=\"justify\"",0,0,"I I I");
}

TEST(CoverageXfaPlainParagraph, JustifyAllDistributesTheFinalParagraphLineAcrossTheAvailableWidth) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Single("justify-all-final","hAlign=\"justifyAll\"",0,120,"I I I");
}

TEST(CoverageXfaPlainParagraph, TrailingSpacesAreTrimmedBeforeRightAlignment) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Single("right-trailing-spaces","hAlign=\"right\"",144,0,"III   ","III");
}

TEST(CoverageXfaPlainParagraph, NonbreakingSpacesUseThePlainTextSpaceAdvance) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Single("plain-nbsp","",0,0,"I&#xA0;I","I I");
}

TEST(CoverageXfaPlainParagraph, SpaceAboveMovesTheFirstPlainParagraphBaseline) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spacing("space-above","lineHeight=\"24pt\" spaceAbove=\"8pt\"",1,8,0);
}

TEST(CoverageXfaPlainParagraph, SpaceAboveIsAppliedOnceBeforeSeveralPlainParagraphs) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spacing("space-above-multiline","lineHeight=\"24pt\" spaceAbove=\"8pt\"",3,8,0);
}

TEST(CoverageXfaPlainParagraph, ExplicitLineHeightMovesEachSubsequentBaseline) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spacing("line-height-36","lineHeight=\"36pt\"",3,0,12);
}

TEST(CoverageXfaPlainParagraph, ZeroLineHeightUsesTheFontBasedDefaultForSubsequentLines) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spacing("line-height-zero","lineHeight=\"0pt\"",3,0,0);
}

TEST(CoverageXfaPlainParagraph, AutoHeightReservesParagraphSpaceBelowBeforeTheFollowingDraw) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  FlowSpacing("auto-space-below","spaceBelow=\"12pt\"",12);
}

TEST(CoverageXfaPlainParagraph, AutoHeightReservesBothParagraphSpacesBeforeTheFollowingDraw) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  FlowSpacing("auto-space-both","spaceAbove=\"8pt\" spaceBelow=\"12pt\"",20);
}

TEST(CoverageXfaPlainParagraph, RightMarginChangesWrappingWithoutLosingAnyPlainTextGlyphs) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.text="IIIIIIIIIIIIIIII";s.para="lineHeight=\"24pt\" marginRight=\"24pt\"";
  Scenario("right-margin-wrap",s,[](const Raster& image){const auto a=Lines(image,0,130),b=Lines(image,130,140);ASSERT_EQ(a.size(),2u);ASSERT_EQ(b.size(),2u);
    EXPECT_NEAR(b[0].left-a[0].left,0,1);EXPECT_NEAR(b[0].right-a[0].right,-24,1);EXPECT_NEAR(b[1].right-a[1].right,24,1);
    EXPECT_NEAR(b[1].top-a[1].top,130,1);});
}

TEST(CoverageXfaPlainParagraph, FirstLineIndentChangesOnlyTheFirstWrappedLineOrigin) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.text="IIIIIIIIIIIIIIII";s.para="lineHeight=\"24pt\" textIndent=\"24pt\"";
  Scenario("first-indent-wrap",s,[](const Raster& image){const auto a=Lines(image,0,130),b=Lines(image,130,140);ASSERT_EQ(a.size(),2u);ASSERT_EQ(b.size(),2u);
    EXPECT_NEAR(b[0].left-a[0].left,24,1);EXPECT_NEAR(b[0].right-a[0].right,0,1);EXPECT_NEAR(b[1].left-a[1].left,0,1);
    EXPECT_NEAR(b[1].right-a[1].right,24,1);});
}

}  // namespace
}  // namespace pdfoxium::integration
