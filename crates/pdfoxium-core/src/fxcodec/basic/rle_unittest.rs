// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcodec/basic/rle_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::BasicModule;
use crate::fpdfapi::parser::fpdf_parser_decode::run_length_decode;

#[test]
fn RLEEmptyInput() {
    assert!(BasicModule::run_length_encode(&[]).is_empty());
}

#[test]
fn RLEShortInput() {
    let src_buf = [1];
    let dest_buf = BasicModule::run_length_encode(&src_buf);
    assert_eq!(dest_buf.len(), 3);
    assert_eq!(dest_buf[0], 0);
    assert_eq!(dest_buf[1], 1);
    assert_eq!(dest_buf[2], 128);
}

#[test]
fn RLENormalInputs() {
    // Match/match, match/non-match and non-match/match.
    for src_buf in [
        [2, 2, 2, 2, 4, 4, 4, 4, 4, 4],
        [2, 2, 2, 2, 1, 2, 3, 4, 5, 6],
        [1, 2, 3, 4, 5, 3, 3, 3, 3, 3],
    ] {
        let dest_buf = BasicModule::run_length_encode(&src_buf);
        let result = run_length_decode(&dest_buf);
        assert_eq!(result.data, src_buf);
    }
}

#[test]
fn RLEFullLengthInputs() {
    {
        // C++ {{1}} sets only the first element; all others are ZERO.
        let mut src_buf_1 = [0u8; 260];
        src_buf_1[0] = 1;
        let dest_buf = BasicModule::run_length_encode(&src_buf_1);
        let result = run_length_decode(&dest_buf);
        assert_eq!(result.data, src_buf_1);
    }
    {
        let mut src_buf_2 = [0u8; 260];
        src_buf_2[0] = 2;
        for i in 128..260 {
            src_buf_2[i] = (i - 125) as u8;
        }
        let dest_buf = BasicModule::run_length_encode(&src_buf_2);
        let result = run_length_decode(&dest_buf);
        assert_eq!(result.data, src_buf_2);
    }
    {
        let mut src_buf_3 = [0u8; 260];
        for i in 0..128 {
            src_buf_3[i] = i as u8;
        }
        let dest_buf = BasicModule::run_length_encode(&src_buf_3);
        let result = run_length_decode(&dest_buf);
        assert_eq!(result.data, src_buf_3);
    }
    {
        let mut src_buf_4 = [0u8; 260];
        for i in 0..260 {
            src_buf_4[i] = i as u8;
        }
        let dest_buf = BasicModule::run_length_encode(&src_buf_4);
        let result = run_length_decode(&dest_buf);
        assert_eq!(result.data, src_buf_4);
    }
}
