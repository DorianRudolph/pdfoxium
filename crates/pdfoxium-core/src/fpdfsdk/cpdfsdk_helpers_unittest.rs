// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Source string-copy contracts and all ParsePageRangeString inputs/results.
#![allow(non_snake_case)]
use super::parse_page_range_string;

#[test]
fn ParsePageRangeString() {
    let cases: &[(&[u8], u32, &[u32])] = &[
        (b"", 1, &[]),
        (b" ", 1, &[]),
        (b"clams", 1, &[]),
        (b"0", 0, &[]),
        (b"1", 0, &[]),
        (b",1", 10, &[]),
        (b"1,", 10, &[]),
        (b"1,clams", 1, &[]),
        (b"clams,1", 1, &[]),
        (b"0-1", 10, &[]),
        (b"1-0", 10, &[]),
        (b"1-5", 4, &[]),
        (b"1-11,", 10, &[]),
        (b",1-1", 10, &[]),
        (b"1-", 10, &[]),
        (b"1-,", 10, &[]),
        (b"-2,", 10, &[]),
        (b"1-clams", 10, &[]),
        (b"clams-1,", 10, &[]),
        (b"1-2clams", 10, &[]),
        (b"0,1", 10, &[]),
        (b"1,0", 10, &[]),
        (b"1-2,,,,3-4", 10, &[]),
        (b"1-2-", 10, &[]),
        (b"1-1", 10, &[0]),
        (b"1", 1, &[0]),
        (b"1-4", 4, &[0, 1, 2, 3]),
        (b"1- 4", 4, &[0, 1, 2, 3]),
        (b"1 -4", 4, &[0, 1, 2, 3]),
        (b"1,2", 10, &[0, 1]),
        (b"2,1", 10, &[1, 0]),
        (b"1,50,2", 100, &[0, 49, 1]),
        (b"1-4,50", 100, &[0, 1, 2, 3, 49]),
        (b"50,1-2", 100, &[49, 0, 1]),
        (b"5  0, 1-2 ", 100, &[49, 0, 1]),
        (b"1-3,4-6", 10, &[0, 1, 2, 3, 4, 5]),
        (b"1-4,3-6", 10, &[0, 1, 2, 3, 2, 3, 4, 5]),
    ];
    for &(input, count, expected) in cases {
        assert_eq!(
            parse_page_range_string(input, count),
            expected,
            "range {input:?}"
        );
    }
}

#[test]
fn NulTerminateMaybeCopyAndReturnLength() {
    use super::nul_terminate_maybe_copy_and_return_length as copy;
    let text = b"toBeCopied";
    assert_eq!(text.len(), 10);
    assert_eq!(copy(text, &mut []), 11);
    let mut buffer = [0x42; 11];
    assert_eq!(copy(text, &mut buffer[..10]), 11);
    assert!(buffer.iter().all(|&byte| byte == 0x42));
    assert_eq!(copy(text, &mut buffer), 11);
    assert_eq!(&buffer[..10], text);
    assert_eq!(buffer[10], 0);
    let mut empty = [0x42];
    assert_eq!(copy(b"", &mut empty), 1);
    assert_eq!(empty, [0]);
}
