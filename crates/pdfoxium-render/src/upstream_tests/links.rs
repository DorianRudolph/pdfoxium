// Upstream pdfium-render 0.9.4 tests; MIT.
use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;

#[test]
fn test_link_rect() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();

    // The document under test contains a single page with a single link.

    let document = pdfium.load_pdf_from_file("./test/links-test.pdf", None)?;

    const EXPECTED: PdfRect = PdfRect::new_from_values(733.3627, 207.85417, 757.6127, 333.1458);

    // Allow a little bit of error, because it's unreasonable to expect floating point
    // calculations to be identical across builds and platforms.

    const ABS_ERR: PdfPoints = PdfPoints::new(f32::EPSILON * 1000.);

    let actual = document
        .pages()
        .iter()
        .next()
        .unwrap()
        .links()
        .iter()
        .next()
        .unwrap()
        .rect()?;

    assert!((actual.top() - EXPECTED.top()).abs() < ABS_ERR);
    assert!((actual.bottom() - EXPECTED.bottom()).abs() < ABS_ERR);
    assert!((actual.left() - EXPECTED.left()).abs() < ABS_ERR);
    assert!((actual.right() - EXPECTED.right()).abs() < ABS_ERR);

    Ok(())
}
