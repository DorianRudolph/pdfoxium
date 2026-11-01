#include "pinned_test_fonts.h"
#include "xfa_form_fixture.h"

#include <algorithm>
#include <array>
#include <functional>
#include <string>
#include <utility>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using RGB = std::array<int, 3>;

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
                             const std::string& page_set = DefaultPages()) {
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
      PdfStream("", "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data>" + data + "</xfa:data></xfa:datasets>"),
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
  return "function eq(a,b){if(JSON.stringify(a)!==JSON.stringify(b))throw 'expected '+JSON.stringify(b)+' got '+JSON.stringify(a);}"
      "function near(a,b){if(Math.abs(a-b)>.002)throw 'expected '+b+' got '+a;}"
      "function yes(a){if(!a)throw 'assertion';}"
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
  EXPECT_EQ(s.Text(), u"PASS");
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
             bool text = true) {
  const auto original = Pages(s);
  const auto saved = SaveDocument(s.api, s.document, artifacts, writer.name() + "-" + label);
  ASSERT_FALSE(saved.empty());
  std::array<std::vector<Raster>, 2> images;
  size_t backend = 0;
  for (const auto* reader : Implementations()) {
    SCOPED_TRACE(writer.name() + " -> " + reader->name());
    FlowSession loaded(reader->api(), saved); auto& reopened = loaded.session;
    ASSERT_TRUE(reopened.valid()); ASSERT_TRUE(reopened.xfa_loaded);
    inspect(reopened);
    images[backend++] = Pages(reopened);
    // Native mixed/text dataset serialization can insert a leading newline.
    // Both readers still inspect identical bytes; Rust output must also retain
    // the original appearance rather than learning that serialization defect.
    if (writer.name() == "rust") ComparePages(original, images[backend - 1], artifacts, label + "-original", text);
  }
  ComparePages(images[0], images[1], artifacts, label + "-readers", text);
}

TEST(CoverageXfaFlow, LiveLayoutQueriesUseActualGeometryUnitsPageContentAndErrorRecovery) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::string program = R"JS(
    var l=xfa.layout,r=xfa.form.root,n=r.group.red;
    yes(l.ready);throws(function(){l.ready=false;});
    eq(l.pageCount(),1);eq(l.absPageCount(),1);eq(l.sheetCount(),1);
    eq(l.absPageCountInBatch(),0);eq(l.sheetCountInBatch(),0);
    eq(l.page(n),1);eq(l.absPage(n),0);eq(l.sheet(n),0);
    eq(l.pageSpan(n),1);eq(l.absPageSpan(n),1);
    eq(l.absPageInBatch(n),0);eq(l.sheetInBatch(n),0);
    near(l.x(n),24);near(l.y(n),36);near(l.w(n),72);near(l.h(n),36);
    near(l.x(n,'in'),.333);near(l.y(n,'in'),.5);
    near(l.w(n,'in'),1);near(l.h(n,'in'),.5);
    near(l.w(n,'cm'),2.54);near(l.h(n,'mm'),12.7);near(l.w(n,'pc'),6);near(l.w(n,'mp'),72000);
    near(l.w(n,''),72);eq(l.w(n,'pt',99),0);eq(l.h(n,'pt',-1),36);
    eq(typeof l.x({}),'undefined');eq(l.page({}),0);eq(typeof l.pageSpan({}),'undefined');
    eq(l.pageContent(0,'pageArea').length,1);eq(l.pageContent(0,'contentArea').length,1);
    eq(l.pageContent(0,'draw').length,1);eq(l.pageContent(0,'draw').item(0).name,'red');
    eq(l.pageContent(0,'field').length,3);eq(l.pageContent(0,'subform').length,2);
    eq(l.pageContent(0,'area').length,0);eq(l.pageContent(0,'unknown').length,0);
    eq(l.pageContent(0,'field',true).length,0);eq(l.pageContent(-1,'draw').length,0);
    eq(l.pageContent(99,'draw').length,0);yes(l.pageContent(0).length>=7);
    throws(function(){l.x();});throws(function(){l.w(n,'nonsense');});throws(function(){l.w(n,'px');});
    throws(function(){l.h(n,'pt',0,1);});throws(function(){l.page();});
    throws(function(){l.pageSpan();});throws(function(){l.absPageInBatch();});
    throws(function(){l.sheetInBatch();});throws(function(){l.pageContent();});
    eq(l.relayoutPageArea(0),undefined);
  )JS";
  const auto body = Probe(program) + "<subform name=\"group\" layout=\"position\" w=\"180pt\" h=\"100pt\">" +
      Block("red", {255,0,0}, 72, 36, "x=\"24pt\" y=\"36pt\"") +
      Field("value", "Value", "x=\"110pt\" y=\"36pt\"", "<bind match=\"none\"/>", 60) + "</subform>";
  Artifacts artifacts; std::array<std::vector<Raster>,2> frames; size_t backend = 0;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    FlowSession loaded(library->api(), Fixture(body)); auto& s = loaded.session;
    ASSERT_TRUE(s.valid()); ASSERT_TRUE(s.xfa_loaded); RunProbe(s);
    frames[backend++] = Pages(s); ASSERT_EQ(frames[backend-1].size(),1u);
    Pixel(frames[backend-1][0],30,82,{255,0,0});Pixel(frames[backend-1][0],10,82,{255,255,255});
    FourWay(*library,s,artifacts,"layout-queries",[](Session& r){RunProbe(r);});
  }
  ComparePages(frames[0],frames[1],artifacts,"layout-query-backends");
}

TEST(CoverageXfaFlow, LeftAndRightFlowWrapAlignmentAndPresenceKeepIndependentRowGeometry) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct RowCase {const char* flow;const char* align;std::array<int,3> x;};
  const std::array<RowCase,6> cases = {{{"lr-tb","left",{0,60,0}}, {"lr-tb","center",{15,75,45}},
      {"lr-tb","right",{30,90,90}}, {"rl-tb","left",{60,0,0}},
      {"rl-tb","center",{75,15,45}}, {"rl-tb","right",{90,30,90}}}};
  Artifacts artifacts;
  for (const auto& c : cases) for (const std::string presence : {"visible","invisible","hidden"}) {
    SCOPED_TRACE(c.flow);
    SCOPED_TRACE(c.align);
    SCOPED_TRACE(presence);
    const bool hidden = presence=="hidden";
    const int first_x = hidden ? (std::string(c.flow)=="rl-tb" ? 60 : 0) : c.x[0];
    const int last_x = hidden ? (std::string(c.flow)=="rl-tb" ? 0 : 60) : c.x[2];
    // Hidden children consume no row space; alignment still applies to the
    // two visible 60pt children as one 120pt row.
    const int hidden_shift = std::string(c.align)=="center" ? 15 : std::string(c.align)=="right" ? 30 : 0;
    const int red_x = hidden ? first_x+hidden_shift : first_x;
    const int blue_x = hidden ? last_x+hidden_shift : last_x;
    const int blue_y = hidden ? 0 : 20;
    const std::string program = "var g=xfa.form.root.rows,l=xfa.layout;near(l.x(g.red)," + std::to_string(red_x) +
        ");near(l.y(g.red),0);near(l.x(g.blue)," + std::to_string(blue_x) + ");near(l.y(g.blue)," + std::to_string(blue_y) +
        ");near(l.h(g)," + std::to_string(hidden?20:40) + ");";
    const auto cell = [&](const char* name,RGB color,const std::string& attributes) {
      return "<subform name=\"" + std::string(name) + "\" layout=\"position\" w=\"60pt\" h=\"20pt\" hAlign=\""+
          c.align+"\" "+attributes+">"+Block("paint",color,60,20)+"</subform>";
    };
    const auto body = Probe(program)+"<subform name=\"rows\" layout=\""+c.flow+"\" w=\"150pt\">"+
        cell("red",{255,0,0},"")+cell("green",{0,128,0},"presence=\""+presence+"\"")+
        cell("blue",{0,0,255},"")+"</subform>";
    std::array<std::vector<Raster>,2> frames; size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name()); FlowSession loaded(library->api(),Fixture(body));auto& s=loaded.session;
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);RunProbe(s);frames[backend++]=Pages(s);
      ASSERT_EQ(frames[backend-1].size(),1u);const auto& image=frames[backend-1][0];
      Pixel(image,red_x+5,45,{255,0,0});Pixel(image,blue_x+5,45+blue_y,{0,0,255});
      if(!hidden)Pixel(image,c.x[1]+5,45,presence=="visible"?RGB{0,128,0}:RGB{255,255,255});
      Pixel(image,170,50,{255,255,255});
      if(presence=="visible" && std::string(c.align)=="center")
        FourWay(*library,s,artifacts,std::string(c.flow)+"-row-flow",[](Session& r){RunProbe(r);});
    }
    ComparePages(frames[0],frames[1],artifacts,std::string(c.flow)+c.align+presence);
  }
}

TEST(CoverageXfaFlow, KeepNextChainMovesTogetherToTheTallerContentAreaAndReportsFragments) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::string script = R"JS(
    var l=xfa.layout,r=xfa.form.root;
    near(l.y(r.prefix),40);near(l.y(r.leader),0);near(l.y(r.follower),30);
    near(l.h(r,'pt',0),60);near(l.h(r,'pt',1),60);eq(l.h(r,'pt',2),0);
    eq(l.pageSpan(r),1);eq(l.absPageSpan(r),1);eq(l.page(r.follower),1);
    eq(l.pageContent(0,'contentArea').length,2);
  )JS";
  const auto kept = [](const char* name,RGB color,const std::string& keep) {
    return "<subform name=\""+std::string(name)+"\" layout=\"position\" w=\"60pt\" h=\"30pt\">"+
        keep+Block("paint",color,60,30)+"</subform>";
  };
  const auto body=Probe(script)+Block("prefix",{0,0,255},60,20)+
      kept("leader",{0,128,0},"<keep next=\"contentArea\"/>")+kept("follower",{255,0,0},"");
  const auto areas="<contentArea name=\"top\" x=\"10pt\" y=\"10pt\" w=\"200pt\" h=\"70pt\"/>"
      "<contentArea name=\"bottom\" x=\"10pt\" y=\"110pt\" w=\"200pt\" h=\"100pt\"/>";
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());FlowSession loaded(library->api(),Fixture(body,"<root/>",DefaultPages(areas)));auto& s=loaded.session;
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);
    // Content starts at(10,10), so the probe and report are offset too.
    s.Click(18,20);ReopenPages(s);ASSERT_TRUE(s.Focus(18,40));EXPECT_EQ(s.Text(),u"PASS");s.api.FORM_ForceToKillFocus(s.form);
    frames[backend++]=Pages(s);ASSERT_EQ(frames[backend-1].size(),1u);
    Pixel(frames[backend-1][0],20,60,{0,0,255});Pixel(frames[backend-1][0],20,90,{255,255,255});
    Pixel(frames[backend-1][0],20,120,{0,128,0});Pixel(frames[backend-1][0],20,150,{255,0,0});
    FourWay(*library,s,artifacts,"keep-content-areas",[](Session& r){
      r.Click(18,20);ReopenPages(r);ASSERT_TRUE(r.Focus(18,40));EXPECT_EQ(r.Text(),u"PASS");r.api.FORM_ForceToKillFocus(r.form);
    });
  }
  ComparePages(frames[0],frames[1],artifacts,"keep-content-areas-backends");
}

TEST(CoverageXfaFlow, BookendsAndOverflowLeadersTrailersAreRealGeneratedContentInOrder) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const auto generated = [](const char* name,RGB color) {
    return "<subform name=\""+std::string(name)+"\" id=\""+name+"\" layout=\"position\" w=\"60pt\" h=\"10pt\">"
        "<occur min=\"1\" max=\"-1\"/>"+Block("paint",color,60,10)+"</subform>";
  };
  const auto prototypes="<proto>"+generated("head",{255,0,0})+generated("tail",{0,0,255})+"</proto>";
  Artifacts artifacts;
  for(bool overflow:{false,true}) {
    SCOPED_TRACE(overflow);
    std::string body=prototypes;
    if(overflow) {
      body+="<subform name=\"stream\" layout=\"tb\" w=\"60pt\"><overflow leader=\"#head\" trailer=\"#tail\"/>";
      for(int i=0;i<4;++i)body+="<subform name=\"row"+std::to_string(i)+"\" layout=\"position\" w=\"60pt\" h=\"60pt\"><keep intact=\"none\"/>"+
          Block("top",{0,128,0},60,30)+Block("bottom",{0,128,0},60,30,"y=\"30pt\"")+"</subform>";
      body+="</subform>";
    } else {
      for(int i=0;i<2;++i)body+="<subform name=\"section"+std::to_string(i)+"\" layout=\"tb\" w=\"60pt\">"
          "<bookend leader=\"#head\" trailer=\"#tail\"/>"+Block("paint",{0,128,0},60,20)+"</subform>";
    }
    const auto pages=DefaultPages("<contentArea w=\"200pt\" h=\""+std::string(overflow?"100":"240")+"pt\"/>");
    const auto inspect=[&](Session& s){
      const auto images=Pages(s);
      if(overflow) {
        // Overflow decorations apply to splittable containers. The earlier
        // atomic rows only moved intact, so their unused decorations were
        // correctly discarded by both engines. Real 30pt split boundaries
        // now require headers/trailers while preserving all240pt of green.
        ASSERT_GE(images.size(),3u);ASSERT_LE(images.size(),8u);
        int green_rows=0;
        for(size_t page=0;page<images.size();++page) {
          std::vector<std::pair<RGB,int>> bands;
          for(int y=0;y<240;++y) {
            const auto at=(static_cast<size_t>(y)*240+5)*4;
            const RGB color{images[page].pixels[at+2],images[page].pixels[at+1],images[page].pixels[at]};
            if(color==RGB{255,255,255})continue;
            if(!bands.empty()&&bands.back().first==color)++bands.back().second;
            else bands.push_back({color,1});
          }
          size_t index=0;
          if(page){ASSERT_LT(index,bands.size());EXPECT_EQ(bands[index].first,(RGB{255,0,0}));EXPECT_EQ(bands[index++].second,10);}
          ASSERT_LT(index,bands.size());EXPECT_EQ(bands[index].first,(RGB{0,128,0}));green_rows+=bands[index++].second;
          if(page+1<images.size()){ASSERT_LT(index,bands.size());EXPECT_EQ(bands[index].first,(RGB{0,0,255}));EXPECT_EQ(bands[index++].second,10);}
          EXPECT_EQ(index,bands.size());
          Pixel(images[page],80,40,{255,255,255});
        }
        EXPECT_EQ(green_rows,240);
      } else {
        ASSERT_EQ(images.size(),1u);
        for(int top:{0,40}) {
          Pixel(images[0],5,top+5,{255,0,0});Pixel(images[0],5,top+15,{0,128,0});Pixel(images[0],5,top+35,{0,0,255});
        }
      }
    };
    std::array<std::vector<Raster>,2> frames;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());FlowSession loaded(library->api(),Fixture(body,"<root/>",pages));auto& s=loaded.session;
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);inspect(s);frames[backend++]=Pages(s);
      FourWay(*library,s,artifacts,overflow?"overflow-generated":"bookend-generated",inspect,false);
    }
    ComparePages(frames[0],frames[1],artifacts,overflow?"overflow-generated-backends":"bookend-generated-backends",false);
  }
}

TEST(CoverageXfaFlow, SimplexOnlyLastAndDuplexParitySelectVisibleMasterPages) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  Artifacts artifacts;
  for(bool duplex:{false,true})for(int count:{1,2,3}) {
    SCOPED_TRACE(duplex);
    SCOPED_TRACE(count);
    const std::vector<std::string> names=duplex?
        (count==1?std::vector<std::string>{"first"}:count==2?std::vector<std::string>{"first","even"}:std::vector<std::string>{"first","even","odd"}):
        (count==1?std::vector<std::string>{"only"}:count==2?std::vector<std::string>{"first","last"}:std::vector<std::string>{"first","rest","last"});
    std::string checks="eq(xfa.layout.absPageCount(),"+std::to_string(count)+");eq(xfa.layout.sheetCount(),"+std::to_string(count)+");";
    for(int i=0;i<count;++i) {
      checks+="eq(xfa.layout.pageContent("+std::to_string(i)+",'pageArea').item(0).name,'"+names[i]+"');";
      checks+="var paints=xfa.layout.pageContent("+std::to_string(i)+",'draw',true);eq(paints.length,1);eq(paints.item(0).name,'"+names[i]+"Paint');near(xfa.layout.x(paints.item(0)),180);near(xfa.layout.y(paints.item(0)),80);near(xfa.layout.w(paints.item(0)),40);near(xfa.layout.h(paints.item(0)),10);";
    }
    const auto master=[&](const std::string& name,const std::string& position,const std::string& parity,RGB color) {
      return Area(name,"<contentArea w=\"160pt\" h=\"100pt\"/>"+
          Block(name+"Paint",color,40,10,"x=\"180pt\" y=\"80pt\""),
          "pagePosition=\""+position+"\""+(parity.empty()?"":" oddOrEven=\""+parity+"\""));
    };
    std::string pages="<pageSet relation=\""+std::string(duplex?"duplexPaginated":"simplexPaginated")+"\">";
    if(duplex)pages+=master("first","first","odd",{255,0,0})+master("even","rest","even",{0,128,0})+master("odd","rest","odd",{0,0,255});
    else pages+=master("first","first","",{255,0,0})+master("rest","rest","",{0,128,0})+
        master("only","only","",{255,255,0})+master("last","last","",{0,0,255});
    pages+="</pageSet>";
    std::string body;
    for(int i=0;i<count;++i)body+="<subform name=\"record"+std::to_string(i)+"\" layout=\"position\" w=\"60pt\" h=\"70pt\">"+
        Block("paint",{128,0,128},60,70)+
        (i==0?Probe(checks,true,"xfa.form.root.record0.report"):"")+"</subform>";
    const auto inspect=[&](Session& s,bool native){
      ASSERT_EQ(s.api.FPDF_GetPageCount(s.document),count);
      const auto verify=[&](const std::vector<Raster>& images,bool after){
        for(int i=0;i<count;++i){
          SCOPED_TRACE(i);
          const RGB master_color=names[i]=="only"?RGB{255,255,0}:names[i]=="first"?RGB{255,0,0}:
              (names[i]=="rest"||names[i]=="even")?RGB{0,128,0}:RGB{0,0,255};
          // Native parity selection uses the old page-cache length on
          // relayout. Odd caches lose master paints; even caches select odd
          // masters for every reused page. Rust must retain correct masters.
          const RGB expected=after&&native&&duplex?
              (count%2?RGB{255,255,255}:i==0?RGB{255,0,0}:RGB{0,0,255}):master_color;
          for(int y=80;y<90;++y)for(int x=180;x<220;++x)Pixel(images[i],x,y,expected);
          for(const auto& [x,y]:std::array<std::pair<int,int>,4>{{{179,85},{220,85},{190,79},{190,90}}})Pixel(images[i],x,y,{255,255,255});
          Pixel(images[i],5,5,{128,0,128});Pixel(images[i],80,50,{255,255,255});
        }
      };
      {SCOPED_TRACE("before probe");verify(Pages(s),false);}
      s.api.FORM_ForceToKillFocus(s.form);
      s.Click(205,10);
      {SCOPED_TRACE("after click before reopen");verify(Pages(s),true);}
      ASSERT_TRUE(s.Focus(185,35));EXPECT_EQ(s.Text(),u"PASS");
      s.api.FORM_ForceToKillFocus(s.form);
      {SCOPED_TRACE("after focus before reopen");verify(Pages(s),true);}
      ReopenPages(s);
      {SCOPED_TRACE("after probe/reopen");verify(Pages(s),true);}
    };
    const auto compare=[&](const std::vector<Raster>& a,const std::vector<Raster>& b,
                            const std::string& label,bool native){
      if(!duplex||!native){ComparePages(a,b,artifacts,label);return;}
      // Exclude only the independently checked400px native parity defect.
      // Preserve original pixels in diagnostics and compare every other pixel.
      ASSERT_EQ(a.size(),b.size());
      for(size_t i=0;i<a.size();++i){
        for(const auto& rect:std::array<std::array<int,4>,4>{{{0,0,240,80},{0,90,240,150},{0,80,180,10},{220,80,20,10}}})
          CompareRaster(CropRaster(a[i],rect[0],rect[1],rect[2],rect[3]),
                        CropRaster(b[i],rect[0],rect[1],rect[2],rect[3]),artifacts,
                        label+"-"+std::to_string(i)+"-"+std::to_string(rect[0])+"-"+std::to_string(rect[1]),
                        RasterTolerance{18,0,.15});
      }
    };
    std::array<std::vector<Raster>,2> frames,initial;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());FlowSession loaded(library->api(),Fixture(body,"<root/>",pages));auto& s=loaded.session;
      ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);initial[backend]=Pages(s);
      inspect(s,library->name()=="native");frames[backend++]=Pages(s);
      const auto label="master-pages-"+std::to_string(duplex)+std::to_string(count);
      const auto saved=SaveDocument(s.api,s.document,artifacts,library->name()+"-"+label);
      ASSERT_FALSE(saved.empty());
      std::array<std::vector<Raster>,2> reloaded;size_t reader_index=0;
      for(const auto* reader:Implementations()){
        FlowSession reopened(reader->api(),saved);auto& r=reopened.session;
        ASSERT_TRUE(r.valid());ASSERT_TRUE(r.xfa_loaded);inspect(r,reader->name()=="native");
        reloaded[reader_index++]=Pages(r);
        // Native serializes caption text as <text>\nProbe</text>, which
        // changes its appearance on reload. Both readers must agree on those
        // bytes; Rust writers must also preserve the original appearance.
        if(library->name()=="rust")
          compare(frames[backend-1],reloaded[reader_index-1],library->name()+"-"+label+"-original-"+reader->name(),reader->name()=="native");
      }
      compare(reloaded[0],reloaded[1],library->name()+"-"+label+"-readers",true);
    }
    ComparePages(initial[0],initial[1],artifacts,"master-pages-initial-backends-"+std::to_string(duplex)+std::to_string(count));
    compare(frames[0],frames[1],"master-pages-backends-"+std::to_string(duplex)+std::to_string(count),true);
  }
}

TEST(CoverageXfaFlow, InstanceInsertMoveCountLimitsAndBoundValuesRelayoutAndSave) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::string script=R"JS(
    var r=xfa.form.root,m=r._row;
    function row(i){return r.resolveNode('row['+i+']');}
    function text(n){return String(n.rawValue).replace(/^\n/,'');}
    eq(m.min,1);eq(m.max,4);throws(function(){m.min=0;});throws(function(){m.max=9;});
    throws(function(){m.setInstances();});throws(function(){m.setInstances(0);});throws(function(){m.setInstances(5);});
    throws(function(){m.insertInstance(-1);});throws(function(){m.removeInstance(99);});
    throws(function(){m.moveInstance(0);});throws(function(){m.addInstance(1,2);});
    if(text(r.phase)!='done'){
      eq(m.count,2);eq(text(row(0).value),'A');eq(text(row(1).value),'B');
      m.setInstances(3);row(2).value.rawValue='C';
      m.insertInstance(1,true).value.rawValue='X';eq(m.count,4);
      throws(function(){m.addInstance();});
      m.moveInstance(3,0);eq(text(row(0).value),'C');eq(text(row(2).value),'X');
      m.removeInstance(2);eq(m.count,3);m.count=2;eq(m.count,2);
      m.addInstance(true).value.rawValue='Z';r.phase.rawValue='done';
      xfa.layout.relayout();
    }
    eq(m.count,3);eq(text(row(0).value),'C');eq(text(row(1).value),'A');eq(text(row(2).value),'Z');
    eq(m.moveInstance(-1,0),undefined);eq(m.moveInstance(0,0),undefined);
    eq(row(0).instanceIndex,0);eq(row(1).instanceIndex,1);eq(row(2).instanceIndex,2);
  )JS";
  const auto body=Probe(script)+Field("phase","before","","<bind match=\"once\"/>")+
      "<subform name=\"row\" layout=\"position\" w=\"200pt\" h=\"20pt\"><occur min=\"1\" max=\"4\" initial=\"2\"/>"+
      Field("value","Seed","","<bind match=\"once\"/>")+"</subform>";
  const auto data="<root><phase>before</phase><row><value>A</value></row><row><value>B</value></row></root>";
  Artifacts artifacts;std::array<std::vector<Raster>,2> frames;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());FlowSession loaded(library->api(),Fixture(body,data));auto& s=loaded.session;
    ASSERT_TRUE(s.valid());ASSERT_TRUE(s.xfa_loaded);RunProbe(s);
    for(const auto& [y,value]:std::array<std::pair<int,const char16_t*>,3>{{{70,u"C"},{90,u"A"},{110,u"Z"}}}){
      ASSERT_TRUE(s.Focus(8,y));EXPECT_EQ(s.Text(),value);s.api.FORM_ForceToKillFocus(s.form);
    }
    frames[backend++]=Pages(s);ASSERT_EQ(frames[backend-1].size(),1u);
    forms::ExpectInk(frames[backend-1][0],0,60,100,80);
    forms::ExpectInk(frames[backend-1][0],0,100,100,120);
    if(library->name()=="rust") forms::ExpectInk(frames[backend-1][0],0,80,100,100);
    FourWay(*library,s,artifacts,"instance-manager-edits",[](Session& r){RunProbe(r);});
  }
  // Native OnLayoutItemAdded compares GetWidgetRect() and RecacheWidgetRect()
  // by reference to the same member, suppressing PerformLayout/SetFWLRect.
  // Native keeps a stale live rectangle for the moved row; its model and both
  // save/reload readers have the correct C,A,Z order. Compare unaffected live
  // regions, and require Rust's full image to match an independently laid out
  // final document rather than accepting that stale geometry as the oracle.
  for(const auto& [y,height]:std::array<std::pair<int,int>,2>{{{0,60},{100,140}}})
    CompareRaster(CropRaster(frames[0][0],0,y,240,height),CropRaster(frames[1][0],0,y,240,height),artifacts,
                  "instance-manager-unmoved-"+std::to_string(y),RasterTolerance{18,0,.15});
  std::array<std::vector<Raster>,2> canonical;backend=0;
  for(const auto* library:Implementations()) {
    FlowSession final(library->api(),Fixture(body,
        "<root><phase>done</phase><row><value>C</value></row><row><value>A</value></row><row><value>Z</value></row></root>"));
    ASSERT_TRUE(final.session.valid());ASSERT_TRUE(final.session.xfa_loaded);RunProbe(final.session);
    canonical[backend++]=Pages(final.session);
    forms::ExpectInk(canonical[backend-1][0],0,80,100,100);
  }
  ComparePages(canonical[0],canonical[1],artifacts,"instance-manager-final-document");
  ComparePages(canonical[1],frames[1],artifacts,"instance-manager-rust-live-final-geometry");
}

}  // namespace
}  // namespace pdfoxium::integration
