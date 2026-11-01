// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
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

use super::bc_c40_encoder::write_next_triplet;
use super::bc_encoder::Encoding;
use super::bc_encoder_context::CbcEncoderContext;
use super::bc_high_level_encoder::{look_ahead_test, C40_UNLATCH};
pub(crate) fn encode(context: &mut CbcEncoderContext) -> bool {
    let mut buffer = Vec::new();
    while context.has_more_characters() {
        let ch = context.current();
        context.pos += 1;
        buffer.push(match ch {
            b'\r' => 0,
            b'*' => 1,
            b'>' => 2,
            b' ' => 3,
            b'0'..=b'9' => ch - b'0' + 4,
            b'A'..=b'Z' => ch - b'A' + 14,
            _ => return false,
        });
        if buffer.len() % 3 == 0 {
            write_next_triplet(context, &mut buffer);
            let next = look_ahead_test(&context.msg, context.pos, Encoding::X12);
            if next != Encoding::X12 {
                context.new_encoding = Some(next);
                break;
            }
        }
    }
    if !context.update_symbol_info(context.codewords.len()) {
        return false;
    }
    let available = context.capacity() - context.codewords.len();
    match buffer.len() {
        2 => {
            context.codewords.push(C40_UNLATCH);
            let Some(pos) = context.pos.checked_sub(2) else {
                return false;
            };
            context.pos = pos;
            context.new_encoding = Some(Encoding::Ascii);
        }
        1 => {
            let Some(pos) = context.pos.checked_sub(1) else {
                return false;
            };
            context.pos = pos;
            if available > 1 {
                context.codewords.push(C40_UNLATCH);
            }
            context.new_encoding = Some(Encoding::Ascii);
        }
        0 => {}
        _ => return false,
    }
    true
}
