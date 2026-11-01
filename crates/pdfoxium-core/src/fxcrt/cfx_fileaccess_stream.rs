// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcrt/cfx_fileaccess_stream.h` and `.cpp`.

use super::fileaccess_iface::{FileAccessIface, StdFileAccess};
use super::fx_stream::{SeekableReadStream, SeekableWriteStream, StreamWithSize, WriteStream};
use super::fx_types::FxFileSize;
use std::cell::RefCell;

pub struct CfxFileAccessStream {
    file: RefCell<Box<dyn FileAccessIface>>,
}

impl CfxFileAccessStream {
    pub fn create_from_filename(filename: &str) -> Option<Self> {
        Self::create_from_path(std::path::Path::new(filename))
    }

    pub fn create_from_path(filename: &std::path::Path) -> Option<Self> {
        let mut file = Box::new(StdFileAccess::new());
        if !file.open_path(filename) {
            return None;
        }
        Some(Self::new(file))
    }

    pub fn new(file: Box<dyn FileAccessIface>) -> Self {
        Self {
            file: RefCell::new(file),
        }
    }
}

impl StreamWithSize for CfxFileAccessStream {
    fn get_size(&self) -> FxFileSize {
        self.file.borrow().get_size()
    }
}

impl SeekableReadStream for CfxFileAccessStream {
    fn is_eof(&self) -> bool {
        let mut file = self.file.borrow_mut();
        file.get_position() >= file.get_size()
    }

    fn get_position(&self) -> FxFileSize {
        self.file.borrow_mut().get_position()
    }

    fn read_block_at_offset(&self, buffer: &mut [u8], offset: FxFileSize) -> bool {
        self.file.borrow_mut().read_pos(buffer, offset) > 0
    }
}

impl WriteStream for CfxFileAccessStream {
    fn write_block(&mut self, buffer: &[u8]) -> bool {
        let file = self.file.get_mut();
        if file.set_position(file.get_size()) == -1 {
            return false;
        }
        file.write(buffer)
    }
}

impl SeekableWriteStream for CfxFileAccessStream {
    fn flush(&mut self) -> bool {
        self.file.get_mut().flush()
    }
}

impl super::fx_stream::SeekableStream for CfxFileAccessStream {}

#[cfg(test)]
mod tests {
    use super::CfxFileAccessStream;
    use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize};

    #[test]
    fn file_reads_accept_partial_data_and_track_position() {
        let path = std::env::temp_dir().join(format!(
            "pdfoxium-stream-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        std::fs::write(&path, b"abc").unwrap();
        let stream = CfxFileAccessStream::create_from_filename(path.to_str().unwrap()).unwrap();
        assert_eq!(stream.get_size(), 3);
        let mut buffer = [0xaa; 4];
        assert!(stream.read_block_at_offset(&mut buffer, 1));
        assert_eq!(buffer, [b'b', b'c', 0xaa, 0xaa]);
        assert_eq!(stream.get_position(), 3);
        assert!(stream.is_eof());
        assert!(!stream.read_block_at_offset(&mut buffer, 3));
        assert_eq!(stream.get_position(), 3);
        drop(stream);
        std::fs::remove_file(path).unwrap();
    }
}
