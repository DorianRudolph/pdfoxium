// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fxcodec/image_predictors.h` and `.cpp`.

use super::data_and_bytes_consumed::DataAndBytesConsumed;
use crate::fxcrt::fx_2d_size::fx_2d_size_or_die;
use crate::fxge::calculate_pitch::calculate_pitch8;

#[cfg(feature = "simd")]
#[path = "png_sub_simd.rs"]
mod png_sub_simd;

pub const INVALID_OFFSET: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredictorType {
    None,
    Tiff,
    Png,
}

#[inline]
fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let bc = i32::from(b) - i32::from(c);
    let ac = i32::from(a) - i32::from(c);
    let pa = bc.abs();
    let pb = ac.abs();
    let pc = (ac + bc).abs();
    let bc_best = if pb <= pc { b } else { c };
    if pa <= pb && pa <= pc {
        a
    } else {
        bc_best
    }
}

fn predict_sub_rest<const BPP: usize>(dest: &mut [u8], src: &[u8]) {
    let row_size = dest.len();
    if row_size <= BPP {
        return;
    }
    let mut left = [0u8; BPP];
    left.copy_from_slice(&dest[..BPP]);
    for (s, d) in src
        .chunks_exact(BPP)
        .skip(1)
        .zip(dest.chunks_exact_mut(BPP).skip(1))
    {
        for c in 0..BPP {
            left[c] = left[c].wrapping_add(s[c]);
        }
        d.copy_from_slice(&left);
    }
    for i in row_size - row_size % BPP..row_size {
        dest[i] = src[i].wrapping_add(dest[i - BPP]);
    }
}

fn predict_sub_row(dest: &mut [u8], src: &[u8], bpp: usize, lead: usize) {
    #[cfg(feature = "simd")]
    if dest.len() >= 64 && (1..=4).contains(&bpp) {
        png_sub_simd::predict_sub_row(dest, src, bpp);
        return;
    }
    dest[..lead].copy_from_slice(&src[..lead]);
    match bpp {
        1 => predict_sub_rest::<1>(dest, src),
        2 => predict_sub_rest::<2>(dest, src),
        3 => predict_sub_rest::<3>(dest, src),
        4 => predict_sub_rest::<4>(dest, src),
        _ => {
            for i in lead..dest.len() {
                dest[i] = src[i].wrapping_add(dest[i - bpp]);
            }
        }
    }
}

fn predict_average_first_row_rest<const BPP: usize>(dest: &mut [u8], src: &[u8]) {
    let row_size = dest.len();
    if row_size <= BPP {
        return;
    }
    let mut left = [0u8; BPP];
    left.copy_from_slice(&dest[..BPP]);
    for (s, d) in src
        .chunks_exact(BPP)
        .skip(1)
        .zip(dest.chunks_exact_mut(BPP).skip(1))
    {
        for c in 0..BPP {
            left[c] = s[c].wrapping_add(left[c] / 2);
        }
        d.copy_from_slice(&left);
    }
    for i in row_size - row_size % BPP..row_size {
        dest[i] = src[i].wrapping_add(dest[i - BPP] / 2);
    }
}

fn predict_average_rest<const BPP: usize>(dest: &mut [u8], src: &[u8], last: &[u8]) {
    let row_size = dest.len();
    if row_size <= BPP {
        return;
    }
    let mut left = [0u8; BPP];
    left.copy_from_slice(&dest[..BPP]);
    for ((s, u), d) in src
        .chunks_exact(BPP)
        .skip(1)
        .zip(last.chunks_exact(BPP).skip(1))
        .zip(dest.chunks_exact_mut(BPP).skip(1))
    {
        for c in 0..BPP {
            left[c] = s[c].wrapping_add(((u16::from(u[c]) + u16::from(left[c])) / 2) as u8);
        }
        d.copy_from_slice(&left);
    }
    for i in row_size - row_size % BPP..row_size {
        let avg = (u16::from(last[i]) + u16::from(dest[i - BPP])) / 2;
        dest[i] = src[i].wrapping_add(avg as u8);
    }
}

fn predict_paeth_rest<const BPP: usize>(dest: &mut [u8], src: &[u8], last: &[u8]) {
    let row_size = dest.len();
    if row_size <= BPP {
        return;
    }
    let mut left = [0u8; BPP];
    left.copy_from_slice(&dest[..BPP]);
    let mut upper_left = [0u8; BPP];
    upper_left.copy_from_slice(&last[..BPP]);
    for ((s, u), d) in src
        .chunks_exact(BPP)
        .skip(1)
        .zip(last.chunks_exact(BPP).skip(1))
        .zip(dest.chunks_exact_mut(BPP).skip(1))
    {
        for c in 0..BPP {
            left[c] = s[c].wrapping_add(paeth_predictor(left[c], u[c], upper_left[c]));
        }
        upper_left.copy_from_slice(u);
        d.copy_from_slice(&left);
    }
    for i in row_size - row_size % BPP..row_size {
        dest[i] = src[i].wrapping_add(paeth_predictor(dest[i - BPP], last[i], last[i - BPP]));
    }
}

pub fn get_predictor(predictor: i32) -> PredictorType {
    if predictor >= 10 {
        PredictorType::Png
    } else if predictor == 2 {
        PredictorType::Tiff
    } else {
        PredictorType::None
    }
}

pub fn apply_predictor(
    mut decoded_buf: Vec<u8>,
    predictor: i32,
    colors: i32,
    bits_per_component: i32,
    columns: i32,
    bytes_consumed: u32,
) -> DataAndBytesConsumed {
    match get_predictor(predictor) {
        PredictorType::None => DataAndBytesConsumed::new(decoded_buf, bytes_consumed),
        PredictorType::Png => {
            match png_predictor(colors, bits_per_component, columns, &decoded_buf) {
                Some(result) => DataAndBytesConsumed::new(result, bytes_consumed),
                None => DataAndBytesConsumed::new(decoded_buf, INVALID_OFFSET),
            }
        }
        PredictorType::Tiff => {
            let valid = tiff_predictor(colors, bits_per_component, columns, &mut decoded_buf);
            DataAndBytesConsumed::new(
                decoded_buf,
                if valid {
                    bytes_consumed
                } else {
                    INVALID_OFFSET
                },
            )
        }
    }
}

fn png_predictor(
    colors: i32,
    bits_per_component: i32,
    columns: i32,
    src: &[u8],
) -> Option<Vec<u8>> {
    let row_size = calculate_pitch8(
        bits_per_component.try_into().ok()?,
        colors.try_into().ok()?,
        columns,
    )?;
    if row_size == 0 {
        return None;
    }
    let src_row_size = row_size.wrapping_add(1);
    if src_row_size == 0 {
        return None;
    }
    let row_count = src.len().checked_add(row_size as usize)? / src_row_size as usize;
    if row_count == 0 {
        return None;
    }
    let last_row_size = src.len() % src_row_size as usize;
    let mut dest_size = fx_2d_size_or_die(row_size, row_count);
    if last_row_size != 0 {
        dest_size -= src_row_size as usize - last_row_size;
    }
    let mut dest = vec![0u8; dest_size];
    let bpp_bits = colors.checked_mul(bits_per_component)?;
    let bpp = (bpp_bits / 8 + i32::from(bpp_bits % 8 > 0)) as u32;
    let mut src_pos = 0;
    let mut dest_pos = 0;
    let mut prev_start = 0;
    for row in 0..row_count {
        let remaining_row_size = (row_size as usize).min(src.len() - src_pos - 1);
        let (done, current) = dest.split_at_mut(dest_pos);
        let prev = if row == 0 {
            &[][..]
        } else {
            &done[prev_start..prev_start + remaining_row_size]
        };
        png_predict_line(current, &src[src_pos..], prev, remaining_row_size, bpp);
        src_pos += remaining_row_size + 1;
        prev_start = dest_pos;
        dest_pos += remaining_row_size;
    }
    Some(dest)
}

fn tiff_predictor(colors: i32, bits_per_component: i32, columns: i32, data: &mut [u8]) -> bool {
    let Ok(bpc) = u32::try_from(bits_per_component) else {
        return false;
    };
    let Ok(components) = u32::try_from(colors) else {
        return false;
    };
    let Some(row_size) = calculate_pitch8(bpc, components, columns) else {
        return false;
    };
    if row_size == 0 {
        return false;
    }
    for row in data.chunks_mut(row_size as usize) {
        tiff_predict_line(row, bits_per_component, colors, columns);
    }
    true
}

pub fn png_predict_line(
    dest: &mut [u8],
    src: &[u8],
    last: &[u8],
    row_size: usize,
    bytes_per_pixel: u32,
) {
    #[cfg(feature = "render-profile")]
    let _profile = crate::render_profile::Scope::new("PNG predictor");
    let tag = src[0];
    let src = &src[1..1 + row_size];
    let dest = &mut dest[..row_size];
    let last = if last.is_empty() {
        last
    } else {
        &last[..row_size]
    };
    let bpp = bytes_per_pixel as usize;
    let lead = bpp.min(row_size);
    match tag {
        1 => predict_sub_row(dest, src, bpp, lead),
        2 => {
            if last.is_empty() {
                dest.copy_from_slice(src);
            } else {
                for i in 0..row_size {
                    dest[i] = src[i].wrapping_add(last[i]);
                }
            }
        }
        3 => {
            if last.is_empty() {
                dest[..lead].copy_from_slice(&src[..lead]);
                match bpp {
                    1 => predict_average_first_row_rest::<1>(dest, src),
                    2 => predict_average_first_row_rest::<2>(dest, src),
                    3 => predict_average_first_row_rest::<3>(dest, src),
                    4 => predict_average_first_row_rest::<4>(dest, src),
                    _ => {
                        for i in lead..row_size {
                            dest[i] = src[i].wrapping_add(dest[i - bpp] / 2);
                        }
                    }
                }
            } else {
                for i in 0..lead {
                    dest[i] = src[i].wrapping_add(last[i] / 2);
                }
                match bpp {
                    1 => predict_average_rest::<1>(dest, src, last),
                    2 => predict_average_rest::<2>(dest, src, last),
                    3 => predict_average_rest::<3>(dest, src, last),
                    4 => predict_average_rest::<4>(dest, src, last),
                    _ => {
                        for i in lead..row_size {
                            let avg = (u16::from(last[i]) + u16::from(dest[i - bpp])) / 2;
                            dest[i] = src[i].wrapping_add(avg as u8);
                        }
                    }
                }
            }
        }
        4 => {
            if last.is_empty() {
                predict_sub_row(dest, src, bpp, lead);
            } else {
                for i in 0..lead {
                    dest[i] = src[i].wrapping_add(last[i]);
                }
                match bpp {
                    1 => predict_paeth_rest::<1>(dest, src, last),
                    2 => predict_paeth_rest::<2>(dest, src, last),
                    3 => predict_paeth_rest::<3>(dest, src, last),
                    4 => predict_paeth_rest::<4>(dest, src, last),
                    _ => {
                        for i in lead..row_size {
                            dest[i] = src[i].wrapping_add(paeth_predictor(
                                dest[i - bpp],
                                last[i],
                                last[i - bpp],
                            ));
                        }
                    }
                }
            }
        }
        _ => dest.copy_from_slice(src),
    }
}

pub fn tiff_predict_line(dest: &mut [u8], bits_per_component: i32, colors: i32, columns: i32) {
    if matches!(bits_per_component, 1 | 2 | 4) {
        let (Ok(colors), Ok(columns)) = (usize::try_from(colors), usize::try_from(columns)) else {
            return;
        };
        if colors == 0 {
            return;
        }
        // Horizontal prediction operates on samples, including packed 1/2/4-bit
        // components. Rounding their pixel stride down to bytes either loses
        // the component relationship or adds each byte to itself.
        let bits = bits_per_component as usize;
        let samples = colors
            .saturating_mul(columns)
            .min(dest.len().saturating_mul(8) / bits);
        let mask = (1u8 << bits) - 1;
        for sample in colors..samples {
            let position = sample * bits;
            let previous = (sample - colors) * bits;
            let shift = 8 - bits - position % 8;
            let previous_shift = 8 - bits - previous % 8;
            let current = (dest[position / 8] >> shift) & mask;
            let left = (dest[previous / 8] >> previous_shift) & mask;
            let value = current.wrapping_add(left) & mask;
            dest[position / 8] = (dest[position / 8] & !(mask << shift)) | (value << shift);
        }
        return;
    }
    let bytes_per_pixel = (bits_per_component * colors / 8) as usize;
    if bits_per_component == 16 {
        for i in (bytes_per_pixel..dest.len().saturating_sub(1)).step_by(2) {
            let previous =
                u16::from_be_bytes([dest[i - bytes_per_pixel], dest[i - bytes_per_pixel + 1]]);
            let current = u16::from_be_bytes([dest[i], dest[i + 1]]);
            let pixel = previous.wrapping_add(current).to_be_bytes();
            dest[i..i + 2].copy_from_slice(&pixel);
        }
    } else {
        for i in bytes_per_pixel..dest.len() {
            dest[i] = dest[i].wrapping_add(dest[i - bytes_per_pixel]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_predictor, png_predict_line, tiff_predict_line, INVALID_OFFSET};

    fn reference_png_line(dest: &mut [u8], src: &[u8], last: &[u8], bpp: usize) {
        let tag = src[0];
        for i in 0..dest.len() {
            let left = if i >= bpp { dest[i - bpp] } else { 0 };
            let up = last.get(i).copied().unwrap_or(0);
            let upper_left = if i >= bpp {
                last.get(i - bpp).copied().unwrap_or(0)
            } else {
                0
            };
            let predictor = match tag {
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => super::paeth_predictor(left, up, upper_left),
                _ => 0,
            };
            dest[i] = src[i + 1].wrapping_add(predictor);
        }
    }

    #[test]
    fn png_tags_and_widths_match_bitwise_reference() {
        for tag in 0u8..=5 {
            for bpp in [1u32, 2, 3, 4, 6] {
                for row_size in [1usize, 2, 3, 4, 5, 7, 8, 12, 16, 31, 64, 129] {
                    let mut src = vec![tag];
                    let mut last = Vec::with_capacity(row_size);
                    for i in 0..row_size {
                        src.push((i * 73 + tag as usize * 19 + bpp as usize * 41) as u8);
                        last.push((i * 151 + tag as usize * 31 + bpp as usize * 17) as u8);
                    }
                    for previous in [&[][..], &last[..]] {
                        let mut expected = vec![0; row_size];
                        let mut actual = vec![0; row_size];
                        reference_png_line(&mut expected, &src, previous, bpp as usize);
                        png_predict_line(&mut actual, &src, previous, row_size, bpp);
                        assert_eq!(actual, expected, "tag={tag} bpp={bpp} row_size={row_size}");
                    }
                }
            }
        }
    }

    #[test]
    fn truncated_png_row_and_invalid_pitch_preserve_return_contract() {
        let output = apply_predictor(vec![1, 1, 2], 12, 1, 8, 3, 10);
        assert_eq!(output.data, [1, 3]);
        assert_eq!(output.bytes_consumed, 10);
        let output = apply_predictor(vec![1, 2], 12, 0, 8, 3, 10);
        assert_eq!(output.data, [1, 2]);
        assert_eq!(output.bytes_consumed, INVALID_OFFSET);
    }

    #[test]
    fn tiff_packed_components_follow_sample_stride_and_keep_padding_bits() {
        for (bits, colors, columns, input, expected) in [
            (2, 1, 8, vec![0x55, 0x55], vec![0x6c, 0x6c]),
            (2, 2, 4, vec![0x55, 0x55], vec![0x5a, 0xf0]),
            (4, 1, 4, vec![0x11, 0x11], vec![0x12, 0x34]),
            (4, 2, 2, vec![0x11, 0x11], vec![0x11, 0x22]),
            (1, 3, 2, vec![0b111_111_01], vec![0b111_000_01]),
            (2, 1, 3, vec![0b01_01_01_11], vec![0b01_10_11_11]),
        ] {
            let mut actual = input;
            tiff_predict_line(&mut actual, bits, colors, columns);
            assert_eq!(actual, expected, "bits={bits} colors={colors}");
        }
    }

    #[test]
    fn tiff_one_eight_and_sixteen_bit_lines() {
        let mut one_bit = [0b1010_0000u8];
        tiff_predict_line(&mut one_bit, 1, 1, 4);
        assert_eq!(one_bit[0] >> 4, 0b1100);
        let mut eight_bit = [1u8, 2, 3, 4];
        tiff_predict_line(&mut eight_bit, 8, 1, 4);
        assert_eq!(eight_bit, [1, 3, 6, 10]);
        let mut sixteen_bit = [0, 1, 0, 2, 0, 3];
        tiff_predict_line(&mut sixteen_bit, 16, 1, 3);
        assert_eq!(sixteen_bit, [0, 1, 0, 3, 0, 6]);
    }
}

#[cfg(test)]
#[path = "image_predictors_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
#[path = "png_sub_tests.rs"]
mod png_sub_tests;
