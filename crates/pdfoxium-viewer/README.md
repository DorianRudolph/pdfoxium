# pdfoxium viewer

An egui/wgpu demo client for the safe Rust PDF API. It includes continuous pages,
outline/thumbnails/links/history, search and text selection, annotations, page/
content editing, PDF layers, AcroForms with JavaScript, interactive/dynamic XFA
and animation.

```sh
cargo run --release -p pdfoxium-viewer
cargo run --release -p pdfoxium-viewer -- input.pdf
```

Without a file, the embedded sample picker opens. Reopen it from File → Sample
documents. Open/drop a PDF or use Cmd/Ctrl+O; Save a copy writes edited output.
Use the page/zoom controls, Find and Annotations, or the navigation sidebar's
Pages/Outline tabs. Pinch/Ctrl-scroll zoom anchors beneath the cursor. Select
text by dragging and copy with Cmd/Ctrl+C. Search typing highlights without
moving; Enter/next/previous navigates matches with wraparound.

Layered PDFs show a Layers dropdown with visibility, configurations and Reset.
The embedded map sample demonstrates layers and PDF layer-toggle actions.
Ordinary visibility changes do not mark the PDF dirty; see
[optional content](../../docs/optional-content.md).

The annotation panel creates text/shape notes and selection markup, with style,
move/resize and deletion controls. Page editing includes rotation, import, image
insertion and saving. The animated sample starts/pauses on click. Form scripts
use Boa; XFA samples use the same live form owner for input and pagination.

The [browser build](../../docs/viewer-wasm.md) uses shared-memory workers and
requires cross-origin isolation. [Architecture notes](../../docs/viewer.md)
explain worker ownership, rendering policy, pixel placement and invalidation.
The viewer remains a demo rather than a complete editor product.

## Control the running app

App control follows the same command and egui input paths as human interaction;
screenshots capture the actual wgpu framebuffer. It is opt-in:

```sh
mkdir -p "$HOME/.pdfoxium"
chmod 700 "$HOME/.pdfoxium"
cargo run --release -p pdfoxium-viewer -- input.pdf --control "$HOME/.pdfoxium/viewer.json"
```

The new endpoint file contains a loopback port, protocol version and per-launch
token. Existing files/symlinks are refused; Unix mode is 0600. It is removed on
normal shutdown; remove a stale endpoint after a crash. The authenticated client
has the viewer's file access, including screenshot/save paths.

Use the built-in client to discover commands, inspect state, inject input and
capture output:

```sh
target/release/pdfoxium-viewer ui --control "$HOME/.pdfoxium/viewer.json" ui.commands
target/release/pdfoxium-viewer ui --control "$HOME/.pdfoxium/viewer.json" ui.state
target/release/pdfoxium-viewer ui --control "$HOME/.pdfoxium/viewer.json" ui.command '{"id":"view.zoom","zoom":2.0}'
target/release/pdfoxium-viewer ui --control "$HOME/.pdfoxium/viewer.json" ui.wait
target/release/pdfoxium-viewer ui --control "$HOME/.pdfoxium/viewer.json" ui.screenshot '{"path":"/tmp/viewer.png"}'
```

Useful inspections are `ui.inspect`, `ui.pages`, `ui.outline`, `ui.links`,
`ui.thumbnails`, `ui.forms`, `ui.annotations`, `ui.selection` and `ui.search`.
`ui.click`, `ui.pointer`, `ui.drag`, `ui.type`, `ui.key`, `ui.scroll`, `ui.pinch`
and `ui.resize` inject ordinary input. Coordinates are logical window points;
page numbers are one-based, while text ranges are UTF-8 byte ranges. Query
rectangles/IDs before input rather than hardcoding layout positions. The current
protocol is implemented in [control.rs](src/control.rs) and the smoke scripts.

`ui.wait` waits for pending document/text/visible-render work. Settled screenshots
reject intervening revisions; `{"settled":false}` captures live animation with
no completion guarantee. Dirty close/open requests need the same Save/Discard/
Cancel decision as the visible app. Control errors produce unsuccessful client
exit status.

## Validation and video

Build before running graphical tests, and inspect saved screenshots:

```sh
cargo build --release -p pdfoxium-viewer
python3 scripts/smoke-viewer.py --output target/viewer-smoke
```

Run scripts from the repository root. Other `smoke-viewer*.py` regressions cover
individual features; see [development tools](../../scripts/README.md). They use
`--control-only-input` to isolate the test from native pointer/keyboard/focus
changes; that mode still requires a visible window. Browser testing uses the
opt-in in-memory `web-test` bridge described in the WASM guide.

The [README video pipeline](../../docs/viewer-showcase.md) records the real app,
adds a visible mouse/captions and validates a readable reel under one minute.
Raw captures and MP4s stay under ignored `target/` paths.
