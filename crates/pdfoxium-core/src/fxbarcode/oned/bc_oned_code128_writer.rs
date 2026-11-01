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
//! `BC_OnedCode128Writer.{h,cpp}`.
use super::bc_one_dim_writer::{filter_contents, CbcOneDimWriter};
use crate::fxbarcode::bc_library::BcType;
const CODE_PATTERN_TABLE: [[u8; 7]; 107] = [
    [2, 1, 2, 2, 2, 2, 0],
    [2, 2, 2, 1, 2, 2, 0],
    [2, 2, 2, 2, 2, 1, 0],
    [1, 2, 1, 2, 2, 3, 0],
    [1, 2, 1, 3, 2, 2, 0],
    [1, 3, 1, 2, 2, 2, 0],
    [1, 2, 2, 2, 1, 3, 0],
    [1, 2, 2, 3, 1, 2, 0],
    [1, 3, 2, 2, 1, 2, 0],
    [2, 2, 1, 2, 1, 3, 0],
    [2, 2, 1, 3, 1, 2, 0],
    [2, 3, 1, 2, 1, 2, 0],
    [1, 1, 2, 2, 3, 2, 0],
    [1, 2, 2, 1, 3, 2, 0],
    [1, 2, 2, 2, 3, 1, 0],
    [1, 1, 3, 2, 2, 2, 0],
    [1, 2, 3, 1, 2, 2, 0],
    [1, 2, 3, 2, 2, 1, 0],
    [2, 2, 3, 2, 1, 1, 0],
    [2, 2, 1, 1, 3, 2, 0],
    [2, 2, 1, 2, 3, 1, 0],
    [2, 1, 3, 2, 1, 2, 0],
    [2, 2, 3, 1, 1, 2, 0],
    [3, 1, 2, 1, 3, 1, 0],
    [3, 1, 1, 2, 2, 2, 0],
    [3, 2, 1, 1, 2, 2, 0],
    [3, 2, 1, 2, 2, 1, 0],
    [3, 1, 2, 2, 1, 2, 0],
    [3, 2, 2, 1, 1, 2, 0],
    [3, 2, 2, 2, 1, 1, 0],
    [2, 1, 2, 1, 2, 3, 0],
    [2, 1, 2, 3, 2, 1, 0],
    [2, 3, 2, 1, 2, 1, 0],
    [1, 1, 1, 3, 2, 3, 0],
    [1, 3, 1, 1, 2, 3, 0],
    [1, 3, 1, 3, 2, 1, 0],
    [1, 1, 2, 3, 1, 3, 0],
    [1, 3, 2, 1, 1, 3, 0],
    [1, 3, 2, 3, 1, 1, 0],
    [2, 1, 1, 3, 1, 3, 0],
    [2, 3, 1, 1, 1, 3, 0],
    [2, 3, 1, 3, 1, 1, 0],
    [1, 1, 2, 1, 3, 3, 0],
    [1, 1, 2, 3, 3, 1, 0],
    [1, 3, 2, 1, 3, 1, 0],
    [1, 1, 3, 1, 2, 3, 0],
    [1, 1, 3, 3, 2, 1, 0],
    [1, 3, 3, 1, 2, 1, 0],
    [3, 1, 3, 1, 2, 1, 0],
    [2, 1, 1, 3, 3, 1, 0],
    [2, 3, 1, 1, 3, 1, 0],
    [2, 1, 3, 1, 1, 3, 0],
    [2, 1, 3, 3, 1, 1, 0],
    [2, 1, 3, 1, 3, 1, 0],
    [3, 1, 1, 1, 2, 3, 0],
    [3, 1, 1, 3, 2, 1, 0],
    [3, 3, 1, 1, 2, 1, 0],
    [3, 1, 2, 1, 1, 3, 0],
    [3, 1, 2, 3, 1, 1, 0],
    [3, 3, 2, 1, 1, 1, 0],
    [3, 1, 4, 1, 1, 1, 0],
    [2, 2, 1, 4, 1, 1, 0],
    [4, 3, 1, 1, 1, 1, 0],
    [1, 1, 1, 2, 2, 4, 0],
    [1, 1, 1, 4, 2, 2, 0],
    [1, 2, 1, 1, 2, 4, 0],
    [1, 2, 1, 4, 2, 1, 0],
    [1, 4, 1, 1, 2, 2, 0],
    [1, 4, 1, 2, 2, 1, 0],
    [1, 1, 2, 2, 1, 4, 0],
    [1, 1, 2, 4, 1, 2, 0],
    [1, 2, 2, 1, 1, 4, 0],
    [1, 2, 2, 4, 1, 1, 0],
    [1, 4, 2, 1, 1, 2, 0],
    [1, 4, 2, 2, 1, 1, 0],
    [2, 4, 1, 2, 1, 1, 0],
    [2, 2, 1, 1, 1, 4, 0],
    [4, 1, 3, 1, 1, 1, 0],
    [2, 4, 1, 1, 1, 2, 0],
    [1, 3, 4, 1, 1, 1, 0],
    [1, 1, 1, 2, 4, 2, 0],
    [1, 2, 1, 1, 4, 2, 0],
    [1, 2, 1, 2, 4, 1, 0],
    [1, 1, 4, 2, 1, 2, 0],
    [1, 2, 4, 1, 1, 2, 0],
    [1, 2, 4, 2, 1, 1, 0],
    [4, 1, 1, 2, 1, 2, 0],
    [4, 2, 1, 1, 1, 2, 0],
    [4, 2, 1, 2, 1, 1, 0],
    [2, 1, 2, 1, 4, 1, 0],
    [2, 1, 4, 1, 2, 1, 0],
    [4, 1, 2, 1, 2, 1, 0],
    [1, 1, 1, 1, 4, 3, 0],
    [1, 1, 1, 3, 4, 1, 0],
    [1, 3, 1, 1, 4, 1, 0],
    [1, 1, 4, 1, 1, 3, 0],
    [1, 1, 4, 3, 1, 1, 0],
    [4, 1, 1, 1, 1, 3, 0],
    [4, 1, 1, 3, 1, 1, 0],
    [1, 1, 3, 1, 4, 1, 0],
    [1, 1, 4, 1, 3, 1, 0],
    [3, 1, 1, 1, 4, 1, 0],
    [4, 1, 1, 1, 3, 1, 0],
    [2, 1, 1, 4, 1, 2, 0],
    [2, 1, 1, 2, 1, 4, 0],
    [2, 1, 1, 2, 3, 2, 0],
    [2, 3, 3, 1, 1, 1, 2],
];
pub struct CbcOnedCode128Writer {
    pub base: CbcOneDimWriter,
    pub code_format: BcType,
}
impl CbcOnedCode128Writer {
    pub fn new(code_format: BcType) -> Self {
        Self {
            base: CbcOneDimWriter::default(),
            code_format,
        }
    }
    pub fn check_content_validity(&self, contents: &[u32]) -> bool {
        CbcOneDimWriter::has_valid_content_size(contents)
            && contents
                .iter()
                .all(|&ch| (32..=126).contains(&ch) && ch != 34)
    }
    pub fn filter_contents(&self, contents: &[u32]) -> Vec<u32> {
        let limit = if self.code_format == BcType::Code128B {
            126
        } else {
            106
        };
        filter_contents(contents, |ch, _| (32..=limit).contains(&ch).then_some(ch))
    }
    pub fn encode(&mut self, contents: &[u8]) -> Vec<u8> {
        if contents.is_empty() || contents.len() > 80 {
            return Vec::new();
        }
        let mut patterns = Vec::new();
        let checksum = if self.code_format == BcType::Code128B {
            Self::encode128_b(contents, &mut patterns)
        } else {
            Self::encode128_c(contents, &mut patterns)
        };
        patterns.push(checksum.rem_euclid(103));
        patterns.push(106);
        self.base.content_len = contents.len() + 3;
        let mut output = Vec::new();
        for pattern in patterns {
            let Some(row) = usize::try_from(pattern)
                .ok()
                .and_then(|i| CODE_PATTERN_TABLE.get(i))
            else {
                return Vec::new();
            };
            CbcOneDimWriter::append_pattern(&mut output, row, true);
        }
        output
    }
    pub fn encode128_b(contents: &[u8], patterns: &mut Vec<i32>) -> i32 {
        patterns.push(104);
        let mut sum = 104;
        for (index, &ch) in contents.iter().enumerate() {
            let pattern = i32::from(ch) - 32;
            patterns.push(pattern);
            sum += pattern * (index as i32 + 1);
        }
        sum
    }
    pub fn encode128_c(contents: &[u8], patterns: &mut Vec<i32>) -> i32 {
        patterns.push(105);
        let mut sum = 105;
        let mut position = 0;
        let mut weight = 1;
        while position < contents.len() {
            let ch = contents[position];
            let pattern;
            if ch.is_ascii_digit() {
                let mut value = i32::from(ch - b'0');
                position += 1;
                if let Some(&next) = contents.get(position) {
                    if next.is_ascii_digit() {
                        value = value * 10 + i32::from(next - b'0');
                        position += 1;
                    }
                }
                pattern = value;
            } else {
                pattern = i32::from(ch);
                position += 1;
            }
            patterns.push(pattern);
            sum += pattern * weight;
            weight += 1;
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encode128_b() {
        let cases: &[(&[u8], i32, &[i32])] = &[
            (b"", 104, &[104]),
            (b"a", 169, &[104, 65]),
            (b"1", 121, &[104, 17]),
            (b"a1", 203, &[104, 65, 17]),
            (b"ab", 301, &[104, 65, 66]),
            (b"12", 157, &[104, 17, 18]),
            (b"abc", 502, &[104, 65, 66, 67]),
            (b"123", 214, &[104, 17, 18, 19]),
            (b"abc123", 774, &[104, 65, 66, 67, 17, 18, 19]),
            (b"ABC123", 582, &[104, 33, 34, 35, 17, 18, 19]),
            (b"321ABC", 722, &[104, 19, 18, 17, 33, 34, 35]),
            (b"XYZ", 448, &[104, 56, 57, 58]),
        ];
        for &(text, expected_sum, expected_patterns) in cases {
            let mut patterns = Vec::new();
            assert_eq!(
                CbcOnedCode128Writer::encode128_b(text, &mut patterns),
                expected_sum
            );
            assert_eq!(patterns, expected_patterns);
        }
    }
    #[test]
    fn encode128_c() {
        let cases: &[(&[u8], i32, &[i32])] = &[
            (b"", 105, &[105]),
            (b"a", 202, &[105, 97]),
            (b"1", 106, &[105, 1]),
            (b"a1", 204, &[105, 97, 1]),
            (b"ab", 398, &[105, 97, 98]),
            (b"12", 117, &[105, 12]),
            (b"abc", 695, &[105, 97, 98, 99]),
            (b"123", 123, &[105, 12, 3]),
            (b"abc123", 758, &[105, 97, 98, 99, 12, 3]),
            (b"ABC123", 566, &[105, 65, 66, 67, 12, 3]),
            (b"321ABC", 933, &[105, 32, 1, 65, 66, 67]),
            (b"XYZ", 641, &[105, 88, 89, 90]),
        ];
        for &(text, expected_sum, expected_patterns) in cases {
            let mut patterns = Vec::new();
            assert_eq!(
                CbcOnedCode128Writer::encode128_c(text, &mut patterns),
                expected_sum
            );
            assert_eq!(patterns, expected_patterns);
        }
    }
    #[test]
    fn check_content_validity() {
        for kind in [BcType::Code128B, BcType::Code128C] {
            let writer = CbcOnedCode128Writer::new(kind);
            assert_eq!(writer.check_content_validity(&[102, 111, 111]), true);
            assert_eq!(writer.check_content_validity(&[120, 121, 122]), true);
            assert_eq!(writer.check_content_validity(&[]), false);
            assert_eq!(writer.check_content_validity(&[34]), false);
            assert_eq!(writer.check_content_validity(&[102, 16, 111, 111]), false);
            assert_eq!(writer.check_content_validity(&[98, 97, 114, 127]), false);
            assert_eq!(writer.check_content_validity(&[113, 117, 120, 136]), false);
        }
    }
    #[test]
    fn filter_contents() {
        let writer = CbcOnedCode128Writer::new(BcType::Code128B);
        assert!(writer.filter_contents(&[]).is_empty());
        assert_eq!(
            writer.filter_contents(&[102, 111, 111, 16]),
            [102, 111, 111]
        );
        assert_eq!(
            writer.filter_contents(&[102, 111, 111, 16, 108]),
            [102, 111, 111, 108]
        );
        assert_eq!(
            writer.filter_contents(&[102, 111, 111, 16, 127]),
            [102, 111, 111]
        );
        assert_eq!(
            writer.filter_contents(&[102, 111, 111, 16, 127, 136]),
            [102, 111, 111]
        );
        assert_eq!(
            writer.filter_contents(&[98, 97, 114, 16, 127, 136]),
            [98, 97, 114]
        );
        let writer = CbcOnedCode128Writer::new(BcType::Code128C);
        assert!(writer.filter_contents(&[]).is_empty());
        assert_eq!(writer.filter_contents(&[102, 111, 111, 16]), [102]);
        assert_eq!(writer.filter_contents(&[102, 111, 111, 16, 108]), [102]);
        assert_eq!(writer.filter_contents(&[102, 111, 111, 16, 127]), [102]);
        assert_eq!(
            writer.filter_contents(&[102, 111, 111, 16, 127, 136]),
            [102]
        );
        assert_eq!(
            writer.filter_contents(&[98, 97, 114, 16, 127, 136]),
            [98, 97]
        );
    }
}
