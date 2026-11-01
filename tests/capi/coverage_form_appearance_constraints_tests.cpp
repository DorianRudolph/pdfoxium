#include "coverage_editing_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using Bytes = std::vector<uint8_t>;
constexpr int kWidth=240, kHeight=160;

struct Spec {
  std::string field="/FT /Tx /V (Alpha)", da="/Helv 12 Tf 0 g";
  std::string bs="/W 1 /S /S", mk="/BG [1 1 1] /BC [0 0 0]";
  std::string dr="/Helv 5 0 R", extra, widget_da;
  std::u16string value=u"Alpha";
};
Bytes Fixture(const Spec& s) {
  // AP is deliberately absent. Rendering before creating the SDK environment
  // reaches CPDF_AnnotList -> GenerateFormAP rather than CFFL's AP generator.
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 160] /Annots [6 0 R] /Contents 7 0 R /Resources << >> >>",
    "<< /Fields [6 0 R] /NeedAppearances true /DA ("+s.da+") /DR << /Font << "+s.dr+" >> >> >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    "<< /Type /Annot /Subtype /Widget /P 3 0 R /T (subject) /F 4 /Rect [20 40 220 140] /BS << "+s.bs+" >> /MK << "+s.mk+" >> "+s.field+" "+s.extra+" "+s.widget_da+" >>",
    PdfStream("", "0 0 1 rg 4 4 8 8 re f")});
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& name) {
  coverage_editing::TextRegions(a,b,artifacts,name,{{22,22,196,116}});
}
void Guards(const Raster& image) {
  EXPECT_EQ(coverage_editing::Pixel(image,8,151),(std::array<uint8_t,4>{255,0,0,255}));
  EXPECT_EQ(coverage_editing::Pixel(image,230,80),(std::array<uint8_t,4>{255,255,255,255}));
  coverage_editing::Ink(image,20,20,200,120);
}
using Action=std::function<void(Session&)>;
void CheckValue(Session& s,const std::u16string& expected) {
  EXPECT_EQ(s.Field(0),expected);
  EXPECT_TRUE(s.host->callbacks_valid);
  EXPECT_FALSE(s.host->callback_error);
}
void Workflow(const Spec& spec,const std::string& name,const Action& action={},
              const std::u16string& edited=u"",bool own_selection=false,
              const std::u16string& ap_token=u"BT") {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> original,active;
  size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(name+" writer="+writer->name());
    const auto& api=writer->api();
    auto fixture=Fixture(spec);
    Bytes prepared;
    {
      Handle document(api.FPDF_LoadMemDocument64(fixture.data(),fixture.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);
      Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);
      ASSERT_NE(page.get(),nullptr);
      // Widgets are not painted by this static pass, but NeedAppearances
      // must still create a usable normal AP in the document object graph.
      const auto background=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);
      EXPECT_EQ(coverage_editing::Pixel(background,8,151),(std::array<uint8_t,4>{255,0,0,255}));
      Handle annot(api.FPDFPage_GetAnnot(page.get(),0),api.FPDFPage_CloseAnnot);
      ASSERT_NE(annot.get(),nullptr);
      const auto ap=coverage_editing::Appearance(api,annot.get());
      ASSERT_FALSE(ap.empty());
      EXPECT_NE(ap.find(ap_token),std::u16string::npos);
      EXPECT_NE(ap.find(u"Tf"),std::u16string::npos);
      prepared=SaveDocument(api,document.get(),artifacts,name+"-static-"+writer->name());
      ASSERT_FALSE(prepared.empty());
    }
    Session session(api,prepared,false,false,2,[&](forms::FormHost& host){
      if(own_selection) host.FFI_OutputSelectedRect=nullptr;
    });
    ASSERT_TRUE(session.valid());
    CheckValue(session,spec.value);
    original[backend]=session.Draw(kWidth,kHeight);
    Guards(original[backend]);
    if(action) {
      ASSERT_NO_FATAL_FAILURE(action(session));
      active[backend]=session.Draw(kWidth,kHeight);
      EXPECT_EQ(session.Text(),edited);
      api.FORM_ForceToKillFocus(session.form);
      CheckValue(session,edited);
    }
    const auto stable=session.Draw(kWidth,kHeight);
    Guards(stable);
    const auto saved=SaveDocument(api,session.document,artifacts,name+"-edited-"+writer->name());
    ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE("reader="+reader->name());
      Session reloaded(reader->api(),saved);
      ASSERT_TRUE(reloaded.valid());
      CheckValue(reloaded,action?edited:spec.value);
      const auto raster=reloaded.Draw(kWidth,kHeight);
      Guards(raster);
      Compare(stable,raster,artifacts,name+"-reload-"+writer->name()+"-"+reader->name());
    }
    ++backend;
  }
  Compare(original[0],original[1],artifacts,name+"-initial");
  if(action) Compare(active[0],active[1],artifacts,name+"-active");
}
void ReplaceByTyping(Session& s) {
  ASSERT_TRUE(s.Focus(80,90));
  ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
  EXPECT_EQ(s.Text(true),u"Alpha");
  for(unsigned short c:u"Beta") { if(c) { ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,c,0)); } }
  EXPECT_EQ(s.Text(),u"Beta");
}

TEST(CoverageFormAppearanceConstraints, CatalogDefaultAppearanceIsInheritedByTheWidget) {
  Spec s;
  Workflow(s,"CatalogDefaultAppearanceIsInheritedByTheWidget");
}
TEST(CoverageFormAppearanceConstraints, WidgetDefaultAppearanceOverridesTheCatalog) {
  Spec s;s.widget_da="/DA (/Helv 15 Tf 0 g)";
  Workflow(s,"WidgetDefaultAppearanceOverridesTheCatalog");
}
TEST(CoverageFormAppearanceConstraints, MissingRequestedDefaultFontIsAddedAsHelveticaFallback) {
  Spec s;s.da="/Missing 12 Tf 0 g";
  Workflow(s,"MissingRequestedDefaultFontIsAddedAsHelveticaFallback");
}
TEST(CoverageFormAppearanceConstraints, EmptyDefaultFontResourcesReceiveTheRequestedFallback) {
  Spec s;s.dr="";
  Workflow(s,"EmptyDefaultFontResourcesReceiveTheRequestedFallback");
}
TEST(CoverageFormAppearanceConstraints, GrayTextColorIsParsedFromDefaultAppearance) {
  Spec s;s.da="/Helv 12 Tf 0.25 g";
  Workflow(s,"GrayTextColorIsParsedFromDefaultAppearance");
}
TEST(CoverageFormAppearanceConstraints, RgbTextColorIsParsedFromDefaultAppearance) {
  Spec s;s.da="/Helv 12 Tf 0.1 0.2 0.7 rg";
  Workflow(s,"RgbTextColorIsParsedFromDefaultAppearance");
}
TEST(CoverageFormAppearanceConstraints, CmykTextColorIsParsedFromDefaultAppearance) {
  Spec s;s.da="/Helv 12 Tf 1 0 0 0 k";
  Workflow(s,"CmykTextColorIsParsedFromDefaultAppearance");
}
TEST(CoverageFormAppearanceConstraints, CmykBorderAndBackgroundUseTheirOwnPaintOperators) {
  Spec s;s.mk="/BG [0 0 0 0] /BC [1 0 0 0]";
  Workflow(s,"CmykBorderAndBackgroundUseTheirOwnPaintOperators");
}
TEST(CoverageFormAppearanceConstraints, DashedBorderUsesTheDeclaredDashAndGap) {
  Spec s;s.bs="/W 2 /S /D /D [5 2]";
  Workflow(s,"DashedBorderUsesTheDeclaredDashAndGap");
}
TEST(CoverageFormAppearanceConstraints, DashedBorderWithoutAnArrayUsesTheDefaultPattern) {
  Spec s;s.bs="/W 2 /S /D";
  Workflow(s,"DashedBorderWithoutAnArrayUsesTheDefaultPattern");
}
TEST(CoverageFormAppearanceConstraints, DashedBorderWithAnEmptyArrayUsesAValidDefault) {
  Spec s;s.bs="/W 2 /S /D /D []";
  Workflow(s,"DashedBorderWithAnEmptyArrayUsesAValidDefault");
}
TEST(CoverageFormAppearanceConstraints, UnderlineBorderHasOnlyTheLowerStroke) {
  Spec s;s.bs="/W 2 /S /U";
  Workflow(s,"UnderlineBorderHasOnlyTheLowerStroke");
}
TEST(CoverageFormAppearanceConstraints, BeveledBorderProducesTwoShadedEdges) {
  Spec s;s.bs="/W 3 /S /B";
  Workflow(s,"BeveledBorderProducesTwoShadedEdges");
}
TEST(CoverageFormAppearanceConstraints, InsetBorderProducesTheReversedShadedEdges) {
  Spec s;s.bs="/W 3 /S /I";
  Workflow(s,"InsetBorderProducesTheReversedShadedEdges");
}
TEST(CoverageFormAppearanceConstraints, AbsentBorderColorLeavesTheBackgroundAndText) {
  Spec s;s.mk="/BG [0.95 0.95 0.95]";
  Workflow(s,"AbsentBorderColorLeavesTheBackgroundAndText");
}
TEST(CoverageFormAppearanceConstraints, AbsentBackgroundColorLeavesTheBorderAndText) {
  Spec s;s.mk="/BC [0 0 0]";
  Workflow(s,"AbsentBackgroundColorLeavesTheBorderAndText");
}
TEST(CoverageFormAppearanceConstraints, ZeroWidthBorderLeavesTheTextPlateUndeflated) {
  Spec s;s.bs="/W 0 /S /S";
  Workflow(s,"ZeroWidthBorderLeavesTheTextPlateUndeflated");
}
TEST(CoverageFormAppearanceConstraints, MultilineLineFeedsCreateSeparateTextPositions) {
  Spec s;s.field="/FT /Tx /Ff 4096 /V (Alpha\\nBeta\\nGamma)";s.value=u"Alpha\nBeta\nGamma";
  Workflow(s,"MultilineLineFeedsCreateSeparateTextPositions");
}
TEST(CoverageFormAppearanceConstraints, MultilineCrLfCreatesOneBreakPerParagraph) {
  Spec s;s.field="/FT /Tx /Ff 4096 /V (Alpha\\r\\nBeta)";s.value=u"Alpha\r\nBeta";
  Workflow(s,"MultilineCrLfCreatesOneBreakPerParagraph");
}
TEST(CoverageFormAppearanceConstraints, MultilineEmptyParagraphKeepsTheFollowingLinePosition) {
  Spec s;s.field="/FT /Tx /Ff 4096 /V (Alpha\\n\\nBeta)";s.value=u"Alpha\n\nBeta";
  Workflow(s,"MultilineEmptyParagraphKeepsTheFollowingLinePosition");
}
TEST(CoverageFormAppearanceConstraints, CenteredTextPositionsTheGeneratedEditStream) {
  Spec s;s.extra="/Q 1";
  Workflow(s,"CenteredTextPositionsTheGeneratedEditStream");
}
TEST(CoverageFormAppearanceConstraints, RightAlignedTextPositionsTheGeneratedEditStream) {
  Spec s;s.extra="/Q 2";
  Workflow(s,"RightAlignedTextPositionsTheGeneratedEditStream");
}
TEST(CoverageFormAppearanceConstraints, OverflowingFixedSizeTextIsClippedToTheBody) {
  // A long ordinary-size line reaches the same overflow clipping branch
  // without turning this AP-generation case into a large-glyph hinting test.
  Spec s;s.field="/FT /Tx /V (Alpha Alpha Alpha Alpha Alpha Alpha Alpha Alpha)";
  s.value=u"Alpha Alpha Alpha Alpha Alpha Alpha Alpha Alpha";
  Workflow(s,"OverflowingFixedSizeTextIsClippedToTheBody",{},u"",false,u"W\nn\nBT");
}
TEST(CoverageFormAppearanceConstraints, AutomaticFontSizeFitsTheAvailableTextPlate) {
  Spec s;s.da="/Helv 0 Tf 0 g";
  Workflow(s,"AutomaticFontSizeFitsTheAvailableTextPlate");
}
TEST(CoverageFormAppearanceConstraints, RotatedNinetyDegreeWidgetHasASwappedAppearanceBox) {
  Spec s;s.mk="/BG [1 1 1] /BC [0 0 0] /R 90";
  Workflow(s,"RotatedNinetyDegreeWidgetHasASwappedAppearanceBox");
}
TEST(CoverageFormAppearanceConstraints, RotatedOneEightyDegreeWidgetUsesItsAppearanceMatrix) {
  Spec s;s.mk="/BG [1 1 1] /BC [0 0 0] /R 180";
  Workflow(s,"RotatedOneEightyDegreeWidgetUsesItsAppearanceMatrix");
}
TEST(CoverageFormAppearanceConstraints, RotatedTwoSeventyDegreeWidgetHasASwappedAppearanceBox) {
  Spec s;s.mk="/BG [1 1 1] /BC [0 0 0] /R 270";
  Workflow(s,"RotatedTwoSeventyDegreeWidgetHasASwappedAppearanceBox");
}

// These draw the live PWL window, not just the generated PDF AP. In particular
// the null selection callback requests PWL's own blue selection background.
TEST(CoverageFormAppearanceConstraints, ActiveDashedEditBorderSurvivesARealKeyboardCommit) {
  Spec s;s.bs="/W 2 /S /D /D [4 2]";
  Workflow(s,"active-dash",ReplaceByTyping,u"Beta");
}
TEST(CoverageFormAppearanceConstraints, ActiveUnderlineEditBorderSurvivesARealKeyboardCommit) {
  Spec s;s.bs="/W 2 /S /U";
  Workflow(s,"active-underline",ReplaceByTyping,u"Beta");
}
TEST(CoverageFormAppearanceConstraints, SelectedTextPaintsItsOwnBackgroundWithoutTheHostSelectionService) {
  Workflow({},"selection-background",[](Session& s){
    ASSERT_TRUE(s.Focus(80,90));
    ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
    EXPECT_EQ(s.Text(true),u"Alpha");
    EXPECT_TRUE(s.host->selections.empty());
    const auto raster=s.Draw(kWidth,kHeight);
    size_t blue=0;
    for(int y=22;y<138;++y)for(int x=22;x<218;++x){
      const auto p=coverage_editing::Pixel(raster,x,y);
      blue+=p[0]>p[2]+60&&p[0]>p[1]+30;
    }
    EXPECT_GT(blue,20u);
  },u"Alpha",true);
}
TEST(CoverageFormAppearanceConstraints, ShiftLeftSelectionCanBePaintedAndReplacedWithoutAHostSelectionService) {
  Workflow({},"selection-shift",[](Session& s){
    ASSERT_TRUE(s.Focus(80,90));
    ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_End,0));
    ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Left,FWL_EVENTFLAG_ShiftKey));
    EXPECT_EQ(s.Text(true),u"a");
    const auto selected=s.Draw(kWidth,kHeight);
    EXPECT_FALSE(selected.pixels.empty());
    ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,'X',0));
    EXPECT_EQ(s.Text(),u"AlphX");
  },u"AlphX",true);
}

Bytes LinkFixture(const std::string& entry) {
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /Dests << /target [3 0 R /FitH 110] >> >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 160] /Annots [4 0 R] /Contents 5 0 R /Resources << >> >>",
    "<< /Type /Annot /Subtype /Link /F 4 /Rect [20 40 220 140] /Border [0 0 1] /C [0 0 1] "+entry+" >>",
    PdfStream("","0 0 1 rg 4 4 8 8 re f")});
}
using LinkCheck=std::function<void(Session&)>;
void LinkWorkflow(const std::string& name,const std::string& entry,const LinkCheck& check,int modifiers=0) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> focused;
  const auto configure=[](forms::FormHost& host) {
    // The generic form host expects Widget focus. This fixture explicitly
    // makes Links focusable and must validate those borrowed handles instead.
    host.FFI_OnFocusChange=[](FPDF_FORMFILLINFO* info,FPDF_ANNOTATION annotation,int page) {
      auto& h=forms::FormHost::Self(info);
      h.Record([&] {
        h.focus_pages.push_back(page);
        h.callbacks_valid &= page==0;
        if(annotation)h.callbacks_valid &= h.api.FPDFAnnot_GetSubtype(annotation)==FPDF_ANNOT_LINK;
      });
    };
  };
  size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(name+" writer="+writer->name());
    auto exercise=[&](Session& s,bool record){
      ASSERT_TRUE(s.valid());
      const FPDF_ANNOTATION_SUBTYPE subtype=FPDF_ANNOT_LINK;
      ASSERT_TRUE(s.api.FPDFAnnot_SetFocusableSubtypes(s.form,&subtype,1));
      Handle annot(s.api.FPDFPage_GetAnnot(s.page,0),s.api.FPDFPage_CloseAnnot);
      ASSERT_NE(annot.get(),nullptr);
      ASSERT_TRUE(s.api.FORM_SetFocusedAnnot(s.form,annot.get()));
      ASSERT_FALSE(s.host->focus_pages.empty());
      EXPECT_EQ(s.host->focus_pages.back(),0);
      if(record)focused[backend]=s.Draw(kWidth,kHeight);
      s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Return,modifiers);
      ASSERT_NO_FATAL_FAILURE(check(s));
      EXPECT_TRUE(s.host->callbacks_valid);
      EXPECT_FALSE(s.host->callback_error);
      s.api.FORM_ForceToKillFocus(s.form);
    };
    Session s(writer->api(),LinkFixture(entry),false,false,2,configure);
    ASSERT_NO_FATAL_FAILURE(exercise(s,true));
    const auto stable=s.Draw(kWidth,kHeight);
    const auto saved=SaveDocument(s.api,s.document,artifacts,name+writer->name());
    ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE("reader="+reader->name());
      Session reloaded(reader->api(),saved,false,false,2,configure);
      ASSERT_TRUE(reloaded.valid());
      CompareRaster(stable,reloaded.Draw(kWidth,kHeight),artifacts,name+"-reload-"+reader->name(),1);
      ASSERT_NO_FATAL_FAILURE(exercise(reloaded,false));
    }
    ++backend;
  }
  CompareRaster(focused[0],focused[1],artifacts,name+"-focus",1);
}
void Destination(Session& s,int mode,const std::vector<float>& args) {
  ASSERT_EQ(s.host->destinations.size(),1u);
  EXPECT_EQ(std::get<0>(s.host->destinations[0]),0);
  EXPECT_EQ(std::get<1>(s.host->destinations[0]),mode);
  EXPECT_EQ(std::get<2>(s.host->destinations[0]),args);
}
TEST(CoverageFormAppearanceConstraints, FocusedLinkReturnRunsItsKeyStrokeUriAction) {
  LinkWorkflow("link-uri","/AA << /K << /S /URI /URI (https://example.invalid/key) >> >>",[](Session& s){
    EXPECT_EQ(s.host->uris,(std::vector<std::string>{"https://example.invalid/key"}));
  });
}
TEST(CoverageFormAppearanceConstraints, FocusedLinkReturnForwardsKeyboardModifiersToItsUriCallback) {
  LinkWorkflow("link-uri-modifiers","/AA << /K << /S /URI /URI (https://example.invalid/key) >> >>",[](Session& s){
    EXPECT_EQ(s.host->uris,(std::vector<std::string>{"https://example.invalid/key"}));
    EXPECT_EQ(s.host->uri_modifiers,(std::vector<int>{FWL_EVENTFLAG_ControlKey|FWL_EVENTFLAG_ShiftKey}));
  },FWL_EVENTFLAG_ControlKey|FWL_EVENTFLAG_ShiftKey);
}
TEST(CoverageFormAppearanceConstraints, FocusedLinkReturnRejectsANamedKeyActionWithoutFallingThroughToTheDestination) {
  LinkWorkflow("link-named","/Dest [3 0 R /Fit] /AA << /K << /S /Named /N /NextPage >> >>",[](Session& s){
    // DoActionLink deliberately accepts only GoTo and URI actions.
    EXPECT_TRUE(s.host->named_actions.empty());EXPECT_TRUE(s.host->destinations.empty());
  });
}
TEST(CoverageFormAppearanceConstraints, FocusedLinkReturnRunsItsKeyStrokeGotoBeforeTheDirectDestination) {
  LinkWorkflow("link-goto","/Dest [3 0 R /Fit] /AA << /K << /S /GoTo /D [3 0 R /XYZ 12 25 1.5] >> >>",[](Session& s){Destination(s,1,{12,25,1.5f});});
}
TEST(CoverageFormAppearanceConstraints, FocusedLinkWithoutKeyActionNavigatesToItsDirectFitDestination) {
  LinkWorkflow("link-fit","/Dest [3 0 R /Fit]",[](Session& s){Destination(s,2,{});});
}
TEST(CoverageFormAppearanceConstraints, FocusedLinkWithoutKeyActionNavigatesToItsNamedDestination) {
  LinkWorkflow("link-dest-name","/Dest /target",[](Session& s){Destination(s,3,{110});});
}
TEST(CoverageFormAppearanceConstraints, FocusedLinkWithoutKeyActionRetainsFitRectangleParameters) {
  LinkWorkflow("link-fitr","/Dest [3 0 R /FitR 12 23 145 134]",[](Session& s){Destination(s,5,{12,23,145,134});});
}
TEST(CoverageFormAppearanceConstraints, FocusedLinkReturnFallsBackToThePrimaryActionBeforeTheDestination) {
  LinkWorkflow("link-ordinary-action","/Dest [3 0 R /Fit] /A << /S /URI /URI (https://example.invalid/mouse) >>",[](Session& s){
    // BAAnnot::GetAAction(KeyStroke) falls back to /A when /AA/K is absent.
    EXPECT_EQ(s.host->uris,(std::vector<std::string>{"https://example.invalid/mouse"}));
    EXPECT_TRUE(s.host->destinations.empty());EXPECT_TRUE(s.host->named_actions.empty());
  });
}

void HighlightWorkflow(const std::string& name,const std::string& color,const std::string& quads,int count) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  const auto bytes=PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 160] /Annots [4 0 R] /Resources << >> >>",
    "<< /Type /Annot /Subtype /Highlight /Rect [20 40 220 140] /F 4 /QuadPoints ["+quads+"] "+color+" >>"});
  std::array<Raster,2> originals;
  size_t backend=0;
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api();
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(),nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);
    ASSERT_NE(page.get(),nullptr);
    originals[backend]=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);
    const auto sample=coverage_editing::Pixel(originals[backend],70,40);
    EXPECT_NE(sample,(std::array<uint8_t,4>{255,255,255,255}));
    Handle annot(api.FPDFPage_GetAnnot(page.get(),0),api.FPDFPage_CloseAnnot);
    EXPECT_EQ(api.FPDFAnnot_CountAttachmentPoints(annot.get()),static_cast<size_t>(count));
    EXPECT_FALSE(coverage_editing::Appearance(api,annot.get()).empty());
    const auto saved=SaveDocument(api,doc.get(),artifacts,name+writer->name());
    ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      const auto& read=reader->api();
      Handle reloaded(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);
      ASSERT_NE(reloaded.get(),nullptr);
      Handle loaded_page(read.FPDF_LoadPage(reloaded.get(),0),read.FPDF_ClosePage);
      ASSERT_NE(loaded_page.get(),nullptr);
      CompareRaster(originals[backend],Render(read,loaded_page.get(),kWidth,kHeight,FPDF_ANNOT),artifacts,name+"-reload-"+reader->name(),1);
    }
    ++backend;
  }
  CompareRaster(originals[0],originals[1],artifacts,name+"-native-rust",1);
}
TEST(CoverageFormAppearanceConstraints, MissingHighlightAppearanceIsGeneratedForTwoSeparateQuadRows) {
  HighlightWorkflow("highlight-two","/C [1 1 0]","30 130 190 130 30 110 190 110 30 90 180 90 30 70 180 70",2);
}
TEST(CoverageFormAppearanceConstraints, MissingHighlightAppearanceUsesTheDeclaredRgbFillForAllThreeQuads) {
  HighlightWorkflow("highlight-three","/C [0.2 0.8 0.4]","30 130 190 130 30 110 190 110 30 100 180 100 30 80 180 80 30 70 160 70 30 50 160 50",3);
}
TEST(CoverageFormAppearanceConstraints, MissingHighlightAppearanceUsesTheDefaultYellowWithoutAColorEntry) {
  HighlightWorkflow("highlight-default","","30 130 190 130 30 110 190 110",1);
}
}  // namespace
}  // namespace pdfoxium::integration
