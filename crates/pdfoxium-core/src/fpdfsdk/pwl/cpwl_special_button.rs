// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Push, checkbox and radio controls from `fpdfsdk/pwl/cpwl_special_button.{h,cpp}`.
use super::cpwl_wnd::{CpwlWnd, CreateParams, Window};
use super::ipwl_fillernotify::{CursorStyle, PerWindowData};
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF};
use std::cell::Cell;
use std::rc::Rc;

pub struct CpwlPushButton {
    pub wnd: CpwlWnd,
    mouse_down: Cell<bool>,
}
impl CpwlPushButton {
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
}
impl Window for CpwlPushButton {
    fn wnd(&self) -> &CpwlWnd {
        &self.wnd
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn focus_rect(&self) -> CfxFloatRect {
        let width = self.wnd.border_width() as f32;
        self.wnd.window_rect().get_deflated(width, width)
    }
    fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_down(flags, point);
        if !self.wnd.is_alive() {
            return true;
        }
        self.mouse_down.set(true);
        self.wnd.set_capture();
        true
    }
    fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_up(flags, point);
        if !self.wnd.is_alive() {
            return true;
        }
        self.wnd.release_capture();
        self.mouse_down.set(false);
        true
    }
}

pub struct CpwlCheckBox {
    pub wnd: CpwlWnd,
    mouse_down: Cell<bool>,
    checked: Cell<bool>,
}
impl CpwlCheckBox {
    pub fn new(mut cp: CreateParams, attached: Option<Box<dyn PerWindowData>>) -> Rc<Self> {
        cp.cursor_type = CursorStyle::Hand;
        let result = Rc::new(Self {
            wnd: CpwlWnd::new(cp, attached),
            mouse_down: Cell::new(false),
            checked: Cell::new(false),
        });
        let owner: Rc<dyn Window> = result.clone();
        result.wnd.bind_owner(&owner);
        result
    }
    pub fn is_checked(&self) -> bool {
        self.checked.get()
    }
    pub fn set_check(&self, checked: bool) {
        self.checked.set(checked);
    }
    fn toggle(&self) -> bool {
        if self.wnd.is_read_only() {
            return false;
        }
        self.set_check(!self.is_checked());
        true
    }
}
impl Window for CpwlCheckBox {
    fn wnd(&self) -> &CpwlWnd {
        &self.wnd
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_down(flags, point);
        if !self.wnd.is_alive() {
            return true;
        }
        self.mouse_down.set(true);
        self.wnd.set_capture();
        true
    }
    fn on_l_button_up(&self, _flags: u32, _point: CfxPointF) -> bool {
        self.toggle()
    }
    fn on_char(&self, _character: u16, _flags: u32) -> bool {
        self.toggle()
    }
}

pub struct CpwlRadioButton {
    pub wnd: CpwlWnd,
    mouse_down: Cell<bool>,
    checked: Cell<bool>,
}
impl CpwlRadioButton {
    pub fn new(mut cp: CreateParams, attached: Option<Box<dyn PerWindowData>>) -> Rc<Self> {
        cp.cursor_type = CursorStyle::Hand;
        let result = Rc::new(Self {
            wnd: CpwlWnd::new(cp, attached),
            mouse_down: Cell::new(false),
            checked: Cell::new(false),
        });
        let owner: Rc<dyn Window> = result.clone();
        result.wnd.bind_owner(&owner);
        result
    }
    pub fn is_checked(&self) -> bool {
        self.checked.get()
    }
    pub fn set_check(&self, checked: bool) {
        self.checked.set(checked);
    }
    fn select(&self) -> bool {
        if self.wnd.is_read_only() {
            return false;
        }
        self.set_check(true);
        true
    }
}
impl Window for CpwlRadioButton {
    fn wnd(&self) -> &CpwlWnd {
        &self.wnd
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_down(flags, point);
        if !self.wnd.is_alive() {
            return true;
        }
        self.mouse_down.set(true);
        self.wnd.set_capture();
        true
    }
    fn on_l_button_up(&self, _flags: u32, _point: CfxPointF) -> bool {
        self.select()
    }
    fn on_char(&self, _character: u16, _flags: u32) -> bool {
        self.select()
    }
}

#[cfg(test)]
mod tests {
    use super::super::cpwl_wnd::{WINDOW_BORDER, WINDOW_READ_ONLY, WINDOW_VISIBLE};
    use super::*;
    #[test]
    fn checkboxes_toggle_radios_select_and_read_only_preserves_state() {
        for read_only in [false, true] {
            let cp = CreateParams {
                rect: CfxFloatRect::new(0.0, 0.0, 20.0, 20.0),
                flags: WINDOW_VISIBLE | if read_only { WINDOW_READ_ONLY } else { 0 },
                ..CreateParams::default()
            };
            let check = CpwlCheckBox::new(cp.clone(), None);
            check.wnd.realize();
            check.set_check(true);
            assert_eq!(check.on_char(13, 0), !read_only);
            assert_eq!(check.is_checked(), read_only);
            assert_eq!(check.on_l_button_up(0, CfxPointF::default()), !read_only);
            assert!(check.is_checked());
            let radio = CpwlRadioButton::new(cp, None);
            radio.wnd.realize();
            assert_eq!(radio.on_char(13, 0), !read_only);
            assert_eq!(radio.is_checked(), !read_only);
            assert_eq!(radio.on_l_button_up(0, CfxPointF::default()), !read_only);
            assert_eq!(radio.is_checked(), !read_only);
        }
    }
    #[test]
    fn push_button_focus_rect_and_capture_follow_base_button_events() {
        let cp = CreateParams {
            rect: CfxFloatRect::new(0.0, 0.0, 20.0, 20.0),
            flags: WINDOW_VISIBLE | WINDOW_BORDER,
            border_width: 2,
            ..CreateParams::default()
        };
        let push = CpwlPushButton::new(cp, None);
        push.wnd.realize();
        assert_eq!(push.focus_rect(), CfxFloatRect::new(2.0, 2.0, 18.0, 18.0));
        assert!(push.on_l_button_down(0, CfxPointF::new(10.0, 10.0)));
        assert!(push.wnd.is_capture_mouse());
        assert!(push.on_l_button_up(0, CfxPointF::new(10.0, 10.0)));
        assert!(!push.wnd.is_capture_mouse());
    }
}
