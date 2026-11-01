// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Catalog, signature metadata and thumbnail public ABI.
use super::*;
use pdfoxium_core::fpdfsdk::{
    fpdf_catalog as catalog, fpdf_signature as signature, fpdf_thumbnail as thumbnail,
};

#[no_mangle]
pub unsafe extern "C" fn FPDFCatalog_IsTagged(document: *mut c_void) -> c_int {
    unsafe { document.cast::<DocumentHandle>().as_ref() }
        .is_some_and(|document| catalog::is_tagged(&document.document.borrow())) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFCatalog_GetLanguage(
    document: *mut c_void,
    buffer: *mut u16,
    buflen: c_ulong,
) -> c_ulong {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    let Some(text) = catalog::language(&document.document.borrow()) else {
        return 0;
    };
    unsafe { doc::copy_utf16(&text, buffer.cast(), buflen) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFCatalog_SetLanguage(
    document: *mut c_void,
    language: *const u16,
) -> c_int {
    if language.is_null() {
        return 0;
    }
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    catalog::set_language(&document.document.borrow(), &unsafe {
        doc::wide_string(language)
    }) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_GetSignatureCount(document: *mut c_void) -> c_int {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return -1;
    };
    c_int::try_from(signature::collect_signatures(&document.document.borrow()).len())
        .unwrap_or(c_int::MAX)
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_GetSignatureObject(
    document: *mut c_void,
    index: c_int,
) -> *mut c_void {
    let (Some(document), Ok(index)) = (
        unsafe { document.cast::<DocumentHandle>().as_ref() },
        usize::try_from(index),
    ) else {
        return std::ptr::null_mut();
    };
    signature::collect_signatures(&document.document.borrow())
        .get(index)
        .map_or(std::ptr::null_mut(), |object| {
            doc::borrow_object(&document.borrowed_objects, object.clone())
        })
}
#[no_mangle]
pub unsafe extern "C" fn FPDFSignatureObj_GetContents(
    signature: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    let Some(data) =
        (unsafe { doc::borrowed(signature) }).and_then(|object| signature::contents(&object))
    else {
        return 0;
    };
    unsafe { doc::copy_bytes(&data, buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFSignatureObj_GetByteRange(
    signature: *mut c_void,
    buffer: *mut c_int,
    length: c_ulong,
) -> c_ulong {
    let Some(range) =
        (unsafe { doc::borrowed(signature) }).and_then(|object| signature::byte_range(&object))
    else {
        return 0;
    };
    if !buffer.is_null() && length as usize >= range.len() {
        unsafe {
            std::ptr::copy_nonoverlapping(range.as_ptr(), buffer, range.len());
        }
    }
    range.len() as c_ulong
}
#[no_mangle]
pub unsafe extern "C" fn FPDFSignatureObj_GetSubFilter(
    signature: *mut c_void,
    buffer: *mut c_char,
    length: c_ulong,
) -> c_ulong {
    let Some(text) =
        (unsafe { doc::borrowed(signature) }).and_then(|object| signature::sub_filter(&object))
    else {
        return 0;
    };
    unsafe { doc::copy_terminated(&text, buffer.cast(), length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFSignatureObj_GetReason(
    signature: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    let Some(text) =
        (unsafe { doc::borrowed(signature) }).and_then(|object| signature::reason(&object))
    else {
        return 0;
    };
    unsafe { doc::copy_utf16(&text, buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFSignatureObj_GetTime(
    signature: *mut c_void,
    buffer: *mut c_char,
    length: c_ulong,
) -> c_ulong {
    let Some(text) =
        (unsafe { doc::borrowed(signature) }).and_then(|object| signature::time(&object))
    else {
        return 0;
    };
    unsafe { doc::copy_terminated(&text, buffer.cast(), length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFSignatureObj_GetDocMDPPermission(signature: *mut c_void) -> u32 {
    (unsafe { doc::borrowed(signature) }).map_or(0, |object| signature::doc_mdp_permission(&object))
}
#[no_mangle]
pub unsafe extern "C" fn FPDFPage_GetDecodedThumbnailData(
    page: *mut c_void,
    buffer: *mut c_void,
    buflen: c_ulong,
) -> c_ulong {
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return 0;
    };
    thumbnail::with_thumbnail_data(page.pdf_page().unwrap(), true, |data| unsafe {
        doc::copy_bytes(data, buffer, buflen)
    })
    .unwrap_or(0)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFPage_GetRawThumbnailData(
    page: *mut c_void,
    buffer: *mut c_void,
    buflen: c_ulong,
) -> c_ulong {
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return 0;
    };
    thumbnail::with_thumbnail_data(page.pdf_page().unwrap(), false, |data| unsafe {
        doc::copy_bytes(data, buffer, buflen)
    })
    .unwrap_or(0)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFPage_GetThumbnailAsBitmap(page: *mut c_void) -> *mut c_void {
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return std::ptr::null_mut();
    };
    thumbnail::thumbnail_as_bitmap(page.pdf_page().unwrap()).map_or(
        std::ptr::null_mut(),
        |bitmap| {
            Box::into_raw(Box::new(BitmapHandle {
                storage: Rc::new(RefCell::new(BitmapStorage::Owned(bitmap))),
            }))
            .cast()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdfoxium_core::fpdfapi::parser::{
        cpdf_array::CpdfArray,
        cpdf_object::{retain_object, ObjectRef},
        cpdf_scalar_object::CpdfScalarObject,
        cpdf_stream::CpdfStream,
        cpdf_string::CpdfString,
    };
    fn dict_mut<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
        f(object
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap())
    }
    unsafe fn document() -> *mut c_void {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        Box::into_raw(Box::new(DocumentHandle::new(Rc::new(RefCell::new(
            document,
        )))))
        .cast()
    }
    #[test]
    fn native_catalog_is_tagged() {
        unsafe {
            assert_eq!(FPDFCatalog_IsTagged(std::ptr::null_mut()), 0);
            let owner = Rc::new(RefCell::new(CpdfDocument::new()));
            let document = Box::into_raw(Box::new(DocumentHandle::new(owner.clone()))).cast();
            assert_eq!(FPDFCatalog_IsTagged(document), 0); // No catalog root.
            owner.borrow_mut().create_new_doc();
            let root = owner.borrow().root().unwrap();
            dict_mut(&root, |dict| {
                for key in dict.get_keys() {
                    dict.remove_for(&key);
                }
            });
            assert_eq!(FPDFCatalog_IsTagged(document), 0); // Empty root.
            dict_mut(&root, |dict| {
                dict.set_new_string_for(b"OTHER_KEY", b"other value");
            });
            assert_eq!(FPDFCatalog_IsTagged(document), 0);
            let mark = retain_object(CpdfDictionary::new());
            dict_mut(&root, |dict| {
                dict.set_for(b"MarkInfo", Some(mark.clone()));
            });
            assert_eq!(FPDFCatalog_IsTagged(document), 0);
            dict_mut(&mark, |dict| {
                dict.set_new_for(b"Marked", CpdfScalarObject::number_from_signed(0));
            });
            assert_eq!(FPDFCatalog_IsTagged(document), 0);
            dict_mut(&mark, |dict| {
                dict.set_new_for(b"Marked", CpdfScalarObject::number_from_signed(1));
            });
            assert_eq!(FPDFCatalog_IsTagged(document), 1);
            FPDF_CloseDocument(document);
        }
    }
    #[test]
    fn native_catalog_get_language() {
        unsafe {
            assert_eq!(
                FPDFCatalog_GetLanguage(std::ptr::null_mut(), std::ptr::null_mut(), 0),
                0
            );
            let owner = Rc::new(RefCell::new(CpdfDocument::new()));
            let document = Box::into_raw(Box::new(DocumentHandle::new(owner.clone()))).cast();
            assert_eq!(
                FPDFCatalog_GetLanguage(document, std::ptr::null_mut(), 0),
                0
            );
            owner.borrow_mut().create_new_doc();
            let root = owner.borrow().root().unwrap();
            dict_mut(&root, |dict| {
                for key in dict.get_keys() {
                    dict.remove_for(&key);
                }
            });
            assert_eq!(
                FPDFCatalog_GetLanguage(document, std::ptr::null_mut(), 0),
                2
            );
            let mut empty = [0xffffu16; 1];
            assert_eq!(FPDFCatalog_GetLanguage(document, empty.as_mut_ptr(), 2), 2);
            assert_eq!(empty, [0]);
            dict_mut(&root, |dict| {
                dict.set_new_string_for(b"Lang", b"en-US");
            });
            let size = FPDFCatalog_GetLanguage(document, std::ptr::null_mut(), 0);
            assert_eq!(size, 12);
            let mut buffer = vec![0xffffu16; size as usize / 2];
            assert_eq!(
                FPDFCatalog_GetLanguage(document, buffer.as_mut_ptr(), size),
                size
            );
            assert_eq!(
                buffer,
                "en-US".encode_utf16().chain(Some(0)).collect::<Vec<_>>()
            );
            FPDF_CloseDocument(document);
        }
    }
    #[test]
    fn signature_metadata_aliases_and_buffer_units_follow_native_contracts() {
        unsafe {
            assert_eq!(FPDF_GetSignatureCount(std::ptr::null_mut()), -1);
            assert_eq!(
                FPDFSignatureObj_GetDocMDPPermission(std::ptr::null_mut()),
                0
            );
            let document = document();
            let handle = &*document.cast::<DocumentHandle>();
            let mut value = CpdfDictionary::new();
            value.set_new_for(
                b"Contents",
                CpdfString::from_hex_bytes(None, &[0, 1, 0xfe, 0xff]),
            );
            value.set_new_name_for(b"SubFilter", b"adbe.pkcs7.detached");
            value.set_new_for(
                b"Reason",
                CpdfString::from_unicode(None, &[0x1f58a, 0x4e2d]),
            );
            value.set_new_for(
                b"M",
                CpdfString::from_bytes(None, b"D:20261023123456+02'00'"),
            );
            let mut range = CpdfArray::new();
            for number in [0, 42, 99, 7] {
                range.append_new(CpdfScalarObject::number_from_signed(number));
            }
            value.set_new_for(b"ByteRange", range);
            let params = retain_object(CpdfDictionary::new());
            let mut reference = CpdfDictionary::new();
            reference.set_new_name_for(b"TransformMethod", b"DocMDP");
            reference.set_for(b"TransformParams", Some(params.clone()));
            let mut references = CpdfArray::new();
            references.append_new(CpdfScalarObject::null());
            references.append_new(reference);
            value.set_new_for(b"Reference", references);
            let mut field = CpdfDictionary::new();
            field.set_new_name_for(b"FT", b"Sig");
            field.set_new_for(b"V", value);
            let mut fields = CpdfArray::new();
            fields.append_new(CpdfDictionary::new());
            fields.append_new(field);
            let mut form = CpdfDictionary::new();
            form.set_new_for(b"Fields", fields);
            dict_mut(&handle.document.borrow().root().unwrap(), |root| {
                root.set_new_for(b"AcroForm", form);
            });
            assert_eq!(FPDF_GetSignatureCount(document), 1);
            assert!(FPDF_GetSignatureObject(document, -1).is_null());
            assert!(FPDF_GetSignatureObject(document, 1).is_null());
            let signature = FPDF_GetSignatureObject(document, 0);
            assert_eq!(signature, FPDF_GetSignatureObject(document, 0));
            let mut bytes = [0xaa; 32];
            assert_eq!(
                FPDFSignatureObj_GetContents(signature, bytes.as_mut_ptr().cast(), 3),
                4
            );
            assert_eq!(&bytes[..4], &[0xaa; 4]);
            assert_eq!(
                FPDFSignatureObj_GetContents(signature, bytes.as_mut_ptr().cast(), 4),
                4
            );
            assert_eq!(&bytes[..4], &[0, 1, 0xfe, 0xff]);
            let mut range = [-1; 4];
            assert_eq!(
                FPDFSignatureObj_GetByteRange(signature, range.as_mut_ptr(), 3),
                4
            );
            assert_eq!(range, [-1; 4]);
            assert_eq!(
                FPDFSignatureObj_GetByteRange(signature, range.as_mut_ptr(), 4),
                4
            );
            assert_eq!(range, [0, 42, 99, 7]);
            assert_eq!(
                FPDFSignatureObj_GetSubFilter(signature, bytes.as_mut_ptr().cast(), 32),
                20
            );
            assert_eq!(&bytes[..20], b"adbe.pkcs7.detached\0");
            assert_eq!(
                FPDFSignatureObj_GetReason(signature, bytes.as_mut_ptr().cast(), 32),
                8
            );
            assert_eq!(&bytes[..8], &[0x3d, 0xd8, 0x8a, 0xdd, 0x2d, 0x4e, 0, 0]);
            let time = b"D:20261023123456+02'00'\0";
            assert_eq!(
                FPDFSignatureObj_GetTime(signature, bytes.as_mut_ptr().cast(), 32) as usize,
                time.len()
            );
            assert_eq!(&bytes[..time.len()], time);
            assert_eq!(FPDFSignatureObj_GetDocMDPPermission(signature), 2);
            for (permission, expected) in [(-1, 0), (0, 0), (1, 1), (2, 2), (3, 3), (4, 0)] {
                dict_mut(&params, |dict| {
                    dict.set_new_for(b"P", CpdfScalarObject::number_from_signed(permission));
                });
                assert_eq!(FPDFSignatureObj_GetDocMDPPermission(signature), expected);
            }
            FPDF_CloseDocument(document);
        }
    }
    #[test]
    fn catalog_language_and_thumbnail_pixels_survive_save_reload() {
        unsafe {
            let document = document();
            assert_eq!(FPDFCatalog_IsTagged(document), 0);
            let mut mark = CpdfDictionary::new();
            mark.set_new_for(b"Marked", CpdfScalarObject::boolean(true));
            let root = (&*document.cast::<DocumentHandle>())
                .document
                .borrow()
                .root()
                .unwrap();
            dict_mut(&root, |root| {
                root.set_new_for(b"MarkInfo", mark);
            });
            assert_eq!(FPDFCatalog_IsTagged(document), 1);
            assert_eq!(FPDFCatalog_SetLanguage(document, std::ptr::null()), 0);
            let language: Vec<u16> = "de-DE".encode_utf16().chain(Some(0)).collect();
            assert_eq!(FPDFCatalog_SetLanguage(document, language.as_ptr()), 1);
            let mut language_out = [0xffff; 6];
            assert_eq!(
                FPDFCatalog_GetLanguage(document, language_out.as_mut_ptr(), 10),
                12
            );
            assert_eq!(language_out, [0xffff; 6]);
            assert_eq!(
                FPDFCatalog_GetLanguage(document, language_out.as_mut_ptr(), 12),
                12
            );
            assert_eq!(&language_out, language.as_slice());
            let page = FPDFPage_New(document, 0, 20.0, 20.0);
            let mut dictionary = CpdfDictionary::new();
            dictionary.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
            dictionary.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
            dictionary.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
            dictionary.set_new_name_for(b"ColorSpace", b"DeviceRGB");
            dictionary.set_new_name_for(b"Filter", b"ASCIIHexDecode");
            let raw = b"FF000000FF00>";
            let stream = retain_object(CpdfStream::from_data_and_dict(
                raw.to_vec(),
                retain_object(dictionary),
            ));
            let objects = (&*document.cast::<DocumentHandle>())
                .document
                .borrow()
                .holder();
            objects.borrow_mut().add_indirect_object(stream.clone());
            dict_mut(
                &(&*page.cast::<PageHandle>()).page.as_ref().unwrap().dict(),
                |dict| {
                    dict.set_for(
                        b"Thumb",
                        Some(pdfoxium_core::fpdfapi::parser::cpdf_object::make_reference(
                            &stream, &objects,
                        )),
                    );
                },
            );
            let check = |page| {
                let mut bytes = [0xaa; 16];
                assert_eq!(
                    FPDFPage_GetRawThumbnailData(page, bytes.as_mut_ptr().cast(), 12),
                    13
                );
                assert_eq!(bytes, [0xaa; 16]);
                assert_eq!(
                    FPDFPage_GetRawThumbnailData(page, bytes.as_mut_ptr().cast(), 16),
                    13
                );
                assert_eq!(&bytes[..13], raw);
                assert_eq!(
                    FPDFPage_GetDecodedThumbnailData(page, bytes.as_mut_ptr().cast(), 16),
                    6
                );
                assert_eq!(&bytes[..6], &[255, 0, 0, 0, 255, 0]);
                let bitmap = FPDFPage_GetThumbnailAsBitmap(page);
                assert!(!bitmap.is_null());
                assert_eq!(FPDFBitmap_GetWidth(bitmap), 2);
                assert_eq!(FPDFBitmap_GetHeight(bitmap), 1);
                assert_eq!(
                    std::slice::from_raw_parts(FPDFBitmap_GetBuffer(bitmap).cast::<u8>(), 6),
                    &[0, 0, 255, 0, 255, 0]
                );
                FPDFBitmap_Destroy(bitmap);
            };
            check(page);
            struct Output(Vec<u8>);
            impl WriteStream for Output {
                fn write_block(&mut self, bytes: &[u8]) -> bool {
                    self.0.extend_from_slice(bytes);
                    true
                }
            }
            let mut writer = Output(Vec::new());
            {
                let owner = (&*document.cast::<DocumentHandle>()).document.clone();
                let mut creator = CpdfCreator::new(owner, &mut writer);
                assert!(creator.create(0, 0));
            }
            FPDF_ClosePage(page);
            FPDF_CloseDocument(document);
            let reloaded =
                FPDF_LoadMemDocument64(writer.0.as_ptr().cast(), writer.0.len(), std::ptr::null());
            assert!(!reloaded.is_null());
            assert_eq!(FPDFCatalog_IsTagged(reloaded), 1);
            assert_eq!(
                FPDFCatalog_GetLanguage(reloaded, language_out.as_mut_ptr(), 12),
                12
            );
            assert_eq!(&language_out, language.as_slice());
            let page = FPDF_LoadPage(reloaded, 0);
            check(page);
            FPDF_ClosePage(page);
            FPDF_CloseDocument(reloaded);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn FPDF_VIEWERREF_GetPrintScaling(document: *mut c_void) -> c_int {
    unsafe { document.cast::<DocumentHandle>().as_ref() }.is_none_or(|document| {
        pdfoxium_core::fpdfdoc::cpdf_viewerpreferences::CpdfViewerPreferences::new(
            &document.document.borrow(),
        )
        .print_scaling()
    }) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_VIEWERREF_GetNumCopies(document: *mut c_void) -> c_int {
    unsafe { document.cast::<DocumentHandle>().as_ref() }.map_or(1, |document| {
        pdfoxium_core::fpdfdoc::cpdf_viewerpreferences::CpdfViewerPreferences::new(
            &document.document.borrow(),
        )
        .num_copies()
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_VIEWERREF_GetPrintPageRange(document: *mut c_void) -> *mut c_void {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    pdfoxium_core::fpdfdoc::cpdf_viewerpreferences::CpdfViewerPreferences::new(
        &document.document.borrow(),
    )
    .print_page_range()
    .map_or(std::ptr::null_mut(), |range| {
        doc::borrow_object(&document.borrowed_objects, range)
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_VIEWERREF_GetPrintPageRangeCount(range: *mut c_void) -> usize {
    unsafe { doc::borrowed(range) }.map_or(0, |range| {
        range
            .borrow()
            .as_any()
            .downcast_ref::<pdfoxium_core::fpdfapi::parser::cpdf_array::CpdfArray>()
            .map_or(0, |array| array.size())
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_VIEWERREF_GetPrintPageRangeElement(
    range: *mut c_void,
    index: usize,
) -> c_int {
    unsafe { doc::borrowed(range) }.map_or(-1, |range| {
        range
            .borrow()
            .as_any()
            .downcast_ref::<pdfoxium_core::fpdfapi::parser::cpdf_array::CpdfArray>()
            .filter(|array| index < array.size())
            .map_or(-1, |array| array.get_integer_at(index))
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_VIEWERREF_GetDuplex(document: *mut c_void) -> c_int {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    match pdfoxium_core::fpdfdoc::cpdf_viewerpreferences::CpdfViewerPreferences::new(
        &document.document.borrow(),
    )
    .duplex()
    .as_slice()
    {
        b"Simplex" => 1,
        b"DuplexFlipShortEdge" => 2,
        b"DuplexFlipLongEdge" => 3,
        _ => 0,
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_VIEWERREF_GetName(
    document: *mut c_void,
    key: *const c_char,
    buffer: *mut c_char,
    length: c_ulong,
) -> c_ulong {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    if key.is_null() {
        return 0;
    }
    let name = pdfoxium_core::fpdfdoc::cpdf_viewerpreferences::CpdfViewerPreferences::new(
        &document.document.borrow(),
    )
    .generic_name(unsafe { CStr::from_ptr(key) }.to_bytes());
    name.map_or(0, |text| unsafe {
        doc::copy_terminated(&text, buffer.cast(), length)
    })
}
