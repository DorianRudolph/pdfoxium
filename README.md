# pdfoxium

A PDF engine written in safe\* Rust, translated from [PDFium](https://pdfium.googlesource.com/pdfium/) with AI.

**Features:** Rendering, text extraction and search, annotations, Forms (AcroForms and XFA), JavaScript, editing, layers, compatible C API.

**Performance:** Rendering and search are generally on par or faster than pdfium, ≈30% faster on tested textbooks at 288 DPI on M1 Max.

**Status:** The entire feature set of PDFium has been implemented and tested on macOS and WASM. Other platforms remain to be tested, but I don't expect major issues, there is very little platform-specific code. Differential tests through the C API cover >85% of lines in `pdfoxium-core`.

**Compatability:** pdfoxium does not aim to be bug-compatible or render identical pixels to PDFium. In fact, we do fix some bugs present in upstream pdfium, and implement additional features such as layers or configurable cache limits. PDFium's platform-specific features and skia integration are also not ported. pdfoxium uses its own port of AGG.

**Multithreading:** PDFium famously does not support multithreading, not even with two different documents. pdfoxium can be used safely from multiple threads, but a single document currently cannot be used from multiple threads. True multithreading would require major changes to the code architecture, which unfortunately does not cleanly separate the data from caches and rendering (same as PDFium).

\*dependency crates do contain a small amount of unsafe code.

## Demo Viewer

A bare-bones PDF viewer is implemented in [pdfoxium-viewer](crates/pdfoxium-viewer/README.md).
The walkthrough video demonstrates support of layers, annotations, AcroForms, XFA, JavaScript, and animations.

https://github.com/user-attachments/assets/507e1db3-4e98-41e8-97d9-dbd75ad62fb8

Launch: `cargo run --release -p pdfoxium-viewer`

## Usage


```toml
[dependencies]
pdfoxium = { git = "https://github.com/DorianRudolph/pdfoxium" }
```

Search the first page, highlights each match, adds a text object,
renders a PNG and save an edited copy:

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



## License

This crate is available under MIT license.

Translated files preserve original PDFium's BSD-3-Clause notices.
The viewer also credits Chromium's PDF viewer and PdfCraft's egui architecture
and programmatic control approach.

The [license inventory](THIRD_PARTY_LICENSES.md) contains project links and
versions, with links to full license texts and notices for the Rust dependency
graph, derived code, bundled fonts, runtime and test oracles. 