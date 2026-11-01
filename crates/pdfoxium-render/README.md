# pdfoxium-render

The safe `pdfium-render` 0.9.4 interface, backed directly by `pdfoxium-core`.
The library is named `pdfium_render`; its public types are available through
`prelude::*`. First-party implementation and tests forbid unsafe Rust. Neither
the `pdfoxium` wrapper nor the C ABI is a production dependency. Consumers do
not need a native PDFium library, a C/C++ compiler or the PDFium submodule.
This is a 0.1 preview; APIs may change during 0.x.

[API reference](https://docs.rs/pdfoxium-render) ·
[crates.io](https://crates.io/crates/pdfoxium-render) ·
[Source](https://github.com/DorianRudolph/pdfoxium/tree/main/crates/pdfoxium-render)

## Installation

Requires Rust 1.95 or later. To keep existing `pdfium_render` imports, replace
the upstream dependency with this package alias:

```toml
[dependencies]
pdfium-render = { package = "pdfoxium-render", version = "0.1.0" }
```

You can also declare `pdfoxium-render = "0.1.0"` directly; the library target is
still named `pdfium_render`. Choose one dependency declaration, and remove the
upstream `pdfium-render` dependency to avoid an import-name collision.

## Open and render

Use `Pdfium::default()` in place of native library discovery and binding setup:

```no_run
# #[cfg(feature = "image_api")]
use pdfium_render::prelude::*;

# #[cfg(feature = "image_api")]
fn main() -> Result<(), PdfiumError> {
    let pdfium = Pdfium::default();
    let document = pdfium.load_pdf_from_file("input.pdf", None)?;
    let config = PdfRenderConfig::new().set_target_width(1200);
    for (index, page) in document.pages().iter().enumerate() {
        page.render_with_config(&config)?.as_image()?
            .save(format!("page-{index}.png"))
            .map_err(|_| PdfiumError::ImageError)?;
    }
    Ok(())
}
# #[cfg(not(feature = "image_api"))]
# fn main() {}
```

## API compatibility

Use `prelude::*` for the public types and extension traits. `Pdfium` loads or
creates a `PdfDocument`; `PdfDocument::pages` provides `PdfPage` handles.
`PdfRenderConfig` controls rendering, and `PdfBitmap` exposes raw pixels or,
with the default features, an `image::DynamicImage`. Text is available through
`PdfPage::text`, editing through object/annotation collections, and output
through `PdfDocument::save_to_file` or `PdfDocument::save_to_bytes`.

For live form event dispatch, JavaScript callbacks and render worker pools,
use [pdfoxium](https://docs.rs/pdfoxium). The compatibility form collections
provide field queries and edits, not that crate's `FormSession` event loop.

The upstream declarations are copied, including explicit lifetimes, collection
references, enums, distinct typed object and annotation wrappers, named iterators,
and common traits. Implementations call core's safe Rust operations. The port
covers document loading/saving, pages and rendering, text/search, paths and images,
object groups and copying, annotations and form fields, metadata and bookmarks,
links/actions/destinations, attachments, catalog, permissions, signatures, fonts,
glyph paths, and custom font providers.

`Pdfium::default()` initializes the core. Native library discovery, C binding
construction/configuration, raw handles, unchecked constructors, and unsafe
thread-safety implementations are excluded. The experimental paragraph module is
not exported by upstream's prelude. Browser fetch/blob/bitmap declarations are
also copied; `browser-interface-check` compiles them on the host. That check does
not establish a working WASM build of the entire core engine.

[`SAFE_API.json`](https://github.com/DorianRudolph/pdfoxium/blob/main/crates/pdfoxium-render/SAFE_API.json) pins the original public declarations to upstream
commit `ea2961cb8ac777bb1a06dddb8efd272d34d74f18`, with source hashes and explicit
native-binding exclusions. The interface audit checks 167 public definitions,
771 inherent methods, 113 trait methods, enum variants and public record fields.
Transformation methods use the copied upstream macros.

## Features and platforms

| Feature | Default | Effect |
|---|---|---|
| `image` | Yes | Enables `image_api` and the `image_025` compatibility alias. |
| `image_api` | Via `image` | `image` 0.25 conversions such as `PdfBitmap::as_image`. |
| `image_025` | Via `image` | Compatibility alias for `image_api` with `image` 0.25. |
| `browser-interface-check` | No | Compiles browser declarations on a native host for interface checks. |

`default-features = false` removes image conversion support; raw pixel rendering
remains available. This does not disable the underlying core's default SIMD,
JavaScript and XFA features, which are enabled by this crate's core dependency.

Native runtime validation covers macOS arm64 and Linux x64. Windows is not yet
runtime-validated. Browser declarations do not establish a working WebAssembly
build of this adapter and the whole engine. The docs.rs build includes the
browser declarations using `browser-interface-check` on Linux.

## Ownership

Objects retain their core document and share identity across repeated queries.
Moving or detaching an object updates existing wrappers. Page deletion, page
reload and annotation deletion invalidate affected references. Copying imports
resources into the destination, including shared and cyclic indirect references.
Font providers can re-enter core and replace themselves during a callback.

Borrowed bitmaps retain exclusive access to the caller's slice:

```compile_fail
use pdfium_render::prelude::*;
let mut bytes = vec![0; 4];
let bitmap = PdfBitmap::from_bytes(1, 1, PdfBitmapFormat::BGRA, &mut bytes).unwrap();
bytes[0] = 1;
let _ = bitmap.as_raw_bytes();
```

Document handles remain single-threaded:

```compile_fail
use pdfium_render::prelude::*;
fn needs_send<T: Send>(_: T) {}
let engine = Pdfium::default();
let document = engine.create_new_pdf().unwrap();
needs_send(document);
```

## Behavioral corrections

Confirmed upstream bugs are corrected while retaining their interfaces. Form
flags use the PDF specification's bit positions: spell checking and scrolling
are independent. Font-provider serif/script masks test their actual bits.
Grayscale images discard row padding, and `as_rgba_bytes()` returns four channels
as documented. Browser `as_array()` copies into JavaScript-owned memory because
returning an untracked view into Rust memory would require unsafe code.

## Provenance and development

The compatibility interface is pinned to `pdfium-render` 0.9.4 at commit
`ea2961cb8ac777bb1a06dddb8efd272d34d74f18`.
[`SAFE_API.json`](https://github.com/DorianRudolph/pdfoxium/blob/main/crates/pdfoxium-render/SAFE_API.json)
records the declarations and exclusions;
[`UPSTREAM_TESTS.tsv`](https://github.com/DorianRudolph/pdfoxium/blob/main/crates/pdfoxium-render/UPSTREAM_TESTS.tsv)
records the inherited test selection and fixture provenance. Native PDFium is
used only as a development test oracle.

From a repository checkout, run `python3 scripts/audit-pdfium-render-interface.py`
and `cargo test -p pdfoxium-render`. For local API documentation including the
browser declarations, run
`cargo doc -p pdfoxium-render --no-deps --features browser-interface-check`.
See [release preparation](https://github.com/DorianRudolph/pdfoxium/blob/main/docs/release.md)
and [known limitations](https://github.com/DorianRudolph/pdfoxium/blob/main/TODO.md).

The crate combines MIT-licensed upstream interfaces and algorithms with the
BSD-3-Clause PDFium-derived engine integration. The package includes
`LICENSE.pdfium-render`, `LICENSE.pdfium` and `THIRD_PARTY_LICENSES.md`. See the
[complete license inventory](https://github.com/DorianRudolph/pdfoxium/blob/main/THIRD_PARTY_LICENSES.md).
