// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/maybe_owned_data_vector_unittest.cpp over borrowed/owned slices.
//! Cow preserves selected ownership variants without copying borrowed data.
#![allow(non_snake_case)]
use std::borrow::Cow;

#[test]
fn Default() {
    let vec: Cow<'_, [i32]> = Cow::Borrowed(&[]);
    assert!(!matches!(vec, Cow::Owned(_)));
    assert!(vec.is_empty());
}
#[test]
fn Unowned() {
    let data = [1, 2, 3, 4, 5];
    let vec = Cow::Borrowed(&data[..]);
    assert!(!matches!(vec, Cow::Owned(_)));
    assert!(std::ptr::eq(data.as_ptr(), vec.as_ptr()));
    assert_eq!(&vec[..], &[1, 2, 3, 4, 5]);
}
#[test]
fn Owned() {
    let data = vec![1, 2, 3, 4, 5];
    let vec: Cow<'_, [i32]> = Cow::Owned(data);
    assert!(matches!(vec, Cow::Owned(_)));
    assert_eq!(&vec[..], &[1, 2, 3, 4, 5]);
}
#[test]
fn ResizeOwned() {
    let data = vec![1, 2, 3];
    let mut vec: Cow<'_, [i32]> = Cow::Owned(data);
    assert!(matches!(vec, Cow::Owned(_)));
    assert_eq!(&vec[..], &[1, 2, 3]);
    match &mut vec {
        Cow::Owned(data) => data.resize(5, 0),
        Cow::Borrowed(_) => panic!("ResizeOwned"),
    }
    assert!(matches!(vec, Cow::Owned(_)));
    assert_eq!(&vec[..], &[1, 2, 3, 0, 0]);
}
#[test]
fn Copy() {
    let data = vec![1, 2, 3];
    let vec1: Cow<'_, [i32]> = Cow::Owned(data);
    let vec2 = vec1.clone(); // Explicit native Copy operation being tested.
    assert!(matches!(vec1, Cow::Owned(_)));
    assert!(matches!(vec2, Cow::Owned(_)));
    assert_eq!(&vec1[..], &[1, 2, 3]);
    assert_eq!(&vec2[..], &[1, 2, 3]);
    assert!(!std::ptr::eq(vec1.as_ptr(), vec2.as_ptr()));
}
#[test]
fn Move() {
    let data = vec![1, 2, 3];
    let mut vec1: Cow<'_, [i32]> = Cow::Owned(data);
    let vec2 = std::mem::replace(&mut vec1, Cow::Borrowed(&[]));
    assert!(!matches!(vec1, Cow::Owned(_)));
    assert!(vec1.is_empty());
    assert!(matches!(vec2, Cow::Owned(_)));
    assert_eq!(&vec2[..], &[1, 2, 3]);
}
