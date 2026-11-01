// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
/// A reader for reading bits and bytes from a byte stream.
// PDFium compatibility branches: Copyright 2014 The PDFium Authors.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// These branches use the BSD-style license in ../LICENSE-PDFIUM.

#[derive(Debug, Clone)]
pub(crate) struct Reader<'a> {
    /// The underlying data.
    data: &'a [u8],
    /// The position in bits.
    cur_pos: usize,
    pdfium: bool,
}

impl<'a> Reader<'a> {
    pub(crate) fn new_pdfium(data: &'a [u8]) -> Self {
        Self {
            data,
            cur_pos: 0,
            pdfium: true,
        }
    }
    pub(crate) fn source_len(&self) -> usize {
        self.data.len()
    }

    pub(crate) fn is_pdfium(&self) -> bool {
        self.pdfium
    }

    #[inline(always)]
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            cur_pos: 0,
            pdfium: false,
        }
    }

    #[inline(always)]
    pub(crate) fn align(&mut self) {
        let bit_pos = self.bit_pos();

        if !bit_pos.is_multiple_of(8) {
            self.cur_pos += 8 - bit_pos;
        }
    }

    #[inline(always)]
    pub(crate) fn at_end(&self) -> bool {
        self.byte_pos() >= self.data.len()
    }

    #[inline(always)]
    pub(crate) fn tail(&self) -> Option<&'a [u8]> {
        self.data.get(self.byte_pos()..)
    }

    /// Read the given number of bytes.
    ///
    /// Assumes that the reader is currently byte-aligned.
    #[inline(always)]
    pub(crate) fn read_bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        debug_assert!(self.pdfium || self.bit_pos() == 0);

        let bytes = self.peek_bytes(len)?;
        self.cur_pos += len * 8;

        Some(bytes)
    }

    /// Read a single byte.
    ///
    /// Assumes that the reader is currently byte-aligned.
    #[inline(always)]
    pub(crate) fn read_byte(&mut self) -> Option<u8> {
        debug_assert!(self.pdfium || self.bit_pos() == 0);

        let byte = self.peek_byte()?;
        self.cur_pos += 8;

        Some(byte)
    }

    /// Read a single byte, returning `None` if it's zero.
    #[inline(always)]
    pub(crate) fn read_nonzero_byte(&mut self) -> Option<u8> {
        let byte = self.read_byte()?;

        if byte == 0 { None } else { Some(byte) }
    }

    /// Skip the given number of bytes.
    ///
    /// Assumes that the reader is currently byte-aligned.
    #[inline(always)]
    pub(crate) fn skip_bytes(&mut self, len: usize) -> Option<()> {
        debug_assert!(self.pdfium || self.bit_pos() == 0);

        self.read_bytes(len).map(|_| ())
    }

    /// Peek the given number of bytes.
    ///
    /// Assumes that the reader is currently byte-aligned.
    #[inline(always)]
    pub(crate) fn peek_bytes(&self, len: usize) -> Option<&'a [u8]> {
        debug_assert!(self.pdfium || self.bit_pos() == 0);

        let start = self.byte_pos();
        let end = start.checked_add(len)?;
        self.data.get(start..end)
    }

    /// Peek the next byte.
    ///
    /// Assumes that the reader is currently byte-aligned.
    #[inline(always)]
    pub(crate) fn peek_byte(&self) -> Option<u8> {
        debug_assert!(self.pdfium || self.bit_pos() == 0);

        self.data.get(self.byte_pos()).copied()
    }

    /// Read an u16 number.
    ///
    /// Assumes that the reader is currently byte-aligned.
    #[inline(always)]
    pub(crate) fn read_u16(&mut self) -> Option<u16> {
        Some(u16::from_be_bytes(self.read_bytes(2)?.try_into().ok()?))
    }

    /// Read an u32 number.
    ///
    /// Assumes that the reader is currently byte-aligned.
    #[inline(always)]
    pub(crate) fn read_u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.read_bytes(4)?.try_into().ok()?))
    }

    /// Read an i32 number.
    ///
    /// Assumes that the reader is currently byte-aligned.
    #[inline(always)]
    pub(crate) fn read_i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.read_bytes(4)?.try_into().ok()?))
    }

    #[inline(always)]
    pub(crate) fn read_bit(&mut self) -> Option<u8> {
        let byte = self.cur_byte()?;
        let shift = 7 - (self.bit_pos());
        self.cur_pos += 1;
        Some((byte >> shift) & 1)
    }

    #[inline(always)]
    pub(crate) fn read_bits(&mut self, count: u8) -> Option<u32> {
        // Native readNBits also accepts wider fields; the unsigned 32-bit
        // accumulator loses high bits as they shift out. Keep the strict
        // codec reader's original precondition outside PDFium mode.
        debug_assert!(self.pdfium || count <= 32);

        let mut value = 0_u32;
        // CJBig2_BitStream::readNBits fails only if the starting byte is
        // outside the stream. A truncated final field succeeds with all
        // remaining bits, without shifting in implicit zeros. Even a zero-bit
        // field requires an in-bounds starting byte.
        let mut remaining = if self.pdfium {
            self.cur_byte()?;
            usize::from(count).min(self.data.len() * 8 - self.cur_pos) as u8
        } else {
            count
        };

        while remaining > 0 {
            let bit_offset = self.bit_pos();
            let byte = self.cur_byte()? as u32;

            let available = (8 - bit_offset) as u8;
            let take = remaining.min(available);

            let shift = available - take;
            let mask = (1 << take) - 1;
            let bits = (byte >> shift) & mask;

            value = (value << take) | bits;
            self.cur_pos += take as usize;
            remaining -= take;
        }

        Some(value)
    }

    #[inline(always)]
    pub(crate) fn byte_pos(&self) -> usize {
        self.cur_pos >> 3
    }

    #[inline(always)]
    pub(crate) fn cur_byte(&self) -> Option<u8> {
        self.data.get(self.byte_pos()).copied()
    }

    #[inline(always)]
    pub(crate) fn bit_pos(&self) -> usize {
        self.cur_pos & 7
    }
}

#[cfg(test)]
mod tests {
    use super::Reader;

    #[test]
    fn pdfium_partial_fields_and_zero_bit_eof() {
        let mut reader = Reader::new_pdfium(&[0xb3]);
        assert_eq!(reader.read_bits(3), Some(5));
        assert_eq!(reader.read_bits(12), Some(0x13));
        assert_eq!(reader.byte_pos(), 1);
        assert_eq!(reader.read_bits(0), None);
        assert_eq!(reader.read_bits(1), None);
        let mut strict = Reader::new(&[0xb3]);
        assert_eq!(strict.read_bits(12), None);
        assert_eq!(Reader::new_pdfium(&[0]).read_bits(0), Some(0));
    }

    #[test]
    fn pdfium_byte_reads_preserve_bit_index() {
        let mut reader = Reader::new_pdfium(&[0x80, 0x61, 0x7f, 0x04]);
        assert_eq!(reader.read_bit(), Some(1));
        assert_eq!(reader.read_byte(), Some(0x80));
        assert_eq!(reader.bit_pos(), 1);
        assert_eq!(reader.read_u16(), Some(0x617f));
        assert_eq!(reader.bit_pos(), 1);
        reader.align();
        assert!(reader.at_end());
        assert_eq!(reader.read_bit(), None);
    }
}

#[cfg(test)]
#[path = "jbig2_bit_stream_unittest.rs"]
mod upstream_tests;
