#include "document_fixture.h"
#include "runtime_fixture.h"

#include <array>
#include <iomanip>
#include <map>
#include <sstream>
#include <utility>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using namespace document_test;
constexpr char kPaint[] = "0.25 0.5 0.75 rg 16 16 64 48 re f\n";

// Source-derived variations of CPDF_Parser::{ParseCrossRefTable,
// LoadCrossRefStream,FindAllCrossReferenceTablesAndStream,RebuildCrossRef}.
// Offsets are calculated from the actual serialized bytes, including updates.
struct Pdf {
  std::string data = "%PDF-1.7\n";
  std::map<int, std::pair<size_t, int>> objects;
  size_t previous = 0;
  void Object(int number, const std::string& body, int generation = 0) {
    objects[number] = {data.size(), generation};
    data += std::to_string(number) + " " + std::to_string(generation) +
        " obj\n" + body + "\nendobj\n";
  }
  std::string Entry(int number) const {
    std::ostringstream out;
    if (!number) return "0000000000 65535 f \n";
    const auto& entry = objects.at(number);
    out << std::setfill('0') << std::setw(10) << entry.first << ' '
        << std::setw(5) << entry.second << " n \n";
    return out.str();
  }
  void Classic(const std::vector<std::pair<int, int>>& sections,
               const std::string& trailer = "", bool chain = false,
               int root = 1) {
    const auto current = data.size();
    data += "xref\n";
    for (const auto& section : sections) {
      data += std::to_string(section.first) + " " + std::to_string(section.second) + "\n";
      for (int n = section.first; n < section.first + section.second; ++n)
        data += Entry(n);
    }
    data += "trailer << /Size " + std::to_string(objects.rbegin()->first + 1) +
        " /Root " + std::to_string(root) + " 0 R /Info 5 0 R " + trailer;
    if (chain) data += " /Prev " + std::to_string(previous);
    data += " >>\nstartxref\n" + std::to_string(current) + "\n%%EOF\n";
    previous = current;
  }
  Bytes BytesValue() const { return {data.begin(), data.end()}; }
};

Pdf Base(int generation = 0, bool inherited = false) {
  Pdf pdf;
  const auto ref = [&](int n) {
    return std::to_string(n) + " " + std::to_string(n == 2 || n == 3 || n == 4 ? generation : 0) + " R";
  };
  pdf.Object(1, "<< /Type /Catalog /Pages " + ref(2) + " >>");
  pdf.Object(2, "<< /Type /Pages /Count 1 /Kids [" + ref(3) + "] " +
      (inherited ? "/MediaBox [0 0 128 96] /Resources 6 0 R " : "") + ">>", generation);
  pdf.Object(3, "<< /Type /Page /Parent " + ref(2) +
      (inherited ? "" : " /MediaBox [0 0 128 96] /Resources << >>") +
      " /Contents " + ref(4) + " >>", generation);
  pdf.Object(4, PdfStream("", (inherited ? std::string("/GS gs\n") : std::string()) + kPaint), generation);
  pdf.Object(5, "<< /Title (Parser boundary) >>");
  if (inherited) pdf.Object(6, "<< /ExtGState << /GS 7 0 R >> >>");
  if (inherited) pdf.Object(7, "<< /Type /ExtGState /ca 1 /CA 1 >>");
  return pdf;
}

void Inspect(const Api& api, FPDF_DOCUMENT document, const std::u16string& title) {
  EXPECT_EQ(api.FPDF_GetPageCount(document), 1);
  EXPECT_EQ(api.FPDF_GetDocUserPermissions(document), 0xffffffffu);
  ExpectBytes([&](void* b, unsigned long n) {
    return api.FPDF_GetMetaText(document, "Title", b, n);
  }, Utf16(title));
  Handle page(api.FPDF_LoadPage(document, 0), api.FPDF_ClosePage);
  ASSERT_NE(page.get(), nullptr);
  EXPECT_FLOAT_EQ(api.FPDF_GetPageWidthF(page.get()), 128);
  EXPECT_FLOAT_EQ(api.FPDF_GetPageHeightF(page.get()), 96);
  ASSERT_EQ(api.FPDFPage_CountObjects(page.get()), 1);
  EXPECT_EQ(api.FPDFPageObj_GetType(api.FPDFPage_GetObject(page.get(), 0)), FPDF_PAGEOBJ_PATH);
}

void RoundTrip(const Bytes& input, const std::string& name,
               const std::u16string& title = u"Parser boundary", bool valid_xref = true) {
  Artifacts artifacts;
  artifacts.Pdf(name + "-input", input);
  std::array<Raster, 2> originals;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name() + "-" + name);
    const auto& api = writer->api();
    Handle document(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr) << api.FPDF_GetLastError();
    EXPECT_EQ(!!api.FPDF_DocumentHasValidCrossReferenceTable(document.get()), valid_xref);
    ASSERT_NO_FATAL_FAILURE(Inspect(api, document.get(), title));
    ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, document.get(), &originals[backend]));
    ASSERT_NO_FATAL_FAILURE(runtime_test::ExpectRectangle(originals[backend]));
    const auto language = Wide(u"en-NZ");
    ASSERT_TRUE(api.FPDFCatalog_SetLanguage(document.get(), language.data()));
    for (const auto flags : {FPDF_NO_INCREMENTAL, FPDF_INCREMENTAL}) {
      const auto label = writer->name() + "-" + name + "-flags" + std::to_string(flags);
      const auto saved = SaveDocument(api, document.get(), artifacts, label, flags);
      ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api();
        SCOPED_TRACE(label + "-to-" + reader->name());
        Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
        ASSERT_NE(reopened.get(), nullptr);
        ASSERT_NO_FATAL_FAILURE(Inspect(read, reopened.get(), title));
        ExpectBytes([&](void* b, unsigned long n) {
          return read.FPDFCatalog_GetLanguage(reopened.get(), static_cast<FPDF_WCHAR*>(b), n);
        }, Utf16(u"en-NZ"));
        Raster image;
        ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(read, reopened.get(), &image));
        ASSERT_NO_FATAL_FAILURE(runtime_test::ExpectRectangle(image));
        CompareRaster(originals[backend], image, artifacts, label + "-to-" + reader->name());
      }
    }
    ++backend;
  }
  CompareRaster(originals[0], originals[1], artifacts, name + "-native-rust");
}

// W[0]==0 implies normal objects; W[2]==0 implies generation zero. /Index
// segments are consumed consecutively from the byte stream, independently of
// object-number gaps. These legal encodings do not require repair.
Bytes StreamXref(bool omit_type, bool omit_generation, bool split_index, bool unknown_entry = false) {
  auto pdf = Base();
  pdf.objects[6] = {pdf.data.size(), 0};
  std::string records;
  for (int n = omit_type ? 1 : 0; n <= (unknown_entry ? 7 : 6); ++n) {
    if (!omit_type) records.push_back(n == 7 ? 3 : n ? 1 : 0);
    const uint32_t offset = n && n != 7 ? static_cast<uint32_t>(pdf.objects.at(n).first) : 0;
    for (int shift : {24, 16, 8, 0}) records.push_back(static_cast<char>(offset >> shift));
    if (!omit_generation) {
      records.push_back(n ? 0 : '\xff');
      records.push_back(n ? 0 : '\xff');
    }
  }
  std::string index = omit_type ? "/Index [1 6]" : split_index ? "/Index [0 2 2 2 4 3]" : "";
  const auto offset = pdf.data.size();
  pdf.Object(6, PdfStream("/Type /XRef /Size " + std::string(unknown_entry ? "8" : "7") + " /Root 1 0 R /Info 5 0 R /W [" +
      std::string(omit_type ? "0" : "1") + " 4 " + (omit_generation ? "0" : "2") + "] " + index, records));
  pdf.data += "startxref\n" + std::to_string(offset) + "\n%%EOF\n";
  return pdf.BytesValue();
}

TEST(CoverageParserRecoveryBoundaries, NonzeroGenerationsResolvePagesContentsAndInheritedBoxes) {
  auto pdf = Base(7, true);
  pdf.Classic({{0, 8}});
  RoundTrip(pdf.BytesValue(), "nonzero-generations");
}

TEST(CoverageParserRecoveryBoundaries, SplitClassicSubsectionsIncludeLegalEmptySubsections) {
  auto pdf = Base();
  pdf.Classic({{0, 2}, {2, 0}, {2, 2}, {4, 2}});
  RoundTrip(pdf.BytesValue(), "split-classic-zero-count");
}

TEST(CoverageParserRecoveryBoundaries, OmittedCrossReferenceStreamTypeDefaultsToNormalObjects) {
  RoundTrip(StreamXref(true, false, false), "xref-default-type");
}

TEST(CoverageParserRecoveryBoundaries, OmittedCrossReferenceStreamGenerationDefaultsToZero) {
  // Exclude object zero: a free entry's generation need not be represented.
  RoundTrip(StreamXref(true, true, false), "xref-default-generation");
}

TEST(CoverageParserRecoveryBoundaries, CrossReferenceStreamIndexSegmentsConsumeDisjointObjectRanges) {
  RoundTrip(StreamXref(false, false, true), "xref-index-segments");
}

TEST(CoverageParserRecoveryBoundaries, UnsupportedUnreferencedXrefEntryDoesNotHideTheHealthyPage) {
  // ISO32000 xref stream types beyond0/1/2 are ignored by PDFium's switch.
  // Only the unused object7 uses that unsupported type; all reachable page,
  // content and trailer objects retain independently checked normal entries.
  RoundTrip(StreamXref(false, false, false, true), "xref-unknown-unused-entry");
}

TEST(CoverageParserRecoveryBoundaries, IncrementalGenerationReplacementSelectsLatestPageContentAndInfo) {
  auto pdf = Base();
  pdf.Object(4, PdfStream("", "1 0 0 rg 16 16 64 48 re f\n"));
  pdf.Classic({{0, 6}});
  pdf.Object(3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents 4 1 R >>");
  pdf.Object(4, PdfStream("", kPaint), 1);
  pdf.Object(5, "<< /Title (Newest generation) >>");
  pdf.Classic({{3, 3}}, "", true);
  RoundTrip(pdf.BytesValue(), "incremental-generation", u"Newest generation");
}

TEST(CoverageParserRecoveryBoundaries, MultipleIncrementalRevisionsPreserveNewestDictionaryAndOldResources) {
  auto pdf = Base(0, true);
  pdf.Object(4, PdfStream("", "1 0 0 rg 16 16 64 48 re f\n"));
  pdf.Classic({{0, 8}});
  pdf.Object(5, "<< /Title (Intermediate revision) >>");
  pdf.Classic({{5, 1}}, "", true);
  pdf.Object(4, PdfStream("", std::string("/GS gs\n") + kPaint));
  pdf.Object(5, "<< /Title (Final revision) >>");
  pdf.Classic({{4, 2}}, "", true);
  RoundTrip(pdf.BytesValue(), "two-prev-revisions", u"Final revision");
}

TEST(CoverageParserRecoveryBoundaries, IncrementalCatalogReplacementRetainsOriginalPageTree) {
  auto pdf = Base();
  pdf.Classic({{0, 6}});
  pdf.Object(6, "<< /Type /Catalog /Pages 2 0 R /Lang (fr-CA) >>");
  pdf.Classic({{6, 1}}, "", true, 6);
  const auto input = pdf.BytesValue();
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    ExpectBytes([&](void* b, unsigned long n) {
      return api.FPDFCatalog_GetLanguage(doc.get(), static_cast<FPDF_WCHAR*>(b), n);
    }, Utf16(u"fr-CA"));
  }
  RoundTrip(input, "replacement-root");
}

TEST(CoverageParserRecoveryBoundaries, CyclicPreviousCrossReferenceIsRebuiltWithoutLosingThePage) {
  // Bounded analogue of FPDFViewEmbedderTest.CrossRefV4Loop: the latest
  // trailer refers to its own xref offset. Detection must trigger rebuild.
  auto pdf = Base();
  const auto xref = pdf.data.size();
  pdf.Classic({{0, 6}}, "/Prev " + std::to_string(xref));
  RoundTrip(pdf.BytesValue(), "self-prev-rebuild", u"Parser boundary", false);
}

TEST(CoverageParserRecoveryBoundaries, PinnedRebuildRetainsNonzeroGenerationFontsAndBothTextRuns) {
  // ParserTest.RebuildCrossRefCorrectly uses this original input. Its page/font
  // references have generations2/4/6/8 and its content has no Length entry.
  const auto input = runtime_test::Resource("parser_rebuildxref_correct.pdf");
  Artifacts artifacts;
  std::array<Raster, 2> images;
  size_t backend = 0;
  const auto inspect = [](const Api& api, FPDF_DOCUMENT doc) {
    EXPECT_EQ(api.FPDF_GetPageCount(doc), 1);
    Handle page(api.FPDF_LoadPage(doc, 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    EXPECT_FLOAT_EQ(api.FPDF_GetPageWidthF(page.get()), 200);
    EXPECT_FLOAT_EQ(api.FPDF_GetPageHeightF(page.get()), 200);
    Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
    ASSERT_NE(text.get(), nullptr);
    std::u16string actual;
    for (int n = 0; n < api.FPDFText_CountChars(text.get()); ++n) {
      const auto c = api.FPDFText_GetUnicode(text.get(), n);
      if (c != '\r' && c != '\n') actual.push_back(static_cast<char16_t>(c));
    }
    EXPECT_EQ(actual, u"Hello, world!Goodbye, world!");
  };
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    SCOPED_TRACE(writer->name());
    Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(inspect(api, doc.get()));
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    images[backend] = Render(api, page.get(), 200, 200);
    EXPECT_TRUE(std::any_of(images[backend].pixels.begin(), images[backend].pixels.end(), [](uint8_t b) { return b < 200; }));
    const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-pinned-rebuild");
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(inspect(read, reopened.get()));
      Handle loaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(loaded.get(), nullptr);
      CompareRaster(images[backend], Render(read, loaded.get(), 200, 200), artifacts,
                    writer->name() + "-pinned-rebuild-to-" + reader->name(), RasterTolerance{24, 0, 0.15});
    }
    ++backend;
  }
  CompareRaster(images[0], images[1], artifacts, "pinned-rebuild-native-rust", RasterTolerance{24, 0, 0.15});
}

struct Transport : FX_FILEAVAIL, FX_DOWNLOADHINTS {
  explicit Transport(Bytes input) : bytes(std::move(input)), arrived(bytes.size(), false) {
    FX_FILEAVAIL::version = 1;
    FX_DOWNLOADHINTS::version = 1;
    FX_FILEAVAIL::IsDataAvail = [](FX_FILEAVAIL* opaque, size_t offset, size_t count) -> FPDF_BOOL {
      auto& source = *static_cast<Transport*>(opaque);
      if (offset > source.bytes.size() || count > source.bytes.size() - offset) return 0;
      return std::all_of(source.arrived.begin() + offset, source.arrived.begin() + offset + count,
                         [](bool present) { return present; });
    };
    FX_DOWNLOADHINTS::AddSegment = [](FX_DOWNLOADHINTS* opaque, size_t offset, size_t count) {
      auto& source = *static_cast<Transport*>(opaque);
      try { source.requests.emplace_back(offset, count); } catch (...) { source.valid = false; }
    };
    access.m_FileLen = static_cast<unsigned long>(bytes.size());
    access.m_Param = this;
    access.m_GetBlock = [](void* opaque, unsigned long offset, unsigned char* output, unsigned long count) -> int {
      auto& source = *static_cast<Transport*>(opaque);
      ++source.reads;
      const bool bounded = output && count && offset <= source.bytes.size() && count <= source.bytes.size() - offset;
      source.valid &= bounded;
      if (!bounded || !source.IsDataAvail(&source, offset, count)) return 0;
      std::copy_n(source.bytes.data() + offset, count, output);
      return 1;
    };
  }
  void Deliver() {
    for (const auto& request : requests) {
      EXPECT_LE(request.first, bytes.size());
      const size_t begin = std::min(request.first, bytes.size());
      const size_t end = begin + std::min(request.second, bytes.size() - begin);
      size_t supplied = 0;
      for (size_t n = begin; n < end && supplied < 389; ++n) {
        if (!arrived[n]) { arrived[n] = true; ++supplied; ++delivered; }
      }
    }
    requests.clear();
  }
  Bytes bytes;
  std::vector<bool> arrived;
  std::vector<std::pair<size_t, size_t>> requests;
  FPDF_FILEACCESS access{};
  size_t reads = 0, delivered = 0;
  bool valid = true;
};

void Staged(bool rebuild) {
  auto pdf = Base(0, true);
  // Ensure transport windows don't collapse to a single already-cached file.
  pdf.data += "%" + std::string(16000, ' ') + "\n";
  const auto offset = pdf.data.size();
  pdf.Classic({{0, 8}}, rebuild ? "/Prev " + std::to_string(offset) : "");
  const auto input = pdf.BytesValue();
  Artifacts artifacts;
  std::array<Raster, 2> images;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    SCOPED_TRACE(writer->name());
    Transport source(input);
    Handle provider(api.FPDFAvail_Create(&source, &source.access), api.FPDFAvail_Destroy);
    ASSERT_NE(provider.get(), nullptr);
    int state = PDF_DATA_NOTAVAIL;
    for (int poll = 0; poll < 2048 && state == PDF_DATA_NOTAVAIL; ++poll) {
      state = api.FPDFAvail_IsDocAvail(provider.get(), &source);
      source.Deliver();
    }
    ASSERT_EQ(state, PDF_DATA_AVAIL);
    EXPECT_EQ(api.FPDFAvail_IsLinearized(provider.get()), PDF_NOT_LINEARIZED);
    Handle doc(api.FPDFAvail_GetDocument(provider.get(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    state = PDF_DATA_NOTAVAIL;
    for (int poll = 0; poll < 2048 && state == PDF_DATA_NOTAVAIL; ++poll) {
      state = api.FPDFAvail_IsPageAvail(provider.get(), 0, &source);
      source.Deliver();
    }
    ASSERT_EQ(state, PDF_DATA_AVAIL);
    EXPECT_EQ(!!api.FPDF_DocumentHasValidCrossReferenceTable(doc.get()), !rebuild);
    ASSERT_NO_FATAL_FAILURE(Inspect(api, doc.get(), u"Parser boundary"));
    ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, doc.get(), &images[backend]));
    ASSERT_NO_FATAL_FAILURE(runtime_test::ExpectRectangle(images[backend]));
    std::fill(source.arrived.begin(), source.arrived.end(), true);
    EXPECT_EQ(api.FPDFAvail_IsFormAvail(provider.get(), nullptr), PDF_FORM_NOTEXIST);
    const auto label = writer->name() + (rebuild ? "-staged-prev-repair" : "-staged-inherited");
    const auto saved = SaveDocument(api, doc.get(), artifacts, label);
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(Inspect(read, reopened.get(), u"Parser boundary"));
      Raster actual;
      ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(read, reopened.get(), &actual));
      ASSERT_NO_FATAL_FAILURE(runtime_test::ExpectRectangle(actual));
      CompareRaster(images[backend], actual, artifacts, label + "-to-" + reader->name());
    }
    EXPECT_TRUE(source.valid);
    EXPECT_GT(source.reads, 0u);
    EXPECT_GT(source.delivered, 0u);
    ++backend;
  }
  CompareRaster(images[0], images[1], artifacts, rebuild ? "staged-repair-readers" : "staged-inherited-readers");
}

TEST(CoverageParserRecoveryBoundaries, StagedCyclicXrefRecoveryLoadsAllRequiredSourceBytes) {
  // Corresponds to native CrossRefV4Loop's availability assertion, retaining
  // a visible scene and four writer/reader checks instead of only non-hanging.
  Staged(true);
}

TEST(CoverageParserRecoveryBoundaries, StagedInheritedResourcesAndInfoResolveBeforePageUse) {
  Staged(false);
}

TEST(CoverageParserRecoveryBoundaries, RebuiltEncryptedXrefKeepsPasswordSemanticsAndCanSaveWithoutSecurity) {
  // Native SetEncryptHandler must be restored after RebuildCrossRef. The
  // untouched pinned R5/R6 files provide independently known password pairs.
  Artifacts artifacts;
  for (int revision : {5, 6}) {
    const auto original = runtime_test::Resource("encrypted_hello_world_r" + std::to_string(revision) + ".pdf");
    std::string damaged(original.begin(), original.end());
    const auto marker = damaged.rfind("startxref");
    ASSERT_NE(marker, std::string::npos);
    damaged.resize(marker);
    damaged += "%%EOF\n";
    const Bytes input(damaged.begin(), damaged.end());
    std::array<Raster, 2> images;
    size_t backend = 0;
    for (const auto* writer : Implementations()) {
      const auto& api = writer->api();
      const auto label = writer->name() + "-encrypted-rebuild-r" + std::to_string(revision);
      SCOPED_TRACE(label);
      for (const char* password : {static_cast<const char*>(nullptr), "incorrect"}) {
        Handle denied(api.FPDF_LoadMemDocument64(input.data(), input.size(), password), api.FPDF_CloseDocument);
        EXPECT_EQ(denied.get(), nullptr);
        EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_PASSWORD));
      }
      for (const char* password : {"\xc3\xa2ge", "h\xc3\xb4tel"}) {
        Handle control(api.FPDF_LoadMemDocument64(original.data(), original.size(), password), api.FPDF_CloseDocument);
        Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), password), api.FPDF_CloseDocument);
        ASSERT_NE(control.get(), nullptr);
        ASSERT_NE(doc.get(), nullptr);
        EXPECT_EQ(api.FPDF_GetPageCount(doc.get()), 1);
        EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(doc.get()), revision);
        EXPECT_EQ(api.FPDF_GetDocUserPermissions(doc.get()), api.FPDF_GetDocUserPermissions(control.get()));
        EXPECT_EQ(api.FPDF_GetDocPermissions(doc.get()), api.FPDF_GetDocPermissions(control.get()));
        Raster expected;
        ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, control.get(), &expected));
        ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, doc.get(), &images[backend]));
        EXPECT_TRUE(std::any_of(images[backend].pixels.begin(), images[backend].pixels.end(), [](uint8_t c) { return c < 200; }));
        CompareRaster(expected, images[backend], artifacts, label + "-control-" + password);
        const auto saved = SaveDocument(api, doc.get(), artifacts, label + "-removed-" + password, FPDF_REMOVE_SECURITY);
        ASSERT_FALSE(saved.empty());
        for (const auto* reader : Implementations()) {
          const auto& read = reader->api();
          Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
          ASSERT_NE(reopened.get(), nullptr);
          EXPECT_EQ(read.FPDF_GetSecurityHandlerRevision(reopened.get()), -1);
          EXPECT_EQ(read.FPDF_GetDocUserPermissions(reopened.get()), 0xffffffffu);
          EXPECT_EQ(read.FPDF_GetPageCount(reopened.get()), 1);
          Raster actual;
          ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(read, reopened.get(), &actual));
          CompareRaster(images[backend], actual, artifacts, label + "-to-" + reader->name() + "-" + password, RasterTolerance{24, 0, 0.15});
        }
      }
      ++backend;
    }
    CompareRaster(images[0], images[1], artifacts, "encrypted-rebuild-r" + std::to_string(revision), RasterTolerance{24, 0, 0.15});
  }
}

}  // namespace
}  // namespace pdfoxium::integration
