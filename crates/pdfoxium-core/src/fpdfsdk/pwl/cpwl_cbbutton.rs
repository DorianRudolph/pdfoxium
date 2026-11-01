// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Combo-box arrow button from `fpdfsdk/pwl/cpwl_cbbutton.{h,cpp}`.
use super::cpwl_wnd::{CpwlWnd, CreateParams, Window};
use super::ipwl_fillernotify::PerWindowData;
use super::pwl_draw::PwlDraw;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF};
use std::any::Any;
use std::rc::Rc;
pub struct CpwlCbButton {
    wnd: CpwlWnd,
}
impl CpwlCbButton {
    pub fn new(cp: CreateParams, attached: Option<Box<dyn PerWindowData>>) -> Rc<Self> {
        let result = Rc::new(Self {
            wnd: CpwlWnd::new(cp, attached),
        });
        let window: Rc<dyn Window> = result.clone();
        result.wnd.bind_owner(&window);
        result
    }
}
impl Window for CpwlCbButton {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn wnd(&self) -> &CpwlWnd {
        &self.wnd
    }
    fn draw_this_appearance(&self, draw: &mut dyn PwlDraw, matrix: CfxMatrix) {
        self.wnd.base_draw_this_appearance(draw, matrix);
        if !self.wnd.is_visible() {
            return;
        }
        let window = self.wnd.window_rect();
        if window.is_empty()
            || !(window.width() > 6.0 && (window.width() - 6.0).abs() >= 0.0001)
            || !(window.height() > 3.0 && (window.height() - 3.0).abs() >= 0.0001)
        {
            return;
        }
        let center = self.wnd.center_point();
        let points = [
            CfxPointF::new(center.x - 3.0, center.y + 1.5),
            CfxPointF::new(center.x + 3.0, center.y + 1.5),
            CfxPointF::new(center.x, center.y - 1.5),
            CfxPointF::new(center.x - 3.0, center.y + 1.5),
        ];
        draw.fill_area(
            matrix,
            &points,
            ((self.wnd.transparency() as u32) & 255) << 24,
        );
    }
    fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_down(flags, point);
        self.wnd.set_capture();
        if let Some(parent) = self.wnd.parent() {
            parent.notify_l_button_down(self, point);
        }
        true
    }
    fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_up(flags, point);
        self.wnd.release_capture();
        true
    }
}
