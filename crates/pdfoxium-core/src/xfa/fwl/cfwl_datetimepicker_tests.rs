// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived date picker selection/format, popup and actual drawing checks.
use super::*;
use crate::{
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
    xfa::fwl::{
        cfwl_picturebox::CfwlPictureBox, ifwl_widgetdelegate::WidgetDelegate, test_support::new_app,
    },
};
use std::cell::RefCell;
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn picker() -> (Rc<CfwlApp>, Rc<CfwlDateTimePicker>) {
    let (app, _host) = new_app();
    let picker = CfwlDateTimePicker::new(&app);
    picker
        .base
        .set_widget_rect(CfxRectF::new(0., 0., 120., 24.));
    picker.update();
    (app, picker)
}
#[derive(Default)]
struct Events(RefCell<Vec<(super::super::cfwl_event::EventType, Option<(i32, i32, i32)>)>>);
impl WidgetDelegate for Events {
    fn on_process_message(&self, _: &mut FwlMessage) {}
    fn on_process_event(&self, event: &mut FwlEvent) {
        let date = match event.data {
            EventData::SelectChanged {
                year, month, day, ..
            } => Some((year, month, day)),
            _ => None,
        };
        self.0.borrow_mut().push((event.get_type(), date));
    }
    fn on_draw_widget(&self, _: &mut CfgasGeGraphics, _: CfxMatrix) {}
}
#[test]
fn selection_bounds_and_both_source_format_strings() {
    let (_app, picker) = picker();
    assert_eq!(picker.get_cur_sel(), (-1, -1, -1));
    picker.set_cur_sel(2024, 2, 29);
    assert_eq!(picker.get_cur_sel(), (2024, 2, 29));
    assert_eq!(picker.month_calendar().selection(), Some((2024, 2, 29)));
    for date in [
        (0, 1, 1),
        (3000, 1, 1),
        (2024, 0, 1),
        (2024, 13, 1),
        (2024, 1, 0),
        (2024, 1, 32),
        (2023, 2, 29),
        (2024, 4, 31),
    ] {
        picker.set_cur_sel(date.0, date.1, date.2);
        assert_eq!(picker.get_cur_sel(), (2024, 2, 29));
    }
    assert_eq!(picker.format_date_string(2024, 2, 9), wide("2024-2-9"));
    picker
        .base
        .modify_style_exts(0, FWL_STYLEEXT_DTP_SHORT_DATE_FORMAT);
    assert_eq!(
        picker.format_date_string(2024, 2, 9),
        wide("2024 Year 2 Month 9 Day")
    );
    // The source edit engine enforces the available horizontal area. Give
    // the long literal format enough space before testing its complete text.
    picker
        .base
        .set_widget_rect(CfxRectF::new(0., 0., 300., 24.));
    picker.update();
    picker.process_sel_changed(2024, 2, 9);
    assert_eq!(picker.get_edit_text(), wide("2024 Year 2 Month 9 Day"));
}
#[test]
fn edit_selection_limit_alignment_and_clipboard_forward_to_real_child() {
    let (_app, picker) = picker();
    picker.set_edit_text(&wide("Date text"));
    assert_eq!(picker.get_edit_text_length(), 9);
    picker.select_all();
    assert!(picker.has_selection());
    assert_eq!(picker.get_selection(), (0, 9));
    assert_eq!(picker.copy(), Some(wide("Date text")));
    assert_eq!(picker.cut(), Some(wide("Date text")));
    assert_eq!(picker.get_edit_text(), wide(""));
    assert!(picker.paste(&wide("abc")));
    assert_eq!(picker.get_edit_text(), wide("abc"));
    assert!(picker.can_undo());
    assert!(picker.undo());
    assert_eq!(picker.get_edit_text(), wide(""));
    assert!(picker.can_redo());
    assert!(picker.redo());
    assert_eq!(picker.get_edit_text(), wide("abc"));
    picker.clear_selection();
    assert!(!picker.has_selection());
    picker.clear_text();
    picker.set_edit_limit(5);
    assert!(picker.paste(&wide("1234567")));
    assert_eq!(picker.get_edit_text(), wide("12345"));
    picker.base.modify_style_exts(
        FWL_STYLEEXT_DTP_EDIT_H_CENTER
            | FWL_STYLEEXT_DTP_EDIT_V_FAR
            | FWL_STYLEEXT_DTP_EDIT_JUSTIFIED,
        0,
    );
    picker.update();
    assert_eq!(
        picker.edit().base().get_style_exts()
            & (EDIT_H_ALIGN_MASK | EDIT_V_ALIGN_MASK | EDIT_H_ALIGN_MODE_MASK),
        (1 << 18) | (2 << 20) | (1 << 22)
    );
}
#[test]
fn calendar_popup_click_updates_parent_emits_date_and_hides() {
    let (app, picker) = picker();
    picker.set_cur_sel(2024, 2, 29);
    let listener = CfwlPictureBox::new(&app);
    let observer = Rc::new(Events::default());
    listener.base().set_delegate(Some(observer.clone()));
    let listener_ref: WidgetRef = listener;
    let source: WidgetRef = picker.clone();
    app.note_driver()
        .register_event_target(&listener_ref, Some(&source));
    let mut focus = FwlMessage::set_focus(Some(picker.clone()));
    picker.on_process_message(&mut focus);
    assert!(!picker.button_rect().is_empty());
    assert_eq!(
        picker.hit_test(picker.button_rect().center()),
        FwlWidgetHit::Client
    );
    let before = picker.get_bbox();
    picker.show_month_calendar();
    assert!(picker.is_month_calendar_visible());
    assert!(picker.get_bbox().height > before.height);
    let calendar = picker.month_calendar();
    let point = calendar.day_rect(15).center();
    for command in [MouseCommand::LeftButtonDown, MouseCommand::LeftButtonUp] {
        let mut message = FwlMessage::mouse(Some(calendar.clone()), command, 0, point);
        calendar.on_process_message(&mut message);
    }
    assert_eq!(picker.get_cur_sel(), (2024, 2, 15));
    assert_eq!(picker.get_edit_text(), wide("2024-2-15"));
    assert!(!picker.is_month_calendar_visible());
    assert!(observer.0.borrow().iter().any(|&(kind, date)| kind
        == super::super::cfwl_event::EventType::SelectChanged
        && date == Some((2024, 2, 15))));
}
#[test]
fn draw_and_date_edit_mouse_dismissal_use_real_children_and_teardown() {
    let (app, picker) = picker();
    picker.set_cur_sel(2024, 2, 12);
    picker.process_sel_changed(2024, 2, 12);
    let mut focus = FwlMessage::set_focus(Some(picker.clone()));
    picker.on_process_message(&mut focus);
    picker.show_month_calendar();
    let bounds = picker.get_bbox();
    let width = bounds.right().ceil().max(150.) as i32;
    let height = bounds.bottom().ceil().max(60.) as i32;
    let mut bitmap = CfxDibitmap::create(width, height, FxdibFormat::Bgra, 0).unwrap();
    bitmap.clear(0xffffffff);
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        picker.draw_widget(&mut graphics, CfxMatrix::default());
    }
    assert!(
        bitmap
            .buffer()
            .chunks_exact(4)
            .filter(|pixel| *pixel != [255, 255, 255, 255])
            .count()
            > 100
    );
    let mut down = FwlMessage::mouse(
        Some(picker.edit().clone()),
        MouseCommand::LeftButtonDown,
        0,
        CfxPointF::new(5., 5.),
    );
    picker.edit().on_process_message(&mut down);
    assert!(!picker.is_month_calendar_visible());
    let edit_id = picker.edit().base().id();
    let calendar_id = picker.month_calendar().base().id();
    app.remove_widget(picker.base.id());
    assert!(app.widget_mgr().get_widget(edit_id).is_none());
    assert!(app.widget_mgr().get_widget(calendar_id).is_none());
    picker.show_month_calendar();
    assert!(!picker.is_month_calendar_visible());
}
