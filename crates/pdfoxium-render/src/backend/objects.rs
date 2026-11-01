// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors. BSD-3-Clause; see upstream/pdfium/LICENSE.
use crate::backend::prelude::*;
use pdfoxium_core::fpdfapi::{
    edit::cpdf_pagecontentgenerator::CpdfPageContentGenerator, page::cpdf_page::CpdfPage,
};
use std::rc::Rc;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PdfFontBuiltin {
    TimesRoman,
    TimesBold,
    TimesItalic,
    TimesBoldItalic,
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
    Courier,
    CourierBold,
    CourierOblique,
    CourierBoldOblique,
    Symbol,
    ZapfDingbats,
}
impl PdfFontBuiltin {
    pub fn to_pdf_font_name(&self) -> &str {
        match self {
            PdfFontBuiltin::TimesRoman => "Times-Roman",
            PdfFontBuiltin::TimesBold => "Times-Bold",
            PdfFontBuiltin::TimesItalic => "Times-Italic",
            PdfFontBuiltin::TimesBoldItalic => "Times-BoldItalic",
            PdfFontBuiltin::Helvetica => "Helvetica",
            PdfFontBuiltin::HelveticaBold => "Helvetica-Bold",
            PdfFontBuiltin::HelveticaOblique => "Helvetica-Oblique",
            PdfFontBuiltin::HelveticaBoldOblique => "Helvetica-BoldOblique",
            PdfFontBuiltin::Courier => "Courier",
            PdfFontBuiltin::CourierBold => "Courier-Bold",
            PdfFontBuiltin::CourierOblique => "Courier-Oblique",
            PdfFontBuiltin::CourierBoldOblique => "Courier-BoldOblique",
            PdfFontBuiltin::Symbol => "Symbol",
            PdfFontBuiltin::ZapfDingbats => "ZapfDingbats",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PdfFontToken {
    pub(crate) owner: u64,
    pub(crate) index: usize,
}
impl PdfPage {
    pub(crate) fn alias(&self) -> Self {
        Self {
            state: self.state.clone(),
            document: self.document.clone(),
        }
    }
    pub(crate) fn edit<T>(
        &self,
        f: impl FnOnce(&mut CpdfPage) -> Result<T, PdfiumError>,
    ) -> Result<T, PdfiumError> {
        self.check_live()?;
        self.state.text_cache.borrow_mut().take();
        let mut core = self
            .state
            .core
            .try_borrow_mut()
            .map_err(|_| PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        let page =
            Rc::get_mut(&mut core).ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        let result = f(page);
        if self.state.strategy.get()
            != crate::document::PdfPageContentRegenerationStrategy::AutomaticOnEveryChange
        {
            return result;
        }
        // Preserve full graphics state on newly inserted objects.
        let holder = page.mutable_holder();
        if holder.iter().any(|o| o.base().content_stream() == -1) {
            holder.prepare_full_rewrite();
        }
        CpdfPageContentGenerator::new(holder, Some(&mut self.document.core.borrow_mut()))
            .generate_content();
        result
    }
    pub fn regenerate_content(&mut self) -> Result<(), PdfiumError> {
        self.check_live()?;
        let mut core = self
            .state
            .core
            .try_borrow_mut()
            .map_err(|_| PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        let page =
            Rc::get_mut(&mut core).ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        let holder = page.mutable_holder();
        if holder.iter().any(|o| o.base().content_stream() == -1) {
            holder.prepare_full_rewrite();
        }
        CpdfPageContentGenerator::new(holder, Some(&mut self.document.core.borrow_mut()))
            .generate_content();
        Ok(())
    }
}
