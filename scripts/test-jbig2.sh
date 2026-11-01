#!/bin/sh
# Compile the actual safe codecs and their unit modules. The excluded vendor
# Cargo manifest otherwise resolves unrelated benchmark/image dev dependencies.
set -eu
cd "$(dirname "$0")/.."
pdfoxium_test_dir=$(mktemp -d "${TMPDIR:-/tmp}/pdfoxium-jbig2-tests.XXXXXX")
trap 'rm -rf "$pdfoxium_test_dir"' EXIT HUP INT TERM
rustc --edition=2024 --crate-name hayro_ccitt --crate-type rlib \
    third_party/hayro-ccitt/src/lib.rs -o "$pdfoxium_test_dir/libhayro_ccitt.rlib"
CARGO_MANIFEST_DIR="$PWD/third_party/hayro-jbig2" \
    rustc --edition=2024 --crate-name hayro_jbig2 --test --cfg 'feature="std"' \
    third_party/hayro-jbig2/src/lib.rs \
    --extern "hayro_ccitt=$pdfoxium_test_dir/libhayro_ccitt.rlib" \
    -o "$pdfoxium_test_dir/jbig2-tests"
"$pdfoxium_test_dir/jbig2-tests" "$@"
