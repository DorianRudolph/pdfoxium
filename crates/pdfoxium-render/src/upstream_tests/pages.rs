// Tests from pdfium-render 0.9.4; MIT. See LICENSE.pdfium-render.
use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;

#[test]
fn test_page_size() -> Result<(), PdfiumError> {
    // Tests the dimensions of each page in a sample file.

    let pdfium = test_bind_to_pdfium();

    let document = pdfium.load_pdf_from_file("./test/page-sizes-test.pdf", None)?;

    assert_eq!(document.pages().page_size(0)?, expected_page_0_size());
    assert_eq!(document.pages().page_size(1)?, expected_page_1_size());
    assert_eq!(document.pages().page_size(2)?, expected_page_2_size());
    assert_eq!(document.pages().page_size(3)?, expected_page_3_size());
    assert_eq!(document.pages().page_size(4)?, expected_page_4_size());
    assert!(document.pages().page_size(5).is_err());

    Ok(())
}

#[test]
fn test_page_sizes() -> Result<(), PdfiumError> {
    // Tests the dimensions of all pages in a sample file.

    let pdfium = test_bind_to_pdfium();

    let document = pdfium.load_pdf_from_file("./test/page-sizes-test.pdf", None)?;

    assert_eq!(
        document.pages().page_sizes()?,
        vec!(
            expected_page_0_size(),
            expected_page_1_size(),
            expected_page_2_size(),
            expected_page_3_size(),
            expected_page_4_size(),
        ),
    );

    Ok(())
}

const fn expected_page_0_size() -> PdfRect {
    PdfRect::new_from_values(0.0, 0.0, 841.8898, 595.30396)
}

const fn expected_page_1_size() -> PdfRect {
    PdfRect::new_from_values(0.0, 0.0, 595.30396, 841.8898)
}

const fn expected_page_2_size() -> PdfRect {
    PdfRect::new_from_values(0.0, 0.0, 1190.5511, 841.8898)
}

const fn expected_page_3_size() -> PdfRect {
    PdfRect::new_from_values(0.0, 0.0, 419.5559, 595.30396)
}

const fn expected_page_4_size() -> PdfRect {
    expected_page_0_size()
}

#[test]
fn copy_page_range_from_document() -> Result<(), PdfiumError> {
    // Tests that copy_page_range_from_document() copies the expected
    // number of pages.

    let pdfium = test_bind_to_pdfium();

    let max_page_count = 200;

    for i in 0..(max_page_count / 2) {
        let mut source = pdfium.create_new_pdf()?;

        for _ in 0..max_page_count {
            source
                .pages_mut()
                .create_page_at_end(PdfPagePaperSize::a4())?;
        }

        let mut destination = pdfium.create_new_pdf()?;

        for _ in 0..i {
            destination
                .pages_mut()
                .create_page_at_end(PdfPagePaperSize::a4())?;
        }

        let destination_page_index = destination.pages().len() / 2;

        let source_from_page_index = source.pages().len() / 2 - i;
        let source_to_page_index = source.pages().len() / 2 + i;
        let source_page_range_len = source_to_page_index - source_from_page_index + 1; // Page ranges are inclusive

        destination.pages_mut().copy_page_range_from_document(
            &source,
            source_from_page_index..=source_to_page_index,
            destination_page_index,
        )?;

        assert_eq!(destination.pages().len(), i + source_page_range_len);
    }

    Ok(())
}
