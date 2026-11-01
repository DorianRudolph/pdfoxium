#include "document_fixture.h"
#include "runtime_fixture.h"
#include "form_fixture.h"

#include <cstdint>
#include <cstdlib>
#include <functional>
#include <iomanip>
#include <sstream>

namespace pdfoxium::integration {
namespace {
using namespace document_test;

std::string Bytes(const std::vector<uint8_t>& data) {
  return {data.begin(),data.end()};
}

// Small test-only encoders generate independently specified PDF filter inputs.
// Zlib uses stored DEFLATE blocks, so no native compression dependency is needed.
std::string ZlibStored(const std::string& input) {
  std::string result("\x78\x01",2);
  for(size_t offset=0;offset<input.size()||offset==0;) {
    const size_t count=std::min<size_t>(65535,input.size()-offset);
    result.push_back(offset+count==input.size()?1:0);
    result.push_back(static_cast<char>(count));result.push_back(static_cast<char>(count>>8));
    const auto inverse=static_cast<uint16_t>(~static_cast<uint16_t>(count));
    result.push_back(static_cast<char>(inverse));result.push_back(static_cast<char>(inverse>>8));
    result.append(input,offset,count);offset+=count;if(offset==input.size())break;
  }
  uint32_t a=1,b=0;
  for(unsigned char value:input){a=(a+value)%65521;b=(b+a)%65521;}
  const uint32_t checksum=(b<<16)|a;
  for(int shift:{24,16,8,0})result.push_back(static_cast<char>(checksum>>shift));
  return result;
}

std::string HexStream(const std::string& input) {
  constexpr char digits[]="0123456789aBcDeF";
  std::string result;
  for(size_t i=0;i<input.size();++i) {
    const auto value=static_cast<uint8_t>(input[i]);
    result.push_back(digits[value>>4]);result.push_back(digits[value&15]);
    if(i%11==0)result+=" \n\t";
  }
  return result+">";
}

std::string Ascii85(const std::string& input) {
  std::string result;
  for(size_t offset=0;offset<input.size();offset+=4) {
    const size_t count=std::min<size_t>(4,input.size()-offset);uint32_t value=0;
    for(size_t i=0;i<4;++i)value=(value<<8)|(i<count?static_cast<uint8_t>(input[offset+i]):0);
    if(count==4&&value==0){result+='z';continue;}
    char encoded[5];for(int i=4;i>=0;--i){encoded[i]=static_cast<char>('!'+value%85);value/=85;}
    result.append(encoded,count+1);
    if(offset%28==0)result+='\n';
  }
  return result+"~>";
}

std::string RunLength(const std::string& input) {
  std::string result;
  for(size_t position=0;position<input.size();) {
    size_t run=1;while(run<128&&position+run<input.size()&&input[position+run]==input[position])++run;
    if(run>=3){result.push_back(static_cast<char>(257-run));result+=input[position];position+=run;continue;}
    const size_t start=position;position+=run;
    while(position<input.size()&&position-start<128) {
      size_t next=1;while(next<3&&position+next<input.size()&&input[position+next]==input[position])++next;
      if(next>=3)break;
      position+=std::min(next,128-(position-start));
    }
    result.push_back(static_cast<char>(position-start-1));result.append(input,start,position-start);
  }
  result.push_back(static_cast<char>(128));return result;
}

std::string LzwLiterals(const std::string& input) {
  // Clear codes keep this intentionally simple literal encoder in the 9-bit
  // dictionary range. Both EarlyChange settings must decode the same bytes.
  std::vector<unsigned int> codes{256};
  for(size_t i=0;i<input.size();++i){if(i&&i%200==0)codes.push_back(256);codes.push_back(static_cast<uint8_t>(input[i]));}
  codes.push_back(257);std::string result;uint32_t accumulator=0;unsigned int bits=0;
  for(unsigned int code:codes){accumulator=(accumulator<<9)|code;bits+=9;while(bits>=8){bits-=8;result.push_back(static_cast<char>(accumulator>>bits));}}
  if(bits)result.push_back(static_cast<char>(accumulator<<(8-bits)));
  return result;
}

int Paeth(int left,int above,int corner) {
  const int p=left+above-corner,a=std::abs(p-left),b=std::abs(p-above),c=std::abs(p-corner);
  return a<=b&&a<=c?left:b<=c?above:corner;
}

std::string PngPredictor(const std::string& input,int predictor) {
  std::string result;
  for(size_t row=0;row<input.size()/4;++row) {
    const int filter=predictor==15?row%5:predictor-10;result.push_back(static_cast<char>(filter));
    for(size_t column=0;column<4;++column) {
      const size_t index=row*4+column;
      const int value=static_cast<uint8_t>(input[index]);
      const int left=column?static_cast<uint8_t>(input[index-1]):0;
      const int above=row?static_cast<uint8_t>(input[index-4]):0;
      const int corner=row&&column?static_cast<uint8_t>(input[index-5]):0;
      const int predicted=filter==1?left:filter==2?above:filter==3?(left+above)/2:filter==4?Paeth(left,above,corner):0;
      result.push_back(static_cast<char>(value-predicted));
    }
  }
  return result;
}

std::string TiffPredictor(const std::string& input,unsigned int bits) {
  std::string result(input.size(),'\0');
  const unsigned int mask=(1u<<bits)-1;
  for(size_t row=0;row<input.size()/4;++row) {
    unsigned int previous=0;
    for(unsigned int sample=0;sample<32/bits;++sample) {
      unsigned int value=0;
      for(unsigned int bit=0;bit<bits;++bit) {
        const auto position=sample*bits+bit;
        value=(value<<1)|((static_cast<uint8_t>(input[row*4+position/8])>>(7-position%8))&1);
      }
      const auto difference=(value-previous)&mask;previous=value;
      for(unsigned int bit=0;bit<bits;++bit) {
        const auto position=sample*bits+bit;
        result[row*4+position/8]|=static_cast<char>(((difference>>(bits-bit-1))&1)<<(7-position%8));
      }
    }
  }
  return result;
}

struct FilterCase {
  std::string name,entries;
  std::function<std::string(const std::string&)> encode;
  std::vector<std::string> indirect;
};

std::vector<FilterCase> FilterCases() {
  const auto identity=[](const std::string& value){return value;};
  std::vector<FilterCase> result={
    {"none","",identity,{}},{"empty-array","/Filter [] /DecodeParms []",identity,{}},
    {"hex","/Filter /ASCIIHexDecode",HexStream,{}},{"hex-alias","/Filter /AHx",HexStream,{}},
    {"a85","/Filter /ASCII85Decode",Ascii85,{}},{"a85-alias","/Filter /A85",Ascii85,{}},
    {"runlength","/Filter /RunLengthDecode",RunLength,{}},{"runlength-alias","/Filter /RL",RunLength,{}},
    {"lzw","/Filter /LZWDecode /DecodeParms << /EarlyChange 1 >>",LzwLiterals,{}},
    {"lzw-early0","/Filter /LZW /DecodeParms << /EarlyChange 0 >>",LzwLiterals,{}},
    {"flate","/Filter /FlateDecode",ZlibStored,{}},{"flate-alias","/Filter /Fl",ZlibStored,{}},
    {"hex-a85-flate","/Filter [/AHx /A85 /Fl] /DecodeParms [null null null]",
      [](const std::string& value){return HexStream(Ascii85(ZlibStored(value)));},{}},
    {"indirect-filter-params","/Filter 8 0 R /DecodeParms 9 0 R",
      [](const std::string& value){return Ascii85(ZlibStored(PngPredictor(value,15)));},
      {"[/ASCII85Decode /FlateDecode]","[null << /Predictor 15 /Columns 4 /Colors 1 /BitsPerComponent 8 >>]"}}};
  for(int predictor=10;predictor<=15;++predictor)result.push_back({"png"+std::to_string(predictor),
    "/Filter /FlateDecode /DecodeParms << /Predictor "+std::to_string(predictor)+" /Columns 4 /Colors 1 /BitsPerComponent 8 >>",
    [predictor](const std::string& value){return ZlibStored(PngPredictor(value,predictor));},{}});
  for(unsigned int bits:{1u,2u,4u,8u,16u})result.push_back({"tiff"+std::to_string(bits),
    "/Filter /Fl /DecodeParms << /Predictor 2 /Columns "+std::to_string(32/bits)+" /Colors 1 /BitsPerComponent "+std::to_string(bits)+" >>",
    [bits](const std::string& value){return ZlibStored(TiffPredictor(value,bits));},{}});
  return result;
}

std::vector<uint8_t> BinaryPayload() {
  std::vector<uint8_t> result(128,0);
  result.insert(result.end(),128,255);
  for(int repeat=0;repeat<2;++repeat)for(int value=0;value<128;++value)result.push_back(value);
  return result;
}

void CheckPayload(const Api& api,FPDF_DOCUMENT document,const std::vector<uint8_t>& expected) {
  ASSERT_EQ(api.FPDFDoc_GetAttachmentCount(document),1);
  auto attachment=api.FPDFDoc_GetAttachment(document,0);ASSERT_NE(attachment,nullptr);
  unsigned long size=0;ASSERT_TRUE(api.FPDFAttachment_GetFile(attachment,nullptr,0,&size));ASSERT_EQ(size,expected.size());
  // Native's known stream helper capacity bug permits query/exact only.
  std::vector<uint8_t> bytes(size+16,0xa5);unsigned long actual=0;
  ASSERT_TRUE(api.FPDFAttachment_GetFile(attachment,bytes.data()+8,size,&actual));
  EXPECT_EQ(actual,expected.size());EXPECT_TRUE(std::equal(expected.begin(),expected.end(),bytes.begin()+8));
  EXPECT_TRUE(std::all_of(bytes.begin(),bytes.begin()+8,[](uint8_t v){return v==0xa5;}));
  EXPECT_TRUE(std::all_of(bytes.end()-8,bytes.end(),[](uint8_t v){return v==0xa5;}));
}

TEST(CoverageDocument, FilterAliasesChainsIndirectParametersAndPredictorsReachBinaryAndPageStreams) {
  Artifacts artifacts;const auto payload=BinaryPayload();
  std::string content="0.25 0.5 0.75 rg 16 16 64 48 re f\n";while(content.size()%4)content+=' ';
  for(const auto& item:FilterCases()) {
    SCOPED_TRACE(item.name);
    const bool packed_tiff=item.name=="tiff2"||item.name=="tiff4";
    std::vector<uint8_t> native_payload=payload;
    if(packed_tiff) {
      // Native rounds the sample stride down to zero bytes and adds each byte
      // to itself. Record that observation; require sample-correct decoding
      // and a visible rectangle independently from pdfoxium.
      const auto packed=TiffPredictor(Bytes(payload),item.name=="tiff2"?2:4);
      for(size_t i=0;i<packed.size();++i)native_payload[i]=static_cast<uint8_t>(packed[i])*2;
    }
    const auto inspect_raster=[&](const Library& backend,const Raster& image){
      if(packed_tiff&&backend.name()=="native") {
        EXPECT_TRUE(std::all_of(image.pixels.begin(),image.pixels.end(),[](uint8_t value){return value==255;}));
      } else {runtime_test::ExpectRectangle(image);}
    };
    std::vector<std::string> objects={
      "<< /Type /Catalog /Pages 2 0 R /Names << /EmbeddedFiles << /Names [(payload) 5 0 R] >> >> >>",
      "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents 4 0 R >>",
      PdfStream(item.entries,item.encode(content)),
      "<< /Type /Filespec /F (payload.bin) /UF (payload.bin) /EF << /F 6 0 R /UF 6 0 R >> >>",
      PdfStream("/Type /EmbeddedFile "+item.entries,item.encode(Bytes(payload))),
      "<< /Title (filter-pipeline) >>"};
    objects.insert(objects.end(),item.indirect.begin(),item.indirect.end());
    const auto bytes=PdfFixture(objects,"/Info 7 0 R");
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* writer:Implementations()) {
      const auto& api=writer->api();SCOPED_TRACE(writer->name());
      Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
      ASSERT_NO_FATAL_FAILURE(CheckPayload(api,document.get(),packed_tiff&&writer->name()=="native"?native_payload:payload));
      ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api,document.get(),&images[backend]));
      ASSERT_NO_FATAL_FAILURE(inspect_raster(*writer,images[backend]));
      const auto saved=SaveDocument(api,document.get(),artifacts,writer->name()+"-filter-"+item.name);
      for(const auto* reader:Implementations()) {
        const auto& read=reader->api();Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);
        // Both writers preserve these valid encoded streams and DecodeParms;
        // the reader's known packed-predictor limitation determines the native
        // observation, not which engine wrote the file.
        ASSERT_NO_FATAL_FAILURE(CheckPayload(read,reopened.get(),packed_tiff&&reader->name()=="native"?native_payload:payload));
        Raster actual;ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(read,reopened.get(),&actual));
        ASSERT_NO_FATAL_FAILURE(inspect_raster(*reader,actual));
        if(!packed_tiff||writer==reader)
          CompareRaster(images[backend],actual,artifacts,writer->name()+"-"+item.name+"-to-"+reader->name());
      }
      ++backend;
    }
    if(!packed_tiff)CompareRaster(images[0],images[1],artifacts,item.name+"-native-rust");
    else artifacts.Text(item.name+"-native-packed-predictor", "Pinned native doubles packed predictor bytes and paints no rectangle; Rust decodes the independently encoded payload and paints the expected rectangle before and after both writers save.");
  }
}

TEST(CoverageDocument, RetainedSecurityEncryptsEditedContentUnicodeStringsAndAttachmentsOnSave) {
  Artifacts artifacts;const auto payload=BinaryPayload();
  const auto name=Wide(u"payload-\u03a9\U0001f642.bin");const auto language=Wide(u"en-US \u03a9\U0001f642");
  // Pinned PDFium resource provenance: valid R2/R3 RC4 and R5/R6 AES-256
  // documents. Public C APIs cannot create an encryption dictionary/password.
  for(int revision:{2,3,4,5,6}) {
    const auto input=runtime_test::Resource(revision==4?"encrypted.pdf":"encrypted_hello_world_r"+std::to_string(revision)+".pdf");
    for(const auto* writer:Implementations())for(FPDF_DWORD flags:{FPDF_DWORD{FPDF_NO_INCREMENTAL},FPDF_DWORD{FPDF_INCREMENTAL}}) {
      const auto& api=writer->api();const auto label=writer->name()+"-retained-r"+std::to_string(revision)+"-flags"+std::to_string(flags);SCOPED_TRACE(label);
      Handle document(api.FPDF_LoadMemDocument64(input.data(),input.size(),revision==4?"5678":"h\xc3\xb4tel"),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
      EXPECT_EQ(api.FPDF_GetSecurityHandlerRevision(document.get()),revision);
      ASSERT_TRUE(api.FPDFCatalog_SetLanguage(document.get(),language.data()));
      auto attachment=api.FPDFDoc_AddAttachment(document.get(),name.data());ASSERT_NE(attachment,nullptr);
      ASSERT_TRUE(api.FPDFAttachment_SetFile(attachment,document.get(),payload.data(),payload.size()));
      Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      Handle path(api.FPDFPageObj_CreateNewRect(20,20,40,40),api.FPDFPageObj_Destroy);ASSERT_NE(path.get(),nullptr);
      ASSERT_TRUE(api.FPDFPageObj_SetFillColor(path.get(),0,200,0,255));ASSERT_TRUE(api.FPDFPath_SetDrawMode(path.get(),FPDF_FILLMODE_WINDING,0));
      api.FPDFPage_InsertObject(page.get(),path.release());ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      const auto before=Render(api,page.get(),128,96);
      int green_x=0,green_y=0;
      ASSERT_TRUE(api.FPDF_PageToDevice(page.get(),0,0,128,96,0,40,40,&green_x,&green_y));
      ASSERT_GE(green_x,0);ASSERT_LT(green_x,128);ASSERT_GE(green_y,0);ASSERT_LT(green_y,96);
      const size_t pixel=(green_y*128+green_x)*4;
      EXPECT_EQ(before.pixels[pixel],0);EXPECT_EQ(before.pixels[pixel+1],200);EXPECT_EQ(before.pixels[pixel+2],0);
      const auto saved=SaveDocument(api,document.get(),artifacts,label,flags);ASSERT_FALSE(saved.empty());
      for(const auto* reader:Implementations()) {
        const auto& read=reader->api();
        for(const char* password:{static_cast<const char*>(nullptr),"incorrect"}) {
          Handle denied(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),password),read.FPDF_CloseDocument);
          EXPECT_EQ(denied.get(),nullptr);EXPECT_EQ(read.FPDF_GetLastError(),static_cast<unsigned long>(FPDF_ERR_PASSWORD));
        }
        for(const char* password:{revision==4?"1234":"\xc3\xa2ge",revision==4?"5678":"h\xc3\xb4tel"}) {
          Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),password),read.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);
          EXPECT_EQ(read.FPDF_GetSecurityHandlerRevision(reopened.get()),revision);
          ExpectBytes([&](void* b,unsigned long n){return read.FPDFCatalog_GetLanguage(reopened.get(),static_cast<FPDF_WCHAR*>(b),n);},Utf16(u"en-US \u03a9\U0001f642"));
          auto item=read.FPDFDoc_GetAttachment(reopened.get(),0);ASSERT_NE(item,nullptr);
          ExpectBytes([&](void* b,unsigned long n){return read.FPDFAttachment_GetName(item,static_cast<FPDF_WCHAR*>(b),n);},Utf16(u"payload-\u03a9\U0001f642.bin"));
          ASSERT_NO_FATAL_FAILURE(CheckPayload(read,reopened.get(),payload));
          Raster actual;ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(read,reopened.get(),&actual));
          CompareRaster(before,actual,artifacts,label+"-to-"+reader->name(),writer==reader?RasterTolerance{}:RasterTolerance{24,0,0.1});
        }
      }
    }
  }
}

TEST(CoverageDocument, NestedAttachmentNameTreeUpdatesAncestorLimitsAndRemovesEmptyLeaves) {
  Artifacts artifacts;
  const auto bytes=OnePage("/Names << /EmbeddedFiles 5 0 R >>",{
    "<< /Kids [6 0 R 7 0 R] /Limits [(c) (x)] >>",
    "<< /Names [(c) 8 0 R (f) 9 0 R] /Limits [(c) (f)] >>",
    "<< /Names [(m) 10 0 R (x) 11 0 R] /Limits [(m) (x)] >>",
    "<< /Type /Filespec /F (c) >>","<< /Type /Filespec /F (f) >>",
    "<< /Type /Filespec /F (m) >>","<< /Type /Filespec /F (x) >>"});
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api();SCOPED_TRACE(writer->name());
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    for(const auto& name:{u"a",u"h",u"z"}){const auto wide=Wide(name);ASSERT_NE(api.FPDFDoc_AddAttachment(document.get(),wide.data()),nullptr);}
    const auto inspect=[&](const Api& read,FPDF_DOCUMENT doc,const std::vector<std::u16string>& names){
      ASSERT_EQ(read.FPDFDoc_GetAttachmentCount(doc),names.size());
      for(size_t index=0;index<names.size();++index){auto item=read.FPDFDoc_GetAttachment(doc,index);ASSERT_NE(item,nullptr);ExpectBytes([&](void* b,unsigned long n){return read.FPDFAttachment_GetName(item,static_cast<FPDF_WCHAR*>(b),n);},Utf16(names[index]));}
    };
    ASSERT_NO_FATAL_FAILURE(inspect(api,document.get(),{u"a",u"c",u"f",u"h",u"m",u"x",u"z"}));
    // Removing edge names shrinks ancestor limits; exhausting one leaf causes
    // child removal and sibling traversal while surviving borrowed handles are
    // reacquired through the owner document.
    for(const auto& name:{u"a",u"c",u"f",u"z"}) {
      int found=-1;
      for(int index=0;index<api.FPDFDoc_GetAttachmentCount(document.get());++index) {
        auto item=api.FPDFDoc_GetAttachment(document.get(),index);const auto size=api.FPDFAttachment_GetName(item,nullptr,0);std::vector<FPDF_WCHAR> output(size/2);
        api.FPDFAttachment_GetName(item,output.data(),size);
        if(std::u16string(output.begin(),output.end()-1)==name){found=index;break;}
      }
      ASSERT_GE(found,0);ASSERT_TRUE(api.FPDFDoc_DeleteAttachment(document.get(),found));
    }
    CrossRead(*writer,document.get(),artifacts,"nested-name-tree",[&](const Api& read,FPDF_DOCUMENT doc){inspect(read,doc,{u"h",u"m",u"x"});});
    for(int index=2;index>=0;--index)ASSERT_TRUE(api.FPDFDoc_DeleteAttachment(document.get(),index));
    EXPECT_EQ(api.FPDFDoc_GetAttachmentCount(document.get()),0);
    const auto last=Wide(u"restored");
    auto restored=api.FPDFDoc_AddAttachment(document.get(),last.data());
    if(writer->name()=="native") {
      // Native leaves /Kids [] at the root and cannot find a leaf to insert.
      // Rust restores an empty /Names leaf without collapsing deeper trees.
      EXPECT_EQ(restored,nullptr);EXPECT_EQ(api.FPDFDoc_GetAttachmentCount(document.get()),0);
      continue;
    }
    ASSERT_NE(restored,nullptr);
    CrossRead(*writer,document.get(),artifacts,"reinsert-empty-name-tree",[&](const Api& read,FPDF_DOCUMENT doc){inspect(read,doc,{u"restored"});});
  }
}

std::vector<uint8_t> CompressedObjectDocument(bool hybrid,bool incremental,bool reconstruct,size_t padding=0) {
  std::string result="%PDF-1.7\n";std::array<size_t,7> offsets{};
  const auto add=[&](int number,const std::string& object){offsets[number]=result.size();result+=std::to_string(number)+" 0 obj\n"+object+"\nendobj\n";};
  add(1,"<< /Type /Catalog /Pages 2 0 R >>");
  std::string content="0.25 0.5 0.75 rg 16 16 64 48 re f\n";
  if(padding)content+="%"+std::string(padding,'p')+"\n";
  add(4,PdfStream("",content));
  const std::string pages="<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 128 96] /Resources << >> >>";
  const std::string page="<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>";
  const std::string header="2 0 3 "+std::to_string(pages.size()+1)+" ";
  add(5,PdfStream("/Type /ObjStm /N 2 /First "+std::to_string(header.size())+" /Filter [/ASCII85Decode /FlateDecode]",Ascii85(ZlibStored(header+pages+" "+page))));
  offsets[6]=result.size();std::string records;
  for(int number=0;number<7;++number) {
    const unsigned int type=number==0?0:(number==2||number==3)?2:1;
    const uint32_t field=type==2?5:static_cast<uint32_t>(offsets[number]);
    records.push_back(static_cast<char>(type));
    for(int shift:{24,16,8,0})records.push_back(static_cast<char>(field>>shift));
    const uint16_t generation=number==0?65535:number==3?1:0;
    records.push_back(static_cast<char>(generation>>8));records.push_back(static_cast<char>(generation));
  }
  add(6,PdfStream("/Type /XRef /Size 7 /Root 1 0 R /W [1 4 2] /Index [0 7] /Filter /FlateDecode",ZlibStored(records)));
  size_t previous=offsets[6];
  if(hybrid) {
    previous=result.size();std::ostringstream xref;xref<<"xref\n0 7\n";
    for(int number=0;number<7;++number) {
      const bool used=number!=0&&number!=2&&number!=3;
      xref<<std::setw(10)<<std::setfill('0')<<(used?offsets[number]:0)<<' '<<std::setw(5)<<(used?0:65535)<<(used?" n \n":" f \n");
    }
    result+=xref.str()+"trailer\n<< /Root 1 0 R /Size 7 /XRefStm "+std::to_string(offsets[6])+" >>\n";
  }
  result+="startxref\n"+std::to_string(previous)+"\n%%EOF\n";
  if(incremental) {
    add(4,PdfStream("/Filter /RunLengthDecode",RunLength("0.25 0.5 0.75 rg 16 16 64 48 re f\n")));
    const size_t current=result.size();std::ostringstream entry;entry<<std::setw(10)<<std::setfill('0')<<offsets[4];
    result+="xref\n4 1\n"+entry.str()+" 00000 n \ntrailer\n<< /Root 1 0 R /Size 7 /Prev "+std::to_string(previous)+" >>\nstartxref\n"+std::to_string(current)+"\n%%EOF\n";
  }
  if(reconstruct) {
    // Omit both xref formats. Recovery must recognize the ObjStm and rebuild
    // type-2 entries from its index rather than losing the compressed pages.
    result.resize(offsets[6]);result+="trailer\n<< /Root 1 0 R /Size 7 >>\n%%EOF\n";
  }
  return {result.begin(),result.end()};
}

TEST(CoverageDocument, CompressedPagesHybridCrossReferencesIncrementalChainsAndReconstructionStayEditable) {
  Artifacts artifacts;
  for(const auto& mode:std::vector<std::array<bool,3>>{{false,false,false},{true,false,false},{false,true,false},{true,true,false},{false,false,true}}) {
    const auto label=std::string(mode[0]?"hybrid":"stream")+(mode[1]?"-incremental":"")+(mode[2]?"-recovered":"");SCOPED_TRACE(label);
    const auto input=CompressedObjectDocument(mode[0],mode[1],mode[2]);std::array<Raster,2> initial;size_t backend=0;
    for(const auto* writer:Implementations()) {
      const auto& api=writer->api();SCOPED_TRACE(writer->name());
      Handle document(api.FPDF_LoadMemDocument64(input.data(),input.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
      EXPECT_EQ(api.FPDF_GetPageCount(document.get()),1);
      EXPECT_EQ(!!api.FPDF_DocumentHasValidCrossReferenceTable(document.get()),!mode[2]);
      Handle page(api.FPDF_LoadPage(document.get(),0),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
      EXPECT_FLOAT_EQ(api.FPDF_GetPageWidthF(page.get()),128);EXPECT_FLOAT_EQ(api.FPDF_GetPageHeightF(page.get()),96);
      EXPECT_EQ(api.FPDFPage_CountObjects(page.get()),1);
      initial[backend]=Render(api,page.get(),128,96);ASSERT_NO_FATAL_FAILURE(runtime_test::ExpectRectangle(initial[backend]));
      ASSERT_TRUE(api.FPDFPage_GenerateContent(page.get()));
      const auto saved=SaveDocument(api,document.get(),artifacts,writer->name()+"-compressed-"+label);
      for(const auto* reader:Implementations()) {
        const auto& read=reader->api();Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);
        EXPECT_EQ(read.FPDF_GetPageCount(reopened.get()),1);Raster actual;ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(read,reopened.get(),&actual));
        CompareRaster(initial[backend],actual,artifacts,writer->name()+"-"+label+"-to-"+reader->name());
      }
      ++backend;
    }
    CompareRaster(initial[0],initial[1],artifacts,label+"-readers");
  }
}

TEST(CoverageDocument, Pdf20TextStringsDecodeBomByteOrderLanguageTagsAndLiteralEscapes) {
  Artifacts artifacts;
  struct Case {const char* name;std::string encoded;std::u16string expected;};
  const std::vector<Case> cases={
    {"utf8-bom","<EFBBBF48656C6C6F20CEA9F09F9982>",u"Hello \u03a9\U0001f642"},
    {"utf8-language","<EFBBBF1B656E2D55531B48656C6C6F201B656E2D47421BCEA9F09F9982>",u"Hello \u03a9\U0001f642"},
    {"utf16le","<FFFE480065006C006C006F002000A9033DD842DE>",u"Hello \u03a9\U0001f642"},
    {"utf16be-language",PdfText(u"\x1b" u"en-US\x1b" u"Hello \u03a9\U0001f642"),u"Hello \u03a9\U0001f642"},
    {"literal-escapes","(a\\(b\\)\\134c\\n\\r\\t\\b\\f\\101\\12\\1)",u"a(b)\\c\n\r\t\b\fA\n\x01"},
    {"line-continuation","(first\\\nsecond\\\r\nthird)",u"firstsecondthird"},
    {"hex-odd-nibble","<4142F>",u"AB\u00f0"}};
  for(const auto& item:cases) {
    SCOPED_TRACE(item.name);
    const auto bytes=PdfFixture({
      "<< /Type /Catalog /Pages 2 0 R >>","<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Resources << >> /Contents 4 0 R >>",
      PdfStream("","0.25 0.5 0.75 rg 16 16 64 48 re f\n"),"<< /Title "+item.encoded+" >>"},"/Info 5 0 R",20);
    for(const auto* writer:Implementations()) {
      const auto& api=writer->api();Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
      CrossRead(*writer,document.get(),artifacts,std::string("pdf20-text-")+item.name,[&](const Api& read,FPDF_DOCUMENT doc){
        ExpectBytes([&](void* b,unsigned long n){return read.FPDF_GetMetaText(doc,"Title",b,n);},Utf16(item.expected));
      });
    }
  }
}

std::vector<uint8_t> NestedPagesFixture() {
  const std::array<int,6> page_objects={5,7,8,10,11,12};
  std::string destinations="/Dests << ";
  for(int label=0;label<6;++label)destinations+="/page"+std::to_string(label)+" ["+std::to_string(page_objects[label])+" 0 R /Fit] ";
  destinations+=">>";
  std::vector<std::string> objects={
    "<< /Type /Catalog /Pages 2 0 R "+destinations+" >>",
    "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 6 /MediaBox [0 0 128 96] /Resources << >> >>",
    "<< /Type /Pages /Parent 2 0 R /Kids [5 0 R 6 0 R] /Count 3 >>",
    "<< /Type /Pages /Parent 2 0 R /Kids [10 0 R 11 0 R 12 0 R] /Count 3 >>",
    "<< /Type /Page /Parent 3 0 R /Contents 13 0 R >>",
    "<< /Type /Pages /Parent 3 0 R /Kids [7 0 R 8 0 R] /Count 2 >>",
    "<< /Type /Page /Parent 6 0 R /MediaBox [0 0 136 96] /Contents 14 0 R >>",
    "<< /Type /Page /Parent 6 0 R /MediaBox [0 0 144 96] /Contents 15 0 R >>",
    "null",
    "<< /Type /Page /Parent 4 0 R /MediaBox [0 0 152 96] /Contents 16 0 R >>",
    "<< /Type /Page /Parent 4 0 R /MediaBox [0 0 160 96] /Contents 17 0 R >>",
    "<< /Type /Page /Parent 4 0 R /MediaBox [0 0 168 96] /Contents 18 0 R >>"};
  for(int label=0;label<6;++label) {
    const std::string colors=label==0?"1 0 0":label==1?"0 1 0":label==2?"0 0 1":label==3?"1 0 1":label==4?"1 1 0":"0 1 1";
    objects.push_back(PdfStream("",colors+" rg 16 16 64 48 re f\n"));
  }
  return PdfFixture(objects);
}

void CheckNestedPages(const Api& api,FPDF_DOCUMENT document,const std::vector<int>& labels,
                      std::vector<Raster>* images) {
  ASSERT_EQ(api.FPDF_GetPageCount(document),labels.size());images->resize(labels.size());
  // Descending and ascending accesses repeatedly cross page-tree branches.
  std::vector<size_t> order;for(size_t i=0;i<labels.size();++i)order.push_back(labels.size()-i-1);
  for(size_t i=0;i<labels.size();++i)order.push_back(i);
  for(size_t index:order) {
    Handle page(api.FPDF_LoadPage(document,index),api.FPDF_ClosePage);ASSERT_NE(page.get(),nullptr);
    EXPECT_FLOAT_EQ(api.FPDF_GetPageWidthF(page.get()),128+labels[index]*8);
    EXPECT_FLOAT_EQ(api.FPDF_GetPageHeightF(page.get()),96);
    ASSERT_EQ(api.FPDFPage_CountObjects(page.get()),1);
    const auto object=api.FPDFPage_GetObject(page.get(),0);ASSERT_NE(object,nullptr);
    EXPECT_EQ(api.FPDFPageObj_GetType(object),FPDF_PAGEOBJ_PATH);
    (*images)[index]=Render(api,page.get(),176,96);
    const auto pixel=(48*176+48)*4;
    const std::array<std::array<int,3>,7> rgb={{{255,0,0},{0,255,0},{0,0,255},{255,0,255},{255,255,0},{0,255,255},{0,0,0}}};
    EXPECT_EQ((*images)[index].pixels[pixel],rgb[labels[index]][2]);
    EXPECT_EQ((*images)[index].pixels[pixel+1],rgb[labels[index]][1]);
    EXPECT_EQ((*images)[index].pixels[pixel+2],rgb[labels[index]][0]);
  }
  for(int label=0;label<6;++label) {
    auto destination=api.FPDF_GetNamedDestByName(document,("page"+std::to_string(label)).c_str());ASSERT_NE(destination,nullptr);
    const auto found=std::find(labels.begin(),labels.end(),label);
    const int expected=found==labels.end()?-1:static_cast<int>(found-labels.begin());
    EXPECT_EQ(api.FPDFDest_GetDestPageIndex(document,destination),expected);
  }
}

TEST(CoverageDocument, NestedPageTreeNavigationInsertDeleteAndMoveKeepDestinationObjectIdentity) {
  Artifacts artifacts;const auto bytes=NestedPagesFixture();
  std::array<std::vector<Raster>,2> final_images;size_t backend=0;
  for(const auto* writer:Implementations()) {
    const auto& api=writer->api();SCOPED_TRACE(writer->name());
    Handle document(api.FPDF_LoadMemDocument64(bytes.data(),bytes.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
    std::vector<Raster> initial;ASSERT_NO_FATAL_FAILURE(CheckNestedPages(api,document.get(),{0,1,2,3,4,5},&initial));
    {
      Handle inserted(api.FPDFPage_New(document.get(),2,176,96),api.FPDF_ClosePage);ASSERT_NE(inserted.get(),nullptr);
      Handle rectangle(api.FPDFPageObj_CreateNewRect(16,16,64,48),api.FPDFPageObj_Destroy);ASSERT_NE(rectangle.get(),nullptr);
      ASSERT_TRUE(api.FPDFPageObj_SetFillColor(rectangle.get(),0,0,0,255));ASSERT_TRUE(api.FPDFPath_SetDrawMode(rectangle.get(),FPDF_FILLMODE_WINDING,0));
      api.FPDFPage_InsertObject(inserted.get(),rectangle.release());ASSERT_TRUE(api.FPDFPage_GenerateContent(inserted.get()));
    }
    std::vector<Raster> intermediate;
    ASSERT_NO_FATAL_FAILURE(CheckNestedPages(api,document.get(),{0,1,6,2,3,4,5},&intermediate));
    api.FPDFPage_Delete(document.get(),4);
    ASSERT_NO_FATAL_FAILURE(CheckNestedPages(api,document.get(),{0,1,6,2,4,5},&intermediate));
    const int move[]={5,1};ASSERT_TRUE(api.FPDF_MovePages(document.get(),move,2,2));
    const std::vector<int> expected={0,6,5,1,2,4};
    ASSERT_NO_FATAL_FAILURE(CheckNestedPages(api,document.get(),expected,&final_images[backend]));
    const auto saved=SaveDocument(api,document.get(),artifacts,writer->name()+"-nested-page-edits");
    for(const auto* reader:Implementations()) {
      const auto& read=reader->api();Handle loaded(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(loaded.get(),nullptr);
      std::vector<Raster> actual;ASSERT_NO_FATAL_FAILURE(CheckNestedPages(read,loaded.get(),expected,&actual));
      for(size_t i=0;i<actual.size();++i)CompareRaster(final_images[backend][i],actual[i],artifacts,writer->name()+"-nested-page"+std::to_string(i)+"-to-"+reader->name());
    }
    ++backend;
  }
  for(size_t i=0;i<final_images[0].size();++i)CompareRaster(final_images[0][i],final_images[1][i],artifacts,"nested-page"+std::to_string(i)+"-readers");
}

struct CoverageSparseDocument : FX_FILEAVAIL,FX_DOWNLOADHINTS {
  explicit CoverageSparseDocument(std::vector<uint8_t> input):bytes(std::move(input)),available(bytes.size(),false) {
    FX_FILEAVAIL::version=1;FX_DOWNLOADHINTS::version=1;
    FX_FILEAVAIL::IsDataAvail=[](FX_FILEAVAIL* descriptor,size_t offset,size_t size)->FPDF_BOOL {
      auto& self=*static_cast<CoverageSparseDocument*>(descriptor);
      if(offset>self.bytes.size()||size>self.bytes.size()-offset)return 0;
      return std::all_of(self.available.begin()+offset,self.available.begin()+offset+size,[](bool value){return value;});
    };
    FX_DOWNLOADHINTS::AddSegment=[](FX_DOWNLOADHINTS* descriptor,size_t offset,size_t size) {
      auto& self=*static_cast<CoverageSparseDocument*>(descriptor);
      try{self.hints.emplace_back(offset,size);}catch(...){self.valid=false;}
    };
    access.m_FileLen=bytes.size();access.m_Param=this;
    access.m_GetBlock=[](void* opaque,unsigned long offset,unsigned char* output,unsigned long size)->int {
      auto& self=*static_cast<CoverageSparseDocument*>(opaque);++self.reads;
      const bool bounded=output&&size&&offset<=self.bytes.size()&&size<=self.bytes.size()-offset;
      self.valid&=bounded;
      if(!bounded||!self.IsDataAvail(&self,offset,size))return 0;
      std::copy_n(self.bytes.data()+offset,size,output);return 1;
    };
  }
  void Deliver() {
    for(const auto& hint:hints){
      EXPECT_LE(hint.first,bytes.size());
      const size_t begin=std::min(hint.first,bytes.size()),length=std::min(hint.second,bytes.size()-begin);
      std::fill(available.begin()+begin,available.begin()+begin+length,true);++deliveries;
    }
    hints.clear();
  }
  std::vector<uint8_t> bytes;
  std::vector<bool> available;
  std::vector<std::pair<size_t,size_t>> hints;
  FPDF_FILEACCESS access{};
  bool valid=true;size_t reads=0,deliveries=0;
};

TEST(CoverageDocument, SparseAvailabilityWalksCompressedHybridAndPreviousCrossReferences) {
  Artifacts artifacts;
  for(bool hybrid:{false,true})for(bool incremental:{false,true}) {
    const auto label=std::string(hybrid?"hybrid":"stream")+(incremental?"-prev":"");SCOPED_TRACE(label);
    const auto input=CompressedObjectDocument(hybrid,incremental,false,16384);
    std::array<Raster,2> images;size_t backend=0;
    for(const auto* library:Implementations()) {
      SCOPED_TRACE(library->name());const auto& api=library->api();CoverageSparseDocument source(input);
      Handle provider(api.FPDFAvail_Create(&source,&source.access),api.FPDFAvail_Destroy);ASSERT_NE(provider.get(),nullptr);
      int status=api.FPDFAvail_IsDocAvail(provider.get(),&source);EXPECT_EQ(status,PDF_DATA_NOTAVAIL);
      // DATA_NOTAVAIL may advance the internal parser state without issuing a
      // request in that call. A hybrid xref enters the full-file fallback on
      // one poll and requests that range on the next, in both implementations.
      // Bound total polls and require eventual readiness rather than one hint
      // per call; Deliver() is intentionally harmless for an empty hint list.
      for(int attempt=0;attempt<100&&status==PDF_DATA_NOTAVAIL;++attempt){source.Deliver();status=api.FPDFAvail_IsDocAvail(provider.get(),&source);}
      ASSERT_EQ(status,PDF_DATA_AVAIL);EXPECT_EQ(api.FPDFAvail_IsLinearized(provider.get()),PDF_NOT_LINEARIZED);
      Handle document(api.FPDFAvail_GetDocument(provider.get(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
      EXPECT_EQ(api.FPDF_GetPageCount(document.get()),1);EXPECT_EQ(api.FPDFAvail_GetFirstPageNum(document.get()),0);
      int form=api.FPDFAvail_IsFormAvail(provider.get(),&source);
      for(int attempt=0;attempt<100&&form==PDF_FORM_NOTAVAIL;++attempt){source.Deliver();form=api.FPDFAvail_IsFormAvail(provider.get(),&source);}
      EXPECT_TRUE(form==PDF_FORM_NOTEXIST||form==PDF_FORM_AVAIL);
      status=api.FPDFAvail_IsPageAvail(provider.get(),0,&source);
      for(int attempt=0;attempt<100&&status==PDF_DATA_NOTAVAIL;++attempt){source.Deliver();status=api.FPDFAvail_IsPageAvail(provider.get(),0,&source);}
      ASSERT_EQ(status,PDF_DATA_AVAIL);
      ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api,document.get(),&images[backend]));
      ASSERT_NO_FATAL_FAILURE(runtime_test::ExpectRectangle(images[backend]));
      Handle ordinary(api.FPDF_LoadMemDocument64(input.data(),input.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(ordinary.get(),nullptr);
      Raster full;ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(api,ordinary.get(),&full));
      CompareRaster(full,images[backend],artifacts,label+"-"+library->name()+"-sparse-vs-full");
      EXPECT_TRUE(source.valid);EXPECT_GT(source.reads,0u);EXPECT_GT(source.deliveries,0u);
      // Saving may need source bytes beyond the currently available page.
      std::fill(source.available.begin(),source.available.end(),true);
      const auto saved=SaveDocument(api,document.get(),artifacts,label+"-"+library->name()+"-sparse-save");
      for(const auto* reader:Implementations()) {
        const auto& read=reader->api();Handle reopened(read.FPDF_LoadMemDocument64(saved.data(),saved.size(),nullptr),read.FPDF_CloseDocument);ASSERT_NE(reopened.get(),nullptr);
        Raster actual;ASSERT_NO_FATAL_FAILURE(runtime_test::RenderDocument(read,reopened.get(),&actual));
        CompareRaster(images[backend],actual,artifacts,label+"-"+library->name()+"-sparse-save-to-"+reader->name());
      }
      ++backend;
    }
    CompareRaster(images[0],images[1],artifacts,label+"-sparse-readers");
  }
}

TEST(CoverageDocument, CatalogVersionOverridesAreNamesWithExactMajorDotMinorSyntax) {
  Artifacts artifacts;
  struct Case{std::string entry;int header,expected;};
  for(const auto& item:std::vector<Case>{{"/Version /1.7",14,17},{"/Version /2.0",17,20},{"",14,14},
      {"/Version (1.7)",14,14},{"/Version /1#2E70",14,14},{"/Version /v.7",14,14},{"/Version /17",14,14}}) {
    SCOPED_TRACE(item.entry);
    const auto input=PdfFixture({"<< /Type /Catalog /Pages 2 0 R "+item.entry+" >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>","<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 96] /Contents 4 0 R /Resources << >> >>",
      PdfStream("","0.25 0.5 0.75 rg 16 16 64 48 re f\n")},"",item.header);
    for(const auto* writer:Implementations()) {
      const auto& api=writer->api();Handle document(api.FPDF_LoadMemDocument64(input.data(),input.size(),nullptr),api.FPDF_CloseDocument);ASSERT_NE(document.get(),nullptr);
      int version=0;ASSERT_TRUE(api.FPDF_GetFileVersion(document.get(),&version));EXPECT_EQ(version,item.expected);
      CrossRead(*writer,document.get(),artifacts,"catalog-version-"+std::to_string(item.expected),[&](const Api& read,FPDF_DOCUMENT doc){
        int actual=0;ASSERT_TRUE(read.FPDF_GetFileVersion(doc,&actual));EXPECT_EQ(actual,item.expected);
      });
    }
  }
}

TEST(CoverageDocument, UrlEncodedSubmitParsesFdfAndEscapesSelectedFieldDataInsideHost) {
  Artifacts artifacts;forms::Actions actions;
  actions.document="this.getField('input').value='alpha beta';this.getField('second').value='A&B=C%+';";
  actions.button="this.submitForm({cURL:'https://example.invalid/encoded',bFDF:false,bEmpty:true,aFields:['input','second']});";
  std::array<Raster,2> images;size_t backend=0;
  for(const auto* library:Implementations()) {
    SCOPED_TRACE(library->name());forms::Session session(library->api(),forms::Fixture(actions),true);ASSERT_TRUE(session.valid());
    session.api.FORM_DoDocumentJSAction(session.form);session.Click(140,305);
    EXPECT_EQ(session.Field(0),u"alpha beta");EXPECT_EQ(session.Field(1),u"A&B=C%+");
    ASSERT_EQ(session.host->submissions.size(),1u);EXPECT_EQ(session.host->submit_urls,(std::vector<std::u16string>{u"https://example.invalid/encoded"}));
    const std::string actual(session.host->submissions[0].begin(),session.host->submissions[0].end());
    // Native concatenates raw name/value pairs. Rust escapes query components
    // to keep reserved &, =, %, + and spaces from changing their meaning.
    EXPECT_EQ(actual,library->name()=="native"?"input=alpha beta&second=A&B=C%+":"input=alpha+beta&second=A%26B%3DC%25%2B");
    EXPECT_TRUE(session.host->callbacks_valid);EXPECT_FALSE(session.host->callback_error);
    images[backend++]=session.Draw();
    forms::ExpectInk(images[backend-1],22,32,178,58);forms::ExpectInk(images[backend-1],222,32,378,58);
  }
  // Compare actual custom widget text in both changed field rectangles.
  for(const auto& box:std::array<std::array<int,4>,2>{{{20,30,160,30},{220,30,160,30}}})
    CompareRaster(CropRaster(images[0],box[0],box[1],box[2],box[3]),CropRaster(images[1],box[0],box[1],box[2],box[3]),artifacts,"url-submit-field-"+std::to_string(box[0]),RasterTolerance{18,0,0.25});
}

}  // namespace
}  // namespace pdfoxium::integration
