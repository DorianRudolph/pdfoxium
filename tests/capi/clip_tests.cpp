#include "editing_fixture.h"

#include <cmath>
#include <tuple>
#include <set>

namespace pdfoxium::integration {
namespace {

using ClipPoint = std::tuple<int, float, float, bool>;

std::vector<std::vector<ClipPoint>> InspectClip(const Api& api, FPDF_CLIPPATH clip) {
  std::vector<std::vector<ClipPoint>> paths;
  const int count = api.FPDFClipPath_CountPaths(clip);
  EXPECT_GT(count, 0);
  for (int path = 0; path < count; ++path) {
    const int segments = api.FPDFClipPath_CountPathSegments(clip, path);
    EXPECT_GT(segments, 0);
    paths.emplace_back();
    for (int index = 0; index < segments; ++index) {
      auto segment = api.FPDFClipPath_GetPathSegment(clip, path, index);
      EXPECT_NE(segment, nullptr);
      if (!segment) continue;
      float x = 0, y = 0;
      EXPECT_TRUE(api.FPDFPathSegment_GetPoint(segment, &x, &y));
      paths.back().emplace_back(api.FPDFPathSegment_GetType(segment), x, y,
                                !!api.FPDFPathSegment_GetClose(segment));
    }
    EXPECT_EQ(api.FPDFClipPath_GetPathSegment(clip, path, -1), nullptr);
    EXPECT_EQ(api.FPDFClipPath_GetPathSegment(clip, path, segments), nullptr);
    EXPECT_EQ(api.FPDFClipPath_GetPathSegment(clip, path, segments + 1), nullptr);
  }
  for (int path : {-1, count, count + 1}) {
    EXPECT_EQ(api.FPDFClipPath_CountPathSegments(clip, path), -1);
    EXPECT_EQ(api.FPDFClipPath_GetPathSegment(clip, path, 0), nullptr);
  }
  return paths;
}

std::vector<uint8_t> ClipFixture(bool array_contents, bool multiple) {
  // Public APIs cannot construct arbitrary multi-path object clip states.
  std::vector<std::string> objects = {
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << >> /Contents " +
          std::string(array_contents ? "[4 0 R 5 0 R]" : "4 0 R") + " >>",
      PdfStream("", std::string(multiple ? "q 8 16 m 104 16 l 56 96 l h W n " : "q 8 16 96 80 re W n ") +
          (multiple ? "16 24 m 96 24 l 96 88 l 16 88 l h 32 40 16 16 re W* n " : "") +
          "1 0 0 rg 0 0 128 128 re f Q")};
  if (array_contents) objects.push_back(PdfStream("", "0 0 1 rg 112 112 8 8 re f"));
  return PdfFixture(objects);
}

TEST(Clipping, OwnedFractionalRectanglesAndBorrowedMultipleClipsHaveDistinctLifetimes) {
  Artifacts artifacts;
  std::array<Raster, 2> pictures;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle owned(api.FPDF_CreateClipPath(-2.5f, 3.25f, 84.75f, 92.5f), api.FPDF_DestroyClipPath);
    ASSERT_NE(owned.get(), nullptr);
    const auto rectangle = InspectClip(api, owned.get());
    ASSERT_EQ(rectangle.size(), 1u); ASSERT_EQ(rectangle[0].size(), 5u);
    EXPECT_EQ(rectangle[0][0], (ClipPoint{FPDF_SEGMENT_MOVETO, -2.5f, 3.25f, false}));
    EXPECT_EQ(rectangle[0][4], (ClipPoint{FPDF_SEGMENT_LINETO, -2.5f, 3.25f, true}));
    const auto bytes = ClipFixture(false, true);
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    ASSERT_EQ(api.FPDFPage_CountObjects(page.get()), 1);
    auto object = api.FPDFPage_GetObject(page.get(), 0);
    auto borrowed = api.FPDFPageObj_GetClipPath(object);
    ASSERT_NE(borrowed, nullptr);
    const auto paths = InspectClip(api, borrowed);
    EXPECT_GE(paths.size(), 2u);
    size_t total = 0; for (const auto& points : paths) total += points.size();
    EXPECT_GE(total, 10u);
    pictures[backend++] = Render(api, page.get(), 128, 128);
    Handle blank(api.FPDFPageObj_CreateNewRect(1, 2, 3, 4), api.FPDFPageObj_Destroy);
    auto empty = api.FPDFPageObj_GetClipPath(blank.get());
    ASSERT_NE(empty, nullptr);
    EXPECT_EQ(api.FPDFClipPath_CountPaths(empty), -1);
    EXPECT_EQ(api.FPDFPageObj_GetClipPath(nullptr), nullptr);
    EXPECT_EQ(api.FPDFClipPath_CountPaths(nullptr), -1);
    EXPECT_EQ(api.FPDFClipPath_CountPathSegments(nullptr, 0), -1);
    EXPECT_EQ(api.FPDFClipPath_GetPathSegment(nullptr, 0, 0), nullptr);
    // borrowed/empty belong to their object; only the explicitly created clip
    // is destroyed independently by its Handle.
  }
  CompareRaster(pictures[0], pictures[1], artifacts, "multiple-object-clips");
}

TEST(Clipping, InsertingAnOwnedClipCopiesIntoStreamAndArrayContent) {
  Artifacts artifacts;
  for (bool array_contents : {false, true}) {
    std::array<Raster, 2> pictures;
    size_t backend = 0;
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name() + (array_contents ? "-array" : "-stream"));
      const auto& api = writer->api();
      const auto bytes = ClipFixture(array_contents, false);
      Handle doc(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
      Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      {
        Handle clip(api.FPDF_CreateClipPath(24.25f, 32.5f, 72.75f, 80.25f), api.FPDF_DestroyClipPath);
        ASSERT_NE(clip.get(), nullptr);
        EXPECT_EQ(InspectClip(api, clip.get()).size(), 1u);
        api.FPDFPage_InsertClipPath(page.get(), clip.get());
        // The source clip is still caller-owned and can be released now.
      }
      const auto saved = SaveDocument(api, doc.get(), artifacts,
          writer->name() + "-insert-clip-" + std::to_string(array_contents));
      size_t reader_index = 0;
      std::array<Raster, 2> rendered;
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api();
        Handle reloaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
        Handle p(read.FPDF_LoadPage(reloaded.get(), 0), read.FPDF_ClosePage);
        ASSERT_NE(p.get(), nullptr);
        rendered[reader_index] = Render(read, p.get(), 128, 128);
        const auto& pixels = rendered[reader_index].pixels;
        ASSERT_EQ(pixels.size(), 128u * 128 * 4);
        auto pixel = [&](int x, int y) { const size_t n = (y * 128 + x) * 4; return std::array<uint8_t, 4>{pixels[n], pixels[n + 1], pixels[n + 2], pixels[n + 3]}; };
        EXPECT_EQ(pixel(40, 64), (std::array<uint8_t, 4>{0, 0, 255, 255}));
        EXPECT_EQ(pixel(16, 64), (std::array<uint8_t, 4>{255, 255, 255, 255}));
        EXPECT_EQ(pixel(116, 12), (std::array<uint8_t, 4>{255, 255, 255, 255}));
        ++reader_index;
      }
      CompareRaster(rendered[0], rendered[1], artifacts, writer->name() + "-copied-clip");
      pictures[backend++] = rendered[0];
    }
    CompareRaster(pictures[0], pictures[1], artifacts, "insert-clips-backends");
  }
}

TEST(Clipping, ObjectClipTransformsComposeAndPersistWithoutMovingObjectGeometry) {
  Artifacts artifacts;
  const std::array<FS_MATRIX, 5> matrices = {{{1, 0, 0, 1, 4, 8}, {.5f, 0, 0, .75f, 0, 0},
      {0, 1, -1, 0, 128, 0}, {1, .125f, .25f, 1, 0, 0}, {1, 0, 0, 1, 0, 0}}};
  for (size_t index = 0; index < matrices.size(); ++index) {
    std::array<Raster, 2> pictures;
    size_t backend = 0;
    for (const auto* writer : Implementations()) {
      const auto& api = writer->api();
      const auto fixture = ClipFixture(false, false);
      Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
      Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      auto object = api.FPDFPage_GetObject(page.get(), 0);
      const auto bounds = editing::Bounds(api, object);
      const auto original = InspectClip(api, api.FPDFPageObj_GetClipPath(object));
      const auto& m = matrices[index];
      api.FPDFPageObj_TransformClipPath(object, m.a, m.b, m.c, m.d, m.e, m.f);
      const auto first = InspectClip(api, api.FPDFPageObj_GetClipPath(object));
      ASSERT_EQ(first.size(), original.size());
      for (size_t p = 0; p < original.size(); ++p) for (size_t n = 0; n < original[p].size(); ++n) {
        const auto [type, x, y, close] = original[p][n];
        EXPECT_NEAR(std::get<1>(first[p][n]), m.a * x + m.c * y + m.e, .0001f);
        EXPECT_NEAR(std::get<2>(first[p][n]), m.b * x + m.d * y + m.f, .0001f);
        EXPECT_EQ(std::get<0>(first[p][n]), type); EXPECT_EQ(std::get<3>(first[p][n]), close);
      }
      api.FPDFPageObj_TransformClipPath(object, 1, 0, 0, 1, 2, 3);
      EXPECT_EQ(editing::Bounds(api, object), bounds);
      const auto expected = InspectClip(api, api.FPDFPageObj_GetClipPath(object));
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*writer, doc.get(), page.get(), artifacts,
          writer->name() + "-clip-transform" + std::to_string(index), [expected](const Api& read, FPDF_PAGE p) {
            auto object = read.FPDFPage_GetObject(p, 0);
            const auto actual = InspectClip(read, read.FPDFPageObj_GetClipPath(object));
            ASSERT_EQ(actual.size(), expected.size());
            for (size_t i = 0; i < actual.size(); ++i) {
              ASSERT_EQ(actual[i].size(), expected[i].size());
              auto corners = [](const auto& points) { std::set<std::pair<float,float>> result;
                for (const auto& point : points) result.emplace(std::get<1>(point), std::get<2>(point));
                return result; };
              EXPECT_EQ(corners(actual[i]), corners(expected[i]));
              EXPECT_TRUE(std::get<3>(actual[i].back()));
            }
          }));
      pictures[backend++] = Render(api, page.get(), 128, 128);
      Handle absent(api.FPDFPageObj_CreateNewRect(0, 0, 8, 8), api.FPDFPageObj_Destroy);
      api.FPDFPageObj_TransformClipPath(absent.get(), 1, 0, 0, 1, 2, 3);
      EXPECT_EQ(api.FPDFClipPath_CountPaths(api.FPDFPageObj_GetClipPath(absent.get())), -1);
      api.FPDFPageObj_TransformClipPath(nullptr, 1, 0, 0, 1, 2, 3);
    }
    CompareRaster(pictures[0], pictures[1], artifacts, "clip-transform" + std::to_string(index));
  }
}

TEST(Clipping, WholePageMatrixClipAndCombinedTransformsPersistAcrossReaders) {
  Artifacts artifacts;
  // A path, text and image make the transformation affect all supported visual
  // categories. The stream operations preserve their original resource owners.
  const auto fixture = editing::AllObjectTypesFixture();
  const std::array<FS_MATRIX, 5> matrices = {{{1, 0, 0, 1, 0, 0}, {1, 0, 0, 1, 8, 12},
      {.5f, 0, 0, .75f, 0, 0}, {0, 1, -1, 0, 128, 0}, {1, .125f, .25f, 1, 0, 0}}};
  const FS_RECTF clip{16, 112, 104, 16};
  for (int combination = 0; combination < 3; ++combination) for (size_t index = 0; index < matrices.size(); ++index) {
    std::array<Raster, 2> writer_pictures;
    size_t writer_index = 0;
    for (const auto* writer : Implementations()) {
      const auto& api = writer->api();
      Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
      Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      EXPECT_FALSE(api.FPDFPage_TransFormWithClip(page.get(), nullptr, nullptr));
      EXPECT_FALSE(api.FPDFPage_TransFormWithClip(nullptr, &matrices[index], &clip));
      ASSERT_TRUE(api.FPDFPage_TransFormWithClip(page.get(), combination == 1 ? nullptr : &matrices[index],
                                               combination == 0 ? nullptr : &clip));
      // The API wraps content streams. Reopen them rather than expecting the
      // cached page-object list to be reparsed in place.
      const auto label = writer->name() + "-whole-page-" + std::to_string(combination) + "-" + std::to_string(index);
      const auto saved = SaveDocument(api, doc.get(), artifacts, label);
      std::array<Raster, 2> read_pictures;
      editing::TextRegions regions;
      size_t reader_index = 0;
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api();
        Handle loaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
        Handle p(read.FPDF_LoadPage(loaded.get(), 0), read.FPDF_ClosePage);
        ASSERT_NE(p.get(), nullptr);
        EXPECT_EQ(read.FPDFPage_CountObjects(p.get()), 5);
        regions=editing::TextBounds(read,p.get());
        read_pictures[reader_index++] = Render(read, p.get(), 128, 128);
      }
      editing::CompareScene(read_pictures[0], read_pictures[1], artifacts, label, regions, 1);
      writer_pictures[writer_index++] = read_pictures[0];
    }
    CompareRaster(writer_pictures[0], writer_pictures[1], artifacts,
                  "whole-page-writers-" + std::to_string(combination) + "-" + std::to_string(index), 1);
  }
}

}  // namespace
}  // namespace pdfoxium::integration
