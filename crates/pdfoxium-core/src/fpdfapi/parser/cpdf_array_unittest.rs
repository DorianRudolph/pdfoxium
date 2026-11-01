// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_array_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
use crate::fpdfapi::parser::cpdf_object::{
    clone_direct_object, get_direct_object, retain_object, ObjectRef, PdfObject,
};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use std::rc::Rc;

#[test]
fn GetBooleanAt() {
    let mut arr = CpdfArray::new();
    arr.append_new(CpdfScalarObject::boolean(true));
    arr.append_new(CpdfScalarObject::boolean(false));
    arr.append_new(CpdfScalarObject::number_from_signed(100));
    arr.append_new(CpdfScalarObject::number_from_signed(0));
    assert_eq!(arr.size(), 4);
    assert!(arr.get_boolean_at(0, true));
    assert!(arr.get_boolean_at(0, false));
    assert!(!arr.get_boolean_at(1, true));
    assert!(!arr.get_boolean_at(1, false));
    assert!(arr.get_boolean_at(2, true));
    assert!(!arr.get_boolean_at(2, false));
    assert!(arr.get_boolean_at(3, true));
    assert!(!arr.get_boolean_at(3, false));
    assert!(arr.get_boolean_at(99, true));
    assert!(!arr.get_boolean_at(99, false));
}

#[test]
fn RemoveAt() {
    {
        let mut arr = CpdfArray::new();
        for elem in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
            arr.append_new(CpdfScalarObject::number_from_signed(elem));
        }
        for _ in 0..3 {
            arr.remove_at(3);
        }
        let expected = [1, 2, 3, 7, 8, 9, 10];
        assert_eq!(arr.size(), expected.len());
        for i in 0..expected.len() {
            assert_eq!(arr.get_integer_at(i), expected[i]);
        }
        arr.remove_at(4);
        arr.remove_at(4);
        let expected2 = [1, 2, 3, 7, 10];
        assert_eq!(arr.size(), expected2.len());
        for i in 0..expected2.len() {
            assert_eq!(arr.get_integer_at(i), expected2[i]);
        }
    }
    {
        let mut arr = CpdfArray::new();
        for elem in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
            arr.append_new(CpdfScalarObject::number_from_signed(elem));
        }
        assert_eq!(arr.size(), 10);
        arr.remove_at(11);
        assert_eq!(arr.size(), 10);
    }
}

#[test]
fn Clear() {
    let mut arr = CpdfArray::new();
    for elem in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
        arr.append_new(CpdfScalarObject::number_from_signed(elem));
    }
    assert_eq!(arr.size(), 10);
    arr.clear();
    assert_eq!(arr.size(), 0);
}

#[test]
fn SetAtBeyond() {
    let mut arr = CpdfArray::new();
    assert!(arr
        .set_new_at(0, CpdfScalarObject::number_from_signed(0))
        .is_none());
    assert!(arr
        .insert_new_at(0, CpdfScalarObject::number_from_signed(0))
        .is_some());
    assert!(arr
        .set_new_at(1, CpdfScalarObject::number_from_signed(0))
        .is_none());
}

#[test]
fn InsertAt() {
    let elems = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let mut arr = CpdfArray::new();
    for i in 0..elems.len() {
        arr.insert_new_at(i, CpdfScalarObject::number_from_signed(elems[i]));
    }
    assert_eq!(arr.size(), elems.len());
    for i in 0..elems.len() {
        assert_eq!(arr.get_integer_at(i), elems[i]);
    }
    arr.insert_new_at(3, CpdfScalarObject::number_from_signed(33));
    arr.insert_new_at(6, CpdfScalarObject::number_from_signed(55));
    arr.insert_new_at(12, CpdfScalarObject::number_from_signed(12));
    let expected = [1, 2, 3, 33, 4, 5, 55, 6, 7, 8, 9, 10, 12];
    assert_eq!(arr.size(), expected.len());
    for i in 0..expected.len() {
        assert_eq!(arr.get_integer_at(i), expected[i]);
    }
}

#[test]
fn InsertAtBeyond() {
    let mut arr = CpdfArray::new();
    assert!(arr
        .insert_new_at(1, CpdfScalarObject::number_from_signed(0))
        .is_none());
    assert!(arr
        .insert_new_at(0, CpdfScalarObject::number_from_signed(0))
        .is_some());
    assert!(arr
        .insert_new_at(2, CpdfScalarObject::number_from_signed(0))
        .is_none());
}

fn array_size(object: &ObjectRef) -> usize {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .unwrap()
        .size()
}
fn array_object(object: &ObjectRef, index: usize) -> ObjectRef {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .unwrap()
        .get_object_at(index)
        .unwrap()
}
fn array_integer(object: &ObjectRef, index: usize) -> i32 {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .unwrap()
        .get_integer_at(index)
}

#[test]
fn Clone() {
    {
        let elems = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mut arr = CpdfArray::new();
        for i in 0..elems.len() {
            arr.insert_new_at(i, CpdfScalarObject::number_from_signed(elems[i]));
        }
        let arr2 = arr.clone_object();
        assert_eq!(arr.size(), array_size(&arr2));
        for i in 0..elems.len() {
            assert!(!Rc::ptr_eq(
                &arr.get_object_at(i).unwrap(),
                &array_object(&arr2, i)
            ));
            assert_eq!(arr.get_integer_at(i), array_integer(&arr2, i));
        }
    }
    {
        const kNumOfRows: usize = 3;
        const kNumOfColumns: usize = 5;
        let elems = [[1, 2, 3, 4, 5], [10, 9, 8, 7, 6], [11, 12, 13, 14, 15]];
        let mut arr = CpdfArray::new();
        let obj_holder = CpdfIndirectObjectHolder::new();
        for i in 0..kNumOfRows {
            let mut arr_elem = CpdfArray::new();
            for j in 0..kNumOfColumns {
                let obj = retain_object(CpdfScalarObject::number_from_signed(elems[i][j]));
                let obj_num = (i * kNumOfColumns + j + 1) as u32;
                assert!(obj_holder
                    .borrow_mut()
                    .replace_indirect_object_if_higher_generation(obj_num, Some(obj)));
                arr_elem.insert_new_at(j, CpdfReference::new(&obj_holder, obj_num));
            }
            arr.insert_at(i, retain_object(arr_elem));
        }
        assert_eq!(arr.size(), kNumOfRows);
        let arr = retain_object(arr);
        let arr1 = arr.borrow().clone_object();
        assert_eq!(array_size(&arr), array_size(&arr1));
        let arr2 = clone_direct_object(&arr).unwrap();
        assert_eq!(array_size(&arr), array_size(&arr2));
        for i in 0..kNumOfRows {
            let arr_elem = array_object(&arr, i);
            let arr1_elem = array_object(&arr1, i);
            let arr2_elem = array_object(&arr2, i);
            assert!(!Rc::ptr_eq(&arr_elem, &arr1_elem));
            assert!(!Rc::ptr_eq(&arr_elem, &arr2_elem));
            for j in 0..kNumOfColumns {
                let elem_obj = array_object(&arr_elem, j);
                let elem_obj1 = array_object(&arr1_elem, j);
                let elem_obj2 = array_object(&arr2_elem, j);
                assert!(!Rc::ptr_eq(&elem_obj, &elem_obj1));
                assert_eq!(elem_obj1.borrow().object_type(), ObjectType::Reference);
                assert!(Rc::ptr_eq(
                    &get_direct_object(&elem_obj).unwrap(),
                    &get_direct_object(&elem_obj1).unwrap()
                ));
                assert_eq!(
                    elem_obj.borrow().get_integer(),
                    elem_obj1.borrow().get_integer()
                );
                assert!(!Rc::ptr_eq(&elem_obj, &elem_obj2));
                assert_eq!(elem_obj2.borrow().object_type(), ObjectType::Number);
                assert!(!Rc::ptr_eq(
                    &get_direct_object(&elem_obj).unwrap(),
                    &elem_obj2
                ));
                assert_eq!(elem_obj.borrow().obj_num(), elem_obj2.borrow().obj_num());
                assert_eq!(
                    elem_obj.borrow().get_integer(),
                    elem_obj2.borrow().get_integer()
                );
            }
        }
        drop(arr);
        assert_eq!(array_size(&arr1), kNumOfRows);
        for i in 0..kNumOfRows {
            for j in 0..kNumOfColumns {
                let elem_obj1 = array_object(&array_object(&arr1, i), j);
                assert_eq!(elem_obj1.borrow().object_type(), ObjectType::Reference);
                assert_eq!(elem_obj1.borrow().get_integer(), elems[i][j]);
                assert_eq!(array_integer(&array_object(&arr2, i), j), elems[i][j]);
            }
        }
    }
}

#[test]
fn Find() {
    let mut arr = CpdfArray::new();
    let dict0 = retain_object(CpdfDictionary::new());
    let dict1 = retain_object(CpdfDictionary::new());
    let dict2 = retain_object(CpdfDictionary::new());
    arr.append(Rc::clone(&dict0));
    arr.append(Rc::clone(&dict1));
    let maybe_found = arr.find(None);
    assert!(maybe_found.is_none());
    let maybe_found = arr.find(&dict0);
    assert!(maybe_found.is_some());
    assert_eq!(maybe_found.unwrap(), 0);
    let maybe_found = arr.find(&dict1);
    assert!(maybe_found.is_some());
    assert_eq!(maybe_found.unwrap(), 1);
    let maybe_found = arr.find(&dict2);
    assert!(maybe_found.is_none());
}

#[test]
fn Contains() {
    let mut arr = CpdfArray::new();
    let dict0 = retain_object(CpdfDictionary::new());
    let dict1 = retain_object(CpdfDictionary::new());
    let dict2 = retain_object(CpdfDictionary::new());
    arr.append(Rc::clone(&dict0));
    arr.append(Rc::clone(&dict1));
    assert!(arr.contains(&dict0));
    assert!(arr.contains(&dict1));
    assert!(!arr.contains(&dict2));
}

#[test]
fn Iterator() {
    let elems = [
        -23, -11, 3, 455, 2345877, 0, 7895330, -12564334, 10000, -100000,
    ];
    let mut arr = CpdfArray::new();
    for i in 0..elems.len() {
        arr.insert_new_at(i, CpdfScalarObject::number_from_signed(elems[i]));
    }
    let mut index = 0;
    arr.with_locked_objects(|locker| {
        for it in locker {
            assert_eq!(it.borrow().object_type(), ObjectType::Number);
            assert_eq!(it.borrow().get_integer(), elems[index]);
            index += 1;
        }
    });
    assert_eq!(index, elems.len());
}
