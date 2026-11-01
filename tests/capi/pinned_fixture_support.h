#pragma once

#include "text_fixture.h"

#include <limits>

namespace pdfoxium::integration {

// Developer-prepared, lossless RGBA copies of the explicitly selected pinned
// PNGs. PNG decoding is not a runtime dependency of the C++ harness.
inline Raster LoadPinnedGolden(const std::string& png_relative_path) {
  auto relative = std::filesystem::path(png_relative_path);
  relative.replace_extension(".rgba");
  const auto path = TestRoot() / "tests/capi/resources/pixel_goldens" / relative;
  const auto bytes = text_fixture::Read(path);
  EXPECT_GE(bytes.size(), 12u) << path.u8string();
  if (bytes.size() < 12) return {};
  EXPECT_EQ(std::string(bytes.begin(), bytes.begin() + 4), "PDRG");
  if (std::string(bytes.begin(), bytes.begin() + 4) != "PDRG") return {};
  const auto word = [&](size_t offset) {
    return uint32_t(bytes[offset]) | (uint32_t(bytes[offset + 1]) << 8) |
           (uint32_t(bytes[offset + 2]) << 16) | (uint32_t(bytes[offset + 3]) << 24);
  };
  const auto width = word(4), height = word(8);
  const auto int_max = static_cast<uint32_t>(std::numeric_limits<int>::max());
  EXPECT_TRUE(width > 0 && height > 0 && width <= int_max && height <= int_max);
  if (!width || !height || width > int_max || height > int_max) return {};
  const auto payload = uint64_t(width) * height * 4;
  EXPECT_EQ(payload, bytes.size() - 12) << path.u8string();
  if (payload != bytes.size() - 12) return {};
  Raster result{static_cast<int>(width), static_cast<int>(height),
                {bytes.begin() + 12, bytes.end()}};
  for (size_t offset = 0; offset < result.pixels.size(); offset += 4)
    std::swap(result.pixels[offset], result.pixels[offset + 2]);
  return result;
}

}  // namespace pdfoxium::integration
