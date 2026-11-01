// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived CFDE_TextEditEngine tests. Bundled Helvetica Bold replaces
//! the native fixture's host-installed Arimo Bold. Its H/e/! advances match;
//! the i advance is 278 instead of 277, explicitly reflected in its rectangles.

use super::cfde_texteditengine::{
    CfdeTextEditEngine, FdeTextEditPiece, RecordOperation, TextChange, TextEditDelegate,
};
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fxcrt::fx_coordinates::CfxPointF;
use crate::xfa::fgas::font::cfgas_gefont::CfgasGeFont;
use std::{cell::Cell, rc::Rc};

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}

fn engine() -> CfdeTextEditEngine {
    // This fixture's numeric expectations use bundled Helvetica Bold metrics.
    // A global stock-font lookup can substitute a host font on macOS.
    let font = CpdfDocument::new()
        .page_data_ref()
        .get_stock_font(
            b"Helvetica-Bold",
            &mut crate::fxge::cfx_fontmapper::CfxFontMapper::new(),
        )
        .unwrap();
    let font = CfgasGeFont::from_pdf_font(crate::fpdfapi::font::cpdf_font::CpdfFont::Simple(font))
        .unwrap();
    let mut engine = CfdeTextEditEngine::new();
    engine.set_font(Some(font));
    engine.set_font_size(12.);
    engine
}

fn insert(engine: &mut CfdeTextEditEngine, at: usize, text: &str) {
    engine.insert(at, &wide(text), RecordOperation::InsertRecord);
}

fn text(engine: &CfdeTextEditEngine, expected: &str) {
    assert_eq!(engine.get_text(), wide(expected));
}

fn selected(engine: &CfdeTextEditEngine, expected: &str) {
    assert_eq!(engine.get_selected_text(), wide(expected));
}

#[derive(Default)]
struct Delegate {
    fail_validation: Cell<bool>,
    text_is_full: Cell<bool>,
}
impl Delegate {
    fn reset(&self) {
        self.fail_validation.set(false);
        self.text_is_full.set(false);
    }
}
impl TextEditDelegate for Delegate {
    fn notify_text_full(&self, _engine: &mut CfdeTextEditEngine) {
        self.text_is_full.set(true);
    }
    fn on_validate(&self, _engine: &mut CfdeTextEditEngine, _text: &[u32]) -> bool {
        !self.fail_validation.get()
    }
}

#[test]
fn insert_native() {
    let mut e = engine();
    text(&e, "");
    insert(&mut e, 0, "");
    text(&e, "");
    assert_eq!(e.get_length(), 0);
    insert(&mut e, 0, "Hello");
    text(&e, "Hello");
    assert_eq!(e.get_length(), 5);
    insert(&mut e, 5, " World");
    text(&e, "Hello World");
    assert_eq!(e.get_length(), 11);
    insert(&mut e, 5, " New");
    text(&e, "Hello New World");
    insert(&mut e, 100, " Cat");
    text(&e, "Hello New World Cat");
    e.clear();
    e.set_has_character_limit(true);
    e.set_character_limit(5);
    insert(&mut e, 0, "Hello");
    insert(&mut e, 5, " World");
    text(&e, "Hello");
    e.set_character_limit(8);
    insert(&mut e, 5, " World");
    text(&e, "Hello Wo");
    e.clear();
    let delegate = Rc::new(Delegate::default());
    e.set_delegate(Some(delegate.clone()));
    e.set_character_limit(5);
    insert(&mut e, 0, "Hello");
    insert(&mut e, 5, " World");
    assert!(delegate.text_is_full.get());
    text(&e, "Hello");
    delegate.reset();
    e.set_character_limit(8);
    insert(&mut e, 5, " World");
    assert!(delegate.text_is_full.get());
    text(&e, "Hello Wo");
    delegate.reset();
    e.set_has_character_limit(false);
    e.clear();
    insert(&mut e, 0, "Hello");
    delegate.fail_validation.set(true);
    e.enable_validation(true);
    insert(&mut e, 5, " World");
    text(&e, "Hello");
    delegate.fail_validation.set(false);
    insert(&mut e, 5, " World");
    text(&e, "Hello World");
    e.enable_validation(false);
    e.clear();
    insert(&mut e, 0, "Hello\nWorld");
    assert!(!delegate.text_is_full.get());
    text(&e, "Hello\nWorld");
    delegate.reset();
    e.clear();
    e.limit_horizontal_scroll(true);
    e.set_available_width(52); // Fits "Hello Wo", as in the native case.
    insert(&mut e, 0, "Hello");
    assert!(!delegate.text_is_full.get());
    insert(&mut e, 5, " World");
    assert!(delegate.text_is_full.get());
    text(&e, "Hello Wo");
    e.limit_horizontal_scroll(false);
    delegate.reset();
    e.clear();
    e.set_line_space(12.);
    e.limit_vertical_scroll(true);
    insert(&mut e, 0, "Hello");
    assert!(!delegate.text_is_full.get());
    insert(&mut e, 5, " Wo\nrld");
    assert!(delegate.text_is_full.get());
    text(&e, "Hello Wo\n");
    e.limit_vertical_scroll(false);
    e.set_delegate(None);
}

#[test]
fn insert_toggle_limit() {
    let mut e = engine();
    e.set_has_character_limit(true);
    insert(&mut e, 0, "Hello World");
    e.set_character_limit(5);
    insert(&mut e, 0, "Not Inserted before ");
    text(&e, "Hello World");
    e.set_has_character_limit(false);
    insert(&mut e, 0, "Inserted before ");
    e.set_has_character_limit(true);
    insert(&mut e, 0, "Not Inserted before ");
    text(&e, "Inserted before Hello World");
}

#[test]
fn insert_skip_notify() {
    let mut e = engine();
    e.set_has_character_limit(true);
    e.set_character_limit(8);
    insert(&mut e, 0, "Hello");
    e.insert(5, &wide(" World"), RecordOperation::SkipNotify);
    text(&e, "Hello World");
    insert(&mut e, 0, "Not inserted");
    text(&e, "Hello World");
    e.delete(5, 1, RecordOperation::InsertRecord);
    text(&e, "HelloWorld");
    insert(&mut e, 0, "****");
    text(&e, "*HelloWorld");
}

#[test]
fn insert_grow_gap() {
    let mut e = engine();
    insert(&mut e, 0, "||");
    for i in 1..1023 {
        insert(&mut e, i, "a");
    }
    let result = e.get_text();
    assert_eq!(result.len(), 1024);
    assert_eq!(result[0], u32::from('|'));
    assert_eq!(result[1], u32::from('a'));
    assert_eq!(result[2], u32::from('a'));
    assert_eq!(result[1022], u32::from('a'));
    assert_eq!(result[1023], u32::from('|'));
}

#[test]
fn delete_native() {
    let mut e = engine();
    assert_eq!(e.delete(0, 50, RecordOperation::InsertRecord), wide(""));
    text(&e, "");
    insert(&mut e, 0, "Hello World");
    assert_eq!(
        e.delete(5, 6, RecordOperation::InsertRecord),
        wide(" World")
    );
    text(&e, "Hello");
    e.clear();
    insert(&mut e, 0, "Hello World");
    assert_eq!(e.delete(5, 1, RecordOperation::InsertRecord), wide(" "));
    text(&e, "HelloWorld");
    assert_eq!(
        e.delete(1, 50, RecordOperation::InsertRecord),
        wide("elloWorld")
    );
    text(&e, "H");
}

#[test]
fn clear_native() {
    let mut e = engine();
    text(&e, "");
    e.clear();
    text(&e, "");
    insert(&mut e, 0, "Hello World");
    text(&e, "Hello World");
    e.clear();
    text(&e, "");
    assert_eq!(e.get_length(), 0);
}

#[test]
fn get_char() {
    let mut e = engine();
    assert_eq!(e.get_char(0), 0);
    insert(&mut e, 0, "Hello World");
    assert_eq!(e.get_char(0), u32::from('H'));
    assert_eq!(e.get_char(e.get_length() - 1), u32::from('d'));
    assert_eq!(e.get_char(5), u32::from(' '));
    insert(&mut e, 5, " A");
    text(&e, "Hello A World");
    assert_eq!(e.get_char(8), u32::from('W'));
    e.enable_password_mode(true);
    assert_eq!(e.get_char(8), u32::from('*'));
    e.set_alias_char(u32::from('+'));
    assert_eq!(e.get_char(8), u32::from('+'));
}

#[test]
fn get_width_of_char() {
    let mut e = engine();
    assert_eq!(e.get_width_of_char(0), 0);
    insert(&mut e, 0, "Hello World");
    assert_eq!(e.get_width_of_char(0), 173280);
    assert_eq!(e.get_width_of_char(1), 133440);
    insert(&mut e, 0, "\t");
    assert_eq!(e.get_width_of_char(0), 0);
}

#[test]
fn get_display_pos() {
    let mut e = engine();
    assert!(e.get_display_pos(&FdeTextEditPiece::default()).is_empty());
}

#[test]
fn selection() {
    let mut e = engine();
    assert!(!e.has_selection());
    e.select_all();
    assert!(!e.has_selection());
    insert(&mut e, 0, "Hello World");
    assert_eq!(
        e.delete_selected_text(RecordOperation::InsertRecord),
        wide("")
    );
    assert!(!e.has_selection());
    e.select_all();
    assert!(e.has_selection());
    selected(&e, "Hello World");
    e.clear_selection();
    assert!(!e.has_selection());
    selected(&e, "");
    e.select_all();
    assert_eq!(e.get_selection(), (0, 11));
    selected(&e, "Hello World");
    assert!(e.has_selection());
    text(&e, "Hello World");
    insert(&mut e, 5, " A");
    assert!(!e.has_selection());
    selected(&e, "");
    e.select_all();
    assert!(e.has_selection());
    selected(&e, "Hello A World");
    e.clear();
    insert(&mut e, 0, "Hello World");
    e.select_all();
    assert_eq!(
        e.delete_selected_text(RecordOperation::InsertRecord),
        wide("Hello World")
    );
    assert!(!e.has_selection());
    text(&e, "");
    insert(&mut e, 0, "Hello World");
    e.set_selection(5, 5);
    assert_eq!(
        e.delete_selected_text(RecordOperation::InsertRecord),
        wide(" Worl")
    );
    assert!(!e.has_selection());
    text(&e, "Hellod");
    e.clear();
    insert(&mut e, 0, "Hello World");
    e.select_all();
    e.replace_selected_text(&wide("Goodbye Everybody"));
    assert!(!e.has_selection());
    text(&e, "Goodbye Everybody");
    e.clear();
    insert(&mut e, 0, "Hello World");
    e.set_selection(1, 4);
    e.replace_selected_text(&wide("i,"));
    assert!(!e.has_selection());
    text(&e, "Hi, World");
    e.clear();
    insert(&mut e, 0, "Hello");
    insert(&mut e, 0, "A ");
    e.set_selection(3, 6);
    selected(&e, "ello");
    e.clear();
    insert(&mut e, 0, "Hello World");
    e.clear_selection();
    e.delete_selected_text(RecordOperation::InsertRecord);
    text(&e, "Hello World");
}

#[test]
fn undo_redo() {
    let mut e = engine();
    assert!(!e.can_undo());
    assert!(!e.can_redo());
    assert!(!e.undo());
    assert!(!e.redo());
    insert(&mut e, 0, "Hello");
    assert!(e.can_undo());
    assert!(!e.can_redo());
    assert!(e.undo());
    text(&e, "");
    assert!(!e.can_undo());
    assert!(e.can_redo());
    assert!(e.redo());
    text(&e, "Hello");
    assert!(e.can_undo());
    assert!(!e.can_redo());
    e.clear();
    assert!(!e.can_undo());
    assert!(!e.can_redo());
    insert(&mut e, 0, "Hello World");
    e.select_all();
    e.delete_selected_text(RecordOperation::InsertRecord);
    text(&e, "");
    assert!(e.can_undo());
    assert!(e.undo());
    text(&e, "Hello World");
    assert!(e.can_redo());
    assert!(e.redo());
    text(&e, "");
    assert!(e.can_undo());
    assert!(!e.can_redo());
    insert(&mut e, 0, "Hello World");
    e.select_all();
    e.replace_selected_text(&wide("Goodbye Friend"));
    text(&e, "Goodbye Friend");
    assert!(e.can_undo());
    assert!(e.undo());
    text(&e, "Hello World");
    assert!(e.can_redo());
    assert!(e.redo());
    text(&e, "Goodbye Friend");
    e.clear();
    e.set_max_edit_operations_for_testing(3);
    insert(&mut e, 0, "First ");
    let at = e.get_length();
    insert(&mut e, at, "Second ");
    let at = e.get_length();
    insert(&mut e, at, "Third");
    assert!(e.can_undo());
    assert!(e.undo());
    text(&e, "First Second ");
    assert!(e.can_undo());
    assert!(e.undo());
    assert!(!e.can_undo());
    text(&e, "First ");
    assert!(e.can_redo());
    assert!(e.redo());
    assert!(e.can_redo());
    assert!(e.redo());
    assert!(!e.can_redo());
    text(&e, "First Second Third");
    e.clear();
    e.set_max_edit_operations_for_testing(4);
    for (at, s) in [
        (0, "H"),
        (1, "e"),
        (2, "l"),
        (3, "l"),
        (4, "o"),
        (5, " World"),
    ] {
        insert(&mut e, at, s);
    }
    text(&e, "Hello World");
    e.delete(4, 3, RecordOperation::InsertRecord);
    text(&e, "Hellorld");
    assert!(e.undo());
    text(&e, "Hello World");
    e.delete(5, 6, RecordOperation::InsertRecord);
    text(&e, "Hello");
    assert!(e.undo());
    text(&e, "Hello World");
    assert!(e.redo());
    text(&e, "Hello");
    assert!(e.undo());
    text(&e, "Hello World");
    assert!(e.undo());
    text(&e, "Hello");
    assert!(e.undo());
    text(&e, "Hell");
    assert!(!e.undo());
    text(&e, "Hell");
}

#[test]
fn get_index_for_point() {
    let mut e = engine();
    e.set_font_size(10.);
    insert(&mut e, 0, "Hello World");
    for (x, y, expected) in [
        (0., 0., 0),
        (999999., 9999999., 11),
        (999999., 0., 11),
        (5., 5., 1),
        (10., 5., 2),
    ] {
        assert_eq!(e.get_index_for_point(CfxPointF::new(x, y)), expected);
    }
}

#[test]
fn get_index_for_point_line_wrap() {
    let mut e = engine();
    e.set_font_size(10.);
    e.enable_multi_line(true);
    e.enable_line_wrap(true);
    e.set_available_width(100);
    insert(
        &mut e,
        0,
        "A text long enough to span multiple lines and test getting indexes on multi-line edits.",
    );
    for (x, y, expected) in [
        (0., 0., 0),
        (999999., 9999999., 87),
        (999999., 0., 18),
        (1., 10., 19),
        (5., 5., 1),
        (10., 5., 2),
    ] {
        assert_eq!(
            e.get_index_for_point(CfxPointF::new(x, y)),
            expected,
            "point=({x},{y})"
        );
    }
}

#[test]
fn get_index_for_point_space_at_end() {
    let mut e = engine();
    e.set_font_size(10.);
    insert(&mut e, 0, "Hello World ");
    for (x, y, expected) in [(0., 0., 0), (999999., 9999999., 12), (999999., 0., 12)] {
        assert_eq!(e.get_index_for_point(CfxPointF::new(x, y)), expected);
    }
}

#[test]
fn get_index_for_point_line_breaks() {
    let mut e = engine();
    e.set_font_size(10.);
    e.enable_multi_line(true);
    insert(&mut e, 0, "Hello\nWorld");
    for (x, y, expected) in [
        (0., 0., 0),
        (999999., 0., 5),
        (0., 10., 6),
        (999999., 9999999., 11),
    ] {
        assert_eq!(e.get_index_for_point(CfxPointF::new(x, y)), expected);
    }
}

#[test]
fn can_generate_character_info() {
    let mut e = engine();
    assert!(e.get_font().is_some());
    assert!(!e.can_generate_character_info());
    insert(&mut e, 0, "Hi!");
    assert!(e.can_generate_character_info());
    e.set_font(None);
    assert!(!e.can_generate_character_info());
    e.clear();
    assert!(!e.can_generate_character_info());
}

#[test]
fn get_character_info() {
    let mut e = engine();
    insert(&mut e, 0, "Hi!");
    assert_eq!(e.get_length(), 3);
    // Source Arimo Bold i=277: left/width pairs were (0,8.664),
    // (8.664,3.324), (11.988,3.996), (15.984,0). Bundled Helvetica Bold
    // i=278 adds 0.012 to i's width and each subsequent left edge.
    for (index, left, width) in [
        (0, 0., 8.664),
        (1, 8.664, 3.336),
        (2, 12., 3.996),
        (3, 15.996, 0.),
    ] {
        let (level, rect) = e.get_character_info(index).unwrap();
        assert_eq!(level, 0);
        assert!(rect.left.to_bits().abs_diff((left as f32).to_bits()) <= 4);
        assert_eq!(rect.top, 0.);
        assert!(rect.width.to_bits().abs_diff((width as f32).to_bits()) <= 4);
        assert_eq!(rect.height, 12.);
    }
}

fn select_word(
    e: &mut CfdeTextEditEngine,
    index: usize,
    expected_bounds: Option<(usize, usize)>,
    expected: &str,
) {
    let bounds = e.bounds_for_word_at(index);
    if let Some(expected_bounds) = expected_bounds {
        assert_eq!(bounds, expected_bounds);
    }
    e.set_selection(bounds.0, bounds.1);
    selected(e, expected);
}

#[test]
fn bounds_for_word_at() {
    let mut e = engine();
    select_word(&mut e, 100, Some((0, 0)), "");
    e.clear();
    insert(&mut e, 0, "Hello");
    select_word(&mut e, 0, Some((0, 5)), "Hello");
    e.clear();
    insert(&mut e, 0, "Hello World");
    select_word(&mut e, 100, Some((0, 0)), "");
    for index in [0, 1, 4] {
        select_word(&mut e, index, Some((0, 5)), "Hello");
    }
    select_word(&mut e, 5, Some((5, 1)), " ");
    select_word(&mut e, 6, Some((6, 5)), "World");
    for (input, index, expected) in [
        ("123 456 789", 5, "456"),
        ("123def789", 5, "123def789"),
        ("abc456ghi", 5, "abc456ghi"),
        ("hello, world", 0, "hello"),
        ("hello, world", 5, ","),
        ("np-complete", 6, "complete"),
    ] {
        e.clear();
        insert(&mut e, 0, input);
        select_word(&mut e, index, None, expected);
    }
    e.clear();
    insert(&mut e, 0, "(123) 456-7890");
    for (index, expected) in [(0, "("), (1, "123"), (7, "456"), (11, "7890")] {
        select_word(&mut e, index, None, expected);
    }
    let cases: &[(&str, &[&str])] = &[
        ("AAA", &["AAA"]),
        ("A:A", &["A:A"]),
        ("A::A", &["A", ":", ":", "A"]),
        ("A00A", &["A00A"]),
        ("0,0", &["0,0"]),
        ("0,,0", &["0", ",", ",", "0"]),
        ("\u{3031}\u{3031}", &["\u{3031}\u{3031}"]),
        ("A_0_\u{3031}_", &["A_0_\u{3031}_"]),
        ("A__A", &["A__A"]),
    ];
    for &(input, words) in cases {
        e.clear();
        insert(&mut e, 0, input);
        let mut index = 0;
        for word in words {
            let bounds = e.bounds_for_word_at(index);
            e.set_selection(bounds.0, bounds.1);
            assert_eq!(e.get_selected_text(), wide(word), "input={input:?}");
            index += bounds.1;
        }
    }
}

#[test]
fn cursor_movement() {
    let mut e = engine();
    e.clear();
    insert(&mut e, 0, "Hello");
    assert_eq!(e.get_index_left(0), 0);
    assert_eq!(e.get_index_right(5), 5);
    assert_eq!(e.get_index_up(2), 2);
    assert_eq!(e.get_index_down(2), 2);
    assert_eq!(e.get_index_left(2), 1);
    assert_eq!(e.get_index_right(2), 3);
    assert_eq!(e.get_index_at_start_of_line(2), 0);
    assert_eq!(e.get_index_at_end_of_line(2), 5);
    e.clear();
    insert(&mut e, 0, "The book is \"مدخل إلى C++\"");
    assert_eq!(e.get_character_info(3).unwrap().0 & 1, 0);
    assert_eq!(e.get_index_left(3), 2);
    assert_eq!(e.get_index_right(3), 4);
    assert_eq!(e.get_character_info(15).unwrap().0 & 1, 1);
    assert_eq!(e.get_index_left(15), 14);
    assert_eq!(e.get_index_right(15), 16);
    assert_eq!(e.get_character_info(23).unwrap().0 & 1, 0);
    assert_eq!(e.get_index_left(23), 22);
    assert_eq!(e.get_index_right(23), 24);
    for (input, world, test, line_break) in [
        ("Hello\r\nWorld\r\nTest", 7, 14, "\r\n"),
        ("Hello\rWorld\rTest", 6, 12, "\r"),
        ("Hello\nWorld\nTest", 6, 12, "\n"),
    ] {
        e.clear();
        insert(&mut e, 0, input);
        e.set_selection(e.get_index_left(world), line_break.chars().count() + 5);
        selected(&e, &format!("{line_break}World"));
        e.set_selection(e.get_index_up(world + 1), 2);
        selected(&e, "el");
        e.set_selection(e.get_index_up(test + 1), 2);
        selected(&e, "or");
        e.set_selection(e.get_index_down(1), 2);
        selected(&e, "or");
        e.set_selection(e.get_index_down(world + 1), 2);
        selected(&e, "es");
        let start = e.get_index_at_start_of_line(world + 1);
        let end = e.get_index_at_end_of_line(world + 1);
        e.set_selection(start, end - start);
        selected(&e, "World");
        e.set_selection(e.get_index_right(5), 5);
        selected(&e, "World");
    }
    e.clear();
    insert(&mut e, 0, "Short\nAnd a very long line");
    e.set_selection(e.get_index_up(14), 11);
    selected(&e, "\nAnd a very");
    e.clear();
    insert(&mut e, 0, "A Very long line\nShort");
    assert_eq!(e.get_index_down(8), e.get_length());
}

#[test]
fn logical_caret_navigation_keeps_line_end_columns_and_empty_lines() {
    let mut e = engine();
    for separator in ["\n", "\r", "\r\n"] {
        e.clear();
        insert(&mut e, 0, &format!("ab{separator}cdef{separator}Z"));
        let middle = 2 + separator.len();
        let middle_end = middle + 4;
        let last = middle_end + separator.len();
        assert_eq!(e.get_index_left(last + 1), last);
        assert_eq!(e.get_index_right(1), 2);
        assert_eq!(e.get_index_right(2), middle);
        assert_eq!(e.get_index_left(middle), 2);
        assert_eq!(e.get_index_at_start_of_line(middle_end), middle);
        assert_eq!(e.get_index_at_end_of_line(middle), middle_end);
        assert_eq!(e.get_index_down(2), middle + 2);
        assert_eq!(e.get_index_up(last + 1), middle + 1);
        assert_eq!(e.get_index_down(middle_end), last + 1);
        if separator == "\r\n" {
            assert_eq!(e.get_index_at_start_of_line(3), 0);
            assert_eq!(e.get_index_at_end_of_line(3), 2);
        }
        e.clear();
        insert(&mut e, 0, &format!("A{separator}{separator}B"));
        let empty = 1 + separator.len();
        let last = empty + separator.len();
        assert_eq!(e.get_index_up(last), empty);
        assert_eq!(e.get_index_down(1), empty);
        assert_eq!(e.get_index_down(empty), last);
        assert_eq!(e.get_index_left(last), empty);
    }
}

#[test]
fn default_single_line_and_comb_keep_all_characters_on_the_same_visual_row() {
    let mut e = engine();
    e.set_available_width(48);
    insert(&mut e, 0, "ABCDEFGHIJKLMNO");
    assert!(e.get_text_pieces().iter().all(|piece| piece.rect.top == 0.));
    assert!(e.get_contents_bounding_box().width > 48.);
    e.enable_multi_line(true);
    e.enable_line_wrap(true);
    assert!(e.get_contents_bounding_box().height > e.get_line_space());
    e.enable_multi_line(false);
    e.enable_line_wrap(false);
    assert!(e.get_text_pieces().iter().all(|piece| piece.rect.top == 0.));

    e.clear();
    e.set_available_width(180);
    e.set_has_character_limit(true);
    e.set_character_limit(6);
    e.set_comb_text(true);
    insert(&mut e, 0, "ABCDEF");
    for index in 0..6 {
        let (_, rect) = e.get_character_info(index).unwrap();
        assert_eq!(rect.top, 0.);
        assert_eq!(rect.left, index as f32 * 30.);
        assert_eq!(rect.width, 30.);
    }
}

#[test]
fn delegate_cancellation_and_mutation() {
    struct ChangeDelegate;
    impl TextEditDelegate for ChangeDelegate {
        fn on_text_will_change(&self, engine: &mut CfdeTextEditEngine, change: &mut TextChange) {
            assert_eq!(change.previous_text, engine.get_text());
            if change.text == wide("cancel") {
                change.cancelled = true;
            } else if change.text == wide("rewrite") {
                change.text = wide("changed");
            }
        }
    }
    let mut e = engine();
    e.set_delegate(Some(Rc::new(ChangeDelegate)));
    insert(&mut e, 0, "cancel");
    text(&e, "");
    assert!(!e.can_undo());
    insert(&mut e, 0, "rewrite");
    text(&e, "changed");
    assert!(e.undo());
    text(&e, "");
    assert!(e.redo());
    text(&e, "changed");
}

#[test]
fn truncated_insert_history_records_committed_text() {
    let mut e = engine();
    e.set_has_character_limit(true);
    e.set_character_limit(8);
    insert(&mut e, 0, "Hello");
    insert(&mut e, 5, " World");
    text(&e, "Hello Wo");
    assert!(e.undo());
    text(&e, "Hello");
    assert!(e.redo());
    text(&e, "Hello Wo");
}

#[test]
fn clear_and_character_range_layout() {
    let mut e = engine();
    insert(&mut e, 0, "Hello");
    let character = e.get_character_info(1).unwrap().1;
    assert!(character.width > 0.);
    assert_eq!(e.get_character_rects_in_range(1, 1), vec![character]);
    assert!(!e.get_text_pieces().is_empty());
    e.clear();
    assert!(e.get_text_pieces().is_empty());
    let bounds = e.get_contents_bounding_box();
    assert_eq!(bounds.width, 0.);
    assert_eq!(bounds.height, 0.);
    assert!(!e.can_generate_character_info());
}

#[test]
fn final_directional_pieces_share_height_and_visible_glyph_baseline() {
    for value in [
        "בני",
        "A\u{200e}(12)\u{200f}B",
        "first\nA\u{200e}(12)\u{200f}B",
    ] {
        let mut e = engine();
        e.enable_multi_line(true);
        e.set_line_space(18.);
        e.set_available_width(200);
        insert(&mut e, 0, value);
        let pieces = e.get_text_pieces().to_vec();
        let final_top = pieces.last().unwrap().rect.top;
        let final_pieces: Vec<_> = pieces
            .iter()
            .filter(|piece| piece.rect.top == final_top)
            .collect();
        assert!(
            final_pieces.len() > 1,
            "directional text and end sentinel must form separate pieces"
        );
        for piece in &final_pieces {
            assert_eq!(piece.rect.height, 12.);
        }
        let bounds = e.get_contents_bounding_box();
        assert_eq!(bounds.bottom(), final_top + 12.);
        if value.contains('A') {
            let ys: Vec<_> = final_pieces
                .iter()
                .flat_map(|piece| e.get_display_pos(piece))
                .map(|position| position.origin.y)
                .collect();
            assert!(ys.len() >= 5);
            assert!(ys.iter().all(|y| (*y - ys[0]).abs() < 0.001));
        }
    }
}

#[test]
fn delegate_can_reenter_the_engine() {
    struct Reenter {
        entered: Cell<bool>,
    }
    impl TextEditDelegate for Reenter {
        fn on_text_changed(&self, engine: &mut CfdeTextEditEngine) {
            if !self.entered.replace(true) {
                engine.insert(
                    engine.get_length(),
                    &wide(" nested"),
                    RecordOperation::SkipRecord,
                );
            }
        }
    }
    let mut e = engine();
    let delegate = Rc::new(Reenter {
        entered: Cell::new(false),
    });
    e.set_delegate(Some(delegate.clone()));
    insert(&mut e, 0, "outer");
    assert!(delegate.entered.get());
    text(&e, "outer nested");
}
