// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/edit/cpdf_stringarchivestream.{h,cpp}`. A borrowed byte vector
//! supplies the binary `ostringstream` sink without extending its lifetime.

use crate::fxcrt::fx_stream::{ArchiveStream, WriteStream};
use crate::fxcrt::fx_types::FxFileSize;

pub struct CpdfStringArchiveStream<'a> {
    stream: &'a mut Vec<u8>,
}

impl<'a> CpdfStringArchiveStream<'a> {
    pub fn new(stream: &'a mut Vec<u8>) -> Self {
        Self { stream }
    }
}

impl WriteStream for CpdfStringArchiveStream<'_> {
    fn write_block(&mut self, buffer: &[u8]) -> bool {
        self.stream.extend_from_slice(buffer);
        true
    }
}

impl ArchiveStream for CpdfStringArchiveStream<'_> {
    fn current_offset(&self) -> FxFileSize {
        panic!("CPDF_StringArchiveStream::CurrentOffset is NOTREACHED")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::{retain_object, PdfObject};
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn binary_writes_preserve_existing_content_and_empty_writes() {
        let mut buffer = b"prefix".to_vec();
        {
            let mut stream = CpdfStringArchiveStream::new(&mut buffer);
            assert!(stream.write_block(&[0, 0xff, b'\n']));
            assert!(stream.write_block(&[]));
            assert!(stream.write_block(b"suffix"));
        }
        assert_eq!(buffer, b"prefix\0\xff\nsuffix");
    }

    #[test]
    fn marked_content_dictionary_serializes_through_archive_stream() {
        // CPDF_PageContentGenerator::ProcessContentMarks writes a direct mark
        // dictionary through this adapter, then appends its BDC operator.
        let mut dictionary = CpdfDictionary::new();
        dictionary.set_for(
            b"MCID",
            Some(retain_object(CpdfScalarObject::number_from_signed(7))),
        );
        let mut buffer = Vec::new();
        assert!(dictionary.write_to(&mut CpdfStringArchiveStream::new(&mut buffer), None));
        buffer.extend_from_slice(b" BDC\n");
        assert_eq!(buffer, b"<</MCID 7>> BDC\n");
    }

    #[test]
    #[should_panic(expected = "CurrentOffset is NOTREACHED")]
    fn current_offset_is_an_upstream_unreachable_operation() {
        CpdfStringArchiveStream::new(&mut Vec::new()).current_offset();
    }
}
