// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpvt_wordrange.h`.

use super::cpvt_wordplace::CpvtWordPlace;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CpvtWordRange {
    pub begin: CpvtWordPlace,
    pub end: CpvtWordPlace,
}
impl CpvtWordRange {
    pub fn new(begin: CpvtWordPlace, end: CpvtWordPlace) -> Self {
        let mut range = Self { begin, end };
        range.normalize();
        range
    }
    pub fn is_empty(self) -> bool {
        self.begin == self.end
    }
    pub fn normalize(&mut self) {
        if self.begin > self.end {
            std::mem::swap(&mut self.begin, &mut self.end);
        }
    }
}
