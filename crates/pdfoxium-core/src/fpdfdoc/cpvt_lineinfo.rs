// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpvt_lineinfo.h`.

#[derive(Clone, Copy, Debug)]
pub struct CpvtLineInfo {
    pub begin_word_index: i32,
    pub end_word_index: i32,
    pub line_x: f32,
    pub line_y: f32,
    pub line_width: f32,
    pub line_ascent: f32,
    pub line_descent: f32,
}
impl Default for CpvtLineInfo {
    fn default() -> Self {
        Self {
            begin_word_index: -1,
            end_word_index: -1,
            line_x: 0.0,
            line_y: 0.0,
            line_width: 0.0,
            line_ascent: 0.0,
            line_descent: 0.0,
        }
    }
}
