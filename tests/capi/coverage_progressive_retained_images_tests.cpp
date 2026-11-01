// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "runtime_fixture.h"

#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
constexpr int kSide = 96;
constexpr int kFlags = FPDF_RENDER_NO_SMOOTHIMAGE | FPDF_RENDER_NO_SMOOTHPATH;
Bytes RetainedImage(uint8_t gray) {
  // Native retains lazy scanlines at 60,000,000 decoded bytes. The encoded
  // program stays below 1MB and its decoder owns one 512-byte row. Translation
  // makes the stretcher skip 119,920 rows before painting the visible bottom80.
  std::string samples; samples.reserve(960001);
  for (int run = 0; run < 480000; ++run) { samples.push_back(char(129)); samples.push_back(char(gray)); }
  samples.push_back(char(128));
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Resources << /XObject << /I 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", "q 54 0 0 120000 16 16 cm /I Do Q 0 1 0 rg 76 8 8 8 re f"),
      PdfStream("/Type /XObject /Subtype /Image /Width 512 /Height 120000 /BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /RunLengthDecode", samples)});
}
void Pixel(const Raster& raster, int x, int y, std::array<int, 3> rgb) {
  ASSERT_EQ(raster.width, kSide); ASSERT_EQ(raster.height, kSide);
  const auto at = (size_t(kSide - y - 1) * kSide + x) * 4;
  EXPECT_EQ(raster.pixels.at(at), rgb[2]); EXPECT_EQ(raster.pixels.at(at + 1), rgb[1]);
  EXPECT_EQ(raster.pixels.at(at + 2), rgb[0]); EXPECT_EQ(raster.pixels.at(at + 3), 255);
}
void CompletePixels(const Raster& raster, uint8_t gray) {
  Pixel(raster, 20, 20, {gray, gray, gray}); Pixel(raster, 60, 90, {gray, gray, gray});
  Pixel(raster, 10, 50, {255, 255, 255}); Pixel(raster, 30, 10, {255, 255, 255});
  Pixel(raster, 80, 12, {0, 255, 0});
}
struct DelayedPause : IFSDK_PAUSE {
  DelayedPause() { version = 1; user = this; NeedToPauseNow = Need; }
  static FPDF_BOOL Need(IFSDK_PAUSE* value) {
    auto& self = *static_cast<DelayedPause*>(value); ++self.calls;
    if (self.calls >= 1024 && self.yields < self.budget) { ++self.yields; return 1; }
    return 0;
  }
  size_t calls = 0;
  int yields = 0, budget = 3;
};
struct LiveJob {
  LiveJob(const Api& functions, FPDF_PAGE value) : api(functions), page(value) {}
  ~LiveJob() { api.FPDF_RenderPage_Close(page); }
  const Api& api; FPDF_PAGE page;
};
void StillSkipping(const Raster& raster) {
  // A pause at callback1024 occurs far before the decoder's target119920.
  // Neither the image nor its following green rectangle has painted yet.
  Pixel(raster, 20, 20, {255, 255, 255}); Pixel(raster, 60, 90, {255, 255, 255});
  Pixel(raster, 80, 12, {255, 255, 255});
}
void CompleteJob(const Api& api, FPDF_PAGE page, int* status, DelayedPause* pause) {
  int continuations = 0;
  while (*status == FPDF_RENDER_TOBECONTINUED && continuations++ < 8)
    *status = api.FPDF_RenderPage_Continue(page, pause);
  ASSERT_EQ(*status, FPDF_RENDER_DONE); EXPECT_LE(continuations, 8);
  EXPECT_GE(pause->calls, 1024u); EXPECT_EQ(pause->yields, pause->budget);
}
// CAPI-created bitmaps guarantee the alignment native uint32_t spans require.
// The source remains lazy: do not request GetBitmap() or warm the page cache
// with an ordinary render before the delayed-pause assertion.
void ProgressiveDocument(const Api& api, const Bytes& bytes, uint8_t gray,
                         bool cancel, Artifacts& artifacts, const std::string& label, Raster* result = nullptr) {
  Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
  ASSERT_NE(document.get(), nullptr);
  Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
  Handle bitmap(api.FPDFBitmap_Create(kSide, kSide, 1), api.FPDFBitmap_Destroy); ASSERT_NE(bitmap.get(), nullptr);
  ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, kSide, kSide, 0xffffffff));
  DelayedPause pause; LiveJob job(api, page.get());
  int status = api.FPDF_RenderPageBitmap_Start(bitmap.get(), page.get(), 0, 0, kSide, kSide, 0, kFlags, &pause);
  ASSERT_EQ(status, FPDF_RENDER_TOBECONTINUED);
  EXPECT_GE(pause.calls, 1024u); EXPECT_EQ(pause.yields, 1);
  ASSERT_NO_FATAL_FAILURE(StillSkipping(runtime_test::Bitmap(api, bitmap.get())));
  if (cancel) {
    api.FPDF_RenderPage_Close(page.get());
    ASSERT_NO_FATAL_FAILURE(StillSkipping(runtime_test::Bitmap(api, bitmap.get())));
    // Restart the same page with its retained source at a nonzero decoder row.
    // Both callback owner and destination remain alive throughout cancellation.
    pause.calls = 0; pause.yields = 0;
    status = api.FPDF_RenderPageBitmap_Start(bitmap.get(), page.get(), 0, 0, kSide, kSide, 0, kFlags, &pause);
    ASSERT_EQ(status, FPDF_RENDER_TOBECONTINUED);
    EXPECT_GE(pause.calls, 1024u); EXPECT_EQ(pause.yields, 1);
    ASSERT_NO_FATAL_FAILURE(StillSkipping(runtime_test::Bitmap(api, bitmap.get())));
  }
  ASSERT_NO_FATAL_FAILURE(CompleteJob(api, page.get(), &status, &pause));
  const auto actual = runtime_test::Bitmap(api, bitmap.get());
  ASSERT_NO_FATAL_FAILURE(CompletePixels(actual, gray));
  api.FPDF_RenderPage_Close(page.get());
  // A static render after completion also verifies that the retained decoder
  // can seek back from its last visible row without losing the source samples.
  const auto ordinary = RenderWithExactFlags(api, page.get(), kSide, kSide, kFlags);
  ASSERT_NO_FATAL_FAILURE(CompletePixels(ordinary, gray));
  CompareRaster(ordinary, actual, artifacts, label + "-ordinary-progressive");
  if (result) *result = actual;
}
void SavedProgressive(bool cancel) {
  const auto bytes = RetainedImage(64); Artifacts artifacts;
  std::array<Raster, 2> originals; size_t index = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    ASSERT_NO_FATAL_FAILURE(ProgressiveDocument(api, bytes, 64, cancel, artifacts, writer->name() + "-original", &originals[index]));
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    const auto saved = SaveDocument(api, document.get(), artifacts, writer->name()); ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      Raster after;
      ASSERT_NO_FATAL_FAILURE(ProgressiveDocument(reader->api(), saved, 64, cancel, artifacts, writer->name() + "-" + reader->name(), &after));
      CompareRaster(originals[index], after, artifacts, writer->name() + "-" + reader->name() + "-save-reload");
    }
    ++index;
  }
  CompareRaster(originals[0], originals[1], artifacts, "native-rust-progressive");
}
TEST(CoverageProgressiveRetainedImages, DelayedRealPauseYieldsInsideCroppedDecoderSkipping) {
  SavedProgressive(false);
}
TEST(CoverageProgressiveRetainedImages, CancellingDuringDecoderSkippingAllowsTheSamePageToRestart) {
  SavedProgressive(true);
}

void InterleavedDocuments(const Api& api, const Bytes& first, const Bytes& second,
                          Artifacts& artifacts, const std::string& label, std::array<Raster, 2>* result = nullptr) {
  Handle one(api.FPDF_LoadMemDocument64(first.data(), first.size(), nullptr), api.FPDF_CloseDocument);
  Handle two(api.FPDF_LoadMemDocument64(second.data(), second.size(), nullptr), api.FPDF_CloseDocument);
  ASSERT_NE(one.get(), nullptr); ASSERT_NE(two.get(), nullptr);
  Handle page_one(api.FPDF_LoadPage(one.get(), 0), api.FPDF_ClosePage);
  Handle page_two(api.FPDF_LoadPage(two.get(), 0), api.FPDF_ClosePage);
  ASSERT_NE(page_one.get(), nullptr); ASSERT_NE(page_two.get(), nullptr);
  Handle bitmap_one(api.FPDFBitmap_Create(kSide, kSide, 1), api.FPDFBitmap_Destroy);
  Handle bitmap_two(api.FPDFBitmap_Create(kSide, kSide, 1), api.FPDFBitmap_Destroy);
  ASSERT_NE(bitmap_one.get(), nullptr); ASSERT_NE(bitmap_two.get(), nullptr);
  ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap_one.get(), 0, 0, kSide, kSide, 0xffffffff));
  ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap_two.get(), 0, 0, kSide, kSide, 0xffffffff));
  DelayedPause pause_one, pause_two;
  LiveJob job_one(api, page_one.get()), job_two(api, page_two.get());
  int status_one = api.FPDF_RenderPageBitmap_Start(bitmap_one.get(), page_one.get(), 0, 0, kSide, kSide, 0, kFlags, &pause_one);
  int status_two = api.FPDF_RenderPageBitmap_Start(bitmap_two.get(), page_two.get(), 0, 0, kSide, kSide, 0, kFlags, &pause_two);
  ASSERT_EQ(status_one, FPDF_RENDER_TOBECONTINUED); ASSERT_EQ(status_two, FPDF_RENDER_TOBECONTINUED);
  ASSERT_NO_FATAL_FAILURE(StillSkipping(runtime_test::Bitmap(api, bitmap_one.get())));
  ASSERT_NO_FATAL_FAILURE(StillSkipping(runtime_test::Bitmap(api, bitmap_two.get())));
  // Advance the second document first. Its callback and row position must not
  // replace the still-suspended job attached to the first document's page.
  ASSERT_NO_FATAL_FAILURE(CompleteJob(api, page_two.get(), &status_two, &pause_two));
  ASSERT_NO_FATAL_FAILURE(StillSkipping(runtime_test::Bitmap(api, bitmap_one.get())));
  ASSERT_NO_FATAL_FAILURE(CompleteJob(api, page_one.get(), &status_one, &pause_one));
  const auto actual_one = runtime_test::Bitmap(api, bitmap_one.get());
  const auto actual_two = runtime_test::Bitmap(api, bitmap_two.get());
  ASSERT_NO_FATAL_FAILURE(CompletePixels(actual_one, 64));
     ASSERT_NO_FATAL_FAILURE(CompletePixels(actual_two, 192));
  api.FPDF_RenderPage_Close(page_one.get()); api.FPDF_RenderPage_Close(page_two.get());
  CompareRaster(RenderWithExactFlags(api, page_one.get(), kSide, kSide, kFlags), actual_one, artifacts, label + "-first");
  CompareRaster(RenderWithExactFlags(api, page_two.get(), kSide, kSide, kFlags), actual_two, artifacts, label + "-second");
  if (result) *result = {actual_one, actual_two};
}
TEST(CoverageProgressiveRetainedImages, IndependentDocumentsRetainTheirOwnSuspendedDecoderPositions) {
  const auto first = RetainedImage(64), second = RetainedImage(192); Artifacts artifacts;
  std::array<std::array<Raster, 2>, 2> originals; size_t index = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    ASSERT_NO_FATAL_FAILURE(InterleavedDocuments(api, first, second, artifacts, writer->name() + "-original", &originals[index]));
    Handle one(api.FPDF_LoadMemDocument64(first.data(), first.size(), nullptr), api.FPDF_CloseDocument);
    Handle two(api.FPDF_LoadMemDocument64(second.data(), second.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(one.get(), nullptr); ASSERT_NE(two.get(), nullptr);
    const auto saved_one = SaveDocument(api, one.get(), artifacts, writer->name() + "-first");
    const auto saved_two = SaveDocument(api, two.get(), artifacts, writer->name() + "-second");
    ASSERT_FALSE(saved_one.empty()); ASSERT_FALSE(saved_two.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      std::array<Raster, 2> after;
      ASSERT_NO_FATAL_FAILURE(InterleavedDocuments(reader->api(), saved_one, saved_two, artifacts, writer->name() + "-" + reader->name(), &after));
      for (size_t document = 0; document < 2; ++document)
        CompareRaster(originals[index][document], after[document], artifacts, writer->name() + "-" + reader->name() + "-saved-" + std::to_string(document));
    }
    ++index;
  }
  for (size_t document = 0; document < 2; ++document)
    CompareRaster(originals[0][document], originals[1][document], artifacts, "native-rust-document-" + std::to_string(document));
}
}  // namespace
}  // namespace pdfoxium::integration
