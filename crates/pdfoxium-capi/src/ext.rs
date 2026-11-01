// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Portable `fpdfsdk/fpdf_ext.cpp` / `public/fpdf_ext.h` and native unsupported
//! callback storage. The legacy C time_t/tm test hooks are excluded: production
//! JavaScript/PDF dates use Boa's existing clock and timezone services.
use super::*;
use pdfoxium_core::fpdfdoc::cpdf_metadata::UnsupportedFeature;
use pdfoxium_core::fpdfsdk::cpdfsdk_unsupported;
pub use pdfoxium_core::fpdfsdk::cpdfsdk_unsupported::{
    PAGEMODE_FULLSCREEN, PAGEMODE_UNKNOWN, PAGEMODE_USEATTACHMENTS, PAGEMODE_USENONE,
    PAGEMODE_USEOC, PAGEMODE_USEOUTLINES, PAGEMODE_USETHUMBS,
};
pub const FPDF_UNSP_DOC_XFAFORM: c_int = 1;
pub const FPDF_UNSP_DOC_PORTABLECOLLECTION: c_int = 2;
pub const FPDF_UNSP_DOC_ATTACHMENT: c_int = 3;
pub const FPDF_UNSP_DOC_SECURITY: c_int = 4;
pub const FPDF_UNSP_DOC_SHAREDREVIEW: c_int = 5;
pub const FPDF_UNSP_DOC_SHAREDFORM_ACROBAT: c_int = 6;
pub const FPDF_UNSP_DOC_SHAREDFORM_FILESYSTEM: c_int = 7;
pub const FPDF_UNSP_DOC_SHAREDFORM_EMAIL: c_int = 8;
pub const FPDF_UNSP_ANNOT_3DANNOT: c_int = 11;
pub const FPDF_UNSP_ANNOT_MOVIE: c_int = 12;
pub const FPDF_UNSP_ANNOT_SOUND: c_int = 13;
pub const FPDF_UNSP_ANNOT_SCREEN_MEDIA: c_int = 14;
pub const FPDF_UNSP_ANNOT_SCREEN_RICHMEDIA: c_int = 15;
pub const FPDF_UNSP_ANNOT_ATTACHMENT: c_int = 16;
pub const FPDF_UNSP_ANNOT_SIG: c_int = 17;
#[repr(C)]
#[allow(non_snake_case, non_camel_case_types)]
pub struct UNSUPPORT_INFO {
    pub version: c_int,
    pub FSDK_UnSupport_Handler: Option<unsafe extern "C" fn(*mut UNSUPPORT_INFO, c_int)>,
}
thread_local! {static UNSUPPORT_HANDLER:Cell<*mut UNSUPPORT_INFO>=const{Cell::new(std::ptr::null_mut())};}
pub(super) fn raise_unsupported_error(feature: UnsupportedFeature) -> bool {
    let descriptor = UNSUPPORT_HANDLER.with(Cell::get);
    if descriptor.is_null() {
        return false;
    }
    let callback = unsafe { (*descriptor).FSDK_UnSupport_Handler };
    if let Some(callback) = callback {
        unsafe { callback(descriptor, feature as c_int) }
    }
    true
}
pub(super) fn report_unsupported_features(document: &Rc<RefCell<CpdfDocument>>) {
    let root = document.borrow().root();
    cpdfsdk_unsupported::report_unsupported_features(root, &mut |feature| {
        raise_unsupported_error(feature);
    });
}
pub(super) fn report_unsupported_xfa(document: &Rc<RefCell<CpdfDocument>>, has_extension: bool) {
    let root = document.borrow().root();
    cpdfsdk_unsupported::report_unsupported_xfa(root, has_extension, &mut |feature| {
        raise_unsupported_error(feature);
    });
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FSDK_SetUnSpObjProcessHandler(info: *mut UNSUPPORT_INFO) -> c_int {
    if info.is_null() || unsafe { (*info).version } != 1 {
        return 0;
    }
    UNSUPPORT_HANDLER.with(|handler| handler.set(info));
    cpdfsdk_unsupported::set_unsupported_handler(Some(Rc::new(raise_unsupported_error)));
    1
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FPDFDoc_GetPageMode(document: *mut c_void) -> c_int {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return PAGEMODE_UNKNOWN;
    };
    let document = document.document.clone();
    let root = document.borrow().root();
    cpdfsdk_unsupported::get_page_mode(root)
}

#[cfg(test)]
#[path = "ext_tests.rs"]
mod tests;
