# Preview release preparation

The initial 0.1.0 preview demonstrates the Rust engine, safe API, C ABI and
egui/wgpu viewer. The API may change during 0.x. The native validation targets
are macOS arm64 and Linux x64; Windows is not yet runtime-validated. The browser
viewer requires cross-origin isolation and the pinned nightly build described
in [the browser guide](viewer-wasm.md). Known engine/API limitations remain in
[TODO.md](../TODO.md).

## Local validation

Rust 1.95 is the minimum. Initialize the pinned submodule for development tests;
consumer builds of the engine, CLI, viewer and C ABI do not need it. The C ABI
still requires libclang for binding generation from its bundled public headers.

```sh
cargo fmt --all --check
cargo check --workspace --all-targets --locked
python3 scripts/check-release.py --package
```

`--allow-dirty` permits reviewing packages before committing. The checker
packages all publishable archives, extracts them into a fresh consumer workspace and
builds them together, checking that external dependency versions remain locked.
This avoids Cargo's temporary-registry cache reusing stale sources when a
not-yet-published package keeps the same version. It does **not** publish anything.
Each archive includes the
complete standalone license document, including upstream and transitive dependency
notices. The checked-in [inventory](../THIRD_PARTY_LICENSES.md) links to full
texts in `docs/licenses/`; pages are limited to 250,000 bytes for GitHub browsing.
Large Rust standard-library collections are split at package boundaries, retaining
the original text and order. To refresh it after dependency/asset changes:

```sh
python3 scripts/generate-licenses.py --fetch
python3 scripts/generate-licenses.py --check
```

Manifests use SPDX `license` metadata. Before packaging, the release checker
assembles `target/licenses/THIRD_PARTY_LICENSES.md` and stages identical standalone
copies into the publishable package directories; explicit `include` entries keep
them in each archive even though the copies are Git-ignored. The checker verifies their contents afterward.
Use this checker before distributing archives or publishing. To stage copies
before invoking Cargo packaging directly, run
`python3 scripts/generate-licenses.py --sync-packages`.

The generated inventory includes all platforms/features and development
dependencies, so it is broader than any one shipped binary. Some dependencies
omit license files even in their inspected upstream snapshots; the generated
document identifies the affected versions. The inventory preserves their
manifest declarations and available author metadata alongside the selected
standard MIT or CC0 legal text, without inventing copyright notices.

The maintained codecs use distinct package names (`pdfoxium-ccitt`,
`pdfoxium-jbig2`, `pdfoxium-jpeg2000`, `pdfoxium-jpeg`) with direct, versioned
dependencies. Their upstream identities/revisions and changes remain in each
`third_party/*/UPSTREAM.md`. Consumers receive the same sources when using a Git
dependency; no consumer `[patch]` table is required. Registry publication later
must include these codec packages before the engine and dependent crates.

The [CI workflow](../.github/workflows/ci.yml) checks native builds, the minimum
Rust version, reduced features, API/viewer tests, focused C API comparisons,
license/package verification and the WASM build. It has no publication steps.
Its Linux/macOS jobs must actually pass before claiming those release builds
validated; a local Mac run alone is not Linux validation.

## crates.io and docs.rs documentation

`pdfoxium` and `pdfoxium-render` declare their README and docs.rs URL in package
metadata. Their READMEs are also the crate-level Rustdoc guides; examples are
checked by doctests. Current installation examples use GitHub dependencies while
registry publication is pending; switch them to registry versions when the
packages are published. Repository guides use absolute URLs so links also work
from crates.io and docs.rs.

The docs.rs metadata builds one native target, `x86_64-unknown-linux-gnu`, with
default features. `pdfoxium-render` additionally enables
`browser-interface-check` to document browser declarations without requiring a
WebAssembly engine build. Internal C ABI and profiling features are not enabled.
The settings follow [docs.rs build metadata](https://docs.rs/about/metadata).

Validate the documentation locally before publishing:

```sh
RUSTDOCFLAGS="-D warnings" cargo doc -p pdfoxium -p pdfoxium-render --no-deps \
  --features pdfoxium-render/browser-interface-check --locked
cargo test --doc -p pdfoxium -p pdfoxium-render --locked
cargo test --doc -p pdfoxium --no-default-features --locked
cargo test --doc -p pdfoxium-render --no-default-features --locked
```

These commands check documentation and example compilation on the local host;
they do not reproduce the Linux docs.rs sandbox. Inspect the generated landing
pages at `target/doc/pdfoxium/index.html` and
`target/doc/pdfium_render/index.html`. The latter uses the compatibility library
name, even though its package is `pdfoxium-render`.

Run the package checker above to verify the distributed inputs as well. The
unversioned development-only path to `pdfoxium-diff` is stripped by Cargo when
packaging `pdfoxium-render`; it is an oracle helper, not a registry dependency.
Publish the versioned codec/color dependencies before `pdfoxium-core`, then
publish `pdfoxium` and `pdfoxium-render` after the required core version is
available. [Docs.rs](https://docs.rs/about/builds) builds the API reference after
each crates.io publication;
check its build result before announcing the documentation as available.

## Release contents, when approved

Prepare the publishable Cargo archives and native CLI/viewer/C ABI builds from the
validated commit. Run `python3 scripts/generate-licenses.py --sync-packages` and
include the generated `target/licenses/THIRD_PARTY_LICENSES.md` beside binaries; include the
C API's `include/public/` headers for C consumers. The viewer carries its own
demo assets and opens the sample picker without a path argument.

The short release description should identify 0.1.0 as a preview, list the
rendering/text/editing/form/JS/XFA capabilities, link the README video, document
the supported platform builds and point to the known limitations. Publishing
crates, uploading artifacts and creating a release/tag are separate actions.
