# hayro-jbig2 local extension

Upstream: https://github.com/LaurenzV/hayro, crate hayro-jbig2 0.3.1.
Upstream revision: ea9c81dc70cde40c64205986fdcfcd481fe934d8 (hayro-jbig2).
Original crate checksum:
`000e025850f4592ddcaee85640db70704151f2bafd43b6f431aa5b6a2c6a35a9`.

Sources are copied from the locked crates.io release. Original MIT and Apache
licenses and upstream source notices are retained. PDFium-derived extensions
retain their BSD-style notices and LICENSE-PDFIUM.

Changes for PDFium compatibility:

- An embedded decode may specify the externally supplied PDF canvas dimensions,
  mirroring CJBig2_Context::GetFirstPage and buf_specified_. PageInfo still supplies
  default pixels and region interpretation; regions clip to the supplied canvas.
- This path accepts unknown/zero PageInfo dimensions without deriving stripe
  height, as the native external canvas already defines the dimensions.
- PdfiumDocumentContext supplies the native two-entry MRU global symbol cache,
  keyed by stream key and data offset. Segment metadata preserves globals
  identity without retaining encoded bytes. Cache copies are the native explicit
  deep copies, inserted before decoded arithmetic contexts are retained. Cache
  hits attach the native initial contexts; unused/invalid context arrays follow
  the C++ flags and size checks. The symbol decode loop is shared unchanged.
- A PDFium reader mode preserves sequential first-page framing, short trailing
  headers, EndOfPage/EndOfFile payload behavior, the 64-reference limit, native
  symbol limits and template/unused-flag interpretation. Region composition
  masks reserved bits with GetComposeOp's native mapping. The upstream default
  reader/API retains its original stricter validation and sorting.
- Canvas allocations use CJBig2_Image's signed stride/total-pixel bound. Ordinary
  region limits and the default upstream API retain their original behavior.

The core codec algorithms remain the planned safe Rust hayro implementation.
- Native context dispatch processes PageInfo in source order, preserves symbol
  dictionaries across repeated fills, isolates global page bitmaps, and searches
  references in global-first insertion order. Missing metadata, unknown/no-op
  segment types, clamped skips and short streams follow DecodeSequential.
  Successful prefix segments run before a later framing error, retaining their
  document cache effects. Global striped pages may expand; external canvases do
  not. The final default pixel supplies native output padding.
- readNBits accepts a partial final field but requires a valid starting byte,
  including zero-bit fields. Native byte reads retain the bit index. Symbol and
  text Huffman decoders retain the reader mode when borrowing payload slices.
- Refinement uses local reference coordinates and an explicit native SubImage
  copy when no segment is referenced. Explicit references follow C++'s repeated
  first-reference lookup, including wrong-type failure and ignored later entries.
  Text/refinement/halftone headers use IsValidImageSize; halftone requires exactly
  one pattern reference and masks internal composition flags with GetComposeOp.
- Known-length payload readers borrow the full remaining stream; declared
  lengths position the following header with native EOF clamping. Overflow is
  reported after the current decoded segment's cache effects. Top-level text
  instance bounds use the whole stream; internal aggregation keeps its native
  separate validation path. Unknown-length segment handling remains under audit.
- Bitmap arithmetic reads preserve native IsComplete checks while integer and
  IAID reads retain their sentinel behavior. Native integer decoding interprets
  its 32-bit magnitude as signed before checked class-offset addition, preserving
  sign/OOB behavior; explicit unsigned shifts define C++ signed-shift overflow.
- Generic arithmetic and MMR regions use their separate native dimension
  checks; MMR supports dimensions above 65535 within signed stride/size bounds.
  Adaptive pixels follow native permissive parsing. Pattern indices retain the
  native cap and signed-byte template negation wraps explicitly.
- Custom and symbol/run-code Huffman tables use native canonical indexing,
  preserving source order, overfull/long-prefix behavior and checked first codes.
  Coded tables require a first ordinary line even for equal bounds and use the
  exact declared upper base. Range arithmetic retains native unsigned narrowing.
  All 15 precomputed standard codebooks match pinned PDFium codes/range semantics.
- Native MMR uses hayro-ccitt's PDFium G4 rows, preserving partial/EOF rows,
  exact bit positions and inversion of row padding. Conversion borrows two row
  buffers without copying an image. Halftone planes skip native EOFB bytes and
  clamp pattern indices. Collective symbol BMSIZE selects raw/MMR without bounding
  MMR data; its reader advances to the actual native decode endpoint.
- Native image composition preserves the inclusive coordinate guard and clips
  rows before traversal, with widened extent arithmetic. Global PageInfo rejects owned images
  without pixel storage after construction. Native mode follows local region storage
  so reference/text/halftone composition keeps the same guard.
- Unknown generic segments retain a borrowed tail and resume sequential framing
  after the codec's actual offset, native marker advancement and four-byte skip.
  Native decodes the header height and ignores footer row counts, including
  markerless FF termination at EOF. Deferred global headers retain full-stream
  cache offsets and first-page termination independently of the local stream.
- Every parsed unknown-length type resumes after its decoder's actual endpoint:
  symbol/pattern dictionaries, text, halftone, refinement, tables and PageInfo.
  Offsets are returned separately from image/dictionary ownership. Cached symbols
  consume no payload/marker bytes, including native unknown-length cache-hit
  behavior. Header metadata and typed result lifetimes retain native source order.
- Native TRD keeps required/ignored arithmetic integer statuses, full refinement
  truthiness and arithmetic versus Huffman instance-stop order. Huffman
  refinements share contexts, validate actual marker-inclusive sizes and use
  native quarter-delta coordinates. Top-level i32 instance bounds reject native
  CHECK overflow safely; internal aggregation omits that context-only bound.
- Native export flags treat the declared count as an upper bound and retain
  required versus ignored integer statuses. Imported images use the explicit
  native deep copy; new images move into exports, and unexported images drop.
  Export storage follows actual count without padding or cloning new buffers.
- Owned image construction retains empty objects for invalid signed stride/
  total-pixel sizes, separately from each region procedure's has_data check.
  SubImage and global image replacement follow that distinction. Refinements use
  checked u32 dimensions and retain GRRD's wide/empty return without pixel decode.
  Invalid halftone plane allocation is checked before expanded scratch buffers.
- Null symbol slots are distinct from owned empty images, retain their indices
  and survive cache copies. SDD preserves zero/aggregation/MMR null branches,
  per-symbol bounds, raw source/padding checks and marker-inclusive refinement
  sizes. Arithmetic TRD rejects nulls; Huffman TRD skips them.
- Progressive GRD retains its image on arithmetic EOF. Immediate generic regions
  compose only completed rows; intermediate/pattern/gray paths keep partial
  pixels. Native generic direct decode is limited to MMR. Nonprogressive SDD and
  refinement still propagate their required bitmap failures.
- TRD retains native deferred checked-value validity, including discarded final
  cursor increments and unused zero-instance strip overflow. Dimension-minus-one
  increments use uint32 arithmetic; placement subtraction retains native narrowing.
- Native SDD aggregation code tables contain only known symbols, while code
  length includes the full declared count and is at least one for Huffman.
  Raw collective width checks preserve signed narrowing before allocation.
- Native GRRD kernels live in decode/generic_refinement/pdfium.rs, retaining
  byte/line shift registers, unshifted prediction rows, padding and distinct
  arithmetic completion checks. Deliberate correctness departure: the byte kernels
  preserve encoded DY instead of native's invalid reset to zero. Row arithmetic
  is widened to i64 and reference bounds are checked before indexing, retaining
  fast-path eligibility even for extreme offsets. General-kernel coordinates
  wrap explicitly, safely defining -INT_MIN. See the
  [investigation and regressions](../../docs/jbig2-refinement-offset-investigation.md).
- Native custom GRD templates use source line registers and USESKIP dispatch
  in decode/generic/pdfium.rs; optimized bytes retain native commit/EOF order.
  Native HTRD retains packed GSPLANES and allocates HTREG after them; XOR clips
  to visible plane width. PDD uses native padded SubImage copies and omits the
  backend shifted-pattern cache. Skip-plane allocation errors fail safely.
- Native image SetPixel normalizes nonzero values and clears zero values.
  Local canvas allocation follows successful globals and framing failures retain
  prior cache effects. Global PageInfo requires has_data after construction.
  Text RDX/RDY selectors and custom table order follow native X-then-Y order;
  an initial symbol-length repeat code 32 repeats zero.

Output performance extension:

- `Decoder::push_packed_row` optionally accepts a borrowed MSB-first u32 row
  before pixel/run emission. Its default returns false, retaining the original
  callbacks for existing consumers. Accepted rows still receive `next_line`.
  The PDFium adapter copies packed bytes with polarity conversion, preserving
  partial-byte/stride padding and avoiding per-pixel expansion/repacking. No
  decoder algorithm, cache semantics or scratch allocations change. Regressions
  compare both output paths, including mixed bytes and partial words.

## Bounded zero-byte arithmetic optimization

Checked bitmap reads are inlined while retaining their per-pixel EOF behavior.
The default-template byte loop may coalesce eight zero bits when its context
label and both previous-row windows are zero, the context MPS is zero, and the
normalized arithmetic interval proves all eight reads stay on the fast-MPS path:
`A_next = A - 8 * Qe >= 0x8000` and `(C >> 16) < A_next`. Normalized A fits in
16 bits. Every intermediate interval is at least A_next, so no renormalization,
input read or context update occurs; only A changes. The byte is explicitly
written as zero. The EOF guard is checked before taking the shortcut.

Failed guards use the original checked scalar path. Tail bits, custom adaptive
pixels, row completion and pause/resume boundaries retain their behavior. This
is an algebraic optimization beyond native PDFium's per-pixel loop, not removal
of native EOF checks. Independent arithmetic/native-equation and frozen scalar
bitmap tests cover states, truncation, output words and continuation state.
See the [Mac optimization report](../../benchmarks/jbig2-opt-mac-2026-10-25/README.md)
for measured effects and validation.

Packaged as `pdfoxium-jbig2` 0.1.0. The distinct
package name preserves the upstream identity above while making the maintained
PDFium extensions available to downstream consumers without workspace patches.
