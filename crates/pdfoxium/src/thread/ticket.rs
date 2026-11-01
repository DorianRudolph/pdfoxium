// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use crate::{Error, Result};
use std::sync::MutexGuard;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Condvar, Mutex};
use std::{
    future::Future,
    panic::{catch_unwind, AssertUnwindSafe},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};
#[cfg(target_arch = "wasm32")]
use wasm_sync::{Condvar, Mutex};

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// Cancels requests attached before `cancel_all`, without cancelling later ones.
/// Cancellation is observed by workers at their next scheduling/pause point.
#[derive(Clone, Default)]
pub struct CancelGroup(Arc<AtomicU64>);

impl CancelGroup {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel_all(&self) {
        // Saturation permanently cancels an exhausted group instead of reviving
        // ancient requests after a generation wrap.
        let mut generation = self.0.load(Ordering::Acquire);
        loop {
            match self.0.compare_exchange_weak(
                generation,
                generation.saturating_add(1),
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(current) => generation = current,
            }
        }
    }
}

pub(super) struct RunState {
    pub shutdown: AtomicBool,
    pub generation: Arc<AtomicU64>,
    pub source_generation: u64,
}

pub(super) struct Cancellation {
    cancelled: AtomicBool,
    groups: Mutex<Vec<(Arc<AtomicU64>, u64)>>,
    run: Arc<RunState>,
}

impl Cancellation {
    pub fn is_cancelled(&self) -> bool {
        if self.cancelled.load(Ordering::Acquire)
            || self.run.shutdown.load(Ordering::Acquire)
            || self.run.generation.load(Ordering::Acquire) != self.run.source_generation
        {
            return true;
        }
        lock(&self.groups).iter().any(|(group, snapshot)| {
            let now = group.load(Ordering::Acquire);
            now == u64::MAX || now != *snapshot
        })
    }

    fn group(&self, group: &CancelGroup) {
        lock(&self.groups).push((group.0.clone(), group.0.load(Ordering::Acquire)));
    }
}

struct Slot<T> {
    result: Option<Result<T>>,
    waker: Option<Waker>,
}
struct Shared<T> {
    slot: Mutex<Slot<T>>,
    available: Condvar,
    cancellation: Arc<Cancellation>,
}

fn wake(waker: Option<Waker>) {
    if let Some(waker) = waker {
        // A user-supplied waker must not unwind through the worker's document.
        let _ = catch_unwind(AssertUnwindSafe(|| waker.wake()));
    }
}

/// An owned asynchronous result. Poll with `try_take`, block with `wait`, or await
/// it using any executor. Dropping a pending ticket cancels its render.
/// Results may be taken only once; `try_take` returns `None` thereafter.
#[must_use = "dropping a ticket cancels the request"]
pub struct Ticket<T> {
    shared: Arc<Shared<T>>,
    taken: bool,
}

impl<T> Ticket<T> {
    /// Attach a cancellation group at this call's generation. For attachment
    /// before enqueueing, use `RenderPool::render_in_group` instead.
    pub fn group(self, group: &CancelGroup) -> Self {
        self.shared.cancellation.group(group);
        self
    }

    /// Cancel the request. Waiting/polling returns `Error::Cancelled`; core work
    /// stops cooperatively, possibly only when the current render completes.
    pub fn cancel(&self) {
        self.shared
            .cancellation
            .cancelled
            .store(true, Ordering::Release);
        let waker = lock(&self.shared.slot).waker.take();
        self.shared.available.notify_all();
        wake(waker);
    }

    pub fn is_cancelled(&self) -> bool {
        self.shared.cancellation.is_cancelled()
    }

    pub fn try_take(&mut self) -> Option<Result<T>> {
        if self.taken {
            return None;
        }
        let mut slot = lock(&self.shared.slot);
        let result = if self.is_cancelled() {
            slot.result.take();
            Some(Err(Error::Cancelled))
        } else {
            slot.result.take()
        };
        if result.is_some() {
            self.taken = true;
        }
        result
    }

    /// Block until completion or cancellation. Do not call on a UI thread.
    pub fn wait(mut self) -> Result<T> {
        if self.taken {
            return Err(Error::InvalidArgument("ticket result already taken"));
        }
        let mut slot = lock(&self.shared.slot);
        loop {
            if self.is_cancelled() {
                self.taken = true;
                return Err(Error::Cancelled);
            }
            if let Some(result) = slot.result.take() {
                self.taken = true;
                return result;
            }
            slot = self
                .shared
                .available
                .wait(slot)
                .unwrap_or_else(|error| error.into_inner());
        }
    }
}

impl<T> Future for Ticket<T> {
    type Output = Result<T>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        assert!(!this.taken, "ticket polled after completion");
        let mut slot = lock(&this.shared.slot);
        if this.is_cancelled() {
            this.taken = true;
            return Poll::Ready(Err(Error::Cancelled));
        }
        if let Some(result) = slot.result.take() {
            this.taken = true;
            return Poll::Ready(result);
        }
        if slot
            .waker
            .as_ref()
            .is_none_or(|old| !old.will_wake(cx.waker()))
        {
            slot.waker = Some(cx.waker().clone());
        }
        Poll::Pending
    }
}

impl<T> Drop for Ticket<T> {
    fn drop(&mut self) {
        if !self.taken {
            self.cancel();
        }
    }
}

pub(super) struct Completion<T> {
    shared: Arc<Shared<T>>,
    completed: bool,
}

impl<T> Completion<T> {
    pub fn pair(run: Arc<RunState>, group: Option<&CancelGroup>) -> (Ticket<T>, Self) {
        let cancellation = Arc::new(Cancellation {
            cancelled: AtomicBool::new(false),
            groups: Mutex::new(Vec::new()),
            run,
        });
        if let Some(group) = group {
            cancellation.group(group);
        }
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot {
                result: None,
                waker: None,
            }),
            available: Condvar::new(),
            cancellation,
        });
        (
            Ticket {
                shared: shared.clone(),
                taken: false,
            },
            Self {
                shared,
                completed: false,
            },
        )
    }
    pub fn cancellation(&self) -> Arc<Cancellation> {
        self.shared.cancellation.clone()
    }
    pub fn finish(mut self, result: Result<T>) {
        self.send(result);
    }
    fn send(&mut self, result: Result<T>) {
        self.completed = true;
        if matches!(result, Err(Error::Cancelled)) {
            self.shared
                .cancellation
                .cancelled
                .store(true, Ordering::Release);
        }
        let result = if self.shared.cancellation.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            result
        };
        let waker = {
            let mut slot = lock(&self.shared.slot);
            slot.result = Some(result);
            slot.waker.take()
        };
        self.shared.available.notify_all();
        wake(waker);
    }
}
impl<T> Drop for Completion<T> {
    fn drop(&mut self) {
        if !self.completed {
            self.send(Err(Error::Poisoned));
        }
    }
}
