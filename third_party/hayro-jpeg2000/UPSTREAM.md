# Local PDFium codec boundary

Baseline: hayro-jpeg2000 0.4.1, from LaurenzV/hayro commit
`ea9c81dc70cde40c64205986fdcfcd481fe934d8` (`hayro-jpeg2000`).
Crates.io checksum:
`122c5f834c37a2260b9c851bb8be685cf539b2a8b94b704a931ef7d2525d5835`.

The original MIT/Apache notices are retained. Native compatibility additions
translated from pinned PDFium use LICENSE-PDFIUM (BSD-3-Clause). Component rectangle/level-shift logic retains OpenJPEG
BSD-2-Clause notices in LICENSE-OPENJPEG-J2K and LICENSE-OPENJPEG-TCD.
The existing public decoding path keeps its upstream behavior. The additional
raw component boundary is for PDFium's own sampling/color/palette adapter.

## Raw component boundary

- `Image::decode_pdfium_components` transfers raw sample buffers without palette,
  CDEF or JP2 color conversion. The existing public decode path is unchanged.
- `src/j2c/pdfium.rs` maps pinned OpenJPEG component rectangles and DC level shift;
  IDWT output is stored at each component's own reduced sample dimensions.
- Signed SIZ flags are retained. Native signed planes omit the unsigned bias and
  clamp separately; irreversible sample rounding uses ties-to-even.
- Codec reconstruction stays f32. Moving samples out of SIMD storage truncates
  padding without cloning the plane. The first-party adapter owns JP2 metadata and color handling.

- `pdfium::Codestream` accepts exact reduction counts and bypasses JP2 color
  repair assumptions. Original reference dimensions support IHDR/SIZ checks.
  Actual post-EOC positions are returned separately from owned component planes
  for the adapter's native JP2 trailing-header checks. No input or plane clone
  is needed to separate these lifetimes.
- Native Psot=0 tile parts retain all remaining input except two EOC bytes,
  as pinned opj_j2k_read_sod does. No marker scan or JP2C length limit is added.
- Native MCT and DC level shifts run per decoded tile over borrowed tile
  slices, including scalar SIMD tails; unfilled regions remain zero. Small-
  component MCT skips its matrix as native does. Mixed wavelets retain native
  component-zero word interpretation. Tile overrides cannot underflow resolution indexing.
- Native SOT validates its fixed size and increasing part index, and retains
  the PHR twelve-byte empty-part convention. Declared count/completion cases
  follow the static whole-image path described below.
- Static native tile completion validates declared counts and applies the
  one-time TPsot==TNsot lookahead repair. Ready/buffered tiles are separate;
  EOC flushing, missing-EOC fallback, final non-EOC pairs and next-marker stop
  positions follow pinned control flow without copying input or image buffers.
- Native SIZ validates exact descriptor length/count before allocation. Packet
  component indices retain the native range through 16384 instead of narrowing
  to eight bits; no decoded-plane copies are introduced.
- Native COD and packet progression use the full 1..65535 layer count; the
  original codec entry keeps its 32-layer limit. Inclusion tag-tree values were
  already wide enough, and coding-pass counts remain independently bounded.
- `src/j2c/pdfium_progression.rs` translates native POC parsing and all five
  packet orders from j2k.c/pi.c (LICENSE-OPENJPEG-PI). Shared byte inclusion flags
  suppress overlaps with checked native strides; packet traversal remains lazy.
  Component resolution counts, partial tile origins and sampling steps are
  retained. Header/tile POC copies are small explicit native parameter copies.
- RGN retains native exact field/index checks and per-tile overrides. Native
  Maxshift increases starting bitplanes and shifts the doubled reconstruction
  midpoint before integer division/dequantization; bypass timing follows t1.c.
  Native high-bitplane failures are checked before coefficient decoding. Notices
  for translated coefficient logic are in LICENSE-OPENJPEG-T1.
- `src/j2c/pdfium_packed.rs` retains native PPM/PPT minimum sizes, duplicate and
  sparse sequence handling, Nppm widths/continuations and explicit merge copies.
  PPM consumes a global cursor in tile-ready order. Native segment parsing uses
  concatenated tile-part bodies and ranges into one reusable owned buffer, as
  j2k.c/t2.c do; input copies are explicit native copies, not borrow workarounds.
  Packet enumeration stops at expected packets and ignores unused body bytes.
  Additional tier-2 notices are in LICENSE-OPENJPEG-T2.
- Native COD/COC payloads are bounded and validated before use. COD resets
  previous component overrides/packet flags; COC affects its precinct flag but
  cannot override TCP SOP/EPH flags. Unknown COD progression is deferred to
  tier-2 so a valid POC can supersede it. Mixed HT and invalid precinct sizes
  retain native failures; pure HT decoding follows the native path described below.
- `src/j2c/ht` maps pinned ht_dec.c/t1_ht_luts.h with notices retained in
  LICENSE-OPENJPEG-HT. MEL/VLC/MRP/MagSgn/SPP readers, UVLC and exact tables are
  ported; production block decoding/wiring remains in progress. Reader byte
  stuffing, refill/termination values and alignment batching retain native order
  with safe slices and scalar offsets.
- HT cleanup/SPP/MRP decoding and production dispatch are connected, with
  reusable contiguous native coefficient/flag state, original quad/stripe update
  order, partial edges and causal membership. Native HT segment grouping and
  independent TERMALL/LAZY pass limits are retained; signed words convert before
  division/dequantization. Empty and zero-byte chunks are distinct. Native ROI,
  Mb/zero-bitplane/Scup/length checks and cleanup-only fallbacks are preserved.
  CAP/CPF have the pinned implementation's payload-only behavior. Repeated
  native sample bodies become fixed loops without changing update order.
- `src/j2c/pdfium_part2.rs` translates MCT/MCC/MCO/CBD metadata and element
  readers. Stable vector indices preserve native record links and replacement
  effects. Source bytes stay borrowed; matrix copies in per-tile parameter copies
  are explicit native copies. Native SIZ offsets survive CBD precision changes;
  MCO resets and applies offsets in source order. Matrix data is converted and
  retained, but static decoder COD accepts only MCT 0/1; custom MCT=2 is assigned
  only by the deferred encoder setup. Rust defines native undefined out-of-range
  float-to-int casts with saturation/NaN conversion.
- `src/j2c/pdfium_quantization.rs` retains fixed native QCD/QCC arrays, prefix
  updates, component override resets, clamped scalar derivation and additional
  style values. Native reversible reconstruction ignores quantization steps.
  Irreversible zero band gain is coupled to the historic two_invK constant in
  scalar/SIMD IDWT, with dwt.c notices in LICENSE-OPENJPEG-DWT. Native odd-origin
  singleton filtering and tile-local wavelet selection are retained.
- Native packet input in `src/reader.rs` follows bio.c's byte buffering, stuffed
  MSB behavior, alignment and zero-filled EOF without advancing beyond input.
  Notices are in LICENSE-OPENJPEG-BIO. The original bit reader is unchanged for
  the dependency's standard entry. Exhausted unbounded zero-bitplane prefixes
  return an error instead of native's integer-overflow/nonterminating iteration.
- `src/j2c/pdfium_markers.rs` retains main-header pair recovery/state checks,
  recovered SIZ validation, tile unknown-marker failure and native ignored-marker
  payload constraints. TLM seek-index storage is omitted for CJPX whole-image
  decode, which never selects an individual tile; invalid TLM entries are ignored
  in that path. PLT tail checks preserve unsigned wrapping and zero tails.
- Native MQ/HT packet segments retain full pass/missing-plane counts and
  independent 109/TERMALL/LAZY boundaries. Reconstruction uses actual native
  bitplanes and complete needed segment bytes, ignoring passes beyond available
  planes; nonpositive/native-empty cases reconstruct zero. Native raw bypass
  preserves stuffed bits, marker hold and artificial FFFF padding (notices in
  LICENSE-OPENJPEG-MQC). Segmentation-symbol values are consumed without error.
- Exact native reduction counts are checked at each COD/COC marker in main
  and tile headers, before later coding-style overrides; requests above 32
  fail at the adapter boundary before parsing.
  Overskipping an otherwise valid component now has its own
  `TooManySkippedResolutionLevels` validation error. The exact codestream entry
  still fails; the first-party rendering adapter can distinguish a resolution
  hint from malformed metadata and retry only this condition with fewer skipped
  levels. All other metadata/format/decoding failures remain failures.
- Native tier-2 state retains each POC index and per-component resno_decoded,
  including empty packets and the first-skipped fallback. IDWT reconstructs
  only the actual visited resolution. Reusable TCD tile buffers, native MCT
  sample-count/actual-level validation, component-zero word interpretation,
  strided DC shifts and actual-resolution image copies follow t2.c/tcd.c/j2k.c.
  The whole single-tile transfer uses its native geometry conditions. Signed
  DC addition wraps explicitly instead of invoking C signed-overflow undefined
  behavior; irreversible out-of-range samples clamp before offset addition.
- Native SIZ limits tile count to 65535 and saturates tile-grid boundary
  additions before clipping. Selected whole-image decoding retains strict
  packet/body bounds; non-strict and selected-tile decoder APIs are not used
  by CJPX's static caller.
- Horizontal reversible 5/3 lifting now uses the existing safe interleaved SIMD
  gather/store adapter, with scalar-equivalent FMA/floor ordering, mirrored
  edges, odd origins and tails. Both wavelets are checked bit-for-bit against
  scalar lifting across widths, origins and coefficient magnitudes.
- Reversible native DC level shifts keep conversion, wrapping addition and
  clamping in i32 when both bounds fit. Wider bounds retain the i64 clamp.
  This permits packed integer processing without changing native overflow,
  NaN/saturation or final f32 conversion; precision/offset boundary tests
  compare the original wide expression. Mac profiling and measurements:
  [codec report](../../benchmarks/codec-profile-mac-2026-10-24/README.md).

## Fused reversible row reconstruction

The 5/3 path now reads contiguous low/high subband rows directly, reconstructs
low samples in eight-lane batches, and interleaves those with the original high
samples. The existing second lift completes the row before vertical filtering.
This follows pinned OpenJPEG dwt.c::opj_idwt53_h's direct-from-band structure,
but retains the codec's f32 addition/FMA/floor order rather than native integer
shifts. It removes a separate interleave pass and the first lift's SIMD gathers;
no row buffers or allocations are added. Symmetric edges, singleton scaling,
origin parity, tails and the 9/7 path retain their existing results. Independent
interleave-then-scalar-lift regressions compare output bits and guards, including
signed zeros, subnormals and large finite coefficients. The mixed MQ/raw test's
fixture now supplies its real total coding-pass count, as production segment
parsing does, so its existing debug invariant is meaningful.

Measurements and validation: [Mac wavelet report](../../benchmarks/wavelet-opt-mac-2026-10-25/README.md).

Packaged as `pdfoxium-jpeg2000` 0.1.0. The distinct
package name preserves the upstream identity above while making the maintained
PDFium extensions available to downstream consumers without workspace patches.
