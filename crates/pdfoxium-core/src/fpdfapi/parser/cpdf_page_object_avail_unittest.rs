// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_page_object_avail_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]
use super::*;
use crate::fpdfapi::parser::availability_test_support::{
    ObjectState::{Available, Unavailable},
    TestHolder,
};
use crate::fpdfapi::parser::{
    cpdf_array::CpdfArray, cpdf_dictionary::CpdfDictionary, cpdf_object::retain_object,
    cpdf_string::CpdfString,
};

use crate::fpdfapi::parser::{cpdf_name::CpdfName, cpdf_reference::CpdfReference};
#[test]
fn ExcludePages() {
    let holder = TestHolder::new();
    holder.AddObject(1, retain_object(CpdfDictionary::new()), Available);
    holder.SetReference(1, b"Kids", 2);
    holder.AddObject(2, retain_object(CpdfArray::new()), Available);
    holder.WithArray(2, |array| {
        array.append_new(CpdfReference::new(&holder.holder, 3));
    });
    holder.AddObject(3, retain_object(CpdfDictionary::new()), Available);
    holder.WithDict(3, |dict| {
        dict.set_for(b"Type", Some(retain_object(CpdfName::new(None, b"Page"))));
    });
    holder.SetReference(3, b"OtherPageData", 4);
    // Add unavailable object related to other page.
    holder.AddObject(
        4,
        retain_object(CpdfString::from_bytes(None, b"Other page data")),
        Unavailable,
    );
    let mut avail = CpdfPageObjectAvail::from_obj_num(holder.GetValidator(), &holder.holder, 1);
    // Object 4 is unavailable but is in the skipped other page.
    assert_eq!(DocAvailStatus::DataAvailable, avail.check_avail());
}
