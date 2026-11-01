// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "text_fixture.h"

#include <algorithm>
#include <array>
#include <cstring>
#include <map>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using Tables = std::map<uint32_t, Bytes>;
uint16_t Word(const Bytes& b, size_t p) { return (uint16_t(b.at(p)) << 8) | b.at(p + 1); }
uint32_t Long(const Bytes& b, size_t p) { return (uint32_t(Word(b, p)) << 16) | Word(b, p + 2); }
void SetWord(Bytes& b, size_t p, uint16_t v) { b.at(p) = v >> 8; b.at(p + 1) = v; }
void SetLong(Bytes& b, size_t p, uint32_t v) { SetWord(b, p, v >> 16); SetWord(b, p + 2, v); }
uint32_t Tag(const char* s) { return (uint32_t(uint8_t(s[0])) << 24) | (uint32_t(uint8_t(s[1])) << 16) | (uint32_t(uint8_t(s[2])) << 8) | uint8_t(s[3]); }
uint32_t Sum(const Bytes& b) { uint32_t s = 0; for (size_t i = 0; i < b.size(); i += 4) { uint32_t v = 0; for (size_t j = 0; j < 4; ++j) v = (v << 8) | (i + j < b.size() ? b[i + j] : 0); s += v; } return s; }
Tables Split(const Bytes& font) { Tables out; for (unsigned i = 0; i < Word(font, 4); ++i) { const auto r = 12 + i * 16; const auto p = Long(font, r + 8), n = Long(font, r + 12); out[Long(font, r)] = Bytes(font.begin() + p, font.begin() + p + n); } return out; }
Bytes Assemble(Tables tables) {
  SetLong(tables.at(Tag("head")), 8, 0);
  Bytes out(12 + tables.size() * 16); SetLong(out, 0, 0x10000); SetWord(out, 4, tables.size());
  unsigned power = 1, selector = 0; while (power * 2 <= tables.size()) { power *= 2; ++selector; }
  SetWord(out, 6, power * 16); SetWord(out, 8, selector); SetWord(out, 10, tables.size() * 16 - power * 16);
  size_t index = 0, head = 0;
  for (const auto& [tag, bytes] : tables) { while (out.size() % 4) out.push_back(0); const auto row = 12 + 16 * index++, start = out.size(); SetLong(out, row, tag); SetLong(out, row + 4, Sum(bytes)); SetLong(out, row + 8, start); SetLong(out, row + 12, bytes.size()); if (tag == Tag("head")) head = start; out.insert(out.end(), bytes.begin(), bytes.end()); }
  while (out.size() % 4) { out.push_back(0); }
  SetLong(out, head + 8, 0xb1b0afba - Sum(out)); return out;
}
// Outlines, glyph IDs, hints and advances are unchanged. Style bits describe
// the requested external face, so native synthetic bold/skew is unnecessary.
// Legacy faces use a bounded byte cmap and post3: real post glyph names would
// allow FreeType/Fontations to synthesize a Unicode map and bypass this case.
void RenameNames(Bytes& names, std::initializer_list<uint16_t> ids, const std::string& replacement) {
  const auto strings = Word(names, 4);
  for (unsigned i = 0; i < Word(names, 2); ++i) {
    const auto record = 6 + i * 12;
    if (std::find(ids.begin(), ids.end(), Word(names, record + 6)) == ids.end()) continue;
    Bytes encoded;
    for (const auto c : replacement) { if (Word(names, record) == 3) encoded.push_back(0); encoded.push_back(uint8_t(c)); }
    SetWord(names, record + 8, encoded.size()); SetWord(names, record + 10, names.size() - strings);
    names.insert(names.end(), encoded.begin(), encoded.end());
  }
}
Bytes Ahem(bool bold = false, bool italic = false, int legacy_encoding = -1, const std::string& legacy_family = "") {
  auto tables = Split(text_fixture::Resource("fonts/ahem/Ahem.ttf"));
  if (bold || italic) {
    auto& os2 = tables.at(Tag("OS/2")); auto flags = Word(os2, 62); flags &= ~uint16_t(0x61);
    if (bold) { flags |= 0x20; }
    if (italic) { flags |= 1; }
    if (!bold && !italic) { flags |= 0x40; }
    SetWord(os2, 62, flags); SetWord(os2, 4, bold ? 700 : 400);
    SetWord(tables.at(Tag("head")), 44, (bold ? 1 : 0) | (italic ? 2 : 0));
  }
  if (legacy_encoding >= 0) {
    const auto original = tables.at(Tag("cmap")); size_t format4 = 0;
    for (unsigned i = 0; i < Word(original, 2); ++i) { const auto start = Long(original, 8 + i * 8); if (Word(original, start) == 4) { format4 = start; break; } }
    EXPECT_NE(format4, 0u);
    const unsigned count = Word(original, format4 + 6) / 2;
    const auto ends = format4 + 14, starts = ends + count * 2 + 2, deltas = starts + count * 2, ranges = deltas + count * 2;
    const auto glyph = [&](uint16_t code) -> uint16_t {
      for (unsigned i = 0; i < count; ++i) {
        if (code < Word(original, starts + i * 2) || code > Word(original, ends + i * 2)) continue;
        const auto range = Word(original, ranges + i * 2), delta = Word(original, deltas + i * 2);
        if (!range) return uint16_t(code + delta);
        const auto index = Word(original, ranges + i * 2 + range + 2 * (code - Word(original, starts + i * 2)));
        return index ? uint16_t(index + delta) : 0;
      }
      return 0;
    };
    // Format0 has exactly256 one-byte glyph IDs. Ahem's A is original gid35;
    // unmapped byte codes and original gids above255 stay explicitly missing.
    Bytes map(12 + 262); SetWord(map, 2, 1); SetWord(map, 4, 1); SetWord(map, 6, legacy_encoding); SetLong(map, 8, 12); SetWord(map, 12, 0); SetWord(map, 14, 262);
    for (unsigned code = 0; code < 256; ++code) { const auto id = glyph(code); map[18 + code] = id <= 255 ? uint8_t(id) : 0; }
    EXPECT_EQ(map[18 + 'A'], 35);
    tables[Tag("cmap")] = std::move(map);
    auto post = tables.at(Tag("post")); post.resize(32); SetLong(post, 0, 0x30000); tables[Tag("post")] = std::move(post);
    if (!legacy_family.empty()) RenameNames(tables.at(Tag("name")), {1, 4, 6}, legacy_family);
  }
  return Assemble(std::move(tables));
}
enum class NamePolicy { Normal, Absent, ShortRead, NoPostscriptName };
struct Config {
  std::string base = "RecoveryFace", expected_family = "RecoveryFace", enumeration = "Recovery Installed", postscript_name, face_name;
  int flags = 32, expected_weight = 400, expected_pitch = -1, expected_charset = FXFONT_ANSI_CHARSET;
  int angle = 0, legacy_encoding = -1;
  bool expected_italic = false, bold_face = false, italic_face = false;
  bool map_fails = false, enum_get_fails = false, face_name_absent = false, localized_alias = false;
  bool cid = false, legacy_chinese = false;
  NamePolicy name_policy = NamePolicy::Normal;
};
struct Request { int weight; bool italic; int charset, pitch; std::string family; };
struct Provider : FPDF_SYSFONTINFO {
  Provider(const Api& value, Config options) : api(value), config(std::move(options)), bytes(Ahem(config.bold_face, config.italic_face, config.legacy_encoding, config.face_name)) {
    version = 1; Release = Drop; EnumFonts = Enum; MapFont = Map; GetFont = Get; GetFontData = Data;
    GetFaceName = config.face_name_absent ? nullptr : Face; GetFontCharset = Charset; DeleteFont = Delete;
    names = Split(bytes).at(Tag("name"));
    if (!config.postscript_name.empty()) {
      RenameNames(names, {6}, config.postscript_name);
      auto tables = Split(bytes); tables[Tag("name")] = names; bytes = Assemble(std::move(tables));
    }
    if (config.name_policy == NamePolicy::NoPostscriptName) {
      // Remove nameID6 records, preserving a valid format0 table and every
      // unrelated string/record rather than inventing duplicate version IDs.
      const auto strings = Word(names, 4); Bytes records;
      for (unsigned i = 0; i < Word(names, 2); ++i) {
        const auto start = 6 + i * 12;
        if (Word(names, start + 6) != 6) records.insert(records.end(), names.begin() + start, names.begin() + start + 12);
      }
      Bytes revised(6); SetWord(revised, 2, records.size() / 12); SetWord(revised, 4, 6 + records.size());
      revised.insert(revised.end(), records.begin(), records.end()); revised.insert(revised.end(), names.begin() + strings, names.end()); names = std::move(revised);
      auto tables = Split(bytes); tables[Tag("name")] = names; bytes = Assemble(std::move(tables));
    }
  }
  static Provider& Self(FPDF_SYSFONTINFO* p) { return *static_cast<Provider*>(p); }
  static void Drop(FPDF_SYSFONTINFO* p) { ++Self(p).releases; }
  static void Enum(FPDF_SYSFONTINFO* p, void* mapper) {
    auto& s = Self(p); ++s.enumerations; s.valid &= mapper != nullptr;
    if (mapper) {
      // Enumerating a second real alias makes this provider's localized name
      // probe independent of the mapper's retained last-family optimization.
      s.api.FPDF_AddInstalledFont(mapper, "Recovery Provider Sentinel", FXFONT_ANSI_CHARSET);
      s.api.FPDF_AddInstalledFont(mapper, s.config.enumeration.c_str(), FXFONT_ANSI_CHARSET);
      s.api.FPDF_AddInstalledFont(mapper, s.config.enumeration.c_str(), FXFONT_ANSI_CHARSET);
    }
  }
  bool CatalogueName(const char* family) const {
    return family && (config.enumeration == family || std::strcmp(family, "Recovery Provider Sentinel") == 0);
  }
  bool MainName(const char* family) const {
    if (!family) return false;
    if (config.base == family || (!config.expected_family.empty() && config.expected_family == family)) return true;
    if (!config.legacy_chinese) return false;
    // These PDF names use explicit CP936 byte escapes, not UTF8 family names.
    std::string decoded;
    const auto hex = [](char c) { return c >= '0' && c <= '9' ? c - '0' : c >= 'A' && c <= 'F' ? c - 'A' + 10 : c - 'a' + 10; };
    for (size_t i = 0; i < config.base.size(); ++i) {
      if (config.base[i] == '#' && i + 2 < config.base.size()) { decoded.push_back(char(16 * hex(config.base[i + 1]) + hex(config.base[i + 2]))); i += 2; }
      else decoded.push_back(config.base[i]);
    }
    return decoded == family;
  }
  static void* Map(FPDF_SYSFONTINFO* p, int weight, FPDF_BOOL italic, int charset, int pitch, const char* family, FPDF_BOOL* exact) {
    auto& s = Self(p); s.valid &= family != nullptr;
    const bool own_name = s.CatalogueName(family) || s.MainName(family);
    // Native XFA revisits old installed families using unspecified weight0.
    // Accept that weight only for a pending GetFont failure on this exact
    // owned family. Rust may consume EnumFonts' collected entries after the
    // callback returns; the public Get->Map fallback remains the same.
    const bool zero_fallback = family && s.pending_get_fallback == family;
    if (!own_name || (weight == 0 && !zero_fallback)) { if (exact) *exact = false; ++s.rejected_catalogue_requests; return nullptr; }
    if (weight == 0) s.pending_get_fallback.clear();
    if (exact) *exact = true;
    s.requests.push_back({weight, !!italic, charset, pitch, family ? family : ""}); return s.config.map_fails ? nullptr : &s.token;
  }
  static void* Get(FPDF_SYSFONTINFO* p, const char* family) {
    auto& s = Self(p); s.valid &= family != nullptr;
    if (!s.CatalogueName(family)) return nullptr;
    s.gets.push_back(family);
    if (s.config.enum_get_fails) { s.pending_get_fallback = family; return nullptr; }
    return &s.token;
  }
  static unsigned long Data(FPDF_SYSFONTINFO* p, void* font, unsigned int table, unsigned char* buffer, unsigned long capacity) {
    auto& s = Self(p); s.valid &= font == &s.token;
    const Bytes* data = &s.bytes; Bytes piece;
    if (table == Tag("name")) { ++s.name_queries; if (s.config.name_policy == NamePolicy::Absent) return 0; data = &s.names; }
    else if (table) { const auto all = Split(s.bytes); const auto entry = all.find(table); if (entry == all.end()) return 0; piece = entry->second; data = &piece; }
    else ++s.full_queries;
    if (!buffer || capacity < data->size()) return data->size();
    const size_t count = table == Tag("name") && s.config.name_policy == NamePolicy::ShortRead ? data->size() - 1 : data->size();
    std::copy_n(data->data(), count, buffer); return count;
  }
  static unsigned long Face(FPDF_SYSFONTINFO* p, void* font, char* output, unsigned long capacity) {
    auto& s = Self(p); s.valid &= font == &s.token;
    const auto& family = s.config.face_name; const char* name = family.empty() ? "Ahem" : family.c_str();
    const auto length = std::strlen(name) + 1; if (output && capacity >= length) std::memcpy(output, name, length); return length;
  }
  static int Charset(FPDF_SYSFONTINFO*, void*) { return FXFONT_ANSI_CHARSET; }
  static void Delete(FPDF_SYSFONTINFO* p, void* font) { auto& s = Self(p); s.valid &= font == &s.token; ++s.deletions; }
  const Api& api; Config config; Bytes bytes, names; int token = 0;
  int releases = 0, enumerations = 0, name_queries = 0, full_queries = 0, deletions = 0;
  bool valid = true; std::string pending_get_fallback; size_t rejected_catalogue_requests = 0; std::vector<Request> requests; std::vector<std::string> gets;
};
struct Installed {
  Installed(const Library& owner, Provider& provider) : library(owner) { library.api().FPDF_SetSystemFontInfo(&provider); }
  ~Installed() { library.restore_default_font_info(); }
  const Library& library;
};
std::string Unicode(bool cid) { return std::string("/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /AliasUnicode def /CMapType 2 def 1 begincodespacerange ") + (cid ? "<0000> <ffff>" : "<00> <ff>") + " endcodespacerange 1 beginbfchar " + (cid ? "<0041>" : "<41>") + " <0041> endbfchar endcmap CMapName currentdict /CMap defineresource pop end end"; }
Bytes Fixture(const Config& c, const Bytes* embedded = nullptr) {
  std::string widths;
  // An unused B has a different PDF width. Otherwise 256 identical widths
  // would add FixedPitch automatically, obscuring the descriptor flag test.
  for (int i = 0; i < 256; ++i) widths += i == 'B' ? "900 " : "1000 ";
  const std::string descriptor = "<< /Type /FontDescriptor /FontName /" + c.base + " /Flags " + std::to_string(c.flags) + " /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /CapHeight 800 /StemV 80 /FontWeight 400 /ItalicAngle " + std::to_string(c.angle) + (embedded ? " /FontFile2 8 0 R" : "") + " >>";
  std::string font;
  if (c.cid && !embedded) font = "<< /Type /Font /Subtype /Type0 /BaseFont /" + c.base + " /Encoding /Identity-H /DescendantFonts [9 0 R] /ToUnicode 7 0 R >>";
  else font = "<< /Type /Font /Subtype /TrueType /BaseFont /" + c.base + " /FirstChar 0 /LastChar 255 /Widths [" + widths + "] /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R /ToUnicode 7 0 R >>";
  std::vector<std::string> objects{
      "<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << /Font << /F 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", std::string("BT /F 24 Tf ") + (embedded && c.legacy_chinese ? "50 Tz " : "") + "20 40 Td " + (c.cid && !embedded ? "<0041>" : "(A)") + " Tj ET"), font, descriptor,
      PdfStream("", Unicode(c.cid && !embedded)), embedded ? PdfStream("/Length1 " + std::to_string(embedded->size()), std::string(embedded->begin(), embedded->end())) : "null",
      "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /" + c.base + " /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 6 0 R /DW 1000 >>"};
  return PdfFixture(objects);
}
void Inspect(const Api& api, FPDF_PAGE page, bool embedded) {
  Handle text(api.FPDFText_LoadPage(page), api.FPDFText_ClosePage); ASSERT_NE(text.get(), nullptr);
  EXPECT_EQ(text_fixture::Extract(api, text.get()), u"A"); EXPECT_NEAR(api.FPDFText_GetFontSize(text.get(), 0), 24, .001);
  const auto object = api.FPDFPage_GetObject(page, 0); ASSERT_NE(object, nullptr); EXPECT_EQ(api.FPDFPageObj_GetType(object), FPDF_PAGEOBJ_TEXT);
  const auto font = api.FPDFTextObj_GetFont(object); ASSERT_NE(font, nullptr); EXPECT_EQ(api.FPDFFont_GetIsEmbedded(font), embedded ? 1 : 0);
  const auto path = api.FPDFFont_GetGlyphPath(font, 'A', 24); ASSERT_NE(path, nullptr); EXPECT_GT(api.FPDFGlyphPath_CountGlyphSegments(path), 0);
}
void Visible(const Raster& r, bool half_width = false) {
  ASSERT_EQ(r.width, 128); ASSERT_EQ(r.height, 96);
  for (const auto point : {std::array<int, 2>{25, 45}, {half_width ? 30 : 35, 50}}) {
    const auto at = (size_t(95 - point[1]) * 128 + point[0]) * 4;
    for (int c = 0; c < 3; ++c) { EXPECT_EQ(r.pixels[at + c], 0); }
    EXPECT_EQ(r.pixels[at + 3], 255);
  }
  const auto at = (size_t(2) * 128 + 2) * 4; for (int c = 0; c < 4; ++c) EXPECT_EQ(r.pixels[at + c], 255);
}
void Exercise(const Config& config) {
  Artifacts artifacts; std::array<Raster, 2> images; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name() + "-" + config.base); Provider provider(writer->api(), config);
    { Installed installed(*writer, provider); const auto input = Fixture(config); const auto& api = writer->api();
      Handle document(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument); ASSERT_NE(document.get(), nullptr);
      Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr); Inspect(api, page.get(), false);
      images[backend] = Render(api, page.get(), 128, 96); Visible(images[backend], config.legacy_chinese);
      const auto saved = SaveDocument(api, document.get(), artifacts, "provider-font"); ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        const auto reload = [&]() {
          const auto& read = reader->api(); Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument); ASSERT_NE(reopened.get(), nullptr);
          Handle loaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage); ASSERT_NE(loaded.get(), nullptr); Inspect(read, loaded.get(), false);
          const auto after = Render(read, loaded.get(), 128, 96); Visible(after, config.legacy_chinese);
          CompareRaster(images[backend], after, artifacts, writer->name() + "-" + reader->name());
        };
        if (reader == writer) { ASSERT_NO_FATAL_FAILURE(reload()); }
        else {
          // EnumFonts' opaque mapper belongs to the calling library. A second
          // provider owns that reader's callbacks and outlives its documents.
          Provider peer(reader->api(), config);
          { Installed scope(*reader, peer);
     ASSERT_NO_FATAL_FAILURE(reload()); }
          EXPECT_TRUE(peer.valid); EXPECT_GE(peer.releases, 1);
        }
      }
      // Embedding the same outlines is an independent raster control for the
      // external-provider pipeline; PDF glyph widths and text remain explicit.
      // Legacy Chinese dictionaries have source-defined 500-unit ASCII widths;
      // the control expresses that same half-width through public PDF Tz.
      // The legacy control restores the original Unicode cmap, retaining the
      // identical outlines/glyph IDs/advances. Its purpose is to check the
      // glyph selected by the byte map, without invoking that map twice.
      const auto control_font = config.legacy_encoding < 0 ? provider.bytes : Ahem(config.bold_face, config.italic_face);
      const auto control = Fixture(config, &control_font);
      Handle embedded(api.FPDF_LoadMemDocument64(control.data(), control.size(), nullptr), api.FPDF_CloseDocument); ASSERT_NE(embedded.get(), nullptr);
      Handle control_page(api.FPDF_LoadPage(embedded.get(), 0), api.FPDF_ClosePage); ASSERT_NE(control_page.get(), nullptr); Inspect(api, control_page.get(), true);
      const auto expected = Render(api, control_page.get(), 128, 96); Visible(expected, config.legacy_chinese); CompareRaster(expected, images[backend], artifacts, "embedded-provider-control");
      EXPECT_TRUE(provider.valid); EXPECT_GT(provider.enumerations, 0); EXPECT_GT(provider.full_queries, 0); EXPECT_GT(provider.deletions, 0);
      const auto request = std::find_if(provider.requests.begin(), provider.requests.end(), [&](const Request& r) { return r.weight == config.expected_weight && r.italic == config.expected_italic && (config.expected_family.empty() || r.family == config.expected_family) && r.charset == config.expected_charset; });
      EXPECT_NE(request, provider.requests.end()) << config.expected_family << " weight=" << config.expected_weight;
      if (request != provider.requests.end() && config.expected_pitch >= 0) { EXPECT_EQ(request->pitch, config.expected_pitch); }
      if (config.localized_alias) { EXPECT_GE(provider.name_queries, 2); EXPECT_NE(std::find(provider.gets.begin(), provider.gets.end(), config.enumeration), provider.gets.end()); }
    }
    EXPECT_GE(provider.releases, 1); ++backend;
  }
  CompareRaster(images[0], images[1], artifacts, "font-provider-engines");
}
Config Localized(const char* identity) {
  Config c; c.base = c.postscript_name = std::string("RecoveryPS") + identity;
  c.enumeration = std::string(u8"回復フォント") + identity;
  c.expected_family = c.enumeration; c.map_fails = true; c.localized_alias = true; return c;
}

TEST(CoverageFontProviderAliasBoundaries, LocalizedInstalledFamilyResolvesThroughItsPostscriptNameTable) { Exercise(Localized("Localized")); }
TEST(CoverageFontProviderAliasBoundaries, LocalizedEnumerationUsesMapFontWhenGetFontCannotSupplyItsHandle) { auto c = Localized("EnumFallback"); c.enum_get_fails = true; c.map_fails = false; Exercise(c); }
TEST(CoverageFontProviderAliasBoundaries, MissingNameTableLeavesTheOriginalRequestedFamilyAvailableToMapFont) { auto c = Localized("MissingName"); c.map_fails = false; c.name_policy = NamePolicy::Absent; c.expected_family = c.base; c.localized_alias = false; Exercise(c); }
TEST(CoverageFontProviderAliasBoundaries, ShortNameTableReadsFallBackWithoutDiscardingUsableFontData) { auto c = Localized("ShortName"); c.map_fails = false; c.name_policy = NamePolicy::ShortRead; c.expected_family = c.base; c.localized_alias = false; Exercise(c); }
TEST(CoverageFontProviderAliasBoundaries, ProviderWithoutFaceNameUsesTheParsedSfntFamilyAndRegularStyle) { Config c; c.face_name_absent = true; Exercise(c); }
TEST(CoverageFontProviderAliasBoundaries, PlusSuffixNormalizationStillFindsTheLocalizedPostscriptAlias) { auto c = Localized("PlusSuffix"); c.base += "+Decorative"; Exercise(c); }
TEST(CoverageFontProviderAliasBoundaries, ScriptFamilyAliasesChoosePalaceFrenchAndFreestyleFaces) { for (const auto& pair : {std::pair<const char*, const char*>{"PalaceScript", "PalaceScriptMT"}, {"FrenchScript", "FrenchScriptMT"}, {"FreeStyleScript", "FreeStyleScript"}}) { Config c; c.base = pair.first; c.expected_family = pair.second; Exercise(c); } }
TEST(CoverageFontProviderAliasBoundaries, BoldScriptStyleSelectsTheScriptMtFamilyBeforeTheProviderCall) { Config c; c.base = "RecoveryScript-Bold"; c.expected_family = "ScriptMTBold"; Exercise(c); }
TEST(CoverageFontProviderAliasBoundaries, HistoricalGaramondBankGothicAndForteAliasesUseTheirInstalledFamilyNames) { for (const auto& pair : {std::pair<const char*, const char*>{"AGaramondPro", "Adobe Garamond Pro"}, {"BankGothicBT-Medium", "BankGothic Md BT"}, {"ForteMT", "Forte"}}) { Config c; c.base = pair.first; c.expected_family = pair.second; Exercise(c); } }
TEST(CoverageFontProviderAliasBoundaries, NarrowAndCondensedUnknownFacesRequestTheNarrowFallbackFamily) {
  // CFX_FontMapper's kNarrowFamily is a platform selection before MapFont(),
  // independent of the installed-font provider. Preserve that selection in
  // the provider's accepted family and in the independent callback assertion.
#if defined(__ANDROID__)
  const char* family = "RobotoCondensed";
#elif defined(__linux__) || defined(OS_ASMJS)
  const char* family = "LiberationSansNarrow";
#else
  const char* family = "ArialNarrow";
#endif
  for (const auto* name : {"RecoveryNarrow", "RecoveryCondensed"}) {
    Config c; c.base = name; c.expected_family = family; Exercise(c);
  }
}
TEST(CoverageFontProviderAliasBoundaries, MyriadProClearsTheSerifPitchBeforeExternalSubstitution) { Config c; c.base = c.expected_family = "MyriadPro"; c.flags = 34; c.expected_pitch = 0; Exercise(c); }
TEST(CoverageFontProviderAliasBoundaries, DescriptorPitchFlagsAreDeliveredIndependentlyToTheProvider) { for (const auto& pair : {std::pair<int, int>{33, FXFONT_FF_FIXEDPITCH}, {34, FXFONT_FF_ROMAN}, {40, FXFONT_FF_SCRIPT}}) { Config c; c.flags = pair.first; c.expected_pitch = pair.second; Exercise(c); } }
TEST(CoverageFontProviderAliasBoundaries, StandardFamilyStylesRequestBoldAndItalicFacesWithoutSyntheticOutlineChanges) { for (const auto& name : {"Helvetica,Bold", "Helvetica,Italic", "Helvetica,BoldItalic", "Courier,Bold", "Times-Roman,Bold"}) { Config c; c.base = name; const bool bold = c.base.find("Bold") != std::string::npos; const bool italic = c.base.find("Italic") != std::string::npos; c.expected_weight = bold ? 700 : 400; c.expected_italic = italic; c.bold_face = bold; c.italic_face = italic; c.expected_family = c.base.find("Helvetica") == 0 ? (bold ? (italic ? "Helvetica-BoldOblique" : "Helvetica-Bold") : "Helvetica-Oblique") : c.base.find("Courier") == 0 ? "Courier-Bold" : "Times-Bold"; Exercise(c); } }
TEST(CoverageFontProviderAliasBoundaries, RepeatedBoldAndLateItalicStyleComponentsKeepAUsableExplicitFallback) { for (const auto& pair : {std::pair<const char*, const char*>{"Recovery,Bold,Bold", "Recovery"}, {"Recovery,Bold,Italic", "Recovery,Bold,Italic"}, {"Recovery,Unknown,Bold", "Recovery,Unknown,Bold"}}) { Config c; c.base = pair.first; c.expected_family = pair.second; Exercise(c); } }
TEST(CoverageFontProviderAliasBoundaries, UnknownHyphenStylesPreserveTheWholeOriginalFamilyWhileRegularSuffixesAreRemoved) { for (const auto& pair : {std::pair<const char*, const char*>{"Recovery-Regular", "Recovery"}, {"Recovery-Unavailable", "Recovery-Unavailable"}}) { Config c; c.base = pair.first; c.expected_family = pair.second; Exercise(c); } }
TEST(CoverageFontProviderAliasBoundaries, TinyDescriptorItalicAnglesRequestItalicWithoutSkewingTheOriginalGlyphs) { Config c; c.flags = 96; c.angle = -2; c.expected_italic = true; Exercise(c); }
TEST(CoverageFontProviderAliasBoundaries, NonembeddedCidFontsCanSelectAMacintoshOnlyExternalCharmap) { Config c; c.base = c.expected_family = c.face_name = "RecoveryLegacyMacRoman"; c.cid = true; c.legacy_encoding = 0; Exercise(c); }
TEST(CoverageFontProviderAliasBoundaries, NonembeddedCidFontsCanUseTheFirstLegacyCharmapWhenNoEncodingConversionExists) { Config c; c.base = c.expected_family = c.face_name = "RecoveryLegacyUninterpreted"; c.cid = true; c.legacy_encoding = 32; Exercise(c); }
TEST(CoverageFontProviderAliasBoundaries, LegacyChineseTrueTypeNamesRouteThroughGbkWithoutAnEmbeddedFontFile) { for (const auto* name : {"#CB#CE#CC#E5", "#BF#AC#CC#E5", "#BA#DA#CC#E5", "#B7#C2#CB#CE", "#D0#C2#CB#CE"}) { Config c; c.base = name; c.expected_family.clear(); c.expected_charset = FXFONT_GB2312_CHARSET; c.legacy_chinese = true; Exercise(c); } }
TEST(CoverageFontProviderAliasBoundaries, NameTablesWithoutPostscriptRecordsStillPermitDirectProviderMapping) { auto c = Localized("NoPsName"); c.map_fails = false; c.name_policy = NamePolicy::NoPostscriptName; c.expected_family = c.base; c.localized_alias = false; Exercise(c); }

}  // namespace
}  // namespace pdfoxium::integration
