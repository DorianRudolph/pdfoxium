#include "document_fixture.h"

#include <cmath>
#include <fstream>
#include <set>

namespace pdfoxium::integration {
namespace {
using namespace document_test;

std::vector<uint8_t> NavigationFixture() {
  // Outlines/actions/destinations/metadata have no public constructors.
  // Objects18/20/21 are links; object19 is an intervening non-link annotation.
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /Outlines 7 0 R /Names << /Dests << /Names [(last) [5 0 R /Fit]] >> >> /PageMode /UseOutlines >>",
    "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Resources << >> /Contents 4 0 R /Annots [18 0 R 19 0 R 20 0 R 21 0 R] /AA << /O 26 0 R /C 15 0 R >> >>",
    PdfStream("","0.2 0.4 0.6 rg 8 8 80 80 re f"),
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] /Resources << >> /Contents 6 0 R >>",
    PdfStream("","0.6 0.4 0.2 rg 8 8 100 64 re f"),
    "<< /Type /Outlines /First 8 0 R /Last 11 0 R /Count 6 >>",
    "<< /Title (First) /Parent 7 0 R /Next 9 0 R /First 12 0 R /Last 13 0 R /Count 2 /Dest [3 0 R /XYZ 0 null 1] /A 26 0 R /C [0 1 .25] /F 0 >>",
    "<< /Title "+PdfText(u"\u03a9\U0001f642")+" /Parent 7 0 R /Prev 8 0 R /Next 10 0 R /First 14 0 R /Last 14 0 R /Count -1 >>",
    "<< /Title () /Parent 7 0 R /Prev 9 0 R /Next 11 0 R /Dest (last) /F 2 >>",
    "<< /Title (Duplicate) /Parent 7 0 R /Prev 10 0 R /C [2 -1 0] /F 3 /A 16 0 R >>",
    "<< /Title (Duplicate) /Parent 8 0 R /Next 13 0 R /Dest [5 0 R /FitH 100] /F 1 >>",
    "<< /Title (Leaf) /Parent 8 0 R /Prev 12 0 R /C [1 0 1] /A 25 0 R >>",
    "<< /Title (Remote) /Parent 9 0 R /A 17 0 R >>",
    "<< /S /URI /URI <68747470733A2F2F6578616D706C652E746573742FFEC3A9> >>",
    "<< /S /Launch /F << /UF "+PdfText(u"\u0444\u0430\u0439\u043b.pdf")+" >> >>",
    "<< /S /GoToR /F (remote.pdf) /D [0 /Fit] >>",
    "<< /Type /Annot /Subtype /Link /Rect [10 10 50 50] /Dest [5 0 R /Fit] /QuadPoints [10 50 50 50 10 10 50 10 12 48 48 48 12 12 48 12] /Border [0 0 1] >>",
    "<< /Type /Annot /Subtype /Square /Rect [0 0 5 5] >>",
    "<< /Type /Annot /Subtype /Link /Rect [20 20 60 60] /A 15 0 R /Border [0 0 1] >>",
    "<< /Type /Annot /Subtype /Link /Rect [60.5 10.25 80.75 30.5] /Dest (last) /A 22 0 R /QuadPoints [60.5 30.5 80.75 30.5 60.5 10.25 80.75 10.25] >>",
    "<< /S /GoToE /D [0 /Fit] /T << /R /C /N (embedded.pdf) >> >>",
    "<< /Title "+PdfText(u"Metadata \u03a9")+" /Author (Author) /Subject () /Keywords (a,b) /Creator (Creator) /Producer (Producer) /CreationDate (D:20260203112233Z) /ModDate (D:20260204112233+01'00') >>",
    "null",
    "<< /S /JavaScript /JS (1+1) >>",
    "<< /S /GoTo /D [3 0 R /XYZ 0 null 1] >>"},
    "/Info 23 0 R /ID [<410042FF> <430044FE>]");
}

void CheckNavigation(const Api& api,FPDF_DOCUMENT document) {
  auto first=api.FPDFBookmark_GetFirstChild(document,nullptr);ASSERT_NE(first,nullptr);
  auto unicode=api.FPDFBookmark_GetNextSibling(document,first);ASSERT_NE(unicode,nullptr);
  auto empty=api.FPDFBookmark_GetNextSibling(document,unicode);ASSERT_NE(empty,nullptr);
  auto duplicate=api.FPDFBookmark_GetNextSibling(document,empty);ASSERT_NE(duplicate,nullptr);
  EXPECT_EQ(api.FPDFBookmark_GetNextSibling(document,duplicate),nullptr);
  auto child=api.FPDFBookmark_GetFirstChild(document,first);ASSERT_NE(child,nullptr);
  auto leaf=api.FPDFBookmark_GetNextSibling(document,child);ASSERT_NE(leaf,nullptr);
  EXPECT_EQ(api.FPDFBookmark_GetNextSibling(document,leaf),nullptr);
  EXPECT_EQ(api.FPDFBookmark_GetFirstChild(document,leaf),nullptr);
  for(const auto& entry:std::vector<std::pair<FPDF_BOOKMARK,std::u16string>>{{first,u"First"},{unicode,u"\u03a9\U0001f642"},{empty,u""},{child,u"Duplicate"}}) {
    ExpectBytes([&](void* b,unsigned long n){return api.FPDFBookmark_GetTitle(entry.first,b,n);},Utf16(entry.second));
  }
  EXPECT_EQ(api.FPDFBookmark_GetCount(first),2);EXPECT_EQ(api.FPDFBookmark_GetCount(unicode),-1);EXPECT_EQ(api.FPDFBookmark_GetCount(leaf),0);EXPECT_EQ(api.FPDFBookmark_GetCount(nullptr),0);
  for(const auto& entry:std::vector<std::pair<std::u16string,FPDF_BOOKMARK>>{{u"First",first},{u"\u03a9\U0001f642",unicode},{u"Duplicate",child},{u"absent",nullptr}}) {
    auto title=Wide(entry.first);EXPECT_EQ(api.FPDFBookmark_Find(document,title.data()),entry.second);
  }
  EXPECT_EQ(api.FPDFBookmark_GetStyle(first),0);EXPECT_EQ(api.FPDFBookmark_GetStyle(child),1);EXPECT_EQ(api.FPDFBookmark_GetStyle(empty),2);EXPECT_EQ(api.FPDFBookmark_GetStyle(duplicate),3);EXPECT_EQ(api.FPDFBookmark_GetStyle(nullptr),0);
  float red=91,green=92,blue=93;
  ASSERT_TRUE(api.FPDFBookmark_GetColor(first,&red,&green,&blue));EXPECT_FLOAT_EQ(red,0);EXPECT_FLOAT_EQ(green,1);EXPECT_FLOAT_EQ(blue,.25);
  ASSERT_TRUE(api.FPDFBookmark_GetColor(leaf,&red,&green,&blue));EXPECT_FLOAT_EQ(red,1);EXPECT_FLOAT_EQ(green,0);EXPECT_FLOAT_EQ(blue,1);
  for(auto bookmark:{unicode,duplicate,FPDF_BOOKMARK{}}) {
    red=91;green=92;blue=93;EXPECT_FALSE(api.FPDFBookmark_GetColor(bookmark,&red,&green,&blue));EXPECT_FLOAT_EQ(red,91);EXPECT_FLOAT_EQ(green,92);EXPECT_FLOAT_EQ(blue,93);
  }
  auto first_dest=api.FPDFBookmark_GetDest(document,first);ASSERT_NE(first_dest,nullptr);EXPECT_EQ(api.FPDFDest_GetDestPageIndex(document,first_dest),0);
  auto named=api.FPDFBookmark_GetDest(document,empty);ASSERT_NE(named,nullptr);EXPECT_EQ(api.FPDFDest_GetDestPageIndex(document,named),1);
  EXPECT_EQ(api.FPDFBookmark_GetDest(document,unicode),nullptr);
  EXPECT_EQ(api.FPDFBookmark_GetAction(empty),nullptr);
  auto go=api.FPDFBookmark_GetAction(first);ASSERT_NE(go,nullptr);EXPECT_EQ(api.FPDFAction_GetType(go),PDFACTION_GOTO);
  EXPECT_EQ(api.FPDFDest_GetDestPageIndex(document,api.FPDFAction_GetDest(document,go)),0);
  auto unsupported=api.FPDFBookmark_GetAction(leaf);ASSERT_NE(unsupported,nullptr);EXPECT_EQ(api.FPDFAction_GetType(unsupported),PDFACTION_UNSUPPORTED);
  auto launch=api.FPDFBookmark_GetAction(duplicate);ASSERT_NE(launch,nullptr);EXPECT_EQ(api.FPDFAction_GetType(launch),PDFACTION_LAUNCH);
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFAction_GetFilePath(launch,b,n);},Ascii("\xd1\x84\xd0\xb0\xd0\xb9\xd0\xbb.pdf"));
  EXPECT_EQ(api.FPDFAction_GetDest(document,launch),nullptr);
  auto remote_bookmark=api.FPDFBookmark_GetFirstChild(document,unicode);ASSERT_NE(remote_bookmark,nullptr);
  auto remote=api.FPDFBookmark_GetAction(remote_bookmark);ASSERT_NE(remote,nullptr);EXPECT_EQ(api.FPDFAction_GetType(remote),PDFACTION_REMOTEGOTO);
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFAction_GetFilePath(remote,b,n);},Ascii("remote.pdf"));
  // Resolve the action's returned relative path in a controlled local base,
  // then actually open that target with the same library before resolving D.
  const auto remote_bytes=OnePage();
  std::array<char,64> remote_name{};ASSERT_EQ(api.FPDFAction_GetFilePath(remote,remote_name.data(),remote_name.size()),11u);
  const auto remote_directory=TestRoot()/"target/capi/navigation-targets";
  std::filesystem::create_directories(remote_directory);
  const auto remote_path=remote_directory/remote_name.data();
  struct RemoveFile {std::filesystem::path path;~RemoveFile(){std::error_code error;std::filesystem::remove(path,error);}} cleanup{remote_path};
  {std::ofstream stream(remote_path,std::ios::binary);stream.write(reinterpret_cast<const char*>(remote_bytes.data()),static_cast<std::streamsize>(remote_bytes.size()));ASSERT_TRUE(stream.good());}
  const auto remote_utf8=remote_path.u8string();
  Handle remote_document(api.FPDF_LoadDocument(remote_utf8.c_str(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(remote_document.get(),nullptr);
  auto remote_dest=api.FPDFAction_GetDest(remote_document.get(),remote);ASSERT_NE(remote_dest,nullptr);EXPECT_EQ(api.FPDFDest_GetDestPageIndex(remote_document.get(),remote_dest),0);
  Handle page(api.FPDF_LoadPage(document,0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
  const auto uri=api.FPDF_GetPageAAction(page.get(),FPDFPAGE_AACTION_CLOSE);ASSERT_NE(uri,nullptr);EXPECT_EQ(api.FPDFAction_GetType(uri),PDFACTION_URI);
  const std::vector<uint8_t> expected_uri={'h','t','t','p','s',':','/','/','e','x','a','m','p','l','e','.','t','e','s','t','/',0xfe,0xc3,0xa9,0};
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFAction_GetURIPath(document,uri,b,n);},expected_uri);
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFAction_GetFilePath(uri,b,n);},{});
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFAction_GetURIPath(document,launch,b,n);},{});
  auto opening=api.FPDF_GetPageAAction(page.get(),FPDFPAGE_AACTION_OPEN);ASSERT_NE(opening,nullptr);EXPECT_EQ(api.FPDFAction_GetType(opening),PDFACTION_GOTO);
  Handle second(api.FPDF_LoadPage(document,1),api.FPDF_ClosePage);ASSERT_NE(second.get(),nullptr);EXPECT_EQ(api.FPDF_GetPageAAction(second.get(),FPDFPAGE_AACTION_OPEN),nullptr);
  EXPECT_EQ(api.FPDF_GetPageAAction(page.get(),99),nullptr);
}

TEST(DocumentNavigation, OutlinesActionsDestinationsAndMetadataSurviveSave) {
  Artifacts artifacts;const auto bytes=NavigationFixture();
  for(const auto* library:Implementations()) {
    const auto& api=library->api();SCOPED_TRACE(library->name());
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    CrossRead(*library,document.get(),artifacts,"navigation",CheckNavigation);
  }
}

TEST(DocumentNavigation, DestinationViewModesAndIndependentXyzPresence) {
  struct View{std::string value;unsigned long mode;std::vector<float> parameters;};
  const std::vector<View> views={{"/XYZ 0 20 1.5",PDFDEST_VIEW_XYZ,{0,20,1.5}},{"/Fit",PDFDEST_VIEW_FIT,{}},{"/FitH 15",PDFDEST_VIEW_FITH,{15}},{"/FitV 25",PDFDEST_VIEW_FITV,{25}},{"/FitR 1 2 30 40",PDFDEST_VIEW_FITR,{1,2,30,40}},{"/FitB",PDFDEST_VIEW_FITB,{}},{"/FitBH 0",PDFDEST_VIEW_FITBH,{0}},{"/FitBV 0",PDFDEST_VIEW_FITBV,{0}},{"/Unknown",PDFDEST_VIEW_UNKNOWN_MODE,{}}};
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    for(const auto& view:views) {
      const auto bytes=OnePage("/Outlines << /First 5 0 R >>",{"<< /Title (view) /Dest [3 0 R "+view.value+"] >>"});
      Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
      auto bookmark=api.FPDFBookmark_GetFirstChild(doc.get(),nullptr);ASSERT_NE(bookmark,nullptr);
      auto dest=api.FPDFBookmark_GetDest(doc.get(),bookmark);ASSERT_NE(dest,nullptr);
      std::array<float,6> params;params.fill(91);unsigned long count=999;
      EXPECT_EQ(api.FPDFDest_GetView(dest,&count,params.data()+1),view.mode);EXPECT_EQ(count,view.parameters.size());
      for(size_t i=0;i<view.parameters.size();++i)EXPECT_FLOAT_EQ(params[i+1],view.parameters[i]);
      for(size_t i=view.parameters.size()+1;i<5;++i)EXPECT_FLOAT_EQ(params[i],91);
      EXPECT_FLOAT_EQ(params[0],91);EXPECT_FLOAT_EQ(params[5],91);
      if(view.mode!=PDFDEST_VIEW_XYZ){FPDF_BOOL x=7,y=8,z=9;float a=91,b=92,c=93;EXPECT_FALSE(api.FPDFDest_GetLocationInPage(dest,&x,&y,&z,&a,&b,&c));}
      else {
        FPDF_BOOL has_x=0,has_y=0,has_zoom=0;float x=91,y=92,zoom=93;
        ASSERT_TRUE(api.FPDFDest_GetLocationInPage(dest,&has_x,&has_y,&has_zoom,&x,&y,&zoom));
        EXPECT_TRUE(has_x);EXPECT_TRUE(has_y);EXPECT_TRUE(has_zoom);EXPECT_FLOAT_EQ(x,0);EXPECT_FLOAT_EQ(y,20);EXPECT_FLOAT_EQ(zoom,1.5f);
      }
    }
    for(int missing=0;missing<8;++missing) {
      const std::string xyz=std::string("/XYZ ")+(missing&1?"null":"0")+" "+(missing&2?"null":"0")+" "+(missing&4?"null":"0");
      const auto bytes=OnePage("/Outlines << /First 5 0 R >>",{"<< /Title (view) /Dest [3 0 R "+xyz+"] >>"});
      Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
      auto dest=api.FPDFBookmark_GetDest(doc.get(),api.FPDFBookmark_GetFirstChild(doc.get(),nullptr));ASSERT_NE(dest,nullptr);
      FPDF_BOOL has_x=7,has_y=8,has_zoom=9;float x=91,y=92,zoom=93;
      ASSERT_TRUE(api.FPDFDest_GetLocationInPage(dest,&has_x,&has_y,&has_zoom,&x,&y,&zoom));
      unsigned long parameter_count=99;std::array<float,4> parameters={91,92,93,94};
      EXPECT_EQ(api.FPDFDest_GetView(dest,&parameter_count,parameters.data()),PDFDEST_VIEW_XYZ);EXPECT_EQ(parameter_count,3u);
      EXPECT_FLOAT_EQ(parameters[0],0);EXPECT_FLOAT_EQ(parameters[1],0);EXPECT_FLOAT_EQ(parameters[2],0);EXPECT_FLOAT_EQ(parameters[3],94);
      EXPECT_EQ(!!has_x,!(missing&1));EXPECT_EQ(!!has_y,!(missing&2));
      // Zero zoom means "retain current magnification", not a present scale.
      EXPECT_FALSE(has_zoom);
      if(has_x){EXPECT_FLOAT_EQ(x,0);}else{EXPECT_FLOAT_EQ(x,91);}
      if(has_y){EXPECT_FLOAT_EQ(y,0);}else{EXPECT_FLOAT_EQ(y,92);}
      EXPECT_FLOAT_EQ(zoom,93);
    }
    EXPECT_EQ(api.FPDFDest_GetDestPageIndex(nullptr,nullptr),-1);
    FPDF_BOOL hx=7,hy=8,hz=9;float x=91,y=92,z=93;
    EXPECT_FALSE(api.FPDFDest_GetLocationInPage(nullptr,&hx,&hy,&hz,&x,&y,&z));
  }
}

TEST(DocumentNavigation, LinksHitTestZOrderQuadsAndAnnotationOwnership) {
  Artifacts artifacts;const auto bytes=NavigationFixture();std::array<Raster,2> rasters;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    int position=0;FPDF_LINK link=nullptr;std::vector<FPDF_LINK> links;
    while(api.FPDFLink_Enumerate(page.get(),&position,&link)){ASSERT_LT(links.size(),4u);ASSERT_NE(link,nullptr);links.push_back(link);}
    ASSERT_EQ(links.size(),3u);EXPECT_EQ(position,4);EXPECT_FALSE(api.FPDFLink_Enumerate(page.get(),&position,&link));
    EXPECT_EQ(api.FPDFLink_GetLinkAtPoint(page.get(),15,15),links[0]);EXPECT_EQ(api.FPDFLink_GetLinkAtPoint(page.get(),30,30),links[1]);EXPECT_EQ(api.FPDFLink_GetLinkAtPoint(page.get(),70,20),links[2]);
    EXPECT_EQ(api.FPDFLink_GetLinkZOrderAtPoint(page.get(),15,15),0);EXPECT_EQ(api.FPDFLink_GetLinkZOrderAtPoint(page.get(),30,30),2);
    EXPECT_EQ(api.FPDFLink_GetLinkAtPoint(page.get(),0,95),nullptr);EXPECT_EQ(api.FPDFLink_GetLinkZOrderAtPoint(page.get(),0,95),-1);
    // Native rectangles include left/bottom/right/top boundaries.
    EXPECT_EQ(api.FPDFLink_GetLinkAtPoint(page.get(),10,10),links[0]);
    EXPECT_EQ(api.FPDFLink_GetLinkAtPoint(page.get(),50,50),links[1]);
    int device_x=0,device_y=0;ASSERT_TRUE(api.FPDF_PageToDevice(page.get(),7,11,192,192,1,15,15,&device_x,&device_y));
    double page_x=0,page_y=0;ASSERT_TRUE(api.FPDF_DeviceToPage(page.get(),7,11,192,192,1,device_x,device_y,&page_x,&page_y));
    EXPECT_EQ(api.FPDFLink_GetLinkAtPoint(page.get(),page_x,page_y),links[0]);
    EXPECT_EQ(api.FPDFDest_GetDestPageIndex(doc.get(),api.FPDFLink_GetDest(doc.get(),links[0])),1);
    EXPECT_EQ(api.FPDFLink_GetDest(doc.get(),links[1]),nullptr);
    EXPECT_EQ(api.FPDFDest_GetDestPageIndex(doc.get(),api.FPDFLink_GetDest(doc.get(),links[2])),1);
    EXPECT_EQ(api.FPDFLink_GetAction(links[0]),nullptr);ASSERT_NE(api.FPDFLink_GetAction(links[1]),nullptr);
    EXPECT_EQ(api.FPDFAction_GetType(api.FPDFLink_GetAction(links[2])),PDFACTION_EMBEDDEDGOTO);
    for(size_t index=0;index<links.size();++index) {
      Handle annot(api.FPDFLink_GetAnnot(page.get(),links[index]),api.FPDFPage_CloseAnnot);ASSERT_NE(annot.get(),nullptr);EXPECT_EQ(api.FPDFAnnot_GetSubtype(annot.get()),FPDF_ANNOT_LINK);
      FS_RECTF rect{};ASSERT_TRUE(api.FPDFLink_GetAnnotRect(links[index],&rect));FS_RECTF via_annot{};ASSERT_TRUE(api.FPDFAnnot_GetRect(annot.get(),&via_annot));
      EXPECT_FLOAT_EQ(rect.left,via_annot.left);EXPECT_FLOAT_EQ(rect.top,via_annot.top);EXPECT_FLOAT_EQ(rect.right,via_annot.right);EXPECT_FLOAT_EQ(rect.bottom,via_annot.bottom);
      const int count=api.FPDFLink_CountQuadPoints(links[index]);EXPECT_EQ(count,(std::array<int,3>{2,0,1})[index]);
      for(int quad=0;quad<count;++quad){
        FS_QUADPOINTSF points{};ASSERT_TRUE(api.FPDFLink_GetQuadPoints(links[index],quad,&points));
        const float inset=quad?2:0;
        const std::array<float,8> expected=index==0?std::array<float,8>{10+inset,50-inset,50-inset,50-inset,10+inset,10+inset,50-inset,10+inset}:std::array<float,8>{60.5f,30.5f,80.75f,30.5f,60.5f,10.25f,80.75f,10.25f};
        const std::array<float,8> actual={points.x1,points.y1,points.x2,points.y2,points.x3,points.y3,points.x4,points.y4};
        for(size_t coordinate=0;coordinate<8;++coordinate)EXPECT_FLOAT_EQ(actual[coordinate],expected[coordinate]);
      }
      FS_QUADPOINTSF sentinel{91,92,93,94,95,96,97,98};EXPECT_FALSE(api.FPDFLink_GetQuadPoints(links[index],-1,&sentinel));EXPECT_FALSE(api.FPDFLink_GetQuadPoints(links[index],count,&sentinel));
    }
    FS_RECTF fractional{};ASSERT_TRUE(api.FPDFLink_GetAnnotRect(links[2],&fractional));EXPECT_FLOAT_EQ(fractional.left,60.5f);EXPECT_FLOAT_EQ(fractional.bottom,10.25f);EXPECT_FLOAT_EQ(fractional.right,80.75f);EXPECT_FLOAT_EQ(fractional.top,30.5f);
    EXPECT_EQ(api.FPDFLink_GetAnnot(nullptr,links[0]),nullptr);EXPECT_EQ(api.FPDFLink_GetAnnot(page.get(),nullptr),nullptr);EXPECT_FALSE(api.FPDFLink_GetAnnotRect(nullptr,&fractional));
    rasters[backend++]=Render(api,page.get(),96,96,FPDF_ANNOT);
  }
  CompareRaster(rasters[0],rasters[1],artifacts,"link-annotations");
}

TEST(DocumentNavigation, FileIdentifiersAndMetadataBufferContracts) {
  Artifacts artifacts;const auto bytes=NavigationFixture();
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetFileIdentifier(doc.get(),FILEIDTYPE_PERMANENT,b,n);},{'A',0,'B',255,0});
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetFileIdentifier(doc.get(),FILEIDTYPE_CHANGING,b,n);},{'C',0,'D',254,0});
    const std::vector<std::pair<const char*,std::u16string>> tags={{"Title",u"Metadata \u03a9"},{"Author",u"Author"},{"Subject",u""},{"Keywords",u"a,b"},{"Creator",u"Creator"},{"Producer",u"Producer"},{"CreationDate",u"D:20260203112233Z"},{"ModDate",u"D:20260204112233+01'00'"},{"absent",u""}};
    for(const auto& tag:tags)ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetMetaText(doc.get(),tag.first,b,n);},Utf16(tag.second));
    auto absent_bytes=OnePage();Handle absent(api.FPDF_LoadMemDocument64(absent_bytes.data(),absent_bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(absent.get(),nullptr);
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetFileIdentifier(absent.get(),FILEIDTYPE_PERMANENT,b,n);},{});
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetMetaText(absent.get(),"Title",b,n);},{});
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetMetaText(nullptr,"Title",b,n);},{});
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetMetaText(doc.get(),nullptr,b,n);},{});
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetFileIdentifier(nullptr,FILEIDTYPE_PERMANENT,b,n);},{});
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetFileIdentifier(doc.get(),static_cast<FPDF_FILEIDTYPE>(99),b,n);},{});
    CrossRead(*library,doc.get(),artifacts,"metadata-and-identifiers",[&](const Api& read,FPDF_DOCUMENT loaded){
      for(const auto& tag:tags)ExpectBytes([&](void* b,unsigned long n){return read.FPDF_GetMetaText(loaded,tag.first,b,n);},Utf16(tag.second));
      for(auto id:{FILEIDTYPE_PERMANENT,FILEIDTYPE_CHANGING}) {
        const unsigned long needed=read.FPDF_GetFileIdentifier(loaded,id,nullptr,0);ASSERT_GT(needed,1u);ASSERT_LT(needed,1024u);
        std::vector<uint8_t> identifier(needed);ASSERT_EQ(read.FPDF_GetFileIdentifier(loaded,id,identifier.data(),needed),needed);EXPECT_EQ(identifier.back(),0);
        ExpectBytes([&](void* b,unsigned long n){return read.FPDF_GetFileIdentifier(loaded,id,b,n);},identifier);
      }
    });
  }
}

TEST(DocumentNavigation, CatalogLanguageAndTaggedStateRoundTrip) {
  Artifacts artifacts;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    for(const std::string mark:{"","/MarkInfo << >>","/MarkInfo << /Marked false >>","/MarkInfo << /Marked true >>"}) {
      const auto bytes=OnePage(mark);Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
      EXPECT_EQ(!!api.FPDFCatalog_IsTagged(doc.get()),mark.find("true")!=std::string::npos);
      ExpectBytes([&](void* b,unsigned long n){return api.FPDFCatalog_GetLanguage(doc.get(),static_cast<FPDF_WCHAR*>(b),n);},Utf16(u""));
      for(const auto& value:{std::u16string(u"en-US"),std::u16string(u"zh-Hant \u03a9\U0001f642"),std::u16string(u"")}) {
        auto wide=Wide(value);ASSERT_TRUE(api.FPDFCatalog_SetLanguage(doc.get(),wide.data()));
        CrossRead(*library,doc.get(),artifacts,"catalog-language",[&](const Api& read,FPDF_DOCUMENT loaded){
          EXPECT_EQ(!!read.FPDFCatalog_IsTagged(loaded),mark.find("true")!=std::string::npos);
          ExpectBytes([&](void* b,unsigned long n){return read.FPDFCatalog_GetLanguage(loaded,static_cast<FPDF_WCHAR*>(b),n);},Utf16(value));
        });
      }
      EXPECT_FALSE(api.FPDFCatalog_SetLanguage(doc.get(),nullptr));
    }
    EXPECT_FALSE(api.FPDFCatalog_IsTagged(nullptr));const auto empty=Wide(u"");EXPECT_FALSE(api.FPDFCatalog_SetLanguage(nullptr,empty.data()));
    ExpectBytes([&](void* b,unsigned long n){return api.FPDFCatalog_GetLanguage(nullptr,static_cast<FPDF_WCHAR*>(b),n);},{});
  }
}

TEST(DocumentNavigation, PageLabelsDecimalRomanAlphabeticAndRestarts) {
  std::vector<std::string> objects={"<< /Type /Catalog /Pages 2 0 R /PageLabels << /Nums [0 << /S /D /P (p-) /St 2 >> 2 << /S /R /St 4 >> 4 << /S /r /St 9 >> 6 << /S /A /St 26 >> 8 << /S /a /P (x) /St 27 >>] >> >>","<< /Type /Pages /Kids ["};
  for(int i=0;i<10;++i)objects[1]+=std::to_string(3+i)+" 0 R ";objects[1]+="] /Count 10 >>";
  for(int i=0;i<10;++i)objects.push_back("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Resources << >> >>");
  const auto bytes=PdfFixture(objects);const std::array<std::u16string,10> labels={u"p-2",u"p-3",u"IV",u"V",u"ix",u"x",u"Z",u"AA",u"xaa",u"xbb"};
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    for(int i=0;i<10;++i)ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetPageLabel(doc.get(),i,b,n);},Utf16(labels[i]));
    for(int i:{-1,10})ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetPageLabel(doc.get(),i,b,n);},{});
    const auto plain=OnePage();Handle no_labels(api.FPDF_LoadMemDocument64(plain.data(),plain.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(no_labels.get(),nullptr);
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetPageLabel(no_labels.get(),0,b,n);},{});
    const auto defaults=OnePage("/PageLabels << /Nums [] >>");Handle default_labels(api.FPDF_LoadMemDocument64(defaults.data(),defaults.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(default_labels.get(),nullptr);
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetPageLabel(default_labels.get(),0,b,n);},Utf16(u"1"));
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_GetPageLabel(nullptr,0,b,n);},{});
  }
}

TEST(DocumentNavigation, CircularOutlineFindIsBoundedAndDoesNotLoseFirstMatch) {
  const auto bytes=OnePage("/Outlines << /First 5 0 R >>",{"<< /Title (Loop) /Next 5 0 R /First 5 0 R >>"});
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    auto first=api.FPDFBookmark_GetFirstChild(doc.get(),nullptr);ASSERT_NE(first,nullptr);
    auto present=Wide(u"Loop"),absent=Wide(u"absent");EXPECT_EQ(api.FPDFBookmark_Find(doc.get(),present.data()),first);EXPECT_EQ(api.FPDFBookmark_Find(doc.get(),absent.data()),nullptr);
    std::set<FPDF_BOOKMARK> visited;auto current=first;for(int count=0;current&&count<4;++count){if(!visited.insert(current).second)break;current=api.FPDFBookmark_GetNextSibling(doc.get(),current);}EXPECT_EQ(visited.size(),1u);
  }
}

TEST(DocumentNavigation, EmptyOutlinesAndMissingOrMalformedDestinationsFailSafely) {
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    const auto bytes=OnePage();Handle empty(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(empty.get(),nullptr);
    EXPECT_EQ(api.FPDFBookmark_GetFirstChild(empty.get(),nullptr),nullptr);EXPECT_EQ(api.FPDFBookmark_GetNextSibling(empty.get(),nullptr),nullptr);
    for(const std::string value:{"[]","[true /Fit]"}) {
      const auto malformed=OnePage("/Outlines << /First 5 0 R >>",{"<< /Title (bad-dest) /Dest "+value+" /A << /S /GoTo >> >>"});
      Handle doc(api.FPDF_LoadMemDocument64(malformed.data(),malformed.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
      auto bookmark=api.FPDFBookmark_GetFirstChild(doc.get(),nullptr);ASSERT_NE(bookmark,nullptr);
      auto dest=api.FPDFBookmark_GetDest(doc.get(),bookmark);ASSERT_NE(dest,nullptr);
      EXPECT_EQ(api.FPDFDest_GetDestPageIndex(doc.get(),dest),-1);
      auto action=api.FPDFBookmark_GetAction(bookmark);ASSERT_NE(action,nullptr);EXPECT_EQ(api.FPDFAction_GetDest(doc.get(),action),nullptr);
    }
  }
}

}  // namespace
}  // namespace pdfoxium::integration
