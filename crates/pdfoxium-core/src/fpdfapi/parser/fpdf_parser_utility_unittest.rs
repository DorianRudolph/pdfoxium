// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/fpdf_parser_utility_unittest.cpp`.

#![allow(non_snake_case)]

use super::*;
use crate::fpdfapi::page::cpdf_pagemodule::{destroy_page_module, initialize_page_module};
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_string::CpdfString;

// TestWithPageModule's SetUp/TearDown, including cleanup on assertion failure.
struct TestWithPageModule;
impl TestWithPageModule {
    fn set_up() -> Self {
        initialize_page_module();
        Self
    }
}
impl Drop for TestWithPageModule {
    fn drop(&mut self) {
        destroy_page_module();
    }
}

fn set_type(dict: &ObjectRef, name: &[u8]) {
    dict.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_name_for(b"Type", name);
}

#[test]
fn NameDecode() {
    assert_eq!(b"", pdf_name_decode(b"").as_slice());
    assert_eq!(b"A", pdf_name_decode(b"A").as_slice());
    assert_eq!(b"#", pdf_name_decode(b"#").as_slice());
    assert_eq!(b"#4", pdf_name_decode(b"#4").as_slice());
    assert_eq!(b"A", pdf_name_decode(b"#41").as_slice());
    assert_eq!(b"A1", pdf_name_decode(b"#411").as_slice());
}

#[test]
fn NameEncode() {
    assert_eq!(b"", pdf_name_encode(b"").as_slice());
    assert_eq!(b"A", pdf_name_encode(b"A").as_slice());
    assert_eq!(b"#23", pdf_name_encode(b"#").as_slice());
    assert_eq!(b"#20", pdf_name_encode(b" ").as_slice());
    assert_eq!(
        b"!@#23$#25^&*#28#29#3C#3E#5B#5D",
        pdf_name_encode(b"!@#$%^&*()<>[]").as_slice()
    );
    assert_eq!(b"#C2", pdf_name_encode(b"\xc2").as_slice());
    assert_eq!(b"f#C2#A5", pdf_name_encode(b"f\xc2\xa5").as_slice());
}

#[test]
fn ValidateDictType() {
    let mut dict = CpdfDictionary::new();
    // No type.
    assert!(!validate_dict_type(Some(&dict), b"foo"));
    assert!(!validate_dict_type(Some(&dict), b"bar"));
    // Add the wrong object type.
    dict.set_new_for(
        b"Type",
        CpdfString::from_unicode(None, &[b'f' as u32, b'o' as u32, b'o' as u32]),
    );
    assert!(!validate_dict_type(Some(&dict), b"foo"));
    assert!(!validate_dict_type(Some(&dict), b"bar"));
    // Add the correct object type.
    dict.set_new_name_for(b"Type", b"foo");
    assert!(validate_dict_type(Some(&dict), b"foo"));
    assert!(!validate_dict_type(Some(&dict), b"bar"));
}

#[test]
fn ValidateDictAllResourcesOfType() {
    let _fixture = TestWithPageModule::set_up();
    {
        // Direct dictionary.
        let mut dict = CpdfDictionary::new();
        // Empty dict is ok.
        assert!(validate_dict_all_resources_of_type(Some(&dict), b"foo"));
        assert!(validate_dict_all_resources_of_type(Some(&dict), b"bar"));
        // nullptr is not.
        assert!(!validate_dict_all_resources_of_type(None, b"foo"));
        assert!(!validate_dict_all_resources_of_type(None, b"bar"));
        // Add two correct dictionary entries and one string entry.
        let new_dict = dict.set_new_dictionary_for(b"f1");
        set_type(&new_dict, b"foo");
        let new_dict = dict.set_new_dictionary_for(b"f2");
        set_type(&new_dict, b"foo");
        dict.set_new_for(b"f3", CpdfString::from_unicode(None, &[102, 111, 111]));
        assert!(!validate_dict_all_resources_of_type(Some(&dict), b"foo"));
        assert!(!validate_dict_all_resources_of_type(Some(&dict), b"bar"));
        // Replace the string with a dictionary with the wrong /Type.
        let new_dict = dict.set_new_dictionary_for(b"f3");
        set_type(&new_dict, b"bar");
        assert!(!validate_dict_all_resources_of_type(Some(&dict), b"foo"));
        assert!(!validate_dict_all_resources_of_type(Some(&dict), b"bar"));
        // Change the /Type to match.
        set_type(&new_dict, b"foo");
        assert!(validate_dict_all_resources_of_type(Some(&dict), b"foo"));
        assert!(!validate_dict_all_resources_of_type(Some(&dict), b"bar"));
    }
    {
        // CPDF_TestDocument supplies a normal document's object table/caches.
        let doc = CpdfDocument::new();
        let objects = doc.holder();
        let mut dict = CpdfDictionary::new();
        let new_dict = objects.borrow_mut().new_indirect_dictionary();
        set_type(&new_dict, b"foo");
        dict.set_new_for(
            b"f1",
            CpdfReference::new(&objects, new_dict.borrow().obj_num()),
        );
        assert!(validate_dict_all_resources_of_type(Some(&dict), b"foo"));
        assert!(!validate_dict_all_resources_of_type(Some(&dict), b"bar"));
    }
}

#[test]
fn ValidateDictOptionalType() {
    let mut dict = CpdfDictionary::new();
    // No type is ok.
    assert!(validate_dict_optional_type(Some(&dict), b"foo"));
    assert!(validate_dict_optional_type(Some(&dict), b"bar"));
    // Add the wrong object type.
    dict.set_new_for(b"Type", CpdfString::from_unicode(None, &[102, 111, 111]));
    assert!(!validate_dict_optional_type(Some(&dict), b"foo"));
    assert!(!validate_dict_optional_type(Some(&dict), b"bar"));
    // Add the correct object type.
    dict.set_new_name_for(b"Type", b"foo");
    assert!(validate_dict_optional_type(Some(&dict), b"foo"));
    assert!(!validate_dict_optional_type(Some(&dict), b"bar"));
}
