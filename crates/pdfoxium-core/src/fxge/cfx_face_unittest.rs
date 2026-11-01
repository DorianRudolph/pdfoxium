// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Generic substitute regression against CFX_Face::AdjustVariationParams().
//! The upstream Fontations-only BuiltInGenericFontScaling test exercises the
//! horizontal-stretch fallback. Actual MM axes deliberately replace that policy;
//! widths outside the font's design range now clamp, as in native FreeType.

use super::CfxFace;
use crate::fxcrt::fx_coordinates::CfxMatrix;
use crate::fxge::cfx_standardfont::{get_generic_sans_font_data, get_generic_serif_font_data};
use crate::fxge::cfx_substfont::CfxSubstFont;
use crate::fxge::fx_font::FontAntiAliasingMode;
use crate::fxge::fx_fontencoding::FontEncoding;

#[test]
fn generic_variation_clamps_to_width_axis_instead_of_stretching() {
    let face = CfxFace::new(get_generic_sans_font_data(), 0).unwrap();
    let mut subst = CfxSubstFont::default();
    subst.set_is_builtin_generic_font();
    assert!(face.select_charmap(FontEncoding::AdobeCustom));
    let glyph = face.get_char_index(120);
    assert_eq!(glyph, 103);
    assert_eq!(face.glyph_width(glyph, 0, Some(&subst)), 530);
    assert_eq!(face.glyph_width(glyph, 1060, Some(&subst)), 778);
    assert_eq!(face.glyph_width(glyph, i32::MAX, Some(&subst)), 778);
    let normal = face.load_glyph_path(glyph, 0, false, Some(&subst)).unwrap();
    let wide = face
        .load_glyph_path(glyph, 1060, false, Some(&subst))
        .unwrap();
    assert!(wide.bounding_box().width() > normal.bounding_box().width());
    assert!(wide.bounding_box().width() < normal.bounding_box().width() * 1.8);
    assert_eq!(
        wide.points(),
        face.load_glyph_path(glyph, i32::MAX, false, Some(&subst))
            .unwrap()
            .points()
    );
    assert_eq!(
        normal.points(),
        face.load_glyph_path(glyph, 0, false, Some(&subst))
            .unwrap()
            .points()
    );
}

#[test]
fn generic_weight_changes_stems_in_paths_and_bitmaps_without_shared_face_state() {
    for (data, expected_widths) in [
        (get_generic_sans_font_data(), [699, 700, 700, 700, 700]),
        (get_generic_serif_font_data(), [700, 700, 778, 700, 827]),
    ] {
        let face = CfxFace::new(data, 0).unwrap();
        let glyph = face.name_index("H");
        assert!(glyph > 0);
        let mut regular = CfxSubstFont::default();
        regular.set_is_builtin_generic_font();
        regular.set_weight(300);
        let mut bold = regular.clone();
        bold.set_weight(700);
        let normal = face
            .load_glyph_path(glyph, 700, false, Some(&regular))
            .unwrap();
        let heavy = face
            .load_glyph_path(glyph, 700, false, Some(&bold))
            .unwrap();
        assert_ne!(normal.points(), heavy.points());
        let draw = |subst| {
            face.render_glyph(
                glyph,
                false,
                false,
                CfxMatrix::new(48.0, 0.0, 0.0, 48.0, 0.0, 0.0),
                700,
                FontAntiAliasingMode::Normal,
                Some(subst),
            )
            .unwrap()
        };
        let normal_bitmap = draw(&regular);
        let bold_bitmap = draw(&bold);
        let ink = |bitmap: &crate::fxge::cfx_glyphbitmap::CfxGlyphBitmap| {
            bitmap
                .bitmap()
                .buffer()
                .iter()
                .map(|&v| u64::from(v))
                .sum::<u64>()
        };
        assert!(ink(&bold_bitmap) > ink(&normal_bitmap) * 13 / 10);
        assert!(normal_bitmap
            .bitmap()
            .buffer()
            .iter()
            .any(|&v| v > 0 && v < 255));
        // Heavy serif masters cannot narrow H to 700 units: clamp at the
        // design endpoint instead of stretching the already bold outline.
        for (weight, expected) in [0, 300, 700, i32::MIN, i32::MAX]
            .into_iter()
            .zip(expected_widths)
        {
            let width = face.glyph_width_with_weight(glyph, 700, weight, Some(&regular));
            assert_eq!(width, expected, "weight={weight}");
        }
        assert_ne!(
            face.glyph_width_with_weight(glyph, 0, 300, Some(&regular)),
            face.glyph_width_with_weight(glyph, 0, 700, Some(&regular))
        );
        assert_eq!(
            draw(&regular).bitmap().buffer(),
            normal_bitmap.bitmap().buffer()
        );
        assert_eq!(
            face.load_glyph_path(glyph, 700, false, Some(&regular))
                .unwrap()
                .points(),
            normal.points()
        );
        assert!(face
            .load_glyph_path(u32::MAX, 700, false, Some(&bold))
            .is_none());
        assert_eq!(
            face.glyph_width_with_weight(u32::MAX, 700, 700, Some(&bold)),
            0
        );
        // Blank glyphs still have a valid instance advance.
        let space = face.name_index("space");
        assert!(face.glyph_width_with_weight(space, 700, 700, Some(&bold)) > 0);
    }
}
