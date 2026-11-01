// Copyright 2014 The PDFium Authors
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Common, text and literal picture clauses from
//! `xfa/fgas/crt/cfgas_stringformatter.{h,cpp}`. Numeric and date clauses extend
//! this same formatter in sibling modules; no second expression engine exists.
use std::collections::VecDeque;
use unicode_general_category::{get_general_category, GeneralCategory};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    Unknown,
    Date,
    Time,
    DateTime,
    Num,
    Text,
    Zero,
    Null,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DateTimeType {
    Unknown,
    Date,
    Time,
    DateTime,
    TimeDate,
}

#[derive(Clone, Debug)]
pub struct CfgasStringFormatter {
    pub(super) pattern: Vec<u32>,
}

fn is_constant(character: u32) -> bool {
    matches!(character, 44 | 45 | 58 | 47 | 46 | 32)
}
fn equals_ascii(value: &[u32], ascii: &[u8]) -> bool {
    value.len() == ascii.len()
        && value
            .iter()
            .zip(ascii)
            .all(|(&left, &right)| left == u32::from(right))
}
fn is_alpha(character: u32) -> bool {
    // ICU u_isalpha selects the five Letter categories. Rust's broader
    // Alphabetic property also accepts marks and letter-number characters.
    char::from_u32(character).is_some_and(|character| {
        matches!(
            get_general_category(character),
            GeneralCategory::UppercaseLetter
                | GeneralCategory::LowercaseLetter
                | GeneralCategory::TitlecaseLetter
                | GeneralCategory::ModifierLetter
                | GeneralCategory::OtherLetter
        )
    })
}
fn is_digit(character: u32) -> bool {
    (48..=57).contains(&character)
}
fn convert_hex(value: u32, character: u32) -> u32 {
    let digit = match character {
        48..=57 => character - 48,
        65..=70 => character - 65 + 10,
        97..=102 => character - 97 + 10,
        _ => return value,
    };
    value * 16 + digit
}

/// Source GetLiteralText: the cursor finishes one past the closing quote, or
/// at the end of the pattern. Doubled quotes and four-digit escapes retain
/// their source interpretation; raw wide code units are preserved.
pub(super) fn get_literal_text(pattern: &[u32], index: &mut usize) -> Vec<u32> {
    let mut output = Vec::new();
    if pattern.get(*index) != Some(&39) {
        return output;
    }
    *index += 1;
    let mut quote_count = 1usize;
    while *index < pattern.len() {
        if pattern[*index] == 39 {
            quote_count += 1;
            *index += 1;
            if *index >= pattern.len() || (pattern[*index] != 39 && quote_count % 2 == 0) {
                break;
            }
            quote_count += 1;
        } else if pattern[*index] == 92 && pattern.get(*index + 1) == Some(&117) {
            let mut value = 0;
            *index += 2;
            for _ in 0..4 {
                let Some(&character) = pattern.get(*index) else {
                    break;
                };
                *index += 1;
                value = convert_hex(value, character);
            }
            if value != 0 {
                output.push(value & 0xffff);
            }
            continue;
        }
        output.push(pattern[*index]);
        *index += 1;
    }
    output
}

/// Source GetLiteralTextReverse: the cursor finishes at the opening quote;
/// the enclosing reverse loop decrements it afterward. Exhaustion uses
/// usize::MAX, matching the source's defined unsigned-underflow sentinel.
pub(super) fn get_literal_text_reverse(pattern: &[u32], index: &mut usize) -> Vec<u32> {
    let mut output = VecDeque::new();
    if pattern.get(*index) != Some(&39) {
        return Vec::new();
    }
    *index = index.wrapping_sub(1);
    let mut quote_count = 1usize;
    while *index < pattern.len() {
        if pattern[*index] == 39 {
            quote_count += 1;
            let previous = index.wrapping_sub(1);
            if previous >= pattern.len() || (pattern[previous] != 39 && quote_count % 2 == 0) {
                break;
            }
            quote_count += 1;
            *index = previous;
        } else if pattern[*index] == 92 && pattern.get(*index + 1) == Some(&117) {
            *index = index.wrapping_sub(1);
            let mut value = 0;
            let length = output.len().min(5);
            for position in 1..length {
                value = convert_hex(value, output[position]);
            }
            if value != 0 {
                output.drain(..length);
                output.push_front(value & 0xffff);
            }
            continue;
        }
        output.push_front(pattern[*index]);
        *index = index.wrapping_sub(1);
    }
    output.into_iter().collect()
}

impl CfgasStringFormatter {
    pub fn new(pattern: &[u32]) -> Self {
        Self {
            pattern: pattern.to_vec(),
        }
    }

    pub fn split_on_bars(format: &[u32]) -> Vec<Vec<u32>> {
        let mut results = Vec::new();
        let mut start = 0;
        let mut quoted = false;
        for (index, &character) in format.iter().enumerate() {
            if character == 39 {
                quoted = !quoted;
            } else if character == 124 && !quoted {
                results.push(format[start..index].to_vec());
                start = index + 1;
            }
        }
        results.push(format[start..].to_vec());
        results
    }

    pub fn get_category(&self) -> Category {
        let mut category = Category::Unknown;
        let mut index = 0;
        let mut brace_open = false;
        while index < self.pattern.len() {
            if self.pattern[index] == 39 {
                get_literal_text(&self.pattern, &mut index);
                continue;
            }
            if !brace_open && !is_constant(self.pattern[index]) {
                let start = index;
                index += 1;
                loop {
                    let Some(&character) = self.pattern.get(index) else {
                        return category;
                    };
                    if matches!(character, 46 | 40) {
                        break;
                    }
                    if character == 123 {
                        brace_open = true;
                        break;
                    }
                    index += 1;
                }
                let name = &self.pattern[start..index];
                if equals_ascii(name, b"datetime") {
                    return Category::DateTime;
                }
                if equals_ascii(name, b"text") {
                    return Category::Text;
                }
                if equals_ascii(name, b"num") {
                    return Category::Num;
                }
                if equals_ascii(name, b"zero") {
                    return Category::Zero;
                }
                if equals_ascii(name, b"null") {
                    return Category::Null;
                }
                if equals_ascii(name, b"date") {
                    if category == Category::Time {
                        return Category::DateTime;
                    }
                    category = Category::Date;
                } else if equals_ascii(name, b"time") {
                    if category == Category::Date {
                        return Category::DateTime;
                    }
                    category = Category::Time;
                }
            } else if self.pattern[index] == 125 {
                brace_open = false;
            }
            index += 1;
        }
        category
    }

    pub(super) fn get_text_format(&self, category: &[u32]) -> Vec<u32> {
        let mut index = 0;
        let mut bracket_open = false;
        let mut result = Vec::new();
        while index < self.pattern.len() {
            if self.pattern[index] == 39 {
                let start = index;
                get_literal_text(&self.pattern, &mut index);
                result.extend_from_slice(&self.pattern[start..index]);
                continue;
            }
            if !bracket_open && !is_constant(self.pattern[index]) {
                let start = index;
                index += 1;
                while index < self.pattern.len() && !matches!(self.pattern[index], 123 | 46 | 40) {
                    index += 1;
                }
                if &self.pattern[start..index] != category {
                    continue;
                }
                while index < self.pattern.len() {
                    if self.pattern[index] == 40 {
                        index += 1;
                        while index < self.pattern.len() && self.pattern[index] != 41 {
                            index += 1;
                        }
                    } else if self.pattern[index] == 123 {
                        bracket_open = true;
                        break;
                    }
                    index += 1;
                }
            } else if self.pattern[index] != 125 {
                result.push(self.pattern[index]);
            }
            index += 1;
        }
        if bracket_open {
            result
        } else {
            self.pattern.clone()
        }
    }

    pub fn parse_text(&self, source: &[u32], output: &mut Vec<u32>) -> bool {
        output.clear();
        if source.is_empty() || self.pattern.is_empty() {
            return false;
        }
        let pattern = self.get_text_format(&[116, 101, 120, 116]);
        if pattern.is_empty() {
            return false;
        }
        let (mut text, mut index) = (0, 0);
        while index < pattern.len() && text < source.len() {
            match pattern[index] {
                39 => {
                    let literal = get_literal_text(&pattern, &mut index);
                    if !source[text..].starts_with(&literal) {
                        output.clear();
                        output.extend_from_slice(source);
                        return false;
                    }
                    text += literal.len();
                }
                65 | 88 | 79 | 48 | 57 => {
                    let accepts = match pattern[index] {
                        65 => is_alpha(source[text]),
                        88 => true,
                        79 | 48 => is_digit(source[text]) || is_alpha(source[text]),
                        _ => is_digit(source[text]),
                    };
                    if accepts {
                        output.push(source[text]);
                        text += 1;
                    }
                    index += 1;
                }
                character => {
                    if character != source[text] {
                        output.clear();
                        output.extend_from_slice(source);
                        return false;
                    }
                    index += 1;
                    text += 1;
                }
            }
        }
        index == pattern.len() && text == source.len()
    }

    pub fn format_text(&self, source: &[u32], output: &mut Vec<u32>) -> bool {
        if source.is_empty() || self.pattern.is_empty() {
            return false;
        }
        let pattern = self.get_text_format(&[116, 101, 120, 116]);
        let (mut text, mut index) = (0, 0);
        while index < pattern.len() {
            match pattern[index] {
                39 => output.extend(get_literal_text(&pattern, &mut index)),
                65 | 88 | 79 | 48 | 57 => {
                    let Some(&character) = source.get(text) else {
                        return false;
                    };
                    let accepts = match pattern[index] {
                        65 => is_alpha(character),
                        88 => true,
                        79 | 48 => is_digit(character) || is_alpha(character),
                        _ => is_digit(character),
                    };
                    if !accepts {
                        return false;
                    }
                    output.push(character);
                    text += 1;
                    index += 1;
                }
                character => {
                    output.push(character);
                    index += 1;
                }
            }
        }
        text == source.len()
    }

    fn parse_literal_clause(&self, category: &[u32], source: &[u32]) -> bool {
        let pattern = self.get_text_format(category);
        let (mut text, mut index) = (0, 0);
        while index < pattern.len() && text < source.len() {
            if pattern[index] == 39 {
                let literal = get_literal_text(&pattern, &mut index);
                if !source[text..].starts_with(&literal) {
                    return false;
                }
                text += literal.len();
            } else {
                if pattern[index] != source[text] {
                    return false;
                }
                text += 1;
                index += 1;
            }
        }
        index == pattern.len() && text == source.len()
    }
    pub fn parse_zero(&self, source: &[u32]) -> bool {
        self.parse_literal_clause(&[122, 101, 114, 111], source)
    }
    pub fn parse_null(&self, source: &[u32]) -> bool {
        self.parse_literal_clause(&[110, 117, 108, 108], source)
    }
    fn format_literal_clause(&self, category: &[u32], output: &mut Vec<u32>) -> bool {
        if self.pattern.is_empty() {
            return false;
        }
        let pattern = self.get_text_format(category);
        let mut index = 0;
        while index < pattern.len() {
            if pattern[index] == 39 {
                output.extend(get_literal_text(&pattern, &mut index));
            } else {
                output.push(pattern[index]);
                index += 1;
            }
        }
        true
    }
    pub fn format_zero(&self, output: &mut Vec<u32>) -> bool {
        self.format_literal_clause(&[122, 101, 114, 111], output)
    }
    pub fn format_null(&self, output: &mut Vec<u32>) -> bool {
        self.format_literal_clause(&[110, 117, 108, 108], output)
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn formatter(pattern: &str) -> CfgasStringFormatter {
        CfgasStringFormatter::new(&wide(pattern))
    }
    #[test]
    fn SplitFormatString() {
        assert_eq!(
            CfgasStringFormatter::split_on_bars(&[]),
            vec![Vec::<u32>::new()]
        );
        assert_eq!(
            CfgasStringFormatter::split_on_bars(&wide("|")),
            vec![Vec::<u32>::new(), Vec::new()]
        );
        assert_eq!(
            CfgasStringFormatter::split_on_bars(&wide(
                "null{'No|data'} | null{} | text{999*9999} | text{999*999*9999}"
            )),
            [
                "null{'No|data'} ",
                " null{} ",
                " text{999*9999} ",
                " text{999*999*9999}"
            ]
            .map(wide)
        );
    }
    #[test]
    fn TextParse() {
        for (source, pattern, expected) in [
            ("ABC-1234-5", "AAA-9999-X", "ABC12345"),
            ("ABC-1234-D", "AAA-9999-X", "ABC1234D"),
            ("A1C-1234-D", "OOO-9999-X", "A1C1234D"),
            ("A1C-1234-D", "000-9999-X", "A1C1234D"),
            ("A1C-1234-D text", "000-9999-X 'text'", "A1C1234D"),
        ] {
            let mut output = Vec::new();
            assert!(
                formatter(pattern).parse_text(&wide(source), &mut output),
                "{source}/{pattern}"
            );
            assert_eq!(output, wide(expected));
        }
    }
    #[test]
    fn InvalidTextParse() {
        let mut output = Vec::new();
        assert!(!formatter("AAA-9999-X").parse_text(&wide("123-4567-8"), &mut output));
    }
    #[test]
    fn TextFormat() {
        for (source, pattern, expected) in [
            ("K1S5K2", "A9A 9A9", "K1S 5K2"),
            ("K1S5K2", "text(fr){A9A 9A9}", "K1S 5K2"),
            ("6135551212", "'+1 ('9#99') '999-9999", "+1 (6#13) 555-1212"),
            ("6135551212", "999.999.9999", "613.555.1212"),
            ("6135551212", "999#999*9999", "613#555*1212"),
            ("K1#5K2", "00X OO9", "K1# 5K2"),
        ] {
            let mut output = Vec::new();
            assert!(
                formatter(pattern).format_text(&wide(source), &mut output),
                "{source}/{pattern}"
            );
            assert_eq!(output, wide(expected));
        }
    }
    #[test]
    fn NullParse() {
        for (source, pattern) in [("", "null{}"), ("No data", "null{'No data'}")] {
            assert!(formatter(pattern).parse_null(&wide(source)));
        }
    }
    #[test]
    fn NullFormat() {
        for (pattern, expected) in [("null{'n/a'}", "n/a"), ("null{}", "")] {
            let mut output = Vec::new();
            assert!(formatter(pattern).format_null(&mut output));
            assert_eq!(output, wide(expected));
        }
    }
    #[test]
    fn ZeroParse() {
        for (source, pattern) in [("", "zero{}"), ("9", "zero{9}"), ("a", "zero{'a'}")] {
            assert!(formatter(pattern).parse_zero(&wide(source)));
        }
    }
    #[test]
    fn ZeroFormat() {
        let mut output = Vec::new();
        assert!(formatter("zero{}").format_zero(&mut output));
        assert_eq!(output, wide(""));
    }
    #[test]
    fn GetCategory() {
        for (pattern, expected) in [
            ("'just text'", Category::Unknown),
            ("null{}", Category::Null),
            ("zero{}", Category::Zero),
            ("num{}", Category::Num),
            ("text{}", Category::Text),
            ("datetime{}", Category::DateTime),
            ("time{}", Category::Time),
            ("date{}", Category::Date),
            ("time{} date{}", Category::DateTime),
            ("date{} time{}", Category::DateTime),
            ("num(en_GB){}", Category::Num),
            ("date.long{}", Category::Date),
        ] {
            assert_eq!(formatter(pattern).get_category(), expected, "{pattern}");
        }
    }
    #[test]
    fn literal_cursors_escaping_raw_wide_units_and_output_contracts() {
        for (text, expected) in [
            ("'a''b'", "a'b"),
            ("'\\u0023x'", "#x"),
            ("'unterminated", "unterminated"),
        ] {
            let pattern = wide(text);
            let mut index = 0;
            assert_eq!(get_literal_text(&pattern, &mut index), wide(expected));
            assert_eq!(index, pattern.len());
        }
        for (text, expected) in [("'a''b'", "a'b"), ("'\\u0023x'", "#x"), ("''", "")] {
            let pattern = wide(text);
            let mut index = pattern.len() - 1;
            assert_eq!(
                get_literal_text_reverse(&pattern, &mut index),
                wide(expected)
            );
            assert_eq!(index, 0);
        }
        let mut index = 0;
        assert!(get_literal_text_reverse(&[39], &mut index).is_empty());
        assert_eq!(index, usize::MAX);
        let mut output = vec![0xd800];
        assert!(formatter("X").format_text(&[0x110000], &mut output));
        assert_eq!(output, vec![0xd800, 0x110000]);
        assert!(!formatter("'literal'").parse_text(&wide("wrong"), &mut output));
        assert_eq!(output, wide("wrong"));
        assert!(!formatter("X").parse_text(&[], &mut output));
        assert!(output.is_empty());
        assert!(!formatter("").format_null(&mut output));
        assert!(!formatter("null{'n/a'}").parse_null(&wide("other")));
        assert!(formatter("AAA").format_text(&wide("é中Ω"), &mut output));
        assert!(!formatter("A").format_text(&wide("Ⅰ"), &mut output));
    }
}
