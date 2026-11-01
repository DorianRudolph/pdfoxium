// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fxjs/global_timer.{h,cpp}`. Weak callback captures replace the native
//! process-global raw-pointer map. Scheduling remains entirely host-driven.
use super::{cjs_app::CjsApp, cjs_runtime::CjsRuntime};
use crate::fxcrt::cfx_timer::TimerHandler;
use std::{
    cell::Cell,
    rc::{Rc, Weak},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerType {
    Repeating,
    OneShot,
}
pub struct GlobalTimer {
    kind: TimerType,
    processing: Cell<bool>,
    cancelled: Cell<bool>,
    timer_id: Cell<i32>,
    time_out: u32,
    script: Vec<u16>,
    runtime: Weak<CjsRuntime>,
    app: Weak<CjsApp>,
    handler: Option<Weak<dyn TimerHandler>>,
}
impl GlobalTimer {
    pub fn new(
        app: &Rc<CjsApp>,
        runtime: &Rc<CjsRuntime>,
        kind: TimerType,
        script: Vec<u16>,
        elapse: u32,
        time_out: u32,
    ) -> Rc<Self> {
        let handler = runtime
            .environment()
            .and_then(|environment| environment.timer_handler());
        let timer = Rc::new(Self {
            kind,
            processing: Cell::new(false),
            cancelled: Cell::new(false),
            timer_id: Cell::new(0),
            time_out,
            script,
            runtime: Rc::downgrade(runtime),
            app: Rc::downgrade(app),
            handler: handler.as_ref().map(Rc::downgrade),
        });
        if let Some(handler) = handler {
            let weak = Rc::downgrade(&timer);
            let id = handler.set_timer(
                elapse as i32,
                Rc::new(move |id| {
                    if let Some(timer) = weak.upgrade() {
                        // A synchronous notification before SetTimer returns has
                        // no registered timer yet, exactly as in the source map.
                        if id != 0 && timer.timer_id() == id {
                            timer.trigger();
                        }
                    }
                }),
            );
            timer.timer_id.set(id);
        }
        timer
    }
    pub fn timer_id(&self) -> i32 {
        self.timer_id.get()
    }
    pub fn cancel_from_app(self: &Rc<Self>) {
        if let Some(app) = self.app.upgrade() {
            app.cancel_timer(self);
        } else {
            self.cancel();
        }
    }
    pub fn cancel(&self) {
        if self.cancelled.replace(true) {
            return;
        }
        if self.timer_id() != 0 {
            if let Some(handler) = self.handler.as_ref().and_then(Weak::upgrade) {
                handler.kill_timer(self.timer_id());
            }
        }
    }
    fn trigger(self: &Rc<Self>) {
        if self.cancelled.get() || self.processing.replace(true) {
            return;
        }
        struct Processing<'a>(&'a Cell<bool>);
        impl Drop for Processing<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _processing = Processing(&self.processing);
        if let Some(runtime) = self
            .runtime
            .upgrade()
            .filter(|runtime| runtime.environment().is_some())
        {
            if (self.kind != TimerType::OneShot || self.time_out > 0) && !runtime.is_blocking() {
                let context = runtime.new_event_context();
                context.on_external_exec();
                // The source ignores an exception from the timer script.
                let _ = context.run_script(&self.script);
            }
        }
        // JavaScript may have cancelled this timer or detached its runtime.
        // Only the still-live timer is eligible for one-shot cancellation.
        if !self.cancelled.get() && self.kind == TimerType::OneShot {
            self.cancel_from_app();
        }
    }
}
impl Drop for GlobalTimer {
    fn drop(&mut self) {
        self.cancel();
    }
}
