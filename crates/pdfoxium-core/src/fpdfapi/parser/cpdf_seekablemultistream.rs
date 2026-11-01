// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/parser/cpdf_seekablemultistream.{h,cpp}`.

use super::cpdf_object::ObjectRef;
use super::cpdf_stream_acc::CpdfRetainedStreamAcc;
use crate::fxcrt::fx_stream::{
    SeekableReadStream, SeekableStream, SeekableWriteStream, StreamWithSize, WriteStream,
};
use crate::fxcrt::fx_types::FxFileSize;

pub struct CpdfSeekableMultiStream {
    data: Vec<CpdfRetainedStreamAcc>,
}

impl CpdfSeekableMultiStream {
    pub fn new(streams: Vec<ObjectRef>) -> Self {
        let mut data = Vec::new();
        for stream in streams {
            // The native constructor takes typed, non-null CPDF_Stream owners.
            // The retained accessor preserves the stream and filtered data
            // without copying its raw backing allocation.
            data.push(
                CpdfRetainedStreamAcc::load_filtered(stream)
                    .expect("CPDF_SeekableMultiStream requires PDF streams"),
            );
        }
        Self { data }
    }
}

impl StreamWithSize for CpdfSeekableMultiStream {
    fn get_size(&self) -> FxFileSize {
        let mut size: FxFileSize = 0;
        for acc in &self.data {
            // CPDF_StreamAcc::GetSize() narrows its span length to uint32_t.
            size = size
                .checked_add(FxFileSize::from(acc.get_span().len() as u32))
                .expect("multi-stream size exceeds FX_FILESIZE");
        }
        size
    }
}

impl SeekableReadStream for CpdfSeekableMultiStream {
    fn get_position(&self) -> FxFileSize {
        0
    }

    fn is_eof(&self) -> bool {
        false
    }

    fn read_block_at_offset(&self, mut buffer: &mut [u8], mut offset: FxFileSize) -> bool {
        let count = i32::try_from(self.data.len()).expect("multi-stream count exceeds int32_t");
        let mut index = 0;
        while index < count {
            let size = FxFileSize::from(self.data[index as usize].get_span().len() as u32);
            if offset < size {
                break;
            }

            offset -= size;
            index += 1;
        }
        while index < count {
            let acc_span = self.data[index as usize].get_span();
            // A negative offset reaches native span::subspan's bounds CHECK.
            let start = usize::try_from(offset).expect("negative multi-stream offset");
            let source = &acc_span[start..];
            let read = buffer.len().min(source.len());
            let (destination, remaining) = buffer.split_at_mut(read);
            destination.copy_from_slice(&source[..read]);
            buffer = remaining;
            if buffer.is_empty() {
                return true;
            }

            offset = 0;
            index += 1;
        }
        false
    }
}

impl WriteStream for CpdfSeekableMultiStream {
    fn write_block(&mut self, _buffer: &[u8]) -> bool {
        panic!("CPDF_SeekableMultiStream::WriteBlock is NOTREACHED")
    }
}

impl SeekableWriteStream for CpdfSeekableMultiStream {
    fn flush(&mut self) -> bool {
        panic!("CPDF_SeekableMultiStream::Flush is NOTREACHED")
    }
}

impl SeekableStream for CpdfSeekableMultiStream {}

#[cfg(test)]
#[path = "cpdf_seekablemultistream_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_name::CpdfName;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
    use crate::fxcodec::flate;
    use std::rc::Rc;

    #[test]
    fn retains_streams_and_reads_decoded_data_across_empty_segments() {
        let compressed = flate::encode(b"decoded");
        let source = CpdfStream::from_bytes(&compressed);
        source
            .get_dict()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(
                b"Filter",
                Some(retain_object(CpdfName::new(None, b"FlateDecode"))),
            );
        let stream = retain_object(source);
        let weak = Rc::downgrade(&stream);
        let multi = CpdfSeekableMultiStream::new(vec![
            retain_object(CpdfStream::from_bytes(b"")),
            stream,
            retain_object(CpdfStream::from_bytes(b"")),
            retain_object(CpdfStream::from_bytes(b" tail")),
            retain_object(CpdfStream::from_bytes(b"")),
        ]);
        assert!(weak.upgrade().is_some());
        assert_eq!(multi.get_size(), 12);
        let mut result = [0; 12];
        assert!(multi.read_block_at_offset(&mut result, 0));
        assert_eq!(&result, b"decoded tail");
        let mut result = [0xbd; 5];
        assert!(multi.read_block_at_offset(&mut result, 7));
        assert_eq!(&result, b" tail");
        assert!(!multi.read_block_at_offset(&mut [], 12));
        assert!(!multi.read_block_at_offset(&mut [], i64::MAX));
        assert_eq!(multi.get_position(), 0);
        assert!(!multi.is_eof());
        drop(multi);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn raw_stream_accessor_keeps_original_buffer() {
        let stream = retain_object(CpdfStream::from_bytes(b"original"));
        let source = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_in_memory_raw_data()
            .as_ptr();
        let multi = CpdfSeekableMultiStream::new(vec![stream]);
        assert_eq!(multi.data[0].get_span().as_ptr(), source);
    }

    #[test]
    #[should_panic(expected = "negative multi-stream offset")]
    fn negative_offset_preserves_native_span_check() {
        CpdfSeekableMultiStream::new(vec![retain_object(CpdfStream::from_bytes(b"a"))])
            .read_block_at_offset(&mut [], -1);
    }

    #[test]
    #[should_panic(expected = "WriteBlock is NOTREACHED")]
    fn write_is_unreachable_even_for_empty_input() {
        CpdfSeekableMultiStream::new(Vec::new()).write_block(&[]);
    }

    #[test]
    #[should_panic(expected = "Flush is NOTREACHED")]
    fn flush_is_unreachable() {
        CpdfSeekableMultiStream::new(Vec::new()).flush();
    }
}
