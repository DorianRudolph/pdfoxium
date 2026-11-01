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
//! `BC_OnedEANChecksum.{h,cpp}`.
pub fn ean_calc_checksum(contents: &[u8]) -> i32 {
    let (mut odd, mut even) = (0, 0);
    for (index, &ch) in contents.iter().rev().enumerate() {
        let digit = i32::from(ch) - i32::from(b'0');
        if index % 2 == 0 {
            odd += digit;
        } else {
            even += digit;
        }
    }
    (10 - (odd * 3 + even).rem_euclid(10)) % 10
}
