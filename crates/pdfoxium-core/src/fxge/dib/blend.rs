// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxge/dib/blend.{h,cpp}`.

use super::fx_dib::BlendMode;

const COLOR_SQRT: [u8; 256] = [
    0x00, 0x03, 0x07, 0x0B, 0x0F, 0x12, 0x16, 0x19, 0x1D, 0x20, 0x23, 0x26, 0x29, 0x2C, 0x2F, 0x32,
    0x35, 0x37, 0x3A, 0x3C, 0x3F, 0x41, 0x43, 0x46, 0x48, 0x4A, 0x4C, 0x4E, 0x50, 0x52, 0x54, 0x56,
    0x57, 0x59, 0x5B, 0x5C, 0x5E, 0x60, 0x61, 0x63, 0x64, 0x65, 0x67, 0x68, 0x69, 0x6B, 0x6C, 0x6D,
    0x6E, 0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E,
    0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x87, 0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D, 0x8E,
    0x8F, 0x90, 0x91, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x97, 0x98, 0x99, 0x9A, 0x9B, 0x9C,
    0x9C, 0x9D, 0x9E, 0x9F, 0xA0, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA4, 0xA5, 0xA6, 0xA7, 0xA7, 0xA8,
    0xA9, 0xAA, 0xAA, 0xAB, 0xAC, 0xAD, 0xAD, 0xAE, 0xAF, 0xB0, 0xB0, 0xB1, 0xB2, 0xB3, 0xB3, 0xB4,
    0xB5, 0xB5, 0xB6, 0xB7, 0xB7, 0xB8, 0xB9, 0xBA, 0xBA, 0xBB, 0xBC, 0xBC, 0xBD, 0xBE, 0xBE, 0xBF,
    0xC0, 0xC0, 0xC1, 0xC2, 0xC2, 0xC3, 0xC4, 0xC4, 0xC5, 0xC6, 0xC6, 0xC7, 0xC7, 0xC8, 0xC9, 0xC9,
    0xCA, 0xCB, 0xCB, 0xCC, 0xCC, 0xCD, 0xCE, 0xCE, 0xCF, 0xD0, 0xD0, 0xD1, 0xD1, 0xD2, 0xD3, 0xD3,
    0xD4, 0xD4, 0xD5, 0xD6, 0xD6, 0xD7, 0xD7, 0xD8, 0xD9, 0xD9, 0xDA, 0xDA, 0xDB, 0xDC, 0xDC, 0xDD,
    0xDD, 0xDE, 0xDE, 0xDF, 0xE0, 0xE0, 0xE1, 0xE1, 0xE2, 0xE2, 0xE3, 0xE4, 0xE4, 0xE5, 0xE5, 0xE6,
    0xE6, 0xE7, 0xE7, 0xE8, 0xE9, 0xE9, 0xEA, 0xEA, 0xEB, 0xEB, 0xEC, 0xEC, 0xED, 0xED, 0xEE, 0xEE,
    0xEF, 0xF0, 0xF0, 0xF1, 0xF1, 0xF2, 0xF2, 0xF3, 0xF3, 0xF4, 0xF4, 0xF5, 0xF5, 0xF6, 0xF6, 0xF7,
    0xF7, 0xF8, 0xF8, 0xF9, 0xF9, 0xFA, 0xFA, 0xFB, 0xFB, 0xFC, 0xFC, 0xFD, 0xFD, 0xFE, 0xFE, 0xFF,
];

/// Handles the separable blend modes used per color channel.
pub fn blend(mode: BlendMode, backdrop: i32, source: i32) -> i32 {
    debug_assert!((0..=255).contains(&backdrop));
    debug_assert!((0..=255).contains(&source));
    match mode {
        BlendMode::Normal => source,
        BlendMode::Multiply => source * backdrop / 255,
        BlendMode::Screen => source + backdrop - source * backdrop / 255,
        BlendMode::Overlay => blend(BlendMode::HardLight, source, backdrop),
        BlendMode::Darken => source.min(backdrop),
        BlendMode::Lighten => source.max(backdrop),
        BlendMode::ColorDodge => {
            if source == 255 {
                255
            } else {
                (backdrop * 255 / (255 - source)).min(255)
            }
        }
        BlendMode::ColorBurn => {
            if source == 0 {
                0
            } else {
                255 - ((255 - backdrop) * 255 / source).min(255)
            }
        }
        BlendMode::HardLight => {
            if source < 128 {
                source * backdrop * 2 / 255
            } else {
                blend(BlendMode::Screen, backdrop, 2 * source - 255)
            }
        }
        BlendMode::SoftLight => {
            if source < 128 {
                backdrop - (255 - 2 * source) * backdrop * (255 - backdrop) / 255 / 255
            } else {
                backdrop
                    + (2 * source - 255) * (i32::from(COLOR_SQRT[backdrop as usize]) - backdrop)
                        / 255
            }
        }
        BlendMode::Difference => (backdrop - source).abs(),
        BlendMode::Exclusion => backdrop + source - 2 * backdrop * source / 255,
        BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity => {
            unreachable!("nonseparable blend mode passed to Blend")
        }
    }
}

/// PDF's nonseparable modes operate on RGB triples so that hue, saturation,
/// and luminosity can be borrowed independently from either color.
pub fn blend_rgb(mode: BlendMode, backdrop: [u8; 3], source: [u8; 3]) -> [u8; 3] {
    if mode as i32 <= BlendMode::Exclusion as i32 {
        return std::array::from_fn(|i| blend(mode, backdrop[i] as i32, source[i] as i32) as u8);
    }
    super::cfx_scanlinecompositor::rgb_blend(mode, source.map(i32::from), backdrop.map(i32::from))
        .map(|value| value as u8)
}

#[cfg(test)]
mod tests {
    use super::{blend, blend_rgb, BlendMode, COLOR_SQRT};

    #[test]
    fn source_table_and_separable_modes() {
        assert_eq!(
            (COLOR_SQRT[0], COLOR_SQRT[64], COLOR_SQRT[255]),
            (0, 128, 255)
        );
        assert_eq!(blend(BlendMode::Normal, 80, 100), 100);
        assert_eq!(blend(BlendMode::Multiply, 80, 100), 31);
        assert_eq!(blend(BlendMode::Screen, 80, 100), 149);
        assert_eq!(blend(BlendMode::ColorDodge, 80, 255), 255);
        assert_eq!(blend(BlendMode::ColorBurn, 80, 0), 0);
        assert_eq!(blend(BlendMode::SoftLight, 64, 255), 128);
        assert_eq!(blend(BlendMode::Difference, 80, 100), 20);
        assert_eq!(blend(BlendMode::Exclusion, 80, 100), 118);
    }

    #[test]
    fn nonseparable_modes_preserve_pdf_color_attributes() {
        let backdrop = [180, 100, 50];
        let source = [40, 130, 200];
        let lum = |color: [u8; 3]| {
            0.3 * f32::from(color[0]) + 0.59 * f32::from(color[1]) + 0.11 * f32::from(color[2])
        };
        for mode in [BlendMode::Hue, BlendMode::Saturation, BlendMode::Color] {
            let result = blend_rgb(mode, backdrop, source);
            assert!((lum(result) - lum(backdrop)).abs() < 1.0);
        }
        let color = blend_rgb(BlendMode::Color, backdrop, source);
        assert!(color[2] > color[1] && color[1] > color[0]);
        let luminosity = blend_rgb(BlendMode::Luminosity, backdrop, source);
        assert!((lum(luminosity) - lum(source)).abs() < 1.0);
        assert!(luminosity[0] > luminosity[1] && luminosity[1] > luminosity[2]);
    }
}

#[cfg(test)]
#[path = "blend_unittest.rs"]
mod upstream_tests;
