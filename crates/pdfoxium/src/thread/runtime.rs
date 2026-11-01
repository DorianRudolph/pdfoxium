// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Small thread-launch boundary shared with the viewer. Browser hosts must
//! initialize wasm-bindgen-spawn's dispatcher before starting library workers.
#[cfg(not(target_arch = "wasm32"))]
pub use std::thread::{spawn, JoinHandle};

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_named<F, T>(name: String, f: F) -> std::io::Result<JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new().name(name).spawn(f)
}

/// Join after signalling shutdown, avoiding self-join. Browser UI teardown joins
/// asynchronously; it must never wait for a worker or its event loop.
#[cfg(not(target_arch = "wasm32"))]
pub fn join<T>(handle: JoinHandle<T>) {
    if handle.thread().id() != std::thread::current().id() {
        let _ = handle.join();
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::{sync::Arc, thread::ThreadId};
    use wasm_sync::Mutex;

    pub struct JoinHandle<T: Send + 'static> {
        handle: wasm_bindgen_spawn::JoinHandle<T>,
        id: Arc<Mutex<Option<ThreadId>>>,
    }
    impl<T: Send + 'static> JoinHandle<T> {
        pub fn is_finished(&self) -> bool {
            self.handle.is_finished()
        }
        /// Blocking join is for background workers only.
        pub fn join(self) -> std::thread::Result<T> {
            self.handle.join()
        }
    }
    pub fn spawn_named<F, T>(_name: String, f: F) -> std::io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let id = Arc::new(Mutex::new(None));
        let worker_id = id.clone();
        let handle = wasm_bindgen_spawn::try_spawn(move || {
            *worker_id.lock().unwrap_or_else(|p| p.into_inner()) =
                Some(std::thread::current().id());
            f()
        })
        .map_err(std::io::Error::other)?;
        Ok(JoinHandle { handle, id })
    }
    pub fn spawn<F, T>(f: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        spawn_named(String::new(), f).expect("cannot start WASM worker")
    }
    pub fn join<T: Send + 'static>(handle: JoinHandle<T>) {
        if *handle.id.lock().unwrap_or_else(|p| p.into_inner()) == Some(std::thread::current().id())
        {
            return;
        }
        if web_sys::window().is_some() {
            wasm_bindgen_futures::spawn_local(async move {
                let _ = handle.handle.await;
            });
        } else {
            let _ = handle.join();
        }
    }
}
#[cfg(target_arch = "wasm32")]
pub use browser::{join, spawn, spawn_named, JoinHandle};
