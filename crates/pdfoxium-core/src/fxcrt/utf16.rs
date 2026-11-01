// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fxcrt/utf16.h`.

pub const SURROGATE_BITS: u32 = 10;
pub const SURROGATE_MASK: u16 = (1 << SURROGATE_BITS) - 1;
pub const MINIMUM_SUPPLEMENTARY_CODE_POINT: u32 = 0x10000;
pub const MAXIMUM_SUPPLEMENTARY_CODE_POINT: u32 = MINIMUM_SUPPLEMENTARY_CODE_POINT
    + ((SURROGATE_MASK as u32) << SURROGATE_BITS | SURROGATE_MASK as u32);
pub const MINIMUM_HIGH_SURROGATE_CODE_UNIT: u16 = 0xd800;
pub const MAXIMUM_HIGH_SURROGATE_CODE_UNIT: u16 = MINIMUM_HIGH_SURROGATE_CODE_UNIT | SURROGATE_MASK;
pub const MINIMUM_LOW_SURROGATE_CODE_UNIT: u16 = MAXIMUM_HIGH_SURROGATE_CODE_UNIT + 1;
pub const MAXIMUM_LOW_SURROGATE_CODE_UNIT: u16 = MINIMUM_LOW_SURROGATE_CODE_UNIT | SURROGATE_MASK;

pub const fn is_supplementary(code_point: u32) -> bool {
    code_point >= MINIMUM_SUPPLEMENTARY_CODE_POINT && code_point <= MAXIMUM_SUPPLEMENTARY_CODE_POINT
}

pub const fn is_high_surrogate(code_point: u32) -> bool {
    code_point >= MINIMUM_HIGH_SURROGATE_CODE_UNIT as u32
        && code_point <= MAXIMUM_HIGH_SURROGATE_CODE_UNIT as u32
}

pub const fn is_low_surrogate(code_point: u32) -> bool {
    code_point >= MINIMUM_LOW_SURROGATE_CODE_UNIT as u32
        && code_point <= MAXIMUM_LOW_SURROGATE_CODE_UNIT as u32
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurrogatePair {
    high: u16,
    low: u16,
}

impl SurrogatePair {
    pub const fn new(high: u16, low: u16) -> Self {
        debug_assert!(is_high_surrogate(high as u32));
        debug_assert!(is_low_surrogate(low as u32));
        Self { high, low }
    }

    pub const fn from_code_point(code_point: u32) -> Self {
        debug_assert!(is_supplementary(code_point));
        let value = code_point - MINIMUM_SUPPLEMENTARY_CODE_POINT;
        let high =
            MINIMUM_HIGH_SURROGATE_CODE_UNIT | ((value >> SURROGATE_BITS) as u16 & SURROGATE_MASK);
        let low = MINIMUM_LOW_SURROGATE_CODE_UNIT | (value as u16 & SURROGATE_MASK);
        Self { high, low }
    }

    pub const fn high(&self) -> u16 {
        self.high
    }

    pub const fn low(&self) -> u16 {
        self.low
    }

    pub const fn to_code_point(self) -> u32 {
        let mut code_point = (self.low & SURROGATE_MASK) as u32;
        code_point |= ((self.high & SURROGATE_MASK) as u32) << SURROGATE_BITS;
        MINIMUM_SUPPLEMENTARY_CODE_POINT + code_point
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SurrogatePair, MAXIMUM_SUPPLEMENTARY_CODE_POINT, MINIMUM_SUPPLEMENTARY_CODE_POINT,
    };

    #[test]
    fn surrogate_round_trip_at_boundaries() {
        for point in [
            MINIMUM_SUPPLEMENTARY_CODE_POINT,
            0x1f3a8,
            MAXIMUM_SUPPLEMENTARY_CODE_POINT,
        ] {
            let pair = SurrogatePair::from_code_point(point);
            assert_eq!(
                SurrogatePair::new(pair.high(), pair.low()).to_code_point(),
                point
            );
        }
    }
}

#[cfg(test)]
#[path = "utf16_unittest.rs"]
mod upstream_tests;
