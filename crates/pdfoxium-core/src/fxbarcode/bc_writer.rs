// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Shared dimensions and limits from `fxbarcode/BC_Writer.{h,cpp}`.
use crate::fxge::cfx_font::CfxFont;

pub const BAR_COLOR: u32 = 0xff000000;
pub const BACKGROUND_COLOR: u32 = 0xffffffff;

/// A retained font owner. XFA fonts expose their existing device font through
/// this adapter, without duplicating the font or its backing data.
pub trait BarcodeFont {
    fn dev_font(&self) -> &CfxFont<'static>;
}
impl BarcodeFont for CfxFont<'static> {
    fn dev_font(&self) -> &CfxFont<'static> {
        self
    }
}

#[derive(Clone, Debug)]
pub struct CbcWriter {
    pub module_height: i32,
    pub module_width: i32,
    pub height: i32,
    pub width: i32,
}
impl Default for CbcWriter {
    fn default() -> Self {
        Self {
            module_height: 1,
            module_width: 1,
            height: 320,
            width: 640,
        }
    }
}
impl CbcWriter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_module_height(&mut self, height: i32) -> bool {
        if !(1..=10).contains(&height) {
            return false;
        }
        self.module_height = height;
        true
    }
    pub fn set_module_width(&mut self, width: i32) -> bool {
        if !(1..=10).contains(&width) {
            return false;
        }
        self.module_width = width;
        true
    }
    pub fn set_height(&mut self, height: i32) {
        self.height = height;
    }
    pub fn set_width(&mut self, width: i32) {
        self.width = width;
    }
}
