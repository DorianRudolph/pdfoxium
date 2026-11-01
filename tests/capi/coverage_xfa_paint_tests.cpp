#include "xfa_form_fixture.h"

#include <array>
#include <cmath>
#include <functional>
#include <string>

namespace pdfoxium::integration {
namespace {
using RGB = std::array<int, 3>;
using Inspect = std::function<void(const Raster&, int)>;

// The public API cannot construct fill-style nodes. Both engines build and
// render this minimal template; expected colors and geometry are independent.
std::vector<uint8_t> PaintFixture(const std::string& style, const std::string& color,
                                  const std::string& box_override = "") {
  const std::string box = box_override.empty()
      ? "<rectangle><edge presence=\"hidden\"/><fill><color value=\"" + color + "\"/>" + style + "</fill></rectangle>"
      : box_override;
  const std::string xml =
      "<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
      "<subform name=\"root\" layout=\"position\"><pageSet><pageArea name=\"page\">"
      "<medium short=\"160pt\" long=\"160pt\"/><contentArea w=\"160pt\" h=\"160pt\"/>"
      "</pageArea></pageSet><draw name=\"paint\" x=\"20pt\" y=\"20pt\" w=\"100pt\" h=\"80pt\">"
      "<value>" + box + "</value></draw><draw x=\"140pt\" y=\"140pt\" w=\"8pt\" h=\"8pt\">"
      "<value><rectangle><edge presence=\"hidden\"/><fill><color value=\"0,128,0\"/></fill>"
      "</rectangle></value></draw></subform></template>";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 160 160] >>",
      "<< /Fields [] /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("", "<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("", "<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("", xml),
      PdfStream("", "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data/></xfa:datasets>"),
      PdfStream("", "</xdp:xdp>")});
}
void Pixel(const Raster& image, int x, int y, RGB color, int tolerance = 1) {
  ASSERT_GE(x, 0); ASSERT_GE(y, 0); ASSERT_LT(x, image.width); ASSERT_LT(y, image.height);
  const size_t offset = (static_cast<size_t>(y) * image.width + x) * 4;
  for (int channel = 0; channel < 3; ++channel)
    EXPECT_NEAR(image.pixels[offset + 2 - channel], color[channel], tolerance)
        << "pixel " << x << ',' << y << " channel " << channel;
  EXPECT_EQ(image.pixels[offset + 3], 255);
}
void Guards(const Raster& image, int scale) {
  Pixel(image, 144 * scale, 144 * scale, {0, 128, 0});
  for (const auto& p : std::array<std::array<int, 2>, 5>{{{5, 5}, {125, 50}, {50, 105}, {15, 50}, {50, 15}}})
    Pixel(image, p[0] * scale, p[1] * scale, {255, 255, 255}, 0);
}
void Workflow(const std::string& name, const std::string& style, const Inspect& inspect,
              const std::string& color = "255,0,0", int scale = 1,
              const std::string& box_override = "") {
  Artifacts artifacts;
  const auto input = PaintFixture(style, color, box_override);
  std::array<Raster, 2> originals;
  size_t index = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(name + " writer=" + writer->name());
    forms::Session session(writer->api(), input, true, true);
    ASSERT_TRUE(session.valid()); ASSERT_TRUE(session.xfa_loaded);
    ASSERT_EQ(session.api.FPDF_GetPageCount(session.document), 1);
    originals[index] = session.Draw(160 * scale, 160 * scale);
    ASSERT_NO_FATAL_FAILURE(Guards(originals[index], scale));
    ASSERT_NO_FATAL_FAILURE(inspect(originals[index], scale));
    CompareRaster(originals[index], session.Draw(160 * scale, 160 * scale), artifacts, name + "-repeat");
    const auto saved = SaveDocument(session.api, session.document, artifacts, writer->name() + "-" + name);
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE("reader=" + reader->name());
      forms::Session reopened(reader->api(), saved, true, true);
      ASSERT_TRUE(reopened.valid()); ASSERT_TRUE(reopened.xfa_loaded);
      const auto image = reopened.Draw(160 * scale, 160 * scale);
      ASSERT_NO_FATAL_FAILURE(Guards(image, scale));
      ASSERT_NO_FATAL_FAILURE(inspect(image, scale));
      CompareRaster(originals[index], image, artifacts, name + "-" + writer->name() + "-" + reader->name(), RasterTolerance{1, 0, .05});
    }
    ++index;
  }
  CompareRaster(originals[0], originals[1], artifacts, name + "-backends", RasterTolerance{1, 0, .05});
}

TEST(CoverageXfaPaint, LinearDirectionsInterpolateAndScaleWithoutChangingClippedShape) {
  for (const std::string direction : {"toRight", "toLeft", "toBottom", "toTop"}) for (int scale : {1, 2}) {
    Workflow("linear-" + direction + "-" + std::to_string(scale),
        "<linear type=\"" + direction + "\"><color value=\"0,0,255\"/></linear>",
        [=](const Raster& image, int s) {
          for (int quarter : {1, 2, 3}) {
            const bool horizontal = direction == "toRight" || direction == "toLeft";
            const int x = horizontal ? 20 + 25 * quarter : 70;
            const int y = horizontal ? 60 : 20 + 20 * quarter;
            const double t = (direction == "toLeft" || direction == "toTop") ? 1.0 - quarter / 4.0 : quarter / 4.0;
            Pixel(image, x * s, y * s, {static_cast<int>(255 * (1 - t)), 0, static_cast<int>(255 * t)}, 3);
          }
        }, "255,0,0", scale);
  }
}
TEST(CoverageXfaPaint, RadialDirectionsUseTheSpecifiedCenterAndCircularDistance) {
  for (const std::string direction : {"toEdge", "toCenter"}) {
    Workflow("radial-" + direction, "<radial type=\"" + direction + "\"><color value=\"0,0,255\"/></radial>",
        [=](const Raster& image, int scale) {
          const double radius = std::hypot(100., 80.) / 2.;
          for (const auto& p : std::array<std::array<int, 2>, 4>{{{70, 60}, {95, 60}, {70, 80}, {110, 90}}}) {
            double t = std::hypot(p[0] - 70., p[1] - 60.) / radius;
            if (direction == "toCenter") t = 1 - t;
            Pixel(image, p[0] * scale, p[1] * scale, {static_cast<int>(255 * (1 - t)), 0, static_cast<int>(255 * t)}, 3);
          }
        });
  }
}
TEST(CoverageXfaPaint, SixHatchStylesHaveIndependentEightPixelGeometry) {
  // Describe the periodic geometry, rather than copying production mask bytes.
  const std::array<std::string, 6> names{"horizontal", "vertical", "diagonalLeft", "diagonalRight", "crossHatch", "crossDiagonal"};
  for (size_t kind = 0; kind < names.size(); ++kind) {
    Workflow("hatch-" + names[kind], "<pattern type=\"" + names[kind] + "\"><color value=\"0,0,255\"/></pattern>",
        [=](const Raster& image, int) {
          size_t ink = 0, mismatches = 0;
          for (int y = 1; y < 79; ++y) for (int x = 1; x < 99; ++x) {
            const bool horizontal = y % 8 == 0, vertical = x % 8 == 0;
            const bool forward = x % 8 == y % 8, backward = (x + y) % 8 == 7;
            const bool painted = kind == 0 ? horizontal : kind == 1 ? vertical : kind == 2 ? forward :
                kind == 3 ? backward : kind == 4 ? horizontal || vertical : forward || backward;
            const RGB expected = painted ? RGB{0, 0, 255} : RGB{255, 255, 255};
            const size_t offset = (static_cast<size_t>(20 + y) * image.width + 20 + x) * 4;
            bool mismatch = image.pixels[offset + 3] != 255;
            for (int channel = 0; channel < 3; ++channel)
              mismatch |= std::abs(int(image.pixels[offset + 2 - channel]) - expected[channel]) > 1;
            mismatches += mismatch;
            ink += painted;
          }
          EXPECT_GT(ink, 500u);
          EXPECT_EQ(mismatches, 0u) << names[kind];
        }, "255,255,255");
  }
}
TEST(CoverageXfaPaint, StippleUsesChildColorAndSpecifiedOpacityWithZeroMeaningOpaque) {
  for (int rate : {0, 1, 25, 50, 75, 100}) {
    Workflow("stipple-" + std::to_string(rate), "<stipple rate=\"" + std::to_string(rate) + "\"><color value=\"0,0,255\"/></stipple>",
        [=](const Raster& image, int scale) {
          const int alpha = rate == 0 ? 255 : rate * 255 / 100;
          for (const auto& p : std::array<std::array<int, 2>, 3>{{{30, 30}, {70, 60}, {110, 90}}})
            Pixel(image, p[0] * scale, p[1] * scale, {255 - alpha, 255 - alpha, 255}, 1);
        });
  }
}

TEST(CoverageXfaPaint, ThreeDimensionalBordersHaveDistinctOuterAndInnerShades) {
  for (const std::string style : {"lowered", "raised", "etched", "embossed"}) {
    const auto box = "<rectangle hand=\"even\"><corner stroke=\"" + style +
        "\" thickness=\"4pt\" join=\"square\"/><edge stroke=\"" + style +
        "\" thickness=\"4pt\"/><fill><color value=\"255,255,255\"/></fill></rectangle>";
    Workflow("border-" + style, "", [=](const Raster& image, int) {
      const int top_outer = style == "lowered" || style == "raised" ? 0 : 128;
      const int top_inner = style == "lowered" ? 128 : style == "embossed" ? 0 : 255;
      const int bottom_outer = style == "etched" ? 255 : 0;
      const int bottom_inner = style == "lowered" ? 192 : 128;
      Pixel(image, 70, 21, {top_outer, top_outer, top_outer});
      Pixel(image, 70, 23, {top_inner, top_inner, top_inner});
      Pixel(image, 70, 97, {bottom_inner, bottom_inner, bottom_inner});
      Pixel(image, 70, 99, {bottom_outer, bottom_outer, bottom_outer});
      Pixel(image, 70, 60, {255, 255, 255});
    }, "255,255,255", 1, box);
  }
}

TEST(CoverageXfaPaint, StrokeDashFamiliesHaveVisibleGapsAndDoNotFillTheRectangleInterior) {
  for (const std::string style : {"solid", "dashed", "dotted", "dashDot", "dashDotDot"}) {
    const auto stroke = " stroke=\"" + style + "\" thickness=\"4pt\"";
    const auto box = "<rectangle hand=\"even\"><corner join=\"square\"" + stroke +
        "><color value=\"255,0,0\"/></corner><edge" + stroke +
        "><color value=\"255,0,0\"/></edge><fill><color value=\"255,255,255\"/></fill></rectangle>";
    Workflow("dash-" + style, "", [=](const Raster& image, int) {
      size_t red = 0;
      for (int x = 24; x < 116; ++x) {
        const size_t at = (static_cast<size_t>(20) * image.width + x) * 4;
        red += image.pixels[at] < 30 && image.pixels[at + 1] < 30 && image.pixels[at + 2] > 225;
      }
      EXPECT_GT(red, 30u);
      if (style == "solid") { EXPECT_EQ(red, 92u); }
      else { EXPECT_LT(red, 88u); }
      Pixel(image, 70, 60, {255, 255, 255});
    }, "255,255,255", 1, box);
  }
}

TEST(CoverageXfaPaint, EllipticAndCircularArcsRespectTheirGeometricBounds) {
  for (bool circular : {false, true}) {
    const auto box = std::string("<arc circular=\"") + (circular ? "1" : "0") +
        "\"><edge presence=\"hidden\"/><fill><color value=\"0,128,0\"/></fill></arc>";
    Workflow("arc-full-" + std::to_string(circular), "", [=](const Raster& image, int) {
      Pixel(image, 70, 60, {0, 128, 0});
      Pixel(image, 70, 30, {0, 128, 0});
      Pixel(image, 25, 60, circular ? RGB{255, 255, 255} : RGB{0, 128, 0});
      Pixel(image, 25, 25, {255, 255, 255});
    }, "255,255,255", 1, box);
  }
}

TEST(CoverageXfaPaint, HalfEllipseSweepsCloseAcrossTheDeclaredStartAndEndPoints) {
  for (int start : {0, 90, 180, 270}) {
    const auto box = "<arc startAngle=\"" + std::to_string(start) +
        "\" sweepAngle=\"180\"><edge presence=\"hidden\"/><fill><color value=\"0,128,0\"/></fill></arc>";
    Workflow("arc-half-" + std::to_string(start), "", [=](const Raster& image, int) {
      for (const auto& p : std::array<std::array<int, 2>, 4>{{{90, 40}, {50, 40}, {50, 80}, {90, 80}}}) {
        const bool painted = start == 0 ? p[1] < 60 : start == 90 ? p[0] < 70 :
            start == 180 ? p[1] > 60 : p[0] > 70;
        Pixel(image, p[0], p[1], painted ? RGB{0, 128, 0} : RGB{255, 255, 255});
      }
    }, "255,255,255", 1, box);
  }
}
}  // namespace
}  // namespace pdfoxium::integration
