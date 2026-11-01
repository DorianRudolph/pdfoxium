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

//! Direct port of `fxbarcode/qrcode/BC_QRCodeWriter.cpp` and its header.

use super::{
    bc_qr_coder::CbcQrCoder, bc_qr_coder_encoder::CbcQrCoderEncoder,
    bc_qr_coder_error_correction_level::CbcQrCoderErrorCorrectionLevel,
};
use crate::fxbarcode::bc_two_dim_writer::{CbcTwoDimWriter, EncodeResult};
pub struct CbcQrCodeWriter {
    pub base: CbcTwoDimWriter,
}
impl Default for CbcQrCodeWriter {
    fn default() -> Self {
        Self::new()
    }
}
impl CbcQrCodeWriter {
    pub fn new() -> Self {
        Self {
            base: CbcTwoDimWriter::new(true),
        }
    }
    pub fn set_error_correction_level(&mut self, level: i32) -> bool {
        if !(0..=3).contains(&level) {
            return false;
        }
        self.base.set_correction_level(level);
        true
    }
    pub fn encode(&mut self, contents: &[u32], ec_level: i32) -> EncodeResult {
        let Some(ec) = CbcQrCoderErrorCorrectionLevel::from_ordinal(ec_level) else {
            return EncodeResult::default();
        };
        let mut qr = CbcQrCoder::new();
        if !CbcQrCoderEncoder::encode(contents, ec, &mut qr) {
            return EncodeResult::default();
        }
        let Some(matrix) = qr.take_matrix() else {
            return EncodeResult::default();
        };
        EncodeResult::new(matrix.take_array(), qr.matrix_width, qr.matrix_width)
    }
}
