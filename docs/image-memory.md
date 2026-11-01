# Image realization limits

`pdfoxium::ImageMemoryLimits` configures the current thread's page-image caches.
The single `cached_image_bytes` field is a threshold for realizing a newly
decoded main image plane into retained converted pixels. The default is
60,000,000 bytes, preserving PDFium's strict comparison: a plane whose
pitch × height equals the threshold remains lazy. Zero disables this realization;
`usize::MAX` permits every representable plane.

This is not a total cache, peak-memory or RSS budget. Lazy images still retain
source/decoder data, masks retain their existing realization policy, and other
PDF resources and render targets have separate lifetimes. The renderer's
resampling thresholds remain unchanged, so changing this setting must preserve
output pixels.

Limits are independent per thread and may change at any time. They apply when
an image finishes decoding; existing realized entries are not evicted or
converted back into lazy entries. Call `page.unload()` to release page-local
caches, then render again to populate them under the new setting. An active
render job prevents unloading. Repeated library initialization preserves limits;
C library destruction followed by initialization restores the defaults.

The C extensions in `crates/pdfoxium-capi/include/pdfoxium.h` are
`PDFOXIUM_SetImageMemoryLimits(uint64_t cached_image_bytes)` and
`PDFOXIUM_GetImageMemoryLimits(uint64_t *cached_image_bytes)`. Both return 1 on
success and 0 before library initialization. The getter rejects a null output;
the setter rejects finite values that cannot fit `size_t`, without changing the
setting. `UINT64_MAX` means unlimited.

The separate `FPDF_RENDER_LIMITEDIMAGECACHE` flag retains its upstream per-page
trim policy. This setting neither replaces nor enables that flag. Viewers still
need to bound loaded pages and retained output tiles independently.
