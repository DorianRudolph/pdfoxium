// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original code copyright 2006 Jeremias Maerki in part, and ZXing Authors in part.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.
//! Direct port of `BC_PDF417ErrorCorrection.{h,cpp}`.
use super::tables::EC_COEFFICIENTS;
pub struct CbcPdf417ErrorCorrection;
impl CbcPdf417ErrorCorrection {
    pub fn get_error_correction_codeword_count(level: i32) -> i32 {
        if !(0..=8).contains(&level) {
            -1
        } else {
            1 << (level + 1)
        }
    }
    pub fn generate_error_correction(data: &[u16], level: i32) -> Option<Vec<u16>> {
        let k = Self::get_error_correction_codeword_count(level);
        if k < 0 || data.iter().any(|&word| word >= 929) {
            return None;
        }
        let k = k as usize;
        let coefficients = EC_COEFFICIENTS[level as usize];
        let mut ech = vec![0u32; k];
        for &word in data {
            let t1 = (u32::from(word) + ech[k - 1]) % 929;
            for j in (1..k).rev() {
                let t2 = (t1 * u32::from(coefficients[j])) % 929;
                ech[j] = (ech[j - 1] + 929 - t2) % 929;
            }
            let t2 = (t1 * u32::from(coefficients[0])) % 929;
            ech[0] = (929 - t2) % 929;
        }
        Some(
            ech.into_iter()
                .rev()
                .map(|word| if word == 0 { 0 } else { (929 - word) as u16 })
                .collect(),
        )
    }
}
