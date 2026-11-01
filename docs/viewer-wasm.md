# Threaded browser viewer

The browser build uses the same egui viewer and Rust AGG engine as the native
application. The page fills the viewport with the viewer; the file input and
startup-error element are hidden during ordinary use. All six original sample
PDFs are embedded in WASM, including a two-page [layers example](optional-content.md). The Layers toolbar dropdown appears for PDFs with groups. File → Open, Cmd/Ctrl+O and canvas file drops open local
PDF bytes. Save a copy downloads a PDF. Page import and PNG insertion use browser
file pickers. The invisible egui IME helper ignores pointer input so repeated
clicks reach PDF controls, while keyboard/composition input still works. Files are processed locally, without uploading their contents.

## Build and run

From the repository root:

```sh
rustup toolchain install nightly-2026-10-29 --profile minimal --component rust-src
cargo install wasm-bindgen-cli --version 0.2.130 --locked
scripts/build-viewer-wasm.sh
python3 scripts/serve-viewer-wasm.py
```

Open <http://127.0.0.1:8080/>. The output is `target/viewer-wasm/`; serve that
whole directory, including any generated snippets. `WASM_BINDGEN` can point at a
specific CLI executable. The script uses the pinned nightly explicitly; native
workspace builds continue to use the developer's normal toolchain. Cargo's
lockfile pins the browser dependencies. No Trunk, npm bundler or wasm-opt is
required. Compress WASM/JS responses in production hosting.

The host must provide a secure context (HTTPS or localhost) and these response
headers, including on the HTML document:

```http
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

The supplied development server sets both. `web/_headers` supplies the equivalent
static-host configuration for hosts that understand that format. Other hosts
must configure the headers themselves; ordinary GitHub Pages cannot set these
headers. The app reports a startup error when cross-origin isolation or
SharedArrayBuffer is unavailable. There is no main-thread rendering fallback.

## Threading and ownership

`wasm-bindgen-spawn` **0.0.7** creates Web Workers over a single shared
`WebAssembly.Memory` backed by SharedArrayBuffer. The `.cargo/config.toml`
WASM-only settings enable atomics, imported shared memory and TLS exports, with a
2 GiB maximum. The build script rebuilds `std` and `panic_abort` with those flags.
The browser bootstrap initializes the dispatcher before the viewer accepts work.

The existing ownership model is retained: loader/edit preparation workers can
start and wait for nested workers; a static PDF has two render workers and a
reader worker. Forms/XFA use the owning document worker plus the reader. Each
engine document, Boa context and font cache stays on its own thread. Owned
bitmaps and shared immutable source bytes cross the existing channels/tickets.
`thread::runtime` is the small launch/join adapter; native spawning still uses
`std::thread`. Native thread names are retained; the browser runtime assigns its
own worker names.

`wasm_sync` supplies the short shared mutexes and condition variables touched by
the UI. It spins for contended short locks on the UI thread and waits atomically
on workers. Startup/channel waits run on background workers. Shutdown first
signals cancellation and wakes queues/dialogs, then joins; UI-thread destruction
awaits joins asynchronously, and worker-ID tracking avoids self-join. GPU/egui
handles are released on the UI thread. Worker repaint callbacks only set a shared
atomic flag, polled every 16 ms by the browser; no egui context or JS-backed UI
callback crosses the worker boundary.

The threaded build retains panic-abort on its pinned nightly. Ordinary invalid
input continues through the engine's Result error paths; a Rust panic/hard worker
abort is a separate recovery limitation. The runtime can terminate an aborted
worker, but Rust stack destructors cannot run in that worker. Native panic
recovery is unchanged. The eframe 0.37/wasm-bindgen 0.2.130 update is validated
with this existing configuration; WASM unwinding has not been validated.

## Portable dependencies

Boa's browser `js` feature supplies browser clocks and entropy while PDF
JavaScript continues to execute in the Rust interpreter. Viewer/form timers use
`web-time`; native builds get its standard-library reexports. The existing
FX_Random algorithm uses the browser clock without an unavailable process ID.
Browser XFA name substitution uses the existing POSIX font-name table.

The browser cannot enumerate OS fonts. Embedded PDF fonts and existing built-in
font substitution remain available; no test font is added as a production
fallback. XFA app creation now precedes theme/font resolution, as in pinned
upstream `CXFA_FFApp`. Theme loading can resolve the document's embedded Helvetica
when host font lookup is unavailable. Existing host-font preference remains for
native themes. The safe API's XFA worker regression explicitly disables system
font directories and still renders the real embedded-font form.

## Browser checks

Build the opt-in smoke bridge, serve it and run the Playwright CLI from the
repository root:

```sh
scripts/build-viewer-wasm.sh --features web-test
python3 scripts/serve-viewer-wasm.py
# In another terminal (create the artifact directory before the walkthrough):
mkdir -p output/playwright/wasm
playwright-cli -s=pdfoxium-wasm open http://127.0.0.1:8080 --headed
playwright-cli -s=pdfoxium-wasm run-code --filename scripts/smoke-viewer-wasm.js
```

The bridge reuses the existing viewer command/inspection implementation in
memory, with no control socket. Its exports are absent from a normal build.
The walkthrough checks nested threads, condition variables, channels, TLS/thread
identity and repeated joins; every embedded PDF; live form input and downloaded
PDF reopening; dynamic XFA pagination and visible changes; animation movement
and pause; browser uploads/drops and unsaved departure decisions; static worker
rendering, navigation/search and viewport sizing. It saves screenshots and PDFs
to the ignored `output/playwright/wasm/` directory and returns a report.
The actual File → Open native chooser is checked separately with the CLI's
`upload` command, since its modal interception interrupts a running script.

After smoke verification, rerun `scripts/build-viewer-wasm.sh` without `web-test`
for the distributable build.
