// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Rendered pattern APIs from fpdfsdk/fpdf_editpage.cpp.
use super::*;
unsafe fn rendered_pattern(
    document: *mut c_void,
    object: *mut c_void,
    stroke: bool,
) -> *mut c_void {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let data = document.document.borrow().page_data_ref();
    let bitmap = unsafe {
        with_page_object_ref(object, |object| {
            pdfoxium_core::fpdfsdk::fpdf_editpattern::rendered_pattern(data, object, stroke)
        })
    }
    .flatten();
    bitmap.map_or(std::ptr::null_mut(), |bitmap| {
        Box::into_raw(Box::new(BitmapHandle {
            storage: Rc::new(RefCell::new(BitmapStorage::Owned(bitmap))),
        }))
        .cast()
    })
}
/// # Safety
/// Document and object are null or live and available for reading. Caller owns
/// a returned bitmap and must destroy it with FPDFBitmap_Destroy.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPageObj_GetRenderedStrokePattern(
    document: *mut c_void,
    object: *mut c_void,
) -> *mut c_void {
    unsafe { rendered_pattern(document, object, true) }
}
/// # Safety
/// Document and object are null or live and available for reading. Caller owns
/// a returned bitmap and must destroy it with FPDFBitmap_Destroy.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPageObj_GetRenderedFillPattern(
    document: *mut c_void,
    object: *mut c_void,
) -> *mut c_void {
    unsafe { rendered_pattern(document, object, false) }
}
