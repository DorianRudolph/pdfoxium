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

//! Direct `fxbarcode/cbc_pdf417i.{h,cpp}`.
use super::pdf417::bc_pdf417_writer::CbcPdf417Writer;
use crate::{
    fxcrt::fx_coordinates::CfxMatrix, fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
};

pub struct CbcPdf417I {
    pub writer: CbcPdf417Writer,
}
impl Default for CbcPdf417I {
    fn default() -> Self {
        Self::new()
    }
}
impl CbcPdf417I {
    pub fn new() -> Self {
        Self {
            writer: CbcPdf417Writer::new(),
        }
    }
    pub fn encode(&mut self, contents: &[u32]) -> bool {
        // Source guard applies before allocating the encoded representation.
        if contents.len() > 2710 {
            self.writer.base.clear_result();
            return false;
        }
        let result = self.writer.encode(contents);
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal() {
        assert!(CbcPdf417I::new().encode(&"Foo".chars().map(u32::from).collect::<Vec<_>>()));
    }
    #[test]
    fn max_length() {
        assert!(!CbcPdf417I::new().encode(&vec![u32::from('1'); 2711]));
    }
}
