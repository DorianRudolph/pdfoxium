// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Static and retained progressive paths through `cpdf_progressiverenderer`.
//! The owned renderer keeps the original object/layer cursor and clipping
//! masks. Public jobs release device borrows before calling an embedding pause.
//! The bitmap entry points use the already parsed page holders.

use super::cpdf_rendercontext::CpdfRenderContext;
use super::cpdf_renderoptions::CpdfRenderOptions;
use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;

pub(crate) fn render_static(
    context: &CpdfRenderContext<'_>,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    options: &CpdfRenderOptions,
) -> bool {
    super::cpdf_renderstatus::render_layers(context, driver, None, Some(options), None, true)
}

#[path = "cpdf_progressiverenderer_resume.rs"]
mod resume;
pub use resume::*;
