// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_object_walker_unittest.cpp`.
#![allow(non_snake_case)]
use super::*;
use crate::fpdfapi::parser::{
    cpdf_indirect_object_holder::CpdfIndirectObjectHolder,
    cpdf_object::retain_object,
    cpdf_reference::CpdfReference,
    cpdf_scalar_object::{CpdfScalarObject, ObjectType},
    cpdf_string::CpdfString,
};
use std::rc::Rc;
fn Walk(object: ObjectRef) -> String {
    let mut result = String::new();
    let mut walker = CpdfObjectWalker::new(object);
    while let Some(obj) = walker.get_next() {
        result.push_str(match obj.borrow().object_type() {
            ObjectType::Dictionary => " Dict",
            ObjectType::Array => " Arr",
            ObjectType::String => " Str",
            ObjectType::Boolean => " Bool",
            ObjectType::Stream => " Stream",
            ObjectType::Reference => " Ref",
            ObjectType::Number => " Num",
            ObjectType::Null => " Null",
            _ => " Unknown",
        });
    }
    if !result.is_empty() {
        result.remove(0);
    }
    result
}
#[test]
fn Simple() {
    assert_eq!(Walk(retain_object(CpdfScalarObject::null())), "Null");
    assert_eq!(Walk(retain_object(CpdfDictionary::new())), "Dict");
    assert_eq!(Walk(retain_object(CpdfArray::new())), "Arr");
    assert_eq!(Walk(retain_object(CpdfString::new())), "Str");
    assert_eq!(
        Walk(retain_object(CpdfScalarObject::boolean_default())),
        "Bool"
    );
    assert_eq!(
        Walk(retain_object(CpdfStream::from_dict(retain_object(
            CpdfDictionary::new()
        )))),
        "Stream Dict Num"
    );
    assert_eq!(Walk(retain_object(CpdfReference::without_holder(0))), "Ref");
}
#[test]
fn CombinedObject() {
    let holder = CpdfIndirectObjectHolder::new();
    let mut dict = CpdfDictionary::new();
    dict.set_for(b"1", Some(retain_object(CpdfString::new())));
    dict.set_for(
        b"2",
        Some(retain_object(CpdfScalarObject::boolean_default())),
    );
    let mut array = CpdfArray::new();
    array.append(retain_object(CpdfReference::without_holder(0)));
    array.append(retain_object(CpdfScalarObject::null()));
    let stream = retain_object(CpdfStream::from_dict(retain_object(CpdfDictionary::new())));
    // NewIndirect returns a retained object in the selected safe holder API.
    holder.borrow_mut().add_indirect_object(stream.clone());
    let stream_object_number = stream.borrow().obj_num();
    assert!(stream_object_number > 0u32);
    array.append(retain_object(CpdfReference::new(
        &holder,
        stream_object_number,
    )));
    dict.set_for(b"3", Some(retain_object(array)));
    assert_eq!(Walk(retain_object(dict)), "Dict Str Bool Arr Ref Null Ref");
}
#[test]
fn GetParent() {
    let holder = CpdfIndirectObjectHolder::new();
    let level_4 = CpdfScalarObject::number_from_signed(0);
    let mut level_3 = CpdfDictionary::new();
    level_3.set_for(b"Length", Some(retain_object(level_4)));
    let level_2 = retain_object(CpdfStream::from_dict(retain_object(level_3)));
    holder.borrow_mut().add_indirect_object(level_2.clone());
    let level_2_object_number = level_2.borrow().obj_num();
    assert!(level_2_object_number > 0u32);
    let mut level_1 = CpdfArray::new();
    level_1.append(retain_object(CpdfReference::new(
        &holder,
        level_2_object_number,
    )));
    let mut level_0 = CpdfDictionary::new();
    level_0.set_for(b"Array", Some(retain_object(level_1)));
    // Each step increases depth, with the previous object as current parent.
    let mut cur_parent: Option<ObjectRef> = None;
    let mut walker = CpdfObjectWalker::new(retain_object(level_0));
    while let Some(obj) = walker.get_next() {
        match (cur_parent.as_ref(), walker.parent()) {
            (None, None) => {}
            (Some(expected), Some(actual)) => assert!(Rc::ptr_eq(expected, &actual)),
            _ => panic!("parent mismatch"),
        }
        cur_parent = Some(obj);
    }
}
#[test]
fn SkipWalkIntoCurrentObject() {
    let mut root_array = CpdfArray::new();
    // [null1, null2]
    root_array.append_new(CpdfScalarObject::null());
    root_array.append_new(CpdfScalarObject::null());
    // [null1, null2, [null3, null4]]
    // Clone is the operation under test in the native fixture.
    let cloned = crate::fpdfapi::parser::cpdf_object::PdfObject::clone_object(&root_array);
    root_array.append(cloned);
    let root_array = retain_object(root_array);
    let mut non_array_objects = 0;
    let mut walker = CpdfObjectWalker::new(root_array.clone());
    while let Some(obj) = walker.get_next() {
        if !Rc::ptr_eq(&obj, &root_array) && obj.borrow().object_type() == ObjectType::Array {
            walker.skip_walk_into_current_object();
        }
        if obj.borrow().object_type() != ObjectType::Array {
            non_array_objects += 1;
        }
    }
    assert_eq!(2, non_array_objects);
}
#[test]
fn DictionaryKey() {
    let mut dict = CpdfDictionary::new();
    dict.set_for(b"1", Some(retain_object(CpdfScalarObject::null())));
    dict.set_for(b"2", Some(retain_object(CpdfScalarObject::null())));
    dict.set_for(b"3", Some(retain_object(CpdfScalarObject::null())));
    dict.set_for(b"4", Some(retain_object(CpdfScalarObject::null())));
    dict.set_for(b"5", Some(retain_object(CpdfScalarObject::null())));
    let dict = retain_object(dict);
    let mut walker = CpdfObjectWalker::new(dict.clone());
    while let Some(obj) = walker.get_next() {
        if Rc::ptr_eq(&dict, &obj) {
            continue;
        }
        let parent = walker.parent().unwrap();
        let parent = parent.borrow();
        let expected = parent
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_object_for(walker.dictionary_key())
            .unwrap();
        assert!(Rc::ptr_eq(&expected, &obj));
    }
}
