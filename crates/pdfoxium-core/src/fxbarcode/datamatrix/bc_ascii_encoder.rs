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
    let digits = context.msg[context.pos..]
        .iter()
        .take_while(|ch| ch.is_ascii_digit())
        .count();
    if digits >= 2 {
        let a = context.current() - b'0';
        let b = context.msg[context.pos + 1] - b'0';
        context.codewords.push(a * 10 + b + 130);
        context.pos += 2;
    } else {
        let next = look_ahead_test(&context.msg, context.pos, Encoding::Ascii);
        if next != Encoding::Ascii {
            context.codewords.push(match next {
                Encoding::C40 => 230,
                Encoding::Text => 239,
                Encoding::X12 => 238,
                Encoding::Edifact => 240,
                Encoding::Base256 => 231,
                Encoding::Ascii => unreachable!(),
            });
            context.new_encoding = Some(next);
        } else {
            let ch = context.current();
            if ch >= 128 {
                context.codewords.extend_from_slice(&[235, ch - 128 + 1]);
            } else {
                context.codewords.push(ch + 1);
            }
            context.pos += 1;
        }
    }
    true
}
