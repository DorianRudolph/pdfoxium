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
//! `BC_OnedCode39Writer.{h,cpp}`.
use super::bc_one_dim_writer::{filter_contents, wide_to_utf8, CbcOneDimWriter};
const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-. *$/+%";
const CHECKSUM: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-. $/+%";
const ENCODING: [u16; 44] = [
    0x0034, 0x0121, 0x0061, 0x0160, 0x0031, 0x0130, 0x0070, 0x0025, 0x0124, 0x0064, 0x0109, 0x0049,
    0x0148, 0x0019, 0x0118, 0x0058, 0x000D, 0x010C, 0x004C, 0x001C, 0x0103, 0x0043, 0x0142, 0x0013,
    0x0112, 0x0052, 0x0007, 0x0106, 0x0046, 0x0016, 0x0181, 0x00C1, 0x01C0, 0x0091, 0x0190, 0x00D0,
    0x0085, 0x0184, 0x00C4, 0x0094, 0x00A8, 0x00A2, 0x008A, 0x002A,
];
pub struct CbcOnedCode39Writer {
    pub base: CbcOneDimWriter,
    pub wide_narrow_ratio: u8,
}
impl Default for CbcOnedCode39Writer {
    fn default() -> Self {
        Self {
            base: CbcOneDimWriter::default(),
            wide_narrow_ratio: 3,
        }
    }
}
impl CbcOnedCode39Writer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn check_content_validity(&self, contents: &[u32]) -> bool {
        CbcOneDimWriter::has_valid_content_size(contents)
            && contents
                .iter()
                .all(|&ch| ch < 128 && ALPHABET.contains(&(ch as u8)))
    }
    fn filter(&self, contents: &[u32], upper: bool) -> Vec<u32> {
        filter_contents(contents, |ch, i| {
            if ch == u32::from(b'*') && (i == 0 || i + 1 == contents.len()) {
                return None;
            }
            let uppercase = (ch as u8).to_ascii_uppercase();
            (ch < 128 && ALPHABET.contains(&uppercase)).then_some(if upper {
                u32::from(uppercase)
            } else {
                ch
            })
        })
    }
    pub fn filter_contents(&self, contents: &[u32]) -> Vec<u32> {
        self.filter(contents, true)
    }
    pub fn render_text_contents(&self, contents: &[u32]) -> Vec<u32> {
        self.filter(contents, false)
    }
    pub fn set_wide_narrow_ratio(&mut self, ratio: i8) -> bool {
        if !(2..=3).contains(&ratio) {
            return false;
        }
        self.wide_narrow_ratio = ratio as u8;
        true
    }
    pub fn calc_checksum(contents: &[u8]) -> Option<u8> {
        if contents.len() > 80 {
            return None;
        }
        let mut sum = 0;
        for &ch in contents {
            let index = ALPHABET.iter().position(|&value| value == ch)?;
            if ch != b'*' {
                sum += index;
            }
        }
        Some(CHECKSUM[sum % 43])
    }
    fn widths(&self, index: usize) -> [u8; 9] {
        std::array::from_fn(|bit| {
            if ENCODING[index] & (1 << bit) == 0 {
                1
            } else {
                self.wide_narrow_ratio
            }
        })
    }
    pub fn encode(&mut self, contents: &[u8]) -> Vec<u8> {
        let Some(checksum) = Self::calc_checksum(contents) else {
            return Vec::new();
        };
        let mut encoded = contents.to_vec();
        if self.base.calc_checksum {
            encoded.push(checksum);
        }
        self.base.content_len = encoded.len();
        let mut result = Vec::new();
        CbcOneDimWriter::append_pattern(&mut result, &self.widths(39), true);
        result.push(0);
        for &ch in encoded.iter().rev() {
            let Some(index) = ALPHABET.iter().position(|&value| value == ch) else {
                return Vec::new();
            };
            CbcOneDimWriter::append_pattern(&mut result, &self.widths(index), true);
            result.push(0);
        }
        CbcOneDimWriter::append_pattern(&mut result, &self.widths(39), true);
        result.reverse();
        result
    }
    pub fn encoded_contents(&self, contents: &[u32]) -> Option<Vec<u32>> {
        let mut result = contents.to_vec();
        if self.base.calc_checksum && self.base.print_checksum {
            let checksum = Self::calc_checksum(&wide_to_utf8(&self.filter_contents(contents)))?;
            result.push(u32::from(checksum));
        }
        Some(result)
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
    fn set_wide_narrow_ratio() {
        let mut writer = CbcOnedCode39Writer::new();
        assert_eq!(writer.set_wide_narrow_ratio(0), false);
        assert_eq!(writer.set_wide_narrow_ratio(1), false);
        assert_eq!(writer.set_wide_narrow_ratio(2), true);
        assert_eq!(writer.set_wide_narrow_ratio(3), true);
        assert_eq!(writer.set_wide_narrow_ratio(4), false);
        assert_eq!(writer.set_wide_narrow_ratio(100), false);
        assert_eq!(writer.set_wide_narrow_ratio(3), true);
        assert_modules(writer.encode(b"PDFIUM"),"#   # ### ### # # ### ### #   # # # ###   # ### # ### ###   # # # ### #   ### # ###   # # # ### ### ### # #   # #   # ### ### #");
        assert_eq!(writer.set_wide_narrow_ratio(2), true);
        assert_modules(writer.encode(b"PDFIUM"),"#  # ## ## # # ## ## #  # # # ##  # ## # ## ##  # # # ## #  ## # ##  # # # ## ## ## # #  # #  # ## ## #");
    }
    #[test]
    fn encode() {
        let mut writer = CbcOnedCode39Writer::new();
        assert_modules(writer.encode(b""), "#   # ### ### # #   # ### ### #");
        assert_modules(
            writer.encode(b"123"),
            "#   # ### ### # ### #   # # ### # ###   # # ### ### ###   # # # #   # ### ### #",
        );
        assert_modules(writer.encode(b"PDFIUM"),"#   # ### ### # # ### ### #   # # # ###   # ### # ### ###   # # # ### #   ### # ###   # # # ### ### ### # #   # #   # ### ### #");
        assert_modules(writer.encode(b"A -$%./+Z"),"#   # ### ### # ### # #   # ### #   ### # ### # #   # # ### ### #   #   #   # # # #   #   #   # ###   # # ### # #   #   # #   # #   # #   #   # #   ### ### # # #   # ### ### #");
    }
    #[test]
    fn checksum() {
        let mut writer = CbcOnedCode39Writer::new();
        writer.base.calc_checksum = true;
        assert_modules(writer.encode(b"123"),"#   # ### ### # ### #   # # ### # ###   # # ### ### ###   # # # # ###   ### # # #   # ### ### #");
        assert_modules(writer.encode(b"PDFIUM"),"#   # ### ### # # ### ### #   # # # ###   # ### # ### ###   # # # ### #   ### # ###   # # # ### ### ### # #   # ###   # # ### # #   # ### ### #");
    }
}
