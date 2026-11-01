// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fxcrt/code_point_view_unittest.cpp`. In the selected wchar32 build,
//! CodePointView aliases WideStringView, represented by a borrowed u32 slice
//! (the same view iterated by fx_string::utf8_encode).
#![allow(non_snake_case)]

type CodePointView<'a> = &'a [u32];

fn Materialize(view: CodePointView<'_>) -> Vec<u32> {
    let mut materialized = Vec::new();
    for &code_point in view {
        materialized.push(code_point);
    }
    materialized
}

#[test]
fn Empty() {
    assert_eq!([] as [u32; 0], Materialize(&[]).as_slice());
}

#[test]
fn Basic() {
    assert_eq!(
        [0x28, 0x80, 0xffff, 0x29],
        Materialize(&[0x28, 0x80, 0xffff, 0x29]).as_slice()
    );
}

#[test]
fn Supplementary() {
    assert_eq!(
        [0x28, 0x1f3a8, 0x29],
        Materialize(&[0x28, 0x1f3a8, 0x29]).as_slice()
    );
}

#[test]
fn UnpairedHighSurrogate() {
    assert_eq!([0xd800], Materialize(&[0xd800]).as_slice());
}

#[test]
fn UnpairedLowSurrogate() {
    assert_eq!([0xdc00], Materialize(&[0xdc00]).as_slice());
}
