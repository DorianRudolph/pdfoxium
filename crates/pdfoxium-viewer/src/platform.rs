// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
pub use pdfoxium::thread::runtime::spawn;

#[cfg(not(target_arch = "wasm32"))]
pub type Repaint = eframe::egui::Context;
#[cfg(not(target_arch = "wasm32"))]
pub fn repaint(ctx: &eframe::egui::Context) -> Repaint {
    ctx.clone()
}

// Never share egui's parking_lot locks or JS-backed repaint callback with a
// worker. The browser event loop polls this atomic and repaints on the UI thread.
#[cfg(target_arch = "wasm32")]
static REPAINT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct Repaint;
#[cfg(target_arch = "wasm32")]
impl Repaint {
    pub fn request_repaint(&self) {
        REPAINT.store(true, std::sync::atomic::Ordering::Release);
    }
}
#[cfg(target_arch = "wasm32")]
pub fn repaint(_ctx: &eframe::egui::Context) -> Repaint {
    Repaint
}
#[cfg(target_arch = "wasm32")]
pub fn needs_repaint() -> bool {
    REPAINT.swap(false, std::sync::atomic::Ordering::AcqRel)
}

// GPU/egui handles must be released on the UI thread in browser builds.
#[cfg(not(target_arch = "wasm32"))]
pub fn discard<T: Send + 'static>(value: T) {
    spawn(move || drop(value));
}
#[cfg(target_arch = "wasm32")]
pub fn discard<T>(value: T) {
    drop(value);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn language() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
        .next()
        .filter(|v| v != "C" && v != "POSIX")
        .map(|v| v.split('.').next().unwrap_or("").replace('_', "-"))
        .unwrap_or_default()
}
#[cfg(target_arch = "wasm32")]
pub fn language() -> String {
    web_sys::window()
        .and_then(|w| w.navigator().language())
        .unwrap_or_default()
}
