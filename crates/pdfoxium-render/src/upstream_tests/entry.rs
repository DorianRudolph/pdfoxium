// Upstream pdfium-render 0.9.4 safe API tests; MIT.
use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;
use image::ImageFormat;
use std::fs::File;
use std::path::Path;

#[test]
fn test_readme_example() -> Result<(), PdfiumError> {
    // Runs the code in the main example at the top of README.md.

    fn export_pdf_to_jpegs(
        path: &impl AsRef<Path>,
        password: Option<&str>,
    ) -> Result<(), PdfiumError> {
        // Renders each page in the given test PDF file to a separate JPEG file.

        // Bind to a Pdfium library in the same directory as our Rust executable.
        // See the "Dynamic linking" section below.

        let pdfium = Pdfium::default();

        // Open the PDF document...

        let document = pdfium.load_pdf_from_file(path, password)?;

        // ... set rendering options that will apply to all pages...

        let render_config = PdfRenderConfig::new()
            .set_target_width(2000)
            .set_maximum_height(2000)
            .rotate_if_landscape(PdfPageRenderRotation::Degrees90, true);

        // ... then render each page to a bitmap image, saving each image to a JPEG file.

        for (index, page) in document.pages().iter().enumerate() {
            page.render_with_config(&render_config)?
                .as_image()? // Renders this page to an Image::DynamicImage...
                .into_rgb8() // ... then converts it to an Image::Image ...
                .save_with_format(
                    format!(
                        "{}/../../target/pdfium-render-entry-page-{}.jpg",
                        env!("CARGO_MANIFEST_DIR"),
                        index
                    ),
                    ImageFormat::Jpeg,
                ) // ... and saves it to a file.
                .map_err(|_| PdfiumError::ImageError)?;
        }

        Ok(())
    }

    export_pdf_to_jpegs(&"./test/export-test.pdf", None)
}

#[test]
fn test_dynamic_bindings() -> Result<(), PdfiumError> {
    let pdfium = Pdfium::default();

    let document = pdfium.load_pdf_from_file("./test/form-test.pdf", None)?;

    let render_config = PdfRenderConfig::new()
        .set_target_width(2000)
        .set_maximum_height(2000)
        .rotate_if_landscape(PdfPageRenderRotation::Degrees90, true)
        .render_form_data(true)
        .render_annotations(true);

    for (index, page) in document.pages().iter().enumerate() {
        let result = page
            .render_with_config(&render_config)?
            .as_image()?
            .into_rgb8()
            .save_with_format(
                format!(
                    "{}/../../target/pdfium-render-form-page-{}.jpg",
                    env!("CARGO_MANIFEST_DIR"),
                    index
                ),
                ImageFormat::Jpeg,
            );

        assert!(result.is_ok());
    }

    Ok(())
}

#[test]
fn test_reader_lifetime() -> Result<(), PdfiumError> {
    // Confirms that a reader given to Pdfium::load_pdf_from_reader() does not need
    // a lifetime longer than that of the PdfDocument it is used to create.

    let pdfium = test_bind_to_pdfium();

    let paths = ["test/form-test.pdf", "test/annotations-test.pdf"];

    for path in paths {
        let page_count = {
            let reader = File::open(path).map_err(PdfiumError::IoError)?;

            let document = pdfium.load_pdf_from_reader(reader, None)?;

            document.pages().len()

            // reader will be dropped here, immediately after document.
        };

        println!("{} has {} pages", path, page_count);
    }

    Ok(())
}
