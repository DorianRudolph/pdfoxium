// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/page/cpdf_function_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CpdfFunction;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

fn set_function_type(dict: &ObjectRef, value: i32) {
    dict.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(value));
}

#[test]
fn BadFunctionType() {
    let dict = retain_object(CpdfDictionary::new());
    set_function_type(&dict, -2);
    assert!(CpdfFunction::load(&dict).is_none());
    set_function_type(&dict, 5);
    assert!(CpdfFunction::load(&dict).is_none());
}

#[test]
fn NoDomain() {
    let dict = retain_object(CpdfDictionary::new());
    set_function_type(&dict, 0);
    assert!(CpdfFunction::load(&dict).is_none());
}

#[test]
fn EmptyDomain() {
    let dict = retain_object(CpdfDictionary::new());
    set_function_type(&dict, 0);
    dict.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_array_for(b"Domain");
    assert!(CpdfFunction::load(&dict).is_none());
}

#[test]
fn NoRange() {
    let dict = retain_object(CpdfDictionary::new());
    set_function_type(&dict, 0);
    let array = dict
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_array_for(b"Domain");
    {
        let mut borrowed = array.borrow_mut();
        let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        array.append_new(CpdfScalarObject::number_from_signed(0));
        array.append_new(CpdfScalarObject::number_from_signed(10));
    }
    assert!(CpdfFunction::load(&dict).is_none());
}
