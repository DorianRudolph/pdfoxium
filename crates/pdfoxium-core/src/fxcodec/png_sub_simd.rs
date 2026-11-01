// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Parallel form of image_predictors.cpp::PredictSubRow's independent
//! wrapping-byte channel chains. Each vector computes local prefix sums;
//! the preceding vector supplies one accumulated carry per channel.

use fearless_simd::{dispatch, prelude::*, u8x16, Level};

pub(super) fn predict_sub_row(dest: &mut [u8], src: &[u8], bpp: usize) {
    dispatch!(Level::new(), simd => match bpp {
        1 => predict::<_, 1>(simd, dest, src),
        2 => predict::<_, 2>(simd, dest, src),
        3 => predict::<_, 3>(simd, dest, src),
        4 => predict::<_, 4>(simd, dest, src),
        _ => unreachable!("SIMD Sub requires one to four bytes per pixel"),
    });
}

#[inline(always)]
fn predict<S: Simd, const BPP: usize>(simd: S, dest: &mut [u8], src: &[u8]) {
    let full = dest.len() / 16 * 16;
    let zero = u8x16::splat(simd, 0);
    // Sixteen need not be a multiple of BPP. The previous vector's last BPP
    // bytes are already ordered for the next vector's channel phase.
    let carry_indices = u8x16::from_fn(simd, |i| (16 - BPP + i % BPP) as u8);
    let mut previous = zero;
    for (input, output) in src[..full]
        .as_chunks::<16>()
        .0
        .iter()
        .zip(dest[..full].as_chunks_mut::<16>().0.iter_mut())
    {
        let mut values = u8x16::from_slice(simd, input);
        match BPP {
            1 => {
                values = values + zero.slide::<15>(values);
                values = values + zero.slide::<14>(values);
                values = values + zero.slide::<12>(values);
                values = values + zero.slide::<8>(values);
            }
            2 => {
                values = values + zero.slide::<14>(values);
                values = values + zero.slide::<12>(values);
                values = values + zero.slide::<8>(values);
            }
            3 => {
                values = values + zero.slide::<13>(values);
                values = values + zero.slide::<10>(values);
                values = values + zero.slide::<4>(values);
            }
            4 => {
                values = values + zero.slide::<12>(values);
                values = values + zero.slide::<8>(values);
            }
            _ => unreachable!(),
        }
        values = values + previous.swizzle_dyn(carry_indices);
        values.store_slice(output);
        previous = values;
    }
    for i in full..dest.len() {
        dest[i] = src[i].wrapping_add(if i >= BPP { dest[i - BPP] } else { 0 });
    }
}
