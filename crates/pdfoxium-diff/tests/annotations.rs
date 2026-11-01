//! Annotation appearance/object edits, embedded files and flattening through
//! both public libraries. Compare each reader's rendering across both writers.
mod common;

use pdfium_render::prelude::*;
use pdfoxium_diff::FPDF_NO_NATIVETEXT;
use std::ffi::{c_int, c_ulong, c_void};
const PAYLOAD: &[u8] = b"Rust annotation attachment\0binary\xff";
#[repr(C)]
struct Sink {
    interface: FPDF_FILEWRITE,
    bytes: Vec<u8>,
}
unsafe extern "C" fn append(
    this: *mut FPDF_FILEWRITE,
    data: *const c_void,
    size: c_ulong,
) -> c_int {
    let sink = unsafe { &mut *this.cast::<Sink>() };
    if size != 0 {
        sink.bytes.extend_from_slice(unsafe {
            std::slice::from_raw_parts(data.cast::<u8>(), size as usize)
        });
    }
    1
}
fn save(b: &dyn PdfiumLibraryBindings, doc: FPDF_DOCUMENT) -> Vec<u8> {
    let mut sink = Sink {
        interface: FPDF_FILEWRITE {
            version: 1,
            WriteBlock: Some(append),
        },
        bytes: Vec::new(),
    };
    unsafe {
        assert_eq!(b.FPDF_SaveAsCopy(doc, &mut sink.interface, 0), 1);
    }
    sink.bytes
}
fn attachment_bytes(b: &dyn PdfiumLibraryBindings, attachment: FPDF_ATTACHMENT) -> Vec<u8> {
    assert!(!attachment.is_null());
    unsafe {
        let mut length = 0;
        assert_eq!(
            b.FPDFAttachment_GetFile(attachment, std::ptr::null_mut(), 0, &mut length),
            1
        );
        let mut bytes = vec![0; length as usize];
        assert_eq!(
            b.FPDFAttachment_GetFile(attachment, bytes.as_mut_ptr().cast(), length, &mut length),
            1
        );
        bytes
    }
}
fn construct(b: &dyn PdfiumLibraryBindings) -> Vec<u8> {
    unsafe {
        let doc = b.FPDF_CreateNewDocument();
        assert!(!doc.is_null());
        let page = b.FPDFPage_New(doc, 0, 100., 100.);
        let annot = b.FPDFPage_CreateAnnot(page, 13); // Stamp
        assert!(!annot.is_null());
        assert_eq!(
            b.FPDFAnnot_SetRect(
                annot,
                &FS_RECTF {
                    left: 20.,
                    top: 80.,
                    right: 80.,
                    bottom: 20.
                }
            ),
            1
        );
        assert_eq!(b.FPDFAnnot_SetFlags(annot, 4), 1);
        assert_eq!(
            b.FPDFAnnot_SetStringValue_str(annot, "Contents", "Editable stamp"),
            1
        );
        assert_eq!(
            b.FPDFAnnot_SetAP_str(annot, 0, "1 0 0 rg 20 20 60 60 re f"),
            1
        );
        assert_eq!(b.FPDFAnnot_GetObjectCount(annot), 1);
        let object = b.FPDFAnnot_GetObject(annot, 0);
        assert!(!object.is_null());
        assert_eq!(b.FPDFPageObj_SetFillColor(object, 0, 180, 40, 255), 1);
        assert_eq!(b.FPDFAnnot_UpdateObject(annot, object), 1);
        let corner = b.FPDFPageObj_CreateNewRect(20., 20., 10., 10.);
        assert_eq!(b.FPDFPath_SetDrawMode(corner, 2, 0), 1);
        assert_eq!(b.FPDFPageObj_SetFillColor(corner, 0, 0, 255, 255), 1);
        assert_eq!(b.FPDFAnnot_AppendObject(annot, corner), 1);
        assert_eq!(b.FPDFAnnot_GetObjectCount(annot), 2);
        assert_eq!(b.FPDFPage_GetAnnotIndex(page, annot), 0);
        b.FPDFPage_CloseAnnot(annot);

        let file_annot = b.FPDFPage_CreateAnnot(page, 17); // FileAttachment
        assert!(!file_annot.is_null());
        assert_eq!(
            b.FPDFAnnot_SetRect(
                file_annot,
                &FS_RECTF {
                    left: 2.,
                    top: 12.,
                    right: 12.,
                    bottom: 2.
                }
            ),
            1
        );
        assert_eq!(b.FPDFAnnot_SetFlags(file_annot, 2), 1); // Hidden: data only.
        let name: Vec<u16> = "note.bin".encode_utf16().chain([0]).collect();
        let attachment = b.FPDFAnnot_AddFileAttachment(file_annot, name.as_ptr());
        assert!(!attachment.is_null());
        assert_eq!(
            b.FPDFAttachment_SetFile(
                attachment,
                doc,
                PAYLOAD.as_ptr().cast(),
                PAYLOAD.len() as c_ulong
            ),
            1
        );
        assert_eq!(
            attachment_bytes(b, b.FPDFAnnot_GetFileAttachment(file_annot)),
            PAYLOAD
        );
        b.FPDFPage_CloseAnnot(file_annot);
        // The borrowed file object belongs to the document, surviving closure
        // of the annotation handle that exposed it.
        assert_eq!(attachment_bytes(b, attachment), PAYLOAD);
        let global = b.FPDFDoc_AddAttachment_str(doc, "document.bin");
        assert!(!global.is_null());
        assert_eq!(
            b.FPDFAttachment_SetFile(
                global,
                doc,
                PAYLOAD.as_ptr().cast(),
                PAYLOAD.len() as c_ulong
            ),
            1
        );
        assert_eq!(
            b.FPDFAttachment_SetStringValue_str(global, "Description", "Portable bytes"),
            1
        );
        assert_eq!(b.FPDFDoc_GetAttachmentCount(doc), 1);
        let bytes = save(b, doc);
        b.FPDF_ClosePage(page);
        b.FPDF_CloseDocument(doc);
        bytes
    }
}
fn snapshot(b: &dyn PdfiumLibraryBindings, bytes: &[u8], flattened: bool) -> Vec<u8> {
    unsafe {
        let doc = b.FPDF_LoadMemDocument64(bytes, None);
        assert!(!doc.is_null());
        assert_eq!(b.FPDF_GetFormType(doc), 0);
        assert_eq!(b.FPDFDoc_GetAttachmentCount(doc), 1);
        assert_eq!(
            attachment_bytes(b, b.FPDFDoc_GetAttachment(doc, 0)),
            PAYLOAD
        );
        let page = b.FPDF_LoadPage(doc, 0);
        assert!(!page.is_null());
        assert_eq!(
            b.FPDFPage_GetAnnotCount(page),
            if flattened { 0 } else { 2 }
        );
        if !flattened {
            let stamp = b.FPDFPage_GetAnnot(page, 0);
            assert_eq!(b.FPDFAnnot_GetSubtype(stamp), 13);
            assert_eq!(b.FPDFAnnot_GetFlags(stamp), 4);
            assert_eq!(b.FPDFAnnot_GetObjectCount(stamp), 2);
            b.FPDFPage_CloseAnnot(stamp);
            let file = b.FPDFPage_GetAnnot(page, 1);
            assert_eq!(
                attachment_bytes(b, b.FPDFAnnot_GetFileAttachment(file)),
                PAYLOAD
            );
            b.FPDFPage_CloseAnnot(file);
        }
        let bitmap = b.FPDFBitmap_Create(100, 100, 1);
        assert!(!bitmap.is_null());
        assert_eq!(b.FPDFBitmap_FillRect(bitmap, 0, 0, 100, 100, 0xffffffff), 1);
        b.FPDF_RenderPageBitmap(
            bitmap,
            page,
            0,
            0,
            100,
            100,
            0,
            (if flattened { 0 } else { 1 }) | FPDF_NO_NATIVETEXT,
        );
        let pixels = b.FPDFBitmap_GetBuffer_as_vec(bitmap);
        assert!(pixels.chunks_exact(4).any(|p| p == [40, 180, 0, 255]));
        b.FPDFBitmap_Destroy(bitmap);
        b.FPDF_ClosePage(page);
        b.FPDF_CloseDocument(doc);
        pixels
    }
}
fn flatten(b: &dyn PdfiumLibraryBindings, bytes: &[u8]) -> Vec<u8> {
    unsafe {
        let doc = b.FPDF_LoadMemDocument64(bytes, None);
        let page = b.FPDF_LoadPage(doc, 0);
        assert_eq!(b.FPDFPage_Flatten(page, 0), 1);
        assert_eq!(b.FPDFPage_GetAnnotCount(page), 0);
        let output = save(b, doc);
        b.FPDF_ClosePage(page);
        b.FPDF_CloseDocument(doc);
        output
    }
}
#[test]
fn annotations_objects_files_and_flattened_output_survive_both_readers() {
    common::with_libraries(|native, rust| {
        let n = construct(native.as_ref());
        let r = construct(rust.as_ref());
        for reader in [native.as_ref(), rust.as_ref()] {
            assert!(
                snapshot(reader, &n, false) == snapshot(reader, &r, false),
                "annotation writers changed rendered appearance"
            );
        }
        for writer in [native.as_ref(), rust.as_ref()] {
            for original in [&n, &r] {
                let flat = flatten(writer, original);
                for reader in [native.as_ref(), rust.as_ref()] {
                    assert!(
                        snapshot(reader, original, false) == snapshot(reader, &flat, true),
                        "flattening changed {original_len} source bytes' appearance",
                        original_len = original.len()
                    );
                }
            }
        }
    });
}
