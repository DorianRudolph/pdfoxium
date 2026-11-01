// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
/*
 * Copyright 2011 ZXing authors
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

//! Direct `fxbarcode/cbc_qrcode.{h,cpp}`.
use super::qrcode::bc_qr_code_writer::CbcQrCodeWriter;
use crate::{
    fxcrt::fx_coordinates::CfxMatrix, fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
};

pub struct CbcQrCode {
    pub writer: CbcQrCodeWriter,
}
impl Default for CbcQrCode {
    fn default() -> Self {
        Self::new()
    }
}
impl CbcQrCode {
    pub fn new() -> Self {
        Self {
            writer: CbcQrCodeWriter::new(),
        }
    }
    pub fn encode(&mut self, contents: &[u32]) -> bool {
        let result = self
            .writer
            .encode(contents, self.writer.base.error_correction_level());
        self.writer.base.render_result(&result)
    }
    pub fn render_device(
        &self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
    ) -> bool {
        self.writer.base.render_device_result(device, matrix)
    }
}
