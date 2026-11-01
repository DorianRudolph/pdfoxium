# pdfoxium

A PDF engine written in safe Rust, translated from
[PDFium](https://pdfium.googlesource.com/pdfium/). It supports AGG bitmap
rendering, text extraction and search, editing and saving, annotations,
AcroForms, JavaScript through Boa, and XFA. Production dependencies are Rust-only;
native PDFium is used as a test oracle.

https://github.com/user-attachments/assets/507e1db3-4e98-41e8-97d9-dbd75ad62fb8

## Rust example

Add `pdfoxium = "0.1.0"` to your dependencies. This searches the first page,
highlights each match, adds a text object, renders a PNG and saves an edited copy:

```rust
use pdfoxium::{
    AnnotationSpec, Color, Document, Matrix, RenderConfig, SearchOptions, StandardFont, TextSpec,
};

fn main() -> pdfoxium::Result<()> {
    let document = Document::open("input.pdf")?;
    let page = document.page(0)?;

    for hit in page.text()?.search_with_rects("edit", SearchOptions::default())? {
        let hit = hit?;
        println!("Found {:?} at {:?}", hit.text(), hit.byte_range());
        page.add_annotation(AnnotationSpec::highlight(hit.rects))?;
    }

    let font = document.standard_font(StandardFont::Helvetica)?;
    page.add_text(&TextSpec {
        text: "Edited with pdfoxium",
        font: &font,
        size: 12.0,
        matrix: Matrix::translate(72.0, 72.0),
        color: Color::BLACK,
    })?;

    page.render(&RenderConfig::dpi(144.0))?.write_png("page.png")?;
    document.save_to(&mut std::fs::File::create("edited.pdf")?)?;
    Ok(())
}
```

See the [safe API guide](crates/pdfoxium/README.md) for more examples.

## Workspace

| Crate | Purpose |
|---|---|
| [pdfoxium](crates/pdfoxium/README.md) | Safe document, page, render, text, editing and form APIs |
| [pdfoxium-cli](crates/pdfoxium-cli/README.md) | `info`, `render`, `text`, `search`, `merge` and `split` commands |
| [pdfoxium-viewer](crates/pdfoxium-viewer/README.md) | Native and WebAssembly egui/wgpu demo with an app control channel |
| [pdfoxium-render](crates/pdfoxium-render/README.md) | Safe `pdfium-render` API compatibility implemented directly on the Rust core |
| `pdfoxium-core` | Parser, graphics, fonts, codecs, scripting and form engine |
| `pdfoxium-capi` | PDFium-compatible C ABI and additive `PDFOXIUM_*` memory controls |
| `pdfoxium-diff` | Native/Rust rendering comparisons and benchmarks |

The viewer opens its embedded sample picker when launched without a PDF:

```sh
git submodule update --init
cargo run --release -p pdfoxium-viewer
cargo run --release -p pdfoxium-cli -- render input.pdf --output rendered
```

See each crate's README for command options. The viewer is a demonstration of
the engine and its safe API, with navigation, search, selection, annotations,
page editing, AcroForms, interactive/dynamic XFA and animation. The
[recording pipeline](docs/viewer-showcase.md) can generate a replacement README reel.

## Build and test

Requires **Rust 1.95 or newer**. The upstream submodule is needed for development
tests, not for consumer builds. C API consumers also need Clang/libclang.

```sh
python3 scripts/download-pdfium.py
cargo check --workspace --all-targets
cargo test --workspace
scripts/test-jbig2.sh
python3 scripts/test-feature-flags.py
```

Default features are `simd` and `xfa`; `xfa` enables `js`. For a smaller build:

```sh
cargo build --release -p pdfoxium-capi --no-default-features
cargo build --release -p pdfoxium-capi --no-default-features --features simd,js
```

The same features apply to `pdfoxium` and `pdfoxium-core`. See
[feature boundaries](docs/compile-features.md), the
[public C API test harness](tests/capi/README.md), and
[threaded browser build](docs/viewer-wasm.md).
The C ABI build requires Clang/libclang for the pinned public headers;
GoogleTest and C++ are test dependencies only.

[Release preparation](docs/release.md) describes the local package checks and
CI matrix. Nothing is published by those checks.

## Render comparisons and benchmarks

```sh
cargo build --release -p pdfoxium-capi -p pdfoxium-diff
target/release/pdfoxium-diff check bundled
target/release/pdfoxium-diff check input.pdf
target/release/pdfoxium-diff bench bundled --iterations 3
# Optional larger inputs; downloading is always explicit:
target/release/pdfoxium-diff fetch books
target/release/pdfoxium-diff bench books --iterations 3
```

The checked-in corpus has 43 small fixtures / 51 pages. Large books and the
Hayro corpus are downloaded into ignored local caches. The harness separates
page loading, bitmap fill and drawing; PNG encoding and file writes are outside
render timing. See [corpus usage and provenance](tests/render_corpus/README.md)
and [measurement guidance](benchmarks/README.md).

Measured on **Apple M1 Max, 10 CPU cores, 32 GiB, macOS 27.2**, using default
release builds and native PDFium Chromium-8100 AGG (2026-10-31). Speedup is
PDFium time divided by pdfoxium time; higher is faster. Each result sums
per-document medians of three measured passes after one warmup, including
document opening, page loading, bitmap fill and CPU drawing. Allocation,
closing, PNG encoding and GPU upload are excluded; backends run serially.

| Workload | Timed pages | Speedup at 144 DPI | Speedup at 288 DPI |
|---|---:|---:|---:|
| Full selected corpus | 5,536 | **1.59×** | **1.70×** |
| Hefferon | 448 | 1.37× | 1.30× |
| Beyond Popular Science | 570 | 1.18× | 1.28× |
| Biology 2e | 1,475 | 1.45× | 1.32× |

The corpus selects 1,658 PDFs; 12 declared document rejections and five
unrenderable pages are recorded separately. The full timing cohort includes
documented visual differences, including damaged-input recovery and native
defects. A few dense CAD pages substantially affect totals, so book results
are shown separately. Some pages remain slower in pdfoxium. See
[absolute timings, results without declared page exceptions, and methodology](benchmarks/README.md#readme-measurement-provenance).

To benchmark the full downloaded corpus directly:

```sh
target/release/pdfoxium-diff bench all hayro --dpi 144 --iterations 3 --timeout 1800
```

## Compatibility and development

The selected backend is AGG. Ordinary font outlines are unhinted; fonts that
require their interpreter use the documented Fontations policy. Exact FreeType
pixel parity is not a goal. Correctness takes precedence over reproducing
[known native defects](docs/capi-known-native-differences.md).
CFF/CFF2 subsetting and several rendering/recovery issues remain open; see
[TODO.md](TODO.md). Source-port completion and coverage percentages do not imply
universal PDF or raster equivalence.

[Development notes](docs/README.md) cover ownership, callback reentry, dependency
boundaries, rendering contracts and testing. API details live in Rustdoc and
runnable examples rather than a separate implementation plan.

## Pinned baseline and licenses

The pinned upstream revision is in the
[porting notes](docs/porting-notes.md#pinned-upstream-baseline). Oracle download
URLs and checksums are in the
[C API testing guide](tests/capi/README.md#native-oracle-provenance).

Translation crates retain PDFium's BSD-3-Clause notices. Original repository
tooling uses [MIT](LICENSE); adapted files retain their upstream notices.
The viewer also credits Chromium's PDF viewer and PdfCraft's egui architecture
and programmatic control approach.

The [license inventory](THIRD_PARTY_LICENSES.md) contains project links and
versions, with links to full license texts and notices for the Rust dependency
graph, derived code, bundled fonts, runtime and test oracles. Every Cargo archive
includes a complete standalone copy. PDF/font fixtures retain their separate rights notices.
