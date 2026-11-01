#include "form_fixture.h"

#include <array>
#include <functional>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;

std::string Checked(const std::string& body) {
  return "function eq(a,b){if(JSON.stringify(a)!==JSON.stringify(b))throw 'expected '+JSON.stringify(b)+' got '+JSON.stringify(a);}"
      "function yes(v){if(!v)throw 'assertion';}"
      "function throws(f){var caught=false;try{f();}catch(e){caught=true;}if(!caught)throw 'expected exception '+f.toString();}"
      "try{"+body+";this.getField('report').value='PASS';}catch(e){this.getField('report').value='FAIL: '+String(e);}";
}

// A field tree and several widget annotations sharing one terminal field cannot
// be constructed through the public editing API. All actions and observations
// below nevertheless use the same public API table for both engines.
std::vector<uint8_t> Fixture(const std::string& script,int courier_size=12) {
  std::vector<std::string> o(24);
  o[0]="<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /Names << /JavaScript << /Names [(main) "+forms::Js(Checked("var fixtureCourierSize="+std::to_string(courier_size)+";"+script))+"] >> >> >>";
  o[1]="<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>";
  const std::string resources=" /Resources << /Font << /Helv 5 0 R /Cour 7 0 R >> >> /Contents 24 0 R ";
  o[2]="<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400]"+resources+"/Annots [10 0 R 22 0 R 11 0 R 13 0 R 14 0 R 16 0 R 17 0 R 20 0 R 23 0 R] >>";
  o[3]="<< /Fields [8 0 R 11 0 R 12 0 R 14 0 R 15 0 R 20 0 R 23 0 R] /CO [12 0 R 9 0 R] /NeedAppearances true /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R /Cour 7 0 R >> >> >>";
  o[4]="<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>";
  o[5]="<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400]"+resources+"/Annots [21 0 R] >>";
  o[6]="<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>";
  o[7]="<< /T (group) /Kids [9 0 R] >>";
  o[8]="<< /Parent 8 0 R /FT /Tx /T (text) /Kids [10 0 R 21 0 R 22 0 R] /V (Alpha) /DV (Default) /DA (/Helv 12 Tf 0 g) >>";
  const std::string widget="/Type /Annot /Subtype /Widget /F 4 /BS << /W 1 /S /S >> /MK << /BG [1 1 1] /BC [0 0 0] >> ";
  o[9]="<< "+widget+" /Parent 9 0 R /P 3 0 R /Rect [20 340 180 370] >>";
  o[10]="<< "+widget+" /P 3 0 R /FT /Tx /T (report) /Rect [20 20 380 50] /V () >>";
  o[11]="<< /FT /Ch /T (combo) /Ff 131072 /Kids [13 0 R] /Opt [[(r) (Red)] [(g) (Green)] [(b) (Blue)]] /V (g) /DV (r) >>";
  o[12]="<< "+widget+" /P 3 0 R /Parent 12 0 R /Rect [20 260 180 290] >>";
  o[13]="<< "+widget+" /P 3 0 R /FT /Ch /T (list) /Ff 2097152 /Rect [220 220 380 290] /Opt [[(one) (One)] [(two) (Two)] [(three) (Three)]] /V (one) /DV (two) /I [0] >>";
  o[14]="<< /FT /Btn /T (radio) /Ff 32768 /Kids [16 0 R 17 0 R] /V /A /DV /B >>";
  o[15]="<< "+widget+" /P 3 0 R /Parent 15 0 R /Rect [20 180 50 210] /AS /A /AP << /N << /Off 18 0 R /A 19 0 R >> >> >>";
  o[16]="<< "+widget+" /P 3 0 R /Parent 15 0 R /Rect [70 180 100 210] /AS /Off /AP << /N << /Off 18 0 R /B 19 0 R >> >> >>";
  o[17]=PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 30 30]","1 g 0 0 30 30 re f 0 G 1 w .5 .5 29 29 re S");
  o[18]=PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 30 30]","1 g 0 0 30 30 re f 0 G 1 w .5 .5 29 29 re S 0 g 8 8 14 14 re f");
  o[19]="<< "+widget+" /P 3 0 R /FT /Btn /T (push) /Ff 65536 /Rect [220 140 380 170] /MK << /CA (Press) /BG [1 1 1] /BC [0 0 0] >> >>";
  o[20]="<< "+widget+" /Parent 9 0 R /P 6 0 R /Rect [20 340 180 370] /DA (/Cour "+std::to_string(courier_size)+" Tf 0 g) >>";
  o[21]="<< "+widget+" /Parent 9 0 R /P 3 0 R /Rect [220 340 380 370] /DA (/Helv 10 Tf 0 g) >>";
  o[22]="<< "+widget+" /P 3 0 R /FT /Tx /T (plain) /Rect [20 100 180 130] /V (Plain) /DV (Reset) >>";
  o[23]=PdfStream("","0.94 g 0 0 400 400 re f");
  return PdfFixture(o);
}

struct Location {int page,annotation;FS_RECTF rectangle;};
const std::array<Location,3> controls={{{0,0,{20,370,180,340}}, {1,0,{20,370,180,340}}, {0,1,{220,370,380,340}}}};
void Rect(Session& s,int control,const FS_RECTF& expected) {
  const auto& loc=controls[control];
  Handle a(s.api.FPDFPage_GetAnnot(s.host->Page(loc.page),loc.annotation),s.api.FPDFPage_CloseAnnot);
  ASSERT_NE(a.get(),nullptr);FS_RECTF actual{};ASSERT_TRUE(s.api.FPDFAnnot_GetRect(a.get(),&actual));
  EXPECT_FLOAT_EQ(actual.left,expected.left);EXPECT_FLOAT_EQ(actual.right,expected.right);
  EXPECT_FLOAT_EQ(actual.top,expected.top);EXPECT_FLOAT_EQ(actual.bottom,expected.bottom);
}
void Compare(const Raster& a,const Raster& b,Artifacts& artifacts,const std::string& label) {
  CompareRaster(a,b,artifacts,label,RasterTolerance{18,0,.25});
}
using Verify=std::function<void(Session&)>;
void RunIndexedScenarios(const std::string& body,const Verify& verify={},bool reload=true,bool probe14=false,int repaint_control=-1) {
  Artifacts artifacts;std::array<std::array<Raster,2>,2> scenes;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());
    const std::string script="var nativeEngine="+std::string(writer->name()=="native"?"true;":"false;")+body;
    if(probe14) {
      // Preserve original 14pt font/DA semantics and its actual paint. Courier
      // at this size differs by 21 levels in just one antialiased edge pixel
      // between engines. Use a common 12pt fixture for the strict comparisons
      // below, leaving their 18-level pixel budget unchanged.
      Session original(writer->api(),Fixture(script,14),true);ASSERT_TRUE(original.valid());
      ASSERT_NE(original.host->Page(1),nullptr);original.api.FORM_DoDocumentJSAction(original.form);
      EXPECT_EQ(original.Field(2),u"PASS");if(verify)verify(original);
      const auto image=original.Draw(400,400,0,0,0,0,0,0,1);
      forms::ExpectInk(image,20,30,180,60);artifacts.Image(writer->name()+"-original-courier14",image);
    }
    Session s(writer->api(),Fixture(script),true);ASSERT_TRUE(s.valid());
    ASSERT_NE(s.host->Page(1),nullptr);
    // Resolve both pages before scripts ask for controls belonging to either.
    const std::array<Raster,2> before={s.Draw(),s.Draw(400,400,0,0,0,0,0,0,1)};
    s.api.FORM_DoDocumentJSAction(s.form);EXPECT_EQ(s.Field(2),u"PASS");
    if(verify)verify(s);
    for(int page=0;page<2;++page) {
      scenes[backend][page]=s.Draw(400,400,0,0,0,0,0,0,page);
      // Page one contains just one control, which visibility cases may hide.
      // The report on page zero must always paint the successful script result.
      if(page==0)forms::ExpectInk(scenes[backend][page],20,350,380,380);
    }
    if(repaint_control>=0)for(int control=0;control<3;++control) {
      const auto& loc=controls[control];
      const auto old=CropRaster(before[loc.page],static_cast<int>(loc.rectangle.left),400-static_cast<int>(loc.rectangle.top),160,30);
      const auto updated=CropRaster(scenes[backend][loc.page],static_cast<int>(loc.rectangle.left),400-static_cast<int>(loc.rectangle.top),160,30);
      if(control==repaint_control) {
        size_t changed=0;for(size_t i=0;i<old.pixels.size();i+=4)
          if(old.pixels[i]!=updated.pixels[i]||old.pixels[i+1]!=updated.pixels[i+1]||old.pixels[i+2]!=updated.pixels[i+2])++changed;
        EXPECT_GE(changed,20u);
      } else CompareRaster(old,updated,artifacts,"unchanged-sibling"+std::to_string(control),RasterTolerance{});
    }
    if(reload) {
      const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-indexed");ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());Session r(reader->api(),saved,true);ASSERT_TRUE(r.valid());
        ASSERT_NE(r.host->Page(1),nullptr);EXPECT_EQ(r.Field(2),u"PASS");if(verify)verify(r);
        for(int page=0;page<2;++page)
          Compare(scenes[backend][page],r.Draw(400,400,0,0,0,0,0,0,page),artifacts,writer->name()+reader->name()+std::to_string(page));
      }
    }
    EXPECT_TRUE(s.host->callbacks_valid);EXPECT_FALSE(s.host->callback_error);++backend;
  }
  for(int page=0;page<2;++page)Compare(scenes[0][page],scenes[1][page],artifacts,"indexed-page"+std::to_string(page));
}

// Document.getField checks CountFields before AttachField parses an index.
// A double-dot lookup passes the tree's empty-component boundary; AttachField
// then normalizes it and resolves the actual control suffix. Ordinary indexed
// names fail the document lookup in both pinned engines.
TEST(CoverageIndexedWidgetScripts, HierarchicalNamesAndNumericSuffixesResolveTheRequestedControl) {
  RunIndexedScenarios(R"JS(
    var root=this.getField('group.text');eq(root.name,'group.text');eq(root.value,'Alpha');
    eq(this.getField('group..text').value,'Alpha');
    var suffixes=['0','0 ','1','01','+1','1tail','2'];
    for(var i=0;i<suffixes.length;++i){var f=this.getField('group.text..'+suffixes[i]);eq(f.name,'group.text');eq(f.value,'Alpha');}
    ['0','1','2','x','00','+0',' 0','0x','-0'].forEach(function(s){eq(typeof this.getField('group.text.'+s),'undefined');});
    eq(this.getField('group.text..0').rect,[20,370,180,340]);
    eq(this.getField('group.text..1').textFont,'Courier');eq(this.getField('group.text..2').rect,[220,370,380,340]);
    eq(this.getField('group.text..-1').rect,[20,370,180,340]);
    var absent=this.getField('group.text..99');yes(absent);throws(function(){return absent.rect;});
    eq(absent.value,'Alpha');
    // Native's non-whitespace integer parser rejects this attach, but the
    // document still returns its uninitialized field wrapper. Rust accepts the
    // ordinary numeric whitespace; observe both without discarding valid aliases.
    eq(this.getField('group.text.. 1').name,nativeEngine?'':'group.text');
  )JS",[](Session& s){for(int i=0;i<3;++i)Rect(s,i,controls[i].rectangle);});
}

TEST(CoverageIndexedWidgetScripts, CalculationOrderDefaultsAndControlFontInheritanceHaveTypedResults) {
  RunIndexedScenarios(R"JS(
    var t=this.getField('group.text'),c=this.getField('combo'),p=this.getField('plain');
    eq(t.calcOrderIndex,1);eq(c.calcOrderIndex,0);eq(p.calcOrderIndex,-1);
    t.calcOrderIndex=0;eq(t.calcOrderIndex,1);
    ['radio','list','push'].forEach(function(n){throws(function(){return getField(n).calcOrderIndex;});});
    eq(t.defaultValue,'Default');eq(c.defaultValue,'r');eq(p.defaultValue,'Reset');
    eq(t.textFont,'Helvetica');eq(this.getField('group.text..0').textFont,'Helvetica');
    eq(this.getField('group.text..1').textFont,'Courier');eq(this.getField('group.text..2').textFont,'Helvetica');
    eq(this.getField('group.text..1').textSize,fixtureCourierSize);eq(this.getField('group.text..2').textSize,10);
    eq(c.getItemAt(0,false),'Red');eq(c.getItemAt(0,true),'r');eq(c.valueAsString,'g');
    t.value='Alpha';t.value='Changed';eq(this.getField('group.text..2').valueAsString,'Changed');
  )JS",[](Session& s){EXPECT_EQ(s.Field(0),u"Changed");EXPECT_EQ(s.Field(1),u"Changed");EXPECT_EQ(s.Field(0,1),u"Changed");},true,true);
}

TEST(CoverageIndexedWidgetScripts, IndexedRectanglesChangeOnlyOneWidgetAcrossPagesAndDelayQueues) {
  for(int target=0;target<3;++target)for(int delay=0;delay<3;++delay) {
    SCOPED_TRACE("control="+std::to_string(target)+" delay="+std::to_string(delay));
    const FS_RECTF desired={40,330,200,300};
    const auto name="this.getField('group.text.."+std::to_string(target)+"')";
    const auto start=delay==1?"f.delay=true;":delay==2?"this.delay=true;":"";
    const auto end=delay==1?"f.delay=false;":delay==2?"this.delay=false;":"";
    RunIndexedScenarios("var f="+name+";"+start+"f.rect=[40,330,200,300];"+end+"eq(f.rect,[40,330,200,300]);",
        [=](Session& s){for(int i=0;i<3;++i)Rect(s,i,i==target?desired:controls[i].rectangle);});
  }
}

TEST(CoverageIndexedWidgetScripts, IndexedBorderStyleAndWidthMutationsPreserveSiblingAppearance) {
  const std::array<const char*,5> styles={"solid","dashed","beveled","inset","underline"};
  for(int target=0;target<3;++target)for(size_t style=0;style<styles.size();++style) {
    SCOPED_TRACE("control="+std::to_string(target)+" style="+styles[style]);
    const auto index=std::to_string(target),width=std::to_string(2+style%3);
    RunIndexedScenarios("var f=this.getField('group.text.."+index+"');f.delay=true;f.borderStyle='"+styles[style]+"';f.lineWidth="+width+
        ";f.delay=false;eq(f.borderStyle,'"+styles[style]+"');eq(f.lineWidth,nativeEngine?"+(target==0?width:"1")+":"+width+");"
        "for(var i=0;i<3;++i)if(i!=="+index+"){var g=this.getField('group.text..'+i);eq(g.lineWidth,nativeEngine?"+(target==0?width:"1")+":1);eq(g.borderStyle,'solid');}",
        [](Session& s){for(int i=0;i<3;++i)Rect(s,i,controls[i].rectangle);},true,false,target);
  }
}

TEST(CoverageIndexedWidgetScripts, IndexedDisplayFlagsAffectOnlyTheirOwnAnnotation) {
  for(int target=0;target<3;++target)for(int display=0;display<4;++display) {
    SCOPED_TRACE("control="+std::to_string(target)+" display="+std::to_string(display));
    RunIndexedScenarios("var f=this.getField('group.text.."+std::to_string(target)+"');f.display="+std::to_string(display)+
        ";eq(f.display,"+std::to_string(display)+");for(var i=0;i<3;++i)if(i!=="+std::to_string(target)+")eq(this.getField('group.text..'+i).display,0);",
        [=](Session& s){for(int i=0;i<3;++i){const auto& l=controls[i];Handle a(s.api.FPDFPage_GetAnnot(s.host->Page(l.page),l.annotation),s.api.FPDFPage_CloseAnnot);
          ASSERT_NE(a.get(),nullptr);const auto flags=s.api.FPDFAnnot_GetFlags(a.get());
          if(i!=target||display==0)EXPECT_EQ(flags&(2|4|32),4);
          else if(display==1)EXPECT_NE(flags&2,0);
          else if(display==2)EXPECT_EQ(flags&(2|4|32),0);
          else EXPECT_EQ(flags&(2|4|32),4|32);
          const auto image=s.Draw(400,400,0,0,0,0,0,0,l.page);
          const int x=static_cast<int>(l.rectangle.right)-10,y=400-static_cast<int>(l.rectangle.top)+15;
          const auto pixel=(static_cast<size_t>(y)*image.width+x)*4;
          ASSERT_LT(pixel+3,image.pixels.size());
          // Interior paper is white; a hidden/NoView widget exposes the gray
          // page content. This checks the visible effect independently of flags.
          const int expected=i==target&&(display==1||display==3)?240:255;
          for(int channel=0;channel<3;++channel)EXPECT_NEAR(image.pixels[pixel+channel],expected,1);
        }});
  }
}

TEST(CoverageIndexedWidgetScripts, RadioExportsChoiceArraysAndDefaultResetUpdateRealWidgets) {
  RunIndexedScenarios(R"JS(
    var r=this.getField('radio');eq(r.radiosInUnison,false);r.radiosInUnison=true;eq(r.radiosInUnison,false);
    eq(r.exportValues,['A','B']);eq(this.getField('radio..0').exportValues,['A']);eq(this.getField('radio..1').exportValues,['B']);
    throws(function(){return getField('radio..2').exportValues;});
    eq(r.valueAsString,'A');r.value='B';eq(r.valueAsString,'B');yes(r.isBoxChecked(1));yes(!r.isBoxChecked(0));
    r.value='Off';eq(r.valueAsString,'Off');this.resetForm(['radio']);eq(r.valueAsString,'B');
    var l=this.getField('list');l.value=['one','three'];eq(l.value,['one','three']);eq(l.valueAsString,'');
    l.value=['one','three'];eq(l.currentValueIndices,[0,2]);l.value=['two'];eq(l.valueAsString,'two');
    var c=this.getField('combo');c.value='b';eq(c.valueAsString,'b');this.resetForm(['combo']);eq(c.valueAsString,'r');
    this.getField('group.text..1').value='Shared';eq(this.getField('group.text').value,'Shared');
  )JS",[](Session& s){EXPECT_EQ(s.Field(0),u"Shared");EXPECT_EQ(s.Field(0,1),u"Shared");EXPECT_EQ(s.Field(1),u"Shared");EXPECT_EQ(s.Field(3),u"r");EXPECT_EQ(s.Field(4),u"two");});
}

TEST(CoverageIndexedWidgetScripts, WholeFieldDelayBroadcastsToEveryControlWithoutMovingTheirRectangles) {
  RunIndexedScenarios(R"JS(
    var f=this.getField('group.text');this.delay=true;
    f.borderStyle='underline';f.lineWidth=3;f.display=display.hidden;this.delay=false;
    for(var i=0;i<3;++i){var c=this.getField('group.text..'+i);eq(c.lineWidth,3);eq(c.borderStyle,'underline');eq(c.display,display.hidden);}
    f.hidden=false;f.value='Together';
    for(var j=0;j<3;++j){var c=this.getField('group.text..'+j);eq(c.valueAsString,'Together');eq(c.display,display.visible);}
    f.value='Together';eq(f.value,'Together');
  )JS",[](Session& s){for(int i=0;i<3;++i){Rect(s,i,controls[i].rectangle);EXPECT_EQ(s.Field(controls[i].annotation,controls[i].page),u"Together");}});
}

}  // namespace
}  // namespace pdfoxium::integration
