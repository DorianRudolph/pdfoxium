# Render corpus

[manifest.json](manifest.json) identifies fixtures, page counts, render settings,
source URLs, coverage and rights. The primary catalog has 46 PDFs / 2,544 pages:
43 small checked-in fixtures (51 pages) and three optional large textbooks.
Hamiltonian's tiny DjVu/JBIG2 conversion is a checked-in exception in `books/`;
the two-page *Right to Privacy* excerpt also preserves converted JBIG2/OCR.

## Fetch, render and compare

```sh
python3 scripts/download-pdfium.py
cargo build --release -p pdfoxium-capi -p pdfoxium-diff
target/release/pdfoxium-diff check bundled
target/release/pdfoxium-diff check clip_path --dpi 300
target/release/pdfoxium-diff check /path/to/input.pdf --page 0
target/release/pdfoxium-diff fetch books
target/release/pdfoxium-diff fetch hayro/local
target/release/pdfoxium-diff check hayro/local --available
```

The [native oracle downloader](../capi/README.md#native-oracle-provenance)
installs the shared library expected by these commands.

Downloading is explicit, timeout-bounded and reuses existing PDFs. Failed/non-PDF
responses do not replace a cached input. External book/Hayro files are ignored.
Publisher URLs may change; recorded expected page counts catch some changes,
not content identity. Record input hashes for comparisons/benchmarks.

| Selector | Meaning |
|---|---|
| `bundled` | Small checked-in fixtures; no external inputs |
| `books` | Hefferon TOC, OBP *Beyond Popular Science*, OpenStax *Biology 2e* |
| `all` | All primary catalog cases, including downloadable books |
| `smoke`, `core`, `real-world` | Manifest subsets |
| `hayro`, `hayro/local` | Whole Hayro catalog / repository-hosted subset |
| `hayro/custom`, `hayro/pdfjs`, `hayro/pdfbox`, `hayro/pdfium`, `hayro/corpus` | Pinned issue collections |
| Case ID, PDF path, directory | Individual inputs or recursive directory selection |
| `workspace` | Catalogued PDFs already present locally |

Multiple selectors form a union. `--available` filters missing inputs; without it
missing files are errors. An empty selection fails except for listing. Hayro
passwords/page ranges and known ignored inputs retain their manifest policy;
`--all-pages` overrides ranges. Page indices are zero-based.

`check` and `diff` render both libraries into BGRA buffers and compare them in
memory. They default to **144 DPI**, two worker processes and
[comparison-baseline.json](comparison-baseline.json). Every catalog case is
listed in that policy; an unlisted catalog entry is an error. Ad hoc paths use
the normal limit. Each worker owns independent native/Rust binding tables,
documents and bitmaps. Long documents are split into at most 64 selected pages
per worker; crashes/timeouts fail the case. No comparison reads cached PNGs.

The gate uses the repository's [raster metric](../../crates/pdfoxium-diff/src/raster/mod.rs):
symmetric radius-two coverage transport, 10% local stroke-contrast leeway, and
the largest eight-connected residual mass above a 0.01 noise floor. The corpus
calibration (`connected-residual-bgra-v2`) uses no additional channel allowance and
divides residual mass by `1 + 15 × local_contrast / 255`. Contrast is the smaller
local range in the two images. This discounts coverage imbalance where both
images already contain a stroke, while missing features or seams on a flat
background cannot provide their own allowance. The normal limit is **2.0**.

The stricter C API `score_bgra()` settings remain unchanged (zero channel
allowance, edge divisor one, caller limit normally 1.0). Corpus calibration is
separate because its unhinted Fontations/AGG outlines intentionally differ from
native FreeType coverage. Controls include `cropped_text`, the Hayro CID font
and blank-page-with-text fixtures. The existing 0.01 noise floor ignores at most two channel levels, but
a four-level flat-fill difference remains a failure for colour-transform investigation.
Focused tests retain failures for missing dots, thin stems, new seams and changed
flat fills. This remains a regression heuristic alongside independent semantic
and exact-pixel tests, rather than proof that either engine renders every PDF
correctly. Raw channel/pixel differences remain in the report.

Colour investigations use isolated source-profile samples, preserved-payload
controls, or independently colour-converted RGB controls. Rewriting a PDF with
its profiles unchanged must retain the discrepancy before trusting a control;
PDF writers can drop a stream when replacing its dictionary. Preserve raw
payloads/filters and verify them. A device-colour-space bypass isolates the
colour transform but does not establish which original colour was correct.

The checked-in JSON records expected page counts, known clean rejections,
invalid geometry, and case/page-specific limits with mandatory explanations.
Numerical exceptions use the observed 144-DPI score plus 1% headroom, rounded up
to 0.001, and bound known differences; they cannot waive dimensions,
page-count disagreements, missing inputs, worker errors or crashes. Page rules
override case rules. Page indices are zero-based. Limits apply unchanged at any
DPI unless a page has an explicitly reviewed `max_error_by_dpi` entry (for
example `"288": 12.5`). The normal limit stays 2.0 at every DPI. Higher-DPI
exceptions are measured and reviewed separately; limits never scale implicitly.

`rejected_pages` names exact page indices and reasons for clean API rejections;
`invalid_geometry` records pages with unusable page dimensions. The benchmark
uses these declarations too, verifies that the expected failures still occur,
and measures the remaining pages. New rejections, different backend inventories,
and worker errors are failures, regardless of numerical visual tolerances.

The baseline is a regression policy, not a claim that every output is identical
or correct. Reasons distinguish native-oracle defects, bounded raster/codec
approximations, reasonable damaged-input recovery and recorded limitations.
Use the triples and relevant source/independent controls when reviewing a new
failure; ordinary runs apply the recorded limits automatically. Do not broaden
the default limit to absorb a missing glyph, shifted feature or colour cast.

```sh
# Fetch explicitly first if necessary; never downloads during checks.
target/release/pdfoxium-diff fetch all hayro
# Establish/check the baseline, then test the same rules at higher resolution.
target/release/pdfoxium-diff check all hayro --dpi 144 --report target/render-diff/144.json
target/release/pdfoxium-diff check all hayro --dpi 288 --jobs 1 --timeout 1800 --report target/render-diff/288.json
```

`--baseline PATH` selects another policy, `--jobs N` bounds concurrency and
`--timeout SECONDS` bounds each worker. The full corpus includes drawings above
500 million pixels at 288 DPI. Two BGRA frames alone require about 4 GB for such
a page, before diff buffers and failure images; the complete higher-resolution
example runs one worker to bound peak memory. Smaller selections can use more.
The longer timeout also accommodates exact transport matching of `issue9679`'s
widespread differences from its inconsistent JPEG/PDF dimensions; this can take
minutes even when rendering is quick.
Reports are checkpointed during the run (at most once per second) and on completion
and include every selected case, page outcomes, residual scores, limits/reasons,
raw differing-pixel counts, maximum/mean channel deltas and comparison time.
**Only failed page pairs write PNGs**, under the report's `.failures/` directory:
`native.png`, `rust.png`, `difference.png`, and **`triple.png`**, with PDFium,
pdfoxium and the raw difference side by side. The triple is full resolution,
composites display images over white, and labels its three columns. Its raw diff
also shows alpha changes: green versus white appears magenta, for example.
Each failure has a `comparison.json` sidecar. An `index.html` gallery in the
failure directory shows all triples, with links to their full resolution. Passing
exceptions still report their measured scores; they do not emit images.
Worker logs/checkpoints live in the report's `.work/` directory. Those directories
are replaced on each run; use distinct report paths to retain separate runs.

For explicit manual image exports, `native` and `rust` retain PNG output under
`target/render-diff/`. Native exports are cached by flags/resolution;
`--refresh` rerenders them. These exports do not participate in the gate.

The opt-in bundled integration runs offline:

```sh
cargo test -p pdfoxium-diff --test render_corpus all_bundled_pdf_pages -- --ignored --exact --nocapture
```

## Focused investigations

`--dpi` selects resolution; `--page` avoids rendering an entire book. The `glyphs`
command uses native text boxes to inspect letter crops; confirm actual glyph
geometry/content rather than relying on whole-page averages. `--flip` adds FLIP metrics to the JSON report as
additional perceptual diagnostics, not a replacement for independent assertions.
See `pdfoxium-diff --help` for report paths and stage profiling.

[Region tests](../../docs/region-rendering.md) compare same-engine tiles against
whole pages at fixed scale. [The C API harness](../capi/README.md) adds independent
semantic/pixel contracts. Font hinting, JPEG repair/detail and malformed-source
recovery can legitimately differ from native; investigate under the documented
[font policy](../../docs/fonts.md), [decode policy](../../docs/image-decode-resolution.md)
and [native exceptions](../../docs/capi-known-native-differences.md).

Negative fixtures use explicit `render`, `reject` or `no-crash` expectations.
Only a clean declared API rejection qualifies; worker crashes/timeouts, missing
files or harness errors are never accepted as rejection. Robustness-only `no-crash` cases exercise both engines without imposing visual
equality. Do not infer known malformed-input policy from
an arbitrary unsuccessful subprocess.

For performance use release builds and [matched-work methodology](../../benchmarks/README.md).
Generated images, reports and profiles belong under ignored `target/` paths.

## Licenses and sources

The manifest's license entries and adjacent notices are authoritative. Keep
original PDFium BSD notices and the separate Roboto/fixture font rights. Original
synthetic fixtures/sources are in `pdfs/` and `sources/`; these are test inputs,
not snapshots of a previous engine's rendered output.

- [Hamiltonian provenance and explicit author permission](books/hamiltonian.NOTICE.md)
- [Right to Privacy source, conversion and public-domain notice](pdfs/right_to_privacy_first_two_pages.NOTICE.md)
- [Hefferon](books/hefferon-toc.NOTICE.md), [OBP](books/obp.0526.NOTICE.md),
  [OpenStax](books/openstax-biology-2e.NOTICE.md)
- [Hayro source/licensing and initialization](hayro.NOTICE.md)

The two DjVu-derived fixtures credit
[djvu2pdf2smol](https://github.com/dorianrudolph/djvu2pdf2smol). Only small
regression inputs are bundled; large books remain explicit local downloads.
