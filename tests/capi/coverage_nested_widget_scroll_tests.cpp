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
#include <cmath>
#include <stdexcept>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
enum class Kind { Text, Combo, List };
struct Spec {const char* name;Kind kind;int depth;int rotation=0;bool resize=false;};
struct Box {int x,y,w,h;};
Box Bounds(const Spec& spec,bool resized=false) {
  const int w=resized?144:100,h=spec.kind==Kind::List?64:24;
  switch(spec.rotation){case 90:return {120,120-w,h,w};case 180:return {120-w,120-h,w,h};case 270:return {120-h,120,h,w};default:return {120,120,w,h};}
}
std::string Path(int depth){std::string path;for(int i=0;i<depth;++i)path+="outer"+std::to_string(i)+".";return path+"input";}
std::vector<uint8_t> Fixture(const Spec& spec,bool nested) {
  const int depth=nested?spec.depth:0;int x=120,y=120;std::string body;
  constexpr int dx[]={30,20,11},dy[]={40,30,13};
  for(int i=0;i<depth;++i){x-=dx[i];y-=dy[i];body+="<subform name=\"outer"+std::to_string(i)+"\" layout=\"position\" x=\""+std::to_string(dx[i])+"pt\" y=\""+std::to_string(dy[i])+"pt\" w=\"260pt\" h=\"260pt\">";}
  body+="<field name=\"input\" x=\""+std::to_string(x)+"pt\" y=\""+std::to_string(y)+"pt\" w=\"100pt\" h=\""+std::string(spec.kind==Kind::List?"64":"24")+"pt\" rotate=\""+std::to_string(spec.rotation)+"\"><bind match=\"once\"/><ui>";
  body+=spec.kind==Kind::Text?"<textEdit/>":spec.kind==Kind::Combo?"<choiceList open=\"userControl\" textEntry=\"1\"/>":"<choiceList open=\"always\"/>";
  body+="</ui><font typeface=\"Helvetica\" size=\"12pt\"/>";
  if(spec.kind!=Kind::Text)body+="<items><text>Alpha label extends well beyond the available width</text><text>Beta label extends well beyond the available width</text><text>Gamma label extends well beyond the available width</text></items><items save=\"1\"><text>a</text><text>b</text><text>c</text></items>";
  body+="<value><text>"+std::string(spec.kind==Kind::Text?"seed":"a")+"</text></value>"+forms::Event("if(typeof this.rawValue==='string')this.rawValue=this.rawValue.replace(/^\\n+/,'');for(var j=0;j<this.nodes.length;j++){var n=this.nodes.item(j);if(n.className!=='items')continue;for(var k=0;k<n.nodes.length;k++){var v=n.nodes.item(k);v.value=String(v.value).replace(/^\\n+/,'');}}","initialize")+"</field>";
  for(int i=0;i<depth;++i)body+="</subform>";
  const auto field="xfa.form.root.resolveNode('"+Path(depth)+"')";
  body+="<field name=\"observe\" x=\"230pt\" y=\"260pt\" w=\"80pt\" h=\"24pt\"><ui><button/></ui>"+forms::Event("var f="+field+";xfa.host.messageBox(String(f.rawValue));","click")+"</field>";
  body+="<field name=\"resize\" x=\"230pt\" y=\"290pt\" w=\"80pt\" h=\"24pt\"><ui><button/></ui>"+forms::Event("var f="+field+";f.w='144pt';xfa.layout.relayout();xfa.host.messageBox('resized');","click")+"</field>";
  body+="<draw x=\"285pt\" y=\"235pt\" w=\"10pt\" h=\"10pt\"><value><rectangle><fill><color value=\"0,128,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
  const std::string preamble="<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
  const std::string config="<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
  std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" locale=\"en_US\" restoreState=\"auto\"><pageSet><pageArea name=\"p\"><occur min=\"1\" max=\"-1\"/><medium short=\"320pt\" long=\"320pt\"/><contentArea w=\"320pt\" h=\"320pt\"/></pageArea></pageSet>"+body+"</subform></template>";
  // Avoid the independently documented native CDATA chunk-boundary defect.
  // Opening-marker padding belongs OUTSIDE the script: native otherwise
  // treats leading text whitespace as its content and saves that whitespace
  // as a form-packet override, discarding the following CDATA JavaScript.
  // Closing-marker padding is harmless trailing whitespace inside JavaScript.
  for(size_t i=0;i<xml.size();){
    const size_t n=xml.compare(i,9,"<![CDATA[")==0?9:xml.compare(i,3,"]]>")==0?3:0;
    if(!n){++i;continue;}
    const auto offset=preamble.size()+config.size()+i;
    if(offset/1024!=(offset+n-1)/1024){
      const auto pad=1024-offset%1024;
      const auto insertion=n==9?xml.rfind("<script",i):i;
      if(insertion==std::string::npos)throw std::runtime_error("CDATA without script in nested fixture");
      xml.insert(insertion,pad,' ');i+=pad;
    }
    i+=n;
  }
  std::vector<std::string> objects={"<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 320] /Resources << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> >>",
    "<< /Fields [] /DR << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
    PdfStream("",preamble),PdfStream("",config),PdfStream("",xml),PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root/></xfa:data></xfa:datasets>"),PdfStream("","</xdp:xdp>"),"null"};
  auto font=forms::PinnedXfaArimoFontObjects();for(auto& object:font)objects.push_back(std::move(object));return PdfFixture(objects);
}
void Key(Session& s,int code,int flags=0){s.api.FORM_OnKeyDown(s.form,s.page,code,flags);}
void Hit(Session& s,const Spec& spec,bool resized=false) {
  const auto b=Bounds(spec,resized);const double x=b.x+b.w/2.,y=b.y+b.h/2.;
  EXPECT_EQ(s.api.FPDFPage_HasFormFieldAtPoint(s.form,s.page,x,y),spec.kind==Kind::Text?FPDF_FORMFIELD_XFA_TEXTFIELD:spec.kind==Kind::Combo?FPDF_FORMFIELD_XFA_COMBOBOX:FPDF_FORMFIELD_XFA_LISTBOX);
  ASSERT_TRUE(s.Focus(x,y));s.Click(x,y);
}
void Observe(Session& s,const std::u16string& expected) {
  s.host->alerts.clear();s.Click(250,270);ASSERT_EQ(s.host->alerts.size(),1u);EXPECT_EQ(std::get<0>(s.host->alerts.front()),expected);
}
void Resize(Session& s) {
  s.host->alerts.clear();s.Click(250,300);ASSERT_EQ(s.host->alerts.size(),1u);EXPECT_EQ(std::get<0>(s.host->alerts.front()),u"resized");
}
void Normalize(Session& s,const Spec& spec,bool resized) {
  Hit(s,spec,resized);
  if(spec.kind==Kind::List){Key(s,FWL_VKEY_Home);Key(s,FWL_VKEY_Down);}else{Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);Key(s,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey);EXPECT_EQ(s.Text(),u"ReadQ");EXPECT_EQ(s.Text(true),u"R");}
  s.api.FORM_OnMouseMove(s.form,s.page,0,315,245);
}
void Exercise(Session& s,const Spec& spec,Raster& image) {
  Hit(s,spec);
  if(spec.kind==Kind::List){Key(s,FWL_VKEY_Home);Key(s,FWL_VKEY_Down);}else{
    ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(u"Ready");EXPECT_EQ(s.Text(),u"Ready");
    Key(s,FWL_VKEY_End,FWL_EVENTFLAG_ControlKey);Key(s,FWL_VKEY_Left,FWL_EVENTFLAG_ShiftKey);EXPECT_EQ(s.Text(true),u"y");s.Replace(u"Q");EXPECT_EQ(s.Text(),u"ReadQ");
  }
  const auto expected=spec.kind==Kind::List?u"b":u"ReadQ";
  Observe(s,expected);Hit(s,spec);
  if(spec.kind==Kind::List){Key(s,FWL_VKEY_Home);Key(s,FWL_VKEY_Down);}
  // No native or Rust XFA list creates a horizontal scrollbar here. The
  // public wheel still must preserve the logical choice and clipped scene.
  const auto b=Bounds(spec);FS_POINTF point{float(b.x+b.w/2),float(b.y+b.h/2)};
  for(int dx:{-40,40,0})s.api.FORM_OnMouseWheel(s.form,s.page,0,&point,dx,0);
  Observe(s,expected);
  if(spec.resize){Resize(s);Observe(s,expected);}
  Normalize(s,spec,spec.resize);image=s.DrawForComparison(320,320);const auto visible=Bounds(spec,spec.resize);
  forms::ExpectInk(image,visible.x,visible.y,visible.x+visible.w,visible.y+visible.h);
  const auto clipped=CropLogicalRaster(image,visible.x,visible.y,visible.w,visible.h);size_t blue=0;
  for(size_t i=0;i<clipped.pixels.size();i+=4)blue+=clipped.pixels[i]>clipped.pixels[i+2]+20&&clipped.pixels[i+1]>clipped.pixels[i+2]+5;
  EXPECT_GT(blue,0u);
  EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& name){CompareRaster(a,b,artifacts,name,RasterTolerance::ConnectedError(1.0));}
// Native ComboBox draws its edit, then WidgetMgr draws that same child
// again (docs/capi-known-native-differences.md, repeated composite paints).
// Compare actual glyph/selection geometry inside precisely that edit, retaining
// the original rasters and unchanged full-pixel comparison everywhere else.
std::array<int,5> PaintedBounds(const Raster& image,bool selection) {
  std::array<int,5> box{image.width,image.height,-1,-1,0};
  for(int y=0;y<image.height;++y)for(int x=0;x<image.width;++x){const auto at=(size_t(y)*image.width+x)*4;
    const bool painted=selection?image.pixels[at]>image.pixels[at+2]+20&&image.pixels[at+1]>image.pixels[at+2]+5:
      image.pixels[at]<180&&image.pixels[at+1]<180&&image.pixels[at+2]<180;
    if(painted){box[0]=std::min(box[0],x);box[1]=std::min(box[1],y);box[2]=std::max(box[2],x);box[3]=std::max(box[3],y);++box[4];}
  }
  return box;
}
void CrossEngine(const Spec& spec,const Raster& native,const Raster& rust,Artifacts& artifacts,const std::string& label) {
  if(spec.kind!=Kind::Combo){Compare(native,rust,artifacts,label);return;}
  artifacts.Image(label+"-native-original",native);artifacts.Image(label+"-rust-original",rust);
  // The pinned XFA theme supplies a 9pt button; ComboBox::Layout reserves
  // exactly those 9pt before assigning the remaining client rectangle to edit.
  const Box edit=spec.rotation==90?Box{120,29,24,91}:Box{120,120,91,24};
  const auto a=CropLogicalRaster(native,edit.x,edit.y,edit.w,edit.h),b=CropLogicalRaster(rust,edit.x,edit.y,edit.w,edit.h);
  for(bool selection:{false,true}){const auto first=PaintedBounds(a,selection),second=PaintedBounds(b,selection);EXPECT_GT(first[4],selection?0:20);EXPECT_GT(second[4],selection?0:20);
    for(size_t i=0;i<4;++i)EXPECT_NEAR(first[i],second[i],std::ceil(native.render_scale));}
  const std::array<Box,4> outside={Box{0,0,320,edit.y},Box{0,edit.y+edit.h,320,320-edit.y-edit.h},Box{0,edit.y,edit.x,edit.h},Box{edit.x+edit.w,edit.y,320-edit.x-edit.w,edit.h}};
  for(size_t i=0;i<outside.size();++i){const auto r=outside[i];Compare(CropLogicalRaster(native,r.x,r.y,r.w,r.h),CropLogicalRaster(rust,r.x,r.y,r.w,r.h),artifacts,label+"-outside-edit-"+std::to_string(i));}
}
class CoverageNestedWidgetScroll:public testing::TestWithParam<Spec>{};
TEST_P(CoverageNestedWidgetScroll, NestedCoordinatesMatchIndependentFlatControlsAcrossInputAndSave) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);const auto spec=GetParam();Artifacts artifacts;const auto libraries=Implementations();std::array<Raster,2> controls,original;
  for(size_t wi=0;wi<2;++wi){
    SCOPED_TRACE(libraries[wi]->name());
    Session flat(libraries[wi]->api(),Fixture(spec,false),true,true);ASSERT_TRUE(flat.valid());ASSERT_TRUE(flat.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(Exercise(flat,spec,controls[wi]));
    Session nested(libraries[wi]->api(),Fixture(spec,true),true,true);ASSERT_TRUE(nested.valid());ASSERT_TRUE(nested.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(Exercise(nested,spec,original[wi]));
    Compare(controls[wi],original[wi],artifacts,"nested-flat-coordinates");
    Observe(nested,spec.kind==Kind::List?u"b":u"ReadQ");
    const auto saved=SaveDocument(nested.api,nested.document,artifacts,libraries[wi]->name()+spec.name);ASSERT_FALSE(saved.empty());
    for(size_t ri=0;ri<2;++ri){
      SCOPED_TRACE(libraries[ri]->name());
      Session replay(libraries[ri]->api(),saved,true,true);ASSERT_TRUE(replay.valid());ASSERT_TRUE(replay.xfa_loaded);
      Observe(replay,spec.kind==Kind::List?u"b":u"ReadQ");
      // Reapply the real geometry event: XFA layout state is not PDF datasets.
      if(spec.resize)Resize(replay);
      Normalize(replay,spec,spec.resize);
      const auto actual=replay.DrawForComparison(320,320);
      // Fresh flat control from the reader avoids writer-specific layout state.
      Session control(libraries[ri]->api(),Fixture(spec,false),true,true);ASSERT_TRUE(control.valid());ASSERT_TRUE(control.xfa_loaded);
      Raster expected;
      ASSERT_NO_FATAL_FAILURE(Exercise(control,spec,expected));
      Compare(expected,actual,artifacts,"saved-reader-flat-control");
    }
  }
  CrossEngine(spec,controls[0],controls[1],artifacts,"native-rust-flat-control");CrossEngine(spec,original[0],original[1],artifacts,"native-rust-nested-selection");
}
INSTANTIATE_TEST_SUITE_P(NestedCoordinateSequences,CoverageNestedWidgetScroll,testing::Values(
  Spec{"TextOneOuter",Kind::Text,1},Spec{"TextTwoOuters",Kind::Text,2},Spec{"TextThreeOuters",Kind::Text,3},
  Spec{"TextQuarterTurn",Kind::Text,2,90},Spec{"TextHalfTurn",Kind::Text,2,180},Spec{"TextThreeQuarterTurn",Kind::Text,3,270},
  Spec{"EditableComboTwoOuters",Kind::Combo,2},Spec{"EditableComboQuarterTurn",Kind::Combo,3,90},Spec{"NestedTextPublicWidthResize",Kind::Text,2,0,true},
  Spec{"WideListTwoOuters",Kind::List,2},Spec{"WideListQuarterTurn",Kind::List,3,90},Spec{"WideListThreeQuarterTurn",Kind::List,2,270}),
  [](const testing::TestParamInfo<Spec>& info){return info.param.name;});
}  // namespace
}  // namespace pdfoxium::integration
