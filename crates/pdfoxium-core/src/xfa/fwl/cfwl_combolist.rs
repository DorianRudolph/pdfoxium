// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cfwl_combolist.{h,cpp}`. Stable shared list items replace unowned pointers;
//! the parent is resolved through the widget tree and checked before callbacks.
use super::{
    cfwl_app::CfwlApp,
    cfwl_event::FwlEvent,
    cfwl_listbox::CfwlListBox,
    cfwl_message::{FwlMessage, KeyCommand, MessageData, MouseCommand},
    cfwl_widget::{dispatch_message_to, CfwlWidget, FwlType, FwlWidget, Properties, WidgetRef},
    fwl_widgetdef::{VKEY_DOWN, VKEY_ESCAPE, VKEY_RETURN, VKEY_UP},
    fwl_widgethit::FwlWidgetHit,
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
use std::{cell::Cell, ops::Deref, rc::Rc};

pub struct CfwlComboList {
    list: CfwlListBox,
    notify_owner: Cell<bool>,
}
impl CfwlComboList {
    pub fn new(app: &Rc<CfwlApp>, properties: Properties, outer: Option<WidgetRef>) -> Rc<Self> {
        let control = Rc::new(Self {
            list: CfwlListBox::new_unregistered(app, properties, outer),
            notify_owner: Cell::new(true),
        });
        app.register_widget(control.clone());
        control
    }
    pub fn reset_notify_owner(&self) {
        self.notify_owner.set(true);
    }
    pub fn match_item(&self, text: &[u32]) -> i32 {
        if text.is_empty() {
            return -1;
        }
        for index in 0..self.list.count_items() {
            if self
                .list
                .get_item(index)
                .is_some_and(|item| item.text().starts_with(text))
            {
                return index;
            }
        }
        -1
    }
    pub fn change_selected(&self, index: i32) {
        let item = self.list.get_item(index);
        let old = self.list.get_sel_item(0);
        let old_index = self.list.get_item_index(old.as_ref());
        if old_index == index {
            return;
        }
        let mut invalid = CfxRectF::default();
        if let Some(old) = old {
            invalid = old.get_rect();
            self.list.set_sel_item(Some(&old), false);
        }
        if let Some(item) = item {
            invalid.union(item.get_rect());
            self.list.set_sel_item(Some(&item), true);
        }
        if !invalid.is_empty() {
            self.base().repaint_rect(invalid);
        }
    }
    fn client_to_outer(&self, p: CfxPointF) -> CfxPointF {
        let rect = self.base().get_widget_rect();
        CfxPointF::new(p.x + rect.left, p.y + rect.top)
    }
    fn forward_outer(&self, message: &mut FwlMessage) {
        if let Some(outer) = self.base().outer() {
            dispatch_message_to(&outer, message);
        }
    }
    fn hide_outer(&self) {
        if let Some(outer) = self.base().outer() {
            if let Some(combo) = outer.as_combo_box() {
                combo.hide_drop_down_list();
            }
        }
    }
    fn process_selection(&self, button_up: bool) {
        if let Some(outer) = self.base().outer() {
            if let Some(combo) = outer.as_combo_box() {
                combo.process_sel_changed(button_up);
            }
        }
    }
    fn scrollbar_contains(&self, p: CfxPointF) -> bool {
        self.list.is_show_vert_scroll_bar()
            && self
                .list
                .get_vert_scroll_bar()
                .is_some_and(|bar| bar.get_widget_rect().contains_point(p))
    }
    fn on_drop_list_key_down(&self, key: u32) {
        let Some(outer) = self.base().outer() else {
            return;
        };
        let Some(combo) = outer.as_combo_box() else {
            return;
        };
        let item = self.list.get_item(combo.get_current_selection());
        let Some(item) = self.list.get_list_item(item.as_ref(), key) else {
            return;
        };
        self.list.set_selection(Some(&item), Some(&item), true);
        self.list.scroll_to_visible(Some(&item));
        self.base().repaint_rect(self.base().get_relative_rect());
    }
    fn on_drop_list_key(&self, message: &mut FwlMessage) -> bool {
        let MessageData::Key {
            command,
            key_code_or_char,
            ..
        } = message.data
        else {
            return false;
        };
        if command == KeyCommand::KeyDown {
            match key_code_or_char {
                VKEY_RETURN | VKEY_ESCAPE => {
                    self.hide_outer();
                    return true;
                }
                VKEY_UP | VKEY_DOWN => {
                    self.on_drop_list_key_down(key_code_or_char);
                    self.process_selection(false);
                    return true;
                }
                _ => {}
            }
        }
        if let Some(outer) = self.base().outer() {
            message.destination = Some(outer.clone());
            dispatch_message_to(&outer, message);
            return true;
        }
        false
    }
}
impl Deref for CfwlComboList {
    type Target = CfwlListBox;
    fn deref(&self) -> &Self::Target {
        &self.list
    }
}
impl FwlWidget for CfwlComboList {
    fn base(&self) -> &CfwlWidget {
        self.list.base()
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::ListBox
    }
    fn update(&self) {
        self.list.update();
    }
    fn draw_widget(&self, g: &mut CfgasGeGraphics, m: CfxMatrix) {
        self.list.draw_widget(g, m);
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        self.list.on_process_event(event);
    }
    fn pre_finalize(&self) {
        self.list.pre_finalize();
    }
    fn hit_test(&self, p: CfxPointF) -> FwlWidgetHit {
        self.list.hit_test(p)
    }
    fn get_client_rect(&self) -> CfxRectF {
        self.list.get_client_rect()
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        if self.base().is_destroyed() || !self.base().is_enabled() {
            return;
        }
        match &message.data {
            MessageData::KillFocus {
                set_focus: Some(focus),
            } => {
                if let Some(outer) = self.base().outer() {
                    if let Some(combo) = outer.as_combo_box() {
                        let edit: WidgetRef = combo.get_combo_edit();
                        if Rc::ptr_eq(focus, &outer) || Rc::ptr_eq(focus, &edit) {
                            combo.hide_drop_down_list();
                        }
                    }
                }
            }
            MessageData::Mouse { command, pos, .. } => {
                let command = *command;
                let pos = *pos;
                if self.scrollbar_contains(pos) {
                    if let Some(bar) = self.list.get_vert_scroll_bar() {
                        let rect = bar.get_widget_rect();
                        if let MessageData::Mouse { pos, .. } = &mut message.data {
                            *pos = CfxPointF::new(pos.x - rect.left, pos.y - rect.top);
                        }
                        let bar: WidgetRef = bar;
                        dispatch_message_to(&bar, message);
                        return;
                    }
                }
                match command {
                    MouseCommand::Move => {
                        if self.list.client_rect().contains_point(pos) {
                            self.notify_owner.set(false);
                            if let Some(item) = self.list.get_item_at_point(pos) {
                                self.change_selected(self.list.get_item_index(Some(&item)));
                            }
                        } else if self.notify_owner.get() {
                            if let MessageData::Mouse { pos, .. } = &mut message.data {
                                *pos = self.client_to_outer(*pos);
                            }
                            self.forward_outer(message);
                        }
                        return;
                    }
                    MouseCommand::LeftButtonDown => {
                        if !self.list.client_rect().contains_point(pos) {
                            self.hide_outer();
                        }
                        return;
                    }
                    MouseCommand::LeftButtonUp => {
                        if self.notify_owner.get() {
                            if let MessageData::Mouse { pos, .. } = &mut message.data {
                                *pos = self.client_to_outer(*pos);
                            }
                            self.forward_outer(message);
                        } else {
                            self.hide_outer();
                            if self.list.get_item_at_point(pos).is_some() {
                                self.process_selection(true);
                            }
                        }
                        return;
                    }
                    _ => {}
                }
            }
            MessageData::Key { .. } => {
                if self.on_drop_list_key(message) {
                    return;
                }
            }
            _ => {}
        }
        if !self.base().is_destroyed() {
            self.list.on_process_message(message);
        }
    }
}
