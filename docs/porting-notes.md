# Porting boundaries

Engine modules generally retain PDFium directory/file names under
`crates/pdfoxium-core/src`; module comments identify merged/adapted sources.
Constructor-only C++ wrappers can share Rust representations, but their schema,
script inheritance and behavior must remain concrete. Avoid one empty Rust type
per native header or APIs disconnected from real callers.

## Pinned upstream baseline

The read-only `upstream/pdfium` submodule is pinned to
`f51279699cb59754d1f49aef10eb8e825fe56532` on `chromium/8100`
(PDFium 157.0.8100.0). Keep its revision unchanged. Native oracle download URLs
and checksums are in the [C API testing guide](../tests/capi/README.md#native-oracle-provenance).

## Ownership and representations

Retained objects preserve allocation identity; weak edges replace non-owning
observers/back-pointers. C++ tracing is adapted to explicit stores/owners, not
strong cycles. Native aliasing/observer invalidation and teardown order are
behavioral contracts. [Safe wrapper leases](rust-api-ownership.md),
[JavaScript reentry](javascript-runtime.md) and [XFA stores](xfa.md) explain the
cross-module ownership choices.

Use safe checked input boundaries while preserving meaningful operation order,
rounding, fallbacks and source error paths. PDF strings/source may contain NULs
or lone UTF-16 surrogates; converting everything through Rust Unicode strings
changes contracts. Do not infer portable behavior from a C++ class/function name
without reading its callers.

## Dependency boundaries

| Boundary | Adapter policy |
|---|---|
| Fonts | Fontations metadata/outlines plus the safe raw PostScript adapter; PDFium selection/encoding/metrics remain in engine. Existing PDF glyphs are not reshaped. |
| JavaScript | Boa supplies actual realms, values, coercion, GC roots and exceptions. Keep synchronous PDF/host callback order. |
| ICC | [Vendored moxcms](../third_party/moxcms/UPSTREAM.md), with its accuracy fallback enabled and black-point compensation retained in the direct pipeline. Lab colours preserve fractional inputs and use direct evaluation, avoiding byte quantization and resampled-LUT colour casts. |
| JPEG | Rust libjpeg-turbo adapter; a narrow Zune fallback preserves raw two-component DeviceN images. Eager decoding remains distinct from native demand-driven rows. |
| JBIG2, fax, JPX | Local Hayro extensions retain explicit upstream/PDFium provenance and codec contracts in their `UPSTREAM.md` files. |
| XML/CSS/XFA | Portable native algorithms with explicit Rust ownership; widgets/layout/scripts connect to the same document owner. |
| C API | Unsafe raw handles/callback pointers stay in the FFI crate. Production never falls back to native PDFium. |

Consult [fonts](fonts.md), [image codecs](image-codecs.md) and
[feature boundaries](compile-features.md) before replacing an adapter. Serialized
PDF output may use different valid compression, names or object numbering;
compare semantics, except where exact bytes are explicitly contractual.

## Image caching and numerical proofs

Cache hits must preserve color-space/resource identity, mask state, conversion
flags and decode detail. Dimensions alone cannot prove a malformed JPEG fallback
contains full source detail. Resolve new mappings outside cache borrows and
restore outer request state after reentry. A CMYK-conversion revision rejects
reuse across intervening standard-conversion changes. Editing/reset invalidates
proofs; failed reloads must not publish a new successful key.

The image-to-device transform selects reduction, independently of target bitmap
or tile size. Same-level requests can reuse decodes; differently reduced planes
cannot silently substitute. See [drawn-size policy](image-decode-resolution.md).
Memory thresholds remain retention/realization policies, not pixel policies.

SIMD regrouping is valid only with the original arithmetic semantics. Integer
filter/predictor products and sums may wrap at a defined width; float reorderings
can alter coverage or color. ICC batching must preserve exact input encodings,
Decode tables and gray/CMYK branches. Sparse compositing may skip a block only
when it proves the destination, including hidden RGB/alpha, remains unchanged.
Benchmark results explain why these boundaries matter in
[performance guidance](../benchmarks/README.md).

Content parsing and inline-image dictionaries use the document's existing
string pool through weak references, as native `CPDF_StreamParser` and
`New<CPDF_Dictionary>` do. Sharing key/name byte storage does not share mutable
PDF objects. Abbreviation expansion borrows the original entries and applies
replacements afterward in native sorted order; colliding abbreviated and full
keys must retain that ordering.

## FDE callback edits commit atomically

An edit prepares replacement text/ranges and history before invoking its
synchronous delegate. Cancellation or rejected validation must not leave half an
edit or advance the history cursor. Intentional changes made by a nested host
callback remain; rollback cannot overwrite unrelated reentrant work. Successful
history actions advance before change notification to preserve nested actions.

Retired XFA overflow fragments must sever old paint/tree/node edges explicitly;
removing a DOM node alone does not free Rust arena allocations. Reused fragments
must detach prior-pass child links before rebuilding, while retaining current-
pass continuations and the node identity needed by surviving fragments.

## Font aliases and enumeration

Appearance resources store decoded PDF names. Escape names again when emitting
`Tf`/default-appearance tokens; spaces and literal hash bytes otherwise change
meaning without changing the resource dictionary. Recoloring must preserve
resource identity and decode/reencode correctly.

Charmap enumeration must reject glyph zero and glyph IDs beyond the font's
actual glyph count, just as checked lookup does. An unusable map keeps the native
first-character sentinel; a valid first character beyond the requested limit
returns no entries. This distinction matters when building widths/ToUnicode
maps. Independent tests cover these cases and
[other native differences](capi-known-native-differences.md).
