#include "pinned_test_fonts.h"
#include "form_fixture.h"

#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;

// Public constructors do not create AcroForm terminal fields or AA dictionaries.
// This fixture keeps text, numeric button exports, choice values and an
// intentionally widgetless field distinct, with a fixed calculation order.
struct ActionSpec {
  std::string document;
  std::string catalog;
  std::string calculate;
  std::string format;
  std::string check_caption = "4";
  std::string radio_caption = "l";
  std::string highlight = "I";
  std::string icon_fit;
  int alignment = 0;
  int position = 0;
  std::string expected_calc;
};

std::string ActionChecked(const std::string& body, bool set_ready) {
  return R"JS(
    function eq(a,b) {
      if(JSON.stringify(a)!==JSON.stringify(b))
        throw 'expected '+JSON.stringify(b)+' got '+JSON.stringify(a);
    }
    function yes(a) { if(!a) throw 'assertion'; }
    function fails(f) {
      var caught=false;try{f();}catch(e){caught=true;}
      if(!caught)throw 'expected exception';
    }
    try {
  )JS" + body + (set_ready ? "this.getField('input').value='Ready';" : "") + R"JS(
      this.getField('report').value='PASS';
    } catch(e) { this.getField('report').value='FAIL: '+String(e); }
  )JS";
}

std::vector<uint8_t> FieldActionsFixture(const ActionSpec& spec,
                                        const std::string& body,
                                        bool set_ready) {
  const std::string common = "/Type /Annot /Subtype /Widget /P 3 0 R /F 4 "
      "/BS << /W 1 /S /S >> /DA (/Helv 12 Tf 0 g) ";
  const auto field = [&](const std::string& entries) {
    return "<< " + common + entries + " >>";
  };
  std::vector<std::string> objects(7);
  objects[0] = "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R " + spec.catalog +
      " /Names << /JavaScript << /Names [(main) " +
      forms::Js(ActionChecked(body, set_ready)) + "] >> >> >>";
  objects[1] = "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>";
  objects[2] = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] "
      "/Resources << /Font << /Helv 5 0 R >> >> /Contents 7 0 R "
      "/Annots [8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R 14 0 R 15 0 R 16 0 R 17 0 R 18 0 R] >>";
  objects[3] = "<< /Fields [8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R 14 0 R "
      "15 0 R 16 0 R 17 0 R 18 0 R 19 0 R] /CO [12 0 R] /NeedAppearances true "
      "/DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>";
  objects[4] = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>";
  objects[5] = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] "
      "/Resources << >> /Contents 7 0 R >>";
  objects[6] = PdfStream("", "1 g 0 0 400 400 re f 0 0 1 rg 0 0 400 8 re f");
  objects.push_back(field("/FT /Tx /T (input) /Rect [20 340 180 370] /V () /Q " +
      std::to_string(spec.alignment) + " /MK << /BG [1 1 1] /BC [0 0 0] >> /AA << /F " +
      forms::Js(spec.format) + " >>"));
  objects.push_back(field("/FT /Tx /T (report) /Rect [220 340 380 370] /V () /MK << /BG [1 1 1] >>"));
  objects.push_back(field("/FT /Tx /T (A) /Rect [20 290 180 320] /V (10) /DV (5) /MK << /BG [1 1 1] >>"));
  objects.push_back(field("/FT /Tx /T (B) /Rect [220 290 380 320] /V (20) /DV (7) /MK << /BG [1 1 1] >>"));
  objects.push_back(field("/FT /Tx /T (calc) /Rect [20 240 180 270] /Ff 1 /V (0) "
      "/MK << /BG [1 1 1] >> /AA << /C " + forms::Js(spec.calculate) + " >>"));
  objects.push_back(field("/FT /Btn /T (push) /Ff 65536 /Rect [220 240 380 270] /H /" +
      spec.highlight + " /MK << /CA (Push) /TP " + std::to_string(spec.position) +
      " /BG [1 1 1] /BC [0 0 0] " + spec.icon_fit + " >>"));
  objects.push_back(field("/FT /Btn /T (check) /Rect [20 190 50 220] /V /Off /AS /Off "
      "/MK << /CA (" + spec.check_caption + ") >> /AP << /N << /Off 20 0 R /12 21 0 R >> >>"));
  objects.push_back(field("/FT /Btn /T (radio) /Ff 32768 /Rect [70 190 100 220] /V /Off /AS /Off "
      "/MK << /CA (" + spec.radio_caption + ") >> /AP << /N << /Off 20 0 R /34 22 0 R >> >>"));
  objects.push_back(field("/FT /Ch /T (list) /Ff 2097152 /Rect [220 130 380 220] "
      "/Opt [[(3) (Three)] [(5) (Five)] [() (LabelOnly)]] /V (3) /I [0] /MK << /BG [1 1 1] >>"));
  objects.push_back(field("/FT /Ch /T (combo) /Ff 393216 /Rect [20 140 180 164] "
      "/Opt [[(3) (Three)] [(5) (Five)]] /V (3) /I [0] /MK << /BG [1 1 1] >>"));
  objects.push_back(field("/FT /Sig /T (signature) /Rect [20 80 180 104]"));
  objects.push_back("<< /FT /Tx /T (detached) /V (7) /DV (9) >>");
  objects.push_back(PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 30 30]",
      "1 g 0 0 30 30 re f 0 G 1 w .5 .5 29 29 re S"));
  objects.push_back(PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 30 30]",
      "1 g 0 0 30 30 re f 0 G 1 w .5 .5 29 29 re S 0 g 8 8 14 14 re f"));
  objects.push_back(PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 30 30]",
      "1 g 0 0 30 30 re f 0 G 1 w .5 .5 29 29 re S 0 g 6 6 18 18 re f"));
  return PdfFixture(objects);
}

void CompareActions(const Raster& expected, const Raster& actual,
                    Artifacts& artifacts, const std::string& label) {
  CompareRaster(expected, actual, artifacts, label, RasterTolerance{24, 0, .4});
  for (const auto& region : std::array<std::array<int,4>,4>{{
      {20,30,160,30}, {20,130,160,30}, {20,180,80,30}, {220,180,160,90}}}) {
    CompareRaster(CropRaster(expected,region[0],region[1],region[2],region[3]),
                  CropRaster(actual,region[0],region[1],region[2],region[3]),
                  artifacts,label+"-region-"+std::to_string(region[1]),
                  RasterTolerance{24,0,.5});
  }
}

void FieldActionScenario(const ActionSpec& spec, const std::string& body,
                         const std::u16string& expected_input = u"Ready",
                         const std::function<void(forms::FormHost&)>& configure = {}) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  const auto bytes = FieldActionsFixture(spec,body,expected_input==u"Ready");
  std::array<Raster,2> originals;
  size_t backend=0;
  const auto execute = [&](Session& session) {
    session.api.FORM_DoDocumentJSAction(session.form);
    EXPECT_EQ(session.Field(1),u"PASS");
    EXPECT_EQ(session.Field(0),expected_input);
    if (!spec.expected_calc.empty()) {
      EXPECT_EQ(session.Field(4),std::u16string(spec.expected_calc.begin(),spec.expected_calc.end()));
    }
    session.api.FORM_ForceToKillFocus(session.form);
    EXPECT_TRUE(session.host->callbacks_valid);
    EXPECT_FALSE(session.host->callback_error);
  };
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    Session s(writer->api(),bytes,true,false,2,configure);
    ASSERT_TRUE(s.valid());
    ASSERT_NO_FATAL_FAILURE(execute(s));
    originals[backend]=s.Draw();
    // All pages contain the authored blue footer independently of scripts.
    const size_t footer=(size_t(396)*400+200)*4;
    EXPECT_EQ(originals[backend].pixels[footer],255);
    EXPECT_EQ(originals[backend].pixels[footer+1],0);
    EXPECT_EQ(originals[backend].pixels[footer+2],0);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-field-actions");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      Session reopened(reader->api(),saved,true,false,2,configure);
      ASSERT_TRUE(reopened.valid());
      EXPECT_EQ(reopened.Field(0),expected_input);
      EXPECT_EQ(reopened.Field(1),u"PASS");
      if (!spec.expected_calc.empty()) {
        EXPECT_EQ(reopened.Field(4),std::u16string(spec.expected_calc.begin(),spec.expected_calc.end()));
      }
      ASSERT_NO_FATAL_FAILURE(execute(reopened));
      CompareActions(originals[backend],reopened.Draw(),artifacts,
                     writer->name()+reader->name()+"-field-actions");
    }
    ++backend;
  }
  CompareActions(originals[0],originals[1],artifacts,"field-actions-engines");
}

TEST(CoverageScriptFieldActions, SignatureAndWidgetlessFieldsExposeTheirActualTypesAndValues) {
  FieldActionScenario({},R"JS(
    var signature=this.getField('signature'),detached=this.getField('detached');
    eq(signature.type,'signature');eq(signature.value,'');
    eq(detached.type,'text');eq(detached.value,7);eq(detached.valueAsString,'7');
    eq(detached.page,-1);eq(detached.getArray().length,1);
    eq(detached.getArray()[0].name,'detached');
  )JS");
}

TEST(CoverageScriptFieldActions, WidgetlessFieldGeometryAndFocusErrorsDoNotInvalidateOtherFields) {
  FieldActionScenario({},R"JS(
    var detached=this.getField('detached');
    fails(function(){return detached.rect;});
    fails(function(){return detached.textFont;});
    fails(function(){detached.setFocus();});
    eq(this.getField('A').value,10);eq(this.getField('signature').type,'signature');
  )JS");
}

TEST(CoverageScriptFieldActions, PushHighlightModesAreReadFromTheSpecificWidgetDictionary) {
  const std::array<std::pair<const char*,const char*>,5> cases = {{{"N","none"},{"I","invert"},
      {"O","outline"},{"P","push"},{"T","toggle"}}};
  for (const auto& item:cases) {
    SCOPED_TRACE(item.first);ActionSpec spec;spec.highlight=item.first;
    FieldActionScenario(spec,"eq(this.getField('push').highlight,'"+std::string(item.second)+"');");
  }
}

TEST(CoverageScriptFieldActions, CheckStylesDecodeCircleCrossDiamondSquareAndStarCaptions) {
  const std::array<std::pair<const char*,const char*>,6> cases = {{{"l","circle"},{"8","cross"},
      {"u","diamond"},{"n","square"},{"H","star"},{"","check"}}};
  for (const auto& item:cases) {
    SCOPED_TRACE(item.first);ActionSpec spec;spec.check_caption=item.first;
    FieldActionScenario(spec,"eq(this.getField('check').style,'"+std::string(item.second)+"');");
  }
}

TEST(CoverageScriptFieldActions, RadioMissingCaptionUsesItsCircleDefaultAndKeepsNumericExport) {
  ActionSpec spec;spec.radio_caption="";
  FieldActionScenario(spec,R"JS(
    var field=this.getField('radio');eq(field.style,'circle');
    field.checkThisBox(0,true);eq(field.value,34);eq(field.valueAsString,'34');
    eq(field.exportValues,['34']);
  )JS");
}

TEST(CoverageScriptFieldActions, TextAlignmentIncludesCenteredRightAndUnrecognizedDictionaryValues) {
  for (const auto& item:std::array<std::pair<int,const char*>,4>{{{0,"left"},{1,"center"},{2,"right"},{9,""}}}) {
    SCOPED_TRACE(item.first);ActionSpec spec;spec.alignment=item.first;
    FieldActionScenario(spec,"eq(this.getField('input').alignment,'"+std::string(item.second)+"');");
  }
}

TEST(CoverageScriptFieldActions, PushTextPositionsAndScaleWhenCoverAllDocumentedIconFitModes) {
  for (int mode=0;mode<4;++mode) {
    ActionSpec spec;spec.position=mode+2;
    const std::array<const char*,4> when={{"A","B","S","N"}};
    spec.icon_fit="/IF << /SW /"+std::string(when[mode])+" /S /A /FB true /A [1 0] >>";
    FieldActionScenario(spec,"var p=this.getField('push');eq(p.buttonPosition,"+std::to_string(mode+2)+
        ");eq(p.buttonScaleWhen,"+std::to_string(mode)+");eq(p.buttonScaleHow,true);eq(p.buttonFitBounds,true);"
        "eq(p.buttonAlignX,1);eq(p.buttonAlignY,0);");
  }
}

TEST(CoverageScriptFieldActions, MissingFieldRemovalAndIconParameterErrorsLeaveTheDocumentUsable) {
  FieldActionScenario({},R"JS(
    var count=this.numFields;
    eq(this.removeField('missing'),undefined);eq(this.numFields,count);
    fails(function(){getIcon();});fails(function(){getIcon('a','b');});
    fails(function(){removeField();});fails(function(){submitForm();});
    eq(this.getField('A').value,10);
  )JS");
}

void NumericCalculation(const std::string& formula,const std::string& setup,
                        const std::string& expected) {
  ActionSpec spec;spec.calculate=formula;spec.expected_calc=expected;
  FieldActionScenario(spec,"this.calculate=false;"+setup+
      "this.calculate=true;this.calculateNow();eq(String(this.getField('calc').value),'"+expected+"');");
}

TEST(CoverageScriptFieldActions, NumericCheckedCheckboxExportsParticipateInCalculation) {
  NumericCalculation("AFSimple_Calculate('SUM',['check','A'])",
      "this.getField('check').checkThisBox(0,true);this.getField('A').value=10;","22");
}

TEST(CoverageScriptFieldActions, NumericCheckedRadioExportsParticipateInCalculation) {
  NumericCalculation("AFSimple_Calculate('SUM',['radio','B'])",
      "this.getField('radio').checkThisBox(0,true);this.getField('B').value=20;","54");
}

TEST(CoverageScriptFieldActions, UncheckedButtonsContributeZeroToProductsAndSums) {
  NumericCalculation("AFSimple_Calculate('SUM',['check','radio','A'])",
      "this.getField('check').checkThisBox(0,false);this.getField('radio').checkThisBox(0,false);"
      "this.getField('A').value=10;","10");
}

TEST(CoverageScriptFieldActions, SingleSelectedNumericListExportsParticipateInCalculation) {
  NumericCalculation("AFSimple_Calculate('SUM',['list','A'])",
      "this.getField('list').currentValueIndices=[1];this.getField('A').value=10;","15");
}

TEST(CoverageScriptFieldActions, MultipleListSelectionsContributeZeroInsteadOfConcatenatedValues) {
  NumericCalculation("AFSimple_Calculate('SUM',['list','B'])",
      "this.getField('list').currentValueIndices=[0,1];this.getField('B').value=20;","20");
}

TEST(CoverageScriptFieldActions, EmptyListExportsAndSignatureFieldsContributeZero) {
  NumericCalculation("AFSimple_Calculate('SUM',['list','signature','A'])",
      "this.getField('list').currentValueIndices=[2];this.getField('A').value=10;","10");
}

TEST(CoverageScriptFieldActions, NumericComboAndTextExportsParticipateInAverageWithMissingNamesIgnored) {
  NumericCalculation("AFSimple_Calculate('AVG',['combo','A','missing'])",
      "this.getField('combo').currentValueIndices=1;this.getField('A').value=15;","10");
}

TEST(CoverageScriptFieldActions, CalculationArrayGettersAndNameCoercionRetainRealFieldIdentity) {
  NumericCalculation(R"JS(
    var a=['A','B'];
    Object.defineProperty(a,0,{get:function(){this[1]='combo';return 'A';}});
    AFSimple_Calculate('SUM',a);
  )JS", "this.getField('A').value=10;this.getField('combo').currentValueIndices=1;","15");
}

TEST(CoverageScriptFieldActions, ResetWithoutArgumentsRestoresAllDefaultsIncludingWidgetlessFields) {
  FieldActionScenario({},R"JS(
    this.calculate=false;
    this.getField('A').value=99;this.getField('B').value=88;this.getField('detached').value=77;
    this.resetForm();
    eq(this.getField('A').value,5);eq(this.getField('B').value,7);
    eq(this.getField('detached').value,9);this.calculate=true;
  )JS");
}

TEST(CoverageScriptFieldActions, PublicFormatHelpersRejectIncorrectParameterCountsAndRecover) {
  FieldActionScenario({},R"JS(
    [AFNumber_Format,AFNumber_Keystroke,AFPercent_Format,AFPercent_Keystroke,
     AFDate_FormatEx,AFDate_KeystrokeEx,AFDate_Format,AFDate_Keystroke,
     AFTime_Format,AFTime_Keystroke,AFTime_FormatEx,AFTime_KeystrokeEx,
     AFSpecial_Format,AFSpecial_Keystroke,AFSpecial_KeystrokeEx,
     AFMergeChange,AFRange_Validate,AFSimple_Calculate].forEach(function(f) {
      fails(function(){f();});
    });
    eq(AFSimple('SUM',2,3),5);
  )JS");
}

TEST(CoverageScriptFieldActions, ExternalEventRejectsValueAndUnknownFieldFullChannelsWithoutMutation) {
  FieldActionScenario({},R"JS(
    eq(event.type,'Doc');eq(event.name,'Open');
    fails(function(){return event.value;});
    fails(function(){event.value='x';});
    fails(function(){return event.fieldFull;});
    eq(this.getField('A').value,10);
  )JS");
}

TEST(CoverageScriptFieldActions, NumericAndPercentParameterValidationRunsInsideRealFormattingEvents) {
  ActionSpec spec;
  spec.format=R"JS(
    var caught=0;
    try{AFNumber_Format();}catch(e){caught++;}
    try{AFPercent_Format(-1,0);}catch(e){caught++;}
    try{AFPercent_Format(1,-1);}catch(e){caught++;}
    this._formatResult=String(caught);
  )JS";
  FieldActionScenario(spec,R"JS(
    this._formatResult='NOT_RUN';
    this.getField('input').value='0';this.getField('input').value='12';
    eq(this._formatResult,'3');
  )JS",u"12");
}

TEST(CoverageScriptFieldActions, UnknownSpecialStyleProducesAnEmptyFormattedAppearance) {
  ActionSpec spec;spec.format="AFSpecial_Format(99);this._formatResult=event.value;";
  FieldActionScenario(spec,R"JS(
    this._formatResult='NOT_RUN';
    this.getField('input').value='0';this.getField('input').value='12345';
    eq(this._formatResult,'');
  )JS",u"12345");
}

TEST(CoverageScriptFieldActions, DateFormatTokensCoverFullMonthSingleAmPmAndTwoDigitYear) {
  ActionSpec spec;
  spec.format="AFDate_FormatEx('mmmm d yy h:MM t');this._formatResult=event.value;";
  FieldActionScenario(spec,R"JS(
    this._formatResult='NOT_RUN';
    this.getField('input').value='0';this.getField('input').value='Sat Feb 29 15:06:00 GMT 2020';
    eq(this._formatResult,'February 29 20 3:06 p');
  )JS",u"Sat Feb 29 15:06:00 GMT 2020");
}

TEST(CoverageScriptFieldActions, DateFormatUnrecognizedTokensRemainLiteralAndMidnightKeepsItsZeroHour) {
  ActionSpec spec;
  spec.format="AFDate_FormatEx('yyyy-mm-dd hh:MM:ss tt Q yyy');this._formatResult=event.value;";
  FieldActionScenario(spec,R"JS(
    this._formatResult='NOT_RUN';
    this.getField('input').value='0';this.getField('input').value='Sat Feb 29 00:06:07 GMT 2020';
    eq(this._formatResult,'2020-02-29 00:06:07 am Q yyy');
  )JS",u"Sat Feb 29 00:06:07 GMT 2020");
}

TEST(CoverageScriptFieldActions, InvalidGmtGrammarUsesTheDocumentedEpochFallback) {
  ActionSpec spec;
  spec.format="AFDate_FormatEx('yyyy-mm-dd');this._formatResult=event.value;";
  FieldActionScenario(spec,R"JS(
    this._formatResult='NOT_RUN';
    this.getField('input').value='0';this.getField('input').value='invalid GMT';
    eq(this._formatResult,'1970-01-01');
  )JS",u"invalid GMT");
}

TEST(CoverageScriptFieldActions, DateKeystrokeEmptyAndIncompleteInputCanBeInspectedBeforeCommit) {
  FieldActionScenario({},R"JS(
    eq(AFDate_KeystrokeEx('yyyy-mm-dd'),undefined);
    eq(AFTime_KeystrokeEx('HH:MM'),undefined);
    fails(function(){AFSpecial_KeystrokeEx('');});
  )JS");
}

TEST(CoverageScriptFieldActions, GlobalRejectedPrimitiveValuesLeaveTheTrackedNamedValueUsable) {
  FieldActionScenario({},R"JS(
    var name='capiNinthValueKinds';
    try {
      global[name]=7;
      fails(function(){global['']=1;});
      fails(function(){global[name]=Symbol('bad');});
      fails(function(){global[name]=BigInt(9);});
      eq(global[name],7);
      global[name]=null;eq(global[name],null);
    } finally {delete global[name];}
  )JS");
}

TEST(CoverageScriptFieldActions, ReadOnlyStaticMethodAndInheritedColorReadsKeepTheirOriginalHolder) {
  FieldActionScenario({},R"JS(
    var old=global.setPersistent;
    global.setPersistent=undefined;eq(global.setPersistent,old);
    var inherited=Object.create(color), original=color.red;
    try {
      eq(inherited.red,original);
      color.red=['G',0.5];eq(color.red,['G',0.5]);eq(inherited.red,['G',0.5]);
    } finally {color.red=original;}
  )JS");
}

TEST(CoverageScriptFieldActions, TimerDefaultDelayAndClearTimeOutCancelTheRecordingHostTimer) {
  FieldActionScenario({},R"JS(
    fails(function(){app.setTimeOut();});fails(function(){app.setInterval('');});
    fails(function(){app.clearTimeOut();});fails(function(){app.clearInterval();});
    var timer=app.setTimeOut('this.getField("A").value=99;');
    app.clearTimeOut(timer);
    eq(this.getField('A').value,10);
  )JS");
}

TEST(CoverageScriptFieldActions, MissingResponseAndLanguageHostCallbacksUseTheirDocumentedFallbacks) {
  FieldActionScenario({},R"JS(
    eq(app.language,'ENU');fails(function(){app.response('question');});
    eq(app.beep(undefined),undefined);fails(function(){app.beep();});fails(function(){app.beep(1,2);});
    fails(function(){app.response();});fails(function(){app.alert();});
    eq(this.getField('A').value,10);
  )JS",u"Ready",[](forms::FormHost& host) {
    host.FFI_GetLanguage=nullptr;
    host.platform.app_response=nullptr;
  });
}

TEST(CoverageScriptFieldActions, SaveAndPrintAdditionalActionsExposeDocumentEventNamesWithoutFieldChannels) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  ActionSpec spec;
  // The API dispatches document events from the catalog AA, without invoking
  // a writer, printer or any external application in this test host.
  spec.catalog="/AA << /WS "+forms::Js("this.getField('report').value=event.type+'|'+event.name;")+
      " /DS "+forms::Js("this.getField('report').value=event.type+'|'+event.name;")+
      " /WP "+forms::Js("this.getField('report').value=event.type+'|'+event.name;")+
      " /DP "+forms::Js("this.getField('report').value=event.type+'|'+event.name;")+" >>";
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for (const auto* writer:Implementations()) {
    Session s(writer->api(),FieldActionsFixture(spec,"",true),true);ASSERT_TRUE(s.valid());
    s.api.FORM_DoDocumentJSAction(s.form);
    const std::array<int,4> events={{FPDFDOC_AACTION_WS,FPDFDOC_AACTION_DS,FPDFDOC_AACTION_WP,FPDFDOC_AACTION_DP}};
    const std::array<std::u16string,4> expected={{u"Doc|WillSave",u"Doc|DidSave",u"Doc|WillPrint",u"Doc|DidPrint"}};
    for (size_t i=0;i<events.size();++i) {
      s.api.FORM_DoDocumentAAction(s.form,events[i]);EXPECT_EQ(s.Field(1),expected[i]);
    }
    images[backend]=s.Draw();
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-document-events");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader:Implementations()) {
      Session reopened(reader->api(),saved,true);ASSERT_TRUE(reopened.valid());
      for (size_t i=0;i<events.size();++i) {
        reopened.api.FORM_DoDocumentAAction(reopened.form,events[i]);EXPECT_EQ(reopened.Field(1),expected[i]);
      }
      CompareActions(images[backend],reopened.Draw(),artifacts,writer->name()+reader->name()+"-document-events");
    }
    ++backend;
  }
  CompareActions(images[0],images[1],artifacts,"document-event-engines");
}

}  // namespace
}  // namespace pdfoxium::integration
