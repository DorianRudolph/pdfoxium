// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2022 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/fx_string_wrappers_unittest.cpp`.
#![allow(non_snake_case)]
use std::fmt::Write;

#[test]
fn string() {
    let mut str = String::new();
    str.push('2');
    str.push('2');
    str.push_str("C is ");
    str.push('7');
    str.push('1');
    str.push('.');
    str.push('6');
    str.push_str("F");
    assert_eq!("22C is 71.6F", str);
}

#[test]
fn OStringStream() {
    let mut str = String::new();
    write!(&mut str, "{}{}", 22, "C is ").unwrap();
    super::write_default_float(&mut str, 71.6f32).unwrap();
    str.push_str("F");
    assert_eq!("22C is 71.6F", str);
}
