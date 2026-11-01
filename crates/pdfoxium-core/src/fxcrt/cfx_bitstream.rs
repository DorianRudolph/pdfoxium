// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcrt/cfx_bitstream.h` and `.cpp`.

/// Reads big-endian bit fields from a borrowed byte span.
pub struct CfxBitStream<'a> {
    bit_pos: usize,
    bit_size: usize,
    data: &'a [u8],
}

impl<'a> CfxBitStream<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        // Check before multiplication so the length cannot wrap.
        assert!(data.len() <= usize::MAX / 8);
        Self {
            bit_pos: 0,
            bit_size: data.len() * 8,
            data,
        }
    }

    #[inline]
    pub fn byte_align(&mut self) {
        self.bit_pos = self.bit_pos.wrapping_add(7) & !7;
    }

    #[inline]
    pub fn is_eof(&self) -> bool {
        self.bit_pos >= self.bit_size
    }

    #[inline]
    pub fn pos(&self) -> usize {
        self.bit_pos
    }

    #[inline]
    pub fn skip_bits(&mut self, n_bits: usize) {
        self.bit_pos = self.bit_pos.wrapping_add(n_bits);
    }

    #[inline]
    pub fn rewind(&mut self) {
        self.bit_pos = 0;
    }

    #[inline]
    pub fn bits_remaining(&self) -> usize {
        self.bit_size.saturating_sub(self.bit_pos)
    }

    pub fn get_bits(&mut self, n_bits: u32) -> u32 {
        debug_assert!((1..=32).contains(&n_bits));
        if n_bits as usize > self.bit_size || self.bit_pos > self.bit_size - n_bits as usize {
            return 0;
        }

        let bit_pos = (self.bit_pos % 8) as u32;
        let mut byte_pos = self.bit_pos / 8;
        let current_byte = self.data[byte_pos];

        if n_bits == 1 {
            let bit = u32::from((current_byte & (1 << (7 - bit_pos))) != 0);
            self.bit_pos += 1;
            return bit;
        }

        let mut bit_left = n_bits;
        let mut result = 0;
        if bit_pos != 0 {
            let bits_readable = 8 - bit_pos;
            if bits_readable >= bit_left {
                result = u32::from(current_byte & (0xff >> bit_pos)) >> (bits_readable - bit_left);
                self.bit_pos += bit_left as usize;
                return result;
            }
            bit_left -= bits_readable;
            result = u32::from(current_byte & ((1 << bits_readable) - 1)) << bit_left;
            byte_pos += 1;
        }
        while bit_left >= 8 {
            bit_left -= 8;
            result |= u32::from(self.data[byte_pos]) << bit_left;
            byte_pos += 1;
        }
        if bit_left != 0 {
            result |= u32::from(self.data[byte_pos]) >> (8 - bit_left);
        }
        self.bit_pos += n_bits as usize;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::CfxBitStream;

    #[test]
    fn reads_match_bitwise_reference_at_every_position_and_width() {
        let data = [0xde, 0x3f, 0xb1, 0x7c, 0x12, 0x9a, 0x04, 0x56];
        let mut stream = CfxBitStream::new(&data);
        for width in 1..=32 {
            for start in 0..=data.len() * 8 - width {
                stream.rewind();
                stream.skip_bits(start);
                let expected = (0..width).fold(0u32, |value, i| {
                    let pos = start + i;
                    (value << 1) | u32::from((data[pos / 8] >> (7 - pos % 8)) & 1)
                });
                assert_eq!(stream.get_bits(width as u32), expected);
                assert_eq!(stream.pos(), start + width);
            }
        }
    }

    #[test]
    fn failed_reads_and_alignment_preserve_pdfium_position_rules() {
        let mut stream = CfxBitStream::new(&[0x12, 0x34]);
        stream.skip_bits(13);
        assert_eq!(stream.get_bits(4), 0);
        assert_eq!(stream.pos(), 13);
        stream.byte_align();
        assert_eq!(stream.pos(), 16);
        stream.skip_bits(2);
        assert!(stream.is_eof());
        assert_eq!(stream.bits_remaining(), 0);
        assert_eq!(stream.get_bits(1), 0);
        assert_eq!(stream.pos(), 18);
        stream.rewind();
        assert_eq!(stream.get_bits(16), 0x1234);
    }
}

#[cfg(test)]
#[path = "cfx_bitstream_unittest.rs"]
mod upstream_tests;
