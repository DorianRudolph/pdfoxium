#include "document_fixture.h"
#include "runtime_fixture.h"
#include "text_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <string>

namespace pdfoxium::integration {
namespace {
using namespace document_test;
using Bytes = std::vector<uint8_t>;
constexpr char kOwner[] = "\xc3\xa2" "ge";
constexpr char kUser[] = "h\xc3\xb4" "tel";
std::string Unhex(const std::string& hex) {
  const auto digit = [](char c) { return c <= '9' ? c - '0' : c - 'a' + 10; };
  std::string out;
  for (size_t i = 0; i + 1 < hex.size(); i += 2) out.push_back(static_cast<char>(digit(hex[i]) * 16 + digit(hex[i + 1])));
  return out;
}
Bytes FilterFixture(const std::string& filters, bool identity = false, const std::string& names = "/StrF /Chosen /StmF /Chosen", const std::string& length = "128") {
  // Published pypdf6.10.0 RC4_128 vector: ID=b'filter-id', user
  // 'filter-user', owner 'filter-owner', object4 generation0. R3 and R4
  // derive the same key when EncryptMetadata is true. Only CF selection and
  // its length representation change; the known plaintext is a rectangle.
  const std::string paint = identity ? "0.25 0.5 0.75 rg 16 16 64 48 re f\n" : Unhex("7238f2a887ecbdf613be4c23eb59dd3924b00d90214ed0ceb7bf75be1df3e5663100");
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents 4 0 R >>",
      PdfStream("", paint),
      "<< /Filter /Standard /V 4 /R 4 /Length " + length + " /P -3904 /O <0e6a1a27785a39d64f483181764f6a06f8e21109706d6c1f03fcf28f83c7cdb4> /U <a6d9e48986f1dd57f09f0d0b5cade88528bf4e5e4e758a4164004e56fffa0108> " + filters + " " + names + " >>"},
      "/Encrypt 5 0 R /ID [<66696c7465722d6964> <66696c7465722d6964>]");
}
Raster Paint(const Api& api, FPDF_DOCUMENT doc) {
  EXPECT_EQ(api.FPDF_GetPageCount(doc), 1);
  Raster result; runtime_test::RenderDocument(api, doc, &result); return result;
}
void Denied(const Api& api, const Bytes& input, const char* password) {
  Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), password), api.FPDF_CloseDocument);
  EXPECT_EQ(doc.get(), nullptr);
  EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_PASSWORD));
}
void ValidFilter(const std::string& cf, bool identity = false, const std::string& length = "128") {
  const auto input = FilterFixture(cf, identity, identity ? "/StrF /Identity /StmF /Identity" : "/StrF /Chosen /StmF /Chosen", length);
  Artifacts artifacts; artifacts.Pdf("crypt-filter-input", input); std::array<Raster, 2> images; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    for (const char* password : {identity ? "unrelated" : "filter-user", identity ? "" : "filter-owner"}) {
      Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), password), api.FPDF_CloseDocument); ASSERT_NE(doc.get(), nullptr) << api.FPDF_GetLastError();
      EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(doc.get()), 4);
      EXPECT_EQ(api.FPDF_GetDocUserPermissions(doc.get()), 0xfffff0c0u);
      images[backend] = Paint(api, doc.get()); runtime_test::ExpectRectangle(images[backend]);
      const auto language = Wide(u"nl-BE \u03a9"); ASSERT_TRUE(api.FPDFCatalog_SetLanguage(doc.get(), language.data()));
      for (FPDF_DWORD flags : {FPDF_NO_INCREMENTAL, FPDF_INCREMENTAL}) {
        const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-filter-" + std::to_string(flags), flags); ASSERT_FALSE(saved.empty());
        for (const auto* reader : Implementations()) {
          SCOPED_TRACE(reader->name()); const auto& read = reader->api();
          if (!identity) Denied(read, saved, "wrong");
          Handle loaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), password), read.FPDF_CloseDocument); ASSERT_NE(loaded.get(), nullptr);
          ExpectBytes([&](void* out, unsigned long n) { return read.FPDFCatalog_GetLanguage(loaded.get(), static_cast<FPDF_WCHAR*>(out), n); }, Utf16(u"nl-BE \u03a9"));
          CompareRaster(images[backend], Paint(read, loaded.get()), artifacts, writer->name() + "-" + reader->name() + "-filter-save");
        }
      }
    }
    ++backend;
  }
  CompareRaster(images[0], images[1], artifacts, "filter-backends");
}
void InvalidFilter(const std::string& cf, const std::string& names = "/StrF /Chosen /StmF /Chosen") {
  const auto input = FilterFixture(cf, false, names); const auto healthy = runtime_test::RectangleFixture();
  Artifacts artifacts; artifacts.Pdf("rejected-filter", input); std::array<Raster, 2> images; size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name()); const auto& api = library->api();
    Denied(api, input, "filter-user"); Denied(api, input, "filter-owner");
    Handle control(api.FPDF_LoadMemDocument64(healthy.data(), healthy.size(), nullptr), api.FPDF_CloseDocument); ASSERT_NE(control.get(), nullptr);
    images[backend++] = Paint(api, control.get()); runtime_test::ExpectRectangle(images[backend - 1]);
  }
  CompareRaster(images[0], images[1], artifacts, "healthy-after-filter-error");
}
TEST(CoverageCryptoFilterConstraints, IdentityNeedsNoPasswordOrContentEncryption) { ValidFilter("/CF << >>", true); }
TEST(CoverageCryptoFilterConstraints, IdentityIgnoresUnselectedMalformedCryptFilterAndKeyLength) { ValidFilter("/CF << /Unused 17 >>", true, "-1"); }
TEST(CoverageCryptoFilterConstraints, V2FilterAcceptsByteLength16) { ValidFilter("/CF << /Chosen << /CFM /V2 /Length 16 >> >>"); }
TEST(CoverageCryptoFilterConstraints, V2FilterAcceptsBitLength128) { ValidFilter("/CF << /Chosen << /CFM /V2 /Length 128 >> >>"); }
TEST(CoverageCryptoFilterConstraints, MissingFilterLengthUsesDocumentKeyLength) { ValidFilter("/CF << /Chosen << /CFM /V2 >> >>"); }
TEST(CoverageCryptoFilterConstraints, ZeroFilterLengthUsesDocumentKeyLength) { ValidFilter("/CF << /Chosen << /CFM /V2 /Length 0 >> >>"); }
TEST(CoverageCryptoFilterConstraints, MissingCryptFilterDictionaryRejectsEncryption) { InvalidFilter(""); }
TEST(CoverageCryptoFilterConstraints, ArrayCryptFilterDictionaryRejectsEncryption) { InvalidFilter("/CF []"); }
TEST(CoverageCryptoFilterConstraints, MissingSelectedFilterRejectsEncryption) { InvalidFilter("/CF << /Unused << /CFM /V2 /Length 16 >> >>"); }
TEST(CoverageCryptoFilterConstraints, ScalarSelectedFilterRejectsEncryption) { InvalidFilter("/CF << /Chosen 16 >>"); }
TEST(CoverageCryptoFilterConstraints, NegativeSelectedFilterLengthRejectsEncryption) { InvalidFilter("/CF << /Chosen << /CFM /V2 /Length -1 >> >>"); }
TEST(CoverageCryptoFilterConstraints, Rc4KeyShorterThanFiveBytesRejectsEncryption) { InvalidFilter("/CF << /Chosen << /CFM /V2 /Length 4 >> >>"); }
TEST(CoverageCryptoFilterConstraints, Rc4KeyLongerThanSixteenBytesRejectsEncryption) { InvalidFilter("/CF << /Chosen << /CFM /V2 /Length 17 >> >>"); }
TEST(CoverageCryptoFilterConstraints, AesKeyWithUnsupportedLengthRejectsEncryption) { InvalidFilter("/CF << /Chosen << /CFM /AESV2 /Length 15 >> >>"); }
TEST(CoverageCryptoFilterConstraints, DifferentStreamAndStringFiltersRejectEncryption) { InvalidFilter("/CF << /Chosen << /CFM /V2 /Length 16 >> >>", "/StrF /Identity /StmF /Chosen"); }

Bytes AesFixture(int revision) { return runtime_test::Resource("encrypted_hello_world_r" + std::to_string(revision) + ".pdf"); }
Bytes EditHex(Bytes input, const std::string& key, size_t bytes, bool corrupt = false) {
  std::string text(input.begin(), input.end()); const auto marker = "/" + key + " <";
  const auto start = text.find(marker); if (start == std::string::npos) throw std::runtime_error("Pinned encryption entry missing");
  const auto begin = start + marker.size() - 1, end = text.find('>', begin);
  if (end == std::string::npos || bytes * 2 > end - begin - 1) throw std::runtime_error("Pinned encryption hex length changed");
  if (corrupt) { text[begin + 1] = text[begin + 1] == '0' ? '1' : '0'; }
  else { auto replacement = "<" + text.substr(begin + 1, bytes * 2) + ">"; replacement.append(end - begin + 1 - replacement.size(), ' '); text.replace(begin, end - begin + 1, replacement); }
  return {text.begin(), text.end()};
}
Bytes PermissionMismatch(Bytes input, bool metadata) {
  std::string text(input.begin(), input.end());
  if (!metadata) {
    const auto at = text.find("/P -4 "); if (at == std::string::npos) throw std::runtime_error("Pinned permission entry changed"); text.replace(at, 6, "/P -8 ");
  } else {
    const auto start = text.find("7 0 obj"), end = text.find("endobj", start);
    if (start == std::string::npos || end == std::string::npos) throw std::runtime_error("Pinned encryption object changed");
    auto body = text.substr(start, end - start); const auto original_size = body.size();
    // Remove optional CF fields to make room without changing a single xref
    // offset. V5 takes its key length from the outer /Length256 entry.
    for (const auto& token : {std::string("/AuthEvent /DocOpen "), std::string("/Length 32 ")}) {
      const auto at = body.find(token); if (at == std::string::npos) throw std::runtime_error("Pinned optional crypt filter fields changed"); body.erase(at, token.size());
    }
    const auto close = body.rfind(">>"); body.insert(close, "/EncryptMetadata false ");
    if (body.size() > original_size) {
      throw std::runtime_error("Encryption mutation needs extra space");
    }
    body.append(original_size - body.size(), ' ');
    text.replace(start, original_size, body);
  }
  return {text.begin(), text.end()};
}
void InspectHello(const Api& api, FPDF_DOCUMENT doc, int revision) {
  EXPECT_EQ(api.FPDF_GetPageCount(doc), 1); EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(doc), revision);
  Handle page(api.FPDF_LoadPage(doc, 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
  Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage); ASSERT_NE(text.get(), nullptr);
  EXPECT_EQ(text_fixture::Extract(api, text.get()), u"Hello, world!\r\nGoodbye, world!");
}
void AesGuard(int revision, const Bytes& malformed, const char* denied = nullptr, const char* working = nullptr) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto original = AesFixture(revision); ASSERT_EQ(malformed.size(), original.size());
  Artifacts artifacts; artifacts.Pdf("aes-guard-input", malformed); std::array<Raster, 2> images; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    if (denied) Denied(api, malformed, denied); else { Denied(api, malformed, kOwner); Denied(api, malformed, kUser); }
    Handle healthy(api.FPDF_LoadMemDocument64(original.data(), original.size(), kUser), api.FPDF_CloseDocument); ASSERT_NE(healthy.get(), nullptr);
    InspectHello(api, healthy.get(), revision); images[backend] = Paint(api, healthy.get());
    const auto& source = working ? malformed : original;
    Handle doc(api.FPDF_LoadMemDocument64(source.data(), source.size(), working ? working : kOwner), api.FPDF_CloseDocument); ASSERT_NE(doc.get(), nullptr) << api.FPDF_GetLastError();
    InspectHello(api, doc.get(), revision); CompareRaster(images[backend], Paint(api, doc.get()), artifacts, writer->name() + "-surviving-credential");
    const auto language = Wide(u"fr-CA"); ASSERT_TRUE(api.FPDFCatalog_SetLanguage(doc.get(), language.data()));
    for (FPDF_DWORD flags : {FPDF_NO_INCREMENTAL, FPDF_INCREMENTAL}) {
      const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-aes-guard-" + std::to_string(flags), flags); ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api();
     SCOPED_TRACE(reader->name());
        Handle loaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), working ? working : kUser), read.FPDF_CloseDocument); ASSERT_NE(loaded.get(), nullptr);
        InspectHello(read, loaded.get(), revision);
        ExpectBytes([&](void* out, unsigned long n) { return read.FPDFCatalog_GetLanguage(loaded.get(), static_cast<FPDF_WCHAR*>(out), n); }, Utf16(u"fr-CA"));
        const auto reloaded_image = Paint(read, loaded.get());
        const auto label = writer->name() + "-" + reader->name() + "-aes-recovery";
        if (writer->name() == reader->name()) {
          CompareRaster(images[backend], reloaded_image, artifacts, label);
        } else {
          text_fixture::CompareTextRaster(images[backend], reloaded_image, artifacts, label);
        }
        if (denied) Denied(read, saved, denied);
      }
    }
    ++backend;
  }
  text_fixture::CompareTextRaster(images[0], images[1], artifacts, "aes-guard-backends");
}
TEST(CoverageCryptoFilterConstraints, R5ShortOwnerValidationEntryRejectsBothCredentials) { AesGuard(5, EditHex(AesFixture(5), "O", 47)); }
TEST(CoverageCryptoFilterConstraints, R6ShortOwnerValidationEntryRejectsBothCredentials) { AesGuard(6, EditHex(AesFixture(6), "O", 47)); }
TEST(CoverageCryptoFilterConstraints, R5ShortUserValidationEntryRejectsBothCredentials) { AesGuard(5, EditHex(AesFixture(5), "U", 47)); }
TEST(CoverageCryptoFilterConstraints, R6ShortUserValidationEntryRejectsBothCredentials) { AesGuard(6, EditHex(AesFixture(6), "U", 47)); }
TEST(CoverageCryptoFilterConstraints, R5ShortUserEncryptedKeyPreservesOwnerAccess) { AesGuard(5, EditHex(AesFixture(5), "UE", 31), kUser, kOwner); }
TEST(CoverageCryptoFilterConstraints, R6ShortUserEncryptedKeyPreservesOwnerAccess) { AesGuard(6, EditHex(AesFixture(6), "UE", 31), kUser, kOwner); }
TEST(CoverageCryptoFilterConstraints, R5ShortOwnerEncryptedKeyPreservesUserAccess) { AesGuard(5, EditHex(AesFixture(5), "OE", 31), kOwner, kUser); }
TEST(CoverageCryptoFilterConstraints, R6ShortOwnerEncryptedKeyPreservesUserAccess) { AesGuard(6, EditHex(AesFixture(6), "OE", 31), kOwner, kUser); }
TEST(CoverageCryptoFilterConstraints, R5EmptyPermissionCiphertextRejectsPasswords) { AesGuard(5, EditHex(AesFixture(5), "Perms", 0)); }
TEST(CoverageCryptoFilterConstraints, R6EmptyPermissionCiphertextRejectsPasswords) { AesGuard(6, EditHex(AesFixture(6), "Perms", 0)); }
TEST(CoverageCryptoFilterConstraints, R5CorruptPermissionCiphertextRejectsPasswords) { AesGuard(5, EditHex(AesFixture(5), "Perms", 16, true)); }
TEST(CoverageCryptoFilterConstraints, R6CorruptPermissionCiphertextRejectsPasswords) { AesGuard(6, EditHex(AesFixture(6), "Perms", 16, true)); }
TEST(CoverageCryptoFilterConstraints, R5PublicPermissionMismatchRejectsPasswords) { AesGuard(5, PermissionMismatch(AesFixture(5), false)); }
TEST(CoverageCryptoFilterConstraints, R6PublicPermissionMismatchRejectsPasswords) { AesGuard(6, PermissionMismatch(AesFixture(6), false)); }
TEST(CoverageCryptoFilterConstraints, R5MetadataPermissionMismatchRejectsPasswords) { AesGuard(5, PermissionMismatch(AesFixture(5), true)); }
TEST(CoverageCryptoFilterConstraints, R6MetadataPermissionMismatchRejectsPasswords) { AesGuard(6, PermissionMismatch(AesFixture(6), true)); }
}  // namespace
}  // namespace pdfoxium::integration
