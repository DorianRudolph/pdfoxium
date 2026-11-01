// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived list selection, stable ownership, scrolling and real drawing.
use super::*;
use crate::{
    fxcrt::fx_coordinates::CfxVector,
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
    xfa::fwl::{
        fwl_widgetdef::{VKEY_DOWN, VKEY_END, VKEY_HOME},
        test_support::new_app,
    },
};
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn list(multi: bool) -> (Rc<CfwlApp>, Rc<CfwlListBox>) {
    let (app, _) = new_app();
    let list = CfwlListBox::new(
        &app,
        Properties {
            styles: WidgetStyle::VSCROLL,
            style_exts: if multi {
                FWL_STYLEEXT_LTB_MULTI_SELECTION
            } else {
                0
            },
            states: WidgetState::NONE,
        },
        None,
    );
    list.base.set_widget_rect(CfxRectF::new(0., 0., 100., 50.));
    for value in ["one", "two", "three", "four", "five", "six"] {
        list.add_string(&wide(value));
    }
    list.update();
    (app, list)
}
fn mouse(list: &Rc<CfwlListBox>, y: f32, flags: u32) {
    let mut message = FwlMessage::mouse(
        Some(list.clone()),
        MouseCommand::LeftButtonDown,
        flags,
        CfxPointF::new(5., y),
    );
    list.on_process_message(&mut message);
    message.data = MessageData::Mouse {
        command: MouseCommand::LeftButtonUp,
        flags,
        pos: CfxPointF::new(5., y),
    };
    list.on_process_message(&mut message);
}
fn key(list: &Rc<CfwlListBox>, code: u32, flags: u32) {
    let mut message = FwlMessage::key(Some(list.clone()), KeyCommand::KeyDown, flags, code);
    list.on_process_message(&mut message);
}
#[test]
fn single_multi_ctrl_shift_and_keyboard_navigation() {
    let (_app, list) = list(false);
    assert_eq!(list.count_items(), 6);
    assert_eq!(list.calc_item_height(), 16.);
    mouse(&list, 5., 0);
    assert_eq!(list.get_sel_index(0), 0);
    key(&list, VKEY_DOWN, 0);
    assert_eq!(list.get_sel_index(0), 1);
    key(&list, VKEY_END, 0);
    assert_eq!(list.get_sel_index(0), 5);
    key(&list, VKEY_HOME, 0);
    assert_eq!(list.get_sel_index(0), 0);
    assert_eq!(list.count_sel_items(), 1);
    list.set_states(WidgetState::DISABLED);
    key(&list, VKEY_DOWN, 0);
    assert_eq!(list.get_sel_index(0), 0);
    let (_app, list) = self::list(true);
    mouse(&list, 5., 0);
    mouse(&list, 21., KEY_FLAG_CTRL);
    assert_eq!(list.count_sel_items(), 2);
    mouse(&list, 37., KEY_FLAG_SHIFT);
    assert_eq!(list.count_sel_items(), 2);
    assert_eq!(list.get_sel_index(0), 1);
    assert_eq!(list.get_sel_index(1), 2);
    key(&list, VKEY_DOWN, KEY_FLAG_CTRL);
    assert_eq!(list.count_sel_items(), 2);
    assert!(list.get_item(3).unwrap().is_focused());
    list.set_sel_item(None, true);
    assert_eq!(list.count_sel_items(), 6);
    list.set_sel_item(None, false);
    assert_eq!(list.count_sel_items(), 0);
    assert!(list.get_focused_item().is_none());
}
#[test]
fn stable_item_owners_and_deleted_anchor_do_not_select_removed_items() {
    let (_app, list) = list(true);
    let first = list.get_item(0).unwrap();
    for i in 0..400 {
        list.add_string(&wide(&format!("item {i}")));
    }
    assert!(Rc::ptr_eq(&first, &list.get_item(0).unwrap()));
    mouse(&list, 21., KEY_FLAG_CTRL);
    let removed_anchor = list.get_item(1).unwrap();
    list.remove_at(1);
    assert_eq!(list.get_item_index(Some(&removed_anchor)), -1);
    list.clear_selection();
    list.update();
    mouse(&list, 21., KEY_FLAG_SHIFT);
    assert_eq!(list.count_sel_items(), 1);
    assert_eq!(list.get_sel_item(0).unwrap().text(), wide("three"));
    list.set_sel_item(Some(&removed_anchor), true);
    assert_eq!(list.count_sel_items(), 1);
    list.delete_all();
    assert_eq!(list.count_items(), 0);
    assert!(list.anchor().is_none());
    assert_eq!(first.text(), wide("one"));
    list.remove_at(-1);
    list.remove_at(i32::MAX);
    list.delete_string(&removed_anchor);
    assert_eq!(list.count_items(), 0);
}
#[test]
fn source_geometry_scroll_visibility_wheel_and_scroll_codes() {
    let (_app, list) = list(false);
    assert_eq!(
        list.get_item(0).unwrap().get_rect(),
        CfxRectF::new(0., 0., 100., 16.)
    );
    assert_eq!(list.get_item(5).unwrap().get_rect().top, 80.);
    let bar = list.get_vert_scroll_bar().unwrap();
    assert_eq!(bar.get_range(), (0., 46.));
    assert_eq!(bar.get_step_size(), 16.);
    assert!(list.is_show_vert_scroll_bar());
    assert_eq!(
        list.hit_test(CfxPointF::new(90., 10.)),
        FwlWidgetHit::VScrollBar
    );
    assert!(list.scroll_to_visible(list.get_item(5).as_ref()));
    assert_eq!(bar.get_pos(), 46.);
    assert_eq!(
        list.get_item_at_point(CfxPointF::new(5., 40.))
            .unwrap()
            .text(),
        wide("six")
    );
    list.on_scroll(&bar, ScrollCode::Min, 0.);
    assert_eq!(bar.get_pos(), 0.);
    let mut wheel = FwlMessage::mouse_wheel(
        Some(list.clone()),
        CfxPointF::new(5., 10.),
        CfxVector::new(0, -120),
    );
    list.on_process_message(&mut wheel);
    assert_eq!(bar.get_pos(), 16.);
    list.on_scroll(&bar, ScrollCode::PageForward, bar.get_pos());
    assert_eq!(bar.get_pos(), 46.);
    assert!(!list.on_scroll(&bar, ScrollCode::EndScroll, bar.get_pos()));
    list.base
        .modify_style_exts(FWL_STYLEEXT_LTB_SHOW_SCROLL_BAR_FOCUS, 0);
    list.on_process_message(&mut FwlMessage::kill_focus(Some(list.clone())));
    assert!(!list.is_show_vert_scroll_bar());
    list.on_process_message(&mut FwlMessage::set_focus(Some(list.clone())));
    assert!(list.is_show_vert_scroll_bar());
}
fn render(list: &Rc<CfwlListBox>) -> Vec<u8> {
    let mut bitmap = CfxDibitmap::create(120, 70, FxdibFormat::Bgra, 0).unwrap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        let clip = graphics.get_clip_rect();
        list.draw_widget(&mut graphics, CfxMatrix::new(1., 0., 0., 1., 5., 5.));
        assert_eq!(graphics.get_clip_rect(), clip);
    }
    bitmap.buffer().to_vec()
}
#[test]
fn selected_item_bitmap_changes_and_clipping_restores_device_state() {
    let (_app, list) = list(false);
    let plain = render(&list);
    assert!(plain.chunks_exact(4).filter(|pixel| pixel[3] != 0).count() > 100);
    assert!(plain[..120 * 4 * 4].iter().all(|&byte| byte == 0));
    mouse(&list, 5., 0);
    let selected = render(&list);
    assert_ne!(selected, plain);
    assert_eq!(render(&list), selected);
}
#[test]
fn repaint_callback_can_remove_the_list_during_input() {
    let (app, host) = new_app();
    let list = CfwlListBox::new(&app, Properties::default(), None);
    list.base.set_widget_rect(CfxRectF::new(0., 0., 100., 50.));
    list.add_string(&wide("one"));
    list.update();
    let owner = Rc::downgrade(&app);
    let id = list.base.id();
    *host.repaint_hook.borrow_mut() = Some(Rc::new(move |_| {
        if let Some(app) = owner.upgrade() {
            app.remove_widget(id);
        }
    }));
    mouse(&list, 5., 0);
    assert!(list.base.is_destroyed());
    assert!(app.widget_mgr().get_widget(id).is_none());
    assert!(app.note_driver().grab().is_none());
}
