// Copyright 2014 The PDFium Authors
// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Shared safe loading and render configuration for the Rust and C APIs.
//!
//! These implementation interfaces are exposed only for the C ABI adapter;
//! applications use `Document`, `Page` and `RenderConfig` instead.

use pdfoxium_core::{
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
use std::{cell::RefCell, rc::Rc};

/// Load and parse a PDF page while releasing the document borrow before
/// resolving fonts, forms and other resources during content parsing.
pub fn load_pdf_page(document: &Rc<RefCell<CpdfDocument>>, index: usize) -> Option<Rc<CpdfPage>> {
    let mut page = {
        let mut document = document.try_borrow_mut().ok()?;
        if index >= document.page_count() {
            return None;
        }
        CpdfPage::from_document(&mut document, index)?
    };
    page.parse_path_content();
    Some(Rc::new(page))
}

/// Retained page variant produced by the native XFA extension path.
#[cfg(all(feature = "xfa", feature = "capi-support"))]
pub struct LoadedXfaPage {
    pub pdf: Option<Rc<CpdfPage>>,
    pub xfa: Rc<pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_page::CpdfXfaPage>,
    pub sdk: Rc<pdfoxium_core::fpdfsdk::cpdfsdk_pageview::SdkPage>,
}

/// Preserve `FPDF_LoadPage`'s PDF-backed and full-XFA page variants. SDK bridge
/// invalidation and C handle caches remain the responsibility of the adapter.
#[cfg(all(feature = "xfa", feature = "capi-support"))]
pub fn load_xfa_page(
    document: &Rc<RefCell<CpdfDocument>>,
    context: &Rc<pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_context::CpdfXfaContext>,
    index: i32,
) -> Option<LoadedXfaPage> {
    let xfa = context.get_or_create_xfa_page(index)?;
    let pdf = if context.contains_extension_full_form() {
        None
    } else {
        if !xfa.load_pdf_page() {
            return None;
        }
        xfa.take_pdf_page()
    };
    let sdk = if let Some(page) = &pdf {
        pdfoxium_core::fpdfsdk::cpdfsdk_pageview::SdkPage::new(document, page)
    } else {
        context.sdk_page(&xfa)?
    };
    Some(LoadedXfaPage { pdf, xfa, sdk })
}

/// Translate the selected portable PDFium render flags. Keep forced color
/// after grayscale, matching `CPDFSDK_RenderPage`'s operation ordering.
pub fn render_options(
    document: &CpdfDocument,
    flags: i32,
    color_scheme: Option<RenderColorScheme>,
) -> CpdfRenderOptions {
    let mut options = CpdfRenderOptions::new();
    let usage = if flags & 0x800 != 0 {
        UsageType::Print
    } else {
        UsageType::View
    };
    options.set_oc_context(Some(Rc::new(CpdfOcContext::new(document, usage))));
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
    options
}

/// Construct the retained progressive context used by both public APIs.
#[cfg(feature = "capi-support")]
pub fn render_page_with_context(
    page: Rc<CpdfPage>,
    document: &CpdfDocument,
    rect: FxRect,
    rotate: i32,
    flags: i32,
    color_scheme: Option<RenderColorScheme>,
) -> CpdfPageRenderContext {
    render_page_with_snapshot(page, document, rect, rotate, flags, color_scheme, None)
}
pub(crate) fn render_page_with_snapshot(
    page: Rc<CpdfPage>,
    document: &CpdfDocument,
    rect: FxRect,
    rotate: i32,
    flags: i32,
    color_scheme: Option<RenderColorScheme>,
    snapshot: Option<pdfoxium_core::fpdfapi::page::optional_content::OcSnapshot>,
) -> CpdfPageRenderContext {
    let mut options = render_options(document, flags, color_scheme);
    if let Some(snapshot) = snapshot {
        options.set_oc_context(Some(Rc::new(CpdfOcContext::with_snapshot(
            document,
            if flags & 0x800 != 0 {
                UsageType::Print
            } else {
                UsageType::View
            },
            snapshot,
        ))));
    }
    let matrix = page.display_matrix_for_rect(rect, rotate);
    CpdfPageRenderContext::new(page, options, matrix, rect, flags & 0x10 != 0)
}
