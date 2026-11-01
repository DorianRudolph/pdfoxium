// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original code copyright 2012 ZXing authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.
//! Direct port of `BC_PDF417Writer.{h,cpp}`.
use super::bc_pdf417::CbcPdf417;
use crate::fxbarcode::bc_two_dim_writer::{CbcTwoDimWriter, EncodeResult};

pub struct CbcPdf417Writer {
    pub base: CbcTwoDimWriter,
}
impl Default for CbcPdf417Writer {
    fn default() -> Self {
        Self {
            base: CbcTwoDimWriter::new(false),
        }
    }
}
impl CbcPdf417Writer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_error_correction_level(&mut self, level: i32) -> bool {
        if !(0..=8).contains(&level) {
            return false;
        }
        self.base.set_correction_level(level);
        true
    }
    pub fn encode(&mut self, contents: &[u32]) -> EncodeResult {
        let mut encoder = CbcPdf417::new();
        let width = self.base.writer.width;
        let height = self.base.writer.height;
        let module_width = self.base.writer.module_width;
        if module_width <= 0 {
            return EncodeResult::default();
        }
        let col = ((i64::from(width) / i64::from(module_width) - 69) / 17) as i32;
        let row = (i64::from(height) / (i64::from(module_width) * 20)) as i32;
        if (3..=90).contains(&row) && (1..=30).contains(&col) {
            encoder.set_dimensions(col, 1, row, 3);
        } else if (1..=30).contains(&col) {
            encoder.set_dimensions(col, col, 90, 3);
        } else if (3..=90).contains(&row) {
            encoder.set_dimensions(30, 1, row, row);
        }
        if !encoder.generate_barcode_logic(contents, self.base.error_correction_level()) {
            return EncodeResult::default();
        }
        let Some(matrix) = encoder.get_barcode_matrix() else {
            return EncodeResult::default();
        };
        let mut data = matrix.to_bit_array();
        let (mut width, mut height) = (matrix.get_width(), matrix.get_height());
        if width < height {
            let temp = data.clone();
            for i in 0..height {
                for j in 0..width {
                    data[j * height + height - i - 1] = temp[i * width + j];
                }
            }
            std::mem::swap(&mut width, &mut height);
        }
        EncodeResult::new(data, width as i32, height as i32)
    }
}
