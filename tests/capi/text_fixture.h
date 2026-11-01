#pragma once

#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <fstream>
#include <functional>
#include <iterator>
#include <string>
#include <vector>

namespace pdfoxium::integration::text_fixture {

inline std::vector<unsigned short> Wide(const std::u16string& text) {
  std::vector<unsigned short> result(text.begin(), text.end());
  result.push_back(0);
  return result;
}

inline std::vector<uint8_t> Read(const std::filesystem::path& path) {
  std::ifstream stream(path, std::ios::binary | std::ios::ate);
  if (!stream) { ADD_FAILURE() << "Cannot open fixture: " << path.u8string(); return {}; }
  const auto size = stream.tellg();
  if (size < 0) { ADD_FAILURE() << "Cannot size fixture: " << path.u8string(); return {}; }
  std::vector<uint8_t> bytes(static_cast<size_t>(size));
  stream.seekg(0);
  if (!bytes.empty()) stream.read(reinterpret_cast<char*>(bytes.data()), static_cast<std::streamsize>(bytes.size()));
  if (!stream) { ADD_FAILURE() << "Cannot read fixture: " << path.u8string(); return {}; }
  return bytes;
}

inline std::vector<uint8_t> Resource(const std::string& name) {
  return Read(TestRoot() / "upstream/pdfium/testing/resources" / name);
}

inline FPDF_PAGEOBJECT Add(const Api& api, FPDF_DOCUMENT document, FPDF_PAGE page,
                           const std::u16string& text, float x, float y,
                           float size = 18, const char* font = "Courier") {
  Handle object(api.FPDFPageObj_NewTextObj(document, font, size), api.FPDFPageObj_Destroy);
  EXPECT_NE(object.get(), nullptr);
  if (!object.get()) return nullptr;
  const auto units = Wide(text);
  EXPECT_TRUE(api.FPDFText_SetText(object.get(), units.data()));
  const FS_MATRIX matrix{1, 0, 0, 1, x, y};
  EXPECT_TRUE(api.FPDFPageObj_SetMatrix(object.get(), &matrix));
  const auto value = object.release();
  const auto inserted = api.FPDFPage_InsertObject(page, value);
  EXPECT_TRUE(inserted);
  return inserted ? value : nullptr;
}

inline std::u16string Extract(const Api& api, FPDF_TEXTPAGE text) {
  const int count = api.FPDFText_CountChars(text);
  EXPECT_GE(count, 0);
  if (count <= 0) return {};
  std::vector<unsigned short> units(static_cast<size_t>(count) + 1, 0xffff);
  const int actual = api.FPDFText_GetText(text, 0, count, units.data());
  EXPECT_GT(actual, 0);
  EXPECT_LE(actual, count + 1);
  if (actual <= 0 || actual > count + 1) return {};
  EXPECT_EQ(units[actual - 1], 0);
  return std::u16string(units.begin(), units.begin() + actual - 1);
}

inline void CompareTextRaster(const Raster& expected, const Raster& actual,
                              Artifacts& artifacts, const std::string& name,
                              RasterTolerance tolerance=RasterTolerance{32,0,.7}) {
  if (!raster_comparison_settings().enabled) return;
  ASSERT_EQ(expected.width, actual.width);
  ASSERT_EQ(expected.height, actual.height);
  ASSERT_EQ(expected.pixels.size(), actual.pixels.size());
  int left=expected.width, top=expected.height, right=-1, bottom=-1;
  for(int y=0;y<expected.height;++y)for(int x=0;x<expected.width;++x) {
    const size_t offset=(static_cast<size_t>(y)*expected.width+x)*4;
    bool painted=false;
    for(size_t channel=0;channel<4;++channel)
      painted|=expected.pixels[offset+channel]!=255 || actual.pixels[offset+channel]!=255;
    if(painted) { left=std::min(left,x);top=std::min(top,y);right=std::max(right,x);bottom=std::max(bottom,y); }
  }
  if(right<left) { CompareRaster(expected,actual,artifacts,name);return; }
  // The same outlines use FreeType's coverage in native PDFium and AGG's
  // coverage in pdfoxium. Investigated standard-font and rotated/vertical text
  // fixtures differ by at most 26 channel levels at their glyph boundaries.
  // A 32-level per-channel bound permits those edges but rejects missing solid
  // glyphs; the mean bound applies to the union of painted content, not a large
  // blank page. Ownership, characters and geometry are checked independently.
  CompareRaster(CropRaster(expected,left,top,right-left+1,bottom-top+1),
                CropRaster(actual,left,top,right-left+1,bottom-top+1),
                artifacts,name,tolerance);
}

inline void CrossReload(const Library& writer, FPDF_DOCUMENT document,
                        Artifacts& artifacts, const std::string& name,
                        const std::function<void(const Api&, FPDF_PAGE,
                                                 FPDF_TEXTPAGE)>& inspect,
                        FPDF_DWORD flags = FPDF_NO_INCREMENTAL,
                        RasterTolerance tolerance=RasterTolerance{32,0,.7}) {
  const auto& api = writer.api();
  Handle page(api.FPDF_LoadPage(document, 0), api.FPDF_ClosePage);
  ASSERT_NE(page.get(), nullptr);
  ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
  Handle text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
  ASSERT_NE(text.get(), nullptr);
  ASSERT_NO_FATAL_FAILURE(inspect(api, page.get(), text.get()));
  const auto expected_text = Extract(api, text.get());
  const auto before = Render(api, page.get(), 400, 240);
  artifacts.Image(writer.name() + "-" + name + "-before", before);
  text.reset();
  page.reset();
  const auto bytes = SaveDocument(api, document, artifacts,
                                  writer.name() + "-" + name, flags);
  ASSERT_FALSE(bytes.empty());
  std::array<Raster, 2> after;
  size_t index = 0;
  for (const auto* reader : Implementations()) {
    SCOPED_TRACE(writer.name() + "-to-" + reader->name());
    const auto& read = reader->api();
    Handle reopened(read.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr),
                    read.FPDF_CloseDocument);
    ASSERT_NE(reopened.get(), nullptr);
    Handle reloaded(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
    ASSERT_NE(reloaded.get(), nullptr);
    Handle extracted(read.FPDFText_LoadPage(reloaded.get()), read.FPDFText_ClosePage);
    ASSERT_NE(extracted.get(), nullptr);
    EXPECT_EQ(Extract(read, extracted.get()), expected_text);
    ASSERT_NO_FATAL_FAILURE(inspect(read, reloaded.get(), extracted.get()));
    after[index] = Render(read, reloaded.get(), 400, 240);
    const auto label=writer.name() + "-to-" + reader->name() + "-" + name;
    if(writer.name()==reader->name()) CompareRaster(before,after[index],artifacts,label);
    else CompareTextRaster(before,after[index],artifacts,label,tolerance);
    ++index;
  }
  CompareTextRaster(after[0], after[1], artifacts, name + "-readers",tolerance);
}

inline Raster Bitmap(const Api& api, FPDF_BITMAP bitmap) {
  Raster result;
  if (!bitmap) return result;
  result.width = api.FPDFBitmap_GetWidth(bitmap);
  result.height = api.FPDFBitmap_GetHeight(bitmap);
  const int format = api.FPDFBitmap_GetFormat(bitmap);
  const int stride = api.FPDFBitmap_GetStride(bitmap);
  const auto* data = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap));
  EXPECT_NE(data, nullptr);
  if (!data || result.width <= 0 || result.height <= 0) return {};
  result.pixels.resize(static_cast<size_t>(result.width) * result.height * 4);
  for (int y = 0; y < result.height; ++y) {
    for (int x = 0; x < result.width; ++x) {
      auto* pixel = result.pixels.data() + (static_cast<size_t>(y) * result.width + x) * 4;
      const auto* source = data + static_cast<size_t>(y) * stride;
      if (format == FPDFBitmap_Gray) {
        pixel[0] = pixel[1] = pixel[2] = source[x]; pixel[3] = 255;
      } else {
        const int bpp = format == FPDFBitmap_BGR ? 3 : 4;
        source += x * bpp;
        std::copy_n(source, 3, pixel);
        pixel[3] = format == FPDFBitmap_BGRA ? source[3] : 255;
        if(format==FPDFBitmap_BGRA) {
          // GetRenderedBitmap returns straight BGRA. Compare premultiplied
          // color plus alpha: RGB of a transparent pixel has no visual
          // meaning, and a one-level coverage edge can otherwise differ by
          // the entire stroke color despite virtually identical appearance.
          for(int channel=0;channel<3;++channel)
            pixel[channel]=static_cast<uint8_t>((unsigned(pixel[channel])*pixel[3]+127)/255);
        }
      }
    }
  }
  return result;
}

}  // namespace pdfoxium::integration::text_fixture
