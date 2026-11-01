// Tests from pdfium-render 0.9.4; MIT. See LICENSE.pdfium-render.
use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;
use image::{GenericImageView, ImageFormat};

#[test]
fn test_page_rendering_reusing_bitmap() -> Result<(), PdfiumError> {
    // Renders each page in the given test PDF file to a separate JPEG file
    // by re-using the same bitmap buffer for each render.

    let pdfium = test_bind_to_pdfium();

    let document = pdfium.load_pdf_from_file("./test/export-test.pdf", None)?;

    let render_config = PdfRenderConfig::new()
        .set_target_width(2000)
        .set_maximum_height(2000)
        .rotate_if_landscape(PdfPageRenderRotation::Degrees90, true);

    let mut bitmap = PdfBitmap::empty(2500, 2500, PdfBitmapFormat::default())?;

    for (index, page) in document.pages().iter().enumerate() {
        page.render_into_bitmap_with_config(&mut bitmap, &render_config)?; // Re-uses the same bitmap for rendering each page.

        bitmap
            .as_image()?
            .into_rgb8()
            .save_with_format(
                format!(
                    "{}/../../target/pdfium-render-page-page-{}.jpg",
                    env!("CARGO_MANIFEST_DIR"),
                    index
                ),
                ImageFormat::Jpeg,
            )
            .map_err(|_| PdfiumError::ImageError)?;
    }

    Ok(())
}

#[test]
fn test_rendered_image_dimension() -> Result<(), PdfiumError> {
    // Checks that downscaled dimensions are rounded correctly during page rendering.
    // See: https://github.com/ajrcarey/pdfium-render/pull/87

    let pdfium = test_bind_to_pdfium();

    let document = pdfium.load_pdf_from_file("./test/dimensions-test.pdf", None)?;

    let render_config = PdfRenderConfig::new()
        .set_target_width(500)
        .set_maximum_height(500);

    for (_index, page) in document.pages().iter().enumerate() {
        let rendered_page = page.render_with_config(&render_config)?.as_image()?;

        let (width, _height) = rendered_page.dimensions();

        assert_eq!(width, 500);
    }

    Ok(())
}
