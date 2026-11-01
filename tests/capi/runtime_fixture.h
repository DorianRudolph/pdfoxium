#pragma once

#include "test_support.h"

#include <algorithm>
#include <fstream>
#include <iterator>
#include <stdexcept>

#include <gtest/gtest.h>

namespace pdfoxium::integration::runtime_test {

inline std::vector<uint8_t> Resource(const std::string& relative) {
  const auto path = TestRoot() / "upstream/pdfium/testing/resources" / relative;
  std::ifstream stream(path, std::ios::binary);
  if (!stream) throw std::runtime_error("Missing fixture: " + path.u8string());
  return {std::istreambuf_iterator<char>(stream), std::istreambuf_iterator<char>()};
}

// The rectangle has no font dependencies and its interior and background have
// independently known colors. Optional catalog/page entries model structures
// for which the public API supplies no constructor.
inline std::vector<uint8_t> RectangleFixture(const std::string& catalog = "",
                                            const std::string& page = "",
                                            const std::vector<std::string>& extra = {}) {
  std::vector<std::string> objects = {
      "<< /Type /Catalog /Pages 2 0 R " + catalog + " >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources <<>> /Contents 4 0 R " + page + " >>",
      PdfStream("", "0.25 0.5 0.75 rg 16 16 64 48 re f\n")};
  objects.insert(objects.end(), extra.begin(), extra.end());
  return PdfFixture(objects);
}

inline Raster Bitmap(const Api& api, FPDF_BITMAP bitmap) {
  Raster result;
  result.width = api.FPDFBitmap_GetWidth(bitmap);
  result.height = api.FPDFBitmap_GetHeight(bitmap);
  const int stride = api.FPDFBitmap_GetStride(bitmap);
  const auto* bytes = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap));
  if (!bytes || result.width <= 0 || result.height <= 0 || stride < result.width * 4) {
    ADD_FAILURE() << "Expected a live four-byte bitmap";
    return {};
  }
  for (int y = 0; y < result.height; ++y) {
    result.pixels.insert(result.pixels.end(), bytes + y * stride,
                         bytes + y * stride + result.width * 4);
  }
  return result;
}

inline void ExpectRectangle(const Raster& raster) {
  ASSERT_EQ(raster.width, 128);
  ASSERT_EQ(raster.height, 96);
  ASSERT_EQ(raster.pixels.size(), 128u * 96 * 4);
  const size_t background = (4 * 128 + 4) * 4;
  const size_t inside = (48 * 128 + 48) * 4;
  EXPECT_EQ(raster.pixels[background], 255);
  EXPECT_EQ(raster.pixels[background + 1], 255);
  EXPECT_EQ(raster.pixels[background + 2], 255);
  // PDF normalized floats may round by one unit when converted to 8-bit RGB.
  EXPECT_NEAR(raster.pixels[inside], 191, 1);
  EXPECT_NEAR(raster.pixels[inside + 1], 128, 1);
  EXPECT_NEAR(raster.pixels[inside + 2], 64, 1);
}

inline void RenderDocument(const Api& api, FPDF_DOCUMENT document, Raster* raster) {
  ASSERT_GT(api.FPDF_GetPageCount(document), 0);
  Handle page(api.FPDF_LoadPage(document, 0), api.FPDF_ClosePage);
  ASSERT_NE(page.get(), nullptr);
  *raster = Render(api, page.get(), 128, 96);
}

}  // namespace pdfoxium::integration::runtime_test
