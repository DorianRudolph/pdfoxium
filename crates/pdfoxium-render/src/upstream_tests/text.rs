// Tests from pdfium-render 0.9.4; MIT. See LICENSE.pdfium-render.
use itertools::Itertools;
use std::ffi::OsStr;
use std::fs;

use crate::prelude::*;
use crate::test_support::test_bind_to_pdfium;

#[test]
fn test_overlapping_chars_results() -> Result<(), PdfiumError> {
    // Test to make sure the result of the .chars_for_object() function returns the
    // correct results in the event of overlapping text objects.
    // For more details, see: https://github.com/ajrcarey/pdfium-render/issues/98

    let pdfium = test_bind_to_pdfium();

    // Create a new document with three overlapping text objects.

    let mut document = pdfium.create_new_pdf()?;

    let mut page = document
        .pages_mut()
        .create_page_at_start(PdfPagePaperSize::a4())?;

    let font = document.fonts_mut().courier();

    let txt1 = page.objects_mut().create_text_object(
        PdfPoints::ZERO,
        PdfPoints::ZERO,
        "AAAAAA",
        font,
        PdfPoints::new(10.0),
    )?;

    let txt2 = page.objects_mut().create_text_object(
        PdfPoints::ZERO,
        PdfPoints::ZERO,
        "BBBBBB",
        font,
        PdfPoints::new(10.0),
    )?;

    let txt3 = page.objects_mut().create_text_object(
        PdfPoints::ZERO,
        PdfPoints::ZERO,
        "CDCDCDE",
        font,
        PdfPoints::new(10.0),
    )?;

    let page_text = page.text()?;

    // Check the results for all three objects are not affected by overlapping.

    assert!(test_one_overlapping_text_object_results(
        &txt1, &page_text, "AAAAAA"
    )?);
    assert!(test_one_overlapping_text_object_results(
        &txt2, &page_text, "BBBBBB"
    )?);
    assert!(test_one_overlapping_text_object_results(
        &txt3, &page_text, "CDCDCDE"
    )?);

    Ok(())
}

fn test_one_overlapping_text_object_results(
    object: &PdfPageObject,
    page_text: &PdfPageText,
    expected: &str,
) -> Result<bool, PdfiumError> {
    if let Some(txt) = object.as_text_object() {
        assert_eq!(txt.text().trim(), expected);
        assert_eq!(page_text.for_object(txt).trim(), expected);

        for (index, char) in txt.chars(&page_text)?.iter().enumerate() {
            assert_eq!(txt.text().chars().nth(index), char.unicode_char());
            assert_eq!(expected.chars().nth(index), char.unicode_char());
        }

        Ok(true)
    } else {
        Ok(false)
    }
}

#[test]
fn test_text_chars_results_equality() -> Result<(), PdfiumError> {
    // For all available test documents, check that the results of
    // PdfPageObjectText::text() and PdfPageObjectText::chars() match.

    let pdfium = test_bind_to_pdfium();

    let samples = fs::read_dir("./test/")
        .unwrap()
        .filter_map(|entry| match entry {
            Ok(e) => Some(e.path()),
            Err(_) => None,
        })
        .filter(|path| path.extension() == Some(OsStr::new("pdf")))
        .collect::<Vec<_>>();

    assert!(samples.len() > 0);

    for sample in samples {
        println!("Testing all text objects in file {}", sample.display());

        let document = pdfium.load_pdf_from_file(&sample, None)?;

        for page in document.pages().iter() {
            let text = page.text()?;

            for object in page.objects().iter() {
                if let Some(obj) = object.as_text_object() {
                    let chars = obj
                        .chars(&text)?
                        .iter()
                        .filter_map(|char| char.unicode_string())
                        .join("");

                    assert_eq!(obj.text().trim(), chars.replace("\0", "").trim());
                }
            }
        }
    }

    Ok(())
}

#[test]
fn test_text_segment_chars_char_lifetimes() -> Result<(), PdfiumError> {
    // Lifetimes of segments, text chars, and text char should be bound to the
    // lifetime of page text, but not necessarily to one another. See:
    // https://github.com/ajrcarey/pdfium-render/pull/248

    let pdfium = test_bind_to_pdfium();
    let document = pdfium.load_pdf_from_file("./test/export-test.pdf", None)?;
    let page = document.pages().first()?;
    let text = page.text()?;

    let _char = {
        let chars = {
            let segment = text.segments().first()?;

            segment.chars()?
        }; // PdfPageTextSegment object is dropped here

        chars.first()?
    }; // PdfPageTextChars object is dropped here

    Ok(())
}
