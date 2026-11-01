# Region rendering contract

At a fixed full-page scale and rotation, an in-bounds region must match the
corresponding whole-page crop within **2 levels per BGRA/RGBA channel**. The bound
applies to every pixel/channel: no alignment correction, averaging or exempt
pixel percentage. Glyph positions and full-page cache-history controls remain
exact. Tests report raw maximum differences and changed-pixel counts.

The renderer uses PDFium/AGG geometric Liang–Barsky clipping without a guard
band. An earlier exact-edge implementation imposed substantial curve work;
experiments comparing raw pixels and visual crops found only small AA changes
with ordinary clipping. A one-pixel guard did not justify its added work. Do not
reintroduce exact coverage machinery merely to remove an invisible AA delta.
Visible seams, missing strokes and shifted glyphs remain correctness failures.

Clipped BGRA spans without a knockout backdrop validate their destination,
coverage and mask slices once before using the specialized compositor. Masked
alpha retains `alpha * coverage * clip / 255 / 255`: rounding coverage and
clip together before applying alpha changes pixels. Full-cover mode ignores
path coverage, and zero source alpha preserves hidden destination RGB. Other
formats, backdrop operations and unusual span bounds retain the general path.

## Coordinates and caches

A device clip is measured from the top-left of the full scaled/rotated canvas.
It changes output allocation/origin, not scale. Synchronous, progressive and
borrowed-buffer jobs share the same path. No hidden full-page-render-and-crop
or padded temporary bitmap is required.

Compute canonical glyph position/LCD phase before integer tile translation.
Negative remainders and ties-away-from-zero rounding can otherwise move glyphs
when a region origin crosses zero. Preserve original glyph-outline quantization;
it has no page translation. Stroke translation likewise belongs in the final
matrix, rather than an inverse-transform round trip that loses an AGG subpixel.

The stroke builder's 32,000-unit safety clamp must not discard large source
coordinates that ultimately land on the page. Such paths are centered locally
before stroking, with the center restored in the final matrix. The center uses
only source bounds and linear scale, so tiles choose the same one; ordinary
bounded paths retain their existing arithmetic. Core and public C API controls
cover positive/negative large offsets and visible borders, alongside exact
whole-page-versus-region coverage.

Culling needs AA/minimum-width-stroke margins and conservative miter handling.
Pattern cached-cell/direct mode must depend on complete geometry rather than
clip dimensions. Image interpolation needs neighboring source samples, including
its inclusive final sample; affine sampling phase must be anchored to the whole
image. Decode reduction/cache validity uses [drawn geometry](image-decode-resolution.md),
so thumbnail-first or larger-zoom-first requests cannot change final pixels.

Lazy images retain the current converted scanline and a small bounded row
history to avoid decoder rewinds at tile-row interpolation halos. Account that
storage and release it with the cache entry. This does not change decode-level
policy or justify reusing differently reduced image planes.

## Rectangle snapping

Tile-stable rectangle snapping chooses the trimmed fractional edge before
integer translation. Native adds translation earlier; f32 rounding can choose
the other side of a tie. For example `0.6f32 * 100` is slightly greater than 60,
while adding 30 can round it to exactly 90. This affects a synthetic glyph edge,
N-up backgrounds and undersampled PDF417 modules.

Restoring native arithmetic makes three C API comparisons pass but introduces
a full-channel same-engine rectangle tile mismatch. A separate roundoff
stabilization trial changed other native boundaries. Both were discarded.
The C API checks now assert each engine's predicted boundaries explicitly:
the two N-up background rows, the two undersampled PDF417 module rows, and the
100pt synthetic glyph's extra column. Those pixels are checked independently
before comparing the remaining image; no general tolerance is increased.
The font outline bounds and same-engine whole-page/tile contracts remain
independent assertions. See [native differences](capi-known-native-differences.md).

## Reproduction

```sh
cargo test -p pdfoxium --release --test render_regions
# Optional external inputs must be fetched first:
cargo test -p pdfoxium --release --test render_regions corpus_tiles -- --ignored --nocapture
```

`PDFOXIUM_REGION_CASE` and `PDFOXIUM_REGION_SCALE` select corpus cases/scales;
an empty selection fails. Tests cover text/hairlines, quarter turns, images,
masks, gradients, patterns, annotations, zoom/cache histories and borrowed
progressive targets. These are selected same-engine contracts, not a claim of
universal native pixel parity. The broader [C API comparisons](../tests/capi/README.md)
retain independent assertions for native defects.
