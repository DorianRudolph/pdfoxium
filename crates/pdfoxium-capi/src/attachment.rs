// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! File-attachment C ABI. Cached aliases borrow their document's lifetime;
//! engine dictionaries and buffers remain owned by safe Rust.
use super::*;
use pdfoxium_core::fpdfapi::parser::cpdf_object::ObjectRef;
use pdfoxium_core::fpdfsdk::fpdf_attachment as sdk;
use pdfoxium_core::fxcrt::fx_string::utf16_encode;

pub(super) struct AttachmentHandle {
    object: ObjectRef,
}
pub(super) fn attachment_handle(cache: &AttachmentHandleCache, object: ObjectRef) -> *mut c_void {
    let identity = Rc::as_ptr(&object) as *const () as usize;
    let mut cache = cache.borrow_mut();
    let handle = cache
        .entry(identity)
        .or_insert_with(|| Box::new(AttachmentHandle { object }));
    (&mut **handle as *mut AttachmentHandle).cast()
}
unsafe fn attachment_object(attachment: *mut c_void) -> Option<ObjectRef> {
    Some(
        unsafe { attachment.cast::<AttachmentHandle>().as_ref()? }
            .object
            .clone(),
    )
}
unsafe fn byte_string<'a>(value: *const c_char) -> &'a [u8] {
    if value.is_null() {
        &[]
    } else {
        unsafe { CStr::from_ptr(value) }.to_bytes()
    }
}
unsafe fn wide_string(value: *const u16) -> Vec<u32> {
    if value.is_null() {
        return Vec::new();
    }
    let mut length = 0;
    while unsafe { *value.add(length) } != 0 {
        length += 1;
    }
    char::decode_utf16(
        unsafe { std::slice::from_raw_parts(value, length) }
            .iter()
            .copied(),
    )
    .map(|c| c.map_or(0xfffd, u32::from))
    .collect()
}
unsafe fn copy_wide(text: &[u32], buffer: *mut u16, buflen: c_ulong) -> c_ulong {
    let words: Vec<u16> = utf16_encode(text)
        .into_iter()
        .chain(std::iter::once(0))
        .collect();
    let size = words.len() * 2;
    if !buffer.is_null() && buflen as usize >= size {
        unsafe {
            std::ptr::copy_nonoverlapping(words.as_ptr(), buffer, words.len());
        }
    }
    size as c_ulong
}

#[no_mangle]
pub unsafe extern "C" fn FPDFDoc_GetAttachmentCount(document: *mut c_void) -> c_int {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    let parts = sdk::AttachmentDocument::from_document(&document.document.borrow());
    c_int::try_from(sdk::attachment_count(&parts)).unwrap_or(c_int::MAX)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFDoc_AddAttachment(
    document: *mut c_void,
    name: *const u16,
) -> *mut c_void {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let name = unsafe { wide_string(name) };
    let parts = sdk::AttachmentDocument::from_document(&document.document.borrow());
    let file = sdk::add_attachment(&parts, &name);
    file.map_or(std::ptr::null_mut(), |file| {
        attachment_handle(&document.attachment_handles, file)
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDFDoc_GetAttachment(document: *mut c_void, index: c_int) -> *mut c_void {
    let (Some(document), Ok(index)) = (
        (unsafe { document.cast::<DocumentHandle>().as_ref() }),
        usize::try_from(index),
    ) else {
        return std::ptr::null_mut();
    };
    let parts = sdk::AttachmentDocument::from_document(&document.document.borrow());
    let file = sdk::attachment_at(&parts, index);
    file.map_or(std::ptr::null_mut(), |file| {
        attachment_handle(&document.attachment_handles, file)
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDFDoc_DeleteAttachment(document: *mut c_void, index: c_int) -> c_int {
    let (Some(document), Ok(index)) = (
        (unsafe { document.cast::<DocumentHandle>().as_ref() }),
        usize::try_from(index),
    ) else {
        return 0;
    };
    let parts = sdk::AttachmentDocument::from_document(&document.document.borrow());
    sdk::delete_attachment(&parts, index) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAttachment_GetName(
    attachment: *mut c_void,
    buffer: *mut u16,
    buflen: c_ulong,
) -> c_ulong {
    let Some(file) = (unsafe { attachment_object(attachment) }) else {
        return 0;
    };
    unsafe { copy_wide(&sdk::name(&file), buffer, buflen) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAttachment_HasKey(
    attachment: *mut c_void,
    key: *const c_char,
) -> c_int {
    let Some(file) = (unsafe { attachment_object(attachment) }) else {
        return 0;
    };
    sdk::has_key(&file, unsafe { byte_string(key) }) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAttachment_GetValueType(
    attachment: *mut c_void,
    key: *const c_char,
) -> c_int {
    let Some(file) = (unsafe { attachment_object(attachment) }) else {
        return 0;
    };
    sdk::value_type(&file, unsafe { byte_string(key) }).map_or(0, |kind| kind as c_int)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAttachment_SetStringValue(
    attachment: *mut c_void,
    key: *const c_char,
    value: *const u16,
) -> c_int {
    let Some(file) = (unsafe { attachment_object(attachment) }) else {
        return 0;
    };
    sdk::set_string_value(&file, unsafe { byte_string(key) }, &unsafe {
        wide_string(value)
    }) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAttachment_GetStringValue(
    attachment: *mut c_void,
    key: *const c_char,
    buffer: *mut u16,
    buflen: c_ulong,
) -> c_ulong {
    let Some(file) = (unsafe { attachment_object(attachment) }) else {
        return 0;
    };
    let Some(value) = sdk::string_value(&file, unsafe { byte_string(key) }) else {
        return 0;
    };
    unsafe { copy_wide(&value, buffer, buflen) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAttachment_SetDescription(
    attachment: *mut c_void,
    value: *const u16,
) -> c_int {
    let Some(file) = (unsafe { attachment_object(attachment) }) else {
        return 0;
    };
    sdk::set_description(&file, &unsafe { wide_string(value) }) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAttachment_GetDescription(
    attachment: *mut c_void,
    buffer: *mut u16,
    buflen: c_ulong,
) -> c_ulong {
    let Some(file) = (unsafe { attachment_object(attachment) }) else {
        return 0;
    };
    let Some(value) = sdk::description(&file) else {
        return 0;
    };
    unsafe { copy_wide(&value, buffer, buflen) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAttachment_SetFile(
    attachment: *mut c_void,
    document: *mut c_void,
    contents: *const c_void,
    len: c_ulong,
) -> c_int {
    if (contents.is_null() && len != 0) || len > i32::MAX as c_ulong {
        return 0;
    }
    let Some(file) = (unsafe { attachment_object(attachment) }) else {
        return 0;
    };
    let document = {
        let Some(handle) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
            return 0;
        };
        handle.document.clone()
    };
    let objects = document.borrow().holder();
    let contents = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(contents.cast::<u8>(), len as usize) }
    };
    let Some((fields, _utc)) = document_date_fields(Some(&document)) else {
        return 0;
    };
    // Native attachment dates have local fields and no timezone suffix.
    let date = format!(
        "D:{}{:02}{:02}{:02}{:02}{:02}",
        fields.year, fields.month, fields.day, fields.hour, fields.minute, fields.second
    );
    sdk::set_file(&file, &objects, contents, date.as_bytes()) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAttachment_GetFile(
    attachment: *mut c_void,
    buffer: *mut c_void,
    buflen: c_ulong,
    out_buflen: *mut c_ulong,
) -> c_int {
    if out_buflen.is_null() {
        return 0;
    }
    let Some(file) = (unsafe { attachment_object(attachment) }) else {
        return 0;
    };
    sdk::with_file(&file, |bytes| {
        let Ok(length) = c_ulong::try_from(bytes.len()) else {
            return 0;
        };
        unsafe {
            *out_buflen = length;
        }
        if !buffer.is_null() && buflen >= length && !bytes.is_empty() {
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast::<u8>(), bytes.len());
            }
        }
        1
    })
    .unwrap_or(0)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAttachment_GetSubtype(
    attachment: *mut c_void,
    buffer: *mut u16,
    buflen: c_ulong,
) -> c_ulong {
    let Some(file) = (unsafe { attachment_object(attachment) }) else {
        return 0;
    };
    unsafe { copy_wide(&sdk::subtype(&file), buffer, buflen) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAttachment_GetAFRelationship(
    attachment: *mut c_void,
    buffer: *mut u16,
    buflen: c_ulong,
) -> c_ulong {
    let Some(file) = (unsafe { attachment_object(attachment) }) else {
        return 0;
    };
    let Some(value) = sdk::af_relationship(&file) else {
        return 0;
    };
    unsafe { copy_wide(&value, buffer, buflen) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFileAttachment(annotation: *mut c_void) -> *mut c_void {
    let Some(annotation) = (unsafe { annotation.cast::<super::annot::AnnotHandle>().as_ref() })
    else {
        return std::ptr::null_mut();
    };
    let Some(dict) = annotation.pdf_dict() else {
        return std::ptr::null_mut();
    };
    let file = sdk::annotation_attachment(&dict);
    let Some(file) = file else {
        return std::ptr::null_mut();
    };
    let cache = unsafe { &(*annotation.page.as_ptr()).attachment_handles };
    attachment_handle(cache, file)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_AddFileAttachment(
    annotation: *mut c_void,
    name: *const u16,
) -> *mut c_void {
    let Some(annotation) = (unsafe { annotation.cast::<super::annot::AnnotHandle>().as_ref() })
    else {
        return std::ptr::null_mut();
    };
    let name = unsafe { wide_string(name) };
    let objects = unsafe { &(*annotation.page.as_ptr())._document }
        .borrow()
        .holder();
    let Some(dict) = annotation.pdf_dict() else {
        return std::ptr::null_mut();
    };
    let file = sdk::add_annotation_attachment(&dict, &objects, &name);
    let Some(file) = file else {
        return std::ptr::null_mut();
    };
    let cache = unsafe { &(*annotation.page.as_ptr()).attachment_handles };
    attachment_handle(cache, file)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_handles_and_short_buffers_survive_annotation_close() {
        unsafe {
            let document = FPDF_CreateNewDocument();
            let name: Vec<u16> = "payload.dat"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let file = FPDFDoc_AddAttachment(document, name.as_ptr());
            assert!(!file.is_null());
            assert_eq!(FPDFDoc_GetAttachment(document, 0), file);
            let contents = b"\0attachment bytes\xff";
            assert_eq!(
                FPDFAttachment_SetFile(
                    file,
                    document,
                    contents.as_ptr().cast(),
                    contents.len() as c_ulong
                ),
                1
            );
            let mut needed = 0;
            assert_eq!(
                FPDFAttachment_GetFile(file, std::ptr::null_mut(), 0, &mut needed),
                1
            );
            assert_eq!(needed, contents.len() as c_ulong);
            let mut bytes = [0xccu8; 64];
            assert_eq!(
                FPDFAttachment_GetFile(file, bytes.as_mut_ptr().cast(), 1, &mut needed),
                1
            );
            assert_eq!(bytes, [0xcc; 64]);
            assert_eq!(
                FPDFAttachment_GetFile(
                    file,
                    bytes.as_mut_ptr().cast(),
                    bytes.len() as c_ulong,
                    &mut needed
                ),
                1
            );
            assert_eq!(&bytes[..needed as usize], contents);
            let page = FPDFPage_New(document, 0, 100., 100.);
            let annotation = FPDFPage_CreateAnnot(page, 17);
            let embedded = FPDFAnnot_AddFileAttachment(annotation, name.as_ptr());
            assert!(!embedded.is_null());
            assert_eq!(FPDFAnnot_GetFileAttachment(annotation), embedded);
            FPDFPage_CloseAnnot(annotation);
            let mut wide = [0xffffu16; 32];
            assert_eq!(
                FPDFAttachment_GetName(embedded, wide.as_mut_ptr(), 64),
                name.len() as c_ulong * 2
            );
            assert_eq!(&wide[..name.len()], name);
            FPDF_ClosePage(page);
            assert_eq!(
                FPDFAttachment_GetName(embedded, std::ptr::null_mut(), 0),
                name.len() as c_ulong * 2
            );
            assert_eq!(FPDFDoc_DeleteAttachment(document, 0), 1);
            assert_eq!(FPDFDoc_GetAttachmentCount(document), 0);
            assert_eq!(
                FPDFAttachment_GetName(file, std::ptr::null_mut(), 0),
                name.len() as c_ulong * 2
            );
            FPDF_CloseDocument(document);
        }
    }
}
