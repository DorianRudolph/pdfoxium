// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original code is licensed as follows:
/*
 * Copyright 2007 ZXing authors
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

//! Direct port of `fxbarcode/qrcode/BC_QRCoderVersion.cpp` and its header.

use super::{
    bc_qr_coder_ec_block_data::{
        CbcQrCoderEcBlockData, EcBlockDataRow, QR_CODER_EC_BLOCK_DATA_TABLE,
    },
    bc_qr_coder_error_correction_level::CbcQrCoderErrorCorrectionLevel,
};
#[derive(Clone, Copy, Debug)]
pub struct CbcQrCoderVersion {
    version_number: i32,
    total_code_words: i32,
    ec_blocks_array: EcBlockDataRow,
}
impl CbcQrCoderVersion {
    pub const MAX_VERSION: i32 = 40;
    pub fn new(version_number: i32, row: EcBlockDataRow) -> Self {
        Self {
            version_number,
            total_code_words: row[0].get_total_data_code_words(),
            ec_blocks_array: row,
        }
    }
    pub fn get_version_for_number(version: i32) -> Option<Self> {
        if !(1..=Self::MAX_VERSION).contains(&version) {
            return None;
        }
        Some(Self::new(
            version,
            QR_CODER_EC_BLOCK_DATA_TABLE[(version - 1) as usize],
        ))
    }
    pub fn get_version_number(&self) -> i32 {
        self.version_number
    }
    pub fn get_total_code_words(&self) -> i32 {
        self.total_code_words
    }
    pub fn get_dimension_for_version(&self) -> i32 {
        17 + 4 * self.version_number
    }
    pub fn get_ec_blocks_for_level(
        &self,
        level: CbcQrCoderErrorCorrectionLevel,
    ) -> &CbcQrCoderEcBlockData {
        &self.ec_blocks_array[level.ordinal()]
    }
}
