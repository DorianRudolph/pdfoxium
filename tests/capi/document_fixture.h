#pragma once

#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cstring>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration::document_test {

inline std::vector<FPDF_WCHAR> Wide(const std::u16string& text) {
  std::vector<FPDF_WCHAR> result(text.begin(), text.end());
  result.push_back(0);
  return result;
}

inline std::vector<uint8_t> Utf16(const std::u16string& text) {
  std::vector<uint8_t> result;
  for (char16_t unit : text) {
    result.push_back(static_cast<uint8_t>(unit));
    result.push_back(static_cast<uint8_t>(unit >> 8));
  }
  result.push_back(0);
  result.push_back(0);
  return result;
}

inline std::vector<uint8_t> Ascii(const std::string& text, bool terminate = true) {
  std::vector<uint8_t> result(text.begin(), text.end());
  if (terminate) result.push_back(0);
  return result;
}

inline std::string PdfText(const std::u16string& text) {
  constexpr char hex[] = "0123456789ABCDEF";
  std::string result = "<FEFF";
  for (char16_t unit : text) {
    for (int shift : {12, 8, 4, 0}) result += hex[(unit >> shift) & 15];
  }
  return result + ">";
}

// Every capacity is measured in bytes. Storage is aligned for APIs accepting
// FPDF_WCHAR*, and guards cover both the declared capacity and the allocation.
template <typename Getter>
void ExpectBytes(Getter get, const std::vector<uint8_t>& expected) {
  const auto size = static_cast<unsigned long>(expected.size());
  EXPECT_EQ(get(nullptr, 0), size);
  EXPECT_EQ(get(nullptr, size + 1), size);
  const std::array<unsigned long, 5> capacities = {
      0, size ? size - 1 : 0, size, size + 1, size + 8};
  for (unsigned long capacity : capacities) {
    SCOPED_TRACE(capacity);
    std::vector<uint64_t> words((size + 64) / sizeof(uint64_t) + 8,
                                 UINT64_C(0xa5a5a5a5a5a5a5a5));
    auto* begin = reinterpret_cast<uint8_t*>(words.data());
    auto* output = begin + 16;
    EXPECT_EQ(get(output, capacity), size);
    if (size && capacity >= size) {
      EXPECT_TRUE(std::equal(expected.begin(), expected.end(), output));
      EXPECT_TRUE(std::all_of(output + size,
                              begin + words.size() * sizeof(uint64_t),
                              [](uint8_t value) { return value == 0xa5; }));
    } else {
      EXPECT_TRUE(std::all_of(begin, begin + words.size() * sizeof(uint64_t),
                              [](uint8_t value) { return value == 0xa5; }));
    }
    EXPECT_TRUE(std::all_of(begin, output,
                            [](uint8_t value) { return value == 0xa5; }));
  }
}

inline std::vector<uint8_t> OnePage(const std::string& catalog_entries = "",
                                   std::vector<std::string> extra = {},
                                   const std::string& trailer = "") {
  std::vector<std::string> objects = {
      "<< /Type /Catalog /Pages 2 0 R " + catalog_entries + " >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Resources << >> /Contents 4 0 R >>",
      PdfStream("", "0.125 0.375 0.625 rg 12 16 40 32 re f\n")};
  objects.insert(objects.end(), extra.begin(), extra.end());
  return PdfFixture(objects, trailer);
}

inline void CrossRead(const Library& writer, FPDF_DOCUMENT document,
                       Artifacts& artifacts, const std::string& name,
                       const std::function<void(const Api&, FPDF_DOCUMENT)>& inspect,
                       RasterTolerance raster_tolerance={}) {
  ASSERT_NO_FATAL_FAILURE(inspect(writer.api(), document));
  Handle original(writer.api().FPDF_LoadPage(document, 0), writer.api().FPDF_ClosePage);
  ASSERT_NE(original.get(), nullptr);
  const auto before = Render(writer.api(), original.get(), 96, 96);
  const auto bytes = SaveDocument(writer.api(), document, artifacts,
                                   writer.name() + "-" + name);
  ASSERT_FALSE(bytes.empty());
  std::array<Raster, 2> reopened;
  size_t index = 0;
  for (const auto* reader : Implementations()) {
    SCOPED_TRACE(writer.name() + "-" + name + "-to-" + reader->name());
    const auto& api = reader->api();
    Handle loaded(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr),
                  api.FPDF_CloseDocument);
    ASSERT_NE(loaded.get(), nullptr);
    ASSERT_NO_FATAL_FAILURE(inspect(api, loaded.get()));
    Handle page(api.FPDF_LoadPage(loaded.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    reopened[index] = Render(api, page.get(), 96, 96);
    CompareRaster(before, reopened[index], artifacts,
                   writer.name() + "-" + name + "-" + reader->name(),raster_tolerance);
    ++index;
  }
  CompareRaster(reopened[0], reopened[1], artifacts, writer.name() + "-" + name + "-readers",raster_tolerance);
}

}  // namespace pdfoxium::integration::document_test
