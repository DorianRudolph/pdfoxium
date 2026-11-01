// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Scrollbar arrow/thumb buttons from `fpdfsdk/pwl/cpwl_sbbutton.{h,cpp}`.
use super::cpwl_wnd::{CpwlWnd, CreateParams, Window};
use super::ipwl_fillernotify::{CursorStyle, PerWindowData};
use super::pwl_draw::PwlDraw;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF};
use crate::fxge::dib::fx_dib::argb_encode;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Type {
    MinButton,
    MaxButton,
    PosButton,
}
pub struct CpwlSbButton {
    pub wnd: CpwlWnd,
    button_type: Type,
    mouse_down: Cell<bool>,
}
impl CpwlSbButton {
    pub fn new(
        mut cp: CreateParams,
        attached: Option<Box<dyn PerWindowData>>,
        button_type: Type,
    ) -> Rc<Self> {
        cp.cursor_type = CursorStyle::Arrow;
        let result = Rc::new(Self {
            wnd: CpwlWnd::new(cp, attached),
            button_type,
            mouse_down: Cell::new(false),
        });
        let owner: Rc<dyn Window> = result.clone();
        result.wnd.bind_owner(&owner);
        result
    }
    pub fn button_type(&self) -> Type {
        self.button_type
    }
}
impl Window for CpwlSbButton {
    fn wnd(&self) -> &CpwlWnd {
        &self.wnd
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn draw_this_appearance(&self, device: &mut dyn PwlDraw, matrix: CfxMatrix) {
        if !self.wnd.is_visible() {
            return;
        }
        let rect = self.wnd.window_rect();
        if rect.is_empty() {
            return;
        }
        let center = self.wnd.center_point();
        let alpha = self.wnd.transparency();
        let gray = |value: u32| argb_encode(alpha as u32, value, value, value);
        device.stroke_rect(matrix, rect, gray(100), 0.0);
        device.stroke_rect(matrix, rect.get_deflated(0.5, 0.5), gray(255), 1.0);
        if self.button_type != Type::PosButton {
            device.shadow(matrix, rect.get_deflated(1.0, 1.0), alpha, 80, 220);
            if rect.height() > 6.0 {
                let offsets = if self.button_type == Type::MinButton {
                    [
                        (2.5, 4.0),
                        (2.5, 3.0),
                        (4.5, 5.0),
                        (6.5, 3.0),
                        (6.5, 4.0),
                        (4.5, 6.0),
                        (2.5, 4.0),
                    ]
                } else {
                    [
                        (2.5, 5.0),
                        (2.5, 6.0),
                        (4.5, 4.0),
                        (6.5, 6.0),
                        (6.5, 5.0),
                        (4.5, 3.0),
                        (2.5, 5.0),
                    ]
                };
                let points =
                    offsets.map(|(x, y)| CfxPointF::new(rect.left + 1.5 + x, rect.bottom + y));
                device.fill_area(matrix, &points, gray(255));
            }
            return;
        }
        let mut top = CfxPointF::new(rect.left + 1.5, rect.top - 1.0);
        let mut bottom = CfxPointF::new(rect.left + 1.5, rect.bottom + 1.0);
        for value in [210, 220, 240, 240, 210, 180, 150, 150, 180, 210] {
            device.stroke_line(matrix, top, bottom, gray(value), 1.0);
            top.x += 1.0;
            bottom.x += 1.0;
        }
        if rect.height() <= 8.0 {
            return;
        }
        let mut left = CfxPointF::new(center.x - 2.5, center.y - 2.75 + 0.5);
        let mut right = CfxPointF::new(center.x + 2.5, center.y - 2.75 + 0.5);
        for _ in 0..3 {
            device.stroke_line(matrix, left, right, gray(120), 1.0);
            left.y += 2.0;
            right.y += 2.0;
        }
    }
    fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_down(flags, point);
        if !self.wnd.is_alive() {
            return true;
        }
        if let Some(parent) = self.wnd.parent() {
            parent.notify_l_button_down(self, point);
        }
        if self.wnd.is_alive() {
            self.mouse_down.set(true);
            self.wnd.set_capture();
        }
        true
    }
    fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_up(flags, point);
        if !self.wnd.is_alive() {
            return true;
        }
        if let Some(parent) = self.wnd.parent() {
            parent.notify_l_button_up(self, point);
        }
        if self.wnd.is_alive() {
            self.mouse_down.set(false);
            self.wnd.release_capture();
        }
        true
    }
    fn on_mouse_move(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_mouse_move(flags, point);
        if !self.wnd.is_alive() {
            return true;
        }
        if let Some(parent) = self.wnd.parent() {
            parent.notify_mouse_move(self, point);
        }
        true
    }
}
