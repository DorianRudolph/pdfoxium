#include "coverage_editing_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using Configure=std::function<void(forms::FormHost&)>;
using Observe=std::function<void(Session&)>;
struct OptionalSpec {
  std::string code,action;
  Configure configure;
  Observe before,inspect;
  int flags=0;
  bool no_release=false;
  std::u16string input=u"Alpha", report=u"PASS";
};
std::vector<uint8_t> Fixture(const OptionalSpec& s) {
  const std::string script="function check(v){if(!v)throw Error('optional service contract');} "
    "try{"+s.code+";this.getField('report').value='PASS';}catch(e){this.getField('report').value='FAIL:'+String(e);}";
  const std::string widget="/Type /Annot /Subtype /Widget /P 3 0 R /F 4 /BS << /W 1 >> /MK << /BG [1 1 1] /BC [0 0 0] >> ";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
    "<< /Type /Pages /Kids [3 0 R 8 0 R] /Count 2 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 160] /Annots [6 0 R 7 0 R 9 0 R] /Contents 10 0 R /Resources << >> >>",
    "<< /Fields [6 0 R 7 0 R 9 0 R] /NeedAppearances true /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    "<< "+widget+"/FT /Tx /T (input) /V (Alpha) /Rect [20 100 220 130] /Ff "+std::to_string(s.flags)+" >>",
    "<< "+widget+"/FT /Tx /T (report) /V (Ready) /Ff 1 /Rect [20 60 220 90] >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 160] /Resources << >> >>",
    "<< "+widget+"/FT /Btn /T (run) /Ff 65536 /Rect [20 20 220 45] /A "+(s.action.empty()?forms::Js(script):s.action)+" >>",
    PdfStream("","0 0 1 rg 4 4 8 8 re f")});
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  coverage_editing::TextRegions(a,b,artifacts,label,{{22,32,196,26},{22,72,196,26},{22,117,196,21}});
}
void Check(Session& s,const OptionalSpec& spec) {
  EXPECT_EQ(s.Field(0),spec.input);
  ASSERT_EQ(s.Field(1),spec.report);
  EXPECT_TRUE(s.host->callbacks_valid);
  EXPECT_FALSE(s.host->callback_error);
}
void Edit(Session& s) {
  ASSERT_TRUE(s.Focus(80,115));
  ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
  EXPECT_EQ(s.Text(true),u"Alpha");
  for(unsigned short c:u"Beta") { if(c) { ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,c,0)); } }
  EXPECT_EQ(s.Text(),u"Beta");
  s.api.FORM_ForceToKillFocus(s.form);
  EXPECT_EQ(s.Field(0),u"Beta");
}
void Workflow(const std::string& name,const OptionalSpec& spec) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> originals;
  size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(name+" writer="+writer->name());
    Session s(writer->api(),Fixture(spec),true,false,2,spec.configure);
    ASSERT_TRUE(s.valid());
    // JavaScript remains enabled: only individual optional service pointers
    // are absent, never the environment or m_pJsPlatform itself.
    ASSERT_NE(s.host->m_pJsPlatform,nullptr);
    ASSERT_NE(s.host->platform.app_alert,nullptr);
    ASSERT_NE(s.host->platform.app_response,nullptr);
    ASSERT_NE(s.host->platform.app_beep,nullptr);
    ASSERT_NE(s.host->platform.Doc_mail,nullptr);
    ASSERT_NE(s.host->platform.Doc_print,nullptr);
    ASSERT_NE(s.host->platform.Doc_getFilePath,nullptr);
    ASSERT_NE(s.host->platform.Doc_gotoPage,nullptr);
    ASSERT_NE(s.host->platform.Doc_submitForm,nullptr);
    ASSERT_NE(s.host->platform.Field_browse,nullptr);
    if(spec.before) { ASSERT_NO_FATAL_FAILURE(spec.before(s)); }
    s.Click(60,32);
    s.api.FORM_ForceToKillFocus(s.form);
    s.api.FORM_OnMouseMove(s.form,s.page,0,235,150);
    ASSERT_NO_FATAL_FAILURE(Check(s,spec));
    if(spec.inspect) { ASSERT_NO_FATAL_FAILURE(spec.inspect(s)); }
    originals[backend]=s.Draw(240,160);
    EXPECT_EQ(coverage_editing::Pixel(originals[backend],8,151),(std::array<uint8_t,4>{255,0,0,255}));
    coverage_editing::Ink(originals[backend],22,32,196,26);
    const auto saved=SaveDocument(s.api,s.document,artifacts,name+"-"+writer->name());
    ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE("reader="+reader->name());
      Session reloaded(reader->api(),saved,true,false,2,spec.configure);
      ASSERT_TRUE(reloaded.valid());
      ASSERT_NO_FATAL_FAILURE(Check(reloaded,spec));
      Compare(originals[backend],reloaded.Draw(240,160),artifacts,name+"-reload-"+writer->name()+"-"+reader->name());
    }
    s.Close();
    EXPECT_EQ(s.host->releases,spec.no_release?0:1);
    ++backend;
  }
  Compare(originals[0],originals[1],artifacts,name+"-native-rust");
}

// FFI_GetPlatform/GetLanguage are required only for XFA. The XFA-disabled
// AcroForm document exercises their legitimate reader-string fallbacks.
TEST(CoverageOptionalHostServices, MissingPlatformServiceUsesTheReaderPlatformFallback) {
  OptionalSpec s;
  s.code="check(app.platform==='WIN')";
  s.configure=[](forms::FormHost& h){h.FFI_GetPlatform=nullptr;};
  Workflow("MissingPlatformServiceUsesTheReaderPlatformFallback",s);
}

TEST(CoverageOptionalHostServices, MissingLanguageServiceUsesTheReaderLanguageFallback) {
  OptionalSpec s;
  s.code="check(app.language==='ENU')";
  s.configure=[](forms::FormHost& h){h.FFI_GetLanguage=nullptr;};
  Workflow("MissingLanguageServiceUsesTheReaderLanguageFallback",s);
}

TEST(CoverageOptionalHostServices, MissingChangeNotificationServiceDoesNotDiscardCommittedText) {
  OptionalSpec s;s.input=u"Beta";s.before=[](Session& s){Edit(s);};
  s.code="check(this.getField('input').value==='Beta')";
  s.configure=[](forms::FormHost& h){h.FFI_OnChange=nullptr;};
  s.inspect=[](Session& s){EXPECT_EQ(s.host->changes,0);};
  Workflow("MissingChangeNotificationServiceDoesNotDiscardCommittedText",s);
}

TEST(CoverageOptionalHostServices, MissingTextFieldFocusServiceDoesNotBlockTheEditor) {
  OptionalSpec s;s.input=u"Beta";s.before=[](Session& s){Edit(s);};
  s.code="check(this.getField('input').value==='Beta')";
  s.configure=[](forms::FormHost& h){h.FFI_SetTextFieldFocus=nullptr;};
  s.inspect=[](Session& s){EXPECT_TRUE(s.host->focus_values.empty());};
  Workflow("MissingTextFieldFocusServiceDoesNotBlockTheEditor",s);
}

TEST(CoverageOptionalHostServices, MissingAnnotationFocusServiceDoesNotBlockFocusTransitions) {
  OptionalSpec s;s.input=u"Beta";s.before=[](Session& s){Edit(s);};
  s.code="check(this.getField('input').value==='Beta')";
  s.configure=[](forms::FormHost& h){h.FFI_OnFocusChange=nullptr;};
  s.inspect=[](Session& s){EXPECT_TRUE(s.host->focus_pages.empty());};
  Workflow("MissingAnnotationFocusServiceDoesNotBlockFocusTransitions",s);
}

TEST(CoverageOptionalHostServices, MissingSelectionServicePaintsTheSelectionAndAllowsReplacingIt) {
  OptionalSpec s;s.input=u"Beta";
  s.before=[](Session& s){
    ASSERT_TRUE(s.Focus(80,115));
    ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
    EXPECT_EQ(s.Text(true),u"Alpha");
    const auto selected=s.Draw(240,160);
    size_t blue=0;
    for(int y=32;y<58;++y)for(int x=22;x<218;++x){
      const auto p=coverage_editing::Pixel(selected,x,y);
      blue+=p[0]>p[2]+60&&p[0]>p[1]+30;
    }
    EXPECT_GT(blue,20u);
    for(unsigned short c:u"Beta") { if(c) { ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,c,0)); } }
    s.api.FORM_ForceToKillFocus(s.form);
    EXPECT_EQ(s.Field(0),u"Beta");
  };
  s.code="check(this.getField('input').value==='Beta')";
  s.configure=[](forms::FormHost& h){h.FFI_OutputSelectedRect=nullptr;};
  s.inspect=[](Session& s){EXPECT_TRUE(s.host->selections.empty());};
  Workflow("MissingSelectionServicePaintsTheSelectionAndAllowsReplacingIt",s);
}

TEST(CoverageOptionalHostServices, UriActionUsesTheLegacyServiceWhenTheModifierServiceIsAbsent) {
  OptionalSpec s;s.report=u"Ready";
  s.action="<< /S /URI /URI (https://example.invalid/optional) >>";
  s.configure=[](forms::FormHost& h){h.FFI_DoURIActionWithKeyboardModifier=nullptr;};
  s.inspect=[](Session& s){
    EXPECT_EQ(s.host->uris,(std::vector<std::string>{"https://example.invalid/optional"}));
    EXPECT_EQ(s.host->uri_modifiers,(std::vector<int>{0}));
  };
  Workflow("uri-legacy-fallback",s);
}
TEST(CoverageOptionalHostServices, MissingBothUriServicesDoesNotDamageEditedFieldsOrSaving) {
  OptionalSpec s;s.report=u"Ready";s.input=u"Beta";s.before=[](Session& s){Edit(s);};
  s.action="<< /S /URI /URI (https://example.invalid/optional) >>";
  s.configure=[](forms::FormHost& h){h.FFI_DoURIAction=nullptr;h.FFI_DoURIActionWithKeyboardModifier=nullptr;};
  s.inspect=[](Session& s){EXPECT_TRUE(s.host->uris.empty());};
  Workflow("uri-missing",s);
}
TEST(CoverageOptionalHostServices, MissingDestinationServiceDoesNotDamageEditedFieldsOrSaving) {
  OptionalSpec s;s.report=u"Ready";s.input=u"Beta";s.before=[](Session& s){Edit(s);};
  s.action="<< /S /GoTo /D [8 0 R /XYZ 12 25 1.5] >>";
  s.configure=[](forms::FormHost& h){h.FFI_DoGoToAction=nullptr;};
  s.inspect=[](Session& s){EXPECT_TRUE(s.host->destinations.empty());};
  Workflow("destination-missing",s);
}
TEST(CoverageOptionalHostServices, AnEmbedderWithoutOptionalUiNotificationsStillCommitsAndSavesKeyboardEdits) {
  OptionalSpec s;s.input=u"Beta";s.before=[](Session& s){Edit(s);};
  s.code="check(this.getField('input').value==='Beta')";
  s.configure=[](forms::FormHost& h){
    h.FFI_OnChange=nullptr;
    h.FFI_SetTextFieldFocus=nullptr;h.FFI_OnFocusChange=nullptr;
    h.FFI_OutputSelectedRect=nullptr;
  };
  s.inspect=[](Session& s){
    EXPECT_EQ(s.host->changes,0);
    EXPECT_TRUE(s.host->focus_values.empty());EXPECT_TRUE(s.host->focus_pages.empty());
  };
  Workflow("passive-ui-optional-table",s);
}

TEST(CoverageOptionalHostServices, MissingReleaseCallbackStillClosesEditorsTimersAndTheDocument) {
  OptionalSpec s;s.no_release=true;s.input=u"Beta";
  s.configure=[](forms::FormHost& h){h.Release=nullptr;};
  s.before=[](Session& s){Edit(s);};
  s.code="check(this.getField('input').value==='Beta')";
  Workflow("release-missing",s);
}
}  // namespace
}  // namespace pdfoxium::integration
