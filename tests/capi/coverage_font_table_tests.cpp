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
void AppendWord(Bytes& bytes, uint16_t value) {
  bytes.push_back(value >> 8); bytes.push_back(value);
}
void AppendLong(Bytes& bytes, uint32_t value) {
  AppendWord(bytes, value >> 16); AppendWord(bytes, value);
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

// Test-only SFNT assembly keeps the pinned outlines/metrics unchanged. Sorted
// table records, padding and checksums remain valid after replacing a cmap or
// adding a small source-derived GSUB fixture. This is not a production writer.
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
Bytes UnicodeCmap(int format, uint16_t glyph) {
  Bytes sub;
  const std::array<uint16_t, 4> glyphs{glyph, AhemGlyph('B'), AhemGlyph('C'), AhemGlyph('D')};
  if (format == 2) {
    sub.resize(542);
    PutWord(sub, 0, 2); PutWord(sub, 2, sub.size());
    PutWord(sub, 6 + 0x81 * 2, 8);
    PutWord(sub, 518, 65); PutWord(sub, 520, 2); PutWord(sub, 524, 10);
    PutWord(sub, 526, 0x40); PutWord(sub, 528, 2); PutWord(sub, 532, 6);
    for (size_t i = 0; i < glyphs.size(); ++i) PutWord(sub, 534 + i * 2, glyphs[i]);
  } else if (format == 6) {
    for (uint16_t value : {uint16_t(6), uint16_t(16), uint16_t(0), uint16_t(65), uint16_t(3), glyphs[0], glyphs[1], glyphs[2]}) AppendWord(sub, value);
  } else if (format == 10) {
    AppendWord(sub, 10); AppendWord(sub, 0); AppendLong(sub, 26); AppendLong(sub, 0);
    AppendLong(sub, 65); AppendLong(sub, 3);
    for (int i = 0; i < 3; ++i) AppendWord(sub, glyphs[i]);
  } else {
    AppendWord(sub, 13); AppendWord(sub, 0); AppendLong(sub, 52); AppendLong(sub, 0);
    AppendLong(sub, 3);
    for (int i = 0; i < 3; ++i) {
      AppendLong(sub, 65 + i); AppendLong(sub, 65 + i); AppendLong(sub, glyphs[i]);
    }
  }
  Bytes cmap;
  for (uint16_t value : {0, 1, 3, (format >= 10 ? 10 : 1)}) AppendWord(cmap, value);
  AppendLong(cmap, 12); cmap.insert(cmap.end(), sub.begin(), sub.end());
  return cmap;
}
void CheckText(const Api& api, FPDF_PAGE page, const std::u16string& expected) {
  Handle text(api.FPDFText_LoadPage(page), api.FPDFText_ClosePage);
  ASSERT_NE(text.get(), nullptr);
  EXPECT_EQ(text_fixture::Extract(api, text.get()), expected);
  EXPECT_EQ(api.FPDFText_CountChars(text.get()), expected.size());
  for (size_t i = 0; i < expected.size(); ++i) {
    EXPECT_EQ(api.FPDFText_GetUnicode(text.get(), i), expected[i]);
    double x = 0, y = 0;
    ASSERT_TRUE(api.FPDFText_GetCharOrigin(text.get(), i, &x, &y));
    EXPECT_NEAR(x, 20 + 24 * i, 0.02); EXPECT_NEAR(y, 40, 0.02);
  }
}
void Squares(const Raster& image, size_t count) {
  for (size_t i = 0; i < count; ++i) {
    const auto at = (52 * image.width + 32 + 24 * i) * 4;
    EXPECT_EQ(image.pixels[at], 0); EXPECT_EQ(image.pixels[at + 1], 0); EXPECT_EQ(image.pixels[at + 2], 0);
  }
  EXPECT_EQ(image.pixels[(5 * image.width + 5) * 4], 255);
}
void FontWorkflow(const Bytes& program, const std::u16string& value, const std::string& label) {
  Artifacts artifacts;
  std::array<Raster, 2> original;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name() + "/" + label);
    const auto& api = writer->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, 160, 100), api.FPDF_ClosePage);
    Handle font(api.FPDFText_LoadFont(document.get(), program.data(), program.size(), FPDF_FONT_TRUETYPE, 1), api.FPDFFont_Close);
    ASSERT_NE(font.get(), nullptr);
    Handle object(api.FPDFPageObj_CreateTextObj(document.get(), font.get(), 24), api.FPDFPageObj_Destroy);
    ASSERT_NE(object.get(), nullptr);
    const auto text = text_fixture::Wide(value);
    ASSERT_TRUE(api.FPDFText_SetText(object.get(), text.data()));
    const FS_MATRIX matrix{1, 0, 0, 1, 20, 40};
    ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object.get(), &matrix));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), object.release()));
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    CheckText(api, page.get(), value);
    original[backend] = Render(api, page.get(), 160, 100); Squares(original[backend], value.size());
    const auto saved = SaveDocument(api, document.get(), artifacts, writer->name() + label);
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name()); const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      Handle after(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
      CheckText(read, after.get(), value);
      const auto image = Render(read, after.get(), 160, 100); Squares(image, value.size());
      CompareRaster(original[backend], image, artifacts, label + writer->name() + reader->name());
    }
    ++backend;
  }
  CompareRaster(original[0], original[1], artifacts, label + "-backends");
}

TEST(CoverageFontTables, RareUnicodeCmapFormatsReachActualEmbeddedGlyphsAndSaveReload) {
  for (int format : {2, 6, 10, 13}) {
    SCOPED_TRACE(format);
    auto tables = AhemTables(); tables[Tag("cmap")] = UnicodeCmap(format, AhemGlyph('A'));
    if (format == 2) {
      tables[Tag("post")].resize(32); PutLong(tables[Tag("post")], 0, 0x00030000);
    }
    FontWorkflow(Font(std::move(tables)), format == 2 ? u"AB\u8140\u8141" : u"ABC", "unicode-cmap" + std::to_string(format));
  }
}

TEST(CoverageFontTables, PostGlyphNamesSupplyUnicodeWithMissingOrMacOnlyCmap) {
  for (bool mac_only : {false, true}) {
    SCOPED_TRACE(mac_only);
    auto tables = AhemTables(); tables.erase(Tag("cmap"));
    if (mac_only) {
      Bytes cmap(12 + 262); PutWord(cmap, 2, 1); PutWord(cmap, 4, 1); PutLong(cmap, 8, 12);
      PutWord(cmap, 12, 0); PutWord(cmap, 14, 262);
      // The explicit Mac map has no A. Unicode synthesis must consult post
      // glyph names instead of replacing or misinterpreting that charmap.
      tables[Tag("cmap")] = std::move(cmap);
    }
    FontWorkflow(Font(std::move(tables)), u"A", mac_only ? "mac-only" : "post-only");
  }
}

Bytes VerticalSubstitution(uint16_t from, uint16_t to, int substitution_format,
                           int coverage_format, const char* feature_tag) {
  Bytes script;
  AppendWord(script, 1); AppendLong(script, Tag("DFLT")); AppendWord(script, 8);
  AppendWord(script, 10); AppendWord(script, 1);
  AppendLong(script, Tag("JAN ")); AppendWord(script, 18);
  for (int language = 0; language < 2; ++language) {
    AppendWord(script, 0); AppendWord(script, 0xffff);
    AppendWord(script, 1); AppendWord(script, 0);
  }
  Bytes features;
  AppendWord(features, 1); AppendLong(features, Tag(feature_tag)); AppendWord(features, 8);
  AppendWord(features, 0); AppendWord(features, 1); AppendWord(features, 0);
  Bytes substitution;
  AppendWord(substitution, substitution_format);
  AppendWord(substitution, substitution_format == 1 ? 6 : 8);
  if (substitution_format == 1) AppendWord(substitution, static_cast<uint16_t>(to - from));
  else { AppendWord(substitution, 1); AppendWord(substitution, to); }
  AppendWord(substitution, coverage_format); AppendWord(substitution, 1); AppendWord(substitution, from);
  if (coverage_format == 2) { AppendWord(substitution, from); AppendWord(substitution, 0); }
  Bytes lookups;
  AppendWord(lookups, 1); AppendWord(lookups, 4);
  AppendWord(lookups, 1); AppendWord(lookups, 0); AppendWord(lookups, 1); AppendWord(lookups, 8);
  lookups.insert(lookups.end(), substitution.begin(), substitution.end());
  Bytes gsub;
  AppendLong(gsub, 0x00010000); AppendWord(gsub, 10);
  AppendWord(gsub, 10 + script.size()); AppendWord(gsub, 10 + script.size() + features.size());
  for (const auto* part : {&script, &features, &lookups}) gsub.insert(gsub.end(), part->begin(), part->end());
  return gsub;
}

struct VerticalFontProvider : FPDF_SYSFONTINFO {
  const Library& library;
  Bytes bytes;
  std::string name;
  int token = 1, maps = 0;
  VerticalFontProvider(const Library& library, Bytes bytes, std::string name)
      : FPDF_SYSFONTINFO{}, library(library), bytes(std::move(bytes)), name(std::move(name)) {
    version = 2;
    Release = [](FPDF_SYSFONTINFO*) {};
    MapFont = [](FPDF_SYSFONTINFO* info, int, FPDF_BOOL, int, int, const char* family, FPDF_BOOL* exact) -> void* {
      auto& self = *static_cast<VerticalFontProvider*>(info);
      // Native's XFA enumeration revisits already installed family names even
      // for a version-2 provider. Returning our Ahem bytes for unrelated names
      // poisons its retained descriptors: a later Ahem request then loads an
      // unrelated real font after this provider has been removed.
      if (!family || self.name != family) { if (exact) *exact = 0; return nullptr; }
      ++self.maps; if (exact) *exact = 1; return &self.token;
    };
    GetFontData = [](FPDF_SYSFONTINFO* info, void*, unsigned int tag, unsigned char* output, unsigned long capacity) -> unsigned long {
      auto& self = *static_cast<VerticalFontProvider*>(info);
      size_t offset = 0, length = self.bytes.size();
      if (tag) {
        length = 0;
        for (unsigned i = 0; i < Word(self.bytes, 4); ++i) {
          const size_t row = 12 + i * 16;
          if (Long(self.bytes, row) == tag) { offset = Long(self.bytes, row + 8); length = Long(self.bytes, row + 12); break; }
        }
      }
      if (output && capacity >= length) std::copy_n(self.bytes.data() + offset, length, output);
      return length;
    };
    GetFaceName = [](FPDF_SYSFONTINFO* info, void*, char* output, unsigned long capacity) -> unsigned long {
      const auto& name = static_cast<VerticalFontProvider*>(info)->name;
      if (output && capacity >= name.size() + 1) std::copy_n(name.c_str(), name.size() + 1, output);
      return name.size() + 1;
    };
    GetFontCharset = [](FPDF_SYSFONTINFO*, void*) { return FXFONT_SHIFTJIS_CHARSET; };
    DeleteFont = [](FPDF_SYSFONTINFO*, void*) {};
    library.api().FPDF_SetSystemFontInfo(this);
  }
  ~VerticalFontProvider() {
    library.api().FPDF_SetSystemFontInfo(nullptr);
    library.restore_default_font_info();
  }
};

std::vector<uint8_t> VerticalDocument(const std::string& name, bool vertical) {
  // Nonembedded CID fonts exercise the genuine vertical substitution path:
  // explicit PDF CIDToGID maps already specify their final glyph IDs.
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /Font << /F 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", "BT /F 24 Tf 1 0 0 1 60 70 Tm <0041> Tj ET"),
      "<< /Type /Font /Subtype /Type0 /BaseFont /" + name + " /Encoding /UniJIS-UCS2-" +
          (vertical ? "V" : "H") + " /DescendantFonts [6 0 R] /ToUnicode 8 0 R >>",
      "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /" + name +
          " /CIDSystemInfo << /Registry (Adobe) /Ordering (Japan1) /Supplement 6 >> /FontDescriptor 7 0 R /DW 1000 /DW2 [880 -1000] >>",
      "<< /Type /FontDescriptor /FontName /" + name + " /Flags 32 /FontWeight 400 /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /CapHeight 800 /ItalicAngle 0 /StemV 80 >>",
      PdfStream("", "/CIDInit /ProcSet findresource begin 12 dict begin begincmap "
          "/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def "
          "/CMapName /VerticalUnicode def /CMapType 2 def "
          "1 begincodespacerange <0000> <FFFF> endcodespacerange 1 beginbfchar <0041> <0041> endbfchar "
          "endcmap CMapName currentdict /CMap defineresource pop end end")});
}

TEST(CoverageFontTables, VerticalSingleSubstitutionsUseFeaturesCoverageAndWritingDirection) {
  Artifacts artifacts;
  int row = 0;
  for (const auto& kind : std::array<std::pair<int, const char*>, 3>{{{1, "vert"}, {2, "vrt2"}, {2, "liga"}}}) {
    auto tables = AhemTables();
    tables[Tag("GSUB")] = VerticalSubstitution(AhemGlyph('A'), AhemGlyph(' '), kind.first, kind.first, kind.second);
    const auto program = Font(std::move(tables));
    const auto name = "CoverageVertical" + std::to_string(row++);
    std::array<std::unique_ptr<VerticalFontProvider>, 2> providers;
    size_t provider = 0;
    for (const auto* library : Implementations()) providers[provider++] = std::make_unique<VerticalFontProvider>(*library, program, name);
    // The synthetic vertical family must not replace unrelated installed or
    // standard fonts while its provider is active (including XFA enumeration).
    for (const auto* library : Implementations()) {
      const auto& api = library->api();
      Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      Handle font(api.FPDFText_LoadStandardFont(document.get(), "Helvetica"), api.FPDFFont_Close);
      ASSERT_NE(font.get(), nullptr);
      const auto size = api.FPDFFont_GetFamilyName(font.get(), nullptr, 0);
      ASSERT_GT(size, 1u);
      std::vector<char> family(size);
      ASSERT_EQ(api.FPDFFont_GetFamilyName(font.get(), family.data(), size), size);
      EXPECT_STRNE(family.data(), "Ahem");
    }
    for (bool vertical : {false, true}) {
      SCOPED_TRACE(name + (vertical ? "/V" : "/H"));
      const auto bytes = VerticalDocument(name, vertical);
      const bool substituted = vertical && kind.second != std::string("liga");
      std::array<Raster, 2> images;
      size_t backend = 0;
      auto inspect = [&](const Api& api, FPDF_PAGE page) {
        Handle text(api.FPDFText_LoadPage(page), api.FPDFText_ClosePage);
        // Substituting the outline with space deliberately gives this object
        // zero width. CPDF_TextPage::ProcessTextObject omits zero-width objects;
        // horizontal and unrelated-feature controls still extract the PDF A.
        EXPECT_EQ(text_fixture::Extract(api, text.get()), substituted ? u"" : u"A");
        auto image = Render(api, page, 100, 100);
        size_t ink = 0;
        for (size_t i = 0; i < image.pixels.size(); i += 4) ink += image.pixels[i] < 128;
        if (substituted) { EXPECT_EQ(ink, 0u); }
        else { EXPECT_GT(ink, 300u); }
        return image;
      };
      for (const auto* writer : Implementations()) {
        const auto& api = writer->api();
        Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
        ASSERT_NE(document.get(), nullptr);
        Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
        ASSERT_NE(page.get(), nullptr); images[backend] = inspect(api, page.get());
        const auto saved = SaveDocument(api, document.get(), artifacts, name + writer->name());
        for (const auto* reader : Implementations()) {
          const auto& read = reader->api();
          Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
          ASSERT_NE(reopened.get(), nullptr);
          Handle after(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
          CompareRaster(images[backend], inspect(read, after.get()), artifacts, name + writer->name() + reader->name());
        }
        ++backend;
      }
      CompareRaster(images[0], images[1], artifacts, name + "-backends");
    }
    for (const auto& info : providers) EXPECT_GT(info->maps, 0);
  }
}

}  // namespace
}  // namespace pdfoxium::integration
