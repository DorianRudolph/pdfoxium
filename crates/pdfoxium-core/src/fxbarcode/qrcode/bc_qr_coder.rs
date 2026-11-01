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

//! Direct port of `fxbarcode/qrcode/BC_QRCoder.cpp` and its header.

use super::bc_qr_coder_error_correction_level::CbcQrCoderErrorCorrectionLevel;
use crate::fxbarcode::common::bc_common_byte_matrix::CbcCommonByteMatrix;
pub struct CbcQrCoder {
    pub ec_level: Option<CbcQrCoderErrorCorrectionLevel>,
    pub version: i32,
    pub matrix_width: i32,
    pub mask_pattern: i32,
    pub num_total_bytes: i32,
    pub num_data_bytes: i32,
    pub num_ec_bytes: i32,
    pub num_rs_blocks: i32,
    pub matrix: Option<CbcCommonByteMatrix>,
}
impl Default for CbcQrCoder {
    fn default() -> Self {
        Self {
            ec_level: None,
            version: -1,
            matrix_width: -1,
            mask_pattern: -1,
            num_total_bytes: -1,
            num_data_bytes: -1,
            num_ec_bytes: -1,
            num_rs_blocks: -1,
            matrix: None,
        }
    }
}
impl CbcQrCoder {
    pub const NUM_MASK_PATTERNS: i32 = 8;
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_valid_mask_pattern(pattern: i32) -> bool {
        (0..Self::NUM_MASK_PATTERNS).contains(&pattern)
    }
    pub fn take_matrix(&mut self) -> Option<CbcCommonByteMatrix> {
        self.matrix.take()
    }
    pub fn is_valid(&self) -> bool {
        self.ec_level.is_some()
            && self.version != -1
            && self.matrix_width != -1
            && self.num_total_bytes != -1
            && self.num_data_bytes != -1
            && self.num_ec_bytes != -1
            && self.num_rs_blocks != -1
            && Self::is_valid_mask_pattern(self.mask_pattern)
            && self.num_data_bytes.checked_add(self.num_ec_bytes) == Some(self.num_total_bytes)
            && self.matrix.as_ref().is_some_and(|m| {
                m.width() == m.height() && i32::try_from(m.width()) == Ok(self.matrix_width)
            })
    }
}
