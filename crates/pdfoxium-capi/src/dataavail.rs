// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fpdfsdk/fpdf_dataavail.cpp` and `public/fpdf_dataavail.h`.
//! Raw callbacks stay here. The operation state is parked outside its RefCell
//! during callbacks; nested availability operations reject without a panic.
use super::*;
use pdfoxium_core::fpdfapi::parser::cpdf_data_avail::{CpdfDataAvail, DownloadHints, FileAvail};

pub const PDF_LINEARIZATION_UNKNOWN: c_int = -1;
pub const PDF_NOT_LINEARIZED: c_int = 0;
pub const PDF_LINEARIZED: c_int = 1;
pub const PDF_DATA_ERROR: c_int = -1;
pub const PDF_DATA_NOTAVAIL: c_int = 0;
pub const PDF_DATA_AVAIL: c_int = 1;
pub const PDF_FORM_ERROR: c_int = -1;
pub const PDF_FORM_NOTAVAIL: c_int = 0;
pub const PDF_FORM_AVAIL: c_int = 1;
pub const PDF_FORM_NOTEXIST: c_int = 2;
#[repr(C)]
#[allow(non_snake_case, non_camel_case_types)]
pub struct FX_FILEAVAIL {
    pub version: c_int,
    pub IsDataAvail: Option<unsafe extern "C" fn(*mut FX_FILEAVAIL, usize, usize) -> c_int>,
}
#[repr(C)]
#[allow(non_snake_case, non_camel_case_types)]
pub struct FX_DOWNLOADHINTS {
    pub version: c_int,
    pub AddSegment: Option<unsafe extern "C" fn(*mut FX_DOWNLOADHINTS, usize, usize)>,
}
struct FileAvailContext(*mut FX_FILEAVAIL);
impl FileAvail for FileAvailContext {
    fn is_data_avail(&self, offset: i64, size: usize) -> bool {
        let Ok(offset) = usize::try_from(offset) else {
            return false;
        };
        let Some(callback) = (unsafe { (*self.0).IsDataAvail }) else {
            return false;
        };
        unsafe { callback(self.0, offset, size) != 0 }
    }
}
// Retain the access descriptor itself: PDFium requires its lifetime, and reads
// observe callback changes without holding a Rust reference across the call.
struct FileAccessContext(*mut FPDF_FILEACCESS);
impl StreamWithSize for FileAccessContext {
    fn get_size(&self) -> i64 {
        i64::try_from(unsafe { (*self.0).m_FileLen }).unwrap_or(0)
    }
}
impl SeekableReadStream for FileAccessContext {
    fn read_block_at_offset(&self, buffer: &mut [u8], offset: i64) -> bool {
        if buffer.is_empty() || offset < 0 {
            return false;
        }
        let Some(end) = i64::try_from(buffer.len())
            .ok()
            .and_then(|n| offset.checked_add(n))
        else {
            return false;
        };
        if end > self.get_size() {
            return false;
        }
        let Ok(offset) = c_ulong::try_from(offset) else {
            return false;
        };
        let Ok(size) = c_ulong::try_from(buffer.len()) else {
            return false;
        };
        let (callback, param) = unsafe { ((*self.0).m_GetBlock, (*self.0).m_Param) };
        callback.is_some_and(|callback| callback(param, offset, buffer.as_mut_ptr(), size) != 0)
    }
}
struct DownloadHintsContext(*mut FX_DOWNLOADHINTS);
impl DownloadHints for DownloadHintsContext {
    fn add_segment(&mut self, offset: i64, size: usize) {
        let Ok(offset) = usize::try_from(offset) else {
            return;
        };
        if self.0.is_null() {
            return;
        }
        if let Some(callback) = unsafe { (*self.0).AddSegment } {
            unsafe { callback(self.0, offset, size) }
        }
    }
}
struct AvailContext {
    _file_avail: Rc<dyn FileAvail>,
    state: RefCell<Option<CpdfDataAvail>>,
}
struct AvailOperation {
    owner: Rc<AvailContext>,
    state: Option<CpdfDataAvail>,
}
impl Drop for AvailOperation {
    fn drop(&mut self) {
        *self.owner.state.borrow_mut() = self.state.take();
    }
}
unsafe fn operation(avail: *mut c_void) -> Option<AvailOperation> {
    let owner = unsafe { avail.cast::<Rc<AvailContext>>().as_ref()? }.clone();
    let state = owner.state.try_borrow_mut().ok()?.take()?;
    Some(AvailOperation {
        owner,
        state: Some(state),
    })
}
fn hints(pointer: *mut FX_DOWNLOADHINTS) -> Rc<RefCell<dyn DownloadHints>> {
    Rc::new(RefCell::new(DownloadHintsContext(pointer)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn FPDFAvail_Create(
    file_avail: *mut FX_FILEAVAIL,
    file: *mut FPDF_FILEACCESS,
) -> *mut c_void {
    if file_avail.is_null()
        || file.is_null()
        || unsafe { (*file_avail).IsDataAvail.is_none() || (*file).m_GetBlock.is_none() }
    {
        return std::ptr::null_mut();
    }
    let available: Rc<dyn FileAvail> = Rc::new(FileAvailContext(file_avail));
    let stream: Rc<dyn SeekableReadStream> = Rc::new(FileAccessContext(file));
    let state = CpdfDataAvail::new(Some(&available), stream);
    Box::into_raw(Box::new(Rc::new(AvailContext {
        _file_avail: available,
        state: RefCell::new(Some(state)),
    })))
    .cast()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FPDFAvail_Destroy(avail: *mut c_void) {
    if !avail.is_null() {
        drop(unsafe { Box::from_raw(avail.cast::<Rc<AvailContext>>()) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FPDFAvail_IsDocAvail(
    avail: *mut c_void,
    download_hints: *mut FX_DOWNLOADHINTS,
) -> c_int {
    let Some(mut operation) = (unsafe { operation(avail) }) else {
        return PDF_DATA_ERROR;
    };
    let hints = hints(download_hints);
    operation.state.as_mut().unwrap().is_doc_avail(Some(&hints)) as c_int
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FPDFAvail_GetDocument(
    avail: *mut c_void,
    password: *const c_char,
) -> *mut c_void {
    let Some(mut operation) = (unsafe { operation(avail) }) else {
        return std::ptr::null_mut();
    };
    let password = if password.is_null() {
        Vec::new()
    } else {
        unsafe { CStr::from_ptr(password) }.to_bytes().to_vec()
    };
    match operation.state.as_mut().unwrap().parse_document(&password) {
        Ok(document) => {
            super::ext::report_unsupported_features(&document);
            Box::into_raw(Box::new(DocumentHandle::new(document))).cast()
        }
        Err(error) => {
            set_parse_error(error);
            std::ptr::null_mut()
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FPDFAvail_GetFirstPageNum(document: *mut c_void) -> c_int {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    document
        .document
        .try_borrow()
        .map_or(0, |d| d.parser().first_page_no() as c_int)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FPDFAvail_IsPageAvail(
    avail: *mut c_void,
    page_index: c_int,
    download_hints: *mut FX_DOWNLOADHINTS,
) -> c_int {
    let Some(mut operation) = (unsafe { operation(avail) }) else {
        return PDF_DATA_ERROR;
    };
    if page_index < 0 {
        return PDF_DATA_NOTAVAIL;
    }
    let hints = hints(download_hints);
    operation
        .state
        .as_mut()
        .unwrap()
        .is_page_avail(page_index as u32, Some(&hints)) as c_int
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FPDFAvail_IsFormAvail(
    avail: *mut c_void,
    download_hints: *mut FX_DOWNLOADHINTS,
) -> c_int {
    let Some(mut operation) = (unsafe { operation(avail) }) else {
        return PDF_FORM_ERROR;
    };
    let hints = hints(download_hints);
    operation
        .state
        .as_mut()
        .unwrap()
        .is_form_avail(Some(&hints)) as c_int
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FPDFAvail_IsLinearized(avail: *mut c_void) -> c_int {
    let Some(mut operation) = (unsafe { operation(avail) }) else {
        return PDF_LINEARIZATION_UNKNOWN;
    };
    operation.state.as_mut().unwrap().is_linearized_pdf() as c_int
}

#[cfg(test)]
#[path = "dataavail_tests.rs"]
mod tests;
