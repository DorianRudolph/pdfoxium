// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/pdfium_span_unittest.cpp over standard slices and references.
//! Dynamic/constant subviews retain both native blocks. Byte views use native
//! endian scalar conversion. Empty index/front/back deaths become panics.
#![allow(non_snake_case)]

#[test]
fn EmptySpan() {
    let stuff = [1, 2, 3];
    let null_span: &[i32] = &[];
    let stuff_span = &stuff[..];
    assert!(null_span.is_empty());
    assert!(!stuff_span.is_empty());
    {
        let empty_first_span = &stuff_span[..0];
        let empty_last_span = &stuff_span[stuff_span.len() - 0..];
        let empty_sub_span1 = &stuff_span[0..0];
        let empty_sub_span2 = &stuff_span[3..3];
        let empty_sub_span3 = &stuff_span[3..];
        assert!(empty_first_span.is_empty());
        assert!(empty_last_span.is_empty());
        assert!(empty_sub_span1.is_empty());
        assert!(empty_sub_span2.is_empty());
        assert!(empty_sub_span3.is_empty());
    }
    {
        let empty_first_span = &stuff_span[..0];
        let empty_last_span = &stuff_span[stuff_span.len() - 0..];
        let empty_sub_span1 = &stuff_span[0..0];
        let empty_sub_span2 = &stuff_span[3..3];
        let empty_sub_span3 = &stuff_span[3..];
        assert!(empty_first_span.is_empty());
        assert!(empty_last_span.is_empty());
        assert!(empty_sub_span1.is_empty());
        assert!(empty_sub_span2.is_empty());
        assert!(empty_sub_span3.is_empty());
    }
}

#[test]
fn ValidSpan() {
    let stuff = [1, 2, 3];
    let stuff_span = &stuff[..];
    assert!(!stuff_span.is_empty());
    {
        let first_span = &stuff_span[..2];
        let last_span = &stuff_span[stuff_span.len() - 2..];
        let sub_span1 = &stuff_span[1..2];
        let sub_span2 = &stuff_span[2..3];
        let sub_span3 = &stuff_span[1..];
        assert_eq!(first_span, &[1, 2]);
        assert_eq!(last_span, &[2, 3]);
        assert_eq!(sub_span1, &[2]);
        assert_eq!(sub_span2, &[3]);
        assert_eq!(sub_span3, &[2, 3]);
    }
    {
        let first_span = &stuff_span[..2];
        let last_span = &stuff_span[stuff_span.len() - 2..];
        let sub_span1 = &stuff_span[1..2];
        let sub_span2 = &stuff_span[2..3];
        let sub_span3 = &stuff_span[1..];
        assert_eq!(first_span, &[1, 2]);
        assert_eq!(last_span, &[2, 3]);
        assert_eq!(sub_span1, &[2]);
        assert_eq!(sub_span2, &[3]);
        assert_eq!(sub_span3, &[2, 3]);
    }
}

#[test]
fn FrontBack() {
    let one = [1];
    let stuff = [1, 2, 3];
    let one_span = &one[..];
    let stuff_span = &stuff[..];
    assert_eq!(*one_span.first().unwrap(), 1);
    assert_eq!(*one_span.last().unwrap(), 1);
    assert_eq!(*stuff_span.first().unwrap(), 1);
    assert_eq!(*stuff_span.last().unwrap(), 3);
}

#[test]
fn GMockMacroCompatibility() {
    let arr1 = [1, 3, 5];
    let arr2 = [1, 3, 5];
    let vec1 = arr1.to_vec();
    let vec2 = arr2.to_vec();
    let static_span1 = &arr1[..];
    let static_span2 = &arr2[..];
    let dynamic_span1 = &vec1[..];
    let dynamic_span2 = &vec2[..];

    assert_eq!(&arr1[..], &static_span2[..]);
    assert_eq!(&arr1[..], &dynamic_span2[..]);

    assert_eq!(&vec1[..], &static_span2[..]);
    assert_eq!(&vec1[..], &dynamic_span2[..]);

    assert_eq!(static_span1, &[1, 3, 5]);
    assert_eq!(&static_span1[..], &arr2[..]);
    assert_eq!(&static_span1[..], &static_span2[..]);
    assert_eq!(&static_span1[..], &dynamic_span2[..]);
    assert_eq!(&static_span1[..], &vec2[..]);

    assert_eq!(dynamic_span1, &[1, 3, 5]);
    assert_eq!(&dynamic_span1[..], &arr2[..]);
    assert_eq!(&dynamic_span1[..], &static_span2[..]);
    assert_eq!(&dynamic_span1[..], &dynamic_span2[..]);
    assert_eq!(&dynamic_span1[..], &vec2[..]);
}

#[test]
fn AsByteSpan() {
    let c_style = [1i32, 2];
    let const_c_style = [3i32, 4];
    let cxx_style = [1i32, 2];
    let const_cxx_style = [3i32, 4];
    // Both native readable/writable forms assert values only. Preserve all six
    // assertions; safe conversion materializes the eight scalar bytes.
    fn bytes(v: &[i32; 2]) -> [u8; 8] {
        let mut out = [0; 8];
        out[..4].copy_from_slice(&v[0].to_ne_bytes());
        out[4..].copy_from_slice(&v[1].to_ne_bytes());
        out
    }
    let check12 =
        |b: [u8; 8]| assert!(b == [0, 0, 0, 1, 0, 0, 0, 2] || b == [1, 0, 0, 0, 2, 0, 0, 0]);
    let check34 =
        |b: [u8; 8]| assert!(b == [0, 0, 0, 3, 0, 0, 0, 4] || b == [3, 0, 0, 0, 4, 0, 0, 0]);
    check12(bytes(&c_style));
    check12(bytes(&c_style));
    check12(bytes(&cxx_style));
    check12(bytes(&cxx_style));
    check34(bytes(&const_c_style));
    check34(bytes(&const_cxx_style));
}
#[test]
#[should_panic]
fn EmptySpanIndex() {
    let empty_span: &mut [i32] = &mut [];
    let index = std::hint::black_box(0);
    empty_span[index] += 1;
}
#[test]
#[should_panic]
fn EmptySpanFront() {
    let empty_span: &mut [i32] = &mut [];
    *empty_span.first_mut().unwrap() += 1;
}
#[test]
#[should_panic]
fn EmptySpanBack() {
    let empty_span: &mut [i32] = &mut [];
    *empty_span.last_mut().unwrap() += 1;
}
