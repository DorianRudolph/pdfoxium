// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in ../LICENSE-PDFIUM.

//! Close translation of `core/fxcodec/jbig2/jbig2_bit_stream_unittest.cpp`.
//! Option maps the native 0/-1 result plus output value. The reader's cache
//! key lives outside its byte/bit cursor and does not affect these reads.
#![allow(non_snake_case, non_upper_case_globals)]

use super::Reader;

#[test]
fn ReadNBits() {
    let kData = [0xb1]; // 10110001
    let mut stream = Reader::new_pdfium(&kData);
    assert_eq!(stream.read_bits(1), Some(1u32));
    assert_eq!(stream.read_bits(1).map(|val2| val2 as i32), Some(0));
    assert_eq!(stream.read_bits(2).map(|val2| val2 as i32), Some(3));
    assert_eq!(stream.read_bits(4).map(|val2| val2 as i32), Some(1));
}

#[test]
fn ReadNBitsLargerThenData() {
    let kData = [0xb1];
    let mut stream = Reader::new_pdfium(&kData);
    assert_eq!(stream.read_bits(10), Some(0xb1u32));
}

#[test]
fn ReadNBitsNullStream() {
    let mut stream = Reader::new_pdfium(&[]);
    assert_eq!(stream.read_bits(1), None);
    assert_eq!(stream.read_bits(2).map(|val2| val2 as i32), None);
}

#[test]
fn ReadNBitsOutOfBounds() {
    let kData = [0xb1];
    let mut stream = Reader::new_pdfium(&kData);
    // Native asserts success without checking val1 for this read.
    assert!(stream.read_bits(8).is_some());
    assert_eq!(stream.read_bits(2).map(|val2| val2 as i32), None);
}

#[test]
fn ReadNBitsWhereNIs36() {
    let kData = [0xb0, 0x01, 0x00, 0x00, 0x40];
    let mut stream = Reader::new_pdfium(&kData);
    // The native case name says 36, but the actual request is 34 bits.
    // The top two bits shift off the 32-bit result and are lost.
    assert_eq!(stream.read_bits(34), Some(0xc0040001u32));
}
