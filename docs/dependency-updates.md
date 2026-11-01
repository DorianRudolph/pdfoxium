# Dependency compatibility and compiler diagnostics

The viewer uses eframe/egui 0.37 and wgpu 30; the workspace minimum is Rust 1.95.
The threaded browser build retains its pinned nightly and panic-abort settings.
Keep wasm-bindgen's library, build script, CLI and CI installation versions in
sync (currently 0.2.130).

`icu_casemap` stays exactly 2.3.0: Boa 0.22's parser needs
`icu_properties ~2.3.0`, while casemap 2.4 needs `~2.4.0`. Cargo cannot resolve
both compatible-major versions in this graph. Revisit the pin when Boa supports
ICU 2.4. Transitive requirements are not overridden to force newer major versions.

The maintained Hayro codecs retain their PDFium extensions under distinct
`pdfoxium-*` package names with direct path/version dependencies. The JPEG
snapshot remains vendored and publishable; its pinned revision and source
identity are in `third_party/libjpeg-turbo-rs/UPSTREAM.md` and
[the codec guide](image-codecs.md). Upstream PDFium's pinned revision is unchanged.

Viewer migrations preserve root layout, the short GPU queue, IME output,
document-owned input and asynchronous file handling. Screenshot callbacks send
images to the owning app with revision checks and timeouts. Headless layout
tests discard unused texture uploads to satisfy egui's output ownership
contract. The browser walkthrough resizes the actual window and restores its
bounds afterward.

Run `python3 scripts/check-compiler-warnings.py` for the native workspace and
PDF-only, JS-only and scalar-XFA configurations. `--wasm` checks the threaded
browser viewer and test bridge. Both gates are connected to the existing CI
workflow. The gate examines Cargo JSON diagnostics and fails on warnings in
local crates, including maintained codecs, without suppressing warnings globally
or treating external dependency lint changes as local errors.
The native gate checks moxcms separately with its production feature set rather
than `--all-features`, which would enable upstream's nightly-only SVE option.
All moxcms targets use default features plus the core's `options,any_to_any`;
the other workspace packages retain their all-feature check. Production builds
remain on stable Rust and retain the default NEON/SSE/AVX paths.

XFA assignment cleanup follows pinned upstream callers: neither removed local
value was consumed afterward, and overflow processing remains intact. Native
map accessors used only by ported vectors are test-only. Feature-specific
imports and XFA runtime helpers follow their callers' gates. The dependency
upgrade's 36-document/43-page comparison was pixel-identical to its unchanged
main baseline; it was not a new broad corpus or coverage checkpoint.
