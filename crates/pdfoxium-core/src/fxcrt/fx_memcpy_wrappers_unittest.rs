// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fxcrt/fx_memcpy_wrappers_unittest.cpp`: native null/zero ranges map
//! to valid empty Rust slices. Preserve zero length and unchanged destination
//! identity; empty comparisons/searches retain their exact zero/None results.
#![allow(non_snake_case)]

#[test]
fn FXSYSmemset() {
    let destination: &mut [u8] = &mut [];
    let original = destination.as_ptr();
    destination.fill(0);
    let result = destination;
    assert!(std::ptr::eq(original, result.as_ptr()));
    assert_eq!(0usize, result.len());
}

#[test]
fn FXSYSwmemset() {
    let destination: &mut [u32] = &mut [];
    let original = destination.as_ptr();
    destination.fill(0);
    let result = destination;
    assert!(std::ptr::eq(original, result.as_ptr()));
    assert_eq!(0usize, result.len());
}

#[test]
fn FXSYSmemcpy() {
    let destination: &mut [u8] = &mut [];
    let original = destination.as_ptr();
    destination.copy_from_slice(&[]);
    let result = destination;
    assert!(std::ptr::eq(original, result.as_ptr()));
    assert_eq!(0usize, result.len());
}

#[test]
fn FXSYSwmemcpy() {
    let destination: &mut [u32] = &mut [];
    let original = destination.as_ptr();
    destination.copy_from_slice(&[]);
    let result = destination;
    assert!(std::ptr::eq(original, result.as_ptr()));
    assert_eq!(0usize, result.len());
}

#[test]
fn FXSYSmemmove() {
    let destination: &mut [u8] = &mut [];
    let original = destination.as_ptr();
    destination.copy_within(0..0, 0);
    let result = destination;
    assert!(std::ptr::eq(original, result.as_ptr()));
    assert_eq!(0usize, result.len());
}

#[test]
fn FXSYSwmemmove() {
    let destination: &mut [u32] = &mut [];
    let original = destination.as_ptr();
    destination.copy_within(0..0, 0);
    let result = destination;
    assert!(std::ptr::eq(original, result.as_ptr()));
    assert_eq!(0usize, result.len());
}

#[test]
fn FXSYSmemcmp() {
    let left: &[u8] = &[];
    let right: &[u8] = &[];
    assert_eq!(0, left.cmp(right) as i32);
}

#[test]
fn FXSYSwmemcmp() {
    let left: &[u32] = &[];
    let right: &[u32] = &[];
    assert_eq!(0, left.cmp(right) as i32);
}

#[test]
fn FXSYSmemchr() {
    let data: &[u8] = &[];
    assert_eq!(None, data.iter().position(|&ch| ch == 0));
}

#[test]
fn FXSYSwmemchr() {
    let data: &[u32] = &[];
    assert_eq!(None, data.iter().position(|&ch| ch == 0));
}
