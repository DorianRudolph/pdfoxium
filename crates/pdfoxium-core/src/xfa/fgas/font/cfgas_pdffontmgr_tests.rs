// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::fpdfapi::parser::{
    cpdf_dictionary::CpdfDictionary,
    cpdf_object::{make_reference, retain_object},
    cpdf_scalar_object::CpdfScalarObject,
};
use crate::fxge::fx_font::{FONT_STYLE_FORCE_BOLD, FONT_STYLE_ITALIC};
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn document() -> CpdfDocument {
    let mut result = CpdfDocument::new();
    result.create_new_doc();
    result
}
fn resources(document: &CpdfDocument) -> ObjectRef {
    let root = document.root().unwrap();
    let acroform = root
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .get_or_create_dict_for(b"AcroForm");
    let resources = acroform
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .get_or_create_dict_for(b"DR");
    let fonts = resources
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .get_or_create_dict_for(b"Font");
    fonts
}
fn add_embedded(document: &CpdfDocument, name: &[u8]) -> ObjectRef {
    let holder = document.holder();
    let stream = holder
        .borrow_mut()
        .new_indirect_stream_from_bytes(include_bytes!(
            "../../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"
        ));
    let mut descriptor = CpdfDictionary::new();
    descriptor.set_new_for(b"Flags", CpdfScalarObject::number_from_signed(32));
    descriptor.set_for(b"FontFile2", Some(make_reference(&stream, &holder)));
    let mut font = CpdfDictionary::new();
    font.set_new_name_for(b"Type", b"Font");
    font.set_new_name_for(b"Subtype", b"TrueType");
    font.set_new_name_for(b"BaseFont", b"ABCDEF+Ahem");
    font.set_for(b"FontDescriptor", Some(retain_object(descriptor)));
    let font = retain_object(font);
    resources(document)
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(name, Some(font.clone()));
    font
}
#[test]
fn postscript_alias_table_all_styles() {
    for (bold, italic, name) in [
        (false, false, &b"MyriadPro-Light"[..]),
        (true, false, b"MyriadPro-Semibold"),
        (false, true, b"MyriadPro-LightIt"),
        (true, true, b"MyriadPro-SemiboldIt"),
    ] {
        assert_eq!(
            ps_name_to_font_name(b"Myriad Pro Light", bold, italic),
            name
        );
        assert_eq!(
            ps_name_to_font_name(b"Adobe PI Std", bold, italic),
            b"AdobePIStd"
        );
        assert_eq!(ps_name_to_font_name(b"Other", bold, italic), b"Other");
    }
}
#[test]
fn strict_family_style_suffix_and_loose_matching() {
    for (family, bold, italic, resource, expected) in [
        (&b"Arial"[..], false, false, &b"Arial"[..], true),
        (b"Arial", false, false, b"ArialMT", true),
        (b"Arial", false, false, b"ArialRegular", true),
        (b"Arial", true, false, b"Arial-BoldMT", true),
        (b"Arial", false, true, b"Arial-ItalicMT", true),
        (b"Arial", false, true, b"Arial-Oblique", true),
        (b"Arial", true, true, b"Arial-BoldItalic", true),
        (b"Arial", false, false, b"Arial-Bold", false),
        (b"Arial", true, false, b"Arial", false),
        (b"Arial", false, true, b"Arial", false),
        (b"Arial", false, false, b"SubsetArial", false),
        (b"ArialL", false, false, b"ArialLight", true),
        (b"ArialR", false, false, b"ArialRegular", true),
        (b"ArialM", false, false, b"ArialMedium", true),
        (b"", false, false, b"Arial", false),
        (b"Bold", true, false, b"Bold", false),
        (b"Italic", false, true, b"Italic", false),
    ] {
        assert_eq!(
            ps_name_matches_dr_font_name(family, bold, italic, resource, true),
            expected,
            "{} / {}",
            String::from_utf8_lossy(family),
            String::from_utf8_lossy(resource)
        );
    }
    assert!(ps_name_matches_dr_font_name(
        b"Arial",
        false,
        false,
        b"ABCDEF+ArialBold",
        false
    ));
    assert!(!ps_name_matches_dr_font_name(
        b"Arial", false, false, b"arial", false
    ));
}
#[test]
fn embedded_pdf_resource_font_real_glyphs_and_cache_identity() {
    let document = document();
    add_embedded(&document, b"Ahem");
    let manager = CfgasPdfFontMgr::new(&document);
    let font = manager.get_font(&wide("Ahem"), 0, true).unwrap();
    assert!(font.get_glyph_index(u32::from('A')) > 0);
    assert!(font.get_char_width(u32::from('A')).is_some());
    assert!(Rc::ptr_eq(
        &font,
        &manager.get_font(&wide("Ahem"), 0, true).unwrap()
    ));
    let retained = font.get_dev_font().face().unwrap().glyph_count();
    drop(document);
    assert!(retained > 0);
    assert!(Rc::ptr_eq(
        &font,
        &manager.get_font(&wide("Ahem"), 0, true).unwrap()
    ));
    assert!(manager.get_font(&wide("Unknown"), 0, true).is_none());
}
#[test]
fn loose_cache_does_not_weaken_later_strict_lookup_and_misses_are_not_cached() {
    let document = document();
    add_embedded(&document, b"SubsetAhem");
    let manager = CfgasPdfFontMgr::new(&document);
    assert!(manager.get_font(&wide("Ahem"), 0, false).is_some());
    assert!(manager.get_font(&wide("Ahem"), 0, true).is_none());
    add_embedded(&document, b"Ahem");
    assert!(manager.get_font(&wide("Ahem"), 0, true).is_some());
}
#[test]
fn alias_faces_have_real_embedded_font_matching() {
    let document = document();
    for name in [
        &b"MyriadPro-Light"[..],
        b"MyriadPro-Semibold",
        b"MyriadPro-LightIt",
        b"MyriadPro-SemiboldIt",
        b"AdobePIStd",
    ] {
        add_embedded(&document, name);
    }
    let manager = CfgasPdfFontMgr::new(&document);
    for style in [
        0,
        FONT_STYLE_FORCE_BOLD,
        FONT_STYLE_ITALIC,
        FONT_STYLE_FORCE_BOLD | FONT_STYLE_ITALIC,
    ] {
        assert!(manager
            .get_font(&wide("Myriad Pro Light"), style, true)
            .is_some());
        assert!(manager
            .get_font(&wide("Adobe PI Std"), style, true)
            .is_some());
    }
}
#[test]
fn nonembedded_invalid_type_and_missing_resources_are_not_accepted() {
    let document = document();
    let manager = CfgasPdfFontMgr::new(&document);
    assert!(manager.get_font(&wide("Helvetica"), 0, true).is_none());
    let mut font = CpdfDictionary::new();
    font.set_new_name_for(b"Type", b"Font");
    font.set_new_name_for(b"Subtype", b"Type1");
    font.set_new_name_for(b"BaseFont", b"Helvetica");
    resources(&document)
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(b"Helvetica", Some(retain_object(font)));
    assert!(manager.get_font(&wide("Helvetica"), 0, true).is_none());
    let invalid = add_embedded(&document, b"Invalid");
    invalid
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_name_for(b"Type", b"Page");
    assert!(manager.get_font(&wide("Invalid"), 0, true).is_none());
    assert!(manager.load_stock_font(b"Helvetica").is_some());
    assert!(manager
        .load_stock_font(b"UnrelatedAndNotAStockFamily")
        .is_none());
}
#[test]
fn xfa_text_host_uses_real_document_fonts_without_cross_document_cache() {
    use crate::xfa::fxfa::{
        cxfa_fontmgr::{CxfaFontMgr, CxfaTextFontHostAdapter},
        cxfa_textparser::XfaTextFontHost,
    };
    let first = document();
    add_embedded(&first, b"Ahem");
    let second = document();
    add_embedded(&second, b"Ahem");
    let app = Rc::new(CxfaFontMgr::new());
    let pdf_first = Rc::new(CfgasPdfFontMgr::new(&first));
    let pdf_second = Rc::new(CfgasPdfFontMgr::new(&second));
    let host = CxfaTextFontHostAdapter::new(app.clone(), pdf_first.clone());
    let font = host.get_font(&wide("Ahem"), 0).unwrap();
    assert!(Rc::ptr_eq(
        &font,
        &pdf_first.get_font(&wide("Ahem"), 0, true).unwrap()
    ));
    let other = app.get_font(&pdf_second, &wide("Ahem"), 0).unwrap();
    assert!(!Rc::ptr_eq(&font, &other));
}
