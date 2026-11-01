// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original code is licensed as follows:
/*
 * Copyright 2006-2007 Jeremias Maerki.
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

use super::bc_encoder::Encoding;
use super::bc_encoder_context::CbcEncoderContext;
pub(crate) fn encode(context: &mut CbcEncoderContext) -> bool {
    super::bc_c40_encoder::encode(context, Encoding::Text)
}
pub(crate) fn encode_char(ch: u8, buffer: &mut Vec<u8>) -> usize {
    match ch {
        b' ' => buffer.push(3),
        b'0'..=b'9' => buffer.push(ch - b'0' + 4),
        b'a'..=b'z' => buffer.push(ch - b'a' + 14),
        0..=31 => {
            buffer.extend_from_slice(&[0, ch]);
            return 2;
        }
        b'!'..=b'/' => {
            buffer.extend_from_slice(&[1, ch - b'!']);
            return 2;
        }
        b':'..=b'@' => {
            buffer.extend_from_slice(&[1, ch - b':' + 15]);
            return 2;
        }
        b'['..=b'_' => {
            buffer.extend_from_slice(&[1, ch - b'[' + 22]);
            return 2;
        }
        b'`' => {
            buffer.extend_from_slice(&[2, 0]);
            return 2;
        }
        b'A'..=b'Z' => {
            buffer.extend_from_slice(&[2, ch - b'A' + 1]);
            return 2;
        }
        b'{'..=127 => {
            buffer.extend_from_slice(&[2, ch - b'{' + 27]);
            return 2;
        }
        128..=255 => {
            buffer.extend_from_slice(&[1, 30]);
            return 2 + encode_char(ch - 128, buffer);
        }
    }
    1
}
