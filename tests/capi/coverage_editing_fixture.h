#pragma once

#include "editing_fixture.h"
#include "form_fixture.h"

#include <algorithm>
#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration::coverage_editing {

struct PageSpec { int width = 128, height = 128, flags = 0; };
using Inspector = std::function<void(const Api&, FPDF_DOCUMENT, FPDF_PAGE, int)>;
using Comparator = std::function<void(const Raster&, const Raster&, Artifacts&, const std::string&)>;

inline void Exact(const Raster& a, const Raster& b, Artifacts& artifacts,
                  const std::string& label) {
  CompareRaster(a, b, artifacts, label, 1);
}

// Fontations/FreeType edge differences in auto-sized standard-font glyphs
// reach24/255 (five comb glyph pixels); mean remains below0.18/255. They are
// restricted to supplied text interiors. Borders and surrounding paint remain
// strict, independently of the size of the page's white background.
inline void TextRegions(const Raster& a, const Raster& b, Artifacts& artifacts,
                         const std::string& label,
                         const std::vector<std::array<int, 4>>& regions) {
  Raster outside_a = a, outside_b = b;
  for (size_t i = 0; i < regions.size(); ++i) {
    const auto& r = regions[i];
    CompareRaster(CropRaster(a, r[0], r[1], r[2], r[3]),
                  CropRaster(b, r[0], r[1], r[2], r[3]), artifacts,
                  label + "-text" + std::to_string(i), RasterTolerance{24, 0, 0.4});
    for (int y = r[1]; y < r[1] + r[3]; ++y)
      for (int x = r[0]; x < r[0] + r[2]; ++x)
        for (int c = 0; c < 4; ++c) {
          outside_a.pixels[(y * a.width + x) * 4 + c] = 255;
          outside_b.pixels[(y * b.width + x) * 4 + c] = 255;
        }
  }
  CompareRaster(outside_a, outside_b, artifacts, label + "-outside-text", 1);
}

inline std::array<uint8_t, 4> Pixel(const Raster& image, int x, int y) {
  const size_t p = (static_cast<size_t>(y) * image.width + x) * 4;
  return {image.pixels.at(p), image.pixels.at(p + 1), image.pixels.at(p + 2), image.pixels.at(p + 3)};
}

inline void Ink(const Raster& image, int x, int y, int width, int height) {
  size_t ink = 0;
  for (int py = y; py < y + height; ++py)
    for (int px = x; px < x + width; ++px) {
      const auto p = Pixel(image, px, py);
      ink += p[0] < 160 && p[1] < 160 && p[2] < 160;
    }
  EXPECT_GT(ink, 3u) << "the appearance must paint its content";
}

inline std::u16string Appearance(const Api& api, FPDF_ANNOTATION annot) {
  const auto size = api.FPDFAnnot_GetAP(annot, 0, nullptr, 0);
  EXPECT_EQ(size % 2, 0u);
  if (!size) return {};
  std::vector<unsigned short> text(size / 2 + 1, 0);
  EXPECT_EQ(api.FPDFAnnot_GetAP(annot, 0, text.data(), size), size);
  return {text.begin(), text.begin() + size / 2 - 1};
}

inline std::vector<Raster> Observe(const Api& api, FPDF_DOCUMENT document,
                                  const std::vector<PageSpec>& specs,
                                  const Inspector& inspect) {
  EXPECT_EQ(api.FPDF_GetPageCount(document), static_cast<int>(specs.size()));
  std::vector<Raster> pictures;
  for (size_t i = 0; i < specs.size(); ++i) {
    Handle page(api.FPDF_LoadPage(document, static_cast<int>(i)), api.FPDF_ClosePage);
    EXPECT_NE(page.get(), nullptr);
    if (!page.get()) return {};
    inspect(api, document, page.get(), static_cast<int>(i));
    pictures.push_back(Render(api, page.get(), specs[i].width, specs[i].height, specs[i].flags));
  }
  return pictures;
}

inline std::vector<Raster> RoundTrip(const Library& writer, FPDF_DOCUMENT document,
                                    const std::vector<PageSpec>& specs,
                                    Artifacts& artifacts, const std::string& label,
                                    const Inspector& inspect,
                                    const Comparator& compare = Exact) {
  const auto before = Observe(writer.api(), document, specs, inspect);
  const auto bytes = SaveDocument(writer.api(), document, artifacts, writer.name() + "-" + label);
  EXPECT_FALSE(bytes.empty());
  for (const auto* reader : Implementations()) {
    SCOPED_TRACE(writer.name() + "-" + label + "-to-" + reader->name());
    const auto& api = reader->api();
    Handle loaded(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    EXPECT_NE(loaded.get(), nullptr);
    if (!loaded.get()) continue;
    const auto after = Observe(api, loaded.get(), specs, inspect);
    EXPECT_EQ(after.size(), before.size());
    for (size_t i = 0; i < std::min(before.size(), after.size()); ++i)
      compare(before[i], after[i], artifacts,
              writer.name() + "-" + label + "-to-" + reader->name() + "-page" + std::to_string(i));
  }
  return before;
}

}  // namespace pdfoxium::integration::coverage_editing
