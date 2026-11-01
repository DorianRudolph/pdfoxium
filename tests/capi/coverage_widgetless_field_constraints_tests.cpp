#include "coverage_editing_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
struct Spec {
  std::string body,verify;
  int text_flags=0,list_flags=2097152;
  bool browse=false;
};
std::string Checked(const std::string& body,bool toggle=false) {
  const std::string success=toggle?
    "this.getField('report').value=this.getField('report').valueAsString==='PASS'?'VERIFIED':'PASS';":
    "this.getField('report').value='PASS';";
  return "function eq(a,b){if(JSON.stringify(a)!==JSON.stringify(b))throw 'expected '+JSON.stringify(b)+' got '+JSON.stringify(a);}"
    "function fails(f){var caught=false;try{f();}catch(e){caught=true;}if(!caught)throw 'expected exception';}"
    "try{"+body+";"+success+"}catch(e){this.getField('report').value='FAIL: '+String(e);}";
}
std::vector<uint8_t> Fixture(const Spec& spec) {
  // Terminal AcroForm fields may hold values without any widget annotation.
  // Only the four independent healthy controls are in the page's Annots.
  const std::string widget="/Type /Annot /Subtype /Widget /P 3 0 R /F 4 /BS << /W 1 >> /MK << /BG [1 1 1] /BC [0 0 0] >> ";
  const std::string options="[[(one) (One)] [(two) (Two)] [(three) (Three)]]";
  const auto run=forms::Js(Checked("eq(this.numFields,11);"+spec.body));
  const auto verify=forms::Js(Checked("eq(this.getField('visible').valueAsString,'Ready');"+spec.verify,true));
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 160] /Annots [6 0 R 7 0 R 8 0 R 9 0 R] /Contents 17 0 R /Resources << >> >>",
    "<< /Fields [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R 14 0 R 15 0 R 16 0 R] /NeedAppearances true /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    "<< "+widget+"/FT /Tx /T (visible) /V (Alpha) /DV (Safe) /Rect [20 100 220 130] >>",
    "<< "+widget+"/FT /Tx /T (report) /V (Ready) /Ff 1 /Rect [20 60 220 90] >>",
    "<< "+widget+"/FT /Btn /Ff 65536 /T (run) /Rect [20 20 110 45] /A "+run+" >>",
    "<< "+widget+"/FT /Btn /Ff 65536 /T (verify) /Rect [130 20 220 45] /A "+verify+" >>",
    "<< /FT /Tx /T (dataText) /TU (Data label) /V (Alpha) /DV (Default) /MaxLen 12 /Ff "+std::to_string(spec.text_flags)+" >>",
    "<< /FT /Btn /T (dataCheck) /V /Off /DV /Off >>",
    "<< /FT /Btn /Ff 32768 /T (dataRadio) /V /Off /DV /Off >>",
    "<< /FT /Btn /Ff 65536 /T (dataPush) >>",
    "<< /FT /Ch /Ff "+std::to_string(spec.list_flags)+" /T (dataList) /Opt "+options+" /V (one) /DV (two) /I [0] >>",
    "<< /FT /Ch /Ff 131072 /T (dataCombo) /Opt "+options+" /V (one) /DV (two) >>",
    "<< /FT /Sig /T (dataSignature) >>",
    PdfStream("","0 0 1 rg 4 4 8 8 re f")});
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  coverage_editing::TextRegions(a,b,artifacts,label,{{22,32,196,26},{22,72,196,26}});
}
void Verify(Session& s) {
  EXPECT_EQ(s.api.FPDFPage_GetAnnotCount(s.page),4);
  ASSERT_EQ(s.Field(1),u"PASS");
  // Each dispatch must change the report, including after reload. Restore
  // PASS with the second successful dispatch to preserve the final scene.
  for(const auto& expected:{u"VERIFIED",u"PASS"}) {
    s.Click(160,32);
    s.api.FORM_ForceToKillFocus(s.form);
    s.api.FORM_OnMouseMove(s.form,s.page,0,235,150);
    ASSERT_EQ(s.Field(1),expected);
  }
  EXPECT_EQ(s.Field(0),u"Ready");
  EXPECT_TRUE(s.host->callbacks_valid);
  EXPECT_FALSE(s.host->callback_error);
}
void Workflow(const std::string& name,const Spec& spec) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<Raster,2> originals;
  size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(name+" writer="+writer->name());
    Session s(writer->api(),Fixture(spec),true);
    ASSERT_TRUE(s.valid());
    s.Click(50,32);
    s.api.FORM_ForceToKillFocus(s.form);
    ASSERT_EQ(s.Field(1),u"PASS");
    // Prove failed control-property accesses left a real editor usable.
    ASSERT_TRUE(s.Focus(80,115));
    ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
    for(unsigned short c:u"Ready") {
      if(c) { ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,c,0)); }
    }
    s.api.FORM_ForceToKillFocus(s.form);
    ASSERT_NO_FATAL_FAILURE(Verify(s));
    if(spec.browse) { EXPECT_EQ(s.host->browse_queries,2); }
    originals[backend]=s.Draw(240,160);
    EXPECT_EQ(coverage_editing::Pixel(originals[backend],8,151),(std::array<uint8_t,4>{255,0,0,255}));
    coverage_editing::Ink(originals[backend],22,32,196,26);
    const auto saved=SaveDocument(s.api,s.document,artifacts,name+"-"+writer->name());
    ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE("reader="+reader->name());
      Session reloaded(reader->api(),saved,true);
      ASSERT_TRUE(reloaded.valid());
      ASSERT_NO_FATAL_FAILURE(Verify(reloaded));
      Compare(originals[backend],reloaded.Draw(240,160),artifacts,name+"-reload-"+writer->name()+"-"+reader->name());
    }
    ++backend;
  }
  Compare(originals[0],originals[1],artifacts,name+"-native-rust");
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessTextValueCanBeChangedAndResetByItsTerminalFieldName) {
  Spec s;
  s.body="var f=this.getField('dataText');f.value='Changed';eq(f.valueAsString,'Changed');this.resetForm(['dataText']);eq(f.valueAsString,'Default');";
  s.verify="eq(this.getField('dataText').valueAsString,'Default');";
  Workflow("WidgetlessTextValueCanBeChangedAndResetByItsTerminalFieldName",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessDefaultValueSetterPreservesTheSourceValidationOnlyBoundary) {
  Spec s;
  s.body="var f=this.getField('dataText');eq(f.defaultValue,'Default');f.defaultValue='Ignored';eq(f.defaultValue,'Default');f.value='Changed';this.resetForm(['dataText']);";
  s.verify="eq(this.getField('dataText').defaultValue,'Default');eq(this.getField('dataText').valueAsString,'Default');";
  Workflow("WidgetlessDefaultValueSetterPreservesTheSourceValidationOnlyBoundary",s);
}

TEST(CoverageWidgetlessFieldConstraints, EmptyArrayAssignmentDoesNotDestroyAWidgetlessTextValue) {
  Spec s;
  s.body="var f=this.getField('dataText');f.value=[];eq(f.valueAsString,'Alpha');";
  s.verify="eq(this.getField('dataText').valueAsString,'Alpha');";
  Workflow("EmptyArrayAssignmentDoesNotDestroyAWidgetlessTextValue",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessReadonlyFlagIsPersistentWithoutInventingAWidget) {
  Spec s;
  s.body="var f=this.getField('dataText');f.readonly=true;eq(f.readonly,true);eq(f.charLimit,12);eq(f.userName,'Data label');";
  s.verify="var f=this.getField('dataText');eq(f.readonly,true);eq(f.charLimit,12);eq(f.userName,'Data label');";
  Workflow("WidgetlessReadonlyFlagIsPersistentWithoutInventingAWidget",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessPermissionOnlyFlagsDoNotRewriteTheAuthoredFieldDictionary) {
  Spec s;
  s.body="var f=this.getField('dataText');f.required=true;f.multiline=true;f.password=true;f.comb=true;f.charLimit=3;f.userName='Ignored';eq(f.required,false);eq(f.multiline,false);eq(f.password,false);eq(f.comb,false);eq(f.charLimit,12);eq(f.userName,'Data label');";
  s.verify="var f=this.getField('dataText');eq(f.required,false);eq(f.multiline,false);eq(f.charLimit,12);eq(f.userName,'Data label');";
  Workflow("WidgetlessPermissionOnlyFlagsDoNotRewriteTheAuthoredFieldDictionary",s);
}

TEST(CoverageWidgetlessFieldConstraints, ControlColorAndRotationReadsRequireAnActualWidget) {
  Spec s;
  s.body="var f=this.getField('dataText');['textColor','fillColor','strokeColor','rotation','lineWidth','alignment'].forEach(function(p){fails(function(){return f[p];});});eq(f.valueAsString,'Alpha');";
  s.verify="eq(this.getField('dataText').valueAsString,'Alpha');";
  Workflow("ControlColorAndRotationReadsRequireAnActualWidget",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessPresentationSettersRemainUsableWithoutCreatingPageAnnotations) {
  Spec s;
  s.body="var f=this.getField('dataText');f.borderStyle='dashed';f.lineWidth=3;f.display=display.hidden;f.hidden=true;f.print=false;f.textColor=['RGB',1,0,0];f.fillColor=['G',0.8];f.strokeColor=['G',0];f.rotation=90;f.textSize=15;f.textFont='Courier';eq(f.valueAsString,'Alpha');";
  s.verify="eq(this.getField('dataText').valueAsString,'Alpha');";
  Workflow("WidgetlessPresentationSettersRemainUsableWithoutCreatingPageAnnotations",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessCheckFieldsHaveNoExportStatesOrCheckableControlIndices) {
  Spec s;
  s.body="var f=this.getField('dataCheck');eq(f.type,'checkbox');eq(f.exportValues,[]);eq(f.value,'Off');fails(function(){return f.valueAsString;});fails(function(){f.checkThisBox(0,true);});fails(function(){f.isBoxChecked(0);});fails(function(){f.defaultIsChecked(0,true);});";
  s.verify="var f=this.getField('dataCheck');eq(f.exportValues,[]);eq(f.value,'Off');fails(function(){return f.valueAsString;});";
  Workflow("WidgetlessCheckFieldsHaveNoExportStatesOrCheckableControlIndices",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessRadioFieldsHaveNoSelectedOrDefaultSelectedControl) {
  Spec s;
  s.body="var f=this.getField('dataRadio');eq(f.type,'radiobutton');eq(f.valueAsString,'Off');eq(f.exportValues,[]);fails(function(){f.isDefaultChecked(0);});fails(function(){return f.style;});";
  s.verify="eq(this.getField('dataRadio').valueAsString,'Off');eq(this.getField('dataRadio').exportValues,[]);";
  Workflow("WidgetlessRadioFieldsHaveNoSelectedOrDefaultSelectedControl",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessPushButtonLayoutPropertiesRequireAnActualControl) {
  Spec s;
  s.body="var f=this.getField('dataPush');eq(f.type,'button');['buttonAlignX','buttonAlignY','buttonFitBounds','buttonPosition','buttonScaleHow','buttonScaleWhen','highlight'].forEach(function(p){fails(function(){return f[p];});});fails(function(){f.buttonGetCaption();});fails(function(){f.buttonGetIcon();});";
  s.verify="eq(this.getField('dataPush').type,'button');";
  Workflow("WidgetlessPushButtonLayoutPropertiesRequireAnActualControl",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessSignatureRejectsTextDefaultAndButtonExportProperties) {
  Spec s;
  s.body="var f=this.getField('dataSignature');eq(f.type,'signature');eq(f.valueAsString,'');fails(function(){return f.defaultValue;});fails(function(){return f.exportValues;});";
  s.verify="eq(this.getField('dataSignature').type,'signature');eq(this.getField('dataSignature').valueAsString,'');";
  Workflow("WidgetlessSignatureRejectsTextDefaultAndButtonExportProperties",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessMultiSelectListsStoreExportValuesAndIndices) {
  Spec s;
  s.body="var f=this.getField('dataList');f.value=['two','three'];eq(f.value,['two','three']);eq(f.currentValueIndices,[1,2]);eq(f.valueAsString,'');";
  s.verify="var f=this.getField('dataList');eq(f.value,['two','three']);eq(f.currentValueIndices,[1,2]);";
  Workflow("WidgetlessMultiSelectListsStoreExportValuesAndIndices",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessListsResetTheirSelectedExportToTheAuthoredDefault) {
  Spec s;
  s.body="var f=this.getField('dataList');f.value=['one','three'];this.resetForm(['dataList']);eq(f.valueAsString,'two');";
  s.verify="eq(this.getField('dataList').valueAsString,'two');";
  Workflow("WidgetlessListsResetTheirSelectedExportToTheAuthoredDefault",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessSingleSelectListsUseOnlyTheFirstRequestedIndex) {
  Spec s;
  s.body="var f=this.getField('dataList');f.currentValueIndices=[1,2];eq(f.currentValueIndices,1);eq(f.valueAsString,'two');";
  s.verify="eq(this.getField('dataList').currentValueIndices,1);eq(this.getField('dataList').valueAsString,'two');";
  s.list_flags=0;
  Workflow("WidgetlessSingleSelectListsUseOnlyTheFirstRequestedIndex",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessComboValuesAndItemExportsRemainIndependentOfControls) {
  Spec s;
  s.body="var f=this.getField('dataCombo');eq(f.numItems,3);eq(f.getItemAt(1,false),'Two');eq(f.getItemAt(1,true),'two');f.value='three';eq(f.valueAsString,'three');";
  s.verify="eq(this.getField('dataCombo').valueAsString,'three');";
  Workflow("WidgetlessComboValuesAndItemExportsRemainIndependentOfControls",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessComboResetRetainsTheAuthoredDefaultExport) {
  Spec s;
  s.body="var f=this.getField('dataCombo');f.value='three';this.resetForm(['dataCombo']);eq(f.defaultValue,'two');eq(f.valueAsString,'two');";
  s.verify="eq(this.getField('dataCombo').valueAsString,'two');";
  Workflow("WidgetlessComboResetRetainsTheAuthoredDefaultExport",s);
}

TEST(CoverageWidgetlessFieldConstraints, IndexedControlAliasesPreserveTheUnderlyingValueWhenTheControlIsOutOfRange) {
  Spec s;
  s.body="var f=this.getField('visible..99');eq(f.name,'visible');eq(f.valueAsString,'Alpha');['textColor','fillColor','strokeColor','rotation','lineWidth','textFont','textSize'].forEach(function(p){fails(function(){return f[p];});});eq(this.getField('visible').valueAsString,'Alpha');";
  s.verify="eq(this.getField('dataText').valueAsString,'Alpha');";
  Workflow("IndexedControlAliasesPreserveTheUnderlyingValueWhenTheControlIsOutOfRange",s);
}

TEST(CoverageWidgetlessFieldConstraints, OutOfRangeIndexedAppearanceMutationsAreNoopsOnTheHealthyControl) {
  Spec s;
  s.body="var f=this.getField('visible..99');f.borderStyle='dashed';f.lineWidth=5;f.rect=[50,130,180,90];f.display=display.hidden;f.hidden=true;var healthy=this.getField('visible');eq(healthy.borderStyle,'solid');eq(healthy.lineWidth,1);eq(healthy.rect,[20,130,220,100]);eq(healthy.display,display.visible);";
  s.verify="var f=this.getField('visible');eq(f.borderStyle,'solid');eq(f.lineWidth,1);eq(f.rect,[20,130,220,100]);";
  Workflow("OutOfRangeIndexedAppearanceMutationsAreNoopsOnTheHealthyControl",s);
}

TEST(CoverageWidgetlessFieldConstraints, OutOfRangeIndexedPrintMutationRaisesTheSourceValueError) {
  Spec s;
  s.body="var f=this.getField('visible..99');fails(function(){f.print=false;});eq(this.getField('visible').print,true);";
  s.verify="eq(this.getField('visible').print,true);";
  Workflow("OutOfRangeIndexedPrintMutationRaisesTheSourceValueError",s);
}

TEST(CoverageWidgetlessFieldConstraints, IndexedValueMutationTargetsTheTerminalFieldRatherThanTheAbsentWidget) {
  Spec s;
  s.body="var f=this.getField('visible..99');f.value='Changed';eq(this.getField('visible').valueAsString,'Changed');";
  s.verify="eq(this.getField('dataText').valueAsString,'Alpha');";
  Workflow("IndexedValueMutationTargetsTheTerminalFieldRatherThanTheAbsentWidget",s);
}

TEST(CoverageWidgetlessFieldConstraints, ValidIndexedAppearanceMutationsUpdateExactlyTheResolvedControl) {
  Spec s;
  s.body="var f=this.getField('visible..0');f.lineWidth=2;f.borderStyle='dashed';eq(f.lineWidth,2);eq(f.borderStyle,'dashed');";
  s.verify="var f=this.getField('visible');eq(f.lineWidth,2);eq(f.borderStyle,'dashed');";
  Workflow("ValidIndexedAppearanceMutationsUpdateExactlyTheResolvedControl",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessIndexedCheckExportAccessRejectsTheAbsentControl) {
  Spec s;
  s.body="var f=this.getField('dataCheck..0');fails(function(){return f.exportValues;});f.exportValues=['Ignored'];eq(this.getField('dataCheck').exportValues,[]);";
  s.verify="eq(this.getField('dataCheck').exportValues,[]);";
  Workflow("WidgetlessIndexedCheckExportAccessRejectsTheAbsentControl",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessIndexedPrintMutationRejectsTheAbsentControl) {
  Spec s;
  s.body="var f=this.getField('dataText..0');fails(function(){f.print=true;});eq(f.valueAsString,'Alpha');";
  s.verify="eq(this.getField('dataText').valueAsString,'Alpha');";
  Workflow("WidgetlessIndexedPrintMutationRejectsTheAbsentControl",s);
}

TEST(CoverageWidgetlessFieldConstraints, WidgetlessFileSelectStoresTheRealHostBrowseResult) {
  Spec s;
  s.body="var f=this.getField('dataText');eq(f.fileSelect,true);f.browseForFileToSubmit();eq(f.valueAsString,'chosen.pdf');";
  s.verify="eq(this.getField('dataText').valueAsString,'chosen.pdf');";
  s.text_flags=1<<20;s.browse=true;
  Workflow("WidgetlessFileSelectStoresTheRealHostBrowseResult",s);
}
}  // namespace
}  // namespace pdfoxium::integration
