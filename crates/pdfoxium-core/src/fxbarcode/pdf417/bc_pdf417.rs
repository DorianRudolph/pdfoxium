// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original code copyright 2006 Jeremias Maerki in part, and ZXing Authors in part.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.
//! Direct port of `BC_PDF417.{h,cpp}`.
use super::bc_pdf417_barcode_matrix::CbcBarcodeMatrix;
use super::bc_pdf417_barcode_row::CbcBarcodeRow;
use super::bc_pdf417_error_correction::CbcPdf417ErrorCorrection;
use super::bc_pdf417_high_level_encoder::CbcPdf417HighLevelEncoder;
use super::tables::CODEWORD_TABLE;

pub struct CbcPdf417 {
    barcode_matrix: Option<CbcBarcodeMatrix>,
    min_cols: i32,
    max_cols: i32,
    min_rows: i32,
    max_rows: i32,
}
impl Default for CbcPdf417 {
    fn default() -> Self {
        Self {
            barcode_matrix: None,
            min_cols: 1,
            max_cols: 30,
            min_rows: 3,
            max_rows: 90,
        }
    }
}
impl CbcPdf417 {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get_barcode_matrix(&self) -> Option<&CbcBarcodeMatrix> {
        self.barcode_matrix.as_ref()
    }
    pub fn set_dimensions(&mut self, max_cols: i32, min_cols: i32, max_rows: i32, min_rows: i32) {
        self.max_cols = max_cols;
        self.min_cols = min_cols;
        self.max_rows = max_rows;
        self.min_rows = min_rows;
    }
    pub fn generate_barcode_logic(&mut self, msg: &[u32], level: i32) -> bool {
        let k = CbcPdf417ErrorCorrection::get_error_correction_codeword_count(level);
        if k < 0 {
            return false;
        }
        let Some(high_level) = CbcPdf417HighLevelEncoder::encode_high_level(msg) else {
            return false;
        };
        let Ok(source) = i32::try_from(high_level.len()) else {
            return false;
        };
        if source > 929 - k - 1 {
            return false;
        }
        let Some((cols, rows)) = self.determine_dimensions(source, k) else {
            return false;
        };
        let pad = Self::get_number_of_pad_codewords(source, k, cols, rows);
        let n = source + pad + 1;
        if n >= 929 {
            return false;
        }
        let mut data = Vec::with_capacity((n + k) as usize);
        data.push(n as u16);
        data.extend(high_level);
        data.resize(n as usize, 900);
        let Some(ec) = CbcPdf417ErrorCorrection::generate_error_correction(&data, level) else {
            return false;
        };
        data.extend(ec);
        let Some(mut matrix) = CbcBarcodeMatrix::new(cols as usize, rows as usize) else {
            return false;
        };
        if !Self::encode_low_level(&data, cols, rows, level, &mut matrix) {
            return false;
        }
        self.barcode_matrix = Some(matrix);
        true
    }
    fn calculate_number_of_rows(m: i32, k: i32, c: i32) -> i32 {
        let mut r = ((m + 1 + k) / c) + 1;
        if c * r >= m + 1 + k + c {
            r -= 1;
        }
        r
    }
    fn get_number_of_pad_codewords(m: i32, k: i32, c: i32, r: i32) -> i32 {
        let n = c * r - k;
        if n > m + 1 {
            n - m - 1
        } else {
            0
        }
    }
    fn encode_char(pattern: u32, len: u32, row: &mut CbcBarcodeRow) -> bool {
        let mut map = 1 << (len - 1);
        let mut last = pattern & map != 0;
        let mut width = 0;
        for _ in 0..len {
            let black = pattern & map != 0;
            if last == black {
                width += 1;
            } else {
                if !row.add_bar(last, width) {
                    return false;
                }
                last = black;
                width = 1;
            }
            map >>= 1;
        }
        row.add_bar(last, width)
    }
    fn encode_low_level(
        full: &[u16],
        cols: i32,
        rows: i32,
        level: i32,
        matrix: &mut CbcBarcodeMatrix,
    ) -> bool {
        if full.len() != (cols * rows) as usize || full.iter().any(|&word| word >= 929) {
            return false;
        }
        let mut idx = 0;
        for y in 0..rows {
            let Some(row) = matrix.get_row(y as usize) else {
                return false;
            };
            let cluster = y % 3;
            if !Self::encode_char(0x1fea8, 17, row) {
                return false;
            }
            let (left, right) = match cluster {
                0 => (30 * (y / 3) + (rows - 1) / 3, 30 * (y / 3) + cols - 1),
                1 => (
                    30 * (y / 3) + level * 3 + (rows - 1) % 3,
                    30 * (y / 3) + (rows - 1) / 3,
                ),
                _ => (
                    30 * (y / 3) + cols - 1,
                    30 * (y / 3) + level * 3 + (rows - 1) % 3,
                ),
            };
            let table = &CODEWORD_TABLE[cluster as usize];
            if !Self::encode_char(0x10000 | u32::from(table[left as usize]), 17, row) {
                return false;
            }
            for _ in 0..cols {
                if !Self::encode_char(0x10000 | u32::from(table[full[idx] as usize]), 17, row) {
                    return false;
                }
                idx += 1;
            }
            if !Self::encode_char(0x10000 | u32::from(table[right as usize]), 17, row)
                || !Self::encode_char(0x3fa29, 18, row)
            {
                return false;
            }
        }
        true
    }
    fn determine_dimensions(&self, source: i32, k: i32) -> Option<(i32, i32)> {
        if !(1..=30).contains(&self.min_cols)
            || !(self.min_cols..=30).contains(&self.max_cols)
            || !(3..=90).contains(&self.min_rows)
            || !(self.min_rows..=90).contains(&self.max_rows)
        {
            return None;
        }
        let mut dimensions = None;
        let mut ratio = 0.0f32;
        for cols in self.min_cols..=self.max_cols {
            let rows = Self::calculate_number_of_rows(source, k, cols);
            if rows < self.min_rows {
                break;
            }
            if rows > self.max_rows {
                continue;
            }
            let new_ratio = ((17 * cols + 69) as f32 * 0.357) / (rows as f32 * 2.0);
            if dimensions.is_some() && (new_ratio - 3.0).abs() > (ratio - 3.0).abs() {
                continue;
            }
            ratio = new_ratio;
            dimensions = Some((cols, rows));
        }
        if dimensions.is_none() {
            let rows = Self::calculate_number_of_rows(source, k, self.max_cols);
            if rows < self.min_rows {
                dimensions = Some((self.max_cols, self.min_rows));
            } else if (3..=90).contains(&rows) {
                dimensions = Some((self.max_cols, rows));
            }
        }
        dimensions
    }
}
