#include "runtime_fixture.h"

#include <cstring>

namespace pdfoxium::integration {
namespace {

TEST(SaveSecurity, RemoveSecurityFlagsAndExplicitVersionAllowPasswordlessReaders) {
  Artifacts artifacts;
  for (int revision : {2, 3, 5, 6}) {
    const auto input = runtime_test::Resource("encrypted_hello_world_r" + std::to_string(revision) + ".pdf");
    for (const auto* writer : Implementations()) {
      const auto& api = writer->api();
      for (FPDF_DWORD flags : {FPDF_DWORD{FPDF_REMOVE_SECURITY_DEPRECATED}, FPDF_DWORD{FPDF_REMOVE_SECURITY},
                               FPDF_DWORD{FPDF_REMOVE_SECURITY | FPDF_NO_INCREMENTAL}, FPDF_DWORD{FPDF_REMOVE_SECURITY | FPDF_INCREMENTAL}}) {
        for (int version : {0, 15}) {
          const auto label = writer->name() + "-security-r" + std::to_string(revision) + "-flags" + std::to_string(flags) + "-version" + std::to_string(version);
          SCOPED_TRACE(label);
          Handle document(api.FPDF_LoadMemDocument64(input.data(), input.size(), "\xc3\xa2ge"), api.FPDF_CloseDocument);
          ASSERT_NE(document.get(), nullptr);
          Raster before;
          ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, document.get(), &before));
          const auto output = SaveDocument(api, document.get(), artifacts, label, flags, version);
          ASSERT_FALSE(output.empty());
          std::array<Raster, 2> images;
          size_t reader_index = 0;
          for (const auto* reader : Implementations()) {
            const auto& read = reader->api();
            for (const char* password : {static_cast<const char*>(nullptr), ""}) {
              Handle reopened(read.FPDF_LoadMemDocument64(output.data(), output.size(), password), read.FPDF_CloseDocument);
              ASSERT_NE(reopened.get(), nullptr) << reader->name() << " error=" << read.FPDF_GetLastError();
              EXPECT_EQ(read.FPDF_GetSecurityHandlerRevision(reopened.get()), -1);
              EXPECT_EQ(read.FPDF_GetDocPermissions(reopened.get()), 0xffffffffu);
              EXPECT_EQ(read.FPDF_GetDocUserPermissions(reopened.get()), 0xffffffffu);
              EXPECT_EQ(read.FPDF_GetPageCount(reopened.get()), api.FPDF_GetPageCount(document.get()));
              int actual_version = 0;
              ASSERT_TRUE(read.FPDF_GetFileVersion(reopened.get(), &actual_version));
              if (version) { EXPECT_EQ(actual_version, version); }
              ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(read, reopened.get(), &images[reader_index]));
              // The same backend must preserve its raster exactly; native
              // FreeType versus Rust AGG differs only in faint glyph edges.
              CompareRaster(before, images[reader_index], artifacts, label + "-to-" + reader->name(),
                  writer->name() == reader->name() ? RasterTolerance{0, 0, 0}
                                                   : RasterTolerance{24, 0, 0.1});
            }
            ++reader_index;
          }
          CompareRaster(images[0], images[1], artifacts, label + "-readers", RasterTolerance{24, 0, 0.1});
        }
      }
    }
  }
}

std::vector<uint8_t> EmbeddedFontBytes(bool cff) {
  if (!cff) return runtime_test::Resource("fonts/ahem/Ahem.ttf");
  const auto path = TestRoot() / "upstream/pdfium/third_party/NotoSansCJK/NotoSansSC-Regular.subset.otf";
  std::ifstream file(path, std::ios::binary);
  if (!file) throw std::runtime_error("Missing published CFF fixture " + path.u8string());
  return {std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>()};
}

std::vector<uint8_t> ReadFontData(const Api& api, FPDF_FONT font) {
  size_t length = 0;
  EXPECT_TRUE(api.FPDFFont_GetFontData(font, nullptr, 0, &length));
  EXPECT_GT(length, 0u);
  std::vector<uint8_t> data(length);
  EXPECT_TRUE(api.FPDFFont_GetFontData(font, data.data(), data.size(), &length));
  EXPECT_EQ(length, data.size());
  return data;
}

void InspectEmbeddedText(const Api& api, FPDF_DOCUMENT document,
                          const std::array<unsigned short, 4>& expected,
                          Raster* raster, std::vector<uint8_t>* font_data) {
  Handle page(api.FPDF_LoadPage(document, 0), api.FPDF_ClosePage);
  ASSERT_NE(page.get(), nullptr);
  ASSERT_EQ(api.FPDFPage_CountObjects(page.get()), 1);
  const auto text = api.FPDFPage_GetObject(page.get(), 0);
  ASSERT_NE(text, nullptr);
  EXPECT_EQ(api.FPDFPageObj_GetType(text), FPDF_PAGEOBJ_TEXT);
  const auto font = api.FPDFTextObj_GetFont(text);
  ASSERT_NE(font, nullptr);
  EXPECT_EQ(api.FPDFFont_GetIsEmbedded(font), 1);
  *font_data = ReadFontData(api, font);
  Handle text_page(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
  ASSERT_NE(text_page.get(), nullptr);
  ASSERT_EQ(api.FPDFText_CountChars(text_page.get()), 3);
  for (int index = 0; index < 3; ++index) EXPECT_EQ(api.FPDFText_GetUnicode(text_page.get(), index), expected[index]);
  std::array<unsigned short, 8> extracted{};
  EXPECT_EQ(api.FPDFText_GetText(text_page.get(), 0, 3, extracted.data()), 4);
  EXPECT_TRUE(std::equal(expected.begin(), expected.end(), extracted.begin()));
  *raster = Render(api, page.get(), 128, 96, expected[0] < 128 ? FPDF_RENDER_NO_SMOOTHTEXT : 0);
  EXPECT_TRUE(std::any_of(raster->pixels.begin(), raster->pixels.end(), [](uint8_t value) { return value == 0; }));
}

void CompareEmbeddedRaster(const Raster& expected, const Raster& actual,
                           bool cff_across_engines, Artifacts& artifacts,
                           const std::string& name) {
  if (!cff_across_engines) {
    CompareRaster(expected, actual, artifacts, name);
    return;
  }
  // The same CFF outlines have matching geometry; native FreeType and Rust
  // AGG differ in edge coverage (observed peak 41, whole-page mean .351).
  // Compare the affected glyph region with a hard per-channel bound, and
  // independently retain the known ink bounds and substantial glyph ink.
  for (const auto* image : {&expected, &actual}) {
    int left = 128, top = 96, right = -1, bottom = -1, ink = 0;
    for (int y = 0; y < image->height; ++y) {
      for (int x = 0; x < image->width; ++x) {
        if (image->pixels[(y * image->width + x) * 4] >= 128) continue;
        left = std::min(left, x); top = std::min(top, y);
        right = std::max(right, x); bottom = std::max(bottom, y); ++ink;
      }
    }
    EXPECT_EQ(left, 13); EXPECT_EQ(top, 39);
    EXPECT_EQ(right, 70); EXPECT_EQ(bottom, 56);
    EXPECT_GE(ink, 240); EXPECT_LE(ink, 280);
  }
  CompareRaster(CropRaster(expected, 8, 32, 72, 32),
                CropRaster(actual, 8, 32, 72, 32), artifacts, name,
                RasterTolerance{42, 0, 2.0});
}

TEST(SaveFonts, SubsetFlagsKeepUnicodeGlyphAppearanceAndCffOriginalFallback) {
  Artifacts artifacts;
  for (bool cff : {false, true}) {
    const auto font_bytes = EmbeddedFontBytes(cff);
    const std::array<unsigned short, 4> text = cff ? std::array<unsigned short, 4>{0x8fd9, 0x4e00, 0x8fd9, 0}
                                                  : std::array<unsigned short, 4>{'A', 'Z', 'A', 0};
    for (const auto* writer : Implementations()) {
      const auto& api = writer->api();
      for (bool cid : {false, true}) {
        if (cff && !cid) continue; // The fixture is an OpenType CID-keyed CFF.
        for (FPDF_DWORD flags : {FPDF_DWORD{FPDF_SUBSET_NEW_FONTS}, FPDF_DWORD{FPDF_SUBSET_NEW_FONTS | FPDF_INCREMENTAL},
                                 FPDF_DWORD{FPDF_SUBSET_NEW_FONTS | FPDF_NO_INCREMENTAL}}) {
          for (int version : {0, 15}) {
            const auto label = writer->name() + (cff ? "-cff" : "-ttf") + (cid ? "-cid" : "-simple") + "-flags" + std::to_string(flags) + "-version" + std::to_string(version);
            SCOPED_TRACE(label);
            Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
            ASSERT_NE(document.get(), nullptr);
            Handle page(api.FPDFPage_New(document.get(), 0, 128, 96), api.FPDF_ClosePage);
            ASSERT_NE(page.get(), nullptr);
            Handle font(api.FPDFText_LoadFont(document.get(), font_bytes.data(), static_cast<uint32_t>(font_bytes.size()), FPDF_FONT_TRUETYPE, cid), api.FPDFFont_Close);
            ASSERT_NE(font.get(), nullptr);
            Handle object(api.FPDFPageObj_CreateTextObj(document.get(), font.get(), 20), api.FPDFPageObj_Destroy);
            ASSERT_NE(object.get(), nullptr);
            ASSERT_TRUE(api.FPDFText_SetText(object.get(), text.data()));
            api.FPDFPageObj_Transform(object.get(), 1, 0, 0, 1, 12, 40);
            ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), object.release()));
            ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
            page.reset();
            font.reset();
            Raster before;
            std::vector<uint8_t> original_font;
            ASSERT_NO_FATAL_FAILURE(InspectEmbeddedText(api, document.get(), text, &before, &original_font));
            const auto output = SaveDocument(api, document.get(), artifacts, label, flags, version);
            std::array<Raster, 2> images;
            size_t index = 0;
            for (const auto* reader : Implementations()) {
              const auto& read = reader->api();
              Handle reopened(read.FPDF_LoadMemDocument64(output.data(), output.size(), nullptr), read.FPDF_CloseDocument);
              ASSERT_NE(reopened.get(), nullptr);
              std::vector<uint8_t> saved_font;
              ASSERT_NO_FATAL_FAILURE(InspectEmbeddedText(read, reopened.get(), text, &images[index], &saved_font));
              CompareEmbeddedRaster(before, images[index], cff && writer->name() != reader->name(), artifacts, label + "-to-" + reader->name());
              // Published Skera 0.9 cannot yet retain CFF outlines. The adapter
              // must keep the original embedded font instead of writing a broken
              // subset; this assertion can evolve with a future published release.
              if (cff && writer->name() == "rust") { EXPECT_EQ(saved_font, original_font); }
              ++index;
            }
            CompareEmbeddedRaster(images[0], images[1], cff, artifacts, label + "-readers");
          }
        }
      }
    }
  }
}

TEST(SaveVersions, UnsupportedVersionRequestsNeverProduceAnUnreadableSuccessfulSave) {
  Artifacts artifacts;
  const auto input = runtime_test::RectangleFixture();
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    for (int version : {-1, 0, 9, 18, 99}) {
      for (FPDF_DWORD flags : {FPDF_DWORD{0}, FPDF_DWORD{FPDF_INCREMENTAL}, FPDF_DWORD{FPDF_NO_INCREMENTAL}}) {
        Handle document(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument);
        ASSERT_NE(document.get(), nullptr);
        MemoryWriter sink;
        const bool saved = !!api.FPDF_SaveWithVersion(document.get(), &sink, flags, version);
        if (!saved) continue; // No public guarantee requires accepting an unsupported version.
        ASSERT_FALSE(sink.bytes.empty());
        const auto label = writer->name() + "-unsupported-v" + std::to_string(version) + "-flags" + std::to_string(flags);
        artifacts.Pdf(label, sink.bytes);
        for (const auto* reader : Implementations()) {
          const auto& read = reader->api();
          Handle reopened(read.FPDF_LoadMemDocument64(sink.bytes.data(), sink.bytes.size(), nullptr), read.FPDF_CloseDocument);
          ASSERT_NE(reopened.get(), nullptr);
          int actual_version = 0;
          ASSERT_TRUE(read.FPDF_GetFileVersion(reopened.get(), &actual_version));
          EXPECT_GE(actual_version, 10); EXPECT_LE(actual_version, 17);
          Raster image;
          ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(read, reopened.get(), &image));
          runtime_test::ExpectRectangle(image);
        }
      }
    }
  }
}

}  // namespace
}  // namespace pdfoxium::integration
