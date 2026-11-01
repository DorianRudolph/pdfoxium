// Upstream pdfium-render 0.9.4 tests; MIT.
use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;

#[test]
fn test_modification_date() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();

    let document = pdfium.load_pdf_from_file("./test/dimensions-test.pdf", None)?;

    let modification_date = document
        .metadata()
        .get(PdfDocumentMetadataTagType::ModificationDate)
        .map(|tag| tag.value().to_owned());

    assert_eq!(
        modification_date.as_deref(),
        Some("D:20230524100131+02'00'")
    );

    Ok(())
}
