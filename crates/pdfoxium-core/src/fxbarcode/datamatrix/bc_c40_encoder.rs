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
use super::bc_high_level_encoder::{look_ahead_test, C40_UNLATCH};
pub(crate) fn encode_char(ch: u8, buffer: &mut Vec<u8>) -> usize {
    match ch {
        b' ' => buffer.push(3),
        b'0'..=b'9' => buffer.push(ch - b'0' + 4),
        b'A'..=b'Z' => buffer.push(ch - b'A' + 14),
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
        96..=127 => {
            buffer.extend_from_slice(&[2, ch - 96]);
            return 2;
        }
        128..=255 => {
            buffer.extend_from_slice(&[1, 30]);
            return 2 + encode_char(ch - 128, buffer);
        }
    }
    1
}
fn append_char(mode: Encoding, ch: u8, buffer: &mut Vec<u8>) -> usize {
    if mode == Encoding::Text {
        super::bc_text_encoder::encode_char(ch, buffer)
    } else {
        encode_char(ch, buffer)
    }
}
pub(crate) fn write_next_triplet(context: &mut CbcEncoderContext, buffer: &mut Vec<u8>) {
    let value = 1600 * u32::from(buffer[0]) + 40 * u32::from(buffer[1]) + u32::from(buffer[2]) + 1;
    context
        .codewords
        .extend_from_slice(&[(value / 256) as u8, (value % 256) as u8]);
    buffer.drain(..3);
}
fn backtrack_one_character(
    context: &mut CbcEncoderContext,
    buffer: &mut Vec<u8>,
    last_size: usize,
    mode: Encoding,
) -> Option<usize> {
    if context.pos == 0 || buffer.len() < last_size {
        return None;
    }
    buffer.truncate(buffer.len() - last_size);
    context.pos -= 1;
    let mut removed = Vec::new();
    let size = append_char(mode, context.current(), &mut removed);
    context.reset_symbol_info();
    Some(size)
}
pub(crate) fn encode(context: &mut CbcEncoderContext, mode: Encoding) -> bool {
    let mut buffer = Vec::new();
    while context.has_more_characters() {
        let ch = context.current();
        context.pos += 1;
        let mut last_size = append_char(mode, ch, &mut buffer);
        let unwritten = (buffer.len() / 3) * 2;
        let count = context.codewords.len() + unwritten;
        if !context.update_symbol_info(count) {
            return false;
        }
        let available = context.capacity() - count;
        if !context.has_more_characters() {
            if buffer.len() % 3 == 2 && available != 2 {
                let Some(size) = backtrack_one_character(context, &mut buffer, last_size, mode)
                else {
                    return false;
                };
                last_size = size;
            }
            while buffer.len() % 3 == 1 && ((last_size <= 3 && available != 1) || last_size > 3) {
                let Some(size) = backtrack_one_character(context, &mut buffer, last_size, mode)
                else {
                    return false;
                };
                last_size = size;
            }
            break;
        }
        if buffer.len() % 3 == 0 {
            let next = look_ahead_test(&context.msg, context.pos, mode);
            if next != mode {
                context.new_encoding = Some(next);
                break;
            }
        }
    }
    handle_eod(context, &mut buffer)
}
fn handle_eod(context: &mut CbcEncoderContext, buffer: &mut Vec<u8>) -> bool {
    let unwritten = (buffer.len() / 3) * 2;
    let rest = buffer.len() % 3;
    let count = context.codewords.len() + unwritten;
    if !context.update_symbol_info(count) {
        return false;
    }
    let available = context.capacity() - count;
    if rest == 2 {
        buffer.push(0);
        while buffer.len() >= 3 {
            write_next_triplet(context, buffer);
        }
        if context.has_more_characters() {
            context.codewords.push(C40_UNLATCH);
        }
    } else if rest == 1 && available == 1 {
        while buffer.len() >= 3 {
            write_next_triplet(context, buffer);
        }
        if context.has_more_characters() {
            context.codewords.push(C40_UNLATCH);
        }
        let Some(pos) = context.pos.checked_sub(1) else {
            return false;
        };
        context.pos = pos;
    } else if rest == 0 {
        while buffer.len() >= 3 {
            write_next_triplet(context, buffer);
        }
        if available > 0 || context.has_more_characters() {
            context.codewords.push(C40_UNLATCH);
        }
    } else {
        return false;
    }
    context.new_encoding = Some(Encoding::Ascii);
    true
}
