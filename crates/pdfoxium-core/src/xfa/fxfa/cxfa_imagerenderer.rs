// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_imagerenderer.{h,cpp}` using the existing safe AGG continuation.
//! Constructor/Start are combined so the continuation can borrow the device
//! directly instead of requiring a self-referential device/handle pair.
use super::fxfa_basic::XfaAttributeValue;
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxRectF},
    fxge::{
        agg::{
            cfx_agg_devicedriver::CfxAggDeviceDriver, cfx_agg_imagerenderer::CfxAggImageRenderer,
        },
        cfx_fillrenderoptions::CfxFillRenderOptions,
        cfx_path::CfxPath,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibResampleOptions},
    },
    xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
use std::rc::Rc;
pub struct CxfaImageRenderer<'device, 'storage> {
    continuation: CfxAggImageRenderer<'device, 'storage, CfxDibitmap>,
}
impl<'device, 'storage> CxfaImageRenderer<'device, 'storage> {
    pub fn start(
        device: &'device mut CfxAggDeviceDriver<'_, 'storage>,
        bitmap: Rc<CfxDibitmap>,
        image_to_device: CfxMatrix,
    ) -> Self {
        let options = FxdibResampleOptions {
            interpolate_bilinear: true,
            ..Default::default()
        };
        Self {
            continuation: device.start_dibits(bitmap, 1.0, 0, image_to_device, options),
        }
    }
    pub fn continue_render(&mut self) -> bool {
        self.continuation.continue_render(None)
    }
}
/// Physical-pixel aspect fitting from XFA_DrawImage. Invalid DPI is rejected
/// instead of dividing by zero; decoded resources have already defaulted to96.
pub fn image_fit_rect(
    bounds: CfxRectF,
    width: i32,
    height: i32,
    dpi: (i32, i32),
    aspect: XfaAttributeValue,
    horizontal: XfaAttributeValue,
    vertical: XfaAttributeValue,
) -> Option<CfxRectF> {
    if bounds.is_empty() || width <= 0 || height <= 0 || dpi.0 <= 0 || dpi.1 <= 0 {
        return None;
    }
    let mut fit = CfxRectF::new(
        bounds.left,
        bounds.top,
        width as f32 * 72.0 / dpi.0 as f32,
        height as f32 * 72.0 / dpi.1 as f32,
    );
    match aspect {
        XfaAttributeValue::Fit => {
            let scale = (bounds.height / fit.height).min(bounds.width / fit.width);
            fit.width *= scale;
            fit.height *= scale;
        }
        XfaAttributeValue::Height => {
            fit.width *= bounds.height / fit.height;
            fit.height = bounds.height;
        }
        XfaAttributeValue::None => {
            fit.width = bounds.width;
            fit.height = bounds.height;
        }
        XfaAttributeValue::Width => {
            fit.height *= bounds.width / fit.width;
            fit.width = bounds.width;
        }
        _ => {}
    }
    match horizontal {
        XfaAttributeValue::Center => fit.left += (bounds.width - fit.width) / 2.0,
        XfaAttributeValue::Right => fit.left = bounds.right() - fit.width,
        _ => {}
    }
    match vertical {
        XfaAttributeValue::Middle => fit.top += (bounds.height - fit.height) / 2.0,
        XfaAttributeValue::Bottom => fit.top = bounds.bottom() - fit.height,
        _ => {}
    }
    // The source bottom-alignment line subtracts the bounds height, ignoring
    // the fitted image height. Use the actual image height as intended.
    Some(fit)
}
/// XFA_DrawImage: preserve device clip/state and render through AGG bilinear
/// interpolation with physical image DPI, aspect mode and paragraph alignment.
pub fn draw_image(
    graphics: &mut CfgasGeGraphics<'_, '_, '_>,
    bounds: CfxRectF,
    matrix: CfxMatrix,
    bitmap: Rc<CfxDibitmap>,
    aspect: XfaAttributeValue,
    dpi: (i32, i32),
    horizontal: XfaAttributeValue,
    vertical: XfaAttributeValue,
) {
    if bitmap.buffer().is_empty() {
        return;
    }
    let Some(fit) = image_fit_rect(
        bounds,
        bitmap.width(),
        bitmap.height(),
        dpi,
        aspect,
        horizontal,
        vertical,
    ) else {
        return;
    };
    let device = graphics.get_render_device();
    device.save_state();
    let mut clip = CfxPath::new();
    clip.append_rect(bounds.left, bounds.bottom(), bounds.right(), bounds.top);
    if device.set_clip_path_fill(&clip, Some(matrix), CfxFillRenderOptions::winding_options()) {
        let mut image_to_device = CfxMatrix::new(1.0, 0.0, 0.0, -1.0, 0.0, 1.0);
        image_to_device.concat(CfxMatrix::new(
            fit.width, 0.0, 0.0, fit.height, fit.left, fit.top,
        ));
        image_to_device.concat(matrix);
        let mut renderer = CxfaImageRenderer::start(device, bitmap, image_to_device);
        while renderer.continue_render() {}
    }
    device.restore_state(false);
}
