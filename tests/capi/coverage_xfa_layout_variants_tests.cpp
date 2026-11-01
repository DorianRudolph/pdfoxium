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
// Fourth source-derived layout batch: keep all observations at the public C API.
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
void RunProbe(Session& s, bool master = false, int offset_x=0, int offset_y=0) {
  s.api.FORM_ForceToKillFocus(s.form);
  s.Click((master ? 205 : 8)+offset_x, 10+offset_y);
  ReopenPages(s);
  ASSERT_TRUE(s.Focus((master ? 185 : 8)+offset_x, (master ? 35 : 30)+offset_y));
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
              const std::string& pages=DefaultPages(),const std::string& description="",const SceneComparison& comparison={},bool mark_reader=false,const std::function<void(Session&)>& setup={},int offset_x=0,int offset_y=0) {
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
    if(setup){ASSERT_NO_FATAL_FAILURE(setup(s));}
    ASSERT_NO_FATAL_FAILURE(RunProbe(s,false,offset_x,offset_y));
    ASSERT_NO_FATAL_FAILURE(inspect(s));
    images[backend++]=Pages(s);
    for(size_t page=0;page<images[backend-1].size();++page)artifacts.Image(library->name()+"-"+name+"-original-"+std::to_string(page),images[backend-1][page]);
    ASSERT_NO_FATAL_FAILURE(FourWay(*library,s,artifacts,name,[&](Session& r){
      if(mark_reader){ASSERT_TRUE(r.Focus(8,30));ASSERT_TRUE(r.api.FORM_SelectAllText(r.form,r.page));r.Replace(r.api.FPDF_GetPageCount==Implementations()[0]->api().FPDF_GetPageCount?u"N":u"R");r.api.FORM_ForceToKillFocus(r.form);}
      if(setup){ASSERT_NO_FATAL_FAILURE(setup(r));}
      ASSERT_NO_FATAL_FAILURE(RunProbe(r,false,offset_x,offset_y));
      ASSERT_NO_FATAL_FAILURE(inspect(r));
    },true,comparison));
  }
  if(comparison)comparison(images[0],images[1],artifacts,name+"-backends");
  else ComparePages(images[0],images[1],artifacts,name+"-backends");
}
TEST(CoverageXfaLayoutVariants, RotatedAnchorsKeepNestedBoundsAndTextHitTestingConsistent) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Anchor { const char* name; int col,row; };
  const std::array<Anchor,9> anchors={{{"topLeft",0,0},{"topCenter",1,0},{"topRight",2,0},
      {"middleLeft",0,1},{"middleCenter",1,1},{"middleRight",2,1},
      {"bottomLeft",0,2},{"bottomCenter",1,2},{"bottomRight",2,2}}};
  // For a clockwise quarter turn, the original anchor coordinate (c,r)
  // becomes (r,2-c). The 40x20 field occupies an actual20x40 layout box.
  for(int rotation:{0,90,180,270})for(const auto& anchor:anchors) {
    if(rotation!=90&&std::string(anchor.name)!="middleCenter")continue;
    SCOPED_TRACE(rotation);
    SCOPED_TRACE(anchor.name);
    int col=anchor.col,row=anchor.row;
    for(int turn=0;turn<rotation/90;++turn){const int old_col=col;col=row;row=2-old_col;}
    const int width=rotation%180?20:40,height=rotation%180?40:20;
    const int x=80-col*width/2,y=70-row*height/2;
    const auto body="<subform name=\"outer\" layout=\"position\" x=\"12pt\" w=\"180pt\" h=\"150pt\">"
        "<subform name=\"inner\" layout=\"position\" x=\"10pt\" y=\"10pt\" w=\"160pt\" h=\"130pt\">"+
        Block("background",{255,0,0},40,20,"x=\"80pt\" y=\"70pt\" anchorType=\""+std::string(anchor.name)+"\" rotate=\""+std::to_string(rotation)+"\"")+
        Field("input","X","x=\"80pt\" y=\"70pt\" anchorType=\""+std::string(anchor.name)+"\" rotate=\""+std::to_string(rotation)+"\"","<bind match=\"none\"/>",40)+
        "</subform></subform>";
    const auto program="var l=xfa.layout,g=xfa.form.root.outer.inner;near(l.x(g),10);near(l.y(g),10);"
        "near(l.x(g.input),"+std::to_string(x)+");near(l.y(g.input),"+std::to_string(y)+");"
        "near(l.w(g.input),"+std::to_string(width)+");near(l.h(g.input),"+std::to_string(height)+");"
        "near(l.x(g.background),"+std::to_string(x)+");near(l.y(g.background),"+std::to_string(y)+");eq(text(g.input),'X');";
    Scenario("rotated-anchor-"+std::to_string(rotation)+anchor.name,body,program,"<root/>",[=](Session& s){
      const auto images=Pages(s);ASSERT_EQ(images.size(),1u);
      // Outer x is ignored by its flowed parent; inner x/y remain positioned.
      const int left=10+x,top=50+y;
      Pixel(images[0],left+width/2,top+height/2,{255,0,0});
      Pixel(images[0],left-2,top+2,{255,255,255});
      ASSERT_TRUE(s.Focus(left+width/2,top+height/2));
      EXPECT_EQ(s.Text(),u"X");s.api.FORM_ForceToKillFocus(s.form);
    });
  }
}

TEST(CoverageXfaLayoutVariants, WholeRowSpansAndHiddenCellsDoNotConsumeExhaustedColumns) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(bool hidden:{false,true}) {
    SCOPED_TRACE(hidden);
    const auto cell=[](const char* name,RGB color,const std::string& attrs=""){
      return Block(name,color,180,20,attrs);
    };
    const auto body="<subform name=\"table\" layout=\"table\" w=\"180pt\" columnWidths=\"30pt 50pt 100pt\">"
        "<subform name=\"whole\" layout=\"row\">"+cell("red",{255,0,0},"colSpan=\"-1\"")+cell("empty",{255,0,255})+"</subform>"
        "<subform name=\"partial\" layout=\"row\">"+cell("skip",{255,0,255},hidden?"presence=\"hidden\"":"presence=\"invisible\"")+
        cell("green",{0,128,0},"colSpan=\"2\"")+(hidden?cell("blue",{0,0,255}):std::string())+"</subform>"
        "<subform name=\"wide\" layout=\"row\">"+cell("yellow",{255,255,0},"colSpan=\"7\"")+"</subform></subform>";
    const int green_x=hidden?0:30,green_width=hidden?80:150;
    const auto script="var t=xfa.form.root.table,l=xfa.layout;near(l.w(t.whole.red),180);near(l.w(t.whole.empty),0);"
        "near(l.x(t.partial.green),"+std::to_string(green_x)+");near(l.w(t.partial.green),"+std::to_string(green_width)+");"+
        (hidden?"near(l.w(t.partial.blue),100);":std::string())+"near(l.w(t.wide.yellow),180);near(l.h(t),60);";
    Scenario(hidden?"hidden-span":"invisible-span",body,script,"<root/>",[=](Session& s){
      const auto images=Pages(s);ASSERT_EQ(images.size(),1u);const auto& image=images[0];
      Pixel(image,10,50,{255,0,0});Pixel(image,170,50,{255,0,0});
      Pixel(image,green_x+5,70,{0,128,0});
      Pixel(image,5,70,hidden?RGB{0,128,0}:RGB{255,255,255});
      if(hidden)Pixel(image,170,70,{0,0,255});
      Pixel(image,170,90,{255,255,0});Pixel(image,190,90,{255,255,255});
    });
  }
}

TEST(CoverageXfaLayoutVariants, PositionedAutosizeUsesVisibleExtentsAndMarginsWhileHiddenChildrenStayOut) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(bool visible:{false,true}) {
    SCOPED_TRACE(visible);
    const auto body="<subform name=\"positioned\" layout=\"position\">"
        "<margin leftInset=\"5pt\" rightInset=\"7pt\" topInset=\"3pt\" bottomInset=\"9pt\"/>"+
        Block("red",{255,0,0},30,20,"x=\"10pt\" y=\"15pt\"")+
        Block("blue",{0,0,255},20,10,"x=\"80pt\" y=\"50pt\" presence=\""+std::string(visible?"visible":"hidden")+"\"")+
        "<subform name=\"nested\" layout=\"position\" x=\"45pt\" y=\"20pt\">"+
        Block("green",{0,128,0},20,10,"x=\"2pt\" y=\"4pt\"")+"</subform></subform>"+
        Block("after",{255,255,0},20,10);
    const int height=visible?72:47,width=visible?112:79;
    const auto script="var r=xfa.form.root,p=r.positioned,l=xfa.layout;near(l.w(p),"+std::to_string(width)+");"
        "near(l.h(p),"+std::to_string(height)+");near(l.w(p.nested),22);near(l.h(p.nested),14);"
        "near(l.x(p.red),10);near(l.y(p.red),15);near(l.y(r.after),"+std::to_string(40+height)+");";
    Scenario(visible?"autosize-visible":"autosize-hidden",body,script,"<root/>",[=](Session& s){
      const auto image=Pages(s);ASSERT_EQ(image.size(),1u);
      Pixel(image[0],17,60,{255,0,0});Pixel(image[0],54,69,{0,128,0});
      Pixel(image[0],90,95,visible?RGB{0,0,255}:RGB{255,255,255});
      Pixel(image[0],5,45+height,{255,255,0});
    });
  }
}

TEST(CoverageXfaLayoutVariants, ClickEventsRelayoutHiddenAndInvisibleSubformsWithoutLosingVisibleOrder) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const std::string presence:{"visible","invisible","hidden"}) {
    SCOPED_TRACE(presence);
    const auto button="<field name=\"mutate\" w=\"90pt\" h=\"20pt\"><bind match=\"none\"/><ui><button/></ui>"
        "<font typeface=\"Helvetica\" size=\"8pt\"/><caption><value><text>Mode</text></value></caption>"+
        forms::Event("var r=xfa.form.root;r.stream.middle.presence='"+presence+"';xfa.layout.relayout();","click")+"</field>";
    const auto body=button+"<subform name=\"stream\" layout=\"tb\" w=\"60pt\">"+
        Block("red",{255,0,0},60,20)+"<subform name=\"middle\" layout=\"tb\" w=\"60pt\">"+
        Block("green",{0,128,0},60,30)+"</subform>"+Block("blue",{0,0,255},60,20)+"</subform>";
    const int blue_y=presence=="hidden"?20:50;
    const auto script="var g=xfa.form.root.stream,l=xfa.layout;eq(g.middle.presence,'"+presence+"');"
        "near(l.y(g.blue),"+std::to_string(blue_y)+");near(l.h(g),"+std::to_string(blue_y+20)+");eq(l.pageCount(),1);";
    const auto setup=[](Session& s){s.Click(8,50);ReopenPages(s);};
    Scenario("live-presence-"+presence,body,script,"<root/>",[=](Session& s){
      const auto images=Pages(s);ASSERT_EQ(images.size(),1u);
      Pixel(images[0],5,65,{255,0,0});
      Pixel(images[0],5,85,presence=="hidden"?RGB{0,0,255}:presence=="visible"?RGB{0,128,0}:RGB{255,255,255});
      Pixel(images[0],5,65+blue_y,{0,0,255});Pixel(images[0],80,100,{255,255,255});
    },DefaultPages(),"",{},false,setup);
  }
}

TEST(CoverageXfaLayoutVariants, FormOverlayUsesViewRelevanceWithNormalAndPrintingRenderFlags) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto body="<subform name=\"scene\" layout=\"position\" w=\"180pt\" h=\"100pt\">"+
      Block("view",{255,0,0},40,20,"x=\"10pt\" y=\"10pt\" relevant=\"-print\"")+
      Block("print",{0,0,255},40,20,"x=\"70pt\" y=\"10pt\" relevant=\"+print\"")+
      Block("both",{0,128,0},40,20,"x=\"10pt\" y=\"50pt\"")+"</subform>";
  Artifacts artifacts;std::array<std::array<Raster,2>,2> original;size_t backend=0;
  const auto inspect=[](Session& s){
    ASSERT_EQ(s.api.FPDF_GetPageCount(s.document),1);
    const auto view=s.Draw(240,240),print=s.Draw(240,240,0,FPDF_PRINTING);
    Pixel(view,20,60,{255,0,0});Pixel(view,80,60,{255,255,255});Pixel(view,20,100,{0,128,0});
    // FFLCommon creates a view OCContext; DrawFocusAnnot always iterates
    // kViewable widgets. FPDF_PRINTING does not select a print-only overlay.
    Pixel(print,20,60,{255,0,0});Pixel(print,80,60,{255,255,255});Pixel(print,20,100,{0,128,0});
  };
  const auto bytes=Fixture(Probe("var l=xfa.layout,g=xfa.form.root.scene;near(l.w(g),180);near(l.h(g),100);eq(g.view.relevant,'-print');eq(g.print.relevant,'+print');")+body);
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());FlowSession loaded(writer->api(),bytes);auto& s=loaded.session;
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(RunProbe(s));
    ASSERT_NO_FATAL_FAILURE(inspect(s));
    original[backend++]={s.Draw(240,240),s.Draw(240,240,0,FPDF_PRINTING)};
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-relevance");ASSERT_FALSE(saved.empty());
    std::array<std::array<Raster,2>,2> reloaded;size_t reader_index=0;
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());FlowSession again(reader->api(),saved);auto& r=again.session;
      ASSERT_TRUE(r.valid());ASSERT_TRUE(r.xfa_loaded);
      ASSERT_NO_FATAL_FAILURE(RunProbe(r));
      ASSERT_NO_FATAL_FAILURE(inspect(r));
      reloaded[reader_index++]={r.Draw(240,240),r.Draw(240,240,0,FPDF_PRINTING)};
    }
    for(int mode=0;mode<2;++mode) {
      CompareRaster(reloaded[0][mode],reloaded[1][mode],artifacts,"relevance-reload-"+writer->name()+std::to_string(mode),RasterTolerance{18,0,.15});
      if(writer->name()=="rust")CompareRaster(original[backend-1][mode],reloaded[1][mode],artifacts,"relevance-rust-save"+std::to_string(mode),RasterTolerance{18,0,.15});
    }
  }
  for(int mode=0;mode<2;++mode)CompareRaster(original[0][mode],original[1][mode],artifacts,"relevance-backends"+std::to_string(mode),RasterTolerance{18,0,.15});
}

TEST(CoverageXfaLayoutVariants, LeftAndRightRowsContinueAcrossContentAreasWithExactColorConservation) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(const std::string flow:{"lr-tb","rl-tb"}) {
    SCOPED_TRACE(flow);
    const std::array<RGB,6> colors={{{255,0,0},{0,128,0},{0,0,255},{255,255,0},{0,255,255},{255,0,255}}};
    std::string body="<subform name=\"rows\" layout=\""+flow+"\" w=\"100pt\"><keep intact=\"none\"/>";
    for(int i=0;i<6;++i)body+="<subform name=\"cell"+std::to_string(i)+"\" layout=\"position\" w=\"50pt\" h=\"40pt\">"+
        Block("paint",colors[i],50,40)+"</subform>";
    body+="</subform>";
    const auto pages=DefaultPages("<contentArea name=\"left\" x=\"10pt\" y=\"10pt\" w=\"100pt\" h=\"100pt\"/>"
        "<contentArea name=\"right\" x=\"130pt\" y=\"10pt\" w=\"100pt\" h=\"100pt\"/>");
    const int first_x=flow=="lr-tb"?0:50,second_x=50-first_x;
    const auto script="var g=xfa.form.root.rows,l=xfa.layout;near(l.x(g.cell0),"+std::to_string(first_x)+");"
        "near(l.x(g.cell1),"+std::to_string(second_x)+");eq(l.pageCount(),1);eq(l.page(g.cell0),1);eq(l.page(g.cell5),1);"
        "yes(l.pageContent(0,'contentArea').length===2);near(l.h(g.cell0),40);near(l.h(g.cell5),40);";
    // After the report-triggered relayout, pinned native drops this visible
    // button's caption in the two-content-area fixture. Its public click and
    // PASS report still work. Rust must retain the caption; compare every
    // other pixel, including the report and all six conserved color blocks.
    const SceneComparison compare_cross_area=[](const std::vector<Raster>& a,
        const std::vector<Raster>& b,Artifacts& artifacts,const std::string& label){
      ASSERT_EQ(a.size(),b.size());
      auto masked_a=a,masked_b=b;
      for(size_t page=0;page<a.size();++page){
        artifacts.Image(label+"-unmasked-a-"+std::to_string(page),a[page]);
        artifacts.Image(label+"-unmasked-b-"+std::to_string(page),b[page]);
        if(page==0)for(int y=10;y<30;++y)for(int x=10;x<100;++x){
          for(int channel=0;channel<3;++channel){
            masked_a[page].pixels[(y*240+x)*4+channel]=255;
            masked_b[page].pixels[(y*240+x)*4+channel]=255;
          }
        }
      }
      ComparePages(masked_a,masked_b,artifacts,label);
    };
    Scenario("cross-area-"+flow,body,script,"<root/>",[=](Session& s){
      const auto images=Pages(s);ASSERT_EQ(images.size(),1u);const auto& image=images[0];
      if(s.api.FPDF_GetPageCount!=Implementations()[0]->api().FPDF_GetPageCount){
        const auto caption=CropRaster(image,10,10,90,20);
        size_t ink=0;
        for(size_t at=0;at<caption.pixels.size();at+=4)
          ink+=caption.pixels[at]<200&&caption.pixels[at+1]<200&&caption.pixels[at+2]<200;
        EXPECT_GT(ink,20u)<<"Rust must retain the visible Probe caption after relayout";
      }
      // Controls consume40pt in the first area. The first color row fits;
      // the other two rows continue at the start of the second area.
      Pixel(image,10+first_x+5,55,colors[0]);Pixel(image,10+second_x+5,55,colors[1]);
      Pixel(image,130+first_x+5,15,colors[2]);Pixel(image,130+second_x+5,15,colors[3]);
      Pixel(image,130+first_x+5,55,colors[4]);Pixel(image,130+second_x+5,55,colors[5]);
      for(RGB color:colors){
        size_t count=0;
        for(size_t at=0;at<image.pixels.size();at+=4){
          count+=image.pixels[at]==color[2]&&image.pixels[at+1]==color[1]&&image.pixels[at+2]==color[0];
        }
        EXPECT_EQ(count,2000u);
      }
      Pixel(image,120,50,{255,255,255});
    },pages,"",compare_cross_area,false,{},10,10);
  }
}

TEST(CoverageXfaLayoutVariants, NestedOrderedPageSetMinimumCreatesRequiredTrailingMasterPages) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int minimum:{1,2}) {
    SCOPED_TRACE(minimum);
    const auto first="<pageArea name=\"first\" id=\"first\"><occur min=\"1\" max=\"1\"/>"
        "<medium short=\"240pt\" long=\"240pt\"/><contentArea w=\"180pt\" h=\"180pt\"/>"+
        Block("firstMaster",{255,0,0},40,20,"x=\"190pt\" y=\"100pt\"")+"</pageArea>";
    // Required nested blank masters have no contentArea. They are trailing
    // stationery, not additional body overflow targets, and must still paint.
    const auto blank="<pageArea name=\"stationery\" id=\"stationery\"><occur min=\""+std::to_string(minimum)+"\" max=\""+std::to_string(minimum)+"\"/>"
        "<medium short=\"240pt\" long=\"240pt\"/>"+
        Block("stationeryMaster",{0,0,255},40,20,"x=\"190pt\" y=\"100pt\"")+"</pageArea>";
    const auto pages="<pageSet relation=\"orderedOccurrence\"><occur min=\"1\" max=\"1\"/>"+first+
        "<pageSet name=\"appendix\" relation=\"orderedOccurrence\"><occur min=\"1\" max=\"1\"/>"+blank+"</pageSet></pageSet>";
    const auto script="var l=xfa.layout;eq(l.pageCount(),"+std::to_string(minimum+1)+");eq(l.pageContent(0,'pageArea').item(0).name,'first');"
        "for(var p=1;p<l.pageCount();++p){eq(l.pageContent(p,'pageArea').item(0).name,'stationery');eq(l.pageContent(p,'contentArea').length,0);}";
    Scenario("nested-minimum-"+std::to_string(minimum),Block("bodyPaint",{0,128,0},50,20),script,"<root/>",[=](Session& s){
      const auto images=Pages(s);ASSERT_EQ(images.size(),static_cast<size_t>(minimum+1));
      Pixel(images[0],200,110,{255,0,0});Pixel(images[0],10,50,{0,128,0});
      for(int i=1;i<=minimum;++i){Pixel(images[i],200,110,{0,0,255});Pixel(images[i],10,50,{255,255,255});}
    },pages);
  }
}

TEST(CoverageXfaLayoutVariants, EmptyPageSetSynthesizesLetterMediumAndQuarterInchContentInsets) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto program="var l=xfa.layout,r=xfa.form.root;eq(l.pageCount(),1);near(l.w(r.red),40);near(l.h(r.red),20);"
      "eq(l.pageContent(0,'pageArea').length,1);eq(l.pageContent(0,'contentArea').length,1);"
      "var a=l.pageContent(0,'contentArea').item(0);eq(a.x,'0.25in');eq(a.y,'0.25in');eq(a.w,'8in');eq(a.h,'10.5in');";
  const auto bytes=Fixture(Probe(program)+Block("red",{255,0,0},40,20),"<root/>","<pageSet/>");
  Artifacts artifacts;std::array<Raster,2> originals;size_t writer_index=0;
  const auto inspect=[](Session& s){
    ASSERT_EQ(s.api.FPDF_GetPageCount(s.document),1);
    EXPECT_FLOAT_EQ(s.api.FPDF_GetPageWidthF(s.page),612);EXPECT_FLOAT_EQ(s.api.FPDF_GetPageHeightF(s.page),792);
    s.api.FORM_ForceToKillFocus(s.form);s.Click(26,28);ReopenPages(s);
    ASSERT_TRUE(s.Focus(26,48));ASSERT_EQ(s.Text(),u"PASS");s.api.FORM_ForceToKillFocus(s.form);
    const auto image=s.Draw(612,792);Pixel(image,20,60,{255,0,0});Pixel(image,15,60,{255,255,255});
    Pixel(image,20,80,{255,255,255});Pixel(image,610,790,{255,255,255});
  };
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());FlowSession loaded(writer->api(),bytes);auto& s=loaded.session;
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    ASSERT_NO_FATAL_FAILURE(inspect(s));originals[writer_index++]=s.Draw(612,792);
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-default-page");ASSERT_FALSE(saved.empty());
    std::array<Raster,2> reloaded;size_t reader_index=0;
    for(const auto* reader:Implementations()) {
      SCOPED_TRACE(reader->name());FlowSession again(reader->api(),saved);auto& r=again.session;
      ASSERT_TRUE(r.valid());ASSERT_TRUE(r.xfa_loaded);
      ASSERT_NO_FATAL_FAILURE(inspect(r));reloaded[reader_index++]=r.Draw(612,792);
    }
    CompareRaster(reloaded[0],reloaded[1],artifacts,writer->name()+"-default-readers",RasterTolerance{18,0,.15});
    if(writer->name()=="rust")CompareRaster(originals[writer_index-1],reloaded[1],artifacts,"default-rust-save",RasterTolerance{18,0,.15});
  }
  CompareRaster(originals[0],originals[1],artifacts,"default-backends",RasterTolerance{18,0,.15});
}

}  // namespace
}  // namespace pdfoxium::integration
