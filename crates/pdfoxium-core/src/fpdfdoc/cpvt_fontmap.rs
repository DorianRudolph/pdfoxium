// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpvt_fontmap.{h,cpp}` for portable annotation generation.
//! FreeText passes no resource dictionary. SetupAnnotSysPDFFont consequently
//! returns immediately; AddNativeInteractiveFormFont also returns null in the
//! non-Windows configuration. There is no system font or alias to retain.

use super::ipvt_fontmap::IpvtFontMap;
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fxcrt::fx_codepage::FxCharset;

pub struct CpvtFontMap {
    def_font: Option<CpdfFont>,
    def_font_alias: Vec<u8>,
}
impl CpvtFontMap {
    pub fn new(def_font: Option<CpdfFont>, def_font_alias: Vec<u8>) -> Self {
        Self {
            def_font,
            def_font_alias,
        }
    }
}
impl IpvtFontMap for CpvtFontMap {
    fn get_pdf_font(&self, font_index: i32) -> Option<CpdfFont> {
        match font_index {
            0 => self.def_font.clone(),
            _ => None,
        }
    }
    fn get_pdf_font_alias(&self, font_index: i32) -> Vec<u8> {
        match font_index {
            0 => self.def_font_alias.clone(),
            _ => Vec::new(),
        }
    }
    fn get_word_font_index(&self, _: u16, _: FxCharset, _: i32) -> i32 {
        unreachable!("CPVT_FontMap::GetWordFontIndex")
    }
    fn char_code_from_unicode(&self, _: i32, _: u16) -> i32 {
        unreachable!("CPVT_FontMap::CharCodeFromUnicode")
    }
    fn charset_from_unicode(&self, _: u16, _: FxCharset) -> FxCharset {
        unreachable!("CPVT_FontMap::CharSetFromUnicode")
    }
}
