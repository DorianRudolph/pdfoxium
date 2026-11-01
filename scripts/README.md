# Development tools

Build the relevant release binaries before graphical/performance checks. Write
outputs under `target/` or `output/playwright/`; generated reports are not source.
Use each tool's `--help` for parameters.

## Checks

```sh
python3 scripts/audit-capi-tests.py
python3 scripts/audit-unit-tests.py --check
python3 tests/capi/test_generate_api.py
python3 scripts/test_capi_coverage.py
python3 scripts/test_bench_text.py
python3 scripts/check-release.py
scripts/test-jbig2.sh
python3 scripts/test-feature-flags.py
```

The C API audit validates pinned declaration scope and explicit exclusions;
the unit audit validates specific upstream exclusions. Neither substitutes for
runtime tests or infers coverage for unlisted cases. `test-jbig2.sh` builds the
codec sources directly without unrelated development dependencies; pass a
Rust test-name filter such as `reader::` to select a subset.

`check-release.py --package` builds and verifies all publishable archives locally
without publishing. `generate-licenses.py --fetch` refreshes the complete linked
license inventory; `--check` detects stale dependency/notice records. See the
[release guide](../docs/release.md).

`download-pdfium.py` fetches the checksum-pinned native test oracle; it detects
this host or accepts `--platform linux-x64` / `--platform mac-arm64`. Run it once
before differential tests or benchmarks. See [oracle provenance and installation](../tests/capi/README.md#native-oracle-provenance).

`test-diff.sh` builds the C ABI, runs differential integrations and the small
bundled corpus. See [the harness](../tests/capi/README.md) and
[corpus guide](../tests/render_corpus/README.md) for focused commands.

## Viewer tests and samples

`smoke-viewer*.py` launch a real visible viewer through its authenticated app
control channel. They cover navigation/text, forms/JS/XFA, annotations, pixel
placement, tile policy, thumbnails and demo launch/animation. Related
`check-viewer-*-renders.py` scripts independently inspect saved annotation output.
Keep them as executable regressions rather than duplicating their assertions in
status reports. They require a graphical desktop; screenshots need inspection.

```sh
cargo build --release -p pdfoxium-viewer
python3 scripts/smoke-viewer.py --output target/viewer-smoke
```

`make-viewer-demo.py`, `make-animation-demo.py`, `make-dynamic-xfa-demo.py` and
`make-showcase-navigation.py` regenerate the original demo fixtures under
`crates/pdfoxium-viewer/assets/demos/`. The showcase fixture is also used by the browser smoke test.
For browser build/hosting and `smoke-viewer-wasm.js`, see
[the WASM guide](../docs/viewer-wasm.md).

## Benchmarks and README video

Text profiling, viewer latency and the texture-pipeline example are described in
[benchmark guidance](../benchmarks/README.md). `capi-coverage.py` collects exact-
build production LLVM profiles; its [workflow](../docs/capi-coverage.md) keeps
profiles and assertions separate.

The recorder, encoder, timeline/join helper and validator remain available for
[generating a new README video](../docs/viewer-showcase.md). They preserve a
visible pointer and readable timing; captures and MP4s stay out of Git.

## Generated font data

The `extract-*` scripts regenerate tables/assets from the read-only pinned
PDFium source. Keep source copyright/license notices and compare regenerated
data before committing. They are development generators, not production runtime
dependencies.
