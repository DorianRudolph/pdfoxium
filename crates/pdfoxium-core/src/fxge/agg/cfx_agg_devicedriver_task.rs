// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
impl CfxAggDeviceDriver<'_, '_> {
    pub fn continue_dibits_task<S: crate::fxge::dib::cstretchengine::StretchSource>(
        &mut self,
        task: &mut super::super::cfx_agg_imagerenderer::CfxAggImageRenderTask<S>,
        pause: Option<&mut dyn crate::fxcrt::pauseindicator_iface::PauseIndicatorIface>,
    ) -> bool {
        task.continue_render(self.bitmap, pause)
    }
}
