// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fwl/theme/cfwl_utils.h`.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum ThemeState {
    #[default]
    Normal = 1,
    Hover,
    Pressed,
    Disable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ThemeDirection {
    Up = 0,
    Down,
    Left,
    Right,
}

pub const FWL_THEME_COLOR_EDGE_RB1: u32 = 0xfff1efe2;
pub const FWL_THEME_COLOR_BACKGROUND: u32 = 0xffece9d8;
pub const FWL_THEME_COLOR_BACKGROUND_SELECTED: u32 = 0xff99c1da;
pub const FWL_THEME_FONT_SIZE: f32 = 12.0;
pub const FWL_THEME_TEXT_COLOR: u32 = 0xff000000;
pub const FWL_THEME_TEXT_DISABLED_COLOR: u32 = 0xffaca899;
