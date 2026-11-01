// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Source CFGAS_TextUserData. Style/link owners share immutable metadata.
use super::cfgas_linkuserdata::CfgasLinkUserData;
use crate::fxcrt::css::cfx_csscomputedstyle::CfxCssComputedStyle;
use std::rc::Rc;
#[derive(Clone, Debug)]
pub struct CfgasTextUserData {
    pub style: Rc<CfxCssComputedStyle>,
    pub link_data: Option<Rc<CfgasLinkUserData>>,
}
impl CfgasTextUserData {
    pub fn new(style: Rc<CfxCssComputedStyle>) -> Self {
        Self {
            style,
            link_data: None,
        }
    }
    pub fn with_link(style: Rc<CfxCssComputedStyle>, link_data: Rc<CfgasLinkUserData>) -> Self {
        Self {
            style,
            link_data: Some(link_data),
        }
    }
}
