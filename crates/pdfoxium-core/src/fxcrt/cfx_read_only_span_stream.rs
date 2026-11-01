// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2022 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcrt/cfx_read_only_span_stream.h` and `.cpp`.

use super::fx_stream::{SeekableReadStream, StreamWithSize};
use super::fx_types::FxFileSize;

pub struct CfxReadOnlySpanStream<'a> {
    span: &'a [u8],
}

impl<'a> CfxReadOnlySpanStream<'a> {
    pub fn new(span: &'a [u8]) -> Self {
        Self { span }
    }

    pub fn span(&self) -> &'a [u8] {
        self.span
    }
}

impl StreamWithSize for CfxReadOnlySpanStream<'_> {
    fn get_size(&self) -> FxFileSize {
        FxFileSize::try_from(self.span.len()).expect("stream size exceeds FX_FILESIZE")
    }
}

impl SeekableReadStream for CfxReadOnlySpanStream<'_> {
    fn read_block_at_offset(&self, buffer: &mut [u8], offset: FxFileSize) -> bool {
        read_span_at_offset(self.span, buffer, offset)
    }
}

pub(super) fn read_span_at_offset(span: &[u8], buffer: &mut [u8], offset: FxFileSize) -> bool {
    if buffer.is_empty() || offset < 0 {
        return false;
    }
    let Ok(pos) = usize::try_from(offset) else {
        return false;
    };
    let Some(end) = pos.checked_add(buffer.len()) else {
        return false;
    };
    let Some(source) = span.get(pos..end) else {
        return false;
    };
    buffer.copy_from_slice(source);
    true
}

#[cfg(test)]
mod tests {
    use super::CfxReadOnlySpanStream;
    use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize};

    #[test]
    fn bounds_and_empty_reads_match_pdfium() {
        let stream = CfxReadOnlySpanStream::new(b"abcd");
        assert_eq!(stream.get_size(), 4);
        assert_eq!(stream.span(), b"abcd");
        assert!(!stream.is_eof());
        assert_eq!(stream.get_position(), 0);
        let mut dest = [0xaa; 2];
        assert!(!stream.read_block_at_offset(&mut dest, -1));
        assert!(!stream.read_block_at_offset(&mut dest, 3));
        assert!(!stream.read_block_at_offset(&mut dest, i64::MAX));
        assert_eq!(dest, [0xaa; 2]);
        assert!(!stream.read_block_at_offset(&mut [], 0));
        assert!(stream.read_block_at_offset(&mut dest, 2));
        assert_eq!(dest, *b"cd");
    }
}
