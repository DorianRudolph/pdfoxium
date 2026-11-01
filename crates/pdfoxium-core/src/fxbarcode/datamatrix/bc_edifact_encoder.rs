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
use super::bc_high_level_encoder::look_ahead_test;
fn encode_to_codewords(buffer: &[u8]) -> Vec<u8> {
    if buffer.is_empty() {
        return Vec::new();
    }
    let mut value = 0u32;
    for i in 0..4 {
        value += u32::from(buffer.get(i).copied().unwrap_or(0)) << (18 - 6 * i);
    }
    let bytes = [
        ((value >> 16) & 255) as u8,
        ((value >> 8) & 255) as u8,
        (value & 255) as u8,
    ];
    bytes[..buffer.len().min(3)].to_vec()
}
fn handle_eod(context: &mut CbcEncoderContext, buffer: &[u8]) -> bool {
    let count = buffer.len();
    if count == 0 {
        return true;
    }
    if count > 4 {
        return false;
    }
    if count == 1 {
        if !context.update_symbol_info(context.codewords.len()) {
            return false;
        }
        let available = context.capacity() - context.codewords.len();
        if context.remaining_characters() == 0 && available <= 2 {
            return true;
        }
    }
    let rest = count - 1;
    let encoded = encode_to_codewords(buffer);
    let mut rest_in_ascii = !context.has_more_characters() && rest <= 2;
    if rest <= 2 {
        if !context.update_symbol_info(context.codewords.len() + rest) {
            return false;
        }
        let available = context.capacity() - context.codewords.len();
        if available >= 3 {
            rest_in_ascii = false;
            if !context.update_symbol_info(context.codewords.len() + encoded.len()) {
                return false;
            }
        }
    }
    if rest_in_ascii {
        context.reset_symbol_info();
        let Some(pos) = context.pos.checked_sub(rest) else {
            return false;
        };
        context.pos = pos;
    } else {
        context.codewords.extend_from_slice(&encoded);
    }
    context.new_encoding = Some(Encoding::Ascii);
    true
}
pub(crate) fn encode(context: &mut CbcEncoderContext) -> bool {
    let mut buffer = Vec::new();
    while context.has_more_characters() {
        let ch = context.current();
        buffer.push(match ch {
            b' '..=b'?' => ch,
            b'@'..=b'^' => ch - 64,
            _ => return false,
        });
        context.pos += 1;
        if buffer.len() >= 4 {
            context
                .codewords
                .extend_from_slice(&encode_to_codewords(&buffer));
            buffer.drain(..4);
            let next = look_ahead_test(&context.msg, context.pos, Encoding::Edifact);
            if next != Encoding::Edifact {
                context.new_encoding = Some(Encoding::Ascii);
                break;
            }
        }
    }
    buffer.push(31);
    handle_eod(context, &buffer)
}
