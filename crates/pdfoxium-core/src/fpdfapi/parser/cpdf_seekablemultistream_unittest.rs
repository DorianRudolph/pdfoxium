// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_seekablemultistream_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CpdfSeekableMultiStream;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxcrt::fx_stream::SeekableReadStream;

#[test]
fn NoStreams() {
    let fileread = CpdfSeekableMultiStream::new(Vec::new());
    let mut output_buffer = [0xbd; 16];
    assert!(!fileread.read_block_at_offset(&mut output_buffer[..0], 0));
    assert_eq!(output_buffer[0], 0xbd);
}

#[test]
fn EmptyStreams() {
    let streams = vec![retain_object(CpdfStream::from_dict(retain_object(
        CpdfDictionary::new(),
    )))];
    let fileread = CpdfSeekableMultiStream::new(streams);
    let mut output_buffer = [0xbd; 16];
    assert!(!fileread.read_block_at_offset(&mut output_buffer[..0], 0));
    assert_eq!(output_buffer[0], 0xbd);
}

#[test]
fn NormalStreams() {
    // 16 chars total.
    const kOne: &[u8] = b"one t";
    const kTwo: &[u8] = b"wo ";
    const kThree: &[u8] = b"three!!!";
    let stream1 = retain_object(CpdfStream::from_bytes(kOne));
    let stream2 = retain_object(CpdfStream::from_bytes(kTwo));
    let stream3 = retain_object(CpdfStream::from_bytes(kThree));
    let fileread = CpdfSeekableMultiStream::new(vec![stream1, stream2, stream3]);

    let mut output_buffer = [0xbd; 16];
    assert!(fileread.read_block_at_offset(&mut output_buffer[..0], 0));
    assert_eq!(output_buffer[0], 0xbd);

    output_buffer.fill(0xbd);
    assert!(fileread.read_block_at_offset(&mut output_buffer[..0], 1));
    assert_eq!(output_buffer[0], 0xbd);

    output_buffer.fill(0xbd);
    assert!(fileread.read_block_at_offset(&mut output_buffer[..1], 0));
    assert_eq!(&output_buffer[..1], b"o");
    assert_eq!(output_buffer[1], 0xbd);

    output_buffer.fill(0xbd);
    assert!(fileread.read_block_at_offset(&mut output_buffer, 0));
    assert_eq!(&output_buffer, b"one two three!!!");

    output_buffer.fill(0xbd);
    assert!(fileread.read_block_at_offset(&mut output_buffer[..10], 2));
    assert_eq!(&output_buffer[..10], b"e two thre");
    assert_eq!(output_buffer[11], 0xbd);

    output_buffer.fill(0xbd);
    assert!(!fileread.read_block_at_offset(&mut output_buffer, 1));
    assert_eq!(&output_buffer[..15], b"ne two three!!!");
    assert_eq!(output_buffer[15], 0xbd);
}
