// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_object_stream_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]
use super::*;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fxcrt::bytestring::c_str;

const kNormalStreamContent: &[u8] = b"10 0 11 14 12 21<</Name /Foo>>[1 2 3]4\0";
const kNormalStreamContentOffset: i32 = 16;
const _: () = {
    assert!(kNormalStreamContent[kNormalStreamContentOffset as usize] == b'<');
    assert!(kNormalStreamContent[kNormalStreamContentOffset as usize + 1] == b'<');
};

#[test]
fn StreamDictNormal() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(
        b"First",
        CpdfScalarObject::number_from_signed(kNormalStreamContentOffset),
    );

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    let obj_stream = CpdfObjectStream::create(&stream);
    assert!(obj_stream.is_some());
    let obj_stream = obj_stream.unwrap();

    assert_eq!(
        obj_stream.object_info(),
        &[
            ObjectInfo {
                obj_num: 10,
                obj_offset: 0
            },
            ObjectInfo {
                obj_num: 11,
                obj_offset: 14
            },
            ObjectInfo {
                obj_num: 12,
                obj_offset: 21
            }
        ]
    );

    // Check expected indices.
    let holder = CpdfIndirectObjectHolder::new();
    let obj10 = obj_stream.parse_object(Some(&holder), 10, 0);
    assert!(obj10.is_some());
    let obj10 = obj10.unwrap();
    assert_eq!(10u32, obj10.borrow().obj_num());
    assert_eq!(0u32, obj10.borrow().gen_num());
    assert!(obj10.borrow().object_type() == ObjectType::Dictionary);

    let obj11 = obj_stream.parse_object(Some(&holder), 11, 1);
    assert!(obj11.is_some());
    let obj11 = obj11.unwrap();
    assert_eq!(11u32, obj11.borrow().obj_num());
    assert_eq!(0u32, obj11.borrow().gen_num());
    assert!(obj11.borrow().object_type() == ObjectType::Array);

    let obj12 = obj_stream.parse_object(Some(&holder), 12, 2);
    assert!(obj12.is_some());
    let obj12 = obj12.unwrap();
    assert_eq!(12u32, obj12.borrow().obj_num());
    assert_eq!(0u32, obj12.borrow().gen_num());
    assert!(obj12.borrow().object_type() == ObjectType::Number);

    // Check bad indices.
    assert!((obj_stream.parse_object(Some(&holder), 10, 1)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 10, 2)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 10, 3)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 11, 0)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 11, 2)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 11, 3)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 12, 0)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 12, 1)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 12, 3)).is_none());
}

#[test]
fn StreamEmptyDict() {
    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(
        contents_view.to_vec(),
        retain_object(CpdfDictionary::new()),
    );
    assert!((CpdfObjectStream::create(&stream)).is_none());
}

#[test]
fn StreamDictNoType() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(5));

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    assert!((CpdfObjectStream::create(&stream)).is_none());
}

#[test]
fn StreamDictWrongType() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_for(b"Type", CpdfString::from_bytes(None, b"ObjStm"));
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(5));

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    assert!((CpdfObjectStream::create(&stream)).is_none());
}

#[test]
fn StreamDictWrongTypeValue() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStmmmm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(5));

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    assert!((CpdfObjectStream::create(&stream)).is_none());
}

#[test]
fn StreamDictNoCount() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(5));

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    assert!((CpdfObjectStream::create(&stream)).is_none());
}

#[test]
fn StreamDictFloatCount() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_float(2.2));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(5));

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    assert!((CpdfObjectStream::create(&stream)).is_none());
}

#[test]
fn StreamDictNegativeCount() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(-1));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(5));

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    assert!((CpdfObjectStream::create(&stream)).is_none());
}

#[test]
fn StreamDictCountTooBig() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(999999999));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(5));

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    assert!((CpdfObjectStream::create(&stream)).is_none());
}

#[test]
fn StreamDictNoOffset() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    assert!((CpdfObjectStream::create(&stream)).is_none());
}

#[test]
fn StreamDictFloatOffset() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_float(5.5));

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    assert!((CpdfObjectStream::create(&stream)).is_none());
}

#[test]
fn StreamDictNegativeOffset() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(-5));

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    assert!((CpdfObjectStream::create(&stream)).is_none());
}

#[test]
fn StreamDictOffsetTooBig() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    let kTooBigOffset = kNormalStreamContent.len() as i32;
    dict.set_new_for(
        b"First",
        CpdfScalarObject::number_from_signed(kTooBigOffset),
    );

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    let obj_stream = CpdfObjectStream::create(&stream);
    assert!(obj_stream.is_some());
    let obj_stream = obj_stream.unwrap();

    assert_eq!(
        obj_stream.object_info(),
        &[
            ObjectInfo {
                obj_num: 10,
                obj_offset: 0
            },
            ObjectInfo {
                obj_num: 11,
                obj_offset: 14
            },
            ObjectInfo {
                obj_num: 12,
                obj_offset: 21
            }
        ]
    );

    let holder = CpdfIndirectObjectHolder::new();
    assert!((obj_stream.parse_object(Some(&holder), 10, 0)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 11, 1)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 12, 2)).is_none());
}

#[test]
fn StreamDictTooFewCount() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(2));
    dict.set_new_for(
        b"First",
        CpdfScalarObject::number_from_signed(kNormalStreamContentOffset),
    );

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    let obj_stream = CpdfObjectStream::create(&stream);
    assert!(obj_stream.is_some());
    let obj_stream = obj_stream.unwrap();

    assert_eq!(
        obj_stream.object_info(),
        &[
            ObjectInfo {
                obj_num: 10,
                obj_offset: 0
            },
            ObjectInfo {
                obj_num: 11,
                obj_offset: 14
            }
        ]
    );

    let holder = CpdfIndirectObjectHolder::new();
    let obj10 = obj_stream.parse_object(Some(&holder), 10, 0);
    assert!(obj10.is_some());
    let obj10 = obj10.unwrap();
    assert_eq!(10u32, obj10.borrow().obj_num());
    assert_eq!(0u32, obj10.borrow().gen_num());
    assert!(obj10.borrow().object_type() == ObjectType::Dictionary);

    let obj11 = obj_stream.parse_object(Some(&holder), 11, 1);
    assert!(obj11.is_some());
    let obj11 = obj11.unwrap();
    assert_eq!(11u32, obj11.borrow().obj_num());
    assert_eq!(0u32, obj11.borrow().gen_num());
    assert!(obj11.borrow().object_type() == ObjectType::Array);

    assert!((obj_stream.parse_object(Some(&holder), 12, 2)).is_none());
}

#[test]
fn StreamDictTooManyObject() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(9));
    dict.set_new_for(
        b"First",
        CpdfScalarObject::number_from_signed(kNormalStreamContentOffset),
    );

    let contents_view = c_str(kNormalStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    let obj_stream = CpdfObjectStream::create(&stream);
    assert!(obj_stream.is_some());
    let obj_stream = obj_stream.unwrap();

    // TODO(thestig): Can this avoid finding object 2?
    assert_eq!(
        obj_stream.object_info(),
        &[
            ObjectInfo {
                obj_num: 10,
                obj_offset: 0
            },
            ObjectInfo {
                obj_num: 11,
                obj_offset: 14
            },
            ObjectInfo {
                obj_num: 12,
                obj_offset: 21
            },
            ObjectInfo {
                obj_num: 2,
                obj_offset: 3
            }
        ]
    );

    let holder = CpdfIndirectObjectHolder::new();
    assert!((obj_stream.parse_object(Some(&holder), 2, 0)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 2, 1)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 2, 2)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 2, 3)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 2, 4)).is_none());
}

#[test]
fn StreamDictGarbageObjNum() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(19));

    let kStreamContent = b"10 0 hi 14 12 21<</Name /Foo>>[1 2 3]4\0";
    let contents_view = c_str(kStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    let obj_stream = CpdfObjectStream::create(&stream);
    assert!(obj_stream.is_some());
    let obj_stream = obj_stream.unwrap();

    assert_eq!(
        obj_stream.object_info(),
        &[
            ObjectInfo {
                obj_num: 10,
                obj_offset: 0
            },
            ObjectInfo {
                obj_num: 12,
                obj_offset: 21
            }
        ]
    );
}

#[test]
fn StreamDictGarbageObjectOffset() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(16));

    let kStreamContent = b"10 0 11 hi 12 21<</Name /Foo>>[1 2 3]4\0";
    let contents_view = c_str(kStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    let obj_stream = CpdfObjectStream::create(&stream);
    assert!(obj_stream.is_some());
    let obj_stream = obj_stream.unwrap();

    // TODO(thestig): Should object 11 be rejected?
    assert_eq!(
        obj_stream.object_info(),
        &[
            ObjectInfo {
                obj_num: 10,
                obj_offset: 0
            },
            ObjectInfo {
                obj_num: 11,
                obj_offset: 0
            },
            ObjectInfo {
                obj_num: 12,
                obj_offset: 21
            }
        ]
    );

    let holder = CpdfIndirectObjectHolder::new();
    let obj10 = obj_stream.parse_object(Some(&holder), 10, 0);
    assert!(obj10.is_some());
    let obj10 = obj10.unwrap();
    assert_eq!(10u32, obj10.borrow().obj_num());
    assert_eq!(0u32, obj10.borrow().gen_num());
    assert!(obj10.borrow().object_type() == ObjectType::Dictionary);

    let obj11 = obj_stream.parse_object(Some(&holder), 11, 1);
    assert!(obj11.is_some());
    let obj11 = obj11.unwrap();
    assert_eq!(11u32, obj11.borrow().obj_num());
    assert_eq!(0u32, obj11.borrow().gen_num());
    assert!(obj11.borrow().object_type() == ObjectType::Dictionary);
}

#[test]
fn StreamDictNegativeObjectOffset() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(16));

    let kStreamContent = b"10 0 11 -1 12 21<</Name /Foo>>[1 2 3]4\0";
    let contents_view = c_str(kStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    let obj_stream = CpdfObjectStream::create(&stream);
    assert!(obj_stream.is_some());
    let obj_stream = obj_stream.unwrap();

    // TODO(thestig): Should object 11 be rejected?
    assert_eq!(
        obj_stream.object_info(),
        &[
            ObjectInfo {
                obj_num: 10,
                obj_offset: 0
            },
            ObjectInfo {
                obj_num: 11,
                obj_offset: 4294967295
            },
            ObjectInfo {
                obj_num: 12,
                obj_offset: 21
            }
        ]
    );

    let holder = CpdfIndirectObjectHolder::new();
    assert!((obj_stream.parse_object(Some(&holder), 11, 1)).is_none());
}

#[test]
fn StreamDictObjectOffsetTooBig() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(17));

    let kStreamContent = b"10 0 11 999 12 21<</Name /Foo>>[1 2 3]4\0";
    let contents_view = c_str(kStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    let obj_stream = CpdfObjectStream::create(&stream);
    assert!(obj_stream.is_some());
    let obj_stream = obj_stream.unwrap();

    // TODO(thestig): Should object 11 be rejected?
    assert_eq!(
        obj_stream.object_info(),
        &[
            ObjectInfo {
                obj_num: 10,
                obj_offset: 0
            },
            ObjectInfo {
                obj_num: 11,
                obj_offset: 999
            },
            ObjectInfo {
                obj_num: 12,
                obj_offset: 21
            }
        ]
    );

    let holder = CpdfIndirectObjectHolder::new();
    assert!((obj_stream.parse_object(Some(&holder), 11, 1)).is_none());
}

#[test]
fn StreamDictDuplicateObjNum() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(16));

    let kStreamContent = b"10 0 10 14 12 21<</Name /Foo>>[1 2 3]4\0";
    let contents_view = c_str(kStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    let obj_stream = CpdfObjectStream::create(&stream);
    assert!(obj_stream.is_some());
    let obj_stream = obj_stream.unwrap();

    assert_eq!(
        obj_stream.object_info(),
        &[
            ObjectInfo {
                obj_num: 10,
                obj_offset: 0
            },
            ObjectInfo {
                obj_num: 10,
                obj_offset: 14
            },
            ObjectInfo {
                obj_num: 12,
                obj_offset: 21
            }
        ]
    );

    let holder = CpdfIndirectObjectHolder::new();
    let obj10 = obj_stream.parse_object(Some(&holder), 10, 0);
    assert!(obj10.is_some());
    let obj10 = obj10.unwrap();
    assert_eq!(10u32, obj10.borrow().obj_num());
    assert_eq!(0u32, obj10.borrow().gen_num());
    assert!(obj10.borrow().object_type() == ObjectType::Dictionary);

    let new_obj10 = obj_stream.parse_object(Some(&holder), 10, 1);
    drop(obj10);
    let obj10 = new_obj10;
    assert!(obj10.is_some());
    let obj10 = obj10.unwrap();
    assert_eq!(10u32, obj10.borrow().obj_num());
    assert_eq!(0u32, obj10.borrow().gen_num());
    assert!(obj10.borrow().object_type() == ObjectType::Array);

    assert!((obj_stream.parse_object(Some(&holder), 10, 2)).is_none());
    assert!((obj_stream.parse_object(Some(&holder), 10, 3)).is_none());

    let obj12 = obj_stream.parse_object(Some(&holder), 12, 2);
    assert!(obj12.is_some());
    let obj12 = obj12.unwrap();
    assert_eq!(12u32, obj12.borrow().obj_num());
    assert_eq!(0u32, obj12.borrow().gen_num());
    assert!(obj12.borrow().object_type() == ObjectType::Number);
}

#[test]
fn StreamDictUnorderedObjectNumbers() {
    // ISO 32000-1:2008 spec. section 7.5.7, note 6 says there is no restriction
    // on object number ordering.
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(16));

    let kStreamContent = b"11 0 12 14 10 21<</Name /Foo>>[1 2 3]4\0";
    let contents_view = c_str(kStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    let obj_stream = CpdfObjectStream::create(&stream);
    assert!(obj_stream.is_some());
    let obj_stream = obj_stream.unwrap();

    assert_eq!(
        obj_stream.object_info(),
        &[
            ObjectInfo {
                obj_num: 11,
                obj_offset: 0
            },
            ObjectInfo {
                obj_num: 12,
                obj_offset: 14
            },
            ObjectInfo {
                obj_num: 10,
                obj_offset: 21
            }
        ]
    );

    let holder = CpdfIndirectObjectHolder::new();
    let obj10 = obj_stream.parse_object(Some(&holder), 10, 2);
    assert!(obj10.is_some());
    let obj10 = obj10.unwrap();
    assert_eq!(10u32, obj10.borrow().obj_num());
    assert_eq!(0u32, obj10.borrow().gen_num());
    assert!(obj10.borrow().object_type() == ObjectType::Number);

    let obj11 = obj_stream.parse_object(Some(&holder), 11, 0);
    assert!(obj11.is_some());
    let obj11 = obj11.unwrap();
    assert_eq!(11u32, obj11.borrow().obj_num());
    assert_eq!(0u32, obj11.borrow().gen_num());
    assert!(obj11.borrow().object_type() == ObjectType::Dictionary);

    let obj12 = obj_stream.parse_object(Some(&holder), 12, 1);
    assert!(obj12.is_some());
    let obj12 = obj12.unwrap();
    assert_eq!(12u32, obj12.borrow().obj_num());
    assert_eq!(0u32, obj12.borrow().gen_num());
    assert!(obj12.borrow().object_type() == ObjectType::Array);
}

#[test]
fn StreamDictUnorderedObjectOffsets() {
    // ISO 32000-1:2008 spec. section 7.5.7, says offsets shall be in increasing
    // order.
    // TODO(thestig): Should CPDF_ObjectStream check for this and reject this
    // object stream?
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"ObjStm");
    dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(16));

    let kStreamContent = b"10 21 11 0 12 14<</Name /Foo>>[1 2 3]4\0";
    let contents_view = c_str(kStreamContent);
    let stream = CpdfStream::from_data_and_dict(contents_view.to_vec(), retain_object(dict));
    let obj_stream = CpdfObjectStream::create(&stream);
    assert!(obj_stream.is_some());
    let obj_stream = obj_stream.unwrap();

    assert_eq!(
        obj_stream.object_info(),
        &[
            ObjectInfo {
                obj_num: 10,
                obj_offset: 21
            },
            ObjectInfo {
                obj_num: 11,
                obj_offset: 0
            },
            ObjectInfo {
                obj_num: 12,
                obj_offset: 14
            }
        ]
    );

    let holder = CpdfIndirectObjectHolder::new();
    let obj10 = obj_stream.parse_object(Some(&holder), 10, 0);
    assert!(obj10.is_some());
    let obj10 = obj10.unwrap();
    assert_eq!(10u32, obj10.borrow().obj_num());
    assert_eq!(0u32, obj10.borrow().gen_num());
    assert!(obj10.borrow().object_type() == ObjectType::Number);

    let obj11 = obj_stream.parse_object(Some(&holder), 11, 1);
    assert!(obj11.is_some());
    let obj11 = obj11.unwrap();
    assert_eq!(11u32, obj11.borrow().obj_num());
    assert_eq!(0u32, obj11.borrow().gen_num());
    assert!(obj11.borrow().object_type() == ObjectType::Dictionary);

    let obj12 = obj_stream.parse_object(Some(&holder), 12, 2);
    assert!(obj12.is_some());
    let obj12 = obj12.unwrap();
    assert_eq!(12u32, obj12.borrow().obj_num());
    assert_eq!(0u32, obj12.borrow().gen_num());
    assert!(obj12.borrow().object_type() == ObjectType::Array);
}
