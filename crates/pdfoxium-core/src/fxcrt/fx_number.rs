// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcrt/fx_number.h` and `.cpp`.

use super::fx_string::string_to_float;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FxNumber {
    Unsigned(u32),
    Signed(i32),
    Float(f32),
}

impl Default for FxNumber {
    fn default() -> Self {
        Self::Unsigned(0)
    }
}

impl FxNumber {
    pub fn from_signed(value: i32) -> Self {
        Self::Signed(value)
    }
    pub fn from_float(value: f32) -> Self {
        Self::Float(value)
    }

    pub fn from_string(text: &[u8]) -> Self {
        if text.is_empty() {
            return Self::default();
        }
        if text.contains(&b'.') {
            return Self::Float(string_to_float(text));
        }

        // Encryption permission values can use the entire unsigned range.
        let mut digits = text;
        let mut signed = false;
        let mut negative = false;
        if matches!(text[0], b'+' | b'-') {
            signed = true;
            negative = text[0] == b'-';
            digits = &text[1..];
        }
        let mut value = Some(0u32);
        for &digit in digits {
            if !digit.is_ascii_digit() {
                break;
            }
            value = value
                .and_then(|number| number.checked_mul(10))
                .and_then(|number| number.checked_add(u32::from(digit - b'0')));
        }
        let mut value = value.unwrap_or(0);
        if !signed {
            return Self::Unsigned(value);
        }
        let limit = i32::MAX as u32;
        if value > if negative { limit + 1 } else { limit } {
            value = 0;
        }
        let signed_value = value as i32;
        Self::Signed(if negative && signed_value != i32::MIN {
            -signed_value
        } else {
            signed_value
        })
    }

    pub fn is_integer(self) -> bool {
        !matches!(self, Self::Float(_))
    }
    pub fn is_signed(self) -> bool {
        !matches!(self, Self::Unsigned(_))
    }

    pub fn get_signed(self) -> i32 {
        match self {
            Self::Unsigned(value) => value as i32,
            Self::Signed(value) => value,
            Self::Float(value) => value as i32,
        }
    }

    pub fn get_float(self) -> f32 {
        match self {
            Self::Unsigned(value) => value as f32,
            Self::Signed(value) => value as f32,
            Self::Float(value) => value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FxNumber;

    #[test]
    fn parses_signed_unsigned_float_and_overflow_paths() {
        let unsigned = FxNumber::from_string(b"4294967295");
        assert!(unsigned.is_integer());
        assert!(!unsigned.is_signed());
        assert_eq!(unsigned.get_signed(), -1);
        assert_eq!(FxNumber::from_string(b"4294967296").get_signed(), 0);
        assert_eq!(FxNumber::from_string(b"-2147483648").get_signed(), i32::MIN);
        assert_eq!(FxNumber::from_string(b"-2147483649").get_signed(), 0);
        assert_eq!(FxNumber::from_string(b"+2147483648").get_signed(), 0);
        let real = FxNumber::from_string(b"3.24");
        assert!(!real.is_integer());
        assert!(real.is_signed());
        assert_eq!(real.get_float(), 3.24f32);
        assert_eq!(FxNumber::from_float(1e17).get_signed(), i32::MAX);
        assert_eq!(FxNumber::from_float(-1e17).get_signed(), i32::MIN);
    }
}

#[cfg(test)]
#[path = "fx_number_unittest.rs"]
mod upstream_tests;
