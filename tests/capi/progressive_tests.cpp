#include "runtime_fixture.h"

#include <array>
#include <sstream>

namespace pdfoxium::integration {
namespace {

struct Pause : IFSDK_PAUSE {
  Pause(const Api& api, FPDF_PAGE page, bool request) : api(api), page(page), request(request) {
    version = 1;
    user = this;
    NeedToPauseNow = NeedPause;
  }
  static FPDF_BOOL NeedPause(IFSDK_PAUSE* descriptor) noexcept {
    auto* self = static_cast<Pause*>(descriptor);
    ++self->calls;
    self->identity &= descriptor->user == self;
    self->reentry &= self->api.FPDF_GetPageWidthF(self->page) > 0;
    return self->request;
  }
  const Api& api;
  FPDF_PAGE page;
  bool request;
  size_t calls = 0;
  bool identity = true;
  bool reentry = true;
};

struct RenderJob {
  RenderJob(const Api& api, FPDF_PAGE page) : api(api), page(page) {}
  ~RenderJob() { api.FPDF_RenderPage_Close(page); }
  const Api& api;
  FPDF_PAGE page;
};

TEST(Progressive, PausesAccumulateTranslucentPixelsAndCancellationCanRestart) {
  Artifacts artifacts;
  std::array<Raster, 2> completed;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDFPage_New(document.get(), 0, 250, 1), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    for (int index = 0; index < 220; ++index) {
      Handle rectangle(api.FPDFPageObj_CreateNewRect(static_cast<float>(index), 0, 1, 1), api.FPDFPageObj_Destroy);
      ASSERT_NE(rectangle.get(), nullptr);
      ASSERT_TRUE(api.FPDFPageObj_SetFillColor(rectangle.get(), 255, 0, 0, 128));
      ASSERT_TRUE(api.FPDFPath_SetDrawMode(rectangle.get(), FPDF_FILLMODE_WINDING, 0));
      ASSERT_TRUE(api.FPDFPage_InsertObject(page.get(), rectangle.release()));
    }
    const auto expected = Render(api, page.get(), 250, 1);
    for (bool cancel_first : {false, true}) {
      // Retain caller-owned pixels (including padding and guards) until the
      // progressive job and its borrowing bitmap have both closed.
      std::vector<uint8_t> backing(1010, 0xa5);
      Handle bitmap(api.FPDFBitmap_CreateEx(250, 1, FPDFBitmap_BGRA, backing.data() + 4, 1004), api.FPDFBitmap_Destroy);
      ASSERT_NE(bitmap.get(), nullptr);
      ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, 250, 1, 0xffffffff));
      Pause pause(api, page.get(), true);
      RenderJob job(api, page.get());
      int status = api.FPDF_RenderPageBitmap_Start(bitmap.get(), page.get(), 0, 0, 250, 1, 0, FPDF_NO_NATIVETEXT, &pause);
      ASSERT_EQ(status, FPDF_RENDER_TOBECONTINUED);
      const auto first = runtime_test::Bitmap(api, bitmap.get());
      EXPECT_NE(first.pixels, expected.pixels);
      EXPECT_EQ(first.pixels[0], expected.pixels[0]);
      EXPECT_EQ(first.pixels[1], expected.pixels[1]);
      if (cancel_first) {
        api.FPDF_RenderPage_Close(page.get());
        ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, 250, 1, 0xffffffff));
        pause.request = false;
        status = api.FPDF_RenderPageBitmap_Start(bitmap.get(), page.get(), 0, 0, 250, 1, 0, FPDF_NO_NATIVETEXT, &pause);
      } else {
        status = api.FPDF_RenderPage_Continue(page.get(), &pause);
        EXPECT_EQ(status, FPDF_RENDER_TOBECONTINUED);
        const auto second = runtime_test::Bitmap(api, bitmap.get());
        EXPECT_EQ(second.pixels[0], first.pixels[0]);
        EXPECT_EQ(second.pixels[1], first.pixels[1]);
      }
      for (int iteration = 0; iteration < 30 && status == FPDF_RENDER_TOBECONTINUED; ++iteration) {
        // Cover both retained callback pauses and uninterrupted continuation.
        if (iteration % 2 && library->name() == "rust") {
          status = api.FPDF_RenderPage_Continue(page.get(), nullptr);
        } else {
          // The pinned native wrapper rejects NULL despite its public header.
          // A live non-pausing callback tests uninterrupted native progress;
          // Rust separately exercises the documented NULL continuation.
          const bool requested = pause.request;
          if (iteration % 2) pause.request = false;
          status = api.FPDF_RenderPage_Continue(page.get(), &pause);
          pause.request = requested;
        }
      }
      ASSERT_EQ(status, FPDF_RENDER_DONE);
      EXPECT_GT(pause.calls, 0u);
      EXPECT_TRUE(pause.identity);
      EXPECT_TRUE(pause.reentry);
      const auto actual = runtime_test::Bitmap(api, bitmap.get());
      EXPECT_TRUE(std::all_of(backing.begin(), backing.begin() + 4, [](uint8_t value) { return value == 0xa5; }));
      EXPECT_TRUE(std::all_of(backing.begin() + 1004, backing.end(), [](uint8_t value) { return value == 0xa5; }));
      CompareRaster(expected, actual, artifacts, library->name() + (cancel_first ? "-restart" : "-continue"));
      completed[backend] = actual;
    }
    ++backend;
  }
  CompareRaster(completed[0], completed[1], artifacts, "progressive-backends");
}

std::vector<uint8_t> MultiphaseScene() {
  // A fixture adds noninteractive square and widget annotations: ANNOT draws
  // the square but leaves interactive widgets to the form-fill API.
  std::string content;
  for (int index = 0; index < 240; ++index) {
    content += "0.1 0.3 0.7 rg " + std::to_string(index % 24 * 5) + " " +
               std::to_string(index / 24 * 5) + " 4 4 re f\n";
  }
  content += "0 0 0 rg BT /F1 12 Tf 8 72 Td (Progressive) Tj ET\nq 12 0 0 12 100 70 cm /Im Do Q\n";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << /Font << /F1 5 0 R >> /XObject << /Im 6 0 R >> >> /Contents 4 0 R /Annots [7 0 R 8 0 R] >>",
      PdfStream("", content),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
      PdfStream("/Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceRGB", std::string("\xff\0\0\0\xff\0\0\0\xff\xff\xff\xff", 12)),
      "<< /Type /Annot /Subtype /Square /Rect [80 52 96 68] /C [1 0 0] /IC [0 1 0] /F 4 >>",
      "<< /Type /Annot /Subtype /Widget /Rect [30 52 60 66] /FT /Tx /T (widget) /V (hidden) /F 4 >>"});
}

TEST(Progressive, MultiphaseViewportsFlagsAndNullSchemeMatchSynchronousRendering) {
  Artifacts artifacts;
  const auto bytes = MultiphaseScene();
  std::array<std::vector<Raster>, 2> observations;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    for (int rotate = 0; rotate < 4; ++rotate) {
      for (int flags : {0, FPDF_ANNOT, FPDF_REVERSE_BYTE_ORDER,
                        FPDF_ANNOT | FPDF_RENDER_NO_SMOOTHTEXT | FPDF_RENDER_NO_SMOOTHIMAGE | FPDF_RENDER_NO_SMOOTHPATH}) {
        Handle expected(api.FPDFBitmap_Create(160, 128, 1), api.FPDFBitmap_Destroy);
        Handle actual(api.FPDFBitmap_Create(160, 128, 1), api.FPDFBitmap_Destroy);
        ASSERT_NE(expected.get(), nullptr);
        ASSERT_NE(actual.get(), nullptr);
        ASSERT_TRUE(api.FPDFBitmap_FillRect(expected.get(), 0, 0, 160, 128, 0xffffffff));
        ASSERT_TRUE(api.FPDFBitmap_FillRect(actual.get(), 0, 0, 160, 128, 0xffffffff));
        api.FPDF_RenderPageBitmap(expected.get(), page.get(), 7, 11, 144, 108, rotate, flags | FPDF_NO_NATIVETEXT);
        Pause pause(api, page.get(), true);
        RenderJob job(api, page.get());
        int status = api.FPDF_RenderPageBitmapWithColorScheme_Start(actual.get(), page.get(), 7, 11, 144, 108, rotate, flags | FPDF_NO_NATIVETEXT, nullptr, &pause);
        for (int step = 0; step < 100 && status == FPDF_RENDER_TOBECONTINUED; ++step) {
          status = api.FPDF_RenderPage_Continue(page.get(), &pause);
        }
        ASSERT_EQ(status, FPDF_RENDER_DONE);
        EXPECT_TRUE(pause.identity);
        EXPECT_TRUE(pause.reentry);
        const auto image = runtime_test::Bitmap(api, actual.get());
        CompareRaster(runtime_test::Bitmap(api, expected.get()), image, artifacts,
                       library->name() + "-rotation" + std::to_string(rotate) + "-flags" + std::to_string(flags));
        observations[backend].push_back(image);
      }
    }
    ++backend;
  }
  // All per-backend progressive/synchronous comparisons above are exact.
  // With smoothing disabled, isolated binary text-edge pixels survive the
  // stroke metric (inspected connected mass 1.80 at rotations 90/270).
  // Allow 2.0 only for these cross-engine variants; keep the ordinary metric
  // limit at 1.0 and the per-engine progressive/synchronous checks exact.
  ASSERT_EQ(observations[0].size(), observations[1].size());
  for (size_t index = 0; index < observations[0].size(); ++index) {
    CompareRaster(observations[0][index], observations[1][index], artifacts,
                   "multiphase-backends" + std::to_string(index),
        RasterTolerance::ConnectedError(index % 4 == 3 ? 2.0 : 1.0));
  }
}

TEST(Progressive, ColorSchemesOverridePathAndTextAndFillToStrokeChangesInterior) {
  Artifacts artifacts;
  const auto bytes = PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << /Font << /F 5 0 R >> >> /Contents 4 0 R >>",
      PdfStream("", "0 0 0 rg 1 0 0 RG 3 w 16 16 64 40 re B\nBT /F 14 Tf 8 80 Td (Color) Tj ET\nBT /F 14 Tf 1 Tr 70 80 Td (Line) Tj ET\n"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"});
  const FPDF_COLORSCHEME scheme{0xff00ff00, 0xffff0000, 0xff0000ff, 0xffffff00};
  std::array<std::array<Raster, 2>, 2> images;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    const auto& api = library->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    Handle page(api.FPDF_LoadPage(document.get(), 0), api.FPDF_ClosePage);
    ASSERT_NE(page.get(), nullptr);
    for (int convert = 0; convert < 2; ++convert) {
      Handle bitmap(api.FPDFBitmap_Create(128, 96, 1), api.FPDFBitmap_Destroy);
      ASSERT_NE(bitmap.get(), nullptr);
      ASSERT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(), 0, 0, 128, 96, 0xffffffff));
      Pause pause(api, page.get(), false);
      RenderJob job(api, page.get());
      int status = api.FPDF_RenderPageBitmapWithColorScheme_Start(bitmap.get(), page.get(), 0, 0, 128, 96, 0,
          (convert ? FPDF_CONVERT_FILL_TO_STROKE : 0) | FPDF_NO_NATIVETEXT, &scheme, &pause);
      for (int step = 0; step < 50 && status == FPDF_RENDER_TOBECONTINUED; ++step) status = api.FPDF_RenderPage_Continue(page.get(), &pause);
      ASSERT_EQ(status, FPDF_RENDER_DONE);
      images[backend][convert] = runtime_test::Bitmap(api, bitmap.get());
      const auto offset = (56 * 128 + 48) * 4;
      EXPECT_EQ(images[backend][convert].pixels[offset], convert ? 255 : 0);
      EXPECT_EQ(images[backend][convert].pixels[offset + 1], 255);
      EXPECT_EQ(images[backend][convert].pixels[offset + 2], convert ? 255 : 0);
    }
    EXPECT_NE(images[backend][0].pixels, images[backend][1].pixels);
    ++backend;
  }
  for (int convert = 0; convert < 2; ++convert) CompareRaster(images[0][convert], images[1][convert], artifacts, "scheme" + std::to_string(convert), RasterTolerance{16, 0, 0.03});
}

}  // namespace
}  // namespace pdfoxium::integration
