//! Connected path/image/mark editing and ownership, then save/reload through
//! both writers and both readers. Font embedding is covered by save.rs.
mod common;

use pdfium_render::prelude::*;
use pdfoxium_diff::project_root;
use pdfoxium_diff::FPDF_NO_NATIVETEXT;
use std::ffi::{c_int, c_ulong, c_void};

#[repr(C)]
struct Sink {
    interface: FPDF_FILEWRITE,
    data: Vec<u8>,
}
unsafe extern "C" fn append(
    this: *mut FPDF_FILEWRITE,
    bytes: *const c_void,
    size: c_ulong,
) -> c_int {
    let sink = unsafe { &mut *this.cast::<Sink>() };
    if size != 0 {
        sink.data.extend_from_slice(unsafe {
            std::slice::from_raw_parts(bytes.cast::<u8>(), size as usize)
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
        data: Vec::new(),
    };
    unsafe {
        assert_eq!(b.FPDF_SaveAsCopy(doc, &mut sink.interface, 0), 1);
    }
    sink.data
}
fn render(b: &dyn PdfiumLibraryBindings, page: FPDF_PAGE) -> Vec<u8> {
    unsafe {
        let bitmap = b.FPDFBitmap_Create(200, 200, 1);
        assert!(!bitmap.is_null());
        assert_eq!(b.FPDFBitmap_FillRect(bitmap, 0, 0, 200, 200, 0xffffffff), 1);
        b.FPDF_RenderPageBitmap(bitmap, page, 0, 0, 200, 200, 0, FPDF_NO_NATIVETEXT);
        let result = b.FPDFBitmap_GetBuffer_as_vec(bitmap);
        b.FPDFBitmap_Destroy(bitmap);
        result
    }
}
#[allow(deprecated)] // Exercise the deprecated image-matrix API that this batch ports.
fn construct(b: &dyn PdfiumLibraryBindings) -> Vec<u8> {
    unsafe {
        let doc = b.FPDF_CreateNewDocument();
        let page = b.FPDFPage_New(doc, 0, 200., 200.);
        assert!(!page.is_null());
        let path = b.FPDFPageObj_CreateNewPath(10., 10.);
        assert_eq!(b.FPDFPath_LineTo(path, 70., 10.), 1);
        assert_eq!(b.FPDFPath_BezierTo(path, 95., 20., 80., 80., 40., 90.), 1);
        assert_eq!(b.FPDFPath_Close(path), 1);
        assert_eq!(b.FPDFPath_SetDrawMode(path, 2, 1), 1);
        assert_eq!(b.FPDFPageObj_SetFillColor(path, 255, 0, 0, 255), 1);
        assert_eq!(b.FPDFPageObj_SetStrokeColor(path, 0, 0, 255, 255), 1);
        assert_eq!(b.FPDFPageObj_SetStrokeWidth(path, 3.), 1);
        let dashes = [6., 2.];
        assert_eq!(
            b.FPDFPageObj_SetDashArray(path, dashes.as_ptr(), dashes.len(), 1.),
            1
        );
        let mark = b.FPDFPageObj_AddMark(path, "Artifact");
        assert!(!mark.is_null());
        assert_eq!(
            b.FPDFPageObjMark_SetIntParam(doc, path, mark, "MCID", 42),
            1
        );
        assert_eq!(
            b.FPDFPageObjMark_SetStringParam(doc, path, mark, "Label", "Curve"),
            1
        );
        assert_eq!(b.FPDFPage_InsertObject(page, path), 1);
        let rectangle = b.FPDFPageObj_CreateNewRect(100., 100., 70., 60.);
        assert_eq!(b.FPDFPageObj_SetFillColor(rectangle, 0, 190, 40, 255), 1);
        assert_eq!(b.FPDFPath_SetDrawMode(rectangle, 2, 0), 1);
        assert_eq!(b.FPDFPage_InsertObjectAtIndex(page, rectangle, 0), 1);
        assert_eq!(b.FPDFPage_RemoveObject(page, rectangle), 1);
        assert_eq!(b.FPDFPageObj_GetType(rectangle), 2);
        assert_eq!(b.FPDFPage_InsertObject(page, rectangle), 1);
        let bitmap = b.FPDFBitmap_Create(4, 3, 1);
        assert_eq!(b.FPDFBitmap_FillRect(bitmap, 0, 0, 4, 3, 0xffeedd00), 1);
        assert_eq!(b.FPDFBitmap_FillRect(bitmap, 1, 1, 2, 1, 0xff003388), 1);
        let image = b.FPDFPageObj_NewImageObj(doc);
        assert!(!image.is_null());
        let mut pages = [page];
        assert_eq!(
            b.FPDFImageObj_SetBitmap(pages.as_mut_ptr(), 1, image, bitmap),
            1
        );
        b.FPDFBitmap_Destroy(bitmap);
        assert_eq!(
            b.FPDFImageObj_SetMatrix(image, 40., 0., 0., 30., 110., 30.),
            1
        );
        assert_eq!(b.FPDFPage_InsertObject(page, image), 1);
        assert_eq!(b.FPDFPage_GenerateContent(page), 1);
        let output = save(b, doc);
        b.FPDF_ClosePage(page);
        b.FPDF_CloseDocument(doc);
        output
    }
}
fn snapshot(b: &dyn PdfiumLibraryBindings, bytes: &[u8]) -> Vec<u8> {
    unsafe {
        let doc = b.FPDF_LoadMemDocument64(bytes, None);
        assert!(!doc.is_null());
        let page = b.FPDF_LoadPage(doc, 0);
        assert!(!page.is_null());
        assert_eq!(b.FPDFPage_CountObjects(page), 3);
        let curve = b.FPDFPage_GetObject(page, 0);
        assert_eq!(b.FPDFPageObj_CountMarks(curve), 1);
        assert_eq!(b.FPDFPageObj_GetMarkedContentID(curve), 42);
        let mark = b.FPDFPageObj_GetMark(curve, 0);
        let mut id = 0;
        assert_eq!(b.FPDFPageObjMark_GetParamIntValue(mark, "MCID", &mut id), 1);
        assert_eq!(id, 42);
        let pixels = render(b, page);
        assert!(pixels.iter().any(|&v| v != 255));
        b.FPDF_ClosePage(page);
        b.FPDF_CloseDocument(doc);
        pixels
    }
}
fn move_pages(b: &dyn PdfiumLibraryBindings) {
    let fixture = std::fs::read(
        project_root().join("upstream/pdfium/testing/resources/rectangles_multi_pages.pdf"),
    )
    .unwrap();
    // Source: FPDFEditMoveEmbedderTest.MovePagesTest. The oversized-array case
    // uses a real sixth element instead of the source's out-of-bounds span.
    let cases: &[(&[i32], i32, bool, &[usize])] = &[
        (&[0, 1, 2, 3, 4], 0, true, &[0, 1, 2, 3, 4]),
        (&[0, 4, 2, 1, 3], 0, true, &[0, 4, 2, 1, 3]),
        (&[0, 2, 4, 3], 1, true, &[1, 0, 2, 4, 3]),
        (&[1, 4, 2], 2, true, &[0, 3, 1, 4, 2]),
        (&[3, 2], 3, true, &[0, 1, 4, 3, 2]),
        (&[3], 4, true, &[0, 1, 2, 4, 3]),
        (&[1, 1], 2, false, &[]),
        (&[5, 3, 2], 0, false, &[]),
        (&[], 0, false, &[]),
        (&[4, 3, 2, 1, 0, 0], 0, false, &[]),
        (&[], 5, false, &[]),
        (&[], -1, false, &[]),
    ];
    for (indices, destination, success, order) in cases {
        unsafe {
            let doc = b.FPDF_LoadMemDocument64(&fixture, None);
            assert!(!doc.is_null());
            let original: Vec<_> = (0..5)
                .map(|i| {
                    let p = b.FPDF_LoadPage(doc, i);
                    let pixels = render(b, p);
                    b.FPDF_ClosePage(p);
                    pixels
                })
                .collect();
            assert_eq!(
                b.FPDF_MovePages(
                    doc,
                    indices.as_ptr(),
                    indices.len() as c_ulong,
                    *destination
                ),
                i32::from(*success)
            );
            let saved = save(b, doc);
            b.FPDF_CloseDocument(doc);
            let doc = b.FPDF_LoadMemDocument64(&saved, None);
            assert_eq!(b.FPDF_GetPageCount(doc), 5);
            for i in 0..5 {
                let page = b.FPDF_LoadPage(doc, i);
                assert!(
                    render(b, page) == original[if *success { order[i as usize] } else { i as usize }],
                    "page move changed raster order: indices={indices:?}, destination={destination}, page={i}"
                );
                b.FPDF_ClosePage(page);
            }
            b.FPDF_CloseDocument(doc);
        }
    }
}
#[test]
fn object_editing_and_page_moves_save_reload_in_both_engines() {
    common::with_libraries(|native, rust| {
        let n = construct(native.as_ref());
        let r = construct(rust.as_ref());
        for reader in [native.as_ref(), rust.as_ref()] {
            assert!(
                snapshot(reader, &n) == snapshot(reader, &r),
                "edited paths/marks/images changed rendering"
            );
            move_pages(reader);
        }
    });
}
