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

//! Direct port of `fxbarcode/qrcode/BC_QRCoderMode.cpp` and its header.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CbcQrCoderMode {
    Byte,
    Alphanumeric,
    Numeric,
}
impl CbcQrCoderMode {
    pub fn get_bits(self) -> i32 {
        match self {
            Self::Byte => 4,
            Self::Alphanumeric => 2,
            Self::Numeric => 1,
        }
    }
    pub fn get_character_count_bits(self, version: i32) -> usize {
        let i = if version <= 9 {
            0
        } else if version <= 26 {
            1
        } else {
            2
        };
        match self {
            Self::Byte => [8, 16, 16][i],
            Self::Alphanumeric => [9, 11, 13][i],
            Self::Numeric => [10, 12, 14][i],
        }
    }
}
