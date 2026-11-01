// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Widget appearance settings from `core/fpdfdoc/cpdf_apsettings.{h,cpp}`.
use super::cpdf_formfield::{
    array_at, array_len, dict_get, dict_has, dict_int, dict_unicode, typed,
};
use super::cpdf_iconfit::CpdfIconFit;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fxge::cfx_color::{CfxColor, ColorType, TypeAndArgb};
use crate::fxge::dib::fx_dib::argb_encode;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum TextPosition {
    Caption = 0,
    Icon = 1,
    Below = 2,
    Above = 3,
    Right = 4,
    Left = 5,
    Overlaid = 6,
}
#[derive(Clone)]
pub struct CpdfApSettings {
    dict: Option<ObjectRef>,
}
impl CpdfApSettings {
    pub fn new(dict: Option<ObjectRef>) -> Self {
        Self { dict }
    }
    pub fn has_mk_entry(&self, key: &[u8]) -> bool {
        self.dict.as_ref().is_some_and(|d| dict_has(d, key))
    }
    pub fn rotation(&self) -> i32 {
        self.dict.as_ref().map_or(0, |d| dict_int(d, b"R", 0))
    }
    fn color_values(&self, key: &[u8]) -> Option<Vec<f32>> {
        let array = typed(dict_get(self.dict.as_ref()?, key), ObjectType::Array)?;
        Some(
            (0..array_len(&array))
                .map(|i| array_at(&array, i).map_or(0., |o| o.borrow().get_number()))
                .collect(),
        )
    }
    pub fn color_argb(&self, key: &[u8]) -> TypeAndArgb {
        let Some(v) = self.color_values(key) else {
            return TypeAndArgb {
                color_type: ColorType::Transparent,
                argb: 0,
            };
        };
        let (ty, r, g, b) = match v.as_slice() {
            [g] => (ColorType::Gray, *g, *g, *g),
            [r, g, b] => (ColorType::Rgb, *r, *g, *b),
            [c, m, y, k] => (
                ColorType::Cmyk,
                1. - (c + k).min(1.),
                1. - (m + k).min(1.),
                1. - (y + k).min(1.),
            ),
            _ => {
                return TypeAndArgb {
                    color_type: ColorType::Transparent,
                    argb: 0,
                }
            }
        };
        TypeAndArgb {
            color_type: ty,
            argb: argb_encode(
                255,
                (r * 255.) as i32 as u32,
                (g * 255.) as i32 as u32,
                (b * 255.) as i32 as u32,
            ),
        }
    }
    pub fn original_color_component(&self, index: i32, key: &[u8]) -> f32 {
        if index < 0 {
            return 0.;
        }
        self.color_values(key)
            .and_then(|v| v.get(index as usize).copied())
            .unwrap_or(0.)
    }
    pub fn original_color(&self, key: &[u8]) -> CfxColor {
        match self.color_values(key).as_deref() {
            Some([g]) => CfxColor::make_gray(*g),
            Some([r, g, b]) => CfxColor::make_rgb(*r, *g, *b),
            Some([c, m, y, k]) => CfxColor::make_cmyk(*c, *m, *y, *k),
            _ => CfxColor::default(),
        }
    }
    pub fn caption(&self, key: &[u8]) -> Vec<u32> {
        self.dict
            .as_ref()
            .map_or_else(Vec::new, |d| dict_unicode(d, key))
    }
    pub fn icon(&self, key: &[u8]) -> Option<ObjectRef> {
        typed(dict_get(self.dict.as_ref()?, key), ObjectType::Stream)
    }
    pub fn icon_fit(&self) -> CpdfIconFit {
        CpdfIconFit::new(
            self.dict
                .as_ref()
                .and_then(|d| typed(dict_get(d, b"IF"), ObjectType::Dictionary)),
        )
    }
    pub fn text_position(&self) -> TextPosition {
        match self.dict.as_ref().map_or(0, |d| dict_int(d, b"TP", 0)) {
            1 => TextPosition::Icon,
            2 => TextPosition::Below,
            3 => TextPosition::Above,
            4 => TextPosition::Right,
            5 => TextPosition::Left,
            6 => TextPosition::Overlaid,
            _ => TextPosition::Caption,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF, CfxSizeF};
    #[test]
    fn widget_settings_and_icon_fit_preserve_defaults_colors_and_alignment() {
        let empty = CpdfApSettings::new(None);
        assert_eq!(empty.text_position(), TextPosition::Caption);
        assert_eq!(empty.color_argb(b"BC").color_type, ColorType::Transparent);
        assert_eq!(
            empty.icon_fit().icon_bottom_left_position(),
            CfxPointF::new(0.5, 0.5)
        );
        let mut d = CpdfDictionary::new();
        let mut color = CpdfArray::new();
        for c in [0.25, 0.5, 0.75] {
            color.append_new(CpdfScalarObject::number_from_float(c));
        }
        d.set_new_for(b"BC", color);
        d.set_new_for(b"TP", CpdfScalarObject::number_from_signed(100));
        d.set_new_for(b"R", CpdfScalarObject::number_from_signed(90));
        let mut fit = CpdfDictionary::new();
        fit.set_new_name_for(b"SW", b"B");
        fit.set_new_name_for(b"S", b"A");
        let mut position = CpdfArray::new();
        position.append_new(CpdfScalarObject::number_from_float(0.25));
        position.append_new(CpdfScalarObject::number_from_float(0.75));
        fit.set_new_for(b"A", position);
        d.set_new_for(b"IF", fit);
        let settings = CpdfApSettings::new(Some(retain_object(d)));
        assert_eq!(settings.rotation(), 90);
        assert_eq!(settings.text_position(), TextPosition::Caption);
        assert_eq!(
            settings.color_argb(b"BC"),
            TypeAndArgb {
                color_type: ColorType::Rgb,
                argb: 0xff3f7fbf
            }
        );
        assert_eq!(settings.original_color_component(2, b"BC"), 0.75);
        let fit = settings.icon_fit();
        let image = CfxSizeF::new(200., 50.);
        let plate = CfxFloatRect::new(0., 0., 100., 100.);
        let scale = fit.scale(image, plate);
        assert_eq!((scale.x, scale.y), (0.5, 1.));
        assert_eq!(fit.image_offset(image, scale, plate).y, 37.5);
    }
}
