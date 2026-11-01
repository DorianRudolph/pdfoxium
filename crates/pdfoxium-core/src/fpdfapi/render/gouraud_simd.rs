// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! Parallel RGB channel arithmetic for DrawGouraud's direct-color spans.

use crate::fxge::dib::fx_dib::argb_encode;
use fearless_simd::{dispatch, f32x4, i32x4, prelude::*, Level};

pub(super) fn fill(dest: &mut [u8], rgb: [f32; 3], step: [f32; 3], alpha: i32) {
    dispatch!(Level::new(), simd => fill_impl(simd, dest, rgb, step, alpha));
}

#[inline(always)]
fn fill_impl<S: Simd>(simd: S, dest: &mut [u8], rgb: [f32; 3], step: [f32; 3], alpha: i32) {
    let mut value = f32x4::from_slice(simd, &[rgb[0], rgb[1], rgb[2], 0.0]);
    let step = f32x4::from_slice(simd, &[step[0], step[1], step[2], 0.0]);
    for pixel in dest.chunks_exact_mut(4) {
        // Advance channels together, but retain every per-pixel addition.
        // Computing value + n * step would change accumulated rounding.
        value += step;
        let mut channels = [0i32; 4];
        i32x4::truncate_from_precise(value * 255.0).store_slice(&mut channels);
        let color = argb_encode(
            alpha as u32,
            channels[0] as u32,
            channels[1] as u32,
            channels[2] as u32,
        );
        pixel.copy_from_slice(&color.to_le_bytes());
    }
}
