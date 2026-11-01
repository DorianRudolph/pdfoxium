// Copyright 2014 The PDFium Authors
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Date/time picture selection, parsing and formatting from
//! `xfa/fgas/crt/cfgas_stringformatter.cpp`. The picture field bag and grammar
//! are separate from the caller's clock; locale timezone values come from the
//! injected host provider. Calendar arithmetic reuses existing date helpers.
use super::cfgas_stringformatter::{
    get_literal_text, Category, CfgasStringFormatter, DateTimeType,
};
use super::locale_iface::{DateTimeSubcategory, LocaleIface};
use super::locale_mgr_iface::LocaleMgrIface;
use super::picture_datetime::{
    date_from_canonical, parse_locale_date, parse_locale_time, time_from_canonical, PictureDateTime,
};
use crate::fxjs::fx_date_helpers as calendar;
use std::rc::Rc;

fn ascii_eq(value: &[u32], ascii: &str) -> bool {
    value.len() == ascii.len()
        && value
            .iter()
            .zip(ascii.bytes())
            .all(|(&ch, byte)| ch == u32::from(byte))
}
fn has_date(kind: DateTimeType) -> bool {
    matches!(
        kind,
        DateTimeType::Date | DateTimeType::DateTime | DateTimeType::TimeDate
    )
}
fn has_time(kind: DateTimeType) -> bool {
    matches!(
        kind,
        DateTimeType::Time | DateTimeType::DateTime | DateTimeType::TimeDate
    )
}
fn add_date(kind: DateTimeType) -> DateTimeType {
    match kind {
        DateTimeType::Unknown => DateTimeType::Date,
        DateTimeType::Time => DateTimeType::TimeDate,
        _ => kind,
    }
}
fn add_time(kind: DateTimeType) -> DateTimeType {
    match kind {
        DateTimeType::Unknown => DateTimeType::Time,
        DateTimeType::Date => DateTimeType::DateTime,
        _ => kind,
    }
}
fn solar_month_days(year: i32, month: u8) -> u16 {
    (calendar::make_day(year, i32::from(month), 1)
        - calendar::make_day(year, i32::from(month) - 1, 1)) as u16
}
fn week_day(year: i32, month: u8, day: u8) -> u16 {
    calendar::get_day_of_week_from_time(
        calendar::make_day(year, i32::from(month) - 1, i32::from(day)) * 86_400_000.0,
    ) as u16
}
fn week_of_month(year: i32, month: u8, day: u8) -> u16 {
    let weekday = week_day(year, month, 1);
    let day = u16::from(day);
    day / 7 + u16::from(weekday + day % 7 > 7)
}
fn week_of_year(year: i32, month: u8, day: u8) -> u16 {
    let days = (calendar::make_day(year, i32::from(month) - 1, i32::from(day))
        - calendar::make_day(year, 0, 1)
        + 1.0) as u16;
    1 + days / 7 + u16::from(week_day(year, 1, 1) + days % 7 > 7)
}
fn append_number(out: &mut Vec<u32>, width: usize, value: i32) {
    let width = match width {
        1 => 0,
        2 => 2,
        _ => 3,
    };
    out.extend(format!("{value:0width$}").chars().map(u32::from));
}
fn date_format(pattern: &[u32], locale: &dyn LocaleIface, dt: &PictureDateTime) -> Vec<u32> {
    let (year, month, day) = (dt.year, dt.month, dt.day);
    let mut out = Vec::new();
    let mut index = 0;
    while index < pattern.len() {
        if pattern[index] == u32::from(b'\'') {
            out.extend(get_literal_text(pattern, &mut index));
            continue;
        }
        if !b"DJMEeGgYwW"
            .iter()
            .any(|&ch| u32::from(ch) == pattern[index])
        {
            out.push(pattern[index]);
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < pattern.len() && pattern[index] == pattern[start] {
            index += 1;
        }
        let symbol = &pattern[start..index];
        if ascii_eq(symbol, "D") || ascii_eq(symbol, "DD") {
            append_number(&mut out, symbol.len(), i32::from(day));
        } else if ascii_eq(symbol, "J") || ascii_eq(symbol, "JJJ") {
            let days = (1..month)
                .map(|month| solar_month_days(year, month))
                .sum::<u16>()
                + u16::from(day);
            append_number(&mut out, symbol.len(), i32::from(days));
        } else if ascii_eq(symbol, "M") || ascii_eq(symbol, "MM") {
            append_number(&mut out, symbol.len(), i32::from(month));
        } else if ascii_eq(symbol, "MMM") || ascii_eq(symbol, "MMMM") {
            out.extend(locale.get_month_name(i32::from(month) - 1, ascii_eq(symbol, "MMM")));
        } else if ascii_eq(symbol, "e") || ascii_eq(symbol, "E") {
            let weekday = week_day(year, month, day);
            append_number(
                &mut out,
                1,
                i32::from(if ascii_eq(symbol, "E") {
                    weekday + 1
                } else if weekday == 0 {
                    7
                } else {
                    weekday
                }),
            );
        } else if ascii_eq(symbol, "EEE") || ascii_eq(symbol, "EEEE") {
            out.extend(locale.get_day_name(
                i32::from(week_day(year, month, day)),
                ascii_eq(symbol, "EEE"),
            ));
        } else if ascii_eq(symbol, "G") {
            out.extend(locale.get_era_name(year > 0));
        } else if ascii_eq(symbol, "YY") {
            append_number(&mut out, 2, year % 100);
        } else if ascii_eq(symbol, "YYYY") {
            append_number(&mut out, 1, year);
        } else if ascii_eq(symbol, "w") {
            append_number(&mut out, 1, i32::from(week_of_month(year, month, day)));
        } else if ascii_eq(symbol, "WW") {
            append_number(&mut out, 2, i32::from(week_of_year(year, month, day)));
        }
    }
    out
}
fn time_format(pattern: &[u32], locale: &dyn LocaleIface, dt: &PictureDateTime) -> Vec<u32> {
    let mut out = Vec::new();
    let mut hour = dt.hour;
    let pm = pattern.contains(&u32::from(b'A')) && hour >= 12;
    let mut index = 0;
    while index < pattern.len() {
        if pattern[index] == u32::from(b'\'') {
            out.extend(get_literal_text(pattern, &mut index));
            continue;
        }
        if !b"hHkKMSFAzZ"
            .iter()
            .any(|&ch| u32::from(ch) == pattern[index])
        {
            out.push(pattern[index]);
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < pattern.len() && pattern[index] == pattern[start] {
            index += 1;
        }
        let symbol = &pattern[start..index];
        if ascii_eq(symbol, "h") || ascii_eq(symbol, "hh") {
            if hour > 12 {
                hour -= 12;
            }
            append_number(
                &mut out,
                symbol.len(),
                i32::from(if hour == 0 { 12 } else { hour }),
            );
        } else if ascii_eq(symbol, "K") || ascii_eq(symbol, "KK") {
            append_number(
                &mut out,
                symbol.len(),
                i32::from(if hour == 0 { 24 } else { hour }),
            );
        } else if ascii_eq(symbol, "k") || ascii_eq(symbol, "kk") {
            if hour > 12 {
                hour -= 12;
            }
            append_number(&mut out, symbol.len(), i32::from(hour));
        } else if ascii_eq(symbol, "H") || ascii_eq(symbol, "HH") {
            append_number(&mut out, symbol.len(), i32::from(hour));
        } else if ascii_eq(symbol, "M") || ascii_eq(symbol, "MM") {
            append_number(&mut out, symbol.len(), i32::from(dt.minute));
        } else if ascii_eq(symbol, "S") || ascii_eq(symbol, "SS") {
            append_number(&mut out, symbol.len(), i32::from(dt.second));
        } else if ascii_eq(symbol, "FFF") {
            append_number(&mut out, 3, i32::from(dt.millisecond));
        } else if ascii_eq(symbol, "A") {
            out.extend(locale.get_meridiem_name(!pm));
        } else if ascii_eq(symbol, "z") || ascii_eq(symbol, "Z") {
            if ascii_eq(symbol, "Z") {
                out.extend("GMT".chars().map(u32::from));
            }
            let minutes = locale.get_time_zone_in_minutes();
            if minutes != 0 {
                out.push(u32::from(if minutes < 0 { b'-' } else { b'+' }));
                let magnitude = minutes.unsigned_abs();
                out.extend(
                    format!("{:02}:{:02}", magnitude / 60, magnitude % 60)
                        .chars()
                        .map(u32::from),
                );
            }
        }
    }
    out
}
fn format_internal(
    dt: &PictureDateTime,
    date_pattern: &[u32],
    time_pattern: &[u32],
    date_first: bool,
    locale: &dyn LocaleIface,
) -> Vec<u32> {
    let date = date_format(date_pattern, locale, dt);
    let time = time_format(time_pattern, locale, dt);
    let mut out = Vec::with_capacity(date.len() + time.len());
    if date_first {
        out.extend(date);
        out.extend(time);
    } else {
        out.extend(time);
        out.extend(date);
    }
    out
}
struct DateTimeFormat {
    kind: DateTimeType,
    locale: Option<Rc<dyn LocaleIface>>,
    date: Vec<u32>,
    time: Vec<u32>,
}
fn subcategory(name: &[u32]) -> DateTimeSubcategory {
    for (text, value) in [
        ("default", DateTimeSubcategory::Default),
        ("short", DateTimeSubcategory::Short),
        ("medium", DateTimeSubcategory::Medium),
        ("full", DateTimeSubcategory::Full),
        ("long", DateTimeSubcategory::Long),
    ] {
        if ascii_eq(name, text) {
            return value;
        }
    }
    DateTimeSubcategory::Medium
}
// A datetime clause has both pictures in one body. The source discards this
// body at its closing brace, causing a successful empty output. Keep the
// explicit separator, or split before its first unquoted hour symbol.
fn split_combined_picture(pattern: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let mut cursor = 0;
    while cursor < pattern.len() {
        if pattern[cursor] == u32::from(b'\'') {
            get_literal_text(pattern, &mut cursor);
            continue;
        }
        if pattern[cursor] == u32::from(b'T') {
            let mut time = pattern[cursor..].to_vec();
            time[0] = u32::from(b' ');
            return (pattern[..cursor].to_vec(), time);
        }
        if matches!(pattern[cursor], 0x68 | 0x48 | 0x6b | 0x4b) {
            return (pattern[..cursor].to_vec(), pattern[cursor..].to_vec());
        }
        cursor += 1;
    }
    (pattern.to_vec(), Vec::new())
}
impl CfgasStringFormatter {
    fn get_date_time_format(&self, mgr: &dyn LocaleMgrIface) -> DateTimeFormat {
        let mut result = DateTimeFormat {
            kind: DateTimeType::Unknown,
            locale: None,
            date: Vec::new(),
            time: Vec::new(),
        };
        let mut temp = Vec::new();
        let mut category = Category::Unknown;
        let mut index = 0;
        let mut brace_open = false;
        let pattern = &self.pattern;
        while index < pattern.len() {
            if pattern[index] == u32::from(b'\'') {
                let start = index;
                get_literal_text(pattern, &mut index);
                temp.extend_from_slice(&pattern[start..index]);
                continue;
            }
            if !brace_open
                && result.kind != DateTimeType::DateTime
                && !b",-:/. ".iter().any(|&ch| u32::from(ch) == pattern[index])
            {
                let start = index;
                index += 1;
                while index < pattern.len() && !matches!(pattern[index], 0x7b | 0x2e | 0x28) {
                    if pattern[index] == u32::from(b'T') {
                        result.date = pattern[..index].to_vec();
                        result.time = pattern[index..].to_vec();
                        result.time[0] = u32::from(b' ');
                        result.locale = result.locale.or_else(|| mgr.get_def_locale());
                        result.kind = DateTimeType::DateTime;
                        return result;
                    }
                    index += 1;
                }
                let name = &pattern[start..index];
                if !has_date(result.kind) && ascii_eq(name, "date") {
                    result.kind = add_date(result.kind);
                    category = Category::Date;
                } else if !has_time(result.kind) && ascii_eq(name, "time") {
                    result.kind = add_time(result.kind);
                    category = Category::Time;
                } else if ascii_eq(name, "datetime") {
                    result.kind = DateTimeType::DateTime;
                    category = Category::DateTime;
                } else {
                    continue;
                }
                while index < pattern.len() {
                    if pattern[index] == u32::from(b'{') {
                        brace_open = true;
                        break;
                    }
                    if pattern[index] == u32::from(b'(') {
                        index += 1;
                        let start = index;
                        while index < pattern.len() && pattern[index] != u32::from(b')') {
                            index += 1;
                        }
                        result.locale = mgr.get_locale_by_name(&pattern[start..index]);
                    } else if pattern[index] == u32::from(b'.') {
                        index += 1;
                        let start = index;
                        while index < pattern.len() && !matches!(pattern[index], 0x28 | 0x7b) {
                            index += 1;
                        }
                        let style = subcategory(&pattern[start..index]);
                        result.locale = result.locale.or_else(|| mgr.get_def_locale());
                        if let Some(locale) = result.locale.as_ref() {
                            match category {
                                Category::Date => {
                                    result.date = std::mem::take(&mut temp);
                                    result.date.extend(locale.get_date_pattern(style));
                                }
                                Category::Time => {
                                    result.time = std::mem::take(&mut temp);
                                    result.time.extend(locale.get_time_pattern(style));
                                }
                                Category::DateTime => {
                                    result.date = std::mem::take(&mut temp);
                                    result.date.extend(locale.get_date_pattern(style));
                                    result.time = locale.get_time_pattern(style);
                                }
                                _ => {}
                            }
                        }
                        temp.clear();
                        continue;
                    }
                    index += 1;
                }
            } else if pattern[index] == u32::from(b'}') {
                brace_open = false;
                if !temp.is_empty() {
                    if category == Category::Time {
                        result.time = std::mem::take(&mut temp);
                    } else if category == Category::Date {
                        result.date = std::mem::take(&mut temp);
                    } else if category == Category::DateTime {
                        (result.date, result.time) = split_combined_picture(&temp);
                        temp.clear();
                    } else {
                        temp.clear();
                    }
                }
            } else {
                temp.push(pattern[index]);
            }
            index += 1;
        }
        if !temp.is_empty() {
            if category == Category::Date {
                result.date.extend(temp);
            } else {
                result.time.extend(temp);
            }
        }
        result.locale = result.locale.or_else(|| mgr.get_def_locale());
        if result.kind == DateTimeType::Unknown {
            result.time.clear();
            result.date = pattern.clone();
        }
        result
    }
    pub fn parse_date_time(
        &self,
        mgr: &dyn LocaleMgrIface,
        src: &[u32],
        kind: DateTimeType,
        out: &mut PictureDateTime,
    ) -> bool {
        out.reset();
        if src.is_empty() || self.pattern.is_empty() {
            return false;
        }
        let mut format = self.get_date_time_format(mgr);
        let Some(locale) = format.locale.as_deref() else {
            return false;
        };
        if format.kind == DateTimeType::Unknown {
            format.kind = kind;
            if kind == DateTimeType::Time {
                format.time = std::mem::take(&mut format.date);
            }
        }
        let mut index = 0;
        // Source DateTime swaps its two patterns. Parse in the actual declared
        // order; helpers leave the shared cursor at their first remaining char.
        let parsed = match format.kind {
            DateTimeType::Date => parse_locale_date(src, &format.date, locale, out, &mut index),
            DateTimeType::Time => parse_locale_time(src, &format.time, locale, out, &mut index),
            DateTimeType::DateTime => {
                parse_locale_date(src, &format.date, locale, out, &mut index)
                    && parse_locale_time(src, &format.time, locale, out, &mut index)
            }
            DateTimeType::TimeDate => {
                parse_locale_time(src, &format.time, locale, out, &mut index)
                    && parse_locale_date(src, &format.date, locale, out, &mut index)
            }
            DateTimeType::Unknown => false,
        };
        parsed && index == src.len()
    }
    pub fn format_date_time(
        &self,
        mgr: &dyn LocaleMgrIface,
        src: &[u32],
        kind: DateTimeType,
        out: &mut Vec<u32>,
    ) -> bool {
        if src.is_empty() || self.pattern.is_empty() {
            return false;
        }
        let mut format = self.get_date_time_format(mgr);
        let Some(locale) = format.locale.as_deref() else {
            return false;
        };
        if format.kind == DateTimeType::Unknown {
            if kind == DateTimeType::Time {
                format.time = std::mem::take(&mut format.date);
            }
            format.kind = kind;
            if kind == DateTimeType::Unknown {
                return false;
            }
        }
        let mut dt = PictureDateTime::default();
        if let Some(index) = src.iter().position(|&ch| ch == u32::from(b'T')) {
            let (date, time) = (&src[..index], &src[index + 1..]);
            if date.is_empty() || time.is_empty() {
                return false;
            }
            if date_from_canonical(date, &mut dt) && time_from_canonical(locale, time, &mut dt) {
                *out = format_internal(
                    &dt,
                    &format.date,
                    &format.time,
                    format.kind != DateTimeType::TimeDate,
                    locale,
                );
                return true;
            }
        } else if (format.kind == DateTimeType::Date && date_from_canonical(src, &mut dt))
            || (format.kind == DateTimeType::Time && time_from_canonical(locale, src, &mut dt))
        {
            *out = format_internal(&dt, &format.date, &format.time, true, locale);
            return true;
        }
        false
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use crate::xfa::fxfa::parser::cxfa_localemgr::CxfaLocaleMgr;
    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    fn manager(locale: &str, offset: i32) -> CxfaLocaleMgr {
        CxfaLocaleMgr::new(&wide(locale), Rc::new(move || offset))
    }
    // Complete enabled vectors from CFGASStringFormatterTest.DateFormat.
    #[test]
    fn DateFormat() {
        for (locale, input, pattern, expected) in [
            ("en", "2002-10-25", "MMMM DD, YYYY", "October 25, 2002"),
            (
                "en",
                "20040722",
                "'Week of the month is' w",
                "Week of the month is 3",
            ),
            (
                "en",
                "20040722",
                "e 'days after Sunday'",
                "4 days after Sunday",
            ),
            ("en", "20040722", "YYYY-'W'WW-e", "2004-W30-4"),
            (
                "en",
                "20040722",
                "E 'days after Saturday'",
                "5 days after Saturday",
            ),
            (
                "en",
                "2000-01-01",
                "EEE, 'the' D 'of' MMMM, YYYY",
                "Sat, the 1 of January, 2000",
            ),
            (
                "en",
                "2000-01-01",
                "EEEE, 'the' D 'of' MMMM, YYYY",
                "Saturday, the 1 of January, 2000",
            ),
            ("en", "19991202", "MM/D/YY", "12/2/99"),
            ("en", "19990110", "MMM D, YYYY", "Jan 10, 1999"),
            ("en", "19990202", "J", "33"),
            ("en", "19990202", "JJJ", "033"),
            ("en", "19991231", "J", "365"),
            ("en", "20001231", "J", "366"),
            ("en", "19990501", "J", "121"),
            ("en", "19990901", "J", "244"),
            ("en", "19990228", "J", "59"),
            ("en", "20000229", "J", "60"),
            ("en", "21000501", "J", "121"),
            ("en", "19990102", "M", "1"),
            ("en", "19990102", "MMM", "Jan"),
            ("en", "19990102", "YYYY G", "1999 AD"),
            ("de_CH", "20041030", "D. MMMM YYYY", "30. Oktober 2004"),
            ("fr_CA", "20041030", "D MMMM YYYY", "30 octobre 2004"),
            (
                "en",
                "2002-10-25",
                "date(fr){DD MMMM, YYYY}",
                "25 octobre, 2002",
            ),
            (
                "en",
                "2002-10-25",
                "date(es){EEEE, D 'de' MMMM 'de' YYYY}",
                "viernes, 25 de octubre de 2002",
            ),
        ] {
            let formatter = CfgasStringFormatter::new(&wide(pattern));
            let mut result = Vec::new();
            assert!(
                formatter.format_date_time(
                    &manager(locale, 0),
                    &wide(input),
                    DateTimeType::Date,
                    &mut result
                ),
                "{locale}: {input} / {pattern}"
            );
            assert_eq!(result, wide(expected), "{locale}: {input} / {pattern}");
        }
    }
    // Complete enabled vectors from CFGASStringFormatterTest.TimeFormat.
    #[test]
    fn TimeFormat() {
        for (locale, input, pattern, expected) in [
            ("en", "01:01:11", "h:M A", "1:1 AM"),
            ("en", "13:01:11", "h:M A", "1:1 PM"),
            ("en", "01:01:11", "hh:MM:SS A", "01:01:11 AM"),
            ("en", "13:01:11", "hh:MM:SS A", "01:01:11 PM"),
            ("en", "01:01:11", "hh:MM:SS A Z", "01:01:11 AM GMT-02:00"),
            ("en", "01:01:11", "hh:MM:SS A z", "01:01:11 AM -02:00"),
            ("en", "12:01:01", "k:MM:SS", "12:01:01"),
            ("en", "14:01:01", "k:MM:SS", "2:01:01"),
            ("en", "12:01:11", "kk:MM", "12:01"),
            ("en", "14:01:11", "kk:MM", "02:01"),
            ("en", "12:01:11 +04:30", "kk:MM", "05:31"),
            ("en", "12:01:11", "kk:MM A", "12:01 PM"),
            ("en", "00:01:01", "H:M:S", "0:1:1"),
            ("en", "13:02:11", "H:M:S", "13:2:11"),
            ("en", "00:01:11.001", "HH:M:S.FFF", "00:1:11.001"),
            ("en", "13:02:11", "HH:M", "13:2"),
            ("en", "00:01:11", "K:M", "24:1"),
            ("en", "00:02:11", "KK:M", "24:2"),
            (
                "en",
                "11:11:11",
                "HH:MM:SS 'o''clock' A Z",
                "11:11:11 o'clock AM GMT-02:00",
            ),
            ("en", "14:30:59", "h:MM A", "2:30 PM"),
            ("en", "14:30:59", "HH:MM:SS A Z", "14:30:59 PM GMT-02:00"),
        ] {
            let formatter = CfgasStringFormatter::new(&wide(pattern));
            let mut result = Vec::new();
            assert!(
                formatter.format_date_time(
                    &manager(locale, -120),
                    &wide(input),
                    DateTimeType::Time,
                    &mut result
                ),
                "{locale}: {input} / {pattern}"
            );
            assert_eq!(result, wide(expected), "{locale}: {input} / {pattern}");
        }
    }
    // Complete enabled vectors from CFGASStringFormatterTest.DateTimeFormat.
    #[test]
    fn DateTimeFormat() {
        for (locale, input, pattern, expected) in [
            (
                "en",
                "1999-07-16T10:30Z",
                "'At' time{HH:MM Z} 'on' date{MMM DD, YYYY}",
                "At 10:30 GMT on Jul 16, 1999",
            ),
            (
                "en",
                "1999-07-16T10:30",
                "'At' time{HH:MM} 'on' date{MMM DD, YYYY}",
                "At 10:30 on Jul 16, 1999",
            ),
            (
                "en",
                "1999-07-16T10:30Z",
                "time{'At' HH:MM Z} date{'on' MMM DD, YYYY}",
                "At 10:30 GMT on Jul 16, 1999",
            ),
            (
                "en",
                "1999-07-16T10:30Z",
                "time{'At 'HH:MM Z}date{' on 'MMM DD, YYYY}",
                "At 10:30 GMT on Jul 16, 1999",
            ),
            (
                "en",
                "9111T1111:",
                "MMM D, YYYYTh:MM:SS A",
                "Jan 1, 9111 11:11:00 AM",
            ),
        ] {
            let formatter = CfgasStringFormatter::new(&wide(pattern));
            let mut result = Vec::new();
            if input == "9111T1111:" {
                // Retain the source input but reject its dangling separator.
                assert!(!formatter.format_date_time(
                    &manager(locale, 0),
                    &wide(input),
                    DateTimeType::DateTime,
                    &mut result
                ));
                continue;
            }
            assert!(
                formatter.format_date_time(
                    &manager(locale, 0),
                    &wide(input),
                    DateTimeType::DateTime,
                    &mut result
                ),
                "{locale}: {input} / {pattern}"
            );
            assert_eq!(result, wide(expected), "{locale}: {input} / {pattern}");
        }
    }
    // Complete enabled vectors from CFGASStringFormatterTest.TimeDateFormat.
    #[test]
    fn TimeDateFormat() {
        for (locale, input, pattern, expected) in [
            (
                "en",
                "1999-07-16T10:30Z",
                "'At' time{HH:MM Z} 'on' date{MMM DD, YYYY}",
                "At 10:30 GMT on Jul 16, 1999",
            ),
            (
                "en",
                "1999-07-16T10:30",
                "'At' time{HH:MM} 'on' date{MMM DD, YYYY}",
                "At 10:30 on Jul 16, 1999",
            ),
            (
                "en",
                "1999-07-16T10:30Z",
                "time{'At' HH:MM Z} date{'on' MMM DD, YYYY}",
                "At 10:30 GMT on Jul 16, 1999",
            ),
            (
                "en",
                "1999-07-16T10:30Z",
                "time{'At 'HH:MM Z}date{' on 'MMM DD, YYYY}",
                "At 10:30 GMT on Jul 16, 1999",
            ),
        ] {
            let formatter = CfgasStringFormatter::new(&wide(pattern));
            let mut result = Vec::new();
            assert!(
                formatter.format_date_time(
                    &manager(locale, 0),
                    &wide(input),
                    DateTimeType::TimeDate,
                    &mut result
                ),
                "{locale}: {input} / {pattern}"
            );
            assert_eq!(result, wide(expected), "{locale}: {input} / {pattern}");
        }
    }
    // All source DateParse vectors, including the selected unsupported
    // ordinal/week fields and positive-era field representation. Full-width
    // date/time flags and Japanese-era cases remain disabled as upstream.
    #[test]
    fn DateParse() {
        for (locale, input, pattern, expected) in [
            (
                "en",
                "12/2/99",
                "MM/D/YY",
                PictureDateTime {
                    year: 1999,
                    month: 12,
                    day: 2,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "2/2/99",
                "M/D/YY",
                PictureDateTime {
                    year: 1999,
                    month: 2,
                    day: 2,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "2/2/10",
                "M/D/YY",
                PictureDateTime {
                    year: 2010,
                    month: 2,
                    day: 2,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "Jan 10, 1999",
                "MMM D, YYYY",
                PictureDateTime {
                    year: 1999,
                    month: 1,
                    day: 10,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "Jan 10, 1999 AD",
                "MMM D, YYYY G",
                PictureDateTime {
                    year: 1999,
                    month: 1,
                    day: 10,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "Jan 10, 0002 BC",
                "MMM D, YYYY G",
                PictureDateTime {
                    year: 2,
                    month: 1,
                    day: 10,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "October 25, 2002",
                "MMMM DD, YYYY",
                PictureDateTime {
                    year: 2002,
                    month: 10,
                    day: 25,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "1999-33",
                "YYYY-J",
                PictureDateTime {
                    year: 1999,
                    month: 1,
                    day: 1,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "1999-033",
                "YYYY-JJJ",
                PictureDateTime {
                    year: 1999,
                    month: 1,
                    day: 1,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "de_CH",
                "30. Oktober 2004",
                "D. MMMM YYYY",
                PictureDateTime {
                    year: 2004,
                    month: 10,
                    day: 30,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "fr_CA",
                "30 octobre 2004",
                "D MMMM YYYY",
                PictureDateTime {
                    year: 2004,
                    month: 10,
                    day: 30,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "Saturday, the 1 of January, 2000",
                "EEEE, 'the' D 'of' MMMM, YYYY",
                PictureDateTime {
                    year: 2000,
                    month: 1,
                    day: 1,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "Sat, the 1 of January, 2000",
                "EEE, 'the' D 'of' MMMM, YYYY",
                PictureDateTime {
                    year: 2000,
                    month: 1,
                    day: 1,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "7, the 1 of January, 2000",
                "E, 'the' D 'of' MMMM, YYYY",
                PictureDateTime {
                    year: 2000,
                    month: 1,
                    day: 1,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "6, the 1 of January, 2000",
                "e, 'the' D 'of' MMMM, YYYY",
                PictureDateTime {
                    year: 2000,
                    month: 1,
                    day: 1,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "2004-07-22 Week of the month is 3",
                "YYYY-MM-DD 'Week of the month is' w",
                PictureDateTime {
                    year: 2004,
                    month: 7,
                    day: 22,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
            (
                "en",
                "2004-07-22 Week of the year is 03",
                "YYYY-MM-DD 'Week of the year is' WW",
                PictureDateTime {
                    year: 2004,
                    month: 7,
                    day: 22,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 0,
                },
            ),
        ] {
            let formatter = CfgasStringFormatter::new(&wide(pattern));
            let mut result = PictureDateTime::default();
            assert!(
                formatter.parse_date_time(
                    &manager(locale, 0),
                    &wide(input),
                    DateTimeType::Date,
                    &mut result
                ),
                "{locale}: {input} / {pattern}"
            );
            assert_eq!(result, expected, "{locale}: {input} / {pattern}");
        }
    }
    #[test]
    fn standalone_time_and_both_combined_orders_parse_with_full_consumption() {
        let manager = manager("en", 0);
        for (input, pattern, kind, expected) in [
            (
                "18:00",
                "HH:MM",
                DateTimeType::Time,
                PictureDateTime {
                    hour: 18,
                    ..PictureDateTime::default()
                },
            ),
            (
                "12.59 Uhr",
                "H.MM 'Uhr'",
                DateTimeType::Time,
                PictureDateTime {
                    hour: 12,
                    minute: 59,
                    ..PictureDateTime::default()
                },
            ),
            (
                "2024-02-29 18:30",
                "date{YYYY-MM-DD} time{HH:MM}",
                DateTimeType::DateTime,
                PictureDateTime {
                    year: 2024,
                    month: 2,
                    day: 29,
                    hour: 18,
                    minute: 30,
                    ..PictureDateTime::default()
                },
            ),
            (
                "18:30 2024-02-29",
                "time{HH:MM} date{YYYY-MM-DD}",
                DateTimeType::TimeDate,
                PictureDateTime {
                    year: 2024,
                    month: 2,
                    day: 29,
                    hour: 18,
                    minute: 30,
                    ..PictureDateTime::default()
                },
            ),
        ] {
            let formatter = CfgasStringFormatter::new(&wide(pattern));
            let mut result = PictureDateTime::default();
            assert!(
                formatter.parse_date_time(&manager, &wide(input), kind, &mut result),
                "{input} / {pattern}"
            );
            assert_eq!(result, expected, "{input} / {pattern}");
            assert!(!formatter.parse_date_time(
                &manager,
                &wide(&format!("{input}X")),
                kind,
                &mut result
            ));
        }
    }
    #[test]
    fn explicit_datetime_bodies_retain_their_pictures_and_quoted_separator() {
        let manager = manager("en", 0);
        for (pattern, expected) in [
            ("datetime{YYYY-MM-DD HH:MM}", "2024-02-29 18:30"),
            ("datetime{YYYY-MM-DD'T'HH:MM}", "2024-02-29T18:30"),
        ] {
            let formatter = CfgasStringFormatter::new(&wide(pattern));
            let mut output = Vec::new();
            assert!(formatter.format_date_time(
                &manager,
                &wide("2024-02-29T18:30"),
                DateTimeType::DateTime,
                &mut output
            ));
            assert_eq!(output, wide(expected));
            let mut parsed = PictureDateTime::default();
            assert!(formatter.parse_date_time(
                &manager,
                &output,
                DateTimeType::DateTime,
                &mut parsed
            ));
            assert_eq!(parsed, PictureDateTime::new(2024, 2, 29, 18, 30, 0, 0));
        }
    }
}
