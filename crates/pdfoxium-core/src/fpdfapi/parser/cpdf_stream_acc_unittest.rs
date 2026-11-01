// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_stream_acc_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CpdfRetainedStreamAcc;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize};
use std::rc::Rc;

// testing/invalid_seekable_read_stream.{h,cpp}: reads always fail.
struct InvalidSeekableReadStream {
    data_size: i64,
}
impl StreamWithSize for InvalidSeekableReadStream {
    fn get_size(&self) -> i64 {
        self.data_size
    }
}
impl SeekableReadStream for InvalidSeekableReadStream {
    fn read_block_at_offset(&self, _buffer: &mut [u8], _offset: i64) -> bool {
        false
    }
}

#[test]
fn ReadRawDataFailed() {
    let stream = retain_object(CpdfStream::from_file(
        Rc::new(InvalidSeekableReadStream { data_size: 1024 }),
        retain_object(CpdfDictionary::new()),
    ));
    // The retained accessor combines construction and LoadAllDataRaw().
    let stream_acc = CpdfRetainedStreamAcc::load_raw(stream).unwrap();
    assert!(stream_acc.get_span().is_empty());
}

// Regression test for crbug.com/1361849. The accessor must retain the stream
// after the caller releases its original reference.
#[test]
fn DataStreamLifeTime() {
    const kData: &[u8] = b"abc";
    let stream = retain_object(CpdfStream::from_bytes(kData));
    let stream_acc = CpdfRetainedStreamAcc::load_raw(Rc::clone(&stream)).unwrap();
    drop(stream);
    let span = stream_acc.get_span();
    assert_eq!(kData, &*span);
}
