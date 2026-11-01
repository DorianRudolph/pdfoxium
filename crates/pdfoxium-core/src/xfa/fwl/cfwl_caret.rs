// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fwl/cfwl_caret.{h,cpp}`. Host scheduling uses the existing CFX timer
//! adapter; timer callbacks retain only a weak reference to this widget.
use super::{
    cfwl_app::CfwlApp,
    cfwl_message::FwlMessage,
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{Part, PartState},
    cfwl_widget::{CfwlWidget, FwlType, FwlWidget, Properties, WidgetRef, WidgetState},
};
use crate::{
    fxcrt::{
        cfx_timer::{CfxTimer, TimerCallback},
        fx_coordinates::{CfxMatrix, CfxRectF},
    },
    xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
const BLINK_PERIOD_MS: i32 = 600;

pub struct CfwlCaret {
    base: CfwlWidget,
    timer: RefCell<Option<Rc<CfxTimer>>>,
    weak_self: Weak<Self>,
    timer_generation: Cell<u64>,
    finalizing: Cell<bool>,
}
impl CfwlCaret {
    pub fn new(app: &Rc<CfwlApp>, properties: Properties, outer: Option<WidgetRef>) -> Rc<Self> {
        let control = Rc::new_cyclic(|weak| Self {
            base: CfwlWidget::new(app, properties, outer),
            timer: RefCell::new(None),
            weak_self: weak.clone(),
            timer_generation: Cell::new(0),
            finalizing: Cell::new(false),
        });
        control.base.set_states(WidgetState::CARET_HIGHLIGHT);
        app.register_widget(control.clone() as WidgetRef);
        control
    }
    pub fn show_caret(&self) {
        if self.finalizing.get() {
            return;
        }
        let generation = self.timer_generation.get().wrapping_add(1);
        self.timer_generation.set(generation);
        self.cancel_timer();
        if self.timer_generation.get() != generation {
            return;
        }
        if let (Some(app), Some(owner)) = (self.base.app(), self.weak_self.upgrade()) {
            let handler = app.timer_handler();
            let callback: Rc<dyn TimerCallback> = owner;
            let timer = CfxTimer::new(handler.as_ref(), &callback, BLINK_PERIOD_MS);
            // Scheduling is host code. A nested hide/destroy must win over
            // this earlier show, and dropping a timer must not hold the slot.
            if self.timer_generation.get() != generation || self.base.self_ref().is_none() {
                drop(timer);
                return;
            }
            let replaced = self.timer.borrow_mut().replace(timer);
            drop(replaced);
        }
        if self.timer_generation.get() == generation {
            self.base.clear_states(WidgetState::INVISIBLE);
            self.base.set_states(WidgetState::CARET_HIGHLIGHT);
        }
    }
    pub fn hide_caret(&self) {
        self.timer_generation
            .set(self.timer_generation.get().wrapping_add(1));
        self.base.set_states(WidgetState::INVISIBLE);
        self.cancel_timer();
    }
    fn cancel_timer(&self) {
        let timer = self.timer.borrow_mut().take();
        drop(timer);
    }
}
impl TimerCallback for CfwlCaret {
    fn on_timer_fired(&self) {
        if self.finalizing.get() || self.base.self_ref().is_none() {
            return;
        }
        if (self.base.get_states() & WidgetState::CARET_HIGHLIGHT).0 == 0 {
            self.base.set_states(WidgetState::CARET_HIGHLIGHT);
        } else {
            self.base.clear_states(WidgetState::CARET_HIGHLIGHT);
        }
        let rect = self.base.get_widget_rect();
        self.base
            .repaint_rect(CfxRectF::new(0., 0., rect.width + 1., rect.height));
    }
}
impl FwlWidget for CfwlCaret {
    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::Caret
    }
    fn update(&self) {} // Source caret has no layout work.
    fn pre_finalize(&self) {
        self.finalizing.set(true);
        self.timer_generation
            .set(self.timer_generation.get().wrapping_add(1));
        self.cancel_timer();
        self.base.pre_finalize();
    }
    fn draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        if (self.base.get_states() & WidgetState::CARET_HIGHLIGHT).0 == 0 {
            return;
        }
        let Some(app) = self.base.app() else {
            return;
        };
        let mut param = CfwlThemeBackground::new(Part::Background, self);
        let rect = self.get_widget_rect();
        param.part_rect = CfxRectF::new(0., 0., rect.width, rect.height);
        param.states = PartState::HIGHLIGHT;
        param.matrix = matrix;
        app.theme_provider().draw_background(&param, graphics);
    }
    fn on_process_message(&self, _message: &mut FwlMessage) {} // Source caret receives no input.
}
