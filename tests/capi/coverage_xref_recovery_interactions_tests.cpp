// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#include "document_fixture.h"
#include "runtime_fixture.h"

#include <array>
#include <iomanip>
#include <map>
#include <sstream>

namespace pdfoxium::integration {
namespace {
using Bytes = std::vector<uint8_t>;
using RGB = std::array<int, 3>;
using namespace document_test;
const RGB initial{64,128,192}, red{255,0,0}, green{0,255,0}, blue{0,0,255};
std::string Decimal(size_t n) { std::ostringstream s; s << std::setfill('0') << std::setw(10) << n; return s.str(); }
std::string Paint(RGB color) {
  std::ostringstream s;
  for(int c:color)s << std::setprecision(12) << double(c)/255 << ' ';
  s << "rg 16 16 64 48 re f\n";return s.str();
}
struct Revision { size_t offset=0, prev_field=0; };
struct Pdf {
  std::string data="%PDF-1.7\n";
  std::map<int,size_t> objects;
  int next_stream=10, revision=0;
  RGB expected=initial;
  void Object(int number,const std::string& body) {
    objects[number]=data.size();data+=std::to_string(number)+" 0 obj\n"+body+"\nendobj\n";
  }
  Pdf() {
    Object(1,"<< /Type /Catalog /Pages 2 0 R >>");
    Object(2,"<< /Type /Pages /Count 1 /Kids [3 0 R] >>");
    Object(3,"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents 4 0 R >>");
    Update(initial);
  }
  void Update(RGB color) {
    expected=color;++revision;
    Object(4,PdfStream("",Paint(color)));
    Object(5,"<< /Title (Revision"+std::to_string(revision)+") >>");
  }
  Revision Classic(size_t prev=0,bool root=true,size_t hybrid=0) {
    Revision r{data.size(),0};const int size=objects.rbegin()->first+1;
    data+="xref\n0 "+std::to_string(size)+"\n";
    for(int n=0;n<size;++n) {
      if(n==0)data+="0000000000 65535 f \n";
      else if(objects.count(n))data+=Decimal(objects.at(n))+" 00000 n \n";
      else data+="0000000000 00000 f \n";
    }
    data+="trailer << /Size "+std::to_string(size)+" /Info 5 0 R ";
    if(root)data+="/Root 1 0 R ";
    if(hybrid)data+="/XRefStm "+std::to_string(hybrid)+" ";
    data+="/Prev ";r.prev_field=data.size();data+=Decimal(prev)+" >>\n";
    data+="startxref\n"+std::to_string(r.offset)+"\n%%EOF\n";return r;
  }
  Revision Stream(size_t prev=0,bool sparse=false,bool malformed=false) {
    const int number=next_stream++;Revision r{data.size(),0};objects[number]=r.offset;
    const int size=objects.rbegin()->first+1;
    std::string entries;
    auto entry=[&](int n) {
      const bool normal=n && objects.count(n);entries+=char(normal?1:0);
      const uint32_t pos=normal?objects.at(n):0;
      for(int shift:{24,16,8,0})entries+=char(pos>>shift);
      entries+=char(n==0?255:0);entries+=char(n==0?255:0);
    };
    if(sparse){entry(4);entry(5);entry(number);}else for(int n=0;n<size;++n)entry(n);
    std::string dict="/Type /XRef /Size "+std::to_string(size)+" /Root 1 0 R /Info 5 0 R /W "+
      (malformed?"[1 4 -2]":"[1 4 2]");
    if(sparse)dict+=" /Index [4 2 "+std::to_string(number)+" 1]";
    dict+=" /Prev ";const auto prefix=std::to_string(number)+" 0 obj\n<< ";
    r.prev_field=data.size()+prefix.size()+dict.size();dict+=Decimal(prev);
    data+=std::to_string(number)+" 0 obj\n"+PdfStream(dict,entries)+"\nendobj\n";
    data+="startxref\n"+std::to_string(r.offset)+"\n%%EOF\n";return r;
  }
  void Redirect(Revision r,size_t prev) {
    // Fixed-width decimal offsets preserve every object, stream and xref byte
    // position. PdfStream emits dictionary arguments immediately after '<< '.
    if(data.substr(r.prev_field,10).find_first_not_of("0123456789")!=std::string::npos)
      throw std::runtime_error("Cross-reference previous-offset location changed");
    data.replace(r.prev_field,10,Decimal(prev));
  }
  Bytes BytesValue()const{return {data.begin(),data.end()};}
};
Raster Image(const Api& api,FPDF_DOCUMENT doc) {
  Handle page(api.FPDF_LoadPage(doc,0),api.FPDF_ClosePage);
  if(!page.get()){ADD_FAILURE()<<"Expected the retained revision page";return {};}
  EXPECT_FLOAT_EQ(api.FPDF_GetPageWidthF(page.get()),128);EXPECT_FLOAT_EQ(api.FPDF_GetPageHeightF(page.get()),96);
  return Render(api,page.get(),128,96);
}
void Color(const Raster& image,RGB color) {
  ASSERT_EQ(image.width,128);ASSERT_EQ(image.height,96);ASSERT_EQ(image.pixels.size(),128u*96*4);
  for(int y:{34,48,68})for(int x:{20,40,72}) {
    const size_t at=(y*128+x)*4;
    for(int c=0;c<3;++c)EXPECT_EQ(image.pixels[at+c],color[2-c])<<x<<','<<y;
    EXPECT_EQ(image.pixels[at+3],255);
  }
  for(int y:{4,90})for(int x:{4,120})for(int c=0;c<4;++c)EXPECT_EQ(image.pixels[(y*128+x)*4+c],255);
}
void Inspect(const Api& api,FPDF_DOCUMENT doc,const Pdf& pdf) {
  EXPECT_EQ(api.FPDF_GetPageCount(doc),1);
  const auto title="Revision"+std::to_string(pdf.revision);
  ExpectBytes([&](void* out,unsigned long n){return api.FPDF_GetMetaText(doc,"Title",out,n);},Utf16(std::u16string(title.begin(),title.end())));
}
struct CustomRead {
  explicit CustomRead(Bytes source):bytes(std::move(source)) {
    access.m_FileLen=static_cast<unsigned long>(bytes.size());access.m_Param=this;access.m_GetBlock=Read;
  }
  static int Read(void* opaque,unsigned long offset,unsigned char* out,unsigned long count) noexcept {
    auto& s=*static_cast<CustomRead*>(opaque);++s.reads;
    s.valid &= out && count && offset<=s.bytes.size() && count<=s.bytes.size()-offset;
    if(!s.valid)return 0;
    if(s.reject){++s.rejected;return 0;}
    std::copy_n(s.bytes.data()+offset,count,out);return 1;
  }
  Bytes bytes;FPDF_FILEACCESS access{};bool reject=false,valid=true;size_t reads=0,rejected=0;
};
struct RangeRead : FX_FILEAVAIL, FX_DOWNLOADHINTS {
  explicit RangeRead(Bytes input):bytes(std::move(input)),present(bytes.size(),false) {
    FX_FILEAVAIL::version=FX_DOWNLOADHINTS::version=1;FX_FILEAVAIL::IsDataAvail=Has;FX_DOWNLOADHINTS::AddSegment=Hint;
    access.m_FileLen=static_cast<unsigned long>(bytes.size());access.m_Param=this;access.m_GetBlock=Read;
  }
  static FPDF_BOOL Has(FX_FILEAVAIL* opaque,size_t offset,size_t count) noexcept {
    auto& s=*static_cast<RangeRead*>(opaque);
    return offset<=s.bytes.size()&&count<=s.bytes.size()-offset&&std::all_of(s.present.begin()+offset,s.present.begin()+offset+count,[](bool v){return v;});
  }
  static void Hint(FX_DOWNLOADHINTS* opaque,size_t offset,size_t count) noexcept {
    auto& s=*static_cast<RangeRead*>(opaque);try{s.requests.emplace_back(offset,count);}catch(...){s.valid=false;}
  }
  static int Read(void* opaque,unsigned long offset,unsigned char* out,unsigned long count) noexcept {
    auto& s=*static_cast<RangeRead*>(opaque);s.valid&=out&&count&&offset<=s.bytes.size()&&count<=s.bytes.size()-offset;
    if(!s.valid||!Has(&s,offset,count))return 0;
    std::copy_n(s.bytes.data()+offset,count,out);++s.reads;return 1;
  }
  void Deliver() {
    for(const auto& request:requests) {
      size_t begin=std::min(request.first,bytes.size()),end=begin+std::min(request.second,bytes.size()-begin);
      while(begin<end&&present[begin])++begin;
      if(begin<end){std::fill(present.begin()+begin,present.begin()+std::min(end,begin+97),true);++deliveries;}
    }
    requests.clear();
  }
  Bytes bytes;std::vector<bool> present;FPDF_FILEACCESS access{};std::vector<std::pair<size_t,size_t>> requests;
  size_t reads=0,deliveries=0;bool valid=true;
};
void Workflow(const Pdf& pdf,bool custom=false,bool fail_first=false,bool progressive=false) {
  Artifacts artifacts;const auto bytes=pdf.BytesValue();artifacts.Pdf("xref-interactions",bytes);
  std::array<Raster,2> originals;size_t backend=0;
  for(const auto* writer:Implementations()) {
    SCOPED_TRACE(writer->name());const auto& api=writer->api();CustomRead source(bytes);
    if(fail_first) {
      source.reject=true;Handle rejected(api.FPDF_LoadCustomDocument(&source.access,nullptr),api.FPDF_CloseDocument);
      ASSERT_EQ(rejected.get(),nullptr);EXPECT_GT(source.rejected,0u);source.reject=false;
    }
    RangeRead ranges(bytes);Handle provider(static_cast<FPDF_AVAIL>(nullptr),api.FPDFAvail_Destroy);FPDF_DOCUMENT raw=nullptr;
    if(progressive) {
      provider=Handle(api.FPDFAvail_Create(&ranges,&ranges.access),api.FPDFAvail_Destroy);ASSERT_NE(provider.get(),nullptr);
      int state=PDF_DATA_NOTAVAIL;
      for(int poll=0;poll<256&&state==PDF_DATA_NOTAVAIL;++poll){state=api.FPDFAvail_IsDocAvail(provider.get(),&ranges);ranges.Deliver();}
      ASSERT_EQ(state,PDF_DATA_AVAIL);EXPECT_EQ(api.FPDFAvail_IsLinearized(provider.get()),PDF_NOT_LINEARIZED);
      raw=api.FPDFAvail_GetDocument(provider.get(),nullptr);ASSERT_NE(raw,nullptr);
      state=PDF_DATA_NOTAVAIL;
      for(int poll=0;poll<256&&state==PDF_DATA_NOTAVAIL;++poll){state=api.FPDFAvail_IsPageAvail(provider.get(),0,&ranges);ranges.Deliver();}
      ASSERT_EQ(state,PDF_DATA_AVAIL);EXPECT_GT(ranges.deliveries,1u);EXPECT_GT(ranges.reads,0u);EXPECT_TRUE(ranges.valid);
    }else raw=custom?api.FPDF_LoadCustomDocument(&source.access,nullptr):api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr);
    Handle doc(raw,api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(),nullptr)<<api.FPDF_GetLastError();
    ASSERT_NO_FATAL_FAILURE(Inspect(api,doc.get(),pdf));
    originals[backend]=Image(api,doc.get());
    ASSERT_NO_FATAL_FAILURE(Color(originals[backend],pdf.expected));
    EXPECT_TRUE(source.valid);
    const auto language=Wide(u"pl-PL");ASSERT_TRUE(api.FPDFCatalog_SetLanguage(doc.get(),language.data()));
    for(FPDF_DWORD flags:{FPDF_NO_INCREMENTAL,FPDF_INCREMENTAL}) {
      const auto label=writer->name()+"-xref-"+std::to_string(flags);
      const auto saved=SaveDocument(api,doc.get(),artifacts,label,flags);ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        SCOPED_TRACE(reader->name());const auto& read=reader->api();
        Handle reload(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reload.get(),nullptr);
        ASSERT_NO_FATAL_FAILURE(Inspect(read,reload.get(),pdf));
        ExpectBytes([&](void* out,unsigned long n){return read.FPDFCatalog_GetLanguage(reload.get(),static_cast<FPDF_WCHAR*>(out),n);},Utf16(u"pl-PL"));
        const auto image=Image(read,reload.get());
    ASSERT_NO_FATAL_FAILURE(Color(image,pdf.expected));CompareRaster(originals[backend],image,artifacts,label+reader->name());
      }
    }
    EXPECT_TRUE(source.valid);EXPECT_TRUE(ranges.valid);
    ++backend;
  }
  CompareRaster(originals[0],originals[1],artifacts,"xref-native-rust");
}
Pdf Chain(std::initializer_list<bool> streams,bool sparse=false) {
  Pdf pdf;size_t previous=0;int revision=0;
  for(bool stream:streams) {
    if(revision)pdf.Update(revision%2?red:green);
    const auto next=stream?pdf.Stream(previous,sparse&&revision>0):pdf.Classic(previous);
    previous=next.offset;++revision;
  }
  return pdf;
}
TEST(CoverageXrefRecoveryInteractions, ClassicThenStreamRetainsTheNewContentAndMetadata) {Workflow(Chain({false,true}));}
TEST(CoverageXrefRecoveryInteractions, StreamThenClassicRetainsTheNewContentAndMetadata) {Workflow(Chain({true,false}));}
TEST(CoverageXrefRecoveryInteractions, ClassicStreamClassicResolvesAllThreeRevisions) {Workflow(Chain({false,true,false}));}
TEST(CoverageXrefRecoveryInteractions, StreamClassicStreamResolvesAllThreeRevisions) {Workflow(Chain({true,false,true}));}
TEST(CoverageXrefRecoveryInteractions, SparseStreamUpdatesRetainTheOlderPageTree) {Workflow(Chain({false,true},true));}
TEST(CoverageXrefRecoveryInteractions, SparseStreamAfterAClassicUpdateRetainsTheNewestObjects) {Workflow(Chain({true,false,true},true));}
TEST(CoverageXrefRecoveryInteractions, AlternatingFourRevisionChainCanBeLoadedFromCustomReads) {Workflow(Chain({false,true,false,true}),true);}
TEST(CoverageXrefRecoveryInteractions, StreamSelfCycleIsBoundedAndRecoversTheCurrentRevision) {
  Pdf p;p.Classic();p.Update(red);const auto at=p.data.size();p.Stream(at);Workflow(p);
}
TEST(CoverageXrefRecoveryInteractions, TwoStreamPreviousOffsetsCannotCycleIndefinitely) {
  Pdf p;const auto a=p.Stream();p.Update(green);const auto b=p.Stream(a.offset);p.Redirect(a,b.offset);Workflow(p);
}
TEST(CoverageXrefRecoveryInteractions, MixedClassicAndStreamCycleRecoversTheLastWrittenObjects) {
  Pdf p;const auto a=p.Classic();p.Update(blue);const auto b=p.Stream(a.offset);p.Redirect(a,b.offset);Workflow(p);
}
TEST(CoverageXrefRecoveryInteractions, TwoClassicPreviousOffsetsRecoverWithoutLosingTheNewestMetadata) {
  Pdf p;const auto a=p.Classic();p.Update(red);const auto b=p.Classic(a.offset);p.Redirect(a,b.offset);Workflow(p);
}
TEST(CoverageXrefRecoveryInteractions, AHybridTrailerCombinesItsSupplementaryStreamWithThePreviousRevision) {
  Pdf p;const auto old=p.Classic();p.Update(green);const auto stream=p.Stream(old.offset,true);p.Classic(old.offset,true,stream.offset);Workflow(p);
}
TEST(CoverageXrefRecoveryInteractions, AnInvalidAncestorStreamDoesNotHideTheLastClassicRevision) {
  Pdf p;const auto old=p.Stream(0,false,true);p.Update(blue);p.Classic(old.offset);Workflow(p);
}
TEST(CoverageXrefRecoveryInteractions, AnInvalidAncestorStreamDoesNotHideTheLastValidStreamRevision) {
  Pdf p;const auto old=p.Stream(0,false,true);p.Update(red);p.Stream(old.offset);Workflow(p);
}
TEST(CoverageXrefRecoveryInteractions, AFailedCustomReadCanBeRetriedWithTheSameCallbackOwner) {Workflow(Chain({false,true,false}),true,true);}
TEST(CoverageXrefRecoveryInteractions, AFailedCustomReadDoesNotPoisonAnIndependentStreamFirstDocument) {Workflow(Chain({true,false,true},true),true,true);}
TEST(CoverageXrefRecoveryInteractions, MixedCrossReferencesBecomeAvailableInBoundedSmallDeliveries) {Workflow(Chain({false,true,false}),false,false,true);}
TEST(CoverageXrefRecoveryInteractions, SparseStreamPageResourcesSurviveStaggeredWholeFileAvailability) {Workflow(Chain({true,false,true},true),false,false,true);}
}  // namespace
}  // namespace pdfoxium::integration
