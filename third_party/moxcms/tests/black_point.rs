// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: BSD-3-Clause

use moxcms::{
    ColorProfile, DataColorSpace, Layout, LutDataType, LutMultidimensionalType, LutStore, LutType,
    LutWarehouse, Matrix3d, ProfileClass, RenderingIntent, ToneReprCurve, TransformOptions,
    Vector3d,
};

// An original synthetic profile: a plateau forces direct stage evaluation.
// No external ICC profile or PDF is needed for these regression tests.
fn plateau_profile() -> ColorProfile {
    let mut profile = ColorProfile::default();
    profile.color_space = DataColorSpace::Rgb;
    profile.pcs = DataColorSpace::Xyz;
    profile.profile_class = ProfileClass::InputDevice;
    profile.white_point = ColorProfile::new_srgb().white_point;
    profile.media_white_point = Some(profile.white_point);
    let lut = LutWarehouse::Multidimensional(LutMultidimensionalType {
        num_input_channels: 3,
        num_output_channels: 3,
        grid_points: [0; 16],
        clut: None,
        a_curves: Vec::new(),
        b_curves: vec![ToneReprCurve::Lut(Vec::new()); 3],
        m_curves: vec![
            ToneReprCurve::Lut(
                (0..64)
                    .map(|i| if i < 40 {
                        327
                    } else {
                        327 + (65535 - 327) * (i - 39) / 24
                    })
                    .map(|v| v as u16)
                    .collect()
            );
            3
        ],
        matrix: Matrix3d {
            v: [[0.9642, 0., 0.], [0., 1., 0.], [0., 0., 0.8249]],
        },
        bias: Vector3d::default(),
    });
    profile.lut_a_to_b_perceptual = Some(lut.clone());
    profile.lut_a_to_b_colorimetric = Some(lut.clone());
    profile.lut_a_to_b_saturation = Some(lut);
    // Exercise the encoded profile/reader boundary, including ICC v4 selection.
    ColorProfile::new_from_slice(&profile.encode().unwrap()).unwrap()
}

fn lifted_cmyk_profile() -> ColorProfile {
    let mut profile = ColorProfile::default();
    profile.color_space = DataColorSpace::Cmyk;
    profile.pcs = DataColorSpace::Lab;
    profile.profile_class = ProfileClass::OutputDevice;
    profile.white_point = ColorProfile::new_srgb().white_point;
    profile.media_white_point = Some(profile.white_point);
    let mut clut = Vec::new();
    for c in 0..2 {
        for m in 0..2 {
            for y in 0..2 {
                for k in 0..2 {
                    let l = 100. - 78. * k as f64 - 10. * (c + m + y) as f64 / 3.;
                    clut.extend([
                        (l * 652.8).round() as u16,
                        ((128. + k as f64) * 256.).round() as u16,
                        ((128. + 0.5 * k as f64) * 256.).round() as u16,
                    ]);
                }
            }
        }
    }
    profile.lut_a_to_b_perceptual = Some(LutWarehouse::Lut(LutDataType {
        num_input_channels: 4,
        num_output_channels: 3,
        num_clut_grid_points: 2,
        matrix: Matrix3d::IDENTITY,
        num_input_table_entries: 2,
        num_output_table_entries: 2,
        input_table: LutStore::Store16([0, 65535].repeat(4)),
        output_table: LutStore::Store16([0, 65535].repeat(3)),
        clut_table: LutStore::Store16(clut),
        lut_type: LutType::Lut16,
    }));
    let mut encoded = profile.encode().unwrap();
    encoded[8..12].copy_from_slice(&0x02100000u32.to_be_bytes());
    ColorProfile::new_from_slice(&encoded).unwrap()
}

#[test]
fn v2_cmyk_perceptual_compensates_the_darkest_ink_endpoint() {
    let source = lifted_cmyk_profile();
    let destination = ColorProfile::new_srgb();
    for intent in [
        RenderingIntent::Perceptual,
        RenderingIntent::Saturation,
        RenderingIntent::RelativeColorimetric,
        RenderingIntent::AbsoluteColorimetric,
    ] {
        let transform = source
            .create_transform_8bit(
                Layout::Rgba,
                &destination,
                Layout::Rgb,
                TransformOptions {
                    rendering_intent: intent,
                    allow_use_cicp_transfer: false,
                    ..Default::default()
                },
            )
            .unwrap();
        let mut rgb = [0; 3];
        transform.transform(&[0, 0, 0, 255], &mut rgb).unwrap();
        // CIE Lab endpoints (12,1,.5) and (22,1,.5), D50 black/white
        // compensation, Bradford adaptation and sRGB. Keep endpoint chroma.
        let compensated = matches!(
            intent,
            RenderingIntent::Perceptual | RenderingIntent::Saturation
        );
        let expected = if compensated {
            [41u8, 40, 40]
        } else {
            [54, 52, 52]
        };
        for (actual, expected) in rgb.into_iter().zip(expected) {
            assert!(
                actual.abs_diff(expected) <= 1,
                "{intent:?}: {rgb:?}, expected {expected}"
            );
        }
        if compensated {
            transform.transform(&[255; 4], &mut rgb).unwrap();
            assert!(rgb.iter().all(|&v| v <= 1), "darkest ink: {rgb:?}");
        }
        transform.transform(&[0; 4], &mut rgb).unwrap();
        assert_eq!(rgb, [255; 3]);
    }
}

#[test]
fn v4_cmyk_uses_the_specified_perceptual_black() {
    let mut encoded = lifted_cmyk_profile().encode().unwrap();
    encoded[8..12].copy_from_slice(&0x04300000u32.to_be_bytes());
    let source = ColorProfile::new_from_slice(&encoded).unwrap();
    let transform = source
        .create_transform_8bit(
            Layout::Rgba,
            &ColorProfile::new_srgb(),
            Layout::Rgb,
            TransformOptions {
                allow_use_cicp_transfer: false,
                ..Default::default()
            },
        )
        .unwrap();
    let mut rgb = [0; 3];
    transform.transform(&[0, 0, 0, 255], &mut rgb).unwrap();
    // Independently map Lab (22,1,.5) using ICC v4's fixed perceptual black,
    // rather than the v2 profile's measured all-ink endpoint (12,1,.5).
    for (actual, expected) in rgb.into_iter().zip([52u8, 49, 49]) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "{rgb:?}, expected {expected}"
        );
    }
}

#[test]
fn gamma_rounds_integer_output_and_retains_float_precision() {
    let profile = ColorProfile::new_srgb();
    let bytes = profile
        .build_gamma_table::<u8, 65536, 65536, 8>(&profile.green_trc, false)
        .unwrap();
    let floats = profile
        .build_gamma_table::<f32, 65536, 65536, 8>(&profile.green_trc, false)
        .unwrap();
    // Standard sRGB: linear 99/65535 encodes to approximately 4.977/255.
    assert_eq!(bytes[99], 5);
    assert!((4.9..5.).contains(&(floats[99] * 255.)));
    assert_eq!((bytes[0], bytes[65535]), (0, 255));
}

#[test]
#[cfg(feature = "any_to_any")]
fn direct_pipeline_compensates_perceptual_black_but_not_colorimetric() {
    let source = plateau_profile();
    let destination = ColorProfile::new_srgb();
    for intent in [
        RenderingIntent::Perceptual,
        RenderingIntent::Saturation,
        RenderingIntent::RelativeColorimetric,
        RenderingIntent::AbsoluteColorimetric,
    ] {
        let options = TransformOptions {
            rendering_intent: intent,
            allow_use_cicp_transfer: false,
            ..Default::default()
        };
        let transform = source
            .create_transform_8bit(Layout::Rgb, &destination, Layout::Rgb, options)
            .unwrap();
        let mut rgb = [0; 3];
        transform.transform(&[64; 3], &mut rgb).unwrap();
        // Independently evaluating the plateau, XYZ scale, black-point mapping
        // and sRGB transfer gives 19 with compensation, 25 without it.
        let expected = if matches!(
            intent,
            RenderingIntent::Perceptual | RenderingIntent::Saturation
        ) {
            19u8
        } else {
            25
        };
        assert!(
            rgb.iter().all(|&v| v.abs_diff(expected) <= 1),
            "{intent:?}: {rgb:?}, expected {expected}"
        );
    }
}
