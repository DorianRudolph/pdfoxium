// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/containers/to_vector_unittest.cpp` over the selected Vec/slice mapping.
#![allow(non_snake_case)]

#[test]
fn FromSpan() {
    let kData: [u8; 4] = [1, 2, 3, 4];
    let span = kData.as_slice();
    let vec: Vec<u8> = span.to_vec();
    assert_eq!(4, vec.len());
    assert_eq!(1, vec[0]);
    assert_eq!(2, vec[1]);
    assert_eq!(3, vec[2]);
    assert_eq!(4, vec[3]);
}

#[test]
fn FromEmptySpan() {
    let span: &[i32] = &[];
    let vec: Vec<i32> = span.to_vec();
    assert!(vec.is_empty());
}

#[test]
fn FromArray() {
    let arr: [i32; 3] = [10, 20, 30];
    let vec: Vec<i32> = arr.to_vec();
    assert_eq!(3, vec.len());
    assert_eq!(10, vec[0]);
    assert_eq!(20, vec[1]);
    assert_eq!(30, vec[2]);
}

#[test]
fn WithExplicitType() {
    let kData: [u16; 3] = [10, 20, 30];
    let span = kData.as_slice();
    let vec: Vec<u32> = span.iter().copied().map(u32::from).collect();
    assert_eq!(3, vec.len());
    assert_eq!(10, vec[0]);
    assert_eq!(20, vec[1]);
    assert_eq!(30, vec[2]);
}
