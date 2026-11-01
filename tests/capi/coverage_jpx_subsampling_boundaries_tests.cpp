#include "text_fixture.h"

#include <array>
#include <string>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
bool IsNative(const Api& api) {
  for (const auto* library : Implementations()) {
    if (&library->api() == &api) return library->name() == "native";
  }
  ADD_FAILURE() << "Unknown API owner";
  return false;
}

std::string FixtureName(int vertical, int width, int height, int precision,
                        bool alpha, bool pattern) {
  return std::string("sycc-42") + (vertical == 1 ? "2" : "0") + "-" +
      std::to_string(width) + "x" + std::to_string(height) + "-" +
      std::to_string(precision) + (alpha ? "-alpha" : "") +
      (pattern ? "-pattern" : "") + ".jp2";
}
Bytes ImagePdf(const Bytes& image, int width, int height, bool alpha) {
  const auto w = std::to_string(width), h = std::to_string(height);
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 " + w + " " + h +
          "] /Resources << /XObject << /I 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", "q " + w + " 0 0 " + h + " 0 0 cm /I Do Q"),
      PdfStream("/Type /XObject /Subtype /Image /Width " + w + " /Height " + h +
                    " /Filter /JPXDecode" + (alpha ? " /SMaskInData 1" : ""),
                std::string(image.begin(), image.end()))});
}
std::array<int, 3> ExpectedPixel(int x, int y, int vertical, bool alpha, bool pattern,
                                bool native) {
  // Native LoadJpxBitmap preblends unassociated samples with white, then
  // applies the same embedded opacity again (documented JPX oracle defect).
  if (!pattern && alpha && native) return {215,215,215};
  if (!pattern) return alpha ? std::array<int, 3>{177,177,177}
                             : std::array<int, 3>{100,100,100};
  // Independent table for the fixture's five Cr and three Cb sample values.
  // These are the standard SYCC matrix contributions, truncated to integers.
  constexpr int red[] = {0,11,22,33,44};
  constexpr int blue[] = {-28,-14,0};
  constexpr int green[5][3] = {{5,2,0}, {0,-2,-5}, {-5,-8,-11},
                              {-11,-14,-17}, {-17,-20,-22}};
  const int cy = y / vertical, cx = x / 2, luma = 40 + 20 * x + 10 * y;
  return {luma + red[cy], luma + green[cy][cx], luma + blue[cx]};
}
void Inspect(const Api& api, FPDF_PAGE page, const Raster& raster,
             int width, int height, int vertical, bool alpha, bool pattern) {
  ASSERT_EQ(api.FPDFPage_CountObjects(page), 1);
  const auto object = api.FPDFPage_GetObject(page, 0);
  ASSERT_EQ(api.FPDFPageObj_GetType(object), FPDF_PAGEOBJ_IMAGE);
  FPDF_IMAGEOBJ_METADATA metadata{};
  ASSERT_TRUE(api.FPDFImageObj_GetImageMetadata(object, page, &metadata));
  EXPECT_EQ(metadata.width, unsigned(width)); EXPECT_EQ(metadata.height, unsigned(height));
  ASSERT_EQ(raster.width, width); ASSERT_EQ(raster.height, height);
  int largest_error = 0;
  for (int y = 0; y < height; ++y) for (int x = 0; x < width; ++x) {
    const auto expected = ExpectedPixel(x, y, vertical, alpha, pattern, IsNative(api));
    const auto at = (size_t(y) * width + x) * 4;
    for (int c = 0; c < 3; ++c)
      largest_error = std::max(largest_error, std::abs(int(raster.pixels[at + c]) - expected[2 - c]));
    EXPECT_EQ(raster.pixels[at + 3], 255);
  }
  EXPECT_LE(largest_error, 2) << "Known samples, chroma cells and edge replication";
}
Raster SeparateAlphaControl(const Api& api, int width, int height) {
  Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
  if (!document.get()) { ADD_FAILURE(); return {}; }
  Handle page(api.FPDFPage_New(document.get(), 0, width, height), api.FPDF_ClosePage);
  if (!page.get()) { ADD_FAILURE(); return {}; }
  Handle bitmap(api.FPDFBitmap_CreateEx(width, height, FPDFBitmap_BGRA, nullptr, 0),
                api.FPDFBitmap_Destroy);
  if (!bitmap.get()) { ADD_FAILURE(); return {}; }
  EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, width, height, 0x80646464));
  Handle image(api.FPDFPageObj_NewImageObj(document.get()), api.FPDFPageObj_Destroy);
  if (!image.get()) { ADD_FAILURE(); return {}; }
  EXPECT_TRUE(api.FPDFImageObj_SetBitmap(nullptr, 0, image.get(), bitmap.get()));
  const FS_MATRIX matrix{float(width),0,0,float(height),0,0};
  EXPECT_TRUE(api.FPDFPageObj_SetMatrix(image.get(), &matrix));
  EXPECT_TRUE(api.FPDFPage_InsertObject(page.get(), image.get()));
  image.release();
  EXPECT_TRUE(api.FPDFPage_GenerateContent(page.get()));
  const auto raster = Render(api, page.get(), width, height, FPDF_RENDER_NO_SMOOTHIMAGE);
  int error = 0;
  for (size_t at = 0; at < raster.pixels.size(); at += 4) {
    for (int c = 0; c < 3; ++c) error = std::max(error, std::abs(int(raster.pixels[at + c]) - 177));
    EXPECT_EQ(raster.pixels[at + 3], 255);
  }
  EXPECT_LE(error, 2) << "Independent bitmap-to-PDF image and soft mask";
  return raster;
}
void ExerciseJpx(int vertical, int width, int height, int precision,
                 bool alpha = false, bool pattern = false) {
  const auto name = FixtureName(vertical, width, height, precision, alpha, pattern);
  SCOPED_TRACE(name);
  const auto image = text_fixture::Read(TestRoot() / "tests/capi/fixtures/jpx-sycc" / name);
  ASSERT_FALSE(image.empty());
  const auto bytes = ImagePdf(image, width, height, alpha);
  Artifacts artifacts;
  std::array<Raster, 2> originals;
  std::array<Raster, 2> controls;
  size_t backend = 0;
  auto render = [&](const Api& api, FPDF_PAGE page) {
    const auto raster = Render(api, page, width, height, FPDF_RENDER_NO_SMOOTHIMAGE);
    Inspect(api, page, raster, width, height, vertical, alpha, pattern);
    return raster;
  };
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name()); const auto& api = writer->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    originals[backend] = render(api, page.get());
    if (alpha) {
      artifacts.Image(writer->name() + "-original-" + name, originals[backend]);
      controls[backend] = SeparateAlphaControl(api, width, height);
      if (!IsNative(api)) CompareRaster(originals[backend], controls[backend], artifacts,
                                        writer->name() + "-alpha-control-" + name, 2);
    }
    const auto saved = SaveDocument(api, document.get(), artifacts, writer->name() + "-" + name);
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name()); const auto& read = reader->api();
      Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(reopened.get(), nullptr);
      Handle reloaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(reloaded.get(), nullptr);
      const auto actual = render(read, reloaded.get());
      if (!alpha || writer == reader) {
        CompareRaster(originals[backend], actual, artifacts,
                      writer->name() + "-" + reader->name() + "-" + name, 2);
      } else if (!IsNative(read)) {
        CompareRaster(actual, SeparateAlphaControl(read, width, height), artifacts,
                      writer->name() + "-" + reader->name() + "-alpha-control-" + name, 2);
      }
    }
    ++backend;
  }
  CompareRaster(alpha ? controls[0] : originals[0], alpha ? controls[1] : originals[1],
                artifacts, name + "-engines", 2);
}

TEST(CoverageJpxSubsamplingBoundaries, Sycc422EvenDimensionsExpandEverySharedChromaPair) {
  ExerciseJpx(1, 4, 4, 8);
}
TEST(CoverageJpxSubsamplingBoundaries, Sycc422OddLastColumnsAndRowsKeepTheirDecodedSamples) {
  for (const auto& size : {std::array<int,2>{5,4}, {4,5}, {5,5}})
    ExerciseJpx(1, size[0], size[1], 8);
}
TEST(CoverageJpxSubsamplingBoundaries, Sycc420EvenDimensionsExpandEveryTwoByTwoChromaCell) {
  ExerciseJpx(2, 4, 4, 8);
}
TEST(CoverageJpxSubsamplingBoundaries, Sycc420OddLastColumnsAndRowsPreserveTheCorner) {
  for (const auto& size : {std::array<int,2>{5,4}, {4,5}, {5,5}})
    ExerciseJpx(2, size[0], size[1], 8);
}
TEST(CoverageJpxSubsamplingBoundaries, TwelveAndSixteenBit422ConvertBeforePublicEightBitOutput) {
  for (int precision : {12,16}) for (int width : {4,5}) for (int height : {4,5})
    ExerciseJpx(1, width, height, precision);
}
TEST(CoverageJpxSubsamplingBoundaries, TwelveAndSixteenBit420ConvertBeforePublicEightBitOutput) {
  for (int precision : {12,16}) for (int width : {4,5}) for (int height : {4,5})
    ExerciseJpx(2, width, height, precision);
}
TEST(CoverageJpxSubsamplingBoundaries, Subsampled422And420PreserveTheIndependentAlphaPlane) {
  for (int vertical : {1,2}) for (int width : {4,5}) for (int height : {4,5})
    ExerciseJpx(vertical, width, height, 8, true);
}
TEST(CoverageJpxSubsamplingBoundaries, Colored422PatternsKeepRowStrideAndLastColumnChroma) {
  for (int width : {4,5}) for (int height : {4,5})
    ExerciseJpx(1, width, height, 8, false, true);
}
TEST(CoverageJpxSubsamplingBoundaries, Colored420PatternsKeepSharedChromaAndAllOddEdges) {
  for (int width : {4,5}) for (int height : {4,5})
    ExerciseJpx(2, width, height, 8, false, true);
}
}  // namespace
}  // namespace pdfoxium::integration
