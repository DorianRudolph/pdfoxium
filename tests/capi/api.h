#pragma once

// Use the pinned declarations themselves, including their calling conventions.
#include "api_headers.inc"

#include <cstddef>

namespace pdfoxium::integration {

struct Api {
#define PDFIUM_API(name) decltype(&::name) name = nullptr;
#include "api_functions.inc"
#undef PDFIUM_API
};

inline constexpr std::size_t kApiFunctionCount = 0
#define PDFIUM_API(name) +1
#include "api_functions.inc"
#undef PDFIUM_API
    ;

}  // namespace pdfoxium::integration
