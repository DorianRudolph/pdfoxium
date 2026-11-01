#include "test_support.h"

#include <gtest/gtest.h>

#include <array>
#include <cstdint>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

void AddRectangle(const Api& api, FPDF_PAGE page, float left, float bottom,
                  unsigned int red, unsigned int green, unsigned int blue) {
  Handle object(api.FPDFPageObj_CreateNewRect(left, bottom, 24, 20),
                api.FPDFPageObj_Destroy);
  ASSERT_NE(object.get(), nullptr);
  ASSERT_TRUE(api.FPDFPageObj_SetFillColor(object.get(), red, green, blue, 255));
  ASSERT_TRUE(api.FPDFPath_SetDrawMode(object.get(), FPDF_FILLMODE_WINDING, 0));
  api.FPDFPage_InsertObject(page, object.release());
}

void CreateScene(const Api& api, FPDF_DOCUMENT document) {
  Handle page(api.FPDFPage_New(document, 0, 128, 128), api.FPDF_ClosePage);
  ASSERT_NE(page.get(), nullptr);
  ASSERT_NO_FATAL_FAILURE(AddRectangle(api, page.get(), 16, 16, 224, 32, 64));
  ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
}

void CheckSaved(const std::vector<uint8_t>& bytes, int version, int objects,
                const Raster& before, Artifacts& artifacts,
                const std::string& label) {
  ASSERT_FALSE(bytes.empty());
  std::array<Raster, 2> rasters;
  size_t index = 0;
  for (const auto* reader : Implementations()) {
    SCOPED_TRACE(label + "/reader=" + reader->name());
    const auto& api = reader->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr),
                    api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr) << "load error=" << api.FPDF_GetLastError();
    EXPECT_EQ(api.FPDF_GetPageCount(document.get()), 1);
    int actual_version = -1;
    ASSERT_TRUE(api.FPDF_GetFileVersion(document.get(), &actual_version));
    if (version) { EXPECT_EQ(actual_version, version); }
    EXPECT_GE(actual_version, 10);
    EXPECT_LE(actual_version, 17);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    EXPECT_DOUBLE_EQ(api.FPDF_GetPageWidth(page.get()), 128);
    EXPECT_DOUBLE_EQ(api.FPDF_GetPageHeight(page.get()), 128);
    EXPECT_EQ(api.FPDFPage_CountObjects(page.get()), objects);
    rasters[index] = Render(api, page.get(), 128, 128);
    CompareRaster(before, rasters[index], artifacts, label + "-" + reader->name());
    ++index;
  }
  CompareRaster(rasters[0], rasters[1], artifacts, label + "-readers");
}

TEST(Save, ExplicitVersionsAndFlagsReopenWithBothReaders) {
  Artifacts artifacts;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    for (int version : {10, 14, 15, 17}) {
      for (FPDF_DWORD flags : {FPDF_DWORD{0}, FPDF_DWORD{FPDF_INCREMENTAL},
                               FPDF_DWORD{FPDF_NO_INCREMENTAL}}) {
        const auto label = writer->name() + "-v" + std::to_string(version) +
                           "-flags" + std::to_string(flags);
        SCOPED_TRACE(label);
        Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
        ASSERT_NE(document.get(), nullptr);
        int untouched = -1;
        EXPECT_FALSE(api.FPDF_GetFileVersion(document.get(), &untouched));
        ASSERT_NO_FATAL_FAILURE(CreateScene(api, document.get()));
        Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
        ASSERT_NE(page.get(), nullptr);
        const auto before = Render(api, page.get(), 128, 128);
        page.reset();
        const auto bytes = SaveDocument(api, document.get(), artifacts, label, flags, version);
        ASSERT_NO_FATAL_FAILURE(CheckSaved(bytes, version, 1, before, artifacts, label));
      }
    }
  }
}

TEST(Save, NewDocumentsSupportEachFlagAndRepeatedSinks) {
  Artifacts artifacts;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(CreateScene(api, document.get()));
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    const auto before = Render(api, page.get(), 128, 128);
    page.reset();
    for (FPDF_DWORD flags : {FPDF_DWORD{0}, FPDF_DWORD{FPDF_INCREMENTAL},
                             FPDF_DWORD{FPDF_NO_INCREMENTAL}}) {
      for (int repetition = 0; repetition < 2; ++repetition) {
        const auto label = writer->name() + "-flags" + std::to_string(flags) +
                           "-repeat" + std::to_string(repetition);
        SCOPED_TRACE(label);
        const auto bytes = SaveDocument(api, document.get(), artifacts, label, flags);
        ASSERT_NO_FATAL_FAILURE(CheckSaved(bytes, 0, 1, before, artifacts, label));
      }
    }
  }
}

TEST(Save, LoadedEditedDocumentsSupportFullAndIncrementalOutput) {
  Artifacts artifacts;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    Handle original(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(original.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(CreateScene(api, original.get()));
    const auto base = SaveDocument(api, original.get(), artifacts,
                                   writer->name() + "-base", FPDF_NO_INCREMENTAL, 14);
    ASSERT_FALSE(base.empty());
    original.reset();
    for (FPDF_DWORD flags : {FPDF_DWORD{0}, FPDF_DWORD{FPDF_INCREMENTAL},
                             FPDF_DWORD{FPDF_NO_INCREMENTAL}}) {
      const auto label = writer->name() + "-loaded-flags" + std::to_string(flags);
      SCOPED_TRACE(label);
      Handle document(api.FPDF_LoadMemDocument64(base.data(), base.size(), nullptr),
                      api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(AddRectangle(api, page.get(), 64, 64, 32, 96, 224));
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      EXPECT_EQ(api.FPDFPage_CountObjects(page.get()), 2);
      const auto before = Render(api, page.get(), 128, 128);
      page.reset();
      const auto bytes = SaveDocument(api, document.get(), artifacts, label, flags);
      ASSERT_NO_FATAL_FAILURE(CheckSaved(bytes, 14, 2, before, artifacts, label));
    }
  }
}

void CreateLargeImageScene(const Api& api, FPDF_DOCUMENT document) {
  Handle page(api.FPDFPage_New(document, 0, 128, 128), api.FPDF_ClosePage);
  ASSERT_NE(page.get(), nullptr);
  Handle bitmap(api.FPDFBitmap_Create(128, 128, 1), api.FPDFBitmap_Destroy);
  ASSERT_NE(bitmap.get(), nullptr);
  auto* data = static_cast<uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
  ASSERT_NE(data, nullptr);
  const int stride = api.FPDFBitmap_GetStride(bitmap.get());
  uint32_t random = 0x1a2b3c4d;
  for (int y = 0; y < 128; ++y) {
    for (int x = 0; x < 128; ++x) {
      for (int channel = 0; channel < 3; ++channel) {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        data[y * stride + x * 4 + channel] = static_cast<uint8_t>(random);
      }
      data[y * stride + x * 4 + 3] = 255;
    }
  }
  // Two distinct incompressible image streams exceed two ordinary archive
  // buffers. Failure on callback 2 therefore targets a checked write, rather
  // than making any assumption about final-flush error handling.
  for (int copy = 0; copy < 2; ++copy) {
    data[0] ^= 0x55;
    Handle image(api.FPDFPageObj_NewImageObj(document), api.FPDFPageObj_Destroy);
    ASSERT_NE(image.get(), nullptr);
    FPDF_PAGE pages[] = {page.get()};
    ASSERT_TRUE(api.FPDFImageObj_SetBitmap(pages, 1, image.get(), bitmap.get()));
    ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image.get(), 128, 0, 0, 128, 0, 0));
    api.FPDFPage_InsertObject(page.get(), image.release());
  }
  ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
}

struct ControlledWriter : FPDF_FILEWRITE {
  ControlledWriter(const Api& source, FPDF_DOCUMENT document, size_t fail_on)
      : api(source), document(document), fail_on(fail_on) {
    version = 1;
    WriteBlock = Write;
  }
  static int Write(FPDF_FILEWRITE* descriptor, const void* data,
                   unsigned long size) noexcept {
    auto* self = static_cast<ControlledWriter*>(descriptor);
    try {
      ++self->calls;
      self->valid_blocks &= size > 0 && data != nullptr;
      self->queried_page_count &= self->api.FPDF_GetPageCount(self->document) == 1;
      if (self->fail_on && self->calls == self->fail_on) return 0;
      if (size) {
        const auto* start = static_cast<const uint8_t*>(data);
        self->bytes.insert(self->bytes.end(), start, start + size);
      }
      return 1;
    } catch (...) {
      self->exception = true;
      return 0;
    }
  }
  const Api& api;
  FPDF_DOCUMENT document;
  size_t fail_on;
  size_t calls = 0;
  bool valid_blocks = true;
  bool queried_page_count = true;
  bool exception = false;
  std::vector<uint8_t> bytes;
};

TEST(Save, CallbackIdentityReentryAndCopiedBlocksProduceReadableOutput) {
  Artifacts artifacts;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    const auto& api = writer->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(CreateLargeImageScene(api, document.get()));
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    const auto before = Render(api, page.get(), 128, 128);
    page.reset();
    for (int repetition = 0; repetition < 2; ++repetition) {
      ControlledWriter sink(api, document.get(), 0);
      ASSERT_TRUE(api.FPDF_SaveAsCopy(document.get(), &sink, FPDF_NO_INCREMENTAL));
      EXPECT_GT(sink.calls, 2u);  // The image forces several checked writer blocks.
      EXPECT_TRUE(sink.valid_blocks);
      EXPECT_TRUE(sink.queried_page_count);
      EXPECT_FALSE(sink.exception);
      const auto label = writer->name() + "-callback-repeat" + std::to_string(repetition);
      artifacts.Pdf(label, sink.bytes);
      ASSERT_NO_FATAL_FAILURE(CheckSaved(sink.bytes, 0, 2, before, artifacts, label));
    }
  }
}

TEST(Save, FirstAndLaterWriteFailuresPermitRetryWithFreshSink) {
  Artifacts artifacts;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(CreateLargeImageScene(api, document.get()));
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    const auto before = Render(api, page.get(), 128, 128);
    page.reset();
    for (bool explicit_version : {false, true}) {
      for (size_t fail_on : {size_t{1}, size_t{2}}) {
        const auto label = writer->name() + "-failure" + std::to_string(fail_on) +
                           (explicit_version ? "-version" : "-copy");
        SCOPED_TRACE(label);
        ControlledWriter failed(api, document.get(), fail_on);
        const auto result = explicit_version
            ? api.FPDF_SaveWithVersion(document.get(), &failed, FPDF_NO_INCREMENTAL, 15)
            : api.FPDF_SaveAsCopy(document.get(), &failed, FPDF_NO_INCREMENTAL);
        EXPECT_FALSE(result);
        EXPECT_GE(failed.calls, fail_on);
        EXPECT_TRUE(failed.queried_page_count);
        EXPECT_FALSE(failed.exception);
        const auto bytes = SaveDocument(api, document.get(), artifacts, label + "-retry",
                                        FPDF_NO_INCREMENTAL, explicit_version ? 15 : 0);
        ASSERT_NO_FATAL_FAILURE(CheckSaved(bytes, explicit_version ? 15 : 0, 2,
                                           before, artifacts, label));
      }
    }
  }
}

TEST(Save, NullDocumentFailsWithoutCallingLiveWriter) {
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    MemoryWriter sink;
    EXPECT_FALSE(api.FPDF_SaveAsCopy(nullptr, &sink, 0));
    EXPECT_FALSE(api.FPDF_SaveWithVersion(nullptr, &sink, 0, 14));
    EXPECT_EQ(sink.calls, 0u);
    EXPECT_TRUE(sink.bytes.empty());
  }
}

}  // namespace
}  // namespace pdfoxium::integration
