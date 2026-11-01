// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "document_fixture.h"
#include "runtime_fixture.h"

#include <algorithm>
#include <array>
#include <iomanip>
#include <sstream>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using RGB = std::array<int, 3>;
enum class Order { Forward, Reverse, Alternating };
struct Spec {
  std::string name;
  Order order = Order::Forward;
  bool middle = false, leaf = false, shared_leaf = false, nested = false;
  bool prefetch = false, missing = false, parent_cycle = false, no_resources = false;
  int cycle = 0;
  bool fail_read = false;
};
struct Pdf {
  Bytes bytes;
  std::vector<std::pair<size_t, size_t>> spans;
};
Pdf Fixture(const Spec& s) {
  const std::string cycle = s.cycle ? "/Properties << /Probe 15 0 R >> " :
      s.missing ? "/Properties << /Probe 99 0 R >> " : "";
  const std::string common = "/XObject << /Fm 10 0 R >> /ExtGState << /G 11 0 R >> ";
  std::vector<std::string> objects{
      "<< /Type /Catalog /Pages 2 0 R >>",
      std::string("<< /Type /Pages /Count 2 /Kids [8 0 R 4 0 R] ") +
          (s.no_resources ? "" : "/Resources 5 0 R ") + ">>",
      std::string("<< /Type /Page /Parent 8 0 R /MediaBox [0 0 96 96] /Contents 9 0 R ") +
          (s.leaf ? "/Resources 7 0 R " : s.shared_leaf ? "/Resources 5 0 R " : "") + ">>",
      std::string("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Contents 17 0 R ") +
          (s.no_resources || s.shared_leaf ? "/Resources 5 0 R " : "") + ">>",
      "<< " + common + cycle + ">>",
      "<< " + common + cycle + ">>",
      "<< /XObject << /Fm 14 0 R >> /ExtGState << /G 11 0 R >> >>",
      "<< /Type /Pages /Parent " + std::string(s.parent_cycle ? "8" : "2") +
          " 0 R /Kids [3 0 R] /Count 1 " + (s.middle ? "/Resources 6 0 R " : "") + ">>",
      PdfStream("", "q /G gs 64 0 0 48 16 16 cm /Fm Do Q 0 0 1 rg 86 86 4 4 re f"),
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources 13 0 R",
                s.nested ? "/Inner Do" : "1 0 0 rg 0 0 1 1 re f"),
      "<< /Type /ExtGState /ca 1 /CA 1 >>",
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources 13 0 R",
                "1 0 0 rg 0 0 1 1 re f"),
      "<< /XObject << /Inner 12 0 R /UnusedSelf 10 0 R >> >>",
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << >>",
                "0 0 1 rg 0 0 1 1 re f"),
      s.cycle == 2 ? "<< /Other 16 0 R >>" : "[16 0 R]",
      s.cycle == 2 ? "<< /Other 15 0 R >>" : "[15 0 R]",
      PdfStream("", "q /G gs 64 0 0 48 16 16 cm /Fm Do Q 0 1 0 rg 86 86 4 4 re f")};
  // Keep each independently requested object in a separate transport region.
  // Whitespace between indirect objects is legal PDF syntax, not stream data.
  std::string pdf = "%PDF-1.7\n";
  std::vector<size_t> offsets{0};
  Pdf result;
  for (size_t i = 0; i < objects.size(); ++i) {
    pdf += std::string(4096, ' ') + "\n";
    offsets.push_back(pdf.size());
    const size_t begin = pdf.size();
    pdf += std::to_string(i + 1) + " 0 obj\n" + objects[i] + "\nendobj\n";
    result.spans.emplace_back(begin, pdf.size() - begin);
  }
  const size_t xref = pdf.size();
  pdf += "xref\n0 " + std::to_string(objects.size() + 1) + "\n0000000000 65535 f \n";
  for (size_t i = 1; i < offsets.size(); ++i) {
    std::ostringstream line;
    line << std::setw(10) << std::setfill('0') << offsets[i] << " 00000 n \n";
    pdf += line.str();
  }
  pdf += "trailer\n<< /Size " + std::to_string(objects.size() + 1) +
      " /Root 1 0 R >>\nstartxref\n" + std::to_string(xref) + "\n%%EOF\n";
  result.bytes.assign(pdf.begin(), pdf.end());
  return result;
}
struct Transport : FX_FILEAVAIL, FX_DOWNLOADHINTS {
  Transport(const Pdf& pdf, const Spec& spec) : bytes(pdf.bytes), present(bytes.size(), false), order(spec.order) {
    FX_FILEAVAIL::version = FX_DOWNLOADHINTS::version = 1;
    FX_FILEAVAIL::IsDataAvail = Has; FX_DOWNLOADHINTS::AddSegment = Hint;
    access.m_FileLen = static_cast<unsigned long>(bytes.size());
    access.m_Param = this; access.m_GetBlock = Read;
    if (spec.prefetch) {
      for (int object : {5, 6, 10, 11, 12, 13}) {
        const auto span = pdf.spans[object - 1];
        std::fill(present.begin() + span.first, present.begin() + span.first + span.second, true);
      }
    }
  }
  static FPDF_BOOL Has(FX_FILEAVAIL* opaque, size_t offset, size_t count) noexcept {
    auto& self = *static_cast<Transport*>(opaque);
    if (offset > self.bytes.size() || count > self.bytes.size() - offset) return 0;
    return std::all_of(self.present.begin() + offset, self.present.begin() + offset + count, [](bool b) { return b; });
  }
  static void Hint(FX_DOWNLOADHINTS* opaque, size_t offset, size_t count) noexcept {
    auto& self = *static_cast<Transport*>(opaque);
    if (offset > self.bytes.size() || count > self.bytes.size() - offset || !count) {
      self.valid = false; return;
    }
    try { self.requests.emplace_back(offset, count); ++self.hints; }
    catch (...) { self.valid = false; }
  }
  static int Read(void* opaque, unsigned long offset, unsigned char* output, unsigned long count) noexcept {
    auto& self = *static_cast<Transport*>(opaque);
    self.valid &= offset <= self.bytes.size() && count <= self.bytes.size() - offset && (!count || output);
    ++self.reads;
    if (!self.valid) return 0;
    if (self.fail_next) { self.fail_next = false; ++self.failed_reads; return 0; }
    if (!Has(&self, offset, count)) { ++self.unavailable_reads; return 0; }
    if (count) std::copy_n(self.bytes.data() + offset, count, output);
    return 1;
  }
  void Deliver() {
    const bool reverse = order == Order::Reverse || (order == Order::Alternating && deliveries % 2);
    if (reverse) std::reverse(requests.begin(), requests.end());
    size_t budget = 2048;
    for (auto [offset, count] : requests) {
      if (!budget) break;
      if (reverse) {
        for (size_t p = offset + count; p > offset && budget;) {
          --p; if (!present[p]) { present[p] = true; --budget; }
        }
      } else {
        for (size_t p = offset; p < offset + count && budget; ++p) {
          if (!present[p]) { present[p] = true; --budget; }
        }
      }
    }
    deliveries += budget != 2048;
    requests.clear();
  }
  void DeliverAll() { std::fill(present.begin(), present.end(), true); requests.clear(); }
  Bytes bytes; std::vector<bool> present; Order order; FPDF_FILEACCESS access{};
  std::vector<std::pair<size_t, size_t>> requests;
  size_t hints = 0, reads = 0, deliveries = 0, failed_reads = 0, unavailable_reads = 0;
  bool valid = true, fail_next = false;
};
Raster Paint(const Api& api, FPDF_DOCUMENT doc, int number) {
  Handle page(api.FPDF_LoadPage(doc, number), api.FPDF_ClosePage);
  if (!page.get()) { ADD_FAILURE() << "Missing resource page " << number; return {}; }
  return Render(api, page.get(), 96, 96, FPDF_RENDER_NO_SMOOTHIMAGE | FPDF_RENDER_NO_SMOOTHPATH);
}
void Pixel(const Raster& image, int x, int y, RGB color) {
  ASSERT_EQ(image.width, 96); ASSERT_EQ(image.height, 96);
  const size_t at = (size_t(95 - y) * 96 + x) * 4;
  EXPECT_EQ(image.pixels.at(at), color[2]); EXPECT_EQ(image.pixels.at(at + 1), color[1]);
  EXPECT_EQ(image.pixels.at(at + 2), color[0]); EXPECT_EQ(image.pixels.at(at + 3), 255);
}
void Inspect(const Raster& image, int page, const Spec& s) {
  Pixel(image, 30, 30, page == 0 && s.no_resources ? RGB{255,255,255} :
        page == 0 && s.leaf ? RGB{0,0,255} : RGB{255,0,0});
  Pixel(image, 87, 87, page == 0 ? RGB{0,0,255} : RGB{0,255,0});
  Pixel(image, 5, 5, {255,255,255}); Pixel(image, 85, 30, {255,255,255});
}
void Workflow(const Spec& spec) {
  const auto pdf = Fixture(spec); Artifacts artifacts;
  std::array<std::array<Raster,2>,2> originals; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(spec.name + " writer=" + writer->name()); const auto& api = writer->api();
    Transport source(pdf,spec);
    Handle provider(api.FPDFAvail_Create(&source,&source.access),api.FPDFAvail_Destroy);
    ASSERT_NE(provider.get(),nullptr);
    int status = PDF_DATA_NOTAVAIL;
    for (int poll=0;poll<1024 && status==PDF_DATA_NOTAVAIL;++poll) {
      status=api.FPDFAvail_IsDocAvail(provider.get(),&source); source.Deliver();
    }
    ASSERT_EQ(status,PDF_DATA_AVAIL); EXPECT_EQ(api.FPDFAvail_IsLinearized(provider.get()),PDF_NOT_LINEARIZED);
    if (spec.fail_read) {
      source.DeliverAll(); source.fail_next=true;
      Handle rejected(api.FPDFAvail_GetDocument(provider.get(),nullptr),api.FPDF_CloseDocument);
      EXPECT_EQ(rejected.get(),nullptr); EXPECT_EQ(source.failed_reads,1u);
    }
    Handle document(api.FPDFAvail_GetDocument(provider.get(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr); ASSERT_EQ(api.FPDF_GetPageCount(document.get()),2);
    for (int page : {1,0,1}) {
      status=PDF_DATA_NOTAVAIL;
      for (int poll=0;poll<1024 && status==PDF_DATA_NOTAVAIL;++poll) {
        status=api.FPDFAvail_IsPageAvail(provider.get(),page,&source); source.Deliver();
      }
      ASSERT_EQ(status,PDF_DATA_AVAIL);
      EXPECT_EQ(api.FPDFAvail_IsPageAvail(provider.get(),page,&source),PDF_DATA_AVAIL);
      originals[backend][page]=Paint(api,document.get(),page);
      ASSERT_NO_FATAL_FAILURE(Inspect(originals[backend][page],page,spec));
    }
    EXPECT_TRUE(source.valid); EXPECT_GT(source.hints,0u); EXPECT_GT(source.deliveries,0u);
    EXPECT_GT(source.reads,0u); EXPECT_EQ(source.unavailable_reads,0u);
    source.DeliverAll();
    const auto language=document_test::Wide(u"fr-FR");
    ASSERT_TRUE(api.FPDFCatalog_SetLanguage(document.get(),language.data()));
    for (FPDF_DWORD flags : {FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}) {
      const auto saved=SaveDocument(api,document.get(),artifacts,spec.name+"-"+writer->name()+"-"+std::to_string(flags),flags);
      ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        SCOPED_TRACE(reader->name()); const auto& read=reader->api();
        Handle loaded(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);
        ASSERT_NE(loaded.get(),nullptr); ASSERT_EQ(read.FPDF_GetPageCount(loaded.get()),2);
        document_test::ExpectBytes([&](void* out,unsigned long count) {
          return read.FPDFCatalog_GetLanguage(loaded.get(),static_cast<FPDF_WCHAR*>(out),count);
        },document_test::Utf16(u"fr-FR"));
        for (int page=0;page<2;++page) {
          const auto image=Paint(read,loaded.get(),page);
          ASSERT_NO_FATAL_FAILURE(Inspect(image,page,spec));
          CompareRaster(originals[backend][page],image,artifacts,spec.name+"-saved-"+writer->name()+"-"+reader->name()+std::to_string(page));
        }
      }
    }
    ++backend;
  }
  for (int page=0;page<2;++page) CompareRaster(originals[0][page],originals[1][page],artifacts,spec.name+"-backends-"+std::to_string(page));
}
std::vector<Spec> Cases() {
  std::vector<Spec> out;
  auto add=[&](const char* name,Order order=Order::Forward)->Spec& {
    Spec s; s.name=name; s.order=order; out.push_back(s); return out.back();
  };
  add("RootResourcesForward");
  add("RootResourcesReverse",Order::Reverse);
  add("RootResourcesAlternating",Order::Alternating);
  add("ResourcesPrefetchedBeforeTheirPages",Order::Reverse).prefetch=true;
  add("IntermediatePageTreeResources",Order::Alternating).middle=true;
  add("LeafResourcesOverrideTheInheritedForm",Order::Reverse).leaf=true;
  add("BothPagesShareOneResourceObject",Order::Alternating).shared_leaf=true;
  add("NestedFormsArriveInReverseOrder",Order::Reverse).nested=true;
  add("UnusedResourceArrayCycleTerminates",Order::Alternating).cycle=1;
  add("UnusedResourceDictionaryCycleTerminates",Order::Reverse).cycle=2;
  add("MissingUnusedResourceDoesNotHidePaint",Order::Alternating).missing=true;
  auto& cycle=add("CyclicParentStillFindsItsOwnResources",Order::Reverse); cycle.parent_cycle=true; cycle.middle=true;
  auto& absent=add("ResourceLessParentCycleHasBoundedFallback",Order::Alternating); absent.parent_cycle=true; absent.no_resources=true;
  add("TransientReadFailureCanRetryTheSameAvailabilityOwner",Order::Reverse).fail_read=true;
  return out;
}
class CoverageAvailabilityResourceSequences : public testing::TestWithParam<Spec> {};
TEST_P(CoverageAvailabilityResourceSequences, OutOfOrderResourcesRemainRenderableAndSaveable) { Workflow(GetParam()); }
INSTANTIATE_TEST_SUITE_P(ResourceSequences,CoverageAvailabilityResourceSequences,testing::ValuesIn(Cases()),
    [](const testing::TestParamInfo<Spec>& item) { return item.param.name; });
}  // namespace
}  // namespace pdfoxium::integration
