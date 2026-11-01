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

//! Direct port of `fxbarcode/qrcode/BC_QRCoderErrorCorrectionLevel.cpp` and its header.

/// Immutable values replace the source's allocated process-global levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CbcQrCoderErrorCorrectionLevel {
    L,
    M,
    Q,
    H,
}
impl CbcQrCoderErrorCorrectionLevel {
    pub fn ordinal(self) -> usize {
        match self {
            Self::L => 0,
            Self::M => 1,
            Self::Q => 2,
            Self::H => 3,
        }
    }
    pub fn get_bits(self) -> i32 {
        match self {
            Self::L => 1,
            Self::M => 0,
            Self::Q => 3,
            Self::H => 2,
        }
    }
    pub fn from_ordinal(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::L),
            1 => Some(Self::M),
            2 => Some(Self::Q),
            3 => Some(Self::H),
            _ => None,
        }
    }
}
