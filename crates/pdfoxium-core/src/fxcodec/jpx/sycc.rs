// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Use of this source code is governed by the BSD-style license in
// upstream/pdfium/LICENSE.

//! Color helpers from core/fxcodec/jpx/cjpx_decoder.cpp. Plane allocations and
//! indexed rows replace the source's owned buffers and pointer increments.

use super::JpxColorSpace;
use hayro_jpeg2000::PdfiumComponent;

struct RgbData {
    r: Vec<f32>,
    g: Vec<f32>,
    b: Vec<f32>,
}

fn alloc_rgb(count: usize) -> Option<RgbData> {
    let plane = || {
        let mut data = Vec::new();
        data.try_reserve_exact(count).ok()?;
        data.resize(count, 0.0);
        Some(data)
    };
    Some(RgbData {
        r: plane()?,
        g: plane()?,
        b: plane()?,
    })
}

fn chroma_to_rgb(offset: i32, cb: f32, cr: f32) -> [i64; 3] {
    let cb = i64::from(cb as i32) - i64::from(offset);
    let cr = i64::from(cr as i32) - i64::from(offset);
    // Truncate the complete matrix contribution before adding/subtracting Y.
    // Chroma is shared by up to four pixels in the subsampled formats.
    [
        (1.402 * cr as f64) as i64,
        -((0.344 * cb as f64 + 0.714 * cr as f64) as i64),
        (1.772 * cb as f64) as i64,
    ]
}

fn add_luma(upper: i32, y: f32, chroma: [i64; 3]) -> [f32; 3] {
    let y = i64::from(y as i32);
    // Widened sums define native signed-overflow edges without Rust panics.
    chroma.map(|value| (y + value).clamp(0, i64::from(upper)) as f32)
}

fn sycc_to_rgb(offset: i32, upper: i32, y: f32, cb: f32, cr: f32) -> [f32; 3] {
    add_luma(upper, y, chroma_to_rgb(offset, cb, cr))
}

impl RgbData {
    fn store(&mut self, index: usize, pixel: [f32; 3]) {
        self.r[index] = pixel[0];
        self.g[index] = pixel[1];
        self.b[index] = pixel[2];
    }
    fn install(self, components: &mut [PdfiumComponent], expanded: bool) {
        let width = components[0].width;
        let height = components[0].height;
        let dx = components[0].dx;
        let dy = components[0].dy;
        components[0].samples = self.r;
        components[1].samples = self.g;
        components[2].samples = self.b;
        if expanded {
            for component in &mut components[1..3] {
                component.width = width;
                component.height = height;
                component.dx = dx;
                component.dy = dy;
            }
        }
    }
}

fn common_size_valid(components: &[PdfiumComponent]) -> bool {
    components[0].width != u32::MAX
        && components[0].width.div_ceil(2) == components[1].width
        && components[1].width == components[2].width
        && components[1].height == components[2].height
}

fn sizes_have_samples(components: &[PdfiumComponent]) -> bool {
    components[..3].iter().all(|component| {
        (component.width as usize)
            .checked_mul(component.height as usize)
            .is_some_and(|count| count != 0 && component.samples.len() >= count)
    })
}

fn bounds(precision: u8) -> (i32, i32) {
    (
        (1u32 << (precision - 1)) as i32,
        ((1u32 << precision) - 1) as i32,
    )
}

fn sycc444_to_rgb(components: &mut [PdfiumComponent]) {
    let precision = components[0].precision;
    if !(1..=30).contains(&precision) || !sizes_have_samples(components) {
        return;
    }
    let width = components[..3]
        .iter()
        .map(|component| component.width)
        .min()
        .unwrap();
    let height = components[..3]
        .iter()
        .map(|component| component.height)
        .min()
        .unwrap();
    let Some(count) = (width as usize).checked_mul(height as usize) else {
        return;
    };
    if count.checked_mul(4).is_none() {
        return;
    }
    let Some(mut rgb) = alloc_rgb(count) else {
        return;
    };
    let (offset, upper) = bounds(precision);
    // Native 4:4:4 conversion consumes a flat min-width/min-height sample span.
    for index in 0..count {
        rgb.store(
            index,
            sycc_to_rgb(
                offset,
                upper,
                components[0].samples[index],
                components[1].samples[index],
                components[2].samples[index],
            ),
        );
    }
    rgb.install(components, false);
}

fn sycc422_to_rgb(components: &mut [PdfiumComponent]) {
    let precision = components[0].precision;
    if !common_size_valid(components)
        || components[0].height != components[1].height
        || !(1..=31).contains(&precision)
        || !sizes_have_samples(components)
    {
        return;
    }
    let width = components[0].width as usize;
    let height = components[0].height as usize;
    let Some(count) = width.checked_mul(height) else {
        return;
    };
    if count.checked_mul(4).is_none() {
        return;
    }
    let Some(mut rgb) = alloc_rgb(count) else {
        return;
    };
    let (offset, upper) = bounds(precision);
    let chroma_width = components[1].width as usize;
    for row in 0..height {
        for column in (0..width & !1).step_by(2) {
            let chroma = row * chroma_width + column / 2;
            let contribution = chroma_to_rgb(
                offset,
                components[1].samples[chroma],
                components[2].samples[chroma],
            );
            for index in [row * width + column, row * width + column + 1] {
                rgb.store(
                    index,
                    add_luma(upper, components[0].samples[index], contribution),
                );
            }
        }
        if width & 1 != 0 {
            let index = row * width + width - 1;
            let chroma = row * chroma_width + width / 2;
            let contribution = chroma_to_rgb(
                offset,
                components[1].samples[chroma],
                components[2].samples[chroma],
            );
            rgb.store(
                index,
                add_luma(upper, components[0].samples[index], contribution),
            );
        }
    }
    rgb.install(components, true);
}

fn sycc420_to_rgb(components: &mut [PdfiumComponent]) {
    let precision = components[0].precision;
    if !common_size_valid(components)
        || components[0].height == u32::MAX
        || components[0].height.div_ceil(2) != components[1].height
        || !(1..=31).contains(&precision)
        || !sizes_have_samples(components)
    {
        return;
    }
    let width = components[0].width as usize;
    let height = components[0].height as usize;
    let chroma_width = components[1].width as usize;
    let chroma_height = components[1].height as usize;
    let Some(count) = width.checked_mul(height).and_then(|count| {
        // Native 4:2:0 uses FX_SAFE_UINT32 for its three allocations' size.
        u32::try_from(count).ok()?.checked_mul(4).map(|_| count)
    }) else {
        return;
    };
    let Some(mut rgb) = alloc_rgb(count) else {
        return;
    };
    let (offset, upper) = bounds(precision);
    let extend_width = width & 1 != 0 && chroma_width == width / 2;
    let extend_height = height & 1 != 0 && chroma_height == height / 2;
    let even_width = width & !1;
    let even_height = height & !1;
    for row in (0..even_height).step_by(2) {
        for column in (0..even_width).step_by(2) {
            let chroma = row / 2 * chroma_width + column / 2;
            let contribution = chroma_to_rgb(
                offset,
                components[1].samples[chroma],
                components[2].samples[chroma],
            );
            for index in [
                row * width + column,
                row * width + column + 1,
                (row + 1) * width + column,
                (row + 1) * width + column + 1,
            ] {
                rgb.store(
                    index,
                    add_luma(upper, components[0].samples[index], contribution),
                );
            }
        }
        if even_width < width {
            let chroma = row / 2 * chroma_width + even_width / 2 - usize::from(extend_width);
            let contribution = chroma_to_rgb(
                offset,
                components[1].samples[chroma],
                components[2].samples[chroma],
            );
            for index in [row * width + even_width, (row + 1) * width + even_width] {
                rgb.store(
                    index,
                    add_luma(upper, components[0].samples[index], contribution),
                );
            }
        }
    }
    if even_height < height {
        let chroma_row = even_height / 2 - usize::from(extend_height);
        for column in (0..even_width).step_by(2) {
            let chroma = chroma_row * chroma_width + column / 2;
            let contribution = chroma_to_rgb(
                offset,
                components[1].samples[chroma],
                components[2].samples[chroma],
            );
            for index in [
                even_height * width + column,
                even_height * width + column + 1,
            ] {
                rgb.store(
                    index,
                    add_luma(upper, components[0].samples[index], contribution),
                );
            }
        }
        if even_width < width {
            let index = even_height * width + even_width;
            let chroma = chroma_row * chroma_width + even_width / 2 - usize::from(extend_width);
            let contribution = chroma_to_rgb(
                offset,
                components[1].samples[chroma],
                components[2].samples[chroma],
            );
            rgb.store(
                index,
                add_luma(upper, components[0].samples[index], contribution),
            );
        }
    }
    rgb.install(components, true);
}

pub(super) fn start_decode_color(
    components: &mut [PdfiumComponent],
    colorspace: &mut JpxColorSpace,
) {
    if *colorspace != JpxColorSpace::Sycc
        && components.len() == 3
        && components[0].dx == components[0].dy
        && components[1].dx != 1
    {
        *colorspace = JpxColorSpace::Sycc;
    } else if components.len() <= 2 {
        *colorspace = JpxColorSpace::Gray;
    }
    if *colorspace != JpxColorSpace::Sycc {
        return;
    }
    if components.len() < 3 {
        *colorspace = JpxColorSpace::Gray;
        return;
    }
    let samplings = [
        (components[0].dx, components[0].dy),
        (components[1].dx, components[1].dy),
        (components[2].dx, components[2].dy),
    ];
    match samplings {
        [(1, 1), (2, 2), (2, 2)] => sycc420_to_rgb(components),
        [(1, 1), (2, 1), (2, 1)] => sycc422_to_rgb(components),
        [(1, 1), (1, 1), (1, 1)] => sycc444_to_rgb(components),
        _ => return,
    }
    *colorspace = JpxColorSpace::SRgb;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component(width: u32, height: u32, dx: u32, dy: u32, samples: Vec<f32>) -> PdfiumComponent {
        PdfiumComponent {
            width,
            height,
            dx,
            dy,
            precision: 8,
            signed: false,
            samples,
        }
    }

    #[test]
    fn subsampled_chroma_matches_per_pixel_matrix() {
        // Independent pre-optimization formula: check cast saturation,
        // truncation before luma, wide sums, clipping and odd chroma edges.
        let samples = [
            f32::NAN,
            f32::NEG_INFINITY,
            f32::INFINITY,
            i32::MIN as f32,
            i32::MAX as f32,
            -129.75,
            -0.75,
            0.0,
            0.75,
            1.0,
            127.9,
            128.0,
            129.0,
            255.75,
            65535.0,
        ];
        for dy in [1, 2] {
            for width in [1u32, 2, 3, 7, 8, 9, 15, 16, 17] {
                for height in [1u32, 2, 3, 5] {
                    for precision in 1..=31 {
                        let chroma_width = width.div_ceil(2);
                        let chroma_height = height.div_ceil(dy);
                        let plane = |count: u32, shift: usize| {
                            (0..count as usize)
                                .map(|index| samples[(index + shift) % samples.len()])
                                .collect::<Vec<_>>()
                        };
                        let mut planes = vec![
                            component(width, height, 1, 1, plane(width * height, 0)),
                            component(
                                chroma_width,
                                chroma_height,
                                2,
                                dy,
                                plane(chroma_width * chroma_height, 4),
                            ),
                            component(
                                chroma_width,
                                chroma_height,
                                2,
                                dy,
                                plane(chroma_width * chroma_height, 9),
                            ),
                        ];
                        for plane in &mut planes {
                            plane.precision = precision;
                        }
                        let offset = i64::from((1u32 << (precision - 1)) as i32);
                        let upper = i64::from(((1u32 << precision) - 1) as i32);
                        let mut expected = [Vec::new(), Vec::new(), Vec::new()];
                        for row in 0..height as usize {
                            for column in 0..width as usize {
                                let index = row * width as usize + column;
                                let chroma = row / dy as usize * chroma_width as usize + column / 2;
                                let y = i64::from(planes[0].samples[index] as i32);
                                let cb = i64::from(planes[1].samples[chroma] as i32) - offset;
                                let cr = i64::from(planes[2].samples[chroma] as i32) - offset;
                                let pixel = [
                                    y + (1.402 * cr as f64) as i64,
                                    y - (0.344 * cb as f64 + 0.714 * cr as f64) as i64,
                                    y + (1.772 * cb as f64) as i64,
                                ];
                                for (channel, value) in expected.iter_mut().zip(pixel) {
                                    channel.push(value.clamp(0, upper) as f32);
                                }
                            }
                        }
                        if dy == 1 {
                            sycc422_to_rgb(&mut planes);
                        } else {
                            sycc420_to_rgb(&mut planes);
                        }
                        for (plane, expected) in planes.iter().zip(expected) {
                            assert_eq!(
                                plane.samples, expected,
                                "{width}x{height}, dy={dy}, precision={precision}"
                            );
                            assert_eq!(
                                (plane.width, plane.height, plane.dx, plane.dy),
                                (width, height, 1, 1)
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn sycc420_preserves_native_size_acceptance_and_expands_chroma() {
        for width in [0, 1, 30, 31, 32, 33, 34, u32::MAX] {
            let count = if width == u32::MAX {
                0
            } else {
                width as usize * width as usize
            };
            let mut planes = vec![
                component(width, width, 1, 1, vec![100.0; count]),
                component(16, 16, 2, 2, vec![128.0; 256]),
                component(16, 16, 2, 2, vec![128.0; 256]),
            ];
            sycc420_to_rgb(&mut planes);
            assert_eq!(
                planes[1].width == width && planes[1].height == width,
                matches!(width, 31 | 32)
            );
            if matches!(width, 31 | 32) {
                assert!(planes.iter().all(|plane| plane.dx == 1
                    && plane.dy == 1
                    && plane.samples.iter().all(|&value| value == 100.0)));
            }
        }
    }

    #[test]
    fn sycc_odd_edges_keep_native_chroma_replication_and_matrix_truncation() {
        assert_eq!(
            sycc_to_rgb(128, 255, 100.0, 129.0, 127.0),
            [99.0, 100.0, 101.0]
        );
        for (height, dy, chroma, expected_blue) in [
            (1, 1, vec![128.0, 129.0], vec![100.0, 100.0, 101.0]),
            (
                3,
                2,
                vec![128.0, 129.0, 130.0, 131.0],
                vec![
                    100.0, 100.0, 101.0, 100.0, 100.0, 101.0, 103.0, 103.0, 105.0,
                ],
            ),
        ] {
            let chroma_height = (height as u32).div_ceil(dy);
            let mut planes = vec![
                component(3, height as u32, 1, 1, vec![100.0; 3 * height]),
                component(2, chroma_height, 2, dy, chroma),
                component(
                    2,
                    chroma_height,
                    2,
                    dy,
                    vec![128.0; 2 * chroma_height as usize],
                ),
            ];
            let mut colorspace = JpxColorSpace::Unknown;
            start_decode_color(&mut planes, &mut colorspace);
            assert_eq!(colorspace, JpxColorSpace::SRgb);
            assert_eq!(planes[2].samples, expected_blue);
            assert!(planes[0].samples.iter().all(|&value| value == 100.0));
        }
    }
}

#[cfg(test)]
#[path = "jpx_unittest.rs"]
mod upstream_tests;
