#include "pinned_test_fonts.h"
#include "form_fixture.h"

#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using Configure = std::function<void(forms::FormHost&)>;
using Region = std::array<int, 4>;

std::string ServiceChecked(const std::string& body) {
  return R"JS(
    function eq(a,b) {
      if (JSON.stringify(a)!==JSON.stringify(b))
        throw 'expected '+JSON.stringify(b)+' got '+JSON.stringify(a);
    }
    function yes(v) { if (!v) throw 'assertion'; }
    function near(a,b) { if (Math.abs(a-b)>0.00001) throw 'expected '+b+' got '+a; }
    function fails(f) {
      var caught=false; try { f(); } catch(e) { caught=true; }
      if (!caught) throw 'expected exception';
    }
    try {
  )JS" + body + R"JS(
      this.getField('input').value='Service ready';
      this.getField('second').value='PASS';
    } catch(e) { this.getField('second').value='FAIL: '+String(e); }
  )JS";
}

void CompareService(const Raster& expected, const Raster& actual,
                    Artifacts& artifacts, const std::string& label,
                    const std::vector<Region>& regions) {
  for (size_t i = 0; i < regions.size(); ++i) {
    const auto& r = regions[i];
    CompareRaster(CropRaster(expected, r[0], r[1], r[2], r[3]),
                  CropRaster(actual, r[0], r[1], r[2], r[3]), artifacts,
                  label + "-" + std::to_string(i), RasterTolerance{24, 0, .35});
  }
}

void ServiceScenario(const std::string& body, const Configure& configure = {},
                     const std::vector<Region>& regions = {{20, 30, 160, 30}},
                     const std::string& native_body = {}) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  forms::Actions actions;
  // Select the current reader through a real host service, including after a
  // different writer saved this action. Never infer the backend from the
  // result being checked or persist a writer-specific expectation in the PDF.
  actions.document = ServiceChecked(native_body.empty() ? body :
      "var serviceBackend=app.response('C API test backend');"
      "yes(serviceBackend==='native'||serviceBackend==='rust');"
      "if(serviceBackend==='native'){" + native_body + "}else{" + body + "}");
  const auto configure_for = [&](const Library& reader) {
    return Configure([&, name = reader.name()](forms::FormHost& host) {
      if (configure) configure(host);
      if (!native_body.empty()) host.response = name == "native" ? u"native" : u"rust";
    });
  };
  const auto fixture = forms::Fixture(actions);
  std::array<Raster, 2> originals;
  size_t backend = 0;
  const auto execute = [](Session& s) {
    s.api.FORM_DoDocumentJSAction(s.form);
    EXPECT_EQ(s.Field(1), u"PASS");
    EXPECT_EQ(s.Field(0), u"Service ready");
    EXPECT_TRUE(s.host->callbacks_valid);
    EXPECT_FALSE(s.host->callback_error);
    s.api.FORM_ForceToKillFocus(s.form);
  };
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    Session s(writer->api(), fixture, true, false, 2, configure_for(*writer));
    ASSERT_TRUE(s.valid());
    ASSERT_NO_FATAL_FAILURE(execute(s));
    originals[backend] = s.Draw();
    forms::ExpectInk(originals[backend], 20, 30, 180, 60);
    const auto bytes = SaveDocument(s.api, s.document, artifacts,
                                    writer->name() + "-service");
    ASSERT_FALSE(bytes.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE(reader->name());
      Session reopened(reader->api(), bytes, true, false, 2, configure_for(*reader));
      ASSERT_TRUE(reopened.valid());
      EXPECT_EQ(reopened.Field(0), u"Service ready");
      EXPECT_EQ(reopened.Field(1), u"PASS");
      ASSERT_NO_FATAL_FAILURE(execute(reopened));
      CompareService(originals[backend], reopened.Draw(), artifacts,
                     writer->name() + reader->name() + "-service", regions);
    }
    ++backend;
  }
  CompareService(originals[0], originals[1], artifacts, "service-engines", regions);
}

TEST(CoverageJavascriptServiceRecovery, SignedIntegerPaddingSeparatesSignPrecisionAndAlignment) {
  ServiceScenario(R"JS(
    [['% d',7,' 7'],['% d',-7,'-7'],['%+d',0,'+0'],
     ['%-6d',12,'12    '],['%-06d',12,'12    '],['%06d',-12,'-00012'],
     ['%.5d',12,'00012'],['%8.5d',-12,'  -00012']].forEach(function(r) {
      eq(util.printf(r[0],r[1]),r[2]);
    });
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, AlternateRadicesAndZeroPrecisionKeepTheirDocumentedPrefixes) {
  ServiceScenario(R"JS(
    [['%#o',9,'011'],['%#o',0,'0'],['%#x',0,'0'],['%#X',31,'0X1F'],
     ['%#08X',31,'0X00001F'],['%.0d',0,''],['%#.0o',0,'0'],
     ['%#.0x',0,''],['%.4x',15,'000f'],['%u',-2,'4294967294']].forEach(function(r) {
      eq(util.printf(r[0],r[1]),r[2]);
    });
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, FloatingFormatsExerciseSignificantDigitsExponentAndFractionTrimming) {
  ServiceScenario(R"JS(
    [['%.4g',1.23,'1.23'],['%.4g',123.4,'123.4'],['%.4g',0.0000123,'1.23e-05'],
     ['%.2E',12.5,'1.25E+01'],['%.0G',1000,'1E+03'],['% .1f',2.5,' 2.5'],
     ['%+.1f',2.5,'+2.5'],['%08.1f',-2.5,'-00002.5']].forEach(function(r) {
      eq(util.printf(r[0],r[1]),r[2]);
    });
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, AlternateFloatingFormatsPreserveTheRequestedDecimalPoint) {
  ServiceScenario(R"JS(
    eq(util.printf('%#.0f',12),'12.');
    eq(util.printf('%#.0e',12),'1.e+01');
    eq(util.printf('%#.4g',1.23),'1.230');
    eq(util.printf('%#.4G',10000),'1.000E+04');
    eq(util.printf('%-10.1f',2.5),'2.5       ');
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, StringPrintfStopsAtNulAndAppliesPrecisionBeforeWidth) {
  const std::string checks = R"JS(
    eq(util.printf('%s','abc\u0000tail'),'abc');
    eq(util.printf('%8.3s','abcdef'),'     abc');
    eq(util.printf('%-8.3s','abcdef'),'abc     ');
    eq(util.printf('%s','caf\u00e9 \u03a9'),unicodeExpected);
    eq(util.printf('%.0s','nonempty'),'');
    eq(util.printf('%c',65),'A');
  )JS";
  const std::string body = R"JS(var unicodeExpected='caf\u00e9 \u03a9';)JS" + checks;
  std::string native_body;
#if defined(__APPLE__)
  // CJS_Util::ParseDataType rewrites %s to nonstandard %S before libc
  // vswprintf. The pinned Mac oracle drops this segment containing Omega;
  // Rust still independently requires the entire Unicode string.
  native_body = "var unicodeExpected='';" + checks;
#endif
  ServiceScenario(body, {}, {{20, 30, 160, 30}}, native_body);
}

TEST(CoverageJavascriptServiceRecovery, InvalidPrintfSegmentsRemainLiteralAndDoNotBreakFollowingConversions) {
  ServiceScenario(R"JS(
    eq(util.printf('%q',9),'%q');
    eq(util.printf('trailing %',9),'trailing %');
    eq(util.printf('%d %s',7),'7 %s');
    eq(util.printf('%q %d',99,7),'%q 7');
    eq(util.printf('plain',1,2,3),'plain');
    eq(util.printf('%d',null),'0');
    eq(util.printf('%d',true),'1');
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, PrintfCoercionReentryUpdatesTheSameDocumentWithoutLosingLaterArguments) {
  ServiceScenario(R"JS(
    var doc=this, order=[];
    eq(util.printf('% 6d|%-5s',
      {valueOf:function(){order.push('number');doc.getField('combo').value='r';return 12;}},
      {toString:function(){order.push('string');return 'ok';}}),'    12|ok   ');
    eq(order,['number','string']);eq(this.getField('combo').value,'r');
  )JS", {}, {{20, 30, 160, 30}, {20, 130, 160, 30}});
}

TEST(CoverageJavascriptServiceRecovery, PrintxEscapesAndTrailingEscapePreserveLiteralMaskCharacters) {
  ServiceScenario(R"JS(
    eq(util.printx('\\A\\9\\X\\?',''),'A9X?');
    eq(util.printx('AA\\','ab'),'ab');
    eq(util.printx('\\<AA\\>','ab'),'<ab>');
    eq(util.printx('999-999','12'),'12-');
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, PrintxCaseModesApplyToSingleWildcardsAndRestOfInput) {
  ServiceScenario(R"JS(
    eq(util.printx('>*','aB 9!'),'AB 9!');
    eq(util.printx('<*','Ab 9!'),'ab 9!');
    eq(util.printx('>?=?<?','aBc'),'ABc');
    eq(util.printx('>AA=AA<AA','abcdef'),'ABcdef');
    eq(util.printx('*',''),'');
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, PrintxSkipsRejectedSourceCharactersWithoutConsumingMaskSlots) {
  ServiceScenario(R"JS(
    eq(util.printx('AAA-999','!a7b?C123'),'abC-123');
    eq(util.printx('999','a1 b2 c3'),'123');
    eq(util.printx('XXX','!?a-2 b'),'a2b');
    eq(util.printx('AA:99:??','!a!b!1!2!?*'),'ab:12:!?');
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, PrintxUnicodeWildcardsConsumeWholeScalarsAndRetainLiteralUnicode) {
  ServiceScenario(R"JS(
    eq(util.printx('??','\ud83d\ude00x'),'\ud83d\ude00x');
    eq(util.printx('\u03a9:*','caf\u00e9'),'\u03a9:caf\u00e9');
    eq(util.printx('>??','a\u03a9'),'A\u03a9');
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, PrintdShortMonthWeekdayAndTwoDigitYearUseTheSameLocalDate) {
  ServiceScenario(R"JS(
    var d=new Date(2020,1,29,7,6,5);
    eq(util.printd('ddd, dd mmm yy',d),'Sat, 29 Feb 20');
    eq(util.printd('dddd mmmm d yyyy',d),'Saturday February 29 2020');
    eq(util.printd('yyyy\u0000mm',d),'2020');
    eq(util.printd('%%%yyyy',d),'2020');
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, PrintdTwelveHourMidnightNoonAndEveningAgreeWithJavascriptFields) {
  const std::string checks = R"JS(
    [[0,'12 00 AM'],[12,'12 12 PM'],[23,'11 23 PM']].forEach(function(r,i) {
      var d=new Date(2020,0,1,r[0],6,7);
      eq(util.printd('hh HH TT',d),r[1]);
      eq(util.printd('hh HH TT tt',d),r[1]+' '+meridiems[i]);
      eq(util.printd('TT',d).toLowerCase(),i===0?'am':'pm');
      eq(util.printd('MM:ss',d),'06:07');
    });
    eq(util.printd('h:MM TT',new Date(2020,0,1,7,6,0)),' 7:06 AM');
  )JS";
  const std::string body = "var meridiems=['am','pm','pm'];" + checks;
  std::string native_body;
#if defined(__APPLE__)
  // Existing macOS oracle defect: tt maps to unsupported wcsftime %P,
  // which emits literal P. Keep exact Rust lowercase and native TT checks.
  native_body = "var meridiems=['P','P','P'];" + checks;
#endif
  ServiceScenario(body, {}, {{20, 30, 160, 30}}, native_body);
}

TEST(CoverageJavascriptServiceRecovery, PrintdRejectsInvalidModeTypeAndUnsupportedPictureWithoutPoisoningTheRuntime) {
  ServiceScenario(R"JS(
    var d=new Date(2020,0,1);
    fails(function(){util.printd();});
    fails(function(){util.printd(3,d);});
    fails(function(){util.printd({},d);});
    fails(function(){util.printd('yyyy',d,true);});
    fails(function(){util.printd('yyyy',new Date(NaN));});
    eq(util.printd('yyyy-mm-dd',d),'2020-01-01');
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, ScandFullMonthNamesAndCaseFoldingReturnIndependentCalendarComponents) {
  // The native parser inherits omitted clock fields from FX_GetDateTime and
  // returns a UTC epoch. Choose an explicit hour from the independent timezone
  // offset so both UTC and local interpretations retain the expected date,
  // including zones where noon UTC would already be the following day.
  ServiceScenario(R"JS(
    [['February 29, 2020',1,29],['january 7, 2020',0,7],['SEPTEMBER 12, 2020',8,12]].forEach(function(r) {
      var offset=new Date(2020,r[1],r[2],12).getTimezoneOffset();
      var hour=Math.max(0,Math.min(23,12+Math.floor(offset/60)));
      var d=util.scand('mmmm d, yyyy HH:MM:ss',r[0]+' '+('0'+hour).slice(-2)+':00:00');
      eq([d.getFullYear(),d.getMonth(),d.getDate()],[2020,r[1],r[2]]);
    });
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, ScandCenturyExpansionAndLeapYearDatesRetainParsedCalendarComponents) {
  ServiceScenario(R"JS(
    [['0044-03-15',2044,2,15],['0400-02-29',400,1,29],['2000-02-29',2000,1,29]].forEach(function(r) {
      var d=util.scand('yyyy-mm-dd',r[0]);
      eq([d.getFullYear(),d.getMonth(),d.getDate()],[r[1],r[2],r[3]]);
    });
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, ScandEmptyInputUsesTheExistingJavascriptClockAndRemainsLocallyCoherent) {
  const std::string checks = R"JS(
    var before=Date.now(), d=util.scand('yyyy-mm-dd',''), after=Date.now();
    // Native's host clock has whole-second granularity. Its FX_GetDateTime
    // constructs epoch milliseconds from local tm fields, shifting the instant
    // by the local UTC offset. Rust keeps the real JavaScript instant.
    var lower=before, upper=after;
    if (localFieldsAsUtc) {
      lower-=new Date(before).getTimezoneOffset()*60000;
      upper-=new Date(after).getTimezoneOffset()*60000;
    }
    yes(d instanceof Date);
    if (!(d.getTime()>=Math.min(lower,upper)-2000 &&
          d.getTime()<=Math.max(lower,upper)+2000))
      throw 'expected clock in ['+lower+','+upper+'] got '+d.getTime();
    eq(util.printd('yyyy-mm-dd',d),
       d.getFullYear()+'-'+('0'+(d.getMonth()+1)).slice(-2)+'-'+('0'+d.getDate()).slice(-2));
    fails(function(){util.scand('yyyy');});
  )JS";
  ServiceScenario("var localFieldsAsUtc=false;" + checks, {}, {{20, 30, 160, 30}},
                  "var localFieldsAsUtc=true;" + checks);
}

TEST(CoverageJavascriptServiceRecovery, ParseDateExUsesItsPublicFormatAndFallsBackAcrossNumericDateOrders) {
  ServiceScenario(R"JS(
    [['2020/02/29','yyyy/mm/dd'],['29-02-2020','bad'],['2020-02-29','bad'],
     ['02/29/2020','bad']].forEach(function(r) {
      var d=new Date(AFParseDateEx(r[0],r[1]));
      eq([d.getFullYear(),d.getMonth(),d.getDate()],[2020,1,29]);
    });
    fails(function(){AFParseDateEx();});
    fails(function(){AFParseDateEx('2020-02-29');});
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, EveryNamedColorPropertyAcceptsTypedArraysAndRestoresItsOriginalValue) {
  ServiceScenario(R"JS(
    ['black','blue','cyan','dkGray','gray','green','ltGray','magenta','red',
     'transparent','white','yellow'].forEach(function(name) {
      var original=color[name];
      color[name]=['G',0.25];eq(color[name],['G',0.25]);
      color[name]=['RGB',1,0.5,0];eq(color[name],['RGB',1,0.5,0]);
      color[name]=['CMYK',0,0.5,1,0];eq(color[name],['CMYK',0,0.5,1,0]);
      color[name]=['T'];eq(color[name],['T']);
      color[name]=original;eq(color[name],original);
    });
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, PartialAndUnknownColorArraysUseTheNativeTransparentAndZeroComponentDefaults) {
  ServiceScenario(R"JS(
    [[[],['T']],[['G'],['G',0]],[['RGB',0.5],['RGB',0.5,0,0]],
     [['CMYK',0.5],['CMYK',0.5,0,0,0]],[['unknown',1,2,3,4],['T']]].forEach(function(r) {
      var original=color.black;color.black=r[0];eq(color.black,r[1]);color.black=original;
    });
    fails(function(){color.black='black';});
    eq(color.convert(['T'],'G'),['T']);
    eq(color.convert(['RGB',1,0,0],'unknown'),['T']);
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, ColorSpaceConversionsExerciseGrayAndCmykTargetsWithKnownPrimaryColors) {
  ServiceScenario(R"JS(
    eq(color.convert(['RGB',1,0,0],'CMYK'),['CMYK',0,1,1,0]);
    eq(color.convert(['G',0.5],'CMYK'),['CMYK',0,0,0,0.5]);
    eq(color.convert(['CMYK',0,0,0,1],'G'),['G',0]);
    near(color.convert(['RGB',1,0,0],'G')[1],0.3);
    eq(color.convert(['RGB',1,1,1],'G'),['G',1]);
    fails(function(){color.convert([]);});
    fails(function(){color.equal([]);});
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, ColorEqualityConvertsToTheRicherSpaceWithoutMutatingItsInputs) {
  ServiceScenario(R"JS(
    var g=['G',0.5],rgb=['RGB',0.5,0.5,0.5],black=['CMYK',0,0,0,1];
    yes(color.equal(g,rgb));yes(color.equal(black,['G',0]));
    yes(!color.equal(['RGB',1,0,0],['RGB',0,1,0]));
    eq(g,['G',0.5]);eq(rgb,['RGB',0.5,0.5,0.5]);
    fails(function(){color.equal('black',black);});
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, GlobalOwnKeyEnumerationIncludesTheActualDynamicStringProperties) {
  ServiceScenario(R"JS(
    var prefix='capiNinthKeys_', names=['z','a','10','2'];
    try {
      names.forEach(function(n,i){global[prefix+n]=i;});
      var keys=Object.keys(global).filter(function(k){return k.indexOf(prefix)===0;});
      eq(keys,[prefix+'10',prefix+'2',prefix+'a',prefix+'z']);
      names.forEach(function(n,i){eq(global[prefix+n],i);});
    } finally { names.forEach(function(n){delete global[prefix+n];}); }
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, GlobalDataDescriptorsPreserveEnumerablePropertiesAndTheirValues) {
  ServiceScenario(R"JS(
    var name='capiNinthDescriptor';
    try {
      Object.defineProperty(global,name,{value:7,writable:true,enumerable:true,configurable:true});
      eq(global[name],7);yes(Object.getOwnPropertyNames(global).indexOf(name)>=0);
      yes(Object.keys(global).indexOf(name)>=0);
      global[name]=9;eq(global[name],9);
      Object.defineProperty(global,name,{value:11,enumerable:true});
      eq(global[name],11);yes(Object.keys(global).indexOf(name)>=0);
    } finally { delete global[name]; }
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, GlobalUndefinedAssignmentDeletesTrackedNamesAndAllowsFreshDescriptors) {
  ServiceScenario(R"JS(
    var name='capiNinthDelete';
    try {
      global[name]='old';yes(name in global);
      global[name]=undefined;eq(typeof global[name],'undefined');
      Object.defineProperty(global,name,{value:'new',writable:true,enumerable:true,configurable:true});
      eq(global[name],'new');
      yes(delete global[name]);eq(typeof global[name],'undefined');
    } finally { delete global[name]; }
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, CheckedButtonValuesAndTypesRemainIndependentAcrossCheckAndRadioFields) {
  ServiceScenario(R"JS(
    var check=this.getField('check'),radio=this.getField('radio'),list=this.getField('list');
    eq(check.type,'checkbox');eq(radio.type,'radiobutton');eq(list.type,'listbox');
    check.checkThisBox(0,false);radio.checkThisBox(0,false);
    check.checkThisBox(0,true);eq(check.value,'Yes');eq(check.valueAsString,'Yes');
    eq(radio.value,'Off');radio.checkThisBox(0,true);eq(radio.value,'Yes');
    eq(check.value,'Yes');yes(check.isBoxChecked(0));yes(radio.isBoxChecked(0));
  )JS", {}, {{20, 30, 160, 30}, {20, 80, 80, 30}});
}

TEST(CoverageJavascriptServiceRecovery, CheckAndRadioDefaultQueriesRejectInvalidIndicesWithoutChangingTheCheckedState) {
  ServiceScenario(R"JS(
    var b=this.getField('check');
    b.checkThisBox(0,false);
    eq(b.valueAsString,'Off');b.checkThisBox(0,true);
    [-2,-1,1,99].forEach(function(i) {
      fails(function(){b.checkThisBox(i,true);});
      fails(function(){b.isBoxChecked(i);});
      fails(function(){b.isDefaultChecked(i);});
      fails(function(){b.defaultIsChecked(i);});
    });
    fails(function(){b.checkThisBox();});fails(function(){b.defaultIsChecked();});
    eq(b.valueAsString,'Yes');yes(b.isBoxChecked(0));
  )JS", {}, {{20, 30, 160, 30}, {20, 80, 30, 30}});
}

TEST(CoverageJavascriptServiceRecovery, ListSingleSelectionReturnsItsLabelFallbackAndScalarValue) {
  ServiceScenario(R"JS(
    var list=this.getField('list');
    list.currentValueIndices=[2];eq(list.currentValueIndices,2);eq(list.value,'three');
    eq(list.valueAsString,'three');eq(list.type,'listbox');
    list.currentValueIndices=[];eq(list.value,'');
    list.currentValueIndices=[0];eq(list.value,'one');
    eq(list.getItemAt(99),'ten');
  )JS", {}, {{20, 30, 160, 30}, {220, 110, 160, 110}});
}

TEST(CoverageJavascriptServiceRecovery, FieldObjectTypeErrorsLeaveButtonAndTextFieldsUsable) {
  ServiceScenario(R"JS(
    var text=this.getField('input'),button=this.getField('push');
    fails(function(){return button.value;});
    fails(function(){return button.valueAsString;});
    fails(function(){text.checkThisBox(0,true);});
    eq(text.defaultIsChecked(0,true),false);
    fails(function(){text.buttonGetCaption(0);});
    fails(function(){button.buttonGetCaption(-1);});
    fails(function(){button.buttonGetCaption(3);});
    eq(button.buttonGetCaption(0),'Press');eq(text.name,'input');
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, DocumentWindowsPathsAndBareNamesUseTheConfiguredRecordingHost) {
  struct Row { std::string path, expected; };
  for (const auto& row : std::vector<Row>{{"C:\\folder\\file.pdf", "/C/folder/file.pdf"},
                                        {"plain.pdf", "/plain.pdf"},
                                        {"/folder/", "//folder/"}}) {
    SCOPED_TRACE(row.path);
    // JSON string literals quote backslashes without involving PDF syntax.
    const std::string literal = row.path == "C:\\folder\\file.pdf"
        ? "'C:\\\\folder\\\\file.pdf'" : row.path == "plain.pdf" ? "'plain.pdf'" : "'/folder/'";
    ServiceScenario("eq(this.URL," + literal + ");eq(this.path,'" + row.expected + "');" +
                    "eq(this.documentFileName," + std::string(row.path == "C:\\folder\\file.pdf"
                        ? "'file.pdf'" : "''") + ");",
                    [=](forms::FormHost& host) { host.filepath = row.path; });
  }
}

TEST(CoverageJavascriptServiceRecovery, DocumentPageAssignmentsClampNegativeAndOversizedIndicesToRealPages) {
  ServiceScenario(R"JS(
    this.pageNum=999;eq(this.pageNum,1);
    this.pageNum=-99;eq(this.pageNum,0);
    this.pageNum=1;eq(this.getField('later').page,[1]);
    this.pageNum=0;eq(this.pageNum,0);
    fails(function(){this.getNthFieldName();}.bind(this));
  )JS");
}

TEST(CoverageJavascriptServiceRecovery, MergeChangeUsesRealSelectionEndpointsAndCommitValues) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Row { const char* initial; int first, last; char16_t change; const char16_t* expected; };
  const std::vector<Row> rows = {{"ABCD", 0, 0, u'X', u"XABCD"},
                                 {"ABCD", 1, 3, u'X', u"AXD"},
                                 {"ABCD", 0, 4, u'X', u"X"},
                                 {"ABCD", 4, 4, u'X', u"ABCDX"}};
  Artifacts artifacts;
  for (const auto& row : rows) {
    SCOPED_TRACE(row.first);
    std::array<Raster, 2> images;
    size_t backend = 0;
    forms::Actions actions;
    actions.keystroke = "this.getField('second').value=(event.willCommit?'commit:':'key:')+AFMergeChange('unused');";
    for (const auto* writer : Implementations()) {
      SCOPED_TRACE(writer->name());
      Session s(writer->api(), forms::Fixture(actions, row.initial), true);
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.Focus(30,355));
      s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Home,0);
      for (int i=0; i<row.first; ++i) s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Right,0);
      for (int i=row.first; i<row.last; ++i)
        s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey);
      s.api.FORM_OnChar(s.form,s.page,row.change,0);
      EXPECT_EQ(s.Field(1), std::u16string(u"key:")+row.expected);
      EXPECT_EQ(s.Text(), row.expected);
      s.api.FORM_ForceToKillFocus(s.form);
      EXPECT_EQ(s.Field(1), std::u16string(u"commit:")+row.expected);
      EXPECT_EQ(s.Field(0), row.expected);
      images[backend] = s.Draw();
      const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-merge-change");
      ASSERT_FALSE(saved.empty());
      for (const auto* reader : Implementations()) {
        Session reopened(reader->api(),saved,true);ASSERT_TRUE(reopened.valid());
        EXPECT_EQ(reopened.Field(0),row.expected);
        CompareService(images[backend],reopened.Draw(),artifacts,writer->name()+reader->name()+"-merge",
                       {{20,30,160,30}});
      }
      ++backend;
    }
    CompareService(images[0],images[1],artifacts,"merge-engines",{{20,30,160,30}});
  }
}

void FormattedService(const std::string& formatter, const std::string& raw,
                      const std::u16string& expected) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  forms::Actions actions;
  // Force a real value transition after reload: equal-value assignments skip
  // the Format event while NeedAppearances can regenerate the raw appearance.
  actions.document = "this.getField('input').value='0';"
      "this.getField('input').value='" + raw + "';";
  actions.format = formatter + ";this.getField('second').value=event.value;";
  std::array<Raster, 2> images;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(writer->name());
    Session s(writer->api(),forms::Fixture(actions),true);ASSERT_TRUE(s.valid());
    s.api.FORM_DoDocumentJSAction(s.form);
    EXPECT_EQ(s.Field(1),expected);
    images[backend]=s.Draw();forms::ExpectInk(images[backend],20,30,180,60);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-formatted-service");
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      Session reopened(reader->api(),saved,true);ASSERT_TRUE(reopened.valid());
      reopened.api.FORM_DoDocumentJSAction(reopened.form);
      EXPECT_EQ(reopened.Field(1),expected);
      CompareService(images[backend],reopened.Draw(),artifacts,writer->name()+reader->name()+"-formatted",
                     {{20,30,160,30}});
    }
    ++backend;
  }
  CompareService(images[0],images[1],artifacts,"formatted-engines",{{20,30,160,30}});
}

TEST(CoverageJavascriptServiceRecovery, NegativeNumberStylesExerciseColorRequestsWithoutChangingTheSourceDefaultAppearance) {
  for (int style : {1,3}) {
    SCOPED_TRACE(style);
    FormattedService("AFNumber_Format(2,0,"+std::to_string(style)+",0,'$',true);"
                     "if(JSON.stringify(event.target.textColor)!=='[\"RGB\",0,0,0]')throw 'source DA changed'",
                     "-1234.5", style==1?u"$1,234.50":u"($1,234.50)");
  }
}

TEST(CoverageJavascriptServiceRecovery, PercentFormattingSupportsPrefixAndAnEmptyNumericValue) {
  FormattedService("AFPercent_Format(1,0,true)", "0.125", u"%12.5");
  FormattedService("AFPercent_Format(2,0,false)", "", u"0.00%");
}

TEST(CoverageJavascriptServiceRecovery, GmtDateFormattingUsesTheDocumentedEightPartDateGrammar) {
  FormattedService("AFDate_FormatEx('yyyy-mm-dd HH:MM:ss')",
                   "Thu Oct 25 00:46:40 GMT 2012", u"2012-10-25 00:46:40");
}

}  // namespace
}  // namespace pdfoxium::integration
