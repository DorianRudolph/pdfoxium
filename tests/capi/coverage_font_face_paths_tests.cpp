// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "text_fixture.h"

#include <cstring>
#include <cmath>
#include <map>
#include <memory>

namespace pdfoxium::integration {
namespace {
using Bytes=std::vector<uint8_t>;
using Tables=std::map<uint32_t,Bytes>;
uint16_t U16(const Bytes& b,size_t p){return (uint16_t(b.at(p))<<8)|b.at(p+1);}
uint32_t U32(const Bytes& b,size_t p){return (uint32_t(U16(b,p))<<16)|U16(b,p+2);}
void W16(Bytes& b,size_t p,uint16_t v){b.at(p)=v>>8;b.at(p+1)=v;}
void W32(Bytes& b,size_t p,uint32_t v){W16(b,p,v>>16);W16(b,p+2,v);}
uint32_t Tag(const char* s){return (uint32_t(uint8_t(s[0]))<<24)|(uint32_t(uint8_t(s[1]))<<16)|(uint32_t(uint8_t(s[2]))<<8)|uint8_t(s[3]);}
uint32_t Sum(const Bytes& b){uint32_t out=0;for(size_t i=0;i<b.size();i+=4){uint32_t v=0;for(size_t j=0;j<4;++j)v=(v<<8)|(i+j<b.size()?b[i+j]:0);out+=v;}return out;}
Tables Split(const Bytes& b){Tables out;for(unsigned i=0;i<U16(b,4);++i){const size_t r=12+16*i,p=U32(b,r+8),n=U32(b,r+12);out[U32(b,r)]=Bytes(b.begin()+p,b.begin()+p+n);}return out;}
// Test-only directory assembly: outlines, instructions, metrics, names and all
// unrelated tables remain byte-identical to the pinned inputs. Head's checksum
// adjustment alone changes when an existing SFNT directory is reconstructed.
Bytes Assemble(Tables tables,bool otto=false){
  const auto head=tables.find(Tag("head"));if(head!=tables.end())W32(head->second,8,0);
  Bytes b(12+16*tables.size());W32(b,0,otto?Tag("OTTO"):0x10000);W16(b,4,tables.size());
  unsigned power=1,selector=0;while(power*2<=tables.size()){power*=2;++selector;}
  W16(b,6,power*16);W16(b,8,selector);W16(b,10,tables.size()*16-power*16);
  size_t i=0,head_at=0;
  for(const auto& [tag,data]:tables){while(b.size()%4)b.push_back(0);const auto r=12+16*i++,p=b.size();W32(b,r,tag);W32(b,r+4,Sum(data));W32(b,r+8,p);W32(b,r+12,data.size());if(tag==Tag("head"))head_at=p;b.insert(b.end(),data.begin(),data.end());}
  while(b.size()%4){b.push_back(0);}
  if(head_at){W32(b,head_at+8,0xb1b0afba-Sum(b));}
  return b;
}
Bytes OneMap(uint16_t encoding,uint16_t code,uint16_t glyph){
  Bytes b(44);W16(b,2,1);W16(b,4,3);W16(b,6,encoding);W32(b,8,12);
  // Complete format4 table with one mapped scalar and the required sentinel.
  for(const auto& [p,v]:std::map<size_t,uint16_t>{{12,4},{14,32},{18,4},{20,4},{22,1},{26,code},{28,0xffff},{32,code},{34,0xffff},{36,uint16_t(glyph-code)},{38,1}})W16(b,p,v);
  return b;
}
Bytes Hinted(bool required){return text_fixture::Read(TestRoot()/"crates/pdfoxium-core/tests/fixtures"/(required?"hint-reliant.ttf":"hint-optional.ttf"));}
Bytes Headless(const std::string& resource,bool map=false){Tables t{{Tag("CFF "),text_fixture::Resource(resource)}};if(map)t[Tag("cmap")]=OneMap(1,'A',9);return Assemble(t,true);}
std::string UnicodeMap(const std::string& code,const std::string& unicode){return "/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /FaceUnicode def /CMapType 2 def 1 begincodespacerange <0000> <ffff> endcodespacerange 1 beginbfchar <"+code+"> <"+unicode+"> endbfchar endcmap CMapName currentdict /CMap defineresource pop end end";}
Bytes Embedded(const Bytes& font,bool cff,const std::string& content="BT /F 40 Tf 30 100 Td (A) Tj ET",bool cid=false,const std::string& cmap="Identity-H",const std::string& encoded="0041",const std::string& unicode="0041",const std::string& ordering="Japan1"){
  std::string widths;for(int i=0;i<256;++i)widths+=cff?"600 ":"700 ";
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 240] /Resources << /Font << /F 5 0 R >> >> /Contents 4 0 R >>",PdfStream("",content),
    cid?"<< /Type /Font /Subtype /Type0 /BaseFont /FaceProbe /Encoding /"+cmap+" /DescendantFonts [9 0 R] /ToUnicode 8 0 R >>":
      "<< /Type /Font /Subtype /"+std::string(cff?"Type1":"TrueType")+" /BaseFont /FaceProbe /FirstChar 0 /LastChar 255 /Widths ["+widths+"] /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R >>",
    "<< /Type /FontDescriptor /FontName /FaceProbe /Flags 32 /FontBBox [-200 -200 1200 1000] /Ascent 800 /Descent -200 /CapHeight 700 /ItalicAngle 0 /StemV 80 /"+std::string(cff?"FontFile3":"FontFile2")+" 7 0 R >>",
    PdfStream(cff?"/Subtype /OpenType":"/Length1 "+std::to_string(font.size()),std::string(font.begin(),font.end())),
    PdfStream("",UnicodeMap(encoded,unicode)),
    "<< /Type /Font /Subtype /"+std::string(cff?"CIDFontType0":"CIDFontType2")+" /BaseFont /FaceProbe /CIDSystemInfo << /Registry (Adobe) /Ordering ("+ordering+") /Supplement 6 >> /FontDescriptor 6 0 R /DW 1000 >>"});
}
struct Bounds{float left=1e9f,bottom=1e9f,right=-1e9f,top=-1e9f;int lines=0,curves=0;};
// GetGlyphPath returns a borrowed normalized path; font_size does not scale
// its points. Convert to the requested point size explicitly for these oracles.
Bounds Outline(const Api& api,FPDF_FONT font,unsigned code,float size){Bounds b;const auto path=api.FPDFFont_GetGlyphPath(font,code,size);EXPECT_NE(path,nullptr);if(!path)return b;const auto count=api.FPDFGlyphPath_CountGlyphSegments(path);EXPECT_GT(count,0);for(int i=0;i<count;++i){const auto s=api.FPDFGlyphPath_GetGlyphPathSegment(path,i);float x=0,y=0;EXPECT_TRUE(api.FPDFPathSegment_GetPoint(s,&x,&y));x*=size;y*=size;b.left=std::min(b.left,x);b.right=std::max(b.right,x);b.bottom=std::min(b.bottom,y);b.top=std::max(b.top,y);b.lines+=api.FPDFPathSegment_GetType(s)==FPDF_SEGMENT_LINETO;b.curves+=api.FPDFPathSegment_GetType(s)==FPDF_SEGMENT_BEZIERTO;}return b;}
size_t Ink(const Raster& r){size_t n=0;for(size_t p=0;p<r.pixels.size();p+=4)n+=r.pixels[p]<180;return n;}
void Visible(const Raster& r){EXPECT_GT(Ink(r),8u);EXPECT_LT(Ink(r),size_t(r.width*r.height/2));}
void InspectA(const Api& api,FPDF_PAGE page,FPDF_TEXTPAGE text){EXPECT_EQ(text_fixture::Extract(api,text),u"A");const auto obj=api.FPDFPage_GetObject(page,0);ASSERT_NE(obj,nullptr);EXPECT_EQ(api.FPDFPageObj_GetType(obj),FPDF_PAGEOBJ_TEXT);const auto font=api.FPDFTextObj_GetFont(obj);ASSERT_NE(font,nullptr);EXPECT_EQ(api.FPDFFont_GetIsEmbedded(font),1);double l=0,r=0,b=0,t=0;ASSERT_TRUE(api.FPDFText_GetCharBox(text,0,&l,&r,&b,&t));EXPECT_GT(r,l);EXPECT_GT(t,b);EXPECT_TRUE(std::isfinite(l));}
// Fontations/AGG and native FreeType have different curved-glyph coverage.
// This private boundary applies only to pinned Minion U and mapped Foxit B:
// measured maxima72/43, union BGRA means2.511/1.927, mass delta<=1.234%.
// Dark contours/bounds remain identical. Minion's faint top fringe extends
// two pixels from Rust AA; Foxit needs only one. Generic budgets stay intact.
void CffCoverageGuards(const Raster& a,const Raster& b,int aa_radius){
  ASSERT_EQ(a.width,b.width);ASSERT_EQ(a.height,b.height);ASSERT_EQ(a.pixels.size(),b.pixels.size());
  const auto gray=[](const Raster& r,int x,int y){return r.pixels[(size_t(y)*r.width+x)*4];};
  const auto near=[&](const Raster& r,int x,int y,int radius,bool antialias){
    for(int yy=std::max(0,y-radius);yy<=std::min(r.height-1,y+radius);++yy)
      for(int xx=std::max(0,x-radius);xx<=std::min(r.width-1,x+radius);++xx){const auto g=gray(r,xx,yy);if(antialias?(g>0&&g<255):g<180)return true;}
    return false;
  };
  std::array<int,4> box_a{a.width,a.height,-1,-1},box_b=box_a;uint64_t mass_a=0,mass_b=0;
  for(int y=0;y<a.height;++y)for(int x=0;x<a.width;++x){
    const size_t p=(size_t(y)*a.width+x)*4;const auto ga=gray(a,x,y),gb=gray(b,x,y);
    ASSERT_EQ(a.pixels[p+3],255);ASSERT_EQ(b.pixels[p+3],255);
    ASSERT_EQ(a.pixels[p],a.pixels[p+1]);ASSERT_EQ(a.pixels[p],a.pixels[p+2]);
    ASSERT_EQ(b.pixels[p],b.pixels[p+1]);ASSERT_EQ(b.pixels[p],b.pixels[p+2]);
    mass_a+=255-ga;mass_b+=255-gb;
    if(ga<180){box_a[0]=std::min(box_a[0],x);box_a[1]=std::min(box_a[1],y);box_a[2]=std::max(box_a[2],x);box_a[3]=std::max(box_a[3],y);ASSERT_TRUE(near(b,x,y,1,false));}
    if(gb<180){box_b[0]=std::min(box_b[0],x);box_b[1]=std::min(box_b[1],y);box_b[2]=std::max(box_b[2],x);box_b[3]=std::max(box_b[3],y);ASSERT_TRUE(near(a,x,y,1,false));}
    ASSERT_FALSE((ga==0&&gb==255)||(ga==255&&gb==0));
    if(ga!=gb){ASSERT_TRUE(near(a,x,y,aa_radius,true))<<x<<','<<y;ASSERT_TRUE(near(b,x,y,aa_radius,true))<<x<<','<<y;}
  }
  EXPECT_EQ(box_a,box_b);EXPECT_GT(mass_a,0u);const auto difference=mass_a>mass_b?mass_a-mass_b:mass_b-mass_a;
  EXPECT_LE(difference*200,std::max(mass_a,mass_b)*3); //1.5%, no missing solid ink.
}
void CompareCffCoverage(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label,int aa_radius){
  CffCoverageGuards(a,b,aa_radius);text_fixture::CompareTextRaster(a,b,artifacts,label,{80,0,2.75});
}
void Exercise(const Bytes& bytes,const std::string& label,const std::function<void(const Api&,FPDF_PAGE,FPDF_TEXTPAGE)>& inspect=InspectA,int cff_aa_radius=0){
  Artifacts artifacts;std::array<Raster,2> images;size_t i=0;
  for(const auto* library:Implementations()){SCOPED_TRACE(library->name()+label);const auto& api=library->api();Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);inspect(api,page.get(),text.get());images[i++]=Render(api,page.get(),400,240);Visible(images[i-1]);text.reset();page.reset();
    if(cff_aa_radius){const auto guarded=[&](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE t){inspect(read,p,t);CffCoverageGuards(images[i-1],Render(read,p,400,240),cff_aa_radius);};text_fixture::CrossReload(*library,doc.get(),artifacts,label,guarded,FPDF_NO_INCREMENTAL,{80,0,2.75});}
    else text_fixture::CrossReload(*library,doc.get(),artifacts,label,inspect);}
  if(cff_aa_radius)CompareCffCoverage(images[0],images[1],artifacts,label+"-engines",cff_aa_radius);
  else text_fixture::CompareTextRaster(images[0],images[1],artifacts,label+"-engines");
}
void Rectangle(const Api& api,FPDF_PAGE page,float expected){const auto font=api.FPDFTextObj_GetFont(api.FPDFPage_GetObject(page,0));const auto b=Outline(api,font,'A',100);EXPECT_NEAR(b.left,0,.01);EXPECT_NEAR(b.bottom,0,.01);EXPECT_NEAR(b.right,expected,.01);EXPECT_NEAR(b.top,70,.01);EXPECT_EQ(b.curves,0);EXPECT_GE(b.lines,2);float advance=0;ASSERT_TRUE(api.FPDFFont_GetGlyphWidth(font,'A',100,&advance));EXPECT_NEAR(advance,70,.01);}

TEST(CoverageFontFacePaths, HeadlessCffWithoutCmapRetainsRealNamesCurvesAndMetrics){
  Exercise(Embedded(Headless("fonts/foxit_fixed.cff"),true),"headless-cff-names",[](const Api& a,FPDF_PAGE p,FPDF_TEXTPAGE t){InspectA(a,p,t);auto f=a.FPDFTextObj_GetFont(a.FPDFPage_GetObject(p,0));float w=0;ASSERT_TRUE(a.FPDFFont_GetGlyphWidth(f,'A',100,&w));EXPECT_NEAR(w,60,.01);const auto path=Outline(a,f,'O',100);EXPECT_GT(path.curves,0);EXPECT_GT(path.right-path.left,20);});
}
TEST(CoverageFontFacePaths, HeadlessCffPdfEncodingKeepsGlyphNamesDespiteConflictingSfntMap){
  const auto bytes=Embedded(Headless("fonts/foxit_fixed.cff",true),true);Exercise(bytes,"headless-cff-cmap",[](const Api& a,FPDF_PAGE p,FPDF_TEXTPAGE t){InspectA(a,p,t);const auto b=Outline(a,a.FPDFTextObj_GetFont(a.FPDFPage_GetObject(p,0)),'A',100);EXPECT_EQ(b.curves,0);});
}
TEST(CoverageFontFacePaths, HeadlessCidCffKeepsOriginalNonSequentialCidToOutlineMapping){
  // Pinned Minion charset: CID54 is glyph3, U (CID1 is blank). Its
  // outline bounds are independently specified by its original Type2 charstring.
  // The public advance instead follows the explicit PDF DW1000.
  Exercise(Embedded(Headless("fonts/minion.cff"),true,"BT /F 40 Tf 30 100 Td <0036> Tj ET",true,"Identity-H","0036","0055"),"headless-minion-cid",[](const Api& a,FPDF_PAGE p,FPDF_TEXTPAGE t){
    EXPECT_EQ(text_fixture::Extract(a,t),u"U");const auto font=a.FPDFTextObj_GetFont(a.FPDFPage_GetObject(p,0));ASSERT_NE(font,nullptr);
    const auto b=Outline(a,font,'U',100);EXPECT_NEAR(b.left,3.3,.01);EXPECT_NEAR(b.bottom,-1.5,100.0/4096); // Native FT path points are quantized to1/4096em.
    EXPECT_NEAR(b.right,71.6,.01);EXPECT_NEAR(b.top,65,.01);EXPECT_GT(b.curves,0);
    float advance=0;ASSERT_TRUE(a.FPDFFont_GetGlyphWidth(font,'U',100,&advance));EXPECT_NEAR(advance,100,.01); // PDF DW overrides the original736-unit font advance.
  },2);
}
void FactoryCff(bool cid,bool map){
  Artifacts artifacts;std::array<Raster,2> images;Raster native_mapped_control;size_t i=0;
  for(const auto* library:Implementations()){SCOPED_TRACE(library->name());const auto& a=library->api();auto data=Headless("fonts/foxit_fixed.cff",map);Handle doc(a.FPDF_CreateNewDocument(),a.FPDF_CloseDocument);Handle page(a.FPDFPage_New(doc.get(),0,400,240),a.FPDF_ClosePage);Handle font(a.FPDFText_LoadFont(doc.get(),data.data(),data.size(),FPDF_FONT_TYPE1,cid),a.FPDFFont_Close);ASSERT_NE(font.get(),nullptr);data.clear();const auto outline=Outline(a,font.get(),'A',40);EXPECT_GT(outline.right,outline.left);if(cid&&map){if(library->name()=="rust"){EXPECT_GT(outline.curves,0);}else{EXPECT_EQ(outline.curves,0);}}Handle obj(a.FPDFPageObj_CreateTextObj(doc.get(),font.get(),40),a.FPDFPageObj_Destroy);ASSERT_NE(obj.get(),nullptr);ASSERT_TRUE(a.FPDFText_SetText(obj.get(),text_fixture::Wide(u"A").data()));const FS_MATRIX m{1,0,0,1,30,100};ASSERT_TRUE(a.FPDFPageObj_SetMatrix(obj.get(),&m));ASSERT_TRUE(a.FPDFPage_InsertObject(page.get(),obj.release()));ASSERT_TRUE(a.FPDFPage_GenerateContent(page.get()));images[i++]=Render(a,page.get(),400,240);Visible(images[i-1]);page.reset();
    if(cid&&map){const auto guarded=[&](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE t){InspectA(read,p,t);CffCoverageGuards(images[i-1],Render(read,p,400,240),1);};text_fixture::CrossReload(*library,doc.get(),artifacts,"headless-cid-factory",guarded,FPDF_NO_INCREMENTAL,{80,0,2.75});}
    else text_fixture::CrossReload(*library,doc.get(),artifacts,"headless-simple-factory",InspectA);
    if(cid&&map&&library->name()=="native"){
      // FreeType's headless-CFF loader synthesizes its Unicode map from CFF
      // glyph names, ignoring this wrapper's cmap: originalA selectsGID1.
      // Rust honors explicit cmapA->GID9. Compare its correctB outline to a
      // healthy native public-factoryB control, preserving originalA text
      // and each writer's exact four-way save/reload assertions above.
      const auto bytes=Headless("fonts/foxit_fixed.cff",true);Handle control_doc(a.FPDF_CreateNewDocument(),a.FPDF_CloseDocument);
      Handle control_page(a.FPDFPage_New(control_doc.get(),0,400,240),a.FPDF_ClosePage);
      Handle control_font(a.FPDFText_LoadFont(control_doc.get(),bytes.data(),bytes.size(),FPDF_FONT_TYPE1,true),a.FPDFFont_Close);ASSERT_NE(control_font.get(),nullptr);
      EXPECT_GT(Outline(a,control_font.get(),'B',40).curves,0);
      Handle control_object(a.FPDFPageObj_CreateTextObj(control_doc.get(),control_font.get(),40),a.FPDFPageObj_Destroy);ASSERT_NE(control_object.get(),nullptr);
      ASSERT_TRUE(a.FPDFText_SetText(control_object.get(),text_fixture::Wide(u"B").data()));const FS_MATRIX control_matrix{1,0,0,1,30,100};ASSERT_TRUE(a.FPDFPageObj_SetMatrix(control_object.get(),&control_matrix));ASSERT_TRUE(a.FPDFPage_InsertObject(control_page.get(),control_object.release()));
      native_mapped_control=Render(a,control_page.get(),400,240);artifacts.Image("native-original-A",images[0]);artifacts.Image("native-independent-B",native_mapped_control);
    }
  }
  if(cid&&map)CompareCffCoverage(native_mapped_control,images[1],artifacts,"headless-factory-mapped-glyph-control",1);
  else text_fixture::CompareTextRaster(images[0],images[1],artifacts,"headless-factory-engines");
}
TEST(CoverageFontFacePaths, HeadlessCffSimpleFactoryCopiesWrappedFontAndSavesOriginalOutlines){FactoryCff(false,false);}
TEST(CoverageFontFacePaths, HeadlessCffCidFactoryEnumeratesExplicitUnicodeCmap){FactoryCff(true,true);}
TEST(CoverageFontFacePaths, OrdinaryHintOptionalRectangleKeepsUnhintedOutlineAcrossPointSizes){for(float size:{10.f,13.5f,25.f,100.f}){SCOPED_TRACE(size);Exercise(Embedded(Hinted(false),false,"BT /F "+std::to_string(size)+" Tf 30 100 Td (A) Tj ET"),"optional-"+std::to_string(size),[](const Api& a,FPDF_PAGE p,FPDF_TEXTPAGE t){InspectA(a,p,t);Rectangle(a,p,40);});}}
// Independent healthy control for the synthetic required-hinter font. Its
// optional sibling has identical metadata/advance but no tricky-name dispatch.
// Replace only A's glyf/loca and head bounds with an unhinted600x700 rectangle;
// do not copy either engine's path or bitmap. Native FreeType executes SHPIX
// at a different ppem for this deliberately adversarial program; Rust's
// documented fixed1000ppem interpretation reconstructs this exact outline.
Bytes RequiredControl(){
  auto tables=Split(Hinted(false));Bytes glyph(34);W16(glyph,0,1);W16(glyph,6,600);W16(glyph,8,700);W16(glyph,10,3);
  for(size_t i=14;i<18;++i)glyph[i]=1;
  W16(glyph,20,600);W16(glyph,24,uint16_t(-600));W16(glyph,30,700);
  tables[Tag("glyf")]=glyph;tables[Tag("loca")]=Bytes(6);W16(tables[Tag("loca")],4,17);W16(tables[Tag("head")],40,600);
  return Assemble(tables);
}
void RequiredCases(float size,bool off_page){
  Artifacts artifacts;const auto content="BT /F "+std::to_string(size)+" Tf "+(off_page?"-50":"30")+" 100 Td (A) Tj ET";
  const auto source=Embedded(Hinted(true),false,content),healthy=Embedded(RequiredControl(),false,content);
  const auto implementations=Implementations();std::array<Raster,2> originals;size_t index=0;
  for(const auto* library:implementations){
    SCOPED_TRACE(library->name());const auto& api=library->api();Handle doc(api.FPDF_LoadMemDocument64(source.data(),source.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);EXPECT_EQ(text_fixture::Extract(api,text.get()),u"A");
    originals[index++]=Render(api,page.get(),400,240);artifacts.Image(library->name()+"-required-original",originals[index-1]);
    if(library->name()=="rust"){
      Rectangle(api,page.get(),60);
      if(off_page){EXPECT_GT(Ink(CropRaster(originals[index-1],0,70,10,70)),400u);EXPECT_EQ(Ink(CropRaster(originals[index-1],12,0,388,240)),0u);}
    }
    text.reset();page.reset();const auto saved=SaveDocument(api,doc.get(),artifacts,library->name()+"-required");ASSERT_FALSE(saved.empty());
    for(const auto* reader:implementations){
      SCOPED_TRACE(reader->name());const auto& read=reader->api();Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);Handle p(read.FPDF_LoadPage(reopened.get(),0),read.FPDF_ClosePage);ASSERT_NE(p.get(),nullptr);Handle t(read.FPDFText_LoadPage(p.get()),read.FPDFText_ClosePage);ASSERT_NE(t.get(),nullptr);EXPECT_EQ(text_fixture::Extract(read,t.get()),u"A");
      Handle reference(read.FPDF_LoadMemDocument64(source.data(),source.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reference.get(),nullptr);Handle rp(read.FPDF_LoadPage(reference.get(),0),read.FPDF_ClosePage);ASSERT_NE(rp.get(),nullptr);
      CompareRaster(Render(read,rp.get(),400,240),Render(read,p.get(),400,240),artifacts,library->name()+reader->name()+"-required-own-original");
      if(reader->name()=="rust")Rectangle(read,p.get(),60);
    }
  }
  std::array<Raster,2> controls;std::array<Bounds,2> control_paths;index=0;
  for(const auto* library:implementations){const auto& api=library->api();Handle doc(api.FPDF_LoadMemDocument64(healthy.data(),healthy.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);Handle p(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(p.get(),nullptr);Rectangle(api,p.get(),60);control_paths[index]=Outline(api,api.FPDFTextObj_GetFont(api.FPDFPage_GetObject(p.get(),0)),'A',100);controls[index++]=Render(api,p.get(),400,240);}
  // Compare Rust's required interpretation to its independent, unhinted
  //600-unit outline exactly. Native's64*64 fixed-point path conversion rounds
  // the600/1000 edge to2458/4096. Rust retains the unhinted outline, but its
  // tile-stable rectangle snap ceils 0.6f32*100 before adding the integer
  // origin: both controls paint the predicted x90 boundary strip at100pt.
  CompareRaster(controls[1],originals[1],artifacts,"rust-required-independent-control");
  if(size==100&&!off_page){
    EXPECT_NEAR(control_paths[0].right,100.0*2458/4096,.000001);
    EXPECT_NEAR(control_paths[1].right,60,.00001);
    EXPECT_EQ(Ink(controls[0]),4270u);EXPECT_EQ(Ink(controls[1]),4270u);
    for(int y=70;y<140;++y){const size_t p=(size_t(y)*400+90)*4;
      for(size_t c=0;c<3;++c){ASSERT_EQ(controls[0].pixels[p+c],0);ASSERT_EQ(controls[1].pixels[p+c],0);}
      ASSERT_EQ(controls[0].pixels[p+3],255);ASSERT_EQ(controls[1].pixels[p+3],255);
    }
    artifacts.Image("native-fixed-point-boundary-control",controls[0]);
    CompareRaster(controls[0],controls[1],artifacts,"required-healthy-control");
  }else CompareRaster(controls[0],controls[1],artifacts,"required-healthy-control",size==13.5f?1:0);
}
TEST(CoverageFontFacePaths, RequiredInterpreterReconstructsRectangleAndAdvanceAcrossPointSizes){for(float size:{10.f,13.5f,25.f,100.f}){SCOPED_TRACE(size);RequiredCases(size,false);}}
TEST(CoverageFontFacePaths, RequiredInterpreterBoundsKeepRightEdgeVisibleAtNegativePageOrigin){RequiredCases(100,true);}
TEST(CoverageFontFacePaths, OptionalOutlineOutsidePageIsCulledWithoutInventingVisibleInk){
  Artifacts artifacts;const auto bytes=Embedded(Hinted(false),false,"BT /F 100 Tf -50 100 Td (A) Tj ET");for(const auto* lib:Implementations()){SCOPED_TRACE(lib->name());const auto& a=lib->api();Handle doc(a.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),a.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);text_fixture::CrossReload(*lib,doc.get(),artifacts,"optional-off-page",[](const Api& a,FPDF_PAGE p,FPDF_TEXTPAGE){Rectangle(a,p,40);EXPECT_EQ(Ink(Render(a,p,400,240)),0u);});}}

// With a predefined CMap and an embedded font, the source deliberately uses
// CID directly (the predefined map has no allocated direct table). ToUnicode
// controls extraction, while the legacy cmap must not change glyph selection.
Bytes CidFont(uint16_t encoding,uint16_t code,uint16_t cid){const auto raw=text_fixture::Resource("fonts/ahem/Ahem.ttf");auto tables=Split(raw);const auto cmap=tables.at(Tag("cmap"));uint16_t glyph=0;
  for(unsigned i=0;i<U16(cmap,2)&&!glyph;++i){const auto p=U32(cmap,8+i*8);if(U16(cmap,p)!=4)continue;const unsigned n=U16(cmap,p+6)/2;const auto ends=p+14,starts=ends+2*n+2,deltas=starts+2*n,ranges=deltas+2*n;for(unsigned j=0;j<n;++j)if('A'>=U16(cmap,starts+2*j)&&'A'<=U16(cmap,ends+2*j)){const auto ro=U16(cmap,ranges+2*j);glyph=ro?U16(cmap,ranges+2*j+ro+2*('A'-U16(cmap,starts+2*j))):uint16_t('A');if(glyph)glyph=uint16_t(glyph+U16(cmap,deltas+2*j));break;}}
  EXPECT_NE(glyph,0);
  // Predefined CMaps return CID directly for embedded fonts. Copy the pinned
  // A outline to that actual CID; changing cmap alone cannot affect lookup.
  // All other glyph data stays unchanged; appended slots are empty, with the
  // same1000-unit advance. This explicit glyf/loca derivative is test-only.
  const auto old_count=U16(tables.at(Tag("maxp")),4);const auto count=std::max<unsigned>(old_count,unsigned(cid)+1);
  const auto old_glyf=tables.at(Tag("glyf")),old_loca=tables.at(Tag("loca")),old_hmtx=tables.at(Tag("hmtx"));
  const bool long_loca=U16(tables.at(Tag("head")),50)==1;const auto metrics=U16(tables.at(Tag("hhea")),34);
  const auto offset=[&](unsigned index){return long_loca?U32(old_loca,index*4):uint32_t(U16(old_loca,index*2))*2;};
  const Bytes a_outline(old_glyf.begin()+offset(glyph),old_glyf.begin()+offset(glyph+1));
  Bytes glyf,loca((count+1)*4),hmtx(count*4);
  for(unsigned index=0;index<count;++index){W32(loca,index*4,glyf.size());if(index==cid)glyf.insert(glyf.end(),a_outline.begin(),a_outline.end());else if(index<old_count)glyf.insert(glyf.end(),old_glyf.begin()+offset(index),old_glyf.begin()+offset(index+1));while(glyf.size()%4)glyf.push_back(0);
    uint16_t advance=1000,bearing=0;if(index<old_count){advance=U16(old_hmtx,4*std::min<unsigned>(index,metrics-1));bearing=U16(old_hmtx,index<metrics?4*index+2:4*metrics+2*(index-metrics));}W16(hmtx,4*index,advance);W16(hmtx,4*index+2,bearing);}
  W32(loca,count*4,glyf.size());tables[Tag("glyf")]=glyf;tables[Tag("loca")]=loca;tables[Tag("hmtx")]=hmtx;W16(tables[Tag("head")],50,1);W16(tables[Tag("hhea")],34,count);W16(tables[Tag("maxp")],4,count);
  tables[Tag("cmap")]=OneMap(encoding,code,glyph);tables[Tag("post")]=Bytes(32);W32(tables[Tag("post")],0,0x30000);return Assemble(tables);
}
void InspectCidSquare(const Api& a,FPDF_PAGE p,FPDF_TEXTPAGE t){InspectA(a,p,t);double l=0,r=0,b=0,top=0;ASSERT_TRUE(a.FPDFText_GetCharBox(t,0,&l,&r,&b,&top));EXPECT_NEAR(r-l,40,.05);EXPECT_NEAR(top-b,40.48,40.0/1000+0.00001); // char_bbox adds800/64units; outline bounds can truncate one font unit.
  const auto image=Render(a,p,400,240);EXPECT_EQ(Ink(CropRaster(image,31,109,38,30)),1140u);}
TEST(CoverageFontFacePaths, PredefinedCidCmapsSelectOriginalDirectCidOutlinesWithoutCidToGidMap){
  struct Map {const char* name;const char* ordering;const char* code;uint16_t cid;};
  for(const auto& map:std::array<Map,4>{{{"90ms-RKSJ-H","Japan1","8141",634},{"GBK-EUC-H","GB1","8141",10073},{"ETen-B5-H","CNS1","A140",99},{"KSCms-UHC-H","Korea1","8141",9333}}}){
    SCOPED_TRACE(map.name);Exercise(Embedded(CidFont(1,'A',map.cid),false,std::string("BT /F 40 Tf 30 100 Td <")+map.code+"> Tj ET",true,map.name,map.code,"0041",map.ordering),std::string("unicode-cid-")+map.name,InspectCidSquare);
  }
}
TEST(CoverageFontFacePaths, PredefinedDirectCidSelectionDoesNotConsultLegacySfntCharmaps){for(uint16_t encoding:{2,3,4,5,6}){SCOPED_TRACE(encoding);Exercise(Embedded(CidFont(encoding,0x8141,634),false,"BT /F 40 Tf 30 100 Td <8141> Tj ET",true,"90ms-RKSJ-H","8141"),"legacy-cid-"+std::to_string(encoding),InspectCidSquare);}}

// TTC retains the complete original SFNT payloads; only each directory's table
// offsets are relocated to absolute collection offsets. Returning size-offset
// for tag0 is PDFium's documented GetCachedTTCFace/GetTTCIndex provider contract.
struct Collection{Bytes bytes;std::array<size_t,2> offsets;};
Collection MakeCollection(bool reverse){std::array<Bytes,2> faces={text_fixture::Resource("fonts/ahem/Ahem.ttf"),Hinted(false)};if(reverse)std::swap(faces[0],faces[1]);Collection out{Bytes(20),{}};W32(out.bytes,0,Tag("ttcf"));W32(out.bytes,4,0x10000);W32(out.bytes,8,2);for(size_t i=0;i<2;++i){while(out.bytes.size()%4)out.bytes.push_back(0);const auto offset=out.bytes.size();out.offsets[i]=offset;W32(out.bytes,12+i*4,offset);auto face=faces[i];for(unsigned j=0;j<U16(face,4);++j){const auto p=12+j*16+8;W32(face,p,U32(face,p)+offset);}out.bytes.insert(out.bytes.end(),face.begin(),face.end());}return out;}
struct CollectionProvider:FPDF_SYSFONTINFO{
  CollectionProvider(const Library& lib,bool reverse):library(lib),collection(MakeCollection(reverse)),reversed(reverse){version=2;Release=ReleaseInfo;EnumFonts=Enumerate;MapFont=Map;GetFont=Get;GetFontData=Data;GetFaceName=Name;GetFontCharset=Charset;DeleteFont=Delete;}
  static CollectionProvider* Self(FPDF_SYSFONTINFO* p){return static_cast<CollectionProvider*>(p);}
  static void ReleaseInfo(FPDF_SYSFONTINFO* p){++Self(p)->releases;}
  static void Enumerate(FPDF_SYSFONTINFO*,void*){}
  static void* Map(FPDF_SYSFONTINFO* p,int,FPDF_BOOL,int,int,const char* face,FPDF_BOOL* exact){
    // Do not register these synthetic faces under unrelated installed names
    // when native XFA enumerates an existing font inventory during installation.
    if(!face||(std::strcmp(face,"CollectionSquare")&&std::strcmp(face,"CollectionRectangle"))){if(exact)*exact=0;return nullptr;}
    auto* s=Self(p);if(exact)*exact=1;const bool rectangle=std::strcmp(face,"CollectionRectangle")==0;
    const size_t index=size_t(rectangle)^size_t(s->reversed);++s->maps[index];return &s->tokens[index];
  }
  static void* Get(FPDF_SYSFONTINFO* p,const char* face){return Map(p,400,0,0,0,face,nullptr);}
  static unsigned long Data(FPDF_SYSFONTINFO* p,void* token,unsigned int tag,unsigned char* buffer,unsigned long size){auto* s=Self(p);const size_t index=token==&s->tokens[1]?1:0;s->valid&=token==&s->tokens[index];if(tag!=0&&tag!=Tag("ttcf"))return 0;const auto offset=tag?0:s->collection.offsets[index];const auto n=s->collection.bytes.size()-offset;++s->queries;if(buffer&&size>=n){std::copy_n(s->collection.bytes.data()+offset,n,buffer);++s->copies;}return n;}
  static unsigned long Name(FPDF_SYSFONTINFO* p,void* token,char* buffer,unsigned long n){auto* s=Self(p);const size_t index=token==&s->tokens[1]?1:0;const char* name=(index^size_t(s->reversed))?"Outline-Test":"Ahem";const auto size=std::strlen(name)+1;if(buffer&&n>=size)std::memcpy(buffer,name,size);return size;}
  static int Charset(FPDF_SYSFONTINFO*,void*){return FXFONT_ANSI_CHARSET;}
  static void Delete(FPDF_SYSFONTINFO* p,void*){++Self(p)->deletes;}
  const Library& library;Collection collection;bool reversed,valid=true;std::array<int,2>tokens{11,22};std::array<size_t,2>maps{};size_t queries=0,copies=0,deletes=0,releases=0;
};
Bytes Substitute(const char* name){return PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>","<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 240] /Resources << /Font << /F 5 0 R >> >> /Contents 4 0 R >>",PdfStream("","BT /F 40 Tf 30 100 Td (A) Tj ET"),"<< /Type /Font /Subtype /TrueType /BaseFont /"+std::string(name)+" /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R >>","<< /Type /FontDescriptor /FontName /"+std::string(name)+" /Flags 32 /FontBBox [0 0 1000 1000] /Ascent 800 /Descent -200 /CapHeight 700 /ItalicAngle 0 /StemV 80 >>"});}
void CollectionCases(bool reverse){Artifacts artifacts;std::array<std::unique_ptr<CollectionProvider>,2> providers;size_t provider_index=0;for(const auto* lib:Implementations()){providers[provider_index]=std::make_unique<CollectionProvider>(*lib,reverse);lib->api().FPDF_SetSystemFontInfo(providers[provider_index++].get());}
  struct Restore{~Restore(){for(const auto* lib:Implementations())lib->restore_default_font_info();}}restore;
  std::array<std::array<Raster,2>,2> images;size_t backend=0;for(const auto* library:Implementations()){
  SCOPED_TRACE(library->name());const auto& a=library->api();auto& provider=*providers[backend];
  const auto square=Substitute("CollectionSquare"),rect=Substitute("CollectionRectangle");Handle first(a.FPDF_LoadMemDocument64(square.data(),square.size(),nullptr),a.FPDF_CloseDocument);Handle second(a.FPDF_LoadMemDocument64(rect.data(),rect.size(),nullptr),a.FPDF_CloseDocument);ASSERT_NE(first.get(),nullptr);ASSERT_NE(second.get(),nullptr);
  // Keep both documents/faces alive while requesting the original collection
  // again: this is the shared-byte-owner cache path, not two isolated loads.
  for(size_t pass=0;pass<2;++pass)for(size_t face=0;face<2;++face){SCOPED_TRACE(std::to_string(pass)+"-"+std::to_string(face));const auto doc=face?second.get():first.get();Handle page(a.FPDF_LoadPage(doc,0),a.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);auto font=a.FPDFTextObj_GetFont(a.FPDFPage_GetObject(page.get(),0));ASSERT_NE(font,nullptr);const auto bounds=Outline(a,font,'A',100);EXPECT_NEAR(bounds.right-bounds.left,face?40:100,.1);images[backend][face]=Render(a,page.get(),400,240);Visible(images[backend][face]);}
  for(size_t face=0;face<2;++face){const auto& bytes=face?rect:square;Handle third(a.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),a.FPDF_CloseDocument);ASSERT_NE(third.get(),nullptr);text_fixture::CrossReload(*library,third.get(),artifacts,"ttc-"+std::to_string(face),[face](const Api& api,FPDF_PAGE p,FPDF_TEXTPAGE t){EXPECT_EQ(text_fixture::Extract(api,t),u"A");const auto b=Outline(api,api.FPDFTextObj_GetFont(api.FPDFPage_GetObject(p,0)),'A',100);EXPECT_NEAR(b.right-b.left,face?40:100,.1);});}
  EXPECT_GT(provider.maps[0],0u);EXPECT_GT(provider.maps[1],0u);EXPECT_GT(provider.queries,0u);EXPECT_GT(provider.copies,0u);EXPECT_GT(provider.deletes,0u);EXPECT_TRUE(provider.valid);EXPECT_EQ(provider.releases,0u);EXPECT_NE(images[backend][0].pixels,images[backend][1].pixels);++backend;
 }for(size_t face=0;face<2;++face)text_fixture::CompareTextRaster(images[0][face],images[1][face],artifacts,"ttc-engines-"+std::to_string(face));}
TEST(CoverageFontFacePaths, TwoFaceCollectionSelectsBothLiveFacesAndReusesCachedByteOwner){CollectionCases(false);}
TEST(CoverageFontFacePaths, ReversedCollectionDirectoryStillSelectsRequestedNonzeroFace){CollectionCases(true);}
} // namespace
} // namespace pdfoxium::integration
