// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
#[repr(C)]
struct Source {
    api: FX_FILEAVAIL,
    bytes: Vec<u8>,
    ranges: Vec<(usize, usize)>,
    avail: *mut c_void,
    reenter: bool,
    nested: Vec<c_int>,
    destroy: bool,
    document: *mut c_void,
}
unsafe extern "C" fn available(api: *mut FX_FILEAVAIL, offset: usize, size: usize) -> c_int {
    let source = api.cast::<Source>();
    // Take all state before calling back; never retain a Rust reference across
    // reentry or handle destruction.
    let (handle, reenter, destroy) =
        unsafe { ((*source).avail, (*source).reenter, (*source).destroy) };
    unsafe {
        (*source).reenter = false;
        (*source).destroy = false;
    }
    if reenter {
        let document = unsafe { (*source).document };
        if !document.is_null() {
            assert_eq!(unsafe { FPDF_GetPageCount(document) }, 2);
            assert_eq!(unsafe { FPDFAvail_GetFirstPageNum(document) }, 0);
        }
        let nested = unsafe { FPDFAvail_IsDocAvail(handle, std::ptr::null_mut()) };
        unsafe {
            (*source).nested.push(nested);
        }
    }
    if destroy {
        unsafe {
            FPDFAvail_Destroy(handle);
            (*source).avail = std::ptr::null_mut();
        }
    }
    let source = unsafe { &*source };
    c_int::from(
        source.ranges.iter().any(|&(start, end)| {
            offset >= start && offset.checked_add(size).is_some_and(|n| n <= end)
        }),
    )
}
extern "C" fn read(param: *mut c_void, offset: c_ulong, buffer: *mut u8, size: c_ulong) -> c_int {
    let source = unsafe { &*param.cast::<Source>() };
    let Ok(offset) = usize::try_from(offset) else {
        return 0;
    };
    let Ok(size) = usize::try_from(size) else {
        return 0;
    };
    let Some(data) = offset
        .checked_add(size)
        .and_then(|end| source.bytes.get(offset..end))
    else {
        return 0;
    };
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr(), buffer, size);
    }
    1
}
#[repr(C)]
struct Requests {
    api: FX_DOWNLOADHINTS,
    ranges: Vec<(usize, usize)>,
}
unsafe extern "C" fn add(api: *mut FX_DOWNLOADHINTS, offset: usize, size: usize) {
    unsafe {
        (*api.cast::<Requests>()).ranges.push((offset, size));
    }
}
fn fixture() -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../upstream/pdfium/testing/resources/feature_linearized_loading.pdf",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
#[test]
fn public_availability_callbacks_errors_document_and_lifetime() {
    unsafe {
        assert_eq!(
            FPDFAvail_IsDocAvail(std::ptr::null_mut(), std::ptr::null_mut()),
            PDF_DATA_ERROR
        );
        assert_eq!(
            FPDFAvail_IsPageAvail(std::ptr::null_mut(), 0, std::ptr::null_mut()),
            PDF_DATA_ERROR
        );
        assert_eq!(
            FPDFAvail_IsFormAvail(std::ptr::null_mut(), std::ptr::null_mut()),
            PDF_FORM_ERROR
        );
        assert_eq!(
            FPDFAvail_IsLinearized(std::ptr::null_mut()),
            PDF_LINEARIZATION_UNKNOWN
        );
        assert!(FPDFAvail_GetDocument(std::ptr::null_mut(), std::ptr::null()).is_null());
        assert_eq!(FPDFAvail_GetFirstPageNum(std::ptr::null_mut()), 0);
        FPDFAvail_Destroy(std::ptr::null_mut());
        let mut source = Box::new(Source {
            api: FX_FILEAVAIL {
                version: 1,
                IsDataAvail: Some(available),
            },
            bytes: fixture(),
            ranges: Vec::new(),
            avail: std::ptr::null_mut(),
            reenter: true,
            nested: Vec::new(),
            destroy: false,
            document: std::ptr::null_mut(),
        });
        let mut access = FPDF_FILEACCESS {
            m_FileLen: source.bytes.len() as c_ulong,
            m_GetBlock: Some(read),
            m_Param: (&mut *source as *mut Source).cast(),
        };
        let handle = FPDFAvail_Create(&mut source.api, &mut access);
        assert!(!handle.is_null());
        source.avail = handle;
        assert_eq!(FPDFAvail_IsLinearized(handle), PDF_LINEARIZATION_UNKNOWN);
        assert_eq!(source.nested, [PDF_DATA_ERROR]);
        assert_eq!(
            FPDFAvail_IsPageAvail(handle, -1, std::ptr::null_mut()),
            PDF_DATA_NOTAVAIL
        );
        let mut requests = Requests {
            api: FX_DOWNLOADHINTS {
                version: 1,
                AddSegment: Some(add),
            },
            ranges: Vec::new(),
        };
        let mut polls = 0;
        loop {
            let result = FPDFAvail_IsDocAvail(handle, &mut requests.api);
            if result == PDF_DATA_AVAIL {
                break;
            }
            assert_eq!(result, PDF_DATA_NOTAVAIL);
            assert!(!requests.ranges.is_empty());
            for (offset, size) in std::mem::take(&mut requests.ranges) {
                source.ranges.push((offset, offset + size));
            }
            source.ranges.sort_unstable();
            let mut merged: Vec<(usize, usize)> = Vec::new();
            for (start, end) in std::mem::take(&mut source.ranges) {
                if let Some(last) = merged.last_mut().filter(|last| last.1 >= start) {
                    last.1 = last.1.max(end);
                } else {
                    merged.push((start, end));
                }
            }
            source.ranges = merged;
            polls += 1;
            assert!(polls < 20);
        }
        assert_eq!(FPDFAvail_IsLinearized(handle), PDF_LINEARIZED);
        let document = FPDFAvail_GetDocument(handle, std::ptr::null());
        assert!(!document.is_null());
        assert!(FPDFAvail_GetDocument(handle, std::ptr::null()).is_null());
        assert_eq!(FPDF_GetLastError(), 5);
        assert_eq!(FPDFAvail_GetFirstPageNum(document), 0);
        assert_eq!(FPDF_GetPageCount(document), 2);
        assert_eq!(
            FPDFAvail_IsPageAvail(handle, 0, std::ptr::null_mut()),
            PDF_DATA_AVAIL
        );
        source.document = document;
        source.reenter = true;
        let mut polls = 0;
        loop {
            let result = FPDFAvail_IsPageAvail(handle, 1, &mut requests.api);
            if result == PDF_DATA_AVAIL {
                break;
            }
            assert_eq!(result, PDF_DATA_NOTAVAIL);
            assert!(!requests.ranges.is_empty());
            for (offset, size) in std::mem::take(&mut requests.ranges) {
                source.ranges.push((offset, offset + size));
            }
            source.ranges.sort_unstable();
            let mut merged: Vec<(usize, usize)> = Vec::new();
            for (start, end) in std::mem::take(&mut source.ranges) {
                if let Some(last) = merged.last_mut().filter(|last| last.1 >= start) {
                    last.1 = last.1.max(end);
                } else {
                    merged.push((start, end));
                }
            }
            source.ranges = merged;
            polls += 1;
            assert!(polls < 20);
        }
        assert_eq!(source.nested, [PDF_DATA_ERROR, PDF_DATA_ERROR]);
        let form = FPDFAvail_IsFormAvail(handle, std::ptr::null_mut());
        assert!(form == PDF_FORM_AVAIL || form == PDF_FORM_NOTEXIST);
        FPDF_CloseDocument(document);
        assert_eq!(
            FPDFAvail_IsPageAvail(handle, 0, std::ptr::null_mut()),
            PDF_DATA_ERROR
        );
        FPDFAvail_Destroy(handle);
    }
}
#[test]
fn callback_can_destroy_current_availability_owner() {
    unsafe {
        let mut source = Box::new(Source {
            api: FX_FILEAVAIL {
                version: 1,
                IsDataAvail: Some(available),
            },
            bytes: fixture(),
            ranges: Vec::new(),
            avail: std::ptr::null_mut(),
            reenter: false,
            nested: Vec::new(),
            destroy: true,
            document: std::ptr::null_mut(),
        });
        let mut access = FPDF_FILEACCESS {
            m_FileLen: source.bytes.len() as c_ulong,
            m_GetBlock: Some(read),
            m_Param: (&mut *source as *mut Source).cast(),
        };
        let handle = FPDFAvail_Create(&mut source.api, &mut access);
        source.avail = handle;
        assert_eq!(
            FPDFAvail_IsDocAvail(handle, std::ptr::null_mut()),
            PDF_DATA_NOTAVAIL
        );
        assert!(source.avail.is_null());
    }
}
