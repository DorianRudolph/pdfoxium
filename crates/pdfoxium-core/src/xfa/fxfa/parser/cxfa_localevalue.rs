// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fxfa/parser/cxfa_localevalue.{h,cpp}`. Locale values connect canonical
//! data, picture alternatives and explicit locale services. The manager is
//! optional, matching native numeric values that do not need one. No XFA node,
//! clock or timezone implementation is attached to the value object.
use crate::fxcrt::fx_string::string_to_double_wide;
use crate::xfa::fgas::crt::cfgas_stringformatter::{Category, CfgasStringFormatter, DateTimeType};
use crate::xfa::fgas::crt::locale_iface::LocaleIface;
use crate::xfa::fgas::crt::locale_mgr_iface::LocaleMgrIface;
use crate::xfa::fgas::crt::picture_datetime::{
    date_from_canonical, time_from_canonical, PictureDateTime,
};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum ValueType {
    #[default]
    Null = 0,
    Boolean,
    Integer,
    Decimal,
    Float,
    Text,
    Date,
    Time,
    DateTime,
}
/// The four `XFA_ValuePicture` values from `cxfa_node.h`, independent of the
/// still-pending node representation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum XfaValuePicture {
    #[default]
    Raw = 0,
    Display,
    Edit,
    DataBind,
}

fn value_category(category: Category, kind: ValueType) -> Category {
    if category != Category::Unknown {
        return category;
    }
    match kind {
        ValueType::Boolean | ValueType::Integer | ValueType::Decimal | ValueType::Float => {
            Category::Num
        }
        ValueType::Text => Category::Text,
        ValueType::Date => Category::Date,
        ValueType::Time => Category::Time,
        ValueType::DateTime => Category::DateTime,
        ValueType::Null => Category::Unknown,
    }
}
fn split_date_time(value: &[u32]) -> Option<(&[u32], &[u32])> {
    let index = value
        .iter()
        .position(|&c| c == 84)
        .or_else(|| value.iter().position(|&c| c == 32))?;
    Some((&value[..index], &value[index + 1..]))
}
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}

struct ScopedLocale<'a> {
    manager: &'a dyn LocaleMgrIface,
    previous: Option<Rc<dyn LocaleIface>>,
    changed: bool,
}
impl<'a> ScopedLocale<'a> {
    fn new(manager: &'a dyn LocaleMgrIface, locale: Option<Rc<dyn LocaleIface>>) -> Self {
        let changed = locale.is_some();
        let previous = if changed {
            manager.get_def_locale()
        } else {
            None
        };
        if changed {
            manager.set_def_locale(locale);
        }
        Self {
            manager,
            previous,
            changed,
        }
    }
}
impl Drop for ScopedLocale<'_> {
    fn drop(&mut self) {
        if self.changed {
            self.manager.set_def_locale(self.previous.take());
        }
    }
}

#[derive(Clone)]
pub struct CxfaLocaleValue {
    locale_mgr: Option<Rc<dyn LocaleMgrIface>>,
    value: Vec<u32>,
    kind: ValueType,
    valid: bool,
}
impl Default for CxfaLocaleValue {
    fn default() -> Self {
        Self {
            locale_mgr: None,
            value: Vec::new(),
            kind: ValueType::Null,
            valid: true,
        }
    }
}
impl CxfaLocaleValue {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_type(kind: ValueType, locale_mgr: Option<Rc<dyn LocaleMgrIface>>) -> Self {
        Self {
            locale_mgr,
            kind,
            valid: kind != ValueType::Null,
            value: Vec::new(),
        }
    }
    pub fn with_value(
        kind: ValueType,
        value: &[u32],
        locale_mgr: Option<Rc<dyn LocaleMgrIface>>,
    ) -> Self {
        Self {
            locale_mgr,
            value: value.to_vec(),
            kind,
            valid: Self::validate_canonical_value(value, kind),
        }
    }
    pub fn with_pattern(
        kind: ValueType,
        value: &[u32],
        pattern: &[u32],
        locale: Option<Rc<dyn LocaleIface>>,
        locale_mgr: Option<Rc<dyn LocaleMgrIface>>,
    ) -> Self {
        let mut result = Self::with_type(kind, locale_mgr);
        result.valid = result.parse_pattern_value(value, pattern, locale);
        result
    }
    pub fn is_valid(&self) -> bool {
        self.valid
    }
    pub fn get_value(&self) -> &[u32] {
        &self.value
    }
    pub fn get_type(&self) -> ValueType {
        self.kind
    }
    pub fn get_double_num(&self) -> f64 {
        if self.valid
            && matches!(
                self.kind,
                ValueType::Boolean | ValueType::Integer | ValueType::Decimal | ValueType::Float
            )
        {
            string_to_double_wide(&self.value)
        } else {
            0.0
        }
    }
    pub fn get_date(&self) -> PictureDateTime {
        let mut result = PictureDateTime::default();
        if self.valid && self.kind == ValueType::Date {
            let value =
                split_date_time(&self.value).map_or(self.value.as_slice(), |(date, _)| date);
            date_from_canonical(value, &mut result);
        }
        result
    }
    pub fn get_time(&self) -> PictureDateTime {
        let mut result = PictureDateTime::default();
        if !self.valid || self.kind != ValueType::Time {
            return result;
        }
        let Some(locale) = self
            .locale_mgr
            .as_ref()
            .and_then(|mgr| mgr.get_def_locale())
        else {
            return result;
        };
        let value = split_date_time(&self.value).map_or(self.value.as_slice(), |(_, time)| time);
        time_from_canonical(locale.as_ref(), value, &mut result);
        result
    }
    pub fn set_date(&mut self, value: &PictureDateTime) {
        self.kind = ValueType::Date;
        self.value = wide(&format!(
            "{:04}-{:02}-{:02}",
            value.year, value.month, value.day
        ));
    }
    fn set_time(&mut self, value: &PictureDateTime) {
        self.kind = ValueType::Time;
        self.value = wide(&format!(
            "{:02}:{:02}:{:02}",
            value.hour, value.minute, value.second
        ));
        if value.millisecond > 0 {
            // The source's "%:03d" is malformed; canonical fractions use a dot.
            self.value
                .extend(wide(&format!(".{:03}", value.millisecond)));
        }
    }
    fn set_date_time(&mut self, value: &PictureDateTime) {
        self.kind = ValueType::DateTime;
        self.value = wide(&format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            value.year, value.month, value.day, value.hour, value.minute, value.second
        ));
        if value.millisecond > 0 {
            self.value
                .extend(wide(&format!(".{:03}", value.millisecond)));
        }
    }

    pub fn validate_value(
        &self,
        value: &[u32],
        pattern: &[u32],
        locale: Option<Rc<dyn LocaleIface>>,
        match_format: Option<&mut Vec<u32>>,
    ) -> bool {
        let Some(manager) = self.locale_mgr.as_ref() else {
            return false;
        };
        let _locale = ScopedLocale::new(manager.as_ref(), locale);
        let mut output = Vec::new();
        let mut date = PictureDateTime::default();
        for alternative in CfgasStringFormatter::split_on_bars(pattern) {
            let formatter = CfgasStringFormatter::new(&alternative);
            let valid = match value_category(formatter.get_category(), self.kind) {
                Category::Null => formatter.parse_null(value) || value.is_empty(),
                Category::Zero => formatter.parse_zero(value) || value == [48],
                Category::Num => {
                    formatter.parse_num(manager.as_ref(), value, &mut Vec::new())
                        || formatter.format_num(manager.as_ref(), value, &mut output)
                }
                Category::Text => {
                    let parsed = formatter.parse_text(value, &mut output);
                    output.clear();
                    parsed || formatter.format_text(value, &mut output)
                }
                Category::Date => {
                    Self::validate_canonical_date(value, &mut date)
                        || formatter.parse_date_time(
                            manager.as_ref(),
                            value,
                            DateTimeType::Date,
                            &mut date,
                        )
                        || formatter.format_date_time(
                            manager.as_ref(),
                            value,
                            DateTimeType::Date,
                            &mut output,
                        )
                }
                Category::Time => {
                    formatter.parse_date_time(
                        manager.as_ref(),
                        value,
                        DateTimeType::Time,
                        &mut date,
                    ) || formatter.format_date_time(
                        manager.as_ref(),
                        value,
                        DateTimeType::Time,
                        &mut output,
                    )
                }
                Category::DateTime => {
                    formatter.parse_date_time(
                        manager.as_ref(),
                        value,
                        DateTimeType::DateTime,
                        &mut date,
                    ) || formatter.format_date_time(
                        manager.as_ref(),
                        value,
                        DateTimeType::DateTime,
                        &mut output,
                    )
                }
                Category::Unknown => false,
            };
            if valid {
                if let Some(match_format) = match_format {
                    *match_format = alternative;
                }
                return true;
            }
        }
        false
    }
    pub fn format_patterns(
        &self,
        result: &mut Vec<u32>,
        pattern: &[u32],
        locale: Option<Rc<dyn LocaleIface>>,
        picture: XfaValuePicture,
    ) -> bool {
        result.clear();
        for alternative in CfgasStringFormatter::split_on_bars(pattern) {
            if self.format_single_pattern(result, &alternative, locale.clone(), picture) {
                return true;
            }
        }
        false
    }
    fn format_single_pattern(
        &self,
        result: &mut Vec<u32>,
        pattern: &[u32],
        locale: Option<Rc<dyn LocaleIface>>,
        picture: XfaValuePicture,
    ) -> bool {
        let Some(manager) = self.locale_mgr.as_ref() else {
            return false;
        };
        let _locale = ScopedLocale::new(manager.as_ref(), locale);
        result.clear();
        let formatter = CfgasStringFormatter::new(pattern);
        let category = value_category(formatter.get_category(), self.kind);
        let valid = match category {
            Category::Null => self.value.is_empty() && formatter.format_null(result),
            Category::Zero => self.value == [48] && formatter.format_zero(result),
            Category::Num => formatter.format_num(manager.as_ref(), &self.value, result),
            Category::Text => formatter.format_text(&self.value, result),
            Category::Date => formatter.format_date_time(
                manager.as_ref(),
                &self.value,
                DateTimeType::Date,
                result,
            ),
            Category::Time => formatter.format_date_time(
                manager.as_ref(),
                &self.value,
                DateTimeType::Time,
                result,
            ),
            Category::DateTime => formatter.format_date_time(
                manager.as_ref(),
                &self.value,
                DateTimeType::DateTime,
                result,
            ),
            Category::Unknown => {
                result.extend_from_slice(&self.value);
                true
            }
        };
        if !valid && (category != Category::Num || picture != XfaValuePicture::Display) {
            result.clear();
            result.extend_from_slice(&self.value);
        }
        valid
    }

    fn validate_canonical_value(value: &[u32], kind: ValueType) -> bool {
        if value.is_empty() {
            return true;
        }
        let mut date = PictureDateTime::default();
        match kind {
            ValueType::Date => {
                Self::validate_canonical_date(value, &mut date)
                    || split_date_time(value).is_some_and(|(date_value, _)| {
                        Self::validate_canonical_date(date_value, &mut date)
                    })
            }
            ValueType::Time => {
                Self::validate_canonical_time(value)
                    || split_date_time(value)
                        .is_some_and(|(_, time)| Self::validate_canonical_time(time))
            }
            // The source accidentally falls through true after a failed test.
            ValueType::DateTime => split_date_time(value).is_some_and(|(date_value, time)| {
                Self::validate_canonical_date(date_value, &mut date)
                    && Self::validate_canonical_time(time)
            }),
            _ => true,
        }
    }
    fn validate_canonical_date(value: &[u32], result: &mut PictureDateTime) -> bool {
        // Share the canonical date grammar; the source's separate validator
        // stopped accepting dates after 2029 and left partial-date fields zero.
        date_from_canonical(value, result)
    }
    fn validate_canonical_time(value: &[u32]) -> bool {
        fn pair(value: &[u32], index: &mut usize) -> Option<u32> {
            let digits = value.get(*index..index.checked_add(2)?)?;
            if !digits.iter().all(|c| (48..=57).contains(c)) {
                return None;
            }
            *index += 2;
            Some((digits[0] - 48) * 10 + digits[1] - 48)
        }
        let mut index = 0;
        if pair(value, &mut index).is_none_or(|hour| hour >= 24) {
            return false;
        }
        if index == value.len() {
            return true;
        }
        let separated = value.get(index) == Some(&58);
        if separated {
            index += 1;
        }
        if pair(value, &mut index).is_none_or(|minute| minute >= 60) {
            return false;
        }
        if index == value.len() {
            return true;
        }
        if separated {
            if value.get(index) != Some(&58) {
                return false;
            }
            index += 1;
        }
        if pair(value, &mut index).is_none_or(|second| second >= 60) {
            return false;
        }
        if value.get(index) == Some(&46) {
            index += 1;
            let Some(fraction) = value.get(index..index.saturating_add(3)) else {
                return false;
            };
            if !fraction.iter().all(|c| (48..=57).contains(c)) {
                return false;
            }
            index += 3;
        }
        match value.get(index) {
            None => true,
            Some(90) => index + 1 == value.len(),
            Some(43 | 45) => {
                index += 1;
                if pair(value, &mut index).is_none_or(|hour| hour > 23) {
                    return false;
                }
                if value.get(index) == Some(&58) {
                    index += 1;
                }
                pair(value, &mut index).is_some_and(|minute| minute < 60) && index == value.len()
            }
            _ => false,
        }
    }
    fn parse_pattern_value(
        &mut self,
        value: &[u32],
        pattern: &[u32],
        locale: Option<Rc<dyn LocaleIface>>,
    ) -> bool {
        let Some(manager) = self.locale_mgr.clone() else {
            return false;
        };
        let _locale = ScopedLocale::new(manager.as_ref(), locale);
        for alternative in CfgasStringFormatter::split_on_bars(pattern) {
            let formatter = CfgasStringFormatter::new(&alternative);
            let mut date = PictureDateTime::default();
            let valid = match value_category(formatter.get_category(), self.kind) {
                Category::Null => {
                    let valid = formatter.parse_null(value);
                    if valid {
                        self.value.clear();
                    }
                    valid
                }
                Category::Zero => {
                    let valid = formatter.parse_zero(value);
                    if valid {
                        self.value = vec![48];
                    }
                    valid
                }
                Category::Num => {
                    let mut number = Vec::new();
                    let valid = formatter.parse_num(manager.as_ref(), value, &mut number);
                    if valid {
                        self.value = number;
                    }
                    valid
                }
                Category::Text => formatter.parse_text(value, &mut self.value),
                Category::Date => {
                    let valid = Self::validate_canonical_date(value, &mut date)
                        || formatter.parse_date_time(
                            manager.as_ref(),
                            value,
                            DateTimeType::Date,
                            &mut date,
                        );
                    if valid {
                        self.set_date(&date);
                    }
                    valid
                }
                Category::Time => {
                    let valid = formatter.parse_date_time(
                        manager.as_ref(),
                        value,
                        DateTimeType::Time,
                        &mut date,
                    );
                    if valid {
                        self.set_time(&date);
                    }
                    valid
                }
                Category::DateTime => {
                    let valid = formatter.parse_date_time(
                        manager.as_ref(),
                        value,
                        DateTimeType::DateTime,
                        &mut date,
                    );
                    if valid {
                        self.set_date_time(&date);
                    }
                    valid
                }
                Category::Unknown => {
                    self.value = value.to_vec();
                    true
                }
            };
            if valid {
                return true;
            }
        }
        self.value.clear();
        self.value.extend_from_slice(value);
        false
    }

    /// Build the source numeric-edit mask. Invalid negative counts and capacity
    /// arithmetic fail safely rather than entering the native decrement loop.
    pub fn get_numeric_format(
        &self,
        format: &mut Vec<u32>,
        integer_digits: i32,
        decimal_digits: i32,
    ) -> bool {
        if !format.is_empty() || integer_digits < -1 || decimal_digits < -1 {
            return false;
        }
        let integer_count = if integer_digits == -1 {
            2
        } else {
            integer_digits as usize
        };
        let decimal_count = if decimal_digits == -1 {
            2
        } else {
            decimal_digits as usize
        };
        let Some(total) = integer_count
            .checked_add(decimal_count)
            .and_then(|n| n.checked_add(1 + usize::from(decimal_digits != 0)))
        else {
            return false;
        };
        if total > i32::MAX as usize || format.try_reserve_exact(total).is_err() {
            return false;
        }
        format.push(115);
        if integer_digits == -1 {
            format.extend([122, 42]);
        } else {
            format.resize(format.len() + integer_count, 122);
        }
        if decimal_digits != 0 {
            format.push(46);
        }
        if decimal_digits == -1 {
            format.extend([122, 42]);
        } else {
            format.resize(format.len() + decimal_count, 122);
        }
        true
    }
    pub fn validate_numeric_temp(
        &self,
        value: &[u32],
        format: &[u32],
        locale: Option<&dyn LocaleIface>,
    ) -> bool {
        if format.is_empty() || value.is_empty() {
            return true;
        }
        let (mut index, mut pattern) = (0usize, 0usize);
        if format[0] == 115 {
            if matches!(value[0], 43 | 45) {
                index += 1;
            }
            pattern += 1;
        }
        let mut limited = true;
        while index < value.len()
            && (!limited || pattern < format.len())
            && (48..=57).contains(&value[index])
        {
            if limited {
                match format[pattern] {
                    42 => limited = false,
                    122 => pattern += 1,
                    _ => return false,
                }
            }
            index += 1;
        }
        if index == value.len() {
            return true;
        }
        if pattern == format.len() {
            return false;
        }
        while format.get(pattern).is_some_and(|&c| c != 46) {
            if !matches!(format[pattern], 122 | 42) {
                return false;
            }
            pattern += 1;
        }
        let decimal = locale
            .map(|locale| locale.get_decimal_symbol())
            .unwrap_or_else(|| vec![46]);
        if format.get(pattern) != Some(&46)
            || (value[index] != 46 && decimal.as_slice() != &value[index..index + 1])
        {
            return false;
        }
        pattern += 1;
        index += 1;
        limited = true;
        while index < value.len()
            && (!limited || pattern < format.len())
            && (48..=57).contains(&value[index])
        {
            if limited {
                match format[pattern] {
                    42 => limited = false,
                    122 => pattern += 1,
                    _ => return false,
                }
            }
            index += 1;
        }
        index == value.len()
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::super::cxfa_localemgr::CxfaLocaleMgr;
    use super::*;
    fn manager(locale: &str) -> Rc<dyn LocaleMgrIface> {
        Rc::new(CxfaLocaleMgr::new(&wide(locale), Rc::new(|| 0)))
    }
    #[test]
    fn GetDoubleNum() {
        // Complete CXFALocaleValueTest.GetDoubleNum, including its null manager.
        for (input, expected) in [
            ("", 0.0f32),
            ("0", 0.0),
            ("0.", 0.0),
            ("0.0", 0.0),
            ("0.x", 0.0),
            ("7.x", 7.0),
            (".54321", 0.54321),
            ("0.54321", 0.54321),
            ("+0.54321", 0.54321),
            ("   +0.54321", 0.54321),
            ("-.54321", -0.54321),
            ("-0.54321", -0.54321),
            ("  -0.54321", -0.54321),
            ("-0.54321e-1", -0.054321),
            ("-0.54321e0", -0.54321),
            ("-0.54321e1", -5.4321),
        ] {
            let actual = CxfaLocaleValue::with_value(ValueType::Float, &wide(input), None)
                .get_double_num() as f32;
            assert!(
                actual == expected || actual.to_bits().abs_diff(expected.to_bits()) <= 4,
                "{input}: {actual} != {expected}"
            );
        }
    }
    #[test]
    fn source_value_types_clone_and_null_manager_paths_remain_distinct() {
        assert!(CxfaLocaleValue::new().is_valid());
        assert!(!CxfaLocaleValue::with_type(ValueType::Null, None).is_valid());
        for kind in [
            ValueType::Boolean,
            ValueType::Integer,
            ValueType::Decimal,
            ValueType::Float,
        ] {
            let value = CxfaLocaleValue::with_value(kind, &wide("7.tail"), None);
            assert!(value.is_valid());
            assert_eq!(value.get_double_num(), 7.0);
            assert_eq!(value.clone().get_value(), wide("7.tail"));
        }
        let value = CxfaLocaleValue::with_value(ValueType::Text, &wide("7"), None);
        assert_eq!(value.get_double_num(), 0.0);
        assert!(!value.format_patterns(
            &mut Vec::new(),
            &wide("text{X}"),
            None,
            XfaValuePicture::Display
        ));
    }
    #[test]
    fn canonical_dates_times_and_milliseconds_reject_malformed_input() {
        for text in ["1900", "2024-02", "20240229", "2030-12-31", "9999-12-31"] {
            let value = CxfaLocaleValue::with_value(ValueType::Date, &wide(text), None);
            assert!(value.is_valid(), "{text}");
            assert_ne!(value.get_date().year, 0);
        }
        for text in [
            "1899-12-31",
            "2023-02-29",
            "2024-02-00",
            "2024-0229",
            "2024-02-29x",
        ] {
            assert!(
                !CxfaLocaleValue::with_value(ValueType::Date, &wide(text), None).is_valid(),
                "{text}"
            );
        }
        for text in ["2024-02-29T12:34:56.123", "2030-01-01 23:59:59Z"] {
            assert!(
                CxfaLocaleValue::with_value(ValueType::DateTime, &wide(text), None).is_valid(),
                "{text}"
            );
        }
        for text in [
            "invalid",
            "2024-02-29T24:00:00",
            "2024-02-29T12:34:",
            "2024-02-29T12:34:56.",
            "2024-02-29T12:34:56+",
            "2024-02-29T12:34:56Zx",
        ] {
            assert!(
                !CxfaLocaleValue::with_value(ValueType::DateTime, &wide(text), None).is_valid(),
                "{text}"
            );
        }
        let mgr = manager("en_US");
        let time = CxfaLocaleValue::with_value(
            ValueType::Time,
            &wide("2024-02-29T12:34:56.123"),
            Some(mgr),
        );
        assert!(time.is_valid());
        assert_eq!(time.get_time().millisecond, 123);
        let mut value = CxfaLocaleValue::new();
        let fields = PictureDateTime {
            year: 2030,
            month: 1,
            day: 2,
            hour: 3,
            minute: 4,
            second: 5,
            millisecond: 6,
        };
        value.set_time(&fields);
        assert_eq!(value.get_value(), wide("03:04:05.006"));
        value.set_date_time(&fields);
        assert_eq!(value.get_value(), wide("2030-01-02T03:04:05.006"));
    }
    #[test]
    fn numeric_edit_masks_preserve_temporary_signs_limits_and_locale_decimal() {
        let value = CxfaLocaleValue::new();
        for (integer, decimal, expected) in [
            (3, 2, "szzz.zz"),
            (-1, -1, "sz*.z*"),
            (0, 0, "s"),
            (2, 0, "szz"),
        ] {
            let mut format = Vec::new();
            assert!(value.get_numeric_format(&mut format, integer, decimal));
            assert_eq!(format, wide(expected));
        }
        assert!(!value.get_numeric_format(&mut Vec::new(), -2, 2));
        assert!(!value.get_numeric_format(&mut Vec::new(), i32::MAX, i32::MAX));
        let format = wide("szzz.zz");
        for input in ["", "+", "-", "12", "123", "123.", "123.4", "-123.45"] {
            assert!(
                value.validate_numeric_temp(&wide(input), &format, None),
                "{input}"
            );
        }
        for input in ["1234", "1.234", "1x", "1.2x", "++1"] {
            assert!(
                !value.validate_numeric_temp(&wide(input), &format, None),
                "{input}"
            );
        }
        assert!(!value.validate_numeric_temp(&wide("1."), &wide("szz"), None));
        let mgr = manager("de_DE");
        let locale = mgr.get_def_locale().unwrap();
        assert!(value.validate_numeric_temp(&wide("-12,34"), &format, Some(locale.as_ref())));
    }
    #[test]
    fn locale_picture_alternatives_validate_parse_format_and_restore_default() {
        let mgr = manager("en_US");
        let original = mgr.get_def_locale().unwrap();
        let german = mgr.get_locale_by_name(&wide("de_DE")).unwrap();
        let value =
            CxfaLocaleValue::with_value(ValueType::Decimal, &wide("1234.5"), Some(mgr.clone()));
        let mut output = Vec::new();
        assert!(value.format_patterns(
            &mut output,
            &wide("num{z,zz9.99}"),
            Some(german.clone()),
            XfaValuePicture::Display
        ));
        assert_eq!(output, wide("1.234,50"));
        assert!(Rc::ptr_eq(&original, &mgr.get_def_locale().unwrap()));
        let parsed = CxfaLocaleValue::with_pattern(
            ValueType::Decimal,
            &output,
            &wide("num{z,zz9.99}"),
            Some(german.clone()),
            Some(mgr.clone()),
        );
        assert!(parsed.is_valid());
        assert_eq!(parsed.get_double_num(), 1234.5);
        assert!(Rc::ptr_eq(&original, &mgr.get_def_locale().unwrap()));
        let mut matched = Vec::new();
        assert!(parsed.validate_value(
            &wide("1234.50"),
            &wide("null{'missing'}|num{z,zz9.99}"),
            None,
            Some(&mut matched)
        ));
        assert_eq!(matched, wide("num{z,zz9.99}"));
        let failed = CxfaLocaleValue::with_pattern(
            ValueType::Date,
            &wide("not a date"),
            &wide("date{YYYY-MM-DD}"),
            Some(german),
            Some(mgr.clone()),
        );
        assert!(!failed.is_valid());
        assert_eq!(failed.get_value(), wide("not a date"));
        assert!(Rc::ptr_eq(&original, &mgr.get_def_locale().unwrap()));
    }
}
