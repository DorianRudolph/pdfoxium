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
pub(crate) fn encode(context: &mut CbcEncoderContext) -> bool {
    let mut buffer = Vec::with_capacity(context.remaining_characters() + 1);
    buffer.push(0);
    while context.has_more_characters() {
        buffer.push(context.current());
        context.pos += 1;
        let next = look_ahead_test(&context.msg, context.pos, Encoding::Base256);
        if next != Encoding::Base256 {
            context.new_encoding = Some(next);
            break;
        }
    }
    let count = buffer.len() - 1;
    let current_size = context.codewords.len() + count + 1;
    if !context.update_symbol_info(current_size) {
        return false;
    }
    if context.has_more_characters() || context.capacity() > current_size {
        if count <= 249 {
            buffer[0] = count as u8;
        } else if count <= 1555 {
            buffer[0] = (count / 250 + 249) as u8;
            buffer.insert(1, (count % 250) as u8);
        } else {
            return false;
        }
    }
    // A zero length means all remaining symbol data. The source's decimal-first-
    // digit conversion here produced an invalid length when the symbol was full.
    for ch in buffer {
        let pseudo = (149 * (context.codewords.len() + 1)) % 255 + 1;
        context
            .codewords
            .push(((usize::from(ch) + pseudo) % 256) as u8);
    }
    true
}
