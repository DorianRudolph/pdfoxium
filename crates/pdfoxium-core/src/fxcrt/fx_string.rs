// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Numeric prefixes and UTF conversion from `core/fxcrt/fx_string.{h,cpp}`.
//! Wide views retain 32-bit wchar_t values, including unpaired surrogates.

use super::utf16::{
    is_high_surrogate, is_low_surrogate, SurrogatePair, MAXIMUM_SUPPLEMENTARY_CODE_POINT,
};
use std::borrow::Cow;

fn parse_leading_chars<T: Copy + Into<u32>>(mut text: &[T]) -> &[T] {
    let mut start = 0;
    while start < text.len() && matches!(text[start].into(), 32 | 43 | 45) {
        start += 1;
    }
    if start > 0 && text[start - 1].into() == u32::from(b'-') {
        start -= 1;
    }
    text = &text[start..];
    text
}

/// StringToInt permits repeated leading signs/spaces, then uses the native
/// saturating decimal conversion. Only the final adjacent minus controls sign.
pub fn string_to_int(text: &[u8]) -> i32 {
    super::fx_system::atoi(parse_leading_chars(text))
}

pub fn string_to_int_wide(text: &[u32]) -> i32 {
    super::fx_system::wtoi(parse_leading_chars(text))
}

/// `WideString::EncodeEntities` from `core/fxcrt/widestring.cpp`. Preserve
/// its replacement order so existing entity spellings are escaped too.
pub fn encode_entities(text: &[u32]) -> Cow<'_, [u32]> {
    let mut result = Cow::Borrowed(text);
    for (character, replacement) in [
        (b'&', b"&amp;".as_slice()),
        (b'<', b"&lt;".as_slice()),
        (b'>', b"&gt;".as_slice()),
        (b'\'', b"&apos;".as_slice()),
        (b'\"', b"&quot;".as_slice()),
    ] {
        let matches = result
            .iter()
            .filter(|&&point| point == u32::from(character))
            .count();
        if matches == 0 {
            continue;
        }
        let mut replaced = Vec::with_capacity(result.len() + matches * (replacement.len() - 1));
        for &point in result.as_ref() {
            if point == u32::from(character) {
                replaced.extend(replacement.iter().map(|&byte| u32::from(byte)));
            } else {
                replaced.push(point);
            }
        }
        result = Cow::Owned(replaced);
    }
    result
}

// Find only the prefix accepted by fast_float::from_chars(general). Keep
// f32/f64 conversion separate so decimal rounding happens at the target width.
fn float_prefix_len<T: Copy + Into<u32>>(text: &[T]) -> usize {
    let at = |index: usize| text.get(index).copied().map(Into::into);
    let digit = |index: usize| at(index).is_some_and(|c| (48..=57).contains(&c));
    let sign = usize::from(at(0) == Some(45));
    let special = |word: &[u8; 3]| {
        (0..3).all(|index| at(sign + index).is_some_and(|c| c | 32 == u32::from(word[index])))
    };
    if special(b"inf") || special(b"nan") {
        return sign + 3;
    }
    let mut index = sign;
    let mut has_digit = false;
    while digit(index) {
        index += 1;
        has_digit = true;
    }
    if at(index) == Some(46) {
        index += 1;
        while digit(index) {
            index += 1;
            has_digit = true;
        }
    }
    if !has_digit {
        return 0;
    }
    let mut end = index;
    if matches!(at(index), Some(69 | 101)) {
        index += 1;
        if matches!(at(index), Some(43 | 45)) {
            index += 1;
        }
        let exponent = index;
        while digit(index) {
            index += 1;
        }
        if index > exponent {
            end = index;
        }
    }
    end
}

fn float_prefix(text: &[u8]) -> &str {
    let text = parse_leading_chars(text);
    std::str::from_utf8(&text[..float_prefix_len(text)]).expect("ASCII float prefix")
}

fn wide_float_prefix(text: &[u32]) -> Vec<u8> {
    let text = parse_leading_chars(text);
    // Only the ASCII numeric prefix needs conversion, not the entire string.
    text[..float_prefix_len(text)]
        .iter()
        .map(|&unit| unit as u8)
        .collect()
}

/// Decimal prefix parsing follows native leading sign/space handling. Rust's
/// decimal parser supplies IEEE rounding, overflow and underflow at each width.
pub fn string_to_float(text: &[u8]) -> f32 {
    float_prefix(text).parse().unwrap_or(0.0)
}

pub fn string_to_double(text: &[u8]) -> f64 {
    float_prefix(text).parse().unwrap_or(0.0)
}

pub fn string_to_float_wide(text: &[u32]) -> f32 {
    let prefix = wide_float_prefix(text);
    std::str::from_utf8(&prefix)
        .expect("ASCII float prefix")
        .parse()
        .unwrap_or(0.0)
}

pub fn string_to_double_wide(text: &[u32]) -> f64 {
    let prefix = wide_float_prefix(text);
    std::str::from_utf8(&prefix)
        .expect("ASCII float prefix")
        .parse()
        .unwrap_or(0.0)
}

/// FX_UTF8Encode for 32-bit wchar_t. Unpaired surrogate values retain native
/// three-byte encoding; values above U+10FFFF are skipped, rather than replaced.
pub fn utf8_encode(text: &[u32]) -> Vec<u8> {
    let mut result = Vec::new();
    for &point in text {
        if point > super::utf16::MAXIMUM_SUPPLEMENTARY_CODE_POINT {
            continue;
        }
        if point < 0x80 {
            result.push(point as u8);
            continue;
        }
        let count = if point < 0x800 {
            2
        } else if point < 0x10000 {
            3
        } else {
            4
        };
        let mut order = 1 << ((count - 1) * 6);
        result.push([0xc0, 0xe0, 0xf0][count - 2] | (point / order) as u8);
        let mut point = point;
        for _ in 0..count - 1 {
            point %= order;
            order >>= 6;
            result.push(0x80 | (point / order) as u8);
        }
    }
    result
}

/// FX_UTF16Encode for 32-bit wchar_t. Native preserves unpaired BMP surrogates
/// and narrows non-supplementary values to a single 16-bit unit.
pub fn utf16_encode(text: &[u32]) -> Vec<u16> {
    let mut result = Vec::with_capacity(text.len());
    for &point in text {
        if super::utf16::is_supplementary(point) {
            let pair = super::utf16::SurrogatePair::from_code_point(point);
            result.push(pair.high());
            result.push(pair.low());
        } else {
            result.push(point as u16);
        }
    }
    result
}

// WideString::FromUTF16LE/BE and FromUTF8 from widestring.cpp. Keep raw
// surrogate code points and native malformed UTF-8 recovery on 32-bit wchar_t.
pub fn utf16_decode(data: &[u8], big_endian: bool) -> Vec<u32> {
    let mut result = Vec::with_capacity(data.len() / 2);
    let mut chunks = data.chunks_exact(2).peekable();
    while let Some(bytes) = chunks.next() {
        let unit = if big_endian {
            u16::from_be_bytes([bytes[0], bytes[1]])
        } else {
            u16::from_le_bytes([bytes[0], bytes[1]])
        };
        if is_high_surrogate(unit as u32) {
            if let Some(next) = chunks.peek() {
                let low = if big_endian {
                    u16::from_be_bytes([next[0], next[1]])
                } else {
                    u16::from_le_bytes([next[0], next[1]])
                };
                if is_low_surrogate(low as u32) {
                    result.push(SurrogatePair::new(unit, low).to_code_point());
                    chunks.next();
                    continue;
                }
            }
        }
        result.push(u32::from(unit));
    }
    result
}

pub fn utf8_decode(data: &[u8]) -> Vec<u32> {
    let mut result = Vec::new();
    let mut remaining = 0;
    let mut code_point = 0u32;
    for &unit in data {
        if unit < 0x80 {
            remaining = 0;
            result.push(u32::from(unit));
        } else if unit < 0xc0 {
            if remaining > 0 {
                remaining -= 1;
                code_point = (code_point << 6) | u32::from(unit & 0x3f);
                if remaining == 0 && code_point <= MAXIMUM_SUPPLEMENTARY_CODE_POINT {
                    result.push(code_point);
                }
            }
        } else if unit < 0xe0 {
            remaining = 1;
            code_point = u32::from(unit & 0x1f);
        } else if unit < 0xf0 {
            remaining = 2;
            code_point = u32::from(unit & 0x0f);
        } else if unit < 0xf8 {
            remaining = 3;
            code_point = u32::from(unit & 0x07);
        } else {
            remaining = 0;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_entities_escape_existing_spellings_and_preserve_raw_wide_units() {
        let source: Vec<_> = "&amp;\0<"
            .chars()
            .map(u32::from)
            .chain([0xd800, 0x110000])
            .collect();
        let expected: Vec<_> = "&amp;amp;\0&lt;"
            .chars()
            .map(u32::from)
            .chain([0xd800, 0x110000])
            .collect();
        assert_eq!(encode_entities(&source), expected);
        assert_eq!(encode_entities(&[]), Vec::<u32>::new());
        assert_eq!(
            encode_entities(&[0xd800, 0x110000]).as_ref(),
            &[0xd800, 0x110000]
        );
    }

    #[test]
    fn string_integer_leading_signs_differ_from_c_string_conversions() {
        for (text, expected) in [
            (b" --100tail".as_slice(), -100),
            (b"+-100", -100),
            (b"++100", 100),
            (b"- +100", 100),
            (b"\t100", 0),
            (b"-2147483648", i32::MIN),
            (b"2147483648", i32::MAX),
        ] {
            assert_eq!(string_to_int(text), expected);
            let wide: Vec<_> = text.iter().map(|&byte| u32::from(byte)).collect();
            assert_eq!(string_to_int_wide(&wide), expected);
        }
    }

    #[test]
    fn native_float_widths_unicode_stops_and_signed_special_values() {
        for text in [
            b"+-100.25tail".as_slice(),
            b"1.2e+tail",
            b"-0",
            b"-inf",
            b"-nan",
            b"1e999",
            b"-1e-999",
        ] {
            let wide: Vec<_> = text.iter().map(|&byte| u32::from(byte)).collect();
            assert_eq!(
                string_to_float(text).to_bits(),
                string_to_float_wide(&wide).to_bits()
            );
            assert_eq!(
                string_to_double(text).to_bits(),
                string_to_double_wide(&wide).to_bits()
            );
        }
        assert_eq!(string_to_double(b"1.99987792968929034"), 1.9998779296892903);
        assert_eq!(string_to_float_wide(&[49, 50, 51, 0x3132, 46, 53]), 123.0);
        assert_eq!(string_to_float_wide(&[0x1f3a8, 57]), 0.0);
        assert!(string_to_float(b"-nan").is_sign_negative());
        assert!(string_to_float(b"-1e-999").is_sign_negative());
        assert_eq!(string_to_double(b"1.2e+tail"), 1.2);
    }

    #[test]
    fn native_utf_encoders_preserve_surrogates_and_out_of_range_rules() {
        let text = [0, 0x80, 0xd800, 0x1f3a8, 0x10ffff, 0x110000];
        assert_eq!(
            utf8_encode(&text),
            [0, 0xc2, 0x80, 0xed, 0xa0, 0x80, 0xf0, 0x9f, 0x8e, 0xa8, 0xf4, 0x8f, 0xbf, 0xbf]
        );
        assert_eq!(
            utf16_encode(&text),
            [0, 0x80, 0xd800, 0xd83c, 0xdfa8, 0xdbff, 0xdfff, 0]
        );
    }

    #[test]
    fn decimal_prefixes_and_rounding_match_pdfium_examples() {
        assert_eq!(string_to_float(b"+-100.0tail"), -100.0);
        assert_eq!(string_to_float(b"++100.0"), 100.0);
        assert_eq!(string_to_float(b"1.2e34"), 1.2e34);
        assert_eq!(string_to_float(b"38.895285"), 38.89528656005859375f32);
        assert_eq!(string_to_float(b"invalid"), 0.0);
        assert_eq!(
            string_to_float(b"999999999999999999999999999999999999999"),
            f32::INFINITY
        );
    }
}

#[cfg(test)]
#[path = "fx_string_unittest.rs"]
mod upstream_tests;
