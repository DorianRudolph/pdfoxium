// Copyright 2015 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Selected cases from `core/fxcrt/fx_extension_unittest.cpp`.
//! ASCII operations use the same standard primitives as selected callers.
//! Hex decoding uses the shared selected parser helper; byte hex output and
//! NaN ordering exercise their existing parser/graphics-cache entry points.
#![allow(non_snake_case)]
use crate::fpdfapi::parser::fpdf_parser_utility::hex_char_to_int;
use crate::fxcrt::fx_extension::wcstof;

/// Complete `fxcrt.FXSYSwcstof`, including all consumed-length assertions.
#[test]
fn FXSYSwcstof() {
    for (text, expected, used) in [
        ("-12", -12.0_f32, 3),
        ("+12", 12.0_f32, 3),
        (" 123", 123.0_f32, 4),
        (" 123 ", 123.0_f32, 4),
        (" 1 2 3 ", 1.0_f32, 2),
        ("1.5362", 1.5362_f32, 6),
        ("1 .5362", 1.0_f32, 1),
        ("1. 5362", 1.0_f32, 2),
        ("1.5.3.6.2", 1.5_f32, 3),
        ("0.875", 0.875_f32, 5),
        ("5.56e-2", 5.56e-2_f32, 7),
        ("1.234E10", 1.234e10_f32, 8),
        ("1.234E100000000000000", f32::INFINITY, 21),
        ("1.234E-128", 0.0_f32, 10),
        ("123456789.012345678", 123456789.012345678_f32, 19),
        ("99999999999999999", 99999999999999999.0_f32, 17),
    ] {
        let wide: Vec<u32> = text.chars().map(u32::from).collect();
        let (value, used_len) = wcstof(&wide);
        assert_eq!(value, expected, "{text}");
        assert_eq!(used_len, used, "{text}");
    }
    let invalid: Vec<u32> = "e76".chars().map(u32::from).collect();
    assert_eq!(wcstof(&invalid).0, 0.0);
    // Both native overflow inputs contain 130 decimal digits; the second
    // includes a minus sign. Retain the entire input and length contract.
    let positive = "8".repeat(130);
    let negative = format!("-{positive}");
    for (text, used) in [(&positive, 130), (&negative, 131)] {
        let wide: Vec<u32> = text.chars().map(u32::from).collect();
        let (value, used_len) = wcstof(&wide);
        assert!(value.is_infinite(), "{text}");
        assert_eq!(used_len, used);
    }
}

#[test]
fn wcstof_preserves_prefix_boundaries_signs_and_wide_stops() {
    for (text, expected, used) in [
        ("", 0.0, 0),
        ("   ", 0.0, 3),
        ("   invalid", 0.0, 3),
        ("\t12", 0.0, 0),
        ("\n12", 0.0, 0),
        ("\u{a0}12", 0.0, 0),
        ("--12", 0.0, 0),
        ("+-12", 0.0, 0),
        ("++12", 0.0, 0),
        ("  +", 0.0, 2),
        ("+.5em", 0.5, 3),
        ("-.5%", -0.5, 3),
        (".", 0.0, 0),
        ("+.", 0.0, 0),
        ("1e", 1.0, 1),
        ("1e+", 1.0, 1),
        ("1e-pt", 1.0, 1),
        ("1e+2px", 100.0, 4),
        ("12\0.5", 12.0, 2),
        ("12\u{1f3a8}5", 12.0, 2),
        ("-0", -0.0, 2),
        ("-1e-999", -0.0, 7),
        ("+INFINITYsuffix", f32::INFINITY, 9),
        ("-InFiNiTy", f32::NEG_INFINITY, 9),
        ("infinite", f32::INFINITY, 3),
    ] {
        let wide: Vec<u32> = text.chars().map(u32::from).collect();
        let (value, used_len) = wcstof(&wide);
        assert_eq!(value.to_bits(), expected.to_bits(), "{text}");
        assert_eq!(used_len, used, "{text}");
    }
    for (text, used, negative) in [
        ("NaN", 3, false),
        ("+nan(payload_42)", 16, false),
        ("-NAN()tail", 6, true),
        ("nan(payload", 3, false),
        ("nan(bad-payload)", 3, false),
    ] {
        let wide: Vec<u32> = text.chars().map(u32::from).collect();
        let (value, used_len) = wcstof(&wide);
        assert!(value.is_nan(), "{text}");
        assert_eq!(value.is_sign_negative(), negative, "{text}");
        assert_eq!(used_len, used, "{text}");
    }
    assert_eq!(wcstof(&[49, 50, 0xd800, 51]), (12.0, 2));
}

fn IsLowerASCII(c: i32) -> bool {
    u8::try_from(c).is_ok_and(|ch| ch.is_ascii_lowercase())
}
fn IsUpperASCII(c: i32) -> bool {
    u8::try_from(c).is_ok_and(|ch| ch.is_ascii_uppercase())
}
fn IsDecimalDigit(c: i32) -> bool {
    u8::try_from(c).is_ok_and(|ch| ch.is_ascii_digit())
}
fn DecimalCharToInt(c: i32) -> i32 {
    u8::try_from(c)
        .ok()
        .filter(u8::is_ascii_digit)
        .map_or(0, |ch| i32::from(ch - b'0'))
}
const _: () = {
    assert!(b'a'.is_ascii_lowercase());
    assert!(!b'A'.is_ascii_lowercase());
    assert!(b'A'.is_ascii_uppercase());
    assert!(!b'a'.is_ascii_uppercase());
    assert!(b'A'.to_ascii_lowercase() == b'a');
    assert!(b'a'.to_ascii_lowercase() == b'a');
    assert!(b'a'.to_ascii_uppercase() == b'A');
    assert!(b'A'.to_ascii_uppercase() == b'A');
    assert!(matches!(b'7', b'0'..=b'7'));
    assert!(!matches!(b'8', b'0'..=b'7'));
    assert!(hex_char_to_int(b'a') == 10);
    assert!(hex_char_to_int(b'a') == 10); // wchar_t ASCII 'a' narrows after validation.
};

#[test]
fn FXSYSIsLowerASCII() {
    assert!(IsLowerASCII(b'a' as i32));
    assert!(IsLowerASCII(b'a' as i32));
    assert!(IsLowerASCII(b'b' as i32));
    assert!(IsLowerASCII(b'b' as i32));
    assert!(IsLowerASCII(b'y' as i32));
    assert!(IsLowerASCII(b'y' as i32));
    assert!(IsLowerASCII(b'z' as i32));
    assert!(IsLowerASCII(b'z' as i32));
    assert!(!IsLowerASCII(b'`' as i32));
    assert!(!IsLowerASCII(b'`' as i32));
    assert!(!IsLowerASCII(b'{' as i32));
    assert!(!IsLowerASCII(b'{' as i32));
    assert!(!IsLowerASCII(b'Z' as i32));
    assert!(!IsLowerASCII(b'Z' as i32));
    assert!(!IsLowerASCII(b'7' as i32));
    assert!(!IsLowerASCII(b'7' as i32));
    assert!(!IsLowerASCII(-78));
    assert!(!IsLowerASCII(0xb2));
}

#[test]
fn FXSYSIsUpperASCII() {
    assert!(IsUpperASCII(b'A' as i32));
    assert!(IsUpperASCII(b'A' as i32));
    assert!(IsUpperASCII(b'B' as i32));
    assert!(IsUpperASCII(b'B' as i32));
    assert!(IsUpperASCII(b'Y' as i32));
    assert!(IsUpperASCII(b'Y' as i32));
    assert!(IsUpperASCII(b'Z' as i32));
    assert!(IsUpperASCII(b'Z' as i32));
    assert!(!IsUpperASCII(b'@' as i32));
    assert!(!IsUpperASCII(b'@' as i32));
    assert!(!IsUpperASCII(b'[' as i32));
    assert!(!IsUpperASCII(b'[' as i32));
    assert!(!IsUpperASCII(b'z' as i32));
    assert!(!IsUpperASCII(b'z' as i32));
    assert!(!IsUpperASCII(b'7' as i32));
    assert!(!IsUpperASCII(b'7' as i32));
    assert!(!IsUpperASCII(-78));
    assert!(!IsUpperASCII(0xb2));
}

#[test]
fn FXSYSHexCharToInt() {
    assert_eq!(10, hex_char_to_int(b'a'));
    assert_eq!(10, hex_char_to_int(b'A'));
    assert_eq!(7, hex_char_to_int(b'7'));
    assert_eq!(0, hex_char_to_int(b'i'));
}

#[test]
fn FXSYSDecimalCharToInt() {
    assert_eq!(7, DecimalCharToInt(b'7' as i32));
    assert_eq!(0, DecimalCharToInt(b'a' as i32));
    assert_eq!(7, DecimalCharToInt(b'7' as i32));
    assert_eq!(0, DecimalCharToInt(b'a' as i32));
    assert_eq!(0, DecimalCharToInt(-78));
    assert_eq!(0, DecimalCharToInt(0xb2));
}

#[test]
fn FXSYSIsDecimalDigit() {
    assert!(IsDecimalDigit(b'7' as i32));
    assert!(IsDecimalDigit(b'7' as i32));
    assert!(!IsDecimalDigit(b'a' as i32));
    assert!(!IsDecimalDigit(b'a' as i32));
    assert!(!IsDecimalDigit(-78));
    assert!(!IsDecimalDigit(0xb2));
}

#[test]
fn FXSYSIntToTwoHexChars() {
    let mut buf = [0u8; 2];
    let write = super::fx_extension::int_to_two_hex_chars;
    write(0x0, &mut buf);
    assert_eq!(buf, [b'0', b'0']);
    write(0x9, &mut buf);
    assert_eq!(buf, [b'0', b'9']);
    write(0xA, &mut buf);
    assert_eq!(buf, [b'0', b'A']);
    write(0x8C, &mut buf);
    assert_eq!(buf, [b'8', b'C']);
    write(0xBE, &mut buf);
    assert_eq!(buf, [b'B', b'E']);
    write(0xD0, &mut buf);
    assert_eq!(buf, [b'D', b'0']);
    write(0xFF, &mut buf);
    assert_eq!(buf, [b'F', b'F']);
}

#[test]
fn FXSYSIntToFourHexChars() {
    use super::fx_extension::int_to_four_hex_chars;
    let mut buf = [0u8; 4];
    int_to_four_hex_chars(0x0, &mut buf);
    assert_eq!(&buf, b"0000");
    int_to_four_hex_chars(0xA23, &mut buf);
    assert_eq!(&buf, b"0A23");
    int_to_four_hex_chars(0xB701, &mut buf);
    assert_eq!(&buf, b"B701");
    int_to_four_hex_chars(0xFFFF, &mut buf);
    assert_eq!(&buf, b"FFFF");
}

#[test]
fn FXSYSToUTF16BE() {
    use super::fx_extension::to_utf16be;
    let mut buf = [0u8; 8];
    assert_eq!(to_utf16be(0x0, &mut buf), b"0000");
    assert_eq!(to_utf16be(0xD7FF, &mut buf), b"D7FF");
    assert_eq!(to_utf16be(0xE000, &mut buf), b"E000");
    assert_eq!(to_utf16be(0xFFFF, &mut buf), b"FFFF");
    assert_eq!(to_utf16be(0x10000, &mut buf), b"D800DC00");
    assert_eq!(to_utf16be(0x10FFFF, &mut buf), b"DBFFDFFF");
    assert_eq!(to_utf16be(0x2003E, &mut buf), b"D840DC3E");
}

#[test]
fn whitespace_keeps_icu_control_extensions_and_separator_boundaries() {
    use super::fx_extension::is_wide_space;
    for point in [
        0x9, 0xd, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x85, 0xa0, 0x1680, 0x2000, 0x200a, 0x2028, 0x2029,
        0x202f, 0x205f, 0x3000,
    ] {
        assert!(is_wide_space(point), "U+{point:04X}");
    }
    for point in [
        0,
        0x8,
        0xe,
        0x1b,
        0x21,
        0x84,
        0x86,
        0x9f,
        0xa1,
        0x167f,
        0x1681,
        0x180e,
        0x1fff,
        0x200b,
        0x2027,
        0x202a,
        0x202e,
        0x2030,
        0x205e,
        0x2060,
        0x2fff,
        0x3001,
        0xd800,
        0x10ffff,
        0x110000,
        u32::MAX,
    ] {
        assert!(!is_wide_space(point), "U+{point:04X}");
    }
}
#[test]
fn FXSYSSafeOps() {
    use crate::fpdfapi::page::cpdf_pageobjectholder::GraphicsData;
    use crate::fxge::dib::fx_dib::BlendMode;
    let fMin = f32::MIN_POSITIVE;
    let fMax = f32::MAX;
    let fInf = f32::INFINITY;
    let fNan = f32::NAN;
    let ascending = [fMin, 1.0, 2.0, fMax, fInf, fNan];
    let key = |value| GraphicsData {
        fill_alpha: value,
        stroke_alpha: 0.0,
        blend_type: BlendMode::Normal,
    };
    for i in 0..ascending.len() {
        for j in 0..ascending.len() {
            if i == j {
                assert!(key(ascending[i]) == key(ascending[j]), "at {i} {j}");
            } else {
                assert!(key(ascending[i]) != key(ascending[j]), "at {i} {j}");
            }
            if i < j {
                assert!(key(ascending[i]) < key(ascending[j]), "at {i} {j}");
            } else {
                assert!(!(key(ascending[i]) < key(ascending[j])), "at {i} {j}");
            }
        }
    }
}

#[test]
fn FXSYSTimeZoneOffsetInMinutes() {
    use super::fx_extension::{time_zone_offset_in_minutes, TimeZoneFields};
    for (local, utc, expected) in [
        ((23, 10, 35), (23, 3, 5), 450),
        ((22, 23, 5), (23, 3, 5), -240),
        ((24, 7, 5), (23, 17, 5), 840),
        ((1, 7, 5), (31, 17, 5), 840),
        ((31, 23, 5), (1, 3, 5), -240),
    ] {
        let fields = |(day, hour, minute)| TimeZoneFields { day, hour, minute };
        assert_eq!(
            time_zone_offset_in_minutes(fields(local), fields(utc)),
            expected
        );
    }
}
