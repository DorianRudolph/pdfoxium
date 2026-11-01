// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
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
struct Spec {const char* name;int width=88,height=24,resize_width=0,resize_height=0;bool multiline=false,wheel=false,edit=false,xfa=false;};
std::u16string Seed(const Spec& spec) {
  if(spec.xfa){std::u16string text;for(int i=0;i<11;++i)text+=u"\u05e0\u05d1 ";return text+u"\u05e0\u05d1";}
  if(spec.multiline){std::u16string text=u"FIRST LINE\r\n";for(int i=0;i<10;++i)text+=u"MIDDLE LINE\r\n";return text+u"LAST Z";}
  return u"START III III III III III III III III III III III III III III END";
}
std::string Hex(const std::u16string& text) {
  constexpr char digits[]="0123456789ABCDEF";std::string out="<FEFF";
  for(char16_t ch:text)for(int shift:{12,8,4,0})out+=digits[(ch>>shift)&15];
  return out+">";
}
std::string Resource(const char* name) {std::ifstream in(TestRoot()/name,std::ios::binary);if(!in.good())throw std::runtime_error("Missing pinned scroll font");return {std::istreambuf_iterator<char>(in),std::istreambuf_iterator<char>()};}
std::vector<uint8_t> XfaFixture() {
  static const auto font=Resource("tests/capi/resources/hebrew_unicode.ttf"),cmap=Resource("upstream/pdfium/testing/resources/hebrew_cmap.fragment");
  const std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" locale=\"en_US\" restoreState=\"auto\"><pageSet><pageArea name=\"p\"><occur min=\"1\" max=\"-1\"/><medium short=\"320pt\" long=\"240pt\"/><contentArea w=\"320pt\" h=\"240pt\"/></pageArea></pageSet>"
    "<field name=\"input\" x=\"20pt\" y=\"20pt\" w=\"80pt\" h=\"40pt\"><bind match=\"once\"/><ui><textEdit multiLine=\"1\"/></ui><font typeface=\"PdfoxiumHebrewTest\" size=\"20pt\"/><para lineHeight=\"20pt\"/><value><text>seed</text></value>"+
    forms::Event("if(typeof this.rawValue==='string')this.rawValue=this.rawValue.replace(/^\\n/,'');","initialize")+"</field><draw x=\"280pt\" y=\"215pt\" w=\"10pt\" h=\"10pt\"><value><rectangle><fill><color value=\"0,128,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw></subform></template>";
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 240] /Resources << /Font << /PdfoxiumHebrewTest 11 0 R >> >> >>",
    "<< /Fields [] /DR << /Font << /PdfoxiumHebrewTest 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
    PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
    PdfStream("",xml),PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),PdfStream("","</xdp:xdp>"),"null",
    "<< /Type /Font /Subtype /TrueType /BaseFont /PdfoxiumHebrewTest-Regular /FirstChar 0 /LastChar 5 /Widths [600 294 235 645 397 542] /FontDescriptor 12 0 R /ToUnicode 14 0 R >>",
    "<< /Type /FontDescriptor /FontName /PdfoxiumHebrewTest-Regular /Flags 4 /FontBBox [-210 -252 716 869] /Ascent 1069 /Descent -293 /CapHeight 869 /StemV 80 /FontFile2 13 0 R >>",PdfStream("/Length1 "+std::to_string(font.size()),font),PdfStream("",cmap)});
}
std::vector<uint8_t> Fixture(const Spec& spec) {
  if(spec.xfa)return XfaFixture();
  const auto rect="20 "+std::to_string(200-spec.height)+" "+std::to_string(20+spec.width)+" 200";
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 240] /Annots [6 0 R] /Resources << /Font << /Cour 5 0 R >> >> /Contents 7 0 R >>",
    "<< /Fields [6 0 R] /NeedAppearances true /DA (/Cour 12 Tf 0 g) /DR << /Font << /Cour 5 0 R >> >> >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>",
    "<< /Type /Annot /Subtype /Widget /P 3 0 R /FT /Tx /T (input) /F 4 /Ff "+std::to_string(spec.multiline?4096:0)+" /Rect ["+rect+"] /V "+Hex(Seed(spec))+" /BS << /W 1 >> /MK << /BC [0 0 0] /BG [1 1 1] >> >>",
    PdfStream("","0 .5 0 rg 280 15 10 10 re f")});
}
void Key(Session& s,int code,int flags=0){s.api.FORM_OnKeyDown(s.form,s.page,code,flags);}
void Focus(Session& s,const Spec& spec){ASSERT_TRUE(s.Focus(24,spec.xfa?24:195));}
void Blur(Session& s){s.api.FORM_ForceToKillFocus(s.form);s.Click(310,230);s.api.FORM_ForceToKillFocus(s.form);}
void Put(Session& s,const Spec& spec,const std::u16string& value){Focus(s,spec);ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(value);ASSERT_EQ(s.Text(),value);Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);Blur(s);}
int Blue(const Raster& image,const Spec& spec) {
  const int top=spec.xfa?20:40;int result=0;const auto scale=image.render_scale;
  for(int y=int(top*scale);y<int((top+spec.height)*scale);++y)for(int x=int(20*scale);x<int((20+spec.width)*scale);++x){const auto at=(size_t(y)*image.width+x)*4;
    result+=image.pixels[at]>image.pixels[at+2]+20&&image.pixels[at]>image.pixels[at+1]+5;}
  return result;
}
Raster Endpoint(Session& s,const Spec& spec,const std::u16string& value,bool end) {
  Focus(s,spec);s.Click(24,spec.xfa?24:195);Key(s,end?FWL_VKEY_End:FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
  Key(s,end?FWL_VKEY_Left:FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey);
  EXPECT_EQ(s.Text(),value);EXPECT_EQ(s.Text(true),end?value.substr(value.size()-1):value.substr(0,1));
  const auto frame=s.DrawForComparison(320,240);EXPECT_GT(Blue(frame,spec),0);return frame;
}
void Normalize(Session& s,const Spec& spec){Focus(s,spec);Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);s.Click(24,spec.xfa?24:195);Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);Blur(s);}
void Wheel(Session& s,const Spec& spec,int delta,const std::u16string& value){FS_POINTF point{30.f,float(spec.xfa?30:185)};const auto selected=s.Text(true);s.api.FORM_OnMouseWheel(s.form,s.page,0,&point,0,delta);EXPECT_EQ(s.Text(),value);EXPECT_EQ(s.Text(true),selected);}
void Resize(Session& s,Spec& spec) {
  Blur(s);{
    Handle annot(s.api.FPDFPage_GetAnnot(s.page,0),s.api.FPDFPage_CloseAnnot);ASSERT_NE(annot.get(),nullptr);
    FS_RECTF r{20.f,200.f,float(20+spec.resize_width),float(200-spec.resize_height)};
    ASSERT_TRUE(s.api.FPDFAnnot_SetRect(annot.get(),&r));FS_RECTF actual{};ASSERT_TRUE(s.api.FPDFAnnot_GetRect(annot.get(),&actual));
    EXPECT_EQ(actual.left,r.left);EXPECT_EQ(actual.top,r.top);EXPECT_EQ(actual.right,r.right);EXPECT_EQ(actual.bottom,r.bottom);
  }
  s.ClosePage(0);s.page=s.host->Page(0);ASSERT_NE(s.page,nullptr);spec.width=spec.resize_width;spec.height=spec.resize_height;
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& name){CompareRaster(a,b,artifacts,name,RasterTolerance::ConnectedError(1.0));}
void BuiltinSelection(forms::FormHost& host) {
  // The optional OutputSelectedRect callback delegates selection painting to
  // the embedder. This scenario checks the engine's own visible selection, so
  // decline that optional service instead of recording rectangles without paint.
  host.FFI_OutputSelectedRect=nullptr;
}
void AcroWorkflow(const Spec& input) {
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());Spec spec=input;auto value=Seed(spec);Session s(writer->api(),Fixture(spec),false,false,2,BuiltinSelection);ASSERT_TRUE(s.valid());
    Focus(s,spec);ASSERT_EQ(s.Text(),value);frames[backend].push_back(Endpoint(s,spec,value,false));frames[backend].push_back(Endpoint(s,spec,value,true));
    if(spec.wheel){Wheel(s,spec,-20,value);frames[backend].push_back(Endpoint(s,spec,value,false));Wheel(s,spec,20,value);frames[backend].push_back(Endpoint(s,spec,value,true));Wheel(s,spec,0,value);}
    if(spec.edit){s.Replace(u"Q");auto changed=value;changed.back()=u'Q';ASSERT_EQ(s.Text(),changed);ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));ASSERT_EQ(s.Text(),value);ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));value=changed;ASSERT_EQ(s.Text(),value);}
    if(spec.resize_width){ASSERT_NO_FATAL_FAILURE(Resize(s,spec));Focus(s,spec);ASSERT_EQ(s.Text(),value);}
    const auto home=Endpoint(s,spec,value,false),end=Endpoint(s,spec,value,true);frames[backend].push_back(home);frames[backend].push_back(end);Normalize(s,spec);
    EXPECT_EQ(s.Field(0),value);const auto final=s.DrawForComparison(320,240);frames[backend].push_back(final);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+input.name);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());Session reopened(reader->api(),saved,false,false,2,BuiltinSelection);ASSERT_TRUE(reopened.valid());Focus(reopened,spec);ASSERT_EQ(reopened.Text(),value);
      Compare(home,Endpoint(reopened,spec,value,false),artifacts,"saved-home-selection");Compare(end,Endpoint(reopened,spec,value,true),artifacts,"saved-end-selection");Normalize(reopened,spec);Compare(final,reopened.DrawForComparison(320,240),artifacts,"saved-viewport");
    }
    EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);++backend;
  }
  ASSERT_EQ(frames[0].size(),frames[1].size());for(size_t i=0;i<frames[0].size();++i)Compare(frames[0][i],frames[1][i],artifacts,"scroll-selection-stage-"+std::to_string(i));
}
void XfaWorkflow(const Spec& spec) {
  Artifacts artifacts;const auto libs=Implementations();auto value=Seed(spec);if(spec.edit)value.back()=u'\u05de';
  std::array<Raster,2> original,common;
  for(size_t i=0;i<2;++i){Session ref(libs[i]->api(),Fixture(spec),true,true);ASSERT_TRUE(ref.valid());ASSERT_TRUE(ref.xfa_loaded);Put(ref,spec,value);Normalize(ref,spec);original[i]=ref.DrawForComparison(320,240);Put(ref,spec,u"\u05d1");common[i]=ref.DrawForComparison(320,240);}
  Compare(common[0],common[1],artifacts,"rtl-singleton-control");
  for(size_t wi=0;wi<2;++wi){SCOPED_TRACE(libs[wi]->name());Session s(libs[wi]->api(),Fixture(spec),true,true);ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);auto seed=Seed(spec);Put(s,spec,seed);
    (void)Endpoint(s,spec,seed,false);(void)Endpoint(s,spec,seed,true);Wheel(s,spec,-8,seed);(void)Endpoint(s,spec,seed,false);Wheel(s,spec,8,seed);(void)Endpoint(s,spec,seed,true);
    if(spec.edit){s.Replace(u"\u05de");ASSERT_EQ(s.Text(),value);ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));ASSERT_EQ(s.Text(),seed);ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));ASSERT_EQ(s.Text(),value);}
    Normalize(s,spec);Compare(original[wi],s.DrawForComparison(320,240),artifacts,"rtl-original-viewport");const auto saved=SaveDocument(s.api,s.document,artifacts,libs[wi]->name()+spec.name);ASSERT_FALSE(saved.empty());
    for(size_t ri=0;ri<2;++ri){SCOPED_TRACE(libs[ri]->name());Session replay(libs[ri]->api(),saved,true,true);ASSERT_TRUE(replay.valid());ASSERT_TRUE(replay.xfa_loaded);Focus(replay,spec);ASSERT_EQ(replay.Text(),value);
      (void)Endpoint(replay,spec,value,false);(void)Endpoint(replay,spec,value,true);Normalize(replay,spec);Compare(original[ri],replay.DrawForComparison(320,240),artifacts,"rtl-reader-original-viewport");Put(replay,spec,u"\u05d1");Compare(common[ri],replay.DrawForComparison(320,240),artifacts,"rtl-reader-singleton-control");}
  }
}
class CoverageScrollSelectionSequences:public testing::TestWithParam<Spec>{};
TEST_P(CoverageScrollSelectionSequences, ScrolledSelectionRemainsVisibleThroughEditResizeAndSave) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);const auto spec=GetParam();
  if(spec.xfa){
    ASSERT_NO_FATAL_FAILURE(XfaWorkflow(spec));
  }else{
    ASSERT_NO_FATAL_FAILURE(AcroWorkflow(spec));
  }
}
INSTANTIATE_TEST_SUITE_P(ScrollSelectionTransitions,CoverageScrollSelectionSequences,testing::Values(
  Spec{"HorizontalEndpoints",72,24},Spec{"HorizontalWheelDoesNotChangeValue",88,24,0,0,false,true},Spec{"HorizontalReplacementHistory",88,24,0,0,false,false,true},
  Spec{"HorizontalWidthGrowth",80,24,160,24},Spec{"HorizontalWidthShrink",160,24,72,24},
  Spec{"MultilineEndpoints",120,40,0,0,true},Spec{"MultilineWheelClampsThenCaretReturns",120,40,0,0,true,true},Spec{"MultilineReplacementHistory",120,48,0,0,true,false,true},
  Spec{"MultilineHeightGrowth",120,40,120,88,true},Spec{"MultilineHeightShrink",120,88,120,40,true},
  Spec{"WrappedRtlEndpointsAfterWheel",80,40,0,0,true,true,false,true},Spec{"WrappedRtlReplacementUndoAndReplay",80,40,0,0,true,true,true,true}),
  [](const testing::TestParamInfo<Spec>& item){return item.param.name;});
} // namespace
} // namespace pdfoxium::integration
