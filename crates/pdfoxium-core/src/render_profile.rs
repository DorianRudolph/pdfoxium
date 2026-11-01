// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! Opt-in, per-thread stage timings for rendering investigations.
//! Times are exclusive of nested scopes. Normal builds contain no hooks.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Opt-in event trace for matching bitmap-cache requests to actual hint draws.
/// Keep this separate from timings: logging is diagnostic, never a benchmark.
pub fn trace_glyphs() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("PDFOXIUM_TRACE_GLYPHS").is_some())
}

#[derive(Default)]
struct State {
    active: bool,
    stack: Vec<(Instant, Duration)>,
    totals: BTreeMap<&'static str, (usize, Duration)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub struct Scope(Option<&'static str>);

impl Scope {
    pub fn new(name: &'static str) -> Self {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if !state.active {
                return Self(None);
            }
            state.stack.push((Instant::now(), Duration::ZERO));
            Self(Some(name))
        })
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let Some(name) = self.0 else { return };
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let (start, children) = state.stack.pop().unwrap();
            let elapsed = start.elapsed();
            if let Some((_, parent_children)) = state.stack.last_mut() {
                *parent_children += elapsed;
            }
            let total = state.totals.entry(name).or_default();
            total.0 += 1;
            total.1 += elapsed.saturating_sub(children);
        });
    }
}

pub struct Session(Option<Scope>);

impl Session {
    pub fn new(name: &'static str) -> Self {
        STATE.with(|state| {
            *state.borrow_mut() = State {
                active: true,
                ..State::default()
            }
        });
        Self(Some(Scope::new(name)))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.0.take();
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.active = false;
            let mut totals: Vec<_> = state.totals.iter().collect();
            totals.sort_by_key(|(_, (_, time))| std::cmp::Reverse(*time));
            for (name, (calls, time)) in totals {
                eprintln!(
                    "profile: {name}: {calls} calls, {:.3} ms exclusive",
                    time.as_secs_f64() * 1_000.0
                );
            }
        });
    }
}
