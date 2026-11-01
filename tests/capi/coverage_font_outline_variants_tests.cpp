#include "text_fixture.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstring>
#include <memory>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;

const Bytes& RegularAhem() {
  static const Bytes bytes = text_fixture::Resource("fonts/ahem/Ahem.ttf");
  return bytes;
}
struct OutlineProvider : FPDF_SYSFONTINFO {
  explicit OutlineProvider(const Api& owner) : api(owner) {
    version = 2;
    Release = Drop;
    EnumFonts = Enumerate;
    MapFont = Map;
    GetFont = Get;
    GetFontData = Data;
    GetFaceName = Face;
    GetFontCharset = Charset;
    DeleteFont = Delete;
  }
  static OutlineProvider& Self(FPDF_SYSFONTINFO* info) {
    return *static_cast<OutlineProvider*>(info);
  }
  static bool Owned(const char* family) {
    return family && std::strncmp(family, "OutlineVariant", 14) == 0;
  }
  static void Drop(FPDF_SYSFONTINFO* info) { ++Self(info).releases; }
  static void Enumerate(FPDF_SYSFONTINFO* info, void* mapper) {
    auto& self = Self(info);
    self.valid &= mapper != nullptr;
    if (mapper) self.api.FPDF_AddInstalledFont(mapper, "OutlineVariant", FXFONT_ANSI_CHARSET);
  }
  static void* Map(FPDF_SYSFONTINFO* info, int weight, FPDF_BOOL italic,
                   int, int, const char* family, FPDF_BOOL* exact) {
    auto& self = Self(info);
    if (exact) *exact = Owned(family);
    if (!Owned(family)) return nullptr;
    ++self.maps;
    self.bold_request |= weight >= 700;
    self.italic_request |= !!italic;
    return &self.token;
  }
  static void* Get(FPDF_SYSFONTINFO* info, const char* family) {
    return Owned(family) ? &Self(info).token : nullptr;
  }
  static unsigned long Data(FPDF_SYSFONTINFO* info, void* font, unsigned int table,
                            unsigned char* buffer, unsigned long capacity) {
    auto& self = Self(info);
    self.valid &= font == &self.token;
    const auto& bytes = RegularAhem();
    size_t offset = 0, length = bytes.size();
    auto word = [&](size_t at) { return (unsigned(bytes.at(at)) << 8) | bytes.at(at + 1); };
    auto integer = [&](size_t at) { return (uint32_t(word(at)) << 16) | word(at + 2); };
    if (table) {
      bool found = false;
      for (unsigned entry = 0; entry < word(4); ++entry) {
        const size_t record = 12 + entry * 16;
        if (integer(record) != table) continue;
        offset = integer(record + 8);
        length = integer(record + 12);
        found = true;
        break;
      }
      if (!found) return 0;
    }
    if (buffer && capacity >= length) std::copy_n(bytes.data() + offset, length, buffer);
    ++self.data_queries;
    return length;
  }
  static unsigned long Face(FPDF_SYSFONTINFO* info, void* font, char* buffer, unsigned long capacity) {
    auto& self = Self(info);
    self.valid &= font == &self.token;
    constexpr char name[] = "Ahem";
    if (buffer && capacity >= sizeof(name)) std::memcpy(buffer, name, sizeof(name));
    return sizeof(name);
  }
  static int Charset(FPDF_SYSFONTINFO*, void*) { return FXFONT_ANSI_CHARSET; }
  static void Delete(FPDF_SYSFONTINFO* info, void* font) {
    auto& self = Self(info);
    self.valid &= font == &self.token;
    ++self.deletions;
  }
  const Api& api;
  int token = 0;
  int maps = 0, data_queries = 0, deletions = 0, releases = 0;
  bool bold_request = false, italic_request = false, valid = true;
};
struct ProviderEnvironment {
  ProviderEnvironment() {
    size_t index = 0;
    for (const auto* library : Implementations()) {
      providers[index] = std::make_unique<OutlineProvider>(library->api());
      library->api().FPDF_SetSystemFontInfo(providers[index++].get());
    }
  }
  ~ProviderEnvironment() {
    for (const auto* library : Implementations()) library->restore_default_font_info();
  }
  std::array<std::unique_ptr<OutlineProvider>, 2> providers;
};
struct Variant {
  bool bold = false;
  bool italic = false;
  bool vertical = false;
  int angle = 0;
  int skew = 0;
};
Bytes OutlinePdf(const Variant& variant) {
  const std::string name = std::string("OutlineVariant") +
      (variant.bold ? (variant.italic ? ",BoldItalic" : ",Bold") : (variant.italic ? ",Italic" : ""));
  std::string widths;
  for (int code = 0; code < 256; ++code) widths += code == 'B' ? "900 " : "1000 ";
  const std::string font = variant.vertical ?
      "<< /Type /Font /Subtype /Type0 /BaseFont /" + name +
          " /Encoding /Identity-V /DescendantFonts [8 0 R] /ToUnicode 7 0 R >>" :
      "<< /Type /Font /Subtype /TrueType /BaseFont /" + name + " /FirstChar 0 /LastChar 255 /Widths [" + widths +
          "] /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R /ToUnicode 7 0 R >>";
  const std::string unicode = std::string("/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /OutlineUnicode def /CMapType 2 def 1 begincodespacerange ") +
      (variant.vertical ? "<0000> <ffff>" : "<00> <ff>") + " endcodespacerange 1 beginbfchar " +
      (variant.vertical ? "<0041>" : "<41>") + " <0041> endbfchar endcmap CMapName currentdict /CMap defineresource pop end end";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 160 120] /Resources << /Font << /F 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", std::string("BT /F 40 Tf 60 60 Td ") + (variant.vertical ? "<0041>" : "(A)") + " Tj ET"),
      font,
      "<< /Type /FontDescriptor /FontName /" + name + " /Flags " + std::to_string(variant.italic || variant.angle ? 96 : 32) +
          " /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /CapHeight 800 /StemV 80 /FontWeight " +
          std::to_string(variant.bold ? 700 : 400) + " /ItalicAngle " + std::to_string(variant.angle) + " >>",
      PdfStream("", unicode),
      "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /" + name +
          " /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 6 0 R /DW 1000 /DW2 [880 -1000] >>"});
}
struct Bounds {
  float left = 1e9f, bottom = 1e9f, right = -1e9f, top = -1e9f;
};
Bounds PathBounds(const Api& api, FPDF_FONT font, float size) {
  Bounds bounds;
  const auto path = api.FPDFFont_GetGlyphPath(font, 'A', size);
  EXPECT_NE(path, nullptr);
  if (!path) return bounds;
  const int count = api.FPDFGlyphPath_CountGlyphSegments(path);
  EXPECT_GE(count, 4);
  for (int index = 0; index < count; ++index) {
    const auto segment = api.FPDFGlyphPath_GetGlyphPathSegment(path, index);
    EXPECT_NE(segment, nullptr);
    float x = 0, y = 0;
    EXPECT_TRUE(api.FPDFPathSegment_GetPoint(segment, &x, &y));
    EXPECT_TRUE(std::isfinite(x));
    EXPECT_TRUE(std::isfinite(y));
    bounds.left = std::min(bounds.left, x);
    bounds.right = std::max(bounds.right, x);
    bounds.bottom = std::min(bounds.bottom, y);
    bounds.top = std::max(bounds.top, y);
    EXPECT_NE(api.FPDFPathSegment_GetType(segment), FPDF_SEGMENT_BEZIERTO);
  }
  return bounds;
}
void InspectOutline(const Api& api, FPDF_PAGE page, const Variant& variant) {
  ASSERT_EQ(api.FPDFPage_CountObjects(page), 1);
  const auto object = api.FPDFPage_GetObject(page, 0);
  ASSERT_EQ(api.FPDFPageObj_GetType(object), FPDF_PAGEOBJ_TEXT);
  const auto font = api.FPDFTextObj_GetFont(object);
  ASSERT_NE(font, nullptr);
  EXPECT_EQ(api.FPDFFont_GetIsEmbedded(font), 0);
  Handle text(api.FPDFText_LoadPage(page), api.FPDFText_ClosePage);
  ASSERT_NE(text.get(), nullptr);
  EXPECT_EQ(text_fixture::Extract(api, text.get()), u"A");
  const auto bounds = PathBounds(api, font, 40);
  // Ahem A is the original rectangle [0,-.2,1,.8]. Source synthetic skew
  // shears x by -skew*y/100 horizontally or y by skew*x/100 vertically.
  std::array<std::array<float, 2>, 4> corners{{{0,-.2f},{1,-.2f},{1,.8f},{0,.8f}}};
  Bounds expected;
  for (auto point : corners) {
    if (variant.vertical) point[1] += variant.skew * point[0] / 100.0f;
    else point[0] -= variant.skew * point[1] / 100.0f;
    expected.left = std::min(expected.left, point[0]);
    expected.right = std::max(expected.right, point[0]);
    expected.bottom = std::min(expected.bottom, point[1]);
    expected.top = std::max(expected.top, point[1]);
  }
  const float tolerance = variant.bold ? .04f : .001f;
  EXPECT_NEAR(bounds.left, expected.left, tolerance);
  EXPECT_NEAR(bounds.right, expected.right, tolerance);
  EXPECT_NEAR(bounds.bottom, expected.bottom, tolerance);
  EXPECT_NEAR(bounds.top, expected.top, tolerance);
  if (variant.bold) {
    EXPECT_GT(bounds.right - bounds.left, expected.right - expected.left);
    EXPECT_GT(bounds.top - bounds.bottom, expected.top - expected.bottom);
  }
  for (float size : {.5f, 12.0f, 400.0f}) {
    const auto other = PathBounds(api, font, size);
    EXPECT_NEAR(other.left, bounds.left, .0001);
    EXPECT_NEAR(other.right, bounds.right, .0001);
    EXPECT_NEAR(other.bottom, bounds.bottom, .0001);
    EXPECT_NEAR(other.top, bounds.top, .0001);
  }
  float width = 0;
  ASSERT_TRUE(api.FPDFFont_GetGlyphWidth(font, 'A', 40, &width));
  // Identity-V exposes the signed PDF DW2 advance (-1000), not its magnitude.
  EXPECT_NEAR(width, variant.vertical ? -40 : 40, .001);
}
void OutlineCase(const Variant& variant) {
  ProviderEnvironment environment;
  const auto bytes = OutlinePdf(variant);
  Artifacts artifacts;
  std::array<Raster, 2> originals;
  size_t index = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    const auto& api = writer->api();
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    {
      Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(InspectOutline(api, page.get(), variant));
      originals[index] = Render(api, page.get(), 160, 120);
    }
    const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-synthetic-outline");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      Handle page(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(InspectOutline(read, page.get(), variant));
      CompareRaster(originals[index], Render(read, page.get(), 160, 120), artifacts,
          writer->name() + "-" + reader->name(), RasterTolerance::ConnectedError(1));
    }
    ++index;
  }
  CompareRaster(originals[0], originals[1], artifacts, "synthetic-outline-engines", RasterTolerance::ConnectedError(1));
  for (const auto& provider : environment.providers) {
    EXPECT_TRUE(provider->valid);
    EXPECT_GT(provider->maps, 0);
    EXPECT_GT(provider->data_queries, 0);
    if (variant.bold) { EXPECT_TRUE(provider->bold_request); }
    if (variant.italic || variant.angle) { EXPECT_TRUE(provider->italic_request); }
  }
}

TEST(CoverageFontOutlineVariants, RegularHorizontal) {
  OutlineCase(Variant{false, false, false, 0, 0});
}

TEST(CoverageFontOutlineVariants, SyntheticBoldHorizontal) {
  OutlineCase(Variant{true, false, false, 0, 0});
}

TEST(CoverageFontOutlineVariants, SyntheticItalicHorizontal) {
  OutlineCase(Variant{false, true, false, 0, -21});
}

TEST(CoverageFontOutlineVariants, SyntheticBoldItalicHorizontal) {
  OutlineCase(Variant{true, true, false, 0, -21});
}

TEST(CoverageFontOutlineVariants, DescriptorFiveDegreeHorizontal) {
  OutlineCase(Variant{false, true, false, -5, -9});
}

TEST(CoverageFontOutlineVariants, DescriptorTwentyDegreeHorizontal) {
  OutlineCase(Variant{false, true, false, -20, -36});
}

TEST(CoverageFontOutlineVariants, DescriptorTwentyNineDegreeHorizontal) {
  OutlineCase(Variant{false, true, false, -29, -55});
}

TEST(CoverageFontOutlineVariants, RegularVertical) {
  OutlineCase(Variant{false, false, true, 0, 0});
}

TEST(CoverageFontOutlineVariants, SyntheticBoldVertical) {
  OutlineCase(Variant{true, false, true, 0, 0});
}

TEST(CoverageFontOutlineVariants, SyntheticItalicVertical) {
  OutlineCase(Variant{false, true, true, 0, -21});
}

TEST(CoverageFontOutlineVariants, SyntheticBoldItalicVertical) {
  OutlineCase(Variant{true, true, true, 0, -21});
}

TEST(CoverageFontOutlineVariants, DescriptorTwentyDegreeVertical) {
  OutlineCase(Variant{false, true, true, -20, -36});
}

TEST(CoverageFontOutlineVariants, TinyAngleKeepsRegularHorizontalGeometry) {
  OutlineCase(Variant{false, true, false, -2, 0});
}

TEST(CoverageFontOutlineVariants, LargeAngleUsesTheBoundedSkewFallback) {
  OutlineCase(Variant{false, true, false, -45, -58});
}

}  // namespace
}  // namespace pdfoxium::integration
