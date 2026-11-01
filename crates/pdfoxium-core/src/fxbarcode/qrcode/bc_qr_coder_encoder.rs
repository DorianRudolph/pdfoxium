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

//! Direct port of `fxbarcode/qrcode/BC_QRCoderEncoder.cpp` and its header.

use super::{
    bc_qr_coder::CbcQrCoder, bc_qr_coder_bit_vector::CbcQrCoderBitVector,
    bc_qr_coder_error_correction_level::CbcQrCoderErrorCorrectionLevel,
    bc_qr_coder_mask_util::CbcQrCoderMaskUtil, bc_qr_coder_matrix_util::CbcQrCoderMatrixUtil,
    bc_qr_coder_mode::CbcQrCoderMode, bc_qr_coder_version::CbcQrCoderVersion,
};
use crate::{
    fxbarcode::common::{
        bc_common_byte_matrix::CbcCommonByteMatrix,
        reedsolomon::{
            bc_reed_solomon::CbcReedSolomonEncoder, bc_reed_solomon_gf256::QR_CODE_FIELD,
        },
    },
    fxcrt::fx_string::utf8_encode,
};
const ALPHA_NUMERIC_TABLE: [i8; 59] = [
    36, -1, -1, -1, 37, 38, -1, -1, -1, -1, 39, 40, -1, 41, 42, 43, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
    44, -1, -1, -1, -1, -1, -1, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
    27, 28, 29, 30, 31, 32, 33, 34, 35,
];
struct QrCoderBlockPair {
    data: Vec<u8>,
    ecc: Vec<u8>,
}
pub struct CbcQrCoderEncoder;
impl CbcQrCoderEncoder {
    pub fn get_alpha_numeric_code(code: u8) -> i32 {
        code.checked_sub(32)
            .and_then(|i| ALPHA_NUMERIC_TABLE.get(usize::from(i)))
            .map_or(-1, |&c| i32::from(c))
    }
    pub fn choose_mode(content: &[u8]) -> CbcQrCoderMode {
        let mut numeric = false;
        let mut alpha_numeric = false;
        for &c in content {
            if c.is_ascii_digit() {
                numeric = true;
            } else if Self::get_alpha_numeric_code(c) != -1 {
                alpha_numeric = true;
            } else {
                return CbcQrCoderMode::Byte;
            }
        }
        if alpha_numeric {
            CbcQrCoderMode::Alphanumeric
        } else if numeric {
            CbcQrCoderMode::Numeric
        } else {
            CbcQrCoderMode::Byte
        }
    }
    pub fn append_bytes(
        content: &[u8],
        mode: CbcQrCoderMode,
        bits: &mut CbcQrCoderBitVector,
    ) -> bool {
        match mode {
            CbcQrCoderMode::Numeric => {
                for chunk in content.chunks(3) {
                    if !chunk.iter().all(u8::is_ascii_digit) {
                        return false;
                    }
                    let value = chunk.iter().fold(0, |a, &b| a * 10 + i32::from(b - b'0'));
                    bits.append_bits(
                        value,
                        match chunk.len() {
                            3 => 10,
                            2 => 7,
                            _ => 4,
                        },
                    );
                }
            }
            CbcQrCoderMode::Alphanumeric => {
                for chunk in content.chunks(2) {
                    let first = Self::get_alpha_numeric_code(chunk[0]);
                    if first < 0 {
                        return false;
                    }
                    if chunk.len() == 2 {
                        let second = Self::get_alpha_numeric_code(chunk[1]);
                        if second < 0 {
                            return false;
                        }
                        bits.append_bits(first * 45 + second, 11);
                    } else {
                        bits.append_bits(first, 6);
                    }
                }
            }
            CbcQrCoderMode::Byte => {
                for &c in content {
                    bits.append_bits(i32::from(c), 8);
                }
            }
        }
        true
    }
    pub fn append_length_info(
        num_letters: usize,
        version: i32,
        mode: CbcQrCoderMode,
        bits: &mut CbcQrCoderBitVector,
    ) -> bool {
        if CbcQrCoderVersion::get_version_for_number(version).is_none() {
            return false;
        }
        let num_bits = mode.get_character_count_bits(version);
        // The source accidentally checks num_bits rather than num_letters and silently
        // truncates oversized lengths. Reject a value that cannot fit its count field.
        if num_bits == 0 || num_letters >= (1usize << num_bits) {
            return false;
        }
        bits.append_bits(num_letters as i32, num_bits);
        true
    }
    fn init_qr_code(
        num_input_bytes: usize,
        ec: CbcQrCoderErrorCorrectionLevel,
        qr: &mut CbcQrCoder,
    ) -> bool {
        qr.ec_level = Some(ec);
        for i in 1..=CbcQrCoderVersion::MAX_VERSION {
            let version = CbcQrCoderVersion::get_version_for_number(i).expect("bounded QR version");
            let total = version.get_total_code_words();
            let blocks = version.get_ec_blocks_for_level(ec);
            let ec_bytes = blocks.get_total_ec_code_words();
            let data = total - ec_bytes;
            if num_input_bytes
                .checked_add(3)
                .is_some_and(|n| n <= data as usize)
            {
                qr.version = i;
                qr.num_total_bytes = total;
                qr.num_data_bytes = data;
                qr.num_rs_blocks = blocks.get_num_blocks();
                qr.num_ec_bytes = ec_bytes;
                qr.matrix_width = version.get_dimension_for_version();
                return true;
            }
        }
        false
    }
    pub fn generate_ec_bytes(data: &[u8], num_ec_bytes: usize) -> Option<Vec<u8>> {
        let total = data.len().checked_add(num_ec_bytes)?;
        if num_ec_bytes == 0 || total > 255 {
            return None;
        }
        let mut encoded = Vec::with_capacity(total);
        encoded.extend(data.iter().map(|&b| i32::from(b)));
        encoded.resize(total, 0);
        let mut encoder = CbcReedSolomonEncoder::new(&QR_CODE_FIELD);
        if !encoder.encode(&mut encoded, num_ec_bytes) {
            return None;
        }
        Some(encoded[data.len()..].iter().map(|&c| c as u8).collect())
    }
    pub fn get_num_data_bytes_and_num_ec_bytes_for_block_id(
        total: i32,
        data: i32,
        rs_blocks: i32,
        block_id: i32,
    ) -> Option<(usize, usize)> {
        if total < 0
            || data < 0
            || data > total
            || rs_blocks <= 0
            || block_id < 0
            || block_id >= rs_blocks
        {
            return None;
        }
        let group2_blocks = total % rs_blocks;
        let group1_blocks = rs_blocks - group2_blocks;
        let group1_total = total / rs_blocks;
        let group1_data = data / rs_blocks;
        let extra = i32::from(block_id >= group1_blocks);
        let ec = group1_total - group1_data;
        if ec <= 0 {
            return None;
        }
        Some(((group1_data + extra) as usize, ec as usize))
    }
    pub fn terminate_bits(num_data_bytes: usize, bits: &mut CbcQrCoderBitVector) -> bool {
        let Some(capacity) = num_data_bytes.checked_mul(8) else {
            return false;
        };
        if bits.size() > capacity {
            return false;
        }
        for _ in 0..4 {
            if bits.size() >= capacity {
                break;
            }
            bits.append_bit(0);
        }
        while bits.size() % 8 != 0 {
            bits.append_bit(0);
        }
        let Some(padding) = num_data_bytes.checked_sub(bits.size_in_bytes()) else {
            return false;
        };
        for i in 0..padding {
            bits.append_bits(if i % 2 == 0 { 0xec } else { 0x11 }, 8);
        }
        bits.size() == capacity
    }
    pub fn interleave_with_ec_bytes(
        bits: &CbcQrCoderBitVector,
        total: i32,
        data: i32,
        rs_blocks: i32,
        result: &mut CbcQrCoderBitVector,
    ) -> bool {
        if data < 0 || bits.size_in_bytes() != data as usize || rs_blocks <= 0 {
            return false;
        }
        let mut blocks = Vec::new();
        let mut offset: usize = 0;
        let mut max_data = 0;
        let mut max_ec = 0;
        for i in 0..rs_blocks {
            let Some((data_count, ec_count)) =
                Self::get_num_data_bytes_and_num_ec_bytes_for_block_id(total, data, rs_blocks, i)
            else {
                return false;
            };
            let Some(end) = offset.checked_add(data_count) else {
                return false;
            };
            let Some(data_bytes) = bits.get_array().get(offset..end) else {
                return false;
            };
            let Some(ec_bytes) = Self::generate_ec_bytes(data_bytes, ec_count) else {
                return false;
            };
            max_data = max_data.max(data_bytes.len());
            max_ec = max_ec.max(ec_bytes.len());
            blocks.push(QrCoderBlockPair {
                data: data_bytes.to_vec(),
                ecc: ec_bytes,
            });
            offset = end;
        }
        if offset != data as usize {
            return false;
        }
        for i in 0..max_data {
            for block in &blocks {
                if let Some(&byte) = block.data.get(i) {
                    result.append_bits(i32::from(byte), 8);
                }
            }
        }
        for i in 0..max_ec {
            for block in &blocks {
                if let Some(&byte) = block.ecc.get(i) {
                    result.append_bits(i32::from(byte), 8);
                }
            }
        }
        total >= 0 && result.size_in_bytes() == total as usize
    }
    fn choose_mask_pattern(
        bits: &CbcQrCoderBitVector,
        ec: CbcQrCoderErrorCorrectionLevel,
        version: i32,
        matrix: &mut CbcCommonByteMatrix,
    ) -> Option<i32> {
        // Use the full penalty range so a large symbol always receives a mask.
        let mut min_penalty = i32::MAX;
        let mut best = None;
        for mask in 0..CbcQrCoder::NUM_MASK_PATTERNS {
            if !CbcQrCoderMatrixUtil::build_matrix(bits, ec, version, mask, matrix) {
                return None;
            }
            let penalty = CbcQrCoderMaskUtil::calculate_mask_penalty(matrix);
            if penalty < min_penalty {
                min_penalty = penalty;
                best = Some(mask);
            }
        }
        best
    }
    pub fn encode(
        content: &[u32],
        ec: CbcQrCoderErrorCorrectionLevel,
        qr: &mut CbcQrCoder,
    ) -> bool {
        let utf8_data = utf8_encode(content);
        let mode = Self::choose_mode(&utf8_data);
        let mut data_bits = CbcQrCoderBitVector::new();
        if !Self::append_bytes(&utf8_data, mode, &mut data_bits)
            || !Self::init_qr_code(data_bits.size_in_bytes(), ec, qr)
        {
            return false;
        }
        let mut header_and_data = CbcQrCoderBitVector::new();
        header_and_data.append_bits(mode.get_bits(), 4);
        let num_letters = if mode == CbcQrCoderMode::Byte {
            data_bits.size_in_bytes()
        } else {
            content.len()
        };
        if !Self::append_length_info(num_letters, qr.version, mode, &mut header_and_data) {
            return false;
        }
        header_and_data.append_bit_vector(&data_bits);
        if !Self::terminate_bits(qr.num_data_bytes as usize, &mut header_and_data) {
            return false;
        }
        let mut final_bits = CbcQrCoderBitVector::new();
        if !Self::interleave_with_ec_bytes(
            &header_and_data,
            qr.num_total_bytes,
            qr.num_data_bytes,
            qr.num_rs_blocks,
            &mut final_bits,
        ) {
            return false;
        }
        let Some(mut matrix) =
            CbcCommonByteMatrix::new(qr.matrix_width as usize, qr.matrix_width as usize)
        else {
            return false;
        };
        let Some(mask) = Self::choose_mask_pattern(&final_bits, ec, qr.version, &mut matrix) else {
            return false;
        };
        qr.mask_pattern = mask;
        if !CbcQrCoderMatrixUtil::build_matrix(&final_bits, ec, qr.version, mask, &mut matrix) {
            return false;
        }
        qr.matrix = Some(matrix);
        qr.is_valid()
    }
}
