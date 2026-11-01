// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Four-tap dot products for long pinned wrapping-u32 RGB filters.
//! Integer lane accumulation and reduction are exact modulo 2^32. Only actual
//! source taps are loaded; tails and BGRx's unused byte retain scalar behavior.

use super::{pixel_from_fixed, pixel_weight_from_entry, weighted_rgb, WeightTable};
use fearless_simd::{dispatch, prelude::*, u32x4, u8x16, Level};

pub(super) fn stretch_rgb_row<const STRIDE: usize>(
    table: &WeightTable,
    source: &[u8],
    dest: &mut [u8],
) {
    dispatch!(Level::new(), simd => row::<_, STRIDE>(simd, table, source, dest));
}

#[inline(always)]
fn row<S: Simd, const STRIDE: usize>(simd: S, table: &WeightTable, source: &[u8], dest: &mut [u8]) {
    let indices: [u8x16<S>; 3] =
        std::array::from_fn(|channel| u8x16::from_fn(simd, |i| ((i % 4) * STRIDE + channel) as u8));
    for (pixel, entry) in dest
        .chunks_exact_mut(STRIDE)
        .zip(table.words.chunks_exact(table.item_words))
    {
        let weights = pixel_weight_from_entry(entry);
        let rgb = if weights.weights.len() < 8 {
            weighted_rgb::<STRIDE, true>(&weights, source)
        } else {
            let source = &source
                [weights.src_start * STRIDE..(weights.src_start + weights.weights.len()) * STRIDE];
            let mut sums = [u32x4::splat(simd, 0); 3];
            let full = weights.weights.len() / 4 * 4;
            for (taps, input) in weights.weights[..full]
                .as_chunks::<4>()
                .0
                .iter()
                .zip(source[..full * STRIDE].chunks_exact(4 * STRIDE))
            {
                let mut bytes = [0u8; 16];
                bytes[..4 * STRIDE].copy_from_slice(input);
                let bytes = u8x16::simd_from(simd, bytes);
                let weights = u32x4::from_slice(simd, taps);
                for channel in 0..3 {
                    let values = bytes.swizzle_dyn(indices[channel]).widen().0.widen().0;
                    sums[channel] = sums[channel] + weights * values;
                }
            }
            let mut rgb = sums.map(|sum| sum.reduce_sum());
            for (&weight, input) in weights.weights[full..]
                .iter()
                .zip(source[full * STRIDE..].chunks_exact(STRIDE))
            {
                for channel in 0..3 {
                    rgb[channel] =
                        rgb[channel].wrapping_add(weight.wrapping_mul(u32::from(input[channel])));
                }
            }
            rgb
        };
        for channel in 0..3 {
            pixel[channel] = pixel_from_fixed(rgb[channel]);
        }
    }
}
