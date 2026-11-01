#pragma once

#include <filesystem>

namespace pdfoxium::integration {
// Called before the ordinary comparison session is initialized. Every call is
// a fresh process: the native V8 platform cannot be reinitialized after teardown.
int RunLifecycleScenario(const std::filesystem::path& native_path,
                         const std::filesystem::path& rust_path,
                         int version, bool brotli, bool isolate);
}  // namespace pdfoxium::integration
