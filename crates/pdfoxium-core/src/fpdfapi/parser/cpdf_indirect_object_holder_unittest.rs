// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_indirect_object_holder_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CpdfIndirectObjectHolder;
use crate::fpdfapi::parser::cpdf_object::{retain_object, INVALID_OBJ_NUM};
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn RecursiveParseOfSameObject() {
    let mock_holder = CpdfIndirectObjectHolder::new();
    let calls = Rc::new(Cell::new(0));
    let weak = Rc::downgrade(&mock_holder);
    let call_count = Rc::clone(&calls);
    mock_holder
        .borrow_mut()
        .set_parser(Some(Rc::new(move |objnum| {
            call_count.set(call_count.get() + 1);
            assert_eq!(call_count.get(), 1);
            let mock_holder = weak.upgrade().unwrap();
            let same_parse =
                CpdfIndirectObjectHolder::get_or_parse_indirect_object(&mock_holder, objnum);
            assert!(same_parse.is_none());
            Some(retain_object(CpdfScalarObject::null()))
        })));
    assert!(CpdfIndirectObjectHolder::get_or_parse_indirect_object(&mock_holder, 1000).is_some());
    assert_eq!(calls.get(), 1);
}

#[test]
fn GetObjectMethods() {
    const kObjNum: u32 = 1000;
    let mock_holder = CpdfIndirectObjectHolder::new();
    let calls = Rc::new(Cell::new(0));
    let call_count = Rc::clone(&calls);
    mock_holder
        .borrow_mut()
        .set_parser(Some(Rc::new(move |_objnum| {
            call_count.set(call_count.get() + 1);
            Some(retain_object(CpdfScalarObject::null()))
        })));
    assert!(mock_holder.borrow().get_indirect_object(kObjNum).is_none());
    assert_eq!(calls.replace(0), 0);
    assert!(
        CpdfIndirectObjectHolder::get_or_parse_indirect_object(&mock_holder, kObjNum).is_some()
    );
    assert_eq!(calls.replace(0), 1);
    assert!(mock_holder.borrow().get_indirect_object(kObjNum).is_some());
    assert_eq!(calls.get(), 0);
    assert_eq!(
        kObjNum,
        mock_holder
            .borrow()
            .get_indirect_object(kObjNum)
            .unwrap()
            .borrow()
            .obj_num()
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn ParseInvalidObjNum() {
    let mock_holder = CpdfIndirectObjectHolder::new();
    mock_holder
        .borrow_mut()
        .set_parser(Some(Rc::new(|_| panic!("unexpected ParseIndirectObject"))));
    assert!(
        CpdfIndirectObjectHolder::get_or_parse_indirect_object(&mock_holder, INVALID_OBJ_NUM)
            .is_none()
    );
}

#[test]
fn ReplaceObjectWithInvalidObjNum() {
    let mock_holder = CpdfIndirectObjectHolder::new();
    mock_holder
        .borrow_mut()
        .set_parser(Some(Rc::new(|_| panic!("unexpected ParseIndirectObject"))));
    assert!(!mock_holder
        .borrow_mut()
        .replace_indirect_object_if_higher_generation(
            INVALID_OBJ_NUM,
            Some(retain_object(CpdfScalarObject::null()))
        ));
}

#[test]
fn TemplateNewMethods() {
    let mock_holder = CpdfIndirectObjectHolder::new();
    let dict = mock_holder.borrow_mut().new_indirect_dictionary();
    let array = mock_holder.borrow_mut().new_indirect_array();
    mock_holder
        .borrow_mut()
        .delete_indirect_object(dict.borrow().obj_num());
    mock_holder
        .borrow_mut()
        .delete_indirect_object(array.borrow().obj_num());
    // Retained values outlive their removal from the holder.
    assert_eq!(dict.borrow().object_type(), ObjectType::Dictionary);
    assert_eq!(array.borrow().object_type(), ObjectType::Array);
}
