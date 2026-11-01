// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Combo-box window from `fpdfsdk/pwl/cpwl_combo_box.{h,cpp}`.
use super::cpwl_cbbutton::CpwlCbButton;
use super::cpwl_cblistbox::CpwlCbListBox;
use super::cpwl_edit::CpwlEdit;
use super::cpwl_wnd::*;
use super::ipwl_fillernotify::{CursorStyle, PerWindowData};
use crate::fpdfsdk::cpdfsdk_baannot::BorderStyle;
use crate::fpdfsdk::fpdf_fwlevent::*;
use crate::fxcrt::cfx_bidi_resolver::ParagraphDirection;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF};
use crate::fxge::cfx_color::{CfxColor, ColorType};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
pub struct CpwlComboBox {
    wnd: CpwlWnd,
    edit: RefCell<Option<Weak<CpwlEdit>>>,
    button: RefCell<Option<Weak<CpwlCbButton>>>,
    list: RefCell<Option<Weak<CpwlCbListBox>>>,
    old_window_rect: Cell<CfxFloatRect>,
    is_popup: Cell<bool>,
    is_bottom: Cell<bool>,
    select_item: Cell<i32>,
}
impl CpwlComboBox {
    pub fn new(mut cp: CreateParams, attached: Option<Box<dyn PerWindowData>>) -> Rc<Self> {
        cp.flags &= !WINDOW_VSCROLL;
        let result = Rc::new(Self {
            wnd: CpwlWnd::new(cp, attached),
            edit: RefCell::new(None),
            button: RefCell::new(None),
            list: RefCell::new(None),
            old_window_rect: Cell::new(CfxFloatRect::default()),
            is_popup: Cell::new(false),
            is_bottom: Cell::new(true),
            select_item: Cell::new(-1),
        });
        let window: Rc<dyn Window> = result.clone();
        result.wnd.bind_owner(&window);
        result
    }
    pub fn edit(&self) -> Option<Rc<CpwlEdit>> {
        self.edit.borrow().as_ref().and_then(Weak::upgrade)
    }
    fn button(&self) -> Option<Rc<CpwlCbButton>> {
        self.button.borrow().as_ref().and_then(Weak::upgrade)
    }
    pub fn list(&self) -> Option<Rc<CpwlCbListBox>> {
        self.list.borrow().as_ref().and_then(Weak::upgrade)
    }
    pub fn set_text(&self, text: &[u32]) {
        if let Some(edit) = self.edit() {
            edit.set_text(text);
        }
    }
    pub fn add_string(&self, text: &[u32]) {
        if let Some(list) = self.list() {
            list.add_string(text);
        }
    }
    pub fn selected_index(&self) -> i32 {
        self.select_item.get()
    }
    pub fn set_select(&self, index: i32) -> bool {
        let (Some(list), Some(edit)) = (self.list(), self.edit()) else {
            return false;
        };
        list.select(index);
        if !self.wnd.is_alive() {
            return false;
        }
        edit.set_text(&list.text());
        if !self.wnd.is_alive() {
            return false;
        }
        self.select_item.set(index);
        true
    }
    pub fn set_edit_selection(&self, start: i32, end: i32) {
        if let Some(edit) = self.edit() {
            edit.set_selection(start, end);
        }
    }
    pub fn clear_selection(&self) {
        if let Some(edit) = self.edit() {
            edit.clear_selection();
        }
    }
    pub fn select_all(&self) {
        if self.wnd.has_flag(COMBOBOX_ALLOW_CUSTOM_TEXT) {
            self.select_all_text();
        }
    }
    pub fn is_popup(&self) -> bool {
        self.is_popup.get()
    }
    pub fn set_select_text(&self) {
        let (Some(edit), Some(list)) = (self.edit(), self.list()) else {
            return;
        };
        edit.select_all_text();
        if !self.wnd.is_alive() {
            return;
        }
        edit.replace_selection(&list.text());
        if !self.wnd.is_alive() {
            return;
        }
        edit.select_all_text();
        if !self.wnd.is_alive() {
            return;
        }
        self.select_item.set(list.cur_sel());
    }
    fn create_edit(&self, cp: &CreateParams) {
        if self.edit().is_some() {
            return;
        }
        let mut params = cp.clone();
        params.flags = WINDOW_VISIBLE | WINDOW_BORDER | EDIT_CENTER | EDIT_AUTO_SCROLL | EDIT_UNDO;
        if self.wnd.has_flag(WINDOW_AUTO_FONT_SIZE) {
            params.flags |= WINDOW_AUTO_FONT_SIZE;
        }
        if !self.wnd.has_flag(COMBOBOX_ALLOW_CUSTOM_TEXT) {
            params.flags |= WINDOW_READ_ONLY;
        }
        params.rect = CfxFloatRect::default();
        params.border_width = 0;
        params.border_style = BorderStyle::Solid;
        let edit = CpwlEdit::new(params, self.wnd.clone_attached_data());
        *self.edit.borrow_mut() = Some(Rc::downgrade(&edit));
        let child: Rc<dyn Window> = edit.clone();
        self.wnd.add_child(child);
        edit.wnd().realize();
    }
    fn create_button(&self, cp: &CreateParams) {
        if self.button().is_some() {
            return;
        }
        let mut params = cp.clone();
        params.flags = WINDOW_VISIBLE | WINDOW_BORDER | WINDOW_BACKGROUND;
        params.background_color = CfxColor::make_rgb_bytes(220, 220, 220);
        params.border_color = CfxColor::make_gray(0.0);
        params.border_width = 2;
        params.border_style = BorderStyle::Beveled;
        params.cursor_type = CursorStyle::Arrow;
        let button = CpwlCbButton::new(params, self.wnd.clone_attached_data());
        *self.button.borrow_mut() = Some(Rc::downgrade(&button));
        let child: Rc<dyn Window> = button.clone();
        self.wnd.add_child(child);
        button.wnd().realize();
    }
    fn create_list_box(&self, cp: &CreateParams) {
        if self.list().is_some() {
            return;
        }
        let mut params = cp.clone();
        params.flags = WINDOW_BORDER | WINDOW_BACKGROUND | LISTBOX_HOVER_SEL | WINDOW_VSCROLL;
        params.border_style = BorderStyle::Solid;
        params.border_width = 1;
        params.cursor_type = CursorStyle::Arrow;
        params.rect = CfxFloatRect::default();
        params.font_size = if cp.flags & WINDOW_AUTO_FONT_SIZE != 0 {
            12.0
        } else {
            cp.font_size
        };
        if cp.border_color.color_type == ColorType::Transparent {
            params.border_color = CfxColor::make_gray(0.0);
        }
        if cp.background_color.color_type == ColorType::Transparent {
            params.background_color = CfxColor::make_gray(1.0);
        }
        let list = CpwlCbListBox::new(params, self.wnd.clone_attached_data());
        *self.list.borrow_mut() = Some(Rc::downgrade(&list));
        let child: Rc<dyn Window> = list.clone();
        self.wnd.add_child(child);
        if let Some(edit) = self.edit() {
            list.set_text_direction(edit.text_direction());
        }
        list.wnd().realize();
    }
    pub fn set_popup(&self, popup: bool) -> bool {
        let Some(list) = self.list() else {
            return self.wnd.is_alive();
        };
        if popup == self.is_popup.get() {
            return self.wnd.is_alive();
        }
        let height = list.content_rect().height();
        if !(height > 0.0 && height.abs() >= 0.0001) {
            return self.wnd.is_alive();
        }
        if !popup {
            self.is_popup.set(false);
            return self.wnd.move_window(self.old_window_rect.get(), true, true);
        }
        let Some(notify) = self.wnd.filler_notify() else {
            return false;
        };
        if notify.on_popup_pre_open(self.wnd.attached_data(), 0) {
            return self.wnd.is_alive();
        }
        if !self.wnd.is_alive() {
            return false;
        }
        let border = list.wnd().border_width() as f32 * 2.0;
        let min = if list.count() > 3 {
            list.first_height() * 3.0 + border
        } else {
            0.0
        };
        let max = height + border;
        let (bottom, popup_height) = notify.query_where_popup(self.wnd.attached_data(), min, max);
        if !self.wnd.is_alive() {
            return false;
        }
        if !(popup_height > 0.0 && popup_height.abs() >= 0.0001) {
            return true;
        }
        let mut rect = self.wnd.window_rect();
        self.old_window_rect.set(rect);
        self.is_popup.set(true);
        self.is_bottom.set(bottom);
        if bottom {
            rect.bottom -= popup_height
        } else {
            rect.top += popup_height
        }
        if !self.wnd.move_window(rect, true, true) {
            return false;
        }
        notify.on_popup_post_open(self.wnd.attached_data(), 0);
        self.wnd.is_alive()
    }
    fn selection_popup_callbacks(&self, flags: u32) -> bool {
        let Some(notify) = self.wnd.filler_notify() else {
            return false;
        };
        if notify.on_popup_pre_open(self.wnd.attached_data(), flags) || !self.wnd.is_alive() {
            return false;
        }
        !notify.on_popup_post_open(self.wnd.attached_data(), flags) && self.wnd.is_alive()
    }
}
impl Window for CpwlComboBox {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn wnd(&self) -> &CpwlWnd {
        &self.wnd
    }
    fn on_destroy(&self) {
        self.list.borrow_mut().take();
        self.button.borrow_mut().take();
        self.edit.borrow_mut().take();
    }
    fn create_child_wnd(&self, cp: &CreateParams) {
        self.create_edit(cp);
        if !self.wnd.is_alive() {
            return;
        }
        self.create_button(cp);
        if !self.wnd.is_alive() {
            return;
        }
        self.create_list_box(cp);
    }
    fn reposition_child_wnd(&self) -> bool {
        let client = self.wnd.client_rect();
        if self.is_popup.get() {
            let old_height = self.old_window_rect.get().height();
            let client_height = old_height - self.wnd.border_width() as f32 * 2.0;
            let mut list_rect = self.wnd.window_rect();
            let mut button = client;
            button.left = (button.right - 13.0).max(client.left);
            let mut edit = client;
            edit.right = (button.left - 1.0).max(edit.left);
            if self.is_bottom.get() {
                button.bottom = button.top - client_height;
                edit.bottom = edit.top - client_height;
                list_rect.top -= old_height;
            } else {
                button.top = button.bottom + client_height;
                edit.top = edit.bottom + client_height;
                list_rect.bottom += old_height;
            }
            if let Some(child) = self.button() {
                child.wnd().move_window(button, true, false);
                if !self.wnd.is_alive() {
                    return false;
                }
            }
            if let Some(child) = self.edit() {
                child.wnd().move_window(edit, true, false);
                if !self.wnd.is_alive() {
                    return false;
                }
            }
            if let Some(list) = self.list() {
                if !list.wnd().set_visible(true) || !self.wnd.is_alive() {
                    return false;
                }
                if !list.wnd().move_window(list_rect, true, false) || !self.wnd.is_alive() {
                    return false;
                }
                list.scroll_to_list_item(self.select_item.get());
                if !self.wnd.is_alive() {
                    return false;
                }
            }
            return true;
        }
        let mut button = client;
        button.left = (button.right - 13.0).max(client.left);
        if let Some(child) = self.button() {
            child.wnd().move_window(button, true, false);
            if !self.wnd.is_alive() {
                return false;
            }
        }
        let mut edit = client;
        edit.right = (button.left - 1.0).max(edit.left);
        if let Some(child) = self.edit() {
            child.wnd().move_window(edit, true, false);
            if !self.wnd.is_alive() {
                return false;
            }
        }
        if let Some(list) = self.list() {
            if !list.wnd().set_visible(false) {
                self.list.borrow_mut().take();
                return false;
            }
            if !self.wnd.is_alive() {
                return false;
            }
        }
        true
    }
    fn focus_rect(&self) -> CfxFloatRect {
        CfxFloatRect::default()
    }
    fn set_focus(&self) {
        if let Some(edit) = self.edit() {
            edit.set_focus();
        }
    }
    fn kill_focus(&self) {
        if self.set_popup(false) {
            self.wnd.kill_focus();
        }
    }
    fn set_text_direction(&self, direction: ParagraphDirection) {
        if let Some(edit) = self.edit() {
            edit.set_text_direction(direction);
        }
        if let Some(list) = self.list() {
            list.set_text_direction(direction);
        }
    }
    fn text(&self) -> Vec<u32> {
        self.edit().map_or_else(Vec::new, |e| e.text())
    }
    fn selected_text(&self) -> Vec<u32> {
        self.edit().map_or_else(Vec::new, |e| e.selected_text())
    }
    fn replace_and_keep_selection(&self, text: &[u32]) {
        if let Some(edit) = self.edit() {
            edit.replace_and_keep_selection(text);
        }
    }
    fn replace_selection(&self, text: &[u32]) {
        if let Some(edit) = self.edit() {
            edit.replace_selection(text);
        }
    }
    fn select_all_text(&self) -> bool {
        self.edit().is_some_and(|e| e.select_all_text())
    }
    fn can_undo(&self) -> bool {
        self.edit().is_some_and(|e| e.can_undo())
    }
    fn can_redo(&self) -> bool {
        self.edit().is_some_and(|e| e.can_redo())
    }
    fn undo(&self) -> bool {
        self.edit().is_some_and(|e| e.undo())
    }
    fn redo(&self) -> bool {
        self.edit().is_some_and(|e| e.redo())
    }
    fn on_key_down(&self, key: i32, flags: u32) -> bool {
        let (Some(list), Some(edit)) = (self.list(), self.edit()) else {
            return false;
        };
        self.select_item.set(-1);
        let movement = match key {
            FWL_VKEY_Up => Some(list.cur_sel() > 0),
            FWL_VKEY_Down => Some(list.cur_sel() < list.count() - 1),
            _ => None,
        };
        if let Some(can_move) = movement {
            if can_move {
                if !self.selection_popup_callbacks(flags) {
                    return false;
                }
                if list.is_movement_key(key) {
                    if list.on_movement_key_down(key, flags) || !self.wnd.is_alive() {
                        return false;
                    }
                    self.set_select_text();
                }
            }
            return true;
        }
        self.wnd.has_flag(COMBOBOX_ALLOW_CUSTOM_TEXT) && edit.on_key_down(key, flags)
    }
    fn on_char(&self, ch: u16, flags: u32) -> bool {
        let (Some(list), Some(edit)) = (self.list(), self.edit()) else {
            return false;
        };
        match ch {
            13 => {
                if !self.set_popup(!self.is_popup()) {
                    return false;
                }
                self.set_select_text();
                return true;
            }
            32 if !self.wnd.has_flag(COMBOBOX_ALLOW_CUSTOM_TEXT) => {
                if !self.is_popup() {
                    if !self.set_popup(true) {
                        return false;
                    }
                    self.set_select_text();
                }
                return true;
            }
            _ => {}
        }
        self.select_item.set(-1);
        if self.wnd.has_flag(COMBOBOX_ALLOW_CUSTOM_TEXT) {
            return edit.on_char(ch, flags);
        }
        if !self.selection_popup_callbacks(flags) {
            return false;
        }
        if !list.is_char(ch, flags) {
            return false;
        }
        list.on_char_notify(ch, flags)
    }
    fn notify_l_button_down(&self, child: &dyn Window, _pos: CfxPointF) {
        if self
            .button()
            .is_some_and(|button| std::ptr::eq(button.wnd(), child.wnd()))
        {
            self.set_popup(!self.is_popup.get());
        }
    }
    fn notify_l_button_up(&self, child: &dyn Window, _pos: CfxPointF) {
        let (Some(edit), Some(list)) = (self.edit(), self.list()) else {
            return;
        };
        if !std::ptr::eq(list.wnd(), child.wnd()) {
            return;
        }
        self.set_select_text();
        self.select_all_text();
        edit.set_focus();
        if self.wnd.is_alive() {
            self.set_popup(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ipwl_fillernotify::{BeforeKeystrokeResult, FillerNotify};
    use super::*;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfdoc::cpdf_bafontmap::CpdfBaFontMap;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn string(text: Vec<u32>) -> String {
        text.into_iter().filter_map(char::from_u32).collect()
    }
    struct Host {
        events: RefCell<Vec<&'static str>>,
        bottom: Cell<bool>,
        combo: RefCell<Option<Weak<CpwlComboBox>>>,
        destroy_on_pre: Cell<bool>,
    }
    impl Host {
        fn new() -> Rc<Self> {
            Rc::new(Self {
                events: RefCell::new(Vec::new()),
                bottom: Cell::new(true),
                combo: RefCell::new(None),
                destroy_on_pre: Cell::new(false),
            })
        }
    }
    impl FillerNotify for Host {
        fn invalidate_rect(&self, _data: Option<&dyn PerWindowData>, _rect: CfxFloatRect) {}
        fn output_selected_rect(&self, _data: Option<&dyn PerWindowData>, _rect: CfxFloatRect) {}
        fn is_selection_implemented(&self) -> bool {
            false
        }
        fn set_cursor(&self, _cursor: CursorStyle) {}
        fn query_where_popup(
            &self,
            _data: Option<&dyn PerWindowData>,
            min: f32,
            max: f32,
        ) -> (bool, f32) {
            self.events.borrow_mut().push("query");
            assert!(max >= min);
            (self.bottom.get(), 42.0)
        }
        fn on_before_key_stroke(
            &self,
            _data: Option<&dyn PerWindowData>,
            _change: &mut Vec<u32>,
            _ex: &[u32],
            _start: i32,
            _end: i32,
            _key: bool,
            _flags: u32,
        ) -> BeforeKeystrokeResult {
            BeforeKeystrokeResult {
                rc: true,
                exit: false,
            }
        }
        fn on_popup_pre_open(&self, _data: Option<&dyn PerWindowData>, _flags: u32) -> bool {
            self.events.borrow_mut().push("pre");
            if self.destroy_on_pre.get() {
                let combo = self.combo.borrow().as_ref().and_then(Weak::upgrade);
                if let Some(combo) = combo {
                    combo.wnd.destroy();
                }
            }
            false
        }
        fn on_popup_post_open(&self, _data: Option<&dyn PerWindowData>, _flags: u32) -> bool {
            self.events.borrow_mut().push("post");
            false
        }
    }
    fn combo(editable: bool) -> (Rc<RefCell<CpdfDocument>>, Rc<Host>, Rc<CpwlComboBox>) {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        let mut annot = CpdfDictionary::new();
        annot.set_new_name_for(b"Subtype", b"Widget");
        let map = Rc::new(CpdfBaFontMap::new(&document, retain_object(annot), b"N"));
        let host = Host::new();
        let notify: Rc<dyn FillerNotify> = host.clone();
        let mut params = CreateParams::new(None, Some(&notify), None);
        params.rect = CfxFloatRect::new(0.0, 0.0, 100.0, 20.0);
        params.flags = WINDOW_VISIBLE | WINDOW_BORDER;
        if editable {
            params.flags |= COMBOBOX_ALLOW_CUSTOM_TEXT;
        }
        params.font_map = Some(map);
        params.text_color = CfxColor::make_gray(0.0);
        params.font_size = 12.0;
        let combo = CpwlComboBox::new(params, None);
        *host.combo.borrow_mut() = Some(Rc::downgrade(&combo));
        combo.wnd.realize();
        for option in ["Apple", "Banana", "Cherry", "Date"] {
            combo.add_string(&wide(option));
        }
        (document, host, combo)
    }
    #[test]
    fn source_combo_selection_fragments_and_option_text() {
        let (_document, _host, combo) = combo(false);
        assert!(combo.set_select(1));
        assert_eq!(string(combo.text()), "Banana");
        assert!(combo.selected_text().is_empty());
        combo.set_select_text();
        assert_eq!(string(combo.selected_text()), "Banana");
        // GetSelectedTextFragmentsNormal source vectors, exercised through the
        // actual child edit window rather than the still-pending FORM owner.
        for (start, end, expected) in [
            (0, 0, ""),
            (0, 1, "B"),
            (0, -1, "Banana"),
            (-8, -1, ""),
            (4, 1, "ana"),
            (1, 4, "ana"),
            (5, 6, "a"),
        ] {
            combo.set_edit_selection(start, end);
            assert_eq!(string(combo.selected_text()), expected);
        }
        assert!(combo.set_select(2));
        combo.set_select_text();
        assert_eq!(string(combo.selected_text()), "Cherry");
        // CPWL_Edit::OnCharInternal consumes read-only input without mutation;
        // the source embedder's false result belongs to its formfiller layer.
        assert!(combo.edit().unwrap().on_char(b'a' as u16, 0));
        assert_eq!(string(combo.text()), "Cherry");
        assert_eq!(string(combo.selected_text()), "Cherry");
    }
    #[test]
    fn editable_combo_child_typing_replacement_and_undo() {
        let (_document, _host, combo) = combo(true);
        combo.set_focus();
        for ch in b"ABC" {
            assert!(combo.on_char(u16::from(*ch), 0));
        }
        assert_eq!(string(combo.text()), "ABC");
        assert_eq!(combo.selected_index(), -1);
        combo.set_edit_selection(1, 2);
        combo.replace_and_keep_selection(&wide("xy"));
        assert_eq!(string(combo.text()), "AxyC");
        assert_eq!(string(combo.selected_text()), "xy");
        assert!(combo.can_undo());
        assert!(combo.undo());
        assert_eq!(string(combo.text()), "ABC");
        assert!(combo.redo());
        assert_eq!(string(combo.text()), "AxyC");
    }
    #[test]
    fn popup_geometry_above_below_close_and_selection() {
        let (_document, host, combo) = combo(false);
        assert!(combo.set_select(1));
        let original = combo.wnd.window_rect();
        assert!(combo.set_popup(true));
        assert!(combo.is_popup());
        assert_eq!(
            combo.wnd.window_rect(),
            CfxFloatRect::new(
                original.left,
                original.bottom - 42.0,
                original.right,
                original.top
            )
        );
        assert!(combo.list().unwrap().wnd().is_visible());
        assert_eq!(&*host.events.borrow(), &["pre", "query", "post"]);
        assert!(combo.set_popup(false));
        assert_eq!(combo.wnd.window_rect(), original);
        assert!(!combo.list().unwrap().wnd().is_visible());
        host.bottom.set(false);
        assert!(combo.set_popup(true));
        assert_eq!(
            combo.wnd.window_rect(),
            CfxFloatRect::new(
                original.left,
                original.bottom,
                original.right,
                original.top + 42.0
            )
        );
        assert!(combo.set_popup(false));
        assert!(combo.on_key_down(FWL_VKEY_Down, 0));
        assert_eq!(combo.selected_index(), 2);
        assert_eq!(string(combo.text()), "Cherry");
    }
    #[test]
    fn popup_callback_can_destroy_retained_parent_and_children() {
        let (_document, host, combo) = combo(false);
        let edit = combo.edit().unwrap();
        let list = combo.list().unwrap();
        host.destroy_on_pre.set(true);
        assert!(!combo.set_popup(true));
        assert!(!combo.wnd.is_alive());
        assert!(!edit.wnd().is_alive());
        assert!(!list.wnd().is_alive());
        assert_eq!(&*host.events.borrow(), &["pre"]);
    }
}
