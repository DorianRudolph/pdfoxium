#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;

// The packet is a real custom locale, not a built-in locale alias. Distinct
// names/symbols make fallback to en_US immediately visible in the output.
std::string CustomLocale() {
  std::string calendar="<calendarSymbols name=\"gregorian\">";
  for(bool abbreviated:{false,true}) {
    calendar+="<monthNames abbr=\""+std::string(abbreviated?"1":"0")+"\">";
    for(int i=1;i<=12;++i)calendar+="<month>"+std::string(abbreviated?"M":"Month")+(i<10?"0":"")+std::to_string(i)+"</month>";
    calendar+="</monthNames><dayNames abbr=\""+std::string(abbreviated?"1":"0")+"\">";
    for(int i=0;i<7;++i)calendar+="<day>"+std::string(abbreviated?"D":"Day")+std::to_string(i)+"</day>";
    calendar+="</dayNames>";
  }
  calendar+="<meridiemNames><meridiem>amx</meridiem><meridiem>pmx</meridiem></meridiemNames>"
    "<eraNames><era>old</era><era>new</era></eraNames></calendarSymbols>";
  return "<localeSet xmlns=\"http://www.xfa.org/schema/xfa-locale-set/2.7/\"><locale name=\"zz_ZZ\">"+calendar+
    "<datePatterns><datePattern name=\"short\">YYYY/MM/DD</datePattern><datePattern name=\"med\">DD-MMM-YYYY</datePattern>"
    "<datePattern name=\"long\">D MMMM YYYY</datePattern><datePattern name=\"full\">EEEE D MMMM YYYY</datePattern></datePatterns>"
    "<timePatterns><timePattern name=\"short\">h:MM A</timePattern><timePattern name=\"med\">h:MM:SS A</timePattern>"
    "<timePattern name=\"long\">HH:MM:SS Z</timePattern><timePattern name=\"full\">HH:MM:SS Z</timePattern></timePatterns>"
    "<dateTimeSymbols>GyMdkHmsSEDFwWahKzZ</dateTimeSymbols>"
    "<numberSymbols><numberSymbol name=\"decimal\">,</numberSymbol><numberSymbol name=\"grouping\">'</numberSymbol>"
    "<numberSymbol name=\"percent\">%</numberSymbol><numberSymbol name=\"minus\">~</numberSymbol><numberSymbol name=\"zero\">0</numberSymbol></numberSymbols>"
    "<currencySymbols><currencySymbol name=\"symbol\">@</currencySymbol><currencySymbol name=\"isoname\">ZZZ</currencySymbol>"
    "<currencySymbol name=\"decimal\">,</currencySymbol></currencySymbols></locale></localeSet>";
}
std::string ReportField(const std::string& event) {
  return "<field name=\"report\" w=\"200pt\" h=\"32pt\"><ui><textEdit/></ui>"
    "<font typeface=\"Courier\" size=\"8pt\"/><bind match=\"once\"/><value><text/></value>"+event+"</field>";
}
std::vector<uint8_t> LocaleFixture(const std::string& event,const std::string& before="",bool custom=true) {
  const std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"tb\" locale=\""+
    std::string(custom?"zz_ZZ":"en_US")+"\" restoreState=\"auto\"><pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/>"
    "<medium short=\"240pt\" long=\"240pt\"/><contentArea w=\"220pt\" h=\"220pt\"/></pageArea></pageSet>"+before+ReportField(event)+"</subform></template>";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 240] /Resources << /Font << /Courier 11 0 R >> >> >>",
    "<< /Fields [] /DR << /Font << /Courier 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (localeSet) 10 0 R (postamble) 9 0 R] >>",
    PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
    PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
    PdfStream("",xml),
    PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root><report/></root></xfa:data></xfa:datasets>"),
    PdfStream("","</xdp:xdp>"),PdfStream("",CustomLocale()),
    "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>"});
}
void CompareLocale(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,RasterTolerance{24,0,0.5});
  CompareRaster(CropRaster(a,0,0,200,32),CropRaster(b,0,0,200,32),artifacts,label+"-report",RasterTolerance{24,0,0.5});
}
void ExpectReport(Session& s,const std::u16string& expected) {
  ASSERT_TRUE(s.Focus(4,6));EXPECT_EQ(s.Text(),expected);s.api.FORM_ForceToKillFocus(s.form);
}
void ExpectUuidVariants(const Library& owner,const Session& s) {
  ASSERT_FALSE(s.host->alerts.empty());
  const auto& message=std::get<0>(s.host->alerts.back());
  size_t start=0;int count=0;
  while(start<message.size()) {
    const auto end=message.find(u'|',start);
    auto uuid=message.substr(start,end==std::u16string::npos?end:end-start);
    uuid.erase(std::remove(uuid.begin(),uuid.end(),u'-'),uuid.end());
    ASSERT_EQ(uuid.size(),32u);
    if(owner.name()=="rust") {
      const auto variant=uuid[16];
      EXPECT_TRUE(variant==u'8'||variant==u'9'||variant==u'a'||variant==u'b'||variant==u'A'||variant==u'B');
    }
    ++count;
    if(end==std::u16string::npos)break;
    start=end+1;
  }
  EXPECT_EQ(count,4);
}
void ReplayLocale(const Library& writer,Session& s,const Raster& original,const std::u16string& expected,Artifacts& artifacts,const std::string& label,bool check_uuids=false) {
  const auto bytes=SaveDocument(s.api,s.document,artifacts,writer.name()+"-"+label);ASSERT_FALSE(bytes.empty());
  for(const auto* reader:Implementations()) {
    Session loaded(reader->api(),bytes,true,true);ASSERT_TRUE(loaded.valid());ASSERT_TRUE(loaded.xfa_loaded);
    // The initialize event recomputes the expected value through the loaded
    // custom locale, rather than comparing serialized dataset whitespace.
    ExpectReport(loaded,expected);
    if(check_uuids)ExpectUuidVariants(*reader,loaded);
    CompareLocale(original,loaded.Draw(240,240),artifacts,writer.name()+"-"+reader->name()+"-"+label);
  }
}
struct LocaleCase {const char* expression;const char16_t* expected;bool native_group_quote_bug=false;};
void LocaleCases(const std::vector<LocaleCase>& rows,const std::string& label) {
  Artifacts artifacts;size_t index=0;
  for(const auto& row:rows) {
    SCOPED_TRACE(row.expression);std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());
      const auto event=forms::Event("$.rawValue="+std::string(row.expression),"initialize","application/x-formcalc");
      Session s(library->api(),LocaleFixture(event),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      const bool native_empty=library->name()=="native"&&row.native_group_quote_bug;
      ExpectReport(s,native_empty?u"":row.expected);
      if(native_empty) {
        // Native interprets the actual input grouping apostrophe as a picture
        // quote and misses the decimal. Assert its empty original, then use
        // public input to compare the intended canonical value's full paint.
        const auto original=s.Draw(240,240);artifacts.Image("native-original-apostrophe-group",original);
        for(int y=0;y<32;++y)for(int x=0;x<200;++x) {
          const auto p=(size_t(y)*original.width+x)*4;
          EXPECT_EQ(original.pixels[p],255);EXPECT_EQ(original.pixels[p+1],255);EXPECT_EQ(original.pixels[p+2],255);
        }
        ASSERT_TRUE(s.Focus(4,6));s.Replace(row.expected);EXPECT_EQ(s.Text(),row.expected);s.api.FORM_ForceToKillFocus(s.form);
      }
      images[backend]=s.Draw(240,240);forms::ExpectInk(images[backend],0,0,200,32);
      if(index==0||index+1==rows.size()) {
        ReplayLocale(*library,s,images[backend],row.expected,artifacts,label+std::to_string(index));
      }
      ++backend;
    }
    CompareLocale(images[0],images[1],artifacts,label+std::to_string(index++));
  }
}

TEST(CoverageLocaleService, CustomNumberSymbolsFormatAndParseCanonicalValuesThroughActualDocumentLocale) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  LocaleCases({
    {"Format(\"num{9.99}\",7.5)",u"7,50"},
    {"Format(\"num{z,zz9.99}\",1234.5)",u"1'234,50"},
    {"Format(\"num{9.99}\",-7.5)",u"~7,50"},
    {"Format(\"num{$9.99}\",7.5)",u"@7,50"},
    {"Format(\"num{zz9.9%}\",0.125)",u"12,5%"},
    {"Parse(\"num{9.99}\",\"7,50\")",u"7.50"},
    {"Parse(\"num{z,zz9.99}\",\"1'234,50\")",u"1234.50",true},
    {"Parse(\"num{S9.99}\",\"~7,50\")",u"-7.50"},
    {"Parse(\"num{$9.99}\",\"@7,50\")",u"7.50"},
    // Native percent arithmetic and Round both retain binary float tails.
    // Format the parsed value at the asserted precision through this locale.
    {"Format(\"num{9.999}\",Parse(\"num{zz9.9%}\",\"12,5%\"))",u"0,125"},
    {"Format(\"num{9.99}\",Parse(\"num{9.99}\",\"8,25\"))",u"8,25"}},"number-locale-");
}

TEST(CoverageLocaleService, CustomCalendarNamesAbbreviationsErasAndPatternsRoundTripKnownDates) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  LocaleCases({
    {"Num2Date(1,\"DD MMM YYYY\",\"zz_ZZ\")",u"01 M01 1900"},
    {"Num2Date(1,\"DD MMMM YYYY\",\"zz_ZZ\")",u"01 Month01 1900"},
    {"Num2Date(1,\"EEEE\",\"zz_ZZ\")",u"Day1"},
    {"Num2Date(1,\"EEE\",\"zz_ZZ\")",u"D1"},
    {"Date2Num(\"01 M01 1900\",\"DD MMM YYYY\",\"zz_ZZ\")",u"1"},
    {"Date2Num(\"01 Month01 1900\",\"DD MMMM YYYY\",\"zz_ZZ\")",u"1"},
    {"DateFmt(1,\"zz_ZZ\")",u"YYYY/MM/DD"},
    {"DateFmt(2,\"zz_ZZ\")",u"DD-MMM-YYYY"},
    {"DateFmt(3,\"zz_ZZ\")",u"D MMMM YYYY"},
    {"DateFmt(4,\"zz_ZZ\")",u"EEEE D MMMM YYYY"},
    {"TimeFmt(1,\"zz_ZZ\")",u"h:MM A"},
    {"TimeFmt(2,\"zz_ZZ\")",u"h:MM:SS A"},
    {"TimeFmt(3,\"zz_ZZ\")",u"HH:MM:SS Z"},
    {"Format(\"date{G YYYY}\",\"1900-01-01\")",u"new 1900"},
    {"Format(\"time{h:MM A}\",\"13:14:00\")",u"1:14 pmx"},
    {"Format(\"time{h:MM A}\",\"01:14:00\")",u"1:14 amx"}},"calendar-locale-");
}

TEST(CoverageLocaleService, LiveDateTimeAndUuidServicesAgreeWithIntrinsicDatesAndDocumentedShapes) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto hidden=[](const char* name,const char* expression) {
      return "<field name=\""+std::string(name)+"\" presence=\"hidden\"><value><text/></value>"+
        forms::Event("$.rawValue="+std::string(expression),"initialize","application/x-formcalc")+"</field>";
    };
    const auto before=hidden("day","Date()")+hidden("clock","Time()")+hidden("uuid0","Uuid()")+
      hidden("uuid1","Uuid(1)")+hidden("uuid2","Uuid(0)")+hidden("uuid3","Uuid(null)");
    const std::string script=R"(
var r=xfa.form.root, d=new Date();
var day=Number(r.day.rawValue), t=Number(r.clock.rawValue);
var midnight=Date.UTC(d.getUTCFullYear(),d.getUTCMonth(),d.getUTCDate());
var serial=Math.floor((midnight-Date.UTC(1900,0,1))/86400000)+1;
var now=(d.getUTCHours()*3600+d.getUTCMinutes()*60+d.getUTCSeconds())*1000;
var distance=Math.abs(now-t); distance=Math.min(distance,86400000-distance);
var a=String(r.uuid0.rawValue),b=String(r.uuid1.rawValue),c=String(r.uuid2.rawValue),e=String(r.uuid3.rawValue);
var hex=/^[0-9a-f]{32}$/i, hyphen=/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
var ok=Math.abs(day-serial)<=1 && t>=0 && t<86400000 && t%1000===0 && distance<=10000;
ok=ok&&hex.test(a)&&hex.test(c)&&hex.test(e)&&hyphen.test(b)&&a.charAt(12)==='4'&&c.charAt(12)==='4'&&e.charAt(12)==='4';
ok=ok&&a!==c&&a!==e&&c!==e&&b.replace(/-/g,'')!==a;
)";
    // Source GUIDString sets the version but omits the RFC variant bits. Rust
    // corrects that; require its intended variant without imposing the native bug.
    // Capture actual values and assert variant bits for the executing owner;
    // keep the persisted script neutral when the other library reloads it.
    // MessageBox is intentionally suppressed during initialization. Capture
    // the actual initialized UUIDs on a real focus/enter event after loading.
    const auto capture=forms::Event("var r=xfa.form.root;xfa.host.messageBox(String(r.uuid0.rawValue)+'|'+String(r.uuid1.rawValue)+'|'+String(r.uuid2.rawValue)+'|'+String(r.uuid3.rawValue));","enter");
    Session s(library->api(),LocaleFixture(forms::Event(script+"this.rawValue=ok?'PASS':'FAIL';","initialize")+capture,before,false),true,true);
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);ExpectReport(s,u"PASS");images[backend]=s.Draw(240,240);forms::ExpectInk(images[backend],0,0,200,32);
    ExpectUuidVariants(*library,s);
    ReplayLocale(*library,s,images[backend],u"PASS",artifacts,"live-services",true);++backend;
  }
  CompareLocale(images[0],images[1],artifacts,"live-services");
}

TEST(CoverageLocaleService, ServiceArityErrorsStopTheirEventAndAllowSubsequentPublicEditingAndSaving) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const char* invalid[]={"Date(1)","Time(1)","Uuid(1,2)","Date(1,2)","Time(null)"};
  Artifacts artifacts;int index=0;
  for(const char* call:invalid){SCOPED_TRACE(call);std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      const auto event=forms::Event("$.rawValue=\"before\"\n"+std::string(call)+"\n$.rawValue=\"unreachable\"","initialize","application/x-formcalc");
      Session s(library->api(),LocaleFixture(event,"",false),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);ExpectReport(s,u"before");
      ASSERT_TRUE(s.Focus(4,6));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"usable");EXPECT_EQ(s.Text(),u"usable");
      s.api.FORM_ForceToKillFocus(s.form);images[backend]=s.Draw(240,240);forms::ExpectInk(images[backend],0,0,200,32);
      const auto bytes=SaveDocument(s.api,s.document,artifacts,library->name()+"-recovered-service-"+std::to_string(index));ASSERT_FALSE(bytes.empty());
      ++backend;
    }CompareLocale(images[0],images[1],artifacts,"service-error-"+std::to_string(index++));
  }
}
}  // namespace
}  // namespace pdfoxium::integration
