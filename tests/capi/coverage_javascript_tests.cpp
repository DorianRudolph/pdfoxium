#include "form_fixture.h"

#include <gtest/gtest.h>

#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Actions;
using forms::Fixture;
using forms::Session;

// Execute through the document action entry point, with failures reported in a
// real PDF field. Script exceptions must not masquerade as successful execution.
std::string CheckedScript(const std::string& body) {
  return "function eq(a,b){if(JSON.stringify(a)!==JSON.stringify(b))throw 'expected '+JSON.stringify(b)+' got '+JSON.stringify(a);}"
      "function yes(a){if(!a)throw 'assertion failed';}"
      "function throws(f,label){var caught=false;try{f();}catch(e){caught=true;}if(!caught)throw 'expected exception: '+(label||f.toString());}"
      "try{" + body + ";this.getField('second').value='PASS';}"
      "catch(e){this.getField('second').value='FAIL: '+String(e);}";
}

void CompareInput(const Raster& a, const Raster& b, Artifacts& artifacts,
                  const std::string& label) {
  // Same standard-font AA budget as the form scenarios; compare only the
  // changed widget, independently of the script's PASS/FAIL result field.
  CompareRaster(CropRaster(a,20,30,160,30),CropRaster(b,20,30,160,30),
                artifacts,label,RasterTolerance{18,0,0.25});
}

void CheckDocumentScript(const std::string& body, bool visual = false,
                         const std::string& native_body = {}) {
  Artifacts artifacts;
  std::array<Raster,2> images;
  size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Actions actions;
    actions.document=CheckedScript(library->name()=="native" && !native_body.empty()?native_body:body);
    Session session(library->api(),Fixture(actions),true);
    ASSERT_TRUE(session.valid());
    session.api.FORM_DoDocumentJSAction(session.form);
    EXPECT_EQ(session.Field(1),u"PASS");
    if(visual) {
      images[backend]=session.Draw();
      forms::ExpectInk(images[backend],20,30,180,60);
      const auto bytes=SaveDocument(session.api,session.document,artifacts,
                                    library->name()+"-script");
      ASSERT_FALSE(bytes.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());
        Session loaded(reader->api(),bytes,true);ASSERT_TRUE(loaded.valid());
        EXPECT_EQ(loaded.Field(0),session.Field(0));
        EXPECT_EQ(loaded.Field(1),u"PASS");
        CompareInput(images[backend],loaded.Draw(),artifacts,
                     library->name()+"-"+reader->name()+"-script-reload");
      }
    }
    ++backend;
  }
  if(visual) CompareInput(images[0],images[1],artifacts,"script-input");
}

TEST(CoverageJavaScript, PrintfMasksAndByteConversionsUseRealHostBindings) {
  CheckDocumentScript(R"JS(
    eq(util.printf('n=%+06d hex=%#x f=%.2f s=%5s',12,31,1.25,'ok'),
       'n=+00012 hex=0x1f f=1.25 s=   ok');
    eq(util.printf('%u %o %X %c %.3g %.2e',-1,9,31,65,1234,1.25),
       '4294967295 11 1F A 1.23e+03 1.25e+00');
    eq(util.printf('%d %s',7),'7 %s');
    eq(util.printf('literal %d',5),'literal 5');
    eq(util.printf('%s','caf\u00e9'),'caf\u00e9');
    eq(util.printx('999-99-9999','123456789'),'123-45-6789');
    eq(util.printx('>AAAA','abcd'),'ABCD');
    eq(util.printx('<AAAA','ABCD'),'abcd');
    eq(util.printx('XX-99','ab12'),'ab-12');
    eq([0,65,127,128,255].map(function(n){return util.byteToChar(n).charCodeAt(0);}),[0,65,127,128,255]);
    throws(function(){util.byteToChar(-1);});throws(function(){util.byteToChar(256);});
    throws(function(){util.printf();});throws(function(){util.printx('99');});
    var order=[];
    eq(util.printf('%d %s',{valueOf:function(){order.push('number');return 7;}},
                          {toString:function(){order.push('string');return 'ok';}}),'7 ok');
    eq(order,['number','string']);
  )JS");
}

TEST(CoverageJavaScript, PrintfCoercionExceptionsPreserveTheThrownValueInRust) {
  // Native's ReentrantToInt32Helper explicitly squashes V8 exceptions and
  // substitutes zero. Rust preserves the script exception and remains usable.
  const std::string coercion="var sentinel={},caught=false,result;"
      "try{result=util.printf('%d',{valueOf:function(){throw sentinel;}});}"
      "catch(e){caught=e===sentinel;}";
  CheckDocumentScript(coercion+"eq(caught,true);eq(typeof result,'undefined');",false,
                      coercion+"eq(caught,false);eq(result,'0');");
}

TEST(CoverageJavaScript, DateFormattingAndScanningRoundTripLocalJavaScriptDates) {
  const std::string date=R"JS(
    var d=new Date(2014,6,4,15,59,58);
    eq(util.printd('mmmm d, yyyy dddd HH:MM:ss TT',d),'July 4, 2014 Friday 15:59:58 PM');
  )JS";
  const std::string lowercase_meridiem=R"JS(
    eq(util.printd('mmmm d, yyyy dddd HH:MM:ss tt',d),'July 4, 2014 Friday 15:59:58 pm');
  )JS";
  const std::string roundtrip_and_errors=R"JS(
    eq([0,1,2].map(function(n){return util.printd(n,d);}),
       ['D:20140704155958','2014.07.04 15:59:58','2014/07/04 15:59:58']);
    var formats=['yyyy/mm/dd HH:MM:ss','mm/dd/yyyy','dd-mmm-yyyy','yyyymmdd'];
    var values=['2016/12/20 11:22:03','12/20/2016','20-Dec-2016','20161220'];
    for(var i=0;i<formats.length;++i){var parsed=util.scand(formats[i],values[i]);
      eq(parsed.getFullYear(),2016);eq(parsed.getMonth(),11);eq(parsed.getDate(),20);
      eq(util.printd('yyyy-mm-dd',parsed),'2016-12-20');}
    throws(function(){util.printd('mm',new Date(NaN));});
    throws(function(){util.printd('mm',0);});throws(function(){util.scand();});
  )JS";
  std::string native_body;
#if defined(__APPLE__)
  // Pinned native maps tt to libc's unsupported %P on macOS, producing P.
  // TT uses supported %p. Check that native result and normalize only its
  // meridiem for comparison; Rust must still format tt as lowercase pm.
  native_body=date+R"JS(
    eq(util.printd('TT',d).toLowerCase(),'pm');
  )JS"+roundtrip_and_errors;
#endif
  CheckDocumentScript(date+lowercase_meridiem+roundtrip_and_errors,false,native_body);
}

TEST(CoverageJavaScript, AcrobatNumericExtractionArithmeticAndColorConversions) {
  CheckDocumentScript(R"JS(
    eq(AFExtractNums('A12-B345,C6'),['12','345','6']);
    eq(AFExtractNums('.75'),['0','75']);eq(typeof AFExtractNums('words'),'undefined');
    eq(AFMakeNumber('12,5'),12.5);eq(AFMakeNumber('not numeric'),0);
    var ops=['SUM','PRD','AVG','MIN','MAX'];var results=[14,40,7,4,10];
    for(var i=0;i<ops.length;++i)eq(AFSimple(ops[i],10,4),results[i]);
    throws(function(){AFSimple('unknown',1,2);});throws(function(){AFSimple('SUM',NaN,2);});
    throws(function(){AFExtractNums();});throws(function(){AFMakeNumber();});
    eq(color.convert(['G',0.5],'RGB'),['RGB',0.5,0.5,0.5]);
    eq(color.convert(['CMYK',0,1,1,0],'RGB'),['RGB',1,0,0]);
    yes(color.equal(['G',0],['RGB',0,0,0]));yes(!color.equal(color.red,color.blue));
    eq(color.transparent,['T']);eq(color.yellow,['CMYK',0,0,1,0]);
    throws(function(){color.convert(1,'RGB');});
  )JS");
}

TEST(CoverageJavaScript, FieldValuesChoicesFlagsAndButtonAccessorsUseTypedFields) {
  CheckDocumentScript(R"JS(
    var t=this.getField('input'),c=this.getField('combo'),l=this.getField('list');
    eq(t.type,'text');eq(t.name,'input');eq(t.page,[0]);eq(t.charLimit,0);
    eq(t.value,'');t.value='00123';eq(t.value,123);eq(t.valueAsString,'00123');
    eq(t.readonly,false);eq(t.multiline,false);eq(t.password,false);eq(t.comb,false);
    eq(t.doNotScroll,false);eq(t.doNotSpellCheck,false);eq(t.fileSelect,false);eq(t.richText,false);
    eq(this.getField('readonly').readonly,true);eq(this.getField('multiline').multiline,true);
    eq(this.getField('password').password,true);eq(this.getField('comb').comb,true);
    eq(this.getField('comb').charLimit,6);eq(t.getArray().length,1);
    eq(c.type,'combobox');eq(c.editable,true);eq(c.numItems,3);
    eq(c.getItemAt(1,false),'Green');eq(c.getItemAt(1,true),'g');
    c.currentValueIndices=2;eq(c.currentValueIndices,2);eq(c.value,'b');
    eq(l.multipleSelection,true);l.currentValueIndices=[0,3,9];
    eq(l.currentValueIndices,[0,3,9]);eq(l.value,['one','four','ten']);
    var b=this.getField('check');eq(b.exportValues,['Yes']);eq(b.style,'check');
    b.checkThisBox(0,true);yes(b.isBoxChecked(0));b.checkThisBox(0,false);yes(!b.isBoxChecked(0));
    yes(b.defaultIsChecked(0));eq(b.isDefaultChecked(0),false);
    var p=this.getField('push');eq(p.type,'button');eq(p.buttonGetCaption(0),'Press');
    eq(p.buttonGetCaption(1),'Down');eq(p.buttonGetCaption(2),'Hover');
    eq(p.buttonAlignX,0);eq(p.buttonAlignY,0);eq(p.buttonFitBounds,false);
    eq(p.buttonPosition,0);eq(p.buttonScaleHow,false);eq(p.buttonScaleWhen,0);
    throws(function(){t.getItemAt(0);});eq(t.isBoxChecked(0),false);
    throws(function(){p.buttonGetCaption(7);});
  )JS",true);
}

TEST(CoverageJavaScript, FieldDelayQueuesFlushAppearanceAndVisibilityChanges) {
  CheckDocumentScript(R"JS(
    var t=this.getField('input');t.value='Delayed';
    t.delay=true;yes(t.delay);t.borderStyle='dashed';t.lineWidth=2;
    t.rect=[24,374,184,340];t.display=display.hidden;
    t.delay=false;yes(!t.delay);eq(t.borderStyle,'dashed');eq(t.lineWidth,2);
    eq(t.rect,[24,374,184,340]);eq(t.display,display.hidden);t.print=false;eq(t.print,false);
    t.display=display.visible;t.print=true;eq(t.display,display.visible);eq(t.print,true);
    this.delay=true;yes(this.delay);t.borderStyle='underline';t.lineWidth=1;
    this.delay=false;yes(!this.delay);eq(t.borderStyle,'underline');eq(t.lineWidth,1);
    t.hidden=true;yes(t.hidden);t.hidden=false;yes(!t.hidden);
    eq(t.strokeColor,['RGB',0,0,0]);eq(t.fillColor,['RGB',1,1,1]);eq(t.textSize,12);eq(t.rotation,0);
  )JS",true);
}

TEST(CoverageJavaScript, DocumentPropertiesResetAndFieldEnumerationHaveRealEffects) {
  CheckDocumentScript(R"JS(
    eq(this.numPages,2);eq(this.numFields,15);eq(this.getNthFieldName(0),'input');
    eq(typeof this.getField('does-not-exist'),'undefined');throws(function(){getField();});
    this.dirty=false;eq(this.dirty,false);this.dirty=true;eq(this.dirty,true);
    this.calculate=false;eq(this.calculate,false);this.calculate=true;eq(this.calculate,true);
    this.baseURL='https://example.invalid/base/';eq(this.baseURL,'https://example.invalid/base/');
    this.getField('input').value='changed';this.resetForm(['input']);eq(this.getField('input').value,'');
    this.getField('input').value='Ready';
    this.pageNum=1;eq(this.pageNum,1);this.pageNum=0;eq(this.pageNum,0);
    var names=[];for(var i=0;i<this.numFields;++i)names.push(this.getNthFieldName(i));
    yes(names.indexOf('calc')>=0);yes(names.indexOf('later')>=0);
    eq(typeof app.viewerVersion,'number');eq(typeof app.language,'string');
    app.calculate=false;eq(app.calculate,false);app.calculate=true;eq(app.calculate,true);
  )JS",true);
}

TEST(CoverageJavaScript, FormattingEventsCoverNumericPercentDateTimeAndSpecialMasks) {
  struct Case {const char* value;const char* action;std::u16string expected;};
  const std::vector<Case> cases={
    {"1234.5","AFNumber_Format(2,0,0,0,'',false)",u"1,234.50"},
    {"1234.5","AFNumber_Format(2,1,0,0,'$',true)",u"$1234.50"},
    {"1234,5","AFNumber_Format(2,2,0,0,'',false)",u"1.234,50"},
    {"1234,5","AFNumber_Format(2,3,0,0,' EUR',false)",u"1234,50 EUR"},
    {"-1234.5","AFNumber_Format(2,0,2,0,'',false)",u"(1,234.50)"},
    {"0.125","AFPercent_Format(1,0)",u"12.5%"},
    {"0.125","AFPercent_Format(2,2)",u"12,50%"},
    {"12/20/2016","AFDate_FormatEx('yyyy-mm-dd')",u"2016-12-20"},
    {"12/20/2016","AFDate_Format(10)",u"Dec 20, 2016"},
    {"23:45","AFTime_Format(0)",u"23:45"},
    {"23:45","AFTime_Format(1)",u"11:45 pm"},
    {"23:45:06","AFTime_FormatEx('HH:MM:ss')",u"23:45:06"},
    {"12345","AFSpecial_Format(0)",u"12345"},
    {"123456789","AFSpecial_Format(1)",u"12345-6789"},
    {"2125551234","AFSpecial_Format(2)",u"(212) 555-1234"},
    {"123456789","AFSpecial_Format(3)",u"123-45-6789"}};
  Artifacts artifacts;
  for(size_t i=0;i<cases.size();++i) {
    const auto& item=cases[i];SCOPED_TRACE(item.action);
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());
      Actions actions;actions.format=std::string(item.action)+";this.getField('second').value=event.value;";
      Session session(library->api(),Fixture(actions),true);ASSERT_TRUE(session.valid());
      ASSERT_TRUE(session.Focus(30,355));
      session.Replace(std::u16string(item.value,item.value+std::char_traits<char>::length(item.value)));
      ASSERT_TRUE(session.api.FORM_ForceToKillFocus(session.form));
      EXPECT_EQ(session.Field(1),item.expected);
      images[backend++]=session.Draw();
    }
    CompareInput(images[0],images[1],artifacts,"format-"+std::to_string(i));
  }
}

TEST(CoverageJavaScript, CalculationEventsAcceptArrayAndDelimitedFieldLists) {
  for(const auto& op:std::array<std::pair<std::string,std::u16string>,5>{{
      {"SUM",u"14"},{"PRD",u"40"},{"AVG",u"7"},{"MIN",u"4"},{"MAX",u"10"}}}) {
    SCOPED_TRACE(op.first);
    for(const std::string fields:{"['input','second']","'input, second'"}) {
      for(const auto* library:Implementations()) {
        SCOPED_TRACE(library->name());
        Actions actions;actions.document="this.getField('second').value=4;this.getField('input').value=10;";
        actions.calculate="AFSimple_Calculate('"+op.first+"',"+fields+");";
        Session session(library->api(),Fixture(actions),true);ASSERT_TRUE(session.valid());
        session.api.FORM_DoDocumentJSAction(session.form);
        EXPECT_EQ(session.Field(13),op.second);
      }
    }
  }
}

TEST(CoverageJavaScript, ReadOnlyAndUnsupportedBindingsRejectWithoutChangingFields) {
  CheckDocumentScript(R"JS(
    var t=this.getField('input'),p=this.getField('push'),c=this.getField('combo');
    var doc=this;
    ['numPages','numFields','info','URL','path','documentFileName','filesize','icons'].forEach(function(n){
      throws(function(){doc[n]='changed';},n);
    });
    ['name','valueAsString','numItems','type','defaultStyle','doc','page'].forEach(function(n){
      throws(function(){t[n]='changed';},n);
    });
    throws(function(){return t.defaultStyle;});
    ['getLock','setLock','signatureGetModifications','signatureGetSeedValue','signatureInfo',
     'signatureSetSeedValue','signatureSign','signatureValidate'].forEach(function(n){
      throws(function(){t[n]();},n);
    });
    ['buttonAlignX','buttonAlignY','buttonFitBounds','buttonPosition','buttonScaleHow',
     'buttonScaleWhen','highlight','style','multipleSelection','currentValueIndices'].forEach(function(n){
      throws(function(){return t[n];},n);
    });
    throws(function(){return p.required;});throws(function(){return p.valueAsString;});
    throws(function(){t.rect={};});throws(function(){t.rect=[1,2,3];});
    throws(function(){t.fillColor='red';});throws(function(){t.strokeColor=1;});
    throws(function(){t.textFont='';});eq(c.getItemAt(),'b');
    throws(function(){doc.getNthFieldName(-1);});throws(function(){doc.getNthFieldName(999);});
    eq(t.name,'input');eq(t.rect,[20,370,180,340]);eq(t.textSize,12);
    t.value='Still usable';eq(t.value,'Still usable');
  )JS",true);
}

TEST(CoverageJavaScript, SourcePermissionOnlySettersPreserveFlagsAndAppearance) {
  // These setters deliberately perform only permission checks in PDFium.
  // A throwing coercion object verifies they do not run conversion hooks.
  CheckDocumentScript(R"JS(
    var t=this.getField('input'),p=this.getField('push'),c=this.getField('combo');
    var poison={valueOf:function(){throw 'unexpected valueOf';},toString:function(){throw 'unexpected toString';}};
    ['alignment','charLimit','comb','defaultValue','doNotScroll','doNotSpellCheck','multiline',
     'password','rotation','richText','textSize','userName'].forEach(function(n){
      var old=t[n];t[n]=poison;eq(t[n],old);
    });
    ['buttonAlignX','buttonAlignY','buttonFitBounds','buttonPosition','buttonScaleHow',
     'buttonScaleWhen','highlight'].forEach(function(n){var old=p[n];p[n]=poison;eq(p[n],old);});
    var old=c.commitOnSelChange;c.commitOnSelChange=poison;eq(c.commitOnSelChange,old);
    old=c.editable;c.editable=poison;eq(c.editable,old);
    t.required=true;eq(t.required,false);t.fileSelect=true;eq(t.fileSelect,false);
    t.readonly=true;yes(t.readonly);t.readonly=false;yes(!t.readonly);
    t.value='Preserved';eq(t.value,'Preserved');
  )JS",true);
}

TEST(CoverageJavaScript, IconCollectionsKeepNamesDuplicatesAndObjectValidation) {
  CheckDocumentScript(R"JS(
    eq(this.icons,undefined);
    var p=this.getField('push'),icon=p.buttonGetIcon();eq(icon.name,'');
    eq(Object.create(icon).name,'');
    this.addIcon('one',icon);this.addIcon('two',icon);this.addIcon('one',icon);
    eq(this.icons.length,3);eq(this.icons.map(function(i){return i.name;}),['one','two','one']);
    eq(this.getIcon('one').name,'one');yes(this.getIcon('one')!==this.getIcon('one'));
    this.removeIcon('one');eq(this.icons.length,3);
    throws(function(){icon.name='changed';});
    throws(function(){addIcon('fake',{name:''});});throws(function(){addIcon('short');});
    throws(function(){getIcon('absent');});throws(function(){p.buttonGetIcon(3);});
    throws(function(){this.getField('input').buttonGetIcon();});
    eq(this.getField('input').value,'');
  )JS");
}

TEST(CoverageJavaScript, ReaderUiNoopsDoNotCoerceArgumentsOrMutateTheDocument) {
  CheckDocumentScript(R"JS(
    var doc=this,poison={toString:function(){throw 'unexpected conversion';}};
    var pages=this.numPages,fields=this.numFields;
    ['addAnnot','addField','addLink','closeDoc','createDataObject','deletePages',
     'exportAsText','exportAsFDF','exportAsXFDF','extractPages','getAnnot3D','getAnnots3D',
     'getLinks','getOCGs','getPageBox','getURL','importAnFDF','importAnXFDF','importTextData',
     'insertPages','replacePages','removeIcon','saveAs','syncAnnotScan'].forEach(function(n){
      eq(doc[n](poison),undefined);
    });
    ['ADBE','bookmarkRoot','Collab','layout','media','mouseX','mouseY','pageWindowRect','zoom','zoomType'].forEach(function(n){
      eq(doc[n],undefined);doc[n]=poison;eq(doc[n],undefined);
    });
    eq(this.external,true);this.external=poison;eq(this.external,true);eq(this.filesize,0);
    throws(function(){doc.getPrintParams();});throws(function(){doc.getPageNthWordQuads();});
    eq(this.numPages,pages);eq(this.numFields,fields);
    this.getField('input').value='Unchanged document';
  )JS",true);
}

}  // namespace
}  // namespace pdfoxium::integration
