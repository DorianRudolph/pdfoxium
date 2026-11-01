// Copyright 2022 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_dictionary_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};

#[test]
fn Iterators() {
    let mut dict = CpdfDictionary::new();
    dict.set_new_dictionary_for(b"the-dictionary");
    dict.set_new_array_for(b"the-array");
    dict.set_new_for(b"the-number", CpdfScalarObject::number_from_signed(42));
    // The closure holds DictionaryLocker's lock until iteration completes.
    dict.with_locked_entries(|locked_dict| {
        let mut it = locked_dict.iter();
        let entry = it.next();
        assert!(entry.is_some());
        let (key, object) = entry.unwrap();
        assert_eq!(&**key, b"the-array");
        assert_eq!(object.borrow().object_type(), ObjectType::Array);

        let entry = it.next();
        assert!(entry.is_some());
        let (key, object) = entry.unwrap();
        assert_eq!(&**key, b"the-dictionary");
        assert_eq!(object.borrow().object_type(), ObjectType::Dictionary);

        let entry = it.next();
        assert!(entry.is_some());
        let (key, object) = entry.unwrap();
        assert_eq!(&**key, b"the-number");
        assert_eq!(object.borrow().object_type(), ObjectType::Number);

        assert!(it.next().is_none());
    });
}
