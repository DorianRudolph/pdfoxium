// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

#pragma once

#include "text_fixture.h"

#include <array>
#include <cstring>
#include <map>
#include <memory>
#include <string_view>

namespace pdfoxium::integration {

// Public-CAPI counterpart of testing/test_fonts.cpp: Times maps to Tinos,
// Arial/Helvetica/Calibri to Arimo, and Courier to Cousine, with real regular,
// bold, italic and bold-italic faces. Using the
// default builtin substitutes changes their exact metrics and PNGs.
// Install both providers before creating documents; destroy documents before
// this scope ends. Compatible nested scopes share callback ownership. The last
// scope restores owned defaults without reinitializing either library.
class ScopedPinnedTestFonts {
 public:
  explicit ScopedPinnedTestFonts(bool enumerate_xfa=false):previous_(active_) {
    if(previous_ && (previous_->enumerate_xfa || !enumerate_xfa)) {
      state_=previous_;
    } else {
      // Allocate both callback owners before replacing either library's table.
      state_=std::make_shared<State>(enumerate_xfa);
      size_t index=0;
      for(const auto* library:Implementations())
        state_->providers[index++]=std::make_unique<Provider>(*library,enumerate_xfa);
      try { Install(state_); }
      catch(...) { Install(previous_);throw; }
    }
    active_=state_;
  }
  ~ScopedPinnedTestFonts() {
    if(state_!=active_) {
      ADD_FAILURE()<<"Pinned font scopes must leave in reverse construction order";
      return;
    }
    if(state_!=previous_)Install(previous_);
    active_=previous_;
    for(const auto& provider:state_->providers)EXPECT_TRUE(provider->valid);
  }
  ScopedPinnedTestFonts(const ScopedPinnedTestFonts&)=delete;
  ScopedPinnedTestFonts& operator=(const ScopedPinnedTestFonts&)=delete;
  void ExpectMapped() const {
    // Mappers may retain font faces between nested scenarios. Verify the active
    // provider has supplied real fonts without requiring an unnecessary remap.
    for(const auto& provider:state_->providers)
      EXPECT_GT(provider->maps+provider->gets,0u)<<provider->library.name();
  }

 private:
  struct Face {
    const char* family;
    const char* file;
    const std::vector<uint8_t>* bytes=nullptr;
  };
  struct Provider:FPDF_SYSFONTINFO {
    explicit Provider(const Library& owner,bool enumerate_xfa):library(owner),enumerate_xfa(enumerate_xfa),faces{{
        {"Tinos","Tinos-Regular.ttf"},
        {"Tinos Bold","Tinos-Bold.ttf"},
        {"Tinos Italic","Tinos-Italic.ttf"},
        {"Tinos Bold Italic","Tinos-BoldItalic.ttf"},
        {"Arimo","Arimo-Regular.ttf"},
        {"Arimo Bold","Arimo-Bold.ttf"},
        {"Arimo Italic","Arimo-Italic.ttf"},
        {"Arimo Bold Italic","Arimo-BoldItalic.ttf"},
        {"Cousine","Cousine-Regular.ttf"},
        {"Cousine Bold","Cousine-Bold.ttf"},
        {"Cousine Italic","Cousine-Italic.ttf"},
        {"Cousine Bold Italic","Cousine-BoldItalic.ttf"}}} {
      version=enumerate_xfa?1:2;Release=ReleaseInfo;EnumFonts=Enumerate;MapFont=Map;GetFont=Get;
      GetFontData=Data;GetFaceName=Name;GetFontCharset=Charset;DeleteFont=Delete;
    }
    static Provider* Self(FPDF_SYSFONTINFO* info){return static_cast<Provider*>(info);}
    Face* Choose(const char* name,bool bold=false,bool italic=false) {
      if(!name)return nullptr;
      const std::string_view family(name);
      size_t first;
      if(family.empty()||family.find("Times")!=family.npos||family.find("Tinos")!=family.npos)first=0;
      else if(family.find("Arial")!=family.npos||family.find("Helvetica")!=family.npos||
              family.find("Calibri")!=family.npos||family.find("Arimo")!=family.npos)first=4;
      else if(family.find("Courier")!=family.npos||family.find("Cousine")!=family.npos)first=8;
      else return nullptr;
      bold|=family.find("Bold")!=family.npos;
      italic|=family.find("Italic")!=family.npos||family.find("Oblique")!=family.npos;
      auto* face=&faces[first+(bold?1:0)+(italic?2:0)];
      // Share immutable fixture bytes across both libraries and subsequent
      // scenarios. Enumeration alone does not load twelve font files.
      if(!face->bytes) {
        static std::map<std::string,std::vector<uint8_t>> cache;
        auto [entry,inserted]=cache.try_emplace(face->file);
        if(inserted)entry->second=text_fixture::Read(TestRoot()/"tests/capi/resources/test_fonts"/face->file);
        face->bytes=&entry->second;
        EXPECT_FALSE(face->bytes->empty())<<face->file;
        valid&=!face->bytes->empty();
      }
      return face->bytes->empty()?nullptr:face;
    }
    Face* Checked(void* handle) {
      for(auto& face:faces)if(handle==&face)return &face;
      valid=false;return nullptr;
    }
    static void ReleaseInfo(FPDF_SYSFONTINFO*) {}  // Owner is the shared scope state.
    static void Enumerate(FPDF_SYSFONTINFO* info,void* mapper) {
      auto* self=Self(info);if(!mapper){self->valid=false;return;}
      for(const auto& face:self->faces)self->library.api().FPDF_AddInstalledFont(mapper,face.family,FXFONT_ANSI_CHARSET);
      // FGAS matches names against its enumerated catalogue before GetFont.
      // Advertise the same family/style aliases that Map/Get actually supply.
      if(self->enumerate_xfa)for(const char* alias:{"Arial","Helvetica","Calibri","Times","Times New Roman","Courier","Courier New"})
        for(const char* style:{""," Bold"," Italic"," Bold Italic"}) {
          const std::string name=std::string(alias)+style;
          self->library.api().FPDF_AddInstalledFont(mapper,name.c_str(),FXFONT_ANSI_CHARSET);
        }
    }
    static void* Map(FPDF_SYSFONTINFO* info,int weight,FPDF_BOOL italic,int,int,const char* name,FPDF_BOOL* exact) {
      auto* self=Self(info);++self->maps;
      auto* face=self->Choose(name,weight>500,italic!=0);if(exact)*exact=face?1:0;return face;
    }
    static void* Get(FPDF_SYSFONTINFO* info,const char* name){auto* self=Self(info);++self->gets;return self->Choose(name);}
    static uint32_t Long(const std::vector<uint8_t>& bytes,size_t at) {
      if(at>bytes.size()||bytes.size()-at<4)return 0;
      return (uint32_t(bytes[at])<<24)|(uint32_t(bytes[at+1])<<16)|(uint32_t(bytes[at+2])<<8)|bytes[at+3];
    }
    static unsigned long Data(FPDF_SYSFONTINFO* info,void* handle,unsigned int table,
        unsigned char* buffer,unsigned long capacity) {
      auto* face=Self(info)->Checked(handle);if(!face)return 0;
      const auto& bytes=*face->bytes;size_t offset=0,length=bytes.size();
      if(table) {
        offset=length=0;
        if(bytes.size()<12)return 0;
        const size_t count=std::min<size_t>((size_t(bytes[4])<<8)|bytes[5],(bytes.size()-12)/16);
        for(size_t index=0;index<count;++index) {
          const auto row=12+index*16;
          if(Long(bytes,row)==table){offset=Long(bytes,row+8);length=Long(bytes,row+12);break;}
        }
        if(offset>bytes.size()||length>bytes.size()-offset)return 0;
      }
      if(buffer&&capacity>=length)std::copy_n(bytes.data()+offset,length,buffer);
      return static_cast<unsigned long>(length);
    }
    static unsigned long Name(FPDF_SYSFONTINFO* info,void* handle,char* buffer,unsigned long capacity) {
      auto* face=Self(info)->Checked(handle);if(!face)return 0;
      const auto size=std::strlen(face->family)+1;
      if(buffer&&capacity>=size)std::memcpy(buffer,face->family,size);
      return static_cast<unsigned long>(size);
    }
    static int Charset(FPDF_SYSFONTINFO* info,void* handle){Self(info)->Checked(handle);return FXFONT_ANSI_CHARSET;}
    static void Delete(FPDF_SYSFONTINFO* info,void* handle){Self(info)->Checked(handle);}
    const Library& library;bool enumerate_xfa;std::array<Face,12> faces;size_t maps=0,gets=0;bool valid=true;
  };
  struct State {
    explicit State(bool enumerate):enumerate_xfa(enumerate) {}
    bool enumerate_xfa;
    std::array<std::unique_ptr<Provider>,2> providers;
  };
  static void Install(const std::shared_ptr<State>& state) {
    if(state) {
      for(const auto& provider:state->providers)
        provider->library.api().FPDF_SetSystemFontInfo(provider.get());
    } else {
      for(const auto* library:Implementations())library->restore_default_font_info();
    }
  }
  inline static thread_local std::shared_ptr<State> active_;
  std::shared_ptr<State> previous_,state_;
};

// Install before RunWithLibraries. Listener callbacks acquire providers only
// after the owning comparison session is active, and release them before exit.
void InstallPinnedFontBatches();
void ResetPinnedFontBatches();

}  // namespace pdfoxium::integration
