// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in ../../../LICENSE-PDFIUM.

//! Frozen scalar byte recurrence for the default templates. Unlike a per-pixel
//! bitmap oracle, this also preserves old partial-byte writes on arithmetic EOF.

use alloc::{vec, vec::Vec};

use super::{ArithmeticProgress, decode_bitmap_arithmetic_coding_progressive};
use crate::arithmetic_decoder::{ArithmeticDecoder, ArithmeticDecoderContext};
use crate::bitmap::Bitmap;
use crate::decode::{AdaptiveTemplatePixel, Template};
use crate::error::Result;

const TEMPLATES: [Template; 4] = [
    Template::Template0,
    Template::Template1,
    Template::Template2,
    Template::Template3,
];

// Native TemplateOpt3Params / original scalar fast-loop constants, separate
// from the production parameter definitions and the new zero-byte shortcut.
// prev2 shift/mask, prev1 shift/mask, rolling mask, next-row2/next-row1 masks.
const PARAMS: [(u32, u32, u32, u32, u32, u32, u32); 4] = [
    (6, 0xf800, 0, 0x07f0, 0x7bf7, 0x0800, 0x0010),
    (4, 0x1e00, 1, 0x01f8, 0x0efb, 0x0200, 0x0008),
    (1, 0x0380, 3, 0x007c, 0x01bd, 0x0080, 0x0004),
    (0, 0x0000, 1, 0x03f0, 0x01f7, 0x0000, 0x0010),
];
const TP_CONTEXT: [usize; 4] = [0x9b25, 0x0795, 0x00e5, 0x0195];

fn template_index(template: Template) -> usize {
    match template {
        Template::Template0 => 0,
        Template::Template1 => 1,
        Template::Template2 => 2,
        Template::Template3 => 3,
    }
}

fn adaptive_pixels(template: Template) -> [AdaptiveTemplatePixel; 4] {
    let mut at = [AdaptiveTemplatePixel::default(); 4];
    at[0] = AdaptiveTemplatePixel { x: 2, y: -1 };
    if matches!(template, Template::Template0 | Template::Template1) {
        at[0].x = 3;
    }
    if matches!(template, Template::Template0) {
        at[1] = AdaptiveTemplatePixel { x: -3, y: -1 };
        at[2] = AdaptiveTemplatePixel { x: 2, y: -2 };
        at[3] = AdaptiveTemplatePixel { x: -2, y: -2 };
    }
    at
}

fn row_byte(bitmap: &Bitmap, row: u32, byte: usize) -> u32 {
    let word = bitmap.data[row as usize * bitmap.stride as usize + byte / 4];
    (word >> (24 - 8 * (byte % 4))) & 0xff
}

fn put_row_byte(bitmap: &mut Bitmap, row: u32, byte: usize, value: u8) {
    let index = row as usize * bitmap.stride as usize + byte / 4;
    let shift = 24 - 8 * (byte % 4);
    bitmap.data[index] = (bitmap.data[index] & !(0xff << shift)) | (u32::from(value) << shift);
}

fn scalar_default_loop(
    bitmap: &mut Bitmap,
    decoder: &mut ArithmeticDecoder<'_>,
    contexts: &mut [ArithmeticDecoderContext],
    template: Template,
    tpgdon: bool,
    progress: &mut ArithmeticProgress,
    pause: &mut dyn FnMut() -> bool,
) -> Result<()> {
    progress.paused = false;
    if bitmap.width == 0 || bitmap.height == 0 {
        return Ok(());
    }
    let index = template_index(template);
    let (prev2_shift, prev2_mask, prev1_shift, prev1_mask, roll_mask, next2_mask, next1_mask) =
        PARAMS[index];
    let stride = bitmap.stride as usize;
    let full_bytes = ((bitmap.width - 1) / 8) as usize;
    let last_bits = ((bitmap.width - 1) & 7) + 1;
    let mut ltp = progress.ltp;
    for row in progress.row..bitmap.height {
        if tpgdon {
            ltp ^= decoder.read_bitmap_bit(&mut contexts[TP_CONTEXT[index]])? != 0;
        }
        if ltp {
            if row > 0 {
                let source = (row as usize - 1) * stride;
                bitmap
                    .data
                    .copy_within(source..source + stride, row as usize * stride);
            }
        } else {
            let mut previous2 = if row >= 2 {
                row_byte(bitmap, row - 2, 0) << prev2_shift
            } else {
                0
            };
            let mut previous1 = if row >= 1 {
                row_byte(bitmap, row - 1, 0)
            } else {
                0
            };
            let mut context = (previous2 & prev2_mask) | ((previous1 >> prev1_shift) & prev1_mask);
            for byte_index in 0..full_bytes {
                previous2 = if row >= 2 {
                    (previous2 << 8) | (row_byte(bitmap, row - 2, byte_index + 1) << prev2_shift)
                } else {
                    0
                };
                previous1 = if row >= 1 {
                    (previous1 << 8) | row_byte(bitmap, row - 1, byte_index + 1)
                } else {
                    0
                };
                let mut value = 0;
                for bit in (0..8).rev() {
                    let pixel = decoder.read_bitmap_bit(&mut contexts[context as usize])?;
                    value |= (pixel as u8) << bit;
                    context = ((context & roll_mask) << 1)
                        | pixel
                        | ((previous2 >> bit) & next2_mask)
                        | ((previous1 >> (bit + prev1_shift)) & next1_mask);
                }
                put_row_byte(bitmap, row, byte_index, value);
            }
            previous2 <<= 8;
            previous1 <<= 8;
            let mut value = 0;
            for bit in 0..last_bits {
                let pixel = decoder.read_bitmap_bit(&mut contexts[context as usize])?;
                value |= (pixel as u8) << (7 - bit);
                context = ((context & roll_mask) << 1)
                    | pixel
                    | ((previous2 >> (7 - bit)) & next2_mask)
                    | ((previous1 >> (7 + prev1_shift - bit)) & next1_mask);
            }
            put_row_byte(bitmap, row, full_bytes, value);
        }
        decoder.completed_rows = row + 1;
        progress.row = row + 1;
        progress.ltp = ltp;
        if pause() {
            progress.paused = true;
            return Ok(());
        }
    }
    Ok(())
}

fn compare_case(
    data: &[u8],
    width: u32,
    height: u32,
    template: Template,
    pdfium: bool,
    tpgdon: bool,
    seed: Option<u32>,
    pause_every: Option<u32>,
    background: u32,
) {
    let mut actual = Bitmap::new(width, height).unwrap();
    actual.pdfium = pdfium;
    actual.data.fill(background);
    let mut expected = actual.clone();
    let mut decoder = ArithmeticDecoder::new_with_pdfium(data, pdfium);
    let mut scalar = ArithmeticDecoder::new_with_pdfium(data, pdfium);
    let initial = seed.map_or_else(ArithmeticDecoderContext::default, |mps| {
        ArithmeticDecoderContext::optimization_test_context(45, mps)
    });
    let mut contexts = vec![initial; 1 << template.context_bits()];
    let mut reference_contexts = contexts.clone();
    let mut progress = ArithmeticProgress::default();
    let mut reference_progress = ArithmeticProgress::default();
    let mut calls = 0;
    let mut scalar_calls = 0;
    let mut finished = false;
    for _ in 0..height + 2 {
        let result = decode_bitmap_arithmetic_coding_progressive(
            &mut actual,
            &mut decoder,
            &mut contexts,
            template,
            tpgdon,
            &adaptive_pixels(template),
            &mut progress,
            &mut || {
                calls += 1;
                pause_every.is_some_and(|period| calls % period == 0)
            },
        );
        let reference_result = scalar_default_loop(
            &mut expected,
            &mut scalar,
            &mut reference_contexts,
            template,
            tpgdon,
            &mut reference_progress,
            &mut || {
                scalar_calls += 1;
                pause_every.is_some_and(|period| scalar_calls % period == 0)
            },
        );
        assert_eq!(
            result,
            reference_result,
            "{template:?} {width}x{height}, pdfium={pdfium}, tpgdon={tpgdon}, input={}",
            data.len()
        );
        assert_eq!(actual.data, expected.data);
        assert_eq!(decoder.save(), scalar.save());
        assert_eq!(decoder.byte_position(), scalar.byte_position());
        assert_eq!(decoder.completed_rows, scalar.completed_rows);
        assert_eq!(
            (progress.row, progress.ltp, progress.paused),
            (
                reference_progress.row,
                reference_progress.ltp,
                reference_progress.paused
            )
        );
        assert_eq!(calls, scalar_calls);
        for (a, b) in contexts.iter().zip(&reference_contexts) {
            assert_eq!((a.index(), a.mps()), (b.index(), b.mps()));
        }
        if result.is_err() || !progress.paused {
            finished = true;
            break;
        }
        // The decoded bitmap, contexts and row metadata stay live, while the
        // arithmetic registers may be suspended independently of their source.
        decoder = ArithmeticDecoder::resume(data, decoder.save());
    }
    assert!(
        finished,
        "row suspension must eventually finish or reach the same EOF"
    );
}

fn random_data() -> Vec<u8> {
    let mut state = 0x62d8_0ab3u32;
    (0..96)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 24) as u8
        })
        .collect()
}

#[test]
fn default_templates_match_scalar_byte_commits_at_bitmap_edges() {
    let random = random_data();
    for data in [&random[..], &[0xff, 0x90][..], &[0; 96][..]] {
        for template in TEMPLATES {
            for pdfium in [false, true] {
                for tpgdon in [false, true] {
                    for width in [0, 1, 7, 8, 9, 31, 32, 33, 63, 64, 65, 257] {
                        for height in [0, 1, 3, 9] {
                            compare_case(
                                data, width, height, template, pdfium, tpgdon, None, None, 0,
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn every_input_prefix_preserves_partial_bytes_contexts_and_eof_rows() {
    let random = random_data();
    let marked = [
        0x10, 0xff, 0, 0x72, 0xff, 0x8f, 0xff, 0x90, 0x55, 0xff, 0xac,
    ];
    for data in [&random[..], &marked[..]] {
        for end in 0..=data.len() {
            for template in TEMPLATES {
                for pdfium in [false, true] {
                    for tpgdon in [false, true] {
                        compare_case(
                            &data[..end],
                            65,
                            7,
                            template,
                            pdfium,
                            tpgdon,
                            None,
                            None,
                            0xa55a_08f0,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn low_qe_white_and_black_runs_match_scalar_across_row_suspension() {
    // A high-confidence zero context plus FF termination gives long all-white
    // rows: complete bytes after the first scalar interval must take the new
    // shortcut. MPS1 seeds, nonzero previous rows, TPGDON and random streams
    // exercise its rejection and fallback conditions.
    let random = random_data();
    for data in [&[][..], &[0xff, 0x90][..], &random[..]] {
        for template in TEMPLATES {
            for pdfium in [false, true] {
                for tpgdon in [false, true] {
                    for mps in [0, 1] {
                        for period in [1, 2, 3] {
                            compare_case(
                                data,
                                257,
                                9,
                                template,
                                pdfium,
                                tpgdon,
                                Some(mps),
                                Some(period),
                                0,
                            );
                            compare_case(
                                data,
                                33,
                                5,
                                template,
                                pdfium,
                                tpgdon,
                                Some(mps),
                                Some(period),
                                0xffff_ffff,
                            );
                        }
                    }
                }
            }
        }
    }
}
