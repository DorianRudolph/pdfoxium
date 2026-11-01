// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `CLZWDecoder` from `core/fxcodec/flate/flatemodule.cpp`.

use crate::fxcodec::data_and_bytes_consumed::DataAndBytesConsumed;
use crate::fxcodec::image_predictors::INVALID_OFFSET;

struct LzwDecoder<'a> {
    src: &'a [u8],
    dest: Vec<u8>,
    src_bit_pos: u32,
    dest_byte_pos: u32,
    decode_stack: [u8; 4000],
    stack_len: usize,
    early_change: u32,
    code_len: u8,
    current_code: u32,
    codes: [u32; 5021],
}

impl<'a> LzwDecoder<'a> {
    fn new(src: &'a [u8], early_change: bool) -> Self {
        Self {
            src,
            dest: vec![0; 512],
            src_bit_pos: 0,
            dest_byte_pos: 0,
            decode_stack: [0; 4000],
            stack_len: 0,
            early_change: u32::from(early_change),
            code_len: 9,
            current_code: 0,
            codes: [0; 5021],
        }
    }

    fn add_code(&mut self, prefix_code: u32, append_char: u8) {
        if self.current_code + self.early_change == 4094 {
            return;
        }
        self.codes[self.current_code as usize] = (prefix_code << 16) | u32::from(append_char);
        self.current_code += 1;
        let count = self.current_code + self.early_change;
        if count == 512 - 258 {
            self.code_len = 10;
        } else if count == 1024 - 258 {
            self.code_len = 11;
        } else if count == 2048 - 258 {
            self.code_len = 12;
        }
    }

    fn decode_string(&mut self, mut code: u32) {
        loop {
            if code < 258 || code - 258 >= self.current_code {
                break;
            }
            let data = self.codes[(code - 258) as usize];
            if self.stack_len >= self.decode_stack.len() {
                return;
            }
            self.decode_stack[self.stack_len] = data as u8;
            self.stack_len += 1;
            code = data >> 16;
        }
        if self.stack_len < self.decode_stack.len() {
            self.decode_stack[self.stack_len] = code as u8;
            self.stack_len += 1;
        }
    }

    fn expand_dest_buf(&mut self, additional_size: usize) -> bool {
        let Some(new_size) = self
            .dest
            .len()
            .checked_add((self.dest.len() / 2).max(additional_size))
        else {
            self.dest.clear();
            return false;
        };
        self.dest.resize(new_size, 0);
        true
    }

    fn read_code(&mut self) -> Option<u32> {
        let end_bit = usize::try_from(self.src_bit_pos).ok()? + usize::from(self.code_len);
        if end_bit > self.src.len().checked_mul(8)? {
            return None;
        }
        let mut code = 0u32;
        for bit in usize::try_from(self.src_bit_pos).ok()?..end_bit {
            code = (code << 1) | u32::from((self.src[bit / 8] >> (7 - bit % 8)) & 1);
        }
        self.src_bit_pos = self.src_bit_pos.checked_add(u32::from(self.code_len))?;
        Some(code)
    }

    fn decode(&mut self) -> bool {
        let mut old_code = u32::MAX;
        let mut last_char = 0u8;
        while let Some(code) = self.read_code() {
            if code < 256 {
                let pos = self.dest_byte_pos as usize;
                if pos >= self.dest.len() && !self.expand_dest_buf(pos - self.dest.len() + 1) {
                    return false;
                }
                self.dest[pos] = code as u8;
                self.dest_byte_pos += 1;
                last_char = code as u8;
                if old_code != u32::MAX {
                    self.add_code(old_code, last_char);
                }
                old_code = code;
                continue;
            }
            if code == 256 {
                self.code_len = 9;
                self.current_code = 0;
                old_code = u32::MAX;
                continue;
            }
            if code == 257 {
                break;
            }
            if old_code == u32::MAX {
                return false;
            }

            self.stack_len = 0;
            if code - 258 >= self.current_code {
                self.decode_stack[self.stack_len] = last_char;
                self.stack_len += 1;
                self.decode_string(old_code);
            } else {
                self.decode_string(code);
            }
            let Some(required_size) = self.dest_byte_pos.checked_add(self.stack_len as u32) else {
                return false;
            };
            if required_size as usize > self.dest.len()
                && !self.expand_dest_buf(required_size as usize - self.dest.len())
            {
                return false;
            }
            for i in 0..self.stack_len {
                self.dest[self.dest_byte_pos as usize + i] =
                    self.decode_stack[self.stack_len - i - 1];
            }
            self.dest_byte_pos += self.stack_len as u32;
            last_char = self.decode_stack[self.stack_len - 1];
            if old_code >= 258 && old_code - 258 >= self.current_code {
                break;
            }
            self.add_code(old_code, last_char);
            old_code = code;
        }
        self.dest_byte_pos != 0
    }
}

pub fn lzw_decode(src: &[u8], early_change: bool) -> DataAndBytesConsumed {
    let mut decoder = LzwDecoder::new(src, early_change);
    if !decoder.decode() {
        return DataAndBytesConsumed::new(Vec::new(), INVALID_OFFSET);
    }
    decoder.dest.truncate(decoder.dest_byte_pos as usize);
    DataAndBytesConsumed::new(decoder.dest, decoder.src_bit_pos.div_ceil(8))
}

#[cfg(test)]
mod tests {
    use super::lzw_decode;
    use crate::fxcodec::image_predictors::INVALID_OFFSET;

    fn pack_codes(codes: &[(u32, u8)]) -> Vec<u8> {
        let mut result = Vec::new();
        let mut bit_count = 0;
        for &(code, width) in codes {
            for shift in (0..width).rev() {
                if bit_count % 8 == 0 {
                    result.push(0);
                }
                let last = result.len() - 1;
                result[last] |= (((code >> shift) & 1) as u8) << (7 - bit_count % 8);
                bit_count += 1;
            }
        }
        result
    }

    #[test]
    fn literals_dictionary_clear_and_end() {
        let src = pack_codes(&[(256, 9), (65, 9), (66, 9), (258, 9), (257, 9)]);
        let result = lzw_decode(&src, true);
        assert_eq!(result.data, b"ABAB");
        assert_eq!(result.bytes_consumed, 6);

        let src = pack_codes(&[(65, 9), (256, 9), (66, 9), (257, 9)]);
        assert_eq!(lzw_decode(&src, false).data, b"AB");
    }

    #[test]
    fn missing_first_literal_and_empty_output() {
        assert_eq!(
            lzw_decode(&pack_codes(&[(258, 9)]), true).bytes_consumed,
            INVALID_OFFSET
        );
        assert_eq!(
            lzw_decode(&pack_codes(&[(257, 9)]), true).bytes_consumed,
            INVALID_OFFSET
        );
    }

    #[test]
    fn early_change_moves_the_nine_to_ten_bit_boundary() {
        for (early_change, last_nine_bit_literal) in [(true, 254), (false, 255)] {
            let mut codes = vec![(256, 9)];
            for i in 1..=260 {
                codes.push((
                    u32::from(b'A'),
                    if i <= last_nine_bit_literal { 9 } else { 10 },
                ));
            }
            codes.push((257, 10));
            let src = pack_codes(&codes);
            let result = lzw_decode(&src, early_change);
            assert_eq!(result.data, vec![b'A'; 260]);
            assert_eq!(result.bytes_consumed, src.len() as u32);
        }
    }

    #[test]
    fn next_code_expands_previous_string() {
        let src = pack_codes(&[(256, 9), (65, 9), (258, 9), (257, 9)]);
        assert_eq!(lzw_decode(&src, true).data, b"AAA");
    }
}
