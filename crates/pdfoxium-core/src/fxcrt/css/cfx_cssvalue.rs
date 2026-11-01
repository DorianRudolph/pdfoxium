// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Tagged ownership for `CFX_CSSValue` and its enum/color/string/list
//! subclasses (`cfx_css{value,enumvalue,colorvalue,stringvalue,valuelist}`).
//! Rc keeps shared immutable declaration values without C++ virtual downcasts.
use super::cfx_css::CfxCssPropertyValue;
use super::cfx_cssnumbervalue::CfxCssNumber;
use std::rc::Rc;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CfxCssPrimitiveType {
    Unknown,
    Number,
    String,
    RGB,
    Enum,
    Function,
    List,
}
#[derive(Clone, Debug, PartialEq)]
pub enum CfxCssValue {
    Number(CfxCssNumber),
    String(Vec<u32>),
    Color(u32),
    Enum(CfxCssPropertyValue),
    List(Vec<Rc<CfxCssValue>>),
}
impl CfxCssValue {
    pub fn get_type(&self) -> CfxCssPrimitiveType {
        match self {
            Self::Number(_) => CfxCssPrimitiveType::Number,
            Self::String(_) => CfxCssPrimitiveType::String,
            Self::Color(_) => CfxCssPrimitiveType::RGB,
            Self::Enum(_) => CfxCssPrimitiveType::Enum,
            Self::List(_) => CfxCssPrimitiveType::List,
        }
    }
}
/// `CFX_CSSColorPack`: ARGB packed as 0xAARRGGBB.
pub const fn color_pack(a: u8, r: u8, g: u8, b: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}
