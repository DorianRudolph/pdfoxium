#include "xfa_form_fixture.h"
#include "text_fixture.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <map>
#include <sstream>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
constexpr int kSide=320;
constexpr float kSize=18;
struct Metric {int advance,width,height;};
// Independently read from the unmodified pinned DejaVuSans.ttf cmap/glyf/hmtx:
// advance is in 1000/em units; outline dimensions are original 2048/em units.
// These are presentation-form glyph metrics, not inferred from either engine.
const std::map<char16_t,Metric> kMetrics={
  {0x0020,{317,0,0}},
  {0x0021,{400,203,1493}},
  {0x0022,{459,548,555}},
  {0x0023,{837,1401,1470}},
  {0x0024,{636,963,1857}},
  {0x0025,{950,1720,1549}},
  {0x0026,{779,1405,1549}},
  {0x0027,{274,170,555}},
  {0x0028,{390,459,1824}},
  {0x0029,{390,459,1824}},
  {0x002a,{500,902,934}},
  {0x002b,{837,1282,1284}},
  {0x002c,{317,293,492}},
  {0x002d,{360,539,164}},
  {0x002e,{317,211,254}},
  {0x002f,{336,690,1683}},
  {0x0030,{636,1032,1549}},
  {0x0031,{636,889,1493}},
  {0x0032,{636,948,1520}},
  {0x0033,{636,983,1549}},
  {0x0034,{636,1088,1493}},
  {0x0035,{636,966,1522}},
  {0x0036,{636,1031,1549}},
  {0x0037,{636,960,1493}},
  {0x0038,{636,1024,1549}},
  {0x0039,{636,1030,1549}},
  {0x003a,{336,211,1059}},
  {0x003b,{336,293,1297}},
  {0x003c,{837,1282,1096}},
  {0x003d,{837,1282,578}},
  {0x003e,{837,1282,1096}},
  {0x003f,{530,797,1520}},
  {0x0621,{470,635,905}},
  {0x0622,{277,720,1923}},
  {0x0623,{277,343,2045}},
  {0x0624,{482,917,1705}},
  {0x0625,{277,343,2056}},
  {0x0626,{782,1342,1474}},
  {0x0627,{277,184,1556}},
  {0x0628,{941,1641,1019}},
  {0x0629,{523,789,1108}},
  {0x062a,{941,1641,820}},
  {0x062b,{941,1641,1070}},
  {0x062c,{645,1163,1370}},
  {0x062d,{645,1163,1370}},
  {0x062e,{645,1163,1700}},
  {0x062f,{445,670,888}},
  {0x0630,{445,670,1238}},
  {0x0631,{482,951,1050}},
  {0x0632,{482,951,1450}},
  {0x0633,{1220,2200,1250}},
  {0x0634,{1220,2200,1700}},
  {0x0635,{1208,2193,1241}},
  {0x0636,{1208,2193,1450}},
  {0x0637,{924,1612,1556}},
  {0x0638,{924,1612,1556}},
  {0x0639,{596,1085,1566}},
  {0x063a,{596,1085,1850}},
  {0x0640,{292,640,184}},
  {0x0641,{1036,1820,1392}},
  {0x0642,{775,1328,1740}},
  {0x0643,{824,1335,1611}},
  {0x0644,{726,1160,1868}},
  {0x0645,{619,978,1247}},
  {0x0646,{734,1205,1281}},
  {0x0647,{523,789,792}},
  {0x0648,{482,917,1146}},
  {0x0649,{782,1342,1111}},
  {0x064a,{782,1342,1342}},
  {0x064b,{0,584,480}},
  {0x064c,{0,584,579}},
  {0x064d,{0,584,480}},
  {0x064e,{0,584,240}},
  {0x064f,{0,584,580}},
  {0x0650,{0,584,240}},
  {0x0651,{0,624,553}},
  {0x0652,{0,548,549}},
  {0x0653,{0,720,264}},
  {0x0654,{0,343,440}},
  {0x0655,{0,343,440}},
  {0x0660,{537,220,250}},
  {0x0661,{537,422,1300}},
  {0x0662,{537,926,1300}},
  {0x0663,{537,966,1300}},
  {0x0664,{537,760,1332}},
  {0x0665,{537,830,1336}},
  {0x0666,{537,925,1300}},
  {0x0667,{537,980,1300}},
  {0x0668,{537,980,1300}},
  {0x0669,{537,910,1310}},
  {0x0670,{0,110,584}},
  {0xfe81,{277,720,1923}},
  {0xfe82,{304,720,1923}},
  {0xfe83,{277,343,2105}},
  {0xfe84,{304,536,2105}},
  {0xfe85,{482,917,1705}},
  {0xfe86,{516,1163,1705}},
  {0xfe87,{277,343,2056}},
  {0xfe88,{304,536,2056}},
  {0xfe89,{782,1342,1474}},
  {0xfe8a,{833,1597,1227}},
  {0xfe8b,{278,485,1255}},
  {0xfe8c,{301,658,1255}},
  {0xfe8d,{277,184,1556}},
  {0xfe8e,{304,451,1556}},
  {0xfe8f,{941,1641,1019}},
  {0xfe90,{981,1901,1019}},
  {0xfe91,{278,411,900}},
  {0xfe92,{301,658,900}},
  {0xfe93,{523,789,1108}},
  {0xfe94,{536,973,1050}},
  {0xfe95,{941,1641,820}},
  {0xfe96,{981,1901,820}},
  {0xfe97,{278,519,1000}},
  {0xfe98,{301,658,1000}},
  {0xfe99,{941,1641,1070}},
  {0xfe9a,{981,1901,1070}},
  {0xfe9b,{278,519,1250}},
  {0xfe9c,{301,658,1250}},
  {0xfe9d,{645,1163,1370}},
  {0xfe9e,{645,1185,1370}},
  {0xfe9f,{618,1136,1115}},
  {0xfea0,{645,1362,1115}},
  {0xfea1,{645,1163,1370}},
  {0xfea2,{645,1185,1370}},
  {0xfea3,{618,1136,815}},
  {0xfea4,{645,1362,815}},
  {0xfea5,{645,1163,1700}},
  {0xfea6,{645,1185,1700}},
  {0xfea7,{618,1136,1100}},
  {0xfea8,{645,1362,1100}},
  {0xfea9,{445,670,888}},
  {0xfeaa,{524,970,888}},
  {0xfeab,{445,670,1238}},
  {0xfeac,{524,970,1238}},
  {0xfead,{482,951,1050}},
  {0xfeae,{551,1235,1050}},
  {0xfeaf,{482,951,1450}},
  {0xfeb0,{551,1235,1450}},
  {0xfeb1,{1220,2200,1250}},
  {0xfeb2,{1274,2501,1250}},
  {0xfeb3,{837,1566,779}},
  {0xfeb4,{892,1867,779}},
  {0xfeb5,{1220,2200,1700}},
  {0xfeb6,{1274,2501,1700}},
  {0xfeb7,{837,1566,1229}},
  {0xfeb8,{892,1867,1229}},
  {0xfeb9,{1208,2193,1241}},
  {0xfeba,{1225,2399,1241}},
  {0xfebb,{849,1606,741}},
  {0xfebc,{867,1816,741}},
  {0xfebd,{1208,2193,1450}},
  {0xfebe,{1225,2399,1450}},
  {0xfebf,{849,1606,950}},
  {0xfec0,{867,1816,950}},
  {0xfec1,{924,1612,1556}},
  {0xfec2,{949,1820,1556}},
  {0xfec3,{795,1512,1556}},
  {0xfec4,{820,1720,1556}},
  {0xfec5,{924,1612,1556}},
  {0xfec6,{949,1820,1556}},
  {0xfec7,{795,1512,1556}},
  {0xfec8,{820,1720,1556}},
  {0xfec9,{596,1085,1566}},
  {0xfeca,{532,1085,1282}},
  {0xfecb,{596,1036,1066}},
  {0xfecc,{482,1028,782}},
  {0xfecd,{596,1085,1850}},
  {0xfece,{532,1085,1600}},
  {0xfecf,{522,1036,1350}},
  {0xfed0,{482,1028,1100}},
  {0xfed1,{1036,1820,1392}},
  {0xfed2,{1035,2010,1191}},
  {0xfed3,{478,851,1300}},
  {0xfed4,{505,1076,1150}},
  {0xfed5,{775,1328,1740}},
  {0xfed6,{833,1621,1525}},
  {0xfed7,{478,851,1300}},
  {0xfed8,{505,1076,1150}},
  {0xfed9,{824,1335,1611}},
  {0xfeda,{842,1602,1611}},
  {0xfedb,{476,995,1556}},
  {0xfedc,{552,1171,1556}},
  {0xfedd,{726,1160,1868}},
  {0xfede,{757,1427,1868}},
  {0xfedf,{304,451,1556}},
  {0xfee0,{331,718,1556}},
  {0xfee1,{619,978,1247}},
  {0xfee2,{665,1243,1120}},
  {0xfee3,{535,954,672}},
  {0xfee4,{578,1224,671}},
  {0xfee5,{734,1205,1281}},
  {0xfee6,{761,1432,1200}},
  {0xfee7,{278,411,1000}},
  {0xfee8,{301,658,1000}},
  {0xfee9,{523,789,792}},
  {0xfeea,{536,973,750}},
  {0xfeeb,{527,977,1064}},
  {0xfeec,{460,984,1182}},
  {0xfeed,{482,917,1146}},
  {0xfeee,{516,1163,1146}},
  {0xfeef,{782,1342,1111}},
  {0xfef0,{833,1597,786}},
  {0xfef1,{782,1342,1342}},
  {0xfef2,{833,1597,1014}},
  {0xfef3,{278,519,900}},
  {0xfef4,{301,658,900}},
  {0xfef5,{570,1169,1793}},
  {0xfef6,{596,1452,1793}},
  {0xfef7,{570,986,1975}},
  {0xfef8,{596,1269,1975}},
  {0xfef9,{570,936,2056}},
  {0xfefa,{596,1219,2056}},
  {0xfefb,{570,875,1576}},
  {0xfefc,{596,1158,1576}},
};
const Metric& Metrics(char16_t code) {
  const auto found=kMetrics.find(code);if(found!=kMetrics.end())return found->second;
  ADD_FAILURE()<<"missing independent font metric U+"<<std::hex<<unsigned(code);
  static const Metric missing{};return missing;
}
std::string Number(float value){std::ostringstream out;out<<value;return out.str();}
std::string Xml(const std::u16string& text) {
  std::string out;for(char16_t ch:text)out+="&#"+std::to_string(unsigned(ch))+";";return out;
}
std::string Field(const std::string& name,const std::u16string& value,float x,float y,
    int width=220,int height=44,bool bound=false) {
  return "<field name=\""+name+"\" x=\""+Number(x)+"pt\" y=\""+Number(y)+"pt\" w=\""+
    std::to_string(width)+"pt\" h=\""+std::to_string(height)+"pt\"><bind match=\""+(bound?"once":"none")+
    "\"/><ui><textEdit multiLine=\"1\"/></ui><font typeface=\"DejaVu Sans\" size=\"18pt\"/>"
    "<para lineHeight=\"18pt\" spaceAbove=\"4pt\"/><value><text>"+Xml(value)+"</text></value>"+
    forms::Event("if(this.rawValue!==null)this.rawValue=String(this.rawValue).replace(/^\\n/,'');","initialize")+"</field>";
}
std::string Rich(const std::string& html,float x,float y,int width=220,int height=64) {
  return "<draw x=\""+Number(x)+"pt\" y=\""+Number(y)+"pt\" w=\""+std::to_string(width)+"pt\" h=\""+
    std::to_string(height)+"pt\"><font typeface=\"DejaVu Sans\" size=\"18pt\"/>"
    "<para lineHeight=\"18pt\" spaceAbove=\"4pt\"/><value><exData contentType=\"text/html\"><body xmlns=\"http://www.w3.org/1999/xhtml\">"+
    html+"</body></exData></value></draw>";
}
std::vector<uint8_t> Fixture(const std::string& body) {
  static const auto font=text_fixture::Read(TestRoot()/"tests/capi/resources/test_fonts/DejaVuSans.ttf");
  std::string widths;for(int i=0;i<256;++i)widths+="1000 ";
  const auto xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
    "<subform name=\"root\" layout=\"position\" locale=\"en_US\" restoreState=\"auto\">"
    "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"320pt\" long=\"320pt\"/>"
    "<contentArea w=\"310pt\" h=\"310pt\"/></pageArea></pageSet>"+body+"</subform></template>";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 320] /Resources << /Font << /DejaVuSans 10 0 R >> >> >>",
    "<< /Fields [] /DR << /Font << /DejaVuSans 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    // Native's known streaming decoder drops a UTF8 scalar across byte1023.
    // Keep this shaping batch on the common healthy path; the original split
    // remains covered by CoverageBarcodeBoundaries.Utf8ScalarAcrossInputPlane...
    // and docs/capi-known-native-differences.md. Logical values are unchanged.
    PdfStream("",std::string(64,' ')+"<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
    PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
    PdfStream("",xml),
    PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
    PdfStream("","</xdp:xdp>"),
    "<< /Type /Font /Subtype /TrueType /BaseFont /DejaVuSans /FirstChar 0 /LastChar 255 /Widths ["+widths+
      "] /Encoding /WinAnsiEncoding /FontDescriptor 11 0 R >>",
    "<< /Type /FontDescriptor /FontName /DejaVuSans /Flags 32 /FontBBox [-1021 -463 1794 1233] /Ascent 928 /Descent -235 /CapHeight 729 /ItalicAngle 0 /StemV 80 /FontFile2 12 0 R >>",
    PdfStream("/Length1 "+std::to_string(font.size()),std::string(font.begin(),font.end()))});
}
void Blur(Session& s){s.api.FORM_ForceToKillFocus(s.form);s.Click(315,315);s.api.FORM_ForceToKillFocus(s.form);}
void Edit(Session& s,const std::u16string& value) {
  ASSERT_TRUE(s.Focus(4,6));
  if(!s.Text().empty()){ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));}
  s.Replace(value);EXPECT_EQ(s.Text(),value);
  ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));EXPECT_EQ(s.Text(true),value);
  ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey));Blur(s);
}
void History(Session& s,const std::u16string& value) {
  ASSERT_TRUE(s.Focus(4,6));EXPECT_EQ(s.Text(),value);
  ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));EXPECT_EQ(s.Text(true),value);
  s.Replace(u"\u0645");EXPECT_EQ(s.Text(),u"\u0645");
  ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),value);
  ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),u"\u0645");
  ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),value);
  ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey));Blur(s);
}
struct InkBox {int left=9999,top=9999,right=-1,bottom=-1;uint64_t mass=0;size_t pixels=0;};
InkBox Ink(const Raster& image) {
  InkBox b;for(int y=0;y<image.height;++y)for(int x=0;x<image.width;++x) {
    const auto p=(size_t(y)*image.width+x)*4;const int gray=std::min({image.pixels[p],image.pixels[p+1],image.pixels[p+2]});
    b.mass+=255-gray;if(gray>=180)continue;++b.pixels;
    b.left=std::min(b.left,x);b.right=std::max(b.right,x);b.top=std::min(b.top,y);b.bottom=std::max(b.bottom,y);
  }
  return b;
}
int Lines(const Raster& image) {
  int lines=0,last=-20;
  for(int y=0;y<image.height;++y) {
    bool ink=false;for(int x=0;x<image.width;++x){const auto p=(size_t(y)*image.width+x)*4;ink|=std::min({image.pixels[p],image.pixels[p+1],image.pixels[p+2]})<180;}
    if(ink){if(y-last>5)++lines;last=y;}
  }return lines;
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  text_fixture::CompareTextRaster(a,b,artifacts,label,{32,0,.7});
}
std::string Singletons(const std::u16string& visual,float y=100.25f,float x=.25f,bool rich=false) {
  std::string body;int glyph=0;
  for(char16_t code:visual) {
    const auto& metric=Metrics(code);
    if(code!=u' ') {
      // The pinned FEF5 glyph has xMin=-210/2048 em. A field beginning
      // at the pen clips its accent; right alignment expands that clip to
      // the left while retaining the same independently specified pen.
      const bool overhang=code==0xfef5;
      const float left=overhang?x+metric.advance*kSize/1000-40:x;
      auto element=rich?Rich("<p"+std::string(overhang?" style=\"text-align:right\"":"")+">"+Xml({code})+"</p>",left,y,40,64):
          Field("g"+std::to_string(glyph++),{code},left,y,40,64);
      if(overhang) {
        const auto para=element.find("<para ");
        if(para==std::string::npos){ADD_FAILURE()<<"Missing singleton paragraph";return {};}
        element.insert(para+6,"hAlign=\"right\" ");
      }
      body+=element;
    }
    x+=metric.advance*kSize/1000;
  }return body;
}
void Geometry(const Raster& run,const Raster& reference,Artifacts& artifacts,const std::string& label) {
  ASSERT_EQ(run.width,reference.width);ASSERT_EQ(run.height,reference.height);
  artifacts.Image(label+"-logical-run",run);artifacts.Image(label+"-presentation-controls",reference);
  const auto dark=[](const Raster& r,int x,int y){const auto p=(size_t(y)*r.width+x)*4;return std::min({r.pixels[p],r.pixels[p+1],r.pixels[p+2]})<180;};
  const auto directed=[&](const Raster& a,const Raster& b) {
    for(int y=0;y<a.height;++y)for(int x=0;x<a.width;++x) {
      if(!dark(a,x,y)){continue;}
      bool found=false;
      for(int yy=std::max(0,y-1);yy<=std::min(b.height-1,y+1);++yy)
        for(int xx=std::max(0,x-1);xx<=std::min(b.width-1,x+1);++xx)found|=dark(b,xx,yy);
      ASSERT_TRUE(found)<<label<<" unmatched glyph coverage "<<x<<','<<y;
    }
  };
  directed(run,reference);directed(reference,run);
  const auto a=Ink(run),b=Ink(reference);EXPECT_GT(a.pixels,3u);EXPECT_GT(b.pixels,3u);
  EXPECT_NEAR(a.left,b.left,1);EXPECT_NEAR(a.right,b.right,1);EXPECT_NEAR(a.top,b.top,1);EXPECT_NEAR(a.bottom,b.bottom,1);
  EXPECT_LE(a.mass>b.mass?a.mass-b.mass:b.mass-a.mass,std::max(a.mass,b.mass)/10);
}
void Reopen(const Library& writer,Session& s,const std::u16string& value,const Raster& before,
    Artifacts& artifacts,const std::string& label,const std::vector<uint8_t>& source,bool native_bidi=false) {
  const auto saved=SaveDocument(s.api,s.document,artifacts,label+writer.name());ASSERT_FALSE(saved.empty());
  for(const auto* reader:Implementations()) {
    SCOPED_TRACE(reader->name());Session after(reader->api(),saved,true,true);ASSERT_TRUE(after.valid());ASSERT_TRUE(after.xfa_loaded);
    ASSERT_TRUE(after.Focus(4,6));EXPECT_EQ(after.Text(),value);
    ASSERT_TRUE(after.api.FORM_SelectAllText(after.form,after.page));EXPECT_EQ(after.Text(true),value);
    ASSERT_TRUE(after.api.FORM_OnKeyDown(after.form,after.page,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey));Blur(after);
    const auto image=after.Draw(kSide,kSide);
    if(native_bidi&&reader!=&writer) {
      // The pinned native bidi path mismatches logical text and reordered
      // widths. Each reader must preserve its own original appearance; Rust
      // keeps the independent correct presentation-form assertions above.
      Session original(reader->api(),source,true,true);ASSERT_TRUE(original.valid());ASSERT_TRUE(original.xfa_loaded);
      Edit(original,value);
      CompareRaster(original.Draw(kSide,kSide),image,artifacts,label+writer.name()+reader->name()+"-reader-original");
    } else if(reader==&writer)CompareRaster(before,image,artifacts,label+writer.name()+reader->name());
    else Compare(before,image,artifacts,label+writer.name()+reader->name());
  }
}
bool GeometryFits(const Raster& a,const Raster& b) {
  if(a.width!=b.width||a.height!=b.height)return false;
  const auto dark=[](const Raster& r,int x,int y){const auto p=(size_t(y)*r.width+x)*4;return std::min({r.pixels[p],r.pixels[p+1],r.pixels[p+2]})<180;};
  const auto directed=[&](const Raster& x,const Raster& y){
    for(int yy=0;yy<x.height;++yy)for(int xx=0;xx<x.width;++xx) {
      if(!dark(x,xx,yy)){continue;}
      bool near=false;
      for(int j=std::max(0,yy-1);j<=std::min(y.height-1,yy+1);++j)
        for(int i=std::max(0,xx-1);i<=std::min(y.width-1,xx+1);++i)near|=dark(y,i,j);
      if(!near)return false;
    }
    return true;
  };
  const auto x=Ink(a),y=Ink(b);
  return directed(a,b)&&directed(b,a)&&std::abs(x.left-y.left)<=1&&std::abs(x.right-y.right)<=1&&
    std::abs(x.top-y.top)<=1&&std::abs(x.bottom-y.bottom)<=1&&
    (x.mass>y.mass?x.mass-y.mass:y.mass-x.mass)<=std::max(x.mass,y.mass)/10;
}
// Native CFGAS_TxtBreak::BidiLine reorders characters/widths while its edit
// consumer still addresses the logical string. The marked finite rows below
// exercise the corrected Rust path and separately retain the native defect
// observation; public one-glyph controls remain a healthy common oracle.
void NativeBidiObservation(const Raster& run,const Raster& control,Artifacts& artifacts,const std::string& label) {
  artifacts.Image(label+"-native-original",run);artifacts.Image(label+"-correct-presentations",control);
  EXPECT_FALSE(GeometryFits(run,control))<<label<<" pinned native bidi association unexpectedly changed";
}
void ObserveNativeOriginal(const std::array<Raster,2>& images,Artifacts& artifacts,const std::string& label) {
  artifacts.Image(label+"-original-native",images[0]);
  artifacts.Image(label+"-original-rust",images[1]);
  EXPECT_NE(images[0].pixels,images[1].pixels)<<label<<" pinned native bidi association unexpectedly changed";
}
void CompareHealthyFontCoverage(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  ASSERT_EQ(a.width,b.width);ASSERT_EQ(a.height,b.height);
  ASSERT_EQ(a.pixels.size(),b.pixels.size());
  const auto first=Ink(a),second=Ink(b);
  EXPECT_GT(first.pixels,3u);EXPECT_GT(second.pixels,3u);
  EXPECT_EQ(first.left,second.left);EXPECT_EQ(first.right,second.right);
  EXPECT_EQ(first.top,second.top);EXPECT_EQ(first.bottom,second.bottom);
  EXPECT_TRUE(GeometryFits(a,b));
  const auto difference=first.mass>second.mass?first.mass-second.mass:second.mass-first.mass;
  EXPECT_LE(difference*10000,std::max(first.mass,second.mass)*150);
  const auto near_coverage=[](const Raster& image,int x,int y) {
    for(int row=std::max(0,y-1);row<=std::min(image.height-1,y+1);++row)
      for(int column=std::max(0,x-1);column<=std::min(image.width-1,x+1);++column) {
        const auto gray=image.pixels[(size_t(row)*image.width+column)*4];
        if(gray>0&&gray<255)return true;
      }
    return false;
  };
  bool opaque=true,gray=true,endpoints=true,edges=true;
  for(int y=0;y<a.height;++y)for(int x=0;x<a.width;++x) {
    const auto pixel=(size_t(y)*a.width+x)*4;
    opaque&=a.pixels[pixel+3]==255&&b.pixels[pixel+3]==255;
    gray&=a.pixels[pixel]==a.pixels[pixel+1]&&a.pixels[pixel]==a.pixels[pixel+2]&&
        b.pixels[pixel]==b.pixels[pixel+1]&&b.pixels[pixel]==b.pixels[pixel+2];
    const auto old=a.pixels[pixel],current=b.pixels[pixel];
    if(old==current)continue;
    endpoints&=!((old==0&&current==255)||(old==255&&current==0));
    edges&=near_coverage(a,x,y)&&near_coverage(b,x,y);
  }
  EXPECT_TRUE(opaque);EXPECT_TRUE(gray);EXPECT_TRUE(endpoints);EXPECT_TRUE(edges);
  // Identical pinned DejaVu outlines: native FreeType versus Fontations/AGG.
  // Measured coverage maxima are 137 / 2.224 BGRA mean / 1.364% ink;
  // the guards above keep this allowance confined to antialiased edges.
  text_fixture::CompareTextRaster(a,b,artifacts,label,{144,0,2.5});
}
void HealthyControls(std::u16string visual,Artifacts& artifacts,const std::string& label) {
  // Public singleton widgets have no multi-character bidi-width association.
  // Keep this comparison separate from the recorded defective native run.
  visual.erase(std::remove_if(visual.begin(),visual.end(),[](char16_t ch){return ch!=u' '&&Metrics(ch).advance==0;}),visual.end());
  const auto bytes=Fixture(Field("input",u"\u0628",.25f,.25f,220,64,true)+Singletons(visual));
  std::array<Raster,2> images;size_t index=0;
  for(const auto* library:Implementations()) {
    Session s(library->api(),bytes,true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);Blur(s);
    images[index++]=CropRaster(s.Draw(kSide,kSide),0,100,240,64);
  }
  CompareHealthyFontCoverage(images[0],images[1],artifacts,label+"-healthy-singletons");
}
struct Row {std::string label;std::u16string logical,visual;bool native_bidi=false;};
void JoiningRows(const std::vector<Row>& rows) {
  Artifacts artifacts;
  for(const auto& row:rows) {
    SCOPED_TRACE(row.label);const auto bytes=Fixture(Field("input",u"\u0628",.25f,.25f,220,64,true)+Singletons(row.visual));
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),bytes,true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      Edit(s,row.logical);History(s,row.logical);images[backend]=s.Draw(kSide,kSide);
      const auto actual=CropRaster(images[backend],0,0,240,64),control=CropRaster(images[backend],0,100,240,64);
      if(row.native_bidi&&library->name()=="native")NativeBidiObservation(actual,control,artifacts,row.label);
      else Geometry(actual,control,artifacts,row.label+library->name());
      if(row.visual.size()==1&&(!row.native_bidi||library->name()=="rust")) {
        const auto ink=Ink(actual);const auto& metric=Metrics(row.visual.front());
        EXPECT_NEAR(ink.right-ink.left+1,metric.width*kSize/2048,2);
        EXPECT_NEAR(ink.bottom-ink.top+1,metric.height*kSize/2048,2);
      }
      Reopen(*library,s,row.logical,images[backend],artifacts,row.label,bytes,row.native_bidi);++backend;
    }
    if(row.native_bidi) {
      ObserveNativeOriginal(images,artifacts,row.label);
      HealthyControls(row.visual.empty()?u"\ufe90\ufe91":row.visual,artifacts,row.label);
    }
    else Compare(images[0],images[1],artifacts,row.label+"-engines");
  }
}

TEST(CoverageXfaArabic, ContextualIsolatedInitialMedialFinalAndNonJoiningLettersUseRealPresentationForms) {
  JoiningRows({
    {"beh-isolated",u"\u0628",u"\ufe8f"},
    {"beh-pair",u"\u0628\u0628",u"\ufe90\ufe91",true},
    {"beh-medial",u"\u0628\u0628\u0628",u"\ufe90\ufe92\ufe91",true},
    {"teh-medial",u"\u062a\u062a\u062a",u"\ufe96\ufe98\ufe97",true},
    {"noon-medial",u"\u0646\u0646\u0646",u"\ufee6\ufee8\ufee7",true},
    {"seen-meem",u"\u0633\u0645",u"\ufee2\ufeb3",true},
    {"beh-alef-beh",u"\u0628\u0627\u0628",u"\ufe8f\ufe8e\ufe91",true},
    {"beh-dal-beh",u"\u0628\u062f\u0628",u"\ufe8f\ufeaa\ufe91",true},
    {"dal-beh",u"\u062f\u0628",u"\ufe8f\ufea9",true},
    {"waw-beh",u"\u0648\u0628",u"\ufe8f\ufeed",true},
    {"ta-marbuta-beh",u"\u0629\u0628",u"\ufe8f\ufe93",true},
    {"hamza-boundary",u"\u0628\u0621\u0628",u"\ufe8f\u0621\ufe8f"}});
}

TEST(CoverageXfaArabic, AllFourLamAlefLigaturesRetainBothLogicalCharactersAndConnectedContexts) {
  std::vector<Row> rows;
  for(const auto& variant:std::array<std::array<char16_t,2>,4>{{{0x0622,0xfef5},{0x0623,0xfef7},{0x0625,0xfef9},{0x0627,0xfefb}}}) {
    const auto alef=variant[0],isolated=variant[1],final=static_cast<char16_t>(isolated+1);const auto name=std::to_string(unsigned(alef));
    rows.push_back({"lam-alef-"+name,{0x0644,alef},{isolated},true});
    rows.push_back({"beh-lam-alef-"+name,{0x0628,0x0644,alef},{final,0xfe91},true});
    rows.push_back({"lam-alef-beh-"+name,{0x0644,alef,0x0628},{0xfe8f,isolated},true});
    rows.push_back({"lam-lam-alef-"+name,{0x0644,0x0644,alef},{final,0xfedf},true});
  }
  JoiningRows(rows);
}

TEST(CoverageXfaArabic, LamLamHehEmitsItsRealShaddaAndSuperscriptAlefGlyphs) {
  // CFGAS_TxtBreak::GetDisplayPos emits three glyphs for the second Lam in
  // Lam/Lam/Heh. Explicit presentation-form text supplies an independent
  // control without triggering that original-character special case.
  const std::vector<Row> rows={
    {"lam-lam-heh",u"\u0644\u0644\u0647",u"\ufedf\ufee0\u0651\u0670\ufeea",true},
    {"alef-lam-lam-heh",u"\u0627\u0644\u0644\u0647",u"\ufe8d\ufedf\ufee0\u0651\u0670\ufeea",true},
    {"beh-lam-lam-heh",u"\u0628\u0644\u0644\u0647",u"\ufe91\ufee0\ufee0\u0651\u0670\ufeea",true}};
  Artifacts artifacts;
  for(const auto& row:rows) {
    SCOPED_TRACE(row.label);const auto bytes=Fixture(Field("input",u"\u0628",.25f,.25f,220,64,true)+
      Field("presentation",row.visual,.25f,100.25f,220,64));
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),bytes,true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      Edit(s,row.logical);History(s,row.logical);images[backend]=s.Draw(kSide,kSide);
      if(library->name()=="native")NativeBidiObservation(CropRaster(images[backend],0,0,240,64),CropRaster(images[backend],0,100,240,64),artifacts,row.label);
      else Geometry(CropRaster(images[backend],0,0,240,64),CropRaster(images[backend],0,100,240,64),artifacts,row.label+library->name());
      Reopen(*library,s,row.logical,images[backend],artifacts,row.label,bytes,row.native_bidi);++backend;
    }
    if(row.native_bidi) {
      ObserveNativeOriginal(images,artifacts,row.label);
      HealthyControls(row.visual.empty()?u"\ufe90\ufe91":row.visual,artifacts,row.label);
    }
    else Compare(images[0],images[1],artifacts,row.label+"-engines");
  }
}

TEST(CoverageXfaArabic, ArabicIndicAndEuropeanNumbersKeepVisibleOrderBesideJoinedWords) {
  JoiningRows({
    {"arabic-then-european",u"\u0628\u0628 123",u"123 \ufe90\ufe91",true},
    {"european-then-arabic",u"123 \u0628\u0628",u"123 \ufe90\ufe91",true},
    {"arabic-indic",u"\u0628\u0628 \u0661\u0662\u0663",u"\u0661\u0662\u0663 \ufe90\ufe91",true},
    {"indic-then-arabic",u"\u0661\u0662\u0663 \u0628\u0628",u"\ufe90\ufe91 \u0661\u0662\u0663",true},
    {"arabic-decimal",u"\u0628\u0628 12.3",u"12.3 \ufe90\ufe91",true},
    {"arabic-comma",u"\u0628\u0628 1,234",u"1,234 \ufe90\ufe91",true}});
}

TEST(CoverageXfaArabic, OrdinaryCombiningMarksChangeInkWithoutBecomingMissingFontBoxes) {
  Artifacts artifacts;
  // No FC5E..FC62 assertions: the pinned font has no Unicode glyphs there.
  // These real single marks exercise negative-width/skipped-combination paths.
  for(char16_t mark:{char16_t(0x064b),char16_t(0x064c),char16_t(0x064d),char16_t(0x064e),char16_t(0x064f),
      char16_t(0x0650),char16_t(0x0651),char16_t(0x0652),char16_t(0x0653),char16_t(0x0654),char16_t(0x0655),char16_t(0x0670)}) {
    SCOPED_TRACE(unsigned(mark));const std::u16string value={0x0628,mark,0x0628};
    const auto bytes=Fixture(Field("input",u"\u0628",.25f,.25f,220,64,true));std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),bytes,true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      Edit(s,u"\u0628\u0628");const auto base=CropRaster(s.Draw(kSide,kSide),0,0,240,64);const auto base_ink=Ink(base);
      Edit(s,value);History(s,value);images[backend]=s.Draw(kSide,kSide);const auto marked=CropRaster(images[backend],0,0,240,64);const auto ink=Ink(marked);
      EXPECT_NE(marked.pixels,base.pixels);EXPECT_GT(ink.mass,base_ink.mass);
      // A zero-advance combining mark can overhang the base glyph's ink box.
      // Bound that added ink by the actual pinned mark outline, not by its
      // advance width or an assumed unchanged visible word width.
      const int overhang=static_cast<int>(std::ceil(Metrics(mark).width*kSize/2048))+1;
      EXPECT_GE(ink.left,base_ink.left-overhang);EXPECT_LE(ink.right,base_ink.right+overhang);
      EXPECT_LE(ink.bottom-ink.top+1,32);EXPECT_GT(ink.pixels,10u);
      Reopen(*library,s,value,images[backend],artifacts,"mark-"+std::to_string(unsigned(mark)),bytes,true);++backend;
    }
    ObserveNativeOriginal(images,artifacts,"mark-"+std::to_string(unsigned(mark)));
    HealthyControls(u"\ufe8f\ufe90\ufe91\ufe92",artifacts,"mark-"+std::to_string(unsigned(mark)));
  }
}

TEST(CoverageXfaArabic, JoiningAndDirectionalControlsRemainLogicalThroughSelectionReplacementAndHistory) {
  const std::vector<std::u16string> rows={u"\u0628\u200c\u0628",u"\u0628\u200d\u0628",u"\u200d\u0628",
    u"\u0628\u200d",u"\u0644\u200c\u0627",u"\u0644\u200d\u0627",u"\u0628\u200e\u0628",
    u"\u0628\u200f\u0628",u"\u202a\u0628\u0628 12\u202c",u"\u202b12 \u0628\u0628\u202c",
    u"\u202d\u0628\u0628\u202c",u"\u202e12 \u0628\u0628\u202c"};
  Artifacts artifacts;int row=0;
  for(const auto& value:rows) {
    SCOPED_TRACE(row);const bool native_bidi=row==4||row==5||row>=8;const auto bytes=Fixture(Field("input",u"\u0628",.25f,.25f,220,64,true));std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),bytes,true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      Edit(s,value);History(s,value);images[backend]=s.Draw(kSide,kSide);const auto ink=Ink(CropRaster(images[backend],0,0,240,64));
      EXPECT_GT(ink.mass,3000u);EXPECT_LT(ink.right-ink.left,160);EXPECT_LE(ink.bottom-ink.top,32);
      Reopen(*library,s,value,images[backend],artifacts,"control-"+std::to_string(row),bytes,native_bidi);++backend;
    }
    if(native_bidi) {
      ObserveNativeOriginal(images,artifacts,"control-"+std::to_string(row));
      HealthyControls(u"\ufe8f\ufefb123",artifacts,"control-"+std::to_string(row));
    }
    else Compare(images[0],images[1],artifacts,"control-engines-"+std::to_string(row));
    ++row;
  }
}

TEST(CoverageXfaArabic, RichArabicWordsShapeAcrossRealRtfRunsAndPreserveExplicitLineBreaks) {
  struct RichRow {std::string label,html;std::u16string logical,visual;int lines=1;bool native_bidi=true;};
  const std::vector<RichRow> rows={
    {"rich-pair","<p>"+Xml(u"\u0628\u0628")+"</p>",u"\u0628\u0628",u"\ufe90\ufe91"},
    {"rich-medial","<p><span>"+Xml(u"\u0628\u0628\u0628")+"</span></p>",u"\u0628\u0628\u0628",u"\ufe90\ufe92\ufe91"},
    {"rich-lam-alef","<p>"+Xml(u"\u0628\u0644\u0627")+"</p>",u"\u0628\u0644\u0627",u"\ufefc\ufe91"},
    {"rich-colored-words","<p><span style=\"color:red\">"+Xml(u"\u0628\u0628")+"</span> <span style=\"color:blue\">"+Xml(u"\u0628\u0628")+"</span></p>",u"\u0628\u0628 \u0628\u0628",u""},
    {"rich-break","<p>"+Xml(u"\u0628\u0628")+"<br/>"+Xml(u"\u0628\u0628")+"</p>",u"\u0628\u0628",u"",2},
    {"rich-paragraphs","<p>"+Xml(u"\u0628\u0628")+"</p><p>"+Xml(u"\u0628\u0628")+"</p>",u"\u0628\u0628",u"",2}};
  Artifacts artifacts;
  for(const auto& row:rows) {
    SCOPED_TRACE(row.label);const auto bytes=Fixture(Field("input",u"\u0628",.25f,.25f,220,44,true)+Rich(row.html,.25f,80.25f,220,80)+
        (row.visual.empty()?"":Singletons(row.visual,180.25f,.25f,true)));
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),bytes,true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      Edit(s,row.logical);History(s,row.logical);images[backend]=s.Draw(kSide,kSide);
      const auto rich=CropRaster(images[backend],0,80,240,80);EXPECT_GT(Ink(rich).pixels,10u);EXPECT_EQ(Lines(rich),row.lines);
      if(!row.visual.empty()) {
        if(library->name()=="native")NativeBidiObservation(CropRaster(rich,0,0,240,64),CropRaster(images[backend],0,180,240,64),artifacts,row.label);
        else Geometry(CropRaster(rich,0,0,240,64),CropRaster(images[backend],0,180,240,64),artifacts,row.label+library->name());
      }
      Reopen(*library,s,row.logical,images[backend],artifacts,row.label,bytes,row.native_bidi);++backend;
    }
    if(row.native_bidi) {
      ObserveNativeOriginal(images,artifacts,row.label);
      HealthyControls(row.visual.empty()?u"\ufe90\ufe91":row.visual,artifacts,row.label);
    }
    else Compare(images[0],images[1],artifacts,row.label+"-engines");
  }
}

TEST(CoverageXfaArabic, WrappedJoinedWordsMoveBetweenOneTwoThreeAndSixVisibleLinesWithoutChangingLogicalText) {
  const std::u16string word=u"\u0628\u0628";std::u16string value=word;for(int i=1;i<6;++i)value+=u" "+word;
  Artifacts artifacts;
  for(const auto& setting:std::array<std::array<int,2>,4>{{{200,1},{90,2},{60,3},{40,6}}}) {
    const auto width=setting[0],lines=setting[1];SCOPED_TRACE(width);
    const auto bytes=Fixture(Field("input",u"\u0628",.25f,.25f,width,160,true));std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),bytes,true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      Edit(s,value);History(s,value);images[backend]=s.Draw(kSide,kSide);const auto ink=CropRaster(images[backend],0,0,width+2,164);
      EXPECT_EQ(Lines(ink),lines);EXPECT_GT(Ink(ink).mass,10000u);
      Reopen(*library,s,value,images[backend],artifacts,"wrap-"+std::to_string(width),bytes,true);++backend;
    }
    ObserveNativeOriginal(images,artifacts,"wrap-"+std::to_string(width));
    HealthyControls(u"\ufe90\ufe91",artifacts,"wrap-"+std::to_string(width));
  }
}
}  // namespace
}  // namespace pdfoxium::integration
