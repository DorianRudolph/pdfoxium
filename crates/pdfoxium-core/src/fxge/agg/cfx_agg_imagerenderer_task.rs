// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Owned image continuation: the original transformer/stretch engine and
//! composer allocations survive; the destination is borrowed only per step.
use super::super::cfx_agg_bitmapcomposer::DetachedBitmapComposer;
use super::*;
pub struct CfxAggImageRenderTask<S: StretchSource> {
    transformer: Option<CfxImageTransformer<S>>,
    stretcher: Option<CfxImageStretcher<S>>,
    composer: DetachedBitmapComposer,
    alpha: f32,
    mask_color: u32,
    state: State,
}
impl<S: StretchSource> CfxAggImageRenderer<'_, '_, S> {
    pub fn into_task(self) -> CfxAggImageRenderTask<S> {
        CfxAggImageRenderTask {
            transformer: self.transformer,
            stretcher: self.stretcher,
            composer: self.composer.detach(),
            alpha: self.alpha,
            mask_color: self.mask_color,
            state: self.state,
        }
    }
}
impl<S: StretchSource> CfxAggImageRenderTask<S> {
    pub fn continue_render(
        &mut self,
        bitmap: &mut CfxDibitmapData<'_>,
        pause: Option<&mut dyn PauseIndicatorIface>,
    ) -> bool {
        let transformer = &mut self.transformer;
        let stretcher = &mut self.stretcher;
        let state = self.state;
        let alpha = self.alpha;
        let color = &mut self.mask_color;
        self.composer.with_device(bitmap, |composer| {
            if state == State::Stretching {
                return stretcher
                    .as_mut()
                    .expect("retained image stretcher")
                    .continue_stretch(composer, pause);
            }
            if state != State::Transforming {
                return false;
            }
            let transformer = transformer.as_mut().expect("retained image transformer");
            if transformer.continue_transform(pause) {
                return true;
            }
            let Some(bitmap) = transformer.detach_bitmap() else {
                return false;
            };
            let mut bitmap = bitmap.borrow_mut();
            if bitmap.buffer().is_empty() {
                return false;
            }
            if get_is_mask_from_format(bitmap.format()) && alpha != 1.0 {
                *color = argb_mul_alpha(*color, (alpha * 255.0).round() as u32);
            } else if !get_is_mask_from_format(bitmap.format()) {
                bitmap.multiply_alpha(alpha);
            }
            let result = transformer.result();
            composer.composite_transformed(&bitmap, *color, result.left, result.top);
            false
        })
    }
}
#[cfg(test)]
#[path = "cfx_agg_imagerenderer_task_tests.rs"]
mod tests;
