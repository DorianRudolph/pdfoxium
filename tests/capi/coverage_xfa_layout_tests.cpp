#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;

// The public API cannot construct XFA templates or datasets. Only the packet
// envelope is serialized here; both implementations perform the actual merge,
// script execution, layout, input, rendering and PDF serialization themselves.
std::vector<uint8_t> LayoutFixture(const std::string& body,
                                  const std::string& data = "<root/>",
                                  const std::string& areas = "<contentArea w=\"240pt\" h=\"240pt\"/>",
                                  int page_height = 240) {
  const std::string xml =
      "<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
      "<subform name=\"root\" layout=\"tb\" locale=\"en_US\" restoreState=\"auto\">"
      "<pageSet><pageArea name=\"page\" id=\"page\"><occur min=\"1\" max=\"-1\"/>"
      "<medium short=\"240pt\" long=\"" + std::to_string(page_height) + "pt\"/>" + areas +
      "</pageArea></pageSet>" + body + "</subform></template>";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 " + std::to_string(page_height) + "] /Resources << /Font << /Helv 10 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("", "<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("", "<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("", xml),
      PdfStream("", "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data>" + data + "</xfa:data></xfa:datasets>"),
      PdfStream("", "</xdp:xdp>"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"});
}

std::string TextField(const std::string& name, const std::string& binding,
                      const std::string& value = "", const std::string& event = "",
                      int height = 20) {
  return "<field name=\"" + name + "\" w=\"180pt\" h=\"" + std::to_string(height) +
      "pt\"><ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"12pt\"/>" + binding +
      "<value><text>" + value + "</text></value>" + event + "</field>";
}
std::string Block(const std::string& color, int width = 80, int height = 60) {
  return "<draw w=\"" + std::to_string(width) + "pt\" h=\"" + std::to_string(height) +
      "pt\"><value><rectangle><fill><color value=\"" + color +
      "\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
}
struct LayoutSession {
  forms::XfaCallbacks callbacks;
  Session session;
  LayoutSession(const Api& api, std::vector<uint8_t> bytes)
      : session(api, std::move(bytes), true, true, 2,
                [this](forms::FormHost& host) { callbacks.Configure(host); }) {}
};
void Commit(Session& s) {
  // Some callers already cleared focus. The return value reports whether a
  // focused annotation was released, not whether the document is committed.
  s.api.FORM_ForceToKillFocus(s.form);
  // Native UpdateScope flushes queued calculations/layout on actual mouse
  // dispatch. FORM_OnFocus by itself does not flush those queues.
  s.Click(230, 230);
  s.api.FORM_ForceToKillFocus(s.form);
}
void Value(Session& s, int y, const std::u16string& expected, int page = 0,
           bool native_serialization = false) {
  ASSERT_TRUE(s.Focus(8, y, page));
  auto actual = s.Text(false, page);
  // Native's XFA writer inserts a newline after the dataset element's start
  // tag. Require it only on that writer's output, and compare both readers of
  // those identical bytes. The Rust writer must preserve the original value.
  if (native_serialization && !actual.empty() && actual.front() == u'\n') actual.erase(0, 1);
  EXPECT_EQ(actual, expected);
}
void SetValue(Session& s, int y, const std::u16string& value) {
  ASSERT_TRUE(s.Focus(8, y));
  if (!s.Text().empty()) {
    EXPECT_TRUE(s.api.FORM_SelectAllText(s.form, s.page));
  }
  s.Replace(value);
  EXPECT_EQ(s.Text(), value);
  Commit(s);
}
std::vector<Raster> Pages(Session& s) {
  std::vector<Raster> images;
  const int count = s.api.FPDF_GetPageCount(s.document);
  for (int page = 0; page < count; ++page) images.push_back(s.Draw(240, 240, 0, 0, 0, 0, 0, 0, page));
  return images;
}
void ComparePages(const std::vector<Raster>& native, const std::vector<Raster>& rust,
                  Artifacts& artifacts, const std::string& label, bool text = true) {
  ASSERT_EQ(native.size(), rust.size());
  for (size_t page = 0; page < native.size(); ++page) {
    if (!text) {
      CompareRaster(native[page], rust[page], artifacts, label + "-page" + std::to_string(page));
      continue;
    }
    // Bound the previously measured Helvetica edge coverage in each row too;
    // a blank page cannot hide a missing field or a lost rich-text fragment.
    CompareRaster(native[page], rust[page], artifacts, label + "-page" + std::to_string(page),
                  RasterTolerance{18, 0, 0.15});
    for (int y = 0; y < 240; y += 20)
      CompareRaster(CropRaster(native[page], 0, y, 240, 20),
                    CropRaster(rust[page], 0, y, 240, 20), artifacts,
                    label + "-page" + std::to_string(page) + "-row" + std::to_string(y),
                    RasterTolerance{18, 0, 0.5});
  }
}
void Pixel(const Raster& image, int x, int y, std::array<int, 3> rgb) {
  ASSERT_GE(x, 0); ASSERT_GE(y, 0); ASSERT_LT(x, image.width); ASSERT_LT(y, image.height);
  const auto at = (static_cast<size_t>(y) * image.width + x) * 4;
  EXPECT_EQ(image.pixels[at], rgb[2]);
  EXPECT_EQ(image.pixels[at + 1], rgb[1]);
  EXPECT_EQ(image.pixels[at + 2], rgb[0]);
}
void ReopenPages(Session& s) {
  // A remerge retires native's old SDK page views even if the page count stays
  // unchanged. Use the public before-close/after-load lifecycle for new views.
  for (size_t i = 0; i < s.host->pages.size(); ++i) s.ClosePage(static_cast<int>(i));
  s.page = s.host->Page(0);
  ASSERT_NE(s.page, nullptr);
}
void FourWay(const Library& writer, Session& original, Artifacts& artifacts,
             const std::string& label,
             const std::function<void(Session&, bool)>& semantics, bool text = true) {
  original.api.FORM_ForceToKillFocus(original.form);
  const auto saved = SaveDocument(original.api, original.document, artifacts, writer.name() + "-" + label);
  ASSERT_FALSE(saved.empty());
  std::array<std::vector<Raster>, 2> images;
  size_t backend = 0;
  for (const auto* reader : Implementations()) {
    SCOPED_TRACE(writer.name() + " -> " + reader->name());
    LayoutSession reopened(reader->api(), saved);
    ASSERT_TRUE(reopened.session.valid()); ASSERT_TRUE(reopened.session.xfa_loaded);
    semantics(reopened.session, writer.name() == "native");
    reopened.session.api.FORM_ForceToKillFocus(reopened.session.form);
    images[backend++] = Pages(reopened.session);
  }
  ComparePages(images[0], images[1], artifacts, writer.name() + "-" + label + "-reload", text);
}

TEST(CoverageXfaLayout, OnceGlobalDataRefAndUnboundValuesUseRealDatasetIdentity) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  const auto body = TextField("tag", "<bind match=\"once\"/>") +
      TextField("tag", "<bind match=\"once\"/>") +
      TextField("shared", "<bind match=\"global\"/>") +
      TextField("shared", "<bind match=\"global\"/>") +
      TextField("mirror", "<bind match=\"dataRef\" ref=\"$record.shared\"/>") +
      TextField("local", "<bind match=\"none\"/>", "Local") +
      TextField("created", "<bind match=\"dataRef\" ref=\"$record.nested.value\"/>", "Seed");
  const auto bytes = LayoutFixture(body, "<root><tag>First</tag><tag>Second</tag><shared>Shared</shared></root>");
  artifacts.Pdf("bindings-original", bytes);
  std::array<std::vector<Raster>, 2> images;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    LayoutSession loaded(library->api(), bytes); auto& s = loaded.session;
    ASSERT_TRUE(s.valid()); ASSERT_TRUE(s.xfa_loaded);
    Value(s, 10, u"First"); Value(s, 30, u"Second");
    Value(s, 50, u"Shared"); Value(s, 70, u"Shared"); Value(s, 90, u"Shared");
    Value(s, 110, u"Local"); Value(s, 130, u"Seed");
    SetValue(s, 50, u"Updated");
    Value(s, 70, u"Updated"); Value(s, 90, u"Updated");
    SetValue(s, 10, u"OnlyFirst"); Value(s, 30, u"Second");
    SetValue(s, 130, u"CreatedPath");
    Commit(s); images[backend++] = Pages(s);
    FourWay(*library, s, artifacts, "binding-identity", [](Session& r, bool native_writer) {
      Value(r, 10, u"OnlyFirst", 0, native_writer); Value(r, 30, u"Second", 0, native_writer);
      for (int y : {50, 70, 90}) Value(r, y, u"Updated", 0, native_writer);
      Value(r, 110, u"Local", 0, native_writer); Value(r, 130, u"CreatedPath", 0, native_writer);
    });
  }
  ComparePages(images[0], images[1], artifacts, "binding-identity");
}

TEST(CoverageXfaLayout, LoadXmlAppendOverwriteAndRemergeRegenerateBoundRecordsAndPages) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  // Execute remerge from normal click events, after text editing has committed.
  // Rebuilding form nodes inside the edited field's exit event would invalidate
  // the event target while its commit notification is still on the stack.
  const auto button = [](const char* name, const char* script) {
    return std::string("<field name=\"") + name + "\" w=\"180pt\" h=\"20pt\"><bind match=\"none\"/>"
        "<ui><button/></ui><caption><value><text>" + name + "</text></value></caption>" +
        forms::Event(script, "click") + "</field>";
  };
  const auto body = button("append", "xfa.datasets.data.root.loadXML('<root><row><value>Two</value></row><row><value>Three</value></row></root>',true,false);xfa.host.messageBox(xfa.datasets.data.root.saveXML());xfa.form.remerge();") +
      button("replace", "xfa.datasets.data.root.loadXML('<root><row><value>Last</value></row></root>',true,true);xfa.form.remerge();") +
      "<subform name=\"row\" layout=\"tb\" w=\"180pt\" h=\"100pt\"><occur min=\"0\" max=\"-1\" initial=\"0\"/>"
      "<keep intact=\"contentArea\"/>" + TextField("value", "<bind match=\"once\"/>") +
      Block("0,128,0", 180, 80) + "</subform>";
  const auto bytes = LayoutFixture(body, "<root><row><value>One</value></row></root>");
  artifacts.Pdf("remerge-original", bytes);
  std::array<std::vector<Raster>, 2> grown, replaced;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    LayoutSession loaded(library->api(), bytes); auto& s = loaded.session;
    ASSERT_TRUE(s.valid()); ASSERT_TRUE(s.xfa_loaded);
    EXPECT_EQ(s.api.FPDF_GetPageCount(s.document), 1); Value(s, 50, u"One");
    s.host->page_events.clear();
    Commit(s); s.Click(8, 10); ReopenPages(s);
    ASSERT_FALSE(s.host->alerts.empty());
    const auto& data_xml = std::get<0>(s.host->alerts.back());
    EXPECT_NE(data_xml.find(u"One"), std::u16string::npos);
    EXPECT_NE(data_xml.find(u"Two"), std::u16string::npos);
    EXPECT_NE(data_xml.find(u"Three"), std::u16string::npos);
    const bool native = library->name() == "native";
    ASSERT_EQ(s.api.FPDF_GetPageCount(s.document), native ? 1 : 2);
    if (native) {
      Value(s, 50, u"Two"); Value(s, 150, u"Three");
    } else {
      Value(s, 50, u"One"); Value(s, 150, u"Two"); Value(s, 10, u"Three", 1);
    }
    Commit(s); grown[backend] = Pages(s);
    EXPECT_EQ(std::any_of(s.host->page_events.begin(), s.host->page_events.end(),
                         [](const auto& event) { return event.first > 0 && event.second == FXFA_PAGEVIEWEVENT_POSTADDED; }), !native);
    s.host->page_events.clear();
    Commit(s); s.Click(8, 30); ReopenPages(s);
    EXPECT_EQ(s.api.FPDF_GetPageCount(s.document), 1);
    Value(s, 50, u"Last");
    Commit(s); replaced[backend++] = Pages(s);
    EXPECT_EQ(std::any_of(s.host->page_events.begin(), s.host->page_events.end(),
                         [](const auto& event) { return event.first > 0 && event.second == FXFA_PAGEVIEWEVENT_POSTREMOVED; }), !native);
  }
  // Replaced records return to the common one-page geometry. Appended
  // native layout is retained separately because its first record is lost.
  ComparePages(replaced[0], replaced[1], artifacts, "remerge-overwritten");
}

TEST(CoverageXfaLayout, LocalUseAndUseHrefInheritPrototypeUiAndKeepLocalOverrides) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  const std::string body =
      "<proto><field id=\"base\" w=\"180pt\" h=\"20pt\"><bind match=\"none\"/><ui><textEdit/></ui>"
      "<font typeface=\"Helvetica\" size=\"12pt\"><fill><color value=\"0,0,255\"/></fill></font>"
      "<value><text>Inherited</text></value></field></proto>"
      "<field name=\"first\" use=\"#base\"><bind match=\"once\"/></field>"
      "<field name=\"second\" usehref=\"#base\"><value><text>Override</text></value>"
      "<font><fill><color value=\"255,0,0\"/></fill></font></field>"
      "<field name=\"third\" usehref=\".#base\"><value><text>Relative</text></value></field>";
  const auto bytes = LayoutFixture(body, "<root><first>Inherited</first></root>");
  std::array<std::vector<Raster>, 2> images;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    LayoutSession loaded(library->api(), bytes); auto& s = loaded.session;
    ASSERT_TRUE(s.valid()); ASSERT_TRUE(s.xfa_loaded);
    Value(s, 10, u"Inherited"); Value(s, 30, u"Override"); Value(s, 50, u"Relative");
    for (int y : {10, 30, 50})
      EXPECT_EQ(s.api.FPDFPage_HasFormFieldAtPoint(s.form, s.page, 8, y), FPDF_FORMFIELD_XFA_TEXTFIELD);
    SetValue(s, 10, u"Edited"); Value(s, 30, u"Override"); Value(s, 50, u"Relative");
    Commit(s); images[backend++] = Pages(s);
    for (const auto& region : {std::array<int, 3>{0, 20, 2}, {20, 40, 0}, {40, 60, 2}}) {
      int colored = 0;
      const auto& image = images[backend - 1][0];
      for (int y = region[0]; y < region[1]; ++y) for (int x = 0; x < 180; ++x) {
        const auto at = (static_cast<size_t>(y) * 240 + x) * 4;
        const int channel = 2 - region[2];
        colored += image.pixels[at + channel] > 200 && image.pixels[at + (channel + 1) % 3] < 100;
      }
      EXPECT_GT(colored, 5) << "The inherited/local font color must actually draw";
    }
    FourWay(*library, s, artifacts, "prototype-inheritance", [](Session& r, bool native_writer) {
      Value(r, 10, u"Edited", 0, native_writer); Value(r, 30, u"Override", 0, native_writer);
      Value(r, 50, u"Relative", 0, native_writer);
    });
  }
  ComparePages(images[0], images[1], artifacts, "prototype-inheritance");
}

TEST(CoverageXfaLayout, TableColumnSpansAndPresenceProduceIndependentCellGeometry) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<std::vector<Raster>, 2> images;
  const auto cell = [](const std::string& color, const std::string& attrs = "") {
    return "<draw w=\"180pt\" h=\"20pt\" " + attrs + "><value><rectangle><fill><color value=\"" +
        color + "\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
  };
  // relocate_table_row_cells supplies each draw's actual width rather than
  // its initial180pt. Spanning two columns must consume40+60, not40+80.
  const std::string body = "<subform layout=\"table\" columnWidths=\"40pt 60pt 80pt\" w=\"180pt\">"
      "<subform layout=\"row\">" + cell("255,0,0") + cell("0,255,0") + cell("0,0,255") + "</subform>"
      "<subform layout=\"row\">" + cell("255,255,0", "colSpan=\"2\"") + cell("0,255,255") + "</subform>"
      "</subform>" +
      "<subform presence=\"hidden\" h=\"30pt\">" + Block("255,0,255", 180, 30) + "</subform>" +
      "<subform presence=\"invisible\" h=\"30pt\">" + Block("255,0,255", 180, 30) + "</subform>" +
      Block("0,128,0", 180, 20);
  const auto bytes = LayoutFixture(body);
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    LayoutSession loaded(library->api(), bytes); auto& s = loaded.session;
    ASSERT_TRUE(s.valid()); ASSERT_TRUE(s.xfa_loaded);
    ASSERT_EQ(s.api.FPDF_GetPageCount(s.document), 1);
    images[backend++] = Pages(s); const auto& image = images[backend - 1][0];
    Pixel(image, 20, 10, {255, 0, 0}); Pixel(image, 70, 10, {0, 255, 0}); Pixel(image, 140, 10, {0, 0, 255});
    Pixel(image, 20, 30, {255, 255, 0}); Pixel(image, 90, 30, {255, 255, 0}); Pixel(image, 140, 30, {0, 255, 255});
    Pixel(image, 20, 50, {255, 255, 255}); Pixel(image, 20, 65, {255, 255, 255});
    Pixel(image, 20, 80, {0, 128, 0}); Pixel(image, 20, 95, {255, 255, 255});
    FourWay(*library, s, artifacts, "table-spans-presence", [](Session& r, bool) {
      EXPECT_EQ(r.api.FPDF_GetPageCount(r.document), 1);
      const auto image = r.Draw(240, 240);
      Pixel(image, 90, 30, {255, 255, 0}); Pixel(image, 140, 30, {0, 255, 255});
      Pixel(image, 20, 65, {255, 255, 255}); Pixel(image, 20, 80, {0, 128, 0});
    }, false);
  }
  ComparePages(images[0], images[1], artifacts, "table-spans-presence", false);
}

TEST(CoverageXfaLayout, ExplicitContentAndPageBreaksAndAtomicOverflowKeepColorOrder) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  struct LayoutCase { const char* name; std::string body, areas; int pages; std::vector<std::array<int, 6>> samples; };
  const auto atomic = [](const std::string& color, const std::string& breaks = "") {
    std::string name = color;
    std::replace(name.begin(), name.end(), ',', '_');
    return "<subform name=\"block" + name + "\" layout=\"tb\" w=\"80pt\" h=\"70pt\"><bind match=\"none\"/>"
        "<keep intact=\"contentArea\"/>" + breaks + Block(color, 80, 70) + "</subform>";
  };
  const std::string two_areas = "<contentArea w=\"240pt\" h=\"100pt\"/><contentArea y=\"120pt\" w=\"240pt\" h=\"100pt\"/>";
  const std::string one_area = "<contentArea w=\"240pt\" h=\"100pt\"/>";
  const std::vector<LayoutCase> cases = {
      {"content-area", atomic("255,0,0") + atomic("0,255,0", "<breakBefore targetType=\"contentArea\" startNew=\"1\"/>"),
       two_areas, 1, {{0, 20, 20, 255, 0, 0}, {0, 20, 140, 0, 255, 0}}},
      {"page-area", atomic("255,0,0") + atomic("0,255,0", "<breakBefore targetType=\"pageArea\" startNew=\"1\"/>"),
       two_areas, 2, {{0, 20, 20, 255, 0, 0}, {1, 20, 20, 0, 255, 0}}},
      {"atomic-overflow", atomic("255,0,0") + atomic("0,255,0") + atomic("0,0,255"),
       one_area, 3, {{0, 20, 20, 255, 0, 0}, {1, 20, 20, 0, 255, 0}, {2, 20, 20, 0, 0, 255}}},
  };
  for (const auto& test : cases) {
    SCOPED_TRACE(test.name);
    std::array<std::vector<Raster>, 2> images;
    size_t backend = 0;
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name());
      LayoutSession loaded(library->api(), LayoutFixture(test.body, "<root/>", test.areas));
      auto& s = loaded.session; ASSERT_TRUE(s.valid()); ASSERT_TRUE(s.xfa_loaded);
      const bool native_atomic = std::string(test.name) == "atomic-overflow" && library->name() == "native";
      // The pinned source re-lays out the retained, already completed green
      // flowed block, subtracting used_size twice and collapsing its height.
      // Keep its observed red/blue pages explicit; Rust must retain all three.
      ASSERT_EQ(s.api.FPDF_GetPageCount(s.document), native_atomic ? 2 : test.pages);
      images[backend++] = Pages(s);
      if (native_atomic) {
        Pixel(images[backend - 1][0], 20, 20, {255, 0, 0});
        Pixel(images[backend - 1][1], 20, 20, {0, 0, 255});
      } else {
        for (const auto& p : test.samples) Pixel(images[backend - 1][p[0]], p[1], p[2], {p[3], p[4], p[5]});
      }
      if (std::string(test.name) == "page-area")
        FourWay(*library, s, artifacts, test.name, [&](Session& r, bool) {
          ASSERT_EQ(r.api.FPDF_GetPageCount(r.document), test.pages);
          const auto rendered = Pages(r);
          for (const auto& p : test.samples) Pixel(rendered[p[0]], p[1], p[2], {p[3], p[4], p[5]});
        }, false);
    }
    if (std::string(test.name) == "atomic-overflow") {
      CompareRaster(images[0].front(), images[1].front(), artifacts, "atomic-first");
      CompareRaster(images[0].back(), images[1].back(), artifacts, "atomic-last");
    } else {
      ComparePages(images[0], images[1], artifacts, test.name, false);
    }
  }
}

std::vector<std::array<int, 3>> RichPalette() {
  std::vector<std::array<int, 3>> colors;
  for (int i = 0; i < 20; ++i) {
    std::array<int, 3> color{0, 0, 0};
    color[i % 3] = 255;
    colors.push_back(color);
  }
  return colors;
}
struct RichBand { int page, first, last, color; };
std::vector<RichBand> RichBands(const std::vector<Raster>& images) {
  std::vector<RichBand> bands;
  for (size_t page = 0; page < images.size(); ++page) {
    const auto& image = images[page];
    for (int y = 0; y < image.height; ++y) {
      std::array<int, 3> dominant{};
      for (int x = 0; x < 180; ++x) {
        const auto at = (static_cast<size_t>(y) * image.width + x) * 4;
        for (int channel = 0; channel < 3; ++channel) {
          const int value = image.pixels[at + 2 - channel];
          if (value > image.pixels[at + 2 - (channel + 1) % 3] + 32 &&
              value > image.pixels[at + 2 - (channel + 2) % 3] + 32)
            ++dominant[channel];
        }
      }
      const auto best = std::max_element(dominant.begin(), dominant.end());
      if (*best == 0) continue;
      const int color = static_cast<int>(best - dominant.begin());
      if (!bands.empty() && bands.back().page == static_cast<int>(page) &&
          bands.back().last == y - 1 && bands.back().color == color) {
        bands.back().last = y;
      } else {
        bands.push_back({static_cast<int>(page), y, y, color});
      }
    }
  }
  return bands;
}
void RichLines(const std::vector<Raster>& images, bool split = true) {
  const auto bands = RichBands(images);
  ASSERT_EQ(bands.size(), 20u) << "Every styled one-line paragraph must survive once";
  for (size_t line = 0; line < bands.size(); ++line) {
    EXPECT_EQ(bands[line].color, static_cast<int>(line % 3)) << "paragraph " << line;
    EXPECT_LE(bands[line].last - bands[line].first, 18);
  }
  EXPECT_EQ(bands.front().page, 0);
  EXPECT_EQ(bands.back().page, split ? 1 : 0);
}
void NativeRichSplit(const std::vector<Raster>& images) {
  ASSERT_EQ(images.size(), 2u);
  // Native's reset/resume loader overwrites the first block's pieces while
  // loading the second block. Its first page is empty and its last has ink.
  EXPECT_TRUE(std::all_of(images[0].pixels.begin(), images[0].pixels.end(),
                          [](uint8_t value) { return value == 255; }));
  forms::ExpectInk(images[1], 0, 0, 180, 180);
}
TEST(CoverageXfaLayout, RichXhtmlSplitResumePreservesStyledParagraphsAndRealHyperlinkInput) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::string paragraphs;
  const auto colors = RichPalette();
  for (size_t i = 0; i < colors.size(); ++i) {
    const auto& c = colors[i];
    const std::string style = "color:rgb(" + std::to_string(c[0]) + "," + std::to_string(c[1]) + "," + std::to_string(c[2]) + ");font-weight:bold";
    const std::string text = "MMMM" + std::to_string(10 + i);
    paragraphs += "<p style=\"margin:0pt;line-height:16pt\"><span style=\"" + style + "\">" +
        (i == 0 ? "<a href=\"https://example.invalid/xfa-layout\" style=\"" + style + "\">" + text + "</a>" : text) + "</span></p>";
  }
  const auto body = "<draw name=\"story\" w=\"180pt\"><keep intact=\"none\"/><font typeface=\"Helvetica\" size=\"14pt\"/><value><exData contentType=\"text/html\">"
      "<body xmlns=\"http://www.w3.org/1999/xhtml\">" + paragraphs + "</body></exData></value></draw>";
  const auto bytes = LayoutFixture(body, "<root/>", "<contentArea w=\"180pt\" h=\"180pt\"/>");
  const auto control_bytes = LayoutFixture(body, "<root/>", "<contentArea w=\"180pt\" h=\"480pt\"/>", 480);
  artifacts.Pdf("rich-split-original", bytes);
  std::array<std::vector<Raster>, 2> images;
  std::array<Raster, 2> controls;
  size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    LayoutSession control(library->api(), control_bytes);
    ASSERT_TRUE(control.session.valid()); ASSERT_TRUE(control.session.xfa_loaded);
    ASSERT_EQ(control.session.api.FPDF_GetPageCount(control.session.document), 1);
    controls[backend] = control.session.Draw(240, 480);
    RichLines({controls[backend]}, false);
    artifacts.Image(library->name() + "-rich-control", controls[backend]);
    LayoutSession loaded(library->api(), bytes); auto& s = loaded.session;
    ASSERT_TRUE(s.valid()); ASSERT_TRUE(s.xfa_loaded);
    ASSERT_EQ(s.api.FPDF_GetPageCount(s.document), 2);
    images[backend] = Pages(s);
    for (size_t page = 0; page < images[backend].size(); ++page)
      artifacts.Image(library->name() + "-rich-page" + std::to_string(page), images[backend][page]);
    if (library->name() == "native") {
      NativeRichSplit(images[backend]);
    } else {
      RichLines(images[backend]);
      bool clicked = false;
      const auto& first = images[backend][0];
      for (int y = 0; y < 20 && !clicked; ++y) for (int x = 0; x < 160 && !clicked; ++x) {
        const auto i = (static_cast<size_t>(y) * 240 + x) * 4;
        if (first.pixels[i + 2] > first.pixels[i] + 32 && first.pixels[i + 2] > first.pixels[i + 1] + 32) {
          s.Click(x + 0.5, y + 0.5); clicked = true;
        }
      }
      ASSERT_TRUE(clicked);
      ASSERT_EQ(loaded.callbacks.urls.size(), 1u);
      EXPECT_EQ(loaded.callbacks.urls[0], u"https://example.invalid/xfa-layout");
    }
    const auto saved = SaveDocument(s.api, s.document, artifacts, library->name() + "-rich-split");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      LayoutSession reopened(reader->api(), saved); auto& r = reopened.session;
      ASSERT_TRUE(r.valid()); ASSERT_TRUE(r.xfa_loaded);
      const auto after = Pages(r);
      if (reader->name() == "native") {
        NativeRichSplit(after);
      } else {
        RichLines(after);
        // Rust's independently checked original is available when Rust is the
        // writer; both writers retain the same untouched rich template.
        if (library->name() == "rust") ComparePages(images[backend], after, artifacts, "rich-rust-reload");
      }
    }
    ++backend;
  }
  CompareRaster(controls[0], controls[1], artifacts, "rich-unpaginated-control", RasterTolerance{18, 0, 0.15});
  // Compare each correctly paginated Rust paragraph to the corresponding
  // native paragraph from its valid, unpaginated control, including its text
  // and styling. Band count/color/order above remain independent assertions.
  const auto native_lines = RichBands({controls[0]});
  const auto rust_lines = RichBands(images[1]);
  ASSERT_EQ(native_lines.size(), 20u); ASSERT_EQ(rust_lines.size(), 20u);
  for (size_t line = 0; line < 20; ++line) {
    const auto& a = native_lines[line]; const auto& b = rust_lines[line];
    // Align painted band tops: a resumed paragraph can start at page row 0,
    // where adding one padding row above would clamp only that crop.
    const int native_y = a.first, rust_y = b.first;
    const int height = std::min({std::max(a.last - native_y, b.last - rust_y) + 2,
                                 controls[0].height - native_y, images[1][b.page].height - rust_y});
    CompareRaster(CropRaster(controls[0], 0, native_y, 180, height),
                  CropRaster(images[1][b.page], 0, rust_y, 180, height), artifacts,
                  "rich-paragraph" + std::to_string(line), RasterTolerance{18, 0, 0.5});
  }
}

}  // namespace
}  // namespace pdfoxium::integration
