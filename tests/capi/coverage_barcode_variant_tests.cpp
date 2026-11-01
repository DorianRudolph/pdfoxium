#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <cstdint>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;

enum class Pattern { Linear, Qr, DataMatrix, Pdf417, Blank };
struct BarcodeCase {
  std::string label, kind;
  std::u16string value;
  std::string options;
  Pattern pattern = Pattern::Linear;
  int width = 180, height = 120, module = 1;
  bool reload = false;
  bool native_full_base256_length = false;
};

std::string JsString(const std::u16string& value) {
  constexpr char hex[] = "0123456789abcdef";
  std::string quoted="\"";
  for (uint16_t ch : value) {
    quoted += "\\u";
    for (int shift : {12,8,4,0}) quoted += hex[(ch>>shift)&15];
  }
  return quoted+"\"";
}
std::u16string Repeated(const std::u16string& value, size_t count) {
  std::u16string result;
  for(size_t i=0;i<count;++i) result+=value;
  return result;
}
std::string Rectangle(const char* color, int height=20) {
  return "<draw w=\"20pt\" h=\""+std::to_string(height)+"pt\"><value><rectangle>"
      "<fill><color value=\""+color+"\"/></fill><edge presence=\"hidden\"/>"
      "</rectangle></value></draw>";
}
std::vector<uint8_t> BarcodeFixture(const BarcodeCase& c) {
  const auto width=std::to_string(c.width+40),height=std::to_string(c.height+80);
  auto options=c.options;
  if(options.find("textLocation=")==std::string::npos) options+=" textLocation=\"none\"";
  if(options.find("moduleWidth=")==std::string::npos) options+=" moduleWidth=\""+std::to_string(c.module)+"pt\"";
  if(options.find("moduleHeight=")==std::string::npos) options+=" moduleHeight=\""+std::to_string(c.module)+"pt\"";
  // No public API constructs XFA templates. All encoding, painting, interaction
  // and save/reload below uses public C API calls. Initializers supply the exact
  // encoder input again on load: native XML saving otherwise inserts whitespace
  // into <text>, a separately documented serializer defect, not a barcode mode.
  const auto xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
      "<subform name=\"root\" layout=\"tb\" locale=\"en_US\"><pageSet><pageArea name=\"page\">"
      "<occur min=\"1\" max=\"-1\"/><medium short=\""+width+"pt\" long=\""+height+"pt\"/>"
      "<contentArea w=\""+width+"pt\" h=\""+height+"pt\"/></pageArea></pageSet>"+
      Rectangle("255,0,0")+
      "<field name=\"barcode\" w=\""+std::to_string(c.width)+"pt\" h=\""+std::to_string(c.height)+"pt\">"
      "<bind match=\"none\"/><font typeface=\"Helvetica\" size=\"10pt\"/>"
      "<ui><barcode type=\""+c.kind+"\" "+options+"/></ui>"
      "<value><text>input</text></value>"+forms::Event("this.rawValue="+JsString(c.value)+";","initialize")+"</field>"
      "<field name=\"audit\" w=\"180pt\" h=\"20pt\"><bind match=\"none\"/><ui><textEdit/></ui>"
      "<font typeface=\"Helvetica\" size=\"10pt\"/><value><text/></value>"+
      forms::Event("var v=xfa.form.root.barcode.rawValue;this.rawValue=String(v===null?0:String(v).length);","initialize")+"</field>"+
      Rectangle("0,255,0")+"</subform></template>";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 "+width+" "+height+"] /Resources << /Font << /Helv 10 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 10 Tf 0 g) /DR << /Font << /Helv 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("",xml),
      PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
      PdfStream("","</xdp:xdp>"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"});
}

bool Dark(const Raster& image,int x,int y) {
  const auto at=(static_cast<size_t>(y)*image.width+x)*4;
  return image.pixels.at(at)<64 && image.pixels.at(at+1)<64 && image.pixels.at(at+2)<64;
}
void ExpectCell(const Raster& image,int x,int y,bool black) {
  const auto at=(static_cast<size_t>(y)*image.width+x)*4;
  for(int k=0;k<3;++k) {
    if(black) EXPECT_LT(image.pixels.at(at+k),64) << x << ',' << y;
    else EXPECT_GT(image.pixels.at(at+k),240) << x << ',' << y;
  }
}
struct Bounds {int left,top,right,bottom;size_t dark=0;};
Bounds InkBounds(const Raster& image,const BarcodeCase& c) {
  Bounds b{c.width,c.height,0,0};
  for(int y=0;y<c.height;++y)for(int x=0;x<c.width;++x)if(Dark(image,x,y+20)) {
    b.left=std::min(b.left,x);b.top=std::min(b.top,y);
    b.right=std::max(b.right,x+1);b.bottom=std::max(b.bottom,y+1);++b.dark;
  }
  return b;
}
void Inspect(const Raster& image,const BarcodeCase& c) {
  ASSERT_EQ(image.width,c.width+40);ASSERT_EQ(image.height,c.height+80);
  // The encoder must not damage the real red/green document landmarks.
  const auto red=(static_cast<size_t>(5)*image.width+5)*4;
  EXPECT_EQ(image.pixels[red],0);EXPECT_EQ(image.pixels[red+1],0);EXPECT_EQ(image.pixels[red+2],255);
  const auto green=(static_cast<size_t>(c.height+45)*image.width+5)*4;
  EXPECT_EQ(image.pixels[green],0);EXPECT_EQ(image.pixels[green+1],255);EXPECT_EQ(image.pixels[green+2],0);
  const auto b=InkBounds(image,c);
  if(c.pattern==Pattern::Blank){EXPECT_EQ(b.dark,0u);return;}
  EXPECT_GT(b.dark,20u);ASSERT_GT(b.right,b.left);ASSERT_GT(b.bottom,b.top);
  if(c.pattern==Pattern::Linear || c.pattern==Pattern::Pdf417) return;
  const int m=std::min(c.module,8),n=(b.right-b.left)/m;
  ASSERT_EQ(b.right-b.left,n*m);ASSERT_EQ(b.bottom-b.top,n*m);
  EXPECT_EQ(b.left,(c.width-(n+2)*m)/2+m);
  EXPECT_EQ(b.top,(c.height-(n+2)*m)/2+m);
  const auto cell=[&](int x,int y,bool black){ExpectCell(image,b.left+x*m+m/2,b.top+20+y*m+m/2,black);};
  if(c.pattern==Pattern::Qr) {
    ASSERT_GE(n,21);ASSERT_LE(n,177);EXPECT_EQ(n%4,1);
    for(const auto& corner:std::array<std::array<int,2>,3>{{{0,0},{n-7,0},{0,n-7}}})
      for(int y=0;y<7;++y)for(int x=0;x<7;++x)
        cell(corner[0]+x,corner[1]+y,x==0||x==6||y==0||y==6||(x>=2&&x<=4&&y>=2&&y<=4));
  } else {
    ASSERT_GE(n,10);ASSERT_LE(n,144);EXPECT_EQ(n%2,0);
    // ECC200's L-shaped finder and alternating timing borders are independent
    // of payload compaction, symbol size and Reed-Solomon block layout.
    for(int i=0;i<n;++i){cell(0,i,true);cell(i,n-1,true);cell(i,0,i%2==0);cell(n-1,i,i%2==1);}
  }
  // Quiet modules outside the symbol retain the white background.
  ExpectCell(image,b.left-1,b.top+20,false);
  ExpectCell(image,b.right,b.bottom+19,false);
}
Raster Observe(Session& s,const BarcodeCase& c) {
  EXPECT_EQ(s.api.FPDF_GetPageCount(s.document),1);
  EXPECT_FLOAT_EQ(s.api.FPDF_GetPageWidthF(s.page),c.width+40.f);
  if(!s.Focus(5,c.height+25)){ADD_FAILURE()<<"Audit field is not focusable";return {};}
  const auto number=std::to_string(c.value.size());
  EXPECT_EQ(s.Text(),std::u16string(number.begin(),number.end()));
  s.api.FORM_ForceToKillFocus(s.form);
  auto image=s.Draw(c.width+40,c.height+80);Inspect(image,c);
  EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
  return image;
}
void CompareScene(const Raster& a,const Raster& b,const BarcodeCase& c,
                  Artifacts& artifacts,const std::string& label) {
  const bool labels=c.options.find("textLocation=")!=std::string::npos;
  CompareRaster(CropRaster(a,0,20,c.width,c.height),CropRaster(b,0,20,c.width,c.height),artifacts,label+"-modules",
      labels?RasterTolerance{18,0,0.3}:RasterTolerance{});
  CompareRaster(a,b,artifacts,label,RasterTolerance{18,0,0.1});
}

uint8_t DmMultiply(uint8_t a,uint8_t b) {
  // Test-only polynomial multiplication, independent of the encoder's tables.
  unsigned x=a,y=b,value=0;
  while(y) {if(y&1)value^=x;y>>=1;x<<=1;if(x&256)x^=0x12d;}
  return static_cast<uint8_t>(value);
}
void InspectFullBase256Symbol(const Raster& image,const BarcodeCase& c,bool native) {
  const auto bounds=InkBounds(image,c);
  const int side=bounds.right-bounds.left, data=side-2;
  ASSERT_EQ(side,c.value.size()==3?14:16);
  ASSERT_EQ(c.module,1);
  // Inverse ECC200 placement for these two single-region symbols. Coordinates
  // are Utah anchors in the data area; the sentinel is corner1 (12x12) or
  // corner2 (14x14). This reads the rendered bits, not encoder internals.
  std::vector<std::pair<int,int>> anchors={{4,0},{2,2},{0,4},{1,7},{3,5},{5,3},
      {7,1},{-1,-1},{10,2},{8,4},{6,6},{4,8},{2,10}};
  if(data==12) anchors.insert(anchors.end(),{{5,11},{7,9},{9,7},{11,5},{10,10}});
  else anchors.insert(anchors.end(),{{3,13},{5,11},{7,9},{9,7},{11,5},{13,3},
      {12,8},{10,10},{8,12},{11,13},{13,11}});
  std::vector<uint8_t> words;
  for(const auto& [row,col]:anchors) {
    std::array<std::pair<int,int>,8> cells;
    if(row==-1 && data==12)
      cells={{{data-1,0},{data-1,1},{data-1,2},{0,data-2},{0,data-1},
              {1,data-1},{2,data-1},{3,data-1}}};
    else if(row==-1)
      cells={{{data-3,0},{data-2,0},{data-1,0},{0,data-4},{0,data-3},
              {0,data-2},{0,data-1},{1,data-1}}};
    else cells={{{row-2,col-2},{row-2,col-1},{row-1,col-2},{row-1,col-1},
                 {row-1,col},{row,col-2},{row,col-1},{row,col}}};
    uint8_t word=0;
    for(auto [y,x]:cells) {
      if(y<0){y+=data;x+=4-(data+4)%8;}
      if(x<0){x+=data;y+=4-(data+4)%8;}
      ASSERT_GE(x,0);ASSERT_LT(x,data);ASSERT_GE(y,0);ASSERT_LT(y,data);
      const int px=bounds.left+x+1,py=bounds.top+20+y+1;
      const bool black=Dark(image,px,py);ExpectCell(image,px,py,black);
      word=static_cast<uint8_t>((word<<1)|black);
    }
    words.push_back(word);
  }
  if(data==14)for(int y=0;y<2;++y)for(int x=0;x<2;++x)
    ExpectCell(image,bounds.left+data-1+x,bounds.top+20+data-1+y,x==y);
  const int capacity=data==12?8:12,errors=data==12?10:12;
  ASSERT_EQ(words.size(),static_cast<size_t>(capacity+errors));
  ASSERT_EQ(words[0],231);  // Base256 latch.
  const auto unrandomize=[&](int index){return (int(words[index])-((149*(index+1))%255+1)+256)%256;};
  // Zero means all remaining data bytes. Native's decimal-first-digit shortcut
  // writes explicit6 (also valid) for six bytes, but1 (wrong) for ten bytes.
  const int encoded_length=unrandomize(1);
  EXPECT_EQ(encoded_length,native?(c.value.size()==3?6:1):0);
  const int decoded_length=encoded_length==0?capacity-2:encoded_length;
  EXPECT_EQ(decoded_length,native&&c.value.size()==5?1:static_cast<int>(c.value.size()*2));
  for(int i=2;i<capacity;++i) EXPECT_EQ(unrandomize(i),i%2==0?0xc3:0xbf);
  // Every data/ECC codeword is read. ECC200's generator roots are alpha^1..k
  // in GF(256) with primitive polynomial0x12d. All syndromes must vanish;
  // this independently checks every remaining rendered parity bit.
  uint8_t root=1;
  for(int k=0;k<errors;++k) {
    root=DmMultiply(root,2);uint8_t syndrome=0;
    for(uint8_t word:words) syndrome=DmMultiply(syndrome,root)^word;
    EXPECT_EQ(syndrome,0)<<"ECC200 syndrome "<<k;
  }
}
void CompareBase256Boundary(const Raster& native,const Raster& rust,const BarcodeCase& c,
                           Artifacts& artifacts) {
  artifacts.Image(c.label+"-native-original",native);artifacts.Image(c.label+"-rust-original",rust);
  const auto a=InkBounds(native,c),b=InkBounds(rust,c);
  ASSERT_EQ(a.left,b.left);ASSERT_EQ(a.top,b.top);ASSERT_EQ(a.right,b.right);ASSERT_EQ(a.bottom,b.bottom);
  const int x=a.left+1,y=a.top+21,w=a.right-a.left-2,h=a.bottom-a.top-2;
  // Only the independently decoded data area differs. Compare actual original
  // pixels everywhere else, including every finder/timing/quiet-area pixel.
  CompareRaster(CropRaster(native,0,0,native.width,y),CropRaster(rust,0,0,rust.width,y),artifacts,c.label+"-above-data",RasterTolerance{18,0,0.1});
  CompareRaster(CropRaster(native,0,y,x,h),CropRaster(rust,0,y,x,h),artifacts,c.label+"-left-data");
  CompareRaster(CropRaster(native,x+w,y,native.width-x-w,h),CropRaster(rust,x+w,y,rust.width-x-w,h),artifacts,c.label+"-right-data");
  CompareRaster(CropRaster(native,0,y+h,native.width,native.height-y-h),CropRaster(rust,0,y+h,rust.width,rust.height-y-h),artifacts,c.label+"-below-data",RasterTolerance{18,0,0.1});
}
void RunCases(const std::vector<BarcodeCase>& cases) {
  Artifacts artifacts;
  for(const auto& c:cases) {
    SCOPED_TRACE(c.label);
    SCOPED_TRACE(c.kind);
    SCOPED_TRACE(c.options);
    const auto bytes=BarcodeFixture(c);std::array<Raster,2> images;size_t backend=0;
    for(const auto* writer:Implementations()) {
      SCOPED_TRACE(writer->name());Session s(writer->api(),bytes,true,true);
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      images[backend]=Observe(s,c);
      if(c.native_full_base256_length) InspectFullBase256Symbol(images[backend],c,writer->name()=="native");
      if(c.reload) {
        const auto saved=SaveDocument(s.api,s.document,artifacts,c.label+writer->name());
        ASSERT_FALSE(saved.empty());
        for(const auto* reader:Implementations()) {
          SCOPED_TRACE(reader->name());Session r(reader->api(),saved,true,true);
          ASSERT_TRUE(r.valid());ASSERT_TRUE(r.xfa_loaded);
          CompareScene(images[backend],Observe(r,c),c,artifacts,c.label+writer->name()+reader->name());
        }
      }
      ++backend;
    }
    if(c.native_full_base256_length) CompareBase256Boundary(images[0],images[1],c,artifacts);
    else CompareScene(images[0],images[1],c,artifacts,c.label);
  }
}

TEST(CoverageBarcodeVariants, QrErrorCorrectionModesRemaindersAndVersionBoundariesRetainFinders) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::vector<BarcodeCase> cases;
  const std::vector<std::u16string> values={u"",u"1",u"12",u"123",u"ABCD123",u"ABCD1234",u"byte!? lower",u"\u00e9\u03a9\u4e2d\u6587",u"\u65e5\u672c\u8a9e"};
  for(int ec=0;ec<4;++ec)for(size_t i=0;i<values.size();++i)
    cases.push_back({"qr-mode-"+std::to_string(ec)+"-"+std::to_string(i),"QRCode",values[i],
      "errorCorrectionLevel=\""+std::to_string(ec)+"\"",Pattern::Qr,180,180,1,ec==3&&i==6});
  for(int ec=0;ec<4;++ec)for(int length:{100,400})
    cases.push_back({"qr-version-"+std::to_string(ec)+"-"+std::to_string(length),"QRCode",Repeated(u"a",length),
      "errorCorrectionLevel=\""+std::to_string(ec)+"\"",Pattern::Qr,180,180,1,ec==0&&length==400});
  RunCases(cases);
}

TEST(CoverageBarcodeVariants, DataMatrixCompactionRemaindersExtendedBytesAndLargeSymbolsRetainEcc200Borders) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::vector<BarcodeCase> cases;
  const std::vector<std::u16string> roots={u"ABCXYZ",u"abcxyz",u"ABC>\r*",u"@^?>+!",u"123456",u"\u00ff\u00c0\u00e9"};
  for(size_t mode=0;mode<roots.size();++mode)for(int tail=0;tail<3;++tail)
    cases.push_back({"dm-mode-"+std::to_string(mode)+"-"+std::to_string(tail),"dataMatrix",
      Repeated(roots[mode],6)+std::u16string(tail,u'A'),"",Pattern::DataMatrix,180,180,1,mode==3&&tail==2});
  for(int length:{1,2,3,7,12,21,40,100,300,800})
    cases.push_back({"dm-size-"+std::to_string(length),"dataMatrix",Repeated(u"a",length),"",Pattern::DataMatrix,180,180,1,length==300});
  for(int length:{1,2,3,4,5,6,7,50,200,750})
    cases.push_back({"dm-base256-"+std::to_string(length),"dataMatrix",Repeated(u"\u00ff",length),"",Pattern::DataMatrix,180,180,1,false,length==3||length==5});
  cases.push_back({"dm-macro-05","dataMatrix",u"[)>\x1e" u"05ABC123\x1d\x04","",Pattern::DataMatrix,180,180});
  cases.push_back({"dm-macro-06","dataMatrix",u"[)>\x1e" u"06abc123\x1d\x04","",Pattern::DataMatrix,180,180});
  RunCases(cases);
}

TEST(CoverageBarcodeVariants, Pdf417AllCorrectionLevelsAndTextNumericByteCompactionRender) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::vector<BarcodeCase> cases;
  const std::vector<std::u16string> values={u"ALPHA lower 12345;:!? end",u"12345678901234567890123456789012345678901234567890",u"\u00e9\u00ff\u00c0\u00f6\u00fc\u00df\u4e2d"};
  for(int ec=0;ec<=8;++ec)for(size_t i=0;i<values.size();++i)
    cases.push_back({"pdf417-ec-"+std::to_string(ec)+"-"+std::to_string(i),"pdf417",values[i],
      "errorCorrectionLevel=\""+std::to_string(ec)+"\"",Pattern::Pdf417,540,720,1,ec==8&&i==0});
  for(int length:{1,5,6,7,12,13,44,45}) {
    cases.push_back({"pdf417-digits-"+std::to_string(length),"pdf417",Repeated(u"1",length),"",Pattern::Pdf417,360,240});
    cases.push_back({"pdf417-bytes-"+std::to_string(length),"pdf417",Repeated(u"\u00ff",length),"",Pattern::Pdf417,360,240});
  }
  RunCases(cases);
}

TEST(CoverageBarcodeVariants, OneDimensionalChecksumsDelimitersAndNarrowRatiosChangeActualBars) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::vector<BarcodeCase> cases;
  for(const std::string ratio:{"2","3","6:2","1","8:0"})for(const std::string checksum:{"none","auto"}) {
    const auto options="wideNarrowRatio=\""+ratio+"\" checksum=\""+checksum+"\"";
    cases.push_back({"code39-"+ratio+checksum,"code3Of9",u"ABC 123-$%./+",options,Pattern::Linear});
    cases.push_back({"codabar-"+ratio+checksum,"codabar",u"123456-$:/.+",options+" dataLength=\"20\" startChar=\"T\" endChar=\"N\"",Pattern::Linear});
  }
  for(const std::string delimiters:{"AD","BC","TN","*E","ad","!Z"})
    cases.push_back({"codabar-delimiters-"+delimiters,"codabar",u"1234",
      "startChar=\""+delimiters.substr(0,1)+"\" endChar=\""+delimiters.substr(1,1)+"\"",Pattern::Linear,180,70,1,delimiters=="TN"});
  RunCases(cases);
}

TEST(CoverageBarcodeVariants, HumanReadableLocationsDrawAboveBelowAndInsideTheBars) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  for(const std::string kind:{"code128B","code3Of9"}) {
    BarcodeCase bars{"human-none",kind,u"ABC 123","",Pattern::Linear,180,70};
    // Native's OneDimWriter has a no-space shortcut that omits all labels.
    // A real space intentionally takes its normal text-rendering path here.
    std::array<Raster,2> none;size_t index=0;
    for(const auto* lib:Implementations()) {
      Session s(lib->api(),BarcodeFixture(bars),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      none[index++]=Observe(s,bars);
    }
    for(const std::string location:{"above","below","aboveEmbedded","belowEmbedded"}) {
      auto c=bars;c.label=kind+location;c.options="textLocation=\""+location+"\"";
      std::array<Raster,2> text;index=0;
      for(const auto* lib:Implementations()) {
        Session s(lib->api(),BarcodeFixture(c),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
        text[index]=Observe(s,c);
        EXPECT_NE(CropRaster(none[index],0,20,180,70).pixels,CropRaster(text[index],0,20,180,70).pixels);
        const bool above=location=="above"||location=="aboveEmbedded";
        const int strip=above?20:79;
        const auto letters=CropRaster(text[index],0,strip,180,11);
        size_t ink=0;for(size_t at=0;at<letters.pixels.size();at+=4) ink+=letters.pixels[at]<128;
        EXPECT_GT(ink,10u);
        CompareRaster(CropRaster(none[index],0,above?31:20,180,59),CropRaster(text[index],0,above?31:20,180,59),artifacts,c.label+lib->name()+"-unchanged-bars");
        ++index;
      }
      CompareScene(text[0],text[1],c,artifacts,c.label);
    }
  }
}

std::u16string PaddedEan(std::u16string value,size_t digits) {
  if(value.size()>digits) {value.resize(digits+1);return value;}
  value.insert(value.begin(),digits-value.size(),u'0');
  int sum=0,weight=3;
  for(auto at=value.rbegin();at!=value.rend();++at){sum+=(*at-u'0')*weight;weight=4-weight;}
  value.push_back(static_cast<char16_t>(u'0'+(10-sum%10)%10));return value;
}
TEST(CoverageBarcodeVariants, EanPaddingCheckDigitsTruncationAndCode128OddPairsMatchExplicitInputs) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  for(const auto& kind:std::array<std::pair<const char*,int>,3>{{{"ean8",7},{"ean13",12},{"upcA",11}}})
    for(const auto& value:std::vector<std::u16string>{u"5",u"12345",Repeated(u"9",kind.second),Repeated(u"3",kind.second+5)}) {
      BarcodeCase a{"ean-input",kind.first,value,"dataLength=\"37\"",Pattern::Linear,190,70};
      auto b=a;b.label="ean-explicit";b.value=PaddedEan(value,kind.second);
      std::array<Raster,2> images;size_t index=0;
      for(const auto* lib:Implementations()) {
        Session s(lib->api(),BarcodeFixture(a),true,true),control(lib->api(),BarcodeFixture(b),true,true);
        ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);ASSERT_TRUE(control.valid());ASSERT_TRUE(control.xfa_loaded);
        images[index]=Observe(s,a);const auto other=Observe(control,b);
        CompareRaster(CropRaster(images[index],0,20,a.width,a.height),CropRaster(other,0,20,a.width,a.height),artifacts,std::string(kind.first)+lib->name()+"-padding");
        ++index;
      }
      CompareScene(images[0],images[1],a,artifacts,kind.first);
    }
  for(const auto& value:std::vector<std::u16string>{u"1",u"123",u"12345",u"1234567"}) {
    BarcodeCase a{"code128-odd","code128C",value,"",Pattern::Linear,180,70};auto b=a;b.value+=u'0';
    for(const auto* lib:Implementations()) {
      Session s(lib->api(),BarcodeFixture(a),true,true),control(lib->api(),BarcodeFixture(b),true,true);
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);ASSERT_TRUE(control.valid());ASSERT_TRUE(control.xfa_loaded);
      CompareRaster(CropRaster(Observe(s,a),0,20,a.width,a.height),CropRaster(Observe(control,b),0,20,a.width,a.height),artifacts,lib->name()+"-odd-pair");
    }
  }
}

TEST(CoverageBarcodeVariants, ModuleBoundsAndRejectedCorrectionOptionsKeepTheDefaultEncoding) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  for(const auto& kind:std::array<std::pair<const char*,Pattern>,3>{{{"QRCode",Pattern::Qr},{"dataMatrix",Pattern::DataMatrix},{"pdf417",Pattern::Pdf417}}}) {
    BarcodeCase normal{"module-default",kind.first,u"123456789","",kind.second,220,180};
    std::array<Raster,2> expected;size_t index=0;
    for(const auto* lib:Implementations()) {
      Session s(lib->api(),BarcodeFixture(normal),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);expected[index++]=Observe(s,normal);
    }
    for(const std::string bad:{"moduleWidth=\"0pt\" moduleHeight=\"0pt\"","moduleWidth=\"11pt\" moduleHeight=\"11pt\"","errorCorrectionLevel=\"-1\"","errorCorrectionLevel=\"99\""}) {
      auto c=normal;c.options=bad;c.label=kind.first+bad;index=0;
      auto control=normal;
      const bool correction=bad.find("errorCorrectionLevel=")!=std::string::npos;
      // Omitted XFA CData is parsed as0; a rejected explicit level instead
      // retains the new writer's level1. Compare that path with explicit1.
      if(correction)control.options="errorCorrectionLevel=\"1\"";
      for(const auto* lib:Implementations()) {
        Session s(lib->api(),BarcodeFixture(c),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
        if(correction) {
          Session reference(lib->api(),BarcodeFixture(control),true,true);
          ASSERT_TRUE(reference.valid());ASSERT_TRUE(reference.xfa_loaded);
          CompareScene(Observe(reference,control),Observe(s,c),c,artifacts,c.label+lib->name());
          ++index;
        } else CompareScene(expected[index++],Observe(s,c),c,artifacts,c.label+lib->name());
      }
    }
  }
  std::vector<BarcodeCase> cases;
  for(const auto& kind:std::array<std::pair<const char*,Pattern>,2>{{{"QRCode",Pattern::Qr},{"dataMatrix",Pattern::DataMatrix}}})
    for(int module:{2,3,8,10})cases.push_back({"module-"+std::to_string(module)+kind.first,kind.first,u"A","",kind.second,220,220,module,module==8});
  RunCases(cases);
}

TEST(CoverageBarcodeVariants, InvalidDataAndCapacityFailuresLeaveTheRestOfThePageUsable) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::vector<BarcodeCase> cases={
    {"code39-invalid","code3Of9",u"lowercase","",Pattern::Blank},
    {"code128-quote","code128B",u"a\"b","",Pattern::Blank},
    {"ean-invalid","ean8",u"123x4567","",Pattern::Blank},
    {"codabar-invalid","codabar",u"ABC","",Pattern::Blank},
    {"qr-over-capacity","QRCode",Repeated(u"x",4000),"errorCorrectionLevel=\"3\"",Pattern::Blank,180,180},
    {"dm-over-capacity","dataMatrix",Repeated(u"X",5000),"",Pattern::Blank,180,180},
    {"pdf417-over-capacity","pdf417",Repeated(u"X",5000),"",Pattern::Blank,360,240},
    {"unsupported-kind","code93",u"123456","",Pattern::Blank},
    {"empty-ean","ean8",u"","",Pattern::Blank}};
  RunCases(cases);
}

TEST(CoverageBarcodeVariants, EditingInvalidLinearValuesClearsTheCachedBarsAndValidInputRestoresThem) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  for(const auto& item:std::array<std::array<std::u16string,3>,4>{{
      {{u"code3Of9",u"ABC 12",u"lowercase"}},{{u"code128B",u"AB12",u"a\"b"}},
      {{u"ean8",u"12345670",u"invalid"}},{{u"codabar",u"1234",u"ABC"}}}}) {
    BarcodeCase c{"edit-recovery",std::string(item[0].begin(),item[0].end()),item[1],"",Pattern::Linear,180,70};
    std::array<std::vector<Raster>,2> frames;size_t backend=0;
    for(const auto* lib:Implementations()) {
      Session s(lib->api(),BarcodeFixture(c),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      const auto before=Observe(s,c);frames[backend].push_back(before);
      ASSERT_TRUE(s.Focus(5,25));EXPECT_EQ(s.Text(),item[1]);
      ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(item[2]);EXPECT_EQ(s.Text(),item[2]);
      s.api.FORM_ForceToKillFocus(s.form);
      auto blank=c;blank.pattern=Pattern::Blank;const auto failed=s.Draw(c.width+40,c.height+80);Inspect(failed,blank);frames[backend].push_back(failed);
      ASSERT_TRUE(s.Focus(5,25));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(item[1]);
      EXPECT_EQ(s.Text(),item[1]);s.api.FORM_ForceToKillFocus(s.form);
      const auto restored=s.Draw(c.width+40,c.height+80);Inspect(restored,c);frames[backend].push_back(restored);
      CompareScene(before,restored,c,artifacts,c.kind+lib->name()+"-restored");
      ++backend;
    }
    for(size_t i=0;i<frames[0].size();++i)CompareScene(frames[0][i],frames[1][i],c,artifacts,c.kind+std::to_string(i));
  }
}

TEST(CoverageBarcodeVariants, EanHumanReadableGroupsPaintDigitsWithoutChangingTheirGuardBars) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  for(const auto& item:std::array<std::pair<const char*,std::u16string>,3>{{
      {"ean8",u"12345670"},{"ean13",u"5901234123457"},{"upcA",u"042100005264"}}}) {
    BarcodeCase bars{"ean-no-label",item.first,item.second,"",Pattern::Linear,190,70};
    auto labeled=bars;labeled.label="ean-embedded-label";labeled.options="textLocation=\"belowEmbedded\"";
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* lib:Implementations()) {
      SCOPED_TRACE(lib->name());
      SCOPED_TRACE(item.first);
      Session original(lib->api(),BarcodeFixture(bars),true,true),with_text(lib->api(),BarcodeFixture(labeled),true,true);
      ASSERT_TRUE(original.valid());ASSERT_TRUE(original.xfa_loaded);ASSERT_TRUE(with_text.valid());ASSERT_TRUE(with_text.xfa_loaded);
      const auto plain=Observe(original,bars);images[backend]=Observe(with_text,labeled);
      artifacts.Image(std::string(item.first)+lib->name()+"-label-original",images[backend]);
      // EAN digit groups occupy only the bottom font-size+1 rows. Their
      // left/right and center guards above the human label remain identical.
      CompareRaster(CropRaster(plain,0,20,190,59),CropRaster(images[backend],0,20,190,59),artifacts,std::string(item.first)+lib->name()+"-guards");
      const auto a=CropRaster(plain,0,79,190,11),b=CropRaster(images[backend],0,79,190,11);
      if(lib->name()=="native") {
        // Source BC_OneDimWriter::RenderDeviceResult incorrectly skips
        // ShowChars when contents contain no space. EAN preprocessing always
        // supplies digits, so the pinned native result remains bars-only.
        CompareRaster(a,b,artifacts,std::string(item.first)+"-native-no-space-shortcut");
      } else {
        EXPECT_NE(a.pixels,b.pixels);
        size_t different_rows=0;
        for(int y=1;y<11;++y) {
          size_t changed=0;
          // Ten-point LCD glyph strokes include partial-coverage gray pixels.
          // Classify text ink at the same200 threshold used by other form
          // geometry checks; barcode module assertions remain black/white64.
          const auto ink=[&](int x,int row){const auto at=(static_cast<size_t>(row)*b.width+x)*4;
            return b.pixels[at]<200&&b.pixels[at+1]<200&&b.pixels[at+2]<200;};
          for(int x=0;x<190;++x) changed+=ink(x,y)!=ink(x,0);
          different_rows+=changed>3;
        }
        EXPECT_GT(different_rows,2u)<<"Human-readable digits require glyph-shaped rows";
      }
      ++backend;
    }
    CompareRaster(CropRaster(images[0],0,0,230,79),CropRaster(images[1],0,0,230,79),artifacts,std::string(item.first)+"-above-label");
    CompareRaster(CropRaster(images[0],0,90,230,60),CropRaster(images[1],0,90,230,60),artifacts,std::string(item.first)+"-below-label",RasterTolerance{18,0,0.1});
  }
}

}  // namespace
}  // namespace pdfoxium::integration
