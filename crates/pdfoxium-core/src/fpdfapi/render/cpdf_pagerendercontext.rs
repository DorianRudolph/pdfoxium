// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! CPDF_PageRenderContext lifetime owner. The retained original page, parsed
//! annotation appearances, options and rendering cursor survive continuation;
//! borrowed AGG device/layer views are created only during an actual step.
use super::{
    cpdf_progressiverenderer::{CpdfProgressiveRenderer, RenderStatus, RenderStep},
    cpdf_rendercontext::CpdfRenderContext,
    cpdf_renderoptions::CpdfRenderOptions,
};
use crate::{
    fpdfapi::page::cpdf_page::CpdfPage,
    fpdfdoc::{cpdf_annot::CpdfAnnot, cpdf_annotlist::CpdfAnnotList},
    fxcrt::fx_coordinates::{CfxMatrix, FxRect},
    fxge::{agg::cfx_agg_devicedriver::CfxAggDeviceDriver, dib::cfx_dibitmap::CfxDibitmapData},
};
use std::rc::Rc;
pub struct CpdfPageRenderContext {
    renderer: CpdfProgressiveRenderer,
    annotations: Vec<CpdfAnnot>,
    options: CpdfRenderOptions,
    page: Rc<CpdfPage>,
    matrix: CfxMatrix,
    clip: FxRect,
    reverse_byte_order: bool,
}
impl CpdfPageRenderContext {
    pub fn new(
        page: Rc<CpdfPage>,
        options: CpdfRenderOptions,
        matrix: CfxMatrix,
        clip: FxRect,
        reverse_byte_order: bool,
    ) -> Self {
        let annotations = if options.draw_annots() {
            CpdfAnnotList::new(&page).into_annotations()
        } else {
            Vec::new()
        };
        Self {
            renderer: CpdfProgressiveRenderer::new(),
            annotations,
            options,
            page,
            matrix,
            clip,
            reverse_byte_order,
        }
    }
    pub fn status(&self) -> RenderStatus {
        self.renderer.status()
    }
    pub fn start(&mut self) -> RenderStatus {
        self.renderer.start()
    }
    pub fn renderer(&self) -> &CpdfProgressiveRenderer {
        &self.renderer
    }
    pub fn options(&self) -> &CpdfRenderOptions {
        &self.options
    }
    /// No embedding callback is made while `bitmap` is borrowed. On CheckPause
    /// the embedding can release its device/bitmap borrow and invoke the host.
    pub fn render_step(&mut self, bitmap: &mut CfxDibitmapData<'_>) -> RenderStep {
        let mut driver = CfxAggDeviceDriver::new(bitmap, self.reverse_byte_order, None);
        if !driver.set_clip_rect(self.clip) {
            return RenderStep::Failed;
        }
        let mut context = CpdfRenderContext::new(&self.page);
        context.append_layer(self.page.holder(), self.matrix);
        CpdfAnnotList::display_owned_annots_with_options(
            &self.page,
            &mut self.annotations,
            &mut context,
            self.options.printing(),
            self.matrix,
            Some(&self.options),
        );
        self.renderer
            .render_step(&context, &mut driver, &self.options)
    }
}
