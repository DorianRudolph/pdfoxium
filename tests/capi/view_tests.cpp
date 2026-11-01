#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cstring>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

// Neither legacy destination dictionaries nor name trees have public writers.
// Distinct page colors make the resolved target observable through rendering.
std::vector<uint8_t> DestinationFixture(bool tree, bool legacy) {
  const std::string catalog = "<< /Type /Catalog /Pages 2 0 R " +
      std::string(tree ? "/Names << /Dests << /Names [(Alpha) [3 0 R /XYZ 10 20 1.5] <feff03a9> << /D [4 0 R /Fit] >>] >> >> " : "") +
      (legacy ? "/Dests << /Legacy [4 0 R /Fit] >> " : "") + ">>";
  return PdfFixture({catalog,
      "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Contents 5 0 R >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Contents 6 0 R >>",
      PdfStream("", "1 0 0 rg 8 8 48 48 re f"),
      PdfStream("", "0 0 1 rg 8 8 48 48 re f")});
}

TEST(ViewQueries, NamedDestinationCollectionsAndTargetRendering) {
  Artifacts artifacts;
  for (bool tree : {false, true}) {
    for (bool legacy : {false, true}) {
      const auto bytes = DestinationFixture(tree, legacy);
      std::array<std::vector<Raster>, 2> observations;
      size_t backend = 0;
      for (const auto* library : Implementations()) {
        const auto& api = library->api();
        SCOPED_TRACE(library->name() + " tree=" + std::to_string(tree) + " legacy=" + std::to_string(legacy));
        Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
        ASSERT_NE(document.get(), nullptr);
        EXPECT_EQ(api.FPDF_CountNamedDests(document.get()), (tree ? 2u : 0u) + (legacy ? 1u : 0u));
        EXPECT_EQ(api.FPDF_GetNamedDestByName(document.get(), "missing"), nullptr);
        EXPECT_EQ(api.FPDF_GetNamedDestByName(document.get(), ""), nullptr);
        EXPECT_EQ(api.FPDF_GetNamedDestByName(document.get(), nullptr), nullptr);
        for (const auto& query : {std::pair<const char*, bool>{"Alpha", tree}, {"Legacy", legacy}}) {
          const auto destination = api.FPDF_GetNamedDestByName(document.get(), query.first);
          if (!query.second) {
            EXPECT_EQ(destination, nullptr);
            continue;
          }
          ASSERT_NE(destination, nullptr);
          const int expected = std::string(query.first) == "Alpha" ? 0 : 1;
          EXPECT_EQ(api.FPDFDest_GetDestPageIndex(document.get(), destination), expected);
          unsigned long count = 99;
          std::array<float, 4> params{};
          EXPECT_EQ(api.FPDFDest_GetView(destination, &count, params.data()),
                    expected == 0 ? PDFDEST_VIEW_XYZ : PDFDEST_VIEW_FIT);
          if (expected == 0) {
            EXPECT_EQ(count, 3u);
            EXPECT_FLOAT_EQ(params[0], 10);
            EXPECT_FLOAT_EQ(params[1], 20);
            EXPECT_FLOAT_EQ(params[2], 1.5f);
          }
          Handle page(api.FPDF_LoadPage(document.get(), expected), api.FPDF_ClosePage);
          ASSERT_NE(page.get(), nullptr);
          auto raster = Render(api, page.get(), 64, 64);
          ASSERT_EQ(raster.pixels.size(), 64u * 64 * 4);
          const size_t center = (32 * 64 + 32) * 4;
          EXPECT_EQ(raster.pixels[center], expected == 0 ? 0 : 255);
          EXPECT_EQ(raster.pixels[center + 1], 0);
          EXPECT_EQ(raster.pixels[center + 2], expected == 0 ? 255 : 0);
          observations[backend].push_back(std::move(raster));
        }
        ++backend;
      }
      ASSERT_EQ(observations[0].size(), observations[1].size());
      for (size_t index = 0; index < observations[0].size(); ++index) {
        CompareRaster(observations[0][index], observations[1][index], artifacts,
                      "tree" + std::to_string(tree) + "-legacy" + std::to_string(legacy) + "-target" + std::to_string(index));
      }
    }
  }
}

TEST(ViewQueries, IndexedDestinationUtf16BuffersAndErrors) {
  const auto bytes = DestinationFixture(true, true);
  const std::array<std::u16string, 3> names = {u"Alpha", u"\u03a9", u"Legacy"};
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    for (int index = 0; index < 3; ++index) {
      long required = 999;
      const auto indexed = api.FPDF_GetNamedDest(document.get(), index, nullptr, &required);
      ASSERT_NE(indexed, nullptr);
      const auto& name = names[index];
      EXPECT_EQ(required, static_cast<long>((name.size() + 1) * 2));
      EXPECT_EQ(api.FPDFDest_GetDestPageIndex(document.get(), indexed), index == 0 ? 0 : 1);
      for (long capacity : {0L, 1L, required - 1, required, required + 4}) {
        std::vector<uint16_t> buffer(static_cast<size_t>(required / 2) + 8, 0xa55a);
        long length = capacity;
        EXPECT_EQ(api.FPDF_GetNamedDest(document.get(), index, buffer.data() + 1, &length), indexed);
        if (capacity < required) {
          EXPECT_EQ(length, -1);
          EXPECT_TRUE(std::all_of(buffer.begin(), buffer.end(), [](uint16_t value) { return value == 0xa55a; }));
        } else {
          EXPECT_EQ(length, required);
          EXPECT_EQ(std::u16string(buffer.begin() + 1, buffer.begin() + 1 + name.size()), name);
          EXPECT_EQ(buffer[name.size() + 1], 0);
          EXPECT_EQ(buffer.front(), 0xa55a);
          EXPECT_TRUE(std::all_of(buffer.begin() + name.size() + 2, buffer.end(), [](uint16_t value) { return value == 0xa55a; }));
        }
      }
      if (index != 1) {
        const char* lookup = index == 0 ? "Alpha" : "Legacy";
        EXPECT_EQ(api.FPDF_GetNamedDestByName(document.get(), lookup), indexed);
      }
    }
    for (int index : {-1, 3, 99}) {
      std::array<uint16_t, 16> buffer;
      buffer.fill(0xa55a);
      long length = 20;
      EXPECT_EQ(api.FPDF_GetNamedDest(document.get(), index, buffer.data(), &length), nullptr);
      EXPECT_TRUE(std::all_of(buffer.begin(), buffer.end(), [](uint16_t value) { return value == 0xa55a; }));
    }
    EXPECT_EQ(api.FPDF_CountNamedDests(nullptr), 0u);
    EXPECT_EQ(api.FPDF_GetNamedDestByName(nullptr, "Alpha"), nullptr);
    long length = 0;
    EXPECT_EQ(api.FPDF_GetNamedDest(nullptr, 0, nullptr, &length), nullptr);
  }
}

std::vector<uint8_t> PacketFixture(int variant) {
  // XFA packet dictionaries have no public constructor. They are queried here
  // without loading an XFA form, so binary content can be checked independently.
  std::vector<std::string> objects = {
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] >>", ""};
  if (variant == 0) objects[3] = "<< /Fields [] >>";
  if (variant == 1) {
    objects[3] = "<< /Fields [] /XFA 5 0 R >>";
    objects.push_back(PdfStream("", "<xdp/>"));
  }
  if (variant == 2) {
    objects[3] = "<< /Fields [] /XFA [(template) 5 0 R (empty) 6 0 R (binary) 7 0 R /not-a-string 5 0 R (not-a-stream) 42 (unpaired)] >>";
    objects.push_back(PdfStream("/Filter /ASCIIHexDecode", "3c74656d706c6174652f3e>"));
    objects.push_back(PdfStream("", ""));
    objects.push_back(PdfStream("", std::string("a\0b\xff", 4)));
  }
  if (variant == 3) objects[3] = "<< /Fields [] /XFA 17 >>";
  return PdfFixture(objects);
}

void CheckPackets(const Api& api, FPDF_DOCUMENT document, int variant,
                   bool native_backend) {
  const std::vector<std::string> names = variant == 1 ? std::vector<std::string>{""}
      : variant == 2 ? std::vector<std::string>{"template", "empty", "binary"} : std::vector<std::string>{};
  const std::vector<std::string> contents = variant == 1 ? std::vector<std::string>{"<xdp/>"}
      : variant == 2 ? std::vector<std::string>{"<template/>", "", std::string("a\0b\xff", 4)} : std::vector<std::string>{};
  ASSERT_EQ(api.FPDF_GetXFAPacketCount(document), static_cast<int>(names.size()));
  for (size_t index = 0; index < names.size(); ++index) {
    const auto required = static_cast<unsigned long>(names[index].size() + 1);
    EXPECT_EQ(api.FPDF_GetXFAPacketName(document, static_cast<int>(index), nullptr, 0), required);
    for (unsigned long capacity : {0UL, required - 1, required, required + 3}) {
      std::vector<uint8_t> buffer(required + 8, 0xa5);
      EXPECT_EQ(api.FPDF_GetXFAPacketName(document, static_cast<int>(index), buffer.data() + 1, capacity), required);
      if (capacity < required) {
        EXPECT_TRUE(std::all_of(buffer.begin(), buffer.end(), [](uint8_t byte) { return byte == 0xa5; }));
      } else {
        EXPECT_EQ(std::string(buffer.begin() + 1, buffer.begin() + required), names[index]);
        EXPECT_EQ(buffer[required], 0);
        EXPECT_EQ(buffer.front(), 0xa5);
        EXPECT_TRUE(std::all_of(buffer.begin() + required + 1, buffer.end(), [](uint8_t byte) { return byte == 0xa5; }));
      }
    }
    unsigned long size = 999;
    ASSERT_TRUE(api.FPDF_GetXFAPacketContent(document, static_cast<int>(index), nullptr, 0, &size));
    if (native_backend && contents[index].empty() && size != 0) {
      // Native also mistakes a successfully decoded empty Flate stream for a
      // failed decode and exposes its encoded bytes. Confirm that observation
      // without imposing compressed-byte identity; Rust must still return the
      // genuinely empty packet. See the known-native-differences document.
      std::vector<uint8_t> encoded(size);
      unsigned long copied = 0;
      ASSERT_TRUE(api.FPDF_GetXFAPacketContent(document, static_cast<int>(index), encoded.data(), size, &copied));
      ASSERT_GE(encoded.size(), 6u);
      EXPECT_EQ(copied, size);
      EXPECT_EQ(encoded[0] & 15, 8);  // zlib DEFLATE method.
      EXPECT_EQ((encoded[0] * 256 + encoded[1]) % 31, 0);
      EXPECT_EQ((std::vector<uint8_t>(encoded.end() - 4, encoded.end())),
                (std::vector<uint8_t>{0, 0, 0, 1}));  // Adler-32 of empty input.
      continue;
    }
    EXPECT_EQ(size, contents[index].size());
    for (unsigned long capacity : {0UL, size ? size - 1 : 0, size, size + 3}) {
      // Pinned native's stream-copy helper reverses its capacity comparison:
      // short buffers trap, oversized buffers are ignored. Compare exact native
      // output while requiring the documented complete boundary contract from
      // Rust. See docs/capi-known-native-differences.md.
      if (native_backend && capacity != 0 && capacity != size) continue;
      std::vector<uint8_t> buffer(size + 8, 0xa5);
      unsigned long actual = 999;
      EXPECT_TRUE(api.FPDF_GetXFAPacketContent(document, static_cast<int>(index), buffer.data() + 1, capacity, &actual));
      EXPECT_EQ(actual, size);
      if (capacity < size) {
        EXPECT_TRUE(std::all_of(buffer.begin(), buffer.end(), [](uint8_t byte) { return byte == 0xa5; }));
      } else {
        EXPECT_EQ(std::string(buffer.begin() + 1, buffer.begin() + 1 + size), contents[index]);
        EXPECT_EQ(buffer.front(), 0xa5);
        EXPECT_TRUE(std::all_of(buffer.begin() + 1 + size, buffer.end(), [](uint8_t byte) { return byte == 0xa5; }));
      }
    }
  }
  for (int index : {-1, static_cast<int>(names.size()), 99}) {
    std::array<uint8_t, 32> buffer;
    buffer.fill(0xa5);
    EXPECT_EQ(api.FPDF_GetXFAPacketName(document, index, buffer.data(), buffer.size()), 0u);
    unsigned long size = 999;
    EXPECT_FALSE(api.FPDF_GetXFAPacketContent(document, index, buffer.data(), buffer.size(), &size));
    EXPECT_EQ(size, 999u);
    EXPECT_TRUE(std::all_of(buffer.begin(), buffer.end(), [](uint8_t byte) { return byte == 0xa5; }));
  }
  EXPECT_FALSE(api.FPDF_GetXFAPacketContent(document, 0, nullptr, 0, nullptr));
}

TEST(ViewQueries, XfaPacketsNamesBinaryContentBoundsAndSavedReaders) {
  Artifacts artifacts;
  for (int variant : {0, 1, 2, 3}) {
    const auto bytes = PacketFixture(variant);
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name() + " variant=" + std::to_string(variant));
      const auto& api = writer->api();
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(CheckPackets(api, document.get(), variant, writer->name() == "native"));
      const auto saved = SaveDocument(api, document.get(), artifacts, writer->name() + "-packets" + std::to_string(variant));
      ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api();
        Handle reloaded(read.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), read.FPDF_CloseDocument);
        ASSERT_NE(reloaded.get(), nullptr);
        ASSERT_NO_FATAL_FAILURE(CheckPackets(read, reloaded.get(), variant, reader->name() == "native"));
      }
    }
  }
  for (const auto* library : Implementations()) {
    EXPECT_EQ(library->api().FPDF_GetXFAPacketCount(nullptr), -1);
    EXPECT_EQ(library->api().FPDF_GetXFAPacketName(nullptr, 0, nullptr, 0), 0u);
    unsigned long size = 999;
    EXPECT_FALSE(library->api().FPDF_GetXFAPacketContent(nullptr, 0, nullptr, 0, &size));
    EXPECT_EQ(size, 999u);
  }
}

struct BstrOwner {
  explicit BstrOwner(const Api& api) : api(api) { EXPECT_EQ(api.FPDF_BStr_Init(&value), 0); }
  ~BstrOwner() { api.FPDF_BStr_Clear(&value); }
  const Api& api;
  FPDF_BSTR value{};
};

TEST(ViewQueries, BstrCopiesExplicitBinaryLengthsReplacesClearsAndReuses) {
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    BstrOwner owned(api);
    EXPECT_EQ(owned.value.str, nullptr);
    EXPECT_EQ(owned.value.len, 0);
    EXPECT_EQ(api.FPDF_BStr_Clear(&owned.value), 0);
    const std::vector<std::string> values = {"plain", std::string("A\0\xa9\x03\0\0", 6), std::string(4096, 'x'), "short", ""};
    for (const auto& value : values) {
      std::string source = value;
      ASSERT_EQ(api.FPDF_BStr_Set(&owned.value, source.c_str(), static_cast<int>(source.size())), 0);
      EXPECT_EQ(owned.value.len, value.size());
      if (value.empty()) {
        EXPECT_EQ(owned.value.str, nullptr);
      } else {
        ASSERT_NE(owned.value.str, nullptr);
        source.assign(source.size(), 'q');
        EXPECT_EQ(std::string(owned.value.str, owned.value.len), value);
        EXPECT_EQ(owned.value.str[owned.value.len], '\0');
      }
    }
    EXPECT_EQ(api.FPDF_BStr_Set(&owned.value, "terminated", -1), 0);
    EXPECT_EQ(owned.value.len, 10);
    EXPECT_STREQ(owned.value.str, "terminated");
    EXPECT_EQ(api.FPDF_BStr_Clear(&owned.value), 0);
    EXPECT_EQ(api.FPDF_BStr_Clear(&owned.value), 0);
    EXPECT_EQ(owned.value.len, 0);
    EXPECT_EQ(owned.value.str, nullptr);
    EXPECT_EQ(api.FPDF_BStr_Set(&owned.value, "reuse", 5), 0);
    EXPECT_STREQ(owned.value.str, "reuse");
    EXPECT_EQ(api.FPDF_BStr_Init(nullptr), -1);
    EXPECT_EQ(api.FPDF_BStr_Set(nullptr, "x", 1), -1);
    EXPECT_EQ(api.FPDF_BStr_Set(&owned.value, nullptr, 0), -1);
    EXPECT_EQ(api.FPDF_BStr_Clear(nullptr), -1);
  }
}

}  // namespace
}  // namespace pdfoxium::integration
