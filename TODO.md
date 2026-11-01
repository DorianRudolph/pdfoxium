# Open work and scope

The selected port covers loading, AGG rendering, editing/saving, import/export,
annotations, text/search, interactive forms, JavaScript and XFA. Source modules
retain PDFium names and notices; exceptions and adapter rationale are in the
[development notes](docs/README.md). The pinned upstream revision is in
[porting notes](docs/porting-notes.md#pinned-upstream-baseline).

## Engine

- **CFF/CFF2 subsetting:** await published Fontations/Skera support. Unsupported
  or invalid subsets retain the original embedded font so saving can continue.
  See [fonts](docs/fonts.md).
- **Malformed SFNT recovery:** `hayro/pdfjs/issue11131_reduced` and
  `bug1650302` lose glyphs/components with unsorted `loca`; `issue2537` differs on
  an invalid `indexToLocFormat`. Recovery remains deferred. Do not describe
  these files as fixed by unrelated font-fallback work.
- **Malformed embedded CMap recovery:** `hayro/pdfjs/issue15292` uses two
  nonbreaking-space codes outside its declared codespaces. Native recovers them;
  Rust paints `.notdef` marks. Recovery is deferred; see
  [the font notes](docs/fonts.md#malformed-embedded-cmaps) and
  [corpus baseline](tests/render_corpus/comparison-baseline.json).
- **Coverage:** the last measured Linux integration build had 85.28% raw core
  line coverage. A 95% stretch goal remains; newer builds need new measurements.
  See [measurement scope](docs/capi-coverage.md).

## Product/API boundaries

- Documents, pages, form sessions and script contexts stay on their owning
  thread. The render pool is for independent static-document workers; live
  form interaction uses one document owner.
- The safe editing API has no general transaction/rollback or undo history.
  Mutations may leave partial changes on error. Cross-document object transfer
  and tagged-object remapping need additional design.
- The viewer remains a demo. Its browser build requires cross-origin isolation
  and panic-abort; aborted worker stacks cannot run Rust destructors.

## Intentional exclusions

Skia, platform graphics backends and native V8 integration are not selected.
The production engine cannot fall back to C/C++ PDFium. Portable behavior takes
precedence over upstream undefined behavior and platform quirks.

The [C ABI exclusions](tests/capi/exclusions.tsv) identify eight unselected
exports. [Upstream unit exclusions](tests/upstream-unit-exclusions.tsv) retain
case-specific scope reasons; they are not evidence that all other tests execute.
Run both audit scripts after changing scope. Actual tests and runtime coverage,
not completion checklists, establish validation.
