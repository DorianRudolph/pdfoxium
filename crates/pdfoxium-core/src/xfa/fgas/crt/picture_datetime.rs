// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Calendar fields and parsing from `cfgas_stringformatter.cpp`. This is the
//! XFA picture grammar's field representation, not a clock or timezone engine.
//! Calendar arithmetic reuses fx_date_helpers; locale offsets are supplied by
//! the host through LocaleIface, whose production provider uses Boa.
use super::cfgas_stringformatter::get_literal_text;
use super::locale_iface::LocaleIface;
use crate::fxjs::fx_date_helpers as calendar;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PictureDateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millisecond: u16,
}
impl PictureDateTime {
    pub const fn new(
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        millisecond: u16,
    ) -> Self {
        Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            millisecond,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn is_set(&self) -> bool {
        *self != Self::default()
    }
    pub fn set_date(&mut self, year: i32, month: u8, day: u8) {
        self.year = year;
        self.month = month;
        self.day = day;
    }
    pub fn set_time(&mut self, hour: u8, minute: u8, second: u8, millisecond: u16) {
        self.hour = hour;
        self.minute = minute;
        self.second = second;
        self.millisecond = millisecond;
    }
    pub fn get_year(&self) -> i32 {
        self.year
    }
    pub fn get_month(&self) -> u8 {
        self.month
    }
    pub fn get_day(&self) -> u8 {
        self.day
    }
    pub fn get_hour(&self) -> u8 {
        self.hour
    }
    pub fn get_minute(&self) -> u8 {
        self.minute
    }
    pub fn get_second(&self) -> u8 {
        self.second
    }
    pub fn get_millisecond(&self) -> u16 {
        self.millisecond
    }
    pub fn get_day_of_week(&self) -> i32 {
        if !valid_date(self.year, u32::from(self.month), u32::from(self.day)) {
            return 0;
        }
        calendar::get_day_of_week_from_time(
            calendar::make_day(self.year, i32::from(self.month) - 1, i32::from(self.day))
                * 86_400_000.0,
        )
    }
}

pub(super) fn days_in_month(year: i32, month: u32) -> u32 {
    if !(1..=12).contains(&month) {
        return 0;
    }
    (calendar::make_day(year, month as i32, 1) - calendar::make_day(year, month as i32 - 1, 1))
        as u32
}
fn valid_date(year: i32, month: u32, day: u32) -> bool {
    (0..=9999).contains(&year)
        && (1..=12).contains(&month)
        && day != 0
        && day <= days_in_month(year, month)
}
fn is_digit(ch: u32) -> bool {
    (0x30..=0x39).contains(&ch)
}

/// Native ExtractCountDigits: retains the successfully consumed prefix on
/// failure, but checked accumulation cannot wrap on maliciously long input.
fn extract_count_digits(input: &[u32], count: usize, cursor: &mut usize, value: &mut u32) -> bool {
    for _ in 0..count {
        let Some(&ch) = input.get(*cursor).filter(|&&ch| is_digit(ch)) else {
            return false;
        };
        let Some(next) = value.checked_mul(10).and_then(|v| v.checked_add(ch - 0x30)) else {
            return false;
        };
        *value = next;
        *cursor += 1;
    }
    true
}
fn extract_count_digits_with_optional(
    input: &[u32],
    count: usize,
    cursor: &mut usize,
    value: &mut u32,
) -> bool {
    if !extract_count_digits(input, count, cursor, value) {
        return false;
    }
    extract_count_digits(input, 1, cursor, value);
    true
}

/// Offset syntax used by the source's z/GMT picture clauses. A two-digit hour
/// alone is valid; a present minute component must contain two valid digits.
fn parse_time_zone(input: &[u32]) -> Option<(usize, i32)> {
    let negative = match input.first().copied()? {
        0x2d => true,
        0x2b => false,
        _ => return None,
    };
    let mut cursor = 1;
    let mut hour = 0;
    if !extract_count_digits(input, 2, &mut cursor, &mut hour) || hour > 23 {
        return None;
    }
    let mut minute = 0;
    if input.get(cursor) == Some(&0x3a) {
        cursor += 1;
        if !extract_count_digits(input, 2, &mut cursor, &mut minute) {
            return None;
        }
    } else if input.get(cursor).copied().is_some_and(is_digit)
        && !extract_count_digits(input, 2, &mut cursor, &mut minute)
    {
        return None;
    }
    if minute > 59 {
        return None;
    }
    let offset = (hour * 60 + minute) as i32;
    Some((cursor, if negative { -offset } else { offset }))
}
fn resolve_zone(offset: i32, locale: &dyn LocaleIface, hour: &mut u32, minute: &mut u32) {
    let total =
        (i64::from(*hour) * 60 + i64::from(*minute) + i64::from(locale.get_time_zone_in_minutes())
            - i64::from(offset))
        .rem_euclid(1440);
    *hour = (total / 60) as u32;
    *minute = (total % 60) as u32;
}
fn consume(input: &[u32], cursor: &mut usize, text: &[u32]) -> bool {
    if input
        .get(*cursor..)
        .is_some_and(|tail| tail.starts_with(text))
    {
        *cursor += text.len();
        true
    } else {
        false
    }
}
fn next_symbol(pattern: &[u32], index: &mut usize) -> (u32, usize) {
    let symbol = pattern[*index];
    let start = *index;
    while pattern.get(*index) == Some(&symbol) {
        *index += 1;
    }
    (symbol, *index - start)
}

/// Consume one complete date picture, leaving a shared cursor for a following
/// time picture. The outer formatter checks complete input consumption.
pub(super) fn parse_locale_date(
    input: &[u32],
    pattern: &[u32],
    locale: &dyn LocaleIface,
    out: &mut PictureDateTime,
    cursor: &mut usize,
) -> bool {
    let start = *cursor;
    let (mut year, mut month, mut day) = (1900u32, 1u32, 1u32);
    let mut index = 0;
    while index < pattern.len() {
        if pattern[index] == 0x27 {
            let literal = get_literal_text(pattern, &mut index);
            if !consume(input, cursor, &literal) {
                return false;
            }
            continue;
        }
        if ![0x44, 0x4a, 0x4d, 0x45, 0x65, 0x47, 0x67, 0x59, 0x77, 0x57].contains(&pattern[index]) {
            if !consume(input, cursor, &pattern[index..index + 1]) {
                return false;
            }
            index += 1;
            continue;
        }
        let (symbol, count) = next_symbol(pattern, &mut index);
        match (symbol, count) {
            (0x44, 1 | 2) => {
                day = 0;
                if !extract_count_digits_with_optional(input, 1, cursor, &mut day) {
                    return false;
                }
            }
            (0x4d, 1 | 2) => {
                month = 0;
                if !extract_count_digits_with_optional(input, 1, cursor, &mut month) {
                    return false;
                }
            }
            (0x4d, 3 | 4) => {
                let mut matched = false;
                for number in 0..12 {
                    let name = locale.get_month_name(number, count == 3);
                    if !name.is_empty() && consume(input, cursor, &name) {
                        month = number as u32 + 1;
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return false;
                }
            }
            (0x45, 3 | 4) => {
                let mut matched = false;
                for number in 0..7 {
                    let name = locale.get_day_name(number, count == 3);
                    if !name.is_empty() && consume(input, cursor, &name) {
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return false;
                }
            }
            (0x59, 2 | 4) => {
                year = 0;
                if !extract_count_digits(input, count, cursor, &mut year) {
                    return false;
                }
                if count == 2 {
                    year += if year <= 29 { 2000 } else { 1900 };
                }
            }
            (0x47, 1) => {
                // The selected source grammar recognizes the era text but
                // does not convert BC to a proleptic negative year.
                let ad = locale.get_era_name(true);
                let bc = locale.get_era_name(false);
                if !(!ad.is_empty() && consume(input, cursor, &ad))
                    && !(!bc.is_empty() && consume(input, cursor, &bc))
                {
                    return false;
                }
            }
            (0x4a, 1 | 3) => {
                // J/JJJ remain source-selected ignored ordinal fields. Consume
                // up to three digits safely; the native J case includes 33.
                let mut digits = 0;
                let mut ignored = 0;
                while digits < 3 && input.get(*cursor).copied().is_some_and(is_digit) {
                    extract_count_digits(input, 1, cursor, &mut ignored);
                    digits += 1;
                }
                if digits == 0 {
                    return false;
                }
            }
            (0x45 | 0x65, 1) | (0x77, 1) | (0x57, 2) => {
                let mut ignored = 0;
                if !extract_count_digits(input, count, cursor, &mut ignored) {
                    return false;
                }
            }
            _ => {}
        }
    }
    if *cursor == start || !valid_date(year as i32, month, day) {
        return false;
    }
    out.set_date(year as i32, month as u8, day as u8);
    true
}

/// Consume one complete time picture. Named offsets are picture tokens, not a
/// timezone database; local adjustment is delegated to the locale's host hook.
pub(super) fn parse_locale_time(
    input: &[u32],
    pattern: &[u32],
    locale: &dyn LocaleIface,
    out: &mut PictureDateTime,
    cursor: &mut usize,
) -> bool {
    let start = *cursor;
    let (mut hour, mut minute, mut second, mut millisecond) = (0u32, 0u32, 0u32, 0u32);
    let (mut index, mut hour_symbol) = (0usize, 0u32);
    let mut meridiem = None;
    let mut zone = None;
    while index < pattern.len() {
        if pattern[index] == 0x27 {
            let literal = get_literal_text(pattern, &mut index);
            if !consume(input, cursor, &literal) {
                return false;
            }
            continue;
        }
        if ![0x68, 0x48, 0x6b, 0x4b, 0x4d, 0x53, 0x46, 0x41, 0x7a, 0x5a].contains(&pattern[index]) {
            if !consume(input, cursor, &pattern[index..index + 1]) {
                return false;
            }
            index += 1;
            continue;
        }
        let (symbol, count) = next_symbol(pattern, &mut index);
        match (symbol, count) {
            (0x68 | 0x48 | 0x6b | 0x4b, 1 | 2) => {
                hour = 0;
                hour_symbol = symbol;
                let valid = if count == 1 {
                    extract_count_digits_with_optional(input, 1, cursor, &mut hour)
                } else {
                    extract_count_digits(input, 2, cursor, &mut hour)
                };
                if !valid {
                    return false;
                }
                if symbol == 0x4b && hour == 24 {
                    hour = 0;
                }
            }
            (0x4d, 1 | 2) => {
                minute = 0;
                let valid = if count == 1 {
                    extract_count_digits_with_optional(input, 1, cursor, &mut minute)
                } else {
                    extract_count_digits(input, 2, cursor, &mut minute)
                };
                if !valid {
                    return false;
                }
            }
            (0x53, 1 | 2) => {
                second = 0;
                let valid = if count == 1 {
                    extract_count_digits_with_optional(input, 1, cursor, &mut second)
                } else {
                    extract_count_digits(input, 2, cursor, &mut second)
                };
                if !valid {
                    return false;
                }
            }
            (0x46, 3) => {
                millisecond = 0;
                if !extract_count_digits(input, 3, cursor, &mut millisecond) {
                    return false;
                }
            }
            (0x41, 1) => {
                let am = locale.get_meridiem_name(true);
                let pm = locale.get_meridiem_name(false);
                if !am.is_empty() && consume(input, cursor, &am) {
                    meridiem = Some(false);
                } else if !pm.is_empty() && consume(input, cursor, &pm) {
                    meridiem = Some(true);
                } else {
                    return false;
                }
            }
            (0x5a, 1) => {
                let Some(text) = input.get(*cursor..).and_then(|tail| tail.get(..3)) else {
                    return false;
                };
                *cursor += 3;
                if text == [0x47, 0x4d, 0x54] {
                    if matches!(input.get(*cursor), Some(0x2b | 0x2d)) {
                        let Some((used, offset)) = parse_time_zone(&input[*cursor..]) else {
                            return false;
                        };
                        *cursor += used;
                        zone = Some(offset);
                    } else {
                        zone = Some(0);
                    }
                } else {
                    const ZONES: [(&[u8], i32); 8] = [
                        (b"CDT", -300),
                        (b"CST", -360),
                        (b"EDT", -240),
                        (b"EST", -300),
                        (b"MDT", -360),
                        (b"MST", -420),
                        (b"PDT", -420),
                        (b"PST", -480),
                    ];
                    zone = ZONES
                        .iter()
                        .find(|(name, _)| text.iter().zip(*name).all(|(&a, &b)| a == u32::from(b)))
                        .map(|(_, offset)| *offset);
                    if zone.is_none() {
                        return false;
                    }
                }
            }
            (0x7a, 1) => {
                if input.get(*cursor) == Some(&0x5a) {
                    *cursor += 1;
                    zone = Some(0);
                } else {
                    let Some((used, offset)) = input.get(*cursor..).and_then(parse_time_zone)
                    else {
                        return false;
                    };
                    *cursor += used;
                    zone = Some(offset);
                }
            }
            _ => {}
        }
    }
    if matches!(hour_symbol, 0x68 | 0x6b) {
        if !(1..=12).contains(&hour) {
            return false;
        }
    }
    if hour_symbol == 0 || matches!(hour_symbol, 0x68 | 0x6b) {
        if let Some(pm) = meridiem {
            hour = hour % 12 + if pm { 12 } else { 0 };
        }
    }
    if *cursor == start || hour > 23 || minute > 59 || second > 59 || millisecond > 999 {
        return false;
    }
    if let Some(offset) = zone {
        resolve_zone(offset, locale, &mut hour, &mut minute);
    }
    out.set_time(hour as u8, minute as u8, second as u8, millisecond as u16);
    true
}

/// Source canonical date grammar, rejecting trailing text and mismatched
/// separators rather than silently accepting an incomplete numeric prefix.
pub fn date_from_canonical(input: &[u32], out: &mut PictureDateTime) -> bool {
    if input.len() > 10 {
        return false;
    }
    let (mut cursor, mut year) = (0usize, 0u32);
    if !extract_count_digits(input, 4, &mut cursor, &mut year) || year < 1900 {
        return false;
    }
    if cursor == input.len() {
        out.set_date(year as i32, 1, 1);
        return true;
    }
    let separated = input[cursor] == 0x2d;
    if separated {
        cursor += 1;
    }
    let mut month = 0;
    if !extract_count_digits(input, 2, &mut cursor, &mut month) || !(1..=12).contains(&month) {
        return false;
    }
    if cursor == input.len() {
        out.set_date(year as i32, month as u8, 1);
        return true;
    }
    if separated {
        if input.get(cursor) != Some(&0x2d) {
            return false;
        }
        cursor += 1;
    }
    let mut day = 0;
    if !extract_count_digits(input, 2, &mut cursor, &mut day)
        || cursor != input.len()
        || !valid_date(year as i32, month, day)
    {
        return false;
    }
    out.set_date(year as i32, month as u8, day as u8);
    true
}

/// Source canonical time grammar with explicit Z/numeric zone adjustment.
/// The source skipped Z and arbitrary suffix text; here every suffix is checked.
pub fn time_from_canonical(
    locale: &dyn LocaleIface,
    input: &[u32],
    out: &mut PictureDateTime,
) -> bool {
    let (mut cursor, mut hour, mut minute, mut second, mut millisecond) =
        (0usize, 0u32, 0u32, 0u32, 0u32);
    if !extract_count_digits(input, 2, &mut cursor, &mut hour) || hour > 23 {
        return false;
    }
    let mut has_seconds = false;
    if input
        .get(cursor)
        .is_some_and(|&ch| ch == 0x3a || is_digit(ch))
    {
        let separated = input[cursor] == 0x3a;
        if separated {
            cursor += 1;
        }
        if !extract_count_digits(input, 2, &mut cursor, &mut minute) || minute > 59 {
            return false;
        }
        if input
            .get(cursor)
            .is_some_and(|&ch| ch == 0x3a || is_digit(ch))
        {
            if separated {
                if input.get(cursor) != Some(&0x3a) {
                    return false;
                }
                cursor += 1;
            } else if input[cursor] == 0x3a {
                return false;
            }
            if !extract_count_digits(input, 2, &mut cursor, &mut second) || second > 59 {
                return false;
            }
            has_seconds = true;
        }
    }
    if input.get(cursor) == Some(&0x2e) {
        if !has_seconds {
            return false;
        }
        cursor += 1;
        if !extract_count_digits(input, 3, &mut cursor, &mut millisecond) {
            return false;
        }
    }
    while input.get(cursor) == Some(&0x20) {
        cursor += 1;
    }
    let zone = if input.get(cursor) == Some(&0x5a) {
        cursor += 1;
        Some(0)
    } else if cursor < input.len() {
        let Some((used, offset)) = parse_time_zone(&input[cursor..]) else {
            return false;
        };
        cursor += used;
        Some(offset)
    } else {
        None
    };
    while input.get(cursor) == Some(&0x20) {
        cursor += 1;
    }
    if cursor != input.len() {
        return false;
    }
    if let Some(offset) = zone {
        resolve_zone(offset, locale, &mut hour, &mut minute);
    }
    out.set_time(hour as u8, minute as u8, second as u8, millisecond as u16);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xfa::fgas::crt::locale_mgr_iface::LocaleMgrIface;
    use crate::xfa::fxfa::parser::cxfa_localemgr::CxfaLocaleMgr;
    use std::rc::Rc;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn locale(offset: i32) -> Rc<dyn LocaleIface> {
        CxfaLocaleMgr::new(&wide("en_US"), Rc::new(move || offset))
            .get_def_locale()
            .unwrap()
    }
    #[test]
    fn canonical_dates_validate_complete_input_and_share_calendar_arithmetic() {
        let mut fields = PictureDateTime::default();
        for (input, expected) in [
            ("1900", (1900, 1, 1)),
            ("200002", (2000, 2, 1)),
            ("2000-02", (2000, 2, 1)),
            ("20000229", (2000, 2, 29)),
            ("2000-02-29", (2000, 2, 29)),
            ("9999-12-31", (9999, 12, 31)),
        ] {
            assert!(date_from_canonical(&wide(input), &mut fields), "{input}");
            assert_eq!((fields.year, fields.month, fields.day), expected);
        }
        for input in [
            "",
            "1899",
            "2000-",
            "2000-0",
            "1900-02-29",
            "2100-02-29",
            "2000-02-30",
            "2000-00-01",
            "2000-01-00",
            "2000-0101",
            "200001-01",
            "2000-01-1",
            "2000-01-01x",
            "200001011",
        ] {
            assert!(!date_from_canonical(&wide(input), &mut fields), "{input}");
        }
        fields.set_date(2000, 1, 1);
        assert_eq!(fields.get_day_of_week(), 6);
        fields.set_date(1970, 1, 1);
        assert_eq!(fields.get_day_of_week(), 4);
        fields.reset();
        assert!(!fields.is_set());
    }
    #[test]
    fn canonical_times_use_explicit_host_offset_and_reject_malformed_suffixes() {
        let locale = locale(-120);
        let mut fields = PictureDateTime::default();
        for (input, expected) in [
            ("12", (12, 0, 0, 0)),
            ("1201", (12, 1, 0, 0)),
            ("12:01:11.001", (12, 1, 11, 1)),
            ("12:01:11 +04:30", (5, 31, 11, 0)),
            ("01:30Z", (23, 30, 0, 0)),
            ("12:01:11-04", (14, 1, 11, 0)),
            ("12:01:11-0430", (14, 31, 11, 0)),
        ] {
            assert!(
                time_from_canonical(locale.as_ref(), &wide(input), &mut fields),
                "{input}"
            );
            assert_eq!(
                (
                    fields.hour,
                    fields.minute,
                    fields.second,
                    fields.millisecond
                ),
                expected
            );
        }
        for input in [
            "",
            "24",
            "12:",
            "1111:",
            "12:60",
            "12:01:60",
            "12:01:00.1",
            "12:01:00.0012",
            "12:01garbage",
            "12:01Zjunk",
            "12:01+",
            "12:01+2",
            "12:01+24:00",
            "12:01+01:60",
            "12:01+01:",
            "12:0101",
            "1201:01",
        ] {
            assert!(
                !time_from_canonical(locale.as_ref(), &wide(input), &mut fields),
                "{input}"
            );
        }
    }
    #[test]
    fn sequential_pictures_bound_names_and_resolve_named_zone_after_meridiem() {
        let locale = locale(0);
        let mut fields = PictureDateTime::default();
        let mut cursor = 0;
        let input = wide("January 2, 2024 1:05:10 PM PST");
        assert!(parse_locale_date(
            &input,
            &wide("MMMM D, YYYY "),
            locale.as_ref(),
            &mut fields,
            &mut cursor
        ));
        assert!(parse_locale_time(
            &input,
            &wide("h:MM:SS A Z"),
            locale.as_ref(),
            &mut fields,
            &mut cursor
        ));
        assert_eq!(cursor, input.len());
        assert_eq!(
            (
                fields.year,
                fields.month,
                fields.day,
                fields.hour,
                fields.minute
            ),
            (2024, 1, 2, 21, 5)
        );
        assert!(parse_locale_time(
            &wide("PM"),
            &wide("A"),
            locale.as_ref(),
            &mut fields,
            &mut 0
        ));
        assert_eq!(fields.hour, 12);
        for (input, pattern) in [
            ("Jan", "MMMM"),
            ("Frog 2, 2024", "MMM D, YYYY"),
            ("Feb 30, 2024", "MMM D, YYYY"),
        ] {
            assert!(
                !parse_locale_date(
                    &wide(input),
                    &wide(pattern),
                    locale.as_ref(),
                    &mut fields,
                    &mut 0
                ),
                "{input}"
            );
        }
        for (input, pattern) in [
            ("1:05 XM", "h:MM A"),
            ("1:05 PM XYZ", "h:MM A Z"),
            ("25:00", "HH:MM"),
            ("12:00+", "HH:MMz"),
        ] {
            assert!(
                !parse_locale_time(
                    &wide(input),
                    &wide(pattern),
                    locale.as_ref(),
                    &mut fields,
                    &mut 0
                ),
                "{input}"
            );
        }
    }
}
