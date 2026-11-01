// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/cfx_substfont_unittest.cpp`.

#![allow(non_snake_case, non_upper_case_globals)]

use super::*;

#[test]
fn EffectiveSkew() {
    let mut font = CfxSubstFont::new();
    font.set_italic_angle(-12);
    assert_eq!(-21, font.effective_skew(false));

    font.set_subst_cjk(true);
    font.set_italic_cjk(true);
    assert_eq!(-27, font.effective_skew(true));
}

#[test]
fn EffectiveWeight() {
    let mut font = CfxSubstFont::new();
    font.set_weight(400);
    font.set_weight_cjk(700);
    font.set_subst_cjk(true);

    assert_eq!(400, font.effective_weight(false));
    assert_eq!(700, font.effective_weight(true));
}

#[test]
fn EmboldenLevels() {
    let mut font = CfxSubstFont::new();
    font.set_weight(700); // index = (700-400)/10 = 30

    assert_eq!(font.embolden_level_for_render(false, 1024, 0), 1);
    assert_eq!(font.embolden_level_for_load(), 70);

    // Test intermediate overflow handling (should not overflow final result).
    // 39 * (30000000 + 30000000) = 2,340,000,000 > INT_MAX (2,147,483,647)
    // 2,340,000,000 / 36655 = 63838
    assert_eq!(
        63838,
        font.embolden_level_for_render(false, 30000000, 30000000)
    );

    font.set_is_builtin_generic_font();
    assert_eq!(0, font.embolden_level_for_render(false, 1024, 0));
    assert_eq!(0, font.embolden_level_for_load());
}

#[test]
fn EstimatedStemV() {
    let mut font = CfxSubstFont::new();
    font.set_weight(700);
    assert_eq!(140, font.estimated_stem_v());
}

#[test]
fn IsActualFontLoaded() {
    let mut font = CfxSubstFont::new();
    font.set_family(b"Times New Roman");

    assert!(font.is_actual_font_loaded(b"timesnewroman,bold"));
    assert!(font.is_actual_font_loaded(b"timesnewromanps-bold"));
    assert!(!(font.is_actual_font_loaded(b"arial,bold")));
}
