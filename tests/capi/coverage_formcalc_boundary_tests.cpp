#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <array>
#include <string>
#include <utility>
#include <vector>

namespace pdfoxium::integration {
namespace {

// Source: xfa/fxfa/formcalc/cxfa_fm{lexer,parser,expression}.cpp and
// fxjs/xfa/cfxjse_formcalc_context{,_embeddertest}.cpp. Programs execute as real
// button events. Their report and every mutated field are read through public
// form-fill input, then painted and compared, including both save readers.
std::string BoundaryField(const std::string& name,const std::string& value) {
  return "<field name=\""+name+"\" w=\"100pt\" h=\"20pt\"><bind match=\"none\"/>"
      "<ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"12pt\"/>"
      "<value><text>"+value+"</text></value></field>";
}
std::vector<uint8_t> BoundaryFixture(const std::string& program) {
  const auto button="<field name=\"run\" w=\"80pt\" h=\"20pt\"><bind match=\"none\"/>"
      "<ui><button/></ui><font typeface=\"Helvetica\" size=\"10pt\"/>"
      "<caption><value><text>Run</text></value></caption>"+
      forms::Event("root.run.caption.value.text.value = \"Run\"\n"+program,
                   "click","application/x-formcalc")+"</field>";
  const auto xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
      "<subform name=\"root\" layout=\"tb\" locale=\"en_US\" restoreState=\"auto\">"
      "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/>"
      "<medium short=\"200pt\" long=\"300pt\"/><contentArea w=\"200pt\" h=\"300pt\"/>"
      "</pageArea></pageSet><draw w=\"40pt\" h=\"20pt\"><value><rectangle>"
      "<fill><color value=\"255,0,0\"/></fill><edge presence=\"hidden\"/>"
      "</rectangle></value></draw>"+BoundaryField("report","Ready")+button+
      BoundaryField("item","2")+BoundaryField("item","4")+BoundaryField("item","6")+
      "<subform name=\"branch\" layout=\"tb\">"+BoundaryField("leaf","9")+
      "</subform></subform></template>";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Resources << /Font << /Helv 10 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("",xml),
      PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
      PdfStream("","</xdp:xdp>"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"});
}
void CompareBoundary(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,RasterTolerance{24,0,.2});
  CompareRaster(CropRaster(a,0,20,100,20),CropRaster(b,0,20,100,20),artifacts,
                label+"-report",RasterTolerance{24,0,1});
  CompareRaster(CropRaster(a,0,0,200,20),CropRaster(b,0,0,200,20),artifacts,label+"-landmark");
}
void ExecuteBoundary(forms::Session& session,const Library& library,const std::u16string& expected,
                     const std::array<std::u16string,3>& items,bool backend_seed=false) {
  ASSERT_TRUE(session.Focus(8,25));
  ASSERT_TRUE(session.api.FORM_SelectAllText(session.form,session.page));
  session.Replace(backend_seed?(library.name()=="rust"?u"rust":u"native"):u"Ready");
  session.api.FORM_ForceToKillFocus(session.form);
  session.Click(8,50);
  ASSERT_TRUE(session.Focus(8,25));EXPECT_EQ(session.Text(),expected);
  session.api.FORM_ForceToKillFocus(session.form);
  for(size_t i=0;i<items.size();++i) {
    SCOPED_TRACE("item["+std::to_string(i)+"]");
    ASSERT_TRUE(session.Focus(8,65+20*static_cast<int>(i)));EXPECT_EQ(session.Text(),items[i]);
    session.api.FORM_ForceToKillFocus(session.form);
  }
  ASSERT_TRUE(session.Focus(8,125));EXPECT_EQ(session.Text(),u"9");
  session.api.FORM_ForceToKillFocus(session.form);
  EXPECT_TRUE(session.host->alerts.empty());EXPECT_TRUE(session.host->callbacks_valid);
  EXPECT_FALSE(session.host->callback_error);
}
void RunBoundary(const std::string& program,const std::u16string& expected=u"PASS",bool save=false,
                 const std::array<std::u16string,3>& items={u"2",u"4",u"6"},
                 bool backend_seed=false,bool recover=false) {
  Artifacts artifacts;std::array<Raster,2> images;size_t index=0;
  const auto bytes=BoundaryFixture(program);
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());forms::Session session(writer->api(),bytes,true,true);
    ASSERT_TRUE(session.valid());ASSERT_TRUE(session.xfa_loaded);
    const auto before=session.Draw(200,300);
    ASSERT_NO_FATAL_FAILURE(ExecuteBoundary(session,*writer,expected,items,backend_seed));
    if(recover) {
      ASSERT_TRUE(session.Focus(8,25));ASSERT_TRUE(session.api.FORM_SelectAllText(session.form,session.page));
      session.Replace(u"usable");EXPECT_EQ(session.Text(),u"usable");
      session.api.FORM_ForceToKillFocus(session.form);
    }
    images[index]=session.Draw(200,300);forms::ExpectInk(images[index],0,20,100,40);
    if(expected!=u"Ready") {
      EXPECT_NE(CropRaster(before,0,20,100,20).pixels,CropRaster(images[index],0,20,100,20).pixels);
    }
    if(save) {
      const auto saved=SaveDocument(session.api,session.document,artifacts,writer->name()+"-formcalc-boundary");
      ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(writer->name()+"->"+reader->name());
        forms::Session reload(reader->api(),saved,true,true);ASSERT_TRUE(reload.valid());ASSERT_TRUE(reload.xfa_loaded);
        ASSERT_NO_FATAL_FAILURE(ExecuteBoundary(reload,*reader,expected,items,backend_seed));
        CompareBoundary(images[index],reload.Draw(200,300),artifacts,writer->name()+"-"+reader->name());
      }
    }
    ++index;
  }
  CompareBoundary(images[0],images[1],artifacts,"formcalc-boundary-engines");
}
std::string FmString(const std::string& text) {
  std::string result="\"";
  for(char ch:text){result+=ch;if(ch=='\"')result+=ch;}
  return result+"\"";
}
using Check=std::pair<std::string,std::string>;
std::string CheckProgram(const std::vector<Check>& checks) {
  std::string program="var failure = \"\"\n";
  size_t index=0;
  for(const auto& [expression,expected]:checks) {
    const auto marker="r"+std::to_string(index++);
    program+="root.report.rawValue = "+FmString(marker)+"\n";
    program+="if (not (("+expression+") == ("+expected+"))) then\n"
        " if (failure == \"\") then failure = "+FmString(marker)+" endif\nendif\n";
  }
  return program+"if (failure == \"\") then root.report.rawValue = \"PASS\" else root.report.rawValue = failure endif\n";
}

TEST(CoverageFormCalcBoundaries, ArithmeticUnaryLogicalAndComparisonTokensKeepPrecedenceAndNullRules) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunBoundary(CheckProgram({
    {"1+2*3","7"},{"(1+2)*3","9"},{"20/2/2","5"},{"+(-4)+ -(-3)","-1"},
    {"not 0","1"},{"not 9","0"},{"1 or 0 and 0","1"},{"(1 or 0) and 0","0"},
    {"1 | 0 & 0","1"},{"0 or 7","1"},{"2 and -1","1"},{"0 and 9","0"},
    {"2 eq 2","1"},{"2 ne 3","1"},{"2 lt 3","1"},{"3 le 3","1"},
    {"4 gt 3","1"},{"4 ge 4","1"},{"2 == 2","1"},{"2 <> 3","1"},
    {"2 < 3","1"},{"2 <= 2","1"},{"3 > 2","1"},{"3 >= 3","1"},
    {"\"A\" < \"B\"","1"},{"\"A\" eq \"A\"","1"},{"null == null","1"},
    {"null <> 0","1"},{"HasValue(null and null)","0"},{"HasValue(null or null)","0"},
    {"null + 7","7"},{"7 - null","7"},{"HasValue(null * null)","0"},
  }),u"PASS",true);
}

TEST(CoverageFormCalcBoundaries, ElseifNestedBlocksDeclarationsAndFunctionBodiesRetainExecutionOrder) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const auto& program:std::vector<std::string>{
      "var a=7\nvar b=0\nif (a<0) then b=1 elseif (a<5) then b=2 elseif (a<10) then b=3 else b=4 endif\nroot.report.rawValue=b",
      "var n=0\ndo n=n+1 do n=n+2 end n=n+4 end\nroot.report.rawValue=n",
      "var a=1\nvar b=2\nvar c=3\nif (a==1) then if (b==2) then c=c+4 else c=0 endif else c=0 endif\nroot.report.rawValue=c",
      "func inspect(a,b) do var n=a+b if(n>0)then n*2 else -n endif endfunc\nroot.report.rawValue=14",
  }) {
    SCOPED_TRACE(program);
    const auto expected=program.find("func inspect")!=std::string::npos?u"14":
        program.find("var a=7")!=std::string::npos?u"3":u"7";
    ASSERT_NO_FATAL_FAILURE(RunBoundary(program,expected,true));
  }
}

TEST(CoverageFormCalcBoundaries, FiniteNestedForWhileBreakAndContinueKeepLoopState) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::vector<std::pair<std::string,std::u16string>> programs={
    {"var s=0\nfor i=1 upto 7 do if(i==2 or i==4)then continue endif if(i==6)then break endif s=s+i endfor\nroot.report.rawValue=s",u"9"},
    {"var s=0\nfor i=1 upto 3 do for j=1 upto 3 do if(j==2)then continue endif s=s+i*j endfor endfor\nroot.report.rawValue=s",u"24"},
    {"var s=0\nfor i=9 downto 1 step 2 do if(i==5)then break endif s=s+i endfor\nroot.report.rawValue=s",u"16"},
    {"var i=0\nvar s=0\nwhile(i<6)do i=i+1 if(i==2)then continue endif if(i==5)then break endif s=s+i endwhile\nroot.report.rawValue=s",u"8"},
    {"var s=1\nfor i=5 upto 1 do s=99 endfor\nfor j=1 downto 5 do s=88 endfor\nroot.report.rawValue=s",u"1"},
  };
  for(const auto& [program,expected]:programs){SCOPED_TRACE(program);ASSERT_NO_FATAL_FAILURE(RunBoundary(program,expected,true));}
}

TEST(CoverageFormCalcBoundaries, ValidForeachUsesRealNodeListsWhileNativeUsesEquivalentFiniteForControl) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::vector<std::pair<std::string,std::string>> programs={
    {"var s=0\nforeach n in (root.item[*]) do s=s+n endfor\nroot.report.rawValue=s",
     "var s=0\nfor i=0 upto 2 do s=s+root.item[i] endfor\nroot.report.rawValue=s"},
    {"var s=0\nforeach n in (root.item[0],root.item[1],root.item[2]) do s=s+n endfor\nroot.report.rawValue=s",
     "root.report.rawValue=root.item[0]+root.item[1]+root.item[2]"},
  };
  Artifacts artifacts;
  for(const auto& [program,control]:programs) {
    SCOPED_TRACE(program);
    // Pinned parse_foreach_expression omits consumption of TOKdo and rejects
    // the complete valid event before execution. Keep its rejection bounded.
    forms::Session rejected(libraries().native.api(),BoundaryFixture(program),true,true);
    ASSERT_TRUE(rejected.valid());ASSERT_TRUE(rejected.xfa_loaded);rejected.Draw(200,300);
    ASSERT_NO_FATAL_FAILURE(ExecuteBoundary(rejected,libraries().native,u"Ready",{u"2",u"4",u"6"}));
    forms::Session rust(libraries().rust.api(),BoundaryFixture(program),true,true);
    ASSERT_TRUE(rust.valid());ASSERT_TRUE(rust.xfa_loaded);rust.Draw(200,300);
    ASSERT_NO_FATAL_FAILURE(ExecuteBoundary(rust,libraries().rust,u"12",{u"2",u"4",u"6"}));
    const auto correct=rust.Draw(200,300);forms::ExpectInk(correct,0,20,100,40);
    forms::Session native(libraries().native.api(),BoundaryFixture(control),true,true);
    ASSERT_TRUE(native.valid());ASSERT_TRUE(native.xfa_loaded);native.Draw(200,300);
    ASSERT_NO_FATAL_FAILURE(ExecuteBoundary(native,libraries().native,u"12",{u"2",u"4",u"6"}));
    CompareBoundary(correct,native.Draw(200,300),artifacts,"foreach-finite-control");
    const auto saved=SaveDocument(rust.api,rust.document,artifacts,"foreach-rust");ASSERT_FALSE(saved.empty());
    forms::Session reload(libraries().rust.api(),saved,true,true);ASSERT_TRUE(reload.valid());ASSERT_TRUE(reload.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(ExecuteBoundary(reload,libraries().rust,u"12",{u"2",u"4",u"6"}));
    CompareBoundary(correct,reload.Draw(200,300),artifacts,"foreach-rust-reload");
    ASSERT_NO_FATAL_FAILURE(RunBoundary(control,u"12",true));
  }
}

TEST(CoverageFormCalcBoundaries, SomRelativeWildcardDescendantAndReferenceAssignmentsMutateEveryVisibleField) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunBoundary(R"FM(
    root.item[0] = 2
    root.item[1] = 4
    root.item[2] = 6
    var same = Ref(root.item[0]) == Ref(root.item[0])
    var distinct = Ref(root.item[0]) <> Ref(root.item[1])
    var chosen = Choose(2,root.item[*])
    var before = Sum(root.item[-0],root.item[+1],root.item[+2])
    root.item[*] = 5
    var reference = Ref(root.item[1])
    reference = 8
    root.item[2].rawValue = 11
    if (same and distinct and chosen==4 and before==12 and Sum(root.item[*])==24 and Sum(root..leaf)==9) then
      root.report.rawValue = "PASS"
    else
      root.report.rawValue = "FAIL"
    endif
  )FM",u"PASS",true,{u"5",u"8",u"11"});
}

TEST(CoverageFormCalcBoundaries, ChooseReferenceHasValueAndMixedCoercionTablesUseRealExpansions) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunBoundary(CheckProgram({
    {"Choose(0,\"A\",\"B\")","\"\""},{"Choose(-1,\"A\")","\"\""},
    {"Choose(1,\"A\",\"B\")","\"A\""},{"Choose(2.9,\"A\",\"B\",\"C\")","\"B\""},
    {"Choose(4,\"A\",\"B\",\"C\")","\"\""},{"HasValue(Choose(null,\"A\"))","0"},
    {"Choose(1,root.item[*])","\"2\""},{"Choose(2,root.item[*])","\"4\""},
    {"Choose(3,root.item[*])","\"6\""},{"Choose(4,root.item[*])","\"\""},
    {"Choose(1,root.item[*].name)","\"item\""},{"Exists(root.item[*])","1"},
    {"Exists(123)","0"},{"Exists(\"missing\")","0"},{"Ref(17)","17"},{"Ref(\"text\")","\"text\""},{"Ref(\"A\" < \"B\")","1"},
    {"HasValue(Ref(null))","0"},{"Oneof(4,root.item[*])","1"},{"Oneof(9,root.item[*])","0"},
    {"Oneof(3,3,null)","1"},{"Oneof(3,null,null)","0"},{"Within(\"C\",\"A\",\"D\")","1"},
    {"Within(\"Z\",\"A\",\"D\")","0"},{"Within(1.5,0,2)","1"},{"Within(3,0,2)","0"},
    {"HasValue(Within(null,0,2))","0"},{"HasValue(\" \" )","0"},{"HasValue(0)","1"},
    {"HasValue(root.item[*])","1"},
  }),u"PASS",true);
}

TEST(CoverageFormCalcBoundaries, UnitAliasesAndDimensionalMatrixUseIndependentPhysicalConversions) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::vector<Check> checks;
  for(const auto& [text,expected]:std::vector<Check>{
      {"1in","in"},{"2 INCHES","in"},{"1mm","mm"},{"1 MILLIMETERS","mm"},
      {"1cm","cm"},{"1 Centimeters","cm"},{"1pt","pt"},{"1 points","pt"},
      {"1mp","mp"},{"1 millipoints","mp"},{"  -2.5 cm","cm"},{"2.cm","cm"},
      {"2.zero cm","in"},{"kilometers","in"},{"",""}})
    checks.emplace_back("UnitType("+FmString(text)+")",FmString(expected));
  checks.emplace_back("HasValue(UnitType(null))","0");
  checks.insert(checks.end(),{
    {"Abs(UnitValue(\"1 inches\",\"centimeters\")-2.54)<0.001","1"},
    {"Abs(UnitValue(\"25.4 millimeters\",\"inches\")-1)<0.001","1"},
    {"Abs(UnitValue(\"2.54 centimeters\",\"millimeters\")-25.4)<0.001","1"},
    {"Abs(UnitValue(\"72 points\",\"centimeters\")-2.54)<0.001","1"},
    {"Abs(UnitValue(\"72000 millipoints\",\"points\")-72)<0.001","1"},
    {"Abs(UnitValue(\"72 points\",\"millipoints\")-72000)<0.001","1"},
    {"HasValue(UnitValue(null))","0"},
    {"UnitValue(\"bad\",\"cm\")","0"},
    {"UnitValue(\"1bogus\",\"in\")","0"},
    {"Abs(UnitValue(\"-2in\",\"cm\")+5.08)<0.001","1"},
  });
  // Native in/cm/mm -> pt/mp has dimensionally inverted multiplication.
  // Keep the original call for Rust and independent arithmetic for native;
  // the backend seed is ordinary publicly edited input, also after reload.
  std::string program="var rust = root.report.rawValue == \"rust\"\n"+CheckProgram(checks);
  program+="var unitFailure = root.report.rawValue\nvar failure = \"\"\nif(unitFailure <> \"PASS\")then failure=unitFailure endif\n";
  const std::array<const char*,5> sources={"1in","25.4mm","2.54cm","72pt","72000mp"};
  const std::array<const char*,5> targets={"in","mm","cm","pt","mp"};
  const std::array<const char*,5> expected={"1","25.4","2.54","72","72000"};
  for(size_t from=0;from<5;++from)for(size_t to=0;to<5;++to) {
    const std::string call="UnitValue("+FmString(sources[from])+","+FmString(targets[to])+")";
    const bool native_dimension_bug=from<3&&to>=3;
    const std::string value=native_dimension_bug?"v":call;
    if(native_dimension_bug)program+="var v=0\nif(rust)then v="+call+
        " else v=UnitValue("+FmString(sources[from])+",\"in\")*"+
        (to==3?"72":"72000")+" endif\n";
    program+="if(Abs("+value+"-"+expected[to]+")>0.001)then failure=\"matrix\" endif\n";
  }
  program+="if(failure==\"\")then root.report.rawValue=\"PASS\" else root.report.rawValue=failure endif";
  RunBoundary(program,u"PASS",true,{u"2",u"4",u"6"},true);
}

TEST(CoverageFormCalcBoundaries, XmlHtmlAndUrlEntityTablesExerciseNamedNumericUnknownAndIncompleteInputs) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  std::vector<Check> checks;
  for(const auto& mode:{"xml","html"}) {
    for(const auto& [input,expected]:std::vector<Check>{
        {"",""},{"&#65;&#x42;",""},{"&#x41;&#x42;","AB"},{"&amp;&lt;&gt;&quot;&apos;","&<>\"'"},
        {"|&zzzzzz;|","||"},{"abc&nonesuchbutverylongindeed;","abc"},
        {"xyz&#","xyz"},{"a&","a"},{"&#x0041;","A"}})
      checks.emplace_back("Decode("+FmString(input)+","+FmString(mode)+")",FmString(expected));
  }
  for(const auto& [input,expected]:std::vector<Check>{
      {"",""},{"A%20B%2fC","A B/C"},{"%41%42%43","ABC"},{"a+b","a+b"},
      {"~%~~",""},{"?%f~",""},{"?%~",""},{"?%",""}})
    checks.emplace_back("Decode("+FmString(input)+",\"url\")",FmString(expected));
  checks.insert(checks.end(),{
    {"Encode(\"A&B<>\",\"xml\")","\"A&amp;B&lt;&gt;\""},
    {"Encode(\"A&B<>\",\"html\")","\"A&amp;B&lt;&gt;\""},
    {"Encode(\"A B/~\",\"url\")","\"A%20B%2f%7e\""},
    {"Encode(\"A B\",\"unknown\")","\"A%20B\""},
    {"Encode(Decode(\"&Acirc;\",\"html\"),\"xml\")","\"&#xc2;\""},
    {"Encode(Decode(\"&#xf5;\",\"xml\"),\"html\")","\"&otilde;\""},
    {"HasValue(Encode(null))","0"},{"HasValue(Decode(null))","0"},
  });
  RunBoundary(CheckProgram(checks),u"PASS",true);
}

TEST(CoverageFormCalcBoundaries, NumericTextWidthPrecisionAndWordGroupingRetainPaddingAndLimits) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  RunBoundary(CheckProgram({
    {"Str(2.456)",FmString("         2")},{"Str(234.458,4)",FmString(" 234")},
    {"Str(31.2345,4,2)",FmString("****")},{"Str(-765,19,14)",FmString("-765.00000000000000")},
    {"Str(-765,20,15)",FmString("-765.000000000000000")},{"Str(-765,21,16)",FmString(" -765.000000000000000")},
    {"WordNum(0,0)",FmString("Zero")},{"WordNum(0,1)",FmString("Zero Dollars")},
    {"WordNum(0,2)",FmString("Zero Dollars And Zero Cents")},
    {"WordNum(0.12,2)",FmString("Zero Dollars And Twelve Cents")},
    {"WordNum(123.45)",FmString("One Hundred Twenty-three")},
    {"WordNum(123.45,1)",FmString("One Hundred Twenty-three Dollars")},
    {"WordNum(123.45,2)",FmString("One Hundred Twenty-three Dollars And Forty-five Cents")},
    {"WordNum(123.45,-1)",FmString("")},{"WordNum(123.45,3)",FmString("")},
    {"WordNum(-1)",FmString("*")},{"WordNum(1.234e18)",FmString("*")},
    {"HasValue(Str(null))","0"},{"HasValue(WordNum(null))","0"},
  }),u"PASS",true);
}

TEST(CoverageFormCalcBoundaries, BoundedSyntaxRejectionsAndBuiltinErrorsLeavePublicInputUsable) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const auto& program:std::vector<std::string>{
      "if(1)then root.report.rawValue=1", "do root.report.rawValue=1",
      "for i=1 upto 2 do root.report.rawValue=i", "while(0)do root.report.rawValue=1",
      "foreach n in () do root.report.rawValue=1 endfor", "root.report.rawValue=(1+)",
      "root.report.rawValue=root.item[]", "root.report.rawValue=root.item[+]",
      "func f(a) do a+1 endfunc root.report.rawValue=f(1)"}) {
    SCOPED_TRACE(program);ASSERT_NO_FATAL_FAILURE(RunBoundary(program,u"Ready",false,{u"2",u"4",u"6"},false,true));
  }
  for(const auto* call:{"Choose()","Oneof(1)","Within(1,2,3,4)","Ref()","Ref(1,2)","Ref(root.item[*].rawValue)",
      "UnitType(1,2)","UnitValue(1,2,3)","Decode()","Decode(1,2,3)","Encode()",
      "Str(42,15654909)","WordNum()","WordNum(1,2,3,4)","Num2GMTime()","IsoTime2Num(1,2)"}) {
    SCOPED_TRACE(call);
    ASSERT_NO_FATAL_FAILURE(RunBoundary("root.report.rawValue=\"before\"\n"+std::string(call)+
        "\nroot.report.rawValue=\"after\"",u"before",false,{u"2",u"4",u"6"},false,true));
  }
}

}  // namespace
}  // namespace pdfoxium::integration
