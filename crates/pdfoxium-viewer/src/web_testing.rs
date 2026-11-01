// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Opt-in browser smoke bridge. Absent from normal builds; no socket transport.
use crate::control::Request;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{mpsc, Arc},
};
use wasm_bindgen::prelude::*;
thread_local! {
    static REQUESTS: RefCell<Vec<Request>> = const { RefCell::new(Vec::new()) };
    static REPLIES: RefCell<BTreeMap<u32, mpsc::Receiver<Result<serde_json::Value, String>>>> = const { RefCell::new(BTreeMap::new()) };
    static NEXT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static TLS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}
#[wasm_bindgen]
pub fn test_request(method: String, params: String) -> Result<u32, JsValue> {
    let params = serde_json::from_str(&params).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let (reply, rx) = mpsc::channel();
    let id = NEXT.with(|v| {
        v.set(v.get() + 1);
        v.get()
    });
    REPLIES.with_borrow_mut(|replies| replies.insert(id, rx));
    REQUESTS.with_borrow_mut(|requests| {
        requests.push(Request {
            method,
            params,
            reply,
            since: web_time::Instant::now(),
        })
    });
    super::CONTEXT.with_borrow(|ctx| {
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    });
    Ok(id)
}
#[wasm_bindgen]
pub fn test_reply(id: u32) -> Option<String> {
    REPLIES.with_borrow_mut(|replies| {
        let reply = replies.get(&id)?.try_recv().ok()?;
        replies.remove(&id);
        Some(
            match reply {
                Ok(value) => serde_json::json!({"result":value}),
                Err(error) => serde_json::json!({"error":error}),
            }
            .to_string(),
        )
    })
}
pub fn take_requests() -> Vec<Request> {
    REQUESTS.with_borrow_mut(std::mem::take)
}
/// Exercise the real launch adapter, nested startup, synchronization, distinct
/// TLS/thread IDs and repeated joined teardown without blocking the UI thread.
#[wasm_bindgen]
pub async fn test_threads() -> Result<u32, JsValue> {
    TLS.with(|v| v.set(7));
    let handle = wasm_bindgen_spawn::spawn(|| {
        use pdfoxium::thread::runtime;
        use wasm_sync::{Condvar, Mutex};
        TLS.with(|v| {
            assert_eq!(v.get(), 0);
            v.set(11);
        });
        let pair = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_pair = pair.clone();
        let (tx, rx) = mpsc::channel();
        let nested = runtime::spawn(move || {
            TLS.with(|v| {
                assert_eq!(v.get(), 0);
                v.set(13);
            });
            *worker_pair.0.lock().unwrap() = true;
            worker_pair.1.notify_one();
            tx.send(42).unwrap();
            std::thread::current().id()
        });
        let mut ready = pair.0.lock().unwrap();
        while !*ready {
            ready = pair.1.wait(ready).unwrap();
        }
        drop(ready);
        assert_eq!(rx.recv().unwrap(), 42);
        assert_ne!(nested.join().unwrap(), std::thread::current().id());
        TLS.with(|v| assert_eq!(v.get(), 11));
        for i in 0..16 {
            assert_eq!(runtime::spawn(move || i).join().unwrap(), i);
        }
        42
    });
    let answer = handle
        .await
        .map_err(|_| JsValue::from_str("thread probe failed"))?;
    TLS.with(|v| assert_eq!(v.get(), 7));
    Ok(answer)
}
