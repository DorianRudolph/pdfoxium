#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

using Labels = std::vector<int>;

double LabelWidth(int label) { return 80.25 + label * 8; }
double LabelHeight(int label) { return 64.5 + label * 4; }

void AddMarkedPage(const Api& api, FPDF_DOCUMENT document, int index, int label) {
  Handle page(api.FPDFPage_New(document, index, LabelWidth(label),
                               LabelHeight(label)),
              api.FPDF_ClosePage);
  ASSERT_NE(page.get(), nullptr);
  Handle mark(api.FPDFPageObj_CreateNewRect(0, 0,
                                           static_cast<float>(LabelWidth(label)),
                                           static_cast<float>(LabelHeight(label))),
              api.FPDFPageObj_Destroy);
  ASSERT_NE(mark.get(), nullptr);
  ASSERT_TRUE(api.FPDFPageObj_SetFillColor(mark.get(), 32 + label * 24,
                                          48 + label * 16, 64 + label * 8, 255));
  ASSERT_TRUE(api.FPDFPath_SetDrawMode(mark.get(), FPDF_FILLMODE_WINDING, 0));
  api.FPDFPage_InsertObject(page.get(), mark.release());
  ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
}

void Populate(const Api& api, FPDF_DOCUMENT document, const Labels& labels) {
  for (size_t index = 0; index < labels.size(); ++index) {
    ASSERT_NO_FATAL_FAILURE(
        AddMarkedPage(api, document, static_cast<int>(index), labels[index]));
  }
}

void ObserveMarkedPages(const Api& api, FPDF_DOCUMENT document,
                        const Labels& labels, std::vector<Raster>* rasters) {
  ASSERT_EQ(api.FPDF_GetPageCount(document), static_cast<int>(labels.size()));
  rasters->clear();
  for (size_t index = 0; index < labels.size(); ++index) {
    SCOPED_TRACE(index);
    Handle page(api.FPDF_LoadPage(document, static_cast<int>(index)),
                api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    const int label = labels[index];
    EXPECT_DOUBLE_EQ(api.FPDF_GetPageWidth(page.get()), LabelWidth(label));
    EXPECT_DOUBLE_EQ(api.FPDF_GetPageHeight(page.get()), LabelHeight(label));
    ASSERT_EQ(api.FPDFPage_CountObjects(page.get()), 1);
    FPDF_PAGEOBJECT mark = api.FPDFPage_GetObject(page.get(), 0);
    ASSERT_NE(mark, nullptr);
    unsigned int red = 0, green = 0, blue = 0, alpha = 0;
    ASSERT_TRUE(api.FPDFPageObj_GetFillColor(mark, &red, &green, &blue, &alpha));
    EXPECT_EQ(red, 32u + label * 24);
    EXPECT_EQ(green, 48u + label * 16);
    EXPECT_EQ(blue, 64u + label * 8);
    EXPECT_EQ(alpha, 255u);
    Raster raster = Render(api, page.get(), 80, 64);
    ASSERT_EQ(raster.pixels.size(), 80u * 64 * 4);
    const size_t center = (32 * 80 + 40) * 4;
    EXPECT_EQ(raster.pixels[center], blue);
    EXPECT_EQ(raster.pixels[center + 1], green);
    EXPECT_EQ(raster.pixels[center + 2], red);
    EXPECT_EQ(raster.pixels[center + 3], alpha);
    rasters->push_back(std::move(raster));
  }
}

void ComparePages(const std::vector<Raster>& expected,
                  const std::vector<Raster>& actual, Artifacts* artifacts,
                  const std::string& name) {
  ASSERT_EQ(expected.size(), actual.size());
  for (size_t index = 0; index < expected.size(); ++index) {
    const std::string page_name = name + "-page-" + std::to_string(index);
    artifacts->Image(page_name + "-expected", expected[index]);
    artifacts->Image(page_name + "-actual", actual[index]);
    CompareRaster(expected[index], actual[index], *artifacts, page_name);
  }
}

// Each writer is called by the surrounding dual-library loop; both readers
// reopen these exact bytes. Backing storage outlives every borrowed document.
void RoundTripPages(const Library& writer, FPDF_DOCUMENT document,
                    const Labels& labels, Artifacts* artifacts,
                    const std::string& name,
                    std::vector<Raster>* original = nullptr) {
  std::vector<Raster> before;
  ASSERT_NO_FATAL_FAILURE(
      ObserveMarkedPages(writer.api(), document, labels, &before));
  const auto bytes = SaveDocument(writer.api(), document, *artifacts,
                                  writer.name() + "-" + name);
  ASSERT_FALSE(bytes.empty());
  std::array<std::vector<Raster>, 2> reopened;
  size_t index = 0;
  for (const auto* reader : Implementations()) {
    const std::string comparison =
        writer.name() + "-to-" + reader->name() + "-" + name;
    SCOPED_TRACE(comparison);
    const auto& api = reader->api();
    Handle loaded(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr),
                  api.FPDF_CloseDocument);
    ASSERT_NE(loaded.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(
        ObserveMarkedPages(api, loaded.get(), labels, &reopened[index]));
    ComparePages(before, reopened[index], artifacts, comparison);
    ++index;
  }
  ComparePages(reopened[0], reopened[1], artifacts,
               writer.name() + "-readers-" + name);
  if (original) *original = std::move(before);
}

TEST(Pages, IndependentDocumentsSurviveOtherDocumentCleanup) {
  Artifacts artifacts;
  std::array<std::vector<Raster>, 2> observations;
  size_t implementation = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle first(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle second(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(first.get(), nullptr);
    ASSERT_NE(second.get(), nullptr);
    EXPECT_NE(first.get(), second.get());
    EXPECT_EQ(api.FPDF_GetPageCount(first.get()), 0);
    EXPECT_EQ(api.FPDF_GetPageCount(second.get()), 0);
    ASSERT_NO_FATAL_FAILURE(Populate(api, first.get(), {0, 1}));
    ASSERT_NO_FATAL_FAILURE(Populate(api, second.get(), {4}));
    ASSERT_NO_FATAL_FAILURE(
        RoundTripPages(*library, first.get(), {0, 1}, &artifacts, "first"));
    ASSERT_NO_FATAL_FAILURE(
        RoundTripPages(*library, second.get(), {4}, &artifacts, "second"));
    api.FPDF_CloseDocument(first.release());
    ASSERT_NO_FATAL_FAILURE(AddMarkedPage(api, second.get(), 1, 3));
    ASSERT_NO_FATAL_FAILURE(RoundTripPages(*library, second.get(), {4, 3},
                                          &artifacts, "survivor",
                                          &observations[implementation++]));
  }
  ComparePages(observations[0], observations[1], &artifacts, "implementations");
}

TEST(Pages, InsertAtBeginningMiddleEndAndClampedIndexes) {
  Artifacts artifacts;
  std::array<std::vector<Raster>, 2> observations;
  size_t index = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    EXPECT_EQ(api.FPDFPage_New(nullptr, 0, 80, 64), nullptr);
    ASSERT_NO_FATAL_FAILURE(AddMarkedPage(api, document.get(), 0, 1));
    ASSERT_NO_FATAL_FAILURE(AddMarkedPage(api, document.get(), 99, 3));
    ASSERT_NO_FATAL_FAILURE(AddMarkedPage(api, document.get(), -1, 0));
    ASSERT_NO_FATAL_FAILURE(AddMarkedPage(api, document.get(), 2, 2));
    ASSERT_NO_FATAL_FAILURE(AddMarkedPage(api, document.get(), 4, 4));
    ASSERT_NO_FATAL_FAILURE(RoundTripPages(*library, document.get(), {0, 1, 2, 3, 4},
                                          &artifacts, "insertions",
                                          &observations[index++]));
  }
  ComparePages(observations[0], observations[1], &artifacts, "implementations");
}

TEST(Pages, DeleteFirstMiddleLastAndInvalidIndexes) {
  Artifacts artifacts;
  std::array<std::vector<Raster>, 2> observations;
  size_t index = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    api.FPDFPage_Delete(document.get(), 0);
    EXPECT_EQ(api.FPDF_GetPageCount(document.get()), 0);
    ASSERT_NO_FATAL_FAILURE(Populate(api, document.get(), {0, 1, 2, 3, 4}));
    api.FPDFPage_Delete(nullptr, 0);
    api.FPDFPage_Delete(document.get(), -1);
    api.FPDFPage_Delete(document.get(), 5);
    EXPECT_EQ(api.FPDF_GetPageCount(document.get()), 5);
    api.FPDFPage_Delete(document.get(), 0);
    EXPECT_EQ(api.FPDF_GetPageCount(document.get()), 4);
    api.FPDFPage_Delete(document.get(), 1);
    EXPECT_EQ(api.FPDF_GetPageCount(document.get()), 3);
    api.FPDFPage_Delete(document.get(), 2);
    ASSERT_NO_FATAL_FAILURE(RoundTripPages(*library, document.get(), {1, 3},
                                          &artifacts, "deletions",
                                          &observations[index++]));
    api.FPDFPage_Delete(document.get(), 0);
    api.FPDFPage_Delete(document.get(), 0);
    api.FPDFPage_Delete(document.get(), 0);
    EXPECT_EQ(api.FPDF_GetPageCount(document.get()), 0);
  }
  ComparePages(observations[0], observations[1], &artifacts, "implementations");
}

TEST(Pages, MoveSelectionOrderAtBeginningMiddleAndEnd) {
  struct Case { Labels selection; int destination; Labels expected; };
  const std::vector<Case> cases = {
      {{3, 2}, 1, {0, 3, 2, 1}}, {{3}, 0, {3, 0, 1, 2}},
      {{0}, 3, {1, 2, 3, 0}}, {{0, 2}, 0, {0, 2, 1, 3}},
      {{0, 2}, 2, {1, 3, 0, 2}}, {{3, 2, 1, 0}, 0, {3, 2, 1, 0}}};
  Artifacts artifacts;
  for (size_t case_index = 0; case_index < cases.size(); ++case_index) {
    SCOPED_TRACE(case_index);
    const auto& item = cases[case_index];
    std::array<std::vector<Raster>, 2> observations;
    size_t index = 0;
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name());
      const auto& api = library->api();
      Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(Populate(api, document.get(), {0, 1, 2, 3}));
      ASSERT_TRUE(api.FPDF_MovePages(document.get(), item.selection.data(),
                                     item.selection.size(), item.destination));
      ASSERT_NO_FATAL_FAILURE(RoundTripPages(*library, document.get(), item.expected,
                                            &artifacts, "move-" + std::to_string(case_index),
                                            &observations[index++]));
    }
    ComparePages(observations[0], observations[1], &artifacts,
                 "move-implementations-" + std::to_string(case_index));
  }
}

TEST(Pages, MoveRejectsInvalidSelectionAndDestination) {
  struct Case { Labels selection; int destination; };
  const std::vector<Case> cases = {
      {{0, 0}, 0}, {{-1}, 0}, {{4}, 0}, {{0}, -1}, {{0}, 4}, {{}, 0},
      {{0, 1, 2, 3, 0}, 0}};
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    const int valid_index = 0;
    EXPECT_FALSE(api.FPDF_MovePages(nullptr, &valid_index, 1, 0));
    Handle empty(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(empty.get(), nullptr);
    EXPECT_FALSE(api.FPDF_MovePages(empty.get(), &valid_index, 1, 0));
    for (size_t index = 0; index < cases.size(); ++index) {
      SCOPED_TRACE(index);
      const auto& item = cases[index];
      Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(Populate(api, document.get(), {0, 1, 2, 3}));
      // The native contract permits partial mutation on failure; this fresh
      // document is discarded, with no assertion about rollback.
      EXPECT_FALSE(api.FPDF_MovePages(document.get(), item.selection.data(),
                                      item.selection.size(), item.destination));
    }
  }
}

void ExpectSizes(const Api& api, FPDF_DOCUMENT document, FPDF_PAGE page,
                 int index, float width, float height) {
  EXPECT_FLOAT_EQ(api.FPDF_GetPageWidthF(page), width);
  EXPECT_FLOAT_EQ(api.FPDF_GetPageHeightF(page), height);
  EXPECT_DOUBLE_EQ(api.FPDF_GetPageWidth(page), static_cast<double>(width));
  EXPECT_DOUBLE_EQ(api.FPDF_GetPageHeight(page), static_cast<double>(height));
  FS_SIZEF modern{-1, -2};
  ASSERT_TRUE(api.FPDF_GetPageSizeByIndexF(document, index, &modern));
  EXPECT_FLOAT_EQ(modern.width, width);
  EXPECT_FLOAT_EQ(modern.height, height);
  double legacy_width = -3, legacy_height = -4;
  ASSERT_TRUE(api.FPDF_GetPageSizeByIndex(document, index, &legacy_width,
                                         &legacy_height));
  EXPECT_DOUBLE_EQ(legacy_width, width);
  EXPECT_DOUBLE_EQ(legacy_height, height);
}

TEST(Pages, AllRotationsUpdateFractionalDimensionsAndPersistedRendering) {
  Artifacts artifacts;
  std::array<std::vector<Raster>, 2> observations;
  size_t implementation = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    EXPECT_EQ(api.FPDFPage_GetRotation(nullptr), -1);
    api.FPDFPage_SetRotation(nullptr, 1);
    for (int rotation = 0; rotation != 4; ++rotation) {
      SCOPED_TRACE(rotation);
      Handle page(api.FPDFPage_New(document.get(), rotation, 80.25, 64.5),
                  api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      EXPECT_EQ(api.FPDFPage_GetRotation(page.get()), 0);
      Handle mark(api.FPDFPageObj_CreateNewRect(8, 12, 16, 20),
                  api.FPDFPageObj_Destroy);
      ASSERT_NE(mark.get(), nullptr);
      ASSERT_TRUE(api.FPDFPageObj_SetFillColor(mark.get(), 255, 0, 0, 255));
      ASSERT_TRUE(api.FPDFPath_SetDrawMode(mark.get(), FPDF_FILLMODE_WINDING, 0));
      api.FPDFPage_InsertObject(page.get(), mark.release());
      Handle annotation(api.FPDFPage_CreateAnnot(page.get(), FPDF_ANNOT_STAMP),
                        api.FPDFPage_CloseAnnot);
      ASSERT_NE(annotation.get(), nullptr);
      const FS_RECTF annotation_rect{40, 40, 50, 30};
      ASSERT_TRUE(api.FPDFAnnot_SetRect(annotation.get(), &annotation_rect));
      const std::u16string appearance = u"0 0 1 rg 0 0 100 100 re f";
      ASSERT_TRUE(api.FPDFAnnot_SetAP(annotation.get(), FPDF_ANNOT_APPEARANCEMODE_NORMAL,
          reinterpret_cast<const FPDF_WCHAR*>(appearance.c_str())));
      api.FPDFPage_SetRotation(page.get(), rotation);
      api.FPDFPage_SetRotation(page.get(), rotation);
      EXPECT_EQ(api.FPDFPage_GetRotation(page.get()), rotation);
      ASSERT_NO_FATAL_FAILURE(ExpectSizes(api, document.get(), page.get(), rotation,
                                          rotation % 2 ? 64.5f : 80.25f,
                                          rotation % 2 ? 80.25f : 64.5f));
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      observations[implementation].push_back(Render(api, page.get(), 80, 64, FPDF_ANNOT));
      const auto& raster = observations[implementation].back();
      ASSERT_EQ(raster.pixels.size(), 80u * 64 * 4);
      // The red rectangle's center is transformed independently for each
      // clockwise page rotation; this checks orientation, not just agreement.
      const double u = 16.0 / 80.25, v = 22.0 / 64.5;
      const std::array<std::array<double, 2>, 4> centers = {{
          {u, 1 - v}, {v, u}, {1 - u, v}, {1 - v, 1 - u}}};
      const size_t pixel = (static_cast<int>(centers[rotation][1] * 64) * 80 +
                            static_cast<int>(centers[rotation][0] * 80)) * 4;
      EXPECT_EQ(raster.pixels[pixel], 0);
      EXPECT_EQ(raster.pixels[pixel + 1], 0);
      EXPECT_EQ(raster.pixels[pixel + 2], 255);
      EXPECT_EQ(raster.pixels[pixel + 3], 255);
      const double au = 45.0 / 80.25, av = 35.0 / 64.5;
      const std::array<std::array<double, 2>, 4> annotation_centers = {{
          {au, 1 - av}, {av, au}, {1 - au, av}, {1 - av, 1 - au}}};
      const size_t annotation_pixel =
          (static_cast<int>(annotation_centers[rotation][1] * 64) * 80 +
           static_cast<int>(annotation_centers[rotation][0] * 80)) * 4;
      EXPECT_EQ(raster.pixels[annotation_pixel], 255);
      EXPECT_EQ(raster.pixels[annotation_pixel + 1], 0);
      EXPECT_EQ(raster.pixels[annotation_pixel + 2], 0);
      EXPECT_EQ(raster.pixels[annotation_pixel + 3], 255);
      EXPECT_EQ(raster.pixels[0], 255);
      EXPECT_EQ(raster.pixels[1], 255);
      EXPECT_EQ(raster.pixels[2], 255);
      artifacts.Image(library->name() + "-rotation-" + std::to_string(rotation),
                       observations[implementation].back());
    }
    const auto bytes = SaveDocument(api, document.get(), artifacts,
                                    library->name() + "-rotations");
    ASSERT_FALSE(bytes.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      const auto& read = reader->api();
      Handle loaded(read.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr),
                    read.FPDF_CloseDocument);
      ASSERT_NE(loaded.get(), nullptr);
      for (int rotation = 0; rotation != 4; ++rotation) {
        Handle page(read.FPDF_LoadPage(loaded.get(), rotation), read.FPDF_ClosePage);
        ASSERT_NE(page.get(), nullptr);
        EXPECT_EQ(read.FPDFPage_GetRotation(page.get()), rotation);
        ASSERT_NO_FATAL_FAILURE(ExpectSizes(read, loaded.get(), page.get(), rotation,
                                            rotation % 2 ? 64.5f : 80.25f,
                                            rotation % 2 ? 80.25f : 64.5f));
        CompareRaster(observations[implementation][rotation],
                       Render(read, page.get(), 80, 64, FPDF_ANNOT), artifacts,
                       library->name() + "-to-" + reader->name() + "-rotation-" +
                           std::to_string(rotation));
      }
    }
    ++implementation;
  }
  ComparePages(observations[0], observations[1], &artifacts, "rotations");
}

using BoxGetter = decltype(Api::FPDFPage_GetMediaBox);
using BoxSetter = decltype(Api::FPDFPage_SetMediaBox);

std::array<BoxGetter, 5> BoxGetters(const Api& api) {
  return {api.FPDFPage_GetMediaBox, api.FPDFPage_GetCropBox,
           api.FPDFPage_GetBleedBox, api.FPDFPage_GetTrimBox,
           api.FPDFPage_GetArtBox};
}

std::array<BoxSetter, 5> BoxSetters(const Api& api) {
  return {api.FPDFPage_SetMediaBox, api.FPDFPage_SetCropBox,
           api.FPDFPage_SetBleedBox, api.FPDFPage_SetTrimBox,
           api.FPDFPage_SetArtBox};
}

void ExpectBox(BoxGetter getter, FPDF_PAGE page,
                const std::array<float, 4>& expected) {
  std::array<float, 4> actual{-1, -2, -3, -4};
  ASSERT_TRUE(getter(page, &actual[0], &actual[1], &actual[2], &actual[3]));
  EXPECT_EQ(actual, expected);
}

void ExpectBoxFailure(BoxGetter getter, FPDF_PAGE page) {
  std::array<float, 4> output{101, 102, 103, 104};
  const auto original = output;
  EXPECT_FALSE(getter(page, &output[0], &output[1], &output[2], &output[3]));
  EXPECT_EQ(output, original);
}

TEST(Pages, SetOverwriteAndReloadEveryPageBox) {
  const std::array<std::array<float, 4>, 5> expected = {{
      {-10.25f, -20.5f, 190.75f, 100.5f},
      {0.25f, -10.5f, 150.75f, 90.5f},
      {1.25f, 2.5f, 140.75f, 80.5f},
      {3.25f, 4.5f, 130.75f, 70.5f},
      {5.25f, 6.5f, 120.75f, 60.5f}}};
  Artifacts artifacts;
  std::array<Raster, 2> observations;
  size_t implementation = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(AddMarkedPage(api, document.get(), 0, 0));
    {
      Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      const auto setters = BoxSetters(api);
      const auto getters = BoxGetters(api);
      for (size_t box = 0; box < setters.size(); ++box) {
        SCOPED_TRACE(box);
        setters[box](page.get(), 1, 2, 30, 40);
        ASSERT_NO_FATAL_FAILURE(ExpectBox(getters[box], page.get(), {1, 2, 30, 40}));
        const auto& e = expected[box];
        setters[box](page.get(), e[0], e[1], e[2], e[3]);
        ASSERT_NO_FATAL_FAILURE(ExpectBox(getters[box], page.get(), e));
        setters[box](nullptr, e[0], e[1], e[2], e[3]);
      }
      ASSERT_NO_FATAL_FAILURE(ExpectSizes(api, document.get(), page.get(), 0,
                                          150.5f, 101.0f));
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      observations[implementation] = Render(api, page.get(), 96, 64);
      artifacts.Image(library->name() + "-page-boxes", observations[implementation]);
    }
    const auto bytes = SaveDocument(api, document.get(), artifacts,
                                    library->name() + "-page-boxes");
    ASSERT_FALSE(bytes.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      const auto& read = reader->api();
      Handle loaded(read.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr),
                    read.FPDF_CloseDocument);
      ASSERT_NE(loaded.get(), nullptr);
      Handle page(read.FPDF_LoadPage(loaded.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      const auto getters = BoxGetters(read);
      for (size_t box = 0; box < getters.size(); ++box)
        ASSERT_NO_FATAL_FAILURE(ExpectBox(getters[box], page.get(), expected[box]));
      ASSERT_NO_FATAL_FAILURE(ExpectSizes(read, loaded.get(), page.get(), 0,
                                          150.5f, 101.0f));
      CompareRaster(observations[implementation], Render(read, page.get(), 96, 64),
                     artifacts, library->name() + "-to-" + reader->name() + "-boxes");
    }
    ++implementation;
  }
  CompareRaster(observations[0], observations[1], artifacts, "boxes-implementations");
}

// Public APIs cannot assign page-tree inherited attributes. This one-page
// classic-xref fixture has MediaBox/CropBox/Rotate only on its Pages parent.
std::vector<uint8_t> InheritedGeometryFixture() {
  const std::array<std::string, 4> objects = {
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [-10 -20 190 100] "
      "/CropBox [0 -10 150 90] /Rotate 90 >>",
      "<< /Type /Page /Parent 2 0 R /Resources << >> /Contents 4 0 R >>",
      "<< /Length 0 >>\nstream\n\nendstream"};
  std::string pdf = "%PDF-1.7\n";
  std::vector<size_t> offsets;
  for (size_t index = 0; index < objects.size(); ++index) {
    offsets.push_back(pdf.size());
    pdf += std::to_string(index + 1) + " 0 obj\n" + objects[index] + "\nendobj\n";
  }
  const size_t xref = pdf.size();
  pdf += "xref\n0 5\n0000000000 65535 f \n";
  for (size_t offset : offsets) {
    char entry[32];
    std::snprintf(entry, sizeof(entry), "%010llu 00000 n \n",
                   static_cast<unsigned long long>(offset));
    pdf += entry;
  }
  pdf += "trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n" +
         std::to_string(xref) + "\n%%EOF\n";
  return {pdf.begin(), pdf.end()};
}

TEST(Pages, BoxQueriesDistinguishAbsentAndInheritedEntries) {
  Artifacts artifacts;
  const auto fixture = InheritedGeometryFixture();
  artifacts.Pdf("inherited-geometry-fixture", fixture);
  std::array<Raster, 2> inherited_rasters;
  size_t implementation = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDFPage_New(document.get(), 0, 80.25, 64.5), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    const auto getters = BoxGetters(api);
    ASSERT_NO_FATAL_FAILURE(ExpectBox(getters[0], page.get(), {0, 0, 80.25f, 64.5f}));
    for (size_t box = 1; box < getters.size(); ++box)
      ASSERT_NO_FATAL_FAILURE(ExpectBoxFailure(getters[box], page.get()));
    for (const auto getter : getters) {
      ASSERT_NO_FATAL_FAILURE(ExpectBoxFailure(getter, nullptr));
      for (size_t omitted = 0; omitted < 4; ++omitted) {
        std::array<float, 4> outputs{101, 102, 103, 104};
        const auto original = outputs;
        std::array<float*, 4> pointers = {&outputs[0], &outputs[1], &outputs[2], &outputs[3]};
        pointers[omitted] = nullptr;
        EXPECT_FALSE(getter(page.get(), pointers[0], pointers[1], pointers[2], pointers[3]));
        EXPECT_EQ(outputs, original);
      }
    }
    Handle inherited(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr),
                     api.FPDF_CloseDocument);
    ASSERT_NE(inherited.get(), nullptr);
    Handle inherited_page(api.FPDF_LoadPage(inherited.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(inherited_page.get(), nullptr);
    EXPECT_EQ(api.FPDFPage_GetRotation(inherited_page.get()), 1);
    ASSERT_NO_FATAL_FAILURE(ExpectSizes(api, inherited.get(), inherited_page.get(),
                                        0, 100, 150));
    // Getters read the page dictionary itself, while page dimensions and the
    // rotation accessor walk inheritance. Do not invent inherited getter output.
    for (const auto getter : getters)
      ASSERT_NO_FATAL_FAILURE(ExpectBoxFailure(getter, inherited_page.get()));
    inherited_rasters[implementation++] = Render(api, inherited_page.get(), 64, 96);
  }
  CompareRaster(inherited_rasters[0], inherited_rasters[1], artifacts, "inherited");
}

TEST(Pages, BoundingBoxIntersectsMediaAndCropWithoutRotatingCoordinates) {
  Artifacts artifacts;
  std::array<Raster, 2> observations;
  size_t implementation = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDFPage_New(document.get(), 0, 200, 120), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    api.FPDFPage_SetMediaBox(page.get(), -10, -20, 190, 100);
    api.FPDFPage_SetCropBox(page.get(), 0, -30, 210, 90);
    for (int rotation = 0; rotation < 4; ++rotation) {
      api.FPDFPage_SetRotation(page.get(), rotation);
      FS_RECTF box{};
      ASSERT_TRUE(api.FPDF_GetPageBoundingBox(page.get(), &box));
      EXPECT_FLOAT_EQ(box.left, 0);
      EXPECT_FLOAT_EQ(box.bottom, -20);
      EXPECT_FLOAT_EQ(box.right, 190);
      EXPECT_FLOAT_EQ(box.top, 90);
      ASSERT_NO_FATAL_FAILURE(ExpectSizes(api, document.get(), page.get(), 0,
                                          rotation % 2 ? 110 : 190,
                                          rotation % 2 ? 190 : 110));
    }
    FS_RECTF sentinel{101, 102, 103, 104};
    EXPECT_FALSE(api.FPDF_GetPageBoundingBox(nullptr, &sentinel));
    EXPECT_FLOAT_EQ(sentinel.left, 101);
    EXPECT_FLOAT_EQ(sentinel.top, 102);
    EXPECT_FLOAT_EQ(sentinel.right, 103);
    EXPECT_FLOAT_EQ(sentinel.bottom, 104);
    EXPECT_FALSE(api.FPDF_GetPageBoundingBox(page.get(), nullptr));
    observations[implementation++] = Render(api, page.get(), 64, 96);
  }
  CompareRaster(observations[0], observations[1], artifacts, "intersection");
}

TEST(Pages, IndexedSizesAndPageLoadErrorsPreserveOutputs) {
  Artifacts artifacts;
  std::array<std::vector<Raster>, 2> observations;
  size_t implementation = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(Populate(api, document.get(), {0, 1, 2}));
    for (int index : {0, 2, 0, 2}) {
      Handle page(api.FPDF_LoadPage(document.get(), index), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(ExpectSizes(api, document.get(), page.get(), index,
                                          static_cast<float>(LabelWidth(index)),
                                          static_cast<float>(LabelHeight(index))));
      observations[implementation].push_back(Render(api, page.get(), 80, 64));
    }
    for (int index : {-1, 3, 99}) {
      EXPECT_EQ(api.FPDF_LoadPage(document.get(), index), nullptr);
      FS_SIZEF modern{101, 102};
      EXPECT_FALSE(api.FPDF_GetPageSizeByIndexF(document.get(), index, &modern));
      EXPECT_FLOAT_EQ(modern.width, 101);
      EXPECT_FLOAT_EQ(modern.height, 102);
      double width = 103, height = 104;
      EXPECT_FALSE(api.FPDF_GetPageSizeByIndex(document.get(), index, &width, &height));
      EXPECT_DOUBLE_EQ(width, 103);
      EXPECT_DOUBLE_EQ(height, 104);
    }
    FS_SIZEF modern{101, 102};
    EXPECT_FALSE(api.FPDF_GetPageSizeByIndexF(nullptr, 0, &modern));
    EXPECT_FLOAT_EQ(modern.width, 101);
    EXPECT_FLOAT_EQ(modern.height, 102);
    EXPECT_FALSE(api.FPDF_GetPageSizeByIndexF(document.get(), 0, nullptr));
    double width = 103, height = 104;
    EXPECT_FALSE(api.FPDF_GetPageSizeByIndex(nullptr, 0, &width, &height));
    EXPECT_FALSE(api.FPDF_GetPageSizeByIndex(document.get(), 0, nullptr, &height));
    EXPECT_FALSE(api.FPDF_GetPageSizeByIndex(document.get(), 0, &width, nullptr));
    EXPECT_DOUBLE_EQ(width, 103);
    EXPECT_DOUBLE_EQ(height, 104);
    ++implementation;
  }
  ComparePages(observations[0], observations[1], &artifacts, "repeated-loads");
}

TEST(Pages, DeviceAndPageCoordinatesMatchCornersAndRoundTripWithRounding) {
  std::array<std::vector<double>, 2> observations;
  size_t implementation = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDFPage_New(document.get(), 0, 200, 120), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    for (bool offset_box : {false, true}) {
      const double left = offset_box ? -10 : 0;
      const double bottom = offset_box ? -20 : 0;
      api.FPDFPage_SetMediaBox(page.get(), static_cast<float>(left),
                               static_cast<float>(bottom),
                               static_cast<float>(left + 200),
                               static_cast<float>(bottom + 120));
      for (const auto& viewport : {std::array<int, 4>{0, 0, 400, 240},
                                   std::array<int, 4>{13, 17, 800, 360}}) {
        const auto [start_x, start_y, width, height] = viewport;
        for (int rotation = 0; rotation < 4; ++rotation) {
          SCOPED_TRACE(rotation);
          for (const auto& fraction : {std::array<double, 2>{0, 0},
                                       std::array<double, 2>{1, 0},
                                       std::array<double, 2>{0, 1},
                                       std::array<double, 2>{1, 1},
                                       std::array<double, 2>{0.25, 0.75},
                                       std::array<double, 2>{0.5, 0.5}}) {
            const auto [u, v] = fraction;
            const std::array<std::array<double, 2>, 4> normalized = {{
                {u, 1 - v}, {v, u}, {1 - u, v}, {1 - v, 1 - u}}};
            const int expected_x = static_cast<int>(start_x + normalized[rotation][0] * width);
            const int expected_y = static_cast<int>(start_y + normalized[rotation][1] * height);
            const double x = left + u * 200, y = bottom + v * 120;
            int device_x = -1, device_y = -2;
            ASSERT_TRUE(api.FPDF_PageToDevice(page.get(), start_x, start_y, width,
                                               height, rotation, x, y,
                                               &device_x, &device_y));
            EXPECT_EQ(device_x, expected_x);
            EXPECT_EQ(device_y, expected_y);
            double page_x = -3, page_y = -4;
            ASSERT_TRUE(api.FPDF_DeviceToPage(page.get(), start_x, start_y, width,
                                               height, rotation, expected_x,
                                               expected_y, &page_x, &page_y));
            EXPECT_NEAR(page_x, x, 0.0001);
            EXPECT_NEAR(page_y, y, 0.0001);
            observations[implementation].insert(observations[implementation].end(),
                                                  {static_cast<double>(device_x),
                                                   static_cast<double>(device_y), page_x, page_y});
          }
          const double x = left + 37.123, y = bottom + 71.234;
          int device_x = -1, device_y = -2;
          ASSERT_TRUE(api.FPDF_PageToDevice(page.get(), start_x, start_y, width,
                                             height, rotation, x, y,
                                             &device_x, &device_y));
          double restored_x = -1, restored_y = -2;
          ASSERT_TRUE(api.FPDF_DeviceToPage(page.get(), start_x, start_y, width,
                                             height, rotation, device_x, device_y,
                                             &restored_x, &restored_y));
          // PageToDevice rounds to integer pixels: allow half a device pixel
          // transformed back into the appropriate page-space axis.
          EXPECT_NEAR(restored_x, x, 100.0 / (rotation % 2 ? height : width) + 0.0001);
          EXPECT_NEAR(restored_y, y, 60.0 / (rotation % 2 ? width : height) + 0.0001);
        }
      }
    }
    int device_x = 101, device_y = 102;
    EXPECT_FALSE(api.FPDF_PageToDevice(nullptr, 0, 0, 400, 240, 0, 1, 2,
                                        &device_x, &device_y));
    EXPECT_FALSE(api.FPDF_PageToDevice(page.get(), 0, 0, 400, 240, 0, 1, 2,
                                        nullptr, &device_y));
    EXPECT_FALSE(api.FPDF_PageToDevice(page.get(), 0, 0, 400, 240, 0, 1, 2,
                                        &device_x, nullptr));
    EXPECT_EQ(device_x, 101);
    EXPECT_EQ(device_y, 102);
    double page_x = 103, page_y = 104;
    EXPECT_FALSE(api.FPDF_DeviceToPage(nullptr, 0, 0, 400, 240, 0, 1, 2,
                                        &page_x, &page_y));
    EXPECT_FALSE(api.FPDF_DeviceToPage(page.get(), 0, 0, 400, 240, 0, 1, 2,
                                        nullptr, &page_y));
    EXPECT_FALSE(api.FPDF_DeviceToPage(page.get(), 0, 0, 400, 240, 0, 1, 2,
                                        &page_x, nullptr));
    EXPECT_DOUBLE_EQ(page_x, 103);
    EXPECT_DOUBLE_EQ(page_y, 104);
    ++implementation;
  }
  ASSERT_EQ(observations[0].size(), observations[1].size());
  for (size_t index = 0; index < observations[0].size(); ++index)
    EXPECT_NEAR(observations[0][index], observations[1][index], 0.0001) << index;
}

}  // namespace
}  // namespace pdfoxium::integration
