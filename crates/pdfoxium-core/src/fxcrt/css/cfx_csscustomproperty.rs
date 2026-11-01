// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Owned custom declarations from `cfx_csscustomproperty.{h,cpp}`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CfxCssCustomProperty {
    pub name: Vec<u32>,
    pub value: Vec<u32>,
}
impl CfxCssCustomProperty {
    pub fn new(name: &[u32], value: &[u32]) -> Self {
        Self {
            name: name.to_vec(),
            value: value.to_vec(),
        }
    }
}
