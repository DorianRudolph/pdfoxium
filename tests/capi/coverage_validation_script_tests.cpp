#include "form_fixture.h"

#include <algorithm>
#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

std::string Narrow(const std::u16string& value) {
  // All event-table strings are deliberately ASCII; Unicode host bindings are
  // covered separately. Quote literals without executing a second parser.
  std::string result;
  for (char16_t c : value) result.push_back(static_cast<char>(c));
  return result;
}
std::string Quote(const std::string& value) {
  std::string result = "'";
  for (char c : value) {
    if (c == '\\' || c == '\'') result.push_back('\\');
    if (c == '\n') result += "\\n";
    else if (c == '\r') result += "\\r";
    else if (c == '\t') result += "\\t";
    else result.push_back(c);
  }
  return result + "'";
}
std::string Checked(const std::string& body) {
  return "function eq(a,b){if(JSON.stringify(a)!==JSON.stringify(b))throw 'expected '+JSON.stringify(b)+' got '+JSON.stringify(a);}"
      "function yes(v){if(!v)throw 'assertion';}"
      "function throws(f){var caught=false;try{f();}catch(e){caught=true;}if(!caught)throw 'expected exception '+f.toString();}"
      "try{" + body + ";this.getField('second').value='PASS';}"
      "catch(e){this.getField('second').value='FAIL: '+String(e);}";
}
Raster Input(forms::Session& s) {
  return CropRaster(s.Draw(), 20, 30, 160, 30);
}
void CompareInputs(const Raster& a, const Raster& b, Artifacts& artifacts,
                   const std::string& label) {
  CompareRaster(a, b, artifacts, label, RasterTolerance{18, 0, .25});
}
void Select(forms::Session& s, int first, int last) {
  ASSERT_TRUE(s.Focus(30, 355));
  ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form, s.page, FWL_VKEY_Home, 0));
  for (int i = 0; i < first; ++i)
    ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form, s.page, FWL_VKEY_Right, 0));
  for (int i = first; i < last; ++i)
    ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form, s.page, FWL_VKEY_Right, FWL_EVENTFLAG_ShiftKey));
}
void Recover(forms::Session& s, const std::u16string& value) {
  ASSERT_TRUE(s.Focus(30, 355));
  ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page));
  s.Replace(value);
  s.api.FORM_ForceToKillFocus(s.form);
  EXPECT_EQ(s.Field(0), value);
  EXPECT_TRUE(s.host->callbacks_valid);
  EXPECT_FALSE(s.host->callback_error);
}
void ReloadInput(const Library& writer, forms::Session& s,
                  const std::u16string& expected, Artifacts& artifacts,
                  const std::string& label) {
  const auto before = Input(s);
  const auto saved = SaveDocument(s.api, s.document, artifacts, writer.name() + "-" + label);
  ASSERT_FALSE(saved.empty());
  for (const auto* reader : Implementations()) {
    SCOPED_TRACE(reader->name());
    forms::Session reopened(reader->api(), saved, true);
    ASSERT_TRUE(reopened.valid());
    EXPECT_EQ(reopened.Field(0), expected);
    CompareInputs(before, Input(reopened), artifacts,
                  writer.name() + "-" + reader->name() + "-" + label);
  }
}

struct KeyCase {
  std::u16string initial;
  int first, last;
  char16_t character;
  bool accepted;
  std::u16string change;
  std::u16string merged;
  int style = 0;
};

TEST(CoverageValidationScripts, NumberAndPercentKeystrokesCheckSignsSeparatorsAndSelectedReplacement) {
  // These expected merges describe editor input, independently of AF helpers.
  const std::vector<KeyCase> cases = {
      {u"",0,0,u'1',true,u"1",u"1"},
      {u"12",2,2,u'3',true,u"3",u"123"},
      {u"12",2,2,u'.',true,u".",u"12."},
      {u"12.3",4,4,u'.',false,u".",u"12.3"},
      {u"12",2,2,u'x',false,u"x",u"12"},
      {u"",0,0,u'-',true,u"-",u"-"},
      {u"12",2,2,u'-',false,u"-",u"12"},
      {u"-12",3,3,u'-',false,u"-",u"-12"},
      {u"12",0,0,u'-',true,u"-",u"-12"},
      {u"1",1,1,u'+',false,u"+",u"1"},
      {u"12",2,2,u',',true,u",",u"12,",2},
      {u"12,3",4,4,u',',false,u",",u"12,3",2},
      {u"12",2,2,u'.',false,u".",u"12",2},
      {u"12",2,2,u'4',true,u"4",u"124",3},
      {u"12",2,2,u'.',true,u".",u"12.",99},
      {u"-12",0,3,u'3',true,u"3",u"3"},
      {u"-12",1,3,u'7',true,u"7",u"-7"},
      {u"1234",1,3,u'9',true,u"9",u"194"}};
  Artifacts artifacts;
  for (const std::string helper : {"AFNumber_Keystroke", "AFPercent_Keystroke"}) {
    for (size_t row = 0; row < cases.size(); ++row) {
      SCOPED_TRACE(helper + " row=" + std::to_string(row));
      const auto& item = cases[row];
      std::array<Raster,2> recovered;
      size_t backend = 0;
      for (const auto* library : Implementations()) {
        SCOPED_TRACE(library->name());
        forms::Actions actions;
        actions.keystroke = "if(!event.willCommit){" + helper + "(2," + std::to_string(item.style) +
            ");this.getField('second').value=(event.rc?'accept':'reject')+'|'+event.change+'|'+event.value;}";
        forms::Session s(library->api(), forms::Fixture(actions, Narrow(item.initial)), true);
        ASSERT_TRUE(s.valid());
        Select(s, item.first, item.last);
        s.api.FORM_OnChar(s.form,s.page,item.character,0);
        const auto report = std::u16string(item.accepted?u"accept|":u"reject|") + item.change + u"|" + item.merged;
        EXPECT_EQ(s.Field(1),report);
        // Pinned native's OnAAction ignores the returned live key change/rc.
        auto native = item.initial;
        native.replace(item.first, item.last-item.first, 1, item.character);
        EXPECT_EQ(s.Text(),library->name()=="native"?native:item.merged);
        Recover(s,u"12");recovered[backend++]=Input(s);
        forms::ExpectInk(s.Draw(),20,30,180,60);
        if (row == cases.size()-1) ReloadInput(*library,s,u"12",artifacts,helper+"-recovery");
      }
      CompareInputs(recovered[0],recovered[1],artifacts,helper+"-row"+std::to_string(row));
    }
  }
}

TEST(CoverageValidationScripts, SpecialMasksRewriteLiteralSlotsRejectBadCharactersAndRecover) {
  struct MaskCase {const char* mask;KeyCase key;};
  const std::vector<MaskCase> cases = {
      {"99999",{u"12",2,2,u'3',true,u"3",u"123"}},
      {"99999",{u"12",2,2,u'a',false,u"a",u"12"}},
      {"AA-9",{u"",0,0,u'b',true,u"b",u"b"}},
      {"AA-9",{u"A",1,1,u'7',false,u"7",u"A"}},
      {"AA-9",{u"AB",2,2,u'5',true,u"-",u"AB-"}},
      {"AA-9",{u"AB-",3,3,u'5',true,u"5",u"AB-5"}},
      {"OOXX",{u"",0,0,u'7',true,u"7",u"7"}},
      {"OOXX",{u"A",1,1,u'?',false,u"?",u"A"}},
      {"OOXX",{u"A7",2,2,u'?',true,u"?",u"A7?"}},
      {"OOXX",{u"A7?",3,3,u'!',true,u"!",u"A7?!"}},
      {"999",{u"123",3,3,u'4',false,u"4",u"123"}},
      {"999",{u"123",0,3,u'4',true,u"4",u"4"}},
      {"",{u"a",1,1,u'?',true,u"?",u"a?"}}};
  Artifacts artifacts;
  for (size_t row=0;row<cases.size();++row) {
    SCOPED_TRACE(row);const auto& item=cases[row];
    std::array<Raster,2> images;size_t backend=0;
    for (const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());
      forms::Actions actions;
      actions.keystroke="if(!event.willCommit){AFSpecial_KeystrokeEx("+Quote(item.mask)+
          ");this.getField('second').value=(event.rc?'accept':'reject')+'|'+event.change;}";
      forms::Session s(library->api(),forms::Fixture(actions,Narrow(item.key.initial)),true);
      ASSERT_TRUE(s.valid());Select(s,item.key.first,item.key.last);
      s.api.FORM_OnChar(s.form,s.page,item.key.character,0);
      EXPECT_EQ(s.Field(1),std::u16string(item.key.accepted?u"accept|":u"reject|")+item.key.change);
      auto native=item.key.initial;
      native.replace(item.key.first,item.key.last-item.key.first,1,item.key.character);
      EXPECT_EQ(s.Text(),library->name()=="native"?native:item.key.merged);
      if (!item.key.accepted && row==10) { EXPECT_FALSE(s.host->alerts.empty()); }
      Recover(s,u"123");images[backend++]=Input(s);
      if (row==cases.size()-1) ReloadInput(*library,s,u"123",artifacts,"special-mask-recovery");
    }
    CompareInputs(images[0],images[1],artifacts,"special-mask-"+std::to_string(row));
  }
}

TEST(CoverageValidationScripts, CommitValidationRejectsIncompleteMasksDatesTimesAndNumbersThenRecovers) {
  struct Case {const char* helper;const char* valid;const char* invalid;bool numeric;};
  const std::vector<Case> cases = {
      {"AFDate_KeystrokeEx('yyyy-mm-dd')","2024-02-29","not a date",false},
      {"AFDate_KeystrokeEx('mm/dd/yyyy')","12/20/2016","words",false},
      {"AFDate_Keystroke(7)","24-02-29","bad date",false},
      {"AFDate_Keystroke(10)","Feb 29, 2024","bad date",false},
      {"AFTime_KeystrokeEx('HH:MM')","23:45","not a time",false},
      {"AFTime_Keystroke(0)","23:45","bad time",false},
      {"AFTime_Keystroke(1)","11:45 pm","bad time",false},
      {"AFTime_Keystroke(2)","23:45:06","bad time",false},
      {"AFTime_Keystroke(3)","11:45:06 pm","bad time",false},
      {"AFNumber_Keystroke(2,0)","12.5","bad",true},
      {"AFNumber_Keystroke(2,2)","12,5","1.2.3",true},
      {"AFPercent_Keystroke(1,0)","0.25","--12",true},
      {"AFSpecial_KeystrokeEx('999')","123","12",false},
      {"AFSpecial_KeystrokeEx('AA-9')","AB-5","AB-55",false},
      {"AFSpecial_KeystrokeEx('AA-9')","AB-5","12-5",false},
      {"AFSpecial_Keystroke(0)","12345","1234",false},
      {"AFSpecial_Keystroke(1)","123456789","123",false},
      {"AFSpecial_Keystroke(2)","5551234","12",false},
      {"AFSpecial_Keystroke(3)","123456789","123",false}};
  Artifacts artifacts;
  for(size_t row=0;row<cases.size();++row) {
    SCOPED_TRACE(cases[row].helper);std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());forms::Actions actions;
      actions.keystroke="if(event.willCommit){var threw=false;try{"+std::string(cases[row].helper)+
          ";}catch(e){threw=true;if(event.rc)throw e;}this.getField('second').value=(event.rc?'accept':'reject')+(threw?'|threw':'');}";
      forms::Session s(library->api(),forms::Fixture(actions,"7"),true);ASSERT_TRUE(s.valid());
      ASSERT_TRUE(s.Focus(30,355));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
      s.Replace(std::u16string(cases[row].invalid,cases[row].invalid+std::char_traits<char>::length(cases[row].invalid)));
      s.api.FORM_ForceToKillFocus(s.form);
      EXPECT_EQ(s.Field(1),cases[row].numeric?u"reject|threw":u"reject");EXPECT_EQ(s.Field(0),u"7");
      ASSERT_FALSE(s.host->alerts.empty());
      EXPECT_FALSE(std::get<0>(s.host->alerts.back()).empty());
      const auto good=std::u16string(cases[row].valid,cases[row].valid+std::char_traits<char>::length(cases[row].valid));
      Recover(s,good);EXPECT_EQ(s.Field(1),u"accept");images[backend++]=Input(s);
      forms::ExpectInk(s.Draw(),20,30,180,60);
      if(row==0 || row==cases.size()-1) ReloadInput(*library,s,good,artifacts,"commit-recovery-"+std::to_string(row));
    }
    CompareInputs(images[0],images[1],artifacts,"commit-recovery-"+std::to_string(row));
  }
}

TEST(CoverageValidationScripts, RangeValidationUsesInclusiveBoundsAndAlertsWithoutLosingRecovery) {
  struct Case {bool lower,upper;double lo,hi;const char* candidate;bool accepted;};
  const std::vector<Case> cases={
      {true,true,0,10,"0",true},{true,true,0,10,"10",true},
      {true,true,0,10,"-1",false},{true,true,0,10,"11",false},
      {true,false,0,10,"-1",false},{true,false,0,10,"100",true},
      {false,true,0,10,"11",false},{false,true,0,10,"-100",true},
      {false,false,0,10,"100",true},{true,true,0,10,"",true},
      {true,true,0,10," 20",false},{true,true,0,10,"5.5",true}};
  Artifacts artifacts;
  for(size_t row=0;row<cases.size();++row) {
    SCOPED_TRACE(row);const auto& item=cases[row];std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());forms::Actions actions;
      actions.validate="AFRange_Validate("+std::string(item.lower?"true":"false")+","+std::to_string(item.lo)+","+
          (item.upper?"true":"false")+","+std::to_string(item.hi)+
          ");this.getField('second').value=event.rc?'accept':'reject';";
      forms::Session s(library->api(),forms::Fixture(actions,"5"),true);ASSERT_TRUE(s.valid());
      ASSERT_TRUE(s.Focus(30,355));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
      const auto candidate=std::u16string(item.candidate,item.candidate+std::char_traits<char>::length(item.candidate));
      s.Replace(candidate);s.api.FORM_ForceToKillFocus(s.form);
      EXPECT_EQ(s.Field(1),item.accepted?u"accept":u"reject");
      EXPECT_EQ(s.Field(0),item.accepted?candidate:u"5");
      EXPECT_EQ(s.host->alerts.empty(),item.accepted);
      if(!item.accepted) { EXPECT_FALSE(std::get<0>(s.host->alerts.back()).empty()); }
      Recover(s,u"7");EXPECT_EQ(s.Field(1),u"accept");images[backend++]=Input(s);
      if(row==3) ReloadInput(*library,s,u"7",artifacts,"range-recovery");
    }
    CompareInputs(images[0],images[1],artifacts,"range-"+std::to_string(row));
  }
}

std::vector<uint8_t> MetadataFixture(const std::string& script) {
  // No public API constructs hierarchical field dictionaries or Info values.
  // The scripts and all operations after this minimal envelope use the C API.
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /Names << /JavaScript << /Names [(main) "+forms::Js(Checked(script))+"] >> >> >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Contents 6 0 R /Resources << /Font << /Helv 5 0 R >> >> /Annots [7 0 R 8 0 R 9 0 R 10 0 R] >>",
      "<< /Fields [7 0 R 8 0 R 11 0 R] /CO [9 0 R] /NeedAppearances true /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>",
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
      PdfStream("","0.94 g 0 0 400 400 re f"),
      "<< /Type /Annot /Subtype /Widget /FT /Tx /T (input) /P 3 0 R /Rect [20 340 180 370] /F 4 /V (seed) /DV (Default) >>",
      "<< /Type /Annot /Subtype /Widget /FT /Tx /T (second) /P 3 0 R /Rect [220 340 380 370] /F 4 /V () >>",
      "<< /Type /Annot /Subtype /Widget /FT /Tx /T (calc) /Parent 11 0 R /P 3 0 R /Rect [20 180 180 210] /F 4 /Ff 1 /V (0) /DV (0) /AA << /C "+
          forms::Js("event.value=Number(this.getField('input').value)*2;")+" >> >>",
      "<< /Type /Annot /Subtype /Widget /FT /Tx /T (other) /Parent 11 0 R /P 3 0 R /Rect [220 180 380 210] /F 4 /V (Old) /DV (DefaultOther) >>",
      "<< /T (group) /Kids [9 0 R 10 0 R] >>",
      "<< /Title (Metadata) /Author (Author) /Subject (Subject) /Keywords (a,b) /Creator (Creator) /Producer (Producer) /CreationDate (D:20240229112233Z) /ModDate (D:20240301112233Z) /Trapped /False /CustomName /Custom /CustomNumber 17.25 /CustomBool true /IgnoredArray [1 2] >>"},
      "/Info 12 0 R");
}

TEST(CoverageValidationScripts, DocumentMetadataHierarchyResetAndCalculationHaveObservableEffects) {
  const auto fixture=MetadataFixture(R"JS(
    eq(author,'Author');eq(title,'Metadata');eq(subject,'Subject');eq(keywords,'a,b');
    eq(creator,'Creator');eq(producer,'Producer');eq(creationDate,'D:20240229112233Z');eq(modDate,'D:20240301112233Z');
    var details=info;eq(details.Author,'Author');eq(details.CustomName,'Custom');
    eq(details.CustomNumber,17.25);eq(details.CustomBool,true);eq(typeof details.IgnoredArray,'undefined');
    var poison={toString:function(){throw 'metadata setters must not coerce';}};
    author=poison;title=poison;subject=poison;keywords=poison;creator=poison;producer=poison;creationDate=poison;modDate=poison;
    eq(author,'Author');eq(title,'Metadata');details.Title='Local snapshot';eq(title,'Metadata');
    eq(URL,'/virtual/form.pdf');eq(documentFileName,'form.pdf');eq(path,'//virtual/form.pdf');
    var names=this.getField('group').getArray().map(function(f){return f.name;});
    eq(names,['group.calc','group.other']);yes(this.getField('group.other').doc===this);
    this.calculate=false;this.getField('input').value='changed';this.getField('group.other').value='ChangedOther';
    this.resetForm('input');eq(this.getField('input').value,'Default');eq(this.getField('group.other').value,'ChangedOther');
    this.resetForm([{toString:function(){this.called=true;return 'group.other';}}]);
    eq(this.getField('group.other').value,'DefaultOther');
    this.dirty=false;this.resetForm(['absent']);eq(this.dirty,false);
    this.getField('input').value='9';this.calculate=true;this.calculateNow();eq(this.getField('group.calc').value,18);
    this.removeField('input');eq(this.getField('input').name,'input');eq(this.numFields,4);
    throws(function(){removeField();});throws(function(){gotoNamedDest();});throws(function(){gotoNamedDest('absent');});
    this.getField('input').value='Metadata ready';
  )JS");
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());forms::Session s(writer->api(),fixture,true);ASSERT_TRUE(s.valid());
    s.api.FORM_DoDocumentJSAction(s.form);EXPECT_EQ(s.Field(1),u"PASS");EXPECT_EQ(s.Field(0),u"Metadata ready");
    EXPECT_GT(s.host->invalidations,0);EXPECT_GT(s.host->file_queries,0);images[backend++]=Input(s);
    ReloadInput(*writer,s,u"Metadata ready",artifacts,"metadata-hierarchy");
  }
  CompareInputs(images[0],images[1],artifacts,"metadata-hierarchy-backends");
}

TEST(CoverageValidationScripts, TypedFieldChoiceCoercionsColorValidationAndQueueIsolationRemainConnected) {
  forms::Actions actions;
  actions.document=Checked(R"JS(
    var t=this.getField('input'),c=this.getField('combo'),l=this.getField('list'),b=this.getField('check'),p=this.getField('push');
    c.currentValueIndices=[2,0];eq(c.currentValueIndices,2);eq(c.value,'b');
    l.currentValueIndices=[{valueOf:function(){return 0;}},3,3,99,-1];eq(l.currentValueIndices,[0,3]);
    eq(l.value,['one','four']);eq(l.valueAsString,'');
    c.currentValueIndices=-1;eq(c.currentValueIndices,-1);
    l.currentValueIndices=[];eq(l.currentValueIndices,-1);
    eq(c.getItemAt(-1,false),'Blue');eq(c.getItemAt(99,false),'Blue');eq(c.getItemAt(0,true),'r');
    c.insertItemAt({toString:function(){throw 'unused';}});c.deleteItemAt(99);c.setItems([]);c.clearItems();eq(c.numItems,3);
    var exports=b.exportValues;b.exportValues=['Changed'];eq(b.exportValues,exports);
    throws(function(){b.exportValues=1;});throws(function(){t.exportValues=[];});
    t.fillColor=['RGB',1,0,0];t.strokeColor=['G',0.5];t.textColor=['RGB',0,1,0];
    eq(t.fillColor,['RGB',1,1,1]);eq(t.strokeColor,['RGB',0,0,0]);eq(t.textColor,['RGB',0,0,0]);
    throws(function(){t.textColor=1;});throws(function(){p.buttonSetCaption('Changed');});
    throws(function(){p.buttonSetIcon(p.buttonGetIcon());});eq(p.buttonGetCaption(0),'Press');
    this.delay=true;t.delay=true;t.value='First';t.lineWidth=3;t.borderStyle='beveled';
    this.delay=false;eq(t.value,'First');eq(t.lineWidth,3);eq(t.borderStyle,'beveled');
    t.delay=false;t.value='Ready';t.borderStyle='solid';t.lineWidth=1;
    t.readonly=true;eq(t.readonly,true);t.readonly=false;eq(t.readonly,false);
    throws(function(){t.rect=[];});throws(function(){t.getItemAt(0);});
    eq(t.value,'Ready');
  )JS");
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());forms::Session s(writer->api(),forms::Fixture(actions),true);ASSERT_TRUE(s.valid());
    s.api.FORM_DoDocumentJSAction(s.form);EXPECT_EQ(s.Field(1),u"PASS");EXPECT_EQ(s.Field(0),u"Ready");
    images[backend++]=Input(s);ReloadInput(*writer,s,u"Ready",artifacts,"typed-field-edges");
  }
  CompareInputs(images[0],images[1],artifacts,"typed-field-edges-backends");
}

}  // namespace
}  // namespace pdfoxium::integration
