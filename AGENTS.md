# PDFium Rust port

## Scope and sources

- Build a correct, useful PDF engine in safe Rust. Use pinned `upstream/pdfium`
  as the implementation guide; keep it read-only and its revision unchanged.
- Loading, AGG rendering, editing/saving, import/export, annotations, text,
  interactive forms, JavaScript and XFA are selected. Skia and platform graphics
  backends are excluded. Current limitations are in [TODO.md](TODO.md).
- Read upstream implementations, headers and relevant callers before fixes.
  Preserve recognizable algorithms, numeric behavior, error paths, fallbacks
  and operation ordering. Correctness takes precedence over upstream bugs or UB.
- Preserve copyright/license notices. Keep dependency and fixture provenance.
  Record meaningful departures in focused [development notes](docs/README.md),
  rather than per-file completion inventories or chronological work logs.

## Ownership and performance

- First-party engine logic must use safe Rust. Keep unavoidable raw pointer
  handling in the documented `pdfoxium-capi` FFI boundary.
- Production runtime dependencies must be Rust-only. C/C++ PDFium and
  `pdfium-render` are test oracles, never production fallbacks; C++/GoogleTest
  are allowed in the integration harness.
- Model ownership and non-owning references accurately. Avoid strong cycles.
  Release interior-mutability borrows before callbacks or script operations
  that can reenter. Preserve identity, invalidation, cancellation, timer
  ownership and teardown. Change threading only for a concrete requirement.
- Preserve algorithmic complexity. Prefer contiguous buffers and borrowed
  slices; avoid repeated allocation, hot-loop indirection and substantial
  clones used to satisfy borrowing. Benchmark concrete performance questions.
- Preserve AGG behavior and PDF font selection, encoding, metrics and positions.
  Do not reshape text whose glyphs/positions are already specified by the PDF.
- Use established Rust libraries behind small adapters. Consult
  [runtime reentry](docs/javascript-runtime.md), [fonts](docs/fonts.md) and
  [porting boundaries](docs/porting-notes.md) before changing shared adapters.

## Tests and validation

- Public C API scenarios are written once in GoogleTest and run against both
  libraries. Follow [the harness design](tests/capi/README.md): fresh scenario
  state, valid API calls, independent assertions and both writers/readers for
  save/reopen tests. Resolving symbols or making null-only calls is not coverage.
- Visual changes must render and inspect/compare native and Rust output and
  assert the intended visible result. Investigate differences rather than
  relaxing tolerances. Keep independent Rust assertions for
  [confirmed native defects](docs/capi-known-native-differences.md).
- Compare semantic results at library boundaries. Valid compressed streams,
  object numbers, font encodings and dictionary order may differ. Retain exact
  assertions where bytes/pixels are the contract.
- Preserve applicable upstream test vectors and error cases. Keep specific
  scope reasons in [unit exclusions](tests/upstream-unit-exclusions.tsv) and
  [C ABI exclusions](tests/capi/exclusions.tsv). Audit with
  `scripts/audit-unit-tests.py` and `scripts/audit-capi-tests.py`; neither audit
  establishes runtime coverage.
- Report coverage with the raw production core denominator visible. Do not
  exclude difficult source paths to meet targets. Follow
  [the coverage workflow](docs/capi-coverage.md).
- Use focused checks during development and workspace checks at coherent
  checkpoints. Rerun affected checks after fixes; broaden only when evidence
  warrants it. Documentation-only work needs reference/diff checks, not engine
  test campaigns. Rendering changes need relevant comparisons; see
  [the corpus workflow](tests/render_corpus/README.md) and
  [benchmark guidance](benchmarks/README.md).

## Workflow

- Commit coherent, validated batches. Do not use subagents unless explicitly
  requested by the user.
- Keep the repository release-focused: code, runnable tests/tooling, concise
  usage, durable design rationale, provenance and unresolved work. Put generated
  reports, captures and profiles under ignored `target/` or `output/playwright/`.
  Do not add completed checklists, duplicate API catalogs or work-log archives.
