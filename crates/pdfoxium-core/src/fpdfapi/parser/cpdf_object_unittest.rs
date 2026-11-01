// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_object_unittest.cpp`.
//! Scoped typed borrows replace native As* pointers; scalar types share the
//! selected tagged CpdfScalarObject representation. Rc clones retain handles.
#![allow(non_snake_case, non_upper_case_globals, non_camel_case_types)]
use super::*;
use crate::fpdfapi::parser::{
    cpdf_array::CpdfArray,
    cpdf_dictionary::CpdfDictionary,
    cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef},
    cpdf_name::CpdfName,
    cpdf_scalar_object::{CpdfScalarObject, ObjectType},
    cpdf_stream::CpdfStream,
    cpdf_stream_acc::CpdfStreamAcc,
    cpdf_string::CpdfString,
};
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use std::cell::{Ref, RefMut};
use std::collections::BTreeSet;
fn AddIndirect(holder: &HolderRef, object: ObjectRef) -> u32 {
    holder.borrow_mut().add_indirect_object(object)
}
fn N(n: i32) -> ObjectRef {
    retain_object(CpdfScalarObject::number_from_signed(n))
}
fn F(n: f32) -> ObjectRef {
    retain_object(CpdfScalarObject::number_from_float(n))
}
fn B(n: bool) -> ObjectRef {
    retain_object(CpdfScalarObject::boolean(n))
}
fn S(s: &[u8]) -> ObjectRef {
    retain_object(CpdfString::from_bytes(None, s))
}
fn U(s: &str) -> ObjectRef {
    retain_object(CpdfString::from_unicode(
        None,
        &s.chars().map(u32::from).collect::<Vec<_>>(),
    ))
}
fn Name(s: &[u8]) -> ObjectRef {
    retain_object(CpdfName::new(None, s))
}
fn Null() -> ObjectRef {
    retain_object(CpdfScalarObject::null())
}
fn D() -> ObjectRef {
    retain_object(CpdfDictionary::new())
}
fn A() -> ObjectRef {
    retain_object(CpdfArray::new())
}
fn Array(obj: &ObjectRef) -> Ref<'_, CpdfArray> {
    Ref::map(obj.borrow(), |o| o.as_any().downcast_ref().unwrap())
}
fn ArrayMut(obj: &ObjectRef) -> RefMut<'_, CpdfArray> {
    RefMut::map(obj.borrow_mut(), |o| o.as_any_mut().downcast_mut().unwrap())
}
fn Dict(obj: &ObjectRef) -> Ref<'_, CpdfDictionary> {
    Ref::map(obj.borrow(), |o| o.as_any().downcast_ref().unwrap())
}
fn DictMut(obj: &ObjectRef) -> RefMut<'_, CpdfDictionary> {
    RefMut::map(obj.borrow_mut(), |o| o.as_any_mut().downcast_mut().unwrap())
}
fn Stream(obj: &ObjectRef) -> Ref<'_, CpdfStream> {
    Ref::map(obj.borrow(), |o| o.as_any().downcast_ref().unwrap())
}
fn StreamMut(obj: &ObjectRef) -> RefMut<'_, CpdfStream> {
    RefMut::map(obj.borrow_mut(), |o| o.as_any_mut().downcast_mut().unwrap())
}
fn AssertPtr(expected: Option<&ObjectRef>, actual: Option<&ObjectRef>) {
    match (expected, actual) {
        (None, None) => {}
        (Some(e), Some(a)) => assert!(Rc::ptr_eq(e, a)),
        _ => panic!("null/non-null pointer mismatch"),
    }
}
// The casts used by production callers: concrete Any downcasts, with the
// scalar tag distinguishing native Boolean/Number/Null classes.
fn AsType(obj: &ObjectRef, kind: ObjectType) -> Option<ObjectRef> {
    let b = obj.borrow();
    let matches = match kind {
        ObjectType::Array => b.as_any().is::<CpdfArray>(),
        ObjectType::Boolean | ObjectType::Number | ObjectType::Null => {
            b.as_any().is::<CpdfScalarObject>() && b.object_type() == kind
        }
        ObjectType::Name => b.as_any().is::<CpdfName>(),
        ObjectType::String => b.as_any().is::<CpdfString>(),
        ObjectType::Dictionary => b.as_any().is::<CpdfDictionary>(),
        ObjectType::Stream => b.as_any().is::<CpdfStream>(),
        ObjectType::Reference => b.as_any().is::<CpdfReference>(),
    };
    matches.then(|| obj.clone())
}
#[allow(clippy::too_many_arguments)]
fn TestArrayAccessors(
    arr: &CpdfArray,
    index: usize,
    str_val: &[u8],
    _const_str_val: Option<&[u8]>,
    int_val: i32,
    float_val: f32,
    arr_val: Option<&ObjectRef>,
    dict_val: Option<&ObjectRef>,
    stream_val: Option<&ObjectRef>,
) {
    assert_eq!(str_val, arr.get_byte_string_at(index));
    assert_eq!(int_val, arr.get_integer_at(index));
    assert_eq!(float_val, arr.get_float_at(index));
    AssertPtr(arr_val, arr.get_array_at(index).as_ref());
    AssertPtr(dict_val, arr.get_dict_at(index).as_ref());
    AssertPtr(stream_val, arr.get_stream_at(index).as_ref());
}
fn Equal(obj1: Option<&ObjectRef>, obj2: Option<&ObjectRef>) -> bool {
    if obj1.zip(obj2).is_some_and(|(a, b)| Rc::ptr_eq(a, b)) {
        return true;
    }
    let (Some(obj1), Some(obj2)) = (obj1, obj2) else {
        return obj1.is_none() && obj2.is_none();
    };
    let (o1, o2) = (obj1.borrow(), obj2.borrow());
    if o1.object_type() != o2.object_type() {
        return false;
    }
    match o1.object_type() {
        ObjectType::Boolean => o1.get_integer() == o2.get_integer(),
        ObjectType::Number => {
            o1.as_any()
                .downcast_ref::<CpdfScalarObject>()
                .unwrap()
                .is_integer()
                == o2
                    .as_any()
                    .downcast_ref::<CpdfScalarObject>()
                    .unwrap()
                    .is_integer()
                && o1.get_integer() == o2.get_integer()
        }
        ObjectType::String | ObjectType::Name => o1.get_string() == o2.get_string(),
        ObjectType::Array => {
            let (a1, a2) = (
                o1.as_any().downcast_ref::<CpdfArray>().unwrap(),
                o2.as_any().downcast_ref::<CpdfArray>().unwrap(),
            );
            if a1.size() != a2.size() {
                return false;
            }
            for i in 0..a1.size() {
                if !Equal(a1.get_object_at(i).as_ref(), a2.get_object_at(i).as_ref()) {
                    return false;
                }
            }
            true
        }
        ObjectType::Dictionary => {
            let (d1, d2) = (
                o1.as_any().downcast_ref::<CpdfDictionary>().unwrap(),
                o2.as_any().downcast_ref::<CpdfDictionary>().unwrap(),
            );
            if d1.size() != d2.size() {
                return false;
            }
            d1.with_locked_entries(|entries| {
                for (key, value) in entries {
                    if !Equal(Some(value), d2.get_object_for(key).as_ref()) {
                        return false;
                    }
                }
                true
            })
        }
        ObjectType::Null => true,
        ObjectType::Stream => {
            let (s1, s2) = (
                o1.as_any().downcast_ref::<CpdfStream>().unwrap(),
                o2.as_any().downcast_ref::<CpdfStream>().unwrap(),
            );
            if !Equal(Some(&s1.get_dict()), Some(&s2.get_dict())) {
                return false;
            }
            let mut streamAcc1 = CpdfStreamAcc::new(s1);
            streamAcc1.load_all_data_raw();
            let mut streamAcc2 = CpdfStreamAcc::new(s2);
            streamAcc2.load_all_data_raw();
            let (span1, span2) = (streamAcc1.get_span(), streamAcc2.get_span());
            if span1.len() != span2.len() {
                return false;
            }
            span1 == span2
        }
        ObjectType::Reference => {
            o1.as_any()
                .downcast_ref::<CpdfReference>()
                .unwrap()
                .ref_obj_num()
                == o2
                    .as_any()
                    .downcast_ref::<CpdfReference>()
                    .unwrap()
                    .ref_obj_num()
        }
    }
}
struct PDFObjectsTest {
    // Reverse native declaration order: Rust destroys fields in declaration order.
    indirect_obj_nums_: Vec<u32>,
    array_obj_: ObjectRef,
    stream_dict_obj_: ObjectRef,
    dict_obj_: ObjectRef,
    ref_objs_: Vec<ObjectRef>,
    direct_obj_types_: Vec<ObjectType>,
    direct_objs_: Vec<ObjectRef>,
    obj_holder_: HolderRef,
}
impl PDFObjectsTest {
    fn SetUp() -> Self {
        let boolean_false_obj = B(false);
        let boolean_true_obj = B(true);
        let number_int_obj = N(1245);
        let number_float_obj = F(9.00345f32);
        let str_reg_obj = U("A simple test");
        let str_spec_obj = U("\t\n");
        let name_obj = Name(b"space");
        let array_obj_ = A();
        ArrayMut(&array_obj_).insert_at(0, N(8902));
        ArrayMut(&array_obj_).insert_at(1, Name(b"address"));
        let dict_obj_ = D();
        DictMut(&dict_obj_).set_for(b"bool", Some(B(false)));
        DictMut(&dict_obj_).set_for(b"num", Some(F(0.23f32)));
        const kContents: &[u8] = b"abcdefghijklmnopqrstuvwxyz\0";
        let pNewDict = D();
        let stream_dict_obj_ = pNewDict.clone();
        DictMut(&stream_dict_obj_).set_for(b"key1", Some(U(" test dict")));
        DictMut(&stream_dict_obj_).set_for(b"key2", Some(N(-1)));
        let stream_obj = retain_object(CpdfStream::from_data_and_dict(
            kContents[..kContents.len() - 1].to_vec(),
            pNewDict,
        ));
        let null_obj = Null();
        let direct_objs_ = vec![
            boolean_false_obj,
            boolean_true_obj.clone(),
            number_int_obj.clone(),
            number_float_obj,
            str_reg_obj,
            str_spec_obj.clone(),
            name_obj.clone(),
            array_obj_.clone(),
            dict_obj_.clone(),
            stream_obj.clone(),
            null_obj,
        ];
        let direct_obj_types_ = vec![
            ObjectType::Boolean,
            ObjectType::Boolean,
            ObjectType::Number,
            ObjectType::Number,
            ObjectType::String,
            ObjectType::String,
            ObjectType::Name,
            ObjectType::Array,
            ObjectType::Dictionary,
            ObjectType::Stream,
            ObjectType::Null,
        ];
        let obj_holder_ = CpdfIndirectObjectHolder::new();
        let indirect_obj_nums_ = vec![
            AddIndirect(&obj_holder_, boolean_true_obj.borrow().clone_object()),
            AddIndirect(&obj_holder_, number_int_obj.borrow().clone_object()),
            AddIndirect(&obj_holder_, str_spec_obj.borrow().clone_object()),
            AddIndirect(&obj_holder_, name_obj.borrow().clone_object()),
            AddIndirect(&obj_holder_, array_obj_.borrow().clone_object()),
            AddIndirect(&obj_holder_, dict_obj_.borrow().clone_object()),
            AddIndirect(&obj_holder_, stream_obj.borrow().clone_object()),
        ];
        let mut ref_objs_ = Vec::new();
        for &objnum in &indirect_obj_nums_ {
            ref_objs_.push(retain_object(CpdfReference::new(&obj_holder_, objnum)));
        }
        Self {
            indirect_obj_nums_,
            array_obj_,
            stream_dict_obj_,
            dict_obj_,
            ref_objs_,
            direct_obj_types_,
            direct_objs_,
            obj_holder_,
        }
    }
}
mod PDFObjectsTestCases {
    use super::*;
    #[test]
    fn GetString() {
        let f = PDFObjectsTest::SetUp();
        let direct_obj_results: &[&[u8]] = &[
            b"false",
            b"true",
            b"1245",
            b"9.00345",
            b"A simple test",
            b"\t\n",
            b"space",
            b"",
            b"",
            b"",
            b"",
        ];
        for i in 0..f.direct_objs_.len() {
            assert_eq!(
                direct_obj_results[i],
                f.direct_objs_[i].borrow().get_string()
            );
        }
        let indirect_obj_results: &[&[u8]] = &[b"true", b"1245", b"\t\n", b"space", b"", b"", b""];
        for i in 0..f.ref_objs_.len() {
            assert_eq!(
                indirect_obj_results[i],
                f.ref_objs_[i].borrow().get_string()
            );
        }
    }
    #[test]
    fn GetUnicodeText() {
        let f = PDFObjectsTest::SetUp();
        let direct_obj_results = [
            "",
            "",
            "",
            "",
            "A simple test",
            "\t\n",
            "space",
            "",
            "",
            "abcdefghijklmnopqrstuvwxyz",
            "",
        ];
        for i in 0..f.direct_objs_.len() {
            crate::test_support::assert_wide_cstr_eq(
                &direct_obj_results[i]
                    .chars()
                    .map(u32::from)
                    .collect::<Vec<_>>(),
                &f.direct_objs_[i].borrow().get_unicode_text(),
            );
        }
        for it in &f.ref_objs_ {
            crate::test_support::assert_wide_cstr_eq(&[], &it.borrow().get_unicode_text());
        }
    }
    #[test]
    fn GetNumber() {
        let f = PDFObjectsTest::SetUp();
        let direct_obj_results: [f32; 11] = [0., 0., 1245., 9.00345f32, 0., 0., 0., 0., 0., 0., 0.];
        for i in 0..f.direct_objs_.len() {
            assert_eq!(
                direct_obj_results[i],
                f.direct_objs_[i].borrow().get_number()
            );
        }
        let indirect_obj_results: [f32; 7] = [0., 1245., 0., 0., 0., 0., 0.];
        for i in 0..f.ref_objs_.len() {
            assert_eq!(
                indirect_obj_results[i],
                f.ref_objs_[i].borrow().get_number()
            );
        }
    }
    #[test]
    fn GetInteger() {
        let f = PDFObjectsTest::SetUp();
        let direct_obj_results = [0, 1, 1245, 9, 0, 0, 0, 0, 0, 0, 0];
        for i in 0..f.direct_objs_.len() {
            assert_eq!(
                direct_obj_results[i],
                f.direct_objs_[i].borrow().get_integer()
            );
        }
        let indirect_obj_results = [1, 1245, 0, 0, 0, 0, 0];
        for i in 0..f.ref_objs_.len() {
            assert_eq!(
                indirect_obj_results[i],
                f.ref_objs_[i].borrow().get_integer()
            );
        }
    }
    #[test]
    fn GetDict() {
        let f = PDFObjectsTest::SetUp();
        let direct_obj_results = [
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&f.dict_obj_),
            Some(&f.stream_dict_obj_),
            None,
        ];
        for i in 0..f.direct_objs_.len() {
            AssertPtr(
                direct_obj_results[i],
                get_dict_object(&f.direct_objs_[i]).as_ref(),
            );
        }
        let indirect_obj_results = [
            None,
            None,
            None,
            None,
            None,
            Some(&f.dict_obj_),
            Some(&f.stream_dict_obj_),
        ];
        for i in 0..f.ref_objs_.len() {
            assert!(Equal(
                indirect_obj_results[i],
                get_dict_object(&f.ref_objs_[i]).as_ref()
            ));
        }
    }
    #[test]
    fn GetNameFor() {
        let f = PDFObjectsTest::SetUp();
        DictMut(&f.dict_obj_).set_for(b"string", Some(S(b"ium")));
        DictMut(&f.dict_obj_).set_for(b"name", Some(Name(b"Pdf")));
        assert_eq!(b"", Dict(&f.dict_obj_).get_name_for(b"invalid").as_slice());
        assert_eq!(b"", Dict(&f.dict_obj_).get_name_for(b"bool").as_slice());
        assert_eq!(b"", Dict(&f.dict_obj_).get_name_for(b"num").as_slice());
        assert_eq!(b"", Dict(&f.dict_obj_).get_name_for(b"string").as_slice());
        assert_eq!(b"Pdf", Dict(&f.dict_obj_).get_name_for(b"name").as_slice());
        assert_eq!(
            b"",
            Dict(&f.dict_obj_)
                .get_byte_string_for(b"invalid", b"")
                .as_slice()
        );
        assert_eq!(
            b"false",
            Dict(&f.dict_obj_)
                .get_byte_string_for(b"bool", b"")
                .as_slice()
        );
        assert_eq!(
            b".23",
            Dict(&f.dict_obj_)
                .get_byte_string_for(b"num", b"")
                .as_slice()
        );
        assert_eq!(
            b"ium",
            Dict(&f.dict_obj_)
                .get_byte_string_for(b"string", b"")
                .as_slice()
        );
        assert_eq!(
            b"Pdf",
            Dict(&f.dict_obj_)
                .get_byte_string_for(b"name", b"")
                .as_slice()
        );
    }
    #[test]
    fn GetArray() {
        let f = PDFObjectsTest::SetUp();
        let direct_obj_results = [
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&f.array_obj_),
            None,
            None,
            None,
        ];
        for i in 0..f.direct_objs_.len() {
            AssertPtr(
                direct_obj_results[i],
                AsType(&f.direct_objs_[i], ObjectType::Array).as_ref(),
            );
        }
        for it in &f.ref_objs_ {
            assert!(AsType(it, ObjectType::Array).is_none());
        }
    }
    #[test]
    fn Clone() {
        let f = PDFObjectsTest::SetUp();
        for i in 0..f.direct_objs_.len() {
            let obj = f.direct_objs_[i].borrow().clone_object();
            assert!(Equal(Some(&f.direct_objs_[i]), Some(&obj)));
        }
        for it in &f.ref_objs_ {
            let obj = it.borrow().clone_object();
            assert!(Equal(Some(it), Some(&obj)));
        }
    }
    #[test]
    fn GetType() {
        let f = PDFObjectsTest::SetUp();
        for i in 0..f.direct_objs_.len() {
            assert_eq!(
                f.direct_obj_types_[i],
                f.direct_objs_[i].borrow().object_type()
            );
        }
        for it in &f.ref_objs_ {
            assert_eq!(ObjectType::Reference, it.borrow().object_type());
        }
    }
    #[test]
    fn GetDirect() {
        let f = PDFObjectsTest::SetUp();
        for i in 0..f.direct_objs_.len() {
            AssertPtr(
                Some(&f.direct_objs_[i]),
                get_direct_object(&f.direct_objs_[i]).as_ref(),
            );
        }
        for i in 0..f.ref_objs_.len() {
            assert_eq!(
                f.indirect_obj_nums_[i],
                get_direct_object(&f.ref_objs_[i])
                    .unwrap()
                    .borrow()
                    .obj_num()
            );
        }
    }
    #[test]
    fn SetString() {
        let f = PDFObjectsTest::SetUp();
        let set_values: &[&[u8]] = &[
            b"true", b"fake", b"3.125f", b"097", b"changed", b"", b"NewName",
        ];
        let expected: &[&[u8]] = &[
            b"true", b"false", b"3.125", b"97", b"changed", b"", b"NewName",
        ];
        for i in 0..set_values.len() {
            f.direct_objs_[i].borrow_mut().set_string(set_values[i]);
            assert_eq!(expected[i], f.direct_objs_[i].borrow().get_string());
        }
    }
    #[test]
    fn IsTypeAndAsType() {
        let f = PDFObjectsTest::SetUp();
        for i in 0..f.direct_objs_.len() {
            for kind in [
                ObjectType::Array,
                ObjectType::Boolean,
                ObjectType::Name,
                ObjectType::Number,
                ObjectType::String,
                ObjectType::Dictionary,
                ObjectType::Stream,
            ] {
                if f.direct_obj_types_[i] == kind {
                    assert_eq!(f.direct_objs_[i].borrow().object_type(), kind);
                    AssertPtr(
                        Some(&f.direct_objs_[i]),
                        AsType(&f.direct_objs_[i], kind).as_ref(),
                    );
                } else {
                    assert_ne!(f.direct_objs_[i].borrow().object_type(), kind);
                    assert!(AsType(&f.direct_objs_[i], kind).is_none());
                }
            }
            assert_ne!(
                f.direct_objs_[i].borrow().object_type(),
                ObjectType::Reference
            );
            assert!(AsType(&f.direct_objs_[i], ObjectType::Reference).is_none());
        }
        for i in 0..f.ref_objs_.len() {
            assert_eq!(f.ref_objs_[i].borrow().object_type(), ObjectType::Reference);
            AssertPtr(
                Some(&f.ref_objs_[i]),
                AsType(&f.ref_objs_[i], ObjectType::Reference).as_ref(),
            );
        }
    }
    #[test]
    fn MakeReferenceGeneric() {
        let f = PDFObjectsTest::SetUp();
        let original_obj = Null();
        original_obj.borrow_mut().set_obj_num(42);
        assert!(!original_obj.borrow().is_inline());
        let ref_obj = make_reference(&original_obj, &f.obj_holder_);
        assert_eq!(ObjectType::Reference, ref_obj.borrow().object_type());
        assert_eq!(
            original_obj.borrow().obj_num(),
            ref_obj
                .borrow()
                .as_any()
                .downcast_ref::<CpdfReference>()
                .unwrap()
                .ref_obj_num()
        );
    }
    #[test]
    fn KeyForCache() {
        let f = PDFObjectsTest::SetUp();
        let mut key_set = BTreeSet::new();
        for direct in &f.direct_objs_ {
            assert!(key_set.insert(key_for_cache(direct)));
        }
        for (_, object) in f.obj_holder_.borrow().iter() {
            assert!(key_set.insert(key_for_cache(object.as_ref().unwrap())));
        }
        assert_eq!(18usize, key_set.len());
    }
}
mod PDFArrayTest {
    use super::*;
    #[test]
    fn GetMatrix() {
        let elems: [[f32; 6]; 4] = [
            [0., 0., 0., 0., 0., 0.],
            [1., 2., 3., 4., 5., 6.],
            [2.3, 4.05, 3., -2., -3., 0.],
            [0.05, 0.1, 0.56, 0.67, 1.34, 99.9],
        ];
        for elem in elems {
            let mut arr = CpdfArray::new();
            for f in elem {
                arr.append(F(f));
            }
            let matrix = CfxMatrix::new(elem[0], elem[1], elem[2], elem[3], elem[4], elem[5]);
            let arr_matrix = arr.get_matrix();
            assert_eq!(matrix, arr_matrix);
        }
    }
    #[test]
    fn GetRect() {
        let elems: [[f32; 4]; 4] = [
            [0., 0., 0., 0.],
            [1., 2., 5., 6.],
            [2.3, 4.05, -3., 0.],
            [0.05, 0.1, 1.34, 99.9],
        ];
        for elem in elems {
            let mut arr = CpdfArray::new();
            for f in elem {
                arr.append(F(f));
            }
            let rect = CfxFloatRect::new(elem[0], elem[1], elem[2], elem[3]);
            let arr_rect = arr.get_rect();
            assert_eq!(rect, arr_rect);
        }
    }
    #[test]
    fn GetTypeAt() {
        {
            // Boolean array.
            let vals = [true, false, false, true, true];
            let mut arr = CpdfArray::new();
            for i in 0..vals.len() {
                arr.insert_at(i, B(vals[i]));
            }
            for i in 0..vals.len() {
                TestArrayAccessors(
                    &arr,
                    i,
                    if vals[i] { b"true" } else { b"false" },
                    None,
                    if vals[i] { 1 } else { 0 },
                    0.,
                    None,
                    None,
                    None,
                );
            }
        }
        {
            // Integer array.
            let vals = [10, 0, -345, 2089345456, -1000000000, 567, 93658767];
            let mut arr = CpdfArray::new();
            for i in 0..vals.len() {
                arr.insert_at(i, N(vals[i]));
            }
            for i in 0..vals.len() {
                let mut buf = [0u8; 33];
                let str_val = crate::fxcrt::fx_system::itoa(vals[i], &mut buf, 10);
                TestArrayAccessors(
                    &arr,
                    i,
                    str_val,
                    None,
                    vals[i],
                    vals[i] as f32,
                    None,
                    None,
                    None,
                );
            }
        }
        {
            // Float array.
            let vals: [f32; 10] = [0., 0., 10., 10., 0.0345, 897.34, -2.5, -1., -345., -0.];
            let mut arr = CpdfArray::new();
            for i in 0..vals.len() {
                arr.insert_at(i, F(vals[i]));
            }
            let expected_str: &[&[u8]] = &[
                b"0", b"0", b"10", b"10", b".0345", b"897.34", b"-2.5", b"-1", b"-345", b"0",
            ];
            for i in 0..vals.len() {
                TestArrayAccessors(
                    &arr,
                    i,
                    expected_str[i],
                    None,
                    vals[i] as i32,
                    vals[i],
                    None,
                    None,
                    None,
                );
            }
        }
        {
            // String and name array.
            let vals: &[&[u8]] = &[
                b"this",
                b"adsde$%^",
                b"\r\t",
                b"\"012",
                b".",
                b"EYREW",
                b"It is a joke :)",
            ];
            let mut string_array = CpdfArray::new();
            let mut name_array = CpdfArray::new();
            for i in 0..vals.len() {
                string_array.insert_at(i, S(vals[i]));
                name_array.insert_at(i, Name(vals[i]));
            }
            for i in 0..vals.len() {
                TestArrayAccessors(
                    &string_array,
                    i,
                    vals[i],
                    Some(vals[i]),
                    0,
                    0.,
                    None,
                    None,
                    None,
                );
                TestArrayAccessors(
                    &name_array,
                    i,
                    vals[i],
                    Some(vals[i]),
                    0,
                    0.,
                    None,
                    None,
                    None,
                );
            }
        }
        {
            // Null element array.
            let mut arr = CpdfArray::new();
            for i in 0..3 {
                arr.insert_at(i, Null());
            }
            for i in 0..3 {
                TestArrayAccessors(&arr, i, b"", None, 0, 0., None, None, None);
            }
        }
        {
            // Array of array.
            let mut vals: [Option<ObjectRef>; 3] = [None, None, None];
            let mut arr = CpdfArray::new();
            for i in 0..3 {
                vals[i] = Some(arr.append_new(CpdfArray::new()));
                for j in 0..3 {
                    let value = j as i32 + 100;
                    ArrayMut(vals[i].as_ref().unwrap()).insert_at(j, N(value));
                }
            }
            for i in 0..3 {
                TestArrayAccessors(&arr, i, b"", None, 0, 0., vals[i].as_ref(), None, None);
            }
        }
        {
            // Dictionary array.
            let mut vals: [Option<ObjectRef>; 3] = [None, None, None];
            let mut arr = CpdfArray::new();
            for i in 0..3 {
                vals[i] = Some(arr.append_new(CpdfDictionary::new()));
                for j in 0..3 {
                    let mut key = b"key".to_vec();
                    let mut buf = [0u8; 33];
                    key.extend_from_slice(crate::fxcrt::fx_system::itoa(j as i32, &mut buf, 10));
                    let value = j as i32 + 200;
                    DictMut(vals[i].as_ref().unwrap()).set_for(&key, Some(N(value)));
                }
            }
            for i in 0..3 {
                TestArrayAccessors(&arr, i, b"", None, 0, 0., None, vals[i].as_ref(), None);
            }
        }
        {
            // Stream array.
            let object_holder = CpdfIndirectObjectHolder::new();
            let mut vals: [Option<ObjectRef>; 3] = [None, None, None];
            let mut stream_vals: [Option<ObjectRef>; 3] = [None, None, None];
            let mut arr = CpdfArray::new();
            for i in 0..3 {
                vals[i] = Some(D());
                for j in 0..3 {
                    let mut key = b"key".to_vec();
                    let mut buf = [0u8; 33];
                    key.extend_from_slice(crate::fxcrt::fx_system::itoa(j as i32, &mut buf, 10));
                    let value = j as i32 + 200;
                    DictMut(vals[i].as_ref().unwrap()).set_for(&key, Some(N(value)));
                }
                // Native begin/end includes the C array terminator.
                const kContents: &[u8] = b"content: this is a stream\0";
                let stream = retain_object(CpdfStream::from_data_and_dict(
                    kContents.to_vec(),
                    vals[i].as_ref().unwrap().clone(),
                ));
                object_holder
                    .borrow_mut()
                    .add_indirect_object(stream.clone());
                stream_vals[i] = Some(stream);
                arr.append_new(CpdfReference::new(
                    &object_holder,
                    stream_vals[i].as_ref().unwrap().borrow().obj_num(),
                ));
            }
            for i in 0..3 {
                TestArrayAccessors(
                    &arr,
                    i,
                    b"",
                    None,
                    0,
                    0.,
                    None,
                    vals[i].as_ref(),
                    stream_vals[i].as_ref(),
                );
            }
        }
        {
            // Mixed array.
            let object_holder = CpdfIndirectObjectHolder::new();
            let mut arr = CpdfArray::new();
            arr.insert_at(0, B(true));
            arr.insert_at(1, B(false));
            arr.insert_at(2, N(0));
            arr.insert_at(3, N(-1234));
            arr.insert_at(4, F(2345.0));
            arr.insert_at(5, F(0.05));
            arr.insert_at(6, S(b""));
            arr.insert_at(7, S(b"It is a test!"));
            arr.insert_at(8, Name(b"NAME"));
            arr.insert_at(9, Name(b"test"));
            arr.insert_at(10, Null());
            let arr_val = arr.insert_new_at(11, CpdfArray::new()).unwrap();
            ArrayMut(&arr_val).append(N(1));
            ArrayMut(&arr_val).append(N(2));
            let dict_val = arr.insert_new_at(12, CpdfDictionary::new()).unwrap();
            DictMut(&dict_val).set_for(b"key1", Some(S(b"Linda")));
            DictMut(&dict_val).set_for(b"key2", Some(S(b"Zoe")));
            let stream_dict = D();
            DictMut(&stream_dict).set_for(b"key1", Some(S(b"John")));
            DictMut(&stream_dict).set_for(b"key2", Some(S(b"King")));
            const kData: &[u8] = b"A stream for test\0";
            let stream_val = retain_object(CpdfStream::from_data_and_dict(
                kData.to_vec(),
                stream_dict.clone(),
            ));
            object_holder
                .borrow_mut()
                .add_indirect_object(stream_val.clone());
            arr.insert_new_at(
                13,
                CpdfReference::new(&object_holder, stream_val.borrow().obj_num()),
            );
            let expected_str: &[&[u8]] = &[
                b"true",
                b"false",
                b"0",
                b"-1234",
                b"2345",
                b".05",
                b"",
                b"It is a test!",
                b"NAME",
                b"test",
                b"",
                b"",
                b"",
                b"",
            ];
            let expected_int = [1, 0, 0, -1234, 2345, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            let expected_float: [f32; 14] = [
                0., 0., 0., -1234., 2345., 0.05, 0., 0., 0., 0., 0., 0., 0., 0.,
            ];
            for i in 0..arr.size() {
                assert_eq!(expected_str[i], arr.get_byte_string_at(i));
                assert_eq!(expected_int[i], arr.get_integer_at(i));
                assert_eq!(expected_float[i], arr.get_float_at(i));
                if i == 11 {
                    AssertPtr(Some(&arr_val), arr.get_array_at(i).as_ref());
                } else {
                    assert!(arr.get_array_at(i).is_none());
                }
                if i == 13 {
                    AssertPtr(Some(&stream_dict), arr.get_dict_at(i).as_ref());
                    AssertPtr(Some(&stream_val), arr.get_stream_at(i).as_ref());
                } else {
                    assert!(arr.get_stream_at(i).is_none());
                    if i == 12 {
                        AssertPtr(Some(&dict_val), arr.get_dict_at(i).as_ref());
                    } else {
                        assert!(arr.get_dict_at(i).is_none());
                    }
                }
            }
        }
    }
    #[test]
    fn AddNumber() {
        let vals: [f32; 8] = [1., -1., 0., 0.456734, 12345.54321, 0.5, 1000., 0.000045];
        let mut arr = CpdfArray::new();
        for i in 0..vals.len() {
            arr.append(F(vals[i]));
        }
        for i in 0..vals.len() {
            assert_eq!(
                ObjectType::Number,
                arr.get_object_at(i).unwrap().borrow().object_type()
            );
            assert_eq!(vals[i], arr.get_object_at(i).unwrap().borrow().get_number());
        }
    }
    #[test]
    fn AddInteger() {
        let vals = [0, 1, 934435456, 876, 10000, -1, -24354656, -100];
        let mut arr = CpdfArray::new();
        for i in 0..vals.len() {
            arr.append(N(vals[i]));
        }
        for i in 0..vals.len() {
            assert_eq!(
                ObjectType::Number,
                arr.get_object_at(i).unwrap().borrow().object_type()
            );
            assert_eq!(
                vals[i] as f32,
                arr.get_object_at(i).unwrap().borrow().get_number()
            );
        }
    }
    #[test]
    fn AddStringAndName() {
        let kVals: &[&[u8]] = &[
            b"",
            b"a",
            b"ehjhRIOYTTFdfcdnv",
            b"122323",
            b"$#%^&**",
            b" ",
            b"This is a test.\r\n",
        ];
        let mut string_array = CpdfArray::new();
        let mut name_array = CpdfArray::new();
        for val in kVals {
            string_array.append(S(val));
            name_array.append(Name(val));
        }
        for i in 0..kVals.len() {
            assert_eq!(
                ObjectType::String,
                string_array
                    .get_object_at(i)
                    .unwrap()
                    .borrow()
                    .object_type()
            );
            assert_eq!(
                kVals[i],
                string_array.get_object_at(i).unwrap().borrow().get_string()
            );
            assert_eq!(
                ObjectType::Name,
                name_array.get_object_at(i).unwrap().borrow().object_type()
            );
            assert_eq!(
                kVals[i],
                name_array.get_object_at(i).unwrap().borrow().get_string()
            );
        }
    }
    #[test]
    fn AddReferenceAndGetObjectAt() {
        let holder = CpdfIndirectObjectHolder::new();
        let boolean_obj = B(true);
        let int_obj = N(-1234);
        let float_obj = F(2345.089);
        let str_obj = S(b"Adsfdsf 343434 %&&*\n");
        let name_obj = Name(b"Title:");
        let null_obj = Null();
        let indirect_objs = [boolean_obj, int_obj, float_obj, str_obj, name_obj, null_obj];
        let obj_nums = [2, 4, 7, 2345, 799887, 1];
        let mut arr = CpdfArray::new();
        let mut arr1 = CpdfArray::new();
        for i in 0..indirect_objs.len() {
            assert!(holder
                .borrow_mut()
                .replace_indirect_object_if_higher_generation(
                    obj_nums[i],
                    Some(indirect_objs[i].clone())
                ));
            arr.append_new(CpdfReference::new(&holder, obj_nums[i]));
            arr1.append_new(CpdfReference::new(
                &holder,
                indirect_objs[i].borrow().obj_num(),
            ));
        }
        for i in 0..obj_nums.len() {
            AssertPtr(
                Some(&indirect_objs[i]),
                CpdfIndirectObjectHolder::get_or_parse_indirect_object(&holder, obj_nums[i])
                    .as_ref(),
            );
        }
        assert_eq!(arr.size(), arr1.size());
        for i in 0..arr.size() {
            assert_eq!(
                ObjectType::Reference,
                arr.get_object_at(i).unwrap().borrow().object_type()
            );
            AssertPtr(
                Some(&indirect_objs[i]),
                get_direct_object(&arr.get_object_at(i).unwrap()).as_ref(),
            );
            AssertPtr(
                Some(&indirect_objs[i]),
                arr.get_direct_object_at(i).as_ref(),
            );
            assert_eq!(
                ObjectType::Reference,
                arr1.get_object_at(i).unwrap().borrow().object_type()
            );
            AssertPtr(
                Some(&indirect_objs[i]),
                get_direct_object(&arr1.get_object_at(i).unwrap()).as_ref(),
            );
            AssertPtr(
                Some(&indirect_objs[i]),
                arr1.get_direct_object_at(i).as_ref(),
            );
        }
    }
    #[test]
    fn PDFArrayTest_CloneDirectObject() {
        let objects_holder = CpdfIndirectObjectHolder::new();
        let array = A();
        ArrayMut(&array).append_new(CpdfReference::new(&objects_holder, 1234));
        assert_eq!(1usize, Array(&array).size());
        let obj = Array(&array).get_object_at(0);
        assert!(obj.is_some());
        assert_eq!(ObjectType::Reference, obj.unwrap().borrow().object_type());
        let cloned_array_object = clone_direct_object(&array);
        assert!(cloned_array_object.is_some());
        let cloned_array_object = cloned_array_object.unwrap();
        assert_eq!(
            ObjectType::Array,
            cloned_array_object.borrow().object_type()
        );
        let cloned_array = cloned_array_object;
        assert_eq!(0usize, Array(&cloned_array).size());
        let cloned_obj = Array(&cloned_array).get_object_at(0);
        assert!(cloned_obj.is_none());
    }
    #[test]
    fn PDFArrayTest_ConvertIndirect() {
        let objects_holder = CpdfIndirectObjectHolder::new();
        let mut array = CpdfArray::new();
        let pObj = array.append_new(CpdfScalarObject::number_from_signed(42));
        array.convert_to_indirect_object_at(0, &objects_holder);
        let pRef = array.get_object_at(0).unwrap();
        let pNum = array.get_direct_object_at(0).unwrap();
        assert_eq!(ObjectType::Reference, pRef.borrow().object_type());
        assert_eq!(ObjectType::Number, pNum.borrow().object_type());
        assert!(!Rc::ptr_eq(&pObj, &pRef));
        AssertPtr(Some(&pObj), Some(&pNum));
        assert_eq!(42, array.get_integer_at(0));
    }
}
mod PDFStreamTest {
    use super::*;
    #[test]
    fn SetData() {
        let data = vec![0u8; 100];
        // The native constructor copies this lvalue, retained for its size below.
        let stream = retain_object(CpdfStream::from_data_and_dict(data.clone(), D()));
        assert_eq!(
            data.len() as i32,
            Dict(&Stream(&stream).get_dict()).get_integer_for(b"Length", 0)
        );
        DictMut(&Stream(&stream).get_dict()).set_for(b"Filter", Some(U("SomeFilter")));
        DictMut(&Stream(&stream).get_dict()).set_for(b"DecodeParms", Some(U("SomeParams")));
        let new_data = vec![0u8; data.len() * 2];
        StreamMut(&stream).set_data(&new_data);
        assert_eq!(
            new_data.len() as i32,
            Dict(&Stream(&stream).get_dict()).get_integer_for(b"Length", 0)
        );
        crate::test_support::assert_wide_cstr_eq(
            &"SomeFilter".chars().map(u32::from).collect::<Vec<_>>(),
            &Dict(&Stream(&stream).get_dict()).get_unicode_text_for(b"Filter"),
        );
        crate::test_support::assert_wide_cstr_eq(
            &"SomeParams".chars().map(u32::from).collect::<Vec<_>>(),
            &Dict(&Stream(&stream).get_dict()).get_unicode_text_for(b"DecodeParms"),
        );
    }
    #[test]
    fn SetDataAndRemoveFilter() {
        let data = vec![0u8; 100];
        let stream = retain_object(CpdfStream::from_data_and_dict(data.clone(), D()));
        assert_eq!(
            data.len() as i32,
            Dict(&Stream(&stream).get_dict()).get_integer_for(b"Length", 0)
        );
        DictMut(&Stream(&stream).get_dict()).set_for(b"Filter", Some(U("SomeFilter")));
        DictMut(&Stream(&stream).get_dict()).set_for(b"DecodeParms", Some(U("SomeParams")));
        let new_data = vec![0u8; data.len() * 2];
        StreamMut(&stream).set_data_and_remove_filter(&new_data);
        assert_eq!(
            new_data.len() as i32,
            Dict(&Stream(&stream).get_dict()).get_integer_for(b"Length", 0)
        );
        assert!(!Dict(&Stream(&stream).get_dict()).key_exist(b"Filter"));
        assert!(!Dict(&Stream(&stream).get_dict()).key_exist(b"DecodeParms"));
    }
    #[test]
    fn LengthInDictionaryOnCreate() {
        const kBufSize: u32 = 100;
        {
            let stream = CpdfStream::from_data_and_dict(vec![0; kBufSize as usize], D());
            assert_eq!(
                kBufSize as i32,
                Dict(&stream.get_dict()).get_integer_for(b"Length", 0)
            );
        }
        {
            let dict = D();
            DictMut(&dict).set_for(b"Length", Some(N(30000)));
            let stream = CpdfStream::from_data_and_dict(vec![0; kBufSize as usize], dict);
            assert_eq!(
                kBufSize as i32,
                Dict(&stream.get_dict()).get_integer_for(b"Length", 0)
            );
        }
    }
}
mod PDFDictionaryTest {
    use super::*;
    #[test]
    fn PDFDictionaryTest_CloneDirectObject() {
        let objects_holder = CpdfIndirectObjectHolder::new();
        let dict = D();
        DictMut(&dict).set_new_for(b"foo", CpdfReference::new(&objects_holder, 1234));
        assert_eq!(1usize, Dict(&dict).size());
        let obj = Dict(&dict).get_object_for(b"foo");
        assert!(obj.is_some());
        assert_eq!(ObjectType::Reference, obj.unwrap().borrow().object_type());
        let cloned_dict_object = clone_direct_object(&dict);
        assert!(cloned_dict_object.is_some());
        let cloned_dict_object = cloned_dict_object.unwrap();
        assert_eq!(
            ObjectType::Dictionary,
            cloned_dict_object.borrow().object_type()
        );
        let cloned_dict = cloned_dict_object;
        assert_eq!(0usize, Dict(&cloned_dict).size());
        let cloned_obj = Dict(&cloned_dict).get_object_for(b"foo");
        assert!(cloned_obj.is_none());
    }
    #[test]
    fn PDFDictionaryTest_ConvertIndirect() {
        let objects_holder = CpdfIndirectObjectHolder::new();
        let mut dict = CpdfDictionary::new();
        let pObj = dict.set_new_for(b"clams", CpdfScalarObject::number_from_signed(42));
        dict.convert_to_indirect_object_for(b"clams", &objects_holder);
        let pRef = dict.get_object_for(b"clams").unwrap();
        let pNum = dict.get_direct_object_for(b"clams").unwrap();
        assert_eq!(ObjectType::Reference, pRef.borrow().object_type());
        assert_eq!(ObjectType::Number, pNum.borrow().object_type());
        assert!(!Rc::ptr_eq(&pObj, &pRef));
        AssertPtr(Some(&pObj), Some(&pNum));
        assert_eq!(42, dict.get_integer_for(b"clams", 0));
    }
    #[test]
    fn ExtractObjectOnRemove() {
        let mut dict = CpdfDictionary::new();
        let pObj = dict.set_new_for(b"child", CpdfScalarObject::number_from_signed(42));
        let mut extracted_object = dict.remove_for(b"child");
        AssertPtr(Some(&pObj), extracted_object.as_ref());
        extracted_object = dict.remove_for(b"non_exists_object");
        assert!(extracted_object.is_none());
    }
}
mod PDFObjectTest {
    use super::*;
    #[test]
    fn CloneCheckLoop() {
        {
            // Create a dictionary/array pair with a reference loop.
            let arr_obj = A();
            let dict_obj = ArrayMut(&arr_obj)
                .insert_new_at(0, CpdfDictionary::new())
                .unwrap();
            DictMut(&dict_obj).set_for(b"arr", Some(arr_obj.clone()));
            let cloned_array = AsType(&arr_obj.borrow().clone_object(), ObjectType::Array);
            assert!(cloned_array.is_some());
            let cloned_array = cloned_array.unwrap();
            assert_eq!(1usize, Array(&cloned_array).size());
            let cloned_dict = Array(&cloned_array).get_object_at(0);
            assert!(cloned_dict.is_some());
            let cloned_dict = cloned_dict.unwrap();
            assert_eq!(ObjectType::Dictionary, cloned_dict.borrow().object_type());
            assert!(Dict(&cloned_dict).get_object_for(b"arr").is_none());
            DictMut(&dict_obj).remove_for(b"arr"); // Break deliberate cycle for cleanup.
        }
        {
            let objects_holder = CpdfIndirectObjectHolder::new();
            // Create an object with a reference loop.
            let dict_obj = objects_holder.borrow_mut().new_indirect_dictionary();
            let mut arr_obj = CpdfArray::new();
            arr_obj.insert_new_at(
                0,
                CpdfReference::new(&objects_holder, dict_obj.borrow().obj_num()),
            );
            let elem0 = arr_obj.get_object_at(0);
            DictMut(&dict_obj).set_for(b"arr", Some(retain_object(arr_obj)));
            assert_eq!(1u32, dict_obj.borrow().obj_num());
            assert!(elem0.is_some());
            let elem0 = elem0.unwrap();
            assert_eq!(ObjectType::Reference, elem0.borrow().object_type());
            assert_eq!(
                1u32,
                elem0
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfReference>()
                    .unwrap()
                    .ref_obj_num()
            );
            AssertPtr(Some(&dict_obj), get_direct_object(&elem0).as_ref());
            let cloned_dict =
                clone_direct_object(&dict_obj).and_then(|o| AsType(&o, ObjectType::Dictionary));
            assert!(cloned_dict.is_some());
            let cloned_dict = cloned_dict.unwrap();
            let cloned_arr = Dict(&cloned_dict).get_object_for(b"arr");
            assert!(cloned_arr.is_some());
            let cloned_arr = cloned_arr.unwrap();
            assert_eq!(ObjectType::Array, cloned_arr.borrow().object_type());
            assert_eq!(0usize, Array(&cloned_arr).size());
            assert!(Array(&cloned_arr).get_object_at(0).is_none());
            DictMut(&dict_obj).remove_for(b"arr"); // Break deliberate cycle for cleanup.
        }
    }
}
mod PDFRefernceTest {
    use super::*;
    #[test]
    fn MakeReferenceToReference() {
        let obj_holder = CpdfIndirectObjectHolder::new();
        let original_ref = retain_object(CpdfReference::new(&obj_holder, 42));
        original_ref.borrow_mut().set_obj_num(1952);
        assert!(!original_ref.borrow().is_inline());
        let ref_obj = make_reference(&original_ref, &obj_holder);
        assert_eq!(ObjectType::Reference, ref_obj.borrow().object_type());
        assert_eq!(
            original_ref
                .borrow()
                .as_any()
                .downcast_ref::<CpdfReference>()
                .unwrap()
                .ref_obj_num(),
            ref_obj
                .borrow()
                .as_any()
                .downcast_ref::<CpdfReference>()
                .unwrap()
                .ref_obj_num()
        );
    }
}
