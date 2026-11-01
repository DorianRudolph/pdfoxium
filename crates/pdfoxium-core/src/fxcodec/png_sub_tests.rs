// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! PNG byte-reconstruction oracle for the optimized Sub prefix scan.
//! The byte-by-byte recurrence is independent of SIMD blocks and carries.

use super::png_predict_line;

fn paeth(left: u8, up: u8, upper_left: u8) -> u8 {
    let prediction = i32::from(left) + i32::from(up) - i32::from(upper_left);
    let dl = (prediction - i32::from(left)).abs();
    let du = (prediction - i32::from(up)).abs();
    let dul = (prediction - i32::from(upper_left)).abs();
    if dl <= du && dl <= dul {
        left
    } else if du <= dul {
        up
    } else {
        upper_left
    }
}

fn reconstruct(tag: u8, source: &[u8], previous: &[u8], bpp: usize) -> Vec<u8> {
    let mut output = vec![0; source.len()];
    for (i, &byte) in source.iter().enumerate() {
        let left = if i >= bpp { output[i - bpp] } else { 0 };
        let up = previous.get(i).copied().unwrap_or(0);
        let upper_left = if i >= bpp {
            previous.get(i - bpp).copied().unwrap_or(0)
        } else {
            0
        };
        let predictor = match tag {
            1 => left,
            4 => paeth(left, up, upper_left),
            _ => unreachable!(),
        };
        output[i] = byte.wrapping_add(predictor);
    }
    output
}

fn pattern(width: usize, kind: usize, salt: usize) -> Vec<u8> {
    let mut random = 0x1248_9abc_7654_3210u64 ^ salt as u64;
    (0..width)
        .map(|i| match kind {
            0 => 0,
            1 => 255,
            2 => [255, 254, 253, 1, 2, 3, 127, 128][i % 8],
            3 => (i * 197 + salt * 53 + i / 3) as u8,
            4 => {
                if matches!(
                    i % 64,
                    0 | 2 | 3 | 14 | 15 | 16 | 17 | 30 | 31 | 32 | 33 | 63
                ) {
                    255
                } else {
                    0
                }
            }
            5 => u8::from(i + 1 == width),
            _ => {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                random as u8
            }
        })
        .collect()
}

fn check_line(tag: u8, bytes: &[u8], previous: Option<&[u8]>, bpp: usize) {
    let width = bytes.len();
    let expected = reconstruct(tag, bytes, previous.unwrap_or_default(), bpp);
    let mut source = vec![0x6d; 5];
    source.push(tag);
    source.extend_from_slice(bytes);
    source.extend_from_slice(&[0x49; 11]);
    let source_before = source.clone();
    let mut last = vec![0x97; 3];
    if let Some(previous) = previous {
        assert_eq!(previous.len(), width);
        last.extend_from_slice(previous);
    }
    last.extend_from_slice(&[0xc3; 9]);
    let last_before = last.clone();
    let mut output = vec![0xa7; 3 + width + 13];
    let mut guarded_expected = output.clone();
    guarded_expected[3..3 + width].copy_from_slice(&expected);
    png_predict_line(
        &mut output[3..],
        &source[5..],
        if previous.is_some() { &last[3..] } else { &[] },
        width,
        bpp as u32,
    );
    assert_eq!(
        output, guarded_expected,
        "tag={tag} bpp={bpp} width={width}"
    );
    assert_eq!(source, source_before);
    assert_eq!(last, last_before);
}

#[test]
fn sub_and_first_paeth_rows_preserve_byte_recurrence_block_carries_and_guards() {
    for width in (0usize..=65).chain([127, 128, 129, 255, 256, 257, 1023, 1024, 1025, 12960, 12961])
    {
        for bpp in [1usize, 2, 3, 4, 6, 8] {
            for kind in 0..7 {
                let input = pattern(width, kind, bpp + 11);
                check_line(1, &input, None, bpp);
                check_line(4, &input, None, bpp);
                // Sub ignores the previous row even when one is supplied.
                let previous = pattern(width, 6, bpp + 97);
                check_line(1, &input, Some(&previous), bpp);
            }
        }
    }
}

#[test]
fn paeth_with_previous_row_keeps_its_independent_recurrence() {
    for width in (0usize..=65).chain([127, 128, 129, 1023, 1024, 1025, 12960, 12961]) {
        for bpp in [1usize, 2, 3, 4, 6, 8] {
            for kind in 0..7 {
                let input = pattern(width, kind, bpp + 31);
                let previous = pattern(width, 6 - kind, bpp + 83);
                check_line(4, &input, Some(&previous), bpp);
            }
        }
    }
}
