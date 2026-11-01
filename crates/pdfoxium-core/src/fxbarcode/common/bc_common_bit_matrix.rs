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

//! Direct `BC_CommonBitMatrix.{h,cpp}` packed-row storage. Logical coordinates
//! are bounded independently, so a malformed x cannot alias another row.
#[derive(Clone, Debug)]
pub struct CbcCommonBitMatrix {
    width: usize,
    height: usize,
    row_size: usize,
    bits: Vec<u32>,
}
impl CbcCommonBitMatrix {
    pub fn new(width: usize, height: usize) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let row_size = width.checked_add(31)? / 32;
        let length = row_size.checked_mul(height)?;
        // Preserve the source's strict packed-word allocation bound, while
        // checking zero dimensions and arithmetic before allocation.
        if length >= 1024 * 1024 * 1024 {
            return None;
        }
        let mut bits = Vec::new();
        bits.try_reserve_exact(length).ok()?;
        bits.resize(length, 0);
        Some(Self {
            width,
            height,
            row_size,
            bits,
        })
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn get_width(&self) -> usize {
        self.width
    }
    pub fn get_height(&self) -> usize {
        self.height
    }
    pub fn get(&self, x: usize, y: usize) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.bits[y * self.row_size + (x >> 5)] & (1 << (x & 31)) != 0
    }
    pub fn set(&mut self, x: usize, y: usize) {
        if x < self.width && y < self.height {
            self.bits[y * self.row_size + (x >> 5)] |= 1 << (x & 31);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_rows_cross_word_boundaries_without_aliasing() {
        let mut matrix = CbcCommonBitMatrix::new(33, 3).unwrap();
        for (x, y) in [(0, 0), (31, 0), (32, 1), (5, 2)] {
            matrix.set(x, y);
        }
        for y in 0..3 {
            for x in 0..33 {
                assert_eq!(
                    matrix.get(x, y),
                    [(0, 0), (31, 0), (32, 1), (5, 2)].contains(&(x, y))
                );
            }
        }
        matrix.set(64, 0);
        matrix.set(0, usize::MAX);
        assert!(!matrix.get(64, 0));
        assert!(!matrix.get(0, 1));
        assert!(!matrix.get(usize::MAX, usize::MAX));
        for (w, h) in [(0, 1), (1, 0), (usize::MAX, 1), (32, usize::MAX)] {
            assert!(CbcCommonBitMatrix::new(w, h).is_none());
        }
    }
}
