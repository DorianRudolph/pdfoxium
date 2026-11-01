mod common;

#[test]
fn null_document_file_version_matches_native_pdfium() {
    common::with_libraries(|native, rust| {
        let run = |bindings: &dyn pdfium_render::prelude::PdfiumLibraryBindings| {
            let mut version = -1;
            // SAFETY: PDFium explicitly accepts a null document. `version` is a
            // writable int for the duration of the call.
            let result =
                unsafe { bindings.FPDF_GetFileVersion(std::ptr::null_mut(), &mut version) };
            (result, version)
        };

        let expected = run(native.as_ref());
        assert_eq!(expected, (0, 0));
        assert_eq!(run(rust.as_ref()), expected);
    });
}

#[test]
fn clean_page_content_generation_matches_native_pdfium() {
    const PDF: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/hello_world_2_pages.pdf"
    ));
    common::with_libraries(|native, rust| {
        let run = |bindings: &dyn pdfium_render::prelude::PdfiumLibraryBindings| {
            // SAFETY: the fixture bytes outlive the document, all non-null handles
            // are checked and closed in page-before-document order. PDFium permits
            // the null page passed to GenerateContent.
            let result = unsafe {
                let null_result = bindings.FPDFPage_GenerateContent(std::ptr::null_mut());
                let document = bindings.FPDF_LoadMemDocument64(PDF, None);
                assert!(!document.is_null());
                let page = bindings.FPDF_LoadPage(document, 0);
                assert!(!page.is_null());
                let first = bindings.FPDFPage_GenerateContent(page);
                let second = bindings.FPDFPage_GenerateContent(page);
                let geometry = (
                    bindings.FPDF_GetPageWidthF(page),
                    bindings.FPDF_GetPageHeightF(page),
                );
                bindings.FPDF_ClosePage(page);
                bindings.FPDF_CloseDocument(document);
                (null_result, first, second, geometry)
            };
            result
        };
        let expected = run(native.as_ref());
        assert_eq!(expected, (0, 1, 1, (200.0, 200.0)));
        assert_eq!(run(rust.as_ref()), expected);
    });
}
