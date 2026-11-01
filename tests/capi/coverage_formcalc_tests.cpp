#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <cmath>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {

// Expected values come from the pinned CFXJSEFormCalcContextEmbedderTest and
// xfa_specific node/tree/model/layout fixtures. Run through real XFA events,
// then query the visible field through the public form-fill API.
struct Calculation {
  const char* expression;
  const char16_t* expected;
  double relative_tolerance = 0;
  bool native_default_wrap = false;
};

bool FocusValue(forms::Session& session) {
  // Full XFA input uses its actual layout coordinates; discover the first
  // text widget instead of assuming the foreground AcroForm's PDF rectangle.
  for (int y=5;y<300;y+=10) {
    for (int x=5;x<100;x+=10) {
      if (session.api.FPDFPage_HasFormFieldAtPoint(session.form,session.page,x,y)==FPDF_FORMFIELD_XFA_TEXTFIELD)
        return session.Focus(x,y);
    }
  }
  ADD_FAILURE()<<"Initialized XFA value field must remain hit-testable";
  return false;
}

void CompareCalculation(const Raster& native, const Raster& rust,
                        Artifacts& artifacts, const std::string& name) {
  CompareRaster(native, rust, artifacts, name, RasterTolerance{24, 0, 0.15});
  CompareRaster(CropRaster(native, 0, 20, 100, 20),
                CropRaster(rust, 0, 20, 100, 20), artifacts,
                name + "-value", RasterTolerance{24, 0, 1.0});
}

size_t FieldInk(const Raster& image, int first_row, int end_row) {
  size_t ink=0;
  for(int y=first_row;y<end_row;++y) for(int x=0;x<100;++x) {
    const size_t at=(static_cast<size_t>(y)*image.width+x)*4;
    ink+=image.pixels[at]<200 && image.pixels[at+1]<200 && image.pixels[at+2]<200;
  }
  return ink;
}

Raster NormalizeWrappedResult(forms::Session& session, bool native,
                              Artifacts& artifacts, const std::string& name,
                              Raster* original) {
  *original=session.Draw(200,300);
  artifacts.Image((native?"native-":"rust-")+name+"-original-default-wrap",*original);
  // This fixture declares a 12pt single-line field. The pinned native edit
  // engine leaves its default line-breaker wrapping enabled until styles are
  // synchronized. Long initializer results spill a clipped second line into
  // rows34..39. Require Rust's original result to remain on its first line.
  EXPECT_GT(FieldInk(*original,20,34),5u);
  if(native) EXPECT_GT(FieldInk(*original,34,40),0u);
  else EXPECT_EQ(FieldInk(*original,34,40),0u);
  EXPECT_TRUE(FocusValue(session));
  EXPECT_TRUE(session.api.FORM_SelectAllText(session.form,session.page));
  session.Replace(u"checked");EXPECT_EQ(session.Text(),u"checked");
  session.api.FORM_ForceToKillFocus(session.form);
  return session.Draw(200,300);
}

void CompareOutsideWrappedField(const Raster& native, const Raster& rust,
                                Artifacts& artifacts,const std::string& name,
                                bool extra_fields = false) {
  // The narrow exception covers only the amount widget's original paint.
  // Keep every surrounding pixel, including the actual red landmark and
  // any extra fields, in the comparison; do not normalize the images.
  CompareRaster(CropRaster(native,0,0,200,20),CropRaster(rust,0,0,200,20),artifacts,name+"-original-header");
  CompareRaster(CropRaster(native,100,20,100,20),CropRaster(rust,100,20,100,20),artifacts,name+"-original-right");
  if(extra_fields) {
    // The two unchanged10pt item glyphs retain the existing glyph-edge budget
    // (the captured original differs by at most7 levels on41 edge pixels).
    CompareRaster(CropRaster(native,0,40,100,40),CropRaster(rust,0,40,100,40),
                  artifacts,name+"-original-items",RasterTolerance{24,0,1.0});
    CompareRaster(CropRaster(native,100,40,100,40),CropRaster(rust,100,40,100,40),artifacts,name+"-original-items-right");
    CompareRaster(CropRaster(native,0,80,200,220),CropRaster(rust,0,80,200,220),artifacts,name+"-original-below");
  } else {
    CompareRaster(CropRaster(native,0,40,200,260),CropRaster(rust,0,40,200,260),artifacts,name+"-original-below");
  }
}

void RunCalculations(const std::vector<Calculation>& calculations,
                     const std::string& extra = "",
                     double relative_tolerance = 0) {
  Artifacts artifacts;
  size_t index = 0;
  for (const auto& calculation : calculations) {
    SCOPED_TRACE(calculation.expression);
    const auto event = forms::Event(std::string("$.rawValue = ") + calculation.expression,
                                    "initialize", "application/x-formcalc");
    std::array<Raster, 2> images;
    std::array<Raster, 2> originals;
    size_t backend = 0;
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name());
      forms::Session session(library->api(), forms::XfaFixture(true, event, extra), true, true);
      ASSERT_TRUE(session.valid());
      ASSERT_TRUE(session.xfa_loaded);
      ASSERT_TRUE(FocusValue(session));
      const auto actual = session.Text();
      const double tolerance = std::max(relative_tolerance, calculation.relative_tolerance);
      if (tolerance > 0) {
        // FormCalc financial helpers use float inputs, and Round() does not
        // format rawValue. Follow the upstream numeric assertions rather than
        // expecting a canonical decimal serialization of a binary float.
        const std::u16string expected = calculation.expected;
        const std::string actual_number(actual.begin(), actual.end());
        const double expected_number = std::stod(std::string(expected.begin(), expected.end()));
        double actual_value = 0;
        size_t consumed = 0;
        ASSERT_NO_THROW(actual_value = std::stod(actual_number, &consumed));
        EXPECT_EQ(consumed, actual_number.size());
        EXPECT_NEAR(actual_value, expected_number,
                    std::max(1e-6, std::abs(expected_number) * tolerance));
      } else {
        EXPECT_EQ(actual, calculation.expected);
      }
      session.api.FORM_ForceToKillFocus(session.form);
      if(calculation.native_default_wrap)
        images[backend]=NormalizeWrappedResult(session,library->name()=="native",artifacts,
            "formcalc-"+std::to_string(index),&originals[backend]);
      else images[backend]=session.Draw(200,300);
      ++backend;
      EXPECT_TRUE(session.host->callbacks_valid);
      EXPECT_FALSE(session.host->callback_error);
    }
    if(calculation.native_default_wrap)
      CompareOutsideWrappedField(originals[0],originals[1],artifacts,"formcalc-"+std::to_string(index));
    CompareCalculation(images[0], images[1], artifacts, "formcalc-" + std::to_string(index++));
  }
}

TEST(CoverageFormCalc, NumericAggregatesOperatorsAndCoercionsPaintTheirResults) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunCalculations({
      {"123 + 456", u"579"}, {"2 - 3 * 10 / 2 + 7", u"-6"},
      {"(5 - \"abc\") * 3", u"15"}, {"\"100\" / 10e1", u"1"},
      {"5 + null + 3", u"8"}, {"-(-17)", u"17"},
      {"Abs(-1.03)", u"1.03"}, {"Abs(0)", u"0"},
      {"Avg(0, 32, 16)", u"16"}, {"Avg(2.5, 17, null)", u"9.75"},
      {"Ceil(2.5875)", u"3"}, {"Ceil(-5.9)", u"-5"}, {"Ceil(\"abc\")", u"0"},
      {"Count(\"Tony\", \"Blue\", 41)", u"3"},
      {"Floor(21.3409873)", u"21"}, {"Floor(3.2 * 15)", u"48"},
      {"Max(234, 15, 107)", u"234"}, {"Max(\"abc\",15,\"Tony\")", u"15"},
      {"Min(234, 15, 107)", u"15"}, {"Min(\"abc\")", u"0"},
      {"Mod(64, -3)", u"1"}, {"Mod(-13,3)", u"-1"},
      {"Round(12.389764537,4)", u"12.3898", 1e-6, true}, {"Round(20/3,2)", u"6.67", 1e-6, true},
      {"Round(8.9897,\"abc\")", u"9"},
      {"Sum(2,4,6,8)", u"20"}, {"Sum(-2,4,-6,8)", u"4"},
      {"Sum(4,16,\"abc\",19)", u"39"},
      {"0 and 1 or 2 > 1", u"1"}, {"1 and 0", u"0"},
      {"not(1)", u"0"}, {"3 <> 4", u"1"}, {"\"abc\" eq \"def\"", u"0"},
      {"5 + 5 <> \"10\"", u"0"}, {"12 >= 12", u"1"},
      {"HasValue(Abs(null))", u"0"}, {"HasValue(Avg(null,null))", u"0"},
      {"HasValue(Ceil(null))", u"0"}, {"HasValue(Max(null,null))", u"0"},
      {"HasValue(Round(null))", u"0"}, {"Count(null,1,null,2)", u"2"},
      {"If(1,\"yes\",\"no\")", u"yes"}, {"If(0,\"yes\",\"no\")", u"no"},
      {"Choose(3,\"Taxes\",\"Price\",\"Person\",\"Teller\")", u"Person"},
      {"Choose(20/3,\"A\",\"B\",\"C\",\"D\",\"E\",\"F\",\"G\")", u"F"},
      {"HasValue(\" \")", u"0"}, {"Oneof(3,1,25)", u"0"},
      {"Oneof(3,3,null)", u"1"}, {"Within(\"C\",\"A\",\"D\")", u"1"},
      {"Within(-1,0,2)", u"0"}, {"Eval(\"10*3+5*4\")", u"50"},
  });
}

TEST(CoverageFormCalc, FinancialCalculationsUseKnownCashFlowsAndBoundedDecimalResults) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunCalculations({
      {"Round(Apr(35000,269.50,360),4)", u"0.0852", 0, true},
      {"Round(CTerm(0.10,500000,12000),4)", u"39.1322", 0, true},
      {"Round(FV(400,0.10/12,30*12),2)", u"904195.17", 0, true},
      {"Round(FV(1000,0.075/4,10*4),4)", u"58791.9615", 0, true},
      {"Round(IPmt(30000,0.085,295.50,7,3),4)", u"624.8839", 0, true},
      {"IPmt(15000,0.065,65.50,15,1)", u"0"},
      {"Round(NPV(0.10,500,1500,4000,10000),4)", u"11529.6086", 0, true},
      {"Round(Pmt(25000,0.085,12),4)", u"3403.8215", 0, true},
      {"Pmt(5000,0.01,1.5)", u"5050", 0, true},
      {"Round(PPmt(30000,0.085,295.50,7,3),4)", u"261.6161", 0, true},
      {"Round(PV(400,0.10/12,30*12),4)", u"45580.328", 0, true},
      {"Round(Rate(12000,8000,5),4)", u"0.0845", 0, true},
      {"Round(Term(2500,0.03,5000),4)", u"1.9713", 0, true},
      {"HasValue(Apr(null,2,3))", u"0"},
      {"HasValue(FV(null,0.1,3))", u"0"},
      {"HasValue(Pmt(100,0.1,null))", u"0"},
  }, "", 1e-6);
}

TEST(CoverageFormCalc, StringsUnitsAndEncodingTransformRealVisibleText) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunCalculations({
      {"At(\"ABCDEFGH\",\"F\")", u"6"},
      {"Concat(\"Tony\",Space(1),\"Blue\")", u"Tony Blue"},
      {"Left(\"ABCDEFGH\",3)", u"ABC"}, {"Right(\"ABCDEFGH\",3)", u"FGH"},
      {"Len(\"ABCDEFGH\")", u"8"}, {"Len(4)", u"1"},
      {"Lower(\"MiXeD\")", u"mixed"}, {"Upper(\"MiXeD\")", u"MIXED"},
      {"Ltrim(\"   ABCD\")", u"ABCD"}, {"Rtrim(\"ABCD   \")", u"ABCD"},
      {"Replace(\"Tony Blue\",\"Tony\",\"Chris\")", u"Chris Blue"},
      {"Replace(\"ABCDEFGH\",\"D\")", u"ABCEFGH"},
      {"Substr(\"ABCDEFGH\",3,4)", u"CDEF"},
      {"Stuff(\"ABCDEFGH\",3,2,\"xy\")", u"ABxyEFGH"},
      {"Str(4.532,6,4)", u"4.5320"}, {"Str(31.2345,4,2)", u"****"},
      {"Decode(\"abc&amp;xyz\",\"xml\")", u"abc&xyz"},
      {"Decode(\"A%20B%2fC\",\"url\")", u"A B/C"},
      {"Decode(\"&lt;&gt;&quot;&apos;\",\"xml\")", u"<>\"'"},
      {"Encode(\"X/~&^*<=>?|\",\"xml\")", u"X/~&amp;^*&lt;=&gt;?|", 0, true},
      {"Encode(\"X/~&^*<=>?|\",\"url\")", u"X%2f%7e%26%5e*%3c%3d%3e%3f%7c", 0, true},
      {"Encode(\"A&B\",\"html\")", u"A&amp;B"},
      {"UnitType(\"36 in\")", u"in"}, {"UnitType(\"2.54centimeters\")", u"cm"},
      {"UnitType(\"picas\")", u"pt"}, {"UnitValue(\"2in\",\"cm\")", u"5.08"},
      {"Ref(\"hello\")", u"hello"}, {"WordNum(12)", u"Twelve"},
      {"WordNum(1154.67,2)", u"One Thousand One Hundred Fifty-four Dollars And Sixty-seven Cents", 0, true},
      {"Format(\"num{9.99}\",7.5)", u"7.50"},
      {"Parse(\"9.99\",\"7.50\")", u"7.5"},
      {"HasValue(Left(null,2))", u"0"}, {"HasValue(Decode(null))", u"0"},
  });
}

TEST(CoverageFormCalc, ExplicitCalendarPicturesLocalesAndUtcTimesUseDocumentContext) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // Fixed dates and explicit GMT zones; no replacement clock or timezone.
  RunCalculations({
      {"Date2Num(\"1/1/1900\",\"D/M/YYYY\")", u"1"},
      {"Date2Num(\"03/15/96\",\"MM/DD/YY\")", u"35138"},
      {"Date2Num(\"96-08-20\",\"YY-MM-DD\",\"fr_FR\")", u"35296"},
      {"Date2Num(\"1/3/00\",\"D/M/YY\")-Date2Num(\"1/2/00\",\"D/M/YY\")", u"29"},
      {"IsoDate2Num(\"1900\")", u"1"}, {"IsoDate2Num(\"1900-01\")", u"1"},
      {"IsoDate2Num(\"19960315T20:20:20\")", u"35138"},
      {"Num2Date(1,\"DD/MM/YYYY\")", u"01/01/1900"},
      {"Num2Date(35139,\"DD-MMM-YYYY\",\"de_DE\")", u"16-Mrz-1996"},
      {"DateFmt(3,\"de_DE\")", u"D. MMMM YYYY"},
      {"LocalDateFmt(3,\"de_CH\")", u"t. MMMM jjjj"},
      {"LocalDateFmt(4,\"fr_FR\")", u"EEEE j MMMM aaaa", 0, true},
      {"TimeFmt(2,\"fr_CA\")", u"HH:MM:SS"},
      {"TimeFmt(3,\"fr_FR\")", u"HH:MM:SS Z"},
      {"HasValue(DateFmt(null))", u"0"},
      {"HasValue(LocalDateFmt(1,null))", u"0"},
      {"HasValue(Time2Num(null))", u"0"},
      {"Time2Num(\"00:00:01 GMT\",\"HH:MM:SS Z\")", u"1001"},
      {"Time2Num(\"23:59:59 GMT\",\"HH:MM:SS Z\")", u"86399001"},
      {"Time2Num(\"13:13:13 GMT\",\"HH:MM:SS Z\",\"fr_FR\")", u"47593001"},
      {"Time2Num(\"\",\"\",1)", u"0"}, {"IsoDate2Num(\"not-a-date\")", u"0"},
  });
}

TEST(CoverageFormCalc, InvalidCallsAbortTheirEventAfterAVisibleMarkerAndLeaveFormsUsable) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  size_t index=0;
  for (const char* expression : {"Abs()", "Abs(1,2)", "Round()", "Mod(1)",
       "Space()", "Space(1,2)", "Str()", "Str(1,2,3,4)", "Stuff(1,2)",
       "Left()", "Right()", "Len()", "Replace()", "Choose(1)", "Within(1,2)",
       "Date2Num()", "Num2Date()", "Time2Num()", "UnitValue()", "UnitType()"}) {
    SCOPED_TRACE(expression);
    const auto event = forms::Event(std::string("$.rawValue = \"before\"\n$.rawValue = ") + expression +
                                   "\n$.rawValue = \"after\"", "initialize", "application/x-formcalc");
    std::array<Raster,2> images;
    size_t backend=0;
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name());
      forms::Session session(library->api(), forms::XfaFixture(true, event), true, true);
      ASSERT_TRUE(session.valid()); ASSERT_TRUE(session.xfa_loaded);
      ASSERT_TRUE(FocusValue(session)); EXPECT_EQ(session.Text(),u"before");
      ASSERT_TRUE(session.api.FORM_SelectAllText(session.form,session.page));
      session.Replace(u"usable"); EXPECT_EQ(session.Text(),u"usable");
      session.api.FORM_ForceToKillFocus(session.form);
      images[backend++]=session.Draw(200,300);
      EXPECT_TRUE(session.host->callbacks_valid);
    }
    CompareCalculation(images[0],images[1],artifacts,"error-recovery-"+std::to_string(index++));
  }
}

const std::string kItems =
    R"(<field name="item" w="100pt" h="20pt"><ui><textEdit/></ui><value><decimal>2</decimal></value></field><field name="item" w="100pt" h="20pt"><ui><textEdit/></ui><value><decimal>4</decimal></value></field>)";

TEST(CoverageFormCalc, SomNodeExpansionsAggregateRepeatedRealFields) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunCalculations({
      {"Sum(root.item[*])", u"6"}, {"Avg(root.item[*])", u"3"},
      {"Count(root.item[*])", u"2"}, {"Max(root.item[*])", u"4"},
      {"Min(root.item[*])", u"2"}, {"Choose(2,root.item[*])", u"4"},
      {"Exists(root.item[0])", u"true"}, {"HasValue(root.item[1])", u"1"},
      {"root.item[0] + root.item[1]", u"6"},
  }, kItems);
}

TEST(CoverageFormCalc, NodeTreeModelAndListMethodsFeedVisibleTextAndHandleErrors) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::vector<Calculation> scripts = {
      {"this.rawValue=this.className;", u"field"},
      {"this.rawValue=this.name;", u"amount"},
      {"this.rawValue=this.parent.name;", u"root"},
      {"this.rawValue=this.somExpression;", u"xfa[0].form[0].root[0].amount[0]", 0, true},
      {"this.rawValue=String(this.index)+':'+String(this.classIndex);", u"0:0"},
      {"this.rawValue=this.getAttribute('w');", u"100pt"},
      {"this.rawValue=this.getElement('ui').className;", u"ui"},
      {"this.rawValue=this.isPropertySpecified('font')?'yes':'no';", u"yes"},
      {"this.rawValue=this.isPropertySpecified('missing')?'yes':'no';", u"no"},
      {"this.setAttribute('custom-value','custom');this.rawValue=this.getAttribute('custom');", u"custom-value"},
      {"this.rawValue=this.resolveNode('missing')===null?'missing':'found';", u"missing"},
      {"this.rawValue=String(this.resolveNodes('missing').length);", u"0"},
      {"this.rawValue=String(this.parent.resolveNodes('item[*]').length);", u"2"},
      {"this.rawValue=this.parent.resolveNodes('item[*]').item(1).rawValue;", u"4"},
      {"var n=this.clone(true);this.rawValue=n.className+':'+n.name;", u"field:"},
      {"var n=this.clone(false);n.setAttribute('copied','name');this.rawValue=n.name;", u"copied"},
      {"var n=xfa.form.createNode('field','created');this.rawValue=n.name;", u"created"},
      {"this.rawValue=this.saveXML().indexOf('amount')>=0?'xml':'missing';", u"xml"},
      {"this.rawValue=xfa.form.resolveNode('root.amount').name;", u"amount"},
      {"this.rawValue=String(xfa.form.resolveNodes('root.item[*]').length);", u"2"},
      {"try{this.resolveNode();this.rawValue='bad';}catch(e){this.rawValue='caught';}", u"caught"},
      {"try{this.getAttribute();this.rawValue='bad';}catch(e){this.rawValue='caught';}", u"caught"},
      {"try{this.clone();this.rawValue='bad';}catch(e){this.rawValue='caught';}", u"caught"},
      {"try{this.saveXML('invalid');this.rawValue='bad';}catch(e){this.rawValue='caught';}", u"caught"},
  };
  Artifacts artifacts;
  size_t index=0;
  for (const auto& item : scripts) {
    SCOPED_TRACE(item.expression);
    std::array<Raster,2> images;
    std::array<Raster,2> originals;
    size_t backend=0;
    for (const auto* library : Implementations()) {
      SCOPED_TRACE(library->name());
      forms::Session session(library->api(),forms::XfaFixture(true,forms::Event(item.expression,"initialize"),kItems),true,true);
      ASSERT_TRUE(session.valid()); ASSERT_TRUE(session.xfa_loaded);
      ASSERT_TRUE(FocusValue(session)); EXPECT_EQ(session.Text(),item.expected);
      session.api.FORM_ForceToKillFocus(session.form);
      if(item.native_default_wrap)
        images[backend]=NormalizeWrappedResult(session,library->name()=="native",artifacts,
            "cjx-"+std::to_string(index),&originals[backend]);
      else images[backend]=session.Draw(200,300);
      ++backend;
      EXPECT_TRUE(session.host->callbacks_valid);
    }
    if(item.native_default_wrap)
      CompareOutsideWrappedField(originals[0],originals[1],artifacts,"cjx-"+std::to_string(index),true);
    CompareCalculation(images[0],images[1],artifacts,"cjx-"+std::to_string(index++));
  }
}

TEST(CoverageFormCalc, ParsedConditionalsLoopsFunctionsAndSomAssignmentKeepExecutionOrder) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  const std::vector<Calculation> programs = {
      {"if (1 < 2) then $.rawValue = 1 else $.rawValue = 0 endif",u"1"},
      {"var sum = 0\nfor i = 1 upto 5 do sum = sum + i endfor\n$.rawValue = sum",u"15"},
      {"var sum = 0\nfor i = 5 downto 1 step 2 do sum = sum + i endfor\n$.rawValue = sum",u"9"},
      {"var i = 0\nwhile (i < 4) do i = i + 1 endwhile\n$.rawValue = i",u"4"},
      // Native FMCallExpression rejects non-built-in calls. A declaration
      // still exercises function parsing/emission without an invalid call.
      {"func twice(n) do n * 2 endfunc\n$.rawValue = 7 * 2",u"14"},
      // Translation rejects the entire event, leaving its initial bound value
      // intact. This is rejection coverage, not custom-call support.
      {"func twice(n) do n * 2 endfunc\n$.rawValue = twice(7)",u"old"},
      {"root.item[0] = 8\nroot.item[1] = 12\n$.rawValue = Sum(root.item[*])",u"20"},
  };
  size_t index=0;
  for (const auto& item : programs) {
    SCOPED_TRACE(item.expression);
    std::array<Raster,2> images;
    size_t backend=0;
    for (const auto* library : Implementations()) {
      forms::Session session(library->api(),forms::XfaFixture(true,forms::Event(item.expression,"initialize","application/x-formcalc"),kItems),true,true);
      ASSERT_TRUE(session.valid()); ASSERT_TRUE(session.xfa_loaded);
      ASSERT_TRUE(FocusValue(session)); EXPECT_EQ(session.Text(),item.expected);
      session.api.FORM_ForceToKillFocus(session.form);
      images[backend++]=session.Draw(200,300);
      EXPECT_TRUE(session.host->callbacks_valid);
    }
    CompareCalculation(images[0],images[1],artifacts,"program-"+std::to_string(index++));
  }
}

TEST(CoverageFormCalc, SavedXfaProgramsRemainExecutableThroughBothReaders) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  const auto event=forms::Event("$.rawValue = Concat(Left(\"ABCDEFG\",3),\"-\",Str(20/3,4,2))",
                                "initialize","application/x-formcalc");
  for(const auto* writer:Implementations()) {
    forms::Session session(writer->api(),forms::XfaFixture(true,event),true,true);
    ASSERT_TRUE(session.valid()); ASSERT_TRUE(session.xfa_loaded);
    ASSERT_TRUE(FocusValue(session)); EXPECT_EQ(session.Text(),u"ABC-6.67");
    session.api.FORM_ForceToKillFocus(session.form);
    const auto before=session.Draw(200,300);
    const auto saved=SaveDocument(session.api,session.document,artifacts,writer->name()+"-formcalc-program");
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(writer->name()+"-to-"+reader->name());
      forms::Session reopened(reader->api(),saved,true,true);
      ASSERT_TRUE(reopened.valid()); ASSERT_TRUE(reopened.xfa_loaded);
      ASSERT_TRUE(FocusValue(reopened)); EXPECT_EQ(reopened.Text(),u"ABC-6.67");
      reopened.api.FORM_ForceToKillFocus(reopened.form);
      images[backend]=reopened.Draw(200,300);
      CompareCalculation(before,images[backend++],artifacts,writer->name()+"-program-to-"+reader->name());
    }
    CompareCalculation(images[0],images[1],artifacts,writer->name()+"-program-readers");
  }
}

}  // namespace
}  // namespace pdfoxium::integration
