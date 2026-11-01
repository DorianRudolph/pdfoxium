// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Owned cursor buffer from `core/fxcodec/cfx_codec_memory.{h,cpp}`.
#[derive(Default, Debug)]
pub struct CfxCodecMemory {
    buffer: Vec<u8>,
    position: usize,
}
impl CfxCodecMemory {
    pub fn new(size: usize) -> Option<Self> {
        let mut result = Self::default();
        result.try_resize(size).then_some(result)
    }
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            buffer: bytes,
            position: 0,
        }
    }
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }
    pub fn size(&self) -> usize {
        self.buffer.len()
    }
    pub fn position(&self) -> usize {
        self.position
    }
    pub fn is_eof(&self) -> bool {
        self.position >= self.size()
    }
    pub fn unconsumed(&self) -> &[u8] {
        &self.buffer[self.position.min(self.size())..]
    }
    pub fn seek(&mut self, position: usize) -> bool {
        if position > self.size() {
            return false;
        }
        self.position = position;
        true
    }
    pub fn read_block(&mut self, output: &mut [u8]) -> usize {
        let count = output.len().min(self.unconsumed().len());
        output[..count].copy_from_slice(&self.unconsumed()[..count]);
        self.position += count;
        count
    }
    pub fn read_all_or_none(&mut self, output: &mut [u8]) -> bool {
        if output.len() > self.unconsumed().len() {
            return false;
        }
        self.read_block(output);
        true
    }
    pub fn try_resize(&mut self, size: usize) -> bool {
        if size > self.size() && self.buffer.try_reserve_exact(size - self.size()).is_err() {
            return false;
        }
        self.buffer.resize(size, 0);
        // A shrinking buffer must never leave an invalid Rust slice cursor.
        self.position = self.position.min(size);
        true
    }
    /// Move the remaining suffix down without changing the size or cursor.
    pub fn consume(&mut self, consumed: usize) -> bool {
        if consumed > self.size() {
            return false;
        }
        self.buffer.copy_within(consumed.., 0);
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_resize_and_overlap() {
        let mut memory = CfxCodecMemory::from_bytes(vec![0, 1, 2, 3, 4]);
        let mut bytes = [9; 3];
        assert!(memory.read_all_or_none(&mut bytes));
        assert_eq!(bytes, [0, 1, 2]);
        assert!(!memory.read_all_or_none(&mut bytes));
        assert_eq!(memory.position(), 3);
        assert_eq!(memory.read_block(&mut bytes), 2);
        assert_eq!(bytes, [3, 4, 2]);
        assert!(!memory.seek(6));
        assert!(memory.consume(2));
        assert_eq!(&memory.buffer()[..3], &[2, 3, 4]);
        assert!(memory.try_resize(2));
        assert!(memory.is_eof());
        assert!(memory.try_resize(7));
        assert_eq!(memory.unconsumed(), &[0, 0, 0, 0, 0]);
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;

    #[test]
    fn resizing_and_consuming_keep_the_cursor_contract_and_remaining_bytes() {
        let mut memory = CfxCodecMemory::new(5).unwrap();
        memory.buffer_mut().copy_from_slice(b"abcde");
        assert!(memory.seek(2));
        assert!(memory.consume(1));
        assert_eq!(memory.position(), 2);
        assert_eq!(memory.buffer(), b"bcdee");
        assert_eq!(memory.unconsumed(), b"dee");
        assert!(memory.try_resize(8));
        assert_eq!(memory.position(), 2);
        assert_eq!(memory.unconsumed(), b"dee\0\0\0");
        assert!(memory.try_resize(1));
        assert_eq!(memory.position(), 1);
        assert!(memory.is_eof());
        let mut output = [0xbd; 2];
        assert_eq!(memory.read_block(&mut output), 0);
        assert_eq!(output, [0xbd; 2]);
        assert!(memory.try_resize(0));
        assert_eq!(memory.position(), 0);
        assert!(memory.read_all_or_none(&mut []));
        assert_eq!(memory.read_block(&mut output), 0);
        assert!(memory.unconsumed().is_empty());
    }

    #[test]
    fn failed_bounds_and_capacity_requests_are_atomic() {
        let mut memory = CfxCodecMemory::from_bytes(b"abcd".to_vec());
        assert!(memory.seek(2));
        let before = memory.buffer().to_vec();
        assert!(!memory.consume(5));
        assert!(!memory.seek(5));
        // Vec rejects this capacity before allocation; no OOM is attempted.
        assert!(!memory.try_resize(usize::MAX));
        assert_eq!(memory.position(), 2);
        assert_eq!(memory.buffer(), before);
        let mut output = [0xbd; 3];
        assert!(!memory.read_all_or_none(&mut output));
        assert_eq!(output, [0xbd; 3]);
        assert_eq!(memory.position(), 2);
        assert!(memory.consume(0));
        assert_eq!(memory.buffer(), before);
        assert!(memory.consume(4));
        assert_eq!(memory.buffer(), before);
        assert_eq!(memory.position(), 2);
    }
}
