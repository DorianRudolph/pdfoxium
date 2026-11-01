#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <functional>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;

std::string FontBytes(const char* name) {
  std::ifstream input(TestRoot()/"upstream/pdfium/testing/resources/fonts"/name,std::ios::binary);
  EXPECT_TRUE(input.good());
  return {std::istreambuf_iterator<char>(input),std::istreambuf_iterator<char>()};
}
std::string RichDraw(const std::string& html,int height=64,int width=200) {
  return "<draw w=\""+std::to_string(width)+"pt\" h=\""+std::to_string(height)+
    "pt\"><font typeface=\"Ahem\" size=\"12pt\"/><value><exData contentType=\"text/html\">"
    "<body xmlns=\"http://www.w3.org/1999/xhtml\">"+html+"</body></exData></value></draw>";
}
std::vector<uint8_t> BreakFixture(const std::string& body,const std::string& family="Ahem",int field_height=24) {
  static const auto ahem=FontBytes("ahem/Ahem.ttf"),roboto=FontBytes("roboto.ttf");
  const std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
    "<subform name=\"root\" layout=\"tb\" locale=\"en_US\" restoreState=\"auto\">"
    "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"240pt\" long=\"360pt\"/>"
    "<contentArea w=\"220pt\" h=\"336pt\"/></pageArea></pageSet>"
    "<field name=\"input\" w=\"200pt\" h=\""+std::to_string(field_height)+"pt\"><bind match=\"once\"/>"
    "<ui><textEdit multiLine=\"1\"/></ui><font typeface=\""+family+"\" size=\"12pt\"/>"
    "<value><text/></value></field>"+body+"</subform></template>";
  std::string widths;for(int i=0;i<256;++i)widths+="1000 ";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 360] /Resources << /Font << /Ahem 10 0 R /Roboto 13 0 R >> >> >>",
    "<< /Fields [] /DR << /Font << /Ahem 10 0 R /Roboto 13 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
    PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
    PdfStream("",xml),
    PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root><input/></root></xfa:data></xfa:datasets>"),
    PdfStream("","</xdp:xdp>"),
    "<< /Type /Font /Subtype /TrueType /BaseFont /Ahem /FirstChar 0 /LastChar 255 /Widths ["+widths+"] /FontDescriptor 11 0 R /Encoding /WinAnsiEncoding >>",
    "<< /Type /FontDescriptor /FontName /Ahem /Flags 32 /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /CapHeight 800 /StemV 80 /FontFile2 12 0 R >>",
    PdfStream("/Length1 "+std::to_string(ahem.size()),ahem),
    "<< /Type /Font /Subtype /TrueType /BaseFont /Roboto /FirstChar 0 /LastChar 255 /Widths ["+widths+"] /FontDescriptor 14 0 R /Encoding /WinAnsiEncoding >>",
    "<< /Type /FontDescriptor /FontName /Roboto /Flags 32 /FontBBox [-1000 -300 3000 2000] /Ascent 1000 /Descent -300 /CapHeight 700 /StemV 80 /FontFile2 15 0 R >>",
    PdfStream("/Length1 "+std::to_string(roboto.size()),roboto)});
}

struct InkBounds {
  int left = 200, top = 96, right = -1, bottom = -1;
  size_t count = 0;
};
InkBounds Bounds(const Raster& image) {
  InkBounds bounds;
  bool opaque = true;
  for (int y = 0; y < image.height; ++y) {
    for (int x = 0; x < image.width; ++x) {
      const size_t at = (size_t(y) * image.width + x) * 4;
      opaque = opaque && image.pixels[at + 3] == 255;
      if (std::min({image.pixels[at], image.pixels[at + 1], image.pixels[at + 2]}) >= 200)
        continue;
      ++bounds.count;
      bounds.left = std::min(bounds.left, x);
      bounds.right = std::max(bounds.right, x);
      bounds.top = std::min(bounds.top, y);
      bounds.bottom = std::max(bounds.bottom, y);
    }
  }
  EXPECT_TRUE(opaque);
  return bounds;
}
Raster RichRaster(Session& session) {
  return CropRaster(session.Draw(240, 360), 0, 24, 200, 96);
}
void CompareRich(const Raster& expected, const Raster& actual, Artifacts& artifacts,
                 const std::string& label) {
  CompareRaster(expected, actual, artifacts, label, RasterTolerance{24, 0, 0.5});
}
// Exercise parsed declarations through the actual XFA text provider, layout,
// font selection and render device. The alternate HTML is a separately parsed
// equivalent expression, not a bitmap copied from either implementation.
void RichCase(const std::string& label, const std::string& html,
              const std::string& equivalent = "", float expected_size = 0) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster, 2> originals;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name() + "-" + label);
    Session session(writer->api(), BreakFixture(RichDraw(html, 96)), true, true);
    ASSERT_TRUE(session.valid());
    ASSERT_TRUE(session.xfa_loaded);
    originals[backend] = RichRaster(session);
    const auto bounds = Bounds(originals[backend]);
    EXPECT_GT(bounds.count, 20u);
    if (expected_size > 0) {
      EXPECT_NEAR(bounds.right - bounds.left + 1, 4 * expected_size, 2);
      EXPECT_NEAR(bounds.bottom - bounds.top + 1, expected_size, 2);
    }
    if (!equivalent.empty()) {
      Session control(writer->api(), BreakFixture(RichDraw(equivalent, 96)), true, true);
      ASSERT_TRUE(control.valid());
      CompareRich(originals[backend], RichRaster(control), artifacts,
                  writer->name() + "-" + label + "-equivalent");
    }
    const auto saved = SaveDocument(session.api, session.document, artifacts,
                                    writer->name() + "-" + label);
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      Session reopened(reader->api(), saved, true, true);
      ASSERT_TRUE(reopened.valid());
      ASSERT_TRUE(reopened.xfa_loaded);
      CompareRich(originals[backend], RichRaster(reopened), artifacts,
                  writer->name() + "-" + reader->name() + "-" + label);
    }
    ++backend;
  }
  CompareRich(originals[0], originals[1], artifacts, label + "-engines");
}

TEST(CoverageRichtextCodecRecovery, NamedFontSizesAndRelativeKeywordsRetainIndependentGlyphDimensions) {
  struct Row { const char* keyword; float size; };
  for (const auto& row : std::array<Row, 9>{{
           {"xx-small", 12.f / 1.2f / 1.2f / 1.2f}, {"x-small", 12.f / 1.2f / 1.2f},
           {"small", 10}, {"medium", 12}, {"large", 14.4f},
           {"x-large", 12.f * 1.2f * 1.2f}, {"xx-large", 12.f * 1.2f * 1.2f * 1.2f},
           {"larger", 14.4f}, {"smaller", 10}}}) {
    SCOPED_TRACE(row.keyword);
    RichCase(std::string("named-size-") + row.keyword,
             "<p><span style=\"font-size:" + std::string(row.keyword) + "\">MMMM</span></p>",
             "", row.size);
  }
}

TEST(CoverageRichtextCodecRecovery, PaddingShorthandsAndFourExplicitSidesProduceEquivalentParagraphs) {
  struct Row { const char* shorthand; const char* sides; };
  for (const auto& row : std::array<Row, 4>{{
           {"3pt", "3pt 3pt 3pt 3pt"}, {"2pt 5pt", "2pt 5pt 2pt 5pt"},
           {"1pt 2pt 3pt", "1pt 2pt 3pt 2pt"}, {"1pt 2pt 3pt 4pt", "1pt 2pt 3pt 4pt"}}}) {
    SCOPED_TRACE(row.shorthand);
    RichCase("padding-" + std::string(row.shorthand),
             "<p style=\"margin:0;padding:" + std::string(row.shorthand) + "\">MMMM</p>",
             "<p style=\"margin:0;padding:" + std::string(row.sides) + "\">MMMM</p>", 12);
  }
}

TEST(CoverageRichtextCodecRecovery, BorderWidthsAndShorthandsPreserveTheRichTextContentContract) {
  // XFA consumes these CSS declarations as computed text styles; it does not
  // draw browser box borders around inline spans. Glyph ink must stay intact.
  for (const auto* style : {"border:thin solid red", "border:medium dashed #00ff00",
                            "border:thick dotted rgb(0,0,255)", "border:solid blue",
                            "border-width:1pt 2pt 3pt 4pt",
                            "border-left:2pt solid red;border-top:thin solid green;"
                            "border-right:medium solid blue;border-bottom:thick solid black"}) {
    SCOPED_TRACE(style);
    RichCase("border-" + std::to_string(std::string(style).size()),
             "<p><span style=\"" + std::string(style) + "\">MMMM</span></p>",
             "<p><span>MMMM</span></p>", 12);
  }
}

TEST(CoverageRichtextCodecRecovery, PositionalDeclarationsOnInlineTextKeepItsNormalFlow) {
  for (const auto* style : {"top:1pt;bottom:2pt;left:3pt;right:4pt",
                            "top:10%;bottom:20%;left:30%;right:40%",
                            "top:auto;bottom:auto;left:auto;right:auto"}) {
    SCOPED_TRACE(style);
    RichCase("inline-position-" + std::to_string(std::string(style).size()),
             "<p><span style=\"" + std::string(style) + "\">MMMM</span></p>",
             "<p>MMMM</p>", 12);
  }
}

TEST(CoverageRichtextCodecRecovery, NormalAndZeroLengthSpacingRestoreInheritedDefaults) {
  for (const auto* style : {"letter-spacing:normal;word-spacing:normal",
                            "letter-spacing:0pt;word-spacing:0pt",
                            "letter-spacing:0%;word-spacing:0%"}) {
    RichCase("zero-spacing-" + std::to_string(std::string(style).size()),
             "<p><span style=\"" + std::string(style) + "\">MMMM</span></p>",
             "<p>MMMM</p>", 12);
  }
}

TEST(CoverageRichtextCodecRecovery, FontShorthandNormalSmallCapsAndNumericWeightsAgreeWithExplicitStyles) {
  struct Row { const char* shorthand; const char* expanded; };
  for (const auto& row : std::array<Row, 4>{{
           {"normal normal normal 12pt Ahem", "font-style:normal;font-variant:normal;font-weight:400;font-size:12pt;font-family:Ahem"},
           {"small-caps 12pt Ahem", "font-variant:small-caps;font-size:12pt;font-family:Ahem"},
           {"normal small-caps 500 12pt Ahem", "font-style:normal;font-variant:small-caps;font-weight:500;font-size:12pt;font-family:Ahem"},
           {"italic normal 700 12pt Ahem", "font-style:italic;font-variant:normal;font-weight:700;font-size:12pt;font-family:Ahem"}}}) {
    SCOPED_TRACE(row.shorthand);
    RichCase("font-shorthand-" + std::to_string(std::string(row.shorthand).size()),
             "<p><span style=\"font:" + std::string(row.shorthand) + "\">MMMM</span></p>",
             "<p><span style=\"" + std::string(row.expanded) + "\">MMMM</span></p>");
  }
}

TEST(CoverageRichtextCodecRecovery, LegacyParagraphAlignmentAttributesPreserveTheXfaCssTextFlow) {
  for (const auto* align : {"left", "center", "right", "justify"}) {
    RichCase(std::string("align-attribute-") + align,
             "<p align=\"" + std::string(align) + "\">MMMM</p>",
             "<p>MMMM</p>", 12);
  }
}

TEST(CoverageRichtextCodecRecovery, ImportantDeclarationsRetainPriorityThroughDuplicateAndCustomProperties) {
  for (const auto* style : {"font-size:12pt!important;font-size:24pt",
                            "font-size:12pt!important;color:black;color:red;color:black",
                            "xfa-font-horizontal-scale:100;xfa-font-vertical-scale:100;font-size:12pt",
                            "unknown-property:unused;font-size:12pt;color:black;"}) {
    RichCase("style-priority-" + std::to_string(std::string(style).size()),
             "<p><span style=\"" + std::string(style) + "\">MMMM</span></p>",
             "<p>MMMM</p>", 12);
  }
}

TEST(CoverageRichtextCodecRecovery, VerticalAlignmentEnumsRetainContentAndReloadAcrossRealInlineRuns) {
  for (const auto* align : {"baseline", "middle", "bottom", "super", "sub", "text-top", "text-bottom", "top"}) {
    SCOPED_TRACE(align);
    RichCase(std::string("vertical-align-") + align,
             "<p><span>MM</span><span style=\"vertical-align:" + std::string(align) + "\">MM</span></p>");
  }
}

TEST(CoverageRichtextCodecRecovery, ExtraTextDecorationFlagsPreserveActualSolidGlyphsAndSaveReload) {
  for (const auto* decoration : {"overline", "blink", "double", "underline overline",
                                 "line-through overline blink", "none"}) {
    SCOPED_TRACE(decoration);
    RichCase(std::string("decoration-") + std::to_string(std::string(decoration).size()),
             "<p><span style=\"text-decoration:" + std::string(decoration) + "\">MMMM</span></p>");
  }
}

using Bytes = std::vector<uint8_t>;
void PutBe(Bytes& bytes, unsigned value, int count) {
  for (int i = count - 1; i >= 0; --i) bytes.push_back(uint8_t(value >> (8 * i)));
}
void JpegSegment(Bytes& bytes, uint8_t marker, const Bytes& payload) {
  bytes.insert(bytes.end(), {255, marker});
  PutBe(bytes, unsigned(payload.size() + 2), 2);
  bytes.insert(bytes.end(), payload.begin(), payload.end());
}
struct JpegBits {
  Bytes bytes;
  unsigned value = 0, count = 0;
  void Write(unsigned code, int width) {
    for (int bit = width - 1; bit >= 0; --bit) {
      value = (value << 1) | ((code >> bit) & 1);
      if (++count == 8) {
        bytes.push_back(uint8_t(value));
        if (uint8_t(value) == 255) bytes.push_back(0);
        value = count = 0;
      }
    }
  }
  Bytes Finish() {
    if (count) Write((1u << (8 - count)) - 1, int(8 - count));
    return bytes;
  }
};
// Known DC coefficients encode independent constant samples: gray136,
// RGB(136,128,120), or four zero CMYK samples. Only valid JPEG syntax is built.
Bytes UniformJpeg(int components, int unit = 1, int density = 96, bool extra_markers = false) {
  Bytes jpeg{255, 216};
  if (extra_markers) {
    JpegSegment(jpeg, 0xe1, {'E','x','i','f',0,0});
    jpeg.push_back(255);  // Fill byte before the next marker is valid JPEG.
  }
  Bytes jfif{'J','F','I','F',0,1,2,uint8_t(unit)};
  PutBe(jfif, unsigned(density), 2);
  PutBe(jfif, unsigned(density), 2);
  jfif.insert(jfif.end(), {0, 0});
  // JFIF identifies three-component data as YCbCr. Direct Adobe RGB/CMYK
  // samples must not carry a conflicting JFIF color-space declaration.
  if (components == 1) JpegSegment(jpeg, 0xe0, jfif);
  if (components >= 3) JpegSegment(jpeg, 0xee, {'A','d','o','b','e',0,100,0,0,0,0,0});
  Bytes quant(65, 1); quant[0] = 0;
  JpegSegment(jpeg, 0xdb, quant);
  Bytes frame{8,0,8,0,16,uint8_t(components)};
  for (int c = 0; c < components; ++c) frame.insert(frame.end(), {uint8_t(c + 1),0x11,0});
  JpegSegment(jpeg, 0xc0, frame);
  Bytes huffman{0,1,1,1}; huffman.resize(17,0);
  huffman.insert(huffman.end(), {0,7,11,0x10,1});
  huffman.resize(37,0); huffman.push_back(0);
  JpegSegment(jpeg, 0xc4, huffman);
  Bytes scan{uint8_t(components)};
  for (int c = 0; c < components; ++c) scan.insert(scan.end(), {uint8_t(c + 1),0});
  scan.insert(scan.end(), {0,63,0});
  JpegSegment(jpeg, 0xda, scan);
  JpegBits bits;
  std::vector<int> previous(components, 0);
  for (int block = 0; block < 2; ++block) for (int c = 0; c < components; ++c) {
    const int coefficient = components == 4 ? -1024 : components == 1 ? 64 : c == 0 ? 64 : c == 2 ? -64 : 0;
    const int delta = coefficient - previous[c]; previous[c] = coefficient;
    const int category = delta == 0 ? 0 : std::abs(delta) <= 127 ? 7 : 11;
    bits.Write(category == 0 ? 0 : category == 7 ? 2 : 6, category == 0 ? 1 : category == 7 ? 2 : 3);
    if (category) bits.Write(unsigned(delta >= 0 ? delta : (1 << category) - 1 + delta), category);
    bits.Write(0, 1);
  }
  const auto entropy = bits.Finish();
  jpeg.insert(jpeg.end(), entropy.begin(), entropy.end());
  jpeg.insert(jpeg.end(), {255,217});
  return jpeg;
}
std::string Base64(const Bytes& bytes) {
  const char* alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  std::string encoded;
  for (size_t i = 0; i < bytes.size(); i += 3) {
    const unsigned value = unsigned(bytes[i]) << 16 |
        (i + 1 < bytes.size() ? unsigned(bytes[i + 1]) << 8 : 0) |
        (i + 2 < bytes.size() ? bytes[i + 2] : 0);
    encoded += alphabet[(value >> 18) & 63]; encoded += alphabet[(value >> 12) & 63];
    encoded += i + 1 < bytes.size() ? alphabet[(value >> 6) & 63] : '=';
    encoded += i + 2 < bytes.size() ? alphabet[value & 63] : '=';
  }
  return encoded;
}
void JpegCase(const std::string& label, const Bytes& jpeg, const std::string& mime,
              const std::array<int, 3>& rgb) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto image = "<image contentType=\"" + mime + "\" transferEncoding=\"base64\" aspect=\"none\">" + Base64(jpeg) + "</image>";
  const auto extra = "<draw w=\"80pt\" h=\"80pt\"><value>" + image + "</value></draw>"
      "<field name=\"picture\" w=\"80pt\" h=\"80pt\"><bind match=\"none\"/><ui><imageEdit/></ui><value>" + image + "</value></field>";
  Artifacts artifacts;
  std::array<Raster, 2> originals;
  size_t backend = 0;
  auto capture = [&](Session& session) {
    auto raster = CropRaster(session.Draw(200, 300), 0, 40, 80, 160);
    int max_error = 0;
    for (size_t at = 0; at < raster.pixels.size(); at += 4) {
      for (int c = 0; c < 3; ++c) max_error = std::max(max_error, std::abs(int(raster.pixels[at + c]) - rgb[2 - c]));
      max_error = std::max(max_error, 255 - int(raster.pixels[at + 3]));
    }
    EXPECT_LE(max_error, 2) << label;
    return raster;
  };
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name() + "-" + label);
    Session session(writer->api(), forms::XfaFixture(true, "", extra), true, true);
    ASSERT_TRUE(session.valid()); ASSERT_TRUE(session.xfa_loaded);
    EXPECT_EQ(session.api.FPDFPage_HasFormFieldAtPoint(session.form, session.page, 10, 130), FPDF_FORMFIELD_XFA_IMAGEFIELD);
    originals[backend] = capture(session);
    const auto saved = SaveDocument(session.api, session.document, artifacts, writer->name() + "-" + label);
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      Session reopened(reader->api(), saved, true, true);
      ASSERT_TRUE(reopened.valid()); ASSERT_TRUE(reopened.xfa_loaded);
      CompareRaster(originals[backend], capture(reopened), artifacts,
                    writer->name() + "-" + reader->name() + "-" + label, RasterTolerance{2,0,0.2});
    }
    ++backend;
  }
  CompareRaster(originals[0], originals[1], artifacts, label + "-engines", RasterTolerance{2,0,0.2});
}

TEST(CoverageRichtextCodecRecovery, XfaGrayscaleJpegRunsTheActualProgressiveDecoderAndEditableImage) {
  JpegCase("xfa-jpeg-gray", UniformJpeg(1), "image/jpeg", {136,136,136});
}
TEST(CoverageRichtextCodecRecovery, XfaRgbJpegPreservesDirectAdobeComponentColors) {
  JpegCase("xfa-jpeg-rgb", UniformJpeg(3), "image/jpg", {136,128,120});
}
TEST(CoverageRichtextCodecRecovery, XfaCmykJpegConvertsAllFourComponentsBeforeDrawing) {
  JpegCase("xfa-jpeg-cmyk", UniformJpeg(4), "image/jpeg", {0,0,0});
}
TEST(CoverageRichtextCodecRecovery, JpegJfifDensityUnitsAndFallbackMimeRetainTheRequestedImageRectangle) {
  for (int unit : {0,1,2,3}) {
    SCOPED_TRACE(unit);
    JpegCase("xfa-jpeg-density-" + std::to_string(unit), UniformJpeg(1, unit, unit == 2 ? 38 : 96),
             unit == 0 ? "application/octet-stream" : "image/jpeg", {136,136,136});
  }
}
TEST(CoverageRichtextCodecRecovery, JpegExifAndMarkerFillBytesRetainDecodedSamples) {
  JpegCase("xfa-jpeg-marker-fill", UniformJpeg(3, 1, 72, true), "image/jpeg", {136,128,120});
}

}  // namespace
}  // namespace pdfoxium::integration
