# Known differences in the pinned native test oracle

The pinned native library is comparison evidence. Public API contract and PDF
correctness take precedence over reproducing its bugs. The C++ tests document
each exception locally, retain independent expected assertions for pdfoxium and
compare the supported native observations. These are not exclusions of pdfoxium
functionality or reasons to mark an unfinished case complete.

## Lab ICC inputs fall back to the alternate colour space in native

The pinned `IccTransform::CreateTransformSRGB` requests Lab doubles without the
LittleCMS floating-point format bit. The transform is rejected, and
`CPDF_ICCBasedCS` uses the PDF's alternate colour space. Its approximate Lab
conversion differs from evaluating the embedded ICC profile.

[The public API regression](../tests/capi/image_tests.cpp)
`Images.FractionalLabSpotTintsUseTheEmbeddedProfile` constructs an original Lab
identity ICC profile and checks decoded pixels and painted swatches. Rust must
retain fractional Lab inputs and match independently calculated CIE Lab/D50,
Bradford adaptation and sRGB bytes. Native must match the explicit alternate-Lab
control instead. No raster tolerance is widened.

This explains the residual in corpus `hayro/pdfbox/5464`, page index 2. Its JPEG
uses a Separation tint into Lab ICC. For tint 1/255, the correct rounded RGB is
`(254,254,255)`, whereas native's fallback gives `(255,254,255)`. Rust previously
truncated fractional Lab operands to bytes, producing `(252,254,255)` and a
visible blue cast. It now agrees with LittleCMS using Lab doubles to within two
channel levels across all 256 tints. A control replacing only this image with
LittleCMS-converted RGB has zero connected residual against Rust at 144 DPI;
native's original image is byte-identical to an explicit alternate-Lab control.
The remaining one-level background tint follows the encoded colour and is not
removed by forcing near-white pixels to white.

## Valid JBIG2 refinement displacements

The pinned optimized GRRD kernel resets vertical reference offsets whose
magnitude reaches the reference height. Valid shifted refinement symbols then
use incorrect arithmetic contexts and can fail the entire image. Rust preserves
the encoded displacement in the optimized kernel, widens row arithmetic to i64
and checks reference bounds before indexing. Fast-path eligibility is unchanged;
large offsets do not require the general kernel.

The [Right to Privacy investigation](capi-known-native-differences.md#valid-jbig2-refinement-displacements)
isolates the failure. The [public API regression](../tests/capi/coverage_compositing_tests.cpp)
requires all 12 fixture masks to match independent jbig2dec coverage-byte hashes
and ink counts, plus visible dark text and identical progressive/synchronous
output after cancellation. Native retains explicit mask-failure/absent-dark-text
assertions. Both fixture pages match a decoded-mask control exactly at 72/200 DPI;
all 168 masks of the original 28-page PDF also match jbig2dec pixel for pixel.
No raster tolerance is relaxed. Current PDF.js uses PDFium's decoder and shares
the native failure; Poppler and CoreGraphics render the unchanged PDF normally.

## Image decode resolution depends on the destination in native

Pinned `CPDF_ImageRenderer::StartLoadDIBBase` supplies the destination bitmap's
dimensions as the JPEG/JPX resolution hint. Independent small tiles can therefore
decode less detail than the corresponding whole-page draw at identical zoom.
Rust uses the transformed lengths of the image's own unit edges for both decode
and cache validity. Small images on large canvases may also decode fewer levels
than native, while a large clipped image can require more detail.

[The public API scenarios](../tests/capi/coverage_image_drawn_size_tests.cpp)
independently assert exact Rust full-versus-tile crops for MCU-aligned JPEG and
reversible JPX, default/no-image-smoothing flags, offsets and quarter-turn matrix
clips. Native's fresh tiles must exhibit the reduction discrepancy, and its
full-detail warmed tiles are exact controls. Small-draw output is compared with
a native destination whose dimensions request the corresponding reduction.
Cross-engine JPEG comparisons retain only the existing two-channel-level
rounding bound; reversible JPX and same-engine crop checks are exact. Both save
writers and readers are exercised.

A valid 32-square, zero-decomposition JPX drawn at one pixel is another explicit
case: native's one-pixel destination requests unavailable wavelet levels and
leaves white; Rust retries available levels and requires the same visible gray
pixel as the 64-square native control. Other JPX failures are not retried.

The shear scenario also exposes a native clip-phase defect with full source
detail. Rust now anchors affine fixed-point sampling to the whole image and
requires fresh and warmed regions to equal whole-page crops exactly. Native's
whole-versus-clip difference remains an explicit assertion; full-image
cross-engine decoder comparisons are unchanged. No general tolerance or corpus
exemption is introduced. See [the focused design](image-decode-resolution.md)
and [the corpus attribution and timings](../benchmarks/README.md#findings-worth-retaining).

## macOS lowercase meridiem formatting

The pinned `fxjs/cjs_util.cpp::CJS_Util::printd` maps `tt` to `%P` on every
non-Windows platform, then calls `wcsftime`. macOS libc returns literal `P`
for `%P`, whereas the test expects lowercase `pm`. The macOS oracle therefore
returns `July 4, 2014 Friday 15:59:58 P` in
`CoverageJavaScript.DateFormattingAndScanningRoundTripLocalJavaScriptDates`.
The host's `strftime("%P", ...)` reproduces the result independently; `%p`
returns `PM`. Rust's portable formatter correctly returns `pm`.

On macOS only, the native scenario checks `TT` (supported `%p`) and its
lowercased result. Rust still independently checks the full `tt` date against
`pm`, and both libraries check the full `TT` date against `PM`. Other platforms
retain the shared lowercase assertion.

## macOS Unicode printf conversion

`CJS_Util::ParseDataType` rewrites `%s` to the nonstandard `%S` before
`WideString::FormatV` calls libc `vswprintf`. The pinned Mac oracle returns
empty for `util.printf('%s', 'caf\u00e9 \u03a9')`; Rust independently requires
the entire Unicode string. ASCII, NUL termination, width and precision remain
shared assertions. The service fixture uses a real recording `app.response`
callback to identify the current reader on every execution, including all
four-way save/reloads. Both readers then produce the same visible `PASS`
control; no raster tolerance is widened. See
[service tests](../tests/capi/coverage_javascript_service_recovery_tests.cpp)
and [native formatter](../upstream/pdfium/fxjs/cjs_util.cpp).

## Empty scand input uses local fields as an epoch in native

Native `util.scand(format, '')` wraps `FX_GetDateTime`, which constructs epoch
milliseconds from local `tm` year/day/hour fields without subtracting the UTC
offset. UTC Linux tests conceal the shift; on a non-UTC host it returns
`Date.now() - getTimezoneOffset()*60000`, with whole-second granularity. Rust
keeps the actual JavaScript instant. Both results are independently bounded
using their existing JavaScript clock and offset, and local print/scan fields
remain coherent. The reader marker above retains correct assertions and common
visible output after every writer/reader combination. See
[service tests](../tests/capi/coverage_javascript_service_recovery_tests.cpp)
and [native clock construction](../upstream/pdfium/fxjs/fx_date_helpers.cpp).

The same native parser seeds omitted hour/minute/second fields from that clock
for nonempty inputs and returns the assembled fields as a UTC epoch. A
date-only `scand` followed by local JavaScript getters can therefore advance the
calendar day late in the evening. The full-month/case-folding scenario supplies
an explicit hour chosen from the independent JavaScript timezone offset so
both UTC and local interpretations retain the same calendar date, including
UTC+12/+14 zones. Exact calendar assertions do not inherit the current clock.
This reproduced against the library before the cache change on the Berlin Mac
(September 12 became September 13).

## Canonical Z bypasses native local-zone resolution

`CFGAS_StringFormatter::ParseLocaleTime` consumes the canonical `Z` for a `z`
picture without `ResolveZone`; `Time2Num` subsequently subtracts the CRT
standard local offset. Thus `Time2Num("13:00:00 Z", "HH:MM:SS z")` returns
`43200001` on this Berlin Mac rather than the correct UTC `46800001`. Explicit
numeric offsets do run `ResolveZone` and retain shared absolute expectations.

The test derives native's original value independently from the host CRT
standard offset and retains Rust's exact UTC assertion. Public field editing
then supplies the correct shared value before rendering/saving; every reopened
reader reruns and checks its original result before normalization. No clock
policy is changed. See [picture tests](../tests/capi/coverage_xfa_picture_model_boundaries_tests.cpp),
[native parser](../upstream/pdfium/xfa/fgas/crt/cfgas_stringformatter.cpp),
[Time2Num](../upstream/pdfium/fxjs/xfa/cfxjse_formcalc_context.cpp), and
[standard-zone provider](../upstream/pdfium/xfa/fxfa/parser/cxfa_timezoneprovider.cpp).

## macOS ARM64 radial common-tangent stripes

The degenerate/common-tangent branches in `cpdf_rendershading.cpp` can produce
NaN and convert it directly to `int32_t` while calculating a color index.
The C++ conversion is undefined. On the tested macOS ARM64 build NaN becomes
zero, selecting a blue start color outside an unextended gradient. The
`CoverageRendering.RadialRootsExtensionAndShadingPatternBackgroundAreVisible`
case differs at precisely 100 pixels on x30 for the linear-root scene and 59
pixels on x20 for equal radius; every difference is native blue versus Rust
white, with all other pixels identical.

Rust explicitly rejects non-finite/out-of-range indices and already has an
independent common-tangent regression. A runtime AppleClang cast probe confirms
the native mechanism. This is not a reason to reproduce the stripe or widen
the entire scene's raster allowance. The integration test now requires Rust
white on those analytically unpainted pixels, accepts only native blue or white,
and uses white controls solely for cross-engine comparison there. Raw same-engine
save/reload comparisons and every other pixel retain their existing checks. See
[the visual investigation](../tests/capi/README.md).

## Stream-copy capacity checks

`fpdfsdk/cpdfsdk_helpers.cpp::GetStreamMaybeCopyAndReturnLengthImpl`, line 83,
tests `buffer.size() <= stream_data_span.size()` before copying the entire
stream. The inequality is reversed: a nonzero short buffer reaches a checked
copy and traps; an oversized buffer is not filled. A debugger confirmed the
trap through `FPDF_GetXFAPacketContent` on a valid public-API short-buffer call.

Affected stream-returning tests compare native query/exact-sized results and
exercise the full documented short/exact/oversized contract on pdfoxium. They
do not invoke the known trapping native input inside the shared suite. Other
APIs are adapted only after reviewing their actual helper/caller; this is not
a blanket exemption for buffer tests.

## Successfully decoded empty streams

The native stream accessor treats an empty decoded result as though no filter
ran and returns the original encoded bytes. An empty XFA packet becomes
nonempty compressed bytes after a normal compressed save. Pdfoxium distinguishes
successful empty output from deferred image filters and already-applied Crypt
filters; the latter still retain their original source. Empty-packet tests
require empty decoded content from pdfoxium before and after saving, while
recording the native encoded-empty result explicitly.

## Nested signature fields

The native signature inspection wrapper enumerates only the top-level AcroForm
`Fields` array. A valid signature in a terminal field below a nonterminal field
is omitted. Pdfoxium enumerates terminal fields in field-tree order, distinguishes
field children from widget children and honors inherited field attributes.
`Signatures.NestedFieldTreesRemainDiscoverableWithoutNativeBugCompatibility`
records the native limitation and independently requires the correct nested
signature and binary contents from pdfoxium.
Terminal invisible fields may omit the optional partial name `/T`; explicit
`/FT /Sig` children still count. A field's annotation-only widget children do
not introduce additional signatures.

## Mixed structure kids and marked content IDs

The native marked-content count/index wrappers use raw `/K` array length and
positions, including structure-element and object-reference children. Pdfoxium
counts and indexes actual numeric/MCR marked-content IDs as the public API
describes. Mixed-kid fixtures require the filtered IDs independently and record
the pinned native raw positions explicitly.
Pdfoxium also resolves indirect `/K` arrays, entries and MCR attributes before
enumerating IDs, including indirect numeric values for the legacy scalar API.
The native wrappers inspect raw references and omit these valid IDs.

## Text-search boundaries

The pinned search implementation accepts an empty search pattern but advancing
the resulting search dereferences the back of an empty word array. Tests safely
construct and close that native context; pdfoxium must also return false when it
is advanced. Reverse searches for overlapping matches without the consecutive
flag use a left-greedy forward partition in native PDFium. Pdfoxium retains the
rightmost valid backward match, with independent expected index assertions.

## Supplementary Unicode in ActualText and font saves

The pinned text-page marked-content filters discard Unicode values at or above
U+FFFD, including valid supplementary scalars. Pdfoxium preserves valid scalars
above U+FFFF while continuing to exclude the replacement/noncharacter values
and invalid scalar values. Structure/text tests independently require the full
Rust ActualText scalar sequence and record the native omission.

Native font subset serialization retains only the first Unicode value for a
source code, losing surrogate pairs and ligatures in ToUnicode. Its reverse
lookup also cannot map a supplementary scalar through an existing UTF-16 pair.
Pdfoxium preserves complete sequences during subset saves and recognizes valid
surrogate pairs for reverse lookup. Tests require correct Rust extraction and
rendering through both readers and record native writer truncation separately.

The native font descriptor loader also retains only negative ItalicAngle
values. Pdfoxium preserves valid positive angles, with independent metadata
assertions and a recorded native observation. The native CID Type2 factory
reads two-byte mapping entries without validating an odd byte length and can
trap. That malformed-input case is exercised only on pdfoxium, which rejects it;
valid even-sized maps and their glyph semantics run against both libraries.

Text-search patterns decode supplementary UTF-16 pairs to scalars, while some
ToUnicode text entries retain two code units. Native searches can therefore
miss those characters. Pdfoxium normalizes both representations for matching and
maps match endpoints back to the original character positions and counts.

The [large-book text benchmark](../benchmarks/README.md#findings-worth-retaining)
also exposes a related C API conversion limit. On macOS, native
`WideString::ToUCS2LE()` skips supplementary scalar values when `wchar_t` is
32 bits. Hefferon's mathematical letters therefore disappear from some
`FPDFText_GetText()` results even when character counts agree. This is separate
from already-paired UTF-16 units, which survive that conversion.

The initial pdfoxium C API converted those scalars to surrogate pairs and then
trimmed trailing text to fit the `count + 1` buffer cap. That caused differences
on 108 of Hefferon's 448 full-page extractions in the original benchmark.
The C API now follows the documented UCS-2 conversion without scalar expansion;
raw surrogate units still pass through. Bulk extraction agrees on all 2,493
pages of the three books after this fix. Full-Unicode safe Rust extraction,
bounded UTF-16 extraction and mathematical search remain unchanged.

The [math verification](text-extraction.md) distinguishes that documented
UCS-2 omission from an actual search defect: native finds only 11 of page 17's
41 mathematical `𝑞` occurrences, while Rust finds all 41 with correct source
ranges. Both engines' raw Unicode and adequately buffered bounded-text output
agree. The safe Rust API returns complete text; the Rust bulk C API's trailing
truncation was a separate bug, now fixed. Do not classify native's documented UCS-2 omission
itself as an implementation defect.

## Text clipping during content regeneration

The native content generator serializes path clips but omits retained text
clips, allowing later painting to escape its glyph boundaries after regeneration.
Pdfoxium reuses the renderer's glyph positioning and outline transforms to write
compound path clips for each retained text-clip layer. This preserves the clip
without introducing duplicate extractable text or new font mappings, including
when the original text object and its font resource have been removed. Tests
verify clipped pixels, extraction, layer geometry and four-way save/reload.

## Nullable progressive-render pause

The public `FPDF_RenderPage_Continue` contract allows a null pause interface.
The native wrapper rejects it. Native tests continue with a live non-pausing
callback; pdfoxium additionally completes the same rendering with null and
compares it with synchronous output.

## Flattening print-only annotations for display

Native `fpdf_flatten.cpp::ParserAnnots` checks Invisible for normal display but
omits NoView. Flattening a print-only annotation therefore makes it visible on
screen. Pdfoxium honors both flags for normal display and retains the Print flag
selection for print flattening. Tests independently assert the unchanged screen
appearance for pdfoxium, record the native extra square explicitly, and compare
both readers on each writer's actual flattened output.

## Overlapping-widget input order

Native SDK annotation iteration uses drawing order for hit testing as well.
For overlapping widgets this targets the covered back widget, although the
public form-field z-order query identifies the visible front widget. Pdfoxium
tests in reverse painter order, keeping the focused widget first for input and
last for drawing. The overlap scenario independently requires the correct Rust
target, records native's covered target, then edits an exclusive front-widget
point for shared rendering and four-way persistence comparisons.

## Shading objects during content regeneration

The pinned `CPDF_PageContentGenerator::ProcessPageObject` dispatch has no
shading-object branch. Regenerating a stream containing an axial shading and
ordinary objects silently drops its `sh` operator. Both implementations are
still checked against the original shading fixture and its rendered output.
Pdfoxium additionally regenerates shading resources/operators and must preserve
the rendered scene through both native and Rust reloads. Native writing is not
used as the expected result for this operation.

## Removed Form XObject children during saving

The pinned `CPDF_PageContentGenerator::ProcessForm` calls the page-oriented
`GenerateContent` on the form holder. That path updates a `/Contents` entry on
the form dictionary, although a Form XObject paints its own stream bytes. After
removing all children, live ownership/counts/rendering reflect the removal, but
saving and reloading restores stale children from the unchanged form stream.
`Objects.NestedFormChildrenCanBeRemovedTransferredAndRegenerated` exercises
native and Rust live removals, ownership transfer and matching rendering;
pdfoxium must also rewrite the actual form stream and preserve those edits in
both readers. Native's stale saved result is not a persistence oracle.

## AcroForm keystroke rewrites and cancellation

The pinned `CPDFSDK_Widget::OnAAction` executes an ordinary AcroForm action,
then returns false. `CFFL_InteractiveFormFiller::OnBeforeKeyStroke` treats that
result as an unsuccessful action and lets the original input continue, ignoring
updates to `event.change` and `event.rc`. The integration scenario independently
checks that both implementations execute the keystroke script. Pdfoxium must
also apply its rewrite and cancellation results; native's original-input result
is recorded separately and is not the expected edit behavior. Validation,
calculation, committed field values and the final rendered form remain checked
through both implementations and both save/reload readers.

The same false return skips the appearance/value-age repair in
`OnBeforeKeyStroke`. A real `lineWidth`, `borderStyle` or rectangle change can
therefore leave native drawing the pending edit through its old PWL window.
`GetCreateParam()` snapshots the border parameters, while ordinary drawing
uses the existing window. Rust recreates it, preserving the pending text and
selection for appearance-only changes, or loading a changed document value
before applying the pending range. A cancelled edit still preserves an
intentional script-side field-value change.

[The window recreation cases](../tests/capi/coverage_form_window_recreation_tests.cpp)
independently check text, selection, document values and once-only callback
dispatch. Beeps record callbacks without the focus changes caused by alerts.
Only the original keystroke frame's affected widget rectangle is omitted from
the native/Rust raster comparison; Rust's top/bottom border strips also match
an independent fixture with the final border installed before window creation.
A real public button then disables the action, and subsequent editing and both
save modes/four-way reloads compare the full common scene. Validation cases
retain full original-frame comparisons. These controls do not require Rust to
retain native's stale window.

## Focused XFA annotation handles

The pinned `FORM_GetFocusedAnnot` explicitly leaves XFA widget support as a
TODO (`crbug.com/42270486`) and reports its successful no-focus sentinel even
when the XFA field accepts text input. The XFA scenario records that limitation.
Pdfoxium already supports owned XFA annotation handles and must return the live
field, correct page and `FPDF_ANNOT_XFAWIDGET` subtype; the returned handle is
queried, refocused and closed through the creating library.

## Optional Brotli codec in the native binary

The pinned native distribution does not decode `BrotliDecode` streams even
when configuration version 6 enables Brotli. PDFium gates that decoder behind
the build option `PDF_ENABLE_BROTLI`; the runtime option cannot add an absent
codec. Lifecycle tests still initialize and exercise both settings in fresh
processes. Native renders the compressed fixture blank; pdfoxium must render
the same rectangles as the uncompressed fixture when enabled and blank when
disabled. The ordinary uncompressed document remains usable in both engines.

## Platform default font selection

Default system font providers select faces from their platform font inventory.
The two providers may choose different installed faces for the same requested
family, including regular and condensed variants, so their defaults do not
define a portable pixel oracle. Tests install each real default provider,
read its actual font/table data, render nonempty glyphs and release owners in
order. Separate custom-provider cases supply identical real TrueType,
OpenType and Type1 bytes and compare native/Rust pixels deterministically.

## XFA dataset whitespace during native saving

The pinned `CFX_XMLElement::Save` unconditionally writes a newline after every
opening tag, including elements containing only a text node. Saving an edited
XFA dataset value `changed` therefore produces `\nchanged`, which both readers
correctly observe. The XFA save scenario checks this native writer limitation
separately, compares both readers' rendering of that output, and requires the
Rust writer to preserve `changed` and its rendered appearance through both
readers. Master-page scenarios likewise observe a saved native caption
`Probe` becoming `\nProbe`; both readers compare the native output, while
Rust saves also preserve the original caption appearance. Pdfoxium does not
inject whitespace into text-bearing XML elements.

Saved choice labels can likewise begin with LF. In the host-popup scenario,
native `CFDE_TextOut::LoadText` receives `[LF, R, e, d]` in a single-row item
and traps after the newline advances beyond its allocated line. Initialization
removes exactly that known prefix from existing item text before drawing;
display/export pairs, actual popup rows and committed keyboard selection remain
asserted through both writers/readers.

## Rendering an unpopulated image object

Calling `FPDFImageObj_GetRenderedBitmap` with a live new image object, its owning
document and no page reaches `CPDF_DIB::StartLoadDIBBase` with a missing image
stream and segfaults in the pinned native library. A debugger confirmed the
stack through `CPDF_ImageRenderer::Start` and the public entry point on the
isolated empty-image case. Native's raw bitmap, pixel-size, metadata and
wrong-type queries remain tested. Pdfoxium must safely return null for rendering
the empty image; only this proven native crash is omitted from shared execution.

## Repeated native paints of XFA composite children

Native `CFWL_ComboBox::DrawWidget` paints its edit/list children, and
`CFWL_WidgetMgr::DrawChildren` then paints those same children again. Pdfoxium
already skips those repeated paints, as documented in
[xfa-widgets.md](xfa.md). The combo comparison confines reference
normalization to black text on white in the closed edit label: two identical
paints give `native = actual * actual / 255` per channel, so taking the square
root recovers comparable coverage without changing glyph geometry. Popup
content, arrow, backgrounds and the rest of the page are compared directly.

Native combo cleanup can retain its child caret: `ComboEdit`'s kill-focus
handler bypasses `Edit::HideCaret`, and a null successor can be rejected before
base cleanup. Its arrow state updates on mouse-up. Validation/binding comparisons
retain the original raster, then establish real pointer down/Escape/up and
non-null report-field focus. The bound-choice case preserves its original End
raster and compares both readers explicitly focused at Home with an empty
selection. Pixel budgets are unchanged; null-focus and focused-annotation
sentinel differences retain separate semantic assertions.

[The nested-coordinate cases](../tests/capi/coverage_nested_widget_scroll_tests.cpp)
compare each engine's nested controls against independently positioned flat
controls, including keyboard selection, rotations and saved readers. For the
two editable-combo cases, duplicate native painting changes antialiased glyph
and selection intensity inside the closed child edit. These cases retain raw
images and compare independently asserted text plus the ink/selection bounds
inside that exact edit rectangle; every pixel outside it keeps the ordinary
raster comparison. Within each engine, nested/flat and saved-reader/flat
comparisons remain full. This geometry control supplements the black-on-white
normalization above; it does not widen the page's error allowance.

Wide XFA list labels do not create a horizontal scrollbar in either engine:
native `CFWL_ListBox::CalcSize()` leaves `bShowHorzScr` false. The public tests
check clipping and selection preservation during horizontal-wheel input,
without claiming to exercise a live horizontal bar. Explicitly initialized
horizontal-bar behavior and nested host-caret coordinates are checked through
the existing component interfaces in
[the FWL unit cases](../crates/pdfoxium-core/src/xfa/fwl/cfwl_nested_widget_tests.rs).

## Detached XFA fragment positioning after pagination

The newer incremental-overflow scenarios also expose corrupted native continuation
fragments after shrinking and regrowing content: the diagnostic lost 1,200 blue
pixels and retained extra live pages, while a clean reload recovered the intended
layout. Rust must satisfy exact body-color, page and decoration assertions at
every step. For native's affected intermediate states, the tests retain fresh
model callbacks and exact unaffected master/gutter checks, then require strict
common initial and post-`remerge` rendering and four-way save/reload. They do not
require reproduction of native's damaged fragment pixels. See the
[pagination interaction tests](../tests/capi/coverage_xfa_pagination_interactions_tests.cpp),
[native splitting](../upstream/pdfium/xfa/fxfa/layout/cxfa_contentlayoutprocessor.cpp)
and [native layout reuse](../upstream/pdfium/xfa/fxfa/layout/cxfa_viewlayoutprocessor.cpp).

The native repeated-subform scenario retains a 20pt row offset on the final
continuation page although that page has no prelude, leader or margin. Pdfoxium's
existing `detached_split_roots_are_produced_in_order_and_not_nested_in_first_page`
core regression preserves the detached fragment at y0. The integration test
independently requires the complete white page and blue block at native y20
and Rust y0, then compares the block pixels exactly. Its control adds
`<keep intact="contentArea"/>`; both engines then place the whole instance at y0
and their entire added-page rasters must match exactly. Initial and shrunk pages,
field values, page counts and page-added/removed callbacks remain compared.

The deeper modern/legacy overflow cases with decoration occurrence maximum zero
also expose native payload loss: four 60-by-60pt rows contain 14,400 green pixels,
but native loses the first page's 60-by-30px band and paints 12,600. Decorations
must remain absent in both engines. Tests retain exact per-engine observations
and compare every pixel outside that proven band. An independent healthy
90/90/60pt fixture with explicit page breaks paints all 14,400 pixels identically
in both engines; every Rust reader of the original overflow fixture, including
both writers' saves, must match that complete control exactly. Nonzero occurrence
limits retain complete original-scene comparisons.

## Mouse release outside a focused widget

The pinned `CPDFSDK_PageView::OnLButtonUp` forwards an outside release to the
focused widget for cleanup. `CFFL_InteractiveFormFiller::OnLButtonUp` then
unconditionally dispatches its button-up action while focus remains, including
`/AA/U` and the primary `/A` fallback. PDF's U additional-action trigger is
release inside the annotation active area (ISO 32000-1, additional-actions
table). Pdfoxium cleans up capture/pressed state but gates action and click
dispatch on an inside hit. The C API case records the native outside activation
for both explicit U and primary A, independently requires Rust cancellation,
compares the released button appearance, and requires normal inside activation.

## Caret refresh after replacing and retaining selection

Native `CPWL_EditImpl::ReplaceAndKeepSelection` inserts text (updating the
caret), then directly changes `sel_state_` without calling `SetCaretInfo`.
The retained selection is returned correctly, but password/comb widgets may
paint the previous caret through it. Pdfoxium updates caret visibility with the
new selection. Tests assert the retained text/range, then explicitly select all
through the public API to refresh native caret state before comparing the
selected widget render. They retain strict surrounding raster bounds.

## Shared content-array child streams

`CPDF_PageContentManager` snapshots multiply referenced objects before cloning
an indirect `Contents` array shared by two pages. Traversal visits that array
once, so its child streams initially appear singly referenced. After the array
is cloned, the native manager still overwrites those streams in place: editing
a rectangle on one page changes the other page's paint too. Pdfoxium marks the
cloned array's existing child streams shared before updating their data.
`CoverageEditing.SharedContentsCopyOnWriteRemovalAndAppendAcrossGenerations`
records the native cross-page mutation, requires pdfoxium to preserve the other
page, and checks each writer's saved output with both readers. Inline arrays
and shared sole streams retain full native/Rust scene comparisons.

## Packed TIFF predictors and empty attachment trees

Native `tiff_predict_line` adds bytes for 2-bit and 4-bit components, corrupting
valid horizontally predicted samples. Pdfoxium accumulates components using
their actual bit depth and color stride, retaining row padding. Coverage cases
assert exact decoded payloads and rendered geometry; the affected native
observations remain explicit while both writers' encoded streams are checked.

Deleting every leaf of a nested native name tree leaves an empty `Kids` array
that rejects later attachment insertion. Pdfoxium restores an empty root's
`Names` array before insertion. The regression requires successful reinsertion
and reload with both readers, recording native's failed insertion separately.

## CMaps sharing partial prefixes

Native `CPDF_CMap::CheckFourByteCodeRange` accepts a partial match when a later
supplied byte already mismatches the range. Mixed three-byte and four-byte
codes with a common leading zero can therefore consume the wrong length.
Pdfoxium checks every supplied byte before accepting a prefix. Ordinary CMaps
retain direct cross-engine comparisons; the shared-prefix regression asserts
correct Rust extraction/search and the distinct native observation explicitly.

## Constant sampled functions

Native `CPDF_SampledFunc::v_Call` multiplies a singleton grid sample by the
encoded input coordinate. For `Size [1]` and `Encode [0 0]`, this turns a nonzero
constant into zero. Pdfoxium preserves terminal samples during interpolation.
The constant-green shading regression requires green from Rust and records
native black, including each writer's output reopened by both readers.

## Calculator Boolean and integer operators

Native `CPDF_PSEngine` stores every operand as a float and implements `not` as
logical negation. A valid integer operand instead requires bitwise complement
(`5 not` is `-6`). Pdfoxium retains Boolean provenance alongside its fixed float
stack, preserving it through stack operations and distinguishing Boolean
logical operations from integer operations. Unit tests and analytical shading
programs assert the resulting values; the integer-not scene records native's
different color explicitly. Other calculator scenes retain direct comparisons.

## JavaScript coercion exceptions

Native `fxv8::ReentrantToInt32Helper` installs a V8 `TryCatch` and substitutes
zero after a throwing conversion. For `util.printf('%d', value)` this suppresses
the caller's exception. Boa propagates the original thrown value. Integration
tests independently require that identity in Rust, record native's returned
`0`, and verify the document remains usable after either outcome.

`ReentrantToWideStringHelper` likewise suppresses a throwing `toString` and
supplies empty text. In `CoverageScriptAppEvents`, native therefore sends an
alert for a throwing array item (`[]`) and records a `mailMsg` callback with an
empty recipient; Rust propagates the exception without either callback. Tests
assert those exact side effects separately, then require the same successful
recovery message, fields and rendered output through both writers/readers.
Mail and alert callbacks are recording mocks; no external action is performed.

## XFA calendar selection and text-layout clipping

The native calendar sends `DateInBK` to a theme that handles `DatesIn`, so its
selected-day background is missing. Pdfoxium already uses the handled part.
Tests require the intended Rust highlight and the native absence explicitly,
comparing unaffected calendar regions and checking the selected text geometry.

The text layout rectangle is not a glyph-ink clip. Native `DrawLogicText` adds
no effective extra clip; widgets apply their own viewport clips. A prior Rust
adaptation clipped every layout box and cut descenders. That was a Rust
regression, not a native oracle defect, and has been removed. See the
[correction and regression tests](xfa.md). Full
descenders are compared normally; caller viewport clipping remains enforced.

## Explicit barcode text suppression

The native EAN/UPC writers ignore `textLocation="none"` and retain embedded
labels. A separate native painter shortcut can hide labels without spaces.
Pdfoxium honors explicit text suppression independently of font availability;
the XFA barcode scenarios require actual modules and compare the no-label ROI.

## Grayscale JPEG 2000 embedded alpha

Native `CPDF_DIB` explicitly leaves non-RGB embedded soft masks unimplemented
([crbug.com/42270756](https://crbug.com/42270756)). A valid two-component
gray/alpha image with `SMaskInData 1` is painted as opaque gray. Pdfoxium keeps
the gray plane and extracts the second component through its existing inline
mask path. Tests independently reconstruct all expected pixels from the pinned
4-by-4 fixture's gray/alpha samples, requiring correct Rust blending and the
known opaque native observation for both writers and readers.

## Full-width Type0 source codes

PDF 32000-1 §§9.7.6.2 and 9.10.3 permit source character codes of up to four
bytes and require ToUnicode mappings to use the font's encoding. The pinned
`CPDF_ToUnicodeMap` applies a 16-bit CID limit to those source codes, discarding
valid mappings above `FFFF`. Source codes and descendant-font CIDs are distinct.
Rust retains the full `u32` source code while preserving UTF-16 destinations
and bounded range expansion. The public text/font regression checks extraction,
search, rendering, reverse mapping during edits and both writers/readers; native
extraction's fallback is asserted explicitly. Core tests include scalar, array
and multi-string mappings at `FFFFFFFF`, with overflow-safe iteration.

## XFA exponent pictures and fractional seconds

Native `CFGAS_StringFormatter::ParseNum` consumes digits as a reverse exponent
without requiring an `E` separator. Consequently `Format("num{99999E}",12345)`
produces `00000E+0`; Rust rejects that invalid intermediate parse and preserves
the correct `12345E+0`. Native `CXFA_LocaleValue::SetTime` and `SetDateTime`
append milliseconds with the malformed format string `%:03d`. Formatting
`00:01:11.001` therefore drops its fraction, while Rust retains `001`.

The public picture scenarios assert each original result explicitly, compare
the unchanged scene outside the affected widget, then replace both fields
through normal public input with the independently expected text and compare
the rendered widget using the ordinary glyph tolerance. These are test-oracle
adaptations; neither correct Rust implementation is changed.

## Inline images during content regeneration

Native `CPDF_PageContentGenerator::ProcessImage` returns immediately for inline
images. Regenerating a page after changing an inline image's matrix therefore
drops the image. Rust promotes the existing image stream to an ordinary image
XObject through the existing resource path, preserving encoded data and image
semantics. The public inline-image tests compare original decoding/rendering
against native and independently require the edited image to survive Rust
regeneration and both readers. Ordinary XObject controls remain comparable.

## AGG image blend forwarding

Native `CFX_AggDeviceDriver::StartDIBitsWithBlend` discards its blend argument;
the Rust port inherited that omission. Consequently image overprint can choose
Darken correctly in the image renderer but still paint with Normal. Rust now
forwards the requested mode through the existing AGG image renderer/composer,
including transformed RGB images and masks and retained render tasks. Tests
assert the correct Rust overlap color independently and scope the native
exception to enabled overprint; ordinary blends, other image state and the
surrounding scene retain their comparisons.

## Imported popup annotation parent references

Native `CPDF_PageOrganizer` skips every dictionary key named `Parent`, including
popup annotation back-references. Such an imported reference can retain its
source-document holder. An isolated debugger run confirmed a native use-after-
free through `CPDF_IndirectObjectHolder::GetOrParseIndirectObjectInternal` when
regenerating imported content after closing that source. The ordinary scenario
keeps the native source alive through saving; it does not invoke the crash.

Rust's weak ownership avoids that unsafe dereference, and the importer now
remaps annotation parents through the ordinary destination mapping while
preserving the separately rebuilt page-tree parent. The Rust scenario closes
its source early and independently verifies the popup's reverse annotation
link and the saved document with both readers.

## Inherited resource dictionaries during editing

Native resource sharing traversal can miss an inherited resource edge. After
cloning the parent Resources dictionary, pruning its still-shared indirect
XObject category then removes an image used by another page. Rust also detaches
the indirect category dictionaries when cloning that parent. The public
resource scenario independently requires the untouched Rust page to retain its
image before and after saving, and records the pinned native image loss.

## JavaScript words at text-object boundaries

Native `CJS_Document::getPageNthWord` selects the preceding text object when
the requested word lies exactly at its end (`>=` instead of `>`). It returns
an empty string for the first word of the next object. The existing corrected
Rust traversal returns that word. Public script tests assert each observation,
compare the actual unchanged page text visually and reopen both writers' PDFs
with both readers. Ordinary punctuation, whitespace and page traversal cases
retain direct expected assertions for both implementations.

## Completed flowed blocks retried on the next XFA page

An intact flowed child can finish layout before its parent discovers that it
does not fit the remaining content area. Native reruns that completed child on
the next page and subtracts its accumulated used height again, collapsing its
retained layout item. A three-block example loses its middle colored block.
Rust returns the completed retained item before that subtraction, while
unfinished, pending and already-consumed continuations keep their existing
paths. The core regression checks all three original node identities and
70-point heights across 100-point fragments; the public scenario independently
checks page count, block color/order and both writers/readers.

## Annotation name conversion exceptions

The native V8 string conversion helper suppresses an exception thrown by an
annotation name's `toString` and substitutes an empty name. Rust preserves the
thrown value and the previous name. The public annotation script records each
outcome in a field for an independent C++ assertion, restores a valid name, and
continues checking rename, visibility, public flags and rendering. No raster
tolerance changes are needed for the differing report field.

## XFA remerge and rich-text continuation

Native `DoDataRemerge` leaves old form/data binding pairs attached to detached
form nodes. Subsequent matching can skip the original dataset record. The
existing Rust cleanup in `cxfa_document_datamerge.rs` preserves that record;
the public scenario verifies all original/appended data before remerge,
requires correct Rust fields/page-change events, and records native's skipped
record and unchanged page count.

Ordinary remerge also queues `BindItems` work without executing the first-load
`init_layout` path that normally drains it. Native remerged choices consequently
have zero items despite two final dataset rows. Rust drains that existing queue
under the normal update lock before calculation/validation, using the same
resolver and insertion algorithm. The regression retains native's exact empty
table, output and save, requires ordered Rust display/export pairs and real
keyboard selection, and compares against native initialized directly from the
same final rows. A nested-lock unit checks that the queue drains once after
unlocking; no full initialization or new binding algorithm is introduced.

Native rich-text block loading resets a shaped run and resumes at its
triggering character, losing the word prefix; its style/ancestor resumption
also produces misplaced paragraphs in the paginated fixture. Existing Rust
continuation code preserves those runs and scopes. Tests require all 20 Rust
paragraphs in order across both pages, exercise a real hyperlink, and compare
each paragraph with native's valid unpaginated control. Native's blank first
page and damaged second page are observed separately, including save/reload.

## Global script values across runtimes

Native `CJS_Global::CommitGlobalPersisitentVariables` has a declaration and
definition but no caller in the pinned tree. Its runtime-local changes never
reach the shared memory store. Rust commits its changes at the existing runtime
boundary. Public tests keep an anchor runtime alive, close a producer, and
require typed values and deletions to be visible in a fresh Rust consumer;
native's absent values are asserted explicitly. Each case removes its own keys.
No disk persistence or encrypted store serialization is claimed: the public
runtime supplies no storage delegate.

## Type3 and Gray8 text opacity

Native Type3 bitmap batching writes text opacity into the combined mask and
applies it again while painting. Its colored offscreen glyph path can omit the
outer text opacity. Rust builds the combined mask opaque and applies text alpha
once; colored glyph planes receive that alpha once without mutating cached
outlines or bitmaps. Public scenarios assert the original native observations
and independently require correct Rust opacity, including repeated rendering
at different alpha values, save/reload and unchanged pixels outside the glyph.

The ordinary smooth-text path has the same double-alpha problem on a real
Gray8 destination: `CFX_RenderDevice::DrawNormalText` applies fill alpha while
accumulating glyph coverage, then again in `SetBitMask`. At `/ca .5`, an Ahem
square with red luminance 76 becomes native gray 210 instead of the correct
165. Rust accumulates opaque coverage and applies fill alpha once. The device
scenario records native's original result, compares correct Rust output with an
opaque Ahem control in the independently computed final gray, and checks both
writers' saves against each reader's own original rendering. Overlapping-mask
regressions cover zero, half and full alpha without changing glyph coverage.

## XFA edit navigation and layout notifications

The pinned `CXFA_TextLayout::DoTabstops()` checks its initial `tab_index_ == -1`
before advancing it, so its positioned-tab adjustment never starts. The
rich-text input scenario records native's contiguous 12pt advances and requires
Rust's correct 48pt/96pt tab positions, including both writers' save/reload.
The fixture explicitly uses pinned 20pt Cousine `I` glyphs: their narrow ink
leaves three separate regions even in native's fallback, and their side bearings
permit valid padded crops. Aligned glyph crops are compared across engines;
the spacing difference is independently asserted rather than tolerated away.
The prior inherited 14pt `M` fixture could merge native glyphs into one region,
assumed the wrong advance, and cropped left of the image when ink began at x=0.

The new input tables exercise actual keyboard, selection, clipboard and mouse
events, including CRLF, empty lines, Shift navigation and comb cells. Rust keeps
CRLF pairs atomic and preserves the selection anchor when navigating backward.
Its default single-line edit engine also synchronizes the line breaker's wrap
settings. Tests use explicit multiline settings when wrapping is intended.
Native's breaker starts with a 100-point wrapping width even though the edit
engine declares single-line/no-wrap defaults. Its idempotent false setters leave
that state active. Long FormCalc, picture and form-event scenarios retain exact
original values and independently check native's clipped second-line ink versus
Rust's single line. They then enter the same short value through the public API
and compare the entire actual page with the original strict raster budgets.
The [pinned Arimo form fixtures](../tests/capi/README.md) include fitting
controls: `validated:initialized` and `[October 25, 2002]` require one line in
both engines. Only genuinely long strings retain the native wrapping assertion.
Native comb editing can retain a horizontal caret scroll after an insertion is
rejected at the final cell. The scenario navigates Home before comparing the
actual displayed and reloaded cells; the insertion/rejection assertions remain.

Native instance movement can retain stale loaded-widget coordinates:
`CXFA_FFNotify::OnLayoutItemAdded` compares `GetWidgetRect()` with
`RecacheWidgetRect()`, whose reference aliases the same updated member. Rust
updates the loaded widget. The public instance test compares Rust's live final
geometry against fresh final documents from both implementations, separately
asserts native's stale moved rows and compares unaffected regions normally.
Script-driven XFA value writes now propagate the script-modification flag to
loaded field controls; public and core regressions check both stored values and
the resulting visible text and selections.

## Embedded image decoder limits

Native `ProgressiveDecoder::BmpDetectImageTypeInBuffer` applies an
uncompressed pitched-size guard to RLE data. Valid compact RLE4/RLE8 programs
are rejected before command decoding; identical EOI-terminated programs padded
to the expected output size provide passing native/Rust controls. Native
`CFX_BmpDecompressor::ReadBmpHeaderIfh` also rejects BI_BITFIELDS headers larger
than 40 bytes. The public image tables assert correct Rust samples for valid
extended headers and compact RLE, and explicitly observe native's blank image.

Native GIF interlace pass advancement rejects the short two-row fixture.
Its TIFF adapter rejects a partial final vertical YCbCr clump; the table also
includes a full-clump control. A valid tiled TIFF is independently checked in
Rust, with native's unsupported decode observed separately. Missing TIFF
Photometric is tested as permissive malformed-input recovery, with an explicit
RGB control; it is not classified as a valid-file native defect. Tests compare
original raster samples, unchanged surrounding content and representative
four-way reloads without replacing failed native image pixels.

## Duplex master pages after relayout

Native `CXFA_ViewLayoutProcessor::MatchPageAreaOddOrEven` uses the length of
the retained page vector to choose parity. `PrepareLayout` reuses that vector
while resetting the current layout's available-page count. Subsequent layout
therefore selects the wrong master artwork: odd retained totals can omit it,
and even retained totals can paint the odd master on an even page. Rust uses
the current layout ordinal. Public tests independently check initial and
post-interaction master colors, body geometry, page names and counts, and both
writers/readers. Native's affected master-artwork region is observed separately;
the unchanged body and ordinary simplex cases retain direct comparisons.

## Embedded JPX opacity and advanced codec recovery

For `/SMaskInData 1`, JPX color samples are unassociated with opacity.
Native `core/fpdfapi/page/cpdf_dib.cpp` (775–798) premattes RGB over white
while extracting the mask. `StartLoadMask` creates no Matte entry, so the
prematting is not reversed before opacity is applied again. Rust preserves
straight RGB and detaches the mask, applying opacity once. The integration
cases independently check all source-cell interiors before and after both
writers/readers; common opaque regions retain strict comparisons.

Native's same loader detaches embedded opacity only for RGB; its
`crbug.com/42270756` TODO leaves the second Gray and fifth CMYK components
unused. Rust detaches those coverage planes too. CMYK extraction preserves all
four color samples for the existing color conversion and compacts them in place;
it adds no decoder or color-conversion algorithm. Focused tests check declared
and inferred color spaces, requested mask loading, unchanged CMYK samples and
single-use mask detachment.

The committed `jpxdecode_without_colorspace` PNG is also an imperfect oracle:
its Gray tile includes opacity that pinned native ignores, its RGB tile contains
white prematting, and its CMYK tile is shifted right by one pixel. The old CMYK
tile occupies x49–80 instead of the PDF's x48–79; tests explicitly check the
golden's colored x80 column and the correct render's gray background there.
Original PNG/RGBA assets remain unchanged. All unaffected pixels compare to the
golden, while every alpha tile has independent 4×4 source-sample/composition
assertions. Full scenes also compare to raw-image/separate-SMask controls:
correct native/Rust controls agree, Rust's original JPX agrees with that correct
control, and native's original legacy behavior is checked separately. Both
writers must retain each reader's original appearance; progressive/static and
cache-repeat comparisons remain intact.

Malformed JPEG recovery differs: native `LibJpegScanlineDecoder::PatchUpTrailer`
replaces the final two source bytes with EOI, potentially destroying entropy
bytes. The missing-EOI fixture compares the unaffected 112 columns and checks
each decoder's final 16 columns independently. A separate replaceable-trailer
control must recover the complete image in both engines. Invalid JP2 CMAP
component indices are safely rejected in Rust; the packaged native decoder
can recover palette pixels despite the pinned OpenJPEG bounds check. That
malformed-input case permits only explicit palette recovery or omission in
native, with an unaffected shared control. Neither recovery difference defines
a valid-input requirement.

Native libtiff's `PickContigCase` selects a separated-CMYK conversion only for
8-bit samples (`third_party/libtiff/tif_getimage.c`, 3164–3175). Valid 16-bit
CMYK fixtures require independently specified red/black interiors in Rust and
observe native's blank image. Equivalent 8-bit controls and unaffected page
regions compare normally, including four-way reloads.

## Locale numeric parsing and rich-text declarations

Native numeric parsing scans actual values as though they were picture
patterns. An apostrophe grouping separator can hide the decimal separator;
`1'234,50` in the custom locale incorrectly becomes empty. Rust scans the
actual decimal symbol while leaving picture literal processing intact. Tests
require canonical `1234.50`, exercise three grouping symbols and three picture
literal forms, and use public input to provide an equivalent native visual
control before comparing the complete scene.

The pinned CSS parser retains whitespace preceding `!important` in a color
value and rejects `color:blue !important`. Tests independently assert native's
inherited red and Rust's corrected blue using explicit controls, then compare
the same explicit blue scene across engines. Native FormCalc UUID generation
sets version bits but omits RFC variant bits; post-load callback captures
validate actual executing-reader results, requiring correct Rust variant bits.

## XFA SOM identity and reflected properties

Native `CJX_List::item` creates a fresh JavaScript wrapper through
`NewNormalXFAObject`. Tests verify aliases identify the same real class/SOM
node and share mutations; Rust additionally preserves canonical wrapper
identity. Unclosed SOM filters can fall back to the current native node when
`GetFilter` returns -1. Rust rejects malformed syntax, and both readers must
resolve the following valid query correctly. Native
`ReentrantToWideStringHelper` suppresses conversion exceptions; Rust propagates
them while preserving already-completed reentrant side effects.

Native named interception reports deletion success for declared attributes
without removing them, and can report successful definition of an unknown
property without exposing a value, descriptor or enumeration entry. Rust
requires nondeletable declared metadata and ordinary JavaScript expando
behavior. Each outcome is asserted independently; the script leaves the same
visible result for strict raster comparisons. Native-written captions receive
the separately documented XML leading LF; the script explicitly resets the
short Run caption through the actual model before each render.

The implementation corrections follow `cfxjse_engine.cpp` getter/setter and
global lookup ordering: generated metadata accessors must not hide a named
child, and only callable data properties receive method precedence over the
current predicate node. Rust's internal Proxy descriptor objects also use a
null prototype, preventing inherited descriptor fields from invoking user
getters during descriptor conversion. Focused regressions exercise name
collisions, nested predicates, reflection and a throwing prototype getter.

## DataMatrix encodings and barcode labels

Native `BC_Base256Encoder.cpp` initializes a full-symbol length codeword from
only the first decimal digit of the byte count. At exact capacity, it can
retain an incorrect length. The existing Rust encoder uses the valid zero
codeword meaning all remaining data. Tests decode actual rendered 14×14 and
16×16 symbols: six bytes have two valid encodings (explicit six or remaining
zero), while native's length one incorrectly truncates a ten-byte payload.
The independent inverse reader checks the complete UTF-8 payload, codewords,
Reed–Solomon syndromes and remainder modules. Direct raster comparisons omit
only that decoded data area; finder/timing borders, quiet areas and surrounding
pixels still compare. Original full images remain available as evidence.

Native `BC_OneDimWriter::ShowChars` skips human-readable text when the payload
contains no spaces, including digit-only EAN values. Tests require Rust's
requested visible digits, observe native's unchanged bars and compare guard
bars and surrounding content. Code39/Code128 payloads containing spaces supply
shared controls for all label positions. Rejected explicit error-correction
levels use a level-one control, matching the writer's initial state; an omitted
XFA level instead defaults to zero through the property layer.

## XFA split fields and composite edit controls

Native `CXFA_ContentLayoutProcessor::SplitLayoutItem` places a parentless
continuation into the current split subtree (`cxfa_contentlayoutprocessor.cpp`,
850–861). Recursive stabilization can revisit the same field's block and drive
its first fragment toward zero height. The long multiline-field scenario
therefore records native page counts, preserves all paragraphs and compares an
independent master-page sidebar on every page. Rust must render five positive,
populated fragments, preserve the terminal marker, and retain its full raster
after saving/reloading. A fitting control with the same font, caption, insets
and wrapping compares the entire scene through both writers and readers.

Native structured multiselect import concatenates bound child values without
newlines and can detach XML children while retaining their dataGroup binding.
Shrinking that broken native binding can trap in `RemoveChildAndNotify`.
The test records the original import, preserves the bound native group, and
requires correct bound shrink/expand operations independently in Rust. An
equivalent visible unbound list exercises destructive item-table operations in
both engines, including actual keyboard selection and strict selection paint.
Only known native-writer leading line feeds are removed from existing item
labels/values before rendering; the bound group is never destructively rebuilt.

The focused mouse/keyboard list tests use unbound fields. Both serializers omit
unbound Value state, so each reader must first restore the template selection,
then repeat the same real input sequence before its selection paint is compared.
This tests the input contract without requiring transient state to persist.
Native's already loaded list can also omit the selected-row paint after script
item replacement even though its model reports the correct value. That original
image is retained, Rust must already show the selected row, and a real row click
then supplies the equivalent focused native control. Native focus loss also
refreshes unchanged selections using a selection ordinal as an item index,
clearing the row again. The full raster comparison therefore keeps the real
list focused; subsequent focus transfer still checks the exact model value
before saving. These comparisons keep the original pixel budgets and fit the
text inside the actual clipping rectangle.

`CXFA_FFListBox::UpdateFWLData` passes selected item indices to `GetSelItem`,
which expects selection ordinals. The long-list constraints tests independently
require no blue highlight in the original blurred native scene for indices
above zero, while Rust must retain it; index zero remains highlighted in both.
They check the saved index/export before and after replaying actual navigation,
then require both focused controls to paint the selected row and compare the
full scene through all four writer/reader combinations. See the
[widget constraints](../tests/capi/coverage_xfa_text_widget_constraints_tests.cpp)
and [native list adapter](../upstream/pdfium/xfa/fxfa/cxfa_fflistbox.cpp).

Rust had omitted the source `scriptModify` flag on JavaScript selection and
deletion calls, preventing the existing notification path from refreshing live
controls. Passing that flag through single/multiple selection and deletion fixes
stale display text being committed on blur; ordinary model callers keep their
existing flags. Initial combo selection also needs its UI styles installed first
so the edit buffer receives the selected label. Actual-document regressions
check loaded list updates and clear/repopulate, popup and real focus transfer.

XFA solid fills and strokes now use the ported render-device wrapper before
AGG, matching the native call path. In particular, fractional list-selection
rectangles use device rectangle rounding rather than antialiased path fringes.
This corrects a Rust port omission; it requires no additional raster tolerance.

Native ComboBox and DateTimePicker paint their edit children directly and again
through WidgetMgr. Only the affected edit text regions use independent ink
geometry checks; list rows, selection highlights, borders and the rest of the
scene retain strict pixel comparisons. Rust-to-Rust reloads compare full images.
Widget flattening comparisons save/reopen the changed page before rendering;
the static renderer uses the same smoothing options supported by SDK drawing.

## Alpha-only images, stencils and uncolored tiling

Native `CPDF_ImageRenderer::StartBitmapAlpha` encodes constant image opacity in
RGB with opaque alpha. An `EightBppMask` destination consumes alpha instead of
RGB, so alpha soft masks and uncolored pattern cells can lose image opacity.
Rust supplies the opacity in ARGB alpha for that destination, retaining the
existing gray-intensity convention on color surfaces. Independent image/vector
controls check source opacity and overlapping alpha union.

Native ordinary stencil rendering supplies the same constant opacity in both
the mask color and scalar bitmap alpha. Cached uncolored tiling similarly
applies the caller's opacity after the cell has already inherited it: a
half-opacity cell becomes quarter-opacity. Rust applies opacity once in static
and progressive stencil callers and uses opaque caller RGB to color the cached
cell mask. Native cached stroke patterns also select fill color, while stroke
cells inherit fill opacity instead of stroke opacity. Rust selects the stroke
color and adapts the fresh cell's inherited fill opacity to the stroke operation;
caller state and subsequent pattern uses remain independent.

`CoverageAlphaRender` independently checks the affected pixels against alpha
composition, transformed geometry and equivalent vector controls. Native's
specific observed results are asserted separately. Four-way saves compare each
reader against that reader's original appearance; unaffected pixels retain
strict native/Rust comparisons. This boundary does not relax raster tolerances.

## Hebrew direction and final-line baselines

Native `CFGAS_TxtBreak::EndBreakBidiLine` and
`CFGAS_RTFBreak::EndBreakBidiLine` invoke the existing bidi algorithm only when
the line contains Arabic. Hebrew-only runs therefore remain in logical order.
Rust also activates that algorithm for strong right-to-left characters and
directional controls, preserving the logical character/width associations while
recording visual order. No new shaping or bidi implementation is introduced.
When a final line has several directional pieces, Rust gives all of them the
same font-size height; shrinking only the final sentinel piece otherwise shifts
the visible run's baseline.

`CoverageXfaTraversalBidi` retains logical text, selection and undo/redo checks.
Pure Hebrew runs are checked against independently positioned singleton glyphs
using a test-only Unicode-addressable derivative of the pinned embedded font;
native's original logical-order paint is observed separately. After preserving
the original data and raster, public input supplies an equivalent native visual
sequence for direct comparisons. Mixed-run save/reload checks retain original
logical values and visible ink, then use a common glyph control for strict
cross-engine comparisons; focused breaker regressions assert mixed visual order.

`CoverageXfaArabic` extends those checks to actual contextual Arabic forms,
Lam/Alef pairs, combining marks, directional controls, rich text and wrapping
using an unmodified embedded DejaVu Sans. Rust's original runs retain independent
presentation-form geometry, logical selection/history and line-count assertions.
For the finite native bidi-width defects, tests retain the original mismatch and
compare valid public singleton controls. Each reader must preserve its own
original appearance through both writers; those reload comparisons are exact.
Explicit line spacing removes the unrelated final-line baseline difference.
Legal XML padding avoids the separately tested native UTF-8 read-plane defect
described below, preserving every normal text and reload assertion.

The pinned FEF5 glyph has a negative left bearing of 210/2048 em. Its standalone
control uses a wider left clipping area and right alignment to retain the
specified glyph origin without clipping its accent. This changes test geometry,
not the engine or its silhouette tolerance.

Healthy DejaVu singleton comparisons use a private FreeType/Fontations coverage
allowance: maximum channel delta 144, glyph-union BGRA mean 2.5 and ink-mass
difference 1.5%. Before that comparison, dark bounds must match exactly,
bidirectional one-pixel silhouettes must agree, alpha must be opaque, and every
changed grayscale pixel must touch an antialiased edge in both images. Solid
black/white inversions fail. The measured maxima over 63 controls were
137, 2.224 and 1.364%; this allowance does not apply to original logical-run
geometry, unaffected strict comparisons or reader-own reload checks.

## Detached XFA primitive ownership

Native primitive constructors such as Boolean use `CJX_Object`, rather than
inheriting `CJX_Tree`/`CJX_Node` callbacks. The constructor table checks that tree
properties are undefined. Rust now applies the same declaring-class guard;
redispatching an incompatible callback through its own binding previously
recursed until stack exhaustion. This is a corrected Rust dispatch error.

Native content notification can dereference `Value.parent` when mutating a
primitive under an orphan Value. Schema conversion/clone scenarios therefore
create the legitimate Field → Value → primitive ownership chain in both
engines. Detached constructor identity is still exercised; the shared suite does
not require reproducing that native crash.

## XML delimiters across input planes

Native `CFX_XMLParser` checks a CDATA terminator only within the current
1024-character decoded input plane. A valid `]]>` split across two planes can
be missed, swallowing subsequent XML through the next terminator. In
`CoverageBarcodeBoundaries.EdifactSymbolCapacityTailsUnlatchAndResumeAscii`, the
ten-character initializer puts its first closing `]` at concatenated XFA offset
1023: native retains the placeholder barcode value and loses the following audit
field. The test observes that result, including the barcode region and unaffected
green sentinel, against an explicit placeholder control.

Rust preserves the parser's state machine while retaining an incomplete decoded
suffix and refilling delimiter lookahead. The original boundary fixture must
initialize, render and survive Rust save/reload correctly. Saving preserves the
original template packet, so its native reader retains the same defect. The
scene compares to both engines parsing the same template with one harmless
leading XML space, which moves the terminator wholly into the next plane; that
aligned control also exercises both writers and both readers. Comment and CDATA
delimiters have focused UTF-8/UTF-16 boundary regressions. Raster tolerances are
unchanged.

The same stream proxy resets its UTF-8 decoder's pending sequence on every
read, losing a valid multibyte scalar split at a byte-buffer boundary. Pdfoxium
retains the existing decoder's partial code/pending count across reads and
continues after a successful zero-character partial read, rather than reporting
premature EOF. `CoverageBarcodeBoundaries.Utf8ScalarAcrossInputPlaneRetainsBarcodeValuePaintingAndReload`
places the real UTF-8 bytes for `é` across offset 1023/1024 inside a JavaScript
string. Rust must retain that value, its audit length/checksum and its DataMatrix
painting; pinned native loses the scalar and produces an empty value. Its
encoder still paints the smallest 10-by-10 ECC200 symbol, beginning with PAD
codeword 129; independent finder/codeword assertions and an explicit healthy
empty-value native control establish that result. A leading XML space supplies the otherwise identical healthy native
control, including both-writer/both-reader saving, while the original retains
Rust save/reload assertions. Small-buffer decoder regressions cover two-, three-
and four-byte scalars, invalid-sequence recovery and truncated EOF.

## Indexed AcroForm widget properties

Native `CJS_Field::get_line_width` resolves the requested control, then discards
it and reads control zero. Rust now uses that resolved control, agreeing with
the indexed setter and actual border paint. `CoverageIndexedWidgetScripts`
asserts correct selected/sibling Rust getters, observes native's first-control
getter separately, requires a visible change in the selected border and compares
unaffected siblings exactly. Both engines' actual scenes and both writers/readers
still compare normally.

The public indexed path uses a double-dot name: `Document.getField` first checks
the field tree, then `AttachField` normalizes the empty component and parses the
control suffix. Ordinary indexed names return undefined in both engines. A
leading-space suffix is accepted by Rust's existing numeric parser; native's
integer parser rejects it but still returns an uninitialized wrapper with an
empty name. Tests observe this narrow difference and retain common valid aliases.
The original 14pt Courier semantics and actual ink remain checked; a common 12pt
control supplies strict raster comparisons with unchanged tolerances.

## XFA Unicode title getter

Native `CPDFXFA_DocEnvironment::GetTitle` reads the PDF Title as bytes and
converts them with `FromDefANSI`, although its setter writes PDF Unicode.
On the selected Linux build, a UTF-16BE title therefore exposes its BOM and each
high/low byte as separate characters. Host tests observe that exact sequence
only for native, require correct Unicode from Rust, and require correct
`FPDF_GetMetaText` results in both engines through both writers/readers. ASCII
and scalar title roundtrips remain shared; serialization receives no exemption.

## FormCalc foreach and unit conversion

Native `parse_foreach_expression` fails to consume `TOKdo`, rejecting the entire
valid event before execution. The original node-list and explicit-list loops
must sum the real field values to 12 in Rust; native's unchanged report is
observed separately. Equivalent finite-for and explicit-sum controls execute
and paint the same result in native, including both writers/readers. Original
foreach scripts retain Rust save/reload execution checks.

Native `UnitValue` also inverts multiplication for inch/cm/mm conversions to
points/millipoints. The five-by-five conversion table requires the correct
arithmetic in Rust. Only those native calls use the equivalent conversion to
inches followed by multiplication by 72 or 72,000; all remaining calls stay
shared. Real public input selects that control path, and the resulting fields,
rendering and save/reload remain compared with unchanged tolerances.

## Pinned regression fonts and glyph coverage

Upstream's regression harness maps Times to Tinos and Arial/Helvetica/Calibri
to Arimo in `testing/test_fonts.cpp`. The C API fixture provider reproduces
those regular-font mappings using unmodified assets from the pinned test-font
revision. It restores each library's owned default provider after its documents
close. This is test setup; the engine's font selection and production dependencies
remain as documented in [font handling](fonts.md). Asset provenance and licensing
are in [the test-font inventory](../tests/capi/resources/test_fonts/README.md).

With those fonts, native matches the committed AGG PNGs exactly. Rust rasterizes
Fontations outlines through AGG, while native rasterizes glyph outlines through
FreeType. Investigated differences in a few original text fixtures are limited
to grayscale edge coverage. Their local comparisons require matching ink bounds,
conserved ink mass, exact opaque alpha and unchanged colored pixels; solid black
cannot turn white or vice versa. They then bound every glyph-union pixel and
mean absolute BGRA delta: small embedded CID 56/3.5, Hello World 40/0.7, Times
clip/deletion fixtures 32/1.6, and Arial repaired-xref 64/3.0. The latter two also
bound fringe coverage separately. These allowances apply only to the named
fixture comparisons; same-engine regeneration/reloads and native PNG checks stay
exact. Source text, matrices, clips, tight/loose boxes and object assertions
remain independent. Image/JPEG comparisons retain their separate source-derived
RGB mean-squared-error policies.

Two curved CFF comparisons have a separate local channel/union-mean limit of
80/2.75: pinned Minion U and the explicitly mapped Foxit B outline. They require
opaque grayscale output, identical dark (`<180`) ink bounds, bidirectional
one-pixel dark-silhouette correspondence, no solid black/white inversion and
ink-mass difference at most 1.5%. Changed pixels must remain near an antialiased
edge in both images: two pixels for Minion's faint top fringe, one for Foxit.
Measured maxima are respectively 72/2.511 and 43/1.927, with ink-mass changes
1.234% and 0.102%. These allowances do not alter generic text or PNG policies.
For the headless CFF factory, native synthesizes Unicode mapping from glyph
names instead of the wrapper's explicit A-to-B cmap; the original A observation
is retained and a public-factory B control supplies the correct native outline.

The original synthetic required-hint font moves a right edge from 400 to 600
units when its `SHPIX` program runs at Rust's fixed 1000 ppem. Native executes
that adversarial program at 64 ppem and produces a wider outline. Font/device
scenarios retain the native observation and compare correct Rust geometry with
an independent unhinted 600-by-700 sibling, preserving original metrics and
advances. Both writers' saves retain each reader's original appearance. Ordinary
outline normalization also needed a Rust numeric correction: dividing by units
per em avoids the rounded reciprocal that added a solid row at an integer edge.
That correction introduces no native-oracle exception.

At 100pt, native's unhinted rectangle control has a normalized right edge of
2458/4096 rather than exactly 0.6. Rust retains the 600-unit outline; its
tile-stable rectangle snap computes `ceil(0.6f32 * 100)` before adding the
integer translation. Both controls therefore paint the predicted extra x90
column. The test asserts both ink counts (4270) and that entire strip, then
compares the controls exactly. Rust's required outline still must match its
independently constructed unhinted rectangle exactly; the fractional 13.5pt
cross-rasterizer control permits a single coverage level.

## Retained image-cache stream identity

Native `CPDF_PageImageCache` stores a `RetainPtr<const CPDF_Stream>` map key.
Rust previously stored only its address while retaining the mutable image;
`SetBitmap` could replace that image's stream and free the old key allocation.
Later address reuse could therefore return an unrelated old decoded plane.
Rust now retains the exact source stream with the cache entry, matching native
key ownership without adding lookup scans or changing invalidation rules.

A deterministic regression checks replacement colors, retained old pixels and
weak stream lifetimes through replacement and eviction. The C API replacement
scenario retains its visible-change assertions and checks actual rendered center
colors and every direct bitmap pixel for Gray/BGR/BGRx/BGRA with zero, one and
multiple supplied pages. This corrects a Rust ownership defect; it introduces no
native-oracle exception.

## JavaScript Unicode named destinations

Native `CJS_Document::gotoNamedDest` converts a JavaScript name to UTF-8 bytes,
then `CPDF_NameTree::LookupNewStyleNamedDest` passes those bytes to
`PDF_DecodeText`. Without a BOM, that decoder uses PDFDocEncoding: Ω becomes
Î©, so a valid Unicode destination is missed or a different name can match.
Rust preserves the JavaScript Unicode value for modern name-tree lookup,
including array and `/D` dictionary destinations. Legacy `/Dests` dictionary
fallback keeps its UTF-8 byte keys; it does not repeat the incorrect modern
lookup. Focused regressions include competing misdecoded names, non-BMP text,
conversion side effects, legacy fallback and missing-name recovery.

The public scenario first calls the original Ω destination in both engines.
An in-memory host alert distinguishes native's observed error from Rust's
required successful Unicode lookup. Only native's failed call continues through
an ASCII alias of the same destination. Both paths must produce exactly one
navigation with the independently specified page, view mode and coordinates.
The common report and complete page rasters remain comparable with unchanged
budgets, including replay through both readers of both writers' saved PDFs.
The focused unit regression and all eight document-action scenarios pass.

## XFA variables exports and owner layout

Native `UpdateVariableValue` calls `ReentrantSetObjectOwnPropertyHelper`, which
uses `DefineOwnProperty` with default attributes and suppresses errors. Updating
a nonconfigurable top-level `var` can consequently fail silently: assigning 20
to an exported counter initially equal to one leaves it at one. An exported
function writing `this.counter` has the same problem. Rust preserves the existing
declaration's attributes and updates its value. A native export assigned
`undefined` can also retain its previous value. Context scenarios assert these
original native results and correct Rust updates, then use ordinary lexical
assignments for equivalent shared results. A subsequent string assignment checks
that export updates remain usable.

An exported function reading `this.name` observes the script name `alpha` in
native and its variables owner `root` in Rust. Tests retain that specific
observation while independently checking owner-child/parent lookup, isolated
counters, extracted function calls and exception recovery. Unique exported
method names keep sibling SOM resolution separate from these assertions.

A bare owner-attribute write from the variables context updates `box.x` to
20pt in both engines. Native retains the old live widget layout at x0, even
after explicit relayout, while Rust immediately paints it at x20. Tests preserve
and independently assert both original ink positions. Rust's full original
scene must match a healthy native fixture initialized directly at x20, with
unchanged raster tolerances. Saves still come from the original
mutated documents; both readers of both writers must paint x20 and match the
complete healthy scene without modifying the saved input.

## Pattern-painted glyph strokes and nested uncolored Type3 fonts

Native `CPDF_RenderStatus::DrawTextPathWithPattern` copies graph/color state into
its stroked glyph path but omits general state, losing `/CA .5`. The original
shading-pattern text stroke is opaque red in native; Rust must paint half-opacity
red over white (`255,128,128`). An opaque glyph inside a half-opacity transparency
group supplies the healthy control. This extends the existing cached-pattern
notes; it is a different caller from stencil/tiling opacity.
A nested uncolored Type3 glyph also loses its inherited blue in native and draws
black. Rust retains blue and separate active-font identities. An explicit blue
outer `d0` form supplies a healthy native/Rust control. Each reader must preserve
its own original appearance through both writers; no generic budgets expand.
See [render tests](../tests/capi/coverage_render_transparency_boundaries_tests.cpp)
and [render source](../upstream/pdfium/core/fpdfapi/render/cpdf_renderstatus.cpp).

## Recreating an empty XFA instance group

Native `CXFA_Node::InsertItem` asks for the last instance when the group is empty,
then returns without attaching the newly created node. `addInstance(false)`
therefore returns a detached node and leaves count zero. Rust must attach it
immediately and report count one. Only the native detached result emits the
`DETACHED` diagnostic and uses public `nodes.insert` to establish equivalent
layout. Both engines then rebuild the expected page/payload and retain strict
save/reload comparisons. See [layout tests](../tests/capi/coverage_xfa_layout_recovery_tests.cpp)
and [InsertItem](../upstream/pdfium/xfa/fxfa/parser/cxfa_node.cpp).

## Unbound XFA choice branches after save/reload

After a fresh remerge, both engines retain only the first unbound local choice
branch. Direct reload through native revives the inactive second branch for
**both writers**; Rust retains the first branch alone. The test independently
checks red payload `1200` and blue payload native `1200` versus Rust `0`, before
remerging through the public model API for the common scene. This is a reader
boundary, not a claim that only native-written PDFs are affected.
See [choice tests](../tests/capi/coverage_xfa_merge_page_recovery_tests.cpp).

## Character input and Delete with an existing XFA edit selection

Native `CFWL_Edit::OnChar` inserts at the cursor and Delete removes one character,
even when all `ABCDEF` is selected. Typing `Z` yields native `ZABCDEF` versus Rust
`Z`; Delete yields native `BCDEF` versus Rust empty text. Both original results
and undo/redo are asserted. Public selection replacement then establishes the
common saved/rendered scene. See [capture/navigation tests](../tests/capi/coverage_widget_capture_navigation_tests.cpp)
and [edit source](../upstream/pdfium/xfa/fwl/cfwl_edit.cpp).

## FormCalc named zones and date picture parsing

Native adds named-zone offsets with the wrong sign. Modulo one day, PST minus
PDT returns `82800000` instead of Rust `3600000`; PST minus EST returns `75600000`
instead of `10800000`. Zone pairs cancel the actual local offset; tests assume
no UTC policy and add no clock implementation. Native date-before-time parsing
passes the date/time patterns to the opposite parsers, returning empty instead
of `2000-02-29T13:04:05`. Its `G` era clause advances two characters without
validating them: `2000-02-29 XX` is accepted as a date; Rust rejects it.
Original scalars are checked through a host callback after actual focus entry,
then public editing supplies the shared visible `PASS` control for four-way
rendering/saves. See [timezone tests](../tests/capi/coverage_xfa_timezone_boundary_tests.cpp)
and [picture source](../upstream/pdfium/xfa/fgas/crt/cfgas_stringformatter.cpp).

## Trimmed email URLs retain native's untrimmed character range

Native `CheckMailLink` rewrites the URL but `ExtractLinks` keeps the original
candidate start/count. Trailing double-period text, a discarded local-part
prefix, and `mailto:` therefore remain inside native's reported selection range.
Rust reports only the actual address. Both engines must return the same URL and
unchanged page ink; each range and its rectangle are checked independently on
both writers/readers. See [link tests](../tests/capi/coverage_text_search_link_boundaries_tests.cpp)
and [link extractor](../upstream/pdfium/core/fpdftext/cpdf_linkextract.cpp).

## Native-written form binding pictures gain a leading newline

Native's saved form packet can prepend a newline inside a binding `picture`,
preventing text/date/time normalization on reload. The merge tests first assert
the original malformed picture and formatted dataset value, then remove that
one newline and apply the known canonical value through the public model API.
Only native-writer reloads require this diagnostic; Rust-writer reloads must
retain their original valid pictures. Numeric and invalid-picture cases keep
their independent original assertions. See the
[binding-picture workflows](../tests/capi/coverage_xfa_merge_page_recovery_tests.cpp).

## FormCalc indexing a reference with a dotted node name

Native FormCalc `ResolveObjects` prepends the raw original node name for an
unnamed index accessor. A reference to `dot.name` followed by `[0]` becomes
`dot.name[0]`, splitting the literal name into two SOM components. Rust escapes
the name before resolving the index. The original event must produce `4` in
Rust; native leaves the report and audit unchanged. Tests retain those scalars
and original images on every reader. A separate event directly resolves the
escaped name and reads its raw value, requiring `4` in both engines before
strict rendering and four-way save/reload comparisons. See the
[resolution tests](../tests/capi/coverage_script_resolution_constraints_tests.cpp)
and [native FormCalc resolver](../upstream/pdfium/fxjs/xfa/cfxjse_formcalc_context.cpp).

## Generic multiple-master fallback metrics without PDF widths

Native's built-in Chrome Sans/Serif MM fallback shares a mutable FreeType face.
`CFX_Face::AdjustVariationParams` changes its design coordinates for requested
glyph widths. When a non-Base14 simple font omits PDF widths,
`CPDF_FaceBasedSimpleFont::LoadCharMetrics` instead loads glyph metrics from the
face's current coordinates, making that fallback sensitive to earlier paints.
Such widthless fixtures cannot serve as deterministic font-classification
controls. The recovery tests provide `FirstChar`, `LastChar` and `Widths`, warm
the shared face at a different width, and require correct serif/sans family
classification, logical text, visible ink and strict four-way rendering. This
adds no production state reset or general font-rendering exception. See the
[font recovery tests](../tests/capi/coverage_render_font_recovery_tests.cpp),
[native metric fallback](../upstream/pdfium/core/fpdfapi/font/cpdf_facebasedsimplefont.cpp)
and [native variation handling](../upstream/pdfium/core/fxge/cfx_face.cpp).

## FormCalc ISO validation accepts malformed separators and suffixes

Native `IsoDate2Num` accepts `1900-01/01`, `1900-01-01T12:34:56Zx`,
`1900-01-01T12:`, and `1900-01-01T12:34:56+05:`, returning `1` for each.
Rust correctly rejects them with `0`. Native `IsIsoDateFormat` skips checking
one separator; `IsIsoTimeFormat` returns immediately on `Z` and consumes final
clock/zone colons without requiring another digit pair. The tests retain exact
per-reader scalar assertions before the common rendered pass-count scene and
four-way save/reload comparisons. See [the scalar tests](../tests/capi/coverage_formcalc_scalar_constraints_tests.cpp)
and [native parser](../upstream/pdfium/fxjs/xfa/cfxjse_formcalc_context.cpp).

## XFA Shift+Right uses an end index as a selection length

Native `CFWL_Edit::OnKeyDown` passes an absolute end index as the second argument
of `SetSelection`, which expects a count. Selecting one character at index two in
`I I I` therefore selects native `I I` versus Rust `I`. Native's rightward index
helper also skips following line endings. The hit-testing scenarios independently
check the original native slice and require the correct one-character Rust result,
then restore a common visible scene for rendering and four-way saves. See
[the hit-testing cases](../tests/capi/coverage_text_hit_testing_tests.cpp) and
[native edit handling](../upstream/pdfium/xfa/fwl/cfwl_edit.cpp).

### Caption lost when a flow continues into another content area

`CoverageXfaLayoutVariants.LeftAndRightRowsContinueAcrossContentAreasWithExactColorConservation`
uses a visible `Probe` button above left-to-right/right-to-left colored rows.
After the report-triggered relayout, pinned native omits the button's caption
in both directions. Its click callback, report, all six color areas and their
positions remain correct. Native push-button layout/drawing still defines the
caption as visible; Rust now retains it when recycled fragments rebuild their
children. Matching the missing native caption would discard valid content.

The case checks Rust caption ink after the actual click and every reopen.
Cross-reader/backend raster comparisons exclude only the probe's 90-by-20-point
caption rectangle at (10, 10); they retain full unmasked artifacts and compare
the report, payload and remaining page. Rust-to-Rust saved comparisons remain
unmasked. All exact color conservation, geometry and page-count assertions remain.

### Decoded appearance font aliases need PDF-name escaping

`CoverageAppearanceFontResources` finds that pinned native writes decoded aliases
such as `Right Font` and `Font#Two` directly into appearance `Tf` operands. Its
shared `StringFromFontNameAndSize()` helper omits `PDF_NameEncode()`, so these
aliases become malformed tokens and the intended Courier font falls back to
Helvetica. Rust escapes the aliases at serialization and retains the correct
Courier resource, font bytes and metrics. Default-appearance recoloring uses the
same correction; encoded glyph strings are unaffected.

The two escaped-alias cases retain strict Rust assertions and record native's
fallback independently. Each original writer's output is reopened and rendered
with both readers under both save modes. A separate ordinary-alias document
supplies the common native/Rust comparison scene; the original document's font
mappings are not rewritten to conceal the defect.

### Editing an unbound field can drift a retained multi-area XFA view

`CoverageXfaRelayoutSequences` verifies two/three content areas, nested splitting,
keep pairs and repeated resize/presence/instance mutations. Before editing, both
libraries have the expected geometry. Editing the unbound controller causes
pinned native to empty the first content area and shift retained fragments while
preserving overall colored area. Its saved document reopens with correct layout.
Rust keeps one live document and must retain the expected geometry, payload,
page counts and edited text after every operation.

Native frames used as layout oracles come from its actual saved bytes after
each operation. Raw transient frames and callback checks remain available; no
text, layout attributes or instance state are reseeded after reopening. Both
save modes and both readers still check exact geometry and compare rendering.

Unbound-value serialization is a separate, shared source contract:
`ContentNodeNeedtoExport()` in `xfa_utils.cpp` excludes value content without a
bound data node from changed-form serialization. The live Rust controller must
retain `Edited`/`Second`; every saved reader must restore its template default
`Seed`. Only comparisons between these intentionally different live/saved note
values omit the exact note rectangle `(10,10,60,18)`, with raw images retained.
Saved-reader comparisons are full, and all page/payload/geometry assertions
remain strict.

### Same-data XFA remerge retains obsolete native binding edges

Native `CXFA_Document::DoDataRemerge()` removes the old form children but only
clears the root binding. Descendant dataset nodes retain strong reverse binding
edges; subsequent once-binding lookup skips them through `HasBindItem()`.
Rust clears the retired subtree's binding edges and must rebuild all current
rows against the same data, preserving values and calculation dependencies.

[The clone/rebinding cases](../tests/capi/coverage_xfa_clone_rebinding_tests.cpp)
exercise Rust's original same-data remerge. Native's explicit healthy control
deep-clones the actual current record children before remerge, preserving their
values, order and XML while retiring obsolete child identities. It retains the
record root and never supplies expected replacement values. Both paths assert
current field/data identity, detached-node isolation, calculations, exact page
geometry and full rendered comparisons through both save modes and readers.

Identity probes use the cached resolver rather than `NodeList.item()` for the
record root: native `CJX_List::item()` calls `NewNormalXFAObject()`, which replaces
the cached JavaScript wrapper even when the underlying node is unchanged.
The tests retain exact clone/root identity through cached resolution and parent
ownership; they do not require Rust to reproduce native wrapper churn.

## Region origins and zoom-history repeatability

The phase-3 [region gate](region-rendering.md) found several defects also present
in the pinned native AGG renderer. These are same-engine whole/crop or
cold/warm comparisons, not allowances for unrelated cross-engine differences:

- `cfx_renderdevice.cpp` rounds glyph origins with ties away from zero and uses
  a signed remainder for LCD phase. On a 64-square page, Helvetica 32 at
  `(8 + fraction, 43.5)`, glyph `H` with fractions 0.1/0.4/0.8 differs by
  119/80/119 pixels in a region starting at x=16. A `g` with a half-pixel baseline
  differs by 50 pixels after translating the region origin to y=24. The corrected
  Rust results equal their full-page crops exactly.
- Geometric object culling drops a 0.01-width line at x=15.9 from a region starting
  at x=16 even though AGG widens it into that region. The native reproduction
  loses 16 pixels. Stroke-transform translation also changes two boundary pixels
  for a zero-width line on a 173-by-127 page rendered at 217-by-159, cropping
  `(31, 155, 31, 4)`. Rust retains the raster fringe and avoids the translation
  round trip; both exact-crop regressions pass.
- Native's higher-detail image cache reuse changes subsequent downscaled output.
  The pinned `mona_lisa.jpg` on a 120-square page rendered cold at 30-square differs
  on 855/900 pixels from a 480-square render followed by 30-square. The Rust
  reduction-level cache key gives identical cold/large-first/thumbnail-first
  output, also identical to native's cold result for this fixture.
- A large-transform radial gradient changes four native pixels when rendered
  at a translated region origin. Rust retains the same solver while avoiding
  separately rounded inverse-translation terms.

Integer-boundary AGG geometric clipping changes edge coverage slightly by
rounding new intersection endpoints. This is now accepted in Rust too: the
[clipping experiment](../benchmarks/README.md#findings-worth-retaining)
found only small AA differences, so the exact cell-clipping path was removed.
Twelve pinned barcode fixtures expose one such pixel: the background stroke
corner at `(0, 298)` is gray 128 in the original fixture and 127 when translated
by `(2, 2)` away from the clip. Barcode tests now match the original native
hashes directly, without checksum normalization, and bound translated-crop
channel differences at 2/255. Glyph placement and other structural fixes remain.

Rectangle fast-path snapping can also trim a different edge after integer
translation. Native `stamp_appearance.pdf` at the 300-DPI canvas changes 288
pixels for region `(512, 512, 512, 288)`. Rust snaps before adding the integer
offset; geometric clipping can still contribute small AA differences. Pattern
fills likewise change when the
clipped area switches between cached-cell and direct painting; a native
834-by-1250 canvas probe changes 352 pixels in `(512, 512, 322, 512)` for both
colored and uncolored fill fixtures. Rust selects the algorithm independently
of clipping and tests the pattern cases at four scales.

The native probes used chromium-8100 macOS ARM64. Core unit regressions and
`pdfoxium/tests/render_regions.rs` retain independent Rust assertions, with the
region gate allowing at most 2/255 per channel for geometric-clipping AA.
Dedicated glyph-placement and cache-history checks remain exact.
Local diagnostic evidence is under `target/region-gate/`; no universal raster
tolerance is introduced.

## XFA plain mouse selection and combo caret blur

The pinned `CFWL_Edit::OnMouseMove()` requires `XFA_FWL_KeyFlag::kShift` even
while its primary-button drag is active. Consequently, ordinary unmodified
mouse dragging does not select XFA edit text in the native C API. Rust accepts
an active primary-button drag and retains the press index as the selection
anchor, including reversal across that index. `XfaForms.ViewerDemoInputPaintAndSaveReopen`
asserts native's empty selection and Rust's nonempty selection for the ordinary
gesture. It then uses the Shift gesture in both libraries, requires the same
selected text and compares its actual painted selection with the existing
edited-control raster budget.

Native `CFWL_ComboEdit::OnProcessMessage()` also consumes KillFocus after clearing
the focus bit, skipping `CFWL_Edit::OnFocusLost()` and its caret cleanup. Rust
forwards blur to the edit while that bit is still set, hiding the old caret,
clearing selection and cancelling its blink timer. A focused FWL regression
asserts timer creation and cancellation. The public XFA scenario checks that
moving focus out of the combo leaves at most the current control's timer, that
blur removes all remaining timers, and that the combo edit body returns to its original
unfocused raster. The viewer control walkthrough additionally compares the
blurred combo across two 600ms blink periods.


## Optional content defaults, usage and single-group policies

Pinned `CPDF_OCContext::GetConfig` automatically selects an alternate `/Configs`
entry with View/All intent, instead of the opening `/D` configuration. Its
`LoadOCGState` also unconditionally applies `/Usage` metadata without a matching
`/AS` application. PDF Reference 1.7 §4.10.3 requires explicit configuration
selection and application of usage metadata through `/AS`. Rust now follows
those rules for registered groups. The existing configuration scenario retains
independent native/Rust expected pixels for the three affected input cases and
all four writer/reader save combinations; shared cases keep exact comparisons.

`LoadOCMDState` returns the group state directly when `/OCGs` is a dictionary,
ignoring `/P /AnyOff` and `/AllOff`. A single group still follows the membership
policy under §4.10.1. Rust negates it for those two policies; native/Rust tests
assert both ON and OFF cases independently, including cross-reader saves. No
raster tolerance was widened. See
[the graphics-state scenarios](../tests/capi/coverage_graphics_state_tests.cpp)
and [the layer design](optional-content.md).
