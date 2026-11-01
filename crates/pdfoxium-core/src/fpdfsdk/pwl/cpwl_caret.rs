// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fpdfsdk/pwl/cpwl_caret.{h,cpp}`: blinking editable-text caret.
use super::cpwl_wnd::{CpwlWnd, CreateParams, Window};
use super::ipwl_fillernotify::PerWindowData;
use super::pwl_draw::PwlDraw;
use crate::fxcrt::cfx_timer::{CfxTimer, TimerCallback};
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use crate::fxge::cfx_fillrenderoptions::CfxFillRenderOptions;
use crate::fxge::cfx_graphstatedata::CfxGraphStateData;
use crate::fxge::cfx_path::CfxPath;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
pub struct CpwlCaret {
    wnd: CpwlWnd,
    this: Weak<Self>,
    flash: Cell<bool>,
    head: Cell<CfxPointF>,
    foot: Cell<CfxPointF>,
    width: f32,
    invalid_rect: Cell<CfxFloatRect>,
    timer: RefCell<Option<Rc<CfxTimer>>>,
}
impl CpwlCaret {
    pub fn new(cp: CreateParams, attached: Option<Box<dyn PerWindowData>>) -> Rc<Self> {
        let caret = Rc::new_cyclic(|this| Self {
            wnd: CpwlWnd::new(cp, attached),
            this: this.clone(),
            flash: Cell::new(false),
            head: Cell::new(CfxPointF::default()),
            foot: Cell::new(CfxPointF::default()),
            width: 0.4,
            invalid_rect: Cell::new(CfxFloatRect::default()),
            timer: RefCell::new(None),
        });
        let owner: Rc<dyn Window> = caret.clone();
        caret.wnd.bind_owner(&owner);
        caret
    }
    pub fn set_invalid_rect(&self, rect: CfxFloatRect) {
        self.invalid_rect.set(rect);
    }
    pub fn caret_rect(&self) -> CfxFloatRect {
        let foot = self.foot.get();
        let head = self.head.get();
        CfxFloatRect::new(foot.x, foot.y, head.x + self.width, head.y)
    }
    pub fn set_caret(&self, visible: bool, head: CfxPointF, foot: CfxPointF) {
        if !visible {
            self.head.set(CfxPointF::default());
            self.foot.set(CfxPointF::default());
            self.flash.set(false);
            if !self.wnd.is_visible() {
                return;
            }
            let timer = self.timer.borrow_mut().take();
            drop(timer);
            self.wnd.set_visible(false);
            return;
        }
        if !self.wnd.is_visible() {
            self.head.set(head);
            self.foot.set(foot);
            if let Some(this) = self.this.upgrade() {
                let callback: Rc<dyn TimerCallback> = this;
                let handler = self.wnd.timer_handler();
                let timer = CfxTimer::new(handler.as_ref(), &callback, 500);
                if !self.wnd.is_alive() {
                    drop(timer);
                    return;
                }
                *self.timer.borrow_mut() = Some(timer);
            }
            if !self.wnd.set_visible(true) {
                return;
            }
            self.flash.set(true);
            self.wnd.move_window(self.invalid_rect.get(), false, true);
            return;
        }
        if self.head.get() == head && self.foot.get() == foot {
            return;
        }
        self.head.set(head);
        self.foot.set(foot);
        self.flash.set(true);
        self.wnd.move_window(self.invalid_rect.get(), false, true);
    }
}
impl TimerCallback for CpwlCaret {
    fn on_timer_fired(&self) {
        if self.wnd.is_valid() {
            self.flash.set(!self.flash.get());
            self.invalidate_rect(None);
        }
    }
}
impl Window for CpwlCaret {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn wnd(&self) -> &CpwlWnd {
        &self.wnd
    }
    fn draw_this_appearance(&self, draw: &mut dyn PwlDraw, matrix: CfxMatrix) {
        if !self.wnd.is_visible() || !self.flash.get() {
            return;
        }
        let mut rect = self.caret_rect();
        let clip = self.wnd.clip_rect();
        let x = rect.left + self.width * 0.5;
        if !clip.is_empty() {
            rect.intersect(clip);
            if rect.is_empty() {
                return;
            }
        }
        let mut path = CfxPath::new();
        path.append_line(CfxPointF::new(x, rect.bottom), CfxPointF::new(x, rect.top));
        let mut graph = CfxGraphStateData::default();
        graph.set_line_width(self.width);
        draw.draw_path(
            &path,
            matrix,
            Some(&graph),
            0,
            0xff000000,
            CfxFillRenderOptions::even_odd_options(),
        );
    }
    fn invalidate_rect(&self, rect: Option<CfxFloatRect>) -> bool {
        let Some(mut rect) = rect else {
            return self.wnd.invalidate_rect(None);
        };
        if !rect.is_empty() {
            rect.inflate_xy(0.5, 0.5);
            rect.normalize();
        }
        rect.top += 1.;
        rect.bottom -= 1.;
        self.wnd.invalidate_rect(Some(rect))
    }
    // Source intentionally controls the actual base visibility in SetCaret;
    // parent visibility propagation must not make an inactive caret appear.
    fn set_visible(&self, _visible: bool) -> bool {
        self.wnd.is_alive()
    }
    fn on_destroy(&self) {
        let timer = self.timer.borrow_mut().take();
        drop(timer);
        self.flash.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::cfx_timer::TimerHandler;
    #[derive(Default)]
    struct Host {
        callback: RefCell<Option<Rc<dyn Fn(i32)>>>,
        killed: Cell<usize>,
    }
    impl TimerHandler for Host {
        fn set_timer(&self, interval: i32, callback: Rc<dyn Fn(i32)>) -> i32 {
            assert_eq!(interval, 500);
            *self.callback.borrow_mut() = Some(callback);
            42
        }
        fn kill_timer(&self, id: i32) {
            assert_eq!(id, 42);
            self.killed.set(self.killed.get() + 1);
        }
    }
    #[test]
    fn caret_blink_geometry_hiding_and_late_timer_events() {
        let host = Rc::new(Host::default());
        let handler: Rc<dyn TimerHandler> = host.clone();
        let cp = CreateParams::new(Some(&handler), None, None);
        let caret = CpwlCaret::new(cp, None);
        caret.wnd().realize();
        caret.set_invalid_rect(CfxFloatRect::new(0., 0., 100., 50.));
        caret.set_caret(true, CfxPointF::new(10., 40.), CfxPointF::new(10., 10.));
        assert!(caret.wnd().is_visible());
        assert!(caret.flash.get());
        let rect = caret.caret_rect();
        assert_eq!((rect.left, rect.bottom, rect.top), (10., 10., 40.));
        assert!((rect.right - 10.4).abs() < 0.0001);
        let fire = host.callback.borrow().as_ref().unwrap().clone();
        fire(42);
        assert!(!caret.flash.get());
        fire(42);
        assert!(caret.flash.get());
        caret.set_caret(false, CfxPointF::default(), CfxPointF::default());
        assert!(!caret.wnd().is_visible());
        assert!(!caret.flash.get());
        assert_eq!(host.killed.get(), 1);
        fire(42);
        assert!(!caret.flash.get());
        caret.wnd().destroy();
        assert_eq!(host.killed.get(), 1);
    }
}
