// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Borrowed UTF-32 input from `core/fxcrt/css/cfx_cssinputtextbuf.{h,cpp}`.
#[derive(Debug)]
pub struct CfxCssInputTextBuf<'a> {
    buffer: &'a [u32],
    pos: usize,
}
impl<'a> CfxCssInputTextBuf<'a> {
    pub fn new(buffer: &'a [u32]) -> Self {
        Self { buffer, pos: 0 }
    }
    pub fn is_eof(&self) -> bool {
        self.pos >= self.buffer.len()
    }
    pub fn move_next(&mut self) {
        if self.pos < self.buffer.len() {
            self.pos += 1;
        }
    }
    pub fn get_char(&self) -> u32 {
        self.buffer.get(self.pos).copied().unwrap_or(0)
    }
    pub fn get_next_char(&self) -> u32 {
        self.buffer
            .get(self.pos.saturating_add(1))
            .copied()
            .unwrap_or(0)
    }
}
