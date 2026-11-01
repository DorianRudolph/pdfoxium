// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <fstream>
#include <iterator>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
constexpr int kWidth=320,kHeight=240,kScale=4;
struct Spec {
  const char* name;
  std::u16string text;
  int width=240;
  bool after=false,hebrew=false;
};
struct Box {int left,top,right,bottom;};
std::string Resource(const char* path) {
  std::ifstream in(TestRoot()/path,std::ios::binary);
  if(!in.good())throw std::runtime_error("Missing pinned hit-test font");
  return {std::istreambuf_iterator<char>(in),std::istreambuf_iterator<char>()};
}
std::vector<uint8_t> Fixture(const Spec& spec) {
  static const auto hebrew=Resource("tests/capi/resources/hebrew_unicode.ttf");
  static const auto cmap=Resource("upstream/pdfium/testing/resources/hebrew_cmap.fragment");
  const std::string family=spec.hebrew?"PdfoxiumHebrewTest":"Courier";
  const std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" locale=\"en_US\" restoreState=\"auto\">"
    "<pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"320pt\" long=\"240pt\"/><contentArea w=\"320pt\" h=\"240pt\"/></pageArea></pageSet>"
    "<field name=\"input\" x=\"20pt\" y=\"20pt\" w=\""+std::to_string(spec.width)+"pt\" h=\"120pt\"><bind match=\"once\"/><ui><textEdit multiLine=\"1\"/></ui><font typeface=\""+family+"\" size=\"20pt\"/><para lineHeight=\"20pt\"/><value><text>seed</text></value>"+
    forms::Event("if(typeof this.rawValue==='string')this.rawValue=this.rawValue.replace(/^\\n/,'');","initialize")+"</field>"
    "<draw x=\"280pt\" y=\"215pt\" w=\"10pt\" h=\"10pt\"><value><rectangle><fill><color value=\"0,128,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>"
    "</subform></template>";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 240] /Resources << /Font << /Courier 10 0 R /PdfoxiumHebrewTest 11 0 R >> >> >>",
    "<< /Fields [] /DR << /Font << /Courier 10 0 R /PdfoxiumHebrewTest 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
    PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
    PdfStream("",xml),
    PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),
    PdfStream("","</xdp:xdp>"),
    "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>",
    "<< /Type /Font /Subtype /TrueType /BaseFont /PdfoxiumHebrewTest-Regular /FirstChar 0 /LastChar 5 /Widths [600 294 235 645 397 542] /FontDescriptor 12 0 R /ToUnicode 14 0 R >>",
    "<< /Type /FontDescriptor /FontName /PdfoxiumHebrewTest-Regular /Flags 4 /FontBBox [-210 -252 716 869] /Ascent 1069 /Descent -293 /CapHeight 869 /StemV 80 /FontFile2 13 0 R >>",
    PdfStream("/Length1 "+std::to_string(hebrew.size()),hebrew),PdfStream("",cmap)});
}
void Blur(Session& s) {
  s.api.FORM_ForceToKillFocus(s.form);
  s.Click(310,230);
  s.api.FORM_ForceToKillFocus(s.form);
}
void NormalizeViewport(Session& s) {
  const auto text=s.Text();
  const bool undo=s.api.FORM_CanUndo(s.form,s.page),redo=s.api.FORM_CanRedo(s.form,s.page);
  // Clicking just after a full wrapped row legitimately scrolls one pixel to
  // expose its caret. Return to a common logical origin before paint checks;
  // the last line's caret otherwise fits without undoing that viewport shift.
  ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey));
  EXPECT_EQ(s.Text(),text);
  EXPECT_EQ(bool(s.api.FORM_CanUndo(s.form,s.page)),undo);
  EXPECT_EQ(bool(s.api.FORM_CanRedo(s.form,s.page)),redo);
}
void Put(Session& s,const std::u16string& value) {
  ASSERT_TRUE(s.Focus(22,24));
  ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
  s.Replace(value);
  ASSERT_EQ(s.Text(),value);
  ASSERT_NO_FATAL_FAILURE(NormalizeViewport(s));
  Blur(s);
}
bool Dark(const Raster& image,int x,int y) {
  const size_t at=(size_t(y)*image.width+x)*4;
  return image.pixels[at]<160&&image.pixels[at+1]<160&&image.pixels[at+2]<160;
}
std::vector<Box> GlyphBoxes(const Raster& image,const Spec& spec) {
  // Glyph ink, rather than a guessed font advance, supplies the actual click
  // targets. Courier I and the pinned Hebrew forms have contiguous horizontal
  // projections and separated ink; spaces and line gaps remain measurable.
  std::vector<std::array<int,2>> rows;
  for(int y=20*kScale;y<140*kScale;++y) {
    bool ink=false;
    for(int x=20*kScale;x<(20+spec.width)*kScale&&!ink;++x)ink=Dark(image,x,y);
    if(!ink)continue;
    if(rows.empty()||rows.back()[1]!=y-1)rows.push_back({y,y});else rows.back()[1]=y;
  }
  std::vector<Box> result;
  for(const auto& row:rows) {
    for(int x=20*kScale;x<(20+spec.width)*kScale;++x) {
      bool ink=false;
      for(int y=row[0];y<=row[1]&&!ink;++y)ink=Dark(image,x,y);
      if(!ink)continue;
      if(result.empty()||result.back().top!=row[0]||result.back().right!=x-1)result.push_back({x,row[0],x,row[1]});
      else result.back().right=x;
    }
  }
  return result;
}
void Probe(Session& s,const Spec& spec,bool native) {
  const auto high=s.Draw(kWidth*kScale,kHeight*kScale);
  const auto boxes=GlyphBoxes(high,spec);
  std::vector<size_t> logical;
  for(size_t i=0;i<spec.text.size();++i)if(spec.text[i]!=u' '&&spec.text[i]!=u'\n')logical.push_back(i);
  if(spec.hebrew&&!native)std::reverse(logical.begin(),logical.end());
  ASSERT_EQ(boxes.size(),logical.size());
  if(spec.width==60) {
    std::vector<int> tops;
    for(const auto& b:boxes)if(tops.empty()||tops.back()!=b.top)tops.push_back(b.top);
    EXPECT_GE(tops.size(),2u);
  }
  for(size_t glyph=0;glyph<boxes.size();++glyph) {
    SCOPED_TRACE("glyph="+std::to_string(glyph));
    const auto& b=boxes[glyph];
    const double fraction=spec.after?.75:.25;
    const double x=(b.left+(b.right-b.left+1)*fraction)/kScale;
    const double y=(b.top+(b.bottom-b.top+1)*.5)/kScale;
    // Native's documented Hebrew breaker stays LTR; Rust paints the logical
    // run RTL and therefore reverses the insertion affinity inside each cell.
    const size_t at=logical[glyph]+size_t(spec.hebrew&&!native?!spec.after:spec.after);
    auto expected=spec.text;expected.insert(at,1,u'|');
    s.Click(x,y);
    s.Replace(u"|");
    ASSERT_EQ(s.Text(),expected);
    ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));
    ASSERT_EQ(s.Text(),spec.text);
    // Re-click after undo because history restores content, not cursor affinity.
    s.Click(x,y);
    if(at<spec.text.size()) {
      ASSERT_TRUE(s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey));
      // Native OnKeyDown passes the absolute end position to SetSelection's
      // count parameter. Its GetIndexRight also skips following line endings.
      // Preserve those original observations; Rust must select one character.
      auto selected=spec.text.substr(at,1);
      if(native) {
        size_t next=at+1;
        while(next<spec.text.size()&&(spec.text[next]==u'\r'||spec.text[next]==u'\n'))++next;
        selected=spec.text.substr(at,next);
      }
      EXPECT_EQ(s.Text(true),selected);
      s.Click(x,y);
    }
  }
  ASSERT_NO_FATAL_FAILURE(NormalizeViewport(s));
  Blur(s);
  EXPECT_TRUE(s.host->callbacks_valid);
  EXPECT_FALSE(s.host->callback_error);
}
void Common(Session& s,const Spec& spec,bool native) {
  auto visual=spec.text;
  if(spec.hebrew&&native)std::reverse(visual.begin(),visual.end());
  Put(s,visual);
}
void Workflow(const Spec& spec) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  std::array<std::unique_ptr<Session>,2> writers;
  std::array<Raster,2> originals,common;
  const auto libs=Implementations();
  for(size_t i=0;i<2;++i) {
    writers[i]=std::make_unique<Session>(libs[i]->api(),Fixture(spec),true,true);
    ASSERT_TRUE(writers[i]->valid());ASSERT_TRUE(writers[i]->xfa_loaded);
    Put(*writers[i],spec.text);
    originals[i]=writers[i]->DrawForComparison(kWidth,kHeight);
    Common(*writers[i],spec,libs[i]->name()=="native");
    common[i]=writers[i]->DrawForComparison(kWidth,kHeight);
    Put(*writers[i],spec.text);
  }
  CompareRaster(common[0],common[1],artifacts,"independent-common-scene",RasterTolerance::ConnectedError(1.0));
  for(size_t wi=0;wi<2;++wi) {
    SCOPED_TRACE("writer="+libs[wi]->name());auto& s=*writers[wi];
    ASSERT_NO_FATAL_FAILURE(Probe(s,spec,libs[wi]->name()=="native"));
    CompareRaster(originals[wi],s.DrawForComparison(kWidth,kHeight),artifacts,"probe-restores-original");
    const auto saved=SaveDocument(s.api,s.document,artifacts,libs[wi]->name()+"-hit-test");
    ASSERT_FALSE(saved.empty());
    for(size_t ri=0;ri<2;++ri) {
      SCOPED_TRACE("reader="+libs[ri]->name());Session reopened(libs[ri]->api(),saved,true,true);
      ASSERT_TRUE(reopened.valid());ASSERT_TRUE(reopened.xfa_loaded);
      ASSERT_TRUE(reopened.Focus(22,24));ASSERT_EQ(reopened.Text(),spec.text);Blur(reopened);
      ASSERT_NO_FATAL_FAILURE(Probe(reopened,spec,libs[ri]->name()=="native"));
      CompareRaster(originals[ri],reopened.DrawForComparison(kWidth,kHeight),artifacts,"reader-retains-original-direction");
      Common(reopened,spec,libs[ri]->name()=="native");
      CompareRaster(common[ri],reopened.DrawForComparison(kWidth,kHeight),artifacts,"saved-common-scene");
    }
  }
}
class CoverageTextHitTesting : public testing::TestWithParam<Spec> {};
TEST_P(CoverageTextHitTesting, RenderedGlyphHalvesDriveInsertionSelectionAndFourWayReplay) {
  ASSERT_NO_FATAL_FAILURE(Workflow(GetParam()));
}
INSTANTIATE_TEST_SUITE_P(VisualHitLocations,CoverageTextHitTesting,testing::Values(
  Spec{"SingleBefore",u"I",240,false},Spec{"SingleAfter",u"I",240,true},
  Spec{"AdjacentBefore",u"IIII",240,false},Spec{"AdjacentAfter",u"IIII",240,true},
  Spec{"SpacesBefore",u"I I I",240,false},Spec{"SpacesAfter",u"I I I",240,true},
  Spec{"NewlineBefore",u"II\nII",240,false},Spec{"NewlineAfter",u"II\nII",240,true},
  Spec{"WrappedBefore",u"II II II",60,false},Spec{"WrappedAfter",u"II II II",60,true},
  Spec{"EdgeWhitespaceBefore",u" I I ",240,false},Spec{"EdgeWhitespaceAfter",u" I I ",240,true},
  Spec{"HebrewSingleBefore",u"\u05d1",240,false,true},Spec{"HebrewSingleAfter",u"\u05d1",240,true,true},
  Spec{"HebrewRunBefore",u"\u05e0\u05d1",240,false,true},Spec{"HebrewRunAfter",u"\u05e0\u05d1",240,true,true}),
  [](const testing::TestParamInfo<Spec>& item){return item.param.name;});
}  // namespace
}  // namespace pdfoxium::integration
