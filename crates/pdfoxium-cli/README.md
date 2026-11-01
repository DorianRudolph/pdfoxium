# pdfoxium CLI

A standalone command-line client of the safe Rust API:

```sh
cargo run --release -p pdfoxium-cli -- info input.pdf
cargo run --release -p pdfoxium-cli -- text input.pdf --pages 1-3
cargo run --release -p pdfoxium-cli -- search input.pdf "term" --whole-word
cargo run --release -p pdfoxium-cli -- search input.pdf "term" --rects
cargo run --release -p pdfoxium-cli -- render input.pdf --jobs 4 --output rendered
cargo run --release -p pdfoxium-cli -- merge a.pdf b.pdf --output merged.pdf
cargo run --release -p pdfoxium-cli -- split input.pdf --pages 3,1-2,3 --output subset.pdf
```

`--help` lists commands; `render --help` lists rendering controls. `--password`
is supported by all commands. For merge it applies to every input; documents
with different passwords should be decrypted/copied through the library API.
Input paths may contain non-UTF-8 characters. Use `--` before positional values
starting with a dash.

`text` writes UTF-8 to stdout, placing a form-feed separator between pages.
`search` prints tab-separated one-based page number, UTF-8 byte range and quoted
matched text. Ranges refer to that page's extracted string, not the combined
stdout of `text`. `--rects` adds grouped highlight rectangles in unscaled page
units; ordinary search does not construct them. Missing matches succeed with
empty stdout. Matching follows the engine's semantics, with optional
`--match-case` and `--whole-word`.

Text/search page lists and split preserve the requested order and duplicates.
Render instead sorts/deduplicates page selection; output filenames identify
source pages, so workers can finish in any order. Parallel rendering shares
immutable source bytes but each worker owns its document and caches.

Merge and split validate input documents and selections before creating output.
They save to a temporary sibling file, then rename it after successful save;
existing output files are replaced, but an input path cannot be used as output.
These commands import page content/resources, not an entire document's catalog,
bookmarks, attachments or interactive form environment. Rendering writes one
PNG at a time; a later error can leave completed PNGs in its output directory.
