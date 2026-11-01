// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2022 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/binary_buffer_unittest.cpp`.
//! mem::take retains the explicit native moved-from empty state.
#![allow(non_snake_case)]
use super::BinaryBuffer;

#[test]
fn Empty() {
    let buffer = BinaryBuffer::new();
    assert!(buffer.is_empty());
    assert_eq!(0, buffer.get_size());
    assert_eq!(0, buffer.get_length());
    assert!(buffer.span().is_empty());
}

#[test]
fn MoveConstruct() {
    let mut buffer = BinaryBuffer::new();
    buffer.append_uint8(65);

    let buffer2 = std::mem::take(&mut buffer);

    assert!(buffer.is_empty());
    assert_eq!(0, buffer.get_size());
    assert_eq!(0, buffer.get_length());
    assert!(buffer.span().is_empty());

    assert!(!buffer2.is_empty());
    assert_eq!(1, buffer2.get_size());
    assert_eq!(1, buffer2.get_length());
    assert_eq!(65, buffer2.span()[0]);
}

#[test]
fn MoveAssign() {
    let mut buffer = BinaryBuffer::new();
    let mut buffer2 = BinaryBuffer::new();
    buffer.append_uint8(65);
    drop(std::mem::replace(&mut buffer2, std::mem::take(&mut buffer)));

    assert!(buffer.is_empty());
    assert_eq!(0, buffer.get_size());
    assert_eq!(0, buffer.get_length());
    assert!(buffer.span().is_empty());

    assert!(!buffer2.is_empty());
    assert_eq!(1, buffer2.get_size());
    assert_eq!(1, buffer2.get_length());
    assert_eq!(65, buffer2.span()[0]);
}

#[test]
fn Clear() {
    let mut buffer = BinaryBuffer::new();
    buffer.append_uint8(65);
    buffer.clear();
    assert!(buffer.is_empty());
    assert_eq!(0, buffer.get_size());
    assert_eq!(0, buffer.get_length());
    assert!(buffer.span().is_empty());
}

#[test]
fn AppendSpans() {
    let mut buffer = BinaryBuffer::new();
    let aaa = vec![65u8; 3];
    let bbb = vec![66u8; 3];
    buffer.append_span(&aaa);
    buffer.append_span(&bbb);
    assert!(!buffer.is_empty());
    assert_eq!(6, buffer.get_size());
    assert_eq!(6, buffer.get_length());
    assert_eq!(65, buffer.span()[0]);
    assert_eq!(65, buffer.span()[1]);
    assert_eq!(65, buffer.span()[2]);
    assert_eq!(66, buffer.span()[3]);
    assert_eq!(66, buffer.span()[4]);
    assert_eq!(66, buffer.span()[5]);
}

#[test]
fn AppendBlocks() {
    let mut buffer = BinaryBuffer::new();
    let aaa = vec![65u8; 3];
    let bbb = vec![66u8; 3];
    buffer.append_span(&aaa);
    buffer.append_span(&bbb);
    assert_eq!(6, buffer.get_size());
    assert_eq!(6, buffer.get_length());
    assert_eq!(65, buffer.span()[0]);
    assert_eq!(65, buffer.span()[1]);
    assert_eq!(65, buffer.span()[2]);
    assert_eq!(66, buffer.span()[3]);
    assert_eq!(66, buffer.span()[4]);
    assert_eq!(66, buffer.span()[5]);
}

#[test]
fn AppendStrings() {
    let mut buffer = BinaryBuffer::new();
    buffer.append_string(b"AA");
    buffer.append_string(b"BB");
    assert_eq!(4, buffer.get_size());
    assert_eq!(4, buffer.get_length());
    assert_eq!(65, buffer.span()[0]);
    assert_eq!(65, buffer.span()[1]);
    assert_eq!(66, buffer.span()[2]);
    assert_eq!(66, buffer.span()[3]);
}

#[test]
fn AppendBytes() {
    let mut buffer = BinaryBuffer::new();
    buffer.append_uint8(65);
    buffer.append_uint8(66);
    assert_eq!(2, buffer.get_size());
    assert_eq!(2, buffer.get_length());
    assert_eq!(65, buffer.span()[0]);
    assert_eq!(66, buffer.span()[1]);
}

// Assumes little endian.
#[test]
fn AppendUint16() {
    let mut buffer = BinaryBuffer::new();
    buffer.append_uint16(0x4321);
    assert_eq!(2, buffer.get_size());
    assert_eq!(2, buffer.get_length());
    assert_eq!(0x21, buffer.span()[0]);
    assert_eq!(0x43, buffer.span()[1]);
}

// Assumes little endian.
#[test]
fn AppendUint32() {
    let mut buffer = BinaryBuffer::new();
    buffer.append_uint32(0x87654321);
    assert_eq!(4, buffer.get_size());
    assert_eq!(4, buffer.get_length());
    assert_eq!(0x21, buffer.span()[0]);
    assert_eq!(0x43, buffer.span()[1]);
    assert_eq!(0x65, buffer.span()[2]);
    assert_eq!(0x87, buffer.span()[3]);
}

#[test]
fn AppendDouble() {
    let mut buffer = BinaryBuffer::new();
    buffer.append_double(1234.5678);
    assert_eq!(8, buffer.get_size());
    assert_eq!(8, buffer.get_length());
    // arch-dependent bit pattern.
}
