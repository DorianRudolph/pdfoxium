// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT
// Standalone measurement worker for scripts/bench-text.py. Each process owns
// one library and one document; no rendering, UI, or cross-library globals.
#include "fpdf_text.h"
#include "fpdfview.h"

#include <dlfcn.h>

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <iterator>
#include <limits>
#include <memory>
#include <stdexcept>
#include <string>
#include <thread>
#include <type_traits>
#include <vector>

namespace {
using Clock = std::chrono::steady_clock;
double Ms(Clock::time_point start) {
  return std::chrono::duration<double, std::milli>(Clock::now() - start).count();
}
struct Library {
  void* handle;
#define API(name) decltype(&::name) name = nullptr
  API(FPDF_InitLibrary);
  API(FPDF_DestroyLibrary);
  API(FPDF_LoadMemDocument64);
  API(FPDF_GetLastError);
  API(FPDF_GetPageCount);
  API(FPDF_LoadPage);
  API(FPDF_ClosePage);
  API(FPDF_CloseDocument);
  API(FPDFText_LoadPage);
  API(FPDFText_ClosePage);
  API(FPDFText_CountChars);
  API(FPDFText_GetText);
  API(FPDFText_FindStart);
  API(FPDFText_FindNext);
  API(FPDFText_GetSchResultIndex);
  API(FPDFText_GetSchCount);
  API(FPDFText_FindClose);
#undef API
  explicit Library(const char* path) : handle(dlopen(path, RTLD_NOW | RTLD_LOCAL)) {
    if (!handle) throw std::runtime_error(dlerror());
    try {
#define API(name) Bind(name, #name)
      API(FPDF_InitLibrary); API(FPDF_DestroyLibrary);
      API(FPDF_LoadMemDocument64); API(FPDF_GetLastError); API(FPDF_GetPageCount);
      API(FPDF_LoadPage); API(FPDF_ClosePage); API(FPDF_CloseDocument);
      API(FPDFText_LoadPage); API(FPDFText_ClosePage); API(FPDFText_CountChars);
      API(FPDFText_GetText); API(FPDFText_FindStart); API(FPDFText_FindNext);
      API(FPDFText_GetSchResultIndex); API(FPDFText_GetSchCount);
      API(FPDFText_FindClose);
#undef API
    } catch (...) { dlclose(handle); throw; }
    FPDF_InitLibrary();
  }
  template <typename T> void Bind(T& function, const char* name) {
    void* symbol = dlsym(handle, name);
    if (!symbol) throw std::runtime_error(std::string("missing symbol: ") + name);
    static_assert(sizeof(function) == sizeof(symbol));
    std::memcpy(&function, &symbol, sizeof(function));
  }
  ~Library() { FPDF_DestroyLibrary(); dlclose(handle); }
};
template <typename T, typename Close> auto Own(T handle, Close close) {
  if (!handle) throw std::runtime_error("PDFium returned a null handle");
  return std::unique_ptr<std::remove_pointer_t<T>, Close>(handle, close);
}
// Stable little-endian semantic records, written outside measured intervals.
void U32(std::ostream& stream, uint32_t value) {
  for (unsigned shift = 0; shift < 32; shift += 8) stream.put(char(value >> shift));
}
struct Page {
  int index, characters;
  size_t results;
  double load_ms, text_ms, buffer_ms = 0, operation_ms, close_ms;
};

struct MatchStats {
  double start_ms = 0, scan_ms = 0, close_ms = 0;
  uint64_t matches = 0, digest = 14695981039346656037ull;
};
// Keep a named root so sampling can exclude document preparation and waiting.
__attribute__((noinline)) MatchStats MatchingPass(
    const Library& api, const std::vector<FPDF_TEXTPAGE>& texts,
    const std::vector<unsigned short>& query, unsigned long flags,
    std::ostream* semantics) {
  MatchStats stats;
  for (size_t page = 0; page < texts.size(); ++page) {
    auto start = Clock::now();
    auto search = Own(api.FPDFText_FindStart(texts[page], query.data(), flags, 0), api.FPDFText_FindClose);
    stats.start_ms += Ms(start);
    start = Clock::now();
    std::vector<std::pair<int, int>> results;
    while (api.FPDFText_FindNext(search.get())) {
      const int at = api.FPDFText_GetSchResultIndex(search.get());
      const int length = api.FPDFText_GetSchCount(search.get());
      if (at < 0 || length <= 0) throw std::runtime_error("invalid profile search range");
      stats.digest = (stats.digest ^ uint32_t(at)) * 1099511628211ull;
      stats.digest = (stats.digest ^ uint32_t(length)) * 1099511628211ull;
      ++stats.matches;
      if (semantics) results.emplace_back(at, length);
    }
    stats.scan_ms += Ms(start);
    start = Clock::now();
    search.reset();
    stats.close_ms += Ms(start);
    if (semantics) {
      U32(*semantics, page);
      U32(*semantics, api.FPDFText_CountChars(texts[page]));
      U32(*semantics, results.size());
      for (const auto& range : results) { U32(*semantics, range.first); U32(*semantics, range.second); }
    }
  }
  return stats;
}

void Profile(int argc, char** argv, Library& api, FPDF_DOCUMENT document,
             const std::vector<unsigned short>& query, std::ostream& semantics) {
  if (argc != 11) throw std::runtime_error("profile needs READY GO SECONDS FLAGS after PAGE_LIMIT");
  const double seconds = std::stod(argv[9]);
  const unsigned long flags = std::stoul(argv[10]);
  const int count = api.FPDF_GetPageCount(document), requested = std::stoi(argv[6]);
  if (seconds <= 0 || requested <= 0 || count <= 0) throw std::runtime_error("invalid profile duration/page count");
  const int n = std::min(requested, count);
  std::vector<decltype(Own(FPDF_PAGE{}, api.FPDF_ClosePage))> pages;
  std::vector<decltype(Own(FPDF_TEXTPAGE{}, api.FPDFText_ClosePage))> text_owners;
  std::vector<FPDF_TEXTPAGE> texts;
  std::vector<int> indices;
  for (int i = 0; i < n; ++i) {
    const int index = n == 1 ? 0 : int(int64_t(i) * (count - 1) / (n - 1));
    auto page = Own(api.FPDF_LoadPage(document, index), api.FPDF_ClosePage);
    auto text = Own(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
    texts.push_back(text.get());
    text_owners.push_back(std::move(text));
    pages.push_back(std::move(page));
    indices.push_back(index);
  }
  const auto expected = MatchingPass(api, texts, query, flags, &semantics);
  // The caller starts sampling only after every selected text page is ready.
  { std::ofstream ready(argv[7]); ready << "ready\n"; if (!ready) throw std::runtime_error("ready marker failed"); }
  auto wait = Clock::now();
  while (!std::ifstream(argv[8]).good()) {
    if (Ms(wait) > 30000) throw std::runtime_error("profile start timed out");
    std::this_thread::sleep_for(std::chrono::milliseconds(5));
  }
  MatchStats total;
  uint64_t passes = 0;
  const auto start = Clock::now();
  do {
    const auto sample = MatchingPass(api, texts, query, flags, nullptr);
    if (sample.matches != expected.matches || sample.digest != expected.digest)
      throw std::runtime_error("profile semantic instability");
    total.start_ms += sample.start_ms;
    total.scan_ms += sample.scan_ms;
    total.close_ms += sample.close_ms;
    ++passes;
  } while (Ms(start) < seconds * 1000);
  std::cout << std::setprecision(12) << "{\"passes\":" << passes << ",\"wall_ms\":" << Ms(start)
            << ",\"start_ms\":" << total.start_ms / passes
            << ",\"scan_ms\":" << total.scan_ms / passes
            << ",\"search_close_ms\":" << total.close_ms / passes
            << ",\"matches_per_pass\":" << expected.matches << ",\"flags\":" << flags
            << ",\"page_indices\":[";
  for (size_t i = 0; i < indices.size(); ++i) { if (i) std::cout << ','; std::cout << indices[i]; }
  std::cout << "]}\n";
}

void Run(int argc, char** argv) {
  if (argc != 7 && argc != 11) throw std::runtime_error("expected LIBRARY PDF extract|search|profile QUERY_UTF16 SEMANTICS PAGE_LIMIT [READY GO SECONDS FLAGS]");
  const bool extract = std::string(argv[3]) == "extract";
  const bool profile = std::string(argv[3]) == "profile";
  if (!extract && !profile && std::string(argv[3]) != "search") throw std::runtime_error("unknown operation");
  const int limit = std::stoi(argv[6]);
  if (limit < 0) throw std::runtime_error("negative page limit");
  // File reads, dlopen, initialization and query construction are excluded.
  std::ifstream input(argv[2], std::ios::binary);
  if (!input) throw std::runtime_error("cannot open PDF");
  std::vector<char> bytes((std::istreambuf_iterator<char>(input)), {});
  std::ifstream query_input(argv[4], std::ios::binary);
  std::vector<unsigned short> query;
  while (true) {
    const int low = query_input.get();
    if (low < 0) break;
    const int high = query_input.get();
    if (high < 0) throw std::runtime_error("odd query byte length");
    query.push_back(static_cast<unsigned short>(low | (high << 8)));
  }
  if (!extract && query.empty()) throw std::runtime_error("empty search query");
  query.push_back(0);
  std::ofstream semantics(argv[5], std::ios::binary);
  if (!semantics) throw std::runtime_error("cannot create semantic output");
  Library api(argv[1]);
  auto start = Clock::now();
  auto document = Own(api.FPDF_LoadMemDocument64(bytes.data(), bytes.size(), nullptr), api.FPDF_CloseDocument);
  const double open_ms = Ms(start);
  if (profile) { Profile(argc, argv, api, document.get(), query, semantics); return; }
  const int count = api.FPDF_GetPageCount(document.get());
  if (count <= 0) throw std::runtime_error("document has no pages");
  const int end = limit > 0 ? std::min(count, limit) : count;
  std::vector<Page> pages;
  std::vector<unsigned short> buffer;
  for (int index = 0; index < end; ++index) {
    Page sample{};
    sample.index = index;
    start = Clock::now();
    auto page = Own(api.FPDF_LoadPage(document.get(), index), api.FPDF_ClosePage);
    sample.load_ms = Ms(start);
    start = Clock::now();
    auto text = Own(api.FPDFText_LoadPage(page.get()), api.FPDFText_ClosePage);
    sample.characters = api.FPDFText_CountChars(text.get());
    sample.text_ms = Ms(start);
    if (sample.characters < 0 || sample.characters == std::numeric_limits<int>::max())
      throw std::runtime_error("invalid character count");
    std::vector<std::pair<int, int>> matches;
    if (extract) {
      start = Clock::now();
      buffer.resize(size_t(sample.characters) + 1);
      sample.buffer_ms = Ms(start);
      start = Clock::now();
      const int written = api.FPDFText_GetText(text.get(), 0, sample.characters, buffer.data());
      sample.operation_ms = Ms(start);
      // Empty text pages may return zero rather than a terminator.
      if (written < 0 || written > sample.characters + 1 ||
          (written > 0 && buffer[written - 1] != 0) || (sample.characters && !written))
        throw std::runtime_error("invalid extraction result");
      sample.results = written > 0 ? size_t(written - 1) : 0;
    } else {
      start = Clock::now();
      auto search = Own(api.FPDFText_FindStart(text.get(), query.data(), 0, 0), api.FPDFText_FindClose);
      while (api.FPDFText_FindNext(search.get())) {
        const int at = api.FPDFText_GetSchResultIndex(search.get());
        const int length = api.FPDFText_GetSchCount(search.get());
        if (at < 0 || length <= 0 || at > sample.characters - length ||
            (!matches.empty() && at <= matches.back().first))
          throw std::runtime_error("invalid/non-progressing search range");
        matches.emplace_back(at, length);
      }
      search.reset();
      sample.operation_ms = Ms(start);
      sample.results = matches.size();
    }
    start = Clock::now();
    text.reset();
    page.reset();
    sample.close_ms = Ms(start);
    U32(semantics, index); U32(semantics, sample.characters); U32(semantics, sample.results);
    if (extract) {
      for (size_t i = 0; i < sample.results; ++i) {
        semantics.put(char(buffer[i])); semantics.put(char(buffer[i] >> 8));
      }
    } else {
      for (const auto& match : matches) { U32(semantics, match.first); U32(semantics, match.second); }
    }
    pages.push_back(sample);
  }
  start = Clock::now();
  document.reset();
  const double document_close_ms = Ms(start);
  semantics.close();
  if (!semantics) throw std::runtime_error("failed writing semantic output");
  std::cout << std::setprecision(12) << "{\"document_pages\":" << count
            << ",\"open_ms\":" << open_ms << ",\"document_close_ms\":" << document_close_ms << ",\"pages\":[";
  for (size_t i = 0; i < pages.size(); ++i) {
    const auto& p = pages[i];
    if (i) std::cout << ',';
    std::cout << "{\"page\":" << p.index << ",\"characters\":" << p.characters
              << ",\"results\":" << p.results << ",\"page_load_ms\":" << p.load_ms
              << ",\"text_load_ms\":" << p.text_ms << ",\"buffer_ms\":" << p.buffer_ms
              << ",\"operation_ms\":" << p.operation_ms << ",\"close_ms\":" << p.close_ms << '}';
  }
  std::cout << "]}\n";
}
}  // namespace
int main(int argc, char** argv) {
  try { Run(argc, argv); }
  catch (const std::exception& error) { std::cerr << error.what() << '\n'; return 1; }
}
