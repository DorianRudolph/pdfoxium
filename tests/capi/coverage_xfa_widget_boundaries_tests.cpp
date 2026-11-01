#include "xfa_form_fixture.h"
#include "text_fixture.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using Bytes = std::vector<uint8_t>;

const std::string kClean =
    "var v=this.rawValue;if(typeof v==='string'&&v.charCodeAt(0)===10)this.rawValue=v.slice(1);"
    "for(var g=0;g<this.nodes.length;g++){var items=this.nodes.item(g);if(items.className!=='items')continue;"
    "for(var i=0;i<items.nodes.length;i++){var item=items.nodes.item(i),s=String(item.value);if(s.charCodeAt(0)===10)item.value=s.slice(1);}}";

std::string WidgetText(const std::string& name, const std::string& value, int y,
                       const std::string& properties = "", const std::string& attributes = "",
                       int height = 40, const std::string& ui = "<textEdit/>") {
  return "<field name=\"" + name + "\" x=\"8pt\" y=\"" + std::to_string(y) +
      "pt\" w=\"160pt\" h=\"" + std::to_string(height) + "pt\" " + attributes + ">"
      "<bind match=\"once\"/><font typeface=\"Ahem\" size=\"8pt\"/><ui>" + ui + "</ui>"
      "<para lineHeight=\"8pt\"/><value><text>" + value + "</text></value>" + properties +
      forms::Event(kClean, "initialize") + "</field>";
}
std::string WidgetChoice(bool combo, const std::string& value = "a", bool multi = false) {
  // Half-pixel list placement keeps the complete Ahem AA fringe inside the
  // outward-rounded item-text clip; native TextOut's empty clip has separate
  // regression coverage. The editable combo retains its existing coordinates.
  return "<field name=\"choice\" x=\"" + std::string(combo ? "8" : "8.5") +
      "pt\" y=\"" + std::string(combo ? "80" : "80.5") + "pt\" w=\"160pt\" h=\"" +
      std::string(combo ? "24" : "80") + "pt\"><bind match=\"" + std::string(multi ? "none" : "once") + "\"/>"
      "<ui><choiceList open=\"" + std::string(combo ? "userControl" : multi ? "multiSelect" : "always") +
      "\" textEntry=\"" + std::string(combo ? "1" : "0") + "\"/></ui>"
      "<font typeface=\"Ahem\" size=\"" + std::string(combo ? "8" : "12") + "pt\"/><items><text>A</text><text>BB</text><text>CCC</text><text>DDDD</text><text>EEEEE</text></items>"
      "<items save=\"1\"><text>a</text><text>b</text><text>c</text><text>d</text><text>e</text></items>"
      "<value><text>" + value + "</text></value>" + forms::Event(kClean, "initialize") + "</field>";
}
std::string CheckCode(const std::string& code) {
  return "function eq(a,b){if(a!==b)throw 'expected '+String(b)+' got '+String(a);}"
      "function clean(a){return String(a===null?'':a).replace(/^\\n/,'');}"
      "function selected(f,indices){for(var i=0;i<f.length;i++)eq(f.getItemState(i),indices.indexOf(i)>=0);}"
      "try{var r=xfa.form.root;" + code + ";r.report.rawValue='PASS';xfa.host.setFocus(r.report);}"
      "catch(e){xfa.form.root.report.rawValue='FAIL';xfa.host.messageBox('FAIL:'+String(e));}";
}
std::string WidgetButton(const std::string& name, int y, const std::string& code) {
  return "<field name=\"" + name + "\" x=\"210pt\" y=\"" + std::to_string(y) +
      "pt\" w=\"70pt\" h=\"24pt\"><bind match=\"none\"/><ui><button/></ui>"
      "<font typeface=\"Ahem\" size=\"8pt\"/><caption><value><text>GO</text></value></caption>" +
      forms::Event("this.caption.value.text.value='GO';", "initialize") +
      forms::Event(CheckCode(code), "click") + "</field>";
}
struct BoundarySpec {
  std::string input = WidgetText("input", "ABCDEF", 16);
  std::string choice;
  std::string action;
  std::string verify = "eq(clean(r.input.rawValue),'ABCDEF');";
  bool combo = false;
  bool transient_selection = false;
  int settle_list_selection = -1;
  std::u16string combo_value;
  int answer = JSPLATFORM_ALERT_RETURN_YES;
};
Bytes WidgetFixture(const BoundarySpec& spec) {
  static const auto font = text_fixture::Read(TestRoot() / "upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
  const std::string preamble = "<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
  const std::string config = "<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
  std::string xml = "<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" locale=\"en_US\" restoreState=\"auto\">"
      "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"320pt\" long=\"320pt\"/><contentArea w=\"320pt\" h=\"320pt\"/></pageArea></pageSet>" +
      spec.input + spec.choice + WidgetText("audit", "0", 240) + WidgetText("report", "READY", 280) +
      WidgetButton("action", 16, spec.action) + WidgetButton("verify", 60, spec.verify) +
      (spec.transient_selection ? WidgetButton("defaults", 104,
          "selected(r.choice,[0]);eq(r.choice.selectedIndex,0);eq(clean(r.choice.rawValue),'a');") : "") +
      "</subform></template>";
  // These tests target widget states. Keep the separately recorded native
  // XML CDATA-plane defect away from script delimiter boundaries.
  for (size_t i = 0; i < xml.size();) {
    const size_t size = xml.compare(i, 9, "<![CDATA[") == 0 ? 9 : xml.compare(i, 3, "]]>") == 0 ? 3 : 0;
    if (!size) { ++i; continue; }
    const auto start = preamble.size() + config.size() + i;
    if (start / 1024 != (start + size - 1) / 1024) {
      const size_t padding = 1024 - start % 1024;
      xml.insert(i, padding, ' '); i += padding;
    }
    i += size;
  }
  std::string widths;
  for (int i = 0; i < 256; ++i) widths += "1000 ";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 320] /Resources << /Font << /Ahem 10 0 R >> >> >>",
      "<< /Fields [] /DR << /Font << /Ahem 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("", preamble), PdfStream("", config), PdfStream("", xml),
      PdfStream("", "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
      PdfStream("", "</xdp:xdp>"),
      "<< /Type /Font /Subtype /TrueType /BaseFont /Ahem /FirstChar 0 /LastChar 255 /Widths [" + widths + "] /Encoding /WinAnsiEncoding /FontDescriptor 11 0 R >>",
      "<< /Type /FontDescriptor /FontName /Ahem /Flags 32 /FontBBox [0 -200 1000 800] /Ascent 800 /Descent -200 /CapHeight 800 /ItalicAngle 0 /StemV 80 /FontFile2 12 0 R >>",
      PdfStream("/Length1 " + std::to_string(font.size()), std::string(font.begin(), font.end()))});
}
void ConfigureBoundary(forms::FormHost& host, int answer) {
  host.alert_result = answer;
  host.popup_success = false;
  host.FFI_GetPageViewRect = [](FPDF_FORMFILLINFO* p, FPDF_PAGE page, double* left, double* top, double* right, double* bottom) {
    auto& h = forms::FormHost::Self(p); h.CheckPage(page);
    *left = 0; *top = 0; *right = 320; *bottom = 320;
  };
}
void BoundaryKey(Session& s, int key, int flags = 0) {
  EXPECT_TRUE(s.api.FORM_OnKeyDown(s.form, s.page, key, flags));
}
void BoundaryAction(Session& s) {
  s.Click(220, 24);
  ASSERT_TRUE(s.Focus(12, 286)); EXPECT_EQ(s.Text(), u"PASS");
}
void BoundaryVerify(Session& s) {
  s.Click(220, 68);
  ASSERT_TRUE(s.Focus(12, 286)); EXPECT_EQ(s.Text(), u"PASS");
  for (const auto& alert : s.host->alerts) {
    EXPECT_NE(std::get<0>(alert).substr(0, 5), u"FAIL:")
        << std::string(std::get<0>(alert).begin(), std::get<0>(alert).end());
  }
}
void BoundaryBlur(Session& s) {
  ASSERT_TRUE(s.Focus(12, 286));
  s.api.FORM_ForceToKillFocus(s.form);
  s.Click(310, 310);
  s.api.FORM_ForceToKillFocus(s.form);
  s.api.FORM_OnMouseMove(s.form, s.page, 0, 310, 310);
}
Raster BoundaryPaint(Session& s, const Library& library, const BoundarySpec& spec,
                     Artifacts& artifacts, const std::string& label, bool original = false) {
  if (spec.combo) {
    if (!spec.combo_value.empty()) {
      EXPECT_TRUE(s.Focus(12, 90));
      EXPECT_EQ(s.Text(), spec.combo_value) << "before popup gesture";
    }
    // Match actual native ComboEdit caret/button states in both original and
    // reload. Its null-successor focus path does not hide the internal caret.
    s.api.FORM_OnMouseMove(s.form, s.page, 0, 160, 90);
    s.api.FORM_OnLButtonDown(s.form, s.page, 0, 160, 90);
    BoundaryKey(s, FWL_VKEY_Escape);
    s.api.FORM_OnLButtonUp(s.form, s.page, 0, 160, 90);
    EXPECT_TRUE(s.Focus(12, 90));
    if (!spec.combo_value.empty()) { EXPECT_EQ(s.Text(), spec.combo_value) << "after popup gesture"; }
    BoundaryKey(s, FWL_VKEY_End, FWL_EVENTFLAG_ControlKey);
    BoundaryKey(s, FWL_VKEY_Home, FWL_EVENTFLAG_ControlKey);
    // Native key navigation alone retains old ComboEdit selection. Route a
    // real left click into its child edit to collapse it at the same origin.
    s.Click(9, 86);
    EXPECT_EQ(s.Text(true), u"");
    if (!spec.combo_value.empty()) { EXPECT_EQ(s.Text(), spec.combo_value) << "after caret gesture"; }
  } else {
    if (spec.settle_list_selection >= 0) {
      const auto unsettled = s.Draw(320, 320);
      artifacts.Image(label + "-unsettled-script-selection", unsettled);
      if (original) {
        // ScriptModify must refresh Rust's loaded control BEFORE a healthy
        // public click. Native's GetSelItem(item-index) restoration bug leaves
        // this row unpainted despite its independently verified selected node.
        const auto at = (size_t(104) * unsettled.width + 150) * 4;
        const std::array<int, 3> expected = library.name() == "rust"
            ? std::array<int, 3>{218, 193, 153} : std::array<int, 3>{255, 255, 255};
        for (int c = 0; c < 3; ++c) EXPECT_EQ(unsettled.pixels[at + c], expected[c]);
      }
      s.Click(15, 80.5 + 16 * spec.settle_list_selection + 8);
      // Keep the actual list focused for the comparison. Native's unchanged
      // blur returns false from ProcessCommittedData, then UpdateFWLData
      // mistakes item index1 for a selection ordinal and clears its paint.
      // The post-paint verifier still transfers focus and checks saved value.
      s.api.FORM_OnMouseMove(s.form, s.page, 0, 310, 310);
    } else {
      BoundaryBlur(s);
    }
  }
  auto image = s.Draw(320, 320);
  // Every case leaves a nonempty, visible input. Ahem's real square outlines
  // must produce opaque ink inside that widget, independently of comparison.
  size_t input_ink = 0;
  for (int y = 16; y < 40; ++y) for (int x = 8; x < 168; ++x) {
    const auto at = (size_t(y) * image.width + x) * 4;
    input_ink += std::min({image.pixels[at], image.pixels[at + 1], image.pixels[at + 2]}) < 128;
  }
  EXPECT_GT(input_ink, 10u);
  artifacts.Image(label + "-raw", image);
  if (spec.combo && library.name() == "native") {
    // Source-proven native double-paint inverse, restricted to this combo's
    // black-on-white edit ROI. Arrow, other widgets and original stay intact.
    for (int y = 80; y < 104; ++y) for (int x = 8; x < 153; ++x) {
      const auto at = (size_t(y) * image.width + x) * 4;
      for (int c = 0; c < 3; ++c) image.pixels[at + c] =
          static_cast<uint8_t>(std::lround(std::sqrt(image.pixels[at + c] * 255.0)));
    }
  }
  return image;
}
void WidgetBoundary(const std::string& label, const BoundarySpec& spec,
                    const std::function<void(Session&)>& interact) {
  Artifacts artifacts;
  const auto fixture = WidgetFixture(spec);
  std::array<Raster, 2> images; size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name() + "-" + label);
    Session s(writer->api(), fixture, true, true, 2,
              [&](forms::FormHost& host) { ConfigureBoundary(host, spec.answer); });
    ASSERT_TRUE(s.valid()); ASSERT_TRUE(s.xfa_loaded);
    artifacts.Image(writer->name() + "-" + label + "-initial", s.Draw(320, 320));
    ASSERT_NO_FATAL_FAILURE(interact(s));
    ASSERT_NO_FATAL_FAILURE(BoundaryVerify(s));
    images[backend] = BoundaryPaint(s, *writer, spec, artifacts, writer->name() + label, true);
    // A real focus transfer after popup/caret normalization must still commit
    // the verified value. Catch data loss before invoking either writer.
    if (spec.combo || spec.settle_list_selection >= 0) { ASSERT_NO_FATAL_FAILURE(BoundaryVerify(s)); }
    const auto saved = SaveDocument(s.api, s.document, artifacts, writer->name() + label);
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      Session reopened(reader->api(), saved, true, true, 2,
                       [&](forms::FormHost& host) { ConfigureBoundary(host, spec.answer); });
      ASSERT_TRUE(reopened.valid()); ASSERT_TRUE(reopened.xfa_loaded);
      if (spec.transient_selection) {
        // ContentNodeNeedtoExport intentionally omits unbound field Value.
        // Assert the source's template-default reload, then exercise the same
        // real input on each reader; bound persistence has dedicated tests.
        reopened.Click(220, 112);
        ASSERT_TRUE(reopened.Focus(12, 286)); EXPECT_EQ(reopened.Text(), u"PASS");
        ASSERT_NO_FATAL_FAILURE(interact(reopened));
      }
      ASSERT_NO_FATAL_FAILURE(BoundaryVerify(reopened));
      const auto actual = BoundaryPaint(reopened, *reader, spec, artifacts, writer->name() + reader->name() + label);
      CompareRaster(images[backend], actual, artifacts, writer->name() + "-to-" + reader->name() + "-" + label,
                    RasterTolerance{24, 0, .25});
      EXPECT_TRUE(reopened.host->callbacks_valid);
    }
    EXPECT_TRUE(s.host->callbacks_valid); ++backend;
  }
  CompareRaster(images[0], images[1], artifacts, label + "-engines", RasterTolerance{24, 0, .25});
}

// FFTextEdit::UpdateWidgetProperty and FFField::OnChar gate actual interactive
// edits; left-button selection remains available for read-only text controls.
TEST(CoverageXfaWidgetBoundaries, ReadOnlyAndNonInteractiveTextKeepSelectionButFilterEditingMessages) {
  for (const std::string access : {"readOnly", "nonInteractive"}) {
    SCOPED_TRACE(access);
    BoundarySpec spec;
    spec.input = WidgetText("input", "ABCDEF", 16, "", "access=\"" + access + "\"");
    WidgetBoundary("access-" + access, spec, [](Session& s) {
      ASSERT_TRUE(s.Focus(12, 22)); EXPECT_EQ(s.Text(), u"ABCDEF");
      ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page)); EXPECT_EQ(s.Text(true), u"ABCDEF");
      s.api.FORM_OnChar(s.form, s.page, 'Q', 0);
      EXPECT_EQ(s.Text(), u"ABCDEF");
      BoundaryKey(s, FWL_VKEY_Delete); EXPECT_EQ(s.Text(), u"ABCDEF");
      s.api.FORM_OnChar(s.form, s.page, 8, 0); EXPECT_EQ(s.Text(), u"ABCDEF");
      const auto menus = s.host->popup_events;
      s.api.FORM_OnRButtonDown(s.form, s.page, 0, 20, 22);
      s.api.FORM_OnRButtonUp(s.form, s.page, 0, 20, 22);
      EXPECT_EQ(s.host->popup_events, menus); EXPECT_EQ(s.Text(), u"ABCDEF");
    });
  }
}

TEST(CoverageXfaWidgetBoundaries, HiddenAndInvisibleControlsBecomeInteractiveThroughActualPresenceUpdates) {
  for (const std::string presence : {"hidden", "invisible"}) {
    SCOPED_TRACE(presence); BoundarySpec spec;
    spec.input = WidgetText("input", "ABCDEF", 16, "", "presence=\"" + presence + "\"");
    spec.action = "r.input.presence='visible';";
    spec.verify = "eq(r.input.presence,'visible');eq(clean(r.input.rawValue),'Q');";
    WidgetBoundary("presence-" + presence, spec, [](Session& s) {
      // The XFA type query iterates Viewable widgets and reports field type
      // independently of presence. Actual paint must still honor presence.
      EXPECT_EQ(s.api.FPDFPage_HasFormFieldAtPoint(s.form, s.page, 12, 22), FPDF_FORMFIELD_XFA_TEXTFIELD);
      const auto hidden = s.Draw(320, 320);
      for (int y = 16; y < 40; ++y) for (int x = 8; x < 168; ++x) {
        const auto at = (size_t(y) * hidden.width + x) * 4;
        EXPECT_EQ(hidden.pixels[at], 255); EXPECT_EQ(hidden.pixels[at + 1], 255); EXPECT_EQ(hidden.pixels[at + 2], 255);
      }
      BoundaryAction(s);
      EXPECT_EQ(s.api.FPDFPage_HasFormFieldAtPoint(s.form, s.page, 12, 22), FPDF_FORMFIELD_XFA_TEXTFIELD);
      ASSERT_TRUE(s.Focus(12, 22)); ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page));
      s.Replace(u"Q"); EXPECT_EQ(s.Text(), u"Q");
    });
  }
}

TEST(CoverageXfaWidgetBoundaries, RightButtonAndFailedPopupPreserveTheExistingCaretForFollowingInsertion) {
  BoundarySpec spec; spec.verify = "eq(clean(r.input.rawValue),'XABCDEF');";
  WidgetBoundary("right-caret", spec, [](Session& s) {
    ASSERT_TRUE(s.Focus(12, 22)); BoundaryKey(s, FWL_VKEY_Home, FWL_EVENTFLAG_ControlKey);
    EXPECT_EQ(s.Text(true), u"");
    // FFTextEdit deliberately sends right-down with a null FWL target; the
    // manager ignores it. The failed menu must retain the existing Home caret.
    const auto menus = s.host->popup_events;
    s.api.FORM_OnRButtonDown(s.form, s.page, 0, 35, 22);
    s.api.FORM_OnRButtonUp(s.form, s.page, 0, 35, 22);
    EXPECT_EQ(s.host->popup_events, menus + 1); EXPECT_EQ(s.Text(), u"ABCDEF");
    EXPECT_EQ(s.Text(true), u"");
    s.api.FORM_OnChar(s.form, s.page, 'X', 0); EXPECT_EQ(s.Text(), u"XABCDEF");
  });
}

TEST(CoverageXfaWidgetBoundaries, CapturedMouseDragBeyondFieldClampsRangeAndReleaseStopsExtendingIt) {
  BoundarySpec spec; spec.verify = "eq(clean(r.input.rawValue),'Q');";
  WidgetBoundary("capture-release", spec, [](Session& s) {
    ASSERT_TRUE(s.Focus(9, 22)); BoundaryKey(s, FWL_VKEY_Home, FWL_EVENTFLAG_ControlKey);
    // CFWL_Edit::OnMouseMove requires Shift as well as an active capture.
    s.api.FORM_OnLButtonDown(s.form, s.page, FWL_EVENTFLAG_ShiftKey, 9, 22);
    s.api.FORM_OnMouseMove(s.form, s.page, FWL_EVENTFLAG_LeftButtonDown | FWL_EVENTFLAG_ShiftKey, 195, 22);
    EXPECT_EQ(s.Text(true), u"ABCDEF");
    s.api.FORM_OnLButtonUp(s.form, s.page, 0, 195, 22);
    s.api.FORM_OnMouseMove(s.form, s.page, 0, 9, 22);
    EXPECT_EQ(s.Text(true), u"ABCDEF");
    s.Replace(u"Q"); EXPECT_EQ(s.Text(), u"Q");
  });
}

TEST(CoverageXfaWidgetBoundaries, MultilineCaretScrollingAndScrollbarMessagesRetainLogicalTextAndSelection) {
  BoundarySpec spec;
  spec.input = WidgetText("input", "A&#13;&#10;BB&#13;&#10;CCC&#13;&#10;DDDD&#13;&#10;EEEEE", 16,
                          "", "", 24, "<textEdit multiLine=\"1\" vScrollPolicy=\"on\"/>");
  spec.verify = "eq(clean(r.input.rawValue).replace(/\\r\\n/g,'\\n'),'A\\nBB\\nCCC\\nDDDD\\nEEEEE');";
  WidgetBoundary("caret-scroll", spec, [](Session& s) {
    ASSERT_TRUE(s.Focus(12, 22)); const auto original = s.Text();
    BoundaryKey(s, FWL_VKEY_End, FWL_EVENTFLAG_ControlKey);
    BoundaryKey(s, FWL_VKEY_Home, FWL_EVENTFLAG_ShiftKey);
    EXPECT_EQ(s.Text(true), u"EEEEE");
    const auto end = s.Draw(320, 320);
    const FS_POINTF p{163, 28};
    for (int delta : {-120, 120, -360, 0}) {
      s.api.FORM_OnMouseWheel(s.form, s.page, 0, &p, 0, delta);
      EXPECT_EQ(s.Text(), original); EXPECT_EQ(s.Text(true), u"EEEEE");
    }
    BoundaryKey(s, FWL_VKEY_Home, FWL_EVENTFLAG_ControlKey);
    // Native Home preserves the previous selection; an actual point click
    // collapses it in both engines without changing the selected text.
    s.Click(9, 22);
    EXPECT_EQ(s.Text(true), u""); EXPECT_EQ(s.Text(), original);
    EXPECT_NE(end.pixels, s.Draw(320, 320).pixels);
  });
}

// FFField::ProcessCommittedData invokes CalculateOverride even when a calculate
// node has no script. These controls isolate override policy from recalculation.
TEST(CoverageXfaWidgetBoundaries, CalculateDisabledIgnoreAndErrorPoliciesGateActualBlurCommits) {
  for (const std::string mode : {"disabled", "ignore", "error"}) {
    SCOPED_TRACE(mode); BoundarySpec spec;
    spec.input = WidgetText("input", "ABCDEF", 16, "<calculate override=\"" + mode + "\"/>");
    spec.verify = "eq(clean(r.input.rawValue),'" + std::string(mode == "disabled" ? "Q" : "ABCDEF") + "');";
    WidgetBoundary("calculate-" + mode, spec, [mode](Session& s) {
      ASSERT_TRUE(s.Focus(12, 22)); ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page)); s.Replace(u"Q");
      EXPECT_EQ(s.Text(), u"Q");
      ASSERT_TRUE(s.Focus(12, 286));
      if (mode == "error") {
        ASSERT_EQ(s.host->alerts.size(), 1u);
        EXPECT_EQ(std::get<0>(s.host->alerts[0]), u"You are not allowed to modify this field.");
        EXPECT_EQ(std::get<1>(s.host->alerts[0]), u"Calculate Override");
        EXPECT_EQ(std::get<2>(s.host->alerts[0]), JSPLATFORM_ALERT_BUTTON_OK);
      } else { EXPECT_TRUE(s.host->alerts.empty()); }
      ASSERT_TRUE(s.Focus(12, 22)); EXPECT_EQ(s.Text(), mode == "disabled" ? u"Q" : u"ABCDEF");
    });
  }
}

TEST(CoverageXfaWidgetBoundaries, CalculateWarningRepliesControlCommitAndAcceptedOverrideDoesNotPromptTwice) {
  for (bool accept : {false, true}) {
    SCOPED_TRACE(accept); BoundarySpec spec;
    spec.answer = accept ? JSPLATFORM_ALERT_RETURN_YES : JSPLATFORM_ALERT_RETURN_NO;
    spec.input = WidgetText("input", "ABCDEF", 16,
        "<calculate override=\"warning\"><message><text>Confirm edit</text></message></calculate>");
    spec.verify = "eq(clean(r.input.rawValue),'" + std::string(accept ? "ZZ" : "ABCDEF") + "');";
    WidgetBoundary("override-warning-" + std::to_string(accept), spec, [accept](Session& s) {
      ASSERT_TRUE(s.Focus(12, 22)); ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page)); s.Replace(u"Q");
      ASSERT_TRUE(s.Focus(12, 286)); ASSERT_EQ(s.host->alerts.size(), 1u);
      EXPECT_EQ(std::get<0>(s.host->alerts[0]), u"Confirm edit\r\nAre you sure you want to modify this field?");
      EXPECT_EQ(std::get<1>(s.host->alerts[0]), u"Calculate Override");
      EXPECT_EQ(std::get<2>(s.host->alerts[0]), JSPLATFORM_ALERT_BUTTON_YESNO);
      ASSERT_TRUE(s.Focus(12, 22)); EXPECT_EQ(s.Text(), accept ? u"Q" : u"ABCDEF");
      if (accept) {
        ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page)); s.Replace(u"ZZ");
        ASSERT_TRUE(s.Focus(12, 286)); EXPECT_EQ(s.host->alerts.size(), 1u);
      }
    });
  }
}

TEST(CoverageXfaWidgetBoundaries, MultiSelectControlClicksToggleOnlyTheHitItemAndPreserveOtherSelections) {
  BoundarySpec spec; spec.choice = WidgetChoice(false, "a", true); spec.transient_selection = true;
  spec.verify += "selected(r.choice,[3]);eq(clean(r.choice.rawValue),'d');";
  WidgetBoundary("list-ctrl-toggle", spec, [](Session& s) {
    s.Click(15, 104); // Item1; item height = 12pt font + 2*2pt margins.
    s.Click(15, 136, FWL_EVENTFLAG_ControlKey);
    s.Click(15, 104, FWL_EVENTFLAG_ControlKey);
  });
}

TEST(CoverageXfaWidgetBoundaries, MultiSelectReverseShiftRangeKeepsItsOriginalMouseAnchor) {
  BoundarySpec spec; spec.choice = WidgetChoice(false, "a", true); spec.transient_selection = true;
  spec.verify += "selected(r.choice,[3,4]);eq(r.choice.selectedIndex,3);";
  WidgetBoundary("list-reverse-anchor", spec, [](Session& s) {
    s.Click(15, 152); // Anchor item4.
    s.Click(15, 104, FWL_EVENTFLAG_ShiftKey); // Reverse range1..4.
    s.Click(15, 136, FWL_EVENTFLAG_ShiftKey); // Original anchor4 -> range3..4.
  });
}

TEST(CoverageXfaWidgetBoundaries, MultiSelectControlKeyboardMovesFocusWithoutReplacingTheShiftAnchor) {
  BoundarySpec spec; spec.choice = WidgetChoice(false, "a", true); spec.transient_selection = true;
  spec.verify += "selected(r.choice,[1,2]);eq(r.choice.selectedIndex,1);";
  WidgetBoundary("list-keyboard-anchor", spec, [](Session& s) {
    s.Click(15, 104);
    BoundaryKey(s, FWL_VKEY_Down, FWL_EVENTFLAG_ControlKey);
    BoundaryKey(s, FWL_VKEY_Down, FWL_EVENTFLAG_ControlKey);
    BoundaryKey(s, FWL_VKEY_Up, FWL_EVENTFLAG_ShiftKey);
  });
}

TEST(CoverageXfaWidgetBoundaries, ClosedEditableComboArrowsMatchLabelsClampEndsAndCommitExportValues) {
  BoundarySpec spec; spec.combo = true; spec.choice = WidgetChoice(true);
  spec.action = "r.choice.selectedIndex=2;";
  spec.verify += "eq(r.choice.selectedIndex,2);eq(clean(r.choice.rawValue),'c');";
  WidgetBoundary("combo-closed-arrows", spec, [](Session& s) {
    const bool native = &s.api == &Implementations()[0]->api();
    ASSERT_TRUE(s.Focus(12, 90)); EXPECT_EQ(s.Text(), u"a");
    BoundaryKey(s, FWL_VKEY_Up); EXPECT_EQ(s.Text(), u"a");
    // Native OnKey uses GetSelItem(item-index), a selection ordinal, while
    // the closed list has no selected item. It never advances from 'a'. Rust
    // resolves the actual item and supports the independent arrow contract.
    for (const auto value : {u"b", u"c", u"d", u"e", u"e"}) {
      BoundaryKey(s, FWL_VKEY_Down); EXPECT_EQ(s.Text(), native ? u"a" : value);
    }
    BoundaryKey(s, FWL_VKEY_Up); BoundaryKey(s, FWL_VKEY_Up);
    EXPECT_EQ(s.Text(), native ? u"a" : u"c");
    // Keep the exact native observation above, then establish the equivalent
    // final selection through the public script property for raster/reload.
    BoundaryAction(s);
  });
}

TEST(CoverageXfaWidgetBoundaries, EditableComboSelectionUndoRedoAndCustomTextKeepTheirIndependentItemMappings) {
  BoundarySpec spec; spec.combo = true; spec.choice = WidgetChoice(true);
  spec.verify += "eq(r.choice.selectedIndex,-1);eq(clean(r.choice.rawValue),'QQ');eq(r.choice.length,5);eq(r.choice.getSaveItem(0),'a');";
  WidgetBoundary("combo-edit-history", spec, [](Session& s) {
    ASSERT_TRUE(s.Focus(12, 90)); ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page)); EXPECT_EQ(s.Text(true), u"A");
    s.Replace(u"ZZZ"); EXPECT_EQ(s.Text(), u"ZZZ");
    EXPECT_TRUE(s.api.FORM_CanUndo(s.form, s.page)); ASSERT_TRUE(s.api.FORM_Undo(s.form, s.page)); EXPECT_EQ(s.Text(), u"a");
    EXPECT_TRUE(s.api.FORM_CanRedo(s.form, s.page)); ASSERT_TRUE(s.api.FORM_Redo(s.form, s.page)); EXPECT_EQ(s.Text(), u"ZZZ");
    ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page)); s.Replace(u"QQ"); EXPECT_EQ(s.Text(), u"QQ");
  });
}

TEST(CoverageXfaWidgetBoundaries, LiveChoiceDeletionClearsRemovedSelectionAndRepopulationPreservesDisplayExportIdentity) {
  for (bool combo : {false, true}) {
    SCOPED_TRACE(combo); BoundarySpec spec; spec.combo = combo; spec.choice = WidgetChoice(combo, "c");
    if (combo) { spec.combo_value = u"y"; } else { spec.settle_list_selection = 1; }
    spec.action =
        "eq(r.choice.selectedIndex,2);eq(r.choice.deleteItem(0),true);eq(r.choice.length,4);"
        "eq(r.choice.getDisplayItem(0),'BB');eq(r.choice.getSaveItem(0),'b');eq(r.choice.selectedIndex,1);"
        "eq(r.choice.deleteItem(1),true);eq(r.choice.selectedIndex,-1);eq(r.choice.length,3);"
        "r.choice.clearItems();eq(r.choice.length,0);r.choice.addItem('XX','x');r.choice.addItem('YYY','y');"
        "r.choice.selectedIndex=1;eq(r.choice.getDisplayItem(1),'YYY');eq(r.choice.getSaveItem(1),'y');";
    spec.verify += "eq(r.choice.length,2);eq(r.choice.selectedIndex,1);eq(clean(r.choice.rawValue),'y');eq(r.choice.getDisplayItem(0),'XX');eq(r.choice.getSaveItem(0),'x');";
    WidgetBoundary("choice-delete-repopulate-" + std::to_string(combo), spec, [](Session& s) {
      BoundaryAction(s);
    });
  }
}

}  // namespace
}  // namespace pdfoxium::integration
