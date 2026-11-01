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
//! Shared EAN interface and tables from `BC_OnedEANWriter.{h,cpp}`.
//! The C++ base's InitEANWriter is empty; UPC-A's encoder delegates to EAN-13
//! directly, so no nullable initialization state is required in Rust.
use super::bc_one_dim_writer::{filter_contents, CbcOneDimWriter};
pub const L_PATTERN: [[u8; 4]; 10] = [
    [3, 2, 1, 1],
    [2, 2, 2, 1],
    [2, 1, 2, 2],
    [1, 4, 1, 1],
    [1, 1, 3, 2],
    [1, 2, 3, 1],
    [1, 1, 1, 4],
    [1, 3, 1, 2],
    [1, 2, 1, 3],
    [3, 1, 1, 2],
];
pub fn check_content_validity(contents: &[u32]) -> bool {
    CbcOneDimWriter::has_valid_content_size(contents)
        && contents.iter().all(|&ch| (48..=57).contains(&ch))
}
pub fn filter_digits(contents: &[u32]) -> Vec<u32> {
    filter_contents(contents, |ch, _| (48..=57).contains(&ch).then_some(ch))
}
pub fn encode_ean(contents: &[u8], ean13: bool) -> Vec<u8> {
    let length = if ean13 { 13 } else { 8 };
    if contents.len() != length || !contents.iter().all(u8::is_ascii_digit) {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(if ean13 { 95 } else { 67 });
    CbcOneDimWriter::append_pattern(&mut result, &[1, 1, 1], true);
    let parity = if ean13 {
        [0, 0xB, 0xD, 0xE, 0x13, 0x19, 0x1C, 0x15, 0x16, 0x1A][usize::from(contents[0] - b'0')]
    } else {
        0
    };
    let start = usize::from(ean13);
    let count = if ean13 { 6 } else { 4 };
    for i in 0..count {
        let mut widths = L_PATTERN[usize::from(contents[start + i] - b'0')];
        if ean13 && parity & (1 << (5 - i)) != 0 {
            widths.reverse();
        }
        CbcOneDimWriter::append_pattern(&mut result, &widths, false);
    }
    CbcOneDimWriter::append_pattern(&mut result, &[1, 1, 1, 1, 1], false);
    for &ch in &contents[start + count..] {
        CbcOneDimWriter::append_pattern(&mut result, &L_PATTERN[usize::from(ch - b'0')], true);
    }
    CbcOneDimWriter::append_pattern(&mut result, &[1, 1, 1], true);
    result
}
