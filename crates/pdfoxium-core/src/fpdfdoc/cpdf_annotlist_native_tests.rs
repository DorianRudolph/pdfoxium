// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Complete `core/fpdfdoc/cpdf_annotlist_unittest.cpp` popup cases.
use super::*;
use crate::{
    fpdfapi::{
        parser::{cpdf_document::CpdfDocument, cpdf_string::CpdfString},
        render::cpdf_rendercontext::CpdfRenderContext,
    },
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
};
fn page(contents: &[u8]) -> (CpdfDocument, CpdfPage) {
    let mut document = CpdfDocument::new();
    document.create_new_doc();
    // Native CPDF_TestDocument owns this otherwise detached page and its
    // document resources; no page dictionary/annotation fixture is serialized.
    let mut annotation = CpdfDictionary::new();
    annotation.set_new_name_for(b"Subtype", b"Text");
    annotation.set_new_for(b"Contents", CpdfString::from_bytes(None, contents));
    let mut annotations = CpdfArray::new();
    annotations.append_new(annotation);
    let mut dictionary = CpdfDictionary::new();
    dictionary.set_new_for(b"Annots", annotations);
    let page =
        CpdfPage::new_with_page_data(retain_object(dictionary), document.page_data_ref()).unwrap();
    (document, page)
}
fn has_ap(annotation: &CpdfAnnot) -> bool {
    annotation
        .dict()
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .unwrap()
        .key_exist(b"AP")
}
fn contents(annotation: &CpdfAnnot) -> (Vec<u8>, Vec<u32>) {
    let object = annotation.dict().borrow();
    let dict = object.as_any().downcast_ref::<CpdfDictionary>().unwrap();
    (
        dict.get_byte_string_for(b"Contents", b""),
        dict.get_unicode_text_for(b"Contents"),
    )
}
#[test]
fn create_popup_annot_from_pdf_encoded() {
    let bytes = [b'A', b'a', 0xe4, 0xa0];
    let (_document, page) = page(&bytes);
    let list = CpdfAnnotList::new(&page);
    assert_eq!(list.annot_list.len(), 2);
    assert_eq!(
        contents(&list.annot_list[1]),
        (bytes.to_vec(), "Aaä€".chars().map(u32::from).collect())
    );
}
#[test]
fn create_popup_annot_from_unicode() {
    let bytes = [
        0xfe, 0xff, 0, b'A', 0, b'a', 0, 0xe4, 0x20, 0xac, 0xd8, 0x3c, 0xdf, 0xa8,
    ];
    let (_document, page) = page(&bytes);
    let list = CpdfAnnotList::new(&page);
    assert_eq!(list.annot_list.len(), 2);
    assert_eq!(
        contents(&list.annot_list[1]),
        (bytes.to_vec(), "Aaä€🎨".chars().map(u32::from).collect())
    );
}
#[test]
fn create_popup_annot_from_empty_pdf_encoded() {
    let (_document, page) = page(b"");
    assert_eq!(CpdfAnnotList::new(&page).annot_list.len(), 1);
}
#[test]
fn create_popup_annot_from_empty_unicode() {
    let (_document, page) = page(&[0xfe, 0xff]);
    assert_eq!(CpdfAnnotList::new(&page).annot_list.len(), 1);
}
#[test]
fn create_popup_annot_from_empty_unicoded_with_escape() {
    let (_document, page) = page(&[0xfe, 0xff, 0, 0x1b, b'j', b'a', 0, 0x1b]);
    assert_eq!(CpdfAnnotList::new(&page).annot_list.len(), 1);
}
#[test]
fn closed_popup_appearance_objects() {
    let (document, page) = page(b"Regression comment");
    {
        let list = CpdfAnnotList::new(&page);
        assert_eq!(list.annot_list.len(), 2);
        assert!(has_ap(&list.annot_list[0]));
        assert!(!has_ap(&list.annot_list[1]));
    }
    let objects = document.holder();
    let object_count = objects.borrow().last_obj_num();
    for _ in 0..30 {
        let list = CpdfAnnotList::new(&page);
        assert!(!has_ap(&list.annot_list[1]));
        assert_eq!(objects.borrow().last_obj_num(), object_count);
    }
}
fn draw_popup(
    popup: &mut CpdfAnnot,
    page: &CpdfPage,
    bitmap: &mut CfxDibitmap,
    use_context: bool,
) -> bool {
    bitmap.clear(0xffffffff);
    let mut driver = CfxAggDeviceDriver::new(bitmap, false, None);
    if !use_context {
        return popup.draw_appearance(
            page,
            &mut driver,
            CfxMatrix::default(),
            AppearanceMode::Normal,
        );
    }
    let mut context = CpdfRenderContext::new(page);
    let result = popup.draw_in_context(
        page,
        &mut context,
        CfxMatrix::default(),
        AppearanceMode::Normal,
    );
    assert!(context.render(&mut driver, None, None, None));
    result
}
fn center_pixel(bitmap: &CfxDibitmap) -> u32 {
    u32::from_le_bytes(bitmap.scanline(100).unwrap()[400..404].try_into().unwrap())
}
#[test]
fn popup_rendering() {
    // Both native Bool() parameters, including the direct and deferred context
    // paths, retain every source transition, pixel and allocation assertion.
    for use_context in [false, true] {
        let (document, page) = page(b"Regression comment");
        let mut list = CpdfAnnotList::new(&page);
        assert_eq!(list.annot_list.len(), 2);
        let popup = &mut list.annot_list[1];
        assert_eq!(popup.subtype(), Subtype::Popup);
        assert!(!has_ap(popup));
        let mut bitmap = CfxDibitmap::create(256, 256, FxdibFormat::Bgra, 0).unwrap();
        assert!(!draw_popup(popup, &page, &mut bitmap, use_context));
        assert_eq!(center_pixel(&bitmap), 0xffffffff);
        popup.set_open_state(true);
        assert!(draw_popup(popup, &page, &mut bitmap, use_context));
        assert_eq!(center_pixel(&bitmap), 0xffffff00);
        assert!(has_ap(popup));
        let objects = document.holder();
        let object_count = objects.borrow().last_obj_num();
        popup.set_open_state(false);
        assert!(!draw_popup(popup, &page, &mut bitmap, use_context));
        assert_eq!(center_pixel(&bitmap), 0xffffffff);
        popup.set_open_state(true);
        assert!(draw_popup(popup, &page, &mut bitmap, use_context));
        assert_eq!(center_pixel(&bitmap), 0xffffff00);
        assert_eq!(objects.borrow().last_obj_num(), object_count);
    }
}
