#pragma once

#include "test_support.h"

#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstring>
#include <cstdlib>
#include <limits>
#include <functional>
#include <map>
#include <memory>
#include <string>
#include <tuple>
#include <vector>

namespace pdfoxium::integration::forms {

inline std::u16string Wide(FPDF_WIDESTRING value) {
  std::u16string text;
  if (value) while (*value) text.push_back(static_cast<char16_t>(*value++));
  return text;
}
inline std::vector<uint16_t> Terminated(const std::u16string& value) {
  std::vector<uint16_t> result(value.begin(), value.end());
  result.push_back(0);
  return result;
}
inline std::string Js(const std::string& source) {
  constexpr char hex[] = "0123456789abcdef";
  std::string result = "<< /S /JavaScript /JS <";
  for (unsigned char byte : source) {
    result.push_back(hex[byte >> 4]); result.push_back(hex[byte & 15]);
  }
  return result + "> >>";
}

struct Actions {
  std::string document;
  std::string open;
  std::string page_open;
  std::string page_close;
  std::string later_page_open, later_page_close;
  std::string keystroke;
  std::string validate;
  std::string format;
  std::string calculate;
  std::string catalog;
  std::string enter,exit,down,up,button,button_up;
};

// Field trees, widget subtypes and action dictionaries have no public creation
// APIs. The two-page fixture uses a built-in PDF standard font and keeps every
// field rectangle distinct except the explicitly overlapping text/list widgets.
inline std::vector<uint8_t> Fixture(const Actions& actions = {},
                                    const std::string& initial = "", int text_flags = 0,
                                    int max_length = 0, bool need_appearances = true) {
  const std::vector<std::string> fields = {
      "/FT /Tx /T (input) /Rect [20 340 180 370] /V (" + initial + ") /Ff " + std::to_string(text_flags) +
        (max_length ? " /MaxLen " + std::to_string(max_length) : "") +
        " /AA << /E " + Js(actions.enter) + " /X " + Js(actions.exit) + " /D " + Js(actions.down) + " /U " + Js(actions.up) + " /K " + Js(actions.keystroke) + " /V " + Js(actions.validate) + " /F " + Js(actions.format) + " >>",
      "/FT /Tx /T (second) /Rect [220 340 380 370] /V ()",
      "/FT /Btn /T (check) /Rect [20 290 50 320] /V /Off /AS /Off /MK << /CA (4) >>",
      "/FT /Btn /T (radio) /Rect [70 290 100 320] /Ff 32768 /V /Off /AS /Off /MK << /CA (l) >>",
      "/FT /Btn /T (push) /Rect [120 290 200 320] /Ff 65536 /MK << /CA (Press) /RC (Hover) /AC (Down) >> /A " + Js(actions.button) + (actions.button_up.empty()?"":" /AA << /U "+Js(actions.button_up)+" >>"),
      "/FT /Ch /T (combo) /Rect [20 240 180 270] /Ff 393216 /Opt [[(r) (Red)] [(g) (Green)] [(b) (Blue)]] /V (g)",
      "/FT /Ch /T (list) /Rect [220 180 380 290] /Ff 2097152 /Opt [[(one) (One)] [(two) (Two)] [(three) (Three)] [(four) (Four)] [(five) (Five)] [(six) (Six)] [(seven) (Seven)] [(eight) (Eight)] [(nine) (Nine)] [(ten) (Ten)]] /V (two) /I [1]",
      "/FT /Tx /T (readonly) /Rect [20 140 180 170] /Ff 1 /V (Fixed)",
      "/FT /Tx /T (multiline) /Rect [20 20 180 120] /Ff 4096 /V (First\\nSecond)",
      "/FT /Tx /T (password) /Rect [220 140 380 170] /Ff 8192 /MaxLen 6 /V (secret)",
      "/FT /Tx /T (comb) /Rect [220 100 380 130] /Ff 16777216 /MaxLen 6 /V (123)",
      "/FT /Tx /T (hidden) /Rect [220 60 380 90] /F 2 /V (hidden)",
      "/FT /Btn /T (uri) /Rect [220 20 380 50] /Ff 65536 /MK << /CA (URI) >> /A << /S /URI /URI (https://example.invalid/form) >>",
      "/FT /Tx /T (later) /Rect [20 340 180 370] /V (later)",
      "/FT /Tx /T (calc) /Rect [20 180 180 210] /Ff 1 /V () /AA << /C " + Js(actions.calculate) + " >>"};
  std::vector<std::string> objects(6);
  std::string annots, roots;
  for (size_t index = 0; index < fields.size(); ++index) {
    const auto number = std::to_string(index + 7) + " 0 R ";
    roots += number;
    if (index != 13) annots += number;
    objects.push_back("<< /Type /Annot /Subtype /Widget /P " + std::string(index == 13 ? "6" : "3") +
        " 0 R /F 4 /BS << /W 1 /S /S >> /MK << /BC [0 0 0] /BG [1 1 1] >> " + fields[index] + " >>");
  }
  const auto content = std::to_string(objects.size() + 1) + " 0 R";
  objects.push_back(PdfStream("", "0.94 g 0 0 400 400 re f\n"));
  const int off_number = static_cast<int>(objects.size()) + 1;
  objects.push_back(PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 30 30]",
      "1 g 0 0 30 30 re f 0 G 1 w .5 .5 29 29 re S\n"));
  const int on_number = static_cast<int>(objects.size()) + 1;
  objects.push_back(PdfStream("/Type /XObject /Subtype /Form /BBox [0 0 30 30]",
      "1 g 0 0 30 30 re f 0 G 1 w .5 .5 29 29 re S 0 g 7 7 16 16 re f\n"));
  for (size_t index : {size_t{8}, size_t{9}}) {
    auto& widget = objects[index];
    widget.insert(widget.size()-3, " /AP << /N << /Off " + std::to_string(off_number) +
        " 0 R /Yes " + std::to_string(on_number) + " 0 R >> >>");
  }
  objects[0] = "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R " + actions.catalog;
  if (!actions.document.empty()) objects[0] += " /Names << /JavaScript << /Names [(main) " + Js(actions.document) + "] >> >>";
  if (!actions.open.empty()) objects[0] += " /OpenAction " + Js(actions.open);
  objects[0] += " >>";
  objects[1] = "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>";
  objects[2] = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Contents " + content +
      " /Resources << /Font << /Helv 5 0 R >> >> /Annots [" + annots + "] /AA << /O " + Js(actions.page_open) + " /C " + Js(actions.page_close) + " >> >>";
  objects[3] = "<< /Fields [" + roots + "] /CO [21 0 R] /NeedAppearances " + std::string(need_appearances ? "true" : "false") + " /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>";
  objects[4] = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>";
  objects[5] = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Contents " + content +
      " /Resources << /Font << /Helv 5 0 R >> >> /Annots [20 0 R] /AA << /O " + Js(actions.later_page_open) + " /C " + Js(actions.later_page_close) + " >> >>";
  return PdfFixture(objects);
}

struct FormHost : FPDF_FORMFILLINFO {
  struct Platform : IPDF_JSPLATFORM { FormHost* host = nullptr; } platform;
  const Api& api;
  FPDF_DOCUMENT document = nullptr;
  FPDF_FORMHANDLE form = nullptr;
  FPDF_DOCUMENT independent = nullptr;
  std::vector<FPDF_PAGE> pages;
  int current_page = 0;
  bool callbacks_valid = true;
  bool callback_error = false;
  bool reenter = false;
  int releases = 0, changes = 0, invalidations = 0;
  int next_timer = 100;
  std::map<int, TimerCallback> timers;
  std::vector<int> killed, cursors, focus_pages, goto_pages, beeps;
  std::vector<std::array<double, 4>> rectangles, selections;
  std::vector<std::pair<std::u16string, bool>> focus_values;
  std::vector<std::string> named_actions, uris;
  std::vector<int> uri_modifiers;
  std::vector<std::tuple<int, int, std::vector<float>>> destinations;
  std::vector<std::tuple<std::u16string, std::u16string, int, int>> alerts;
  std::vector<std::array<std::u16string, 4>> responses;
  std::vector<int> response_capacities, response_passwords;
  std::vector<std::array<int, 8>> prints;
  std::vector<std::array<std::u16string, 5>> mails;
  std::vector<int> mail_ui;
  std::vector<std::vector<uint8_t>> mail_packets, submissions;
  std::vector<std::u16string> submit_urls;
  std::u16string response = u"reply";
  std::string filepath = "/virtual/form.pdf", browse = "chosen.pdf";
  int alert_result = JSPLATFORM_ALERT_RETURN_YES;
  int file_queries = 0, browse_queries = 0;
  std::vector<std::pair<int, unsigned long>> page_events;
  int caret_events = 0, popup_events = 0;
  int generated_page_callbacks = 0;
  bool popup_success = true;
  std::vector<int> popup_flags;
  std::vector<std::array<float,2>> popup_points;

  explicit FormHost(const Api& api) : FPDF_FORMFILLINFO{}, api(api) {
    version = 2;
    xfa_disabled = 1;
    Release = [](FPDF_FORMFILLINFO* p) { ++Self(p).releases; };
    FFI_Invalidate = [](FPDF_FORMFILLINFO* p, FPDF_PAGE page, double l, double t, double r, double b) {
      auto& h = Self(p); h.Record([&] { ++h.invalidations; h.CheckPage(page); h.rectangles.push_back({l,t,r,b}); h.Reenter(); });
    };
    FFI_OutputSelectedRect = [](FPDF_FORMFILLINFO* p, FPDF_PAGE page, double l, double t, double r, double b) {
      auto& h = Self(p); h.Record([&] { h.CheckPage(page); h.selections.push_back({l,t,r,b}); });
    };
    FFI_SetCursor = [](FPDF_FORMFILLINFO* p, int cursor) { auto& h = Self(p); h.Record([&] { h.cursors.push_back(cursor); }); };
    FFI_SetTimer = [](FPDF_FORMFILLINFO* p, int, TimerCallback callback) {
      auto& h = Self(p); const int id = ++h.next_timer;
      h.Record([&] { h.timers[id] = callback; }); return id;
    };
    FFI_KillTimer = [](FPDF_FORMFILLINFO* p, int id) { auto& h = Self(p); h.Record([&] { h.killed.push_back(id); h.timers.erase(id); }); };
    FFI_OnChange = [](FPDF_FORMFILLINFO* p) { auto& h = Self(p); ++h.changes; h.Reenter(); };
    FFI_GetPage = [](FPDF_FORMFILLINFO* p, FPDF_DOCUMENT document, int index) -> FPDF_PAGE {
      auto& h = Self(p); if (document != h.document) { h.callbacks_valid = false; return nullptr; }
      return h.Page(index);
    };
    FFI_GetCurrentPage = [](FPDF_FORMFILLINFO* p, FPDF_DOCUMENT document) -> FPDF_PAGE {
      auto& h = Self(p); if (document != h.document) { h.callbacks_valid = false; return nullptr; } return h.Page(h.current_page);
    };
    FFI_GetRotation = [](FPDF_FORMFILLINFO* p, FPDF_PAGE page) { auto& h = Self(p); h.CheckPage(page); return h.api.FPDFPage_GetRotation(page); };
    FFI_SetTextFieldFocus = [](FPDF_FORMFILLINFO* p, FPDF_WIDESTRING value, FPDF_DWORD count, FPDF_BOOL focus) {
      auto& h = Self(p); h.Record([&] {
        std::u16string text; if (value) for (FPDF_DWORD i = 0; i < count; ++i) text.push_back(value[i]);
        h.focus_values.emplace_back(std::move(text), focus != 0);
      });
    };
    FFI_OnFocusChange = [](FPDF_FORMFILLINFO* p, FPDF_ANNOTATION annotation, int page) {
      auto& h = Self(p); h.Record([&] { h.focus_pages.push_back(page); if (annotation) { const int subtype=h.api.FPDFAnnot_GetSubtype(annotation); h.callbacks_valid &= subtype==FPDF_ANNOT_WIDGET || subtype==FPDF_ANNOT_XFAWIDGET; } });
    };
    FFI_ExecuteNamedAction = [](FPDF_FORMFILLINFO* p, FPDF_BYTESTRING action) { auto& h = Self(p); h.Record([&] { h.named_actions.emplace_back(action ? action : ""); }); };
    FFI_DoURIAction = [](FPDF_FORMFILLINFO* p, FPDF_BYTESTRING uri) { auto& h = Self(p); h.Record([&] { h.uris.emplace_back(uri ? uri : ""); h.uri_modifiers.push_back(0); }); };
    FFI_DoURIActionWithKeyboardModifier = [](FPDF_FORMFILLINFO* p, FPDF_BYTESTRING uri, int modifiers) { auto& h = Self(p); h.Record([&] { h.uris.emplace_back(uri ? uri : ""); h.uri_modifiers.push_back(modifiers); }); };
    FFI_DoGoToAction = [](FPDF_FORMFILLINFO* p, int page, int mode, float* positions, int size) {
      auto& h = Self(p); h.Record([&] { std::vector<float> values; if (positions && size > 0) values.assign(positions, positions + size); h.destinations.emplace_back(page, mode, std::move(values)); h.Reenter(); });
    };
    FFI_GetCurrentPageIndex = [](FPDF_FORMFILLINFO* p, FPDF_DOCUMENT) { return Self(p).current_page; };
    FFI_SetCurrentPage = [](FPDF_FORMFILLINFO* p, FPDF_DOCUMENT, int index) { auto& h = Self(p); h.current_page = index; h.Record([&] { h.goto_pages.push_back(index); }); };
    FFI_GetPlatform = [](FPDF_FORMFILLINFO*, void* out, int capacity) {
      const std::u16string text=u"Linux"; const int size = static_cast<int>(text.size()*2);
      if(out && capacity >= size) { std::memcpy(out,text.data(),size); }
      return size;
    };
    FFI_GetLanguage = [](FPDF_FORMFILLINFO*, void* out, int capacity) {
      const std::u16string text=u"en-US"; const int size = static_cast<int>(text.size()*2);
      if(out && capacity >= size) { std::memcpy(out,text.data(),size); }
      return size;
    };
    FFI_GetPageViewRect = [](FPDF_FORMFILLINFO*, FPDF_PAGE, double* l, double* t, double* r, double* b) { *l=0; *t=400; *r=400; *b=0; };
    FFI_DisplayCaret = [](FPDF_FORMFILLINFO* p, FPDF_PAGE page, FPDF_BOOL, double, double, double, double) { auto& h = Self(p); h.CheckPage(page); ++h.caret_events; };
    FFI_PageEvent = [](FPDF_FORMFILLINFO* p, int count, FPDF_DWORD kind) { auto& h = Self(p); h.Record([&] { h.page_events.emplace_back(count, kind); }); };
    FFI_PopupMenu = [](FPDF_FORMFILLINFO* p, FPDF_PAGE page, FPDF_WIDGET widget, int flags, float x, float y) -> FPDF_BOOL {
      auto& h = Self(p); h.CheckPage(page); h.callbacks_valid &= widget == nullptr; ++h.popup_events; h.Record([&] { h.popup_flags.push_back(flags); h.popup_points.push_back({x,y}); }); return h.popup_success;
    };
    platform.host = this;
    platform.version = 2;
    platform.app_alert = [](IPDF_JSPLATFORM* p, FPDF_WIDESTRING message, FPDF_WIDESTRING title, int kind, int icon) {
      auto& h = *static_cast<Platform*>(p)->host; h.Record([&] { h.alerts.emplace_back(Wide(message), Wide(title), kind, icon); h.Reenter(); }); return h.alert_result;
    };
    platform.app_beep = [](IPDF_JSPLATFORM* p, int kind) { auto& h = *static_cast<Platform*>(p)->host; h.Record([&] { h.beeps.push_back(kind); }); };
    platform.app_response = [](IPDF_JSPLATFORM* p, FPDF_WIDESTRING question, FPDF_WIDESTRING title, FPDF_WIDESTRING value, FPDF_WIDESTRING label, FPDF_BOOL password, void* output, int capacity) {
      auto& h = *static_cast<Platform*>(p)->host;
      h.Record([&] { h.responses.push_back({Wide(question), Wide(title), Wide(value), Wide(label)}); h.response_capacities.push_back(capacity); h.response_passwords.push_back(password); });
      const int size = static_cast<int>(h.response.size() * 2);
      if (output && capacity > 0) std::memcpy(output, h.response.data(), static_cast<size_t>(std::min(size, capacity)));
      return size;
    };
    platform.Doc_getFilePath = [](IPDF_JSPLATFORM* p, void* output, int capacity) { auto& h = *static_cast<Platform*>(p)->host; ++h.file_queries; return h.CopyBytes(h.filepath, output, capacity); };
    platform.Field_browse = [](IPDF_JSPLATFORM* p, void* output, int capacity) { auto& h = *static_cast<Platform*>(p)->host; ++h.browse_queries; return h.CopyBytes(h.browse, output, capacity); };
    platform.Doc_gotoPage = [](IPDF_JSPLATFORM* p, int index) { auto& h = *static_cast<Platform*>(p)->host; h.current_page = index; h.Record([&] { h.goto_pages.push_back(index); }); };
    platform.Doc_print = [](IPDF_JSPLATFORM* p, FPDF_BOOL ui, int first, int last, FPDF_BOOL silent, FPDF_BOOL shrink, FPDF_BOOL image, FPDF_BOOL reverse, FPDF_BOOL annotations) {
      auto& h = *static_cast<Platform*>(p)->host; h.Record([&] { h.prints.push_back({ui,first,last,silent,shrink,image,reverse,annotations}); });
    };
    platform.Doc_mail = [](IPDF_JSPLATFORM* p, void* bytes, int size, FPDF_BOOL ui, FPDF_WIDESTRING to, FPDF_WIDESTRING subject, FPDF_WIDESTRING cc, FPDF_WIDESTRING bcc, FPDF_WIDESTRING message) {
      auto& h = *static_cast<Platform*>(p)->host; h.Record([&] { h.mails.push_back({Wide(to),Wide(subject),Wide(cc),Wide(bcc),Wide(message)}); h.mail_ui.push_back(ui); h.mail_packets.emplace_back(); if (bytes && size > 0) h.mail_packets.back().assign(static_cast<uint8_t*>(bytes), static_cast<uint8_t*>(bytes) + size); });
    };
    platform.Doc_submitForm = [](IPDF_JSPLATFORM* p, void* bytes, int size, FPDF_WIDESTRING url) {
      auto& h = *static_cast<Platform*>(p)->host; h.Record([&] { h.submit_urls.push_back(Wide(url)); h.submissions.emplace_back(); if (bytes && size > 0) h.submissions.back().assign(static_cast<uint8_t*>(bytes), static_cast<uint8_t*>(bytes)+size); });
    };
  }
  static FormHost& Self(FPDF_FORMFILLINFO* p) { return *static_cast<FormHost*>(p); }
  template<class F> void Record(F&& function) noexcept { try { function(); } catch (...) { callback_error = true; } }
  int CopyBytes(const std::string& text, void* output, int capacity) {
    const int required = static_cast<int>(text.size()) + 1;
    if (output && capacity >= required) std::memcpy(output, text.c_str(), required);
    return required;
  }
  void Reenter() {
    if (!reenter) return;
    callbacks_valid &= api.FPDF_GetPageCount(document) >= 1;
    if (independent) callbacks_valid &= api.FPDF_GetPageCount(independent) == 0;
  }
  void CheckPage(FPDF_PAGE page) {
    if(!page) { callbacks_valid=false;return; }
    if(std::find(pages.begin(),pages.end(),page)!=pages.end()) return;
    if(!xfa_disabled && page) {
      // Layout can invalidate newly generated XFA pages before the embedder
      // explicitly loads them. These callback handles are borrowed; validate
      // their geometry through the creating API and never close them. A
      // retired layout page legitimately has zero geometry after pagination.
      ++generated_page_callbacks;
      const float width=api.FPDF_GetPageWidthF(page),height=api.FPDF_GetPageHeightF(page);
      callbacks_valid &= std::isfinite(width) && std::isfinite(height) && width>=0 && height>=0;
      return;
    }
    callbacks_valid=false;
  }
  FPDF_PAGE Page(int index) {
    if (index < 0 || index >= api.FPDF_GetPageCount(document)) return nullptr;
    if (static_cast<size_t>(index) >= pages.size()) pages.resize(index + 1, nullptr);
    if (!pages[index]) {
      pages[index] = api.FPDF_LoadPage(document, index);
      if (pages[index] && form) api.FORM_OnAfterLoadPage(pages[index], form);
    }
    return pages[index];
  }
  void Tick(int id) { const auto it = timers.find(id); if (it != timers.end() && it->second) { const auto callback = it->second; callback(id); } }
};

class Session {
 public:
  Session(const Api& api, std::vector<uint8_t> bytes, bool javascript = false,
          bool xfa = false, int version = 2,
          const std::function<void(FormHost&)>& configure = {}, bool int_length_loader = false,
          const char* password = nullptr)
      : api(api), bytes(std::move(bytes)), host(std::make_unique<FormHost>(api)) {
    document = int_length_loader
        ? api.FPDF_LoadMemDocument(this->bytes.data(),static_cast<int>(this->bytes.size()),password)
        : api.FPDF_LoadMemDocument64(this->bytes.data(), this->bytes.size(), password);
    if (!document) { ADD_FAILURE() << "Form document load error " << api.FPDF_GetLastError(); return; }
    host->document = document;
    host->version = version;
    host->xfa_disabled = !xfa;
    host->m_pJsPlatform = javascript ? &host->platform : nullptr;
    if (configure) configure(*host);
    form = api.FPDFDOC_InitFormFillEnvironment(document, host.get());
    host->form = form;
    if (!form) { ADD_FAILURE() << "Form environment creation failed"; return; }
    if (xfa) xfa_loaded = !!api.FPDF_LoadXFA(document);
    page = host->Page(0);
    if (!page) ADD_FAILURE() << "Form page load failed";
    api.FPDF_SetFormFieldHighlightAlpha(form, 0);
  }
  ~Session() { Close(); }
  Session(const Session&) = delete;
  Session& operator=(const Session&) = delete;
  bool valid() const { return document && form && page; }
  void ClosePage(int index) {
    if (index < 0 || static_cast<size_t>(index) >= host->pages.size() || !host->pages[index]) return;
    auto old = host->pages[index];
    api.FORM_OnBeforeClosePage(old, form);
    host->pages[index] = nullptr;
    api.FPDF_ClosePage(old);
    if (index == 0) page = nullptr;
  }
  void Close() {
    if (!document) return;
    if (form) {
      for (size_t i = 0; i < host->pages.size(); ++i) ClosePage(static_cast<int>(i));
      api.FPDFDOC_ExitFormFillEnvironment(form);
      host->form = nullptr;
      form = nullptr;
    }
    api.FPDF_CloseDocument(document); document = nullptr;
  }
  bool Focus(double x, double y, int page_index = 0) { return !!api.FORM_OnFocus(form, host->Page(page_index), 0, x, y); }
  void Click(double x, double y, int modifiers = 0, int page_index = 0) {
    auto p = host->Page(page_index);
    api.FORM_OnMouseMove(form,p,modifiers,x,y);
    api.FORM_OnLButtonDown(form,p,modifiers,x,y);
    api.FORM_OnLButtonUp(form,p,modifiers,x,y);
  }
  std::u16string Text(bool selected = false, int page_index = 0) const {
    const auto get = selected ? api.FORM_GetSelectedText : api.FORM_GetFocusedText;
    const auto required = get(form,host->pages.at(page_index),nullptr,0);
    if (!required) return {};
    if (required < 2 || required % 2) { ADD_FAILURE() << "Invalid UTF-16 byte count"; return {}; }
    std::vector<uint16_t> storage(required / 2 + 1, 0);
    EXPECT_EQ(get(form,host->pages.at(page_index),storage.data(),required),required);
    return std::u16string(storage.begin(), storage.begin() + required / 2 - 1);
  }
  void Replace(const std::u16string& text, bool keep = false) {
    const auto terminated = Terminated(text);
    (keep ? api.FORM_ReplaceAndKeepSelection : api.FORM_ReplaceSelection)(form,page,terminated.data());
  }
  std::u16string Field(int annotation_index, int page_index = 0) {
    auto p = host->Page(page_index);
    Handle annotation(api.FPDFPage_GetAnnot(p,annotation_index),api.FPDFPage_CloseAnnot);
    EXPECT_NE(annotation.get(),nullptr);
    const auto size = api.FPDFAnnot_GetFormFieldValue(form,annotation.get(),nullptr,0);
    if (!size) return {};
    std::vector<uint16_t> storage(size / 2 + 1,0);
    EXPECT_EQ(api.FPDFAnnot_GetFormFieldValue(form,annotation.get(),storage.data(),size),size);
    return std::u16string(storage.begin(),storage.begin()+size/2-1);
  }
  Raster Draw(int width = 400, int height = 400, int rotation = 0, int flags = 0,
              int origin_x = 0, int origin_y = 0, int view_width = 0, int view_height = 0,
              int page_index = 0) {
    auto p = host->Page(page_index);
    Handle bitmap(api.FPDFBitmap_CreateEx(width,height,FPDFBitmap_BGRA,nullptr,0),api.FPDFBitmap_Destroy);
    if (!bitmap.get()) { ADD_FAILURE() << "Form render bitmap allocation failed"; return {}; }
    EXPECT_TRUE(api.FPDFBitmap_FillRect(bitmap.get(),0,0,width,height,0xffffffff));
    const int vw = view_width ? view_width : width, vh = view_height ? view_height : height;
    api.FPDF_RenderPageBitmap(bitmap.get(),p,origin_x,origin_y,vw,vh,rotation,(flags) | FPDF_NO_NATIVETEXT);
    api.FPDF_FFLDraw(form,bitmap.get(),p,origin_x,origin_y,vw,vh,rotation,(flags) | FPDF_NO_NATIVETEXT);
    Raster raster{width,height,std::vector<uint8_t>(static_cast<size_t>(width)*height*4)};
    const int stride = api.FPDFBitmap_GetStride(bitmap.get());
    const auto* buffer = static_cast<const uint8_t*>(api.FPDFBitmap_GetBuffer(bitmap.get()));
    for(int y=0;y<height;++y) std::copy_n(buffer+static_cast<size_t>(y)*stride,width*4,raster.pixels.begin()+static_cast<size_t>(y)*width*4);
    if(std::getenv("PDFOXIUM_CAPI_FORM_SCALE_DIR")) {
      CaptureFormRenderScales(api,raster,[&](int scale) {
        for(int value:{width,height,origin_x,origin_y,vw,vh}) {
          if(value>std::numeric_limits<int>::max()/scale || value<std::numeric_limits<int>::min()/scale) {
            ADD_FAILURE()<<"Scaled form viewport overflows";return Raster{};
          }
        }
        return Draw(width*scale,height*scale,rotation,flags,origin_x*scale,
                    origin_y*scale,vw*scale,vh*scale,page_index);
      });
    }
    return raster;
  }
  // Dedicated differential rendering. Fixed-size Draw remains available for
  // pixel-contract/visibility assertions whose coordinates must not change.
  Raster DrawForComparison(int width=400,int height=400,int rotation=0,int flags=0,
                           int origin_x=0,int origin_y=0,int view_width=0,int view_height=0,
                           int page_index=0) {
    const double scale=raster_comparison_settings().dpi/72.0;
    const auto scaled=[&](int value) { return static_cast<int>(std::lround(value*scale)); };
    for(int value:{width,height,origin_x,origin_y,view_width,view_height}) {
      if(value*scale>std::numeric_limits<int>::max() || value*scale<std::numeric_limits<int>::min()) {
        ADD_FAILURE()<<"Comparison viewport overflows";return {};
      }
    }
    auto image=Draw(scaled(width),scaled(height),rotation,flags,scaled(origin_x),scaled(origin_y),
                    view_width?scaled(view_width):0,view_height?scaled(view_height):0,page_index);
    image.render_scale=scale;
    return image;
  }
  const Api& api;
  std::vector<uint8_t> bytes;
  std::unique_ptr<FormHost> host;
  FPDF_DOCUMENT document = nullptr;
  FPDF_PAGE page = nullptr;
  FPDF_FORMHANDLE form = nullptr;
  bool xfa_loaded = false;
};

inline void ExpectInk(const Raster& image, int left, int top, int right, int bottom) {
  ASSERT_EQ(image.pixels.size(),static_cast<size_t>(image.width)*image.height*4);
  left=static_cast<int>(std::lround(left*image.render_scale));
  top=static_cast<int>(std::lround(top*image.render_scale));
  right=static_cast<int>(std::lround(right*image.render_scale));
  bottom=static_cast<int>(std::lround(bottom*image.render_scale));
  ASSERT_GE(left,0);ASSERT_GE(top,0);ASSERT_LE(right,image.width);ASSERT_LE(bottom,image.height);
  size_t ink=0;
  for(int y=top;y<bottom;++y) for(int x=left;x<right;++x) {
    const auto i=(static_cast<size_t>(y)*image.width+x)*4;
    ink += image.pixels[i]<200 && image.pixels[i+1]<200 && image.pixels[i+2]<200;
  }
  EXPECT_GT(ink,5u) << "The field must actually draw content, not merely accept API calls";
}

}  // namespace pdfoxium::integration::forms
