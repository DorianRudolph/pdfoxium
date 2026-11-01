// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_colorstate.{h,cpp}`.

use super::cpdf_color::CpdfColor;
use super::cpdf_colorspace::{get_stock_device_cs, ColorSpaceFamily, ColorSpaceRef};
use super::cpdf_pattern::PatternRef;
use crate::fxcrt::shared_copy_on_write::SharedCopyOnWrite;
use crate::fxge::dib::fx_dib::FxColorRef;

#[derive(Clone, Default)]
struct ColorData {
    fill_color_ref: FxColorRef,
    stroke_color_ref: FxColorRef,
    fill_color: CpdfColor,
    stroke_color: CpdfColor,
}

impl ColorData {
    fn set_default(&mut self) {
        self.fill_color_ref = 0;
        self.stroke_color_ref = 0;
        self.fill_color
            .set_color_space(get_stock_device_cs(ColorSpaceFamily::DeviceGray));
        self.stroke_color
            .set_color_space(get_stock_device_cs(ColorSpaceFamily::DeviceGray));
    }
}

#[derive(Clone, Default)]
pub struct CpdfColorState {
    data: SharedCopyOnWrite<ColorData>,
}

impl CpdfColorState {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn emplace(&mut self) {
        self.data.emplace(ColorData::default());
    }
    pub fn has_ref(&self) -> bool {
        self.data.is_some()
    }
    pub fn set_default(&mut self) {
        self.data.get_private_copy(ColorData::default).set_default();
    }

    pub fn get_fill_color_ref(&self) -> FxColorRef {
        self.data
            .get_object()
            .expect("color state has no reference")
            .fill_color_ref
    }

    pub fn set_fill_color_ref(&mut self, reference: FxColorRef) {
        if !self.has_ref() || self.get_fill_color_ref() != reference {
            self.data
                .get_private_copy(ColorData::default)
                .fill_color_ref = reference;
        }
    }

    pub fn get_stroke_color_ref(&self) -> FxColorRef {
        self.data
            .get_object()
            .expect("color state has no reference")
            .stroke_color_ref
    }

    pub fn set_stroke_color_ref(&mut self, reference: FxColorRef) {
        if !self.has_ref() || self.get_stroke_color_ref() != reference {
            self.data
                .get_private_copy(ColorData::default)
                .stroke_color_ref = reference;
        }
    }

    pub fn get_fill_color(&self) -> Option<&CpdfColor> {
        self.data.get_object().map(|data| &data.fill_color)
    }
    pub fn get_mutable_fill_color(&mut self) -> &mut CpdfColor {
        &mut self.data.get_private_copy(ColorData::default).fill_color
    }
    pub fn has_fill_color(&self) -> bool {
        self.get_fill_color().is_some_and(|color| !color.is_null())
    }

    pub fn get_stroke_color(&self) -> Option<&CpdfColor> {
        self.data.get_object().map(|data| &data.stroke_color)
    }
    pub fn get_mutable_stroke_color(&mut self) -> &mut CpdfColor {
        &mut self.data.get_private_copy(ColorData::default).stroke_color
    }
    pub fn has_stroke_color(&self) -> bool {
        self.get_stroke_color()
            .is_some_and(|color| !color.is_null())
    }

    pub fn set_fill_color(&mut self, color_space: Option<ColorSpaceRef>, values: Vec<f32>) {
        let data = self.data.get_private_copy(ColorData::default);
        if let Some(reference) = Self::set_color(color_space, values, &mut data.fill_color) {
            data.fill_color_ref = reference;
        }
    }

    pub fn set_stroke_color(&mut self, color_space: Option<ColorSpaceRef>, values: Vec<f32>) {
        let data = self.data.get_private_copy(ColorData::default);
        if let Some(reference) = Self::set_color(color_space, values, &mut data.stroke_color) {
            data.stroke_color_ref = reference;
        }
    }

    pub fn set_fill_pattern(&mut self, pattern: PatternRef, values: &[f32]) {
        let data = self.data.get_private_copy(ColorData::default);
        data.fill_color_ref = Self::set_pattern(pattern, values, &mut data.fill_color);
    }

    pub fn set_stroke_pattern(&mut self, pattern: PatternRef, values: &[f32]) {
        let data = self.data.get_private_copy(ColorData::default);
        data.stroke_color_ref = Self::set_pattern(pattern, values, &mut data.stroke_color);
    }

    fn set_color(
        color_space: Option<ColorSpaceRef>,
        values: Vec<f32>,
        color: &mut CpdfColor,
    ) -> Option<FxColorRef> {
        if let Some(space) = color_space {
            color.set_color_space(space);
        } else if color.is_null() {
            color.set_color_space(get_stock_device_cs(ColorSpaceFamily::DeviceGray));
        }
        if color.component_count() as usize > values.len() {
            return None;
        }
        if !color.is_pattern() {
            color.set_value_for_non_pattern(values);
        }
        Some(color.get_color_ref().unwrap_or(0xffff_ffff))
    }

    fn set_pattern(pattern: PatternRef, values: &[f32], color: &mut CpdfColor) -> FxColorRef {
        color.set_value_for_pattern(Some(pattern.clone()), values);
        if let Some(reference) = color.get_color_ref() {
            return reference;
        }
        if pattern.colored_tiling() {
            0x00bf_bfbf
        } else {
            0xffff_ffff
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::super::cpdf_pattern::CpdfPattern;
    use super::*;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fxcrt::fx_coordinates::CfxMatrix;

    #[test]
    fn colors_copy_on_write_and_pattern_fallback() {
        let mut state = CpdfColorState::new();
        assert!(!state.has_ref());
        state.set_default();
        assert!(state.has_fill_color());
        assert!(state.has_stroke_color());
        assert_eq!(state.get_fill_color_ref(), 0);
        state.set_fill_color(None, vec![0.5]);
        assert_eq!(state.get_fill_color_ref(), 0x0080_8080);
        let original = state.clone();
        state.set_stroke_color(
            Some(get_stock_device_cs(ColorSpaceFamily::DeviceRgb)),
            vec![1.0, 0.0, 0.0],
        );
        assert_eq!(state.get_stroke_color_ref(), 0x0000_00ff);
        assert_eq!(original.get_stroke_color_ref(), 0);
        state.set_fill_color(
            Some(get_stock_device_cs(ColorSpaceFamily::DeviceRgb)),
            vec![0.25],
        );
        assert_eq!(state.get_fill_color_ref(), 0x0080_8080);
        assert!(state.get_fill_color().unwrap().is_color_space_rgb());

        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"PaintType", CpdfScalarObject::number_from_signed(1));
        let colored =
            Rc::new(CpdfPattern::new_tiling(retain_object(dict), CfxMatrix::default()).unwrap());
        state.set_fill_pattern(colored, &[]);
        assert_eq!(state.get_fill_color_ref(), 0x00bf_bfbf);
        assert_eq!(original.get_fill_color_ref(), 0x0080_8080);
    }
}
