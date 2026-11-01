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
//! Direct `BC_CommonByteMatrix.{h,cpp}`. Unassigned modules are 0xff, as in
//! source. Dimensions and allocation failures are checked at construction.
#[derive(Clone, Debug)]
pub struct CbcCommonByteMatrix {
    width: usize,
    height: usize,
    bytes: Vec<u8>,
}
impl CbcCommonByteMatrix {
    pub fn new(width: usize, height: usize) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let length = width.checked_mul(height)?;
        if length >= 256 * 1024 * 1024 {
            return None;
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length).ok()?;
        bytes.resize(length, 0xff);
        Some(Self {
            width,
            height,
            bytes,
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
    pub fn get_array(&self) -> &[u8] {
        &self.bytes
    }
    pub fn take_array(self) -> Vec<u8> {
        self.bytes
    }
    pub fn get(&self, x: usize, y: usize) -> u8 {
        assert!(
            x < self.width && y < self.height,
            "barcode matrix coordinate out of bounds"
        );
        self.bytes[y * self.width + x]
    }
    pub fn set(&mut self, x: usize, y: usize, value: u8) {
        assert!(
            x < self.width && y < self.height,
            "barcode matrix coordinate out of bounds"
        );
        self.bytes[y * self.width + x] = value;
    }
    pub fn fill(&mut self, value: u8) {
        self.bytes.fill(value);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_fill_row_order_replacement_and_owned_extraction() {
        let mut matrix = CbcCommonByteMatrix::new(3, 2).unwrap();
        assert_eq!(matrix.get_array(), &[255; 6]);
        matrix.set(2, 0, 1);
        matrix.set(0, 1, 0);
        assert_eq!(matrix.get_array(), &[255, 255, 1, 0, 255, 255]);
        assert_eq!(matrix.get(2, 0), 1);
        matrix.fill(7);
        assert_eq!(matrix.take_array(), [7; 6]);
        for (w, h) in [(0, 1), (1, 0), (usize::MAX, 2), (256 * 1024 * 1024, 1)] {
            assert!(CbcCommonByteMatrix::new(w, h).is_none());
        }
    }
}
