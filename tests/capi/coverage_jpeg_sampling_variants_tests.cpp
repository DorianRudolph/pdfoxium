#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using Color = std::array<int, 3>;

void Word(Bytes& out, unsigned value) {
  out.push_back(static_cast<uint8_t>(value >> 8));
  out.push_back(static_cast<uint8_t>(value));
}
void Marker(Bytes& out, uint8_t marker, const Bytes& payload) {
  out.insert(out.end(), {255, marker});
  Word(out, payload.size() + 2);
  out.insert(out.end(), payload.begin(), payload.end());
}
struct Entropy {
  Bytes bytes;
  unsigned pending = 0;
  int count = 0;
  void Write(unsigned value, int width) {
    for (int bit = width - 1; bit >= 0; --bit) {
      pending = (pending << 1) | ((value >> bit) & 1);
      if (++count == 8) {
        bytes.push_back(static_cast<uint8_t>(pending));
        if ((pending & 255) == 255) bytes.push_back(0);
        pending = 0;
        count = 0;
      }
    }
  }
  void Block(int coefficient, int& previous) {
    const int delta = coefficient - previous;
    previous = coefficient;
    unsigned magnitude = static_cast<unsigned>(std::abs(delta));
    int category = 0;
    while (magnitude) {
      ++category;
      magnitude >>= 1;
    }
    Write(category, 4);
    if (category) Write(delta >= 0 ? delta : (1 << category) - 1 + delta, category);
    Write(0, 1);  // AC EOB: every AC coefficient is exactly zero.
  }
  void Finish() {
    if (count) Write((1u << (8 - count)) - 1, 8 - count);
  }
};

// Test-only ITU T.81 baseline serialization, following the existing constant
// JPEG fixture in coverage_codec_deep_tests.cpp. Quantization is1, the DC table
// has categories0..11 at length4, and the AC table contains only EOB. A block's
// reconstructed sample is128+DC/8. No image encoder or production helper runs.
// Each MCU writes H*V luma blocks followed by one block per chroma component.
Bytes SamplingJpeg(int width, int height, int horizontal, int vertical, bool color) {
  Bytes out{255, 216};
  Marker(out, 0xe0, {'J','F','I','F',0,1,1,0,0,1,0,1,0,0});
  Bytes quant(65, 1);
  quant[0] = 0;
  Marker(out, 0xdb, quant);
  Bytes frame{8};
  Word(frame, height);
  Word(frame, width);
  frame.insert(frame.end(), {3, 1, static_cast<uint8_t>(16 * horizontal + vertical), 0, 2, 0x11, 0, 3, 0x11, 0});
  Marker(out, 0xc0, frame);
  Bytes huffman{0};
  for (int length = 1; length <= 16; ++length) huffman.push_back(length == 4 ? 12 : 0);
  for (int category = 0; category <= 11; ++category) huffman.push_back(category);
  huffman.push_back(0x10);
  for (int length = 1; length <= 16; ++length) huffman.push_back(length == 1 ? 1 : 0);
  huffman.push_back(0);
  Marker(out, 0xc4, huffman);
  Marker(out, 0xda, {3,1,0,2,0,3,0,0,63,0});
  Entropy entropy;
  std::array<int, 3> previous{};
  const int columns = (width + horizontal * 8 - 1) / (horizontal * 8);
  const int rows = (height + vertical * 8 - 1) / (vertical * 8);
  for (int row = 0; row < rows; ++row) {
    for (int column = 0; column < columns; ++column) {
      for (int block = 0; block < horizontal * vertical; ++block) {
        entropy.Block(color ? 0 : 64, previous[0]);
      }
      entropy.Block(color ? 128 : 0, previous[1]);
      entropy.Block(color ? -128 : 0, previous[2]);
    }
  }
  entropy.Finish();
  out.insert(out.end(), entropy.bytes.begin(), entropy.bytes.end());
  out.insert(out.end(), {255, 217});
  return out;
}

Bytes ImagePdf(const Bytes& image, int width, int height) {
  const std::string w = std::to_string(width), h = std::to_string(height);
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 " + w + " " + h + "] /Resources << /XObject << /I 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", "q " + w + " 0 0 " + h + " 0 0 cm /I Do Q"),
      PdfStream("/Type /XObject /Subtype /Image /Width " + w + " /Height " + h +
          " /BitsPerComponent 8 /ColorSpace /DeviceRGB /Filter /DCTDecode",
          std::string(image.begin(), image.end()))});
}
void Samples(const Api& api, FPDF_PAGE page, const Raster& raster,
             int width, int height, bool color) {
  ASSERT_EQ(api.FPDFPage_CountObjects(page), 1);
  const auto image = api.FPDFPage_GetObject(page, 0);
  ASSERT_EQ(api.FPDFPageObj_GetType(image), FPDF_PAGEOBJ_IMAGE);
  unsigned actual_width = 0, actual_height = 0;
  ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(image, &actual_width, &actual_height));
  EXPECT_EQ(actual_width, static_cast<unsigned>(width));
  EXPECT_EQ(actual_height, static_cast<unsigned>(height));
  // JPEG YCbCr: (128,144,112) -> RGB(106,134,156), or neutral136.
  const Color expected = color ? Color{106,134,156} : Color{136,136,136};
  int maximum = 0;
  for (size_t pixel = 0; pixel < raster.pixels.size(); pixel += 4) {
    for (int channel = 0; channel < 3; ++channel) {
      maximum = std::max(maximum, std::abs(int(raster.pixels[pixel + 2 - channel]) - expected[channel]));
    }
    EXPECT_EQ(raster.pixels[pixel + 3], 255);
  }
  EXPECT_LE(maximum, 1);
}
void SamplingCase(int horizontal, int vertical, bool odd) {
  const int width = odd ? 19 : 64, height = odd ? 23 : 32;
  for (bool color : {false, true}) {
    const auto bytes = ImagePdf(SamplingJpeg(width, height, horizontal, vertical, color), width, height);
    for (int reduction : (odd ? std::vector<int>{1,2} : std::vector<int>{1,2,4,8})) {
      SCOPED_TRACE(std::to_string(horizontal) + "x" + std::to_string(vertical) +
          (color ? "-color-" : "-gray-") + std::to_string(reduction));
      Artifacts artifacts;
      std::array<Raster, 2> originals;
      size_t index = 0;
      auto render = [&](const Api& api, FPDF_PAGE page) {
        return Render(api, page, std::max(1, width / reduction), std::max(1, height / reduction), FPDF_RENDER_NO_SMOOTHIMAGE);
      };
      for (const auto* writer : Implementations()) {
        SCOPED_TRACE(writer->name());
        const auto& api = writer->api();
        Handle doc(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
        ASSERT_NE(doc.get(), nullptr);
        {
          Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
          ASSERT_NE(page.get(), nullptr);
          originals[index] = render(api, page.get());
          Samples(api, page.get(), originals[index], width, height, color);
        }
        const auto saved = SaveDocument(api, doc.get(), artifacts, writer->name() + "-jpeg-sampling");
        ASSERT_FALSE(saved.empty());
        for (const auto* reader : Implementations()) {
          SCOPED_TRACE(reader->name());
          const auto& read = reader->api();
          Handle reopened(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
          ASSERT_NE(reopened.get(), nullptr);
          Handle page(read.FPDF_LoadPage(reopened.get(), 0), read.FPDF_ClosePage);
          ASSERT_NE(page.get(), nullptr);
          const auto after = render(read, page.get());
          Samples(read, page.get(), after, width, height, color);
          CompareRaster(originals[index], after, artifacts, writer->name() + "-" + reader->name(), RasterTolerance::ConnectedError(1));
        }
        ++index;
      }
      CompareRaster(originals[0], originals[1], artifacts, "jpeg-sampling-engines", RasterTolerance::ConnectedError(1));
    }
  }
}

TEST(CoverageJpegSamplingVariants, Sampling444AlignedReductions) {
  SamplingCase(1, 1, false);
}

TEST(CoverageJpegSamplingVariants, Sampling444OddEdges) {
  SamplingCase(1, 1, true);
}

TEST(CoverageJpegSamplingVariants, Sampling422HorizontalAlignedReductions) {
  SamplingCase(2, 1, false);
}

TEST(CoverageJpegSamplingVariants, Sampling422HorizontalOddEdges) {
  SamplingCase(2, 1, true);
}

TEST(CoverageJpegSamplingVariants, Sampling422VerticalAlignedReductions) {
  SamplingCase(1, 2, false);
}

TEST(CoverageJpegSamplingVariants, Sampling422VerticalOddEdges) {
  SamplingCase(1, 2, true);
}

TEST(CoverageJpegSamplingVariants, Sampling420AlignedReductions) {
  SamplingCase(2, 2, false);
}

TEST(CoverageJpegSamplingVariants, Sampling420OddEdges) {
  SamplingCase(2, 2, true);
}

TEST(CoverageJpegSamplingVariants, Sampling411HorizontalAlignedReductions) {
  SamplingCase(4, 1, false);
}

TEST(CoverageJpegSamplingVariants, Sampling411HorizontalOddEdges) {
  SamplingCase(4, 1, true);
}

TEST(CoverageJpegSamplingVariants, Sampling411VerticalAlignedReductions) {
  SamplingCase(1, 4, false);
}

TEST(CoverageJpegSamplingVariants, Sampling411VerticalOddEdges) {
  SamplingCase(1, 4, true);
}

}  // namespace
}  // namespace pdfoxium::integration
