#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <functional>
#include <string>
#include <set>
#include <tuple>
#include <vector>

namespace pdfoxium::integration {
namespace {

using Segment = std::tuple<int, float, float, bool>;

std::vector<Segment> ReadSegments(const Api& api, FPDF_PAGEOBJECT path) {
  std::vector<Segment> result;
  const int count = api.FPDFPath_CountSegments(path);
  EXPECT_GE(count, 0);
  for (int index = 0; index < count; ++index) {
    const auto segment = api.FPDFPath_GetPathSegment(path, index);
    EXPECT_NE(segment, nullptr);
    if (!segment) return result;
    float x = -1234, y = -5678;
    EXPECT_TRUE(api.FPDFPathSegment_GetPoint(segment, &x, &y));
    result.emplace_back(api.FPDFPathSegment_GetType(segment), x, y,
                        !!api.FPDFPathSegment_GetClose(segment));
  }
  EXPECT_EQ(api.FPDFPath_GetPathSegment(path, -1), nullptr);
  EXPECT_EQ(api.FPDFPath_GetPathSegment(path, count), nullptr);
  EXPECT_EQ(api.FPDFPath_GetPathSegment(path, count + 1), nullptr);
  return result;
}

void CheckPathRoundTrip(
    const Library& writer, FPDF_DOCUMENT document, FPDF_PAGE page,
    Artifacts& artifacts, const std::string& label,
    const std::function<void(const Api&, FPDF_PAGE)>& inspect) {
  const auto& api = writer.api();
  ASSERT_TRUE(api.FPDFPage_GenerateContent(page));
  ASSERT_NO_FATAL_FAILURE(inspect(api, page));
  const auto before = Render(api, page, 128, 128);
  artifacts.Image(label + "-before", before);
  const auto bytes = SaveDocument(api, document, artifacts, label);
  ASSERT_FALSE(bytes.empty());
  for (const auto* reader : Implementations()) {
    SCOPED_TRACE("reader=" + reader->name());
    const auto& read = reader->api();
    Handle reopened(read.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr),
                    read.FPDF_CloseDocument);
    ASSERT_NE(reopened.get(), nullptr);
    Handle reloaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
    ASSERT_NE(reloaded.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(inspect(read, reloaded.get()));
    const auto after = Render(read, reloaded.get(), 128, 128);
    artifacts.Image(label + "-to-" + reader->name(), after);
    CompareRaster(before, after, artifacts, label + "-to-" + reader->name());
  }
}

std::array<unsigned int, 4> Color(const Api& api, FPDF_PAGEOBJECT object,
                                  bool stroke) {
  std::array<unsigned int, 4> rgba{};
  const auto get = stroke ? api.FPDFPageObj_GetStrokeColor
                          : api.FPDFPageObj_GetFillColor;
  EXPECT_TRUE(get(object, &rgba[0], &rgba[1], &rgba[2], &rgba[3]));
  return rgba;
}

void ExpectMatrix(const Api& api, FPDF_PAGEOBJECT object,
                  const FS_MATRIX& expected) {
  FS_MATRIX actual{};
  ASSERT_TRUE(api.FPDFPageObj_GetMatrix(object, &actual));
  EXPECT_FLOAT_EQ(actual.a, expected.a);
  EXPECT_FLOAT_EQ(actual.b, expected.b);
  EXPECT_FLOAT_EQ(actual.c, expected.c);
  EXPECT_FLOAT_EQ(actual.d, expected.d);
  EXPECT_FLOAT_EQ(actual.e, expected.e);
  EXPECT_FLOAT_EQ(actual.f, expected.f);
}

TEST(Paths, InitialPointsRectanglesAndCallerOwnership) {
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDFPage_New(document.get(), 0, 128, 128), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    for (const auto& point : {std::array<float, 2>{0, 0}, {-8, -12}, {3.25f, -6.5f}}) {
      Handle path(api.FPDFPageObj_CreateNewPath(point[0], point[1]),
                  api.FPDFPageObj_Destroy);
      ASSERT_NE(path.get(), nullptr);
      EXPECT_EQ(api.FPDFPageObj_GetType(path.get()), FPDF_PAGEOBJ_PATH);
      EXPECT_EQ(ReadSegments(api, path.get()),
                (std::vector<Segment>{{FPDF_SEGMENT_MOVETO, point[0], point[1], false}}));
      EXPECT_EQ(api.FPDFPage_CountObjects(page.get()), 0);
    }
    for (const auto& rect : {std::array<float, 4>{8, 12, 24, 32},
                             {2.25f, 3.5f, 10.5f, 8.25f}, {8, 12, 0, 16},
                             {8, 12, 16, 0}, {40, 50, -16, -24}}) {
      Handle rectangle(api.FPDFPageObj_CreateNewRect(rect[0], rect[1], rect[2], rect[3]),
                       api.FPDFPageObj_Destroy);
      ASSERT_NE(rectangle.get(), nullptr);
      const auto points = ReadSegments(api, rectangle.get());
      ASSERT_EQ(points.size(), 5u);
      const float x = rect[0], y = rect[1], right = x + rect[2], top = y + rect[3];
      EXPECT_EQ(points, (std::vector<Segment>{
          {FPDF_SEGMENT_MOVETO, x, y, false}, {FPDF_SEGMENT_LINETO, x, top, false},
          {FPDF_SEGMENT_LINETO, right, top, false},
          {FPDF_SEGMENT_LINETO, right, y, false},
          {FPDF_SEGMENT_LINETO, x, y, true}}));
      Handle other(api.FPDFPageObj_CreateNewRect(1, 2, 3, 4), api.FPDFPageObj_Destroy);
      ASSERT_NE(other.get(), nullptr);
      ASSERT_TRUE(api.FPDFPath_LineTo(other.get(), 99, 98));
      EXPECT_EQ(ReadSegments(api, rectangle.get()), points);
      EXPECT_EQ(api.FPDFPage_CountObjects(page.get()), 0);
    }
  }
}

TEST(Paths, MultipleSubpathsLinesAndCubicsPersist) {
  Artifacts artifacts;
  std::array<Raster, 2> before;
  std::size_t writer_index = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    const auto& api = writer->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, 128, 128), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    Handle path(api.FPDFPageObj_CreateNewPath(-2.5f, 4.25f), api.FPDFPageObj_Destroy);
    ASSERT_NE(path.get(), nullptr);
    ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 20, 4.25f));
    ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 20, 24));
    ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 30, 34));
    ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 30, 34));
    ASSERT_TRUE(api.FPDFPath_BezierTo(path.get(), 35.5f, 60.25f, 50.75f, 5.5f, 60, 30));
    ASSERT_TRUE(api.FPDFPath_BezierTo(path.get(), 70, 30, 80, 30, 90, 30));
    ASSERT_TRUE(api.FPDFPath_MoveTo(path.get(), 72, 64));
    ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 96, 64));
    ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 84, 88));
    ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 72, 64));
    ASSERT_TRUE(api.FPDFPath_Close(path.get()));
    ASSERT_TRUE(api.FPDFPath_Close(path.get()));
    ASSERT_TRUE(api.FPDFPath_MoveTo(path.get(), 100, 80));
    ASSERT_TRUE(api.FPDFPath_BezierTo(path.get(), 110, 80, 110, 96, 100, 80));
    ASSERT_TRUE(api.FPDFPath_Close(path.get()));
    const std::vector<Segment> expected = {
      {FPDF_SEGMENT_MOVETO, -2.5f, 4.25f, false},
      {FPDF_SEGMENT_LINETO, 20, 4.25f, false}, {FPDF_SEGMENT_LINETO, 20, 24, false},
      {FPDF_SEGMENT_LINETO, 30, 34, false}, {FPDF_SEGMENT_LINETO, 30, 34, false},
      {FPDF_SEGMENT_BEZIERTO, 35.5f, 60.25f, false},
      {FPDF_SEGMENT_BEZIERTO, 50.75f, 5.5f, false}, {FPDF_SEGMENT_BEZIERTO, 60, 30, false},
      {FPDF_SEGMENT_BEZIERTO, 70, 30, false}, {FPDF_SEGMENT_BEZIERTO, 80, 30, false},
      {FPDF_SEGMENT_BEZIERTO, 90, 30, false}, {FPDF_SEGMENT_MOVETO, 72, 64, false},
      {FPDF_SEGMENT_LINETO, 96, 64, false}, {FPDF_SEGMENT_LINETO, 84, 88, false},
      {FPDF_SEGMENT_LINETO, 72, 64, true}, {FPDF_SEGMENT_MOVETO, 100, 80, false},
      {FPDF_SEGMENT_BEZIERTO, 110, 80, false}, {FPDF_SEGMENT_BEZIERTO, 110, 96, false},
      {FPDF_SEGMENT_BEZIERTO, 100, 80, true}};
    EXPECT_EQ(ReadSegments(api, path.get()), expected);
    for (std::size_t endpoint : {7u, 10u, 18u}) {
      FS_POINTF first{}, second{};
      ASSERT_TRUE(api.FPDFPath_GetBezierControlPoints(path.get(), endpoint, &first, &second));
      EXPECT_FLOAT_EQ(first.x, std::get<1>(expected[endpoint - 2]));
      EXPECT_FLOAT_EQ(first.y, std::get<2>(expected[endpoint - 2]));
      EXPECT_FLOAT_EQ(second.x, std::get<1>(expected[endpoint - 1]));
      EXPECT_FLOAT_EQ(second.y, std::get<2>(expected[endpoint - 1]));
    }
    ASSERT_TRUE(api.FPDFPageObj_SetStrokeWidth(path.get(), 2));
    ASSERT_TRUE(api.FPDFPageObj_SetStrokeColor(path.get(), 16, 64, 160, 255));
    ASSERT_TRUE(api.FPDFPath_SetDrawMode(path.get(), FPDF_FILLMODE_NONE, 1));
    api.FPDFPage_InsertObject(page.get(), path.release());
    before[writer_index++] = Render(api, page.get(), 128, 128);
    ASSERT_NO_FATAL_FAILURE(CheckPathRoundTrip(*writer, document.get(), page.get(),
        artifacts, writer->name() + "-subpaths", [&](const Api& read, FPDF_PAGE p) {
          ASSERT_EQ(read.FPDFPage_CountObjects(p), 1);
          const auto object = read.FPDFPage_GetObject(p, 0);
          ASSERT_NE(object, nullptr);
          EXPECT_EQ(ReadSegments(read, object), expected);
        }));
  }
  CompareRaster(before[0], before[1], artifacts, "subpaths-native-rust");
}

TEST(Paths, SegmentAndBezierFailureOutputsRemainUntouched) {
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle image(api.FPDFPageObj_NewImageObj(document.get()), api.FPDFPageObj_Destroy);
    ASSERT_NE(image.get(), nullptr);
    Handle path(api.FPDFPageObj_CreateNewPath(1, 2), api.FPDFPageObj_Destroy);
    ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 3, 4));
    ASSERT_TRUE(api.FPDFPath_BezierTo(path.get(), 5, 6, 7, 8, 9, 10));
    for (auto wrong : {FPDF_PAGEOBJECT{}, image.get()}) {
      EXPECT_EQ(api.FPDFPath_CountSegments(wrong), -1);
      EXPECT_EQ(api.FPDFPath_GetPathSegment(wrong, 0), nullptr);
      EXPECT_FALSE(api.FPDFPath_MoveTo(wrong, 1, 2));
      EXPECT_FALSE(api.FPDFPath_LineTo(wrong, 3, 4));
      EXPECT_FALSE(api.FPDFPath_BezierTo(wrong, 1, 2, 3, 4, 5, 6));
      EXPECT_FALSE(api.FPDFPath_Close(wrong));
      EXPECT_FALSE(api.FPDFPath_SetDrawMode(wrong, FPDF_FILLMODE_WINDING, 1));
      int mode = 91, stroke = 92;
      EXPECT_FALSE(api.FPDFPath_GetDrawMode(wrong, &mode, &stroke));
      EXPECT_EQ(mode, 91);
      EXPECT_EQ(stroke, 92);
      FS_POINTF first{91, 92}, second{93, 94};
      EXPECT_FALSE(api.FPDFPath_GetBezierControlPoints(wrong, 4, &first, &second));
      EXPECT_EQ(first.x, 91);
      EXPECT_EQ(second.y, 94);
    }
    EXPECT_EQ(api.FPDFPageObj_GetType(image.get()), FPDF_PAGEOBJ_IMAGE);
    for (std::size_t index : {0u, 1u, 2u, 3u, 5u, 99u}) {
      FS_POINTF first{91, 92}, second{93, 94};
      EXPECT_FALSE(api.FPDFPath_GetBezierControlPoints(path.get(), index, &first, &second));
      EXPECT_FLOAT_EQ(first.x, 91); EXPECT_FLOAT_EQ(first.y, 92);
      EXPECT_FLOAT_EQ(second.x, 93); EXPECT_FLOAT_EQ(second.y, 94);
    }
    FS_POINTF output{91, 92};
    EXPECT_FALSE(api.FPDFPath_GetBezierControlPoints(path.get(), 4, nullptr, &output));
    EXPECT_FLOAT_EQ(output.x, 91); EXPECT_FLOAT_EQ(output.y, 92);
    EXPECT_FALSE(api.FPDFPath_GetBezierControlPoints(path.get(), 4, &output, nullptr));
    EXPECT_FLOAT_EQ(output.x, 91); EXPECT_FLOAT_EQ(output.y, 92);
    const auto segment = api.FPDFPath_GetPathSegment(path.get(), 0);
    ASSERT_NE(segment, nullptr);
    float value = 91;
    EXPECT_FALSE(api.FPDFPathSegment_GetPoint(segment, nullptr, &value));
    EXPECT_FLOAT_EQ(value, 91);
    EXPECT_FALSE(api.FPDFPathSegment_GetPoint(segment, &value, nullptr));
    EXPECT_FLOAT_EQ(value, 91);
    float other = 92;
    EXPECT_FALSE(api.FPDFPathSegment_GetPoint(nullptr, &value, &other));
    EXPECT_FLOAT_EQ(value, 91); EXPECT_FLOAT_EQ(other, 92);
    EXPECT_EQ(api.FPDFPathSegment_GetType(nullptr), FPDF_SEGMENT_UNKNOWN);
    EXPECT_FALSE(api.FPDFPathSegment_GetClose(nullptr));
    int mode = 91, stroke = 92;
    EXPECT_FALSE(api.FPDFPath_GetDrawMode(path.get(), nullptr, &stroke));
    EXPECT_EQ(stroke, 92);
    EXPECT_FALSE(api.FPDFPath_GetDrawMode(path.get(), &mode, nullptr));
    EXPECT_EQ(mode, 91);
  }
}

TEST(Paths, CloseInitialOnlyAndSeparateSubpaths) {
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle path(api.FPDFPageObj_CreateNewPath(4, 8), api.FPDFPageObj_Destroy);
    ASSERT_TRUE(api.FPDFPath_Close(path.get()));
    ASSERT_TRUE(api.FPDFPath_Close(path.get()));
    EXPECT_EQ(ReadSegments(api, path.get()),
              (std::vector<Segment>{{FPDF_SEGMENT_MOVETO, 4, 8, true}}));
    ASSERT_TRUE(api.FPDFPath_MoveTo(path.get(), 12, 16));
    ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 24, 16));
    ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 18, 28));
    ASSERT_TRUE(api.FPDFPath_Close(path.get()));
    EXPECT_EQ(ReadSegments(api, path.get()), (std::vector<Segment>{
        {FPDF_SEGMENT_MOVETO, 4, 8, true}, {FPDF_SEGMENT_MOVETO, 12, 16, false},
        {FPDF_SEGMENT_LINETO, 24, 16, false}, {FPDF_SEGMENT_LINETO, 18, 28, true}}));
  }
}

void AppendSquare(const Api& api, FPDF_PAGEOBJECT path,
                  float left, float bottom, float size, bool start) {
  if (!start) { ASSERT_TRUE(api.FPDFPath_MoveTo(path, left, bottom)); }
  ASSERT_TRUE(api.FPDFPath_LineTo(path, left + size, bottom));
  ASSERT_TRUE(api.FPDFPath_LineTo(path, left + size, bottom + size));
  ASSERT_TRUE(api.FPDFPath_LineTo(path, left, bottom + size));
  ASSERT_TRUE(api.FPDFPath_LineTo(path, left, bottom));
  ASSERT_TRUE(api.FPDFPath_Close(path));
}

TEST(Paths, EveryDrawModePersistsAndNestedFillRulesDiffer) {
  Artifacts artifacts;
  for (int mode : {FPDF_FILLMODE_NONE, FPDF_FILLMODE_ALTERNATE, FPDF_FILLMODE_WINDING}) {
    for (int stroke : {0, 1}) {
      SCOPED_TRACE("mode=" + std::to_string(mode) + ",stroke=" + std::to_string(stroke));
      std::array<Raster, 2> rendered;
      std::size_t writer_index = 0;
      for (const auto* writer : Implementations()) {
        SCOPED_TRACE(writer->name());
        const auto& api = writer->api();
        Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
        Handle page(api.FPDFPage_New(document.get(), 0, 128, 128), api.FPDF_ClosePage);
        ASSERT_NE(page.get(), nullptr);
        Handle path(api.FPDFPageObj_CreateNewPath(16, 16), api.FPDFPageObj_Destroy);
        ASSERT_NO_FATAL_FAILURE(AppendSquare(api, path.get(), 16, 16, 80, true));
        ASSERT_NO_FATAL_FAILURE(AppendSquare(api, path.get(), 40, 40, 32, false));
        ASSERT_TRUE(api.FPDFPath_SetDrawMode(path.get(), mode, stroke));
        ASSERT_TRUE(api.FPDFPageObj_SetFillColor(path.get(), 200, 40, 80, 255));
        ASSERT_TRUE(api.FPDFPageObj_SetStrokeColor(path.get(), 16, 32, 64, 255));
        ASSERT_TRUE(api.FPDFPageObj_SetStrokeWidth(path.get(), 4));
        int actual_mode = -1, actual_stroke = -1;
        ASSERT_TRUE(api.FPDFPath_GetDrawMode(path.get(), &actual_mode, &actual_stroke));
        EXPECT_EQ(actual_mode, mode); EXPECT_EQ(actual_stroke, stroke);
        api.FPDFPage_InsertObject(page.get(), path.release());
        rendered[writer_index] = Render(api, page.get(), 128, 128);
        const auto& pixels = rendered[writer_index++].pixels;
        ASSERT_EQ(pixels.size(), 128u * 128u * 4u);
        const std::size_t center = (72u * 128u + 56u) * 4u;
        EXPECT_EQ(pixels[center], mode == FPDF_FILLMODE_WINDING ? 80 : 255);
        const std::size_t ring = (104u * 128u + 24u) * 4u;
        EXPECT_EQ(pixels[ring], mode == FPDF_FILLMODE_NONE ? 255 : 80);
        if (mode == FPDF_FILLMODE_NONE && stroke == 0) {
          // A no-paint object has no observable serialized representation.
          EXPECT_TRUE(std::all_of(pixels.begin(), pixels.end(), [](uint8_t c) { return c == 255; }));
          ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
          const auto bytes = SaveDocument(api, document.get(), artifacts,
                                           writer->name() + "-no-paint");
          ASSERT_FALSE(bytes.empty());
          for (const auto* reader : Implementations()) {
            const auto& read = reader->api();
            Handle reopened(read.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr),
                            read.FPDF_CloseDocument);
            ASSERT_NE(reopened.get(), nullptr);
            Handle loaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
            ASSERT_NE(loaded.get(), nullptr);
            CompareRaster(rendered[writer_index - 1], Render(read, loaded.get(), 128, 128),
                          artifacts, writer->name() + "-no-paint-to-" + reader->name());
          }
          continue;
        }
        ASSERT_NO_FATAL_FAILURE(CheckPathRoundTrip(*writer, document.get(), page.get(), artifacts,
            writer->name() + "-draw-" + std::to_string(mode) + "-" + std::to_string(stroke),
            [&](const Api& read, FPDF_PAGE p) {
              ASSERT_EQ(read.FPDFPage_CountObjects(p), 1);
              int fill = -1, line = -1;
              ASSERT_TRUE(read.FPDFPath_GetDrawMode(read.FPDFPage_GetObject(p, 0), &fill, &line));
              EXPECT_EQ(fill, mode); EXPECT_EQ(line, stroke);
            }));
      }
      CompareRaster(rendered[0], rendered[1], artifacts,
                    "draw-mode-" + std::to_string(mode) + "-" + std::to_string(stroke));
    }
  }
}

TEST(Paths, StrokeCapsJoinsAndWidthsRoundTrip) {
  Artifacts artifacts;
  for (int cap : {FPDF_LINECAP_BUTT, FPDF_LINECAP_ROUND, FPDF_LINECAP_PROJECTING_SQUARE}) {
    for (int join : {FPDF_LINEJOIN_MITER, FPDF_LINEJOIN_ROUND, FPDF_LINEJOIN_BEVEL}) {
      std::array<Raster, 2> rendered;
      std::size_t index = 0;
      for (const auto* writer : Implementations()) {
        SCOPED_TRACE(writer->name() + ":cap=" + std::to_string(cap) + ",join=" + std::to_string(join));
        const auto& api = writer->api();
        Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
        Handle page(api.FPDFPage_New(document.get(), 0, 128, 128), api.FPDF_ClosePage);
        Handle path(api.FPDFPageObj_CreateNewPath(24, 32), api.FPDFPageObj_Destroy);
        ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 64, 96));
        ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 104, 32));
        EXPECT_EQ(api.FPDFPageObj_GetLineCap(path.get()), FPDF_LINECAP_BUTT);
        EXPECT_EQ(api.FPDFPageObj_GetLineJoin(path.get()), FPDF_LINEJOIN_MITER);
        ASSERT_TRUE(api.FPDFPageObj_SetLineCap(path.get(), cap));
        ASSERT_TRUE(api.FPDFPageObj_SetLineJoin(path.get(), join));
        for (float width : {0.0f, 0.75f, 12.0f}) {
          ASSERT_TRUE(api.FPDFPageObj_SetStrokeWidth(path.get(), width));
          float actual = -1;
          ASSERT_TRUE(api.FPDFPageObj_GetStrokeWidth(path.get(), &actual));
          EXPECT_FLOAT_EQ(actual, width);
        }
        EXPECT_FALSE(api.FPDFPageObj_SetStrokeWidth(path.get(), -1));
        for (int invalid : {-1, 3, 99}) {
          EXPECT_FALSE(api.FPDFPageObj_SetLineCap(path.get(), invalid));
          EXPECT_FALSE(api.FPDFPageObj_SetLineJoin(path.get(), invalid));
          EXPECT_EQ(api.FPDFPageObj_GetLineCap(path.get()), cap);
          EXPECT_EQ(api.FPDFPageObj_GetLineJoin(path.get()), join);
        }
        ASSERT_TRUE(api.FPDFPath_SetDrawMode(path.get(), FPDF_FILLMODE_NONE, 1));
        api.FPDFPage_InsertObject(page.get(), path.release());
        rendered[index++] = Render(api, page.get(), 128, 128);
        ASSERT_NO_FATAL_FAILURE(CheckPathRoundTrip(*writer, document.get(), page.get(), artifacts,
            writer->name() + "-cap-" + std::to_string(cap) + "-join-" + std::to_string(join),
            [&](const Api& read, FPDF_PAGE p) {
              const auto object = read.FPDFPage_GetObject(p, 0);
              ASSERT_NE(object, nullptr);
              EXPECT_EQ(read.FPDFPageObj_GetLineCap(object), cap);
              EXPECT_EQ(read.FPDFPageObj_GetLineJoin(object), join);
              float width = -1;
              ASSERT_TRUE(read.FPDFPageObj_GetStrokeWidth(object, &width));
              EXPECT_FLOAT_EQ(width, 12);
            }));
      }
      CompareRaster(rendered[0], rendered[1], artifacts,
                    "cap-" + std::to_string(cap) + "-join-" + std::to_string(join));
    }
  }
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    EXPECT_EQ(api.FPDFPageObj_GetLineCap(nullptr), -1);
    EXPECT_EQ(api.FPDFPageObj_GetLineJoin(nullptr), -1);
    EXPECT_FALSE(api.FPDFPageObj_SetLineCap(nullptr, 0));
    EXPECT_FALSE(api.FPDFPageObj_SetLineJoin(nullptr, 0));
    EXPECT_FALSE(api.FPDFPageObj_SetStrokeWidth(nullptr, 1));
    float width = 91;
    EXPECT_FALSE(api.FPDFPageObj_GetStrokeWidth(nullptr, &width));
    EXPECT_FLOAT_EQ(width, 91);
  }
}

TEST(Paths, SelfIntersectingStarDistinguishesFillRulesAfterReload) {
  Artifacts artifacts;
  for (int mode : {FPDF_FILLMODE_ALTERNATE, FPDF_FILLMODE_WINDING}) {
    std::array<Raster, 2> rendered;
    std::size_t index = 0;
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name() + ":fill=" + std::to_string(mode));
      const auto& api = writer->api();
      Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      Handle page(api.FPDFPage_New(document.get(), 0, 128, 128), api.FPDF_ClosePage);
      Handle star(api.FPDFPageObj_CreateNewPath(64, 112), api.FPDFPageObj_Destroy);
      for (const auto& point : {std::array<float, 2>{96, 16}, {16, 80},
                                {112, 80}, {32, 16}, {64, 112}}) {
        ASSERT_TRUE(api.FPDFPath_LineTo(star.get(), point[0], point[1]));
      }
      ASSERT_TRUE(api.FPDFPath_Close(star.get()));
      ASSERT_TRUE(api.FPDFPath_SetDrawMode(star.get(), mode, 0));
      ASSERT_TRUE(api.FPDFPageObj_SetFillColor(star.get(), 200, 40, 80, 255));
      api.FPDFPage_InsertObject(page.get(), star.release());
      rendered[index++] = Render(api, page.get(), 128, 128);
      ASSERT_EQ(rendered[index - 1].pixels.size(), 128u * 128u * 4u);
      const std::size_t center = (64u * 128u + 64u) * 4u;
      EXPECT_EQ(rendered[index - 1].pixels[center],
                mode == FPDF_FILLMODE_WINDING ? 80 : 255);
      ASSERT_NO_FATAL_FAILURE(CheckPathRoundTrip(*writer, document.get(), page.get(), artifacts,
          writer->name() + "-star-" + std::to_string(mode),
          [&](const Api& read, FPDF_PAGE p) {
            ASSERT_EQ(read.FPDFPage_CountObjects(p), 1);
            int fill = -1, stroke = -1;
            ASSERT_TRUE(read.FPDFPath_GetDrawMode(read.FPDFPage_GetObject(p, 0), &fill, &stroke));
            EXPECT_EQ(fill, mode);
            EXPECT_EQ(stroke, 0);
          }));
    }
    CompareRaster(rendered[0], rendered[1], artifacts,
                  "star-mode-" + std::to_string(mode));
  }
}

TEST(Paths, FillAndStrokeColorsAreIndependentAndValidateChannels) {
  Artifacts artifacts;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, 128, 128), api.FPDF_ClosePage);
    Handle path(api.FPDFPageObj_CreateNewRect(16, 16, 80, 80), api.FPDFPageObj_Destroy);
    const std::array<unsigned int, 4> black{0, 0, 0, 255};
    EXPECT_EQ(Color(api, path.get(), false), black);
    EXPECT_EQ(Color(api, path.get(), true), black);
    for (const auto& rgb : {std::array<unsigned int, 3>{255, 0, 0}, {0, 255, 0},
                            {0, 0, 255}, {0, 0, 0}, {255, 255, 255}}) {
      for (unsigned int alpha : {0u, 1u, 127u, 254u, 255u}) {
        ASSERT_TRUE(api.FPDFPageObj_SetFillColor(path.get(), rgb[0], rgb[1], rgb[2], alpha));
        EXPECT_EQ(Color(api, path.get(), false),
                  (std::array<unsigned int, 4>{rgb[0], rgb[1], rgb[2], alpha}));
        EXPECT_EQ(Color(api, path.get(), true), black);
      }
    }
    ASSERT_TRUE(api.FPDFPageObj_SetFillColor(path.get(), 64, 96, 128, 127));
    for (const auto& rgb : {std::array<unsigned int, 3>{255, 0, 0}, {0, 255, 0},
                            {0, 0, 255}, {0, 0, 0}, {255, 255, 255}}) {
      for (unsigned int alpha : {0u, 1u, 127u, 254u, 255u}) {
        ASSERT_TRUE(api.FPDFPageObj_SetStrokeColor(path.get(), rgb[0], rgb[1], rgb[2], alpha));
        EXPECT_EQ(Color(api, path.get(), true),
                  (std::array<unsigned int, 4>{rgb[0], rgb[1], rgb[2], alpha}));
        EXPECT_EQ(Color(api, path.get(), false), (std::array<unsigned int, 4>{64, 96, 128, 127}));
      }
    }
    for (bool stroke : {false, true}) {
      const auto set = stroke ? api.FPDFPageObj_SetStrokeColor : api.FPDFPageObj_SetFillColor;
      const auto previous = Color(api, path.get(), stroke);
      for (int channel = 0; channel < 4; ++channel) {
        auto invalid = previous;
        invalid[channel] = 256;
        EXPECT_FALSE(set(path.get(), invalid[0], invalid[1], invalid[2], invalid[3]));
        EXPECT_EQ(Color(api, path.get(), stroke), previous);
      }
      EXPECT_FALSE(set(nullptr, 1, 2, 3, 4));
      const auto get = stroke ? api.FPDFPageObj_GetStrokeColor : api.FPDFPageObj_GetFillColor;
      for (int omitted = 0; omitted < 4; ++omitted) {
        std::array<unsigned int, 4> values{91, 92, 93, 94};
        std::array<unsigned int*, 4> outputs{&values[0], &values[1], &values[2], &values[3]};
        outputs[omitted] = nullptr;
        EXPECT_FALSE(get(path.get(), outputs[0], outputs[1], outputs[2], outputs[3]));
        EXPECT_EQ(values, (std::array<unsigned int, 4>{91, 92, 93, 94}));
      }
    }
    ASSERT_TRUE(api.FPDFPath_SetDrawMode(path.get(), FPDF_FILLMODE_WINDING, 1));
    ASSERT_TRUE(api.FPDFPageObj_SetStrokeWidth(path.get(), 4));
    api.FPDFPage_InsertObject(page.get(), path.release());
    ASSERT_NO_FATAL_FAILURE(CheckPathRoundTrip(*library, document.get(), page.get(), artifacts,
        library->name() + "-colors", [&](const Api& read, FPDF_PAGE p) {
          const auto object = read.FPDFPage_GetObject(p, 0);
          ASSERT_NE(object, nullptr);
          EXPECT_EQ(Color(read, object, false), (std::array<unsigned int, 4>{64, 96, 128, 127}));
          EXPECT_EQ(Color(read, object, true), (std::array<unsigned int, 4>{255, 255, 255, 255}));
        }));
  }
}

TEST(Paths, DashArraysCapacitiesPhasesAndPersistence) {
  Artifacts artifacts;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    const auto& api = writer->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, 128, 128), api.FPDF_ClosePage);
    Handle path(api.FPDFPageObj_CreateNewPath(16, 32), api.FPDFPageObj_Destroy);
    ASSERT_TRUE(api.FPDFPath_LineTo(path.get(), 112, 32));
    EXPECT_EQ(api.FPDFPageObj_GetDashCount(path.get()), 0);
    float phase = 91;
    ASSERT_TRUE(api.FPDFPageObj_GetDashPhase(path.get(), &phase));
    // With no dash array the path is solid, regardless of the initial phase.
    // Set a meaningful value explicitly: an unmaterialized graph state and a
    // newly allocated default state need not expose the same incidental phase.
    EXPECT_TRUE(std::isfinite(phase));
    ASSERT_TRUE(api.FPDFPageObj_SetDashPhase(path.get(), 0));
    ASSERT_TRUE(api.FPDFPageObj_GetDashPhase(path.get(), &phase));
    EXPECT_FLOAT_EQ(phase, 0);
    const std::array<std::vector<float>, 4> patterns = {{{}, {5}, {4, 2}, {3, 1, 2}}};
    for (const auto& pattern : patterns) {
      for (float requested_phase : {0.0f, 1.25f, -2.5f, 21.0f}) {
        ASSERT_TRUE(api.FPDFPageObj_SetDashArray(path.get(), pattern.data(), pattern.size(), requested_phase));
        EXPECT_EQ(api.FPDFPageObj_GetDashCount(path.get()), static_cast<int>(pattern.size()));
        ASSERT_TRUE(api.FPDFPageObj_GetDashPhase(path.get(), &phase));
        EXPECT_FLOAT_EQ(phase, requested_phase);
        std::vector<float> values(pattern.size() + 2, 91);
        ASSERT_TRUE(api.FPDFPageObj_GetDashArray(path.get(), values.data(), pattern.size()));
        EXPECT_TRUE(std::equal(pattern.begin(), pattern.end(), values.begin()));
        EXPECT_FLOAT_EQ(values[pattern.size()], 91);
        std::fill(values.begin(), values.end(), 91);
        ASSERT_TRUE(api.FPDFPageObj_GetDashArray(path.get(), values.data(), values.size()));
        EXPECT_TRUE(std::equal(pattern.begin(), pattern.end(), values.begin()));
        EXPECT_FLOAT_EQ(values[pattern.size()], 91);
        if (!pattern.empty()) {
          std::fill(values.begin(), values.end(), 91);
          EXPECT_FALSE(api.FPDFPageObj_GetDashArray(path.get(), values.data(), pattern.size() - 1));
          EXPECT_TRUE(std::all_of(values.begin(), values.end(), [](float v) { return v == 91; }));
        }
        ASSERT_TRUE(api.FPDFPageObj_SetDashPhase(path.get(), requested_phase + 0.5f));
        EXPECT_EQ(api.FPDFPageObj_GetDashCount(path.get()), static_cast<int>(pattern.size()));
        ASSERT_TRUE(api.FPDFPageObj_GetDashPhase(path.get(), &phase));
        EXPECT_FLOAT_EQ(phase, requested_phase + 0.5f);
      }
    }
    EXPECT_FALSE(api.FPDFPageObj_SetDashArray(path.get(), nullptr, 1, 0));
    EXPECT_FALSE(api.FPDFPageObj_GetDashArray(path.get(), nullptr, 0));
    EXPECT_FALSE(api.FPDFPageObj_GetDashPhase(path.get(), nullptr));
    EXPECT_EQ(api.FPDFPageObj_GetDashCount(nullptr), -1);
    EXPECT_FALSE(api.FPDFPageObj_SetDashPhase(nullptr, 1));
    EXPECT_FALSE(api.FPDFPageObj_SetDashArray(nullptr, nullptr, 0, 0));
    // The setter stores negative entries without validation. Observe that
    // bounded input contract, then restore a valid pattern before rendering.
    const std::array<float,2> negative_dash{-2,4};
    ASSERT_TRUE(api.FPDFPageObj_SetDashArray(path.get(),negative_dash.data(),negative_dash.size(),0));
    std::array<float,2> observed_negative{};
    ASSERT_TRUE(api.FPDFPageObj_GetDashArray(path.get(),observed_negative.data(),observed_negative.size()));
    EXPECT_EQ(observed_negative,negative_dash);
    const std::array<float, 3> final_dash{6, 2, 1};
    ASSERT_TRUE(api.FPDFPageObj_SetDashArray(path.get(), final_dash.data(), final_dash.size(), 1.25f));
    ASSERT_TRUE(api.FPDFPageObj_SetStrokeWidth(path.get(), 4));
    ASSERT_TRUE(api.FPDFPageObj_SetLineCap(path.get(), FPDF_LINECAP_PROJECTING_SQUARE));
    ASSERT_TRUE(api.FPDFPath_SetDrawMode(path.get(), FPDF_FILLMODE_NONE, 1));
    api.FPDFPage_InsertObject(page.get(), path.release());
    ASSERT_NO_FATAL_FAILURE(CheckPathRoundTrip(*writer, document.get(), page.get(), artifacts,
        writer->name() + "-dashes", [&](const Api& read, FPDF_PAGE p) {
          const auto object = read.FPDFPage_GetObject(p, 0);
          ASSERT_NE(object, nullptr);
          ASSERT_EQ(read.FPDFPageObj_GetDashCount(object), 3);
          std::array<float, 3> values{};
          ASSERT_TRUE(read.FPDFPageObj_GetDashArray(object, values.data(), values.size()));
          EXPECT_EQ(values, final_dash);
          float value = 91;
          ASSERT_TRUE(read.FPDFPageObj_GetDashPhase(object, &value));
          EXPECT_FLOAT_EQ(value, 1.25f);
        }));
  }
}

TEST(Paths, MatrixReplacementCompositionAndNullValidation) {
  Artifacts artifacts;
  const FS_MATRIX identity{1, 0, 0, 1, 0, 0};
  const FS_MATRIX translation{1, 0, 0, 1, 8.25f, 12.5f};
  const FS_MATRIX scale{2, 0, 0, 3, 0, 0};
  const std::array<FS_MATRIX, 6> matrices = {{identity, translation, scale,
      {0, 1, -1, 0, 96, 0}, {1, 0.25f, 0.5f, 1, 8, 4}, {-1, 0, 0, 1, 96, 0}}};
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    const auto& api = writer->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, 128, 128), api.FPDF_ClosePage);
    Handle path(api.FPDFPageObj_CreateNewRect(8, 8, 20, 12), api.FPDFPageObj_Destroy);
    ExpectMatrix(api, path.get(), identity);
    for (const auto& matrix : matrices) {
      ASSERT_TRUE(api.FPDFPageObj_SetMatrix(path.get(), &translation));
      ASSERT_TRUE(api.FPDFPageObj_SetMatrix(path.get(), &matrix));
      ExpectMatrix(api, path.get(), matrix);
      ASSERT_TRUE(api.FPDFPageObj_SetMatrix(path.get(), &identity));
      ASSERT_TRUE(api.FPDFPageObj_TransformF(path.get(), &matrix));
      ExpectMatrix(api, path.get(), matrix);
      ASSERT_TRUE(api.FPDFPageObj_SetMatrix(path.get(), &identity));
      api.FPDFPageObj_Transform(path.get(), matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f);
      ExpectMatrix(api, path.get(), matrix);
    }
    ASSERT_TRUE(api.FPDFPageObj_SetMatrix(path.get(), &translation));
    ASSERT_TRUE(api.FPDFPageObj_TransformF(path.get(), &scale));
    const FS_MATRIX composed{2, 0, 0, 3, 16.5f, 37.5f};
    ExpectMatrix(api, path.get(), composed);
    ASSERT_TRUE(api.FPDFPageObj_SetMatrix(path.get(), &scale));
    ASSERT_TRUE(api.FPDFPageObj_TransformF(path.get(), &translation));
    ExpectMatrix(api, path.get(), FS_MATRIX{2, 0, 0, 3, 8.25f, 12.5f});
    EXPECT_FALSE(api.FPDFPageObj_SetMatrix(path.get(), nullptr));
    EXPECT_FALSE(api.FPDFPageObj_TransformF(path.get(), nullptr));
    ExpectMatrix(api, path.get(), FS_MATRIX{2, 0, 0, 3, 8.25f, 12.5f});
    FS_MATRIX sentinel{91, 92, 93, 94, 95, 96};
    EXPECT_FALSE(api.FPDFPageObj_GetMatrix(nullptr, &sentinel));
    EXPECT_FLOAT_EQ(sentinel.a, 91); EXPECT_FLOAT_EQ(sentinel.f, 96);
    EXPECT_FALSE(api.FPDFPageObj_GetMatrix(path.get(), nullptr));
    EXPECT_FALSE(api.FPDFPageObj_SetMatrix(nullptr, &identity));
    EXPECT_FALSE(api.FPDFPageObj_TransformF(nullptr, &identity));
    ASSERT_TRUE(api.FPDFPath_SetDrawMode(path.get(), FPDF_FILLMODE_WINDING, 0));
    ASSERT_TRUE(api.FPDFPageObj_SetFillColor(path.get(), 32, 96, 160, 255));
    api.FPDFPage_InsertObject(page.get(), path.release());
    ASSERT_NO_FATAL_FAILURE(CheckPathRoundTrip(*writer, document.get(), page.get(), artifacts,
        writer->name() + "-matrices", [&](const Api& read, FPDF_PAGE p) {
          const auto object = read.FPDFPage_GetObject(p, 0);
          ASSERT_NE(object, nullptr);
          ExpectMatrix(read, object, FS_MATRIX{2, 0, 0, 3, 8.25f, 12.5f});
        }));
  }
}

TEST(Paths, GlyphContoursUseTheSameBorrowedSegmentInterface) {
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle font(api.FPDFText_LoadStandardFont(document.get(), "Helvetica"), api.FPDFFont_Close);
    ASSERT_NE(font.get(), nullptr);
    bool saw_line = false, saw_curve = false, saw_close = false;
    for (uint32_t character : {uint32_t('A'), uint32_t('S')}) {
      const auto glyph = api.FPDFFont_GetGlyphPath(font.get(), character, 24);
      ASSERT_NE(glyph, nullptr);
      const int count = api.FPDFGlyphPath_CountGlyphSegments(glyph);
      ASSERT_GT(count, 0);
      EXPECT_EQ(api.FPDFGlyphPath_GetGlyphPathSegment(glyph, -1), nullptr);
      EXPECT_EQ(api.FPDFGlyphPath_GetGlyphPathSegment(glyph, count), nullptr);
      for (int index = 0; index < count; ++index) {
        const auto segment = api.FPDFGlyphPath_GetGlyphPathSegment(glyph, index);
        ASSERT_NE(segment, nullptr);
        float x = 0, y = 0;
        ASSERT_TRUE(api.FPDFPathSegment_GetPoint(segment, &x, &y));
        EXPECT_TRUE(std::isfinite(x)); EXPECT_TRUE(std::isfinite(y));
        const int type = api.FPDFPathSegment_GetType(segment);
        EXPECT_TRUE(type == FPDF_SEGMENT_MOVETO || type == FPDF_SEGMENT_LINETO ||
                    type == FPDF_SEGMENT_BEZIERTO);
        if (index == 0) { EXPECT_EQ(type, FPDF_SEGMENT_MOVETO); }
        saw_line |= type == FPDF_SEGMENT_LINETO;
        saw_curve |= type == FPDF_SEGMENT_BEZIERTO;
        saw_close |= !!api.FPDFPathSegment_GetClose(segment);
      }
    }
    EXPECT_TRUE(saw_line); EXPECT_TRUE(saw_curve); EXPECT_TRUE(saw_close);
    EXPECT_EQ(api.FPDFGlyphPath_CountGlyphSegments(nullptr), -1);
    EXPECT_EQ(api.FPDFGlyphPath_GetGlyphPathSegment(nullptr, 0), nullptr);
  }
}

TEST(Paths, RectangleDegeneracyAndBoundsPersistWithFillAndStroke) {
  Artifacts artifacts;
  const std::array<std::array<float, 4>, 5> rectangles = {{{16, 16, 48, 32},
      {12.25f, 14.5f, 48.5f, 32.25f}, {16, 16, 0, 32}, {16, 16, 48, 0},
      {80, 80, -48, -32}}};
  for (size_t index = 0; index < rectangles.size(); ++index) {
    std::array<Raster, 2> pictures;
    size_t backend = 0;
    for (const auto* writer : Implementations()) {
      const auto& api = writer->api();
      Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      const auto& r = rectangles[index];
      Handle rectangle(api.FPDFPageObj_CreateNewRect(r[0], r[1], r[2], r[3]), api.FPDFPageObj_Destroy);
      ASSERT_NE(rectangle.get(), nullptr);
      ASSERT_TRUE(api.FPDFPageObj_SetFillColor(rectangle.get(), 224, 64, 32, 255));
      ASSERT_TRUE(api.FPDFPageObj_SetStrokeColor(rectangle.get(), 32, 64, 224, 255));
      ASSERT_TRUE(api.FPDFPageObj_SetStrokeWidth(rectangle.get(), 2));
      ASSERT_TRUE(api.FPDFPath_SetDrawMode(rectangle.get(), FPDF_FILLMODE_WINDING, 1));
      const auto segments = ReadSegments(api, rectangle.get());
      ASSERT_EQ(segments.size(), 5u);
      EXPECT_TRUE(std::get<3>(segments.back()));
      api.FPDFPage_InsertObject(page.get(), rectangle.release());
      auto inspect = [segments](const Api& read, FPDF_PAGE loaded) {
        ASSERT_EQ(read.FPDFPage_CountObjects(loaded), 1);
        auto object = read.FPDFPage_GetObject(loaded, 0);
        const auto actual = ReadSegments(read, object);
        ASSERT_EQ(actual.size(), segments.size());
        auto corners = [](const auto& points) {
          std::set<std::pair<float, float>> result;
          for (const auto& point : points) result.emplace(std::get<1>(point), std::get<2>(point));
          return result;
        };
        EXPECT_EQ(corners(actual), corners(segments));
        EXPECT_EQ(std::get<0>(actual.front()), FPDF_SEGMENT_MOVETO);
        EXPECT_TRUE(std::get<3>(actual.back()));
        std::array<float, 4> bounds{};
        ASSERT_TRUE(read.FPDFPageObj_GetBounds(object, &bounds[0], &bounds[1], &bounds[2], &bounds[3]));
        for (float value : bounds) EXPECT_TRUE(std::isfinite(value));
        EXPECT_LE(bounds[0], bounds[2]); EXPECT_LE(bounds[1], bounds[3]);
      };
      ASSERT_NO_FATAL_FAILURE(CheckPathRoundTrip(*writer, doc.get(), page.get(), artifacts,
          writer->name() + "-rectangle" + std::to_string(index), inspect));
      pictures[backend++] = Render(api, page.get(), 128, 128);
    }
    CompareRaster(pictures[0], pictures[1], artifacts, "rectangle" + std::to_string(index));
  }
}

TEST(Paths, CubicExtremumAndImplicitTriangleReturnEdgeRenderAfterReload) {
  Artifacts artifacts;
  std::array<Raster, 2> pictures;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    auto curve = api.FPDFPageObj_CreateNewPath(16, 16); ASSERT_NE(curve, nullptr);
    ASSERT_TRUE(api.FPDFPath_BezierTo(curve, 16, 112, 112, 112, 112, 16));
    ASSERT_TRUE(api.FPDFPath_Close(curve));
    ASSERT_TRUE(api.FPDFPageObj_SetFillColor(curve, 255, 0, 0, 255));
    ASSERT_TRUE(api.FPDFPath_SetDrawMode(curve, FPDF_FILLMODE_WINDING, 0));
    ASSERT_EQ(api.FPDFPath_CountSegments(curve), 4);
    auto triangle = api.FPDFPageObj_CreateNewPath(88, 96); ASSERT_NE(triangle, nullptr);
    ASSERT_TRUE(api.FPDFPath_LineTo(triangle, 112, 96));
    ASSERT_TRUE(api.FPDFPath_LineTo(triangle, 100, 120));
    ASSERT_TRUE(api.FPDFPath_Close(triangle)); // No explicit line back to origin.
    ASSERT_TRUE(api.FPDFPageObj_SetFillColor(triangle, 0, 0, 255, 255));
    ASSERT_TRUE(api.FPDFPageObj_SetStrokeColor(triangle, 0, 0, 0, 255));
    ASSERT_TRUE(api.FPDFPageObj_SetStrokeWidth(triangle, 1));
    ASSERT_TRUE(api.FPDFPath_SetDrawMode(triangle, FPDF_FILLMODE_WINDING, 1));
    ASSERT_EQ(api.FPDFPath_CountSegments(triangle), 3);
    const auto curve_points = ReadSegments(api, curve);
    const auto triangle_points = ReadSegments(api, triangle);
    EXPECT_TRUE(std::get<3>(curve_points.back())); EXPECT_TRUE(std::get<3>(triangle_points.back()));
    api.FPDFPage_InsertObject(page.get(), curve); api.FPDFPage_InsertObject(page.get(), triangle);
    auto inspect = [curve_points, triangle_points](const Api& read, FPDF_PAGE loaded) {
      ASSERT_EQ(read.FPDFPage_CountObjects(loaded), 2);
      auto normalized = [](std::vector<Segment> points) {
        if (points.size() > 1 && std::get<0>(points.back()) == FPDF_SEGMENT_LINETO &&
            std::get<3>(points.back()) && std::get<1>(points.back()) == std::get<1>(points.front()) &&
            std::get<2>(points.back()) == std::get<2>(points.front())) {
          points.pop_back(); std::get<3>(points.back()) = true;
        }
        return points;
      };
      EXPECT_EQ(normalized(ReadSegments(read, read.FPDFPage_GetObject(loaded, 0))), normalized(curve_points));
      EXPECT_EQ(normalized(ReadSegments(read, read.FPDFPage_GetObject(loaded, 1))), normalized(triangle_points));
      const auto raster = Render(read, loaded, 128, 128);
      ASSERT_EQ(raster.pixels.size(), 128u * 128 * 4);
      // For this cubic y(t)=16+288t(1-t), the peak is y=88 at t=.5.
      // The controls at112 must not make paint appear above the actual curve.
      auto pixel = [&](int x, int y) { const size_t n = (y * 128 + x) * 4; return std::array<uint8_t, 4>{raster.pixels[n], raster.pixels[n + 1], raster.pixels[n + 2], raster.pixels[n + 3]}; };
      EXPECT_EQ(pixel(64, 32), (std::array<uint8_t, 4>{255, 255, 255, 255}));
      EXPECT_EQ(pixel(64, 64), (std::array<uint8_t, 4>{0, 0, 255, 255}));
      EXPECT_EQ(pixel(100, 24), (std::array<uint8_t, 4>{255, 0, 0, 255}));
    };
    ASSERT_NO_FATAL_FAILURE(CheckPathRoundTrip(*writer, doc.get(), page.get(), artifacts,
        writer->name() + "-extrema-close", inspect));
    pictures[backend++] = Render(api, page.get(), 128, 128);
  }
  CompareRaster(pictures[0], pictures[1], artifacts, "cubic-and-implicit-close");
}

TEST(Paths, EveryDashPatternAndPhasePersistsWithEveryLineCap) {
  Artifacts artifacts;
  const std::vector<std::vector<float>> patterns = {{}, {4}, {4, 2}, {2.5f, 3.25f, 1.5f}, {0, 4}};
  for (size_t pattern_index = 0; pattern_index < patterns.size(); ++pattern_index) {
    for (float phase : {0.0f, 1.25f, -2.5f, 64.0f}) for (int cap : {FPDF_LINECAP_BUTT, FPDF_LINECAP_ROUND, FPDF_LINECAP_PROJECTING_SQUARE}) {
      std::array<Raster, 2> pictures;
      size_t backend = 0;
      for (const auto* writer : Implementations()) {
        const auto& api = writer->api();
        Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
        Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
        ASSERT_NE(page.get(), nullptr);
        auto line = api.FPDFPageObj_CreateNewPath(16, 64); ASSERT_NE(line, nullptr);
        ASSERT_TRUE(api.FPDFPath_LineTo(line, 112, 64));
        ASSERT_TRUE(api.FPDFPageObj_SetLineCap(line, cap));
        ASSERT_TRUE(api.FPDFPageObj_SetStrokeWidth(line, 4));
        ASSERT_TRUE(api.FPDFPageObj_SetStrokeColor(line, 32, 96, 160, 255));
        ASSERT_TRUE(api.FPDFPath_SetDrawMode(line, FPDF_FILLMODE_NONE, 1));
        const auto& pattern = patterns[pattern_index];
        ASSERT_TRUE(api.FPDFPageObj_SetDashArray(line, pattern.empty() ? nullptr : pattern.data(), pattern.size(), phase));
        // Changing only the phase must not modify the array.
        ASSERT_TRUE(api.FPDFPageObj_SetDashPhase(line, phase + .5f));
        ASSERT_TRUE(api.FPDFPageObj_SetDashPhase(line, phase));
        float stored_phase = 991;
        ASSERT_TRUE(api.FPDFPageObj_GetDashPhase(line, &stored_phase)); EXPECT_FLOAT_EQ(stored_phase, phase);
        api.FPDFPage_InsertObject(page.get(), line);
        auto inspect = [pattern, phase, cap](const Api& read, FPDF_PAGE loaded) {
          auto line = read.FPDFPage_GetObject(loaded, 0); ASSERT_NE(line, nullptr);
          EXPECT_EQ(read.FPDFPageObj_GetDashCount(line), pattern.size());
          std::vector<float> buffer(pattern.size() + 2, 991);
          ASSERT_TRUE(read.FPDFPageObj_GetDashArray(line, buffer.data(), buffer.size()));
          EXPECT_TRUE(std::equal(pattern.begin(), pattern.end(), buffer.begin()));
          EXPECT_EQ(buffer[pattern.size()], 991);
          float actual_phase = 991;
          ASSERT_TRUE(read.FPDFPageObj_GetDashPhase(line, &actual_phase));
          if (!pattern.empty()) { EXPECT_FLOAT_EQ(actual_phase, phase); }
          else { EXPECT_TRUE(std::isfinite(actual_phase)); }
          EXPECT_EQ(read.FPDFPageObj_GetLineCap(line), cap);
        };
        ASSERT_NO_FATAL_FAILURE(CheckPathRoundTrip(*writer, doc.get(), page.get(), artifacts,
            writer->name() + "-dash" + std::to_string(pattern_index) + "-phase" + std::to_string(phase) + "-cap" + std::to_string(cap), inspect));
        pictures[backend++] = Render(api, page.get(), 128, 128);
      }
      CompareRaster(pictures[0], pictures[1], artifacts,
                    "dash" + std::to_string(pattern_index) + "-phase" + std::to_string(phase) + "-cap" + std::to_string(cap));
    }
  }
}

}  // namespace
}  // namespace pdfoxium::integration
