// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fgas/font/cfgas_defaultfontmanager.{h,cpp}`.
use super::{cfgas_gefont::CfgasGeFont, cfgas_gemodule, fgas_fontutils::font_info_by_font_name};
use crate::fxcrt::fx_codepage::FxCodePage;
use crate::fxge::fx_font::{
    font_style_is_force_bold, font_style_is_italic, FONT_STYLE_FORCE_BOLD, FONT_STYLE_ITALIC,
};
use std::rc::Rc;
pub struct CfgasDefaultFontManager;
impl CfgasDefaultFontManager {
    pub fn get_font(name: &[u32], styles: u32) -> Option<Rc<CfgasGeFont>> {
        let manager = cfgas_gemodule::font_manager();
        if let Some(font) = manager.load_font(Some(name), styles, FxCodePage::Failure) {
            return Some(font);
        }
        let replacements = font_info_by_font_name(name)?.replacement_font?;
        let mut fallback_style = 0;
        if font_style_is_force_bold(styles) {
            fallback_style |= FONT_STYLE_FORCE_BOLD;
        }
        if font_style_is_italic(styles) {
            fallback_style |= FONT_STYLE_ITALIC;
        }
        for replacement in replacements.split(',') {
            if replacement.is_empty() {
                continue;
            }
            let name: Vec<u32> = replacement.chars().map(u32::from).collect();
            if let Some(font) = manager.load_font(Some(&name), fallback_style, FxCodePage::Failure)
            {
                return Some(font);
            }
        }
        None
    }
    pub fn get_default_font(styles: u32) -> Option<Rc<CfgasGeFont>> {
        let manager = cfgas_gemodule::font_manager();
        let name: Vec<u32> = "Arial Narrow".chars().map(u32::from).collect();
        manager
            .load_font(Some(&name), styles, FxCodePage::Failure)
            .or_else(|| manager.load_font(None, styles, FxCodePage::Failure))
    }
}
