# Vendored moxcms

Source: [awxkee/moxcms](https://github.com/awxkee/moxcms), master revision
[`097dccf3a9ab40811f23baa5da71cbfb985f8de8`](https://github.com/awxkee/moxcms/commit/097dccf3a9ab40811f23baa5da71cbfb985f8de8).
Upstream still declares version 0.9.1 at this revision. This local package is
`pdfoxium-moxcms` 0.1.0; its Rust library name remains `moxcms`.
The source and original BSD-3-Clause / Apache-2.0 notices are retained. Upstream
application, fuzz workspace and benchmark photograph are not redistributed.
The profiles used by upstream unit tests retain their embedded notices; see
[the fixture provenance](assets/NOTICE.md).

## Local fixes

The direct stage pipeline (`any_to_any`) must apply the same black-point
compensation as the resampled LUT pipeline. The local stage is inserted in XYZ
PCS, after source Lab conversion and before destination Lab conversion. It uses
upstream's existing black-point detection and compensation arithmetic. Relative
and absolute colorimetric intents retain their uncompensated behavior.

The core enables `any_to_any` so profiles with degenerate, discontinuous or
non-monotonic curves can use upstream's direct accuracy fallback. Most profiles
retain upstream's fast path. This snapshot also includes upstream's correction
to round integer gamma-table entries rather than truncate them.

Lab source profiles also use direct evaluation. Sampling the composed
Lab-to-XYZ-to-sRGB transform into a 33³ LUT introduces neutral and near-white
colour errors even for an identity Lab profile. The core retains fractional Lab
inputs through its float executor and quantizes only the resulting RGB bytes;
packed image rows retain the byte executor. Original synthetic core and public
C API tests cover neutral endpoints, fractional spot tints and visible swatches.

CMYK source black points are no longer skipped. V2 perceptual/saturation input
evaluates the darkest all-ink endpoint through the selected A2B table, decodes
the PCS Lab/XYZ units, bounds lightness while retaining chroma, and supplies
physical XYZ to the existing compensation stage. V4 uses its specified
perceptual black. Relative/absolute colourimetric transforms remain unchanged.
The endpoint algorithm follows LittleCMS `BlackPointAsDarkerColorant`; its MIT
notice is retained in [LICENSE-LCMS](LICENSE-LCMS). The package's SPDX expression
includes these additional terms. An original synthetic v2 CMYK regression
checks compensated shadows, black, white and uncompensated intents. The upstream
v2/v4 Lab-encoding control now uses relative/absolute intents, since perceptual
v2/v4 black-point behaviour intentionally differs.

Do not replace this with a consumer-side `[patch]`: the versioned path dependency
makes the fix part of downstream builds and packaged releases. Remove the fork
when a published upstream release includes equivalent behavior, after focused
ICC and corpus comparisons.

Original synthetic regression tests cover perceptual/saturation black mapping,
uncompensated colorimetric intents, and integer versus float gamma quantization.
The black-mapping test fails on the unmodified pinned master.

The direct four-channel LUT input stage reuses its preceding PCS result for
identical consecutive integer samples. Like LittleCMS's `CachedXFORM`, this
avoids reevaluating a colour run; it does not resample the transform or change
its arithmetic. The cache is local to a conversion call, with no retained image
data or synchronization. Float inputs retain their original path, including
NaNs and signed zero. Synthetic byte/word controls compare exact PCS float bits
against independent one-pixel calls and exercise changes in every component.
