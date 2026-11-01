// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Document catalog operations from `fpdfsdk/fpdf_catalog.cpp`.
use crate::fpdfapi::parser::{
    cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument, cpdf_string::CpdfString,
};
pub fn is_tagged(document: &CpdfDocument) -> bool {
    let Some(root) = document.root() else {
        return false;
    };
    let mark = root
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .and_then(|dict| {
            dict.get_typed_direct_for(
                b"MarkInfo",
                crate::fpdfapi::parser::cpdf_scalar_object::ObjectType::Dictionary,
            )
        });
    mark.is_some_and(|mark| {
        mark.borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .is_some_and(|dict| dict.get_integer_for(b"Marked", 0) != 0)
    })
}
pub fn language(document: &CpdfDocument) -> Option<Vec<u32>> {
    let root = document.root()?;
    let borrowed = root.borrow();
    Some(
        borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_unicode_text_for(b"Lang"),
    )
}
pub fn set_language(document: &CpdfDocument, language: &[u32]) -> bool {
    let Some(root) = document.root() else {
        return false;
    };
    let mut borrowed = root.borrow_mut();
    let Some(dict) = borrowed.as_any_mut().downcast_mut::<CpdfDictionary>() else {
        return false;
    };
    dict.set_new_for(b"Lang", CpdfString::from_unicode(None, language));
    true
}
