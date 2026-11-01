// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Source CFGAS_BreakLine. Break pieces index this line's character buffer.
use super::{cfgas_breakpiece::CfgasBreakPiece, cfgas_char::CfgasChar};
#[derive(Clone, Default)]
pub struct CfgasBreakLine {
    pub line_chars: Vec<CfgasChar>,
    pub line_pieces: Vec<CfgasBreakPiece>,
    pub start: i32,
    pub width: i32,
    pub arabic_chars: usize,
}
impl CfgasBreakLine {
    pub fn last_char(&self) -> Option<&CfgasChar> {
        self.line_chars.last()
    }
    pub fn last_char_mut(&mut self) -> Option<&mut CfgasChar> {
        self.line_chars.last_mut()
    }
    pub fn get_line_end(&self) -> i32 {
        self.start.saturating_add(self.width)
    }
    pub fn clear(&mut self) {
        self.line_chars.clear();
        self.line_pieces.clear();
        self.width = 0;
        self.arabic_chars = 0;
    }
    pub fn increment_arabic_char_count(&mut self) {
        self.arabic_chars += 1;
    }
    pub fn decrement_arabic_char_count(&mut self) {
        self.arabic_chars = self.arabic_chars.saturating_sub(1);
    }
    pub fn has_arabic_char(&self) -> bool {
        self.arabic_chars > 0
    }
}
