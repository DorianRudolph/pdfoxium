#include "runtime_fixture.h"

#include <cstring>
#include <limits>
#include <tuple>

namespace pdfoxium::integration {
namespace {

class SystemFonts : public testing::Test {
 protected:
  void TearDown() override {
    for (const auto* library : Implementations())
      library->restore_default_font_info();
  }
};

uint32_t FontU32(const std::vector<uint8_t>& bytes, size_t offset) {
  if (offset + 4 > bytes.size()) return 0;
  return (uint32_t{bytes[offset]} << 24) | (uint32_t{bytes[offset + 1]} << 16) |
         (uint32_t{bytes[offset + 2]} << 8) | bytes[offset + 3];
}

struct RecordingFontInfo : FPDF_SYSFONTINFO {
  RecordingFontInfo(const Api& api, int requested_version, bool fallback,
                    std::vector<uint8_t> data = {})
      : api(api), bytes(data.empty() ? runtime_test::Resource("fonts/ahem/Ahem.ttf") : std::move(data)), fallback(fallback) {
    version = requested_version;
    Release = ReleaseInfo; EnumFonts = Enumerate; MapFont = Map; GetFont = Get;
    GetFontData = Data; GetFaceName = Name; GetFontCharset = Charset; DeleteFont = Delete;
  }
  static RecordingFontInfo* Self(FPDF_SYSFONTINFO* info) { return static_cast<RecordingFontInfo*>(info); }
  static void ReleaseInfo(FPDF_SYSFONTINFO* info) { ++Self(info)->releases; }
  static void Enumerate(FPDF_SYSFONTINFO* info, void* mapper) {
    auto* self = Self(info);
    ++self->enumerations;
    self->valid &= mapper != nullptr;
    if (mapper) {
      // This is the only valid source of the opaque mapper argument.
      self->api.FPDF_AddInstalledFont(mapper, "CallbackFace", FXFONT_ANSI_CHARSET);
      self->api.FPDF_AddInstalledFont(mapper, "Ahem", FXFONT_ANSI_CHARSET);
      self->api.FPDF_AddInstalledFont(mapper, "CallbackSymbol", FXFONT_SYMBOL_CHARSET);
    }
  }
  static void* Map(FPDF_SYSFONTINFO* info, int weight, FPDF_BOOL italic,
                   int charset, int pitch, const char* face, FPDF_BOOL* exact) {
    auto* self = Self(info);
    ++self->maps;
    self->valid &= face != nullptr && weight >= 0 && (italic == 0 || italic == 1);
    try {
      if (face) self->requests.emplace_back(weight, !!italic, charset, pitch, face);
    } catch (...) { self->valid = false; return nullptr; }
    if (exact) *exact = self->exact;
    self->reentry &= self->api.FPDF_GetDefaultTTFMapCount() > 0;
    return self->fallback ? nullptr : &self->token;
  }
  static void* Get(FPDF_SYSFONTINFO* info, const char* face) {
    auto* self = Self(info);
    ++self->gets;
    self->valid &= face != nullptr;
    return &self->token;
  }
  static unsigned long Data(FPDF_SYSFONTINFO* info, void* font, unsigned int table,
                             unsigned char* buffer, unsigned long capacity) {
    auto* self = Self(info);
    ++self->data_calls;
    self->valid &= font == &self->token;
    size_t offset = 0, length = self->bytes.size();
    if (table) {
      offset = length = 0;
      const size_t declared = self->bytes.size() >= 6 ? (size_t{self->bytes[4]} << 8) | self->bytes[5] : 0;
      const size_t tables = std::min(declared, self->bytes.size() >= 12 ? (self->bytes.size() - 12) / 16 : 0);
      for (size_t index = 0; index < tables; ++index) {
        const size_t entry = 12 + index * 16;
        if (FontU32(self->bytes, entry) == table) { offset = FontU32(self->bytes, entry + 8); length = FontU32(self->bytes, entry + 12); break; }
      }
      if (offset > self->bytes.size() || length > self->bytes.size() - offset) return 0;
    }
    if (!buffer || capacity < length) { ++self->queries; return static_cast<unsigned long>(length); }
    std::copy_n(self->bytes.data() + offset, length, buffer);
    ++self->copies;
    return static_cast<unsigned long>(length);
  }
  static unsigned long Name(FPDF_SYSFONTINFO* info, void* font, char* buffer, unsigned long capacity) {
    auto* self = Self(info); self->valid &= font == &self->token; ++self->names;
    if (buffer && capacity >= 5) std::memcpy(buffer, "Ahem", 5);
    return 5;
  }
  static int Charset(FPDF_SYSFONTINFO* info, void* font) { auto* self = Self(info); self->valid &= font == &self->token; return FXFONT_ANSI_CHARSET; }
  static void Delete(FPDF_SYSFONTINFO* info, void* font) { auto* self = Self(info); self->valid &= font == &self->token; ++self->deletes; }
  const Api& api;
  std::vector<uint8_t> bytes;
  bool fallback;
  int exact = 0;
  int token = 123;
  size_t enumerations = 0, maps = 0, gets = 0, data_calls = 0, queries = 0, copies = 0, names = 0, deletes = 0, releases = 0;
  bool valid = true, reentry = true;
  std::vector<std::tuple<int, bool, int, int, std::string>> requests;
};

struct InstalledFontInfo {
  explicit InstalledFontInfo(const Api& api, FPDF_SYSFONTINFO* info) : api(api) { api.FPDF_SetSystemFontInfo(info); }
  ~InstalledFontInfo() { api.FPDF_SetSystemFontInfo(nullptr); }
  const Api& api;
};

std::vector<uint8_t> MissingEmbeddedFont(bool styled = false) {
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << /Font << /F 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", "0 0 0 rg BT /F 16 Tf 8 40 Td (ABCDE) Tj ET\n"),
      "<< /Type /Font /Subtype /TrueType /BaseFont /CallbackFace /FirstChar 32 /LastChar 126 /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R >>",
      styled ? "<< /Type /FontDescriptor /FontName /CallbackFace /Flags 97 /FontWeight 700 /ItalicAngle -12 /Ascent 1000 /Descent 0 /CapHeight 1000 /StemV 140 /FontBBox [0 0 1000 1000] >>"
             : "<< /Type /FontDescriptor /FontName /CallbackFace /Flags 32 /ItalicAngle 0 /Ascent 1000 /Descent 0 /CapHeight 1000 /StemV 80 /FontBBox [0 0 1000 1000] >>"});
}

TEST_F(SystemFonts, StyledMissingFontPassesWeightItalicCharsetPitchAndFamilyToMapper) {
  Artifacts artifacts;
  const auto bytes = MissingEmbeddedFont(true);
  std::array<Raster, 2> images;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    RecordingFontInfo provider(api, 2, false);
    InstalledFontInfo installed(api, &provider);
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, document.get(), &images[backend++]));
    EXPECT_TRUE(std::any_of(provider.requests.begin(), provider.requests.end(), [](const auto& request) {
      const auto& [weight, italic, charset, pitch, family] = request;
      return weight == 700 && italic && charset == FXFONT_ANSI_CHARSET && (pitch & 1) && family == "CallbackFace";
    }));
    EXPECT_TRUE(provider.valid);
  }
  CompareRaster(images[0], images[1], artifacts, "styled-system-font", RasterTolerance{24, 0, 0.2});
}

#if defined(__APPLE__)
TEST_F(SystemFonts, MacDefaultLookupMatchesStyleBase14AndCharsetRules) {
  const auto implementations = Implementations();
  std::array<FPDF_SYSFONTINFO*, 2> providers{};
  struct Cleanup {
    const std::array<const Library*, 2>& implementations;
    std::array<FPDF_SYSFONTINFO*, 2>& providers;
    ~Cleanup() {
      for (size_t i = 0; i < providers.size(); ++i) {
        if (!providers[i]) continue;
        implementations[i]->api().FPDF_SetSystemFontInfo(nullptr);
        implementations[i]->api().FPDF_FreeDefaultSystemFontInfo(providers[i]);
      }
    }
  } cleanup{implementations, providers};
  for (size_t i = 0; i < providers.size(); ++i) {
    const auto& api = implementations[i]->api();
    providers[i] = api.FPDF_GetDefaultSystemFontInfo();
    ASSERT_NE(providers[i], nullptr);
    // Installation enumerates the actual font folder through this provider.
    api.FPDF_SetSystemFontInfo(providers[i]);
  }
  struct Request {
    const char* family; int weight; bool italic; int charset; int pitch;
    const char* expected;
  };
  for (const Request request : {
      Request{"Helvetica", 700, true, FXFONT_ANSI_CHARSET, 0, "Arial"},
      {"Helvetica-Bold", 400, false, FXFONT_ANSI_CHARSET, 0, "Arial Bold"},
      {"Helvetica-Oblique", 400, false, FXFONT_ANSI_CHARSET, 0, "Arial Italic"},
      {"Helvetica-BoldOblique", 400, false, FXFONT_ANSI_CHARSET, 0, "Arial Bold Italic"},
      {"Courier", 700, true, FXFONT_ANSI_CHARSET, 0, "Courier New"},
      {"Times-Roman", 700, true, FXFONT_ANSI_CHARSET, 0, "Times New Roman"},
      {"Arial", 400, false, FXFONT_ANSI_CHARSET, 0, "Arial"},
      {"Arial", 401, false, FXFONT_ANSI_CHARSET, 0, "Arial Bold"},
      {"Arial", 400, true, FXFONT_ANSI_CHARSET, 0, "Arial Italic"},
      {"Arial", 700, true, FXFONT_ANSI_CHARSET, 0, "Arial Bold Italic"},
      {"Arial Bold", 700, true, FXFONT_ANSI_CHARSET, 0, "Arial Bold"},
      {"Pdfoxium Missing Family", 400, false, FXFONT_ANSI_CHARSET, 0, nullptr},
      {"Pdfoxium Missing Family", 400, false, FXFONT_SYMBOL_CHARSET, FXFONT_FF_FIXEDPITCH, nullptr},
      {"Pdfoxium Missing Family", 400, false, FXFONT_ANSI_CHARSET, FXFONT_FF_FIXEDPITCH, "Courier New"},
      {"Pdfoxium Gothic", 400, false, FXFONT_SHIFTJIS_CHARSET, FXFONT_FF_ROMAN, "Hiragino Kaku Gothic Pro W6"},
      {"Pdfoxium Missing Family", 400, false, FXFONT_SHIFTJIS_CHARSET, 0, "Hiragino Mincho Pro W6"},
      {"Pdfoxium Missing Family", 700, false, FXFONT_SHIFTJIS_CHARSET, 0, "Hiragino Kaku Gothic Pro W6"},
      {"Pdfoxium Missing Family", 700, false, FXFONT_SHIFTJIS_CHARSET, FXFONT_FF_ROMAN, "Hiragino Mincho Pro W6"},
      {"Pdfoxium Missing Family", 400, false, FXFONT_GB2312_CHARSET, 0, "STSong"},
      {"Pdfoxium Missing Family", 400, false, FXFONT_HANGEUL_CHARSET, 0, "AppleMyungjo"},
      {"Pdfoxium Missing Family", 400, false, FXFONT_CHINESEBIG5_CHARSET, 0, "LiSong Pro Light"},
  }) {
    SCOPED_TRACE(std::string(request.family) + " weight=" + std::to_string(request.weight) +
                 " charset=" + std::to_string(request.charset));
    std::array<bool, 2> found{};
    std::array<std::vector<uint8_t>, 2> font_bytes;
    for (size_t i = 0; i < providers.size(); ++i) {
      SCOPED_TRACE(implementations[i]->name());
      auto* provider = providers[i];
      FPDF_BOOL exact = 0;
      void* font = provider->MapFont(provider, request.weight, request.italic,
          request.charset, request.pitch, request.family, &exact);
      void* expected = request.expected ? provider->GetFont(provider, request.expected) : nullptr;
      EXPECT_EQ(font, expected);
      found[i] = font != nullptr;
      if (expected) provider->DeleteFont(provider, expected);
      if (!font) continue; // The expected platform family may be absent.
      unsigned int table = 0;
      auto size = provider->GetFontData(provider, font, table, nullptr, 0);
      if (!size) {
        // Collection faces expose the whole file as TTCF, not table zero.
        // Compare their face-specific name table rather than reading the
        // same potentially large collection for each CJK request.
        table = 0x6e616d65u;
        size = provider->GetFontData(provider, font, table, nullptr, 0);
      }
      ASSERT_GT(size, 0u);
      ASSERT_LT(size, 32u * 1024 * 1024);
      font_bytes[i].resize(size);
      EXPECT_EQ(provider->GetFontData(provider, font, table, font_bytes[i].data(), size), size);
      provider->DeleteFont(provider, font);
    }
    EXPECT_EQ(found[0], found[1]);
    EXPECT_EQ(font_bytes[0], font_bytes[1]);
  }
}
#endif

TEST_F(SystemFonts, LegacyMapSentinelCountAndEveryIndexedEntryAgree) {
  std::array<std::vector<std::pair<int, std::string>>, 2> observations;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    const auto* legacy = api.FPDF_GetDefaultTTFMap();
    ASSERT_NE(legacy, nullptr);
    const size_t count = api.FPDF_GetDefaultTTFMapCount();
    ASSERT_GT(count, 0u);
    ASSERT_LT(count, 100u);
    for (size_t index = 0; index < count; ++index) {
      ASSERT_NE(legacy[index].fontname, nullptr);
      ASSERT_NE(legacy[index].charset, -1);
      const auto* entry = api.FPDF_GetDefaultTTFMapEntry(index);
      ASSERT_NE(entry, nullptr);
      EXPECT_EQ(entry->charset, legacy[index].charset);
      EXPECT_STREQ(entry->fontname, legacy[index].fontname);
      observations[backend].emplace_back(entry->charset, entry->fontname);
    }
    EXPECT_EQ(legacy[count].charset, -1);
    EXPECT_EQ(legacy[count].fontname, nullptr);
    EXPECT_EQ(api.FPDF_GetDefaultTTFMapEntry(count), nullptr);
    EXPECT_EQ(api.FPDF_GetDefaultTTFMapEntry(std::numeric_limits<size_t>::max()), nullptr);
    ++backend;
  }
  EXPECT_EQ(observations[0], observations[1]);
}

TEST_F(SystemFonts, VersionedProvidersEnumerateMapSupplyRealFontAndReleaseAfterUse) {
  Artifacts artifacts;
  const auto bytes = MissingEmbeddedFont();
  std::array<std::vector<Raster>, 2> observations;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    SCOPED_TRACE(library->name());
    for (int version : {1, 2}) {
      for (bool fallback : {false, true}) {
        // Version 2 explicitly relies on MapFont per request and skips the
        // enumeration needed for named GetFont lookup. Exercise GetFont and
        // NULL MapFont fallback with the enumerating version-1 provider.
        if (version == 2 && fallback) continue;
        RecordingFontInfo provider(api, version, fallback);
        {
          InstalledFontInfo installed(api, &provider);
          for (int exact : {0, 1}) {
            provider.exact = exact;
            Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
            ASSERT_NE(document.get(), nullptr);
            Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
            ASSERT_NE(page.get(), nullptr);
            const auto image = Render(api, page.get(), 128, 96, FPDF_RENDER_NO_SMOOTHTEXT);
            EXPECT_TRUE(std::any_of(image.pixels.begin(), image.pixels.end(), [](uint8_t pixel) { return pixel == 0; }));
            observations[backend].push_back(image);
          }
          EXPECT_GT(provider.maps + provider.gets, 0u);
          EXPECT_GT(provider.data_calls, 0u);
          EXPECT_GT(provider.queries, 0u);
          EXPECT_GT(provider.copies, 0u);
          if (version == 1) { EXPECT_GT(provider.enumerations, 0u); }
          else { EXPECT_EQ(provider.enumerations, 0u); }
          EXPECT_TRUE(provider.valid);
          EXPECT_TRUE(provider.reentry);
          EXPECT_EQ(provider.releases, 0u);
        }
        EXPECT_EQ(provider.releases, 1u);
        EXPECT_GT(provider.deletes, 0u);
        CompareRaster(observations[backend][observations[backend].size() - 2], observations[backend].back(), artifacts,
                       library->name() + "-obsolete-exact-v" + std::to_string(version) + (fallback ? "-fallback" : "-map"));
      }
    }
    ++backend;
  }
  ASSERT_EQ(observations[0].size(), observations[1].size());
  for (size_t index = 0; index < observations[0].size(); ++index) CompareRaster(observations[0][index], observations[1][index], artifacts, "custom-font" + std::to_string(index));
}

TEST_F(SystemFonts, DefaultProvidersReadRealFileAndTablesBeforeUninstallAndFree) {
  Artifacts artifacts;
  std::array<Raster, 2> images;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    SCOPED_TRACE(library->name());
    FPDF_SYSFONTINFO* provider = api.FPDF_GetDefaultSystemFontInfo();
    const size_t current = backend++;
    if (!provider) continue;  // Documented absence on some native platforms.
    struct Owner {
      const Api& api; FPDF_SYSFONTINFO* provider; bool installed = false;
      ~Owner() { if (installed) api.FPDF_SetSystemFontInfo(nullptr); else if (provider->Release) provider->Release(provider); api.FPDF_FreeDefaultSystemFontInfo(provider); }
    } owner{api, provider};
    ASSERT_NE(provider->MapFont, nullptr);
    ASSERT_NE(provider->GetFontData, nullptr);
    ASSERT_NE(provider->DeleteFont, nullptr);
    api.FPDF_SetSystemFontInfo(provider); owner.installed = true;
    FPDF_BOOL exact = 0;
    void* font = provider->MapFont(provider, 400, 0, FXFONT_ANSI_CHARSET, 0, "DejaVu Sans", &exact);
    if (!font && provider->GetFont) font = provider->GetFont(provider, "DejaVu Sans");
    if (!font) continue; // A real platform provider need not have this family.
    struct FontOwner { FPDF_SYSFONTINFO* p; void* font; ~FontOwner() { p->DeleteFont(p, font); } } font_owner{provider, font};
    for (unsigned int table : {0u, 0x68656164u}) {
      const auto length = provider->GetFontData(provider, font, table, nullptr, 0);
      ASSERT_GT(length, 0u);
      ASSERT_LT(length, 32u * 1024 * 1024);
      for (unsigned long capacity : {length - 1, length, length + 3}) {
        std::vector<unsigned char> buffer(capacity + 2, 0xa5);
        const auto result = provider->GetFontData(provider, font, table, buffer.data() + 1, capacity);
        EXPECT_EQ(buffer.front(), 0xa5); EXPECT_EQ(buffer.back(), 0xa5);
        if (capacity >= length) { EXPECT_EQ(result, length); }
        // Platform providers differ in whether insufficient size returns zero
        // or the required size; neither may write outside the supplied buffer.
        else { EXPECT_TRUE(result == 0 || result == length); }
      }
    }
    if (provider->GetFaceName) {
      const auto length = provider->GetFaceName(provider, font, nullptr, 0);
      ASSERT_GT(length, 0u);
      std::vector<char> name(length + 1, '#');
      EXPECT_EQ(provider->GetFaceName(provider, font, name.data(), length), length);
      EXPECT_EQ(name[length], '#');
    }
    if (provider->GetFontCharset) { EXPECT_GE(provider->GetFontCharset(provider, font), 0); }
    auto bytes = MissingEmbeddedFont();
    // Request the real family mapped above, rather than letting two platform
    // font inventories choose unrelated fallbacks for the synthetic name.
    std::string text(bytes.begin(), bytes.end());
    for (size_t position = 0; (position = text.find("CallbackFace", position)) != std::string::npos;) {
      text.replace(position, 12, "DejaVuSans  ");
      position += 12;
    }
    bytes.assign(text.begin(), text.end());
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, document.get(), &images[current]));
    EXPECT_TRUE(std::any_of(images[current].pixels.begin(), images[current].pixels.end(), [](uint8_t value) { return value < 128; }));
  }
  // Platform font providers may resolve a family to different installed faces
  // (for example condensed versus regular DejaVu Sans). Their selections are
  // deliberately platform dependent. Deterministic provider bytes are compared
  // across engines in the custom-provider cases above and below instead.
}

TEST_F(SystemFonts, EnumeratedTrueTypeOpenTypeAndType1FacesSupplyActualRenderedGlyphs) {
  Artifacts artifacts;
  const auto bytes = MissingEmbeddedFont();
  std::ifstream otf(TestRoot() / "upstream/pdfium/third_party/NotoSansCJK/NotoSansSC-Regular.subset.otf", std::ios::binary);
  ASSERT_TRUE(otf.good());
  const std::array<std::vector<uint8_t>, 3> fonts = {
      runtime_test::Resource("fonts/ahem/Ahem.ttf"),
      std::vector<uint8_t>(std::istreambuf_iterator<char>(otf), std::istreambuf_iterator<char>()),
      runtime_test::Resource("fonts/foxit_fixed.cff")};
  std::array<std::array<Raster, 3>, 2> observations;
  for (size_t kind = 0; kind < fonts.size(); ++kind) {
    std::array<Raster, 2> images;
    size_t backend = 0;
    for (const auto* library : Implementations()) {
      const auto& api = library->api();
      SCOPED_TRACE(library->name() + "-font-kind" + std::to_string(kind));
      RecordingFontInfo provider(api, 1, false, fonts[kind]);
      {
        InstalledFontInfo installed(api, &provider);
        Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
        ASSERT_NE(document.get(), nullptr);
        ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, document.get(), &images[backend++]));
        EXPECT_GT(provider.enumerations, 0u);
        EXPECT_GT(provider.maps, 0u);
        EXPECT_GT(provider.copies, 0u);
        EXPECT_TRUE(provider.valid);
      }
      EXPECT_EQ(provider.releases, 1u);
    }
    // The exact Ahem square font has no edge ambiguity. The other real faces
    // use the same explicitly bounded FreeType/AGG antialiasing allowance.
    CompareRaster(images[0], images[1], artifacts, "enumerated-face" + std::to_string(kind),
                   kind ? RasterTolerance{24, 0, 0.2} : RasterTolerance{0, 0, 0});
    for (size_t backend = 0; backend < 2; ++backend) observations[backend][kind] = images[backend];
  }
  for (const auto& backend : observations) {
    EXPECT_NE(backend[0].pixels, backend[1].pixels);
    EXPECT_NE(backend[0].pixels, backend[2].pixels);
  }
}

TEST_F(SystemFonts, ReplacingProvidersReleasesOldOwnerAndOptionalCallbacksCanBeAbsent) {
  Artifacts artifacts;
  const auto bytes = MissingEmbeddedFont();
  std::array<Raster, 2> images;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    RecordingFontInfo first(api, 1, false), second(api, 2, false);
    second.GetFaceName = nullptr;
    second.GetFontCharset = nullptr;
    second.GetFont = nullptr;
    second.EnumFonts = nullptr;
    second.Release = nullptr;
    {
      InstalledFontInfo installed(api, &first);
      {
        Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
        ASSERT_NE(document.get(), nullptr);
        Raster first_image;
        ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, document.get(), &first_image));
      }
      EXPECT_EQ(first.releases, 0u);
      api.FPDF_SetSystemFontInfo(&second);
      EXPECT_EQ(first.releases, 1u);
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, document.get(), &images[backend++]));
      EXPECT_GT(second.maps, 0u);
      EXPECT_GT(second.copies, 0u);
      EXPECT_TRUE(second.valid);
      EXPECT_TRUE(second.reentry);
    }
    EXPECT_EQ(first.releases, 1u);
    EXPECT_EQ(second.releases, 0u); // Release is optional, not silently required.
    EXPECT_GT(second.deletes, 0u);
  }
  CompareRaster(images[0], images[1], artifacts, "provider-replacement");
}

}  // namespace
}  // namespace pdfoxium::integration
