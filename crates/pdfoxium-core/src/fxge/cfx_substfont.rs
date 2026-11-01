// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Non-Skia, non-Apple `core/fxge/cfx_substfont.{h,cpp}`.

use crate::fxge::fx_font::{FONT_WEIGHT_BOLD, FONT_WEIGHT_NORMAL};

// FX_Charset values consumed by CFX_SubstFont. Other values remain valid u8s.
pub const CHARSET_ANSI: u8 = 0;
pub const CHARSET_SYMBOL: u8 = 2;
pub const CHARSET_SHIFT_JIS: u8 = 128;

const WEIGHT_POW: [u8; 100] = [
    0, 6, 12, 14, 16, 18, 22, 24, 28, 30, 32, 34, 36, 38, 40, 42, 44, 46, 48, 50, 52, 54, 56, 58,
    60, 62, 64, 66, 68, 70, 70, 72, 72, 74, 74, 74, 76, 76, 76, 78, 78, 78, 80, 80, 80, 82, 82, 82,
    84, 84, 84, 84, 86, 86, 86, 88, 88, 88, 88, 90, 90, 90, 90, 92, 92, 92, 92, 94, 94, 94, 94, 96,
    96, 96, 96, 96, 98, 98, 98, 98, 100, 100, 100, 100, 100, 102, 102, 102, 102, 102, 104, 104,
    104, 104, 104, 106, 106, 106, 106, 106,
];
const WEIGHT_POW_11: [u8; 100] = [
    0, 4, 7, 8, 9, 10, 12, 13, 15, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 28, 29, 30, 31, 32, 33,
    34, 35, 36, 37, 39, 39, 40, 40, 41, 41, 41, 42, 42, 42, 43, 43, 43, 44, 44, 44, 45, 45, 45, 46,
    46, 46, 46, 43, 47, 47, 48, 48, 48, 48, 45, 50, 50, 50, 46, 51, 51, 51, 52, 52, 52, 52, 53, 53,
    53, 53, 53, 54, 54, 54, 54, 55, 55, 55, 55, 55, 56, 56, 56, 56, 56, 57, 57, 57, 57, 57, 58, 58,
    58, 58, 58,
];
const WEIGHT_POW_SHIFT_JIS: [u8; 100] = [
    0, 0, 2, 4, 6, 8, 10, 14, 16, 20, 22, 26, 28, 32, 34, 38, 42, 44, 48, 52, 56, 60, 64, 66, 70,
    74, 78, 82, 86, 90, 96, 96, 96, 96, 98, 98, 98, 100, 100, 100, 100, 102, 102, 102, 102, 104,
    104, 104, 104, 104, 106, 106, 106, 106, 106, 108, 108, 108, 108, 108, 110, 110, 110, 110, 110,
    112, 112, 112, 112, 112, 112, 114, 114, 114, 114, 114, 114, 114, 116, 116, 116, 116, 116, 116,
    116, 118, 118, 118, 118, 118, 118, 118, 120, 120, 120, 120, 120, 120, 120, 120,
];
const ANGLE_SKEW: [i8; 30] = [
    0, -2, -3, -5, -7, -9, -11, -12, -14, -16, -18, -19, -21, -23, -25, -27, -29, -31, -32, -34,
    -36, -38, -40, -42, -45, -47, -49, -51, -53, -55,
];

fn skew_from_angle(angle: i32) -> i32 {
    usize::try_from(angle.saturating_neg())
        .ok()
        .and_then(|index| ANGLE_SKEW.get(index))
        .map_or(-58, |value| i32::from(*value))
}

#[derive(Clone, Debug)]
pub struct CfxSubstFont {
    family: Vec<u8>,
    charset: u8,
    weight: i32,
    italic_angle: i32,
    weight_cjk: i32,
    subst_cjk: bool,
    italic_cjk: bool,
    built_in_generic: bool,
}

impl Default for CfxSubstFont {
    fn default() -> Self {
        Self {
            family: Vec::new(),
            charset: CHARSET_ANSI,
            weight: 0,
            italic_angle: 0,
            weight_cjk: 0,
            subst_cjk: false,
            italic_cjk: false,
            built_in_generic: false,
        }
    }
}

impl CfxSubstFont {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn family(&self) -> &[u8] {
        &self.family
    }
    pub fn charset(&self) -> u8 {
        self.charset
    }
    pub fn weight(&self) -> i32 {
        self.weight
    }
    pub fn italic_angle(&self) -> i32 {
        self.italic_angle
    }
    pub fn is_symbolic(&self) -> bool {
        self.charset == CHARSET_SYMBOL
    }
    pub fn is_bold(&self) -> bool {
        self.weight == i32::from(FONT_WEIGHT_BOLD)
    }
    pub fn is_builtin_generic_font(&self) -> bool {
        self.built_in_generic
    }
    pub fn estimated_stem_v(&self) -> i32 {
        self.weight / 5
    }
    pub fn skew(&self) -> i32 {
        skew_from_angle(self.italic_angle)
    }
    pub fn effective_skew(&self, is_cid_font: bool) -> i32 {
        if self.subst_cjk && is_cid_font {
            skew_from_angle(if self.italic_cjk { -15 } else { 0 })
        } else {
            self.skew()
        }
    }
    pub fn effective_weight(&self, is_cid_font: bool) -> i32 {
        if self.subst_cjk && is_cid_font {
            self.weight_cjk
        } else {
            self.weight
        }
    }

    pub fn set_family(&mut self, family: &[u8]) {
        self.family = family.to_vec();
    }
    pub fn set_charset(&mut self, charset: u8) {
        self.charset = charset;
    }
    pub fn set_weight(&mut self, weight: i32) {
        self.weight = weight;
    }
    pub fn set_italic_angle(&mut self, angle: i32) {
        self.italic_angle = angle;
    }
    pub fn set_weight_cjk(&mut self, weight: i32) {
        self.weight_cjk = weight;
    }
    pub fn set_subst_cjk(&mut self, subst: bool) {
        self.subst_cjk = subst;
    }
    pub fn set_italic_cjk(&mut self, italic: bool) {
        self.italic_cjk = italic;
    }
    pub fn set_is_builtin_generic_font(&mut self) {
        self.built_in_generic = true;
    }

    pub fn use_chrome_serif(&mut self) {
        self.weight = self.weight.saturating_mul(4) / 5;
        self.family = b"Chrome Serif".to_vec();
    }

    pub fn is_actual_font_loaded(&self, base_font_name: &[u8]) -> bool {
        let mut name: Vec<u8> = self
            .family
            .iter()
            .copied()
            .filter(|&ch| ch != b' ')
            .collect();
        crate::fxcrt::fx_system::strlwr(&mut name);
        !name.is_empty() && base_font_name.starts_with(&name)
    }

    fn weight_level(&self, index: usize) -> Option<i32> {
        let values = if self.charset == CHARSET_SHIFT_JIS {
            &WEIGHT_POW_SHIFT_JIS
        } else {
            &WEIGHT_POW_11
        };
        values.get(index).map(|&level| i32::from(level))
    }

    fn weight_level_for_load(&self, index: usize) -> i32 {
        let index = index.min(99);
        if self.charset == CHARSET_SHIFT_JIS {
            i32::from(WEIGHT_POW_SHIFT_JIS[index]) * 65536 / 36655
        } else {
            i32::from(WEIGHT_POW[index])
        }
    }

    pub fn embolden_level_for_render(
        &self,
        is_cid_font: bool,
        matrix_xx: i32,
        matrix_xy: i32,
    ) -> i32 {
        if self.built_in_generic {
            return 0;
        }
        let weight = self.effective_weight(is_cid_font);
        if weight <= 400 {
            return 0;
        }
        let index = ((weight - 400) / 10) as usize;
        let Some(level) = self.weight_level(index) else {
            return -1;
        };
        let matrix_sum = i64::from(matrix_xx).abs() + i64::from(matrix_xy).abs();
        let value = i64::from(level).checked_mul(matrix_sum).map(|v| v / 36655);
        value.and_then(|v| i32::try_from(v).ok()).unwrap_or(0)
    }

    pub fn embolden_level_for_load(&self) -> i32 {
        if self.built_in_generic || self.weight <= 400 {
            return 0;
        }
        self.weight_level_for_load(((self.weight - 400) / 10) as usize)
    }

    pub fn configure_external_subst(
        &mut self,
        face_name: &[u8],
        charset: u8,
        weight: i32,
        is_italic: bool,
        mut italic_angle: i32,
        face_is_bold: bool,
        face_is_italic: bool,
    ) {
        self.family = face_name.to_vec();
        self.charset = charset;
        let face_weight = if face_is_bold {
            FONT_WEIGHT_BOLD
        } else {
            FONT_WEIGHT_NORMAL
        };
        if weight != i32::from(face_weight) {
            self.weight = weight;
        }
        if is_italic && !face_is_italic {
            if italic_angle == 0 {
                italic_angle = -12;
            } else if italic_angle.unsigned_abs() < 5 {
                italic_angle = 0;
            }
            self.italic_angle = italic_angle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitution_skew_weights_and_emboldening_match_pdfium() {
        let mut font = CfxSubstFont::new();
        font.set_italic_angle(-12);
        assert_eq!(font.effective_skew(false), -21);
        font.set_subst_cjk(true);
        font.set_italic_cjk(true);
        assert_eq!(font.effective_skew(true), -27);

        font.set_weight(400);
        font.set_weight_cjk(700);
        assert_eq!(font.effective_weight(false), 400);
        assert_eq!(font.effective_weight(true), 700);

        font.set_weight(700);
        assert_eq!(font.embolden_level_for_render(false, 1024, 0), 1);
        assert_eq!(font.embolden_level_for_load(), 70);
        assert_eq!(
            font.embolden_level_for_render(false, 30_000_000, 30_000_000),
            63_838
        );
        assert_eq!(font.estimated_stem_v(), 140);
        font.set_is_builtin_generic_font();
        assert_eq!(font.embolden_level_for_render(false, 1024, 0), 0);
        assert_eq!(font.embolden_level_for_load(), 0);
    }

    #[test]
    fn substitution_family_and_external_style_match_pdfium() {
        let mut font = CfxSubstFont::new();
        assert!(!font.is_actual_font_loaded(b"anything"));
        font.set_family(b"A rial\0ABC");
        assert!(font.is_actual_font_loaded(b"arial\0ABC,Bold"));
        assert!(!font.is_actual_font_loaded(b"arial\0abc,Bold"));
        font.set_family(b"Times New Roman");
        assert!(font.is_actual_font_loaded(b"timesnewroman,bold"));
        assert!(font.is_actual_font_loaded(b"timesnewromanps-bold"));
        assert!(!font.is_actual_font_loaded(b"arial,bold"));

        font.configure_external_subst(b"Example", CHARSET_SYMBOL, 700, true, 0, false, false);
        assert_eq!(font.family(), b"Example");
        assert!(font.is_symbolic());
        assert!(font.is_bold());
        assert_eq!(font.italic_angle(), -12);
    }
}

#[cfg(test)]
#[path = "cfx_substfont_unittest.rs"]
mod upstream_tests;
