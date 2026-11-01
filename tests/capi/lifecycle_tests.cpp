#include "lifecycle_scenarios.h"
#include "runtime_fixture.h"

#include <iostream>

namespace pdfoxium::integration {
namespace {

void LifecycleScene(const Api& api, Raster* raster) {
  Handle first(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
  Handle second(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
  ASSERT_NE(first.get(), nullptr); ASSERT_NE(second.get(), nullptr);
  EXPECT_EQ(api.FPDF_GetPageCount(first.get()), 0);
  EXPECT_EQ(api.FPDF_GetPageCount(second.get()), 0);
  {
    Handle page(api.FPDFPage_New(first.get(), 0, 128, 96), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    Handle rectangle(api.FPDFPageObj_CreateNewRect(16, 16, 64, 48), api.FPDFPageObj_Destroy);
    ASSERT_NE(rectangle.get(), nullptr);
    ASSERT_TRUE(api.FPDFPageObj_SetFillColor(rectangle.get(), 64, 128, 191, 255));
    ASSERT_TRUE(api.FPDFPath_SetDrawMode(rectangle.get(), FPDF_FILLMODE_WINDING, 0));
    ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), rectangle.release()));
    ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
    *raster = Render(api, page.get(), 128, 96);
    runtime_test::ExpectRectangle(*raster);
  }
  first.reset();
  EXPECT_EQ(api.FPDF_GetPageCount(second.get()), 0);
  Handle survivor(api.FPDFPage_New(second.get(), 0, 64, 32), api.FPDF_ClosePage);
  ASSERT_NE(survivor.get(), nullptr);
  EXPECT_EQ(api.FPDF_GetPageCount(second.get()), 1);
}

std::vector<uint8_t> ScriptIsolationFixture() {
  // No public API constructs AcroForm trees/document-level script name trees.
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 5 0 R /Names << /JavaScript << /Names [(counter) 6 0 R] >> >> >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Annots [4 0 R] /Contents 7 0 R /Resources << /Font << /F1 8 0 R >> >> >>",
      "<< /Type /Annot /Subtype /Widget /FT /Tx /T (v) /V () /Rect [8 8 80 24] /DA (/F1 12 Tf 0 g) /P 3 0 R >>",
      "<< /Fields [4 0 R] /DA (/F1 12 Tf 0 g) /DR << /Font << /F1 8 0 R >> >> >>",
      "<< /S /JavaScript /JS 9 0 R >>",
      PdfStream("", "0.25 0.5 0.75 rg 16 32 64 48 re f\n"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
      PdfStream("", "var localCount = (typeof localCount === 'undefined' ? 0 : localCount) + 1; this.getField('v').value = String(localCount);")});
}

void CheckScriptIsolation(const Api& api) {
  const auto bytes = ScriptIsolationFixture();
  Handle first(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
  Handle second(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
  ASSERT_NE(first.get(), nullptr); ASSERT_NE(second.get(), nullptr);
  IPDF_JSPLATFORM platform{}; platform.version = 3;
  FPDF_FORMFILLINFO info{}; info.version = 2; info.m_pJsPlatform = &platform;
  Handle first_form(api.FPDFDOC_InitFormFillEnvironment(first.get(), &info), api.FPDFDOC_ExitFormFillEnvironment);
  Handle second_form(api.FPDFDOC_InitFormFillEnvironment(second.get(), &info), api.FPDFDOC_ExitFormFillEnvironment);
  ASSERT_NE(first_form.get(), nullptr); ASSERT_NE(second_form.get(), nullptr);
  for (const auto& entry : {std::pair<FPDF_DOCUMENT, FPDF_FORMHANDLE>{first.get(), first_form.get()}, {second.get(), second_form.get()}}) {
    Handle page(api.FPDF_LoadPage(entry.first, 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    api.FORM_OnAfterLoadPage(page.get(), entry.second);
    api.FORM_DoDocumentJSAction(entry.second);
    {
      Handle field(api.FPDFPage_GetAnnot(page.get(), 0), api.FPDFPage_CloseAnnot);
      ASSERT_NE(field.get(), nullptr);
      std::array<unsigned short, 8> value{};
      EXPECT_EQ(api.FPDFAnnot_GetFormFieldValue(entry.second, field.get(), value.data(), sizeof(value)), 4u);
      EXPECT_EQ(value[0], static_cast<unsigned short>('1'));
      EXPECT_EQ(value[1], 0);
    }
    api.FORM_OnBeforeClosePage(page.get(), entry.second);
  }
}

void CheckBrotliConfiguration(const Api& api, bool enabled, bool native) {
  const auto regular_bytes = runtime_test::Resource("rectangles.pdf");
  const auto brotli_bytes = runtime_test::Resource("rectangles_brotli.pdf");
  Handle regular(api.FPDF_LoadMemDocument64(regular_bytes.data(), regular_bytes.size(), nullptr), api.FPDF_CloseDocument);
  Handle compressed(api.FPDF_LoadMemDocument64(brotli_bytes.data(), brotli_bytes.size(), nullptr), api.FPDF_CloseDocument);
  ASSERT_NE(regular.get(), nullptr); ASSERT_NE(compressed.get(), nullptr);
  Raster expected, actual;
  ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, regular.get(), &expected));
  ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api, compressed.get(), &actual));
  // The distributed native oracle is built without PDF_ENABLE_BROTLI.
  // Configuration remains valid there but does not add an absent codec.
  if (enabled && !native) { EXPECT_EQ(actual.pixels, expected.pixels); }
  else { EXPECT_TRUE(std::all_of(actual.pixels.begin(), actual.pixels.end(), [](uint8_t value) { return value == 255; })); }
}
}  // namespace

int RunLifecycleScenario(const std::filesystem::path& native_path,
                         const std::filesystem::path& rust_path,
                         int version, bool brotli, bool isolate) {
  Library native("native", native_path), rust("rust", rust_path);
  const std::string font_path = (TestRoot() / "upstream/pdfium/testing/resources/fonts/ahem").u8string();
  const char* font_paths[] = {font_path.c_str(), nullptr};
  FPDF_LIBRARY_CONFIG config{};
  config.version = version;
  config.m_pUserFontPaths = font_paths;
  config.m_RendererType = FPDF_RENDERERTYPE_AGG;
  // The oracle supports its default FreeType backend. Rust keeps its selected
  // Fontations implementation rather than requiring native backend internals.
  config.m_FontLibraryType = FPDF_FONTBACKENDTYPE_FREETYPE;
  config.m_BrotliEnabled = brotli;
  config.m_IsolatePerDocument = isolate;
  if (version) {
    native.api().FPDF_InitLibraryWithConfig(&config);
    config.m_FontLibraryType = FPDF_FONTBACKENDTYPE_FONTATIONS;
    rust.api().FPDF_InitLibraryWithConfig(&config);
  } else {
    native.api().FPDF_InitLibrary();
    rust.api().FPDF_InitLibrary();
  }
  struct Teardown { const Library& native; const Library& rust; ~Teardown() { rust.api().FPDF_DestroyLibrary(); native.api().FPDF_DestroyLibrary(); } } teardown{native, rust};
  std::array<Raster, 2> images;
  size_t backend = 0;
  for (const auto* library : {&native, &rust}) {
    std::cout << "Lifecycle " << library->name() << " config=" << version << " brotli=" << brotli << " isolate=" << isolate << '\n';
    LifecycleScene(library->api(), &images[backend++]);
    if (version >= 6) CheckBrotliConfiguration(library->api(), brotli, library->name() == "native");
    if (version >= 7) CheckScriptIsolation(library->api());
  }
  EXPECT_EQ(images[0].pixels, images[1].pixels);
  return ::testing::UnitTest::GetInstance()->ad_hoc_test_result().Failed() ? 1 : 0;
}

}  // namespace pdfoxium::integration
