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
  int minimum_qr_version = 0;
  int first_data_word = 0;
  int rotated_pdf_width = 0;
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
std::vector<uint8_t> BarcodeFixture(const BarcodeCase& c, bool leading_xml_space=false, bool utf8_boundary=false) {
  const auto width=std::to_string(c.width+40),height=std::to_string(c.height+80);
  auto options=c.options;
  if(options.find("textLocation=")==std::string::npos) options+=" textLocation=\"none\"";
  if(options.find("moduleWidth=")==std::string::npos) options+=" moduleWidth=\""+std::to_string(c.module)+"pt\"";
  if(options.find("moduleHeight=")==std::string::npos) options+=" moduleHeight=\""+std::to_string(c.module)+"pt\"";
  // No public API constructs XFA templates. All encoding, painting, interaction
  // and save/reload below uses public C API calls. Initializers supply the exact
  // encoder input again on load: native XML saving otherwise inserts whitespace
  // into <text>, a separately documented serializer defect, not a barcode mode.
  const std::string preamble="<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
  const std::string config="<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
  const auto initialize=utf8_boundary?std::string("this.rawValue=\"\xc3\xa9\";"):
      "this.rawValue="+JsString(c.value)+";";
  std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
      "<subform name=\"root\" layout=\"tb\" locale=\"en_US\"><pageSet><pageArea name=\"page\">"
      "<occur min=\"1\" max=\"-1\"/><medium short=\""+width+"pt\" long=\""+height+"pt\"/>"
      "<contentArea w=\""+width+"pt\" h=\""+height+"pt\"/></pageArea></pageSet>"+
      Rectangle("255,0,0")+
      "<field name=\"barcode\" w=\""+std::to_string(c.width)+"pt\" h=\""+std::to_string(c.height)+"pt\">"
      "<bind match=\"none\"/><font typeface=\"Helvetica\" size=\"10pt\"/>"
      "<ui><barcode type=\""+c.kind+"\" "+options+"/></ui>"
      "<value><text>input</text></value>"+forms::Event(initialize,"initialize")+"</field>"
      "<field name=\"audit\" w=\"180pt\" h=\"20pt\"><bind match=\"none\"/><ui><textEdit/></ui>"
      "<font typeface=\"Helvetica\" size=\"10pt\"/><value><text/></value>"+
      forms::Event("var v=xfa.form.root.barcode.rawValue;v=v===null?\"\":String(v);var sum=0;for(var i=0;i<v.length;++i)sum+=v.charCodeAt(i);this.rawValue=v.length+\":\"+sum;","initialize")+"</field>"+
      Rectangle("0,255,0")+"</subform></template>";
  if(utf8_boundary) {
    const auto scalar=xml.find("\xc3\xa9");
    const auto statement=xml.find("this.rawValue=");
    // Native's decoder reads byte-sized chunks into the 1024-character plane.
    // Whitespace before the statement leaves its JavaScript value unchanged.
    const auto padding=(1023+1024-(preamble.size()+config.size()+scalar)%1024)%1024;
    xml.insert(statement,padding,' ');
  }
  if(leading_xml_space)xml.insert(0,1,' ');
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 "+width+" "+height+"] /Resources << /Font << /Helv 10 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 10 Tf 0 g) /DR << /Font << /Helv 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("",preamble),
      PdfStream("",config),
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
  if(c.pattern==Pattern::Linear) return;
  if(c.pattern==Pattern::Pdf417) {
    if(c.rotated_pdf_width) {
      EXPECT_EQ(b.right-b.left,c.rotated_pdf_width);
      ASSERT_EQ(b.bottom-b.top,1720);
      // A one-column PDF417 has86 modules across. Rotation makes its fixed
      // start/stop words horizontal stripes; the tall field scales each row20x.
      const int x=(b.left+b.right)/2, y=b.top+20;
      for(int i=0;i<17;++i)ExpectCell(image,x,y+i*20+10,(0x1fea8>>(16-i))&1);
      for(int i=0;i<18;++i)ExpectCell(image,x,y+(86-18+i)*20+10,(0x3fa29>>(17-i))&1);
    }
    return;
  }
  const int m=std::min(c.module,8),n=(b.right-b.left)/m;
  ASSERT_EQ(b.right-b.left,n*m);ASSERT_EQ(b.bottom-b.top,n*m);
  EXPECT_EQ(b.left,(c.width-(n+2)*m)/2+m);
  EXPECT_EQ(b.top,(c.height-(n+2)*m)/2+m);
  const auto cell=[&](int x,int y,bool black){ExpectCell(image,b.left+x*m+m/2,b.top+20+y*m+m/2,black);};
  if(c.pattern==Pattern::Qr) {
    ASSERT_GE(n,21);ASSERT_LE(n,177);EXPECT_EQ(n%4,1);
    EXPECT_GE((n-17)/4,c.minimum_qr_version);
    for(const auto& corner:std::array<std::array<int,2>,3>{{{0,0},{n-7,0},{0,n-7}}})
      for(int y=0;y<7;++y)for(int x=0;x<7;++x)
        cell(corner[0]+x,corner[1]+y,x==0||x==6||y==0||y==6||(x>=2&&x<=4&&y>=2&&y<=4));
  } else {
    ASSERT_GE(n,10);ASSERT_LE(n,144);EXPECT_EQ(n%2,0);
    if(c.first_data_word) {
      ASSERT_LE(n,26);  // These mode probes fit one ECC200 data region.
      const int side=n-2;
      const std::array<std::pair<int,int>,8> first={{{2,-2},{2,-1},{3,-2},{3,-1},{3,0},{4,-2},{4,-1},{4,0}}};
      unsigned word=0;
      for(auto [row,col]:first) {
        if(row<0){row+=side;col+=4-(side+4)%8;}
        if(col<0){col+=side;row+=4-(side+4)%8;}
        word=(word<<1)|Dark(image,b.left+(col+1)*m+m/2,b.top+20+(row+1)*m+m/2);
      }
      EXPECT_EQ(word,static_cast<unsigned>(c.first_data_word))<<"Expected real compaction latch";
    }
    // ECC200's L-shaped finder and alternating timing borders are independent
    // of payload compaction, symbol size and Reed-Solomon block layout.
    for(int i=0;i<n;++i){cell(0,i,true);cell(i,n-1,true);cell(i,0,i%2==0);cell(n-1,i,i%2==1);}
  }
  // Quiet modules outside the symbol retain the white background.
  ExpectCell(image,b.left-1,b.top+20,false);
  ExpectCell(image,b.right,b.bottom+19,false);
}
Raster Observe(Session& s,const BarcodeCase& c,Artifacts* artifacts=nullptr) {
  EXPECT_EQ(s.api.FPDF_GetPageCount(s.document),1);
  EXPECT_FLOAT_EQ(s.api.FPDF_GetPageWidthF(s.page),c.width+40.f);
  // Exercise initial rendering before interactive audit queries; XFA controls
  // may be instantiated lazily while their document view prepares the page.
  const auto initial=s.Draw(c.width+40,c.height+80);
  if(initial.pixels.empty()){ADD_FAILURE()<<"Initial barcode rendering failed";return {};}
  if(!s.Focus(5,c.height+25)) {
    if(artifacts)artifacts->Image(c.label+"-unfocused-audit",initial);
    const bool probe=s.Focus(5,25);
    s.api.FORM_ForceToKillFocus(s.form);
    ADD_FAILURE()<<"Post-load barcode probe focus="<<probe;
    for(const auto& alert:s.host->alerts) {
      const auto& message=std::get<0>(alert);
      ADD_FAILURE()<<"Layout diagnostic: "<<std::string(message.begin(),message.end());
    }
    ADD_FAILURE()<<"Audit field is not focusable; type="
      <<s.api.FPDFPage_HasFormFieldAtPoint(s.form,s.page,5,c.height+25);
    return {};
  }
  uint32_t sum=0;for(uint16_t ch:c.value)sum+=ch;
  const auto number=std::to_string(c.value.size())+":"+std::to_string(sum);
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

// The pinned parser misses a CDATA close split at its 1024-character plane.
// Retain the original valid input for Rust; a harmless leading XML space moves
// that close into the next plane and supplies a native rendering oracle.
void RunCdataPlaneBoundary(const BarcodeCase& c, Artifacts& artifacts) {
  const auto original=BarcodeFixture(c), aligned=BarcodeFixture(c,true);
  Session rust(libraries().rust.api(),original,true,true);
  ASSERT_TRUE(rust.valid());ASSERT_TRUE(rust.xfa_loaded);
  const auto correct=Observe(rust,c,&artifacts);ASSERT_FALSE(correct.pixels.empty());
  for(const auto* reader:Implementations()) {
    SCOPED_TRACE(reader->name());
    Session control(reader->api(),aligned,true,true);
    ASSERT_TRUE(control.valid());ASSERT_TRUE(control.xfa_loaded);
    const auto image=Observe(control,c,&artifacts);ASSERT_FALSE(image.pixels.empty());
    CompareScene(correct,image,c,artifacts,c.label+"-aligned-"+reader->name());
    const auto saved_control=SaveDocument(control.api,control.document,artifacts,c.label+"-aligned-"+reader->name());
    ASSERT_FALSE(saved_control.empty());
    for(const auto* reloader:Implementations()) {
      SCOPED_TRACE("aligned-save:"+reader->name()+"->"+reloader->name());
      Session reload(reloader->api(),saved_control,true,true);
      ASSERT_TRUE(reload.valid());ASSERT_TRUE(reload.xfa_loaded);
      const auto after=Observe(reload,c,&artifacts);ASSERT_FALSE(after.pixels.empty());
      CompareScene(correct,after,c,artifacts,c.label+"-aligned-save-"+reader->name()+"-"+reloader->name());
    }
  }
  const auto saved=SaveDocument(rust.api,rust.document,artifacts,c.label+"-rust");
  ASSERT_FALSE(saved.empty());
  {
    SCOPED_TRACE("original-rust-save->rust");
    // Saving preserves the original template packet, including its boundary.
    // Use the aligned fixture above for both native-reader save comparisons.
    Session reload(libraries().rust.api(),saved,true,true);
    ASSERT_TRUE(reload.valid());ASSERT_TRUE(reload.xfa_loaded);
    const auto image=Observe(reload,c,&artifacts);ASSERT_FALSE(image.pixels.empty());
    CompareScene(correct,image,c,artifacts,c.label+"-original-rust-reload");
  }

  // Establish the exact native failure independently: its initializer is lost,
  // the barcode retains "input", and the audit field is swallowed as CDATA.
  Session native(libraries().native.api(),original,true,true);
  ASSERT_TRUE(native.valid());ASSERT_TRUE(native.xfa_loaded);
  const auto broken=native.Draw(c.width+40,c.height+80);
  ASSERT_FALSE(broken.pixels.empty());
  ASSERT_TRUE(native.Focus(5,25));EXPECT_EQ(native.Text(),u"input");
  native.api.FORM_ForceToKillFocus(native.form);
  EXPECT_EQ(native.api.FPDFPage_HasFormFieldAtPoint(native.form,native.page,5,c.height+25),-1);
  const auto green=(static_cast<size_t>(c.height+25)*broken.width+5)*4;
  EXPECT_EQ(broken.pixels[green],0);EXPECT_EQ(broken.pixels[green+1],255);EXPECT_EQ(broken.pixels[green+2],0);
  BarcodeCase fallback=c;fallback.label+="-native-placeholder";
  fallback.value=u"input";fallback.first_data_word=106;
  Session placeholder(libraries().native.api(),BarcodeFixture(fallback,true),true,true);
  ASSERT_TRUE(placeholder.valid());ASSERT_TRUE(placeholder.xfa_loaded);
  const auto expected=Observe(placeholder,fallback,&artifacts);ASSERT_FALSE(expected.pixels.empty());
  CompareRaster(CropRaster(broken,0,20,c.width,c.height),CropRaster(expected,0,20,c.width,c.height),
                artifacts,c.label+"-native-lost-initializer",RasterTolerance{});
}

// Reopen selected examples with both readers. The explicit initializer supplies
// encoder inputs consistently across native's separately documented XML newline
// serialization behavior; these cases do not claim raw-value persistence.
void RunBoundaries(const std::vector<BarcodeCase>& cases) {
  Artifacts artifacts;
  for(const auto& c:cases) {
    SCOPED_TRACE(c.label);
    SCOPED_TRACE(c.kind);
    if(c.label=="edifact-10-end") {
      ASSERT_NO_FATAL_FAILURE(RunCdataPlaneBoundary(c,artifacts));
      continue;
    }
    const auto input=BarcodeFixture(c);std::array<Raster,2> images;size_t backend=0;
    for(const auto* writer:Implementations()) {
      SCOPED_TRACE(writer->name());
      Session s(writer->api(),input,true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      images[backend]=Observe(s,c,&artifacts);
      ASSERT_FALSE(images[backend].pixels.empty());
      if(c.reload) {
        const auto saved=SaveDocument(s.api,s.document,artifacts,c.label+writer->name());ASSERT_FALSE(saved.empty());
        for(const auto* reader:Implementations()) {
          SCOPED_TRACE(reader->name());
          Session r(reader->api(),saved,true,true);ASSERT_TRUE(r.valid());ASSERT_TRUE(r.xfa_loaded);
          const auto after=Observe(r,c,&artifacts);ASSERT_FALSE(after.pixels.empty());
          CompareScene(images[backend],after,c,artifacts,c.label+writer->name()+reader->name());
        }
      }
      ++backend;
    }
    if(c.label=="pdf417-both-smaller-than-symbol") {
      // At8px high the six20-module PDF417 rows are undersampled. The two
      // middle boundary rows hit the rectangle margin tie. Retain independent
      // exact goldens for both snapping policies, then compare every other
      // pixel normally. These are not ignored rows or relaxed AA tolerances.
      constexpr const char* expected[2][2]={
        {".#######.#.#.#..##.#.##########.#############..##.#.#########..######.#...##..#.",
         ".#######.#.#.#..##.#.###.#......####.#...#..##.####.#.##..###..######.#...##..#."},
        {".#######.#.#.#..##.#.#.####.....####.#.######..##.#.#.####.....######.#...##..#.",
         ".#######.#.#.#..##.#.##########.#######..#####.####.#########..######.#...##..#."},
      };
      for(size_t backend=0;backend<2;++backend)for(int row=0;row<2;++row) {
        auto& image=images[backend];ASSERT_EQ(image.width,120);ASSERT_EQ(image.height,88);
        ASSERT_EQ(std::char_traits<char>::length(expected[backend][row]),80u);
        for(int x=0;x<80;++x) {
          const size_t p=(size_t(row+23)*image.width+x)*4;
          const uint8_t color=expected[backend][row][x]=='#'?0:255;
          for(size_t channel=0;channel<3;++channel)ASSERT_EQ(image.pixels[p+channel],color)<<backend<<" "<<x<<","<<row+23;
          ASSERT_EQ(image.pixels[p+3],255);
          std::fill_n(image.pixels.begin()+p,4,255);
        }
      }
    }
    CompareScene(images[0],images[1],c,artifacts,c.label);
  }
}

TEST(CoverageBarcodeBoundaries, Utf8ScalarAcrossInputPlaneRetainsBarcodeValuePaintingAndReload) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  BarcodeCase c{"utf8-scalar-plane","dataMatrix",u"é","",Pattern::DataMatrix,180,180};
  const auto original=BarcodeFixture(c,false,true),aligned=BarcodeFixture(c,true,true);
  Session rust(libraries().rust.api(),original,true,true);
  ASSERT_TRUE(rust.valid());ASSERT_TRUE(rust.xfa_loaded);
  const auto correct=Observe(rust,c,&artifacts);ASSERT_FALSE(correct.pixels.empty());
  for(const auto* reader:Implementations()) {
    SCOPED_TRACE(reader->name());
    Session control(reader->api(),aligned,true,true);
    ASSERT_TRUE(control.valid());ASSERT_TRUE(control.xfa_loaded);
    const auto image=Observe(control,c,&artifacts);ASSERT_FALSE(image.pixels.empty());
    CompareScene(correct,image,c,artifacts,c.label+"-aligned-"+reader->name());
    const auto saved_control=SaveDocument(control.api,control.document,artifacts,c.label+"-aligned-"+reader->name());
    ASSERT_FALSE(saved_control.empty());
    for(const auto* reloader:Implementations()) {
      SCOPED_TRACE("aligned-save:"+reader->name()+"->"+reloader->name());
      Session reload(reloader->api(),saved_control,true,true);
      ASSERT_TRUE(reload.valid());ASSERT_TRUE(reload.xfa_loaded);
      const auto after=Observe(reload,c,&artifacts);ASSERT_FALSE(after.pixels.empty());
      CompareScene(correct,after,c,artifacts,c.label+"-aligned-save-"+reader->name()+"-"+reloader->name());
    }
  }
  const auto saved=SaveDocument(rust.api,rust.document,artifacts,c.label+"-rust");
  ASSERT_FALSE(saved.empty());
  {
    SCOPED_TRACE("original-rust-save->rust");
    // Saving preserves the original template packet, including its boundary.
    // Use the aligned fixture above for both native-reader save comparisons.
    Session reload(libraries().rust.api(),saved,true,true);
    ASSERT_TRUE(reload.valid());ASSERT_TRUE(reload.xfa_loaded);
    const auto image=Observe(reload,c,&artifacts);ASSERT_FALSE(image.pixels.empty());
    CompareScene(correct,image,c,artifacts,c.label+"-original-rust-reload");
  }
  // Pinned native consumes the UTF8 lead at the end of its first block and
  // discards the continuation on its next read, leaving an empty JS string.
  SCOPED_TRACE("native-empty-observation");
  BarcodeCase lost=c;lost.value.clear();lost.first_data_word=129;
  // HighLevelEncoder still creates the smallest ECC200 symbol for an empty
  // payload: its first data codeword is PAD (129), not an absent barcode.
  Session native(libraries().native.api(),original,true,true);
  ASSERT_TRUE(native.valid());ASSERT_TRUE(native.xfa_loaded);
  const auto empty_symbol=Observe(native,lost,&artifacts);ASSERT_FALSE(empty_symbol.pixels.empty());
  const auto bounds=InkBounds(empty_symbol,lost);
  EXPECT_EQ(bounds.right-bounds.left,10);EXPECT_EQ(bounds.bottom-bounds.top,10);
  lost.label+="-empty-control";
  Session empty_control(libraries().native.api(),BarcodeFixture(lost,true),true,true);
  ASSERT_TRUE(empty_control.valid());ASSERT_TRUE(empty_control.xfa_loaded);
  const auto expected=Observe(empty_control,lost,&artifacts);ASSERT_FALSE(expected.pixels.empty());
  CompareScene(empty_symbol,expected,lost,artifacts,c.label+"-native-empty-value");
}

TEST(CoverageBarcodeBoundaries, QrVersionsAboveTwentySixRetainCountFieldsFindersAndQuietModules) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::vector<BarcodeCase> cases;
  struct Row {const char* name;char16_t value;int length,level;};
  // Standard QR capacity ranges require versions27..40. One-point modules
  // still fit the maximum177x177 symbol plus its quiet modules in this field.
  for(const auto& row:std::array<Row,6>{{{"byte-h1000",u'a',1000,3},{"byte-h1200",u'b',1200,3},
      {"byte-m1700",u'c',1700,1},{"byte-l2600",u'd',2600,0},
      {"numeric-l6500",u'1',6500,0},{"alphanumeric-h1600",u'A',1600,3}}}) {
    BarcodeCase c{row.name,"QRCode",std::u16string(row.length,row.value),
        "errorCorrectionLevel=\""+std::to_string(row.level)+"\"",Pattern::Qr,180,180};
    c.minimum_qr_version=27;c.reload=std::string(row.name)=="byte-h1000";cases.push_back(c);
  }
  RunBoundaries(cases);
}

TEST(CoverageBarcodeBoundaries, DataMatrixC40AndTextShiftsKeepTheirActualCompactionMode) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::vector<BarcodeCase> cases;
  const std::vector<std::u16string> shifts={u" ",u"\t",u"!",u":",u"[",u"`",u"A",u"a",u"{",u"\x7f",u"\u00ff"};
  for(bool text:{false,true})for(size_t i=0;i<shifts.size();++i) {
    const auto prefix=Repeated(text?u"abc":u"ABC",4);
    BarcodeCase c{std::string(text?"text-shift-":"c40-shift-")+std::to_string(i),"dataMatrix",
      prefix+shifts[i]+prefix,"",Pattern::DataMatrix,180,180};
    c.first_data_word=text?239:230;c.reload=i==2;cases.push_back(c);
  }
  // Short tails vary the encoded remainder and available symbol capacity,
  // reaching genuine backtracking rather than directly constructing encoders.
  for(bool text:{false,true})for(int length:{10,11,13,14,16,17,19,20,22,23,25,26}) {
    BarcodeCase c{std::string(text?"text-tail-":"c40-tail-")+std::to_string(length),"dataMatrix",
      std::u16string(length,text?u'a':u'A')+u"!","",Pattern::DataMatrix,180,180};
    cases.push_back(c);
  }
  RunBoundaries(cases);
}

TEST(CoverageBarcodeBoundaries, EdifactSymbolCapacityTailsUnlatchAndResumeAscii) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::vector<BarcodeCase> cases;
  const auto source=Repeated(u"@^?>",9);
  for(int length:{9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31})
    for(bool ascii_tail:{false,true}) {
      BarcodeCase c{"edifact-"+std::to_string(length)+(ascii_tail?"-ascii":"-end"),"dataMatrix",
        source.substr(0,length)+(ascii_tail?u"xyz":u""),"",Pattern::DataMatrix,180,180};
      c.first_data_word=240;c.reload=length==19&&ascii_tail;cases.push_back(c);
    }
  RunBoundaries(cases);
}

TEST(CoverageBarcodeBoundaries, Pdf417ColumnOnlyRowOnlyTinyAndRotatedFieldsKeepVisibleModules) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::vector<BarcodeCase> cases;
  // With moduleWidth1, column count=(width-69)/17 and row count=height/20.
  // These deliberately select each caller's dimension-constraint branch.
  cases.push_back({"pdf417-columns-only","pdf417",u"ABCDEF123456","",Pattern::Pdf417,240,40,1,true});
  cases.push_back({"pdf417-rows-only","pdf417",Repeated(u"A",50),"",Pattern::Pdf417,60,120});
  cases.push_back({"pdf417-both-smaller-than-symbol","pdf417",u"ABCDEF","",Pattern::Pdf417,80,8});
  for(int length:{162,164}) {
    BarcodeCase c{"pdf417-rotated-"+std::to_string(length),"pdf417",std::u16string(length,u'A'),
      "errorCorrectionLevel=\"1\"",Pattern::Pdf417,90,1800};
    c.rotated_pdf_width=length/2+6;c.reload=length==162;cases.push_back(c);
  }
  RunBoundaries(cases);
}

TEST(CoverageBarcodeBoundaries, Pdf417ModeTransitionsHandleSingleBinaryShiftsAndNumericLookahead) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::vector<BarcodeCase> cases;
  const std::vector<std::u16string> values={u"ABCDE!FG",u"ABCDE;FG",u"abcde!fg",u"12345@XYZ",
    u"12345abcXYZ",u"12345;XYZ",u"ABCDE\x01" u"FGHIJ",u"AB\x01" u"1234567890123",
    u"AB\x01" u"abcdef",u"1234567890123ABCDE",u"ABCDE\tFGHIJ",u"ABCDE\r\nFGHIJ"};
  for(size_t i=0;i<values.size();++i) {
    BarcodeCase c{"pdf417-transition-"+std::to_string(i),"pdf417",values[i],"",Pattern::Pdf417,360,160};
    // A binary control is a valid PDF417 byte input but is not XML1.0 text;
    // keep that runtime-only case out of save/reload. Its initializer is XML.
    c.reload=i==0;cases.push_back(c);
  }
  RunBoundaries(cases);
}

TEST(CoverageBarcodeBoundaries, LinearLengthLimitsClearPreviousModulesAndAllowRecovery) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  struct Limit {const char* kind;int good,bad,width;};
  for(const auto& row:std::array<Limit,4>{{{"code3Of9",80,81,1500},{"code128B",80,81,1100},
      {"codabar",4,8193,180},{"ean8",8,8193,180}}}) {
    const char16_t ch=std::string(row.kind)=="codabar"||std::string(row.kind)=="ean8"?u'1':u'A';
    BarcodeCase c{std::string(row.kind)+"-length-boundary",row.kind,std::u16string(row.good,ch),"",Pattern::Linear,row.width,70};
    if(std::string(row.kind)=="ean8")c.value=u"12345670";
    std::array<std::vector<Raster>,2> frames;size_t backend=0;
    for(const auto* writer:Implementations()) {
      SCOPED_TRACE(writer->name());
      SCOPED_TRACE(row.kind);
      Session s(writer->api(),BarcodeFixture(c),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      frames[backend].push_back(Observe(s,c));
      ASSERT_TRUE(s.Focus(5,25));EXPECT_EQ(s.Text(),c.value);
      ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(std::u16string(row.bad,ch));
      EXPECT_EQ(s.Text().size(),static_cast<size_t>(row.bad));s.api.FORM_ForceToKillFocus(s.form);
      auto failed=c;failed.pattern=Pattern::Blank;frames[backend].push_back(s.Draw(c.width+40,c.height+80));Inspect(frames[backend].back(),failed);
      ASSERT_TRUE(s.Focus(5,25));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(c.value);
      EXPECT_EQ(s.Text(),c.value);s.api.FORM_ForceToKillFocus(s.form);
      frames[backend].push_back(s.Draw(c.width+40,c.height+80));Inspect(frames[backend].back(),c);
      CompareScene(frames[backend].front(),frames[backend].back(),c,artifacts,c.label+writer->name()+"-restored");
      const auto saved=SaveDocument(s.api,s.document,artifacts,c.label+writer->name());ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        Session r(reader->api(),saved,true,true);ASSERT_TRUE(r.valid());ASSERT_TRUE(r.xfa_loaded);
        CompareScene(frames[backend].back(),Observe(r,c),c,artifacts,c.label+writer->name()+reader->name());
      }
      ++backend;
    }
    for(size_t i=0;i<frames[0].size();++i)CompareScene(frames[0][i],frames[1][i],c,artifacts,c.label+std::to_string(i));
  }
}

}  // namespace
}  // namespace pdfoxium::integration
