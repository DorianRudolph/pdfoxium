// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/page/cpdf_page_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use crate::fpdfapi::page::cpdf_pagemodule::{destroy_page_module, initialize_page_module};
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

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

use super::CpdfPage;
use crate::fpdfapi::parser::cpdf_name::CpdfName;
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_string::CpdfString;

fn set_type(page_dict: &ObjectRef, value: ObjectRef) {
    page_dict
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(b"Type", Some(value));
}

#[test]
fn IsValidPageDict() {
    let _fixture = TestWithPageModule::set_up();
    // Null is represented by None; Rust references require a present object.
    let null: Option<ObjectRef> = None;
    assert!(!null.as_ref().is_some_and(CpdfPage::is_valid_page_dict));
    let page_dict = retain_object(CpdfDictionary::new());
    set_type(&page_dict, retain_object(CpdfName::new(None, b"Page")));
    assert!(CpdfPage::is_valid_page_dict(&page_dict));
}

#[test]
fn IsValidPageDictWrongTypes() {
    let _fixture = TestWithPageModule::set_up();
    let page_dict = retain_object(CpdfDictionary::new());
    assert!(!CpdfPage::is_valid_page_dict(&page_dict));
    set_type(&page_dict, retain_object(CpdfName::new(None, b"Font")));
    assert!(!CpdfPage::is_valid_page_dict(&page_dict));
    set_type(
        &page_dict,
        retain_object(CpdfString::from_bytes(None, b"Page")),
    );
    assert!(!CpdfPage::is_valid_page_dict(&page_dict));
    set_type(&page_dict, retain_object(CpdfScalarObject::null()));
    assert!(!CpdfPage::is_valid_page_dict(&page_dict));
}

#[test]
fn IsValidPageDictReference() {
    let _fixture = TestWithPageModule::set_up();
    let doc = CpdfDocument::new();
    let holder = doc.holder();
    let page_dict = holder.borrow_mut().new_indirect_dictionary();
    let num = holder
        .borrow_mut()
        .add_indirect_object(retain_object(CpdfName::new(None, b"Page")));
    set_type(&page_dict, retain_object(CpdfReference::new(&holder, num)));
    assert!(CpdfPage::is_valid_page_dict(&page_dict));
    let num = holder
        .borrow_mut()
        .add_indirect_object(retain_object(CpdfName::new(None, b"Pages")));
    set_type(&page_dict, retain_object(CpdfReference::new(&holder, num)));
    assert!(!CpdfPage::is_valid_page_dict(&page_dict));
    let num = holder
        .borrow_mut()
        .add_indirect_object(retain_object(CpdfScalarObject::null()));
    set_type(&page_dict, retain_object(CpdfReference::new(&holder, num)));
    assert!(!CpdfPage::is_valid_page_dict(&page_dict));
}

#[test]
fn IsValidPageDictLoose() {
    let _fixture = TestWithPageModule::set_up();
    let null: Option<ObjectRef> = None;
    assert!(!null
        .as_ref()
        .is_some_and(CpdfPage::is_valid_page_dict_loose));
    let page_dict = retain_object(CpdfDictionary::new());
    assert!(CpdfPage::is_valid_page_dict_loose(&page_dict));
    set_type(&page_dict, retain_object(CpdfName::new(None, b"Page")));
    assert!(CpdfPage::is_valid_page_dict_loose(&page_dict));
}

#[test]
fn IsValidPageDictLooseWrongTypes() {
    let _fixture = TestWithPageModule::set_up();
    let page_dict = retain_object(CpdfDictionary::new());
    set_type(&page_dict, retain_object(CpdfName::new(None, b"Font")));
    assert!(!CpdfPage::is_valid_page_dict_loose(&page_dict));
    set_type(
        &page_dict,
        retain_object(CpdfString::from_bytes(None, b"Page")),
    );
    assert!(!CpdfPage::is_valid_page_dict_loose(&page_dict));
    set_type(&page_dict, retain_object(CpdfScalarObject::null()));
    assert!(!CpdfPage::is_valid_page_dict_loose(&page_dict));
}

#[test]
fn IsValidPageDictLooseReference() {
    let _fixture = TestWithPageModule::set_up();
    let doc = CpdfDocument::new();
    let holder = doc.holder();
    let page_dict = holder.borrow_mut().new_indirect_dictionary();
    let num = holder
        .borrow_mut()
        .add_indirect_object(retain_object(CpdfName::new(None, b"Page")));
    set_type(&page_dict, retain_object(CpdfReference::new(&holder, num)));
    assert!(CpdfPage::is_valid_page_dict_loose(&page_dict));
    let num = holder
        .borrow_mut()
        .add_indirect_object(retain_object(CpdfName::new(None, b"Pages")));
    set_type(&page_dict, retain_object(CpdfReference::new(&holder, num)));
    assert!(!CpdfPage::is_valid_page_dict_loose(&page_dict));
    let num = holder
        .borrow_mut()
        .add_indirect_object(retain_object(CpdfScalarObject::null()));
    set_type(&page_dict, retain_object(CpdfReference::new(&holder, num)));
    assert!(!CpdfPage::is_valid_page_dict_loose(&page_dict));
}
