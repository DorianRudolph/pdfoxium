#include "text_fixture.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <string>
#include <tuple>
#include <vector>

namespace pdfoxium::integration {
namespace {
using namespace text_fixture;
constexpr std::array<const char*,14> kStandardFonts={
 "Courier","Courier-Bold","Courier-Oblique","Courier-BoldOblique",
 "Helvetica","Helvetica-Bold","Helvetica-Oblique","Helvetica-BoldOblique",
 "Times-Roman","Times-Bold","Times-Italic","Times-BoldItalic","Symbol","ZapfDingbats"};

std::string Name(const Api& api,FPDF_FONT font,bool family) {
  const auto get=family?api.FPDFFont_GetFamilyName:api.FPDFFont_GetBaseFontName;
  const auto size=get(font,nullptr,0);EXPECT_GT(size,0u);
  if(!size)return {};
  std::vector<char> output(size+1,'~');
  EXPECT_EQ(get(font,output.data(),size),size);EXPECT_EQ(output[size-1],0);EXPECT_EQ(output[size],'~');
  const std::string expected(output.data());
  for(size_t capacity:{size-1,size,size+1}) {
    std::vector<char> buffer(capacity+2,'~');
    EXPECT_EQ(get(font,buffer.data(),capacity),size);
    if(capacity>=size) { EXPECT_EQ(std::string(buffer.data()),expected); }
    else { EXPECT_TRUE(std::all_of(buffer.begin(),buffer.end(),[](char c){return c=='~';})); }
    EXPECT_EQ(buffer[capacity],'~');EXPECT_EQ(buffer[capacity+1],'~');
  }
  return expected;
}

std::vector<uint8_t> FontData(const Api& api,FPDF_FONT font) {
  size_t size=0;
  EXPECT_TRUE(api.FPDFFont_GetFontData(font,nullptr,0,&size));EXPECT_GT(size,4u);
  if(size<=4)return {};
  std::vector<uint8_t> result;
  for(size_t capacity:{size-1,size,size+1}) {
    std::vector<uint8_t> buffer(capacity+2,0xa5);size_t reported=0;
    EXPECT_TRUE(api.FPDFFont_GetFontData(font,buffer.data(),capacity,&reported));EXPECT_EQ(reported,size);
    if(capacity<size) { EXPECT_TRUE(std::all_of(buffer.begin(),buffer.end(),[](auto c){return c==0xa5;})); }
    else { result.assign(buffer.begin(),buffer.begin()+size); }
    EXPECT_EQ(buffer[capacity],0xa5);EXPECT_EQ(buffer[capacity+1],0xa5);
  }
  const bool sfnt=(result[0]==0 && result[1]==1 && result[2]==0 && result[3]==0) ||
    std::equal(result.begin(),result.begin()+4,"OTTO") || std::equal(result.begin(),result.begin()+4,"ttcf") ||
    std::equal(result.begin(),result.begin()+4,"true");
  const bool cff=(result[0]==1 || result[0]==2) && result[1]==0 && result[2]>=4 && result[2]<result.size();
  const bool type1=(result[0]=='%' && result[1]=='!') || (result[0]==0x80 && result[1]==1);
  EXPECT_TRUE(sfnt || cff || type1)<<"Decoded public font data has no supported font-program signature";
  return result;
}

void BasicMetrics(const Api& api,FPDF_FONT font) {
  float ascent1=-1,descent1=1;
  ASSERT_TRUE(api.FPDFFont_GetAscent(font,1,&ascent1));
  ASSERT_TRUE(api.FPDFFont_GetDescent(font,1,&descent1));
  EXPECT_GE(ascent1,0);EXPECT_LE(descent1,0);
  for(float size:{0.f,1.f,12.f,25.5f}) {
    float ascent=-1,descent=1;
    ASSERT_TRUE(api.FPDFFont_GetAscent(font,size,&ascent));ASSERT_TRUE(api.FPDFFont_GetDescent(font,size,&descent));
    EXPECT_NEAR(ascent,ascent1*size,0.001);EXPECT_NEAR(descent,descent1*size,0.001);
    for(uint32_t glyph:{0u,32u,static_cast<uint32_t>('i'),static_cast<uint32_t>('W'),0x10ffffu}) {
      float unit=-1,width=-1;
      ASSERT_TRUE(api.FPDFFont_GetGlyphWidth(font,glyph,1,&unit));
      ASSERT_TRUE(api.FPDFFont_GetGlyphWidth(font,glyph,size,&width));
      EXPECT_GE(width,0);EXPECT_NEAR(width,unit*size,0.001);
    }
  }
}

void ObjectTextBuffer(const Api& api,FPDF_PAGEOBJECT object,FPDF_TEXTPAGE text,
                      const std::u16string& expected) {
  const auto needed=api.FPDFTextObj_GetText(object,text,nullptr,0);
  ASSERT_EQ(needed,(expected.size()+1)*2);
  for(unsigned long capacity:{needed-1,needed,needed+2}) {
    std::vector<unsigned short> buffer((capacity+1)/2+2,0xaaaa);
    EXPECT_EQ(api.FPDFTextObj_GetText(object,text,buffer.data(),capacity),needed);
    if(capacity>=needed) {
      EXPECT_EQ(std::u16string(buffer.begin(),buffer.begin()+expected.size()),expected);
      EXPECT_EQ(buffer[expected.size()],0);
    } else {
      EXPECT_TRUE(std::all_of(buffer.begin(),buffer.end(),[](auto value){return value==0xaaaa;}));
    }
    EXPECT_EQ(buffer[(capacity+1)/2],0xaaaa);
  }
}

using GlyphSegment=std::tuple<int,float,float,bool>;
std::vector<GlyphSegment> Outline(const Api& api,FPDF_FONT font,uint32_t unicode,float size) {
  std::vector<GlyphSegment> result;
  const auto path=api.FPDFFont_GetGlyphPath(font,unicode,size);EXPECT_NE(path,nullptr);
  if(!path)return result;
  const int count=api.FPDFGlyphPath_CountGlyphSegments(path);EXPECT_GT(count,0);
  float start_x=0,start_y=0;
  for(int index=0;index<count;++index) {
    const auto segment=api.FPDFGlyphPath_GetGlyphPathSegment(path,index);EXPECT_NE(segment,nullptr);
    if(!segment)continue;
    float x=0,y=0;EXPECT_TRUE(api.FPDFPathSegment_GetPoint(segment,&x,&y));
    const int type=api.FPDFPathSegment_GetType(segment);const bool close=api.FPDFPathSegment_GetClose(segment);
    if(type==FPDF_SEGMENT_MOVETO) { start_x=x;start_y=y; }
    const bool implicit=close&&(std::abs(x-start_x)>.00001f||std::abs(y-start_y)>.00001f);
    result.emplace_back(type,x,y,close&&!implicit);
    if(implicit)result.emplace_back(FPDF_SEGMENT_LINETO,start_x,start_y,true);
  }
  return result;
}

void SameOutline(const std::vector<GlyphSegment>& expected,const std::vector<GlyphSegment>& actual) {
  ASSERT_EQ(expected.size(),actual.size());
  for(size_t index=0;index<expected.size();++index) {
    EXPECT_EQ(std::get<0>(expected[index]),std::get<0>(actual[index]));
    EXPECT_NEAR(std::get<1>(expected[index]),std::get<1>(actual[index]),.01);
    EXPECT_NEAR(std::get<2>(expected[index]),std::get<2>(actual[index]),.01);
    EXPECT_EQ(std::get<3>(expected[index]),std::get<3>(actual[index]));
  }
}

TEST(Fonts, AllStandardFontsHaveNamesMetricsOwnedLoadsAndPersistedText) {
  Artifacts artifacts;
  for(const char* name:kStandardFonts) {
    std::array<Raster,2> pixels;std::array<std::string,2> base_names,family_names;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name()+"/"+name);const auto& api=library->api();
      Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
      Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
      Handle loaded(api.FPDFText_LoadStandardFont(document.get(),name),api.FPDFFont_Close);
      ASSERT_NE(loaded.get(),nullptr);EXPECT_EQ(api.FPDFFont_GetIsEmbedded(loaded.get()),0);
      base_names[backend]=Name(api,loaded.get(),false);EXPECT_EQ(base_names[backend],name);
      family_names[backend]=Name(api,loaded.get(),true);EXPECT_FALSE(family_names[backend].empty());
      if(std::string(name).find('-')!=std::string::npos) { EXPECT_NE(family_names[backend],base_names[backend]); }
      // Stock font dictionaries have no FontWeight/StemV descriptor. The
      // public metadata getter reports its zero StemV fallback, not the
      // inferred boldness of the substituted face.
      EXPECT_EQ(api.FPDFFont_GetWeight(loaded.get()),0);
      EXPECT_FALSE(FontData(api,loaded.get()).empty());BasicMetrics(api,loaded.get());
      int angle=123;ASSERT_TRUE(api.FPDFFont_GetItalicAngle(loaded.get(),&angle));
      // Standard-font resources have no descriptor ItalicAngle. Font style
      // names do not supply a missing PDF descriptor value.
      EXPECT_EQ(angle,0);
      Handle object(api.FPDFPageObj_CreateTextObj(document.get(),loaded.get(),20),api.FPDFPageObj_Destroy);
      ASSERT_NE(object.get(),nullptr);EXPECT_EQ(api.FPDFPageObj_GetType(object.get()),FPDF_PAGEOBJ_TEXT);
      const auto words=Wide(std::string(name)=="Symbol"?u"\u0391\u0392\u0393":
                            std::string(name)=="ZapfDingbats"?u"\u2701\u2702\u2703":u"Hello 123");
      ASSERT_TRUE(api.FPDFText_SetText(object.get(),words.data()));
      const FS_MATRIX matrix{1,0,0,1,20,120};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object.get(),&matrix));
      const auto borrowed=api.FPDFTextObj_GetFont(object.get());ASSERT_NE(borrowed,nullptr);
      EXPECT_EQ(Name(api,borrowed,false),name);
      // The text object keeps the font resource alive after the separately
      // loaded caller-owned font handle is closed.
      loaded.reset();
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),object.release()));
      auto created=Add(api,document.get(),page.get(),u"x",20,70,12,name);ASSERT_NE(created,nullptr);
      pixels[backend]=Render(api,page.get(),400,240);
      size_t ink=0;
      for(size_t pixel=0;pixel+3<pixels[backend].pixels.size();pixel+=4)
        ink+=pixels[backend].pixels[pixel]<240 || pixels[backend].pixels[pixel+1]<240 || pixels[backend].pixels[pixel+2]<240;
      EXPECT_GT(ink,20u);
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
      CrossReload(*library,document.get(),artifacts,std::string("standard-")+name,
          [name](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE text) {
            EXPECT_EQ(read.FPDFPage_CountObjects(p),2);
            const auto obj=read.FPDFPage_GetObject(p,0);ASSERT_NE(obj,nullptr);
            const auto font=read.FPDFTextObj_GetFont(obj);ASSERT_NE(font,nullptr);
            EXPECT_EQ(Name(read,font,false),name);EXPECT_EQ(read.FPDFFont_GetIsEmbedded(font),0);
            EXPECT_GT(read.FPDFText_CountChars(text),0);
          });++backend;
    }
    EXPECT_EQ(base_names[0],base_names[1]);EXPECT_EQ(family_names[0],family_names[1]);
    CompareTextRaster(pixels[0],pixels[1],artifacts,std::string("standard-")+name);
  }
}

TEST(Fonts, TextCreationSizesAndInvalidNamesRespectOwnership) {
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle font(api.FPDFText_LoadStandardFont(document.get(),"Helvetica"),api.FPDFFont_Close);
    ASSERT_NE(font.get(),nullptr);
    for(float size:{0.f,.5f,12.f,72.f}) {
      Handle a(api.FPDFPageObj_NewTextObj(document.get(),"Helvetica",size),api.FPDFPageObj_Destroy);
      Handle b(api.FPDFPageObj_CreateTextObj(document.get(),font.get(),size),api.FPDFPageObj_Destroy);
      ASSERT_NE(a.get(),nullptr);ASSERT_NE(b.get(),nullptr);
      float actual=-1;EXPECT_TRUE(api.FPDFTextObj_GetFontSize(a.get(),&actual));EXPECT_FLOAT_EQ(actual,size);
      EXPECT_TRUE(api.FPDFTextObj_GetFontSize(b.get(),&actual));EXPECT_FLOAT_EQ(actual,size);
    }
    EXPECT_EQ(api.FPDFText_LoadStandardFont(nullptr,"Helvetica"),nullptr);
    EXPECT_EQ(api.FPDFText_LoadStandardFont(document.get(),nullptr),nullptr);
    EXPECT_EQ(api.FPDFText_LoadStandardFont(document.get(),""),nullptr);
    EXPECT_EQ(api.FPDFText_LoadStandardFont(document.get(),"NoSuchFont"),nullptr);
    EXPECT_EQ(api.FPDFPageObj_CreateTextObj(nullptr,font.get(),12),nullptr);
    EXPECT_EQ(api.FPDFPageObj_CreateTextObj(document.get(),nullptr,12),nullptr);
    EXPECT_EQ(api.FPDFPageObj_NewTextObj(nullptr,"Helvetica",12),nullptr);
    EXPECT_EQ(api.FPDFPageObj_NewTextObj(document.get(),nullptr,12),nullptr);
    EXPECT_EQ(api.FPDFPageObj_NewTextObj(document.get(),"NoSuchFont",12),nullptr);
    api.FPDFFont_Close(nullptr);
  }
}

TEST(Fonts, FontSizeEditsPreserveMatrixTextAndRoundTripVisualSize) {
  Artifacts artifacts;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
    const auto object=Add(api,document.get(),page.get(),u"Size",20,120,10.5);
    ASSERT_NE(object,nullptr);const FS_MATRIX matrix{2,0,0,1.5f,20,120};
    ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object,&matrix));
    for(float size:{0.f,.5f,10.5f,24.f}) {
      ASSERT_TRUE(api.FPDFTextObj_SetFontSize(object,size));float actual=-1;
      ASSERT_TRUE(api.FPDFTextObj_GetFontSize(object,&actual));EXPECT_FLOAT_EQ(actual,size);
      EXPECT_FALSE(api.FPDFTextObj_SetFontSize(object,-.5f));
      ASSERT_TRUE(api.FPDFTextObj_GetFontSize(object,&actual));EXPECT_FLOAT_EQ(actual,size);
      FS_MATRIX unchanged{};ASSERT_TRUE(api.FPDFPageObj_GetMatrix(object,&unchanged));
      EXPECT_FLOAT_EQ(unchanged.a,2);EXPECT_FLOAT_EQ(unchanged.d,1.5f);EXPECT_FLOAT_EQ(unchanged.e,20);
    }
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
    CrossReload(*library,document.get(),artifacts,"size-edits",[](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE text) {
      const auto object=read.FPDFPage_GetObject(p,0);ASSERT_NE(object,nullptr);
      float size=-1;ASSERT_TRUE(read.FPDFTextObj_GetFontSize(object,&size));EXPECT_FLOAT_EQ(size,24);
      EXPECT_EQ(Extract(read,text),u"Size");
    });
  }
}

TEST(Fonts, TextReplacementCharcodesAndExplicitPositionsPersist) {
  Artifacts artifacts;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
    const auto object=Add(api,document.get(),page.get(),u"Original",20,120,20);ASSERT_NE(object,nullptr);
    {
      Handle empty(api.FPDFPageObj_NewTextObj(document.get(),"Courier",20),api.FPDFPageObj_Destroy);
      ASSERT_NE(empty.get(),nullptr);EXPECT_FALSE(api.FPDFText_SetPositions(empty.get(),nullptr,0));
      ASSERT_TRUE(api.FPDFText_SetText(empty.get(),Wide(u"A").data()));
      const float one=10;EXPECT_FALSE(api.FPDFText_SetPositions(empty.get(),&one,1));
      EXPECT_FALSE(api.FPDFText_SetPositions(empty.get(),nullptr,0));
      const uint32_t zero=0;ASSERT_TRUE(api.FPDFText_SetCharcodes(empty.get(),&zero,1));
      EXPECT_FALSE(api.FPDFText_SetPositions(empty.get(),nullptr,0));
      ASSERT_TRUE(api.FPDFText_SetText(empty.get(),Wide(u"AB").data()));
      ASSERT_TRUE(api.FPDFTextObj_SetFontSize(empty.get(),0));
      EXPECT_FALSE(api.FPDFText_SetPositions(empty.get(),&one,1));
    }
    auto replacement=Wide(u"Replaced");ASSERT_TRUE(api.FPDFText_SetText(object,replacement.data()));
    {Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);EXPECT_EQ(Extract(api,text.get()),u"Replaced");ObjectTextBuffer(api,object,text.get(),u"Replaced");}
    const std::array<uint32_t,4> chars={'A','B','C','D'};
    ASSERT_TRUE(api.FPDFText_SetCharcodes(object,chars.data(),chars.size()));
    for(const auto& positions:{std::array<float,3>{12,30,42},{12,12,30},{-4,12,24}}) {
      ASSERT_TRUE(api.FPDFText_SetPositions(object,positions.data(),positions.size()));
      Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
      int source_index=0;
      for(int i=0;i<api.FPDFText_CountChars(text.get());++i) {
        if(api.FPDFText_IsGenerated(text.get(),i))continue;
        ASSERT_LT(source_index,4);
        double x=0,y=0;ASSERT_TRUE(api.FPDFText_GetCharOrigin(text.get(),i,&x,&y));
        EXPECT_NEAR(x,20+(source_index?positions[source_index-1]:0),.001);EXPECT_NEAR(y,120,.001);
        ++source_index;
      }
      EXPECT_EQ(source_index,4);
    }
    const std::array<float,3> positions={12,30,42};ASSERT_TRUE(api.FPDFText_SetPositions(object,positions.data(),3));
    EXPECT_FALSE(api.FPDFText_SetPositions(object,positions.data(),2));
    const std::array<float,4> too_many={1,2,3,4};EXPECT_FALSE(api.FPDFText_SetPositions(object,too_many.data(),4));
    EXPECT_FALSE(api.FPDFText_SetPositions(object,nullptr,3));
    EXPECT_FALSE(api.FPDFText_SetText(object,Wide(u"").data()));EXPECT_FALSE(api.FPDFText_SetText(object,nullptr));
    EXPECT_FALSE(api.FPDFText_SetCharcodes(object,nullptr,1));EXPECT_FALSE(api.FPDFText_SetCharcodes(object,chars.data(),0));
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
    CrossReload(*library,document.get(),artifacts,"positions",[](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE text) {
      const auto extracted=Extract(read,text);
      std::u16string source;
      for(int i=0;i<read.FPDFText_CountChars(text);++i)
        if(!read.FPDFText_IsGenerated(text,i))source.push_back(static_cast<char16_t>(read.FPDFText_GetUnicode(text,i)));
      EXPECT_EQ(source,u"ABCD");
      EXPECT_NE(extracted.find(u"AB"),std::u16string::npos);
      // Explicit advances may add generated spaces without changing source text.
      const auto object=read.FPDFPage_GetObject(p,0);ASSERT_NE(object,nullptr);
      EXPECT_EQ(read.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_TEXT);
      EXPECT_FALSE(Extract(read,text).empty());
    });
  }
}

TEST(Fonts, EveryTextRenderingModeAndInvalidValuesPersist) {
  Artifacts artifacts;
  for(int mode=0;mode<=7;++mode) {
    std::array<Raster,2> observations;size_t backend=0;
    for(const auto* library:Implementations()) {
      const auto& api=library->api();SCOPED_TRACE(library->name()+" mode="+std::to_string(mode));
      Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
      Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
      Handle fill(api.FPDFPageObj_CreateNewRect(0,0,400,240),api.FPDFPageObj_Destroy);
      ASSERT_TRUE(api.FPDFPageObj_SetFillColor(fill.get(),48,160,96,255));
      ASSERT_TRUE(api.FPDFPath_SetDrawMode(fill.get(),FPDF_FILLMODE_WINDING,0));
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),fill.release()));
      const auto object=Add(api,document.get(),page.get(),u"Mode",40,100,60);ASSERT_NE(object,nullptr);
      ASSERT_TRUE(api.FPDFPageObj_SetFillColor(object,192,32,64,127));
      ASSERT_TRUE(api.FPDFPageObj_SetStrokeColor(object,32,96,224,254));
      ASSERT_TRUE(api.FPDFPageObj_SetStrokeWidth(object,2));
      ASSERT_TRUE(api.FPDFTextObj_SetTextRenderMode(object,static_cast<FPDF_TEXT_RENDERMODE>(mode)));
      EXPECT_EQ(api.FPDFTextObj_GetTextRenderMode(object),mode);
      for(int invalid:{-1,8,1000}) {
        EXPECT_FALSE(api.FPDFTextObj_SetTextRenderMode(object,static_cast<FPDF_TEXT_RENDERMODE>(invalid)));
        EXPECT_EQ(api.FPDFTextObj_GetTextRenderMode(object),mode);
      }
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      observations[backend++]=Render(api,page.get(),400,240);
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
      CrossReload(*library,document.get(),artifacts,"mode-"+std::to_string(mode),[mode](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE) {
        const auto object=read.FPDFPage_GetObject(p,1);ASSERT_NE(object,nullptr);
        EXPECT_EQ(read.FPDFTextObj_GetTextRenderMode(object),mode);
      });
    }
    CompareTextRaster(observations[0],observations[1],artifacts,"mode-"+std::to_string(mode));
  }
}

TEST(Fonts, VerticalFontAdvancesAndExplicitPositionsRemainVerticalAfterSave) {
  // Constructors currently create horizontal fonts. This existing embedded
  // UniGB-UTF16-V fixture supplies explicit W2/DW2 metrics through public APIs.
  const auto bytes=Resource("vertical_text.pdf");ASSERT_FALSE(bytes.empty());
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
    ASSERT_NE(page.get(),nullptr);const auto object=api.FPDFPage_GetObject(page.get(),0);ASSERT_NE(object,nullptr);
    ASSERT_EQ(api.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_TEXT);
    const auto font=api.FPDFTextObj_GetFont(object);ASSERT_NE(font,nullptr);
    for(float size:{0.f,1.f,12.f,24.f}) {
      float width=999;ASSERT_TRUE(api.FPDFFont_GetGlyphWidth(font,'H',size,&width));EXPECT_NEAR(width,-.918*size,.001);
      ASSERT_TRUE(api.FPDFFont_GetGlyphWidth(font,'e',size,&width));EXPECT_NEAR(width,-.723*size,.001);
    }
    const std::array<uint32_t,3> codes={'H','e','l'};
    ASSERT_TRUE(api.FPDFText_SetCharcodes(object,codes.data(),codes.size()));
    const std::array<float,2> positions={-18,-42};ASSERT_TRUE(api.FPDFText_SetPositions(object,positions.data(),positions.size()));
    const auto inspect=[](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE text) {
      const auto object=read.FPDFPage_GetObject(p,0);ASSERT_NE(object,nullptr);
      const auto font=read.FPDFTextObj_GetFont(object);ASSERT_NE(font,nullptr);
      float width=999;ASSERT_TRUE(read.FPDFFont_GetGlyphWidth(font,'H',12,&width));EXPECT_NEAR(width,-11.016,.001);
      std::vector<std::array<double,2>> origins;
      for(int index=0;index<read.FPDFText_CountChars(text);++index)
        if(read.FPDFText_GetTextObject(text,index)==object && !read.FPDFText_IsGenerated(text,index)) {
          std::array<double,2> value;ASSERT_TRUE(read.FPDFText_GetCharOrigin(text,index,&value[0],&value[1]));origins.push_back(value);
        }
      ASSERT_EQ(origins.size(),3u);
      EXPECT_NEAR(origins[1][1]-origins[0][1],-18+12*(.818-.623),.005);
      EXPECT_NEAR(origins[2][1]-origins[0][1],-42,.005);
    };
    {
      Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);inspect(api,page.get(),text.get());
    }
    images[backend++]=Render(api,page.get(),400,240);
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));page.reset();
    CrossReload(*library,document.get(),artifacts,"vertical-positions",inspect);
  }
  CompareTextRaster(images[0],images[1],artifacts,"vertical-position-backends");
}

TEST(Fonts, TextClipModesRestrictSubsequentPaintingAndSurviveRegeneration) {
  const auto data=Resource("fonts/ahem/Ahem.ttf");ASSERT_FALSE(data.empty());
  const std::string program(data.begin(),data.end());Artifacts artifacts;
  for(int mode:{4,5,6,7}) {
    // Public page-object insertion isolates each object's graphics state.
    // A source content stream is needed to paint after ET within the same
    // text clipping state. Ahem's solid rectangle gives independent pixels.
    // Fractional positioning avoids a FreeType/fontations outline ULP landing
    // on opposite sides of an integer: rectangular clipping deliberately uses
    // floor/ceil outer bounds in CFX_AggDeviceDriver::SetClip_PathFill.
    const auto bytes=PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 240] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
      "<< /Type /Font /Subtype /TrueType /BaseFont /Ahem /FirstChar 65 /LastChar 65 /Widths [1000] /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R >>",
      PdfStream("","q 1 0 0 rg 0 0 1 RG 2 w BT /F1 60 Tf "+std::to_string(mode)+" Tr 40.25 100.25 Td (A) Tj ET 0 1 0 rg 0 0 400 240 re f Q"),
      "<< /Type /FontDescriptor /FontName /Ahem /Flags 32 /FontBBox [0 -200 1000 800] /ItalicAngle 0 /Ascent 800 /Descent -200 /StemV 80 /FontFile2 7 0 R >>",
      PdfStream("/Length1 "+std::to_string(data.size()),program)});
    std::array<Raster,2> images;size_t backend=0;
    const auto inspect=[mode](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE text,bool clipped) {
      EXPECT_EQ(Extract(read,text),u"A");
      const auto object=read.FPDFText_GetTextObject(text,0);ASSERT_NE(object,nullptr);
      EXPECT_EQ(read.FPDFTextObj_GetTextRenderMode(object),mode);
      double l,r,b,t;ASSERT_TRUE(read.FPDFText_GetCharBox(text,0,&l,&r,&b,&t));
      const auto raster=Render(read,p,400,240);const int x=static_cast<int>((l+r)/2),y=240-static_cast<int>((b+t)/2);
      ASSERT_GE(x,0);ASSERT_LT(x,raster.width);ASSERT_GE(y,0);ASSERT_LT(y,raster.height);
      const auto center=(static_cast<size_t>(y)*raster.width+x)*4;
      EXPECT_EQ(raster.pixels[center],0);EXPECT_EQ(raster.pixels[center+1],255);EXPECT_EQ(raster.pixels[center+2],0);
      EXPECT_EQ(raster.pixels[0],clipped?255:0);EXPECT_EQ(raster.pixels[1],255);EXPECT_EQ(raster.pixels[2],clipped?255:0);
      size_t green=0;
      for(size_t pixel=0;pixel+3<raster.pixels.size();pixel+=4)
        green+=raster.pixels[pixel]==0&&raster.pixels[pixel+1]==255&&raster.pixels[pixel+2]==0;
      if(clipped) { EXPECT_GT(green,3000u);EXPECT_LT(green,4000u); }
      else { EXPECT_EQ(green,96000u); }
    };
    for(const auto* library:Implementations()) {
      const auto& api=library->api();SCOPED_TRACE(library->name()+" clip="+std::to_string(mode));
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      {
        Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);inspect(api,page.get(),text.get(),true);
        const auto object=api.FPDFText_GetTextObject(text.get(),0);ASSERT_NE(object,nullptr);
        ASSERT_TRUE(api.FPDFTextObj_SetTextRenderMode(object,static_cast<FPDF_TEXT_RENDERMODE>(mode)));
      }
      images[backend++]=Render(api,page.get(),400,240);ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      const bool retained=library->name()=="rust";
      // Live cached objects retain their clip after generation. Pinned native
      // serialization omits it, observable only after saving and reopening.
      {
        Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);inspect(api,page.get(),text.get(),true);
      }
      const auto regenerated=Render(api,page.get(),400,240);
      if(retained)CompareRaster(images[backend-1],regenerated,artifacts,"retained-text-clip-"+std::to_string(mode));
      page.reset();const auto saved=SaveDocument(api,document.get(),artifacts,library->name()+"-text-clip-"+std::to_string(mode),FPDF_NO_INCREMENTAL);
      ASSERT_FALSE(saved.empty());
      std::array<Raster,2> saved_images;size_t reader_index=0;
      for(const auto* reader:Implementations()) {
        const auto& read=reader->api();SCOPED_TRACE(library->name()+"-to-"+reader->name());
        Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);
        ASSERT_NE(reopened.get(),nullptr);Handle reloaded(read.FPDF_LoadPage(reopened.get(),0),read.FPDF_ClosePage);ASSERT_NE(reloaded.get(),nullptr);
        Handle text(read.FPDFText_LoadPage(reloaded.get()),read.FPDFText_ClosePage);inspect(read,reloaded.get(),text.get(),retained);
        saved_images[reader_index++]=Render(read,reloaded.get(),400,240);
        if(retained)CompareTextRaster(regenerated,saved_images[reader_index-1],artifacts,library->name()+"-to-"+reader->name()+"-clip-"+std::to_string(mode));
      }
      CompareTextRaster(saved_images[0],saved_images[1],artifacts,library->name()+"-clip-readers-"+std::to_string(mode));
    }
    CompareTextRaster(images[0],images[1],artifacts,"text-clip-backends-"+std::to_string(mode));
  }
}

TEST(Fonts, StandaloneAndInsertedTextBitmapsScaleAndRetainOwnership) {
  Artifacts artifacts;
  std::array<std::vector<Raster>,2> observations;size_t backend=0;
  for(bool embedded:{false,true}) {
  observations={};backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
    const auto data=embedded?Resource("fonts/ahem/Ahem.ttf"):std::vector<uint8_t>{};
    Handle font(embedded?api.FPDFText_LoadFont(document.get(),data.data(),data.size(),FPDF_FONT_TRUETYPE,0):nullptr,api.FPDFFont_Close);
    if(embedded) { ASSERT_NE(font.get(),nullptr); }
    Handle object(embedded?api.FPDFPageObj_CreateTextObj(document.get(),font.get(),32):
                           api.FPDFPageObj_NewTextObj(document.get(),"Courier",32),api.FPDFPageObj_Destroy);
    ASSERT_NE(object.get(),nullptr);ASSERT_TRUE(api.FPDFText_SetText(object.get(),Wide(u"Bitmap").data()));
    ASSERT_TRUE(api.FPDFPageObj_SetFillColor(object.get(),192,32,64,255));
    ASSERT_TRUE(api.FPDFPageObj_SetStrokeColor(object.get(),32,96,224,255));
    ASSERT_TRUE(api.FPDFPageObj_SetStrokeWidth(object.get(),.5));
    ASSERT_TRUE(api.FPDFTextObj_SetTextRenderMode(object.get(),FPDF_TEXTRENDERMODE_FILL_STROKE));
    FS_MATRIX transform{1,.125f,0,1,20,100};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object.get(),&transform));
    for(bool inserted:{false,true}) {
      const auto text=object.get();
      if(inserted) { ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),object.release())); }
      for(float scale:{.5f,1.f,2.f}) {
        Handle bitmap(api.FPDFTextObj_GetRenderedBitmap(document.get(),inserted?page.get():nullptr,text,scale),api.FPDFBitmap_Destroy);
        ASSERT_NE(bitmap.get(),nullptr);EXPECT_GT(api.FPDFBitmap_GetWidth(bitmap.get()),0);
        EXPECT_GT(api.FPDFBitmap_GetHeight(bitmap.get()),0);
        observations[backend].push_back(Bitmap(api,bitmap.get()));
      }
      EXPECT_EQ(api.FPDFTextObj_GetRenderedBitmap(document.get(),nullptr,text,0),nullptr);
      EXPECT_EQ(api.FPDFTextObj_GetRenderedBitmap(document.get(),nullptr,text,-1),nullptr);
      if(inserted) {
        ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
        CrossReload(*library,document.get(),artifacts,std::string("text-bitmap-")+(embedded?"embedded":"standard"),[](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE) {
          const auto object=read.FPDFPage_GetObject(p,0);ASSERT_NE(object,nullptr);
          Handle rendered(read.FPDFTextObj_GetRenderedBitmap(nullptr,p,object,1),read.FPDFBitmap_Destroy);
          EXPECT_EQ(rendered.get(),nullptr);
        });
      }
    }
    ++backend;
  }
  ASSERT_EQ(observations[0].size(),observations[1].size());
  for(size_t index=0;index<observations[0].size();++index)
    CompareTextRaster(observations[0][index],observations[1][index],artifacts,std::string("text-bitmap-")+(embedded?"embedded-":"standard-")+std::to_string(index));
  }
}

TEST(Fonts, EmbeddedTrueTypeAndType1FontsCopyDataAndRetainSparseGlyphs) {
  Artifacts artifacts;
  struct Input { const char* path; int type; bool cid; const char16_t* text; };
  const std::array<Input,4> inputs={{{"fonts/ahem/Ahem.ttf",FPDF_FONT_TRUETYPE,false,u"A Z"},
    {"fonts/ahem/Ahem.ttf",FPDF_FONT_TRUETYPE,true,u"A Z"},
    {"fonts/foxit_fixed.cff",FPDF_FONT_TYPE1,false,u"A Z"},
    {"fonts/foxit_fixed.cff",FPDF_FONT_TYPE1,true,u"A Z"}}};
  for(const auto& input:inputs) {
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      const auto& api=library->api();const auto label=library->name()+"-"+std::to_string(&input-inputs.data());SCOPED_TRACE(label);
      Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
      Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
      auto data=Resource(input.path);ASSERT_FALSE(data.empty());
      Handle font(api.FPDFText_LoadFont(document.get(),data.data(),static_cast<uint32_t>(data.size()),input.type,input.cid),api.FPDFFont_Close);
      ASSERT_NE(font.get(),nullptr);EXPECT_EQ(api.FPDFFont_GetIsEmbedded(font.get()),1);
      data.clear();data.shrink_to_fit();
      EXPECT_FALSE(Name(api,font.get(),false).empty());EXPECT_FALSE(FontData(api,font.get()).empty());BasicMetrics(api,font.get());
      const auto outline=Outline(api,font.get(),'A',12);ASSERT_FALSE(outline.empty());
      Handle object(api.FPDFPageObj_CreateTextObj(document.get(),font.get(),30),api.FPDFPageObj_Destroy);
      ASSERT_NE(object.get(),nullptr);ASSERT_TRUE(api.FPDFText_SetText(object.get(),Wide(input.text).data()));
      FS_MATRIX matrix{1,0,0,1,30,130};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object.get(),&matrix));
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),object.release()));font.reset();
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));images[backend++]=Render(api,page.get(),400,240);ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
      CrossReload(*library,document.get(),artifacts,label,[&](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE text) {
        const auto object=read.FPDFPage_GetObject(p,0);ASSERT_NE(object,nullptr);
        EXPECT_EQ(read.FPDFFont_GetIsEmbedded(read.FPDFTextObj_GetFont(object)),1);
        SameOutline(outline,Outline(read,read.FPDFTextObj_GetFont(object),'A',12));
        const auto borrowed=read.FPDFTextObj_GetFont(object);const auto base=Name(read,borrowed,false);
        int flags=-1;const auto required=read.FPDFText_GetFontInfo(text,0,nullptr,0,&flags);
        EXPECT_EQ(required,base.size()+1);EXPECT_EQ(flags&0x7ffff,read.FPDFFont_GetFlags(borrowed));
        for(size_t capacity:{required-1,required,required+1}) {
          std::vector<char> buffer(capacity+2,'~');
          EXPECT_EQ(read.FPDFText_GetFontInfo(text,0,buffer.data(),capacity,nullptr),required);
          if(capacity>=required) { EXPECT_EQ(std::string(buffer.data()),base); }
          else { EXPECT_TRUE(std::all_of(buffer.begin(),buffer.end(),[](char value){return value=='~';})); }
          EXPECT_EQ(buffer[capacity],'~');EXPECT_EQ(buffer[capacity+1],'~');
        }
        EXPECT_NE(Extract(read,text).find(u'A'),std::u16string::npos);
        EXPECT_NE(Extract(read,text).find(u'Z'),std::u16string::npos);
      },FPDF_SUBSET_NEW_FONTS);
    }
    CompareTextRaster(images[0],images[1],artifacts,"loadedfont-"+std::to_string(&input-inputs.data()));
  }
}

uint16_t U16(const std::vector<uint8_t>& data,size_t at) {
  EXPECT_LT(at+1,data.size());if(at+1>=data.size())return 0;
  return static_cast<uint16_t>((data[at]<<8)|data[at+1]);
}
uint32_t U32(const std::vector<uint8_t>& data,size_t at) {
  return (uint32_t(U16(data,at))<<16)|U16(data,at+2);
}
uint16_t GlyphFor(const std::vector<uint8_t>& data,uint16_t codepoint) {
  // Inspect the fixture's public SFNT cmap bytes only; this is not an engine
  // dependency or a shaping implementation. Format4 maps ASCII in Ahem.
  uint32_t cmap=0;
  for(uint16_t i=0;i<U16(data,4);++i) {
    const size_t record=12+i*16;
    if(U32(data,record)==0x636d6170u)cmap=U32(data,record+8);
  }
  EXPECT_NE(cmap,0u);if(!cmap)return 0;
  for(uint16_t i=0;i<U16(data,cmap+2);++i) {
    const size_t sub=cmap+U32(data,cmap+4+i*8+4);
    if(U16(data,sub)!=4)continue;
    const uint16_t count=U16(data,sub+6)/2;
    const size_t ends=sub+14,starts=ends+2*count+2,deltas=starts+2*count,ranges=deltas+2*count;
    for(uint16_t segment=0;segment<count;++segment) if(codepoint>=U16(data,starts+2*segment)&&codepoint<=U16(data,ends+2*segment)) {
      const uint16_t delta=U16(data,deltas+2*segment),range=U16(data,ranges+2*segment);
      if(!range)return static_cast<uint16_t>(codepoint+delta);
      const size_t address=ranges+2*segment+range+2*(codepoint-U16(data,starts+2*segment));
      const uint16_t glyph=U16(data,address);return glyph?static_cast<uint16_t>(glyph+delta):0;
    }
  }
  ADD_FAILURE()<<"No format4 mapping in fixture";return 0;
}

TEST(Fonts, CustomCidMappingsAreCopiedNonIdentityAndPreservedBySubsetting) {
  Artifacts artifacts;
  const char* cmap=R"(/CIDInit /ProcSet findresource begin 12 dict begin begincmap
/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> def
/CMapName /Test def /CMapType 2 def
1 begincodespacerange <0000> <FFFF> endcodespacerange
4 beginbfchar <0000> <00AD> <0001> <0041> <0007> <005A> <0009> <4E2D> endbfchar
endcmap CMapName currentdict /CMap defineresource pop end end)";
  std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
    auto data=Resource("fonts/ahem/Ahem.ttf");ASSERT_FALSE(data.empty());
    const auto a=GlyphFor(data,'A'),z=GlyphFor(data,'Z');ASSERT_NE(a,0);ASSERT_NE(z,0);
    std::vector<uint8_t> map(20,0);map[2]=static_cast<uint8_t>(a>>8);map[3]=static_cast<uint8_t>(a);
    map[14]=static_cast<uint8_t>(z>>8);map[15]=static_cast<uint8_t>(z);map[18]=static_cast<uint8_t>(a>>8);map[19]=static_cast<uint8_t>(a);
    Handle font(api.FPDFText_LoadCidType2Font(document.get(),data.data(),static_cast<uint32_t>(data.size()),cmap,map.data(),static_cast<uint32_t>(map.size())),api.FPDFFont_Close);
    ASSERT_NE(font.get(),nullptr);data.clear();data.shrink_to_fit();map.clear();map.shrink_to_fit();
    Handle object(api.FPDFPageObj_CreateTextObj(document.get(),font.get(),24),api.FPDFPageObj_Destroy);ASSERT_NE(object.get(),nullptr);
    const std::array<uint32_t,4> codes={0,1,7,9};ASSERT_TRUE(api.FPDFText_SetCharcodes(object.get(),codes.data(),codes.size()));
    const FS_MATRIX matrix{1,0,0,1,30,130};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object.get(),&matrix));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),object.release()));font.reset();
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    images[backend++]=Render(api,page.get(),400,240);ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
    CrossReload(*library,document.get(),artifacts,"cidmapping",[](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE text) {
      const auto value=Extract(read,text);EXPECT_NE(value.find(u"AZ"),std::u16string::npos);
      EXPECT_EQ(read.FPDFText_GetUnicode(text,0),0xadu);
      EXPECT_EQ(read.FPDFText_IsHyphen(text,0),0);EXPECT_EQ(read.FPDFText_IsGenerated(text,0),0);
      EXPECT_EQ(read.FPDFText_HasUnicodeMapError(text,0),0);
      EXPECT_NE(value.find(u"\u4e2d"),std::u16string::npos);
      const auto object=read.FPDFPage_GetObject(p,0);ASSERT_NE(object,nullptr);ObjectTextBuffer(read,object,text,u"\u00adAZ\u4e2d");
      for(float size:{0.f,1.f,12.f,24.f})for(uint32_t unicode:{uint32_t('A'),uint32_t('Z'),0x4e2du}) {
        float width=-1;ASSERT_TRUE(read.FPDFFont_GetGlyphWidth(read.FPDFTextObj_GetFont(object),unicode,size,&width));
        EXPECT_NEAR(width,size,.001); // Every mapped Ahem glyph advances one em.
      }
    },FPDF_SUBSET_NEW_FONTS);
  }
  CompareTextRaster(images[0],images[1],artifacts,"customcid");
}

TEST(Fonts, SupplementaryAndLigatureUnicodeSequencesSurviveRustSubsetSave) {
  Artifacts artifacts;
  const auto data=Resource("fonts/ahem/Ahem.ttf");ASSERT_FALSE(data.empty());
  const auto gid=GlyphFor(data,'A');ASSERT_NE(gid,0);
  const char* cmap=R"(/CIDInit /ProcSet findresource begin 12 dict begin begincmap
/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> def
/CMapName /Sequences def /CMapType 2 def
1 begincodespacerange <0000> <FFFF> endcodespacerange
4 beginbfchar <0001> <00E9> <0002> <4E2D> <0003> <D83DDE00> <0004> <006600660069> endbfchar
endcmap CMapName currentdict /CMap defineresource pop end end)";
  const std::u16string expected=u"\u00e9\u4e2d\U0001f600ffi";
  const auto check_sequences=[&](const Api& read,FPDF_TEXTPAGE text) {
    EXPECT_EQ(Extract(read,text),expected);
    ASSERT_EQ(read.FPDFText_CountChars(text),static_cast<int>(expected.size()));
    for(size_t index=0;index<expected.size();++index) {
      EXPECT_EQ(read.FPDFText_GetUnicode(text,index),static_cast<unsigned int>(expected[index]));
      EXPECT_EQ(read.FPDFText_GetCharIndexFromTextIndex(text,index),static_cast<int>(index));
      EXPECT_EQ(read.FPDFText_GetTextIndexFromCharIndex(text,index),static_cast<int>(index));
    }
    for(const auto& item:{std::pair<std::u16string,int>{u"\u00e9\u4e2d",0},{u"ffi",4}}) {
      const auto pattern=Wide(item.first);
      Handle search(read.FPDFText_FindStart(text,pattern.data(),FPDF_MATCHCASE,0),read.FPDFText_FindClose);
      ASSERT_NE(search.get(),nullptr);ASSERT_TRUE(read.FPDFText_FindNext(search.get()));
      EXPECT_EQ(read.FPDFText_GetSchResultIndex(search.get()),item.second);
      EXPECT_EQ(read.FPDFText_GetSchCount(search.get()),static_cast<int>(item.first.size()));
      EXPECT_FALSE(read.FPDFText_FindNext(search.get()));
    }
    for(bool backward:{false,true}) {
      const auto pattern=Wide(u"\U0001f600");
      Handle search(read.FPDFText_FindStart(text,pattern.data(),FPDF_MATCHCASE,backward?-1:0),read.FPDFText_FindClose);
      ASSERT_NE(search.get(),nullptr);
      const bool found=backward?read.FPDFText_FindPrev(search.get()):read.FPDFText_FindNext(search.get());
      // Unix native decodes the query into wchar32 but leaves ToUnicode
      // destinations as surrogate units. Windows native retains wchar16.
      if(&read==&Implementations()[0]->api() && sizeof(wchar_t)>2) { EXPECT_FALSE(found); }
      else {
        ASSERT_TRUE(found);EXPECT_EQ(read.FPDFText_GetSchResultIndex(search.get()),2);
        EXPECT_EQ(read.FPDFText_GetSchCount(search.get()),2);
        EXPECT_FALSE(backward?read.FPDFText_FindPrev(search.get()):read.FPDFText_FindNext(search.get()));
      }
    }
    EXPECT_EQ(read.FPDFText_GetBoundedText(text,0,240,400,0,nullptr,0),7);
    // Capacity three cuts immediately after the high surrogate; this API
    // reports a prefix in UTF-16 units and does not repair the split pair.
    std::array<unsigned short,5> short_buffer;short_buffer.fill(0xaaaa);
    EXPECT_EQ(read.FPDFText_GetBoundedText(text,0,240,400,0,short_buffer.data(),3),3);
    EXPECT_EQ(short_buffer[0],0xe9);EXPECT_EQ(short_buffer[1],0x4e2d);EXPECT_EQ(short_buffer[2],0xd83d);
    EXPECT_EQ(short_buffer[3],0xaaaa);EXPECT_EQ(short_buffer[4],0xaaaa);
  };
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api();SCOPED_TRACE(writer->name());
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
    std::vector<uint8_t> map(10,0);
    for(int code=1;code<=4;++code) { map[2*code]=static_cast<uint8_t>(gid>>8);map[2*code+1]=static_cast<uint8_t>(gid); }
    Handle font(api.FPDFText_LoadCidType2Font(document.get(),data.data(),static_cast<uint32_t>(data.size()),cmap,map.data(),map.size()),api.FPDFFont_Close);
    ASSERT_NE(font.get(),nullptr);
    Handle object(api.FPDFPageObj_CreateTextObj(document.get(),font.get(),24),api.FPDFPageObj_Destroy);
    ASSERT_NE(object.get(),nullptr);const std::array<uint32_t,4> codes={1,2,3,4};
    ASSERT_TRUE(api.FPDFText_SetCharcodes(object.get(),codes.data(),codes.size()));
    const FS_MATRIX matrix{1,0,0,1,20,120};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object.get(),&matrix));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),object.release()));
    {
      const auto inserted=api.FPDFPage_GetObject(page.get(),0);ASSERT_NE(inserted,nullptr);
      ASSERT_TRUE(api.FPDFText_SetText(inserted,Wide(u"\u4e2d\U0001f600\u00e9").data()));
      Handle replacement(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
      const auto replaced=Extract(api,replacement.get());
      if(writer->name()=="rust") { EXPECT_EQ(replaced,u"\u4e2d\U0001f600\u00e9"); }
      else {
        // Native ReverseLookup does not decode a ToUnicode surrogate pair.
        EXPECT_EQ(replaced.find(u"\U0001f600"),std::u16string::npos);
        EXPECT_NE(replaced.find(u'\u4e2d'),std::u16string::npos);
        EXPECT_NE(replaced.find(u'\u00e9'),std::u16string::npos);
      }
      replacement.reset();ASSERT_TRUE(api.FPDFText_SetCharcodes(inserted,codes.data(),codes.size()));
    }
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    {
      Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
      check_sequences(api,text.get());
      EXPECT_EQ(api.FPDFText_GetUnicode(text.get(),0),0xe9u);
      EXPECT_EQ(api.FPDFText_GetUnicode(text.get(),1),0x4e2du);
      ObjectTextBuffer(api,api.FPDFPage_GetObject(page.get(),0),text.get(),expected);
    }
    const auto before=Render(api,page.get(),400,240);
    const auto bytes=SaveDocument(api,document.get(),artifacts,writer->name()+"-unicode-sequences",FPDF_SUBSET_NEW_FONTS);
    ASSERT_FALSE(bytes.empty());
    for(const auto* reader:Implementations()) {
      const auto& read=reader->api();SCOPED_TRACE(writer->name()+"-to-"+reader->name());
      Handle reloaded(read.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),read.FPDF_CloseDocument);
      ASSERT_NE(reloaded.get(),nullptr);Handle reloaded_page(read.FPDF_LoadPage(reloaded.get(),0),read.FPDF_ClosePage);
      ASSERT_NE(reloaded_page.get(),nullptr);Handle text(read.FPDFText_LoadPage(reloaded_page.get()),read.FPDFText_ClosePage);
      const auto value=Extract(read,text.get());
      if(writer->name()=="rust") {
        check_sequences(read,text.get());
        ObjectTextBuffer(read,read.FPDFPage_GetObject(reloaded_page.get(),0),text.get(),expected);
      } else {
        // Native CPDF_FontSubsetter::AddUsedText keeps only Unicode.Front().
        // It replaces a leading surrogate by U+0000 and truncates a ligature.
        EXPECT_EQ(value.substr(0,2),u"\u00e9\u4e2d");
        EXPECT_EQ(value.find(u"\U0001f600"),std::u16string::npos);
        EXPECT_EQ(value.find(u"ffi"),std::u16string::npos);
      }
      CompareTextRaster(before,Render(read,reloaded_page.get(),400,240),artifacts,writer->name()+"-to-"+reader->name()+"-sequences");
    }
  }
}

std::vector<uint8_t> DescriptorFixture(int flags,int weight,int angle,const std::string& base="Helvetica",bool descriptor=true) {
  std::string font="<< /Type /Font /Subtype /Type1 /BaseFont /"+base+" /Encoding /WinAnsiEncoding";
  if(descriptor) { font+=" /FontDescriptor 6 0 R"; }
  font+=" >>";
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 240] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",font,
    PdfStream("","BT /F1 20 Tf 20 120 Td (Metrics) Tj ET"),
    "<< /Type /FontDescriptor /FontName /"+base+" /Flags "+std::to_string(flags)+" /FontBBox [0 -200 1000 800] /ItalicAngle "+std::to_string(angle)+" /Ascent 800 /Descent -200 /StemV 80 /FontWeight "+std::to_string(weight)+" >>"});
}

TEST(Fonts, ExplicitDescriptorFlagsWeightsAnglesAndUnicodeNamesAreObservable) {
  Artifacts artifacts;
  // A synthetic non-ASCII font name has platform-dependent fallback outlines;
  // its UTF-8 metadata contract is independent of those outlines. Rendering
  // comparisons below use the deterministic standard Helvetica face.
  for(bool descriptor:{false,true})for(const auto* library:Implementations()) {
    const auto bytes=DescriptorFixture(32,400,0,"#C3#89cole",descriptor);
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
    ASSERT_NE(page.get(),nullptr);const auto object=api.FPDFPage_GetObject(page.get(),0);ASSERT_NE(object,nullptr);
    const auto font=api.FPDFTextObj_GetFont(object);ASSERT_NE(font,nullptr);
    EXPECT_EQ(Name(api,font,false),"\xC3\x89" "cole");EXPECT_FALSE(Name(api,font,true).empty());
    Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
    int flags=-1;const auto size=api.FPDFText_GetFontInfo(text.get(),0,nullptr,0,&flags);ASSERT_EQ(size,7u);
    std::vector<char> name(size+1,'~');EXPECT_EQ(api.FPDFText_GetFontInfo(text.get(),0,name.data(),size,nullptr),size);
    EXPECT_EQ(std::string(name.data()),"\xC3\x89" "cole");EXPECT_EQ(name[size],'~');
  }
  for(int flags:{1,2,4,8,32,64,65536,131072,262144,1|2|32|64}) {
    const auto bytes=DescriptorFixture(flags,400,0);artifacts.Pdf("flags-"+std::to_string(flags),bytes);
    for(const auto* library:Implementations()) {
      const auto& api=library->api();SCOPED_TRACE(library->name()+" flags="+std::to_string(flags));
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      const auto object=api.FPDFPage_GetObject(page.get(),0);ASSERT_NE(object,nullptr);
      const auto font=api.FPDFTextObj_GetFont(object);ASSERT_NE(font,nullptr);
      EXPECT_EQ(api.FPDFFont_GetFlags(font),flags);EXPECT_EQ(api.FPDFFont_GetWeight(font),400);
      float ascent,descent;ASSERT_TRUE(api.FPDFFont_GetAscent(font,20,&ascent));ASSERT_TRUE(api.FPDFFont_GetDescent(font,20,&descent));
      EXPECT_NEAR(ascent,16,.001);EXPECT_NEAR(descent,-4,.001);
    }
  }
  for(int weight:{100,400,700,900})for(int angle:{-12,0,15}) {
    const auto bytes=DescriptorFixture(32,weight,angle);
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      const auto& api=library->api();SCOPED_TRACE(library->name());
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      const auto font=api.FPDFTextObj_GetFont(api.FPDFPage_GetObject(page.get(),0));ASSERT_NE(font,nullptr);
      EXPECT_EQ(Name(api,font,false),"Helvetica");EXPECT_FALSE(Name(api,font,true).empty());
      EXPECT_EQ(api.FPDFFont_GetWeight(font),weight);int value=123;EXPECT_TRUE(api.FPDFFont_GetItalicAngle(font,&value));
      // Pinned native LoadFontDescriptor retains only negative angles.
      EXPECT_EQ(value,library->name()=="native"?std::min(angle,0):angle);
      {
        Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
        ASSERT_NE(text.get(),nullptr);
        for(int index=0;index<api.FPDFText_CountChars(text.get());++index)
          EXPECT_EQ(api.FPDFText_GetFontWeight(text.get(),index),weight);
      }
      images[backend++]=Render(api,page.get(),400,240);ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
      CrossReload(*library,document.get(),artifacts,"descriptors-"+std::to_string(weight)+"-"+std::to_string(angle),
        [weight,angle](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE) {
          auto font=read.FPDFTextObj_GetFont(read.FPDFPage_GetObject(p,0));ASSERT_NE(font,nullptr);
          EXPECT_EQ(read.FPDFFont_GetWeight(font),weight);int value;EXPECT_TRUE(read.FPDFFont_GetItalicAngle(font,&value));
          EXPECT_EQ(value,&read==&Implementations()[0]->api()?std::min(angle,0):angle);
        });
    }
    CompareTextRaster(images[0],images[1],artifacts,"descriptor-backends-"+std::to_string(weight)+"-"+std::to_string(angle));
  }
}

TEST(Fonts, BorrowedGlyphPathsExposeContoursAndSafeIndexBoundaries) {
  Artifacts artifacts;
  for(const std::string resource:{"fonts/ahem/Ahem.ttf","fonts/foxit_fixed.cff"}) {
    const auto data=Resource(resource);ASSERT_FALSE(data.empty());
    std::array<std::vector<std::tuple<int,float,float,bool>>,2> observations;size_t backend=0;
    for(const auto* library:Implementations()) {
      const auto& api=library->api();SCOPED_TRACE(library->name()+"/"+resource);
      Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
      Handle font(api.FPDFText_LoadFont(document.get(),data.data(),static_cast<uint32_t>(data.size()),resource.find(".ttf")!=std::string::npos?FPDF_FONT_TRUETYPE:FPDF_FONT_TYPE1,0),api.FPDFFont_Close);
      ASSERT_NE(font.get(),nullptr);
      for(float size:{1.f,12.f,24.f})for(uint32_t code:{0u,32u,static_cast<uint32_t>('i'),static_cast<uint32_t>('O'),static_cast<uint32_t>('S')}) {
        const auto glyph=api.FPDFFont_GetGlyphPath(font.get(),code,size);
        if(!glyph) { EXPECT_TRUE(code==0 || code==32);continue; }
        const int count=api.FPDFGlyphPath_CountGlyphSegments(glyph);EXPECT_GE(count,0);
        if(code!=' ' && code!=0) { EXPECT_GT(count,0); }
        EXPECT_EQ(api.FPDFGlyphPath_GetGlyphPathSegment(glyph,-1),nullptr);
        EXPECT_EQ(api.FPDFGlyphPath_GetGlyphPathSegment(glyph,count),nullptr);
        EXPECT_EQ(api.FPDFGlyphPath_GetGlyphPathSegment(glyph,count+10),nullptr);
        bool move=false,close=false;float contour_x=0,contour_y=0;
        for(int index=0;index<count;++index) {
          const auto segment=api.FPDFGlyphPath_GetGlyphPathSegment(glyph,index);ASSERT_NE(segment,nullptr);
          float x,y;ASSERT_TRUE(api.FPDFPathSegment_GetPoint(segment,&x,&y));EXPECT_TRUE(std::isfinite(x));EXPECT_TRUE(std::isfinite(y));
          const int type=api.FPDFPathSegment_GetType(segment);EXPECT_TRUE(type==FPDF_SEGMENT_MOVETO||type==FPDF_SEGMENT_LINETO||type==FPDF_SEGMENT_BEZIERTO);
          move|=type==FPDF_SEGMENT_MOVETO;close|=!!api.FPDFPathSegment_GetClose(segment);
          const bool closes=!!api.FPDFPathSegment_GetClose(segment);
          if(type==FPDF_SEGMENT_MOVETO) { contour_x=x;contour_y=y; }
          // FreeType emits an explicit closing line; the Rust outline pen
          // closes the preceding segment implicitly. Canonicalize this
          // equivalent contour representation before comparing geometry.
          const bool implicit=closes && (std::abs(x-contour_x)>.00001f || std::abs(y-contour_y)>.00001f);
          observations[backend].emplace_back(type,x,y,closes&&!implicit);
          if(implicit)observations[backend].emplace_back(FPDF_SEGMENT_LINETO,contour_x,contour_y,true);
        }
        if(count) { EXPECT_TRUE(move);EXPECT_TRUE(close); }
        if(resource.find("foxit_fixed")!=std::string::npos && (code=='O' || code=='S')) {
          bool curved=false;int contours=0;
          for(int index=0;index<count;++index) {
            const auto segment=api.FPDFGlyphPath_GetGlyphPathSegment(glyph,index);
            curved|=api.FPDFPathSegment_GetType(segment)==FPDF_SEGMENT_BEZIERTO;
            contours+=api.FPDFPathSegment_GetType(segment)==FPDF_SEGMENT_MOVETO;
          }
          EXPECT_TRUE(curved);if(code=='O') { EXPECT_GE(contours,2); }
        }
      }
      EXPECT_EQ(api.FPDFFont_GetGlyphPath(font.get(),0xffffffffu,12),nullptr);
      ++backend;
    }
    ASSERT_EQ(observations[0].size(),observations[1].size());
    for(size_t index=0;index<observations[0].size();++index) {
      EXPECT_EQ(std::get<0>(observations[0][index]),std::get<0>(observations[1][index]));
      EXPECT_NEAR(std::get<1>(observations[0][index]),std::get<1>(observations[1][index]),.01);
      EXPECT_NEAR(std::get<2>(observations[0][index]),std::get<2>(observations[1][index]),.01);
      EXPECT_EQ(std::get<3>(observations[0][index]),std::get<3>(observations[1][index]));
    }
  }
}

TEST(Fonts, InvalidTextFontAndMetricArgumentsFailWithoutMutatingOutputs) {
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle font(api.FPDFText_LoadStandardFont(document.get(),"Helvetica"),api.FPDFFont_Close);
    Handle path(api.FPDFPageObj_CreateNewRect(1,2,3,4),api.FPDFPageObj_Destroy);
    ASSERT_NE(font.get(),nullptr);ASSERT_NE(path.get(),nullptr);
    const std::array<uint8_t,8> random={1,2,3,4,5,6,7,8};const std::array<uint8_t,2> map={0,1};
    for(int type:{FPDF_FONT_TYPE1,FPDF_FONT_TRUETYPE,-1,99}) {
      EXPECT_EQ(api.FPDFText_LoadFont(document.get(),random.data(),random.size(),type,0),nullptr);
      EXPECT_EQ(api.FPDFText_LoadFont(document.get(),random.data(),0,type,0),nullptr);
    }
    EXPECT_EQ(api.FPDFText_LoadFont(nullptr,random.data(),random.size(),FPDF_FONT_TRUETYPE,0),nullptr);
    EXPECT_EQ(api.FPDFText_LoadFont(document.get(),nullptr,random.size(),FPDF_FONT_TRUETYPE,0),nullptr);
    EXPECT_EQ(api.FPDFText_LoadCidType2Font(document.get(),random.data(),random.size(),"",map.data(),map.size()),nullptr);
    EXPECT_EQ(api.FPDFText_LoadCidType2Font(document.get(),random.data(),random.size(),"bad cmap",map.data(),map.size()),nullptr);
    EXPECT_EQ(api.FPDFText_LoadCidType2Font(document.get(),random.data(),0,"bad cmap",map.data(),map.size()),nullptr);
    EXPECT_EQ(api.FPDFText_LoadCidType2Font(document.get(),random.data(),random.size(),nullptr,map.data(),map.size()),nullptr);
    EXPECT_EQ(api.FPDFText_LoadCidType2Font(document.get(),random.data(),random.size(),"bad cmap",nullptr,0),nullptr);
    const auto valid=Resource("fonts/ahem/Ahem.ttf");ASSERT_FALSE(valid.empty());
    const char* cmap="1 beginbfchar <0001> <0041> endbfchar";
    EXPECT_EQ(api.FPDFText_LoadCidType2Font(nullptr,valid.data(),valid.size(),cmap,map.data(),map.size()),nullptr);
    EXPECT_EQ(api.FPDFText_LoadCidType2Font(document.get(),nullptr,valid.size(),cmap,map.data(),map.size()),nullptr);
    EXPECT_EQ(api.FPDFText_LoadCidType2Font(document.get(),valid.data(),0,cmap,map.data(),map.size()),nullptr);
    EXPECT_EQ(api.FPDFText_LoadCidType2Font(document.get(),valid.data(),valid.size(),nullptr,map.data(),map.size()),nullptr);
    EXPECT_EQ(api.FPDFText_LoadCidType2Font(document.get(),valid.data(),valid.size(),"",map.data(),map.size()),nullptr);
    EXPECT_EQ(api.FPDFText_LoadCidType2Font(document.get(),valid.data(),valid.size(),cmap,nullptr,map.size()),nullptr);
    EXPECT_EQ(api.FPDFText_LoadCidType2Font(document.get(),valid.data(),valid.size(),cmap,map.data(),0),nullptr);
    if(library->name()=="rust") {
      // Native LoadCustomCompositeFont takes .first<2>() on every map entry
      // and traps on the final odd byte. Rust rejects this malformed map.
      EXPECT_EQ(api.FPDFText_LoadCidType2Font(document.get(),valid.data(),valid.size(),cmap,map.data(),1),nullptr);
    }
    {
      const uint16_t gid=GlyphFor(valid,'A');ASSERT_NE(gid,0);
      const std::array<uint8_t,4> mapping={0,0,static_cast<uint8_t>(gid>>8),static_cast<uint8_t>(gid)};
      // Nonempty malformed CMap syntax is retained by both factories. It
      // supplies no Unicode mapping; glyph selection still uses CIDToGID.
      Handle malformed(api.FPDFText_LoadCidType2Font(document.get(),valid.data(),valid.size(),"not a CMap",mapping.data(),mapping.size()),api.FPDFFont_Close);
      ASSERT_NE(malformed.get(),nullptr);EXPECT_FALSE(FontData(api,malformed.get()).empty());
      Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
      Handle object(api.FPDFPageObj_CreateTextObj(document.get(),malformed.get(),24),api.FPDFPageObj_Destroy);
      ASSERT_NE(object.get(),nullptr);const std::array<uint32_t,2> codes={1,0};
      ASSERT_TRUE(api.FPDFText_SetCharcodes(object.get(),codes.data(),codes.size()));
      const FS_MATRIX transform{1,0,0,1,20,120};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object.get(),&transform));
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),object.release()));
      Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
      ASSERT_EQ(api.FPDFText_CountChars(text.get()),2);
      EXPECT_EQ(api.FPDFText_GetUnicode(text.get(),0),1u);EXPECT_EQ(api.FPDFText_HasUnicodeMapError(text.get(),0),1);
      EXPECT_EQ(api.FPDFText_GetUnicode(text.get(),1),0u);EXPECT_EQ(api.FPDFText_IsGenerated(text.get(),1),0);
      EXPECT_EQ(api.FPDFText_GetTextIndexFromCharIndex(text.get(),1),-1);
      EXPECT_EQ(api.FPDFText_GetCharIndexFromTextIndex(text.get(),0),0);
    }
    for(uint32_t truncated:{1u,4u,12u})
      EXPECT_EQ(api.FPDFText_LoadFont(document.get(),valid.data(),truncated,FPDF_FONT_TRUETYPE,0),nullptr);
    for(int invalid:{-1,0,99})
      EXPECT_EQ(api.FPDFText_LoadFont(document.get(),valid.data(),valid.size(),invalid,0),nullptr);
    {
      // This factory deliberately does not check the parsed face against the
      // caller's PDF font_type (see the upstream format-validation TODO).
      Handle mismatch(api.FPDFText_LoadFont(document.get(),valid.data(),valid.size(),FPDF_FONT_TYPE1,0),api.FPDFFont_Close);
      ASSERT_NE(mismatch.get(),nullptr);EXPECT_EQ(api.FPDFFont_GetIsEmbedded(mismatch.get()),1);
      EXPECT_FALSE(FontData(api,mismatch.get()).empty());
    }
    for(auto object:{FPDF_PAGEOBJECT{},path.get()}) {
      float size=91;EXPECT_FALSE(api.FPDFTextObj_GetFontSize(object,&size));EXPECT_EQ(size,91);
      EXPECT_FALSE(api.FPDFTextObj_SetFontSize(object,10));EXPECT_EQ(api.FPDFTextObj_GetFont(object),nullptr);
      EXPECT_FALSE(api.FPDFText_SetText(object,Wide(u"A").data()));
      const uint32_t code='A';EXPECT_FALSE(api.FPDFText_SetCharcodes(object,&code,1));
      const float position=1;EXPECT_FALSE(api.FPDFText_SetPositions(object,&position,1));
      EXPECT_EQ(api.FPDFTextObj_GetTextRenderMode(object),FPDF_TEXTRENDERMODE_UNKNOWN);
      EXPECT_FALSE(api.FPDFTextObj_SetTextRenderMode(object,FPDF_TEXTRENDERMODE_FILL));
      EXPECT_EQ(api.FPDFTextObj_GetText(object,nullptr,nullptr,0),0u);
      EXPECT_EQ(api.FPDFTextObj_GetRenderedBitmap(document.get(),nullptr,object,1),nullptr);
    }
    EXPECT_FALSE(api.FPDFTextObj_GetFontSize(nullptr,nullptr));
    EXPECT_EQ(api.FPDFFont_GetBaseFontName(nullptr,nullptr,0),0u);EXPECT_EQ(api.FPDFFont_GetFamilyName(nullptr,nullptr,0),0u);
    EXPECT_EQ(api.FPDFFont_GetIsEmbedded(nullptr),-1);EXPECT_EQ(api.FPDFFont_GetFlags(nullptr),-1);EXPECT_EQ(api.FPDFFont_GetWeight(nullptr),-1);
    int angle=91;EXPECT_FALSE(api.FPDFFont_GetItalicAngle(nullptr,&angle));EXPECT_EQ(angle,91);
    EXPECT_FALSE(api.FPDFFont_GetItalicAngle(font.get(),nullptr));
    for(auto query:{api.FPDFFont_GetAscent,api.FPDFFont_GetDescent}) {
      float value=91;EXPECT_FALSE(query(nullptr,12,&value));EXPECT_FLOAT_EQ(value,91);EXPECT_FALSE(query(font.get(),12,nullptr));
    }
    float width=91;EXPECT_FALSE(api.FPDFFont_GetGlyphWidth(nullptr,'A',12,&width));EXPECT_FLOAT_EQ(width,91);
    EXPECT_FALSE(api.FPDFFont_GetGlyphWidth(font.get(),'A',12,nullptr));
    size_t size=91;EXPECT_FALSE(api.FPDFFont_GetFontData(nullptr,nullptr,0,&size));EXPECT_EQ(size,91u);
    EXPECT_FALSE(api.FPDFFont_GetFontData(font.get(),nullptr,0,nullptr));
    EXPECT_EQ(api.FPDFFont_GetGlyphPath(nullptr,'A',12),nullptr);
    EXPECT_EQ(api.FPDFGlyphPath_CountGlyphSegments(nullptr),-1);EXPECT_EQ(api.FPDFGlyphPath_GetGlyphPathSegment(nullptr,0),nullptr);
  }
}

size_t FontTable(const std::vector<uint8_t>& data,uint32_t tag) {
  if(data.size()<12)return 0;
  for(uint16_t index=0;index<U16(data,4);++index) {
    const size_t record=12+static_cast<size_t>(index)*16;
    if(record+16>data.size())return 0;
    if(U32(data,record)==tag) {
      const size_t offset=U32(data,record+8),length=U32(data,record+12);
      EXPECT_LE(offset+length,data.size());return offset+length<=data.size()?offset:0;
    }
  }
  return 0;
}

TEST(Fonts, MissingFamilyMetadataKeepsAnEmptyNameAndUsableEmbeddedOutlines) {
  auto bytes=Resource("fonts/ahem/Ahem.ttf");ASSERT_FALSE(bytes.empty());
  const auto names=FontTable(bytes,0x6e616d65);ASSERT_NE(names,0u);
  const auto records=U16(bytes,names+2);int removed=0;
  for(uint16_t index=0;index<records;++index) {
    const auto field=names+6+static_cast<size_t>(index)*12+6;const auto id=U16(bytes,field);
    if(id==1 || id==16 || id==21) { bytes[field]=0x7f;bytes[field+1]=0xff;++removed; }
  }
  const auto checksum=[&](size_t offset,size_t length) {
    uint32_t sum=0;
    for(size_t index=0;index<length;index+=4) {
      uint32_t word=0;
      for(size_t byte=0;byte<4;++byte)word=(word<<8)|(index+byte<length?bytes[offset+index+byte]:0);
      sum+=word;
    }
    return sum;
  };
  const auto put32=[&](size_t offset,uint32_t value) {
    for(int byte=0;byte<4;++byte)bytes[offset+byte]=static_cast<uint8_t>(value>>(24-byte*8));
  };
  for(uint16_t index=0;index<U16(bytes,4);++index) {
    const size_t record=12+static_cast<size_t>(index)*16;
    if(U32(bytes,record)==0x6e616d65)put32(record+4,checksum(names,U32(bytes,record+12)));
  }
  const auto head=FontTable(bytes,0x68656164);ASSERT_NE(head,0u);
  put32(head+8,0);put32(head+8,0xb1b0afbau-checksum(0,bytes.size()));
  ASSERT_GT(removed,0);Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
    Handle font(api.FPDFText_LoadFont(document.get(),bytes.data(),bytes.size(),FPDF_FONT_TRUETYPE,0),api.FPDFFont_Close);
    ASSERT_NE(font.get(),nullptr);EXPECT_EQ(Name(api,font.get(),true),"");EXPECT_EQ(Name(api,font.get(),false),"Ahem");
    Handle object(api.FPDFPageObj_CreateTextObj(document.get(),font.get(),24),api.FPDFPageObj_Destroy);
    ASSERT_NE(object.get(),nullptr);ASSERT_TRUE(api.FPDFText_SetText(object.get(),Wide(u"AZ").data()));
    const FS_MATRIX matrix{1,0,0,1,20,120};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object.get(),&matrix));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),object.release()));
    images[backend++]=Render(api,page.get(),400,240);ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));page.reset();
    CrossReload(*library,document.get(),artifacts,"missing-family",[](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE text) {
      EXPECT_EQ(Extract(read,text),u"AZ");
      const auto font=read.FPDFTextObj_GetFont(read.FPDFPage_GetObject(p,0));ASSERT_NE(font,nullptr);
      EXPECT_EQ(Name(read,font,true),"");EXPECT_FALSE(Outline(read,font,'A',12).empty());
    });
  }
  CompareTextRaster(images[0],images[1],artifacts,"missing-family-backends");
}

TEST(Fonts, EmbeddedCff2AndPublicFontFactorySaveWithValidOutlineTables) {
  // Licensed, pinned upstream test font; see fixtures/README.md. No unreleased
  // library is linked: this is an ordinary externally produced CFF2 font.
  const auto font_bytes=Read(TestRoot()/"tests/capi/fixtures/Cantarell-VF-ABC.otf");
  ASSERT_FALSE(font_bytes.empty());ASSERT_NE(FontTable(font_bytes,0x43464632),0u);
  const std::string font_program(font_bytes.begin(),font_bytes.end());
  const auto embedded=PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 240] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Cantarell-Regular /FirstChar 65 /LastChar 67 /Widths [600 600 600] /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R >>",
    PdfStream("","BT /F1 30 Tf 30 130 Td (ABC) Tj ET"),
    "<< /Type /FontDescriptor /FontName /Cantarell-Regular /Flags 32 /FontBBox [0 -250 1000 1000] /ItalicAngle 0 /Ascent 900 /Descent -250 /StemV 80 /FontFile3 7 0 R >>",
    PdfStream("/Subtype /OpenType",font_program)});
  Artifacts artifacts;artifacts.Pdf("embedded-cff2",embedded);
  for(bool factory:{false,true}) {
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      const auto& api=library->api();SCOPED_TRACE(library->name()+" factory="+std::to_string(factory));
      Handle document(factory?api.FPDF_CreateNewDocument():api.FPDF_LoadMemDocument64(embedded.data(),embedded.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);
      Handle page(factory?api.FPDFPage_New(document.get(),0,400,240):api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
      ASSERT_NE(page.get(),nullptr);
      if(factory) {
        Handle font(api.FPDFText_LoadFont(document.get(),font_bytes.data(),font_bytes.size(),FPDF_FONT_TYPE1,0),api.FPDFFont_Close);
        ASSERT_NE(font.get(),nullptr);
        Handle object(api.FPDFPageObj_CreateTextObj(document.get(),font.get(),30),api.FPDFPageObj_Destroy);ASSERT_NE(object.get(),nullptr);
        ASSERT_TRUE(api.FPDFText_SetText(object.get(),Wide(u"ABC").data()));
        const FS_MATRIX matrix{1,0,0,1,30,130};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object.get(),&matrix));
        ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),object.release()));
      }
      const auto object=api.FPDFPage_GetObject(page.get(),0);ASSERT_NE(object,nullptr);
      const auto original=Outline(api,api.FPDFTextObj_GetFont(object),'A',12);ASSERT_FALSE(original.empty());
      const auto inspect=[&](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE text) {
        EXPECT_EQ(Extract(read,text),u"ABC");
        const auto font=read.FPDFTextObj_GetFont(read.FPDFPage_GetObject(p,0));ASSERT_NE(font,nullptr);
        EXPECT_EQ(read.FPDFFont_GetIsEmbedded(font),1);
        const auto data=FontData(read,font);ASSERT_NE(FontTable(data,0x43464632),0u);
        EXPECT_EQ(GlyphFor(data,'A'),GlyphFor(font_bytes,'A'));
        SameOutline(original,Outline(read,font,'A',12));
      };
      {
        Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);inspect(api,page.get(),text.get());
      }
      const auto image=Render(api,page.get(),400,240);size_t ink=0;
      for(size_t pixel=0;pixel+3<image.pixels.size();pixel+=4)
        ink+=image.pixels[pixel]<128 || image.pixels[pixel+1]<128 || image.pixels[pixel+2]<128;
      EXPECT_GT(ink,20u);images[backend++]=image;
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));page.reset();
      CrossReload(*library,document.get(),artifacts,factory?"factory-cff2":"embedded-cff2",inspect,FPDF_SUBSET_NEW_FONTS,RasterTolerance{56,0,4});
    }
    // Default CFF2 outlines agree point-for-point, while FreeType and AGG
    // differ by <=52 at coverage edges (RGB mean3.4 in this tight text ROI).
    // Independently retain the painted bounds and dark ink population.
    const auto ink=[](const Raster& image) {
      std::array<int,5> result={image.width,image.height,-1,-1,0};
      for(int y=0;y<image.height;++y)for(int x=0;x<image.width;++x) {
        const auto p=(static_cast<size_t>(y)*image.width+x)*4;
        if(image.pixels[p]<128 || image.pixels[p+1]<128 || image.pixels[p+2]<128) {
          result[0]=std::min(result[0],x);result[1]=std::min(result[1],y);
          result[2]=std::max(result[2],x);result[3]=std::max(result[3],y);++result[4];
        }
      }
      return result;
    };
    const auto native=ink(images[0]),rust=ink(images[1]);
    for(int coordinate=0;coordinate<4;++coordinate)EXPECT_EQ(native[coordinate],rust[coordinate]);
    EXPECT_LE(std::abs(native[4]-rust[4]),10);EXPECT_GT(native[4],300);EXPECT_GT(rust[4],300);
    CompareTextRaster(images[0],images[1],artifacts,factory?"factory-cff2-backends":"embedded-cff2-backends",RasterTolerance{56,0,4});
  }
}

void VerifyNotdefOutline(const std::vector<uint8_t>& data) {
  const auto head=FontTable(data,0x68656164),loca=FontTable(data,0x6c6f6361),glyf=FontTable(data,0x676c7966);
  ASSERT_NE(head,0u);ASSERT_NE(loca,0u);ASSERT_NE(glyf,0u);
  const int format=U16(data,head+50);
  const auto first=format?U32(data,loca):uint32_t(U16(data,loca))*2;
  const auto second=format?U32(data,loca+4):uint32_t(U16(data,loca+2))*2;
  ASSERT_GT(second,first);ASSERT_LE(glyf+second,data.size());
  EXPECT_GT(U16(data,glyf+first),0u); // Nonempty simple outline, not only a slot.
}

TEST(Fonts, SparseTrueTypeSaveRetainsGlyphIdsAndNotdefOutline) {
  Artifacts artifacts;
  const auto original=Resource("fonts/ahem/Ahem.ttf");ASSERT_FALSE(original.empty());
  const auto expected_a=GlyphFor(original,'A'),expected_z=GlyphFor(original,'Z');
  ASSERT_NE(expected_a,0);ASSERT_NE(expected_z,0);VerifyNotdefOutline(original);
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api();SCOPED_TRACE(writer->name());
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
    Handle font(api.FPDFText_LoadFont(document.get(),original.data(),static_cast<uint32_t>(original.size()),FPDF_FONT_TRUETYPE,0),api.FPDFFont_Close);
    ASSERT_NE(font.get(),nullptr);
    Handle object(api.FPDFPageObj_CreateTextObj(document.get(),font.get(),30),api.FPDFPageObj_Destroy);
    ASSERT_NE(object.get(),nullptr);ASSERT_TRUE(api.FPDFText_SetText(object.get(),Wide(u"AZ").data()));
    FS_MATRIX matrix{1,0,0,1,30,130};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object.get(),&matrix));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(),object.release()));font.reset();
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    const auto before=Render(api,page.get(),400,240);ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
    const auto bytes=SaveDocument(api,document.get(),artifacts,writer->name()+"-sparse-ttf",FPDF_SUBSET_NEW_FONTS);
    ASSERT_FALSE(bytes.empty());
    for(const auto* reader:Implementations()) {
      const auto& read=reader->api();SCOPED_TRACE(reader->name());
      Handle reopened(read.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(),nullptr);Handle reloaded(read.FPDF_LoadPage(reopened.get(),0),read.FPDF_ClosePage);ASSERT_NE(reloaded.get(),nullptr);
      const auto text=read.FPDFPage_GetObject(reloaded.get(),0);ASSERT_NE(text,nullptr);
      const auto borrowed=read.FPDFTextObj_GetFont(text);ASSERT_NE(borrowed,nullptr);
      const auto subset=FontData(read,borrowed);ASSERT_FALSE(subset.empty());
      EXPECT_LT(subset.size(),original.size());
      EXPECT_EQ(GlyphFor(subset,'A'),expected_a);EXPECT_EQ(GlyphFor(subset,'Z'),expected_z);
      const auto maxp=FontTable(subset,0x6d617870);ASSERT_NE(maxp,0u);
      EXPECT_GT(U16(subset,maxp+4),std::max(expected_a,expected_z));
      VerifyNotdefOutline(subset);
      Handle extracted(read.FPDFText_LoadPage(reloaded.get()),read.FPDFText_ClosePage);
      EXPECT_EQ(Extract(read,extracted.get()),u"AZ");
      CompareTextRaster(before,Render(read,reloaded.get(),400,240),artifacts,writer->name()+"-to-"+reader->name()+"-retained-gids");
    }
  }
}

TEST(Fonts, StrokedTextColorEndpointsAndWidthsPersistWithRenderedOutput) {
  struct Expected {std::array<unsigned int,4> rgba;float width;};
  const std::array<std::array<unsigned int,3>,5> colors={{{255,0,0},{0,255,0},{0,0,255},{0,0,0},{255,255,255}}};
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,400,240),api.FPDF_ClosePage);
    std::vector<Expected> expected;
    for(unsigned int alpha:{0u,1u,127u,254u,255u})for(auto color:colors)for(float width:{0.f,.75f,12.f}) {
      const auto index=expected.size();
      const auto object=Add(api,document.get(),page.get(),u"M",20+static_cast<float>(index%10)*36,
                             220-static_cast<float>(index/10)*26,18);
      ASSERT_NE(object,nullptr);
      ASSERT_TRUE(api.FPDFPageObj_SetFillColor(object,color[0],color[1],color[2],alpha));
      ASSERT_TRUE(api.FPDFPageObj_SetStrokeColor(object,color[0],color[1],color[2],alpha));
      ASSERT_TRUE(api.FPDFPageObj_SetStrokeWidth(object,width));
      ASSERT_TRUE(api.FPDFTextObj_SetTextRenderMode(object,FPDF_TEXTRENDERMODE_FILL_STROKE));
      expected.push_back({{color[0],color[1],color[2],alpha},width});
    }
    images[backend++]=Render(api,page.get(),400,240);ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); page.reset();
    CrossReload(*library,document.get(),artifacts,"text-style-endpoints",[&](const Api& read,FPDF_PAGE p,FPDF_TEXTPAGE text){
      ASSERT_EQ(read.FPDFPage_CountObjects(p),static_cast<int>(expected.size()));
      for(size_t index=0;index<expected.size();++index) {
        SCOPED_TRACE(index);const auto object=read.FPDFPage_GetObject(p,static_cast<int>(index));ASSERT_NE(object,nullptr);
        std::array<unsigned int,4> fill{},stroke{};
        ASSERT_TRUE(read.FPDFPageObj_GetFillColor(object,&fill[0],&fill[1],&fill[2],&fill[3]));
        ASSERT_TRUE(read.FPDFPageObj_GetStrokeColor(object,&stroke[0],&stroke[1],&stroke[2],&stroke[3]));
        EXPECT_EQ(fill,expected[index].rgba);EXPECT_EQ(stroke,expected[index].rgba);
        float width=-1;ASSERT_TRUE(read.FPDFPageObj_GetStrokeWidth(object,&width));EXPECT_FLOAT_EQ(width,expected[index].width);
        EXPECT_EQ(read.FPDFTextObj_GetTextRenderMode(object),FPDF_TEXTRENDERMODE_FILL_STROKE);
      }
      EXPECT_GT(read.FPDFText_CountChars(text),0);
    });
  }
  CompareTextRaster(images[0],images[1],artifacts,"text-style-endpoints-backends");
}

}  // namespace
}  // namespace pdfoxium::integration
