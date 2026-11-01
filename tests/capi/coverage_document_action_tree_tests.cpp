#include "pinned_test_fonts.h"
#include "form_fixture.h"
#include "document_fixture.h"

#include <algorithm>
#include <array>
#include <cctype>
#include <functional>
#include <map>
#include <regex>
#include <string>
#include <vector>

namespace pdfoxium::integration {
namespace {
using forms::Session;
using Values=std::array<std::u16string,3>;
using Fields=std::map<std::string,std::string>;

struct FormSpec {
  std::string action,catalog;
  std::array<std::string,3> values{{"Alpha","Beta",""}};
  std::array<int,3> annot_flags{{4,4,4}},field_flags{{0,0,0}};
  bool colored=false;
};
std::vector<uint8_t> ActionFixture(const FormSpec& spec) {
  // Public APIs cannot create field/action/name-tree dictionaries. All action
  // dispatch, editing, callbacks, appearances and saves use the public C API.
  std::vector<std::string> objects(16);
  objects[0]="<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R "+spec.catalog+" >>";
  objects[1]="<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>";
  objects[2]="<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << /Font << /Helv 5 0 R >> >> /Contents 12 0 R /Annots [7 0 R 8 0 R 9 0 R 10 0 R 11 0 R] >>";
  // NeedAppearances explicitly requests replacement of existing widget APs.
  // Preserve the authored RGB appearances used to prove hide/unhide visually.
  objects[3]="<< /Fields [7 0 R 8 0 R 9 0 R 10 0 R 11 0 R] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> /NeedAppearances "+
    std::string(spec.colored?"false":"true")+" >>";
  objects[4]="<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>";
  objects[5]="<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << >> /Contents 13 0 R >>";
  const std::array<std::string,3> rectangles{{"20 340 180 370","220 340 380 370","20 290 180 320"}};
  for(int index=0;index<3;++index){const std::string name(1,char('A'+index));
    objects[6+index]="<< /Type /Annot /Subtype /Widget /P 3 0 R /FT /Tx /T ("+name+") /V ("+spec.values[index]+") /DV (D"+name+") /Rect ["+
      rectangles[index]+"] /F "+std::to_string(spec.annot_flags[index])+" /Ff "+std::to_string(spec.field_flags[index])+
      " /MK << /BC [0 0 0] /BG [1 1 1] >> /BS << /W 1 >>"+
      (spec.colored?" /AP << /N "+std::to_string(14+index)+" 0 R >>":"")+" >>";
  }
  objects[9]="<< /Type /Annot /Subtype /Widget /P 3 0 R /FT /Tx /T (report) /V (Ready) /Ff 5 /F 4 /Rect [220 290 380 320] /MK << /BG [1 1 1] >> >>";
  objects[10]="<< /Type /Annot /Subtype /Widget /P 3 0 R /FT /Btn /T (trigger) /Ff 65536 /F 4 /Rect [20 240 180 270] /MK << /CA (Trigger) /BG [1 1 1] >> /A "+spec.action+" >>";
  objects[11]=PdfStream("","1 g 0 0 400 400 re f 1 0 0 rg 0 0 400 10 re f");
  objects[12]=PdfStream("","1 g 0 0 400 400 re f 0 0 1 rg 0 0 400 10 re f");
  const std::array<const char*,3> colors{{"1 0 0","0 0 1","0 1 0"}};
  for(int index=0;index<3;++index)objects[13+index]=PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 160 30]",
    std::string(colors[index])+" rg 0 0 160 30 re f");
  return PdfFixture(objects);
}
void Settle(Session& s) {
  s.api.FORM_ForceToKillFocus(s.form);s.api.FORM_OnMouseMove(s.form,s.page,0,390,390);
}
void Trigger(Session& s){s.Click(50,255);Settle(s);}
void ValueCheck(Session& s,const Values& expected) {
  for(int index=0;index<3;++index)EXPECT_EQ(s.Field(index),expected[index])<<index;
}
void SetField(Session& s,int index,const std::u16string& value) {
  ASSERT_TRUE(s.Focus(index==1?240:40,index==2?305:355));
  if(!s.Text().empty()){
    ASSERT_TRUE(s.api.FORM_SelectAllText(s.form,s.page));
  }
  s.Replace(value);EXPECT_EQ(s.Text(),value);Settle(s);
}
void Pixel(const Raster& image,int x,int y,std::array<int,3> color) {
  const auto at=(static_cast<size_t>(y)*image.width+x)*4;
  EXPECT_EQ(image.pixels[at],color[2]);EXPECT_EQ(image.pixels[at+1],color[1]);EXPECT_EQ(image.pixels[at+2],color[0]);
}
std::array<Raster,2> FormPages(Session& s) {
  return {s.Draw(),s.Draw(400,400,0,0,0,0,0,0,1)};
}
void CompareFormPages(const std::array<Raster,2>& a,const std::array<Raster,2>& b,Artifacts& artifacts,const std::string& label) {
  for(int page=0;page<2;++page){CompareRaster(a[page],b[page],artifacts,label+std::to_string(page),RasterTolerance{24,0,.3});
    if(page==0)for(const auto& rect:std::array<std::array<int,4>,4>{{{20,30,160,30},{220,30,160,30},{20,80,160,30},{220,80,160,30}}})
      CompareRaster(CropRaster(a[0],rect[0],rect[1],rect[2],rect[3]),CropRaster(b[0],rect[0],rect[1],rect[2],rect[3]),artifacts,
        label+"-field-"+std::to_string(rect[0])+"-"+std::to_string(rect[1]),RasterTolerance{24,0,.5});
  }
}
using FormStep=std::function<void(Session&)>;
void FormWorkflow(const FormSpec& spec,const std::string& label,const FormStep& act,const FormStep& inspect) {
  Artifacts artifacts;const auto bytes=ActionFixture(spec);artifacts.Pdf(label+"-input",bytes);
  std::array<std::array<Raster,2>,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()){
    SCOPED_TRACE(writer->name());Session s(writer->api(),bytes,true);ASSERT_TRUE(s.valid());
    ASSERT_NO_FATAL_FAILURE(act(s));
    ASSERT_NO_FATAL_FAILURE(inspect(s));
    Settle(s);
    const auto images=FormPages(s);originals[backend++]=images;
    Pixel(images[0],10,395,{255,0,0});Pixel(images[1],10,395,{0,0,255});
    const auto saved=SaveDocument(s.api,s.document,artifacts,writer->name()+"-"+label);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()){
      SCOPED_TRACE(reader->name());Session reopened(reader->api(),saved,true);ASSERT_TRUE(reopened.valid());
      ASSERT_NO_FATAL_FAILURE(inspect(reopened));Settle(reopened);
      CompareFormPages(images,FormPages(reopened),artifacts,writer->name()+"-"+reader->name()+"-"+label);
      EXPECT_TRUE(reopened.host->callbacks_valid);EXPECT_FALSE(reopened.host->callback_error);
    }
  }
  CompareFormPages(originals[0],originals[1],artifacts,label+"-backends");
}
Fields Packet(const std::vector<uint8_t>& bytes,bool fdf=true) {
  const std::string data(bytes.begin(),bytes.end());Fields fields;
  if(fdf){
    EXPECT_EQ(data.substr(0,5),"%FDF-");
    // Source fields are simple ASCII strings. Match leaf dictionaries without
    // depending on object numbers, whitespace or T/V dictionary order.
    const std::regex dictionary("<<([^<>]*)>>"),name("/T\\s*\\(([^()]*)\\)"),value("/V\\s*\\(([^()]*)\\)");
    for(auto it=std::sregex_iterator(data.begin(),data.end(),dictionary);it!=std::sregex_iterator();++it){
      std::smatch key,val;const auto leaf=(*it)[1].str();
      if(std::regex_search(leaf,key,name)&&std::regex_search(leaf,val,value))fields[key[1].str()]=val[1].str();
    }
  }else{
    size_t first=0;while(first<data.size()){
      const auto last=data.find('&',first),end=last==std::string::npos?data.size():last;
      const auto equals=data.find('=',first);EXPECT_LT(equals,end);if(equals>=end)break;
      fields[data.substr(first,equals-first)]=data.substr(equals+1,end-equals-1);first=end+1;
    }
  }
  return fields;
}
void Submission(Session& s,const Fields& expected,bool fdf=true) {
  ASSERT_EQ(s.host->submissions.size(),1u);ASSERT_EQ(s.host->submit_urls.size(),1u);
  EXPECT_EQ(s.host->submit_urls[0],u"https://example.invalid/record-only");EXPECT_EQ(Packet(s.host->submissions[0],fdf),expected);
  EXPECT_TRUE(s.host->mails.empty());EXPECT_TRUE(s.host->uris.empty());
}
// Declare a URL file specification so platform file-path decoding does not
// interpret slashes as legacy Mac separators or Windows backslashes.
const std::string destination="/F << /FS /URL /F (https://example.invalid/record-only) >>";

TEST(CoverageDocumentActionTree, HideTargetsClearInvisibleAndNoViewAndPersistTheirActualWidgetPaint) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(int target:{0,1,2,3})for(int mode:{0,1,2}){
    SCOPED_TRACE(target);
    SCOPED_TRACE(mode);
    const bool hide=mode!=2;const std::array<std::string,4> targets{{"(A)","[(A) (B)]","[(A) 8 0 R 17]","(absent)"}};
    const std::array<bool,3> affected{{target!=3,target==1,false}};
    FormSpec spec;spec.colored=true;spec.action="<< /S /Hide /T "+targets[target]+(mode==0?"":mode==1?" /H true":" /H false")+" >>";
    if(!hide)for(int i=0;i<3;++i)if(affected[i])spec.annot_flags[i]=4|1|2|32;
    const auto inspect=[=](Session& s){ValueCheck(s,{u"Alpha",u"Beta",u""});
      for(int i=0;i<3;++i){Handle annot(s.api.FPDFPage_GetAnnot(s.page,i),s.api.FPDFPage_CloseAnnot);ASSERT_NE(annot.get(),nullptr);
        EXPECT_EQ(s.api.FPDFAnnot_GetFlags(annot.get()),affected[i]&&hide?6:4);}
      const auto image=s.Draw();Pixel(image,100,45,affected[0]&&hide?std::array<int,3>{255,255,255}:std::array<int,3>{255,0,0});
      Pixel(image,300,45,affected[1]&&hide?std::array<int,3>{255,255,255}:std::array<int,3>{0,0,255});Pixel(image,100,95,{0,255,0});
    };
    FormWorkflow(spec,"hide-"+std::to_string(target)+std::to_string(mode),[](Session& s){Trigger(s);},inspect);
  }
}

TEST(CoverageDocumentActionTree, ResetFormInclusionExclusionEmptyListsAndMixedEntriesRestoreOnlySelectedDefaults) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Case{std::string fields;int flags;std::array<bool,3> reset;};
  const std::vector<Case> cases={{"",0,{true,true,true}},{"[(A)]",0,{true,false,false}},{"[(A)]",1,{false,true,true}},
    {"[(A) (A)]",0,{true,false,false}},{"[]",0,{false,false,false}},{"[]",1,{true,true,true}},
    {"[8 0 R 17 (B)]",0,{false,true,false}}};
  for(size_t row=0;row<cases.size();++row){SCOPED_TRACE(row);const auto c=cases[row];FormSpec spec;spec.values[2]="Gamma";
    spec.action="<< /S /ResetForm "+std::string(c.fields.empty()?"":"/Fields "+c.fields)+" /Flags "+std::to_string(c.flags)+" >>";
    const Values expected{{c.reset[0]?u"DA":u"Alpha",c.reset[1]?u"DB":u"Beta",c.reset[2]?u"DC":u"Gamma"}};
    FormWorkflow(spec,"reset-"+std::to_string(row),[](Session& s){Trigger(s);},[=](Session& s){ValueCheck(s,expected);});
  }
}

TEST(CoverageDocumentActionTree, SubmitActionFiltersUseOnlyStringFieldNamesAndEnforceRequiredFieldsBeforeCallbacks) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Case{std::string fields;int flags;Fields expected;};
  const std::vector<Case> cases={{"",0,{{"A","Alpha"},{"B","Beta"},{"C",""}}},
    {"[(A)]",0,{{"A","Alpha"}}},{"[(A)]",1,{{"B","Beta"},{"C",""}}},
    {"[(A) (A)]",0,{{"A","Alpha"}}},{"[]",0,{{"A","Alpha"},{"B","Beta"},{"C",""}}},
    {"[8 0 R 17 (A)]",0,{{"A","Alpha"}}}};
  for(size_t row=0;row<cases.size();++row){SCOPED_TRACE(row);const auto c=cases[row];FormSpec spec;
    spec.action="<< /S /SubmitForm "+destination+(c.fields.empty()?"":" /Fields "+c.fields)+" /Flags "+std::to_string(c.flags)+" >>";
    FormWorkflow(spec,"action-submit-"+std::to_string(row),[=](Session& s){Trigger(s);Submission(s,c.expected);},
      [](Session& s){ValueCheck(s,{u"Alpha",u"Beta",u""});});
  }
  for(bool excluded:{false,true}){SCOPED_TRACE(excluded);FormSpec spec;spec.field_flags[2]=2;
    spec.action="<< /S /SubmitForm "+destination+(excluded?" /Fields [(C)] /Flags 1":"")+" >>";
    FormWorkflow(spec,"required-submit-"+std::to_string(excluded),[=](Session& s){Trigger(s);
      if(excluded)Submission(s,{{"A","Alpha"},{"B","Beta"}});
      else {EXPECT_TRUE(s.host->submissions.empty());SetField(s,2,u"Filled");Trigger(s);Submission(s,{{"A","Alpha"},{"B","Beta"},{"C","Filled"}});}
    },[=](Session& s){ValueCheck(s,{u"Alpha",u"Beta",excluded?u"":u"Filled"});});
  }
}

TEST(CoverageDocumentActionTree, PositionalAndKeywordSubmitFormAgreeOnEmptyFilteringFormatsAndDuplicateSelections) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(bool keyword:{false,true})for(bool fdf:{false,true})for(bool empty:{false,true}){
    SCOPED_TRACE(keyword);
    SCOPED_TRACE(fdf);
    SCOPED_TRACE(empty);
    const auto call=keyword?"this.submitForm({cURL:'https://example.invalid/record-only',bFDF:"+std::string(fdf?"true":"false")+
      ",bEmpty:"+(empty?"true":"false")+",aFields:['A','C','A']});":
      "this.submitForm('https://example.invalid/record-only',"+std::string(fdf?"true":"false")+","+(empty?"true":"false")+",['A','C','A']);";
    FormSpec spec;spec.action=forms::Js(call);Fields expected{{"A","Alpha"}};if(empty)expected["C"]="";
    FormWorkflow(spec,"script-submit-"+std::to_string(keyword)+std::to_string(fdf)+std::to_string(empty),
      [=](Session& s){Trigger(s);Submission(s,expected,fdf);},[](Session& s){ValueCheck(s,{u"Alpha",u"Beta",u""});});
  }
  for(bool keyword:{false,true})for(bool empty:{false,true}){
    SCOPED_TRACE(keyword);
    SCOPED_TRACE(empty);
    FormSpec spec;spec.action=forms::Js(keyword?
      "this.submitForm({cURL:'https://example.invalid/record-only',bFDF:true,bEmpty:"+std::string(empty?"true":"false")+",aFields:[]});":
      "this.submitForm('https://example.invalid/record-only',true,"+std::string(empty?"true":"false")+",[]);");
    const Fields expected=empty?Fields{{"A","Alpha"},{"B","Beta"},{"C",""}}:Fields{};
    FormWorkflow(spec,"script-empty-subset-"+std::to_string(keyword)+std::to_string(empty),
      [=](Session& s){Trigger(s);Submission(s,expected);},[](Session& s){ValueCheck(s,{u"Alpha",u"Beta",u""});});
  }
  for(bool keyword:{false,true}){
    SCOPED_TRACE(keyword);FormSpec spec;spec.field_flags[2]=2;
    spec.action=forms::Js(keyword?
      "this.submitForm({cURL:'https://example.invalid/record-only',bFDF:true,bEmpty:true,aFields:['A','C']});":
      "this.submitForm('https://example.invalid/record-only',true,true,['A','C']);");
    FormWorkflow(spec,"script-required-recovery-"+std::to_string(keyword),[](Session& s){
      Trigger(s);EXPECT_TRUE(s.host->submissions.empty());SetField(s,2,u"Filled");Trigger(s);
      Submission(s,{{"A","Alpha"},{"C","Filled"}});
    },[](Session& s){ValueCheck(s,{u"Alpha",u"Beta",u"Filled"});});
  }
}

TEST(CoverageDocumentActionTree, NextDictionaryAndArrayActionsRunInOrderBeforeTheRecordedSubmission) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(bool array:{false,true}){SCOPED_TRACE(array);FormSpec spec;spec.values[2]="Gamma";
    const auto submit="<< /S /SubmitForm "+destination+" /Next << /S /Named /N /LastPage >> >>";
    spec.action=array?"<< /S /Named /N /FirstPage /Next [<< /S /Hide /T (A) >> << /S /ResetForm >> "+submit+"] >>":
      "<< /S /Named /N /FirstPage /Next << /S /ResetForm /Next "+submit+" >> >>";
    FormWorkflow(spec,"next-order-"+std::to_string(array),[=](Session& s){Trigger(s);
      EXPECT_EQ(s.host->named_actions,(std::vector<std::string>{"FirstPage","LastPage"}));
      Submission(s,{{"A","DA"},{"B","DB"},{"C","DC"}});
    },[=](Session& s){ValueCheck(s,{u"DA",u"DB",u"DC"});Handle annot(s.api.FPDFPage_GetAnnot(s.page,0),s.api.FPDFPage_CloseAnnot);
      EXPECT_EQ(s.api.FPDFAnnot_GetFlags(annot.get()),array?6:4);});
  }
}

TEST(CoverageDocumentActionTree, JavaScriptNamedDestinationsResolveModernArraysWrappedDictionariesAndLegacyFallbacks) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  struct Case{std::string key,dest,mode;std::vector<float> coordinates;bool legacy=false,fallback=false;};
  const std::vector<Case> cases={{"spot","/XYZ 12 34 1.5","XYZ",{12,34,1.5}},
    {"spot","/FitH 271","FitH",{271}},{"unicode","/FitR 10 20 210 320","FitR",{10,20,210,320}},
    {"spot","/Fit","Fit",{},true,false},{"spot","/FitV 99","FitV",{99},true,true},
    {"spot","/FitB","FitB",{}},{"spot","/FitBH 333","FitBH",{333}},{"spot","/FitBV 122","FitBV",{122}}};
  const std::map<std::string,int> modes{{"XYZ",PDFDEST_VIEW_XYZ},{"Fit",PDFDEST_VIEW_FIT},{"FitH",PDFDEST_VIEW_FITH},
    {"FitV",PDFDEST_VIEW_FITV},{"FitR",PDFDEST_VIEW_FITR},{"FitB",PDFDEST_VIEW_FITB},{"FitBH",PDFDEST_VIEW_FITBH},{"FitBV",PDFDEST_VIEW_FITBV}};
  for(size_t row=0;row<cases.size();++row){SCOPED_TRACE(row);const auto c=cases[row];FormSpec spec;
    const auto dest="[6 0 R "+c.dest+"]";const std::string key=c.key=="unicode"?document_test::PdfText(u"\u03a9"):"(spot)";
    const auto stored=row==1?"<< /D "+dest+" >>":dest;
    if(c.legacy)spec.catalog="/Dests << /spot "+stored+" >>"+(c.fallback?std::string(" /Names << /Dests << /Names [(other) [3 0 R /Fit]] >> >>"):"");
    else spec.catalog="/Names << /Dests << /Names ["+(c.key=="unicode"?std::string("(spot) ")+stored+" ":std::string())+key+" "+stored+"] >> >>";
    std::string program="var caught=false;try{this.gotoNamedDest('missing');}catch(e){caught=true;}";
    if(c.key=="unicode"){
      // Native converts the JS name to UTF-8, then decodes those bytes as
      // PDFDocEncoding at name-tree lookup. Observe the genuine Unicode call
      // first; only that pinned native failure uses the healthy ASCII alias.
      program+="var unicodeMissing=false;try{this.gotoNamedDest('\\u03a9');}catch(e){unicodeMissing=true;}"
        "app.alert(unicodeMissing?'UNICODE-MISSING':'UNICODE');if(unicodeMissing)this.gotoNamedDest('spot');";
    }else program+="this.gotoNamedDest('spot');";
    spec.action=forms::Js(program+"this.getField('report').value=caught?'PASS':'FAIL';");
    const auto navigate=[=](Session& s){Trigger(s);EXPECT_EQ(s.Field(3),u"PASS");ASSERT_EQ(s.host->destinations.size(),1u);
      if(c.key=="unicode"){
        ASSERT_EQ(s.host->alerts.size(),1u);
        EXPECT_EQ(std::get<0>(s.host->alerts[0]),s.api.FPDF_GetPageCount==Implementations()[0]->api().FPDF_GetPageCount?u"UNICODE-MISSING":u"UNICODE");
      }
      const auto& observed=s.host->destinations[0];EXPECT_EQ(std::get<0>(observed),1);EXPECT_EQ(std::get<1>(observed),modes.at(c.mode));
      EXPECT_EQ(std::get<2>(observed),c.coordinates);
    };
    FormWorkflow(spec,"named-script-"+std::to_string(row),[](Session&){},navigate);
  }
}

using DocInspect=std::function<void(const Api&,FPDF_DOCUMENT)>;
const std::array<std::array<int,3>,6> page_colors{{{255,0,0},{0,128,0},{0,0,255},{255,128,0},{0,128,128},{128,0,128}}};
void PlainWorkflow(const std::vector<uint8_t>& bytes,const std::string& label,int count,const DocInspect& inspect){
  Artifacts artifacts;const auto pictures=[&](const Api& api,FPDF_DOCUMENT document){std::vector<Raster> result;
    for(int i=0;i<count;++i){Handle page(api.FPDF_LoadPage(document,i),api.FPDF_ClosePage);EXPECT_NE(page.get(),nullptr);
      result.push_back(Render(api,page.get(),100,100));
      const auto color=page_colors[i%page_colors.size()];
      for(size_t at=0;at<result.back().pixels.size();at+=4){
        EXPECT_EQ(result.back().pixels[at],color[2]);EXPECT_EQ(result.back().pixels[at+1],color[1]);
        EXPECT_EQ(result.back().pixels[at+2],color[0]);EXPECT_EQ(result.back().pixels[at+3],255);
      }
    }return result;
  };
  const auto compare=[&](const std::vector<Raster>& a,const std::vector<Raster>& b,const std::string& name){ASSERT_EQ(a.size(),b.size());
    for(size_t i=0;i<a.size();++i)CompareRaster(a[i],b[i],artifacts,name+std::to_string(i));};
  std::array<std::vector<Raster>,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()){
    SCOPED_TRACE(writer->name());const auto& api=writer->api();Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    ASSERT_EQ(api.FPDF_GetPageCount(document.get()),count);ASSERT_NO_FATAL_FAILURE(inspect(api,document.get()));originals[backend]=pictures(api,document.get());
    const auto saved=SaveDocument(api,document.get(),artifacts,writer->name()+label);ASSERT_FALSE(saved.empty());
    for(const auto* reader:Implementations()){
      SCOPED_TRACE(reader->name());const auto& read=reader->api();Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);
      ASSERT_EQ(read.FPDF_GetPageCount(reopened.get()),count);ASSERT_NO_FATAL_FAILURE(inspect(read,reopened.get()));compare(originals[backend],pictures(read,reopened.get()),writer->name()+reader->name()+label);
    }++backend;
  }compare(originals[0],originals[1],"backends-"+label);
}
int Add(std::vector<std::string>& objects,const std::string& object){objects.push_back(object);return static_cast<int>(objects.size());}
std::string Ref(int number){return std::to_string(number)+" 0 R";}
std::vector<int> ColorPages(std::vector<std::string>& objects,int count,const std::vector<int>& parents={}){
  std::vector<int> pages;std::string kids;
  for(int i=0;i<count;++i){const auto color=page_colors[i%page_colors.size()];const auto content=Add(objects,PdfStream("",
      (parents.empty()||(parents[i]!=2&&parents[i]!=11&&parents[i]!=19&&parents[i]!=31)?std::string():std::string("/P << /MCID 0 >> BDC "))+
      std::to_string(color[0]/255.)+" "+std::to_string(color[1]/255.)+" "+std::to_string(color[2]/255.)+" rg 0 0 100 100 re f"+
      (parents.empty()||(parents[i]!=2&&parents[i]!=11&&parents[i]!=19&&parents[i]!=31)?"":" EMC")));
    const auto page=Add(objects,"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << >> /Contents "+Ref(content)+
      (parents.empty()?"":" /StructParents "+std::to_string(parents[i]))+" >>");pages.push_back(page);kids+=Ref(page)+" ";}
  objects[1]="<< /Type /Pages /Count "+std::to_string(count)+" /Kids ["+kids+"] >>";return pages;
}

TEST(CoverageDocumentActionTree, NestedPageLabelNumberTreesUseFloorKeysAcrossGapsAndEveryBoundary) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  for(bool nested:{false,true}){SCOPED_TRACE(nested);std::vector<std::string> objects(2);ColorPages(objects,13);
    const std::string low="2 << /P (v-) /S /R /St 5 >> 5 << /S /A /St 26 >>";
    const std::string high="8 << /P (p-) /S /D /St 7 >> 11 << /S /a /St 27 >>";
    int labels;
    if(nested){const int left=Add(objects,"<< /Limits [2 5] /Nums ["+std::string(low)+"] >>"),right=Add(objects,"<< /Limits [8 11] /Nums ["+high+"] >>");
      const int empty=Add(objects,"<< /Nums [] >>");const int middle=Add(objects,"<< /Limits [2 11] /Kids ["+Ref(left)+" "+Ref(empty)+" "+Ref(right)+"] >>");
      labels=Add(objects,"<< /Kids ["+Ref(middle)+"] >>");
    }else labels=Add(objects,"<< /Nums ["+std::string(low)+" "+high+"] >>");
    objects[0]="<< /Type /Catalog /Pages 2 0 R /PageLabels "+Ref(labels)+" >>";
    PlainWorkflow(PdfFixture(objects),"label-floor-"+std::to_string(nested),13,[](const Api& api,FPDF_DOCUMENT document){
      const std::array<std::u16string,13> expected{{u"1",u"2",u"v-V",u"v-VI",u"v-VII",u"Z",u"AA",u"BB",u"p-7",u"p-8",u"p-9",u"aa",u"bb"}};
      for(int i=0;i<13;++i)document_test::ExpectBytes([&](void* out,unsigned long size){return api.FPDF_GetPageLabel(document,i,out,size);},document_test::Utf16(expected[i]));
    });
  }
}

TEST(CoverageDocumentActionTree, NestedParentTreesUseExactKeysAndDoNotLeakNeighborPageStructureAcrossGaps) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);
  const std::vector<int> ids{0,2,7,11,13,19,31,40};const std::array<int,4> selected{{1,3,5,6}};
  for(bool nested:{false,true}){SCOPED_TRACE(nested);std::vector<std::string> objects(3);const auto pages=ColorPages(objects,8,ids);
    const int tree_root=3;std::array<int,4> elements{};
    for(size_t i=0;i<selected.size();++i)elements[i]=Add(objects,"<< /Type /StructElem /S /P /P 3 0 R /Pg "+Ref(pages[selected[i]])+" /K 0 /Alt (page"+std::to_string(selected[i])+") >>");
    const auto pairs=[&](int first,int last){std::string result;for(int i=first;i<=last;++i)result+=std::to_string(ids[selected[i]])+" ["+Ref(elements[i])+"] ";return result;};
    int parent;
    if(nested){const int low=Add(objects,"<< /Limits [2 11] /Nums ["+pairs(0,1)+"] >>"),high=Add(objects,"<< /Limits [19 31] /Nums ["+pairs(2,3)+"] >>");
      const int empty=Add(objects,"<< /Nums [] >>"),middle=Add(objects,"<< /Limits [2 31] /Kids ["+Ref(low)+" "+Ref(empty)+" "+Ref(high)+"] >>");
      parent=Add(objects,"<< /Kids ["+Ref(middle)+"] >>");
    }else parent=Add(objects,"<< /Nums ["+pairs(0,3)+"] >>");
    std::string children;for(int element:elements)children+=Ref(element)+" ";
    objects[tree_root-1]="<< /Type /StructTreeRoot /K ["+children+"] /ParentTree "+Ref(parent)+" >>";
    objects[0]="<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> /StructTreeRoot 3 0 R >>";
    PlainWorkflow(PdfFixture(objects),"parent-exact-"+std::to_string(nested),8,[=](const Api& api,FPDF_DOCUMENT document){
      for(int page_index=0;page_index<8;++page_index){SCOPED_TRACE(page_index);Handle page(api.FPDF_LoadPage(document,page_index),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
        Handle tree(api.FPDF_StructTree_GetForPage(page.get()),api.FPDF_StructTree_Close);ASSERT_NE(tree.get(),nullptr);ASSERT_EQ(api.FPDF_StructTree_CountChildren(tree.get()),4);
        for(size_t child=0;child<4;++child){const auto element=api.FPDF_StructTree_GetChildAtIndex(tree.get(),static_cast<int>(child));
          if(selected[child]!=page_index){EXPECT_EQ(element,nullptr);continue;}
          ASSERT_NE(element,nullptr);EXPECT_EQ(api.FPDF_StructElement_GetParent(element),nullptr);EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentID(element),0);
          EXPECT_EQ(api.FPDF_StructElement_CountChildren(element),1);EXPECT_EQ(api.FPDF_StructElement_GetChildMarkedContentID(element,0),0);
          document_test::ExpectBytes([&](void* out,unsigned long size){return api.FPDF_StructElement_GetType(element,out,size);},document_test::Utf16(u"P"));
          const auto label=std::string("page")+std::to_string(page_index);const std::u16string wide(label.begin(),label.end());
          document_test::ExpectBytes([&](void* out,unsigned long size){return api.FPDF_StructElement_GetAltText(element,out,size);},document_test::Utf16(wide));
        }
      }
    });
  }
}
}  // namespace
}  // namespace pdfoxium::integration
