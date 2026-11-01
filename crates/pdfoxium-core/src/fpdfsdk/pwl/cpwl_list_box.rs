// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! List-box window from `fpdfsdk/pwl/cpwl_list_box.{h,cpp}`.
use super::cpwl_list_ctrl::{CpwlListCtrl, ListNotify};
use super::cpwl_scroll_bar::PwlScrollInfo;
use super::cpwl_wnd::*;
use super::ipwl_fillernotify::PerWindowData;
use super::pwl_draw::PwlDraw;
use crate::fpdfsdk::fpdf_fwlevent::*;
use crate::fxcrt::cfx_bidi_resolver::ParagraphDirection;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

pub struct CpwlListBox {
    pub(super) wnd: CpwlWnd,
    pub(super) mouse_down: Cell<bool>,
    hover_sel: Cell<bool>,
    pub(super) list_ctrl: CpwlListCtrl,
    notify_owner: RefCell<Option<Weak<dyn ListNotify>>>,
}
impl CpwlListBox {
    pub fn new(cp: CreateParams, attached: Option<Box<dyn PerWindowData>>) -> Rc<Self> {
        let result = Rc::new(Self::unbound(cp, attached));
        let window: Rc<dyn Window> = result.clone();
        result.wnd.bind_owner(&window);
        let notify: Rc<dyn ListNotify> = result.clone();
        result.bind_notify(&notify);
        result
    }
    pub(super) fn unbound(cp: CreateParams, attached: Option<Box<dyn PerWindowData>>) -> Self {
        Self {
            wnd: CpwlWnd::new(cp, attached),
            mouse_down: Cell::new(false),
            hover_sel: Cell::new(false),
            list_ctrl: CpwlListCtrl::new(),
            notify_owner: RefCell::new(None),
        }
    }
    pub(super) fn bind_notify(&self, notify: &Rc<dyn ListNotify>) {
        *self.notify_owner.borrow_mut() = Some(Rc::downgrade(notify));
    }
    pub fn on_created(&self) {
        self.list_ctrl.set_font_map(self.wnd.font_map());
        let notify = self.notify_owner.borrow().as_ref().and_then(Weak::upgrade);
        self.list_ctrl.set_notify(notify.as_ref());
        self.set_hover_sel(self.wnd.has_flag(LISTBOX_HOVER_SEL));
        self.list_ctrl
            .set_multiple_sel(self.wnd.has_flag(LISTBOX_MULTIPLE_SEL));
        self.list_ctrl
            .set_font_size(self.wnd.creation_params().font_size);
    }
    pub fn on_destroy(&self) {
        self.list_ctrl.set_notify(None);
    }
    pub fn draw_this_appearance(&self, draw: &mut dyn PwlDraw, matrix: CfxMatrix) {
        self.wnd.base_draw_this_appearance(draw, matrix);
        let plate = self.list_ctrl.plate_rect();
        let list = self.list_rect();
        let client = self.client_rect();
        for index in 0..self.list_ctrl.count() {
            let mut item = self.list_ctrl.item_rect(index);
            if item.bottom > plate.top || item.top < plate.bottom {
                continue;
            }
            let offset = CfxPointF::new(item.left, (item.top + item.bottom) * 0.5);
            let Some(edit) = self.list_ctrl.item_edit(index) else {
                continue;
            };
            let content = edit.borrow().content_rect();
            item.intersect(if content.width() > client.width() {
                list
            } else {
                client
            });
            let notify = self.wnd.filler_notify();
            if self.list_ctrl.is_item_selected(index) {
                if notify
                    .as_ref()
                    .is_some_and(|n| n.is_selection_implemented())
                {
                    let plan = edit.borrow().draw_edit_plan(
                        matrix,
                        self.wnd.text_color().to_fx_color(255),
                        list,
                        offset,
                        None,
                        true,
                    );
                    plan.draw_observed(draw, notify.as_deref(), self.wnd.attached_data(), &|| {
                        self.wnd.is_alive()
                    });
                    // Release the text-editor borrow before embedder notification.
                    if !self.wnd.is_alive() {
                        return;
                    }
                    if let Some(notify) = notify {
                        notify.output_selected_rect(self.wnd.attached_data(), item);
                    }
                    if !self.wnd.is_alive() {
                        return;
                    }
                } else {
                    draw.fill_rect(matrix, item, 0xff003371);
                    let plan = edit
                        .borrow()
                        .draw_edit_plan(matrix, 0xffffffff, list, offset, None, false);
                    plan.draw_observed(draw, notify.as_deref(), self.wnd.attached_data(), &|| {
                        self.wnd.is_alive()
                    });
                }
            } else {
                let plan = edit.borrow().draw_edit_plan(
                    matrix,
                    self.wnd.text_color().to_fx_color(255),
                    list,
                    offset,
                    None,
                    notify
                        .as_ref()
                        .is_some_and(|n| n.is_selection_implemented()),
                );
                plan.draw_observed(draw, notify.as_deref(), None, &|| self.wnd.is_alive());
            }
            if !self.wnd.is_alive() {
                return;
            }
        }
    }
    pub(super) fn movement(&self, key: i32, flags: u32) -> bool {
        let shift = flags & FWL_EVENTFLAG_ShiftKey != 0;
        let ctrl = flags & FWL_EVENTFLAG_ControlKey != 0;
        match key {
            FWL_VKEY_Up => self.list_ctrl.on_vk_up(shift, ctrl),
            FWL_VKEY_Down => self.list_ctrl.on_vk_down(shift, ctrl),
            FWL_VKEY_Home => self.list_ctrl.on_vk_home(shift, ctrl),
            FWL_VKEY_Left => self.list_ctrl.on_vk_left(shift, ctrl),
            FWL_VKEY_End => self.list_ctrl.on_vk_end(shift, ctrl),
            FWL_VKEY_Right => self.list_ctrl.on_vk_right(shift, ctrl),
            _ => return false,
        }
        true
    }
    pub fn on_key_down(&self, key: i32, flags: u32) -> bool {
        self.wnd.base_on_key_down(key, flags);
        if !self.movement(key, flags) {
            return false;
        }
        self.on_notify_selection_changed(true, flags);
        true
    }
    pub fn on_char(&self, ch: u16, flags: u32) -> bool {
        self.wnd.base_on_char(ch, flags);
        if !self.list_ctrl.on_char(
            ch,
            flags & FWL_EVENTFLAG_ShiftKey != 0,
            flags & FWL_EVENTFLAG_ControlKey != 0,
        ) {
            return false;
        }
        self.on_notify_selection_changed(true, flags);
        true
    }
    pub fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_down(flags, point);
        if self.wnd.client_hit_test(point) {
            self.mouse_down.set(true);
            self.set_focus();
            self.wnd.set_capture();
            self.list_ctrl.on_mouse_down(
                point,
                flags & FWL_EVENTFLAG_ShiftKey != 0,
                flags & FWL_EVENTFLAG_ControlKey != 0,
            );
        }
        true
    }
    pub fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_up(flags, point);
        if self.mouse_down.replace(false) {
            self.wnd.release_capture();
        }
        self.on_notify_selection_changed(false, flags);
        true
    }
    pub fn set_hover_sel(&self, value: bool) {
        self.hover_sel.set(value);
    }
    pub fn on_mouse_move(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_mouse_move(flags, point);
        if self.hover_sel.get() && !self.wnd.is_capture_mouse() && self.wnd.client_hit_test(point) {
            self.list_ctrl.select(self.list_ctrl.item_index(point));
        }
        if self.mouse_down.get() {
            self.list_ctrl.on_mouse_move(
                point,
                flags & FWL_EVENTFLAG_ShiftKey != 0,
                flags & FWL_EVENTFLAG_ControlKey != 0,
            );
        }
        true
    }
    pub fn set_scroll_info(&self, info: &PwlScrollInfo) {
        if let Some(scroll) = self.wnd.v_scroll_bar() {
            scroll.set_scroll_info(info);
        }
    }
    pub fn set_scroll_position(&self, pos: f32) {
        if let Some(scroll) = self.wnd.v_scroll_bar() {
            scroll.set_scroll_position(pos);
        }
    }
    pub fn scroll_window_vertically(&self, pos: f32) {
        self.list_ctrl.set_scroll_pos(CfxPointF::new(0.0, pos));
    }
    pub fn reposition_child_wnd(&self) -> bool {
        if !self.wnd.base_reposition_child_wnd() {
            return false;
        }
        self.list_ctrl.set_plate_rect(self.list_rect());
        self.wnd.is_alive()
    }
    pub fn on_notify_selection_changed(&self, key_down: bool, flags: u32) -> bool {
        let Some(notify) = self.wnd.filler_notify() else {
            return false;
        };
        let mut change = self.text();
        let end = i32::try_from(change.len()).unwrap_or(i32::MAX);
        let result = notify.on_before_key_stroke(
            self.wnd.attached_data(),
            &mut change,
            &[],
            0,
            end,
            key_down,
            flags,
        );
        self.wnd.is_alive() && result.exit
    }
    pub fn focus_rect(&self) -> CfxFloatRect {
        if self.list_ctrl.is_multiple_sel() {
            let mut rect = self.list_ctrl.item_rect(self.list_ctrl.caret());
            rect.intersect(self.client_rect());
            rect
        } else {
            self.wnd.focus_rect()
        }
    }
    pub fn add_string(&self, text: &[u32]) {
        self.list_ctrl.add_string(text);
    }
    pub fn text(&self) -> Vec<u32> {
        self.list_ctrl.text()
    }
    pub fn set_font_size(&self, size: f32) {
        self.list_ctrl.set_font_size(size);
    }
    pub fn font_size(&self) -> f32 {
        self.list_ctrl.font_size()
    }
    pub fn set_text_direction(&self, direction: ParagraphDirection) {
        self.list_ctrl.set_text_direction(direction);
    }
    pub fn select(&self, index: i32) {
        self.list_ctrl.select(index);
    }
    pub fn deselect(&self, index: i32) {
        self.list_ctrl.deselect(index);
    }
    pub fn set_caret(&self, index: i32) {
        self.list_ctrl.set_caret(index);
    }
    pub fn set_top_visible_index(&self, index: i32) {
        self.list_ctrl.set_top_item(index);
    }
    pub fn scroll_to_list_item(&self, index: i32) {
        self.list_ctrl.scroll_to_list_item(index);
    }
    pub fn is_multiple_sel(&self) -> bool {
        self.list_ctrl.is_multiple_sel()
    }
    pub fn caret_index(&self) -> i32 {
        self.list_ctrl.caret()
    }
    pub fn cur_sel(&self) -> i32 {
        self.list_ctrl.selected_index()
    }
    pub fn is_item_selected(&self, index: i32) -> bool {
        self.list_ctrl.is_item_selected(index)
    }
    pub fn top_visible_index(&self) -> i32 {
        self.list_ctrl
            .scroll_to_list_item(self.list_ctrl.first_selected());
        self.list_ctrl.top_item()
    }
    pub fn count(&self) -> i32 {
        self.list_ctrl.count()
    }
    pub fn content_rect(&self) -> CfxFloatRect {
        self.list_ctrl.content_rect()
    }
    pub fn first_height(&self) -> f32 {
        self.list_ctrl.first_height()
    }
    pub fn list_rect(&self) -> CfxFloatRect {
        let width = (self.wnd.border_width() + self.wnd.inner_border_width()) as f32;
        self.wnd.window_rect().get_deflated(width, width)
    }
    pub fn on_mouse_wheel(&self, flags: u32, _point: CfxPointF, delta: CfxPointF) -> bool {
        let shift = flags & FWL_EVENTFLAG_ShiftKey != 0;
        let ctrl = flags & FWL_EVENTFLAG_ControlKey != 0;
        if delta.y < 0.0 {
            self.list_ctrl.on_vk_down(shift, ctrl)
        } else {
            self.list_ctrl.on_vk_up(shift, ctrl)
        }
        self.on_notify_selection_changed(false, flags);
        true
    }
    pub fn client_rect(&self) -> CfxFloatRect {
        self.wnd.client_rect()
    }
}
impl Window for CpwlListBox {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn wnd(&self) -> &CpwlWnd {
        &self.wnd
    }
    fn on_created(&self) {
        self.on_created()
    }
    fn on_destroy(&self) {
        self.on_destroy()
    }
    fn draw_this_appearance(&self, d: &mut dyn PwlDraw, m: CfxMatrix) {
        self.draw_this_appearance(d, m)
    }
    fn on_key_down(&self, k: i32, f: u32) -> bool {
        self.on_key_down(k, f)
    }
    fn on_char(&self, c: u16, f: u32) -> bool {
        self.on_char(c, f)
    }
    fn on_l_button_down(&self, f: u32, p: CfxPointF) -> bool {
        self.on_l_button_down(f, p)
    }
    fn on_l_button_up(&self, f: u32, p: CfxPointF) -> bool {
        self.on_l_button_up(f, p)
    }
    fn on_mouse_move(&self, f: u32, p: CfxPointF) -> bool {
        self.on_mouse_move(f, p)
    }
    fn on_mouse_wheel(&self, f: u32, p: CfxPointF, d: CfxPointF) -> bool {
        self.on_mouse_wheel(f, p, d)
    }
    fn set_scroll_info(&self, i: &PwlScrollInfo) {
        self.set_scroll_info(i)
    }
    fn set_scroll_position(&self, p: f32) {
        self.set_scroll_position(p)
    }
    fn scroll_window_vertically(&self, p: f32) {
        self.scroll_window_vertically(p)
    }
    fn reposition_child_wnd(&self) -> bool {
        self.reposition_child_wnd()
    }
    fn focus_rect(&self) -> CfxFloatRect {
        self.focus_rect()
    }
    fn text(&self) -> Vec<u32> {
        self.text()
    }
    fn font_size(&self) -> f32 {
        self.font_size()
    }
    fn set_font_size(&self, s: f32) {
        self.set_font_size(s)
    }
    fn set_text_direction(&self, d: ParagraphDirection) {
        self.set_text_direction(d)
    }
}
impl ListNotify for CpwlListBox {
    fn set_scroll_info_y(
        &self,
        min: f32,
        max: f32,
        content_min: f32,
        content_max: f32,
        small_step: f32,
        big_step: f32,
    ) {
        let info = PwlScrollInfo {
            plate_width: max - min,
            content_min,
            content_max,
            small_step,
            big_step,
        };
        self.set_scroll_info(&info);
        let Some(scroll) = self.wnd.v_scroll_bar() else {
            return;
        };
        let range = info.content_max - info.content_min;
        let fits = info.plate_width > range || (info.plate_width - range).abs() < 0.0001;
        if fits {
            if scroll.wnd().is_visible() && scroll.wnd().set_visible(false) {
                self.reposition_child_wnd();
            }
        } else if !scroll.wnd().is_visible() && scroll.wnd().set_visible(true) {
            self.reposition_child_wnd();
        }
    }
    fn set_scroll_pos_y(&self, y: f32) {
        self.set_scroll_position(y)
    }
    fn invalidate_rect(&self, rect: CfxFloatRect) -> bool {
        self.wnd.invalidate_rect(Some(rect))
    }
}
