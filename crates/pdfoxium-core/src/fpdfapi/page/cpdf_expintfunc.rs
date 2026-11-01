// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/page/cpdf_expintfunc.{h,cpp}`.

use super::cpdf_function::{CpdfFunctionData, FunctionType};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_dict_object, ObjectRef};

pub struct CpdfExpIntFunc {
    base: CpdfFunctionData,
    orig_outputs: u32,
    exponent: f32,
    begin_values: Vec<f32>,
    end_values: Vec<f32>,
}

impl CpdfExpIntFunc {
    pub fn load(object: &ObjectRef) -> Option<Self> {
        let mut base = CpdfFunctionData::init(object, FunctionType::ExponentialInterpolation)?;
        let dict = get_dict_object(object)?;
        let borrowed = dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        if dict.get_integer_for(b"FunctionType", -1) != 2 {
            return None;
        }
        let exponent = dict.get_number_for(b"N")?.borrow().get_number();
        let first = dict.get_array_for(b"C0");
        let second = dict.get_array_for(b"C1");
        if base.output_count() == 0 {
            if let Some(first) = &first {
                let borrowed = first.borrow();
                base.set_output_count(
                    u32::try_from(borrowed.as_any().downcast_ref::<CpdfArray>()?.size()).ok()?,
                );
            }
        }
        if base.output_count() == 0 {
            base.set_output_count(1);
        }
        let orig_outputs = base.output_count();
        let size = (orig_outputs as usize).checked_mul(2)?;
        let mut begin_values = vec![0.0; size];
        let mut end_values = vec![0.0; size];
        for i in 0..orig_outputs as usize {
            begin_values[i] = first.as_ref().map_or(0.0, |array| {
                array
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfArray>()
                    .unwrap()
                    .get_float_at(i)
            });
            end_values[i] = second.as_ref().map_or(1.0, |array| {
                array
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfArray>()
                    .unwrap()
                    .get_float_at(i)
            });
        }
        base.set_output_count(orig_outputs.checked_mul(base.input_count())?);
        base.resize_ranges_for_outputs();
        Some(Self {
            base,
            orig_outputs,
            exponent,
            begin_values,
            end_values,
        })
    }

    pub fn base(&self) -> &CpdfFunctionData {
        &self.base
    }
    pub fn orig_outputs(&self) -> u32 {
        self.orig_outputs
    }
    pub fn exponent(&self) -> f32 {
        self.exponent
    }
    pub fn begin_values(&self) -> &[f32] {
        &self.begin_values
    }
    pub fn end_values(&self) -> &[f32] {
        &self.end_values
    }
    pub fn call(&self, inputs: &[f32], results: &mut [f32]) -> Option<u32> {
        self.base.call(inputs, results, |inputs, results| {
            for (i, &input) in inputs.iter().enumerate() {
                let power = input.powf(self.exponent);
                for j in 0..self.orig_outputs as usize {
                    results[i * self.orig_outputs as usize + j] =
                        self.begin_values[j] + power * (self.end_values[j] - self.begin_values[j]);
                }
            }
            true
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfExpIntFunc;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    fn float_array(values: &[f32]) -> CpdfArray {
        let mut array = CpdfArray::new();
        for &value in values {
            array.append_new(CpdfScalarObject::number_from_float(value));
        }
        array
    }

    #[test]
    fn exponential_interpolation_clamps_domain_and_range() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"Domain", float_array(&[0.0, 1.0]));
        dict.set_new_for(b"Range", float_array(&[0.0, 0.5, 0.0, 1.0]));
        dict.set_new_for(b"C0", float_array(&[0.0, 0.2]));
        dict.set_new_for(b"C1", float_array(&[1.0, 0.8]));
        dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(2));
        let function = CpdfExpIntFunc::load(&retain_object(dict)).unwrap();
        assert_eq!(function.orig_outputs(), 2);
        let mut results = [0.0; 2];
        assert_eq!(function.call(&[0.5], &mut results), Some(2));
        assert_eq!(results[0], 0.25);
        assert!((results[1] - 0.35).abs() < 0.000001);
        assert_eq!(function.call(&[2.0], &mut results), Some(2));
        assert_eq!(results, [0.5, 0.8]);
        assert_eq!(function.call(&[], &mut results), None);
    }
}
