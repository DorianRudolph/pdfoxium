// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/cfx_read_only_container_stream_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CfxReadOnlyContainerStream;
use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize};

#[test]
fn FromDataVector() {
    let data = Vec::from(&b"abcd"[..]);
    let stream = CfxReadOnlyContainerStream::new(data);
    assert_eq!(stream.get_size(), 4);
    let mut buffer = [0; 2];
    assert!(stream.read_block_at_offset(&mut buffer, 1));
    assert_eq!(buffer, *b"bc");
    assert_eq!(stream.span(), b"abcd");
}

#[test]
fn FromFixedSizeDataVector() {
    // Box<[u8]> is the selected fixed-size owner. Initialize every byte before
    // observation rather than exposing native uninitialized allocation.
    let mut data = vec![0; 4].into_boxed_slice();
    data[0] = b'e';
    data[1] = b'f';
    data[2] = b'g';
    data[3] = b'h';
    let stream = CfxReadOnlyContainerStream::new(data);
    assert_eq!(stream.get_size(), 4);
    let mut buffer = [0; 2];
    assert!(stream.read_block_at_offset(&mut buffer, 2));
    assert_eq!(buffer, *b"gh");
    assert_eq!(stream.span(), b"efgh");
}

#[test]
fn OutOfBoundsRead() {
    let data = Vec::from(&b"abc"[..]);
    let stream = CfxReadOnlyContainerStream::new(data);
    let mut buffer = [0; 2];
    assert!(!stream.read_block_at_offset(&mut buffer, 2));
    assert!(!stream.read_block_at_offset(&mut buffer, 3));
    assert!(!stream.read_block_at_offset(&mut buffer, 4));
}
