# Font rendering and subsetting

## Rendering boundary

Fontations (`read-fonts` and `skrifa`) supplies supported SFNT metadata,
charmaps, metrics, and outlines through
[`CfxFace`](../crates/pdfoxium-core/src/fxge/cfx_face.rs). The safe
[`raw_postscript_face`](../crates/pdfoxium-core/src/fxge/raw_postscript_face.rs)
adapter handles raw Type 1/CFF operations. PDFium's font selection, encoding,
CMap, substitution, and positioning logic remains in the engine; existing PDF
text is not reshaped. AGG rasterizes the resulting outlines.

Ordinary outlines are unhinted. Fonts that Skrifa identifies as requiring the
interpreter use embedded instructions with `Target::Mono` at a fixed 1000 ppem;
`prefer_interpreter` alone does not enable hinting. Bitmap masks and extracted
paths share this outline path and normalized geometry. Failure of the required
interpreter returns failure rather than malformed unhinted fallback. The final
AGG rasterization remains antialiased. This is a deliberate departure from FreeType pixel parity.

Generic Chrome Serif/Sans substitutes use their Type 1 Multiple Master weight
and width instances. Width queries, paths, and bitmaps share that selection.
SFNT faces without a usable outline source are rejected so the existing PDF
font substitution path can run; merely accepting their headers can lose text.
Optional-table damage alone does not justify rejecting an otherwise usable face.

Glyph retention and scratch budgets are documented separately in
[font memory controls](font-memory.md). Exact FreeType pixel coverage is not an
acceptance requirement; missing or malformed glyph structure needs investigation.

## Malformed embedded CMaps

`hayro/pdfjs/issue15292` declares an Encoding CMap codespace of
`<0001> <0192>` but uses `<00A0>` (nonbreaking space), explicitly mapped to CID
98. Codespace bounds apply to each byte independently ([ISO 32000-2,
§9.7.6.2](https://developer.adobe.com/document-services/docs/assets/5b15559b96303194340b99820d3a70fa/PDF_ISO_32000-2.pdf));
`A0` exceeds the second-byte upper bound `92`, and no other declared range
accepts that code. Rust rejects it as character code zero and draws CID 0's
`.notdef` outline. Native's partial-prefix recovery accepts the two-byte code
and leaves the space blank. The two footer marks are therefore malformed-CMap
recovery differences, not a missing raw CID Type 1 outline feature or an
antialiasing difference. Recovery remains deferred in [TODO.md](../TODO.md).
Do not undo the valid shared-prefix decoding fix to recover this input; see
[the native CMap difference](capi-known-native-differences.md#cmaps-sharing-partial-prefixes).

## Host font lookup

On macOS the safe folder provider follows `CFX_MacFontInfo` in pinned
`core/fxge/apple/capple_platform.cpp`, including canonical Base14 substitutions,
case-sensitive Bold/Italic suffix lookup before exact names, ANSI fixed-pitch
fallback, and the named Japanese/Chinese/Korean fallbacks. It does not use the
Linux provider's family similarity scoring. Initial graphics setup and
`FPDF_GetDefaultSystemFontInfo` use the same platform selection.

Explicit user font paths replace defaults, including an explicitly empty list.
The native default order is `~/Library/Fonts`, `/Library/Fonts`, then
`/System/Library/Fonts`. The pinned POSIX scanner passes the first string to
`opendir` literally, without expanding `~`; the Rust scanner preserves that
behavior. Callers can provide an absolute user-font directory explicitly.
Quartz/native glyph drawing remains excluded, and `FPDF_NO_NATIVETEXT` has no
effect on the Rust AGG output. The existing platform-specific XFA replacement
tables are unchanged; matching font selection does not imply identical FreeType
and Fontations/AGG glyph coverage.

## XFA text

FGAS font selection and FDE text output reuse these same metric/outline adapters.
Their source bidi, Arabic forms, line breaking and drawing lay out new XFA text;
existing PDF glyph positions are unchanged. See [the text-layout boundary](xfa.md)
for font ownership, substitution slots, borrowed drawing views and validation.

FGAS enumerates each installed face name once, retaining its first occurrence.
The font mapper lists a face separately for each supported charset, but XFA's
stream lookup maps every such entry with the same name and default charset.
Skipping those duplicate entries avoids repeatedly reading whole font files and
registering identical descriptors while preserving candidate order and ranking.

## Save-time subsetting

The selected backend is published **Skera 0.9.0**, using the same Fontations
types as rendering. The adapter is
[`cpdf_fontsubsetter_backend.rs`](../crates/pdfoxium-core/src/fpdfapi/edit/cpdf_fontsubsetter_backend.rs);
PDF-side candidate collection and save overrides remain in
[`cpdf_fontsubsetter.rs`](../crates/pdfoxium-core/src/fpdfapi/edit/cpdf_fontsubsetter.rs).

The design preserves PDFium's save strategy: retain glyph IDs and the
`.notdef` outline so existing PDF font mappings need no rewrite. Typst's
`subsetter` was considered, but its glyph renumbering and CID-oriented output
would require broader changes to font mappings and embedding. Native HarfBuzz
does not satisfy the Rust-only requirement. The experimental Allsorts/custom
CFF rewriting approach was set aside in favor of this smaller adapter.

Before installing any save override, the adapter validates the glyph mapping,
counts, metrics, usable original-format outline tables, and requested glyph
slots. A successful library return alone is insufficient: Skera 0.9 can drop
unsupported outline tables while reporting success. A failed or invalid subset
keeps the original embedded font and allows saving to continue.

TrueType subsetting is connected. CFF/CFF2 subsetting remains incomplete pending
published upstream support, tracked by
[Fontations #2248](https://github.com/googlefonts/fontations/pull/2248) and
[#2331](https://github.com/googlefonts/fontations/pull/2331). The adapter does not
reject CFF before calling Skera; a future published release can use the same
validation path. Retaining the original font is a fallback, not completed CFF
subsetting coverage.

Tests cover sparse glyph IDs, `.notdef`, unusable-outline rejection, unchanged
PDF mappings, original-font fallback, and save/reload rendering through both
readers. Font-table bytes and saved-file sizes may differ from native PDFium.
Remaining scope is in [TODO.md](../TODO.md); executable tests are described in
[the testing guide](../tests/README.md).

Annotation appearance generation uses the same configured font mapper as
rendering. It captures the default appearance and font resource, releases
dictionary borrows while loading the font (which can invoke host callbacks),
then writes the appearance using that resolved face. This keeps generated text
metrics and subsequent save/reload rendering consistent. The `AppearanceFonts`
public API scenarios cover configured font data, placement, pixels and callback
reentry for this path.
