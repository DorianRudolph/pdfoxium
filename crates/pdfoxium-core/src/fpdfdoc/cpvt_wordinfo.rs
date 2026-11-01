// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpvt_wordinfo.{h,cpp}`.

use crate::fxcrt::fx_codepage::FxCharset;

#[derive(Clone, Copy, Debug)]
pub struct CpvtWordInfo {
    pub word: u16,
    pub charset: FxCharset,
    pub word_x: f32,
    pub word_y: f32,
    pub word_tail: f32,
    pub font_index: i32,
    pub is_rtl: bool,
}
impl CpvtWordInfo {
    pub fn new(word: u16, charset: FxCharset, font_index: i32) -> Self {
        Self {
            word,
            charset,
            word_x: 0.0,
            word_y: 0.0,
            word_tail: 0.0,
            font_index,
            is_rtl: false,
        }
    }
}
