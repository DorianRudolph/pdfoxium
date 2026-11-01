// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/byteorder_unittest.cpp` at the pinned PDFium revision.

#![allow(non_snake_case, non_upper_case_globals)]

use super::*;
const kTestValues32: [u32; 17] = [
    0x0, 0x1, 0x2, 0x3, 0x4, 0xfe, 0xff, 0x100, 0x101, 0xffff, 0x10000, 0x123456, 0x345167,
    0x2f3e4a5b, 0xff000000, 0xfffffffe, 0xffffffff,
];

#[test]
fn FromLE16() {
    for v in 0..0x10000u32 {
        let v16 = v as u16;
        let expected = get_uint16_lsb_first(&v16.to_ne_bytes());
        assert_eq!(expected, from_le16(v16), "{v}");
    }
}

#[test]
fn FromLE32() {
    for v in kTestValues32 {
        let expected = get_uint32_lsb_first(&v.to_ne_bytes());
        assert_eq!(expected, from_le32(v), "{v}");
    }
}

#[test]
fn FromBE16() {
    for v in 0..0x10000u32 {
        let v16 = v as u16;
        let expected = get_uint16_msb_first(&v16.to_ne_bytes());
        assert_eq!(expected, from_be16(v16), "{v}");
    }
}

#[test]
fn FromBE32() {
    for v in kTestValues32 {
        let expected = get_uint32_msb_first(&v.to_ne_bytes());
        assert_eq!(expected, from_be32(v), "{v}");
    }
}

#[test]
fn GetUInt16LSBFirst() {
    let kBuf = [0xff, 0xfe];
    assert_eq!(0xfeff, get_uint16_lsb_first(&kBuf));
}

#[test]
fn PutUInt16LSBFirst() {
    let mut buf = [0; 2];
    put_uint16_lsb_first(0xfffe, &mut buf);
    assert_eq!(buf, [0xfe, 0xff]);
}

#[test]
fn GetUInt16MSBFirst() {
    let kBuf = [0xff, 0xfe];
    assert_eq!(0xfffe, get_uint16_msb_first(&kBuf));
}

#[test]
fn PutUInt16MSBFirst() {
    let mut buf = [0; 2];
    put_uint16_msb_first(0xfffe, &mut buf);
    assert_eq!(buf, [0xff, 0xfe]);
}

#[test]
fn GetUInt32LSBFirst() {
    let kBuf = [0xff, 0xfe, 0xfd, 0xfc];
    assert_eq!(0xfcfdfeff, get_uint32_lsb_first(&kBuf));
}

#[test]
fn PutUInt32LSBFirst() {
    let mut buf = [0; 4];
    put_uint32_lsb_first(0xfffefdfc, &mut buf);
    assert_eq!(buf, [0xfc, 0xfd, 0xfe, 0xff]);
}

#[test]
fn GetUInt32MSBFirst() {
    let kBuf = [0xff, 0xfe, 0xfd, 0xfc];
    assert_eq!(0xfffefdfc, get_uint32_msb_first(&kBuf));
}

#[test]
fn PutUInt32MSBFirst() {
    let mut buf = [0; 4];
    put_uint32_msb_first(0xfffefdfc, &mut buf);
    assert_eq!(buf, [0xff, 0xfe, 0xfd, 0xfc]);
}
