# Production coverage

Coverage is measured through the public C API against an instrumented production
shared library, separately from Rust unit tests. Export resolution and the API
scope audit do not establish execution, assertions or parameter coverage.

The last complete Linux measurement used Rust 1.99.1 / LLVM 23.1.1 and the pinned
XFA-enabled native oracle. It is a historical baseline, not a measurement of the
current release tree:

| Scope | Covered / raw lines | Line coverage |
|---|---:|---:|
| Production core | 126,401 / 148,216 | **85.28%** |
| C API crate | 8,463 / 9,114 | **92.86%** |

All 470 selected C functions executed in that campaign. The separately projected
unit/integration physical-line union was 132,493 / 143,854 core lines (92.10%).
That union has a different denominator and must not replace the raw core result.
The 85% checkpoint was met; 95% remains a stretch goal. No core source exclusions
were used. Stable Rust's ordinary instrumentation supplied no branch records.

The measured production library SHA-256 was
`c65eeb0a0a4bd9877d4e8974949b60b4c8249ca2af210aab34a8a83a7cabf062`.
Recompute coverage after code changes rather than carrying this percentage
forward as a current-build claim.

## Reproduction

Use [the C API harness](../tests/capi/README.md#production-coverage-runs) and
`scripts/capi-coverage.py`. The script snapshots the instrumented binary, hashes
production sources, records toolchain/settings and rejects mismatched builds or
profiles. Artifacts belong under `target/capi-coverage/`, not in Git.

Keep the raw core denominator visible. Default callbacks, defensive branches,
private helpers and paths without public callers remain in the measured scope.
Use generated `missed-ranges.tsv` from a fresh matching build to choose further
connected tests; old missed-line rankings are not current reachability evidence.

Unit-test executables, compiler/build-script profiles and profiles from other
production builds must remain separate. A failed test's execution profile can
be useful diagnostically, but does not turn the failed validation into a pass.
Collection with `--skip-raster-diff` still renders and checks independent API/
visibility assertions; it is not a replacement for comparison-enabled visual
validation. DPI and comparison settings must accompany reported measurements.
