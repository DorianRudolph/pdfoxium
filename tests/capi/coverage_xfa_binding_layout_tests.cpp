#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <functional>
#include <fstream>
#include <iterator>
#include <string>
#include <utility>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using RGB = std::array<int, 3>;
using SceneComparison=std::function<void(const std::vector<Raster>&,const std::vector<Raster>&,Artifacts&,const std::string&)>;

std::string Area(const std::string& name, const std::string& content,
                 const std::string& attributes = "") {
  return "<pageArea name=\"" + name + "\" id=\"" + name + "\" " + attributes +
      "><occur min=\"1\" max=\"-1\"/><medium short=\"240pt\" long=\"240pt\"/>" +
      content + "</pageArea>";
}
std::string DefaultPages(const std::string& content = "<contentArea name=\"body\" w=\"240pt\" h=\"240pt\"/>") {
  return "<pageSet>" + Area("page", content) + "</pageSet>";
}
std::vector<uint8_t> Fixture(const std::string& body, const std::string& data = "<root/>",
                             const std::string& page_set = DefaultPages(), const std::string& description = "") {
  // Public C API construction does not expose XFA template/dataset nodes.
  // The engines themselves merge, lay out, execute, paint and serialize them.
  const std::string xml = "<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">"
      "<subform name=\"root\" layout=\"tb\" locale=\"en_US\" restoreState=\"auto\">" +
      page_set + body + "</subform></template>";
  return PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering true >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 240] /Resources << /Font << /Helv 10 0 R >> >> >>",
      "<< /Fields [] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 10 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("", "<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("", "<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("", xml),
      PdfStream("", "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data>" + data + "</xfa:data>" + description + "</xfa:datasets>"),
      PdfStream("", "</xdp:xdp>"),
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"});
}
std::string Block(const std::string& name, RGB color, int width, int height,
                  const std::string& attributes = "") {
  return "<draw name=\"" + name + "\" w=\"" + std::to_string(width) + "pt\" h=\"" +
      std::to_string(height) + "pt\" " + attributes + "><value><rectangle><fill><color value=\"" +
      std::to_string(color[0]) + "," + std::to_string(color[1]) + "," + std::to_string(color[2]) +
      "\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
}
std::string Field(const std::string& name, const std::string& value = "Ready",
                  const std::string& attributes = "", const std::string& binding = "<bind match=\"none\"/>",
                  int width = 200) {
  return "<field name=\"" + name + "\" w=\"" + std::to_string(width) + "pt\" h=\"20pt\" " + attributes +
      "><ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"12pt\"/>" + binding +
      "<value><text>" + value + "</text></value></field>";
}
std::string Checked(const std::string& body, const std::string& output = "xfa.form.root.report") {
  return "function eq(a,b){if(a===b)return;if((a!==null&&typeof a==='object'&&!Array.isArray(a))||(b!==null&&typeof b==='object'&&!Array.isArray(b)))throw 'identity';if(JSON.stringify(a)!==JSON.stringify(b))throw 'expected '+JSON.stringify(b)+' got '+JSON.stringify(a);}"
      "function near(a,b){if(Math.abs(a-b)>.002)throw 'expected '+b+' got '+a;}"
      "function yes(a,label){if(!a)throw 'assertion '+(label||'');}"
      "function throws(f){var caught=false;try{f();}catch(e){caught=true;}if(!caught)throw 'expected exception';}"
      "try{" + body + ";" + output + ".rawValue='PASS';}catch(e){" + output + ".rawValue='FAIL: '+String(e);}";
}
std::string Probe(const std::string& body, bool master = false,
                  const std::string& output = "") {
  const std::string target = !output.empty() ? output : master ? "this.parent.report" : "xfa.form.root.report";
  const std::string attributes = master ? "x=\"180pt\" y=\"0pt\" w=\"50pt\"" : "w=\"200pt\"";
  const std::string button = "<field name=\"probe\" h=\"20pt\" " + attributes +
      "><bind match=\"none\"/><ui><button/></ui><font typeface=\"Helvetica\" size=\"8pt\"/>"
      "<caption><value><text>Probe</text></value></caption>" +
      forms::Event(Checked(body, target), "click") + "</field>";
  if (!master) return button + Field("report");
  return button + "<field name=\"report\" x=\"180pt\" y=\"30pt\" w=\"50pt\" h=\"20pt\">"
      "<bind match=\"none\"/><ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"8pt\"/>"
      "<value><text>Ready</text></value></field>";
}
struct FlowSession {
  forms::XfaCallbacks callbacks;
  Session session;
  FlowSession(const Api& api, std::vector<uint8_t> bytes)
      : session(api, std::move(bytes), true, true, 2,
                [this](forms::FormHost& host) { callbacks.Configure(host); }) {}
};
void ReopenPages(Session& s) {
  for (size_t i = 0; i < s.host->pages.size(); ++i) s.ClosePage(static_cast<int>(i));
  s.page = s.host->Page(0);
  ASSERT_NE(s.page, nullptr);
}
void RunProbe(Session& s, bool master = false) {
  s.api.FORM_ForceToKillFocus(s.form);
  s.Click(master ? 205 : 8, 10);
  ReopenPages(s);
  ASSERT_TRUE(s.Focus(master ? 185 : 8, master ? 35 : 30));
  ASSERT_EQ(s.Text(), u"PASS");
  s.api.FORM_ForceToKillFocus(s.form);
  EXPECT_TRUE(s.host->callbacks_valid);
  EXPECT_FALSE(s.host->callback_error);
}
std::vector<Raster> Pages(Session& s) {
  std::vector<Raster> result;
  const int count = s.api.FPDF_GetPageCount(s.document);
  for (int page = 0; page < count; ++page)
    result.push_back(s.Draw(240, 240, 0, 0, 0, 0, 0, 0, page));
  return result;
}
void Pixel(const Raster& raster, int x, int y, RGB color) {
  ASSERT_GE(x, 0); ASSERT_GE(y, 0); ASSERT_LT(x, raster.width); ASSERT_LT(y, raster.height);
  const auto at = (static_cast<size_t>(y) * raster.width + x) * 4;
  EXPECT_EQ(raster.pixels[at], color[2]) << x << "," << y;
  EXPECT_EQ(raster.pixels[at + 1], color[1]) << x << "," << y;
  EXPECT_EQ(raster.pixels[at + 2], color[0]) << x << "," << y;
}
void ComparePages(const std::vector<Raster>& a, const std::vector<Raster>& b,
                  Artifacts& artifacts, const std::string& label, bool text = true) {
  ASSERT_EQ(a.size(), b.size());
  for (size_t i = 0; i < a.size(); ++i) {
    CompareRaster(a[i], b[i], artifacts, label + std::to_string(i),
                  text ? RasterTolerance{18, 0, 0.15} : RasterTolerance{});
    if (text) for (int y = 0; y < 240; y += 20)
      CompareRaster(CropRaster(a[i], 0, y, 240, 20), CropRaster(b[i], 0, y, 240, 20), artifacts,
                    label + std::to_string(i) + "-row" + std::to_string(y), RasterTolerance{18, 0, 0.5});
  }
}
void FourWay(const Library& writer, Session& s, Artifacts& artifacts,
             const std::string& label, const std::function<void(Session&)>& inspect,
             bool text = true,const SceneComparison& comparison={}) {
  const auto original = Pages(s);
  const auto saved = SaveDocument(s.api, s.document, artifacts, writer.name() + "-" + label);
  ASSERT_FALSE(saved.empty());
  std::array<std::vector<Raster>, 2> images;
  size_t backend = 0;
  for (const auto* reader : Implementations()) {
    SCOPED_TRACE(writer.name() + " -> " + reader->name());
    FlowSession loaded(reader->api(), saved); auto& reopened = loaded.session;
    ASSERT_TRUE(reopened.valid()); ASSERT_TRUE(reopened.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(inspect(reopened));
    images[backend++] = Pages(reopened);
    // Native mixed/text dataset serialization can insert a leading newline.
    // Both readers still inspect identical bytes; Rust output must also retain
    // the original appearance rather than learning that serialization defect.
    if(writer.name()=="rust") {
      if(comparison&&reader->name()!="rust")comparison(original,images[backend-1],artifacts,label+"-original");
      else ComparePages(original,images[backend-1],artifacts,label+"-original",text);
    }
  }
  if(comparison)comparison(images[0],images[1],artifacts,label+"-readers");
  else ComparePages(images[0],images[1],artifacts,label+"-readers",text);
}

// These batches exercise the model through real click callbacks and inspect
// actual layout fragments/paint. No test calls a private formatter or layout API.
void Scenario(const std::string& name,const std::string& body,const std::string& program,
              const std::string& data,const std::function<void(Session&)>& inspect,
              const std::string& pages=DefaultPages(),const std::string& description="",const SceneComparison& comparison={},bool mark_reader=false) {
  Artifacts artifacts;std::array<std::vector<Raster>,2> images;size_t backend=0;
  auto controls=Probe("function clean(v){v=String(v);return v.charCodeAt(0)===10?v.slice(1):v;}function text(n){return clean(n.rawValue);}"+program);
  // A probe must fit the100pt content areas used by the explicit-break table.
  for(size_t at=0;(at=controls.find("w=\"200pt\"",at))!=std::string::npos;)controls.replace(at,9,"w=\"90pt\"");
  const auto bytes=Fixture(controls+body,data,pages,description);
  artifacts.Pdf(name+"-input",bytes);
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());FlowSession loaded(library->api(),bytes);auto& s=loaded.session;
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    if(mark_reader){ASSERT_TRUE(s.Focus(8,30));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(library->name()=="native"?u"N":u"R");s.api.FORM_ForceToKillFocus(s.form);}
    ASSERT_NO_FATAL_FAILURE(RunProbe(s));
    ASSERT_NO_FATAL_FAILURE(inspect(s));
    images[backend++]=Pages(s);
    for(size_t page=0;page<images[backend-1].size();++page)artifacts.Image(library->name()+"-"+name+"-original-"+std::to_string(page),images[backend-1][page]);
    ASSERT_NO_FATAL_FAILURE(FourWay(*library,s,artifacts,name,[&](Session& r){
      if(mark_reader){ASSERT_TRUE(r.Focus(8,30));ASSERT_TRUE(r.api.FORM_SelectAllText(r.form,r.page));r.Replace(r.api.FPDF_GetPageCount==Implementations()[0]->api().FPDF_GetPageCount?u"N":u"R");r.api.FORM_ForceToKillFocus(r.form);}
      ASSERT_NO_FATAL_FAILURE(RunProbe(r));
      ASSERT_NO_FATAL_FAILURE(inspect(r));
    },true,comparison));
  }
  if(comparison)comparison(images[0],images[1],artifacts,name+"-backends");
  else ComparePages(images[0],images[1],artifacts,name+"-backends");
}
std::string Edit(const std::string& name,const std::string& ui,const std::string& value,
                 const std::string& extra="",const std::string& dimensions="w=\"180pt\" h=\"24pt\"") {
  return "<field name=\""+name+"\" "+dimensions+"><ui>"+ui+"</ui><font typeface=\"Helvetica\" size=\"10pt\"/>"+value+extra+"</field>";
}
void OnePageInk(Session& s,int top=40,int bottom=200) {
  ASSERT_EQ(s.api.FPDF_GetPageCount(s.document),1);
  const auto image=Pages(s);ASSERT_EQ(image.size(),1u);forms::ExpectInk(image[0],0,top,180,bottom);
  Pixel(image[0],230,220,{255,255,255});
}

TEST(CoverageXfaBindingLayout, UnorderedAndChoiceSetsFollowDatasetOrderAndPreserveSelectedNodeIdentity) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // DataMerge_CopySubformSet sorts matching children by dataset order. A
  // choice takes the first match; unordered appends unmatched template nodes.
  for(const std::string relation:{"choice","unordered"})for(bool reverse:{false,true}) {
    SCOPED_TRACE(relation);
    SCOPED_TRACE(reverse);
    const auto children=Field("a","fallbackA","","<bind match=\"once\"/>")+
        Field("b","fallbackB","","<bind match=\"once\"/>")+
        Field("c","fallbackC","","<bind match=\"once\"/>");
    const std::string first=reverse?"a":"b",second=reverse?"b":"a";
    const auto body="<subformSet name=\"set\" relation=\""+relation+"\">"+children+"</subformSet>"+
        Block("after",{0,128,0},60,10);
    const auto script="var r=xfa.form.root,s=r.set;var fields=[];for(var i=0;i<s.nodes.length;i++){var n=s.nodes.item(i);if(n.className=='field')fields.push(n.name);}eq(fields,"+
        (relation=="choice"?"['"+first+"']":"['"+first+"','"+second+"','c']")+
        ");eq(text(s.resolveNode('"+first+"')),'"+(reverse?"A":"B")+"');eq(s.resolveNode('"+first+"').dataNode,xfa.datasets.data.root.resolveNode('"+first+"'));"+
        (relation=="unordered"?"eq(text(s.c),'fallbackC');eq(text(s.resolveNode('"+second+"')),'"+(reverse?"B":"A")+"');":"eq(s.resolveNode('"+second+"'),null);");
    Scenario(relation+std::to_string(reverse),body,script,
        reverse?"<root><a>A</a><b>B</b></root>":"<root><b>B</b><a>A</a></root>",
        [=](Session& s){OnePageInk(s,40,110);const auto image=Pages(s);Pixel(image[0],5,relation=="choice"?65:105,{0,128,0});});
  }
}

void CompareReadyCombo(const std::vector<Raster>& a,const std::vector<Raster>& b,Artifacts& artifacts,const std::string& label) {
  ASSERT_EQ(a.size(),1u);ASSERT_EQ(b.size(),1u);
  // Native ComboBox draws its edit at cfwl_combobox.cpp128, then WidgetMgr
  // draws that child again. Rust's draws_child avoids duplicate compositing.
  // The exact Ready value is asserted in script; bound its visible glyphs and
  // compare all list rows, selections, combo chrome and other pixels normally.
  const auto bounds=[](const Raster& image){std::array<int,5> out{40,186,-1,-1,0};
    for(int y=172;y<186;++y)for(int x=0;x<40;++x){const auto at=(static_cast<size_t>(y)*240+x)*4;
      if(image.pixels[at]<180&&image.pixels[at+1]<180&&image.pixels[at+2]<180){out[0]=std::min(out[0],x);out[1]=std::min(out[1],y);out[2]=std::max(out[2],x);out[3]=std::max(out[3],y);++out[4];}}
    return out;};
  const auto first=bounds(a[0]),second=bounds(b[0]);EXPECT_GT(first[4],5);EXPECT_GT(second[4],5);
  for(size_t coordinate=0;coordinate<4;++coordinate)EXPECT_NEAR(first[coordinate],second[coordinate],1);
  for(const auto& r:std::array<std::array<int,4>,3>{{{0,0,240,172},{0,186,240,54},{40,172,200,14}}})
    CompareRaster(CropRaster(a[0],r[0],r[1],r[2],r[3]),CropRaster(b[0],r[0],r[1],r[2],r[3]),artifacts,
                  label+"-outside-"+std::to_string(r[0])+"-"+std::to_string(r[1]),RasterTolerance{18,0,.15});
}

TEST(CoverageXfaBindingLayout, StructuredMultiselectGroupsAndItemMappingMutationsStayBoundAndVisible) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::string items="<items><text>Red</text><text>Green</text><text>Blue</text></items><items save=\"1\"><text>r</text><text>g</text><text>b</text></items>";
  const auto body=Field("phase","before","","<bind match=\"once\"/>")+
      // Keep the imported calculation field's original item definitions. Its
      // data binding remains active while the visible preview is independent.
      Edit("bound","<choiceList open=\"multiSelect\"/>","<value><exData contentType=\"text/xml\"/></value>",items+"<bind match=\"once\"/>","presence=\"invisible\" w=\"160pt\" h=\"48pt\"")+
      Edit("selection","<choiceList open=\"multiSelect\"/>","<value><text>r&#10;b</text></value>",items+"<bind match=\"none\"/>","w=\"160pt\" h=\"64pt\"")+
      Edit("empty","<choiceList open=\"userControl\" textEntry=\"1\"/>","<value><text/></value>","<bind match=\"none\"/>","w=\"160pt\" h=\"24pt\"");
  const auto script=R"JS(var r=xfa.form.root,b=r.bound,s=r.selection,e=r.empty,initial=text(r.phase)!='done',native=text(r.report)=='N';
    function clean(v){v=String(v);return v.charCodeAt(0)===10?v.slice(1):v;}
    function cleanItems(f){var labels=[],saved=[];for(var i=0;i<f.length;i++){labels.push(clean(f.getDisplayItem(i)));saved.push(clean(f.getSaveItem(i)));}f.deleteItem(-1);for(var i=0;i<labels.length;i++)f.addItem(labels[i],saved[i]);}
    if(initial){
      xfa.host.messageBox('initial:'+String(b.rawValue)+':'+Number(b.getItemState(0))+Number(b.getItemState(1))+Number(b.getItemState(2)));
      eq(b.dataNode,xfa.datasets.data.root.bound);eq(b.dataNode.nodes.length,2);
    }
    // Native detached the imported group's XML children. Reducing its bound
    // selection CHECKs in RemoveChildAndNotify. Retain its two values; only
    // Rust exercises that source-proven broken native mutation path below.
    // Keep each item node and mapping in place. Native-writer reload adds
    // one LF to its text payload (including saved values), making 'r' fail
    // to match '\nr'. Remove exactly that serialization LF via node.value;
    // never clear a bound selection or remove its mapped dataset children.
    for(var groupIndex=0;groupIndex<b.nodes.length;groupIndex++){
      var itemGroup=b.nodes.item(groupIndex);if(itemGroup.className!='items')continue;
      for(var itemIndex=0;itemIndex<itemGroup.nodes.length;itemIndex++){
        var item=itemGroup.nodes.item(itemIndex),value=String(item.value);
        if(value.charCodeAt(0)===10)item.value=value.slice(1);
      }
    }
    eq(b.getSaveItem(0),'r');eq(b.getSaveItem(1),'g');eq(b.getSaveItem(2),'b');
    b.rawValue=['r','b'].join(String.fromCharCode(10));
    if(!native){
      b.addItem('Amber','a');b.setItemState(0,0);b.setItemState(1,1);b.setItemState(3,1);
      eq(b.dataNode.nodes.length,3);eq(clean(b.dataNode.nodes.item(0).value),'b');
      eq(clean(b.dataNode.nodes.item(1).value),'g');eq(clean(b.dataNode.nodes.item(2).value),'a');
      b.rawValue=['r','b'].join(String.fromCharCode(10));b.deleteItem(3);
    }
    eq(b.length,3);eq(b.dataNode.nodes.length,2);eq(clean(b.dataNode.nodes.item(0).value),'r');eq(clean(b.dataNode.nodes.item(1).value),'b');
    yes(b.getItemState(0),'bound-r');yes(!b.getItemState(1),'bound-notg');yes(b.getItemState(2),'bound-b');
    // Native serialized labels gain a leading LF and can trap its text
    // renderer. Rebuild equivalent UNBOUND items before any assertions/draw.
    cleanItems(s);cleanItems(e);
    if(initial){
      s.rawValue=['r','b'].join(String.fromCharCode(10));
      eq(s.length,3);yes(s.getItemState(0),'initial-r');yes(!s.getItemState(1),'initial-notg');yes(s.getItemState(2),'initial-b');
      eq(s.getDisplayItem(1),'Green');eq(s.getSaveItem(1),'g');eq(s.boundItem('Blue'),'b');
      s.addItem('Amber','a');eq(s.length,4);s.setItemState(0,0);s.setItemState(1,1);s.setItemState(3,1);
      yes(!s.getItemState(0),'removed-r');yes(s.getItemState(1),'added-g');yes(s.getItemState(2),'kept-b');yes(s.getItemState(3),'added-a');
      s.deleteItem(0);eq(s.length,3);eq(s.getDisplayItem(0),'Green');eq(s.getSaveItem(2),'a');
      e.addItem('One');e.addItem('Two','2');eq(e.boundItem('One'),'One');eq(e.boundItem('Two'),'2');
      e.rawValue='2';eq(e.selectedIndex,1);e.deleteItem(-1);eq(e.length,0);e.addItem('Ready','ok');e.rawValue='ok';r.phase.rawValue='done';
    }else{
      s.deleteItem(-1);s.addItem('Green','g');s.addItem('Blue','b');s.addItem('Amber','a');s.rawValue=['g','b','a'].join(String.fromCharCode(10));
      e.deleteItem(-1);e.addItem('Ready','ok');e.rawValue='ok';
    }
    eq(s.length,3);eq(s.getSaveItem(0),'g');yes(s.getItemState(0),'final-g');yes(s.getItemState(1),'final-b');yes(s.getItemState(2),'final-a');
    eq(s.dataNode,null);eq(e.length,1);eq(e.getDisplayItem(0),'Ready');eq(e.getSaveItem(0),'ok');eq(text(e),'ok');
  )JS";
  Scenario("structured-multiselect",body,script,"<root><phase>before</phase><bound xfa:dataNode=\"dataGroup\"><value>r</value><value>b</value></bound></root>",
      [](Session& s){
        if(!s.host->alerts.empty()) {
          const bool native=s.api.FPDF_GetPageCount==Implementations()[0]->api().FPDF_GetPageCount;
          EXPECT_EQ(std::get<0>(s.host->alerts.front()),native?u"initial:rb:000":u"initial:r\nb:101");
        }
        EXPECT_EQ(s.api.FPDFPage_HasFormFieldAtPoint(s.form,s.page,5,115),FPDF_FORMFIELD_XFA_LISTBOX);
        // Select through real input so both visible controls share the same
        // selected rows even though native's script-to-FWL restoration uses
        // selection ordinals instead of item indices (GetSelItem bug).
        s.Click(8,115);s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_Home,0);
        s.api.FORM_OnKeyDown(s.form,s.page,FWL_VKEY_End,FWL_EVENTFLAG_ShiftKey);
        ASSERT_TRUE(s.Focus(8,30));s.api.FORM_ForceToKillFocus(s.form);
        OnePageInk(s,108,196);const auto image=Pages(s);
        Pixel(image[0],5,70,{255,255,255});
        for(int y:{115,129,143})Pixel(image[0],80,y,{153,193,218});
      },DefaultPages(),"",CompareReadyCombo,true);
}

void CompareDateBindings(const std::vector<Raster>& a,const std::vector<Raster>& b,Artifacts& artifacts,const std::string& label) {
  ASSERT_EQ(a.size(),1u);ASSERT_EQ(b.size(),1u);
  // DateTimePicker::DrawWidget draws its edit, then native WidgetMgr draws the
  // child again. Rust's draws_child contract avoids that duplicate paint.
  // Match the existing calendar scenario: require identical ink geometry and
  // compare every pixel outside the three composite edits normally.
  for(int top:{88,112,136}) {
    const auto bounds=[&](const Raster& image){std::array<int,5> out{180,top+24,-1,-1,0};
      for(int y=top;y<top+24;++y)for(int x=0;x<180;++x){const auto at=(static_cast<size_t>(y)*240+x)*4;
        if(image.pixels[at]<180&&image.pixels[at+1]<180&&image.pixels[at+2]<180){out[0]=std::min(out[0],x);out[1]=std::min(out[1],y);out[2]=std::max(out[2],x);out[3]=std::max(out[3],y);++out[4];}}
      return out;};
    const auto first=bounds(a[0]),second=bounds(b[0]);EXPECT_GT(first[4],5);EXPECT_GT(second[4],5);
    for(size_t coordinate=0;coordinate<4;++coordinate)EXPECT_NEAR(first[coordinate],second[coordinate],1);
  }
  for(const auto& rect:std::array<std::array<int,4>,3>{{{0,0,240,88},{0,160,240,80},{180,88,60,72}}})
    CompareRaster(CropRaster(a[0],rect[0],rect[1],rect[2],rect[3]),CropRaster(b[0],rect[0],rect[1],rect[2],rect[3]),artifacts,
                  label+"-outside-"+std::to_string(rect[0])+"-"+std::to_string(rect[1]),RasterTolerance{18,0,.15});
}

TEST(CoverageXfaBindingLayout, TypedBindingsNormalizeNumbersAndSplitDateTimeDisplayWithoutLosingRawData) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  auto body=Edit("amount","<numericEdit/>","<value><decimal leadDigits=\"-1\" fracDigits=\"2\"/></value>","<bind match=\"once\"/>")+
      Edit("zeros","<numericEdit/>","<value><float/></value>","<bind match=\"once\"/>")+
      Edit("day","<dateTimeEdit/>","<value><date/></value>","<bind match=\"once\"/>")+
      Edit("time","<dateTimeEdit/>","<value><time/></value>","<bind match=\"once\"/>")+
      Edit("stamp","<dateTimeEdit/>","<value><dateTime/></value>","<bind match=\"once\"/>");
  for(size_t at=0;(at=body.find("size=\"10pt\"",at))!=std::string::npos;)body.replace(at,11,"size=\"8pt\"");
  const auto script=R"JS(var r=xfa.form.root;
    eq(Number(r.amount.rawValue),1234.5);eq(Number(r.zeros.rawValue),0);
    eq(text(r.day),'2004-02-29');eq(text(r.time),'13:14:15');eq(text(r.stamp),'2004-02-29T13:14:15');
    eq(String(r.amount.formattedValue),'1,234.5');eq(String(r.day.formattedValue),'Feb 29, 2004');
    eq(String(r.time.formattedValue),'1:14:15 PM');
    eq(r.amount.dataNode,xfa.datasets.data.root.amount);eq(r.day.dataNode,xfa.datasets.data.root.day);
  )JS";
  // Metadata is a real XFA dataset representation of these canonical primitive
  // values. It also keeps native's pretty-print LF out of numeric/date lexemes;
  // no reader repair or extra date/number implementation is involved.
  Scenario("typed-bindings",body,script,
      "<root xfa:dataNode=\"dataGroup\" amount=\"0001234.500\" zeros=\"0000\" day=\"2004-02-29\" time=\"13:14:15\" stamp=\"2004-02-29T13:14:15\"><anchor/></root>",
      [](Session& s){OnePageInk(s,40,160);const auto images=Pages(s);for(int top:{40,64,88,112,136})forms::ExpectInk(images[0],0,top,180,top+24);},DefaultPages(),"",CompareDateBindings);
}

TEST(CoverageXfaBindingLayout, CaptionPlacementMarginsAndMinMaxConstraintsSizeActualWidgets) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const std::string placement:{"left","right","top","bottom","inline"}) {
    SCOPED_TRACE(placement);std::string body;
    const std::array<std::string,4> names{"text","number","check","button"};
    const std::array<std::string,4> uis{"<textEdit/>","<numericEdit/>","<checkButton size=\"10pt\"/>","<button/>"};
    for(size_t i=0;i<names.size();++i) {
      const auto caption="<caption placement=\""+placement+"\" reserve=\""+
          std::string(placement=="top"||placement=="bottom"?"12":"20")+"pt\"><margin leftInset=\"1pt\" rightInset=\"1pt\" topInset=\"1pt\" bottomInset=\"1pt\"/><value><text>Cap</text></value></caption>";
      body+=Edit(names[i],uis[i],i==1?"<value><decimal>12</decimal></value>":"<value><text>Hi</text></value>",
          "<bind match=\"none\"/>"+caption+"<margin leftInset=\"2pt\" rightInset=\"2pt\" topInset=\"2pt\" bottomInset=\"2pt\"/>",
          "minW=\"90pt\" maxW=\"120pt\" minH=\"24pt\" maxH=\"40pt\"");
    }
    body+="<draw name=\"end\" minW=\"70pt\" maxW=\"90pt\" minH=\"10pt\" maxH=\"20pt\"><value><rectangle><fill><color value=\"0,128,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw>";
    const auto script=R"JS(var r=xfa.form.root,l=xfa.layout,y=40;
      for(var i=0;i<4;i++){var n=r.resolveNode(['text','number','check','button'][i]);
        yes(l.w(n)>=90);yes(l.w(n)<=120);yes(l.h(n)>=24);yes(l.h(n)<=40);near(l.y(n),y);y+=l.h(n);}
      near(l.y(r.end),y);near(l.w(r.end),70);near(l.h(r.end),10);eq(l.pageCount(),1);
    )JS";
    Scenario("caption-auto-"+placement,body,script,"<root/>",[](Session& s){OnePageInk(s,40,210);});
  }
}

TEST(CoverageXfaBindingLayout, MultilineEditWithCaptionAndInsetsSplitsIntoVisiblePageFragments) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto story=[](int count){
    std::string lines;for(int i=0;i<count;++i)lines+="Row"+std::to_string(i)+" abcdefghijklmnopqrst&#10;";
    return Edit("story","<textEdit multiLine=\"1\"/>","<value><text>"+lines+"</text></value>",
        "<bind match=\"none\"/><keep intact=\"none\"/><caption placement=\"top\" reserve=\"14pt\"><value><text>Story</text></value></caption>"
        "<margin topInset=\"3pt\" bottomInset=\"3pt\" leftInset=\"2pt\" rightInset=\"2pt\"/><para lineHeight=\"12pt\"/>","w=\"120pt\"")+
        Block("end",{0,128,0},60,10);
  };
  const auto pages=DefaultPages("<contentArea w=\"180pt\" h=\"180pt\"/>");
  // The fitting control retains exactly the same caption, margins, line wrap
  // and autosizing. Both engines and all writer/reader pairs compare in full.
  Scenario("fitting-multiline-edit",story(3),R"JS(var r=xfa.form.root,l=xfa.layout;
    eq(text(r.story).split('\n').length,4);eq(l.pageCount(),1);eq(l.pageSpan(r.story),1);yes(l.h(r.story)>60);
  )JS","<root/>",[](Session& s){OnePageInk(s,40,180);},pages);
  const auto program=R"JS(var r=xfa.form.root,l=xfa.layout,native=text(r.report)=='N';
    eq(text(r.story).split('\n').length,31);yes(text(r.story).indexOf('Row29')>=0);
    if(!native){eq(l.pageCount(),5);eq(l.absPage(r.end),4);eq(l.pageSpan(r.story),5);eq(l.absPageSpan(r.story),5);for(var i=0;i<5;i++)yes(l.h(r.story,'pt',i)>20,'fragment'+i);}
  )JS";
  const auto long_pages=DefaultPages("<contentArea w=\"180pt\" h=\"180pt\"/>"+Block("sidebar",{0,0,255},20,60,"x=\"200pt\" y=\"60pt\""));
  std::string native_observed_pages;
  Artifacts artifacts;const auto bytes=Fixture(Probe("function clean(v){v=String(v);return v.charCodeAt(0)===10?v.slice(1):v;}function text(n){return clean(n.rawValue);}"+std::string(program))+story(30),"<root/>",long_pages);
  const auto inspect=[&](Session& s,bool native){
    // Mark the current reader through public text input, never serialized
    // backend-specific JavaScript. This also exercises repeated relayout.
    ASSERT_TRUE(s.Focus(8,30));ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));s.Replace(native?u"N":u"R");
    s.api.FORM_ForceToKillFocus(s.form);ASSERT_NO_FATAL_FAILURE(RunProbe(s));
    const auto images=Pages(s);ASSERT_FALSE(images.empty());
    if(native){
      native_observed_pages+=std::to_string(images.size())+",";
      // Native nests orphan continuations inside the current split subtree.
      // Their page count/paint can vary across reflows. Preserve the paragraph
      // assertions above and require the independent master sidebar below;
      // do not use this broken long-text paint as Rust's expected output.
    }else{
      ASSERT_EQ(images.size(),5u);
      for(size_t p=0;p<images.size();++p){
        SCOPED_TRACE(p);size_t black=0;
        for(int y=p?0:40;y<180;++y)for(int x=0;x<120;++x){const auto at=(static_cast<size_t>(y)*240+x)*4;black+=images[p].pixels[at]<100&&images[p].pixels[at+1]<100&&images[p].pixels[at+2]<100;}
        EXPECT_GT(black,100u);
      }
      size_t green=0;for(int y=0;y<180;++y)for(int x=0;x<120;++x){const auto at=(static_cast<size_t>(y)*240+x)*4;green+=images.back().pixels[at]==0&&images.back().pixels[at+1]==128&&images.back().pixels[at+2]==0;}EXPECT_EQ(green,600u);
    }
    for(const auto& image:images){
      for(int y=60;y<120;++y)for(int x=200;x<220;++x)Pixel(image,x,y,{0,0,255});
      for(const auto& [x,y]:std::array<std::pair<int,int>,4>{{{199,90},{220,90},{205,59},{205,120}}})Pixel(image,x,y,{255,255,255});
      Pixel(image,230,200,{255,255,255});
    }
  };
  std::array<std::vector<Raster>,2> original;size_t backend=0;
  for(const auto* writer:Implementations()){
    SCOPED_TRACE(writer->name());FlowSession loaded(writer->api(),bytes);auto& s=loaded.session;
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    const auto initial=Pages(s);for(size_t p=0;p<initial.size();++p)artifacts.Image(writer->name()+"-split-before-marker-"+std::to_string(p),initial[p]);
    ASSERT_NO_FATAL_FAILURE(inspect(s,writer->name()=="native"));original[backend++]=Pages(s);
    for(size_t p=0;p<original[backend-1].size();++p)artifacts.Image(writer->name()+"-split-original-"+std::to_string(p),original[backend-1][p]);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-split-multiline-edit");ASSERT_FALSE(saved.empty());
    std::array<std::vector<Raster>,2> reloaded;size_t reader_index=0;
    for(const auto* reader:Implementations()){
      SCOPED_TRACE(reader->name());FlowSession loaded_reader(reader->api(),saved);auto& r=loaded_reader.session;
      ASSERT_TRUE(r.valid());ASSERT_TRUE(r.xfa_loaded);
      ASSERT_NO_FATAL_FAILURE(inspect(r,reader->name()=="native"));reloaded[reader_index++]=Pages(r);
      if(writer->name()=="rust"&&reader->name()=="rust")ComparePages(original[backend-1],reloaded[reader_index-1],artifacts,"split-rust-save-original");
    }
    const auto sidebar=CropRaster(reloaded[1][0],190,50,40,80);
    for(size_t reader=0;reader<reloaded.size();++reader)for(size_t page=0;page<reloaded[reader].size();++page)
      CompareRaster(sidebar,CropRaster(reloaded[reader][page],190,50,40,80),artifacts,writer->name()+"-split-sidebar-"+std::to_string(reader)+"-"+std::to_string(page));
  }
  CompareRaster(CropRaster(original[0][0],190,50,40,80),CropRaster(original[1][0],190,50,40,80),artifacts,"split-sidebar-backends");
  ::testing::Test::RecordProperty("native_long_split_observed_page_counts",native_observed_pages);
}

TEST(CoverageXfaBindingLayout, ScriptedBeforeAndAfterBreaksGenerateBookendsAcrossNamedContentAreas) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(bool before:{false,true})for(bool enabled:{false,true}) {
    SCOPED_TRACE(before);
    SCOPED_TRACE(enabled);
    const auto proto=std::string("<proto><subform id=\"head\" name=\"head\" layout=\"position\" w=\"60pt\" h=\"10pt\"><occur min=\"1\" max=\"-1\"/>")+
        Block("red",{255,0,0},60,10)+"</subform><subform id=\"tail\" name=\"tail\" layout=\"position\" w=\"60pt\" h=\"10pt\"><occur min=\"1\" max=\"-1\"/>"+
        Block("blue",{0,0,255},60,10)+"</subform></proto>";
    const std::string kind=before?"breakBefore":"breakAfter";
    const auto breaker="<"+kind+" targetType=\"contentArea\" target=\"#right\" startNew=\"1\" leader=\"#head\" trailer=\"#tail\"><script contentType=\"application/x-javascript\">"+
        std::string(enabled?"true;":"false;")+"</script></"+kind+">";
    // PrepareFirstPage follows the first Subform chain. Make the actual
    // prologue the first container, rather than allowing the later section's
    // BreakBefore to select the initial area before any prologue is laid out.
    const auto body=proto+"<subform name=\"prologue\" layout=\"position\" w=\"60pt\" h=\"20pt\">"+
        Block("first",{0,128,0},60,20)+"</subform><subform name=\"section\" layout=\"tb\" w=\"60pt\">"+
        breaker+Block("second",{0,128,0},60,20)+"</subform>"+Block("end",{255,128,0},60,10);
    const auto program="var r=xfa.form.root,l=xfa.layout;eq(l.pageCount(),1);near(l.x(r.prologue.first),0);near(l.y(r.prologue.first),0);near(l.y(r.prologue),40);near(l.x(r.end),"+std::to_string(0)+");";
    Scenario("script-break-"+std::to_string(before)+std::to_string(enabled),body,program,"<root/>",[=](Session& s){
      const auto images=Pages(s);ASSERT_EQ(images.size(),1u);
      int green=0,red=0,blue=0,orange=0;
      for(int x:{5,125})for(int y=40*(x==5);y<100;++y){
        const auto at=(static_cast<size_t>(y)*240+x)*4;RGB c{images[0].pixels[at+2],images[0].pixels[at+1],images[0].pixels[at]};
        green+=c==RGB{0,128,0};red+=c==RGB{255,0,0};blue+=c==RGB{0,0,255};orange+=c==RGB{255,128,0};
      }
      EXPECT_EQ(green,40);EXPECT_EQ(orange,10);EXPECT_EQ(red,enabled?10:0);EXPECT_EQ(blue,enabled?10:0);
      if(enabled){Pixel(images[0],125,5,{255,0,0});Pixel(images[0],5,before?65:85,{0,0,255});}
      else Pixel(images[0],5,85,{255,128,0});
      Pixel(images[0],200,200,{255,255,255});
    },DefaultPages("<contentArea name=\"left\" id=\"left\" w=\"100pt\" h=\"100pt\"/><contentArea name=\"right\" id=\"right\" x=\"120pt\" w=\"100pt\" h=\"100pt\"/>"));
  }
}

TEST(CoverageXfaBindingLayout, DataDescriptionsConstrainGeneratedNodesAndPreserveExistingMetadataBindings) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // Metadata already exists as an XML attribute; this avoids native's separate
  // known malformed-attribute creation bug while exercising descriptor links,
  // grouped descriptor search and refusal to create undeclared data nodes.
  const auto body=Field("allowed","Seed","","<bind match=\"once\"/>")+
      Field("blocked","Local","","<bind match=\"once\"/>")+
      Field("meta","Fallback","","<bind match=\"once\"/>")+
      Block("end",{0,128,0},60,10);
  const auto program=R"JS(var r=xfa.form.root,d=xfa.datasets.data.root;
    eq(text(r.allowed),'Seed');eq(text(r.blocked),'Local');eq(text(r.meta),'Bound');
    yes(r.allowed.dataNode);eq(r.blocked.dataNode,null);yes(r.meta.dataNode);
    eq(d.resolveNode('blocked'),null);eq(clean(d.allowed.value),'Seed');
    eq(r.meta.dataNode,d.resolveNode('meta'));yes(d.saveXML().indexOf('meta="Bound"')>=0);
    eq(xfa.layout.pageCount(),1);near(xfa.layout.y(r.end),100);
  )JS";
  Scenario("description-bindings",body,program,"<root meta=\"Bound\" xfa:dataNode=\"dataGroup\"/>",
      [](Session& s){OnePageInk(s,40,110);Pixel(Pages(s)[0],5,105,{0,128,0});},DefaultPages(),
      "<dd:dataDescription xmlns:dd=\"http://ns.adobe.com/data-description/\"><root meta=\"\"><dd:group><allowed/></dd:group></root></dd:dataDescription>");
}

TEST(CoverageXfaBindingLayout, DatasetImageContentTypeFeedsIntrinsicSizeConstraintsAndRenderedImage) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  // This pinned decoder fixture is a solid red250x250 PNG. Bind its bytes
  // through datasets metadata rather than an inline form value, then let the
  // real image resource and field autosize path apply the80pt upper bound.
  std::ifstream input(TestRoot()/"upstream/pdfium/testing/resources/pixel/xfa_specific/xfa_png_image.in",std::ios::binary);
  ASSERT_TRUE(input.good());const std::string source((std::istreambuf_iterator<char>(input)),{});
  const auto start=source.find("<image "),content=source.find('>',start),end=source.find("</image>",content);
  ASSERT_NE(start,std::string::npos);ASSERT_NE(content,std::string::npos);ASSERT_NE(end,std::string::npos);
  const auto encoded=source.substr(content+1,end-content-1);
  const auto body=Edit("picture","<imageEdit/>","<value><image transferEncoding=\"base64\" aspect=\"none\"/></value>","<bind match=\"once\"/>",
      "minW=\"40pt\" maxW=\"80pt\" minH=\"40pt\" maxH=\"80pt\"")+Block("end",{0,128,0},60,10);
  const auto program=R"JS(var r=xfa.form.root,l=xfa.layout,p=r.picture;
    eq(p.dataNode,xfa.datasets.data.root.picture);eq(p.value.image.contentType,'image/png');
    eq(p.value.image.transferEncoding,'base64');yes(String(p.rawValue).length>100);
    near(l.w(p),80);near(l.h(p),80);near(l.y(p),40);near(l.y(r.end),120);
  )JS";
  Scenario("dataset-image-size",body,program,"<root><picture xfa:contentType=\"image/png\">"+encoded+"</picture></root>",
      [](Session& s){OnePageInk(s,40,130);const auto image=Pages(s);
        for(const auto& [x,y]:std::array<std::pair<int,int>,3>{{{5,45},{40,80},{75,115}}})Pixel(image[0],x,y,{255,0,0});
        Pixel(image[0],5,125,{0,128,0});Pixel(image[0],85,80,{255,255,255});});
}

}  // namespace
}  // namespace pdfoxium::integration
