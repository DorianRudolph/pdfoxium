// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct port of `BC_PDF417BarcodeMatrix.{h,cpp}`.
use super::bc_pdf417_barcode_row::CbcBarcodeRow;
#[derive(Clone, Debug)]
pub struct CbcBarcodeMatrix {
    matrix: Vec<CbcBarcodeRow>,
    width: usize,
}
impl CbcBarcodeMatrix {
    pub fn new(columns: usize, height: usize) -> Option<Self> {
        if !(1..=30).contains(&columns) || !(3..=90).contains(&height) {
            return None;
        }
        let width = (columns + 4) * 17 + 1;
        Some(Self {
            matrix: (0..height).map(|_| CbcBarcodeRow::new(width)).collect(),
            width,
        })
    }
    pub fn get_row(&mut self, row: usize) -> Option<&mut CbcBarcodeRow> {
        self.matrix.get_mut(row)
    }
    pub fn get_width(&self) -> usize {
        self.width
    }
    pub fn get_height(&self) -> usize {
        self.matrix.len()
    }
    pub fn to_bit_array(&self) -> Vec<u8> {
        self.matrix
            .iter()
            .flat_map(|row| row.get_row().iter().copied())
            .collect()
    }
}
