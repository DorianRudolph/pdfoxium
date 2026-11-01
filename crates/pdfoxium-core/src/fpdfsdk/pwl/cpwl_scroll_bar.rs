// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Scroll range, thumb geometry, dragging and repeat timers from
//! `fpdfsdk/pwl/cpwl_scroll_bar.{h,cpp}`.
use super::cpwl_sbbutton::{CpwlSbButton, Type};
use super::cpwl_wnd::{
    CpwlWnd, CreateParams, Window, WINDOW_AUTO_TRANSPARENT, WINDOW_BACKGROUND, WINDOW_BORDER,
    WINDOW_NO_REFRESH_CLIP, WINDOW_VISIBLE,
};
use super::ipwl_fillernotify::{CursorStyle, PerWindowData};
use super::pwl_draw::PwlDraw;
use crate::fpdfsdk::cpdfsdk_baannot::BorderStyle;
use crate::fxcrt::cfx_timer::{CfxTimer, TimerCallback};
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use crate::fxge::dib::fx_dib::argb_encode;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

fn equal(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.0001
}
fn bigger(a: f32, b: f32) -> bool {
    a > b && !equal(a, b)
}
fn smaller(a: f32, b: f32) -> bool {
    a < b && !equal(a, b)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PwlScrollInfo {
    pub content_min: f32,
    pub content_max: f32,
    pub plate_width: f32,
    pub big_step: f32,
    pub small_step: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PwlFloatRange {
    pub min: f32,
    pub max: f32,
}
impl PwlFloatRange {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn set(&mut self, min: f32, max: f32) {
        self.min = min.min(max);
        self.max = min.max(max);
    }
    pub fn contains(&self, value: f32) -> bool {
        (bigger(value, self.min) || equal(value, self.min))
            && (smaller(value, self.max) || equal(value, self.max))
    }
    pub fn width(&self) -> f32 {
        self.max - self.min
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PwlScrollPrivateData {
    pub scroll_range: PwlFloatRange,
    pub client_width: f32,
    pub scroll_pos: f32,
    pub big_step: f32,
    pub small_step: f32,
}
impl Default for PwlScrollPrivateData {
    fn default() -> Self {
        Self {
            scroll_range: PwlFloatRange::default(),
            client_width: 0.0,
            scroll_pos: 0.0,
            big_step: 10.0,
            small_step: 1.0,
        }
    }
}
impl PwlScrollPrivateData {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn set_scroll_range(&mut self, min: f32, max: f32) {
        self.scroll_range.set(min, max);
        if smaller(self.scroll_pos, self.scroll_range.min) {
            self.scroll_pos = self.scroll_range.min;
        }
        if bigger(self.scroll_pos, self.scroll_range.max) {
            self.scroll_pos = self.scroll_range.max;
        }
    }
    pub fn set_client_width(&mut self, width: f32) {
        self.client_width = width;
    }
    pub fn set_small_step(&mut self, step: f32) {
        self.small_step = step;
    }
    pub fn set_big_step(&mut self, step: f32) {
        self.big_step = step;
    }
    pub fn set_pos(&mut self, pos: f32) -> bool {
        if self.scroll_range.contains(pos) {
            self.scroll_pos = pos;
            true
        } else {
            false
        }
    }
    pub fn add_small(&mut self) {
        if !self.set_pos(self.scroll_pos + self.small_step) {
            self.set_pos(self.scroll_range.max);
        }
    }
    pub fn sub_small(&mut self) {
        if !self.set_pos(self.scroll_pos - self.small_step) {
            self.set_pos(self.scroll_range.min);
        }
    }
    pub fn add_big(&mut self) {
        if !self.set_pos(self.scroll_pos + self.big_step) {
            self.set_pos(self.scroll_range.max);
        }
    }
    pub fn sub_big(&mut self) {
        if !self.set_pos(self.scroll_pos - self.big_step) {
            self.set_pos(self.scroll_range.min);
        }
    }
}

pub struct CpwlScrollBar {
    pub wnd: CpwlWnd,
    self_weak: Weak<Self>,
    origin_info: Cell<PwlScrollInfo>,
    min_button: RefCell<Weak<CpwlSbButton>>,
    max_button: RefCell<Weak<CpwlSbButton>>,
    pos_button: RefCell<Weak<CpwlSbButton>>,
    timer: RefCell<Option<Rc<CfxTimer>>>,
    private_data: Cell<PwlScrollPrivateData>,
    mouse_down: Cell<bool>,
    min_or_max: Cell<bool>,
    old_pos: Cell<f32>,
    old_pos_button: Cell<f32>,
}
impl CpwlScrollBar {
    pub const WIDTH: f32 = 12.0;
    pub const TRANSPARENCY: u8 = 150;
    pub fn new(mut cp: CreateParams, attached: Option<Box<dyn PerWindowData>>) -> Rc<Self> {
        cp.cursor_type = CursorStyle::Arrow;
        let result = Rc::new_cyclic(|weak| Self {
            wnd: CpwlWnd::new(cp, attached),
            self_weak: weak.clone(),
            origin_info: Cell::new(PwlScrollInfo::default()),
            min_button: RefCell::new(Weak::new()),
            max_button: RefCell::new(Weak::new()),
            pos_button: RefCell::new(Weak::new()),
            timer: RefCell::new(None),
            private_data: Cell::new(PwlScrollPrivateData::default()),
            mouse_down: Cell::new(false),
            min_or_max: Cell::new(false),
            old_pos: Cell::new(0.0),
            old_pos_button: Cell::new(0.0),
        });
        let owner: Rc<dyn Window> = result.clone();
        result.wnd.bind_owner(&owner);
        result
    }
    fn change_data(&self, change: impl FnOnce(&mut PwlScrollPrivateData)) {
        let mut data = self.private_data.get();
        change(&mut data);
        self.private_data.set(data);
    }
    pub fn scroll_position(&self) -> f32 {
        self.private_data.get().scroll_pos
    }
    fn stop_timer(&self) {
        let timer = self.timer.replace(None);
        drop(timer);
    }
    fn start_timer(&self) {
        self.stop_timer();
        let Some(owner) = self.self_weak.upgrade() else {
            return;
        };
        let callback: Rc<dyn TimerCallback> = owner;
        let timer = CfxTimer::new(self.wnd.timer_handler().as_ref(), &callback, 100);
        if self.wnd.is_alive() {
            self.timer.replace(Some(timer));
        } else {
            timer.cancel();
        }
    }
    fn set_scroll_range(&self, min: f32, max: f32, client_width: f32) {
        let Some(button) = self.pos_button.borrow().upgrade() else {
            return;
        };
        self.change_data(|data| {
            data.set_scroll_range(min, max);
            data.set_client_width(client_width);
        });
        if smaller(self.private_data.get().scroll_range.width(), 0.0) {
            button.wnd.set_visible(false);
            return;
        }
        if !button.wnd.set_visible(true) || !self.wnd.is_alive() {
            return;
        }
        self.move_pos_button(true);
    }
    fn set_scroll_pos(&self, pos: f32) {
        let old = self.scroll_position();
        self.change_data(|data| {
            data.set_pos(pos);
        });
        if !equal(self.scroll_position(), old) {
            self.move_pos_button(true);
        }
    }
    fn set_scroll_step(&self, big: f32, small: f32) {
        self.change_data(|data| {
            data.set_big_step(big);
            data.set_small_step(small);
        });
    }
    fn scroll_area(&self) -> CfxFloatRect {
        let client = self.wnd.client_rect();
        let (Some(min), Some(max)) = (
            self.min_button.borrow().upgrade(),
            self.max_button.borrow().upgrade(),
        ) else {
            return client;
        };
        let min_height = min.wnd.window_rect().height();
        let max_height = max.wnd.window_rect().height();
        let mut area = if client.height() > min_height + max_height + 2.0 {
            CfxFloatRect::new(
                client.left,
                client.bottom + min_height + 1.0,
                client.right,
                client.top - max_height - 1.0,
            )
        } else {
            CfxFloatRect::new(
                client.left,
                client.bottom + min_height + 1.0,
                client.right,
                client.bottom + min_height + 1.0,
            )
        };
        area.normalize();
        area
    }
    fn true_to_face(&self, value: f32) -> f32 {
        let area = self.scroll_area();
        let data = self.private_data.get();
        let width = data.scroll_range.width() + data.client_width;
        area.top - value * area.height() / if width == 0.0 { 1.0 } else { width }
    }
    fn face_to_true(&self, value: f32) -> f32 {
        let area = self.scroll_area();
        let data = self.private_data.get();
        let width = data.scroll_range.width() + data.client_width;
        // A collapsed track has no draggable distance. Preserve the current
        // position instead of manufacturing NaN/infinite scroll coordinates.
        if area.height() == 0.0 {
            return data.scroll_pos;
        }
        (area.top - value) * if width == 0.0 { 1.0 } else { width } / area.height()
    }
    fn move_pos_button(&self, refresh: bool) -> bool {
        let Some(button) = self.pos_button.borrow().upgrade() else {
            return self.wnd.is_alive();
        };
        if button.wnd.is_visible() {
            let area = self.scroll_area();
            let data = self.private_data.get();
            let mut top = self.true_to_face(data.scroll_pos);
            let mut bottom = self.true_to_face(data.scroll_pos + data.client_width);
            if smaller(top - bottom, 2.0) {
                bottom = top - 2.0;
            }
            if smaller(bottom, area.bottom) {
                bottom = area.bottom;
                top = bottom + 2.0;
            }
            button.wnd.move_window(
                CfxFloatRect::new(area.left, bottom, area.right, top),
                true,
                refresh,
            );
        }
        self.wnd.is_alive()
    }
    fn notify_scroll_window(&self) {
        if let Some(parent) = self.wnd.parent() {
            parent.scroll_window_vertically(
                self.origin_info.get().content_max - self.scroll_position(),
            );
        }
    }
    fn min_button_down(&self) {
        self.change_data(PwlScrollPrivateData::sub_small);
        if !self.move_pos_button(true) {
            return;
        }
        self.notify_scroll_window();
        if !self.wnd.is_alive() {
            return;
        }
        self.min_or_max.set(true);
        self.start_timer();
    }
    fn max_button_down(&self) {
        self.change_data(PwlScrollPrivateData::add_small);
        if !self.move_pos_button(true) {
            return;
        }
        self.notify_scroll_window();
        if !self.wnd.is_alive() {
            return;
        }
        self.min_or_max.set(false);
        self.start_timer();
    }
    fn pos_button_down(&self, point: CfxPointF) {
        self.mouse_down.set(true);
        if let Some(button) = self.pos_button.borrow().upgrade() {
            self.old_pos.set(point.y);
            self.old_pos_button.set(button.wnd.window_rect().top);
        }
    }
    fn pos_button_move(&self, point: CfxPointF) {
        if (point.y - self.old_pos.get()).abs() < 1.0 {
            return;
        }
        let old = self.scroll_position();
        let mut pos = self.face_to_true(self.old_pos_button.get() + point.y - self.old_pos.get());
        if self.mouse_down.get() {
            let range = self.private_data.get().scroll_range;
            if smaller(pos, range.min) {
                pos = range.min;
            }
            if bigger(pos, range.max) {
                pos = range.max;
            }
            self.change_data(|data| {
                data.set_pos(pos);
            });
            if !equal(old, self.scroll_position()) && self.move_pos_button(true) {
                self.notify_scroll_window();
            }
        }
    }
    fn button_type(&self, child: &dyn Window) -> Option<Type> {
        if self
            .min_button
            .borrow()
            .upgrade()
            .is_some_and(|button| std::ptr::eq(button.wnd(), child.wnd()))
        {
            Some(Type::MinButton)
        } else if self
            .max_button
            .borrow()
            .upgrade()
            .is_some_and(|button| std::ptr::eq(button.wnd(), child.wnd()))
        {
            Some(Type::MaxButton)
        } else if self
            .pos_button
            .borrow()
            .upgrade()
            .is_some_and(|button| std::ptr::eq(button.wnd(), child.wnd()))
        {
            Some(Type::PosButton)
        } else {
            None
        }
    }
    fn create_buttons(&self, cp: &CreateParams) {
        let mut cp = cp.clone();
        cp.border_width = 2;
        cp.border_style = BorderStyle::Beveled;
        cp.flags = WINDOW_VISIBLE | WINDOW_BORDER | WINDOW_BACKGROUND | WINDOW_NO_REFRESH_CLIP;
        for (slot, kind) in [
            (&self.min_button, Type::MinButton),
            (&self.max_button, Type::MaxButton),
            (&self.pos_button, Type::PosButton),
        ] {
            if slot.borrow().upgrade().is_some() {
                continue;
            }
            let button = CpwlSbButton::new(cp.clone(), self.wnd.clone_attached_data(), kind);
            slot.replace(Rc::downgrade(&button));
            if kind == Type::PosButton && (!button.wnd.set_visible(false) || !self.wnd.is_alive()) {
                return;
            }
            let child: Rc<dyn Window> = button.clone();
            self.wnd.add_child(child);
            button.wnd.realize();
            if !self.wnd.is_alive() {
                return;
            }
        }
    }
}

impl Window for CpwlScrollBar {
    fn wnd(&self) -> &CpwlWnd {
        &self.wnd
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn on_destroy(&self) {
        self.stop_timer();
        self.min_button.replace(Weak::new());
        self.max_button.replace(Weak::new());
        self.pos_button.replace(Weak::new());
    }
    fn reposition_child_wnd(&self) -> bool {
        let client = self.wnd.client_rect();
        let (mut min, mut max) = (CfxFloatRect::default(), CfxFloatRect::default());
        let width = if bigger(client.height(), 9.0 * 2.0 + 2.0 + 2.0) {
            9.0
        } else {
            (client.height() - 2.0 - 2.0) / 2.0
        };
        if bigger(width, 0.0) {
            min = CfxFloatRect::new(client.left, client.top - width, client.right, client.top);
            max = CfxFloatRect::new(
                client.left,
                client.bottom,
                client.right,
                client.bottom + width,
            );
        } else if !self.wnd.set_visible(false) {
            return false;
        }
        let min_button = self.min_button.borrow().upgrade();
        if let Some(button) = min_button {
            button.wnd.move_window(min, true, false);
            if !self.wnd.is_alive() {
                return false;
            }
        }
        let max_button = self.max_button.borrow().upgrade();
        if let Some(button) = max_button {
            button.wnd.move_window(max, true, false);
            if !self.wnd.is_alive() {
                return false;
            }
        }
        self.move_pos_button(false)
    }
    fn draw_this_appearance(&self, device: &mut dyn PwlDraw, matrix: CfxMatrix) {
        let rect = self.wnd.window_rect();
        if self.wnd.is_visible() && !rect.is_empty() {
            let alpha = self.wnd.transparency();
            device.fill_rect(matrix, rect, self.wnd.background_color().to_fx_color(alpha));
            let color = argb_encode(alpha as u32, 100, 100, 100);
            device.stroke_line(
                matrix,
                CfxPointF::new(rect.left + 2.0, rect.top - 2.0),
                CfxPointF::new(rect.left + 2.0, rect.bottom + 2.0),
                color,
                1.0,
            );
            device.stroke_line(
                matrix,
                CfxPointF::new(rect.right - 2.0, rect.top - 2.0),
                CfxPointF::new(rect.right - 2.0, rect.bottom + 2.0),
                color,
                1.0,
            );
        }
    }
    fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_down(flags, point);
        if !self.wnd.is_alive() {
            return true;
        }
        if self.wnd.has_flag(WINDOW_AUTO_TRANSPARENT) && self.wnd.transparency() != 255 {
            self.wnd.set_transparency(255);
            if !self.wnd.invalidate_rect(None) {
                return true;
            }
        }
        let button = self.pos_button.borrow().upgrade();
        if let Some(button) = button.filter(|button| button.wnd.is_visible()) {
            let client = self.wnd.client_rect();
            let rect = button.wnd.window_rect();
            let mut min_area =
                CfxFloatRect::new(client.left, rect.top, client.right, client.top - 9.0);
            let mut max_area =
                CfxFloatRect::new(client.left, client.bottom + 9.0, client.right, rect.bottom);
            min_area.normalize();
            max_area.normalize();
            if min_area.contains_point(point) {
                self.change_data(PwlScrollPrivateData::sub_big);
                if !self.move_pos_button(true) {
                    return true;
                }
                self.notify_scroll_window();
                if !self.wnd.is_alive() {
                    return true;
                }
            }
            if max_area.contains_point(point) {
                self.change_data(PwlScrollPrivateData::add_big);
                if !self.move_pos_button(true) {
                    return true;
                }
                self.notify_scroll_window();
            }
        }
        true
    }
    fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_up(flags, point);
        if !self.wnd.is_alive() {
            return true;
        }
        if self.wnd.has_flag(WINDOW_AUTO_TRANSPARENT)
            && self.wnd.transparency() != i32::from(Self::TRANSPARENCY)
        {
            self.wnd.set_transparency(i32::from(Self::TRANSPARENCY));
            if !self.wnd.invalidate_rect(None) {
                return true;
            }
        }
        self.stop_timer();
        self.mouse_down.set(false);
        true
    }
    fn set_scroll_info(&self, info: &PwlScrollInfo) {
        if *info == self.origin_info.get() {
            return;
        }
        self.origin_info.set(*info);
        self.set_scroll_range(
            0.0,
            (info.content_max - info.content_min - info.plate_width).max(0.0),
            info.plate_width,
        );
        if self.wnd.is_alive() {
            self.set_scroll_step(info.big_step, info.small_step);
        }
    }
    fn set_scroll_position(&self, pos: f32) {
        self.set_scroll_pos(self.origin_info.get().content_max - pos);
    }
    fn notify_l_button_down(&self, child: &dyn Window, point: CfxPointF) {
        match self.button_type(child) {
            Some(Type::MinButton) => self.min_button_down(),
            Some(Type::MaxButton) => self.max_button_down(),
            Some(Type::PosButton) => self.pos_button_down(point),
            None => {}
        }
    }
    fn notify_l_button_up(&self, child: &dyn Window, _point: CfxPointF) {
        if self.button_type(child) == Some(Type::PosButton) {
            self.mouse_down.set(false);
        }
        // Up/move notifications for min/max are empty in the source. The
        // enclosing scrollbar's mouse-up cancels the repeating timer.
    }
    fn notify_mouse_move(&self, child: &dyn Window, point: CfxPointF) {
        if self.button_type(child) == Some(Type::PosButton) {
            self.pos_button_move(point);
        }
    }
    fn create_child_wnd(&self, cp: &CreateParams) {
        self.create_buttons(cp);
    }
    fn scroll_bar_width(&self) -> f32 {
        if self.wnd.is_visible() {
            Self::WIDTH
        } else {
            0.0
        }
    }
}
impl TimerCallback for CpwlScrollBar {
    fn on_timer_fired(&self) {
        if !self.wnd.is_alive() {
            return;
        }
        let old = self.private_data.get();
        self.change_data(|data| {
            if self.min_or_max.get() {
                data.sub_small();
            } else {
                data.add_small();
            }
        });
        if old == self.private_data.get() {
            return;
        }
        if self.move_pos_button(true) {
            self.notify_scroll_window();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::cfx_timer::TimerHandler;

    #[test]
    fn range_reorders_endpoints_clamps_positions_and_saturates_steps() {
        let mut range = PwlFloatRange::default();
        range.set(8.0, 2.0);
        assert_eq!(range, PwlFloatRange { min: 2.0, max: 8.0 });
        assert_eq!(range.width(), 6.0);
        for value in [2.0, 5.0, 8.0, 2.0 - 0.00001, 8.0 + 0.00001] {
            assert!(range.contains(value));
        }
        for value in [1.0, 9.0, f32::NAN] {
            assert!(!range.contains(value));
        }
        let mut data = PwlScrollPrivateData::default();
        data.set_scroll_range(5.0, 25.0);
        assert_eq!(data.scroll_pos, 5.0);
        assert!(!data.set_pos(40.0));
        assert_eq!(data.scroll_pos, 5.0);
        data.sub_small();
        assert_eq!(data.scroll_pos, 5.0);
        data.add_big();
        assert_eq!(data.scroll_pos, 15.0);
        data.add_big();
        data.add_big();
        assert_eq!(data.scroll_pos, 25.0);
        data.set_small_step(3.0);
        data.sub_small();
        assert_eq!(data.scroll_pos, 22.0);
        data.set_scroll_range(10.0, 12.0);
        assert_eq!(data.scroll_pos, 12.0);
        data.reset();
        assert_eq!(data, PwlScrollPrivateData::default());
    }
    fn scrollbar() -> Rc<CpwlScrollBar> {
        let cp = CreateParams {
            rect: CfxFloatRect::new(0.0, 0.0, 12.0, 100.0),
            flags: WINDOW_VISIBLE,
            ..CreateParams::default()
        };
        let bar = CpwlScrollBar::new(cp, None);
        bar.wnd.realize();
        bar.set_scroll_info(&PwlScrollInfo {
            content_min: 0.0,
            content_max: 100.0,
            plate_width: 10.0,
            big_step: 10.0,
            small_step: 1.0,
        });
        bar
    }
    #[test]
    fn thumb_mapping_dragging_and_small_tracks_remain_bounded() {
        let bar = scrollbar();
        assert!(bar.wnd.is_valid());
        assert_eq!(bar.wnd.children().len(), 3);
        assert_eq!(bar.scroll_area(), CfxFloatRect::new(0.0, 10.0, 12.0, 90.0));
        assert_eq!(bar.true_to_face(0.0), 90.0);
        assert_eq!(bar.true_to_face(100.0), 10.0);
        assert_eq!(bar.face_to_true(50.0), 50.0);
        bar.set_scroll_position(60.0);
        assert_eq!(bar.scroll_position(), 40.0);
        let button = bar.pos_button.borrow().upgrade().unwrap();
        let top = button.wnd.window_rect().top;
        bar.pos_button_down(CfxPointF::new(6.0, top));
        bar.pos_button_move(CfxPointF::new(6.0, top - 800.0));
        assert_eq!(bar.scroll_position(), 90.0);
        bar.pos_button_move(CfxPointF::new(6.0, top + 800.0));
        assert_eq!(bar.scroll_position(), 0.0);
        bar.notify_l_button_up(button.as_ref(), CfxPointF::default());
        bar.pos_button_move(CfxPointF::new(6.0, top - 800.0));
        assert_eq!(bar.scroll_position(), 0.0);
        bar.wnd
            .move_window(CfxFloatRect::new(0.0, 0.0, 12.0, 3.0), true, false);
        assert!(!bar.wnd.is_visible());
        assert!(bar.face_to_true(0.0).is_finite());
    }

    #[derive(Default)]
    struct Host {
        callback: RefCell<Option<Rc<dyn Fn(i32)>>>,
        killed: Cell<u32>,
    }
    impl TimerHandler for Host {
        fn set_timer(&self, interval: i32, callback: Rc<dyn Fn(i32)>) -> i32 {
            assert_eq!(interval, 100);
            *self.callback.borrow_mut() = Some(callback);
            7
        }
        fn kill_timer(&self, id: i32) {
            assert_eq!(id, 7);
            self.killed.set(self.killed.get() + 1);
        }
    }
    #[test]
    fn repeat_timer_stops_on_mouse_up_and_logical_destruction() {
        let host = Rc::new(Host::default());
        let handler: Rc<dyn TimerHandler> = host.clone();
        let mut cp = CreateParams::new(Some(&handler), None, None);
        cp.flags = WINDOW_VISIBLE;
        cp.rect = CfxFloatRect::new(0.0, 0.0, 12.0, 100.0);
        let bar = CpwlScrollBar::new(cp, None);
        bar.wnd.realize();
        bar.set_scroll_info(&PwlScrollInfo {
            content_min: 0.0,
            content_max: 100.0,
            plate_width: 10.0,
            big_step: 10.0,
            small_step: 1.0,
        });
        bar.max_button_down();
        assert_eq!(bar.scroll_position(), 1.0);
        let fire = host.callback.borrow().as_ref().unwrap().clone();
        fire(7);
        assert_eq!(bar.scroll_position(), 2.0);
        bar.on_l_button_up(0, CfxPointF::new(-10.0, -10.0));
        assert_eq!(host.killed.get(), 1);
        fire(7);
        assert_eq!(bar.scroll_position(), 2.0);
        bar.min_button_down();
        assert_eq!(bar.scroll_position(), 1.0);
        let fire = host.callback.borrow().as_ref().unwrap().clone();
        bar.wnd.destroy();
        assert_eq!(host.killed.get(), 2);
        assert!(!bar.wnd.is_alive());
        fire(7);
        assert_eq!(bar.scroll_position(), 1.0);
    }
}
