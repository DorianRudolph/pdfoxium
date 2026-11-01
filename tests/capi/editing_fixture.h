#pragma once

#include "test_support.h"

#include <gtest/gtest.h>

#include <array>
#include <algorithm>
#include <cmath>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration::editing {

inline std::vector<uint8_t> SceneFixture(const std::string& contents,
                                        const std::string& resources = "",
                                        std::vector<std::string> extra = {}) {
  std::vector<std::string> objects = {
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << " +
          resources + " >> /Contents 4 0 R >>",
      PdfStream("", contents)};
  objects.insert(objects.end(), extra.begin(), extra.end());
  return PdfFixture(objects);
}

inline FPDF_PAGEOBJECT Rectangle(const Api& api, float x, float y, float width,
                                 float height, unsigned int r, unsigned int g,
                                 unsigned int b, unsigned int alpha = 255) {
  auto object = api.FPDFPageObj_CreateNewRect(x, y, width, height);
  EXPECT_NE(object, nullptr);
  if (!object) return nullptr;
  EXPECT_TRUE(api.FPDFPageObj_SetFillColor(object, r, g, b, alpha));
  EXPECT_TRUE(api.FPDFPath_SetDrawMode(object, FPDF_FILLMODE_WINDING, 0));
  return object;
}

inline std::array<float, 4> Bounds(const Api& api, FPDF_PAGEOBJECT object) {
  std::array<float, 4> result{};
  EXPECT_TRUE(api.FPDFPageObj_GetBounds(object, &result[0], &result[1],
                                       &result[2], &result[3]));
  return result;
}

inline void ExpectBounds(const Api& api, FPDF_PAGEOBJECT object,
                          const std::array<float, 4>& expected,
                          float tolerance = 0.002f) {
  const auto actual = Bounds(api, object);
  for (size_t i = 0; i < actual.size(); ++i) {
    EXPECT_NEAR(actual[i], expected[i], tolerance) << "bound " << i;
  }
}

// Font rasterizers differ slightly at glyph edges. Restrict that allowance
// to bounds obtained from actual text objects; all other pixels stay strict.
using TextRegions = std::vector<std::array<int, 4>>;
inline TextRegions TextBounds(const Api& api, FPDF_PAGE page) {
  TextRegions result;
  std::function<void(FPDF_PAGEOBJECT, FS_MATRIX)> visit = [&](FPDF_PAGEOBJECT object, FS_MATRIX parent) {
    if (api.FPDFPageObj_GetType(object) == FPDF_PAGEOBJ_TEXT) {
      const auto b = Bounds(api, object);
      float left = 128, bottom = 128, right = 0, top = 0;
      for (float x : {b[0], b[2]}) for (float y : {b[1], b[3]}) {
        const float px = parent.a*x + parent.c*y + parent.e;
        const float py = parent.b*x + parent.d*y + parent.f;
        left=std::min(left,px); right=std::max(right,px);
        bottom=std::min(bottom,py); top=std::max(top,py);
      }
      result.push_back({std::max(0, int(std::floor(left))-2), std::max(0,int(std::floor(128-top))-2),
                        std::min(128,int(std::ceil(right))+2), std::min(128,int(std::ceil(128-bottom))+2)});
    } else if (api.FPDFPageObj_GetType(object) == FPDF_PAGEOBJ_FORM) {
      FS_MATRIX m{}; EXPECT_TRUE(api.FPDFPageObj_GetMatrix(object, &m));
      FS_MATRIX combined{parent.a*m.a+parent.c*m.b, parent.b*m.a+parent.d*m.b,
                         parent.a*m.c+parent.c*m.d, parent.b*m.c+parent.d*m.d,
                         parent.a*m.e+parent.c*m.f+parent.e, parent.b*m.e+parent.d*m.f+parent.f};
      for (int i=0; i<api.FPDFFormObj_CountObjects(object); ++i)
        visit(api.FPDFFormObj_GetObject(object,i), combined);
    }
  };
  for (int i=0; i<api.FPDFPage_CountObjects(page); ++i)
    visit(api.FPDFPage_GetObject(page,i), FS_MATRIX{1,0,0,1,0,0});
  return result;
}
inline void CompareScene(const Raster& a, const Raster& b, Artifacts& artifacts,
                         const std::string& label, const TextRegions& regions,
                         int tolerance = 0) {
  if (!raster_comparison_settings().enabled) return;
  ASSERT_EQ(a.width,b.width); ASSERT_EQ(a.height,b.height);
  Raster outside_a=a, outside_b=b;
  for (size_t i=0; i<regions.size(); ++i) {
    const auto& r=regions[i];
    if (r[2]<=r[0] || r[3]<=r[1]) continue;
    CompareRaster(CropRaster(a,r[0],r[1],r[2]-r[0],r[3]-r[1]),
                  CropRaster(b,r[0],r[1],r[2]-r[0],r[3]-r[1]), artifacts,
                  label+"-glyph-region"+std::to_string(i), RasterTolerance{24,0,1.0});
    for (int y=r[1]; y<r[3]; ++y) for (int x=r[0]; x<r[2]; ++x)
      for (int channel=0; channel<4; ++channel) {
        outside_a.pixels[(y*a.width+x)*4+channel]=255;
        outside_b.pixels[(y*b.width+x)*4+channel]=255;
      }
  }
  CompareRaster(outside_a,outside_b,artifacts,label+"-outside-glyphs",tolerance);
}

inline void RoundTrip(const Library& writer, FPDF_DOCUMENT document,
                       FPDF_PAGE page, Artifacts& artifacts,
                       const std::string& label,
                       const std::function<void(const Api&, FPDF_PAGE)>& inspect,
                       int tolerance = 0) {
  const auto& api = writer.api();
  ASSERT_TRUE(api.FPDFPage_GenerateContent(page));
  ASSERT_NO_FATAL_FAILURE(inspect(api, page));
  const auto before = Render(api, page, 128, 128);
  const auto text_regions = TextBounds(api,page);
  artifacts.Image(label + "-before", before);
  const auto bytes = SaveDocument(api, document, artifacts, label);
  ASSERT_FALSE(bytes.empty());
  std::array<Raster, 2> observations;
  size_t i = 0;
  for (const auto* reader : Implementations()) {
    SCOPED_TRACE(label + "-to-" + reader->name());
    const auto& read = reader->api();
    Handle loaded(read.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr),
                  read.FPDF_CloseDocument);
    ASSERT_NE(loaded.get(), nullptr);
    Handle loaded_page(read.FPDF_LoadPage(loaded.get(), 0), read.FPDF_ClosePage);
    ASSERT_NE(loaded_page.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(inspect(read, loaded_page.get()));
    observations[i] = Render(read, loaded_page.get(), 128, 128);
    CompareScene(before, observations[i], artifacts,
                  label + "-to-" + reader->name(), text_regions, tolerance);
    ++i;
  }
  CompareScene(observations[0], observations[1], artifacts,
                label + "-readers", text_regions, tolerance);
}

// No public constructor creates a shading or a nested form's content stream.
// This minimal fixture supplies every object category and two levels of forms;
// all subsequent queries, edits, saves and rendering use only the public API.
inline std::vector<uint8_t> AllObjectTypesFixture(bool include_shading = true) {
  const auto image = PdfStream(
      "/Type /XObject /Subtype /Image /Width 2 /Height 1 /ColorSpace /DeviceRGB "
      "/BitsPerComponent 8", std::string("\xff\0\0\0\xff\0", 6));
  return SceneFixture(
      std::string(include_shading ? "/Sh1 sh " : "") + "1 0 0 rg 8 8 24 24 re f "
      "BT /F1 12 Tf 1 0 0 1 16 104 Tm (AB) Tj ET "
      "q 24 0 0 16 64 8 cm /Im1 Do Q "
      "q 1 0 0 1 64 64 cm /Fm1 Do Q",
      "/Font << /F1 5 0 R >> /XObject << /Im1 6 0 R /Fm1 7 0 R >> "
      "/Shading << /Sh1 8 0 R >>",
      {"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>", image,
       PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 48 48] "
                 "/Resources << /Font << /F1 5 0 R >> /XObject << /Im1 6 0 R /Fm2 9 0 R >> >>",
                 "0 0 1 rg 0 0 12 12 re f "
                 "BT /F1 10 Tf 1 0 0 1 0 20 Tm (CD) Tj ET "
                 "q 8 0 0 8 20 0 cm /Im1 Do Q /Fm2 Do"),
       "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 128 0] "
       "/Function << /FunctionType 2 /Domain [0 1] /C0 [1 1 1] /C1 [0.8 0.8 1] /N 1 >> "
       "/Extend [true true] >>",
       PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 48 48] "
                 "/Resources << /XObject << /Empty 10 0 R >> >>",
                 "0 1 0 rg 28 28 10 10 re f /Empty Do"),
       PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << >>", "")});
}

}  // namespace pdfoxium::integration::editing
