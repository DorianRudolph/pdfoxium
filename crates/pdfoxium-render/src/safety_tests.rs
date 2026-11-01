use crate::prelude::*;
fn document(engine: &Pdfium) -> (PdfDocument<'_>, PdfPage<'_>) {
    let mut doc = engine.create_new_pdf().unwrap();
    let page = doc
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::new_custom(
            PdfPoints::new(40.),
            PdfPoints::new(30.),
        ))
        .unwrap();
    (doc, page)
}
#[test]
fn borrowed_bitmap_is_written_in_place_and_preserves_tail() {
    let engine = Pdfium::default();
    let (_doc, mut page) = document(&engine);
    page.objects_mut()
        .create_path_object_rect(
            PdfRect::new_from_values(0., 0., 30., 40.),
            None,
            None,
            Some(PdfColor::RED),
        )
        .unwrap();
    let mut bytes = vec![93; 40 * 30 * 4 + 16];
    let original = bytes.as_ptr();
    {
        let mut bitmap = PdfBitmap::from_bytes(40, 30, PdfBitmapFormat::BGRA, &mut bytes).unwrap();
        page.render_into_bitmap_with_config(&mut bitmap, &PdfRenderConfig::new())
            .unwrap();
        assert_eq!(bitmap.as_raw_bytes_mut().as_ptr(), original);
        assert!(bitmap
            .as_rgba_bytes()
            .chunks_exact(4)
            .all(|p| p == [255, 0, 0, 255]));
        bitmap.fill_rect(0, 0, 1, 1, PdfColor::BLUE);
        assert_eq!(&bitmap.as_rgba_bytes()[..4], &[0, 0, 255, 255]);
    }
    assert_ne!(bytes[0], 93);
    assert_eq!(&bytes[40 * 30 * 4..], &[93; 16]);
}
#[test]
fn page_and_text_outlive_document_handle() {
    let engine = Pdfium::default();
    let (mut document, mut page) = document(&engine);
    let font = document.fonts_mut().courier();
    page.objects_mut()
        .create_text_object(
            PdfPoints::new(1.),
            PdfPoints::new(10.),
            "retained",
            font,
            PdfPoints::new(8.),
        )
        .unwrap();
    let text = page.text().unwrap();
    drop(document);
    assert_eq!(text.all().trim(), "retained");
    assert!(page.render_with_config(&PdfRenderConfig::new()).is_ok());
    assert_eq!(text.all().trim(), "retained");
}
#[test]
fn deleted_page_handles_fail_without_retargeting() {
    let engine = Pdfium::default();
    let (mut document, page) = document(&engine);
    let mut old = page;
    let mut object = old
        .objects_mut()
        .create_path_object_rect(
            PdfRect::new_from_values(0., 0., 5., 5.),
            None,
            None,
            Some(PdfColor::RED),
        )
        .unwrap();
    document.pages().get(0).unwrap().delete().unwrap();
    document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())
        .unwrap();
    assert!(matches!(
        old.render_with_config(&PdfRenderConfig::new()),
        Err(PdfiumError::OwnershipNotAttachedToPage)
    ));
    assert!(matches!(
        object.translate(PdfPoints::new(1.), PdfPoints::ZERO),
        Err(PdfiumError::OwnershipNotAttachedToPage)
    ));
}
#[test]
fn insertion_and_import_preserve_existing_page_identity() {
    let engine = Pdfium::default();
    let (mut destination, first) = document(&engine);
    let (source, _) = document(&engine);
    destination
        .pages_mut()
        .copy_pages_from_document(&source, "1,1", 0)
        .unwrap();
    assert_eq!(destination.pages().len(), 3);
    assert!(first.text().is_ok());
    destination.pages().get(2).unwrap().delete().unwrap();
    assert!(matches!(
        first.text(),
        Err(PdfiumError::OwnershipNotAttachedToPage)
    ));
}
#[test]
fn wrong_document_font_and_object_are_rejected() {
    let engine = Pdfium::default();
    let (mut a, _) = document(&engine);
    let (b, mut page) = document(&engine);
    let font = a.fonts_mut().courier();
    assert!(page
        .objects_mut()
        .create_text_object(
            PdfPoints::ZERO,
            PdfPoints::ZERO,
            "wrong",
            font,
            PdfPoints::new(10.)
        )
        .is_err());
    let object = PdfPagePathObject::new_rect(
        &a,
        PdfRect::new_from_values(0., 0., 5., 5.),
        None,
        None,
        Some(PdfColor::RED),
    )
    .unwrap();
    assert!(page.objects_mut().add_path_object(object).is_err());
    assert_eq!(b.pages().first().unwrap().objects().len(), 0);
}
#[test]
fn edits_survive_save_and_reopen_and_snapshots_stay_immutable() {
    let engine = Pdfium::default();
    let (mut document, mut page) = document(&engine);
    let font = document.fonts_mut().courier();
    page.objects_mut()
        .create_text_object(
            PdfPoints::new(1.),
            PdfPoints::new(10.),
            "first",
            font,
            PdfPoints::new(8.),
        )
        .unwrap();
    let before = page.text().unwrap();
    document
        .pages()
        .first()
        .unwrap()
        .objects_mut()
        .create_text_object(
            PdfPoints::new(1.),
            PdfPoints::new(20.),
            "second",
            font,
            PdfPoints::new(8.),
        )
        .unwrap();
    assert!(!before.all().contains("second"));
    assert!(page.text().unwrap().all().contains("second"));
    let bytes = document.save_to_bytes().unwrap();
    let reopened = engine.load_pdf_from_byte_vec(bytes, None).unwrap();
    assert_eq!(
        page.text().unwrap().all(),
        reopened.pages().first().unwrap().text().unwrap().all()
    );
}
#[test]
fn attachment_roundtrip_retains_data_after_name_removal() {
    let engine = Pdfium::default();
    let (mut document, _) = document(&engine);
    let attachment = document
        .attachments_mut()
        .create_attachment_from_bytes("notes.txt", b"safe bytes")
        .unwrap();
    assert_eq!(attachment.save_to_bytes().unwrap(), b"safe bytes");
    drop(attachment);
    let attachment = document.attachments_mut().get(0).unwrap();
    let reopened = engine
        .load_pdf_from_byte_vec(document.save_to_bytes().unwrap(), None)
        .unwrap();
    assert_eq!(
        reopened
            .attachments()
            .get(0)
            .unwrap()
            .save_to_bytes()
            .unwrap(),
        b"safe bytes"
    );
    document.attachments_mut().delete_at_index(0).unwrap();
    assert!(document.attachments().is_empty());
    assert_eq!(attachment.save_to_bytes().unwrap(), b"safe bytes");
}
#[test]
fn malformed_input_and_writer_errors_are_reported() {
    let engine = Pdfium::default();
    assert!(Pdfium::default()
        .load_pdf_from_byte_slice(b"not a PDF", None)
        .is_err());
    struct Failing;
    impl std::io::Write for Failing {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("sink failed"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let (document, _) = document(&engine);
    assert!(matches!(
        document.save_to_writer(&mut Failing),
        Err(PdfiumError::IoError(_))
    ));
}
