// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/fx_ceil_div_unittest.cpp`.

#![allow(non_snake_case, non_upper_case_globals)]

use super::*;

const _: () = {
    assert!(ceil_div_i32(0, 8) == 0);
    assert!(ceil_div_i32(1, 8) == 1);
    assert!(ceil_div_i32(8, 8) == 1);
    assert!(ceil_div_i32(9, 8) == 2);
};

#[test]
fn Exact() {
    assert_eq!(0, ceil_div_i32(0, 4));
    assert_eq!(1, ceil_div_i32(4, 4));
    assert_eq!(2, ceil_div_i32(8, 4));
    assert_eq!(25, ceil_div_i32(100, 4));
}

#[test]
fn RoundsUp() {
    assert_eq!(1, ceil_div_i32(1, 4));
    assert_eq!(1, ceil_div_i32(3, 4));
    assert_eq!(2, ceil_div_i32(5, 4));
    assert_eq!(2, ceil_div_i32(7, 4));
}

#[test]
fn DivisorOfOne() {
    assert_eq!(0, ceil_div_i32(0, 1));
    assert_eq!(7, ceil_div_i32(7, 1));
}

#[test]
fn NegativeDividend() {
    // Round towards positive infinity, i.e. towards zero here.
    assert_eq!(0, ceil_div_i32(-1, 4));
    assert_eq!(0, ceil_div_i32(-3, 4));
    assert_eq!(-1, ceil_div_i32(-4, 4));
    assert_eq!(-1, ceil_div_i32(-7, 4));
    assert_eq!(-2, ceil_div_i32(-8, 4));
}

#[test]
fn NoOverflowNearMax() {
    const kMax32: u32 = u32::MAX;
    assert_eq!(kMax32 / 8 + 1, ceil_div_u32(kMax32, 8));
    assert_eq!(kMax32, ceil_div_u32(kMax32, 1));
    const kMaxSigned32: i32 = i32::MAX;
    assert_eq!(kMaxSigned32 / 8 + 1, ceil_div_i32(kMaxSigned32, 8));
    const kMinSigned32: i32 = i32::MIN;
    assert_eq!(kMinSigned32 / 8, ceil_div_i32(kMinSigned32, 8));
    const kMax64: u64 = u64::MAX;
    assert_eq!(kMax64 / 8 + 1, ceil_div_u64(kMax64, 8));
}

#[test]
fn NarrowTypes() {
    const kMax8: u8 = u8::MAX;
    // The binding checks the original compile-time return-type assertion.
    let result: u8 = ceil_div_u8(kMax8, 2);
    assert_eq!(128, result);
    assert_eq!(3, ceil_div_u8(5, 2));
    const kMax16: u16 = u16::MAX;
    assert_eq!(32768, ceil_div_u16(kMax16, 2));
}

#[test]
fn CheckedType() {
    // CheckedNumeric validity is Option in the selected Rust representation.
    let value = Some(9u32);
    assert_eq!(2, value.map(|value| ceil_div_u32(value, 8)).unwrap());
    let size = Some(16usize);
    assert_eq!(2, size.map(|size| ceil_div_usize(size, 8)).unwrap());
}

#[test]
fn CheckedTypePropagatesInvalid() {
    let invalid = u32::MAX.checked_mul(2);
    assert!(invalid.map(|value| ceil_div_u32(value, 8)).is_none());
}

#[test]
fn CheckedTypeNoOverflowNearMax() {
    // A valid dividend stays valid however close to the maximum it is.
    const kMax32: u32 = u32::MAX;
    let near_max = Some(kMax32);
    assert_eq!(
        kMax32 / 8 + 1,
        near_max.map(|value| ceil_div_u32(value, 8)).unwrap()
    );
}
