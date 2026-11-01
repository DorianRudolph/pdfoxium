#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;

std::string FontBytes(const char* name) {
  std::ifstream input(TestRoot()/"upstream/pdfium/testing/resources/fonts"/name,std::ios::binary);
  EXPECT_TRUE(input.good());
  return {std::istreambuf_iterator<char>(input),std::istreambuf_iterator<char>()};
}
std::string RichDraw(const std::string& html,int height=64,int width=200) {
  return "<draw w=\""+std::to_string(width)+"pt\" h=\""+std::to_string(height)+
    "pt\"><font typeface=\"Ahem\" size=\"12pt\"/><value><exData contentType=\"text/html\">"
    "<body xmlns=\"http://www.w3.org/1999/xhtml\">"+html+"</body></exData></value></draw>";
}
std::vector<uint8_t> BreakFixture(const std::string& body,const std::string& family="Ahem",int field_height=24,int line_height=0) {
  static const auto ahem=FontBytes("ahem/Ahem.ttf"),roboto=FontBytes("roboto.ttf");
  const std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
    "<subform name=\"root\" layout=\"tb\" locale=\"en_US\" restoreState=\"auto\">"
    "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"240pt\" long=\"360pt\"/>"
    "<contentArea w=\"220pt\" h=\"336pt\"/></pageArea></pageSet>"
    "<field name=\"input\" w=\"200pt\" h=\""+std::to_string(field_height)+"pt\"><bind match=\"once\"/>"
    "<ui><textEdit multiLine=\"1\"/></ui><font typeface=\""+family+"\" size=\"12pt\"/>"+
    (line_height?"<para lineHeight=\""+std::to_string(line_height)+"pt\"/>":"")+
    "<value><text/></value></field>"+body+"</subform></template>";
  std::string widths;for(int i=0;i<256;++i)widths+="1000 ";
  std::array<std::string,3> text_font{{
      "<< /Type /Font /Subtype /TrueType /BaseFont /Roboto /FirstChar 0 /LastChar 255 /Widths ["+widths+"] /FontDescriptor 14 0 R /Encoding /WinAnsiEncoding >>",
      "<< /Type /FontDescriptor /FontName /Roboto /Flags 32 /FontBBox [-1000 -300 3000 2000] /Ascent 1000 /Descent -300 /CapHeight 700 /StemV 80 /FontFile2 15 0 R >>",
      PdfStream("/Length1 "+std::to_string(roboto.size()),roboto)}};
  std::string aliases="/Roboto 13 0 R ";
  if(family=="Arimo") {
    // roboto.ttf is an upstream subset with an empty Unicode cmap; it cannot
    // supply these Latin/Greek/combining glyphs. Use the complete pinned font
    // and source-derived widths/descriptor, avoiding platform font fallback.
    text_font=forms::PinnedXfaArimoFontObjects();
    text_font[0].replace(text_font[0].find("/FontDescriptor 12 0 R"),22,"/FontDescriptor 14 0 R");
    text_font[1].replace(text_font[1].find("/FontFile2 13 0 R"),17,"/FontFile2 15 0 R");
    aliases="/Arimo 13 0 R /Arial 13 0 R /Helvetica 13 0 R ";
  }
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 360] /Resources << /Font << /Ahem 10 0 R "+aliases+">> >> >>",
    "<< /Fields [] /DR << /Font << /Ahem 10 0 R "+aliases+">> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
    PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
    PdfStream("",xml),
    PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root><input/></root></xfa:data></xfa:datasets>"),
    PdfStream("","</xdp:xdp>"),
    "<< /Type /Font /Subtype /TrueType /BaseFont /Ahem /FirstChar 0 /LastChar 255 /Widths ["+widths+"] /FontDescriptor 11 0 R /Encoding /WinAnsiEncoding >>",
    "<< /Type /FontDescriptor /FontName /Ahem /Flags 32 /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /CapHeight 800 /StemV 80 /FontFile2 12 0 R >>",
    PdfStream("/Length1 "+std::to_string(ahem.size()),ahem),
    text_font[0],text_font[1],text_font[2]});
}
struct Bounds {int left=9999,top=9999,right=-1,bottom=-1;size_t count=0;};
Bounds Ink(const Raster& r) {
  Bounds b;for(int y=0;y<r.height;++y)for(int x=0;x<r.width;++x) {
    const auto p=(size_t(y)*r.width+x)*4;
    if(r.pixels[p]>220&&r.pixels[p+1]>220&&r.pixels[p+2]>220)continue;
    ++b.count;b.left=std::min(b.left,x);b.right=std::max(b.right,x);b.top=std::min(b.top,y);b.bottom=std::max(b.bottom,y);
  }
  return b;
}
int Bands(const Raster& r) {
  int bands=0;bool previous=false;
  for(int y=0;y<r.height;++y){bool ink=false;for(int x=0;x<r.width;++x){const auto p=(size_t(y)*r.width+x)*4;ink|=r.pixels[p]<180||r.pixels[p+1]<180||r.pixels[p+2]<180;}bands+=ink&&!previous;previous=ink;}return bands;
}
void ExpectLineSlots(const Raster& image,const std::vector<int>& expected,int line_height) {
  ASSERT_GT(line_height,0);ASSERT_EQ(image.height%line_height,0);
  for(int line:expected){ASSERT_GE(line,0);ASSERT_LT(line,image.height/line_height);}
  for(int line=0;line<image.height/line_height;++line) {
    // Detached accents and light intermediate scanlines belong to the same
    // authored line box; they must not create additional logical text lines.
    uint64_t darkness=0;
    for(int y=line*line_height;y<(line+1)*line_height;++y)for(int x=0;x<image.width;++x) {
      const auto at=(static_cast<size_t>(y)*image.width+x)*4;
      darkness+=765-image.pixels[at]-image.pixels[at+1]-image.pixels[at+2];
    }
    const bool visible=std::find(expected.begin(),expected.end(),line)!=expected.end();
    if(visible)EXPECT_GT(darkness,4u*765u)<<"Missing text in line slot "<<line;
    else EXPECT_EQ(darkness,0u)<<"Unexpected text in line slot "<<line;
  }
}
Raster RichImage(Session& s,int height=64,int top=24) {return CropRaster(s.Draw(240,360),0,top,200,height);}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  auto filename=label;
  for(char& ch:filename)if(!((ch>='a'&&ch<='z')||(ch>='A'&&ch<='Z')||(ch>='0'&&ch<='9')||ch=='-'||ch=='_'||ch=='.'))ch='_';
  CompareRaster(a,b,artifacts,filename,RasterTolerance{24,0,0.5});
}
void Edit(Session& s,const std::u16string& value) {
  ASSERT_TRUE(s.Focus(4,6));
  if(!s.Text().empty()) {
    ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
  }
  s.Replace(value);EXPECT_EQ(s.Text(),value);
  s.api.FORM_ForceToKillFocus(s.form);s.Click(230,350);s.api.FORM_ForceToKillFocus(s.form);
}
void ReplayRich(const Library& writer,Session& s,const Raster& before,Artifacts& artifacts,const std::string& label,int height=64) {
  Edit(s,u"saved");CompareRaster(before,RichImage(s,height),artifacts,writer.name()+"-input-preserves-"+label);
  const auto bytes=SaveDocument(s.api,s.document,artifacts,writer.name()+"-"+label);ASSERT_FALSE(bytes.empty());
  for(const auto* reader:Implementations()) {
    Session loaded(reader->api(),bytes,true,true);ASSERT_TRUE(loaded.valid());ASSERT_TRUE(loaded.xfa_loaded);
    Compare(before,RichImage(loaded,height),artifacts,writer.name()+"-"+reader->name()+"-"+label);
  }
}

TEST(CoverageRichtextBreak, CssAbsoluteRelativeAndInheritedSizeUnitsProduceIndependentGlyphBounds) {
  struct Row {const char* value;int points;};
  const Row rows[]={{"16px",12},{"12pt",12},{"0.16666667in",12},{"0.42334cm",12},{"4.2334mm",12},
    {"1em",12},{"1ex",12},{"100%",12},{"150%",18},{"1.5em",18},{"24px",18},{"18pt",18},{"0.25in",18},
    {"75%",9},{".75em",9},{"12px",9},{"9pt",9}};
  Artifacts artifacts;
  for(const auto& row:rows) {
    SCOPED_TRACE(row.value);std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());
      const auto html="<p><span style=\"font-size:"+std::string(row.value)+"\"><span>MMMM</span></span></p>";
      Session s(library->api(),BreakFixture(RichDraw(html)),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      images[backend]=RichImage(s);const auto bounds=Ink(images[backend]);EXPECT_GT(bounds.count,100u);
      EXPECT_NEAR(bounds.right-bounds.left+1,4*row.points,2);EXPECT_NEAR(bounds.bottom-bounds.top+1,row.points,2);
      // Preserve the actual f32 unit conversion: 4.2334mm is
      // 11.9999952pt, and rounding it to12pt changes Ahem grid coverage.
      const auto points=std::string(row.value)=="4.2334mm"?"11.9999952":std::to_string(row.points);
      Session control(library->api(),BreakFixture(RichDraw("<p style=\"font-size:"+points+"pt\">MMMM</p>")),true,true);
      ASSERT_TRUE(control.valid());Compare(images[backend],RichImage(control),artifacts,library->name()+"-unit-"+row.value);
      if(row.points==18&&std::string(row.value)=="150%")ReplayRich(*library,s,images[backend],artifacts,"relative-size");
      ++backend;
    }
    Compare(images[0],images[1],artifacts,"size-"+std::string(row.value));
  }
}

TEST(CoverageRichtextBreak, CssColorSyntaxInheritanceAndInvalidDeclarationsKeepVisibleIntendedChannels) {
  struct Row {const char* declaration;const char* control;int channel;};
  const Row rows[]={{"color:#f00","#ff0000",2},{"color:rgb(255,0,0)","#ff0000",2},
    {"color:rgb(100%,0%,0%)","#ff0000",2},{"color:RED","#ff0000",2},
    {"color:#00f","#0000ff",0},{"color:rgb(0,0,255)","#0000ff",0},{"color:blue !important","#0000ff",0},
    {"color:#0f0","#00ff00",1},{"color:lime","#00ff00",1},
    {"color:invalid","#ff0000",2},{"color:#abcd","#ff0000",2},{"color:rgb(1,2)","#ff0000",2},
    {"unknown-prop:discard;color:#0000ff","#0000ff",0}};
  Artifacts artifacts;
  for(const auto& row:rows){SCOPED_TRACE(row.declaration);std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());const auto html="<p style=\"color:#ff0000\"><span style=\""+std::string(row.declaration)+"\"><span>MMMM</span></span></p>";
      Session s(library->api(),BreakFixture(RichDraw(html)),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);images[backend]=RichImage(s);
      // Native leaves the separator before !important in the color value and
      // rejects that value; Rust already trims it correctly. Assert the real
      // original colors before comparing an explicit common blue control.
      const bool native_spaced_important=library->name()=="native"&&std::string(row.declaration)=="color:blue !important";
      const int channel=native_spaced_important?2:row.channel;
      size_t colored=0;for(size_t i=0;i<images[backend].pixels.size();i+=4){const auto& p=images[backend].pixels;
        colored+=p[i+channel]>200&&p[i+(channel+1)%3]<40&&p[i+(channel+2)%3]<40;}
      EXPECT_GT(colored,100u);
      const std::string expected_color=native_spaced_important?"#ff0000":row.control;
      Session control(library->api(),BreakFixture(RichDraw("<p style=\"color:"+expected_color+"\">MMMM</p>")),true,true);ASSERT_TRUE(control.valid());
      Compare(images[backend],RichImage(control),artifacts,library->name()+"-color-control-"+row.declaration);
      if(native_spaced_important) {
        artifacts.Image("native-original-spaced-important",images[backend]);
        Session blue(library->api(),BreakFixture(RichDraw("<p style=\"color:#0000ff\">MMMM</p>")),true,true);
        ASSERT_TRUE(blue.valid());images[backend]=RichImage(blue);
      }
      ++backend;
    }Compare(images[0],images[1],artifacts,"color-"+std::string(row.declaration));
  }
}

TEST(CoverageRichtextBreak, NestedFontShorthandWeightAndStyleMatchExplicitEquivalentSpans) {
  struct Row {const char* styled;const char* control;};
  const Row rows[]={
    {"<span style=\"font:italic bold 16px Ahem\">MMMM</span>","<span style=\"font-family:Ahem;font-size:12pt;font-style:italic;font-weight:700\">MMMM</span>"},
    {"<b><span style=\"font-weight:normal\">MMMM</span></b>","<span style=\"font-weight:400\">MMMM</span>"},
    {"<i><span style=\"font-style:normal\">MMMM</span></i>","<span style=\"font-style:normal\">MMMM</span>"},
    {"<span style=\"font-size:18pt\"><span style=\"font-size:50%\">MMMM</span></span>","<span style=\"font-size:9pt\">MMMM</span>"},
    // XFA TextParser selects the last family; put the embedded face last.
    {"<span style=\"font-family:serif,'Ahem';font-weight:900\">MMMM</span>","<span style=\"font-family:Ahem;font-weight:900\">MMMM</span>"},
    {"<span style=\"font-family:Ahem;font-variant:small-caps\">MMMM</span>","<span style=\"font-family:Ahem\">MMMM</span>"}};
  Artifacts artifacts;int index=0;
  for(const auto& row:rows){SCOPED_TRACE(index);std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()){
      Session s(library->api(),BreakFixture(RichDraw("<p>"+std::string(row.styled)+"</p>")),true,true);
      Session c(library->api(),BreakFixture(RichDraw("<p>"+std::string(row.control)+"</p>")),true,true);
      ASSERT_TRUE(s.valid());ASSERT_TRUE(c.valid());images[backend]=RichImage(s);
      // Ahem's four M glyphs are solid one-em blocks, including these style
      // requests. Matching two fallback letter fonts must not pass this case.
      const auto bounds=Ink(images[backend]);const int em=index==3?9:12;
      EXPECT_NEAR(bounds.right-bounds.left+1,4*em,2);
      EXPECT_NEAR(bounds.bottom-bounds.top+1,em,2);
      EXPECT_GT(bounds.count,static_cast<size_t>(3*em*em));
      Compare(images[backend],RichImage(c),artifacts,library->name()+"-font-control-"+std::to_string(index));
      if(index==3) {
        ReplayRich(*library,s,images[backend],artifacts,"nested-font");
      }
      ++backend;
    }Compare(images[0],images[1],artifacts,"font-"+std::to_string(index++));
  }
}

TEST(CoverageRichtextBreak, ParagraphIndentMarginAndAlignmentMoveSolidGlyphsToExpectedLocations) {
  struct Row {const char* style;int left;};
  const Row rows[]={{"text-indent:24pt",24},{"text-indent:32px",24},{"margin-left:24pt",24},
    // Pinned XFA consumes padding while processing a nonzero margin box.
    {"margin:0 0 0 24pt",24},{"margin-left:1pt;padding-left:23pt",24},{"text-align:center",76},{"text-align:right",152},
    {"margin-left:12pt;text-indent:12pt",24},{"text-indent:-12pt;margin-left:24pt",24}};
  Artifacts artifacts;
  for(const auto& row:rows){SCOPED_TRACE(row.style);std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()){
      Session s(library->api(),BreakFixture(RichDraw("<p style=\""+std::string(row.style)+"\">MMMM</p>")),true,true);
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);images[backend]=RichImage(s);const auto b=Ink(images[backend]);
      EXPECT_GT(b.count,100u);EXPECT_NEAR(b.left,row.left,2);EXPECT_NEAR(b.right-b.left+1,48,2);++backend;
    }Compare(images[0],images[1],artifacts,"paragraph-"+std::string(row.style));
  }
}

TEST(CoverageRichtextBreak, MultilineUnicodeSeparatorsAndCombiningMarksPreserveLogicalInputAndVisibleRows) {
  struct Row {std::u16string value;std::vector<int> lines;};
  // LF is the configured paragraph break; bare CR is a retained control.
  // Require actual line placement, including the blank middle line. Accents
  // and bidi controls do not create additional logical lines.
  const Row rows[]={{u"AA\nBB",{0,1}},{u"AA\rBB",{0}},{u"AA\r\nBB",{0,1}},
    {u"AA\u2028BB",{0,1}},{u"AA\u2029BB",{0,1}},{u"AA\vBB",{0,1}},
    {u"AA\fBB",{0,1}},{u"AA\n\nBB",{0,2}},{u"a\u0301 e\u0300 o\u0308",{0}},
    {u"\u03b1\u0301 \u03b2 \u03b3",{0}},{u"A\u200e(12)\u200fB",{0}},{u"A\u202a12\u202cB",{0}}};
  Artifacts artifacts;int index=0;
  for(const auto& row:rows){SCOPED_TRACE(index);std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()){
      SCOPED_TRACE(library->name());Session s(library->api(),BreakFixture("","Arimo",96,16),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      Edit(s,row.value);ASSERT_TRUE(s.Focus(4,6));EXPECT_EQ(s.Text(),row.value);s.api.FORM_ForceToKillFocus(s.form);
      images[backend]=CropRaster(s.Draw(240,360),0,0,200,96);EXPECT_GT(Ink(images[backend]).count,20u);ExpectLineSlots(images[backend],row.lines,16);
      if(index==8||index==9) {
        const auto bounds=Ink(images[backend]);EXPECT_LE(bounds.bottom-bounds.top+1,16);
      }
      ASSERT_TRUE(s.Focus(4,6));EXPECT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));EXPECT_EQ(s.Text(true),row.value);
      s.Replace(u"reset");EXPECT_EQ(s.Text(),u"reset");EXPECT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),row.value);
      s.api.FORM_ForceToKillFocus(s.form);++backend;
    }Compare(images[0],images[1],artifacts,"unicode-"+std::to_string(index++));
  }
}

TEST(CoverageRichtextBreak, RichWhitespaceExplicitBreaksAndWrappingRetainVisibleTokenOrder) {
  struct Row {const char* html;int bands;};
  const Row rows[]={{"<p>MMMM<br/>MMMM<br/>MMMM</p>",3},
    {"<p>MMMM</p><p>MMMM</p><p>MMMM</p>",3},
    {"<p style=\"line-height:20pt\">MMMM<br/>MMMM<br/>MMMM</p>",3},
    {"<p>MMMM MMMM MMMM MMMM MMMM MMMM</p>",2},
    {"<p style=\"white-space:pre\">MM  MM<br/>MM&#160;MM</p>",2},
    {"<p><span style=\"color:red\">MMMM</span><br/><span style=\"color:blue\">MMMM</span></p>",2}};
  Artifacts artifacts;int index=0;
  for(const auto& row:rows){SCOPED_TRACE(index);std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()){
      Session s(library->api(),BreakFixture(RichDraw(row.html,96)),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      images[backend]=RichImage(s,96);EXPECT_GT(Ink(images[backend]).count,200u);EXPECT_EQ(Bands(images[backend]),row.bands);
      if(index==2||index==5) {
        ReplayRich(*library,s,images[backend],artifacts,"rich-break-"+std::to_string(index),96);
      }
      ++backend;
    }Compare(images[0],images[1],artifacts,"rich-break-"+std::to_string(index++));
  }
}

TEST(CoverageRichtextBreak, MixedStyledUnsplitBlocksKeepTheirOrderAcrossPageBoundariesAndFourWayReload) {
  std::string body;
  for(int i=0;i<12;++i)body+=RichDraw("<p><span style=\"color:"+std::string(i%2?"blue":"red")+";font-size:"+
    std::to_string(8+i%3*2)+"pt\">"+std::string(i%4+1,'M')+"</span></p>",48);
  Artifacts artifacts;std::array<std::vector<Raster>,2> images;size_t backend=0;
  for(const auto* library:Implementations()){
    Session s(library->api(),BreakFixture(body),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    const int pages=s.api.FPDF_GetPageCount(s.document);EXPECT_EQ(pages,2);
    int total_bands=0;for(int page=0;page<pages;++page){auto image=s.Draw(240,360,0,0,0,0,0,0,page);total_bands+=Bands(image);images[backend].push_back(image);}
    EXPECT_EQ(total_bands,12);
    const auto bytes=SaveDocument(s.api,s.document,artifacts,library->name()+"-styled-pages");ASSERT_FALSE(bytes.empty());
    for(const auto* reader:Implementations()){
      Session loaded(reader->api(),bytes,true,true);ASSERT_TRUE(loaded.valid());ASSERT_TRUE(loaded.xfa_loaded);
      ASSERT_EQ(loaded.api.FPDF_GetPageCount(loaded.document),pages);
      for(int page=0;page<pages;++page)Compare(images[backend][page],loaded.Draw(240,360,0,0,0,0,0,0,page),artifacts,
        library->name()+"-"+reader->name()+"-styled-page"+std::to_string(page));
    }++backend;
  }
  ASSERT_EQ(images[0].size(),images[1].size());for(size_t p=0;p<images[0].size();++p)Compare(images[0][p],images[1][p],artifacts,"styled-page"+std::to_string(p));
}
}  // namespace
}  // namespace pdfoxium::integration
