// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Synchronous JavaScript dialog requests; no UI or filesystem policy is implied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormDialog {
    Alert {
        message: String,
        title: String,
        /// PDFium codes: 0 OK, 1 OK/Cancel, 2 Yes/No, 3 Yes/No/Cancel.
        buttons: i32,
        /// PDFium codes: 0 error, 1 warning, 2 question, 3 status.
        icon: i32,
    },
    Response {
        question: String,
        title: String,
        default: String,
        label: String,
        password: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormDialogResponse {
    Ok,
    Cancel,
    Yes,
    No,
    Text(String),
}
impl FormDialog {
    pub fn accepts(&self, response: &FormDialogResponse) -> bool {
        use FormDialogResponse::*;
        match self {
            Self::Alert { buttons, .. } => matches!(
                (buttons, response),
                (0, Ok) | (1, Ok | Cancel) | (2, Yes | No) | (3, Yes | No | Cancel)
            ),
            Self::Response { .. } => matches!(response, Text(_) | Cancel),
        }
    }
}
