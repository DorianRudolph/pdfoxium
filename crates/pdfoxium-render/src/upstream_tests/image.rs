// Upstream pdfium-render 0.9.4 tests; MIT.
use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;
use image::DynamicImage;

#[test]
fn test_page_image_object_retains_format() -> Result<(), PdfiumError> {
    // Make sure the format of the image we pass into a new PdfPageImageObject is the
    // same when we later retrieve it.

    let pdfium = test_bind_to_pdfium();

    let image = pdfium
        .load_pdf_from_file("./test/path-test.pdf", None)?
        .pages()
        .get(0)?
        .render_with_config(&PdfRenderConfig::new().set_target_width(1000))?
        .as_image()?;

    let mut document = pdfium.create_new_pdf()?;

    let mut page = document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())?;

    let object = page.objects_mut().create_image_object(
        PdfPoints::new(100.0),
        PdfPoints::new(100.0),
        &image,
        Some(PdfPoints::new(image.width() as f32)),
        Some(PdfPoints::new(image.height() as f32)),
    )?;

    // Since the object has no image filters applied, both the raw and processed images should
    // be identical to the source image we assigned to the object. The processed image will
    // take the object's scale factors into account, but we made sure to set those to the actual
    // pixel dimensions of the source image.

    // A visual inspection can be carried out by uncommenting the PNG save commands below.

    let raw_image = object.as_image_object().unwrap().get_raw_image()?;

    // raw_image
    //     .save_with_format("./test/1.png", ImageFormat::Png)
    //     .unwrap();

    let processed_image = object
        .as_image_object()
        .unwrap()
        .get_processed_image(&document)?;

    // processed_image
    //     .save_with_format("./test/2.png", ImageFormat::Png)
    //     .unwrap();

    assert!(compare_equality_of_byte_arrays(
        image.as_bytes(),
        raw_image.into_rgba8().as_raw().as_slice()
    ));

    assert!(compare_equality_of_byte_arrays(
        image.as_bytes(),
        processed_image.into_rgba8().as_raw().as_slice()
    ));

    Ok(())
}

fn compare_equality_of_byte_arrays(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    for index in 0..a.len() {
        if a[index] != b[index] {
            return false;
        }
    }

    true
}

#[test]
fn test_image_scaling_keeps_aspect_ratio() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();

    let mut document = pdfium.create_new_pdf()?;

    let mut page = document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())?;

    let image = DynamicImage::new_rgb8(100, 200);

    let object = page.objects_mut().create_image_object(
        PdfPoints::new(0.0),
        PdfPoints::new(0.0),
        &image,
        Some(PdfPoints::new(image.width() as f32)),
        Some(PdfPoints::new(image.height() as f32)),
    )?;

    let image_object = object.as_image_object().unwrap();

    assert_eq!(
        image_object
            .get_processed_bitmap_with_width(&document, 50)?
            .height(),
        100
    );
    assert_eq!(
        image_object
            .get_processed_image_with_width(&document, 50)?
            .height(),
        100
    );
    assert_eq!(
        image_object
            .get_processed_bitmap_with_height(&document, 50)?
            .width(),
        25
    );
    assert_eq!(
        image_object
            .get_processed_image_with_height(&document, 50)?
            .width(),
        25
    );

    Ok(())
}
