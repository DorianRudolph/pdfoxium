#!/bin/sh
set -eu

cargo build -p pdfoxium-capi
cargo test -p pdfoxium-diff
cargo test -p pdfoxium-diff --test render_corpus all_bundled_pdf_pages -- --ignored --exact --nocapture

