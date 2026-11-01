#include "editing_fixture.h"
#include <algorithm>

#include <cmath>
#include <cstring>
#include <fstream>
#include <iterator>
#include <set>

namespace pdfoxium::integration {
namespace {

std::vector<uint8_t> ReadImageFile(const std::filesystem::path& path) {
  std::ifstream stream(path, std::ios::binary);
  EXPECT_TRUE(stream.good()) << path.u8string();
  return {std::istreambuf_iterator<char>(stream), std::istreambuf_iterator<char>()};
}

Raster CopyImageBitmap(const Api& api, FPDF_BITMAP bitmap) {
  Raster result;
  if (!bitmap) { ADD_FAILURE() << "Missing image bitmap"; return result; }
  result.width = api.FPDFBitmap_GetWidth(bitmap);
  result.height = api.FPDFBitmap_GetHeight(bitmap);
  const int format = api.FPDFBitmap_GetFormat(bitmap);
  const int stride = api.FPDFBitmap_GetStride(bitmap);
  const auto* buffer = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap));
  EXPECT_GT(result.width, 0); EXPECT_GT(result.height, 0); EXPECT_NE(buffer, nullptr);
  if (!buffer || result.width <= 0 || result.height <= 0) return result;
  const int bpp = format == FPDFBitmap_Gray ? 1 : format == FPDFBitmap_BGR ? 3 : 4;
  EXPECT_GE(stride, result.width * bpp);
  result.pixels.reserve(static_cast<size_t>(result.width) * result.height * 4);
  for (int y = 0; y < result.height; ++y) for (int x = 0; x < result.width; ++x) {
    const auto* pixel = buffer + y * stride + x * bpp;
    result.pixels.push_back(pixel[0]);
    result.pixels.push_back(bpp == 1 ? pixel[0] : pixel[1]);
    result.pixels.push_back(bpp == 1 ? pixel[0] : pixel[2]);
    result.pixels.push_back(format == FPDFBitmap_BGRA ? pixel[3] : 255);
  }
  return result;
}

std::vector<uint8_t> ImageFixture(const std::string& color_space,
                                  const std::string& data,
                                  const std::string& image_extra = "",
                                  std::vector<std::string> extra = {}) {
  std::vector<std::string> objects = {
      PdfStream("/Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace " +
                 color_space + " " + image_extra, data)};
  objects.insert(objects.end(), extra.begin(), extra.end());
  return editing::SceneFixture("/Span << /MCID 7 >> BDC q 32 0 0 16 16 32 cm /I Do Q EMC",
                               "/XObject << /I 5 0 R >>", objects);
}

std::vector<uint8_t> ImageData(const Library& library, FPDF_PAGEOBJECT object,
                               bool decoded, const std::vector<uint8_t>* expected = nullptr) {
  const auto& api = library.api();
  const auto get = decoded ? api.FPDFImageObj_GetImageDataDecoded : api.FPDFImageObj_GetImageDataRaw;
  const auto needed = get(object, nullptr, 0);
  std::vector<uint8_t> output(needed + 3, 0xa5);
  if (library.name() == "rust" && needed) {
    // The pinned native helper has an inverted capacity check and traps on
    // nonzero short buffers. Verify the public safe contract on Rust only.
    EXPECT_EQ(get(object, output.data(), needed - 1), needed);
    EXPECT_TRUE(std::all_of(output.begin(), output.end(), [](auto c) { return c == 0xa5; }));
  }
  EXPECT_EQ(get(object, output.data(), needed), needed);
  EXPECT_EQ(output[needed], 0xa5);
  if (expected) {
    EXPECT_EQ(needed, expected->size());
    EXPECT_TRUE(std::equal(expected->begin(), expected->end(), output.begin()));
  }
  if (library.name() == "rust") {
    std::fill(output.begin(), output.end(), 0xa5);
    EXPECT_EQ(get(object, output.data(), output.size()), needed);
    if (expected) { EXPECT_TRUE(std::equal(expected->begin(), expected->end(), output.begin())); }
    EXPECT_EQ(output[needed], 0xa5);
  }
  return {output.begin(), output.begin() + needed};
}

std::vector<std::string> Filters(const Api& api, FPDF_PAGEOBJECT image) {
  std::vector<std::string> result;
  const int count = api.FPDFImageObj_GetImageFilterCount(image);
  EXPECT_GE(count, 0);
  for (int index = 0; index < count; ++index) {
    const auto needed = api.FPDFImageObj_GetImageFilter(image, index, nullptr, 0);
    EXPECT_GT(needed, 0u);
    std::vector<char> buffer(needed + 2, '\x55');
    EXPECT_EQ(api.FPDFImageObj_GetImageFilter(image, index, buffer.data(), needed - 1), needed);
    EXPECT_TRUE(std::all_of(buffer.begin(), buffer.end(), [](char c) { return c == '\x55'; }));
    EXPECT_EQ(api.FPDFImageObj_GetImageFilter(image, index, buffer.data(), needed), needed);
    EXPECT_EQ(buffer[needed - 1], 0); EXPECT_EQ(buffer[needed], '\x55');
    result.emplace_back(buffer.data());
    EXPECT_EQ(api.FPDFImageObj_GetImageFilter(image, index, buffer.data(), buffer.size()), needed);
    EXPECT_EQ(buffer[needed], '\x55');
  }
  for (int invalid : {-1, count, count + 1}) {
    std::array<char, 16> buffer{}; buffer.fill('\x55');
    EXPECT_EQ(api.FPDFImageObj_GetImageFilter(image, invalid, buffer.data(), buffer.size()), 0u);
    EXPECT_TRUE(std::all_of(buffer.begin(), buffer.end(), [](char c) { return c == '\x55'; }));
  }
  return result;
}

TEST(Images, EmptyAndWrongTypeObjectsRejectPixelDependentOperations) {
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    Handle empty(api.FPDFPageObj_NewImageObj(doc.get()), api.FPDFPageObj_Destroy);
    ASSERT_NE(empty.get(), nullptr);
    EXPECT_EQ(api.FPDFPageObj_GetType(empty.get()), FPDF_PAGEOBJ_IMAGE);
    EXPECT_EQ(api.FPDFImageObj_GetBitmap(empty.get()), nullptr);
    EXPECT_EQ(api.FPDFImageObj_GetImageDataRaw(empty.get(), nullptr, 0), 0u);
    EXPECT_EQ(api.FPDFImageObj_GetImageDataDecoded(empty.get(), nullptr, 0), 0u);
    EXPECT_EQ(api.FPDFImageObj_GetImageFilterCount(empty.get()), 0);
    unsigned int width = 91, height = 92;
    ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(empty.get(), &width, &height));
    EXPECT_EQ(width, 0u); EXPECT_EQ(height, 0u);
    EXPECT_EQ(api.FPDFPageObj_NewImageObj(nullptr), nullptr);
    Handle path(api.FPDFPageObj_CreateNewRect(0, 0, 4, 4), api.FPDFPageObj_Destroy);
    for (auto invalid : {FPDF_PAGEOBJECT{}, path.get()}) {
      EXPECT_EQ(api.FPDFImageObj_GetBitmap(invalid), nullptr);
      EXPECT_EQ(api.FPDFImageObj_GetRenderedBitmap(doc.get(), page.get(), invalid), nullptr);
      EXPECT_FALSE(api.FPDFImageObj_SetMatrix(invalid, 1, 0, 0, 1, 0, 0));
      EXPECT_FALSE(api.FPDFImageObj_SetBitmap(nullptr, 0, invalid, nullptr));
      EXPECT_FALSE(api.FPDFImageObj_GetImageMetadata(invalid, page.get(), nullptr));
      FPDF_IMAGEOBJ_METADATA invalid_metadata{};
      invalid_metadata.width=91; invalid_metadata.height=92;
      EXPECT_FALSE(api.FPDFImageObj_GetImageMetadata(invalid,page.get(),&invalid_metadata));
      EXPECT_EQ(invalid_metadata.width,91u); EXPECT_EQ(invalid_metadata.height,92u);
      EXPECT_EQ(api.FPDFImageObj_GetImageFilterCount(invalid),0);
      EXPECT_EQ(api.FPDFImageObj_GetImageDataRaw(invalid, nullptr, 0), 0u);
      EXPECT_EQ(api.FPDFImageObj_GetImageDataDecoded(invalid, nullptr, 0), 0u);
      width = 91; height = 92;
      EXPECT_FALSE(api.FPDFImageObj_GetImagePixelSize(invalid, &width, &height));
      EXPECT_EQ(width, 91u); EXPECT_EQ(height, 92u);
      size_t needed = 991;
      EXPECT_FALSE(api.FPDFImageObj_GetIccProfileDataDecoded(invalid, page.get(), nullptr, 0, &needed)); EXPECT_EQ(needed, 991u);
    }
    FPDF_IMAGEOBJ_METADATA empty_metadata{};
    ASSERT_TRUE(api.FPDFImageObj_GetImageMetadata(empty.get(),page.get(),&empty_metadata));
    EXPECT_EQ(empty_metadata.width,0u); EXPECT_EQ(empty_metadata.height,0u);
    EXPECT_EQ(empty_metadata.marked_content_id,-1);
    // Native's image renderer dereferences the missing stream (confirmed in
    // CPDF_DIB::StartLoadDIBBase). Rust must return the documented failure.
    if(library->name()=="rust") {
      EXPECT_EQ(api.FPDFImageObj_GetRenderedBitmap(doc.get(),nullptr,empty.get()),nullptr);
    }
    EXPECT_FALSE(api.FPDFImageObj_SetBitmap(nullptr, 0, empty.get(), nullptr));
    EXPECT_FALSE(api.FPDFImageObj_GetImagePixelSize(empty.get(), nullptr, &height));
    EXPECT_FALSE(api.FPDFImageObj_GetImagePixelSize(empty.get(), &width, nullptr));
    EXPECT_FALSE(api.FPDFImageObj_GetImageMetadata(empty.get(), page.get(), nullptr));
    EXPECT_EQ(api.FPDFImageObj_GetRenderedBitmap(nullptr, page.get(), empty.get()), nullptr);
  }
}

TEST(Images, EveryBitmapFormatPaddingAlphaAndCacheReplacementPersist) {
  Artifacts artifacts;
  for (int format : {FPDFBitmap_Gray, FPDFBitmap_BGR, FPDFBitmap_BGRx, FPDFBitmap_BGRA}) {
    SCOPED_TRACE(format);
    std::array<Raster, 2> pictures;
    size_t backend = 0;
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name());
      const auto& api = writer->api();
      Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
      Handle other(api.FPDFPage_New(doc.get(), 1, 128, 128), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr); ASSERT_NE(other.get(), nullptr);
      const int bpp = format == FPDFBitmap_Gray ? 1 : format == FPDFBitmap_BGR ? 3 : 4;
      constexpr int width = 3, height = 2;
      const int stride = (width * bpp + 7) & ~3;
      std::vector<uint8_t> storage(stride * height + 16, 0xa5);
      Handle bitmap(api.FPDFBitmap_CreateEx(width, height, format, storage.data(), stride), api.FPDFBitmap_Destroy);
      ASSERT_NE(bitmap.get(), nullptr);
      ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, width, height, 0x7fff4020));
      auto image = api.FPDFPageObj_NewImageObj(doc.get()); ASSERT_NE(image, nullptr);
      ASSERT_TRUE(api.FPDFImageObj_SetBitmap(nullptr, 0, image, bitmap.get()));
      ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image, 48, 0, 0, 32, 16, 32));
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), image));
      const auto initial = Render(api, page.get(), 128, 128);
      auto previous=initial.pixels;
      artifacts.Image(writer->name()+"-bitmap-format-"+std::to_string(format)+"-initial",initial);
      // Explicitly supply zero, one and multiple loaded pages while replacing
      // image data that has already been rendered/cached.
      for (int count : {0, 1, 2}) {
        SCOPED_TRACE(count);
        ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, width, height,
                                           count % 2 ? 0xff00ff00 : 0xff0000ff));
        FPDF_PAGE pages[] = {page.get(), other.get()};
        ASSERT_TRUE(api.FPDFImageObj_SetBitmap(count ? pages : nullptr, count, image, bitmap.get()));
        auto current = Render(api, page.get(), 128, 128);
        EXPECT_NE(initial.pixels, current.pixels);
        EXPECT_NE(previous,current.pixels);
        previous=current.pixels;
        const int gray=count%2?(255*59/100):(255*11/100);
        const std::array<int,4> expected=format==FPDFBitmap_Gray?std::array<int,4>{gray,gray,gray,255}:
            count%2?std::array<int,4>{0,255,0,255}:std::array<int,4>{255,0,0,255};
        const auto center=(size_t{80}*128+40)*4; // PDF image center(40,48), top-down device row80.
        for(size_t channel=0;channel<4;++channel)EXPECT_EQ(current.pixels[center+channel],expected[channel]);
        artifacts.Image(writer->name()+"-bitmap-format-"+std::to_string(format)+"-replacement-"+std::to_string(count),current);
        Handle replaced(api.FPDFImageObj_GetBitmap(image),api.FPDFBitmap_Destroy);
        ASSERT_NE(replaced.get(),nullptr);const auto direct=CopyImageBitmap(api,replaced.get());
        ASSERT_EQ(direct.width,width);ASSERT_EQ(direct.height,height);
        for(size_t pixel=0;pixel<direct.pixels.size();pixel+=4)
          for(size_t channel=0;channel<4;++channel)EXPECT_EQ(direct.pixels[pixel+channel],expected[channel]);
      }
      unsigned int pixel_width = 0, pixel_height = 0;
      ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(image, &pixel_width, &pixel_height));
      EXPECT_EQ(pixel_width, width); EXPECT_EQ(pixel_height, height);
      Handle raw(api.FPDFImageObj_GetBitmap(image), api.FPDFBitmap_Destroy);
      ASSERT_NE(raw.get(), nullptr);
      EXPECT_EQ(api.FPDFBitmap_GetWidth(raw.get()), width);
      EXPECT_EQ(api.FPDFBitmap_GetHeight(raw.get()), height);
      Handle standalone(api.FPDFImageObj_GetRenderedBitmap(doc.get(), nullptr, image), api.FPDFBitmap_Destroy);
      ASSERT_NE(standalone.get(), nullptr);
      EXPECT_EQ(api.FPDFBitmap_GetWidth(standalone.get()), 48);
      EXPECT_EQ(api.FPDFBitmap_GetHeight(standalone.get()), 32);
      pictures[backend++] = Render(api, page.get(), 128, 128);
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*writer, doc.get(), page.get(), artifacts,
          writer->name() + "-bitmap-image-" + std::to_string(format), [](const Api& read, FPDF_PAGE loaded) {
            ASSERT_EQ(read.FPDFPage_CountObjects(loaded), 1);
            auto image = read.FPDFPage_GetObject(loaded, 0);
            unsigned int w = 0, h = 0;
            ASSERT_TRUE(read.FPDFImageObj_GetImagePixelSize(image, &w, &h)); EXPECT_EQ(w, 3u); EXPECT_EQ(h, 2u);
          }, 1));
      bitmap.reset(); // external allocation remains caller-owned after destroy.
      EXPECT_TRUE(std::all_of(storage.end() - 16, storage.end(), [](auto c) { return c == 0xa5; }));
    }
    CompareRaster(pictures[0], pictures[1], artifacts, "bitmap-image-format" + std::to_string(format), 1);
  }
}

TEST(Images, ImageMatricesRawAndRenderedBitmapsExposeDifferentGeometry) {
  Artifacts artifacts;
  const auto fixture = ImageFixture("/DeviceRGB", std::string("\xff\0\0\0\xff\0\0\0\xff\xff\xff\0", 12));
  const std::array<FS_MATRIX, 5> matrices = {{{1, 0, 0, 1, 0, 0}, {32, 0, 0, 16, 16, 32},
      {0, 32, -16, 0, 80, 16}, {24, 8, 12, 20, 16, 16}, {-32, 0, 0, 16, 64, 32}}};
  for (size_t index = 0; index < matrices.size(); ++index) {
    SCOPED_TRACE(index);
    std::array<Raster, 2> raw_pictures, rendered_pictures, pages;
    size_t backend = 0;
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name());
      const auto& api = writer->api();
      Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
      Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      auto image = api.FPDFPage_GetObject(page.get(), 0); ASSERT_NE(image, nullptr);
      const auto& m = matrices[index];
      ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image, m.a, m.b, m.c, m.d, m.e, m.f));
      FS_MATRIX actual{}; ASSERT_TRUE(api.FPDFPageObj_GetMatrix(image, &actual));
      EXPECT_FLOAT_EQ(actual.a, m.a); EXPECT_FLOAT_EQ(actual.b, m.b); EXPECT_FLOAT_EQ(actual.c, m.c);
      EXPECT_FLOAT_EQ(actual.d, m.d); EXPECT_FLOAT_EQ(actual.e, m.e); EXPECT_FLOAT_EQ(actual.f, m.f);
      Handle raw(api.FPDFImageObj_GetBitmap(image), api.FPDFBitmap_Destroy);
      ASSERT_NE(raw.get(), nullptr);
      raw_pictures[backend] = CopyImageBitmap(api, raw.get());
      EXPECT_EQ(raw_pictures[backend].width, 2); EXPECT_EQ(raw_pictures[backend].height, 2);
      Handle rendered(api.FPDFImageObj_GetRenderedBitmap(doc.get(), page.get(), image), api.FPDFBitmap_Destroy);
      ASSERT_NE(rendered.get(), nullptr);
      rendered_pictures[backend] = CopyImageBitmap(api, rendered.get());
      EXPECT_EQ(rendered_pictures[backend].width, static_cast<int>(std::ceil(std::hypot(m.a, m.c))));
      EXPECT_EQ(rendered_pictures[backend].height, static_cast<int>(std::ceil(std::hypot(m.b, m.d))));
      FPDF_IMAGEOBJ_METADATA metadata{};
      ASSERT_TRUE(api.FPDFImageObj_GetImageMetadata(image, page.get(), &metadata));
      EXPECT_EQ(metadata.width, 2u); EXPECT_EQ(metadata.height, 2u); EXPECT_EQ(metadata.marked_content_id, 7);
      const auto bounds = editing::Bounds(api, image);
      if (bounds[2] != bounds[0] && bounds[3] != bounds[1]) {
        EXPECT_NEAR(metadata.horizontal_dpi, 144 / (bounds[2] - bounds[0]), .001f);
        EXPECT_NEAR(metadata.vertical_dpi, 144 / (bounds[3] - bounds[1]), .001f);
      }
      FPDF_IMAGEOBJ_METADATA absent{};
      ASSERT_TRUE(api.FPDFImageObj_GetImageMetadata(image, nullptr, &absent));
      EXPECT_EQ(absent.colorspace, FPDF_COLORSPACE_UNKNOWN); EXPECT_EQ(absent.bits_per_pixel, 0u);
      EXPECT_FALSE(api.FPDFImageObj_GetImageMetadata(image, page.get(), nullptr));
      pages[backend++] = Render(api, page.get(), 128, 128);
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*writer, doc.get(), page.get(), artifacts,
          writer->name() + "-image-matrix" + std::to_string(index), [m](const Api& read, FPDF_PAGE loaded) {
            auto object = read.FPDFPage_GetObject(loaded, 0);
            FS_MATRIX actual{}; ASSERT_TRUE(read.FPDFPageObj_GetMatrix(object, &actual));
            EXPECT_FLOAT_EQ(actual.a, m.a); EXPECT_FLOAT_EQ(actual.b, m.b); EXPECT_FLOAT_EQ(actual.c, m.c);
            EXPECT_FLOAT_EQ(actual.d, m.d); EXPECT_FLOAT_EQ(actual.e, m.e); EXPECT_FLOAT_EQ(actual.f, m.f);
          }, 1));
    }
    CompareRaster(raw_pictures[0], raw_pictures[1], artifacts, "raw-image" + std::to_string(index));
    CompareRaster(rendered_pictures[0], rendered_pictures[1], artifacts, "rendered-image" + std::to_string(index), 1);
    CompareRaster(pages[0], pages[1], artifacts, "image-page" + std::to_string(index), 1);
  }
}

TEST(Images, RawDecodedFiltersAndBufferUnitsPreserveDataAndPipelineOrder) {
  Artifacts artifacts;
  const std::string raw("\xff\0\0\0\xff\0\0\0\xff\xff\xff\0", 12);
  const std::vector<uint8_t> expected(raw.begin(), raw.end());
  // A literal zlib stream for the 12 samples above, serialized as ASCIIHex for
  // the two-filter fixture. Both are fixture bytes, not output snapshots.
  const std::string hex = "789CFBCFC0C0F01F84FFFF6700001CEF04FC>";
  for (int filters = 0; filters < 2; ++filters) {
    const auto fixture = ImageFixture("/DeviceRGB", filters ? hex : raw,
        filters ? "/Filter [/ASCIIHexDecode /FlateDecode]" : "");
    std::array<Raster, 2> pictures;
    size_t backend = 0;
    for (const auto* library : Implementations()) {
      const auto& api = library->api();
      Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
      Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      auto image = api.FPDFPage_GetObject(page.get(), 0); ASSERT_NE(image, nullptr);
      const std::vector<uint8_t> source(filters ? hex.begin() : raw.begin(), filters ? hex.end() : raw.end());
      EXPECT_EQ(ImageData(*library, image, false, &source), source);
      EXPECT_EQ(ImageData(*library, image, true, &expected), expected);
      EXPECT_EQ(Filters(api, image), filters ? (std::vector<std::string>{"ASCIIHexDecode", "FlateDecode"}) : std::vector<std::string>{});
      pictures[backend++] = Render(api, page.get(), 128, 128);
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*library, doc.get(), page.get(), artifacts,
          library->name() + "-filters" + std::to_string(filters), [expected](const Api& read, FPDF_PAGE loaded) {
            auto image = read.FPDFPage_GetObject(loaded, 0);
            // Saving may recompress unfiltered streams; decoded samples are the contract.
            Filters(read, image);
            std::vector<uint8_t> samples(expected.size());
            ASSERT_EQ(read.FPDFImageObj_GetImageDataDecoded(image,samples.data(),samples.size()),samples.size());
            EXPECT_EQ(samples,expected);
          }));
    }
    CompareRaster(pictures[0], pictures[1], artifacts, "filter-pipeline" + std::to_string(filters));
  }
}

struct JpegAccess {
  FPDF_FILEACCESS interface{};
  std::vector<uint8_t> bytes;
  size_t reads = 0;
  bool fail = false;
  bool valid_requests = true;
  explicit JpegAccess(std::vector<uint8_t> data) : bytes(std::move(data)) {
    interface.m_FileLen = static_cast<unsigned long>(bytes.size());
    interface.m_Param = this;
    interface.m_GetBlock = Read;
  }
  static int Read(void* context, unsigned long offset, unsigned char* destination,
                   unsigned long count) noexcept {
    auto* self = static_cast<JpegAccess*>(context);
    ++self->reads;
    const bool valid = count > 0 && offset <= self->bytes.size() && count <= self->bytes.size() - offset;
    self->valid_requests &= valid;
    if (self->fail || !valid) return 0;
    std::memcpy(destination, self->bytes.data() + offset, count);
    return 1;
  }
};

TEST(Images, InlineAndDeferredJpegCallbacksSupportGrayRgbCmykAndReplacement) {
  Artifacts artifacts;
  struct Source { const char* file; unsigned int width; unsigned int height; };
  const std::array<Source, 3> sources = {{{"crates/pdfoxium-core/tests/fixtures/gray.jpg", 16, 1},
      {"upstream/pdfium/testing/resources/mona_lisa.jpg", 120, 120},
      {"crates/pdfoxium-core/tests/fixtures/cmyk.jpg", 16, 1}}};
  for (bool inline_image : {false, true}) for (const auto& source : sources) {
    const auto original = ReadImageFile(TestRoot() / source.file); ASSERT_FALSE(original.empty());
    std::array<Raster, 2> pictures;
    size_t backend = 0;
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name());
      const auto& api = writer->api();
      JpegAccess access(original); // Outlives all deferred decoder/save access.
      JpegAccess replacement(ReadImageFile(TestRoot() / "crates/pdfoxium-core/tests/fixtures/gray.jpg"));
      Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
      Handle second(api.FPDFPage_New(doc.get(), 1, 128, 128), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr); ASSERT_NE(second.get(), nullptr);
      auto image = api.FPDFPageObj_NewImageObj(doc.get()); ASSERT_NE(image, nullptr);
      const auto load = inline_image ? api.FPDFImageObj_LoadJpegFileInline : api.FPDFImageObj_LoadJpegFile;
      ASSERT_TRUE(load(nullptr, 0, image, &access.interface));
      EXPECT_GT(access.reads, 0u); EXPECT_TRUE(access.valid_requests);
      if (inline_image) {
        // Inline form copies bytes; make any accidental later callback fail.
        access.fail = true; access.bytes.clear(); access.bytes.shrink_to_fit();
      }
      ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image, 96, 0, 0, 80, 16, 24));
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), image));
      unsigned int width = 0, height = 0;
      ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(image, &width, &height));
      EXPECT_EQ(width, source.width); EXPECT_EQ(height, source.height);
      EXPECT_EQ(Filters(api, image), (std::vector<std::string>{"DCTDecode"}));
      EXPECT_EQ(ImageData(*writer, image, false, &original), original);
      // DCT is a deferred image filter: decoded-stream access still exposes
      // the JPEG codestream, while GetBitmap performs the pixel decoding.
      EXPECT_EQ(ImageData(*writer, image, true, &original), original);
      pictures[backend] = Render(api, page.get(), 128, 128, FPDF_RENDER_NO_SMOOTHIMAGE);
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*writer, doc.get(), page.get(), artifacts,
          writer->name() + "-jpeg-" + std::to_string(inline_image) + "-" + std::to_string(source.width) + "-" + std::to_string(original.size()),
          [source](const Api& read, FPDF_PAGE loaded) {
            auto image = read.FPDFPage_GetObject(loaded, 0);
            unsigned int w = 0, h = 0;
            ASSERT_TRUE(read.FPDFImageObj_GetImagePixelSize(image, &w, &h));
            EXPECT_EQ(w, source.width); EXPECT_EQ(h, source.height);
          }, 2));
      // Replacement supplies every affected loaded page, after caching pixels.
      FPDF_PAGE pages[] = {page.get(), second.get()};
      ASSERT_TRUE(load(pages, 2, image, &replacement.interface));
      ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(image, &width, &height));
      EXPECT_EQ(width, 16u); EXPECT_EQ(height, 1u);
      const auto replaced = Render(api, page.get(), 128, 128, FPDF_RENDER_NO_SMOOTHIMAGE);
      if (source.width != 16 || std::string(source.file) == sources[2].file) { EXPECT_NE(replaced.pixels, pictures[backend].pixels); }
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*writer,doc.get(),page.get(),artifacts,
          writer->name()+"-jpeg-replaced-"+std::to_string(inline_image)+"-"+std::to_string(original.size()),
          [](const Api& read,FPDF_PAGE loaded) {
            auto image=read.FPDFPage_GetObject(loaded,0);
            unsigned int w=0,h=0;
            ASSERT_TRUE(read.FPDFImageObj_GetImagePixelSize(image,&w,&h));
            EXPECT_EQ(w,16u); EXPECT_EQ(h,1u);
            EXPECT_EQ(Filters(read,image),(std::vector<std::string>{"DCTDecode"}));
          },2));
      ++backend;
    }
    // Decoder/color conversion differences are bounded to two channel levels;
    // no smoothing keeps JPEG interpolation out of this observation.
    CompareRaster(pictures[0], pictures[1], artifacts, "jpeg" + std::to_string(inline_image) + "-" + std::to_string(original.size()), 2);
  }
}

TEST(Images, SharedImageReplacementInvalidatesEverySuppliedLoadedPage) {
  Artifacts artifacts;
  // Public insertion transfers a page object rather than sharing it. These two
  // pages reference one image resource, as real PDF resource dictionaries do.
  const auto fixture=PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << /XObject << /I 7 0 R >> >> /Contents 5 0 R >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << /XObject << /I 7 0 R >> >> /Contents 6 0 R >>",
      PdfStream("","q 96 0 0 80 16 24 cm /I Do Q"),
      PdfStream("","q 96 0 0 80 16 24 cm /I Do Q"),
      PdfStream("/Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceRGB",
                std::string("\xff\0\0\0\xff\0\0\0\xff\xff\xff\0",12))});
  for (bool inline_image : {false,true}) {
    std::array<Raster,2> results;
    size_t backend=0;
    for (const auto* writer:Implementations()) {
      SCOPED_TRACE(writer->name()+"-inline"+std::to_string(inline_image));
      const auto& api=writer->api();
      JpegAccess initial(ReadImageFile(TestRoot()/"crates/pdfoxium-core/tests/fixtures/gray.jpg"));
      JpegAccess middle(ReadImageFile(TestRoot()/"upstream/pdfium/testing/resources/mona_lisa.jpg"));
      JpegAccess last(ReadImageFile(TestRoot()/"crates/pdfoxium-core/tests/fixtures/gray.jpg"));
      Handle doc(api.FPDF_LoadMemDocument64(fixture.data(),fixture.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(doc.get(),nullptr);
      Handle first(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);
      Handle second(api.FPDF_LoadPage(doc.get(),1),api.FPDF_ClosePage);
      ASSERT_NE(first.get(),nullptr); ASSERT_NE(second.get(),nullptr);
      auto image=api.FPDFPage_GetObject(first.get(),0);
      auto other=api.FPDFPage_GetObject(second.get(),0);
      ASSERT_NE(image,nullptr); ASSERT_NE(other,nullptr);
      const auto load=inline_image?api.FPDFImageObj_LoadJpegFileInline:api.FPDFImageObj_LoadJpegFile;
      // No page has cached pixels yet: zero supplied pages is sufficient.
      ASSERT_TRUE(load(nullptr,0,image,&initial.interface));
      if(inline_image) { initial.fail=true; initial.bytes.clear(); }
      const auto gray=Render(api,first.get(),128,128);
      FPDF_PAGE pages[]={first.get(),second.get()};
      ASSERT_TRUE(load(pages,1,image,&middle.interface));
      if(inline_image) { middle.fail=true; middle.bytes.clear(); }
      const auto rgb=Render(api,first.get(),128,128);
      EXPECT_NE(gray.pixels,rgb.pixels);
      CompareRaster(rgb,Render(api,second.get(),128,128),artifacts,writer->name()+"-shared-middle");
      ASSERT_TRUE(load(pages,2,image,&last.interface));
      // Cache invalidation pages do not transfer/edit their page objects.
      // Explicitly update both owned objects before requiring both saved
      // resource references to point at the replacement image stream.
      ASSERT_TRUE(load(pages,2,other,&last.interface));
      if(inline_image) { last.fail=true; last.bytes.clear(); }
      const auto replaced=Render(api,first.get(),128,128);
      CompareRaster(gray,replaced,artifacts,writer->name()+"-shared-last");
      CompareRaster(replaced,Render(api,second.get(),128,128),artifacts,writer->name()+"-shared-last-page2");
      unsigned int width=0,height=0;
      ASSERT_TRUE(api.FPDFImageObj_GetImagePixelSize(other,&width,&height));
      EXPECT_EQ(width,16u); EXPECT_EQ(height,1u);
      ASSERT_TRUE(api.FPDFPage_GenerateContent(first.get()));
      ASSERT_TRUE(api.FPDFPage_GenerateContent(second.get()));
      const auto bytes=SaveDocument(api,doc.get(),artifacts,writer->name()+"-shared-jpeg-"+std::to_string(inline_image));
      for(const auto* reader:Implementations()) {
        const auto& read=reader->api();
        Handle loaded(read.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),read.FPDF_CloseDocument);
        ASSERT_NE(loaded.get(),nullptr);
        for(int index=0;index<2;++index) {
          Handle page(read.FPDF_LoadPage(loaded.get(),index),read.FPDF_ClosePage);
          ASSERT_NE(page.get(),nullptr);
          CompareRaster(replaced,Render(read,page.get(),128,128),artifacts,
                        writer->name()+"-shared-to-"+reader->name()+std::to_string(index),2);
        }
      }
      results[backend++]=replaced;
    }
    CompareRaster(results[0],results[1],artifacts,"shared-jpeg-backends"+std::to_string(inline_image),2);
  }
}

TEST(Images, JpegMalformedAndReadFailureNeverInventRenderablePixels) {
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    for (bool inline_image : {false, true}) {
      const auto load = inline_image ? api.FPDFImageObj_LoadJpegFileInline : api.FPDFImageObj_LoadJpegFile;
      for (bool failed_read : {false, true}) {
        JpegAccess invalid({0xff, 0xd8, 0, 1, 2, 3, 4, 5}); invalid.fail = failed_read;
        Handle image(api.FPDFPageObj_NewImageObj(doc.get()), api.FPDFPageObj_Destroy);
        ASSERT_NE(image.get(), nullptr);
        // Native assignment does not report decoder failure in its Boolean;
        // the pixel query proves malformed bytes did not become a real image.
        (void)load(nullptr, 0, image.get(), &invalid.interface);
        EXPECT_EQ(api.FPDFImageObj_GetBitmap(image.get()), nullptr);
        EXPECT_FALSE(load(nullptr, 0, image.get(), nullptr));
        EXPECT_FALSE(load(nullptr, 0, nullptr, &invalid.interface));
      }
    }
  }
}

TEST(Images, FractionalLabSpotTintsUseTheEmbeddedProfile) {
  // Original ICC v4 Lab->Lab identity profile: D50 wtpt and three identity
  // B curves in an mAB A2B0 tag. No external profile or generated binary.
  std::string profile(244, '\0');
  const auto word = [&](size_t offset, uint32_t value) {
    for (int i = 0; i < 4; ++i) profile[offset+i] = static_cast<char>(value >> (24-8*i));
  };
  const auto signature = [&](size_t offset, const char* text) {
    profile.replace(offset, 4, text, 4);
  };
  word(0, profile.size()); word(8, 0x04300000);
  signature(12, "spac"); signature(16, "Lab "); signature(20, "Lab ");
  signature(36, "acsp");
  word(68, 0x0000f6d6); word(72, 0x00010000); word(76, 0x0000d32d);
  word(128, 2);
  signature(132, "wtpt"); word(136, 156); word(140, 20);
  signature(144, "A2B0"); word(148, 176); word(152, 68);
  signature(156, "XYZ ");
  word(164, 0x0000f6d6); word(168, 0x00010000); word(172, 0x0000d32d);
  signature(176, "mAB "); profile[184] = 3; profile[185] = 3; word(188, 32);
  for (size_t offset : {208, 220, 232}) signature(offset, "curv");

  const std::string alternate = "[/Lab << /WhitePoint [0.964203 1 0.824905] /Range [-128 127 -128 127] >>]";
  const std::string tint = " << /FunctionType 2 /Domain [0 1] /C0 [100 0 0] /C1 [34.1176 -2 -64] /N 1 /Range [0 100 -128 127 -128 127] >>]";
  const std::string samples = {1, 64, static_cast<char>(128), 0};
  const auto bytes = ImageFixture("[/Separation /Blue [/ICCBased 6 0 R]" + tint,
      samples, "", {PdfStream("/N 3 /Alternate " + alternate, profile)});
  const auto alternate_bytes = ImageFixture("[/Separation /Blue " + alternate + tint, samples);
  // Independent CIE Lab/D50 -> Bradford D65 -> sRGB, rounded to bytes.
  // The pinned native transform rejects Lab input and uses its approximate
  // alternate conversion instead; assert that observation separately.
  const std::array<std::array<uint8_t,3>,4> correct = {{{254,254,255}, {195,209,238}, {132,165,221}, {255,255,255}}};
  const std::array<std::array<uint8_t,3>,4> fallback = {{{255,254,255}, {192,209,238}, {122,166,220}, {255,255,255}}};
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    Handle bitmap(api.FPDFImageObj_GetBitmap(api.FPDFPage_GetObject(page.get(), 0)), api.FPDFBitmap_Destroy);
    ASSERT_NE(bitmap.get(), nullptr);
    const auto raster = CopyImageBitmap(api, bitmap.get());
    ASSERT_EQ(raster.width, 2); ASSERT_EQ(raster.height, 2);
    const auto& expected = library->name() == "rust" ? correct : fallback;
    for (size_t i = 0; i < 4; ++i) for (size_t channel = 0; channel < 3; ++channel)
      EXPECT_EQ(raster.pixels[4*i + 2-channel], expected[i][channel]) << i << "," << channel;
    const auto painted = Render(api, page.get(), 128, 128, FPDF_RENDER_NO_SMOOTHIMAGE);
    for (size_t i = 0; i < 4; ++i) for (size_t channel = 0; channel < 3; ++channel) {
      const size_t pixel = (84 + 8*(i/2))*128 + 24 + 16*(i%2);
      EXPECT_EQ(painted.pixels[4*pixel + 2-channel], expected[i][channel]) << "paint " << i << "," << channel;
    }
    if (library->name() == "native") {
      Handle control_doc(api.FPDF_LoadMemDocument64(alternate_bytes.data(), alternate_bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(control_doc.get(), nullptr);
      Handle control_page(api.FPDF_LoadPage(control_doc.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(control_page.get(), nullptr);
      Handle control(api.FPDFImageObj_GetBitmap(api.FPDFPage_GetObject(control_page.get(), 0)), api.FPDFBitmap_Destroy);
      ASSERT_NE(control.get(), nullptr);
      EXPECT_EQ(raster.pixels, CopyImageBitmap(api, control.get()).pixels);
    }
  }
}

TEST(Images, MetadataEnumeratesApplicableColorspaceFamiliesAndIccPayloads) {
  Artifacts artifacts;
  const auto rgb_profile = ReadImageFile(TestRoot() / "third_party/hayro-jpeg2000/assets/ProPhoto-v2-micro.icc");
  const auto cmyk_profile = ReadImageFile(TestRoot() / "third_party/hayro-jpeg2000/assets/CGATS001Compat-v2-micro.icc");
  ASSERT_FALSE(rgb_profile.empty()); ASSERT_FALSE(cmyk_profile.empty());
  struct Color { std::string name; int family; int components; std::vector<std::string> extra; const std::vector<uint8_t>* profile; };
  const std::vector<Color> colors = {
      {"/DeviceGray", FPDF_COLORSPACE_DEVICEGRAY, 1, {}, nullptr},
      {"/DeviceRGB", FPDF_COLORSPACE_DEVICERGB, 3, {}, nullptr},
      {"/DeviceCMYK", FPDF_COLORSPACE_DEVICECMYK, 4, {}, nullptr},
      {"[/CalGray << /WhitePoint [0.9505 1 1.089] /Gamma 2.2 >>]", FPDF_COLORSPACE_CALGRAY, 1, {}, nullptr},
      {"[/CalRGB << /WhitePoint [0.9505 1 1.089] /Gamma [2.2 2.2 2.2] >>]", FPDF_COLORSPACE_CALRGB, 3, {}, nullptr},
      {"[/Lab << /WhitePoint [0.9505 1 1.089] /Range [-100 100 -100 100] >>]", FPDF_COLORSPACE_LAB, 3, {}, nullptr},
      {"[/Indexed /DeviceRGB 1 <ff000000ff00>]", FPDF_COLORSPACE_INDEXED, 1, {}, nullptr},
      {"[/Separation /Spot /DeviceRGB << /FunctionType 2 /Domain [0 1] /C0 [1 1 1] /C1 [0 0 1] /N 1 >>]", FPDF_COLORSPACE_SEPARATION, 1, {}, nullptr},
      {"[/DeviceN [/A /B] /DeviceRGB 6 0 R]", FPDF_COLORSPACE_DEVICEN, 2,
       {PdfStream("/FunctionType 4 /Domain [0 1 0 1] /Range [0 1 0 1 0 1]", "{ pop 0 0 }")}, nullptr},
      {"[/ICCBased 6 0 R]", FPDF_COLORSPACE_ICCBASED, 3,
       {PdfStream("/N 3 /Alternate /DeviceRGB", std::string(rgb_profile.begin(), rgb_profile.end()))}, &rgb_profile},
      {"[/ICCBased 6 0 R]", FPDF_COLORSPACE_ICCBASED, 4,
       {PdfStream("/N 4 /Alternate /DeviceCMYK", std::string(cmyk_profile.begin(), cmyk_profile.end()))}, &cmyk_profile},
      {"/UnknownColorSpace", FPDF_COLORSPACE_UNKNOWN, 1, {}, nullptr}};
  // Pattern is a non-image colorspace, intentionally not fabricated as an image.
  for (size_t color_index = 0; color_index < colors.size(); ++color_index) {
    const auto& color = colors[color_index];
    std::string samples(color.components * 4, '\0');
    for (size_t i = 0; i < samples.size(); ++i) samples[i] = static_cast<char>((i * 47 + 17) & 0xff);
    if (color.family == FPDF_COLORSPACE_INDEXED) samples = std::string("\0\1\0\1", 4);
    const auto fixture = ImageFixture(color.name, samples, "", color.extra);
    std::array<Raster, 2> pictures;
    std::array<FPDF_IMAGEOBJ_METADATA, 2> metadata{};
    size_t backend = 0;
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name() + "-color" + std::to_string(color_index));
      SCOPED_TRACE(writer->name());
      const auto& api = writer->api();
      Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
      Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      auto image = api.FPDFPage_GetObject(page.get(), 0); ASSERT_NE(image, nullptr);
      auto& m = metadata[backend];
      ASSERT_TRUE(api.FPDFImageObj_GetImageMetadata(image, page.get(), &m));
      EXPECT_EQ(m.width, 2u); EXPECT_EQ(m.height, 2u); EXPECT_EQ(m.marked_content_id, 7);
      EXPECT_FLOAT_EQ(m.horizontal_dpi, 4.5f); EXPECT_FLOAT_EQ(m.vertical_dpi, 9.0f);
      EXPECT_EQ(m.colorspace, color.family);
      if (color.family == FPDF_COLORSPACE_UNKNOWN) { EXPECT_EQ(m.bits_per_pixel, 0u); }
      else { EXPECT_TRUE(m.bits_per_pixel == 8 || m.bits_per_pixel == 24 || m.bits_per_pixel == 32); }
      size_t needed = 991;
      const auto has_icc = api.FPDFImageObj_GetIccProfileDataDecoded(image, page.get(), nullptr, 0, &needed);
      if (color.profile) {
        ASSERT_TRUE(has_icc); EXPECT_EQ(needed, color.profile->size());
        std::vector<uint8_t> output(needed + 3, 0xa5);
        if (needed) {
          EXPECT_TRUE(api.FPDFImageObj_GetIccProfileDataDecoded(image, page.get(), output.data(), needed - 1, &needed));
          EXPECT_TRUE(std::all_of(output.begin(), output.end(), [](auto c) { return c == 0xa5; }));
        }
        EXPECT_TRUE(api.FPDFImageObj_GetIccProfileDataDecoded(image, page.get(), output.data(), needed, &needed));
        EXPECT_TRUE(std::equal(color.profile->begin(), color.profile->end(), output.begin())); EXPECT_EQ(output[needed], 0xa5);
        EXPECT_TRUE(api.FPDFImageObj_GetIccProfileDataDecoded(image, page.get(), output.data(), output.size(), &needed));
        EXPECT_EQ(output[needed], 0xa5);
        EXPECT_FALSE(api.FPDFImageObj_GetIccProfileDataDecoded(image, page.get(), nullptr, 0, nullptr));
        EXPECT_FALSE(api.FPDFImageObj_GetIccProfileDataDecoded(image, nullptr, nullptr, 0, &needed));
      } else {
        EXPECT_FALSE(has_icc); EXPECT_EQ(needed, 991u);
      }
      pictures[backend++] = Render(api, page.get(), 128, 128, FPDF_RENDER_NO_SMOOTHIMAGE);
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*writer, doc.get(), page.get(), artifacts,
          writer->name() + "-colorspace" + std::to_string(color_index), [color](const Api& read, FPDF_PAGE p) {
            FPDF_IMAGEOBJ_METADATA loaded{};
            ASSERT_TRUE(read.FPDFImageObj_GetImageMetadata(read.FPDFPage_GetObject(p, 0), p, &loaded));
            EXPECT_EQ(loaded.width, 2u); EXPECT_EQ(loaded.height, 2u); EXPECT_EQ(loaded.colorspace, color.family);
            EXPECT_FLOAT_EQ(loaded.horizontal_dpi,4.5f); EXPECT_FLOAT_EQ(loaded.vertical_dpi,9.0f);
            EXPECT_EQ(loaded.marked_content_id,7);
            if (color.profile) {
              size_t size=0;
              ASSERT_TRUE(read.FPDFImageObj_GetIccProfileDataDecoded(read.FPDFPage_GetObject(p,0),p,nullptr,0,&size));
              ASSERT_EQ(size,color.profile->size());
              std::vector<uint8_t> profile(size);
              ASSERT_TRUE(read.FPDFImageObj_GetIccProfileDataDecoded(read.FPDFPage_GetObject(p,0),p,profile.data(),profile.size(),&size));
              EXPECT_EQ(profile,*color.profile);
            }
          }, 2));
    }
    EXPECT_EQ(metadata[0].bits_per_pixel, metadata[1].bits_per_pixel);
    CompareRaster(pictures[0], pictures[1], artifacts, "colorspace" + std::to_string(color_index), 2);
  }
}

TEST(Images, HardAndSoftMasksAffectRenderedBitmapButNotRawBitmapPixels) {
  Artifacts artifacts;
  for (bool soft : {false, true}) {
    const std::string samples("\xff\0\0\0\xff\0\0\0\xff\xff\xff\0", 12);
    const std::string mask = soft
        ? PdfStream("/Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceGray", std::string("\0\x40\x80\xff", 4))
        : PdfStream("/Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 1 /ImageMask true /Decode [0 1]", std::string("\x40\x80", 2));
    const auto fixture = ImageFixture("/DeviceRGB", samples, soft ? "/SMask 6 0 R" : "/Mask 6 0 R", {mask});
    std::array<Raster, 2> raw_pictures, masked_pictures, pages;
    std::array<std::array<bool,2>,2> transparency{};
    size_t backend = 0;
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name());
      const auto& api = writer->api();
      Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
      Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      auto image = api.FPDFPage_GetObject(page.get(), 0); ASSERT_NE(image, nullptr);
      // Reported page/object transparency is distinct from pixel alpha; retain
      // both observations while validating actual mask rendering below.
      const std::array<bool,2> reported{bool(api.FPDFPage_HasTransparency(page.get())),
                                      bool(api.FPDFPageObj_HasTransparency(image))};
      transparency[backend]=reported;
      Handle raw(api.FPDFImageObj_GetBitmap(image), api.FPDFBitmap_Destroy);
      Handle masked(api.FPDFImageObj_GetRenderedBitmap(doc.get(), page.get(), image), api.FPDFBitmap_Destroy);
      ASSERT_NE(raw.get(), nullptr); ASSERT_NE(masked.get(), nullptr);
      raw_pictures[backend] = CopyImageBitmap(api, raw.get());
      masked_pictures[backend] = CopyImageBitmap(api, masked.get());
      EXPECT_EQ(raw_pictures[backend].width, 2); EXPECT_EQ(raw_pictures[backend].height, 2);
      for (size_t index = 3; index < raw_pictures[backend].pixels.size(); index += 4) EXPECT_EQ(raw_pictures[backend].pixels[index], 255);
      EXPECT_EQ(masked_pictures[backend].width, 32); EXPECT_EQ(masked_pictures[backend].height, 16);
      bool saw_transparent = false, saw_opaque = false;
      for (size_t index = 3; index < masked_pictures[backend].pixels.size(); index += 4) {
        saw_transparent |= masked_pictures[backend].pixels[index] < 255;
        saw_opaque |= masked_pictures[backend].pixels[index] == 255;
      }
      EXPECT_TRUE(saw_transparent); EXPECT_TRUE(saw_opaque);
      pages[backend++] = Render(api, page.get(), 128, 128);
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*writer, doc.get(), page.get(), artifacts,
          writer->name() + "-mask-" + std::to_string(soft), [reported](const Api& read, FPDF_PAGE p) {
            auto image = read.FPDFPage_GetObject(p, 0);
            EXPECT_EQ(bool(read.FPDFPage_HasTransparency(p)),reported[0]);
            EXPECT_EQ(bool(read.FPDFPageObj_HasTransparency(image)),reported[1]);
            unsigned int w = 0, h = 0;
            ASSERT_TRUE(read.FPDFImageObj_GetImagePixelSize(image, &w, &h)); EXPECT_EQ(w, 2u); EXPECT_EQ(h, 2u);
          }, 1));
    }
    EXPECT_EQ(transparency[0],transparency[1]);
    CompareRaster(raw_pictures[0], raw_pictures[1], artifacts, "mask-raw" + std::to_string(soft));
    CompareRaster(masked_pictures[0], masked_pictures[1], artifacts, "mask-rendered" + std::to_string(soft), 1);
    CompareRaster(pages[0], pages[1], artifacts, "mask-page" + std::to_string(soft), 1);
  }
}

}  // namespace
}  // namespace pdfoxium::integration
