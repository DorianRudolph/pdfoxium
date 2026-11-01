// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use super::*;
use std::cell::RefCell;

struct Host {
    enabled: bool,
    is_dst: i32,
    fails_localtime: bool,
    calls: RefCell<Vec<String>>,
}

impl WideCharacterHost for Host {
    fn is_wide_alphanumeric(&self, c: u32) -> bool {
        c == 0x0130 || u8::try_from(c).is_ok_and(|c| c.is_ascii_alphanumeric())
    }
    fn lowercase_wide(&self, c: u32) -> u32 {
        // An ICU simple mapping which Rust's full lowercase would expand.
        if c == 0x0130 {
            return u32::from(b'i');
        }
        if (u32::from(b'A')..=u32::from(b'Z')).contains(&c) {
            c + 32
        } else {
            c
        }
    }
}
impl DateTimeHost for Host {
    fn machine_time_access_enabled(&self) -> bool {
        self.calls.borrow_mut().push("policy".into());
        self.enabled
    }
    fn time(&self) -> i64 {
        self.calls.borrow_mut().push("time".into());
        0
    }
    fn localtime(&self, t: i64) -> Option<LocalTimeFields> {
        self.calls.borrow_mut().push(format!("localtime({t})"));
        (!self.fails_localtime).then_some(LocalTimeFields {
            year: 70,
            year_day: 0,
            hour: 3,
            minute: 4,
            second: 5,
            is_dst: self.is_dst,
        })
    }
    fn timezone_seconds_west(&self) -> i64 {
        self.calls.borrow_mut().push("timezone".into());
        -7200
    }
}
fn host(enabled: bool, is_dst: i32, fails_localtime: bool) -> Host {
    Host {
        enabled,
        is_dst,
        fails_localtime,
        calls: RefCell::new(Vec::new()),
    }
}
fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}

#[test]
fn clock_policy_local_fields_timezone_and_dst_order() {
    let h = host(false, 1, false);
    assert_eq!(get_date_time(&h), 0.0);
    assert_eq!(*h.calls.borrow(), ["policy"]);
    h.calls.borrow_mut().clear();
    assert_eq!(local_time(-1500.0, &h), -1500.0);
    assert_eq!(*h.calls.borrow(), ["policy", "policy"]);

    let h = host(true, 1, false);
    assert_eq!(get_date_time(&h), 11_045_000.0);
    assert_eq!(*h.calls.borrow(), ["policy", "time", "localtime(0)"]);
    h.calls.borrow_mut().clear();
    assert_eq!(local_time(-1500.0, &h), -1500.0 + 10_800_000.0);
    assert_eq!(
        *h.calls.borrow(),
        [
            "policy",
            "time",
            "localtime(0)",
            "timezone",
            "policy",
            "localtime(-1)"
        ]
    );

    // A failed localtime only removes DST; the standard zone still applies.
    assert_eq!(local_time(0.0, &host(true, 1, true)), 7_200_000.0);
    assert_eq!(local_time(0.0, &host(true, -1, false)), 7_200_000.0);
}

#[test]
fn month_tokens_horizon_rollover_and_unchanged_failure_output() {
    let h = host(true, 0, false);
    for (value, format, year, month, day) in [
        ("49", "yy", 2049, 0, 1),
        ("50", "yy", 1950, 0, 1),
        ("x", "y", 1970, 0, 1),
        ("04", "mmm", 1970, 3, 1),
        ("apr", "mmmm", 1970, 3, 1),
        ("bru", "mmmm", 1970, 1, 1),   // Contains, not prefix matching.
        ("Aprİl", "mmmm", 1970, 3, 1), // Host supplies ICU simple lowercase.
        ("2020/02/31", "yyyy/mm/dd", 2020, 2, 2),
        ("2020-04-23extra", "yyyy/mm/dd", 2020, 3, 23), // Separators skipped, suffix ignored.
    ] {
        let mut result = -123.0;
        assert_eq!(
            parse_date_using_format(&wide(value), &wide(format), &mut result, &h),
            ConversionStatus::Success,
            "{value} {format}"
        );
        assert_eq!(
            result,
            make_date(make_day(year, month, day), make_time(3, 4, 5, 0)),
            "{value} {format}"
        );
    }
    for (value, format, expected) in [
        ("2020", "yyy", ConversionStatus::BadFormat),
        ("bogus", "mmm", ConversionStatus::BadFormat),
        ("!", "mmmm", ConversionStatus::BadFormat),
        ("13", "mm", ConversionStatus::BadDate),
        ("32", "dd", ConversionStatus::BadDate),
        ("25", "HH", ConversionStatus::BadDate),
        ("61", "MM", ConversionStatus::BadDate),
        ("61", "ss", ConversionStatus::BadDate),
    ] {
        let mut result = -123.0;
        assert_eq!(
            parse_date_using_format(&wide(value), &wide(format), &mut result, &h),
            expected,
            "{value} {format}"
        );
        assert_eq!(result, -123.0);
    }
    let mut result = 0.0;
    assert_eq!(
        parse_date_using_format(&wide("pm 1:30"), &wide("tt h:MM"), &mut result, &h),
        ConversionStatus::Success
    );
    assert_eq!(
        result,
        make_date(make_day(1970, 0, 1), make_time(13, 30, 5, 0))
    );
    assert_eq!(
        parse_date_using_format(
            &wide("2020/04/23 24:60:60"),
            &wide("yyyy/mm/dd HH:MM:ss"),
            &mut result,
            &h
        ),
        ConversionStatus::Success
    );
    assert_eq!(
        result,
        make_date(make_day(2020, 3, 23), make_time(24, 60, 60, 0))
    );
}

#[test]
fn integer_prefix_limits_and_date_math_boundaries() {
    for (input, start, limit, expected, expected_skip) in [
        ("a12345x", 1, 2, 12, 2),
        ("123", 0, 0, 1, 1), // Native limit checked after consuming the first digit.
        ("123", 10, 4, 0, 0),
        ("-12", 0, 4, 0, 0),
        ("١2", 0, 4, 0, 0), // FXSYS_IsDecimalDigit is ASCII only.
    ] {
        let mut skipped = usize::MAX;
        assert_eq!(
            parse_string_integer(&wide(input), start, &mut skipped, limit),
            expected
        );
        assert_eq!(skipped, expected_skip);
    }
    assert_eq!(make_day(2000, -1, 1), make_day(1999, 11, 1));
    assert_eq!(make_day(2000, 12, 1), make_day(2001, 0, 1));
    assert_eq!(
        (
            get_hour_from_time(-1.0),
            get_min_from_time(-1.0),
            get_sec_from_time(-1.0)
        ),
        (23, 59, 59)
    );
    assert!(make_date(f64::NAN, 0.0).is_nan());
    assert!(make_date(0.0, f64::INFINITY).is_nan());
}
