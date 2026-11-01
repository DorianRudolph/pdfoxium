// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fxcrt/cfx_read_only_container_stream.h` and `.cpp`.
//! The generic `AsRef<[u8]>` form covers PDFium's data vector, fixed-size data
//! vector, and byte string instantiations. The platform mapped-file form is
//! outside the portable Phase 1 configuration.

use super::cfx_read_only_span_stream::read_span_at_offset;
use super::fx_stream::{SeekableReadStream, StreamWithSize};
use super::fx_types::FxFileSize;

pub struct CfxReadOnlyContainerStream<T: AsRef<[u8]>> {
    data: T,
}

impl<T: AsRef<[u8]>> CfxReadOnlyContainerStream<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }

    pub fn span(&self) -> &[u8] {
        self.data.as_ref()
    }
}

impl<T: AsRef<[u8]>> StreamWithSize for CfxReadOnlyContainerStream<T> {
    fn get_size(&self) -> FxFileSize {
        FxFileSize::try_from(self.data.as_ref().len()).expect("stream size exceeds FX_FILESIZE")
    }
}

impl<T: AsRef<[u8]>> SeekableReadStream for CfxReadOnlyContainerStream<T> {
    fn read_block_at_offset(&self, buffer: &mut [u8], offset: FxFileSize) -> bool {
        read_span_at_offset(self.data.as_ref(), buffer, offset)
    }
}

#[cfg(test)]
mod tests {
    use super::CfxReadOnlyContainerStream;
    use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize};

    #[test]
    fn owns_data_and_reads_exact_range() {
        let stream = CfxReadOnlyContainerStream::new(Vec::from(&b"abcd"[..]));
        assert_eq!(stream.get_size(), 4);
        assert_eq!(stream.span(), b"abcd");
        let mut dest = [0xaa; 2];
        assert!(stream.read_block_at_offset(&mut dest, 1));
        assert_eq!(dest, *b"bc");
        assert!(!stream.read_block_at_offset(&mut dest, 3));
        assert_eq!(dest, *b"bc");
    }
}

#[cfg(test)]
#[path = "cfx_read_only_container_stream_unittest.rs"]
mod upstream_tests;
