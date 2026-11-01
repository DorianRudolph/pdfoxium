# Image decode resolution follows drawn geometry

The pinned PDFium image renderer passes the destination bitmap's width and
height to `CPDF_ImageLoader::Start`. `CPDF_DIB::StartLoadDIBBase` computes
`floor(log2(max(1, min(source_width / requested_width,
source_height / requested_height))))`, using integer ratios. Its page-image
cache checks the decoded plane against the same request. The Rust port used
this behavior in both synchronous and progressive rendering.

A small destination can therefore select less image detail even when it is a
tile of a zoomed-in page. This is a decode choice, before resampling or clipping.
An independent fresh tile is needed to reproduce it: rendering the full page
first can warm the cache with a higher-resolution plane and hide the problem.
The destination may also be a small internal soft-mask surface, rather than the
page bitmap. Drawn edges remain unchanged by translating into such a surface.

## Geometry and codec policy

Both Rust rendering paths now pass `image_decode_size(image_to_device)`.
An image matrix maps a unit square. The request is
`(ceil(hypot(a, b)), ceil(hypot(c, d)))`, the two transformed source-axis edge
lengths. The matrix includes the image's placement and the enclosing page/form
transform. Translation, clipping and destination bitmap dimensions do not
participate. A rotated bounding box would swap or inflate these source-axis
requirements and is deliberately not used.

The helper evaluates the supplied float coefficients in double precision so
rounding just above an integer does not cross a decode-scale threshold. A
singular, nonfinite or unrepresentable linear transform yields `(0, 0)`, which
retains the existing no-reduction fallback. The skip formula is unchanged;
JPEG retains its 1/2, 1/4 and 1/8 IDCT choices and existing MCU-alignment guard.
JPEG 2000 uses its available wavelet resolutions.

A smaller drawn-size hint can exceed a valid JPX codestream's available main,
tile or component decomposition levels. Rendering now retries fewer skipped
levels only for a specific `TooManySkippedResolutionLevels` codec error. The
chosen level, rather than the original hint, updates dimensions and the cache's
full-detail proof. Malformed input and every other decoder failure remain
failures; the strict `CjpxDecoder::create` entry retains its exact-level behavior.
A valid 32-square JPX with zero decomposition levels remains visible when drawn
as one pixel. Pinned native instead drops that image in a one-pixel destination
because OpenJPEG rejects the excessive reduction.

## Cache coordination

The same drawn-size request governs cache validity. Increasing the drawn extent
invalidates an insufficient reduced plane. The preceding full-source reuse
optimization remains: a complete full-resolution plane can serve larger
requests when its successful decode inputs match.

Smaller requests make ordinary dimension-valid hits more frequent. These hits
now also require the successful decode-input record, including resolved color
spaces, mask inputs, group family, conversion flags and the CMYK conversion
revision. Otherwise a shared stream drawn under another resource/group scope
could reuse incorrect colors. Resolution occurs without live cache borrows and
the outer cursor is restored after callback reentry. Incomplete masks still
prevent a reusable input proof. A failed reload retains both the old proof and
its successful request's bounded/unbounded status; it cannot mark an old
reduced plane as suitable for every size.

The 60 MB realization threshold is unchanged. Correcting an under-detailed
decode can increase its working plane and cache pressure; conversely, smaller
figures can use less memory. The existing explicit image
editing/reset invalidation contract also remains. The phase-3 region gate now
requires the successful decode inputs to match the normalized requested reduction
level too: JPEG levels are capped at three, JPX retains the original request before
available-level fallback, and non-reducible codecs use zero. Requests within one
level still reuse the plane; crossing levels decodes again, even when the cached
plane has more detail. This makes the resampling input independent of zoom history.
Different JPX requests that fall back to the same actual level may conservatively
miss the cache. Cache accounting replaces the previous plane's contribution on
reload, rather than repeatedly accumulating its size. See the
[region gate](region-rendering.md) for pixel and zoom-history validation.
