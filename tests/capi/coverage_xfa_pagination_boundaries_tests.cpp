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
const RGB red{255,0,0},green{0,128,0},blue{0,0,255},orange{255,128,0},yellow{255,255,0},cyan{0,255,255},magenta{255,0,255};

std::string Block(const std::string& name,RGB color,int width,int height,const std::string& attrs="") {
  return "<draw name=\""+name+"\" w=\""+std::to_string(width)+"pt\" h=\""+std::to_string(height)+"pt\" "+attrs+
    "><value><rectangle><fill><color value=\""+std::to_string(color[0])+","+std::to_string(color[1])+","+std::to_string(color[2])+
    "\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
}
std::string Cell(const std::string& name,RGB color,int height,const std::string& properties="") {
  return "<subform name=\""+name+"\" layout=\"position\" w=\"60pt\" h=\""+std::to_string(height)+"pt\">"+
    properties+Block("paint",color,60,height)+"</subform>";
}
std::string Area(const std::string& name,const std::string& content,const std::string& occur="<occur min=\"1\" max=\"-1\"/>",const std::string& attrs="") {
  return "<pageArea name=\""+name+"\" id=\""+name+"\" "+attrs+">"+occur+"<medium short=\"240pt\" long=\"240pt\"/>"+content+"</pageArea>";
}
std::string Pageset(const std::string& areas="<contentArea w=\"180pt\" h=\"100pt\"/>") {
  return "<pageSet name=\"pages\">"+Area("page",areas)+"</pageSet>";
}
std::vector<uint8_t> Fixture(const std::string& body,const std::string& pages,const std::string& version) {
  // XFA template/page-set constructors are not public C APIs. All subsequent
  // merge, pagination, painting and serialization are engine operations.
  const std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/"+version+"/\">"
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
void Scenario(const std::string& label,const std::string& body,const std::string& pages,const Inspector& inspect,const std::string& version="3.3") {
  Artifacts artifacts;const auto bytes=Fixture(body,pages,version);artifacts.Pdf(label+"-input",bytes);
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
std::string Master(RGB color) {return Block("master",color,10,10,"x=\"220pt\" y=\"220pt\"");}
std::string Occur(int minimum,int maximum){return "<occur min=\""+std::to_string(minimum)+"\" max=\""+std::to_string(maximum)+"\"/>";}
std::string Decoration(const std::string& name,RGB color,int width,int height,const std::string& align="left",int maximum=-1){
  return "<subform name=\""+name+"\" id=\""+name+"\" layout=\"position\" w=\""+std::to_string(width)+"pt\" h=\""+
    std::to_string(height)+"pt\" hAlign=\""+align+"\">"+Occur(0,maximum)+Block("paint",color,width,height)+"</subform>";
}
std::string SplitRows(int count=4){
  std::string body;
  for(int i=0;i<count;++i)body+="<subform name=\"row"+std::to_string(i)+"\" layout=\"position\" w=\"60pt\" h=\"60pt\">"
    "<keep intact=\"none\"/>"+Block("upper",green,60,30)+Block("lower",green,60,30,"y=\"30pt\"")+"</subform>";
  return body;
}

TEST(CoverageXfaPaginationBoundaries, InitialBeforeBreakSelectsAnOptionalMasterBeforeBodyLayoutStarts) {
  for(bool legacy:{false,true})for(bool hidden_prefix:{false,true}){
    SCOPED_TRACE(legacy);
    SCOPED_TRACE(hidden_prefix);
    const auto pages="<pageSet relation=\"orderedOccurrence\">"+
      Area("unused","<contentArea w=\"100pt\" h=\"100pt\"/>"+Master(yellow),Occur(0,1))+
      Area("destination","<contentArea w=\"100pt\" h=\"100pt\"/>"+Master(cyan),Occur(0,1))+"</pageSet>";
    const std::string breaker=legacy?"<break before=\"pageArea\" beforeTarget=\"#destination\" startNew=\"0\"/>":
      "<breakBefore targetType=\"pageArea\" target=\"#destination\" startNew=\"0\"/>";
    const auto body=std::string(hidden_prefix?"<subform name=\"skip\" presence=\"hidden\" layout=\"position\" w=\"60pt\" h=\"20pt\"/>":"")+
      Cell("firstVisible",green,20,breaker);
    Scenario("initial-target-"+std::to_string(legacy)+std::to_string(hidden_prefix),body,pages,[](const Frames& images){
      ASSERT_EQ(images.size(),1u);Pixel(images,0,5,5,green);Pixel(images,0,225,225,cyan);
      EXPECT_EQ(Count(images,green),1200u);EXPECT_EQ(Count(images,yellow),0u);EXPECT_EQ(Count(images,cyan),100u);
    });
  }
}

TEST(CoverageXfaPaginationBoundaries, ExplicitMidstreamBlankMasterTargetsPreserveTheStationeryPageAndFollowingBody) {
  for(bool before:{false,true}){
    SCOPED_TRACE(before);
    const auto pages="<pageSet relation=\"orderedOccurrence\">"+
      Area("first","<contentArea w=\"100pt\" h=\"100pt\"/>"+Master(cyan),Occur(1,1))+
      Area("stationery",Master(yellow),Occur(1,1))+
      Area("last","<contentArea w=\"100pt\" h=\"100pt\"/>"+Master(magenta),Occur(1,1))+"</pageSet>";
    const std::string kind=before?"breakBefore":"breakAfter";
    const auto breaker="<"+kind+" targetType=\"pageArea\" target=\"#stationery\" startNew=\"1\"/>";
    const auto body=Cell("prefix",red,20,before?"":breaker)+Cell("following",green,20,before?breaker:"");
    Scenario("blank-target-"+std::to_string(before),body,pages,[](const Frames& images){
      ASSERT_EQ(images.size(),3u);Pixel(images,0,5,5,red);Pixel(images,1,5,5,{255,255,255});Pixel(images,2,5,5,green);
      Pixel(images,0,225,225,cyan);Pixel(images,1,225,225,yellow);Pixel(images,2,225,225,magenta);
      EXPECT_EQ(Count(images,red),1200u);EXPECT_EQ(Count(images,green),1200u);EXPECT_EQ(Count(images,yellow),100u);
    });
  }
}

TEST(CoverageXfaPaginationBoundaries, OverflowTargetsSelectNamedContentOrNewPageOccurrencesWithoutLosingPayload) {
  for(bool legacy:{false,true})for(bool page_target:{false,true}){
    SCOPED_TRACE(legacy);
    SCOPED_TRACE(page_target);
    const auto proto="<proto>"+Decoration("head",magenta,60,10)+Decoration("tail",blue,60,10)+"</proto>";
    const std::string target=page_target?"#page":"#right";
    const auto overflow=legacy?"<break overflowLeader=\"#head\" overflowTrailer=\"#tail\" overflowTarget=\""+target+"\"/>":
      "<overflow leader=\"#head\" trailer=\"#tail\" target=\""+target+"\"/>";
    const auto body=proto+"<subform name=\"stream\" layout=\"tb\" w=\"60pt\">"+overflow+SplitRows()+"</subform>";
    Scenario("overflow-target-"+std::to_string(legacy)+std::to_string(page_target),body,
      Pageset("<contentArea name=\"left\" w=\"100pt\" h=\"100pt\"/><contentArea name=\"right\" id=\"right\" x=\"120pt\" w=\"100pt\" h=\"100pt\"/>"),
      [=](const Frames& images){ASSERT_GE(images.size(),page_target?3u:2u);ASSERT_LE(images.size(),6u);
        EXPECT_EQ(Count(images,green),14400u);EXPECT_GT(Count(images,magenta),0u);EXPECT_GT(Count(images,blue),0u);
        Pixel(images,0,5,5,green);
        if(page_target){Pixel(images,1,5,5,magenta);Pixel(images,0,125,5,{255,255,255});}
        else Pixel(images,0,125,5,magenta);
        for(size_t page=0;page<images.size();++page)Pixel(images,int(page),230,100,{255,255,255});
      });
  }
}

TEST(CoverageXfaPaginationBoundaries, FittingOverflowContainersDiscardTheirUnusedGeneratedDecorations) {
  for(bool legacy:{false,true})for(int height:{20,40,60}){
    SCOPED_TRACE(legacy);
    SCOPED_TRACE(height);
    const auto proto="<proto>"+Decoration("head",magenta,60,10)+Decoration("tail",blue,60,20)+"</proto>";
    const auto overflow=legacy?"<break overflowLeader=\"#head\" overflowTrailer=\"#tail\"/>":"<overflow leader=\"#head\" trailer=\"#tail\"/>";
    const auto body=proto+"<subform name=\"fits\" layout=\"tb\" w=\"60pt\">"+overflow+Cell("payload",green,height)+"</subform>";
    Scenario("unused-overflow-"+std::to_string(legacy)+std::to_string(height),body,Pageset(),[=](const Frames& images){
      ASSERT_EQ(images.size(),1u);EXPECT_EQ(Count(images,green),static_cast<size_t>(60*height));
      EXPECT_EQ(Count(images,magenta),0u);EXPECT_EQ(Count(images,blue),0u);
      Pixel(images,0,5,5,green);Pixel(images,0,5,height+2,{255,255,255});
    });
  }
}

TEST(CoverageXfaPaginationBoundaries, InlineOverflowTrailersUseTheRemainingRowWidthWhileLeadersKeepTheirAlignment) {
  for(const std::string align:{"left","center","right"}){
    SCOPED_TRACE(align);
    const auto proto="<proto>"+Decoration("head",magenta,20,10,align)+Decoration("tail",blue,20,10)+"</proto>";
    auto body=proto+"<subform name=\"stream\" layout=\"lr-tb\" w=\"100pt\"><keep intact=\"none\"/>"
      "<overflow leader=\"#head\" trailer=\"#tail\"/>"
      "<subform name=\"split\" layout=\"position\" w=\"60pt\" h=\"180pt\"><keep intact=\"none\"/>";
    // One genuinely splittable positioned child inherits the parent's overflow.
    // Atomic rows merely moved to another area discard unused decorations.
    for(int i=0;i<6;++i)body+=Block("row"+std::to_string(i),green,60,30,"y=\""+std::to_string(30*i)+"pt\"");
    body+="</subform></subform>";
    Artifacts artifacts;const auto label="inline-trailer-"+align;
    const auto bytes=Fixture(body,Pageset(),"3.3");artifacts.Pdf(label+"-input",bytes);
    const int head_x=align=="right"?80:align=="center"?40:0;
    const int green_x=align=="left"?20:0,green_y=align=="left"?0:10;
    // Native's parentless split nests its continuation into the first subtree:
    // page 0 becomes blank and page 1 retains only the first 90pt plus trailer.
    // Two explicit positioned fragments provide the complete independent paint
    // control without calling that split path in either implementation.
    const auto explicit_body="<subform name=\"first\" layout=\"position\" w=\"100pt\" h=\"90pt\">"+
      Block("body",green,60,90)+Block("trailer",blue,20,10,"x=\"60pt\"")+"</subform>"
      "<subform name=\"second\" layout=\"position\" w=\"100pt\" h=\"100pt\">"
      "<breakBefore targetType=\"pageArea\" startNew=\"1\"/>"+
      Block("leader",magenta,20,10,"x=\""+std::to_string(head_x)+"pt\"")+
      Block("body",green,60,90,"x=\""+std::to_string(green_x)+"pt\" y=\""+std::to_string(green_y)+"pt\"")+"</subform>";
    const auto control_bytes=Fixture(explicit_body,Pageset(),"3.3");
    artifacts.Pdf(label+"-explicit-control",control_bytes);Frames control;
    for(const auto* library:Implementations()){
      Loaded loaded(*library,control_bytes);ASSERT_TRUE(loaded.s.valid());ASSERT_TRUE(loaded.s.xfa_loaded);
      const auto images=Render(loaded.s);ASSERT_EQ(images.size(),2u);EXPECT_EQ(Count(images,green),10800u);
      EXPECT_EQ(Count(images,blue),200u);EXPECT_EQ(Count(images,magenta),200u);
      Pixel(images,0,65,5,blue);Pixel(images,1,head_x+5,5,magenta);Pixel(images,1,25,15,green);
      if(control.empty())control=images;else Compare(control,images,artifacts,label+"-explicit-control");
    }
    const auto inspect=[&](const Frames& images,const Library& owner){
      ASSERT_EQ(images.size(),2u);EXPECT_EQ(Count(images,blue),200u);
      if(owner.name()=="native"){
        EXPECT_EQ(Count(images,green),5400u);EXPECT_EQ(Count(images,magenta),0u);
        const Raster blank{240,240,std::vector<uint8_t>(240*240*4,255)};
        CompareRaster(blank,images[0],artifacts,label+"-native-blank-first");
        CompareRaster(control[0],images[1],artifacts,label+"-native-retained-fragment");
      }else{
        EXPECT_EQ(Count(images,green),10800u);EXPECT_EQ(Count(images,magenta),200u);
        Pixel(images,0,65,5,blue);Pixel(images,1,head_x+5,5,magenta);Pixel(images,1,25,15,green);
        Compare(control,images,artifacts,label+"-rust-complete-control");
      }
    };
    std::array<Frames,2> originals;std::array<std::vector<uint8_t>,2> saved;size_t backend=0;
    for(const auto* writer:Implementations()){
      SCOPED_TRACE(writer->name());Loaded loaded(*writer,bytes);ASSERT_TRUE(loaded.s.valid());ASSERT_TRUE(loaded.s.xfa_loaded);
      originals[backend]=Render(loaded.s);ASSERT_NO_FATAL_FAILURE(inspect(originals[backend],*writer));
      for(size_t page=0;page<originals[backend].size();++page)artifacts.Image(writer->name()+"-"+label+"-original-"+std::to_string(page),originals[backend][page]);
      Compare(originals[backend],Render(loaded.s),artifacts,writer->name()+"-"+label+"-repeat");
      saved[backend]=SaveDocument(loaded.s.api,loaded.s.document,artifacts,writer->name()+"-"+label);ASSERT_FALSE(saved[backend].empty());++backend;
    }
    CompareRaster(originals[0][1],originals[1][0],artifacts,label+"-common-retained-fragment");
    for(size_t writer=0;writer<2;++writer)for(size_t reader=0;reader<2;++reader){
      SCOPED_TRACE(std::to_string(writer)+" -> "+std::to_string(reader));Loaded loaded(*Implementations()[reader],saved[writer]);
      ASSERT_TRUE(loaded.s.valid());ASSERT_TRUE(loaded.s.xfa_loaded);const auto images=Render(loaded.s);
      ASSERT_NO_FATAL_FAILURE(inspect(images,*Implementations()[reader]));
      Compare(originals[reader],images,artifacts,label+"-reader-original-"+std::to_string(writer)+std::to_string(reader));
    }
  }
}

TEST(CoverageXfaPaginationBoundaries, MultipleBookendInstructionsForceOneOrderedCopyRegardlessOfTemplateRepeatMaximum) {
  for(const std::string align:{"left","center","right"})for(int maximum:{0,1}){
    SCOPED_TRACE(align);
    SCOPED_TRACE(maximum);
    const auto proto="<proto>"+Decoration("head1",red,20,5,align,1)+Decoration("tail1",blue,20,6,align,1)+
      Decoration("head2",orange,30,7,align,maximum)+Decoration("tail2",cyan,30,8,align,maximum)+"</proto>";
    const auto body=proto+"<subform name=\"section\" layout=\"tb\" w=\"100pt\">"
      "<bookend leader=\"#head1\" trailer=\"#tail1\"/><bookend leader=\"#head2\" trailer=\"#tail2\"/>"+
      Block("payload",green,60,20)+"</subform>";
    Scenario("multiple-bookends-"+align+std::to_string(maximum),body,Pageset(),[=](const Frames& images){
      // ProcessBookend requests one explicit copy and never applies the
      // overflow occurrence gate. InsertFlowedItem reads alignment from the
      // current Bookend instruction (default left), not the prototype subform.
      ASSERT_EQ(images.size(),1u);Pixel(images,0,3,2,red);Pixel(images,0,3,8,orange);
      Pixel(images,0,5,17,green);Pixel(images,0,3,34,blue);Pixel(images,0,3,41,cyan);
      EXPECT_EQ(Count(images,red),100u);EXPECT_EQ(Count(images,green),1200u);EXPECT_EQ(Count(images,blue),120u);
      EXPECT_EQ(Count(images,orange),210u);EXPECT_EQ(Count(images,cyan),240u);
    });
  }
}

TEST(CoverageXfaPaginationBoundaries, NonPagingBreakAfterUsesInstructionAlignmentAndRetainsTrailerLeaderOrder) {
  for(const std::string align:{"left","center","right"}){
    SCOPED_TRACE(align);
    const auto proto="<proto>"+Decoration("head",magenta,20,10,align)+Decoration("tail",blue,20,10)+"</proto>";
    const auto body=proto+"<subform name=\"section\" layout=\"tb\" w=\"100pt\">"+
      Cell("first",green,20,"<breakAfter targetType=\"auto\" leader=\"#head\" trailer=\"#tail\"/>")+
      Cell("following",orange,20)+"</subform>";
    Scenario("nonpaging-after-"+align,body,Pageset(),[=](const Frames& images){
      // The current BreakAfter instruction supplies alignment here, unlike
      // an overflow's pending leader placed by AddLeaderAfterSplit.
      ASSERT_EQ(images.size(),1u);Pixel(images,0,5,5,green);Pixel(images,0,5,25,blue);
      Pixel(images,0,5,35,magenta);Pixel(images,0,5,45,orange);
      EXPECT_EQ(Count(images,green),1200u);EXPECT_EQ(Count(images,orange),1200u);
      EXPECT_EQ(Count(images,blue),200u);EXPECT_EQ(Count(images,magenta),200u);
    });
  }
}

TEST(CoverageXfaPaginationBoundaries, ExhaustedNestedOrderedOccurrencesAdvanceToTheNextChapterAndKeepAllAtomicRows) {
  for(int first_count:{1,2}){
    SCOPED_TRACE(first_count);
    const auto first=Area("chapterOne","<contentArea w=\"100pt\" h=\"100pt\"/>"+Master(yellow),Occur(1,1));
    const auto second=Area("chapterTwo","<contentArea w=\"100pt\" h=\"100pt\"/>"+Master(cyan),Occur(1,1));
    const auto pages="<pageSet relation=\"orderedOccurrence\">"+Occur(1,1)+
      "<pageSet name=\"firstChapter\" relation=\"orderedOccurrence\">"+Occur(1,first_count)+first+"</pageSet>"
      "<pageSet name=\"secondChapter\" relation=\"orderedOccurrence\">"+Occur(1,1)+second+"</pageSet></pageSet>";
    const std::array<RGB,3> colors{red,green,blue};std::string body;
    for(int i=0;i<=first_count;++i)body+=Cell("record"+std::to_string(i),colors[i],70);
    Scenario("ordered-exhaustion-"+std::to_string(first_count),body,pages,[=](const Frames& images){
      ASSERT_EQ(images.size(),static_cast<size_t>(first_count+1));
      for(int i=0;i<=first_count;++i){Pixel(images,i,5,5,colors[i]);Pixel(images,i,225,225,i<first_count?yellow:cyan);
        EXPECT_EQ(Count(images,colors[i]),4200u);}
      EXPECT_EQ(Count(images,yellow),static_cast<size_t>(first_count*100));EXPECT_EQ(Count(images,cyan),100u);
    });
  }
}

TEST(CoverageXfaPaginationBoundaries, ExplicitDuplexTargetsInsertOnlyTheParityPageNeededBeforeTheDestination) {
  for(bool odd:{false,true}){
    SCOPED_TRACE(odd);
    const auto pages="<pageSet relation=\"duplexPaginated\">"+
      Area("first","<contentArea w=\"100pt\" h=\"100pt\"/>"+Master(yellow),Occur(1,-1),"pagePosition=\"first\" oddOrEven=\"odd\"")+
      Area("even","<contentArea w=\"100pt\" h=\"100pt\"/>"+Master(cyan),Occur(1,-1),"pagePosition=\"rest\" oddOrEven=\"even\"")+
      Area("odd","<contentArea w=\"100pt\" h=\"100pt\"/>"+Master(magenta),Occur(1,-1),"pagePosition=\"rest\" oddOrEven=\"odd\"")+"</pageSet>";
    const auto body=Cell("prefix",red,20)+Cell("target",green,20,
      "<breakBefore targetType=\"pageArea\" target=\"#"+std::string(odd?"odd":"even")+"\" startNew=\"1\"/>");
    Scenario("duplex-target-"+std::to_string(odd),body,pages,[=](const Frames& images){
      ASSERT_EQ(images.size(),odd?3u:2u);Pixel(images,0,5,5,red);Pixel(images,odd?2:1,5,5,green);
      Pixel(images,0,225,225,yellow);Pixel(images,1,225,225,cyan);
      if(odd){Pixel(images,1,5,5,{255,255,255});Pixel(images,2,225,225,magenta);}
      EXPECT_EQ(Count(images,red),1200u);EXPECT_EQ(Count(images,green),1200u);
    });
  }
}

TEST(CoverageXfaPaginationBoundaries, TooShortSimplexLastMasterIsAppendedWithoutStealingTheCompletedBodyPage) {
  for(int height:{60,80}){
    SCOPED_TRACE(height);
    const auto pages="<pageSet relation=\"simplexPaginated\">"+
      Area("first","<contentArea w=\"100pt\" h=\"100pt\"/>"+Master(yellow),Occur(1,-1),"pagePosition=\"first\"")+
      Area("rest","<contentArea w=\"100pt\" h=\"100pt\"/>"+Master(orange),Occur(1,-1),"pagePosition=\"rest\"")+
      Area("last","<contentArea w=\"100pt\" h=\"40pt\"/>"+Master(cyan),Occur(1,-1),"pagePosition=\"last\"")+"</pageSet>";
    Scenario("small-last-"+std::to_string(height),Cell("first",red,height)+Cell("second",green,height),pages,[=](const Frames& images){
      ASSERT_EQ(images.size(),3u);Pixel(images,0,5,5,red);Pixel(images,1,5,5,green);Pixel(images,2,5,5,{255,255,255});
      Pixel(images,0,225,225,yellow);Pixel(images,1,225,225,orange);Pixel(images,2,225,225,cyan);
      EXPECT_EQ(Count(images,red),static_cast<size_t>(60*height));EXPECT_EQ(Count(images,green),static_cast<size_t>(60*height));
    });
  }
}

TEST(CoverageXfaPaginationBoundaries, TableKeepPairsGrowMultiChildCellsAndRemainTogetherAcrossLegacyRowIntactRules) {
  for(const std::string version:{"2.7","3.3"})for(const std::string keep:{"contentArea","pageArea"}){
    SCOPED_TRACE(version);
    SCOPED_TRACE(keep);
    const auto tall=[&](const std::string& name,RGB color){return "<subform name=\""+name+"\" layout=\"tb\" w=\"40pt\" h=\"20pt\">"+
      Block("top",color,40,40)+Block("bottom",color,40,40)+"</subform>";};
    const auto body=Block("prefix",magenta,40,80)+"<subform name=\"table\" layout=\"table\" w=\"80pt\" columnWidths=\"40pt 40pt\">"
      "<subform name=\"head\" layout=\"row\"><keep next=\""+keep+"\"/>"+Block("green",green,40,20)+Block("blue",blue,40,20)+"</subform>"
      "<subform name=\"following\" layout=\"row\"><keep intact=\"none\" previous=\""+keep+"\"/>"+tall("orange",orange)+tall("cyan",cyan)+"</subform></subform>";
    Scenario("table-kept-growth-"+version+keep,body,
      Pageset("<contentArea w=\"100pt\" h=\"100pt\"/><contentArea x=\"130pt\" w=\"100pt\" h=\"180pt\"/>"),
      [](const Frames& images){ASSERT_EQ(images.size(),1u);Pixel(images,0,5,5,magenta);Pixel(images,0,5,85,{255,255,255});
        Pixel(images,0,135,5,green);Pixel(images,0,175,5,blue);Pixel(images,0,135,25,orange);Pixel(images,0,175,95,cyan);
        EXPECT_EQ(Count(images,magenta),3200u);EXPECT_EQ(Count(images,green),800u);EXPECT_EQ(Count(images,blue),800u);
        EXPECT_EQ(Count(images,orange),3200u);EXPECT_EQ(Count(images,cyan),3200u);
      },version);
  }
}

TEST(CoverageXfaPaginationBoundaries, LegacyHiddenDrawExtentsSizeAnOtherwiseEmptyPositionedContainerOnlyBeforeTwoPointSeven) {
  for(const std::string version:{"2.6","2.7","3.3"}){
    SCOPED_TRACE(version);const bool legacy=version=="2.6";
    const auto body="<subform name=\"autosize\" layout=\"position\">"+
      Block("hiddenDraw",magenta,40,40,"presence=\"hidden\" x=\"10pt\" y=\"10pt\"")+"</subform>"+Cell("sentinel",green,20);
    Scenario("legacy-hidden-size-"+version,body,Pageset(),[=](const Frames& images){
      ASSERT_EQ(images.size(),1u);EXPECT_EQ(Count(images,magenta),0u);EXPECT_EQ(Count(images,green),1200u);
      Pixel(images,0,5,legacy?55:5,green);Pixel(images,0,5,legacy?5:55,{255,255,255});
    },version);
  }
}

TEST(CoverageXfaPaginationBoundaries, ManualBreaksInsideHorizontalFlowPreserveCompletedCellsAndResumeTheNextRow) {
  for(const std::string flow:{"lr-tb","rl-tb"})for(bool before:{false,true}){
    SCOPED_TRACE(flow);
    SCOPED_TRACE(before);
    const auto cell=[](const std::string& name,RGB color,const std::string& properties){return "<subform name=\""+name+
      "\" layout=\"position\" w=\"40pt\" h=\"40pt\">"+properties+Block("paint",color,40,40)+"</subform>";};
    const auto breaker="<"+std::string(before?"breakBefore":"breakAfter")+" targetType=\"pageArea\" startNew=\"1\"/>";
    const auto body="<subform name=\"horizontal\" layout=\""+flow+"\" w=\"100pt\">"+
      cell("first",red,before?"":breaker)+cell("second",green,before?breaker:"")+cell("third",blue,"")+"</subform>";
    Scenario("horizontal-manual-"+flow+std::to_string(before),body,Pageset(),[=](const Frames& images){
      // RTL group0 is left-aligned as a group: it starts at the occupied
      // row width, not the container's 100pt right edge. One cell starts at 0;
      // the following pair reverses its two 40pt cells within an 80pt group.
      ASSERT_EQ(images.size(),2u);const int first=flow=="lr-tb"?0:40,next=flow=="lr-tb"?40:0;
      Pixel(images,0,5,5,red);Pixel(images,1,first+5,5,green);Pixel(images,1,next+5,5,blue);
      EXPECT_EQ(Count(images,red),1600u);EXPECT_EQ(Count(images,green),1600u);EXPECT_EQ(Count(images,blue),1600u);
      Pixel(images,0,5,80,{255,255,255});Pixel(images,1,5,80,{255,255,255});
    });
  }
}
}  // namespace
}  // namespace pdfoxium::integration
