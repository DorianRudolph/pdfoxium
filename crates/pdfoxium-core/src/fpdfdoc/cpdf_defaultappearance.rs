// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpdf_defaultappearance.{h,cpp}`.

use super::cpdf_formfield::get_field_attr_for_dict;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_simple_parser::CpdfSimpleParser;
use crate::fpdfapi::parser::fpdf_parser_utility::pdf_name_decode;
use crate::fxcrt::fx_string::string_to_float;
use crate::fxge::cfx_color::{CfxColor, ColorType, TypeAndArgb};
use crate::fxge::dib::fx_dib::argb_encode;

#[derive(Default)]
pub struct FontNameAndSize {
    pub name: Vec<u8>,
    pub size: f32,
}

pub struct CpdfDefaultAppearance {
    da: Vec<u8>,
}

pub fn find_tag_param_from_start(
    parser: &mut CpdfSimpleParser<'_>,
    token: &[u8],
    params: usize,
) -> bool {
    let params = params + 1;
    let mut positions = vec![0; params];
    let mut index = 0;
    let mut count = 0;
    parser.set_current_position(0);
    loop {
        positions[index] = parser.current_position();
        index += 1;
        if index == params {
            index = 0;
        }
        count = (count + 1).min(params);
        let word = parser.get_word();
        if word.is_empty() {
            return false;
        }
        if word == token && count >= params {
            parser.set_current_position(positions[index]);
            return true;
        }
    }
}

impl CpdfDefaultAppearance {
    pub fn new(da: Vec<u8>) -> Self {
        Self { da }
    }

    pub fn from_dicts(annot: Option<&CpdfDictionary>, acroform: Option<&CpdfDictionary>) -> Self {
        let mut da = get_field_attr_for_dict(annot, b"DA")
            .map_or_else(Vec::new, |attr| attr.borrow().get_string());
        if da.is_empty() {
            if let Some(acroform) = acroform {
                da = acroform.get_byte_string_for(b"DA", b"");
            }
        }
        Self::new(da)
    }

    pub fn font(&self) -> Option<FontNameAndSize> {
        if self.da.is_empty() {
            return None;
        }
        let mut syntax = CpdfSimpleParser::new(&self.da);
        if !find_tag_param_from_start(&mut syntax, b"Tf", 2) {
            return Some(FontNameAndSize::default());
        }
        let name = pdf_name_decode(syntax.get_word().get(1..).unwrap_or_default());
        let size = string_to_float(syntax.get_word());
        Some(FontNameAndSize { name, size })
    }

    pub fn font_size_or_zero(&self) -> f32 {
        self.font().map_or(0.0, |font| font.size)
    }

    pub fn color(&self) -> Option<CfxColor> {
        if self.da.is_empty() {
            return None;
        }
        let mut syntax = CpdfSimpleParser::new(&self.da);
        if find_tag_param_from_start(&mut syntax, b"g", 1) {
            return Some(CfxColor::make_gray(string_to_float(syntax.get_word())));
        }
        if find_tag_param_from_start(&mut syntax, b"rg", 3) {
            let red = string_to_float(syntax.get_word());
            let green = string_to_float(syntax.get_word());
            let blue = string_to_float(syntax.get_word());
            return Some(CfxColor::make_rgb(red, green, blue));
        }
        if find_tag_param_from_start(&mut syntax, b"k", 4) {
            let cyan = string_to_float(syntax.get_word());
            let magenta = string_to_float(syntax.get_word());
            let yellow = string_to_float(syntax.get_word());
            let key = string_to_float(syntax.get_word());
            return Some(CfxColor::make_cmyk(cyan, magenta, yellow, key));
        }
        None
    }

    pub fn color_argb(&self) -> Option<TypeAndArgb> {
        let color = self.color()?;
        let byte = |value: f32| (value * 255.0 + 0.5) as i32 as u32;
        let (red, green, blue) = match color.color_type {
            ColorType::Gray => (byte(color.color1), byte(color.color1), byte(color.color1)),
            ColorType::Rgb => (byte(color.color1), byte(color.color2), byte(color.color3)),
            ColorType::Cmyk => (
                byte(1.0 - (color.color1 + color.color4).min(1.0)),
                byte(1.0 - (color.color2 + color.color4).min(1.0)),
                byte(1.0 - (color.color3 + color.color4).min(1.0)),
            ),
            ColorType::Transparent => unreachable!("DA operators produce gray, RGB or CMYK"),
        };
        Some(TypeAndArgb {
            color_type: color.color_type,
            argb: argb_encode(255, red, green, blue),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_appearance_uses_first_tag_and_gray_before_rgb() {
        let da = CpdfDefaultAppearance::new(b"Tf 1 0 0 rg /F#31 12 Tf /F2 8 Tf 0.5 g 1 g".to_vec());
        let font = da.font().unwrap();
        assert_eq!(font.name, b"F1");
        assert_eq!(font.size, 12.0);
        assert_eq!(
            da.color_argb().unwrap(),
            TypeAndArgb {
                color_type: ColorType::Gray,
                argb: 0xff808080
            }
        );
        assert!(CpdfDefaultAppearance::new(Vec::new()).font().is_none());
        let missing = CpdfDefaultAppearance::new(b"q Q".to_vec()).font().unwrap();
        assert!(missing.name.is_empty());
        assert_eq!(missing.size, 0.0);
    }

    #[test]
    fn inherited_da_and_explicit_empty_da_follow_acroform_fallback() {
        let mut parent = CpdfDictionary::new();
        parent.set_new_string_for(b"DA", b"/Parent 9 Tf");
        let mut annot = CpdfDictionary::new();
        annot.set_new_for(b"Parent", parent);
        let mut acroform = CpdfDictionary::new();
        acroform.set_new_string_for(b"DA", b"/Acroform 11 Tf");
        assert_eq!(
            CpdfDefaultAppearance::from_dicts(Some(&annot), Some(&acroform))
                .font()
                .unwrap()
                .name,
            b"Parent"
        );
        annot.set_new_string_for(b"DA", b"");
        assert_eq!(
            CpdfDefaultAppearance::from_dicts(Some(&annot), Some(&acroform))
                .font()
                .unwrap()
                .name,
            b"Acroform"
        );
    }
}

#[cfg(test)]
#[path = "cpdf_defaultappearance_unittest.rs"]
mod upstream_tests;
