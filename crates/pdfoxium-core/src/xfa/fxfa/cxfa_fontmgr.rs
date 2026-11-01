// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! `cxfa_fontmgr.{h,cpp}`. Preserve the actual XFA lookup sequence: embedded
//! strict, requested system family, embedded loose, default system font, then
//! the requested stock family. No unrelated stock substitute is invented.
use super::cxfa_textparser::XfaTextFontHost;
use crate::xfa::fgas::font::{
    cfgas_defaultfontmanager::CfgasDefaultFontManager, cfgas_gefont::CfgasGeFont,
    cfgas_pdffontmgr::CfgasPdfFontMgr, fgas_fontutils::font_name_to_english_name,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
#[derive(Default)]
pub struct CxfaFontMgr {
    fonts: RefCell<BTreeMap<(Vec<u32>, u32), Rc<CfgasGeFont>>>,
}
impl CxfaFontMgr {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get_font(
        &self,
        pdf_fonts: &CfgasPdfFontMgr,
        family: &[u32],
        styles: u32,
    ) -> Option<Rc<CfgasGeFont>> {
        let key = (family.to_vec(), styles);
        if let Some(font) = self.fonts.borrow().get(&key) {
            return Some(font.clone());
        }
        let english = font_name_to_english_name(family);
        if let Some(font) = pdf_fonts.get_font(&english, styles, true) {
            return Some(font);
        }
        let mut font = CfgasDefaultFontManager::get_font(family, styles);
        if font.is_none() {
            if let Some(font) = pdf_fonts.get_font(&english, styles, false) {
                return Some(font);
            }
        }
        if font.is_none() {
            font = CfgasDefaultFontManager::get_default_font(styles);
        }
        if font.is_none() {
            let bytes: String = family
                .iter()
                .filter_map(|&code| char::from_u32(code))
                .collect();
            font = pdf_fonts.load_stock_font(bytes.as_bytes());
        }
        let font = font?;
        self.fonts.borrow_mut().insert(key, font.clone());
        Some(font)
    }
}
/// Retained document services for text layout; no borrow of an FFDoc or PDF
/// document spans system font callbacks, glyph loading or rich-text parsing.
pub struct CxfaTextFontHostAdapter {
    font_mgr: Rc<CxfaFontMgr>,
    pdf_fonts: Rc<CfgasPdfFontMgr>,
}
impl CxfaTextFontHostAdapter {
    pub fn new(font_mgr: Rc<CxfaFontMgr>, pdf_fonts: Rc<CfgasPdfFontMgr>) -> Self {
        Self {
            font_mgr,
            pdf_fonts,
        }
    }
}
impl XfaTextFontHost for CxfaTextFontHostAdapter {
    fn get_font(&self, family: &[u32], styles: u32) -> Option<Rc<CfgasGeFont>> {
        self.font_mgr.get_font(&self.pdf_fonts, family, styles)
    }
}
