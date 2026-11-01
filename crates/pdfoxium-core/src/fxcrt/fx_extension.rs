// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Hexadecimal serialization and wide numeric helpers from
//! `core/fxcrt/fx_extension.{h,cpp}`. Includes the selected timezone-offset
//! helper; the other source services remain tracked separately.

/// `FXSYS_wcstof`: parse one general-format float prefix, returning its value
/// and consumed wide-character count. Unlike StringToFloat, this source entry
/// point skips only leading ASCII spaces and accepts a single leading sign.
/// Rust's f32 parser supplies the decimal rounding, overflow and underflow;
/// the scanner retains from_chars' prefix boundaries for CSS unit suffixes.
pub fn wcstof(text: &[u32]) -> (f32, usize) {
    let start = text.iter().position(|&ch| ch != 0x20).unwrap_or(text.len());
    let mut index = start;
    let negative = text.get(index) == Some(&u32::from(b'-'));
    if matches!(text.get(index), Some(0x2b | 0x2d)) {
        index += 1;
    }
    let word = |offset: usize, expected: &[u8]| {
        text.get(offset..offset + expected.len())
            .is_some_and(|actual| {
                actual
                    .iter()
                    .zip(expected)
                    .all(|(&ch, &byte)| ch | 0x20 == u32::from(byte))
            })
    };
    if word(index, b"inf") {
        index += if word(index, b"infinity") { 8 } else { 3 };
        return (
            if negative {
                f32::NEG_INFINITY
            } else {
                f32::INFINITY
            },
            index,
        );
    }
    if word(index, b"nan") {
        index += 3;
        // from_chars consumes an optional alphanumeric/underscore NaN payload
        // only when the entire parenthesized payload is present.
        if text.get(index) == Some(&u32::from(b'(')) {
            let mut payload = index + 1;
            while text.get(payload).is_some_and(|&ch| {
                u8::try_from(ch).is_ok_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            }) {
                payload += 1;
            }
            if text.get(payload) == Some(&u32::from(b')')) {
                index = payload + 1;
            }
        }
        return (if negative { -f32::NAN } else { f32::NAN }, index);
    }
    let digit = |offset: usize| {
        text.get(offset)
            .is_some_and(|&ch| (0x30..=0x39).contains(&ch))
    };
    let integer_start = index;
    while digit(index) {
        index += 1;
    }
    let mut has_digit = index != integer_start;
    if text.get(index) == Some(&u32::from(b'.')) {
        index += 1;
        let fraction_start = index;
        while digit(index) {
            index += 1;
        }
        has_digit |= index != fraction_start;
    }
    if !has_digit {
        return (0.0, start);
    }
    let mut end = index;
    if matches!(text.get(index), Some(0x45 | 0x65)) {
        index += 1;
        if matches!(text.get(index), Some(0x2b | 0x2d)) {
            index += 1;
        }
        let exponent_start = index;
        while digit(index) {
            index += 1;
        }
        if index != exponent_start {
            end = index;
        }
    }
    let prefix: Vec<u8> = text[start..end].iter().map(|&ch| ch as u8).collect();
    let value = std::str::from_utf8(&prefix)
        .expect("ASCII float prefix")
        .parse::<f32>()
        .expect("valid float prefix");
    (value, end)
}

/// `FXSYS_iswspace` delegates to ICU `u_isspace`: Unicode separator classes
/// plus ICU's whitespace control ranges (including U+001C through U+001F).
/// Keep this distinct from XML's four whitespace characters and Rust's
/// `char::is_whitespace`, which omits those four controls.
pub fn is_wide_space(code_unit: u32) -> bool {
    matches!(
        code_unit,
        0x0009..=0x000d | 0x001c..=0x0020 | 0x0085 | 0x00a0 | 0x1680
            | 0x2000..=0x200a | 0x2028..=0x2029 | 0x202f | 0x205f | 0x3000
    )
}

pub fn int_to_two_hex_chars(value: u8, buffer: &mut [u8; 2]) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    buffer[0] = HEX[usize::from(value / 16)];
    buffer[1] = HEX[usize::from(value % 16)];
}

pub fn int_to_four_hex_chars(value: u16, buffer: &mut [u8; 4]) {
    let (high, low) = buffer.split_at_mut(2);
    int_to_two_hex_chars((value / 256) as u8, high.try_into().unwrap());
    int_to_two_hex_chars((value % 256) as u8, low.try_into().unwrap());
}

/// Write one Unicode scalar as UTF-16BE hex digits, without delimiters.
pub fn to_utf16be(unicode: u32, buffer: &mut [u8; 8]) -> &[u8] {
    debug_assert!(unicode <= 0x10ffff);
    debug_assert!(!(0xd800..=0xdfff).contains(&unicode));
    if unicode <= 0xffff {
        int_to_four_hex_chars(unicode as u16, (&mut buffer[..4]).try_into().unwrap());
        return &buffer[..4];
    }
    let scalar = unicode.wrapping_sub(0x10000);
    let (high, low) = buffer.split_at_mut(4);
    int_to_four_hex_chars(
        (0xd800 | ((scalar >> 10) & 0x3ff)) as u16,
        high.try_into().unwrap(),
    );
    int_to_four_hex_chars((0xdc00 | (scalar & 0x3ff)) as u16, low.try_into().unwrap());
    buffer
}

/// The struct tm fields read by FXSYS_TimeZoneOffsetInMinutes.
#[derive(Clone, Copy)]
pub struct TimeZoneFields {
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
}
pub fn time_zone_offset_in_minutes(local: TimeZoneFields, utc: TimeZoneFields) -> i32 {
    let mut day_offset = local.day - utc.day;
    if day_offset > 1 {
        day_offset = -1;
    } else if day_offset < -1 {
        day_offset = 1;
    }
    day_offset * 1440 + (local.hour - utc.hour) * 60 + local.minute - utc.minute
}

/// ICU's scalar (not expanding, context-sensitive string) mappings. Compiled
/// ICU4X data supplies the same operation without a native ICU dependency.
pub fn to_lower_wide(code_unit: u32) -> u32 {
    char::from_u32(code_unit).map_or(code_unit, |ch| {
        u32::from(icu_casemap::CaseMapper::new().simple_lowercase(ch))
    })
}
pub fn to_upper_wide(code_unit: u32) -> u32 {
    char::from_u32(code_unit).map_or(code_unit, |ch| {
        u32::from(icu_casemap::CaseMapper::new().simple_uppercase(ch))
    })
}
pub fn is_lower_wide(code_unit: u32) -> bool {
    char::from_u32(code_unit).is_some_and(|ch| {
        unicode_general_category::get_general_category(ch)
            == unicode_general_category::GeneralCategory::LowercaseLetter
    })
}
pub fn is_upper_wide(code_unit: u32) -> bool {
    char::from_u32(code_unit).is_some_and(|ch| {
        unicode_general_category::get_general_category(ch)
            == unicode_general_category::GeneralCategory::UppercaseLetter
    })
}
pub fn is_alpha_wide(code_unit: u32) -> bool {
    use unicode_general_category::GeneralCategory::*;
    char::from_u32(code_unit).is_some_and(|ch| {
        matches!(
            unicode_general_category::get_general_category(ch),
            UppercaseLetter | LowercaseLetter | TitlecaseLetter | ModifierLetter | OtherLetter
        )
    })
}
pub fn is_alphanumeric_wide(code_unit: u32) -> bool {
    is_alpha_wide(code_unit)
        || char::from_u32(code_unit).is_some_and(|ch| {
            unicode_general_category::get_general_category(ch)
                == unicode_general_category::GeneralCategory::DecimalNumber
        })
}
pub const fn is_lower_ascii(code_unit: i32) -> bool {
    code_unit >= b'a' as i32 && code_unit <= b'z' as i32
}
pub const fn is_upper_ascii(code_unit: i32) -> bool {
    code_unit >= b'A' as i32 && code_unit <= b'Z' as i32
}
pub const fn to_lower_ascii(byte: u8) -> u8 {
    if byte >= b'A' && byte <= b'Z' {
        byte + (b'a' - b'A')
    } else {
        byte
    }
}
pub const fn to_upper_ascii(byte: u8) -> u8 {
    if byte >= b'a' && byte <= b'z' {
        byte - (b'a' - b'A')
    } else {
        byte
    }
}
pub const fn is_octal_digit(code_unit: u32) -> bool {
    code_unit >= b'0' as u32 && code_unit <= b'7' as u32
}
pub const fn is_hex_digit(code_unit: u32) -> bool {
    (code_unit >= b'0' as u32 && code_unit <= b'9' as u32)
        || (code_unit >= b'a' as u32 && code_unit <= b'f' as u32)
        || (code_unit >= b'A' as u32 && code_unit <= b'F' as u32)
}
pub const fn hex_char_to_int(code_unit: u32) -> u32 {
    match code_unit {
        0x30..=0x39 => code_unit - 0x30,
        0x41..=0x46 => code_unit - 0x41 + 10,
        0x61..=0x66 => code_unit - 0x61 + 10,
        _ => 0,
    }
}
pub const fn is_decimal_digit(code_unit: u32) -> bool {
    code_unit >= b'0' as u32 && code_unit <= b'9' as u32
}
pub const fn decimal_char_to_int(code_unit: u32) -> u32 {
    if is_decimal_digit(code_unit) {
        code_unit - b'0' as u32
    } else {
        0
    }
}

#[cfg(test)]
mod unicode_tests {
    use super::*;
    #[test]
    fn scalar_case_mapping_preserves_simple_icu_semantics_and_raw_units() {
        for (from, to) in [
            ('c', 'C'),
            ('ć', 'Ć'),
            ('γ', 'Γ'),
            ('ǳ', 'Ǳ'),
            ('ß', 'ß'),
            ('\u{1f80}', '\u{1f88}'),
            ('\u{fb00}', '\u{fb00}'),
        ] {
            assert_eq!(to_upper_wide(from as u32), to as u32);
        }
        for (from, to) in [('C', 'c'), ('Ć', 'ć'), ('Γ', 'γ'), ('İ', 'i'), ('ẞ', 'ß')] {
            assert_eq!(to_lower_wide(from as u32), to as u32);
        }
        for raw in [0xd800, 0x110000, u32::MAX] {
            assert_eq!(to_lower_wide(raw), raw);
            assert_eq!(to_upper_wide(raw), raw);
            assert!(!is_lower_wide(raw));
            assert!(!is_upper_wide(raw));
            assert!(!is_alpha_wide(raw));
            assert!(!is_alphanumeric_wide(raw));
        }
    }
    #[test]
    fn unicode_categories_match_native_letter_and_decimal_definitions() {
        assert!(is_lower_wide('é' as u32));
        assert!(is_upper_wide('É' as u32));
        assert!(is_alpha_wide('字' as u32));
        assert!(is_alphanumeric_wide('٣' as u32));
        // Other_Lowercase, Other_Alphabetic and non-decimal Number categories
        // differ from Rust's broad is_lowercase/is_alphabetic/is_numeric.
        assert!(!is_lower_wide('\u{02b0}' as u32));
        assert!(is_alpha_wide('\u{02b0}' as u32));
        assert!(!is_alpha_wide('\u{0345}' as u32));
        assert!(!is_alphanumeric_wide('²' as u32));
        assert!(!is_alphanumeric_wide('Ⅳ' as u32));
        for code in 0..=0xff {
            assert_eq!(
                is_lower_ascii(code as i32),
                (b'a' as u32..=b'z' as u32).contains(&code)
            );
            assert_eq!(
                is_upper_ascii(code as i32),
                (b'A' as u32..=b'Z' as u32).contains(&code)
            );
            assert_eq!(
                to_lower_ascii(code as u8),
                (code as u8).to_ascii_lowercase()
            );
            assert_eq!(
                to_upper_ascii(code as u8),
                (code as u8).to_ascii_uppercase()
            );
        }
    }
}
