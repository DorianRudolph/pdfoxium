// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcrt/cfx_memorystream.{h,cpp}`.

use std::cell::Cell;

use super::fx_stream::{
    SeekableReadStream, SeekableStream, SeekableWriteStream, StreamWithSize, WriteStream,
};
use super::fx_types::FxFileSize;

#[derive(Default)]
pub struct CfxMemoryStream {
    data: Vec<u8>,
    cur_size: usize,
    // Reading changes the native stream position even through the existing
    // read-only stream trait. The data itself needs no interior mutability.
    cur_pos: Cell<usize>,
}

impl CfxMemoryStream {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_span(&self) -> &[u8] {
        &self.data[..self.cur_size]
    }
}

impl StreamWithSize for CfxMemoryStream {
    fn get_size(&self) -> FxFileSize {
        self.cur_size as FxFileSize
    }
}

impl SeekableReadStream for CfxMemoryStream {
    fn is_eof(&self) -> bool {
        self.cur_pos.get() >= self.cur_size
    }

    fn get_position(&self) -> FxFileSize {
        self.cur_pos.get() as FxFileSize
    }

    fn read_block_at_offset(&self, buffer: &mut [u8], offset: FxFileSize) -> bool {
        if buffer.is_empty() || offset < 0 {
            return false;
        }

        let Ok(offset) = usize::try_from(offset) else {
            return false;
        };
        let Some(new_pos) = buffer.len().checked_add(offset) else {
            return false;
        };
        if new_pos == 0 || new_pos > self.cur_size {
            return false;
        }

        self.cur_pos.set(new_pos);
        buffer.copy_from_slice(&self.get_span()[offset..new_pos]);
        true
    }
}

impl WriteStream for CfxMemoryStream {
    fn write_block(&mut self, buffer: &[u8]) -> bool {
        if buffer.is_empty() {
            return true;
        }

        let Some(new_pos) = buffer.len().checked_add(self.cur_size) else {
            return false;
        };
        if new_pos > self.data.len() {
            const BLOCK_SIZE: usize = 64 * 1024;
            let Some(new_size) = new_pos
                .checked_mul(2)
                .and_then(|size| size.checked_add(BLOCK_SIZE - 1))
                .map(|size| size / BLOCK_SIZE)
                .and_then(|size| size.checked_mul(BLOCK_SIZE))
            else {
                return false;
            };
            self.data.resize(new_size, 0);
        }
        self.cur_pos.set(new_pos);

        self.data[self.cur_size..new_pos].copy_from_slice(buffer);
        self.cur_size = self.cur_size.max(self.cur_pos.get());
        true
    }
}

impl SeekableWriteStream for CfxMemoryStream {
    fn flush(&mut self) -> bool {
        true
    }
}

impl SeekableStream for CfxMemoryStream {}

#[cfg(test)]
#[path = "cfx_memorystream_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_position_and_append_after_read_preserve_native_state() {
        let mut stream = CfxMemoryStream::new();
        assert_eq!(stream.get_size(), 0);
        assert_eq!(stream.get_position(), 0);
        assert!(stream.is_eof());
        assert!(stream.write_block(b"abcdef"));
        assert_eq!(stream.get_position(), 6);
        assert!(stream.is_eof());

        let mut buffer = [0xbd; 2];
        assert!(stream.read_block_at_offset(&mut buffer, 1));
        assert_eq!(buffer, *b"bc");
        assert_eq!(stream.get_position(), 3);
        assert!(!stream.is_eof());
        assert!(stream.write_block(&[]));
        assert_eq!(stream.get_position(), 3);
        for offset in [-1, 5, i64::MAX] {
            assert!(!stream.read_block_at_offset(&mut buffer, offset));
            assert_eq!(buffer, *b"bc");
            assert_eq!(stream.get_position(), 3);
        }
        assert!(!stream.read_block_at_offset(&mut [], 0));
        assert_eq!(stream.get_position(), 3);

        // Native WriteBlock appends at cur_size_, not at the read position.
        assert!(stream.write_block(b"gh"));
        assert_eq!(stream.get_span(), b"abcdefgh");
        assert_eq!(stream.get_position(), 8);
        assert!(stream.is_eof());
        assert!(stream.flush());
    }

    #[test]
    fn append_and_read_across_native_growth_boundary() {
        let mut stream = CfxMemoryStream::new();
        assert!(stream.write_block(&vec![0x31; 65_535]));
        assert!(stream.write_block(&vec![0xa7; 65_538]));
        let mut buffer = [0; 4];
        assert!(stream.read_block_at_offset(&mut buffer, 65_533));
        assert_eq!(buffer, [0x31, 0x31, 0xa7, 0xa7]);
        assert_eq!(stream.get_size(), 131_073);
        assert_eq!(stream.get_span()[131_072], 0xa7);
    }
}
