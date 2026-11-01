// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Native PseudoRetainable fixture adapted to real Rc owner tokens. Counters
//! observe every handle clone/drop; the payload's actual last Rc destruction
//! marks the object dead. Diagnostic state remains available to the fixture.
//! No engine intrusive-reference implementation or raw dereference is used.
#![allow(non_snake_case)]
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

#[derive(Default)]
struct Counts {
    retains: Cell<i32>,
    releases: Cell<i32>,
    dead: Cell<bool>,
    token: RefCell<Weak<Payload>>,
}
struct Payload {
    state: Rc<Counts>,
    identity: *const PseudoRetainable,
}
impl Drop for Payload {
    fn drop(&mut self) {
        self.state.dead.set(true);
    }
}
#[derive(Default)]
pub(super) struct PseudoRetainable {
    state: Rc<Counts>,
}
impl PseudoRetainable {
    pub fn retain_count(&self) -> i32 {
        self.state.retains.get()
    }
    pub fn release_count(&self) -> i32 {
        self.state.releases.get()
    }
    pub fn alive(&self) -> bool {
        !self.state.dead.get()
    }
}
#[derive(Default)]
pub(super) struct RetainPtr {
    payload: Option<Rc<Payload>>,
}
impl Clone for RetainPtr {
    fn clone(&self) -> Self {
        if let Some(payload) = &self.payload {
            payload.state.retains.set(payload.state.retains.get() + 1);
        }
        Self {
            payload: self.payload.clone(),
        }
    }
}
impl Drop for RetainPtr {
    fn drop(&mut self) {
        if let Some(payload) = &self.payload {
            payload.state.releases.set(payload.state.releases.get() + 1);
        }
    }
}
impl RetainPtr {
    pub fn new(obj: &PseudoRetainable) -> Self {
        let token = obj.state.token.borrow().upgrade();
        let payload = token.unwrap_or_else(|| {
            let payload = Rc::new(Payload {
                state: Rc::clone(&obj.state),
                identity: obj,
            });
            *obj.state.token.borrow_mut() = Rc::downgrade(&payload);
            payload
        });
        payload.state.retains.set(payload.state.retains.get() + 1);
        Self {
            payload: Some(payload),
        }
    }
    pub fn Get(&self) -> Option<*const PseudoRetainable> {
        self.payload.as_ref().map(|p| p.identity)
    }
    pub fn is_some(&self) -> bool {
        self.payload.is_some()
    }
    pub fn Reset(&mut self, obj: Option<&PseudoRetainable>) {
        *self = obj.map_or_else(Self::default, Self::new);
    }
    pub fn Swap(&mut self, other: &mut Self) {
        std::mem::swap(self, other);
    }
    pub fn Leak(&mut self) -> Self {
        std::mem::take(self)
    }
    pub fn Unleak(&mut self, leaked: Self) {
        *self = leaked;
    }
    pub fn assign(&mut self, other: &Self) {
        if self.Get() != other.Get() {
            *self = other.clone();
        }
    }
}
impl PartialEq for RetainPtr {
    fn eq(&self, rhs: &Self) -> bool {
        self.Get() == rhs.Get()
    }
}
impl Eq for RetainPtr {}
impl PartialOrd for RetainPtr {
    fn partial_cmp(&self, rhs: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(rhs))
    }
}
impl Ord for RetainPtr {
    fn cmp(&self, rhs: &Self) -> std::cmp::Ordering {
        self.Get().cmp(&rhs.Get())
    }
}
impl std::fmt::Debug for RetainPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.Get().fmt(f)
    }
}

pub(super) struct ScopeExit<F: FnOnce()> {
    callback: Option<F>,
    active: Cell<bool>,
}
impl<F: FnOnce()> ScopeExit<F> {
    pub fn new(callback: F) -> Self {
        Self {
            callback: Some(callback),
            active: Cell::new(true),
        }
    }
    pub fn abandon(&self) {
        self.active.set(false);
    }
}
impl<F: FnOnce()> Drop for ScopeExit<F> {
    fn drop(&mut self) {
        if let Some(callback) = self.callback.take() {
            if self.active.get() {
                callback();
            }
        }
    }
}
