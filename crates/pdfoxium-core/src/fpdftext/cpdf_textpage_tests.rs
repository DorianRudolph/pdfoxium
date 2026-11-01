// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Connected CPDF_TextPage fixtures using real stock-font page objects.
use super::*;
use crate::fpdfapi::{
    page::{cpdf_docpagedata::CpdfDocPageData, cpdf_form::CpdfForm},
    parser::{
        cpdf_dictionary::CpdfDictionary, cpdf_object::retain_object, cpdf_stream::CpdfStream,
        cpdf_string::CpdfString,
    },
};
use crate::fxge::cfx_fontmapper::CfxFontMapper;
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn make_page() -> CpdfPage {
    let mut dictionary = CpdfDictionary::new();
    dictionary.set_new_name_for(b"Type", b"Page");
    dictionary.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 400.0, 400.0));
    CpdfPage::new(retain_object(dictionary)).unwrap()
}
fn font() -> CpdfFont {
    CpdfFont::Simple(
        CpdfDocPageData::new()
            .get_stock_font(b"Helvetica", &mut CfxFontMapper::new())
            .unwrap(),
    )
}
fn text(font: CpdfFont, value: &[u8], x: f32, y: f32) -> CpdfTextObject {
    let mut object = CpdfTextObject::new(0);
    let dictionary = font.font_dict();
    object
        .base_mut()
        .mutable_text_state()
        .set_font(Some(font), dictionary);
    object.base_mut().mutable_text_state().set_font_size(12.0);
    object.base_mut().mutable_color_state().set_default();
    object.set_text(value);
    object.set_text_matrix(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, x, y));
    object
}
fn append(page: &mut CpdfPage, object: CpdfTextObject) -> PageObjectId {
    let index = page.holder().page_object_count();
    page.mutable_holder().append_page_object(Box::new(object));
    page.holder().page_object_id_by_index(index).unwrap()
}
fn actual(
    object: &mut CpdfTextObject,
    value: &[u32],
) -> crate::fpdfapi::page::cpdf_contentmarks::MarkItemRef {
    let mut dictionary = CpdfDictionary::new();
    dictionary.set_new_for(b"ActualText", CpdfString::from_unicode(None, value));
    let item = object.base_mut().mutable_content_marks().add_mark(b"Span");
    item.borrow_mut().set_direct_dict(retain_object(dictionary));
    item
}
#[test]
fn real_text_order_spaces_linebreaks_ranges_and_object_geometry() {
    let mut page = make_page();
    let font = font();
    append(&mut page, text(font.clone(), b"World", 80.0, 300.0));
    let hello = append(&mut page, text(font.clone(), b"Hello", 20.0, 300.0));
    append(&mut page, text(font, b"Second", 20.0, 270.0));
    let mut extracted = CpdfTextPage::new(&page, false);
    assert_eq!(extracted.get_all_page_text(), wide("Hello World\r\nSecond"));
    assert_eq!(extracted.count_chars(), 19);
    assert_eq!(extracted.get_page_text(1, 3), wide("ell"));
    assert!(extracted.get_page_text(-1, 4).is_empty());
    assert!(extracted.get_page_text(0, 0).is_empty());
    assert!(extracted.get_page_text(i32::MAX, 2).is_empty());
    assert_eq!(extracted.get_page_text(13, i32::MAX), wide("Second"));
    assert_eq!(extracted.get_text_by_object_id(hello), wide("Hello "));
    let info = extracted.get_char_info(0);
    assert_eq!(info.unicode(), 72);
    assert_eq!(info.text_object(), Some(hello));
    assert_eq!(extracted.get_char_font_size(0), 12.0);
    assert!(!info.char_box().is_empty());
    assert!(info.loose_char_box().contains_rect(info.char_box()));
    let center = CfxPointF::new(
        (info.char_box.left + info.char_box.right) / 2.0,
        (info.char_box.bottom + info.char_box.top) / 2.0,
    );
    assert_eq!(
        extracted.get_index_at_pos(center, CfxSizeF::new(0.0, 0.0)),
        0
    );
    assert_eq!(
        extracted.get_index_at_pos(CfxPointF::new(-100.0, -100.0), CfxSizeF::new(1.0, 1.0)),
        -1
    );
    let near = CfxPointF::new(info.char_box.left - 0.25, center.y);
    assert_eq!(extracted.get_index_at_pos(near, CfxSizeF::new(1.0, 0.0)), 0);
    let hello_rect = extracted.get_rect_array(0, 5)[0];
    assert_eq!(extracted.get_text_by_rect(hello_rect), wide("Hello "));
    assert_eq!(extracted.count_rects(0, -1), 3);
    assert!(extracted.get_rect(0).is_some());
    assert!(extracted.get_rect(-1).is_none());
    assert!(extracted.get_rect(3).is_none());
    assert_eq!(extracted.count_rects(-1, 3), -1);
    for index in 0..extracted.count_chars() {
        assert_eq!(extracted.text_index_from_char_index(index), index);
        assert_eq!(extracted.char_index_from_text_index(index), index);
    }
    assert_eq!(extracted.char_index_from_text_index(-1), -1);
    assert_eq!(extracted.char_index_from_text_index(i32::MAX), -1);
}
#[test]
fn inactive_and_duplicate_objects_are_omitted_and_snapshots_survive_page_drop() {
    let mut page = make_page();
    let font = font();
    append(&mut page, text(font.clone(), b"same", 20.0, 300.0));
    append(&mut page, text(font.clone(), b"same", 20.0, 300.0));
    let mut inactive = text(font, b"hidden", 80.0, 300.0);
    inactive.base_mut().set_is_active(false);
    append(&mut page, inactive);
    let extracted = CpdfTextPage::new(&page, false);
    assert_eq!(extracted.get_all_page_text(), wide("same"));
    let id = extracted.get_char_info(0).text_object().unwrap();
    let info = extracted.text_object_info(id).unwrap();
    assert_eq!(info.fill_color, [0, 0, 0, 255]);
    assert_eq!(info.stroke_color, [0, 0, 0, 255]);
    drop(page);
    assert_eq!(extracted.get_all_page_text(), wide("same"));
    assert_eq!(
        extracted
            .get_char_font(0)
            .unwrap()
            .unicode_from_char_code(115),
        wide("s")
    );
    assert!(extracted
        .get_char_loose_bounds(0)
        .contains_rect(extracted.get_char_info(0).char_box()));
}
#[test]
fn kerning_generates_space_and_repeated_spaces_compact_with_aligned_metadata() {
    let mut page = make_page();
    let font = font();
    let mut object = text(font, b"ignored", 20.0, 300.0);
    object.set_segments(&[b"A".to_vec(), b"B  C".to_vec()], &[-600.0]);
    object.calc_position_data(1.0);
    append(&mut page, object);
    let extracted = CpdfTextPage::new(&page, false);
    assert_eq!(extracted.get_all_page_text(), wide("A B C"));
    assert_eq!(extracted.count_chars(), 5);
    assert_eq!(extracted.get_char_info(1).char_type(), CharType::Generated);
    assert_eq!(extracted.get_char_info(1).unicode(), 32);
    assert_eq!(extracted.get_char_info(4).unicode(), 67);
}
#[test]
fn hyphenated_line_join_preserves_source_character_marker_and_indices() {
    let mut page = make_page();
    let font = font();
    append(&mut page, text(font.clone(), b"inter-", 20.0, 300.0));
    append(&mut page, text(font, b"national", 20.0, 270.0));
    let extracted = CpdfTextPage::new(&page, false);
    let mut expected = wide("inter");
    expected.push(0xfffe);
    expected.extend(wide("national"));
    assert_eq!(extracted.get_all_page_text(), expected);
    assert_eq!(extracted.get_char_info(5).char_type(), CharType::Hyphen);
    assert_eq!(extracted.get_char_info(5).unicode(), 2);
    assert_eq!(extracted.get_page_text(6, 8), wide("national"));
}
#[test]
fn actual_text_shared_mark_is_emitted_once_with_per_character_boxes() {
    let mut page = make_page();
    let font = font();
    let mut first = text(font.clone(), b"AB", 20.0, 300.0);
    let mark = actual(&mut first, &wide("replacement"));
    let mut second = text(font, b"CD", 40.0, 300.0);
    second
        .base_mut()
        .mutable_content_marks()
        .add_existing_mark(mark);
    append(&mut page, first);
    append(&mut page, second);
    let extracted = CpdfTextPage::new(&page, false);
    assert_eq!(extracted.get_all_page_text(), wide("replacement"));
    assert_eq!(extracted.count_chars(), 11);
    for index in 0..extracted.size() {
        let info = extracted.get_char_info(index);
        assert_eq!(info.char_type(), CharType::ActualText);
        assert_eq!(info.char_code(), INVALID_CHAR_CODE);
        assert!(info.char_box().width() > 0.0);
    }
}
#[test]
fn actual_text_bidi_ligatures_control_filtering_and_logical_order() {
    let mut page = make_page();
    let font = font();
    let mut object = text(font.clone(), b"ABCD", 20.0, 300.0);
    actual(&mut object, &[0xfb03, 32, 1488, 1489]);
    append(&mut page, object);
    let extracted = CpdfTextPage::new(&page, false);
    assert_eq!(
        extracted.get_all_page_text(),
        vec![102, 102, 105, 32, 1488, 1489]
    );
    assert_eq!(extracted.count_chars(), 6);
    assert_eq!(extracted.get_char_info(0).char_type(), CharType::Piece);
    let mut page = make_page();
    let mut object = text(font, b"ABCDE", 20.0, 300.0);
    actual(&mut object, &[65, 2, 66, 0xfffd, 67]);
    append(&mut page, object);
    let extracted = CpdfTextPage::new(&page, false);
    assert_eq!(extracted.get_all_page_text(), wide("A BC"));
}
#[test]
fn actual_text_keeps_supplementary_unicode_with_and_without_bmp_neighbors() {
    for replacement in [vec![0x1f642], wide("actual Ω🙂")] {
        let mut page = make_page();
        let mut object = text(font(), b"AB", 20.0, 300.0);
        actual(&mut object, &replacement);
        append(&mut page, object);
        let extracted = CpdfTextPage::new(&page, false);
        assert_eq!(extracted.get_all_page_text(), replacement);
        assert_eq!(extracted.count_chars(), replacement.len() as i32);
        for (index, &scalar) in replacement.iter().enumerate() {
            let info = extracted.get_char_info(index);
            assert_eq!(info.unicode(), scalar);
            assert_eq!(info.char_type(), CharType::ActualText);
            assert!(info.char_box().width() > 0.0);
        }
    }
}
#[test]
fn nested_form_matrix_reaches_text_and_bounds_without_cloning_page_objects() {
    let font = font();
    let mut dictionary = CpdfDictionary::new();
    dictionary.set_rect_for(b"BBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
    let stream = retain_object(CpdfStream::from_data_and_dict(
        Vec::new(),
        retain_object(dictionary),
    ));
    let mut form = CpdfForm::new(stream, None, None).unwrap();
    form.holder_mut()
        .append_page_object(Box::new(text(font, b"nested", 10.0, 20.0)));
    let id = form.holder().page_object_id_by_index(0).unwrap();
    let object = CpdfFormObject::new(0, form, CfxMatrix::new(2.0, 0.0, 0.0, 2.0, 30.0, 40.0));
    let mut page = make_page();
    page.mutable_holder().append_page_object(Box::new(object));
    let extracted = CpdfTextPage::new(&page, false);
    assert_eq!(extracted.get_all_page_text(), wide("nested"));
    assert_eq!(extracted.get_char_info(0).text_object(), Some(id));
    assert_eq!(
        extracted.get_char_info(0).origin(),
        CfxPointF::new(50.0, 80.0)
    );
    assert_eq!(extracted.get_text_by_object_id(id), wide("nested"));
}
