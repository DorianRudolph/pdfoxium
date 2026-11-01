// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived control sequences exercise real theme output and host timers.
use crate::xfa::fwl::{
    cfwl_app::CfwlApp,
    cfwl_caret::CfwlCaret,
    cfwl_checkbox::{CfwlCheckBox, STYLEEXT_CKB_3_STATE, STYLEEXT_CKB_RADIO_BUTTON},
    cfwl_event::{EventType, FwlEvent},
    cfwl_message::{FwlMessage, KeyCommand, MouseCommand},
    cfwl_picturebox::CfwlPictureBox,
    cfwl_pushbutton::CfwlPushButton,
    cfwl_widget::{FwlWidget, Properties, WidgetRef, WidgetState, WidgetStyle},
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
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

struct Recorder {
    events: RefCell<Vec<EventType>>,
    app: Weak<CfwlApp>,
    remove_source_on_mouse: Cell<bool>,
}
impl WidgetDelegate for Recorder {
    fn on_process_message(&self, _message: &mut FwlMessage) {}
    fn on_process_event(&self, event: &mut FwlEvent) {
        self.events.borrow_mut().push(event.get_type());
        if event.get_type() == EventType::Mouse && self.remove_source_on_mouse.replace(false) {
            if let (Some(app), Some(source)) = (self.app.upgrade(), event.source.take()) {
                app.remove_widget(source.base().id());
            }
        }
    }
    fn on_draw_widget(&self, _graphics: &mut CfgasGeGraphics<'_, '_, '_>, _matrix: CfxMatrix) {}
}
fn listen(app: &Rc<CfwlApp>, source: &WidgetRef) -> Rc<Recorder> {
    let listener = CfwlPictureBox::new(app);
    let listener_ref: WidgetRef = listener.clone();
    let recorder = Rc::new(Recorder {
        events: RefCell::new(Vec::new()),
        app: Rc::downgrade(app),
        remove_source_on_mouse: Cell::new(false),
    });
    listener.base().set_delegate(Some(recorder.clone()));
    app.note_driver()
        .register_event_target(&listener_ref, Some(source));
    recorder
}
fn key(widget: &WidgetRef, key: u32) {
    let mut message = FwlMessage::key(Some(widget.clone()), KeyCommand::KeyDown, 0, key);
    widget.on_process_message(&mut message);
}
fn mouse(widget: &WidgetRef, command: MouseCommand, x: f32, y: f32) {
    let mut message = FwlMessage::mouse(Some(widget.clone()), command, 0, CfxPointF::new(x, y));
    widget.on_process_message(&mut message);
}
fn render(widget: &WidgetRef) -> Vec<u8> {
    let mut bitmap = CfxDibitmap::create(180, 50, FxdibFormat::Bgra, 0).unwrap();
    bitmap.clear(0xffffffff);
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        widget.draw_widget(&mut graphics, CfxMatrix::default());
    }
    bitmap.buffer().to_vec()
}
fn changes(recorder: &Recorder, kind: EventType) -> usize {
    recorder
        .events
        .borrow()
        .iter()
        .filter(|&&event| event == kind)
        .count()
}

#[test]
fn checkbox_binary_three_state_radio_and_disabled_keyboard() {
    let (app, _host) = new_app();
    let checkbox = CfwlCheckBox::new(&app);
    let widget: WidgetRef = checkbox.clone();
    checkbox
        .base()
        .set_widget_rect(CfxRectF::new(0., 0., 150., 30.));
    checkbox.update();
    let recorder = listen(&app, &widget);
    assert_eq!(checkbox.get_check_state(), 0);
    key(&widget, 0x09);
    assert_eq!(checkbox.get_check_state(), 0);
    key(&widget, 0x20);
    assert_eq!(checkbox.get_check_state(), 1);
    key(&widget, 0x0d);
    assert_eq!(checkbox.get_check_state(), 0);
    checkbox
        .base()
        .modify_style_exts(STYLEEXT_CKB_3_STATE, u32::MAX);
    for expected in [2, 1, 0] {
        key(&widget, 0x20);
        assert_eq!(checkbox.get_check_state(), expected);
    }
    assert_eq!(changes(&recorder, EventType::CheckStateChanged), 5);
    checkbox
        .base()
        .modify_style_exts(STYLEEXT_CKB_RADIO_BUTTON, u32::MAX);
    key(&widget, 0x0d);
    key(&widget, 0x20);
    assert_eq!(checkbox.get_check_state(), 1);
    assert_eq!(changes(&recorder, EventType::CheckStateChanged), 6);
    checkbox.set_states(WidgetState::DISABLED);
    key(&widget, 0x20);
    assert_eq!(checkbox.get_check_state(), 1);
    checkbox.clear_states(WidgetState::DISABLED);
    checkbox.set_check_state(2);
    assert_eq!(checkbox.get_check_state(), 0);
    checkbox
        .base()
        .modify_style_exts(STYLEEXT_CKB_3_STATE, u32::MAX);
    checkbox.set_check_state(2);
    assert_eq!(checkbox.get_check_state(), 2);
}

#[test]
fn checkbox_mouse_geometry_focus_and_real_theme_drawing() {
    let (app, host) = new_app();
    let checkbox = CfwlCheckBox::new(&app);
    let widget: WidgetRef = checkbox.clone();
    checkbox
        .base()
        .set_widget_rect(CfxRectF::new(0., 0., 150.4, 30.4));
    checkbox.update();
    assert_eq!(checkbox.get_widget_rect().width, 150.);
    assert_eq!(checkbox.box_rect(), CfxRectF::new(0., 0., 16., 16.));
    assert_eq!(checkbox.caption_rect(), CfxRectF::new(21., 5., 124., 20.));
    let unchecked = render(&widget);
    mouse(&widget, MouseCommand::LeftButtonDown, 2., 2.);
    assert!(checkbox
        .base()
        .get_states()
        .intersects(WidgetState::PRESSED));
    mouse(&widget, MouseCommand::Move, 170., 2.);
    assert!(!checkbox
        .base()
        .get_states()
        .intersects(WidgetState::PRESSED));
    mouse(&widget, MouseCommand::Move, 2., 2.);
    mouse(&widget, MouseCommand::LeftButtonUp, 2., 2.);
    assert_eq!(checkbox.get_check_state(), 1);
    assert_ne!(render(&widget), unchecked);
    mouse(&widget, MouseCommand::LeftButtonDown, 2., 2.);
    mouse(&widget, MouseCommand::LeftButtonUp, 170., 2.);
    assert!(!checkbox
        .base()
        .get_states()
        .intersects(WidgetState::PRESSED));
    assert_eq!(checkbox.get_check_state(), 1);
    let mut focus = FwlMessage::set_focus(Some(widget.clone()));
    checkbox.on_process_message(&mut focus);
    assert!(checkbox
        .base()
        .get_states()
        .intersects(WidgetState::FOCUSED));
    let mut blur = FwlMessage::kill_focus(Some(widget));
    checkbox.on_process_message(&mut blur);
    assert!(!checkbox
        .base()
        .get_states()
        .intersects(WidgetState::FOCUSED));
    assert!(!host.repaints.borrow().is_empty());
}

#[test]
fn pushbutton_mouse_keyboard_disabled_and_event_removal() {
    let (app, _host) = new_app();
    let button = CfwlPushButton::new(&app);
    let widget: WidgetRef = button.clone();
    button
        .base()
        .set_widget_rect(CfxRectF::new(0., 0., 80., 25.));
    button.update();
    let recorder = listen(&app, &widget);
    let normal = render(&widget);
    mouse(&widget, MouseCommand::LeftButtonDown, 5., 5.);
    let pressed = render(&widget);
    assert_ne!(pressed, normal);
    mouse(&widget, MouseCommand::LeftButtonUp, 5., 5.);
    assert_eq!(changes(&recorder, EventType::Click), 1);
    mouse(&widget, MouseCommand::LeftButtonDown, 5., 5.);
    mouse(&widget, MouseCommand::LeftButtonUp, 100., 5.);
    assert_eq!(changes(&recorder, EventType::Click), 1);
    key(&widget, 0x20);
    assert_eq!(changes(&recorder, EventType::Click), 1);
    key(&widget, 0x0d);
    assert_eq!(changes(&recorder, EventType::Click), 2);
    button.set_states(WidgetState::DISABLED);
    assert_eq!(button.base().get_states(), WidgetState::DISABLED);
    key(&widget, 0x0d);
    mouse(&widget, MouseCommand::LeftButtonDown, 5., 5.);
    mouse(&widget, MouseCommand::LeftButtonUp, 5., 5.);
    assert_eq!(changes(&recorder, EventType::Click), 2);
    button.clear_states(WidgetState::DISABLED);
    recorder.remove_source_on_mouse.set(true);
    key(&widget, 0x0d);
    assert_eq!(changes(&recorder, EventType::Click), 2);
    assert!(!button.base().is_registered());
}

#[test]
fn picture_border_and_update_lock_use_common_widget_contract() {
    let (app, _host) = new_app();
    let picture = CfwlPictureBox::with_properties(
        &app,
        Properties {
            styles: WidgetStyle::CHILD | WidgetStyle::BORDER,
            ..Default::default()
        },
        None,
    );
    let widget: WidgetRef = picture.clone();
    picture
        .base()
        .set_widget_rect(CfxRectF::new(0., 0., 70., 25.));
    {
        let _lock = picture.base().scoped_update_lock();
        picture.update();
        assert_eq!(picture.client_rect(), CfxRectF::default());
    }
    picture.update();
    assert_eq!(picture.client_rect(), CfxRectF::new(1., 1., 68., 23.));
    let bordered = render(&widget);
    assert!(bordered
        .chunks_exact(4)
        .any(|pixel| pixel != [255, 255, 255, 255]));
    picture.base().set_styles(WidgetStyle::CHILD);
    assert!(render(&widget).iter().all(|&byte| byte == 255));
}

#[test]
fn caret_blink_hide_replacement_destruction_and_late_callbacks() {
    let (app, host) = new_app();
    let caret = CfwlCaret::new(&app, Properties::default(), None);
    let widget: WidgetRef = caret.clone();
    caret.base().set_widget_rect(CfxRectF::new(0., 0., 2., 18.));
    caret.show_caret();
    let scheduled = host.scheduled();
    assert_eq!(scheduled.len(), 1);
    let (id, interval) = scheduled[0];
    assert_eq!(interval, 600);
    let late = host.timer_callback(id);
    let highlighted = render(&widget);
    assert!(highlighted.chunks_exact(4).any(|pixel| pixel[0] == 0));
    host.fire(id);
    assert!(!caret
        .base()
        .get_states()
        .intersects(WidgetState::CARET_HIGHLIGHT));
    assert!(render(&widget).iter().all(|&byte| byte == 255));
    host.fire(id);
    assert_eq!(render(&widget), highlighted);
    caret.show_caret();
    assert_eq!(host.timer_count(), 1);
    assert!(host.killed.borrow().contains(&id));
    caret.hide_caret();
    assert_eq!(host.timer_count(), 0);
    assert!(!caret.base().is_visible());
    let before = caret.base().get_states();
    late(id);
    assert_eq!(caret.base().get_states(), before);
    caret.show_caret();
    let (id, _) = host.scheduled()[0];
    let late = host.timer_callback(id);
    app.remove_widget(caret.base().id());
    assert_eq!(host.timer_count(), 0);
    late(id);
    caret.show_caret();
    assert_eq!(host.timer_count(), 0);
    assert!(!host.repaints.borrow().is_empty());
}
