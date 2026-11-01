// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Retain the native composer's allocations while releasing its device borrow.
use super::*;
pub(crate) struct DetachedBitmapComposer {
    clip: Option<CfxAggClipRgn>,
    dest_rect: FxRect,
    alpha: f32,
    mask_color: u32,
    vertical: bool,
    flip_x: bool,
    flip_y: bool,
    rgb_byte_order: bool,
    blend_mode: BlendMode,
    compositor: Option<CfxScanlineCompositor>,
    scanline_v: Vec<u8>,
    clip_scan_v: Vec<u8>,
    add_clip_scan: Vec<u8>,
}
impl CfxAggBitmapComposer<'_, '_> {
    pub(crate) fn detach(self) -> DetachedBitmapComposer {
        DetachedBitmapComposer {
            clip: self.clip.cloned(),
            dest_rect: self.dest_rect,
            alpha: self.alpha,
            mask_color: self.mask_color,
            vertical: self.vertical,
            flip_x: self.flip_x,
            flip_y: self.flip_y,
            rgb_byte_order: self.rgb_byte_order,
            blend_mode: self.blend_mode,
            compositor: self.compositor,
            scanline_v: self.scanline_v,
            clip_scan_v: self.clip_scan_v,
            add_clip_scan: self.add_clip_scan,
        }
    }
}
impl DetachedBitmapComposer {
    pub(crate) fn with_device<R>(
        &mut self,
        bitmap: &mut CfxDibitmapData<'_>,
        operation: impl FnOnce(&mut CfxAggBitmapComposer<'_, '_>) -> R,
    ) -> R {
        let clip = self.clip.clone();
        let mut composer = CfxAggBitmapComposer {
            bitmap,
            clip: clip.as_ref(),
            dest_rect: self.dest_rect,
            alpha: self.alpha,
            mask_color: self.mask_color,
            vertical: self.vertical,
            flip_x: self.flip_x,
            flip_y: self.flip_y,
            rgb_byte_order: self.rgb_byte_order,
            blend_mode: self.blend_mode,
            compositor: self.compositor.take(),
            scanline_v: std::mem::take(&mut self.scanline_v),
            clip_scan_v: std::mem::take(&mut self.clip_scan_v),
            add_clip_scan: std::mem::take(&mut self.add_clip_scan),
        };
        let result = operation(&mut composer);
        self.compositor = composer.compositor.take();
        self.scanline_v = std::mem::take(&mut composer.scanline_v);
        self.clip_scan_v = std::mem::take(&mut composer.clip_scan_v);
        self.add_clip_scan = std::mem::take(&mut composer.add_clip_scan);
        result
    }
}
