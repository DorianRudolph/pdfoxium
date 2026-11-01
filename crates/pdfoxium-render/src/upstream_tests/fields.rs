// Upstream pdfium-render 0.9.4 tests; MIT.
use crate::fields::{FieldHelpers, PdfFormFieldFlags};
use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;

#[test]
fn test_get_form_field_flags() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();
    let document = pdfium.load_pdf_from_file("test/form-test.pdf", None)?;
    let page = document.pages().first()?;
    let annotation = page
        .annotations()
        .iter()
        .find(|annotation| annotation.as_form_field().is_some())
        .unwrap();
    let field = annotation.as_form_field().unwrap();
    let text = field.as_text_field().unwrap();

    let flags = text.get_flags_impl();

    assert!(!flags.contains(PdfFormFieldFlags::ReadOnly));
    assert_eq!(text.is_read_only(), false);

    assert!(!flags.contains(PdfFormFieldFlags::Required));
    assert_eq!(text.is_required(), false);

    assert!(!flags.contains(PdfFormFieldFlags::NoExport));
    assert_eq!(text.is_exported_on_submit(), true);

    assert!(!flags.contains(PdfFormFieldFlags::TextMultiline));
    assert_eq!(text.is_multiline(), false);

    assert!(!flags.contains(PdfFormFieldFlags::TextPassword));
    assert_eq!(text.is_password(), false);

    // The fixture sets bit 23 (DoNotScroll), not bit 22 (DoNotSpellCheck).
    // Upstream 0.9.4 confuses these bits; preserve the PDF specification here.
    assert!(!flags.contains(PdfFormFieldFlags::TextDoNotSpellCheck));
    assert!(flags.contains(PdfFormFieldFlags::TextDoNotScroll));
    assert_eq!(text.is_spell_checked(), true);

    Ok(())
}

#[test]
fn test_set_form_field_flags() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();
    let mut document = pdfium.load_pdf_from_file("test/form-test.pdf", None)?;
    let mut page = document.pages_mut().first()?;
    let mut annotation = page
        .annotations_mut()
        .iter()
        .find(|annotation| annotation.as_form_field().is_some())
        .unwrap();
    let field = annotation.as_form_field_mut().unwrap();
    let text = field.as_text_field_mut().unwrap();

    assert_eq!(text.is_read_only(), false);
    assert_eq!(text.is_required(), false);
    assert_eq!(text.is_exported_on_submit(), true);
    assert_eq!(text.is_multiline(), false);
    assert_eq!(text.is_password(), false);
    assert_eq!(text.is_spell_checked(), true);

    let mut flags = text.get_flags_impl();

    flags.set(PdfFormFieldFlags::ReadOnly, true);
    flags.set(PdfFormFieldFlags::Required, true);
    flags.set(PdfFormFieldFlags::NoExport, true);
    flags.set(PdfFormFieldFlags::TextMultiline, true);
    flags.set(PdfFormFieldFlags::TextPassword, true);
    flags.set(PdfFormFieldFlags::TextDoNotSpellCheck, false);

    assert!(text.set_flags_impl(flags));

    assert_eq!(text.is_read_only(), true);
    assert_eq!(text.is_required(), true);
    assert_eq!(text.is_exported_on_submit(), false);
    assert_eq!(text.is_multiline(), true);
    assert_eq!(text.is_password(), true);
    assert_eq!(text.is_spell_checked(), true);

    Ok(())
}

#[test]
fn test_update_one_form_field_flag() -> Result<(), PdfiumError> {
    let pdfium = test_bind_to_pdfium();
    let mut document = pdfium.load_pdf_from_file("test/form-test.pdf", None)?;
    let mut page = document.pages_mut().first()?;
    let mut annotation = page
        .annotations_mut()
        .iter()
        .find(|annotation| annotation.as_form_field().is_some())
        .unwrap();
    let field = annotation.as_form_field_mut().unwrap();
    let text = field.as_text_field_mut().unwrap();

    assert_eq!(text.is_read_only(), false);
    assert_eq!(text.is_required(), false);
    assert_eq!(text.is_exported_on_submit(), true);
    assert_eq!(text.is_multiline(), false);
    assert_eq!(text.is_password(), false);
    assert_eq!(text.is_spell_checked(), true);

    text.set_is_read_only(true)?;
    assert_eq!(text.is_read_only(), true);

    text.set_is_required(true)?;
    assert_eq!(text.is_required(), true);

    text.set_is_exported_on_submit(false)?;
    assert_eq!(text.is_exported_on_submit(), false);

    text.set_is_multiline(true)?;
    assert_eq!(text.is_multiline(), true);

    text.set_is_password(true)?;
    assert_eq!(text.is_password(), true);

    text.set_is_spell_checked(true)?;
    assert_eq!(text.is_spell_checked(), true);

    assert_eq!(text.is_read_only(), true);
    assert_eq!(text.is_required(), true);
    assert_eq!(text.is_exported_on_submit(), false);
    assert_eq!(text.is_multiline(), true);
    assert_eq!(text.is_password(), true);
    assert_eq!(text.is_spell_checked(), true);

    Ok(())
}
