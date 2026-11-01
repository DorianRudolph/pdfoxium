#pragma once

#include "api.h"

#include <array>
#include <filesystem>
#include <string>
#include <thread>

namespace pdfoxium::integration {

struct RasterComparisonSettings {
  int dpi = 72;
  bool enabled = true;
};

const RasterComparisonSettings& raster_comparison_settings();
// Configure once on the owning thread before running tests. DPI is 18..1152.
void set_raster_comparison_settings(int dpi, bool enabled);

// The OS mapping is deliberately retained until process exit. Native TLS
// destructors and LLVM profile flushing can still refer to either library.
class Library final {
 public:
  Library(std::string name, const std::filesystem::path& path);
  Library(const Library&) = delete;
  Library& operator=(const Library&) = delete;

  const Api& api() const;
  const std::string& name() const { return name_; }
  const std::filesystem::path& path() const { return path_; }

  // Provider tests replace process-wide state. Keep the restored callback
  // table alive until it has been uninstalled, including later XFA scenarios.
  void restore_default_font_info() const;

 private:
  friend struct Libraries;
  void release_restored_font_info() const;
  void* symbol(const char* name) const;

  std::string name_;
  std::filesystem::path path_;
  std::thread::id owner_;
  void* handle_ = nullptr;
  Api api_;
  mutable FPDF_SYSFONTINFO* restored_font_info_ = nullptr;
};

struct Libraries final {
  Libraries(const std::filesystem::path& native_path,
            const std::filesystem::path& rust_path,bool host_fonts=false);
  ~Libraries();
  Libraries(const Libraries&) = delete;
  Libraries& operator=(const Libraries&) = delete;

  Library native;
  Library rust;

 private:
  // PDFium retains the caller's font-path list until library destruction.
  std::string font_path_;
  std::array<const char*,2> font_paths_{};
};

const Libraries& libraries();

// Main and all ordinary GoogleTest scenarios run on this same owning thread.
int RunWithLibraries(const std::filesystem::path& native_path,
                     const std::filesystem::path& rust_path,bool host_fonts=false);

}  // namespace pdfoxium::integration
