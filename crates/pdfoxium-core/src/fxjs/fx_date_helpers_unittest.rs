// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Complete cases from `fxjs/fx_date_helpers_unittest.cpp`.
#![allow(non_snake_case)]

use super::*;

const MILLISECONDS_IN_A_DAY: f64 = 1000.0 * 60.0 * 60.0 * 24.0;

// FakeTimeTest uses epoch 1587654321 and gmtime in place of localtime.
struct FakeTime;
impl WideCharacterHost for FakeTime {
    fn is_wide_alphanumeric(&self, c: u32) -> bool {
        u8::try_from(c).is_ok_and(|c| c.is_ascii_alphanumeric())
    }
    fn lowercase_wide(&self, c: u32) -> u32 {
        if (u32::from(b'A')..=u32::from(b'Z')).contains(&c) {
            c + 32
        } else {
            c
        }
    }
}
impl DateTimeHost for FakeTime {
    fn machine_time_access_enabled(&self) -> bool {
        true
    }
    fn time(&self) -> i64 {
        1_587_654_321
    }
    fn localtime(&self, t: i64) -> Option<LocalTimeFields> {
        assert_eq!(t, 1_587_654_321);
        Some(LocalTimeFields {
            year: 120,
            year_day: 113,
            hour: 15,
            minute: 5,
            second: 21,
            is_dst: 0,
        })
    }
    fn timezone_seconds_west(&self) -> i64 {
        0
    }
}

// GTest EXPECT_DOUBLE_EQ accepts at most four ULPs, including signed-zero equality.
fn expect_double_eq(expected: f64, actual: f64) {
    fn biased(value: f64) -> u64 {
        let bits = value.to_bits();
        let sign = 1u64 << 63;
        if bits & sign != 0 {
            (!bits).wrapping_add(1)
        } else {
            sign | bits
        }
    }
    assert!(
        !expected.is_nan() && !actual.is_nan() && biased(expected).abs_diff(biased(actual)) <= 4,
        "expected {expected:?}, actual {actual:?}"
    );
}

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn parse(value: &str, format: &str, result: &mut f64) -> ConversionStatus {
    parse_date_using_format(&wide(value), &wide(format), result, &FakeTime)
}

#[test]
fn GetYearFromTime() {
    let tests = [
        (-400.0 * MILLISECONDS_IN_A_DAY, 1968),
        (-1.0, 1969),
        (0.0, 1970),
        (1.0, 1970),
        (364.9 * MILLISECONDS_IN_A_DAY, 1970),
        (365.0 * MILLISECONDS_IN_A_DAY, 1971),
        (365.1 * MILLISECONDS_IN_A_DAY, 1971),
        (2.0 * 365.0 * MILLISECONDS_IN_A_DAY, 1972),
        // 1972 is a leap year, so there should be an extra day.
        (3.0 * 365.0 * MILLISECONDS_IN_A_DAY, 1972),
        ((3.0 * 365.0 + 1.0) * MILLISECONDS_IN_A_DAY, 1973),
        // 1900 is not a leap year.
        // There are 17 leap days between 1900 and 1970.
        (-(70.0 * 365.0 + 17.0) * MILLISECONDS_IN_A_DAY, 1900),
        (-(69.0 * 365.0 + 17.0 + 0.1) * MILLISECONDS_IN_A_DAY, 1900),
        (-(69.0 * 365.0 + 17.0) * MILLISECONDS_IN_A_DAY, 1901),
        // 2000 is a leap year.
        // There are 7 leap days between 1970 and 2000.
        ((30.0 * 365.0 + 7.0) * MILLISECONDS_IN_A_DAY, 2000),
        ((31.0 * 365.0 + 7.0) * MILLISECONDS_IN_A_DAY, 2000),
        ((31.0 * 365.0 + 8.0) * MILLISECONDS_IN_A_DAY, 2001),
    ];
    for (time_ms, expected) in tests {
        assert_eq!(expected, get_year_from_time(time_ms), "{time_ms}");
    }
}

#[test]
fn GetMonthFromTime() {
    let tests = [
        (-400.0 * MILLISECONDS_IN_A_DAY, 10),
        (-1.0, 11),
        (0.0, 0),
        (1.0, 0),
        (364.9 * MILLISECONDS_IN_A_DAY, 11),
        (365.0 * MILLISECONDS_IN_A_DAY, 0),
        (365.1 * MILLISECONDS_IN_A_DAY, 0),
        // 1972 is a leap year, so there should be an extra day.
        (2.0 * 365.0 * MILLISECONDS_IN_A_DAY, 0),
        (3.0 * 365.0 * MILLISECONDS_IN_A_DAY, 11),
        ((3.0 * 365.0 + 1.0) * MILLISECONDS_IN_A_DAY, 0),
        // Tests boundaries for all months in 1970 not already covered above.
        (30.0 * MILLISECONDS_IN_A_DAY, 0),
        (31.0 * MILLISECONDS_IN_A_DAY, 1),
        (58.0 * MILLISECONDS_IN_A_DAY, 1),
        (59.0 * MILLISECONDS_IN_A_DAY, 2),
        (89.0 * MILLISECONDS_IN_A_DAY, 2),
        (90.0 * MILLISECONDS_IN_A_DAY, 3),
        (119.0 * MILLISECONDS_IN_A_DAY, 3),
        (120.0 * MILLISECONDS_IN_A_DAY, 4),
        (150.0 * MILLISECONDS_IN_A_DAY, 4),
        (151.0 * MILLISECONDS_IN_A_DAY, 5),
        (180.0 * MILLISECONDS_IN_A_DAY, 5),
        (181.0 * MILLISECONDS_IN_A_DAY, 6),
        (211.0 * MILLISECONDS_IN_A_DAY, 6),
        (212.0 * MILLISECONDS_IN_A_DAY, 7),
        (242.0 * MILLISECONDS_IN_A_DAY, 7),
        (243.0 * MILLISECONDS_IN_A_DAY, 8),
        (272.0 * MILLISECONDS_IN_A_DAY, 8),
        (273.0 * MILLISECONDS_IN_A_DAY, 9),
        (303.0 * MILLISECONDS_IN_A_DAY, 9),
        (304.0 * MILLISECONDS_IN_A_DAY, 10),
        (333.0 * MILLISECONDS_IN_A_DAY, 10),
        (334.0 * MILLISECONDS_IN_A_DAY, 11),
        (364.0 * MILLISECONDS_IN_A_DAY, 11),
        // Tests boundaries for all months in 1972 not already covered above.
        ((2.0 * 365.0 + 30.0) * MILLISECONDS_IN_A_DAY, 0),
        ((2.0 * 365.0 + 31.0) * MILLISECONDS_IN_A_DAY, 1),
        ((2.0 * 365.0 + 59.0) * MILLISECONDS_IN_A_DAY, 1),
        ((2.0 * 365.0 + 60.0) * MILLISECONDS_IN_A_DAY, 2),
        ((2.0 * 365.0 + 90.0) * MILLISECONDS_IN_A_DAY, 2),
        ((2.0 * 365.0 + 91.0) * MILLISECONDS_IN_A_DAY, 3),
        ((2.0 * 365.0 + 120.0) * MILLISECONDS_IN_A_DAY, 3),
        ((2.0 * 365.0 + 121.0) * MILLISECONDS_IN_A_DAY, 4),
        ((2.0 * 365.0 + 151.0) * MILLISECONDS_IN_A_DAY, 4),
        ((2.0 * 365.0 + 152.0) * MILLISECONDS_IN_A_DAY, 5),
        ((2.0 * 365.0 + 181.0) * MILLISECONDS_IN_A_DAY, 5),
        ((2.0 * 365.0 + 182.0) * MILLISECONDS_IN_A_DAY, 6),
        ((2.0 * 365.0 + 212.0) * MILLISECONDS_IN_A_DAY, 6),
        ((2.0 * 365.0 + 213.0) * MILLISECONDS_IN_A_DAY, 7),
        ((2.0 * 365.0 + 243.0) * MILLISECONDS_IN_A_DAY, 7),
        ((2.0 * 365.0 + 244.0) * MILLISECONDS_IN_A_DAY, 8),
        ((2.0 * 365.0 + 273.0) * MILLISECONDS_IN_A_DAY, 8),
        ((2.0 * 365.0 + 274.0) * MILLISECONDS_IN_A_DAY, 9),
        ((2.0 * 365.0 + 304.0) * MILLISECONDS_IN_A_DAY, 9),
        ((2.0 * 365.0 + 305.0) * MILLISECONDS_IN_A_DAY, 10),
        ((2.0 * 365.0 + 334.0) * MILLISECONDS_IN_A_DAY, 10),
        ((2.0 * 365.0 + 335.0) * MILLISECONDS_IN_A_DAY, 11),
        // 1900 is not a leap year.
        // There are 17 leap days between 1900 and 1970.
        ((-(70.0 * 365.0 + 17.0) + 58.0) * MILLISECONDS_IN_A_DAY, 1),
        ((-(70.0 * 365.0 + 17.0) + 59.0) * MILLISECONDS_IN_A_DAY, 2),
        // 2000 is a leap year.
        // There are 7 leap days between 1970 and 2000.
        ((30.0 * 365.0 + 7.0 + 58.0) * MILLISECONDS_IN_A_DAY, 1),
        ((30.0 * 365.0 + 7.0 + 59.0) * MILLISECONDS_IN_A_DAY, 1),
        ((30.0 * 365.0 + 7.0 + 60.0) * MILLISECONDS_IN_A_DAY, 2),
    ];
    for (time_ms, expected) in tests {
        assert_eq!(expected, get_month_from_time(time_ms), "{time_ms}");
    }
}

#[test]
fn GetDayFromTime() {
    let tests = [
        // 1900 is not a leap year.
        // There are 17 leap days between 1900 and 1970.
        ((-(70.0 * 365.0 + 17.0) + 58.0) * MILLISECONDS_IN_A_DAY, 28),
        ((-(70.0 * 365.0 + 17.0) + 59.0) * MILLISECONDS_IN_A_DAY, 1),
        // 1970 is not a leap year.
        (0.0, 1),
        (30.0 * MILLISECONDS_IN_A_DAY, 31),
        (31.0 * MILLISECONDS_IN_A_DAY, 1),
        // 1972 is a leap year.
        ((2.0 * 365.0 + 58.0) * MILLISECONDS_IN_A_DAY, 28),
        ((2.0 * 365.0 + 59.0) * MILLISECONDS_IN_A_DAY, 29),
        ((2.0 * 365.0 + 60.0) * MILLISECONDS_IN_A_DAY, 1),
        // 2000 is a leap year.
        // There are 7 leap days between 1970 and 2000.
        ((30.0 * 365.0 + 7.0 + 58.0) * MILLISECONDS_IN_A_DAY, 28),
        ((30.0 * 365.0 + 7.0 + 59.0) * MILLISECONDS_IN_A_DAY, 29),
        ((30.0 * 365.0 + 7.0 + 60.0) * MILLISECONDS_IN_A_DAY, 1),
    ];
    for (time_ms, expected) in tests {
        assert_eq!(expected, get_day_from_time(time_ms), "{time_ms}");
    }
}

#[test]
fn GetDayOfWeekFromTime() {
    let tests = [
        (0.0, 4),                         // 1970-01-01 is Thursday.
        (1.0 * MILLISECONDS_IN_A_DAY, 5), // 1970-01-02 is Friday.
        (2.0 * MILLISECONDS_IN_A_DAY, 6), // 1970-01-03 is Saturday.
        (3.0 * MILLISECONDS_IN_A_DAY, 0), // 1970-01-04 is Sunday.
        (4.0 * MILLISECONDS_IN_A_DAY, 1), // 1970-01-05 is Monday.
        (5.0 * MILLISECONDS_IN_A_DAY, 2), // 1970-01-06 is Tuesday.
        (6.0 * MILLISECONDS_IN_A_DAY, 3), // 1970-01-07 is Wednesday.
        // Negative timestamps before 1970.
        (-1.0 * MILLISECONDS_IN_A_DAY, 3), // 1969-12-31 is Wednesday.
        (-2.0 * MILLISECONDS_IN_A_DAY, 2), // 1969-12-30 is Tuesday.
        (-3.0 * MILLISECONDS_IN_A_DAY, 1), // 1969-12-29 is Monday.
        (-4.0 * MILLISECONDS_IN_A_DAY, 0), // 1969-12-28 is Sunday.
        (-5.0 * MILLISECONDS_IN_A_DAY, 6), // 1969-12-27 is Saturday.
    ];
    for (time_ms, expected) in tests {
        assert_eq!(expected, get_day_of_week_from_time(time_ms), "{time_ms}");
    }
}

#[test]
fn ParseDateUsingFormatWithEmptyParams() {
    let mut result = 0.0;
    assert_eq!(ConversionStatus::Success, parse("", "", &mut result));
    expect_double_eq(1_587_654_321_000.0, result);
    assert_eq!(ConversionStatus::Success, parse("value", "", &mut result));
    expect_double_eq(1_587_654_321_000.0, result);
    assert_eq!(ConversionStatus::Success, parse("", "format", &mut result));
    expect_double_eq(1_587_654_321_000.0, result);
}

#[test]
fn ParseDateUsingFormatForValidMonthDay() {
    let mut result = 0.0;
    assert_eq!(
        ConversionStatus::Success,
        parse("01/02/2000", "mm/dd/yyyy", &mut result)
    );
    expect_double_eq(946_825_521_000.0, result);
    assert_eq!(
        ConversionStatus::Success,
        parse("1/2/2000", "m/d/yyyy", &mut result)
    );
    expect_double_eq(946_825_521_000.0, result);
    assert_eq!(
        ConversionStatus::Success,
        parse("1-2-2000", "m-d-yyyy", &mut result)
    );
    expect_double_eq(946_825_521_000.0, result);
    assert_eq!(
        ConversionStatus::Success,
        parse("2-1-2000", "d-m-yyyy", &mut result)
    );
    expect_double_eq(946_825_521_000.0, result);
    assert_eq!(
        ConversionStatus::Success,
        parse("11/12/2000", "mm/dd/yyyy", &mut result)
    );
    expect_double_eq(974_041_521_000.0, result);
    assert_eq!(
        ConversionStatus::Success,
        parse("11/12/2000", "m/d/yyyy", &mut result)
    );
    expect_double_eq(974_041_521_000.0, result);
    assert_eq!(
        ConversionStatus::Success,
        parse("11-12-2000", "m-d-yyyy", &mut result)
    );
    expect_double_eq(974_041_521_000.0, result);
    assert_eq!(
        ConversionStatus::Success,
        parse("12-11-2000", "d-m-yyyy", &mut result)
    );
    expect_double_eq(974_041_521_000.0, result);
    assert_eq!(
        ConversionStatus::Success,
        parse("02/29/2000", "mm/dd/yyyy", &mut result)
    );
    expect_double_eq(951_836_721_000.0, result);
}

#[test]
fn ParseDateUsingFormatFor12HourTime() {
    let mut result = 0.0;
    let base_day = make_day(2020, 3, 23);
    // 12:30:00 PM is 12:30.
    assert_eq!(
        ConversionStatus::Success,
        parse(
            "2020/04/23 12:30:00 pm",
            "yyyy/mm/dd hh:MM:ss tt",
            &mut result
        )
    );
    expect_double_eq(make_date(base_day, make_time(12, 30, 0, 0)), result);
    assert_eq!(
        ConversionStatus::Success,
        parse(
            "2020/04/23 12:30:00 PM",
            "yyyy/mm/dd hh:MM:ss tt",
            &mut result
        )
    );
    expect_double_eq(make_date(base_day, make_time(12, 30, 0, 0)), result);
    // 12:30:00 AM is 00:30.
    assert_eq!(
        ConversionStatus::Success,
        parse(
            "2020/04/23 12:30:00 am",
            "yyyy/mm/dd hh:MM:ss tt",
            &mut result
        )
    );
    expect_double_eq(make_date(base_day, make_time(0, 30, 0, 0)), result);
    assert_eq!(
        ConversionStatus::Success,
        parse(
            "2020/04/23 12:30:00 AM",
            "yyyy/mm/dd hh:MM:ss tt",
            &mut result
        )
    );
    expect_double_eq(make_date(base_day, make_time(0, 30, 0, 0)), result);
    // 1:30:00 PM is 13:30.
    assert_eq!(
        ConversionStatus::Success,
        parse(
            "2020/04/23 1:30:00 pm",
            "yyyy/mm/dd h:MM:ss tt",
            &mut result
        )
    );
    expect_double_eq(make_date(base_day, make_time(13, 30, 0, 0)), result);
    assert_eq!(
        ConversionStatus::Success,
        parse(
            "2020/04/23 1:30:00 PM",
            "yyyy/mm/dd h:MM:ss tt",
            &mut result
        )
    );
    expect_double_eq(make_date(base_day, make_time(13, 30, 0, 0)), result);
    // 1:30:00 AM is 01:30.
    assert_eq!(
        ConversionStatus::Success,
        parse(
            "2020/04/23 1:30:00 am",
            "yyyy/mm/dd h:MM:ss tt",
            &mut result
        )
    );
    expect_double_eq(make_date(base_day, make_time(1, 30, 0, 0)), result);
    assert_eq!(
        ConversionStatus::Success,
        parse(
            "2020/04/23 1:30:00 AM",
            "yyyy/mm/dd h:MM:ss tt",
            &mut result
        )
    );
    expect_double_eq(make_date(base_day, make_time(1, 30, 0, 0)), result);
    // Single 't' marker.
    // 12:30:00 P is 12:30.
    assert_eq!(
        ConversionStatus::Success,
        parse(
            "2020/04/23 12:30:00 p",
            "yyyy/mm/dd hh:MM:ss t",
            &mut result
        )
    );
    expect_double_eq(make_date(base_day, make_time(12, 30, 0, 0)), result);
    assert_eq!(
        ConversionStatus::Success,
        parse(
            "2020/04/23 12:30:00 P",
            "yyyy/mm/dd hh:MM:ss t",
            &mut result
        )
    );
    expect_double_eq(make_date(base_day, make_time(12, 30, 0, 0)), result);
    // 12:30:00 A is 00:30.
    assert_eq!(
        ConversionStatus::Success,
        parse(
            "2020/04/23 12:30:00 a",
            "yyyy/mm/dd hh:MM:ss t",
            &mut result
        )
    );
    expect_double_eq(make_date(base_day, make_time(0, 30, 0, 0)), result);
    assert_eq!(
        ConversionStatus::Success,
        parse(
            "2020/04/23 12:30:00 A",
            "yyyy/mm/dd hh:MM:ss t",
            &mut result
        )
    );
    expect_double_eq(make_date(base_day, make_time(0, 30, 0, 0)), result);
    // Malformed AM/PM markers should return kBadFormat.
    assert_eq!(
        ConversionStatus::BadFormat,
        parse(
            "2020/04/23 1:30:00 xm",
            "yyyy/mm/dd h:MM:ss tt",
            &mut result
        )
    );
    assert_eq!(
        ConversionStatus::BadFormat,
        parse(
            "2020/04/23 1:30:00 px",
            "yyyy/mm/dd h:MM:ss tt",
            &mut result
        )
    );
    assert_eq!(
        ConversionStatus::BadFormat,
        parse("2020/04/23 1:30:00 x", "yyyy/mm/dd h:MM:ss t", &mut result)
    );
}
