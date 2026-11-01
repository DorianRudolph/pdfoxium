// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Selected ordinary-PDF document/page creation from `fpdfsdk/fpdf_editpage.cpp`.
//! Clock/local/UTC conversion is supplied by the C ABI's Rust host adapter.
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fxcrt::fx_coordinates::CfxFloatRect;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy)]
pub struct PdfDateFields {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}
pub fn format_pdf_date(local: PdfDateFields, utc: Option<PdfDateFields>) -> Vec<u8> {
    let mut date = format!(
        "D:{:04}{:02}{:02}{:02}{:02}{:02}",
        local.year, local.month, local.day, local.hour, local.minute, local.second
    );
    if let Some(utc) = utc {
        use crate::fxcrt::fx_extension::{time_zone_offset_in_minutes, TimeZoneFields};
        let fields = |time: PdfDateFields| TimeZoneFields {
            day: time.day as i32,
            hour: time.hour as i32,
            minute: time.minute as i32,
        };
        let minutes = time_zone_offset_in_minutes(fields(local), fields(utc));
        let abs = minutes.abs();
        date.push_str(&format!(
            "{}{:02}'{:02}'",
            if minutes < 0 { '-' } else { '+' },
            abs / 60,
            abs % 60
        ));
    }
    date.into_bytes()
}
pub fn create_new_document(
    enabled: bool,
    date: impl FnOnce() -> Option<Vec<u8>>,
) -> Rc<RefCell<CpdfDocument>> {
    let mut document = CpdfDocument::new();
    document.create_new_doc();
    let date = if enabled {
        date().unwrap_or_default()
    } else {
        Vec::new()
    };
    if let Some(info) = document.info() {
        let mut borrowed = info.borrow_mut();
        let info = borrowed
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap();
        if enabled {
            info.set_new_for(b"CreationDate", CpdfString::from_bytes(None, &date));
        }
        info.set_new_for(
            b"Creator",
            CpdfString::from_unicode(None, &[80, 68, 70, 105, 117, 109]),
        );
    }
    Rc::new(RefCell::new(document))
}
pub fn new_page(
    document: &Rc<RefCell<CpdfDocument>>,
    index: i32,
    width: f64,
    height: f64,
) -> Option<CpdfPage> {
    let index = index.clamp(
        0,
        i32::try_from(document.borrow().page_count()).expect("native page count int"),
    );
    let dictionary = document.borrow_mut().create_new_page(index)?;
    {
        let mut borrowed = dictionary.borrow_mut();
        let d = borrowed
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap();
        d.set_rect_for(
            b"MediaBox",
            CfxFloatRect::new(0., 0., width as f32, height as f32),
        );
        d.set_new_for(b"Rotate", CpdfScalarObject::number_from_signed(0));
        d.set_new_for(
            b"Resources",
            CpdfDictionary::with_pool(d.byte_string_pool()),
        );
    }
    let data = document.borrow().page_data_ref();
    let mut page = CpdfPage::new_with_page_data(dictionary, data).expect("new page dictionary");
    page.parse_path_content();
    Some(page)
}

pub fn calc_bounding_box(object: &mut dyn crate::fpdfapi::page::cpdf_pageobject::PageObject) {
    use crate::fpdfapi::page::{
        cpdf_formobject::CpdfFormObject, cpdf_imageobject::CpdfImageObject,
        cpdf_pageobject::PageObjectType, cpdf_pathobject::CpdfPathObject,
        cpdf_shadingobject::CpdfShadingObject,
    };
    match object.object_type() {
        PageObjectType::Text => (),
        PageObjectType::Path => object
            .as_any_mut()
            .downcast_mut::<CpdfPathObject>()
            .unwrap()
            .calc_bounding_box(),
        PageObjectType::Image => object
            .as_any_mut()
            .downcast_mut::<CpdfImageObject>()
            .unwrap()
            .calc_bounding_box(),
        PageObjectType::Shading => object
            .as_any_mut()
            .downcast_mut::<CpdfShadingObject>()
            .unwrap()
            .calc_bounding_box(),
        PageObjectType::Form => object
            .as_any_mut()
            .downcast_mut::<CpdfFormObject>()
            .unwrap()
            .calc_bounding_box(),
    }
}
pub fn set_object_matrix(
    object: &mut dyn crate::fpdfapi::page::cpdf_pageobject::PageObject,
    matrix: crate::fxcrt::fx_coordinates::CfxMatrix,
) -> bool {
    use crate::fpdfapi::page::{
        cpdf_formobject::CpdfFormObject, cpdf_imageobject::CpdfImageObject,
        cpdf_pageobject::PageObjectType, cpdf_pathobject::CpdfPathObject,
        cpdf_textobject::CpdfTextObject,
    };
    match object.object_type() {
        PageObjectType::Text => object
            .as_any_mut()
            .downcast_mut::<CpdfTextObject>()
            .unwrap()
            .set_text_matrix(matrix),
        PageObjectType::Path => object
            .as_any_mut()
            .downcast_mut::<CpdfPathObject>()
            .unwrap()
            .set_path_matrix(matrix),
        PageObjectType::Image => {
            object
                .as_any_mut()
                .downcast_mut::<CpdfImageObject>()
                .unwrap()
                .set_image_matrix(matrix);
            let dirty = object.base().original_matrix() != matrix;
            object.base_mut().set_matrix_dirty(dirty);
            return true;
        }
        PageObjectType::Shading => return false,
        PageObjectType::Form => object
            .as_any_mut()
            .downcast_mut::<CpdfFormObject>()
            .unwrap()
            .set_form_matrix(matrix),
    }
    object.base_mut().set_matrix_dirty(true);
    true
}

/// FPDFPage_TransformAnnots: retain the existing Rect array when present.
/// Appearance geometry is intentionally unchanged, matching the source TODO.
pub fn transform_annots(page: &CpdfPage, matrix: crate::fxcrt::fx_coordinates::CfxMatrix) {
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfdoc::cpdf_annotlist::CpdfAnnotList;
    let annots = CpdfAnnotList::new(page);
    for annot in annots.iter() {
        let rect = matrix.transform_float_rect(annot.rect());
        let existing = annot
            .dict()
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_array_for(b"Rect");
        let array = existing.unwrap_or_else(|| {
            annot
                .dict()
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_for(b"Rect", CpdfArray::new())
        });
        let mut borrowed = array.borrow_mut();
        let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        array.clear();
        for value in [rect.left, rect.bottom, rect.right, rect.top] {
            array.append_new(CpdfScalarObject::number_from_float(value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn document_metadata_preserves_policy_clock_failure_and_creator_encoding() {
        for enabled in [false, true] {
            let doc = create_new_document(enabled, || {
                assert!(enabled);
                None
            });
            let info = doc.borrow_mut().info().unwrap();
            let borrowed = info.borrow();
            let d = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
            assert_eq!(d.key_exist(b"CreationDate"), enabled);
            assert_eq!(d.get_byte_string_for(b"CreationDate", b""), b"");
            assert_eq!(d.get_byte_string_for(b"Creator", b""), b"PDFium");
        }
        let d = create_new_document(true, || Some(b"D:20261022012345+00'00'".to_vec()));
        let info = d.borrow_mut().info().unwrap();
        assert_eq!(
            info.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_byte_string_for(b"CreationDate", b""),
            b"D:20261022012345+00'00'"
        );
    }
    #[test]
    fn pdf_date_formats_positive_negative_quarter_hour_and_year_boundary_offsets() {
        let utc = PdfDateFields {
            year: 2025,
            month: 12,
            day: 31,
            hour: 23,
            minute: 50,
            second: 7,
        };
        let local = PdfDateFields {
            year: 2026,
            month: 1,
            day: 1,
            hour: 5,
            minute: 35,
            second: 7,
        };
        assert_eq!(
            format_pdf_date(local, Some(utc)),
            b"D:20260101053507+05'45'"
        );
        assert_eq!(
            format_pdf_date(utc, Some(local)),
            b"D:20251231235007-05'45'"
        );
        assert_eq!(format_pdf_date(local, None), b"D:20260101053507");
        assert_eq!(format_pdf_date(utc, Some(utc)), b"D:20251231235007+00'00'");
    }
}
