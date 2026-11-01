// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Shared CPDFSDK_RenderPage configuration for the actual AGG progressive job.
use crate::{
    fpdfapi::{
        page::{
            cpdf_occontext::{CpdfOcContext, UsageType},
            cpdf_page::CpdfPage,
        },
        parser::cpdf_document::CpdfDocument,
        render::{
            cpdf_pagerendercontext::CpdfPageRenderContext,
            cpdf_renderoptions::{CpdfRenderOptions, RenderColorMode, RenderColorScheme},
        },
    },
    fxcrt::fx_coordinates::FxRect,
};
use std::rc::Rc;
pub fn render_page_with_context(
    page: Rc<CpdfPage>,
    document: &CpdfDocument,
    rect: FxRect,
    rotate: i32,
    flags: i32,
    color_scheme: Option<RenderColorScheme>,
) -> CpdfPageRenderContext {
    let matrix = page.display_matrix_for_rect(rect, rotate);
    let mut options = CpdfRenderOptions::new();
    let mode = if flags & 0x800 != 0 {
        UsageType::Print
    } else {
        UsageType::View
    };
    options.set_oc_context(Some(Rc::new(CpdfOcContext::new(document, mode))));
    options.set_draw_annots(flags & 0x01 != 0);
    options.set_printing(flags & 0x800 != 0);
    options.options_mut().clear_type = flags & 0x02 != 0;
    options.options_mut().no_native_text = flags & 0x04 != 0;
    options.options_mut().limited_image_cache = flags & 0x200 != 0;
    options.options_mut().force_halftone = flags & 0x400 != 0;
    options.options_mut().no_text_smooth = flags & 0x1000 != 0;
    options.options_mut().no_image_smooth = flags & 0x2000 != 0;
    options.options_mut().no_path_smooth = flags & 0x4000 != 0;
    if flags & 0x08 != 0 {
        options.set_color_mode(RenderColorMode::Gray);
    }
    if let Some(scheme) = color_scheme {
        options.set_color_mode(RenderColorMode::ForcedColor);
        options.set_color_scheme(scheme);
        options.options_mut().convert_fill_to_stroke = flags & 0x20 != 0;
    }
    CpdfPageRenderContext::new(page, options, matrix, rect, flags & 0x10 != 0)
}
