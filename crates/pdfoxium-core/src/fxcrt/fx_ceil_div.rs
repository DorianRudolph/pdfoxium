// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Overflow-safe integer ceiling division from `core/fxcrt/fx_ceil_div.h`.

macro_rules! define_ceil_div {
    ($name:ident, $type:ty) => {
        pub const fn $name(dividend: $type, divisor: $type) -> $type {
            assert!(divisor > 0);
            let quotient = dividend / divisor;
            quotient + (dividend % divisor > 0) as $type
        }
    };
}

define_ceil_div!(ceil_div_i32, i32);
define_ceil_div!(ceil_div_u8, u8);
define_ceil_div!(ceil_div_u16, u16);
define_ceil_div!(ceil_div_u32, u32);
define_ceil_div!(ceil_div_i64, i64);
define_ceil_div!(ceil_div_u64, u64);
define_ceil_div!(ceil_div_usize, usize);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_signed_division_toward_positive_infinity_without_overflow() {
        assert_eq!(ceil_div_i32(-5, 2), -2);
        assert_eq!(ceil_div_i32(5, 2), 3);
        assert_eq!(ceil_div_i32(i32::MAX, 2), 1_073_741_824);
        assert_eq!(ceil_div_u32(u32::MAX, 2), 2_147_483_648);
    }
}

#[cfg(test)]
#[path = "fx_ceil_div_unittest.rs"]
mod upstream_tests;
