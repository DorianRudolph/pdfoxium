#include "document_fixture.h"

#include <iomanip>
#include <sstream>

namespace pdfoxium::integration {
namespace {
using namespace document_test;

struct SignatureFixture {
  std::vector<uint8_t> bytes;
  std::array<int,4> range;
};

SignatureFixture SignedMetadataFixture() {
  // Inspection-only fixture, not a cryptographically signed document. Fixed
  // width ByteRange slots let us record actual file ranges without changing
  // object/xref offsets. Two direct fields plus one missing-value field cover
  // distinct borrowed handles and all optional-metadata failure contracts.
  auto bytes=OnePage("/AcroForm << /Fields [5 0 R 6 0 R 7 0 R] >>",{
    "<< /FT /Sig /T (first) /V 8 0 R >>",
    "<< /FT /Sig /T (second) /V 9 0 R >>",
    "<< /FT /Sig /T (missing) >>",
    "<< /Type /Sig /Contents <308200FF0100> /ByteRange [0000000000 0000000000 0000000000 0000000000] /SubFilter /adbe.pkcs7.detached /Reason "+PdfText(u"Approved \u03a9\U0001f642")+" /M (D:20260203112233+05'30') /Reference [<< /TransformMethod /DocMDP /TransformParams << /P 1 >> >>] >>",
    "<< /Type /Sig /Contents <> /ByteRange [] /SubFilter /ETSI.CAdES.detached /Reason () /M () /Reference [<< /TransformMethod /DocMDP /TransformParams << /P 3 >> >>] >>"});
  std::string text(bytes.begin(),bytes.end());
  const auto contents=text.find("<308200FF0100>");
  const auto end=contents+14;
  std::array<int,4> range={0,static_cast<int>(contents),static_cast<int>(end),static_cast<int>(bytes.size()-end)};
  const auto start=text.find("/ByteRange [")+12;
  std::ostringstream formatted;
  for(size_t i=0;i<range.size();++i){if(i)formatted<<' ';formatted<<std::setfill('0')<<std::setw(10)<<range[i];}
  text.replace(start,43,formatted.str());
  return {{text.begin(),text.end()},range};
}

void CheckSignatureMetadata(const Api& api,FPDF_DOCUMENT document,
                            const std::array<int,4>& expected_range) {
  ASSERT_EQ(api.FPDF_GetSignatureCount(document),3);
  EXPECT_EQ(api.FPDF_GetSignatureObject(document,-1),nullptr);
  EXPECT_EQ(api.FPDF_GetSignatureObject(document,3),nullptr);
  auto first=api.FPDF_GetSignatureObject(document,0);
  auto second=api.FPDF_GetSignatureObject(document,1);
  auto missing=api.FPDF_GetSignatureObject(document,2);
  ASSERT_NE(first,nullptr);ASSERT_NE(second,nullptr);ASSERT_NE(missing,nullptr);
  EXPECT_NE(first,second);
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetContents(first,b,n);},{0x30,0x82,0,0xff,1,0});
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetContents(second,b,n);},{});
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetContents(missing,b,n);},{});
  for(auto cap:{0UL,3UL,4UL,5UL,12UL}) {
    std::array<int,16> output;output.fill(123456);
    EXPECT_EQ(api.FPDFSignatureObj_GetByteRange(first,output.data()+2,cap),4u);
    if(cap>=4){EXPECT_TRUE(std::equal(expected_range.begin(),expected_range.end(),output.begin()+2));}
    else{EXPECT_TRUE(std::all_of(output.begin(),output.end(),[](int x){return x==123456;}));}
    EXPECT_EQ(output[0],123456);EXPECT_EQ(output[1],123456);
    for(size_t i=6;i<output.size();++i)EXPECT_EQ(output[i],123456);
  }
  EXPECT_EQ(api.FPDFSignatureObj_GetByteRange(first,nullptr,0),4u);
  std::array<int,4> sentinel={91,92,93,94};
  EXPECT_EQ(api.FPDFSignatureObj_GetByteRange(missing,sentinel.data(),4),0u);
  EXPECT_EQ(sentinel,(std::array<int,4>{91,92,93,94}));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetSubFilter(first,static_cast<char*>(b),n);},Ascii("adbe.pkcs7.detached"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetSubFilter(second,static_cast<char*>(b),n);},Ascii("ETSI.CAdES.detached"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetReason(first,b,n);},Utf16(u"Approved \u03a9\U0001f642"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetReason(second,b,n);},Utf16(u""));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetTime(first,static_cast<char*>(b),n);},Ascii("D:20260203112233+05'30'"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetTime(second,static_cast<char*>(b),n);},Ascii(""));
  for(auto field:{missing,FPDF_SIGNATURE{}}) {
    ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetContents(field,b,n);},{});
    ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetSubFilter(field,static_cast<char*>(b),n);},{});
    ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetReason(field,b,n);},{});
    ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetTime(field,static_cast<char*>(b),n);},{});
    EXPECT_EQ(api.FPDFSignatureObj_GetDocMDPPermission(field),0u);
  }
  EXPECT_EQ(api.FPDFSignatureObj_GetDocMDPPermission(first),1u);
  EXPECT_EQ(api.FPDFSignatureObj_GetDocMDPPermission(second),3u);
}

TEST(Signatures, BinaryMetadataBuffersAndBorrowedHandles) {
  Artifacts artifacts;
  const auto fixture=SignedMetadataFixture();
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());const auto& api=library->api();
    Handle doc(api.FPDF_LoadMemDocument64(fixture.bytes.data(),fixture.bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(),nullptr);
    ASSERT_NO_FATAL_FAILURE(CheckSignatureMetadata(api,doc.get(),fixture.range));
    // Saving invalidates cryptographic digests by definition. We compare only
    // the unchanged metadata dictionaries and their renderable page content.
    CrossRead(*library,doc.get(),artifacts,"signature-metadata",[&](const Api& read,FPDF_DOCUMENT loaded){CheckSignatureMetadata(read,loaded,fixture.range);});
  }
}

TEST(Signatures, PermissionValuesMissingAndInvalidParameters) {
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    EXPECT_EQ(api.FPDF_GetSignatureCount(nullptr),-1);
    EXPECT_EQ(api.FPDF_GetSignatureObject(nullptr,0),nullptr);
    auto empty=OnePage();
    Handle no_signatures(api.FPDF_LoadMemDocument64(empty.data(),empty.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(no_signatures.get(),nullptr);EXPECT_EQ(api.FPDF_GetSignatureCount(no_signatures.get()),0);
    for(const std::string params:{"/P 1","/P 2","/P 3","/P 0","/P 4",""}) {
      const auto bytes=OnePage("/AcroForm << /Fields [5 0 R] >>",{
        "<< /FT /Sig /V << /Reference [<< /TransformMethod /Other /TransformParams << /P 1 >> >> << /TransformMethod /DocMDP /TransformParams << "+params+" >> >>] >> >>"});
      Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
      auto signature=api.FPDF_GetSignatureObject(doc.get(),0);ASSERT_NE(signature,nullptr);
      const unsigned int expected=params.empty()?2u:params.back()-'0';
      EXPECT_EQ(api.FPDFSignatureObj_GetDocMDPPermission(signature),expected>=1&&expected<=3?expected:0u);
    }
  }
}

TEST(Signatures, NestedFieldTreesRemainDiscoverableWithoutNativeBugCompatibility) {
  // A terminal signature field may be below a nonterminal field. Native's
  // pinned inspection API only scans Fields directly, a known limitation;
  // pdfoxium should enumerate valid terminal signatures recursively instead.
  const auto bytes=OnePage("/AcroForm << /Fields [5 0 R] >>",{
    "<< /T (parent) /Kids [6 0 R] >>",
    "<< /Parent 5 0 R /FT /Sig /T (signed) /V << /Type /Sig /Contents <010002> >> >>"});
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    if(library->name()=="native") {EXPECT_EQ(api.FPDF_GetSignatureCount(doc.get()),0);}
    else {
      ASSERT_EQ(api.FPDF_GetSignatureCount(doc.get()),1);
      auto signature=api.FPDF_GetSignatureObject(doc.get(),0);ASSERT_NE(signature,nullptr);
      ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetContents(signature,b,n);},{1,0,2});
    }
  }
  // A signature's visual widget is not a second terminal field. FT and V may
  // be inherited by the widget, while inspection returns the owner only once.
  const auto widget_bytes=OnePage("/AcroForm << /Fields [5 0 R] >>",{
    "<< /T (parent) /FT /Sig /V << /Contents <010002> >> /Kids [6 0 R] >>",
    "<< /Type /Annot /Subtype /Widget /Parent 5 0 R /Rect [0 0 20 20] /P 3 0 R >>"});
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    Handle doc(api.FPDF_LoadMemDocument64(widget_bytes.data(),widget_bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    ASSERT_EQ(api.FPDF_GetSignatureCount(doc.get()),1);
    auto signature=api.FPDF_GetSignatureObject(doc.get(),0);ASSERT_NE(signature,nullptr);
    ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetContents(signature,b,n);},{1,0,2});
  }
  const auto cycle=OnePage("/AcroForm << /Fields [5 0 R] >>",{
    "<< /T (cycle) /Kids [6 0 R] >>","<< /Parent 5 0 R /Kids [5 0 R] >>"});
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    Handle doc(api.FPDF_LoadMemDocument64(cycle.data(),cycle.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    EXPECT_EQ(api.FPDF_GetSignatureCount(doc.get()),0);EXPECT_EQ(api.FPDF_GetSignatureObject(doc.get(),0),nullptr);
  }
  const auto inherited=OnePage("/AcroForm << /Fields [5 0 R] >>",{
    "<< /T (group) /FT /Sig /Kids [6 0 R] >>",
    "<< /Parent 5 0 R /T (terminal) /V << /Contents <030004> >> >>"});
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    Handle doc(api.FPDF_LoadMemDocument64(inherited.data(),inherited.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    ASSERT_EQ(api.FPDF_GetSignatureCount(doc.get()),1);
    auto signature=api.FPDF_GetSignatureObject(doc.get(),0);ASSERT_NE(signature,nullptr);
    if(library->name()=="native") {
      // Native identifies the nonterminal parent by its explicit FT and misses
      // the terminal field's value instead of applying field inheritance.
      ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetContents(signature,b,n);},{});
    } else {
      ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetContents(signature,b,n);},{3,0,4});
    }
  }
}

TEST(Signatures, AnonymousInvisibleTerminalFieldsAreNotMistakenForWidgets) {
  // /T and a widget annotation are optional. A terminal field with explicit
  // /FT and /V remains a signature field when neither entry is present.
  const auto bytes=OnePage("/AcroForm << /Fields [5 0 R] >>",{
    "<< /T (parent) /Kids [6 0 R 7 0 R] >>",
    "<< /Parent 5 0 R /FT /Sig /V << /Contents <010002> >> >>",
    "<< /Parent 5 0 R /FT /Sig /V << /Contents <030004> >> >>"});
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());const auto& api=library->api();
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(document.get(),nullptr);
    if(library->name()=="native") {
      // The pinned native wrapper visits top-level Fields only.
      EXPECT_EQ(api.FPDF_GetSignatureCount(document.get()),0);
      EXPECT_EQ(api.FPDF_GetSignatureObject(document.get(),0),nullptr);
      continue;
    }
    ASSERT_EQ(api.FPDF_GetSignatureCount(document.get()),2);
    for(int index=0;index<2;++index) {
      auto signature=api.FPDF_GetSignatureObject(document.get(),index);
      ASSERT_NE(signature,nullptr);
      ExpectBytes([&](void* b,unsigned long n){return api.FPDFSignatureObj_GetContents(signature,b,n);},
                  index==0?std::vector<uint8_t>{1,0,2}:std::vector<uint8_t>{3,0,4});
    }
    EXPECT_EQ(api.FPDF_GetSignatureObject(document.get(),-1),nullptr);
    EXPECT_EQ(api.FPDF_GetSignatureObject(document.get(),2),nullptr);
  }
}

}  // namespace
}  // namespace pdfoxium::integration
