#include "harness.h"
#include "lifecycle_scenarios.h"
#include "pinned_test_fonts.h"

#include <cstdlib>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>

#include <gtest/gtest.h>

#ifdef _WIN32
#define NOMINMAX
#include <windows.h>
#endif

namespace {
std::filesystem::path EnvironmentPath(const char* name) {
#ifdef _WIN32
  const wchar_t* key = std::string(name) == "PDFOXIUM_NATIVE_LIBRARY"
                           ? L"PDFOXIUM_NATIVE_LIBRARY"
                           : L"PDFOXIUM_RUST_LIBRARY";
  const wchar_t* value = _wgetenv(key);
  return value ? std::filesystem::path(value) : std::filesystem::path{};
#else
  const char* value = std::getenv(name);
  return value ? std::filesystem::u8path(value) : std::filesystem::path{};
#endif
}

int RunMain(int argc, char** argv) {
  try {
    auto native = EnvironmentPath("PDFOXIUM_NATIVE_LIBRARY");
    auto rust = EnvironmentPath("PDFOXIUM_RUST_LIBRARY");
    std::vector<char*> remaining{argv[0]};
    bool help = false;
    int lifecycle = -1;
    bool brotli = false;
    bool isolate = false;
    bool host_fonts = false;
    bool explicit_filter = std::getenv("GTEST_FILTER")!=nullptr;
    int comparison_dpi = pdfoxium::integration::RasterComparisonSettings{}.dpi;
    bool raster_diff = true;
    for (int i = 1; i < argc; ++i) {
      const std::string arg = argv[i];
      if (arg == "--native-library" || arg == "--rust-library") {
        if (++i == argc) throw std::runtime_error("Missing path after " + arg);
        (arg == "--native-library" ? native : rust) =
            std::filesystem::u8path(argv[i]);
      } else if (arg == "--comparison-dpi") {
        if (++i == argc) throw std::runtime_error("Missing DPI after " + arg);
        const std::string value = argv[i];
        size_t parsed = 0;
        comparison_dpi = std::stoi(value, &parsed);
        if (parsed != value.size()) throw std::runtime_error("Invalid comparison DPI: " + value);
      } else if (arg == "--skip-raster-diff") {
        raster_diff = false;
      } else if (arg == "--host-fonts") {
        host_fonts = true;
      } else if (arg == "--lifecycle-config") {
        if (++i == argc) throw std::runtime_error("Missing configuration version");
        const std::string value = argv[i];
        if (value == "default") lifecycle = 0;
        else if (value.size() == 1 && value[0] >= '1' && value[0] <= '7') lifecycle = value[0] - '0';
        else throw std::runtime_error("Lifecycle configuration must be default or 1 through 7");
      } else if (arg == "--brotli") {
        brotli = true;
      } else if (arg == "--isolate") {
        isolate = true;
      } else {
        help |= arg == "--help" || arg == "-h" || arg == "-?";
        explicit_filter |= arg == "--gtest_filter" || arg.rfind("--gtest_filter=",0)==0;
        remaining.push_back(argv[i]);
      }
    }
    pdfoxium::integration::set_raster_comparison_settings(comparison_dpi, raster_diff);
    int gtest_argc = static_cast<int>(remaining.size());
    remaining.push_back(nullptr);
    ::testing::InitGoogleTest(&gtest_argc, remaining.data());
    if (help) {
      std::cout << "\nComparison options:\n"
                << "  --native-library PATH  (or PDFOXIUM_NATIVE_LIBRARY)\n"
                << "  --rust-library PATH    (or PDFOXIUM_RUST_LIBRARY)\n"
                << "  --comparison-dpi N     comparison render DPI, 18..1152 (default 72)\n"
                << "  --skip-raster-diff     skip shared raster comparisons and image artifacts\n"
                << "                         independent visual/API byte assertions remain active\n"
                << "  --host-fonts           use host inventory (default filter: SystemFonts.*)\n"
                << "                         ordinary mode excludes SystemFonts.* and uses pinned fonts\n"
                << "  PDFOXIUM_CAPI_FONT_PATH overrides the configured inventory in either mode\n"
                << "  --lifecycle-config default|1..7 [--brotli] [--isolate]\n"
                << "  PDFOXIUM_CAPI_ARTIFACT_DIR selects failure-artifact storage\n";
      return 0;
    }
    if (gtest_argc != 1) {
      throw std::runtime_error(std::string("Unknown argument: ") + remaining[1]);
    }
    // Inventory listing must remain complete and must not load either DSO.
    if (::testing::GTEST_FLAG(list_tests)) return RUN_ALL_TESTS();
    if(host_fonts) {
      if(!explicit_filter)::testing::GTEST_FLAG(filter)="SystemFonts.*";
    } else {
      std::string filter=::testing::GTEST_FLAG(filter);
      if(!filter.empty()) {
        const auto separator=filter.find('-');
        if(separator==std::string::npos)filter+="-SystemFonts.*";
        else filter+=(filter.back()=='-'?"":":")+std::string("SystemFonts.*");
        ::testing::GTEST_FLAG(filter)=filter;
      }
    }
    if (native.empty() || rust.empty()) {
      throw std::runtime_error(
          "Specify --native-library PATH and --rust-library PATH, or set "
          "PDFOXIUM_NATIVE_LIBRARY and PDFOXIUM_RUST_LIBRARY");
    }
    if ((brotli && lifecycle < 6) || (isolate && lifecycle < 7)) {
      throw std::runtime_error("Brotli/isolation require lifecycle configuration 6/7 or later");
    }
    std::cout << "Raster comparisons: " << (raster_diff ? "enabled" : "disabled")
              << ", comparison DPI: " << comparison_dpi << std::endl;
    if (lifecycle >= 0) {
      return pdfoxium::integration::RunLifecycleScenario(native, rust, lifecycle, brotli, isolate);
    }
    pdfoxium::integration::InstallPinnedFontBatches();
    return pdfoxium::integration::RunWithLibraries(native, rust,host_fonts);
  } catch (const std::exception& error) {
    std::cerr << "C API integration startup failed: " << error.what() << '\n';
    return 2;
  }
}

#ifdef _WIN32
std::string Utf8(const wchar_t* value) {
  const int size = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, value, -1,
                                      nullptr, 0, nullptr, nullptr);
  if (!size) throw std::runtime_error("Invalid Unicode command-line argument");
  std::string result(static_cast<std::size_t>(size), '\0');
  if (!WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, value, -1,
                           result.data(), size, nullptr, nullptr)) {
    throw std::runtime_error("Cannot decode Unicode command-line argument");
  }
  result.pop_back();
  return result;
}
#endif
}  // namespace

#ifdef _WIN32
int wmain(int argc, wchar_t** argv) {
  try {
    // Preserve Unicode DLL paths instead of interpreting the Windows active
    // code page as UTF-8. The common parser and GoogleTest receive UTF-8.
    std::vector<std::string> encoded;
    encoded.reserve(argc);
    for (int i = 0; i < argc; ++i) encoded.push_back(Utf8(argv[i]));
    std::vector<char*> arguments;
    for (auto& argument : encoded) arguments.push_back(argument.data());
    arguments.push_back(nullptr);
    return RunMain(argc, arguments.data());
  } catch (const std::exception& error) {
    std::cerr << "C API integration startup failed: " << error.what() << '\n';
    return 2;
  }
}
#else
int main(int argc, char** argv) { return RunMain(argc, argv); }
#endif
