#include "harness.h"
#include "pinned_test_fonts.h"

#include <cstdlib>
#include <cstring>
#include <iostream>
#include <stdexcept>
#include <utility>

#include <gtest/gtest.h>

#ifdef _WIN32
#define NOMINMAX
#include <windows.h>
#else
#include <dlfcn.h>
#endif

namespace pdfoxium::integration {
namespace {
const Libraries* active_libraries = nullptr;
RasterComparisonSettings comparison_settings;

#ifdef _WIN32
std::string WindowsError(DWORD error) {
  char* message = nullptr;
  const DWORD size = FormatMessageA(
      FORMAT_MESSAGE_ALLOCATE_BUFFER | FORMAT_MESSAGE_FROM_SYSTEM |
          FORMAT_MESSAGE_IGNORE_INSERTS,
      nullptr, error, 0, reinterpret_cast<char*>(&message), 0, nullptr);
  const std::string result = size ? std::string(message, size)
                                  : "Windows error " + std::to_string(error);
  if (message) LocalFree(message);
  return result;
}
#endif
}  // namespace

const RasterComparisonSettings& raster_comparison_settings() {
  return comparison_settings;
}

void set_raster_comparison_settings(int dpi, bool enabled) {
  if (dpi < 18 || dpi > 1152) {
    throw std::invalid_argument("Comparison DPI must be between 18 and 1152");
  }
  comparison_settings = {dpi, enabled};
}

Library::Library(std::string name, const std::filesystem::path& path)
    : name_(std::move(name)), owner_(std::this_thread::get_id()) {
  std::error_code error;
  path_ = std::filesystem::canonical(path, error);
  if (error || !std::filesystem::is_regular_file(path_)) {
    throw std::runtime_error(name_ + ": missing library " + path.u8string());
  }
#ifdef _WIN32
  // The native oracle and Rust C ABI are currently supported on 64-bit Windows.
  static_assert(sizeof(void*) == 8, "Use a 64-bit Windows build");
  handle_ = reinterpret_cast<void*>(LoadLibraryExW(
      path_.c_str(), nullptr,
      LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS));
  if (!handle_) {
    throw std::runtime_error(name_ + ": load " + path_.u8string() + ": " +
                             WindowsError(GetLastError()));
  }
#else
  handle_ = dlopen(path_.c_str(), RTLD_NOW | RTLD_LOCAL);
  if (!handle_) {
    const char* detail = dlerror();
    throw std::runtime_error(name_ + ": load " + path_.u8string() + ": " +
                             (detail ? detail : "unknown loader error"));
  }
#endif
  // Bind every selected export before initializing either implementation.
  // memcpy avoids imposing C++ object-pointer/function-pointer cast rules;
  // these platforms' dynamic-loader ABIs provide same-size function addresses.
#define PDFIUM_API(name)                                      \
  {                                                          \
    void* address = symbol(#name);                            \
    static_assert(sizeof(api_.name) == sizeof(address));      \
    std::memcpy(&api_.name, &address, sizeof(api_.name));       \
  }
#include "api_functions.inc"
#undef PDFIUM_API
}

const Api& Library::api() const {
  if (std::this_thread::get_id() != owner_) {
    throw std::logic_error(name_ + ": API access must stay on its owning thread");
  }
  return api_;
}

void Library::restore_default_font_info() const {
  const auto& functions = api();
  functions.FPDF_SetSystemFontInfo(nullptr);
  if (restored_font_info_)
    functions.FPDF_FreeDefaultSystemFontInfo(restored_font_info_);
  restored_font_info_ = functions.FPDF_GetDefaultSystemFontInfo();
  if (restored_font_info_)
    functions.FPDF_SetSystemFontInfo(restored_font_info_);
}

void Library::release_restored_font_info() const {
  if (!restored_font_info_) return;
  api().FPDF_SetSystemFontInfo(nullptr);
  api().FPDF_FreeDefaultSystemFontInfo(restored_font_info_);
  restored_font_info_ = nullptr;
}

void* Library::symbol(const char* name) const {
#ifdef _WIN32
  const auto address = GetProcAddress(reinterpret_cast<HMODULE>(handle_), name);
  if (!address) {
    throw std::runtime_error(name_ + ": missing export " + name + " in " +
                             path_.u8string() + ": " + WindowsError(GetLastError()));
  }
  void* result = nullptr;
  static_assert(sizeof(result) == sizeof(address));
  std::memcpy(&result, &address, sizeof(result));
  return result;
#else
  dlerror();
  void* address = dlsym(handle_, name);
  const char* error = dlerror();
  if (error || !address) {
    throw std::runtime_error(name_ + ": missing export " + name + " in " +
                             path_.u8string() + ": " +
                             (error ? error : "null symbol address"));
  }
  return address;
#endif
}

Libraries::Libraries(const std::filesystem::path& native_path,
                     const std::filesystem::path& rust_path,bool host_fonts)
    : native("native", native_path), rust("rust", rust_path) {
  if (std::filesystem::equivalent(native.path(), rust.path())) {
    throw std::runtime_error("Comparison requires two different library files");
  }
  FPDF_LIBRARY_CONFIG config{};
  config.version = 2;
  // Ordinary scenarios use a small deterministic inventory. An explicit
  // override, including an empty path list, takes precedence over either mode.
  const char* override_path=std::getenv("PDFOXIUM_CAPI_FONT_PATH");
  if(override_path || !host_fonts) {
    font_path_=override_path?std::string(override_path):
        (std::filesystem::u8path(PDFOXIUM_TEST_ROOT)/"tests/capi/resources/test_fonts").u8string();
    font_paths_[0]=font_path_.empty()?nullptr:font_path_.c_str();
    config.m_pUserFontPaths=font_paths_.data();
  }
  native.api().FPDF_InitLibraryWithConfig(&config);
  rust.api().FPDF_InitLibraryWithConfig(&config);
}

Libraries::~Libraries() {
  rust.release_restored_font_info();
  native.release_restored_font_info();
  rust.api().FPDF_DestroyLibrary();
  native.api().FPDF_DestroyLibrary();
  // No dlclose/FreeLibrary: retain code through TLS teardown and profile writes.
}

const Libraries& libraries() {
  if (!active_libraries) throw std::logic_error("No active comparison session");
  return *active_libraries;
}

int RunWithLibraries(const std::filesystem::path& native_path,
                     const std::filesystem::path& rust_path,bool host_fonts) {
  Libraries session(native_path, rust_path,host_fonts);
  std::cout << "Comparing " << kApiFunctionCount << " typed exports\n"
            << "  native: " << session.native.path().u8string() << '\n'
            << "  rust:   " << session.rust.path().u8string() << std::endl;
  active_libraries = &session;
  struct ResetSession {
    ~ResetSession() {
      // GoogleTest's normal program-end callback also resets the batch. Keep
      // teardown safe when a listener/test lets an exception leave the runner.
      ResetPinnedFontBatches();
      active_libraries=nullptr;
    }
  } reset;
  return RUN_ALL_TESTS();
}

}  // namespace pdfoxium::integration
