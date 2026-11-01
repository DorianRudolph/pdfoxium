// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Non-normal compositing is pixel-local: partitioning a row into individual
//! pixels must preserve the pinned blend, coverage, alpha and hidden-RGB rules.
//! The single-pixel route bypasses sparse-block detection and is an independent
//! partition oracle for its row-boundary decisions.

use super::*;

const MODES: [BlendMode; 15] = [
    BlendMode::Multiply,
    BlendMode::Screen,
    BlendMode::Overlay,
    BlendMode::Darken,
    BlendMode::Lighten,
    BlendMode::ColorDodge,
    BlendMode::ColorBurn,
    BlendMode::HardLight,
    BlendMode::SoftLight,
    BlendMode::Difference,
    BlendMode::Exclusion,
    BlendMode::Hue,
    BlendMode::Saturation,
    BlendMode::Color,
    BlendMode::Luminosity,
];

fn compare_partitioned_row(
    format: FxdibFormat,
    rgb: bool,
    mode: BlendMode,
    source: &[u8],
    background: &[u8],
    clip: Option<&[u8]>,
) {
    let width = source.len() / 4;
    let stride = get_bpp_from_format(format) as usize / 8;
    let compositor =
        CfxScanlineCompositor::new(format, FxdibFormat::Bgra, &[], 0, mode, rgb).unwrap();
    let mut actual = vec![0xa7; 3 + width * stride + 11];
    actual[3..3 + width * stride].copy_from_slice(background);
    let mut expected = actual.clone();
    let source_before = source.to_vec();
    let clip_before = clip.map(<[u8]>::to_vec);

    // Include the destination guard tail in both routes: width, not slice
    // length, controls which pixels may be touched.
    compositor.composite_rgb_bitmap_line(&mut actual[3..], source, width, clip);
    for col in 0..width {
        let one_clip = clip.filter(|c| !c.is_empty()).map(|c| &c[col..col + 1]);
        compositor.composite_rgb_bitmap_line(
            &mut expected[3 + col * stride..],
            &source[col * 4..col * 4 + 4],
            1,
            one_clip,
        );
    }
    assert_eq!(
        actual, expected,
        "format={format:?} rgb={rgb} mode={mode:?} width={width} clip={clip:?}"
    );
    assert_eq!(source, source_before);
    assert_eq!(clip, clip_before.as_deref());
}

#[test]
fn sparse_bgra_rows_match_pixel_partitioning_for_formats_blends_and_alpha_patterns() {
    for width in [0usize, 1, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 257] {
        for pattern in 0..6 {
            let mut source = vec![0u8; width * 4];
            for (col, pixel) in source.chunks_exact_mut(4).enumerate() {
                let alpha = match pattern {
                    0 | 1 => 0,
                    2 => [0, 0, 1, 127, 255, 0, 254, 0][col % 8],
                    3 => 255,
                    4 => {
                        if col + 1 == width {
                            127
                        } else {
                            0
                        }
                    }
                    _ => {
                        if col % 32 == 0 || col % 32 == 31 {
                            255
                        } else {
                            0
                        }
                    }
                };
                if pattern != 0 {
                    pixel.copy_from_slice(&[
                        (col * 73 + 19) as u8,
                        (col * 41 + 211) as u8,
                        (col * 137 + 67) as u8,
                        alpha,
                    ]);
                }
            }
            let clip_zero = vec![0; width];
            let clip_full = vec![255; width];
            let clip_mixed: Vec<u8> = (0..width)
                .map(|col| [0, 1, 127, 254, 255][col % 5])
                .collect();
            for format in [FxdibFormat::Bgr, FxdibFormat::Bgrx, FxdibFormat::Bgra] {
                let stride = get_bpp_from_format(format) as usize / 8;
                for alpha_pattern in 0..5 {
                    let mut background = vec![0; width * stride];
                    for (col, pixel) in background.chunks_exact_mut(stride).enumerate() {
                        pixel[..3].copy_from_slice(&[
                            (col * 29 + 211) as u8,
                            (col * 61 + 137) as u8,
                            (col * 43 + 29) as u8,
                        ]);
                        if stride == 4 {
                            pixel[3] = match alpha_pattern {
                                0 => 0,
                                1 => 1,
                                2 => 127,
                                3 => 255,
                                _ => [0, 1, 127, 255][col % 4],
                            };
                        }
                    }
                    for rgb in [false, true] {
                        for mode in MODES {
                            for clip in [
                                None,
                                Some(&[][..]),
                                Some(clip_zero.as_slice()),
                                Some(clip_full.as_slice()),
                                Some(clip_mixed.as_slice()),
                            ] {
                                compare_partitioned_row(
                                    format,
                                    rgb,
                                    mode,
                                    &source,
                                    &background,
                                    clip,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn sparse_bgra_single_active_pixels_cover_both_sides_of_block_boundaries() {
    let width = 257;
    for active in [
        0usize, 1, 7, 8, 15, 16, 17, 30, 31, 32, 33, 62, 63, 64, 65, 127, 128, 255, 256,
    ] {
        for source_alpha in [1u8, 127, 255] {
            let mut source = vec![0; width * 4];
            for (col, pixel) in source.chunks_exact_mut(4).enumerate() {
                // Hidden RGB remains nonzero throughout the transparent run.
                pixel.copy_from_slice(&[(col * 71 + 9) as u8, 37, 219, 0]);
            }
            source[active * 4 + 3] = source_alpha;
            let clip: Vec<u8> = (0..width).map(|col| (col * 73 + 19) as u8).collect();
            for format in [FxdibFormat::Bgr, FxdibFormat::Bgrx, FxdibFormat::Bgra] {
                let stride = get_bpp_from_format(format) as usize / 8;
                let mut background = vec![255; width * stride];
                for pixel in background.chunks_exact_mut(stride) {
                    pixel[..3].copy_from_slice(&[199, 103, 17]);
                }
                for rgb in [false, true] {
                    for mode in [BlendMode::Darken, BlendMode::Multiply, BlendMode::Hue] {
                        for clipping in [None, Some(clip.as_slice())] {
                            compare_partitioned_row(
                                format,
                                rgb,
                                mode,
                                &source,
                                &background,
                                clipping,
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn zero_alpha_source_preserves_hidden_rgb_copy_into_transparent_destinations() {
    let width = 65;
    for rgb in [false, true] {
        for mode in MODES {
            let compositor =
                CfxScanlineCompositor::new(FxdibFormat::Bgra, FxdibFormat::Bgra, &[], 0, mode, rgb)
                    .unwrap();
            let source: Vec<u8> = [37, 109, 213, 0].repeat(width);
            let mut background = [11, 23, 47, 0].repeat(width);
            // Include nonzero backdrop alpha in the same transparent block:
            // these pixels must retain their existing RGB instead of copying.
            for col in (1..width).step_by(2) {
                background[col * 4 + 3] = 127;
            }
            compositor.composite_rgb_bitmap_line(
                &mut background,
                &source,
                width,
                Some(&vec![0; width]),
            );
            for (col, pixel) in background.chunks_exact(4).enumerate() {
                if col % 2 == 0 {
                    assert_eq!(
                        pixel,
                        if rgb {
                            &[213, 109, 37, 0]
                        } else {
                            &[37, 109, 213, 0]
                        }
                    );
                } else {
                    assert_eq!(pixel, &[11, 23, 47, 127]);
                }
            }
        }
    }
}
