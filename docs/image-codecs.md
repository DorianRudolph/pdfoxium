# Portable image decoding

XFA image loading uses `fxcodec::ProgressiveDecoder`: format detection, frame
metadata, checked bitmap allocation, format conversion and bounded output-row
continuations. The decoder retains compressed input read through the existing
seekable stream API. Rust ownership replaces PDFium's codec delegate backpointers;
no native decoder is linked into production.

BMP headers, palettes, bitfields and RLE4/RLE8 follow the pinned decompressor.
GIF headers, extensions, frame metadata, interlace, transparency and LZW follow
the pinned GIF sources. The selected XFA decode path renders the first frame,
matching its native caller. Invalid offsets, truncated input, invalid RLE commands
and full LZW tables have checked bounds instead of native memory hazards.

JPEG reuses the existing Rust JPEG adapter for grayscale, RGB and CMYK/YCCK
samples, with JFIF density and retained-input/error/output continuation handled
by the progressive pipeline. Native libjpeg callback and source-manager objects
are replaced by checked retained buffers. BMP decoding also supports two-bit
palettes and V2/V3/V4/V5 embedded bitfields with 16/32-bit alpha. Explicit alpha
keeps a BGRA bitmap through progressive and XFA loading, correcting native
GetBitmapFormat's blanket BGRx choice that discards the channel.

PNG uses published `png` 0.18 with PDFium's output-format and gamma policy.
TIFF uses published `tiff` 0.11.3 with explicit pure Rust deflate/fax/jpeg/lzw
features; the optional native zstd dependency is disabled. A small seekable view
borrows the compressed data and appends only modified directory entries. That
lets the published decompressors produce raw palette indices, Lab samples and
packed YCbCr clumps without copying or reparsing the compressed image payload.
Classic TIFF/BigTIFF, byte order, frame selection, strips/tiles, planar samples,
colormaps, physical DPI and the native alpha/16-bit conversion rules are handled
at this boundary. The native caller requests the file's own orientation, so
stored sample ordering is retained. The native integer YCbCr tables and Lab
matrix/gamma policy are translated from the pinned libtiff conversion routines.

NeXT two-bit RLE, ThunderScan four-bit run/delta, SGILog/SGILog24 display
decoding and PixarLog 11-bit prediction/linear tables follow the pinned decode
algorithms. PixarLog reuses existing Rust zlib support. CCITT RLE, word-aligned
RLE and Group 3/4 reuse the engine's existing pure Rust fax decoder. Compressed
FillOrder bit reversal preserves directory bytes and borrowed source ownership.
Uncompressed fax extension codes and OldJPEG are unsupported by the pinned
native configuration too. Unsupported input returns an error and never a successful empty image.

`cxfa_ffimage` connects embedded/base64 image bytes and explicit owner services
for named PDF images and linked files. It supplies decoded bitmaps and physical
DPI to `cxfa_imagerenderer`, which uses the existing AGG continuation. The host
controls linked-file access. Concrete FF Image/ImageEdit widgets compose the
shared FF Widget/Field layer. Bitmap identity, named-image state and physical DPI
are owned by the node, shared by layout fragments and the real image sizing
path. Loading and image-edit updates release node borrows before owner callbacks;
bitmap clear/pre-finalize preserves native DPI metadata. Image edit attaches the
actual FWL picture box and forwards input through the shared widget adapter.

## JPEG adapter

The production adapter in `crates/pdfoxium-core/src/fxcodec/jpeg` uses the
Rust-only, MIT/Apache-2.0 `libjpeg-turbo-rs` codec, pinned to
`b614c77936e56e6409c06af87fb104a5e75e64ee`. Its unchanged production sources
are packaged as `pdfoxium-jpeg` in `third_party/libjpeg-turbo-rs`; the upstream
published 0.8.0 package predates the evaluated fixes. The maintained Hayro codec
packages likewise use direct, versioned dependencies under `pdfoxium-*` names,
so downstream builds retain our extensions without workspace patches. No C
codec or native fallback is a production dependency. TIFF retains its independent
codec dependency.

Two-component JPEGs used by PDF DeviceN color spaces are decoded through a
narrow Zune 0.5.15 fallback. The primary codec lacks raw two-component output;
rejecting those images omitted issue6364 after the initial switch. This fallback
retains the previous borrowed input reader and scalar box reduction, explicitly
requests raw MultiBand(2) output, and uses the shared PDF repairs, MCU alignment
policy and EOI boundary. Gray/RGB/CMYK decoding stays on libjpeg-turbo-rs.
The issue6364 render is byte-identical to the pre-switch adapter at both DPIs;
unit and public C API tests assert two-component sample and tint behavior.

The behavior guide is pinned PDFium's
`core/fxcodec/jpeg/libjpeg_scanline_decoder.cpp`, `jpegmodule.cpp` and
`core/fpdfapi/page/cpdf_dib.cpp`. The existing DIB caller selects
`1 << min(resolution_levels_to_skip, 3)` and adopts the actual decoded size.
The adapter now applies 1/2, 1/4 or 1/8 scaling inside IDCT instead of decoding
at full size and applying a box filter. Like `InitDecode`, it disables reduction
unless both dimensions are multiples of their largest component sampling
factor times eight. Padded edge blocks can contain arbitrary encoder padding
that reduced IDCT would mix into visible pixels. Invalid denominators fail.

The default ISLOW DCT, fancy chroma upsampling and progressive block smoothing
follow libjpeg defaults. Grayscale remains one component. Three-component
images normally produce RGB; disabling PDF ColorTransform preserves raw YCbCr,
unless an Adobe marker forces transformation. Already-RGB JPEGs stay RGB.
CMYK and YCCK produce four CMYK samples, retaining K and the PDF Decode-array
interpretation. The adapter may decline BGR output as before; callers retain
their existing RGB-to-BGR handling.

SOI scanning, private trailer repair and known corrupt-height repairs remain.
The existing Hayro-derived PDF-dictionary dimension repair is also retained
and remains broader than native PDFium's two special height repairs.
Header probing uses the pinned codec's header-only entry point so later
progressive scans do not change LoadInfo. The codec does not expose a public
consumed-byte cursor: a bounded JPEG marker walk skips opaque payloads,
stuffed entropy bytes and restart markers to find the first EOI. Following
inline PDF tokens and subsequent JPEGs are excluded from the consumed offset.

Decoding is still eager. Scanline rewind reuses the completed pixel allocation,
matching the previous Rust adapter rather than native C's demand-driven rows.
The large partially clipped issue12841 image therefore still decodes all rows.
SIMD is controlled by the core `simd` feature; the old `jpeg-box-simd` spelling
is retained as a compatibility alias. The obsolete box reducer is removed.

The source-derived module tests include native C reference pixels and complete
packed-output hashes at all four scales, raw YCbCr, RGB, gray and Adobe YCCK,
padded-edge fallback, source repairs, inline endpoints and rewind. Native
reference generation is test-only; rendered comparisons use native PDFium.
See [performance guidance](../benchmarks/README.md) for the eager-versus-demand-driven
work distinction and measurement controls.
