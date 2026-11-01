// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Sorted rich-text tabs from `cxfa_texttabstopscontext.{h,cpp}`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XfaTabstop {
    pub align: u32,
    pub position: f32,
}

#[derive(Clone, Debug)]
pub struct CxfaTextTabstopsContext {
    pub tab_index: i32,
    pub has_tabstops: bool,
    pub tab_width: f32,
    pub left: f32,
    pub tabstops: Vec<XfaTabstop>,
}
impl Default for CxfaTextTabstopsContext {
    fn default() -> Self {
        Self {
            tab_index: -1,
            has_tabstops: false,
            tab_width: 0.0,
            left: 0.0,
            tabstops: Vec::new(),
        }
    }
}
impl CxfaTextTabstopsContext {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn append(&mut self, align: u32, position: f32) {
        let index = self
            .tabstops
            .partition_point(|stop| stop.position < position);
        self.tabstops.insert(index, XfaTabstop { align, position });
    }
    pub fn remove_all(&mut self) {
        self.tabstops.clear();
    }
    pub fn reset(&mut self) {
        self.tab_index = -1;
        self.has_tabstops = false;
        self.tab_width = 0.0;
        self.left = 0.0;
    }
}
