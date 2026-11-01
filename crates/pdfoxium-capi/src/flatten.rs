// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Annotation flattening from `fpdfsdk/fpdf_flatten.cpp`.
use super::*;
use pdfoxium_core::fpdfsdk::fpdf_flatten;

/// # Safety
/// Page is null or a live page handle available for exclusive mutation. Other
/// page/annotation handles follow PDFium's documented lifetime requirements.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_Flatten(page: *mut c_void, usage: c_int) -> c_int {
    if page.is_null() {
        return fpdf_flatten::FLATTEN_FAIL;
    }
    // Borrow the engine/document fields separately from the page alias cache.
    // Input callbacks may query public page handles while content is parsed.
    let page = page.cast::<PageHandle>();
    let document = Rc::clone(unsafe { &(*page)._document });
    let Some(engine_page) = unsafe { &*page }.pdf_page() else {
        return fpdf_flatten::FLATTEN_FAIL;
    };
    let result = fpdf_flatten::flatten(&document, engine_page, usage);
    unsafe { &(*page).link_list }
        .borrow_mut()
        .invalidate_page(engine_page);
    result
}
