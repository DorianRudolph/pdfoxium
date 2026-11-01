#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

constexpr int kWidth = 240, kHeight = 200;
const FPDF_WCHAR* Wide(const std::u16string& text) {
  static_assert(sizeof(char16_t) == sizeof(FPDF_WCHAR));
  return reinterpret_cast<const FPDF_WCHAR*>(text.c_str());
}

template <typename Getter>
void ExpectText(Getter get, const std::u16string& expected) {
  const auto needed = static_cast<unsigned long>((expected.size() + 1) * 2);
  ASSERT_EQ(get(nullptr, 0), needed);
  for (unsigned long capacity : {0ul, needed - 1, needed, needed + 6}) {
    std::vector<FPDF_WCHAR> buffer(expected.size() + 8, 0x5a5a);
    const auto initial = buffer;
    EXPECT_EQ(get(buffer.data(), capacity), needed);
    if (capacity < needed) {
      EXPECT_EQ(buffer, initial);
    } else {
      for (size_t index = 0; index < expected.size(); ++index)
        EXPECT_EQ(buffer[index], expected[index]);
      EXPECT_EQ(buffer[expected.size()], 0);
      for (size_t index = expected.size() + 1; index < buffer.size(); ++index)
        EXPECT_EQ(buffer[index], 0x5a5a);
    }
  }
}

void ExpectRect(const Api& api, FPDF_ANNOTATION annot, const FS_RECTF& expected) {
  FS_RECTF actual{};
  ASSERT_TRUE(api.FPDFAnnot_GetRect(annot, &actual));
  EXPECT_FLOAT_EQ(actual.left, expected.left);
  EXPECT_FLOAT_EQ(actual.bottom, expected.bottom);
  EXPECT_FLOAT_EQ(actual.right, expected.right);
  EXPECT_FLOAT_EQ(actual.top, expected.top);
}

void SetAppearance(const Api& api, FPDF_ANNOTATION annot,
                   const std::u16string& stream = u"0.125 0.5 0.875 rg 0 0 240 200 re f") {
  ASSERT_TRUE(api.FPDFAnnot_SetAP(annot, FPDF_ANNOT_APPEARANCEMODE_NORMAL, Wide(stream)));
}

void RoundTrip(const Library& writer, FPDF_DOCUMENT document, FPDF_PAGE page,
               Artifacts& artifacts, const std::string& name,
               const std::function<void(const Api&, FPDF_DOCUMENT, FPDF_PAGE)>& inspect,
               RasterTolerance tolerance = {}) {
  ASSERT_NO_FATAL_FAILURE(inspect(writer.api(), document, page));
  // Rendering may generate and store an AP, changing GetColor's documented
  // availability. Save the edited dictionary state before that lazy work.
  const auto bytes = SaveDocument(writer.api(), document, artifacts, writer.name() + "-" + name);
  ASSERT_FALSE(bytes.empty());
  const auto before = Render(writer.api(), page, kWidth, kHeight, FPDF_ANNOT);
  std::array<Raster, 2> after;
  size_t index = 0;
  for (const auto* reader : Implementations()) {
    SCOPED_TRACE(writer.name() + "->" + reader->name());
    const auto& api = reader->api();
    Handle reloaded(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr),
                    api.FPDF_CloseDocument);
    ASSERT_NE(reloaded.get(), nullptr);
    Handle reopened(api.FPDF_LoadPage(reloaded.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(reopened.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(inspect(api, reloaded.get(), reopened.get()));
    after[index] = Render(api, reopened.get(), kWidth, kHeight, FPDF_ANNOT);
    CompareRaster(before, after[index], artifacts,
                  name + "-" + writer.name() + "-" + reader->name(), tolerance);
    ++index;
  }
  CompareRaster(after[0], after[1], artifacts, name + "-readers-" + writer.name(), tolerance);
}

bool Supported(int subtype) {
  const std::array<int, 13> supported = {
      FPDF_ANNOT_CIRCLE, FPDF_ANNOT_FILEATTACHMENT, FPDF_ANNOT_FREETEXT,
      FPDF_ANNOT_HIGHLIGHT, FPDF_ANNOT_INK, FPDF_ANNOT_LINK, FPDF_ANNOT_POPUP,
      FPDF_ANNOT_SQUARE, FPDF_ANNOT_SQUIGGLY, FPDF_ANNOT_STAMP,
      FPDF_ANNOT_STRIKEOUT, FPDF_ANNOT_TEXT, FPDF_ANNOT_UNDERLINE};
  return std::find(supported.begin(), supported.end(), subtype) != supported.end();
}

TEST(Annotations, EveryDeclaredSubtypeCapabilityMatchesCreationAndPersistence) {
  Artifacts artifacts;
  std::array<Raster, 2> scenes;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDFPage_New(document.get(), 0, kWidth, kHeight), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    EXPECT_EQ(api.FPDFPage_GetAnnotCount(page.get()), 0);
    std::vector<int> types;
    for (int type = FPDF_ANNOT_UNKNOWN; type <= FPDF_ANNOT_REDACT; ++type) {
      SCOPED_TRACE(type);
      EXPECT_EQ(!!api.FPDFAnnot_IsSupportedSubtype(type), Supported(type));
      EXPECT_EQ(!!api.FPDFAnnot_IsObjectSupportedSubtype(type),
                type == FPDF_ANNOT_INK || type == FPDF_ANNOT_STAMP);
      Handle annot(api.FPDFPage_CreateAnnot(page.get(), type), api.FPDFPage_CloseAnnot);
      if (!Supported(type)) {
        EXPECT_EQ(annot.get(), nullptr);
        EXPECT_EQ(api.FPDFPage_GetAnnotCount(page.get()), static_cast<int>(types.size()));
        continue;
      }
      ASSERT_NE(annot.get(), nullptr);
      const int index = static_cast<int>(types.size());
      const float left = 8.0f + (index % 5) * 44;
      const float bottom = 8.0f + (index / 5) * 52;
      const FS_RECTF rect{left, bottom + 36, left + 32, bottom};
      ASSERT_TRUE(api.FPDFAnnot_SetRect(annot.get(), &rect));
      ASSERT_TRUE(api.FPDFAnnot_SetFlags(annot.get(), FPDF_ANNOT_FLAG_PRINT));
      const auto text = std::u16string(u"type-") + static_cast<char16_t>(u'A' + type);
      ASSERT_TRUE(api.FPDFAnnot_SetStringValue(annot.get(), "CaseTag", Wide(text)));
      ASSERT_NO_FATAL_FAILURE(SetAppearance(api, annot.get()));
      EXPECT_EQ(api.FPDFAnnot_GetSubtype(annot.get()), type);
      EXPECT_EQ(api.FPDFPage_GetAnnotIndex(page.get(), annot.get()), index);
      types.push_back(type);
    }
    for (int type : {-1, 99}) {
      EXPECT_FALSE(api.FPDFAnnot_IsSupportedSubtype(type));
      EXPECT_EQ(api.FPDFPage_CreateAnnot(page.get(), type), nullptr);
    }
    EXPECT_EQ(api.FPDFPage_CreateAnnot(nullptr, FPDF_ANNOT_SQUARE), nullptr);
    auto inspect = [&](const Api& read, FPDF_DOCUMENT, FPDF_PAGE loaded) {
      ASSERT_EQ(read.FPDFPage_GetAnnotCount(loaded), static_cast<int>(types.size()));
      for (size_t index = 0; index < types.size(); ++index) {
        Handle annot(read.FPDFPage_GetAnnot(loaded, static_cast<int>(index)),
                     read.FPDFPage_CloseAnnot);
        ASSERT_NE(annot.get(), nullptr);
        EXPECT_EQ(read.FPDFAnnot_GetSubtype(annot.get()), types[index]);
        EXPECT_EQ(read.FPDFPage_GetAnnotIndex(loaded, annot.get()), static_cast<int>(index));
        ExpectText([&](FPDF_WCHAR* buffer, unsigned long length) {
          return read.FPDFAnnot_GetStringValue(annot.get(), "CaseTag", buffer, length);
        }, std::u16string(u"type-") + static_cast<char16_t>(u'A' + types[index]));
      }
      for (int invalid : {-1, static_cast<int>(types.size()), 999})
        EXPECT_EQ(read.FPDFPage_GetAnnot(loaded, invalid), nullptr);
    };
    scenes[backend++] = Render(api, page.get(), kWidth, kHeight, FPDF_ANNOT);
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library, document.get(), page.get(), artifacts,
                                      "all-subtypes", inspect));
  }
  CompareRaster(scenes[0], scenes[1], artifacts, "all-subtypes-backends");
}

TEST(Annotations, HandleClosureAndRemovalPreserveSurvivingOrderAndOtherPages) {
  Artifacts artifacts;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    SCOPED_TRACE(library->name());
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, kWidth, kHeight), api.FPDF_ClosePage);
    Handle other(api.FPDFPage_New(document.get(), 1, kWidth, kHeight), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr); ASSERT_NE(other.get(), nullptr);
    for (int index = 0; index < 6; ++index) {
      Handle annot(api.FPDFPage_CreateAnnot(page.get(), FPDF_ANNOT_STAMP), api.FPDFPage_CloseAnnot);
      const FS_RECTF rect{float(index * 32), 48, float(index * 32 + 24), 16};
      ASSERT_TRUE(api.FPDFAnnot_SetRect(annot.get(), &rect));
      ASSERT_TRUE(api.FPDFAnnot_SetStringValue(annot.get(), "Tag", Wide(std::u16string(1, u'A' + index))));
      ASSERT_NO_FATAL_FAILURE(SetAppearance(api, annot.get()));
    }
    Handle foreign(api.FPDFPage_CreateAnnot(other.get(), FPDF_ANNOT_TEXT), api.FPDFPage_CloseAnnot);
    EXPECT_EQ(api.FPDFPage_GetAnnotIndex(page.get(), foreign.get()), -1);
    EXPECT_EQ(api.FPDFPage_GetAnnotCount(page.get()), 6);
    EXPECT_EQ(api.FPDFPage_GetAnnotCount(other.get()), 1);
    ASSERT_TRUE(api.FPDFPage_RemoveAnnot(page.get(), 0));
    ASSERT_TRUE(api.FPDFPage_RemoveAnnot(page.get(), 2));
    ASSERT_TRUE(api.FPDFPage_RemoveAnnot(page.get(), 3));
    EXPECT_FALSE(api.FPDFPage_RemoveAnnot(page.get(), -1));
    EXPECT_FALSE(api.FPDFPage_RemoveAnnot(page.get(), 3));
    const std::array<std::u16string, 3> expected = {u"B", u"C", u"E"};
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library, document.get(), page.get(), artifacts,
        "removed", [&](const Api& read, FPDF_DOCUMENT, FPDF_PAGE loaded) {
          ASSERT_EQ(read.FPDFPage_GetAnnotCount(loaded), 3);
          for (int index = 0; index < 3; ++index) {
            Handle first(read.FPDFPage_GetAnnot(loaded, index), read.FPDFPage_CloseAnnot);
            Handle independent(read.FPDFPage_GetAnnot(loaded, index), read.FPDFPage_CloseAnnot);
            ASSERT_NE(first.get(), nullptr); ASSERT_NE(independent.get(), nullptr);
            EXPECT_NE(first.get(), independent.get());
            EXPECT_EQ(read.FPDFPage_GetAnnotIndex(loaded, first.get()), index);
            first.reset();
            EXPECT_EQ(read.FPDFPage_GetAnnotCount(loaded), 3);
            ExpectText([&](FPDF_WCHAR* b, unsigned long n) {
              return read.FPDFAnnot_GetStringValue(independent.get(), "Tag", b, n);
            }, expected[index]);
          }
        }));
  }
}

TEST(Annotations, RectanglesAndBordersReplaceValuesAndClearOldAppearance) {
  Artifacts artifacts;
  std::array<Raster, 2> scenes;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, kWidth, kHeight), api.FPDF_ClosePage);
    Handle annot(api.FPDFPage_CreateAnnot(page.get(), FPDF_ANNOT_SQUARE), api.FPDFPage_CloseAnnot);
    ASSERT_NE(annot.get(), nullptr);
    for (const auto& rect : {FS_RECTF{10.25f, 60.5f, 80.75f, 12.125f},
                             FS_RECTF{-4.5f, 35.5f, 22.25f, -2.5f},
                             FS_RECTF{4, 8, 4, 8}}) {
      ASSERT_TRUE(api.FPDFAnnot_SetRect(annot.get(), &rect));
      ExpectRect(api, annot.get(), rect);
    }
    const FS_RECTF final{24, 112, 120, 24};
    ASSERT_TRUE(api.FPDFAnnot_SetRect(annot.get(), &final));
    ASSERT_NO_FATAL_FAILURE(SetAppearance(api, annot.get()));
    for (const auto& border : {std::array<float, 3>{0, 0, 0}, {2.25f, 3.5f, 4.75f}}) {
      ASSERT_TRUE(api.FPDFAnnot_SetBorder(annot.get(), border[0], border[1], border[2]));
      ExpectText([&](FPDF_WCHAR* b, unsigned long n) { return api.FPDFAnnot_GetAP(annot.get(), 0, b, n); }, u"");
      float horizontal = -1, vertical = -2, width = -3;
      ASSERT_TRUE(api.FPDFAnnot_GetBorder(annot.get(), &horizontal, &vertical, &width));
      EXPECT_FLOAT_EQ(horizontal, border[0]); EXPECT_FLOAT_EQ(vertical, border[1]); EXPECT_FLOAT_EQ(width, border[2]);
    }
    float valid = 42;
    EXPECT_FALSE(api.FPDFAnnot_GetBorder(annot.get(), nullptr, &valid, &valid));
    EXPECT_FALSE(api.FPDFAnnot_GetBorder(annot.get(), &valid, nullptr, &valid));
    EXPECT_FALSE(api.FPDFAnnot_GetBorder(annot.get(), &valid, &valid, nullptr));
    ASSERT_TRUE(api.FPDFAnnot_SetColor(annot.get(), FPDFANNOT_COLORTYPE_Color, 32, 96, 224, 255));
    scenes[backend++] = Render(api, page.get(), kWidth, kHeight, FPDF_ANNOT);
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library, document.get(), page.get(), artifacts,
        "rect-border", [&](const Api& read, FPDF_DOCUMENT, FPDF_PAGE loaded) {
          Handle a(read.FPDFPage_GetAnnot(loaded, 0), read.FPDFPage_CloseAnnot);
          ExpectRect(read, a.get(), final);
          float h = 0, v = 0, w = 0;
          ASSERT_TRUE(read.FPDFAnnot_GetBorder(a.get(), &h, &v, &w));
          EXPECT_FLOAT_EQ(h, 2.25f); EXPECT_FLOAT_EQ(v, 3.5f); EXPECT_FLOAT_EQ(w, 4.75f);
        }));
  }
  CompareRaster(scenes[0], scenes[1], artifacts, "borders-backends");
}

std::array<unsigned int, 4> ReadColor(const Api& api, FPDF_ANNOTATION annot,
                                     FPDFANNOT_COLORTYPE which) {
  std::array<unsigned int, 4> color{};
  EXPECT_TRUE(api.FPDFAnnot_GetColor(annot, which, &color[0], &color[1], &color[2], &color[3]));
  return color;
}

TEST(Annotations, ColorDefaultsChannelsOpacityAndAppearanceRestriction) {
  Artifacts artifacts;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, kWidth, kHeight), api.FPDF_ClosePage);
    for (int type : {FPDF_ANNOT_HIGHLIGHT, FPDF_ANNOT_SQUARE}) {
      Handle annot(api.FPDFPage_CreateAnnot(page.get(), type), api.FPDFPage_CloseAnnot);
      const FS_RECTF rect{24, 100, 144, 24};
      ASSERT_TRUE(api.FPDFAnnot_SetRect(annot.get(), &rect));
      EXPECT_EQ(ReadColor(api, annot.get(), FPDFANNOT_COLORTYPE_Color),
                (type == FPDF_ANNOT_HIGHLIGHT ? std::array<unsigned int, 4>{255,255,0,255}
                                            : std::array<unsigned int, 4>{0,0,0,255}));
      for (const auto& color : {std::array<unsigned int, 4>{0,255,127,0},
                                {255,0,1,1}, {17,129,254,127}, {32,96,224,255}}) {
        for (auto which : {FPDFANNOT_COLORTYPE_Color, FPDFANNOT_COLORTYPE_InteriorColor}) {
          ASSERT_TRUE(api.FPDFAnnot_SetColor(annot.get(), which, color[0], color[1], color[2], color[3]));
          EXPECT_EQ(ReadColor(api, annot.get(), which), color);
        }
      }
    }
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library, document.get(), page.get(), artifacts,
        "colors", [&](const Api& read, FPDF_DOCUMENT, FPDF_PAGE loaded) {
          for (int index = 0; index < 2; ++index) {
            Handle a(read.FPDFPage_GetAnnot(loaded, index), read.FPDFPage_CloseAnnot);
            for (auto which : {FPDFANNOT_COLORTYPE_Color, FPDFANNOT_COLORTYPE_InteriorColor})
              EXPECT_EQ(ReadColor(read, a.get(), which), (std::array<unsigned int, 4>{32,96,224,255}));
          }
        }));
    Handle a(api.FPDFPage_GetAnnot(page.get(), 1), api.FPDFPage_CloseAnnot);
    ASSERT_NO_FATAL_FAILURE(SetAppearance(api, a.get()));
    unsigned int r = 0, g = 0, b = 0, alpha = 0;
    EXPECT_FALSE(api.FPDFAnnot_SetColor(a.get(), FPDFANNOT_COLORTYPE_Color, 0,0,0,255));
    EXPECT_FALSE(api.FPDFAnnot_GetColor(a.get(), FPDFANNOT_COLORTYPE_Color, &r,&g,&b,&alpha));
  }
}

void ExpectQuad(const Api& api, FPDF_ANNOTATION annotation, size_t index,
                 const FS_QUADPOINTSF& expected) {
  FS_QUADPOINTSF actual{};
  ASSERT_TRUE(api.FPDFAnnot_GetAttachmentPoints(annotation, index, &actual));
  EXPECT_FLOAT_EQ(actual.x1, expected.x1); EXPECT_FLOAT_EQ(actual.y1, expected.y1);
  EXPECT_FLOAT_EQ(actual.x2, expected.x2); EXPECT_FLOAT_EQ(actual.y2, expected.y2);
  EXPECT_FLOAT_EQ(actual.x3, expected.x3); EXPECT_FLOAT_EQ(actual.y3, expected.y3);
  EXPECT_FLOAT_EQ(actual.x4, expected.x4); EXPECT_FLOAT_EQ(actual.y4, expected.y4);
}

TEST(Annotations, QuadpointCapabilitiesAppendReplaceAndPersistAllSupportedSubtypes) {
  Artifacts artifacts;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(), 0, kWidth, kHeight), api.FPDF_ClosePage);
    std::vector<int> types;
    const std::array<FS_QUADPOINTSF,3> initial = {{
        {20,120,60,120,20,100,60,100}, {70,120,110,120,70,100,110,100},
        {120,120,160,120,120,100,160,100}}};
    std::array<FS_QUADPOINTSF,3> replacement = initial;
    replacement[0] = {12,160,68,160,12,132,68,132};
    replacement[1] = {72,114,106,114,72,102,106,102};
    replacement[2] = {126,116,156,116,126,104,156,104};
    const FS_QUADPOINTSF appended{4,184,204,184,4,168,204,168};
    for (int type = 1; type <= FPDF_ANNOT_REDACT; ++type) {
      if (!Supported(type)) continue;
      Handle a(api.FPDFPage_CreateAnnot(page.get(), type), api.FPDFPage_CloseAnnot);
      const FS_RECTF rect{20,120,160,100};
      ASSERT_TRUE(api.FPDFAnnot_SetRect(a.get(), &rect));
      const bool supported = type == FPDF_ANNOT_HIGHLIGHT || type == FPDF_ANNOT_STRIKEOUT ||
                             type == FPDF_ANNOT_SQUIGGLY || type == FPDF_ANNOT_UNDERLINE || type == FPDF_ANNOT_LINK;
      EXPECT_EQ(!!api.FPDFAnnot_HasAttachmentPoints(a.get()), supported);
      EXPECT_EQ(api.FPDFAnnot_CountAttachmentPoints(a.get()), 0u);
      if (supported) {
        for (const auto& quad : initial) ASSERT_TRUE(api.FPDFAnnot_AppendAttachmentPoints(a.get(), &quad));
        ASSERT_NO_FATAL_FAILURE(SetAppearance(api, a.get()));
        for (size_t index = 0; index < initial.size(); ++index) {
          ASSERT_TRUE(api.FPDFAnnot_SetAttachmentPoints(a.get(), index, &replacement[index]));
          for (size_t other = 0; other < initial.size(); ++other)
            ExpectQuad(api, a.get(), other, other <= index ? replacement[other] : initial[other]);
        }
        // Append beyond the existing appearance BBox, after replacement has
        // already grown it. The original three quads must remain unchanged.
        ASSERT_TRUE(api.FPDFAnnot_AppendAttachmentPoints(a.get(), &appended));
        EXPECT_EQ(api.FPDFAnnot_CountAttachmentPoints(a.get()),4u);
        ExpectQuad(api,a.get(),3,appended);
        for(size_t index=0;index<replacement.size();++index) ExpectQuad(api,a.get(),index,replacement[index]);
        EXPECT_FALSE(api.FPDFAnnot_SetAttachmentPoints(a.get(), 4, &initial[0]));
        FS_QUADPOINTSF output{};
        EXPECT_FALSE(api.FPDFAnnot_GetAttachmentPoints(a.get(), 4, &output));
      } else {
        EXPECT_FALSE(api.FPDFAnnot_AppendAttachmentPoints(a.get(), &initial[0]));
        EXPECT_FALSE(api.FPDFAnnot_SetAttachmentPoints(a.get(), 0, &initial[0]));
        FS_QUADPOINTSF output{};
        EXPECT_FALSE(api.FPDFAnnot_GetAttachmentPoints(a.get(), 0, &output));
        ASSERT_TRUE(api.FPDFAnnot_SetFlags(a.get(), FPDF_ANNOT_FLAG_HIDDEN));
      }
      types.push_back(type);
    }
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library, document.get(), page.get(), artifacts,
        "quadpoints", [&](const Api& read, FPDF_DOCUMENT, FPDF_PAGE loaded) {
          for (size_t index = 0; index < types.size(); ++index) {
            Handle a(read.FPDFPage_GetAnnot(loaded, static_cast<int>(index)), read.FPDFPage_CloseAnnot);
            if (!read.FPDFAnnot_HasAttachmentPoints(a.get())) { EXPECT_EQ(read.FPDFAnnot_CountAttachmentPoints(a.get()),0u); continue; }
            EXPECT_EQ(read.FPDFAnnot_CountAttachmentPoints(a.get()), 4u);
            for (size_t q = 0; q < replacement.size(); ++q) ExpectQuad(read, a.get(), q, replacement[q]);
            ExpectQuad(read,a.get(),3,appended);
          }
        }));
  }
}

template <typename Getter>
void ExpectPoints(Getter get, const std::vector<FS_POINTF>& expected) {
  const auto count = static_cast<unsigned long>(expected.size());
  EXPECT_EQ(get(nullptr, 0), count);
  for (unsigned long capacity : {0ul, count - 1, count, count + 2}) {
    std::vector<FS_POINTF> buffer(expected.size()+3, {91,92});
    EXPECT_EQ(get(buffer.data(), capacity), count);
    for (size_t i = 0; i < buffer.size(); ++i) {
      const FS_POINTF point = capacity >= count && i < count ? expected[i] : FS_POINTF{91,92};
      EXPECT_FLOAT_EQ(buffer[i].x, point.x); EXPECT_FLOAT_EQ(buffer[i].y, point.y);
    }
  }
}

TEST(Annotations, InkStrokeGeometryBufferCapacitiesRemovalAndRecreation) {
  Artifacts artifacts;
  const std::vector<std::vector<FS_POINTF>> paths = {{{20.25f,32.5f},{52.75f,80.25f},{80.5f,44.75f}},
                                                   {{100.5f,96.25f},{132.25f,120.5f}}};
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,kWidth,kHeight),api.FPDF_ClosePage);
    Handle ink(api.FPDFPage_CreateAnnot(page.get(),FPDF_ANNOT_INK),api.FPDFPage_CloseAnnot);
    Handle square(api.FPDFPage_CreateAnnot(page.get(),FPDF_ANNOT_SQUARE),api.FPDFPage_CloseAnnot);
    const FS_RECTF rect{8,152,184,12};
    ASSERT_TRUE(api.FPDFAnnot_SetRect(ink.get(),&rect));
    EXPECT_EQ(api.FPDFAnnot_GetInkListCount(ink.get()),0ul);
    EXPECT_FALSE(api.FPDFAnnot_HasKey(ink.get(),"InkList"));
    EXPECT_TRUE(api.FPDFAnnot_RemoveInkList(ink.get()));
    EXPECT_FALSE(api.FPDFAnnot_RemoveInkList(square.get()));
    EXPECT_EQ(api.FPDFAnnot_AddInkStroke(square.get(),paths[0].data(),paths[0].size()),-1);
    for (size_t p = 0; p < paths.size(); ++p) {
      EXPECT_EQ(api.FPDFAnnot_AddInkStroke(ink.get(),paths[p].data(),paths[p].size()), static_cast<int>(p));
      EXPECT_EQ(api.FPDFAnnot_GetInkListCount(ink.get()),p+1);
      for (size_t old = 0; old <= p; ++old)
        ExpectPoints([&](FS_POINTF* buffer,unsigned long count) {return api.FPDFAnnot_GetInkListPath(ink.get(),old,buffer,count);},paths[old]);
    }
    EXPECT_EQ(api.FPDFAnnot_GetInkListCount(square.get()),0ul);
    EXPECT_EQ(api.FPDFAnnot_GetInkListPath(ink.get(),paths.size(),nullptr,0),0ul);
    EXPECT_EQ(api.FPDFAnnot_GetInkListPath(square.get(),0,nullptr,0),0ul);
    ASSERT_TRUE(api.FPDFAnnot_RemoveInkList(ink.get()));
    EXPECT_FALSE(api.FPDFAnnot_HasKey(ink.get(),"InkList"));
    EXPECT_EQ(api.FPDFAnnot_GetInkListCount(ink.get()),0ul);
    for (size_t p = 0; p < paths.size(); ++p)
      ASSERT_EQ(api.FPDFAnnot_AddInkStroke(ink.get(),paths[p].data(),paths[p].size()), static_cast<int>(p));
    ASSERT_TRUE(api.FPDFAnnot_SetColor(ink.get(),FPDFANNOT_COLORTYPE_Color,32,96,224,255));
    ASSERT_TRUE(api.FPDFAnnot_SetBorder(ink.get(),0,0,3));
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"ink",
        [&](const Api& read,FPDF_DOCUMENT,FPDF_PAGE loaded) {
          Handle a(read.FPDFPage_GetAnnot(loaded,0),read.FPDFPage_CloseAnnot);
          ASSERT_EQ(read.FPDFAnnot_GetInkListCount(a.get()),paths.size());
          for(size_t p=0;p<paths.size();++p)
            ExpectPoints([&](FS_POINTF* b,unsigned long n){return read.FPDFAnnot_GetInkListPath(a.get(),p,b,n);},paths[p]);
        }));
  }
}

// Polygon/polyline/line annotation constructors are not public. The fixture
// contains only three annotations and an otherwise empty page.
std::vector<uint8_t> GeometryFixture() {
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 200] /Annots [4 0 R 5 0 R 6 0 R] >>",
      "<< /Type /Annot /Subtype /Polygon /Rect [0 0 100 100] /Vertices [-1.25 2.5 40.75 80.5 90.25 10.125] /C [1 0 0] /Border [0 0 2] >>",
      "<< /Type /Annot /Subtype /PolyLine /Rect [100 0 200 100] /Vertices [105.25 2.5 140.75 80.5 190.25 10.125] /C [0 0 1] /Border [0 0 2] >>",
      "<< /Type /Annot /Subtype /Line /Rect [-4 0 120 160] /L [-2.5 4.25 110.5 150.75] /C [0 0.5 0] /Border [0 0 2] >>"});
}

TEST(Annotations, FixtureVerticesAndLineEndpointsUsePointCapacities) {
  Artifacts artifacts;
  const auto bytes=GeometryFixture();
  const std::array<std::vector<FS_POINTF>,2> vertices={
      std::vector<FS_POINTF>{{-1.25f,2.5f},{40.75f,80.5f},{90.25f,10.125f}},
      std::vector<FS_POINTF>{{105.25f,2.5f},{140.75f,80.5f},{190.25f,10.125f}}};
  auto inspect=[&](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page) {
    for(int index=0;index<3;++index) {
      Handle a(api.FPDFPage_GetAnnot(page,index),api.FPDFPage_CloseAnnot);
      ASSERT_NE(a.get(),nullptr);
      EXPECT_EQ(api.FPDFAnnot_GetSubtype(a.get()),index==0?FPDF_ANNOT_POLYGON:index==1?FPDF_ANNOT_POLYLINE:FPDF_ANNOT_LINE);
      if(index<2) {
        ExpectPoints([&](FS_POINTF* b,unsigned long n){return api.FPDFAnnot_GetVertices(a.get(),b,n);},vertices[index]);
        FS_POINTF start{},end{};
        EXPECT_FALSE(api.FPDFAnnot_GetLine(a.get(),&start,&end));
      } else {
        ExpectRect(api,a.get(),FS_RECTF{-4,160,120,0});
        EXPECT_EQ(api.FPDFAnnot_GetVertices(a.get(),nullptr,0),0ul);
        FS_POINTF start{},end{};
        ASSERT_TRUE(api.FPDFAnnot_GetLine(a.get(),&start,&end));
        EXPECT_FLOAT_EQ(start.x,-2.5f);EXPECT_FLOAT_EQ(start.y,4.25f);
        EXPECT_FLOAT_EQ(end.x,110.5f);EXPECT_FLOAT_EQ(end.y,150.75f);
        EXPECT_FALSE(api.FPDFAnnot_GetLine(a.get(),nullptr,&end));
        EXPECT_FALSE(api.FPDFAnnot_GetLine(a.get(),&start,nullptr));
      }
    }
  };
  std::array<Raster,2> scenes;size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);
    Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
    ASSERT_NE(page.get(),nullptr);
    ASSERT_NO_FATAL_FAILURE(inspect(api,document.get(),page.get()));
    scenes[backend++]=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"geometry-fixture",inspect));
  }
  CompareRaster(scenes[0],scenes[1],artifacts,"geometry-backends");
}

std::vector<uint8_t> DictionaryFixture() {
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 200] /Annots [4 0 R] >>",
      "<< /Type /Annot /Subtype /Stamp /Rect [24 24 160 100] /Bool true /Integer 17 /Negative -7 /Fraction 1.25 /Zero 0 /Text (abc) /Empty () /Name /Named /Array [1 2] /Dict << /Inner 1 >> /Null null /Ref 5 0 R /Stream 6 0 R >>",
      "(reference value)", PdfStream("", "stream value")});
}

TEST(Annotations, DictionaryTypesUnicodeStringsNumbersAndBufferUnitsPersist) {
  Artifacts artifacts;const auto bytes=DictionaryFixture();
  for(const auto* library:Implementations()) {
    const auto& api=library->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);
    Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
    Handle annot(api.FPDFPage_GetAnnot(page.get(),0),api.FPDFPage_CloseAnnot);
    const std::array<std::pair<const char*,FPDF_OBJECT_TYPE>,12> types={{{"Bool",FPDF_OBJECT_BOOLEAN},{"Integer",FPDF_OBJECT_NUMBER},
        {"Text",FPDF_OBJECT_STRING},{"Empty",FPDF_OBJECT_STRING},{"Name",FPDF_OBJECT_NAME},{"Array",FPDF_OBJECT_ARRAY},
        {"Dict",FPDF_OBJECT_DICTIONARY},{"Null",FPDF_OBJECT_NULLOBJ},{"Ref",FPDF_OBJECT_REFERENCE},
        {"Stream",FPDF_OBJECT_REFERENCE},{"Missing",FPDF_OBJECT_UNKNOWN},{"Zero",FPDF_OBJECT_NUMBER}}};
    for(const auto& item:types) {
      EXPECT_EQ(!!api.FPDFAnnot_HasKey(annot.get(),item.first),std::string(item.first)!="Missing");
      EXPECT_EQ(api.FPDFAnnot_GetValueType(annot.get(),item.first),item.second);
    }
    for(const auto& item:std::array<std::pair<const char*,float>,4>{{{"Integer",17},{"Negative",-7},{"Fraction",1.25f},{"Zero",0}}}) {
      float value=91;ASSERT_TRUE(api.FPDFAnnot_GetNumberValue(annot.get(),item.first,&value));EXPECT_FLOAT_EQ(value,item.second);
    }
    float value=91;
    for(const auto* key:{"Missing","Text","Name"}) EXPECT_FALSE(api.FPDFAnnot_GetNumberValue(annot.get(),key,&value));
    EXPECT_FALSE(api.FPDFAnnot_GetNumberValue(annot.get(),"Integer",nullptr));
    ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetStringValue(annot.get(),"Name",b,n);},u"Named");
    for(const auto* key:{"Empty","Missing","Integer","Bool"})
      ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetStringValue(annot.get(),key,b,n);},u"");
    const std::vector<std::pair<std::string,std::u16string>> values={{"PortableEmpty",u""},{"PortableText",u"ASCII"},
        {"Unicode",u"Gr\u00fc\u00dfe \u6771\u4eac \U0001f642"},{"Integer",u"replaced number"},{"PortableText",u"replacement"}};
    for(const auto& item:values) {
      ASSERT_TRUE(api.FPDFAnnot_SetStringValue(annot.get(),item.first.c_str(),Wide(item.second)));
      EXPECT_EQ(api.FPDFAnnot_GetValueType(annot.get(),item.first.c_str()),FPDF_OBJECT_STRING);
      ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetStringValue(annot.get(),item.first.c_str(),b,n);},item.second);
    }
    std::array<FPDF_WCHAR,4> sentinel;sentinel.fill(0x5a5a);
    EXPECT_EQ(api.FPDFAnnot_GetStringValue(nullptr,"Text",sentinel.data(),8),0ul);
    EXPECT_TRUE(std::all_of(sentinel.begin(),sentinel.end(),[](FPDF_WCHAR c){return c==0x5a5a;}));
    ASSERT_NO_FATAL_FAILURE(SetAppearance(api,annot.get()));
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"dictionary",
        [&](const Api& read,FPDF_DOCUMENT,FPDF_PAGE loaded) {
          Handle a(read.FPDFPage_GetAnnot(loaded,0),read.FPDFPage_CloseAnnot);
          EXPECT_TRUE(read.FPDFAnnot_HasKey(a.get(),"Unicode"));
          EXPECT_EQ(read.FPDFAnnot_GetValueType(a.get(),"Integer"),FPDF_OBJECT_STRING);
          for(const auto& item:std::vector<std::pair<std::string,std::u16string>>{{"PortableEmpty",u""},{"PortableText",u"replacement"},
              {"Unicode",u"Gr\u00fc\u00dfe \u6771\u4eac \U0001f642"},{"Integer",u"replaced number"}})
            ExpectText([&](FPDF_WCHAR* b,unsigned long n){return read.FPDFAnnot_GetStringValue(a.get(),item.first.c_str(),b,n);},item.second);
          for(const auto& item:std::array<std::pair<const char*,float>,3>{{{"Fraction",1.25f},{"Negative",-7},{"Zero",0}}}) {
            float number=0;ASSERT_TRUE(read.FPDFAnnot_GetNumberValue(a.get(),item.first,&number));EXPECT_FLOAT_EQ(number,item.second);
          }
        }));
  }
}

TEST(Annotations, AppearanceModesReplaceIndependentlyClearAndPersist) {
  Artifacts artifacts;std::array<Raster,2> scenes;size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,kWidth,kHeight),api.FPDF_ClosePage);
    Handle a(api.FPDFPage_CreateAnnot(page.get(),FPDF_ANNOT_STAMP),api.FPDFPage_CloseAnnot);
    const FS_RECTF rect{24,120,168,24};ASSERT_TRUE(api.FPDFAnnot_SetRect(a.get(),&rect));
    const std::array<std::u16string,3> streams={u"1 0 0 rg 24 24 144 96 re f",u"0 1 0 rg 24 24 144 96 re f",u"0 0 1 rg 24 24 144 96 re f"};
    for(int mode=0;mode<3;++mode) {
      ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetAP(a.get(),mode,b,n);},u"");
      ASSERT_TRUE(api.FPDFAnnot_SetAP(a.get(),mode,Wide(streams[mode])));
    }
    ASSERT_TRUE(api.FPDFAnnot_SetAP(a.get(),1,Wide(u"")));
    ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetAP(a.get(),1,b,n);},u"");
    ASSERT_TRUE(api.FPDFAnnot_SetAP(a.get(),1,nullptr));
    ASSERT_TRUE(api.FPDFAnnot_SetAP(a.get(),2,nullptr));
    for(int mode=1;mode<3;++mode)
      ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetAP(a.get(),mode,b,n);},u"");
    ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetAP(a.get(),0,b,n);},streams[0]);
    for(int mode=0;mode<3;++mode) ASSERT_TRUE(api.FPDFAnnot_SetAP(a.get(),mode,Wide(streams[mode])));
    ASSERT_TRUE(api.FPDFAnnot_SetAP(a.get(),0,nullptr));
    EXPECT_FALSE(api.FPDFAnnot_HasKey(a.get(),"AP"));
    for(int mode=0;mode<3;++mode) {
      ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetAP(a.get(),mode,b,n);},u"");
      ASSERT_TRUE(api.FPDFAnnot_SetAP(a.get(),mode,Wide(streams[mode])));
    }
    for(int invalid:{-1,3,99}) {
      EXPECT_FALSE(api.FPDFAnnot_SetAP(a.get(),invalid,Wide(streams[0])));
      std::array<FPDF_WCHAR,4> output{};output.fill(0x5a5a);
      EXPECT_EQ(api.FPDFAnnot_GetAP(a.get(),invalid,output.data(),8),0ul);
      EXPECT_EQ(output[0],0x5a5a);
    }
    scenes[backend++]=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"appearance-modes",
        [&](const Api& read,FPDF_DOCUMENT,FPDF_PAGE loaded) {
          Handle annotation(read.FPDFPage_GetAnnot(loaded,0),read.FPDFPage_CloseAnnot);
          for(int mode=0;mode<3;++mode)
            ExpectText([&](FPDF_WCHAR* b,unsigned long n){return read.FPDFAnnot_GetAP(annotation.get(),mode,b,n);},streams[mode]);
        }));
  }
  CompareRaster(scenes[0],scenes[1],artifacts,"appearance-backends");
}

TEST(Annotations, VisibilityFlagsReplaceAndControlScreenAndPrintRendering) {
  Artifacts artifacts;std::array<std::vector<Raster>,2> scenes;size_t backend=0;
  const std::vector<int> flags={0,FPDF_ANNOT_FLAG_INVISIBLE,FPDF_ANNOT_FLAG_HIDDEN,FPDF_ANNOT_FLAG_PRINT,
      FPDF_ANNOT_FLAG_NOZOOM,FPDF_ANNOT_FLAG_NOROTATE,FPDF_ANNOT_FLAG_NOVIEW,FPDF_ANNOT_FLAG_READONLY,
      FPDF_ANNOT_FLAG_LOCKED,FPDF_ANNOT_FLAG_TOGGLENOVIEW,FPDF_ANNOT_FLAG_PRINT|FPDF_ANNOT_FLAG_NOVIEW,0};
  for(const auto* library:Implementations()) {
    const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,kWidth,kHeight),api.FPDF_ClosePage);
    Handle a(api.FPDFPage_CreateAnnot(page.get(),FPDF_ANNOT_STAMP),api.FPDFPage_CloseAnnot);
    const FS_RECTF rect{24,120,168,24};ASSERT_TRUE(api.FPDFAnnot_SetRect(a.get(),&rect));
    ASSERT_NO_FATAL_FAILURE(SetAppearance(api,a.get()));
    EXPECT_EQ(api.FPDFAnnot_GetFlags(a.get()),FPDF_ANNOT_FLAG_NONE);
    const auto blank=Render(api,page.get(),kWidth,kHeight,0);
    for(int value:flags) {
      SCOPED_TRACE(value);
      ASSERT_TRUE(api.FPDFAnnot_SetFlags(a.get(),value));EXPECT_EQ(api.FPDFAnnot_GetFlags(a.get()),value);
      const auto screen=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);
      const auto print=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT|FPDF_PRINTING);
      if(value & (FPDF_ANNOT_FLAG_HIDDEN|FPDF_ANNOT_FLAG_NOVIEW)) { EXPECT_EQ(screen.pixels,blank.pixels); }
      else { EXPECT_NE(screen.pixels,blank.pixels); }
      if((value & FPDF_ANNOT_FLAG_HIDDEN) || !(value & FPDF_ANNOT_FLAG_PRINT)) { EXPECT_EQ(print.pixels,blank.pixels); }
      else { EXPECT_NE(print.pixels,blank.pixels); }
      scenes[backend].push_back(screen);scenes[backend].push_back(print);
    }
    const int final=FPDF_ANNOT_FLAG_PRINT|FPDF_ANNOT_FLAG_READONLY;
    ASSERT_TRUE(api.FPDFAnnot_SetFlags(a.get(),final));
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"flags",
        [&](const Api& read,FPDF_DOCUMENT,FPDF_PAGE loaded) {
          Handle annotation(read.FPDFPage_GetAnnot(loaded,0),read.FPDFPage_CloseAnnot);
          EXPECT_EQ(read.FPDFAnnot_GetFlags(annotation.get()),final);
        }));
    ++backend;
  }
  ASSERT_EQ(scenes[0].size(),scenes[1].size());
  for(size_t i=0;i<scenes[0].size();++i) CompareRaster(scenes[0][i],scenes[1][i],artifacts,"visibility-"+std::to_string(i));
}

std::vector<uint8_t> LinkedFixture() {
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 200] /Annots [4 0 R 5 0 R 6 0 R] >>",
      "<< /Type /Annot /Subtype /Text /Rect [8 8 24 24] /Contents (source) /IRT 5 0 R /Popup 6 0 R /NotAnnot << /Type /Other >> >>",
      "<< /Type /Annot /Subtype /Text /Rect [40 50 80 90] /Contents (reply) >>",
      "<< /Type /Annot /Subtype /Popup /Rect [100 60 180 140] /Contents (popup) >>"});
}

TEST(Annotations, LinkedHandlesAndAsciiUriActionsSurviveReload) {
  Artifacts artifacts;const auto bytes=LinkedFixture();
  for(const auto* library:Implementations()) {
    const auto& api=library->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);
    Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
    Handle source(api.FPDFPage_GetAnnot(page.get(),0),api.FPDFPage_CloseAnnot);
    for(const auto* key:{"Missing","NotAnnot"}) EXPECT_EQ(api.FPDFAnnot_GetLinkedAnnot(source.get(),key),nullptr);
    for(const auto& item:std::array<std::pair<const char*,int>,2>{{{"IRT",FPDF_ANNOT_TEXT},{"Popup",FPDF_ANNOT_POPUP}}}) {
      Handle linked(api.FPDFAnnot_GetLinkedAnnot(source.get(),item.first),api.FPDFPage_CloseAnnot);
      ASSERT_NE(linked.get(),nullptr);EXPECT_EQ(api.FPDFAnnot_GetSubtype(linked.get()),item.second);
      EXPECT_NE(api.FPDFPage_GetAnnotIndex(page.get(),linked.get()),api.FPDFPage_GetAnnotIndex(page.get(),source.get()));
      const bool popup=std::string(item.first)=="Popup";
      ExpectRect(api,linked.get(),popup?FS_RECTF{100,140,180,60}:FS_RECTF{40,90,80,50});
      ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAnnot_GetStringValue(linked.get(),"Contents",b,n);},popup?u"popup":u"reply");
    }
    Handle link(api.FPDFPage_CreateAnnot(page.get(),FPDF_ANNOT_LINK),api.FPDFPage_CloseAnnot);
    const FS_RECTF rect{20,180,180,150};ASSERT_TRUE(api.FPDFAnnot_SetRect(link.get(),&rect));
    ASSERT_NO_FATAL_FAILURE(SetAppearance(api,link.get()));
    EXPECT_EQ(api.FPDFAnnot_GetLink(nullptr),nullptr);EXPECT_EQ(api.FPDFAnnot_GetLink(source.get()),nullptr);
    EXPECT_FALSE(api.FPDFAnnot_SetURI(source.get(),"https://example.invalid"));
    for(const auto* uri:{"","https://example.invalid/path?q=one&x=%20#fragment","mailto:person@example.invalid"}) {
      ASSERT_TRUE(api.FPDFAnnot_SetURI(link.get(),uri));
      const auto borrowed=api.FPDFAnnot_GetLink(link.get());ASSERT_NE(borrowed,nullptr);
      FS_RECTF actual{};ASSERT_TRUE(api.FPDFLink_GetAnnotRect(borrowed,&actual));
      EXPECT_FLOAT_EQ(actual.left,rect.left);EXPECT_FLOAT_EQ(actual.top,rect.top);
      const auto action=api.FPDFLink_GetAction(borrowed);ASSERT_NE(action,nullptr);
      EXPECT_EQ(api.FPDFAction_GetType(action),PDFACTION_URI);
      const auto length=api.FPDFAction_GetURIPath(document.get(),action,nullptr,0);
      ASSERT_EQ(length,std::string(uri).size()+1);
      std::vector<char> value(length+2,'Z');EXPECT_EQ(api.FPDFAction_GetURIPath(document.get(),action,value.data(),length),length);
      EXPECT_EQ(std::string(value.data()),uri);EXPECT_EQ(value[length],'Z');
    }
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"links",
        [&](const Api& read,FPDF_DOCUMENT doc,FPDF_PAGE loaded) {
          Handle a(read.FPDFPage_GetAnnot(loaded,0),read.FPDFPage_CloseAnnot);
          for(const auto* key:{"IRT","Popup"}) {
            Handle linked(read.FPDFAnnot_GetLinkedAnnot(a.get(),key),read.FPDFPage_CloseAnnot);ASSERT_NE(linked.get(),nullptr);
            const bool popup=std::string(key)=="Popup";
            EXPECT_EQ(read.FPDFAnnot_GetSubtype(linked.get()),popup?FPDF_ANNOT_POPUP:FPDF_ANNOT_TEXT);
            ExpectRect(read,linked.get(),popup?FS_RECTF{100,140,180,60}:FS_RECTF{40,90,80,50});
            ExpectText([&](FPDF_WCHAR* b,unsigned long n){return read.FPDFAnnot_GetStringValue(linked.get(),"Contents",b,n);},popup?u"popup":u"reply");
          }
          Handle l(read.FPDFPage_GetAnnot(loaded,3),read.FPDFPage_CloseAnnot);
          const auto action=read.FPDFLink_GetAction(read.FPDFAnnot_GetLink(l.get()));
          std::array<char,64> value{};ASSERT_GT(read.FPDFAction_GetURIPath(doc,action,value.data(),value.size()),0ul);
          EXPECT_STREQ(value.data(),"mailto:person@example.invalid");
        }));
  }
}

TEST(Annotations, EmbeddedFilesRemainDocumentOwnedAfterAnnotationHandleCloses) {
  Artifacts artifacts;const std::vector<uint8_t> payload={'P','D','F',0,1,127,255};
  for(const auto* library:Implementations()) {
    const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,kWidth,kHeight),api.FPDF_ClosePage);
    Handle unsupported(api.FPDFPage_CreateAnnot(page.get(),FPDF_ANNOT_TEXT),api.FPDFPage_CloseAnnot);
    EXPECT_EQ(api.FPDFAnnot_GetFileAttachment(unsupported.get()),nullptr);
    EXPECT_EQ(api.FPDFAnnot_AddFileAttachment(unsupported.get(),Wide(u"bad")),nullptr);
    {
      Handle empty(api.FPDFPage_CreateAnnot(page.get(),FPDF_ANNOT_FILEATTACHMENT),api.FPDFPage_CloseAnnot);
      EXPECT_EQ(api.FPDFAnnot_AddFileAttachment(empty.get(),Wide(u"")),nullptr);
      EXPECT_EQ(api.FPDFAnnot_GetFileAttachment(empty.get()),nullptr);
    }
    ASSERT_TRUE(api.FPDFPage_RemoveAnnot(page.get(),1));
    const std::array<std::u16string,2> names={u"note.bin",u"\u6771\u4eac\U0001f642.bin"};
    for(const auto& name:names) {
      Handle a(api.FPDFPage_CreateAnnot(page.get(),FPDF_ANNOT_FILEATTACHMENT),api.FPDFPage_CloseAnnot);
      EXPECT_EQ(api.FPDFAnnot_GetFileAttachment(a.get()),nullptr);
      const FS_RECTF rect{16,48,48,16};ASSERT_TRUE(api.FPDFAnnot_SetRect(a.get(),&rect));
      ASSERT_TRUE(api.FPDFAnnot_SetFlags(a.get(),FPDF_ANNOT_FLAG_HIDDEN));
      const auto attachment=api.FPDFAnnot_AddFileAttachment(a.get(),Wide(name));ASSERT_NE(attachment,nullptr);
      ASSERT_TRUE(api.FPDFAttachment_SetFile(attachment,document.get(),payload.data(),payload.size()));
      ASSERT_TRUE(api.FPDFAttachment_SetStringValue(attachment,"Description",Wide(u"portable annotation bytes")));
      EXPECT_NE(api.FPDFAnnot_GetFileAttachment(a.get()),nullptr);
      a.reset();
      ExpectText([&](FPDF_WCHAR* b,unsigned long n){return api.FPDFAttachment_GetName(attachment,b,n);},name);
    }
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"attachment",
        [&](const Api& read,FPDF_DOCUMENT,FPDF_PAGE loaded) {
          for(size_t index=0;index<names.size();++index) {
            Handle a(read.FPDFPage_GetAnnot(loaded,static_cast<int>(index)+1),read.FPDFPage_CloseAnnot);
            const auto attachment=read.FPDFAnnot_GetFileAttachment(a.get());ASSERT_NE(attachment,nullptr);
            ExpectText([&](FPDF_WCHAR* b,unsigned long n){return read.FPDFAttachment_GetName(attachment,b,n);},names[index]);
            ExpectText([&](FPDF_WCHAR* b,unsigned long n){return read.FPDFAttachment_GetStringValue(attachment,"Description",b,n);},u"portable annotation bytes");
            unsigned long size=0;ASSERT_TRUE(read.FPDFAttachment_GetFile(attachment,nullptr,0,&size));ASSERT_EQ(size,payload.size());
            std::vector<uint8_t> actual(size+2,0xa5);ASSERT_TRUE(read.FPDFAttachment_GetFile(attachment,actual.data(),size,&size));
            EXPECT_TRUE(std::equal(payload.begin(),payload.end(),actual.begin()));EXPECT_EQ(actual[payload.size()],0xa5);
          }
        }));
  }
}

void PopulateObjects(const Api& api, FPDF_DOCUMENT document, FPDF_ANNOTATION annot) {
  Handle rectangle(api.FPDFPageObj_CreateNewRect(24,24,32,24),api.FPDFPageObj_Destroy);
  ASSERT_NE(rectangle.get(),nullptr);
  ASSERT_TRUE(api.FPDFPageObj_SetFillColor(rectangle.get(),224,32,64,255));
  ASSERT_TRUE(api.FPDFPath_SetDrawMode(rectangle.get(),FPDF_FILLMODE_WINDING,0));
  ASSERT_TRUE(api.FPDFAnnot_AppendObject(annot,rectangle.get()));rectangle.release();
  Handle text(api.FPDFPageObj_NewTextObj(document,"Helvetica",12),api.FPDFPageObj_Destroy);
  ASSERT_NE(text.get(),nullptr);ASSERT_TRUE(api.FPDFText_SetText(text.get(),Wide(u"API")));
  FS_MATRIX text_matrix{1,0,0,1,72,40};ASSERT_TRUE(api.FPDFPageObj_SetMatrix(text.get(),&text_matrix));
  ASSERT_TRUE(api.FPDFAnnot_AppendObject(annot,text.get()));text.release();
  Handle bitmap(api.FPDFBitmap_Create(4,4,0),api.FPDFBitmap_Destroy);
  ASSERT_NE(bitmap.get(),nullptr);ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,4,4,0xff2060e0));
  Handle image(api.FPDFPageObj_NewImageObj(document),api.FPDFPageObj_Destroy);
  ASSERT_NE(image.get(),nullptr);ASSERT_TRUE(api.FPDFImageObj_SetBitmap(nullptr,0,image.get(),bitmap.get()));
  ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image.get(),24,0,0,24,128,24));
  ASSERT_TRUE(api.FPDFAnnot_AppendObject(annot,image.get()));image.release();
}

TEST(Annotations, InkAndStampObjectsAppendUpdateAndRemoveWithCorrectOwnership) {
  Artifacts artifacts;std::array<Raster,2> scenes;size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,kWidth,kHeight),api.FPDF_ClosePage);
    for(int type:{FPDF_ANNOT_INK,FPDF_ANNOT_STAMP}) {
      Handle annot(api.FPDFPage_CreateAnnot(page.get(),type),api.FPDFPage_CloseAnnot);
      const FS_RECTF rect{16,120,192,16};ASSERT_TRUE(api.FPDFAnnot_SetRect(annot.get(),&rect));
      EXPECT_EQ(api.FPDFAnnot_GetObjectCount(annot.get()),0);
      ASSERT_NO_FATAL_FAILURE(PopulateObjects(api,document.get(),annot.get()));
      ASSERT_EQ(api.FPDFAnnot_GetObjectCount(annot.get()),3);
      EXPECT_EQ(api.FPDFAnnot_GetObject(annot.get(),-1),nullptr);
      EXPECT_EQ(api.FPDFAnnot_GetObject(annot.get(),3),nullptr);
      auto path=api.FPDFAnnot_GetObject(annot.get(),0);
      auto text=api.FPDFAnnot_GetObject(annot.get(),1);
      auto image=api.FPDFAnnot_GetObject(annot.get(),2);
      ASSERT_NE(path,nullptr);ASSERT_NE(text,nullptr);ASSERT_NE(image,nullptr);
      EXPECT_EQ(api.FPDFPageObj_GetType(path),FPDF_PAGEOBJ_PATH);
      EXPECT_EQ(api.FPDFPageObj_GetType(text),FPDF_PAGEOBJ_TEXT);
      EXPECT_EQ(api.FPDFPageObj_GetType(image),FPDF_PAGEOBJ_IMAGE);
      ASSERT_TRUE(api.FPDFPageObj_SetFillColor(path,32,180,64,255));
      ASSERT_TRUE(api.FPDFAnnot_UpdateObject(annot.get(),path));
      ASSERT_TRUE(api.FPDFText_SetText(text,Wide(u"edit")));
      ASSERT_TRUE(api.FPDFTextObj_SetFontSize(text,14));
      ASSERT_TRUE(api.FPDFAnnot_UpdateObject(annot.get(),text));
      ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image,32,0,0,24,144,24));
      ASSERT_TRUE(api.FPDFAnnot_UpdateObject(annot.get(),image));
    }
    auto inspect=[](const Api& read,FPDF_DOCUMENT,FPDF_PAGE loaded) {
      for(int index=0;index<2;++index) {
        Handle a(read.FPDFPage_GetAnnot(loaded,index),read.FPDFPage_CloseAnnot);
        ASSERT_EQ(read.FPDFAnnot_GetObjectCount(a.get()),3);
        const auto path=read.FPDFAnnot_GetObject(a.get(),0);
        unsigned int r=0,g=0,b=0,alpha=0;
        ASSERT_TRUE(read.FPDFPageObj_GetFillColor(path,&r,&g,&b,&alpha));
        EXPECT_EQ((std::array<unsigned int,4>{r,g,b,alpha}),(std::array<unsigned int,4>{32,180,64,255}));
        const auto text=read.FPDFAnnot_GetObject(a.get(),1);float size=0;
        ASSERT_TRUE(read.FPDFTextObj_GetFontSize(text,&size));EXPECT_FLOAT_EQ(size,14);
        FS_MATRIX matrix{};ASSERT_TRUE(read.FPDFPageObj_GetMatrix(read.FPDFAnnot_GetObject(a.get(),2),&matrix));
        EXPECT_FLOAT_EQ(matrix.a,32);EXPECT_FLOAT_EQ(matrix.e,144);
      }
    };
    // FreeType and the Rust font rasterizer differ only on antialiased glyph
    // edges here: measured max channel delta 14, mean 0.00536, confined to
    // x72..94/y149..160. Keep shape/color changes exact outside that tiny budget.
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"object-updates",inspect,
                                      RasterTolerance{16,0,0.01}));
    for(int index=0;index<2;++index) {
      Handle a(api.FPDFPage_GetAnnot(page.get(),index),api.FPDFPage_CloseAnnot);
      ASSERT_EQ(api.FPDFAnnot_GetObjectCount(a.get()),3);
      EXPECT_FALSE(api.FPDFAnnot_RemoveObject(a.get(),-1));EXPECT_FALSE(api.FPDFAnnot_RemoveObject(a.get(),3));
      // Each removal invalidates the removed borrowed object; reacquire only
      // surviving objects through the updated annotation list.
      ASSERT_TRUE(api.FPDFAnnot_RemoveObject(a.get(),1));EXPECT_EQ(api.FPDFAnnot_GetObjectCount(a.get()),2);
      ASSERT_TRUE(api.FPDFAnnot_RemoveObject(a.get(),0));EXPECT_EQ(api.FPDFAnnot_GetObjectCount(a.get()),1);
      ASSERT_TRUE(api.FPDFAnnot_RemoveObject(a.get(),0));EXPECT_EQ(api.FPDFAnnot_GetObjectCount(a.get()),0);
    }
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"object-removal",
        [](const Api& read,FPDF_DOCUMENT,FPDF_PAGE loaded) {
          for(int index=0;index<2;++index) {Handle a(read.FPDFPage_GetAnnot(loaded,index),read.FPDFPage_CloseAnnot);EXPECT_EQ(read.FPDFAnnot_GetObjectCount(a.get()),0);}
        }));
    Handle unsupported(api.FPDFPage_CreateAnnot(page.get(),FPDF_ANNOT_TEXT),api.FPDFPage_CloseAnnot);
    const FS_RECTF rect{16,120,192,16};ASSERT_TRUE(api.FPDFAnnot_SetRect(unsupported.get(),&rect));
    ASSERT_NO_FATAL_FAILURE(SetAppearance(api,unsupported.get()));
    auto borrowed=api.FPDFAnnot_GetObject(unsupported.get(),0);ASSERT_NE(borrowed,nullptr);
    EXPECT_FALSE(api.FPDFAnnot_UpdateObject(unsupported.get(),borrowed));
    EXPECT_FALSE(api.FPDFAnnot_RemoveObject(unsupported.get(),0));
    Handle owned(api.FPDFPageObj_CreateNewRect(8,8,16,16),api.FPDFPageObj_Destroy);
    EXPECT_FALSE(api.FPDFAnnot_AppendObject(unsupported.get(),owned.get()));
    scenes[backend++]=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);
  }
  CompareRaster(scenes[0],scenes[1],artifacts,"annotation-object-backends");
}

// A single appearance stream with all public object categories. Forms and
// shading have no annotation-specific public constructors.
std::vector<uint8_t> MixedObjectFixture() {
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 200] /Annots [4 0 R] >>",
      "<< /Type /Annot /Subtype /Stamp /Rect [0 0 240 200] /AP << /N 5 0 R >> >>",
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 240 200] /Resources << /Font << /F1 6 0 R >> /XObject << /Im1 7 0 R /Fm1 8 0 R >> /Shading << /Sh1 9 0 R >> >>",
          "1 0 0 rg 10 10 20 20 re f BT /F1 12 Tf 40 20 Td (API) Tj ET q 20 0 0 20 90 10 cm /Im1 Do Q q 1 0 0 1 120 10 cm /Fm1 Do Q q 150 10 50 30 re W n /Sh1 sh Q"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
      PdfStream("/Type /XObject /Subtype /Image /Width 2 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /ASCIIHexDecode","FF00000000FF>"),
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 20 20]","0 1 0 rg 0 0 20 20 re f"),
      "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [150 10 200 10] /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> /Extend [true true] >>"});
}

TEST(Annotations, AppearanceEnumerationIncludesPathTextImageFormAndShading) {
  Artifacts artifacts;const auto bytes=MixedObjectFixture();
  std::array<Raster,2> scenes;size_t backend=0;
  auto inspect=[](const Api& api,FPDF_DOCUMENT,FPDF_PAGE page) {
    Handle annot(api.FPDFPage_GetAnnot(page,0),api.FPDFPage_CloseAnnot);
    ASSERT_EQ(api.FPDFAnnot_GetObjectCount(annot.get()),5);
    const std::array<int,5> types={FPDF_PAGEOBJ_PATH,FPDF_PAGEOBJ_TEXT,FPDF_PAGEOBJ_IMAGE,FPDF_PAGEOBJ_FORM,FPDF_PAGEOBJ_SHADING};
    for(int index=0;index<5;++index) {
      const auto object=api.FPDFAnnot_GetObject(annot.get(),index);ASSERT_NE(object,nullptr);
      EXPECT_EQ(api.FPDFPageObj_GetType(object),types[index]);
      if(types[index]==FPDF_PAGEOBJ_FORM) { EXPECT_EQ(api.FPDFFormObj_CountObjects(object),1); }
      if(types[index]==FPDF_PAGEOBJ_IMAGE) {unsigned int w=0,h=0;ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(object,&w,&h));EXPECT_EQ(w,2u);EXPECT_EQ(h,1u);}
    }
  };
  for(const auto* library:Implementations()) {
    const auto& api=library->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);
    Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
    ASSERT_NE(page.get(),nullptr);
    ASSERT_NO_FATAL_FAILURE(inspect(api,document.get(),page.get()));
    scenes[backend++]=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);
    // Helvetica edge antialiasing differs by at most 7/255 on 57 pixels,
    // confined to the text at x41..58/y171..179 (mean 0.00125).
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"mixed-object",inspect,
                                      RasterTolerance{8,0,0.003}));
  }
  CompareRaster(scenes[0],scenes[1],artifacts,"mixed-object-backends",RasterTolerance{8,0,0.003});
}

// Direct destinations and embedded file specifications have no public constructors.
TEST(Annotations, FixtureDestinationAndEmbeddedFileSurviveEditingAndBothReaders) {
  const std::string payload("PDF\0\1\177\377",7);
  const auto bytes=PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 200] /Annots [4 0 R 5 0 R] >>",
      "<< /Type /Annot /Subtype /Link /Rect [20 100 180 160] /A << /S /GoTo /D [3 0 R /XYZ 40 150 1.25] >> /AP << /N 8 0 R >> >>",
      "<< /Type /Annot /Subtype /FileAttachment /Rect [20 20 40 40] /F 2 /FS 6 0 R >>",
      "<< /Type /Filespec /F (source.bin) /UF (source.bin) /EF << /F 7 0 R >> >>",
      PdfStream("/Type /EmbeddedFile /Params << /Size 7 /CreationDate (D:20200102030405Z) >>",payload),
      PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 160 60] /Resources << >>","0.2 0.6 0.8 rg 0 0 160 60 re f")});
  Artifacts artifacts;std::array<Raster,2> scenes;size_t backend=0;
  for(const auto* library:Implementations()) {
    const auto& api=library->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);
    Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
    const auto inspect=[&](const Api& read,FPDF_DOCUMENT doc,FPDF_PAGE loaded) {
      Handle link(read.FPDFPage_GetAnnot(loaded,0),read.FPDFPage_CloseAnnot);
      const auto borrowed=read.FPDFAnnot_GetLink(link.get());ASSERT_NE(borrowed,nullptr);
      const auto original_action=read.FPDFLink_GetAction(borrowed);ASSERT_NE(original_action,nullptr);
      EXPECT_EQ(read.FPDFAction_GetType(original_action),PDFACTION_GOTO);
      const auto destination=read.FPDFLink_GetDest(doc,borrowed);ASSERT_NE(destination,nullptr);
      EXPECT_EQ(read.FPDFDest_GetDestPageIndex(doc,destination),0);
      unsigned long count=0;std::array<float,4> params{};
      EXPECT_EQ(read.FPDFDest_GetView(destination,&count,params.data()),PDFDEST_VIEW_XYZ);
      ASSERT_EQ(count,3ul);EXPECT_FLOAT_EQ(params[0],40);EXPECT_FLOAT_EQ(params[1],150);EXPECT_FLOAT_EQ(params[2],1.25);
      Handle file(read.FPDFPage_GetAnnot(loaded,1),read.FPDFPage_CloseAnnot);
      const auto attachment=read.FPDFAnnot_GetFileAttachment(file.get());ASSERT_NE(attachment,nullptr);
      ExpectText([&](FPDF_WCHAR* b,unsigned long n){return read.FPDFAttachment_GetName(attachment,b,n);},u"source.bin");
      ExpectText([&](FPDF_WCHAR* b,unsigned long n){return read.FPDFAttachment_GetStringValue(attachment,"CreationDate",b,n);},u"D:20200102030405Z");
      EXPECT_EQ(read.FPDFAttachment_GetValueType(attachment,"Size"),FPDF_OBJECT_NUMBER);
      unsigned long length=0;ASSERT_TRUE(read.FPDFAttachment_GetFile(attachment,nullptr,0,&length));ASSERT_EQ(length,payload.size());
      std::vector<char> contents(length);ASSERT_TRUE(read.FPDFAttachment_GetFile(attachment,contents.data(),length,&length));
      EXPECT_EQ(std::string(contents.begin(),contents.end()),payload);
    };
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"fixture-link-file",inspect));
    // Replace an existing GoTo action (rather than merely replacing another URI).
    Handle link(api.FPDFPage_GetAnnot(page.get(),0),api.FPDFPage_CloseAnnot);
    ASSERT_TRUE(api.FPDFAnnot_SetURI(link.get(),"https://example.invalid/replaced"));
    scenes[backend++]=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"destination-to-uri",
        [&](const Api& read,FPDF_DOCUMENT doc,FPDF_PAGE loaded) {
          Handle changed(read.FPDFPage_GetAnnot(loaded,0),read.FPDFPage_CloseAnnot);
          const auto borrowed=read.FPDFAnnot_GetLink(changed.get());ASSERT_NE(borrowed,nullptr);
          EXPECT_EQ(read.FPDFLink_GetDest(doc,borrowed),nullptr);
          const auto action=read.FPDFLink_GetAction(borrowed);ASSERT_NE(action,nullptr);
          EXPECT_EQ(read.FPDFAction_GetType(action),PDFACTION_URI);
          std::array<char,64> uri{};ASSERT_GT(read.FPDFAction_GetURIPath(doc,action,uri.data(),uri.size()),0ul);
          EXPECT_STREQ(uri.data(),"https://example.invalid/replaced");
        }));
  }
  CompareRaster(scenes[0],scenes[1],artifacts,"fixture-link-file-backends");
}

TEST(Annotations, QuadAppearanceBoundsGrowWithoutShrinkingOnSmallerReplacement) {
  Artifacts artifacts;std::array<std::array<Raster,3>,2> scenes;size_t backend=0;
  const std::array<FS_QUADPOINTSF,3> initial={{{20,120,60,120,20,100,60,100},
      {70,120,110,120,70,100,110,100},{120,120,160,120,120,100,160,100}}};
  const FS_QUADPOINTSF larger{12,160,68,160,12,132,68,132};
  const FS_QUADPOINTSF appended{4,184,204,184,4,88,204,88};
  const std::u16string appearance=u"1 0 0 rg 20 100 40 20 re f";
  const auto pixel=[](const Raster& raster,int x,int y) {
    const auto offset=(static_cast<size_t>(y)*raster.width+x)*4;
    return std::array<uint8_t,4>{raster.pixels[offset],raster.pixels[offset+1],raster.pixels[offset+2],raster.pixels[offset+3]};
  };
  const std::array<uint8_t,4> red{0,0,255,255},white{255,255,255,255};
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(document.get(),0,kWidth,kHeight),api.FPDF_ClosePage);
    Handle annotation(api.FPDFPage_CreateAnnot(page.get(),FPDF_ANNOT_LINK),api.FPDFPage_CloseAnnot);
    ASSERT_NE(annotation.get(),nullptr);
    const FS_RECTF rect{20,120,160,100};ASSERT_TRUE(api.FPDFAnnot_SetRect(annotation.get(),&rect));
    for(const auto& quad:initial) ASSERT_TRUE(api.FPDFAnnot_AppendAttachmentPoints(annotation.get(),&quad));
    ASSERT_NO_FATAL_FAILURE(SetAppearance(api,annotation.get(),appearance));
    scenes[backend][0]=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);
    EXPECT_EQ(pixel(scenes[backend][0],40,85),red);
    // The AP has a localized red path, so growing its BBox changes the mapping
    // into the fixed annotation rectangle and is observable through rendering.
    ASSERT_TRUE(api.FPDFAnnot_SetAttachmentPoints(annotation.get(),0,&larger));
    scenes[backend][1]=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);
    EXPECT_EQ(pixel(scenes[backend][1],40,85),white);
    EXPECT_EQ(pixel(scenes[backend][1],40,99),red);
    EXPECT_NE(scenes[backend][0].pixels,scenes[backend][1].pixels);
    ASSERT_TRUE(api.FPDFAnnot_SetAttachmentPoints(annotation.get(),0,&initial[0]));
    ExpectQuad(api,annotation.get(),0,initial[0]);
    const auto smaller=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);
    CompareRaster(scenes[backend][1],smaller,artifacts,library->name()+"-bbox-does-not-shrink");
    ASSERT_TRUE(api.FPDFAnnot_AppendAttachmentPoints(annotation.get(),&appended));
    scenes[backend][2]=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);
    EXPECT_EQ(pixel(scenes[backend][2],40,95),red);
    EXPECT_EQ(pixel(scenes[backend][2],40,99),white);
    EXPECT_NE(smaller.pixels,scenes[backend][2].pixels);
    ASSERT_NO_FATAL_FAILURE(RoundTrip(*library,document.get(),page.get(),artifacts,"quad-bbox",
        [&](const Api& read,FPDF_DOCUMENT,FPDF_PAGE loaded) {
          Handle a(read.FPDFPage_GetAnnot(loaded,0),read.FPDFPage_CloseAnnot);
          EXPECT_EQ(read.FPDFAnnot_CountAttachmentPoints(a.get()),4u);
          for(size_t index=0;index<initial.size();++index) ExpectQuad(read,a.get(),index,initial[index]);
          ExpectQuad(read,a.get(),3,appended);
          ExpectText([&](FPDF_WCHAR* b,unsigned long n){return read.FPDFAnnot_GetAP(a.get(),0,b,n);},appearance);
        }));
    ++backend;
  }
  for(size_t stage=0;stage<3;++stage)
    CompareRaster(scenes[0][stage],scenes[1][stage],artifacts,"quad-bbox-backends-"+std::to_string(stage));
}

}  // namespace
}  // namespace pdfoxium::integration
