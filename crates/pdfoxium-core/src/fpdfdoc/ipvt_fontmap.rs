// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/ipvt_fontmap.h`.

use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fxcrt::fx_codepage::FxCharset;

pub trait IpvtFontMap {
    fn get_pdf_font(&self, font_index: i32) -> Option<CpdfFont>;
    fn get_pdf_font_alias(&self, font_index: i32) -> Vec<u8>;
    fn get_word_font_index(&self, word: u16, charset: FxCharset, font_index: i32) -> i32;
    fn char_code_from_unicode(&self, font_index: i32, word: u16) -> i32;
    fn charset_from_unicode(&self, word: u16, old_charset: FxCharset) -> FxCharset;
}
