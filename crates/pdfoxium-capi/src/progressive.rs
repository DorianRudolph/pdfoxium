// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! `fpdfsdk/fpdf_progressive.cpp` and `public/fpdf_progressive.h`.
//! Active calls retain original page/bitmap/document owners. Pause callbacks
//! run after every page/device/buffer borrow has ended, allowing cancellation,
//! replacement, or destruction of public handles without dangling references.
use super::*;
use pdfoxium::engine::render_page_with_context;
use pdfoxium_core::fpdfapi::render::{
    cpdf_pagerendercontext::CpdfPageRenderContext,
    cpdf_progressiverenderer::{RenderStatus, RenderStep},
    cpdf_renderoptions::RenderColorScheme,
};
use std::cell::Cell;
pub const FPDF_RENDER_READY: c_int = 0;
pub const FPDF_RENDER_TOBECONTINUED: c_int = 1;
pub const FPDF_RENDER_DONE: c_int = 2;
pub const FPDF_RENDER_FAILED: c_int = 3;
#[allow(non_snake_case, non_camel_case_types)]
#[repr(C)]
pub struct IFSDK_PAUSE {
    pub version: c_int,
    pub NeedToPauseNow: Option<unsafe extern "C" fn(*mut IFSDK_PAUSE) -> c_int>,
    pub user: *mut c_void,
}
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct FPDF_COLORSCHEME {
    pub path_fill_color: c_ulong,
    pub path_stroke_color: c_ulong,
    pub text_fill_color: c_ulong,
    pub text_stroke_color: c_ulong,
}
struct ProgressiveJob {
    context: CpdfPageRenderContext,
    bitmap: Rc<RefCell<BitmapStorage>>,
    _document: Rc<RefCell<CpdfDocument>>,
}
#[derive(Default)]
pub(super) struct ProgressiveSlot {
    job: RefCell<Option<ProgressiveJob>>,
    cancelled: Cell<bool>,
}
impl ProgressiveSlot {
    pub(super) fn cancel(&self) {
        self.cancelled.set(true);
        self.job.borrow_mut().take();
    }
}
impl ProgressiveJob {
    unsafe fn step(&mut self) -> RenderStep {
        let mut storage = self.bitmap.borrow_mut();
        match &mut *storage {
            BitmapStorage::Owned(bitmap) => self.context.render_step(bitmap),
            BitmapStorage::External {
                width,
                height,
                format,
                pitch,
                buffer,
            } => {
                let length = *pitch as usize * *height as usize;
                // The public progressive contract keeps an external buffer
                // writable until rendering is closed, including paused calls.
                let slice = unsafe { std::slice::from_raw_parts_mut(buffer.as_ptr(), length) };
                let Some(mut bitmap) =
                    CfxDibitmapData::from_buffer(*width, *height, *format, *pitch, slice)
                else {
                    return RenderStep::Failed;
                };
                let result = self.context.render_step(&mut bitmap);
                if let Some(owned) = bitmap.into_owned_if_allocated() {
                    *storage = BitmapStorage::Owned(owned);
                }
                result
            }
        }
    }
}
unsafe fn run(slot: Rc<ProgressiveSlot>, pause: *mut IFSDK_PAUSE) -> c_int {
    // Taking the job permits callback reentry without borrowing the slot. A
    // nested Continue observes an in-progress call and returns FAILED.
    let Some(mut job) = slot.job.borrow_mut().take() else {
        return FPDF_RENDER_FAILED;
    };
    let result = loop {
        if slot.cancelled.get() {
            break FPDF_RENDER_FAILED;
        }
        let step = unsafe { job.step() };
        match step {
            RenderStep::Continue => {}
            RenderStep::CheckPause => {
                let callback =
                    unsafe { pause.as_ref() }.and_then(|interface| interface.NeedToPauseNow);
                let paused = callback.is_some_and(|callback| unsafe { callback(pause) } != 0);
                if slot.cancelled.get() {
                    break FPDF_RENDER_FAILED;
                }
                if paused {
                    break job.context.status() as c_int;
                }
            }
            RenderStep::MaskBreak => break job.context.status() as c_int,
            RenderStep::Done => break FPDF_RENDER_DONE,
            RenderStep::Failed => break FPDF_RENDER_FAILED,
        }
    };
    if !slot.cancelled.get() {
        *slot.job.borrow_mut() = Some(job);
    }
    result
}
/// # Safety
/// Handles must be live or null. A supplied external pixel buffer remains
/// writable until RenderPage_Close, including across pause callbacks. `pause`
/// and its callback/user data must remain valid throughout this call.
#[no_mangle]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub unsafe extern "C" fn FPDF_RenderPageBitmapWithColorScheme_Start(
    bitmap: *mut c_void,
    page: *mut c_void,
    start_x: c_int,
    start_y: c_int,
    size_x: c_int,
    size_y: c_int,
    rotate: c_int,
    flags: c_int,
    color_scheme: *const FPDF_COLORSCHEME,
    pause: *mut IFSDK_PAUSE,
) -> c_int {
    if bitmap.is_null() || page.is_null() || pause.is_null() || unsafe { (*pause).version } != 1 {
        return FPDF_RENDER_FAILED;
    }
    let (Some(right), Some(bottom)) = (start_x.checked_add(size_x), start_y.checked_add(size_y))
    else {
        return FPDF_RENDER_FAILED;
    };
    let slot = {
        let page = unsafe { &mut *page.cast::<PageHandle>() };
        let bitmap = unsafe { &*bitmap.cast::<BitmapHandle>() };
        page.progressive.cancel();
        let colors = unsafe { color_scheme.as_ref() }.map(|colors| RenderColorScheme {
            path_fill_color: colors.path_fill_color as u32,
            path_stroke_color: colors.path_stroke_color as u32,
            text_fill_color: colors.text_fill_color as u32,
            text_stroke_color: colors.text_stroke_color as u32,
        });
        let mut context = render_page_with_context(
            match page.retained_pdf_page() {
                Some(page) => page,
                None => return FPDF_RENDER_FAILED,
            },
            &page._document.borrow(),
            FxRect::new(start_x, start_y, right, bottom),
            rotate,
            flags,
            colors,
        );
        if context.start() != RenderStatus::ToBeContinued {
            return FPDF_RENDER_FAILED;
        }
        let slot = Rc::new(ProgressiveSlot {
            job: RefCell::new(Some(ProgressiveJob {
                context,
                bitmap: bitmap.storage.clone(),
                _document: page._document.clone(),
            })),
            cancelled: Cell::new(false),
        });
        page.progressive = slot.clone();
        slot
    };
    unsafe { run(slot, pause) }
}
/// # Safety
/// The same handle, pause and external-buffer conditions as the color-scheme
/// variant apply.
#[no_mangle]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub unsafe extern "C" fn FPDF_RenderPageBitmap_Start(
    bitmap: *mut c_void,
    page: *mut c_void,
    start_x: c_int,
    start_y: c_int,
    size_x: c_int,
    size_y: c_int,
    rotate: c_int,
    flags: c_int,
    pause: *mut IFSDK_PAUSE,
) -> c_int {
    unsafe {
        FPDF_RenderPageBitmapWithColorScheme_Start(
            bitmap,
            page,
            start_x,
            start_y,
            size_x,
            size_y,
            rotate,
            flags,
            std::ptr::null(),
            pause,
        )
    }
}
/// # Safety
/// The page and pause interface must be live or null. The target's external
/// buffer, if supplied, remains valid until the rendering is closed.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_RenderPage_Continue(
    page: *mut c_void,
    pause: *mut IFSDK_PAUSE,
) -> c_int {
    if page.is_null() || unsafe { pause.as_ref() }.is_some_and(|interface| interface.version != 1) {
        return FPDF_RENDER_FAILED;
    }
    let slot = unsafe { &*page.cast::<PageHandle>() }.progressive.clone();
    unsafe { run(slot, pause) }
}
/// # Safety
/// The page must be live or null. May be called from its pause callback.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_RenderPage_Close(page: *mut c_void) {
    if let Some(page) = unsafe { page.cast::<PageHandle>().as_ref() } {
        page.progressive.cancel();
    }
}
#[cfg(test)]
#[path = "progressive_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "progressive_image_tests.rs"]
mod image_tests;
