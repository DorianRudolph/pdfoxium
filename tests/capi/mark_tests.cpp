#include "editing_fixture.h"

#include <algorithm>
#include <climits>
#include <cstring>
#include <set>

namespace pdfoxium::integration {
namespace {

std::u16string MarkName(const Api& api, FPDF_PAGEOBJECTMARK mark) {
  unsigned long needed = 991;
  EXPECT_TRUE(api.FPDFPageObjMark_GetName(mark, nullptr, 0, &needed));
  EXPECT_GE(needed, 2u);
  EXPECT_EQ(needed % 2, 0u);
  std::vector<unsigned short> buffer(needed / 2 + 2, 0xa5a5);
  EXPECT_TRUE(api.FPDFPageObjMark_GetName(mark, buffer.data(), needed - 1, &needed));
  EXPECT_TRUE(std::all_of(buffer.begin(), buffer.end(), [](auto c) { return c == 0xa5a5; }));
  EXPECT_TRUE(api.FPDFPageObjMark_GetName(mark, buffer.data(), needed, &needed));
  EXPECT_EQ(buffer[needed / 2 - 1], 0);
  EXPECT_EQ(buffer[needed / 2], 0xa5a5);
  std::u16string result(buffer.begin(), buffer.begin() + needed / 2 - 1);
  std::fill(buffer.begin(), buffer.end(), 0xa5a5);
  EXPECT_TRUE(api.FPDFPageObjMark_GetName(mark, buffer.data(), buffer.size() * 2, &needed));
  EXPECT_EQ(buffer[needed / 2], 0xa5a5);
  EXPECT_FALSE(api.FPDFPageObjMark_GetName(mark, buffer.data(), buffer.size() * 2, nullptr));
  return result;
}

std::set<std::u16string> Keys(const Api& api, FPDF_PAGEOBJECTMARK mark) {
  std::set<std::u16string> result;
  const int count = api.FPDFPageObjMark_CountParams(mark);
  EXPECT_GE(count, 0);
  for (int index = 0; index < count; ++index) {
    unsigned long needed = 991;
    EXPECT_TRUE(api.FPDFPageObjMark_GetParamKey(mark, index, nullptr, 0, &needed));
    EXPECT_GE(needed, 2u); EXPECT_EQ(needed % 2, 0u);
    std::vector<unsigned short> buffer(needed / 2 + 2, 0xa5a5);
    EXPECT_TRUE(api.FPDFPageObjMark_GetParamKey(mark, index, buffer.data(), needed - 1, &needed));
    EXPECT_TRUE(std::all_of(buffer.begin(), buffer.end(), [](auto c) { return c == 0xa5a5; }));
    EXPECT_TRUE(api.FPDFPageObjMark_GetParamKey(mark, index, buffer.data(), needed, &needed));
    EXPECT_EQ(buffer[needed / 2 - 1], 0); EXPECT_EQ(buffer[needed / 2], 0xa5a5);
    result.emplace(buffer.begin(), buffer.begin() + needed / 2 - 1);
    EXPECT_TRUE(api.FPDFPageObjMark_GetParamKey(mark, index, buffer.data(), buffer.size() * 2, &needed));
    EXPECT_FALSE(api.FPDFPageObjMark_GetParamKey(mark, index, nullptr, 0, nullptr));
  }
  for (unsigned long index : {static_cast<unsigned long>(count), 999ul}) {
    unsigned long untouched = 991;
    std::array<unsigned short, 8> output{}; output.fill(0xa5a5);
    EXPECT_FALSE(api.FPDFPageObjMark_GetParamKey(mark, index, output.data(), sizeof(output), &untouched));
    EXPECT_EQ(untouched, 991u);
    EXPECT_TRUE(std::all_of(output.begin(), output.end(), [](auto c) { return c == 0xa5a5; }));
  }
  return result;
}

std::vector<uint8_t> Blob(const Api& api, FPDF_PAGEOBJECTMARK mark,
                          const char* key, const std::vector<uint8_t>& expected) {
  unsigned long needed = 991;
  EXPECT_TRUE(api.FPDFPageObjMark_GetParamBlobValue(mark, key, nullptr, 0, &needed));
  EXPECT_EQ(needed, expected.size());
  std::vector<uint8_t> buffer(needed + 3, 0xa5);
  if (needed) {
    EXPECT_TRUE(api.FPDFPageObjMark_GetParamBlobValue(mark, key, buffer.data(), needed - 1, &needed));
    EXPECT_TRUE(std::all_of(buffer.begin(), buffer.end(), [](auto c) { return c == 0xa5; }));
  }
  EXPECT_TRUE(api.FPDFPageObjMark_GetParamBlobValue(mark, key, buffer.data(), needed, &needed));
  EXPECT_TRUE(std::equal(expected.begin(), expected.end(), buffer.begin()));
  EXPECT_EQ(buffer[needed], 0xa5);
  EXPECT_TRUE(api.FPDFPageObjMark_GetParamBlobValue(mark, key, buffer.data(), buffer.size(), &needed));
  EXPECT_EQ(buffer[needed], 0xa5);
  EXPECT_FALSE(api.FPDFPageObjMark_GetParamBlobValue(mark, key, buffer.data(), buffer.size(), nullptr));
  return std::vector<uint8_t>(buffer.begin(), buffer.begin() + needed);
}

std::u16string String(const Api& api, FPDF_PAGEOBJECTMARK mark,
                      const char* key, const std::u16string& expected) {
  unsigned long needed = 991;
  EXPECT_TRUE(api.FPDFPageObjMark_GetParamStringValue(mark, key, nullptr, 0, &needed));
  EXPECT_EQ(needed, (expected.size() + 1) * 2);
  std::vector<unsigned short> buffer(needed / 2 + 2, 0xa5a5);
  EXPECT_TRUE(api.FPDFPageObjMark_GetParamStringValue(mark, key, buffer.data(), needed - 1, &needed));
  EXPECT_TRUE(std::all_of(buffer.begin(), buffer.end(), [](auto c) { return c == 0xa5a5; }));
  EXPECT_TRUE(api.FPDFPageObjMark_GetParamStringValue(mark, key, buffer.data(), needed, &needed));
  EXPECT_EQ(buffer[needed / 2 - 1], 0); EXPECT_EQ(buffer[needed / 2], 0xa5a5);
  const std::u16string result(buffer.begin(), buffer.begin() + needed / 2 - 1);
  EXPECT_EQ(result, expected);
  EXPECT_TRUE(api.FPDFPageObjMark_GetParamStringValue(mark, key, buffer.data(), buffer.size() * 2, &needed));
  EXPECT_FALSE(api.FPDFPageObjMark_GetParamStringValue(mark, key, buffer.data(), buffer.size() * 2, nullptr));
  return result;
}

TEST(Marks, NamesOrderingSharingAndRemovalPersistWithoutChangingDrawing) {
  Artifacts artifacts;
  std::array<Raster, 2> results;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    std::array<FPDF_PAGEOBJECT, 3> objects{};
    for (size_t index = 0; index < objects.size(); ++index) {
      objects[index] = editing::Rectangle(api, 8 + index * 32, 16, 24, 48, 64 + index * 64, 32, 160);
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), objects[index]));
    }
    const auto before = Render(api, page.get(), 128, 128);
    EXPECT_EQ(api.FPDFPageObj_CountMarks(objects[0]), 0);
    auto first = api.FPDFPageObj_AddMark(objects[0], "Ordinary");
    auto escaped = api.FPDFPageObj_AddMark(objects[0], "Space #/Name");
    auto unicode = api.FPDFPageObj_AddMark(objects[0], "\xc3\xa9tiquette");
    auto duplicate = api.FPDFPageObj_AddMark(objects[0], "Ordinary");
    ASSERT_NE(first, nullptr); ASSERT_NE(escaped, nullptr); ASSERT_NE(unicode, nullptr); ASSERT_NE(duplicate, nullptr);
    EXPECT_EQ(MarkName(api, first), u"Ordinary");
    EXPECT_EQ(MarkName(api, escaped), u"Space #/Name");
    EXPECT_EQ(MarkName(api, unicode), u"étiquette");
    EXPECT_EQ(MarkName(api, duplicate), u"Ordinary");
    EXPECT_EQ(api.FPDFPageObj_CountMarks(objects[0]), 4);
    EXPECT_EQ(api.FPDFPageObj_GetMark(objects[0], 0), first);
    EXPECT_EQ(api.FPDFPageObj_GetMark(objects[0], 1), escaped);
    EXPECT_EQ(api.FPDFPageObj_GetMark(objects[0], 2), unicode);
    EXPECT_EQ(api.FPDFPageObj_GetMark(objects[0], 3), duplicate);
    EXPECT_EQ(api.FPDFPageObj_GetMark(objects[0], 4), nullptr);
    EXPECT_EQ(api.FPDFPageObj_GetMark(objects[0], 99), nullptr);
    ASSERT_TRUE(api.FPDFPageObjMark_SetIntParam(doc.get(), objects[0], first, "Shared", 7));
    ASSERT_TRUE(api.FPDFPageObj_AddExistingMark(objects[1], first));
    ASSERT_TRUE(api.FPDFPageObj_AddExistingMark(objects[2], first));
    ASSERT_TRUE(api.FPDFPageObjMark_SetIntParam(doc.get(), objects[1], api.FPDFPageObj_GetMark(objects[1], 0), "Shared", 19));
    int value = 0;
    ASSERT_TRUE(api.FPDFPageObjMark_GetParamIntValue(first, "Shared", &value)); EXPECT_EQ(value, 19);
    ASSERT_TRUE(api.FPDFPageObj_RemoveMark(objects[0], escaped));
    ASSERT_TRUE(api.FPDFPageObj_RemoveMark(objects[0], duplicate));
    ASSERT_TRUE(api.FPDFPageObj_RemoveMark(objects[1], api.FPDFPageObj_GetMark(objects[1], 0)));
    EXPECT_EQ(api.FPDFPageObj_CountMarks(objects[1]), 0);
    ASSERT_TRUE(api.FPDFPageObjMark_GetParamIntValue(api.FPDFPageObj_GetMark(objects[2], 0), "Shared", &value)); EXPECT_EQ(value, 19);
    auto unrelated = api.FPDFPageObj_AddMark(objects[1], "Other");
    ASSERT_NE(unrelated, nullptr);
    EXPECT_FALSE(api.FPDFPageObj_RemoveMark(objects[0], unrelated));
    EXPECT_FALSE(api.FPDFPageObj_RemoveMark(nullptr, first));
    EXPECT_FALSE(api.FPDFPageObj_RemoveMark(objects[0], nullptr));
    EXPECT_FALSE(api.FPDFPageObj_AddExistingMark(nullptr, first));
    EXPECT_FALSE(api.FPDFPageObj_AddExistingMark(objects[0], nullptr));
    EXPECT_EQ(api.FPDFPageObj_AddMark(nullptr, "Name"), nullptr);
    EXPECT_EQ(api.FPDFPageObj_CountMarks(nullptr), -1);
    EXPECT_EQ(api.FPDFPageObj_GetMark(nullptr, 0), nullptr);
    unsigned long length = 991;
    EXPECT_FALSE(api.FPDFPageObjMark_GetName(nullptr, nullptr, 0, &length)); EXPECT_EQ(length, 991u);
    // Empty is a valid, explicitly terminated name; do not fabricate a pointer.
    auto empty = api.FPDFPageObj_AddMark(objects[1], ""); ASSERT_NE(empty, nullptr);
    EXPECT_EQ(MarkName(api, empty), u"");
    ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*library, doc.get(), page.get(), artifacts,
        library->name() + "-marks", [](const Api& read, FPDF_PAGE loaded) {
          ASSERT_EQ(read.FPDFPage_CountObjects(loaded), 3);
          auto object = read.FPDFPage_GetObject(loaded, 0);
          ASSERT_EQ(read.FPDFPageObj_CountMarks(object), 2);
          EXPECT_EQ(MarkName(read, read.FPDFPageObj_GetMark(object, 0)), u"Ordinary");
          EXPECT_EQ(MarkName(read, read.FPDFPageObj_GetMark(object, 1)), u"étiquette");
          auto shared = read.FPDFPageObj_GetMark(read.FPDFPage_GetObject(loaded, 2), 0);
          int observed = 0; ASSERT_TRUE(read.FPDFPageObjMark_GetParamIntValue(shared, "Shared", &observed)); EXPECT_EQ(observed, 19);
        }));
    results[backend] = Render(api, page.get(), 128, 128);
    CompareRaster(before, results[backend++], artifacts, library->name() + "-marks-no-visual-effect");
  }
  CompareRaster(results[0], results[1], artifacts, "marks-backends");
}

TEST(Marks, ParameterReplacementCopyingQueriesAndSharedRemovalPersist) {
  Artifacts artifacts;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    auto object = editing::Rectangle(api, 16, 16, 80, 80, 224, 64, 32);
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), object));
    auto mark = api.FPDFPageObj_AddMark(object, "Parameters"); ASSERT_NE(mark, nullptr);
    EXPECT_EQ(api.FPDFPageObjMark_CountParams(mark), 0);
    for (int number : {0, -1, INT_MIN, INT_MAX}) {
      ASSERT_TRUE(api.FPDFPageObjMark_SetIntParam(doc.get(), object, mark, "Number", number));
      EXPECT_EQ(api.FPDFPageObjMark_CountParams(mark), 1);
      EXPECT_EQ(api.FPDFPageObjMark_GetParamValueType(mark, "Number"), FPDF_OBJECT_NUMBER);
      int observed = 0; float as_float = 0;
      ASSERT_TRUE(api.FPDFPageObjMark_GetParamIntValue(mark, "Number", &observed)); EXPECT_EQ(observed, number);
      ASSERT_TRUE(api.FPDFPageObjMark_GetParamFloatValue(mark, "Number", &as_float)); EXPECT_FLOAT_EQ(as_float, static_cast<float>(number));
    }
    for (float number : {0.0f, -2.5f, 1.25f, 1024.5f}) {
      ASSERT_TRUE(api.FPDFPageObjMark_SetFloatParam(doc.get(), object, mark, "Number", number));
      float observed = 0; ASSERT_TRUE(api.FPDFPageObjMark_GetParamFloatValue(mark, "Number", &observed)); EXPECT_FLOAT_EQ(observed, number);
      int as_int = 0; ASSERT_TRUE(api.FPDFPageObjMark_GetParamIntValue(mark, "Number", &as_int)); EXPECT_EQ(as_int, static_cast<int>(number));
      EXPECT_EQ(api.FPDFPageObjMark_CountParams(mark), 1);
    }
    ASSERT_TRUE(api.FPDFPageObjMark_SetStringParam(doc.get(), object, mark, "Number", "replacement"));
    EXPECT_EQ(String(api, mark, "Number", u"replacement"), u"replacement");
    ASSERT_TRUE(api.FPDFPageObjMark_SetIntParam(doc.get(), object, mark, "Number", -123));
    ASSERT_TRUE(api.FPDFPageObjMark_SetStringParam(doc.get(), object, mark, "String", ""));
    String(api, mark, "String", u""); Blob(api, mark, "String", {});
    ASSERT_TRUE(api.FPDFPageObjMark_SetStringParam(doc.get(), object, mark, "String", "ASCII (escaped) \\ text"));
    String(api, mark, "String", u"ASCII (escaped) \\ text");
    ASSERT_TRUE(api.FPDFPageObjMark_SetStringParam(doc.get(), object, mark, "String", "\xc3\xa9"));
    String(api, mark, "String", u"é"); Blob(api, mark, "String", {0xc3, 0xa9});
    std::vector<uint8_t> binary = {0, 1, 0x80, 0xff, 0, 0x42};
    const auto expected = binary;
    ASSERT_TRUE(api.FPDFPageObjMark_SetBlobParam(doc.get(), object, mark, "Blob", binary.data(), binary.size()));
    std::fill(binary.begin(), binary.end(), 0x11);
    Blob(api, mark, "Blob", expected);
    const unsigned char empty = 0;
    ASSERT_TRUE(api.FPDFPageObjMark_SetBlobParam(doc.get(), object, mark, "Empty", &empty, 0));
    Blob(api, mark, "Empty", {}); String(api, mark, "Empty", u"");
    const unsigned char utf8[] = {0x41, 0xc3, 0xa9};
    ASSERT_TRUE(api.FPDFPageObjMark_SetBlobParam(doc.get(), object, mark, "TextBlob", utf8, sizeof(utf8)));
    String(api, mark, "TextBlob", u"Aé");
    ASSERT_TRUE(api.FPDFPageObjMark_SetFloatParam(doc.get(), object, mark, "Space #/Key", -7.25f));
    ASSERT_TRUE(api.FPDFPageObjMark_SetIntParam(doc.get(), object, mark, "\xc3\xa9", 9));
    EXPECT_EQ(Keys(api, mark), (std::set<std::u16string>{u"Blob", u"Empty", u"Number", u"Space #/Key", u"String", u"TextBlob", u"é"}));
    EXPECT_TRUE(api.FPDFPageObjMark_RemoveParam(object, mark, "Empty"));
    EXPECT_FALSE(api.FPDFPageObjMark_RemoveParam(object, mark, "Empty"));
    EXPECT_EQ(api.FPDFPageObjMark_GetParamValueType(mark, "Empty"), FPDF_OBJECT_UNKNOWN);
    EXPECT_EQ(api.FPDFPageObjMark_GetParamValueType(mark, "missing"), FPDF_OBJECT_UNKNOWN);
    EXPECT_EQ(api.FPDFPageObjMark_GetParamValueType(nullptr, "Number"), FPDF_OBJECT_UNKNOWN);
    EXPECT_EQ(api.FPDFPageObjMark_CountParams(nullptr), -1);
    EXPECT_FALSE(api.FPDFPageObjMark_RemoveParam(nullptr, mark, "Number"));
    EXPECT_FALSE(api.FPDFPageObjMark_RemoveParam(object, nullptr, "Number"));
    ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*library, doc.get(), page.get(), artifacts,
        library->name() + "-parameters", [expected](const Api& read, FPDF_PAGE loaded) {
          auto object = read.FPDFPage_GetObject(loaded, 0);
          auto mark = read.FPDFPageObj_GetMark(object, 0);
          ASSERT_NE(mark, nullptr);
          EXPECT_EQ(read.FPDFPageObjMark_CountParams(mark), 6);
          EXPECT_EQ(Keys(read, mark), (std::set<std::u16string>{u"Blob", u"Number", u"Space #/Key", u"String", u"TextBlob", u"é"}));
          int number = 0; ASSERT_TRUE(read.FPDFPageObjMark_GetParamIntValue(mark, "Number", &number)); EXPECT_EQ(number, -123);
          float fractional = 0; ASSERT_TRUE(read.FPDFPageObjMark_GetParamFloatValue(mark, "Space #/Key", &fractional)); EXPECT_FLOAT_EQ(fractional, -7.25f);
          Blob(read, mark, "Blob", expected); String(read, mark, "String", u"é");
          EXPECT_EQ(read.FPDFPageObjMark_GetParamValueType(mark, "Empty"), FPDF_OBJECT_UNKNOWN);
        }));
  }
}

TEST(Marks, WrongTypesAndRequiredOutputFailuresPreserveSentinels) {
  // Booleans, names, arrays, dictionaries, null and indirect references lack
  // public parameter constructors. The fixture intentionally supplies each.
  const auto fixture = editing::SceneFixture(
      "/Tag /P BDC 1 0 0 rg 16 16 32 32 re f EMC",
      "/Properties << /P 5 0 R >>", {"<< /Bool true /Name /DeviceRGB /Array [1 2] /Dict << /K 1 >> /Null null /Ref 6 0 R /Number 1.5 /String (hello) >>", "(indirect)"});
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    auto object = api.FPDFPage_GetObject(page.get(), 0);
    auto mark = api.FPDFPageObj_GetMark(object, 0);
    ASSERT_NE(mark, nullptr);
    for (const auto& pair : std::vector<std::pair<const char*, int>>{
        {"Bool", FPDF_OBJECT_BOOLEAN}, {"Name", FPDF_OBJECT_NAME}, {"Array", FPDF_OBJECT_ARRAY},
        {"Dict", FPDF_OBJECT_DICTIONARY}, {"Null", FPDF_OBJECT_NULLOBJ},
        {"Ref", FPDF_OBJECT_REFERENCE}, {"Number", FPDF_OBJECT_NUMBER}, {"String", FPDF_OBJECT_STRING}}) {
      EXPECT_EQ(api.FPDFPageObjMark_GetParamValueType(mark, pair.first), pair.second);
    }
    for (const char* key : {"missing", "Bool", "Name", "Array", "Dict", "Null", "String"}) {
      int integer = 91; float fractional = 92;
      EXPECT_FALSE(api.FPDFPageObjMark_GetParamIntValue(mark, key, &integer)); EXPECT_EQ(integer, 91);
      EXPECT_FALSE(api.FPDFPageObjMark_GetParamFloatValue(mark, key, &fractional)); EXPECT_FLOAT_EQ(fractional, 92);
    }
    EXPECT_FALSE(api.FPDFPageObjMark_GetParamIntValue(mark, "Number", nullptr));
    EXPECT_FALSE(api.FPDFPageObjMark_GetParamFloatValue(mark, "Number", nullptr));
    for (const char* key : {"missing", "Bool", "Name", "Array", "Dict", "Null", "Number"}) {
      unsigned long needed = 991;
      std::array<unsigned short, 8> text{}; text.fill(0xa5a5);
      std::array<unsigned char, 8> bytes{}; bytes.fill(0xa5);
      EXPECT_FALSE(api.FPDFPageObjMark_GetParamStringValue(mark, key, text.data(), sizeof(text), &needed)); EXPECT_EQ(needed, 991u);
      EXPECT_FALSE(api.FPDFPageObjMark_GetParamBlobValue(mark, key, bytes.data(), bytes.size(), &needed)); EXPECT_EQ(needed, 991u);
      EXPECT_TRUE(std::all_of(text.begin(), text.end(), [](auto c) { return c == 0xa5a5; }));
      EXPECT_TRUE(std::all_of(bytes.begin(), bytes.end(), [](auto c) { return c == 0xa5; }));
    }
    for (int which = 0; which < 3; ++which) {
      const auto d = which == 0 ? nullptr : doc.get();
      const auto o = which == 1 ? nullptr : object;
      const auto m = which == 2 ? nullptr : mark;
      EXPECT_FALSE(api.FPDFPageObjMark_SetIntParam(d, o, m, "Key", 1));
      EXPECT_FALSE(api.FPDFPageObjMark_SetFloatParam(d, o, m, "Key", 1.5f));
      EXPECT_FALSE(api.FPDFPageObjMark_SetStringParam(d, o, m, "Key", "value"));
      const unsigned char data = 1;
      EXPECT_FALSE(api.FPDFPageObjMark_SetBlobParam(d, o, m, "Key", &data, 1));
    }
  }
}

TEST(Marks, ExplicitAndAbsentMarkedContentIdsSurviveRegeneration) {
  Artifacts artifacts;
  const auto fixture = editing::SceneFixture(
      "1 0 0 rg 8 8 16 16 re f /Span << /MCID 0 >> BDC 0 1 0 rg 32 8 16 16 re f EMC "
      "/Span << /MCID 17 >> BDC 0 0 1 rg 56 8 16 16 re f EMC");
  std::array<Raster, 2> results;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    auto inspect = [](const Api& read, FPDF_PAGE p) {
      ASSERT_EQ(read.FPDFPage_CountObjects(p), 3);
      EXPECT_EQ(read.FPDFPageObj_GetMarkedContentID(read.FPDFPage_GetObject(p, 0)), -1);
      EXPECT_EQ(read.FPDFPageObj_GetMarkedContentID(read.FPDFPage_GetObject(p, 1)), 0);
      EXPECT_EQ(read.FPDFPageObj_GetMarkedContentID(read.FPDFPage_GetObject(p, 2)), 17);
      EXPECT_EQ(read.FPDFPageObj_GetMarkedContentID(nullptr), -1);
    };
    ASSERT_NO_FATAL_FAILURE(inspect(api, page.get()));
    ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*writer, doc.get(), page.get(), artifacts,
        writer->name() + "-mcid", inspect));
    results[backend++] = Render(api, page.get(), 128, 128);
  }
  CompareRaster(results[0], results[1], artifacts, "marked-content-ids");
}

}  // namespace
}  // namespace pdfoxium::integration
