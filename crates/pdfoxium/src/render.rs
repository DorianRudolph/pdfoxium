// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Synchronous and progressive AGG rendering over the same retained engine job.
//!
//! Mirrors `fpdfsdk/fpdf_progressive.cpp`: a step releases all device and buffer
//! borrows before invoking an embedding pause callback. The public page lease
//! remains held until the job is finished or dropped.

use crate::{
    Bitmap, Color, Error, Matrix, Page, PageLease, PixelFormat, Pixels, PixelsMut, Result,
};
use pdfoxium_core::{
    fpdfapi::render::{
        cpdf_pagerendercontext::CpdfPageRenderContext,
        cpdf_progressiverenderer::{RenderStatus, RenderStep},
    },
    fxcrt::fx_coordinates::FxRect,
    fxge::dib::{cfx_dibitmap::CfxDibitmapData, fx_dib::FxdibFormat},
};

/// Clockwise rotation applied in addition to the page's own rotation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(i32)]
pub enum Rotation {
    #[default]
    None = 0,
    Cw90 = 1,
    Cw180 = 2,
    Cw270 = 3,
}

/// A pixel rectangle within the full rendered page, after applying rotation.
///
/// The origin is the canvas's top-left corner. Coordinates and dimensions are
/// validated when rendering: the rectangle must be nonempty and entirely inside
/// the canvas selected by the render configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DeviceRect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Size {
    Dpi(f32),
    Scale(f32),
    Fit(u32, u32),
    FitWidth(u32),
}

/// Render settings. Defaults to 72 DPI, white background, BGRA and annotations.
///
/// Values are checked when rendering. Only noninteractive annotations are
/// included; drawing interactive form state requires the future form API.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderConfig {
    size: Size,
    pub(crate) rotation: Rotation,
    background: Option<Color>,
    annotations: bool,
    print: bool,
    grayscale: bool,
    format: PixelFormat,
    device_clip: Option<DeviceRect>,
    pub(crate) optional_content: Option<crate::OptionalContentSnapshot>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            size: Size::Scale(1.0),
            rotation: Rotation::None,
            background: Some(Color::WHITE),
            annotations: true,
            print: false,
            grayscale: false,
            format: PixelFormat::Bgra8,
            device_clip: None,
            optional_content: None,
        }
    }
}

impl RenderConfig {
    pub(crate) fn empty_bitmap(&self, width: f32, height: f32) -> Result<Bitmap> {
        let (w, h, _) = self.geometry(width, height)?;
        let mut bitmap = Bitmap::new(w, h, self.format)?;
        if let Some(color) = self.background {
            bitmap.view_mut().clear(color);
        }
        Ok(bitmap)
    }
    /// Use an immutable optional-content state instead of the document's current view.
    pub fn optional_content(mut self, state: crate::OptionalContentSnapshot) -> Self {
        self.optional_content = Some(state);
        self
    }
    pub fn dpi(dpi: f32) -> Self {
        Self {
            size: Size::Dpi(dpi),
            ..Self::default()
        }
    }
    pub fn scale(scale: f32) -> Self {
        Self {
            size: Size::Scale(scale),
            ..Self::default()
        }
    }
    /// Preserve aspect ratio and fit inside this pixel rectangle.
    pub fn fit(width: u32, height: u32) -> Self {
        Self {
            size: Size::Fit(width, height),
            ..Self::default()
        }
    }
    /// Preserve aspect ratio and render at the given pixel width.
    pub fn fit_width(width: u32) -> Self {
        Self {
            size: Size::FitWidth(width),
            ..Self::default()
        }
    }
    pub fn rotation(mut self, rotation: Rotation) -> Self {
        self.rotation = rotation;
        self
    }
    /// `None` preserves existing target pixels (owned targets begin transparent).
    pub fn background(mut self, background: Option<Color>) -> Self {
        self.background = background;
        self
    }
    pub fn annotations(mut self, annotations: bool) -> Self {
        self.annotations = annotations;
        self
    }
    pub fn print(mut self, print: bool) -> Self {
        self.print = print;
        self
    }
    pub fn grayscale(mut self, grayscale: bool) -> Self {
        self.grayscale = grayscale;
        self
    }
    /// Format for owned output. Borrowed output uses its target's format.
    pub fn format(mut self, format: PixelFormat) -> Self {
        self.format = format;
        self
    }

    pub(crate) fn covers(&self, other: &Self) -> bool {
        let mut a = self.clone();
        let mut b = other.clone();
        a.device_clip = None;
        b.device_clip = None;
        if a != b {
            return false;
        }
        match (self.device_clip, other.device_clip) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(a), Some(b)) => {
                a.x <= b.x
                    && a.y <= b.y
                    && u64::from(a.x) + u64::from(a.width) >= u64::from(b.x) + u64::from(b.width)
                    && u64::from(a.y) + u64::from(a.height) >= u64::from(b.y) + u64::from(b.height)
            }
        }
    }

    /// Render only this rectangle of the full scaled and rotated page canvas.
    ///
    /// The output is exactly `rect.width` by `rect.height` pixels. Scaling is
    /// calculated from the full page, not from the region size; no full-page
    /// bitmap is allocated. The rectangle must fit entirely inside that canvas.
    /// Geometric clipping can produce small antialiasing differences compared
    /// with cropping a whole-page render. Applies equally to synchronous/progressive
    /// and owned/borrowed targets.
    pub fn device_clip(mut self, rect: DeviceRect) -> Self {
        self.device_clip = Some(rect);
        self
    }

    pub(crate) fn geometry(&self, width: f32, height: f32) -> Result<(u32, u32, FxRect)> {
        let (full_width, full_height) = self.dimensions(width, height)?;
        let region = self
            .device_clip
            .unwrap_or(DeviceRect::new(0, 0, full_width, full_height));
        if region.width == 0
            || region.height == 0
            || region
                .x
                .checked_add(region.width)
                .is_none_or(|right| right > full_width)
            || region
                .y
                .checked_add(region.height)
                .is_none_or(|bottom| bottom > full_height)
        {
            return Err(Error::InvalidArgument(
                "device clip must be nonempty and inside the rendered page",
            ));
        }
        // Validation above bounds all coordinates by the i32-sized canvas.
        // Keep its full width/height in the display matrix; only its origin moves.
        let x = region.x as i32;
        let y = region.y as i32;
        Ok((
            region.width,
            region.height,
            FxRect::new(-x, -y, full_width as i32 - x, full_height as i32 - y),
        ))
    }

    fn dimensions(&self, width: f32, height: f32) -> Result<(u32, u32)> {
        let (width, height) = match self.rotation {
            Rotation::None | Rotation::Cw180 => (width as f64, height as f64),
            Rotation::Cw90 | Rotation::Cw270 => (height as f64, width as f64),
        };
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(Error::InvalidArgument(
                "page dimensions must be positive and finite",
            ));
        }
        let scale = match self.size {
            Size::Dpi(dpi) => dpi as f64 / 72.0,
            Size::Scale(scale) => scale as f64,
            Size::Fit(w, h) => {
                if w == 0 || h == 0 {
                    return Err(Error::InvalidArgument("fit dimensions must be positive"));
                }
                (w as f64 / width).min(h as f64 / height)
            }
            Size::FitWidth(w) => w as f64 / width,
        };
        if !scale.is_finite() || scale <= 0.0 {
            return Err(Error::InvalidArgument(
                "render scale must be positive and finite",
            ));
        }
        let (w, h) = match self.size {
            Size::Dpi(dpi) => (
                (width * dpi as f64 / 72.0).ceil(),
                (height * dpi as f64 / 72.0).ceil(),
            ),
            _ => ((width * scale).ceil(), (height * scale).ceil()),
        };
        if w < 1.0 || h < 1.0 || w > i32::MAX as f64 || h > i32::MAX as f64 {
            return Err(Error::InvalidArgument(
                "render dimensions exceed the renderer's limit",
            ));
        }
        let mut w = w as u32;
        let mut h = h as u32;
        // Avoid a one-pixel overshoot from floating point roundoff at the fit
        // boundary while retaining ceil for the other, proportional dimension.
        match self.size {
            Size::Fit(max_w, max_h) => {
                w = w.min(max_w);
                h = h.min(max_h);
            }
            Size::FitWidth(exact_w) => w = exact_w,
            Size::Dpi(_) | Size::Scale(_) => {}
        }
        Ok((w, h))
    }

    fn flags(&self, format: PixelFormat) -> i32 {
        i32::from(self.annotations)
            | if self.grayscale { 0x08 } else { 0 }
            | if format == PixelFormat::Rgba8 {
                0x10
            } else {
                0
            }
            | if self.print { 0x800 } else { 0 }
    }
}

/// State returned after running a progressive render.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Progress {
    Paused,
    Done,
}

/// Finished output. Borrowed jobs return the caller's original target view.
#[derive(Debug)]
pub enum RenderOutput<'buf> {
    Owned(Bitmap),
    Borrowed(PixelsMut<'buf>),
}

enum Target<'buf> {
    Owned(Bitmap),
    Borrowed(PixelsMut<'buf>),
}

impl Target<'_> {
    fn view(&self) -> Pixels<'_> {
        match self {
            Self::Owned(bitmap) => bitmap.view(),
            Self::Borrowed(pixels) => pixels.view(),
        }
    }
    fn view_mut(&mut self) -> PixelsMut<'_> {
        match self {
            Self::Owned(bitmap) => bitmap.view_mut(),
            Self::Borrowed(pixels) => pixels.reborrow(),
        }
    }
}

/// A same-thread render that can pause and resume. Dropping it cancels the job.
///
/// The job retains the page/document and exclusively reserves rendering of that
/// page. Its lifetime parameter describes only a borrowed pixel buffer. It is
/// deliberately neither clonable nor transferable to another thread.
pub struct RenderJob<'buf> {
    // Declaration order ensures retained core readers are dropped before the
    // lease is released, including on early errors and panic unwinding.
    context: CpdfPageRenderContext,
    target: Target<'buf>,
    lease: PageLease,
    failed: bool,
}

impl Page {
    /// Return the exact pixel dimensions selected by `config`, including rotation
    /// and rounding. Use these dimensions to allocate a borrowed render target.
    /// Loads the page if needed; returns `Busy` while this page has an active job.
    pub fn render_size(&self, config: &RenderConfig) -> Result<(u32, u32)> {
        let lease = self.acquire_render()?;
        let _guard = lease.enter_core()?;
        let page = lease.pdf_page()?;
        config
            .geometry(page.width(), page.height())
            .map(|(w, h, _)| (w, h))
    }

    /// Decode this page's existing `/Thumb` image into owned, straight-alpha pixels.
    /// Returns `None` when absent or undecodable. This never renders a substitute,
    /// adds a thumbnail, or modifies the document. The image keeps its stored
    /// orientation and dimensions; it may be outdated or unsuitable for display.
    /// Like rendering, this returns `Busy` while a page render lease is active.
    pub fn embedded_thumbnail(&self, format: PixelFormat) -> Result<Option<Bitmap>> {
        let lease = self.acquire_render()?;
        let _guard = lease.enter_core()?;
        let page = lease.pdf_page()?;
        let Some(source) = pdfoxium_core::fpdfsdk::fpdf_thumbnail::thumbnail_as_bitmap(&page)
        else {
            return Ok(None);
        };
        let mut bitmap = crate::media::bitmap(source)?;
        if format == PixelFormat::Rgba8 {
            for pixel in bitmap.data.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            bitmap.format = format;
        }
        Ok(Some(bitmap))
    }

    /// Map PDF page coordinates to pixels in the bitmap selected by `config`.
    /// Includes the crop box, intrinsic page rotation, requested rotation and
    /// device-clip translation, exactly as rendering does. No bitmap is allocated.
    /// Loads the page and returns `Busy` while it has an active render job.
    pub fn render_matrix(&self, config: &RenderConfig) -> Result<Matrix> {
        let lease = self.acquire_render()?;
        let _guard = lease.enter_core()?;
        let page = lease.pdf_page()?;
        let (_, _, rect) = config.geometry(page.width(), page.height())?;
        let m = page.display_matrix_for_rect(rect, config.rotation as i32);
        Ok(Matrix {
            a: m.a,
            b: m.b,
            c: m.c,
            d: m.d,
            e: m.e,
            f: m.f,
        })
    }

    /// Render to a new bitmap, blocking until complete.
    pub fn render(&self, config: &RenderConfig) -> Result<Bitmap> {
        let mut job = self.render_job(config)?;
        while job.resume(&mut || false)? != Progress::Done {}
        match job.finish()? {
            RenderOutput::Owned(bitmap) => Ok(bitmap),
            RenderOutput::Borrowed(_) => {
                Err(Error::Internal("owned render returned borrowed output"))
            }
        }
    }

    /// Render directly into caller-owned pixels, blocking until complete.
    /// The target dimensions must equal [`Self::render_size`] for `config`.
    pub fn render_into(&self, target: &mut PixelsMut<'_>, config: &RenderConfig) -> Result<()> {
        let mut job = self.render_job_into(target.reborrow(), config)?;
        while job.resume(&mut || false)? != Progress::Done {}
        job.finish()?;
        Ok(())
    }

    /// Prepare a progressive render with an owned bitmap. Call `resume` to render.
    pub fn render_job(&self, config: &RenderConfig) -> Result<RenderJob<'static>> {
        RenderJob::new(self.acquire_render()?, config, None)
    }

    /// Prepare a progressive render borrowing the caller's pixel buffer.
    /// The target dimensions must equal [`Self::render_size`] for `config`.
    pub fn render_job_into<'buf>(
        &self,
        target: PixelsMut<'buf>,
        config: &RenderConfig,
    ) -> Result<RenderJob<'buf>> {
        RenderJob::new(self.acquire_render()?, config, Some(target))
    }
}

impl<'buf> RenderJob<'buf> {
    fn new(
        lease: PageLease,
        config: &RenderConfig,
        borrowed: Option<PixelsMut<'buf>>,
    ) -> Result<Self> {
        let guard = lease.enter_core()?;
        let page = lease.pdf_page()?;
        let (width, height, display_rect) = config.geometry(page.width(), page.height())?;
        let mut target = match borrowed {
            Some(pixels) => {
                if pixels.width != width || pixels.height != height {
                    return Err(Error::InvalidArgument(
                        "target dimensions do not match render configuration",
                    ));
                }
                Target::Borrowed(pixels)
            }
            None => Target::Owned(Bitmap::new(width, height, config.format)?),
        };
        if let Some(color) = config.background {
            target.view_mut().clear(color);
        }
        let document = lease
            .core_document()
            .try_borrow()
            .map_err(|_| Error::Busy)?;
        if let Some(snapshot) = &config.optional_content {
            snapshot.validate(lease.document())?;
        }
        let mut context = crate::engine::render_page_with_snapshot(
            page,
            &document,
            display_rect,
            config.rotation as i32,
            config.flags(target.view().format()),
            None,
            config
                .optional_content
                .as_ref()
                .and_then(|s| s.state.clone()),
        );
        drop(document);
        lease.check_io()?;
        if context.start() != RenderStatus::ToBeContinued {
            return Err(Error::Render);
        }
        drop(guard);
        Ok(Self {
            context,
            target,
            lease,
            failed: false,
        })
    }

    /// Continue until complete or a core pause point at which `pause` returns true.
    /// Pause points are coarse: this does not guarantee a maximum call duration.
    /// The callback runs without a document or pixel-buffer borrow.
    pub fn resume(&mut self, pause: &mut dyn FnMut() -> bool) -> Result<Progress> {
        if self.failed {
            return Err(Error::Render);
        }
        loop {
            let step = {
                let _guard = self.lease.enter_core()?;
                let pixels = self.target.view_mut();
                let mut bitmap = CfxDibitmapData::from_buffer(
                    pixels.width as i32,
                    pixels.height as i32,
                    FxdibFormat::Bgra,
                    pixels.stride as u32,
                    pixels.data,
                )
                .ok_or(Error::Internal("validated render target was rejected"))?;
                let step = self.context.render_step(&mut bitmap);
                if bitmap.into_owned_if_allocated().is_some() {
                    self.failed = true;
                    return Err(Error::Internal("renderer replaced borrowed pixel storage"));
                }
                if let Err(error) = self.lease.check_io() {
                    self.failed = true;
                    return Err(error);
                }
                step
            };
            match step {
                RenderStep::Continue => {}
                RenderStep::CheckPause => {
                    if pause() {
                        return Ok(Progress::Paused);
                    }
                }
                RenderStep::MaskBreak => return Ok(Progress::Paused),
                RenderStep::Done => return Ok(Progress::Done),
                RenderStep::Failed => {
                    self.failed = true;
                    return Err(Error::Render);
                }
            }
        }
    }

    /// Read the current pixels between calls to `resume`.
    pub fn pixels(&self) -> Pixels<'_> {
        self.target.view()
    }

    /// Stable identity of the page retained by this job.
    pub fn page(&self) -> crate::PageId {
        self.lease.page_id()
    }

    /// Consume a completed job and release its page lease. Calling this before
    /// completion returns `NotFinished` and cancels the consumed job.
    pub fn finish(self) -> Result<RenderOutput<'buf>> {
        if self.failed || self.context.status() == RenderStatus::Failed {
            return Err(Error::Render);
        }
        if self.context.status() != RenderStatus::Done {
            return Err(Error::NotFinished);
        }
        let Self {
            context,
            target,
            lease,
            ..
        } = self;
        drop(context);
        drop(lease);
        Ok(match target {
            Target::Owned(bitmap) => RenderOutput::Owned(bitmap),
            Target::Borrowed(pixels) => RenderOutput::Borrowed(pixels),
        })
    }
}

/// Small pause predicates for progressive rendering.
pub mod pause {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    /// Continue at every optional pause point.
    pub fn never() -> impl FnMut() -> bool {
        || false
    }
    /// Pause when the flag is set. Dropping the job performs cancellation.
    pub fn flag(flag: &AtomicBool) -> impl FnMut() -> bool + '_ {
        || flag.load(Ordering::Relaxed)
    }
    /// Pause after this wall-clock budget. Construct a fresh predicate per resume.
    pub fn budget(duration: Duration) -> impl FnMut() -> bool {
        let start = Instant::now();
        move || start.elapsed() >= duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    // Many visible paths exercise the actual 100-object pause boundary. Exact
    // expected quadrant colors also ensure equality tests aren't blank renders.
    fn fixture() -> Vec<u8> {
        let mut stream = "1 0 0 rg\n".to_owned();
        for _ in 0..250 {
            stream.push_str("0 0 20 15 re f\n");
        }
        stream.push_str("0 0 1 rg 20 15 20 15 re f\n");
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 40 30] /Resources <<>> /Contents 4 0 R >>"
                .to_owned(),
            format!(
                "<< /Length {} >>\nstream\n{}endstream",
                stream.len(),
                stream
            ),
        ];
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, object).as_bytes());
        }
        let xref = pdf.len();
        pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
        );
        pdf
    }

    #[test]
    fn synchronous_progressive_and_borrowed_targets_match_visible_output() {
        let doc = Document::from_bytes(fixture()).unwrap();
        let page = doc.page(0).unwrap();
        let config = RenderConfig::default();
        let expected = page.render(&config).unwrap();
        assert_eq!((expected.width(), expected.height()), (40, 30));
        for (x, y, color) in [
            (5, 5, [255, 255, 255, 255]),
            (35, 5, [255, 0, 0, 255]),
            (5, 25, [0, 0, 255, 255]),
            (35, 25, [255, 255, 255, 255]),
        ] {
            let offset = y * expected.stride() + x * 4;
            assert_eq!(&expected.pixels()[offset..offset + 4], &color);
        }

        let mut job = page.render_job(&config).unwrap();
        let mut pauses = 0;
        while job
            .resume(&mut || {
                pauses += 1;
                true
            })
            .unwrap()
            != Progress::Done
        {}
        assert!(pauses >= 2);
        let RenderOutput::Owned(actual) = job.finish().unwrap() else {
            panic!("owned output");
        };
        assert_eq!(actual.pixels(), expected.pixels());

        let stride = expected.stride() + 12;
        let mut data = vec![0x9c; stride * 30 + 9];
        {
            let mut target = PixelsMut::new(&mut data, 40, 30, stride, PixelFormat::Bgra8).unwrap();
            page.render_into(&mut target, &config).unwrap();
        }
        for row in 0..30 {
            assert_eq!(
                &data[row * stride..row * stride + 160],
                &expected.pixels()[row * 160..(row + 1) * 160]
            );
            assert_eq!(&data[row * stride + 160..(row + 1) * stride], &[0x9c; 12]);
        }
        assert_eq!(&data[stride * 30..], &[0x9c; 9]);
    }

    #[test]
    fn lease_survives_parent_drop_and_blocks_alias_reentry_until_release() {
        let doc = Document::from_bytes(fixture()).unwrap();
        let page = doc.page(0).unwrap();
        let alias = page.clone();
        let mut job = page.render_job(&RenderConfig::default()).unwrap();
        drop(doc);
        drop(page);
        assert!(matches!(alias.unload(), Err(Error::Busy)));
        assert!(matches!(
            alias.render_size(&RenderConfig::default()),
            Err(Error::Busy)
        ));
        assert!(matches!(
            alias.render(&RenderConfig::default()),
            Err(Error::Busy)
        ));
        let mut called = false;
        assert_eq!(
            job.resume(&mut || {
                called = true;
                assert!(matches!(alias.unload(), Err(Error::Busy)));
                assert!(matches!(
                    alias.render(&RenderConfig::default()),
                    Err(Error::Busy)
                ));
                true
            })
            .unwrap(),
            Progress::Paused
        );
        assert!(called);
        drop(job);
        alias.unload().unwrap();
        assert!(alias.render(&RenderConfig::default()).is_ok());

        let unfinished = alias.render_job(&RenderConfig::default()).unwrap();
        assert!(matches!(unfinished.finish(), Err(Error::NotFinished)));
        alias.unload().unwrap();
    }

    #[test]
    fn render_size_matches_rotated_fractional_and_fit_targets() {
        let doc = Document::from_bytes(fixture()).unwrap();
        let page = doc.page(0).unwrap();
        for (config, expected) in [
            (RenderConfig::dpi(97.5), (55, 41)),
            (RenderConfig::dpi(97.5).rotation(Rotation::Cw90), (41, 55)),
            (RenderConfig::fit(11, 7).rotation(Rotation::Cw270), (6, 7)),
            (
                RenderConfig::fit_width(13).rotation(Rotation::Cw90),
                (13, 18),
            ),
        ] {
            page.unload().unwrap();
            assert_eq!(page.render_size(&config).unwrap(), expected);
            let mut target = Bitmap::new(expected.0, expected.1, PixelFormat::Bgra8).unwrap();
            page.render_into(&mut target.view_mut(), &config).unwrap();
            let owned = page.render(&config).unwrap();
            assert_eq!((owned.width(), owned.height()), expected);
            assert_eq!(target.pixels(), owned.pixels());
        }
        assert!(matches!(
            page.render_size(&RenderConfig::dpi(f32::NAN)),
            Err(Error::InvalidArgument(_))
        ));
        page.unload().unwrap();
    }

    #[test]
    fn borrowed_job_returns_original_target_after_parent_handles_drop() {
        let doc = Document::from_bytes(fixture()).unwrap();
        let page = doc.page(0).unwrap();
        let config = RenderConfig::dpi(97.5).rotation(Rotation::Cw90);
        let expected = page.render(&config).unwrap();
        let (width, height) = page.render_size(&config).unwrap();
        let row_bytes = width as usize * 4;
        let stride = row_bytes + 16;
        let length = stride * height as usize;
        let mut data = vec![0xa7; length + 8];
        let original = data.as_ptr();
        let target = PixelsMut::new(&mut data, width, height, stride, PixelFormat::Bgra8).unwrap();
        let mut job = page.render_job_into(target, &config).unwrap();
        drop(page);
        drop(doc);
        let mut pauses = 0;
        while job
            .resume(&mut || {
                pauses += 1;
                true
            })
            .unwrap()
            != Progress::Done
        {}
        assert!(pauses >= 2);
        let RenderOutput::Borrowed(target) = job.finish().unwrap() else {
            panic!("borrowed output");
        };
        assert_eq!(target.data().as_ptr(), original);
        assert_eq!(
            (target.width(), target.height(), target.stride()),
            (width, height, stride)
        );
        for row in 0..height as usize {
            assert_eq!(
                &target.data()[row * stride..row * stride + row_bytes],
                &expected.pixels()[row * row_bytes..(row + 1) * row_bytes]
            );
            assert_eq!(
                &target.data()[row * stride + row_bytes..(row + 1) * stride],
                &[0xa7; 16]
            );
        }
        drop(target);
        assert_eq!(&data[length..], &[0xa7; 8]);
    }

    #[test]
    fn rgba_rotation_transparency_and_invalid_configuration() {
        let doc = Document::from_bytes(fixture()).unwrap();
        let page = doc.page(0).unwrap();
        let bgra = page
            .render(&RenderConfig::default().background(None))
            .unwrap();
        let rgba = page
            .render(
                &RenderConfig::default()
                    .format(PixelFormat::Rgba8)
                    .background(None),
            )
            .unwrap();
        for (b, r) in bgra
            .pixels()
            .chunks_exact(4)
            .zip(rgba.pixels().chunks_exact(4))
        {
            assert_eq!([b[2], b[1], b[0], b[3]], r);
        }
        assert_eq!(&rgba.pixels()[..4], &[0; 4]);
        for rotation in [Rotation::Cw90, Rotation::Cw270] {
            let rotated = page
                .render(&RenderConfig::default().rotation(rotation))
                .unwrap();
            assert_eq!((rotated.width(), rotated.height()), (30, 40));
        }
        for config in [
            RenderConfig::dpi(0.0),
            RenderConfig::dpi(f32::NAN),
            RenderConfig::scale(f32::INFINITY),
            RenderConfig::scale(-1.0),
            RenderConfig::fit(0, 10),
            RenderConfig::fit_width(0),
            RenderConfig::scale(f32::MAX),
        ] {
            assert!(matches!(
                page.render(&config),
                Err(Error::InvalidArgument(_))
            ));
            page.unload().unwrap();
        }
        assert_eq!(
            RenderConfig::dpi(150.0).dimensions(612.0, 792.0).unwrap(),
            (1275, 1650)
        );
        assert_eq!(
            RenderConfig::fit(10, 10).dimensions(40.0, 30.0).unwrap(),
            (10, 8)
        );
        let mut wrong = Bitmap::new(1, 1, PixelFormat::Bgra8).unwrap();
        assert!(matches!(
            page.render_into(&mut wrong.view_mut(), &RenderConfig::default()),
            Err(Error::InvalidArgument(_))
        ));
        page.unload().unwrap();
    }

    fn crop(bitmap: &Bitmap, rect: DeviceRect) -> Vec<u8> {
        let mut result = Vec::new();
        for y in rect.y..rect.y + rect.height {
            let start = y as usize * bitmap.stride() + rect.x as usize * 4;
            result.extend_from_slice(&bitmap.pixels()[start..start + rect.width as usize * 4]);
        }
        result
    }

    #[test]
    fn regions_preserve_full_canvas_scale_rotation_and_pixel_format() {
        let doc = Document::from_bytes(fixture()).unwrap();
        let page = doc.page(0).unwrap();
        for rotation in [
            Rotation::None,
            Rotation::Cw90,
            Rotation::Cw180,
            Rotation::Cw270,
        ] {
            for format in [PixelFormat::Bgra8, PixelFormat::Rgba8] {
                for base in [
                    RenderConfig::dpi(97.5),
                    RenderConfig::fit(23, 19),
                    RenderConfig::scale(0.5),
                ] {
                    let base = base.rotation(rotation).format(format).background(None);
                    let full = page.render(&base).unwrap();
                    assert!(full.pixels().chunks_exact(4).any(|pixel| pixel[3] != 0));
                    let rect = DeviceRect::new(3, 2, full.width() - 8, full.height() - 6);
                    let expected = crop(&full, rect);
                    // Fresh page state exercises a region with no whole-page cache warmup.
                    page.unload().unwrap();
                    let config = base.device_clip(rect);
                    assert_eq!(
                        page.render_size(&config).unwrap(),
                        (rect.width, rect.height)
                    );
                    let actual = page.render(&config).unwrap();
                    assert_eq!(actual.pixels(), expected);
                    let mut job = page.render_job(&config).unwrap();
                    while job.resume(&mut || true).unwrap() != Progress::Done {}
                    let RenderOutput::Owned(progressive) = job.finish().unwrap() else {
                        panic!("owned")
                    };
                    assert_eq!(progressive.pixels(), expected);
                    let row_bytes = rect.width as usize * 4;
                    let stride = row_bytes + 12;
                    let length = stride * rect.height as usize;
                    let mut data = vec![0xa5; length + 7];
                    for row in 0..rect.height as usize {
                        data[row * stride..row * stride + row_bytes].fill(0);
                    }
                    let target =
                        PixelsMut::new(&mut data, rect.width, rect.height, stride, format).unwrap();
                    let mut job = page.render_job_into(target, &config).unwrap();
                    while job.resume(&mut || true).unwrap() != Progress::Done {}
                    let RenderOutput::Borrowed(actual) = job.finish().unwrap() else {
                        panic!("borrowed")
                    };
                    for row in 0..rect.height as usize {
                        assert_eq!(
                            &actual.data()[row * stride..row * stride + row_bytes],
                            &expected[row * row_bytes..(row + 1) * row_bytes]
                        );
                        assert_eq!(
                            &actual.data()[row * stride + row_bytes..(row + 1) * stride],
                            &[0xa5; 12]
                        );
                    }
                    drop(actual);
                    assert_eq!(&data[length..], &[0xa5; 7]);
                }
            }
        }
    }

    #[test]
    fn invalid_regions_reject_overflow_empty_and_outside_before_touching_target() {
        let doc = Document::from_bytes(fixture()).unwrap();
        let page = doc.page(0).unwrap();
        for rect in [
            DeviceRect::new(0, 0, 0, 1),
            DeviceRect::new(0, 0, 1, 0),
            DeviceRect::new(40, 0, 1, 1),
            DeviceRect::new(0, 30, 1, 1),
            DeviceRect::new(39, 29, 2, 1),
            DeviceRect::new(39, 29, 1, 2),
            DeviceRect::new(u32::MAX, 0, 2, 1),
            DeviceRect::new(0, u32::MAX, 1, 2),
            DeviceRect::new(0, 0, u32::MAX, 1),
        ] {
            let config = RenderConfig::default().device_clip(rect);
            assert!(matches!(
                page.render_size(&config),
                Err(Error::InvalidArgument(_))
            ));
            let mut target = Bitmap::new(1, 1, PixelFormat::Bgra8).unwrap();
            target.pixels_mut().fill(0x5a);
            assert!(matches!(
                page.render_into(&mut target.view_mut(), &config),
                Err(Error::InvalidArgument(_))
            ));
            assert_eq!(target.pixels(), &[0x5a; 4]);
            page.unload().unwrap();
        }
        let last_pixel = RenderConfig::default().device_clip(DeviceRect::new(39, 29, 1, 1));
        assert_eq!(page.render_size(&last_pixel).unwrap(), (1, 1));
        assert_eq!(page.render(&last_pixel).unwrap().pixels(), &[255; 4]);
        let mut wrong = Bitmap::new(40, 30, PixelFormat::Bgra8).unwrap();
        assert!(matches!(
            page.render_into(&mut wrong.view_mut(), &last_pixel),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn pause_panic_does_not_leave_borrows_or_a_permanent_lease() {
        let doc = Document::from_bytes(fixture()).unwrap();
        let page = doc.page(0).unwrap();
        let mut job = page.render_job(&RenderConfig::default()).unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = job.resume(&mut || panic!("pause callback"));
        }));
        assert!(panic.is_err());
        while job.resume(&mut || false).unwrap() != Progress::Done {}
        job.finish().unwrap();
        page.unload().unwrap();
    }
}
