#include "runtime_fixture.h"

#include <numeric>
#include <sstream>

namespace pdfoxium::integration {
namespace {

struct SparseSource : FX_FILEAVAIL, FX_DOWNLOADHINTS {
  explicit SparseSource(std::vector<uint8_t> input, bool full = false)
      : bytes(std::move(input)), available(bytes.size(), full) {
    FX_FILEAVAIL::version = 1;
    FX_FILEAVAIL::IsDataAvail = IsAvailable;
    FX_DOWNLOADHINTS::version = 1;
    FX_DOWNLOADHINTS::AddSegment = Hint;
    access.m_FileLen = static_cast<unsigned long>(bytes.size());
    access.m_Param = this;
    access.m_GetBlock = Read;
  }
  static FPDF_BOOL IsAvailable(FX_FILEAVAIL* descriptor, size_t offset, size_t length) noexcept {
    auto* self = static_cast<SparseSource*>(descriptor);
    if (offset > self->bytes.size() || length > self->bytes.size() - offset) return 0;
    ++self->availability_calls;
    return std::all_of(self->available.begin() + offset, self->available.begin() + offset + length, [](bool value) { return value; });
  }
  static void Hint(FX_DOWNLOADHINTS* descriptor, size_t offset, size_t length) noexcept {
    auto* self = static_cast<SparseSource*>(descriptor);
    try { self->hints.emplace_back(offset, length); } catch (...) { self->valid = false; }
  }
  static int Read(void* opaque, unsigned long offset, unsigned char* buffer, unsigned long length) noexcept {
    auto* self = static_cast<SparseSource*>(opaque);
    ++self->reads;
    const bool valid = buffer && length && offset <= self->bytes.size() && length <= self->bytes.size() - offset;
    self->valid &= valid;
    if (!valid || self->reject_reads || !IsAvailable(static_cast<FX_FILEAVAIL*>(self), offset, length)) return 0;
    std::copy_n(self->bytes.data() + offset, length, buffer);
    return 1;
  }
  void DeliverHints() {
    for (const auto& hint : hints) {
      EXPECT_LE(hint.first, bytes.size());
      const size_t start = std::min(hint.first, bytes.size());
      const size_t length = std::min(hint.second, bytes.size() - start);
      std::fill(available.begin() + start, available.begin() + start + length, true);
      history.push_back(hint);
    }
    hints.clear();
  }
  void DeliverAll() { std::fill(available.begin(), available.end(), true); }
  std::vector<uint8_t> bytes;
  std::vector<bool> available;
  FPDF_FILEACCESS access{};
  bool valid = true;
  bool reject_reads = false;
  size_t reads = 0;
  size_t availability_calls = 0;
  std::vector<std::pair<size_t, size_t>> hints;
  std::vector<std::pair<size_t, size_t>> history;
};

std::vector<uint8_t> NonzeroFirstPageFixture() {
  // Reorder an existing three-page linearized fixture so its first available
  // page is page 1. The page-object and byte offsets remain unchanged: /P 1
  // consumes four bytes from existing header padding, and the two Kids swap
  // has the same length. /O still identifies the same first available object.
  auto bytes = runtime_test::Resource("linearized.pdf");
  std::string text(bytes.begin(), bytes.end());
  const std::string kids = "/Kids[31 0 R 1 0 R 8 0 R]";
  const size_t where = text.find(kids);
  if (where == std::string::npos) throw std::runtime_error("Pinned linearized page tree changed");
  text.replace(where, kids.size(), "/Kids[1 0 R 31 0 R 8 0 R]");
  const size_t end = text.find(">>", text.find("/Linearized"));
  text.insert(end, "/P 1");
  const size_t padding = text.find("    ", end + 4);
  if (padding == std::string::npos) throw std::runtime_error("Pinned linearized header padding changed");
  text.erase(padding, 4);
  return {text.begin(), text.end()};
}

TEST(Availability, SparseDeliveryLoadsLinearizedOrdinaryAndNonzeroFirstPages) {
  Artifacts artifacts;
  const std::array<std::vector<uint8_t>, 4> fixtures = {
      runtime_test::RectangleFixture(), runtime_test::Resource("linearized.pdf"),
      runtime_test::Resource("feature_linearized_loading.pdf"), NonzeroFirstPageFixture()};
  for (size_t kind = 0; kind < fixtures.size(); ++kind) {
    std::array<std::vector<Raster>, 2> observations;
    size_t backend = 0;
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name() + "-fixture" + std::to_string(kind));
      const auto& api = library->api();
      SparseSource source(fixtures[kind]);
      Handle provider(api.FPDFAvail_Create(&source, &source.access), api.FPDFAvail_Destroy);
      ASSERT_NE(provider.get(), nullptr);
      EXPECT_EQ(api.FPDFAvail_IsLinearized(provider.get()), PDF_LINEARIZATION_UNKNOWN);
      int status = api.FPDFAvail_IsDocAvail(provider.get(), &source);
      EXPECT_EQ(status, PDF_DATA_NOTAVAIL);
      for (int iteration = 0; iteration < 100 && status == PDF_DATA_NOTAVAIL; ++iteration) {
        ASSERT_FALSE(source.hints.empty()) << "No progress from unavailable document";
        source.DeliverHints();
        status = api.FPDFAvail_IsDocAvail(provider.get(), &source);
      }
      ASSERT_EQ(status, PDF_DATA_AVAIL);
      Handle document(api.FPDFAvail_GetDocument(provider.get(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      const int first = api.FPDFAvail_GetFirstPageNum(document.get());
      EXPECT_EQ(first, kind == 3 ? 1 : 0);
      EXPECT_EQ(api.FPDFAvail_IsLinearized(provider.get()), kind ? PDF_LINEARIZED : PDF_NOT_LINEARIZED);
      int form_status = api.FPDFAvail_IsFormAvail(provider.get(), &source);
      for (int iteration = 0; iteration < 100 && form_status == PDF_FORM_NOTAVAIL; ++iteration) {
        ASSERT_FALSE(source.hints.empty());
        source.DeliverHints();
        form_status = api.FPDFAvail_IsFormAvail(provider.get(), &source);
      }
      ASSERT_TRUE(form_status == PDF_FORM_AVAIL || form_status == PDF_FORM_NOTEXIST);
      const int count = api.FPDF_GetPageCount(document.get());
      ASSERT_GT(count, 0);
      for (int page_index = 0; page_index < count; ++page_index) {
        int page_status = api.FPDFAvail_IsPageAvail(provider.get(), page_index, &source);
        for (int iteration = 0; iteration < 100 && page_status == PDF_DATA_NOTAVAIL; ++iteration) {
          ASSERT_FALSE(source.hints.empty());
          source.DeliverHints();
          page_status = api.FPDFAvail_IsPageAvail(provider.get(), page_index, &source);
        }
        ASSERT_EQ(page_status, PDF_DATA_AVAIL);
        EXPECT_EQ(api.FPDFAvail_IsPageAvail(provider.get(), page_index, nullptr), PDF_DATA_AVAIL);
        Handle page(api.FPDF_LoadPage(document.get(), page_index), api.FPDF_ClosePage);
        ASSERT_NE(page.get(), nullptr);
        observations[backend].push_back(Render(api, page.get(), 128, 96));
      }
      // Form readiness does not imply that the Info dictionary has arrived.
      // Deliver the remaining source bytes before comparing metadata: PDFium
      // legitimately reports an absent Title while its bytes remain withheld.
      source.DeliverAll();
      for (int page_index = 0; page_index < api.FPDF_GetPageCount(document.get()); ++page_index) {
        ASSERT_EQ(api.FPDFAvail_IsPageAvail(provider.get(), page_index, &source), PDF_DATA_AVAIL);
      }
      // Metadata is queried only after incremental form dependencies are ready.
      // Compare to the same pinned bytes through the ordinary fully loaded API.
      Handle ordinary(api.FPDF_LoadMemDocument64(source.bytes.data(), source.bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(ordinary.get(), nullptr);
      const auto metadata_size = api.FPDF_GetMetaText(document.get(), "Title", nullptr, 0);
      EXPECT_EQ(metadata_size, api.FPDF_GetMetaText(ordinary.get(), "Title", nullptr, 0));
      if (metadata_size) {
        ASSERT_EQ(metadata_size % 2, 0u);
        std::vector<unsigned short> actual(metadata_size / 2, 0xa5a5), expected(metadata_size / 2, 0x5a5a);
        EXPECT_EQ(api.FPDF_GetMetaText(document.get(), "Title", actual.data(), metadata_size), metadata_size);
        EXPECT_EQ(api.FPDF_GetMetaText(ordinary.get(), "Title", expected.data(), metadata_size), metadata_size);
        EXPECT_EQ(actual, expected);
        EXPECT_EQ(actual.back(), 0);
      }
      EXPECT_EQ(api.FPDFAvail_IsPageAvail(provider.get(), -1, nullptr), PDF_DATA_NOTAVAIL);
      // Availability describes underlying data readiness, not index validity.
      // The loader must still reject a nonexistent page even when all bytes
      // have already arrived and availability can therefore report AVAIL.
      const int beyond = api.FPDFAvail_IsPageAvail(provider.get(), count, nullptr);
      EXPECT_TRUE(beyond == PDF_DATA_ERROR || beyond == PDF_DATA_NOTAVAIL || beyond == PDF_DATA_AVAIL);
      EXPECT_EQ(api.FPDF_LoadPage(document.get(), count), nullptr);
      EXPECT_TRUE(source.valid);
      EXPECT_GT(source.reads, 0u);
      EXPECT_GT(source.availability_calls, 0u);
      EXPECT_FALSE(source.history.empty());
      std::ostringstream ranges;
      for (const auto& range : source.history) ranges << range.first << ':' << range.second << '\n';
      artifacts.Text(library->name() + "-ranges" + std::to_string(kind), ranges.str());
      ++backend;
    }
    ASSERT_EQ(observations[0].size(), observations[1].size());
    for (size_t index = 0; index < observations[0].size(); ++index) CompareRaster(observations[0][index], observations[1][index], artifacts, "available" + std::to_string(kind) + "-page" + std::to_string(index),
        kind == 2 ? RasterTolerance{10, 0, 0.025} : RasterTolerance{0, 0, 0});
  }
}

TEST(Availability, PasswordAndFormsUseReadyProvidersAndKeepCallbackOwnersAlive) {
  const auto encrypted = runtime_test::Resource("encrypted.pdf");
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    for (const char* password : {"incorrect", "1234", "5678"}) {
      SparseSource source(encrypted, true);
      Handle provider(api.FPDFAvail_Create(&source, &source.access), api.FPDFAvail_Destroy);
      ASSERT_NE(provider.get(), nullptr);
      ASSERT_EQ(api.FPDFAvail_IsDocAvail(provider.get(), nullptr), PDF_DATA_AVAIL);
      Handle document(api.FPDFAvail_GetDocument(provider.get(), password), api.FPDF_CloseDocument);
      if (std::string(password) == "incorrect") {
        EXPECT_EQ(document.get(), nullptr);
        EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_PASSWORD));
      } else {
        ASSERT_NE(document.get(), nullptr);
        EXPECT_GT(api.FPDF_GetPageCount(document.get()), 0);
        EXPECT_EQ(api.FPDFAvail_IsPageAvail(provider.get(), 0, &source), PDF_DATA_AVAIL);
        Raster raster;
        ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, document.get(), &raster));
        EXPECT_FALSE(raster.pixels.empty());
      }
    }
    for (bool with_form : {false, true}) {
      const auto input = with_form ? runtime_test::RectangleFixture("/AcroForm 6 0 R", "/Annots [5 0 R]", {
          "<< /Type /Annot /Subtype /Widget /FT /Tx /T (ready) /V (Ready) /Rect [8 8 80 24] /P 3 0 R >>",
          "<< /Fields [5 0 R] /DA (/F1 10 Tf 0 g) /DR << /Font << /F1 7 0 R >> >> >>",
          "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"}) : runtime_test::RectangleFixture();
      SparseSource source(input, true);
      Handle provider(api.FPDFAvail_Create(&source, &source.access), api.FPDFAvail_Destroy);
      ASSERT_NE(provider.get(), nullptr);
      ASSERT_EQ(api.FPDFAvail_IsDocAvail(provider.get(), &source), PDF_DATA_AVAIL);
      Handle document(api.FPDFAvail_GetDocument(provider.get(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      const int ready = api.FPDFAvail_IsFormAvail(provider.get(), &source);
      EXPECT_EQ(ready, with_form ? PDF_FORM_AVAIL : PDF_FORM_NOTEXIST);
      EXPECT_EQ(api.FPDFAvail_IsFormAvail(provider.get(), nullptr), ready);
      if (with_form) {
        FPDF_FORMFILLINFO info{}; info.version = 2;
        Handle form(api.FPDFDOC_InitFormFillEnvironment(document.get(), &info), api.FPDFDOC_ExitFormFillEnvironment);
        ASSERT_NE(form.get(), nullptr);
        Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
        ASSERT_NE(page.get(), nullptr);
        api.FORM_OnAfterLoadPage(page.get(), form.get());
        Handle annotation(api.FPDFPage_GetAnnot(page.get(), 0), api.FPDFPage_CloseAnnot);
        ASSERT_NE(annotation.get(), nullptr);
        EXPECT_EQ(api.FPDFAnnot_GetFormFieldType(form.get(), annotation.get()), FPDF_FORMFIELD_TEXTFIELD);
        EXPECT_EQ(api.FPDFAnnot_GetFormFieldValue(form.get(), annotation.get(), nullptr, 0), 12u);
        api.FORM_OnBeforeClosePage(page.get(), form.get());
      }
    }
  }
}

TEST(Availability, MalformedAndNullQueriesReportErrorsWithoutInvalidInterfaces) {
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    SparseSource source(std::vector<uint8_t>(2048, 'x'), true);
    Handle provider(api.FPDFAvail_Create(&source, &source.access), api.FPDFAvail_Destroy);
    ASSERT_NE(provider.get(), nullptr);
    // A non-linearized byte source can be fully available without being a
    // valid PDF. Parsing validity is established by GetDocument afterwards.
    EXPECT_EQ(api.FPDFAvail_IsDocAvail(provider.get(), &source), PDF_DATA_AVAIL);
    EXPECT_EQ(api.FPDFAvail_GetDocument(provider.get(), nullptr), nullptr);
    EXPECT_EQ(api.FPDF_GetLastError(), static_cast<unsigned long>(FPDF_ERR_FORMAT));
    SparseSource empty(std::vector<uint8_t>{}, true);
    Handle empty_provider(api.FPDFAvail_Create(&empty, &empty.access), api.FPDFAvail_Destroy);
    ASSERT_NE(empty_provider.get(), nullptr);
    EXPECT_EQ(api.FPDFAvail_IsDocAvail(empty_provider.get(), nullptr), PDF_DATA_ERROR);
    EXPECT_EQ(api.FPDFAvail_IsDocAvail(nullptr, nullptr), PDF_DATA_ERROR);
    EXPECT_EQ(api.FPDFAvail_GetDocument(nullptr, nullptr), nullptr);
    EXPECT_EQ(api.FPDFAvail_GetFirstPageNum(nullptr), 0);
    EXPECT_EQ(api.FPDFAvail_IsPageAvail(nullptr, 0, nullptr), PDF_DATA_ERROR);
    EXPECT_EQ(api.FPDFAvail_IsFormAvail(nullptr, nullptr), PDF_FORM_ERROR);
    EXPECT_EQ(api.FPDFAvail_IsLinearized(nullptr), PDF_LINEARIZATION_UNKNOWN);
  }
}

TEST(Availability, LinearizedFormDependenciesDistinguishWaitingFromReadFailure) {
  const auto input = runtime_test::Resource("linearized.pdf");
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    for (bool read_failure : {false, true}) {
      SCOPED_TRACE(library->name() + (read_failure ? "-read-error" : "-pending"));
      SparseSource source(input);
      Handle provider(api.FPDFAvail_Create(&source, &source.access), api.FPDFAvail_Destroy);
      ASSERT_NE(provider.get(), nullptr);
      int status = api.FPDFAvail_IsDocAvail(provider.get(), &source);
      for (int attempt = 0; status == PDF_DATA_NOTAVAIL && attempt < 100; ++attempt) {
        ASSERT_FALSE(source.hints.empty());
        source.DeliverHints();
        status = api.FPDFAvail_IsDocAvail(provider.get(), &source);
      }
      ASSERT_EQ(status, PDF_DATA_AVAIL);
      Handle document(api.FPDFAvail_GetDocument(provider.get(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      const int pending = api.FPDFAvail_IsFormAvail(provider.get(), &source);
      ASSERT_EQ(pending, PDF_FORM_NOTAVAIL);
      ASSERT_FALSE(source.hints.empty());
      if (read_failure) {
        // The transport advertises the requested ranges, but cannot read
        // their bytes (for example a truncated backing file).
        source.DeliverAll();
        source.reject_reads = true;
        EXPECT_EQ(api.FPDFAvail_IsFormAvail(provider.get(), nullptr), PDF_FORM_ERROR);
      } else {
        int ready = pending;
        for (int attempt = 0; ready == PDF_FORM_NOTAVAIL && attempt < 100; ++attempt) {
          source.DeliverHints();
          ready = api.FPDFAvail_IsFormAvail(provider.get(), &source);
        }
        EXPECT_EQ(ready, PDF_FORM_AVAIL);
        EXPECT_EQ(api.FPDFAvail_IsFormAvail(provider.get(), nullptr), PDF_FORM_AVAIL);
      }
      EXPECT_TRUE(source.valid);
    }
  }
}

}  // namespace
}  // namespace pdfoxium::integration
