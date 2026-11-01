#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using RGB=std::array<int,3>;
const RGB red{255,0,0},blue{0,0,255},white{255,255,255};
void Little(std::string& bytes,size_t at,uint32_t value,int count=4) {for(int i=0;i<count;++i)bytes[at+i]=char(value>>(8*i));}
std::string Bmp(RGB color,bool top_down=false,int xdpi=2835,int ydpi=2835) {
  // Literal 2x2,24bpp rows. Padded bytes and DPI are authored independently of
  // either engine; a negative height selects the BMP top-down row path.
  std::string bytes(70,'\0');bytes[0]='B';bytes[1]='M';Little(bytes,2,70);Little(bytes,10,54);Little(bytes,14,40);
  Little(bytes,18,2);Little(bytes,22,top_down?uint32_t(-2):2);Little(bytes,26,1,2);Little(bytes,28,24,2);
  Little(bytes,34,16);Little(bytes,38,xdpi);Little(bytes,42,ydpi);
  for(int y=0;y<2;++y){for(int x=0;x<2;++x)for(int c=0;c<3;++c)bytes[54+y*8+x*3+c]=char(color[2-c]);}
  return bytes;
}
void Big(std::string& out,uint32_t n){for(int shift:{24,16,8,0})out+=char(n>>shift);}
uint32_t Crc(const std::string& bytes) {uint32_t crc=~0u;for(unsigned char ch:bytes){crc^=ch;for(int i=0;i<8;++i)crc=(crc>>1)^((crc&1)?0xedb88320u:0);}return ~crc;}
std::string Chunk(const std::string& tag,const std::string& bytes){std::string out;Big(out,bytes.size());out+=tag+bytes;Big(out,Crc(tag+bytes));return out;}
std::string Png(RGB color) {
  // One RGB pixel, PNG filter0 and one stored DEFLATE block. No image-library
  // output or captured native pixels are used to manufacture expectations.
  std::string row(1,'\0');for(int c:color)row+=char(c);
  std::string z("\x78\x01\x01\x04\x00\xfb\xff",7);z+=row;uint32_t a=1,b=0;
  for(unsigned char c:row){a=(a+c)%65521;b=(b+a)%65521;}Big(z,(b<<16)|a);
  std::string header;Big(header,1);Big(header,1);header.append("\x08\x02\x00\x00\x00",5);
  return std::string("\x89PNG\r\n\x1a\n",8)+Chunk("IHDR",header)+Chunk("IDAT",z)+Chunk("IEND","");
}
std::string Hex(const std::string& bytes){std::string out;const char* digits="0123456789ABCDEF";for(unsigned char c:bytes){out+=digits[c>>4];out+=digits[c&15];}return out;}
struct Resource {std::string key,bytes,dictionary;bool scalar=false;};
struct Image {std::string href;RGB expected;int x=10,y=10;bool field=false;std::string attributes={};};
struct Spec {std::vector<Resource> resources;std::vector<Image> images;int tree=0;bool missing=false;};
std::string Draw(const Image& image,int index) {
  const auto kind=image.field?"field":"draw";
  return "<"+std::string(kind)+" name=\"image"+std::to_string(index)+"\" x=\""+std::to_string(image.x)+"pt\" y=\""+std::to_string(image.y)+
      "pt\" w=\"40pt\" h=\"40pt\">"+(image.field?"<bind match=\"none\"/><ui><imageEdit/></ui>":"")+
      "<value><image href=\""+image.href+"\" aspect=\"none\" "+image.attributes+"/></value></"+kind+">";
}
std::vector<uint8_t> Fixture(const Spec& spec) {
  std::string body;for(size_t i=0;i<spec.images.size();++i)body+=Draw(spec.images[i],int(i));
  body+="<draw name=\"landmark\" x=\"220pt\" y=\"220pt\" w=\"10pt\" h=\"10pt\"><value><rectangle><fill><color value=\"255,0,255\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
  const auto xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" restoreState=\"auto\">"
      "<pageSet><pageArea><occur min=\"1\" max=\"-1\"/><medium short=\"240pt\" long=\"240pt\"/><contentArea w=\"240pt\" h=\"240pt\"/></pageArea></pageSet>"+body+"</subform></template>";
  std::string entries;for(size_t i=0;i<spec.resources.size();++i)entries+=spec.resources[i].key+" "+std::to_string(10+i)+" 0 R ";
  const int next=10+spec.resources.size();
  std::string names;
  if(spec.tree!=4) {
    const auto leaf="<< /Names ["+entries+"] >>";
    if(spec.tree==1)names=" /Names << /XFAImages << /Kids ["+std::to_string(next)+" 0 R] >> >>";
    else if(spec.tree==2)names=" /Names << /XFAImages << /Names "+std::to_string(next)+" 0 R >> >>";
    else if(spec.tree==3)names=" /Names << /XFAImages "+std::to_string(next)+" 0 R >>";
    else names=" /Names << /XFAImages "+leaf+" >>";
  }
  std::vector<std::string> objects={"<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true"+names+" >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>","<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 240] >>",
      "<< /Fields [] /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("",xml),PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),PdfStream("","</xdp:xdp>")};
  for(const auto& resource:spec.resources)objects.push_back(resource.scalar?resource.bytes:PdfStream(resource.dictionary,resource.bytes));
  if(spec.tree==1||spec.tree==3)objects.push_back("<< /Names ["+entries+"] >>");
  else if(spec.tree==2)objects.push_back("["+entries+"]");
  return PdfFixture(objects);
}
struct Loaded {forms::XfaCallbacks callbacks;forms::Session s;
  Loaded(const Library& library,const std::vector<uint8_t>& bytes):s(library.api(),bytes,true,true,2,[this](forms::FormHost& h){callbacks.success=false;callbacks.Configure(h);}){}
};
void Pixel(const Raster& image,int x,int y,RGB expected) {
  const auto at=(size_t(y)*image.width+x)*4;
  EXPECT_EQ(image.pixels.at(at),expected[2]);EXPECT_EQ(image.pixels.at(at+1),expected[1]);EXPECT_EQ(image.pixels.at(at+2),expected[0]);
}
void Inspect(const Raster& image,const Spec& spec) {
  ASSERT_EQ(image.width,240);ASSERT_EQ(image.height,240);Pixel(image,225,225,{255,0,255});
  for(const auto& request:spec.images){for(int offset:{5,20,35})Pixel(image,request.x+offset,request.y+offset,request.expected);
    Pixel(image,request.x-2,request.y+20,white);Pixel(image,request.x+42,request.y+20,white);}
}
void Observe(Loaded& loaded,const Spec& spec) {
  EXPECT_TRUE(loaded.s.host->callbacks_valid);EXPECT_FALSE(loaded.s.host->callback_error);EXPECT_FALSE(loaded.callbacks.failed);
  EXPECT_TRUE(loaded.callbacks.downloads.empty());
  if(spec.missing){ASSERT_FALSE(loaded.callbacks.opened.empty());for(const auto& call:loaded.callbacks.opened){EXPECT_EQ(std::get<0>(call),0);EXPECT_EQ(std::get<2>(call),"rb");}}
  else EXPECT_TRUE(loaded.callbacks.opened.empty());
}
void Scenario(const std::string& label,const Spec& spec) {
  const auto bytes=Fixture(spec);Artifacts artifacts;artifacts.Pdf(label+"-input",bytes);std::array<Raster,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Loaded loaded(*writer,bytes);ASSERT_TRUE(loaded.s.valid());ASSERT_TRUE(loaded.s.xfa_loaded);ASSERT_EQ(loaded.s.api.FPDF_GetPageCount(loaded.s.document),1);
    originals[backend]=loaded.s.Draw(240,240);ASSERT_NO_FATAL_FAILURE(Inspect(originals[backend],spec));Observe(loaded,spec);
    const auto before=loaded.callbacks.opened.size();const auto repeat=loaded.s.Draw(240,240);Inspect(repeat,spec);
    if(!spec.missing){EXPECT_EQ(loaded.callbacks.opened.size(),before);}
    CompareRaster(originals[backend],repeat,artifacts,writer->name()+label+"-repeat");
    const auto saved=SaveDocument(loaded.s.api,loaded.s.document,artifacts,writer->name()+label);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()){SCOPED_TRACE(reader->name());Loaded reopened(*reader,saved);ASSERT_TRUE(reopened.s.valid());ASSERT_TRUE(reopened.s.xfa_loaded);
      const auto image=reopened.s.Draw(240,240);Inspect(image,spec);Observe(reopened,spec);CompareRaster(originals[backend],image,artifacts,writer->name()+reader->name()+label);}
    ++backend;
  }CompareRaster(originals[0],originals[1],artifacts,label+"-engines");
}
Spec One(const std::string& bytes){Spec s;s.resources={{"(asset)",bytes,""}};s.images={{"asset",red}};return s;}

TEST(CoverageXfaNamedImages, BmpFileBytesAreDecodedFromThePdfNameTree) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Scenario("named-bmp",One(Bmp(red)));
}

TEST(CoverageXfaNamedImages, PngFileBytesAreDecodedFromThePdfNameTree) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Scenario("named-png",One(Png(red)));
}

TEST(CoverageXfaNamedImages, ImageEditUsesTheSameNamedImageResourcePath) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto s=One(Bmp(red));s.images[0].field=true;Scenario("named-image-edit",s);
}

TEST(CoverageXfaNamedImages, RepeatedDrawsReuseOneNamedResourceWithoutHostFileRequests) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto s=One(Png(red));for(int i=1;i<4;++i)s.images.push_back({"asset",red,10+50*i,10});Scenario("repeated-named-image",s);
}

TEST(CoverageXfaNamedImages, DistinctResourceNamesRetainTheirIndependentColors) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto s=One(Bmp(red));s.resources.push_back({"(second)",Png(blue),""});s.images.push_back({"second",blue,70,10});Scenario("distinct-named-images",s);
}

TEST(CoverageXfaNamedImages, UnicodePdfNamesResolveAgainstTheUtf8TemplateHref) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto s=One(Bmp(red));s.resources[0].key="<FEFF00630061006600E9>";s.images[0].href="caf\xc3\xa9";Scenario("unicode-image-name",s);
}

TEST(CoverageXfaNamedImages, ANameTreeWithIndirectKidsStillLocatesTheImageStream) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto s=One(Bmp(red));s.tree=1;Scenario("named-image-kids",s);
}

TEST(CoverageXfaNamedImages, AnIndirectNamesArrayStillLocatesTheImageStream) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto s=One(Png(red));s.tree=2;Scenario("named-image-indirect-array",s);
}

TEST(CoverageXfaNamedImages, AnIndirectNameTreeRootStillLocatesTheImageStream) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto s=One(Png(red));s.tree=3;Scenario("named-image-indirect-root",s);
}

TEST(CoverageXfaNamedImages, PdfStreamFiltersAreDecodedBeforeTheBmpDecoder) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto s=One(Bmp(red));s.resources[0].bytes=Hex(s.resources[0].bytes)+">";s.resources[0].dictionary="/Filter /ASCIIHexDecode";Scenario("filtered-named-bmp",s);
}

TEST(CoverageXfaNamedImages, PdfStreamFiltersAreDecodedBeforeThePngDecoder) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto s=One(Png(red));s.resources[0].bytes=Hex(s.resources[0].bytes)+">";s.resources[0].dictionary="/Filter /ASCIIHexDecode";Scenario("filtered-named-png",s);
}

TEST(CoverageXfaNamedImages, TopDownBmpRowsKeepTheirRequestedColor) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Scenario("top-down-named-bmp",One(Bmp(red,true)));
}

TEST(CoverageXfaNamedImages, NonSquareDpiDoesNotChangeAnExplicitNoneAspectRectangle) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Scenario("named-bmp-dpi",One(Bmp(red,false,2835,5669)));
}

TEST(CoverageXfaNamedImages, AnAbsentNameFallsBackToTheHostWithoutHidingHealthyImages) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto s=One(Bmp(red));s.images.push_back({"missing",white,70,10});s.missing=true;Scenario("absent-name",s);
}

TEST(CoverageXfaNamedImages, AnEmptyNameTreeFallsBackWithoutLosingOrdinaryPagePainting) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.images={{"missing",white}};s.missing=true;Scenario("empty-name-tree",s);
}

TEST(CoverageXfaNamedImages, AnAbsentNameTreeFallsBackWithoutLosingOrdinaryPagePainting) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Spec s;s.tree=4;s.images={{"missing",white}};s.missing=true;Scenario("absent-name-tree",s);
}

TEST(CoverageXfaNamedImages, InvalidNamedImageBytesFallBackWithoutPoisoningAnotherResource) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto s=One(Bmp(red));s.resources.push_back({"(second)","not an image",""});s.images.push_back({"second",white,70,10});s.missing=true;Scenario("invalid-named-image",s);
}

TEST(CoverageXfaNamedImages, ANameMappedToAScalarFallsBackWithoutPoisoningAnotherResource) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto s=One(Bmp(red));s.resources.push_back({"(second)","17","",true});s.images.push_back({"second",white,70,10});s.missing=true;Scenario("scalar-named-image",s);
}

}  // namespace
}  // namespace pdfoxium::integration
