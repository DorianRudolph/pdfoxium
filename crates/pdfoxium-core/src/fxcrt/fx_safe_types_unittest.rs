// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2022 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/fx_safe_types_unittest.cpp`.
//! CheckedNumeric validity maps to Option and Rust checked arithmetic.
#![allow(non_snake_case)]

// PDFium relies on safe types handling the --2147483648 boundary
// condition without overflow.
#[test]
fn UnaryMinus() {
    let safe_val = Some(i32::MIN);
    assert!(safe_val.is_some());
    assert!(safe_val.and_then(i32::checked_neg).is_none());
}

#[test]
fn SubtractFromZero() {
    let safe_val = Some(i32::MIN);
    assert!(safe_val.is_some());
    assert!(safe_val.and_then(|value| 0i32.checked_sub(value)).is_none());
}
