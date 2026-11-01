// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "text_fixture.h"

#include <array>
#include <cmath>
#include <cstring>
#include <sstream>
#include <string>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
constexpr int kSource = 2048;
constexpr int kTile = 512;
constexpr int kFlags = FPDF_NO_NATIVETEXT | FPDF_RENDER_NO_SMOOTHIMAGE |
                       FPDF_RENDER_NO_SMOOTHPATH;
enum class Codec { Jpeg, Jpx };
struct Rect { int x, y, width, height; };
struct Scene {
  const char* name;
  int width, height;
  FS_MATRIX image;
  Rect interior;
};
const Scene kAxis{"axis", 2048, 2048, {2048,0,0,2048,0,0}, {768,768,512,512}};
const std::array<Scene, 3> kAffineScenes{{
    kAxis,
    {"quarter-turn",2048,2048,{0,2048,-2048,0,2048,0},{768,768,512,512}},
    // Integral shear keeps inverse-sampling phases exact. The selected region
    // is far inside the parallelogram, excluding unrelated image-edge AA.
    {"shear",4096,2048,{2048,0,2048,2048,0,0},{1792,768,512,512}},
}};
const Scene kSmall{"small",2048,2048,{512,0,0,512,768,768},{768,768,512,512}};

std::string CodecName(Codec codec) { return codec == Codec::Jpeg ? "jpeg" : "jpx"; }
Bytes Encoded(Codec codec) {
  return text_fixture::Read(TestRoot() / "tests/capi/fixtures/image-drawn-size" /
      (codec == Codec::Jpeg ? "checker-2048-420.jpg" : "checker-2048-lossless.jp2"));
}
struct JpegAccess {
  FPDF_FILEACCESS interface{};
  const Bytes& bytes;
  size_t reads = 0;
  bool valid = true;
  explicit JpegAccess(const Bytes& source) : bytes(source) {
    interface.m_FileLen = static_cast<unsigned long>(bytes.size());
    interface.m_Param = this;
    interface.m_GetBlock = Read;
  }
  static int Read(void* context, unsigned long offset, unsigned char* destination,
                  unsigned long count) noexcept {
    auto& self = *static_cast<JpegAccess*>(context);
    ++self.reads;
    if (offset > self.bytes.size() || count > self.bytes.size() - offset) {
      self.valid = false;
      return 0;
    }
    if (count) std::memcpy(destination, self.bytes.data() + offset, count);
    return 1;
  }
};
Bytes ImageDocument(const Library& writer, Codec codec, const Scene& scene,
                    Artifacts& artifacts) {
  const auto& api = writer.api();
  const auto encoded = Encoded(codec);
  EXPECT_FALSE(encoded.empty());
  if (encoded.empty()) return {};
  if (codec == Codec::Jpeg) {
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    EXPECT_NE(document.get(), nullptr); if (!document.get()) return {};
    Handle page(api.FPDFPage_New(document.get(), 0, scene.width, scene.height), api.FPDF_ClosePage);
    EXPECT_NE(page.get(), nullptr); if (!page.get()) return {};
    Handle image(api.FPDFPageObj_NewImageObj(document.get()), api.FPDFPageObj_Destroy);
    EXPECT_NE(image.get(), nullptr); if (!image.get()) return {};
    JpegAccess source(encoded);
    EXPECT_TRUE(api.FPDFImageObj_LoadJpegFileInline(nullptr, 0, image.get(), &source.interface));
    EXPECT_GT(source.reads, 0u); EXPECT_TRUE(source.valid);
    EXPECT_TRUE(api.FPDFPageObj_SetMatrix(image.get(), &scene.image));
    EXPECT_TRUE(api.FPDFPage_InsertObject(page.get(), image.get()));
    image.release();
    EXPECT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    return SaveDocument(api, document.get(), artifacts,
                        writer.name() + "-" + CodecName(codec) + "-" + scene.name);
  }
  // The public API has no JPX-loading constructor. Supply only the encoded
  // image/resources/content, then save and read through the same public API.
  std::ostringstream content;
  const auto& m = scene.image;
  content << "q " << m.a << ' ' << m.b << ' ' << m.c << ' ' << m.d << ' '
          << m.e << ' ' << m.f << " cm /I Do Q";
  const auto fixture = PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 " + std::to_string(scene.width) +
          " " + std::to_string(scene.height) + "] /Resources << /XObject << /I 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", content.str()),
      PdfStream("/Type /XObject /Subtype /Image /Width 2048 /Height 2048 "
                "/BitsPerComponent 8 /ColorSpace /DeviceRGB /Filter /JPXDecode",
                std::string(encoded.begin(), encoded.end()))});
  Handle document(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
  EXPECT_NE(document.get(), nullptr); if (!document.get()) return {};
  return SaveDocument(api, document.get(), artifacts,
                      writer.name() + "-" + CodecName(codec) + "-" + scene.name);
}
Raster Copy(const Api& api, FPDF_BITMAP bitmap, int width, int height) {
  const auto* data = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap));
  const int stride = api.FPDFBitmap_GetStride(bitmap);
  EXPECT_NE(data, nullptr); EXPECT_GE(stride, width * 4);
  if (!data || stride < width * 4) return {};
  Raster result{width, height, Bytes(static_cast<size_t>(width) * height * 4)};
  for (int y = 0; y < height; ++y)
    std::copy_n(data + static_cast<size_t>(y) * stride, width * 4,
                result.pixels.begin() + static_cast<size_t>(y) * width * 4);
  return result;
}
Raster Paint(const Api& api, FPDF_PAGE page, const Scene& scene, const Rect& area,
             bool matrix, int inset = 0, float zoom = 1, int flags = kFlags) {
  Handle bitmap(api.FPDFBitmap_CreateEx(area.width, area.height, FPDFBitmap_BGRA, nullptr, 0),
                api.FPDFBitmap_Destroy);
  EXPECT_NE(bitmap.get(), nullptr); if (!bitmap.get()) return {};
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, area.width, area.height, 0xffffffff));
  if (matrix) {
    // fpdf_view.cpp first applies the page's ordinary display matrix. The
    // caller matrix is in display/device coordinates, so no extra y-flip.
    const FS_MATRIX transform{zoom,0,0,zoom,float(-area.x),float(-area.y)};
    const FS_RECTF clip{float(inset),float(area.height-inset),
                        float(area.width-inset),float(inset)};
    api.FPDF_RenderPageBitmapWithMatrix(bitmap.get(), page, &transform, &clip, flags);
  } else {
    api.FPDF_RenderPageBitmap(bitmap.get(), page, -area.x, -area.y,
                             int(scene.width * zoom), int(scene.height * zoom), 0, flags);
  }
  return Copy(api, bitmap.get(), area.width, area.height);
}
Raster Fresh(const Library& reader, const Bytes& bytes, const Scene& scene,
             const Rect& area, bool matrix, int inset = 0, int flags = kFlags,
             int source_side = kSource) {
  const auto& api = reader.api();
  Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
  EXPECT_NE(document.get(), nullptr); if (!document.get()) return {};
  Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
  EXPECT_NE(page.get(), nullptr); if (!page.get()) return {};
  EXPECT_EQ(api.FPDFPage_CountObjects(page.get()), 1);
  const auto image = api.FPDFPage_GetObject(page.get(), 0);
  EXPECT_EQ(api.FPDFPageObj_GetType(image), FPDF_PAGEOBJ_IMAGE);
  // PixelSize reads metadata without decoding or warming the page image cache.
  unsigned int width = 0, height = 0;
  EXPECT_TRUE(api.FPDFImageObj_GetImagePixelSize(image, &width, &height));
  EXPECT_EQ(width, unsigned(source_side)); EXPECT_EQ(height, unsigned(source_side));
  return Paint(api, page.get(), scene, area, matrix, inset, 1, flags);
}
Raster ExpectedRegion(const Raster& whole, const Rect& area, int inset = 0) {
  auto region = CropRaster(whole, area.x, area.y, area.width, area.height);
  for (int y = 0; y < region.height; ++y) for (int x = 0; x < region.width; ++x)
    if (x < inset || y < inset || x >= region.width-inset || y >= region.height-inset) {
      const auto at = (static_cast<size_t>(y) * region.width + x) * 4;
      std::fill_n(region.pixels.begin() + at, 4, 255);
    }
  return region;
}
void FullDetail(const Raster& whole, const Scene& scene) {
  ASSERT_EQ(whole.width, scene.width); ASSERT_EQ(whole.height, scene.height);
  const int x = scene.width / 2, y = scene.height / 2;
  const auto at = (static_cast<size_t>(y) * whole.width + x) * 4;
  // The source is a two-pixel checkerboard. A full-size draw must retain its
  // adjacent-cell contrast, independently of another implementation's pixels.
  EXPECT_GT(std::abs(int(whole.pixels.at(at)) - int(whole.pixels.at(at + 2 * 4))), 80);
  size_t nonopaque = 0;
  for (size_t i = 3; i < whole.pixels.size(); i += 4) nonopaque += whole.pixels[i] != 255;
  EXPECT_EQ(nonopaque, 0u);
}
void DecoderBound(const Raster& a, const Raster& b, Codec codec,
                  Artifacts& artifacts, const std::string& label) {
  if (!raster_comparison_settings().enabled) return;
  ASSERT_EQ(a.width, b.width); ASSERT_EQ(a.height, b.height);
  ASSERT_EQ(a.pixels.size(), b.pixels.size());
  int largest = 0;
  for (size_t i = 0; i < a.pixels.size(); ++i)
    largest = std::max(largest, std::abs(int(a.pixels[i]) - int(b.pixels[i])));
  // Full-size JPEG IDCT/color rounding is already a two-channel-level boundary
  // in image_tests.cpp. This never relaxes the exact same-engine region check.
  const int limit = codec == Codec::Jpeg ? 2 : 0;
  EXPECT_LE(largest, limit) << label;
  if (largest > limit) { artifacts.Image(label + "-a", a); artifacts.Image(label + "-b", b); }
}
void RegionContract(const Library& reader, const Raster& expected, const Raster& fresh,
                    Artifacts& artifacts, const std::string& label) {
  if (reader.name() == "rust") {
    CompareRaster(expected, fresh, artifacts, label);
  } else {
    // Pinned CPDF_ImageRenderer requests the device bitmap size. Thus a fresh
    // 512-square output uses a reduced decode despite a 2048-pixel drawn edge.
    EXPECT_NE(expected.pixels, fresh.pixels) << "Pinned native decode-size defect: " << label;
    artifacts.Image(label + "-native-fresh", fresh);
  }
}
void WarmFullControl(const Library& reader, const Bytes& bytes, const Scene& scene,
                     bool matrix, int inset, const Raster& expected,
                     Artifacts& artifacts, const std::string& label, int flags = kFlags,
                     Raster* clipped_control = nullptr, bool expect_whole_crop = true) {
  const auto& api = reader.api();
  Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
  ASSERT_NE(document.get(), nullptr);
  Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
  const auto whole = Paint(api, page.get(), scene, {0,0,scene.width,scene.height}, matrix, 0, 1, flags);
  const auto tile = Paint(api, page.get(), scene, scene.interior, matrix, inset, 1, flags);
  const auto crop = ExpectedRegion(whole, scene.interior, inset);
  if (expect_whole_crop || reader.name() == "rust") {
    CompareRaster(crop, tile, artifacts, label + "-warm");
    CompareRaster(expected, tile, artifacts, label + "-fresh-whole-control");
  } else {
    // Pinned native affine clipping changes the sampling phase even after a
    // full decode. Rust keeps the whole-image phase for every region.
    CompareRaster(expected, crop, artifacts, label + "-full-repeat");
    EXPECT_NE(crop.pixels, tile.pixels) << "Pinned native shear clipping phase";
    artifacts.Image(label + "-full-crop", crop);
    artifacts.Image(label + "-full-detail-clipped", tile);
  }
  if (clipped_control) *clipped_control = tile;
}
Bytes ChangedPixels(const Raster& a, const Raster& b) {
  EXPECT_EQ(a.width, b.width); EXPECT_EQ(a.height, b.height);
  EXPECT_EQ(a.pixels.size(), b.pixels.size());
  if (a.pixels.size() != b.pixels.size()) return {};
  Bytes changed(a.pixels.size() / 4);
  for (size_t pixel = 0; pixel < changed.size(); ++pixel)
    for (size_t channel = 0; channel < 4; ++channel)
      changed[pixel] |= a.pixels[pixel * 4 + channel] != b.pixels[pixel * 4 + channel];
  return changed;
}
void OffsetTiles(Codec codec, int flags) {
  SCOPED_TRACE("flags=" + std::to_string(flags));
  Artifacts artifacts;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE("writer=" + writer->name());
    const auto bytes = ImageDocument(*writer, codec, kAxis, artifacts); ASSERT_FALSE(bytes.empty());
    std::array<Raster,2> wholes; size_t index = 0;
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE("reader=" + reader->name());
      auto& whole = wholes[index++];
      whole = Fresh(*reader, bytes, kAxis, {0,0,kSource,kSource}, false, 0, flags);
      ASSERT_NO_FATAL_FAILURE(FullDetail(whole, kAxis));
      artifacts.Image(writer->name() + "-" + reader->name() + "-whole", whole);
      for (int y = 0; y < kSource; y += kTile) for (int x = 0; x < kSource; x += kTile) {
        const Rect tile{x,y,kTile,kTile};
        const auto actual = Fresh(*reader, bytes, kAxis, tile, false, 0, flags);
        RegionContract(*reader, ExpectedRegion(whole, tile), actual, artifacts,
                       writer->name() + "-" + reader->name() + "-flags" + std::to_string(flags) + "-offset-" + std::to_string(x) + "-" + std::to_string(y));
      }
      WarmFullControl(*reader, bytes, kAxis, false, 0,
                      ExpectedRegion(whole, kAxis.interior), artifacts, writer->name() + "-" + reader->name() + "-flags" + std::to_string(flags) + "-offset", flags);
    }
    DecoderBound(wholes[0], wholes[1], codec, artifacts, writer->name() + "-full-engines");
  }
}
void MatrixRegions(Codec codec) {
  Artifacts artifacts;
  for (const auto& scene : kAffineScenes) {
    SCOPED_TRACE(scene.name);
    const bool shear = std::string(scene.name) == "shear";
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE("writer=" + writer->name());
      const auto bytes = ImageDocument(*writer, codec, scene, artifacts); ASSERT_FALSE(bytes.empty());
      std::array<Raster,2> wholes;
      std::array<Bytes,2> shear_changes;
      size_t index = 0;
      for (const auto* reader : Implementations()) {
        SCOPED_TRACE("reader=" + reader->name());
        const size_t backend = index++;
        auto& whole = wholes[backend];
        whole = Fresh(*reader, bytes, scene, {0,0,scene.width,scene.height}, true);
        ASSERT_NO_FATAL_FAILURE(FullDetail(whole, scene));
        const auto expected = ExpectedRegion(whole, scene.interior, 16);
        const auto actual = Fresh(*reader, bytes, scene, scene.interior, true, 16);
        const auto label = writer->name() + "-" + reader->name() + "-matrix-" + scene.name;
        if (shear) {
          Raster full_detail_clip;
          ASSERT_NO_FATAL_FAILURE(WarmFullControl(*reader, bytes, scene, true, 16,
              expected, artifacts, label, kFlags, &full_detail_clip, false));
          ASSERT_EQ(full_detail_clip.width, scene.interior.width);
          // The control has a known full decode but identical clip geometry.
          // Rust's fresh decode must match it EXACTLY; pinned native's fresh
          // reduced decode must still differ. No affine tolerance is added.
          RegionContract(*reader, full_detail_clip, actual, artifacts, label);
          shear_changes[backend] = ChangedPixels(expected, full_detail_clip);
          const auto changed = std::count(shear_changes[backend].begin(), shear_changes[backend].end(), 1);
          if (reader->name() == "rust") EXPECT_EQ(changed, 0);
          else EXPECT_GT(changed, 0);
        } else {
          RegionContract(*reader, expected, actual, artifacts, label);
          WarmFullControl(*reader, bytes, scene, true, 16, expected, artifacts, label);
        }
      }
      DecoderBound(wholes[0], wholes[1], codec, artifacts,
                   writer->name() + "-full-matrix-" + scene.name);
      if (shear) {
        // Preserve the native defect evidence alongside exact Rust crop checks.
        artifacts.Text(writer->name() + "-shear-clip-phase",
            "native changed pixels=" + std::to_string(std::count(shear_changes[0].begin(), shear_changes[0].end(), 1)) +
            "\nrust changed pixels=" + std::to_string(std::count(shear_changes[1].begin(), shear_changes[1].end(), 1)) + "\n");
      }
    }
  }
}
void SmallDrawnImage(Codec codec) {
  Artifacts artifacts;
  for (const auto* writer : Implementations()) {
    const auto bytes = ImageDocument(*writer, codec, kSmall, artifacts); ASSERT_FALSE(bytes.empty());
    std::array<Raster,2> crops, regions; size_t index = 0;
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(writer->name() + "-" + reader->name());
      const auto whole = Fresh(*reader, bytes, kSmall, {0,0,2048,2048}, false);
      crops[index] = ExpectedRegion(whole, kSmall.interior);
      regions[index] = Fresh(*reader, bytes, kSmall, kSmall.interior, false);
      if (reader->name() == "rust") CompareRaster(crops[index], regions[index], artifacts, "small-rust-region");
      else EXPECT_NE(crops[index].pixels, regions[index].pixels);
      ++index;
    }
    // The native region requests exactly the image's drawn 512-square size,
    // making it a source-derived control for Rust's new whole-page reduction.
    DecoderBound(regions[0], crops[1], codec, artifacts, writer->name() + "-small-native-region-control");
  }
}
void GrowingDrawnSize(Codec codec) {
  Artifacts artifacts;
  for (const auto* writer : Implementations()) {
    const auto bytes = ImageDocument(*writer, codec, kAxis, artifacts); ASSERT_FALSE(bytes.empty());
    for (const auto* reader : Implementations()) for (bool matrix : {false,true}) {
      SCOPED_TRACE(writer->name() + "-" + reader->name() + "-matrix=" + std::to_string(matrix));
      const auto& api = reader->api();
      const auto whole = Fresh(*reader, bytes, kAxis, {0,0,2048,2048}, matrix);
      const auto expected = ExpectedRegion(whole, kAxis.interior);
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage); ASSERT_NE(page.get(), nullptr);
      const auto thumbnail = Paint(api, page.get(), kAxis, {0,0,512,512}, matrix, 0, 0.25f);
      ASSERT_EQ(thumbnail.width, 512); ASSERT_EQ(thumbnail.height, 512);
      // Both output bitmaps are 512-square. Only the viewport/drawn image grows
      // from 512 to 2048, so the cache must invalidate its quarter-size plane.
      const auto enlarged = Paint(api, page.get(), kAxis, kAxis.interior, matrix);
      RegionContract(*reader, expected, enlarged, artifacts, writer->name() + "-" + reader->name() + "-grow");
    }
  }
}
TEST(CoverageImageDrawnSize, JpxWithNoDecompositionLevelsRemainsVisibleWhenDrawnAtOnePixel) {
  const auto encoded = text_fixture::Read(TestRoot() /
      "tests/capi/fixtures/image-drawn-size/gray-32-no-decomposition.jp2");
  ASSERT_FALSE(encoded.empty());
  const Scene scene{"one-pixel-jpx",64,64,{1,0,0,1,20,20},{20,43,1,1}};
  const auto fixture = PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << /XObject << /I 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", "q 1 0 0 1 20 20 cm /I Do Q"),
      PdfStream("/Type /XObject /Subtype /Image /Width 32 /Height 32 "
                "/BitsPerComponent 8 /ColorSpace /DeviceRGB /Filter /JPXDecode",
                std::string(encoded.begin(), encoded.end()))});
  Artifacts artifacts;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE("writer=" + writer->name());
    const auto& api = writer->api();
    Handle document(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    const auto bytes = SaveDocument(api, document.get(), artifacts, writer->name() + "-no-decomposition-jpx");
    ASSERT_FALSE(bytes.empty());
    for (const auto* reader : Implementations()) for (bool matrix : {false,true}) {
      SCOPED_TRACE("reader=" + reader->name() + "-matrix=" + std::to_string(matrix));
      const auto whole = Fresh(*reader, bytes, scene, {0,0,64,64}, matrix, 0, kFlags, 32);
      ASSERT_EQ(whole.width, 64); ASSERT_EQ(whole.height, 64);
      const auto expected = ExpectedRegion(whole, scene.interior);
      EXPECT_EQ(expected.pixels, (Bytes{128,128,128,255}));
      size_t visible_pixels = 0;
      for (size_t i = 0; i < whole.pixels.size(); i += 4) {
        visible_pixels += whole.pixels[i] != 255 || whole.pixels[i+1] != 255 || whole.pixels[i+2] != 255;
        EXPECT_EQ(whole.pixels[i+3], 255);
      }
      EXPECT_EQ(visible_pixels, 1u) << "The valid JPX must remain a visible one-pixel image";
      const auto region = Fresh(*reader, bytes, scene, scene.interior, matrix, 0, kFlags, 32);
      if (reader->name() == "rust") {
        CompareRaster(expected, region, artifacts, writer->name() + "-no-decomposition-rust-region");
      } else {
        // Native uses bitmap1 as its hint, requests log2(32/1)=5 and rejects
        // this valid zero-decomposition codestream. Its bitmap64 control
        // requests skip0 and paints gray128. See cjpx_decoder.cpp::Init and
        // OpenJPEG j2k.c's m_reduce >= numresolutions validation.
        EXPECT_EQ(region.pixels, (Bytes{255,255,255,255}));
      }
    }
  }
}

TEST(CoverageImageDrawnSize, JpegOffsetTilesMatchWholePageCropsWithFreshCachesAndBothSavedReaders) {
  for (int flags : {FPDF_NO_NATIVETEXT, kFlags}) OffsetTiles(Codec::Jpeg, flags);
}
TEST(CoverageImageDrawnSize, JpxOffsetTilesMatchWholePageCropsWithFreshCachesAndBothSavedReaders) {
  for (int flags : {FPDF_NO_NATIVETEXT, kFlags}) OffsetTiles(Codec::Jpx, flags);
}
TEST(CoverageImageDrawnSize, JpegMatrixInteriorClipsPreserveRotatedAndSkewedImageDetail) {
  MatrixRegions(Codec::Jpeg);
}
TEST(CoverageImageDrawnSize, JpxMatrixInteriorClipsPreserveRotatedAndSkewedImageDetail) {
  MatrixRegions(Codec::Jpx);
}
TEST(CoverageImageDrawnSize, SmallJpegDrawUsesItsDrawnSizeOnBothWholePageAndRegion) {
  SmallDrawnImage(Codec::Jpeg);
}
TEST(CoverageImageDrawnSize, SmallJpxDrawUsesItsDrawnSizeOnBothWholePageAndRegion) {
  SmallDrawnImage(Codec::Jpx);
}
TEST(CoverageImageDrawnSize, JpegCacheReloadsWhenDrawnSizeGrowsWithUnchangedBitmapSize) {
  GrowingDrawnSize(Codec::Jpeg);
}
TEST(CoverageImageDrawnSize, JpxCacheReloadsWhenDrawnSizeGrowsWithUnchangedBitmapSize) {
  GrowingDrawnSize(Codec::Jpx);
}
}  // namespace
}  // namespace pdfoxium::integration
