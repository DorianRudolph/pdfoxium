#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cstdint>
#include <limits>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

constexpr uint8_t kCanary = 0xa5;
constexpr std::size_t kGuard = 16;
constexpr std::array<int, 4> kFormats = {
    FPDFBitmap_Gray, FPDFBitmap_BGR, FPDFBitmap_BGRx, FPDFBitmap_BGRA};

int BytesPerPixel(int format) {
  return format == FPDFBitmap_Gray ? 1 : format == FPDFBitmap_BGR ? 3 : 4;
}

std::array<uint8_t, 4> ColorBytes(int format, uint32_t argb) {
  const auto blue = static_cast<uint8_t>(argb);
  const auto green = static_cast<uint8_t>(argb >> 8);
  const auto red = static_cast<uint8_t>(argb >> 16);
  if (format == FPDFBitmap_Gray) {
    return {static_cast<uint8_t>((30 * red + 59 * green + 11 * blue) / 100),
            0, 0, 0};
  }
  return {blue, green, red,
          format == FPDFBitmap_BGRA ? static_cast<uint8_t>(argb >> 24)
                                    : static_cast<uint8_t>(255)};
}

void SetExpectedPixel(std::vector<uint8_t>* bytes, std::size_t prefix,
                      int stride, int format, int x, int y, uint32_t argb) {
  const int bpp = BytesPerPixel(format);
  const auto color = ColorBytes(format, argb);
  const auto offset = prefix + static_cast<std::size_t>(y) * stride + x * bpp;
  std::copy_n(color.begin(), bpp, bytes->begin() + offset);
}

void ExpectPixel(const uint8_t* buffer, int stride, int format, int x, int y,
                 uint32_t argb) {
  const auto color = ColorBytes(format, argb);
  const int bpp = BytesPerPixel(format);
  const auto* actual = buffer + static_cast<std::size_t>(y) * stride + x * bpp;
  for (int channel = 0; channel < bpp; ++channel) {
    EXPECT_EQ(actual[channel], color[channel]) << "channel=" << channel;
  }
}

std::vector<uint8_t> ActiveBytes(const Api& api, FPDF_BITMAP bitmap) {
  const int width = api.FPDFBitmap_GetWidth(bitmap);
  const int height = api.FPDFBitmap_GetHeight(bitmap);
  const int bpp = BytesPerPixel(api.FPDFBitmap_GetFormat(bitmap));
  const int stride = api.FPDFBitmap_GetStride(bitmap);
  const auto* buffer = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap));
  std::vector<uint8_t> packed;
  for (int row = 0; row < height; ++row) {
    packed.insert(packed.end(), buffer + static_cast<std::size_t>(row) * stride,
                  buffer + static_cast<std::size_t>(row) * stride + width * bpp);
  }
  return packed;
}

TEST(Bitmaps, CreateAlphaAndOpaqueHaveTopDownFourByteRows) {
  std::array<std::vector<uint8_t>, 2> observations;
  std::size_t backend = 0;
  for (const Library* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const Api& api = library->api();
    for (int alpha : {0, 1}) {
      for (int width : {1, 3, 7}) {
        SCOPED_TRACE("alpha=" + std::to_string(alpha) +
                     " width=" + std::to_string(width));
        constexpr int height = 4;
        Handle bitmap(api.FPDFBitmap_Create(width, height, alpha),
                      api.FPDFBitmap_Destroy);
        ASSERT_NE(bitmap.get(), nullptr);
        const int format = alpha ? FPDFBitmap_BGRA : FPDFBitmap_BGRx;
        EXPECT_EQ(api.FPDFBitmap_GetFormat(bitmap.get()), format);
        EXPECT_EQ(api.FPDFBitmap_GetWidth(bitmap.get()), width);
        EXPECT_EQ(api.FPDFBitmap_GetHeight(bitmap.get()), height);
        EXPECT_EQ(api.FPDFBitmap_GetStride(bitmap.get()), width * 4);
        auto* buffer = static_cast<uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
        ASSERT_NE(buffer, nullptr);
        EXPECT_EQ(reinterpret_cast<std::uintptr_t>(buffer) % alignof(uint32_t), 0u);
        ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, width, height,
                                           0x42804020));
        ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, width, 1,
                                           0x10203040));
        ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, height - 1, width, 1,
                                           0x90abcdef));
        ExpectPixel(buffer, width * 4, format, 0, 0, 0x10203040);
        ExpectPixel(buffer, width * 4, format, width - 1, 1, 0x42804020);
        ExpectPixel(buffer, width * 4, format, width - 1, height - 1, 0x90abcdef);
        const auto packed = ActiveBytes(api, bitmap.get());
        observations[backend].insert(observations[backend].end(), packed.begin(),
                                     packed.end());
      }
    }
    ++backend;
  }
  EXPECT_EQ(observations[0], observations[1]);
}

TEST(Bitmaps, EveryFormatSupportsInternalExternalAutomaticAndPaddedStorage) {
  std::array<std::vector<uint8_t>, 2> observations;
  std::size_t backend = 0;
  for (const Library* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const Api& api = library->api();
    for (int format : kFormats) {
      for (int width : {3, 5}) {
        for (bool external : {false, true}) {
          for (bool padded : {false, true}) {
            SCOPED_TRACE("format=" + std::to_string(format) +
                         " width=" + std::to_string(width) +
                         " external=" + std::to_string(external) +
                         " padded=" + std::to_string(padded));
            constexpr int height = 4;
            const int bpp = BytesPerPixel(format);
            const int automatic = (width * bpp + 3) & ~3;
            const int expected_stride = automatic + (padded ? 8 : 0);
            const std::size_t size = static_cast<std::size_t>(expected_stride) * height;
            std::vector<uint8_t> storage(kGuard + size + kGuard, kCanary);
            const auto original = storage;
            {
              Handle bitmap(api.FPDFBitmap_CreateEx(
                                width, height, format,
                                external ? storage.data() + kGuard : nullptr,
                                padded ? expected_stride : 0),
                            api.FPDFBitmap_Destroy);
              ASSERT_NE(bitmap.get(), nullptr);
              EXPECT_EQ(api.FPDFBitmap_GetFormat(bitmap.get()), format);
              EXPECT_EQ(api.FPDFBitmap_GetWidth(bitmap.get()), width);
              EXPECT_EQ(api.FPDFBitmap_GetHeight(bitmap.get()), height);
              ASSERT_EQ(api.FPDFBitmap_GetStride(bitmap.get()), expected_stride);
              auto* buffer = static_cast<uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
              ASSERT_NE(buffer, nullptr);
              if (external) { EXPECT_EQ(buffer, storage.data() + kGuard); }
              std::fill_n(buffer, size, kCanary);
              std::vector<uint8_t> expected(size, kCanary);
              ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, width, height,
                                                 0x32804020));
              for (int y = 0; y < height; ++y) {
                for (int x = 0; x < width; ++x) {
                  SetExpectedPixel(&expected, 0, expected_stride, format, x, y,
                                    0x32804020);
                }
              }
              ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 1, 1, 2, 2,
                                                 0xa0123456));
              for (int y = 1; y < 3; ++y) {
                for (int x = 1; x < 3; ++x) {
                  SetExpectedPixel(&expected, 0, expected_stride, format, x, y,
                                    0xa0123456);
                }
              }
              // The public buffer points to the same storage that FillRect uses.
              const auto changed = ColorBytes(format, 0xffdddddd);
              std::copy_n(changed.begin(), bpp, buffer);
              SetExpectedPixel(&expected, 0, expected_stride, format, 0, 0,
                                0xffdddddd);
              EXPECT_EQ(api.FPDFBitmap_GetBuffer(bitmap.get()), buffer);
              const std::vector<uint8_t> actual(buffer, buffer + size);
              EXPECT_EQ(actual, expected);
              observations[backend].insert(observations[backend].end(),
                                           actual.begin(), actual.end());
              if (external) {
                EXPECT_TRUE(std::equal(storage.begin(), storage.begin() + kGuard,
                                       original.begin()));
                EXPECT_TRUE(std::equal(storage.end() - kGuard, storage.end(),
                                       original.end() - kGuard));
              }
            }
            // Destroy releases only the bitmap wrapper; the external allocation
            // remains live and writable, including its row padding and guards.
            if (external) {
              storage[kGuard] = 0x6d;
              EXPECT_EQ(storage[kGuard], 0x6d);
              EXPECT_EQ(storage.front(), kCanary);
              EXPECT_EQ(storage.back(), kCanary);
            }
          }
        }
      }
    }
    ++backend;
  }
  EXPECT_EQ(observations[0], observations[1]);
}

TEST(Bitmaps, PackedExternalBgrStrideNeedNotBeFourByteAligned) {
  std::array<std::vector<uint8_t>, 2> observations;
  std::size_t backend = 0;
  for (const Library* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const Api& api = library->api();
    constexpr int width = 5, height = 3, stride = width * 3;
    std::array<uint8_t, kGuard + stride * height + kGuard> storage;
    storage.fill(kCanary);
    {
      Handle bitmap(api.FPDFBitmap_CreateEx(width, height, FPDFBitmap_BGR,
                                            storage.data() + kGuard, stride),
                    api.FPDFBitmap_Destroy);
      ASSERT_NE(bitmap.get(), nullptr);
      EXPECT_EQ(api.FPDFBitmap_GetStride(bitmap.get()), stride);
      ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, width, height,
                                         0x00123456));
      for (int row = 0; row < height; ++row) {
        ExpectPixel(storage.data() + kGuard, stride, FPDFBitmap_BGR, 0, row,
                    0x00123456);
        ExpectPixel(storage.data() + kGuard, stride, FPDFBitmap_BGR, width - 1,
                    row, 0x00123456);
      }
      observations[backend] = ActiveBytes(api, bitmap.get());
    }
    EXPECT_TRUE(std::all_of(storage.begin(), storage.begin() + kGuard,
                            [](uint8_t byte) { return byte == kCanary; }));
    EXPECT_TRUE(std::all_of(storage.end() - kGuard, storage.end(),
                            [](uint8_t byte) { return byte == kCanary; }));
    ++backend;
  }
  EXPECT_EQ(observations[0], observations[1]);
}

TEST(Bitmaps, InvalidCreationRejectsDimensionsFormatsStridesAndOverflow) {
  for (const Library* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const Api& api = library->api();
    for (const auto dimensions : {std::array<int, 2>{0, 0}, {0, 3}, {3, 0},
                                  {-1, 3}, {3, -1}, {-1, -1}}) {
      for (int alpha : {0, 1}) {
        Handle bitmap(api.FPDFBitmap_Create(dimensions[0], dimensions[1], alpha),
                      api.FPDFBitmap_Destroy);
        EXPECT_EQ(bitmap.get(), nullptr);
      }
      for (int format : kFormats) {
        Handle bitmap(api.FPDFBitmap_CreateEx(dimensions[0], dimensions[1], format,
                                              nullptr, 0),
                      api.FPDFBitmap_Destroy);
        EXPECT_EQ(bitmap.get(), nullptr);
      }
    }
    for (int format : {FPDFBitmap_Unknown, -1, 99, FPDFBitmap_BGRA_Premul}) {
      Handle bitmap(api.FPDFBitmap_CreateEx(3, 2, format, nullptr, 0),
                    api.FPDFBitmap_Destroy);
      EXPECT_EQ(bitmap.get(), nullptr);
    }
    std::array<uint8_t, 64> storage;
    storage.fill(kCanary);
    for (int format : kFormats) {
      const int too_short = 5 * BytesPerPixel(format) - 1;
      Handle external(api.FPDFBitmap_CreateEx(5, 2, format, storage.data(),
                                              too_short),
                      api.FPDFBitmap_Destroy);
      EXPECT_EQ(external.get(), nullptr);
      Handle internal(api.FPDFBitmap_CreateEx(5, 2, format, nullptr, too_short),
                      api.FPDFBitmap_Destroy);
      EXPECT_EQ(internal.get(), nullptr);
    }
    EXPECT_TRUE(std::all_of(storage.begin(), storage.end(),
                            [](uint8_t byte) { return byte == kCanary; }));
    // These fail checked pitch/size arithmetic before any large allocation.
    for (const auto dimensions : {std::array<int, 2>{std::numeric_limits<int>::max(), 1},
                                  {1048576, 1024}, {1, std::numeric_limits<int>::max()}}) {
      Handle bitmap(api.FPDFBitmap_Create(dimensions[0], dimensions[1], 1),
                    api.FPDFBitmap_Destroy);
      EXPECT_EQ(bitmap.get(), nullptr);
      Handle extended(api.FPDFBitmap_CreateEx(dimensions[0], dimensions[1],
                                              FPDFBitmap_BGRA, nullptr, 0),
                      api.FPDFBitmap_Destroy);
      EXPECT_EQ(extended.get(), nullptr);
    }
    Handle negative_stride(api.FPDFBitmap_CreateEx(5, 2, FPDFBitmap_BGRA,
                                                   nullptr, -1),
                            api.FPDFBitmap_Destroy);
    EXPECT_EQ(negative_stride.get(), nullptr);
  }
}

TEST(Bitmaps, FillClipsEdgesReplacesAlphaAndLeavesEmptyOrOverflowInputsUntouched) {
  std::array<std::vector<uint8_t>, 2> observations;
  std::size_t backend = 0;
  for (const Library* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const Api& api = library->api();
    for (int format : kFormats) {
      SCOPED_TRACE(format);
      constexpr int width = 5, height = 4;
      Handle bitmap(api.FPDFBitmap_CreateEx(width, height, format, nullptr, 0),
                    api.FPDFBitmap_Destroy);
      ASSERT_NE(bitmap.get(), nullptr);
      ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, width, height,
                                         0xff102030));
      ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), -2, -1, 4, 3, 0x40804020));
      ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 4, 3, 8, 7, 0x00112233));
      const auto* buffer = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
      ASSERT_NE(buffer, nullptr);
      const int stride = api.FPDFBitmap_GetStride(bitmap.get());
      for (int y = 0; y < height; ++y) {
        for (int x = 0; x < width; ++x) {
          const uint32_t color = x < 2 && y < 2 ? 0x40804020
                                 : x == 4 && y == 3 ? 0x00112233
                                                   : 0xff102030;
          ExpectPixel(buffer, stride, format, x, y, color);
        }
      }
      const auto before = ActiveBytes(api, bitmap.get());
      EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, 0, 0, 0xffffffff));
      EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), width, height, 2, 3,
                                         0xffffffff));
      EXPECT_FALSE(api.FPDFBitmap_FillRect(bitmap.get(),
                                           std::numeric_limits<int>::max(), 0,
                                           1, 1, 0xffffffff));
      EXPECT_FALSE(api.FPDFBitmap_FillRect(bitmap.get(), 0,
                                           std::numeric_limits<int>::max(),
                                           1, 1, 0xffffffff));
      EXPECT_EQ(ActiveBytes(api, bitmap.get()), before);
      observations[backend].insert(observations[backend].end(), before.begin(),
                                   before.end());
    }
    ++backend;
  }
  EXPECT_EQ(observations[0], observations[1]);
}

TEST(Bitmaps, RenderingUsesExternalBufferAndPreservesPaddingAndCallerEdits) {
  std::array<std::vector<uint8_t>, 2> observations;
  std::size_t backend = 0;
  for (const Library* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const Api& api = library->api();
    constexpr int width = 13, height = 9;
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDFPage_New(document.get(), 0, width, height), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    Handle rect(api.FPDFPageObj_CreateNewRect(2, 2, 5, 3), api.FPDFPageObj_Destroy);
    ASSERT_NE(rect.get(), nullptr);
    ASSERT_TRUE(api.FPDFPageObj_SetFillColor(rect.get(), 0, 0, 0, 255));
    ASSERT_TRUE(api.FPDFPath_SetDrawMode(rect.get(), FPDF_FILLMODE_WINDING, 0));
    api.FPDFPage_InsertObject(page.get(), rect.release());
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    for (int format : kFormats) {
      SCOPED_TRACE(format);
      const int bpp = BytesPerPixel(format);
      const int stride = ((width * bpp + 3) & ~3) + 8;
      const std::size_t size = static_cast<std::size_t>(stride) * height;
      std::vector<uint8_t> storage(kGuard + size + kGuard, kCanary);
      std::vector<uint8_t> expected = storage;
      {
        Handle bitmap(api.FPDFBitmap_CreateEx(width, height, format,
                                              storage.data() + kGuard, stride),
                      api.FPDFBitmap_Destroy);
        ASSERT_NE(bitmap.get(), nullptr);
        ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, width, height,
                                           0xffffffff));
        for (int y = 0; y < height; ++y) {
          for (int x = 0; x < width; ++x) {
            const bool inside = x >= 2 && x < 7 && y >= 4 && y < 7;
            SetExpectedPixel(&expected, kGuard, stride, format, x, y,
                              inside ? 0xff000000 : 0xffffffff);
          }
        }
        const uint32_t modified = format == FPDFBitmap_Gray ? 0xff4d4d4d : 0xff191109;
        auto* buffer = static_cast<uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
        ASSERT_EQ(buffer, storage.data() + kGuard);
        const auto edit = ColorBytes(format, modified);
        std::copy_n(edit.begin(), bpp, buffer);
        SetExpectedPixel(&expected, kGuard, stride, format, 0, 0, modified);
        api.FPDF_RenderPageBitmap(bitmap.get(), page.get(), 0, 0, width, height, 0, FPDF_NO_NATIVETEXT);
        EXPECT_EQ(storage, expected);
        const auto packed = ActiveBytes(api, bitmap.get());
        observations[backend].insert(observations[backend].end(), packed.begin(),
                                     packed.end());
      }
      EXPECT_EQ(storage, expected);
      storage[kGuard] ^= 0xff;
      EXPECT_NE(storage[kGuard], expected[kGuard]);
    }
    ++backend;
  }
  EXPECT_EQ(observations[0], observations[1]);
}

TEST(Bitmaps, ImageObjectBitmapsExposeOwnedReadableWritableRows) {
  std::array<std::vector<uint8_t>, 2> observations;
  std::size_t backend = 0;
  for (const Library* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const Api& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    for (int source_format : kFormats) {
      SCOPED_TRACE(source_format);
      constexpr int width = 3, height = 2;
      Handle image(api.FPDFPageObj_NewImageObj(document.get()), api.FPDFPageObj_Destroy);
      ASSERT_NE(image.get(), nullptr);
      {
        Handle source(api.FPDFBitmap_CreateEx(width, height, source_format,
                                              nullptr, 0),
                      api.FPDFBitmap_Destroy);
        ASSERT_NE(source.get(), nullptr);
        ASSERT_TRUE(api.FPDFBitmap_FillRect(source.get(), 0, 0, width, height,
                                           0xff305070));
        ASSERT_TRUE(api.FPDFBitmap_FillRect(source.get(), 0, 0, width, 1,
                                           0xff909090));
        ASSERT_TRUE(api.FPDFImageObj_SetBitmap(nullptr, 0, image.get(), source.get()));
      }
      // The image's encoded samples survive destruction of the source bitmap.
      Handle bitmap(api.FPDFImageObj_GetBitmap(image.get()), api.FPDFBitmap_Destroy);
      ASSERT_NE(bitmap.get(), nullptr);
      EXPECT_EQ(api.FPDFBitmap_GetWidth(bitmap.get()), width);
      EXPECT_EQ(api.FPDFBitmap_GetHeight(bitmap.get()), height);
      const int format = api.FPDFBitmap_GetFormat(bitmap.get());
      ASSERT_NE(std::find(kFormats.begin(), kFormats.end(), format), kFormats.end());
      // SetBitmap encodes 8-bit RGB sources as indexed grayscale. GetBitmap
      // expands that palette to BGR because the public API cannot expose it.
      EXPECT_EQ(format, FPDFBitmap_BGR);
      const int stride = api.FPDFBitmap_GetStride(bitmap.get());
      EXPECT_EQ(stride, (width * BytesPerPixel(format) + 3) & ~3);
      auto* buffer = static_cast<uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
      ASSERT_NE(buffer, nullptr);
      const uint32_t last_color = source_format == FPDFBitmap_Gray ? 0xff494949
                                                                  : 0xff305070;
      ExpectPixel(buffer, stride, format, 0, 0, 0xff909090);
      ExpectPixel(buffer, stride, format, width - 1, height - 1, last_color);
      const auto packed = ActiveBytes(api, bitmap.get());
      observations[backend].insert(observations[backend].end(), packed.begin(),
                                   packed.end());
      buffer[0] = 0x17;
      EXPECT_EQ(static_cast<uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()))[0], 0x17);
      Handle second(api.FPDFImageObj_GetBitmap(image.get()), api.FPDFBitmap_Destroy);
      ASSERT_NE(second.get(), nullptr);
      ExpectPixel(static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(second.get())),
                  api.FPDFBitmap_GetStride(second.get()),
                  api.FPDFBitmap_GetFormat(second.get()), 0, 0, 0xff909090);
    }
    ++backend;
  }
  EXPECT_EQ(observations[0], observations[1]);
}

TEST(Bitmaps, NullQueriesAndFillHaveDefinedFailureResults) {
  for (const Library* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const Api& api = library->api();
    EXPECT_EQ(api.FPDFBitmap_GetFormat(nullptr), FPDFBitmap_Unknown);
    EXPECT_EQ(api.FPDFBitmap_GetWidth(nullptr), 0);
    EXPECT_EQ(api.FPDFBitmap_GetHeight(nullptr), 0);
    EXPECT_EQ(api.FPDFBitmap_GetStride(nullptr), 0);
    EXPECT_EQ(api.FPDFBitmap_GetBuffer(nullptr), nullptr);
    EXPECT_FALSE(api.FPDFBitmap_FillRect(nullptr, 0, 0, 1, 1, 0xffffffff));
    api.FPDFBitmap_Destroy(nullptr);
  }
}

}  // namespace
}  // namespace pdfoxium::integration
