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

//! Direct port of `fxbarcode/qrcode/BC_QRCoderMaskUtil.cpp` and its header.

use crate::fxbarcode::common::bc_common_byte_matrix::CbcCommonByteMatrix;
pub struct CbcQrCoderMaskUtil;
impl CbcQrCoderMaskUtil {
    fn apply_mask_penalty_rule1_internal(matrix: &CbcCommonByteMatrix, horizontal: bool) -> i32 {
        let mut penalty = 0;
        let mut same = 0;
        let mut prev = 255;
        let (outer, inner) = if horizontal {
            (matrix.height(), matrix.width())
        } else {
            (matrix.width(), matrix.height())
        };
        for i in 0..outer {
            for j in 0..inner {
                let bit = if horizontal {
                    matrix.get(j, i)
                } else {
                    matrix.get(i, j)
                };
                if bit == prev {
                    same += 1;
                    if same == 5 {
                        penalty += 3;
                    } else if same > 5 {
                        penalty += 1;
                    }
                } else {
                    same = 1;
                    prev = bit;
                }
            }
            same = 0;
        }
        penalty
    }
    pub fn apply_mask_penalty_rule1(matrix: &CbcCommonByteMatrix) -> i32 {
        Self::apply_mask_penalty_rule1_internal(matrix, true)
            + Self::apply_mask_penalty_rule1_internal(matrix, false)
    }
    pub fn apply_mask_penalty_rule2(matrix: &CbcCommonByteMatrix) -> i32 {
        let mut penalty = 0;
        for y in 0..matrix.height().saturating_sub(1) {
            for x in 0..matrix.width().saturating_sub(1) {
                let bit = matrix.get(x, y);
                if bit == matrix.get(x + 1, y)
                    && bit == matrix.get(x, y + 1)
                    && bit == matrix.get(x + 1, y + 1)
                {
                    penalty += 3;
                }
            }
        }
        penalty
    }
    pub fn apply_mask_penalty_rule3(matrix: &CbcCommonByteMatrix) -> i32 {
        let mut penalty = 0;
        let width = matrix.width();
        let height = matrix.height();
        for y in 0..height {
            for x in 0..width {
                if x == 0 && (y <= 6 || (y + 7 >= height && y < height)) {
                    continue;
                }
                if x + 7 == width && y <= 6 {
                    continue;
                }
                if y == 0 && (x <= 6 || (x + 7 >= width && x < width)) {
                    continue;
                }
                if y + 7 == height && x <= 6 {
                    continue;
                }
                if x + 6 < width
                    && (0..7).all(|i| matrix.get(x + i, y) == [1, 0, 1, 1, 1, 0, 1][i])
                    && ((x + 10 < width && (7..11).all(|i| matrix.get(x + i, y) == 0))
                        || (x >= 4 && (1..5).all(|i| matrix.get(x - i, y) == 0)))
                {
                    penalty += 40;
                }
                if y + 6 < height
                    && (0..7).all(|i| matrix.get(x, y + i) == [1, 0, 1, 1, 1, 0, 1][i])
                    && ((y + 10 < height && (7..11).all(|i| matrix.get(x, y + i) == 0))
                        || (y >= 4 && (1..5).all(|i| matrix.get(x, y - i) == 0)))
                {
                    penalty += 40;
                }
            }
        }
        penalty
    }
    pub fn apply_mask_penalty_rule4(matrix: &CbcCommonByteMatrix) -> i32 {
        let cells = matrix.get_array();
        if cells.is_empty() {
            return 0;
        }
        let dark = cells.iter().filter(|&&bit| bit == 1).count();
        (((dark as f64 / cells.len() as f64 * 100.0 - 50.0) as i32) / 5).abs() * 5 * 10
    }
    pub fn get_data_mask_bit(mask: i32, x: i32, y: i32) -> bool {
        let (x, y) = (i64::from(x), i64::from(y));
        let intermediate = match mask {
            0 => (y + x) & 1,
            1 => y & 1,
            2 => x % 3,
            3 => (y + x) % 3,
            4 => ((y >> 1) + (x / 3)) & 1,
            5 => ((y * x) & 1) + (y * x) % 3,
            6 => (((y * x) & 1) + (y * x) % 3) & 1,
            7 => (((y * x) % 3) + ((y + x) & 1)) & 1,
            _ => return false,
        };
        intermediate == 0
    }
    pub fn calculate_mask_penalty(matrix: &CbcCommonByteMatrix) -> i32 {
        Self::apply_mask_penalty_rule1(matrix)
            + Self::apply_mask_penalty_rule2(matrix)
            + Self::apply_mask_penalty_rule3(matrix)
            + Self::apply_mask_penalty_rule4(matrix)
    }
}
