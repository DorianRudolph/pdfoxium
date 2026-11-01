# Compile features

`pdfoxium`, `pdfoxium-core` and `pdfoxium-capi` default to `simd` and `xfa`.
`xfa` enables `js`; JS uses Boa. `jpeg-box-simd` is a compatibility alias for
`simd`. Feature flags remove engine/runtime dependencies, not just public names.

Without `js`, no Boa runtime or XFA script bindings are compiled. Script actions
are skipped while non-script `Next` actions still traverse. Script-source queries
and XFA packet inspection remain available because they only inspect PDF data.
The C API also checks its own JS opt-in, so dependency feature unification cannot
accidentally enable scripting through that boundary.

Without `xfa`, XFA layout/widgets, extension storage, callbacks and save hooks
are absent. JavaScript-only builds retain ordinary PDF scripting. JS-free
metadata uses the shared portable calendar math and actual system UTC instant;
clock/sandbox policy remains enforced at callers.

## C ABI

- `FPDF_LoadXFA` remains exported and returns false without XFA.
- `FPDF_BStr_Init`, `FPDF_BStr_Set` and `FPDF_BStr_Clear` are XFA-only, matching
  the pinned headers' `PDF_ENABLE_XFA` declarations.
- Ordinary form/action entry points and script-source inspection remain exported
  without JS. Non-XFA form initialization accepts callback versions 1 and 2;
  XFA builds require version 2.
- Define `PDF_ENABLE_XFA` in C headers for an XFA build. V8 isolate/allocator
  integration exports are intentionally excluded; Boa does not emulate them.

Run `python3 scripts/test-feature-flags.py` for the connected feature matrix.
It builds separate libraries, inspects runtime dependency/export scope and tests
reduced-feature rendering, saving, forms, JS opt-in and metadata. Results go
under `target/feature-flags/`; defaults run last to restore the usual debug
library. The full native C++ comparison suite expects the default XFA-enabled
470-function API and must use that build.
