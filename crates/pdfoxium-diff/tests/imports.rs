//! Ordinary PDF import interoperability. Both readers render both writers'
//! results, including shared/inherited resources and partial failed imports.
mod common;

use pdfium_render::prelude::*;
use pdfoxium_diff::project_root;
use pdfoxium_diff::FPDF_NO_NATIVETEXT;
use std::ffi::{c_int, c_ulong, c_void};

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
    // SAFETY: descriptor is first in repr(C), and the callback's borrowed bytes
    // are copied while the synchronous writer and its enclosing sink are live.
    let sink = unsafe { &mut *this.cast::<Sink>() };
    if size != 0 {
        sink.bytes.extend_from_slice(unsafe {
            std::slice::from_raw_parts(data.cast::<u8>(), size as usize)
        });
    }
    1
}
#[derive(Clone, Copy)]
enum Selection<'a> {
    Indices(Option<&'a [i32]>),
    Range(&'a str),
}

fn import_save(
    bindings: &dyn PdfiumLibraryBindings,
    target: &[u8],
    source: &[u8],
    selection: Selection<'_>,
    insertion: i32,
) -> (i32, i32, Vec<u8>, i32) {
    // SAFETY: all backing data/strings/arrays outlive their document/operation;
    // source closes before saving to check that imported resources are owned.
    unsafe {
        let dest = bindings.FPDF_LoadMemDocument64(target, None);
        let src = bindings.FPDF_LoadMemDocument64(source, None);
        assert!(!dest.is_null() && !src.is_null());
        let preferences = bindings.FPDF_CopyViewerPreferences(dest, src);
        let result = match selection {
            Selection::Indices(indices) => bindings.FPDF_ImportPagesByIndex(
                dest,
                src,
                indices.map_or(std::ptr::null(), |v| v.as_ptr()),
                indices.map_or(0, |v| v.len() as c_ulong),
                insertion,
            ),
            Selection::Range(range) => bindings.FPDF_ImportPages(dest, src, range, insertion),
        };
        let count = bindings.FPDF_GetPageCount(dest);
        bindings.FPDF_CloseDocument(src);
        let mut sink = Sink {
            interface: FPDF_FILEWRITE {
                version: 1,
                WriteBlock: Some(append),
            },
            bytes: Vec::new(),
        };
        assert_eq!(bindings.FPDF_SaveAsCopy(dest, &mut sink.interface, 0), 1);
        bindings.FPDF_CloseDocument(dest);
        (result, count, sink.bytes, preferences)
    }
}

#[derive(PartialEq, Debug)]
struct Snapshot {
    dimensions: Vec<(f32, f32)>,
    pixels: Vec<Vec<u8>>,
}
fn snapshot(bindings: &dyn PdfiumLibraryBindings, data: &[u8]) -> Snapshot {
    // SAFETY: input data, documents, pages and bitmap buffers close in order.
    unsafe {
        let doc = bindings.FPDF_LoadMemDocument64(data, None);
        assert!(!doc.is_null());
        let mut result = Snapshot {
            dimensions: Vec::new(),
            pixels: Vec::new(),
        };
        for i in 0..bindings.FPDF_GetPageCount(doc) {
            let page = bindings.FPDF_LoadPage(doc, i);
            assert!(!page.is_null());
            result.dimensions.push((
                bindings.FPDF_GetPageWidthF(page),
                bindings.FPDF_GetPageHeightF(page),
            ));
            let bitmap = bindings.FPDFBitmap_Create(200, 250, 1);
            assert!(!bitmap.is_null());
            assert_eq!(
                bindings.FPDFBitmap_FillRect(bitmap, 0, 0, 200, 250, 0xffffffff),
                1
            );
            bindings.FPDF_RenderPageBitmap(bitmap, page, 0, 0, 200, 250, 0, FPDF_NO_NATIVETEXT);
            result
                .pixels
                .push(bindings.FPDFBitmap_GetBuffer_as_vec(bitmap));
            bindings.FPDFBitmap_Destroy(bitmap);
            bindings.FPDF_ClosePage(page);
        }
        bindings.FPDF_CloseDocument(doc);
        result
    }
}

#[test]
fn page_import_selection_inheritance_and_partial_failures_match_native_after_reload() {
    common::with_libraries(|native, rust| {
        let fixture = |name: &str| {
            std::fs::read(
                project_root()
                    .join("upstream/pdfium/testing/resources")
                    .join(name),
            )
            .unwrap()
        };
        let target = fixture("rectangles.pdf");
        let hello = fixture("hello_world.pdf");
        let viewer = fixture("viewer_ref.pdf");
        let cases = [
            (&hello, Selection::Range("1"), 1, 1, 2),
            (&viewer, Selection::Indices(Some(&[4, 0, 4])), 0, 1, 4),
            (&viewer, Selection::Indices(None), 1, 1, 6),
            (&viewer, Selection::Range("2-4"), 1, 1, 4),
            (&hello, Selection::Indices(Some(&[0, 999])), 0, 0, 3),
            (&hello, Selection::Indices(Some(&[])), 0, 0, 1),
            (&hello, Selection::Indices(Some(&[-1, 0])), 0, 0, 1),
            (&hello, Selection::Range("1,"), 0, 0, 1),
            (&hello, Selection::Range("1"), -1, 0, 1),
        ];
        for (i, (source, selection, index, ok, count)) in cases.into_iter().enumerate() {
            let n = import_save(native.as_ref(), &target, source, selection, index);
            let r = import_save(rust.as_ref(), &target, source, selection, index);
            assert_eq!((n.0, n.1), (ok, count), "native case {i}");
            assert_eq!((r.0, r.1), (ok, count), "Rust case {i}");
            assert_eq!(n.3, i32::from(source == &viewer));
            assert_eq!(r.3, n.3, "viewer-preference copy case {i}");
            for (name, reader) in [("native", native.as_ref()), ("rust", rust.as_ref())] {
                let expected = snapshot(reader, &n.2);
                let actual = snapshot(reader, &r.2);
                assert!(
                    actual == expected,
                    "case {i}: import changed {name} reader geometry/rendering"
                );
            }
        }
        nup_and_xobject_interoperability(native.as_ref(), rust.as_ref());
    });
}

fn render_at(
    bindings: &dyn PdfiumLibraryBindings,
    page: FPDF_PAGE,
    width: i32,
    height: i32,
) -> Vec<u8> {
    unsafe {
        let bitmap = bindings.FPDFBitmap_Create(width, height, 1);
        assert!(!bitmap.is_null());
        assert_eq!(
            bindings.FPDFBitmap_FillRect(bitmap, 0, 0, width, height, 0xffffffff),
            1
        );
        bindings.FPDF_RenderPageBitmap(bitmap, page, 0, 0, width, height, 0, FPDF_NO_NATIVETEXT);
        let mut pixels = bindings.FPDFBitmap_GetBuffer_as_vec(bitmap);
        bindings.FPDFBitmap_Destroy(bitmap);
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        pixels
    }
}
fn golden(bindings: &dyn PdfiumLibraryBindings, page: FPDF_PAGE, name: &str) {
    let path = project_root()
        .join("upstream/pdfium/testing/resources/embedder_tests")
        .join(format!("{name}.png"));
    let expected = image::open(path).unwrap().to_rgba8();
    assert_eq!(
        unsafe {
            (
                bindings.FPDF_GetPageWidthF(page) as u32,
                bindings.FPDF_GetPageHeightF(page) as u32,
            )
        },
        expected.dimensions(),
        "golden page dimensions {name}"
    );
    let actual = render_at(
        bindings,
        page,
        expected.width() as i32,
        expected.height() as i32,
    );
    assert!(
        actual == expected.as_raw().as_slice(),
        "golden {name} differs"
    );
}
fn save_existing(bindings: &dyn PdfiumLibraryBindings, doc: FPDF_DOCUMENT) -> Vec<u8> {
    let mut sink = Sink {
        interface: FPDF_FILEWRITE {
            version: 1,
            WriteBlock: Some(append),
        },
        bytes: Vec::new(),
    };
    assert_eq!(
        unsafe { bindings.FPDF_SaveAsCopy(doc, &mut sink.interface, 0) },
        1
    );
    sink.bytes
}
fn nup_save(
    bindings: &dyn PdfiumLibraryBindings,
    data: &[u8],
    width: f32,
    height: f32,
    x: usize,
    y: usize,
    count: i32,
    goldens: &[&str],
) -> Vec<u8> {
    unsafe {
        let src = bindings.FPDF_LoadMemDocument64(data, None);
        assert!(!src.is_null());
        let doc = bindings.FPDF_ImportNPagesToOne(src, width, height, x, y);
        assert!(!doc.is_null());
        bindings.FPDF_CloseDocument(src);
        assert_eq!(bindings.FPDF_GetPageCount(doc), count);
        for (i, name) in goldens.iter().enumerate() {
            let page = bindings.FPDF_LoadPage(doc, i as i32);
            assert!(!page.is_null());
            golden(bindings, page, name);
            bindings.FPDF_ClosePage(page);
        }
        let data = save_existing(bindings, doc);
        bindings.FPDF_CloseDocument(doc);
        data
    }
}
fn xobject_save(bindings: &dyn PdfiumLibraryBindings, data: &[u8], same_doc: bool) -> Vec<u8> {
    unsafe {
        let src = bindings.FPDF_LoadMemDocument64(data, None);
        assert!(!src.is_null());
        let dest = if same_doc {
            src
        } else {
            bindings.FPDF_CreateNewDocument()
        };
        assert!(!dest.is_null());
        let template = bindings.FPDF_NewXObjectFromPage(dest, src, 0);
        assert!(!template.is_null());
        if !same_doc {
            bindings.FPDF_CloseDocument(src);
        }
        for _ in 0..if same_doc { 1 } else { 2 } {
            let page = if same_doc {
                bindings.FPDF_LoadPage(dest, 0)
            } else {
                bindings.FPDFPage_New(dest, 0, 612., 792.)
            };
            assert!(!page.is_null());
            if same_doc {
                golden(bindings, page, "rectangles_agg");
            }
            let form = bindings.FPDF_NewFormObjectFromXObject(template);
            assert!(!form.is_null());
            assert_eq!(bindings.FPDFPageObj_GetType(form), 5);
            if same_doc {
                let m = FS_MATRIX {
                    a: 0.5,
                    b: 0.,
                    c: 0.,
                    d: 0.5,
                    e: 0.,
                    f: 0.,
                };
                assert_eq!(bindings.FPDFPageObj_SetMatrix(form, &m), 1);
            }
            assert_eq!(bindings.FPDFPage_InsertObject(page, form), 1);
            assert_eq!(bindings.FPDFPage_GenerateContent(page), 1);
            golden(
                bindings,
                page,
                if same_doc {
                    "import_page_to_xobject_with_same_doc_agg"
                } else {
                    "import_page_to_xobject_agg"
                },
            );
            if !same_doc {
                let (mut l, mut b, mut r, mut t) = (0., 0., 0., 0.);
                assert_eq!(
                    bindings.FPDFPageObj_GetBounds(form, &mut l, &mut b, &mut r, &mut t),
                    1
                );
                assert_eq!((l, b, r, t), (-1., -1., 201., 301.));
            }
            bindings.FPDF_ClosePage(page);
        }
        let data = save_existing(bindings, dest);
        bindings.FPDF_CloseXObject(template);
        bindings.FPDF_CloseDocument(dest);
        data
    }
}
fn nup_and_xobject_interoperability(
    native: &dyn PdfiumLibraryBindings,
    rust: &dyn PdfiumLibraryBindings,
) {
    let fixture = |name: &str| {
        std::fs::read(
            project_root()
                .join("upstream/pdfium/testing/resources")
                .join(name),
        )
        .unwrap()
    };
    let multi = fixture("rectangles_multi_pages.pdf");
    let bug = fixture("bug_1229106.pdf");
    // Full upstream source-page render loop (intentionally page 0 four times).
    for reader in [native, rust] {
        unsafe {
            let doc = reader.FPDF_LoadMemDocument64(&bug, None);
            assert_eq!(reader.FPDF_GetPageCount(doc), 4);
            for _ in 0..4 {
                let page = reader.FPDF_LoadPage(doc, 0);
                assert!(!page.is_null());
                golden(reader, page, "bug_1229106_rect");
                reader.FPDF_ClosePage(page);
            }
            reader.FPDF_CloseDocument(doc);
        }
    }
    for (i, (data, w, h, x, y, count, images)) in [
        (&multi, 612., 792., 2, 1, 3, &[][..]),
        (&multi, 612., 792., 5, 1, 1, &[][..]),
        (&multi, 792., 612., 8, 1, 1, &[][..]),
        (&multi, 792., 612., 128, 1, 1, &[][..]),
        (
            &multi,
            792.,
            612.,
            3,
            1,
            2,
            &[
                "rectangles_multi_pages_3_in_1_agg",
                "rectangles_multi_pages_2_in_1_agg",
            ][..],
        ),
        (&multi, 1., 2., 1, 1, 5, &[][..]), // 1x1 preserves original page geometry.
        (
            &bug,
            612.,
            792.,
            1,
            2,
            2,
            &["bug_1229106_two_up", "bug_1229106_two_up"][..],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let n = nup_save(native, data, w, h, x, y, count, images);
        let r = nup_save(rust, data, w, h, x, y, count, images);
        for reader in [native, rust] {
            assert_eq!(snapshot(reader, &n), snapshot(reader, &r), "N-up case {i}");
            unsafe {
                let doc = reader.FPDF_LoadMemDocument64(&r, None);
                for (i, name) in images.iter().enumerate() {
                    let page = reader.FPDF_LoadPage(doc, i as i32);
                    golden(reader, page, name);
                    reader.FPDF_ClosePage(page);
                }
                reader.FPDF_CloseDocument(doc);
            }
        }
    }
    let rect = fixture("rectangles.pdf");
    for same in [false, true] {
        let n = xobject_save(native, &rect, same);
        let r = xobject_save(rust, &rect, same);
        for reader in [native, rust] {
            assert_eq!(
                snapshot(reader, &n),
                snapshot(reader, &r),
                "XObject same-document={same}"
            );
            unsafe {
                let doc = reader.FPDF_LoadMemDocument64(&r, None);
                assert_eq!(reader.FPDF_GetPageCount(doc), if same { 1 } else { 2 });
                for i in 0..if same { 1 } else { 2 } {
                    let page = reader.FPDF_LoadPage(doc, i);
                    assert!(!page.is_null());
                    golden(
                        reader,
                        page,
                        if same {
                            "import_page_to_xobject_with_same_doc_agg"
                        } else {
                            "import_page_to_xobject_agg"
                        },
                    );
                    if !same {
                        assert_eq!(reader.FPDFPage_CountObjects(page), 1);
                        let object = reader.FPDFPage_GetObject(page, 0);
                        assert_eq!(reader.FPDFPageObj_GetType(object), 5);
                        assert_eq!(reader.FPDFFormObj_CountObjects(object), 8);
                        let (mut l, mut b, mut r, mut t) = (0., 0., 0., 0.);
                        assert_eq!(
                            reader.FPDFPageObj_GetBounds(object, &mut l, &mut b, &mut r, &mut t),
                            1
                        );
                        assert_eq!((l, b, r, t), (-1., -1., 201., 301.));
                    }
                    reader.FPDF_ClosePage(page);
                }
                reader.FPDF_CloseDocument(doc);
            }
        }
    }
}
