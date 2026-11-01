#include "document_fixture.h"

#include <set>

namespace pdfoxium::integration {
namespace {
using namespace document_test;

std::vector<uint8_t> TaggedFixture() {
  // ParentTree maps MCID slots to their owning elements. The first top-level
  // element mixes local/remote MCRs, a child element and an OBJR. Public APIs
  // cannot construct any of these tagged-structure dictionaries.
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> /StructTreeRoot 7 0 R >>",
    "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>",
    "<< /Type /Page /Parent 2 0 R /StructParents 0 /MediaBox [0 0 96 96] /Resources << /Font << /F1 22 0 R >> >> /Contents 4 0 R >>",
    PdfStream("","0.2 0.4 0.6 rg 8 8 80 24 re f BT /F1 12 Tf 10 60 Td /Span << /MCID 0 /ActualText "+PdfText(u"actual \u03a9\U0001f642")+" >> BDC (Visible) Tj EMC ET"),
    "<< /Type /Page /Parent 2 0 R /StructParents 1 /MediaBox [0 0 96 96] /Resources << >> /Contents 6 0 R >>",
    PdfStream("","0.6 0.4 0.2 rg 8 8 48 72 re f"),
    "<< /Type /StructTreeRoot /K [8 0 R 12 0 R] /ParentTree 13 0 R /RoleMap << /Custom /P /Chain /Custom /Loop /Loop >> >>",
    "<< /Type /StructElem /S /Custom /P 7 0 R /Pg 3 0 R /Alt "+PdfText(u"alternate \u03a9\U0001f642")+" /ActualText "+PdfText(u"actual \u03a9\U0001f642")+" /E "+PdfText(u"expanded \u03a9")+" /T "+PdfText(u"title \u03a9")+" /ID "+PdfText(u"id-\u03a9")+" /Lang (zh-Hant) /A [14 0 R 15 0 R 2 << >>] /K [0 9 0 R << /Type /MCR /Pg 3 0 R /MCID 2 >> << /Type /OBJR /Pg 3 0 R /Obj 16 0 R >> << /Type /MCR /Pg 5 0 R /MCID 4 >>] >>",
    "<< /Type /StructElem /S /Span /P 8 0 R /Pg 3 0 R /K 1 /Alt () /ActualText () /E () /T () /ID () /Lang () /A << >> >>",
    "<< /Type /StructElem /S /P /P 12 0 R /Pg 5 0 R /K << /Type /MCR /Pg 5 0 R /MCID 3 >> /A 15 0 R >>",
    "<< /Type /Alternative /S /Chain /P 12 0 R /Pg 5 0 R /A [] >>",
    "<< /Type /StructElem /S /Sect /P 7 0 R /Pg 5 0 R /K [10 0 R 11 0 R] >>",
    "<< /Nums [0 [8 0 R 9 0 R 8 0 R] 1 [null null null 10 0 R 8 0 R 11 0 R]] >>",
    "<< /O /Layout /True true /False false /Zero 0 /Negative -2 /Real 1.25 /Text (ASCII) /Unicode <CEA9F09F9982> /PdfUnicode "+PdfText(u"\u03a9\U0001f642")+" /Name /NameValue /Blob <4100FF42> /Empty () /EmptyArray [] /Array [true -7 (x) /Named << /Key 1 >> null [2 3] 17 0 R] /Dict << >> /Nil null /Indirect 18 0 R /Stream 19 0 R /K#E9 (escaped) >>",
    "<< /O /PrintField /Text (second) /Name /Second >>",
    "<< /Type /Annot /Subtype /Square /Rect [1 1 10 10] >>",
    "3.5",
    "true",
    PdfStream("/Type /Metadata","data"),
    "null",
    "null",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"});
}

void CheckElementStrings(const Api& api,FPDF_STRUCTELEMENT element) {
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetAltText(element,b,n);},Utf16(u"alternate \u03a9\U0001f642"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetActualText(element,b,n);},Utf16(u"actual \u03a9\U0001f642"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetExpansion(element,b,n);},Utf16(u"expanded \u03a9"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetTitle(element,b,n);},Utf16(u"title \u03a9"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetID(element,b,n);},Utf16(u"id-\u03a9"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetLang(element,b,n);},Utf16(u"zh-Hant"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetRawType(element,static_cast<FPDF_WCHAR*>(b),n);},Utf16(u"Custom"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetType(element,b,n);},Utf16(u"P"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetObjType(element,b,n);},Utf16(u"StructElem"));
}

void CheckStructureHierarchy(const Api& api,FPDF_DOCUMENT document) {
  Handle first(api.FPDF_LoadPage(document,0),api.FPDF_ClosePage);Handle second(api.FPDF_LoadPage(document,1),api.FPDF_ClosePage);
  ASSERT_NE(first.get(),nullptr);ASSERT_NE(second.get(),nullptr);
  Handle tree(api.FPDF_StructTree_GetForPage(first.get()),api.FPDF_StructTree_Close);
  Handle independent(api.FPDF_StructTree_GetForPage(first.get()),api.FPDF_StructTree_Close);
  Handle other(api.FPDF_StructTree_GetForPage(second.get()),api.FPDF_StructTree_Close);
  ASSERT_NE(tree.get(),nullptr);ASSERT_NE(independent.get(),nullptr);ASSERT_NE(other.get(),nullptr);
  ASSERT_EQ(api.FPDF_StructTree_CountChildren(tree.get()),2);
  auto element=api.FPDF_StructTree_GetChildAtIndex(tree.get(),0);ASSERT_NE(element,nullptr);
  EXPECT_EQ(api.FPDF_StructTree_GetChildAtIndex(tree.get(),1),nullptr); // page-scoped slot
  for(int index:{-1,2,10000})EXPECT_EQ(api.FPDF_StructTree_GetChildAtIndex(tree.get(),index),nullptr);
  ASSERT_NO_FATAL_FAILURE(CheckElementStrings(api,element));
  EXPECT_EQ(api.FPDF_StructElement_GetParent(element),nullptr);
  ASSERT_EQ(api.FPDF_StructElement_CountChildren(element),5);
  auto child=api.FPDF_StructElement_GetChildAtIndex(element,1);ASSERT_NE(child,nullptr);
  EXPECT_EQ(api.FPDF_StructElement_GetParent(child),element);
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetType(child,b,n);},Utf16(u"Span"));
  for(int index:{0,2,3,4,-1,5,10000})EXPECT_EQ(api.FPDF_StructElement_GetChildAtIndex(element,index),nullptr);
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentID(child),1);
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentID(element),-1);
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdCount(child),1);
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(child,0),1);
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(child,-1),-1);
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(child,1),-1);
  const std::array<int,5> local={0,-1,2,-1,-1};
  for(int index=0;index<5;++index)EXPECT_EQ(api.FPDF_StructElement_GetChildMarkedContentID(element,index),local[index]);
  EXPECT_EQ(api.FPDF_StructElement_GetChildMarkedContentID(element,-1),-1);EXPECT_EQ(api.FPDF_StructElement_GetChildMarkedContentID(element,5),-1);
  for(auto get:{api.FPDF_StructElement_GetAltText,api.FPDF_StructElement_GetActualText,api.FPDF_StructElement_GetExpansion,api.FPDF_StructElement_GetTitle}) {
    ExpectBytes([&](void* b,unsigned long n){return get(child,b,n);},{});
    ExpectBytes([&](void* b,unsigned long n){return get(nullptr,b,n);},{});
  }
  for(auto get:{api.FPDF_StructElement_GetID,api.FPDF_StructElement_GetLang}) {
    ExpectBytes([&](void* b,unsigned long n){return get(child,b,n);},Utf16(u""));
    ExpectBytes([&](void* b,unsigned long n){return get(nullptr,b,n);},{});
  }
  EXPECT_EQ(api.FPDF_StructElement_GetAttributeCount(child),1);
  auto empty=api.FPDF_StructElement_GetAttributeAtIndex(child,0);ASSERT_NE(empty,nullptr);EXPECT_EQ(api.FPDF_StructElement_Attr_GetCount(empty),0);
  EXPECT_EQ(api.FPDF_StructElement_GetAttributeAtIndex(child,1),nullptr);
  ASSERT_EQ(api.FPDF_StructTree_CountChildren(other.get()),2);
  auto shared=api.FPDF_StructTree_GetChildAtIndex(other.get(),0);ASSERT_NE(shared,nullptr);
  ASSERT_NO_FATAL_FAILURE(CheckElementStrings(api,shared));
  EXPECT_EQ(api.FPDF_StructElement_GetChildMarkedContentID(shared,0),-1);
  EXPECT_EQ(api.FPDF_StructElement_GetChildMarkedContentID(shared,4),4);
  auto section=api.FPDF_StructTree_GetChildAtIndex(other.get(),1);ASSERT_NE(section,nullptr);
  ASSERT_EQ(api.FPDF_StructElement_CountChildren(section),2);
  auto mcr=api.FPDF_StructElement_GetChildAtIndex(section,0);auto absent=api.FPDF_StructElement_GetChildAtIndex(section,1);ASSERT_NE(mcr,nullptr);ASSERT_NE(absent,nullptr);
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdCount(mcr),1);
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(mcr,0),3);
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentID(mcr),-1); // legacy integer-only API
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdCount(absent),-1);
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(absent,0),-1);
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentID(absent),-1);
  EXPECT_EQ(api.FPDF_StructElement_CountChildren(absent),0);
  EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentID(section),-1);
  EXPECT_EQ(api.FPDF_StructElement_GetAttributeCount(section),-1);
  EXPECT_EQ(api.FPDF_StructElement_GetAttributeCount(absent),0);
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetObjType(absent,b,n);},Utf16(u"Alternative"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetRawType(absent,static_cast<FPDF_WCHAR*>(b),n);},Utf16(u"Chain"));
  ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetType(absent,b,n);},Utf16(u"Custom"));
  for(auto get:{api.FPDF_StructElement_GetID,api.FPDF_StructElement_GetLang,api.FPDF_StructElement_GetAltText,api.FPDF_StructElement_GetActualText,api.FPDF_StructElement_GetExpansion,api.FPDF_StructElement_GetTitle})ExpectBytes([&](void* b,unsigned long n){return get(absent,b,n);},{});
  tree.reset();
  auto live=api.FPDF_StructTree_GetChildAtIndex(independent.get(),0);ASSERT_NE(live,nullptr);
  ASSERT_NO_FATAL_FAILURE(CheckElementStrings(api,live));
}

TEST(Structure, PageScopedHierarchyMetadataAndIndependentOwnersSurviveSave) {
  Artifacts artifacts;const auto bytes=TaggedFixture();
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    // Font-outline antialiasing differs by at most 7/255 in 31 of 9216
    // pixels, confined to the visible word. Keep a scene-local budget; the
    // hierarchy, actual text, and rectangle geometry remain exact assertions.
    CrossRead(*library,doc.get(),artifacts,"tagged-hierarchy",CheckStructureHierarchy,RasterTolerance{2,0.004,0.03});
    Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    Handle text(api.FPDFText_LoadPage(page.get()),api.FPDFText_ClosePage);ASSERT_NE(text.get(),nullptr);
    std::vector<FPDF_WCHAR> output(64);int written=api.FPDFText_GetText(text.get(),0,api.FPDFText_CountChars(text.get()),output.data());
    ASSERT_GT(written,1);
    // Pinned Unix native drops supplementary ActualText in ProcessMarkedContent.
    // Rust preserves the complete Unicode scalar sequence. Windows native stores
    // UTF-16 units instead, so record its surrogate units without requiring Rust
    // to duplicate that internal representation.
    std::u32string expected=U"actual \u03a9\U0001f642";
    if(library->name()=="native") {
      expected=U"actual \u03a9";
      if(sizeof(wchar_t)==2){expected.push_back(0xd83d);expected.push_back(0xde42);}
    }
    ASSERT_EQ(api.FPDFText_CountChars(text.get()),expected.size());
    for(size_t index=0;index<expected.size();++index)
      EXPECT_EQ(api.FPDFText_GetUnicode(text.get(),index),static_cast<unsigned int>(expected[index]));
    // GetText's legacy UCS-2 conversion skips full supplementary scalars;
    // source entries already holding surrogate units pass through unchanged.
    const std::u16string extracted(output.begin(),output.begin()+written-1);
    EXPECT_EQ(extracted,library->name()=="native"&&sizeof(wchar_t)==2?
      u"actual \u03a9\U0001f642":u"actual \u03a9");
    EXPECT_EQ(output[written-1],0);
  }
}

template<typename Getter>
void ExpectOutBytes(Getter get,const std::vector<uint8_t>& expected) {
  ExpectBytes([&](void* buffer,unsigned long size){unsigned long out=999;EXPECT_TRUE(get(buffer,size,&out));return out;},expected);
}

void CheckAttributeMaps(const Api& api,FPDF_DOCUMENT document,const std::string& backend) {
  Handle page(api.FPDF_LoadPage(document,0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
  Handle tree(api.FPDF_StructTree_GetForPage(page.get()),api.FPDF_StructTree_Close);ASSERT_NE(tree.get(),nullptr);
  auto element=api.FPDF_StructTree_GetChildAtIndex(tree.get(),0);ASSERT_NE(element,nullptr);
  ASSERT_EQ(api.FPDF_StructElement_GetAttributeCount(element),4);
  auto attribute=api.FPDF_StructElement_GetAttributeAtIndex(element,0);ASSERT_NE(attribute,nullptr);
  auto indirect_map=api.FPDF_StructElement_GetAttributeAtIndex(element,1);ASSERT_NE(indirect_map,nullptr);
  EXPECT_EQ(api.FPDF_StructElement_GetAttributeAtIndex(element,2),nullptr);
  auto empty=api.FPDF_StructElement_GetAttributeAtIndex(element,3);ASSERT_NE(empty,nullptr);EXPECT_EQ(api.FPDF_StructElement_Attr_GetCount(empty),0);
  for(int index:{-1,4,10000})EXPECT_EQ(api.FPDF_StructElement_GetAttributeAtIndex(element,index),nullptr);
  const std::set<std::string> names={"O","True","False","Zero","Negative","Real","Text","Unicode","PdfUnicode","Name","Blob","Empty","EmptyArray","Array","Dict","Nil","Indirect","Stream",std::string("K\xe9",2)};
  ASSERT_EQ(api.FPDF_StructElement_Attr_GetCount(attribute),names.size());std::set<std::string> actual;
  for(int index=0;index<static_cast<int>(names.size());++index) {
    unsigned long size=999;ASSERT_TRUE(api.FPDF_StructElement_Attr_GetName(attribute,index,nullptr,0,&size));ASSERT_GT(size,1u);
    std::vector<char> key(size);ASSERT_TRUE(api.FPDF_StructElement_Attr_GetName(attribute,index,key.data(),size,&size));actual.insert(std::string(key.data(),size-1));
    ExpectOutBytes([&](void* b,unsigned long n,unsigned long* out){return api.FPDF_StructElement_Attr_GetName(attribute,index,b,n,out);},Ascii(std::string(key.data(),size-1)));
    EXPECT_NE(api.FPDF_StructElement_Attr_GetValue(attribute,key.data()),nullptr);
  }
  EXPECT_EQ(actual,names);
  for(int index:{-1,static_cast<int>(names.size()),10000}) {unsigned long out=777;EXPECT_FALSE(api.FPDF_StructElement_Attr_GetName(attribute,index,nullptr,0,&out));EXPECT_EQ(out,777u);}
  EXPECT_FALSE(api.FPDF_StructElement_Attr_GetName(attribute,0,nullptr,0,nullptr));
  EXPECT_EQ(api.FPDF_StructElement_Attr_GetValue(attribute,"missing"),nullptr);EXPECT_EQ(api.FPDF_StructElement_Attr_GetValue(attribute,"true"),nullptr);
  const std::vector<std::pair<const char*,int>> types={{"True",FPDF_OBJECT_BOOLEAN},{"False",FPDF_OBJECT_BOOLEAN},{"Zero",FPDF_OBJECT_NUMBER},{"Negative",FPDF_OBJECT_NUMBER},{"Real",FPDF_OBJECT_NUMBER},{"Text",FPDF_OBJECT_STRING},{"Name",FPDF_OBJECT_NAME},{"Array",FPDF_OBJECT_ARRAY},{"Dict",FPDF_OBJECT_DICTIONARY},{"Nil",FPDF_OBJECT_NULLOBJ},{"Indirect",FPDF_OBJECT_BOOLEAN},{"Stream",FPDF_OBJECT_STREAM}};
  for(const auto& item:types) {
    auto value=api.FPDF_StructElement_Attr_GetValue(attribute,item.first);ASSERT_NE(value,nullptr);EXPECT_EQ(api.FPDF_StructElement_Attr_GetType(value),item.second);
    EXPECT_EQ(api.FPDF_StructElement_Attr_GetValue(attribute,item.first),value);
    EXPECT_FALSE(api.FPDF_StructElement_Attr_GetBooleanValue(value,nullptr));
    EXPECT_FALSE(api.FPDF_StructElement_Attr_GetNumberValue(value,nullptr));
    FPDF_BOOL boolean=777;float number=777;
    if(item.second==FPDF_OBJECT_BOOLEAN){ASSERT_TRUE(api.FPDF_StructElement_Attr_GetBooleanValue(value,&boolean));EXPECT_EQ(!!boolean,std::string(item.first)!="False");}
    else{EXPECT_FALSE(api.FPDF_StructElement_Attr_GetBooleanValue(value,&boolean));EXPECT_EQ(boolean,777);}
    if(item.second==FPDF_OBJECT_NUMBER){ASSERT_TRUE(api.FPDF_StructElement_Attr_GetNumberValue(value,&number));EXPECT_FLOAT_EQ(number,std::string(item.first)=="Zero"?0:std::string(item.first)=="Negative"?-2:1.25f);}
    else{EXPECT_FALSE(api.FPDF_StructElement_Attr_GetNumberValue(value,&number));EXPECT_FLOAT_EQ(number,777);}
    if(item.second!=FPDF_OBJECT_STRING&&item.second!=FPDF_OBJECT_NAME){unsigned long out=777;EXPECT_FALSE(api.FPDF_StructElement_Attr_GetStringValue(value,nullptr,0,&out));EXPECT_EQ(out,777u);}
    if(item.second!=FPDF_OBJECT_STRING){unsigned long out=777;EXPECT_FALSE(api.FPDF_StructElement_Attr_GetBlobValue(value,nullptr,0,&out));EXPECT_EQ(out,777u);}
    if(item.second!=FPDF_OBJECT_ARRAY){EXPECT_EQ(api.FPDF_StructElement_Attr_CountChildren(value),-1);EXPECT_EQ(api.FPDF_StructElement_Attr_GetChildAtIndex(value,0),nullptr);}
  }
  for(const auto& item:std::vector<std::pair<const char*,std::u16string>>{{"Text",u"ASCII"},{"Name",u"NameValue"},{"Empty",u""},{"Unicode",u"\u03a9\U0001f642"}}) {
    auto value=api.FPDF_StructElement_Attr_GetValue(attribute,item.first);ASSERT_NE(value,nullptr);
    ExpectOutBytes([&](void* b,unsigned long n,unsigned long* out){return api.FPDF_StructElement_Attr_GetStringValue(value,b,n,out);},Utf16(item.second));
  }
  for(const auto& item:std::vector<std::pair<const char*,std::vector<uint8_t>>>{{"Text",{'A','S','C','I','I'}},{"Blob",{'A',0,255,'B'}},{"Empty",{}},{"Unicode",{0xce,0xa9,0xf0,0x9f,0x99,0x82}}}) {
    auto value=api.FPDF_StructElement_Attr_GetValue(attribute,item.first);ASSERT_NE(value,nullptr);
    ExpectOutBytes([&](void* b,unsigned long n,unsigned long* out){return api.FPDF_StructElement_Attr_GetBlobValue(value,b,n,out);},item.second);
  }
  auto array=api.FPDF_StructElement_Attr_GetValue(attribute,"Array");ASSERT_NE(array,nullptr);ASSERT_EQ(api.FPDF_StructElement_Attr_CountChildren(array),8);
  const std::array<int,8> child_types={FPDF_OBJECT_BOOLEAN,FPDF_OBJECT_NUMBER,FPDF_OBJECT_STRING,FPDF_OBJECT_NAME,FPDF_OBJECT_DICTIONARY,FPDF_OBJECT_NULLOBJ,FPDF_OBJECT_ARRAY,FPDF_OBJECT_NUMBER};
  for(int index=0;index<8;++index){auto value=api.FPDF_StructElement_Attr_GetChildAtIndex(array,index);ASSERT_NE(value,nullptr);int expected=child_types[index];if(index==7&&backend=="native")expected=FPDF_OBJECT_REFERENCE;EXPECT_EQ(api.FPDF_StructElement_Attr_GetType(value),expected);}
  auto nested=api.FPDF_StructElement_Attr_GetChildAtIndex(array,6);ASSERT_EQ(api.FPDF_StructElement_Attr_CountChildren(nested),2);
  for(int index:{0,1}) {float value=0;ASSERT_TRUE(api.FPDF_StructElement_Attr_GetNumberValue(api.FPDF_StructElement_Attr_GetChildAtIndex(nested,index),&value));EXPECT_FLOAT_EQ(value,index+2);}
  if(backend!="native") {
    float indirect_number=91;
    ASSERT_TRUE(api.FPDF_StructElement_Attr_GetNumberValue(api.FPDF_StructElement_Attr_GetChildAtIndex(array,7),&indirect_number));
    EXPECT_FLOAT_EQ(indirect_number,3.5f);
  }
  for(int index:{-1,8,10000})EXPECT_EQ(api.FPDF_StructElement_Attr_GetChildAtIndex(array,index),nullptr);
  auto empty_array=api.FPDF_StructElement_Attr_GetValue(attribute,"EmptyArray");EXPECT_EQ(api.FPDF_StructElement_Attr_CountChildren(empty_array),0);EXPECT_EQ(api.FPDF_StructElement_Attr_GetChildAtIndex(empty_array,0),nullptr);
  for(const auto& item:std::vector<std::pair<const char*,std::u16string>>{{"Text",u"ASCII"},{"Name",u"NameValue"},{"Empty",u""}}) {
    // The legacy lookup considers direct dictionaries in an outer /A array.
    // Our primary map is indirect, so expose the direct empty-map negative
    // behavior separately from direct/indirect typed-map traversal above.
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetStringAttribute(element,item.first,b,n);},{});
  }
}

TEST(Structure, AttributeKindsBuffersArraysAndBorrowedValueLifetimes) {
  Artifacts artifacts;const auto bytes=TaggedFixture();
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    ASSERT_NO_FATAL_FAILURE(CheckAttributeMaps(api,doc.get(),library->name()));
    const auto saved=SaveDocument(api,doc.get(),artifacts,library->name()+"-structure-attributes");
    for(const auto* reader:Implementations()) {
      const auto& read=reader->api();Handle loaded(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(loaded.get(),nullptr);
      ASSERT_NO_FATAL_FAILURE(CheckAttributeMaps(read,loaded.get(),reader->name()));
    }
  }
}

TEST(Structure, MixedMarkedContentEnumerationCountsIdsRatherThanAllKids) {
  const auto bytes=TaggedFixture();
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    Handle tree(api.FPDF_StructTree_GetForPage(page.get()),api.FPDF_StructTree_Close);ASSERT_NE(tree.get(),nullptr);
    auto element=api.FPDF_StructTree_GetChildAtIndex(tree.get(),0);ASSERT_NE(element,nullptr);
    if(library->name()=="native") {
      // The pinned implementation indexes raw /K positions, including holes
      // for StructElem and OBJR kids. Its count is therefore not an ID count.
      ASSERT_EQ(api.FPDF_StructElement_GetMarkedContentIdCount(element),5);
      const std::array<int,5> observed={0,-1,2,-1,4};
      for(int index=0;index<5;++index)EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(element,index),observed[index]);
    } else {
      ASSERT_EQ(api.FPDF_StructElement_GetMarkedContentIdCount(element),3);
      const std::array<int,3> expected={0,2,4};
      for(int index=0;index<3;++index)EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(element,index),expected[index]);
      for(int index:{-1,3,10000})EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(element,index),-1);
    }
    Handle second(api.FPDF_LoadPage(doc.get(),1),api.FPDF_ClosePage);ASSERT_NE(second.get(),nullptr);
    Handle other(api.FPDF_StructTree_GetForPage(second.get()),api.FPDF_StructTree_Close);ASSERT_NE(other.get(),nullptr);
    auto section=api.FPDF_StructTree_GetChildAtIndex(other.get(),1);ASSERT_NE(section,nullptr);
    EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdCount(section),library->name()=="native"?2:-1);
    EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(section,0),-1);
    auto scalar_mcr=api.FPDF_StructElement_GetChildAtIndex(section,0);ASSERT_NE(scalar_mcr,nullptr);
    // Native ignores the index for scalar MCR dictionaries. Rust keeps the
    // documented indexed-access bound, as it already does for integer kids.
    EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(scalar_mcr,1),library->name()=="native"?3:-1);
  }
}

TEST(Structure, UntaggedEmptyMalformedAndNullInputsHaveDefinedResults) {
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    for(const std::string root:{"","/MarkInfo << /Marked true >>","/MarkInfo << /Marked true >> /StructTreeRoot << /Type /StructTreeRoot /K [] >>","/MarkInfo << /Marked true >> /StructTreeRoot << /Type /StructTreeRoot /K 17 >>"}) {
      const auto bytes=OnePage(root);Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
      Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      Handle tree(api.FPDF_StructTree_GetForPage(page.get()),api.FPDF_StructTree_Close);
      if(root.empty()){EXPECT_EQ(tree.get(),nullptr);}else{ASSERT_NE(tree.get(),nullptr);EXPECT_EQ(api.FPDF_StructTree_CountChildren(tree.get()),0);EXPECT_EQ(api.FPDF_StructTree_GetChildAtIndex(tree.get(),0),nullptr);}
    }
    for(auto get:{api.FPDF_StructElement_GetType,api.FPDF_StructElement_GetObjType})ExpectBytes([&](void* b,unsigned long n){return get(nullptr,b,n);},{});
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetRawType(nullptr,static_cast<FPDF_WCHAR*>(b),n);},{});
    api.FPDF_StructTree_Close(nullptr);
    EXPECT_EQ(api.FPDF_StructTree_GetForPage(nullptr),nullptr);EXPECT_EQ(api.FPDF_StructTree_CountChildren(nullptr),-1);EXPECT_EQ(api.FPDF_StructTree_GetChildAtIndex(nullptr,0),nullptr);
    EXPECT_EQ(api.FPDF_StructElement_CountChildren(nullptr),-1);EXPECT_EQ(api.FPDF_StructElement_GetChildAtIndex(nullptr,0),nullptr);EXPECT_EQ(api.FPDF_StructElement_GetParent(nullptr),nullptr);EXPECT_EQ(api.FPDF_StructElement_GetChildMarkedContentID(nullptr,0),-1);EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentID(nullptr),-1);EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdCount(nullptr),-1);EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(nullptr,0),-1);
    EXPECT_EQ(api.FPDF_StructElement_GetAttributeCount(nullptr),-1);EXPECT_EQ(api.FPDF_StructElement_GetAttributeAtIndex(nullptr,0),nullptr);EXPECT_EQ(api.FPDF_StructElement_Attr_GetCount(nullptr),-1);EXPECT_EQ(api.FPDF_StructElement_Attr_GetValue(nullptr,"x"),nullptr);EXPECT_EQ(api.FPDF_StructElement_Attr_GetType(nullptr),FPDF_OBJECT_UNKNOWN);EXPECT_EQ(api.FPDF_StructElement_Attr_CountChildren(nullptr),-1);EXPECT_EQ(api.FPDF_StructElement_Attr_GetChildAtIndex(nullptr,0),nullptr);
    FPDF_BOOL boolean=91;float number=92;unsigned long size=93;
    EXPECT_FALSE(api.FPDF_StructElement_Attr_GetBooleanValue(nullptr,&boolean));EXPECT_EQ(boolean,91);EXPECT_FALSE(api.FPDF_StructElement_Attr_GetNumberValue(nullptr,&number));EXPECT_FLOAT_EQ(number,92);
    EXPECT_FALSE(api.FPDF_StructElement_Attr_GetStringValue(nullptr,nullptr,0,&size));EXPECT_EQ(size,93u);EXPECT_FALSE(api.FPDF_StructElement_Attr_GetBlobValue(nullptr,nullptr,0,&size));EXPECT_EQ(size,93u);EXPECT_FALSE(api.FPDF_StructElement_Attr_GetName(nullptr,0,nullptr,0,&size));EXPECT_EQ(size,93u);
  }
}

std::vector<uint8_t> SingleElementFixture(const std::string& entries,
                                         const std::string& kids="/K 0") {
  return PdfFixture({
    "<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> /StructTreeRoot 5 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /StructParents 0 /MediaBox [0 0 96 96] /Resources << >> /Contents 4 0 R >>",
    PdfStream("","/P << /MCID 0 >> BDC 0.2 0.4 0.6 rg 8 8 80 24 re f EMC"),
    "<< /Type /StructTreeRoot /K 6 0 R /ParentTree << /Nums [0 [6 0 R]] >> /RoleMap << /Custom /P /Loop /Loop /Bad 42 >> >>",
    "<< /P 5 0 R /Pg 3 0 R "+kids+" "+entries+" >>"});
}

TEST(Structure, MetadataAbsentEmptyPdfDocEncodingAndUnicodeRemainDistinct) {
  struct TextCase{std::string encoded;std::u16string expected;bool present;};
  const std::vector<TextCase> cases={{"",u"",false},{"()",u"",true},{"(ASCII)",u"ASCII",true},{"<E9>",u"\u00e9",true},{PdfText(u"\u03a9\U0001f642"),u"\u03a9\U0001f642",true},{"(en-US)",u"en-US",true},{"(EN-us)",u"EN-us",true}};
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    for(const auto& item:cases) {
      std::string entries="/Type /StructElem /S /P";
      if(item.present)for(const std::string key:{"Alt","ActualText","E","T","ID","Lang"})entries+=" /"+key+" "+item.encoded;
      const auto bytes=SingleElementFixture(entries);
      Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
      Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      Handle tree(api.FPDF_StructTree_GetForPage(page.get()),api.FPDF_StructTree_Close);ASSERT_NE(tree.get(),nullptr);
      ASSERT_EQ(api.FPDF_StructTree_CountChildren(tree.get()),1);auto element=api.FPDF_StructTree_GetChildAtIndex(tree.get(),0);ASSERT_NE(element,nullptr);
      for(auto get:{api.FPDF_StructElement_GetAltText,api.FPDF_StructElement_GetActualText,api.FPDF_StructElement_GetExpansion,api.FPDF_StructElement_GetTitle}) {
        ExpectBytes([&](void* b,unsigned long n){return get(element,b,n);},item.expected.empty()?std::vector<uint8_t>{}:Utf16(item.expected));
      }
      for(auto get:{api.FPDF_StructElement_GetID,api.FPDF_StructElement_GetLang}) {
        ExpectBytes([&](void* b,unsigned long n){return get(element,b,n);},item.present?Utf16(item.expected):std::vector<uint8_t>{});
      }
      ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetRawType(element,static_cast<FPDF_WCHAR*>(b),n);},Utf16(u"P"));
      ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetType(element,b,n);},Utf16(u"P"));
    }
    for(const std::string type:{"","/Type /Alternative /S /Custom","/Type /StructElem /S (wrong-kind)","/Type /StructElem /S /Loop","/Type /StructElem /S /Unmapped","/Type /StructElem /S /Bad"}) {
      const auto bytes=SingleElementFixture(type);Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
      Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);Handle tree(api.FPDF_StructTree_GetForPage(page.get()),api.FPDF_StructTree_Close);ASSERT_NE(tree.get(),nullptr);
      auto element=api.FPDF_StructTree_GetChildAtIndex(tree.get(),0);ASSERT_NE(element,nullptr);
      const std::u16string raw=type.find("Custom")!=std::string::npos?u"Custom":type.find("Loop")!=std::string::npos?u"Loop":type.find("Unmapped")!=std::string::npos?u"Unmapped":type.find("Bad")!=std::string::npos?u"Bad":u"";
      const std::u16string mapped=raw==u"Custom"?u"P":raw;
      ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetRawType(element,static_cast<FPDF_WCHAR*>(b),n);},raw.empty()?std::vector<uint8_t>{}:Utf16(raw));
      ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetType(element,b,n);},mapped.empty()?std::vector<uint8_t>{}:Utf16(mapped));
      const std::u16string object=type.empty()?u"":type.find("Alternative")!=std::string::npos?u"Alternative":u"StructElem";
      ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetObjType(element,b,n);},object.empty()?std::vector<uint8_t>{}:Utf16(object));
    }
  }
}

TEST(Structure, LegacyStringAttributesReadDirectMapsAndRejectOtherKinds) {
  const std::string entries="/Type /StructElem /S /P /A [<< /O /Layout /Text (ASCII) /Name /NameValue /Empty () /Unicode "+PdfText(u"\u03a9\U0001f642")+" /Num 12 /Bool false /Arr [] /Dict << >> >> 2 << /Text (later) >>]";
  const auto bytes=SingleElementFixture(entries);
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(doc.get(),nullptr);
    Handle page(api.FPDF_LoadPage(doc.get(),0),api.FPDF_ClosePage);Handle tree(api.FPDF_StructTree_GetForPage(page.get()),api.FPDF_StructTree_Close);ASSERT_NE(tree.get(),nullptr);
    auto element=api.FPDF_StructTree_GetChildAtIndex(tree.get(),0);ASSERT_NE(element,nullptr);
    for(const auto& item:std::vector<std::pair<const char*,std::u16string>>{{"Text",u"ASCII"},{"Name",u"NameValue"},{"Empty",u""},{"Unicode",u"\u03a9\U0001f642"}}) {
      ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetStringAttribute(element,item.first,b,n);},Utf16(item.second));
    }
    for(const char* key:{"missing","","text","Num","Bool","Arr","Dict"})ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetStringAttribute(element,key,b,n);},{});
    ExpectBytes([&](void* b,unsigned long n){return api.FPDF_StructElement_GetStringAttribute(nullptr,"Text",b,n);},{});
    auto map=api.FPDF_StructElement_GetAttributeAtIndex(element,0);ASSERT_NE(map,nullptr);
    for(const char* key:{"Text","Name","Empty"}) {
      auto value=api.FPDF_StructElement_Attr_GetValue(map,key);ASSERT_NE(value,nullptr);
      EXPECT_FALSE(api.FPDF_StructElement_Attr_GetStringValue(value,nullptr,0,nullptr));
      EXPECT_FALSE(api.FPDF_StructElement_Attr_GetBlobValue(value,nullptr,0,nullptr));
      EXPECT_FALSE(api.FPDF_StructElement_Attr_GetBooleanValue(value,nullptr));
      EXPECT_FALSE(api.FPDF_StructElement_Attr_GetNumberValue(value,nullptr));
    }
  }
}

TEST(Structure, IndependentDocumentTreesAndScalarArrayEmptyAttributes) {
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    for(const auto& item:std::vector<std::pair<std::string,int>>{{"",-1},{"/A []",0},{"/A << >>",1},{"/A 42",-1},{"/A [<< /O /Layout >> 2 << >>]",3}}) {
      const auto bytes=SingleElementFixture("/Type /StructElem /S /P "+item.first);
      Handle first(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);Handle second(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(first.get(),nullptr);ASSERT_NE(second.get(),nullptr);
      Handle first_page(api.FPDF_LoadPage(first.get(),0),api.FPDF_ClosePage);Handle second_page(api.FPDF_LoadPage(second.get(),0),api.FPDF_ClosePage);
      Handle first_tree(api.FPDF_StructTree_GetForPage(first_page.get()),api.FPDF_StructTree_Close);Handle second_tree(api.FPDF_StructTree_GetForPage(second_page.get()),api.FPDF_StructTree_Close);ASSERT_NE(first_tree.get(),nullptr);ASSERT_NE(second_tree.get(),nullptr);
      auto element=api.FPDF_StructTree_GetChildAtIndex(first_tree.get(),0);ASSERT_NE(element,nullptr);EXPECT_EQ(api.FPDF_StructElement_GetAttributeCount(element),item.second);
      EXPECT_EQ(api.FPDF_StructElement_GetAttributeAtIndex(element,-1),nullptr);EXPECT_EQ(api.FPDF_StructElement_GetAttributeAtIndex(element,item.second<0?0:item.second),nullptr);
      first_tree.reset();first_page.reset();first.reset();
      auto survivor=api.FPDF_StructTree_GetChildAtIndex(second_tree.get(),0);ASSERT_NE(survivor,nullptr);EXPECT_EQ(api.FPDF_StructElement_GetAttributeCount(survivor),item.second);
    }
  }
}

TEST(Structure, IndirectMarkedContentArraysScalarsAndMcrEntriesAreResolved) {
  struct Case {
    const char* name;
    std::string kids;
    std::vector<std::string> objects;
    std::vector<int> ids;
    int native_count;
    std::vector<int> native_ids;
    int legacy_id;
  };
  const std::vector<Case> cases={
    {"indirect-array","7 0 R",
      {"[0 8 0 R 9 0 R]","<< /Type 10 0 R /Pg 3 0 R /MCID 11 0 R >>",
       "<< /Type /MCR /Pg 3 0 R /MCID 2 >>","/MCR","1"},
      {0,1,2},-1,{},-1},
    {"indirect-mcr-in-direct-array","[7 0 R]",
      {"<< /Type 8 0 R /Pg 3 0 R /MCID 9 0 R >>","/MCR","5"},
      {5},1,{-1},-1},
    {"indirect-scalar","7 0 R",{"9"},{9},-1,{},9},
    {"indirect-mcr-scalar","7 0 R",
      {"<< /Type 8 0 R /Pg 3 0 R /MCID 9 0 R >>","/MCR","6"},
      {6},-1,{},-1},
    {"indirect-mcr-type","[<< /Type 7 0 R /Pg 3 0 R /MCID 8 >>]",
      {"/MCR"},{8},1,{-1},-1},
    {"indirect-mcid","[<< /Type /MCR /Pg 3 0 R /MCID 7 0 R >>]",
      {"10"},{10},1,{-1},-1}};
  for(const auto& item:cases) {
    SCOPED_TRACE(item.name);
    // Public APIs cannot construct structure dictionaries. Object 7 onward
    // holds each independently indirect /K, MCR, /Type or /MCID value.
    std::vector<std::string> objects={
      "<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> /StructTreeRoot 5 0 R >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /StructParents 0 /MediaBox [0 0 96 96] /Resources << >> /Contents 4 0 R >>",
      PdfStream("","/P << /MCID 0 >> BDC 0.2 0.4 0.6 rg 8 8 80 24 re f EMC"),
      "<< /Type /StructTreeRoot /K 6 0 R /ParentTree << /Nums [0 [6 0 R]] >> >>",
      "<< /Type /StructElem /S /P /P 5 0 R /Pg 3 0 R /K "+item.kids+" >>"};
    objects.insert(objects.end(),item.objects.begin(),item.objects.end());
    const auto bytes=PdfFixture(objects);
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());const auto& api=library->api();
      const bool native=library->name()=="native";
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
      ASSERT_NE(document.get(),nullptr);
      Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      Handle tree(api.FPDF_StructTree_GetForPage(page.get()),api.FPDF_StructTree_Close);ASSERT_NE(tree.get(),nullptr);
      auto element=api.FPDF_StructTree_GetChildAtIndex(tree.get(),0);ASSERT_NE(element,nullptr);
      // Native tests raw /K types and raw array children. Correct Rust queries
      // resolve each valid PDF reference before inspecting its semantic type.
      const int expected_count=native?item.native_count:static_cast<int>(item.ids.size());
      EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdCount(element),expected_count);
      const auto& expected_ids=native?item.native_ids:item.ids;
      for(size_t index=0;index<expected_ids.size();++index)
        EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(element,index),expected_ids[index]);
      for(int index:{-1,std::max(expected_count,0),10000})
        EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentIdAtIndex(element,index),-1);
      EXPECT_EQ(api.FPDF_StructElement_GetMarkedContentID(element),native?-1:item.legacy_id);
    }
  }
}

}  // namespace
}  // namespace pdfoxium::integration
