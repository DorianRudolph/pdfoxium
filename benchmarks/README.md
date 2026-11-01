# Performance work

Keep benchmark outputs in ignored `target/` directories. Compare matching work
and preserve correctness before interpreting timing. Old per-round records,
profiles and superseded optimization reports are not release assets.

## Reproduce

```sh
python3 scripts/download-pdfium.py
cargo build --release -p pdfoxium-capi -p pdfoxium-diff
target/release/pdfoxium-diff bench bundled --iterations 3
target/release/pdfoxium-diff fetch books
target/release/pdfoxium-diff bench books --iterations 3
# Native and Rust text extraction/search, with separate setup and scan stages:
python3 scripts/bench-text.py --output target/text-bench
# Full rendering corpus (fetch all/hayro explicitly first if needed):
target/release/pdfoxium-diff bench all hayro --dpi 144 --iterations 3 --timeout 1800 --report target/bench-144/report.json
target/release/pdfoxium-diff bench all hayro --dpi 288 --iterations 3 --timeout 1800 --reverse-order --report target/bench-288/report.json
```

Use `pdfoxium-diff --help` for page/DPI selection, persistent-memory probes,
profiling and comparison options. The [corpus guide](../tests/render_corpus/README.md)
explains optional book/Hayro downloads and provenance. Ordinary tests never
silently download inputs.

`bench` runs one backend at a time and alternates their order between cases.
Each backend gets one warmup and three measured fresh-document rounds by default.
The same [baseline JSON](../tests/render_corpus/comparison-baseline.json) used by
`check` records rejected pages and invalid geometry. These are excluded from
timing only when the recorded failure actually occurs; valid pages in the same
document are still measured. Unexpected failures or differing backend inventories
remain errors. Numerical visual tolerances never suppress benchmark errors.
Declared document rejections include null handles and documents with zero pages;
both backends must agree on the exclusion.
Use `--reverse-order` for a reversed confirmation, `--page INDEX` for focused
pages, and `--report PATH` to choose the JSON output and adjacent CSV/raw data.
Benchmarking does not repeat the visual gate or use comparison timings.

### Read saved results

```sh
# README speedups, absolute timings, and the ten slowest pages/slowdowns:
target/release/pdfoxium-diff bench-report target/bench-144/report.json target/bench-288/report.json
# Only the Markdown table used in the main README:
target/release/pdfoxium-diff bench-report target/bench-144/report.json target/bench-288/report.json --readme
# Inspect one run; include sub-millisecond native pages in relative rankings:
target/release/pdfoxium-diff bench-report target/bench-288/report.json --top 20 --min-ms 0
```

`bench-report` reads the recorded rounds without loading PDFs, the catalog,
baseline or engine libraries. It validates paired samples and recomputes the
complete-document medians; it does not sum independent page/stage medians for
the README totals. Supply one report per DPI with matching timed case/page
selections, or inspect differing runs separately. The three rankings show
slowest pdfoxium pages, largest positive extra milliseconds, and largest relative
slowdowns. Rankings include page load, fill and drawing, with separate stage
medians shown as native/Rust milliseconds; page indices are zero-based. Relative
rankings default to native page times of at least 1 ms to avoid tiny timings
dominating the list. Use `--top 0` to hide rankings. A timing report alone does
not establish visual correctness or reconstruct the reviewed visual cohorts.

### Measurement controls

- Build before timing. Run backends serially and alternate/reverse their order;
  do not overlap compilation, profiling, tests or other rendering workloads.
- Record CPU/OS, exact builds/features, library and input identity, DPI/flags,
  warmups and sample counts. Distinguish fresh-document, warm-page and cache-hit
  work. Confirm suspicious full-corpus rankings with longer focused runs.
- Report absolute extra milliseconds alongside percentage differences. A huge
  ratio on a microsecond page is often less useful than a shared multi-second
  cost. A corpus screen is target selection, not proof of a regression.
- Separate page load, bitmap fill and drawing. Existing render `total_ms` means
  load + draw; `load_fill_draw_ms` additionally includes timed fill. Allocation,
  PNG encoding and file writes are outside those render timings.
- The printed document total and JSON `summed_document_medians_ms` include
  document opening plus page load, fill and drawing. Each document's median is
  taken over complete measured rounds before summing documents, preserving
  correlations between stages/pages. Closing and bitmap allocation remain outside
  this total; documents without renderable pages do not contribute.
- Keep search parsing/text-page construction, normalization/setup, scanning and
  range/geometry retrieval separate. Faster result retrieval can simply move
  work into setup. Validate match ranges and Unicode, not only hit counts.
- Validate pixels independently. Before/after Rust equality establishes
  preservation, not native correctness. Different JPEG/JPX decode detail or
  different decoded row counts make nominally identical timing incomparable.

## Findings worth retaining

These observations explain current design choices and useful future controls;
they are historical measurements, not current-release throughput guarantees.

| Area | Finding and useful control |
|---|---|
| JPEG | Native demand-driven decoding can stop early for clipped images; Rust's eager decoder still processes all rows. `hayro/pdfjs/issue12841` decoded only 1,463 of 3,792 rows in native at 300 DPI. Do not attribute that gap solely to entropy decoder quality. Direct BGR output showed no useful gain in alternating tests; progressive JPEG remains a distinct target. |
| Image caches | Biology page 170 at 144 DPI benefits greatly from realizing its large converted plane: warm whole/quadrant/tile totals fell about 4.2×/8.7×/11.2× with unlimited realization, costing roughly 90–97 MiB additional RSS. Cold decode remains. This does not justify making unlimited retention the default. |
| ICC/Flate | Biology's CMYK-ICC illustrations and OBP's six large RGB Flate figures are useful image controls. Batched ICC conversion mattered much more than PNG Sub SIMD alone; downsampling filters become important at low zoom. |
| Repeated ICC colours | PDFBox `5403` page 3 has long CMYK runs: about 78% of JPEG samples and 99% of strip samples repeat the preceding pixel. Reusing the preceding direct four-channel LUT result removed most interpolation work without changing pixels; at 288 DPI, page drawing fell from about 378 to 142 ms. Keep dense CMYK and fast RGB controls: neither run caching nor switching colour libraries is a universal speedup. |
| Inline images | `hayro/pdfjs/issue2618` contains 78,916 tiny fax masks. Connecting content/BI dictionaries to the existing document string pool and borrowing keys/names during deferred abbreviation expansion reduced fresh-page parsing from about 534 to 444 ms at 144 DPI. Fax decoding remains substantial in both parsing and drawing; page teardown allocations also show up in whole-process samples but are outside the draw benchmark. |
| Masked spans | `hayro/pdfjs/issue8019` expands to roughly 416 megapixels at 288 DPI. Clipped BGRA painting dominated its draw profile. Validating a complete span and fixing byte order/full-cover mode outside the pixel loop reduced drawing from about 633 to 354 ms with identical pixels. Retain mixed destination-alpha, solid/variable coverage, shifted clip origins and region controls; this is compositing work, not a change to path clipping or AA. |
| Rotated image columns | `hayro/pdfjs/issue12752` page 1 exposed runtime-sized pixel gather/scatter copies: about 34% of Rust main-thread samples were in tiny copy calls. Selecting a const-generic 1/3/4-byte column-composition loop reduced drawing from about 297 to 189 ms at 288 DPI and 115 to 86 ms at 144 DPI (11 samples per backend, reversed order after). The native gather/blend/scatter algorithm and all pixels are preserved; no new allocation or unsafe code is needed. JPEG 2000 decoding is a minority, not the main optimization target. |
| Sparse compositing | AutoCAD `hayro/pdfium/42271177` exposed large absolute costs shared by both engines. Proving transparent source blocks leave the destination unchanged roughly halved its earlier Rust draw cost; retain dense/mixed controls to catch proof/dispatch overhead. |
| Glyphs | Repeated allocation and rebuilding shared clips can dominate small text even when outlines are fast. Source-style clip identity reuse removed hundreds of redundant masks on Hefferon page 125. Cache/scratch limits bound retention, not RSS. |
| JBIG2 | Per-pixel output callbacks and arithmetic reads were separate bottlenecks. Packed row output and proven zero-context MQ shortcuts helped without lossy symbol matching. Verify decoder state, consumed input, EOF and pause boundaries as well as pixels. |
| Calculator colors | Tiny PostScript `roll` operations dominated several shading pages (`issue6364`, `bug1721218_reduced`, PDFBox `1966_2`); profiling JPEG there would target the wrong subsystem. |
| Regions | Exact-edge clipping cost extra curve work. Plain geometric clipping produced only small AA differences under the selected gate; accepted bounds and unresolved rectangle snapping are in [region rendering](../docs/region-rendering.md). Cross-level image zoom must pay for a new decode when detail changes. |
| Search | Fused normalization and implicit identity maps reduced setup cost while preserving supplementary Unicode/ranges. Keep pairing/scalar and case-sensitive controls; the legacy C UCS-2 contract is separate from safe Unicode extraction. |
| Viewer input | Input acknowledgement followed by a separate render request added a UI frame. Processing dirty paint in the same owner turn avoids it. Frame/texture enqueue measurements are not display-presentation latency. |
| Texture preparation | One final `ColorImage` allocation removed a temporary RGBA copy/conversion. Opaque CPU preparation improved about 67–72%, transparent preparation 10–14%; GPU upload did not show a gain. Measure rendering, CPU preparation and upload separately before adding atlases/custom uploads. |

## Focused tools

- `scripts/profile-text-matching.py` and `analyze-matching-samples.py`: macOS
  matching profiles and unprofiled stage timings.
- `scripts/bench-viewer-{search,typing,click}.py`: control-driven readiness/input
  measurements. Run `--help`; they require a graphical desktop.
- `cargo run --release -p pdfoxium-viewer --example texture_pipeline -- target/texture-pipeline.json`:
  render/CPU conversion/upload comparison on identical bitmaps.
- `pdfoxium-diff memory`: persistent-document scroll/zoom with accounted font
  budgets and Linux RSS/allocator observations. RSS includes much more than caches.

Generated reports should describe their scope and limits. Keep a new persistent
note only when it adds a decision, unresolved issue or measurement insight that
cannot be recovered more clearly from the implementation/tests.

## README measurement provenance

The 2026-10-31 README measurements used an Apple M1 Max (10 CPU cores, 32 GiB),
macOS 27.2, Rust 1.99.1, default release features (SIMD, JavaScript and XFA), and
the native Chromium-8100 AGG library, from checkout `1b8ea80`. These full-corpus
reruns include the repeated-colour ICC, inline-image parsing and clipped BGRA
span optimizations described above.

Wall-clock durations of the timed API calls were recorded. The full corpus ran
at both resolutions with one discarded warmup and three
measured fresh-document rounds per backend. Backend order alternated by case,
and the 288-DPI run reversed that order. No visual comparison or PNG work
contributed to these timings. Each run selected 1,658 PDFs and timed 5,536 pages
in 1,644 documents, excluding 12 declared document rejections and five unusable
pages. Both reports have matching native/Rust sample and bitmap inventories.

Times below are seconds of document opening plus page loading, bitmap fill and
drawing, summed over complete-document round medians. Speedup is native time
divided by Rust time; higher is faster.

| Workload | DPI | Timed pages | PDFium (s) | pdfoxium (s) | Speedup |
|---|---:|---:|---:|---:|---:|
| Full selected corpus | 144 | 5,536 | 244.06 | 153.58 | 1.59× |
| Full selected corpus | 288 | 5,536 | 568.60 | 334.51 | 1.70× |
| Without declared page exceptions | 144 | 5,473 | 234.69 | 150.71 | 1.56× |
| Without declared page exceptions | 288 | 5,455 | 555.35 | 328.63 | 1.69× |
| Hefferon | 144 | 448 | 2.76 | 2.01 | 1.37× |
| Hefferon | 288 | 448 | 3.96 | 3.06 | 1.30× |
| Beyond Popular Science | 144 | 570 | 3.57 | 3.02 | 1.18× |
| Beyond Popular Science | 288 | 570 | 6.84 | 5.32 | 1.28× |
| Biology 2e | 144 | 1,475 | 13.95 | 9.65 | 1.45× |
| Biology 2e | 288 | 1,475 | 20.52 | 15.54 | 1.32× |

The full timing cohort includes documented visual differences, including
damaged-input recovery and native defects. The second cohort excludes the
page exceptions in the reviewed 144/288-DPI visual baseline; it is a fixed
selection of pages, not a new full-corpus visual validation of these timing
runs. The optimizations were validated separately with focused native
comparisons and exact before/after Rust renders. Large CAD cases substantially
affect the aggregate, so the README also gives individual book results. These
results describe this setup, rather than a universal speedup across documents
or machines.

Raw JSON/CSV and individual samples are in the ignored `target/bench-144/` and
`target/bench-288/` directories. The commands above generate equivalent reports
on another checkout; they do not download inputs. The CLI reports record the
input paths, library paths and run settings, but do not capture input/library
hashes or power/thermal state during these manual runs.
