#include "document_fixture.h"
#include "runtime_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <functional>
#include <iomanip>
#include <sstream>
#include <string>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using namespace document_test;
std::string DecodeHex(const std::string& text) {
  std::string result;
  const auto nibble = [](char c) { return c <= '9' ? c - '0' : c - 'a' + 10; };
  for (size_t i = 0; i + 1 < text.size(); i += 2) result.push_back(static_cast<char>(nibble(text[i]) * 16 + nibble(text[i + 1])));
  return result;
}
std::string StoredDeflate(const std::string& input) {
  // RFC1950/1951 stored block, solely for independently specifying malformed
  // hint tables without adding a compressor dependency to the test harness.
  std::string output("\x78\x01", 2); output.push_back(1);
  const auto length = static_cast<uint16_t>(input.size()), inverse = static_cast<uint16_t>(~length);
  for (uint16_t value : {length, inverse}) { output.push_back(static_cast<char>(value)); output.push_back(static_cast<char>(value >> 8)); }
  output += input; uint32_t a = 1, b = 0;
  for (unsigned char ch : input) { a = (a + ch) % 65521; b = (b + a) % 65521; }
  const uint32_t checksum = (b << 16) | a;
  for (int shift : {24, 16, 8, 0}) output.push_back(static_cast<char>(checksum >> shift));
  return output;
}
void SetBits(std::string& bytes, size_t start, unsigned width, uint32_t value) {
  if (start + width > bytes.size() * 8) throw std::runtime_error("Hint mutation outside fixed fixture");
  for (unsigned i = 0; i < width; ++i) {
    auto byte = static_cast<unsigned char>(bytes[(start + i) / 8]);
    const auto mask = static_cast<unsigned char>(1u << (7 - (start + i) % 8));
    byte = (value & (uint32_t(1) << (width - i - 1))) ? byte | mask : byte & ~mask;
    bytes[(start + i) / 8] = static_cast<char>(byte);
  }
}
std::string OriginalHintData() {
  // Published linearized.pdf, object37's 130 decoded bytes, S=52. Only named
  // numeric fields below change; ordinary objects/xref/page data stay intact.
  return DecodeHex("000000070000023f00020000042c0009000000000000000000000009000200040004000808170035806c0789ab88888817003580000000110000109d000000070000000c00000000002000092e9b4010598938592b98cc2612f000000000001600001728000000030000054f0000000000000000001b000028b1000000010000005c");
}
Bytes WithAppendedHint(std::string data, const std::string& shared_offset) {
  const auto original = runtime_test::Resource("linearized.pdf");
  std::string text(original.begin(), original.end());
  const auto hint = "40 0 obj\n" + PdfStream("/Filter /FlateDecode " + shared_offset, StoredDeflate(data)) + "\nendobj\n";
  const std::string old_range = "/H [ 496 191]";
  const auto range = "/H [" + std::to_string(original.size()) + " " + std::to_string(hint.size()) + "]";
  const auto at = text.find(old_range);
  if (at == std::string::npos || range.size() < old_range.size()) throw std::runtime_error("Pinned linearized header changed");
  text.replace(at, old_range.size(), range);
  // Consume only existing header padding so every original object/xref offset
  // remains correct. The new hint stream has no main-xref entry: H identifies
  // its indirect object directly, as required by the linearization parser.
  const auto padding = text.find("    ", text.find("endobj"));
  if (padding == std::string::npos) throw std::runtime_error("Pinned header padding changed");
  text.erase(padding, range.size() - old_range.size());
  const auto old_length = "/L " + std::to_string(original.size());
  const auto new_length = "/L " + std::to_string(original.size() + hint.size());
  if (old_length.size() != new_length.size()) throw std::runtime_error("Linearized length width changed");
  text.replace(text.find(old_length), old_length.size(), new_length);
  text += hint; return {text.begin(), text.end()};
}
struct RangeTransport : FX_FILEAVAIL, FX_DOWNLOADHINTS {
  explicit RangeTransport(Bytes input) : bytes(std::move(input)), present(bytes.size(), false) {
    FX_FILEAVAIL::version = FX_DOWNLOADHINTS::version = 1;
    FX_FILEAVAIL::IsDataAvail = Has; FX_DOWNLOADHINTS::AddSegment = Hint;
    access.m_FileLen = static_cast<unsigned long>(bytes.size()); access.m_Param = this; access.m_GetBlock = Read;
  }
  static FPDF_BOOL Has(FX_FILEAVAIL* p, size_t offset, size_t count) noexcept {
    auto& self = *static_cast<RangeTransport*>(p);
    if (offset > self.bytes.size() || count > self.bytes.size() - offset) return 0;
    return std::all_of(self.present.begin() + offset, self.present.begin() + offset + count, [](bool b) { return b; });
  }
  static void Hint(FX_DOWNLOADHINTS* p, size_t offset, size_t count) noexcept {
    auto& self = *static_cast<RangeTransport*>(p);
    try { self.hints.emplace_back(offset, count); } catch (...) { self.valid = false; }
  }
  static int Read(void* opaque, unsigned long offset, unsigned char* out, unsigned long count) noexcept {
    auto& self = *static_cast<RangeTransport*>(opaque);
    self.valid &= out && count && offset <= self.bytes.size() && count <= self.bytes.size() - offset;
    if (!self.valid || !Has(&self, offset, count)) return 0;
    std::copy_n(self.bytes.data() + offset, count, out); ++self.reads; return 1;
  }
  void Deliver() {
    for (const auto& range : hints) {
      const auto offset = std::min(range.first, bytes.size()), count = std::min(range.second, bytes.size() - offset);
      std::fill(present.begin() + offset, present.begin() + offset + count, true); ++deliveries;
    }
    hints.clear();
  }
  void All() { std::fill(present.begin(), present.end(), true); }
  Bytes bytes; std::vector<bool> present; FPDF_FILEACCESS access{};
  std::vector<std::pair<size_t, size_t>> hints; size_t reads = 0, deliveries = 0; bool valid = true;
};
Raster RenderPage(const Api& api, FPDF_DOCUMENT doc, int index) {
  Handle page(api.FPDF_LoadPage(doc, index), api.FPDF_ClosePage);
  if (!page.get()) { ADD_FAILURE() << "Expected original page " << index; return {}; }
  return Render(api, page.get(), 128, 96);
}
void HintFallback(std::string data, const std::string& shared_offset = "/S 52") {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto input = WithAppendedHint(std::move(data), shared_offset), control_bytes = runtime_test::Resource("linearized.pdf");
  Artifacts artifacts; artifacts.Pdf("mutated-hints", input);
  std::array<std::array<Raster, 3>, 2> images; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    RangeTransport source(input); Handle provider(api.FPDFAvail_Create(&source, &source.access), api.FPDFAvail_Destroy); ASSERT_NE(provider.get(), nullptr);
    int status = PDF_DATA_NOTAVAIL;
    for (int poll = 0; poll < 256 && status == PDF_DATA_NOTAVAIL; ++poll) { status = api.FPDFAvail_IsDocAvail(provider.get(), &source); source.Deliver(); }
    ASSERT_EQ(status, PDF_DATA_AVAIL); EXPECT_EQ(api.FPDFAvail_IsLinearized(provider.get()), PDF_LINEARIZED);
    Handle doc(api.FPDFAvail_GetDocument(provider.get(), nullptr), api.FPDF_CloseDocument); ASSERT_NE(doc.get(), nullptr); ASSERT_EQ(api.FPDF_GetPageCount(doc.get()), 3);
    Handle control(api.FPDF_LoadMemDocument64(control_bytes.data(), control_bytes.size(), nullptr), api.FPDF_CloseDocument); ASSERT_NE(control.get(), nullptr);
    source.All();
    for (int page : {2, 0, 1}) {
      status = PDF_DATA_NOTAVAIL;
      for (int poll = 0; poll < 256 && status == PDF_DATA_NOTAVAIL; ++poll) { status = api.FPDFAvail_IsPageAvail(provider.get(), page, &source); source.Deliver(); }
      ASSERT_EQ(status, PDF_DATA_AVAIL);
      images[backend][page] = RenderPage(api, doc.get(), page);
      CompareRaster(RenderPage(api, control.get(), page), images[backend][page], artifacts, writer->name() + "-original-page" + std::to_string(page));
    }
    EXPECT_TRUE(source.valid); EXPECT_GT(source.reads, 0u); EXPECT_GT(source.deliveries, 0u);
    const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-hint-fallback"); ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api(); Handle loaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument); ASSERT_NE(loaded.get(), nullptr); ASSERT_EQ(read.FPDF_GetPageCount(loaded.get()), 3);
      for (int page = 0; page < 3; ++page) CompareRaster(images[backend][page], RenderPage(read, loaded.get(), page), artifacts, writer->name() + "-" + reader->name() + "-page" + std::to_string(page), {24, 0, .15});
    }
    ++backend;
  }
  for (int page = 0; page < 3; ++page) CompareRaster(images[0][page], images[1][page], artifacts, "hint-fallback-engines" + std::to_string(page), {24, 0, .15});
}
void HintField(size_t bit, unsigned width, uint32_t value, bool padded = false) {
  auto data = OriginalHintData(); if (padded) data.resize(256, '\0');
  SetBits(data, bit, width, value); HintFallback(std::move(data));
}
TEST(CoverageParserRangeSecurity, MissingSharedHintOffsetFallsBackToTheMainCrossReference) { HintFallback(OriginalHintData(), ""); }
TEST(CoverageParserRangeSecurity, NonNumericSharedHintOffsetFallsBackToTheMainCrossReference) { HintFallback(OriginalHintData(), "/S /Unknown"); }
TEST(CoverageParserRangeSecurity, ZeroSharedHintOffsetFallsBackToTheMainCrossReference) { HintFallback(OriginalHintData(), "/S 0"); }
TEST(CoverageParserRangeSecurity, NegativeSharedHintOffsetFallsBackToTheMainCrossReference) { HintFallback(OriginalHintData(), "/S -1"); }
TEST(CoverageParserRangeSecurity, SharedHintOffsetBeyondDecodedDataFallsBackToTheMainCrossReference) { HintFallback(OriginalHintData(), "/S 999"); }
TEST(CoverageParserRangeSecurity, ShortDecodedHintTableDoesNotHideOrdinaryPages) { auto data = OriginalHintData(); data.resize(59); HintFallback(data); }
TEST(CoverageParserRangeSecurity, ZeroLeastPageObjectCountRejectsOnlyTheOptionalHints) { HintField(0, 32, 0); }
TEST(CoverageParserRangeSecurity, ExcessiveLeastPageObjectCountRejectsOnlyTheOptionalHints) { HintField(0, 32, 0xffffffffu); }
TEST(CoverageParserRangeSecurity, ZeroFirstPageObjectOffsetRejectsOnlyTheOptionalHints) { HintField(32, 32, 0); }
TEST(CoverageParserRangeSecurity, ZeroPageObjectCountBitWidthRejectsOnlyTheOptionalHints) { HintField(64, 16, 0); }
TEST(CoverageParserRangeSecurity, ExcessivePageObjectCountBitWidthRejectsOnlyTheOptionalHints) { HintField(64, 16, 33); }
TEST(CoverageParserRangeSecurity, ZeroMinimumPageLengthRejectsOnlyTheOptionalHints) { HintField(80, 32, 0); }
TEST(CoverageParserRangeSecurity, ZeroPageLengthBitWidthRejectsOnlyTheOptionalHints) { HintField(112, 16, 0); }
TEST(CoverageParserRangeSecurity, ExcessivePageLengthBitWidthRejectsOnlyTheOptionalHints) { HintField(112, 16, 33); }
TEST(CoverageParserRangeSecurity, ZeroSharedObjectCountWidthRejectsOnlyTheOptionalHints) { HintField(224, 16, 0); }
TEST(CoverageParserRangeSecurity, ExcessiveSharedObjectCountWidthRejectsOnlyTheOptionalHints) { HintField(224, 16, 33); }
TEST(CoverageParserRangeSecurity, ZeroSharedIdentifierWidthRejectsOnlyTheOptionalHints) { HintField(240, 16, 0); }
TEST(CoverageParserRangeSecurity, ExcessiveSharedIdentifierWidthRejectsOnlyTheOptionalHints) { HintField(240, 16, 33); }
TEST(CoverageParserRangeSecurity, ExcessiveSharedNumeratorWidthRejectsOnlyTheOptionalHints) { HintField(256, 16, 33); }
TEST(CoverageParserRangeSecurity, MinimumPageLengthAdditionCannotWrapTheHintRange) { HintField(80, 32, 0xffffffffu); }
TEST(CoverageParserRangeSecurity, ZeroSharedFirstObjectRejectsOnlyTheOptionalHints) { HintField(416, 32, 0); }
TEST(CoverageParserRangeSecurity, ZeroSharedFirstLocationRejectsOnlyTheOptionalHints) { HintField(448, 32, 0); }
TEST(CoverageParserRangeSecurity, SharedGroupObjectCountWidthCannotExceedThirtyTwoBits) { HintField(544, 16, 33); }
TEST(CoverageParserRangeSecurity, SharedGroupLengthWidthCannotBeZero) { HintField(592, 16, 0); }
TEST(CoverageParserRangeSecurity, ExcessiveSharedGroupLengthWidthRejectsOnlyTheOptionalHints) { HintField(592, 16, 33); }
TEST(CoverageParserRangeSecurity, SharedMinimumLengthAdditionCannotWrapTheHintRange) { HintField(560, 32, 0xffffffffu); }

// Compact malformed xref sections retain a healthy ordinary catalog/page tree.
// Recovery must preserve known content rather than merely accept the header.
struct XrefPdf {
  std::string text = "%PDF-1.7\n"; std::array<size_t, 6> positions{};
  XrefPdf() {
    Object(1, "<< /Type /Catalog /Pages 2 0 R >>"); Object(2, "<< /Type /Pages /Count 1 /Kids [3 0 R] >>");
    Object(3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents 4 0 R >>");
    Object(4, PdfStream("", "0.25 0.5 0.75 rg 16 16 64 48 re f"));
  }
  void Object(int number, const std::string& body) { positions[number] = text.size(); text += std::to_string(number) + " 0 obj\n" + body + "\nendobj\n"; }
  std::string Entries() {
    positions[5] = text.size(); std::string data;
    for (int i = 0; i < 6; ++i) {
      data.push_back(i ? 1 : 0); const uint32_t value = static_cast<uint32_t>(positions[i]);
      for (int shift : {24, 16, 8, 0}) data.push_back(static_cast<char>(value >> shift));
      data.push_back(i ? 0 : '\xff'); data.push_back(i ? 0 : '\xff');
    }
    return data;
  }
  Bytes Stream(const std::string& widths, const std::string& indices, std::string entries = "") {
    const auto offset = text.size(); if (entries.empty()) entries = Entries();
    Object(5, PdfStream("/Type /XRef /Size 6 /Root 1 0 R /W " + widths + " /Index " + indices, entries));
    text += "startxref\n" + std::to_string(offset) + "\n%%EOF\n"; return {text.begin(), text.end()};
  }
};
void RectangleRoundtrip(const Bytes& input) {
  Artifacts artifacts; artifacts.Pdf("xref-range-input", input); std::array<Raster, 2> images; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api(); Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument); ASSERT_NE(doc.get(), nullptr);
    ASSERT_EQ(api.FPDF_GetPageCount(doc.get()), 1); images[backend] = RenderPage(api, doc.get(), 0); runtime_test::ExpectRectangle(images[backend]);
    const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-xref-range"); ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api(); Handle loaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument); ASSERT_NE(loaded.get(), nullptr); ASSERT_EQ(read.FPDF_GetPageCount(loaded.get()), 1);
      const auto image = RenderPage(read, loaded.get(), 0); runtime_test::ExpectRectangle(image); CompareRaster(images[backend], image, artifacts, writer->name() + "-xref-to-" + reader->name());
    }
    ++backend;
  }
  CompareRaster(images[0], images[1], artifacts, "xref-range-engines");
}
TEST(CoverageParserRangeSecurity, NonNumericXrefIndexPairDoesNotConsumeTheValidFollowingRange) { XrefPdf pdf; RectangleRoundtrip(pdf.Stream("[1 4 2]", "[(bad) 1 0 6]")); }
TEST(CoverageParserRangeSecurity, EmptyXrefIndexPairDoesNotConsumeTheValidFollowingRange) { XrefPdf pdf; RectangleRoundtrip(pdf.Stream("[1 4 2]", "[0 0 0 6]")); }
TEST(CoverageParserRangeSecurity, NegativeXrefIndexPairDoesNotConsumeTheValidFollowingRange) { XrefPdf pdf; RectangleRoundtrip(pdf.Stream("[1 4 2]", "[-1 1 0 6]")); }
TEST(CoverageParserRangeSecurity, XrefIndexByteCountOverflowIsRecoveredFromOriginalObjects) { XrefPdf pdf; RectangleRoundtrip(pdf.Stream("[1 4 2]", "[0 2147483647]")); }
TEST(CoverageParserRangeSecurity, XrefWidthSumOverflowIsRecoveredFromOriginalObjects) { XrefPdf pdf; RectangleRoundtrip(pdf.Stream("[2147483647 2147483647 2]", "[0 6]")); }
TEST(CoverageParserRangeSecurity, NegativeXrefFieldWidthIsRecoveredFromOriginalObjects) { XrefPdf pdf; RectangleRoundtrip(pdf.Stream("[-1 4 2]", "[0 6]")); }
TEST(CoverageParserRangeSecurity, ClassicXrefCountCannotAllocateBeyondTheAvailableBytes) {
  for (const std::string count : {"1000000", "25000001"}) {
    XrefPdf pdf; const auto offset = pdf.text.size();
    pdf.text += "xref\n0 " + count + "\n0000000000 65535 f \ntrailer << /Size 5 /Root 1 0 R >>\nstartxref\n" + std::to_string(offset) + "\n%%EOF\n";
    RectangleRoundtrip(Bytes(pdf.text.begin(), pdf.text.end()));
  }
}
TEST(CoverageParserRangeSecurity, NonnumericZeroClassicObjectPositionIsRecoveredInsteadOfUsedAsAnOffset) {
  XrefPdf pdf; const auto offset = pdf.text.size();
  pdf.text += "xref\n0 2\n0000000000 65535 f \nabcdefghij 00000 n \ntrailer << /Size 5 /Root 1 0 R >>\nstartxref\n" + std::to_string(offset) + "\n%%EOF\n";
  RectangleRoundtrip(Bytes(pdf.text.begin(), pdf.text.end()));
}


struct SecurityVector {
  int revision;
  const char* owner;
  const char* user;
  const char* ciphertext;
  bool metadata = true;
  bool fallback_length = false;
  bool legacy_utf8_password = false;
};
struct MetadataNotice : UNSUPPORT_INFO {
  MetadataNotice() : UNSUPPORT_INFO{} {
    version = 1;
    FSDK_UnSupport_Handler = [](UNSUPPORT_INFO* p, int type) {
      if (type == FPDF_UNSP_DOC_SHAREDFORM_EMAIL) ++static_cast<MetadataNotice*>(p)->emails;
    };
  }
  int emails = 0;
};
MetadataNotice& Notice(size_t backend) { static std::array<MetadataNotice, 2> notices; return notices.at(backend); }
void PasswordConversionSave(const SecurityVector& vector) {
  // Fixed vectors generated with published pypdf6.10.0, Encryption.make(...)
  // and encrypt_object(stream,4,0). Legacy inputs use an empty ID and an
  // explicitly Latin1 or UTF8 password byte string; AES128 uses b'coverage-id'.
  // No encryption algorithm or random-IV generator is implemented here.
  const bool aes = vector.revision == 4;
  const bool clear_metadata = aes && !vector.metadata;
  std::vector<std::string> objects = {
      "<< /Type /Catalog /Pages 2 0 R" + std::string(clear_metadata ? " /Metadata 6 0 R" : "") + " >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents 4 0 R >>",
      PdfStream("", DecodeHex(vector.ciphertext)),
      "<< /Filter /Standard /V " + std::string(aes ? "4" : vector.revision == 2 ? "1" : "2") + " /R " + std::to_string(vector.revision) +
          " /Length " + std::string(vector.revision == 2 ? "40" : "128") + " /P -3904 /O <" + vector.owner + "> /U <" + vector.user + ">" +
          (aes ? " /StmF /StdCF /StrF /StdCF /CF << /StdCF << /CFM /AESV2" + std::string(vector.fallback_length ? "" : " /Length 16") + " >> >> /EncryptMetadata " + (vector.metadata ? "true" : "false") : "") + " >>"};
  if (clear_metadata) objects.push_back(PdfStream("/Type /Metadata /Subtype /XML",
      "<root xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\"><adhocwf:workflowType>0</adhocwf:workflowType></root>"));
  const auto input = PdfFixture(objects, "/Encrypt 5 0 R" + std::string(aes ? " /ID [<636f7665726167652d6964> <636f7665726167652d6964>]" : ""));
  Artifacts artifacts; artifacts.Pdf("password-conversion-input", input); std::array<Raster, 2> originals; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
     SCOPED_TRACE(writer->name());
    for (const char* password : {"\xe2ge", "\xc3\xa2ge"}) {
      SCOPED_TRACE(static_cast<unsigned char>(password[0]) == 0xe2 ? "Latin1 password bytes" : "UTF8 password bytes");
      auto& notice = Notice(backend); notice.emails = 0; ASSERT_TRUE(api.FSDK_SetUnSpObjProcessHandler(&notice));
      Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), password), api.FPDF_CloseDocument);
      // Legacy revisions only try UTF8->Latin1 conversion after raw bytes.
      // A file deliberately keyed with raw UTF8 cannot be opened by a Latin1
      // spelling: Latin1->UTF8 fallback belongs to revisions5 and6.
      if (vector.legacy_utf8_password && static_cast<unsigned char>(password[0]) == 0xe2) {
        EXPECT_EQ(doc.get(), nullptr);
        EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_PASSWORD));
        EXPECT_EQ(notice.emails, 0);
        continue;
      }
      ASSERT_NE(doc.get(), nullptr) << api.FPDF_GetLastError();
      EXPECT_EQ(notice.emails, clear_metadata ? 1 : 0);
      EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(doc.get()), vector.revision); EXPECT_EQ(api.FPDF_GetDocUserPermissions(doc.get()), 0xfffff0c0u);
      originals[backend] = RenderPage(api, doc.get(), 0); runtime_test::ExpectRectangle(originals[backend]);
      const auto language = Wide(u"en-NZ \u03a9"); ASSERT_TRUE(api.FPDFCatalog_SetLanguage(doc.get(), language.data()));
      for (FPDF_DWORD flags : {FPDF_NO_INCREMENTAL, FPDF_INCREMENTAL}) {
        const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-password-" + std::to_string(flags), flags); ASSERT_FALSE(saved.empty());
        for (const auto* reader : Implementations()) {
          const auto& read = reader->api();
     SCOPED_TRACE(reader->name());
          Handle denied(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), "wrong"), read.FPDF_CloseDocument); EXPECT_EQ(denied.get(), nullptr); EXPECT_EQ(read.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_PASSWORD));
          for (const char* variant : {"\xe2ge", "\xc3\xa2ge"}) {
            auto& receiver = Notice(reader->name() == "native" ? 0 : 1); receiver.emails = 0; ASSERT_TRUE(read.FSDK_SetUnSpObjProcessHandler(&receiver));
            SCOPED_TRACE(static_cast<unsigned char>(variant[0]) == 0xe2 ? "Latin1 password bytes" : "UTF8 password bytes");
            Handle loaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), variant), read.FPDF_CloseDocument);
            if (vector.legacy_utf8_password && static_cast<unsigned char>(variant[0]) == 0xe2) {
              EXPECT_EQ(loaded.get(), nullptr);
              EXPECT_EQ(read.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_PASSWORD));
              EXPECT_EQ(receiver.emails, 0);
              continue;
            }
            ASSERT_NE(loaded.get(), nullptr);
            EXPECT_EQ(receiver.emails, clear_metadata ? 1 : 0);
            EXPECT_EQ(read.FPDF_GetSecurityHandlerRevision(loaded.get()), vector.revision); EXPECT_EQ(read.FPDF_GetDocUserPermissions(loaded.get()), 0xfffff0c0u);
            ExpectBytes([&](void* out, unsigned long size) { return read.FPDFCatalog_GetLanguage(loaded.get(), static_cast<FPDF_WCHAR*>(out), size); }, Utf16(u"en-NZ \u03a9"));
            const auto image = RenderPage(read, loaded.get(), 0); runtime_test::ExpectRectangle(image); CompareRaster(originals[backend], image, artifacts, writer->name() + "-" + reader->name() + "-decrypted-" + std::to_string(flags));
          }
        }
      }
    }
    ++backend;
  }
  CompareRaster(originals[0], originals[1], artifacts, "password-conversion-engines");
}
TEST(CoverageParserRangeSecurity, R2Latin1PasswordConversionIsRetainedWhenMissingIdSaveRekeysTheFile) {
  PasswordConversionSave({2, "1eb22dbc5de012c8d3e93139d420b71a3f2f6a67d6b0b7a8ffac3add8ba777fa", "4620118c51fd945917535044e2dc77f4ac26e4c268aae898fd8ca046b1ad0d51", "12c01912650bfac251f0549115ae69bf61130a4da0c8c96c6eb413339c912ae1eb77"});
}
TEST(CoverageParserRangeSecurity, R2RawUtf8LegacyPasswordBytesAreRetainedWhenMissingIdSaveRekeysTheFile) {
  PasswordConversionSave({2, "3f772ff1ca1102d8e816fa1cb06eafb33ad46341d69e01ce47fa8472a8022060", "2f9eafc649cef7bea1182ba3e4ae4c19e979cdfcf86c05d77170c330cd9deaea", "277611288cd69908eb0128ce8d2e8d2426d3bdcab131def116a6fae325a259f98bae", true, false, true});
}
TEST(CoverageParserRangeSecurity, R3Latin1PasswordConversionIsRetainedWhenMissingIdSaveRekeysTheFile) {
  PasswordConversionSave({3, "5be32c49aa5e41439ae7ad1396234de0af51650c5f6e772587f34b47d4efd630", "8462cd320ede5933715e036a590717cb28bf4e5e4e758a4164004e56fffa0108", "d5fe186bd123cc93022d08cd0e8018a21e7782edbf257bb88e94d2f98a3fcc85cb8d"});
}
TEST(CoverageParserRangeSecurity, R3RawUtf8LegacyPasswordBytesAreRetainedWhenMissingIdSaveRekeysTheFile) {
  PasswordConversionSave({3, "7a262e043daf5153a1186636f26d5549aaaa6c2a5f40c1433fa5f5e8f74a81aa", "c0517a0f96e2a1d8f8fd88391485355f28bf4e5e4e758a4164004e56fffa0108", "542c28553334feda86595ef852efa465ff661ce8530ad82b870bcd18b27b860eeb92", true, false, true});
}
TEST(CoverageParserRangeSecurity, Aes128ExplicitCryptFilterLengthPreservesEncryptedStreamsAcrossSaving) {
  PasswordConversionSave({4, "5be32c49aa5e41439ae7ad1396234de0af51650c5f6e772587f34b47d4efd630", "8c66a0e7f7bc63c132c781ae9938cab828bf4e5e4e758a4164004e56fffa0108", "64b3a950f0119c5a9585f6b2ec6fdc455f1b2d4e9c80576c98b64f2567d20470699a00bf4b69e1abc0ffe9965c9e2f7b14a0e0802b92bea3983eca67d5d214c3"});
}
TEST(CoverageParserRangeSecurity, Aes128ExcludedMetadataAndMissingFilterLengthUseTheDocumentKeyLength) {
  PasswordConversionSave({4, "5be32c49aa5e41439ae7ad1396234de0af51650c5f6e772587f34b47d4efd630", "78a0159c6198f45debb72a8b3006d01828bf4e5e4e758a4164004e56fffa0108", "078eb37b3fd7898ef099cf018b24ac9a9c805a409cc90a0f741fd9e580db5cda607d13469166a78f5438828d1765dca3321d2b8822569295870e7e53ff288167", false, true});
}

}  // namespace
}  // namespace pdfoxium::integration
