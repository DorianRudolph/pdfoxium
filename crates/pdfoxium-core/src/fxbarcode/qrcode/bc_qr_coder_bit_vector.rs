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

//! Direct port of `fxbarcode/qrcode/BC_QRCoderBitVector.cpp` and its header.

#[derive(Default, Clone, Debug)]
pub struct CbcQrCoderBitVector {
    array: Vec<u8>,
    size: usize,
}
impl CbcQrCoderBitVector {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn at(&self, index: usize) -> u8 {
        assert!(index < self.size);
        (self.array[index >> 3] >> (7 - (index & 7))) & 1
    }
    pub fn size(&self) -> usize {
        self.size
    }
    pub fn size_in_bytes(&self) -> usize {
        self.size.div_ceil(8)
    }
    pub fn get_array(&self) -> &[u8] {
        &self.array
    }
    pub fn append_bit(&mut self, bit: u8) {
        assert!(bit <= 1);
        if self.size & 7 == 0 {
            self.array.push(0);
        }
        self.array[self.size >> 3] |= bit << (7 - (self.size & 7));
        self.size += 1;
    }
    pub fn append_bits(&mut self, value: i32, num_bits: usize) {
        assert!((1..=32).contains(&num_bits));
        let mut remaining = num_bits;
        while remaining > 0 {
            if self.size & 7 == 0 && remaining >= 8 {
                self.array.push(((value as u32) >> (remaining - 8)) as u8);
                self.size += 8;
                remaining -= 8;
            } else {
                remaining -= 1;
                self.append_bit((((value as u32) >> remaining) & 1) as u8);
            }
        }
    }
    pub fn append_bit_vector(&mut self, other: &Self) {
        for i in 0..other.size {
            self.append_bit(other.at(i));
        }
    }
    pub fn xor(&mut self, other: &Self) -> bool {
        if self.size != other.size {
            return false;
        }
        for (a, b) in self.array.iter_mut().zip(&other.array) {
            *a ^= b;
        }
        true
    }
}
