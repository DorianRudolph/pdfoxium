/* Additive pdfoxium extensions. Existing PDFium declarations/ABI are unchanged. */
#ifndef PDFOXIUM_H_
#define PDFOXIUM_H_
#include <stdint.h>
#include "fpdfview.h"
#ifdef __cplusplus
extern "C" {
#endif
/* All calls target the initialized library instance on the calling thread.
 * Byte limits: 0 disables retention; UINT64_MAX is unlimited.
 * Defaults: 64 MiB glyph cache, 4 MiB idle glyph scratch across all fonts/pages.
 * Return 1 on success, 0 if uninitialized, outputs are NULL, or a finite limit
 * does not fit size_t. Getters do not write outputs on failure.
 * These bound accounted retained memory, not active allocations, peak or RSS.
 */
FPDF_EXPORT int FPDF_CALLCONV
PDFOXIUM_SetFontMemoryLimits(uint64_t glyph_cache_bytes, uint64_t glyph_scratch_bytes);
FPDF_EXPORT int FPDF_CALLCONV
PDFOXIUM_GetFontMemoryLimits(uint64_t* glyph_cache_bytes, uint64_t* glyph_scratch_bytes);
FPDF_EXPORT int FPDF_CALLCONV
PDFOXIUM_GetFontMemoryUsage(uint64_t* glyph_cache_bytes, uint64_t* glyph_scratch_bytes);
/* Discard idle font retention without changing limits or page image caches.
 * Active scratch is discarded on return. This never trims the OS allocator. */
FPDF_EXPORT int FPDF_CALLCONV PDFOXIUM_TrimFontMemory(void);
/* Threshold for realizing newly decoded page image color planes (strictly less
 * than the limit). Default 60,000,000 bytes; 0 keeps color planes lazy;
 * UINT64_MAX is unlimited. Existing cached entries, masks and decoder storage
 * are unaffected. This is not an aggregate budget or a rendering quality knob.
 * Requires initialization on the calling thread; same failure rules as above. */
FPDF_EXPORT int FPDF_CALLCONV PDFOXIUM_SetImageMemoryLimits(uint64_t cached_image_bytes);
FPDF_EXPORT int FPDF_CALLCONV PDFOXIUM_GetImageMemoryLimits(uint64_t* cached_image_bytes);
#ifdef __cplusplus
}
#endif
#endif
