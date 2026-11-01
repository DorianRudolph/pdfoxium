#include "document_fixture.h"
#include "runtime_fixture.h"

#include <cmath>

namespace pdfoxium::integration {
namespace {
using namespace document_test;
enum class Place { Page, Form, NestedForm, Annotation };
enum class Kind { Text, Image, Pattern };
using Bytes = std::vector<uint8_t>;

Bytes Scene(Place place, Kind kind) {
  const auto font = runtime_test::Resource("fonts/ahem/Ahem.ttf");
  const std::string resources = "/Font << /F 5 0 R >> /XObject << /Im 8 0 R /Outer 9 0 R /Inner 10 0 R >> /Pattern << /Blue 11 0 R /Red 12 0 R >>";
  std::string target = kind == Kind::Image ? "q 32 0 0 24 64 32 cm /Im Do Q\n"
      : kind == Kind::Pattern ? "/Pattern cs /Blue scn /Pattern CS /Red SCN 2 w BT /F 20 Tf 2 Tr 20 64 Td (A) Tj ET\n"
                             : "0 0 0 rg BT /F 20 Tf 20 64 Td (A) Tj ET\n";
  const std::string earlier = "1 0 0 rg 4 4 8 8 re f\n";
  const std::string form_entries = "/Type /XObject /Subtype /Form /BBox [0 0 128 128] /Resources << " + resources + " >>";
  const bool separate_pattern = place == Place::Page && kind == Kind::Pattern;
  const std::string content = separate_pattern ? earlier : place == Place::Annotation ? "" : earlier +
      (place == Place::Page ? target : "q /Outer Do Q\n");
  std::string pixels;
  for (int i = 0; i < 12; ++i) pixels += std::string("\0\0\xff", 3);
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << " + resources +
          " >> /Contents " + (separate_pattern ? "[4 0 R 15 0 R]" : "4 0 R") + " " +
          (place == Place::Annotation ? "/Annots [14 0 R]" : "") + " >>",
      PdfStream("", content),
      "<< /Type /Font /Subtype /TrueType /BaseFont /Ahem /FirstChar 65 /LastChar 65 /Widths [1000] /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R >>",
      "<< /Type /FontDescriptor /FontName /Ahem /Flags 32 /FontBBox [0 -200 1000 800] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 800 /StemV 80 /FontFile2 7 0 R >>",
      PdfStream("/Length1 " + std::to_string(font.size()), std::string(font.begin(), font.end())),
      PdfStream("/Type /XObject /Subtype /Image /Width 4 /Height 3 /ColorSpace /DeviceRGB /BitsPerComponent 8", pixels),
      PdfStream(form_entries, place == Place::NestedForm ? "/Inner Do\n" : target),
      PdfStream(form_entries, target),
      PdfStream("/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 8 8] /XStep 8 /YStep 8 /Resources << >>", "0 0 1 rg 0 0 8 8 re f"),
      PdfStream("/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 8 8] /XStep 8 /YStep 8 /Resources << >>", "1 0 0 rg 0 0 8 8 re f"),
      PdfStream(form_entries, earlier + target),
      "<< /Type /Annot /Subtype /Stamp /Rect [0 0 128 128] /F 4 /AP << /N 13 0 R >> >>",
      PdfStream("", target)});
}

Raster Copy(const Api& api, FPDF_BITMAP bitmap) {
  Raster result{api.FPDFBitmap_GetWidth(bitmap), api.FPDFBitmap_GetHeight(bitmap), {}};
  const auto* bytes = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap));
  const int stride = api.FPDFBitmap_GetStride(bitmap), format = api.FPDFBitmap_GetFormat(bitmap);
  const int channels = format == FPDFBitmap_Gray ? 1 : format == FPDFBitmap_BGR ? 3 : 4;
  EXPECT_GT(result.width, 0); EXPECT_GT(result.height, 0);
  if (!bytes || result.width <= 0 || result.height <= 0) return {};
  for (int y = 0; y < result.height; ++y) for (int x = 0; x < result.width; ++x) {
    const auto* p = bytes + y * stride + x * channels;
    if (channels == 1) result.pixels.insert(result.pixels.end(), {p[0], p[0], p[0], 255});
    else result.pixels.insert(result.pixels.end(), {p[0], p[1], p[2], static_cast<uint8_t>(format == FPDFBitmap_BGRA ? p[3] : 255)});
  }
  return result;
}

void Pixel(const Raster& image, int x, int y, int b, int g, int r) {
  ASSERT_GE(x, 0); ASSERT_LT(x, image.width); ASSERT_GE(y, 0); ASSERT_LT(y, image.height);
  const size_t at = (y * image.width + x) * 4;
  EXPECT_EQ(image.pixels.at(at), b);
  EXPECT_EQ(image.pixels.at(at + 1), g);
  EXPECT_EQ(image.pixels.at(at + 2), r);
}

FPDF_PAGEOBJECT Target(const Api& api, FPDF_PAGE page, FPDF_ANNOTATION annot, Place place, int page_index) {
  if (place == Place::Annotation) return api.FPDFAnnot_GetObject(annot, page_index);
  auto object = api.FPDFPage_GetObject(page, page_index);
  if (place == Place::Form || place == Place::NestedForm) object = api.FPDFFormObj_GetObject(object, 0);
  if (place == Place::NestedForm) object = api.FPDFFormObj_GetObject(object, 0);
  return object;
}

void ReadShared(const Api& api, FPDF_DOCUMENT doc, FPDF_PAGE page,
                FPDF_PAGEOBJECT object, Place place, Kind kind) {
  ASSERT_NE(object, nullptr);
  Handle wrong(api.FPDFPageObj_CreateNewRect(1, 1, 2, 2), api.FPDFPageObj_Destroy);
  ASSERT_NE(wrong.get(), nullptr);
  Handle textpage(api.FPDFText_LoadPage(page), api.FPDFText_ClosePage);
  ASSERT_NE(textpage.get(), nullptr);
  EXPECT_EQ(api.FPDFTextObj_GetText(wrong.get(), textpage.get(), nullptr, 0), 0u);
  EXPECT_EQ(api.FPDFTextObj_GetRenderedBitmap(doc, page, wrong.get(), 1), nullptr);
  EXPECT_EQ(api.FPDFImageObj_GetRenderedBitmap(doc, page, wrong.get()), nullptr);
  FPDF_IMAGEOBJ_METADATA invalid{};
  EXPECT_FALSE(api.FPDFImageObj_GetImageMetadata(wrong.get(), page, &invalid));
  EXPECT_EQ(api.FPDFPageObj_GetRenderedFillPattern(doc, wrong.get()), nullptr);
  EXPECT_EQ(api.FPDFPageObj_GetRenderedStrokePattern(doc, wrong.get()), nullptr);
  if (kind == Kind::Image) {
    EXPECT_EQ(api.FPDFTextObj_GetText(object, textpage.get(), nullptr, 0), 0u);
    EXPECT_EQ(api.FPDFTextObj_GetRenderedBitmap(doc, page, object, 1), nullptr);
    FPDF_IMAGEOBJ_METADATA metadata{};
    ASSERT_TRUE(api.FPDFImageObj_GetImageMetadata(object, page, &metadata));
    EXPECT_EQ(metadata.width, 4u); EXPECT_EQ(metadata.height, 3u);
    EXPECT_EQ(metadata.bits_per_pixel, 24u);
    EXPECT_EQ(metadata.colorspace, FPDF_COLORSPACE_DEVICERGB);
    EXPECT_NEAR(metadata.horizontal_dpi, 9, .001);
    EXPECT_NEAR(metadata.vertical_dpi, 9, .001);
    Handle bitmap(api.FPDFImageObj_GetRenderedBitmap(doc, page, object), api.FPDFBitmap_Destroy);
    ASSERT_NE(bitmap.get(), nullptr);
    const auto pixels = Copy(api, bitmap.get());
    EXPECT_EQ(pixels.width, 32); EXPECT_EQ(pixels.height, 24);
    Pixel(pixels, 16, 12, 255, 0, 0);
    return;
  }
  ExpectBytes([&](void* b, unsigned long n) {
    return api.FPDFTextObj_GetText(object, textpage.get(), static_cast<FPDF_WCHAR*>(b), n);
  }, Utf16(place == Place::Annotation ? u"" : u"A"));
  EXPECT_EQ(api.FPDFImageObj_GetRenderedBitmap(doc, page, object), nullptr);
  EXPECT_FALSE(api.FPDFImageObj_GetImageMetadata(object, page, &invalid));
  Handle bitmap(api.FPDFTextObj_GetRenderedBitmap(doc, page, object, 1), api.FPDFBitmap_Destroy);
  ASSERT_NE(bitmap.get(), nullptr);
  const auto pixels = Copy(api, bitmap.get());
  EXPECT_GE(pixels.width, 20); EXPECT_LE(pixels.width, 24);
  EXPECT_GE(pixels.height, 20); EXPECT_LE(pixels.height, 24);
  Pixel(pixels, pixels.width / 2, pixels.height / 2, kind == Kind::Pattern ? 255 : 0, 0, 0);
  for (bool fill : {true, false}) {
    Handle pattern(fill ? api.FPDFPageObj_GetRenderedFillPattern(doc, object)
                        : api.FPDFPageObj_GetRenderedStrokePattern(doc, object), api.FPDFBitmap_Destroy);
    if (kind != Kind::Pattern) { EXPECT_EQ(pattern.get(), nullptr); continue; }
    ASSERT_NE(pattern.get(), nullptr);
    const auto tile = Copy(api, pattern.get());
    EXPECT_EQ(tile.width, 8); EXPECT_EQ(tile.height, 8);
    Pixel(tile, 4, 4, fill ? 255 : 0, 0, fill ? 0 : 255);
  }
}

void Exercise(Place place, Kind kind, const std::string& name) {
  const auto input = Scene(place, kind);
  Artifacts artifacts;
  std::array<Raster, 2> final_images;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    SCOPED_TRACE(writer->name() + "-" + name);
    Handle doc(api.FPDF_LoadMemDocument64(input.data(), input.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    Handle annot(place == Place::Annotation ? api.FPDFPage_GetAnnot(page.get(), 0) : nullptr, api.FPDFPage_CloseAnnot);
    if (place == Place::Annotation) { ASSERT_NE(annot.get(), nullptr); }
    // Borrow BEFORE removing the earlier sibling. Never access its removed
    // handle; only the target and its owning parent remain alive.
    const auto object = Target(api, page.get(), annot.get(), place, 1);
    ASSERT_NE(object, nullptr);
    ASSERT_NO_FATAL_FAILURE(ReadShared(api, doc.get(), page.get(), object, place, kind));
    const auto before = Render(api, page.get(), 128, 128, FPDF_ANNOT);
    Pixel(before, 8, 120, 0, 0, 255);
    FPDF_PAGEOBJECT parent = nullptr;
    if (place == Place::Annotation) {
      ASSERT_TRUE(api.FPDFAnnot_RemoveObject(annot.get(), 0));
      ASSERT_EQ(api.FPDFAnnot_GetObjectCount(annot.get()), 1);
    } else {
      if (place == Place::Form || place == Place::NestedForm)
        parent = api.FPDFPage_GetObject(page.get(), 1);
      const auto earlier = api.FPDFPage_GetObject(page.get(), 0);
      ASSERT_TRUE(api.FPDFPage_RemoveObject(page.get(), earlier));
      api.FPDFPageObj_Destroy(earlier);
      ASSERT_EQ(api.FPDFPage_CountObjects(page.get()), 1);
    }
    // These calls use the original borrowed handle with its now-stale index.
    ASSERT_NO_FATAL_FAILURE(ReadShared(api, doc.get(), page.get(), object, place, kind));
    auto moved = parent ? parent : object;
    FS_MATRIX matrix{};
    ASSERT_TRUE(api.FPDFPageObj_GetMatrix(moved, &matrix));
    const bool preserved_pattern_stream = place == Place::Page && kind == Kind::Pattern;
    // Native ProcessGraphics writes only RGB/Gray colors, so regenerating a
    // dirty patterned text object loses its pattern operators. Give the
    // removed sibling a separate content stream and keep the patterned target
    // clean. Removing the visible sibling still changes the page and exercises
    // the original target handle's stale index before and after generation.
    if (!preserved_pattern_stream) {
      matrix.e += 8;
      ASSERT_TRUE(api.FPDFPageObj_SetMatrix(moved, &matrix));
    }
    if (place == Place::Annotation) {
      ASSERT_TRUE(api.FPDFAnnot_UpdateObject(annot.get(), object));
    } else {
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    }
    ASSERT_NO_FATAL_FAILURE(ReadShared(api, doc.get(), page.get(), object, place, kind));
    final_images[backend] = Render(api, page.get(), 128, 128, FPDF_ANNOT);
    Pixel(final_images[backend], 8, 120, 255, 255, 255);
    if (kind == Kind::Image) Pixel(final_images[backend], 88, 84, 255, 0, 0);
    else Pixel(final_images[backend], preserved_pattern_stream ? 30 : 38, 58, kind == Kind::Pattern ? 255 : 0, 0, 0);
    EXPECT_NE(before.pixels, final_images[backend].pixels);
    const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-" + name);
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api();
      SCOPED_TRACE(reader->name());
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      Handle loaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(loaded.get(), nullptr);
      Handle restored_annot(place == Place::Annotation ? read.FPDFPage_GetAnnot(loaded.get(), 0) : nullptr, read.FPDFPage_CloseAnnot);
      if (place == Place::Annotation) { ASSERT_NE(restored_annot.get(), nullptr); }
      const auto restored = Target(read, loaded.get(), restored_annot.get(), place, 0);
      ASSERT_NO_FATAL_FAILURE(ReadShared(read, reopened.get(), loaded.get(), restored, place, kind));
      const auto actual = Render(read, loaded.get(), 128, 128, FPDF_ANNOT);
      Pixel(actual, 8, 120, 255, 255, 255);
      CompareRaster(final_images[backend], actual, artifacts, writer->name() + "-" + name + "-to-" + reader->name(), RasterTolerance{2, 0, 0.025});
    }
    ++backend;
  }
  CompareRaster(final_images[0], final_images[1], artifacts, name + "-native-rust", RasterTolerance{2, 0, 0.025});
}

TEST(CoverageBorrowedObjectIdentity, PageTextSharedReadersSurviveEarlierSiblingRemoval) {
  Exercise(Place::Page, Kind::Text, "page-text-stale-index");
}
TEST(CoverageBorrowedObjectIdentity, PageImageSharedReadersKeepPixelsAndDpiAfterEarlierSiblingRemoval) {
  Exercise(Place::Page, Kind::Image, "page-image-stale-index");
}
TEST(CoverageBorrowedObjectIdentity, PatternTextSharedReadersKeepBothTilesAfterPageSiblingRemoval) {
  Exercise(Place::Page, Kind::Pattern, "page-pattern-stale-index");
}
TEST(CoverageBorrowedObjectIdentity, FormTextAndNestedTextSurviveTheirParentPageIndexMoving) {
  Exercise(Place::Form, Kind::Text, "form-text-parent-index");
  Exercise(Place::NestedForm, Kind::Text, "nested-text-parent-index");
}
TEST(CoverageBorrowedObjectIdentity, FormImageAndNestedImageSurviveTheirParentPageIndexMoving) {
  Exercise(Place::Form, Kind::Image, "form-image-parent-index");
  Exercise(Place::NestedForm, Kind::Image, "nested-image-parent-index");
}
TEST(CoverageBorrowedObjectIdentity, FormPatternTilesSurviveRecursiveParentIndexResolution) {
  Exercise(Place::Form, Kind::Pattern, "form-pattern-parent-index");
  Exercise(Place::NestedForm, Kind::Pattern, "nested-pattern-parent-index");
}
TEST(CoverageBorrowedObjectIdentity, AnnotationTextBitmapSurvivesEarlierAppearanceObjectRemoval) {
  Exercise(Place::Annotation, Kind::Text, "annotation-text-stale-index");
}
TEST(CoverageBorrowedObjectIdentity, AnnotationImageMetadataAndBitmapSurviveEarlierAppearanceObjectRemoval) {
  Exercise(Place::Annotation, Kind::Image, "annotation-image-stale-index");
}
}  // namespace
}  // namespace pdfoxium::integration
