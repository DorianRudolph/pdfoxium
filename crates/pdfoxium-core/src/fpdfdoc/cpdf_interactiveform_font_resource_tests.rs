// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use std::cell::RefCell;

fn standard(objects: &HolderRef, data: &CpdfDocPageData, name: &[u8]) -> Rc<CpdfLoadedSimpleFont> {
    let encoding = CpdfFontEncoding::new(FontEncoding::WinAnsi);
    data.add_standard_font(name, Some(&encoding), objects, &mut CfxFontMapper::new())
        .unwrap()
}
fn form_with_fonts(fonts: CpdfDictionary) -> CpdfDictionary {
    let mut resources = CpdfDictionary::new();
    resources.set_new_for(b"Font", fonts);
    let mut form = CpdfDictionary::new();
    form.set_new_for(b"DR", resources);
    form
}
fn font_map(form: &CpdfDictionary) -> ObjectRef {
    form.get_dict_for(b"DR")
        .unwrap()
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .unwrap()
        .get_dict_for(b"Font")
        .unwrap()
}
fn reference(font: &CpdfLoadedSimpleFont, objects: &HolderRef) -> CpdfReference {
    CpdfReference::new(objects, font.font_dict().unwrap().borrow().obj_num())
}

#[test]
fn adding_an_existing_font_reuses_its_dictionary_identity_and_alias() {
    let objects = CpdfIndirectObjectHolder::new();
    let data = CpdfDocPageData::new();
    let helvetica = standard(&objects, &data, b"Helvetica");
    let courier = standard(&objects, &data, b"Courier");
    let mut fonts = CpdfDictionary::new();
    fonts.set_new_for(b"Different", reference(&courier, &objects));
    fonts.set_new_for(b"Existing Alias", reference(&helvetica, &objects));
    let mut form = form_with_fonts(fonts);
    let map = font_map(&form);
    let before = map
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .unwrap()
        .get_keys();
    let mut requested = b"unused requested name".to_vec();
    add_font(&mut form, &objects, &helvetica, &mut requested);
    assert_eq!(requested, b"Existing Alias");
    assert_eq!(find_font(&form, &helvetica), Some(requested));
    assert_eq!(
        map.borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_keys(),
        before
    );
}

#[test]
fn a_same_named_font_dictionary_is_not_the_same_font_resource() {
    let objects = CpdfIndirectObjectHolder::new();
    let data = CpdfDocPageData::new();
    let font = standard(&objects, &data, b"Helvetica");
    let duplicate = font.font_dict().unwrap().borrow().clone_object();
    let mut fonts = CpdfDictionary::new();
    fonts.set_for(b"Similar", Some(duplicate));
    let form = form_with_fonts(fonts);
    assert_eq!(find_font(&form, &font), None);
}

#[test]
fn malformed_resource_entries_reject_the_whole_map_without_mutating_it() {
    let objects = CpdfIndirectObjectHolder::new();
    let data = CpdfDocPageData::new();
    let font = standard(&objects, &data, b"Helvetica");
    for invalid in 0..3 {
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"Valid", reference(&font, &objects));
        match invalid {
            0 => {
                fonts.set_new_for(b"Invalid", CpdfScalarObject::number_from_signed(17));
            }
            1 => {
                let mut wrong = CpdfDictionary::new();
                wrong.set_new_name_for(b"Type", b"XObject");
                fonts.set_new_for(b"Invalid", wrong);
            }
            _ => {
                fonts.set_new_for(b"Invalid", CpdfReference::new(&objects, 999_999));
            }
        }
        let form = form_with_fonts(fonts);
        let map = font_map(&form);
        assert_eq!(find_font(&form, &font), None);
        assert_eq!(
            map.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_keys()
                .len(),
            2
        );
    }
}

#[test]
fn short_blank_and_empty_requested_aliases_have_distinct_collision_contracts() {
    let objects = CpdfIndirectObjectHolder::new();
    let data = CpdfDocPageData::new();
    let font = standard(&objects, &data, b"Helvetica");
    let other = standard(&objects, &data, b"Courier");
    for (requested, expected, collision) in [
        (b" A ".as_slice(), b"A1230".as_slice(), b"A123".as_slice()),
        (b"   ", b"ZiTi0", b"ZiTi"),
        (b"", b"Helve", b"Helv"),
    ] {
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(collision, reference(&other, &objects));
        let mut form = form_with_fonts(fonts);
        let mut alias = requested.to_vec();
        add_font(&mut form, &objects, &font, &mut alias);
        assert_eq!(alias, expected);
        let map = font_map(&form);
        let b = map.borrow();
        let map = b.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        assert_eq!(map.get_keys().len(), 2);
        assert!(Rc::ptr_eq(
            &map.get_dict_for(collision).unwrap(),
            &other.font_dict().unwrap()
        ));
        assert!(Rc::ptr_eq(
            &map.get_dict_for(expected).unwrap(),
            &font.font_dict().unwrap()
        ));
    }
}

#[test]
fn exhausted_short_resource_prefix_advances_past_multi_digit_suffixes() {
    let mut fonts = CpdfDictionary::new();
    fonts.set_new_name_for(b"ABCD", b"Occupied");
    for n in 0..12 {
        fonts.set_new_name_for(format!("ABCD{n}").as_bytes(), b"Occupied");
    }
    let mut resources = CpdfDictionary::new();
    resources.set_new_for(b"Font", fonts);
    assert_eq!(
        generate_new_font_resource_name(&resources, b"ABCD"),
        b"ABCD12"
    );
}

fn document_with_fonts(fonts: CpdfDictionary) -> Rc<RefCell<CpdfDocument>> {
    let doc = Rc::new(RefCell::new(CpdfDocument::new()));
    doc.borrow_mut().create_new_doc();
    let root = doc.borrow().root().unwrap();
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(b"AcroForm", form_with_fonts(fonts));
    doc
}
#[test]
fn portable_native_font_selection_skips_unloadable_entries_and_reuses_ansi() {
    let doc = Rc::new(RefCell::new(CpdfDocument::new()));
    doc.borrow_mut().create_new_doc();
    let objects = doc.borrow().holder();
    let data = doc.borrow().page_data_ref();
    let font = standard(&objects, &data, b"Helvetica");
    let mut bad = CpdfDictionary::new();
    bad.set_new_name_for(b"Type", b"Font");
    bad.set_new_name_for(b"Subtype", b"Type0");
    let mut fonts = CpdfDictionary::new();
    fonts.set_new_for(b"AUnusable", bad);
    fonts.set_new_for(b"ZExisting", reference(&font, &objects));
    let root = doc.borrow().root().unwrap();
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(b"AcroForm", form_with_fonts(fonts));
    let mut tag = b"unchanged".to_vec();
    let chosen = CpdfInteractiveForm::add_native_interactive_form_font(&doc, &mut tag).unwrap();
    assert_eq!(tag, b"ZExisting");
    assert!(Rc::ptr_eq(
        &chosen.font_dict().unwrap(),
        &font.font_dict().unwrap()
    ));
}
#[test]
fn rejected_native_font_maps_leave_the_requested_tag_intact() {
    let mut bad = CpdfDictionary::new();
    bad.set_new_for(b"NotAFont", CpdfScalarObject::number_from_signed(17));
    let doc = document_with_fonts(bad);
    let mut tag = b"unchanged".to_vec();
    assert!(CpdfInteractiveForm::add_native_interactive_form_font(&doc, &mut tag).is_none());
    assert_eq!(tag, b"unchanged");
    let doc = document_with_fonts(CpdfDictionary::new());
    assert!(CpdfInteractiveForm::add_native_interactive_form_font(&doc, &mut tag).is_none());
    assert_eq!(tag, b"unchanged");
}
#[test]
fn form_font_lookup_decodes_the_resource_alias_before_resolving_identity() {
    let doc = Rc::new(RefCell::new(CpdfDocument::new()));
    doc.borrow_mut().create_new_doc();
    let objects = doc.borrow().holder();
    let data = doc.borrow().page_data_ref();
    let font = standard(&objects, &data, b"Courier");
    let mut fonts = CpdfDictionary::new();
    fonts.set_new_for(b"Left Font#1", reference(&font, &objects));
    let root = doc.borrow().root().unwrap();
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(b"AcroForm", form_with_fonts(fonts));
    let form = CpdfInteractiveForm::new(&doc);
    let chosen = form.form_font(b"Left#20Font#231").unwrap();
    assert!(Rc::ptr_eq(
        &chosen.font_dict().unwrap(),
        &font.font_dict().unwrap()
    ));
    assert!(form.form_font(b"Missing").is_none());
    assert!(form.form_font(b"").is_none());
}
