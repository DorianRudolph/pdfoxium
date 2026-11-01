// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! JPEG2000 decoder boundary types from `core/fxcodec/jpx/cjpx_decoder.h`.

#[cfg(feature = "simd")]
mod component_simd;
mod jp2;
mod sycc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JpxColorSpace {
    Unknown,
    Unspecified,
    SRgb,
    Gray,
    Sycc,
    Eycc,
    Cmyk,
}

#[derive(Clone, Copy)]
pub struct JpxImageInfo {
    pub width: u32,
    pub height: u32,
    pub channels: u32,
    pub colorspace: JpxColorSpace,
}

use crate::fxge::calculate_pitch::calculate_pitch32;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;

/// The adapter retains borrowed source identity and native component planes,
/// emitting samples straight into the caller's pitched bitmap.
pub struct CjpxDecoder<'a> {
    _source: &'a [u8],
    components: Vec<hayro_jpeg2000::PdfiumComponent>,
    info: JpxImageInfo,
}

enum JpxCreateError {
    InvalidInput,
    Codec(hayro_jpeg2000::DecodeError),
}

impl From<hayro_jpeg2000::DecodeError> for JpxCreateError {
    fn from(error: hayro_jpeg2000::DecodeError) -> Self {
        Self::Codec(error)
    }
}

impl<'a> CjpxDecoder<'a> {
    /// Exact reduction entry matching pinned CJPX_Decoder/OpenJPEG behavior.
    pub fn create(source: &'a [u8], indexed: bool, levels_to_skip: u8) -> Option<Self> {
        Self::create_exact(source, indexed, levels_to_skip).ok()
    }

    /// Rendering supplies a resolution hint, which may exceed a component's
    /// available main/tile coding levels. Retry only that specific rejection;
    /// malformed containers and all other decoding failures remain failures.
    pub(crate) fn create_with_resolution_hint(
        source: &'a [u8],
        indexed: bool,
        levels_to_skip: u8,
    ) -> Option<(Self, u8)> {
        let mut actual_skip = levels_to_skip;
        loop {
            match Self::create_exact(source, indexed, actual_skip) {
                Ok(decoder) => return Some((decoder, actual_skip)),
                Err(JpxCreateError::Codec(hayro_jpeg2000::DecodeError::Validation(
                    hayro_jpeg2000::ValidationError::TooManySkippedResolutionLevels,
                ))) if actual_skip > 0 => actual_skip -= 1,
                _ => return None,
            }
        }
    }

    fn create_exact(
        source: &'a [u8],
        indexed: bool,
        levels_to_skip: u8,
    ) -> Result<Self, JpxCreateError> {
        if source.len() < 12 || levels_to_skip > 32 {
            return Err(JpxCreateError::InvalidInput);
        }
        let mut header = jp2::Header::parse(source).ok_or(JpxCreateError::InvalidInput)?;
        let codestream =
            hayro_jpeg2000::pdfium::Codestream::new(header.codestream, levels_to_skip)?;
        if !header.dimensions_match(codestream.reference_dimensions()) {
            return Err(JpxCreateError::InvalidInput);
        }
        let (components, consumed) = codestream.decode_components_with_offset()?;
        let mut colorspace = header.colorspace;
        let mut components = header
            .apply_components(components, indexed)
            .ok_or(JpxCreateError::InvalidInput)?;
        header
            .finish(consumed)
            .ok_or(JpxCreateError::InvalidInput)?;
        sycc::start_decode_color(&mut components, &mut colorspace);
        let first = components.first().ok_or(JpxCreateError::InvalidInput)?;
        let info = JpxImageInfo {
            width: first.width,
            height: first.height,
            channels: components.len() as u32,
            colorspace,
        };
        Ok(Self {
            _source: source,
            components,
            info,
        })
    }

    pub fn get_info(&self) -> JpxImageInfo {
        self.info
    }

    pub fn decode(&self, bitmap: &mut CfxDibitmap, swap_rgb: bool, component_count: u32) -> bool {
        if component_count > self.info.channels {
            return false;
        }
        let channel_count = if component_count == 3 && self.info.channels == 4 {
            4
        } else {
            component_count
        };
        let Some(pitch) = calculate_pitch32((8 * channel_count) as i32, self.info.width as i32)
        else {
            return false;
        };
        if bitmap.pitch() < pitch
            || (swap_rgb && channel_count < 3)
            || bitmap.height() < self.info.height as i32
        {
            return false;
        }
        decode_components(&self.components, bitmap, swap_rgb, channel_count, self.info)
    }
}

fn decode_components(
    components: &[hayro_jpeg2000::PdfiumComponent],
    bitmap: &mut CfxDibitmap,
    swap_rgb: bool,
    channel_count: u32,
    info: JpxImageInfo,
) -> bool {
    let width = info.width as usize;
    let height = info.height as usize;
    let pitch = bitmap.pitch() as usize;
    let Some(size) = height.checked_mul(pitch) else {
        return false;
    };
    let Some(target) = bitmap.buffer_mut().get_mut(..size) else {
        return false;
    };
    target.fill(255);
    let Some(first) = components.first() else {
        return false;
    };
    if !(1..=31).contains(&first.precision)
        || components.iter().any(|component| {
            component.dx != first.dx
                || component.dy != first.dy
                || component.precision != first.precision
        })
    {
        return false;
    }
    let Some(count) = width.checked_mul(height) else {
        return false;
    };
    for (channel, component) in components.iter().take(channel_count as usize).enumerate() {
        if component.samples.is_empty() {
            continue;
        }
        if component.samples.len() < count {
            return false;
        }
        let output_channel = if swap_rgb && channel == 0 {
            2
        } else if swap_rgb && channel == 2 {
            0
        } else {
            channel
        };
        #[cfg(feature = "simd")]
        component_simd::store(
            component,
            target,
            width,
            height,
            pitch,
            channel_count as usize,
            output_channel,
        );
        #[cfg(not(feature = "simd"))]
        {
            // Retain the source's separate precision loops outside pixel iteration.
            if component.precision < 8 {
                let shift = u32::from(8 - component.precision);
                store_component(
                    component,
                    target,
                    width,
                    height,
                    pitch,
                    channel_count as usize,
                    output_channel,
                    |value| value.wrapping_shl(shift) as u8,
                );
            } else if component.precision == 8 {
                store_component(
                    component,
                    target,
                    width,
                    height,
                    pitch,
                    channel_count as usize,
                    output_channel,
                    |value| value as u8,
                );
            } else {
                let shift = u32::from(component.precision - 8);
                store_component(
                    component,
                    target,
                    width,
                    height,
                    pitch,
                    channel_count as usize,
                    output_channel,
                    |value| ((value >> shift) + ((value >> (shift - 1)) % 2)).clamp(0, 255) as u8,
                );
            }
        }
    }
    true
}

#[cfg(not(feature = "simd"))]
#[inline]
fn store_component(
    component: &hayro_jpeg2000::PdfiumComponent,
    target: &mut [u8],
    width: usize,
    height: usize,
    pitch: usize,
    channel_count: usize,
    output_channel: usize,
    convert: impl Fn(i32) -> u8,
) {
    let offset = if component.signed {
        1i32 << (component.precision - 1)
    } else {
        0
    };
    for row in 0..height {
        let samples = &component.samples[row * width..(row + 1) * width];
        let output = &mut target[row * pitch..(row + 1) * pitch];
        for (column, &sample) in samples.iter().enumerate() {
            let value = (sample as i32).wrapping_add(offset);
            output[column * channel_count + output_channel] = convert(value);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn component_stores_preserve_casts_precision_channels_and_padding() {
        let values = [
            f32::NEG_INFINITY,
            -2147483648.0,
            -65537.0,
            -257.0,
            -1.5,
            -0.5,
            0.0,
            0.5,
            1.5,
            127.0,
            128.0,
            255.0,
            256.0,
            511.0,
            32767.0,
            65535.0,
            2147483648.0,
            f32::INFINITY,
            f32::NAN,
        ];
        for width in [1usize, 7, 8, 9, 15, 16, 17, 31, 32, 33, 65] {
            for channels in [1usize, 2, 3, 4, 5, 257] {
                let pitch = width * channels + 3;
                for precision in 1u8..=31 {
                    for signed in [false, true] {
                        let mut actual = vec![0xa5; 3 * pitch];
                        let mut expected = actual.clone();
                        for channel in 0..channels {
                            if channels > 4 && ![0, 2, channels - 1].contains(&channel) {
                                continue;
                            }
                            let component = hayro_jpeg2000::PdfiumComponent {
                                width: width as u32,
                                height: 3,
                                dx: 1,
                                dy: 1,
                                precision,
                                signed,
                                samples: (0..3 * width)
                                    .map(|i| values[(i + channel) % values.len()])
                                    .collect(),
                            };
                            #[cfg(not(feature = "simd"))]
                            let convert = |value: i32| {
                                if precision < 8 {
                                    value.wrapping_shl(u32::from(8 - precision)) as u8
                                } else if precision == 8 {
                                    value as u8
                                } else {
                                    let shift = precision - 8;
                                    ((value >> shift) + ((value >> (shift - 1)) % 2)).clamp(0, 255)
                                        as u8
                                }
                            };
                            #[cfg(not(feature = "simd"))]
                            {
                                super::store_component(
                                    &component,
                                    &mut actual,
                                    width,
                                    3,
                                    pitch,
                                    channels,
                                    channel,
                                    convert,
                                );
                            }
                            #[cfg(feature = "simd")]
                            {
                                super::component_simd::store(
                                    &component,
                                    &mut actual,
                                    width,
                                    3,
                                    pitch,
                                    channels,
                                    channel,
                                );
                            }
                            for (index, &sample) in component.samples.iter().enumerate() {
                                // Widen the signed bias before narrowing to model
                                // modulo-i32 addition independently of the store.
                                let bias = if signed { 1i64 << (precision - 1) } else { 0 };
                                let value = (i64::from(sample as i32) + bias) as i32;
                                let value = i64::from(value);
                                let byte = if precision < 8 {
                                    (value << (8 - precision)) as u8
                                } else if precision == 8 {
                                    value as u8
                                } else {
                                    let shift = precision - 8;
                                    ((value >> shift) + ((value >> (shift - 1)) % 2)).clamp(0, 255)
                                        as u8
                                };
                                expected
                                    [index / width * pitch + index % width * channels + channel] =
                                    byte;
                            }
                        }
                        assert_eq!(actual, expected, "width={width} channels={channels} precision={precision} signed={signed}");
                    }
                }
            }
        }
    }

    // One empty packet per component. Empty coefficients reconstruct to the
    // unsigned DC bias, making component traversal independent of tier-1 data.
    pub(crate) fn empty_packet_codestream(components: u16, layers: u16) -> Vec<u8> {
        let mut source = vec![255, 79, 255, 81];
        source.extend_from_slice(&(38 + 3 * components).to_be_bytes());
        source.extend_from_slice(&[0, 0]);
        for field in [1u32, 1, 0, 0, 1, 1, 0, 0] {
            source.extend_from_slice(&field.to_be_bytes());
        }
        source.extend_from_slice(&components.to_be_bytes());
        for _ in 0..components {
            source.extend_from_slice(&[7, 1, 1]);
        }
        source.extend_from_slice(&[255, 82, 0, 12, 0, 0]);
        source.extend_from_slice(&layers.to_be_bytes());
        source.extend_from_slice(&[0, 0, 4, 4, 0, 1]);
        source.extend_from_slice(&[255, 92, 0, 4, 64, 64]);
        source.extend_from_slice(&[255, 144, 0, 10, 0, 0]);
        let packets = usize::from(components) * usize::from(layers);
        source.extend_from_slice(&(14 + packets as u32).to_be_bytes());
        source.extend_from_slice(&[0, 1, 255, 147]);
        source.resize(source.len() + packets, 0);
        source.extend_from_slice(&[255, 217]);
        source
    }

    // Three resolutions, a 4x4 tile and enough empty headers for all packets.
    fn empty_resolution_codestream(components: u16, pocs: &[u8]) -> Vec<u8> {
        let mut source = empty_packet_codestream(components, 3);
        for offset in [8, 12, 24, 28] {
            source[offset..offset + 4].copy_from_slice(&4u32.to_be_bytes());
        }
        let cod = source.windows(2).position(|m| m == [255, 82]).unwrap();
        source[cod + 6..cod + 8].copy_from_slice(&1u16.to_be_bytes());
        source[cod + 9] = 2;
        let qcd = source.windows(2).position(|m| m == [255, 92]).unwrap();
        source.splice(qcd..qcd + 6, marker(92, &[64; 8]));
        if !pocs.is_empty() {
            let sot = source.windows(2).position(|m| m == [255, 144]).unwrap();
            source.splice(sot..sot, marker(95, pocs));
        }
        source
    }

    #[test]
    fn jpx_partial_poc_uses_actual_resolution_and_minimum_stride() {
        for end in [1, 2, 3] {
            let source = empty_resolution_codestream(1, &[0, 0, 0, 1, end, 1, 0]);
            let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
            let side = 1usize << (end - 1);
            for y in 0..4 {
                for x in 0..4 {
                    assert_eq!(
                        decoder.components[0].samples[y * 4 + x],
                        if x < side && y < side { 128.0 } else { 0.0 }
                    );
                }
            }
        }
        // First-pass flags reset for each POC: a skipped packet in a later
        // range supplies the highest requested resolution, even after res0.
        let source = empty_resolution_codestream(1, &[0, 0, 0, 1, 1, 1, 0, 2, 0, 0, 1, 3, 1, 0]);
        let decoder = super::CjpxDecoder::create(&source, false, 1).unwrap();
        assert_eq!(decoder.components[0].samples, [128.0; 4]);
    }

    #[test]
    fn jpx_partial_poc_tiles_copy_from_actual_global_coordinates() {
        let mut source = empty_resolution_codestream(1, &[0, 0, 0, 1, 1, 1, 0]);
        source[8..12].copy_from_slice(&12u32.to_be_bytes());
        let sot = source.windows(2).position(|m| m == [255, 144]).unwrap();
        let part = source[sot..source.len() - 2].to_vec();
        source.truncate(source.len() - 2);
        for index in 1..3u16 {
            let start = source.len();
            source.extend_from_slice(&part);
            source[start + 4..start + 6].copy_from_slice(&index.to_be_bytes());
        }
        source.extend_from_slice(&[255, 217]);
        let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
        assert_eq!(&decoder.components[0].samples[..3], &[128.0; 3]);
        assert!(decoder.components[0].samples[3..].iter().all(|&v| v == 0.0));

        // One clipped tile uses update_image_data, rather than the direct
        // single-tile transfer. Intersect the actual resolution with x0=1.
        let mut source = empty_resolution_codestream(1, &[0, 0, 0, 1, 1, 1, 0]);
        source[16..20].copy_from_slice(&1u32.to_be_bytes());
        let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
        assert!(decoder.components[0].samples.iter().all(|&v| v == 0.0));
        source[8..12].copy_from_slice(&5u32.to_be_bytes());
        source[24..28].copy_from_slice(&8u32.to_be_bytes());
        let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
        assert_eq!(decoder.components[0].samples[0], 128.0);
        assert!(decoder.components[0].samples[1..].iter().all(|&v| v == 0.0));
    }

    #[test]
    fn jpx_native_mct_checks_sample_counts_and_actual_resolutions() {
        let mut source =
            empty_resolution_codestream(3, &[0, 0, 0, 1, 1, 1, 0, 0, 1, 0, 1, 2, 3, 0]);
        let cod = source.windows(2).position(|m| m == [255, 82]).unwrap();
        source[cod + 8] = 1;
        // The first component visited res0; the others visited res1.
        assert!(super::CjpxDecoder::create(&source, false, 0).is_none());

        let mut source = empty_resolution_codestream(3, &[]);
        let cod = source.windows(2).position(|m| m == [255, 82]).unwrap();
        source[cod + 8] = 1;
        // Equal sample counts are sufficient even with different widths.
        source[43..45].copy_from_slice(&[1, 2]);
        source[46..48].copy_from_slice(&[2, 1]);
        source[49..51].copy_from_slice(&[1, 2]);
        let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
        assert!(decoder.components.iter().all(|c| c.samples == [128.0; 8]));
    }

    #[test]
    fn jpx_siz_retains_native_tile_count_and_saturating_bounds() {
        for count in [65535u32, 65536] {
            let mut source = empty_packet_codestream(1, 1);
            source[8..12].copy_from_slice(&count.to_be_bytes());
            assert_eq!(
                hayro_jpeg2000::pdfium::Codestream::new(&source, 0).is_ok(),
                count == 65535
            );
        }
        // A very large tile still clips to a one-pixel image. Native SIZ
        // validation saturates offset+size instead of rejecting its overflow.
        let mut source = empty_packet_codestream(1, 1);
        source[8..12].copy_from_slice(&11u32.to_be_bytes());
        source[16..20].copy_from_slice(&10u32.to_be_bytes());
        source[24..28].copy_from_slice(&u32::MAX.to_be_bytes());
        source[32..36].copy_from_slice(&9u32.to_be_bytes());
        let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
        assert_eq!(decoder.components[0].samples, [128.0]);
    }

    #[test]
    fn jpx_layer_count_is_independent_of_sample_precision() {
        for layers in [32, 33, 256, 65535] {
            let source = empty_packet_codestream(1, layers);
            let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
            assert_eq!(decoder.components[0].samples, [128.0]);
        }
        let mut source = empty_packet_codestream(1, 1);
        source[51..53].copy_from_slice(&0u16.to_be_bytes());
        assert!(super::CjpxDecoder::create(&source, false, 0).is_none());
    }

    #[test]
    fn jpx_poc_markers_append_ranges_in_main_and_tile_headers() {
        let fixture = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rgb.j2c"
        ));
        let expected = super::CjpxDecoder::create(fixture, false, 0).unwrap();
        let sot = fixture
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        for tile_header in [false, true] {
            for order in 0..3 {
                // This fixture has one layer/precinct per resolution. LRCP,
                // RLCP and RPCL therefore visit the same nonempty packets.
                let mut marker = vec![255, 95, 0, 9, 0, 0, 0, 1, 5, 4, order];
                let mut source = fixture.to_vec();
                // A repeated range must share the packet inclusion table.
                marker.extend_from_within(..);
                let insertion = sot + if tile_header { 12 } else { 0 };
                if tile_header {
                    let length = u32::from_be_bytes(source[sot + 6..sot + 10].try_into().unwrap());
                    source[sot + 6..sot + 10]
                        .copy_from_slice(&(length + marker.len() as u32).to_be_bytes());
                }
                source.splice(insertion..insertion, marker);
                let actual = super::CjpxDecoder::create(&source, false, 0).unwrap();
                for (actual, expected) in actual.components.iter().zip(&expected.components) {
                    assert_eq!(actual.samples, expected.samples);
                }
            }
        }
        for count in [31, 32] {
            let mut source = empty_packet_codestream(1, 1);
            let sot = source
                .windows(2)
                .position(|marker| marker == [255, 144])
                .unwrap();
            let mut marker = vec![255, 95];
            marker.extend_from_slice(&(2 + count * 7u16).to_be_bytes());
            for _ in 0..count {
                marker.extend_from_slice(&[0, 0, 0, 1, 1, 1, 0]);
            }
            source.splice(sot..sot, marker);
            assert_eq!(
                super::CjpxDecoder::create(&source, false, 0).is_some(),
                count == 31
            );
        }
        // More than 256 components switches both POC component fields to u16.
        let mut source = empty_packet_codestream(257, 1);
        let sot = source
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        source.splice(sot..sot, [255, 95, 0, 11, 0, 0, 0, 0, 1, 1, 1, 1, 4]);
        assert!(super::CjpxDecoder::create(&source, false, 0).is_some());
    }

    #[test]
    fn jpx_rgn_shifts_precede_dequantization_and_tile_overrides() {
        let fixture = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rgb.j2c"
        ));
        let expected = super::CjpxDecoder::create(fixture, false, 0).unwrap();
        let sot = fixture
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        for shift in [1, 2, 3] {
            let mut source = fixture.to_vec();
            // Srgn is consumed without validation by pinned OpenJPEG.
            source.splice(sot..sot, [255, 94, 0, 5, 0, 99, shift]);
            let actual = super::CjpxDecoder::create(&source, false, 0).unwrap();
            for (actual, expected) in actual.components.iter().zip(&expected.components) {
                assert_eq!(actual.samples, expected.samples, "shift {shift}");
            }
        }
        // Tile RGN replaces the inherited main-header value, in source order.
        let mut source = fixture.to_vec();
        let length = u32::from_be_bytes(source[sot + 6..sot + 10].try_into().unwrap());
        source[sot + 6..sot + 10].copy_from_slice(&(length + 7).to_be_bytes());
        source.splice(sot + 12..sot + 12, [255, 94, 0, 5, 0, 0, 0]);
        source.splice(sot..sot, [255, 94, 0, 5, 0, 0, 255]);
        assert!(super::CjpxDecoder::create(&source, false, 0).is_some());
        for (length, component, shift, succeeds) in [
            (5, 0, 30, true),
            (5, 0, 31, false),
            (5, 1, 0, false),
            (6, 0, 0, false),
        ] {
            let mut source = empty_packet_codestream(1, 1);
            let sot = source
                .windows(2)
                .position(|marker| marker == [255, 144])
                .unwrap();
            source.splice(sot..sot, [255, 94, 0, length, component, 0, shift]);
            assert_eq!(
                super::CjpxDecoder::create(&source, false, 0).is_some(),
                succeeds
            );
        }
    }

    fn packed_marker(kind: u8, index: u8, data: &[u8]) -> Vec<u8> {
        let mut marker = vec![255, kind];
        marker.extend_from_slice(&(3 + data.len() as u16).to_be_bytes());
        marker.push(index);
        marker.extend_from_slice(data);
        marker
    }

    #[test]
    fn jpx_cod_coc_lengths_flags_and_overrides_follow_native_order() {
        let source = empty_packet_codestream(1, 1);
        let cod = source
            .windows(2)
            .position(|marker| marker == [255, 82])
            .unwrap();
        for (offset, value) in [(3, 11), (3, 13), (4, 8), (8, 2), (12, 128)] {
            let mut invalid = source.clone();
            invalid[cod + offset] = value;
            assert!(super::CjpxDecoder::create(&invalid, false, 0).is_none());
        }
        let mut unknown_order = source.clone();
        unknown_order[cod + 5] = 255;
        assert!(super::CjpxDecoder::create(&unknown_order, false, 0).is_none());
        let sot = source
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        unknown_order.splice(sot..sot, [255, 95, 0, 9, 0, 0, 0, 1, 1, 1, 0]);
        assert!(super::CjpxDecoder::create(&unknown_order, false, 0).is_some());
        // COC's raw reserved EPH bit does not enable TCP packet-header EPH.
        for tile in [false, true] {
            let mut encoded = source.clone();
            if tile {
                encoded[sot + 6..sot + 10].copy_from_slice(&26u32.to_be_bytes());
            }
            let insertion = if tile { sot + 12 } else { sot };
            encoded.splice(insertion..insertion, [255, 83, 0, 9, 0, 4, 0, 4, 4, 0, 1]);
            assert!(super::CjpxDecoder::create(&encoded, false, 0).is_some());
        }
        // A later COD replaces all earlier component coding parameters and
        // clears prior TCP SOP/EPH flags, including an intervening COC.
        let mut encoded = source.clone();
        encoded[cod + 4] = 4;
        let mut overrides = vec![255, 83, 0, 9, 0, 0, 1, 4, 4, 0, 1];
        overrides.extend_from_slice(&source[cod..cod + 14]);
        encoded.splice(sot..sot, overrides);
        assert!(super::CjpxDecoder::create(&encoded, false, 0).is_some());
    }

    #[test]
    fn jpx_ht_blocks_use_native_cleanup_refinement_and_failure_paths() {
        fn encoded(passes: u8, second_length: u8) -> Vec<u8> {
            let mut source = empty_packet_codestream(1, 1);
            source[8..12].copy_from_slice(&2u32.to_be_bytes());
            source[24..28].copy_from_slice(&2u32.to_be_bytes());
            let cod = source
                .windows(2)
                .position(|marker| marker == [255, 82])
                .unwrap();
            source[cod + 12] = 64;
            let qcd = source
                .windows(2)
                .position(|marker| marker == [255, 92])
                .unwrap();
            source[qcd + 5] = 48; // Exponent six + two guard bits => Mb seven.
            let sot = source
                .windows(2)
                .position(|marker| marker == [255, 144])
                .unwrap();
            let mut bits = String::from("111"); // Present/inclusion/zero-MSB trees.
            bits.push_str(match passes {
                1 => "0",
                2 => "10",
                3 => "1100",
                5 => "1110",
                _ => unreachable!(),
            });
            bits.push('0'); // No Lblock increment.
            bits.push_str("010"); // Two cleanup bytes in segment zero.
            if passes > 1 {
                let width = 3 + (passes - 1).ilog2() as usize;
                bits.push_str(&format!("{:0width$b}", second_length));
            }
            while bits.len() % 8 != 0 {
                bits.push('0');
            }
            let mut body: Vec<u8> = bits
                .as_bytes()
                .chunks(8)
                .map(|bits| {
                    bits.iter()
                        .fold(0, |value, &bit| (value << 1) | u8::from(bit == b'1'))
                })
                .collect();
            body.extend_from_slice(&[0x22, 0]);
            if passes > 1 {
                body.extend_from_slice(&[0x5a, 0xa5][..usize::from(second_length)]);
            }
            source.truncate(sot + 14);
            source[sot + 6..sot + 10].copy_from_slice(&(14 + body.len() as u32).to_be_bytes());
            source.extend_from_slice(&body);
            source.extend_from_slice(&[255, 217]);
            // Native accepts and ignores CAP/CPF payloads.
            source.splice(sot..sot, [255, 80, 0, 4, 255, 0, 255, 89, 0, 3, 255]);
            source
        }
        for (passes, second, expected) in [(1, 0, 32.0), (2, 2, 32.0), (3, 2, 16.0), (5, 0, 32.0)] {
            let source = encoded(passes, second);
            let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
            assert_eq!(
                decoder.components[0].samples,
                [128.0, expected],
                "passes {passes}"
            );
        }
        for style in [2, 5] {
            let mut source = encoded(1, 0);
            let qcd = source
                .windows(2)
                .position(|marker| marker == [255, 92])
                .unwrap();
            source.splice(qcd..qcd + 6, marker(92, &[64 | style, 48, 0]));
            // Reversible reconstruction ignores the signaled quantization
            // step, including native's accepted additional style values.
            assert_eq!(
                super::CjpxDecoder::create(&source, false, 0)
                    .unwrap()
                    .components[0]
                    .samples,
                [128.0, 32.0]
            );
        }
        let mut source = encoded(1, 0);
        let cod = source
            .windows(2)
            .position(|marker| marker == [255, 82])
            .unwrap();
        source[cod + 13] = 0;
        assert_eq!(
            super::CjpxDecoder::create(&source, false, 0)
                .unwrap()
                .components[0]
                .samples,
            [128.0, 0.0]
        );
        let mut source = encoded(1, 0);
        let sot = source
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        let mut overrides = marker(93, &[0, 64, 56]);
        overrides.extend_from_slice(&marker(92, &[64, 48]));
        source.splice(sot..sot, overrides);
        assert_eq!(
            super::CjpxDecoder::create(&source, false, 0)
                .unwrap()
                .components[0]
                .samples,
            [128.0, 32.0]
        );
        assert!(super::CjpxDecoder::create(&encoded(5, 2), false, 0).is_none());
        let mut source = encoded(1, 0);
        let sot = source
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        source.splice(sot..sot, [255, 94, 0, 5, 0, 0, 1]); // Native HT rejects ROI.
        assert!(super::CjpxDecoder::create(&source, false, 0).is_none());
    }

    fn marker(kind: u8, payload: &[u8]) -> Vec<u8> {
        let mut marker = vec![255, kind];
        marker.extend_from_slice(&(2 + payload.len() as u16).to_be_bytes());
        marker.extend_from_slice(payload);
        marker
    }

    #[test]
    fn jpx_cbd_updates_precision_and_signedness_without_replacing_siz_dc_shift() {
        for (definition, sample, precision, signed) in [(3, 15.0, 4, false), (135, 127.0, 8, true)]
        {
            let mut source = empty_packet_codestream(1, 1);
            let sot = source
                .windows(2)
                .position(|marker| marker == [255, 144])
                .unwrap();
            source.splice(sot..sot, marker(120, &[0, 1, definition]));
            let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
            assert_eq!(decoder.components[0].samples, [sample]);
            assert_eq!(decoder.components[0].precision, precision);
            assert_eq!(decoder.components[0].signed, signed);
        }
        for payload in [&[0, 1, 31][..], &[0, 2, 7], &[0, 1]] {
            let mut source = empty_packet_codestream(1, 1);
            let sot = source
                .windows(2)
                .position(|marker| marker == [255, 144])
                .unwrap();
            source.splice(sot..sot, marker(120, payload));
            assert!(super::CjpxDecoder::create(&source, false, 0).is_none());
        }
    }

    #[test]
    fn jpx_missing_empty_packet_headers_use_native_bio_zero_fill() {
        let mut source = empty_packet_codestream(3, 2);
        let sot = source
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        source.truncate(sot + 15); // One real empty packet; remaining headers at EOF.
        source[sot + 6..sot + 10].copy_from_slice(&15u32.to_be_bytes());
        source.extend_from_slice(&[255, 217]);
        let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
        assert!(decoder
            .components
            .iter()
            .all(|comp| comp.samples == [128.0]));
        let cod = source
            .windows(2)
            .position(|marker| marker == [255, 82])
            .unwrap();
        source[cod + 4] = 4; // EPH still requires two real marker bytes.
        assert!(super::CjpxDecoder::create(&source, false, 0).is_none());
    }

    #[test]
    fn jpx_unknown_main_markers_resynchronize_in_pairs_with_native_states() {
        let source = empty_packet_codestream(1, 1);
        let cod = source
            .windows(2)
            .position(|marker| marker == [255, 82])
            .unwrap();
        let sot = source
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        for (position, bytes, succeeds) in [
            (2, &[255, 101, 1, 2][..], true),
            (2, &[255, 101, 255, 102], false), // Unknown recovery disallowed before SIZ.
            (cod, &[255, 101, 0, 6, 1, 2], true),
            (cod, &[255, 101, 255, 81], false), // SIZ is not legal in main-header state.
            (sot, &[255, 101, 0], false),       // A byte-wise scan would incorrectly recover SOT.
        ] {
            let mut encoded = source.clone();
            encoded.splice(position..position, bytes.iter().copied());
            assert_eq!(
                super::CjpxDecoder::create(&encoded, false, 0).is_some(),
                succeeds
            );
        }
        let mut encoded = source.clone();
        encoded[sot + 6..sot + 10].copy_from_slice(&19u32.to_be_bytes());
        encoded.splice(sot + 12..sot + 12, [255, 48, 0, 2]);
        assert!(super::CjpxDecoder::create(&encoded, false, 0).is_none());
    }

    #[test]
    fn jpx_ignored_markers_retain_native_size_and_packet_length_checks() {
        let source = empty_packet_codestream(1, 1);
        let sot = source
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        for (kind, payload, succeeds) in [
            (85, &[0][..], false),
            (85, &[0, 48], true),
            (87, &[], false),
            (87, &[0], true),
            (99, &[0, 0, 0], false),
            (99, &[0, 0, 0, 0], true),
            (100, &[], true),
        ] {
            let mut encoded = source.clone();
            encoded.splice(sot..sot, marker(kind, payload));
            assert_eq!(
                super::CjpxDecoder::create(&encoded, false, 0).is_some(),
                succeeds
            );
        }
        for (payload, succeeds) in [
            (&[][..], false),
            (&[0], true),
            (&[0, 129], false),
            (&[0, 128], true),
            (&[0, 129, 0], true),
        ] {
            let mut encoded = source.clone();
            let marker = marker(88, payload);
            encoded[sot + 6..sot + 10].copy_from_slice(&(15 + marker.len() as u32).to_be_bytes());
            encoded.splice(sot + 12..sot + 12, marker);
            assert_eq!(
                super::CjpxDecoder::create(&encoded, false, 0).is_some(),
                succeeds
            );
        }
    }

    #[test]
    fn jpx_mq_pass_counts_cross_segments_and_layers_then_stop_at_available_planes() {
        fn packet(first: bool, passes: u32, groups: &[u32]) -> Vec<u8> {
            let mut bits = String::from(if first { "111" } else { "11" });
            if passes >= 37 {
                bits.push_str(&format!("111111111{:07b}", passes - 37));
            } else {
                bits.push_str(&format!("1111{:05b}", passes - 6));
            }
            bits.push('0');
            for &group in groups {
                let width = 3 + group.ilog2() as usize;
                bits.push_str(&format!("{:0width$b}", 1));
            }
            let mut encoded = Vec::new();
            let mut byte = 0u8;
            let mut used = 0;
            let mut capacity = 8;
            for bit in bits.bytes() {
                byte = (byte << 1) | u8::from(bit == b'1');
                used += 1;
                if used == capacity {
                    encoded.push(byte);
                    capacity = if byte == 255 { 7 } else { 8 };
                    byte = 0;
                    used = 0;
                }
            }
            if used != 0 {
                encoded.push(byte << (capacity - used));
            } else if encoded.last() == Some(&255) {
                encoded.push(0);
            }
            encoded.resize(encoded.len() + groups.len(), 0); // One byte per segment contribution.
            encoded
        }
        fn source(packets: &[Vec<u8>]) -> Vec<u8> {
            let mut source = empty_packet_codestream(1, packets.len() as u16);
            let sot = source
                .windows(2)
                .position(|marker| marker == [255, 144])
                .unwrap();
            source.truncate(sot + 14);
            let bytes: usize = packets.iter().map(Vec::len).sum();
            source[sot + 6..sot + 10].copy_from_slice(&(14 + bytes as u32).to_be_bytes());
            for packet in packets {
                source.extend_from_slice(packet);
            }
            source.extend_from_slice(&[255, 217]);
            source
        }
        // Mb nine has 25 available passes. Native consumes larger packet pass
        // counts, groups ordinary MQ segments at 109, and decodes only 25.
        let reference = source(&[packet(true, 25, &[25])]);
        let expected = super::CjpxDecoder::create(&reference, false, 0).unwrap();
        for packets in [
            vec![packet(true, 164, &[109, 55])],
            vec![
                packet(true, 164, &[109, 55]),
                packet(false, 164, &[54, 109, 1]),
            ],
        ] {
            let encoded = source(&packets);
            let actual = super::CjpxDecoder::create(&encoded, false, 0).unwrap();
            assert_eq!(actual.components[0].samples, expected.components[0].samples);
        }
        // A never-included block uses numbps zero, even when the fixed native
        // quantization state has a negative band Mb from an empty QCD prefix.
        let mut empty = empty_packet_codestream(1, 1);
        let qcd = empty
            .windows(2)
            .position(|marker| marker == [255, 92])
            .unwrap();
        empty.splice(qcd..qcd + 6, marker(92, &[0]));
        assert_eq!(
            super::CjpxDecoder::create(&empty, false, 0)
                .unwrap()
                .components[0]
                .samples,
            [128.0]
        );
    }

    #[test]
    fn jpx_reduction_is_checked_at_each_coding_marker_before_later_overrides() {
        let fixture = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rgb.j2c"
        ));
        let cod = fixture
            .windows(2)
            .position(|marker| marker == [255, 82])
            .unwrap();
        let sot = fixture
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        let original = &fixture[cod..cod + 14];
        let mut shallow = original.to_vec();
        shallow[9] = 0;
        let coc = vec![255, 83, 0, 9, 0, 0, 0, 4, 4, 0, 1];
        assert!(super::CjpxDecoder::create(fixture, false, 1).is_some());
        for tile in [false, true] {
            for shallow in [&shallow, &coc] {
                let mut markers = shallow.clone();
                markers.extend_from_slice(original);
                let mut source = fixture.to_vec();
                if tile {
                    let length = u32::from_be_bytes(source[sot + 6..sot + 10].try_into().unwrap());
                    source[sot + 6..sot + 10]
                        .copy_from_slice(&(length + markers.len() as u32).to_be_bytes());
                }
                let position = if tile { sot + 12 } else { sot };
                source.splice(position..position, markers);
                assert!(super::CjpxDecoder::create(&source, false, 0).is_some());
                assert!(super::CjpxDecoder::create(&source, false, 1).is_none());
            }
        }
    }

    #[test]
    fn jpx_mct_mcc_mco_offsets_preserve_native_element_casts_and_reset_order() {
        for (element, data, sample) in [
            (0u16, vec![255, 255], 255.0),
            (1, (-2i32).to_be_bytes().to_vec(), 0.0),
            (2, 12.75f32.to_be_bytes().to_vec(), 12.0),
            (3, 19.9f64.to_be_bytes().to_vec(), 19.0),
        ] {
            let mut source = empty_packet_codestream(1, 1);
            let sot = source
                .windows(2)
                .position(|marker| marker == [255, 144])
                .unwrap();
            let mut payload = vec![0, 0];
            payload.extend_from_slice(&((element << 10) | (2 << 8) | 1).to_be_bytes());
            payload.extend_from_slice(&[0, 0]);
            payload.extend_from_slice(&data);
            let mut markers = marker(116, &payload);
            markers.extend_from_slice(&marker(
                117,
                &[0, 0, 3, 0, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0],
            ));
            markers.extend_from_slice(&marker(119, &[1, 3]));
            source.splice(sot..sot, markers);
            let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
            assert_eq!(decoder.components[0].samples, [sample]);
        }
        for (payload, sample) in [(&[0][..], 0.0), (&[1, 99], 0.0), (&[2], 128.0)] {
            let mut source = empty_packet_codestream(1, 1);
            let sot = source
                .windows(2)
                .position(|marker| marker == [255, 144])
                .unwrap();
            source.splice(sot..sot, marker(119, payload));
            assert_eq!(
                super::CjpxDecoder::create(&source, false, 0)
                    .unwrap()
                    .components[0]
                    .samples,
                [sample]
            );
        }
        let mut source = empty_packet_codestream(1, 1);
        let sot = source
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        source[sot + 6..sot + 10].copy_from_slice(&20u32.to_be_bytes());
        source.splice(sot + 12..sot + 12, marker(119, &[0]));
        assert_eq!(
            super::CjpxDecoder::create(&source, false, 0)
                .unwrap()
                .components[0]
                .samples,
            [0.0]
        );
    }

    #[test]
    fn jpx_packed_headers_preserve_fragment_order_and_eph_boundaries() {
        // Six empty headers with EPH. Keep a body byte so native SOD owns data.
        let mut source = empty_packet_codestream(3, 2);
        let cod = source
            .windows(2)
            .position(|marker| marker == [255, 82])
            .unwrap();
        source[cod + 4] = 4;
        let sot = source
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        source.truncate(sot + 14);
        source.extend_from_slice(&[0, 255, 217]);
        source[sot + 6..sot + 10].copy_from_slice(&15u32.to_be_bytes());
        let headers = [0, 255, 146].repeat(6);
        for ppm in [false, true] {
            let mut fragments = if ppm {
                (headers.len() as u32).to_be_bytes().to_vec()
            } else {
                Vec::new()
            };
            fragments.extend_from_slice(&headers);
            let cut = if ppm { 6 } else { 2 }; // Split an EPH marker after FF.
            let mut markers = packed_marker(if ppm { 96 } else { 97 }, 7, &fragments[cut..]);
            markers.extend_from_slice(&packed_marker(
                if ppm { 96 } else { 97 },
                2,
                &fragments[..cut],
            ));
            let mut packed = source.clone();
            if !ppm {
                packed[sot + 6..sot + 10]
                    .copy_from_slice(&(15 + markers.len() as u32).to_be_bytes());
            }
            packed.splice(
                sot + if ppm { 0 } else { 12 }..sot + if ppm { 0 } else { 12 },
                markers,
            );
            let actual = super::CjpxDecoder::create(&packed, false, 0).unwrap();
            assert!(actual.components.iter().all(|comp| comp.samples == [128.0]));
        }
        // Duplicate sequence indices, split Nppm, and incomplete payload fail.
        for markers in [
            [
                packed_marker(96, 0, &[0, 0, 0, 1, 0]),
                packed_marker(96, 0, &[0]),
            ]
            .concat(),
            [
                packed_marker(96, 0, &[0, 0]),
                packed_marker(96, 1, &[0, 1, 0]),
            ]
            .concat(),
            packed_marker(96, 0, &[0, 0, 0, 2, 0]),
        ] {
            let mut invalid = source.clone();
            invalid.splice(sot..sot, markers);
            assert!(super::CjpxDecoder::create(&invalid, false, 0).is_none());
        }
    }

    #[test]
    fn jpx_ppm_cursor_follows_tile_decode_order() {
        let mut source = empty_packet_codestream(1, 1);
        source[8..12].copy_from_slice(&2u32.to_be_bytes());
        let sot = source
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        let cod = source
            .windows(2)
            .position(|marker| marker == [255, 82])
            .unwrap();
        let mut coding_style = source[cod..cod + 14].to_vec();
        coding_style[4] = 4; // Only tile one consumes EPH in the global headers.
        let mut tile_one = source[sot..source.len() - 2].to_vec();
        tile_one[4..6].copy_from_slice(&1u16.to_be_bytes());
        tile_one[6..10].copy_from_slice(&29u32.to_be_bytes());
        tile_one.splice(12..12, coding_style);
        let tile_zero = source[sot..source.len() - 2].to_vec();
        source.truncate(sot);
        source.extend_from_slice(&packed_marker(96, 0, &[0, 0, 0, 4, 0, 255, 146, 0]));
        source.extend_from_slice(&tile_one);
        source.extend_from_slice(&tile_zero);
        source.extend_from_slice(&[255, 217]);
        let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
        assert_eq!(decoder.components[0].samples, [128.0, 128.0]);
    }

    #[test]
    fn jpx_packet_header_and_body_can_cross_tile_part_boundaries() {
        let fixture = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rgb.j2c"
        ));
        let sot = fixture
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        let expected = super::CjpxDecoder::create(fixture, false, 0).unwrap();
        for bytes in [1, 2, 20] {
            let mut source = fixture[..sot + 14 + bytes].to_vec();
            source[sot + 6..sot + 10].copy_from_slice(&(14 + bytes as u32).to_be_bytes());
            source[sot + 11] = 2;
            let mut second = fixture[sot..sot + 14].to_vec();
            let remaining = &fixture[sot + 14 + bytes..fixture.len() - 2];
            second[6..10].copy_from_slice(&(14 + remaining.len() as u32).to_be_bytes());
            second[10] = 1;
            second[11] = 2;
            source.extend_from_slice(&second);
            source.extend_from_slice(remaining);
            source.extend_from_slice(&[255, 217]);
            let actual = super::CjpxDecoder::create(&source, false, 0).unwrap();
            for (actual, expected) in actual.components.iter().zip(&expected.components) {
                assert_eq!(actual.samples, expected.samples);
            }
        }
    }

    #[test]
    fn jpx_component_indices_preserve_native_siz_range() {
        use crate::fxge::dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat};
        for components in [255, 256, 257] {
            let mut source = empty_packet_codestream(components, 1);
            let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
            assert_eq!(decoder.components.len(), usize::from(components));
            let mut bitmap =
                CfxDibitmap::create(i32::from(components), 1, FxdibFormat::EightBppRgb, 0).unwrap();
            assert!(decoder.decode(&mut bitmap, false, u32::from(components)));
            assert!(bitmap.scanline(0).unwrap()[..usize::from(components)]
                .iter()
                .all(|&sample| sample == 128));
            let length = 37 + 3 * components;
            source[4..6].copy_from_slice(&length.to_be_bytes());
            assert!(super::CjpxDecoder::create(&source, false, 0).is_none());
        }
    }

    #[test]
    fn jpx_native_tile_completion_preserves_next_marker_and_eof_order() {
        let fixture = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rgb.j2c"
        ));
        let sot = fixture
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        for count in [0, 1, 2] {
            for (suffix, succeeds) in [
                (&[][..], count == 0),
                (&[255, 217][..], true),
                (&[1, 2][..], true),
                (&[255, 144][..], true),
                (&[255, 144, 0][..], count == 1),
                (&[1, 2, 3][..], false),
            ] {
                let mut source = fixture[..fixture.len() - 2].to_vec();
                source[sot + 11] = count;
                source.extend_from_slice(suffix);
                assert_eq!(
                    super::CjpxDecoder::create(&source, false, 0).is_some(),
                    succeeds,
                    "count {count}, suffix {suffix:?}"
                );
            }
        }
    }

    #[test]
    fn jpx_native_tile_count_repair_uses_one_lookahead_before_decoding() {
        let fixture = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rgb.j2c"
        ));
        let sot = fixture
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        let mut source = fixture[..sot].to_vec();
        for part in 0..2 {
            let mut empty = fixture[sot..sot + 12].to_vec();
            empty[6..10].copy_from_slice(&12u32.to_be_bytes());
            empty[10] = part;
            empty[11] = 2;
            source.extend_from_slice(&empty);
            source.extend_from_slice(&[255, 147]);
        }
        let mut final_part = fixture[sot..].to_vec();
        final_part[10] = 2;
        final_part[11] = 2; // Nonconformant TPsot == TNsot triggers native correction.
        source.extend_from_slice(&final_part);
        assert!(super::CjpxDecoder::create(&source, false, 0).is_some());
        let last_count = source.len() - final_part.len() + 11;
        source[last_count] = 1;
        assert!(super::CjpxDecoder::create(&source, false, 0).is_none());
    }

    #[test]
    fn jpx_tile_mct_and_level_shift_stay_with_their_own_tile() {
        use crate::fxge::dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat};
        let fixture = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rgb.j2c"
        ));
        let sot = fixture
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        let tile_length =
            u32::from_be_bytes(fixture[sot + 6..sot + 10].try_into().unwrap()) as usize;
        let cod = fixture
            .windows(2)
            .position(|marker| marker == [255, 82])
            .unwrap();
        let cod_length = usize::from(u16::from_be_bytes(
            fixture[cod + 2..cod + 4].try_into().unwrap(),
        )) + 2;
        for second_tile in [false, true] {
            let mut source = fixture[..sot + tile_length].to_vec();
            source[8..12].copy_from_slice(&32u32.to_be_bytes()); // Two 16-wide tiles.
            if second_tile {
                let mut tile = fixture[sot..sot + tile_length].to_vec();
                tile[4..6].copy_from_slice(&1u16.to_be_bytes());
                let mut coding_style = fixture[cod..cod + cod_length].to_vec();
                assert_eq!(coding_style[8], 0); // Original tile has no MCT.
                assert_eq!(coding_style[13], 1); // Reversible 5/3 transform.
                coding_style[8] = 1;
                tile.splice(12..12, coding_style);
                let length = tile.len() as u32;
                tile[6..10].copy_from_slice(&length.to_be_bytes());
                source.extend_from_slice(&tile);
            }
            source.extend_from_slice(&[255, 217]);
            let decoder = super::CjpxDecoder::create(&source, false, 0).unwrap();
            let mut bitmap = CfxDibitmap::create(32, 16, FxdibFormat::Bgra, 0).unwrap();
            assert!(decoder.decode(&mut bitmap, true, 4));
            let row = bitmap.scanline(8).unwrap();
            assert_eq!(&row[..4], &[0, 0, 255, 255]);
            assert_eq!(
                &row[64..68],
                if second_tile {
                    &[192, 255, 192, 255]
                } else {
                    &[0; 4]
                }
            );
            if !second_tile {
                assert!(row[64..128].iter().all(|&sample| sample == 0));
            }
        }
    }

    #[test]
    fn jpx_empty_phr_tile_part_precedes_a_nonempty_part() {
        let fixture = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rgb.j2c"
        ));
        let sot = fixture
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        let mut source = fixture[..sot].to_vec();
        let mut empty = fixture[sot..sot + 12].to_vec();
        empty[6..10].copy_from_slice(&12u32.to_be_bytes());
        empty[11] = 2;
        source.extend_from_slice(&empty);
        source.extend_from_slice(&[255, 147]); // SOD is outside the twelve declared bytes.
        let mut remaining = fixture[sot..].to_vec();
        remaining[10] = 1;
        remaining[11] = 2;
        source.extend_from_slice(&remaining);
        assert!(super::CjpxDecoder::create(&source, false, 0).is_some());
    }

    #[test]
    fn jpx_sot_size_and_tile_part_index_use_native_validation() {
        let fixture = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rgb.j2c"
        ));
        let sot = fixture
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        for (length, index, succeeds) in [
            (10u16, 0u8, true),
            (9, 0, false),
            (10, 1, false),
            (10, 255, false),
        ] {
            let mut source = fixture.to_vec();
            source[sot + 2..sot + 4].copy_from_slice(&length.to_be_bytes());
            source[sot + 10] = index;
            assert_eq!(
                super::CjpxDecoder::create(&source, false, 0).is_some(),
                succeeds
            );
        }
    }

    #[test]
    fn jpx_zero_final_tile_length_reserves_native_eoc_bytes() {
        let fixture = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rgb.j2c"
        ));
        let mut source = fixture.to_vec();
        let sot = source
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        source[sot + 6..sot + 10].copy_from_slice(&0u32.to_be_bytes());
        assert!(super::CjpxDecoder::create(&source, false, 0).is_some());
        // Native packet iteration ignores unused earlier EOC/box bytes in the
        // body. Completion accepts the final pair at EOF; the box is not read
        // separately as a JP2 trailing box.
        source.extend_from_slice(&[0, 0, 0, 8, b'u', b'u', b'i', b'd']);
        assert!(super::CjpxDecoder::create(&source, false, 0).is_some());
    }

    #[test]
    fn jpx_raw_planes_retain_signed_codestream_values_and_component_metadata() {
        let unsigned = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rgb.j2c"
        ));
        let settings = hayro_jpeg2000::DecodeSettings {
            strict: true,
            ..Default::default()
        };
        let unsigned_image = hayro_jpeg2000::Image::new(unsigned, &settings).unwrap();
        let unsigned_planes = unsigned_image.decode_pdfium_components().unwrap();
        let mut signed = unsigned.to_vec();
        assert_eq!(&signed[2..4], &[255, 81]); // SIZ starts immediately after SOC.
        for component in 0..4 {
            signed[42 + 3 * component] |= 128;
        }
        let signed_image = hayro_jpeg2000::Image::new(&signed, &settings).unwrap();
        let signed_planes = signed_image.decode_pdfium_components().unwrap();
        assert_eq!(unsigned_planes.len(), 4);
        assert_eq!(signed_planes.len(), 4);
        for (unsigned, signed) in unsigned_planes.iter().zip(&signed_planes) {
            assert_eq!(
                (
                    signed.width,
                    signed.height,
                    signed.dx,
                    signed.dy,
                    signed.precision
                ),
                (16, 16, 1, 1, 16)
            );
            assert!(!unsigned.signed);
            assert!(signed.signed);
            assert_eq!(signed.samples.len(), 16 * 16);
            for (&unsigned, &signed) in unsigned.samples.iter().zip(&signed.samples) {
                assert_eq!(signed + 32768.0, unsigned);
                assert_eq!(signed, signed.round());
            }
        }
        use crate::fxge::dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat};
        let unsigned_decoder = super::CjpxDecoder::create(unsigned, false, 0).unwrap();
        let signed_decoder = super::CjpxDecoder::create(&signed, false, 0).unwrap();
        assert_eq!(unsigned_decoder.components.len(), 4);
        assert_eq!(signed_decoder.components.len(), 4);
        let mut unsigned_bitmap = CfxDibitmap::create(16, 16, FxdibFormat::Bgra, 0).unwrap();
        let mut signed_bitmap = CfxDibitmap::create(16, 16, FxdibFormat::Bgra, 0).unwrap();
        assert!(unsigned_decoder.decode(&mut unsigned_bitmap, true, 4));
        assert!(signed_decoder.decode(&mut signed_bitmap, true, 4));
        assert_eq!(unsigned_bitmap.buffer(), signed_bitmap.buffer());
    }
}

#[cfg(test)]
#[path = "resolution_hint_tests.rs"]
mod resolution_hint_tests;

#[cfg(test)]
pub(crate) use tests::empty_packet_codestream;
