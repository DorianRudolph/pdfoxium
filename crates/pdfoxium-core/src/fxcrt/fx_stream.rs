// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcrt/fx_stream.h` and `.cpp`. Rust ownership replaces
//! PDFium's `Retainable` base; the stream methods preserve its behavior.

use super::fx_types::FxFileSize;

pub trait WriteStream {
    /// A zero-length write is a successful no-op for implementations.
    fn write_block(&mut self, data: &[u8]) -> bool;

    fn write_string(&mut self, value: &[u8]) -> bool {
        self.write_block(value)
    }

    fn write_byte(&mut self, byte: u8) -> bool {
        self.write_block(&[byte])
    }

    fn write_dword(&mut self, value: u32) -> bool {
        // Upstream passes uint32_t to FXSYS_itoa(int). On the supported
        // two's-complement targets, the upper half is formatted as negative.
        let mut buffer = [0; 32];
        self.write_block(super::fx_system::itoa(value as i32, &mut buffer, 10))
    }

    fn write_filesize(&mut self, size: FxFileSize) -> bool {
        let mut buffer = [0; 32];
        self.write_block(super::fx_system::i64toa(size, &mut buffer, 10))
    }
}

pub trait ArchiveStream: WriteStream {
    fn current_offset(&self) -> FxFileSize;
}

pub trait StreamWithSize {
    fn get_size(&self) -> FxFileSize;
}

pub trait SeekableWriteStream: StreamWithSize + WriteStream {
    fn flush(&mut self) -> bool;
}

pub trait SeekableReadStream: StreamWithSize {
    fn is_eof(&self) -> bool {
        false
    }

    fn get_position(&self) -> FxFileSize {
        0
    }

    fn read_block_at_offset(&self, buffer: &mut [u8], offset: FxFileSize) -> bool;
}

pub trait SeekableStream: SeekableReadStream + SeekableWriteStream {}
