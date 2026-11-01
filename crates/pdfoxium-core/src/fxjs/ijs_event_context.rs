// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Safe runtime/event boundary from `fxjs/ijs_event_context.h` and the
//! `IJS_Runtime::JS_Error`/field-event operations used by CJS_EventContext.
//! Script execution belongs to the real engine adapter; this interface has no
//! no-JavaScript success implementation.
use super::cjs_event_context::{CjsEventContext, Kind};
use crate::fpdfsdk::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment;
use std::rc::Rc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsError {
    pub line: i32,
    pub column: i32,
    pub message: Vec<u16>,
}
impl JsError {
    pub fn new(line: i32, column: i32, message: impl Into<Vec<u16>>) -> Self {
        Self {
            line,
            column,
            message: message.into(),
        }
    }
    pub fn text(message: &str) -> Self {
        Self::new(1, 1, message.encode_utf16().collect::<Vec<_>>())
    }
}

/// Native FieldEvent compares the complete field name and event kind. Rc keeps
/// the immutable UTF-16 name shared with its event rather than cloning a buffer.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct FieldEvent {
    pub target_name: Rc<[u16]>,
    pub kind: Kind,
}

/// The real engine owns the active event stack and the duplicate-event set.
/// Implementations release their set/stack borrows before executing JavaScript.
pub trait EventRuntime {
    fn execute_script(&self, script: &[u16], event: Rc<CjsEventContext>) -> Result<(), JsError>;
    fn add_event(&self, event: FieldEvent) -> bool;
    fn remove_event(&self, event: &FieldEvent);
    fn environment(&self) -> Option<Rc<CpdfSdkFormFillEnvironment>>;
}
