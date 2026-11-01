// Copyright 2014 The PDFium Authors
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Numeric portions of `cfgas_stringformatter.cpp`: picture expansion, the
//! two directional parse/format passes, and decimal exponent/percent scaling.
use super::cfgas_decimal::CfgasDecimal;
use super::cfgas_stringformatter::{
    get_literal_text, get_literal_text_reverse, CfgasStringFormatter,
};
use super::locale_iface::{LocaleIface, NumSubcategory};
use super::locale_mgr_iface::LocaleMgrIface;
use std::collections::VecDeque;
use std::rc::Rc;

const PERCENT: u8 = 1;
const EXPONENT: u8 = 2;
const DOT_V: u8 = 4;
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn is_digit(ch: u32) -> bool {
    (48..=57).contains(&ch)
}
fn starts_at(input: &[u32], at: usize, expected: &[u32]) -> bool {
    at.checked_add(expected.len())
        .and_then(|end| input.get(at..end))
        == Some(expected)
}
fn consume_reverse(input: &[u32], at: &mut usize, expected: &[u32]) -> bool {
    if expected.is_empty() {
        return true;
    }
    let start = at.wrapping_sub(expected.len() - 1);
    if !starts_at(input, start, expected) {
        return false;
    }
    *at = start.wrapping_sub(1);
    true
}
fn prepend(output: &mut VecDeque<u32>, text: &[u32]) {
    for &ch in text.iter().rev() {
        output.push_front(ch);
    }
}
fn numeric_dot_index(input: &[u32], symbol: &[u32]) -> (bool, usize) {
    let mut at = 0;
    while at < input.len() {
        // This is the actual value, not a picture. An apostrophe may be the
        // locale's grouping symbol; picture quoting belongs to numeric_format
        // and the literal-token parse branches below.
        if starts_at(input, at, symbol) {
            return (true, at);
        }
        at += 1;
    }
    match input.iter().position(|&ch| ch == 46) {
        Some(index) => (true, index),
        None => (false, input.len()),
    }
}
fn trailing_limit(pattern: &[u32], dot: usize) -> (usize, bool) {
    let mut count = 0;
    let mut trim = false;
    for &ch in pattern.iter().skip(dot.saturating_add(1)) {
        if matches!(ch, 122 | 57 | 90) {
            count += 1;
            trim = ch != 57;
        }
    }
    (count, trim)
}

impl CfgasStringFormatter {
    fn numeric_format(
        &self,
        mgr: &dyn LocaleMgrIface,
    ) -> Option<(Rc<dyn LocaleIface>, usize, u8, Vec<u32>)> {
        let pattern = &self.pattern;
        let mut locale = None;
        let mut at = 0;
        let mut found_dot = false;
        let mut bracket_open = false;
        let mut output = Vec::new();
        let mut dot = 0;
        let mut style = 0;
        while at < pattern.len() {
            let ch = pattern[at];
            if ch == 39 {
                let start = at;
                get_literal_text(pattern, &mut at);
                output.extend_from_slice(&pattern[start..at]);
                continue;
            }
            if !bracket_open && !matches!(ch, 44 | 45 | 58 | 47 | 46 | 32) {
                let start = at;
                at += 1;
                while at < pattern.len() && !matches!(pattern[at], 123 | 46 | 40) {
                    at += 1;
                }
                if &pattern[start..at] != [110, 117, 109] {
                    // This is a bare picture, not a `num` category wrapper.
                    // Reparse its whole contents once: retaining the prefix
                    // already collected here duplicates leading literals and
                    // separators in the source implementation.
                    bracket_open = true;
                    output.clear();
                    found_dot = false;
                    dot = 0;
                    style = 0;
                    at = 0;
                    continue;
                }
                while at < pattern.len() {
                    if pattern[at] == 123 {
                        bracket_open = true;
                        break;
                    }
                    if pattern[at] == 40 {
                        at += 1;
                        let name = at;
                        while at < pattern.len() && pattern[at] != 41 {
                            at += 1;
                        }
                        locale = mgr.get_locale_by_name(&pattern[name..at]);
                    } else if pattern[at] == 46 {
                        at += 1;
                        let start = at;
                        while at < pattern.len() && !matches!(pattern[at], 40 | 123) {
                            at += 1;
                        }
                        let name = &pattern[start..at];
                        let hash = name
                            .iter()
                            .fold(0u32, |hash, &ch| hash.wrapping_mul(1313).wrapping_add(ch));
                        let category = match hash {
                            0x46f95531 => NumSubcategory::Percent,
                            0x4c4e8acb => NumSubcategory::Currency,
                            0x7568e6ae => NumSubcategory::Integer,
                            _ => NumSubcategory::Decimal,
                        };
                        if locale.is_none() {
                            locale = mgr.get_def_locale();
                        }
                        let subpattern = locale.as_ref()?.get_num_pattern(category);
                        if let Some(index) = subpattern
                            .iter()
                            .position(|&ch| ch == 46)
                            .filter(|&i| i != 0)
                        {
                            if !found_dot {
                                dot = output.len() + index;
                            }
                            found_dot = true;
                            style |= DOT_V;
                        }
                        output.extend_from_slice(&subpattern);
                        if category == NumSubcategory::Percent {
                            style |= PERCENT;
                        }
                        continue;
                    }
                    at += 1;
                }
            } else if ch == 69 {
                style |= EXPONENT;
                output.push(ch);
            } else if ch == 37 {
                style |= PERCENT;
                output.push(ch);
            } else if ch != 125 {
                output.push(ch);
            }
            if !found_dot && at < pattern.len() && matches!(pattern[at], 46 | 86 | 118) {
                found_dot = true;
                dot = output.len().wrapping_sub(1);
                style |= DOT_V;
            }
            at += 1;
        }
        if !found_dot {
            dot = output.len();
        }
        if locale.is_none() {
            locale = mgr.get_def_locale();
        }
        Some((locale?, dot, style, output))
    }

    pub fn parse_num(
        &self,
        mgr: &dyn LocaleMgrIface,
        input: &[u32],
        output: &mut Vec<u32>,
    ) -> bool {
        output.clear();
        if input.is_empty() || self.pattern.is_empty() {
            return false;
        }
        let Some((locale, dot_f, style, pattern)) = self.numeric_format(mgr) else {
            return false;
        };
        if pattern.is_empty() {
            return false;
        }
        let decimal_symbol = locale.get_decimal_symbol();
        let group = locale.get_grouping_symbol();
        let minus = locale.get_minus_symbol();
        let mut exponent = 0i32;
        let mut percent = false;
        let mut negative = false;
        let (has_dot, dot) = numeric_dot_index(input, &decimal_symbol);
        let reverse_only = !has_dot && style & DOT_V != 0;
        let mut at = dot.wrapping_sub(1);
        let mut pf = dot_f.wrapping_sub(1);
        // Integer digits are collected backwards and reversed once. Native
        // InsertAtFront has the same order but repeatedly moves the prefix.
        let mut integer = Vec::new();
        while pf < pattern.len() && at < input.len() {
            let token = pattern[pf];
            match token {
                39 => {
                    let literal = get_literal_text_reverse(&pattern, &mut pf);
                    if !consume_reverse(input, &mut at, &literal) {
                        return false;
                    }
                    pf = pf.wrapping_sub(1);
                }
                57 => {
                    if !is_digit(input[at]) {
                        return false;
                    }
                    integer.push(input[at]);
                    at = at.wrapping_sub(1);
                    pf = pf.wrapping_sub(1);
                }
                122 | 90 => {
                    if token == 122 || input[at] != 32 {
                        if is_digit(input[at]) {
                            integer.push(input[at]);
                            at = at.wrapping_sub(1);
                        }
                    } else {
                        at = at.wrapping_sub(1);
                    }
                    pf = pf.wrapping_sub(1);
                }
                83 | 115 => {
                    if input[at] == 43 || (token == 83 && input[at] == 32) {
                        at = at.wrapping_sub(1);
                    } else {
                        if !consume_reverse(input, &mut at, &minus) {
                            return false;
                        }
                        negative = true;
                    }
                    pf = pf.wrapping_sub(1);
                }
                69 => {
                    let mut exp_negative = false;
                    let mut value = 0i32;
                    let mut multiplier = 1i32;
                    while at < input.len() {
                        let ch = input[at];
                        if matches!(ch, 69 | 101) {
                            break;
                        }
                        if is_digit(ch) {
                            let Some(add) = multiplier.checked_mul((ch - 48) as i32) else {
                                return false;
                            };
                            let Some(next) = value.checked_add(add) else {
                                return false;
                            };
                            value = next;
                            let Some(next) = multiplier.checked_mul(10) else {
                                return false;
                            };
                            multiplier = next;
                            at = at.wrapping_sub(1);
                            continue;
                        }
                        if ch == 43 {
                            at = at.wrapping_sub(1);
                            continue;
                        }
                        if !minus.is_empty() && consume_reverse(input, &mut at, &minus) {
                            exp_negative = true;
                            continue;
                        }
                        return false;
                    }
                    at = at.wrapping_sub(1);
                    exponent = if exp_negative { -value } else { value };
                    pf = pf.wrapping_sub(1);
                }
                36 | 37 => {
                    let symbol = if token == 36 {
                        locale.get_currency_symbol()
                    } else {
                        locale.get_percent_symbol()
                    };
                    if !consume_reverse(input, &mut at, &symbol) {
                        return false;
                    }
                    percent |= token == 37;
                    pf = pf.wrapping_sub(1);
                }
                114 | 82 | 98 | 66 => {
                    let previous = pf.wrapping_sub(1);
                    let first = if matches!(token, 114 | 82) {
                        if token == 82 {
                            67
                        } else {
                            99
                        }
                    } else if token == 66 {
                        68
                    } else {
                        100
                    };
                    if previous < pattern.len() && pattern[previous] == first {
                        if matches!(token, 82 | 66) && input[at] == 32 {
                            at = at.wrapping_sub(2);
                        } else if input[at] == if matches!(token, 114 | 82) { 82 } else { 66 }
                            && at.wrapping_sub(1) < input.len()
                            && input[at - 1] == if matches!(token, 114 | 82) { 67 } else { 68 }
                        {
                            negative = true;
                            at = at.wrapping_sub(2);
                        }
                        pf = pf.wrapping_sub(2);
                    } else {
                        pf = pf.wrapping_sub(1);
                    }
                }
                46 | 86 | 118 | 56 => return false,
                44 => {
                    consume_reverse(input, &mut at, &group);
                    pf = pf.wrapping_sub(1);
                }
                40 | 41 => {
                    if input[at] == token {
                        negative = true;
                    } else if input[at] != 32 {
                        return false;
                    }
                    at = at.wrapping_sub(1);
                    pf = pf.wrapping_sub(1);
                }
                _ => {
                    if token != input[at] {
                        return false;
                    }
                    at = at.wrapping_sub(1);
                    pf = pf.wrapping_sub(1);
                }
            }
        }
        if at < input.len() {
            if input[at] == 45 {
                negative = true;
                at = at.wrapping_sub(1);
            }
            if at < input.len() {
                return false;
            }
        }
        integer.reverse();
        output.extend(integer);
        if style & DOT_V != 0 && dot < input.len() {
            output.push(46);
        }
        if !reverse_only {
            at = if dot == input.len() {
                input.len()
            } else {
                dot + 1
            };
            pf = dot_f.saturating_add(1);
            while at < input.len() && pf < pattern.len() {
                let token = pattern[pf];
                match token {
                    39 => {
                        let literal = get_literal_text(&pattern, &mut pf);
                        if !starts_at(input, at, &literal) {
                            return false;
                        }
                        at += literal.len();
                        continue;
                    }
                    57 => {
                        if !is_digit(input[at]) {
                            return false;
                        }
                        output.push(input[at]);
                        at += 1;
                    }
                    122 | 90 => {
                        if token == 122 || input[at] != 32 {
                            if is_digit(input[at]) {
                                output.push(input[at]);
                                at += 1;
                            }
                        } else {
                            at += 1;
                        }
                    }
                    83 | 115 => {
                        if input[at] == 43 || (token == 83 && input[at] == 32) {
                            at += 1;
                        } else {
                            if !starts_at(input, at, &minus) {
                                return false;
                            }
                            negative = true;
                            at += minus.len();
                        }
                    }
                    69 => {
                        if !matches!(input[at], 69 | 101) {
                            return false;
                        }
                        exponent = 0;
                        at += 1;
                        let exp_negative = input.get(at) == Some(&45);
                        if matches!(input.get(at), Some(43 | 45)) {
                            at += 1;
                        }
                        while at < input.len() && is_digit(input[at]) {
                            let Some(next) = exponent
                                .checked_mul(10)
                                .and_then(|v| v.checked_add((input[at] - 48) as i32))
                            else {
                                return false;
                            };
                            exponent = next;
                            at += 1;
                        }
                        if exp_negative {
                            exponent = -exponent;
                        }
                    }
                    36 => {
                        let symbol = locale.get_currency_symbol();
                        if !starts_at(input, at, &symbol) {
                            return false;
                        }
                        at += symbol.len();
                    }
                    99 | 67 | 100 | 68 => {
                        let second = if matches!(token, 99 | 67) {
                            if token == 67 {
                                82
                            } else {
                                114
                            }
                        } else if token == 68 {
                            66
                        } else {
                            98
                        };
                        if pattern.get(pf + 1) == Some(&second) {
                            if matches!(token, 67 | 68) && input[at] == 32 {
                                at += 1;
                            } else if input[at] == if matches!(token, 99 | 67) { 67 } else { 68 }
                                && input.get(at + 1)
                                    == Some(&if matches!(token, 99 | 67) { 82 } else { 66 })
                            {
                                negative = true;
                                at += 2;
                            }
                            pf += 1;
                        }
                    }
                    46 | 86 | 118 => return false,
                    37 => {
                        let symbol = locale.get_percent_symbol();
                        if starts_at(input, at, &symbol) {
                            at += symbol.len();
                        }
                        percent = true;
                    }
                    56 => {
                        while pattern.get(pf + 1) == Some(&56) {
                            pf += 1;
                        }
                        while at < input.len() && is_digit(input[at]) {
                            output.push(input[at]);
                            at += 1;
                        }
                    }
                    44 => {
                        if starts_at(input, at, &group) {
                            at += group.len();
                        }
                    }
                    40 | 41 => {
                        if input[at] == token {
                            negative = true;
                        } else if input[at] != 32 {
                            return false;
                        }
                        at += 1;
                    }
                    _ => {
                        if token != input[at] {
                            return false;
                        }
                        at += 1;
                    }
                }
                pf += 1;
            }
            if at != input.len() {
                return false;
            }
        }
        if exponent != 0 || percent {
            let mut decimal = CfgasDecimal::from_wide_string(output);
            if exponent != 0 {
                let Some(power) = CfgasDecimal::try_from_f32(10f32.powf(exponent as f32), 3) else {
                    return false;
                };
                decimal = decimal * power;
            }
            if percent {
                decimal = decimal / CfgasDecimal::from_u32(100);
            }
            *output = decimal.to_wide_string();
        }
        if negative {
            output.insert(0, 45);
        }
        true
    }

    pub fn format_num(
        &self,
        mgr: &dyn LocaleMgrIface,
        input: &[u32],
        output: &mut Vec<u32>,
    ) -> bool {
        if input.is_empty() || self.pattern.is_empty() {
            return false;
        }
        let Some((locale, dot_f, style, pattern)) = self.numeric_format(mgr) else {
            return false;
        };
        if pattern.is_empty() {
            return false;
        }
        let first = input.iter().position(|&ch| ch != 48).unwrap_or(input.len());
        let mut source = input[first..].to_vec();
        if source.is_empty() || source[0] == 46 {
            source.insert(0, 48);
        }
        let mut decimal = CfgasDecimal::from_wide_string(&source);
        if style & PERCENT != 0 {
            decimal = decimal * CfgasDecimal::from_u32(100);
            source = decimal.to_wide_string();
        }
        let mut exponent = 0i32;
        if style & EXPONENT != 0 {
            let mut fixed = 0usize;
            let mut pf = 0;
            while pf < dot_f && pf < pattern.len() {
                if pattern[pf] == 39 {
                    get_literal_text(&pattern, &mut pf);
                    continue;
                }
                if matches!(pattern[pf], 57 | 122 | 90) {
                    fixed += 1;
                }
                pf += 1;
            }
            let mut threshold = 1u32;
            for _ in 1..fixed {
                let Some(next) = threshold.checked_mul(10) else {
                    return false;
                };
                threshold = next;
            }
            let mut adjusted = false;
            while decimal.is_not_zero() && decimal.to_double().abs() < f64::from(threshold) {
                decimal = decimal * CfgasDecimal::from_u32(10);
                exponent -= 1;
                adjusted = true;
            }
            if !adjusted {
                let Some(next) = threshold.checked_mul(10) else {
                    return false;
                };
                threshold = next;
                while decimal.is_not_zero() && decimal.to_double().abs() > f64::from(threshold) {
                    decimal = decimal / CfgasDecimal::from_u32(10);
                    exponent += 1;
                }
            }
            // Decimal division can shed redundant zeros. Always synchronize
            // the source digits after exponent normalization, even when its
            // resulting scale already equals the picture's fractional limit.
            if fixed != 0 {
                source = decimal.to_wide_string();
            }
        }
        let (trailing, trim) = trailing_limit(&pattern, dot_f);
        let scale = decimal.get_scale();
        if trailing < usize::from(scale) {
            decimal.set_scale(trailing as u8);
            source = decimal.to_wide_string();
        }
        if trim && scale > 0 && trailing > 0 {
            while source.last() == Some(&48) {
                source.pop();
            }
            if source.last() == Some(&46) {
                source.pop();
            }
        }
        let group = locale.get_grouping_symbol();
        let negative = source.first() == Some(&45);
        if negative {
            source.remove(0);
        }
        let dot = source
            .iter()
            .position(|&ch| ch == 46)
            .unwrap_or(source.len());
        let mut at = dot.wrapping_sub(1);
        let mut result: VecDeque<u32> = std::mem::take(output).into();
        let mut negative_emitted = false;
        let mut pf = dot_f.wrapping_sub(1);
        while pf < pattern.len() {
            let token = pattern[pf];
            match token {
                57 | 122 | 90 => {
                    if at < source.len() {
                        if !is_digit(source[at]) {
                            *output = result.into();
                            return false;
                        }
                        if token == 57 {
                            result.push_front(source[at]);
                        } else if token == 122 {
                            if source.first() != Some(&48) {
                                result.push_front(source[at]);
                            }
                        } else {
                            result.push_front(if source.first() == Some(&48) {
                                32
                            } else {
                                source[at]
                            });
                        }
                        at = at.wrapping_sub(1);
                    } else if token == 57 {
                        result.push_front(48);
                    } else if token == 90 {
                        result.push_front(32);
                    }
                }
                83 | 115 => {
                    if negative {
                        prepend(&mut result, &locale.get_minus_symbol());
                        negative_emitted = true;
                    } else if token == 83 {
                        result.push_front(32);
                    }
                }
                69 => prepend(&mut result, &wide(&format!("E{exponent:+}"))),
                36 => prepend(&mut result, &locale.get_currency_symbol()),
                114 | 82 | 98 | 66 => {
                    let previous = pf.wrapping_sub(1);
                    let first = if matches!(token, 114 | 82) {
                        if token == 82 {
                            67
                        } else {
                            99
                        }
                    } else if token == 66 {
                        68
                    } else {
                        100
                    };
                    if previous < pattern.len() && pattern[previous] == first {
                        if negative {
                            prepend(
                                &mut result,
                                &wide(if matches!(token, 114 | 82) {
                                    "CR"
                                } else if token == 98 {
                                    "db"
                                } else {
                                    "DB"
                                }),
                            );
                        } else if matches!(token, 82 | 66) {
                            prepend(&mut result, &wide("  "));
                        }
                        pf = pf.wrapping_sub(1);
                        negative_emitted = true;
                    } else {
                        result.push_front(token);
                    }
                }
                37 => prepend(&mut result, &locale.get_percent_symbol()),
                44 => {
                    if at < source.len() {
                        prepend(&mut result, &group);
                    }
                }
                40 | 41 => {
                    result.push_front(if negative { token } else { 32 });
                    if token == 40 {
                        negative_emitted = true;
                    }
                }
                39 => {
                    let literal = get_literal_text_reverse(&pattern, &mut pf);
                    prepend(&mut result, &literal);
                }
                _ => result.push_front(token),
            }
            pf = pf.wrapping_sub(1);
        }
        if at < source.len() {
            result.clear();
            let position = dot % 3;
            for (index, &ch) in source[..dot].iter().enumerate() {
                if index % 3 == position && index != 0 {
                    result.extend(group.iter().copied());
                }
                result.push_back(ch);
            }
            if dot < source.len() {
                result.extend(locale.get_decimal_symbol());
                result.extend(source[dot + 1..].iter().copied());
            }
            if negative {
                prepend(&mut result, &locale.get_minus_symbol());
            }
            *output = result.into();
            return true;
        }
        if dot_f == pattern.len() {
            if !negative_emitted && negative {
                prepend(&mut result, &locale.get_minus_symbol());
            }
            *output = result.into();
            return true;
        }
        if dot_f >= pattern.len() {
            *output = result.into();
            return false;
        }
        if pattern[dot_f] == 86
            || (pattern[dot_f] == 46
                && (dot < source.len() || matches!(pattern.get(dot_f + 1), Some(57 | 90))))
        {
            result.extend(locale.get_decimal_symbol());
        }
        at = dot.saturating_add(1);
        pf = dot_f + 1;
        while pf < pattern.len() {
            let token = pattern[pf];
            match token {
                39 => {
                    result.extend(get_literal_text(&pattern, &mut pf));
                    continue;
                }
                57 | 122 | 90 => {
                    if at < source.len() {
                        if !is_digit(source[at]) {
                            *output = result.into();
                            return false;
                        }
                        result.push_back(source[at]);
                        at += 1;
                    } else if matches!(token, 57 | 90) {
                        result.push_back(48);
                    }
                }
                69 => result.extend(wide(&format!("E{exponent:+}"))),
                36 => result.extend(locale.get_currency_symbol()),
                99 | 67 | 100 | 68 => {
                    let second = if matches!(token, 99 | 67) {
                        if token == 67 {
                            82
                        } else {
                            114
                        }
                    } else if token == 68 {
                        66
                    } else {
                        98
                    };
                    if pattern.get(pf + 1) == Some(&second) {
                        if negative {
                            result.extend(wide(if matches!(token, 99 | 67) {
                                "CR"
                            } else if token == 100 {
                                "db"
                            } else {
                                "DB"
                            }));
                        } else if matches!(token, 67 | 68) {
                            result.extend(wide("  "));
                        }
                        pf += 1;
                        negative_emitted = true;
                    }
                }
                37 => result.extend(locale.get_percent_symbol()),
                56 => {
                    while pattern.get(pf + 1) == Some(&56) {
                        pf += 1;
                    }
                    while at < source.len() && is_digit(source[at]) {
                        result.push_back(source[at]);
                        at += 1;
                    }
                }
                44 => result.extend(group.iter().copied()),
                40 | 41 => {
                    result.push_back(if negative { token } else { 32 });
                    if token == 40 {
                        negative_emitted = true;
                    }
                }
                _ => {}
            }
            pf += 1;
        }
        if !negative_emitted && negative {
            prepend(&mut result, &locale.get_minus_symbol());
        }
        *output = result.into();
        true
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use crate::xfa::fxfa::parser::cxfa_localemgr::CxfaLocaleMgr;
    fn manager(locale: &str) -> CxfaLocaleMgr {
        CxfaLocaleMgr::new(&wide(locale), Rc::new(|| 0))
    }
    #[test]
    fn grouped_input_apostrophes_do_not_hide_the_decimal_or_picture_literals() {
        use crate::fxcrt::{
            cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser,
        };
        use crate::xfa::fxfa::{
            fxfa_basic::XfaPacketType,
            parser::{cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder},
        };
        use std::cell::RefCell;

        for grouping in ["'", "_", "."] {
            let xml = format!(
                "<localeSet><locale name=\"custom\"><numberSymbols><numberSymbol name=\"decimal\">,</numberSymbol><numberSymbol name=\"grouping\">{grouping}</numberSymbol></numberSymbols></locale></localeSet>"
            );
            let document = CxfaDocument::new();
            let stream = Rc::new(CfxReadOnlySpanStream::new(xml.as_bytes()));
            let parsed = Rc::new(RefCell::new(CfxXmlParser::new(stream).parse().unwrap()));
            let mut builder = CxfaDocumentBuilder::new(document.clone());
            assert!(builder.build_document(parsed, XfaPacketType::LocaleSet));
            let manager =
                CxfaLocaleMgr::with_locale_set(builder.get_root_node(), &[], Rc::new(|| 0));
            for (picture, prefix) in [
                ("num{z,zz9.99}", ""),
                ("num{'ID' z,zz9.99}", "ID "),
                ("num{'a''b' z,zz9.99}", "a'b "),
            ] {
                let input = format!("{prefix}1{grouping}234,50");
                let mut canonical = Vec::new();
                assert!(
                    CfgasStringFormatter::new(&wide(picture)).parse_num(
                        &manager,
                        &wide(&input),
                        &mut canonical
                    ),
                    "{picture} / {input}"
                );
                assert_eq!(canonical, wide("1234.50"), "{picture} / {input}");
            }
        }
    }
    #[test]
    fn NumParse() {
        for (locale, input, pattern, expected) in [
            ("en", "12.345e3", "99.999E", "12345.000000"),
            ("en", "12.345e+3", "99.999E", "12345.000000"),
            ("en", "12.345E-2", "99.999E", "0.123450"),
            ("en", "12e2", "99E", "1200.000"),
            ("en", "12e-2", "99E", "0.120"),
            ("en", "150", "z999", "150"),
            ("en", "150.50$", "zzz.zz$", "150.50"),
            ("en", "0150", "z999", "0150"),
            ("en", "123CR", "999cr", "-123"),
            ("en", "123", "999cr", "123"),
            ("en", "123CR", "999CR", "-123"),
            ("en", "123  ", "999CR", "123"),
            ("en", "123DB", "999db", "-123"),
            ("en", "123", "999db", "123"),
            ("en", "123DB", "999DB", "-123"),
            ("en", "123  ", "999DB", "123"),
            ("en", "123.5CR", "999.9cr", "-123.5"),
            ("en", "123.5", "999.9cr", "123.5"),
            ("en", "123.5CR", "999.9CR", "-123.5"),
            ("en", "123.5DB", "999.9db", "-123.5"),
            ("en", "123.5", "999.9db", "123.5"),
            ("en", "123.5DB", "999.9DB", "-123.5"),
            ("en", "10.50", "z,zz9.99", "10.50"),
            ("en", "3,125.00", "z,zz9.99", "3125.00"),
            ("en", "$1,234.00", "$z,zz9.99DB", "1234.00"),
            ("en", "1.234", "zz9.zzz", "1.234"),
            ("en", "1 text", "num{z 'text'}", "1"),
            ("en", "1.234 text", "z.zzz 'text'", "1.234"),
            ("en", "  1.234", "ZZ9.ZZZ", "1.234"),
            ("en", "12.345", "zz9.zzz", "12.345"),
            ("en", " 12.345", "ZZ9.ZZZ", "12.345"),
            ("en", "123.456", "zz9.zzz", "123.456"),
            ("en", "123.456", "ZZ9.ZZZ", "123.456"),
            ("en", "123.456-", "ZZ9.ZZZS", "-123.456"),
            ("en", "123.456+", "ZZ9.ZZZS", "123.456"),
            ("en", "123.456 ", "ZZ9.ZZZS", "123.456"),
            ("en", "123.456-", "ZZ9.ZZZS", "-123.456"),
            ("en", "123.456+", "ZZ9.ZZZS", "123.456"),
            ("en", "123", "zz9.zzz", "123"),
            ("en", "123.", "ZZ9.ZZZ", "123."),
            ("en", "123.", "zz9.zzz", "123."),
            ("en", "123.", "ZZ9.ZZZ", "123."),
            ("en", "123.0", "zz9.zzz", "123.0"),
            ("en", "123.0", "ZZ9.ZZZ", "123.0"),
            ("en", "123.000", "zz9.zzz", "123.000"),
            ("en", "123.000", "ZZ9.ZZZ", "123.000"),
            ("en", "12,345.67", "zzz,zz9.88888888", "12345.67"),
            ("en", "12,345.0000", "zzz,zz9.88888888", "12345.0000"),
            ("en", "12,345.6789", "zzz,zz9.8", "12345.6789"),
            ("en", "12,345.", "zzz,zz9.8", "12345."),
            ("en", "123,456.000", "zzz,zz9.8888", "123456.000"),
            ("en", "123,456.0", "zzz,zz9.8888", "123456.0"),
            ("en", "123,456", "zzz,zz9.8888", "123456"),
            ("en", "123,456", "ZZZ,ZZ9.88", "123456"),
            ("en", "12,345.67", "zzz,zz9.88888888", "12345.67"),
            ("en", "12,345.0000", "zzz,zz9.88888888", "12345.0000"),
            ("en", "12,345.6789", "zzz,zz9.8", "12345.6789"),
            ("en", "12,345.", "zzz,zz9.8", "12345."),
            ("en", "1,234.50%", "zzz,zz9.99%%", "12.345"),
            ("en", " 001.23", "S999V99", "001.23"),
            ("en", "  12.30", "SZZ9.99", "12.30"),
            ("en", "- 12.30", "SZ99.99", "-12.30"),
            ("en", "123.00", "szz9.99", "123.00"),
            ("en", "-123.00", "szz9.99", "-123.00"),
            ("en", "$1,234.00DB", "$z,zz9.99DB", "-1234.00"),
            ("en", "1 234", "num(fr){z,zzz}", "1234"),
            ("fr", "123 456", "zzz,zzz", "123456"),
            ("en", "12%", "zz%", "0.12"),
            ("en", "(123", "(zzz", "-123"),
            ("en", "123)", "zzz)", "-123"),
            ("en", "(123)", "(zzz)", "-123"),
            ("en", "123 ", "zzz)", "123"),
            ("en", " 123", "(zzz", "123"),
            ("en", " 123 ", "(zzz)", "123"),
            ("en", "123.5(", "zzz.z(", "-123.5"),
            ("en", "123.5)", "zzz.z)", "-123.5"),
            ("en", "123.5 ", "zzz.z)", "123.5"),
            ("en", "123.5 ", "zzz.z(", "123.5"),
            ("en", "123.545,4", "zzz.zzz,z", "123.5454"),
            ("en", "1", " num.().().}", "1"),
        ] {
            let mut output = Vec::new();
            let formatter = CfgasStringFormatter::new(&wide(pattern));
            assert!(
                formatter.parse_num(&manager(locale), &wide(input), &mut output),
                "{locale}: {input} / {pattern}"
            );
            assert_eq!(output, wide(expected), "{locale}: {input} / {pattern}");
        }
        for (locale, input, pattern) in [
            ("en", "..", "VC"),
            ("en", "PDF", "num( "),
            ("en", "9.E99999999999", "EdEE.E999"),
            ("en", "-3.E98998998 ", " 35EEEE.EE98"),
        ] {
            let mut output = Vec::new();
            let formatter = CfgasStringFormatter::new(&wide(pattern));
            assert!(
                !formatter.parse_num(&manager(locale), &wide(input), &mut output),
                "{locale}: {input} / {pattern}"
            );
        }
    }
    #[test]
    fn NumFormat() {
        for (locale, input, pattern, expected) in [
            ("en", "1.234", "zz9.zzz", "1.234"),
            ("en", "1", "num{z 'text'}", "1 text"),
            ("en", "1", "num{'text' z}", "text 1"),
            ("en", "1.234", "ZZ9.ZZZ", "  1.234"),
            ("en", "12.345", "zz9.zzz", "12.345"),
            ("en", "12.345", "ZZ9.ZZZ", " 12.345"),
            ("en", "123.456", "zz9.zzz", "123.456"),
            ("en", "123.456", "ZZ9.ZZZ", "123.456"),
            ("en", "123", "zz9.zzz", "123"),
            ("en", "123", "ZZ9.ZZZ", "123.000"),
            ("en", "123.", "zz9.zzz", "123."),
            ("en", "123.", "ZZ9.ZZZ", "123.000"),
            ("en", "123.0", "zz9.zzz", "123"),
            ("en", "123.0", "ZZ9.ZZZ", "123.000"),
            ("en", "123.000", "zz9.zzz", "123"),
            ("en", "123.000", "ZZ9.ZZZ", "123.000"),
            ("en", "123456", "zzz,zz9.8888", "123,456"),
            ("en", "123456", "ZZZ,ZZ9.88", "123,456"),
            ("en", "-1.23", "S999v99", "-00123"),
            ("en", "1.23", "S999V99", " 001.23"),
            ("en", "123", "S999V99", " 123.00"),
            ("en", "12.3", "SZZ9.99", "  12.30"),
            ("en", "-12.3", "SZ99.99", "- 12.30"),
            ("en", "123", "szz9.99", "123.00"),
            ("en", "-123", "szz9.99", "-123.00"),
            ("en", "-1234", "$z,zz9.99DB", "$1,234.00DB"),
            ("en", "12345", "99.999E", "12.345E+3"),
            ("en", "12345", "99999E", "12345E+0"),
            ("en", ".12345", "99.999E", "12.345E-2"),
            ("en", "12345", "99,999", "12,345"),
            ("en", "1234", "num(fr){z,zzz}", "1 234"),
            ("en", "12.34", "num.percent{}", "1,234%"),
            ("en", "12.34", "num(fr).percent{}", "1 234%"),
            ("en", "-123", "zzzCR", "123CR"),
            ("en", "123", "zzzCR", "123  "),
            ("en", "-123", "zzzcr", "123CR"),
            ("en", "123", "zzzcr", "123"),
            ("en", "123", "zzz$", "123$"),
            ("en", "-123.5", "zzz.zCR", "123.5CR"),
            ("en", "123.5", "zzz.zCR", "123.5  "),
            ("en", "-123.5", "zzz.zcr", "123.5CR"),
            ("en", "123.5", "zzz.zcr", "123.5"),
            ("en", "-123.5", "999.9db", "123.5db"),
            ("en", "123.5", "999.9db", "123.5"),
            ("en", "-123.5", "999.9DB", "123.5DB"),
            ("en", "123.5", "999.9DB", "123.5  "),
            ("en", "-123", "(zzz", "(123"),
            ("en", "-123", "(zzz)", "(123)"),
            ("en", "123", "zzz)", "123 "),
            ("en", "123", "(zzz", " 123"),
            ("en", "123", "(zzz)", " 123 "),
            ("en", "-123.5", "zzz.z(", "123.5("),
            ("en", "123.5", "zzz.z)", "123.5 "),
            ("en", "123.5", "zzz.z(", "123.5 "),
            ("en", "1", "r9", "r1"),
            ("en", "1", "R9", "R1"),
            ("en", "1", "b9", "b1"),
            ("en", "1", "B9", "B1"),
            ("en", "1", "9.c", "1"),
            ("en", "1", "9.C", "1"),
            ("en", "1", "9.d", "1"),
            ("en", "1", "9.D", "1"),
            ("en", "1", "E", "1"),
            ("en", "0", "E", "0"),
            ("en", "-1", "E", "-1"),
            (
                "en",
                "900000000000000000000",
                "E",
                "900,000,000,000,000,000,000",
            ),
            ("en", "1", "| num.().().", "1"),
            ("en", "1", "9.", "1"),
        ] {
            let mut output = Vec::new();
            let formatter = CfgasStringFormatter::new(&wide(pattern));
            assert!(
                formatter.format_num(&manager(locale), &wide(input), &mut output),
                "{locale}: {input} / {pattern}"
            );
            assert_eq!(output, wide(expected), "{locale}: {input} / {pattern}");
        }
        for (locale, input, pattern) in [("en", "1", "num.{E")] {
            let mut output = Vec::new();
            let formatter = CfgasStringFormatter::new(&wide(pattern));
            assert!(
                !formatter.format_num(&manager(locale), &wide(input), &mut output),
                "{locale}: {input} / {pattern}"
            );
        }
    }
    #[test]
    fn directional_literals_scaling_and_checked_exponents_are_connected() {
        let mgr = manager("en");
        for (pattern, input, expected) in [
            ("CR999", "-123", "CR123"),
            ("DB999", "-123", "DB123"),
            ("'tag' 999", "12", "tag 012"),
            ("-999", "12", "-012"),
            ("99.999E", "12345", "12.345E+3"),
            ("zzz.zz%", ".125", "12.5%"),
        ] {
            let formatter = CfgasStringFormatter::new(&wide(pattern));
            let mut output = Vec::new();
            assert!(formatter.format_num(&mgr, &wide(input), &mut output));
            assert_eq!(output, wide(expected), "{pattern} / {input}");
        }
        for (pattern, input, expected) in [("'tag' 999", "tag 012", "012"), (".99", ".12", ".12")] {
            let mut output = Vec::new();
            assert!(CfgasStringFormatter::new(&wide(pattern)).parse_num(
                &mgr,
                &wide(input),
                &mut output
            ));
            assert_eq!(output, wide(expected), "{pattern} / {input}");
        }
        for (pattern, input) in [
            ("99E", "12e99999999999"),
            ("99E", "12e1000"),
            ("99.9E", "12.3E99999999999"),
        ] {
            let mut output = Vec::new();
            assert!(!CfgasStringFormatter::new(&wide(pattern)).parse_num(
                &mgr,
                &wide(input),
                &mut output
            ));
        }
        assert!(!CfgasStringFormatter::new(&[]).parse_num(&mgr, &wide("12"), &mut Vec::new()));
        assert!(!CfgasStringFormatter::new(&wide("9")).format_num(&mgr, &[], &mut Vec::new()));
    }
}
