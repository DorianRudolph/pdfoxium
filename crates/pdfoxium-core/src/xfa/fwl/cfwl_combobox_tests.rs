// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::fxge::{
    agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
    dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
};
use crate::xfa::fwl::{
    cfwl_picturebox::CfwlPictureBox, ifwl_widgetdelegate::WidgetDelegate, test_support::new_app,
};
use std::{cell::RefCell, rc::Weak};
fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}
fn combo() -> (Rc<CfwlApp>, Rc<CfwlComboBox>) {
    let (app, _) = new_app();
    let combo = CfwlComboBox::new(&app);
    combo.modify_style_exts(STYLEEXT_CMB_DROP_DOWN, 0);
    combo
        .base()
        .set_widget_rect(CfxRectF::new(0., 0., 120., 24.));
    combo.update();
    for text in ["Alpha", "Beta", "Gamma", "Delta"] {
        combo.add_string(&wide(text));
    }
    (app, combo)
}
struct Observer {
    events: RefCell<Vec<EventType>>,
    cancel_pre: bool,
    combo: Weak<CfwlComboBox>,
}
impl WidgetDelegate for Observer {
    fn on_process_message(&self, _: &mut FwlMessage) {}
    fn on_process_event(&self, event: &mut FwlEvent) {
        self.events.borrow_mut().push(event.get_type());
        if self.cancel_pre && event.get_type() == EventType::PreDropDown {
            if let Some(combo) = self.combo.upgrade() {
                combo.hide_drop_down_list();
            }
        }
    }
    fn on_draw_widget(&self, _: &mut CfgasGeGraphics, _: CfxMatrix) {}
}
fn observe(app: &Rc<CfwlApp>, combo: &Rc<CfwlComboBox>, cancel_pre: bool) -> Rc<Observer> {
    let listener = CfwlPictureBox::new(app);
    let observer = Rc::new(Observer {
        events: RefCell::new(Vec::new()),
        cancel_pre,
        combo: Rc::downgrade(combo),
    });
    listener.base().set_delegate(Some(observer.clone()));
    let listener: WidgetRef = listener;
    let source: WidgetRef = combo.clone();
    app.note_driver()
        .register_event_target(&listener, Some(&source));
    observer
}
#[test]
fn popup_events_selection_and_repeated_gestures() {
    let (app, combo) = combo();
    let observer = observe(&app, &combo, false);
    combo.set_cur_sel(1);
    assert_eq!(
        observer.events.borrow().as_slice(),
        &[EventType::EditChanged]
    );
    observer.events.borrow_mut().clear();
    combo.show_drop_down_list();
    assert!(combo.is_drop_list_visible());
    assert_eq!(
        observer.events.borrow().as_slice(),
        &[EventType::PreDropDown, EventType::PostDropDown]
    );
    assert_eq!(combo.get_edit_text(), wide("Beta"));
    let list = combo.get_combo_list();
    let item = list.get_item(2).unwrap();
    let rect = item.get_rect();
    let point = CfxPointF::new(rect.left + 2., rect.top + rect.height / 2.);
    for command in [MouseCommand::Move, MouseCommand::LeftButtonUp] {
        let target: WidgetRef = list.clone();
        let mut message = FwlMessage::mouse(Some(target), command, 0, point);
        list.on_process_message(&mut message);
    }
    assert_eq!(combo.get_cur_sel(), 2);
    assert_eq!(combo.get_edit_text(), wide("Gamma"));
    assert!(!combo.is_drop_list_visible());
    assert_eq!(
        observer.events.borrow().last(),
        Some(&EventType::SelectChanged)
    );
    combo.show_drop_down_list();
    assert!(combo.is_drop_list_visible());
    combo.hide_drop_down_list();
    assert!(!combo.is_drop_list_visible());
}
#[test]
fn prefix_matching_keyboard_navigation_and_stable_selection_after_remove() {
    let (_app, combo) = combo();
    let list = combo.get_combo_list();
    assert_eq!(list.match_item(&wide("")), -1);
    assert_eq!(list.match_item(&wide("Bet")), 1);
    assert_eq!(list.match_item(&wide("beta")), -1);
    combo.set_cur_sel(1);
    let target: WidgetRef = combo.clone();
    let mut down = FwlMessage::key(Some(target), KeyCommand::KeyDown, 0, VKEY_DOWN);
    combo.on_process_message(&mut down);
    assert_eq!(combo.get_cur_sel(), 2);
    assert_eq!(combo.get_edit_text(), wide("Gamma"));
    combo.remove_at(0);
    assert_eq!(combo.get_cur_sel(), 1);
    assert_eq!(combo.get_text_by_index(1), wide("Gamma"));
    combo.remove_at(1);
    assert_eq!(combo.get_cur_sel(), -1);
    assert!(combo.get_edit_text().is_empty());
    combo.modify_style_exts(STYLEEXT_CMB_READ_ONLY, 0);
    combo.update();
    assert!(combo.button_rect().is_empty());
    combo.modify_style_exts(0, STYLEEXT_CMB_READ_ONLY);
    combo.update();
    combo.set_cur_sel(0);
    combo.show_drop_down_list();
    assert!(combo.is_drop_list_visible());
    list.set_states(WidgetState::DISABLED);
    let rect = list.get_item(0).unwrap().get_rect();
    let point = CfxPointF::new(rect.left + 2., rect.top + rect.height / 2.);
    for command in [MouseCommand::Move, MouseCommand::LeftButtonUp] {
        let mut message = FwlMessage::mouse(Some(list.clone()), command, 0, point);
        list.on_process_message(&mut message);
    }
    assert_eq!(combo.get_cur_sel(), 0);
    assert!(combo.is_drop_list_visible());
}
#[test]
fn pre_drop_down_callback_can_cancel_without_reopening() {
    let (app, combo) = combo();
    let observer = observe(&app, &combo, true);
    combo.show_drop_down_list();
    assert!(!combo.is_drop_list_visible());
    assert_eq!(
        observer.events.borrow().as_slice(),
        &[EventType::PreDropDown]
    );
}
fn draw(combo: &Rc<CfwlComboBox>, via_manager: bool) -> Vec<u8> {
    let mut bitmap = CfxDibitmap::create(160, 160, FxdibFormat::Bgra, 0).unwrap();
    bitmap.clear(0xffffffff);
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        if via_manager {
            let owner: WidgetRef = combo.clone();
            combo.base().app().unwrap().widget_mgr().on_draw_widget(
                &owner,
                &mut graphics,
                CfxMatrix::default(),
            );
        } else {
            combo.draw_widget(&mut graphics, CfxMatrix::default());
        }
    }
    bitmap.buffer().to_vec()
}
#[test]
fn actual_child_drawing_matches_manager_without_duplicate_compositing() {
    let (_app, combo) = combo();
    combo.set_cur_sel(1);
    combo.show_drop_down_list();
    let direct = draw(&combo, false);
    assert!(direct
        .chunks_exact(4)
        .any(|pixel| pixel != [255, 255, 255, 255]));
    // Children with their own descendants (carets/scrollbars) remain visible;
    // this fixture fits every list item and has no focused caret.
    assert_eq!(draw(&combo, true), direct);
    assert!(combo.get_bbox().height > combo.base().get_widget_rect().height);
}

#[test]
fn combo_blur_hides_child_caret_and_cancels_its_blink_timer() {
    let (app, host) = new_app();
    let combo = CfwlComboBox::new(&app);
    combo.modify_style_exts(STYLEEXT_CMB_DROP_DOWN, 0);
    combo
        .base()
        .set_widget_rect(CfxRectF::new(0., 0., 120., 24.));
    combo.update();
    combo.add_string(&wide("Alpha"));
    combo.set_cur_sel(0);
    let edit = combo.edit();
    edit.base().set_states(WidgetState::FOCUSED);
    edit.set_cursor_position(1);
    let caret = edit.caret().unwrap();
    assert_eq!(host.timer_count(), 1);
    assert!(!caret.base().get_states().contains(WidgetState::INVISIBLE));
    let target: WidgetRef = combo.clone();
    combo.on_process_message(&mut FwlMessage::kill_focus(Some(target)));
    assert!(caret.base().get_states().contains(WidgetState::INVISIBLE));
    assert!(!edit.base().get_states().contains(WidgetState::FOCUSED));
    assert_eq!(host.timer_count(), 0);
}
