// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original code is licensed as follows:
/*
 * Copyright 2008 ZXing authors
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use super::{
    bc_default_placement::CbcDefaultPlacement, bc_error_correction::encode_ecc200,
    bc_high_level_encoder::encode_high_level, bc_symbol_info::CbcSymbolInfo,
};
use crate::fxbarcode::bc_two_dim_writer::{CbcTwoDimWriter, EncodeResult};
use crate::fxbarcode::common::bc_common_byte_matrix::CbcCommonByteMatrix;
pub struct CbcDataMatrixWriter {
    pub base: CbcTwoDimWriter,
}
impl Default for CbcDataMatrixWriter {
    fn default() -> Self {
        Self::new()
    }
}
impl CbcDataMatrixWriter {
    pub fn new() -> Self {
        Self {
            base: CbcTwoDimWriter::new(true),
        }
    }
    pub fn set_error_correction_level(&mut self, level: i32) -> bool {
        self.base.set_correction_level(level);
        true
    }
    pub fn encode(&mut self, contents: &[u32]) -> EncodeResult {
        let encoded = encode_high_level(contents);
        if encoded.is_empty() {
            return EncodeResult::default();
        }
        let Some(symbol) = CbcSymbolInfo::lookup(encoded.len(), false) else {
            return EncodeResult::default();
        };
        let Some(codewords) = encode_ecc200(&encoded, symbol) else {
            return EncodeResult::default();
        };
        let Some(placement) = CbcDefaultPlacement::new(
            codewords,
            symbol.symbol_data_width(),
            symbol.symbol_data_height(),
        ) else {
            return EncodeResult::default();
        };
        let Some(matrix) = encode_low_level(&placement, symbol) else {
            return EncodeResult::default();
        };
        let width = matrix.width() as i32;
        let height = matrix.height() as i32;
        EncodeResult::new(matrix.take_array(), width, height)
    }
}
fn encode_low_level(
    placement: &CbcDefaultPlacement,
    symbol: &CbcSymbolInfo,
) -> Option<CbcCommonByteMatrix> {
    let data_width = symbol.symbol_data_width();
    let data_height = symbol.symbol_data_height();
    let width = symbol.symbol_width();
    let height = symbol.symbol_height();
    let mut matrix = CbcCommonByteMatrix::new(width, height)?;
    let mut matrix_y = 0;
    for y in 0..data_height {
        if y % symbol.matrix_height == 0 {
            for x in 0..width {
                matrix.set(x, matrix_y, u8::from(x % 2 == 0));
            }
            matrix_y += 1;
        }
        let mut matrix_x = 0;
        for x in 0..data_width {
            if x % symbol.matrix_width == 0 {
                matrix.set(matrix_x, matrix_y, 1);
                matrix_x += 1;
            }
            matrix.set(matrix_x, matrix_y, u8::from(placement.get_bit(x, y)));
            matrix_x += 1;
            if x % symbol.matrix_width == symbol.matrix_width - 1 {
                matrix.set(matrix_x, matrix_y, u8::from(y % 2 == 0));
                matrix_x += 1;
            }
        }
        matrix_y += 1;
        if y % symbol.matrix_height == symbol.matrix_height - 1 {
            for x in 0..width {
                matrix.set(x, matrix_y, 1);
            }
            matrix_y += 1;
        }
    }
    Some(matrix)
}
