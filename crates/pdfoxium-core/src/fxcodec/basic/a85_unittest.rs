// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcodec/basic/a85_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::BasicModule;

#[test]
fn A85EmptyInput() {
    assert!(BasicModule::a85_encode(&[]).is_empty());
}

#[test]
fn A85Basic() {
    let src_buf = [1, 2, 3, 4, 255, 255, 255, 255];
    let dest_buf = BasicModule::a85_encode(&src_buf);
    assert_eq!(dest_buf, [33, 60, 78, 63, 43, 115, 56, 87, 45, 33, 126, 62]);
}

#[test]
fn A85LeftoverBytes() {
    {
        let src_buf_1leftover = [1, 2, 3, 4, 255];
        let dest_buf = BasicModule::a85_encode(&src_buf_1leftover);
        assert_eq!(dest_buf, [33, 60, 78, 63, 43, 114, 114, 126, 62]);
    }
    {
        let src_buf_2leftover = [1, 2, 3, 4, 255, 254];
        let dest_buf = BasicModule::a85_encode(&src_buf_2leftover);
        assert_eq!(dest_buf, [33, 60, 78, 63, 43, 115, 56, 68, 126, 62]);
    }
    {
        let src_buf_3leftover = [1, 2, 3, 4, 255, 254, 253];
        let dest_buf = BasicModule::a85_encode(&src_buf_3leftover);
        assert_eq!(dest_buf, [33, 60, 78, 63, 43, 115, 56, 77, 114, 126, 62]);
    }
}

#[test]
fn A85Zeros() {
    {
        let src_buf = [1, 2, 3, 4, 0, 0, 0, 0];
        let dest_buf = BasicModule::a85_encode(&src_buf);
        assert_eq!(dest_buf, [33, 60, 78, 63, 43, 122, 126, 62]);
    }
    {
        let src_buf_2 = [0, 0, 0, 0, 1, 2, 3, 4];
        let dest_buf = BasicModule::a85_encode(&src_buf_2);
        assert_eq!(dest_buf, [122, 33, 60, 78, 63, 43, 126, 62]);
    }
    {
        let src_buf_3 = [1, 2, 3, 4, 0, 0];
        let dest_buf = BasicModule::a85_encode(&src_buf_3);
        assert_eq!(dest_buf, [33, 60, 78, 63, 43, 33, 33, 33, 126, 62]);
    }
}

#[test]
fn A85LineBreaks() {
    let mut src_buf = [0u8; 131];
    for k in (0..116).step_by(4) {
        src_buf[k] = 1;
        src_buf[k + 1] = 2;
        src_buf[k + 2] = 3;
        src_buf[k + 3] = 4;
    }
    // This second source loop advances by ONE, including its overlapping writes.
    for k in 120..128 {
        src_buf[k] = 1;
        src_buf[k + 1] = 2;
        src_buf[k + 2] = 3;
        src_buf[k + 3] = 4;
    }
    let dest_buf = BasicModule::a85_encode(&src_buf);
    assert_eq!(dest_buf.len(), 166);
    assert_eq!(dest_buf[75], 13);
    assert_eq!(dest_buf[76], 10);
    assert_eq!(dest_buf[153], 13);
    assert_eq!(dest_buf[154], 10);
}
