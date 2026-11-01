// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/fx_number_unittest.cpp` at the pinned PDFium revision.

#![allow(non_snake_case, non_upper_case_globals)]

use super::FxNumber;
use crate::test_support::assert_float_eq;

#[test]
fn Default() {
    let number = FxNumber::default();
    assert!(number.is_integer());
    assert!(!(number.is_signed()));
    assert_eq!(0, number.get_signed());
    assert_float_eq(0.0f32, number.get_float());
}

#[test]
fn FromSigned() {
    let number = FxNumber::from_signed(-128);
    assert!(number.is_integer());
    assert!(number.is_signed());
    assert_eq!(-128, number.get_signed());
    assert_float_eq(-128.0f32, number.get_float());

    // Show that assignment works.
    let number2 = number;
    assert!(number2.is_integer());
    assert!(number2.is_signed());
    assert_eq!(-128, number2.get_signed());
    assert_float_eq(-128.0f32, number2.get_float());
}

#[test]
fn FromFloat() {
    let number = FxNumber::from_float(-100.001f32);
    assert!(!(number.is_integer()));
    assert!(number.is_signed());
    assert_eq!(-100, number.get_signed());
    assert_float_eq(-100.001f32, number.get_float());

    // Show that assignment works.
    let number2 = number;
    assert!(!(number2.is_integer()));
    assert!(number2.is_signed());
    assert_eq!(-100, number2.get_signed());
    assert_float_eq(-100.001f32, number2.get_float());

    // Show positive saturation.
    let number3 = FxNumber::from_float(1e17f32);
    assert!(!(number3.is_integer()));
    assert!(number3.is_signed());
    assert_eq!(i32::MAX, number3.get_signed());

    // Show negative saturation.
    let number4 = FxNumber::from_float(-1e17f32);
    assert!(!(number4.is_integer()));
    assert!(number4.is_signed());
    assert_eq!(i32::MIN, number4.get_signed());
}

#[test]
fn FromStringUnsigned() {
    let test_func = |test_cases: &[(&[u8], i32)]| {
        for &(input, expected_output) in test_cases {
            let number = FxNumber::from_string(input);
            assert!(number.is_integer());
            assert!(!number.is_signed());
            assert_eq!(expected_output, number.get_signed());
        }
    };
    let kNormalCases: &[(&[u8], i32)] = &[(b"", 0), (b"0", 0), (b"10", 10)];
    test_func(kNormalCases);

    let kOverflowCases: &[(&[u8], i32)] = &[
        (b"4223423494965252", 0),
        (b"4294967296", 0),
        (b"4294967297", 0),
        (b"5000000000", 0),
    ];
    test_func(kOverflowCases);

    let kNegativeCases: &[(&[u8], i32)] = &[
        (b"4294965252", -2044),
        (b"4294967247", -49),
        (b"4294967248", -48),
        (b"4294967292", -4),
        (b"4294967295", -1),
    ];
    test_func(kNegativeCases);
}

#[test]
fn FromStringSigned() {
    {
        let number = FxNumber::from_string(b"-0");
        assert!(number.is_integer());
        assert!(number.is_signed());
        assert_eq!(0, number.get_signed());
    }
    {
        let number = FxNumber::from_string(b"+0");
        assert!(number.is_integer());
        assert!(number.is_signed());
        assert_eq!(0, number.get_signed());
    }
    {
        let number = FxNumber::from_string(b"-10");
        assert!(number.is_integer());
        assert!(number.is_signed());
        assert_eq!(-10, number.get_signed());
    }
    {
        let number = FxNumber::from_string(b"+10");
        assert!(number.is_integer());
        assert!(number.is_signed());
        assert_eq!(10, number.get_signed());
    }
    {
        let number = FxNumber::from_string(b"-2147483648");
        assert!(number.is_integer());
        assert!(number.is_signed());
        assert_eq!(i32::MIN, number.get_signed());
    }
    {
        let number = FxNumber::from_string(b"+2147483647");
        assert!(number.is_integer());
        assert!(number.is_signed());
        assert_eq!(i32::MAX, number.get_signed());
    }
    {
        // Value underflows.
        let number = FxNumber::from_string(b"-2147483649");
        assert_eq!(0, number.get_signed());
    }
    {
        // Value overflows.
        let number = FxNumber::from_string(b"+2147483648");
        assert_eq!(0, number.get_signed());
    }
}

#[test]
fn FromStringFloat() {
    let number = FxNumber::from_string(b"3.24");
    assert_float_eq(3.24f32, number.get_float());
}
