// Copyright 2015 The PDFium Authors
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Use of this source code is governed by the BSD-style license in
// ../../../LICENSE-PDFIUM.

//! Safe translation of core/fxcodec/jbig2/jbig2_grrd_proc.cpp.
//! Row byte access uses the bitmap's contiguous u32 storage without conversion.

use super::has_default_at_pixels;
use crate::arithmetic_decoder::{ArithmeticDecoder, ArithmeticDecoderContext};
use crate::bitmap::Bitmap;
use crate::decode::{AdaptiveTemplatePixel, RefinementTemplate};
use crate::error::{OverflowError, Result, bail};

pub(super) fn decode(
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut [ArithmeticDecoderContext],
    region: &mut Bitmap,
    reference: &Bitmap,
    reference_dx: i32,
    reference_dy: i32,
    template: RefinementTemplate,
    at: &[AdaptiveTemplatePixel],
    tpgron: bool,
) -> Result<()> {
    if !Bitmap::is_valid_image_size(region.width, region.height) {
        return Ok(());
    }
    let optimized = reference_dx == 0
        && region.width == reference.width
        && (template == RefinementTemplate::Template1 || has_default_at_pixels(at));
    if region.data.is_empty() || (optimized && reference.data.is_empty()) {
        bail!(OverflowError::BitmapDimension);
    }
    if optimized {
        decode_optimized(
            decoder,
            contexts,
            region,
            reference,
            reference_dy,
            template,
            tpgron,
        )
    } else {
        decode_unoptimized(
            decoder,
            contexts,
            region,
            reference,
            reference_dx,
            reference_dy,
            template,
            at,
            tpgron,
        )
    }
}

#[inline]
fn pixel(image: &Bitmap, x: u32, y: u32) -> u32 {
    u32::from(image.get_pixel(x, y))
}

/// GetRowRefs/TypicalPrediction use unshifted rows, including when the context
/// rows selected by GetRowRefsDy have a nonzero displacement.
fn typical_prediction(reference: &Bitmap, x: u32, y: u32, value: u32) -> bool {
    let xs = [x.wrapping_sub(1), x, x.wrapping_add(1)];
    let ys = [y.wrapping_sub(1), y, y.wrapping_add(1)];
    for (row, &ry) in ys.iter().enumerate() {
        for (column, &rx) in xs.iter().enumerate() {
            if row == 1 && column == 1 {
                continue;
            }
            if value != pixel(reference, rx, ry) {
                return false;
            }
        }
    }
    true
}

fn decode_unoptimized(
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut [ArithmeticDecoderContext],
    region: &mut Bitmap,
    reference: &Bitmap,
    dx: i32,
    dy: i32,
    template: RefinementTemplate,
    at: &[AdaptiveTemplatePixel],
    tpgron: bool,
) -> Result<()> {
    let template0 = template == RefinementTemplate::Template0;
    let mut ltp = 0;
    for h in 0..region.height {
        if tpgron {
            ltp ^= decoder.read_bitmap_bit(&mut contexts[if template0 { 0x10 } else { 8 }])?;
        }
        // The native expressions mix uint32 row/column indices and signed
        // displacements. Wrapping also defines its signed -INT_MIN edge safely.
        let prev = h.wrapping_sub(1);
        let ref_y = h.wrapping_sub(dy as u32);
        let ref_x = (dx as u32).wrapping_neg();
        let ys = [ref_y.wrapping_sub(1), ref_y, ref_y.wrapping_add(1)];
        let mut lines = [0u32; 5];
        lines[0] = pixel(region, 1, prev) | (pixel(region, 0, prev) << 1);
        if template0 {
            lines[2] = pixel(reference, ref_x.wrapping_add(1), ys[0])
                | (pixel(reference, ref_x, ys[0]) << 1);
            lines[4] = pixel(reference, ref_x.wrapping_add(1), ys[2])
                | (pixel(reference, ref_x, ys[2]) << 1)
                | (pixel(reference, ref_x.wrapping_sub(1), ys[2]) << 2);
        } else {
            lines[2] = pixel(reference, ref_x, ys[0]);
            lines[4] = pixel(reference, ref_x.wrapping_add(1), ys[2])
                | (pixel(reference, ref_x, ys[2]) << 1);
        }
        lines[3] = pixel(reference, ref_x.wrapping_add(1), ys[1])
            | (pixel(reference, ref_x, ys[1]) << 1)
            | (pixel(reference, ref_x.wrapping_sub(1), ys[1]) << 2);
        for w in 0..region.width {
            let mut value = pixel(reference, w, h);
            if ltp == 0 || !typical_prediction(reference, w, h, value) {
                let context = if template0 {
                    lines[4]
                        | (lines[3] << 3)
                        | (lines[2] << 6)
                        | (pixel(
                            reference,
                            w.wrapping_sub(dx as u32).wrapping_add(at[1].x as u32),
                            ref_y.wrapping_add(at[1].y as u32),
                        ) << 8)
                        | (lines[1] << 9)
                        | (lines[0] << 10)
                        | (pixel(
                            region,
                            w.wrapping_add(at[0].x as u32),
                            h.wrapping_add(at[0].y as u32),
                        ) << 12)
                } else {
                    lines[4] | (lines[3] << 2) | (lines[2] << 5) | (lines[1] << 6) | (lines[0] << 7)
                };
                value = decoder.read_bitmap_bit(&mut contexts[context as usize])?;
            }
            region.set_pixel(w, h, value as u8);
            if template0 {
                let w_dx = w.wrapping_sub(dx as u32).wrapping_add(2);
                lines[0] = ((lines[0] << 1) | pixel(region, w + 2, prev)) & 3;
                lines[1] = ((lines[1] << 1) | value) & 1;
                lines[2] = ((lines[2] << 1) | pixel(reference, w_dx, ys[0])) & 3;
                lines[3] = ((lines[3] << 1) | pixel(reference, w_dx, ys[1])) & 7;
                lines[4] = ((lines[4] << 1) | pixel(reference, w_dx, ys[2])) & 7;
            } else {
                let w_dx = w.wrapping_sub(dx as u32).wrapping_add(1);
                lines[0] = ((lines[0] << 1) | pixel(region, w + 2, prev)) & 7;
                lines[1] = ((lines[1] << 1) | value) & 1;
                lines[2] = ((lines[2] << 1) | pixel(reference, w_dx, ys[0])) & 1;
                lines[3] = ((lines[3] << 1) | pixel(reference, w_dx.wrapping_add(1), ys[1])) & 7;
                lines[4] = ((lines[4] << 1) | pixel(reference, w_dx.wrapping_add(1), ys[2])) & 3;
            }
        }
    }
    Ok(())
}

#[inline]
fn byte(image: &Bitmap, y: u32, x: u32) -> u32 {
    if y >= image.height || x >= image.stride * 4 {
        return 0;
    }
    (image.data[(y * image.stride + x / 4) as usize] >> (24 - (x % 4) * 8)) & 255
}

#[inline]
fn set_byte(image: &mut Bitmap, y: u32, x: u32, value: u32) {
    let shift = 24 - (x % 4) * 8;
    let word = &mut image.data[(y * image.stride + x / 4) as usize];
    *word = (*word & !(255 << shift)) | (value << shift);
}

/// DecodeTemplate0Opt/DecodeTemplate1Opt retain byte-sized shift registers.
/// Their non-LTP loops intentionally do not check IsComplete before Decode.
fn decode_optimized(
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut [ArithmeticDecoderContext],
    region: &mut Bitmap,
    reference: &Bitmap,
    dy: i32,
    template: RefinementTemplate,
    tpgron: bool,
) -> Result<()> {
    let template0 = template == RefinementTemplate::Template0;
    // Unlike native, preserve every encoded DY. T.88 6.3.5.2 defines samples
    // outside the displaced reference as zero. Widen before subtraction and
    // neighboring-row arithmetic so even i32::MIN/MAX cannot overflow.
    let reference_height = i64::from(reference.height);
    let mut ltp = 0;
    for h in 0..region.height {
        if tpgron {
            ltp ^= decoder.read_bitmap_bit(&mut contexts[if template0 { 0x10 } else { 8 }])?;
        }
        let mut line1 = if h == 0 {
            0
        } else {
            byte(region, h - 1, 0) << if template0 { 4 } else { 1 }
        };
        let reference_h = i64::from(h) - i64::from(dy);
        // Validate once per row, before narrowing to the bitmap index type.
        let ys = [reference_h - 1, reference_h, reference_h + 1]
            .map(|y| (0..reference_height).contains(&y).then_some(y as u32));
        let mut refs = ys.map(|y| y.map_or(0, |y| byte(reference, y, 0)));
        let mut context = if template0 {
            (line1 & 0x1c00) | (refs[0] & 0x01c0) | ((refs[1] >> 3) & 0x0038) | ((refs[2] >> 6) & 7)
        } else {
            (line1 & 0x0380)
                | ((refs[0] >> 2) & 0x20)
                | ((refs[1] >> 4) & 0x1c)
                | ((refs[2] >> 6) & 3)
        };
        for w in (0..region.width).step_by(8) {
            let n_bits = (region.width - w).min(8);
            if h != 0 {
                line1 <<= 8;
                if w + 8 < region.width {
                    line1 |= byte(region, h - 1, w / 8 + 1) << if template0 { 4 } else { 1 };
                }
            }
            if template0 && ltp == 0 && reference_h > reference_height + 1 {
                refs = [0; 3];
            } else {
                for (y, value) in ys.into_iter().zip(&mut refs) {
                    if let Some(y) = y {
                        *value <<= 8;
                        if w + 8 < reference.width {
                            *value |= byte(reference, y, w / 8 + 1);
                        }
                    }
                }
            }
            let mut output = 0;
            for k in 0..n_bits {
                let value = if ltp == 0 {
                    decoder.read_bit(&mut contexts[context as usize])
                } else {
                    let predicted = pixel(reference, w + k, h);
                    if typical_prediction(reference, w + k, h, predicted) {
                        predicted
                    } else {
                        decoder.read_bitmap_bit(&mut contexts[context as usize])?
                    }
                };
                output |= value << (7 - k);
                context = if template0 {
                    ((context & 0x0cdb) << 1)
                        | (value << 9)
                        | ((line1 >> (7 - k)) & 0x0400)
                        | ((refs[0] >> (7 - k)) & 0x0040)
                        | ((refs[1] >> (10 - k)) & 8)
                        | ((refs[2] >> (13 - k)) & 1)
                } else {
                    ((context & 0x018d) << 1)
                        | (value << 6)
                        | ((line1 >> (7 - k)) & 0x80)
                        | ((refs[0] >> (9 - k)) & 0x20)
                        | ((refs[1] >> (11 - k)) & 4)
                        | ((refs[2] >> (13 - k)) & 1)
                };
            }
            set_byte(region, h, w / 8, output);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn out_of_bounds_reference_dy_keeps_general_prediction_contexts() {
        let mut reference = Bitmap::new_pdfium(17, 3);
        reference
            .data
            .copy_from_slice(&[0x81818000, 0x7e7e0000, 0xa5a58000]);
        let at = [AdaptiveTemplatePixel { x: -1, y: -1 }; 2];
        for template in [RefinementTemplate::Template0, RefinementTemplate::Template1] {
            for dy in [i32::MIN, i32::MAX, -4, -3, 3, 4] {
                let mut expected = None;
                for dispatch in [false, true] {
                    let mut output = Bitmap::new_pdfium(17, 3);
                    let mut decoder = ArithmeticDecoder::new_with_pdfium(&[0x55; 128], true);
                    let mut contexts =
                        vec![ArithmeticDecoderContext::default(); 1 << template.context_bits()];
                    if dispatch {
                        decode(
                            &mut decoder,
                            &mut contexts,
                            &mut output,
                            &reference,
                            0,
                            dy,
                            template,
                            &at,
                            true,
                        )
                        .unwrap();
                    } else {
                        decode_unoptimized(
                            &mut decoder,
                            &mut contexts,
                            &mut output,
                            &reference,
                            0,
                            dy,
                            template,
                            &at,
                            true,
                        )
                        .unwrap();
                    }
                    let result = (output.data, decoder.byte_position());
                    if let Some(expected) = &expected {
                        assert_eq!(&result, expected, "template {template:?}, dy {dy}");
                    } else {
                        expected = Some(result);
                    }
                }
            }
        }
    }

    #[test]
    fn refinement_shift_registers_agree_without_padding_or_prediction() {
        let mut reference = Bitmap::new_pdfium(32, 4);
        reference
            .data
            .copy_from_slice(&[0x81f035a7, 0x7e6c935a, 0xa55ac33c, 0x7b913f08]);
        let at = [AdaptiveTemplatePixel { x: -1, y: -1 }; 2];
        for template in [RefinementTemplate::Template0, RefinementTemplate::Template1] {
            for dy in [-1, 0, 1] {
                let mut expected = None;
                for optimized in [false, true] {
                    let mut output = Bitmap::new_pdfium(32, 4);
                    let mut decoder = ArithmeticDecoder::new_with_pdfium(&[0x55; 128], true);
                    let mut contexts =
                        vec![ArithmeticDecoderContext::default(); 1 << template.context_bits()];
                    if optimized {
                        decode_optimized(
                            &mut decoder,
                            &mut contexts,
                            &mut output,
                            &reference,
                            dy,
                            template,
                            false,
                        )
                        .unwrap();
                    } else {
                        decode_unoptimized(
                            &mut decoder,
                            &mut contexts,
                            &mut output,
                            &reference,
                            0,
                            dy,
                            template,
                            &at,
                            false,
                        )
                        .unwrap();
                    }
                    let result = (output.data, decoder.byte_position());
                    if let Some(expected) = &expected {
                        assert_eq!(&result, expected, "template {template:?}, dy {dy}");
                    } else {
                        expected = Some(result);
                    }
                }
            }
        }
    }

    #[test]
    fn unoptimized_extreme_offsets_resolve_outside_the_reference() {
        let mut reference = Bitmap::new_pdfium(17, 3);
        reference.data.fill(u32::MAX);
        let empty = Bitmap::new_pdfium(0, 0);
        let at = [AdaptiveTemplatePixel { x: 0, y: -1 }; 2];
        for template in [RefinementTemplate::Template0, RefinementTemplate::Template1] {
            let mut expected = None;
            for image in [&empty, &reference] {
                let mut output = Bitmap::new_pdfium(17, 3);
                let mut decoder = ArithmeticDecoder::new_with_pdfium(&[0x55; 128], true);
                let mut contexts =
                    vec![ArithmeticDecoderContext::default(); 1 << template.context_bits()];
                decode(
                    &mut decoder,
                    &mut contexts,
                    &mut output,
                    image,
                    i32::MIN,
                    i32::MIN,
                    template,
                    &at,
                    false,
                )
                .unwrap();
                let result = (output.data, decoder.byte_position());
                if let Some(expected) = &expected {
                    assert_eq!(&result, expected);
                } else {
                    expected = Some(result);
                }
            }
        }
    }
}
