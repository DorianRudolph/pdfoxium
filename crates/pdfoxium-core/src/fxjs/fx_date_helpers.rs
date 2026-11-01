// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `fxjs/fx_date_helpers.h` and `.cpp`.
//!
//! The SDK machine-time policy, clock/localtime/timezone and character-library
//! dependencies are explicit host interfaces, not fixed default implementations.
//! Runtime callers supply Boa local time to the format parser; the native host
//! interface remains available for source tests. Wide strings are wchar32/u32 slices.
//! Source-undefined overflowing integer arithmetic/conversions and a null
//! `localtime` in GetDateTime fail with a checked panic rather than UB.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum ConversionStatus {
    Success = 0,
    BadFormat,
    BadDate,
}

/// The fields read from upstream's `struct tm`; `year` is years since 1900,
/// `year_day` is zero-based, and positive `is_dst` means daylight saving time.
#[derive(Clone, Copy, Debug)]
pub struct LocalTimeFields {
    pub year: i32,
    pub year_day: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
    pub is_dst: i32,
}

/// Preserve the two distinct native character operations. `iswalnum` follows
/// libc's current locale, whereas `FXSYS_towlower` uses ICU's simple mapping.
pub trait WideCharacterHost {
    fn is_wide_alphanumeric(&self, code_unit: u32) -> bool;
    fn lowercase_wide(&self, code_unit: u32) -> u32;
}

pub trait DateTimeHost: WideCharacterHost {
    fn machine_time_access_enabled(&self) -> bool;
    fn time(&self) -> i64;
    fn localtime(&self, epoch_seconds: i64) -> Option<LocalTimeFields>;
    /// Native `timezone`: seconds west of UTC, excluding the DST adjustment.
    fn timezone_seconds_west(&self) -> i64;
}

pub const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
pub const FULL_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS_MONTH: [u16; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
const LEAP_DAYS_MONTH: [u16; 12] = [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335];
const MONTH_ABBREVIATION_LENGTH: usize = 3;
const LONGEST_FULL_MONTH_LENGTH: usize = 9;

fn integer(value: f64) -> i32 {
    let value = value.trunc();
    assert!(
        value.is_finite() && value >= i32::MIN as f64 && value <= i32::MAX as f64,
        "upstream float-to-int conversion outside its defined range"
    );
    value as i32
}

fn modulo(x: f64, y: f64) -> f64 {
    let mut r = x % y;
    if r < 0.0 {
        r += y;
    }
    r
}

fn get_local_tza(host: &impl DateTimeHost) -> f64 {
    if !host.machine_time_access_enabled() {
        return 0.0;
    }
    let t = host.time();
    // Native localtime updates the timezone global even when its result is unused.
    let _ = host.localtime(t);
    -(host
        .timezone_seconds_west()
        .checked_mul(1000)
        .expect("timezone overflow") as f64)
}

fn get_daylight_saving_ta(d: f64, host: &impl DateTimeHost) -> i32 {
    if !host.machine_time_access_enabled() {
        return 0;
    }
    let t = (d / 1000.0).trunc();
    assert!(
        t.is_finite() && t >= i64::MIN as f64 && t < -(i64::MIN as f64),
        "upstream time_t conversion outside its defined range"
    );
    let Some(tmp) = host.localtime(t as i64) else {
        return 0;
    };
    if tmp.is_dst > 0 {
        60 * 60 * 1000
    } else {
        0
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn day_from_year(y: i32) -> i32 {
    let y = y as f64;
    integer(
        365.0 * (y - 1970.0) + ((y - 1969.0) / 4.0).floor() - ((y - 1901.0) / 100.0).floor()
            + ((y - 1601.0) / 400.0).floor(),
    )
}

fn time_from_year(y: i32) -> f64 {
    86_400_000.0 * day_from_year(y) as f64
}

fn time_from_year_month(y: i32, m: i32) -> f64 {
    let month = if is_leap_year(y) {
        LEAP_DAYS_MONTH[m as usize]
    } else {
        DAYS_MONTH[m as usize]
    };
    time_from_year(y) + month as f64 * 86_400_000.0
}

fn day(t: f64) -> i32 {
    integer((t / 86_400_000.0).floor())
}

fn year_from_time(t: f64) -> i32 {
    // Estimate the time.
    let mut y = 1970i32
        .checked_add(integer(t / (365.2425 * 86_400_000.0)))
        .expect("year overflow");
    if time_from_year(y) <= t {
        while time_from_year(y.checked_add(1).expect("year overflow")) <= t {
            y = y.checked_add(1).expect("year overflow");
        }
    } else {
        while time_from_year(y) > t {
            y = y.checked_sub(1).expect("year overflow");
        }
    }
    y
}

fn day_within_year(t: f64) -> i32 {
    let year = year_from_time(t);
    day(t)
        .checked_sub(day_from_year(year))
        .expect("day overflow")
}

fn month_from_time(t: f64) -> i32 {
    // Check for negative |day| values and check for January.
    let mut day = day_within_year(t);
    if day < 0 {
        return -1;
    }
    if day < 31 {
        return 0;
    }
    if is_leap_year(year_from_time(t)) {
        day -= 1;
    }
    // Check for February onwards.
    const CUMULATIVE_DAYS_IN_MONTHS: [i32; 11] =
        [59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365];
    for (i, &days) in CUMULATIVE_DAYS_IN_MONTHS.iter().enumerate() {
        if day < days {
            return i as i32 + 1;
        }
    }
    -1
}

fn date_from_time(t: f64) -> i32 {
    let day = day_within_year(t);
    let leap = i32::from(is_leap_year(year_from_time(t)));
    match month_from_time(t) {
        0 => day + 1,
        1 => day - 30,
        2 => day - 58 - leap,
        3 => day - 89 - leap,
        4 => day - 119 - leap,
        5 => day - 150 - leap,
        6 => day - 180 - leap,
        7 => day - 211 - leap,
        8 => day - 242 - leap,
        9 => day - 272 - leap,
        10 => day - 303 - leap,
        11 => day - 333 - leap,
        _ => 0,
    }
}

fn find_subword_length(value: &[u32], start: usize, host: &impl WideCharacterHost) -> usize {
    let mut i = start;
    while i < value.len() && host.is_wide_alphanumeric(value[i]) {
        i += 1;
    }
    i - start
}

pub fn get_date_time(host: &impl DateTimeHost) -> f64 {
    if !host.machine_time_access_enabled() {
        return 0.0;
    }
    let t = host.time();
    let tm = host
        .localtime(t)
        .expect("upstream GetDateTime requires localtime success");
    let t1 = time_from_year(tm.year.checked_add(1900).expect("year overflow"));
    t1 + tm.year_day as f64 * 86_400_000.0
        + tm.hour as f64 * 3_600_000.0
        + tm.minute as f64 * 60_000.0
        + tm.second as f64 * 1000.0
}

pub fn get_year_from_time(dt: f64) -> i32 {
    year_from_time(dt)
}
pub fn get_month_from_time(dt: f64) -> i32 {
    month_from_time(dt)
}
pub fn get_day_from_time(dt: f64) -> i32 {
    date_from_time(dt)
}
pub fn get_day_of_week_from_time(dt: f64) -> i32 {
    // 1970-01-01 was Thursday (day 4 in 0-based Sunday-indexed week).
    integer(modulo(
        day(dt).checked_add(4).expect("day overflow") as f64,
        7.0,
    ))
}
pub fn get_hour_from_time(dt: f64) -> i32 {
    integer(modulo((dt / 3_600_000.0).floor(), 24.0))
}
pub fn get_min_from_time(dt: f64) -> i32 {
    integer(modulo((dt / 60_000.0).floor(), 60.0))
}
pub fn get_sec_from_time(dt: f64) -> i32 {
    integer(modulo((dt / 1000.0).floor(), 60.0))
}
pub fn is_valid_month(m: i32) -> bool {
    (1..=12).contains(&m)
}
// TODO(thestig): Should this take the month into consideration?
pub fn is_valid_day(d: i32) -> bool {
    (1..=31).contains(&d)
}
// TODO(thestig): Should 24 be allowed? Similarly, 60 for minutes and seconds.
pub fn is_valid_24_hour(h: i32) -> bool {
    (0..=24).contains(&h)
}
pub fn is_valid_minute(m: i32) -> bool {
    (0..=60).contains(&m)
}
pub fn is_valid_second(s: i32) -> bool {
    (0..=60).contains(&s)
}

pub fn local_time(d: f64, host: &impl DateTimeHost) -> f64 {
    d + get_local_tza(host) + get_daylight_saving_ta(d, host) as f64
}

pub fn make_day(year: i32, month: i32, date: i32) -> f64 {
    let y = year as f64;
    let m = month as f64;
    let dt = date as f64;
    let ym = y + (m / 12.0).floor();
    let mn = modulo(m, 12.0);
    let t = time_from_year_month(integer(ym), integer(mn));
    if year_from_time(t) as f64 != ym || month_from_time(t) as f64 != mn || date_from_time(t) != 1 {
        return f64::NAN;
    }
    day(t) as f64 + dt - 1.0
}

pub fn make_time(hour: i32, min: i32, sec: i32, ms: i32) -> f64 {
    hour as f64 * 3_600_000.0 + min as f64 * 60_000.0 + sec as f64 * 1000.0 + ms as f64
}

pub fn make_date(day: f64, time: f64) -> f64 {
    if !day.is_finite() || !time.is_finite() {
        return f64::NAN;
    }
    day * 86_400_000.0 + time
}

pub fn parse_string_integer(value: &[u32], start: usize, skip: &mut usize, max_step: usize) -> i32 {
    let mut result = 0i32;
    let mut skipped = 0;
    for (i, &c) in value.iter().enumerate().skip(start) {
        if i - start > 10 {
            break;
        }
        if !(u32::from(b'0')..=u32::from(b'9')).contains(&c) {
            break;
        }
        result = result
            .checked_mul(10)
            .and_then(|r| r.checked_add((c - u32::from(b'0')) as i32))
            .expect("upstream integer parsing overflow");
        skipped += 1;
        if skipped >= max_step {
            break;
        }
    }
    *skip = skipped;
    result
}

fn equals_ascii_no_case(value: &[u32], ascii: &str) -> bool {
    value.len() == ascii.len()
        && value.iter().zip(ascii.bytes()).all(|(&a, b)| {
            let a = if (u32::from(b'A')..=u32::from(b'Z')).contains(&a) {
                a + 32
            } else {
                a
            };
            a == u32::from(b.to_ascii_lowercase())
        })
}

pub fn parse_date_using_format(
    value: &[u32],
    format: &[u32],
    result: &mut f64,
    host: &impl DateTimeHost,
) -> ConversionStatus {
    parse_date_using_format_at(value, format, result, get_date_time(host), host)
}

/// Source format parser with the caller's current local wall-clock time.
pub fn parse_date_using_format_at(
    value: &[u32],
    format: &[u32],
    result: &mut f64,
    dt: f64,
    host: &impl WideCharacterHost,
) -> ConversionStatus {
    if format.is_empty() || value.is_empty() {
        *result = dt;
        return ConversionStatus::Success;
    }
    let mut year = get_year_from_time(dt);
    let mut month = get_month_from_time(dt) + 1;
    let mut day = get_day_from_time(dt);
    let mut hour = get_hour_from_time(dt);
    let mut minute = get_min_from_time(dt);
    let mut second = get_sec_from_time(dt);
    #[derive(PartialEq)]
    enum AmPm {
        None,
        Am,
        Pm,
    }
    let mut am_pm = AmPm::None;
    let mut exit_loop = false;
    let mut bad_format = false;
    let mut format_idx = 0;
    let mut value_idx = 0;
    while format_idx < format.len() {
        if exit_loop {
            break;
        }
        let format_char = format[format_idx];
        let format_character = char::from_u32(format_char);
        match format_character {
            Some(':' | '.' | '-' | '\\' | '/') => {
                // ':', '.', '-', '\\', '/'
                format_idx += 1;
                value_idx += 1;
            }
            Some('y' | 'm' | 'd' | 'H' | 'h' | 'M' | 's' | 't') => {
                // y m d H h M s t
                let old_value_idx = value_idx;
                let mut chars_to_skip = 0;
                let mut token_len = 1;
                while format_idx + token_len < format.len()
                    && format[format_idx + token_len] == format_char
                {
                    token_len += 1;
                }
                if token_len <= 2 {
                    match format_character {
                        Some('y') => {
                            if token_len == 1 {
                                value_idx += 1;
                            } else {
                                year =
                                    parse_string_integer(value, value_idx, &mut chars_to_skip, 2);
                                value_idx += chars_to_skip;
                            }
                        }
                        Some('m' | 'd' | 'H' | 'h' | 'M' | 's') => {
                            let parsed =
                                parse_string_integer(value, value_idx, &mut chars_to_skip, 2);
                            match format_character {
                                Some('m') => month = parsed,
                                Some('d') => day = parsed,
                                Some('H' | 'h') => hour = parsed,
                                Some('M') => minute = parsed,
                                Some('s') => second = parsed,
                                _ => unreachable!(),
                            }
                            value_idx += chars_to_skip;
                        }
                        Some('t') => {
                            if value_idx + token_len <= value.len() {
                                let marker = &value[value_idx..value_idx + token_len];
                                if equals_ascii_no_case(
                                    marker,
                                    if token_len == 1 { "p" } else { "pm" },
                                ) {
                                    am_pm = AmPm::Pm;
                                    value_idx += token_len;
                                } else if equals_ascii_no_case(
                                    marker,
                                    if token_len == 1 { "a" } else { "am" },
                                ) {
                                    am_pm = AmPm::Am;
                                    value_idx += token_len;
                                }
                            }
                        }
                        _ => unreachable!(),
                    }
                    format_idx += token_len;
                } else if token_len == 3 {
                    match format_character {
                        Some('m') => {
                            let mut found = false;
                            chars_to_skip = find_subword_length(value, value_idx, host);
                            if chars_to_skip == MONTH_ABBREVIATION_LENGTH {
                                let month_str =
                                    &value[value_idx..value_idx + MONTH_ABBREVIATION_LENGTH];
                                for (m, name) in MONTHS.iter().enumerate() {
                                    if equals_ascii_no_case(month_str, name) {
                                        month = m as i32 + 1;
                                        format_idx += 3;
                                        value_idx += chars_to_skip;
                                        found = true;
                                        break;
                                    }
                                }
                            }
                            if !found {
                                month =
                                    parse_string_integer(value, value_idx, &mut chars_to_skip, 3);
                                format_idx += 3;
                                value_idx += chars_to_skip;
                            }
                        }
                        Some('y') => {}
                        _ => {
                            format_idx += 3;
                            value_idx += 3;
                        }
                    }
                } else if token_len == 4 {
                    match format_character {
                        Some('y') => {
                            year = parse_string_integer(value, value_idx, &mut chars_to_skip, 4);
                            format_idx += 4;
                            value_idx += chars_to_skip;
                        }
                        Some('m') => {
                            let mut found = false;
                            chars_to_skip = find_subword_length(value, value_idx, host);
                            if chars_to_skip <= LONGEST_FULL_MONTH_LENGTH {
                                // Native Substr tolerates an out-of-range start, returning empty.
                                let source = value
                                    .get(value_idx..value_idx + chars_to_skip)
                                    .unwrap_or(&[]);
                                let month_str: Vec<u32> =
                                    source.iter().map(|&c| host.lowercase_wide(c)).collect();
                                for (m, name) in FULL_MONTHS.iter().enumerate() {
                                    let full_month: Vec<u32> = name
                                        .bytes()
                                        .map(|c| host.lowercase_wide(u32::from(c)))
                                        .collect();
                                    if month_str.is_empty()
                                        || full_month
                                            .windows(month_str.len())
                                            .any(|s| s == month_str)
                                    {
                                        month = m as i32 + 1;
                                        format_idx += 4;
                                        value_idx += chars_to_skip;
                                        found = true;
                                        break;
                                    }
                                }
                            }
                            if !found {
                                month =
                                    parse_string_integer(value, value_idx, &mut chars_to_skip, 4);
                                format_idx += 4;
                                value_idx += chars_to_skip;
                            }
                        }
                        _ => {
                            format_idx += 4;
                            value_idx += 4;
                        }
                    }
                } else {
                    if value_idx >= value.len() || format[format_idx] != value[value_idx] {
                        bad_format = true;
                        exit_loop = true;
                    }
                    format_idx += 1;
                    value_idx += 1;
                }
                if old_value_idx == value_idx {
                    bad_format = true;
                    exit_loop = true;
                }
            }
            _ => {
                if value.len() <= value_idx {
                    exit_loop = true;
                } else if format[format_idx] != value[value_idx] {
                    bad_format = true;
                    exit_loop = true;
                }
                format_idx += 1;
                value_idx += 1;
            }
        }
    }
    if bad_format {
        return ConversionStatus::BadFormat;
    }
    // Adjust 12-hour clock (1-12) to 24-hour clock (0-23), after the loop
    // because 't' can precede 'h' in format strings.
    if am_pm != AmPm::None {
        hour = (hour % 12) + if am_pm == AmPm::Pm { 12 } else { 0 };
    }
    // Acrobat's date horizon: <50 => 2000, >=50 => 1900.
    if (0..100).contains(&year) {
        year += if year < 50 { 2000 } else { 1900 };
    }
    if !is_valid_month(month)
        || !is_valid_day(day)
        || !is_valid_24_hour(hour)
        || !is_valid_minute(minute)
        || !is_valid_second(second)
    {
        return ConversionStatus::BadDate;
    }
    let parsed_dt = make_date(
        make_day(year, month - 1, day),
        make_time(hour, minute, second, 0),
    );
    if parsed_dt.is_nan() {
        return ConversionStatus::BadDate;
    }
    *result = parsed_dt;
    ConversionStatus::Success
}

#[cfg(test)]
#[path = "fx_date_helpers_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
#[path = "fx_date_helpers_regression_tests.rs"]
mod regression_tests;
