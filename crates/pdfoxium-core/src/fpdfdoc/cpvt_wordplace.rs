// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpvt_wordplace.h`.

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CpvtWordPlace {
    pub sec_index: i32,
    pub line_index: i32,
    pub word_index: i32,
}

impl Default for CpvtWordPlace {
    fn default() -> Self {
        Self::new(-1, -1, -1)
    }
}

impl CpvtWordPlace {
    pub const fn new(sec_index: i32, line_index: i32, word_index: i32) -> Self {
        Self {
            sec_index,
            line_index,
            word_index,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn advance_section(&mut self) {
        self.sec_index += 1;
        self.line_index = 0;
        self.word_index = -1;
    }
    pub fn line_cmp(self, other: Self) -> i32 {
        if self.sec_index != other.sec_index {
            self.sec_index - other.sec_index
        } else {
            self.line_index - other.line_index
        }
    }
}
