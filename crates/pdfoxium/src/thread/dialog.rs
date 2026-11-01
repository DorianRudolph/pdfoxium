// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::ticket::lock;
use crate::{Error, FormDialog, FormDialogResponse, Result};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Condvar, Mutex};
#[cfg(target_arch = "wasm32")]
use wasm_sync::{Condvar, Mutex};

#[derive(Default)]
struct State {
    closed: bool,
    next: u64,
    pending: Option<(u64, FormDialog, Option<FormDialogResponse>)>,
}
#[derive(Default)]
pub(super) struct Dialogs {
    state: Mutex<State>,
    wake: Condvar,
}
impl Dialogs {
    pub fn request(
        &self,
        dialog: FormDialog,
        publish: impl FnOnce(u64, FormDialog),
    ) -> FormDialogResponse {
        let mut state = lock(&self.state);
        if state.closed {
            return FormDialogResponse::Cancel;
        }
        state.next = state.next.wrapping_add(1);
        let id = state.next;
        state.pending = Some((id, dialog.clone(), None));
        drop(state);
        publish(id, dialog);
        let mut state = lock(&self.state);
        loop {
            if state.closed {
                state.pending = None;
                return FormDialogResponse::Cancel;
            }
            if let Some((_, _, answer)) = &mut state.pending {
                if let Some(answer) = answer.take() {
                    state.pending = None;
                    return answer;
                }
            }
            state = self.wake.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }
    pub fn answer(&self, id: u64, response: FormDialogResponse) -> Result<()> {
        let mut state = lock(&self.state);
        let Some((pending_id, dialog, answer)) = &mut state.pending else {
            return Err(Error::Stale);
        };
        if *pending_id != id || answer.is_some() {
            return Err(Error::Stale);
        }
        if !dialog.accepts(&response) {
            return Err(Error::InvalidArgument("response does not match dialog"));
        }
        *answer = Some(response);
        self.wake.notify_one();
        Ok(())
    }
    pub fn close(&self) {
        lock(&self.state).closed = true;
        self.wake.notify_all();
    }
}
