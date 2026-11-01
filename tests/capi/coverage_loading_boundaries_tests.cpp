#include "document_fixture.h"
#include "runtime_fixture.h"

#include <array>
#include <cmath>
#include <cstdint>
#include <functional>
#include <iomanip>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace pdfoxium::integration {
namespace {
using Bytes=std::vector<uint8_t>;
std::string Raw(const Bytes& bytes){return {bytes.begin(),bytes.end()};}
Bytes Data(const std::string& text){return {text.begin(),text.end()};}
void Replace(std::string& text,const std::string& from,const std::string& to) {
  const auto at=text.find(from);ASSERT_NE(at,std::string::npos)<<from;text.replace(at,from.size(),to);
}
void ExpectRect(const Raster& image) {ASSERT_NO_FATAL_FAILURE(runtime_test::ExpectRectangle(image));}
Raster Page(const Api& api,FPDF_DOCUMENT document,int index=0) {
  Handle page(api.FPDF_LoadPage(document,index),api.FPDF_ClosePage);
  if(!page.get()){ADD_FAILURE()<<"page "<<index;return {};}
  return Render(api,page.get(),128,96);
}
void CrossReload(const Library& writer,FPDF_DOCUMENT document,const Raster& before,
                 Artifacts& artifacts,const std::string& label,const std::function<void(const Api&,FPDF_DOCUMENT)>& inspect={}) {
  const auto saved=SaveDocument(writer.api(),document,artifacts,writer.name()+"-"+label);ASSERT_FALSE(saved.empty());
  for(const auto* reader:Implementations()) {
    SCOPED_TRACE(writer.name()+"->"+reader->name());const auto& api=reader->api();
    Handle reopened(api.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);
    if(inspect) { ASSERT_NO_FATAL_FAILURE(inspect(api,reopened.get())); }
    const auto image=Page(api,reopened.get());
    CompareRaster(before,image,artifacts,writer.name()+"-"+label+"-"+reader->name());
  }
}
Bytes SecurityPdf(const std::string& dictionary) {
  return PdfFixture({"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents 4 0 R >>",
      PdfStream("","0.25 0.5 0.75 rg 16 16 64 48 re f"),dictionary},"/Encrypt 5 0 R /ID [<01020304> <05060708>]");
}

TEST(CoverageLoadingBoundaries, IdentityCryptFiltersNormalizePermissionBitsWithoutEncryptingSamples) {
  // CPDF_SecurityHandler::OnInit short-circuits authentication for the explicit
  // Identity filter; no encryption primitive or invented password is needed.
  const std::array<int32_t,7> permissions={0,4,8,16,32,-4,-3904};Artifacts artifacts;
  for(auto permission:permissions)for(bool metadata:{false,true}) {
    const auto label="identity-p"+std::to_string(permission)+(metadata?"-metadata":"-no-metadata");
    SCOPED_TRACE(label);
    const auto bytes=SecurityPdf("<< /Filter /Standard /V 4 /R 4 /P "+std::to_string(permission)+
        " /CF << >> /StmF /Identity /StrF /Identity /EncryptMetadata "+std::string(metadata?"true":"false")+" >>");
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());const auto& api=library->api();
      for(const char* password:{static_cast<const char*>(nullptr),"unneeded"}) {
        Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),password),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
        const auto expected=(static_cast<uint32_t>(permission)&0xfffffffcu)|0xfffff0c0u;
        EXPECT_EQ(api.FPDF_GetDocPermissions(document.get()),expected);EXPECT_EQ(api.FPDF_GetDocUserPermissions(document.get()),expected);
        EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(document.get()),4);
        images[backend]=Page(api,document.get());ASSERT_NO_FATAL_FAILURE(ExpectRect(images[backend]));
        if(permission==-3904&&metadata&&password==nullptr) {
          CrossReload(*library,document.get(),images[backend],artifacts,label,
            [=](const Api& read,FPDF_DOCUMENT doc){EXPECT_EQ(read.FPDF_GetDocUserPermissions(doc),expected);EXPECT_EQ(read.FPDF_GetSecurityHandlerRevision(doc),4);});
        }
      }
      ++backend;
    }
    CompareRaster(images[0],images[1],artifacts,label);
  }
}

TEST(CoverageLoadingBoundaries, InvalidCryptFilterStructuresFailBeforeFollowingHealthyLoads) {
  struct Case{const char* name;std::string entries;};
  const std::vector<Case> cases={
      {"missing-cf","/V 4 /R 4 /StmF /Identity /StrF /Identity"},
      {"non-dictionary-cf","/V 4 /R 4 /CF 17 /StmF /Identity /StrF /Identity"},
      {"different-stream-string","/V 4 /R 4 /CF << /StdCF << /CFM /AESV2 /Length 16 >> >> /StmF /StdCF /StrF /Other"},
      {"missing-selected-filter","/V 4 /R 4 /CF << >> /StmF /Absent /StrF /Absent"},
      {"non-dictionary-filter","/V 4 /R 4 /CF << /StdCF [1 2] >> /StmF /StdCF /StrF /StdCF"},
      {"negative-length","/V 4 /R 4 /CF << /StdCF << /CFM /AESV2 /Length -1 >> >> /StmF /StdCF /StrF /StdCF"},
      {"too-long-key","/V 4 /R 4 /CF << /StdCF << /CFM /AESV2 /Length 512 >> >> /StmF /StdCF /StrF /StdCF"},
      {"short-rc4-key","/V 4 /R 4 /CF << /StdCF << /CFM /V2 /Length 2 >> >> /StmF /StdCF /StrF /StdCF"},
      {"short-aes-key","/V 4 /R 4 /CF << /StdCF << /CFM /AESV2 /Length 8 >> >> /StmF /StdCF /StrF /StdCF"},
      {"missing-legacy-u","/V 1 /R 2 /O <0001>"},
      {"missing-legacy-o","/V 2 /R 3 /Length 128 /U <0001>"},
      {"negative-legacy-length","/V 2 /R 3 /Length -8"},
      {"too-long-legacy-length","/V 2 /R 3 /Length 264"},
      {"missing-aes-validation-keys","/V 5 /R 6 /Length 256 /CF << /StdCF << /CFM /AESV3 /Length 32 >> >> /StmF /StdCF /StrF /StdCF"}};
  const auto healthy=runtime_test::RectangleFixture();
  for(const auto& item:cases) {
    SCOPED_TRACE(item.name);const auto bytes=SecurityPdf("<< /Filter /Standard /P -4 "+item.entries+" >>");
    for(const auto* library:Implementations()) {
      const auto& api=library->api();
    SCOPED_TRACE(library->name());
      for(const char* password:{"", "wrong"}) {
        Handle denied(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),password),api.FPDF_CloseDocument);
        EXPECT_EQ(denied.get(),nullptr);EXPECT_EQ(api.FPDF_GetLastError(),static_cast<unsigned long>(FPDF_ERR_PASSWORD));
      }
      Handle next(api.FPDF_LoadMemDocument64(healthy.data(),healthy.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(next.get(),nullptr);
      ASSERT_NO_FATAL_FAILURE(ExpectRect(Page(api,next.get())));
    }
  }
}

TEST(CoverageLoadingBoundaries, AesKeyEntryAndPermissionDamagePreservesOnlyIndependentlyValidPasswordPaths) {
  // Reuse upstream encrypted packets rather than adding test crypto. Equal
  // length key renames preserve xref offsets and damage only the selected key.
  struct Change{const char* name;const char* from;const char* to;bool user,owner;};
  const std::array<Change,6> changes={{{"missing-o","/O ","/Z ",false,false},
      {"missing-u","/U ","/Z ",false,false},{"missing-oe","/OE ","/ZE ",true,false},
      {"missing-ue","/UE ","/ZE ",false,true},{"missing-perms","/Perms ","/Zerms ",false,false},
      {"changed-permission","/P -4","/P -8",false,false}}};
  Artifacts artifacts;
  for(int revision:{5,6}) {
    const auto original=runtime_test::Resource("encrypted_hello_world_r"+std::to_string(revision)+".pdf");
    for(const auto& change:changes) {
      SCOPED_TRACE(change.name);
    SCOPED_TRACE(revision);auto changed=Raw(original);Replace(changed,change.from,change.to);const auto bytes=Data(changed);
      for(const auto* library:Implementations()) {
        SCOPED_TRACE(library->name());const auto& api=library->api();
        for(bool owner:{false,true}) {
          SCOPED_TRACE(owner);// Pinned OwnerPasswordVersion5/6UTF8 uses âge; hôtel is user.
          const char* password=owner?"\xc3\xa2ge":"h\xc3\xb4tel";
          Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),password),api.FPDF_CloseDocument);
          if(!(owner?change.owner:change.user)) {EXPECT_EQ(document.get(),nullptr);EXPECT_EQ(api.FPDF_GetLastError(),static_cast<unsigned long>(FPDF_ERR_PASSWORD));}
          else {
            ASSERT_NE(document.get(),nullptr);EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(document.get()),revision);
            Handle control(api.FPDF_LoadMemDocument64(original.data(),original.size(),password),api.FPDF_CloseDocument);ASSERT_NE(control.get(),nullptr);
            CompareRaster(Page(api,control.get()),Page(api,document.get()),artifacts,library->name()+"-aes"+std::to_string(revision)+change.name);
          }
        }
      }
    }
  }
}

TEST(CoverageLoadingBoundaries, PinnedDamagedOwnerKeyPacketsRejectAndDoNotPoisonPasswordRetry) {
  // Source: cpdf_security_handler_embeddertest.cpp BadOkeyVersion2/3,
  // crbug.com/42270437. These bounded packets are upstream non-crash cases.
  for(int revision:{2,3}) {
    const auto damaged=runtime_test::Resource("encrypted_hello_world_r"+std::to_string(revision)+"_bad_okey.pdf");
    const auto valid=runtime_test::Resource("encrypted_hello_world_r"+std::to_string(revision)+".pdf");
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());
    SCOPED_TRACE(revision);const auto& api=library->api();
      Handle denied(api.FPDF_LoadMemDocument64(damaged.data(),damaged.size(),"a"),api.FPDF_CloseDocument);EXPECT_EQ(denied.get(),nullptr);
      EXPECT_EQ(api.FPDF_GetLastError(),static_cast<unsigned long>(FPDF_ERR_PASSWORD));
      for(const char* password:{"\xe2ge","h\xf4tel"}) {
        Handle document(api.FPDF_LoadMemDocument64(valid.data(),valid.size(),password),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
        EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(document.get()),revision);const auto image=Page(api,document.get());
        EXPECT_TRUE(std::any_of(image.pixels.begin(),image.pixels.end(),[](uint8_t v){return v!=255;}));
      }
    }
  }
}

TEST(CoverageLoadingBoundaries, CrossReferenceRepairIgnoresFalseObjectMarkersAndBoundedBrokenTables) {
  const auto original=Raw(runtime_test::RectangleFixture());const auto xref=original.find("xref\n");ASSERT_NE(xref,std::string::npos);
  const auto body=original.substr(0,xref);std::vector<std::pair<std::string,std::string>> cases;
  // Strings/comments containing apparent catalog/object markers must never
  // replace the real objects during RebuildCrossRef's token scan.
  const std::string junk="% 91 0 obj << /Type /Catalog /Pages 99 0 R >> endobj\n(92 0 obj << /Type /Catalog >> endobj)\n<39332030206f626a20656e646f626a>\n";
  for(const auto& tail:std::vector<std::pair<std::string,std::string>>{{"zero-startxref","0"},{"negative-startxref","-1"},{"non-numeric-startxref","unknown"},{"past-end-startxref","999999"}})
    cases.push_back({tail.first,body+junk+"trailer << /Size 5 /Root 1 0 R >>\nstartxref\n"+tail.second+"\n%%EOF\n"});
  const std::vector<std::string> fields={"/W []","/W [-1 4 2]","/W [1 9 2]","/W [0 0 0]",
      "/W [1 4 2] /Index [-1 1]","/W [1 4 2] /Index [2]","/W [1 4 2] /Index [0 6]"};
  for(size_t i=0;i<fields.size();++i) {
    auto text=body+junk;const auto offset=text.size();text+="5 0 obj\n"+PdfStream("/Type /XRef /Size 6 /Root 1 0 R "+fields[i],std::string(3,'\0'))+"\nendobj\nstartxref\n"+std::to_string(offset)+"\n%%EOF\n";
    cases.push_back({"xref-stream-field"+std::to_string(i),std::move(text)});
  }
  Artifacts artifacts;
  for(const auto& item:cases) {
    SCOPED_TRACE(item.first);const auto bytes=Data(item.second);artifacts.Pdf(item.first,bytes);std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      const auto& api=library->api();
    SCOPED_TRACE(library->name());
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
      EXPECT_EQ(api.FPDF_GetPageCount(document.get()),1);
      // This API exposes the parser's initial-xref-rebuilt flag. Width/Index
      // headers2..6 are accepted before the short payload triggers secondary
      // root recovery; that recovery deliberately leaves the flag unchanged
      // (CpdfParser::parse_internal, root/try_init fallback).
      const bool header_accepted=item.first.rfind("xref-stream-field",0)==0 &&
          item.first!="xref-stream-field0" && item.first!="xref-stream-field1";
      EXPECT_EQ(!!api.FPDF_DocumentHasValidCrossReferenceTable(document.get()),header_accepted);
      images[backend]=Page(api,document.get());ASSERT_NO_FATAL_FAILURE(ExpectRect(images[backend]));
      if(item.first=="zero-startxref"||item.first=="xref-stream-field1")CrossReload(*library,document.get(),images[backend],artifacts,item.first);
      ++backend;
    }
    CompareRaster(images[0],images[1],artifacts,item.first+"-readers");
  }
}

struct Sparse : FX_FILEAVAIL,FX_DOWNLOADHINTS {
  explicit Sparse(Bytes input,bool ready=false):bytes(std::move(input)),present(bytes.size(),ready) {
    FX_FILEAVAIL::version=1;FX_DOWNLOADHINTS::version=1;
    FX_FILEAVAIL::IsDataAvail=[](FX_FILEAVAIL* p,size_t offset,size_t size)->FPDF_BOOL {
      auto& s=*static_cast<Sparse*>(p);if(offset>s.bytes.size()||size>s.bytes.size()-offset)return 0;
      return std::all_of(s.present.begin()+offset,s.present.begin()+offset+size,[](bool v){return v;});};
    FX_DOWNLOADHINTS::AddSegment=[](FX_DOWNLOADHINTS* p,size_t offset,size_t size) {
      auto& s=*static_cast<Sparse*>(p);try{s.hints.emplace_back(offset,size);}catch(...){s.valid=false;}};
    access.m_FileLen=bytes.size();access.m_Param=this;
    access.m_GetBlock=[](void* p,unsigned long offset,unsigned char* output,unsigned long size)->int {
      auto& s=*static_cast<Sparse*>(p);++s.reads;
      const bool bounded=output&&size&&offset<=s.bytes.size()&&size<=s.bytes.size()-offset;s.valid&=bounded;
      if(!bounded||!s.IsDataAvail(&s,offset,size))return 0;
      std::copy_n(s.bytes.data()+offset,size,output);return 1;};
  }
  void Deliver(size_t maximum=512) {
    for(const auto& hint:hints) {
      EXPECT_LE(hint.first,bytes.size());const auto begin=std::min(hint.first,bytes.size());const auto end=begin+std::min(hint.second,bytes.size()-begin);
      auto first=std::find(present.begin()+begin,present.begin()+end,false);const auto count=std::min(maximum,static_cast<size_t>(present.begin()+end-first));
      std::fill(first,first+count,true);delivered+=count;
    }
    hints.clear();
  }
  void All(){std::fill(present.begin(),present.end(),true);}
  Bytes bytes;std::vector<bool> present;FPDF_FILEACCESS access{};std::vector<std::pair<size_t,size_t>> hints;
  bool valid=true;size_t reads=0,delivered=0;
};
Bytes NestedDocument(bool indirect_kids,bool indirect_count) {
  std::vector<std::string> objects={"<< /Type /Catalog /Pages 2 0 R /Metadata 15 0 R >>",
      "<< /Type /Pages /Count "+std::string(indirect_count?"14 0 R":"4")+" /Kids "+std::string(indirect_kids?"13 0 R":"[3 0 R 4 0 R]")+" >>",
      "<< /Type /Pages /Parent 2 0 R /Count 2 /Kids [5 0 R 6 0 R] >>",
      "<< /Type /Pages /Parent 2 0 R /Count 2 /Kids [7 0 R 8 0 R] >>"};
  for(int i=0;i<4;++i)objects.push_back("<< /Type /Page /Parent "+std::to_string(i<2?3:4)+" 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents "+std::to_string(9+i)+" 0 R >>");
  for(const char* color:{"1 0 0","0 1 0","0 0 1","1 0.5 0"})objects.push_back(PdfStream("",std::string(color)+" rg 16 16 64 48 re f"));
  objects.push_back("[3 0 R 4 0 R]");objects.push_back("4");objects.push_back(PdfStream("/Type /Metadata /Subtype /XML","<metadata>boundaries</metadata>"));
  objects.push_back("<< /Title (Boundaries) >>");
  std::string text="%PDF-1.7\n";std::vector<size_t> offsets{0};
  for(size_t i=0;i<objects.size();++i){text+="%"+std::string(2046,' ')+"\n";offsets.push_back(text.size());text+=std::to_string(i+1)+" 0 obj\n"+objects[i]+"\nendobj\n";}
  const auto xref=text.size();std::ostringstream entries;entries<<"xref\n0 "<<objects.size()+1<<"\n0000000000 65535 f \n";
  for(size_t i=1;i<offsets.size();++i)entries<<std::setw(10)<<std::setfill('0')<<offsets[i]<<" 00000 n \n";
  text+=entries.str()+"trailer << /Size "+std::to_string(objects.size()+1)+" /Root 1 0 R /Info 16 0 R >>\nstartxref\n"+std::to_string(xref)+"\n%%EOF\n";return Data(text);
}
void ExpectPageColor(const Raster& image,int page) {
  const std::array<std::array<int,3>,4> expected={{{255,0,0},{0,255,0},{0,0,255},{255,128,0}}};
  const auto at=(48*image.width+40)*4;EXPECT_NEAR(image.pixels.at(at+2),expected[page][0],1);EXPECT_NEAR(image.pixels.at(at+1),expected[page][1],1);EXPECT_NEAR(image.pixels.at(at),expected[page][2],1);
  EXPECT_EQ(image.pixels.at(0),255);EXPECT_EQ(image.pixels.at(1),255);EXPECT_EQ(image.pixels.at(2),255);
}
TEST(CoverageLoadingBoundaries, SparseIndirectPageTreesAndProviderReusePreservePageIdentity) {
  Artifacts artifacts;
  for(bool kids:{false,true})for(bool count:{false,true}) {
    SCOPED_TRACE(kids);
    SCOPED_TRACE(count);const auto input=NestedDocument(kids,count);std::array<std::array<Raster,4>,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());const auto& api=library->api();Sparse source(input);
      Handle provider(api.FPDFAvail_Create(&source,&source.access),api.FPDFAvail_Destroy);ASSERT_NE(provider.get(),nullptr);
      int status=PDF_DATA_NOTAVAIL;
      for(int poll=0;poll<2048&&status==PDF_DATA_NOTAVAIL;++poll){status=api.FPDFAvail_IsDocAvail(provider.get(),&source);source.Deliver();}
      ASSERT_EQ(status,PDF_DATA_AVAIL);EXPECT_EQ(api.FPDFAvail_IsLinearized(provider.get()),PDF_NOT_LINEARIZED);
      for(int pass=0;pass<2;++pass) {
        Handle document(api.FPDFAvail_GetDocument(provider.get(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);EXPECT_EQ(api.FPDF_GetPageCount(document.get()),4);
        for(int page:{3,1,0,2}) {
          status=PDF_DATA_NOTAVAIL;
          for(int poll=0;poll<2048&&status==PDF_DATA_NOTAVAIL;++poll){status=api.FPDFAvail_IsPageAvail(provider.get(),page,&source);source.Deliver();}
          ASSERT_EQ(status,PDF_DATA_AVAIL);images[backend][page]=Page(api,document.get(),page);ExpectPageColor(images[backend][page],page);
        }
        source.All();
        if(kids&&count&&pass==1)CrossReload(*library,document.get(),images[backend][0],artifacts,"nested-indirect-tree",[](const Api& read,FPDF_DOCUMENT doc){EXPECT_EQ(read.FPDF_GetPageCount(doc),4);for(int p=0;p<4;++p)ExpectPageColor(Page(read,doc,p),p);});
      }
      EXPECT_TRUE(source.valid);EXPECT_GT(source.reads,0u);EXPECT_GT(source.delivered,0u);++backend;
    }
    for(int page=0;page<4;++page)CompareRaster(images[0][page],images[1][page],artifacts,"nested"+std::to_string(kids)+std::to_string(count)+"-p"+std::to_string(page));
  }
  // A failed password must not consume the provider's one live-document slot.
  const auto encrypted=runtime_test::Resource("encrypted_hello_world_r6.pdf");
  for(const auto* library:Implementations()) {
    const auto& api=library->api();Sparse source(encrypted,true);Handle provider(api.FPDFAvail_Create(&source,&source.access),api.FPDFAvail_Destroy);ASSERT_NE(provider.get(),nullptr);
    int readiness=PDF_DATA_NOTAVAIL;
    // Ready bytes do not imply that the availability state machine has
    // completed its xref/encryption/full-file fallback transitions.
    for(int poll=0;poll<64&&readiness==PDF_DATA_NOTAVAIL;++poll)
      readiness=api.FPDFAvail_IsDocAvail(provider.get(),nullptr);
    ASSERT_EQ(readiness,PDF_DATA_AVAIL);
    EXPECT_EQ(api.FPDFAvail_GetDocument(provider.get(),"wrong"),nullptr);EXPECT_EQ(api.FPDF_GetLastError(),static_cast<unsigned long>(FPDF_ERR_PASSWORD));
    for(const char* password:{"\xc3\xa2ge","h\xc3\xb4tel"}) {
      Handle document(api.FPDFAvail_GetDocument(provider.get(),password),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
      EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(document.get()),6);
      readiness=PDF_DATA_NOTAVAIL;
      for(int poll=0;poll<64&&readiness==PDF_DATA_NOTAVAIL;++poll)
        readiness=api.FPDFAvail_IsPageAvail(provider.get(),0,nullptr);
      EXPECT_EQ(readiness,PDF_DATA_AVAIL);
    }
    EXPECT_TRUE(source.valid);
  }
}

TEST(CoverageLoadingBoundaries, InvalidLinearizationHeaderNumbersFallBackToTheIntactOrdinaryDocument) {
  const auto original=runtime_test::Resource("linearized.pdf");
  const std::vector<std::pair<std::string,std::string>> changes={{"/Linearized","/Zinearized"},{"/L 11384","/L 11383"},{"/O 31","/O 00"},{"/E 1880","/E 0000"},{"/N 3","/N 0"},{"/T 10709","/T 00000"}};
  Artifacts artifacts;
  for(size_t index=0;index<changes.size();++index) {
    auto changed=Raw(original);Replace(changed,changes[index].first,changes[index].second);ASSERT_EQ(changed.size(),original.size());const auto bytes=Data(changed);
    SCOPED_TRACE(index);
    std::array<std::array<Raster,3>,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());const auto& api=library->api();Sparse source(bytes);Handle provider(api.FPDFAvail_Create(&source,&source.access),api.FPDFAvail_Destroy);ASSERT_NE(provider.get(),nullptr);
      int status=PDF_DATA_NOTAVAIL;for(int poll=0;poll<2048&&status==PDF_DATA_NOTAVAIL;++poll){status=api.FPDFAvail_IsDocAvail(provider.get(),&source);source.Deliver();}
      ASSERT_EQ(status,PDF_DATA_AVAIL);EXPECT_EQ(api.FPDFAvail_IsLinearized(provider.get()),PDF_NOT_LINEARIZED);
      Handle document(api.FPDFAvail_GetDocument(provider.get(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);EXPECT_EQ(api.FPDF_GetPageCount(document.get()),3);
      Handle control(api.FPDF_LoadMemDocument64(original.data(),original.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(control.get(),nullptr);
      source.All();
      for(int page=0;page<3;++page) {
        // Page-number/tree scans can advance without requesting any bytes;
        // poll until the intact ordinary fallback page is actually ready.
        status=PDF_DATA_NOTAVAIL;
        for(int poll=0;poll<2048&&status==PDF_DATA_NOTAVAIL;++poll)
          status=api.FPDFAvail_IsPageAvail(provider.get(),page,nullptr);
        ASSERT_EQ(status,PDF_DATA_AVAIL);
        images[backend][page]=Page(api,document.get(),page);
        CompareRaster(Page(api,control.get(),page),images[backend][page],artifacts,library->name()+"-header"+std::to_string(index)+"-page"+std::to_string(page));
      }
      EXPECT_TRUE(source.valid);++backend;
    }
    for(int page=0;page<3;++page)CompareRaster(images[0][page],images[1][page],artifacts,"header"+std::to_string(index)+"-native-rust-page"+std::to_string(page),RasterTolerance{10,0,0.025});
  }
}

Bytes ContentPdf(const std::vector<std::string>& contents,const std::string& resources="",bool mixed=false) {
  std::string references="[";for(size_t i=0;i<contents.size();++i){references+=std::to_string(5+i)+" 0 R ";if(mixed)references+="null 17 ";}references+="]";
  std::vector<std::string> objects={"<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << /Font << /F1 4 0 R >> "+resources+" >> /Contents "+references+" >>",
      "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>"};
  for(const auto& content:contents)objects.push_back(PdfStream("",content));
  return PdfFixture(objects);
}
struct TextCase {const char* name;std::string operators,control;float x=10,y=70,a=1;};
void CheckText(const Api& api,FPDF_DOCUMENT document,const TextCase& item) {
  Handle page(api.FPDF_LoadPage(document,0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);ASSERT_EQ(api.FPDFPage_CountObjects(page.get()),1);
  const auto object=api.FPDFPage_GetObject(page.get(),0);ASSERT_EQ(api.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_TEXT);FS_MATRIX matrix{};ASSERT_TRUE(api.FPDFPageObj_GetMatrix(object,&matrix));
  EXPECT_NEAR(matrix.a,item.a,0.0001);EXPECT_NEAR(matrix.b,0,0.0001);EXPECT_NEAR(matrix.c,0,0.0001);EXPECT_NEAR(matrix.d,1,0.0001);EXPECT_NEAR(matrix.e,item.x,0.0001);EXPECT_NEAR(matrix.f,item.y,0.0001);
  float size=0;ASSERT_TRUE(api.FPDFTextObj_GetFontSize(object,&size));EXPECT_FLOAT_EQ(size,12);
  Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);EXPECT_EQ(api.FPDFText_CountChars(text.get()),1);EXPECT_EQ(api.FPDFText_GetUnicode(text.get(),0),static_cast<unsigned long>('A'));
}
TEST(CoverageLoadingBoundaries, QuoteAndNumericOnlyTjOperatorsHaveExplicitEquivalentTextPositions) {
  const std::vector<TextCase> cases={
      {"single-quote","(A) '","T* (A) Tj",10,56},
      {"double-quote","2 1 (A) \"","2 Tw 1 Tc T* (A) Tj",10,56},
      {"empty-single-quote","() ' (A) Tj","T* (A) Tj",10,56},
      {"empty-double-quote","2 1 () \" (A) Tj","2 Tw 1 Tc T* (A) Tj",10,56},
      {"numeric-only","[100 200 -50] TJ (A) Tj","1 0 0 1 7 70 Tm (A) Tj",7},
      {"numeric-with-empty-string","[100 200 () -50] TJ (A) Tj","1 0 0 1 7 70 Tm (A) Tj",7},
      {"initial-kerning","[100 () 50 (A)] TJ","1 0 0 1 8.2 70 Tm (A) Tj",8.2f},
      {"empty-array","[] TJ (A) Tj","(A) Tj"},
      {"zero-array","[0 0 0] TJ (A) Tj","(A) Tj"},
      {"negative-leading","-14 TL (A) '","0 14 Td (A) Tj",10,84},
      {"half-horizontal","50 Tz (A) Tj","50 Tz (A) Tj",10,70,.5f},
      {"non-integer-horizontal","37.5 Tz (A) Tj","37.5 Tz (A) Tj",10,70,.375f}};
  Artifacts artifacts;
  for(const auto& item:cases) {
    SCOPED_TRACE(item.name);const std::string prefix="BT /F1 12 Tf 14 TL 1 0 0 1 10 70 Tm ";const auto bytes=ContentPdf({prefix,item.operators+" ET"});const auto control=ContentPdf({prefix+item.control+" ET"});std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      const auto& api=library->api();
    SCOPED_TRACE(library->name());Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);CheckText(api,document.get(),item);
      Handle ordinary(api.FPDF_LoadMemDocument64(control.data(),control.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(ordinary.get(),nullptr);images[backend]=Page(api,document.get());CompareRaster(Page(api,ordinary.get()),images[backend],artifacts,library->name()+"-"+item.name+"-expanded-operators");
      if(item.name==std::string("double-quote")||item.name==std::string("initial-kerning")) {
        const auto saved=SaveDocument(api,document.get(),artifacts,library->name()+"-"+item.name);
        for(const auto* reader:Implementations()){SCOPED_TRACE(reader->name());const auto& read=reader->api();Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);CheckText(read,reopened.get(),item);CompareRaster(images[backend],Page(read,reopened.get()),artifacts,library->name()+"-"+item.name+"-to-"+reader->name(),RasterTolerance{24,0,0.1});}
      }
      ++backend;
    }
    CompareRaster(images[0],images[1],artifacts,std::string(item.name)+"-backends",RasterTolerance{24,0,0.1});
  }
}

TEST(CoverageLoadingBoundaries, PathAbbreviationsDegenerateClipsAndBoundedOperandsPreserveFollowingGraphics) {
  const std::string color="0.25 0.5 0.75 rg ",rectangle="16 16 64 48 re f ";
  struct Case{const char* name;std::string content,control;bool mixed=false;};
  std::string many="BX ";for(int i=0;i<24;++i)many+="("+std::to_string(i)+") ";many+="UnsupportedOperator EX ";
  const std::vector<Case> cases={
      {"explicit-lines",color+"16 16 m 80 16 l 80 64 l 16 64 l h f",color+rectangle},
      {"repeat-move",color+"16 16 m 16 16 m 80 16 l 80 64 l 16 64 l h f",color+rectangle},
      {"replace-move",color+"8 8 m 16 16 m 80 16 l 80 64 l 16 64 l h f",color+rectangle},
      {"trailing-move",color+"16 16 64 48 re 8 8 m f",color+rectangle},
      {"cubic-straight-edges",color+"16 16 m 32 16 64 16 80 16 c 80 32 80 48 80 64 c 64 64 32 64 16 64 c h f",color+rectangle},
      {"v-straight-edges",color+"16 16 m 48 16 80 16 v 80 40 80 64 v 48 64 16 64 v h f",color+rectangle},
      {"y-straight-edges",color+"16 16 m 48 16 80 16 y 80 40 80 64 y 48 64 16 64 y h f",color+rectangle},
      {"closed-even-odd-stroke",color+"16 16 m 80 16 l 80 64 l 16 64 l b*",color+"16 16 64 48 re B*"},
      {"empty-winding-clip","q 10 10 m W n 1 0 1 rg 0 0 128 96 re f Q "+color+rectangle,color+rectangle},
      {"empty-even-odd-clip","q 10 10 m W* n 1 0 1 rg 0 0 128 96 re f Q "+color+rectangle,color+rectangle},
      {"round-closed-point","q 1 J 4 w 10 10 m h S Q "+color+rectangle,"q 1 J 4 w 10 10 m 10 10 l h S Q "+color+rectangle},
      {"orphan-path-operators","h S f n 10 10 l 1 2 3 4 5 6 c n "+color+rectangle,color+rectangle},
      {"parameter-ring",many+color+rectangle,color+rectangle},
      {"missing-resources","/Missing gs /Missing Do /Missing sh /Missing cs 1 sc n "+color+rectangle,color+rectangle},
      {"mixed-contents-array",color+rectangle,color+rectangle,true}};
  Artifacts artifacts;
  const std::string defaults="/ColorSpace << /DefaultRGB /DeviceRGB /DefaultGray /DeviceGray /DefaultCMYK /DeviceCMYK >>";
  for(const auto& item:cases) {
    SCOPED_TRACE(item.name);const auto bytes=ContentPdf({"q 1 0 0 1 0 0 cm ",item.content," Q"},defaults,item.mixed);const auto control=ContentPdf({item.control},defaults);
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());const auto& api=library->api();Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
      Handle reference(api.FPDF_LoadMemDocument64(control.data(),control.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(reference.get(),nullptr);
      images[backend]=Page(api,document.get());CompareRaster(Page(api,reference.get()),images[backend],artifacts,library->name()+"-"+item.name+"-control");
      // Interior/background colors are independent of the equivalent control;
      // the round-point case additionally has a deliberate black mark at10,10.
      ASSERT_NO_FATAL_FAILURE(ExpectRect(images[backend]));
      if(item.name==std::string("v-straight-edges")||item.name==std::string("empty-winding-clip")||item.mixed)CrossReload(*library,document.get(),images[backend],artifacts,item.name);
      ++backend;
    }
    CompareRaster(images[0],images[1],artifacts,std::string(item.name)+"-backends");
  }
}
}  // namespace
}  // namespace pdfoxium::integration
