// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2024 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/stl_util_unittest.cpp` over selected
//! slice fill/copy primitives. Uninitialized native arrays are safely zeroed
//! before the same fill/copy operations; tests never inspect their initial state.
#![allow(non_snake_case)]

#[test]
fn FillCArray() {
    let mut buf = [0 as u32; 4];
    buf.fill(0x01020304);
    for b in buf {
        assert_eq!(b, 0x01020304);
    }
}

#[test]
fn FillStdArray() {
    let mut buf = [0 as u16; 10];
    buf.fill(0x0102);
    for b in buf {
        assert_eq!(b, 0x0102);
    }
}

#[test]
fn FillStdVector() {
    let mut buf = vec![0 as u8; 15];
    buf.fill(0x32);
    for b in buf {
        assert_eq!(b, 0x32);
    }
}

#[test]
fn FillSpan() {
    let mut buf = [0 as f32; 12];
    let buf_span = buf.as_mut_slice();
    buf_span.fill(123.0f32);
    for b in buf {
        assert_eq!(b, 123.0f32);
    }
}

#[test]
fn CopyCArray() {
    let mut dst = [0 as u32; 4];
    {
        let mut buf = [0 as u32; 4];
        buf.fill(0x01020304);
        dst.copy_from_slice(&buf);
    }
    for b in dst {
        assert_eq!(b, 0x01020304);
    }
}

#[test]
fn CopyStdArray() {
    let mut dst = [0 as u16; 10];
    {
        let mut buf = [0 as u16; 10];
        buf.fill(0x0102);
        dst.copy_from_slice(&buf);
    }
    for b in dst {
        assert_eq!(b, 0x0102);
    }
}

#[test]
fn CopyStdVector() {
    let mut dst = [0 as u8; 15];
    {
        let mut buf = vec![0 as u8; 15];
        buf.fill(0x32);
        dst.copy_from_slice(&buf);
    }
    for b in dst {
        assert_eq!(b, 0x32);
    }
}

#[test]
fn CopySpan() {
    let mut dst = [0 as f32; 12];
    {
        let mut buf = [0 as f32; 12];
        let buf_span = buf.as_mut_slice();
        buf_span.fill(123.0f32);
        dst.copy_from_slice(buf_span);
    }
    for b in dst {
        assert_eq!(b, 123.0f32);
    }
}
