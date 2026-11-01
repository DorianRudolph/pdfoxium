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
//! Direct port of `BC_PDF417HighLevelEncoder.{h,cpp}`.

const MIXED: [i8; 128] = [
    -1, -1, -1, -1, -1, -1, -1, -1, -1, 12, -1, -1, -1, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1, 26, -1, -1, 15, 18, 21, 10, -1, -1, -1, 22, 20, 13, 16, 17, 19,
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 14, -1, -1, 23, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 24, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1,
];
const PUNCTUATION: [i8; 128] = [
    -1, -1, -1, -1, -1, -1, -1, -1, -1, 12, 15, -1, -1, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, 10, 20, -1, 18, -1, -1, 28, 23, 24, 22, -1, 13, 16, 17, 19,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 14, 0, 1, -1, 2, 25, 3, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 4, 5, 6, -1, 7, 8, -1,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, 26, 21, 27, 9, -1,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EncodingMode {
    Unknown,
    Text,
    Byte,
    Numeric,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SubMode {
    Alpha,
    Lower,
    Mixed,
    Punctuation,
}

fn is_upper(ch: u32) -> bool {
    ch == 32 || (65..=90).contains(&ch)
}
fn is_lower(ch: u32) -> bool {
    ch == 32 || (97..=122).contains(&ch)
}
fn is_digit(ch: u32) -> bool {
    (48..=57).contains(&ch)
}
fn is_mixed(ch: u32) -> bool {
    ch < 128 && MIXED[ch as usize] >= 0
}
fn is_punctuation(ch: u32) -> bool {
    ch < 128 && PUNCTUATION[ch as usize] >= 0
}
fn is_text(ch: u32) -> bool {
    (32..=126).contains(&ch) || matches!(ch, 9 | 10 | 13)
}

pub struct CbcPdf417HighLevelEncoder;
impl CbcPdf417HighLevelEncoder {
    pub fn encode_high_level(msg: &[u32]) -> Option<Vec<u16>> {
        let mut utf8 = String::new();
        for &ch in msg {
            utf8.push(char::from_u32(ch)?);
        }
        let bytes = utf8.as_bytes();
        let result: Vec<u32> = bytes.iter().map(|&ch| u32::from(ch)).collect();
        let len = result.len();
        let mut sb = Vec::with_capacity(len);
        let mut p = 0;
        let mut text_submode = SubMode::Alpha;
        let mut encoding_mode = EncodingMode::Unknown;
        while p < len {
            let n = Self::determine_consecutive_digit_count(&result, p);
            if n >= 13 {
                sb.push(902);
                encoding_mode = EncodingMode::Numeric;
                text_submode = SubMode::Alpha;
                Self::encode_numeric(&result, p, n, &mut sb)?;
                p += n;
            } else {
                let t = Self::determine_consecutive_text_count(&result, p);
                if t >= 5 || n == len {
                    if encoding_mode != EncodingMode::Text {
                        sb.push(900);
                        encoding_mode = EncodingMode::Text;
                        text_submode = SubMode::Alpha;
                    }
                    text_submode = Self::encode_text(&result, p, t, text_submode, &mut sb)?;
                    p += t;
                } else {
                    let b = Self::determine_consecutive_binary_count(&result, bytes, p)?.max(1);
                    if b == 1 && encoding_mode == EncodingMode::Text {
                        Self::encode_binary(bytes, p, 1, EncodingMode::Text, &mut sb)?;
                    } else {
                        Self::encode_binary(bytes, p, b, encoding_mode, &mut sb)?;
                        encoding_mode = EncodingMode::Byte;
                        text_submode = SubMode::Alpha;
                    }
                    p += b;
                }
            }
        }
        Some(sb)
    }

    pub(super) fn encode_text(
        msg: &[u32],
        start: usize,
        count: usize,
        initial: SubMode,
        sb: &mut Vec<u16>,
    ) -> Option<SubMode> {
        let text = msg.get(start..start.checked_add(count)?)?;
        let mut tmp = Vec::<u16>::with_capacity(count);
        let mut submode = initial;
        let mut idx = 0;
        while idx < count {
            let ch = text[idx];
            match submode {
                SubMode::Alpha => {
                    if is_upper(ch) {
                        tmp.push(if ch == 32 { 26 } else { (ch - 65) as u16 });
                    } else if is_lower(ch) {
                        submode = SubMode::Lower;
                        tmp.push(27);
                        continue;
                    } else if is_mixed(ch) {
                        submode = SubMode::Mixed;
                        tmp.push(28);
                        continue;
                    } else if is_punctuation(ch) {
                        tmp.extend([29, PUNCTUATION[ch as usize] as u16]);
                    } else {
                        return None;
                    }
                }
                SubMode::Lower => {
                    if is_lower(ch) {
                        tmp.push(if ch == 32 { 26 } else { (ch - 97) as u16 });
                    } else if is_upper(ch) {
                        tmp.extend([27, (ch - 65) as u16]);
                    } else if is_mixed(ch) {
                        submode = SubMode::Mixed;
                        tmp.push(28);
                        continue;
                    } else if is_punctuation(ch) {
                        tmp.extend([29, PUNCTUATION[ch as usize] as u16]);
                    } else {
                        return None;
                    }
                }
                SubMode::Mixed => {
                    if is_mixed(ch) {
                        tmp.push(MIXED[ch as usize] as u16);
                    } else if is_upper(ch) {
                        submode = SubMode::Alpha;
                        tmp.push(28);
                        continue;
                    } else if is_lower(ch) {
                        submode = SubMode::Lower;
                        tmp.push(27);
                        continue;
                    }
                    // The lookahead is relative to the selected substring. Upstream
                    // compares the absolute start against count, missing nonzero offsets.
                    else if text.get(idx + 1).is_some_and(|&next| is_punctuation(next)) {
                        submode = SubMode::Punctuation;
                        tmp.push(25);
                        continue;
                    } else if is_punctuation(ch) {
                        tmp.extend([29, PUNCTUATION[ch as usize] as u16]);
                    } else {
                        return None;
                    }
                }
                SubMode::Punctuation => {
                    if is_punctuation(ch) {
                        tmp.push(PUNCTUATION[ch as usize] as u16);
                    } else {
                        submode = SubMode::Alpha;
                        tmp.push(29);
                        continue;
                    }
                }
            }
            idx += 1;
        }
        for pair in tmp.chunks(2) {
            sb.push(pair[0] * 30 + pair.get(1).copied().unwrap_or(29));
        }
        Some(submode)
    }

    pub(super) fn encode_binary(
        bytes: &[u8],
        start: usize,
        count: usize,
        mode: EncodingMode,
        sb: &mut Vec<u16>,
    ) -> Option<()> {
        let bytes = bytes.get(start..start.checked_add(count)?)?;
        if count == 1 && mode == EncodingMode::Text {
            sb.push(913);
        }
        let mut idx = 0;
        if count >= 6 {
            sb.push(924);
            while count - idx >= 6 {
                let mut t = 0u64;
                for &byte in &bytes[idx..idx + 6] {
                    t = (t << 8) + u64::from(byte);
                }
                let mut chars = [0u16; 5];
                for ch in &mut chars {
                    *ch = (t % 900) as u16;
                    t /= 900;
                }
                sb.extend(chars.into_iter().rev());
                idx += 6;
            }
        }
        if idx < count {
            sb.push(901);
        }
        sb.extend(bytes[idx..].iter().map(|&byte| u16::from(byte)));
        Some(())
    }

    pub(super) fn encode_numeric(
        msg: &[u32],
        start: usize,
        count: usize,
        sb: &mut Vec<u16>,
    ) -> Option<()> {
        let msg = msg.get(start..start.checked_add(count)?)?;
        if msg.iter().any(|&ch| !is_digit(ch)) {
            return None;
        }
        for chunk in msg.chunks(44) {
            // Exactly the source's arbitrary-precision '1'+digits / 900 operation,
            // using decimal long division for its bounded (45 digit) operand.
            let mut digits = Vec::with_capacity(chunk.len() + 1);
            digits.push(1u8);
            digits.extend(chunk.iter().map(|&ch| (ch - 48) as u8));
            let mut tmp = Vec::new();
            while !digits.is_empty() {
                let mut quotient = Vec::with_capacity(digits.len());
                let mut remainder = 0u16;
                for digit in digits {
                    let value = remainder * 10 + u16::from(digit);
                    let q = (value / 900) as u8;
                    remainder = value % 900;
                    if q != 0 || !quotient.is_empty() {
                        quotient.push(q);
                    }
                }
                tmp.push(remainder);
                digits = quotient;
            }
            sb.extend(tmp.into_iter().rev());
        }
        Some(())
    }

    pub(super) fn determine_consecutive_digit_count(msg: &[u32], start: usize) -> usize {
        msg.get(start..)
            .unwrap_or_default()
            .iter()
            .take_while(|&&ch| is_digit(ch))
            .count()
    }
    pub(super) fn determine_consecutive_text_count(msg: &[u32], start: usize) -> usize {
        let mut idx = start;
        while idx < msg.len() {
            let mut numeric_count = 0;
            while numeric_count < 13 && idx < msg.len() && is_digit(msg[idx]) {
                numeric_count += 1;
                idx += 1;
            }
            if numeric_count >= 13 {
                return idx - start - numeric_count;
            }
            if numeric_count > 0 {
                continue;
            }
            if !is_text(msg[idx]) {
                break;
            }
            idx += 1;
        }
        idx.saturating_sub(start)
    }
    pub(super) fn determine_consecutive_binary_count(
        msg: &[u32],
        bytes: &[u8],
        start: usize,
    ) -> Option<usize> {
        if bytes.len() < msg.len() || start > msg.len() {
            return None;
        }
        let mut idx = start;
        while idx < msg.len() {
            let mut ch = msg[idx];
            let mut numeric_count = 0;
            while numeric_count < 13 && is_digit(ch) {
                numeric_count += 1;
                let i = idx + numeric_count;
                if i >= msg.len() {
                    break;
                }
                ch = msg[i];
            }
            if numeric_count >= 13 {
                return Some(idx - start);
            }
            let mut text_count = 0;
            while text_count < 5 && is_text(ch) {
                text_count += 1;
                let i = idx + text_count;
                if i >= msg.len() {
                    break;
                }
                ch = msg[i];
            }
            if text_count >= 5 {
                return Some(idx - start);
            }
            if bytes[idx] == 63 && msg[idx] != 63 {
                return None;
            }
            idx += 1;
        }
        Some(idx - start)
    }
}
