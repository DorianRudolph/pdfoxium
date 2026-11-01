// Upstream pdfium-render 0.9.4 tests; MIT.
use crate::backend::annotations::PdfAnnotationFlags;
use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;

#[test]
fn test_get_annotation_flags() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();
    let document = pdfium.load_pdf_from_file("test/form-test.pdf", None)?;
    let page = document.pages().first()?;
    let annotation = page
        .annotations()
        .iter()
        .find(|annotation| annotation.as_form_field().is_some())
        .unwrap();
    let widget = annotation.as_widget_annotation().unwrap();

    let flags = widget.get_flags_impl();

    assert!(!flags.contains(PdfAnnotationFlags::LockedContents));
    assert_eq!(widget.is_editable(), true);

    assert!(!flags.contains(PdfAnnotationFlags::Locked));
    assert_eq!(widget.is_locked(), false);

    assert!(!flags.contains(PdfAnnotationFlags::ReadOnly));
    assert_eq!(widget.is_read_only(), false);

    assert!(flags.contains(PdfAnnotationFlags::Print));
    assert_eq!(widget.is_printed(), true);

    assert!(!flags.contains(PdfAnnotationFlags::Hidden));
    assert_eq!(widget.is_hidden(), false);

    assert!(!flags.contains(PdfAnnotationFlags::Invisible));
    assert_eq!(widget.is_invisible_if_unsupported(), false);

    Ok(())
}

#[test]
fn test_set_annotation_flags() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();
    let mut document = pdfium.load_pdf_from_file("test/form-test.pdf", None)?;
    let mut page = document.pages_mut().first()?;
    let mut annotation = page
        .annotations_mut()
        .iter()
        .find(|annotation| annotation.as_form_field().is_some())
        .unwrap();
    let widget = annotation.as_widget_annotation_mut().unwrap();

    assert_eq!(widget.is_editable(), true);
    assert_eq!(widget.is_locked(), false);
    assert_eq!(widget.is_read_only(), false);
    assert_eq!(widget.is_printed(), true);
    assert_eq!(widget.is_hidden(), false);
    assert_eq!(widget.is_invisible_if_unsupported(), false);

    let mut flags = widget.get_flags_impl();

    flags.set(PdfAnnotationFlags::ReadOnly, true);
    flags.set(PdfAnnotationFlags::Locked, true);
    flags.set(PdfAnnotationFlags::LockedContents, true);
    flags.set(PdfAnnotationFlags::Print, false);
    flags.set(PdfAnnotationFlags::Hidden, true);
    flags.set(PdfAnnotationFlags::Invisible, true);

    assert!(widget.set_flags_impl(flags));

    assert_eq!(widget.is_editable(), false);
    assert_eq!(widget.is_locked(), true);
    assert_eq!(widget.is_read_only(), true);
    assert_eq!(widget.is_printed(), false);
    assert_eq!(widget.is_hidden(), true);
    assert_eq!(widget.is_invisible_if_unsupported(), true);

    Ok(())
}

#[test]
fn test_update_one_annotation_flag() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();
    let mut document = pdfium.load_pdf_from_file("test/form-test.pdf", None)?;
    let mut page = document.pages_mut().first()?;
    let mut annotation = page
        .annotations_mut()
        .iter()
        .find(|annotation| annotation.as_form_field().is_some())
        .unwrap();
    let widget = annotation.as_widget_annotation_mut().unwrap();

    assert_eq!(widget.is_editable(), true);
    assert_eq!(widget.is_locked(), false);
    assert_eq!(widget.is_read_only(), false);
    assert_eq!(widget.is_printed(), true);
    assert_eq!(widget.is_hidden(), false);
    assert_eq!(widget.is_invisible_if_unsupported(), false);

    widget.set_is_editable(false)?;
    assert_eq!(widget.is_editable(), false);

    widget.set_is_locked(true)?;
    assert_eq!(widget.is_locked(), true);

    widget.set_is_read_only(true)?;
    assert_eq!(widget.is_read_only(), true);

    widget.set_is_printed(false)?;
    assert_eq!(widget.is_printed(), false);

    widget.set_is_hidden(true)?;
    assert_eq!(widget.is_hidden(), true);

    widget.set_is_invisible_if_unsupported(true)?;
    assert_eq!(widget.is_invisible_if_unsupported(), true);

    assert_eq!(widget.is_editable(), false);
    assert_eq!(widget.is_locked(), true);
    assert_eq!(widget.is_read_only(), true);
    assert_eq!(widget.is_printed(), false);
    assert_eq!(widget.is_hidden(), true);
    assert_eq!(widget.is_invisible_if_unsupported(), true);

    Ok(())
}
