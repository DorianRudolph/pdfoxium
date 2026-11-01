# Testing

- Rust unit/integration tests: `cargo test --workspace`.
- [Public C API scenarios](capi/README.md): the same valid API workflows against
  native and Rust libraries, with independent semantic/pixel assertions.
- [Render corpus](render_corpus/README.md): small offline fixtures and explicit
  external downloads; same-engine region tests have a separate AA contract.
- Viewer checks: [control-channel smoke scripts](../scripts/README.md), including
  real input and screenshots. [Browser checks](../docs/viewer-wasm.md) use the
  same app through an opt-in in-memory bridge.
- Excluded codec crate checks: `scripts/test-jbig2.sh`; feature combinations:
  `python3 scripts/test-feature-flags.py`.

## Upstream scope

The pinned native tree includes unit, fixture and embedder/integration sources.
Ported Rust tests retain recognizable source names/vectors; the executable tests
are the implementation record. The compact
[upstream unit exclusions](upstream-unit-exclusions.tsv) retain individual reasons
for C++-only machinery, unselected backends/platforms or unreachable APIs.
`scripts/audit-unit-tests.py` validates those entries against the pinned source.
It does not infer coverage for unlisted cases.

The [C ABI exclusions](capi/exclusions.tsv) define the unselected exports.
`scripts/audit-capi-tests.py` checks declaration drift and scope before generating
the typed harness table. Runtime function execution and assertions are separate
from that inventory check. See [coverage methodology](../docs/capi-coverage.md).

Test inputs, expected contract pixels and font/PDF provenance remain checked in.
Generated profiles, campaign CSV/JSON/XML, screenshots and saved diagnostics belong
under ignored `target/` or `output/playwright/`, not in source directories.
