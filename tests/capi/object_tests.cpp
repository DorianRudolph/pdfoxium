#include "editing_fixture.h"

#include <algorithm>
#include <cmath>
#include <limits>

namespace pdfoxium::integration {
namespace {

using editing::Rectangle;

TEST(Objects, EveryCategoryHasExpectedTypeBoundsMatrixAndOwnedChildren) {
  Artifacts artifacts;
  const auto bytes = editing::AllObjectTypesFixture();
  std::array<Raster, 2> pictures;
  editing::TextRegions regions;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    ASSERT_EQ(api.FPDFPage_CountObjects(page.get()), 5);
    const std::array<int, 5> types = {FPDF_PAGEOBJ_SHADING, FPDF_PAGEOBJ_PATH,
                                     FPDF_PAGEOBJ_TEXT, FPDF_PAGEOBJ_IMAGE,
                                     FPDF_PAGEOBJ_FORM};
    for (size_t index = 0; index < types.size(); ++index) {
      auto object = api.FPDFPage_GetObject(page.get(), static_cast<int>(index));
      ASSERT_NE(object, nullptr);
      EXPECT_EQ(api.FPDFPageObj_GetType(object), types[index]);
      const auto bounds = editing::Bounds(api, object);
      for (float value : bounds) EXPECT_TRUE(std::isfinite(value));
      EXPECT_LE(bounds[0], bounds[2]); EXPECT_LE(bounds[1], bounds[3]);
      FS_MATRIX matrix{};
      if (types[index] == FPDF_PAGEOBJ_SHADING) {
        EXPECT_FALSE(api.FPDFPageObj_GetMatrix(object, &matrix));
      } else {
        ASSERT_TRUE(api.FPDFPageObj_GetMatrix(object, &matrix));
        EXPECT_TRUE(std::isfinite(matrix.a)); EXPECT_TRUE(std::isfinite(matrix.f));
      }
      FPDF_BOOL active = 0;
      ASSERT_TRUE(api.FPDFPageObj_GetIsActive(object, &active));
      EXPECT_TRUE(active);
      EXPECT_TRUE(api.FPDFPageObj_SetIsActive(object, 0));
      EXPECT_TRUE(api.FPDFPageObj_GetIsActive(object, &active)); EXPECT_FALSE(active);
      EXPECT_EQ(api.FPDFPage_CountObjects(page.get()), 5);
      EXPECT_TRUE(api.FPDFPageObj_SetIsActive(object, 1));
      EXPECT_TRUE(api.FPDFPageObj_GetIsActive(object, &active)); EXPECT_TRUE(active);
    }
    EXPECT_EQ(api.FPDFPage_GetObject(page.get(), -1), nullptr);
    EXPECT_EQ(api.FPDFPage_GetObject(page.get(), 5), nullptr);
    EXPECT_EQ(api.FPDFPage_GetObject(page.get(), 99), nullptr);
    EXPECT_EQ(api.FPDFPageObj_GetType(nullptr), FPDF_PAGEOBJ_UNKNOWN);
    FPDF_BOOL untouched = 93;
    EXPECT_FALSE(api.FPDFPageObj_GetIsActive(nullptr, &untouched)); EXPECT_EQ(untouched, 93);
    EXPECT_FALSE(api.FPDFPageObj_GetIsActive(api.FPDFPage_GetObject(page.get(), 1), nullptr));
    EXPECT_FALSE(api.FPDFPageObj_SetIsActive(nullptr, 0));
    auto form = api.FPDFPage_GetObject(page.get(), 4);
    ASSERT_EQ(api.FPDFFormObj_CountObjects(form), 4);
    const std::array<int, 4> children = {FPDF_PAGEOBJ_PATH, FPDF_PAGEOBJ_TEXT,
                                        FPDF_PAGEOBJ_IMAGE, FPDF_PAGEOBJ_FORM};
    for (size_t index = 0; index < children.size(); ++index) {
      auto child = api.FPDFFormObj_GetObject(form, index);
      ASSERT_NE(child, nullptr);
      EXPECT_EQ(api.FPDFPageObj_GetType(child), children[index]);
      FS_MATRIX matrix{};
      ASSERT_TRUE(api.FPDFPageObj_GetMatrix(child, &matrix));
      if (index == 0) { EXPECT_FLOAT_EQ(matrix.e, 0); EXPECT_FLOAT_EQ(matrix.f, 0); }
      editing::Bounds(api, child);
    }
    auto nested = api.FPDFFormObj_GetObject(form, 3);
    ASSERT_EQ(api.FPDFFormObj_CountObjects(nested), 2);
    auto empty = api.FPDFFormObj_GetObject(nested, 1);
    ASSERT_EQ(api.FPDFPageObj_GetType(empty), FPDF_PAGEOBJ_FORM);
    EXPECT_EQ(api.FPDFFormObj_CountObjects(empty), 0);
    EXPECT_EQ(api.FPDFFormObj_GetObject(form, 4), nullptr);
    EXPECT_EQ(api.FPDFFormObj_GetObject(form, 99), nullptr);
    EXPECT_EQ(api.FPDFFormObj_CountObjects(nullptr), -1);
    EXPECT_EQ(api.FPDFFormObj_CountObjects(api.FPDFPage_GetObject(page.get(), 1)), -1);
    EXPECT_EQ(api.FPDFFormObj_GetObject(nullptr, 0), nullptr);
    EXPECT_EQ(api.FPDFFormObj_GetObject(api.FPDFPage_GetObject(page.get(), 1), 0), nullptr);
    regions = editing::TextBounds(api,page.get());
    pictures[backend++] = Render(api, page.get(), 128, 128);
    // Native's generator omits shading objects. Its read-only category and
    // rendering checks remain above; Rust shading persistence is tested below.
    if (library->name() == "rust") {
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*library, doc.get(), page.get(), artifacts,
          library->name() + "-categories", [](const Api& read, FPDF_PAGE loaded) {
            ASSERT_EQ(read.FPDFPage_CountObjects(loaded), 5);
            auto loaded_form = read.FPDFPage_GetObject(loaded, 4);
            ASSERT_EQ(read.FPDFFormObj_CountObjects(loaded_form), 4);
            EXPECT_EQ(read.FPDFFormObj_CountObjects(read.FPDFFormObj_GetObject(loaded_form, 3)), 2);
          }, 1));
    }
  }
  // Shading conversion may round a channel by one; font edges use local ROIs.
  editing::CompareScene(pictures[0], pictures[1], artifacts, "all-types", regions, 1);
}

TEST(Objects, InsertAtIndexTransfersOwnershipAndPreservesStackingAfterRemoval) {
  Artifacts artifacts;
  std::array<Raster, 2> results;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    auto red = Rectangle(api, 8, 8, 80, 80, 255, 0, 0);
    auto green = Rectangle(api, 24, 24, 80, 80, 0, 255, 0);
    auto blue = Rectangle(api, 40, 40, 80, 80, 0, 0, 255);
    ASSERT_TRUE(api.FPDFPage_InsertObjectAtIndex(page.get(), red, 0));
    ASSERT_TRUE(api.FPDFPage_InsertObjectAtIndex(page.get(), blue, 1));
    ASSERT_TRUE(api.FPDFPage_InsertObjectAtIndex(page.get(), green, 1));
    ASSERT_EQ(api.FPDFPage_CountObjects(page.get()), 3);
    EXPECT_EQ(api.FPDFPage_GetObject(page.get(), 0), red);
    EXPECT_EQ(api.FPDFPage_GetObject(page.get(), 1), green);
    EXPECT_EQ(api.FPDFPage_GetObject(page.get(), 2), blue);
    ASSERT_FALSE(api.FPDFPage_InsertObjectAtIndex(page.get(), nullptr, 0));
    // Nonnull failed insertions are consumed, so never destroy/reuse them.
    EXPECT_FALSE(api.FPDFPage_InsertObjectAtIndex(page.get(), Rectangle(api, 0, 0, 1, 1, 1, 2, 3), 99));
    EXPECT_FALSE(api.FPDFPage_InsertObjectAtIndex(nullptr, Rectangle(api, 0, 0, 1, 1, 1, 2, 3), 0));
    EXPECT_FALSE(api.FPDFPage_InsertObject(nullptr, Rectangle(api, 0, 0, 1, 1, 1, 2, 3)));
    EXPECT_FALSE(api.FPDFPage_InsertObject(page.get(), nullptr));
    ASSERT_TRUE(api.FPDFPage_RemoveObject(page.get(), green));
    Handle removed(green, api.FPDFPageObj_Destroy);
    ASSERT_EQ(api.FPDFPage_CountObjects(page.get()), 2);
    EXPECT_EQ(api.FPDFPage_GetObject(page.get(), 1), blue);
    // Removed objects become caller-owned and can be inserted again.
    ASSERT_TRUE(api.FPDFPage_InsertObjectAtIndex(page.get(), removed.release(), 2));
    ASSERT_TRUE(api.FPDFPage_RemoveObject(page.get(), red));
    api.FPDFPageObj_Destroy(red);
    ASSERT_TRUE(api.FPDFPage_RemoveObject(page.get(), green));
    api.FPDFPageObj_Destroy(green);
    EXPECT_EQ(api.FPDFPage_CountObjects(page.get()), 1);
    EXPECT_EQ(api.FPDFPage_GetObject(page.get(), 0), blue);
    Handle other_page(api.FPDFPage_New(doc.get(), 1, 128, 128), api.FPDF_ClosePage);
    auto foreign = Rectangle(api, 8, 8, 16, 16, 64, 64, 64);
    ASSERT_TRUE(api.FPDFPage_InsertObject(other_page.get(), foreign));
    EXPECT_FALSE(api.FPDFPage_RemoveObject(page.get(), foreign));
    EXPECT_FALSE(api.FPDFPage_RemoveObject(page.get(), nullptr));
    EXPECT_FALSE(api.FPDFPage_RemoveObject(nullptr, blue));
    EXPECT_EQ(api.FPDFPage_CountObjects(other_page.get()), 1);
    EXPECT_EQ(api.FPDFPage_GetObject(other_page.get(), 0), foreign);
    ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*library, doc.get(), page.get(), artifacts,
        library->name() + "-indexed", [](const Api& read, FPDF_PAGE loaded) {
          ASSERT_EQ(read.FPDFPage_CountObjects(loaded), 1);
          EXPECT_EQ(read.FPDFPageObj_GetType(read.FPDFPage_GetObject(loaded, 0)), FPDF_PAGEOBJ_PATH);
        }));
    results[backend++] = Render(api, page.get(), 128, 128);
  }
  CompareRaster(results[0], results[1], artifacts, "indexed-removal");
}

TEST(Objects, GenerateMixedEditsRepeatedlyAndRemoveTextWithoutStaleTextPages) {
  Artifacts artifacts;
  std::array<Raster, 2> results;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    auto path = Rectangle(api, 8, 8, 48, 48, 255, 0, 0);
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), path));
    auto text = api.FPDFPageObj_NewTextObj(doc.get(), "Helvetica", 12);
    ASSERT_NE(text, nullptr);
    const unsigned short value[] = {'A', 'B', 0};
    ASSERT_TRUE(api.FPDFText_SetText(text, value));
    FS_MATRIX matrix{1, 0, 0, 1, 16, 96};
    ASSERT_TRUE(api.FPDFPageObj_SetMatrix(text, &matrix));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), text));
    Handle bitmap(api.FPDFBitmap_Create(3, 2, 0), api.FPDFBitmap_Destroy);
    ASSERT_NE(bitmap.get(), nullptr);
    ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, 3, 2, 0xff00ff00));
    auto image = api.FPDFPageObj_NewImageObj(doc.get());
    ASSERT_NE(image, nullptr);
    ASSERT_TRUE(api.FPDFImageObj_SetBitmap(nullptr, 0, image, bitmap.get()));
    ASSERT_TRUE(api.FPDFImageObj_SetMatrix(image, 24, 0, 0, 16, 64, 8));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), image));
    auto mark = api.FPDFPageObj_AddMark(path, "Editable");
    ASSERT_NE(mark, nullptr);
    ASSERT_TRUE(api.FPDFPageObjMark_SetIntParam(doc.get(), path, mark, "Count", 3));
    for (int repeat = 0; repeat < 2; ++repeat) {
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      EXPECT_EQ(api.FPDFPage_CountObjects(page.get()), 3);
    }
    {
      Handle text_page(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
      ASSERT_NE(text_page.get(), nullptr);
      EXPECT_EQ(api.FPDFText_CountChars(text_page.get()), 2);
    }
    ASSERT_TRUE(api.FPDFPage_RemoveObject(page.get(), text));
    api.FPDFPageObj_Destroy(text);
    ASSERT_TRUE(api.FPDFPageObj_SetIsActive(image, 0));
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    EXPECT_EQ(api.FPDFPage_CountObjects(page.get()), 2);
    Handle current_text(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
    ASSERT_NE(current_text.get(), nullptr);
    EXPECT_EQ(api.FPDFText_CountChars(current_text.get()), 0);
    current_text.reset();
    ASSERT_TRUE(api.FPDFPageObj_SetIsActive(image, 1));
    ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, 3, 2, 0xff0000ff));
    FPDF_PAGE pages[] = {page.get()};
    ASSERT_TRUE(api.FPDFImageObj_SetBitmap(pages, 1, image, bitmap.get()));
    ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*library, doc.get(), page.get(), artifacts,
        library->name() + "-regenerate", [](const Api& read, FPDF_PAGE loaded) {
          ASSERT_EQ(read.FPDFPage_CountObjects(loaded), 2);
          auto loaded_path = read.FPDFPage_GetObject(loaded, 0);
          ASSERT_EQ(read.FPDFPageObj_CountMarks(loaded_path), 1);
          int count = -1;
          ASSERT_TRUE(read.FPDFPageObjMark_GetParamIntValue(read.FPDFPageObj_GetMark(loaded_path, 0), "Count", &count));
          EXPECT_EQ(count, 3);
        }));
    results[backend++] = Render(api, page.get(), 128, 128);
    EXPECT_FALSE(api.FPDFPage_GenerateContent(nullptr));
    // Never-inserted objects exercise the caller-owned destruction paths.
    api.FPDFPageObj_Destroy(api.FPDFPageObj_CreateNewPath(0, 0));
    api.FPDFPageObj_Destroy(api.FPDFPageObj_NewTextObj(doc.get(), "Helvetica", 8));
    api.FPDFPageObj_Destroy(api.FPDFPageObj_NewImageObj(doc.get()));
  }
  CompareRaster(results[0], results[1], artifacts, "regenerated-edits");
}

TEST(Objects, InactiveObjectsRemainOwnedAndResumeVisualAndTextEffects) {
  Artifacts artifacts;
  const auto fixture = editing::AllObjectTypesFixture(false);
  std::array<Raster, 2> restored;
  editing::TextRegions regions;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    const auto before = Render(api, page.get(), 128, 128);
    int active_chars=0;
    { Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
      ASSERT_NE(text.get(),nullptr); active_chars=api.FPDFText_CountChars(text.get());
      EXPECT_GT(active_chars,0); }
    for (int index = 0; index < 4; ++index) ASSERT_TRUE(api.FPDFPageObj_SetIsActive(api.FPDFPage_GetObject(page.get(), index), 0));
    EXPECT_EQ(api.FPDFPage_CountObjects(page.get()), 4);
    { Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
      ASSERT_NE(text.get(),nullptr); EXPECT_EQ(api.FPDFText_CountChars(text.get()),0); }
    const auto hidden = Render(api, page.get(), 128, 128);
    EXPECT_NE(before.pixels, hidden.pixels);
    EXPECT_TRUE(std::all_of(hidden.pixels.begin(), hidden.pixels.end(), [](uint8_t value) { return value == 255; }));
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    const auto inactive = SaveDocument(api, doc.get(), artifacts, library->name() + "-inactive");
    for (const auto* reader : Implementations()) {
      const auto& read = reader->api();
      Handle loaded(read.FPDF_LoadMemDocument64(inactive.data(), inactive.size(), nullptr), read.FPDF_CloseDocument);
      ASSERT_NE(loaded.get(), nullptr);
      Handle p(read.FPDF_LoadPage(loaded.get(), 0), read.FPDF_ClosePage);
      ASSERT_NE(p.get(), nullptr);
      EXPECT_EQ(read.FPDFPage_CountObjects(p.get()), 0);
      CompareRaster(hidden, Render(read, p.get(), 128, 128), artifacts, library->name() + "-hidden-to-" + reader->name());
    }
    for (int index = 0; index < 4; ++index) ASSERT_TRUE(api.FPDFPageObj_SetIsActive(api.FPDFPage_GetObject(page.get(), index), 1));
    { Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);
      ASSERT_NE(text.get(),nullptr); EXPECT_EQ(api.FPDFText_CountChars(text.get()),active_chars); }
    regions = editing::TextBounds(api,page.get());
    restored[backend] = Render(api, page.get(), 128, 128);
    CompareRaster(before, restored[backend++], artifacts, library->name() + "-restored");
    ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*library, doc.get(), page.get(), artifacts,
        library->name() + "-active", [](const Api& read, FPDF_PAGE loaded) {
          EXPECT_EQ(read.FPDFPage_CountObjects(loaded), 4);
        }, 1));
  }
  editing::CompareScene(restored[0], restored[1], artifacts, "restored-backends", regions, 1);
}

TEST(Objects, NestedFormChildrenCanBeRemovedTransferredAndRegenerated) {
  Artifacts artifacts;
  const auto fixture = editing::AllObjectTypesFixture(false);
  std::array<Raster, 2> results;
  editing::TextRegions regions;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(), nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    auto form = api.FPDFPage_GetObject(page.get(), 3);
    auto unrelated = api.FPDFPage_GetObject(page.get(), 1);
    EXPECT_FALSE(api.FPDFFormObj_RemoveObject(form, unrelated));
    EXPECT_FALSE(api.FPDFFormObj_RemoveObject(nullptr, unrelated));
    EXPECT_FALSE(api.FPDFFormObj_RemoveObject(form, nullptr));
    // Remove middle text, first path and final nested form; leave one image.
    for (unsigned long index : {1ul, 0ul, 1ul}) {
      auto child = api.FPDFFormObj_GetObject(form, index);
      ASSERT_NE(child, nullptr);
      ASSERT_TRUE(api.FPDFFormObj_RemoveObject(form, child));
      api.FPDFPageObj_Destroy(child);
    }
    ASSERT_EQ(api.FPDFFormObj_CountObjects(form), 1);
    auto remaining = api.FPDFFormObj_GetObject(form, 0);
    EXPECT_EQ(api.FPDFPageObj_GetType(remaining), FPDF_PAGEOBJ_IMAGE);
    ASSERT_TRUE(api.FPDFFormObj_RemoveObject(form, remaining));
    ASSERT_EQ(api.FPDFFormObj_CountObjects(form), 0);
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), remaining));
    // Native mutates live ownership but regenerates /Contents on the form's
    // dictionary, leaving stale stream bytes on disk. Keep the live checks
    // above for both; Rust must preserve its edited rendering through reload.
    if (library->name() == "rust") {
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*library, doc.get(), page.get(), artifacts,
          library->name() + "-form-edits", [](const Api& read, FPDF_PAGE loaded) {
            ASSERT_EQ(read.FPDFPage_CountObjects(loaded), 5);
            EXPECT_EQ(read.FPDFFormObj_CountObjects(read.FPDFPage_GetObject(loaded, 3)), 0);
            EXPECT_EQ(read.FPDFPageObj_GetType(read.FPDFPage_GetObject(loaded, 4)), FPDF_PAGEOBJ_IMAGE);
          }, 1));
    }
    regions = editing::TextBounds(api,page.get());
    results[backend++] = Render(api, page.get(), 128, 128);
  }
  editing::CompareScene(results[0], results[1], artifacts, "form-removals", regions, 1);
}

TEST(Objects, AllBlendModesAndAlphaChangesRenderAndPersist) {
  Artifacts artifacts;
  const std::array<const char*, 16> names = {"Color", "ColorBurn", "ColorDodge", "Darken",
      "Difference", "Exclusion", "HardLight", "Hue", "Lighten", "Luminosity", "Multiply",
      "Normal", "Overlay", "Saturation", "Screen", "SoftLight"};
  for (const char* name : names) {
    std::array<Raster, 2> pictures;
    size_t backend = 0;
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name() + "-" + name);
      const auto& api = library->api();
      Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      auto bottom = Rectangle(api, 16, 16, 80, 80, 32, 96, 224);
      auto top = Rectangle(api, 40, 40, 72, 72, 224, 64, 32, 127);
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), bottom));
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), top));
      api.FPDFPageObj_SetBlendMode(top, name);
      EXPECT_TRUE(api.FPDFPageObj_HasTransparency(top));
      // Page-level reporting depends on parsed rendering resources; object
      // alpha is the immediate edit contract. Exercise the query without
      // requiring the native parser cache to update before regeneration.
      (void)api.FPDFPage_HasTransparency(page.get());
      pictures[backend] = Render(api, page.get(), 128, 128);
      ASSERT_FALSE(pictures[backend].pixels.empty());
      const size_t inside_bottom = (100 * 128 + 24) * 4;
      EXPECT_EQ(pictures[backend].pixels[inside_bottom], 224);
      EXPECT_EQ(pictures[backend].pixels[inside_bottom + 1], 96);
      EXPECT_EQ(pictures[backend].pixels[inside_bottom + 2], 32);
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*library, doc.get(), page.get(), artifacts,
          library->name() + "-blend-" + name, [](const Api& read, FPDF_PAGE loaded) {
            ASSERT_EQ(read.FPDFPage_CountObjects(loaded), 2);
            (void)read.FPDFPage_HasTransparency(loaded);
            EXPECT_TRUE(read.FPDFPageObj_HasTransparency(read.FPDFPage_GetObject(loaded, 1)));
          }, 1));
      ++backend;
      api.FPDFPageObj_SetBlendMode(top, "Normal");
      ASSERT_TRUE(api.FPDFPageObj_SetFillColor(top, 224, 64, 32, 255));
      EXPECT_FALSE(api.FPDFPageObj_HasTransparency(top));
      const auto normal = Render(api,page.get(),128,128);
      // Unknown mode names map to Normal in the native state parser.
      api.FPDFPageObj_SetBlendMode(top,"UnknownBlendMode");
      EXPECT_FALSE(api.FPDFPageObj_HasTransparency(top));
      CompareRaster(normal,Render(api,page.get(),128,128),artifacts,
                    library->name()+"-unknown-blend-"+name);
      (void)api.FPDFPage_HasTransparency(page.get());
      EXPECT_TRUE(api.FPDFPage_RemoveObject(page.get(), top));
      api.FPDFPageObj_Destroy(top);
      (void)api.FPDFPage_HasTransparency(page.get());
    }
    CompareRaster(pictures[0], pictures[1], artifacts, std::string("blend-") + name, 1);
  }
}

TEST(Objects, FillAndStrokeAlphaEndpointsAgreeWithPageTransparency) {
  Artifacts artifacts;
  for (unsigned int alpha : {0u, 1u, 127u, 254u, 255u}) {
    std::array<Raster, 2> pictures;
    size_t backend = 0;
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name() + "-alpha" + std::to_string(alpha));
      const auto& api = library->api();
      Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      auto path = Rectangle(api, 16, 16, 64, 64, 255, 0, 0, alpha);
      ASSERT_TRUE(api.FPDFPageObj_SetStrokeColor(path, 0, 0, 255, alpha));
      ASSERT_TRUE(api.FPDFPageObj_SetStrokeWidth(path, 4.5f));
      ASSERT_TRUE(api.FPDFPath_SetDrawMode(path, FPDF_FILLMODE_WINDING, 1));
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), path));
      EXPECT_EQ(!!api.FPDFPageObj_HasTransparency(path), alpha != 255);
      (void)api.FPDFPage_HasTransparency(page.get());
      pictures[backend++] = Render(api, page.get(), 128, 128);
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*library, doc.get(), page.get(), artifacts,
          library->name() + "-alpha" + std::to_string(alpha), [alpha](const Api& read, FPDF_PAGE loaded) {
            auto object = read.FPDFPage_GetObject(loaded, 0);
            unsigned int r = 0, g = 0, b = 0, a = 0;
            ASSERT_TRUE(read.FPDFPageObj_GetFillColor(object, &r, &g, &b, &a));
            EXPECT_EQ(r, 255u); EXPECT_EQ(g, 0u); EXPECT_EQ(b, 0u); EXPECT_EQ(a, alpha);
            ASSERT_TRUE(read.FPDFPageObj_GetStrokeColor(object, &r, &g, &b, &a));
            EXPECT_EQ(r, 0u); EXPECT_EQ(g, 0u); EXPECT_EQ(b, 255u); EXPECT_EQ(a, alpha);
          }, 1));
    }
    CompareRaster(pictures[0], pictures[1], artifacts, "alpha" + std::to_string(alpha), 1);
  }
}

TEST(Objects, EveryObjectCategorySupportsMatrixReplacementAndComposition) {
  Artifacts artifacts;
  const auto fixture = editing::AllObjectTypesFixture(false);
  const std::array<FS_MATRIX, 6> transforms = {{
      {1, 0, 0, 1, 0, 0}, {1, 0, 0, 1, 2.5f, 3.25f}, {0.5f, 0, 0, 0.75f, 0, 0},
      {0, 1, -1, 0, 128, 0}, {1, 0.125f, 0.25f, 1, 0, 0}, {-1, 0, 0, 1, 128, 0}}};
  for (size_t matrix_index = 0; matrix_index < transforms.size(); ++matrix_index) {
    std::array<Raster, 2> results;
    editing::TextRegions regions;
    size_t backend = 0;
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name() + "-matrix" + std::to_string(matrix_index));
      const auto& api = writer->api();
      Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
      Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      const auto& matrix = transforms[matrix_index];
      std::vector<FS_MATRIX> expected;
      for (int index = 0; index < 4; ++index) {
        auto object = api.FPDFPage_GetObject(page.get(), index);
        // Apply the same operation through both signatures, starting from a
        // restored original matrix. This checks float/double agreement.
        FS_MATRIX original{}; ASSERT_TRUE(api.FPDFPageObj_GetMatrix(object, &original));
        ASSERT_TRUE(api.FPDFPageObj_TransformF(object, &matrix));
        FS_MATRIX via_float{}; ASSERT_TRUE(api.FPDFPageObj_GetMatrix(object, &via_float));
        ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object, &original));
        api.FPDFPageObj_Transform(object, matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f);
        FS_MATRIX via_double{}; ASSERT_TRUE(api.FPDFPageObj_GetMatrix(object, &via_double));
        EXPECT_FLOAT_EQ(via_float.a, via_double.a); EXPECT_FLOAT_EQ(via_float.b, via_double.b);
        EXPECT_FLOAT_EQ(via_float.c, via_double.c); EXPECT_FLOAT_EQ(via_float.d, via_double.d);
        EXPECT_FLOAT_EQ(via_float.e, via_double.e); EXPECT_FLOAT_EQ(via_float.f, via_double.f);
        // A second translation does not commute with rotation/scaling. Check
        // the independently computed post-concatenation translation explicitly.
        const FS_MATRIX second{1,0,0,1,3,4};
        ASSERT_TRUE(api.FPDFPageObj_TransformF(object,&second));
        FS_MATRIX composed{}; ASSERT_TRUE(api.FPDFPageObj_GetMatrix(object,&composed));
        EXPECT_FLOAT_EQ(composed.a,via_double.a); EXPECT_FLOAT_EQ(composed.b,via_double.b);
        EXPECT_FLOAT_EQ(composed.c,via_double.c); EXPECT_FLOAT_EQ(composed.d,via_double.d);
        EXPECT_FLOAT_EQ(composed.e,via_double.e+3); EXPECT_FLOAT_EQ(composed.f,via_double.f+4);
        expected.push_back(composed);
        auto bounds = editing::Bounds(api, object);
        for (float value : bounds) EXPECT_TRUE(std::isfinite(value));
      }
      regions = editing::TextBounds(api,page.get());
    results[backend++] = Render(api, page.get(), 128, 128);
      ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*writer, doc.get(), page.get(), artifacts,
          writer->name() + "-matrix" + std::to_string(matrix_index),
          [expected](const Api& read, FPDF_PAGE loaded) {
            ASSERT_EQ(read.FPDFPage_CountObjects(loaded), 4);
            for (int index = 0; index < 4; ++index) {
              FS_MATRIX actual{};
              ASSERT_TRUE(read.FPDFPageObj_GetMatrix(read.FPDFPage_GetObject(loaded, index), &actual));
              EXPECT_NEAR(actual.a, expected[index].a, 0.0001f);
              EXPECT_NEAR(actual.b, expected[index].b, 0.0001f);
              EXPECT_NEAR(actual.c, expected[index].c, 0.0001f);
              EXPECT_NEAR(actual.d, expected[index].d, 0.0001f);
              EXPECT_NEAR(actual.e, expected[index].e, 0.0001f);
              EXPECT_NEAR(actual.f, expected[index].f, 0.0001f);
            }
          }, 1));
    }
    editing::CompareScene(results[0], results[1], artifacts, "category-transform" + std::to_string(matrix_index), regions, 1);
  }
}

TEST(Objects, RotatedTextAndImageQuadsUseFourOrderedCorners) {
  Artifacts artifacts;
  const auto fixture = editing::AllObjectTypesFixture();
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    for (int index : {2, 3}) {
      auto object = api.FPDFPage_GetObject(page.get(), index);
      const FS_MATRIX identity{1,0,0,1,0,0};
      ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object,&identity));
      const auto original = editing::Bounds(api,object);
      for (const FS_MATRIX matrix : {FS_MATRIX{24, 0, 0, 16, 40, 32}, {0, 24, -16, 0, 64, 32},
                                     {20, 10, 5, 12, 32, 32}, {-24, 0, 0, 16, 64, 32}}) {
        ASSERT_TRUE(api.FPDFPageObj_SetMatrix(object, &matrix));
        FS_QUADPOINTSF quad{};
        ASSERT_TRUE(api.FPDFPageObj_GetRotatedBounds(object, &quad));
        const auto bounds = editing::Bounds(api, object);
        const std::array<float, 4> xs = {quad.x1, quad.x2, quad.x3, quad.x4};
        const std::array<float, 4> ys = {quad.y1, quad.y2, quad.y3, quad.y4};
        const std::array<std::pair<float,float>,4> corners = {{{original[0],original[1]},
            {original[2],original[1]}, {original[2],original[3]}, {original[0],original[3]}}};
        for (size_t corner=0; corner<corners.size(); ++corner) {
          EXPECT_NEAR(xs[corner],matrix.a*corners[corner].first+matrix.c*corners[corner].second+matrix.e,.002f);
          EXPECT_NEAR(ys[corner],matrix.b*corners[corner].first+matrix.d*corners[corner].second+matrix.f,.002f);
        }
        EXPECT_NEAR(*std::min_element(xs.begin(), xs.end()), bounds[0], 0.002f);
        EXPECT_NEAR(*std::max_element(xs.begin(), xs.end()), bounds[2], 0.002f);
        EXPECT_NEAR(*std::min_element(ys.begin(), ys.end()), bounds[1], 0.002f);
        EXPECT_NEAR(*std::max_element(ys.begin(), ys.end()), bounds[3], 0.002f);
      }
      EXPECT_FALSE(api.FPDFPageObj_GetRotatedBounds(object, nullptr));
    }
    FS_QUADPOINTSF sentinel{};
    for (auto object : {FPDF_PAGEOBJECT{}, api.FPDFPage_GetObject(page.get(), 0),
                         api.FPDFPage_GetObject(page.get(), 1), api.FPDFPage_GetObject(page.get(), 4)}) {
      EXPECT_FALSE(api.FPDFPageObj_GetRotatedBounds(object, &sentinel));
    }
    std::array<float, 4> bounds = {91, 92, 93, 94};
    EXPECT_FALSE(api.FPDFPageObj_GetBounds(nullptr, &bounds[0], &bounds[1], &bounds[2], &bounds[3]));
    EXPECT_EQ(bounds, (std::array<float, 4>{91, 92, 93, 94}));
  }
}

TEST(Objects, TransformAnnotationsChangesRectsWithoutMovingPageObjects) {
  Artifacts artifacts;
  for (const FS_MATRIX matrix : {FS_MATRIX{1, 0, 0, 1, 8, 12}, {0.5f, 0, 0, 0.75f, 0, 0},
                                  {0, 1, -1, 0, 128, 0}}) {
    std::array<Raster, 2> pictures;
    size_t backend = 0;
    for (const auto* library : Implementations()) {
      const auto& api = library->api();
      Handle doc(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
      Handle page(api.FPDFPage_New(doc.get(), 0, 128, 128), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      auto object = Rectangle(api, 0, 0, 8, 8, 0, 0, 255);
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), object));
      const auto original_bounds = editing::Bounds(api, object);
      api.FPDFPage_TransformAnnots(page.get(), matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f);
      for (int index = 0; index < 3; ++index) {
        Handle annot(api.FPDFPage_CreateAnnot(page.get(), FPDF_ANNOT_SQUARE), api.FPDFPage_CloseAnnot);
        ASSERT_NE(annot.get(), nullptr);
        FS_RECTF rect{float(16 + index * 24), 96, float(32 + index * 24), 80};
        ASSERT_TRUE(api.FPDFAnnot_SetRect(annot.get(), &rect));
        ASSERT_TRUE(api.FPDFAnnot_SetColor(annot.get(), FPDFANNOT_COLORTYPE_Color, 255, 0, 0, 255));
        ASSERT_TRUE(api.FPDFAnnot_SetBorder(annot.get(), 0, 0, 2));
      }
      api.FPDFPage_TransformAnnots(page.get(), matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f);
      EXPECT_EQ(editing::Bounds(api, object), original_bounds);
      for (int index = 0; index < 3; ++index) {
        Handle annot(api.FPDFPage_GetAnnot(page.get(), index), api.FPDFPage_CloseAnnot);
        ASSERT_NE(annot.get(), nullptr);
        FS_RECTF rect{};
        ASSERT_TRUE(api.FPDFAnnot_GetRect(annot.get(), &rect));
        const float left = float(16 + index * 24), right = float(32 + index * 24);
        std::array<float, 4> xs = {matrix.a * left + matrix.c * 80 + matrix.e,
            matrix.a * left + matrix.c * 96 + matrix.e,
            matrix.a * right + matrix.c * 80 + matrix.e,
            matrix.a * right + matrix.c * 96 + matrix.e};
        std::array<float, 4> ys = {matrix.b * left + matrix.d * 80 + matrix.f,
            matrix.b * left + matrix.d * 96 + matrix.f,
            matrix.b * right + matrix.d * 80 + matrix.f,
            matrix.b * right + matrix.d * 96 + matrix.f};
        EXPECT_NEAR(rect.left, *std::min_element(xs.begin(), xs.end()), .001);
        EXPECT_NEAR(rect.right, *std::max_element(xs.begin(), xs.end()), .001);
        EXPECT_NEAR(rect.bottom, *std::min_element(ys.begin(), ys.end()), .001);
        EXPECT_NEAR(rect.top, *std::max_element(ys.begin(), ys.end()), .001);
      }
      pictures[backend++] = Render(api, page.get(), 128, 128, FPDF_ANNOT);
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      const auto bytes = SaveDocument(api, doc.get(), artifacts, library->name() + "-transformed-annots");
      for (const auto* reader : Implementations()) {
        const auto& read = reader->api();
        Handle loaded(read.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), read.FPDF_CloseDocument);
        Handle p(read.FPDF_LoadPage(loaded.get(), 0), read.FPDF_ClosePage);
        ASSERT_NE(p.get(), nullptr); EXPECT_EQ(read.FPDFPage_GetAnnotCount(p.get()), 3);
        CompareRaster(pictures[backend - 1], Render(read, p.get(), 128, 128, FPDF_ANNOT), artifacts,
                      library->name() + "-annots-to-" + reader->name(), 1);
      }
    }
    CompareRaster(pictures[0], pictures[1], artifacts, "transformed-annots", 1);
  }
}

TEST(Objects, TilingPatternBitmapsUseDistinctFillStrokeColorsAndRejectShadingPatterns) {
  Artifacts artifacts;
  for (bool uncolored : {false, true}) {
    const auto make_fixture = [&](bool common_stroke_color) { return editing::SceneFixture(
        uncolored
            ? std::string("/PC cs 0 0 1 /P scn /PC CS ") +
                  (common_stroke_color ? "0 0 1" : "1 0 0") + " /P SCN 8 w 16 16 64 64 re B"
            : "/Pattern cs /P scn /Pattern CS /Q SCN 8 w 16 16 64 64 re B",
        std::string("/Pattern << /P 5 0 R /Q 6 0 R >> ") + (uncolored ? "/ColorSpace << /PC [/Pattern /DeviceRGB] >>" : ""),
        {PdfStream(std::string("/Type /Pattern /PatternType 1 /PaintType ") + (uncolored ? "2" : "1") +
                    " /TilingType 1 /BBox [0 0 8 8] /XStep 8 /YStep 8 /Resources << >>",
                    uncolored ? "0 0 4 8 re f" : "0 0 1 rg 0 0 4 8 re f"),
         PdfStream("/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 "
                   "/BBox [0 0 8 8] /XStep 8 /YStep 8 /Matrix [1 0 0 1 2 0] /Resources << >>",
                   "1 0 0 rg 0 0 4 8 re f")}); };
    const auto fixture = make_fixture(false);
    std::array<Raster, 2> fills, strokes, pages, common_pages;
    size_t backend = 0;
    for (const auto* library : Implementations()) {
      const auto& api = library->api();
      Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
      Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      auto object = api.FPDFPage_GetObject(page.get(), 0); ASSERT_NE(object, nullptr);
      unsigned int r=91,g=92,b=93,a=94;
      // The public getter returns cached RGB even for patterns; it does not
      // expose the pattern's painted colors through this scalar API.
      EXPECT_TRUE(api.FPDFPageObj_GetFillColor(object,&r,&g,&b,&a)); EXPECT_EQ(a,255u);
      EXPECT_TRUE(api.FPDFPageObj_GetStrokeColor(object,&r,&g,&b,&a)); EXPECT_EQ(a,255u);
      Handle fill(api.FPDFPageObj_GetRenderedFillPattern(doc.get(), object), api.FPDFBitmap_Destroy);
      Handle stroke(api.FPDFPageObj_GetRenderedStrokePattern(doc.get(), object), api.FPDFBitmap_Destroy);
      ASSERT_NE(fill.get(), nullptr); ASSERT_NE(stroke.get(), nullptr);
      auto copy = [](const Api& current, FPDF_BITMAP bitmap) {
        Raster result{current.FPDFBitmap_GetWidth(bitmap), current.FPDFBitmap_GetHeight(bitmap), {}};
        EXPECT_EQ(result.width, 8); EXPECT_EQ(result.height, 8);
        const auto* data = static_cast<const uint8_t*>(current.FPDFBitmap_GetBuffer(bitmap));
        const int stride = current.FPDFBitmap_GetStride(bitmap);
        const int format = current.FPDFBitmap_GetFormat(bitmap);
        const int bpp = format == FPDFBitmap_BGRA || format == FPDFBitmap_BGRx ? 4 : 3;
        for (int y = 0; y < result.height; ++y) for (int x = 0; x < result.width; ++x) {
          const auto* p = data + y * stride + x * bpp;
          result.pixels.insert(result.pixels.end(), {p[0], p[1], p[2], static_cast<uint8_t>(format == FPDFBitmap_BGRA ? p[3] : 255)});
        }
        return result;
      };
      fills[backend] = copy(api,fill.get()); strokes[backend] = copy(api,stroke.get());
      ASSERT_EQ(fills[backend].pixels.size(), 8u * 8 * 4);
      const size_t painted = (4 * 8 + 1) * 4;
      EXPECT_EQ(fills[backend].pixels[painted], 255);
      EXPECT_EQ(fills[backend].pixels[painted + 1], 0);
      EXPECT_EQ(fills[backend].pixels[painted + 2], 0);
      EXPECT_EQ(fills[backend].pixels[painted + 3], 255);
      if (uncolored) {
        EXPECT_EQ(strokes[backend].pixels[painted], 0);
        EXPECT_EQ(strokes[backend].pixels[painted + 2], 255);
      }
      pages[backend] = Render(api, page.get(), 128, 128);
      if (uncolored) {
        // Native cached stroke cells select fill RGB. Keep the original red
        // stroke assertion for Rust, and compare geometry through a public
        // fixture whose fill/stroke colors coincide in both engines.
        const size_t pixel = (114 * 128 + 17) * 4;
        const bool native = library->name() == "native";
        EXPECT_EQ(pages[backend].pixels[pixel], native ? 255 : 0);
        EXPECT_EQ(pages[backend].pixels[pixel + 1], 0);
        EXPECT_EQ(pages[backend].pixels[pixel + 2], native ? 0 : 255);
        EXPECT_EQ(pages[backend].pixels[pixel + 3], 255);
        const auto common = make_fixture(true);
        Handle control(api.FPDF_LoadMemDocument64(common.data(), common.size(), nullptr), api.FPDF_CloseDocument);
        ASSERT_NE(control.get(), nullptr);
        Handle control_page(api.FPDF_LoadPage(control.get(), 0), api.FPDF_ClosePage);
        ASSERT_NE(control_page.get(), nullptr);
        common_pages[backend] = Render(api, control_page.get(), 128, 128);
        if (native) CompareRaster(pages[backend], common_pages[backend], artifacts, "native-fill-colored-stroke");
      } else common_pages[backend] = pages[backend];
      ++backend;
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      const auto bytes=SaveDocument(api,doc.get(),artifacts,library->name()+"-patterns"+std::to_string(uncolored));
      for(const auto* reader:Implementations()) {
        const auto& read=reader->api();
        Handle loaded(read.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),read.FPDF_CloseDocument);
        ASSERT_NE(loaded.get(),nullptr);
        Handle p(read.FPDF_LoadPage(loaded.get(),0),read.FPDF_ClosePage);
        ASSERT_NE(p.get(),nullptr); ASSERT_EQ(read.FPDFPage_CountObjects(p.get()),1);
        auto object=read.FPDFPage_GetObject(p.get(),0);
        Handle fill_bitmap(read.FPDFPageObj_GetRenderedFillPattern(loaded.get(),object),read.FPDFBitmap_Destroy);
        Handle stroke_bitmap(read.FPDFPageObj_GetRenderedStrokePattern(loaded.get(),object),read.FPDFBitmap_Destroy);
        ASSERT_NE(fill_bitmap.get(),nullptr); ASSERT_NE(stroke_bitmap.get(),nullptr);
        CompareRaster(fills[backend-1],copy(read,fill_bitmap.get()),artifacts,library->name()+"-fill-to-"+reader->name());
        CompareRaster(strokes[backend-1],copy(read,stroke_bitmap.get()),artifacts,library->name()+"-stroke-to-"+reader->name());
        if (uncolored && reader->name() != library->name()) {
          // Compare each reader with that reader's original scene, preserving
          // both writers' original distinct-color pattern dictionaries.
          Handle original(read.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), read.FPDF_CloseDocument);
          ASSERT_NE(original.get(), nullptr);
          Handle original_page(read.FPDF_LoadPage(original.get(), 0), read.FPDF_ClosePage);
          ASSERT_NE(original_page.get(), nullptr);
          CompareRaster(Render(read, original_page.get(), 128, 128), Render(read, p.get(), 128, 128), artifacts,
                        library->name()+"-pattern-page-to-"+reader->name());
        } else CompareRaster(pages[backend-1],Render(read,p.get(),128,128),artifacts,library->name()+"-pattern-page-to-"+reader->name());
      }
      Handle plain(api.FPDFPageObj_CreateNewRect(0, 0, 8, 8), api.FPDFPageObj_Destroy);
      EXPECT_EQ(api.FPDFPageObj_GetRenderedFillPattern(doc.get(), plain.get()), nullptr);
      EXPECT_EQ(api.FPDFPageObj_GetRenderedStrokePattern(doc.get(), plain.get()), nullptr);
      EXPECT_EQ(api.FPDFPageObj_GetRenderedFillPattern(nullptr, object), nullptr);
      EXPECT_EQ(api.FPDFPageObj_GetRenderedStrokePattern(doc.get(), nullptr), nullptr);
    }
    CompareRaster(fills[0], fills[1], artifacts, "pattern-fill" + std::to_string(uncolored));
    CompareRaster(strokes[0], strokes[1], artifacts, "pattern-stroke" + std::to_string(uncolored));
    CompareRaster(common_pages[0], common_pages[1], artifacts, "pattern-page" + std::to_string(uncolored));
  }
  const auto shading_pattern = editing::SceneFixture(
      "/Pattern cs /P scn /Pattern CS /P SCN 8 w 16 16 64 64 re B",
      "/Pattern << /P 5 0 R >>",
      {"<< /Type /Pattern /PatternType 2 /Shading 6 0 R >>",
       "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 128 0] /Extend [true true] "
       "/Function << /FunctionType 2 /Domain [0 1] /C0 [0 0 1] /C1 [1 0 0] /N 1 >> >>"});
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle doc(api.FPDF_LoadMemDocument64(shading_pattern.data(), shading_pattern.size(), nullptr), api.FPDF_CloseDocument);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    auto object = api.FPDFPage_GetObject(page.get(), 0); ASSERT_NE(object, nullptr);
    EXPECT_EQ(api.FPDFPageObj_GetRenderedFillPattern(doc.get(), object), nullptr);
    EXPECT_EQ(api.FPDFPageObj_GetRenderedStrokePattern(doc.get(), object), nullptr);
  }
}

TEST(Objects, LoadedGrayRgbCmykFillStrokeColorsConvertIndependently) {
  Artifacts artifacts;
  const auto fixture = editing::SceneFixture(
      "0.25 g 0.75 G 4 w 8 8 24 24 re B "
      "1 0 0 rg 0 0 1 RG 40 8 24 24 re B "
      "0 1 1 0 k 1 0 0 0 K 72 8 24 24 re B");
  std::array<Raster, 2> pictures;
  std::array<std::vector<unsigned int>, 2> observations;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr); ASSERT_EQ(api.FPDFPage_CountObjects(page.get()), 3);
    for (int index = 0; index < 3; ++index) {
      auto object = api.FPDFPage_GetObject(page.get(), index);
      std::array<unsigned int, 4> fill{}, stroke{};
      ASSERT_TRUE(api.FPDFPageObj_GetFillColor(object, &fill[0], &fill[1], &fill[2], &fill[3]));
      ASSERT_TRUE(api.FPDFPageObj_GetStrokeColor(object, &stroke[0], &stroke[1], &stroke[2], &stroke[3]));
      EXPECT_EQ(fill[3], 255u); EXPECT_EQ(stroke[3], 255u);
      if (index == 0) {
        EXPECT_EQ(fill[0], fill[1]); EXPECT_EQ(fill[1], fill[2]);
        EXPECT_EQ(stroke[0], stroke[1]); EXPECT_EQ(stroke[1], stroke[2]);
        EXPECT_NEAR(fill[0], 64, 1); EXPECT_NEAR(stroke[0], 191, 1);
      } else if (index == 1) {
        EXPECT_EQ(fill, (std::array<unsigned int, 4>{255, 0, 0, 255}));
        EXPECT_EQ(stroke, (std::array<unsigned int, 4>{0, 0, 255, 255}));
      } else {
        EXPECT_GT(fill[0], fill[1]); EXPECT_GT(fill[0], fill[2]);
        EXPECT_GT(stroke[1], stroke[0]); EXPECT_GT(stroke[2], stroke[0]);
      }
      observations[backend].insert(observations[backend].end(), fill.begin(), fill.end());
      observations[backend].insert(observations[backend].end(), stroke.begin(), stroke.end());
    }
    pictures[backend++] = Render(api, page.get(), 128, 128);
    ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*library, doc.get(), page.get(), artifacts,
        library->name() + "-loaded-color-spaces", [](const Api& read, FPDF_PAGE loaded) {
          EXPECT_EQ(read.FPDFPage_CountObjects(loaded), 3);
        }, 1));
  }
  ASSERT_EQ(observations[0].size(), observations[1].size());
  for (size_t index = 0; index < observations[0].size(); ++index) EXPECT_NEAR(observations[0][index], observations[1][index], 1);
  CompareRaster(pictures[0], pictures[1], artifacts, "color-space-conversion", 1);
}

TEST(Objects, ParsedNestedTransparencyResourcesRemainVisibleAfterReload) {
  Artifacts artifacts;
  const auto fixture = editing::SceneFixture(
      "0 0 1 rg 8 8 96 96 re f q 1 0 0 1 16 16 cm /F Do Q",
      "/XObject << /F 5 0 R >>",
      {PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 80 80] "
                 "/Group << /S /Transparency /I true >> "
                 "/Resources << /ExtGState << /Alpha << /ca 0.5 /CA 0.5 /BM /Screen >> >> >>",
                 "/Alpha gs 1 0 0 rg 0 0 80 80 re f")});
  std::array<Raster, 2> pictures;
  std::array<FPDF_BOOL, 2> page_reports{};
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    const auto& api = writer->api();
    Handle doc(api.FPDF_LoadMemDocument64(fixture.data(), fixture.size(), nullptr), api.FPDF_CloseDocument);
    Handle page(api.FPDF_LoadPage(doc.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr); ASSERT_EQ(api.FPDFPage_CountObjects(page.get()), 2);
    auto form = api.FPDFPage_GetObject(page.get(), 1);
    EXPECT_TRUE(api.FPDFPageObj_HasTransparency(form));
    ASSERT_EQ(api.FPDFFormObj_CountObjects(form), 1);
    EXPECT_TRUE(api.FPDFPageObj_HasTransparency(api.FPDFFormObj_GetObject(form, 0)));
    page_reports[backend] = api.FPDFPage_HasTransparency(page.get());
    EXPECT_TRUE(page_reports[backend]);
    pictures[backend++] = Render(api, page.get(), 128, 128);
    ASSERT_NO_FATAL_FAILURE(editing::RoundTrip(*writer, doc.get(), page.get(), artifacts,
        writer->name() + "-nested-transparency", [](const Api& read, FPDF_PAGE p) {
          ASSERT_EQ(read.FPDFPage_CountObjects(p), 2);
          EXPECT_TRUE(read.FPDFPageObj_HasTransparency(read.FPDFPage_GetObject(p, 1)));
        }, 1));
  }
  EXPECT_EQ(!!page_reports[0], !!page_reports[1]);
  CompareRaster(pictures[0], pictures[1], artifacts, "nested-transparency", 1);
}

}  // namespace
}  // namespace pdfoxium::integration
