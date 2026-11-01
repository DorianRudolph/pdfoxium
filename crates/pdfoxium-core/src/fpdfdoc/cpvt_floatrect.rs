// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpvt_floatrect.h`: top-down layout coordinates.

use crate::fxcrt::fx_coordinates::CfxFloatRect;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CpvtFloatRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}
impl CpvtFloatRect {
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
    pub fn from_rect(rect: CfxFloatRect) -> Self {
        Self::new(rect.left, rect.top, rect.right, rect.bottom)
    }
    pub fn width(self) -> f32 {
        self.right - self.left
    }
    pub fn height(self) -> f32 {
        if self.top > self.bottom {
            self.top - self.bottom
        } else {
            self.bottom - self.top
        }
    }
}
