// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `fpdfsdk/pwl/cpwl_button.{h,cpp}`.
use super::cpwl_wnd::{CpwlWnd, CreateParams, Window};
use super::ipwl_fillernotify::{CursorStyle, PerWindowData};
use crate::fxcrt::fx_coordinates::CfxPointF;
use std::cell::Cell;
use std::rc::Rc;

pub struct CpwlButton {
    pub wnd: CpwlWnd,
    mouse_down: Cell<bool>,
}
impl CpwlButton {
    pub fn new(mut cp: CreateParams, attached: Option<Box<dyn PerWindowData>>) -> Rc<Self> {
        cp.cursor_type = CursorStyle::Hand;
        let result = Rc::new(Self {
            wnd: CpwlWnd::new(cp, attached),
            mouse_down: Cell::new(false),
        });
        let owner: Rc<dyn Window> = result.clone();
        result.wnd.bind_owner(&owner);
        result
    }
    pub fn mouse_down(&self) -> bool {
        self.mouse_down.get()
    }
    pub fn press(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_down(flags, point);
        if !self.wnd.is_alive() {
            return true;
        }
        self.mouse_down.set(true);
        self.wnd.set_capture();
        true
    }
    pub fn release(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_up(flags, point);
        if !self.wnd.is_alive() {
            return true;
        }
        self.wnd.release_capture();
        self.mouse_down.set(false);
        true
    }
}
impl Window for CpwlButton {
    fn wnd(&self) -> &CpwlWnd {
        &self.wnd
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.press(flags, point)
    }
    fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        self.release(flags, point)
    }
}
