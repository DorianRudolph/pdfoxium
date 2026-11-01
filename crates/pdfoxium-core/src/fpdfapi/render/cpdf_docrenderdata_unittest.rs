// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/render/cpdf_docrenderdata_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]
use super::*;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef};
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

#[rustfmt::skip]
const kExpectedType0FunctionSamples: [u8; 256] = [
    25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25,
    25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25,
    25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25,
    25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25,
    25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25,
    25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25,
    25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25,
    25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25,
    25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 26,
    26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26,
    26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26,
    26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26,
    26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26,
    26, 26, 26, 26, 26, 26, 26, 26, 26];

#[rustfmt::skip]
const kExpectedType2FunctionSamples: [u8; 256] = [
    26, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25,
    25, 25, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24,
    24, 24, 24, 23, 23, 23, 23, 23, 23, 23, 23, 23, 23, 23, 23, 23, 23, 23, 23,
    23, 23, 23, 23, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22,
    22, 22, 22, 22, 22, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21,
    21, 21, 21, 21, 21, 21, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20,
    20, 20, 20, 20, 20, 20, 20, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19,
    19, 19, 19, 19, 19, 19, 19, 18, 18, 18, 18, 18, 18, 18, 18, 18, 18, 18, 18,
    18, 18, 18, 18, 18, 18, 18, 18, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
    17, 17, 17, 17, 17, 17, 17, 17, 17, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 14, 14, 14, 14, 14, 14, 14,
    14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 13, 13, 13, 13, 13, 13,
    13, 13, 13, 13, 13, 13, 13, 13, 13];

#[rustfmt::skip]
const kExpectedType4FunctionSamples: [u8; 256] = [
    0,   3,   6,   9,   13,  16,  19,  22,  25,  28,  31,  34,  37,  40,  43,
    46,  49,  52,  55,  58,  60,  63,  66,  68,  71,  74,  76,  79,  81,  84,
    86,  88,  90,  93,  95,  97,  99,  101, 103, 105, 106, 108, 110, 111, 113,
    114, 115, 117, 118, 119, 120, 121, 122, 123, 124, 125, 125, 126, 126, 127,
    127, 127, 127, 127, 127, 127, 127, 127, 127, 126, 126, 125, 125, 124, 123,
    123, 122, 121, 120, 119, 117, 116, 115, 113, 112, 110, 109, 107, 105, 104,
    102, 100, 98,  96,  94,  92,  89,  87,  85,  82,  80,  77,  75,  72,  70,
    67,  64,  62,  59,  56,  53,  50,  48,  45,  42,  39,  36,  33,  30,  27,
    23,  20,  17,  14,  11,  8,   5,   2,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0];

fn int_array(values: &[i32]) -> CpdfArray {
    let mut array = CpdfArray::new();
    for &value in values {
        array.append_new(CpdfScalarObject::number_from_signed(value));
    }
    array
}
fn float_array(values: &[f32]) -> CpdfArray {
    let mut array = CpdfArray::new();
    for &value in values {
        array.append_new(CpdfScalarObject::number_from_float(value));
    }
    array
}
fn CreateType0FunctionStreamReference(holder: &HolderRef) -> ObjectRef {
    let mut func_dict = CpdfDictionary::new();
    func_dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(0));
    func_dict.set_new_for(b"BitsPerSample", CpdfScalarObject::number_from_signed(8));
    func_dict.set_new_for(b"Domain", int_array(&[0, 1]));
    let mut range_array = CpdfArray::new();
    range_array.append_new(CpdfScalarObject::number_from_signed(0));
    range_array.append_new(CpdfScalarObject::number_from_float(0.5));
    func_dict.set_new_for(b"Range", range_array);
    func_dict.set_new_for(b"Size", int_array(&[4]));
    let contents = b"1234\0";
    let stream = retain_object(CpdfStream::from_data_and_dict(
        contents.to_vec(),
        retain_object(func_dict),
    ));
    holder.borrow_mut().add_indirect_object(stream.clone());
    let obj_num = stream.borrow().obj_num();
    retain_object(CpdfReference::new(holder, obj_num))
}
fn CreateType2FunctionDict() -> ObjectRef {
    let mut func_dict = CpdfDictionary::new();
    func_dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
    func_dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
    func_dict.set_new_for(b"Domain", int_array(&[0, 1]));
    func_dict.set_new_for(b"C0", float_array(&[0.1, 0.2, 0.8]));
    func_dict.set_new_for(b"C1", float_array(&[0.05, 0.01, 0.4]));
    retain_object(func_dict)
}
fn CreateType4FunctionStreamReference(holder: &HolderRef) -> ObjectRef {
    let mut func_dict = CpdfDictionary::new();
    func_dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(4));
    func_dict.set_new_for(b"Domain", int_array(&[0, 1]));
    func_dict.set_new_for(b"Range", int_array(&[-1, 1]));
    let contents = b"{ 360 mul sin 2 div }\0";
    let stream = retain_object(CpdfStream::from_data_and_dict(
        contents.to_vec(),
        retain_object(func_dict),
    ));
    holder.borrow_mut().add_indirect_object(stream.clone());
    let obj_num = stream.borrow().obj_num();
    retain_object(CpdfReference::new(holder, obj_num))
}
fn CreateBadType4FunctionStream() -> ObjectRef {
    let mut func_dict = CpdfDictionary::new();
    func_dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(4));
    func_dict.set_new_for(b"Domain", int_array(&[0, 1]));
    func_dict.set_new_for(b"Range", int_array(&[-1, 1]));
    let contents = b"garbage\0";
    retain_object(CpdfStream::from_data_and_dict(
        contents.to_vec(),
        retain_object(func_dict),
    ))
}

#[test]
fn TransferFunctionOne() {
    let func_dict = CreateType2FunctionDict();

    let render_data = CpdfDocRenderData::new();
    let func = render_data.create_transfer_func(&func_dict);
    assert!(func.is_some());
    let func = func.unwrap();
    assert!(!func.identity());
    assert_eq!(func.samples_r(), &kExpectedType2FunctionSamples);
    assert_eq!(func.samples_g(), &kExpectedType2FunctionSamples);
    assert_eq!(func.samples_b(), &kExpectedType2FunctionSamples);
    assert_eq!(0x000d0d0du32, func.translate_color(0x00ffffff));
    assert_eq!(0x000d1a1au32, func.translate_color(0x00ff0000));
    assert_eq!(0x001a0d1au32, func.translate_color(0x0000ff00));
    assert_eq!(0x001a1a0du32, func.translate_color(0x000000ff));
    assert_eq!(0x000f0f0fu32, func.translate_color(0x00cccccc));
    assert_eq!(0x00191715u32, func.translate_color(0x00123456));
    assert_eq!(0x000d0d0du32, func.translate_color(0xffffffff));
    assert_eq!(0x001a1a1au32, func.translate_color(0xff000000));
    assert_eq!(0x000d0d0du32, func.translate_color(0xccffffff));
    assert_eq!(0x001a1a1au32, func.translate_color(0x99000000));
}

#[test]
fn TransferFunctionArray() {
    let holder = CpdfIndirectObjectHolder::new();
    let mut func_array = CpdfArray::new();
    func_array.append(CreateType0FunctionStreamReference(&holder));
    func_array.append(CreateType2FunctionDict());
    func_array.append(CreateType4FunctionStreamReference(&holder));

    let render_data = CpdfDocRenderData::new();
    let func_array = retain_object(func_array);
    let func = render_data.create_transfer_func(&func_array);
    assert!(func.is_some());
    let func = func.unwrap();
    assert!(!func.identity());
    assert_eq!(func.samples_r(), &kExpectedType0FunctionSamples);
    assert_eq!(func.samples_g(), &kExpectedType2FunctionSamples);
    assert_eq!(func.samples_b(), &kExpectedType4FunctionSamples);
    assert_eq!(0x00000d1au32, func.translate_color(0x00ffffff));
    assert_eq!(0x00001a19u32, func.translate_color(0x00ff0000));
    assert_eq!(0x00000d19u32, func.translate_color(0x0000ff00));
    assert_eq!(0x00001a1au32, func.translate_color(0x000000ff));
    assert_eq!(0x00000f1au32, func.translate_color(0x00cccccc));
    assert_eq!(0x00371719u32, func.translate_color(0x00123456));
    assert_eq!(0x00000d1au32, func.translate_color(0xffffffff));
    assert_eq!(0x00001a19u32, func.translate_color(0xff000000));
    assert_eq!(0x00000d1au32, func.translate_color(0xccffffff));
    assert_eq!(0x00001a19u32, func.translate_color(0x99000000));
}

#[test]
fn TransferFunctionClamping() {
    // Type 2 exponential function: f(x) = C0 + x^N * (C1 - C0).
    // With C0 = -1.0, C1 = 2.0, N = 1:
    // f(0.0) = -1.0 (negative, underflow)
    // f(0.5) = 0.5
    // f(1.0) = 2.0 (greater than 1.0, overflow)
    let mut func_dict = CpdfDictionary::new();
    func_dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
    func_dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
    func_dict.set_new_for(b"Domain", int_array(&[0, 1]));
    func_dict.set_new_for(b"C0", float_array(&[-1.0]));
    func_dict.set_new_for(b"C1", float_array(&[2.0]));
    let func_dict = retain_object(func_dict);

    let render_data = CpdfDocRenderData::new();
    let func = render_data.create_transfer_func(&func_dict);
    assert!(func.is_some());
    let func = func.unwrap();
    assert!(!func.identity());

    // Input 0 gives -1.0f, clamped to 0.
    assert_eq!(0, func.samples_r()[0]);
    assert_eq!(0, func.samples_g()[0]);
    assert_eq!(0, func.samples_b()[0]);

    // Input 128 gives 0.505882f -> 129.
    assert_eq!(129, func.samples_r()[128]);
    assert_eq!(129, func.samples_g()[128]);
    assert_eq!(129, func.samples_b()[128]);

    // Input 255 gives 2.0f, clamped to 255.
    assert_eq!(255, func.samples_r()[255]);
    assert_eq!(255, func.samples_g()[255]);
    assert_eq!(255, func.samples_b()[255]);
}

#[test]
fn BadTransferFunctions() {
    {
        let func_stream = CreateBadType4FunctionStream();

        let render_data = CpdfDocRenderData::new();
        let func = render_data.create_transfer_func(&func_stream);
        assert!(func.is_none());
    }

    {
        let func_array = CpdfArray::new();

        let render_data = CpdfDocRenderData::new();
        let func_array = retain_object(func_array);
        let func = render_data.create_transfer_func(&func_array);
        assert!(func.is_none());
    }

    {
        let holder = CpdfIndirectObjectHolder::new();
        let mut func_array = CpdfArray::new();
        func_array.append(CreateType0FunctionStreamReference(&holder));
        func_array.append(CreateType2FunctionDict());
        let func_stream = CreateBadType4FunctionStream();
        let func_stream_object_number = holder.borrow_mut().add_indirect_object(func_stream);
        func_array.append(retain_object(CpdfReference::new(
            &holder,
            func_stream_object_number,
        )));

        let render_data = CpdfDocRenderData::new();
        let func_array = retain_object(func_array);
        let func = render_data.create_transfer_func(&func_array);
        assert!(func.is_none());
    }
}
