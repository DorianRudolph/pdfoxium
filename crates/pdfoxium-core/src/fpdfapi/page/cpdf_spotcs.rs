// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Separation and DeviceN classes from `core/fpdfapi/page/cpdf_colorspace.cpp`.

use std::rc::Rc;

use super::cpdf_basedcs::CpdfBasedCs;
use super::cpdf_colorspace::{ColorSpaceFamily, ColorSpaceRef, CpdfColorSpace, CpdfColorSpaceData};
use super::cpdf_function::CpdfFunction;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fxge::dib::fx_dib::FxRgbStruct;

pub struct CpdfSeparationCs {
    based: CpdfBasedCs,
    is_none_type: bool,
    function: Option<CpdfFunction>,
}

impl CpdfSeparationCs {
    /// `load_base` represents `CPDF_ColorSpace::Load()` with its visited set.
    pub fn load(
        array_object: &ObjectRef,
        load_base: impl FnOnce(&ObjectRef) -> Option<ColorSpaceRef>,
    ) -> Option<Self> {
        let is_none_type = crate::fxcrt::bytestring::eq_cstr(
            &array_object
                .borrow()
                .as_any()
                .downcast_ref::<CpdfArray>()?
                .get_byte_string_at(1),
            b"None",
        );
        let mut based = CpdfBasedCs::new(ColorSpaceFamily::Separation);
        based.data_mut().set_component_count(1);
        if is_none_type {
            return Some(Self {
                based,
                is_none_type,
                function: None,
            });
        }
        let alt_object = array_object
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()?
            .get_direct_object_at(2)?;
        if Rc::ptr_eq(&alt_object, array_object) {
            return None;
        }
        let base_cs = load_base(&alt_object)?;
        if base_cs.is_special() {
            return None;
        }
        let function_object = array_object
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()?
            .get_direct_object_at(3);
        let function = function_object
            .filter(|object| object.borrow().object_type() != ObjectType::Name)
            .and_then(|object| CpdfFunction::load(&object))
            .filter(|function| function.output_count() >= base_cs.component_count());
        based.set_base_cs(Some(base_cs));
        Some(Self {
            based,
            is_none_type,
            function,
        })
    }
}

impl CpdfColorSpace for CpdfSeparationCs {
    fn data(&self) -> &CpdfColorSpaceData {
        self.based.data()
    }

    fn enable_std_conversion(&self, enabled: bool) {
        self.based.enable_std_conversion(enabled);
    }

    fn get_default_value(&self, _component: usize) -> (f32, f32, f32) {
        (1.0, 0.0, 1.0)
    }

    fn get_rgb(&self, values: &[f32]) -> Option<FxRgbStruct<f32>> {
        if self.is_none_type {
            return None;
        }
        let base_cs = self.based.base_cs()?;
        if let Some(function) = &self.function {
            let mut results = vec![0.0; (function.output_count() as usize).max(16)];
            if function.call(values.get(..1)?, &mut results).unwrap_or(0) == 0 {
                return None;
            }
            base_cs.get_rgb(&results)
        } else {
            let results = vec![*values.first()?; base_cs.component_count() as usize];
            base_cs.get_rgb(&results)
        }
    }
}

pub struct CpdfDeviceNCs {
    based: CpdfBasedCs,
    function: CpdfFunction,
}

impl CpdfDeviceNCs {
    /// `load_base` represents `CPDF_ColorSpace::Load()` with its visited set.
    pub fn load(
        array_object: &ObjectRef,
        load_base: impl FnOnce(&ObjectRef) -> Option<ColorSpaceRef>,
    ) -> Option<Self> {
        let names = array_object
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()?
            .get_array_at(1)?;
        let alternate = array_object
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()?
            .get_direct_object_at(2)?;
        if Rc::ptr_eq(&alternate, array_object) {
            return None;
        }
        let base_cs = load_base(&alternate);
        let function_object = array_object
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()?
            .get_direct_object_at(3);
        let function = function_object.and_then(|object| CpdfFunction::load(&object));
        let (base_cs, function) = (base_cs?, function?);
        if base_cs.is_special() || function.output_count() < base_cs.component_count() {
            return None;
        }
        let components = {
            let borrowed = names.borrow();
            u32::try_from(borrowed.as_any().downcast_ref::<CpdfArray>()?.size()).ok()?
        };
        if components == 0 {
            return None;
        }
        let mut based = CpdfBasedCs::new(ColorSpaceFamily::DeviceN);
        based.set_base_cs(Some(base_cs));
        based.data_mut().set_component_count(components);
        Some(Self { based, function })
    }
}

#[cfg(test)]
mod tests {
    use super::super::cpdf_colorspace::{get_stock_cs_for_name, load_supported_cs};
    use super::*;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    fn values(values: &[f32]) -> CpdfArray {
        let mut array = CpdfArray::new();
        for &value in values {
            array.append_new(CpdfScalarObject::number_from_float(value));
        }
        array
    }

    fn tint_function(domain: &[f32], start: &[f32], end: &[f32]) -> CpdfDictionary {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"Domain", values(domain));
        dict.set_new_for(b"C0", values(start));
        dict.set_new_for(b"C1", values(end));
        dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
        dict
    }

    #[test]
    fn separation_none_and_tint_transform() {
        let mut none = CpdfArray::new();
        none.append_new_name(b"Separation");
        none.append_new_name(b"None");
        let none = CpdfSeparationCs::load(&retain_object(none), |_| None).unwrap();
        assert_eq!(none.get_default_value(0), (1.0, 0.0, 1.0));
        assert_eq!(none.get_rgb(&[0.5]), None);

        let mut array = CpdfArray::new();
        array.append_new_name(b"Separation");
        array.append_new_name(b"Spot");
        array.append_new_name(b"DeviceRGB");
        array.append_new(tint_function(
            &[0.0, 1.0],
            &[1.0, 1.0, 1.0],
            &[0.0, 0.0, 0.0],
        ));
        let object = retain_object(array);
        let cs = CpdfSeparationCs::load(&object, |base| {
            get_stock_cs_for_name(&base.borrow().get_string())
        })
        .unwrap();
        assert_eq!(
            cs.get_rgb(&[0.5]),
            Some(FxRgbStruct {
                red: 0.5,
                green: 0.5,
                blue: 0.5
            })
        );
        assert_eq!(
            load_supported_cs(&object).unwrap().get_rgb(&[0.5]),
            cs.get_rgb(&[0.5])
        );
    }

    #[test]
    fn device_n_passes_all_tints_to_function() {
        let mut names = CpdfArray::new();
        names.append_new_name(b"SpotA");
        names.append_new_name(b"SpotB");
        let mut array = CpdfArray::new();
        array.append_new_name(b"DeviceN");
        array.append_new(names);
        array.append_new_name(b"DeviceGray");
        array.append_new(tint_function(&[0.0, 1.0, 0.0, 1.0], &[0.0], &[1.0]));
        let cs = CpdfDeviceNCs::load(&retain_object(array), |base| {
            get_stock_cs_for_name(&base.borrow().get_string())
        })
        .unwrap();
        assert_eq!(cs.component_count(), 2);
        assert_eq!(
            cs.get_rgb(&[0.25, 0.75]),
            Some(FxRgbStruct {
                red: 0.25,
                green: 0.25,
                blue: 0.25
            })
        );
        assert_eq!(cs.get_rgb(&[0.25]), None);
    }
}

impl CpdfColorSpace for CpdfDeviceNCs {
    fn data(&self) -> &CpdfColorSpaceData {
        self.based.data()
    }

    fn enable_std_conversion(&self, enabled: bool) {
        self.based.enable_std_conversion(enabled);
    }

    fn get_default_value(&self, _component: usize) -> (f32, f32, f32) {
        (1.0, 0.0, 1.0)
    }

    fn get_rgb(&self, values: &[f32]) -> Option<FxRgbStruct<f32>> {
        let mut results = vec![0.0; (self.function.output_count() as usize).max(16)];
        let inputs = values.get(..self.component_count() as usize)?;
        if self.function.call(inputs, &mut results).unwrap_or(0) == 0 {
            return None;
        }
        self.based.base_cs()?.get_rgb(&results)
    }
}
