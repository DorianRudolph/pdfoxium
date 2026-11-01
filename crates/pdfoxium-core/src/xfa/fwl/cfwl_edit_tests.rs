// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived logical edit-key and selection regression.
use super::*;
use crate::xfa::fwl::test_support::new_app;

fn key(edit: &Rc<CfwlEdit>, code: u32, flags: u32) {
    let widget: WidgetRef = edit.clone();
    let mut message = FwlMessage::key(Some(widget), KeyCommand::KeyDown, flags, code);
    edit.on_process_message(&mut message);
}

#[test]
fn navigation_clears_unshifted_selection_and_keeps_the_opposite_shift_anchor() {
    let (app, _host) = new_app();
    let edit = CfwlEdit::new(
        &app,
        Properties {
            style_exts: FWL_STYLEEXT_EDT_MULTI_LINE,
            ..Default::default()
        },
        None,
    );
    edit.base()
        .set_widget_rect(CfxRectF::new(0., 0., 180., 72.));
    let text: Vec<u32> = "ab\ncdef\nZ".chars().map(u32::from).collect();
    edit.set_text(&text);
    edit.update();
    edit.set_cursor_position(7);
    edit.set_selection(3, 4);
    key(&edit, VKEY_HOME, KEY_FLAG_CTRL);
    assert!(!edit.has_selection());
    key(&edit, VKEY_END, KEY_FLAG_CTRL | KEY_FLAG_SHIFT);
    assert_eq!(edit.get_selection(), (0, text.len()));
    key(&edit, VKEY_HOME, KEY_FLAG_CTRL | KEY_FLAG_SHIFT);
    assert!(!edit.has_selection());

    key(&edit, VKEY_END, KEY_FLAG_CTRL);
    key(&edit, VKEY_LEFT, KEY_FLAG_SHIFT);
    assert_eq!(edit.get_selection(), (8, 1));
    key(&edit, VKEY_LEFT, KEY_FLAG_SHIFT);
    assert_eq!(edit.get_selection(), (7, 2));
    key(&edit, VKEY_RIGHT, KEY_FLAG_SHIFT);
    assert_eq!(edit.get_selection(), (8, 1));
    key(&edit, VKEY_RIGHT, KEY_FLAG_SHIFT);
    assert!(!edit.has_selection());
    assert_eq!(edit.get_text(), text);
}

#[test]
fn selected_deletion_collapses_both_caret_directions_before_following_insertion() {
    for cursor in [1, 3] {
        for backspace in [false, true] {
            let (app, _host) = new_app();
            let edit = CfwlEdit::new(&app, Properties::default(), None);
            edit.base()
                .set_widget_rect(CfxRectF::new(0., 0., 180., 36.));
            let wide = |text: &str| text.chars().map(u32::from).collect::<Vec<_>>();
            edit.set_text(&wide("ABCDE"));
            edit.update();
            edit.set_cursor_position(cursor);
            edit.set_selection(1, 2);
            if backspace {
                let widget: WidgetRef = edit.clone();
                let mut message = FwlMessage::key(Some(widget), KeyCommand::Char, 0, 8);
                edit.on_process_message(&mut message);
            } else {
                key(&edit, VKEY_DELETE, 0);
            }
            assert_eq!(edit.get_text(), wide("ADE"));
            assert!(!edit.has_selection());
            assert_eq!(
                edit.cursor_position(),
                1,
                "cursor={cursor}, backspace={backspace}"
            );
            let widget: WidgetRef = edit.clone();
            let mut message = FwlMessage::key(Some(widget), KeyCommand::Char, 0, u32::from('Z'));
            edit.on_process_message(&mut message);
            assert_eq!(edit.get_text(), wide("AZDE"));
            assert_eq!(edit.cursor_position(), 2);
            assert!(edit.undo());
            assert_eq!(edit.get_text(), wide("ADE"));
            assert!(edit.undo());
            assert_eq!(edit.get_text(), wide("ABCDE"));
            assert!(edit.redo());
            assert_eq!(edit.get_text(), wide("ADE"));
            assert!(edit.redo());
            assert_eq!(edit.get_text(), wide("AZDE"));
        }
    }
}

#[test]
fn plain_mouse_drag_keeps_its_anchor_when_reversing_across_the_press() {
    let (app, _) = new_app();
    let edit = CfwlEdit::new(&app, Properties::default(), None);
    edit.base()
        .set_widget_rect(CfxRectF::new(0., 0., 180., 36.));
    edit.set_text(&"ABCDEFGHIJ".chars().map(u32::from).collect::<Vec<_>>());
    edit.update();
    let point = |index| {
        let (_, rect) = edit.engine.borrow_mut().get_character_info(index).unwrap();
        let engine = edit.engine_rect.get();
        CfxPointF::new(
            rect.left + engine.left + 0.1,
            rect.top + engine.top + rect.height / 2. + edit.valign_offset.get(),
        )
    };
    let a = point(4);
    let right = point(8);
    let left = point(1);
    edit.left_down(a, 0);
    assert_eq!(edit.cursor_position(), 4);
    edit.mouse_move(right, KEY_FLAG_LBUTTON);
    assert_eq!(edit.get_selection(), (4, 4));
    edit.mouse_move(left, KEY_FLAG_LBUTTON);
    assert_eq!(edit.get_selection(), (1, 3));
    edit.mouse_move(a, KEY_FLAG_LBUTTON);
    assert!(!edit.has_selection());
}
