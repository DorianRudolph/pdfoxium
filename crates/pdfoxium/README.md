# pdfoxium

Safe document/page handles over the Rust engine, with owned or borrowed rendering,
text/search, editing/saving and interactive forms. Production dependencies are
Rust-only; no native PDFium library, C/C++ compiler or PDFium submodule is needed
by consumers. This is a 0.1 preview; APIs may change during 0.x.

[API reference](https://docs.rs/pdfoxium) ·
[crates.io](https://crates.io/crates/pdfoxium) ·
[Source and examples](https://github.com/DorianRudolph/pdfoxium/tree/main/crates/pdfoxium/examples)

## Installation

Requires Rust 1.95 or later. Add this to `Cargo.toml`:

```toml
[dependencies]
pdfoxium = "0.1.0"
```

Use `pdfoxium` for document/page handles, interactive forms and render workers.
For the safe `pdfium-render` 0.9.4 interface, use
[pdfoxium-render](https://docs.rs/pdfoxium-render) instead.

## Features and platforms

| Feature | Default | Effect |
|---|---|---|
| `simd` | Yes | SIMD acceleration in the engine and JPEG decoder. |
| `xfa` | Yes | XFA layout and interaction; also enables `js`. |
| `js` | Via `xfa` | Boa JavaScript runtime. Form scripts still require `FormOptions.javascript = true`. |
| `render-profile` | No | Renderer profiling instrumentation for development. |
| `capi-support` | No | Internal interface used by the workspace C ABI; not a stable consumer API. |

For rendering/text/editing without JavaScript or XFA, use
`pdfoxium = { version = "0.1.0", default-features = false, features = ["simd"] }`.
Omit `simd` as well for a scalar build. The public API remains available with
reduced features; engine capabilities change.

Native runtime validation covers macOS arm64 and Linux x64. Windows is not yet
runtime-validated. WebAssembly requires additional thread/worker setup; see the
[browser build guide](https://github.com/DorianRudolph/pdfoxium/blob/main/docs/viewer-wasm.md).
The docs.rs build documents the default native features.

## API guide

Start with `Document::open` or `Document::create`. A `Page` provides rendering,
text extraction, annotations and editing. `RenderConfig` selects scale, rotation
and clipping; `RenderJob` supports cooperative progressive rendering.
`FormSession` owns live form interaction, while `thread::DocumentThread` and
`thread::RenderPool` provide workers for interactive and static documents.
The examples below show these operations.

## Open and render

```rust,no_run
use pdfoxium::{Document, RenderConfig};

fn main() -> pdfoxium::Result<()> {
    let document = Document::open("input.pdf")?;
    let config = RenderConfig::dpi(144.0);
    for index in 0..document.page_count()? {
        let page = document.page(index)?;
        let bitmap = page.render(&config)?;
        bitmap.write_png(format!("page-{:04}.png", index + 1))?;
        page.unload()?;
    }
    Ok(())
}
```

`document.optional_content()?` lists and toggles PDF layers without changing saved
defaults or marking the PDF dirty. Independent `OptionalContentState` contexts
produce immutable worker-compatible render snapshots. The native and WASM viewer
show a Layers dropdown for layered PDFs; see [the layer API and semantics](https://github.com/DorianRudolph/pdfoxium/blob/main/docs/optional-content.md).

Handles are cheap clones of live shared state. Pages keep their document/input
alive. `unload()` releases parsed page content and page-local image caches for
all aliases, preserving logical page identity; document resources have separate
lifetimes. Dropping one handle does not unload a cached page.

Documents/pages/jobs stay on their creating thread. Owned pixels can cross
threads. An active render job reserves its page; conflicting render/edit/unload
returns `Busy`. Dropping the job cancels it. See
[ownership and failure contracts](https://github.com/DorianRudolph/pdfoxium/blob/main/docs/rust-api-ownership.md).

`embedded_thumbnail(PixelFormat::Rgba8)` reads existing `/Thumb` data, returning
`None` if unavailable. It never renders a substitute or adds thumbnails.

## Render targets and regions

Given a loaded `page`, allocate caller-owned pixels from its actual dimensions:

```rust,no_run
# fn main() -> pdfoxium::Result<()> {
# let document = pdfoxium::Document::open("input.pdf")?;
# let page = document.page(0)?;
use pdfoxium::{PixelFormat, PixelsMut, RenderConfig};
let config = RenderConfig::dpi(144.0);
let (width, height) = page.render_size(&config)?;
let stride = width as usize * 4;
let mut bytes = vec![0; stride * height as usize];
let mut pixels = PixelsMut::new(&mut bytes, width, height, stride, PixelFormat::Rgba8)?;
page.render_into(&mut pixels, &config)?;
# Ok(())
# }
```

`render_matrix()` gives the same page-to-bitmap transform used by rendering,
including crop, intrinsic/requested rotation and region translation.
A device clip changes output dimensions/origin, while scale remains based on the
full canvas:

```rust,no_run
# fn main() -> pdfoxium::Result<()> {
# let document = pdfoxium::Document::open("input.pdf")?;
# let page = document.page(0)?;
use pdfoxium::{DeviceRect, RenderConfig};
let config = RenderConfig::dpi(144.0)
    .device_clip(DeviceRect::new(512, 256, 256, 256));
let tile = page.render(&config)?;
# Ok(())
# }
```

Regions must be nonempty and inside the full scaled/rotated page. They render
directly into the selected buffer; no full-page temporary is required. See
[region contracts](https://github.com/DorianRudolph/pdfoxium/blob/main/docs/region-rendering.md).

Progressive rendering uses the same renderer on the same thread:

```rust,no_run
# fn main() -> pdfoxium::Result<()> {
# let document = pdfoxium::Document::open("input.pdf")?;
# let page = document.page(0)?;
# let config = pdfoxium::RenderConfig::dpi(144.0);
use pdfoxium::{pause, Progress, RenderOutput};
use std::time::Duration;
let mut job = page.render_job(&config)?;
while job.resume(&mut pause::budget(Duration::from_millis(16)))? == Progress::Paused {
    // Process application events; this page remains reserved by the job.
}
let RenderOutput::Owned(bitmap) = job.finish()? else { unreachable!() };
bitmap.write_png("page.png")?;
# Ok(())
# }
```

Pause budgets are cooperative, not maximum call-duration guarantees. Borrowed
jobs return their original `PixelsMut` in `RenderOutput::Borrowed`; the buffer
stays exclusively borrowed until completion/drop.

## Static render pool

```rust,no_run
# fn main() -> pdfoxium::Result<()> {
# let document = pdfoxium::Document::open("input.pdf")?;
# let page = document.page(0)?;
use pdfoxium::{Config, RenderConfig};
use pdfoxium::thread::{CancelGroup, Priority, RenderPool, Source};

let pool = RenderPool::new(Source::Path("input.pdf".into()), Config::default(), 4)?;
let view = CancelGroup::new();
let ticket = pool.render_in_group(0, RenderConfig::dpi(144.0), Priority::Visible, &view);
let result = ticket.wait()?; // CLI; use try_take() or await in an application.
result.bitmap.write_png("page.png")?;
# Ok(())
# }
```

Workers share immutable input bytes but keep separate documents/caches. This is
for static documents; live form state requires one document owner. See
[worker/tile scheduling](https://github.com/DorianRudolph/pdfoxium/blob/main/docs/render-pool.md).

## Reading and searching

```rust,no_run
# fn main() -> pdfoxium::Result<()> {
use pdfoxium::{Document, SearchOptions};
let document = Document::open("input.pdf")?;
let metadata = document.metadata()?;
let layout = document.page_layout(0)?; // Dictionary-only; no content parse.
let page = document.page(0)?;
let text = page.text()?;
println!("{}", text.as_str());
for hit in text.search("term", SearchOptions::default())? {
    let hit = hit?;
    println!("{} at {:?}", hit.text(), hit.byte_range());
}
for hit in text.search_with_rects("term", SearchOptions::default())? {
    let hit = hit?;
    // `hit.rects` contains grouped highlight rectangles in page coordinates.
    println!("{:?}", hit.rects);
}
page.unload()?; // `text` and retained search hits remain valid.
# Ok(())
# }
```

Text snapshots own their data and survive page unload. UTF-8 byte ranges refer
to that exact page string, not graphemes or concatenated document text. Search
uses PDFium's case/word/overlap rules rather than regular expressions or full
Unicode case folding. Plain search avoids per-match rectangle work;
`search_with_rects` explicitly requests it.

`select(range)` validates UTF-8 boundaries. Tight `rects()` describe ink;
`display_rects()` merges loose font-metric bounds for continuous selection bands.
Positions from `hit_test`/`select_between` are snapshot-bound. Generated whitespace
can lack geometry, and a partial string selection can cover a whole glyph.

Geometry uses ordinary f32 page units, with Y upward. `/UserUnit` remains
unapplied, including DPI, matching the current engine policy. Rendering applies
the viewport transform; page-space rectangles are not pixel coordinates.
Page/layout/outline/attachment/link models are detached data. `page_layouts()`
can inspect dictionary geometry without parsing content. Returned actions are
not executed. Page/search iterators become `Stale` after document edits.
See [Unicode/C ABI index contracts](https://github.com/DorianRudolph/pdfoxium/blob/main/docs/text-extraction.md).

## Page import and full save

```rust,no_run
# fn main() -> pdfoxium::Result<()> {
use pdfoxium::Document;
let source = Document::open("input.pdf")?;
let output = Document::create()?;
output.import_pages(&source, &[2, 0, 2], 0)?; // Zero-based, ordered, duplicates kept.
output.save_to(&mut std::fs::File::create("subset.pdf")?)?;
# Ok(())
# }
```

Indices are zero-based. Imports preserve order/duplicates and update surviving
page handles' indices without changing identity. They copy page content/resources,
not all catalog objects or an interactive form environment. Self-import and
active leases reject. A malformed-source failure may leave partial insertions;
there is no transaction guarantee. The CLI merge/split commands build a fresh
output and publish a saved temporary file after success.

Saving is explicit. Do not truncate a source file while its document still uses
lazy reads; save to a separate path. No fallible save/generation occurs in `Drop`.
`SaveOptions` adds incremental output, security removal, font subsetting and
version selection. Unsupported/invalid font subsets retain the original font.

## Thread configuration and memory

Install optional configuration before opening documents:

```rust,no_run
# fn main() -> pdfoxium::Result<()> {
use pdfoxium::{Config, FontMemoryLimits, ImageMemoryLimits};
Config {
    font_paths: Some(vec!["/path/to/fonts".into()]),
    font_memory: FontMemoryLimits::default(),
    image_memory: ImageMemoryLimits { cached_image_bytes: 128 * 1024 * 1024 },
}.install()?;
# Ok(())
# }
```

Font budgets limit accounted idle retention, not peak/RSS. Image realization is
a per-plane threshold, not a total cache budget. Unload pages to release their
page-local state. See [font limits](https://github.com/DorianRudolph/pdfoxium/blob/main/docs/font-memory.md) and
[image limits](https://github.com/DorianRudolph/pdfoxium/blob/main/docs/image-memory.md).

## Editing

```rust,no_run
# fn main() -> pdfoxium::Result<()> {
use pdfoxium::{AnnotationSpec, Color, Document, Matrix, Size, StandardFont, TextSpec};
let doc = Document::create()?;
let page = doc.insert_blank_page(0, Size { width: 595.0, height: 842.0 })?;
let font = doc.standard_font(StandardFont::Helvetica)?;
let id = page.add_text(&TextSpec {
    text: "Approved", font: &font, size: 18.0,
    matrix: Matrix::translate(72.0, 720.0), color: Color::BLACK,
})?;
page.transform_object(id, Matrix::translate(20.0, 0.0))?;
let text = page.text()?;
page.add_annotation(AnnotationSpec::highlight(text.select(0..8)?.rects().to_vec()))?;
doc.save_to(&mut std::fs::File::create("edited.pdf")?)?;
# Ok(())
# }
```

Run `cargo run -p pdfoxium --example edit -- edited.pdf` for a complete example.
Dirty content is regenerated before save/unload/import. Failed regeneration
keeps loaded state. Mutations conservatively dirty/invalidate revisions even if
a core error left partial changes; failed writes do not clear the dirty flag.
Text snapshots remain unchanged. The wrapper maintains no undo/rollback history.

Recover and edit existing objects through live handles:

```rust,no_run
# fn main() -> pdfoxium::Result<()> {
# let document = pdfoxium::Document::open("input.pdf")?;
# let page = document.page(0)?;
use pdfoxium::{ObjectKind, Matrix};
for object in page.object_handles()? {
    if object.kind()? == ObjectKind::Path {
        let path = object.path()?;
        println!("{:?}", path.commands); // MoveTo, LineTo, CurveTo, Close
        println!("{:?}", path.segments); // Exact points, segment types and close bits
        let style = object.style()?;
        object.transform(Matrix::translate(10.0, 0.0))?;
    } else if object.kind()? == ObjectKind::Form {
        for child in object.children()? {
            println!("{:?}", child.kind()?);
        }
    }
}
# Ok(())
# }
```

Reordering preserves object identity. Removal, unload/flatten/deletion, ancestor
removal and cross-page moves invalidate old object handles; a move returns a
destination handle. Cross-document/tagged-object moves are rejected without the
required resource/structure remapping. Nested form edits isolate shared serialized
storage. Text replacement needs an encodable font and does not reflow paragraphs.

Annotation appearance objects also use live handles:

```rust,no_run
# use pdfoxium::{Page, AnnotId, Color, Matrix, Result};
# fn edit_appearance(page: &Page, stamp: AnnotId) -> Result<()> {
let info = page.annotation_objects(stamp)?;
let object = page.annotation_object(info[0].id)?;
let path = object.path()?;
object.set_fill_color(Color::rgb(0, 128, 255))?;
object.transform(Matrix::translate(10., 0.))?;
# Ok(())
# }
```

Sibling handles share parsed appearance state. Edits through that context preserve
its handles; unrelated document edits or unload can make them stale. High-level
setters regenerate built-in appearances. Low-level/raw setters leave custom AP
stream management to the caller. Path, text, font/image, mark, attachment and
annotation operations are documented on their types rather than a second API
coverage catalog.

## Forms and XFA

`Document::forms(FormOptions::default())` creates a retained script-free session.
Its `input` drives actual controls; its `render` includes live controls, selection
and caret. Static edits return `Busy` while a live session owns that environment.
`Document::save_to` commits pending input before writing.

JavaScript is opt-in through `FormOptions.javascript`. Install a synchronous
`set_dialog_handler` and call `initialize` to run startup scripts. The owning
`thread::DocumentThread` initializes automatically, handles input/timers/rendering
and sends `DocEvent::Dialog` replies through `answer_dialog`. Its save creates a
new file and rejects existing paths. See [form ownership](https://github.com/DorianRudolph/pdfoxium/blob/main/docs/rust-api-forms.md).

Interactive XFA uses that same owner. Use session page sizes/matrices/rendering
for full-XFA pages, whose layout can change dynamically. `Document::snapshot()`
creates an isolated editing copy without clearing dirty state. External submission
is not exposed by the safe form API.

The standalone [CLI](https://github.com/DorianRudolph/pdfoxium/blob/main/crates/pdfoxium-cli/README.md) provides info/render/text/search/
merge/split. The render example supports page selection and independent-worker
parallelism; run `cargo run --release -p pdfoxium-cli -- render --help`.


## Ownership checked by the compiler

Live document handles and text snapshots stay on their owning thread. Send owned
bitmaps or copied text strings to other threads instead:

```compile_fail
fn needs_send<T: Send>() {}
needs_send::<pdfoxium::Document>();
```

A borrowed render job holds exclusive access to its pixels until completion or
drop:

```compile_fail
use pdfoxium::{Document, PixelFormat, PixelsMut, RenderConfig};
let document = Document::open("input.pdf").unwrap();
let page = document.page(0).unwrap();
let mut bytes = vec![0; 40 * 30 * 4];
let pixels = PixelsMut::new(&mut bytes, 40, 30, 160, PixelFormat::Bgra8).unwrap();
let job = page.render_job_into(pixels, &RenderConfig::default()).unwrap();
bytes.fill(0);
drop(job);
```

## Limits and licenses

The engine uses the AGG renderer; Skia and platform graphics backends are outside
scope. Font selection, rendering and malformed-document handling retain known
limitations; see [unresolved work](https://github.com/DorianRudolph/pdfoxium/blob/main/TODO.md).

This crate is BSD-3-Clause licensed. The package includes `LICENSE` and
`THIRD_PARTY_LICENSES.md` with dependency and asset notices. See the
[complete license inventory](https://github.com/DorianRudolph/pdfoxium/blob/main/THIRD_PARTY_LICENSES.md).
