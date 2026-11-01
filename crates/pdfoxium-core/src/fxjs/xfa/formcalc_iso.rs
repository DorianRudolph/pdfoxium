// Copyright 2014 The PDFium Authors
// Copyright 2022 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! SOM index generation and ISO date/time portions of
//! `fxjs/xfa/cfxjse_formcalc_context.{h,cpp}`. Calendar ordinals use the existing
//! fx_date_helpers calendar arithmetic; these operations never read a clock or
//! choose a timezone. The ISO time suffix validates the input but does not shift
//! IsoDate2Num's date, matching the source.
//!
//! Corrected source defects: signed INT_MIN indices retain their magnitude;
//! zero days, missing date separators, missing time/zone hours and trailing time
//! separators or text are rejected rather than accepted as valid ISO input.
use super::formcalc_values::{get_simple_value, param_count, value_to_utf8_string};
use crate::fxjs::fx_date_helpers as date;
use boa_engine::{native_function::NativeFunctionPointer, Context, JsResult, JsValue};

pub const METHODS: &[(&str, NativeFunctionPointer)] = &[("IsoDate2Num", iso_date_to_num)];

pub fn generate_som_expression(
    name: &[u8],
    index_flags: i32,
    index_value: i32,
    is_star: bool,
) -> Vec<u8> {
    let mut result = name.to_vec();
    if is_star {
        result.extend_from_slice(b"[*]");
        return result;
    }
    if index_flags == 0 {
        return result;
    }
    if index_flags == 1 || index_value == 0 {
        result.extend_from_slice(format!("[{index_value}]").as_bytes());
        return result;
    }
    let negative = index_value < 0;
    result.extend_from_slice(if index_flags == 2 {
        if negative {
            b"[-"
        } else {
            b"[+"
        }
    } else if negative {
        b"["
    } else {
        b"[-"
    });
    // C++ checked negation defaults INT_MIN's magnitude to zero. A u32 can
    // represent its actual magnitude without overflow or a lost index.
    result.extend_from_slice(index_value.unsigned_abs().to_string().as_bytes());
    result.push(b']');
    result
}

fn decimal_pair(input: &[u8], index: usize) -> Option<i32> {
    let digits = input.get(index..index.checked_add(2)?)?;
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(i32::from(digits[0] - b'0') * 10 + i32::from(digits[1] - b'0'))
}

pub fn is_iso_date_format(input: &[u8], year: &mut i32, month: &mut i32, day: &mut i32) -> bool {
    *year = 0;
    *month = 1;
    *day = 1;
    let Some(digits) = input.get(..4) else {
        return false;
    };
    if !digits.iter().all(u8::is_ascii_digit) {
        return false;
    }
    *year = digits
        .iter()
        .fold(0, |value, &digit| value * 10 + i32::from(digit - b'0'));
    if input.len() == 4 {
        return true;
    }
    let separated = input[4] == b'-';
    let mut offset = if separated { 5 } else { 4 };
    let Some(parsed) = decimal_pair(input, offset) else {
        return false;
    };
    *month = parsed;
    if !(1..=12).contains(month) {
        return false;
    }
    offset += 2;
    if input.len() == offset {
        return true;
    }
    if separated {
        if input.get(offset) != Some(&b'-') {
            return false;
        }
        offset += 1;
    }
    let Some(parsed) = decimal_pair(input, offset) else {
        return false;
    };
    *day = parsed;
    if input.len() != offset + 2 || *day == 0 {
        return false;
    }
    let maximum = if *month == 2 {
        let leap = (*year % 4 == 0 && *year % 100 != 0) || *year % 400 == 0;
        if leap {
            29
        } else {
            28
        }
    } else if *month < 8 {
        if *month % 2 == 0 {
            30
        } else {
            31
        }
    } else if *month % 2 == 0 {
        31
    } else {
        30
    };
    *day <= maximum
}

pub fn is_iso_time_format(input: &[u8]) -> bool {
    if input.is_empty() {
        return false;
    }
    let zone = input
        .iter()
        .position(|&byte| !byte.is_ascii_digit() && byte != b':')
        .unwrap_or(input.len());
    // The source advances Hour -> Minute -> Second -> Finished, accepting
    // compact and colon-delimited forms, and an omitted minute/second pair.
    let mut state = 0;
    let mut index = 0;
    while index + 1 < zone {
        let Some(value) = decimal_pair(input, index) else {
            return false;
        };
        match state {
            0 if value >= 24 => return false,
            1 if value >= 60 => return false,
            2 if value > 60 => return false, // Leap second is allowed.
            0..=2 => {}
            _ => return false,
        }
        state += 1;
        index += 2;
        if index < zone && input[index] == b':' {
            index += 1;
            if index == zone {
                return false;
            }
        }
    }
    if state == 0 || index != zone {
        return false;
    }
    if input.get(index) == Some(&b'.') {
        let Some(milliseconds) = input.get(index + 1..index + 4) else {
            return false;
        };
        if !milliseconds.iter().all(u8::is_ascii_digit) {
            return false;
        }
        index += 4;
    }
    if input
        .get(index)
        .is_some_and(|byte| byte.eq_ignore_ascii_case(&b'z'))
    {
        return index + 1 == input.len();
    }
    if index == input.len() {
        return true;
    }
    if matches!(input[index], b'+' | b'-') {
        index += 1;
    }
    // ZoneHour -> ZoneMinute -> Finished. The source accepts both +HH and
    // +HHMM / +HH:MM offsets; their sign does not affect date extraction.
    state = 0;
    while index + 1 < input.len() {
        let Some(value) = decimal_pair(input, index) else {
            return false;
        };
        match state {
            0 if value >= 24 => return false,
            1 if value >= 60 => return false,
            0..=1 => {}
            _ => return false,
        }
        state += 1;
        index += 2;
        if input.get(index) == Some(&b':') {
            index += 1;
            if index == input.len() {
                return false;
            }
        }
    }
    state != 0 && index == input.len()
}

pub fn is_iso_date_time_format(
    input: &[u8],
    year: &mut i32,
    month: &mut i32,
    day: &mut i32,
) -> bool {
    *year = 0;
    *month = 0;
    *day = 0;
    let Some(index) = input.iter().position(|&byte| matches!(byte, b'T' | b't')) else {
        return false;
    };
    if index != 8 && index != 10 {
        return false;
    }
    is_iso_date_format(&input[..index], year, month, day) && is_iso_time_format(&input[index + 1..])
}

pub fn date_string_to_num(input: &[u8]) -> i32 {
    let (mut year, mut month, mut day) = (0, 0, 0);
    let valid = if input.len() <= 10 {
        is_iso_date_format(input, &mut year, &mut month, &mut day)
    } else {
        is_iso_date_time_format(input, &mut year, &mut month, &mut day)
    };
    if !valid || year < 1900 {
        return 0;
    }
    // Equivalent to the source's year/month/day accumulation. Four-digit ISO
    // years keep every ordinal below 2^24, so its f32 sums are exact integers.
    (date::make_day(year, month - 1, day) - date::make_day(1900, 0, 1) + 1.) as i32
}

fn iso_date_to_num(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(param_count("IsoDate2Num"));
    }
    let value = get_simple_value(args, 0, context)?;
    if value.is_null() {
        return Ok(JsValue::null());
    }
    let value = value_to_utf8_string(&value, context)?;
    Ok(date_string_to_num(&value).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    // cfxjse_formcalc_context_unittest.cpp: GenerateSomExpression, retaining
    // every vector. Only the two source INT_MIN overflow expectations change.
    #[test]
    fn generate_som_expression_vectors() {
        for (name, flags, index, star, expected) in [
            ("", 0, 0, true, "[*]"),
            ("foo", 0, 0, true, "foo[*]"),
            ("foo", 0, 0, false, "foo"),
            ("fu", 1, 0, false, "fu[0]"),
            ("food", 1, 99, false, "food[99]"),
            ("foot", 1, -65, false, "foot[-65]"),
            ("football", 2, 0, false, "football[0]"),
            ("foosball", 2, 123, false, "foosball[+123]"),
            ("bar", 2, -654, false, "bar[-654]"),
            ("barb", 2, i32::MAX, false, "barb[+2147483647]"),
            ("bart", 2, i32::MIN, false, "bart[-2147483648]"),
            ("bark", 3, 0, false, "bark[0]"),
            ("bard", 3, 357, false, "bard[-357]"),
            ("bars", 3, -9876, false, "bars[9876]"),
            ("cars", 3, i32::MAX, false, "cars[-2147483647]"),
            ("mars", 3, i32::MIN, false, "mars[2147483648]"),
        ] {
            assert_eq!(
                generate_som_expression(name.as_bytes(), flags, index, star),
                expected.as_bytes(),
                "{name}"
            );
        }
    }

    // cfxjse_formcalc_context_unittest.cpp: IsIsoDateFormat, including all
    // output assertions and malformed-short-input vectors (crbug/539204640).
    #[test]
    fn is_iso_date_format_vectors() {
        let (mut year, mut month, mut day) = (0, 0, 0);
        assert!(!is_iso_date_format(b"", &mut year, &mut month, &mut day));
        assert!(is_iso_date_format(
            b"2023-06-24",
            &mut year,
            &mut month,
            &mut day
        ));
        assert_eq!((year, month, day), (2023, 6, 24));
        assert!(is_iso_date_format(b"2020", &mut year, &mut month, &mut day));
        assert_eq!((year, month, day), (2020, 1, 1));
        assert!(is_iso_date_format(
            b"202006", &mut year, &mut month, &mut day
        ));
        assert_eq!(month, 6);
        assert!(is_iso_date_format(
            b"20200615",
            &mut year,
            &mut month,
            &mut day
        ));
        assert_eq!(day, 15);
        for input in ["20205", "2020-", "2020061", "2020-06-"] {
            assert!(
                !is_iso_date_format(input.as_bytes(), &mut year, &mut month, &mut day),
                "{input}"
            );
        }
    }

    // cfxjse_formcalc_context_unittest.cpp: IsIsoTimeFormat, every vector.
    #[test]
    fn is_iso_time_format_vectors() {
        for input in [
            "",
            ":",
            "::",
            ":::",
            "2",
            "2:",
            "203",
            "20:3",
            "20304",
            "20:30:4",
            "2030405",
            "20:30:40:5",
            "243040-07:30",
            "24:30:40-07:30",
            "206040-07:30",
            "20:60:40-07:30",
            "203061-07:30",
            "20:30:61-07:30",
            "203040-24:30",
            "20:30:40-24:30",
            "203040-07:60",
            "20:30:40-07:60",
        ] {
            assert!(!is_iso_time_format(input.as_bytes()), "{input}");
        }
        for input in [
            "20",
            "2030",
            "20:30",
            "203040",
            "20:30:40",
            "203040.001",
            "20:30:40.001",
            "203040z",
            "20:30:40z",
            "203040+07:30",
            "20:30:40+07:30",
            "203040-07:30",
            "20:30:40-07:30",
        ] {
            assert!(is_iso_time_format(input.as_bytes()), "{input}");
        }
    }

    #[test]
    fn iso_ordinals_ranges_and_malformed_inputs() {
        for (input, expected) in [
            ("1900", 1),
            ("1900-01", 1),
            ("19000101", 1),
            ("1899-12-31", 0),
            ("1900-02-28", 59),
            ("1900-02-29", 0),
            ("1900-03-01", 60),
            ("2000-02-29", 36584),
            ("2024-02-29", 45350),
            ("20240229T20:30:60.001-07:30", 45350),
            ("2024-02-29t203040Z", 45350),
            ("9999-12-31", 2958464),
            ("2020-00-01", 0),
            ("2020-13-01", 0),
            ("2020-01-00", 0),
            ("2020-04-31", 0),
            ("2020-06x15", 0),
            ("2020-01-01T", 0),
            ("2020-01-01Tz", 0),
            ("2020-01-01T20:", 0),
            ("2020-01-01T20+", 0),
            ("2020-01-01T20ztrailing", 0),
        ] {
            assert_eq!(date_string_to_num(input.as_bytes()), expected, "{input}");
        }
    }

    #[test]
    fn iso_date_to_num_executes_through_formcalc_parser_and_boa() {
        use super::super::cfxjse_formcalc_context::CfxJseFormcalcContext;
        let context = CfxJseFormcalcContext::new().unwrap();
        for (script, expected) in [
            ("IsoDate2Num(\"1900-01-01\")", 1.),
            ("IsoDate2Num(\"20240229T203040+07:30\")", 45350.),
            ("IsoDate2Num(\"1900-02-29\")", 0.),
        ] {
            let source = script.chars().map(u32::from).collect::<Vec<_>>();
            assert_eq!(
                context.execute(&source).unwrap().as_number(),
                Some(expected)
            );
        }
        assert!(context
            .execute(
                &"IsoDate2Num(null)"
                    .chars()
                    .map(u32::from)
                    .collect::<Vec<_>>()
            )
            .unwrap()
            .is_null());
        assert!(context
            .execute(&"IsoDate2Num()".chars().map(u32::from).collect::<Vec<_>>())
            .is_err());
    }
}
