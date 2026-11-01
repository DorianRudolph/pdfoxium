# Public C API tests

This C++17/GoogleTest harness writes scenarios once and calls both native PDFium
and pdfoxium through a typed dynamic function table. The pinned public headers
supply signatures/calling conventions; neither implementation is linked into
the executable. Native PDFium is an oracle, never a production dependency.

## Build and run

```sh
python3 scripts/download-pdfium.py
cargo build -p pdfoxium-capi
cmake -S tests/capi -B target/capi -DCMAKE_BUILD_TYPE=Debug
cmake --build target/capi --parallel
ctest --test-dir target/capi --output-on-failure
```

Requires CMake 3.24+, Python 3.8+, C++17, the pinned submodule, both shared
libraries and GoogleTest 1.14+. Without an installed GTest, CMake downloads
checksum-pinned 1.17.0. For offline builds set
`FETCHCONTENT_SOURCE_DIR_GOOGLETEST` to an existing source tree.

macOS arm64 and Linux x64 use the downloaded pinned native oracle. Override
`PDFOXIUM_NATIVE_LIBRARY` and `PDFOXIUM_RUST_LIBRARY` in CMake for another
build/profile/platform. The full suite expects the default XFA-enabled ABI;
reduced builds use `scripts/test-feature-flags.py` instead. Windows additionally
needs matching DLL dependencies and configuration; Mac results do not validate
Windows behavior.

Run a focused selection directly:

```sh
target/capi/pdfoxium_capi_tests \
  --native-library vendor/pdfium/chromium-8100/mac-arm64/libpdfium.dylib \
  --rust-library target/debug/libpdfoxium_capi.dylib \
  --gtest_filter='CoverageCompositing.*' --gtest_shuffle
```

The runner accepts the same library paths through environment variables.
`--gtest_list_tests`/`--help` do not load libraries. Missing symbols, identical
libraries and invalid configuration fail startup. `generate_api.py` validates
pinned declaration scope and [eight explicit exclusions](exclusions.tsv) before
emitting the table. No completed-function checklist is needed by the build.

## Native oracle provenance

The downloaded
[bblanchon oracle libraries](https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium/8100)
enable V8/XFA and are never production fallbacks.

`python3 scripts/download-pdfium.py` detects macOS arm64 or Linux x64. Use
`--platform mac-arm64` or `--platform linux-x64` to select the other target.
It verifies the archive and library hashes below, then installs just the shared
library at `vendor/pdfium/chromium-8100/<platform>/libpdfium.{dylib,so}`, matching
the CMake and Rust differential harness defaults. Verified existing libraries
are reused; `--force` reinstalls from a verified archive. Downloads are cached
under ignored `target/pdfium-downloads/`. The libraries are Git-ignored; their
license notices remain tracked. Test runners never download them implicitly.

| Oracle | Original archive SHA-256 | Extracted library SHA-256 |
|---|---|---|
| [macOS arm64](https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8100/pdfium-v8-mac-arm64.tgz) | `6923e58ecb8dc332c01667efbbde258a71385fc3ef736bea4204b87c4d14a83c` | `a6c5d3f87e5ba0015d854eee6c9e7c27d2d5e2548ef9eeb5cd10c556b81cad7f` |
| [Linux x64](https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8100/pdfium-v8-linux-x64.tgz) | `78a68e46d871a52599c98af31a30a433f5441e670c03bba1f6b7a8444d645782` | `5fb3c50fd2b39cc479612c8c8100d83ea8ac6415c84020291e03bd252b57eab6` |

## Ownership and assertions

Initialize each library once per process; ordinary scenarios use fresh documents,
callbacks and environment state on the same owner thread. Keep libraries loaded
until process exit. Independent lifecycle tests use separate processes. CTest
therefore registers the main suite as one invocation, not one process per case.
Do not retain handles across scenarios or release them through the other library.

Use valid public API calls and independent expected state/pixels. Export lookup
or null-only calls are not coverage. Rendering changes must visibly change the
intended output, not only a semantic query. Compare meaningful bytes exactly;
compressed streams, font encodings, object numbers and dictionary order need not
match. Save scenarios reopen both writers' output through both readers, separating
writer and reader behavior.

Investigate differences. [Confirmed native defects](../../docs/capi-known-native-differences.md)
keep correct independent Rust assertions and explicit native observations; do not
widen generic tolerances to hide them. Actual scenarios and assertions live in
the test sources. On failure, `Artifacts` retains PDFs, expected/actual images
and diagnostics under the build's artifact directory.

## Raster comparisons

Exact contracts use all-zero `RasterTolerance`. For explicitly tolerant scenes,
the shared [Rust residual-transport metric](../../crates/pdfoxium-diff/src/raster/mod.rs)
allows small local coverage redistribution/stroke differences, then measures the
strongest connected residual. Missing ink and seams remain detectable even on
a mostly blank page. A global differing-pixel percentage can hide small missing
content; raw max norm over-penalizes harmless coverage differences. Keep explicit
visibility/geometry assertions alongside either comparison mode.

Current constants/scene overrides belong in the metric and test source. The
metric is compiled into a test-only static library and called on in-memory
bitmaps; tests do not need old image-audit executables or saved campaign files.
Passing comparisons avoid encoding images. Region same-engine tests use their
separate [per-channel AA contract](../../docs/region-rendering.md).

`--comparison-dpi N` (18..1152, default 72) controls dedicated scalable XFA
comparison scenes, not every fixed-size API pixel contract. `--skip-raster-diff`
skips shared comparisons/captures while retaining rendering, independent
visibility checks and byte assertions. Such a run is not full visual validation.

## Fonts

Host font lookup can vary by platform. `PDFOXIUM_CAPI_FONT_PATH` supplies an
explicit test font directory to both libraries. Unset uses host defaults; empty
scans no directories. Pinned Arimo and an enumerating theme provider make XFA
calendar/picture tests independent of installed fonts; see
[font fixtures and rights](resources/test_fonts/README.md).

Restore owned font providers after scenarios. `FPDF_SetSystemFontInfo(nullptr)`
uninstalls a provider; it does not recreate the default. Keep callbacks alive
until uninstalled and release them before library destruction.

For diagnostic scale captures, `PDFOXIUM_CAPI_FORM_SCALE_DIR` selects a directory
and `PDFOXIUM_CAPI_FORM_SCALES` selects integer scales 1..16. Captures supplement,
rather than replace, original assertions. Use actual paint clips rather than
clipping text to nominal line boxes; descenders can extend below those boxes.

## Production coverage runs

Use [`scripts/capi-coverage.py`](../../scripts/capi-coverage.py) with a production
library built using the environment from `cargo llvm-cov show-env --sh`. Ensure
the crates actually rebuild with instrumentation; cached ordinary artifacts are
not usable. Do not build unit-test executables into this profile collection.
LLVM tools must match the instrumenting Rust toolchain.

```sh
llvm_bin="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin"
python3 scripts/capi-coverage.py prepare \
  --run-dir target/capi-coverage/run \
  --binary target/debug/libpdfoxium_capi.so --llvm-bin "$llvm_bin"
python3 scripts/capi-coverage.py collect \
  --run-dir target/capi-coverage/run --llvm-bin "$llvm_bin"
python3 scripts/capi-coverage.py analyze \
  --run-dir target/capi-coverage/run --llvm-bin "$llvm_bin"
```

Use the platform's library suffix/path. Override `--runner` and
`--native-library` for other locations. Each run snapshots its binary, hashes
production sources, records settings and isolates per-process profiles. A changed
binary/source tree requires a new directory. Adopt unmanaged profiles only when
their exact originating binary is known.

Collection runs ordinary scenarios plus separate lifecycle configurations. It
retains failure profiles/logs and reports failures. Defaults skip shared raster
diffs at 72-DPI comparison size; `--raster-diff` enables them and
`--comparison-dpi` adjusts scalable scenes. Independent assertions remain active;
comparison-disabled coverage is not visual validation. Inherited form-scale
capture settings are removed and effective settings recorded before execution.

Report the raw core and C API denominators separately. Keep unit/combined
physical-line projections distinct from production integration results. Stable
Rust may supply no branch records; never infer them. See
[measurement scope and the historical baseline](../../docs/capi-coverage.md).
Run `python3 scripts/test_capi_coverage.py` for helper regressions without a new
engine coverage campaign.
