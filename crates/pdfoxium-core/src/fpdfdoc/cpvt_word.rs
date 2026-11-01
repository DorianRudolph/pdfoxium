// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpvt_word.{h,cpp}`.

use crate::fxcrt::fx_codepage::FxCharset;
use crate::fxcrt::fx_coordinates::CfxPointF;

#[derive(Clone, Copy, Debug)]
pub struct CpvtWord {
    pub word: u16,
    pub charset: FxCharset,
    pub location: CfxPointF,
    pub ascent: f32,
    pub descent: f32,
    pub width: f32,
    pub font_index: i32,
    pub font_size: f32,
    pub is_rtl: bool,
}
impl Default for CpvtWord {
    fn default() -> Self {
        Self {
            word: 0,
            charset: FxCharset::Ansi,
            location: CfxPointF::default(),
            ascent: 0.0,
            descent: 0.0,
            width: 0.0,
            font_index: -1,
            font_size: 0.0,
            is_rtl: false,
        }
    }
}
impl CpvtWord {
    pub fn caret_x(self) -> f32 {
        if self.is_rtl {
            self.location.x
        } else {
            self.location.x + self.width
        }
    }
    pub fn ascent_y(self) -> f32 {
        self.location.y + self.ascent
    }
    pub fn descent_y(self) -> f32 {
        self.location.y + self.descent
    }
}
