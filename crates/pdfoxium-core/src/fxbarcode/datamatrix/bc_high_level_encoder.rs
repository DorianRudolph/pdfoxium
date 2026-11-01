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

use super::bc_encoder::{self, Encoding};
use super::bc_encoder_context::CbcEncoderContext;
pub const C40_UNLATCH: u8 = 254;
pub fn encode_high_level(msg: &[u32]) -> Vec<u8> {
    if msg.len() > 3116 {
        return Vec::new();
    }
    let mut context = CbcEncoderContext::new(msg);
    // Macro headers and trailers are ASCII, so scalar and UTF-8 byte offsets agree.
    if msg.last() == Some(&4) && msg.len() >= 8 {
        let header = &msg[..6];
        if header == [91, 41, 62, 30, 48, 53] || header == [91, 41, 62, 30, 48, 54] {
            context
                .codewords
                .push(if header[5] == 53 { 236 } else { 237 });
            context.set_skip_at_end(2);
            context.pos = 6;
        }
    }
    let mut mode = Encoding::Ascii;
    while context.has_more_characters() {
        if !bc_encoder::encode(mode, &mut context) {
            return Vec::new();
        }
        if let Some(next) = context.new_encoding.take() {
            mode = next;
        }
    }
    let len = context.codewords.len();
    if !context.update_symbol_info(len) {
        return Vec::new();
    }
    let capacity = context.capacity();
    if len < capacity && mode != Encoding::Ascii && mode != Encoding::Base256 {
        context.codewords.push(C40_UNLATCH);
    }
    let mut codewords = context.codewords;
    if codewords.len() < capacity {
        codewords.push(129);
    }
    while codewords.len() < capacity {
        let pseudo = ((149 * (codewords.len() + 1)) % 253) + 1;
        let temp = 129 + pseudo;
        codewords.push(if temp <= 254 {
            temp as u8
        } else {
            (temp - 254) as u8
        });
    }
    codewords
}
fn minimums(counts: &[f32; 6]) -> ([i32; 6], [u8; 6], i32) {
    let mut ints = [0; 6];
    let mut mins = [0; 6];
    let mut minimum = i32::MAX;
    for i in 0..6 {
        ints[i] = counts[i].ceil() as i32;
        if minimum > ints[i] {
            minimum = ints[i];
            mins.fill(0);
        }
        if minimum == ints[i] {
            mins[i] += 1;
        }
    }
    (ints, mins, minimum)
}
fn native_c40(ch: u8) -> bool {
    ch == b' ' || ch.is_ascii_digit() || ch.is_ascii_uppercase()
}
fn native_text(ch: u8) -> bool {
    ch == b' ' || ch.is_ascii_digit() || ch.is_ascii_lowercase()
}
fn x12_term_sep(ch: u8) -> bool {
    matches!(ch, b'\r' | b'*' | b'>')
}
fn native_x12(ch: u8) -> bool {
    x12_term_sep(ch) || native_c40(ch)
}
pub(crate) fn look_ahead_test(msg: &[u8], start: usize, current: Encoding) -> Encoding {
    if start >= msg.len() {
        return current;
    }
    let mut counts: [f32; 6] = if current == Encoding::Ascii {
        [0., 1., 1., 1., 1., 1.25]
    } else {
        [1., 2., 2., 2., 2., 2.25]
    };
    counts[current as usize] = 0.;
    let mut processed = 0;
    loop {
        if start + processed == msg.len() {
            let (ints, mins, min) = minimums(&counts);
            if ints[0] == min {
                return Encoding::Ascii;
            }
            if mins.iter().sum::<u8>() == 1 {
                for mode in [
                    Encoding::Base256,
                    Encoding::Edifact,
                    Encoding::Text,
                    Encoding::X12,
                ] {
                    if mins[mode as usize] > 0 {
                        return mode;
                    }
                }
            }
            return Encoding::C40;
        }
        let ch = msg[start + processed];
        processed += 1;
        let extended = ch >= 128;
        counts[0] = if ch.is_ascii_digit() {
            counts[0] + 0.5
        } else {
            counts[0].ceil() + if extended { 2. } else { 1. }
        };
        counts[1] += if native_c40(ch) {
            2. / 3.
        } else if extended {
            8. / 3.
        } else {
            4. / 3.
        };
        counts[2] += if native_text(ch) {
            2. / 3.
        } else if extended {
            8. / 3.
        } else {
            4. / 3.
        };
        counts[3] += if native_x12(ch) {
            2. / 3.
        } else if extended {
            13. / 3.
        } else {
            10. / 3.
        };
        counts[4] += if (b' '..=b'^').contains(&ch) {
            3. / 4.
        } else if extended {
            17. / 4.
        } else {
            13. / 4.
        };
        counts[5] += 1.;
        if processed < 4 {
            continue;
        }
        let (ints, mins, _) = minimums(&counts);
        let [ascii, c40, text, x12, edifact, base256] = ints;
        let bet_min = base256.min(edifact).min(text);
        if ascii < bet_min && ascii < c40 && ascii < x12 {
            return Encoding::Ascii;
        }
        if base256 < ascii || mins[1..5].iter().sum::<u8>() == 0 {
            return Encoding::Base256;
        }
        if mins.iter().sum::<u8>() == 1 {
            for mode in [Encoding::Edifact, Encoding::Text, Encoding::X12] {
                if mins[mode as usize] > 0 {
                    return mode;
                }
            }
        }
        if c40 + 1 < ascii && c40 + 1 < bet_min {
            if c40 < x12 {
                return Encoding::C40;
            }
            if c40 == x12 {
                for &tc in msg.iter().skip(start + processed + 1) {
                    if x12_term_sep(tc) {
                        return Encoding::X12;
                    }
                    if !native_x12(tc) {
                        break;
                    }
                }
                return Encoding::C40;
            }
        }
    }
}
