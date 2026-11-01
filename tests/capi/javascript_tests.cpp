#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cstdint>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

std::string PdfWide(const std::u16string& value) {
  constexpr char digits[] = "0123456789abcdef";
  std::string result = "<feff";
  for (uint16_t word : value) {
    for (int shift : {12, 8, 4, 0}) result.push_back(digits[(word >> shift) & 15]);
  }
  return result + ">";
}

struct ScriptEntry {
  std::u16string name;
  std::u16string source;
  bool stream;
};

const std::vector<ScriptEntry>& Scripts() {
  static const std::vector<ScriptEntry> entries = {
      {u"", u"", false},
      {u"ascii", u"this.getField('input').value='metadata must not execute';", false},
      {u"multiline", u"// first line\nvar result = 3;\nresult += 4;", false},
      {u"stream", std::u16string(2048, u' ') + u"var streamScript = 'complete';", true},
      {u"\u03a9\u4e2d\U0001f642", u"var unicode = '\u03a9\u4e2d\U0001f642';", false}};
  return entries;
}

// JavaScript name-tree/action dictionaries have no public constructors. The
// only field is deliberately untouched while metadata APIs enumerate scripts.
std::vector<uint8_t> ScriptFixture(bool missing = false, bool malformed = false,
                                    size_t count = 5) {
  std::vector<std::string> objects = {
      "", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Annots [5 0 R] >>",
      "<< /Fields [5 0 R] /DA (/Helv 12 Tf 0 g) >>",
      "<< /Type /Annot /Subtype /Widget /P 3 0 R /Rect [10 10 90 30] /FT /Tx /T (input) /V (unchanged) >>"};
  std::string names;
  for (size_t index = 0; index < count; ++index) {
    const auto& entry = Scripts()[index];
    const int action = static_cast<int>(objects.size()) + 1;
    names += PdfWide(entry.name) + " " + std::to_string(action) + " 0 R ";
    if (entry.stream) {
      objects.push_back("<< /S /JavaScript /JS " + std::to_string(action + 1) + " 0 R >>");
      std::string bytes = "\xfe\xff";
      for (uint16_t word : entry.source) {
        bytes.push_back(static_cast<char>(word >> 8));
        bytes.push_back(static_cast<char>(word));
      }
      objects.push_back(PdfStream("", bytes));
    } else {
      objects.push_back("<< /S /JavaScript /JS " + PdfWide(entry.source) + " >>");
    }
  }
  objects[0] = "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R";
  if (!missing) objects[0] += malformed ? " /Names << /JavaScript 42 >>"
                                      : " /Names << /JavaScript << /Names [" + names + "] >> >>";
  objects[0] += " >>";
  return PdfFixture(objects);
}

using Getter = decltype(Api::FPDFJavaScriptAction_GetName);
void CheckWideBoundaries(Getter get, FPDF_JAVASCRIPT_ACTION action,
                         const std::u16string& expected) {
  const auto required = static_cast<unsigned long>((expected.size() + 1) * 2);
  ASSERT_EQ(get(action, nullptr, 0), required);
  EXPECT_EQ(get(action, nullptr, required), required);
  for (unsigned long capacity : {0ul, 1ul, required - 1, required, required + 8}) {
    SCOPED_TRACE(capacity);
    std::vector<uint16_t> storage((required + 16) / 2 + 2, 0xa55a);
    const auto before = storage;
    EXPECT_EQ(get(action, storage.data() + 1, capacity), required);
    if (capacity < required) {
      EXPECT_EQ(storage, before);
    } else {
      const std::vector<uint16_t> actual(storage.begin() + 1,
                                        storage.begin() + 1 + expected.size());
      EXPECT_EQ(actual, std::vector<uint16_t>(expected.begin(), expected.end()));
      EXPECT_EQ(storage[expected.size() + 1], 0);
      EXPECT_EQ(storage.front(), 0xa55a);
      EXPECT_TRUE(std::all_of(storage.begin() + expected.size() + 2, storage.end(),
                              [](uint16_t value) { return value == 0xa55a; }));
    }
  }
}

TEST(JavaScriptMetadata, CountsMissingMalformedOneAndMultipleActionsWithoutExecution) {
  std::array<std::vector<int>, 2> observations;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    EXPECT_EQ(api.FPDFDoc_GetJavaScriptActionCount(nullptr), -1);
    for (const auto config : {std::array<int, 3>{1, 0, 0}, {0, 1, 0},
                               {0, 0, 0}, {0, 0, 1}, {0, 0, 5}}) {
      const auto bytes = ScriptFixture(config[0], config[1], config[2]);
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr),
                      api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      const int count = api.FPDFDoc_GetJavaScriptActionCount(document.get());
      EXPECT_EQ(count, config[2]);
      observations[backend].push_back(count);
      Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      Handle annotation(api.FPDFPage_GetAnnot(page.get(), 0), api.FPDFPage_CloseAnnot);
      ASSERT_NE(annotation.get(), nullptr);
      std::array<uint16_t, 32> value{};
      EXPECT_EQ(api.FPDFAnnot_GetStringValue(annotation.get(), "V", value.data(), sizeof(value)), 20u);
      EXPECT_EQ(std::u16string(value.begin(), value.begin() + 9), u"unchanged");
    }
    ++backend;
  }
  EXPECT_EQ(observations[0], observations[1]);
}

TEST(JavaScriptMetadata, CompleteNamesScriptsBoundariesAndIndependentOwners) {
  const auto bytes = ScriptFixture();
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr),
                    api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    ASSERT_EQ(api.FPDFDoc_GetJavaScriptActionCount(document.get()), static_cast<int>(Scripts().size()));
    EXPECT_EQ(api.FPDFDoc_GetJavaScriptAction(document.get(), -1), nullptr);
    EXPECT_EQ(api.FPDFDoc_GetJavaScriptAction(document.get(), Scripts().size()), nullptr);
    EXPECT_EQ(api.FPDFDoc_GetJavaScriptAction(nullptr, 0), nullptr);
    for (size_t index = 0; index < Scripts().size(); ++index) {
      SCOPED_TRACE(index);
      Handle first(api.FPDFDoc_GetJavaScriptAction(document.get(), static_cast<int>(index)),
                   api.FPDFDoc_CloseJavaScriptAction);
      Handle second(api.FPDFDoc_GetJavaScriptAction(document.get(), static_cast<int>(index)),
                    api.FPDFDoc_CloseJavaScriptAction);
      ASSERT_NE(first.get(), nullptr);
      ASSERT_NE(second.get(), nullptr);
      EXPECT_NE(first.get(), second.get());
      ASSERT_NO_FATAL_FAILURE(CheckWideBoundaries(api.FPDFJavaScriptAction_GetName,
                                                  first.get(), Scripts()[index].name));
      ASSERT_NO_FATAL_FAILURE(CheckWideBoundaries(api.FPDFJavaScriptAction_GetScript,
                                                  second.get(), Scripts()[index].source));
      first.reset();
      Handle reopened(api.FPDFDoc_GetJavaScriptAction(document.get(), static_cast<int>(index)),
                      api.FPDFDoc_CloseJavaScriptAction);
      ASSERT_NE(reopened.get(), nullptr);
      ASSERT_NO_FATAL_FAILURE(CheckWideBoundaries(api.FPDFJavaScriptAction_GetName,
                                                  reopened.get(), Scripts()[index].name));
      ASSERT_NO_FATAL_FAILURE(CheckWideBoundaries(api.FPDFJavaScriptAction_GetScript,
                                                  reopened.get(), Scripts()[index].source));
    }
    for (const Getter getter : {api.FPDFJavaScriptAction_GetName, api.FPDFJavaScriptAction_GetScript}) {
      std::array<uint16_t, 4> sentinel{0xa55a, 0xa55a, 0xa55a, 0xa55a};
      EXPECT_EQ(getter(nullptr, sentinel.data() + 1, 4), 0u);
      EXPECT_EQ(sentinel, (std::array<uint16_t, 4>{0xa55a, 0xa55a, 0xa55a, 0xa55a}));
    }
  }
}

}  // namespace
}  // namespace pdfoxium::integration
