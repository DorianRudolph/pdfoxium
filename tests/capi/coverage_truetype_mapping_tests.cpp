#include "text_fixture.h"

#include <map>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
uint16_t Word(const Bytes& bytes, size_t at) {
  EXPECT_LT(at + 1, bytes.size());
  return at + 1 < bytes.size() ? (uint16_t(bytes[at]) << 8) | bytes[at + 1] : 0;
}
uint32_t Long(const Bytes& bytes, size_t at) {
  return (uint32_t(Word(bytes, at)) << 16) | Word(bytes, at + 2);
}
void PutWord(Bytes& bytes, size_t at, uint16_t value) {
  bytes.at(at) = static_cast<uint8_t>(value >> 8);
  bytes.at(at + 1) = static_cast<uint8_t>(value);
}
void PutLong(Bytes& bytes, size_t at, uint32_t value) {
  PutWord(bytes, at, value >> 16); PutWord(bytes, at + 2, value);
}

uint32_t Tag(const char* name) {
  uint32_t tag = 0;
  for (int i = 0; i < 4; ++i) tag = (tag << 8) | static_cast<uint8_t>(name[i]);
  return tag;
}
uint32_t Checksum(const Bytes& bytes) {
  uint32_t sum = 0;
  for (size_t offset = 0; offset < bytes.size(); offset += 4) {
    uint32_t word = 0;
    for (int i = 0; i < 4; ++i)
      word = (word << 8) | (offset + i < bytes.size() ? bytes[offset + i] : 0);
    sum += word;
  }
  return sum;
}
using Tables = std::map<uint32_t, Bytes>;
Tables AhemTables() {
  const auto original = text_fixture::Resource("fonts/ahem/Ahem.ttf");
  Tables tables;
  for (unsigned i = 0; i < Word(original, 4); ++i) {
    const size_t row = 12 + i * 16, offset = Long(original, row + 8), length = Long(original, row + 12);
    if (offset > original.size() || length > original.size() - offset) {
      ADD_FAILURE() << "Pinned Ahem table outside font"; return {};
    }
    tables[Long(original, row)] = Bytes(original.begin() + offset, original.begin() + offset + length);
  }
  return tables;
}
uint16_t AhemGlyph(uint16_t code) {
  const auto tables = AhemTables();
  const auto& cmap = tables.at(Tag("cmap"));
  for (unsigned i = 0; i < Word(cmap, 2); ++i) {
    const size_t sub = Long(cmap, 8 + i * 8);
    if (Word(cmap, sub) != 4) continue;
    const unsigned count = Word(cmap, sub + 6) / 2;
    const size_t ends = sub + 14, starts = ends + count * 2 + 2;
    const size_t deltas = starts + count * 2, ranges = deltas + count * 2;
    for (unsigned j = 0; j < count; ++j) {
      if (code < Word(cmap, starts + j * 2) || code > Word(cmap, ends + j * 2)) continue;
      const auto delta = Word(cmap, deltas + j * 2), range = Word(cmap, ranges + j * 2);
      if (!range) return static_cast<uint16_t>(code + delta);
      const auto glyph = Word(cmap, ranges + j * 2 + range + (code - Word(cmap, starts + j * 2)) * 2);
      return glyph ? static_cast<uint16_t>(glyph + delta) : 0;
    }
  }
  ADD_FAILURE() << "Pinned Ahem lacks requested glyph"; return 0;
}

// Test-only SFNT assembly keeps every pinned table except cmap, post and head's
// checksum adjustment byte-for-byte unchanged. In particular glyph IDs,
// outlines/instructions, metrics and names retain their pinned contents.
// Sorted records, padding and checksums remain valid. This is not a production
// font writer, and it does not add any production font dependency.
Bytes Font(Tables tables) {
  PutLong(tables.at(Tag("head")), 8, 0);
  const size_t count = tables.size();
  Bytes out(12 + count * 16);
  PutLong(out, 0, 0x00010000); PutWord(out, 4, count);
  unsigned power = 1, selector = 0;
  while (power * 2 <= count) { power *= 2; ++selector; }
  PutWord(out, 6, power * 16); PutWord(out, 8, selector); PutWord(out, 10, count * 16 - power * 16);
  size_t index = 0, head = 0;
  for (const auto& [tag, data] : tables) {
    while (out.size() % 4) out.push_back(0);
    const size_t row = 12 + index++ * 16, offset = out.size();
    PutLong(out, row, tag); PutLong(out, row + 4, Checksum(data));
    PutLong(out, row + 8, offset); PutLong(out, row + 12, data.size());
    if (tag == Tag("head")) head = offset;
    out.insert(out.end(), data.begin(), data.end());
  }
  while (out.size() % 4) out.push_back(0);
  PutLong(out, head + 8, 0xb1b0afba - Checksum(out));
  EXPECT_EQ(Checksum(out), 0xb1b0afbau);
  return out;
}

struct Map {
  uint16_t platform, encoding;
  std::map<uint16_t,uint16_t> glyphs;
};
Bytes Cmap(const std::vector<Map>& maps) {
  Bytes result(4+maps.size()*8);
  PutWord(result,2,maps.size());
  for(size_t i=0;i<maps.size();++i) {
    const auto& map=maps[i];
    PutWord(result,4+i*8,map.platform);PutWord(result,6+i*8,map.encoding);
    PutLong(result,8+i*8,result.size());
    // A short format-4 subtable has one single-code segment per assignment.
    // Keeping the same format across platform records isolates map selection.
    const size_t count=map.glyphs.size()+1;
    Bytes sub(16+count*8);
    PutWord(sub,0,4);PutWord(sub,2,sub.size());PutWord(sub,6,count*2);
    unsigned power=1,selector=0;
    while(power*2<=count){power*=2;++selector;}
    PutWord(sub,8,power*2);PutWord(sub,10,selector);PutWord(sub,12,count*2-power*2);
    size_t j=0;
    for(const auto& [code,glyph]:map.glyphs) {
      PutWord(sub,14+j*2,code);PutWord(sub,16+count*2+j*2,code);
      PutWord(sub,16+count*4+j*2,static_cast<uint16_t>(glyph-code));++j;
    }
    PutWord(sub,14+j*2,0xffff);PutWord(sub,16+count*2+j*2,0xffff);
    PutWord(sub,16+count*4+j*2,1);
    result.insert(result.end(),sub.begin(),sub.end());
  }
  return result;
}
Bytes Program(const std::vector<Map>& maps,bool names=false) {
  auto tables=AhemTables();tables[Tag("cmap")]=Cmap(maps);
  if(!names){tables[Tag("post")].resize(32);PutLong(tables[Tag("post")],0,0x00030000);}
  return Font(std::move(tables));
}
std::map<uint16_t,uint16_t> Letters(uint16_t prefix=0,bool blank=false) {
  std::map<uint16_t,uint16_t> out;
  for(uint16_t code='A';code<='C';++code)out[prefix+code]=AhemGlyph(blank?' ':code);
  return out;
}
std::string Hex(uint16_t value,int digits) {
  static constexpr char hex[]="0123456789ABCDEF";
  std::string out(digits,'0');for(int i=digits-1;i>=0;--i){out[i]=hex[value&15];value>>=4;}return out;
}
struct Case {
  std::string label;
  std::vector<Map> maps;
  int flags=32;
  std::string encoding="/WinAnsiEncoding";
  std::vector<uint8_t> codes{'A','B','C'};
  std::u16string unicode=u"ABC";
  std::vector<bool> ink{true,true,true};
  bool names=false;
};
std::vector<uint8_t> Fixture(const Case& c) {
  const auto font=Program(c.maps,c.names);
  std::string widths,encoded,mappings;
  for(int code=0;code<256;++code)widths+="1000 ";
  for(size_t i=0;i<c.codes.size();++i) {
    encoded+=Hex(c.codes[i],2);
    mappings+="<"+Hex(c.codes[i],2)+"> <"+Hex(c.unicode.at(i),4)+">\n";
  }
  const auto unicode="/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n"
    "/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n"
    "/CMapName /MappingTest def /CMapType 2 def\n"
    "1 begincodespacerange <00> <FF> endcodespacerange\n"+
    std::to_string(c.codes.size())+" beginbfchar\n"+mappings+"endbfchar\n"
    "endcmap CMapName currentdict /CMap defineresource pop end end";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 160 100] /Resources << /Font << /F 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /TrueType /BaseFont /Ahem /FirstChar 0 /LastChar 255 /Widths ["+widths+
      "] /FontDescriptor 6 0 R /ToUnicode 8 0 R "+(c.encoding.empty()?"":"/Encoding "+c.encoding)+" >>",
    PdfStream("","BT /F 24 Tf 1 0 0 1 20 40 Tm <"+encoded+"> Tj ET"),
    "<< /Type /FontDescriptor /FontName /Ahem /Flags "+std::to_string(c.flags)+
      " /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /CapHeight 800 /ItalicAngle 0 /StemV 80 /FontFile2 7 0 R >>",
    PdfStream("/Length1 "+std::to_string(font.size()),std::string(font.begin(),font.end())),
    PdfStream("",unicode)});
}
void Inspect(const Api& api,FPDF_PAGE page,const Case& c,const Raster& image) {
  Handle text(api.FPDFText_LoadPage(page),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
  EXPECT_EQ(text_fixture::Extract(api,text.get()),c.unicode);
  ASSERT_EQ(api.FPDFText_CountChars(text.get()),c.unicode.size());
  ASSERT_EQ(image.width,160);ASSERT_EQ(image.height,100);
  for(size_t i=0;i<c.unicode.size();++i) {
    EXPECT_EQ(api.FPDFText_GetUnicode(text.get(),i),c.unicode[i]);
    double x=0,y=0;ASSERT_TRUE(api.FPDFText_GetCharOrigin(text.get(),i,&x,&y));
    EXPECT_NEAR(x,20+24*i,.02);EXPECT_NEAR(y,40,.02);
    // Ahem's unchanged A/B/C outlines fill [0,-200,1000,800]. Space has
    // no contours. Probe a full interior rectangle, so .notdef's hollow box
    // cannot masquerade as the intended square or blank mapping.
    for(int dy=0;dy<8;++dy)for(int dx=0;dx<8;++dx) {
      const auto pixel=(static_cast<size_t>(52+dy)*image.width+28+24*i+dx)*4;
      const uint8_t expected=c.ink.at(i)?0:255;
      for(size_t channel=0;channel<3;++channel)EXPECT_EQ(image.pixels[pixel+channel],expected);
    }
    if(c.ink.at(i)) {
      double left=0,right=0,bottom=0,top=0;
      ASSERT_TRUE(api.FPDFText_GetCharBox(text.get(),i,&left,&right,&bottom,&top));
      EXPECT_NEAR(left,20+24*i,.05);EXPECT_NEAR(right,44+24*i,.05);
      EXPECT_NEAR(bottom,35.2,.05);EXPECT_NEAR(top,59.2,.05);
    }
  }
  for(int y:{5,90})for(int x:{5,150}) {
    const auto pixel=(static_cast<size_t>(y)*image.width+x)*4;
    for(size_t channel=0;channel<3;++channel)EXPECT_EQ(image.pixels[pixel+channel],255);
  }
}
void RunFontCases(const std::vector<Case>& cases) {
  Artifacts artifacts;
  for(const auto& c:cases) {
    SCOPED_TRACE(c.label);ASSERT_EQ(c.codes.size(),c.unicode.size());ASSERT_EQ(c.codes.size(),c.ink.size());
    const auto bytes=Fixture(c);std::array<Raster,2> images;size_t backend=0;
    for(const auto* writer:Implementations()) {
      SCOPED_TRACE(writer->name());const auto& api=writer->api();
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
      ASSERT_NE(page.get(),nullptr);images[backend]=Render(api,page.get(),160,100);
      ASSERT_NO_FATAL_FAILURE(Inspect(api,page.get(),c,images[backend]));
      const auto saved=SaveDocument(api,document.get(),artifacts,c.label+writer->name());ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());const auto& read=reader->api();
        Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);
        ASSERT_NE(reopened.get(),nullptr);Handle reloaded(read.FPDF_LoadPage(reopened.get(),0),read.FPDF_ClosePage);
        ASSERT_NE(reloaded.get(),nullptr);const auto after=Render(read,reloaded.get(),160,100);
        ASSERT_NO_FATAL_FAILURE(Inspect(read,reloaded.get(),c,after));
        CompareRaster(images[backend],after,artifacts,c.label+writer->name()+reader->name());
      }
      ++backend;
    }
    CompareRaster(images[0],images[1],artifacts,c.label+"-engines");
  }
}

TEST(CoverageTrueTypeMapping, WindowsSymbolSearchTriesLowAndThreePrivateUsePrefixes) {
  std::vector<Case> cases;
  for(uint16_t prefix:{0x0000,0xf000,0xf100,0xf200})for(int flags:{4,32})
    for(const std::string& encoding:{std::string(),std::string("/WinAnsiEncoding"),std::string("/MacRomanEncoding")}) {
      Case c;c.label="symbol-"+std::to_string(prefix)+"-"+std::to_string(flags)+
          (encoding.empty()?"-builtin":"-"+encoding.substr(1));
      c.maps={{3,0,Letters(prefix)}};c.flags=flags;c.encoding=encoding;cases.push_back(c);
    }
  RunFontCases(cases);
}

TEST(CoverageTrueTypeMapping, MacOnlyFontsConvertWinAnsiAndInferSymbolicBaseEncoding) {
  std::vector<Case> cases;
  for(int flags:{4,32})for(bool win:{false,true}) {
    Case c;c.label="mac-"+std::to_string(flags)+(win?"-win":"-mac");
    c.maps={{1,0,{{0x80,AhemGlyph('A')},{0x8e,AhemGlyph('B')}}}};
    c.flags=flags;c.encoding=win?"/WinAnsiEncoding":"/MacRomanEncoding";
    c.codes=(win&&flags==32)?std::vector<uint8_t>{0xc4,0xe9}:std::vector<uint8_t>{0x80,0x8e};
    c.unicode=u"\u00c4\u00e9";c.ink={true,true};cases.push_back(c);
  }
  // Builtin symbolic mapping uses Mac codes directly, independently of names.
  auto builtin=cases.front();builtin.label="mac-builtin";builtin.encoding.clear();cases.push_back(builtin);
  RunFontCases(cases);
}

TEST(CoverageTrueTypeMapping, CharmapPreferenceSeparatesUnicodeSymbolAndMacintoshRecords) {
  std::vector<Case> cases;
  for(bool reversed:{false,true}) {
    Case unicode;unicode.label=reversed?"windows-unicode-last":"windows-unicode-first";
    unicode.maps={{3,1,Letters()},{3,0,Letters(0xf000,true)},{1,0,Letters(0,true)}};
    if(reversed){std::reverse(unicode.maps.begin(),unicode.maps.end());}cases.push_back(unicode);
    auto nonsymbolic=unicode;nonsymbolic.label=reversed?"mac-last":"mac-first";
    nonsymbolic.maps={{3,0,Letters(0xf000,true)},{1,0,Letters()}};
    if(reversed){std::reverse(nonsymbolic.maps.begin(),nonsymbolic.maps.end());}cases.push_back(nonsymbolic);
    auto symbolic=nonsymbolic;symbolic.label=reversed?"symbol-last":"symbol-first";symbolic.flags=4;
    symbolic.encoding="<< /BaseEncoding /WinAnsiEncoding /Differences [65 /A /B /C] >>";
    symbolic.maps={{1,0,Letters(0,true)},{3,0,Letters(0xf000)}};
    if(reversed){std::reverse(symbolic.maps.begin(),symbolic.maps.end());}cases.push_back(symbolic);
    // An Apple Unicode record is not chosen over an explicit symbol record.
    auto apple=symbolic;apple.label=reversed?"apple-shadow-last":"apple-shadow-first";
    apple.maps={{0,3,Letters(0,true)},{3,0,Letters(0xf100)}};
    if(reversed){std::reverse(apple.maps.begin(),apple.maps.end());}cases.push_back(apple);
  }
  RunFontCases(cases);
}

TEST(CoverageTrueTypeMapping, DifferenceNamesFallbackThroughPostNamesToUnicodeAndNotdefSpace) {
  std::vector<Case> cases;
  Case post;post.label="missing-cmap-post-name";post.names=true;
  post.maps={{3,1,{{0x2603,AhemGlyph(' ')}}}};
  post.encoding="<< /BaseEncoding /WinAnsiEncoding /Differences [65 /A /B /C] >>";cases.push_back(post);
  Case fallback;fallback.label="missing-adobe-name-tounicode";
  fallback.maps={{3,1,{{0x03a9,AhemGlyph('A')},{0x03a3,AhemGlyph('B')},{0x03a0,AhemGlyph('C')}}}};
  fallback.encoding="<< /BaseEncoding /WinAnsiEncoding /Differences [65 /CoverageMissingA /CoverageMissingB /CoverageMissingC] >>";
  fallback.unicode=u"\u03a9\u03a3\u03a0";cases.push_back(fallback);
  Case notdef;notdef.label="notdef-selects-space-not-visible-box";
  notdef.maps={{3,1,{{32,AhemGlyph(' ')},{'B',AhemGlyph('B')},{'C',AhemGlyph('C')}}}};
  notdef.encoding="<< /BaseEncoding /WinAnsiEncoding /Differences [65 /.notdef /B /C] >>";
  notdef.ink={false,true,true};cases.push_back(notdef);
  RunFontCases(cases);
}

TEST(CoverageTrueTypeMapping, EmptySymbolMapsContinueToMacintoshAndUnicodeFallbacks) {
  Case mac;mac.label="empty-symbol-then-mac";mac.flags=4;mac.encoding.clear();
  mac.maps={{3,0,{}},{1,0,Letters()}};
  Case unicode=mac;unicode.label="empty-symbol-then-unicode";unicode.maps={{3,0,{}},{3,1,Letters()}};
  RunFontCases({mac,unicode});
}

TEST(CoverageTrueTypeMapping, UnrelatedCharmapFallsBackToOriginalGlyphIds) {
  Case direct;direct.label="sjis-map-keeps-direct-glyph-id-fallback";direct.flags=4;direct.encoding.clear();
  direct.maps={{3,2,{{0x8140,AhemGlyph(' ')}}}};
  direct.codes={static_cast<uint8_t>(AhemGlyph('A')),static_cast<uint8_t>(AhemGlyph('B')),static_cast<uint8_t>(AhemGlyph('C'))};
  RunFontCases({direct});
}

TEST(CoverageTrueTypeMapping, EmbeddedAllCapsPreservesExplicitLowercaseGlyphsAndWidths) {
  std::vector<Case> cases;
  for(int flags:{32,32+65536,4,4+65536}) {
    Case c;c.label="embedded-caps-"+std::to_string(flags);c.flags=flags;c.codes={'a','b','c'};c.unicode=u"abc";
    c.maps={{3,1,{{'A',AhemGlyph('A')},{'B',AhemGlyph('B')},{'C',AhemGlyph('C')},
                  {'a',AhemGlyph(' ')},{'b',AhemGlyph('B')},{'c',AhemGlyph('C')}}}};
    c.ink={false,true,true};cases.push_back(c);
  }
  RunFontCases(cases);
}
}  // namespace
}  // namespace pdfoxium::integration
