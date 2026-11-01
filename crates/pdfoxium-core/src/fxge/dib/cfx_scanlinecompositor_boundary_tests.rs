// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use super::*;

fn compositor(
    dest: FxdibFormat,
    src: FxdibFormat,
    mode: BlendMode,
    rgb: bool,
    color: u32,
) -> CfxScanlineCompositor {
    CfxScanlineCompositor::new(dest, src, &[], color, mode, rgb).unwrap()
}

#[test]
fn non_normal_rgb_and_bgra_sources_accumulate_only_coverage_into_an_alpha_mask() {
    let mut mask = [17, 64, 255, 219];
    let rgba = compositor(
        FxdibFormat::EightBppMask,
        FxdibFormat::Bgra,
        BlendMode::Multiply,
        false,
        0,
    );
    rgba.composite_rgb_bitmap_line(
        &mut mask,
        &[231, 17, 91, 0, 31, 211, 57, 128, 9, 27, 123, 255],
        3,
        Some(&[255, 128, 0]),
    );
    assert_eq!(mask, [17, 112, 255, 219]);
    let rgb = compositor(
        FxdibFormat::EightBppMask,
        FxdibFormat::Bgr,
        BlendMode::Screen,
        false,
        0,
    );
    mask = [17, 64, 127, 219];
    rgb.composite_rgb_bitmap_line(
        &mut mask,
        &[231, 17, 91, 31, 211, 57, 9, 27, 123],
        3,
        Some(&[0, 128, 255]),
    );
    assert_eq!(mask, [17, 160, 255, 219]);
    rgb.composite_rgb_bitmap_line(&mut mask, &[1, 2, 3, 4, 5, 6, 7, 8, 9], 3, None);
    assert_eq!(mask, [255, 255, 255, 219]);
}

#[test]
fn nonseparable_gray_destinations_distinguish_luminosity_from_chromatic_modes() {
    for mode in [
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ] {
        let c = compositor(FxdibFormat::EightBppRgb, FxdibFormat::Bgra, mode, false, 0);
        let mut row = [100, 100, 100, 219];
        // Blue has luminance28 in the PDFium integer RGB-to-gray contract.
        c.composite_rgb_bitmap_line(
            &mut row,
            &[255, 0, 0, 255, 255, 0, 0, 128, 255, 0, 0, 255],
            3,
            Some(&[0, 255, 255]),
        );
        assert_eq!(
            row,
            if mode == BlendMode::Luminosity {
                [100, 63, 28, 219]
            } else {
                [100, 100, 100, 219]
            },
            "{mode:?}"
        );
        let c = compositor(FxdibFormat::EightBppRgb, FxdibFormat::Bgr, mode, false, 0);
        c.composite_rgb_bitmap_line(&mut row, &[255, 0, 0], 1, None);
        assert_eq!(
            row[0],
            if mode == BlendMode::Luminosity {
                28
            } else {
                100
            }
        );
    }
}

#[test]
fn one_bit_palette_gray_blending_starts_at_the_requested_bit_not_the_first_byte() {
    let c = compositor(
        FxdibFormat::EightBppRgb,
        FxdibFormat::OneBppRgb,
        BlendMode::Multiply,
        false,
        0,
    );
    let mut row = [120, 120, 120, 120, 120, 120, 219];
    c.composite_pal_bitmap_line(
        &mut row,
        &[0b0001_0101, 0b1000_0000],
        3,
        6,
        Some(&[255, 255, 128, 128, 0, 255]),
    );
    assert_eq!(row, [120, 0, 120, 59, 120, 120, 219]);
}

#[test]
fn bit_masks_cross_a_byte_boundary_and_keep_zero_bits_and_zero_clip_unchanged() {
    for rgb in [false, true] {
        for format in [FxdibFormat::Bgr, FxdibFormat::Bgrx] {
            let bytes = if format == FxdibFormat::Bgr { 3 } else { 4 };
            let red = if rgb {
                [255, 0, 0, 219]
            } else {
                [0, 0, 255, 219]
            };
            let mut row = vec![197; 6 * bytes + 2];
            for p in row[1..1 + 6 * bytes].chunks_exact_mut(bytes) {
                p.copy_from_slice(&red[..bytes]);
            }
            let c = compositor(
                format,
                FxdibFormat::OneBppMask,
                BlendMode::Multiply,
                rgb,
                0xff0000ff,
            );
            c.composite_bit_mask_line(
                &mut row[1..1 + 6 * bytes],
                &[0b0001_0101, 0b1000_0000],
                3,
                6,
                Some(&[255, 255, 128, 128, 0, 255]),
            );
            for (i, p) in row[1..1 + 6 * bytes].chunks_exact(bytes).enumerate() {
                let r = [0, 255, 127, 255, 255, 0][i];
                assert_eq!(
                    &p[..3],
                    &(if rgb { [r, 0, 0] } else { [0, 0, r] }),
                    "{rgb} {format:?} {i}"
                );
                if bytes == 4 {
                    assert_eq!(p[3], 219);
                }
            }
            assert_eq!(row[0], 197);
            assert_eq!(row[6 * bytes + 1], 197);
        }
    }
}

#[test]
fn clipped_byte_masks_over_a_transparent_backdrop_keep_unassociated_source_color() {
    for rgb in [false, true] {
        for mode in [
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Hue,
            BlendMode::Saturation,
            BlendMode::Color,
            BlendMode::Luminosity,
        ] {
            let c = compositor(
                FxdibFormat::Bgra,
                FxdibFormat::EightBppMask,
                mode,
                rgb,
                0x8000ff00,
            );
            let mut row = [91, 17, 23, 0, 91, 17, 23, 0, 91, 17, 23, 0, 219];
            c.composite_byte_mask_line(&mut row, &[0, 128, 255], 3, Some(&[255, 128, 255]));
            // RGB at alpha0 is unobservable; it deliberately has no color assertion.
            assert_eq!(row[3], 0);
            assert_eq!(&row[4..12], &[0, 255, 0, 32, 0, 255, 0, 128]);
            assert_eq!(row[12], 219);
        }
    }
}

#[test]
fn zero_coverage_byte_masks_cannot_change_a_nontransparent_backdrop() {
    for mode in [
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Hue,
        BlendMode::Luminosity,
    ] {
        let c = compositor(
            FxdibFormat::Bgra,
            FxdibFormat::EightBppMask,
            mode,
            false,
            0x8011dd77,
        );
        let mut row = [71, 92, 123, 64, 71, 92, 123, 255, 219];
        c.composite_byte_mask_line(&mut row, &[0, 255], 2, Some(&[255, 0]));
        assert_eq!(row, [71, 92, 123, 64, 71, 92, 123, 255, 219]);
    }
}

fn reference(source: [u8; 4], back: [u8; 4], clip: u8, mode: BlendMode) -> [f64; 4] {
    // Independent PDF source-over equation in premultiplied blend-space components.
    // Quantize source coverage once at the documented 8-bit mask boundary.
    let a = f64::from(u16::from(source[3]) * u16::from(clip) / 255) / 255.;
    let b = f64::from(back[3]) / 255.;
    let out = a + b - a * b;
    let mut result = [0.; 4];
    result[3] = out * 255.;
    if out == 0. {
        return result;
    }
    for i in 0..3 {
        let s = f64::from(source[i]) / 255.;
        let d = f64::from(back[i]) / 255.;
        let blend = match mode {
            BlendMode::Normal => s,
            BlendMode::Multiply => s * d,
            BlendMode::Screen => s + d - s * d,
            _ => unreachable!(),
        };
        result[i] = 255. * ((1. - a) * b * d + (1. - b) * a * s + a * b * blend) / out;
    }
    result
}

#[test]
fn bgra_partial_alpha_composition_matches_independent_porter_duff_reference() {
    for mode in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen] {
        for rgb in [false, true] {
            let c = compositor(FxdibFormat::Bgra, FxdibFormat::Bgra, mode, rgb, 0);
            for sa in [0, 1, 64, 128, 254, 255] {
                for ba in [0, 1, 64, 128, 254, 255] {
                    for clip in [0, 128, 255] {
                        let source = [231, 57, 19, sa];
                        let back = [23, 91, 201, ba];
                        let mut row = if rgb {
                            [201, 91, 23, ba, 219]
                        } else {
                            [23, 91, 201, ba, 219]
                        };
                        c.composite_rgb_bitmap_line(&mut row, &source, 1, Some(&[clip]));
                        let expected = reference(source, back, clip, mode);
                        // Blend, intermediate color and output ratio each quantize to
                        // 8 bits; their cumulative component error is at most three.
                        assert!(
                            (f64::from(row[3]) - expected[3]).abs() <= 1.00001,
                            "alpha {mode:?} {sa} {ba} {clip}"
                        );
                        if expected[3] > 0. {
                            for i in 0..3 {
                                let actual = row[if rgb { 2 - i } else { i }];
                                assert!((f64::from(actual)-expected[i]).abs()<=3.00001,"channel{i} {mode:?} source{sa} back{ba} clip{clip} rgb{rgb}: {row:?} vs {expected:?}");
                            }
                        }
                        assert_eq!(row[4], 219);
                    }
                }
            }
        }
    }
}
