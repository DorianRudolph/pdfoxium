// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original code is licensed as follows:
/*
 * Copyright 2006 Jeremias Maerki
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

//! ECC200 symbol capacities and data-region geometry.
#[derive(Debug)]
pub struct CbcSymbolInfo {
    pub data_capacity: usize,
    pub error_codewords: usize,
    rs_block_data: usize,
    rs_block_error: usize,
    pub matrix_width: usize,
    pub matrix_height: usize,
    data_regions: usize,
}
impl CbcSymbolInfo {
    const fn new(
        data_capacity: usize,
        error_codewords: usize,
        rs_block_data: usize,
        rs_block_error: usize,
        matrix_width: usize,
        matrix_height: usize,
        data_regions: usize,
    ) -> Self {
        Self {
            data_capacity,
            error_codewords,
            rs_block_data,
            rs_block_error,
            matrix_width,
            matrix_height,
            data_regions,
        }
    }
    pub fn lookup(data_codewords: usize, allow_rectangular: bool) -> Option<&'static Self> {
        SYMBOLS.iter().find(|s| {
            (allow_rectangular || s.matrix_width == s.matrix_height)
                && data_codewords <= s.data_capacity
        })
    }
    pub fn horizontal_data_regions(&self) -> usize {
        match self.data_regions {
            1 => 1,
            2 | 4 => 2,
            16 => 4,
            36 => 6,
            _ => unreachable!(),
        }
    }
    pub fn vertical_data_regions(&self) -> usize {
        match self.data_regions {
            1 | 2 => 1,
            4 => 2,
            16 => 4,
            36 => 6,
            _ => unreachable!(),
        }
    }
    pub fn symbol_data_width(&self) -> usize {
        self.horizontal_data_regions() * self.matrix_width
    }
    pub fn symbol_data_height(&self) -> usize {
        self.vertical_data_regions() * self.matrix_height
    }
    pub fn symbol_width(&self) -> usize {
        self.symbol_data_width() + 2 * self.horizontal_data_regions()
    }
    pub fn symbol_height(&self) -> usize {
        self.symbol_data_height() + 2 * self.vertical_data_regions()
    }
    pub fn interleaved_block_count(&self) -> usize {
        if self.data_capacity == 1558 {
            super::bc_data_matrix_symbol_info144::INTERLEAVED_BLOCK_COUNT
        } else {
            self.data_capacity / self.rs_block_data
        }
    }
    pub fn data_length_for_interleaved_block(&self, index: usize) -> usize {
        // The 144x144 symbol has eight 156-byte blocks and two 155-byte blocks.
        if self.data_capacity == 1558 {
            if index < 8 {
                156
            } else {
                155
            }
        } else {
            self.rs_block_data
        }
    }
    pub fn error_length_for_interleaved_block(&self, _index: usize) -> usize {
        self.rs_block_error
    }
}
pub(super) static SYMBOLS: [CbcSymbolInfo; 30] = [
    CbcSymbolInfo::new(3, 5, 3, 5, 8, 8, 1),
    CbcSymbolInfo::new(5, 7, 5, 7, 10, 10, 1),
    CbcSymbolInfo::new(5, 7, 5, 7, 16, 6, 1),
    CbcSymbolInfo::new(8, 10, 8, 10, 12, 12, 1),
    CbcSymbolInfo::new(10, 11, 10, 11, 14, 6, 2),
    CbcSymbolInfo::new(12, 12, 12, 12, 14, 14, 1),
    CbcSymbolInfo::new(16, 14, 16, 14, 24, 10, 1),
    CbcSymbolInfo::new(18, 14, 18, 14, 16, 16, 1),
    CbcSymbolInfo::new(22, 18, 22, 18, 18, 18, 1),
    CbcSymbolInfo::new(22, 18, 22, 18, 16, 10, 2),
    CbcSymbolInfo::new(30, 20, 30, 20, 20, 20, 1),
    CbcSymbolInfo::new(32, 24, 32, 24, 16, 14, 2),
    CbcSymbolInfo::new(36, 24, 36, 24, 22, 22, 1),
    CbcSymbolInfo::new(44, 28, 44, 28, 24, 24, 1),
    CbcSymbolInfo::new(49, 28, 49, 28, 22, 14, 2),
    CbcSymbolInfo::new(62, 36, 62, 36, 14, 14, 4),
    CbcSymbolInfo::new(86, 42, 86, 42, 16, 16, 4),
    CbcSymbolInfo::new(114, 48, 114, 48, 18, 18, 4),
    CbcSymbolInfo::new(144, 56, 144, 56, 20, 20, 4),
    CbcSymbolInfo::new(174, 68, 174, 68, 22, 22, 4),
    CbcSymbolInfo::new(204, 84, 102, 42, 24, 24, 4),
    CbcSymbolInfo::new(280, 112, 140, 56, 14, 14, 16),
    CbcSymbolInfo::new(368, 144, 92, 36, 16, 16, 16),
    CbcSymbolInfo::new(456, 192, 114, 48, 18, 18, 16),
    CbcSymbolInfo::new(576, 224, 144, 56, 20, 20, 16),
    CbcSymbolInfo::new(696, 272, 174, 68, 22, 22, 16),
    CbcSymbolInfo::new(816, 336, 136, 56, 24, 24, 16),
    CbcSymbolInfo::new(1050, 408, 175, 68, 18, 18, 36),
    CbcSymbolInfo::new(1304, 496, 163, 62, 20, 20, 36),
    CbcSymbolInfo::new(1558, 620, 0, 62, 22, 22, 36),
];
