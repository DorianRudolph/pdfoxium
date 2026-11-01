// Copyright 2017 The PDFium Authors
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

//! Direct port of `fxbarcode/qrcode/BC_QRCoderECBlockData.cpp` and its header.

#[derive(Clone, Copy, Debug)]
pub struct CbcQrCoderEcBlockData {
    pub ec_code_words_per_block: u8,
    pub count1: u8,
    pub data_code_words1: u8,
    pub count2: u8,
    pub data_code_words2: u8,
}
impl CbcQrCoderEcBlockData {
    const fn new(a: [u8; 5]) -> Self {
        Self {
            ec_code_words_per_block: a[0],
            count1: a[1],
            data_code_words1: a[2],
            count2: a[3],
            data_code_words2: a[4],
        }
    }
    pub fn get_num_blocks(&self) -> i32 {
        i32::from(self.count1) + i32::from(self.count2)
    }
    pub fn get_total_ec_code_words(&self) -> i32 {
        i32::from(self.ec_code_words_per_block) * self.get_num_blocks()
    }
    // Despite its upstream name, this includes data AND error correction words.
    pub fn get_total_data_code_words(&self) -> i32 {
        i32::from(self.count1)
            * (i32::from(self.data_code_words1) + i32::from(self.ec_code_words_per_block))
            + i32::from(self.count2)
                * (i32::from(self.data_code_words2) + i32::from(self.ec_code_words_per_block))
    }
}
pub type EcBlockDataRow = [CbcQrCoderEcBlockData; 4];
pub const QR_CODER_EC_BLOCK_DATA_TABLE: [EcBlockDataRow; 40] = [
    [
        CbcQrCoderEcBlockData::new([7, 1, 19, 0, 0]),
        CbcQrCoderEcBlockData::new([10, 1, 16, 0, 0]),
        CbcQrCoderEcBlockData::new([13, 1, 13, 0, 0]),
        CbcQrCoderEcBlockData::new([17, 1, 9, 0, 0]),
    ],
    [
        CbcQrCoderEcBlockData::new([10, 1, 34, 0, 0]),
        CbcQrCoderEcBlockData::new([16, 1, 28, 0, 0]),
        CbcQrCoderEcBlockData::new([22, 1, 22, 0, 0]),
        CbcQrCoderEcBlockData::new([28, 1, 16, 0, 0]),
    ],
    [
        CbcQrCoderEcBlockData::new([15, 1, 55, 0, 0]),
        CbcQrCoderEcBlockData::new([26, 1, 44, 0, 0]),
        CbcQrCoderEcBlockData::new([18, 2, 17, 0, 0]),
        CbcQrCoderEcBlockData::new([22, 2, 13, 0, 0]),
    ],
    [
        CbcQrCoderEcBlockData::new([20, 1, 80, 0, 0]),
        CbcQrCoderEcBlockData::new([18, 2, 32, 0, 0]),
        CbcQrCoderEcBlockData::new([26, 2, 24, 0, 0]),
        CbcQrCoderEcBlockData::new([16, 4, 9, 0, 0]),
    ],
    [
        CbcQrCoderEcBlockData::new([26, 1, 108, 0, 0]),
        CbcQrCoderEcBlockData::new([24, 2, 43, 0, 0]),
        CbcQrCoderEcBlockData::new([18, 2, 15, 2, 16]),
        CbcQrCoderEcBlockData::new([22, 2, 11, 2, 12]),
    ],
    [
        CbcQrCoderEcBlockData::new([18, 2, 68, 0, 0]),
        CbcQrCoderEcBlockData::new([16, 4, 27, 0, 0]),
        CbcQrCoderEcBlockData::new([24, 4, 19, 0, 0]),
        CbcQrCoderEcBlockData::new([28, 4, 15, 0, 0]),
    ],
    [
        CbcQrCoderEcBlockData::new([20, 2, 78, 0, 0]),
        CbcQrCoderEcBlockData::new([18, 4, 31, 0, 0]),
        CbcQrCoderEcBlockData::new([18, 2, 14, 4, 15]),
        CbcQrCoderEcBlockData::new([26, 4, 13, 1, 14]),
    ],
    [
        CbcQrCoderEcBlockData::new([24, 2, 97, 0, 0]),
        CbcQrCoderEcBlockData::new([22, 2, 38, 2, 39]),
        CbcQrCoderEcBlockData::new([22, 4, 18, 2, 19]),
        CbcQrCoderEcBlockData::new([26, 4, 14, 2, 15]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 2, 116, 0, 0]),
        CbcQrCoderEcBlockData::new([22, 3, 36, 2, 37]),
        CbcQrCoderEcBlockData::new([20, 4, 16, 4, 17]),
        CbcQrCoderEcBlockData::new([24, 4, 12, 4, 13]),
    ],
    [
        CbcQrCoderEcBlockData::new([18, 2, 68, 2, 69]),
        CbcQrCoderEcBlockData::new([26, 4, 43, 1, 44]),
        CbcQrCoderEcBlockData::new([24, 6, 19, 2, 20]),
        CbcQrCoderEcBlockData::new([28, 6, 15, 2, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([20, 4, 81, 0, 0]),
        CbcQrCoderEcBlockData::new([30, 1, 50, 4, 51]),
        CbcQrCoderEcBlockData::new([28, 4, 22, 4, 23]),
        CbcQrCoderEcBlockData::new([24, 3, 12, 8, 13]),
    ],
    [
        CbcQrCoderEcBlockData::new([24, 2, 92, 2, 93]),
        CbcQrCoderEcBlockData::new([22, 6, 36, 2, 37]),
        CbcQrCoderEcBlockData::new([26, 4, 20, 6, 21]),
        CbcQrCoderEcBlockData::new([28, 7, 14, 4, 15]),
    ],
    [
        CbcQrCoderEcBlockData::new([26, 4, 107, 0, 0]),
        CbcQrCoderEcBlockData::new([22, 8, 37, 1, 38]),
        CbcQrCoderEcBlockData::new([24, 8, 20, 4, 21]),
        CbcQrCoderEcBlockData::new([22, 12, 11, 4, 12]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 3, 115, 1, 116]),
        CbcQrCoderEcBlockData::new([24, 4, 40, 5, 41]),
        CbcQrCoderEcBlockData::new([20, 11, 16, 5, 17]),
        CbcQrCoderEcBlockData::new([24, 11, 12, 5, 13]),
    ],
    [
        CbcQrCoderEcBlockData::new([22, 5, 87, 1, 88]),
        CbcQrCoderEcBlockData::new([24, 5, 41, 5, 42]),
        CbcQrCoderEcBlockData::new([30, 5, 24, 7, 25]),
        CbcQrCoderEcBlockData::new([24, 11, 12, 7, 13]),
    ],
    [
        CbcQrCoderEcBlockData::new([24, 5, 98, 1, 99]),
        CbcQrCoderEcBlockData::new([28, 7, 45, 3, 46]),
        CbcQrCoderEcBlockData::new([24, 15, 19, 2, 20]),
        CbcQrCoderEcBlockData::new([30, 3, 15, 13, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([28, 1, 107, 5, 108]),
        CbcQrCoderEcBlockData::new([28, 10, 46, 1, 47]),
        CbcQrCoderEcBlockData::new([28, 1, 22, 15, 23]),
        CbcQrCoderEcBlockData::new([28, 2, 14, 17, 15]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 5, 120, 1, 121]),
        CbcQrCoderEcBlockData::new([26, 9, 43, 4, 44]),
        CbcQrCoderEcBlockData::new([28, 17, 22, 1, 23]),
        CbcQrCoderEcBlockData::new([28, 2, 14, 19, 15]),
    ],
    [
        CbcQrCoderEcBlockData::new([28, 3, 113, 4, 114]),
        CbcQrCoderEcBlockData::new([26, 3, 44, 11, 45]),
        CbcQrCoderEcBlockData::new([26, 17, 21, 4, 22]),
        CbcQrCoderEcBlockData::new([26, 9, 13, 16, 14]),
    ],
    [
        CbcQrCoderEcBlockData::new([28, 3, 107, 5, 108]),
        CbcQrCoderEcBlockData::new([26, 3, 41, 13, 42]),
        CbcQrCoderEcBlockData::new([30, 15, 24, 5, 25]),
        CbcQrCoderEcBlockData::new([28, 15, 15, 10, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([28, 4, 116, 4, 117]),
        CbcQrCoderEcBlockData::new([26, 17, 42, 0, 0]),
        CbcQrCoderEcBlockData::new([28, 17, 22, 6, 23]),
        CbcQrCoderEcBlockData::new([30, 19, 16, 6, 17]),
    ],
    [
        CbcQrCoderEcBlockData::new([28, 2, 111, 7, 112]),
        CbcQrCoderEcBlockData::new([28, 17, 46, 0, 0]),
        CbcQrCoderEcBlockData::new([30, 7, 24, 16, 25]),
        CbcQrCoderEcBlockData::new([24, 34, 13, 0, 0]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 4, 121, 5, 122]),
        CbcQrCoderEcBlockData::new([28, 4, 47, 14, 48]),
        CbcQrCoderEcBlockData::new([30, 11, 24, 14, 25]),
        CbcQrCoderEcBlockData::new([30, 16, 15, 14, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 6, 117, 4, 118]),
        CbcQrCoderEcBlockData::new([28, 6, 45, 14, 46]),
        CbcQrCoderEcBlockData::new([30, 11, 24, 16, 25]),
        CbcQrCoderEcBlockData::new([30, 30, 16, 2, 17]),
    ],
    [
        CbcQrCoderEcBlockData::new([26, 8, 106, 4, 107]),
        CbcQrCoderEcBlockData::new([28, 8, 47, 13, 48]),
        CbcQrCoderEcBlockData::new([30, 7, 24, 22, 25]),
        CbcQrCoderEcBlockData::new([30, 22, 15, 13, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([28, 10, 114, 2, 115]),
        CbcQrCoderEcBlockData::new([28, 19, 46, 4, 47]),
        CbcQrCoderEcBlockData::new([28, 28, 22, 6, 23]),
        CbcQrCoderEcBlockData::new([30, 33, 16, 4, 17]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 8, 122, 4, 123]),
        CbcQrCoderEcBlockData::new([28, 22, 45, 3, 46]),
        CbcQrCoderEcBlockData::new([30, 8, 23, 26, 24]),
        CbcQrCoderEcBlockData::new([30, 12, 15, 28, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 3, 117, 10, 118]),
        CbcQrCoderEcBlockData::new([28, 3, 45, 23, 46]),
        CbcQrCoderEcBlockData::new([30, 4, 24, 31, 25]),
        CbcQrCoderEcBlockData::new([30, 11, 15, 31, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 7, 116, 7, 117]),
        CbcQrCoderEcBlockData::new([28, 21, 45, 7, 46]),
        CbcQrCoderEcBlockData::new([30, 1, 23, 37, 24]),
        CbcQrCoderEcBlockData::new([30, 19, 15, 26, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 5, 115, 10, 116]),
        CbcQrCoderEcBlockData::new([28, 19, 47, 10, 48]),
        CbcQrCoderEcBlockData::new([30, 15, 24, 25, 25]),
        CbcQrCoderEcBlockData::new([30, 23, 15, 25, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 13, 115, 3, 116]),
        CbcQrCoderEcBlockData::new([28, 2, 46, 29, 47]),
        CbcQrCoderEcBlockData::new([30, 42, 24, 1, 25]),
        CbcQrCoderEcBlockData::new([30, 23, 15, 28, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 17, 115, 0, 0]),
        CbcQrCoderEcBlockData::new([28, 10, 46, 23, 47]),
        CbcQrCoderEcBlockData::new([30, 10, 24, 35, 25]),
        CbcQrCoderEcBlockData::new([30, 19, 15, 35, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 17, 115, 1, 116]),
        CbcQrCoderEcBlockData::new([28, 14, 46, 21, 47]),
        CbcQrCoderEcBlockData::new([30, 29, 24, 19, 25]),
        CbcQrCoderEcBlockData::new([30, 11, 15, 46, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 13, 115, 6, 116]),
        CbcQrCoderEcBlockData::new([28, 14, 46, 23, 47]),
        CbcQrCoderEcBlockData::new([30, 44, 24, 7, 25]),
        CbcQrCoderEcBlockData::new([30, 59, 16, 1, 17]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 12, 121, 7, 122]),
        CbcQrCoderEcBlockData::new([28, 12, 47, 26, 48]),
        CbcQrCoderEcBlockData::new([30, 39, 24, 14, 25]),
        CbcQrCoderEcBlockData::new([30, 22, 15, 41, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 6, 121, 14, 122]),
        CbcQrCoderEcBlockData::new([28, 6, 47, 34, 48]),
        CbcQrCoderEcBlockData::new([30, 46, 24, 10, 25]),
        CbcQrCoderEcBlockData::new([30, 2, 15, 64, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 17, 122, 4, 123]),
        CbcQrCoderEcBlockData::new([28, 29, 46, 14, 47]),
        CbcQrCoderEcBlockData::new([30, 49, 24, 10, 25]),
        CbcQrCoderEcBlockData::new([30, 24, 15, 46, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 4, 122, 18, 123]),
        CbcQrCoderEcBlockData::new([28, 13, 46, 32, 47]),
        CbcQrCoderEcBlockData::new([30, 48, 24, 14, 25]),
        CbcQrCoderEcBlockData::new([30, 42, 15, 32, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 20, 117, 4, 118]),
        CbcQrCoderEcBlockData::new([28, 40, 47, 7, 48]),
        CbcQrCoderEcBlockData::new([30, 43, 24, 22, 25]),
        CbcQrCoderEcBlockData::new([30, 10, 15, 67, 16]),
    ],
    [
        CbcQrCoderEcBlockData::new([30, 19, 118, 6, 119]),
        CbcQrCoderEcBlockData::new([28, 18, 47, 31, 48]),
        CbcQrCoderEcBlockData::new([30, 34, 24, 34, 25]),
        CbcQrCoderEcBlockData::new([30, 20, 15, 61, 16]),
    ],
];
