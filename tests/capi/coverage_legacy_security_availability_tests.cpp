#include "pinned_test_fonts.h"
#include "document_fixture.h"
#include "runtime_fixture.h"

#include <algorithm>
#include <array>
#include <cstdint>
#include <iomanip>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using namespace document_test;

std::string Unhex(const std::string& hex) {
  std::string result;
  for (size_t i = 0; i < hex.size(); i += 2) {
    const auto digit = [](char c) { return c <= '9' ? c - '0' : c - 'a' + 10; };
    result.push_back(static_cast<char>(digit(hex[i]) * 16 + digit(hex[i + 1])));
  }
  return result;
}

Bytes LegacyNoId(int revision, const std::string& trailer = "") {
  // Fixed vectors generated with published pypdf 6.10.0, not a local crypto
  // implementation. Reproduction: Encryption.make(RC4_40 or RC4_128,
  // 0xfffff0c0, b'').write_entry('user-secret', 'owner-secret'); encrypt_object
  // supplies object 4's stream and object 7's Title (generation 0).
  // The input ID is the empty byte string; the trailer deliberately has no ID.
  // Creator::InitID must generate an ID, regenerate U and reencrypt the objects.
  const bool r2 = revision == 2;
  const std::string owner = r2
      ? "89a62de6cfdd29e5d40604756b20bfabb0a4232df8fe57876d93b2dca90e3f4e"
      : "ccf72c1338637a6e9d08985f2923455120da2c467120970a15ccc346f6469e84";
  const std::string user = r2
      ? "2be5c371ada4eff025ad17820882db6bc2bf8884435193fe4ea04528efc96055"
      : "6250ab06ef5342c62e4b7f85a772d7f128bf4e5e4e758a4164004e56fffa0108";
  // Plaintext: rectangle followed by Helvetica 'Secret' at (20,78).
  const std::string content = r2
      ? "0976deb9a68e74c73cee301286c31c7ba5d4ae87e2bf336b4e6730655f3706fd6901339269d0005ebddbd4a9e57934a5e537b1199741bd72ce24b3f3d86d75f782bf687bfe9f2eacfc96f10b3ddc13"
      : "8f6f37d401067ea5ee0546220f4147ad21eae14a0b6edb435871afd98271a5990a99d094f8f683bf4e0991bb87a9b35243c74b31172827d711c9648d34bf990771574314cc161415e1df43aac96e09";
  const std::string title = r2 ? "0268df6be4565d4b63a5280a79055aa2eb"
                               : "30791a6875e7a18deb61223ee4b2359062";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << /Font << /F1 6 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", Unhex(content)),
      "<< /Filter /Standard /V " + std::string(r2 ? "1" : "2") + " /R " +
          std::to_string(revision) + " /Length " + std::string(r2 ? "40" : "128") +
          " /P -3904 /O <" + owner + "> /U <" + user + "> >>",
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
      "<< /Title <" + title + "> >>"}, "/Encrypt 5 0 R /Info 7 0 R " + trailer);
}

Raster LegacyPage(const Api& api, FPDF_DOCUMENT document) {
  Handle page(api.FPDF_LoadPage(document, 0), api.FPDF_ClosePage);
  if (!page.get()) { ADD_FAILURE() << "Expected encrypted page"; return {}; }
  return Render(api, page.get(), 128, 96);
}

void LegacySemantics(const Api& api, FPDF_DOCUMENT document, int revision, bool edited) {
  EXPECT_EQ(api.FPDF_GetPageCount(document), 1);
  EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(document), revision);
  EXPECT_EQ(api.FPDF_GetDocUserPermissions(document), 0xfffff0c0u);
  ExpectBytes([&](void* b, unsigned long n) {
    return api.FPDF_GetMetaText(document, "Title", b, n);
  }, Utf16(u"Empty-ID original"));
  if (edited) {
    ExpectBytes([&](void* b, unsigned long n) {
      return api.FPDFCatalog_GetLanguage(document, static_cast<FPDF_WCHAR*>(b), n);
    }, Utf16(u"en-US \u03a9"));
  }
  Handle page(api.FPDF_LoadPage(document, 0), api.FPDF_ClosePage);
  ASSERT_NE(page.get(), nullptr);
  Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
  ASSERT_NE(text.get(), nullptr);
  std::u16string characters;
  const int count = api.FPDFText_CountChars(text.get());
  ASSERT_GE(count, 6);
  ASSERT_LT(count, 32);
  for (int i = 0; i < count; ++i) {
    const auto c = api.FPDFText_GetUnicode(text.get(), i);
    if (c != '\r' && c != '\n') characters.push_back(static_cast<char16_t>(c));
  }
  EXPECT_EQ(characters, edited ? u"SecretSaved" : u"Secret");
}

void MissingIdSave(int revision, FPDF_DWORD flags) {
  Artifacts artifacts;
  // An explicit null also has no ID array. This reaches the same public save
  // path, rather than the different old-array (including empty-array) branch.
  for (const std::string trailer : {"", "/ID null"}) {
    const auto input = LegacyNoId(revision, trailer);
    std::array<Raster, 2> originals;
    size_t backend = 0;
    for (const auto* writer : Implementations()) {
      const auto& api = writer->api();
      const auto label = writer->name() + "-missing-id-r" + std::to_string(revision) +
          "-flags" + std::to_string(flags) + (trailer.empty() ? "-absent" : "-null");
      SCOPED_TRACE(label);
      artifacts.Pdf(label + "-input", input);
      for (const char* password : {static_cast<const char*>(nullptr), "incorrect"}) {
        Handle denied(api.FPDF_LoadMemDocument64(input.data(), input.size(), password), api.FPDF_CloseDocument);
        EXPECT_EQ(denied.get(), nullptr);
        EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_PASSWORD));
      }
      // Verify both passwords independently before changing any objects.
      for (const char* password : {"user-secret", "owner-secret"}) {
        Handle authenticated(api.FPDF_LoadMemDocument64(input.data(), input.size(), password), api.FPDF_CloseDocument);
        ASSERT_NE(authenticated.get(), nullptr);
        ASSERT_NO_FATAL_FAILURE(LegacySemantics(api, authenticated.get(), revision, false));
        EXPECT_EQ(api.FPDF_GetDocPermissions(authenticated.get()),
                  std::string(password) == "owner-secret" ? 0xfffffffcu : 0xfffff0c0u);
        EXPECT_EQ(api.FPDF_GetFileIdentifier(authenticated.get(), FILEIDTYPE_PERMANENT, nullptr, 0), 0u);
        runtime_test::ExpectRectangle(LegacyPage(api, authenticated.get()));
      }
      Handle document(api.FPDF_LoadMemDocument64(input.data(), input.size(), "user-secret"), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      const auto value = Wide(u"Saved");
      Handle text(api.FPDFPageObj_NewTextObj(document.get(), "Helvetica", 12), api.FPDFPageObj_Destroy);
      ASSERT_NE(text.get(), nullptr);
      ASSERT_TRUE(api.FPDFText_SetText(text.get(), value.data()));
      api.FPDFPageObj_Transform(text.get(), 1, 0, 0, 1, 20, 4);
      api.FPDFPage_InsertObject(page.get(), text.release());
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      const auto language = Wide(u"en-US \u03a9");
      ASSERT_TRUE(api.FPDFCatalog_SetLanguage(document.get(), language.data()));
      ASSERT_NO_FATAL_FAILURE(LegacySemantics(api, document.get(), revision, true));
      originals[backend] = Render(api, page.get(), 128, 96);
      runtime_test::ExpectRectangle(originals[backend]);
      const auto saved = SaveDocument(api, document.get(), artifacts, label, flags);
      ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api();
        SCOPED_TRACE(reader->name());
        Handle denied(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), "incorrect"), read.FPDF_CloseDocument);
        EXPECT_EQ(denied.get(), nullptr);
        EXPECT_EQ(read.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_PASSWORD));
        for (const char* password : {"user-secret", "owner-secret"}) {
          Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), password), read.FPDF_CloseDocument);
          ASSERT_NE(reopened.get(), nullptr);
          ASSERT_NO_FATAL_FAILURE(LegacySemantics(read, reopened.get(), revision, true));
          for (const auto type : {FILEIDTYPE_PERMANENT, FILEIDTYPE_CHANGING}) {
            EXPECT_EQ(read.FPDF_GetFileIdentifier(reopened.get(), type, nullptr, 0), 17u);
          }
          CompareRaster(originals[backend], LegacyPage(read, reopened.get()), artifacts,
                        label + "-to-" + reader->name() + "-" + password,
                        writer == reader ? RasterTolerance{} : RasterTolerance{24, 0, 0.15});
        }
      }
      ++backend;
    }
    CompareRaster(originals[0], originals[1], artifacts,
                  "legacy-r" + std::to_string(revision) + "-original" + trailer,
                  RasterTolerance{24, 0, 0.15});
  }
}

TEST(CoverageLegacySecurityAvailability, R2MissingIdFullSaveRegeneratesSecurityAndPreservesDecryptedObjects) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  MissingIdSave(2, FPDF_NO_INCREMENTAL);
}
TEST(CoverageLegacySecurityAvailability, R2MissingIdIncrementalSaveReencryptsEditedObjects) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  MissingIdSave(2, FPDF_INCREMENTAL);
}
TEST(CoverageLegacySecurityAvailability, R3MissingIdFullSaveRegeneratesSecurityAndPreservesDecryptedObjects) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  MissingIdSave(3, FPDF_NO_INCREMENTAL);
}
TEST(CoverageLegacySecurityAvailability, R3MissingIdIncrementalSaveReencryptsEditedObjects) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  MissingIdSave(3, FPDF_INCREMENTAL);
}

// Deliberately space dictionaries, content and Info in separate transport
// windows. Unknown/nonpositive root Count selects DataAvail's complete tree
// traversal (CheckPages -> CheckPage -> GetPageKids), rather than the ordinary
// page-number walk used by the positive-count loading-boundary scenarios.
Bytes UnknownCountTree(const std::string& count, bool indirect, bool empty_branch) {
  std::vector<std::string> objects = {
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages " + count + " /Kids " +
          (indirect ? std::string("13 0 R") : std::string("[3 0 R 4 0 R]")) + " >>",
      "<< /Type /Pages /Parent 2 0 R /Count 2 /Kids [5 0 R 6 0 R] >>",
      "<< /Type /Pages /Parent 2 0 R /Count 2 /Kids " +
          (empty_branch ? std::string("[14 0 R 7 0 R 8 0 R]") : std::string("[7 0 R 8 0 R]")) + " >>"};
  for (int page = 0; page < 4; ++page) {
    objects.push_back("<< /Type /Page /Parent " + std::to_string(page < 2 ? 3 : 4) +
        " 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents " + std::to_string(9 + page) + " 0 R >>");
  }
  for (const char* rgb : {"1 0 0", "0 1 0", "0 0 1", "1 0.5 0"}) {
    objects.push_back(PdfStream("", std::string(rgb) + " rg 16 16 64 48 re f"));
  }
  objects.push_back("[3 0 R 4 0 R]");
  objects.push_back("<< /Type /Pages /Parent 4 0 R /Count 0 /Kids [] >>");
  objects.push_back("<< /Title (Unknown-count transport) >>");
  std::ostringstream pdf;
  pdf << "%PDF-1.7\n";
  std::vector<std::streamoff> offsets{0};
  for (size_t i = 0; i < objects.size(); ++i) {
    pdf << '%' << std::string(4094, ' ') << '\n';
    offsets.push_back(pdf.tellp());
    pdf << i + 1 << " 0 obj\n" << objects[i] << "\nendobj\n";
  }
  const auto xref = pdf.tellp();
  pdf << "xref\n0 " << offsets.size() << "\n0000000000 65535 f \n";
  for (size_t i = 1; i < offsets.size(); ++i) {
    pdf << std::setfill('0') << std::setw(10) << offsets[i] << " 00000 n \n";
  }
  pdf << "trailer << /Root 1 0 R /Info 15 0 R /Size " << offsets.size()
      << " >>\nstartxref\n" << xref << "\n%%EOF\n";
  const auto data = pdf.str();
  return {data.begin(), data.end()};
}

struct TreeTransport : FX_FILEAVAIL, FX_DOWNLOADHINTS {
  explicit TreeTransport(Bytes input) : bytes(std::move(input)), present(bytes.size(), false) {
    FX_FILEAVAIL::version = 1;
    FX_DOWNLOADHINTS::version = 1;
    FX_FILEAVAIL::IsDataAvail = [](FX_FILEAVAIL* p, size_t offset, size_t length) -> FPDF_BOOL {
      auto& source = *static_cast<TreeTransport*>(p);
      if (offset > source.bytes.size() || length > source.bytes.size() - offset) return 0;
      return std::all_of(source.present.begin() + offset, source.present.begin() + offset + length,
                         [](bool value) { return value; });
    };
    FX_DOWNLOADHINTS::AddSegment = [](FX_DOWNLOADHINTS* p, size_t offset, size_t length) {
      auto& source = *static_cast<TreeTransport*>(p);
      try { source.hints.emplace_back(offset, length); } catch (...) { source.valid = false; }
    };
    access.m_FileLen = static_cast<unsigned long>(bytes.size());
    access.m_Param = this;
    access.m_GetBlock = [](void* p, unsigned long offset, unsigned char* output, unsigned long length) -> int {
      auto& source = *static_cast<TreeTransport*>(p);
      ++source.reads;
      const bool bounded = output && length && offset <= source.bytes.size() && length <= source.bytes.size() - offset;
      source.valid &= bounded;
      if (!bounded || !source.IsDataAvail(&source, offset, length)) return 0;
      std::copy_n(source.bytes.data() + offset, length, output);
      return 1;
    };
  }
  void Deliver(bool reverse) {
    // Partial ranges are legal: repeat IsDocAvail/IsPageAvail until every byte
    // in each requested window has arrived. Exercise both segment directions.
    for (const auto& range : hints) {
      EXPECT_LE(range.first, bytes.size());
      const size_t begin = std::min(range.first, bytes.size());
      const size_t end = begin + std::min(range.second, bytes.size() - begin);
      size_t supplied = 0;
      for (size_t i = 0; i < end - begin && supplied < 257; ++i) {
        const size_t at = reverse ? end - i - 1 : begin + i;
        if (!present[at]) { present[at] = true; ++supplied; ++delivered; }
      }
    }
    hints.clear();
  }
  void All() { std::fill(present.begin(), present.end(), true); }
  Bytes bytes;
  std::vector<bool> present;
  FPDF_FILEACCESS access{};
  std::vector<std::pair<size_t, size_t>> hints;
  size_t reads = 0;
  size_t delivered = 0;
  bool valid = true;
};

void TreeColor(const Raster& image, int page) {
  ASSERT_EQ(image.pixels.size(), 128u * 96 * 4);
  const std::array<std::array<int, 3>, 4> colors = {{{255, 0, 0}, {0, 255, 0}, {0, 0, 255}, {255, 128, 0}}};
  const size_t pixel = (48 * 128 + 40) * 4;
  EXPECT_NEAR(image.pixels[pixel + 2], colors[page][0], 1);
  EXPECT_NEAR(image.pixels[pixel + 1], colors[page][1], 1);
  EXPECT_NEAR(image.pixels[pixel], colors[page][2], 1);
  EXPECT_EQ(image.pixels[0], 255);
  EXPECT_EQ(image.pixels[1], 255);
  EXPECT_EQ(image.pixels[2], 255);
}

void UnknownCountAvailability(const std::string& count, bool indirect, bool empty_branch) {
  Artifacts artifacts;
  const auto input = UnknownCountTree(count, indirect, empty_branch);
  for (bool reverse : {false, true}) {
    std::array<std::array<Raster, 4>, 2> images;
    size_t backend = 0;
    for (const auto* writer : Implementations()) {
      const auto& api = writer->api();
      const auto label = writer->name() + "-unknown-count-" + count + (reverse ? "-reverse" : "-forward");
      SCOPED_TRACE(label);
      TreeTransport source(input);
      Handle provider(api.FPDFAvail_Create(&source, &source.access), api.FPDFAvail_Destroy);
      ASSERT_NE(provider.get(), nullptr);
      EXPECT_EQ(api.FPDFAvail_IsDocAvail(provider.get(), &source), PDF_DATA_NOTAVAIL);
      ASSERT_FALSE(source.hints.empty());
      // Polling without delivering data cannot falsely make the document ready.
      EXPECT_EQ(api.FPDFAvail_IsDocAvail(provider.get(), nullptr), PDF_DATA_NOTAVAIL);
      int state = PDF_DATA_NOTAVAIL;
      size_t waiting = 0;
      for (int poll = 0; poll < 4096 && state == PDF_DATA_NOTAVAIL; ++poll) {
        source.Deliver(reverse);
        state = api.FPDFAvail_IsDocAvail(provider.get(), &source);
        if (state == PDF_DATA_NOTAVAIL) ++waiting;
      }
      ASSERT_EQ(state, PDF_DATA_AVAIL);
      EXPECT_GT(waiting, 0u);
      EXPECT_EQ(api.FPDFAvail_IsDocAvail(provider.get(), nullptr), PDF_DATA_AVAIL);
      EXPECT_EQ(api.FPDFAvail_IsLinearized(provider.get()), PDF_NOT_LINEARIZED);
      Handle document(api.FPDFAvail_GetDocument(provider.get(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      EXPECT_EQ(api.FPDF_GetPageCount(document.get()), 4);
      EXPECT_EQ(api.FPDFAvail_GetFirstPageNum(document.get()), 0);
      for (int page : {3, 0, 2, 1, 3}) {
        state = PDF_DATA_NOTAVAIL;
        for (int poll = 0; poll < 4096 && state == PDF_DATA_NOTAVAIL; ++poll) {
          state = api.FPDFAvail_IsPageAvail(provider.get(), page, &source);
          source.Deliver(reverse);
        }
        ASSERT_EQ(state, PDF_DATA_AVAIL);
        EXPECT_EQ(api.FPDFAvail_IsPageAvail(provider.get(), page, nullptr), PDF_DATA_AVAIL);
        Handle loaded(api.FPDF_LoadPage(document.get(), page), api.FPDF_ClosePage);
        ASSERT_NE(loaded.get(), nullptr);
        images[backend][page] = Render(api, loaded.get(), 128, 96);
        ASSERT_NO_FATAL_FAILURE(TreeColor(images[backend][page], page));
      }
      source.All();
      EXPECT_EQ(api.FPDFAvail_IsFormAvail(provider.get(), nullptr), PDF_FORM_NOTEXIST);
      ExpectBytes([&](void* b, unsigned long n) {
        return api.FPDF_GetMetaText(document.get(), "Title", b, n);
      }, Utf16(u"Unknown-count transport"));
      const auto saved = SaveDocument(api, document.get(), artifacts, label);
      ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api();
        Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
        ASSERT_NE(reopened.get(), nullptr);
        EXPECT_EQ(read.FPDF_GetPageCount(reopened.get()), 4);
        for (int page = 0; page < 4; ++page) {
          Handle loaded(read.FPDF_LoadPage(reopened.get(), page), read.FPDF_ClosePage);
          ASSERT_NE(loaded.get(), nullptr);
          const auto actual = Render(read, loaded.get(), 128, 96);
          ASSERT_NO_FATAL_FAILURE(TreeColor(actual, page));
          CompareRaster(images[backend][page], actual, artifacts,
                        label + "-to-" + reader->name() + "-p" + std::to_string(page));
        }
      }
      EXPECT_TRUE(source.valid);
      EXPECT_GT(source.reads, 0u);
      EXPECT_GT(source.delivered, 0u);
      ++backend;
    }
    for (int page = 0; page < 4; ++page) {
      CompareRaster(images[0][page], images[1][page], artifacts,
                    "unknown-count" + count + "-direction" + std::to_string(reverse) + "-p" + std::to_string(page));
    }
  }
}

TEST(CoverageLegacySecurityAvailability, MissingRootCountTraversesAllPageDictionariesBeforeOpening) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  UnknownCountAvailability("", false, false);
}
TEST(CoverageLegacySecurityAvailability, ZeroRootCountFollowsIndirectKidsArraysAndNonsequentialPages) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  UnknownCountAvailability("/Count 0", true, false);
}
TEST(CoverageLegacySecurityAvailability, NegativeRootCountTraversesEmptyAndPopulatedSubtrees) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  UnknownCountAvailability("/Count -1", false, true);
}

}  // namespace
}  // namespace pdfoxium::integration
