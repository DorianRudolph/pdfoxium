// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original code is licensed as follows:
// Copyright 2010, 2011 ZXing authors
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//! `BC_OnedEAN13Writer.{h,cpp}`.
use super::{
    bc_one_dim_writer::CbcOneDimWriter,
    bc_oned_ean_checksum::ean_calc_checksum,
    bc_oned_ean_writer::{check_content_validity, encode_ean, filter_digits},
};
use crate::{
    fxbarcode::bc_library::BcTextLoc, fxcrt::fx_coordinates::CfxMatrix,
    fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
};
pub struct CbcOnedEan13Writer {
    pub base: CbcOneDimWriter,
}
impl Default for CbcOnedEan13Writer {
    fn default() -> Self {
        let mut base = CbcOneDimWriter::default();
        base.data_length = 0;
        base.left_padding = true;
        base.right_padding = false;
        Self { base }
    }
}
impl CbcOnedEan13Writer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn check_content_validity(&self, contents: &[u32]) -> bool {
        check_content_validity(contents)
    }
    pub fn filter_contents(&self, contents: &[u32]) -> Vec<u32> {
        filter_digits(contents)
    }
    pub fn calc_checksum(contents: &[u8]) -> i32 {
        ean_calc_checksum(contents)
    }
    pub fn set_data_length(&mut self, length: i32) {
        self.base.data_length = length;
    }
    pub fn set_text_location(&mut self, location: BcTextLoc) {
        if location == BcTextLoc::BelowEmbed {
            self.base.text_location = location;
        }
    }
    pub fn encode(&mut self, contents: &[u8]) -> Vec<u8> {
        self.base.data_length = 13;
        encode_ean(contents, true)
    }
    pub fn show_chars(
        &self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
        contents: &[u32],
    ) -> bool {
        let scale = self.base.output_hscale;
        let width = (42. * scale) as i32 as f32;
        // The first digit occupies seven modules, as in UPC-A. The source
        // accidentally scales the preceding six-digit group width again.
        let first_width = 7. * scale;
        self.base.show_ean_chars(
            device,
            matrix,
            contents,
            &[(10., 51.5), (57., 98.5), (0., 6.5)],
            &[
                (1, 6, 10., width),
                (7, 6, 57., width),
                (0, 1, 0., first_width),
            ],
        )
    }
    pub fn render_device_result(
        &self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
        contents: &[u32],
    ) -> bool {
        self.base.render_bars(device, matrix)
            && (self.base.text_location == BcTextLoc::None
                || self.base.font.is_none()
                || contents.is_empty()
                || self.show_chars(device, matrix, contents))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Full native module strings are retained: exact black/white widths are part
    // of the barcode encoding contract, independent of rasterization.
    fn assert_modules(encoded: Vec<u8>, expected: &str) {
        assert_eq!(encoded.len(), expected.len());
        for (index, (actual, expected)) in encoded.iter().zip(expected.bytes()).enumerate() {
            assert_eq!(*actual != 0, expected != b' ', "module {index}");
        }
    }
    #[test]
    fn encode() {
        let mut writer = CbcOnedEan13Writer::new();
        assert!(writer.encode(b"").is_empty());
        assert!(writer.encode(b"123").is_empty());
        assert!(writer.encode(b"123456789012").is_empty());
        assert!(writer.encode(b"12345678901234").is_empty());
        assert_modules(writer.encode(b"1234567890128"),"# #  #  ## #### #  ### # ##   #    # #  #   # # # #  #   ### #  ###  # ##  ## ## ##  #  #   # #");
        assert_modules(writer.encode(b"7776665554440"),"# # ### ##  #   # # ####    # # # #### ###  # # # #  ### #  ### # ###  # ###  # ###  ###  # # #");
    }
    #[test]
    fn checksum() {
        assert_eq!(CbcOnedEan13Writer::calc_checksum(b""), 0);
        assert_eq!(CbcOnedEan13Writer::calc_checksum(b"123"), 6);
        assert_eq!(CbcOnedEan13Writer::calc_checksum(b"123456789012"), 8);
        assert_eq!(CbcOnedEan13Writer::calc_checksum(b"777666555444"), 0);
    }
}
