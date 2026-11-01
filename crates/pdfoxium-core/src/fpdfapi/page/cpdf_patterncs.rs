// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_patterncs.{h,cpp}`.

use std::rc::Rc;

use super::cpdf_basedcs::CpdfBasedCs;
use super::cpdf_colorspace::{
    ColorSpaceFamily, ColorSpaceRef, CpdfColorSpace, CpdfColorSpaceData, PatternValue,
    MAX_PATTERN_COLOR_COMPS,
};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fxge::dib::fx_dib::FxRgbStruct;

pub struct CpdfPatternCs {
    based: CpdfBasedCs,
}

impl CpdfPatternCs {
    pub fn new_stock() -> Self {
        let mut based = CpdfBasedCs::new(ColorSpaceFamily::Pattern);
        based.data_mut().set_component_count(1);
        Self { based }
    }

    pub fn load(
        array_object: &ObjectRef,
        load_base: impl FnOnce(&ObjectRef) -> Option<ColorSpaceRef>,
    ) -> Option<Self> {
        let base_object = {
            let borrowed = array_object.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfArray>()?
                .get_direct_object_at(1)
        };
        let mut based = CpdfBasedCs::new(ColorSpaceFamily::Pattern);
        let Some(base_object) = base_object else {
            based.data_mut().set_component_count(1);
            return Some(Self { based });
        };
        if Rc::ptr_eq(&base_object, array_object) {
            return None;
        }
        if let Some(base_cs) = load_base(&base_object) {
            if base_cs.family() == ColorSpaceFamily::Pattern
                || base_cs.component_count() as usize > MAX_PATTERN_COLOR_COMPS
            {
                return None;
            }
            based
                .data_mut()
                .set_component_count(base_cs.component_count() + 1);
            based.set_base_cs(Some(base_cs));
        } else {
            based.data_mut().set_component_count(1);
        }
        Some(Self { based })
    }

    pub fn get_pattern_rgb(&self, value: &PatternValue) -> Option<FxRgbStruct<f32>> {
        self.based.base_cs()?.get_rgb(value.comps())
    }
}

impl CpdfColorSpace for CpdfPatternCs {
    fn data(&self) -> &CpdfColorSpaceData {
        self.based.data()
    }
    fn as_pattern_cs(&self) -> Option<&CpdfPatternCs> {
        Some(self)
    }
    fn enable_std_conversion(&self, enabled: bool) {
        self.based.enable_std_conversion(enabled);
    }
    fn get_rgb(&self, _values: &[f32]) -> Option<FxRgbStruct<f32>> {
        panic!("patterns use get_pattern_rgb")
    }
}

#[cfg(test)]
mod tests {
    use super::super::cpdf_colorspace::{get_stock_cs_for_name, load_supported_cs};
    use super::super::cpdf_pattern::CpdfPattern;
    use super::*;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fxcrt::fx_coordinates::CfxMatrix;

    #[test]
    fn stock_and_uncolored_pattern_spaces() {
        let stock = get_stock_cs_for_name(b"Pattern").unwrap();
        assert_eq!(stock.component_count(), 1);
        assert_eq!(stock.family(), ColorSpaceFamily::Pattern);
        let mut array = CpdfArray::new();
        array.append_new_name(b"Pattern");
        array.append_new_name(b"DeviceRGB");
        let cs = load_supported_cs(&retain_object(array)).unwrap();
        assert_eq!(cs.component_count(), 4);
        let mut value = PatternValue::default();
        value.set_comps(&[0.2, 0.4, 0.6]);
        let pattern_cs = cs.as_pattern_cs().unwrap();
        assert_eq!(
            pattern_cs.get_pattern_rgb(&value),
            Some(FxRgbStruct {
                red: 0.2,
                green: 0.4,
                blue: 0.6
            })
        );
        assert_eq!(stock.as_pattern_cs().unwrap().get_pattern_rgb(&value), None);

        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"PaintType", CpdfScalarObject::number_from_signed(1));
        let pattern =
            Rc::new(CpdfPattern::new_tiling(retain_object(dict), CfxMatrix::default()).unwrap());
        assert!(pattern.colored_tiling());
        value.set_pattern(Some(pattern.clone()));
        assert!(Rc::ptr_eq(&pattern, &value.pattern().unwrap()));
    }
}
