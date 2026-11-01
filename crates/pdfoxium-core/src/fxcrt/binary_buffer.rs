// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcrt/binary_buffer.h` and `.cpp`. The byte length is
//! the base class length; a later wide-text wrapper can report character units.

#[derive(Default)]
pub struct BinaryBuffer {
    alloc_step: usize,
    data_size: usize,
    buffer: Vec<u8>,
}

impl BinaryBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mutable_span(&mut self) -> &mut [u8] {
        &mut self.buffer[..self.data_size]
    }

    pub fn span(&self) -> &[u8] {
        &self.buffer[..self.data_size]
    }

    pub fn is_empty(&self) -> bool {
        self.get_length() == 0
    }

    pub fn get_size(&self) -> usize {
        self.data_size
    }

    pub fn get_length(&self) -> usize {
        self.get_size()
    }

    pub fn clear(&mut self) {
        self.data_size = 0;
    }

    pub fn set_alloc_step(&mut self, step: usize) {
        self.alloc_step = step;
    }

    pub fn estimate_size(&mut self, size: usize) {
        if self.buffer.len() < size {
            self.expand_buf(size - self.get_size());
        }
    }

    pub fn append_span(&mut self, span: &[u8]) {
        if span.is_empty() {
            return;
        }
        self.expand_buf(span.len());
        let end = self.data_size + span.len();
        self.buffer[self.data_size..end].copy_from_slice(span);
        self.data_size = end;
    }

    pub fn append_string(&mut self, value: &[u8]) {
        self.append_span(value);
    }

    pub fn append_uint8(&mut self, value: u8) {
        self.append_span(&[value]);
    }

    pub fn append_uint16(&mut self, value: u16) {
        self.append_span(&value.to_ne_bytes());
    }

    pub fn append_uint32(&mut self, value: u32) {
        self.append_span(&value.to_ne_bytes());
    }

    pub fn append_double(&mut self, value: f64) {
        self.append_span(&value.to_ne_bytes());
    }

    pub fn detach_buffer(&mut self) -> Vec<u8> {
        self.buffer.truncate(self.data_size);
        self.data_size = 0;
        std::mem::take(&mut self.buffer)
    }

    pub(crate) fn expand_buf(&mut self, add_size: usize) {
        let mut new_size = self
            .data_size
            .checked_add(add_size)
            .expect("buffer size overflow");
        if self.buffer.len() >= new_size {
            return;
        }

        let alloc_step = 128.max(if self.alloc_step != 0 {
            self.alloc_step
        } else {
            self.buffer.len() / 4
        });
        // Keep the quantization operations separate, as in PDFium.
        new_size = new_size
            .checked_add(alloc_step - 1)
            .expect("buffer size overflow");
        new_size /= alloc_step;
        new_size = new_size
            .checked_mul(alloc_step)
            .expect("buffer size overflow");
        self.buffer.resize(new_size, 0);
    }

    // Protected upstream helper for the later WideTextBuffer translation.
    #[allow(dead_code)]
    pub(crate) fn delete_buf(&mut self, start_index: usize, count: usize) {
        if self.buffer.is_empty() || count > self.data_size || start_index > self.data_size - count
        {
            return;
        }
        self.buffer
            .copy_within(start_index + count..self.data_size, start_index);
        self.data_size -= count;
    }
}

#[cfg(test)]
mod tests {
    use super::BinaryBuffer;

    #[test]
    fn append_delete_and_detach_preserve_logical_length() {
        let mut buffer = BinaryBuffer::new();
        buffer.estimate_size(300);
        buffer.append_span(b"abcdef");
        buffer.delete_buf(2, 2);
        assert_eq!(buffer.span(), b"abef");
        assert_eq!(buffer.get_length(), 4);
        assert_eq!(buffer.detach_buffer(), b"abef");
        assert!(buffer.is_empty());
        buffer.append_string(b"xy");
        assert_eq!(buffer.span(), b"xy");
    }

    #[test]
    fn native_numeric_bytes_follow_pdfium_memory_order() {
        let mut buffer = BinaryBuffer::new();
        buffer.append_uint8(1);
        buffer.append_uint16(0x4321);
        buffer.append_uint32(0x8765_4321);
        buffer.append_double(1234.5678);
        let mut expected = vec![1];
        expected.extend_from_slice(&0x4321u16.to_ne_bytes());
        expected.extend_from_slice(&0x8765_4321u32.to_ne_bytes());
        expected.extend_from_slice(&1234.5678f64.to_ne_bytes());
        assert_eq!(buffer.span(), expected);
    }
}

#[cfg(test)]
#[path = "binary_buffer_unittest.rs"]
mod upstream_tests;
