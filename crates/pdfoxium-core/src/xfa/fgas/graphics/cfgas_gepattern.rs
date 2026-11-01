// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fgas/graphics/cfgas_gepattern.{h,cpp}`.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum HatchStyle {
    Horizontal = 0,
    Vertical = 1,
    ForwardDiagonal = 2,
    BackwardDiagonal = 3,
    Cross = 4,
    DiagonalCross = 5,
}

#[derive(Clone, Debug)]
pub struct CfgasGePattern {
    hatch_style: HatchStyle,
    fore_argb: u32,
    back_argb: u32,
}

impl CfgasGePattern {
    pub const fn new(hatch_style: HatchStyle, fore_argb: u32, back_argb: u32) -> Self {
        Self {
            hatch_style,
            fore_argb,
            back_argb,
        }
    }
    pub fn get_hatch_style(&self) -> HatchStyle {
        self.hatch_style
    }
    pub fn get_fore_argb(&self) -> u32 {
        self.fore_argb
    }
    pub fn get_back_argb(&self) -> u32 {
        self.back_argb
    }
}
