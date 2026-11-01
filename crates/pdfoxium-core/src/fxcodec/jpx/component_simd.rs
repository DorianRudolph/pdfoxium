// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! Exact JPX sample conversion for CJPX_Decoder::Decode's planar loop.

use fearless_simd::{dispatch, f32x8, i32x8, prelude::*, Level};

pub(super) fn store(
    component: &hayro_jpeg2000::PdfiumComponent,
    target: &mut [u8],
    width: usize,
    height: usize,
    pitch: usize,
    channels: usize,
    channel: usize,
) {
    assert!(channel < channels);
    dispatch!(Level::new(), simd => store_impl(simd, component, target, width, height, pitch, channels, channel));
}

#[inline(always)]
fn store_impl<S: Simd>(
    simd: S,
    component: &hayro_jpeg2000::PdfiumComponent,
    target: &mut [u8],
    width: usize,
    height: usize,
    pitch: usize,
    channels: usize,
    channel: usize,
) {
    // Select precision once, retaining the native adapter's separate loops.
    match component.precision.cmp(&8) {
        std::cmp::Ordering::Less => {
            let shift = u32::from(8 - component.precision);
            store_converted(
                simd,
                component,
                target,
                width,
                height,
                pitch,
                channels,
                channel,
                |v| v << shift,
            );
        }
        std::cmp::Ordering::Equal => {
            store_converted(
                simd,
                component,
                target,
                width,
                height,
                pitch,
                channels,
                channel,
                |v| v,
            );
        }
        std::cmp::Ordering::Greater => {
            let shift = u32::from(component.precision - 8);
            store_converted(
                simd,
                component,
                target,
                width,
                height,
                pitch,
                channels,
                channel,
                |v| {
                    // For negative values, either sign of the rounding bit still
                    // clamps to zero. Nonnegative values have `% 2 == & 1`.
                    ((v >> shift) + ((v >> (shift - 1)) & 1)).max(0).min(255)
                },
            );
        }
    }
}

#[inline(always)]
fn store_converted<S: Simd>(
    simd: S,
    component: &hayro_jpeg2000::PdfiumComponent,
    target: &mut [u8],
    width: usize,
    height: usize,
    pitch: usize,
    channels: usize,
    channel: usize,
    convert: impl Fn(i32x8<S>) -> i32x8<S>,
) {
    let offset = if component.signed {
        1i32 << (component.precision - 1)
    } else {
        0
    };
    let bias = i32x8::splat(simd, offset);
    for row in 0..height {
        let samples = &component.samples[row * width..(row + 1) * width];
        let output = &mut target[row * pitch..row * pitch + width * channels];
        let full = width / 8 * 8;
        for (source, dest) in samples[..full]
            .as_chunks::<8>()
            .0
            .iter()
            .zip(output[..full * channels].chunks_exact_mut(8 * channels))
        {
            // Match `as i32` exactly, including saturation and NaN -> 0,
            // then wrap the signed bias before the precision conversion.
            let converted =
                convert(i32x8::truncate_from_precise(f32x8::from_slice(simd, source)) + bias);
            let mut values = [0i32; 8];
            converted.store_slice(&mut values);
            for (pixel, value) in dest.chunks_exact_mut(channels).zip(values) {
                pixel[channel] = value as u8;
            }
        }
        for (pixel, &sample) in output[full * channels..]
            .chunks_exact_mut(channels)
            .zip(&samples[full..])
        {
            let value = (sample as i32).wrapping_add(offset);
            pixel[channel] = if component.precision < 8 {
                value.wrapping_shl(u32::from(8 - component.precision)) as u8
            } else if component.precision == 8 {
                value as u8
            } else {
                let shift = component.precision - 8;
                ((value >> shift) + ((value >> (shift - 1)) % 2)).clamp(0, 255) as u8
            };
        }
    }
}
