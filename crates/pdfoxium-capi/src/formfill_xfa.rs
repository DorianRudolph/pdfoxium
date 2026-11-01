// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! XFA callbacks and public load/BSTR portions of fpdf_view/fpdf_formfill.
//! Only this ABI adapter handles raw callback descriptors and UTF-16 buffers.
use super::super::unimplemented_abi::{FPDF_BSTR, FPDF_FILEHANDLER};
use super::*;
use pdfoxium_core::{
    fpdfsdk::fpdfxfa::{cpdfxfa_context::XfaLoadError, cpdfxfa_docenvironment::XfaSdkHost},
    fxcrt::{fx_stream::SeekableWriteStream, fx_string::utf16_decode, fx_types::FxFileSize},
    xfa::fxfa::cxfa_ffdoc::PageViewEvent,
};
#[no_mangle]
pub unsafe extern "C" fn FPDF_LoadXFA(document: *mut c_void) -> c_int {
    let context = {
        let Some(doc) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
            return 0;
        };
        doc.xfa_context.clone()
    };
    let Some(context) = context else {
        return 0;
    };
    match context.load_xfa_doc() {
        Ok(()) => 1,
        Err(error) => {
            set_last_error(
                if matches!(error, XfaLoadError::Theme | XfaLoadError::Layout(_)) {
                    8
                } else {
                    7
                },
            );
            0
        }
    }
}
/// # Safety
/// `bstr` is writable and fresh (or has been cleared through this API).
#[no_mangle]
pub unsafe extern "C" fn FPDF_BStr_Init(bstr: *mut FPDF_BSTR) -> c_int {
    let Some(bstr) = (unsafe { bstr.as_mut() }) else {
        return -1;
    };
    bstr.str_ = std::ptr::null_mut();
    bstr.len = 0;
    0
}
/// # Safety
/// BSTR was initialized/set using this ABI. Input supplies `length` bytes or,
/// for -1, a NUL-terminated byte string. BSTR storage belongs to these APIs.
#[no_mangle]
pub unsafe extern "C" fn FPDF_BStr_Set(
    bstr: *mut FPDF_BSTR,
    value: *const c_char,
    length: c_int,
) -> c_int {
    if bstr.is_null() || value.is_null() || length < -1 {
        return -1;
    }
    let bytes = if length == -1 {
        unsafe { CStr::from_ptr(value) }.to_bytes()
    } else {
        unsafe { std::slice::from_raw_parts(value.cast::<u8>(), length as usize) }
    };
    let Ok(length) = c_int::try_from(bytes.len()) else {
        return -1;
    };
    if bytes.is_empty() {
        return unsafe { FPDF_BStr_Clear(bstr) };
    }
    // Copy first: the source is permitted to alias the old BSTR buffer.
    let mut owned = Vec::with_capacity(bytes.len() + 1);
    owned.extend_from_slice(bytes);
    owned.push(0);
    unsafe { FPDF_BStr_Clear(bstr) };
    let data = Box::into_raw(owned.into_boxed_slice()).cast::<u8>();
    unsafe {
        (*bstr).str_ = data.cast();
        (*bstr).len = length;
    }
    0
}
/// # Safety
/// BSTR storage was allocated using FPDF_BStr_Set and is not independently freed.
#[no_mangle]
pub unsafe extern "C" fn FPDF_BStr_Clear(bstr: *mut FPDF_BSTR) -> c_int {
    let Some(bstr) = (unsafe { bstr.as_mut() }) else {
        return -1;
    };
    if !bstr.str_.is_null() {
        let Ok(length) = usize::try_from(bstr.len) else {
            return -1;
        };
        let data = std::ptr::slice_from_raw_parts_mut(bstr.str_.cast::<u8>(), length + 1);
        bstr.str_ = std::ptr::null_mut();
        bstr.len = 0;
        drop(unsafe { Box::from_raw(data) });
    } else {
        bstr.len = 0;
    }
    0
}
struct FileHandlerStream {
    handler: FPDF_FILEHANDLER,
    position: Cell<FxFileSize>,
    _host: Rc<FfiHost>,
}
impl FileHandlerStream {
    unsafe fn new(handler: *mut FPDF_FILEHANDLER, host: Rc<FfiHost>) -> Option<Self> {
        Some(Self {
            handler: unsafe { *handler.as_ref()? },
            position: Cell::new(0),
            _host: host,
        })
    }
}
impl Drop for FileHandlerStream {
    fn drop(&mut self) {
        if let Some(release) = self.handler.Release {
            unsafe { release(self.handler.clientData) }
        }
    }
}
impl StreamWithSize for FileHandlerStream {
    fn get_size(&self) -> FxFileSize {
        self.handler.GetSize.map_or(0, |get| {
            i64::try_from(unsafe { get(self.handler.clientData) }).unwrap_or(i64::MAX)
        })
    }
}
impl SeekableReadStream for FileHandlerStream {
    fn get_position(&self) -> FxFileSize {
        self.position.get()
    }
    fn is_eof(&self) -> bool {
        self.position.get() >= self.get_size()
    }
    fn read_block_at_offset(&self, buffer: &mut [u8], offset: FxFileSize) -> bool {
        if buffer.is_empty() || offset < 0 {
            return false;
        }
        let Some(callback) = self.handler.ReadBlock else {
            return false;
        };
        let Some(end) = offset.checked_add(buffer.len() as i64) else {
            return false;
        };
        let Ok(offset) = c_ulong::try_from(offset) else {
            return false;
        };
        let Ok(size) = c_ulong::try_from(buffer.len()) else {
            return false;
        };
        if unsafe {
            callback(
                self.handler.clientData,
                offset,
                buffer.as_mut_ptr().cast(),
                size,
            )
        } != 0
        {
            return false;
        }
        self.position.set(end);
        true
    }
}
impl WriteStream for FileHandlerStream {
    fn write_block(&mut self, buffer: &[u8]) -> bool {
        let Some(callback) = self.handler.WriteBlock else {
            return false;
        };
        let offset = self.get_size();
        let Some(end) = offset.checked_add(buffer.len() as i64) else {
            return false;
        };
        let Ok(offset) = c_ulong::try_from(offset) else {
            return false;
        };
        let Ok(size) = c_ulong::try_from(buffer.len()) else {
            return false;
        };
        if unsafe {
            callback(
                self.handler.clientData,
                offset,
                buffer.as_ptr().cast(),
                size,
            )
        } != 0
        {
            return false;
        }
        self.position.set(end);
        true
    }
}
impl SeekableWriteStream for FileHandlerStream {
    fn flush(&mut self) -> bool {
        self.handler
            .Flush
            .is_none_or(|callback| unsafe { callback(self.handler.clientData) == 0 })
    }
}
impl FfiHost {
    fn xfa_enabled(&self) -> bool {
        !self.released.get()
            && unsafe { (*self.info).version >= 2 && (*self.info).xfa_disabled == 0 }
    }
    fn xfa_page_handle(&self, page: &SdkPage) -> Option<*mut c_void> {
        if let Some(raw) = self.page_handle(page) {
            return Some(raw);
        }
        let owner = <Self as FormFillHost>::get_page(self, page.index())?;
        if owner.id() != page.id() {
            return None;
        }
        self.page_handle(page)
    }
    fn open_xfa_file(&self, kind: i32, path: &[u32], mode: &str) -> Option<FileHandlerStream> {
        if !self.xfa_enabled() {
            return None;
        }
        let callback = unsafe { (*self.info).FFI_OpenFile }?;
        let path = wide(path);
        let mode = std::ffi::CString::new(mode).ok()?;
        let host = self.self_weak.upgrade()?;
        let raw = unsafe { callback(self.info, kind, path.as_ptr(), mode.as_ptr()) };
        unsafe { FileHandlerStream::new(raw, host) }
    }
}
fn wide(value: &[u32]) -> Vec<u16> {
    utf16_encode(value).into_iter().chain(Some(0)).collect()
}
impl XfaSdkHost for FfiHost {
    fn app_name(&self) -> Vec<u32> {
        if self.xfa_enabled() {
            "Acrobat".bytes().map(u32::from).collect()
        } else {
            Vec::new()
        }
    }
    fn download_url(&self, url: &[u32]) -> Option<Rc<dyn SeekableReadStream>> {
        if !self.xfa_enabled() {
            return None;
        }
        let callback = unsafe { (*self.info).FFI_DownloadFromURL }?;
        let url = wide(url);
        let host = self.self_weak.upgrade()?;
        let raw = unsafe { callback(self.info, url.as_ptr()) };
        Some(Rc::new(unsafe { FileHandlerStream::new(raw, host) }?))
    }
    fn post_request_url(
        &self,
        url: &[u32],
        data: &[u32],
        content_type: &[u32],
        encoding: &[u32],
        header: &[u32],
    ) -> Option<Vec<u32>> {
        if !self.xfa_enabled() {
            return None;
        }
        let callback = unsafe { (*self.info).FFI_PostRequestURL }?;
        let (url, data, content_type, encoding, header) = (
            wide(url),
            wide(data),
            wide(content_type),
            wide(encoding),
            wide(header),
        );
        let mut response = FPDF_BSTR {
            str_: std::ptr::null_mut(),
            len: 0,
        };
        let success = unsafe {
            callback(
                self.info,
                url.as_ptr(),
                data.as_ptr(),
                content_type.as_ptr(),
                encoding.as_ptr(),
                header.as_ptr(),
                &mut response,
            )
        } != 0;
        let output = if success && response.len >= 0 && !response.str_.is_null() {
            Some(utf16_decode(
                unsafe {
                    std::slice::from_raw_parts(response.str_.cast::<u8>(), response.len as usize)
                },
                false,
            ))
        } else if success {
            Some(Vec::new())
        } else {
            None
        };
        unsafe { FPDF_BStr_Clear(&mut response) };
        output
    }
    fn put_request_url(&self, url: &[u32], data: &[u32], encoding: &[u32]) -> bool {
        if !self.xfa_enabled() {
            return false;
        }
        let Some(callback) = (unsafe { (*self.info).FFI_PutRequestURL }) else {
            return false;
        };
        let (url, data, encoding) = (wide(url), wide(data), wide(encoding));
        unsafe { callback(self.info, url.as_ptr(), data.as_ptr(), encoding.as_ptr()) != 0 }
    }
    fn display_caret(&self, page: &SdkPage, visible: bool, rect: CfxFloatRect) {
        if !self.xfa_enabled() {
            return;
        }
        let Some(callback) = (unsafe { (*self.info).FFI_DisplayCaret }) else {
            return;
        };
        let Some(page) = self.xfa_page_handle(page) else {
            return;
        };
        unsafe {
            callback(
                self.info,
                page.cast(),
                visible as c_int,
                rect.left as f64,
                rect.top as f64,
                rect.right as f64,
                rect.bottom as f64,
            )
        }
    }
    fn page_view_rect(&self, page: &SdkPage) -> Option<CfxFloatRect> {
        if !self.xfa_enabled() {
            return None;
        }
        let callback = unsafe { (*self.info).FFI_GetPageViewRect }?;
        let page = self.xfa_page_handle(page)?;
        let (mut left, mut top, mut right, mut bottom) = (0., 0., 0., 0.);
        unsafe {
            callback(
                self.info,
                page.cast(),
                &mut left,
                &mut top,
                &mut right,
                &mut bottom,
            )
        };
        Some(CfxFloatRect {
            left: left as f32,
            top: top as f32,
            right: right as f32,
            bottom: bottom as f32,
        })
    }
    fn popup_menu(&self, page: &SdkPage, flags: u32, point: CfxPointF) -> bool {
        if !self.xfa_enabled() {
            return false;
        }
        let Some(callback) = (unsafe { (*self.info).FFI_PopupMenu }) else {
            return false;
        };
        let Some(page) = self.xfa_page_handle(page) else {
            return false;
        };
        unsafe {
            callback(
                self.info,
                page.cast(),
                std::ptr::null_mut(),
                flags as c_int,
                point.x,
                point.y,
            ) != 0
        }
    }
    fn page_event(&self, count: i32, event: PageViewEvent) {
        if !self.xfa_enabled() {
            return;
        }
        if let Some(callback) = unsafe { (*self.info).FFI_PageEvent } {
            unsafe { callback(self.info, count, event as c_ulong) }
        }
    }
    fn current_page_index(&self) -> i32 {
        if !self.xfa_enabled() {
            return -1;
        }
        unsafe { (*self.info).FFI_GetCurrentPageIndex }.map_or(-1, |callback| unsafe {
            callback(self.info, self.document.cast())
        })
    }
    fn set_current_page(&self, index: i32) {
        if !self.xfa_enabled() {
            return;
        }
        if let Some(callback) = unsafe { (*self.info).FFI_SetCurrentPage } {
            unsafe { callback(self.info, self.document.cast(), index) }
        }
    }
    fn goto_url(&self, url: &[u32]) {
        if !self.xfa_enabled() {
            return;
        }
        if let Some(callback) = unsafe { (*self.info).FFI_GotoURL } {
            let url = wide(url);
            unsafe { callback(self.info, self.document.cast(), url.as_ptr()) }
        }
    }
    fn open_file_read(
        &self,
        kind: i32,
        path: &[u32],
        mode: &str,
    ) -> Option<Rc<dyn SeekableReadStream>> {
        Some(Rc::new(self.open_xfa_file(kind, path, mode)?))
    }
    fn open_file_write(
        &self,
        kind: i32,
        path: &[u32],
        mode: &str,
    ) -> Option<Box<dyn SeekableWriteStream>> {
        Some(Box::new(self.open_xfa_file(kind, path, mode)?))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bstr_native_null_empty_set_and_explicit_length_cases() {
        unsafe {
            assert_eq!(FPDF_BStr_Init(std::ptr::null_mut()), -1);
            assert_eq!(
                FPDF_BStr_Set(std::ptr::null_mut(), c"clams".as_ptr(), -1),
                -1
            );
            assert_eq!(FPDF_BStr_Clear(std::ptr::null_mut()), -1);
            let mut bstr = FPDF_BSTR {
                str_: std::ptr::null_mut(),
                len: 0,
            };
            assert_eq!(FPDF_BStr_Init(&mut bstr), 0);
            assert!(bstr.str_.is_null());
            assert_eq!(bstr.len, 0);
            assert_eq!(FPDF_BStr_Clear(&mut bstr), 0);
            assert_eq!(FPDF_BStr_Set(&mut bstr, c"clams".as_ptr(), -1), 0);
            assert_eq!(bstr.len, 5);
            assert_eq!(CStr::from_ptr(bstr.str_).to_bytes(), b"clams");
            assert_eq!(FPDF_BStr_Clear(&mut bstr), 0);
            assert!(bstr.str_.is_null());
            assert_eq!(bstr.len, 0);
            assert_eq!(FPDF_BStr_Init(&mut bstr), 0);
            for length in [3, 5, 1] {
                assert_eq!(FPDF_BStr_Set(&mut bstr, c"clams".as_ptr(), length), 0);
                assert_eq!(bstr.len, length);
                assert_eq!(
                    CStr::from_ptr(bstr.str_).to_bytes(),
                    &b"clams"[..length as usize]
                );
            }
            assert_eq!(FPDF_BStr_Set(&mut bstr, c"clams".as_ptr(), 0), 0);
            assert_eq!(bstr.len, 0);
            assert!(bstr.str_.is_null());
            assert_eq!(FPDF_BStr_Clear(&mut bstr), 0);
        }
    }
}
