// Tests from pdfium-render 0.9.4; MIT. See LICENSE.pdfium-render.
use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;

#[test]
fn test_apply_matrix() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();

    let mut document = pdfium.create_new_pdf()?;

    let mut page = document
        .pages_mut()
        .create_page_at_start(PdfPagePaperSize::a4())?;

    let font = document.fonts_mut().times_roman();

    let mut object = page.objects_mut().create_text_object(
        PdfPoints::ZERO,
        PdfPoints::ZERO,
        "My new text object",
        font,
        PdfPoints::new(10.0),
    )?;

    // Apply some basic transformations to the object...

    object.translate(PdfPoints::new(100.0), PdfPoints::new(100.0))?;
    object.flip_vertically()?;
    object.rotate_clockwise_degrees(45.0)?;
    object.scale(3.0, 4.0)?;

    let previous_matrix = object.matrix()?;

    // _Applying_ the identity matrix should not alter the current matrix.

    object.apply_matrix(PdfMatrix::IDENTITY)?;

    assert_eq!(previous_matrix, object.matrix()?);

    Ok(())
}

#[test]
fn test_reset_matrix_to_identity() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();

    let mut document = pdfium.create_new_pdf()?;

    let mut page = document
        .pages_mut()
        .create_page_at_start(PdfPagePaperSize::a4())?;

    let font = document.fonts_mut().times_roman();

    let mut object = page.objects_mut().create_text_object(
        PdfPoints::ZERO,
        PdfPoints::ZERO,
        "My new text object",
        font,
        PdfPoints::new(10.0),
    )?;

    // Apply some basic transformations to the object...

    object.translate(PdfPoints::new(100.0), PdfPoints::new(100.0))?;
    object.flip_vertically()?;
    object.rotate_clockwise_degrees(45.0)?;
    object.scale(3.0, 4.0)?;

    let previous_matrix = object.matrix()?;

    // _Resetting_ the object's matrix back to the identity matrix should wipe out
    // the current matrix.

    object.reset_matrix_to_identity()?;

    assert_ne!(previous_matrix, object.matrix()?);
    assert_eq!(object.matrix()?, PdfMatrix::IDENTITY);

    Ok(())
}

#[test]
fn test_transform_captured_in_content_regeneration() -> Result<(), PdfiumError> {
    // The purpose of the test is to confirm that object transformations are correctly
    // applied to the page's content streams by automatic content regeneration.
    // In pdfium-render versions 0.8.27 and earlier, this was not reliably the case.
    // See: https://github.com/ajrcarey/pdfium-render/issues/168

    let pdfium = test_bind_to_pdfium();

    let mut document = pdfium.create_new_pdf()?;

    let x = PdfPoints::new(100.0);
    let y = PdfPoints::new(400.0);

    let object_matrix_before_rotation = {
        let mut page = document
            .pages_mut()
            .create_page_at_start(PdfPagePaperSize::a4())?;

        let font = document
            .fonts_mut()
            .new_built_in(PdfFontBuiltin::TimesRoman);

        let mut object = page.objects_mut().create_text_object(
            x,
            y,
            "Hello world!",
            font,
            PdfPoints::new(20.0),
        )?;

        let object_matrix_before_rotation = object.matrix()?;

        // In pdfium-render versions 0.8.27 and earlier, the following transformation
        // will not be captured by automatic content regeneration. It will be lost
        // when the page falls out of scope.
        object.rotate_clockwise_degrees(45.0)?;

        object_matrix_before_rotation
    };

    // The page has now been dropped since it has fallen out of scope. Re-open it,
    // retrieve the page object, and test its transformation matrix. It should be
    // correctly rotated, indicating the transformation was correctly captured by
    // automatic content regeneration.

    assert_eq!(
        object_matrix_before_rotation.rotate_clockwise_degrees(45.0)?,
        document.pages().first()?.objects().first()?.matrix()?
    );

    Ok(())
}
