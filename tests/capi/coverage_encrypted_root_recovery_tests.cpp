// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "document_fixture.h"
#include "runtime_fixture.h"
#include "text_fixture.h"
#include "pinned_test_fonts.h"

#include <array>
#include <iomanip>
#include <sstream>

namespace pdfoxium::integration {
namespace {
using Bytes=std::vector<uint8_t>;
using namespace document_test;
constexpr char kUser[]="h\xc3\xb4" "tel",kOwner[]="\xc3\xa2" "ge";
struct Spec { int revision;std::string root;bool linearized=false,owner=false,recover=true; };
std::string Ten(size_t n){std::ostringstream s;s<<std::setfill('0')<<std::setw(10)<<n;return s.str();}
Bytes Original(int revision){return runtime_test::Resource("encrypted_hello_world_r"+std::to_string(revision)+".pdf");}
std::string Identifier(const std::string& pdf) {
  const auto trailer=pdf.rfind("trailer"),at=pdf.find("/ID",trailer),end=pdf.find(']',at);
  if(trailer==std::string::npos||at==std::string::npos||end==std::string::npos)throw std::runtime_error("Pinned encrypted ID missing");
  return pdf.substr(at,end-at+1);
}
std::array<std::string,8> OpaqueObjects(const std::string& original) {
  std::array<std::string,8> objects;
  for(int n=1;n<=7;++n) {
    const auto marker=std::to_string(n)+" 0 obj";const auto at=original.find(marker),end=original.find("endobj",at);
    if(at==std::string::npos||end==std::string::npos||original.find(marker,at+marker.size())!=std::string::npos)
      throw std::runtime_error("Pinned encrypted object layout changed");
    objects[n]=original.substr(at,end+6-at)+"\n";
  }
  return objects;
}
std::string Xref(const std::array<size_t,9>& offsets,const std::string& id,const std::string& root,bool linearized) {
  const int size=linearized?9:8;
  std::string out="xref\n0 "+std::to_string(size)+"\n0000000000 65535 f \n";
  for(int n=1;n<size;++n)out+=Ten(offsets[n])+" 00000 n \n";
  return out+"trailer << /Size "+std::to_string(size)+" /Encrypt 7 0 R "+id+" "+root+" >>\n";
}
Bytes RecoveryFixture(const Spec& spec) {
  // Copy the pinned ciphertext objects without altering their numbers, IDs,
  // strings or stream bytes. Relocation changes only xref offsets; object
  // encryption keys remain exactly those of the published fixture.
  const auto input=Original(spec.revision);const std::string original(input.begin(),input.end());
  const auto objects=OpaqueObjects(original);const auto id=Identifier(original);
  std::array<size_t,9> offsets{};
  const std::string repair=spec.recover?"/Root 1 0 R":spec.root;
  if(!spec.linearized) {
    std::string pdf="%PDF-1.7\n";
    for(int n=1;n<=7;++n){offsets[n]=pdf.size();pdf+=objects[n];}
    const auto xref=pdf.size();pdf+=Xref(offsets,id,spec.root,false);
    // The selected xref's trailer is broken. The later, unselected trailer
    // supplies the correct root only when the parser physically rebuilds.
    pdf+="trailer << /Size 8 /Encrypt 7 0 R "+id+" "+repair+" >>\nstartxref\n"+std::to_string(xref)+"\n%%EOF\n";
    return {pdf.begin(),pdf.end()};
  }
  // A real linearized first-page table exercises StartLinearizedParse rather
  // than the ordinary parser. N=1 needs no hint table; E includes every opaque
  // first-page object. Fixed-width header numbers preserve all computed offsets.
  std::string header="%PDF-1.7\n8 0 obj\n<< /Linearized 1 /L 0000000000 /O 3 /P 0 /N 1 /E 0000000000 /T 0000000000 >>\nendobj\n";
  offsets[8]=9;const auto first=header.size();const auto first_size=Xref(offsets,id,spec.root,true).size();
  std::string body;
  for(int n=1;n<=7;++n){offsets[n]=header.size()+first_size+body.size();body+=objects[n];}
  std::string pdf=header+Xref(offsets,id,spec.root,true)+body;const auto main=pdf.size();
  pdf+=Xref(offsets,id,repair,true)+"startxref\n"+std::to_string(first)+"\n%%EOF\n";
  for(const auto& field:std::array<std::pair<std::string,size_t>,3>{{{"/L ",pdf.size()},{"/E ",main},{"/T ",main+9}}}) {
    const auto at=pdf.find(field.first);if(at==std::string::npos)throw std::runtime_error("Missing synthetic header field");
    pdf.replace(at+field.first.size(),10,Ten(field.second));
  }
  return {pdf.begin(),pdf.end()};
}
struct CompleteSource:FX_FILEAVAIL,FX_DOWNLOADHINTS {
  explicit CompleteSource(const Bytes& input):bytes(input) {
    FX_FILEAVAIL::version=FX_DOWNLOADHINTS::version=1;FX_FILEAVAIL::IsDataAvail=Has;FX_DOWNLOADHINTS::AddSegment=Hint;
    access.m_FileLen=static_cast<unsigned long>(bytes.size());access.m_Param=this;access.m_GetBlock=Read;
  }
  static FPDF_BOOL Has(FX_FILEAVAIL* opaque,size_t offset,size_t count) noexcept {
    const auto& s=*static_cast<CompleteSource*>(opaque);return offset<=s.bytes.size()&&count<=s.bytes.size()-offset;
  }
  static void Hint(FX_DOWNLOADHINTS*,size_t,size_t) noexcept {}
  static int Read(void* opaque,unsigned long offset,unsigned char* out,unsigned long count) noexcept {
    auto& s=*static_cast<CompleteSource*>(opaque);++s.reads;s.valid&=out&&count&&offset<=s.bytes.size()&&count<=s.bytes.size()-offset;
    if(!s.valid)return 0;
    std::copy_n(s.bytes.data()+offset,count,out);return 1;
  }
  const Bytes& bytes;FPDF_FILEACCESS access{};size_t reads=0;bool valid=true;
};
void Inspect(const Api& api,FPDF_DOCUMENT doc,int revision,bool owner) {
  ASSERT_EQ(api.FPDF_GetPageCount(doc),1);EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(doc),revision);
  const uint32_t permissions=revision==2?0xffffffc0u:revision==3?0xfffff0c0u:0xfffffffcu;
  EXPECT_EQ(api.FPDF_GetDocUserPermissions(doc),permissions);
  EXPECT_EQ(api.FPDF_GetDocPermissions(doc),owner?0xfffffffcu:permissions);
  Handle page(api.FPDF_LoadPage(doc,0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
  Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
  EXPECT_EQ(text_fixture::Extract(api,text.get()),u"Hello, world!\r\nGoodbye, world!");
}
Raster Image(const Api& api,FPDF_DOCUMENT doc) {
  Handle page(api.FPDF_LoadPage(doc,0),api.FPDF_ClosePage);if(!page.get()){ADD_FAILURE()<<"Missing encrypted page";return {};}
  return Render(api,page.get(),200,200);
}
void AddMark(const Api& api,FPDF_DOCUMENT doc) {
  Handle page(api.FPDF_LoadPage(doc,0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
  auto mark=api.FPDFPageObj_CreateNewRect(150,150,20,20);ASSERT_NE(mark,nullptr);
  ASSERT_TRUE(api.FPDFPageObj_SetFillColor(mark,0,255,0,255));
  ASSERT_TRUE(api.FPDFPath_SetDrawMode(mark,FPDF_FILLMODE_WINDING,0));
  api.FPDFPage_InsertObject(page.get(),mark);ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
}
void Mark(const Raster& image) {
  ASSERT_EQ(image.width,200);ASSERT_EQ(image.height,200);const auto at=(40u*200+160)*4;
  EXPECT_EQ(image.pixels.at(at),0);EXPECT_EQ(image.pixels.at(at+1),255);EXPECT_EQ(image.pixels.at(at+2),0);EXPECT_EQ(image.pixels.at(at+3),255);
}
void Scenario(const Spec& spec) {
  ScopedPinnedTestFonts fonts(/*enumerate_xfa=*/true);const auto bytes=RecoveryFixture(spec),healthy=Original(spec.revision);
  const char* password=spec.owner?kOwner:kUser;Artifacts artifacts;artifacts.Pdf("encrypted-root-recovery",bytes);
  std::array<Raster,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());const auto& api=writer->api();CompleteSource source(bytes);
    Handle avail(spec.linearized?api.FPDFAvail_Create(&source,&source.access):static_cast<FPDF_AVAIL>(nullptr),api.FPDFAvail_Destroy);
    if(spec.linearized) {
      ASSERT_NE(avail.get(),nullptr);EXPECT_EQ(api.FPDFAvail_IsLinearized(avail.get()),PDF_LINEARIZED);
      int status=PDF_DATA_NOTAVAIL;
      for(int poll=0;poll<32&&status==PDF_DATA_NOTAVAIL;++poll)status=api.FPDFAvail_IsDocAvail(avail.get(),&source);
      ASSERT_EQ(status,PDF_DATA_AVAIL);
    }
    // Incorrect credentials must fail before either root-recovery path.
    Handle denied(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),"wrong-root-password"),api.FPDF_CloseDocument);
    EXPECT_EQ(denied.get(),nullptr);EXPECT_EQ(api.FPDF_GetLastError(),static_cast<unsigned long>(FPDF_ERR_PASSWORD));
    Handle control(api.FPDF_LoadMemDocument64(healthy.data(),healthy.size(),password),api.FPDF_CloseDocument);ASSERT_NE(control.get(),nullptr);
    ASSERT_NO_FATAL_FAILURE(Inspect(api,control.get(),spec.revision,spec.owner));
    Handle doc(spec.linearized?api.FPDFAvail_GetDocument(avail.get(),password):api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),password),api.FPDF_CloseDocument);
    if(!spec.recover) {
      EXPECT_EQ(doc.get(),nullptr);EXPECT_EQ(api.FPDF_GetLastError(),static_cast<unsigned long>(FPDF_ERR_FORMAT));
      ASSERT_NO_FATAL_FAILURE(Inspect(api,control.get(),spec.revision,spec.owner));
      originals[backend]=Image(api,control.get());EXPECT_TRUE(source.valid);++backend;continue;
    }
    ASSERT_NE(doc.get(),nullptr)<<api.FPDF_GetLastError();
    if(spec.linearized) {
      int status=PDF_DATA_NOTAVAIL;
      for(int poll=0;poll<32&&status==PDF_DATA_NOTAVAIL;++poll)status=api.FPDFAvail_IsPageAvail(avail.get(),0,&source);
      ASSERT_EQ(status,PDF_DATA_AVAIL);EXPECT_GT(source.reads,0u);
    }
    ASSERT_NO_FATAL_FAILURE(Inspect(api,doc.get(),spec.revision,spec.owner));
    CompareRaster(Image(api,control.get()),Image(api,doc.get()),artifacts,writer->name()+"-original-control");
    const auto lang=Wide(u"en-GB \u03a9");ASSERT_TRUE(api.FPDFCatalog_SetLanguage(doc.get(),lang.data()));
    ASSERT_NO_FATAL_FAILURE(AddMark(api,doc.get()));originals[backend]=Image(api,doc.get());
    ASSERT_NO_FATAL_FAILURE(Mark(originals[backend]));
    for(FPDF_DWORD flags:{FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}) {
      const auto label=writer->name()+"-root-repair-"+std::to_string(flags);
      const auto saved=SaveDocument(api,doc.get(),artifacts,label,flags);ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());const auto& read=reader->api();
        Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),password),read.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);
        ASSERT_NO_FATAL_FAILURE(Inspect(read,reopened.get(),spec.revision,spec.owner));
        ExpectBytes([&](void* out,unsigned long n){return read.FPDFCatalog_GetLanguage(reopened.get(),static_cast<FPDF_WCHAR*>(out),n);},Utf16(u"en-GB \u03a9"));
        const auto image=Image(read,reopened.get());ASSERT_NO_FATAL_FAILURE(Mark(image));
        text_fixture::CompareTextRaster(originals[backend],image,artifacts,label+reader->name());
        Handle bad(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),"wrong-root-password"),read.FPDF_CloseDocument);
        EXPECT_EQ(bad.get(),nullptr);EXPECT_EQ(read.FPDF_GetLastError(),static_cast<unsigned long>(FPDF_ERR_PASSWORD));
      }
    }
    EXPECT_TRUE(source.valid);++backend;
  }
  text_fixture::CompareTextRaster(originals[0],originals[1],artifacts,"encrypted-repair-backends");
}
TEST(CoverageEncryptedRootRecovery,R2MissingRootRebuildsOnlyAfterUserAuthentication){Scenario({2,""});}
TEST(CoverageEncryptedRootRecovery,R2DanglingRootPreservesTheOwnerCredentialAndPermissions){Scenario({2,"/Root 99 0 R",false,true});}
TEST(CoverageEncryptedRootRecovery,R3EncryptionDictionaryAsRootRequiresDocumentInitializationRecovery){Scenario({3,"/Root 7 0 R"});}
TEST(CoverageEncryptedRootRecovery,R3ScalarRootIsReplacedByTheRecoveredCatalog){Scenario({3,"/Root 17",false,true});}
TEST(CoverageEncryptedRootRecovery,R5AesKeyStillDecryptsContentAfterRootRecovery){Scenario({5,"/Root 99 0 R"});}
TEST(CoverageEncryptedRootRecovery,LinearizedR2MissingRootReauthenticatesTheRecoveredTrailer){Scenario({2,"",true});}
TEST(CoverageEncryptedRootRecovery,LinearizedR2NullRootRetainsOwnerAuthentication){Scenario({2,"/Root null",true,true});}
TEST(CoverageEncryptedRootRecovery,LinearizedR3DanglingRootRetainsItsFileIdentifier){Scenario({3,"/Root 99 0 R",true});}
TEST(CoverageEncryptedRootRecovery,LinearizedR3ScalarRootRecoversTheCatalogAndPage){Scenario({3,"/Root 17",true,true});}
TEST(CoverageEncryptedRootRecovery,LinearizedR5AesContentSurvivesSecurityHandlerRecreation){Scenario({5,"",true});}
TEST(CoverageEncryptedRootRecovery,AValidCredentialCannotRecoverAnAbsentCatalog){Scenario({3,"/Root 99 0 R",false,false,false});}
TEST(CoverageEncryptedRootRecovery,LinearizedRecoveryStopsWhenNoCatalogCanBeRecovered){Scenario({3,"/Root 99 0 R",true,false,false});}
}  // namespace
}  // namespace pdfoxium::integration
