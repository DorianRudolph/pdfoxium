// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/cfx_memorystream_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CfxMemoryStream;
use crate::fxcrt::fx_stream::{SeekableReadStream, WriteStream};

#[test]
fn ReadBlockAtOffset() {
    let mut stream = CfxMemoryStream::new();
    let kData1 = *b"abcdef";
    assert!(stream.write_block(&kData1));
    assert_eq!(stream.get_span(), b"abcdef");

    let mut buffer = [0; 3];
    assert!(stream.read_block_at_offset(&mut buffer, 2));
    assert_eq!(buffer, *b"cde");

    assert!(stream.read_block_at_offset(&mut buffer, 0));
    assert_eq!(buffer, *b"abc");

    assert!(stream.read_block_at_offset(&mut buffer, 3));
    assert_eq!(buffer, *b"def");

    assert!(!stream.read_block_at_offset(&mut buffer, 4));
    assert_eq!(buffer, *b"def");
}

#[test]
fn WriteBlock() {
    let mut stream = CfxMemoryStream::new();
    assert!(stream.write_block(&[]));
    assert!(stream.get_span().is_empty());

    let kData1 = *b"abc";
    assert!(stream.write_block(&kData1));
    assert_eq!(stream.get_span(), b"abc");

    assert!(stream.write_block(&kData1));
    assert_eq!(stream.get_span(), b"abcabc");

    assert!(stream.write_block(&[]));
    assert_eq!(stream.get_span(), b"abcabc");
}

#[test]
fn WriteZeroBytes() {
    let mut stream = CfxMemoryStream::new();
    let kData1 = *b"abc";
    assert!(stream.write_block(&kData1));
    assert_eq!(stream.get_span(), b"abc");

    assert!(stream.write_block(&[]));
    assert_eq!(stream.get_span(), b"abc");
}
