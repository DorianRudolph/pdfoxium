# Synthetic image draw-size fixtures

Both images contain a 2048×2048 two-pixel grayscale checkerboard carried in
three equal RGB channels. Samples are 64 when `(x / 2 + y / 2)` is even and 192
otherwise, using integer division. There is no third-party artwork. The high
spatial frequency makes an erroneously reduced decode visibly distinct from a
full-resolution decode.

`generate.py` records the complete generation procedure. It uses libjpeg-turbo's
optional `cjpeg` encoder (version 3.2.0, build 20260630) to produce baseline
three-component JPEG at quality 95 with 4:2:0 sampling (`2x2,1x1,1x1`). The 2048
source dimensions are exact multiples of 16, so PDFium's MCU-alignment guard
allows 1/2, 1/4 and 1/8 decoding. OpenJPEG's optional `opj_compress` encoder
(version 2.5.4) produces lossless reversible JP2 with six resolution levels and
default RGB multiple-component transform. Neither encoder is linked into
production or the C API test runner; ordinary tests read the committed bytes.

Regenerate from this directory or the repository root with:

```sh
python3 tests/capi/fixtures/image-drawn-size/generate.py
```

The public API tests place the same source at different drawn sizes. Fresh
page/document state for each render prevents a full-page decode from warming a
tile's image cache. Whole renders and region renders retain identical zoom;
only their output bitmap/clip changes. JPX embedding uses a minimal fixture
because the public API has no JPX-loading constructor; JPEG setup uses public
image construction, inline loading, insertion, content generation and saving.

Generated fixture identities:

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `checker-2048-420.jpg` | 2,294,385 | `8fd76a71c162c8fcc04a2c6acab88853aa34a00b80782d7020279e810303c432` |
| `checker-2048-lossless.jp2` | 17,342 | `f673e666555eae982ff2739b7f3421f33a55878a1043fe219f5fca0c8a341369` |

The axis-aligned tile mosaic is exercised with normal image sampling as well
as `FPDF_RENDER_NO_SMOOTHIMAGE`. Affine interior clips use no smoothing and
integral shear/quarter-turn transforms to isolate decode resolution from the
separate bilinear-resampling halo/edge-rounding behavior. Matrix region tests
use a 512-square destination and translation, rather than merely restricting
the clip on a full-size destination. Axis-aligned and quarter-turn Rust regions
must exactly match full-page crops; warm-full replays are exact controls there.

The integral-shear scene exposed an older shared clipping-phase difference:
even native's full-resolution warm replay differs from the full-page crop.
That case therefore compares Rust's fresh clipped render **exactly** with its
full-decode, same-geometry clipped control, while pinned native's fresh reduced
render must differ. Both engines must report identical whole-vs-clip changed
pixel locations. No affine tolerance is introduced, and the test does not claim
the decode-size fix repairs that separate shared geometry behavior. Full-page
cross-engine comparisons retain the established JPEG rounding bound (two channel
levels); reversible JPX is exact.

`gray-32-no-decomposition.jp2` is a separate 32×32 uniform RGB128 fixture,
encoded with `opj_compress -n 1 -r 1`. Its COD marker is checked to contain zero
decomposition levels. It is 214 bytes, SHA-256
`7162dc1d39ef6b275683b1914ba7eb7385dcc259a533a7a307c889745f83272e`.
Use `generate.py --tiny-only` to regenerate just this edge fixture.

The one-pixel public API scene must remain visible in a 64-square canvas even
though its drawn-size hint requests five skipped levels. Rust clamps to the
levels the codestream actually supplies and produces the exact same gray128
pixel in a fresh one-pixel destination. Pinned native paints the 64-square
control correctly (its old bitmap-size hint requests zero skipped levels), but
its one-pixel destination requests five and OpenJPEG rejects the codestream,
leaving white. This is independently asserted for both offset and matrix APIs,
including both writers/readers. The test does not permit a valid JPX image to
disappear as an optimization fallback.
