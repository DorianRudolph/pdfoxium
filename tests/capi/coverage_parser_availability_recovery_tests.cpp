#include "document_fixture.h"
#include "runtime_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <functional>
#include <iomanip>
#include <map>
#include <sstream>
#include <string>
#include <utility>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using namespace document_test;
constexpr char kPaint[] = "0.25 0.5 0.75 rg 16 16 64 48 re f\n";

// Own all offsets so syntax and recovery cases do not accidentally test a
// second, stale-offset error. Only the selected structure varies per case.
struct RecoveryPdf {
  std::string bytes = "%PDF-1.7\n";
  std::map<int, std::pair<size_t, int>> offsets;
  void Object(int number, const std::string& value, int generation = 0) {
    offsets[number] = {bytes.size(), generation};
    bytes += std::to_string(number) + " " + std::to_string(generation) + " obj\n" + value + "\nendobj\n";
  }
  size_t Xref(const std::string& trailer = "", int root = 1) {
    const size_t start = bytes.size();
    std::ostringstream table;
    table << "xref\n0 " << offsets.rbegin()->first + 1 << "\n0000000000 65535 f \n";
    for (int n = 1; n <= offsets.rbegin()->first; ++n) {
      const auto found = offsets.find(n);
      if (found == offsets.end()) table << "0000000000 00000 f \n";
      else table << std::setfill('0') << std::setw(10) << found->second.first << ' '
                 << std::setw(5) << found->second.second << " n \n";
    }
    bytes += table.str() + "trailer << /Size " + std::to_string(offsets.rbegin()->first + 1) +
        " /Root " + std::to_string(root) + " 0 R " + trailer + " >>\nstartxref\n" +
        std::to_string(start) + "\n%%EOF\n";
    return start;
  }
  Bytes Data() const { return {bytes.begin(), bytes.end()}; }
};
RecoveryPdf Basic(const std::string& content = kPaint,
                  const std::string& content_entries = "", const std::string& catalog = "") {
  RecoveryPdf pdf;
  pdf.Object(1, "<< /Type /Catalog /Pages 2 0 R " + catalog + " >>");
  pdf.Object(2, "<< /Type /Pages /Count 1 /Kids [3 0 R] >>");
  pdf.Object(3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents 4 0 R >>");
  pdf.Object(4, PdfStream(content_entries, content));
  return pdf;
}
Raster FirstPage(const Api& api, FPDF_DOCUMENT document) {
  Handle page(api.FPDF_LoadPage(document, 0), api.FPDF_ClosePage);
  if (!page.get()) { ADD_FAILURE() << "Expected page zero"; return {}; }
  return Render(api, page.get(), 128, 96);
}
void CheckRectangle(const Api& api, FPDF_DOCUMENT document) {
  EXPECT_EQ(api.FPDF_GetPageCount(document), 1);
  runtime_test::ExpectRectangle(FirstPage(api, document));
}
void ExerciseRecovery(const Bytes& input, const std::string& label,
                      const std::function<void(const Api&, FPDF_DOCUMENT)>& inspect = CheckRectangle) {
  Artifacts artifacts;
  artifacts.Pdf(label + "-input", input);
  std::array<Raster, 2> originals;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name() + "-" + label);
    const auto& api = writer->api();
    Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr) << api.FPDF_GetLastError();
    ASSERT_NO_FATAL_FAILURE(inspect(api, doc.get()));
    originals[backend] = FirstPage(api, doc.get());
    for (FPDF_DWORD flags : {FPDF_NO_INCREMENTAL, FPDF_INCREMENTAL}) {
      const auto name = writer->name() + label + "-" + std::to_string(flags);
      const auto saved = SaveDocument(api, doc.get(), artifacts, name, flags);
      ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api();
        SCOPED_TRACE(name + "-" + reader->name());
        Handle loaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
        ASSERT_NE(loaded.get(), nullptr);
        ASSERT_NO_FATAL_FAILURE(inspect(read, loaded.get()));
        CompareRaster(originals[backend], FirstPage(read, loaded.get()), artifacts, name + reader->name());
      }
    }
    ++backend;
  }
  CompareRaster(originals[0], originals[1], artifacts, label + "-engines");
}

TEST(CoverageParserAvailabilityRecovery, UnresolvedTrailerRootIsRejectedAfterTheRepairAttempt) {
  auto pdf = Basic(); pdf.Xref("", 99);
  const auto input = pdf.Data();
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument);
    EXPECT_EQ(doc.get(), nullptr);
    EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_FORMAT));
  }
}
TEST(CoverageParserAvailabilityRecovery, NonDictionaryTrailerRootIsRejectedWithoutParsingPageContents) {
  auto pdf = Basic(); pdf.Object(5, "(not a catalog dictionary)"); pdf.Xref("", 5);
  const auto input = pdf.Data();
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument);
    EXPECT_EQ(doc.get(), nullptr);
    EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_FORMAT));
  }
}
TEST(CoverageParserAvailabilityRecovery, CrossReferenceRebuildSkipsNonCatalogObjectsWithTypeEntries) {
  auto pdf = Basic();
  pdf.Object(5, "<< /Type /SomethingElse /Pages 99 0 R >>");
  pdf.Object(6, "[1 2 3 /Catalog]");
  const auto xref = pdf.Xref();
  const auto marker = "startxref\n" + std::to_string(xref);
  pdf.bytes.replace(pdf.bytes.rfind(marker), marker.size(), "startxref\n0");
  ExerciseRecovery(pdf.Data(), "catalog-type-selection");
}
TEST(CoverageParserAvailabilityRecovery, HeaderPrefixAndMixedLineEndingsRetainActualObjectOffsets) {
  for (const std::string& prefix : std::array<std::string, 3>{" ", "%transport prefix\n", std::string(128, ' ')}) {
    // PDF offsets are relative to the PDF header, not the transport prefix.
    auto pdf = Basic(); pdf.Xref(); pdf.bytes.insert(0, prefix);
    ExerciseRecovery(pdf.Data(), "header-prefix-" + std::to_string(prefix.size()));
  }
}
TEST(CoverageParserAvailabilityRecovery, ReferenceObjectsResolveThroughIndirectLengthAndContentsArrays) {
  auto pdf = Basic();
  pdf.Object(3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents 5 0 R >>");
  pdf.Object(4, "<< /Length 6 0 R >>\nstream\n" + std::string(kPaint) + "endstream");
  pdf.Object(5, "[null 4 0 R]"); pdf.Object(6, std::to_string(std::string(kPaint).size())); pdf.Xref();
  ExerciseRecovery(pdf.Data(), "indirect-content-length-array");
}
TEST(CoverageParserAvailabilityRecovery, NegativeStreamLengthRecoversAtTheEndstreamDelimiter) {
  auto pdf = Basic(); pdf.Object(4, "<< /Length -1 >>\nstream\n" + std::string(kPaint) + "endstream"); pdf.Xref();
  ExerciseRecovery(pdf.Data(), "negative-stream-length");
}
TEST(CoverageParserAvailabilityRecovery, IncorrectLongLengthDoesNotConsumeFollowingObjects) {
  auto pdf = Basic(); pdf.Object(4, "<< /Length 999999 >>\nstream\n" + std::string(kPaint) + "endstream");
  pdf.Object(5, "<< /Title (After recovered stream) >>"); pdf.Xref("/Info 5 0 R");
  ExerciseRecovery(pdf.Data(), "long-stream-length", [](const Api& api, FPDF_DOCUMENT doc) {
    CheckRectangle(api, doc);
    ExpectBytes([&](void* b, unsigned long n) { return api.FPDF_GetMetaText(doc, "Title", b, n); }, Utf16(u"After recovered stream"));
  });
}
TEST(CoverageParserAvailabilityRecovery, ZeroDeclaredLengthWithNonemptyDataUsesRecoveredStream) {
  auto pdf = Basic(); pdf.Object(4, "<< /Length 0 >>\nstream\n" + std::string(kPaint) + "endstream"); pdf.Xref();
  ExerciseRecovery(pdf.Data(), "zero-stream-length");
}
TEST(CoverageParserAvailabilityRecovery, EmptyStreamIsDifferentFromAnAbsentContentsEntry) {
  for (bool absent : {false, true}) {
    auto pdf = Basic("");
    if (absent) pdf.Object(3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << >> >>");
    pdf.Xref();
    ExerciseRecovery(pdf.Data(), "empty-content-" + std::to_string(absent), [](const Api& api, FPDF_DOCUMENT doc) {
      EXPECT_EQ(api.FPDF_GetPageCount(doc), 1);
      const auto image = FirstPage(api, doc);
      EXPECT_TRUE(std::all_of(image.pixels.begin(), image.pixels.end(), [](uint8_t b) { return b == 255; }));
    });
  }
}
TEST(CoverageParserAvailabilityRecovery, EscapedNamesAndHexStringsSurviveMetadataSerialization) {
  auto pdf = Basic();
  pdf.Object(5, "<< /Tit#6ce <FEFF00530079006E007400610078002003A9> /Auth#6fr (A\\050B\\051\\134C) /Subject (line\\\ncontinuation) >>");
  pdf.Xref("/Info 5 0 R");
  ExerciseRecovery(pdf.Data(), "escaped-info", [](const Api& api, FPDF_DOCUMENT doc) {
    CheckRectangle(api, doc);
    for (const auto& entry : {std::pair<const char*, std::u16string>{"Title", u"Syntax \u03a9"}, {"Author", u"A(B)\\C"}, {"Subject", u"linecontinuation"}})
      ExpectBytes([&](void* b, unsigned long n) { return api.FPDF_GetMetaText(doc, entry.first, b, n); }, Utf16(entry.second));
  });
}

// Recursive page-index queries use references before loading the target page.
// Loading in a different order afterward also checks the document's traversal
// cache rather than relying on the page object cache populated by rendering.
std::string DestinationName(int index) {
  std::ostringstream name; name << "page" << std::setfill('0') << std::setw(3) << index; return name.str();
}
Bytes DeepPages(bool indirect_kids, int leaves_per_branch, int depth, bool with_empty) {
  RecoveryPdf pdf;
  int next = 10;
  std::vector<int> page_numbers;
  std::function<int(int, int)> branch = [&](int level, int parent) {
    const int number = next++;
    std::vector<int> children;
    if (level) {
      for (int i = 0; i < 2; ++i) children.push_back(branch(level - 1, number));
    } else {
      for (int i = 0; i < leaves_per_branch; ++i) {
        const int page = next++, content = next++;
        page_numbers.push_back(page);
        pdf.Object(page, "<< /Type /Page /Parent " + std::to_string(number) + " 0 R /Contents " + std::to_string(content) + " 0 R >>");
        pdf.Object(content, PdfStream("", kPaint)); children.push_back(page);
      }
    }
    if (with_empty) {
      const int empty = next++;
      pdf.Object(empty, "<< /Type /Pages /Parent " + std::to_string(number) + " 0 R /Count 0 /Kids [] >>");
      children.insert(children.begin(), empty);
    }
    std::string kids = "[";
    for (int child : children) kids += std::to_string(child) + " 0 R ";
    kids += "]";
    if (indirect_kids) { const int array = next++; pdf.Object(array, kids); kids = std::to_string(array) + " 0 R"; }
    const int count = (1 << level) * leaves_per_branch;
    pdf.Object(number, "<< /Type /Pages /Parent " + std::to_string(parent) + " 0 R /Count " + std::to_string(count) + " /Kids " + kids + " >>");
    return number;
  };
  const int tree = branch(depth, 2);
  pdf.Object(2, "<< /Type /Pages /Count " + std::to_string(page_numbers.size()) +
      " /Kids [" + std::to_string(tree) + " 0 R] /MediaBox [0 0 128 96] /Resources << >> >>");
  std::string names = "[";
  for (size_t i = 0; i < page_numbers.size(); ++i) {
    names += "(" + DestinationName(static_cast<int>(i)) + ") [" + std::to_string(page_numbers[i]) + " 0 R /XYZ 12 34 1] ";
  }
  names += "]";
  pdf.Object(1, "<< /Type /Catalog /Pages 2 0 R /Names << /Dests << /Names " + names + " >> >> >>");
  pdf.Xref(); return pdf.Data();
}
void PageIndices(bool indirect, int width, int depth, bool empty) {
  const int count = width * (1 << depth);
  const auto inspect = [=](const Api& api, FPDF_DOCUMENT doc) {
    EXPECT_EQ(api.FPDF_GetPageCount(doc), count);
    for (int i = count - 1; i >= 0; --i) {
      const auto name = DestinationName(i);
      auto destination = api.FPDF_GetNamedDestByName(doc, name.c_str());
      ASSERT_NE(destination, nullptr);
      EXPECT_EQ(api.FPDFDest_GetDestPageIndex(doc, destination), i);
      unsigned long count = 0; FS_FLOAT parameters[4] = {};
      EXPECT_EQ(api.FPDFDest_GetView(destination, &count, parameters), static_cast<unsigned long>(PDFDEST_VIEW_XYZ));
      ASSERT_EQ(count, 3u); EXPECT_FLOAT_EQ(parameters[0], 12); EXPECT_FLOAT_EQ(parameters[1], 34); EXPECT_FLOAT_EQ(parameters[2], 1);
    }
    for (int i = 0; i < count; i += 2) {
      Handle page(api.FPDF_LoadPage(doc, i), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      EXPECT_FLOAT_EQ(api.FPDF_GetPageWidthF(page.get()), 128);
      EXPECT_FLOAT_EQ(api.FPDF_GetPageHeightF(page.get()), 96);
    }
    runtime_test::ExpectRectangle(FirstPage(api, doc));
  };
  ExerciseRecovery(DeepPages(indirect, width, depth, empty), "page-index-" + std::to_string(indirect) + "-" + std::to_string(width) + "-" + std::to_string(depth) + "-" + std::to_string(empty), inspect);
}
TEST(CoverageParserAvailabilityRecovery, NestedPageIndicesSkipWholePopulatedBranches) { PageIndices(false, 3, 2, false); }
TEST(CoverageParserAvailabilityRecovery, IndirectKidsArraysPreserveDeepDestinationIdentity) { PageIndices(true, 2, 3, false); }
TEST(CoverageParserAvailabilityRecovery, EmptySiblingBranchesDoNotConsumeDestinationIndices) { PageIndices(false, 2, 2, true); }
TEST(CoverageParserAvailabilityRecovery, IndirectKidsWithEmptyBranchesRemainStableAcrossSaveAndReload) { PageIndices(true, 2, 2, true); }
TEST(CoverageParserAvailabilityRecovery, WideLeafArraysSupportReverseQueriesBeforePageLoading) { PageIndices(false, 12, 0, false); }

void AttachmentNames(const Api& api, FPDF_DOCUMENT doc, const std::vector<std::u16string>& expected) {
  ASSERT_EQ(api.FPDFDoc_GetAttachmentCount(doc), static_cast<int>(expected.size()));
  for (size_t i = 0; i < expected.size(); ++i) {
    auto attachment = api.FPDFDoc_GetAttachment(doc, static_cast<int>(i));
    ASSERT_NE(attachment, nullptr);
    ExpectBytes([&](void* b, unsigned long n) { return api.FPDFAttachment_GetName(attachment, static_cast<FPDF_WCHAR*>(b), n); }, Utf16(expected[i]));
  }
}
Bytes AttachmentTree(const std::string& limits, bool nested, bool indirect_names, bool odd_names) {
  std::vector<std::string> extra = {
      "<< /Type /Filespec /F (a) /UF (a) >>", "<< /Type /Filespec /F (c) /UF (c) >>"};
  std::string names = "[(a) 5 0 R (c) 6 0 R" + std::string(odd_names ? " (ignored-dangling-key)" : "") + "]";
  std::string leaf;
  if (indirect_names) { extra.push_back(names); leaf = "/Names 7 0 R"; }
  else leaf = "/Names " + names;
  leaf = "<< " + leaf + (limits.empty() ? "" : " /Limits " + limits) + " >>";
  std::string root = leaf;
  if (nested) {
    const int number = static_cast<int>(extra.size()) + 5;
    extra.push_back(leaf);
    root = "<< /Limits [(a) (c)] /Kids [" + std::to_string(number) + " 0 R] >>";
  }
  return runtime_test::RectangleFixture("/Names << /EmbeddedFiles " + root + " >>", "", extra);
}
void MutateAttachmentTree(const std::string& limits, bool nested, bool indirect, bool odd) {
  const auto input = AttachmentTree(limits, nested, indirect, odd);
  Artifacts artifacts;
  const auto label = "attachment-tree-" + limits + std::to_string(nested) + std::to_string(indirect) + std::to_string(odd);
  std::array<Raster, 2> originals;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(AttachmentNames(api, doc.get(), {u"a", u"c"}));
    // Missing bounds cannot locate a middle insertion. Append beyond the
    // known keys, then test the repaired bounds through deletion/reinsertion.
    const bool append = limits == "[]" || limits == "[(a)]";
    const auto name = Wide(append ? u"z" : u"b");
    const std::vector<std::u16string> expected = append
        ? std::vector<std::u16string>{u"a", u"c", u"z"}
        : std::vector<std::u16string>{u"a", u"b", u"c"};
    auto attachment = api.FPDFDoc_AddAttachment(doc.get(), name.data());
    ASSERT_NE(attachment, nullptr);
    const Bytes payload = {0, 1, 255, 'B'};
    ASSERT_TRUE(api.FPDFAttachment_SetFile(attachment, doc.get(), payload.data(), payload.size()));
    ASSERT_NO_FATAL_FAILURE(AttachmentNames(api, doc.get(), expected));
    originals[backend] = FirstPage(api, doc.get()); runtime_test::ExpectRectangle(originals[backend]);
    const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + label);
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(AttachmentNames(read, reopened.get(), expected));
      auto item = read.FPDFDoc_GetAttachment(reopened.get(), append ? 2 : 1);
      unsigned long size = 0;
      ASSERT_TRUE(read.FPDFAttachment_GetFile(item, nullptr, 0, &size)); ASSERT_EQ(size, payload.size());
      Bytes actual(size); ASSERT_TRUE(read.FPDFAttachment_GetFile(item, actual.data(), size, &size)); EXPECT_EQ(actual, payload);
      CompareRaster(originals[backend], FirstPage(read, reopened.get()), artifacts, writer->name() + label + reader->name());
      // Deleting boundary entries must shrink Limits through every ancestor,
      // including removing the last child from a Kids array.
      ASSERT_TRUE(read.FPDFDoc_DeleteAttachment(reopened.get(), 0));
      ASSERT_NO_FATAL_FAILURE(AttachmentNames(read, reopened.get(), {expected[1], expected[2]}));
      ASSERT_TRUE(read.FPDFDoc_DeleteAttachment(reopened.get(), 1));
      ASSERT_NO_FATAL_FAILURE(AttachmentNames(read, reopened.get(), {expected[1]}));
      ASSERT_TRUE(read.FPDFDoc_DeleteAttachment(reopened.get(), 0));
      ASSERT_NO_FATAL_FAILURE(AttachmentNames(read, reopened.get(), {}));
      const auto last = Wide(u"z");
      const auto replacement = read.FPDFDoc_AddAttachment(reopened.get(), last.data());
      if (nested && reader->name() == "native") {
        // Native removes the final leaf, leaving Kids[], then cannot find a
        // leaf for insertion. Rust's existing empty-root normalization must
        // preserve the useful public delete-all/reinsert behavior.
        EXPECT_EQ(replacement, nullptr);
        ASSERT_NO_FATAL_FAILURE(AttachmentNames(read, reopened.get(), {}));
      } else {
        ASSERT_NE(replacement, nullptr);
        ASSERT_NO_FATAL_FAILURE(AttachmentNames(read, reopened.get(), {u"z"}));
      }
    }
    ++backend;
  }
  CompareRaster(originals[0], originals[1], artifacts, label + "-engines");
}
TEST(CoverageParserAvailabilityRecovery, ReversedNameTreeLimitsAreNormalizedDuringMiddleInsertion) { MutateAttachmentTree("[(c) (a)]", false, false, false); }
TEST(CoverageParserAvailabilityRecovery, ExtraNameTreeLimitsAreTrimmedDuringMutation) { MutateAttachmentTree("[(a) (c) (unused) 42]", false, false, false); }
TEST(CoverageParserAvailabilityRecovery, EmptyNameTreeLimitsAreExtendedBeforeMutation) { MutateAttachmentTree("[]", false, false, false); }
TEST(CoverageParserAvailabilityRecovery, OneSidedNameTreeLimitsAreCompletedBeforeMutation) { MutateAttachmentTree("[(a)]", false, false, false); }
TEST(CoverageParserAvailabilityRecovery, NestedAttachmentDeletionRepairsAllAncestorLimits) { MutateAttachmentTree("[(a) (c)]", true, false, false); }
TEST(CoverageParserAvailabilityRecovery, IndirectNamesArraysRemainMutableAcrossBothWriters) { MutateAttachmentTree("[(a) (c)]", true, true, false); }
TEST(CoverageParserAvailabilityRecovery, DanglingNameTreeKeyDoesNotBecomeAnAttachment) { MutateAttachmentTree("[(a) (c)]", false, false, true); }

struct RecoveryTransport : FX_FILEAVAIL, FX_DOWNLOADHINTS {
  explicit RecoveryTransport(Bytes input) : bytes(std::move(input)), available(bytes.size(), false) {
    FX_FILEAVAIL::version = FX_DOWNLOADHINTS::version = 1;
    FX_FILEAVAIL::IsDataAvail = Has; FX_DOWNLOADHINTS::AddSegment = Hint;
    access.m_FileLen = static_cast<unsigned long>(bytes.size()); access.m_Param = this; access.m_GetBlock = Read;
  }
  static FPDF_BOOL Has(FX_FILEAVAIL* p, size_t offset, size_t count) noexcept {
    auto* self = static_cast<RecoveryTransport*>(p);
    if (offset > self->bytes.size() || count > self->bytes.size() - offset) return 0;
    return std::all_of(self->available.begin() + offset, self->available.begin() + offset + count, [](bool b) { return b; });
  }
  static void Hint(FX_DOWNLOADHINTS* p, size_t offset, size_t count) noexcept {
    auto* self = static_cast<RecoveryTransport*>(p);
    try { self->hints.emplace_back(offset, count); } catch (...) { self->valid = false; }
  }
  static int Read(void* p, unsigned long offset, unsigned char* output, unsigned long count) noexcept {
    auto* self = static_cast<RecoveryTransport*>(p);
    self->valid &= output && count && offset <= self->bytes.size() && count <= self->bytes.size() - offset;
    if (!self->valid || !Has(self, offset, count)) return 0;
    std::copy_n(self->bytes.data() + offset, count, output); ++self->reads; return 1;
  }
  void Deliver() {
    for (const auto& request : hints) {
      const size_t start = std::min(request.first, bytes.size());
      const size_t count = std::min(request.second, bytes.size() - start);
      std::fill(available.begin() + start, available.begin() + start + count, true);
      ++deliveries;
    }
    hints.clear();
  }
  void All() { std::fill(available.begin(), available.end(), true); }
  Bytes bytes; std::vector<bool> available; FPDF_FILEACCESS access{};
  std::vector<std::pair<size_t, size_t>> hints;
  size_t reads = 0, deliveries = 0; bool valid = true;
};
Bytes ChangedLinearized(const std::string& from, const std::string& to) {
  auto bytes = runtime_test::Resource("linearized.pdf");
  std::string text(bytes.begin(), bytes.end());
  const auto at = text.find(from);
  if (at == std::string::npos || from.size() != to.size()) throw std::runtime_error("Pinned linearized replacement changed");
  text.replace(at, from.size(), to);
  if (to == "/Prev 00000") {
    // Optional hints otherwise short-circuit the requested whole-file/xref
    // fallback. Remove only the H key, preserving every byte/object offset.
    const auto hints = text.find("/H [");
    if (hints == std::string::npos) throw std::runtime_error("Pinned linearized hints changed");
    text[hints + 1] = 'J';
  }
  return {text.begin(), text.end()};
}
void LinearizedRecovery(const std::string& from, const std::string& to, int expected) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto input = ChangedLinearized(from, to);
  Artifacts artifacts; std::array<std::vector<Raster>, 2> images; size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    RecoveryTransport source(input);
    Handle provider(api.FPDFAvail_Create(&source, &source.access), api.FPDFAvail_Destroy);
    ASSERT_NE(provider.get(), nullptr);
    int status = PDF_DATA_NOTAVAIL;
    for (int poll = 0; poll < 128 && status == PDF_DATA_NOTAVAIL; ++poll) {
      status = api.FPDFAvail_IsDocAvail(provider.get(), &source); source.Deliver();
    }
    ASSERT_EQ(status, PDF_DATA_AVAIL);
    EXPECT_EQ(api.FPDFAvail_IsLinearized(provider.get()), PDF_LINEARIZED);
    Handle doc(api.FPDFAvail_GetDocument(provider.get(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    EXPECT_EQ(api.FPDFAvail_GetFirstPageNum(doc.get()), 0);
    ASSERT_EQ(api.FPDF_GetPageCount(doc.get()), 3);
    int ready = PDF_DATA_NOTAVAIL;
    for (int poll = 0; poll < 128 && ready == PDF_DATA_NOTAVAIL; ++poll) {
      ready = api.FPDFAvail_IsPageAvail(provider.get(), 1, &source); source.Deliver();
    }
    EXPECT_EQ(ready, expected);
    EXPECT_TRUE(source.valid); EXPECT_GT(source.reads, 0u); EXPECT_GT(source.deliveries, 0u);
    if (ready == PDF_DATA_AVAIL) {
      source.All();
      for (int page = 0; page < 3; ++page) {
        SCOPED_TRACE(page);
        int page_ready = PDF_DATA_NOTAVAIL;
        for (int poll = 0; poll < 128 && page_ready == PDF_DATA_NOTAVAIL; ++poll) {
          page_ready = api.FPDFAvail_IsPageAvail(provider.get(), page, &source);
          source.Deliver();
        }
        ASSERT_EQ(page_ready, PDF_DATA_AVAIL);
        Handle loaded(api.FPDF_LoadPage(doc.get(), page), api.FPDF_ClosePage); ASSERT_NE(loaded.get(), nullptr);
        images[backend].push_back(Render(api, loaded.get(), 128, 96));
      }
      const auto saved = SaveDocument(api, doc.get(), artifacts, library->name() + "linearized-recovery");
      ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api();
        Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
        ASSERT_NE(reopened.get(), nullptr); ASSERT_EQ(read.FPDF_GetPageCount(reopened.get()), 3);
        for (int page = 0; page < 3; ++page) {
          Handle loaded(read.FPDF_LoadPage(reopened.get(), page), read.FPDF_ClosePage); ASSERT_NE(loaded.get(), nullptr);
          CompareRaster(images[backend][page], Render(read, loaded.get(), 128, 96), artifacts, library->name() + "linearized-to-" + reader->name() + std::to_string(page), {24, 0, 0.15});
        }
      }
    }
    ++backend;
  }
  ASSERT_EQ(images[0].size(), images[1].size());
  for (size_t i = 0; i < images[0].size(); ++i) CompareRaster(images[0][i], images[1][i], artifacts, "linearized-recovery-page" + std::to_string(i), {24, 0, 0.15});
}
TEST(CoverageParserAvailabilityRecovery, ZeroMainXrefPreviousOffsetRebuildsTheCompleteLinearizedPageTree) { LinearizedRecovery("/Prev 10700", "/Prev 00000", PDF_DATA_AVAIL); }
TEST(CoverageParserAvailabilityRecovery, NegativeMainXrefPreviousOffsetReportsDataErrorWithoutReadingOutsideTheFile) { LinearizedRecovery("/Prev 10700", "/Prev -0001", PDF_DATA_ERROR); }
TEST(CoverageParserAvailabilityRecovery, DamagedMainLinearizedXrefReportsDataErrorAfterDeliveringTheRequestedRange) { LinearizedRecovery("xref\r0 28", "zref\r0 28", PDF_DATA_ERROR); }

void UnsupportedEncryption(const std::string& dict, unsigned long expected_error = FPDF_ERR_PASSWORD) {
  auto pdf = Basic(); pdf.Object(5, dict); pdf.Xref("/Encrypt 5 0 R");
  const auto bytes = pdf.Data();
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    for (const char* password : {static_cast<const char*>(nullptr), "irrelevant"}) {
      Handle doc(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), password), api.FPDF_CloseDocument);
      EXPECT_EQ(doc.get(), nullptr);
      EXPECT_EQ(api.FPDF_GetLastError(), expected_error);
    }
  }
}
TEST(CoverageParserAvailabilityRecovery, UnknownSecurityHandlerFailsBeforePageParsing) { UnsupportedEncryption("<< /Filter /Unsupported /V 2 /R 3 >>", FPDF_ERR_SECURITY); }
TEST(CoverageParserAvailabilityRecovery, UnsupportedStandardEncryptionRevisionFailsWithoutAuthenticating) { UnsupportedEncryption("<< /Filter /Standard /V 99 /R 99 >>"); }
TEST(CoverageParserAvailabilityRecovery, MismatchedStreamAndStringCryptFiltersAreRejected) {
  UnsupportedEncryption("<< /Filter /Standard /V 4 /R 4 /Length 128 /StmF /One /StrF /Two /CF << /One << /CFM /AESV2 /Length 16 >> /Two << /CFM /V2 /Length 16 >> >> /O () /U () /P -4 >>");
}
TEST(CoverageParserAvailabilityRecovery, InvalidCryptFilterLengthFailsWithoutConstructingADecryptor) {
  UnsupportedEncryption("<< /Filter /Standard /V 4 /R 4 /Length 128 /StmF /One /StrF /One /CF << /One << /CFM /AESV2 /Length 15 >> >> /O () /U () /P -4 >>");
}
TEST(CoverageParserAvailabilityRecovery, ExplicitIdentityCryptFilterPermitsUnencryptedStringsAndStreams) {
  auto pdf = Basic(kPaint, "/Filter /Crypt /DecodeParms << /Name /Identity >>");
  // Identity is a document encryption filter, not a content operator. The
  // actual stream decoder receives its name through DecodeParms.
  pdf.Xref();
  ExerciseRecovery(pdf.Data(), "identity-crypt-stream");
}

}  // namespace
}  // namespace pdfoxium::integration
