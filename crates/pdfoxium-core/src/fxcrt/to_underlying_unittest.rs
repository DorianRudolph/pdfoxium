// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of core/fxcrt/to_underlying_unittest.cpp over selected Rust primitives.
#![allow(non_snake_case, non_upper_case_globals, non_camel_case_types)]

#[test]
fn Basic() {
    #[repr(i32)]
    enum E {
        kA = 1,
        kB = 2,
    }
    assert_eq!(1 as i32, E::kA as i32);
    assert_eq!(2 as i32, E::kB as i32);
}

#[test]
fn NonClassEnum() {
    #[repr(i32)]
    enum E {
        kA = 1,
        kB = 2,
    }
    assert_eq!(1 as i32, E::kA as i32);
    assert_eq!(2 as i32, E::kB as i32);
}

#[test]
fn UnderlyingType() {
    #[repr(i32)]
    enum E {
        kA = 1,
        kB = 2,
    }
    assert_eq!(1 as i32, E::kA as i32);
    assert_eq!(2 as i32, E::kB as i32);
}

#[test]
fn OutOfOrderIndices() {
    #[repr(i32)]
    enum E {
        kA = 1,
        kB = 67,
    }
    assert_eq!(1 as i32, E::kA as i32);
    assert_eq!(67 as i32, E::kB as i32);
}

#[test]
fn NegativeValues() {
    #[repr(i32)]
    enum E {
        kNeg = -1,
        kZero = 0,
        kPos = 1,
    }
    assert_eq!(-1 as i32, E::kNeg as i32);
    assert_eq!(0 as i32, E::kZero as i32);
    assert_eq!(1 as i32, E::kPos as i32);
}

#[test]
fn LargeValues() {
    #[repr(u64)]
    enum E {
        kMax = 0xFFFFFFFFFFFFFFFF,
        kMid = 1234567890123456789,
    }
    assert_eq!(0xFFFFFFFFFFFFFFFF as u64, E::kMax as u64);
    assert_eq!(1234567890123456789 as u64, E::kMid as u64);
}

#[test]
fn CustomUnderlyingType() {
    #[repr(i8)]
    enum E {
        kA = 1,
        kB = 127,
    }
    assert_eq!(1 as i8, E::kA as i8);
    assert_eq!(127 as i8, E::kB as i8);
}

#[test]
fn MultipleEnums() {
    #[repr(i32)]
    enum E1 {
        kA = 1,
    }
    #[repr(i32)]
    enum E2 {
        kB = 2,
    }
    assert_eq!(1 as i32, E1::kA as i32);
    assert_eq!(2 as i32, E2::kB as i32);
}
