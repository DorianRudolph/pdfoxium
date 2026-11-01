// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Fully initialized property records from `cfx_csspropertyholder.{h,cpp}`.
use super::cfx_css::CfxCssProperty;
use super::cfx_cssvalue::CfxCssValue;
use std::rc::Rc;
#[derive(Clone, Debug, PartialEq)]
pub struct CfxCssPropertyHolder {
    pub property: CfxCssProperty,
    pub value: Rc<CfxCssValue>,
    pub important: bool,
}
impl CfxCssPropertyHolder {
    pub fn new(property: CfxCssProperty, value: Rc<CfxCssValue>, important: bool) -> Self {
        Self {
            property,
            value,
            important,
        }
    }
}
