// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "text_fixture.h"

#include <array>
#include <map>
#include <string>
#include <utility>

namespace pdfoxium::integration {
namespace {
using Bytes=std::vector<uint8_t>;
using Tables=std::map<uint32_t,Bytes>;
uint16_t Word(const Bytes& b,size_t at){return(uint16_t(b.at(at))<<8)|b.at(at+1);}
uint32_t Long(const Bytes& b,size_t at){return(uint32_t(Word(b,at))<<16)|Word(b,at+2);}
void PutWord(Bytes& b,size_t at,uint16_t v){b.at(at)=uint8_t(v>>8);b.at(at+1)=uint8_t(v);}
void PutLong(Bytes& b,size_t at,uint32_t v){PutWord(b,at,v>>16);PutWord(b,at+2,v);}
uint32_t Tag(const char* s){uint32_t n=0;for(int i=0;i<4;++i)n=(n<<8)|uint8_t(s[i]);return n;}
uint32_t Checksum(const Bytes& b){uint32_t sum=0;for(size_t i=0;i<b.size();i+=4){uint32_t n=0;for(size_t j=0;j<4;++j)n=(n<<8)|(i+j<b.size()?b[i+j]:0);sum+=n;}return sum;}
Tables TablesFrom(const Bytes& font){
  Tables result;
  for(size_t i=0;i<Word(font,4);++i){const auto row=12+i*16;const size_t start=Long(font,row+8),length=Long(font,row+12);
    if(start>font.size()||length>font.size()-start)throw std::runtime_error("Pinned SFNT table out of bounds");
    result[Long(font,row)]=Bytes(font.begin()+start,font.begin()+start+length);
  }return result;
}
const Bytes& Ahem(){static const Bytes data=text_fixture::Resource("fonts/ahem/Ahem.ttf");return data;}
uint16_t Glyph(uint16_t code){
  const auto tables=TablesFrom(Ahem());const auto& map=tables.at(Tag("cmap"));
  for(size_t i=0;i<Word(map,2);++i){const size_t sub=Long(map,8+i*8);if(Word(map,sub)!=4)continue;
    const size_t count=Word(map,sub+6)/2,ends=sub+14,starts=ends+count*2+2,deltas=starts+count*2,ranges=deltas+count*2;
    for(size_t n=0;n<count;++n){if(code<Word(map,starts+n*2)||code>Word(map,ends+n*2))continue;
      const auto delta=Word(map,deltas+n*2),range=Word(map,ranges+n*2);
      if(!range)return uint16_t(code+delta);
      const auto gid=Word(map,ranges+n*2+range+(code-Word(map,starts+n*2))*2);return gid?uint16_t(gid+delta):0;
    }
  }throw std::runtime_error("Pinned Ahem requested glyph is absent");
}
Bytes Serialize(Tables tables){
  PutLong(tables.at(Tag("head")),8,0);const size_t n=tables.size();Bytes out(12+n*16);PutLong(out,0,0x00010000);PutWord(out,4,n);
  size_t power=1;uint16_t selector=0;while(power*2<=n){power*=2;++selector;}
  PutWord(out,6,power*16);PutWord(out,8,selector);PutWord(out,10,n*16-power*16);size_t i=0,head=0;
  for(const auto& [tag,bytes]:tables){while(out.size()%4)out.push_back(0);const auto row=12+i++*16,offset=out.size();
    PutLong(out,row,tag);PutLong(out,row+4,Checksum(bytes));PutLong(out,row+8,offset);PutLong(out,row+12,bytes.size());
    if(tag==Tag("head")){head=offset;}
    out.insert(out.end(),bytes.begin(),bytes.end());
  }
  while(out.size()%4){out.push_back(0);}
  PutLong(out,head+8,0xb1b0afba-Checksum(out));EXPECT_EQ(Checksum(out),0xb1b0afbau);return out;
}
struct Spec {
  std::vector<std::pair<uint8_t,uint16_t>> mappings;
  std::u16string text;
  std::vector<bool> ink;
  // FreeType selects a Unicode charmap on face creation. Nameless Mac-only
  // maps have no selected charmap in native PDFium; the lower-level units
  // exercise those maps explicitly rather than assuming public Unicode input.
  bool missing_post=false,real_post=false,unicode=true,empty_first=false,small=false;
};
Bytes Font(const Spec& spec){
  const auto original=spec.small?text_fixture::Read(TestRoot()/"crates/pdfoxium-core/tests/fixtures/hint-optional.ttf"):Ahem();
  auto tables=TablesFrom(original);
  if(spec.small){EXPECT_EQ(Word(tables.at(Tag("maxp")),4),2);}
  Bytes cmap(12+262);PutWord(cmap,2,1);PutWord(cmap,4,spec.unicode?0:1);PutWord(cmap,6,0);PutLong(cmap,8,12);PutWord(cmap,14,262);
  for(const auto& [code,gid]:spec.mappings){if(gid>255)throw std::runtime_error("Format0 glyph does not fit its byte field");cmap[18+code]=uint8_t(gid);}
  tables[Tag("cmap")]=std::move(cmap);
  if(spec.missing_post)tables.erase(Tag("post"));
  else if(!spec.real_post){auto& post=tables.at(Tag("post"));post.resize(32);PutLong(post,0,0x00030000);}
  return Serialize(std::move(tables));
}
Bytes PublicFontData(const Api& api,FPDF_FONT font){
  size_t n=0;if(!api.FPDFFont_GetFontData(font,nullptr,0,&n)||n<12){ADD_FAILURE()<<"Expected embedded SFNT bytes";return {};}
  Bytes data(n);size_t written=0;if(!api.FPDFFont_GetFontData(font,data.data(),data.size(),&written)||written!=n){ADD_FAILURE()<<"Embedded font data size changed";return {};}return data;
}
void FontMappings(const Bytes& data,const Spec& spec){
  const auto tables=TablesFrom(data);const auto& map=tables.at(Tag("cmap"));ASSERT_EQ(Word(map,2),1);
  const size_t sub=Long(map,8);ASSERT_EQ(Word(map,sub),0);ASSERT_EQ(Word(map,sub+2),262);
  for(const auto& [code,gid]:spec.mappings)EXPECT_EQ(map.at(sub+6+code),gid);
}
void RetainedGlyphSlots(const Bytes& data,const Spec& spec){
  // PDF CIDToGID stays Identity. Subsetting may rewrite/drop cmap records, so
  // verify original glyph slots and contours rather than saved table bytes.
  const auto tables=TablesFrom(data);const auto& loca=tables.at(Tag("loca"));const auto& glyf=tables.at(Tag("glyf"));
  const auto& head=tables.at(Tag("head"));const auto& maxp=tables.at(Tag("maxp"));const bool wide=Word(head,50)!=0;
  auto offset=[&](uint16_t glyph){return wide?Long(loca,size_t(glyph)*4):uint32_t(Word(loca,size_t(glyph)*2))*2;};
  for(size_t i=0;i<spec.text.size();++i){
    uint16_t gid=0;
    if(spec.real_post)gid=Glyph(spec.text[i]);
    else for(const auto& entry:spec.mappings)if(entry.first==spec.text[i])gid=entry.second;
    ASSERT_NE(gid,0);ASSERT_GT(Word(maxp,4),gid);const auto start=offset(gid),end=offset(gid+1);
    ASSERT_LE(start,end);ASSERT_LE(end,glyf.size());
    if(spec.ink.at(i)){ASSERT_GT(end-start,10u);EXPECT_GT(static_cast<int16_t>(Word(glyf,start)),0);}
    else if(start!=end){
      // A subsetter may encode a blank slot either as an empty loca range or
      // as a valid zero-contour glyph. Its retained index and semantics matter.
      ASSERT_GE(end-start,10u);EXPECT_EQ(static_cast<int16_t>(Word(glyf,start)),0);
    }
  }
}
void Text(const Api& api,FPDF_PAGE page,const Spec& spec){
  Handle text(api.FPDFText_LoadPage(page),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
  EXPECT_EQ(text_fixture::Extract(api,text.get()),spec.text);ASSERT_EQ(api.FPDFText_CountChars(text.get()),spec.text.size());
  for(size_t i=0;i<spec.text.size();++i){EXPECT_EQ(api.FPDFText_GetUnicode(text.get(),i),spec.text[i]);double x=0,y=0;
    ASSERT_TRUE(api.FPDFText_GetCharOrigin(text.get(),i,&x,&y));EXPECT_NEAR(x,20+(spec.small?16.8:24)*i,.02);EXPECT_NEAR(y,40,.02);
  }
}
void Ink(const Raster& image,const Spec& spec){
  ASSERT_EQ(image.width,200);ASSERT_EQ(image.height,100);ASSERT_EQ(spec.ink.size(),spec.text.size());
  for(size_t i=0;i<spec.ink.size();++i){const auto at=(52u*200+(spec.small?25:32)+24*i)*4;for(int c=0;c<3;++c)EXPECT_EQ(image.pixels.at(at+c),spec.ink[i]?0:255)<<i;}
  for(int c=0;c<3;++c)EXPECT_EQ(image.pixels.at((5u*200+5)*4+c),255);
}
void Workflow(const Spec& spec){
  Artifacts artifacts;const auto data=Font(spec);std::array<Raster,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()){
    SCOPED_TRACE(writer->name());const auto& api=writer->api();Handle doc(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    Handle page(api.FPDFPage_New(doc.get(),0,200,100),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    if(spec.empty_first){Spec empty;const auto zero=Font(empty);Handle unused(api.FPDFText_LoadFont(doc.get(),zero.data(),zero.size(),FPDF_FONT_TRUETYPE,1),api.FPDFFont_Close);ASSERT_NE(unused.get(),nullptr);FontMappings(PublicFontData(api,unused.get()),empty);}
    Handle font(api.FPDFText_LoadFont(doc.get(),data.data(),data.size(),FPDF_FONT_TRUETYPE,1),api.FPDFFont_Close);ASSERT_NE(font.get(),nullptr);
    ASSERT_NO_FATAL_FAILURE(FontMappings(PublicFontData(api,font.get()),spec));
    if(spec.small){float width=0;ASSERT_TRUE(api.FPDFFont_GetGlyphWidth(font.get(),'A',24,&width));EXPECT_NEAR(width,16.8,.01);}
    ASSERT_NO_FATAL_FAILURE(RetainedGlyphSlots(PublicFontData(api,font.get()),spec));
    Handle object(api.FPDFPageObj_CreateTextObj(doc.get(),font.get(),24),api.FPDFPageObj_Destroy);ASSERT_NE(object.get(),nullptr);
    const auto wide=text_fixture::Wide(spec.text);ASSERT_TRUE(api.FPDFText_SetText(object.get(),wide.data()));
    const FS_MATRIX matrix{1,0,0,1,20,40};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object.get(),&matrix));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),object.release()));ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    ASSERT_NO_FATAL_FAILURE(Text(api,page.get(),spec));originals[backend]=Render(api,page.get(),200,100);
    ASSERT_NO_FATAL_FAILURE(Ink(originals[backend],spec));
    for(FPDF_DWORD flags:{FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}){
      const auto label=writer->name()+"-byte-cmap-"+std::to_string(flags);const auto saved=SaveDocument(api,doc.get(),artifacts,label,flags);ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()){
        SCOPED_TRACE(reader->name());const auto& read=reader->api();Handle reloaded(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reloaded.get(),nullptr);
        Handle after(read.FPDF_LoadPage(reloaded.get(),0),read.FPDF_ClosePage);ASSERT_NE(after.get(),nullptr);ASSERT_NO_FATAL_FAILURE(Text(read,after.get(),spec));
        ASSERT_EQ(read.FPDFPage_CountObjects(after.get()),1);auto text=read.FPDFPage_GetObject(after.get(),0);ASSERT_EQ(read.FPDFPageObj_GetType(text),FPDF_PAGEOBJ_TEXT);
        const auto retained=read.FPDFTextObj_GetFont(text);ASSERT_NE(retained,nullptr);ASSERT_NO_FATAL_FAILURE(RetainedGlyphSlots(PublicFontData(read,retained),spec));
        const auto image=Render(read,after.get(),200,100);ASSERT_NO_FATAL_FAILURE(Ink(image,spec));if(spec.small)text_fixture::CompareTextRaster(originals[backend],image,artifacts,label+reader->name());else CompareRaster(originals[backend],image,artifacts,label+reader->name());
      }
    }
    ++backend;
  }
  if(spec.small)text_fixture::CompareTextRaster(originals[0],originals[1],artifacts,"format0-native-rust");
  else CompareRaster(originals[0],originals[1],artifacts,"format0-native-rust");
}
TEST(CoverageFontCharmapEnumeration,NamelessUnicodeFormat0RetainsItsFirstAsciiGlyph){Workflow({{{65,Glyph('A')}},u"A",{true}});}
TEST(CoverageFontCharmapEnumeration,SparseUnicodeFormat0SkipsZeroGlyphSlotsWithoutMovingGlyphs){Workflow({{{65,Glyph('A')},{67,Glyph('C')}},u"AC",{true,true}});}
TEST(CoverageFontCharmapEnumeration,TheLastByteCodeIsARealMappingAndNotAnEndSentinel){Workflow({{{255,Glyph('D')}},u"\u00ff",{true}});}
TEST(CoverageFontCharmapEnumeration,AliasedCodesShareAGlyphWithoutChangingItsRetainedSlot){Workflow({{{65,Glyph('A')},{66,Glyph('A')}},u"B",{true}});}
TEST(CoverageFontCharmapEnumeration,AnEncodedBlankGlyphKeepsItsAdvanceAndLogicalCharacter){Workflow({{{65,Glyph('A')},{66,Glyph(' ')},{67,Glyph('C')}},u"ABC",{true,false,true}});}
TEST(CoverageFontCharmapEnumeration,AnAbsentPostTableDoesNotSuppressTheUnicodeByteMap){Spec s{{{65,Glyph('A')},{67,Glyph('C')}},u"AC",{true,true}};s.missing_post=true;Workflow(s);}
TEST(CoverageFontCharmapEnumeration,AUnicodeFormat0RecordUsesTheSameValidByteMappings){Spec s{{{65,Glyph('A')},{66,Glyph('B')},{67,Glyph('C')}},u"ABC",{true,true,true}};s.unicode=true;Workflow(s);}
TEST(CoverageFontCharmapEnumeration,DescendingGlyphIdsPreserveAscendingCharacterOrder){Workflow({{{65,Glyph('C')},{67,Glyph('A')}},u"AC",{true,true}});}
TEST(CoverageFontCharmapEnumeration,AnEmptyByteMapDoesNotPoisonTheNextEmbeddedFont){Spec s{{{65,Glyph('A')},{66,Glyph('B')}},u"AB",{true,true}};s.empty_first=true;Workflow(s);}
TEST(CoverageFontCharmapEnumeration,RealPostNamesTakePriorityOverAConflictingMacMapping){Spec s{{{65,Glyph(' ')}},u"A",{true}};s.real_post=true;s.unicode=false;Workflow(s);}
// The outlines/metrics/maxp remain a valid two-glyph font; only its cmap tail
// deliberately refers to absent glyphs. Exercise the valid public mapping and
// save path without imposing fallback-outline behavior on invalid characters.
TEST(CoverageFontCharmapEnumeration,OutOfRangeByteMappingsDoNotAffectTheValidRetainedGlyph){Spec s{{{65,1},{66,2},{67,255}},u"A",{true}};s.small=true;Workflow(s);}
}  // namespace
}  // namespace pdfoxium::integration
