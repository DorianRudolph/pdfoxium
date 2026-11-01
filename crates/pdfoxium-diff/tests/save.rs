//! Saving interoperability with the bundled native PDFium oracle. Both readers
//! reopen both writers' output; raster comparisons use each reader's own source
//! pixels, so this checks saving without changing rendering tolerances.
mod common;

use pdfium_render::prelude::*;
use pdfoxium_diff::FPDF_NO_NATIVETEXT;
use std::ffi::{c_int, c_ulong, c_void};

#[repr(C)]
struct Sink {
    interface: FPDF_FILEWRITE,
    bytes: Vec<u8>,
}
impl Sink {
    fn new() -> Self {
        Self {
            interface: FPDF_FILEWRITE {
                version: 1,
                WriteBlock: Some(append),
            },
            bytes: Vec::new(),
        }
    }
}
unsafe extern "C" fn append(
    this: *mut FPDF_FILEWRITE,
    data: *const c_void,
    size: c_ulong,
) -> c_int {
    // SAFETY: interface is the first repr(C) field, remains live throughout
    // synchronous Save, and PDFium provides `size` readable callback bytes.
    let sink = unsafe { &mut *this.cast::<Sink>() };
    if size != 0 {
        let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) };
        sink.bytes.extend_from_slice(bytes);
    }
    1
}

fn save(
    bindings: &dyn PdfiumLibraryBindings,
    bytes: &[u8],
    password: Option<&str>,
    flags: u32,
    version: Option<i32>,
) -> Vec<u8> {
    // SAFETY: input bytes and writer outlive the synchronous calls; every
    // checked non-null document is closed after saving completes.
    unsafe {
        let document = bindings.FPDF_LoadMemDocument64(bytes, password);
        assert!(!document.is_null(), "source open failed");
        if flags & 8 != 0 {
            // Incremental saves enumerate cached font dictionaries as new
            // objects. Parse a page first to exercise the actual subset path.
            let page = bindings.FPDF_LoadPage(document, 0);
            assert!(!page.is_null());
            bindings.FPDF_ClosePage(page);
        }
        let mut sink = Sink::new();
        let result = match version {
            Some(version) => {
                bindings.FPDF_SaveWithVersion(document, &mut sink.interface, flags.into(), version)
            }
            None => bindings.FPDF_SaveAsCopy(document, &mut sink.interface, flags.into()),
        };
        bindings.FPDF_CloseDocument(document);
        assert_eq!(result, 1, "save failed: flags={flags}, version={version:?}");
        assert!(!sink.bytes.is_empty());
        sink.bytes
    }
}

fn embedded_font_fixture(
    native: &dyn PdfiumLibraryBindings,
    font_data: &[u8],
    text: &str,
    cid: bool,
) -> Vec<u8> {
    // SAFETY: each writer constructs a fixture through its public font APIs.
    // All handles are checked and close in dependency order.
    unsafe {
        let doc = native.FPDF_CreateNewDocument();
        assert!(!doc.is_null());
        let page = native.FPDFPage_New(doc, 0, 400.0, 400.0);
        assert!(!page.is_null());
        let font = native.FPDFText_LoadFont(
            doc,
            font_data.as_ptr(),
            font_data.len().try_into().unwrap(),
            2, // public/fpdf_edit.h::FPDF_FONT_TRUETYPE (not exported by the bindings).
            i32::from(cid),
        );
        assert!(!font.is_null());
        let object = native.FPDFPageObj_CreateTextObj(doc, font, 30.0);
        assert!(!object.is_null());
        assert_eq!(native.FPDFText_SetText_str(object, text), 1);
        native.FPDFPageObj_Transform(object, 1.0, 0.0, 0.0, 1.0, 50.0, 200.0);
        assert_eq!(native.FPDFPage_InsertObject(page, object), 1);
        assert_eq!(native.FPDFPage_GenerateContent(page), 1);
        let mut sink = Sink::new();
        assert_eq!(native.FPDF_SaveAsCopy(doc, &mut sink.interface, 0), 1);
        native.FPDFFont_Close(font);
        native.FPDF_ClosePage(page);
        native.FPDF_CloseDocument(doc);
        sink.bytes
    }
}

#[derive(Debug, PartialEq)]
struct PageSnapshot {
    count: i32,
    dimensions: (f32, f32),
    pixels: Vec<u8>,
}
fn snapshot(
    bindings: &dyn PdfiumLibraryBindings,
    bytes: &[u8],
    password: Option<&str>,
) -> PageSnapshot {
    // SAFETY: fixture bytes outlive document/page; the bitmap's owned buffer
    // remains valid through the copy, and handles close in dependency order.
    unsafe {
        let document = bindings.FPDF_LoadMemDocument64(bytes, password);
        assert!(!document.is_null(), "saved document cannot be reopened");
        let count = bindings.FPDF_GetPageCount(document);
        let page = bindings.FPDF_LoadPage(document, 0);
        assert!(!page.is_null());
        let dimensions = (
            bindings.FPDF_GetPageWidthF(page),
            bindings.FPDF_GetPageHeightF(page),
        );
        let bitmap = bindings.FPDFBitmap_Create(300, 300, 1);
        assert!(!bitmap.is_null());
        bindings.FPDFBitmap_FillRect(bitmap, 0, 0, 300, 300, 0xffff_ffff);
        bindings.FPDF_RenderPageBitmap(bitmap, page, 0, 0, 300, 300, 0, FPDF_NO_NATIVETEXT);
        let pixels = bindings.FPDFBitmap_GetBuffer_as_vec(bitmap);
        bindings.FPDFBitmap_Destroy(bitmap);
        bindings.FPDF_ClosePage(page);
        bindings.FPDF_CloseDocument(document);
        PageSnapshot {
            count,
            dimensions,
            pixels,
        }
    }
}

#[test]
fn full_incremental_versioned_and_encrypted_saves_reopen_in_both_engines() {
    common::with_libraries(|native, rust| {
        let rectangles = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/rectangles.pdf"
        ));
        let simple = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/hello_world.pdf"
        ));
        let encrypted = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/encrypted_hello_world_r6.pdf"
        ));
        let cases: &[(&[u8], Option<&str>, u32, Option<i32>)] = &[
            (simple, None, 0, None),
            (simple, None, 0, Some(14)),
            (simple, None, 0, Some(18)),
            (rectangles, None, 0, None),
            (rectangles, None, 1, None),
            (rectangles, None, 2, None),
            (rectangles, None, 3, None),
            (rectangles, None, 4, None),
            (rectangles, None, 8, None),
            (rectangles, None, 16, None),
            (encrypted, Some("âge"), 0, None),
            (encrypted, Some("âge"), 1, None),
            (encrypted, Some("âge"), 4, None),
        ];
        for (index, &(input, password, flags, version)) in cases.iter().enumerate() {
            let native_output = save(native.as_ref(), input, password, flags, version);
            let rust_output = save(rust.as_ref(), input, password, flags, version);
            if flags == 1 {
                assert!(native_output.starts_with(input));
                assert!(rust_output.starts_with(input));
            }
            if version == Some(14) {
                assert!(native_output.starts_with(b"%PDF-1.4\r\n"));
                assert!(rust_output.starts_with(b"%PDF-1.4\r\n"));
            }
            let reopened_password = if flags == 4 { None } else { password };
            for (reader_name, reader) in [("native", native.as_ref()), ("rust", rust.as_ref())] {
                let expected = snapshot(reader, input, password);
                for (writer_name, output) in [("native", &native_output), ("rust", &rust_output)] {
                    let actual = snapshot(reader, output, reopened_password);
                    assert!(actual == expected, "case {index}: {writer_name} writer -> {reader_name} reader changed page geometry/pixels");
                }
            }
        }
        let roboto = include_bytes!("../../pdfoxium-core/testdata/harfbuzz/Roboto-Regular.ttf");
        let cff = include_bytes!(
            "../../../upstream/pdfium/third_party/NotoSansCJK/NotoSansSC-Regular.subset.otf"
        );
        for (data, text, cid) in [
            (roboto.as_slice(), "zzz", false),
            (roboto.as_slice(), "zzz", true),
            (cff.as_slice(), "这这", true),
        ] {
            let reference = embedded_font_fixture(native.as_ref(), data, text, cid);
            for (writer_name, writer) in [("native", native.as_ref()), ("rust", rust.as_ref())] {
                let input = if writer_name == "native" {
                    reference.clone()
                } else {
                    embedded_font_fixture(writer, data, text, cid)
                };
                let output = save(rust.as_ref(), &input, None, 1 | 8, None);
                assert!(output.starts_with(&input));
                for (name, reader) in [("native", native.as_ref()), ("rust", rust.as_ref())] {
                    let expected = snapshot(reader, &reference, None);
                    assert!(expected.pixels.iter().any(|&byte| byte != 255));
                    assert!(
                        snapshot(reader, &input, None) == expected,
                        "{writer_name} font embedding changed {name} rendering: cid={cid}, text={text}"
                    );
                    assert!(snapshot(reader, &output, None) == expected,
                        "{writer_name} font subset/fallback changed {name} rendering: cid={cid}, text={text}");
                }
            }
        }
    });
}
