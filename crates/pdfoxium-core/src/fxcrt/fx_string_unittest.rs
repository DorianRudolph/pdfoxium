// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/fx_string_unittest.cpp` (wchar_t = 32).

#![allow(non_snake_case, non_upper_case_globals, clippy::excessive_precision)]

use super::*;
use crate::test_support::{assert_double_eq, assert_float_eq};

fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}

#[test]
fn FXUTF8Encode() {
    assert_eq!("".as_bytes(), utf8_encode(&[]));
    assert_eq!(
        "x\u{80}\u{ff}\u{d7ff}\u{e000}\u{ff2c}\u{ffff}y".as_bytes(),
        utf8_encode(&wide("x\u{80}\u{ff}\u{d7ff}\u{e000}\u{ff2c}\u{ffff}y"))
    );
}

#[test]
fn FXUTF8EncodeSupplementary() {
    assert_eq!(
        "\u{10000}🎨\u{10ffff}".as_bytes(),
        utf8_encode(&wide("\u{10000}\u{1f3a8}\u{10ffff}"))
    );
}

#[test]
fn ByteStringToInt() {
    assert_eq!(0, string_to_int("".as_bytes()));
    assert_eq!(0, string_to_int("0".as_bytes()));
    assert_eq!(-1, string_to_int("-1".as_bytes()));
    assert_eq!(2345, string_to_int("+2345".as_bytes()));
    assert_eq!(2345, string_to_int("2345".as_bytes()));
    assert_eq!(-2147483647, string_to_int("-2147483647".as_bytes()));
    assert_eq!(2147483647, string_to_int("2147483647".as_bytes()));
    assert_eq!(-2147483647 - 1, string_to_int("-2147483648".as_bytes()));
    assert_eq!(2147483647, string_to_int("2147483623423412348".as_bytes()));
    assert_eq!(2147483647, string_to_int("2147483648".as_bytes()));
    assert_eq!(-2147483647 - 1, string_to_int("-2147483652".as_bytes()));
    assert_eq!(0, string_to_int("-".as_bytes()));
    assert_eq!(0, string_to_int("c--".as_bytes()));
    assert_eq!(0, string_to_int("--i".as_bytes()));
    assert_eq!(0, string_to_int("+".as_bytes()));
    assert_eq!(0, string_to_int("c++".as_bytes()));
    assert_eq!(0, string_to_int("++i".as_bytes()));
    assert_eq!(1, string_to_int("1beef".as_bytes()));
    assert_eq!(0, string_to_int("beef1".as_bytes()));
    assert_eq!(0, string_to_int("b33f".as_bytes()));
    assert_eq!(33, string_to_int("33f".as_bytes()));
    assert_eq!(-100, string_to_int("--100".as_bytes()));
    assert_eq!(-100, string_to_int("+-100".as_bytes()));
    assert_eq!(100, string_to_int("++100".as_bytes()));
}

#[test]
fn WideStringToInt() {
    assert_eq!(0, string_to_int_wide(&wide("")));
    assert_eq!(0, string_to_int_wide(&wide("0")));
    assert_eq!(-1, string_to_int_wide(&wide("-1")));
    assert_eq!(2345, string_to_int_wide(&wide("2345")));
    assert_eq!(2345, string_to_int_wide(&wide("+2345")));
    assert_eq!(-2147483647, string_to_int_wide(&wide("-2147483647")));
    assert_eq!(2147483647, string_to_int_wide(&wide("2147483647")));
    assert_eq!(-2147483647 - 1, string_to_int_wide(&wide("-2147483648")));
    assert_eq!(2147483647, string_to_int_wide(&wide("2147483623423412348")));
    assert_eq!(2147483647, string_to_int_wide(&wide("2147483648")));
    assert_eq!(-2147483647 - 1, string_to_int_wide(&wide("-2147483652")));
    assert_eq!(0, string_to_int_wide(&wide("-")));
    assert_eq!(0, string_to_int_wide(&wide("c--")));
    assert_eq!(0, string_to_int_wide(&wide("--i")));
    assert_eq!(0, string_to_int_wide(&wide("+")));
    assert_eq!(0, string_to_int_wide(&wide("c++")));
    assert_eq!(0, string_to_int_wide(&wide("++i")));
    assert_eq!(1, string_to_int_wide(&wide("1beef")));
    assert_eq!(0, string_to_int_wide(&wide("beef1")));
    assert_eq!(0, string_to_int_wide(&wide("b33f")));
    assert_eq!(33, string_to_int_wide(&wide("33f")));
    assert_eq!(-100, string_to_int_wide(&wide("--100")));
    assert_eq!(-100, string_to_int_wide(&wide("+-100")));
    assert_eq!(100, string_to_int_wide(&wide("++100")));
}

#[test]
fn ByteStringToFloat() {
    assert_float_eq(0.0f32, string_to_float("".as_bytes()));
    assert_float_eq(0.0f32, string_to_float("0".as_bytes()));
    assert_float_eq(0.0f32, string_to_float("0.0".as_bytes()));
    assert_float_eq(0.0f32, string_to_float("-0.0".as_bytes()));
    assert_float_eq(0.25f32, string_to_float("0.25".as_bytes()));
    assert_float_eq(0.25f32, string_to_float("+0.25".as_bytes()));
    assert_float_eq(-0.25f32, string_to_float("-0.25".as_bytes()));
    assert_float_eq(100.0f32, string_to_float("100".as_bytes()));
    assert_float_eq(100.0f32, string_to_float("100.0".as_bytes()));
    assert_float_eq(100.0f32, string_to_float("    100.0".as_bytes()));
    assert_float_eq(-100.0f32, string_to_float("-100.0000".as_bytes()));
    assert_float_eq(-100.0f32, string_to_float("--100.0".as_bytes()));
    assert_float_eq(-100.0f32, string_to_float("+-100.0".as_bytes()));
    assert_float_eq(100.0f32, string_to_float("++100.0".as_bytes()));
    assert_float_eq(
        3.402823e+38f32,
        string_to_float("340282300000000000000000000000000000000".as_bytes()),
    );
    assert_float_eq(
        -3.402823e+38f32,
        string_to_float("-340282300000000000000000000000000000000".as_bytes()),
    );
    assert_float_eq(1.000000119f32, string_to_float("1.000000119".as_bytes()));
    assert_float_eq(1.999999881f32, string_to_float("1.999999881".as_bytes()));
    assert_float_eq(0.25f32, string_to_float_wide(&wide("+0.25")));
    assert_float_eq(1.2e34f32, string_to_float("1.2e34".as_bytes()));
    assert_float_eq(
        f32::INFINITY,
        string_to_float("999999999999999999999999999999999999999".as_bytes()),
    );
    assert_float_eq(
        -f32::INFINITY,
        string_to_float("-999999999999999999999999999999999999999".as_bytes()),
    );
    assert_float_eq(0.0f32, string_to_float("invalid".as_bytes()));
    assert_eq!(
        38.89528656005859375f32,
        string_to_float("38.895285".as_bytes())
    );
}

#[test]
fn WideStringToFloat() {
    assert_float_eq(0.0f32, string_to_float_wide(&wide("")));
    assert_float_eq(0.0f32, string_to_float_wide(&wide("0")));
    assert_float_eq(0.0f32, string_to_float_wide(&wide("0.0")));
    assert_float_eq(0.0f32, string_to_float_wide(&wide("-0.0")));
    assert_float_eq(0.25f32, string_to_float_wide(&wide("0.25")));
    assert_float_eq(0.25f32, string_to_float_wide(&wide("+0.25")));
    assert_float_eq(-0.25f32, string_to_float_wide(&wide("-0.25")));
    assert_float_eq(100.0f32, string_to_float_wide(&wide("100")));
    assert_float_eq(100.0f32, string_to_float_wide(&wide("100.0")));
    assert_float_eq(100.0f32, string_to_float_wide(&wide("    100.0")));
    assert_float_eq(-100.0f32, string_to_float_wide(&wide("-100.0000")));
    assert_float_eq(-100.0f32, string_to_float_wide(&wide("--100.0")));
    assert_float_eq(-100.0f32, string_to_float_wide(&wide("+-100.0")));
    assert_float_eq(100.0f32, string_to_float_wide(&wide("++100.0")));
    assert_float_eq(
        3.402823e+38f32,
        string_to_float_wide(&wide("340282300000000000000000000000000000000")),
    );
    assert_float_eq(
        -3.402823e+38f32,
        string_to_float_wide(&wide("-340282300000000000000000000000000000000")),
    );
    assert_float_eq(1.000000119f32, string_to_float_wide(&wide("1.000000119")));
    assert_float_eq(1.999999881f32, string_to_float_wide(&wide("1.999999881")));
    assert_float_eq(0.25f32, string_to_float_wide(&wide("+0.25")));
    assert_float_eq(1.2e34f32, string_to_float_wide(&wide("1.2e34")));
    assert_float_eq(
        f32::INFINITY,
        string_to_float_wide(&wide("999999999999999999999999999999999999999")),
    );
    assert_float_eq(
        -f32::INFINITY,
        string_to_float_wide(&wide("-999999999999999999999999999999999999999")),
    );
    assert_float_eq(0.0f32, string_to_float_wide(&wide("invalid")));
    assert_float_eq(
        123.0f32,
        string_to_float_wide(&wide("123\u{3132}\u{6162}.5")),
    );
    assert_float_eq(0.0f32, string_to_float_wide(&wide("🎨987")));
    assert_eq!(
        38.89528656005859375f32,
        string_to_float_wide(&wide("38.895285"))
    );
}

#[test]
fn ByteStringToDouble() {
    assert_double_eq(0.0, string_to_double("".as_bytes()));
    assert_double_eq(0.0, string_to_double("0".as_bytes()));
    assert_double_eq(0.0, string_to_double("0.0".as_bytes()));
    assert_double_eq(0.0, string_to_double("-0.0".as_bytes()));
    assert_double_eq(0.25, string_to_double("0.25".as_bytes()));
    assert_double_eq(-0.25, string_to_double("-0.25".as_bytes()));
    assert_double_eq(100.0, string_to_double("100".as_bytes()));
    assert_double_eq(100.0, string_to_double("100.0".as_bytes()));
    assert_double_eq(100.0, string_to_double("    100.0".as_bytes()));
    assert_double_eq(-100.0, string_to_double("-100.0000".as_bytes()));
    assert_double_eq(
        3.402823e+38,
        string_to_double("340282300000000000000000000000000000000".as_bytes()),
    );
    assert_double_eq(
        -3.402823e+38,
        string_to_double("-340282300000000000000000000000000000000".as_bytes()),
    );
    assert_double_eq(1.000000119, string_to_double("1.000000119".as_bytes()));
    assert_double_eq(1.999999881, string_to_double("1.999999881".as_bytes()));
    assert_double_eq(0.25, string_to_double("+0.25".as_bytes()));
    assert_double_eq(1.2e34, string_to_double("1.2e34".as_bytes()));
    assert_double_eq(f64::INFINITY, string_to_double("9999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999".as_bytes()));
    assert_double_eq(-f64::INFINITY, string_to_double("-999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999".as_bytes()));
    assert_double_eq(0.0, string_to_double("invalid".as_bytes()));
    assert_eq!(
        1.9998779296892903,
        string_to_double("1.99987792968929034".as_bytes())
    );
}

#[test]
fn WideStringToDouble() {
    assert_double_eq(0.0, string_to_double_wide(&wide("")));
    assert_double_eq(0.0, string_to_double_wide(&wide("0")));
    assert_double_eq(0.0, string_to_double_wide(&wide("0.0")));
    assert_double_eq(0.0, string_to_double_wide(&wide("-0.0")));
    assert_double_eq(0.25, string_to_double_wide(&wide("0.25")));
    assert_double_eq(-0.25, string_to_double_wide(&wide("-0.25")));
    assert_double_eq(100.0, string_to_double_wide(&wide("100")));
    assert_double_eq(100.0, string_to_double_wide(&wide("100.0")));
    assert_double_eq(100.0, string_to_double_wide(&wide("    100.0")));
    assert_double_eq(-100.0, string_to_double_wide(&wide("-100.0000")));
    assert_double_eq(
        3.402823e+38,
        string_to_double_wide(&wide("340282300000000000000000000000000000000")),
    );
    assert_double_eq(
        -3.402823e+38,
        string_to_double_wide(&wide("-340282300000000000000000000000000000000")),
    );
    assert_double_eq(1.000000119, string_to_double_wide(&wide("1.000000119")));
    assert_double_eq(1.999999881, string_to_double_wide(&wide("1.999999881")));
    assert_double_eq(0.25, string_to_double_wide(&wide("+0.25")));
    assert_double_eq(1.2e34, string_to_double_wide(&wide("1.2e34")));
    assert_double_eq(f64::INFINITY, string_to_double_wide(&wide("9999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999")));
    assert_double_eq(-f64::INFINITY, string_to_double_wide(&wide("-999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999")));
    assert_double_eq(0.0, string_to_double_wide(&wide("invalid")));
    assert_float_eq(
        123.0,
        (string_to_double_wide(&wide("123\u{3132}\u{6162}.5"))) as f32,
    );
    assert_float_eq(0.0, (string_to_double_wide(&wide("🎨987"))) as f32);
    assert_eq!(
        1.9998779296892903,
        string_to_double_wide(&wide("1.99987792968929034"))
    );
}

#[test]
fn SplitByteString() {
    let mut result: Vec<Vec<u8>>;
    result = ""
        .as_bytes()
        .split(|&c| c == b',')
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(1, result.len());
    assert_eq!("".as_bytes(), &result[0][..]);
    result = "a"
        .as_bytes()
        .split(|&c| c == b',')
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(1, result.len());
    assert_eq!("a".as_bytes(), &result[0][..]);
    result = ","
        .as_bytes()
        .split(|&c| c == b',')
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(2, result.len());
    assert_eq!("".as_bytes(), &result[0][..]);
    assert_eq!("".as_bytes(), &result[1][..]);
    result = "a,"
        .as_bytes()
        .split(|&c| c == b',')
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(2, result.len());
    assert_eq!("a".as_bytes(), &result[0][..]);
    assert_eq!("".as_bytes(), &result[1][..]);
    result = ",b"
        .as_bytes()
        .split(|&c| c == b',')
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(2, result.len());
    assert_eq!("".as_bytes(), &result[0][..]);
    assert_eq!("b".as_bytes(), &result[1][..]);
    result = "a,b"
        .as_bytes()
        .split(|&c| c == b',')
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(2, result.len());
    assert_eq!("a".as_bytes(), &result[0][..]);
    assert_eq!("b".as_bytes(), &result[1][..]);
    result = "a,b,"
        .as_bytes()
        .split(|&c| c == b',')
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(3, result.len());
    assert_eq!("a".as_bytes(), &result[0][..]);
    assert_eq!("b".as_bytes(), &result[1][..]);
    assert_eq!("".as_bytes(), &result[2][..]);
    result = "a,,"
        .as_bytes()
        .split(|&c| c == b',')
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(3, result.len());
    assert_eq!("a".as_bytes(), &result[0][..]);
    assert_eq!("".as_bytes(), &result[1][..]);
    assert_eq!("".as_bytes(), &result[2][..]);
    result = ",,a"
        .as_bytes()
        .split(|&c| c == b',')
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(3, result.len());
    assert_eq!("".as_bytes(), &result[0][..]);
    assert_eq!("".as_bytes(), &result[1][..]);
    assert_eq!("a".as_bytes(), &result[2][..]);
}

#[test]
fn SplitByteStringView() {
    let mut result: Vec<&[u8]>;
    result = "".as_bytes().split(|&c| c == b',').collect();
    assert_eq!(1, result.len());
    assert_eq!("".as_bytes(), &result[0][..]);
    result = "a".as_bytes().split(|&c| c == b',').collect();
    assert_eq!(1, result.len());
    assert_eq!("a".as_bytes(), &result[0][..]);
    result = ",".as_bytes().split(|&c| c == b',').collect();
    assert_eq!(2, result.len());
    assert_eq!("".as_bytes(), &result[0][..]);
    assert_eq!("".as_bytes(), &result[1][..]);
    result = "a,".as_bytes().split(|&c| c == b',').collect();
    assert_eq!(2, result.len());
    assert_eq!("a".as_bytes(), &result[0][..]);
    assert_eq!("".as_bytes(), &result[1][..]);
    result = ",b".as_bytes().split(|&c| c == b',').collect();
    assert_eq!(2, result.len());
    assert_eq!("".as_bytes(), &result[0][..]);
    assert_eq!("b".as_bytes(), &result[1][..]);
    result = "a,b".as_bytes().split(|&c| c == b',').collect();
    assert_eq!(2, result.len());
    assert_eq!("a".as_bytes(), &result[0][..]);
    assert_eq!("b".as_bytes(), &result[1][..]);
    result = "a,b,".as_bytes().split(|&c| c == b',').collect();
    assert_eq!(3, result.len());
    assert_eq!("a".as_bytes(), &result[0][..]);
    assert_eq!("b".as_bytes(), &result[1][..]);
    assert_eq!("".as_bytes(), &result[2][..]);
    result = "a,,".as_bytes().split(|&c| c == b',').collect();
    assert_eq!(3, result.len());
    assert_eq!("a".as_bytes(), &result[0][..]);
    assert_eq!("".as_bytes(), &result[1][..]);
    assert_eq!("".as_bytes(), &result[2][..]);
    result = ",,a".as_bytes().split(|&c| c == b',').collect();
    assert_eq!(3, result.len());
    assert_eq!("".as_bytes(), &result[0][..]);
    assert_eq!("".as_bytes(), &result[1][..]);
    assert_eq!("a".as_bytes(), &result[2][..]);
}

#[test]
fn SplitWideString() {
    let mut result: Vec<Vec<u32>>;
    result = wide("")
        .split(|&c| c == u32::from(b','))
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(1, result.len());
    assert_eq!(wide("").as_slice(), &result[0][..]);
    result = wide("a")
        .split(|&c| c == u32::from(b','))
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(1, result.len());
    assert_eq!(wide("a").as_slice(), &result[0][..]);
    result = wide(",")
        .split(|&c| c == u32::from(b','))
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(2, result.len());
    assert_eq!(wide("").as_slice(), &result[0][..]);
    assert_eq!(wide("").as_slice(), &result[1][..]);
    result = wide("a,")
        .split(|&c| c == u32::from(b','))
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(2, result.len());
    assert_eq!(wide("a").as_slice(), &result[0][..]);
    assert_eq!(wide("").as_slice(), &result[1][..]);
    result = wide(",b")
        .split(|&c| c == u32::from(b','))
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(2, result.len());
    assert_eq!(wide("").as_slice(), &result[0][..]);
    assert_eq!(wide("b").as_slice(), &result[1][..]);
    result = wide("a,b")
        .split(|&c| c == u32::from(b','))
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(2, result.len());
    assert_eq!(wide("a").as_slice(), &result[0][..]);
    assert_eq!(wide("b").as_slice(), &result[1][..]);
    result = wide("a,b,")
        .split(|&c| c == u32::from(b','))
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(3, result.len());
    assert_eq!(wide("a").as_slice(), &result[0][..]);
    assert_eq!(wide("b").as_slice(), &result[1][..]);
    assert_eq!(wide("").as_slice(), &result[2][..]);
    result = wide("a,,")
        .split(|&c| c == u32::from(b','))
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(3, result.len());
    assert_eq!(wide("a").as_slice(), &result[0][..]);
    assert_eq!(wide("").as_slice(), &result[1][..]);
    assert_eq!(wide("").as_slice(), &result[2][..]);
    result = wide(",,a")
        .split(|&c| c == u32::from(b','))
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(3, result.len());
    assert_eq!(wide("").as_slice(), &result[0][..]);
    assert_eq!(wide("").as_slice(), &result[1][..]);
    assert_eq!(wide("a").as_slice(), &result[2][..]);
}

#[test]
fn SplitWideStringView() {
    let mut result: Vec<&[u32]>;
    let input = wide("");
    result = input.split(|&c| c == u32::from(b',')).collect();
    assert_eq!(1, result.len());
    assert_eq!(wide("").as_slice(), &result[0][..]);
    let input = wide("a");
    result = input.split(|&c| c == u32::from(b',')).collect();
    assert_eq!(1, result.len());
    assert_eq!(wide("a").as_slice(), &result[0][..]);
    let input = wide(",");
    result = input.split(|&c| c == u32::from(b',')).collect();
    assert_eq!(2, result.len());
    assert_eq!(wide("").as_slice(), &result[0][..]);
    assert_eq!(wide("").as_slice(), &result[1][..]);
    let input = wide("a,");
    result = input.split(|&c| c == u32::from(b',')).collect();
    assert_eq!(2, result.len());
    assert_eq!(wide("a").as_slice(), &result[0][..]);
    assert_eq!(wide("").as_slice(), &result[1][..]);
    let input = wide(",b");
    result = input.split(|&c| c == u32::from(b',')).collect();
    assert_eq!(2, result.len());
    assert_eq!(wide("").as_slice(), &result[0][..]);
    assert_eq!(wide("b").as_slice(), &result[1][..]);
    let input = wide("a,b");
    result = input.split(|&c| c == u32::from(b',')).collect();
    assert_eq!(2, result.len());
    assert_eq!(wide("a").as_slice(), &result[0][..]);
    assert_eq!(wide("b").as_slice(), &result[1][..]);
    let input = wide("a,b,");
    result = input.split(|&c| c == u32::from(b',')).collect();
    assert_eq!(3, result.len());
    assert_eq!(wide("a").as_slice(), &result[0][..]);
    assert_eq!(wide("b").as_slice(), &result[1][..]);
    assert_eq!(wide("").as_slice(), &result[2][..]);
    let input = wide("a,,");
    result = input.split(|&c| c == u32::from(b',')).collect();
    assert_eq!(3, result.len());
    assert_eq!(wide("a").as_slice(), &result[0][..]);
    assert_eq!(wide("").as_slice(), &result[1][..]);
    assert_eq!(wide("").as_slice(), &result[2][..]);
    let input = wide(",,a");
    result = input.split(|&c| c == u32::from(b',')).collect();
    assert_eq!(3, result.len());
    assert_eq!(wide("").as_slice(), &result[0][..]);
    assert_eq!(wide("").as_slice(), &result[1][..]);
    assert_eq!(wide("a").as_slice(), &result[2][..]);
}

#[test]
fn ByteStringSplitEfficiency() {
    let commas = vec![b','; 50000];
    let input = commas.to_vec();
    let result: Vec<Vec<u8>> = input
        .split(|&c| c == b',')
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(commas.len() + 1, result.len());
    assert!(result.first().unwrap().is_empty());
    assert!(result.last().unwrap().is_empty());
}

#[test]
fn ByteStringViewSplitEfficiency() {
    let commas = vec![b','; 50000];
    let input = commas.as_slice();
    let result: Vec<&[u8]> = input.split(|&c| c == b',').collect();
    assert_eq!(commas.len() + 1, result.len());
    assert!(result.first().unwrap().is_empty());
    assert!(result.last().unwrap().is_empty());
}

#[test]
fn WideStringSplitEfficiency() {
    let commas = vec![u32::from(b','); 50000];
    let input = commas.to_vec();
    let result: Vec<Vec<u32>> = input
        .split(|&c| c == u32::from(b','))
        .map(|part| part.to_vec())
        .collect();
    assert_eq!(commas.len() + 1, result.len());
    assert!(result.first().unwrap().is_empty());
    assert!(result.last().unwrap().is_empty());
}

#[test]
fn WideStringViewSplitEfficiency() {
    let commas = vec![u32::from(b','); 50000];
    let input = commas.as_slice();
    let result: Vec<&[u32]> = input.split(|&c| c == u32::from(b',')).collect();
    assert_eq!(commas.len() + 1, result.len());
    assert!(result.first().unwrap().is_empty());
    assert!(result.last().unwrap().is_empty());
}
