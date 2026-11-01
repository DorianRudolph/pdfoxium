#include "test_support.h"

#include <gtest/gtest.h>

#include <array>
#include <cstdint>
#include <string>
#include <utility>
#include <vector>

namespace pdfoxium::integration {
namespace {

constexpr int kWidth = 96;
constexpr int kHeight = 64;

struct Snapshot {
  double width = 0;
  double height = 0;
  int objects = 0;
  // Packed BGRA rows, independent of the library's internal row stride.
  Raster raster;
};

void CreateRectangle(const Api& api, FPDF_DOCUMENT document) {
  EXPECT_EQ(api.FPDF_GetPageCount(document), 0);
  Handle page(api.FPDFPage_New(document, 0, kWidth, kHeight), api.FPDF_ClosePage);
  ASSERT_NE(page.get(), nullptr);
  EXPECT_EQ(api.FPDF_GetPageCount(document), 1);
  EXPECT_EQ(api.FPDFPage_CountObjects(page.get()), 0);

  Handle rectangle(api.FPDFPageObj_CreateNewRect(16, 16, 32, 24),
                   api.FPDFPageObj_Destroy);
  ASSERT_NE(rectangle.get(), nullptr);
  EXPECT_EQ(api.FPDFPageObj_GetType(rectangle.get()), FPDF_PAGEOBJ_PATH);
  ASSERT_TRUE(api.FPDFPageObj_SetFillColor(rectangle.get(), 224, 32, 64, 255));
  ASSERT_TRUE(api.FPDFPath_SetDrawMode(rectangle.get(), FPDF_FILLMODE_WINDING, 0));
  api.FPDFPage_InsertObject(page.get(), rectangle.release());
  EXPECT_EQ(api.FPDFPage_CountObjects(page.get()), 1);
  ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
}

void Observe(const Api& api, FPDF_DOCUMENT document, Snapshot* observation) {
  ASSERT_EQ(api.FPDF_GetPageCount(document), 1);
  Handle page(api.FPDF_LoadPage(document, 0), api.FPDF_ClosePage);
  ASSERT_NE(page.get(), nullptr);
  observation->width = api.FPDF_GetPageWidth(page.get());
  observation->height = api.FPDF_GetPageHeight(page.get());
  observation->objects = api.FPDFPage_CountObjects(page.get());
  EXPECT_DOUBLE_EQ(observation->width, kWidth);
  EXPECT_DOUBLE_EQ(observation->height, kHeight);
  EXPECT_EQ(observation->objects, 1);
  EXPECT_EQ(api.FPDFPage_GetRotation(page.get()), 0);

  observation->raster = Render(api, page.get(), kWidth, kHeight);
  ASSERT_EQ(observation->raster.pixels.size(), kWidth * kHeight * 4);
  const auto pixel = [&](int x, int y) {
    const auto offset = static_cast<size_t>((y * kWidth + x) * 4);
    return std::array<uint8_t, 4>{observation->raster.pixels[offset],
                                 observation->raster.pixels[offset + 1],
                                 observation->raster.pixels[offset + 2],
                                 observation->raster.pixels[offset + 3]};
  };
  // Expected output is checked independently of the native/Rust comparison.
  EXPECT_EQ(pixel(4, 4), (std::array<uint8_t, 4>{255, 255, 255, 255}));
  EXPECT_EQ(pixel(24, 32), (std::array<uint8_t, 4>{64, 32, 224, 255}));
}

void Compare(const Snapshot& expected, const Snapshot& actual,
             Artifacts* artifacts, const std::string& name) {
  EXPECT_DOUBLE_EQ(expected.width, actual.width);
  EXPECT_DOUBLE_EQ(expected.height, actual.height);
  EXPECT_EQ(expected.objects, actual.objects);
  CompareRaster(expected.raster, actual.raster, *artifacts, name);
}

TEST(Smoke, InvalidDocumentReportsFormatError) {
  constexpr char invalid[] = "This is not a PDF document.\n";
  std::array<unsigned long, 2> errors{};
  size_t index = 0;
  for (const auto* library : {&libraries().native, &libraries().rust}) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_LoadMemDocument64(invalid, sizeof(invalid) - 1,
                                              nullptr),
                    api.FPDF_CloseDocument);
    ASSERT_EQ(document.get(), nullptr);
    errors[index++] = api.FPDF_GetLastError();
    EXPECT_EQ(errors[index - 1], static_cast<unsigned long>(FPDF_ERR_FORMAT));
    // A subsequent real operation succeeds in the same initialized session.
    Handle fresh(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(fresh.get(), nullptr);
    EXPECT_EQ(api.FPDF_GetPageCount(fresh.get()), 0);
  }
  EXPECT_EQ(errors[0], errors[1]);
}

TEST(Smoke, CreatePageAndRenderRectangle) {
  Artifacts artifacts;
  std::array<Snapshot, 2> observations;
  size_t index = 0;
  for (const auto* library : {&libraries().native, &libraries().rust}) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(CreateRectangle(api, document.get()));
    ASSERT_NO_FATAL_FAILURE(Observe(api, document.get(), &observations[index]));
    artifacts.Image(library->name(), observations[index].raster);
    ++index;
    Handle fresh(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(fresh.get(), nullptr);
    EXPECT_EQ(api.FPDF_GetPageCount(fresh.get()), 0);
  }
  Compare(observations[0], observations[1], &artifacts, "native-rust");
}

TEST(Smoke, SaveAndReloadWithBothReaders) {
  Artifacts artifacts;
  std::array<Snapshot, 2> before;
  std::array<std::vector<uint8_t>, 2> saved;
  const std::array<const Library*, 2> implementations = {&libraries().native,
                                                        &libraries().rust};
  for (size_t writer_index = 0; writer_index < implementations.size();
       ++writer_index) {
    const auto& writer = *implementations[writer_index];
    SCOPED_TRACE("writer=" + writer.name());
    const auto& api = writer.api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(CreateRectangle(api, document.get()));
    ASSERT_NO_FATAL_FAILURE(Observe(api, document.get(), &before[writer_index]));
    artifacts.Image(writer.name() + "-before", before[writer_index].raster);
    MemoryWriter sink;
    const auto result =
        api.FPDF_SaveAsCopy(document.get(), &sink, FPDF_NO_INCREMENTAL);
    saved[writer_index] = std::move(sink.bytes);
    artifacts.Pdf(writer.name(), saved[writer_index]);
    ASSERT_TRUE(result);
    EXPECT_GT(sink.calls, 0u);
    EXPECT_FALSE(sink.failed);
    ASSERT_FALSE(saved[writer_index].empty());
  }
  ASSERT_NO_FATAL_FAILURE(Compare(before[0], before[1], &artifacts, "before"));

  for (size_t writer_index = 0; writer_index < implementations.size();
       ++writer_index) {
    // Saved bytes remain alive until every document backed by them is closed.
    const auto& bytes = saved[writer_index];
    std::array<Snapshot, 2> reopened;
    for (size_t reader_index = 0; reader_index < implementations.size();
         ++reader_index) {
      const auto& reader = *implementations[reader_index];
      const std::string name = implementations[writer_index]->name() + "-to-" +
                               reader.name();
      SCOPED_TRACE(name);
      const auto& api = reader.api();
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(),
                                                nullptr),
                      api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr) << "load error=" << api.FPDF_GetLastError();
      ASSERT_NO_FATAL_FAILURE(Observe(api, document.get(), &reopened[reader_index]));
      artifacts.Image(name, reopened[reader_index].raster);
      Compare(before[writer_index], reopened[reader_index], &artifacts, name);
    }
    Compare(reopened[0], reopened[1], &artifacts,
            implementations[writer_index]->name() + "-readers");
  }
}

}  // namespace
}  // namespace pdfoxium::integration
