// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Combo popup list from `fpdfsdk/pwl/cpwl_cblistbox.{h,cpp}`.
use super::cpwl_combo_box::CpwlComboBox;
use super::cpwl_list_box::CpwlListBox;
use super::cpwl_list_ctrl::ListNotify;
use super::cpwl_scroll_bar::PwlScrollInfo;
use super::cpwl_wnd::{CpwlWnd, CreateParams, Window};
use super::ipwl_fillernotify::PerWindowData;
use super::pwl_draw::PwlDraw;
use crate::fpdfsdk::fpdf_fwlevent::*;
use crate::fxcrt::cfx_bidi_resolver::ParagraphDirection;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use std::any::Any;
use std::rc::Rc;
pub struct CpwlCbListBox {
    pub(super) list: CpwlListBox,
}
impl CpwlCbListBox {
    pub fn new(cp: CreateParams, attached: Option<Box<dyn PerWindowData>>) -> Rc<Self> {
        let result = Rc::new(Self {
            list: CpwlListBox::unbound(cp, attached),
        });
        let window: Rc<dyn Window> = result.clone();
        result.list.wnd.bind_owner(&window);
        let notify: Rc<dyn ListNotify> = result.clone();
        result.list.bind_notify(&notify);
        result
    }
    pub fn is_movement_key(&self, key: i32) -> bool {
        matches!(
            key,
            FWL_VKEY_Up
                | FWL_VKEY_Down
                | FWL_VKEY_Home
                | FWL_VKEY_Left
                | FWL_VKEY_End
                | FWL_VKEY_Right
        )
    }
    pub fn on_movement_key_down(&self, key: i32, flags: u32) -> bool {
        if !self.list.movement(key, flags) {
            return false;
        }
        self.list.on_notify_selection_changed(true, flags)
    }
    pub fn is_char(&self, ch: u16, flags: u32) -> bool {
        self.list.list_ctrl.on_char(
            ch,
            flags & FWL_EVENTFLAG_ShiftKey != 0,
            flags & FWL_EVENTFLAG_ControlKey != 0,
        )
    }
    pub fn on_char_notify(&self, _ch: u16, flags: u32) -> bool {
        if let Some(parent) = self.list.wnd.parent() {
            if let Some(combo) = parent.as_any().downcast_ref::<CpwlComboBox>() {
                combo.set_select_text();
            }
        }
        self.list.on_notify_selection_changed(true, flags)
    }
    pub fn add_string(&self, s: &[u32]) {
        self.list.add_string(s)
    }
    pub fn select(&self, i: i32) {
        self.list.select(i)
    }
    pub fn cur_sel(&self) -> i32 {
        self.list.cur_sel()
    }
    pub fn count(&self) -> i32 {
        self.list.count()
    }
    pub fn content_rect(&self) -> CfxFloatRect {
        self.list.content_rect()
    }
    pub fn first_height(&self) -> f32 {
        self.list.first_height()
    }
    pub fn scroll_to_list_item(&self, i: i32) {
        self.list.scroll_to_list_item(i)
    }
}
impl Window for CpwlCbListBox {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn wnd(&self) -> &CpwlWnd {
        &self.list.wnd
    }
    fn on_created(&self) {
        self.list.on_created()
    }
    fn on_destroy(&self) {
        self.list.on_destroy()
    }
    fn draw_this_appearance(&self, d: &mut dyn PwlDraw, m: CfxMatrix) {
        self.list.draw_this_appearance(d, m)
    }
    fn on_key_down(&self, k: i32, f: u32) -> bool {
        self.list.on_key_down(k, f)
    }
    fn on_char(&self, c: u16, f: u32) -> bool {
        self.list.on_char(c, f)
    }
    fn on_l_button_down(&self, f: u32, p: CfxPointF) -> bool {
        self.list.on_l_button_down(f, p)
    }
    fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        self.list.wnd.base_on_l_button_up(flags, point);
        if !self.list.mouse_down.replace(false) {
            return true;
        }
        self.list.wnd.release_capture();
        if !self.list.wnd.client_hit_test(point) {
            return true;
        }
        if let Some(parent) = self.list.wnd.parent() {
            parent.notify_l_button_up(self, point);
        }
        if !self.list.wnd.is_alive() {
            return false;
        }
        !self.list.on_notify_selection_changed(false, flags)
    }
    fn on_mouse_move(&self, f: u32, p: CfxPointF) -> bool {
        self.list.on_mouse_move(f, p)
    }
    fn on_mouse_wheel(&self, f: u32, p: CfxPointF, d: CfxPointF) -> bool {
        self.list.on_mouse_wheel(f, p, d)
    }
    fn set_scroll_info(&self, i: &PwlScrollInfo) {
        self.list.set_scroll_info(i)
    }
    fn set_scroll_position(&self, p: f32) {
        self.list.set_scroll_position(p)
    }
    fn scroll_window_vertically(&self, p: f32) {
        self.list.scroll_window_vertically(p)
    }
    fn reposition_child_wnd(&self) -> bool {
        self.list.reposition_child_wnd()
    }
    fn focus_rect(&self) -> CfxFloatRect {
        self.list.focus_rect()
    }
    fn text(&self) -> Vec<u32> {
        self.list.text()
    }
    fn font_size(&self) -> f32 {
        self.list.font_size()
    }
    fn set_font_size(&self, s: f32) {
        self.list.set_font_size(s)
    }
    fn set_text_direction(&self, d: ParagraphDirection) {
        self.list.set_text_direction(d)
    }
}
impl ListNotify for CpwlCbListBox {
    fn set_scroll_info_y(&self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) {
        self.list.set_scroll_info_y(a, b, c, d, e, f)
    }
    fn set_scroll_pos_y(&self, y: f32) {
        self.list.set_scroll_pos_y(y)
    }
    fn invalidate_rect(&self, r: CfxFloatRect) -> bool {
        ListNotify::invalidate_rect(&self.list, r)
    }
}
