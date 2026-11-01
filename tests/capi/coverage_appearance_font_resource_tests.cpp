// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "coverage_editing_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <cmath>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
struct Spec {
  std::string name,first="First",second="Second",ap;
  bool same_font=false,indirect=false;
};
constexpr int kWidth=240,kHeight=220,kScale=4;
std::vector<uint8_t> Fixture(const Spec& s) {
  const std::string entries="/"+s.first+" 5 0 R /"+s.second+(s.same_font?" 5 0 R":" 8 0 R");
  const auto font_resources=s.indirect?"15 0 R":"<< "+entries+" >>";
  const std::string first_ap=s.ap.empty()?"":"/AP << /N 10 0 R >> ";
  std::string resources;
  if(s.ap=="empty")resources="/Resources << >>";
  if(s.ap=="scalar")resources="/Resources << /Font 17 >>";
  if(s.ap=="unrelated")resources="/Resources << /Font << /Unrelated 8 0 R >> >>";
  if(s.ap=="alias")resources="/Resources << /Font << /Unrelated 5 0 R >> >>";
  if(s.ap=="indirect")resources="/Resources 14 0 R";
  if(s.ap=="bad_scalar")resources="/Resources << /Font << /"+s.first+" 5 0 R /Bad 17 >> >>";
  if(s.ap=="bad_type")resources="/Resources << /Font << /"+s.first+" 5 0 R /Bad 13 0 R >> >>";
  if(s.ap=="bad_reference")resources="/Resources << /Font << /"+s.first+" 5 0 R /Bad 999 0 R >> >>";
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
    "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 220] /Resources << >> /Annots [6 0 R 7 0 R] /Contents 9 0 R >>",
    "<< /Fields [6 0 R 7 0 R] /NeedAppearances true /DA (/"+s.first+" 18 Tf 0 g) /DR << /Font "+font_resources+" >> >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    "<< /Type /Annot /Subtype /Widget /P 3 0 R /FT /Tx /T (FirstField) /V (ABC) /DA (/"+s.first+" 18 Tf 0 g) /Rect [12 140 228 180] /F 4 /BS << /W 0 >> "+first_ap+">>",
    "<< /Type /Annot /Subtype /Widget /P 3 0 R /FT /Tx /T (SecondField) /V (XYZ) /DA (/"+s.second+" 18 Tf 0 g) /Rect [12 60 228 100] /F 4 /BS << /W 0 >> >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>",
    PdfStream("","0 0 1 rg 4 4 8 8 re f"),
    PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 216 40] "+resources,"BT /"+s.first+" 18 Tf 0 8 Td (Old) Tj ET"),
    "null","null","<< /Type /XObject /Subtype /Form >>",
    "<< /Font << /Unrelated 8 0 R >> >>","<< "+entries+" >>"});
}
std::string BaseName(const Api& api,FPDF_FONT font) {
  const auto n=api.FPDFFont_GetBaseFontName(font,nullptr,0);if(!n){ADD_FAILURE()<<"Missing base font name";return {};}
  std::vector<char> name(n);EXPECT_EQ(api.FPDFFont_GetBaseFontName(font,name.data(),n),n);return name.data();
}
std::u16string Value(const Api& api,FPDF_ANNOTATION annot) {
  const auto n=api.FPDFAnnot_GetStringValue(annot,"V",nullptr,0);if(n<2){ADD_FAILURE()<<"Missing field value";return {};}
  std::vector<FPDF_WCHAR> text(n/2);EXPECT_EQ(api.FPDFAnnot_GetStringValue(annot,"V",text.data(),n),n);return {text.begin(),text.end()-1};
}
void Inspect(const Api& api,FPDF_PAGE page,const Spec& spec,bool native_alias_fallback=false) {
  ASSERT_EQ(api.FPDFPage_GetAnnotCount(page),2);
  for(int field=0;field<2;++field) {
    Handle annot(api.FPDFPage_GetAnnot(page,field),api.FPDFPage_CloseAnnot);ASSERT_NE(annot.get(),nullptr);
    EXPECT_EQ(Value(api,annot.get()),field?u"XYZ":u"ABC");
    const bool courier=field&&!spec.same_font&&!native_alias_fallback;
    const auto expected=text_fixture::Read(TestRoot()/"tests/capi/resources/test_fonts"/(courier?"Cousine-Regular.ttf":"Arimo-Regular.ttf"));ASSERT_FALSE(expected.empty());
    const int count=api.FPDFAnnot_GetObjectCount(annot.get());ASSERT_GT(count,0);int texts=0;
    for(int i=0;i<count;++i) {
      const auto object=api.FPDFAnnot_GetObject(annot.get(),i);ASSERT_NE(object,nullptr);
      if(api.FPDFPageObj_GetType(object)!=FPDF_PAGEOBJ_TEXT)continue;
      ++texts;const auto font=api.FPDFTextObj_GetFont(object);ASSERT_NE(font,nullptr);
      EXPECT_EQ(BaseName(api,font),courier?"Courier":"Helvetica");EXPECT_EQ(api.FPDFFont_GetIsEmbedded(font),0);
      size_t n=0;ASSERT_TRUE(api.FPDFFont_GetFontData(font,nullptr,0,&n));ASSERT_EQ(n,expected.size());std::vector<uint8_t> bytes(n);
      ASSERT_TRUE(api.FPDFFont_GetFontData(font,bytes.data(),bytes.size(),&n));EXPECT_EQ(bytes,expected);
      float size=0,width=0;ASSERT_TRUE(api.FPDFTextObj_GetFontSize(object,&size));EXPECT_FLOAT_EQ(size,18);
      ASSERT_TRUE(api.FPDFFont_GetGlyphWidth(font,65,18,&width));EXPECT_NEAR(width,courier?10.8:12.006,.02);
      FS_MATRIX matrix{};ASSERT_TRUE(api.FPDFPageObj_GetMatrix(object,&matrix));EXPECT_TRUE(std::isfinite(matrix.e));EXPECT_TRUE(std::isfinite(matrix.f));
    }
    EXPECT_GT(texts,0);
  }
}
void Prepare(const Api& api,FPDF_DOCUMENT doc,const Spec& spec,bool native_alias_fallback=false) {
  Handle page(api.FPDF_LoadPage(doc,0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
  const auto image=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);ASSERT_EQ(image.width,kWidth);
  if(!spec.ap.empty()) {
    Handle annot(api.FPDFPage_GetAnnot(page.get(),0),api.FPDFPage_CloseAnnot);ASSERT_NE(annot.get(),nullptr);
    // NeedAppearances does not overwrite an authored AP. Its decoded body is
    // preserved even when its resources are malformed; public invalidation
    // below is what requests a fresh, correct appearance.
    EXPECT_NE(coverage_editing::Appearance(api,annot.get()).find(u"Old"),std::u16string::npos);
  }
  for(int i=0;i<2;++i) {
    Handle annot(api.FPDFPage_GetAnnot(page.get(),i),api.FPDFPage_CloseAnnot);ASSERT_NE(annot.get(),nullptr);
    ASSERT_TRUE(api.FPDFAnnot_SetBorder(annot.get(),0,0,0));
  }
  page.reset();page=Handle(api.FPDF_LoadPage(doc,0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
  const auto regenerated=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);ASSERT_EQ(regenerated.width,kWidth);
  ASSERT_NO_FATAL_FAILURE(Inspect(api,page.get(),spec,native_alias_fallback));
}
void VisibleSaved(const Api& api,const std::vector<uint8_t>& bytes,const Spec& spec,Raster* image,bool native_alias_fallback=false) {
  Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
  Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
  const auto static_page=Render(api,page.get(),kWidth,kHeight,FPDF_ANNOT);ASSERT_EQ(static_page.width,kWidth);ASSERT_NO_FATAL_FAILURE(Inspect(api,page.get(),spec,native_alias_fallback));
  ASSERT_EQ(api.FPDFPage_Flatten(page.get(),FLAT_NORMALDISPLAY),FLATTEN_SUCCESS);page.reset();page=Handle(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
  *image=Render(api,page.get(),kWidth*kScale,kHeight*kScale);
  coverage_editing::Ink(*image,12*kScale,40*kScale,216*kScale,40*kScale);
  coverage_editing::Ink(*image,12*kScale,120*kScale,216*kScale,40*kScale);
  EXPECT_EQ(coverage_editing::Pixel(*image,8*kScale,(kHeight-8)*kScale),(std::array<uint8_t,4>{255,0,0,255}));
  Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);std::u16string visible;
  for(int i=0;i<api.FPDFText_CountChars(text.get());++i){const auto c=api.FPDFText_GetUnicode(text.get(),i);if(c>32)visible.push_back(char16_t(c));}
  EXPECT_EQ(visible,u"ABCXYZ");
}
void Exercise(const Spec& spec) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);const auto input=Fixture(spec);Artifacts artifacts;std::array<Raster,2> original;
  size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(spec.name+" writer="+writer->name());const auto& api=writer->api();
    std::vector<uint8_t> control_input;  // Declared before its memory-backed document.
    Handle doc(api.FPDF_LoadMemDocument64(input.data(),input.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    const bool escaped_alias=spec.name=="EscapedSpacesInResourceNames"||spec.name=="EscapedHashInResourceNames";
    const bool native_fallback=escaped_alias&&writer->name()=="native";
    ASSERT_NO_FATAL_FAILURE(Prepare(api,doc.get(),spec,native_fallback));
    if(escaped_alias) {
      // Native writes the decoded alias directly into Tf, so spaces/hash bytes
      // become malformed PDF names and Courier falls back to Helvetica. Rust
      // must retain the independently asserted Courier identity and metrics.
      // Both readers must preserve each writer's original saved appearance.
      for(FPDF_DWORD flags:{FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}) {
        const auto original_saved=SaveDocument(api,doc.get(),artifacts,spec.name+"-original-"+writer->name()+"-"+std::to_string(flags),flags);
        ASSERT_FALSE(original_saved.empty());Raster original_scene;
        ASSERT_NO_FATAL_FAILURE(VisibleSaved(api,original_saved,spec,&original_scene,native_fallback));
        for(const auto* reader:Implementations()) {
          SCOPED_TRACE("original reader="+reader->name());Raster reread;
          ASSERT_NO_FATAL_FAILURE(VisibleSaved(reader->api(),original_saved,spec,&reread,native_fallback));
          if(writer==reader)CompareRaster(original_scene,reread,artifacts,"original-same-reader");
          else text_fixture::CompareTextRaster(original_scene,reread,artifacts,"original-cross-reader");
        }
      }
      // A separate, ordinary-alias control supplies the common scene without
      // changing the original PDF's font mappings or accepting the native bug.
      Spec common=spec;common.first="First";common.second="Second";
      control_input=Fixture(common);doc.reset();
      doc=Handle(api.FPDF_LoadMemDocument64(control_input.data(),control_input.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
      ASSERT_NO_FATAL_FAILURE(Prepare(api,doc.get(),common));
    }
    bool first=true;
    for(FPDF_DWORD flags:{FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}) {
      const auto saved=SaveDocument(api,doc.get(),artifacts,spec.name+"-"+writer->name()+"-"+std::to_string(flags),flags);ASSERT_FALSE(saved.empty());
      Raster before;ASSERT_NO_FATAL_FAILURE(VisibleSaved(api,saved,spec,&before));if(first){original[backend]=before;first=false;}else CompareRaster(original[backend],before,artifacts,"save-modes");
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());Raster after;ASSERT_NO_FATAL_FAILURE(VisibleSaved(reader->api(),saved,spec,&after));
        if(writer==reader)CompareRaster(before,after,artifacts,"same-reader");else text_fixture::CompareTextRaster(before,after,artifacts,writer->name()+"-"+reader->name());
      }
    }
    ++backend;
  }
  text_fixture::CompareTextRaster(original[0],original[1],artifacts,"native-rust-font-resources");
}
std::vector<Spec> Cases() {
  std::vector<Spec> cases;
  auto add=[&](const char* name,const char* first,const char* second,const char* ap="")->Spec&{Spec s;s.name=name;s.first=first;s.second=second;s.ap=ap;cases.push_back(s);return cases.back();};
  add("SharedIndirectFontUnderTwoAliases","First","Second").same_font=true;
  add("CommonPrefixNamesSelectDistinctFonts","FResOne","FResTwo");
  add("NumericSuffixNamesRemainIndependent","Font0","Font1");
  add("EscapedSpacesInResourceNames","Left#20Font","Right#20Font");
  add("EscapedHashInResourceNames","Font#23One","Font#23Two");
  add("HexEscapedInitialDigits","#31First","#32Second");
  add("AuthoredAppearanceWithoutResourcesCanBeRebuilt","First","Second","missing");
  add("EmptyAppearanceResourcesCanBeRebuilt","First","Second","empty");
  add("ScalarAppearanceFontMapCanBeRebuilt","First","Second","scalar");
  add("UnrelatedValidAppearanceFontDoesNotPoisonFormFonts","First","Second","unrelated");
  add("IndirectAppearanceResourcesCanBeRebuilt","First","Second","indirect");
  add("OtherAliasToTheSameFontSurvivesRebuilding","First","Second","alias");
  add("MalformedScalarSiblingIsIsolatedByPublicInvalidation","First","Second","bad_scalar");
  add("WrongTypeSiblingIsIsolatedByPublicInvalidation","First","Second","bad_type");
  add("UnresolvedSiblingIsIsolatedByPublicInvalidation","First","Second","bad_reference");
  add("IndirectAcroFormFontMapRetainsBothAliases","First","Second").indirect=true;
  return cases;
}
class CoverageAppearanceFontResources:public testing::TestWithParam<Spec>{};
TEST_P(CoverageAppearanceFontResources, RegenerationKeepsFontIdentityMetricsAndBothSavedReaders){Exercise(GetParam());}
INSTANTIATE_TEST_SUITE_P(FontResourceVariants,CoverageAppearanceFontResources,testing::ValuesIn(Cases()),[](const testing::TestParamInfo<Spec>& x){return x.param.name;});
}  // namespace
}  // namespace pdfoxium::integration
