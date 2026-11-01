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
using RGB=std::array<int,3>;
const RGB red{255,0,0},green{0,128,0},blue{0,0,255},orange{255,128,0};

std::string Block(const std::string& name,RGB color,int width,int height,const std::string& attrs="") {
  return "<draw name=\""+name+"\" w=\""+std::to_string(width)+"pt\" h=\""+std::to_string(height)+"pt\" "+attrs+
    "><value><rectangle><fill><color value=\""+std::to_string(color[0])+","+std::to_string(color[1])+","+std::to_string(color[2])+
    "\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
}
std::string Cell(const std::string& name,RGB color,int height,const std::string& properties="") {
  return "<subform name=\""+name+"\" layout=\"position\" w=\"60pt\" h=\""+std::to_string(height)+"pt\">"+
    properties+Block("paint",color,60,height)+"</subform>";
}
std::string Area(const std::string& name,const std::string& content,const std::string& occur="<occur min=\"1\" max=\"-1\"/>") {
  return "<pageArea name=\""+name+"\" id=\""+name+"\">"+occur+"<medium short=\"240pt\" long=\"240pt\"/>"+content+"</pageArea>";
}
std::string Pageset(const std::string& areas="<contentArea w=\"180pt\" h=\"100pt\"/>") {
  return "<pageSet name=\"pages\">"+Area("page",areas)+"</pageSet>";
}
std::vector<uint8_t> Fixture(const std::string& body,const std::string& pages) {
  // XFA template/page-set constructors are not public C APIs. All subsequent
  // merge, pagination, painting and serialization are engine operations.
  const std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
    "<subform name=\"root\" layout=\"tb\" restoreState=\"auto\">"+pages+body+"</subform></template>";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 240] >>",
    "<< /Fields [] /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
    PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
    PdfStream("",xml),PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
    PdfStream("","</xdp:xdp>")});
}
struct Loaded {
  forms::XfaCallbacks callbacks;
  Session s;
  Loaded(const Library& library,const std::vector<uint8_t>& bytes)
    :s(library.api(),bytes,true,true,2,[this](forms::FormHost& host){callbacks.Configure(host);}) {}
};
using Frames=std::vector<Raster>;
Frames Render(Session& s) {
  Frames result;const int count=s.api.FPDF_GetPageCount(s.document);
  EXPECT_GT(count,0);EXPECT_LE(count,16);
  if(count<1||count>16)return result;
  for(int page=0;page<count;++page)result.push_back(s.Draw(240,240,0,0,0,0,0,0,page));
  return result;
}
size_t Count(const Raster& image,RGB color) {
  size_t count=0;for(size_t at=0;at<image.pixels.size();at+=4)
    count+=image.pixels[at]==color[2]&&image.pixels[at+1]==color[1]&&image.pixels[at+2]==color[0];
  return count;
}
size_t Count(const Frames& images,RGB color) {
  size_t count=0;for(const auto& image:images)count+=Count(image,color);return count;
}
void Pixel(const Frames& images,int page,int x,int y,RGB color) {
  ASSERT_GE(page,0);ASSERT_LT(static_cast<size_t>(page),images.size());const auto& image=images[page];
  ASSERT_GT(image.width,x);ASSERT_GT(image.height,y);
  const auto at=(static_cast<size_t>(y)*image.width+x)*4;
  EXPECT_EQ(image.pixels[at],color[2])<<page<<":"<<x<<","<<y;
  EXPECT_EQ(image.pixels[at+1],color[1])<<page<<":"<<x<<","<<y;
  EXPECT_EQ(image.pixels[at+2],color[0])<<page<<":"<<x<<","<<y;
}
void Compare(const Frames& a,const Frames& b,Artifacts& artifacts,const std::string& label) {
  ASSERT_EQ(a.size(),b.size());
  for(size_t page=0;page<a.size();++page)CompareRaster(a[page],b[page],artifacts,label+"-"+std::to_string(page));
}
using Inspector=std::function<void(const Frames&)>;
void Scenario(const std::string& label,const std::string& body,const std::string& pages,const Inspector& inspect) {
  Artifacts artifacts;const auto bytes=Fixture(body,pages);artifacts.Pdf(label+"-input",bytes);
  std::array<Frames,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Loaded loaded(*writer,bytes);auto& s=loaded.s;
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    auto images=Render(s);ASSERT_NO_FATAL_FAILURE(inspect(images));
    for(size_t page=0;page<images.size();++page)artifacts.Image(writer->name()+"-"+label+"-"+std::to_string(page),images[page]);
    Compare(images,Render(s),artifacts,writer->name()+"-repeat-"+label);
    originals[backend++]=images;
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-"+label);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(writer->name()+" -> "+reader->name());Loaded reopened(*reader,saved);
      ASSERT_TRUE(reopened.s.valid());ASSERT_TRUE(reopened.s.xfa_loaded);
      const auto after=Render(reopened.s);ASSERT_NO_FATAL_FAILURE(inspect(after));
      Compare(images,after,artifacts,writer->name()+"-"+reader->name()+"-"+label);
      EXPECT_TRUE(reopened.s.host->callbacks_valid);EXPECT_FALSE(reopened.s.host->callback_error);
    }
  }
  Compare(originals[0],originals[1],artifacts,"backends-"+label);
}

TEST(CoverageXfaDeepPagination, LegacyBeforeAndAfterBreaksSelectNamedContentAndPageAreas) {
  // ExecuteBreak's legacy E::Break branch uses Before/AfterTarget, rather than
  // the BreakBefore/After node's Target attribute tested by the earlier batch.
  for(bool before:{false,true})for(bool page_break:{false,true}) {
    SCOPED_TRACE(before);
    SCOPED_TRACE(page_break);
    const std::string kind=before?"before":"after";
    const auto breaker="<break "+kind+"=\""+(page_break?"pageArea":"contentArea")+"\" "+kind+
      "Target=\""+(page_break?"#page":"#right")+"\" startNew=\"1\"/>";
    const auto body=Cell("first",red,20,before?"":breaker)+Cell("second",green,20,before?breaker:"")+Cell("last",blue,20);
    Scenario("legacy-"+kind+(page_break?"-page":"-area"),body,
      Pageset("<contentArea name=\"left\" id=\"left\" w=\"100pt\" h=\"100pt\"/>"
              "<contentArea name=\"right\" id=\"right\" x=\"120pt\" w=\"100pt\" h=\"100pt\"/>"),
      [=](const Frames& images){
        ASSERT_EQ(images.size(),page_break?2u:1u);
        Pixel(images,0,5,5,red);Pixel(images,page_break?1:0,page_break?5:125,5,green);
        Pixel(images,page_break?1:0,page_break?5:125,25,blue);
        for(RGB c:{red,green,blue})EXPECT_EQ(Count(images,c),1200u);
      });
  }
}

TEST(CoverageXfaDeepPagination, SomBreakTargetsNavigateNestedOrderedPageSetsWithoutDuplicatingContent) {
  // ResolveBreakTarget's runtime SOM branch and recursive ordered page-set
  // search are reached without #id resolution or native's broken alternatives
  // splitter. Every expression below resolves as one complete expression.
  for(const std::string target:{"tail.last","som(tail.last)","  som(tail.last)  "}) {
    SCOPED_TRACE(target);
    const auto first=Area("first","<contentArea w=\"100pt\" h=\"100pt\"/>"+
      Block("firstMaster",{128,0,128},10,10,"x=\"220pt\" y=\"220pt\""),"<occur min=\"1\" max=\"1\"/>");
    const auto last=Area("last","<contentArea w=\"100pt\" h=\"100pt\"/>"+
      Block("lastMaster",orange,10,10,"x=\"220pt\" y=\"220pt\""),"<occur min=\"0\" max=\"1\"/>");
    const auto pages="<pageSet name=\"pages\" relation=\"orderedOccurrence\"><occur min=\"1\" max=\"1\"/>"+first+
      "<pageSet name=\"tail\" relation=\"orderedOccurrence\"><occur min=\"0\" max=\"1\"/>"+last+"</pageSet></pageSet>";
    Scenario("som-target-"+target,Cell("prefix",red,20)+Cell("destination",green,20,
      "<breakBefore targetType=\"pageArea\" target=\""+target+"\" startNew=\"1\"/>"),pages,
      [](const Frames& images){ASSERT_EQ(images.size(),2u);Pixel(images,0,5,5,red);Pixel(images,1,5,5,green);
        Pixel(images,0,225,225,{128,0,128});Pixel(images,1,225,225,orange);
        EXPECT_EQ(Count(images,red),1200u);EXPECT_EQ(Count(images,green),1200u);});
  }
}

TEST(CoverageXfaDeepPagination, PreviousKeepChainsMoveAllMembersToTheNextTallerContentArea) {
  for(int length:{2,3,4})for(bool previous_only:{false,true}) {
    SCOPED_TRACE(length);
    SCOPED_TRACE(previous_only);
    const std::array<RGB,4> colors={red,green,blue,orange};
    std::string body=Cell("prefix",{128,0,128},60);
    for(int i=0;i<length;++i)body+=Cell("kept"+std::to_string(i),colors[i],20,
      "<keep intact=\"contentArea\" "+std::string(i>0?"previous=\"contentArea\" ":"")+
      (i+1<length&&!previous_only?"next=\"contentArea\"":"")+"/>");
    Scenario("previous-chain-"+std::to_string(length)+std::to_string(previous_only),body,
      Pageset("<contentArea w=\"100pt\" h=\"80pt\"/><contentArea x=\"120pt\" w=\"100pt\" h=\"120pt\"/>"),
      [=](const Frames& images){ASSERT_EQ(images.size(),1u);Pixel(images,0,5,5,{128,0,128});Pixel(images,0,5,65,{255,255,255});
        for(int i=0;i<length;++i){Pixel(images,0,125,5+20*i,colors[i]);EXPECT_EQ(Count(images,colors[i]),1200u);}
        EXPECT_EQ(Count(images,RGB{128,0,128}),3600u);});
  }
}

TEST(CoverageXfaDeepPagination, IntactContainersSkipTooShortAreasAndUseTheFirstAreaThatFits) {
  for(int height:{60,80,100}) {
    SCOPED_TRACE(height);
    const auto pages=Pageset("<contentArea name=\"short\" w=\"70pt\" h=\"40pt\"/>"
      "<contentArea name=\"tall\" x=\"90pt\" w=\"70pt\" h=\"120pt\"/>");
    Scenario("intact-skip-"+std::to_string(height),Cell("atomic",red,height,"<keep intact=\"contentArea\"/>"),pages,
      [=](const Frames& images){ASSERT_EQ(images.size(),1u);Pixel(images,0,5,5,{255,255,255});
        Pixel(images,0,95,5,red);Pixel(images,0,95,height-5,red);Pixel(images,0,95,height+2,{255,255,255});
        EXPECT_EQ(Count(images,red),static_cast<size_t>(60*height));});
  }
}

TEST(CoverageXfaDeepPagination, RecursiveFlowSplitsPreserveColoredChildrenWithTopAndBottomInsets) {
  for(int top:{0,4})for(int bottom:{0,6}) {
    SCOPED_TRACE(top);
    SCOPED_TRACE(bottom);
    const std::array<RGB,6> colors={red,green,blue,orange,RGB{0,128,128},RGB{128,0,128}};
    std::string body="<subform name=\"outer\" layout=\"tb\" w=\"80pt\"><keep intact=\"none\"/>"
      "<margin topInset=\""+std::to_string(top)+"pt\" bottomInset=\""+std::to_string(bottom)+"pt\"/>"
      "<subform name=\"inner\" layout=\"tb\" w=\"70pt\"><keep intact=\"none\"/>"
      "<margin topInset=\"3pt\" bottomInset=\"5pt\"/>";
    for(int i=0;i<6;++i)body+=Cell("child"+std::to_string(i),colors[i],30,"<keep intact=\"contentArea\"/>");
    body+="</subform></subform>";
    Scenario("recursive-insets-"+std::to_string(top)+std::to_string(bottom),body,Pageset(),
      [=](const Frames& images){ASSERT_GE(images.size(),2u);ASSERT_LE(images.size(),4u);
        for(RGB c:colors)EXPECT_EQ(Count(images,c),1800u);
        Pixel(images,0,5,top+4,red);for(size_t page=0;page<images.size();++page)Pixel(images,int(page),100,50,{255,255,255});});
  }
}

std::string Decorations(int maximum) {
  const auto decoration=[&](const std::string& name,RGB color){
    return "<subform name=\""+name+"\" id=\""+name+"\" layout=\"position\" w=\"60pt\" h=\"10pt\">"
      "<occur min=\"0\" max=\""+std::to_string(maximum)+"\"/>"+Block("paint",color,60,10)+"</subform>";
  };
  return "<proto>"+decoration("head",red)+decoration("tail",blue)+"</proto>";
}

void ZeroOccurrenceOverflow(const std::string& label,const std::string& body) {
  Artifacts artifacts;const auto bytes=Fixture(body,Pageset());artifacts.Pdf(label+"-input",bytes);
  // With no decorations the correct result is three fragments of90/90/60pt.
  // An explicit page-break control constructs those fragments independently,
  // avoiding native's parentless continuation-tree defect. Compare its entire
  // paint against every Rust reader of the original, unmodified template.
  const auto explicit_break="<breakBefore targetType=\"pageArea\" startNew=\"1\"/>";
  const auto control_bytes=Fixture(Cell("first",green,90)+Cell("second",green,90,explicit_break)+
    Cell("third",green,60,explicit_break),Pageset());
  Frames control;
  for(const auto* library:Implementations()) {
    Loaded loaded(*library,control_bytes);ASSERT_TRUE(loaded.s.valid());ASSERT_TRUE(loaded.s.xfa_loaded);
    const auto image=Render(loaded.s);ASSERT_EQ(image.size(),3u);EXPECT_EQ(Count(image,green),14400u);
    if(control.empty())control=image;else Compare(control,image,artifacts,label+"-explicit-control");
  }
  const auto inspect=[&](const Frames& images,bool native){
    ASSERT_EQ(images.size(),3u);EXPECT_EQ(Count(images,green),native?12600u:14400u);
    EXPECT_EQ(Count(images,red),0u);EXPECT_EQ(Count(images,blue),0u);
    Pixel(images,0,5,65,native?RGB{255,255,255}:green);
    Pixel(images,1,5,85,green);Pixel(images,2,5,55,green);
    if(!native)Compare(control,images,artifacts,label+"-full-correct-control");
  };
  const auto compare=[&](const Frames& a,const Frames& b,bool cross,const std::string& name){
    ASSERT_EQ(a.size(),3u);ASSERT_EQ(b.size(),3u);
    if(!cross){Compare(a,b,artifacts,name);return;}
    // Native loses just the first page's final30pt band. Both original paints
    // and the exact independent counts above remain visible; every unaffected
    // pixel still compares exactly. No raster tolerance is widened.
    for(const auto& rect:std::array<std::array<int,4>,3>{{{0,0,240,60},{60,60,180,30},{0,90,240,150}}})
      CompareRaster(CropRaster(a[0],rect[0],rect[1],rect[2],rect[3]),
                    CropRaster(b[0],rect[0],rect[1],rect[2],rect[3]),artifacts,name+"-first-"+std::to_string(rect[1]));
    for(int page:{1,2})CompareRaster(a[page],b[page],artifacts,name+"-page"+std::to_string(page));
  };
  std::array<Frames,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Loaded loaded(*writer,bytes);ASSERT_TRUE(loaded.s.valid());ASSERT_TRUE(loaded.s.xfa_loaded);
    const auto original=Render(loaded.s);ASSERT_NO_FATAL_FAILURE(inspect(original,writer->name()=="native"));
    for(size_t page=0;page<original.size();++page)artifacts.Image(writer->name()+"-"+label+"-original-"+std::to_string(page),original[page]);
    Compare(original,Render(loaded.s),artifacts,writer->name()+"-repeat-"+label);originals[backend++]=original;
    const auto saved=SaveDocument(loaded.s.api,loaded.s.document,artifacts,writer->name()+"-"+label);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Loaded reopened(*reader,saved);ASSERT_TRUE(reopened.s.valid());ASSERT_TRUE(reopened.s.xfa_loaded);
      const auto after=Render(reopened.s);ASSERT_NO_FATAL_FAILURE(inspect(after,reader->name()=="native"));
      compare(original,after,writer->name()!=reader->name(),writer->name()+"-"+reader->name()+"-"+label);
    }
  }
  compare(originals[0],originals[1],true,label+"-original-backends");
}

TEST(CoverageXfaDeepPagination, GeneratedOverflowDecorationsRespectBoundedOccurrencesAndPreservePayload) {
  for(bool legacy:{false,true})for(int maximum:{0,1,2,-1}) {
    SCOPED_TRACE(legacy);
    SCOPED_TRACE(maximum);
    auto body=Decorations(maximum)+"<subform name=\"stream\" layout=\"tb\" w=\"60pt\">"+
      (legacy?"<break overflowLeader=\"#head\" overflowTrailer=\"#tail\"/>":"<overflow leader=\"#head\" trailer=\"#tail\"/>");
    for(int i=0;i<4;++i)body+="<subform name=\"row"+std::to_string(i)+"\" layout=\"position\" w=\"60pt\" h=\"60pt\">"
      "<keep intact=\"none\"/>"+Block("top",green,60,30)+Block("bottom",green,60,30,"y=\"30pt\"")+"</subform>";
    body+="</subform>";
    if(maximum==0){ZeroOccurrenceOverflow("overflow-zero-"+std::to_string(legacy),body);continue;}
    Scenario("overflow-cap-"+std::to_string(legacy)+"-"+std::to_string(maximum),body,Pageset(),
      [=](const Frames& images){ASSERT_GE(images.size(),3u);ASSERT_LE(images.size(),8u);EXPECT_EQ(Count(images,green),14400u);
        const auto heads=Count(images,red),tails=Count(images,blue);
        EXPECT_EQ(heads%600,0u);EXPECT_EQ(tails%600,0u);
        EXPECT_GE(heads,600u);EXPECT_GE(tails,600u);
        if(maximum>0){EXPECT_LE(heads,static_cast<size_t>(maximum*600));EXPECT_LE(tails,static_cast<size_t>(maximum*600));}
        for(size_t page=0;page<images.size();++page)Pixel(images,int(page),100,50,{255,255,255});});
  }
}

TEST(CoverageXfaDeepPagination, LegacyBookendAttributesProduceOrderedLeadersAndTrailersForEverySection) {
  for(int sections:{1,2,3}) {
    SCOPED_TRACE(sections);auto body=Decorations(-1);
    for(int i=0;i<sections;++i)body+="<subform name=\"section"+std::to_string(i)+"\" layout=\"tb\" w=\"60pt\">"
      "<break bookendLeader=\"#head\" bookendTrailer=\"#tail\"/>"+Block("payload",green,60,20)+"</subform>";
    Scenario("legacy-bookends-"+std::to_string(sections),body,Pageset("<contentArea w=\"180pt\" h=\"200pt\"/>"),
      [=](const Frames& images){ASSERT_EQ(images.size(),1u);
        for(int i=0;i<sections;++i){Pixel(images,0,5,i*40+5,red);Pixel(images,0,5,i*40+15,green);Pixel(images,0,5,i*40+35,blue);}
        EXPECT_EQ(Count(images,red),static_cast<size_t>(sections*600));EXPECT_EQ(Count(images,green),static_cast<size_t>(sections*1200));
        EXPECT_EQ(Count(images,blue),static_cast<size_t>(sections*600));});
  }
}

TEST(CoverageXfaDeepPagination, NamedBreakTargetsDistinguishCurrentPagesFromNewContentOccurrences) {
  for(bool page_target:{false,true})for(bool start_new:{false,true}) {
    SCOPED_TRACE(page_target);
    SCOPED_TRACE(start_new);
    // ShouldGetNextPageArea compares the target with the current pageArea.
    // A contentArea target therefore selects a new occurrence even when
    // startNew is false; only the current pageArea can keep its existing one.
    const bool new_occurrence=start_new||!page_target;
    const auto body=Cell("first",red,20)+Cell("second",green,20,
      "<breakBefore targetType=\""+std::string(page_target?"pageArea":"contentArea")+"\" target=\""+
      (page_target?"#page":"#body")+"\" startNew=\""+std::to_string(start_new)+"\"/>");
    Scenario("same-target-"+std::to_string(page_target)+std::to_string(start_new),body,Pageset("<contentArea name=\"body\" id=\"body\" w=\"100pt\" h=\"100pt\"/>"),
      [=](const Frames& images){ASSERT_EQ(images.size(),new_occurrence?2u:1u);Pixel(images,0,5,5,red);
        Pixel(images,new_occurrence?1:0,5,new_occurrence?5:25,green);EXPECT_EQ(Count(images,red),1200u);EXPECT_EQ(Count(images,green),1200u);});
  }
}
}  // namespace
}  // namespace pdfoxium::integration
