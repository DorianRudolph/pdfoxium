#include "runtime_fixture.h"

#include <array>
#include <cstring>

namespace pdfoxium::integration {
namespace {

TEST(Thumbnails, RawDecodedAndOwnedPixelsHonorAllBufferCapacities) {
  Artifacts artifacts;
  // The public API cannot create /Thumb. This explicit 2x2 RGB thumbnail is
  // red/green on the first row and blue/white on the second, top-down.
  const std::string samples("\xff\0\0\0\xff\0\0\0\xff\xff\xff\xff", 12);
  const std::string hex = "ff000000ff000000ffffffff>";
  std::array<std::vector<uint8_t>, 2> observations;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    SCOPED_TRACE(library->name());
    for (bool filtered : {false, true}) {
      const std::string encoded = filtered ? hex : samples;
      const auto bytes = runtime_test::RectangleFixture("", "/Thumb 5 0 R", {
          PdfStream("/Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceRGB " +
                        std::string(filtered ? "/Filter /ASCIIHexDecode" : ""), encoded)});
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      for (bool decoded : {false, true}) {
        const auto get = decoded ? api.FPDFPage_GetDecodedThumbnailData
                                  : api.FPDFPage_GetRawThumbnailData;
        const std::string& expected = decoded ? samples : encoded;
        ASSERT_EQ(get(page.get(), nullptr, 0), expected.size());
        for (size_t capacity : {size_t{0}, expected.size() - 1, expected.size(), expected.size() + 3}) {
          // The pinned native stream-copy helper reverses its capacity check.
          // Native exercises NULL sizing and exact capacity; pdfoxium also
          // exercises the documented short/oversized contracts safely.
          if (library->name() == "native" && capacity != expected.size()) continue;
          std::vector<uint8_t> buffer(capacity + 2, 0xa5);
          EXPECT_EQ(get(page.get(), buffer.data() + 1, capacity), expected.size());
          EXPECT_EQ(buffer.front(), 0xa5);
          EXPECT_EQ(buffer.back(), 0xa5);
          if (capacity >= expected.size()) {
            EXPECT_EQ(std::string(reinterpret_cast<const char*>(buffer.data() + 1), expected.size()), expected);
            EXPECT_TRUE(std::all_of(buffer.begin() + 1 + expected.size(), buffer.end(), [](uint8_t v) { return v == 0xa5; }));
          } else {
            EXPECT_TRUE(std::all_of(buffer.begin(), buffer.end(), [](uint8_t v) { return v == 0xa5; }));
          }
        }
      }
      Handle bitmap(api.FPDFPage_GetThumbnailAsBitmap(page.get()), api.FPDFBitmap_Destroy);
      ASSERT_NE(bitmap.get(), nullptr);
      EXPECT_EQ(api.FPDFBitmap_GetWidth(bitmap.get()), 2);
      EXPECT_EQ(api.FPDFBitmap_GetHeight(bitmap.get()), 2);
      EXPECT_EQ(api.FPDFBitmap_GetFormat(bitmap.get()), FPDFBitmap_BGR);
      const int stride = api.FPDFBitmap_GetStride(bitmap.get());
      ASSERT_GE(stride, 6);
      const auto* pixels = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
      ASSERT_NE(pixels, nullptr);
      const std::array<uint8_t, 12> bgr = {0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255};
      for (int y = 0; y < 2; ++y) {
        for (int x = 0; x < 6; ++x) EXPECT_EQ(pixels[y * stride + x], bgr[y * 6 + x]);
        observations[backend].insert(observations[backend].end(), pixels + y * stride, pixels + y * stride + 6);
      }
      const auto rendered = Render(api, page.get(), 128, 96);
      runtime_test::ExpectRectangle(rendered);
      artifacts.Image(library->name() + (filtered ? "-filtered" : "-raw"), rendered);
      bitmap.reset(); // Caller-owned thumbnail dies before its page/document.
    }
    ++backend;
  }
  EXPECT_EQ(observations[0], observations[1]);
}

TEST(Thumbnails, AbsentAndUnreadableStreamsHaveDefinedFailureResults) {
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    for (int kind : {0, 1}) {
      const auto bytes = kind == 0 ? runtime_test::RectangleFixture()
          : runtime_test::RectangleFixture("", "/Thumb 5 0 R", {PdfStream("/Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8", "")});
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      std::array<uint8_t, 4> sentinel{9, 8, 7, 6};
      EXPECT_EQ(api.FPDFPage_GetDecodedThumbnailData(page.get(), sentinel.data(), sentinel.size()), 0u);
      EXPECT_EQ(api.FPDFPage_GetRawThumbnailData(page.get(), sentinel.data(), sentinel.size()), 0u);
      EXPECT_EQ(sentinel, (std::array<uint8_t, 4>{9, 8, 7, 6}));
      Handle bitmap(api.FPDFPage_GetThumbnailAsBitmap(page.get()), api.FPDFBitmap_Destroy);
      EXPECT_EQ(bitmap.get(), nullptr);
    }
    EXPECT_EQ(api.FPDFPage_GetRawThumbnailData(nullptr, nullptr, 0), 0u);
    EXPECT_EQ(api.FPDFPage_GetDecodedThumbnailData(nullptr, nullptr, 0), 0u);
    EXPECT_EQ(api.FPDFPage_GetThumbnailAsBitmap(nullptr), nullptr);
  }
}

}  // namespace
}  // namespace pdfoxium::integration
