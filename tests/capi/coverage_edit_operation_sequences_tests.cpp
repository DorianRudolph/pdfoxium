// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using Text = std::u32string;
struct Rng {
  uint32_t state;
  size_t Take(size_t limit) { state=state*1664525u+1013904223u;return state%limit; }
};
std::u16string Utf16(const Text& value) {
  std::u16string out;
  for(char32_t c:value) {
    if(c<=0xffff)out+=char16_t(c);
    else {c-=0x10000;out+=char16_t(0xd800+(c>>10));out+=char16_t(0xdc00+(c&1023));}
  }
  return out;
}
Text Scalars(const std::u16string& value) {
  Text out;
  for(size_t i=0;i<value.size();++i) {
    uint32_t c=value[i];
    if(c=='\r') {if(i+1<value.size()&&value[i+1]=='\n')++i;out+=U'\n';continue;}
    if(c>=0xd800&&c<=0xdbff) {
      if(i+1>=value.size()||value[i+1]<0xdc00||value[i+1]>0xdfff) {ADD_FAILURE()<<"Malformed focused UTF-16";return {};}
      c=0x10000+((c-0xd800)<<10)+(value[++i]-0xdc00);
    }
    out+=char32_t(c);
  }
  return out;
}
struct Model {
  std::vector<Text> states;
  size_t at=0;
  explicit Model(Text initial):states{std::move(initial)} {}
  const Text& Value()const{return states[at];}
  bool Undoable()const{return at!=0;}
  bool Redoable()const{return at+1!=states.size();}
  void Edit(size_t start,size_t count,const Text& added) {
    Text next=Value();next.replace(start,count,added);
    EXPECT_NE(next,Value())<<"Every generated edit must have a real effect";
    states.resize(at+1);states.push_back(std::move(next));++at;
  }
};
void Key(Session& s,int key,int flags=0){s.api.FORM_OnKeyDown(s.form,s.page,key,flags);}
void Select(Session& s,const Model& model,size_t start,size_t count) {
  ASSERT_LE(start+count,model.Value().size());
  Key(s,FWL_VKEY_Home,FWL_EVENTFLAG_ControlKey);
  for(size_t i=0;i<start;++i)Key(s,FWL_VKEY_Right);
  for(size_t i=0;i<count;++i)Key(s,FWL_VKEY_Right,FWL_EVENTFLAG_ShiftKey);
  ASSERT_EQ(Scalars(s.Text(true)),model.Value().substr(start,count));
}
void Check(Session& s,const Model& model,size_t step) {
  ASSERT_EQ(Scalars(s.Text()),model.Value())<<"step="<<step;
  EXPECT_EQ(bool(s.api.FORM_CanUndo(s.form,s.page)),model.Undoable())<<"step="<<step;
  EXPECT_EQ(bool(s.api.FORM_CanRedo(s.form,s.page)),model.Redoable())<<"step="<<step;
}
void Paste(Session& s,Model& model,size_t start,size_t count,const Text& value) {
  ASSERT_NO_FATAL_FAILURE(Select(s,model,start,count));
  s.Replace(Utf16(value));model.Edit(start,count,value);
}
void Character(Session& s,Model& model,size_t start,char32_t value) {
  ASSERT_LT(value,128u);
  ASSERT_NO_FATAL_FAILURE(Select(s,model,start,0));
  ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,int(value),0));
  model.Edit(start,0,Text(1,value));
}
void Sequence(Session& s,Model& model,Rng& rng,bool xfa) {
  // Native FDE's Right navigation skips newline boundaries; that separate
  // oracle defect must not define this model's selection units. AcroForm and
  // scalar unit traces include LF; XFA's dedicated history tests cover newline
  // records while these shared XFA traces use supported BMP cursor stops.
  const std::array<Text,5> words=xfa
      ?std::array<Text,5>{{U"caf\u00e9",U"\u00c5B",U"ab",U"\u00f1Z",U"Q"}}
      :std::array<Text,5>{{U"caf\u00e9",U"\u00c5B",U"a\nb",U"\u00f1Z",U"\nQ"}};
  for(size_t step=0;step<24;++step) {
    SCOPED_TRACE(step);
    const auto size=model.Value().size();
    switch(step%12) {
      case 2:case 5:case 8:
        ASSERT_TRUE(model.Undoable());ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));--model.at;break;
      case 3:case 9:
        ASSERT_TRUE(model.Redoable());ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));++model.at;break;
      case 4: {
        const size_t removed=rng.Take(size);
        if(rng.Take(2)) {
          ASSERT_NO_FATAL_FAILURE(Select(s,model,removed+1,0));
          ASSERT_TRUE(s.api.FORM_OnChar(s.form,s.page,8,0));
        } else {
          ASSERT_NO_FATAL_FAILURE(Select(s,model,removed,0));
          Key(s,FWL_VKEY_Delete);
        }
        model.Edit(removed,1,{});break;
      }
      case 10:
        ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
        EXPECT_EQ(Scalars(s.Text(true)),model.Value());s.Replace(u"");model.Edit(0,size,{});break;
      case 1:case 7: {
        // The native XFA ShiftRight range-count bug is separately documented.
        // Prefix/full selections exercise the valid shared contract; arbitrary
        // ranges are covered here by AcroForm and by the UTF-32 unit sequences.
        const size_t start=step%12==7||xfa?0:rng.Take(size);
        const size_t count=step%12==7?size:1+rng.Take(size-start);
        Text value=words[rng.Take(words.size())];
        if(value==model.Value().substr(start,count))value+=U'X';
        ASSERT_NO_FATAL_FAILURE(Paste(s,model,start,count,value));break;
      }
      case 0:
        ASSERT_NO_FATAL_FAILURE(Character(s,model,rng.Take(size+1),char32_t(U'A'+rng.Take(20))));break;
      default: {
        const auto start=rng.Take(size+1);const auto value=words[rng.Take(words.size())];
        ASSERT_NO_FATAL_FAILURE(Paste(s,model,start,0,value));break;
      }
    }
    ASSERT_NO_FATAL_FAILURE(Check(s,model,step));
  }
  // The existing native saved-XFA XML leading-LF boundary is kept separate
  // from operation ordering: legitimate internal newlines still round-trip.
  ASSERT_NO_FATAL_FAILURE(Character(s,model,0,U'Q'));
  ASSERT_NO_FATAL_FAILURE(Character(s,model,model.Value().size(),U'Z'));
  ASSERT_NO_FATAL_FAILURE(Check(s,model,24));
}
std::vector<uint8_t> Fixture(bool xfa) {
  std::vector<std::string> objects;
  if(!xfa) {
    objects={
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 300] /Annots [5 0 R 6 0 R] /Contents 7 0 R /Resources << /Font << /Helv 11 0 R >> >> >>",
      "<< /Fields [5 0 R 6 0 R] /NeedAppearances true /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 11 0 R >> >> >>",
      "<< /Type /Annot /Subtype /Widget /FT /Tx /T (input) /F 4 /Ff 4096 /P 3 0 R /Rect [8 82 288 292] /V (Seed) >>",
      "<< /Type /Annot /Subtype /Widget /FT /Tx /T (sink) /F 4 /P 3 0 R /Rect [8 18 288 40] /V (sink) >>",
      PdfStream("","0 .5 0 rg 300 10 10 10 re f"),"null","null","null"};
  } else {
    const std::string clean=forms::Event("if(typeof this.rawValue==='string')this.rawValue=this.rawValue.replace(/^\\n/,'');","initialize");
    const std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"position\" locale=\"en_US\" restoreState=\"auto\"><pageSet><pageArea><occur min=\"1\" max=\"-1\"/><medium short=\"320pt\" long=\"300pt\"/><contentArea w=\"320pt\" h=\"300pt\"/></pageArea></pageSet>"
      "<field name=\"input\" x=\"8pt\" y=\"8pt\" w=\"280pt\" h=\"210pt\"><bind match=\"once\"/><ui><textEdit multiLine=\"1\"/></ui><font typeface=\"Helvetica\" size=\"12pt\"/><para lineHeight=\"14pt\"/><value><text>Seed</text></value>"+clean+"</field>"
      "<field name=\"sink\" x=\"8pt\" y=\"260pt\" w=\"280pt\" h=\"22pt\"><bind match=\"none\"/><ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"12pt\"/><value><text>sink</text></value>"+clean+"</field>"
      "<draw x=\"300pt\" y=\"280pt\" w=\"10pt\" h=\"10pt\"><value><rectangle><fill><color value=\"0,128,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw></subform></template>";
    const auto preamble="<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">";
    const auto config="<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>";
    std::string template_xml=xml;
    for(size_t i=0;i<template_xml.size();) {
      const size_t n=template_xml.compare(i,9,"<![CDATA[")==0?9:template_xml.compare(i,3,"]]>")==0?3:0;
      if(!n){++i;continue;}
      const size_t offset=std::strlen(preamble)+std::strlen(config)+i;
      if(offset/1024!=(offset+n-1)/1024){const size_t padding=1024-offset%1024;template_xml.insert(i,padding,' ');i+=padding;}
      i+=n;
    }
    objects={
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 300] /Resources << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> >>",
      "<< /Fields [] /DR << /Font << /Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("",preamble),PdfStream("",config),PdfStream("",template_xml),
      PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root xfa:dataNode=\"dataGroup\"/></xfa:data></xfa:datasets>"),PdfStream("","</xdp:xdp>"),"null"};
  }
  auto font=forms::PinnedXfaArimoFontObjects();for(auto& object:font)objects.push_back(std::move(object));
  return PdfFixture(objects);
}
void FocusInput(Session& s,bool xfa){ASSERT_TRUE(s.Focus(12,xfa?16:285));}
void Blur(Session& s,bool xfa) {
  ASSERT_TRUE(s.Focus(12,xfa?268:28));
  s.api.FORM_ForceToKillFocus(s.form);s.Click(315,295);s.api.FORM_OnMouseMove(s.form,s.page,0,315,295);
}
void ObserveRaster(const Raster& image) {
  forms::ExpectInk(image,8,8,288,218);
  const size_t p=(size_t(285)*320+305)*4;
  ASSERT_EQ(image.width,320);ASSERT_EQ(image.height,300);
  EXPECT_EQ(image.pixels.at(p),0);EXPECT_EQ(image.pixels.at(p+1),128);EXPECT_EQ(image.pixels.at(p+2),0);
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,RasterTolerance::ConnectedError(1.0));
}
void ProbeReload(Session& s,const Text& expected,bool xfa) {
  ASSERT_NO_FATAL_FAILURE(FocusInput(s,xfa));
  Model fresh(expected);
  ASSERT_NO_FATAL_FAILURE(Check(s,fresh,0));
  ASSERT_NO_FATAL_FAILURE(Character(s,fresh,expected.size(),U'Z'));
  ASSERT_NO_FATAL_FAILURE(Check(s,fresh,1));
  ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));--fresh.at;
  ASSERT_NO_FATAL_FAILURE(Check(s,fresh,2));
  ASSERT_TRUE(s.api.FORM_Redo(s.form,s.page));++fresh.at;
  ASSERT_NO_FATAL_FAILURE(Check(s,fresh,3));
  ASSERT_TRUE(s.api.FORM_Undo(s.form,s.page));--fresh.at;
  ASSERT_NO_FATAL_FAILURE(Check(s,fresh,4));
  ASSERT_NO_FATAL_FAILURE(Blur(s,xfa));
}
void Workflow(bool xfa,uint32_t seed) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);Artifacts artifacts;
  std::array<std::vector<Raster>,2> frames;size_t backend=0;
  const auto input=Fixture(xfa);
  for(const auto* writer:Implementations()) {
    Rng rng{seed};Text expected=U"Seed";std::vector<uint8_t> current=input;
    for(size_t chunk=0;chunk<3;++chunk) {
      SCOPED_TRACE(writer->name()+" seed="+std::to_string(seed)+" chunk="+std::to_string(chunk));
      Session s(writer->api(),current,true,xfa);ASSERT_TRUE(s.valid());if(xfa){ASSERT_TRUE(s.xfa_loaded);}
      ASSERT_NO_FATAL_FAILURE(FocusInput(s,xfa));Model model(expected);
  ASSERT_NO_FATAL_FAILURE(Check(s,model,0));
      ASSERT_NO_FATAL_FAILURE(Sequence(s,model,rng,xfa));expected=model.Value();
      ASSERT_NO_FATAL_FAILURE(Blur(s,xfa));const auto stable=s.Draw(320,300);
  ASSERT_NO_FATAL_FAILURE(ObserveRaster(stable));frames[backend].push_back(stable);
      std::vector<uint8_t> next;
      for(FPDF_DWORD flags:{FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}) {
        const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-sequence-"+std::to_string(chunk)+"-"+std::to_string(flags),flags);ASSERT_FALSE(saved.empty());
        if(flags==FPDF_NO_INCREMENTAL)next=saved;
        for(const auto* reader:Implementations()) {
          SCOPED_TRACE(reader->name());Session reload(reader->api(),saved,true,xfa);ASSERT_TRUE(reload.valid());if(xfa){ASSERT_TRUE(reload.xfa_loaded);}
          ASSERT_NO_FATAL_FAILURE(ProbeReload(reload,expected,xfa));const auto image=reload.Draw(320,300);ObserveRaster(image);
          Compare(stable,image,artifacts,writer->name()+"-"+reader->name()+"-chunk-"+std::to_string(chunk)+"-"+std::to_string(flags));
          EXPECT_TRUE(reload.host->callbacks_valid);EXPECT_FALSE(reload.host->callback_error);
        }
      }
      // Saving serializes content, not undo records. The next chunk starts a
      // new owning session and an independent empty history model.
      current=std::move(next);EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);
    }
    ++backend;
  }
  ASSERT_EQ(frames[0].size(),frames[1].size());
  for(size_t i=0;i<frames[0].size();++i)Compare(frames[0][i],frames[1][i],artifacts,"sequence-native-rust-chunk-"+std::to_string(i));
}
TEST(CoverageEditOperationSequences, AcroFormSeed00000001){Workflow(false,1);}
TEST(CoverageEditOperationSequences, AcroFormSeed13579bdf){Workflow(false,0x13579bdf);}
TEST(CoverageEditOperationSequences, AcroFormSeed2468ace0){Workflow(false,0x2468ace0);}
TEST(CoverageEditOperationSequences, AcroFormSeed5eed1234){Workflow(false,0x5eed1234);}
TEST(CoverageEditOperationSequences, AcroFormSeedcafebabe){Workflow(false,0xcafebabe);}
TEST(CoverageEditOperationSequences, XfaSeed00000001){Workflow(true,1);}
TEST(CoverageEditOperationSequences, XfaSeed13579bdf){Workflow(true,0x13579bdf);}
TEST(CoverageEditOperationSequences, XfaSeed2468ace0){Workflow(true,0x2468ace0);}
TEST(CoverageEditOperationSequences, XfaSeed5eed1234){Workflow(true,0x5eed1234);}
TEST(CoverageEditOperationSequences, XfaSeedcafebabe){Workflow(true,0xcafebabe);}
}  // namespace
}  // namespace pdfoxium::integration
