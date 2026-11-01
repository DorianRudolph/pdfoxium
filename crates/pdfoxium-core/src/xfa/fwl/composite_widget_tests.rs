// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Direct-widget adaptations of CFWLEditEmbedderTest input scenarios. PDF/XFA
//! loading, page coordinates, public FORM UTF-16 buffers and native image hashes
//! remain with the pending document-host suite. These tests use the actual
//! widget, source theme renderer and embedded stock font through the Rust AGG
//! device, retaining the native text/control sequences and selection results.
use super::{
    cfwl_app::CfwlApp,
    cfwl_edit::{
        CfwlEdit, FWL_STYLEEXT_EDT_AUTO_HSCROLL, FWL_STYLEEXT_EDT_AUTO_VSCROLL,
        FWL_STYLEEXT_EDT_COMB_TEXT, FWL_STYLEEXT_EDT_MULTI_LINE, FWL_STYLEEXT_EDT_PASSWORD,
        FWL_STYLEEXT_EDT_READ_ONLY, FWL_STYLEEXT_EDT_VALIDATE, FWL_STYLEEXT_EDT_WANT_RETURN,
    },
    cfwl_event::{EventData, FwlEvent},
    cfwl_message::{FwlMessage, KeyCommand, MouseCommand},
    cfwl_themepart::{CfwlThemePart, Part},
    cfwl_widget::{FwlWidget, Properties, WidgetRef, WidgetState},
    fwl_widgetdef::{KEY_FLAG_SHIFT, VKEY_LEFT},
    ifwl_widgetdelegate::WidgetDelegate,
    test_support::new_app,
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
    xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
use std::{
    cell::Cell,
    rc::{Rc, Weak},
};

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn edit(styles: u32) -> (Rc<CfwlApp>, Rc<CfwlEdit>) {
    let (app, _) = new_app();
    let control = CfwlEdit::new(
        &app,
        Properties {
            style_exts: styles | FWL_STYLEEXT_EDT_AUTO_HSCROLL,
            ..Default::default()
        },
        None,
    );
    control
        .base()
        .set_widget_rect(CfxRectF::new(0., 0., 300., 50.));
    control.update();
    (app, control)
}
fn char_input(control: &Rc<CfwlEdit>, ch: u32) {
    let mut message = FwlMessage::key(Some(control.clone()), KeyCommand::Char, 0, ch);
    control.on_process_message(&mut message);
}
fn type_text(control: &Rc<CfwlEdit>, text: &str) {
    for ch in text.chars() {
        char_input(control, u32::from(ch));
    }
}
fn key_input(control: &Rc<CfwlEdit>, key: u32, flags: u32) {
    let mut message = FwlMessage::key(Some(control.clone()), KeyCommand::KeyDown, flags, key);
    control.on_process_message(&mut message);
}
fn mouse_input(control: &Rc<CfwlEdit>, command: MouseCommand, flags: u32, point: CfxPointF) {
    let mut message = FwlMessage::mouse(Some(control.clone()), command, flags, point);
    control.on_process_message(&mut message);
}
fn render(control: &Rc<CfwlEdit>) -> Vec<u8> {
    let mut bitmap = CfxDibitmap::create(340, 90, FxdibFormat::Bgra, 0).unwrap();
    bitmap.clear(0xffffffff);
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        control.draw_widget(&mut graphics, CfxMatrix::default());
    }
    bitmap.buffer().to_vec()
}

#[test]
fn native_simple_fill_and_single_line_return_use_actual_widget_drawing() {
    let (_app, control) = edit(0);
    let blank = render(&control);
    // SimpleFill: the ten native character messages 'a' through 'j'.
    type_text(&control, "abcdefghij");
    assert_eq!(control.get_text(), wide("abcdefghij"));
    let filled = render(&control);
    assert_ne!(filled, blank);
    assert!(filled
        .chunks_exact(4)
        .any(|pixel| pixel[..3] != [255, 255, 255]));

    // FillWithNewLineWithoutMultiline: CR is ignored without WantReturn.
    control.clear_text();
    type_text(&control, "abcde");
    char_input(&control, u32::from('\r'));
    type_text(&control, "fghij");
    assert_eq!(control.get_text(), wide("abcdefghij"));
    assert_eq!(render(&control), filled);
}

#[test]
fn native_shift_click_and_drag_select_defgh_with_font_relative_coordinates() {
    let (app, control) = edit(0);
    type_text(&control, "abcdefghij");
    let provider = app.theme_provider();
    let font = provider.get_font(&CfwlThemePart::new(Part::None, control.as_ref()));
    let origin = control.engine_rect();
    // The native PDF coordinates depend on its XFA host/font. Place clicks at
    // equivalent character boundaries in this widget's actual stock font.
    let boundary = |prefix: &str| {
        CfxPointF::new(
            origin.left
                + prefix
                    .chars()
                    .map(|ch| f32::from(font.get_char_width(u32::from(ch)).unwrap()) * 12. / 1000.)
                    .sum::<f32>()
                + 0.1,
            origin.top + 1.,
        )
    };
    let third = boundary("abc");
    let eighth = boundary("abcdefgh");
    mouse_input(&control, MouseCommand::LeftButtonDown, 0, third);
    mouse_input(
        &control,
        MouseCommand::LeftButtonDown,
        KEY_FLAG_SHIFT,
        eighth,
    );
    assert_eq!(control.get_selection(), (3, 5));
    assert_eq!(control.copy(), Some(wide("defgh")));
    mouse_input(&control, MouseCommand::LeftButtonUp, 0, eighth);

    // DragMouseSelection: preserve the source's Shift+Move input sequence.
    mouse_input(&control, MouseCommand::LeftButtonDown, 0, third);
    mouse_input(&control, MouseCommand::Move, KEY_FLAG_SHIFT, eighth);
    assert_eq!(control.get_selection(), (3, 5));
    assert_eq!(control.copy(), Some(wide("defgh")));
    mouse_input(&control, MouseCommand::LeftButtonUp, 0, eighth);
    let mut focus = FwlMessage::set_focus(Some(control.clone()));
    control.on_process_message(&mut focus);
    let selected = render(&control);
    control.clear_selection();
    assert_ne!(selected, render(&control));
}

#[test]
fn native_multiline_return_left_and_backspace_sequence() {
    let (_app, control) = edit(
        FWL_STYLEEXT_EDT_MULTI_LINE | FWL_STYLEEXT_EDT_WANT_RETURN | FWL_STYLEEXT_EDT_AUTO_VSCROLL,
    );
    type_text(&control, "abcde");
    char_input(&control, u32::from('\r'));
    type_text(&control, "fghij");
    assert_eq!(control.get_text(), wide("abcde\nfghij"));
    let two_lines = render(&control);
    for _ in 0..4 {
        key_input(&control, VKEY_LEFT, 0);
    }
    assert_eq!(control.cursor_position(), 7);
    for _ in 0..2 {
        char_input(&control, u32::from('\u{8}'));
    }
    assert_eq!(control.get_text(), wide("abcdeghij"));
    assert_ne!(render(&control), two_lines);
}

#[test]
fn clipboard_history_and_read_only_disabled_input() {
    let (_app, control) = edit(0);
    control.set_text(&wide("Hello World"));
    control.select_all();
    assert_eq!(control.copy(), Some(wide("Hello World")));
    assert_eq!(control.cut(), Some(wide("Hello World")));
    assert_eq!(control.get_text_length(), 0);
    assert!(control.undo());
    assert_eq!(control.get_text(), wide("Hello World"));
    assert!(control.redo());
    assert_eq!(control.get_text_length(), 0);
    assert!(control.paste(&wide("Restored")));
    assert_eq!(control.get_text(), wide("Restored"));
    control
        .base()
        .modify_style_exts(FWL_STYLEEXT_EDT_READ_ONLY, 0);
    control.update();
    char_input(&control, u32::from('!'));
    char_input(&control, u32::from('\u{8}'));
    assert_eq!(control.get_text(), wide("Restored"));
    control
        .base()
        .modify_style_exts(0, FWL_STYLEEXT_EDT_READ_ONLY);
    control.set_states(WidgetState::DISABLED);
    char_input(&control, u32::from('!'));
    assert_eq!(control.get_text(), wide("Restored"));
}

#[test]
fn password_aliases_and_comb_limits_draw_real_outlines() {
    let (_app, password) = edit(FWL_STYLEEXT_EDT_PASSWORD);
    password.set_text(&wide("Secret"));
    let concealed = render(&password);
    let (_other_app, aliases) = edit(0);
    aliases.set_text(&wide("******"));
    assert_eq!(password.get_text(), wide("Secret"));
    assert_eq!(concealed, render(&aliases));
    password.set_alias_char(u32::from('+'));
    aliases.set_text(&wide("++++++"));
    assert_eq!(render(&password), render(&aliases));

    let (_comb_app, comb) = edit(FWL_STYLEEXT_EDT_COMB_TEXT);
    comb.set_limit(5);
    comb.update();
    type_text(&comb, "abcdef");
    assert_eq!(comb.get_text(), wide("abcde"));
    assert_eq!(comb.get_limit(), 5);
    assert!(render(&comb)
        .chunks_exact(4)
        .any(|pixel| pixel[..3] != [255, 255, 255]));
}

struct MutatingDelegate {
    edit: Weak<CfwlEdit>,
    app: Weak<CfwlApp>,
    nested: Cell<bool>,
    full: Cell<usize>,
    validation: Cell<usize>,
}
impl WidgetDelegate for MutatingDelegate {
    fn on_process_message(&self, _message: &mut FwlMessage) {}
    fn on_draw_widget(&self, _graphics: &mut CfgasGeGraphics<'_, '_, '_>, _matrix: CfxMatrix) {}
    fn on_process_event(&self, event: &mut FwlEvent) {
        let Some(edit) = self.edit.upgrade() else {
            return;
        };
        match &mut event.data {
            EventData::TextWillChange {
                change_text,
                previous_text,
                cancelled,
                ..
            } => {
                assert_eq!(*previous_text, edit.get_text());
                if *change_text == wide("cancel") {
                    *cancelled = true;
                } else if *change_text == wide("rewrite") {
                    *change_text = wide("changed");
                } else if *change_text == wide("outer") && !self.nested.replace(true) {
                    assert!(edit.paste(&wide("Nested")));
                    *cancelled = true;
                } else if *change_text == wide("destroy") {
                    self.app.upgrade().unwrap().remove_widget(edit.base().id());
                }
            }
            EventData::Validate { insert, validate } => {
                self.validation.set(self.validation.get() + 1);
                assert!(edit.get_text_length() <= edit.get_text().len());
                if insert.windows(3).any(|part| part == wide("bad")) {
                    *validate = false;
                }
            }
            EventData::TextFull => self.full.set(self.full.get() + 1),
            _ => {}
        }
    }
}
fn observe(app: &Rc<CfwlApp>, edit: &Rc<CfwlEdit>) -> Rc<MutatingDelegate> {
    let delegate = Rc::new(MutatingDelegate {
        edit: Rc::downgrade(edit),
        app: Rc::downgrade(app),
        nested: Cell::new(false),
        full: Cell::new(0),
        validation: Cell::new(0),
    });
    edit.base().set_delegate(Some(delegate.clone()));
    let target: WidgetRef = edit.clone();
    app.note_driver()
        .register_event_target(&target, Some(&target));
    delegate
}
#[test]
fn cancelling_rewriting_validation_and_nested_edit_callbacks_use_the_live_widget() {
    let (app, control) = edit(FWL_STYLEEXT_EDT_VALIDATE);
    let delegate = observe(&app, &control);
    control.paste(&wide("cancel"));
    assert_eq!(control.get_text_length(), 0);
    assert!(!control.can_undo());
    control.paste(&wide("rewrite"));
    assert_eq!(control.get_text(), wide("changed"));
    assert!(control.undo());
    assert_eq!(control.get_text_length(), 0);
    control.paste(&wide("bad"));
    assert_eq!(control.get_text_length(), 0);
    control.paste(&wide("outer"));
    assert_eq!(control.get_text(), wide("Nested"));
    assert!(delegate.nested.get());
    assert!(delegate.validation.get() >= 3);
    assert!(control.undo());
    assert_eq!(control.get_text_length(), 0);
    control.set_limit(3);
    type_text(&control, "abcd");
    assert_eq!(control.get_text(), wide("abc"));
    assert!(delegate.full.get() >= 1);
}

#[test]
fn removal_from_text_callback_cancels_the_outer_widget_mutation() {
    let (app, control) = edit(FWL_STYLEEXT_EDT_VALIDATE);
    control.set_text(&wide("Before"));
    observe(&app, &control);
    control.set_cursor_position(control.get_text_length());
    control.paste(&wide("destroy"));
    assert!(control.base().is_destroyed());
    assert!(app.widget_mgr().get_widget(control.base().id()).is_none());
    assert_eq!(control.get_text(), wide("Before"));
}

#[test]
fn initial_value_skips_notifications_and_expands_the_interactive_limit() {
    let (app, control) = edit(FWL_STYLEEXT_EDT_VALIDATE);
    let delegate = observe(&app, &control);
    control.set_limit(3);
    // The initial-value path bypasses callbacks ("destroy" would remove the
    // widget in TextWillChange) and expands the backend limit to fit the value.
    control.set_text_skip_notify(&wide("destroy"));
    assert!(!control.base().is_destroyed());
    assert_eq!(control.get_text(), wide("destroy"));
    assert_eq!(delegate.validation.get(), 0);
    assert_eq!(delegate.full.get(), 0);
    assert!(!control.can_undo());
    control.select_all();
    assert_eq!(control.cut(), Some(wide("destroy")));
    assert!(control.undo());
    assert_eq!(control.get_text(), wide("destroy"));
    assert!(control.redo());
    assert!(control.get_text().is_empty());
    assert_eq!(delegate.full.get(), 0);
}
