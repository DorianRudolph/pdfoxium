// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Contiguous token output from `core/fxcrt/css/cfx_cssoutputtextbuf.{h,cpp}`.
#[derive(Debug)]
pub struct CfxCssOutputTextBuf {
    buffer: Vec<u32>,
}
impl Default for CfxCssOutputTextBuf {
    fn default() -> Self {
        Self {
            buffer: Vec::with_capacity(32),
        }
    }
}
impl CfxCssOutputTextBuf {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
    pub fn append_char_if_not_leading_blank(&mut self, character: u32) {
        if self.buffer.is_empty() && character <= u32::from(b' ') {
            return;
        }
        self.buffer.push(character);
    }
    pub fn get_trailing_blank_trimmed_string(&self) -> &[u32] {
        let length = self
            .buffer
            .iter()
            .rposition(|&ch| ch > u32::from(b' '))
            .map_or(0, |i| i + 1);
        &self.buffer[..length]
    }
}
