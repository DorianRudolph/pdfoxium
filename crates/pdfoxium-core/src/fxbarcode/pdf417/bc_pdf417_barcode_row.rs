// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct port of `BC_PDF417BarcodeRow.{h,cpp}`.
#[derive(Clone, Debug)]
pub struct CbcBarcodeRow {
    row: Vec<u8>,
    add_bar_offset: usize,
}
impl CbcBarcodeRow {
    pub fn new(width: usize) -> Self {
        Self {
            row: vec![0; width],
            add_bar_offset: 0,
        }
    }
    pub fn add_bar(&mut self, black: bool, width: usize) -> bool {
        let Some(end) = self.add_bar_offset.checked_add(width) else {
            return false;
        };
        let Some(target) = self.row.get_mut(self.add_bar_offset..end) else {
            return false;
        };
        target.fill(u8::from(black));
        self.add_bar_offset = end;
        true
    }
    pub fn get_row(&self) -> &[u8] {
        &self.row
    }
}
