#include "xfa_form_fixture.h"
#include "pinned_test_fonts.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using forms::XfaFixture;

// Coverage baseline: XFA numeric/password/check/list/date/barcode concrete
// widgets were never constructed by the original happy-path text fixture.
// The following documents supply those actual template UI nodes, not private
// engine instances. Amount occupies layout y20..40; each extra field starts40.
std::string Field(const std::string& name,const std::string& ui,const std::string& value,
                  const std::string& properties="",int width=140,int height=24) {
  return "<field name=\""+name+"\" w=\""+std::to_string(width)+"pt\" h=\""+std::to_string(height)+
    "pt\"><bind match=\"none\"/><font typeface=\"Helvetica\" size=\"12pt\"/><ui>"+ui+
    "</ui>"+properties+"<value>"+value+"</value></field>";
}
void CompareScene(const Raster& expected,const Raster& actual,Artifacts& artifacts,const std::string& label) {
  if (!raster_comparison_settings().enabled) return;
  // Existing form scenes measured standard Helvetica coverage <=18. Retain
  // that bound here; new failures require investigation, not wider thresholds.
  CompareRaster(expected,actual,artifacts,label,RasterTolerance{18,0,0.06});
  for(int y=0;y<expected.height;y+=40) for(int x=0;x<expected.width;x+=100) {
    const int w=std::min(100,expected.width-x),h=std::min(40,expected.height-y);
    CompareRaster(CropRaster(expected,x,y,w,h),CropRaster(actual,x,y,w,h),artifacts,
      label+"-roi"+std::to_string(x)+"x"+std::to_string(y),RasterTolerance{18,0,0.3});
  }
}
void CompareFrames(const std::array<std::vector<Raster>,2>& images,Artifacts& artifacts,const std::string& label) {
  ASSERT_EQ(images[0].size(),images[1].size());
  for(size_t i=0;i<images[0].size();++i) CompareScene(images[0][i],images[1][i],artifacts,label+std::to_string(i));
}
struct Region {int x,y,w,h;};

void CompareOutside(const Raster& expected,const Raster& actual,Artifacts& artifacts,
                    const std::string& label,const std::vector<Region>& exceptions) {
  if(!raster_comparison_settings().enabled)return;
  artifacts.Image(label+"-native-original",expected);artifacts.Image(label+"-rust-original",actual);
  std::vector<int> rows{0,expected.height};
  for(const auto& r:exceptions){rows.push_back(r.y);rows.push_back(r.y+r.h);}
  std::sort(rows.begin(),rows.end());rows.erase(std::unique(rows.begin(),rows.end()),rows.end());
  for(size_t i=1;i<rows.size();++i) {
    const int top=rows[i-1],height=rows[i]-top;if(height<=0)continue;
    std::vector<std::pair<int,int>> holes;
    for(const auto& r:exceptions)if(r.y<=top&&r.y+r.h>=rows[i])holes.emplace_back(r.x,r.x+r.w);
    std::sort(holes.begin(),holes.end());holes.emplace_back(expected.width,expected.width);
    int left=0;
    for(const auto& [begin,end]:holes) {
      if(begin>left) {
        // A one-row11px strip concentrates otherwise ordinary font AA:
        // measured max6, mean0.409. Keep the per-pixel bound strict and use
        // the existing XFA widget-region mean0.5 for such narrow partitions.
        const double mean=(begin-left<32*expected.render_scale||height<4*expected.render_scale)?0.5:0.4;
        CompareRaster(CropRaster(expected,left,top,begin-left,height),
          CropRaster(actual,left,top,begin-left,height),artifacts,
          label+"-region"+std::to_string(left)+"x"+std::to_string(top),RasterTolerance{18,0,mean});
      }
      left=std::max(left,end);
    }
  }
}
void AssertInkGeometry(const Raster& a,const Raster& b,const Region& r,
                       const std::array<int,3>& native_background={255,255,255},
                       const std::array<int,3>& rust_background={255,255,255}) {
  const auto bounds=[&](const Raster& image,const std::array<int,3>& background) {
    std::array<int,5> result{r.w,r.h,-1,-1,0};
    for(int y=0;y<r.h;++y)for(int x=0;x<r.w;++x) {
      const auto i=(static_cast<size_t>(r.y+y)*image.width+r.x+x)*4;
      bool ink=false;for(size_t c=0;c<3;++c)ink|=std::abs(image.pixels[i+c]-background[c])>24;
      if(ink){result[0]=std::min(result[0],x);result[1]=std::min(result[1],y);
        result[2]=std::max(result[2],x);result[3]=std::max(result[3],y);++result[4];}
    }
    return result;
  };
  const auto first=bounds(a,native_background),second=bounds(b,rust_background);
  EXPECT_GT(first[4],10);EXPECT_GT(second[4],10);
  for(size_t c=0;c<4;++c)EXPECT_NEAR(first[c],second[c],1)<<"Element ink bounds coordinate"<<c;
}
void CompareList(const std::array<std::vector<Raster>,2>& frames,Artifacts& artifacts) {
  ASSERT_EQ(frames[0].size(),frames[1].size());
  // List items supply nominal text line boxes, not additional paint clips.
  // The list's outer content viewport clips scrolling; compare descenders
  // normally instead of masking them or requiring them to remain blank.
  for(size_t i=0;i<frames[0].size();++i)
    CompareScene(frames[0][i],frames[1][i],artifacts,"xfa-list-"+std::to_string(i));
}
void CompareDate(const std::array<std::vector<Raster>,2>& frames,Artifacts& artifacts) {
  if(!raster_comparison_settings().enabled)return;
  ASSERT_EQ(frames[0].size(),frames[1].size());
  for(size_t i=0;i<frames[0].size();++i) {
    // Native's composite paints its date edit twice. Check actual label ink
    // geometry plus exact semantic date above; compare every other pixel.
    const auto region=[&](Region r) {
      const double scale=frames[0][i].render_scale;
      const int x=static_cast<int>(std::lround(r.x*scale)),y=static_cast<int>(std::lround(r.y*scale));
      return Region{x,y,static_cast<int>(std::lround((r.x+r.w)*scale))-x,
                          static_cast<int>(std::lround((r.y+r.h)*scale))-y};
    };
    std::vector<Region> exceptions{region({0,40,140,24})};
    AssertInkGeometry(frames[0][i],frames[1][i],exceptions[0]);
    // Caption/footer rectangles are layout bounds, not glyph-ink clips.
    // Compare full descenders normally without per-line exceptions.
    if(i>=1&&i<=5) {
      // Native also carries a selected day-number into other months, while
      // Rust clears it on month change. Observe each29 cell independently;
      // foreground bounds remain comparable despite the native gray label.
      // CalcSize/CalDateItem with pinned Arimo: 25pt cell width plus 6pt
      // horizontal margins, 12pt cell height plus 4pt vertical margins.
      // The oversized popup shifts left; the February 29 cell is [101,188].
      const std::array<int,6> day29_x{0,101,8,101,132,101};
      const Region selected=region({day29_x[i],188,31,16});exceptions.push_back(selected);
      if(i==1) {
      // Native sends DateInBK to a theme that handles DatesIn, so it misses
      // the selected-day background. Existing Rust deliberately fixes this.
      AssertInkGeometry(frames[0][i],frames[1][i],selected,{255,255,255},{239,188,173});
      } else AssertInkGeometry(frames[0][i],frames[1][i],selected);
    }
    CompareOutside(frames[0][i],frames[1][i],artifacts,"date-calendar-"+std::to_string(i),exceptions);
  }
}

void Commit(Session& s) {
  EXPECT_TRUE(s.api.FORM_ForceToKillFocus(s.form));
  // Native XFA queues some display/calculation work until a real UpdateScope.
  if(s.xfa_loaded) {s.Click(5,25);s.api.FORM_ForceToKillFocus(s.form);}
}
void Replace(Session& s,const std::u16string& text) {
  s.api.FORM_SelectAllText(s.form,s.page);s.Replace(text);
  std::u16string expected;for(char16_t c:text)expected+=c==u'\r'?u"\r\n":std::u16string(1,c);
  EXPECT_EQ(s.Text(),expected);
}
void SaveAcro(const Library& writer,Session& s,Artifacts& artifacts,const std::u16string& expected,int index) {
  s.api.FORM_ForceToKillFocus(s.form);
  const auto before=s.Draw();const auto bytes=SaveDocument(s.api,s.document,artifacts,writer.name()+"-coverage-form");
  ASSERT_FALSE(bytes.empty());
  for(const auto* reader:Implementations()) {
    Session reopened(reader->api(),bytes);ASSERT_TRUE(reopened.valid());
    EXPECT_EQ(reopened.Field(index),expected);
    CompareScene(before,reopened.Draw(),artifacts,writer.name()+"-to-"+reader->name());
  }
}

TEST(CoverageForms, EditableAcroComboPopupMouseKeyboardReservedEscapeAndPersistence) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t n=0;
  for(const auto* lib:Implementations()) {
    SCOPED_TRACE(lib->name());Session s(lib->api(),forms::Fixture());ASSERT_TRUE(s.valid());const auto& api=s.api;
    ASSERT_TRUE(s.Focus(30,255));EXPECT_EQ(s.Text(),u"Green");
    frames[n].push_back(s.Draw());s.Click(172,255);frames[n].push_back(s.Draw());
    EXPECT_NE(frames[n][0].pixels,frames[n][1].pixels)<<"The Acro choice popup must actually draw";
    api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Down,0);
    api.FORM_OnChar(s.form,s.page,13,0);
    EXPECT_EQ(s.Text(),u"Blue");frames[n].push_back(s.Draw());
    s.Click(172,255);api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Up,0);
    api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Escape,0);
    EXPECT_EQ(s.Text(),u"Green")<<"Reserved Escape key-down does not reverse immediate choice navigation";
    api.FORM_OnChar(s.form,s.page,13,0);
    // Character Return toggles the popup; Up chooses the first option.
    // CPWL_ComboBox does not forward Home/End to its popup list.
    api.FORM_OnChar(s.form,s.page,13,0);api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Up,0);
    api.FORM_OnChar(s.form,s.page,13,0);Commit(s);
    EXPECT_EQ(s.Field(5),u"r");frames[n++].push_back(s.Draw());
    SaveAcro(*lib,s,artifacts,u"r",5);EXPECT_TRUE(s.host->callbacks_valid);
  }
  CompareFrames(frames,artifacts,"acro-combo-");
}

TEST(CoverageForms, MultilineNavigationSelectionDeletionHistoryAndScrollBar) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t n=0;
  std::u16string text;for(int i=0;i<35;++i) {if(i)text+=u"\r";text+=u"Line ";text+=static_cast<char16_t>('A'+i%26);}
  std::u16string canonical;for(char16_t c:text)canonical+=c==u'\r'?u"\r\n":std::u16string(1,c);
  for(const auto* lib:Implementations()) {
    SCOPED_TRACE(lib->name());Session s(lib->api(),forms::Fixture());ASSERT_TRUE(s.valid());const auto& api=s.api;
    ASSERT_TRUE(s.Focus(30,90));Replace(s,text);EXPECT_EQ(s.Text(),canonical);
    api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);frames[n].push_back(s.Draw());
    for(int i=0;i<4;++i)api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Down,FWL_EVENTFLAG_ShiftKey);
    EXPECT_EQ(s.Text(true),u"Line A\r\nLine B\r\nLine C\r\nLine D\r\n");
    api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Delete,0);EXPECT_EQ(s.Text().substr(0,6),u"Line E");
    ASSERT_TRUE(api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),canonical);
    ASSERT_TRUE(api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text().substr(0,6),u"Line E");
    ASSERT_TRUE(api.FORM_Undo(s.form,s.page));
    api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey);frames[n].push_back(s.Draw());
    EXPECT_NE(frames[n][0].pixels,frames[n][1].pixels)<<"The multiline viewport must scroll";
    // Drive scrollbar arrow, track and captured thumb movement through C API.
    for(const auto& point:std::array<std::array<double,2>,4>{{{174,110},{174,30},{174,65},{174,95}}})s.Click(point[0],point[1]);
    api.FORM_OnLButtonDown(s.form,s.page,0,174,65);
    api.FORM_OnMouseMove(s.form,s.page,FWL_EVENTFLAG_LeftButtonDown,174,90);
    api.FORM_OnLButtonUp(s.form,s.page,0,174,90);
    EXPECT_EQ(s.Text(),canonical);frames[n].push_back(s.Draw());Commit(s);frames[n++].push_back(s.Draw());
    SaveAcro(*lib,s,artifacts,canonical,8);
  }
  CompareFrames(frames,artifacts,"multiline-");
}

TEST(CoverageForms, XfaNumericPictureFormattingPasswordAndReadonlyControls) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t n=0;
  const auto numeric=Field("number","<numericEdit/>","<decimal leadDigits=\"3\" fracDigits=\"2\">12.5</decimal>",
    "<format><picture>num{zz9.99}</picture></format>");
  const auto password=Field("secret","<passwordEdit passwordChar=\"#\"/>","<text maxChars=\"8\">secret</text>");
  auto readonly=Field("locked","<textEdit/>","<text>Locked</text>");
  readonly.insert(readonly.find(' ')+1,"access=\"readOnly\" ");
  for(const auto* lib:Implementations()) {
    SCOPED_TRACE(lib->name());Session s(lib->api(),XfaFixture(true,"",numeric+password+readonly),true,true);
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);const auto& api=s.api;
    frames[n].push_back(s.Draw(200,300));ASSERT_TRUE(s.Focus(5,50));EXPECT_EQ(s.Text(),u"12.5");
    api.FORM_SelectAllText(s.form,s.page);s.Replace(u"123.45");EXPECT_EQ(s.Text(),u"123.45");
    // With no edit picture the numeric validator enforces the locale's
    // lead/fraction grammar rather than accepting arbitrary temporary text.
    api.FORM_OnChar(s.form,s.page,'x',0);EXPECT_EQ(s.Text(),u"123.45");
    Commit(s);ASSERT_TRUE(s.Focus(5,74));EXPECT_EQ(s.Text(),u"secret");Replace(s,u"hidden");
    frames[n].push_back(s.Draw(200,300));Commit(s);ASSERT_TRUE(s.Focus(5,74));EXPECT_EQ(s.Text(),u"hidden");
    Commit(s);frames[n++].push_back(s.Draw(200,300));EXPECT_TRUE(s.host->callbacks_valid);
  }
  CompareFrames(frames,artifacts,"numeric-password-");
}

TEST(CoverageForms, XfaDatePickerCalendarNavigationSelectionAndTextCommit) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t n=0;
  const auto date=Field("date","<dateTimeEdit/><picture>date{YYYY-MM-DD}</picture>","<date>2024-02-29</date>",
    "<format><picture>date{YYYY-MM-DD}</picture></format>");
  for(const auto* lib:Implementations()) {
    SCOPED_TRACE(lib->name());forms::XfaCallbacks callbacks;
    Session s(lib->api(),XfaFixture(true,"",date,false,true),true,true,2,[&](forms::FormHost& h){callbacks.Configure(h);});
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    ASSERT_TRUE(s.Focus(5,50));EXPECT_EQ(s.Text(),u"2024-02-29");frames[n].push_back(s.DrawForComparison(200,300));
    // Date picker puts its button immediately to the right of the focused edit.
    s.Click(145,50);frames[n].push_back(s.DrawForComparison(200,300));
    EXPECT_NE(frames[n][0].pixels,frames[n][1].pixels);EXPECT_GT(callbacks.page_rect_queries,0);
    // Independent selected-day color contract stays active when pairwise
    // raster comparisons are disabled for coverage collection.
    const auto& opened=frames[n][1];
    const int px=static_cast<int>(std::lround(106*opened.render_scale));
    const int py=static_cast<int>(std::lround(190*opened.render_scale));
    const size_t at=(static_cast<size_t>(py)*opened.width+px)*4;
    const std::array<uint8_t,3> selected_color=lib->name()=="native"
        ?std::array<uint8_t,3>{255,255,255}:std::array<uint8_t,3>{239,188,173};
    for(size_t c=0;c<3;++c)EXPECT_EQ(opened.pixels[at+c],selected_color[c]);
    // Popup header arrows traverse adjacent months; selected date is stable
    // until a day is chosen. With pinned Arimo the oversized popup shifts
    // left; x=.5 is inside its clipped left arrow (x=3 misses it).
    for(const auto& p:std::array<std::array<double,2>,4>{{{0.5,77},{175,77},{175,77},{0.5,77}}}) {
      s.Click(p[0],p[1]);EXPECT_EQ(s.Text(),u"2024-02-29");
      frames[n].push_back(s.DrawForComparison(200,300));
    }
    // February begins on Thursday: the first row's Tuesday cell is empty.
    // Choose Tuesday in the second row, an actual day6, after returning Feb.
    s.Click(60,146);frames[n].push_back(s.DrawForComparison(200,300));
    ASSERT_TRUE(s.Focus(5,50));
    EXPECT_EQ(s.Text(),u"2024-02-06")<<"Clicking a populated calendar day must commit its exact date";
    EXPECT_FALSE(s.Text().empty());Replace(s,u"2025-12-31");Commit(s);
    ASSERT_TRUE(s.Focus(5,50));EXPECT_EQ(s.Text(),u"2025-12-31");Commit(s);
    frames[n++].push_back(s.DrawForComparison(200,300));EXPECT_TRUE(s.host->callbacks_valid);
  }
  CompareDate(frames,artifacts);
  fonts.ExpectMapped();
}

TEST(CoverageForms, XfaCheckboxRoundRadioAndExclusiveGroupCommitChangeEvents) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t n=0;
  const auto check=Field("check","<checkButton size=\"12pt\"/>","<text>off</text>",
    "<items><text>yes</text><text>off</text><text>neutral</text></items>"+
    forms::Event("xfa.form.root.amount.rawValue=this.rawValue;","change"));
  const auto group=std::string("<exclGroup name=\"group\" layout=\"tb\" w=\"140pt\" h=\"48pt\">")+
    Field("first","<checkButton shape=\"round\" size=\"12pt\"/>","<text>off</text>","<items><text>A</text><text>off</text></items>")+
    Field("second","<checkButton shape=\"round\" size=\"12pt\"/>","<text>off</text>","<items><text>B</text><text>off</text></items>")+
    forms::Event("xfa.form.root.amount.rawValue=this.rawValue;","change")+"</exclGroup>";
  for(const auto* lib:Implementations()) {
    SCOPED_TRACE(lib->name());Session s(lib->api(),XfaFixture(true,"",check+group),true,true);
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);const auto& api=s.api;
    EXPECT_EQ(api.FPDFPage_HasFormFieldAtPoint(s.form,s.page,5,50),FPDF_FORMFIELD_XFA_CHECKBOX);
    // Without a caption, source PerformLayout aligns the 12pt check UI
    // to the right of its 140pt field; field hit-test alone is insufficient.
    api.FORM_ForceToKillFocus(s.form);frames[n].push_back(s.Draw(200,300));s.Click(134,50);s.Click(5,25);EXPECT_EQ(s.Text(),u"yes");
    // Without a caption, source PerformLayout aligns the 12pt check UI
    // to the right of its 140pt field; field hit-test alone is insufficient.
    api.FORM_ForceToKillFocus(s.form);frames[n].push_back(s.Draw(200,300));s.Click(134,50);s.Click(5,25);EXPECT_EQ(s.Text(),u"off");
    s.Click(134,74);s.Click(5,25);EXPECT_EQ(s.Text(),u"A");api.FORM_ForceToKillFocus(s.form);frames[n].push_back(s.Draw(200,300));
    s.Click(134,98);s.Click(5,25);EXPECT_EQ(s.Text(),u"B");api.FORM_ForceToKillFocus(s.form);frames[n].push_back(s.Draw(200,300));
    api.FORM_ForceToKillFocus(s.form);frames[n++].push_back(s.Draw(200,300));
    EXPECT_TRUE(s.host->callbacks_valid);
  }
  CompareFrames(frames,artifacts,"check-radio-");
}

TEST(CoverageForms, XfaLongListMultiSelectionWheelAndScrollbarAreVisible) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t n=0;
  std::string labels="<items>",values="<items save=\"1\">";
  for(int i=0;i<24;++i) {labels+="<text>Option "+std::to_string(i)+"</text>";values+="<text>v"+std::to_string(i)+"</text>";}
  labels+="</items>";values+="</items>";
  const auto list=Field("list","<choiceList open=\"multiSelect\"/>","<text>v0</text>",labels+values+
    forms::Event("xfa.form.root.amount.rawValue=this.rawValue;","exit"),140,120);
  for(const auto* lib:Implementations()) {
    SCOPED_TRACE(lib->name());Session s(lib->api(),XfaFixture(true,"",list),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    const auto& api=s.api;EXPECT_EQ(api.FPDFPage_HasFormFieldAtPoint(s.form,s.page,5,50),FPDF_FORMFIELD_XFA_LISTBOX);
    frames[n].push_back(s.Draw(200,300));s.Click(15,60);
    api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Down,FWL_EVENTFLAG_ShiftKey);
    api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Down,FWL_EVENTFLAG_ShiftKey);frames[n].push_back(s.Draw(200,300));
    const FS_POINTF point{30,80};for(int delta:{-120,-120,120,0}) {
      api.FORM_OnMouseWheel(s.form,s.page,0,&point,0,delta);frames[n].push_back(s.Draw(200,300));
    }
    for(const auto& p:std::array<std::array<double,2>,4>{{{133,150},{133,50},{133,100},{133,130}}})s.Click(p[0],p[1]);
    api.FORM_OnLButtonDown(s.form,s.page,0,133,90);
    api.FORM_OnMouseMove(s.form,s.page,FWL_EVENTFLAG_LeftButtonDown,133,120);
    api.FORM_OnLButtonUp(s.form,s.page,0,133,120);frames[n].push_back(s.Draw(200,300));
    s.Click(5,25);EXPECT_FALSE(s.Text().empty());api.FORM_ForceToKillFocus(s.form);frames[n++].push_back(s.Draw(200,300));
    EXPECT_TRUE(s.host->callbacks_valid);
  }
  CompareList(frames,artifacts);
}

TEST(CoverageForms, EverySupportedXfaBarcodeEncodesActualDarkModules) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  // Names are from CXFA_FFBarcode's supported table. Unsupported symbologies
  // deliberately map Unknown and cannot exercise an encoder through XFA.
  const std::vector<std::pair<std::string,std::string>> cases={
    {"ean13","5901234123457"},{"ean8","12345670"},{"upcA","042100005264"},
    {"codabar","123456"},{"code128","Abc123"},{"code128B","Hello123"},
    {"code128C","12345678"},{"code3Of9","ABC123"},{"QRCode","Rust PDF 123"},
    {"dataMatrix","Rust PDF 123"},{"pdf417","Rust PDF 123"},
    // Reach content-dependent encoder modes, compaction and code-set switches;
    // one sample per symbology does not exercise these algorithms.
    {"QRCode","0123456789012345678901234567890123456789"},
    {"QRCode","HELLO WORLD 12345"},
    {"QRCode","lowercase byte content: /?=123 ok=true"},
    {"QRCode","ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"},
    {"dataMatrix","012345678901234567890123456789"},
    {"dataMatrix","ABC>ABC>ABC>ABC>ABC>ABC>"},
    {"dataMatrix","abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz"},
    {"dataMatrix","ABCD1234EFGH5678IJKL9012MNOP3456"},
    {"pdf417","012345678901234567890123456789012345678901234567890123456789"},
    {"pdf417","ABC xyz !@#$%^*() mixed case 12345678901234567890 end"},
    {"code128","123456AB12345678cd"},
    {"code128B","~!abc-123"},
    {"code3Of9","A-$%+./ 123"}};
  for(const auto& [kind,value]:cases) {
    SCOPED_TRACE(kind);
    SCOPED_TRACE(value);
    std::array<Raster,2> images;size_t n=0;
    const auto barcode=Field("barcode","<barcode type=\""+kind+"\" textLocation=\"none\" moduleWidth=\"1pt\" moduleHeight=\"1pt\"/>",
      "<text>"+value+"</text>","",180,120);
    for(const auto* lib:Implementations()) {
      SCOPED_TRACE(lib->name());Session s(lib->api(),XfaFixture(true,"",barcode),true,true);
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);images[n]=s.Draw(200,300);
      const auto modules=CropRaster(images[n],0,40,180,120);size_t dark=0,light=0;
      for(size_t i=0;i<modules.pixels.size();i+=4) {
        dark+=modules.pixels[i]<64&&modules.pixels[i+1]<64&&modules.pixels[i+2]<64;
        light+=modules.pixels[i]>240&&modules.pixels[i+1]>240&&modules.pixels[i+2]>240;
      }
      EXPECT_GT(dark,100u)<<"A text-only or blank barcode must not pass";EXPECT_GT(light,100u);
      ++n;
    }
    // No human-readable text: barcode geometry itself should match exactly.
    CompareRaster(CropRaster(images[0],0,40,180,120),CropRaster(images[1],0,40,180,120),artifacts,"barcode-"+kind);
  }
}

TEST(CoverageForms, XfaPrimitiveDrawsRichTextCaptionsAndSignatureRender) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;std::array<Raster,2> images;size_t n=0;
  const std::string extra=R"(<draw w="160pt" h="24pt"><value><line slope="/"/></value></draw>
    <draw w="100pt" h="36pt"><value><arc circular="1"><edge thickness="2pt"><color value="0,0,255"/></edge><fill><color value="0,255,0"/></fill></arc></value></draw>
    <draw w="180pt" h="48pt"><value><exData contentType="text/html"><body xmlns="http://www.w3.org/1999/xhtml"><p><b>Bold</b> <i>Italic</i><br/>Second line</p></body></exData></value><font typeface="Helvetica" size="12pt"/></draw>
    <field name="signature" w="160pt" h="30pt"><ui><signature/></ui><caption placement="left" reserve="60pt"><value><text>Sign here</text></value></caption><border><edge thickness="1pt"/></border></field>)";
  for(const auto* lib:Implementations()) {
    SCOPED_TRACE(lib->name());Session s(lib->api(),XfaFixture(true,"",extra),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    images[n]=s.Draw(200,300);forms::ExpectInk(images[n],0,40,160,64);
    forms::ExpectInk(images[n],0,100,180,148);
    const auto arc=CropRaster(images[n],0,64,100,36);size_t green=0,blue=0;
    for(size_t i=0;i<arc.pixels.size();i+=4) {
      green+=arc.pixels[i+1]>200&&arc.pixels[i]<40&&arc.pixels[i+2]<40;
      blue+=arc.pixels[i]>200&&arc.pixels[i+1]<40&&arc.pixels[i+2]<40;
    }
    EXPECT_GT(green,100u);EXPECT_GT(blue,20u);
    EXPECT_EQ(s.api.FPDFPage_HasFormFieldAtPoint(s.form,s.page,100,160),FPDF_FORMFIELD_XFA_SIGNATURE);
    s.Click(100,160);EXPECT_TRUE(s.host->callbacks_valid);++n;
  }
  CompareScene(images[0],images[1],artifacts,"draws-rich-signature");
}


TEST(CoverageForms, XfaInlineBmpGifPngAndTiffStaticAndEditableImagesSaveReload) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  // Preserve the tiny pinned decoder fixtures verbatim. Their image nodes are
  // embedded through XFA's public PDF packet rather than decoded by the host.
  for(const std::string name:{"xfa_bmp_image","xfa_gif_image","xfa_png_image",
                              "xfa_tiff_image","xfa_tiff_lzw_image","xfa_tiff_packbits_image"}) {
    SCOPED_TRACE(name);
    std::ifstream input(TestRoot()/"upstream/pdfium/testing/resources/pixel/xfa_specific"/(name+".in"),std::ios::binary);
    ASSERT_TRUE(input.good());const std::string source((std::istreambuf_iterator<char>(input)),{});
    const auto begin=source.find("<image "),end=source.find("</image>",begin);
    ASSERT_NE(begin,std::string::npos);ASSERT_NE(end,std::string::npos);
    const auto image=source.substr(begin,end+8-begin);
    const auto extra="<draw w=\"80pt\" h=\"80pt\"><value>"+image+"</value></draw>"+
      Field("picture","<imageEdit/>",image,"",80,80);
    std::array<Raster,2> scenes;size_t n=0;
    for(const auto* writer:Implementations()) {
      SCOPED_TRACE(writer->name());Session s(writer->api(),XfaFixture(true,"",extra),true,true);
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);scenes[n]=s.Draw(200,300);
      EXPECT_EQ(s.api.FPDFPage_HasFormFieldAtPoint(s.form,s.page,10,130),FPDF_FORMFIELD_XFA_IMAGEFIELD);
      for(int top:{40,120}) {
        const auto region=CropRaster(scenes[n],0,top,80,80);size_t colored=0;
        for(size_t i=0;i<region.pixels.size();i+=4)
          colored+=std::max({region.pixels[i],region.pixels[i+1],region.pixels[i+2]})-
                   std::min({region.pixels[i],region.pixels[i+1],region.pixels[i+2]})>80;
        EXPECT_GT(colored,6000u)<<"The pinned image is entirely red, so nearly the whole field must draw";
        const auto center=(static_cast<size_t>(40)*80+40)*4;
        EXPECT_EQ(region.pixels[center],0);EXPECT_EQ(region.pixels[center+1],0);EXPECT_EQ(region.pixels[center+2],255);
      }
      const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-"+name);
      ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        Session reopened(reader->api(),saved,true,true);ASSERT_TRUE(reopened.valid());ASSERT_TRUE(reopened.xfa_loaded);
        const auto after=reopened.Draw(200,300);
        CompareRaster(CropRaster(scenes[n],0,40,80,160),CropRaster(after,0,40,80,160),artifacts,
          writer->name()+"-"+name+"-to-"+reader->name());
      }
      ++n;
    }
    CompareRaster(CropRaster(scenes[0],0,40,80,160),CropRaster(scenes[1],0,40,80,160),artifacts,name+"-decoded-images");
  }
}

}  // namespace
}  // namespace pdfoxium::integration
