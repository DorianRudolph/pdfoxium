#include "coverage_editing_fixture.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using RGB = std::array<int, 3>;
constexpr int kWidth = 240, kHeight = 160;
constexpr int kCrisp = FPDF_RENDER_NO_SMOOTHIMAGE | FPDF_RENDER_NO_SMOOTHPATH;

// Widget dictionaries, MK icons and IF dictionaries have no public constructor.
// SDK page loading must generate the missing AP using these real form objects.
std::vector<uint8_t> WidgetPdf(const std::string& field, const std::string& mk,
                              const std::string& border = "/S /S /W 2",
                              const std::string& appearance = "/Helv 12 Tf 0 g",
                              int icon_width = 40, int icon_height = 20,
                              const std::string& icon_matrix = "1 0 0 1 0 0") {
  const std::string refs = "6 0 R";
  const std::string box = "0 0 " + std::to_string(icon_width) + " " + std::to_string(icon_height);
  auto icon = [&](const std::string& color) {
    return PdfStream("/Type /XObject /Subtype /Form /BBox [" + box + "] /Matrix [" + icon_matrix + "] /Resources << >>",
        color + " rg " + box + " re f");
  };
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 160] /Contents 7 0 R /Annots [" + refs + "] /Resources << >> >>",
      "<< /Fields [" + refs + "] /NeedAppearances true /DA (" + appearance + ") /DR << /Font << /Helv 5 0 R >> >> >>",
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
      "<< /Type /Annot /Subtype /Widget /P 3 0 R /T (subject) /Rect [20 40 220 140] /F 4 /DA (" + appearance + ") /BS << " + border + " >> /MK << /BG [1 1 1] /BC [0 0 0] " + mk + " >> " + field + " >>",
      PdfStream("", "0 0 1 rg 4 4 8 8 re f"), icon("1 0 0"), icon("0 1 0"), icon("0 0 1")});
}
RGB At(const Raster& image, int x, int pdf_y) {
  const size_t at = (static_cast<size_t>(kHeight - 1 - pdf_y) * kWidth + x) * 4;
  return {image.pixels.at(at + 2), image.pixels.at(at + 1), image.pixels.at(at)};
}
void Pixel(const Raster& image, int x, int y, RGB expected) {
  EXPECT_EQ(At(image, x, y), expected) << x << ',' << y;
}
size_t Count(const Raster& image, RGB wanted, int left = 23, int bottom = 43,
             int right = 217, int top = 137) {
  size_t count = 0;
  for (int y = bottom; y < top; ++y) for (int x = left; x < right; ++x)
    count += At(image, x, y) == wanted;
  return count;
}
size_t DarkInk(const Raster& image, int left = 23, int bottom = 43,
               int right = 217, int top = 137) {
  size_t count=0;
  for(int y=bottom;y<top;++y)for(int x=left;x<right;++x){
    const auto color=At(image,x,y);
    count+=color[0]<160&&color[1]<160&&color[2]<160;
  }
  return count;
}
void Guards(const Raster& image) {
  ASSERT_EQ(image.pixels.size(), kWidth * kHeight * 4u);
  Pixel(image, 8, 8, {0,0,255});
  for (auto xy : {std::array<int,2>{5,100}, {235,100}, {120,20}, {120,150}})
    Pixel(image, xy[0], xy[1], {255,255,255});
}
std::u16string AP(Session& session, int mode = 0) {
  Handle annot(session.api.FPDFPage_GetAnnot(session.page, 0), session.api.FPDFPage_CloseAnnot);
  if (!annot.get()) { ADD_FAILURE(); return {}; }
  const auto size = session.api.FPDFAnnot_GetAP(annot.get(), mode, nullptr, 0);
  EXPECT_EQ(size % 2, 0u);
  if (size < 2) return {};
  std::vector<unsigned short> result(size / 2 + 1);
  EXPECT_EQ(session.api.FPDFAnnot_GetAP(annot.get(), mode, result.data(), size), size);
  return {result.begin(), result.begin() + size / 2 - 1};
}
Raster Draw(Session& s) { return s.Draw(kWidth, kHeight, 0, kCrisp); }
void Compare(const Raster& a, const Raster& b, Artifacts& artifacts, const std::string& name, bool text) {
  if (text) coverage_editing::TextRegions(a, b, artifacts, name, {{22,22,196,96}});
  else CompareRaster(a, b, artifacts, name, 1);
}
using Inspector = std::function<void(Session&, const Raster&)>;
void Workflow(const std::vector<uint8_t>& fixture, const std::string& name,
              const Inspector& inspect, bool text = false, bool flatten = true) {
  Artifacts artifacts;
  std::array<Raster, 2> originals;
  size_t backend = 0;
  for (const auto* writer : Implementations()) {
    SCOPED_TRACE(name + " writer=" + writer->name());
    Session session(writer->api(), fixture);
    ASSERT_TRUE(session.valid());
    originals[backend] = Draw(session);
    ASSERT_NO_FATAL_FAILURE(Guards(originals[backend]));
    ASSERT_NO_FATAL_FAILURE(inspect(session, originals[backend]));
    session.api.FORM_ForceToKillFocus(session.form);
    session.api.FORM_OnMouseMove(session.form, session.page, 0, 235, 150);
    const auto stable = Draw(session);
    const auto saved = SaveDocument(session.api, session.document, artifacts, name + "-" + writer->name());
    ASSERT_FALSE(saved.empty());
    for (const auto* reader : Implementations()) {
      SCOPED_TRACE("reader=" + reader->name());
      Session reloaded(reader->api(), saved);
      ASSERT_TRUE(reloaded.valid());
      const auto image = Draw(reloaded);
      Guards(image);
      // Compare the normal saved state before the inspector drives transient
      // hover/down/focus state again on this reader's independent SDK objects.
      Compare(stable, image, artifacts, name + "-reload-" + reader->name(), text);
      ASSERT_NO_FATAL_FAILURE(inspect(reloaded, image));
      EXPECT_TRUE(reloaded.host->callbacks_valid);
      EXPECT_FALSE(reloaded.host->callback_error);
    }
    if (flatten) {
      const auto& api = writer->api();
      Handle document(api.FPDF_LoadMemDocument64(saved.data(), saved.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      Handle page(api.FPDF_LoadPage(document.get(),0), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr);
      ASSERT_EQ(api.FPDFPage_Flatten(page.get(), FLAT_NORMALDISPLAY), FLATTEN_SUCCESS);
      EXPECT_EQ(api.FPDFPage_GetAnnotCount(page.get()), 0);
      // Flatten updates the PDF Contents dictionary. The already parsed
      // page object list is not the flattened display list: reopen the saved
      // bytes in each engine before inspecting the baked page contents.
      const auto flattened_bytes=SaveDocument(api,document.get(),artifacts,name+"-flattened-"+writer->name());
      ASSERT_FALSE(flattened_bytes.empty());
      for(const auto* reader:Implementations()){
        const auto& read=reader->api();
        Handle baked(read.FPDF_LoadMemDocument64(flattened_bytes.data(),flattened_bytes.size(),nullptr),read.FPDF_CloseDocument);
        ASSERT_NE(baked.get(),nullptr);
        Handle baked_page(read.FPDF_LoadPage(baked.get(),0),read.FPDF_ClosePage);
        ASSERT_NE(baked_page.get(),nullptr);
        EXPECT_EQ(read.FPDFPage_GetAnnotCount(baked_page.get()),0);
        // FFLCommon deliberately honors LCD/gray/byte-order flags but
        // does not propagate NO_SMOOTHPATH/NO_SMOOTHIMAGE into SDK AP
        // rendering. Match its default smoothing for the baked AP too.
        const auto flattened=Render(read,baked_page.get(),kWidth,kHeight);
        Compare(stable,flattened,artifacts,name+"-flatten-"+reader->name(),text);
        Guards(flattened);
      }
    }
    ++backend;
  }
  Compare(originals[0], originals[1], artifacts, name + "-native-rust", text);
}

TEST(CoverageWidgetAppearanceVariants, PushButtonNormalRolloverDownIconsAndFallbacksDrawDistinctSdkStates) {
  for (bool distinct : {false,true}) for (const std::string style : {"S", "B", "I"}) {
    const std::string border = "/S /" + style + " /W 2";
    std::array<Raster,3> states;
    Artifacts state_artifacts;
    const auto fixture = WidgetPdf("/FT /Btn /Ff 65536 /H /P",
        "/TP 1 /I 8 0 R " + std::string(distinct ? "/RI 9 0 R /IX 10 0 R" : ""), border);
    Workflow(fixture,"push-icon-states-"+std::to_string(distinct)+style,[&](Session& s,const Raster& initial){
      auto state=[&](int index,const Raster& image){
        if(states[index].pixels.empty())states[index]=image;
        else CompareRaster(states[index],image,state_artifacts,"transient-"+std::to_string(index),1);
      };
      state(0,initial);
      Pixel(initial,120,90,{255,0,0});
      for (int mode : {0,1,2}) {
        const auto stream=AP(s,mode);EXPECT_FALSE(stream.empty());EXPECT_NE(stream.find(u" Do"),std::u16string::npos);
      }
      ASSERT_TRUE(s.api.FORM_OnMouseMove(s.form,s.page,0,120,90));
      const auto hover=Draw(s);
      Pixel(hover,120,90,distinct?RGB{0,255,0}:RGB{255,0,0});state(1,hover);
      ASSERT_TRUE(s.api.FORM_OnLButtonDown(s.form,s.page,0,120,90));
      const auto down=Draw(s);
      Pixel(down,120,90,distinct?RGB{0,0,255}:RGB{255,0,0});state(2,down);
      ASSERT_TRUE(s.api.FORM_OnLButtonUp(s.form,s.page,0,120,90));
      s.api.FORM_ForceToKillFocus(s.form);
      s.api.FORM_OnMouseMove(s.form,s.page,0,235,150);
      Pixel(Draw(s),120,90,{255,0,0});
    });
  }
}

TEST(CoverageWidgetAppearanceVariants, IconFitScalingAlignmentAspectAndBoundsPredictVisibleIconGeometry) {
  struct Case {const char* when; bool proportional, full; int width,height; double align;};
  const std::vector<Case> cases{
      {"N",true,false,40,20,0},{"N",true,false,40,20,.5},{"N",true,false,40,20,1},
      {"A",true,false,40,20,.5},{"A",false,false,40,20,.5},{"A",true,true,40,20,.5},
      {"B",true,false,400,200,.5},{"B",true,false,40,20,.5},
      {"S",true,false,400,200,.5},{"S",true,false,40,20,.5},
      {"A",false,true,400,200,1},{"N",false,false,400,200,0}};
  for (size_t index=0;index<cases.size();++index) {
    const auto c=cases[index]; const double left=c.full?20:22,bottom=c.full?40:42;
    const double pw=c.full?200:196,ph=c.full?100:96;
    double sx=1,sy=1;
    if(c.when[0]=='A'||(c.when[0]=='B'&&pw<c.width)||(c.when[0]=='S'&&pw>c.width))sx=pw/c.width;
    if(c.when[0]=='A'||(c.when[0]=='B'&&ph<c.height)||(c.when[0]=='S'&&ph>c.height))sy=ph/c.height;
    if(c.proportional)sx=sy=std::min(sx,sy);
    const double x0=left+(pw-c.width*sx)*c.align,y0=bottom+(ph-c.height*sy)*c.align;
    const double x1=x0+c.width*sx,y1=y0+c.height*sy;
    const std::string fit="/IF << /SW /"+std::string(c.when)+" /S /"+(c.proportional?"P":"A")+" /A ["+std::to_string(c.align)+" "+std::to_string(c.align)+"] /FB "+(c.full?"true":"false")+" >>";
    // An invertible nonidentity Form matrix must be cancelled by icon AP
    // placement; it must not change IF's intrinsic BBox dimensions.
    const auto fixture=WidgetPdf("/FT /Btn /Ff 65536 /H /P","/TP 1 /I 8 0 R "+fit,"/S /S /W 2","/Helv 12 Tf 0 g",c.width,c.height,
                                index%2?"0 1 -1 0 20 0":"1 0 0 1 0 0");
    Workflow(fixture,"icon-fit-"+std::to_string(index),[=](Session& s,const Raster& image){
      EXPECT_NE(AP(s).find(u" Do"),std::u16string::npos);
      EXPECT_GT(Count(image,{255,0,0}),100u);
      for(int y=46;y<135;y+=7)for(int x=26;x<215;x+=7){
        if(std::abs(x-x0)<2||std::abs(x-x1)<2||std::abs(y-y0)<2||std::abs(y-y1)<2)continue;
        Pixel(image,x,y,x>x0&&x<x1&&y>y0&&y<y1?RGB{255,0,0}:RGB{255,255,255});
      }
    });
  }
}

TEST(CoverageWidgetAppearanceVariants, EveryCaptionIconLayoutAndAutoFontModeGeneratesBothVisibleParts) {
  for(int layout=0;layout<=6;++layout)for(bool automatic:{false,true}) {
    const auto fixture=WidgetPdf("/FT /Btn /Ff 65536 /H /P", "/TP "+std::to_string(layout)+" /I 8 0 R /CA (Label) /RC (Hover) /AC (Down)",
                                 "/S /S /W 2",automatic?"/Helv 0 Tf 0 g":"/Helv 12 Tf 0 g");
    Workflow(fixture,"push-layout-"+std::to_string(layout)+"-"+std::to_string(automatic),[=](Session& s,const Raster& image){
      const auto stream=AP(s);
      EXPECT_EQ(stream.find(u" Do")!=std::u16string::npos,layout!=0);
      EXPECT_EQ(stream.find(u"Tj")!=std::u16string::npos,layout!=1);
      if(layout!=0) { EXPECT_GT(Count(image,{255,0,0}),50u); }
      if(layout!=1) { EXPECT_GT(DarkInk(image),5u); }
    },true);
  }
}

TEST(CoverageWidgetAppearanceVariants, CheckAndRadioMarkStylesGenerateGeometricOnOffAndPressedAppearances) {
  for(bool radio:{false,true})for(char mark:std::string("48Hlnu"))for(const std::string style:{"S","B","I"}) {
    Raster checked_reference; Artifacts checked_artifacts;
    const auto fixture=WidgetPdf("/FT /Btn /Ff "+std::to_string(radio?49152:0)+" /V /Off /AS /Off",
                                 "/CA ("+std::string(1,mark)+")","/S /"+style+" /W 2");
    Workflow(fixture,"mark-"+std::to_string(radio)+mark+style,[&](Session& s,const Raster& initial){
      const bool selected=s.Field(0)!=u"Off";
      if(selected)EXPECT_GT(DarkInk(initial,85,65,155,115),5u);
      else EXPECT_EQ(DarkInk(initial,85,65,155,115),0u);
      if(!selected)s.Click(120,90);
      s.api.FORM_ForceToKillFocus(s.form);
      const auto on=Draw(s);
      if(checked_reference.pixels.empty())checked_reference=on;
      else CompareRaster(checked_reference,on,checked_artifacts,"checked-mark",1);
      EXPECT_GT(DarkInk(on,85,65,155,115),5u);
      EXPECT_NE(s.Field(0),u"Off");
      EXPECT_FALSE(AP(s).empty()); EXPECT_FALSE(AP(s,2).empty());
      // Check boxes can turn off on another click. A radio's selected value
      // remains selected, which is its independent no-toggle-off control.
      if(!radio){s.Click(120,90);s.api.FORM_ForceToKillFocus(s.form);EXPECT_EQ(s.Field(0),u"Off");}
      else {s.Click(120,90);s.api.FORM_ForceToKillFocus(s.form);EXPECT_NE(s.Field(0),u"Off");}
    },false,false);
  }
}

TEST(CoverageWidgetAppearanceVariants, SdkCombAutoSizePasswordAndOverflowTextProduceClippedAppearanceContent) {
  struct Case {int flags,max,rotation; const char* value; const char* style; bool automatic;};
  const std::vector<Case> cases{
      {16777216,6,0,"12345","S",true},{16777216,6,0,"12345","D",true},
      {16777216,6,90,"12345","S",true},{16777216,6,270,"12345","D",false},
      {8192,12,0,"secret","S",true},{8192,12,180,"secret","I",false},
      {0,0,0,"A very long single line whose text extends beyond the right edge of this widget and must remain clipped","S",false},
      {4096,0,0,"First line\\rSecond line\\rThird line","D",false}};
  for(size_t i=0;i<cases.size();++i){const auto c=cases[i];
    const auto fixture=WidgetPdf("/FT /Tx /Ff "+std::to_string(c.flags)+" /MaxLen "+std::to_string(c.max)+" /V ("+c.value+")",
      "/R "+std::to_string(c.rotation),"/S /"+std::string(c.style)+" /W 2 /D [3 2 1]",c.automatic?"/Helv 0 Tf 0 g":"/Helv 12 Tf 0 g");
    Workflow(fixture,"sdk-text-"+std::to_string(i),[=](Session& s,const Raster& image){
      const auto stream=AP(s);EXPECT_NE(stream.find(u"Tj"),std::u16string::npos);EXPECT_GT(DarkInk(image),5u);
      EXPECT_FALSE(s.Field(0).empty());
      if(c.flags==8192){EXPECT_EQ(s.Field(0),u"secret");EXPECT_EQ(stream.find(u"secret"),std::u16string::npos);}
      if(c.flags==16777216){EXPECT_NE(stream.find(u"S\n"),std::u16string::npos);}
    },true);
  }
}

TEST(CoverageWidgetAppearanceVariants, ChoiceTopIndexMultipleSelectionAndEditableComboOverflowKeepSelectionPaintInsideWidget) {
  for(bool combo:{false,true})for(int top:{0,2,5}) {
    std::string options;for(int i=0;i<12;++i)options+="[(v"+std::to_string(i)+") (Option "+std::to_string(i)+
      (i==5?" is deliberately longer than the widget":"")+")] ";
    const std::string field="/FT /Ch /Ff "+std::to_string(combo?393216:2097152)+" /Opt ["+options+"] /TI "+std::to_string(top)+
      (combo?" /V (editable value which extends well past the text rectangle and drop button)":" /V [(v2) (v5)] /I [2 5]");
    const auto fixture=WidgetPdf(field,"","/S /S /W 2","/Helv 12 Tf 0 g");
    Workflow(fixture,"sdk-choice-"+std::to_string(combo)+"-"+std::to_string(top),[=](Session& s,const Raster& image){
      EXPECT_NE(AP(s).find(u"Tj"),std::u16string::npos);
      Handle annot(s.api.FPDFPage_GetAnnot(s.page,0),s.api.FPDFPage_CloseAnnot);
      EXPECT_EQ(s.api.FPDFAnnot_GetOptionCount(s.form,annot.get()),12);
      if(!combo){EXPECT_TRUE(s.api.FPDFAnnot_IsOptionSelected(s.form,annot.get(),2));EXPECT_TRUE(s.api.FPDFAnnot_IsOptionSelected(s.form,annot.get(),5));EXPECT_FALSE(s.api.FPDFAnnot_IsOptionSelected(s.form,annot.get(),0));EXPECT_GT(Count(image,{0,51,113}),100u);}
      else EXPECT_GT(DarkInk(image),10u);
    },true);
  }
}
}  // namespace
}  // namespace pdfoxium::integration
