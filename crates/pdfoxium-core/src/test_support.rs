// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Assertions corresponding to GTest and native C++ comparison contracts.

/// WideString::operator==(const wchar_t*) uses wcscmp, unlike the
/// length-bearing WideString/WideString overload. Preserve that distinction
/// for native EXPECT_EQ assertions whose expected value is a wide literal.
#[track_caller]
pub fn assert_wide_cstr_eq(expected: &[u32], actual: &[u32]) {
    let prefix = |value: &[u32]| value.iter().position(|&ch| ch == 0).unwrap_or(value.len());
    assert_eq!(&expected[..prefix(expected)], &actual[..prefix(actual)]);
}

fn biased_bits(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 != 0 {
        (!bits).wrapping_add(1)
    } else {
        bits | 0x8000_0000
    }
}

/// GTest EXPECT_FLOAT_EQ permits four ULPs, treats signed zeros equally and
/// rejects every NaN comparison (including a NaN compared with itself).
#[track_caller]
pub fn assert_float_eq(expected: f32, actual: f32) {
    assert!(
        !expected.is_nan()
            && !actual.is_nan()
            && biased_bits(expected).abs_diff(biased_bits(actual)) <= 4,
        "expected {expected:?}, actual {actual:?} (GTest FLOAT_EQ, 4 ULPs)"
    );
}

/// GTest EXPECT_DOUBLE_EQ has the same four-ULP contract at f64 precision.
#[track_caller]
pub fn assert_double_eq(expected: f64, actual: f64) {
    let biased = |value: f64| {
        let bits = value.to_bits();
        if bits & (1 << 63) != 0 {
            (!bits).wrapping_add(1)
        } else {
            bits | (1 << 63)
        }
    };
    assert!(
        !expected.is_nan() && !actual.is_nan() && biased(expected).abs_diff(biased(actual)) <= 4,
        "expected {expected:?}, actual {actual:?} (GTest DOUBLE_EQ, 4 ULPs)"
    );
}
