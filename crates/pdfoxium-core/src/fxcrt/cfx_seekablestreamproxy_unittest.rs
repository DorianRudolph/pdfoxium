// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/cfx_seekablestreamproxy_unittest.cpp`.
#![allow(non_snake_case)]

use super::CfxSeekableStreamProxy;
use crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;
use std::rc::Rc;

#[test]
fn NullStream() {
    let mut proxy_stream = CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(&[])));
    let mut buffer = [0; 16];
    assert_eq!(proxy_stream.read_block(&mut buffer), 0);
}

#[test]
fn DefaultStreamBOMNotRecognized() {
    let data = b"abcd";
    let mut proxy_stream = CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(data)));
    let mut buffer = [0; 16];
    assert_eq!(proxy_stream.read_block(&mut buffer), 0);
}

#[test]
fn UTF8Stream() {
    let data = b"\xef\xbb\xbf*\xc2\xa2*";
    let mut proxy_stream = CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(data)));
    let mut buffer = [0; 16];
    assert_eq!(proxy_stream.read_block(&mut buffer), 3);
    assert_eq!(buffer[0], '*' as u32);
    assert_eq!(buffer[1], '\u{00a2}' as u32);
    assert_eq!(buffer[2], '*' as u32);
}

#[test]
fn UTF16LEStream() {
    // Test embedded NUL not ending in NUL.
    let data = [0xff, 0xfe, 0x41, 0x00, 0x42, 0x01];
    let mut proxy_stream = CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(&data)));
    let mut buffer = [0; 16];
    assert_eq!(proxy_stream.read_block(&mut buffer), 2);
    assert_eq!(buffer[0], 'A' as u32);
    assert_eq!(buffer[1], '\u{0142}' as u32);
}

#[test]
fn UTF16BEStream() {
    let data = [0xfe, 0xff, 0x00, 0x41, 0x01, 0x42];
    let mut proxy_stream = CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(&data)));
    let mut buffer = [0; 16];
    assert_eq!(proxy_stream.read_block(&mut buffer), 2);
    assert_eq!(buffer[0], 'A' as u32);
    assert_eq!(buffer[1], '\u{0142}' as u32);
}
