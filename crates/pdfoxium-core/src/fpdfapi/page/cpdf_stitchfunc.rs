// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/page/cpdf_stitchfunc.{h,cpp}`.

use std::collections::HashSet;
use std::rc::Rc;

use super::cpdf_function::{CpdfFunction, CpdfFunctionData, FunctionType};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_dict_object, ObjectRef};
use crate::fpdfapi::parser::fpdf_parser_utility::read_array_elements_to_vector;

pub struct CpdfStitchFunc {
    base: CpdfFunctionData,
    sub_functions: Vec<CpdfFunction>,
    bounds: Vec<f32>,
    encode: Vec<f32>,
}

impl CpdfStitchFunc {
    pub(crate) fn load(object: &ObjectRef, visited: &mut HashSet<usize>) -> Option<Self> {
        let mut base = CpdfFunctionData::init(object, FunctionType::Stitching)?;
        if base.input_count() != 1 {
            return None;
        }
        let dict = get_dict_object(object)?;
        let borrowed = dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        if dict.get_integer_for(b"FunctionType", -1) != 3 {
            return None;
        }
        let functions = dict.get_array_for(b"Functions")?;
        let bounds = dict.get_array_for(b"Bounds")?;
        let encode = dict.get_array_for(b"Encode")?;
        let funcs_borrowed = functions.borrow();
        let funcs = funcs_borrowed.as_any().downcast_ref::<CpdfArray>()?;
        let bounds_borrowed = bounds.borrow();
        let bounds_array = bounds_borrowed.as_any().downcast_ref::<CpdfArray>()?;
        let encode_borrowed = encode.borrow();
        let encode_array = encode_borrowed.as_any().downcast_ref::<CpdfArray>()?;
        let count = funcs.size();
        if count == 0
            || bounds_array.size() < count - 1
            || encode_array.size() < count.checked_mul(2)?
        {
            return None;
        }
        let mut sub_functions = Vec::with_capacity(count);
        let mut outputs = None;
        for i in 0..count {
            let sub = funcs.get_direct_object_at(i)?;
            if Rc::ptr_eq(&sub, object) {
                return None;
            }
            let function = CpdfFunction::load_with_visited(&sub, visited)?;
            let count = function.output_count();
            if function.input_count() != 1
                || count == 0
                || outputs.is_some_and(|previous| previous != count)
            {
                return None;
            }
            outputs = Some(count);
            sub_functions.push(function);
        }
        base.set_output_count(outputs?);
        base.resize_ranges_for_outputs();
        let mut bounds = Vec::with_capacity(count + 1);
        bounds.push(base.domain(0));
        for i in 0..count - 1 {
            bounds.push(bounds_array.get_float_at(i));
        }
        bounds.push(base.domain(1));
        let encode = read_array_elements_to_vector(encode_array, count * 2);
        Some(Self {
            base,
            sub_functions,
            bounds,
            encode,
        })
    }

    pub fn base(&self) -> &CpdfFunctionData {
        &self.base
    }
    pub fn sub_functions(&self) -> &[CpdfFunction] {
        &self.sub_functions
    }
    pub fn bound(&self, index: usize) -> f32 {
        self.bounds[index]
    }
    pub fn encode(&self, index: usize) -> f32 {
        self.encode[index]
    }
    pub fn call(&self, inputs: &[f32], results: &mut [f32]) -> Option<u32> {
        self.base.call(inputs, results, |inputs, results| {
            let input = inputs[0];
            let mut index = 0;
            while index + 1 < self.sub_functions.len() && input >= self.bounds[index + 1] {
                index += 1;
            }
            let value = CpdfFunctionData::interpolate(
                input,
                self.bounds[index],
                self.bounds[index + 1],
                self.encode[index * 2],
                self.encode[index * 2 + 1],
            );
            self.sub_functions[index].call(&[value], results).is_some()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfFunction;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    fn numbers(values: &[f32]) -> CpdfArray {
        let mut array = CpdfArray::new();
        for &value in values {
            array.append_new(CpdfScalarObject::number_from_float(value));
        }
        array
    }

    fn exp(first: f32, last: f32) -> CpdfDictionary {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"Domain", numbers(&[0.0, 1.0]));
        dict.set_new_for(b"C0", numbers(&[first]));
        dict.set_new_for(b"C1", numbers(&[last]));
        dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
        dict
    }

    #[test]
    fn stitches_two_exponential_functions_at_bound() {
        let mut funcs = CpdfArray::new();
        funcs.append_new(exp(0.0, 1.0));
        funcs.append_new(exp(1.0, 0.0));
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(3));
        dict.set_new_for(b"Domain", numbers(&[0.0, 1.0]));
        dict.set_new_for(b"Functions", funcs);
        dict.set_new_for(b"Bounds", numbers(&[0.5]));
        dict.set_new_for(b"Encode", numbers(&[0.0, 1.0, 0.0, 1.0]));
        let function = CpdfFunction::load(&retain_object(dict)).unwrap();
        let mut result = [0.0];
        assert_eq!(function.call(&[0.1], &mut result), Some(1));
        assert!((result[0] - 0.2).abs() < 0.00001);
        assert_eq!(function.call(&[0.6], &mut result), Some(1));
        assert!((result[0] - 0.8).abs() < 0.00001);
    }

    #[test]
    fn rejects_self_referential_function_array() {
        let holder = CpdfIndirectObjectHolder::new();
        let root = holder.borrow_mut().new_indirect_dictionary();
        let mut functions = CpdfArray::new();
        functions.append_new(CpdfReference::new(&holder, root.borrow().obj_num()));
        {
            let mut borrowed = root.borrow_mut();
            let dict = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(3));
            dict.set_new_for(b"Domain", numbers(&[0.0, 1.0]));
            dict.set_new_for(b"Functions", functions);
            dict.set_new_for(b"Bounds", numbers(&[]));
            dict.set_new_for(b"Encode", numbers(&[0.0, 1.0]));
        }
        assert!(CpdfFunction::load(&root).is_none());
    }
}
