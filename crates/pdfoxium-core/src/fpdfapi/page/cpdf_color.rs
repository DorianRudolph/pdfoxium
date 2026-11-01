// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_color.{h,cpp}`.

use std::rc::Rc;

use super::cpdf_colorspace::{
    get_stock_device_cs, ColorSpaceFamily, ColorSpaceRef, PatternValue, MAX_PATTERN_COLOR_COMPS,
};
use super::cpdf_pattern::PatternRef;
use crate::fxge::dib::fx_dib::{fxsys_bgr, FxColorRef, FxRgbStruct};

#[derive(Clone, Default)]
enum ColorData {
    #[default]
    Null,
    Values(Vec<f32>),
    Pattern(Box<PatternValue>),
}

#[derive(Clone, Default)]
pub struct CpdfColor {
    color_data: ColorData,
    cs: Option<ColorSpaceRef>,
}

impl CpdfColor {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_null(&self) -> bool {
        matches!(self.color_data, ColorData::Null)
    }
    pub fn is_pattern(&self) -> bool {
        self.cs.is_some() && self.is_pattern_internal()
    }
    fn is_pattern_internal(&self) -> bool {
        self.cs.as_ref().expect("color space must be set").family() == ColorSpaceFamily::Pattern
    }

    pub fn color_space(&self) -> Option<ColorSpaceRef> {
        self.cs.clone()
    }

    pub fn set_color_space(&mut self, color_space: ColorSpaceRef) {
        self.cs = Some(color_space);
        self.color_data = if self.is_pattern_internal() {
            ColorData::Pattern(Box::default())
        } else {
            ColorData::Values(self.cs.as_ref().unwrap().create_buf_and_set_default_color())
        };
    }

    pub fn set_value_for_non_pattern(&mut self, values: Vec<f32>) {
        assert!(!self.is_pattern_internal());
        assert!(self.component_count() as usize <= values.len());
        self.color_data = ColorData::Values(values);
    }

    pub fn set_value_for_pattern(&mut self, pattern: Option<PatternRef>, values: &[f32]) {
        if values.len() > MAX_PATTERN_COLOR_COMPS {
            return;
        }
        if !self.is_pattern() {
            self.set_color_space(
                super::cpdf_colorspace::get_stock_cs_for_name(b"Pattern")
                    .expect("stock pattern space"),
            );
        }
        let ColorData::Pattern(value) = &mut self.color_data else {
            unreachable!("pattern color has pattern data")
        };
        value.set_pattern(pattern);
        value.set_comps(values);
    }

    pub fn component_count(&self) -> u32 {
        self.cs
            .as_ref()
            .expect("color space must be set")
            .component_count()
    }

    pub fn is_color_space_rgb(&self) -> bool {
        self.cs
            .as_ref()
            .is_some_and(|cs| Rc::ptr_eq(cs, &get_stock_device_cs(ColorSpaceFamily::DeviceRgb)))
    }

    pub fn is_color_space_gray(&self) -> bool {
        self.cs
            .as_ref()
            .is_some_and(|cs| Rc::ptr_eq(cs, &get_stock_device_cs(ColorSpaceFamily::DeviceGray)))
    }

    pub fn get_color_ref(&self) -> Option<FxColorRef> {
        let rgb = self.get_rgb()?;
        let r = (rgb.red.clamp(0.0, 1.0) * 255.0).round() as u8;
        let g = (rgb.green.clamp(0.0, 1.0) * 255.0).round() as u8;
        let b = (rgb.blue.clamp(0.0, 1.0) * 255.0).round() as u8;
        Some(fxsys_bgr(b, g, r))
    }

    pub fn get_rgb(&self) -> Option<FxRgbStruct<f32>> {
        let cs = self.cs.as_ref()?;
        if self.is_pattern_internal() {
            let ColorData::Pattern(value) = &self.color_data else {
                return None;
            };
            cs.as_pattern_cs()?.get_pattern_rgb(value)
        } else {
            let ColorData::Values(values) = &self.color_data else {
                return None;
            };
            cs.get_rgb(values)
        }
    }

    pub fn get_pattern(&self) -> Option<PatternRef> {
        assert!(self.is_pattern());
        let ColorData::Pattern(value) = &self.color_data else {
            unreachable!("pattern data required")
        };
        value.pattern()
    }
}

#[cfg(test)]
mod tests {
    use super::super::cpdf_colorspace::load_supported_cs;
    use super::*;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_object::retain_object;

    #[test]
    fn device_color_ref_and_pattern_rgb() {
        let mut color = CpdfColor::new();
        assert!(color.is_null());
        color.set_color_space(get_stock_device_cs(ColorSpaceFamily::DeviceRgb));
        assert!(color.is_color_space_rgb());
        color.set_value_for_non_pattern(vec![1.0, 0.5, 0.0]);
        assert_eq!(color.get_color_ref(), Some(fxsys_bgr(0, 128, 255)));

        let mut pattern_array = CpdfArray::new();
        pattern_array.append_new_name(b"Pattern");
        pattern_array.append_new_name(b"DeviceRGB");
        color.set_color_space(load_supported_cs(&retain_object(pattern_array)).unwrap());
        assert!(color.is_pattern());
        assert_eq!(
            color.get_rgb(),
            Some(FxRgbStruct {
                red: 0.0,
                green: 0.0,
                blue: 0.0
            })
        );
        color.set_value_for_pattern(None, &[0.25, 0.5, 0.75]);
        assert_eq!(
            color.get_rgb(),
            Some(FxRgbStruct {
                red: 0.25,
                green: 0.5,
                blue: 0.75
            })
        );
        let copy = color.clone();
        color.set_value_for_pattern(None, &[1.0, 1.0, 1.0]);
        assert_eq!(
            copy.get_rgb(),
            Some(FxRgbStruct {
                red: 0.25,
                green: 0.5,
                blue: 0.75
            })
        );
    }
}
