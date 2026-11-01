// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! CPDF_ProgressiveRenderer cursor/state machine. The caller retains the real
//! layer owners and target bitmap; no object or rendered page is cloned.
use super::super::cpdf_renderstatus::{
    culling_rect, object_intersects_culling_rect, CullingRects, ProgressiveRenderStatus,
};
use super::*;
use crate::{
    fpdfapi::page::{cpdf_imageobject::CpdfImageObject, cpdf_pageobject::PageObjectType},
    fxcrt::pauseindicator_iface::PauseIndicatorIface,
};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum RenderStatus {
    Ready = 0,
    ToBeContinued = 1,
    Done = 2,
    Failed = 3,
}
/// A boundary at which the embedding may release the bitmap borrow before
/// invoking its pause callback. `MaskBreak` always yields to the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderStep {
    Continue,
    CheckPause,
    MaskBreak,
    Done,
    Failed,
}
pub struct CpdfProgressiveRenderer {
    status: RenderStatus,
    layer_index: usize,
    object_index: usize,
    objects_to_go: usize,
    current: Option<ProgressiveRenderStatus>,
    clip_rect: CullingRects,
}
impl Default for CpdfProgressiveRenderer {
    fn default() -> Self {
        Self::new()
    }
}
impl CpdfProgressiveRenderer {
    const STEP_LIMIT: usize = 100;
    pub fn new() -> Self {
        Self {
            status: RenderStatus::Ready,
            layer_index: 0,
            object_index: 0,
            objects_to_go: Self::STEP_LIMIT,
            current: None,
            clip_rect: CullingRects::default(),
        }
    }
    pub fn status(&self) -> RenderStatus {
        self.status
    }
    pub fn layer_index(&self) -> usize {
        self.layer_index
    }
    pub fn object_index(&self) -> usize {
        self.object_index
    }
    pub fn start(&mut self) -> RenderStatus {
        self.status = if self.status == RenderStatus::Ready {
            RenderStatus::ToBeContinued
        } else {
            RenderStatus::Failed
        };
        self.status
    }
    /// Render at most one real page object, retaining the exact cursor and clip
    /// cache. Inactive/offscreen objects do not consume the native step budget.
    pub fn render_step(
        &mut self,
        context: &CpdfRenderContext<'_>,
        driver: &mut CfxAggDeviceDriver<'_, '_>,
        options: &CpdfRenderOptions,
    ) -> RenderStep {
        if self.status == RenderStatus::Done {
            return RenderStep::Done;
        }
        if self.status != RenderStatus::ToBeContinued {
            self.status = RenderStatus::Failed;
            return RenderStep::Failed;
        }
        let Some(layer) = context.layers().get(self.layer_index) else {
            self.status = RenderStatus::Done;
            return RenderStep::Done;
        };
        if self.current.is_none() {
            self.current = Some(ProgressiveRenderStatus::default());
            self.clip_rect = culling_rect(layer.matrix, driver.clip_box());
            self.objects_to_go = Self::STEP_LIMIT;
        }
        let Some(object) = layer.object_holder.page_object_by_index(self.object_index) else {
            self.current = None;
            self.layer_index += 1;
            self.object_index = 0;
            return RenderStep::CheckPause;
        };
        let visible = object.base().is_active()
            && object_intersects_culling_rect(
                object,
                layer.matrix,
                driver.clip_box(),
                self.clip_rect,
                options,
            );
        let mut mask = false;
        if visible {
            mask = options.options().break_for_masks
                && object
                    .as_any()
                    .downcast_ref::<CpdfImageObject>()
                    .and_then(CpdfImageObject::image)
                    .is_some_and(|image| image.is_mask());
            if !self.current.as_mut().unwrap().render_object(
                context,
                driver,
                options,
                self.layer_index,
                object,
            ) {
                self.status = RenderStatus::Failed;
                return RenderStep::Failed;
            }
            if self.current.as_ref().unwrap().is_pending() {
                return RenderStep::CheckPause;
            }
            self.objects_to_go = if matches!(
                object.object_type(),
                PageObjectType::Form | PageObjectType::Shading
            ) {
                0
            } else {
                self.objects_to_go - 1
            };
        }
        self.object_index += 1;
        if mask {
            return RenderStep::MaskBreak;
        }
        if self.objects_to_go == 0 {
            self.objects_to_go = Self::STEP_LIMIT;
            RenderStep::CheckPause
        } else {
            RenderStep::Continue
        }
    }
    pub fn continue_render(
        &mut self,
        context: &CpdfRenderContext<'_>,
        driver: &mut CfxAggDeviceDriver<'_, '_>,
        options: &CpdfRenderOptions,
        mut pause: Option<&mut dyn PauseIndicatorIface>,
    ) -> RenderStatus {
        loop {
            match self.render_step(context, driver, options) {
                RenderStep::Continue => {}
                RenderStep::CheckPause => {
                    if pause
                        .as_deref_mut()
                        .is_some_and(PauseIndicatorIface::need_to_pause_now)
                    {
                        break;
                    }
                }
                RenderStep::MaskBreak | RenderStep::Done | RenderStep::Failed => break,
            }
        }
        self.status
    }
}
