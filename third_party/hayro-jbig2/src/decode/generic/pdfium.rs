// Copyright 2015 The PDFium Authors
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Use of this source code is governed by the BSD-style license in
// ../../../LICENSE-PDFIUM.

//! jbig2_grd_proc.cpp's unoptimized templates and static progressive row state.
//! The matching native optimized byte kernels are in the parent module.

use crate::arithmetic_decoder::{ArithmeticDecoder, ArithmeticDecoderContext};
use crate::bitmap::Bitmap;
use crate::decode::{AdaptiveTemplatePixel, Template};
use crate::error::Result;

const TP_CONTEXT: [usize; 4] = [0x9b25, 0x0795, 0x00e5, 0x0195];
const PREV2_SHIFT: [u32; 3] = [12, 9, 7];
const PREV2_MASK: [u32; 3] = [7, 15, 7];
const PREV1_MASK: [u32; 3] = [31, 31, 15];
const CURRENT_MASK: [u32; 4] = [15, 7, 3, 15];

#[inline]
fn pixel(image: &Bitmap, x: u32, y: u32) -> u32 {
    u32::from(image.get_pixel(x, y))
}

pub(super) fn decode_unoptimized(
    image: &mut Bitmap,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut [ArithmeticDecoderContext],
    template: Template,
    tpgdon: bool,
    at: &[AdaptiveTemplatePixel; 4],
    skip: Option<&[u32]>,
) -> Result<()> {
    decode_unoptimized_progressive(
        image,
        decoder,
        contexts,
        template,
        tpgdon,
        at,
        skip,
        &mut super::ArithmeticProgress::default(),
        &mut || false,
    )
}

pub(super) fn decode_unoptimized_progressive(
    image: &mut Bitmap,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut [ArithmeticDecoderContext],
    template: Template,
    tpgdon: bool,
    at: &[AdaptiveTemplatePixel; 4],
    skip: Option<&[u32]>,
    progress: &mut super::ArithmeticProgress,
    pause: &mut dyn FnMut() -> bool,
) -> Result<()> {
    let index = match template {
        Template::Template0 => 0,
        Template::Template1 => 1,
        Template::Template2 => 2,
        Template::Template3 => 3,
    };
    let mod2 = (index % 2) as u32;
    let div2 = (index / 2) as u32;
    let shift = if index == 3 { 4 } else { 4 - index as u32 };
    let mut ltp = u32::from(progress.ltp);
    for h in progress.row..image.height {
        if tpgdon {
            ltp ^= decoder.read_bitmap_bit(&mut contexts[TP_CONTEXT[index]])?;
        }
        let prev1 = h.wrapping_sub(1);
        let prev2 = h.wrapping_sub(2);
        if ltp != 0 {
            let stride = image.stride as usize;
            let start = h as usize * stride;
            if h == 0 {
                image.data[start..start + stride].fill(0);
            } else {
                image.data.copy_within(start - stride..start, start);
            }
            decoder.completed_rows = h + 1;
            progress.row = h + 1;
            progress.ltp = ltp != 0;
            if pause() {
                progress.paused = true;
                return Ok(());
            }
            continue;
        }

        let mut val_prev2 = 0;
        let mut val_prev1;
        if index == 3 {
            val_prev1 = pixel(image, 1, prev1) | (pixel(image, 0, prev1) << 1);
        } else {
            val_prev2 = pixel(image, 1 + mod2, prev2) | (pixel(image, mod2, prev2) << 1);
            if index == 1 {
                val_prev2 |= pixel(image, 0, prev2) << 2;
            }
            val_prev1 = pixel(image, 2 - div2, prev1) | (pixel(image, 1 - div2, prev1) << 1);
            if index < 2 {
                val_prev1 |= pixel(image, 0, prev1) << 2;
            }
        }
        let mut val_current = 0;
        for w in 0..image.width {
            let mut value = 0;
            let skipped = skip.is_some_and(|mask| {
                (mask[(h * image.stride + w / 32) as usize] >> (31 - w % 32)) & 1 != 0
            });
            if !skipped {
                let mut context = val_current
                    | (pixel(
                        image,
                        w.wrapping_add(at[0].x as u32),
                        h.wrapping_add(at[0].y as u32),
                    ) << shift)
                    | (val_prev1 << (shift + 1));
                if index < 3 {
                    context |= val_prev2 << PREV2_SHIFT[index];
                }
                if index == 0 {
                    context |= pixel(
                        image,
                        w.wrapping_add(at[1].x as u32),
                        h.wrapping_add(at[1].y as u32),
                    ) << 10;
                    context |= pixel(
                        image,
                        w.wrapping_add(at[2].x as u32),
                        h.wrapping_add(at[2].y as u32),
                    ) << 11;
                    context |= pixel(
                        image,
                        w.wrapping_add(at[3].x as u32),
                        h.wrapping_add(at[3].y as u32),
                    ) << 15;
                }
                value = decoder.read_bitmap_bit(&mut contexts[context as usize])?;
            }
            if value != 0 {
                image.set_pixel(w, h, value as u8);
            }
            if index == 3 {
                val_prev1 = ((val_prev1 << 1) | pixel(image, w + 2, prev1)) & 31;
            } else {
                val_prev2 =
                    ((val_prev2 << 1) | pixel(image, w + 2 + mod2, prev2)) & PREV2_MASK[index];
                val_prev1 =
                    ((val_prev1 << 1) | pixel(image, w + 3 - div2, prev1)) & PREV1_MASK[index];
            }
            val_current = ((val_current << 1) | value) & CURRENT_MASK[index];
        }
        decoder.completed_rows = h + 1;
        progress.row = h + 1;
        progress.ltp = ltp != 0;
        if pause() {
            progress.paused = true;
            return Ok(());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::generic::decode_bitmap_arithmetic_coding_with_skip;
    use alloc::vec;

    #[test]
    fn skip_dispatch_matches_native_optimized_registers_without_skipped_pixels() {
        for template in [
            Template::Template0,
            Template::Template1,
            Template::Template2,
            Template::Template3,
        ] {
            let at = [
                AdaptiveTemplatePixel {
                    x: if matches!(template, Template::Template0 | Template::Template1) {
                        3
                    } else {
                        2
                    },
                    y: -1,
                },
                AdaptiveTemplatePixel { x: -3, y: -1 },
                AdaptiveTemplatePixel { x: 2, y: -2 },
                AdaptiveTemplatePixel { x: -2, y: -2 },
            ];
            for width in [9u32, 32, 37] {
                let skip = vec![0; width.div_ceil(32) as usize * 4];
                let mut expected = None;
                for mask in [None, Some(skip.as_slice())] {
                    let mut image = Bitmap::new_pdfium(width, 4);
                    let mut decoder = ArithmeticDecoder::new_with_pdfium(&[0x55; 128], true);
                    let mut contexts =
                        vec![ArithmeticDecoderContext::default(); 1 << template.context_bits()];
                    decode_bitmap_arithmetic_coding_with_skip(
                        &mut image,
                        &mut decoder,
                        &mut contexts,
                        template,
                        &at,
                        mask,
                    )
                    .unwrap();
                    let result = (image.data, decoder.byte_position());
                    if let Some(expected) = &expected {
                        assert_eq!(&result, expected, "template {template:?}, width {width}");
                    } else {
                        expected = Some(result);
                    }
                }
            }
        }
    }

    #[test]
    fn fully_skipped_rows_do_not_read_an_empty_arithmetic_stream() {
        for template in [
            Template::Template0,
            Template::Template1,
            Template::Template2,
            Template::Template3,
        ] {
            let mut image = Bitmap::new_pdfium(37, 4);
            let mut decoder = ArithmeticDecoder::new_with_pdfium(&[], true);
            let mut contexts =
                vec![ArithmeticDecoderContext::default(); 1 << template.context_bits()];
            let at = [AdaptiveTemplatePixel { x: -128, y: 127 }; 4];
            decode_bitmap_arithmetic_coding_with_skip(
                &mut image,
                &mut decoder,
                &mut contexts,
                template,
                &at,
                Some(&[u32::MAX; 8]),
            )
            .unwrap();
            assert_eq!(decoder.completed_rows, 4);
            assert!(image.data.iter().all(|&word| word == 0));
        }
    }
}
