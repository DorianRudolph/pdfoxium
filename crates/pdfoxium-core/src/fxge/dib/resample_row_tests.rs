// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Row-level oracle for the pinned wrapping-u32 weighted RGB sums.
//! Decode raw records independently of PixelWeight and use wide arithmetic.

use super::*;

fn check_row<const STRIDE: usize>(table: &WeightTable, source: &[u8]) {
    let width = table.words.len() / table.item_words;
    let mut guarded_source = vec![0x71; 5];
    guarded_source.extend_from_slice(source);
    guarded_source.extend_from_slice(&[0x37; 7]);
    let source_before = guarded_source.clone();
    // Deliberately unaligned and with padding that cannot form a full pixel.
    let mut output = vec![0xa5; 3 + width * STRIDE + STRIDE - 1];
    let mut expected = output.clone();
    for (i, record) in table.words.chunks_exact(table.item_words).enumerate() {
        let start = record[0] as usize;
        let count = (i64::from(record[1] as i32) - start as i64 + 1).max(0) as usize;
        for channel in 0..3 {
            let sum: u128 = (0..count)
                .map(|tap| {
                    u128::from(record[2 + tap])
                        * u128::from(source[(start + tap) * STRIDE + channel])
                })
                .sum();
            expected[3 + i * STRIDE + channel] = ((sum as u32) >> 16) as u8;
        }
    }
    stretch_rgb_row::<STRIDE>(
        table,
        &guarded_source[5..5 + source.len()],
        &mut output[3..],
    );
    assert_eq!(
        output, expected,
        "stride={STRIDE} words={} width={width}",
        table.item_words
    );
    #[cfg(feature = "simd")]
    {
        let mut direct = vec![0xa5; output.len()];
        super::resample_simd::stretch_rgb_row::<STRIDE>(table, source, &mut direct[3..]);
        assert_eq!(direct, expected, "direct SIMD stride={STRIDE}");
    }
    assert_eq!(guarded_source, source_before);
}

fn source<const STRIDE: usize>(pixels: usize) -> Vec<u8> {
    (0..pixels * STRIDE)
        .map(|i| ((i * 197 + i / STRIDE * 53) ^ (i >> 3)) as u8)
        .collect()
}

#[test]
fn rgb_record_sizes_preserve_empty_ranges_unused_weights_overflow_and_guards() {
    fn check<const STRIDE: usize>() {
        let pixels = 137usize;
        let source = source::<STRIDE>(pixels);
        for words in (3..=20).chain([33, 34, 35]) {
            let capacity = words - 2;
            for width in [
                0usize, 1, 2, 3, 4, 5, 7, 8, 9, 11, 12, 13, 15, 16, 17, 31, 32, 33, 65,
            ] {
                let mut table = WeightTable {
                    dest_min: 19,
                    item_words: words,
                    words: vec![0xfedc_ba98; width * words],
                    full_single_taps: false,
                };
                for (i, record) in table.words.chunks_exact_mut(words).enumerate() {
                    let count = i % (capacity + 1);
                    // Some windows end exactly at the source boundary; empty
                    // windows there must not read any unused weight or pixel.
                    let start = if i % 2 == 0 {
                        pixels - count
                    } else {
                        (i * 29) % (pixels - count + 1)
                    };
                    record[0] = start as u32;
                    record[1] = (start as i32 + count as i32 - 1) as u32;
                    for (tap, weight) in record[2..2 + count].iter_mut().enumerate() {
                        *weight = [u32::MAX, 0, 1, 65536, 0x8000_0000, 65535, 7][(i + tap) % 7];
                    }
                    // Unused slots deliberately retain nonzero garbage.
                }
                check_row::<STRIDE>(&table, &source);
            }
        }
    }
    check::<3>();
    check::<4>();
}

#[test]
fn rgb_generated_rows_preserve_book_scales_mirroring_and_destination_clips() {
    fn check<const STRIDE: usize>() {
        for source_width in [29i32, 4320] {
            let source = source::<STRIDE>(source_width as usize);
            for dest_width in [
                1i32, 3, 7, 31, 173, 501, 1001, 4320, 5001, -501, -1001, -4320,
            ] {
                let width = dest_width.abs();
                for left in [0, width / 7] {
                    let right = width - left;
                    for options in [
                        FxdibResampleOptions::default(),
                        FxdibResampleOptions {
                            no_smoothing: true,
                            ..Default::default()
                        },
                        FxdibResampleOptions {
                            interpolate_bilinear: true,
                            ..Default::default()
                        },
                    ] {
                        let mut table = WeightTable::new();
                        assert!(table.calculate_weights(
                            dest_width,
                            left,
                            right,
                            source_width,
                            0,
                            source_width,
                            options,
                        ));
                        check_row::<STRIDE>(&table, &source);
                    }
                }
            }
        }
    }
    check::<3>();
    check::<4>();
}
