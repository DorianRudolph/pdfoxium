// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Transfer function cache from `core/fpdfapi/render/cpdf_docrenderdata.{h,cpp}`.
//! Weak transfer-function and Type 3 bitmap caches.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use super::cpdf_type3cache::CpdfType3Cache;
use crate::fpdfapi::font::cpdf_type3font::CpdfType3Font;
use crate::fpdfapi::page::cpdf_function::CpdfFunction;
use crate::fpdfapi::page::cpdf_transferfunc::{CpdfTransferFunc, CHANNEL_SAMPLE_SIZE};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;

const MAX_OUTPUTS: u32 = 16;

#[derive(Default)]
pub struct CpdfDocRenderData {
    transfer_func_map: RefCell<HashMap<usize, (ObjectRef, Weak<CpdfTransferFunc>)>>,
    type3_face_map: RefCell<HashMap<usize, Weak<CpdfType3Cache>>>,
}

impl CpdfDocRenderData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_cached_type3(&self, font: &Rc<CpdfType3Font>) -> Rc<CpdfType3Cache> {
        let identity = Rc::as_ptr(font) as usize;
        if let Some(cache) = self
            .type3_face_map
            .borrow()
            .get(&identity)
            .and_then(Weak::upgrade)
        {
            return cache;
        }
        let cache = Rc::new(CpdfType3Cache::new(Rc::clone(font)));
        self.type3_face_map
            .borrow_mut()
            .insert(identity, Rc::downgrade(&cache));
        cache
    }

    pub fn get_transfer_func(&self, object: &ObjectRef) -> Option<Rc<CpdfTransferFunc>> {
        let key = Rc::as_ptr(object) as *const () as usize;
        if let Some((_, cached)) = self.transfer_func_map.borrow().get(&key) {
            if let Some(func) = cached.upgrade() {
                return Some(func);
            }
        }
        let func = self.create_transfer_func(object).map(Rc::new);
        self.transfer_func_map.borrow_mut().insert(
            key,
            (
                object.clone(),
                func.as_ref().map_or_else(Weak::new, Rc::downgrade),
            ),
        );
        func
    }

    pub fn create_transfer_func(&self, object: &ObjectRef) -> Option<CpdfTransferFunc> {
        let array = {
            let borrowed = object.borrow();
            borrowed.as_any().downcast_ref::<CpdfArray>().map(|array| {
                if array.size() < 3 {
                    return None;
                }
                (0..array.size())
                    .take(3)
                    .map(|i| array.get_direct_object_at(i))
                    .collect::<Option<Vec<_>>>()
            })
        };
        let functions = if let Some(array) = array {
            array?
                .iter()
                .map(CpdfFunction::load)
                .collect::<Option<Vec<_>>>()?
        } else {
            vec![CpdfFunction::load(object)?]
        };

        let mut output = [0.0f32; MAX_OUTPUTS as usize];
        let mut samples_r = [0u8; CHANNEL_SAMPLE_SIZE];
        let mut samples_g = [0u8; CHANNEL_SAMPLE_SIZE];
        let mut samples_b = [0u8; CHANNEL_SAMPLE_SIZE];
        let mut identity = true;
        if functions.len() == 3 {
            for value in 0..CHANNEL_SAMPLE_SIZE {
                let input = value as f32 / 255.0;
                for (function, samples) in
                    functions
                        .iter()
                        .zip([&mut samples_r, &mut samples_g, &mut samples_b])
                {
                    if function.output_count() > MAX_OUTPUTS {
                        samples[value] = value as u8;
                        continue;
                    }
                    let _ = function.call(&[input], &mut output);
                    let result = (output[0] * 255.0).round().clamp(0.0, 255.0) as u8;
                    if result != value as u8 {
                        identity = false;
                    }
                    samples[value] = result;
                }
            }
        } else {
            let function = &functions[0];
            for value in 0..CHANNEL_SAMPLE_SIZE {
                let input = value as f32 / 255.0;
                if function.output_count() <= MAX_OUTPUTS {
                    let _ = function.call(&[input], &mut output);
                }
                let result = (output[0] * 255.0).round().clamp(0.0, 255.0) as u8;
                if result != value as u8 {
                    identity = false;
                }
                samples_r[value] = result;
                samples_g[value] = result;
                samples_b[value] = result;
            }
        }
        Some(CpdfTransferFunc::new(
            identity, samples_r, samples_g, samples_b,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn samples_transfer_function_and_reuses_live_cache_entry() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"Domain", float_array(&[0.0, 1.0]));
        dict.set_new_for(b"C0", float_array(&[0.0]));
        dict.set_new_for(b"C1", float_array(&[1.0]));
        dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
        let object = retain_object(dict);
        let cache = CpdfDocRenderData::new();
        let first = cache.get_transfer_func(&object).unwrap();
        assert!(first.identity());
        assert_eq!(first.samples_r()[0], 0);
        assert_eq!(first.samples_r()[255], 255);
        let second = cache.get_transfer_func(&object).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        drop(first);
        drop(second);
        assert!(cache.get_transfer_func(&object).is_some());
    }

    #[test]
    fn three_transfer_functions_produce_separate_channels() {
        let mut functions = CpdfArray::new();
        for (start, end) in [(0.0, 1.0), (1.0, 0.0), (0.5, 0.5)] {
            let mut dict = CpdfDictionary::new();
            dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
            dict.set_new_for(b"Domain", float_array(&[0.0, 1.0]));
            dict.set_new_for(b"C0", float_array(&[start]));
            dict.set_new_for(b"C1", float_array(&[end]));
            dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
            functions.append_new(dict);
        }
        let transfer = CpdfDocRenderData::new()
            .create_transfer_func(&retain_object(functions))
            .unwrap();
        assert!(!transfer.identity());
        assert_eq!(transfer.samples_r()[0], 0);
        assert_eq!(transfer.samples_g()[0], 255);
        assert_eq!(transfer.samples_b()[0], 128);
    }

    #[test]
    fn failed_transfer_load_retains_its_dictionary_key_until_teardown() {
        let object = retain_object(CpdfDictionary::new());
        let weak = Rc::downgrade(&object);
        let cache = CpdfDocRenderData::new();
        assert!(cache.get_transfer_func(&object).is_none());
        drop(object);
        assert!(weak.upgrade().is_some());
        drop(cache);
        assert!(weak.upgrade().is_none());
    }
}

#[cfg(test)]
#[path = "cpdf_docrenderdata_unittest.rs"]
mod upstream_tests;
