// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/page/cpdf_psfunc.{h,cpp}`.

use std::cell::RefCell;

use super::cpdf_function::{CpdfFunctionData, FunctionType};
use super::cpdf_psengine::CpdfPsEngine;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;

pub struct CpdfPsFunc {
    base: CpdfFunctionData,
    engine: RefCell<CpdfPsEngine>,
}

impl CpdfPsFunc {
    pub fn load(object: &ObjectRef) -> Option<Self> {
        let base = CpdfFunctionData::init(object, FunctionType::PostScript)?;
        let borrowed = object.borrow();
        let stream = borrowed.as_any().downcast_ref::<CpdfStream>()?;
        let dict = stream.get_dict();
        if dict
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_integer_for(b"FunctionType", -1)
            != 4
        {
            return None;
        }
        let mut acc = CpdfStreamAcc::new(stream);
        acc.load_all_data_filtered();
        let mut engine = CpdfPsEngine::new();
        if !engine.parse(acc.get_span()) {
            return None;
        }
        Some(Self {
            base,
            engine: RefCell::new(engine),
        })
    }

    pub fn base(&self) -> &CpdfFunctionData {
        &self.base
    }
    pub fn call(&self, inputs: &[f32], results: &mut [f32]) -> Option<u32> {
        self.base.call(inputs, results, |inputs, results| {
            let mut engine = self.engine.borrow_mut();
            engine.reset();
            for &input in inputs {
                engine.push(input);
            }
            engine.execute();
            if engine.stack_size() < self.base.output_count() {
                return false;
            }
            for i in 0..self.base.output_count() as usize {
                results[self.base.output_count() as usize - i - 1] = engine.pop();
            }
            true
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfPsFunc;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

    #[test]
    fn executes_type_four_function_with_clamped_input() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(4));
        let mut domain = CpdfArray::new();
        domain.append_new(CpdfScalarObject::number_from_signed(0));
        domain.append_new(CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"Domain", domain);
        let mut range = CpdfArray::new();
        range.append_new(CpdfScalarObject::number_from_signed(0));
        range.append_new(CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"Range", range);
        let stream = retain_object(CpdfStream::from_data_and_dict(
            b"{ dup mul }".to_vec(),
            retain_object(dict),
        ));
        let function = CpdfPsFunc::load(&stream).unwrap();
        let mut result = [0.0];
        assert_eq!(function.call(&[0.5], &mut result), Some(1));
        assert_eq!(result, [0.25]);
        assert_eq!(function.call(&[2.0], &mut result), Some(1));
        assert_eq!(result, [1.0]);
    }
}
