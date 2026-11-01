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
//! `BC_OnedCodaBarWriter.{h,cpp}`.
use super::bc_one_dim_writer::{filter_contents, CbcOneDimWriter};
const ALPHABET: &[u8] = b"0123456789-$:/.+ABCDTN";
const ENCODING: [u8; 22] = [
    0x03, 0x06, 0x09, 0x60, 0x12, 0x42, 0x21, 0x24, 0x30, 0x48, 0x0c, 0x18, 0x45, 0x51, 0x54, 0x15,
    0x1A, 0x29, 0x0B, 0x0E, 0x1A, 0x29,
];
const DELIMITERS: &[u8] = b"ABCDTN*Eabcdtne";
pub struct CbcOnedCodaBarWriter {
    pub base: CbcOneDimWriter,
    pub start: u8,
    pub end: u8,
    pub wide_narrow_ratio: u8,
}
impl Default for CbcOnedCodaBarWriter {
    fn default() -> Self {
        Self {
            base: CbcOneDimWriter::default(),
            start: b'A',
            end: b'B',
            wide_narrow_ratio: 2,
        }
    }
}
impl CbcOnedCodaBarWriter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_start_char(&mut self, ch: u8) -> bool {
        if !DELIMITERS.contains(&ch) {
            return false;
        }
        self.start = ch;
        true
    }
    pub fn set_end_char(&mut self, ch: u8) -> bool {
        if !DELIMITERS.contains(&ch) {
            return false;
        }
        self.end = ch;
        true
    }
    pub fn set_data_length(&mut self, length: i32) {
        self.base.data_length = length.saturating_add(2);
    }
    pub fn set_wide_narrow_ratio(&mut self, ratio: i8) -> bool {
        if !(2..=3).contains(&ratio) {
            return false;
        }
        self.wide_narrow_ratio = ratio as u8;
        true
    }
    pub fn check_content_validity(&self, contents: &[u32]) -> bool {
        CbcOneDimWriter::has_valid_content_size(contents)
            && contents
                .iter()
                .all(|&ch| ch < 128 && b"0123456789-$/:+.".contains(&(ch as u8)))
    }
    pub fn filter_contents(&self, contents: &[u32]) -> Vec<u32> {
        filter_contents(contents, |ch, _| {
            (ch < 128
                && (b"0123456789-$/:+.".contains(&(ch as u8)) || DELIMITERS.contains(&(ch as u8))))
            .then_some(ch)
        })
    }
    pub fn encoded_contents(&self, contents: &[u32]) -> Vec<u32> {
        let mut data = Vec::with_capacity(contents.len() + 2);
        data.push(u32::from(self.start));
        data.extend_from_slice(contents);
        data.push(u32::from(self.end));
        data
    }
    pub fn encode(&mut self, contents: &[u8]) -> Vec<u8> {
        let mut data = Vec::with_capacity(contents.len() + 2);
        data.push(self.start);
        data.extend_from_slice(contents);
        data.push(self.end);
        self.base.content_len = data.len();
        let mut output = Vec::new();
        for (index, ch) in data.iter().enumerate() {
            let ch = match ch.to_ascii_uppercase() {
                b'T' => b'A',
                b'N' => b'B',
                b'*' => b'C',
                b'E' => b'D',
                ch => ch,
            };
            let Some(i) = ALPHABET.iter().position(|&value| value == ch) else {
                return Vec::new();
            };
            let code = ENCODING[i];
            let widths: [u8; 7] = std::array::from_fn(|bit| {
                if code & (1 << (6 - bit)) == 0 {
                    1
                } else {
                    self.wide_narrow_ratio
                }
            });
            CbcOneDimWriter::append_pattern(&mut output, &widths, true);
            if index + 1 < data.len() {
                output.push(0);
            }
        }
        output
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
        let mut writer = CbcOnedCodaBarWriter::new();
        assert_modules(writer.encode(b""), "# ##  #  # #  #  # ##");
        assert_modules(
            writer.encode(b"123"),
            "# ##  #  # # # ##  # # #  # ## ##  # # # #  #  # ##",
        );
        assert_modules(
            writer.encode(b"-$./:+"),
            "# ##  #  # # #  ## # # ##  # # ## ## ## # ## ## # ## ## # ## ## # ## ## ## #  #  # ##",
        );
        assert_modules(writer.encode(b"456.987987987/001"),"# ##  #  # # ## #  # ## # #  # #  # # ## ## ## ## # ## #  # # #  ## # # #  # ## # ## #  # # #  ## # # #  # ## # ## #  # # #  ## # # #  # ## # ## ## # ## # # #  ## # # #  ## # # ##  # #  #  # ##");
    }
    #[test]
    fn set_delimiters() {
        let mut writer = CbcOnedCodaBarWriter::new();
        assert_eq!(writer.set_start_char(b'A'), true);
        assert_eq!(writer.set_start_char(b'B'), true);
        assert_eq!(writer.set_start_char(b'C'), true);
        assert_eq!(writer.set_start_char(b'D'), true);
        assert_eq!(writer.set_start_char(b'E'), true);
        assert_eq!(writer.set_start_char(b'N'), true);
        assert_eq!(writer.set_start_char(b'T'), true);
        assert_eq!(writer.set_start_char(b'*'), true);
        assert_eq!(writer.set_start_char(b'V'), false);
        assert_eq!(writer.set_start_char(b'0'), false);
        assert_eq!(writer.set_start_char(b'\0'), false);
        assert_eq!(writer.set_start_char(b'@'), false);
        assert_eq!(writer.set_end_char(b'A'), true);
        assert_eq!(writer.set_end_char(b'B'), true);
        assert_eq!(writer.set_end_char(b'C'), true);
        assert_eq!(writer.set_end_char(b'D'), true);
        assert_eq!(writer.set_end_char(b'E'), true);
        assert_eq!(writer.set_end_char(b'N'), true);
        assert_eq!(writer.set_end_char(b'T'), true);
        assert_eq!(writer.set_end_char(b'*'), true);
        assert_eq!(writer.set_end_char(b'V'), false);
        assert_eq!(writer.set_end_char(b'0'), false);
        assert_eq!(writer.set_end_char(b'\0'), false);
        assert_eq!(writer.set_end_char(b'@'), false);
        assert_eq!(writer.set_start_char(b'N'), true);
        assert_eq!(writer.set_end_char(b'*'), true);
        assert_modules(
            writer.encode(b"987"),
            "#  #  # ## ## #  # # #  ## # # #  # ## # # #  #  ##",
        );
    }
}
