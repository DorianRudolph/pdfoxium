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
constexpr int kImages = 27;
constexpr int kChangedImage = 5;
int Shade(int index) { return 8 + 8 * index; }
Bytes CachePressureDocument() {
  std::string resources, content;
  std::vector<std::string> objects{
      "<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>", "", ""};
  // Each distinct 2048-square Gray image realizes about 4MiB. The cache's
  // public limited mode trims at 100MiB, so 27 streams cross that threshold
  // while every individual source remains below the 60MB lazy-image boundary.
  // RLE keeps each stream at 65,537 bytes; the complete PDF is below 1.8MB.
  for (int index = 0; index < kImages; ++index) {
    const auto name = "I" + std::to_string(index);
    resources += "/" + name + " " + std::to_string(5 + index) + " 0 R ";
    content += "q 4 0 0 4 " + std::to_string(4 + (index % 9) * 8) + " " +
        std::to_string(4 + (index / 9) * 8) + " cm /" + name + " Do Q\n";
    std::string samples; samples.reserve(65537);
    for (int run = 0; run < 32768; ++run) { samples.push_back(char(129)); samples.push_back(char(Shade(index))); }
    samples.push_back(char(128));
    objects.push_back(PdfStream("/Type /XObject /Subtype /Image /Width 2048 /Height 2048 /BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /RunLengthDecode", samples));
  }
  // The first stream has aged out when the cache crosses 100MiB. Requesting
  // it again checks that eviction preserves its retained encoded source.
  content += "q 4 0 0 4 84 4 cm /I0 Do Q 0 1 0 rg 84 84 4 4 re f";
  objects[2] = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Resources << /XObject << " + resources + " >> >> /Contents 4 0 R >>";
  objects[3] = PdfStream("", content);
  return PdfFixture(objects);
}
void Pixel(const Raster& raster, int x, int y, std::array<int, 3> expected) {
  ASSERT_EQ(raster.width, kSide); ASSERT_EQ(raster.height, kSide);
  const auto at = (size_t(kSide - y - 1) * kSide + x) * 4;
  EXPECT_EQ(raster.pixels.at(at), expected[2]) << x << ',' << y;
  EXPECT_EQ(raster.pixels.at(at + 1), expected[1]) << x << ',' << y;
  EXPECT_EQ(raster.pixels.at(at + 2), expected[0]) << x << ',' << y;
  EXPECT_EQ(raster.pixels.at(at + 3), 255) << x << ',' << y;
}
void Inspect(const Raster& raster, bool edited) {
  for (int index = 0; index < kImages; ++index) {
    const int x = 6 + (index % 9) * 8, y = 6 + (index / 9) * 8;
    const int gray = Shade(index);
    Pixel(raster, x, y, edited && index == kChangedImage ?
          std::array<int, 3>{255, 0, 0} : std::array<int, 3>{gray, gray, gray});
    // Keep the gaps independently white, rather than only comparing engines.
    Pixel(raster, x + 3, y, {255, 255, 255});
  }
  Pixel(raster, 86, 6, {Shade(0), Shade(0), Shade(0)});
  Pixel(raster, 86, 86, {0, 255, 0});
  Pixel(raster, 2, 2, {255, 255, 255}); Pixel(raster, 92, 92, {255, 255, 255});
}
Raster Paint(const Api& api, FPDF_PAGE page, bool limited) {
  return RenderWithExactFlags(api, page, kSide, kSide, kFlags | (limited ? FPDF_RENDER_LIMITEDIMAGECACHE : 0));
}
void RenderBothLimits(const Api& api, FPDF_PAGE page, bool edited,
                      Artifacts& artifacts, const std::string& name, Raster* result) {
  const auto limited = Paint(api, page, true);
  ASSERT_NO_FATAL_FAILURE(Inspect(limited, edited));
  const auto unlimited = Paint(api, page, false);
  ASSERT_NO_FATAL_FAILURE(Inspect(unlimited, edited));
  CompareRaster(limited, unlimited, artifacts, name + "-limited-unlimited");
  const auto limited_again = Paint(api, page, true);
  ASSERT_NO_FATAL_FAILURE(Inspect(limited_again, edited));
  CompareRaster(limited, limited_again, artifacts, name + "-limited-again");
  *result = limited_again;
}
void ExerciseImageCacheLimits(bool edit) {
  const auto bytes = CachePressureDocument(); Artifacts artifacts;
  std::array<Raster, 2> originals; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
    ASSERT_EQ(api.FPDFPage_CountObjects(page.get()), kImages + 2);
    if (edit) {
      // Populate and evict through the public render flag before replacement.
      const auto before = Paint(api, page.get(), true);
      ASSERT_NO_FATAL_FAILURE(Inspect(before, false));
      const auto image = api.FPDFPage_GetObject(page.get(), kChangedImage); ASSERT_NE(image, nullptr);
      ASSERT_EQ(api.FPDFPageObj_GetType(image), FPDF_PAGEOBJ_IMAGE);
      Handle replacement(api.FPDFBitmap_Create(1, 1, 1), api.FPDFBitmap_Destroy); ASSERT_NE(replacement.get(), nullptr);
      ASSERT_TRUE(api.FPDFBitmap_FillRect(replacement.get(), 0, 0, 1, 1, 0xffff0000));
      FPDF_PAGE pages[] = {page.get()};
      ASSERT_TRUE(api.FPDFImageObj_SetBitmap(pages, 1, image, replacement.get()));
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    }
    ASSERT_NO_FATAL_FAILURE(RenderBothLimits(api, page.get(), edit, artifacts, writer->name(), &originals[backend]));
    const auto saved = SaveDocument(api, document.get(), artifacts, writer->name()); ASSERT_FALSE(saved.empty());
    // Destroy the writer's decoded page cache before opening another reader;
    // only the small raster and encoded save bytes cross this boundary.
    page.reset(); document.reset();
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name()); const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      Handle loaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage); ASSERT_NE(loaded.get(), nullptr);
      Raster after;
      ASSERT_NO_FATAL_FAILURE(RenderBothLimits(read, loaded.get(), edit, artifacts, writer->name() + "-" + reader->name(), &after));
      CompareRaster(originals[backend], after, artifacts, writer->name() + "-" + reader->name() + "-save-reload");
    }
    ++backend;
  }
  CompareRaster(originals[0], originals[1], artifacts, "native-rust");
}
TEST(CoverageImageCacheLimits, LimitedAndUnlimitedCachesPreserveEvictedAndRevisitedImageSamples) {
  ExerciseImageCacheLimits(false);
}
TEST(CoverageImageCacheLimits, ReplacingAnEvictedImagePreservesOtherTilesAndBothSaveDirections) {
  ExerciseImageCacheLimits(true);
}
}  // namespace
}  // namespace pdfoxium::integration
