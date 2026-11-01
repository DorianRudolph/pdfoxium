// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/utf16_unittest.cpp` at the pinned PDFium revision.

#![allow(non_snake_case, non_upper_case_globals)]

use super::*;

const _: () = {
    assert!(SURROGATE_MASK == 0x3ff);
    assert!(MAXIMUM_SUPPLEMENTARY_CODE_POINT == 0x10ffff);
    assert!(MAXIMUM_HIGH_SURROGATE_CODE_UNIT == 0xdbff);
    assert!(MINIMUM_LOW_SURROGATE_CODE_UNIT == 0xdc00);
    assert!(MAXIMUM_LOW_SURROGATE_CODE_UNIT == 0xdfff);
    assert!(!is_supplementary(0xffff));
    assert!(is_supplementary(0x10000));
    assert!(is_supplementary(0x10ffff));
    assert!(!is_supplementary(0x110000));
    assert!(!is_high_surrogate(0xd7ff));
    assert!(is_high_surrogate(0xd800));
    assert!(is_high_surrogate(0xdbff));
    assert!(!is_high_surrogate(0xdc00));
    assert!(!is_low_surrogate(0xdbff));
    assert!(is_low_surrogate(0xdc00));
    assert!(is_low_surrogate(0xdfff));
    assert!(!is_low_surrogate(0xe000));
    assert!(SurrogatePair::new(0xd800, 0xdc00).high() == 0xd800);
    assert!(SurrogatePair::new(0xd800, 0xdc00).low() == 0xdc00);
    assert!(SurrogatePair::new(0xd800, 0xdc00).to_code_point() == 0x10000);
    assert!(SurrogatePair::new(0xdbff, 0xdfff).high() == 0xdbff);
    assert!(SurrogatePair::new(0xdbff, 0xdfff).low() == 0xdfff);
    assert!(SurrogatePair::new(0xdbff, 0xdfff).to_code_point() == 0x10ffff);
    assert!(SurrogatePair::from_code_point(0x10000).high() == 0xd800);
    assert!(SurrogatePair::from_code_point(0x10000).low() == 0xdc00);
    assert!(SurrogatePair::from_code_point(0x10000).to_code_point() == 0x10000);
    assert!(SurrogatePair::from_code_point(0x10ffff).high() == 0xdbff);
    assert!(SurrogatePair::from_code_point(0x10ffff).low() == 0xdfff);
    assert!(SurrogatePair::from_code_point(0x10ffff).to_code_point() == 0x10ffff);
};

#[test]
fn RoundTrip() {
    for code_point in MINIMUM_SUPPLEMENTARY_CODE_POINT..=MAXIMUM_SUPPLEMENTARY_CODE_POINT {
        let from_code_point = SurrogatePair::from_code_point(code_point);
        assert_eq!(code_point, from_code_point.to_code_point());
        let from_pair = SurrogatePair::new(from_code_point.high(), from_code_point.low());
        assert_eq!(from_code_point.high(), from_pair.high());
        assert_eq!(from_code_point.low(), from_pair.low());
        assert_eq!(code_point, from_pair.to_code_point());
    }
}
