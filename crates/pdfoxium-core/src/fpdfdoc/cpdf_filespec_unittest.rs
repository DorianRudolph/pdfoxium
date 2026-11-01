// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Full cases from core/fpdfdoc/cpdf_filespec_unittest.cpp.
#![allow(non_snake_case)]
use super::*;
use crate::fpdfapi::parser::{
    cpdf_indirect_object_holder::CpdfIndirectObjectHolder,
    cpdf_name::CpdfName,
    cpdf_object::{make_reference, retain_object},
    cpdf_scalar_object::CpdfScalarObject,
    cpdf_string::CpdfString,
};
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn set(object: &ObjectRef, key: &[u8], value: ObjectRef) {
    object
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(key, Some(value));
}
fn string(text: &str) -> ObjectRef {
    retain_object(CpdfString::from_unicode(None, &wide(text)))
}
const KEYS: [&[u8]; 5] = [b"Unix", b"Mac", b"DOS", b"F", b"UF"];
const STREAMS: [&[u8]; 5] = [b"test1", b"test2", b"test3", b"test4", b"test5"];
// The source's only static_assert checks the embedded-stream fixture lengths.
const _: () = assert!(KEYS.len() == STREAMS.len());
#[test]
fn EncodeDecodeFileName() {
    let mut cases = vec![("", ""), ("test.pdf", "test.pdf")];
    #[cfg(windows)]
    cases.extend([
        (r"r:\pdfdocs\spec.pdf", "/r/pdfdocs/spec.pdf"),
        (r"My Document\test.pdf", "My Document/test.pdf"),
        (r"\pdfdocs\spec.pdf", "//pdfdocs/spec.pdf"),
        (r"\\pdfdocs\spec.pdf", "/pdfdocs/spec.pdf"),
    ]);
    #[cfg(target_vendor = "apple")]
    cases.extend([
        ("Mac HD:PDFDocs:spec.pdf", "/Mac HD/PDFDocs/spec.pdf"),
        ("PDFDocs:spec.pdf", "PDFDocs/spec.pdf"),
    ]);
    #[cfg(not(any(windows, target_vendor = "apple")))]
    cases.extend([
        ("./docs/test.pdf", "./docs/test.pdf"),
        ("../test_docs/test.pdf", "../test_docs/test.pdf"),
        ("/usr/local/home/test.pdf", "/usr/local/home/test.pdf"),
    ]);
    for (input, expected) in cases {
        assert_eq!(CpdfFileSpec::encode_file_name(&wide(input)), wide(expected));
        assert_eq!(CpdfFileSpec::decode_file_name(&wide(expected)), wide(input));
    }
}
#[test]
fn GetFileName() {
    #[cfg(windows)]
    let string_case = ("/C/docs/test.pdf", r"C:\docs\test.pdf");
    #[cfg(target_vendor = "apple")]
    let string_case = ("/Mac HD/docs/test.pdf", "Mac HD:docs:test.pdf");
    #[cfg(not(any(windows, target_vendor = "apple")))]
    let string_case = ("/docs/test.pdf", "/docs/test.pdf");
    assert_eq!(
        CpdfFileSpec::new(string(string_case.0)).file_name(),
        wide(string_case.1)
    );
    #[cfg(windows)]
    let cases = [
        ("/C/docs/test.pdf", r"C:\docs\test.pdf"),
        ("/D/docs/test.pdf", r"D:\docs\test.pdf"),
        ("/E/docs/test.pdf", r"E:\docs\test.pdf"),
        ("/F/docs/test.pdf", r"F:\docs\test.pdf"),
        ("/G/docs/test.pdf", r"G:\docs\test.pdf"),
    ];
    #[cfg(target_vendor = "apple")]
    let cases = [
        ("/Mac HD/docs1/test.pdf", "Mac HD:docs1:test.pdf"),
        ("/Mac HD/docs2/test.pdf", "Mac HD:docs2:test.pdf"),
        ("/Mac HD/docs3/test.pdf", "Mac HD:docs3:test.pdf"),
        ("/Mac HD/docs4/test.pdf", "Mac HD:docs4:test.pdf"),
        ("/Mac HD/docs5/test.pdf", "Mac HD:docs5:test.pdf"),
    ];
    #[cfg(not(any(windows, target_vendor = "apple")))]
    let cases = [
        ("/docs/a/test.pdf", "/docs/a/test.pdf"),
        ("/docs/b/test.pdf", "/docs/b/test.pdf"),
        ("/docs/c/test.pdf", "/docs/c/test.pdf"),
        ("/docs/d/test.pdf", "/docs/d/test.pdf"),
        ("/docs/e/test.pdf", "/docs/e/test.pdf"),
    ];
    let dictionary = retain_object(CpdfDictionary::new());
    let spec = CpdfFileSpec::new(dictionary.clone());
    assert!(spec.file_name().is_empty());
    for (key, (input, expected)) in KEYS.into_iter().zip(cases) {
        set(&dictionary, key, string(input));
        assert_eq!(spec.file_name(), wide(expected));
    }
    set(&dictionary, b"FS", string("URL"));
    assert_eq!(spec.file_name(), wide(cases[4].0));
    let invalid = retain_object(CpdfName::new(None, b"test.pdf"));
    assert!(CpdfFileSpec::new(invalid).file_name().is_empty());
    let dictionary = retain_object(CpdfDictionary::new());
    let spec = CpdfFileSpec::new(dictionary.clone());
    for key in KEYS {
        set(
            &dictionary,
            key,
            retain_object(CpdfName::new(None, b"http://evil.org")),
        );
        assert!(spec.file_name().is_empty());
    }
    set(&dictionary, b"FS", string("URL"));
    assert!(spec.file_name().is_empty());
}
#[test]
fn GetFileStream() {
    assert!(
        CpdfFileSpec::new(retain_object(CpdfName::new(None, b"test.pdf")))
            .file_stream()
            .is_none()
    );
    let dictionary = retain_object(CpdfDictionary::new());
    let spec = CpdfFileSpec::new(dictionary.clone());
    assert!(spec.file_stream().is_none());
    let embedded = retain_object(CpdfDictionary::new());
    set(&dictionary, b"EF", embedded.clone());
    assert!(spec.file_stream().is_none());
    let holder = CpdfIndirectObjectHolder::new();
    for (i, (key, bytes)) in KEYS.into_iter().zip(STREAMS).enumerate() {
        set(&dictionary, key, string("test.pdf"));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            bytes.to_vec(),
            retain_object(CpdfDictionary::new()),
        ));
        holder.borrow_mut().add_indirect_object(stream.clone());
        assert!(stream.borrow().obj_num() > 0);
        set(&embedded, key, make_reference(&stream, &holder));
        let actual = spec.file_stream().expect("embedded stream");
        assert_eq!(
            actual.borrow().get_unicode_text(),
            bytes.iter().copied().map(u32::from).collect::<Vec<_>>()
        );
        if i == 2 {
            set(&dictionary, b"FS", string("URL"));
            assert!(spec.file_stream().is_none());
        }
    }
}
#[test]
fn GetParamsDict() {
    assert!(
        CpdfFileSpec::new(retain_object(CpdfName::new(None, b"test.pdf")))
            .params_dict()
            .is_none()
    );
    let holder = CpdfIndirectObjectHolder::new();
    let dictionary = retain_object(CpdfDictionary::new());
    let embedded = retain_object(CpdfDictionary::new());
    set(&dictionary, b"EF", embedded.clone());
    set(&dictionary, b"UF", string("test.pdf"));
    let spec = CpdfFileSpec::new(dictionary);
    assert!(spec.params_dict().is_none());
    // Native as_byte_span(kHello) includes the terminating zero.
    let stream_dict = retain_object(CpdfDictionary::new());
    let stream = retain_object(CpdfStream::from_data_and_dict(
        b"hello\0".to_vec(),
        stream_dict.clone(),
    ));
    holder.borrow_mut().add_indirect_object(stream.clone());
    assert!(stream.borrow().obj_num() > 0);
    set(&embedded, b"UF", make_reference(&stream, &holder));
    let params = retain_object(CpdfDictionary::new());
    set(&stream_dict, b"Params", params.clone());
    assert!(spec.params_dict().is_some());
    set(
        &params,
        b"Size",
        retain_object(CpdfScalarObject::number_from_signed(6)),
    );
    let actual = spec.params_dict().unwrap();
    assert_eq!(
        actual
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_integer_for(b"Size", 0),
        6
    );
}
