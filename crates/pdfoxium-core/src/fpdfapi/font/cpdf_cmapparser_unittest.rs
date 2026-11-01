// Copyright 2015 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/font/cpdf_cmapparser_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::{get_code, get_code_range};

#[test]
fn GetCode() {
    assert_eq!(get_code(b""), 0);
    assert_eq!(get_code(b"<"), 0);
    assert_eq!(get_code(b"<c2"), 194);
    assert_eq!(get_code(b"<A2"), 162);
    assert_eq!(get_code(b"<Af2"), 2802);
    assert_eq!(get_code(b"<A2z"), 162);
    assert_eq!(get_code(b"12"), 12);
    assert_eq!(get_code(b"12d"), 12);
    assert_eq!(get_code(b"128"), 128);
    assert_eq!(get_code(b"<FFFFFFFF"), 4294967295);
    assert_eq!(get_code(b"<100000000"), 0);
}

#[test]
fn GetCodeRange() {
    assert!(get_code_range(b"", b"").is_none());
    assert!(get_code_range(b"A", b"").is_none());
    assert!(get_code_range(b"<aaaaaaaaaa>", b"").is_none());
    let range = get_code_range(b"<12345678>", b"<87654321>");
    assert!(range.is_some());
    let range = range.unwrap();
    assert_eq!(range.char_size, 4);
    assert_eq!(range.lower, [18, 52, 86, 120]);
    assert_eq!(range.upper, [135, 101, 67, 33]);
    let range = get_code_range(b"<a1>", b"<F3>");
    assert!(range.is_some());
    let range = range.unwrap();
    assert_eq!(range.char_size, 1);
    assert_eq!(range.lower[0], 161);
    assert_eq!(range.upper[0], 243);
    let range = get_code_range(b"<a1>", b"");
    assert!(range.is_some());
    let range = range.unwrap();
    assert_eq!(range.char_size, 1);
    assert_eq!(range.lower[0], 161);
    assert_eq!(range.upper[0], 0);
}
