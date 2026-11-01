// Copyright 2017 The PDFium Authors
// Use of this source code is governed by the BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Decimal conversion from `core/fxcrt/fx_system.cpp`. Inputs are bounded
//! character slices; a NUL terminates the numeric prefix just as in C strings.

use std::sync::atomic::{AtomicU32, Ordering};

// The portable C++ implementation stores last error process-wide. Atomics
// keep that storage safe without changing the API's serialized-use contract.
static LAST_ERROR: AtomicU32 = AtomicU32::new(0);

pub fn set_last_error(error: u32) {
    LAST_ERROR.store(error, Ordering::Relaxed);
}

pub fn get_last_error() -> u32 {
    LAST_ERROR.load(Ordering::Relaxed)
}

fn magnitude<T: Copy + Into<u32>>(text: &[T], maximum: u64) -> (bool, Option<u64>) {
    let first = text.first().copied().map(Into::into);
    let negative = first == Some(u32::from(b'-'));
    let mut digits = text;
    if negative || first == Some(u32::from(b'+')) {
        digits = &text[1..];
    }
    let mut number = 0u64;
    for &character in digits {
        let digit = character.into();
        if !(u32::from(b'0')..=u32::from(b'9')).contains(&digit) {
            break;
        }
        let digit = u64::from(digit - u32::from(b'0'));
        if number > (maximum - digit) / 10 {
            return (negative, None);
        }
        number = number * 10 + digit;
    }
    (negative, Some(number))
}

fn signed<T: Copy + Into<u32>>(text: &[T], minimum: i64, maximum: i64) -> i64 {
    let (negative, number) = magnitude(text, maximum as u64);
    match number {
        None => {
            if negative {
                minimum
            } else {
                maximum
            }
        }
        Some(number) => {
            if negative {
                -(number as i64)
            } else {
                number as i64
            }
        }
    }
}

/// FXSYS_atoi does not skip whitespace or repeated signs.
pub fn atoi(text: &[u8]) -> i32 {
    signed(text, i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

pub fn wtoi(text: &[u32]) -> i32 {
    signed(text, i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

pub fn atoi64(text: &[u8]) -> i64 {
    signed(text, i64::MIN, i64::MAX)
}

pub fn atoui(text: &[u8]) -> u32 {
    let (negative, number) = magnitude(text, u64::from(u32::MAX));
    match number {
        None => u32::MAX,
        Some(number) => {
            if negative {
                (number as u32).wrapping_neg()
            } else {
                number as u32
            }
        }
    }
}

// FXSYS_IntToStr: keep the native digit order and NUL terminator over a safe
// output slice. Callers must supply room for the sign, digits and terminator.
fn int_to_str(value: i64, output: &mut [u8], radix: i32) -> &[u8] {
    if !(2..=16).contains(&radix) {
        output[0] = 0;
        return &output[..0];
    }
    if value == 0 {
        output[0] = b'0';
        output[1] = 0;
        return &output[..1];
    }
    let mut index = 0;
    let mut uvalue = if value < 0 {
        output[index] = b'-';
        index += 1;
        // The same INT_MIN-safe magnitude as -(value + 1) + 1 upstream.
        (-(value + 1)) as u64 + 1
    } else {
        value as u64
    };
    let radix = radix as u64;
    let mut digits = 1;
    let mut order = uvalue / radix;
    while order > 0 {
        digits += 1;
        order /= radix;
    }
    for digit in (0..digits).rev() {
        output[digit + index] = b"0123456789abcdef"[(uvalue % radix) as usize];
        uvalue /= radix;
    }
    output[digits + index] = 0;
    &output[..digits + index]
}

pub fn itoa(value: i32, output: &mut [u8], radix: i32) -> &[u8] {
    int_to_str(i64::from(value), output, radix)
}

pub fn i64toa(value: i64, output: &mut [u8], radix: i32) -> &[u8] {
    int_to_str(value, output, radix)
}

/// Native C-string case conversion stops at the first NUL; bytes beyond it
/// remain part of a ByteString's logical length and must retain their case.
pub fn strlwr(text: &mut [u8]) {
    let end = text
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(text.len());
    text[..end].make_ascii_lowercase();
}

pub fn strupr(text: &mut [u8]) {
    let end = text
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(text.len());
    text[..end].make_ascii_uppercase();
}

pub fn stricmp(left: &[u8], right: &[u8]) -> i32 {
    let mut index = 0;
    loop {
        let left = left.get(index).copied().unwrap_or(0).to_ascii_uppercase();
        let right = right.get(index).copied().unwrap_or(0).to_ascii_uppercase();
        if left == 0 || left != right {
            return i32::from(left) - i32::from(right);
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_decimal_prefixes_saturate_before_applying_the_sign() {
        assert_eq!(atoi(b"-2147483648"), i32::MIN);
        assert_eq!(atoi(b"2147483648tail"), i32::MAX);
        assert_eq!(atoi64(b"-9223372036854775808"), i64::MIN);
        assert_eq!(atoi64(b"-9223372036854775810"), i64::MIN);
        assert_eq!(atoi64(b"9223372036854775808"), i64::MAX);
        assert_eq!(atoui(b"-1"), u32::MAX);
        assert_eq!(atoui(b"4294967296"), u32::MAX);
        assert_eq!(atoui(b"-4294967345"), u32::MAX);
        assert_eq!(atoi(b" 12"), 0);
        assert_eq!(atoi(b"--12"), 0);
        assert_eq!(atoi(b"12\0more"), 12);
        assert_eq!(wtoi(&[b'9' as u32, 0x3132, b'8' as u32]), 9);
    }
}

#[cfg(test)]
#[path = "fx_system_unittest.rs"]
mod upstream_tests;
