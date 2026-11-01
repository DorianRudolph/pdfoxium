//! Native PDFium is a test oracle only; it is never a production backend.
use crate::prelude::*;
use native_pdfium::prelude as native;

#[test]
fn rendered_edits_match_native_pdfium() -> Result<(), Box<dyn std::error::Error>> {
    let library = pdfoxium_diff::library_path("native");
    let oracle = native::Pdfium::new(native::Pdfium::bind_to_library(library)?);
    let engine = Pdfium::default();
    let mut document = engine.create_new_pdf()?;
    let mut page = document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::new_custom(
            PdfPoints::new(120.),
            PdfPoints::new(80.),
        ))?;
    page.objects_mut().create_path_object_rect(
        PdfRect::new_from_values(5., 10., 35., 50.),
        None,
        None,
        Some(PdfColor::RED),
    )?;
    page.objects_mut().create_path_object_rect(
        PdfRect::new_from_values(45., 70., 75., 110.),
        None,
        None,
        Some(PdfColor::BLUE),
    )?;
    let mut native_created = oracle.create_new_pdf()?;
    let mut native_created_page =
        native_created
            .pages_mut()
            .create_page_at_end(native::PdfPagePaperSize::new_custom(
                native::PdfPoints::new(120.),
                native::PdfPoints::new(80.),
            ))?;
    use native::PdfPageObjectsCommon;
    native_created_page.objects_mut().create_path_object_rect(
        native::PdfRect::new_from_values(5., 10., 35., 50.),
        None,
        None,
        Some(native::PdfColor::RED),
    )?;
    native_created_page.objects_mut().create_path_object_rect(
        native::PdfRect::new_from_values(45., 70., 75., 110.),
        None,
        None,
        Some(native::PdfColor::BLUE),
    )?;
    native_created
        .attachments_mut()
        .create_attachment_from_bytes("retained.txt", b"retained")?;
    let retained = native_created.attachments_mut().get(0)?;
    native_created.attachments_mut().delete_at_index(0)?;
    assert_eq!(retained.save_to_bytes()?, b"retained");
    let bytes = document.save_to_bytes()?;
    let native_document = oracle.load_pdf_from_byte_slice(&bytes, None)?;
    let reopened = engine.load_pdf_from_byte_slice(&bytes, None)?;
    let reopened_page = reopened.pages().first()?;
    let native_page = native_document.pages().first()?;
    for (rotation, native_rotation) in [
        (
            PdfPageRenderRotation::None,
            native::PdfPageRenderRotation::None,
        ),
        (
            PdfPageRenderRotation::Degrees90,
            native::PdfPageRenderRotation::Degrees90,
        ),
        (
            PdfPageRenderRotation::Degrees180,
            native::PdfPageRenderRotation::Degrees180,
        ),
        (
            PdfPageRenderRotation::Degrees270,
            native::PdfPageRenderRotation::Degrees270,
        ),
    ] {
        let config = PdfRenderConfig::new()
            .set_target_size(240, 160)
            .rotate(rotation, false);
        let native_config = native::PdfRenderConfig::new()
            .set_target_size(240, 160)
            .rotate(native_rotation, false);
        let actual = page.render_with_config(&config)?.as_rgba_bytes();
        let reparsed = reopened_page.render_with_config(&config)?.as_rgba_bytes();
        let expected = native_page
            .render_with_config(&native_config)?
            .as_rgba_bytes();
        let native_live = native_created_page
            .render_with_config(&native_config)?
            .as_rgba_bytes();
        // Upstream's rectangle helper builds a multi-subpath path. Saving canonicalizes
        // it to `re`, and native PDFium rasterizes those differently at some rotations.
        // Compare corresponding live and reopened representations independently.
        assert!(actual == native_live, "live render differs at {rotation:?}");
        assert!(
            reparsed == expected,
            "reopened render differs at {rotation:?}"
        );
        assert!(actual.chunks_exact(4).any(|p| p == [255, 0, 0, 255]));
        assert!(actual.chunks_exact(4).any(|p| p == [0, 0, 255, 255]));
    }
    Ok(())
}
