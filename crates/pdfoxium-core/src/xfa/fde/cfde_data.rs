// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fde/cfde_data.h`.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum FdeTextAlignment {
    #[default]
    TopLeft = 0,
    CenterLeft,
    Center,
    CenterRight,
}
impl FdeTextAlignment {
    pub fn vertically_centered(self) -> bool {
        matches!(self, Self::CenterLeft | Self::Center | Self::CenterRight)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FdeTextStyle {
    pub single_line: bool,
    pub line_wrap: bool,
    pub last_line_height: bool,
}
impl FdeTextStyle {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
