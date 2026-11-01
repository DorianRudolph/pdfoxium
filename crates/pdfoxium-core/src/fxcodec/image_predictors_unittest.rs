// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcodec/image_predictors_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::png_predict_line;

fn ReferencePaethPredictor(a: u8, b: u8, c: u8) -> u8 {
    let p = i32::from(a) + i32::from(b) - i32::from(c);
    let pa = (p - i32::from(a)).abs();
    let pb = (p - i32::from(b)).abs();
    let pc = (p - i32::from(c)).abs();
    if pa <= pb && pa <= pc {
        return a;
    }
    if pb <= pc {
        b
    } else {
        c
    }
}

fn ReferencePngPredictLine(
    dest_span: &mut [u8],
    src_span: &[u8],
    last_span: &[u8],
    row_size: usize,
    bytes_per_pixel: u32,
) {
    let tag = src_span[0];
    let src_span = &src_span[1..1 + row_size];
    let dest_span = &mut dest_span[..row_size];
    let last_span = if last_span.is_empty() {
        last_span
    } else {
        &last_span[..row_size]
    };
    let bpp = bytes_per_pixel as usize;
    for i in 0..row_size {
        let left = if i >= bpp { dest_span[i - bpp] } else { 0 };
        let up = if last_span.is_empty() {
            0
        } else {
            last_span[i]
        };
        let upper_left = if i >= bpp && !last_span.is_empty() {
            last_span[i - bpp]
        } else {
            0
        };
        dest_span[i] = match tag {
            1 => src_span[i].wrapping_add(left),
            2 => src_span[i].wrapping_add(up),
            3 => src_span[i].wrapping_add(((u16::from(up) + u16::from(left)) / 2) as u8),
            4 => src_span[i].wrapping_add(ReferencePaethPredictor(left, up, upper_left)),
            _ => src_span[i],
        };
    }
}

#[test]
fn PngPredictLineAllTagsAndWidths() {
    for tag in 0u8..=5 {
        for bpp in [1u32, 2, 3, 4, 6] {
            for row_size in [1usize, 2, 3, 4, 5, 7, 8, 12, 16, 31, 64, 129] {
                let mut src = vec![0; row_size + 1];
                let mut last = vec![0; row_size];
                src[0] = tag;
                for i in 0..row_size {
                    src[i + 1] = ((i * 73 + tag as usize * 19 + bpp as usize * 41) & 0xff) as u8;
                    last[i] = ((i * 151 + tag as usize * 31 + bpp as usize * 17) & 0xff) as u8;
                }
                let mut dst_ref_first = vec![0; row_size];
                let mut dst_opt_first = vec![0; row_size];
                ReferencePngPredictLine(&mut dst_ref_first, &src, &[], row_size, bpp);
                png_predict_line(&mut dst_opt_first, &src, &[], row_size, bpp);
                assert_eq!(
                    dst_ref_first, dst_opt_first,
                    "First row mismatch for tag={tag} bpp={bpp} row_size={row_size}"
                );
                let mut dst_ref = vec![0; row_size];
                let mut dst_opt = vec![0; row_size];
                ReferencePngPredictLine(&mut dst_ref, &src, &last, row_size, bpp);
                png_predict_line(&mut dst_opt, &src, &last, row_size, bpp);
                assert_eq!(
                    dst_ref, dst_opt,
                    "Row mismatch for tag={tag} bpp={bpp} row_size={row_size}"
                );
            }
        }
    }
}
