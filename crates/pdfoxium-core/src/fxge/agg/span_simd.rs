// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! Exact variable-coverage CompositeSpanARGB, eight pixels at a time.

use fearless_simd::{dispatch, f32x8, prelude::*, u32x8, u8x32, Level};

pub(super) fn composite(dest: &mut [u8], covers: &[u8], alpha: u8, color: u32) {
    dispatch!(Level::new(), simd => composite_impl(simd, dest, covers, alpha, color));
}

#[inline(always)]
fn div255<S: Simd>(n: u32x8<S>) -> u32x8<S> {
    // Every numerator is in 0..=65025. This is floor division, not the
    // rounded /255 used by some premultiplied-alpha compositors.
    let t = n + 1;
    (t + (t >> 8)) >> 8
}

#[inline(always)]
fn composite_impl<S: Simd>(simd: S, dest: &mut [u8], covers: &[u8], alpha: u8, color: u32) {
    for (pixels, covers) in dest.chunks_exact_mut(32).zip(covers.chunks_exact(8)) {
        let cover = u32x8::from_slice(
            simd,
            &std::array::from_fn::<_, 8, _>(|i| u32::from(covers[i])),
        );
        blend_pixels(simd, pixels, div255(cover * u32::from(alpha)), color);
    }
}

pub(super) fn composite_solid(dest: &mut [u8], alpha: u8, color: u32) {
    dispatch!(Level::new(), simd => composite_solid_impl(simd, dest, alpha, color));
}

#[inline(always)]
fn composite_solid_impl<S: Simd>(simd: S, dest: &mut [u8], alpha: u8, color: u32) {
    let source_alpha = u32x8::splat(simd, u32::from(alpha));
    for pixels in dest.chunks_exact_mut(32) {
        blend_pixels(simd, pixels, source_alpha, color);
    }
}

#[inline(always)]
fn blend_pixels<S: Simd>(simd: S, pixels: &mut [u8], source_alpha: u32x8<S>, color: u32) {
    let input = u32x8::from_bytes(u8x32::from_slice(simd, pixels));
    let back_alpha = input >> 24;
    let output_alpha = back_alpha + source_alpha - div255(back_alpha * source_alpha);
    let ratio = if back_alpha.simd_eq(255).all_true() {
        source_alpha
    } else {
        // Both integer operands are exactly representable. A noninteger
        // quotient differs from an integer by >=1/255, much more than a
        // float's rounding error for results <=255. Real float division
        // followed by truncation therefore matches integer division here.
        // Zero-alpha lanes use a harmless divisor and preserve hidden RGB.
        u32x8::truncate_from(
            f32x8::float_from(source_alpha * 255) / f32x8::float_from(output_alpha.max(1)),
        )
    };
    let inverse = u32x8::splat(simd, 255) - ratio;
    let mut output = output_alpha << 24;
    for shift in [0, 8, 16] {
        let channel = (input >> shift) & 255;
        let source = (color >> shift) & 255;
        output |= div255(channel * inverse + ratio * source) << shift;
    }
    output.to_bytes().store_slice(pixels);
}
