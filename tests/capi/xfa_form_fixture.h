#pragma once

#include "form_fixture.h"
#include "pinned_xfa_font.h"

#include <limits>

namespace pdfoxium::integration::forms {

struct MemoryFile {
  FPDF_FILEHANDLER descriptor{};
  std::vector<uint8_t> data;
  int releases=0,reads=0,writes=0,flushes=0;
  bool failed=false;
  explicit MemoryFile(const std::string& bytes = "") : data(bytes.begin(),bytes.end()) {
    descriptor.clientData=this;
    descriptor.Release=[](void* p){++static_cast<MemoryFile*>(p)->releases;};
    descriptor.GetSize=[](void* p)->FPDF_DWORD{return static_cast<MemoryFile*>(p)->data.size();};
    descriptor.ReadBlock=[](void* p,FPDF_DWORD offset,void* buffer,FPDF_DWORD size)->FPDF_RESULT{
      auto& f=*static_cast<MemoryFile*>(p);++f.reads;
      if(offset>f.data.size()||size>f.data.size()-offset||(!buffer&&size))return 1;
      if(size){std::memcpy(buffer,f.data.data()+offset,size);}return 0;
    };
    descriptor.WriteBlock=[](void* p,FPDF_DWORD offset,const void* buffer,FPDF_DWORD size)->FPDF_RESULT{
      auto& f=*static_cast<MemoryFile*>(p);++f.writes;
      try {
        if(size>1048576 || offset>1048576-size || (!buffer&&size))return 1;
        if(offset+size>f.data.size())f.data.resize(offset+size);
        if(size){std::memcpy(f.data.data()+offset,buffer,size);}return 0;
      }catch(...){f.failed=true;return 1;}
    };
    descriptor.Flush=[](void* p)->FPDF_RESULT{++static_cast<MemoryFile*>(p)->flushes;return 0;};
    descriptor.Truncate=[](void* p,FPDF_DWORD size)->FPDF_RESULT{
      auto& f=*static_cast<MemoryFile*>(p);try{if(size>1048576)return 1;f.data.resize(size);return 0;}catch(...){f.failed=true;return 1;}
    };
  }
};

struct XfaCallbacks {
  const Api* api=nullptr;
  std::vector<std::unique_ptr<MemoryFile>> files;
  std::vector<std::tuple<int,std::u16string,std::string>> opened;
  std::vector<std::u16string> downloads,urls;
  std::vector<std::array<std::u16string,5>> posts,emails;
  std::vector<std::array<std::u16string,3>> puts;
  std::vector<std::u16string> uploads;
  int platform_queries=0,language_queries=0,page_rect_queries=0;
  bool success=true,failed=false;
  static XfaCallbacks& Self(FPDF_FORMFILLINFO* p) {
    return *static_cast<XfaCallbacks*>(p->m_pJsPlatform->m_pFormfillinfo);
  }
  FPDF_FILEHANDLER* File(const std::string& data="") {
    files.push_back(std::make_unique<MemoryFile>(data));return &files.back()->descriptor;
  }
  void Configure(FormHost& host) {
    api=&host.api;
    host.platform.m_pFormfillinfo=this;
    host.FFI_OpenFile=[](FPDF_FORMFILLINFO* p,int flag,FPDF_WIDESTRING url,const char* mode)->FPDF_FILEHANDLER* {
      auto& s=Self(p);try{s.opened.emplace_back(flag,Wide(url),mode?mode:"");return s.success?s.File():nullptr;}catch(...){s.failed=true;return nullptr;}
    };
    host.FFI_DownloadFromURL=[](FPDF_FORMFILLINFO* p,FPDF_WIDESTRING url)->FPDF_FILEHANDLER* {
      auto& s=Self(p);try{s.downloads.push_back(Wide(url));return s.success?s.File("download-response"):nullptr;}catch(...){s.failed=true;return nullptr;}
    };
    host.FFI_PostRequestURL=[](FPDF_FORMFILLINFO* p,FPDF_WIDESTRING url,FPDF_WIDESTRING data,FPDF_WIDESTRING type,FPDF_WIDESTRING encoding,FPDF_WIDESTRING header,FPDF_BSTR* response)->FPDF_BOOL {
      auto& s=Self(p);try{
        s.posts.push_back({Wide(url),Wide(data),Wide(type),Wide(encoding),Wide(header)});
        if(!s.success)return 0;
        const auto bytes=Terminated(u"post-response");
        return s.api->FPDF_BStr_Set(response,reinterpret_cast<const char*>(bytes.data()),static_cast<int>((bytes.size()-1)*2))==0;
      }catch(...){s.failed=true;return 0;}
    };
    host.FFI_PutRequestURL=[](FPDF_FORMFILLINFO* p,FPDF_WIDESTRING url,FPDF_WIDESTRING data,FPDF_WIDESTRING encoding)->FPDF_BOOL {
      auto& s=Self(p);try{s.puts.push_back({Wide(url),Wide(data),Wide(encoding)});return s.success;}catch(...){s.failed=true;return 0;}
    };
    host.FFI_GotoURL=[](FPDF_FORMFILLINFO* p,FPDF_DOCUMENT,FPDF_WIDESTRING url){auto& s=Self(p);try{s.urls.push_back(Wide(url));}catch(...){s.failed=true;}};
    host.FFI_EmailTo=[](FPDF_FORMFILLINFO* p,FPDF_FILEHANDLER* file,FPDF_WIDESTRING to,FPDF_WIDESTRING subject,FPDF_WIDESTRING cc,FPDF_WIDESTRING bcc,FPDF_WIDESTRING message){
      auto& s=Self(p);try{s.emails.push_back({Wide(to),Wide(subject),Wide(cc),Wide(bcc),Wide(message)});s.failed|=file==nullptr;}catch(...){s.failed=true;}
    };
    host.FFI_UploadTo=[](FPDF_FORMFILLINFO* p,FPDF_FILEHANDLER* file,int,FPDF_WIDESTRING target){
      auto& s=Self(p);try{s.uploads.push_back(Wide(target));s.failed|=file==nullptr;}catch(...){s.failed=true;}
    };
    host.FFI_GetPageViewRect=[](FPDF_FORMFILLINFO* p,FPDF_PAGE page,double* left,double* top,double* right,double* bottom) {
      auto& s=Self(p);auto& h=FormHost::Self(p);++s.page_rect_queries;h.CheckPage(page);
      *left=0;*top=0;*right=h.api.FPDF_GetPageWidthF(page);*bottom=h.api.FPDF_GetPageHeightF(page);
    };
    host.FFI_GetPlatform=[](FPDF_FORMFILLINFO* p,void* output,int capacity){auto& s=Self(p);++s.platform_queries;const std::u16string value=u"Linux";const int size=static_cast<int>(value.size()*2);if(output&&capacity>=size){std::memcpy(output,value.data(),size);}return size;};
    host.FFI_GetLanguage=[](FPDF_FORMFILLINFO* p,void* output,int capacity){auto& s=Self(p);++s.language_queries;const std::u16string value=u"en-US";const int size=static_cast<int>(value.size()*2);if(output&&capacity>=size){std::memcpy(output,value.data(),size);}return size;};
  }
};

inline std::vector<uint8_t> XfaFixture(bool full=true,const std::string& amount_events="",
                                      const std::string& extra_fields="",bool malformed=false,bool embed_test_font=false) {
  std::string xml="<template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"root\" layout=\"tb\" locale=\"en_US\"><pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/><medium short=\"200pt\" long=\"300pt\"/><contentArea w=\"200pt\" h=\"300pt\"/></pageArea></pageSet><draw w=\"40pt\" h=\"20pt\"><value><rectangle><fill><color value=\"255,0,0\"/></fill><edge presence=\"hidden\"/></rectangle></value></draw><field name=\"amount\" w=\"100pt\" h=\"20pt\"><ui><textEdit/></ui><font typeface=\"Helvetica\" size=\"12pt\"/><bind match=\"dataRef\" ref=\"$record.amount\"/>"+amount_events+"</field>"+extra_fields+"</subform></template>";
  if(malformed)xml="<template><broken";
  std::vector<std::string> objects{
      "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /NeedsRendering "+std::string(full?"true":"false")+" >>",
      "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Resources << /Font << /Helv 11 0 R >> >> /Annots [10 0 R] >>",
      "<< /Fields [10 0 R] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 11 0 R >> >> /XFA [(preamble) 5 0 R (config) 6 0 R (template) 7 0 R (datasets) 8 0 R (postamble) 9 0 R] >>",
      PdfStream("","<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">"),
      PdfStream("","<config xmlns=\"http://www.xfa.org/schema/xci/3.0/\"><acrobat><acrobat7><dynamicRender>"+std::string(full?"required":"forbidden")+"</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"),
      PdfStream("",xml),
      PdfStream("","<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data><root><amount>old</amount></root></xfa:data></xfa:datasets>"),
      PdfStream("","</xdp:xdp>"),
      "<< /Type /Annot /Subtype /Widget /FT /Tx /T (root[0].amount[0]) /V (old) /Rect [0 260 100 280] /P 3 0 R /F 4 >>",
      "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"};
  if(embed_test_font) {
    // XFA resolves the field and calendar families through embedded DR aliases,
    // before consulting native FGAS's retained system-font catalogue/cache.
    const auto font_entry=objects[3].find("/Helv 11 0 R");
    objects[3].replace(font_entry,12,"/Helv 11 0 R /Helvetica 11 0 R /Arial 11 0 R");
    auto font=PinnedXfaArimoFontObjects();
    objects[10]=std::move(font[0]);
    objects.push_back(std::move(font[1]));
    objects.push_back(std::move(font[2]));
  }
  return PdfFixture(objects);
}

inline std::string Event(const std::string& script,const std::string& activity="exit",
                         const std::string& type="application/x-javascript") {
  return "<event activity=\""+activity+"\"><script contentType=\""+type+"\"><![CDATA["+script+"]]></script></event>";
}

}  // namespace pdfoxium::integration::forms
