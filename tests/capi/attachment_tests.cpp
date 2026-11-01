#include "document_fixture.h"

#include <climits>
#include <set>

namespace pdfoxium::integration {
namespace {
using namespace document_test;

void ExpectAttachmentFile(const Api& api, FPDF_ATTACHMENT attachment,
                           const std::vector<uint8_t>& expected) {
  unsigned long size = 999;
  ASSERT_TRUE(api.FPDFAttachment_GetFile(attachment, nullptr, 0, &size));
  ASSERT_EQ(size, expected.size());
  std::vector<unsigned long> capacities={0,size};
  if(&api!=&libraries().native.api()) {
    // The pinned native stream-copy helper reverses its capacity comparison:
    // a nonempty short buffer traps and an oversized buffer is not copied.
    // Exercise its safe query/exact paths and assert the documented boundary
    // contract independently on Rust, without duplicating the native bug.
    capacities.push_back(size?size-1:0);
    capacities.push_back(size+8);
  }
  for (unsigned long capacity : capacities) {
    std::vector<uint8_t> output(expected.size() + 32, 0xa5);
    unsigned long reported = 999;
    ASSERT_TRUE(api.FPDFAttachment_GetFile(attachment, output.data() + 8,
                                          capacity, &reported));
    EXPECT_EQ(reported, size);
    if (capacity >= size) {
      EXPECT_TRUE(std::equal(expected.begin(), expected.end(), output.begin() + 8));
      EXPECT_TRUE(std::all_of(output.begin() + 8 + size, output.end(),
                              [](uint8_t byte) { return byte == 0xa5; }));
    } else {
      EXPECT_TRUE(std::all_of(output.begin(), output.end(),
                              [](uint8_t byte) { return byte == 0xa5; }));
    }
    EXPECT_TRUE(std::all_of(output.begin(), output.begin() + 8,
                            [](uint8_t byte) { return byte == 0xa5; }));
  }
  std::array<uint8_t, 8> untouched{};
  untouched.fill(0xa5);
  EXPECT_FALSE(api.FPDFAttachment_GetFile(attachment, untouched.data(),
                                         untouched.size(), nullptr));
  EXPECT_EQ(untouched, (std::array<uint8_t, 8>{0xa5,0xa5,0xa5,0xa5,0xa5,0xa5,0xa5,0xa5}));
}

void ExpectName(const Api& api, FPDF_ATTACHMENT attachment,
                 const std::u16string& expected) {
  ExpectBytes([&](void* buffer, unsigned long size) {
    return api.FPDFAttachment_GetName(attachment, static_cast<FPDF_WCHAR*>(buffer), size);
  }, Utf16(expected));
}

TEST(Attachments, PublicCreationPayloadReplacementAndDeletionPersist) {
  Artifacts artifacts;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    Handle document(api.FPDF_CreateNewDocument(), api.FPDF_CloseDocument);
    ASSERT_NE(document.get(), nullptr);
    { Handle page(api.FPDFPage_New(document.get(), 0, 96, 96), api.FPDF_ClosePage);
      ASSERT_NE(page.get(), nullptr); ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get())); }
    EXPECT_EQ(api.FPDFDoc_GetAttachmentCount(document.get()), 0);
    const std::array<std::u16string, 5> names = {u"a.txt", u"b.txt", u"c.txt", u"d.txt", u"\u03a9\U0001f642.bin"};
    const auto empty = Wide(u"");
    EXPECT_EQ(api.FPDFDoc_AddAttachment(document.get(), empty.data()), nullptr);
    for (const auto& name : names) {
      const auto wide = Wide(name);
      auto attachment = api.FPDFDoc_AddAttachment(document.get(), wide.data());
      ASSERT_NE(attachment, nullptr);
      ExpectName(api, attachment, name);
      EXPECT_EQ(api.FPDFDoc_AddAttachment(document.get(), wide.data()), nullptr);
      ASSERT_TRUE(api.FPDFAttachment_SetFile(attachment, document.get(), nullptr, 0));
      ExpectAttachmentFile(api, attachment, {});
      const std::vector<uint8_t> binary = {'a', 0, 'b', 255, 17};
      ASSERT_TRUE(api.FPDFAttachment_SetFile(attachment, document.get(), binary.data(), binary.size()));
      ExpectAttachmentFile(api, attachment, binary);
      const auto description = Wide(u"Description \u03a9\U0001f642");
      ASSERT_TRUE(api.FPDFAttachment_SetDescription(attachment, description.data()));
      const auto key_value = Wide(u"custom \u03a9");
      ASSERT_TRUE(api.FPDFAttachment_SetStringValue(attachment, "Custom", key_value.data()));
      const auto stale_date=Wide(u"stale-date");
      ASSERT_TRUE(api.FPDFAttachment_SetStringValue(attachment,"CreationDate",stale_date.data()));
      EXPECT_TRUE(api.FPDFAttachment_HasKey(attachment, "Custom"));
      EXPECT_FALSE(api.FPDFAttachment_HasKey(attachment, "Desc"));
      const std::vector<uint8_t> abc = {'a','b','c'};
      ASSERT_TRUE(api.FPDFAttachment_SetFile(attachment, document.get(), abc.data(), abc.size()));
      EXPECT_FALSE(api.FPDFAttachment_HasKey(attachment, "Custom"));
      EXPECT_EQ(api.FPDFAttachment_GetValueType(attachment, "Size"), FPDF_OBJECT_NUMBER);
      EXPECT_EQ(api.FPDFAttachment_GetValueType(attachment, "CreationDate"), FPDF_OBJECT_STRING);
      EXPECT_GT(api.FPDFAttachment_GetStringValue(attachment, "CreationDate", nullptr, 0), 2u);
      std::vector<FPDF_WCHAR> creation_date(64,0);
      ASSERT_GT(api.FPDFAttachment_GetStringValue(attachment,"CreationDate",creation_date.data(),creation_date.size()*sizeof(FPDF_WCHAR)),4u);
      EXPECT_EQ(creation_date[0],'D');EXPECT_EQ(creation_date[1],':');
      ExpectBytes([&](void* b, unsigned long n) {
        return api.FPDFAttachment_GetStringValue(attachment, "CheckSum", static_cast<FPDF_WCHAR*>(b), n);
      }, Utf16(u"<900150983CD24FB0D6963F7D28E17F72>"));
      ExpectAttachmentFile(api, attachment, abc);
      ExpectBytes([&](void* b, unsigned long n) {
        return api.FPDFAttachment_GetDescription(attachment, static_cast<FPDF_WCHAR*>(b), n);
      }, Utf16(u"Description \u03a9\U0001f642"));
    }
    EXPECT_EQ(api.FPDFDoc_GetAttachmentCount(document.get()), 5);
    CrossRead(*library,document.get(),artifacts,"all-attachments",[&](const Api& read,FPDF_DOCUMENT doc){
      ASSERT_EQ(read.FPDFDoc_GetAttachmentCount(doc),5);
      for(int index=0;index<5;++index){auto item=read.FPDFDoc_GetAttachment(doc,index);ASSERT_NE(item,nullptr);ExpectName(read,item,names[index]);ExpectAttachmentFile(read,item,{'a','b','c'});}
    });
    EXPECT_EQ(api.FPDFDoc_GetAttachment(document.get(), -1), nullptr);
    EXPECT_EQ(api.FPDFDoc_GetAttachment(document.get(), 5), nullptr);
    EXPECT_FALSE(api.FPDFDoc_DeleteAttachment(document.get(), -1));
    EXPECT_FALSE(api.FPDFDoc_DeleteAttachment(document.get(), 5));
    ASSERT_TRUE(api.FPDFDoc_DeleteAttachment(document.get(), 0));  // first
    ASSERT_TRUE(api.FPDFDoc_DeleteAttachment(document.get(), 1));  // middle
    ASSERT_TRUE(api.FPDFDoc_DeleteAttachment(document.get(), 2));  // last
    CrossRead(*library, document.get(), artifacts, "logical-deletions",
      [&](const Api& read, FPDF_DOCUMENT doc) {
        ASSERT_EQ(read.FPDFDoc_GetAttachmentCount(doc), 2);
        for (int index = 0; index < 2; ++index) {
          auto attachment = read.FPDFDoc_GetAttachment(doc, index);
          ASSERT_NE(attachment, nullptr);
          ExpectName(read, attachment, index ? u"d.txt" : u"b.txt");
          ExpectAttachmentFile(read, attachment, {'a','b','c'});
        }
      });
  }
}

TEST(Attachments, ParamTypesStringsDescriptionSubtypeAndRelationships) {
  Artifacts artifacts;
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api = library->api();
    // These filespec/stream entries have no public constructors. Params is
    // deliberately separate from the filespec dictionary's Desc/AFRelationship.
    for (const std::string relationship : {"Source","Data","Alternative","Supplement","Unspecified","", "(wrong-kind)"}) {
      const std::string rel = relationship.empty() ? "" :
          " /AFRelationship " + (relationship[0] == '(' ? relationship : "/" + relationship);
      const auto bytes = OnePage("/Names << /EmbeddedFiles << /Names [(fixture) 5 0 R] >> >>", {
        "<< /Type /Filespec /F (fixture.bin) /UF (fixture.bin) /Desc 42 /EF << /F 6 0 R /UF 6 0 R >>" + rel + " >>",
        PdfStream("/Type /EmbeddedFile /Subtype /application#2Foctet-stream /Params << /Str (hello) /Name /Value /Num 12 /Bool true /Nil null /Arr [] /Dict << >> /Overwrite 9 >>", "payload")});
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
      ASSERT_NE(document.get(), nullptr);
      auto attachment = api.FPDFDoc_GetAttachment(document.get(), 0);
      ASSERT_NE(attachment, nullptr);
      ExpectName(api, attachment, u"fixture.bin");
      const std::array<std::pair<const char*, int>, 8> entries = {{{"missing",FPDF_OBJECT_UNKNOWN},{"Str",FPDF_OBJECT_STRING},{"Name",FPDF_OBJECT_NAME},{"Num",FPDF_OBJECT_NUMBER},{"Bool",FPDF_OBJECT_BOOLEAN},{"Nil",FPDF_OBJECT_NULLOBJ},{"Arr",FPDF_OBJECT_ARRAY},{"Dict",FPDF_OBJECT_DICTIONARY}}};
      for (const auto& entry : entries) {
        EXPECT_EQ(api.FPDFAttachment_GetValueType(attachment, entry.first), entry.second);
        EXPECT_EQ(!!api.FPDFAttachment_HasKey(attachment, entry.first), entry.second != FPDF_OBJECT_UNKNOWN);
      }
      for (const auto& entry : {std::pair<const char*,std::u16string>{"Str",u"hello"},{"Name",u"Value"},{"Num",u""},{"Bool",u""},{"missing",u""}}) {
        ExpectBytes([&](void* b, unsigned long n) { return api.FPDFAttachment_GetStringValue(attachment,entry.first,static_cast<FPDF_WCHAR*>(b),n); },Utf16(entry.second));
      }
      const auto expected_rel = relationship.empty() || relationship[0] == '(' ? u"" :
          std::u16string(relationship.begin(), relationship.end());
      ExpectBytes([&](void* b, unsigned long n) { return api.FPDFAttachment_GetAFRelationship(attachment,static_cast<FPDF_WCHAR*>(b),n); },Utf16(expected_rel));
      ExpectBytes([&](void* b, unsigned long n) { return api.FPDFAttachment_GetSubtype(attachment,static_cast<FPDF_WCHAR*>(b),n); },Utf16(u"application/octet-stream"));
      ExpectBytes([&](void* b, unsigned long n) { return api.FPDFAttachment_GetDescription(attachment,static_cast<FPDF_WCHAR*>(b),n); },Utf16(u""));
      for (const auto& value : {std::u16string(u"\u03a9\U0001f642"),std::u16string(u"")}) {
        auto wide = Wide(value);
        ASSERT_TRUE(api.FPDFAttachment_SetStringValue(attachment, "Overwrite", wide.data()));
        const char* unicode_key="key-\xce\xa9\xf0\x9f\x99\x82";
        ASSERT_TRUE(api.FPDFAttachment_SetStringValue(attachment, unicode_key, wide.data()));
        EXPECT_TRUE(api.FPDFAttachment_HasKey(attachment, unicode_key));
        ASSERT_TRUE(api.FPDFAttachment_SetDescription(attachment, wide.data()));
        CrossRead(*library, document.get(), artifacts, "param-replacement-"+relationship,
          [&](const Api& read, FPDF_DOCUMENT doc) {
            auto item=read.FPDFDoc_GetAttachment(doc,0); ASSERT_NE(item,nullptr);
            EXPECT_EQ(read.FPDFAttachment_GetValueType(item,"Overwrite"),FPDF_OBJECT_STRING);
            EXPECT_TRUE(read.FPDFAttachment_HasKey(item,unicode_key));
            ExpectBytes([&](void* b,unsigned long n){return read.FPDFAttachment_GetStringValue(item,unicode_key,static_cast<FPDF_WCHAR*>(b),n);},Utf16(value));
            ExpectBytes([&](void* b,unsigned long n){return read.FPDFAttachment_GetStringValue(item,"Overwrite",static_cast<FPDF_WCHAR*>(b),n);},Utf16(value));
            ExpectBytes([&](void* b,unsigned long n){return read.FPDFAttachment_GetDescription(item,static_cast<FPDF_WCHAR*>(b),n);},Utf16(value));
          });
      }
    }
  }
}

TEST(Attachments, MissingStreamsInvalidHandlesAndBoundedLengthFailures) {
  for (const auto* library : Implementations()) {
    SCOPED_TRACE(library->name());
    const auto& api=library->api();
    Handle document(api.FPDF_CreateNewDocument(),api.FPDF_CloseDocument);
    const auto empty_name_bytes=OnePage("/Names << /EmbeddedFiles << /Names [(empty-name) 5 0 R] >> >>",{
      "<< /Type /Filespec /F () /UF () >>"});
    Handle empty_name_doc(api.FPDF_LoadMemDocument64(empty_name_bytes.data(),empty_name_bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(empty_name_doc.get(),nullptr);
    auto empty_name_attachment=api.FPDFDoc_GetAttachment(empty_name_doc.get(),0);
    ASSERT_NE(empty_name_attachment,nullptr);
    ExpectName(api,empty_name_attachment,u"");
    const auto name=Wide(u"empty");
    auto attachment=api.FPDFDoc_AddAttachment(document.get(),name.data()); ASSERT_NE(attachment,nullptr);
    unsigned long out=123;
    std::array<uint8_t,8> buffer;buffer.fill(0xa5);
    EXPECT_FALSE(api.FPDFAttachment_GetFile(attachment,buffer.data(),buffer.size(),&out));EXPECT_EQ(out,123u);
    EXPECT_FALSE(api.FPDFAttachment_GetFile(nullptr,buffer.data(),buffer.size(),&out));EXPECT_EQ(out,123u);
    EXPECT_TRUE(std::all_of(buffer.begin(),buffer.end(),[](uint8_t x){return x==0xa5;}));
    for(auto get:{api.FPDFAttachment_GetName,api.FPDFAttachment_GetDescription,api.FPDFAttachment_GetSubtype,api.FPDFAttachment_GetAFRelationship}) {
      ExpectBytes([&](void* b,unsigned long n){return get(nullptr,static_cast<FPDF_WCHAR*>(b),n);},{});
    }
    for(auto get:{api.FPDFAttachment_GetDescription,api.FPDFAttachment_GetSubtype,api.FPDFAttachment_GetAFRelationship}) {
      ExpectBytes([&](void* b,unsigned long n){return get(attachment,static_cast<FPDF_WCHAR*>(b),n);},Utf16(u""));
    }
    EXPECT_FALSE(api.FPDFAttachment_SetFile(attachment,document.get(),nullptr,1));
    EXPECT_FALSE(api.FPDFAttachment_SetFile(attachment,document.get(),nullptr,static_cast<unsigned long>(INT_MAX)+1UL));
    EXPECT_FALSE(api.FPDFAttachment_SetFile(nullptr,document.get(),nullptr,0));
    EXPECT_FALSE(api.FPDFAttachment_SetFile(attachment,nullptr,nullptr,0));
    EXPECT_FALSE(api.FPDFAttachment_HasKey(nullptr,"Key"));
    EXPECT_EQ(api.FPDFAttachment_GetValueType(nullptr,"Key"),FPDF_OBJECT_UNKNOWN);
    EXPECT_FALSE(api.FPDFAttachment_SetStringValue(nullptr,"Key",name.data()));
    EXPECT_FALSE(api.FPDFAttachment_SetDescription(nullptr,name.data()));
    EXPECT_EQ(api.FPDFDoc_GetAttachment(nullptr,0),nullptr);
    EXPECT_FALSE(api.FPDFDoc_DeleteAttachment(nullptr,0));
  }
}

TEST(Attachments, NameTreeDepthLimitRefusesInsertionWithoutUnboundedRecursion) {
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());
    std::vector<std::string> extra;
    for(int index=0;index<35;++index) extra.push_back("<< /Kids ["+std::to_string(6+index)+" 0 R] /Limits [(a) (z)] >>");
    extra.push_back("<< /Names [(m) 41 0 R] /Limits [(a) (z)] >>");
    extra.push_back("<< /Type /Filespec /F (m) >>");
    const auto bytes=OnePage("/Names << /EmbeddedFiles 5 0 R >>",extra);
    const auto& api=library->api();
    Handle doc(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);
    ASSERT_NE(doc.get(),nullptr);
    const auto name=Wide(u"new");
    EXPECT_EQ(api.FPDFDoc_AddAttachment(doc.get(),name.data()),nullptr);
  }
}

}  // namespace
}  // namespace pdfoxium::integration
