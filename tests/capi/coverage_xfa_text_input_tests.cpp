#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

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

std::string ReadTextInputResource(const std::string& name) {
  std::ifstream stream(TestRoot() / "upstream/pdfium/testing/resources" / name,
                       std::ios::binary);
  EXPECT_TRUE(stream.good()) << name;
  return {std::istreambuf_iterator<char>(stream), std::istreambuf_iterator<char>()};
}

std::string XmlText(const std::u16string& text) {
  std::string result;
  for (size_t i = 0; i < text.size(); ++i) {
    uint32_t code = text[i];
    if (code >= 0xd800 && code <= 0xdbff && i + 1 < text.size() &&
        text[i + 1] >= 0xdc00 && text[i + 1] <= 0xdfff) {
      code = 0x10000 + ((code - 0xd800) << 10) + text[++i] - 0xdc00;
    }
    result += "&#" + std::to_string(code) + ";";
  }
  return result;
}

struct InputSpec {
  std::u16string value;
  std::string ui = "<textEdit/>";
  std::string properties;
  std::string events;
  std::string family = "Courier";
  int width = 180;
  int height = 36;
  int font_size = 12;
  std::string value_attributes;
  std::string field_attributes;
  std::u16string control;
  std::string rich;
};

// XFA has no public template/font-resource constructors. These minimal packets
// supply real bound controls; all editing, layout, rendering and serialization
// go through the public API. Embedded Ahem and Roboto keep metrics reproducible.
std::vector<uint8_t> TextInputFixture(const InputSpec& spec) {
  static const auto ahem = ReadTextInputResource("fonts/ahem/Ahem.ttf");
  static const auto roboto = ReadTextInputResource("fonts/roboto.ttf");
  static const auto hebrew = ReadTextInputResource("hebrew_truetype_font.fragment");
  static const auto hebrew_cmap = ReadTextInputResource("hebrew_cmap.fragment");
  const std::string fonts = "/Courier 5 0 R /Ahem 6 0 R /Roboto 9 0 R /NotoSansHebrew 17 0 R";
  const auto field = [](const std::string& name, int width, int height,
                        const std::string& family, const std::string& ui,
                        const std::string& properties, const std::string& attributes, int size=12) {
    return "<field name=\"" + name + "\" w=\"" + std::to_string(width) +
        "pt\" h=\"" + std::to_string(height) + "pt\"><bind match=\"once\"/>"
        "<font typeface=\"" + family + "\" size=\""+std::to_string(size)+"pt\"/><ui>" + ui + "</ui>" +
        properties + "<value><text " + attributes + "/></value></field>";
  };
  std::string input = field("input", spec.width, spec.height, spec.family,
                             spec.ui, spec.properties, spec.value_attributes,spec.font_size);
  if(!spec.field_attributes.empty())input.insert(input.find(' ')+1,spec.field_attributes+" ");
  input.insert(input.size() - 8, spec.events);
  const std::string body =
      "<draw w=\"20pt\" h=\"10pt\"><value><rectangle><fill><color value=\"255,0,0\"/>"
      "</fill><edge presence=\"hidden\"/></rectangle></value></draw>" + input +
      field("audit", 220, 20, "Courier", "<textEdit/>", "", "") +
      field("control", spec.width, 20, spec.family, "<textEdit/>", "", "",spec.font_size) +
      (spec.rich.empty() ? "" :
       "<draw name=\"rich\" w=\"220pt\" h=\"80pt\"><font typeface=\"Courier\" size=\"14pt\"/>"
       "<value><exData contentType=\"text/html\"><body xmlns=\"http://www.w3.org/1999/xhtml\">" +
       spec.rich + "</body></exData></value></draw>");
  const std::string xml =
      "<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" "
      "layout=\"tb\" locale=\"en_US\" restoreState=\"auto\"><pageSet><pageArea name=\"page\">"
      "<occur min=\"1\" max=\"-1\"/><medium short=\"240pt\" long=\"320pt\"/>"
      "<contentArea w=\"240pt\" h=\"320pt\"/></pageArea></pageSet>" + body + "</subform></template>";
  std::string widths, roboto_widths;
  for (int i = 0; i < 256; ++i) { widths += "1000 "; roboto_widths += "600 "; }
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 320] /Resources << /Font << " + fonts + " >> >> >>",
      "<< /Fields [] /DA (/Courier 12 Tf 0 g) /DR << /Font << " + fonts +
          " >> >> /XFA [(preamble) 12 0 R (config) 13 0 R (template) 14 0 R (datasets) 15 0 R (postamble) 16 0 R] >>",
      "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>",
      "<< /Type /Font /Subtype /TrueType /BaseFont /Ahem /FirstChar 0 /LastChar 255 /Widths [" +
          widths + "] /Encoding /WinAnsiEncoding /FontDescriptor 7 0 R >>",
      "<< /Type /FontDescriptor /FontName /Ahem /Flags 32 /FontBBox [0 -200 1000 800] "
          "/Ascent 800 /Descent -200 /CapHeight 800 /ItalicAngle 0 /StemV 80 /FontFile2 8 0 R >>",
      PdfStream("/Length1 " + std::to_string(ahem.size()), ahem),
      "<< /Type /Font /Subtype /TrueType /BaseFont /Roboto /FirstChar 0 /LastChar 255 /Widths [" +
          roboto_widths + "] /FontDescriptor 10 0 R /Encoding /WinAnsiEncoding >>",
      "<< /Type /FontDescriptor /FontName /Roboto /Flags 32 /FontBBox [-1000 -1000 3000 3000] "
          "/Ascent 1000 /Descent -300 /CapHeight 700 /ItalicAngle 0 /StemV 80 /FontFile2 11 0 R >>",
      PdfStream("/Length1 " + std::to_string(roboto.size()), roboto),
      PdfStream("", "<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("", "<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required"
          "</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("", xml),
      PdfStream("", "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root><input>" +
          XmlText(spec.value) + "</input><audit>0</audit><control>" + XmlText(spec.control) +
          "</control></root></xfa:data></xfa:datasets>"),
      PdfStream("", "</xdp:xdp>"),
      "<< /Type /Font /Subtype /TrueType /BaseFont /BAAAAA+NotoSansHebrew-Regular /FirstChar 0 /LastChar 5 "
          "/Widths [600 294 235 645 397 542] /FontDescriptor 18 0 R /ToUnicode 20 0 R >>",
      "<< /Type /FontDescriptor /FontName /BAAAAA+NotoSansHebrew-Regular /Flags 4 /FontBBox [-210 -252 716 869] "
          "/Ascent 1069 /Descent -293 /CapHeight 869 /ItalicAngle 0 /StemV 80 /FontFile2 19 0 R >>",
      PdfStream("/Filter [/ASCII85Decode /FlateDecode] /Length1 6721", hebrew),
      PdfStream("", hebrew_cmap)});
}

bool FocusInput(Session& s) { return s.Focus(4, 16); }
void Key(Session& s, int key, int flags = 0) {
  EXPECT_TRUE(s.api.FORM_OnKeyDown(s.form, s.page, key, flags));
}
void Put(Session& s, const std::u16string& value) {
  ASSERT_TRUE(FocusInput(s));
  if(!s.Text().empty()) { ASSERT_TRUE(s.api.FORM_SelectAllText(s.form, s.page)); }
  s.Replace(value);
}
void FinishInput(Session& s) {
  s.api.FORM_ForceToKillFocus(s.form);
  s.Click(230, 310);
  s.api.FORM_ForceToKillFocus(s.form);
}
Raster InputImage(Session& s, const InputSpec& spec) {
  return CropRaster(s.Draw(240, 320), 0, 10, spec.width, spec.height);
}
void CompareTextInput(const Raster& a, const Raster& b, Artifacts& artifacts,
                      const std::string& label) {
  // Use the shared stroke-tolerant metric for both colored and black text.
  CompareRaster(a, b, artifacts, label, RasterTolerance{40, 0, 0.5});
}
struct InkBounds { int left = 10000, top = 10000, right = -1, bottom = -1; size_t count = 0; };
InkBounds TextInk(const Raster& image) {
  InkBounds result;
  for (int y = 0; y < image.height; ++y) for (int x = 0; x < image.width; ++x) {
    const auto at = (static_cast<size_t>(y) * image.width + x) * 4;
    if (image.pixels[at] + image.pixels[at + 1] + image.pixels[at + 2] >= 720) continue;
    result.left = std::min(result.left, x); result.right = std::max(result.right, x);
    result.top = std::min(result.top, y); result.bottom = std::max(result.bottom, y); ++result.count;
  }
  return result;
}
InkBounds ColorInk(const Raster& image, bool red) {
  InkBounds result;
  for(int y=0;y<image.height;++y)for(int x=0;x<image.width;++x) {
    const auto at=(static_cast<size_t>(y)*image.width+x)*4;
    const int main=image.pixels[at+(red?2:0)], other=image.pixels[at+(red?0:2)];
    if(main-other<16||main-int(image.pixels[at+1])<16)continue;
    result.left=std::min(result.left,x);result.right=std::max(result.right,x);
    result.top=std::min(result.top,y);result.bottom=std::max(result.bottom,y);++result.count;
  }
  return result;
}
int LongestDarkRun(const Raster& image) {
  int longest=0;
  for(int y=0;y<image.height;++y) {
    int run=0;
    for(int x=0;x<image.width;++x) {
      const auto at=(static_cast<size_t>(y)*image.width+x)*4;
      run=image.pixels[at]+image.pixels[at+1]+image.pixels[at+2]<600?run+1:0;
      longest=std::max(longest,run);
    }
  }
  return longest;
}
std::vector<std::array<int,2>> InkColumns(const Raster& image) {
  std::vector<std::array<int,2>> result;
  for(int x=0;x<image.width;++x) {
    bool ink=false;
    for(int y=0;y<image.height&&!ink;++y) {
      const auto at=(static_cast<size_t>(y)*image.width+x)*4;
      ink=image.pixels[at]+image.pixels[at+1]+image.pixels[at+2]<720;
    }
    if(!ink)continue;
    if(!result.empty()&&result.back()[1]+1==x)result.back()[1]=x;
    else result.push_back({x,x});
  }
  return result;
}
void CheckTabColumns(const Raster& image,bool native) {
  const auto columns=InkColumns(image);ASSERT_EQ(columns.size(),3u);
  EXPECT_GE(columns[0][0],0);EXPECT_LE(columns[0][0],3);
  for(size_t i=1;i<columns.size();++i) {
    // The pinned tab-index -1 guard leaves every positioned stop unused;
    // Our explicit 20pt Cousine fixture advances 12pt without tabs. Rust's
    // corrected implementation places the latter Is at 48/96pt.
    EXPECT_EQ(columns[i][0]-columns[0][0],int(i)*(native?12:48));
    EXPECT_EQ(columns[i][1]-columns[i][0],columns[0][1]-columns[0][0]);
  }
}
void CompareTabGlyphs(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& name) {
  const auto ac=InkColumns(a),bc=InkColumns(b);ASSERT_EQ(ac.size(),3u);ASSERT_EQ(bc.size(),3u);
  for(size_t i=0;i<3;++i) {
    ASSERT_EQ(ac[i][1]-ac[i][0],bc[i][1]-bc[i][0]);
    ASSERT_GE(ac[i][0],1);ASSERT_GE(bc[i][0],1);
    const int width=ac[i][1]-ac[i][0]+3;
    CompareTextInput(CropRaster(a,ac[i][0]-1,0,width,a.height),
                     CropRaster(b,bc[i][0]-1,0,width,b.height),artifacts,name+"-glyph"+std::to_string(i));
  }
}
void SaveTextInput(const Library& writer, Session& s, const InputSpec& spec,
                   const std::u16string& expected, Artifacts& artifacts,
                   const std::string& label, bool reset_horizontal_scroll=false,
                   bool hebrew_native_boundary=false) {
  ASSERT_TRUE(FocusInput(s)); EXPECT_EQ(s.Text(), expected);
  if(reset_horizontal_scroll)Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
  FinishInput(s);
  const auto before = InputImage(s, spec);
  const auto saved = SaveDocument(s.api, s.document, artifacts, writer.name() + "-" + label);
  ASSERT_FALSE(saved.empty());
  Raster common;
  if(hebrew_native_boundary) {
    // Preserve the original saved logical run. The pinned native breaker only
    // starts bidi for Arabic, so use one real Hebrew glyph for the strict
    // cross-engine paint comparison after asserting the original below.
    artifacts.Image(writer.name()+"-original-"+label,before);
    Put(s,u"\u05d1");EXPECT_EQ(s.Text(),u"\u05d1");FinishInput(s);
    common=InputImage(s,spec);
  }
  for (const auto* reader : Implementations()) {
    SCOPED_TRACE(reader->name());
    Session loaded(reader->api(), saved, true, true);
    ASSERT_TRUE(loaded.valid()); ASSERT_TRUE(loaded.xfa_loaded); ASSERT_TRUE(FocusInput(loaded));
    EXPECT_EQ(loaded.Text(), writer.name() == "native" && !expected.empty() ? u"\n" + expected : expected);
    // Native's documented XML writer newline is asserted above. Restore the
    // intended value through actual public input before comparing its paint.
    if (writer.name() == "native" && !expected.empty()) Put(loaded, expected);
    EXPECT_EQ(loaded.Text(), expected);
    if(reset_horizontal_scroll)Key(loaded,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
    FinishInput(loaded);
    const auto original=InputImage(loaded,spec);
    if(hebrew_native_boundary) {
      artifacts.Image(writer.name()+"-"+reader->name()+"-original-"+label,original);
      const auto bounds=TextInk(original);EXPECT_GT(bounds.count,10u);
      EXPECT_GE(bounds.left,0);EXPECT_LT(bounds.right,spec.width);
      EXPECT_GE(bounds.top,0);EXPECT_LT(bounds.bottom,spec.height);
      if(writer.name()==reader->name())
        CompareTextInput(before,original,artifacts,writer.name()+"-same-reader-original-"+label);
      Put(loaded,u"\u05d1");EXPECT_EQ(loaded.Text(),u"\u05d1");FinishInput(loaded);
      CompareTextInput(common,InputImage(loaded,spec),artifacts,
                       writer.name()+"-"+reader->name()+"-common-glyph-"+label);
    } else {
      CompareTextInput(before,original,artifacts,
                       writer.name()+"-"+reader->name()+"-"+label);
    }
  }
}

TEST(CoverageXfaTextInput, LogicalMultilineNavigationSelectionAndBoundaryDeletionKeepHistory) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Step { int key, flags; const char16_t* selected; const char16_t* native_selected=nullptr; };
  const std::vector<std::vector<Step>> programs = {
      {{FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey,u""},{FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey,u"a"}},
      {{FWL_VKEY_End,FWL_EVENTFLAG_ControlKey,u""},{FWL_VKEY_Left,FWL_EVENTFLAG_ShiftKey,u"Z",u"\nZ"}},
      {{FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey,u""},{FWL_VKEY_Down,0,u""},{FWL_VKEY_End,FWL_EVENTFLAG_ShiftKey,u"cdef",u"cdef\nZ"}},
      {{FWL_VKEY_End,FWL_EVENTFLAG_ControlKey,u""},{FWL_VKEY_Up,0,u""},{FWL_VKEY_Home,FWL_EVENTFLAG_ShiftKey,u"c",u"cdef"}},
      {{FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey,u""},{FWL_VKEY_End,0,u""},{FWL_VKEY_Down,0,u""},{FWL_VKEY_End,FWL_EVENTFLAG_ShiftKey,u"ef",u"cdef\nZ"}},
      {{FWL_VKEY_End,FWL_EVENTFLAG_ControlKey,u""},{FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey|FWL_EVENTFLAG_ShiftKey,u"ab\ncdef\nZ"}},
      {{FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey,u""},{FWL_VKEY_Left,0,u""},{FWL_VKEY_Up,0,u""},{FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey,u"a"}}};
  Artifacts artifacts; InputSpec spec; spec.value=u"ab\ncdef\nZ";
  spec.ui="<textEdit multiLine=\"1\"/>"; spec.height=72;
  for (size_t row=0; row<programs.size(); ++row) {
    SCOPED_TRACE(row); std::array<Raster,2> frames; size_t backend=0;
    for (const auto* library:Implementations()) {
      SCOPED_TRACE(library->name()); Session s(library->api(),TextInputFixture(spec),true,true);
      ASSERT_TRUE(s.valid()); ASSERT_TRUE(s.xfa_loaded); ASSERT_TRUE(FocusInput(s)); EXPECT_EQ(s.Text(),spec.value);
      for (const auto& step:programs[row]) {
        Key(s,step.key,step.flags);
        // Native passes the selection end as a count, and its caret helpers
        // skip line boundaries. Assert those defects without copying them.
        EXPECT_EQ(s.Text(true),library->name()=="native"&&step.native_selected?step.native_selected:step.selected);
      }
      if (!s.Text(true).empty()) {
        const auto selected=s.Text(true);const auto start=spec.value.find(selected);ASSERT_NE(start,std::u16string::npos);
        const auto expected=spec.value.substr(0,start)+u"Q"+spec.value.substr(start+selected.size());
        s.Replace(u"Q"); const auto edited=s.Text(); EXPECT_EQ(edited,expected);
        ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page)); EXPECT_EQ(s.Text(),spec.value);
        ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page)); EXPECT_EQ(s.Text(),edited);
        const std::u16string correct_selected=programs[row].back().selected;
        const auto correct_start=spec.value.find(correct_selected);
        const auto correct=spec.value.substr(0,correct_start)+u"Q"+spec.value.substr(correct_start+correct_selected.size());
        if(edited!=correct)Put(s,correct);
        EXPECT_EQ(s.Text(),correct);
      }
      FinishInput(s); frames[backend++]=InputImage(s,spec); EXPECT_GT(TextInk(frames[backend-1]).count,20u);
      if (row==0) SaveTextInput(*library,s,spec,u"Qb\ncdef\nZ",artifacts,"multiline-history");
    }
    CompareTextInput(frames[0],frames[1],artifacts,"navigation-"+std::to_string(row));
  }
  // Delete and Backspace at the actual document edges must be harmless; the
  // same keys at a selected range must delete exactly that range.
  for (const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    Session s(library->api(),TextInputFixture(spec),true,true); ASSERT_TRUE(s.valid()); ASSERT_TRUE(FocusInput(s));
    Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey); s.api.FORM_OnChar(s.form,s.page,8,0); EXPECT_EQ(s.Text(),spec.value);
    Key(s,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey); Key(s,FWL_VKEY_Delete); EXPECT_EQ(s.Text(),spec.value);
    Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey); Key(s,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey);
    Key(s,FWL_VKEY_Delete); EXPECT_EQ(s.Text(),library->name()=="native"?u"a\ncdef\nZ":u"b\ncdef\nZ");
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page)); EXPECT_EQ(s.Text(),spec.value);
  }
  // A partial selection must collapse to its start before the next character.
  // Whole-range deletion cannot detect a stale caret: clamping alone yields0.
  for(bool backwards:{false,true}) {
    SCOPED_TRACE(backwards);std::array<Raster,2> frames;size_t backend=0;
    InputSpec partial=spec;partial.value=u"ABCDE";
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),TextInputFixture(partial),true,true);
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);ASSERT_TRUE(FocusInput(s));
      Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
      for(int i=0;i<(backwards?3:1);++i)Key(s,FWL_VKEY_Right);
      for(int i=0;i<2;++i)Key(s,backwards?FWL_VKEY_Left:FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey);
      const bool native=library->name()=="native";
      // Native passes an end index as a selection count; its forward range
      // therefore includes D. Its Delete then removes only the cursor's char.
      EXPECT_EQ(s.Text(true),native&&!backwards?u"BCD":u"BC");
      Key(s,FWL_VKEY_Delete);
      const std::u16string deleted=native?(backwards?u"ACDE":u"ABCE"):u"ADE";
      EXPECT_EQ(s.Text(),deleted);
      ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,'Z',0));
      const std::u16string entered=native?(backwards?u"AZCDE":u"ABCZE"):u"AZDE";
      EXPECT_EQ(s.Text(),entered);
      ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),deleted);
      ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),partial.value);
      ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),deleted);
      ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),entered);
      if(native) {
        artifacts.Image("native-partial-delete-original-"+std::to_string(backwards),InputImage(s,partial));
        // Preserve the independent native observation above, then supply the
        // correct common visible value through real public input for comparison.
        Put(s,u"AZDE");EXPECT_EQ(s.Text(),u"AZDE");
      }
      FinishInput(s);frames[backend++]=InputImage(s,partial);EXPECT_GT(TextInk(frames[backend-1]).count,20u);
      SaveTextInput(*library,s,partial,u"AZDE",artifacts,"partial-delete-caret-"+std::to_string(backwards));
    }
    CompareTextInput(frames[0],frames[1],artifacts,"partial-delete-caret-"+std::to_string(backwards));
  }
  int terminator_case=0;
  for(const std::u16string value:{u"ab\r\ncdef\r\nZ",u"ab\ncdef\r\nZ",u"ab\rcdef\nZ"}) {
    SCOPED_TRACE(testing::PrintToString(value));
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());
      InputSpec empty=spec;empty.value.clear();Session s(library->api(),TextInputFixture(empty),true,true);
      ASSERT_TRUE(s.valid());Put(s,value);EXPECT_EQ(s.Text(),value);
      Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);Key(s,FWL_VKEY_Down);
      Key(s,FWL_VKEY_End,FWL_EVENTFLAG_ShiftKey);
      const auto native_selected=value.substr(value.find(u"cdef"));
      EXPECT_EQ(s.Text(true),library->name()=="native"?native_selected:u"cdef");
      Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);Key(s,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey|FWL_EVENTFLAG_ShiftKey);
      EXPECT_EQ(s.Text(true),library->name()=="native"?native_selected:value);FinishInput(s);images[backend++]=InputImage(s,spec);
    }
    CompareTextInput(images[0],images[1],artifacts,"mixed-line-terminators-"+std::to_string(terminator_case++));
  }
  std::array<Raster,2> control_frames;size_t control_backend=0;
  for(const auto* library:Implementations()) {
    InputSpec controls=spec;controls.value=u"AB";
    Session s(library->api(),TextInputFixture(controls),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(FocusInput(s));
    Key(s,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey);
    s.api.FORM_OnChar(s.form,s.page,13,0);s.api.FORM_OnChar(s.form,s.page,9,0);s.api.FORM_OnChar(s.form,s.page,'C',0);
    // The FF field consumes Tab before dispatching to FWL; paste exercises
    // actual tab layout separately from this public character dispatch.
    EXPECT_EQ(s.Text(),u"AB\nC");
    for(int ch:{0,1,27,127})s.api.FORM_OnChar(s.form,s.page,ch,0);
    EXPECT_EQ(s.Text(),u"AB\nC");Put(s,u"AB\n\tC");EXPECT_EQ(s.Text(),u"AB\n\tC");
    FinishInput(s);control_frames[control_backend++]=InputImage(s,controls);
  }
  CompareTextInput(control_frames[0],control_frames[1],artifacts,"typed-tab-return-and-ignored-controls");
}

TEST(CoverageXfaTextInput, DoubleClickWordTablesAndShiftMouseRangesUseUnicodeBoundaries) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Case { std::u16string text, selected; int index; const char* family="Ahem"; };
  // CFDE_TextEditEngine's source word-break tables exercise MidLetter,
  // MidNum, ExtendNumLet and mixed alphanumeric states through real hit tests.
  const std::vector<Case> cases={
      {u"Hello World",u"Hello",1},{u"Hello World",u"World",7},
      {u"123 456 789",u"456",5},{u"123def789",u"123def789",4},
      {u"abc456ghi",u"abc456ghi",4},{u"hello, world",u",",5},
      {u"np-complete",u"complete",6},{u"A:A",u"A:A",0},
      {u"A::A",u":",1},{u"0,0",u"0,0",0},{u"0,,0",u",",1},
      {u"A__A",u"A__A",0},{u"A00A",u"A00A",1},
      {u"caf\u00e9 noir",u"caf\u00e9",0},{u"A_0_\u3031_",u"A_0_\u3031_",0},
      {u"\u0391\u0392\u0393 123",u"\u0391\u0392\u0393",0,"Roboto"}};
  Artifacts artifacts;
  for (size_t row=0;row<cases.size();++row) {
    SCOPED_TRACE(row); InputSpec spec;spec.value=cases[row].text;spec.family=cases[row].family;
    spec.font_size=spec.family=="Ahem"?8:12;
    std::array<Raster,2> images;size_t backend=0;
    for (const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),TextInputFixture(spec),true,true);
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);ASSERT_TRUE(FocusInput(s));
      const double x=(cases[row].index+0.35)*spec.font_size;
      EXPECT_TRUE(s.api.FORM_OnLButtonDoubleClick(s.form,s.page,0,x,16));
      EXPECT_EQ(s.Text(true),cases[row].selected);
      s.Replace(u"Q");const auto expected=cases[row].text.substr(0,cases[row].index==0?0:cases[row].text.find(cases[row].selected))+u"Q"+
          cases[row].text.substr(cases[row].text.find(cases[row].selected)+cases[row].selected.size());
      EXPECT_EQ(s.Text(),expected);ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),spec.value);
      FinishInput(s);images[backend++]=InputImage(s,spec);EXPECT_GT(TextInk(images[backend-1]).count,5u);
      if(row==7)SaveTextInput(*library,s,spec,spec.value,artifacts,"word-boundary");
    }
    CompareTextInput(images[0],images[1],artifacts,"word-"+std::to_string(row));
  }
  InputSpec spec;spec.value=u"abcdef";spec.family="Ahem";spec.font_size=8;
  std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    Session s(library->api(),TextInputFixture(spec),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(FocusInput(s));
    Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
    s.api.FORM_OnLButtonDown(s.form,s.page,FWL_EVENTFLAG_ShiftKey,8*3,14);
    s.api.FORM_OnMouseMove(s.form,s.page,FWL_EVENTFLAG_ShiftKey|FWL_EVENTFLAG_LeftButtonDown,8*5,14);
    s.api.FORM_OnLButtonUp(s.form,s.page,FWL_EVENTFLAG_ShiftKey,8*5,14);
    EXPECT_EQ(s.Text(true),u"abcde");s.Replace(u"X");EXPECT_EQ(s.Text(),u"Xf");
    FinishInput(s);images[backend++]=InputImage(s,spec);
  }
  CompareTextInput(images[0],images[1],artifacts,"shift-mouse-range");
}

TEST(CoverageXfaTextInput, CombLimitsTruncatedReplacementAndPasswordAliasesPaintTheirActualCells) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  for (int cells:{0,1,4,6}) {
    SCOPED_TRACE(cells);InputSpec spec;spec.family="Ahem";spec.width=72;spec.value_attributes="maxChars=\"6\"";
    spec.ui="<textEdit><comb numberOfCells=\""+std::to_string(cells)+"\"/></textEdit>";
    spec.events=forms::Event("xfa.form.root.audit.rawValue='full';","full");
    const int limit=cells?cells:6;const std::u16string expected=std::u16string(u"ABCDEFGH").substr(0,limit);
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),TextInputFixture(spec),true,true);
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);ASSERT_TRUE(FocusInput(s));s.Replace(u"ABCDEFGH");EXPECT_EQ(s.Text(),expected);
      EXPECT_TRUE(s.api.FORM_CanUndo(s.form,s.page));ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),u"");
      ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),expected);
      Key(s,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey);s.api.FORM_OnChar(s.form,s.page,'Z',0);EXPECT_EQ(s.Text(),expected);
      // Native scrolls horizontally for the final cell's caret and keeps that
      // offset after blur. Return home before comparing the actual cell paint.
      Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
      FinishInput(s);ASSERT_TRUE(s.Focus(4,10+spec.height+6));EXPECT_EQ(s.Text(),u"full");FinishInput(s);
      images[backend]=InputImage(s,spec);
      for(int cell=0;cell<limit;++cell) {
        const int left=cell*spec.width/limit, width=spec.width/limit;
        EXPECT_GT(TextInk(CropRaster(images[backend],left+3,2,width-6,spec.height-4)).count,10u);
      }
      ++backend;SaveTextInput(*library,s,spec,expected,artifacts,"comb-"+std::to_string(cells),true);
    }
    CompareTextInput(images[0],images[1],artifacts,"comb-backends-"+std::to_string(cells));
  }
  for(const char* alias:{"#","*","X"}) {
    SCOPED_TRACE(alias);InputSpec spec;spec.value=u"secret";spec.control=std::u16string(6,alias[0]);
    spec.ui="<passwordEdit passwordChar=\""+std::string(alias)+"\"/>";spec.height=20;
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      Session s(library->api(),TextInputFixture(spec),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(FocusInput(s));EXPECT_EQ(s.Text(),u"secret");
      FinishInput(s);const auto original=InputImage(s,spec);const auto page=s.Draw(240,320);
      CompareTextInput(original,CropRaster(page,0,50,spec.width,20),artifacts,"password-alias-control-"+std::string(alias));
      Put(s,u"hidden");EXPECT_EQ(s.Text(),u"hidden");FinishInput(s);images[backend]=InputImage(s,spec);
      CompareRaster(original,images[backend++],artifacts,"equal-length-passwords-mask-identically-"+std::string(alias));
      SaveTextInput(*library,s,spec,u"hidden",artifacts,"password-"+std::string(alias));
    }
    CompareTextInput(images[0],images[1],artifacts,"password-backends-"+std::string(alias));
  }
}

TEST(CoverageXfaTextInput, AlignmentWrappingAndEmbeddedUnicodeBidiHaveIndependentVisibleBounds) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  for(const auto* library:Implementations()) {
    std::array<InkBounds,3> horizontal,vertical;
    for(int h=0;h<3;++h)for(int v=0;v<3;++v) {
      InputSpec spec;spec.value=u"MMMM";spec.family="Ahem";spec.height=72;
      spec.properties="<para hAlign=\""+std::string(std::array<const char*,3>{"left","center","right"}[h])+
          "\" vAlign=\""+std::string(std::array<const char*,3>{"top","middle","bottom"}[v])+"\"/>";
      Session s(library->api(),TextInputFixture(spec),true,true);ASSERT_TRUE(s.valid());FinishInput(s);
      const auto bounds=TextInk(InputImage(s,spec));EXPECT_GT(bounds.count,50u);
      if(v==0)horizontal[h]=bounds;
      if(h==0)vertical[v]=bounds;
    }
    EXPECT_GT(horizontal[1].left,horizontal[0].left+20);EXPECT_GT(horizontal[2].left,horizontal[1].left+20);
    EXPECT_GT(vertical[1].top,vertical[0].top+10);EXPECT_GT(vertical[2].top,vertical[1].top+10);
  }
  struct Case {std::u16string text;const char* family;const char* ui;const char* properties;};
  const std::vector<Case> cases={
      {u"AAAA AAAA AAAA","Ahem","<textEdit multiLine=\"1\" hScrollPolicy=\"off\"/>",""},
      {u"AAAA AAAA AAAA","Ahem","<textEdit hScrollPolicy=\"auto\"/>",""},
      {u"caf\u00e9 \u00c5ngstr\u00f6m","Roboto","<textEdit/>",""},
      {u"\u0391\u0392\u0393 123 \u0410\u0411\u0412","Roboto","<textEdit/>",""},
      {u"\u05d1\u05e0\u05d9\u05de\u05d9\u05df","NotoSansHebrew","<textEdit/>","<para hAlign=\"right\"/>"},
      {u"12 (\u05d1\u05e0\u05d9\u05de\u05d9\u05df) 34","NotoSansHebrew","<textEdit/>",""},
      {u"\u05d1\u05d9\u05df\n\u05de\u05e0\u05d9","NotoSansHebrew","<textEdit multiLine=\"1\"/>",""}};
  std::array<InkBounds,2> wrapped;
  for(size_t row=0;row<cases.size();++row) {
    SCOPED_TRACE(row);InputSpec spec;spec.value=cases[row].text;spec.family=cases[row].family;spec.ui=cases[row].ui;
    spec.properties=cases[row].properties;spec.width=row<2?72:180;spec.height=72;
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),TextInputFixture(spec),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      ASSERT_TRUE(FocusInput(s));EXPECT_EQ(s.Text(),spec.value);ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));EXPECT_EQ(s.Text(true),spec.value);
      FinishInput(s);images[backend]=InputImage(s,spec);const auto bounds=TextInk(images[backend]);EXPECT_GT(bounds.count,10u);
      EXPECT_GE(bounds.left,0);EXPECT_LT(bounds.right,spec.width);EXPECT_GE(bounds.top,0);EXPECT_LT(bounds.bottom,spec.height);
      if(library->name()=="rust"&&row<2)wrapped[row]=bounds;
      if(row==1) {
        artifacts.Image(library->name()+"-original-default-nowrap",images[backend]);
        if(library->name()=="native") { EXPECT_GT(bounds.bottom-bounds.top,24); }
        else { EXPECT_LT(bounds.bottom-bounds.top,16); }
        // Native's unsynchronized default breaker wraps this single-line
        // control at100pt. Require correct Rust paint above; compare a common
        // short value after real public input in both controls.
        Put(s,u"AAAA");EXPECT_EQ(s.Text(),u"AAAA");FinishInput(s);images[backend]=InputImage(s,spec);
      }
      if(row>=4) {
        // Keep the original logical payload, selection, bounds and paint above.
        // The dedicated traversal/bidi scenarios independently check correct
        // RTL order against positioned singleton glyphs. Native's Arabic-only
        // bidi trigger gives different original Hebrew paint; use actual public
        // input for a shared visible control, retaining the same pixel budget.
        artifacts.Image(library->name()+"-original-hebrew-"+std::to_string(row),images[backend]);
        if(row==4)SaveTextInput(*library,s,spec,spec.value,artifacts,"hebrew-bidi",false,true);
        Put(s,u"\u05d1");EXPECT_EQ(s.Text(),u"\u05d1");FinishInput(s);
        images[backend]=InputImage(s,spec);
      }
      ++backend;
    }
    CompareTextInput(images[0],images[1],artifacts,"unicode-wrap-"+std::to_string(row));
  }
  EXPECT_GT(wrapped[0].bottom-wrapped[0].top,wrapped[1].bottom-wrapped[1].top+12);
  struct Limited {const char* ui;const char16_t* expected;};
  const std::array<Limited,4> limited={{
      {"<textEdit hScrollPolicy=\"off\"/>",u"ABCD"},
      {"<textEdit hScrollPolicy=\"auto\"/>",u"ABCDEFGH"},
      {"<textEdit multiLine=\"1\" hScrollPolicy=\"off\" vScrollPolicy=\"off\"/>",u"ABCD"},
      {"<textEdit multiLine=\"1\" hScrollPolicy=\"off\" vScrollPolicy=\"auto\"/>",u"ABCDEFGH"}}};
  for(size_t row=0;row<limited.size();++row) {
    SCOPED_TRACE(row);InputSpec spec;spec.family="Ahem";spec.width=48;spec.height=18;spec.ui=limited[row].ui;
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),TextInputFixture(spec),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(FocusInput(s));
      s.Replace(u"ABCDEFGH");
      EXPECT_EQ(s.Text(),library->name()=="native"&&row==2?u"ABCDEFGH":limited[row].expected);
      // Native's separate width/height early breaks accept a fitting width
      // even when both limits are enabled and height overflows. Rust requires
      // both limits. Normalize native through real input only after asserting.
      if(library->name()=="native"&&row==2)Put(s,limited[row].expected);
      FinishInput(s);
      images[backend++]=InputImage(s,spec);EXPECT_GT(TextInk(images[backend-1]).count,20u);
      if(row==0||row==3)SaveTextInput(*library,s,spec,limited[row].expected,artifacts,"scroll-limit-"+std::to_string(row));
    }
    CompareTextInput(images[0],images[1],artifacts,"scroll-limit-backends-"+std::to_string(row));
  }
  InputSpec locked;locked.value=u"Locked";locked.field_attributes="access=\"readOnly\"";
  for(const auto* library:Implementations()) {
    Session s(library->api(),TextInputFixture(locked),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(FocusInput(s));
    s.api.FORM_OnChar(s.form,s.page,'x',0);EXPECT_EQ(s.Text(),u"Locked");
    s.api.FORM_SelectAllText(s.form,s.page);s.Replace(u"replacement");EXPECT_EQ(s.Text(),u"Locked");
  }
}

TEST(CoverageXfaTextInput, ChangeEventsCancelTransformAndRewriteRangesWithoutCorruptingHistory) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  InputSpec spec;spec.value=u"abcd";
  spec.events=forms::Event(
      "xfa.form.root.audit.rawValue=xfa.event.prevText+'|'+xfa.event.selStart+':'+xfa.event.selEnd+'|'+xfa.event.change+'|'+xfa.event.newText;"
      "if(xfa.event.change=='!')xfa.event.cancelAction=true;"
      "if(xfa.event.change=='x')xfa.event.change='X';"
      "if(xfa.event.change=='@'){xfa.event.selStart=1;xfa.event.selEnd=3;xfa.event.change='Q';}","change");
  Artifacts artifacts;std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());Session s(library->api(),TextInputFixture(spec),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(FocusInput(s));
    Key(s,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey);s.api.FORM_OnChar(s.form,s.page,'x',0);EXPECT_EQ(s.Text(),u"abcdX");
    s.api.FORM_OnChar(s.form,s.page,'!',0);EXPECT_EQ(s.Text(),u"abcdX");
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));EXPECT_EQ(s.Text(),u"abcd");
    ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));EXPECT_EQ(s.Text(),u"abcdX");
    Key(s,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey);s.api.FORM_OnChar(s.form,s.page,'@',0);
    EXPECT_EQ(s.Text(),library->name()=="native"?u"aQbcdX":u"aQdX");
    FinishInput(s);
    ASSERT_TRUE(s.Focus(4,10+spec.height+6));EXPECT_EQ(s.Text(),u"abcdX|5:5|@|abcdX@");
    if(library->name()=="native")Put(s,u"aQdX");
    FinishInput(s);images[backend++]=InputImage(s,spec);
    SaveTextInput(*library,s,spec,u"aQdX",artifacts,"change-rewrite");
  }
  CompareTextInput(images[0],images[1],artifacts,"change-event-backends");
}

TEST(CoverageXfaTextInput, RichCssFontDecorationTabsListsAndInlineSpacingSurviveInputAndSaving) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Case {const char* name;const char* html;};
  const std::vector<Case> cases={
      {"bold-italic","<p><b>Bold</b> <i>Italic</i> <b><i>Both</i></b></p>"},
      {"size-scale","<p><span style=\"font-size:8pt;color:#0000ff\">Small</span> <span style=\"font-size:20pt;color:#ff0000\">Large</span></p>"},
      {"underline","<p><span style=\"text-decoration:underline\">MMMM</span></p>"},
      {"strike","<p><span style=\"text-decoration:line-through\">MMMM</span></p>"},
      {"super-sub","<p>M<span style=\"vertical-align:4pt;font-size:10pt;color:#ff0000\">M</span>M"
                   "<span style=\"vertical-align:-4pt;font-size:10pt;color:#0000ff\">M</span>M</p>"},
      {"indent","<p style=\"text-indent:24pt\">Indented</p>"},
      {"right","<p style=\"text-align:right\">Right</p>"},
      {"center","<p style=\"text-align:center\">Center</p>"},
      {"justify","<p style=\"text-align:justify;line-height:16pt\">One two three four five six seven eight nine ten eleven twelve</p>"},
      // Narrow I glyphs leave visible gaps even in native's no-tab fallback.
      // Explicit size/family make its 12pt advance independent of inheritance;
      // the side bearing also leaves room for the glyph comparison's padding.
      {"tabs","<p style=\"font-family:Cousine;font-size:20pt;tab-stops:left 48pt left 96pt\">I<span style=\"xfa-tab-count:1\"/>I<span style=\"xfa-tab-count:1\"/>I</p>"},
      {"lists","<ol><li>First</li><li>Second</li></ol>"},
      {"pre-nbsp","<p style=\"white-space:pre\">A  B&#160;C<br/>D  E</p>"}};
  Artifacts artifacts;
  for(size_t row=0;row<cases.size();++row) {
    SCOPED_TRACE(cases[row].name);InputSpec spec;spec.value=u"Before";spec.rich=cases[row].html;
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());Session s(library->api(),TextInputFixture(spec),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
      const int top=10+spec.height+40;const auto page=s.Draw(240,320);
      const auto before=CropRaster(page,0,top,220,80);const auto ink=TextInk(before);EXPECT_GT(ink.count,15u);
      if(row==1) {
        const auto small=ColorInk(before,false),large=ColorInk(before,true);
        EXPECT_GT(small.count,5u);EXPECT_GT(large.count,5u);
        EXPECT_GT(large.bottom-large.top,small.bottom-small.top+4);
        EXPECT_GT(large.right-large.left,small.right-small.left+8);
      }
      if(row==2||row==3) {
        auto plain_spec=spec;plain_spec.rich="<p>MMMM</p>";
        Session plain(library->api(),TextInputFixture(plain_spec),true,true);ASSERT_TRUE(plain.valid());
        const auto control=CropRaster(plain.Draw(240,320),0,top,220,80);
        if(row==2) { EXPECT_GT(ink.bottom,TextInk(control).bottom); }
        if(row==3) { EXPECT_GT(LongestDarkRun(before),LongestDarkRun(control)+8); }
      }
      if(row==4) {
        const auto up=ColorInk(before,true),down=ColorInk(before,false);
        EXPECT_GT(up.count,3u);EXPECT_GT(down.count,3u);EXPECT_LT(up.top,down.top);
      }
      if(row==5) { EXPECT_GE(ink.left,20); }
      if(row==6) { EXPECT_GT(ink.left,100); }
      if(row==7) { EXPECT_GT(ink.left,50); }
      if(row==8||row==10||row==11) { EXPECT_GT(ink.bottom-ink.top,14); }
      if(row==9) {
        CheckTabColumns(before,library->name()=="native");
        if(::testing::Test::HasFailure())artifacts.Image(library->name()+"-tabs-original",before);
      }
      Put(s,u"Changed");EXPECT_EQ(s.Text(),u"Changed");FinishInput(s);
      images[backend]=CropRaster(s.Draw(240,320),0,top,220,80);
      CompareRaster(before,images[backend++],artifacts,library->name()+"-typing-preserves-rich-"+cases[row].name);
      if(row==0||row==4||row==9||row==10) {
        const auto bytes=SaveDocument(s.api,s.document,artifacts,library->name()+"-rich-"+cases[row].name);
        for(const auto* reader:Implementations()) {
          Session reloaded(reader->api(),bytes,true,true);ASSERT_TRUE(reloaded.valid());ASSERT_TRUE(reloaded.xfa_loaded);
          ASSERT_TRUE(FocusInput(reloaded));EXPECT_EQ(reloaded.Text(),library->name()=="native"?u"\nChanged":u"Changed");FinishInput(reloaded);
          const auto after=CropRaster(reloaded.Draw(240,320),0,top,220,80);
          const auto label=library->name()+"-"+reader->name()+"-rich-"+cases[row].name;
          if(row==9) {
            CheckTabColumns(after,reader->name()=="native");
            if(::testing::Test::HasFailure())artifacts.Image(label+"-tabs-reloaded",after);
            CompareTabGlyphs(before,after,artifacts,label);
          } else CompareTextInput(before,after,artifacts,label);
        }
      }
    }
    if(row==9)CompareTabGlyphs(images[0],images[1],artifacts,"rich-tab-glyphs");
    else CompareTextInput(images[0],images[1],artifacts,"rich-style-"+std::string(cases[row].name));
  }
}

}  // namespace
}  // namespace pdfoxium::integration
