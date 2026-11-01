// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Safe JPEG boundary from `jpegmodule`, `rust_jpeg_scanline_decoder` and
//! `rust_jpeg_ffi.rs`. libjpeg-turbo-rs supplies the codec; PDF color-transform selection,
//! source repairs, output dimensions and sample ordering stay in this adapter.

use libjpeg_turbo_rs::{ColorSpace, DecodeLimits, Decoder, PixelFormat, ScalingFactor};
use std::borrow::Cow;

mod endpoint;
mod multiband;
mod scanline;
pub use scanline::JpegScanlineDecoder;

#[derive(Clone, Copy)]
pub struct JpegInfo {
    pub width: i32,
    pub height: i32,
    pub components: u32,
    pub bits_per_components: i32,
    pub color_transform: bool,
    max_h_samp: usize,
    max_v_samp: usize,
}

pub struct JpegPixels {
    pub info: JpegInfo,
    pub pixels: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub components: u32,
    pub bytes_consumed: u32,
}

fn scan_soi(source: &[u8]) -> &[u8] {
    source
        .windows(2)
        .position(|bytes| bytes == [255, 216])
        .map_or(source, |offset| &source[offset..])
}

fn prepare_source(source: &[u8]) -> Option<Cow<'_, [u8]>> {
    let mut source = Cow::Borrowed(scan_soi(source));
    if source.len() < 2 {
        return None;
    }
    let len = source.len();
    if source[len - 2..] != [255, 217] {
        source.to_mut()[len - 2..].copy_from_slice(&[255, 217]);
    }
    Some(source)
}

// Hayro's maybe_patch_jpeg_dimensions policy: the PDF image dictionary bounds
// a JPEG whose frame claims more pixels. Patch only a private compressed copy;
// ordinary images keep borrowing the stream's bytes. This is deliberately
// broader than PDFium's two known-invalid-height header repairs below.
fn maybe_patch_jpeg_dimensions(source: &mut Cow<'_, [u8]>, width: i32, height: i32) {
    let (Ok(width), Ok(height)) = (u16::try_from(width), u16::try_from(height)) else {
        return;
    };
    if width == 0 || height == 0 {
        return;
    }
    let Some(offset) = find_sof_marker(source) else {
        return;
    };
    let jpeg_height = u16::from_be_bytes([source[offset + 5], source[offset + 6]]);
    let jpeg_width = u16::from_be_bytes([source[offset + 7], source[offset + 8]]);
    if u64::from(jpeg_width) * u64::from(jpeg_height) <= u64::from(width) * u64::from(height) {
        return;
    }
    let source = source.to_mut();
    source[offset + 5..offset + 7].copy_from_slice(&height.to_be_bytes());
    source[offset + 7..offset + 9].copy_from_slice(&width.to_be_bytes());
}

fn find_sof_marker(source: &[u8]) -> Option<usize> {
    let mut offset = 2; // prepare_source() has already located SOI.
    while *source.get(offset)? == 255 {
        let marker = *source.get(offset + 1)?;
        if marker == 255 {
            offset += 1;
            continue;
        }
        if matches!(marker, 0xda | 0xd9) {
            return None; // Never scan entropy data for apparent frame markers.
        }
        if matches!(marker, 0x01 | 0xd8 | 0xd0..=0xd7) {
            offset += 2;
            continue;
        }
        let length =
            u16::from_be_bytes([*source.get(offset + 2)?, *source.get(offset + 3)?]) as usize;
        if length < 2 || length > source.len().checked_sub(offset + 2)? {
            return None;
        }
        if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
            return (length >= 8).then_some(offset);
        }
        offset += length + 2;
    }
    None
}

// The SOF component count is byte 9 after its marker. find_sof_marker() walks
// complete header segments and stops before entropy data.
fn is_two_component_jpeg(source: &[u8]) -> bool {
    find_sof_marker(source)
        .and_then(|offset| source.get(offset + 9))
        .is_some_and(|&count| count == 2)
}

fn patch_known_bad_height(source: &mut Cow<'_, [u8]>, width: i32, height: i32) -> bool {
    for offset in [94, 163] {
        if source.len() > offset + 3
            && source[offset - 5] == 255
            && (0xc0..=0xcf).contains(&source[offset - 4])
            && source[offset..offset + 4] == [255, 255, (width >> 8) as u8, width as u8]
        {
            source.to_mut()[offset..offset + 2].copy_from_slice(&(height as u16).to_be_bytes());
            return true;
        }
    }
    false
}

fn adobe_marker(source: &[u8]) -> bool {
    let mut offset = 2;
    while offset + 1 < source.len() && source[offset] == 255 {
        let marker = source[offset + 1];
        offset += 2;
        if marker == 255 {
            offset -= 1;
            continue;
        }
        if marker == 0xda || marker == 0xd9 {
            break;
        }
        if marker == 0xd8 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let Some(length) = source.get(offset..offset + 2) else {
            break;
        };
        let length = u16::from_be_bytes([length[0], length[1]]) as usize;
        if length < 2 || length > source.len() - offset {
            break;
        }
        if marker == 0xee && length >= 14 && source[offset + 2..offset + 7] == *b"Adobe" {
            return true;
        }
        offset += length;
    }
    false
}

pub fn load_info(source: &[u8]) -> Option<JpegInfo> {
    // LoadInfo scans SOI, but performs neither trailer nor corrupt-height repair.
    let source = scan_soi(source);
    if is_two_component_jpeg(source) {
        return multiband::load_info(source);
    }
    // Match jpeg_read_header: later progressive scans are irrelevant to metadata.
    // This pinned codec exposes its header-only entry point for this purpose.
    let decoder = Decoder::new_header_only(source, DecodeLimits::default()).ok()?;
    info_from_decoder(&decoder)
}

/// JpegModule::CreateDecoder's selected Rust backend. Like PDFium's Rust
/// decoder, it leaves BGR preference unfulfilled and reports RGB scanlines.
pub fn create_decoder(
    source: &[u8],
    width: u32,
    height: u32,
    components: i32,
    color_transform: bool,
    scale_denom: u32,
    _prefer_bgr_output: bool,
) -> Option<JpegScanlineDecoder> {
    JpegScanlineDecoder::create(
        source,
        width,
        height,
        components,
        color_transform,
        scale_denom,
    )
}

fn info_from_decoder(decoder: &Decoder<'_>) -> Option<JpegInfo> {
    let frame = decoder.header();
    if frame.width == 0
        || frame.height == 0
        || frame.width > 65500
        || frame.height > 65500
        || frame.precision != 8
        || !matches!(frame.components.len(), 1 | 3 | 4)
    {
        return None;
    }
    Some(JpegInfo {
        width: i32::from(frame.width),
        height: i32::from(frame.height),
        components: frame.components.len() as u32,
        bits_per_components: i32::from(frame.precision),
        color_transform: matches!(
            decoder.jpeg_color_space(),
            ColorSpace::YCbCr | ColorSpace::Ycck
        ),
        max_h_samp: frame
            .components
            .iter()
            .map(|c| usize::from(c.horizontal_sampling))
            .max()?
            .max(1),
        max_v_samp: frame
            .components
            .iter()
            .map(|c| usize::from(c.vertical_sampling))
            .max()?
            .max(1),
    })
}

pub fn decode(
    source: &[u8],
    width: i32,
    height: i32,
    color_transform: bool,
    scale_denom: u32,
) -> Option<JpegPixels> {
    decode_with_validation(source, width, height, 0, color_transform, scale_denom)
}

pub(crate) fn decode_with_validation(
    source: &[u8],
    width: i32,
    height: i32,
    components: i32,
    color_transform: bool,
    scale_denom: u32,
) -> Option<JpegPixels> {
    if !matches!(scale_denom, 1 | 2 | 4 | 8) {
        return None;
    }
    let mut source = prepare_source(source)?;
    maybe_patch_jpeg_dimensions(&mut source, width, height);
    if is_two_component_jpeg(&source) {
        return multiband::decode(&mut source, width, height, components, scale_denom);
    }
    let mut accept_known_bad_header = true;
    loop {
        let decoder = Decoder::new_with_limits(source.as_ref(), DecodeLimits::default());
        let mut decoder = match decoder {
            Ok(decoder) if info_from_decoder(&decoder).is_some() => decoder,
            _ => {
                if !accept_known_bad_header || !patch_known_bad_height(&mut source, width, height) {
                    return None;
                }
                accept_known_bad_header = false;
                continue;
            }
        };
        let info = info_from_decoder(&decoder)?;
        if (info.components as i32) < components || (info.width as u32) < width as u32 {
            return None;
        }
        // LibjpegScanlineDecoder::InitDecode: Adobe overrides ColorTransform;
        // otherwise preserve raw three-component samples when it is disabled.
        match info.components {
            1 => decoder.set_output_format(PixelFormat::Grayscale),
            4 => decoder.set_output_format(PixelFormat::Cmyk),
            3 => {
                decoder.set_output_format(PixelFormat::Rgb);
                if !color_transform
                    && !adobe_marker(&source)
                    && decoder.jpeg_color_space() == ColorSpace::YCbCr
                {
                    decoder.set_output_colorspace(ColorSpace::YCbCr);
                }
            }
            _ => return None,
        }
        let orig_width = info.width as usize;
        let orig_height = info.height as usize;
        // InitDecode disables scaled IDCT if any component has padded edge
        // blocks: encoder padding must not contaminate the visible edge.
        let scale = if orig_width % (info.max_h_samp * 8) == 0
            && orig_height % (info.max_v_samp * 8) == 0
        {
            scale_denom as usize
        } else {
            1
        };
        decoder.set_scale(ScalingFactor::try_new(1, scale as u32).ok()?);
        decoder.set_lenient(true);
        decoder.set_block_smoothing(true); // libjpeg's default for progressive JPEG.
                                           // Keep libjpeg's default ISLOW DCT and fancy chroma upsampling.
        #[cfg(feature = "render-profile")]
        if std::env::var_os("PDFOXIUM_TRACE_JPEG_REDUCTION").is_some() {
            eprintln!("jpeg-reduction width={orig_width} height={orig_height} requested={scale_denom} effective={scale} components={}", info.components);
        }
        let image = decoder.decode_image().ok()?;
        let out_width = orig_width.div_ceil(scale);
        let out_height = orig_height.div_ceil(scale);
        let size = out_width
            .checked_mul(out_height)?
            .checked_mul(info.components as usize)?;
        if image.width != out_width || image.height != out_height || image.data.len() != size {
            return None;
        }
        // The codec has no public consumed-input cursor. Walk JPEG framing to
        // preserve the inline-image endpoint, excluding following PDF tokens.
        let bytes_consumed = endpoint::jpeg_endpoint(&source)?;
        return Some(JpegPixels {
            info,
            pixels: image.data,
            width: out_width as i32,
            height: out_height as i32,
            components: info.components,
            bytes_consumed,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_jpeg_dimensions_decode_like_the_original_without_mutating_input() {
        for jpeg in [
            include_bytes!("../../../tests/fixtures/gray.jpg").as_slice(),
            include_bytes!("../../../tests/fixtures/cmyk.jpg").as_slice(),
        ] {
            let info = load_info(jpeg).unwrap();
            let expected = decode(jpeg, info.width, info.height, true, 1).unwrap();
            let sof = find_sof_marker(jpeg).unwrap();
            let mut oversized = jpeg.to_vec();
            oversized[sof + 5..sof + 7].copy_from_slice(&((info.height * 2) as u16).to_be_bytes());
            let saved = oversized.clone();
            let actual = decode(&oversized, info.width, info.height, true, 1).unwrap();
            assert_eq!(
                (actual.info.width, actual.info.height),
                (info.width, info.height)
            );
            assert_eq!(
                (actual.width, actual.height, actual.components),
                (expected.width, expected.height, expected.components)
            );
            assert_eq!(actual.pixels, expected.pixels);
            assert_eq!(oversized, saved);
            assert_eq!(load_info(&oversized).unwrap().height, info.height * 2);
        }
    }

    #[test]
    fn jpeg_dimension_repair_uses_area_and_only_walks_complete_header_segments() {
        // APP payload contains a fake frame; the real SOF has fill padding.
        let source = [
            255, 216, 255, 224, 0, 6, 255, 192, 0, 0, 255, 255, 192, 0, 8, 8, 0, 2, 0, 8, 0, 255,
            217,
        ];
        let sof = find_sof_marker(&source).unwrap();
        assert_eq!(sof, 11);
        for (width, height) in [(8, 2), (4, 4), (4, 8), (0, 1), (1, 0), (-1, 1), (65536, 1)] {
            let mut data = Cow::Borrowed(source.as_slice());
            maybe_patch_jpeg_dimensions(&mut data, width, height);
            assert!(matches!(data, Cow::Borrowed(_)), "{width}x{height}");
        }
        let mut data = Cow::Borrowed(source.as_slice());
        maybe_patch_jpeg_dimensions(&mut data, 4, 1);
        assert!(matches!(data, Cow::Owned(_)));
        assert_eq!(&data[sof + 5..sof + 9], &[0, 1, 0, 4]);
        assert_eq!(&source[sof + 5..sof + 9], &[0, 2, 0, 8]);
        for bytes in [
            &[255, 216, 255, 218, 0, 2, 255, 192, 0, 8, 8, 0, 2, 0, 8, 0][..],
            &[255, 216, 255, 224, 0, 1, 255, 192, 0, 8, 8, 0, 2, 0, 8, 0][..],
            &[255, 216, 255, 192, 0, 8, 8, 0, 2][..],
            &[255, 216, 255, 192, 0, 7, 8, 0, 2, 0, 8][..],
        ] {
            let mut data = Cow::Borrowed(bytes);
            maybe_patch_jpeg_dimensions(&mut data, 1, 1);
            assert!(matches!(data, Cow::Borrowed(_)));
        }
    }

    #[test]
    fn corrupt_height_repair_is_decoder_only_and_trailer_repair_keeps_input() {
        let jpeg = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/gray.jpg"
        ));
        let info = load_info(jpeg).unwrap();
        assert_eq!(&jpeg[89..91], &[0xff, 0xc0]);
        let mut corrupt = jpeg.to_vec();
        corrupt[94..96].fill(0xff);
        assert!(load_info(&corrupt).is_none());
        let decoded = decode(&corrupt, info.width, info.height, true, 1).unwrap();
        assert_eq!((decoded.width, decoded.height), (info.width, info.height));
        assert_eq!(&corrupt[94..96], &[0xff, 0xff]);
        // Width must match the narrowly recognized broken header.
        // With no PDF height, only the existing narrow repair applies.
        assert!(decode(&corrupt, info.width + 1, 0, true, 1).is_none());
        let len = corrupt.len();
        corrupt.copy_from_slice(jpeg);
        corrupt[len - 2..].fill(0);
        assert!(load_info(&corrupt).is_some());
        assert!(decode(&corrupt, info.width, info.height, true, 1).is_some());
        assert_eq!(&corrupt[len - 2..], &[0, 0]);
    }
}

#[cfg(test)]
#[path = "jpegmodule_unittest.rs"]
mod upstream_tests;
