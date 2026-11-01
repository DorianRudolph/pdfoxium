// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use serde_json::Value;
use std::sync::mpsc;
use web_time::Instant;
pub type Reply = mpsc::Sender<Result<Value, String>>;
pub struct Request {
    pub method: String,
    pub params: Value,
    pub reply: Reply,
    pub since: Instant,
}

#[cfg(not(target_arch = "wasm32"))]
#[path = "control_native.rs"]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::{call, Server};

// Browser smoke builds connect an in-memory request bridge. Browser builds
// never expose the native socket transport or filesystem control endpoint.
#[cfg(target_arch = "wasm32")]
pub struct Server {
    pub requests: mpsc::Receiver<Request>,
}
