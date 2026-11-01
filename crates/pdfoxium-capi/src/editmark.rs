// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Opaque content-mark handles for `fpdfsdk/fpdf_editpage.cpp`.
//! Boxes belong to their page-object handles and hold weak engine references.
//! They remain stable across mark-list changes and object ownership transfers.

use super::*;
use pdfoxium_core::fpdfapi::page::cpdf_contentmarkitem::CpdfContentMarkItem;
use pdfoxium_core::fpdfapi::page::cpdf_contentmarks::MarkItemRef;
use pdfoxium_core::fpdfsdk::fpdf_editmark as engine;
use std::rc::Weak;

pub(super) struct MarkHandle {
    mark: Weak<RefCell<CpdfContentMarkItem>>,
}

unsafe fn mark_ref(mark: *mut c_void) -> Option<MarkItemRef> {
    if mark.is_null() {
        return None;
    }
    // SAFETY: the caller supplies a live library-owned mark handle.
    unsafe { &*mark.cast::<MarkHandle>() }.mark.upgrade()
}

unsafe fn cache_mark(object: *mut c_void, mark: &MarkItemRef) -> *mut c_void {
    let key = Rc::as_ptr(mark) as usize;
    // SAFETY: `object` is live and its engine borrow ended before this call.
    let object = unsafe { &mut *object.cast::<PageObjectHandle>() };
    let handle = object.mark_handles.entry(key).or_insert_with(|| {
        Box::new(MarkHandle {
            mark: Rc::downgrade(mark),
        })
    });
    std::ptr::from_mut(handle.as_mut()).cast()
}

unsafe fn byte_string<'a>(value: *const c_char) -> &'a [u8] {
    if value.is_null() {
        return &[];
    }
    // SAFETY: the caller supplies a NUL-terminated FPDF_BYTESTRING.
    unsafe { CStr::from_ptr(value) }.to_bytes()
}

unsafe fn copy_bytes(
    bytes: &[u8],
    buffer: *mut u8,
    buflen: c_ulong,
    out_buflen: *mut c_ulong,
) -> c_int {
    if out_buflen.is_null() {
        return 0;
    }
    let Ok(required) = c_ulong::try_from(bytes.len()) else {
        return 0;
    };
    if !buffer.is_null() && buflen >= required && !bytes.is_empty() {
        // SAFETY: the API's caller provides at least `buflen` writable bytes.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len()) };
    }
    // SAFETY: required writable out parameter, checked non-null above.
    unsafe {
        *out_buflen = required;
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_GetMarkedContentID(object: *mut c_void) -> c_int {
    let Some(count) =
        (unsafe { with_page_object(object, |object| object.base().content_marks().count_items()) })
    else {
        return -1;
    };
    // Resolve each item's dictionary after releasing the page-object borrow,
    // since an indirect properties dictionary can invoke input callbacks.
    for index in 0..count {
        let Some(mark) =
            (unsafe { with_page_object(object, |object| engine::mark_at(object, index)) })
                .flatten()
        else {
            continue;
        };
        let Some(params) = engine::mark_params(&mark) else {
            continue;
        };
        let value = {
            let borrowed = params.borrow();
            let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
                continue;
            };
            dict.get_object_for(b"MCID")
        };
        if let Some(value) = value {
            return value.borrow().get_integer();
        }
    }
    -1
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_CountMarks(object: *mut c_void) -> c_int {
    unsafe {
        with_page_object(object, |object| {
            c_int::try_from(object.base().content_marks().count_items()).unwrap_or(-1)
        })
    }
    .unwrap_or(-1)
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_GetMark(object: *mut c_void, index: c_ulong) -> *mut c_void {
    let Ok(index) = usize::try_from(index) else {
        return std::ptr::null_mut();
    };
    let Some(mark) =
        (unsafe { with_page_object(object, |object| engine::mark_at(object, index)) }).flatten()
    else {
        return std::ptr::null_mut();
    };
    unsafe { cache_mark(object, &mark) }
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_AddMark(
    object: *mut c_void,
    name: *const c_char,
) -> *mut c_void {
    let name = unsafe { byte_string(name) };
    let Some(mark) = (unsafe { with_page_object(object, |object| engine::add_mark(object, name)) })
    else {
        return std::ptr::null_mut();
    };
    unsafe { cache_mark(object, &mark) }
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_AddExistingMark(
    object: *mut c_void,
    mark: *mut c_void,
) -> c_int {
    let Some(mark) = (unsafe { mark_ref(mark) }) else {
        return 0;
    };
    unsafe { with_page_object(object, |object| engine::add_existing_mark(object, mark)) }.is_some()
        as c_int
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_RemoveMark(object: *mut c_void, mark: *mut c_void) -> c_int {
    let Some(mark) = (unsafe { mark_ref(mark) }) else {
        return 0;
    };
    let removed = unsafe { with_page_object(object, |object| engine::remove_mark(object, &mark)) }
        .unwrap_or(false);
    if removed {
        // SAFETY: the object was successfully resolved above; removal invalidates
        // its cached mark handle under the public API's lifetime contract.
        unsafe { &mut *object.cast::<PageObjectHandle>() }
            .mark_handles
            .remove(&(Rc::as_ptr(&mark) as usize));
    }
    removed as c_int
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_GetName(
    mark: *mut c_void,
    buffer: *mut u16,
    buflen: c_ulong,
    out_buflen: *mut c_ulong,
) -> c_int {
    if out_buflen.is_null() {
        return 0;
    }
    let Some(mark) = (unsafe { mark_ref(mark) }) else {
        return 0;
    };
    let bytes = engine::utf8_to_utf16le(mark.borrow().name());
    unsafe { copy_bytes(&bytes, buffer.cast(), buflen, out_buflen) }
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_CountParams(mark: *mut c_void) -> c_int {
    let Some(mark) = (unsafe { mark_ref(mark) }) else {
        return -1;
    };
    c_int::try_from(engine::param_count(&mark)).unwrap_or(-1)
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_GetParamKey(
    mark: *mut c_void,
    index: c_ulong,
    buffer: *mut u16,
    buflen: c_ulong,
    out_buflen: *mut c_ulong,
) -> c_int {
    if out_buflen.is_null() {
        return 0;
    }
    let Some(mark) = (unsafe { mark_ref(mark) }) else {
        return 0;
    };
    let Ok(index) = usize::try_from(index) else {
        return 0;
    };
    let Some(key) = engine::param_key(&mark, index) else {
        return 0;
    };
    unsafe {
        copy_bytes(
            &engine::utf8_to_utf16le(&key),
            buffer.cast(),
            buflen,
            out_buflen,
        )
    }
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_GetParamValueType(
    mark: *mut c_void,
    key: *const c_char,
) -> c_int {
    let Some(mark) = (unsafe { mark_ref(mark) }) else {
        return 0;
    };
    engine::param_type(&mark, unsafe { byte_string(key) }).map_or(0, |kind| kind as c_int)
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_GetParamIntValue(
    mark: *mut c_void,
    key: *const c_char,
    out_value: *mut c_int,
) -> c_int {
    if out_value.is_null() {
        return 0;
    }
    let Some(mark) = (unsafe { mark_ref(mark) }) else {
        return 0;
    };
    let Some(value) = engine::param_int(&mark, unsafe { byte_string(key) }) else {
        return 0;
    };
    // SAFETY: the caller supplies a writable out parameter.
    unsafe {
        *out_value = value;
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_GetParamFloatValue(
    mark: *mut c_void,
    key: *const c_char,
    out_value: *mut f32,
) -> c_int {
    if out_value.is_null() {
        return 0;
    }
    let Some(mark) = (unsafe { mark_ref(mark) }) else {
        return 0;
    };
    let Some(value) = engine::param_float(&mark, unsafe { byte_string(key) }) else {
        return 0;
    };
    // SAFETY: the caller supplies a writable out parameter.
    unsafe {
        *out_value = value;
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_GetParamStringValue(
    mark: *mut c_void,
    key: *const c_char,
    buffer: *mut u16,
    buflen: c_ulong,
    out_buflen: *mut c_ulong,
) -> c_int {
    if out_buflen.is_null() {
        return 0;
    }
    let Some(mark) = (unsafe { mark_ref(mark) }) else {
        return 0;
    };
    let Some(value) = engine::param_bytes(&mark, unsafe { byte_string(key) }) else {
        return 0;
    };
    unsafe {
        copy_bytes(
            &engine::utf8_to_utf16le(&value),
            buffer.cast(),
            buflen,
            out_buflen,
        )
    }
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_GetParamBlobValue(
    mark: *mut c_void,
    key: *const c_char,
    buffer: *mut u8,
    buflen: c_ulong,
    out_buflen: *mut c_ulong,
) -> c_int {
    if out_buflen.is_null() {
        return 0;
    }
    let Some(mark) = (unsafe { mark_ref(mark) }) else {
        return 0;
    };
    let Some(value) = engine::param_bytes(&mark, unsafe { byte_string(key) }) else {
        return 0;
    };
    unsafe { copy_bytes(&value, buffer, buflen, out_buflen) }
}

unsafe fn set_param(
    document: *mut c_void,
    object: *mut c_void,
    mark: *mut c_void,
    key: *const c_char,
    value: engine::ParamValue<'_>,
) -> c_int {
    if document.is_null() {
        return 0;
    }
    let Some(mark) = (unsafe { mark_ref(mark) }) else {
        return 0;
    };
    let contains = unsafe {
        with_page_object(object, |object| {
            object.base().content_marks().contains_item(&mark)
        })
    }
    .unwrap_or(false);
    if !contains {
        return 0;
    }
    // SAFETY: the caller supplies a live document. Release its RefCell borrow
    // before resolving mark properties, which may invoke input callbacks.
    let holder = unsafe { &*document.cast::<DocumentHandle>() }
        .document
        .borrow()
        .holder();
    engine::set_param(&holder, &mark, unsafe { byte_string(key) }, value);
    unsafe { with_page_object(object, |object| object.base_mut().set_dirty(true)) }.is_some()
        as c_int
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_SetIntParam(
    document: *mut c_void,
    object: *mut c_void,
    mark: *mut c_void,
    key: *const c_char,
    value: c_int,
) -> c_int {
    unsafe { set_param(document, object, mark, key, engine::ParamValue::Int(value)) }
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_SetFloatParam(
    document: *mut c_void,
    object: *mut c_void,
    mark: *mut c_void,
    key: *const c_char,
    value: f32,
) -> c_int {
    unsafe {
        set_param(
            document,
            object,
            mark,
            key,
            engine::ParamValue::Float(value),
        )
    }
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_SetStringParam(
    document: *mut c_void,
    object: *mut c_void,
    mark: *mut c_void,
    key: *const c_char,
    value: *const c_char,
) -> c_int {
    unsafe {
        set_param(
            document,
            object,
            mark,
            key,
            engine::ParamValue::String(byte_string(value)),
        )
    }
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_SetBlobParam(
    document: *mut c_void,
    object: *mut c_void,
    mark: *mut c_void,
    key: *const c_char,
    value: *const u8,
    value_len: c_ulong,
) -> c_int {
    let Ok(len) = usize::try_from(value_len) else {
        return 0;
    };
    if len > isize::MAX as usize || (len != 0 && value.is_null()) {
        return 0;
    }
    let bytes = if len == 0 {
        &[]
    } else {
        // SAFETY: the caller supplies `value_len` readable bytes.
        unsafe { std::slice::from_raw_parts(value, len) }
    };
    unsafe { set_param(document, object, mark, key, engine::ParamValue::Blob(bytes)) }
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObjMark_RemoveParam(
    object: *mut c_void,
    mark: *mut c_void,
    key: *const c_char,
) -> c_int {
    if object.is_null() {
        return 0;
    }
    let Some(mark) = (unsafe { mark_ref(mark) }) else {
        return 0;
    };
    if !engine::remove_param(&mark, unsafe { byte_string(key) }) {
        return 0;
    }
    unsafe { with_page_object(object, |object| object.base_mut().set_dirty(true)) }.is_some()
        as c_int
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject;

    // Source-derived MarkGetName/GetParamKey/IntParam/FloatParam/StringParam
    // contracts, plus binary blobs and shared identity under AddExistingMark.
    #[test]
    fn mark_handles_typed_values_buffers_and_shared_ownership() {
        unsafe {
            let document = FPDF_CreateNewDocument();
            let first = new_owned_page_object(Box::new(CpdfPathObject::new(-1)));
            let second = new_owned_page_object(Box::new(CpdfPathObject::new(-1)));
            let mark = FPDFPageObj_AddMark(first, c"Bounds".as_ptr());
            assert!(!mark.is_null());
            assert_eq!(FPDFPageObj_GetMark(first, 0), mark);
            assert_eq!(FPDFPageObj_CountMarks(first), 1);
            assert_eq!(FPDFPageObj_CountMarks(std::ptr::null_mut()), -1);
            assert_eq!(FPDFPageObjMark_CountParams(mark), 0);
            assert_eq!(FPDFPageObj_AddExistingMark(second, mark), 1);
            assert_eq!(
                FPDFPageObjMark_SetIntParam(document, first, mark, c"MCID".as_ptr(), 7),
                1
            );
            assert_eq!(
                FPDFPageObjMark_SetFloatParam(document, first, mark, c"Factor".as_ptr(), 3.25),
                1
            );
            assert_eq!(
                FPDFPageObjMark_SetStringParam(
                    document,
                    first,
                    mark,
                    c"Position".as_ptr(),
                    c"Last".as_ptr()
                ),
                1
            );
            let blob = [0u8, 255, 0, 0x42];
            assert_eq!(
                FPDFPageObjMark_SetBlobParam(
                    document,
                    first,
                    mark,
                    c"Blob".as_ptr(),
                    blob.as_ptr(),
                    4
                ),
                1
            );
            assert_eq!(FPDFPageObjMark_CountParams(mark), 4);
            assert_eq!(FPDFPageObj_GetMarkedContentID(first), 7);
            assert_eq!(FPDFPageObj_GetMarkedContentID(second), 7);
            let second_mark = FPDFPageObj_GetMark(second, 0);
            assert_eq!(FPDFPageObjMark_CountParams(second_mark), 4);

            let mut length = 999;
            let mut buffer = [0xccccu16; 32];
            assert_eq!(
                FPDFPageObjMark_GetName(mark, std::ptr::null_mut(), 0, &mut length),
                1
            );
            assert_eq!(length, 14);
            assert_eq!(
                FPDFPageObjMark_GetName(mark, buffer.as_mut_ptr(), 12, &mut length),
                1
            );
            assert_eq!(buffer, [0xcccc; 32]);
            assert_eq!(
                FPDFPageObjMark_GetName(mark, buffer.as_mut_ptr(), 64, &mut length),
                1
            );
            assert_eq!(&buffer[..7], &[66, 111, 117, 110, 100, 115, 0]);
            length = 999;
            assert_eq!(
                FPDFPageObjMark_GetName(std::ptr::null_mut(), buffer.as_mut_ptr(), 64, &mut length),
                0
            );
            assert_eq!(length, 999);
            assert_eq!(
                FPDFPageObjMark_GetName(mark, buffer.as_mut_ptr(), 64, std::ptr::null_mut()),
                0
            );
            assert_eq!(
                FPDFPageObjMark_GetParamKey(mark, 2, buffer.as_mut_ptr(), 64, &mut length),
                1
            );
            assert_eq!(&buffer[..5], &[77, 67, 73, 68, 0]);
            length = 999;
            assert_eq!(
                FPDFPageObjMark_GetParamKey(mark, 4, buffer.as_mut_ptr(), 64, &mut length),
                0
            );
            assert_eq!(length, 999);
            assert_eq!(
                FPDFPageObjMark_GetParamStringValue(
                    mark,
                    c"Position".as_ptr(),
                    buffer.as_mut_ptr(),
                    64,
                    &mut length
                ),
                1
            );
            assert_eq!(length, 10);
            assert_eq!(&buffer[..5], &[76, 97, 115, 116, 0]);
            let mut value = 999;
            assert_eq!(
                FPDFPageObjMark_GetParamIntValue(mark, c"MCID".as_ptr(), &mut value),
                1
            );
            assert_eq!(value, 7);
            assert_eq!(
                FPDFPageObjMark_GetParamIntValue(mark, c"Position".as_ptr(), &mut value),
                0
            );
            assert_eq!(value, 7);
            let mut float = 0.;
            assert_eq!(
                FPDFPageObjMark_GetParamFloatValue(mark, c"Factor".as_ptr(), &mut float),
                1
            );
            assert_eq!(float, 3.25);
            let mut bytes = [0xcc; 4];
            assert_eq!(
                FPDFPageObjMark_GetParamBlobValue(
                    mark,
                    c"Blob".as_ptr(),
                    bytes.as_mut_ptr(),
                    3,
                    &mut length
                ),
                1
            );
            assert_eq!(bytes, [0xcc; 4]);
            assert_eq!(length, 4);
            assert_eq!(
                FPDFPageObjMark_GetParamBlobValue(
                    mark,
                    c"Blob".as_ptr(),
                    bytes.as_mut_ptr(),
                    4,
                    &mut length
                ),
                1
            );
            assert_eq!(bytes, blob);
            assert_eq!(
                FPDFPageObjMark_RemoveParam(first, mark, c"Factor".as_ptr()),
                1
            );
            assert_eq!(
                FPDFPageObjMark_RemoveParam(first, mark, c"Factor".as_ptr()),
                0
            );
            assert_eq!(FPDFPageObj_RemoveMark(first, mark), 1);
            assert_eq!(FPDFPageObj_CountMarks(first), 0);
            assert_eq!(FPDFPageObjMark_CountParams(second_mark), 3);
            FPDFPageObj_Destroy(first);
            assert_eq!(FPDFPageObj_GetMarkedContentID(second), 7);
            FPDFPageObj_Destroy(second);
            FPDF_CloseDocument(document);
        }
    }
}
