// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `core/fxcrt/cfx_timer.{h,cpp}`. The host supplies scheduling. The native
//! manually initialized map becomes thread-local weak storage; callbacks leave
//! the map borrow before invoking consumers. No clock or thread is introduced.
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

thread_local! { static TIMERS: RefCell<BTreeMap<i32,Weak<CfxTimer>>> = const {RefCell::new(BTreeMap::new())}; }

pub trait TimerHandler {
    fn set_timer(&self, interval: i32, callback: Rc<dyn Fn(i32)>) -> i32;
    fn kill_timer(&self, id: i32);
}
pub trait TimerCallback {
    fn on_timer_fired(&self);
}
pub struct CfxTimer {
    id: Cell<i32>,
    handler: Option<Weak<dyn TimerHandler>>,
    callback: Weak<dyn TimerCallback>,
}
impl CfxTimer {
    pub fn new(
        handler: Option<&Rc<dyn TimerHandler>>,
        callback: &Rc<dyn TimerCallback>,
        interval: i32,
    ) -> Rc<Self> {
        let timer = Rc::new(Self {
            id: Cell::new(0),
            handler: handler.map(Rc::downgrade),
            callback: Rc::downgrade(callback),
        });
        if let Some(handler) = handler {
            let id = handler.set_timer(interval, Rc::new(Self::timer_proc));
            timer.id.set(id);
            if id != 0 {
                TIMERS.with(|map| {
                    map.borrow_mut().insert(id, Rc::downgrade(&timer));
                });
            }
        }
        timer
    }
    pub fn has_valid_id(&self) -> bool {
        self.id.get() != 0
    }
    fn timer_proc(id: i32) {
        let timer = TIMERS.with(|map| map.borrow().get(&id).and_then(Weak::upgrade));
        if let Some(timer) = timer {
            if let Some(callback) = timer.callback.upgrade() {
                callback.on_timer_fired();
            }
        }
    }
    pub fn cancel(&self) {
        let id = self.id.replace(0);
        if id != 0 {
            TIMERS.with(|map| {
                let mut map = map.borrow_mut();
                // A broken host may reuse IDs. An older timer must not erase
                // its replacement when it is destroyed.
                if map
                    .get(&id)
                    .is_some_and(|timer| std::ptr::eq(timer.as_ptr(), self))
                {
                    map.remove(&id);
                }
            });
            if let Some(handler) = self.handler.as_ref().and_then(Weak::upgrade) {
                handler.kill_timer(id);
            }
        }
    }
}
impl Drop for CfxTimer {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[derive(Default)]
    struct Host {
        callback: RefCell<Option<Rc<dyn Fn(i32)>>>,
        killed: Cell<u32>,
    }
    impl TimerHandler for Host {
        fn set_timer(&self, interval: i32, callback: Rc<dyn Fn(i32)>) -> i32 {
            assert_eq!(interval, 500);
            callback(7);
            *self.callback.borrow_mut() = Some(callback);
            7
        }
        fn kill_timer(&self, id: i32) {
            assert_eq!(id, 7);
            self.killed.set(self.killed.get() + 1);
        }
    }
    #[derive(Default)]
    struct Consumer {
        fired: Cell<u32>,
        timer: RefCell<Option<Weak<CfxTimer>>>,
    }
    impl TimerCallback for Consumer {
        fn on_timer_fired(&self) {
            self.fired.set(self.fired.get() + 1);
            let timer = self.timer.borrow().as_ref().and_then(Weak::upgrade);
            if let Some(timer) = timer {
                timer.cancel();
            }
        }
    }
    #[test]
    fn cancellation_during_callback_and_late_host_events_are_safe() {
        let host = Rc::new(Host::default());
        let handler: Rc<dyn TimerHandler> = host.clone();
        let consumer = Rc::new(Consumer::default());
        let callback: Rc<dyn TimerCallback> = consumer.clone();
        let timer = CfxTimer::new(Some(&handler), &callback, 500);
        assert!(timer.has_valid_id());
        assert_eq!(consumer.fired.get(), 0);
        *consumer.timer.borrow_mut() = Some(Rc::downgrade(&timer));
        let fire = host.callback.borrow().as_ref().unwrap().clone();
        fire(8);
        assert_eq!(consumer.fired.get(), 0);
        fire(7);
        assert_eq!(consumer.fired.get(), 1);
        assert_eq!(host.killed.get(), 1);
        assert!(!timer.has_valid_id());
        fire(7);
        assert_eq!(consumer.fired.get(), 1);
        drop(timer);
        fire(7);
        assert_eq!(host.killed.get(), 1);
    }
    #[test]
    fn dropping_timer_or_handler_preserves_weak_ownership() {
        let host = Rc::new(Host::default());
        let handler: Rc<dyn TimerHandler> = host.clone();
        let callback: Rc<dyn TimerCallback> = Rc::new(Consumer::default());
        let timer = CfxTimer::new(Some(&handler), &callback, 500);
        drop(timer);
        assert_eq!(host.killed.get(), 1);
        let timer = CfxTimer::new(Some(&handler), &callback, 500);
        drop(handler);
        drop(host);
        drop(timer);
        assert!(!CfxTimer::new(None, &callback, 500).has_valid_id());
    }

    #[derive(Default)]
    struct Scheduler {
        scheduled: RefCell<Vec<(i32, Rc<dyn Fn(i32)>)>>,
        killed: RefCell<Vec<i32>>,
    }
    impl TimerHandler for Scheduler {
        fn set_timer(&self, interval: i32, callback: Rc<dyn Fn(i32)>) -> i32 {
            let mut scheduled = self.scheduled.borrow_mut();
            let id = 1001 + scheduled.len() as i32;
            scheduled.push((interval, callback));
            id
        }
        fn kill_timer(&self, id: i32) {
            self.killed.borrow_mut().push(id);
        }
    }
    /// CFXTimer.ValidTimers: any captured callback routes the supplied event ID.
    #[test]
    fn valid_timers() {
        let scheduler = Rc::new(Scheduler::default());
        let handler: Rc<dyn TimerHandler> = scheduler.clone();
        let cb1 = Rc::new(Consumer::default());
        let iface1: Rc<dyn TimerCallback> = cb1.clone();
        let cb2 = Rc::new(Consumer::default());
        let iface2: Rc<dyn TimerCallback> = cb2.clone();
        let timer1 = CfxTimer::new(Some(&handler), &iface1, 100);
        let timer2 = CfxTimer::new(Some(&handler), &iface2, 200);
        assert!(timer1.has_valid_id());
        assert!(timer2.has_valid_id());
        let scheduled = scheduler.scheduled.borrow();
        assert_eq!(scheduled[0].0, 100);
        assert_eq!(scheduled[1].0, 200);
        let fn1 = scheduled[0].1.clone();
        let _fn2 = scheduled[1].1.clone();
        drop(scheduled);
        fn1(1001);
        fn1(1002);
        fn1(1002);
        assert_eq!(cb1.fired.get(), 1);
        assert_eq!(cb2.fired.get(), 2);
        drop(timer2);
        drop(timer1);
        assert_eq!(*scheduler.killed.borrow(), [1002, 1001]);
    }
    /// CFXTimer.MisbehavingEmbedder: invalid IDs and stale callbacks are ignored.
    #[test]
    fn misbehaving_embedder() {
        let scheduler = Rc::new(Scheduler::default());
        let handler: Rc<dyn TimerHandler> = scheduler.clone();
        let callback = Rc::new(Consumer::default());
        let iface: Rc<dyn TimerCallback> = callback.clone();
        let timer = CfxTimer::new(Some(&handler), &iface, 100);
        assert!(timer.has_valid_id());
        let scheduled = scheduler.scheduled.borrow();
        assert_eq!(scheduled[0].0, 100);
        let fire = scheduled[0].1.clone();
        drop(scheduled);
        fire(-1);
        fire(0);
        fire(1002);
        drop(timer);
        fire(1001);
        assert_eq!(callback.fired.get(), 0);
        assert_eq!(*scheduler.killed.borrow(), [1001]);
    }
}
