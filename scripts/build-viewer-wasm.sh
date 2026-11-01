#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
wasm_toolchain=nightly-2026-10-29
wasm_bindgen_bin="${WASM_BINDGEN:-wasm-bindgen}"
if [[ "$("$wasm_bindgen_bin" --version)" != 'wasm-bindgen 0.2.130' ]]; then
  echo 'Install wasm-bindgen-cli 0.2.130 (cargo install wasm-bindgen-cli --version 0.2.130 --locked).' >&2
  exit 1
fi
cargo "+$wasm_toolchain" build --locked --release --target wasm32-unknown-unknown \
  -Z build-std=std,panic_abort -p pdfoxium-viewer "$@"
mkdir -p target/viewer-wasm
"$wasm_bindgen_bin" target/wasm32-unknown-unknown/release/pdfoxium-viewer.wasm \
  --target web --out-dir target/viewer-wasm --out-name pdfoxium_viewer
cp crates/pdfoxium-viewer/web/index.html crates/pdfoxium-viewer/web/bootstrap.js crates/pdfoxium-viewer/web/_headers target/viewer-wasm/
printf 'Built target/viewer-wasm (serve with scripts/serve-viewer-wasm.py).\n'
