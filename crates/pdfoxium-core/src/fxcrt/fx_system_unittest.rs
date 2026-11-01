// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2015 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/fx_system_unittest.cpp`, selected portable
//! numeric behavior. Native round/roundf are represented by round() as i32 at
//! production call sites; Rust's cast saturates and maps NaN to zero.
#![allow(non_snake_case)]
use super::*;
fn primitive_roundf(value: f32) -> i32 {
    value.round() as i32
}
fn primitive_round(value: f64) -> i32 {
    value.round() as i32
}
fn cstr(buffer: &[u8]) -> &[u8] {
    &buffer[..buffer
        .iter()
        .position(|&ch| ch == 0)
        .expect("NUL terminator")]
}

fn Check32BitBase16Itoa(input: i32, expected_output: &str) {
    const BUF_LEN: usize = 11;
    const SENTINEL: u8 = 0x7f;
    let mut buf = [0u8; BUF_LEN];
    buf[BUF_LEN - 1] = SENTINEL;
    itoa(input, &mut buf, 16);
    assert_eq!(expected_output.as_bytes(), cstr(&buf));
    assert_eq!(SENTINEL, buf[BUF_LEN - 1]);
}

fn Check32BitBase10Itoa(input: i32, expected_output: &str) {
    const BUF_LEN: usize = 13;
    const SENTINEL: u8 = 0x7f;
    let mut buf = [0u8; BUF_LEN];
    buf[BUF_LEN - 1] = SENTINEL;
    itoa(input, &mut buf, 10);
    assert_eq!(expected_output.as_bytes(), cstr(&buf));
    assert_eq!(SENTINEL, buf[BUF_LEN - 1]);
}

fn Check32BitBase2Itoa(input: i32, expected_output: &str) {
    const BUF_LEN: usize = 35;
    const SENTINEL: u8 = 0x7f;
    let mut buf = [0u8; BUF_LEN];
    buf[BUF_LEN - 1] = SENTINEL;
    itoa(input, &mut buf, 2);
    assert_eq!(expected_output.as_bytes(), cstr(&buf));
    assert_eq!(SENTINEL, buf[BUF_LEN - 1]);
}

fn Check64BitBase16Itoa(input: i64, expected_output: &str) {
    const BUF_LEN: usize = 19;
    const SENTINEL: u8 = 0x7f;
    let mut buf = [0u8; BUF_LEN];
    buf[BUF_LEN - 1] = SENTINEL;
    i64toa(input, &mut buf, 16);
    assert_eq!(expected_output.as_bytes(), cstr(&buf));
    assert_eq!(SENTINEL, buf[BUF_LEN - 1]);
}

fn Check64BitBase10Itoa(input: i64, expected_output: &str) {
    const BUF_LEN: usize = 22;
    const SENTINEL: u8 = 0x7f;
    let mut buf = [0u8; BUF_LEN];
    buf[BUF_LEN - 1] = SENTINEL;
    i64toa(input, &mut buf, 10);
    assert_eq!(expected_output.as_bytes(), cstr(&buf));
    assert_eq!(SENTINEL, buf[BUF_LEN - 1]);
}

fn Check64BitBase2Itoa(input: i64, expected_output: &str) {
    const BUF_LEN: usize = 67;
    const SENTINEL: u8 = 0x7f;
    let mut buf = [0u8; BUF_LEN];
    buf[BUF_LEN - 1] = SENTINEL;
    i64toa(input, &mut buf, 2);
    assert_eq!(expected_output.as_bytes(), cstr(&buf));
    assert_eq!(SENTINEL, buf[BUF_LEN - 1]);
}

#[test]
fn FXSYSroundf() {
    assert_eq!(0, primitive_roundf(0.0f32));
    assert_eq!(0, primitive_roundf(-0.0f32));
    assert_eq!(0, primitive_roundf(0.00001f32));
    assert_eq!(0, primitive_roundf(-0.00001f32));
    assert_eq!(3, primitive_roundf(3.14159f32));
    assert_eq!(4, primitive_roundf(3.5f32));

    // Check for smallest non-zero float values.
    assert_eq!(0, primitive_roundf(f32::MIN_POSITIVE));
    assert_eq!(0, primitive_roundf(-f32::MIN_POSITIVE));

    // Function is a wrapper around standard C library function round(), so
    // returns the integral value that is nearest to x, with halfway cases
    // rounded away from zero.
    assert_eq!(-3, primitive_roundf(-3.14159f32));
    assert_eq!(-4, primitive_roundf(-3.5f32));

    // Positive rounding stops at maximum int.
    // MAX_INT=0x7FFFFFFF=2147483647=2.147483647e+9
    // In IEEE-754 format, 2^31 yields exponent of 0x9E with mantissa of all
    // zeroes which is 0x4f000000=2.14748365e+9, which is beyond max integer.
    // Going to next smallest float by minus one from exponent and mantissa of
    // all ones yields binary float representation of 0x4EFFFFFF=2.14748352e+9,
    // which is 2147483520.
    assert_eq!(2147483520, primitive_roundf(2.14748352e+9f32));

    // Using a slightly larger value, expect to see it be capped at MAX_INT.
    assert_eq!(2147483647, primitive_roundf(2.14748365e+9f32));

    assert_eq!(2147483647, primitive_roundf(2.14748365e+10f32));
    assert_eq!(2147483647, primitive_roundf(f32::MAX));

    // Negative rounding stops at minimum int.
    // MIN_INT=0x80000000=-2147483648,=-2.147483648e+9
    // In IEEE-754 format, 2^31 yields exponent of 0x9E with mantissa of all
    // zeroes which is 0x4f000000=2.14748365e+9, and the sign bit set, which
    // is 0xCF000000 and exactly matches the minimum integer.  Going to next
    // smallest negative float by minus one from exponent and mantissa of all
    // ones yields binary float representation of 0xCEFFFFFF=-2.14748352e+9,
    // which is -2147483520.
    assert_eq!(-2147483648, primitive_roundf(-2.147483648e+10f32));
    assert_eq!(-2147483648, primitive_roundf(-2.147483648e+9f32));
    assert_eq!(-2147483520, primitive_roundf(-2.14748352e+9f32));
    assert_eq!(-2147483648, primitive_roundf(-f32::MAX));

    // NaN should give zero.
    assert_eq!(0, primitive_roundf(f32::NAN));
}

#[test]
fn FXSYSround() {
    assert_eq!(0, primitive_round(0.0));
    assert_eq!(0, primitive_round(-0.0));
    assert_eq!(0, primitive_round(0.00001));
    assert_eq!(0, primitive_round(-0.00001));
    assert_eq!(3, primitive_round(3.14159));
    assert_eq!(4, primitive_round(3.5));

    // Check for smallest non-zero double values.
    assert_eq!(0, primitive_round(f64::MIN_POSITIVE));
    assert_eq!(0, primitive_round(-f64::MIN_POSITIVE));

    // Function is a wrapper around standard C library function round(), so
    // returns the integral value that is nearest to x, with halfway cases
    // rounded away from zero.
    assert_eq!(-3, primitive_round(-3.14159));
    assert_eq!(-4, primitive_round(-3.5));

    // Positive rounding stops at maximum int.
    // MAX_INT=0x7FFFFFFF=2147483647=2.147483647e+9
    // In IEEE-754 double precision format, 2^31 yields exponent of 0x41E with
    // mantissa of all zeroes which is 0x41E0000000000000=2.14748365e+9, which
    // is beyond max integer.
    // Going to next smallest float by minus one from exponent and mantissa of
    // all ones yields binary float representation of
    // 41DFFFFFFFC00000=2.147483647e+9, which matches the max integer.
    assert_eq!(2147483647, primitive_round(2.147483647e+9));

    // Using a slightly larger value, expect to see it be capped at MAX_INT.
    assert_eq!(2147483647, primitive_round(2.14748365e+9));

    assert_eq!(2147483647, primitive_round(2.14748365e+10));
    assert_eq!(2147483647, primitive_round(f64::MAX));

    // Negative rounding stops at minimum int.
    // MIN_INT=0x80000000=-2147483648,=-2.147483648e+9
    // In IEEE-754 double precision format, 2^31 yields exponent of 0x41E with
    // mantissa of all zeroes which is 0x41E0000000000000=2.14748365e+9, and the
    // sign bit set, which is 0xC1E0000000000000 and exactly matches the minimum
    // integer.  Going to next smallest negative double by minus one from
    // exponent and mantissa of all ones yields binary float representation of
    // 0xC1DFFFFFFFFFFFFF=-2.1474836479999998e+9, which is -2147483648.
    assert_eq!(-2147483648, primitive_round(-2.1474836479999998e+9));
    assert_eq!(-2147483648, primitive_round(-2.147483648e+9));
    assert_eq!(-2147483648, primitive_round(-2.147483648e+10));
    assert_eq!(-2147483648, primitive_round(-f64::MAX));

    // NaN should give zero.
    assert_eq!(0, primitive_round(f64::NAN));
}

#[test]
fn FXSYSitoaInvalidRadix() {
    let mut buf = [0u8; 32];

    itoa(42, &mut buf, 17); // Ours stops at 16.
    assert_eq!(b"", cstr(&buf));

    itoa(42, &mut buf, 1);
    assert_eq!(b"", cstr(&buf));

    itoa(42, &mut buf, 0);
    assert_eq!(b"", cstr(&buf));

    itoa(42, &mut buf, -1);
    assert_eq!(b"", cstr(&buf));
}

#[test]
fn FXSYSitoa() {
    Check32BitBase16Itoa(i32::MIN, "-80000000");
    Check32BitBase10Itoa(i32::MIN, "-2147483648");
    Check32BitBase2Itoa(i32::MIN, "-10000000000000000000000000000000");

    Check32BitBase16Itoa(-1, "-1");
    Check32BitBase10Itoa(-1, "-1");
    Check32BitBase2Itoa(-1, "-1");

    Check32BitBase16Itoa(0, "0");
    Check32BitBase10Itoa(0, "0");
    Check32BitBase2Itoa(0, "0");

    Check32BitBase16Itoa(42, "2a");
    Check32BitBase10Itoa(42, "42");
    Check32BitBase2Itoa(42, "101010");

    Check32BitBase16Itoa(i32::MAX, "7fffffff");
    Check32BitBase10Itoa(i32::MAX, "2147483647");
    Check32BitBase2Itoa(i32::MAX, "1111111111111111111111111111111");
}

#[test]
fn FXSYSi64toaInvalidRadix() {
    let mut buf = [0u8; 32];

    i64toa(42, &mut buf, 17); // Ours stops at 16.
    assert_eq!(b"", cstr(&buf));

    i64toa(42, &mut buf, 1);
    assert_eq!(b"", cstr(&buf));

    i64toa(42, &mut buf, 0);
    assert_eq!(b"", cstr(&buf));

    i64toa(42, &mut buf, -1);
    assert_eq!(b"", cstr(&buf));
}

#[test]
fn FXSYSi64toa() {
    Check64BitBase16Itoa(i64::MIN, "-8000000000000000");
    Check64BitBase10Itoa(i64::MIN, "-9223372036854775808");
    Check64BitBase2Itoa(
        i64::MIN,
        "-1000000000000000000000000000000000000000000000000000000000000000",
    );

    Check64BitBase16Itoa(-1, "-1");
    Check64BitBase10Itoa(-1, "-1");
    Check64BitBase2Itoa(-1, "-1");

    Check64BitBase16Itoa(0, "0");
    Check64BitBase10Itoa(0, "0");
    Check64BitBase2Itoa(0, "0");

    Check64BitBase16Itoa(42, "2a");
    Check64BitBase10Itoa(42, "42");
    Check64BitBase2Itoa(42, "101010");

    Check64BitBase16Itoa(i64::MAX, "7fffffffffffffff");
    Check64BitBase10Itoa(i64::MAX, "9223372036854775807");
    Check64BitBase2Itoa(
        i64::MAX,
        "111111111111111111111111111111111111111111111111111111111111111",
    );
}

#[test]
fn FXSYSatoi() {
    assert_eq!(0, atoi(b""));
    assert_eq!(0, atoi(b"0"));
    assert_eq!(-1, atoi(b"-1"));
    assert_eq!(2345, atoi(b"2345"));
    assert_eq!(-2147483647, atoi(b"-2147483647"));
    // Handle the sign.
    assert_eq!(-2345, atoi(b"-2345"));
    assert_eq!(2345, atoi(b"+2345"));
    // The max value.
    assert_eq!(2147483647, atoi(b"2147483647"));
    // The min value. Written in -1 format to avoid "unary minus operator applied
    // to unsigned type" warning.
    assert_eq!(-2147483647 - 1, atoi(b"-2147483648"));
    // With invalid char.
    assert_eq!(9, atoi(b"9x9"));

    // Out of range values.
    assert_eq!(2147483647, atoi(b"2147483623423412348"));
    assert_eq!(2147483647, atoi(b"2147483648"));
    assert_eq!(-2147483647 - 1, atoi(b"-2147483650"));
}

#[test]
fn FXSYSatoi64() {
    assert_eq!(0, atoi64(b""));
    assert_eq!(0, atoi64(b"0"));
    assert_eq!(-1, atoi64(b"-1"));
    assert_eq!(2345, atoi64(b"2345"));
    assert_eq!(-9223372036854775807i64, atoi64(b"-9223372036854775807"));
    // Handle the sign.
    assert_eq!(-2345, atoi64(b"-2345"));
    assert_eq!(2345, atoi64(b"+2345"));
    // The max value.
    assert_eq!(9223372036854775807i64, atoi64(b"9223372036854775807"));
    // The min value. Written in -1 format to avoid implicit unsigned warning.
    assert_eq!(
        -9223372036854775807i64 - 1i64,
        atoi64(b"-9223372036854775808")
    );
    // With invalid char.
    assert_eq!(9, atoi64(b"9x9"));

    // Out of range values.
    assert_eq!(
        9223372036854775807i64,
        atoi64(b"922337203685471234123475807")
    );
    assert_eq!(9223372036854775807i64, atoi64(b"9223372036854775808"));
    assert_eq!(
        -9223372036854775807i64 - 1i64,
        atoi64(b"-9223372036854775810")
    );
}

#[test]
fn FXSYSwtoi() {
    assert_eq!(0, wtoi(&[]));
    assert_eq!(0, wtoi(&[48]));
    assert_eq!(-1, wtoi(&[45, 49]));
    assert_eq!(2345, wtoi(&[50, 51, 52, 53]));
    assert_eq!(
        -2147483647,
        wtoi(&[45, 50, 49, 52, 55, 52, 56, 51, 54, 52, 55])
    );
    // The max value.
    assert_eq!(2147483647, wtoi(&[50, 49, 52, 55, 52, 56, 51, 54, 52, 55]));
    // The min value.
    assert_eq!(
        -2147483647 - 1,
        wtoi(&[45, 50, 49, 52, 55, 52, 56, 51, 54, 52, 56])
    );

    // Out of range values.
    assert_eq!(
        2147483647,
        wtoi(&[50, 49, 52, 55, 52, 56, 51, 54, 50, 51, 52, 50, 51, 52, 49, 50, 51, 52, 56])
    );
    assert_eq!(2147483647, wtoi(&[50, 49, 52, 55, 52, 56, 51, 54, 52, 56]));
    assert_eq!(
        -2147483647 - 1,
        wtoi(&[45, 50, 49, 52, 55, 52, 56, 51, 54, 53, 50])
    );
}

#[test]
fn FXSYSatoui() {
    assert_eq!(0u32, atoui(b""));
    assert_eq!(0u32, atoui(b"0"));
    assert_eq!(4294967295, atoui(b"-1"));
    assert_eq!(2345u32, atoui(b"2345"));
    // Handle the sign.
    assert_eq!(4294964951, atoui(b"-2345"));
    assert_eq!(2345u32, atoui(b"+2345"));
    // The max value.
    assert_eq!(4294967295, atoui(b"4294967295"));
    assert_eq!(9u32, atoui(b"9x9"));

    // Out of range values.
    assert_eq!(4294967295, atoui(b"2147483623423412348"));
    assert_eq!(4294967295, atoui(b"4294967296"));
    assert_eq!(4294967295, atoui(b"-4294967345"));
}
