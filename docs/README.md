# Development notes

Public usage is in the [workspace README](../README.md),
[safe Rust API](../crates/pdfoxium/README.md),
[CLI](../crates/pdfoxium-cli/README.md) and
[viewer](../crates/pdfoxium-viewer/README.md). Open work is in [TODO.md](../TODO.md).
These notes retain cross-cutting contracts and rationale that are easy to miss
when reading individual modules.

- [Porting and dependency boundaries](porting-notes.md)
- [Safe API ownership and reentry](rust-api-ownership.md)
- [Forms and document workers](rust-api-forms.md)
- [JavaScript runtime and callbacks](javascript-runtime.md)
- [XFA ownership, layout and input](xfa.md)
- [Fonts and subsetting](fonts.md)
- [Font retention budgets](font-memory.md) and [image realization](image-memory.md)
- [Image codecs](image-codecs.md) and [drawn-size decode policy](image-decode-resolution.md)
- [Incremental loading](incremental-loading.md)
- [Progressive rendering](progressive-rendering.md), [region contracts](region-rendering.md),
  and [worker/tile scheduling](render-pool.md)
- [Text indices and Unicode contracts](text-extraction.md)
- [Viewer architecture](viewer.md) and [browser build](viewer-wasm.md)
- [README video recording and publishing](viewer-showcase.md)
- [Release preparation and package checks](release.md)
- [Complete licenses and project attributions](../THIRD_PARTY_LICENSES.md)
- [Optional content and PDF layers](optional-content.md)
- [Dependency compatibility and compiler warnings](dependency-updates.md)
- [Compile features](compile-features.md)
- [Known native-oracle differences](capi-known-native-differences.md)
- [Coverage measurement](capi-coverage.md)

Run the [C API harness](../tests/capi/README.md),
[render corpus](../tests/render_corpus/README.md) and
[focused scripts](../scripts/README.md) as appropriate. Performance methodology
and retained findings are in [benchmarks](../benchmarks/README.md).
