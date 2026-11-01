// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "coverage_editing_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <string>

namespace pdfoxium::integration {
namespace {
using forms::Session;
struct Spec {
  std::string key,validation,expected_border="/W 1 /S /S";
  std::u16string rust=u"SXd",native=u"SXd",dictionary=u"Seed",committed=u"SXd";
  bool cancelled=false,shifted=false,sibling_changed=false;
};
std::string KeyScript(const std::string& body) {
  if(body.empty())return {};
  return "if(!event.willCommit&&this.getField('gate').value==='on'){"
    "globalThis.windowKeys=(globalThis.windowKeys||0)+1;"+body+";app.beep(100+globalThis.windowKeys);}";
}
std::string ValidationScript(const std::string& body) {
  if(body.empty())return {};
  return "if(this.getField('gate').value==='on'){globalThis.windowValidations=(globalThis.windowValidations||0)+1;"+
    body+";app.beep(200+globalThis.windowValidations);}";
}
std::vector<uint8_t> Fixture(const Spec& spec, bool healthy_border=false) {
  const auto actions=(!spec.key.empty()?"/K "+forms::Js(KeyScript(spec.key)):"")+
    (!spec.validation.empty()?" /V "+forms::Js(ValidationScript(spec.validation)):"");
  const std::string common="/Type /Annot /Subtype /Widget /P 3 0 R /F 4 /MK << /BG [1 1 1] /BC [0 0 0] >> ";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
    "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 160] /Annots [6 0 R 7 0 R 8 0 R] /Contents 10 0 R /Resources << /Font << /Helv 5 0 R >> >> >>",
    "<< /Fields [6 0 R 7 0 R 8 0 R 9 0 R] /NeedAppearances true /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    "<< "+common+("/BS << "+(healthy_border?spec.expected_border:std::string("/W 1 /S /S"))+" >> ")+"/FT /Tx /T (input) /Rect [20 80 220 130] /V (Seed) /AA << "+actions+" >> >>",
    "<< "+common+"/BS << /W 1 /S /S >> /FT /Tx /T (sink) /Rect [20 20 220 50] /V (sink) >>",
    "<< "+common+"/BS << /W 1 /S /S >> /FT /Btn /Ff 65536 /T (disarm) /Rect [150 1 220 15] /MK << /CA (Recover) /BG [1 1 1] /BC [0 0 0] >> /A "+
      forms::Js("this.getField('gate').value='off';app.beep(300);")+" >>",
    "<< /FT /Tx /T (gate) /V (on) >>",
    PdfStream("","0 1 0 rg 4 4 8 8 re f")});
}
void SelectPending(Session& s) {
  ASSERT_TRUE(s.Focus(30,110));s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Home,0);
  s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Right,0);
  for(int i=0;i<2;++i)s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey);
  ASSERT_EQ(s.Text(true),u"ee");
}
void Inspect(const Raster& image) {
  EXPECT_EQ(coverage_editing::Pixel(image,8,151),(std::array<uint8_t,4>{0,255,0,255}));
  forms::ExpectInk(image,24,34,216,76);
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,RasterTolerance::ConnectedError(1.0));
}
void CompareOriginal(const Raster& a,const Raster& b,const Spec& spec,Artifacts& artifacts) {
  if(spec.key.empty()) {Compare(a,b,artifacts,"pending-window-original");return;}
  // The pinned native ordinary OnAAction false-return defect ignores event
  // rewrites/rc and skips age repair, leaving its old PWL border/style even
  // when text happens to match. GetCreateParam snapshots width/style and
  // OnDraw uses the existing PWL window without refreshing its age. Compare
  // outside this one widget; independently check Rust's fresh border below,
  // then compare the complete common recovered scene at the same tolerance.
  auto left=a,right=b;
  for(int y=30;y<80;++y)for(int x=20;x<220;++x)for(int c=0;c<4;++c) {
    left.pixels[(size_t(y)*240+x)*4+c]=255;right.pixels[(size_t(y)*240+x)*4+c]=255;
  }
  Compare(left,right,artifacts,"pending-window-outside-native-stale-widget");
}
void CheckFreshRustBorder(const Api& api,const Raster& pending,const Spec& spec,Artifacts& artifacts) {
  // Build the expected style into an independent fixture before any PWL
  // window exists. This control cannot take the action/age-repair path.
  Spec control;control.expected_border=spec.expected_border;
  Session healthy(api,Fixture(control,true),true);ASSERT_TRUE(healthy.valid());
  ASSERT_TRUE(healthy.Focus(30,110));auto expected=healthy.Draw(240,160),actual=pending;
  // The top/bottom three-pixel border strips avoid all live text; exact text and
  // selection are asserted separately and full recovered scenes below.
  for(int y=0;y<160;++y)for(int x=0;x<240;++x) {
    const bool frame=x>=20&&x<220&&y>=30&&y<80&&
      (y<33||y>=77);
    if(!frame)for(int c=0;c<4;++c) {
      expected.pixels[(size_t(y)*240+x)*4+c]=255;
      actual.pixels[(size_t(y)*240+x)*4+c]=255;
    }
  }
  Compare(expected,actual,artifacts,"pending-rust-border-versus-independent-healthy-window");
}
void Settle(Session& s) {
  s.api.FORM_ForceToKillFocus(s.form);s.Click(235,155);s.api.FORM_OnMouseMove(s.form,s.page,0,235,155);
}
void DisarmAndContinue(Session& s) {
  s.host->beeps.clear();s.Click(170,7);
  ASSERT_EQ(s.host->beeps.size(),1u);ASSERT_EQ(s.host->beeps[0],300);
  ASSERT_TRUE(s.Focus(35,110));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"Stable");
  ASSERT_EQ(s.Text(),u"Stable");s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_End,0);
  ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,'Y',0));ASSERT_EQ(s.Text(),u"StableY");
  EXPECT_EQ(s.host->beeps.size(),1u)<<"Disarmed scripts must not replay the pending key";
  ASSERT_TRUE(s.Focus(30,35));Settle(s);ASSERT_EQ(s.Field(0),u"StableY");
}
void ProbeReload(Session& s) {
  ASSERT_EQ(s.Field(0),u"StableY");ASSERT_TRUE(s.Focus(35,110));ASSERT_EQ(s.Text(),u"StableY");
  ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"Reloaded");ASSERT_EQ(s.Text(),u"Reloaded");
  s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_End,0);s.api.FORM_OnChar(s.form,s.page,'Z',0);ASSERT_EQ(s.Text(),u"ReloadedZ");
  ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"StableY");ASSERT_EQ(s.Text(),u"StableY");
  ASSERT_TRUE(s.Focus(30,35));Settle(s);ASSERT_EQ(s.Field(0),u"StableY");
  EXPECT_TRUE(s.host->beeps.empty())<<"The serialized gate must remain disarmed";
}
void RunScenario(const std::string& name,const Spec& spec) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);Artifacts artifacts;
  std::array<Raster,2> original,common;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(name+" writer="+writer->name());Session s(writer->api(),Fixture(spec),true);ASSERT_TRUE(s.valid());
    const auto before=s.Draw(240,160);
  ASSERT_NO_FATAL_FAILURE(SelectPending(s));s.host->beeps.clear();
    // Retiring the PWL dispatch window may legitimately return false even
    // though the script's pending edit was consumed by its fresh replacement.
    s.api.FORM_OnChar(s.form,s.page,'X',0);
    ASSERT_EQ(s.Text(),backend==0?spec.native:spec.rust);
    if(spec.validation.empty()) {
      ASSERT_EQ(s.host->beeps.size(),1u);EXPECT_EQ(s.host->beeps[0],101);
      EXPECT_EQ(s.Field(0),spec.dictionary)<<"A pending UI edit is not an extra field commit";
      if(spec.cancelled&&backend==1){EXPECT_EQ(s.Text(true),u"ee");}
    } else {
      EXPECT_TRUE(s.host->beeps.empty());ASSERT_TRUE(s.Focus(30,35));
      ASSERT_EQ(s.host->beeps.size(),1u);EXPECT_EQ(s.host->beeps[0],201);
      EXPECT_EQ(s.Field(0),spec.committed);
      if(spec.sibling_changed){EXPECT_EQ(s.Field(1),u"Reset");}
    }
    if(spec.shifted) {
      Handle annotation(s.api.FPDFPage_GetAnnot(s.page,0),s.api.FPDFPage_CloseAnnot);ASSERT_NE(annotation.get(),nullptr);
      FS_RECTF rect{};ASSERT_TRUE(s.api.FPDFAnnot_GetRect(annotation.get(),&rect));EXPECT_FLOAT_EQ(rect.left,24);EXPECT_FLOAT_EQ(rect.right,224);
    }
    original[backend]=s.Draw(240,160);
  ASSERT_NO_FATAL_FAILURE(Inspect(original[backend]));
    if(backend==1&&!spec.key.empty()&&!spec.shifted) {
      ASSERT_NO_FATAL_FAILURE(CheckFreshRustBorder(s.api,original[backend],spec,artifacts));
    }
    if(backend==1&&spec.cancelled&&spec.expected_border=="/W 1 /S /S") {
      // Veto restores the identical visible Seed buffer; the document value
      // side effect Reset is checked independently above.
      Compare(before,original[backend],artifacts,"cancelled-value-restores-original-paint");
    } else {
      EXPECT_NE(original[backend].pixels,before.pixels)<<"The action must affect real form paint";
    }
    ASSERT_NO_FATAL_FAILURE(DisarmAndContinue(s));common[backend]=s.Draw(240,160);
  ASSERT_NO_FATAL_FAILURE(Inspect(common[backend]));
    for(FPDF_DWORD flags:{FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}) {
      const auto saved=SaveDocument(s.api,s.document,artifacts,name+"-"+writer->name()+"-"+std::to_string(flags),flags);ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());Session reload(reader->api(),saved,true);ASSERT_TRUE(reload.valid());
        ASSERT_NO_FATAL_FAILURE(ProbeReload(reload));const auto image=reload.Draw(240,160);Inspect(image);
        Compare(common[backend],image,artifacts,name+"-"+writer->name()+"-"+reader->name()+"-"+std::to_string(flags));
        EXPECT_TRUE(reload.host->callbacks_valid);EXPECT_FALSE(reload.host->callback_error);
      }
    }
    EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);++backend;
  }
  CompareOriginal(original[0],original[1],spec,artifacts);Compare(common[0],common[1],artifacts,"recreated-window-common-scene");
}
Spec Key(std::string body,std::u16string rust=u"SXd",std::string border="/W 3 /S /S"){Spec s;s.key=std::move(body);s.rust=std::move(rust);s.expected_border=std::move(border);return s;}
Spec Validation(std::string body,std::u16string committed=u"SXd"){Spec s;s.validation=std::move(body);s.committed=std::move(committed);return s;}
TEST(CoverageFormWindowRecreation, LineWidthChangeDuringInputPreservesThePendingRange) {RunScenario("width-recreate",Key("event.target.lineWidth=3;"));}
TEST(CoverageFormWindowRecreation, BorderStyleChangeDuringInputPreservesThePendingRange) {RunScenario("style-recreate",Key("event.target.borderStyle='dashed';",u"SXd","/W 1 /S /D"));}
TEST(CoverageFormWindowRecreation, RectangleChangeDuringInputKeepsTheFreshWindowUsable) {auto s=Key("event.target.rect=[24,130,224,80];",u"SXd","/W 1 /S /S");s.shifted=true;RunScenario("rect-recreate",s);}
TEST(CoverageFormWindowRecreation, AppearanceChangeAppliesTheRewrittenChangeOnlyOnce) {RunScenario("rewrite-change",Key("event.target.lineWidth=3;event.change='XY';",u"SXYd"));}
TEST(CoverageFormWindowRecreation, RejectedInputAfterAppearanceChangeRestoresTheSavedSelection) {auto s=Key("event.target.lineWidth=3;event.rc=false;",u"Seed");s.cancelled=true;RunScenario("reject-recreate",s);}
TEST(CoverageFormWindowRecreation, AppearanceChangeUsesTheRewrittenSelectionEndpoints) {RunScenario("rewrite-range",Key("event.target.lineWidth=3;event.selStart=0;event.selEnd=event.value.length;event.change='Done';",u"Done"));}
TEST(CoverageFormWindowRecreation, ScriptValueChangeReloadsTheFreshWindowBeforePendingInput) {auto s=Key("event.target.value='Reset';",u"RXet","/W 1 /S /S");s.dictionary=u"Reset";RunScenario("value-recreate",s);}
TEST(CoverageFormWindowRecreation, CancelAfterScriptValueChangeRetainsTheSavedUiAndScriptSideEffect) {auto s=Key("event.target.value='Reset';event.rc=false;",u"Seed","/W 1 /S /S");s.dictionary=u"Reset";s.cancelled=true;RunScenario("value-cancel",s);}
TEST(CoverageFormWindowRecreation, ValidationAppearanceChangeCommitsThePendingValueOnlyOnce) {RunScenario("validate-width",Validation("event.target.lineWidth=3;"));}
TEST(CoverageFormWindowRecreation, ValidationBorderChangeCommitsThePendingValueOnlyOnce) {RunScenario("validate-style",Validation("event.target.borderStyle='dashed';"));}
// Setting the validating field's own value recursively requests the same
// Validate event and is rejected by the runtime's duplicate-event contract.
// A sibling value write is a legal side effect while this field is committed.
TEST(CoverageFormWindowRecreation, ValidationAppearanceChangePreservesTheSiblingValueSideEffect) {auto s=Validation("event.target.lineWidth=3;this.getField('sink').value='Reset';");s.sibling_changed=true;RunScenario("validate-sibling-value",s);}
TEST(CoverageFormWindowRecreation, RejectedValidationRetainsTheOriginalValueAndChangedAppearance) {RunScenario("validate-reject",Validation("event.target.lineWidth=3;event.rc=false;",u"Seed"));}
}  // namespace
}  // namespace pdfoxium::integration
