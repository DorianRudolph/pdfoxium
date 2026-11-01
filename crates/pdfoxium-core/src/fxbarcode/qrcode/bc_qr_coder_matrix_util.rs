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

//! Direct port of `fxbarcode/qrcode/BC_QRCoderMatrixUtil.cpp` and its header.

const POSITION_COORDINATE_PATTERN_TABLE: [[u8; 7]; 39] = [
    [6, 18, 0, 0, 0, 0, 0],
    [6, 22, 0, 0, 0, 0, 0],
    [6, 26, 0, 0, 0, 0, 0],
    [6, 30, 0, 0, 0, 0, 0],
    [6, 34, 0, 0, 0, 0, 0],
    [6, 22, 38, 0, 0, 0, 0],
    [6, 24, 42, 0, 0, 0, 0],
    [6, 26, 46, 0, 0, 0, 0],
    [6, 28, 50, 0, 0, 0, 0],
    [6, 30, 54, 0, 0, 0, 0],
    [6, 32, 58, 0, 0, 0, 0],
    [6, 34, 62, 0, 0, 0, 0],
    [6, 26, 46, 66, 0, 0, 0],
    [6, 26, 48, 70, 0, 0, 0],
    [6, 26, 50, 74, 0, 0, 0],
    [6, 30, 54, 78, 0, 0, 0],
    [6, 30, 56, 82, 0, 0, 0],
    [6, 30, 58, 86, 0, 0, 0],
    [6, 34, 62, 90, 0, 0, 0],
    [6, 28, 50, 72, 94, 0, 0],
    [6, 26, 50, 74, 98, 0, 0],
    [6, 30, 54, 78, 102, 0, 0],
    [6, 28, 54, 80, 106, 0, 0],
    [6, 32, 58, 84, 110, 0, 0],
    [6, 30, 58, 86, 114, 0, 0],
    [6, 34, 62, 90, 118, 0, 0],
    [6, 26, 50, 74, 98, 122, 0],
    [6, 30, 54, 78, 102, 126, 0],
    [6, 26, 52, 78, 104, 130, 0],
    [6, 30, 56, 82, 108, 134, 0],
    [6, 34, 60, 86, 112, 138, 0],
    [6, 30, 58, 86, 114, 142, 0],
    [6, 34, 62, 90, 118, 146, 0],
    [6, 30, 54, 78, 102, 126, 150],
    [6, 24, 50, 76, 102, 128, 154],
    [6, 28, 54, 80, 106, 132, 158],
    [6, 32, 58, 84, 110, 136, 162],
    [6, 26, 54, 82, 110, 138, 166],
    [6, 30, 58, 86, 114, 142, 170],
];

use super::{
    bc_qr_coder::CbcQrCoder, bc_qr_coder_bit_vector::CbcQrCoderBitVector,
    bc_qr_coder_error_correction_level::CbcQrCoderErrorCorrectionLevel,
    bc_qr_coder_mask_util::CbcQrCoderMaskUtil,
};
use crate::fxbarcode::common::bc_common_byte_matrix::CbcCommonByteMatrix;
const POSITION_DETECTION_PATTERN: [[u8; 7]; 7] = [
    [1, 1, 1, 1, 1, 1, 1],
    [1, 0, 0, 0, 0, 0, 1],
    [1, 0, 1, 1, 1, 0, 1],
    [1, 0, 1, 1, 1, 0, 1],
    [1, 0, 1, 1, 1, 0, 1],
    [1, 0, 0, 0, 0, 0, 1],
    [1, 1, 1, 1, 1, 1, 1],
];
const POSITION_ADJUSTMENT_PATTERN: [[u8; 5]; 5] = [
    [1, 1, 1, 1, 1],
    [1, 0, 0, 0, 1],
    [1, 0, 1, 0, 1],
    [1, 0, 0, 0, 1],
    [1, 1, 1, 1, 1],
];
const TYPE_INFO_COORDINATES: [(usize, usize); 15] = [
    (8, 0),
    (8, 1),
    (8, 2),
    (8, 3),
    (8, 4),
    (8, 5),
    (8, 7),
    (8, 8),
    (7, 8),
    (5, 8),
    (4, 8),
    (3, 8),
    (2, 8),
    (1, 8),
    (0, 8),
];
pub struct CbcQrCoderMatrixUtil;
impl CbcQrCoderMatrixUtil {
    fn embed_data_bits(
        bits: &CbcQrCoderBitVector,
        mask: i32,
        matrix: &mut CbcCommonByteMatrix,
    ) -> bool {
        let mut index = 0;
        let mut direction = -1;
        let mut x = matrix.width() as i32 - 1;
        let mut y = matrix.height() as i32 - 1;
        while x > 0 {
            if x == 6 {
                x -= 1;
            }
            while y >= 0 && y < matrix.height() as i32 {
                if y == 6 {
                    y += direction;
                    continue;
                }
                for i in 0..2 {
                    let xx = x - i;
                    if matrix.get(xx as usize, y as usize) != 255 {
                        continue;
                    }
                    let mut bit = if index < bits.size() {
                        let b = bits.at(index);
                        index += 1;
                        b
                    } else {
                        0
                    };
                    if CbcQrCoderMaskUtil::get_data_mask_bit(mask, xx, y) {
                        bit ^= 1;
                    }
                    matrix.set(xx as usize, y as usize, bit);
                }
                y += direction;
            }
            direction = -direction;
            y += direction;
            x -= 2;
        }
        index == bits.size()
    }
    pub fn calculate_bch_code(value: i32, poly: i32) -> Option<i32> {
        if value < 0 || poly <= 0 {
            return None;
        }
        let poly = poly as u64;
        let degree = 64 - poly.leading_zeros();
        let mut value = (value as u64) << (degree - 1);
        while value != 0 && 64 - value.leading_zeros() >= degree {
            value ^= poly << (64 - value.leading_zeros() - degree);
        }
        Some(value as i32)
    }
    pub fn make_type_info_bits(
        ec: CbcQrCoderErrorCorrectionLevel,
        mask: i32,
        bits: &mut CbcQrCoderBitVector,
    ) -> bool {
        if !CbcQrCoder::is_valid_mask_pattern(mask) {
            return false;
        }
        let info = (ec.get_bits() << 3) | mask;
        bits.append_bits(info, 5);
        let Some(bch) = Self::calculate_bch_code(info, 0x537) else {
            return false;
        };
        bits.append_bits(bch, 10);
        let mut mask_bits = CbcQrCoderBitVector::new();
        mask_bits.append_bits(0x5412, 15);
        bits.xor(&mask_bits)
    }
    fn embed_type_info(
        ec: CbcQrCoderErrorCorrectionLevel,
        mask: i32,
        matrix: &mut CbcCommonByteMatrix,
    ) -> bool {
        let mut bits = CbcQrCoderBitVector::new();
        if !Self::make_type_info_bits(ec, mask, &mut bits) {
            return false;
        }
        for (i, &(x, y)) in TYPE_INFO_COORDINATES.iter().enumerate() {
            let bit = bits.at(bits.size() - 1 - i);
            matrix.set(x, y, bit);
            if i < 8 {
                matrix.set(matrix.width() - i - 1, 8, bit);
            } else {
                matrix.set(8, matrix.height() - 7 + (i - 8), bit);
            }
        }
        true
    }
    fn maybe_embed_version_info(version: i32, matrix: &mut CbcCommonByteMatrix) {
        if version < 7 {
            return;
        }
        let mut bits = CbcQrCoderBitVector::new();
        bits.append_bits(version, 6);
        bits.append_bits(
            Self::calculate_bch_code(version, 0x1f25).expect("QR version BCH"),
            12,
        );
        let mut index: usize = 17;
        for i in 0..6 {
            for j in 0..3 {
                let bit = bits.at(index);
                index = index.saturating_sub(1);
                matrix.set(i, matrix.height() - 11 + j, bit);
                matrix.set(matrix.height() - 11 + j, i, bit);
            }
        }
    }
    fn embed_pattern<const N: usize>(
        x_start: usize,
        y_start: usize,
        pattern: &[[u8; N]; N],
        matrix: &mut CbcCommonByteMatrix,
    ) -> bool {
        for (y, row) in pattern.iter().enumerate() {
            for (x, &bit) in row.iter().enumerate() {
                if matrix.get(x_start + x, y_start + y) != 255 {
                    return false;
                }
                matrix.set(x_start + x, y_start + y, bit);
            }
        }
        true
    }
    fn embed_horizontal_separation_pattern(
        x_start: usize,
        y: usize,
        matrix: &mut CbcCommonByteMatrix,
    ) -> bool {
        for x in x_start..x_start + 8 {
            if matrix.get(x, y) != 255 {
                return false;
            }
            matrix.set(x, y, 0);
        }
        true
    }
    fn embed_vertical_separation_pattern(
        x: usize,
        y_start: usize,
        matrix: &mut CbcCommonByteMatrix,
    ) -> bool {
        for y in y_start..y_start + 7 {
            if matrix.get(x, y) != 255 {
                return false;
            }
            matrix.set(x, y, 0);
        }
        true
    }
    fn embed_position_detection_patterns_and_separators(matrix: &mut CbcCommonByteMatrix) -> bool {
        let width = matrix.width();
        let height = matrix.height();
        Self::embed_pattern(0, 0, &POSITION_DETECTION_PATTERN, matrix)
            && Self::embed_pattern(width - 7, 0, &POSITION_DETECTION_PATTERN, matrix)
            && Self::embed_pattern(0, width - 7, &POSITION_DETECTION_PATTERN, matrix)
            && Self::embed_horizontal_separation_pattern(0, 7, matrix)
            && Self::embed_horizontal_separation_pattern(width - 8, 7, matrix)
            && Self::embed_horizontal_separation_pattern(0, width - 8, matrix)
            && Self::embed_vertical_separation_pattern(7, 0, matrix)
            && Self::embed_vertical_separation_pattern(height - 8, 0, matrix)
            && Self::embed_vertical_separation_pattern(7, height - 7, matrix)
    }
    fn maybe_embed_position_adjustment_patterns(
        version: i32,
        matrix: &mut CbcCommonByteMatrix,
    ) -> bool {
        if version < 2 {
            return true;
        }
        let Some(coordinates) = POSITION_COORDINATE_PATTERN_TABLE.get((version - 2) as usize)
        else {
            return false;
        };
        for &y in coordinates.iter().take_while(|&&v| v != 0) {
            for &x in coordinates.iter().take_while(|&&v| v != 0) {
                let (x, y) = (usize::from(x), usize::from(y));
                if matrix.get(x, y) == 255
                    && !Self::embed_pattern(x - 2, y - 2, &POSITION_ADJUSTMENT_PATTERN, matrix)
                {
                    return false;
                }
            }
        }
        true
    }
    fn embed_timing_patterns(matrix: &mut CbcCommonByteMatrix) -> bool {
        for i in 8..matrix.width() - 8 {
            let bit = ((i + 1) % 2) as u8;
            let horizontal = matrix.get(i, 6);
            let vertical = matrix.get(6, i);
            if ![255, 0, 1].contains(&horizontal) || ![255, 0, 1].contains(&vertical) {
                return false;
            }
            if horizontal == 255 {
                matrix.set(i, 6, bit);
            }
            if vertical == 255 {
                matrix.set(6, i, bit);
            }
        }
        true
    }
    fn embed_basic_patterns(version: i32, matrix: &mut CbcCommonByteMatrix) -> bool {
        if !Self::embed_position_detection_patterns_and_separators(matrix)
            || matrix.get(8, matrix.height() - 8) == 0
        {
            return false;
        }
        matrix.set(8, matrix.height() - 8, 1);
        Self::maybe_embed_position_adjustment_patterns(version, matrix)
            && Self::embed_timing_patterns(matrix)
    }
    pub fn build_matrix(
        bits: &CbcQrCoderBitVector,
        ec: CbcQrCoderErrorCorrectionLevel,
        version: i32,
        mask: i32,
        matrix: &mut CbcCommonByteMatrix,
    ) -> bool {
        if !(1..=40).contains(&version)
            || !CbcQrCoder::is_valid_mask_pattern(mask)
            || matrix.width() != matrix.height()
            || matrix.width() != (17 + version * 4) as usize
        {
            return false;
        }
        matrix.fill(255);
        if !Self::embed_basic_patterns(version, matrix) || !Self::embed_type_info(ec, mask, matrix)
        {
            return false;
        }
        Self::maybe_embed_version_info(version, matrix);
        Self::embed_data_bits(bits, mask, matrix)
    }
}
