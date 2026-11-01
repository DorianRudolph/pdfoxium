// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! Exact straight-alpha BGRA bitmap compositing, eight pixels at a time.

use fearless_simd::{dispatch, f32x8, prelude::*, u32x8, u8x32, Level};

pub(super) fn composite<const RGB: bool, const CLIPPED: bool>(
    dest: &mut [u8],
    source: &[u8],
    clip: &[u8],
) {
    dispatch!(Level::new(), simd => composite_impl::<_, RGB, CLIPPED>(simd, dest, source, clip));
}

#[inline(always)]
fn div255<S: Simd>(value: u32x8<S>) -> u32x8<S> {
    // Numerators never exceed 65025. Preserve floor division by 255.
    let value = value + 1;
    (value + (value >> 8)) >> 8
}

#[inline(always)]
fn composite_impl<S: Simd, const RGB: bool, const CLIPPED: bool>(
    simd: S,
    dest: &mut [u8],
    source: &[u8],
    clip: &[u8],
) {
    for (index, (pixels, source)) in dest
        .chunks_exact_mut(32)
        .zip(source.chunks_exact(32))
        .enumerate()
    {
        let mut source = u32x8::from_bytes(u8x32::from_slice(simd, source));
        if RGB {
            source = (source & 0xff00_ff00) | ((source & 255) << 16) | ((source >> 16) & 255);
        }
        let mut alpha = source >> 24;
        if CLIPPED {
            let covers = u32x8::from_slice(
                simd,
                &std::array::from_fn::<_, 8, _>(|i| u32::from(clip[index * 8 + i])),
            );
            alpha = div255(alpha * covers);
        }
        let source = (source & 0x00ff_ffff) | (alpha << 24);
        if alpha.simd_eq(255).all_true() {
            source.to_bytes().store_slice(pixels);
            continue;
        }
        let back = u32x8::from_bytes(u8x32::from_slice(simd, pixels));
        let back_alpha = back >> 24;
        if alpha.simd_eq(0).all_true() {
            // A transparent backdrop still receives the source's hidden RGB,
            // matching the scalar bitmap compositor's back == 0 branch.
            if !back_alpha.simd_ne(0).all_true() {
                back_alpha
                    .simd_eq(0)
                    .select(source, back)
                    .to_bytes()
                    .store_slice(pixels);
            }
            continue;
        }
        let union = back_alpha + alpha - div255(back_alpha * alpha);
        let ratio = if back_alpha.simd_eq(255).all_true() {
            alpha
        } else {
            // Values <= 65025 are exact floats. A noninteger quotient is at
            // least 1/255 from an integer, so division and truncation are exact
            // here. The harmless divisor 1 handles transparent lanes.
            u32x8::truncate_from(f32x8::float_from(alpha * 255) / f32x8::float_from(union.max(1)))
        };
        let mut output = union << 24;
        for shift in [0, 8, 16] {
            let a = (back >> shift) & 255;
            let b = (source >> shift) & 255;
            output |= div255(a * (u32x8::splat(simd, 255) - ratio) + b * ratio) << shift;
        }
        back_alpha
            .simd_eq(0)
            .select(source, output)
            .to_bytes()
            .store_slice(pixels);
    }
}
