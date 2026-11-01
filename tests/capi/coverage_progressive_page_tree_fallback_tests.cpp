#include "document_fixture.h"
#include "runtime_fixture.h"
#include "pinned_test_fonts.h"
#include "text_fixture.h"

#include <memory>

#include <array>
#include <string>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
// A nonpositive Count forces progressive document availability to walk all
// Kids before publishing a document. The ordinary memory-load control has an
// independently correct Count. Each leaf paints a different primary color.
Bytes PageTree(bool indirect_kids, int count) {
  const std::string kids = indirect_kids ? "9 0 R" : "[3 0 R 4 0 R 5 0 R]";
  std::vector<std::string> objects = {
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count " + std::to_string(count) + " /Kids " + kids + " >>"};
  for (int index = 0; index < 3; ++index) {
    objects.push_back("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents " + std::to_string(6 + index) + " 0 R >>");
  }
  for (const std::string& color : {std::string("1 0 0"), std::string("0 1 0"), std::string("0 0 1")}) {
    objects.push_back(PdfStream("", color + " rg 16 16 64 48 re f\n"));
  }
  objects.push_back("[3 0 R 4 0 R 5 0 R]");
  return PdfFixture(objects);
}
Bytes LinearizedSlowTree() {
  auto bytes = runtime_test::Resource("linearized.pdf");
  std::string pdf(bytes.begin(), bytes.end());
  for (const auto& change : {std::pair<std::string, std::string>{"/Count 3", "/Count 0"}, {"/H [", "/J ["}}) {
    const auto at = pdf.find(change.first);
    if (at == std::string::npos) throw std::runtime_error("Pinned linearized page-tree entry changed");
    pdf.replace(at, change.first.size(), change.second);
  }
  return {pdf.begin(), pdf.end()};
}
struct Transport : FX_FILEAVAIL, FX_DOWNLOADHINTS {
  explicit Transport(Bytes value) : bytes(std::move(value)), present(bytes.size(), false) {
    FX_FILEAVAIL::version = FX_DOWNLOADHINTS::version = 1;
    FX_FILEAVAIL::IsDataAvail = Has; FX_DOWNLOADHINTS::AddSegment = Request;
    access.m_FileLen = static_cast<unsigned long>(bytes.size()); access.m_Param = this; access.m_GetBlock = Read;
  }
  static FPDF_BOOL Has(FX_FILEAVAIL* opaque, size_t offset, size_t count) noexcept {
    const auto& self = *static_cast<Transport*>(opaque);
    if (offset > self.bytes.size() || count > self.bytes.size() - offset) return 0;
    return std::all_of(self.present.begin() + offset, self.present.begin() + offset + count, [](bool value) { return value; });
  }
  static void Request(FX_DOWNLOADHINTS* opaque, size_t offset, size_t count) noexcept {
    auto& self = *static_cast<Transport*>(opaque);
    try { self.requests.emplace_back(offset, count); } catch (...) { self.valid = false; }
  }
  static int Read(void* opaque, unsigned long offset, unsigned char* output, unsigned long count) noexcept {
    auto& self = *static_cast<Transport*>(opaque);
    self.valid &= output && count && offset <= self.bytes.size() && count <= self.bytes.size() - offset;
    if (!self.valid || !Has(&self, offset, count)) return 0;
    std::copy_n(self.bytes.data() + offset, count, output); ++self.reads; return 1;
  }
  void Deliver() {
    for (const auto& request : requests) {
      const auto offset = std::min(request.first, bytes.size());
      const auto count = std::min(request.second, bytes.size() - offset);
      std::fill(present.begin() + offset, present.begin() + offset + count, true); ++deliveries;
    }
    requests.clear();
  }
  Bytes bytes; std::vector<bool> present; FPDF_FILEACCESS access{};
  std::vector<std::pair<size_t, size_t>> requests; size_t deliveries = 0, reads = 0; bool valid = true;
};
Raster Image(const Api& api, FPDF_DOCUMENT doc, int page_number) {
  Handle page(api.FPDF_LoadPage(doc, page_number), api.FPDF_ClosePage);
  if (!page.get()) { ADD_FAILURE() << "Missing original page " << page_number; return {}; }
  return Render(api, page.get(), 128, 96);
}
void FallbackWorkflow(bool indirect_kids, bool linearized = false) {
  std::unique_ptr<ScopedPinnedTestFonts> fonts;
  if (linearized) fonts = std::make_unique<ScopedPinnedTestFonts>(/*enumerate_xfa=*/true);
  const auto original = linearized ? runtime_test::Resource("linearized.pdf") : PageTree(indirect_kids, 3);
  const auto input = linearized ? LinearizedSlowTree() : PageTree(indirect_kids, 0);
  Artifacts artifacts; artifacts.Pdf("progressive-slow-page-tree", input);
  std::array<std::array<Raster, 3>, 2> images; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    Handle control(api.FPDF_LoadMemDocument64(original.data(), original.size(), nullptr), api.FPDF_CloseDocument); ASSERT_NE(control.get(), nullptr); ASSERT_EQ(api.FPDF_GetPageCount(control.get()), 3);
    Transport source(input); Handle provider(api.FPDFAvail_Create(&source, &source.access), api.FPDFAvail_Destroy); ASSERT_NE(provider.get(), nullptr);
    int available = PDF_DATA_NOTAVAIL;
    for (int poll = 0; poll < 256 && available == PDF_DATA_NOTAVAIL; ++poll) { available = api.FPDFAvail_IsDocAvail(provider.get(), &source); source.Deliver(); }
    ASSERT_EQ(available, PDF_DATA_AVAIL); EXPECT_EQ(api.FPDFAvail_IsLinearized(provider.get()), linearized ? PDF_LINEARIZED : PDF_NOT_LINEARIZED);
    Handle doc(api.FPDFAvail_GetDocument(provider.get(), nullptr), api.FPDF_CloseDocument); ASSERT_NE(doc.get(), nullptr);
    ASSERT_EQ(api.FPDF_GetPageCount(doc.get()), 3); EXPECT_EQ(api.FPDFAvail_GetFirstPageNum(doc.get()), 0);
    // Probe last-to-first after IsDocAvail has completed the whole-tree walk.
    // Each differently colored leaf must retain its original page index.
    for (int number : {2, 1, 0}) {
      SCOPED_TRACE(number); int status = PDF_DATA_NOTAVAIL;
      for (int poll = 0; poll < 256 && status == PDF_DATA_NOTAVAIL; ++poll) { status = api.FPDFAvail_IsPageAvail(provider.get(), number, &source); source.Deliver(); }
      ASSERT_EQ(status, PDF_DATA_AVAIL); EXPECT_EQ(api.FPDFAvail_IsPageAvail(provider.get(), number, &source), PDF_DATA_AVAIL);
      images[backend][number] = Image(api, doc.get(), number);
      const auto& image = images[backend][number];
      ASSERT_EQ(image.width, 128); ASSERT_EQ(image.height, 96);
      ASSERT_EQ(image.pixels.size(), 128u * 96 * 4);
      const size_t inside = (48 * 128 + 48) * 4;
      if (!linearized) {
        for (int channel = 0; channel < 3; ++channel) {
          EXPECT_EQ(image.pixels[inside + channel], channel == 2 - number ? 255 : 0);
          EXPECT_EQ(image.pixels[(4 * 128 + 4) * 4 + channel], 255);
        }
      }
      CompareRaster(Image(api, control.get(), number), images[backend][number], artifacts, writer->name() + "-original-page" + std::to_string(number));
    }
    EXPECT_TRUE(source.valid); EXPECT_GT(source.reads, 0u); EXPECT_GT(source.deliveries, 0u);
    // Fully deliver before saving; availability rather than failed read policy
    // is the contract here. Keep the same live document and callback owners.
    std::fill(source.present.begin(), source.present.end(), true);
    const auto language = document_test::Wide(u"sv-SE"); ASSERT_TRUE(api.FPDFCatalog_SetLanguage(doc.get(), language.data()));
    for (FPDF_DWORD flags : {FPDF_NO_INCREMENTAL, FPDF_INCREMENTAL}) {
      const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-slow-tree-" + std::to_string(flags), flags); ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api(); SCOPED_TRACE(reader->name());
        Handle loaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument); ASSERT_NE(loaded.get(), nullptr); ASSERT_EQ(read.FPDF_GetPageCount(loaded.get()), 3);
        document_test::ExpectBytes([&](void* out, unsigned long n) { return read.FPDFCatalog_GetLanguage(loaded.get(), static_cast<FPDF_WCHAR*>(out), n); }, document_test::Utf16(u"sv-SE"));
        for (int number = 0; number < 3; ++number) {
          const auto raster = Image(read, loaded.get(), number); const auto label = writer->name() + "-" + reader->name() + "-saved-page" + std::to_string(number);
          if (!linearized || writer->name() == reader->name()) { CompareRaster(images[backend][number], raster, artifacts, label); }
          else { text_fixture::CompareTextRaster(images[backend][number], raster, artifacts, label); }
        }
      }
    }
    ++backend;
  }
  for (int page = 0; page < 3; ++page) {
    const auto label = "slow-tree-backends-page" + std::to_string(page);
    if (linearized) { text_fixture::CompareTextRaster(images[0][page], images[1][page], artifacts, label); }
    else { CompareRaster(images[0][page], images[1][page], artifacts, label); }
  }
}
TEST(CoverageProgressivePageTreeFallback, ZeroRootCountRequiresACompleteLeafWalkBeforeDocumentAvailability) { FallbackWorkflow(false); }
TEST(CoverageProgressivePageTreeFallback, IndirectKidsArrayIsExpandedDuringTheCompleteLeafWalk) { FallbackWorkflow(true); }
// Regression for LoadDocPages' false return after setting the complete-tree
// flag: CheckPage must stop its PageTree loop and let the next public probe
// enter LoadPages. A true return spins on the same node indefinitely.
TEST(CoverageProgressivePageTreeFallback, LinearizedZeroRootCountHandsOffWithoutSpinningDuringLastPageAvailability) { FallbackWorkflow(false, true); }
}  // namespace
}  // namespace pdfoxium::integration
