// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fgas/crt/cfgas_decimal.{h,cpp}`. A signed, scaled 96-bit coefficient
//! uses the source's base-2^32 limbs and 192-bit multiplication/division work
//! arrays. All widening, carry, borrow and narrowing operations are safe Rust.
use std::cmp::Ordering;
use std::ops::{Div, Mul};

const SCALE_LIMIT: u8 = 28;
const WORD_MASK: u64 = u32::MAX as u64;

fn decimal_helper_div10_any(numbers: &mut [u64]) -> u8 {
    for i in (1..numbers.len()).rev() {
        numbers[i - 1] += (numbers[i] % 10) << 32;
        numbers[i] /= 10;
    }
    let Some(low) = numbers.first_mut() else {
        return 0;
    };
    let remainder = (*low % 10) as u8;
    *low /= 10;
    remainder
}
fn decimal_helper_mul10_any(numbers: &mut [u64]) {
    let Some(low) = numbers.first_mut() else {
        return;
    };
    *low = low.wrapping_mul(10);
    for i in 1..numbers.len() {
        numbers[i] = numbers[i]
            .wrapping_mul(10)
            .wrapping_add(numbers[i - 1] >> 32);
        numbers[i - 1] &= WORD_MASK;
    }
}
fn decimal_helper_normalize_any(numbers: &mut [u64]) {
    for i in 0..numbers.len().saturating_sub(1) {
        numbers[i + 1] = numbers[i + 1].wrapping_add(numbers[i] >> 32);
        numbers[i] &= WORD_MASK;
    }
}
fn decimal_helper_raw_compare_any(left: &[u64], right: &[u64]) -> Ordering {
    for i in (0..left.len().max(right.len())).rev() {
        let order = left.get(i).unwrap_or(&0).cmp(right.get(i).unwrap_or(&0));
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}
fn decimal_helper_dec_any(numbers: &mut [u64]) {
    for number in numbers {
        let original = *number;
        // A borrowed base-2^32 limb becomes 0xffffffff, not uint64_t::MAX.
        *number = number.wrapping_sub(1) & WORD_MASK;
        if original != 0 {
            return;
        }
    }
}
fn decimal_helper_inc_any(numbers: &mut [u64]) {
    for number in numbers {
        *number += 1;
        if *number <= WORD_MASK {
            return;
        }
        *number = 0;
    }
}
fn decimal_helper_raw_mul(left: &[u64], right: &[u64], result: &mut [u64]) {
    debug_assert!(left.len() + right.len() <= result.len());
    result.fill(0);
    for (i, &left) in left.iter().enumerate() {
        for (j, &right) in right.iter().enumerate() {
            let product = left * right;
            result[i + j] += product & WORD_MASK;
            result[i + j + 1] += product >> 32;
        }
    }
    decimal_helper_normalize_any(result);
    for word in result {
        *word &= WORD_MASK;
    }
}
fn decimal_helper_raw_div(numerator: &[u64], divisor: &[u64], result: &mut [u64]) {
    result.fill(0);
    let length = numerator.len();
    debug_assert!(length + divisor.len() <= 16);
    let mut left = [0u64; 16];
    let mut right = [0u64; 16];
    right[..length].copy_from_slice(numerator);
    let mut product = [0u64; 16];
    while decimal_helper_raw_compare_any(&left[..length], &right[..length]) != Ordering::Greater {
        let mut current = [0u64; 16];
        for i in 0..length {
            current[i] = left[i] + right[i];
        }
        // Normalize addition before the source's high-to-low division by two.
        // Comparing noncanonical limbs can choose the wrong binary-search half.
        decimal_helper_normalize_any(&mut current[..=length]);
        for i in (0..=length).rev() {
            if i != 0 {
                current[i - 1] += (current[i] % 2) << 32;
            }
            current[i] /= 2;
        }
        decimal_helper_raw_mul(&current[..length], divisor, &mut product);
        match decimal_helper_raw_compare_any(&product, numerator) {
            Ordering::Less => {
                left = current;
                decimal_helper_inc_any(&mut left[..length]);
            }
            Ordering::Greater => {
                right = current;
                decimal_helper_dec_any(&mut right[..length]);
            }
            Ordering::Equal => {
                let count = result.len().min(length);
                result[..count].copy_from_slice(&current[..count]);
                return;
            }
        }
    }
    // The source returns the upper integer bound before decimal rounding.
    let count = result.len().min(length);
    result[..count].copy_from_slice(&left[..count]);
}
fn decimal_helper_out_of_range(numbers: &[u64], goal: usize) -> bool {
    numbers[goal..].iter().any(|&word| word != 0)
}
fn decimal_helper_shrink_into_range(numbers: &mut [u64], goal: usize, scale: &mut u8) {
    let mut round_up = false;
    while *scale != 0 && (*scale > SCALE_LIMIT || decimal_helper_out_of_range(numbers, goal)) {
        round_up = decimal_helper_div10_any(numbers) >= 5;
        *scale -= 1;
    }
    if round_up {
        decimal_helper_normalize_any(&mut numbers[..goal]);
        decimal_helper_inc_any(&mut numbers[..goal]);
    }
}
fn decimal_helper_truncate(numbers: &mut [u64; 3], scale: &mut u8, minimum_scale: u8) {
    while *scale > minimum_scale {
        let mut divided = *numbers;
        if decimal_helper_div10_any(&mut divided) != 0 {
            break;
        }
        *numbers = divided;
        *scale -= 1;
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CfgasDecimal {
    hi: u32,
    mid: u32,
    lo: u32,
    negative: bool,
    scale: u8,
}
impl CfgasDecimal {
    pub fn new() -> Self {
        Self::default()
    }
    fn from_parts(lo: u32, mid: u32, hi: u32, negative: bool, scale: u8) -> Self {
        Self {
            hi,
            mid,
            lo,
            negative: negative && (hi != 0 || mid != 0 || lo != 0),
            scale: if scale > SCALE_LIMIT { 0 } else { scale },
        }
    }
    pub fn from_u32(value: u32) -> Self {
        Self::from_parts(value, 0, 0, false, 0)
    }
    pub fn from_u64(value: u64) -> Self {
        Self::from_parts(value as u32, (value >> 32) as u32, 0, false, 0)
    }
    pub fn from_i32(value: i32) -> Self {
        Self::from_parts(value.unsigned_abs(), 0, 0, value < 0, 0)
    }
    /// Checked entry point for callers which require a representable integer
    /// magnitude. Decimal scale multiplication retains the source's 96-bit
    /// narrowing behavior; callers can reduce precision first when necessary.
    pub fn try_from_f32(value: f32, scale: u8) -> Option<Self> {
        (value.is_finite() && value.abs() < 2.0f32.powi(96)).then(|| Self::from_f32(value, scale))
    }
    pub fn from_f32(value: f32, scale: u8) -> Self {
        // Oversized scales otherwise make the source uint8 output-length loop
        // wrap indefinitely. Nonfinite floats have no decimal representation.
        let scale = scale.min(SCALE_LIMIT);
        if !value.is_finite() {
            return Self::from_parts(0, 0, 0, false, scale);
        }
        let mut fraction = value.abs();
        let divisor = 2.0f32.powf(64.0);
        // The finite guard above avoids undefined native NaN/infinity casts;
        // ordinary f32 extraction and the source's word narrowing follow below.
        let bottom = (fraction % divisor) as u64;
        let top = (fraction / divisor) as u64;
        let mut numbers = [bottom & WORD_MASK, bottom >> 32, top & WORD_MASK];
        fraction %= 1.0;
        for _ in 0..scale {
            decimal_helper_mul10_any(&mut numbers);
            fraction *= 10.0;
            numbers[0] += fraction as u64;
            fraction %= 1.0;
        }
        numbers[0] += fraction.round() as u64;
        decimal_helper_normalize_any(&mut numbers);
        Self::from_parts(
            numbers[0] as u32,
            numbers[1] as u32,
            numbers[2] as u32,
            value < 0.0,
            scale,
        )
    }
    pub fn from_wide_string(text: &[u32]) -> Self {
        let mut position = 0;
        while text.get(position) == Some(&u32::from(b' ')) {
            position += 1;
        }
        let negative = text.get(position) == Some(&u32::from(b'-'));
        if negative || text.get(position) == Some(&u32::from(b'+')) {
            position += 1;
        }
        let mut point_seen = false;
        let mut scale = 0;
        let mut numbers = [0u64; 3];
        while scale < SCALE_LIMIT {
            let Some(&character) = text.get(position) else {
                break;
            };
            if character == u32::from(b'.') {
                point_seen = true;
                position += 1;
                continue;
            }
            if !(48..=57).contains(&character) {
                break;
            }
            decimal_helper_mul10_any(&mut numbers);
            numbers[0] += u64::from(character - 48);
            // The source lost a carry when adding the final digit crossed a
            // limb boundary (e.g. 4294967296); normalize before narrowing.
            decimal_helper_normalize_any(&mut numbers);
            numbers[2] &= WORD_MASK;
            if point_seen {
                scale += 1;
            }
            position += 1;
        }
        Self::from_parts(
            numbers[0] as u32,
            numbers[1] as u32,
            numbers[2] as u32,
            negative,
            scale,
        )
    }
    fn words(&self) -> [u64; 3] {
        [u64::from(self.lo), u64::from(self.mid), u64::from(self.hi)]
    }
    pub fn is_not_zero(&self) -> bool {
        self.hi != 0 || self.mid != 0 || self.lo != 0
    }
    pub fn get_scale(&self) -> u8 {
        self.scale
    }
    pub fn to_wide_string(&self) -> Vec<u32> {
        let mut numbers = self.words();
        let mut reversed = Vec::with_capacity(30);
        while numbers.iter().any(|&word| word != 0) {
            reversed.push(u32::from(decimal_helper_div10_any(&mut numbers)) + 48);
        }
        while reversed.len() <= usize::from(self.scale) {
            reversed.push(48);
        }
        let mut result = Vec::with_capacity(reversed.len() + 2);
        if self.negative && self.is_not_zero() {
            result.push(45);
        }
        for index in (0..reversed.len()).rev() {
            result.push(reversed[index]);
            if self.scale != 0 && index == usize::from(self.scale) {
                result.push(46);
            }
        }
        result
    }
    pub fn to_float(&self) -> f32 {
        self.to_double() as f32
    }
    pub fn to_double(&self) -> f64 {
        let power = 4_294_967_296.0;
        let base =
            f64::from(self.hi) * power * power + f64::from(self.mid) * power + f64::from(self.lo);
        // Preserve the source's f32 power and final f64 multiplication.
        (if self.negative { -base } else { base }) * f64::from(10.0f32.powf(-f32::from(self.scale)))
    }
    pub fn set_scale(&mut self, new_scale: u8) {
        let new_scale = new_scale.min(SCALE_LIMIT);
        let old_scale = self.scale;
        if old_scale == new_scale {
            return;
        }
        let mut numbers = self.words();
        if new_scale > old_scale {
            for _ in 0..new_scale - old_scale {
                decimal_helper_mul10_any(&mut numbers);
            }
        } else {
            let mut half = [5u64, 0, 0];
            for _ in 0..old_scale - new_scale - 1 {
                decimal_helper_mul10_any(&mut half);
            }
            for i in 0..3 {
                numbers[i] += half[i];
            }
            decimal_helper_normalize_any(&mut numbers);
            for _ in 0..old_scale - new_scale {
                decimal_helper_div10_any(&mut numbers);
            }
        }
        *self = Self::from_parts(
            numbers[0] as u32,
            numbers[1] as u32,
            numbers[2] as u32,
            self.negative,
            new_scale,
        );
    }
    pub fn set_negate(&mut self) {
        if self.is_not_zero() {
            self.negative = !self.negative;
        }
    }
    fn multiply(&self, right: &Self) -> Self {
        let mut product = [0u64; 6];
        decimal_helper_raw_mul(&self.words(), &right.words(), &mut product);
        let mut scale = self.scale + right.scale;
        decimal_helper_shrink_into_range(&mut product, 3, &mut scale);
        Self::from_parts(
            product[0] as u32,
            product[1] as u32,
            product[2] as u32,
            self.negative ^ right.negative,
            scale,
        )
    }
    fn divide(&self, right: &Self) -> Self {
        if !right.is_not_zero() {
            return Self::default();
        }
        let mut numerator = [0u64; 7];
        numerator[..3].copy_from_slice(&self.words());
        let mut scale = 0;
        if self.scale < right.scale {
            for _ in 0..right.scale - self.scale {
                decimal_helper_mul10_any(&mut numerator);
            }
        } else {
            scale = self.scale - right.scale;
        }
        let minimum_scale = scale;
        if !self.is_not_zero() {
            return Self::from_parts(0, 0, 0, false, minimum_scale);
        }
        while numerator[6] == 0 {
            decimal_helper_mul10_any(&mut numerator);
            scale += 1;
        }
        decimal_helper_div10_any(&mut numerator);
        scale -= 1;
        let mut quotient = [0u64; 7];
        decimal_helper_raw_div(&numerator[..6], &right.words(), &mut quotient);
        decimal_helper_shrink_into_range(&mut quotient[..6], 3, &mut scale);
        let mut words = [quotient[0], quotient[1], quotient[2]];
        decimal_helper_truncate(&mut words, &mut scale, minimum_scale);
        Self::from_parts(
            words[0] as u32,
            words[1] as u32,
            words[2] as u32,
            self.negative ^ right.negative,
            scale,
        )
    }
}
impl From<u32> for CfgasDecimal {
    fn from(value: u32) -> Self {
        Self::from_u32(value)
    }
}
impl From<u64> for CfgasDecimal {
    fn from(value: u64) -> Self {
        Self::from_u64(value)
    }
}
impl From<i32> for CfgasDecimal {
    fn from(value: i32) -> Self {
        Self::from_i32(value)
    }
}
impl Mul for CfgasDecimal {
    type Output = Self;
    fn mul(self, right: Self) -> Self {
        self.multiply(&right)
    }
}
impl Div for CfgasDecimal {
    type Output = Self;
    fn div(self, right: Self) -> Self {
        self.divide(&right)
    }
}
impl Mul<&CfgasDecimal> for &CfgasDecimal {
    type Output = CfgasDecimal;
    fn mul(self, right: &CfgasDecimal) -> CfgasDecimal {
        self.multiply(right)
    }
}
impl Div<&CfgasDecimal> for &CfgasDecimal {
    type Output = CfgasDecimal;
    fn div(self, right: &CfgasDecimal) -> CfgasDecimal {
        self.divide(right)
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn decimal(text: &str) -> CfgasDecimal {
        CfgasDecimal::from_wide_string(&wide(text))
    }
    fn assert_prefix(actual: Vec<u32>, expected: &str) {
        let expected = wide(expected);
        assert_eq!(actual.len(), expected.len());
        assert_eq!(&actual[..8], &expected[..8]);
    }
    // All eight cases from `xfa/fgas/crt/cfgas_decimal_unittest.cpp`.
    #[test]
    fn Empty() {
        let empty = CfgasDecimal::default();
        assert_eq!(empty.to_wide_string(), wide("0"));
        assert_eq!(empty.to_float(), 0.0f32);
        assert_eq!(empty.to_double(), 0.0);
    }
    #[test]
    fn FromInt32() {
        assert_eq!(
            CfgasDecimal::from_i32(i32::MAX).to_wide_string(),
            wide("2147483647")
        );
        assert_eq!(
            CfgasDecimal::from_i32(i32::MIN).to_wide_string(),
            wide("-2147483648")
        );
    }
    #[test]
    fn FromUint32() {
        assert_eq!(
            CfgasDecimal::from_u32(u32::MAX).to_wide_string(),
            wide("4294967295")
        );
        assert_eq!(CfgasDecimal::from_u32(u32::MIN).to_wide_string(), wide("0"));
    }
    #[test]
    fn FromUint64() {
        assert_eq!(
            CfgasDecimal::from_u64(u64::MAX).to_wide_string(),
            wide("18446744073709551615")
        );
        assert_eq!(CfgasDecimal::from_u64(u64::MIN).to_wide_string(), wide("0"));
    }
    #[test]
    fn FromFloat() {
        assert_prefix(
            CfgasDecimal::from_f32(2.0f32.powf(95.0), 0).to_wide_string(),
            "39614081257132168796771975168",
        );
        assert_prefix(
            CfgasDecimal::from_f32(1e20f32, 0).to_wide_string(),
            "100000000000000000000",
        );
        assert_prefix(
            CfgasDecimal::from_f32(1e14f32, 4).to_wide_string(),
            "100000000000000.0000",
        );
    }
    #[test]
    fn FromFloatFractional() {
        assert_prefix(
            CfgasDecimal::from_f32(123.456f32, 10).to_wide_string(),
            "123.4560000000",
        );
    }
    #[test]
    fn FromString() {
        assert_eq!(
            decimal("100000000000000000000000000").to_wide_string(),
            wide("100000000000000000000000000")
        );
        assert_eq!(
            decimal("-1000000000000000000000000").to_wide_string(),
            wide("-1000000000000000000000000")
        );
    }
    #[test]
    fn FromString28Digits() {
        assert_eq!(
            decimal("32109876543210.0123456890123").to_wide_string(),
            wide("32109876543210.0123456890123")
        );
    }
    #[test]
    fn carries_rounding_zero_and_scale_limits_are_defined() {
        for text in [
            "4294967296",
            "18446744073709551616",
            "79228162514264337593543950335",
        ] {
            assert_eq!(decimal(text).to_wide_string(), wide(text));
        }
        assert_eq!(
            decimal("79228162514264337593543950336").to_wide_string(),
            wide("0")
        );
        for (text, scale, expected) in [
            ("1.25", 1, "1.3"),
            ("-1.25", 1, "-1.3"),
            ("9.995", 2, "10.00"),
            ("1844674407370955161.65", 1, "1844674407370955161.7"),
            ("0.004", 2, "0.00"),
            ("-0.004", 2, "0.00"),
            ("1.2", 4, "1.2000"),
        ] {
            let mut value = decimal(text);
            value.set_scale(scale);
            assert_eq!(value.to_wide_string(), wide(expected), "{text}/{scale}");
        }
        let mut zero = decimal("-0.000");
        zero.set_negate();
        assert_eq!(zero.to_wide_string(), wide("0.000"));
        assert!(!zero.is_not_zero());
        let mut value = CfgasDecimal::from_u32(1);
        value.set_scale(u8::MAX);
        assert_eq!(value.get_scale(), SCALE_LIMIT);
        assert_eq!(
            value.to_wide_string(),
            wide("1.0000000000000000000000000000")
        );
        value.set_negate();
        assert_eq!(
            value.to_wide_string(),
            wide("-1.0000000000000000000000000000")
        );
        for invalid in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            assert!(CfgasDecimal::try_from_f32(invalid, 2).is_none());
            assert_eq!(
                CfgasDecimal::from_f32(invalid, 2).to_wide_string(),
                wide("0.00")
            );
        }
        assert!(CfgasDecimal::try_from_f32(f32::MAX, 0).is_none());
        assert!(CfgasDecimal::from_f32(f32::MAX, 0).to_wide_string().len() <= 29);
    }
    #[test]
    fn multiplication_and_division_keep_decimal_rounding_and_precision() {
        for (left, right, expected) in [
            ("1.25", "2", "2.50"),
            ("-1.25", "-2.0", "2.500"),
            (
                "123456789012345.67890123456789",
                "2.0",
                "246913578024691.35780246913578",
            ),
            (
                "0.1234567890123456789012345678",
                "1.0",
                "0.1234567890123456789012345678",
            ),
        ] {
            assert_eq!(
                (decimal(left) * decimal(right)).to_wide_string(),
                wide(expected)
            );
        }
        for (left, right, expected) in [
            ("10", "4", "2.5"),
            ("-1.25", ".5", "-2.5"),
            ("1", "3", "0.3333333333333333333333333333"),
            ("2", "3", "0.6666666666666666666666666667"),
            ("100000000000000000000", "10", "10000000000000000000"),
            ("0.00", "2", "0.00"),
            ("12.5", "0", "0"),
        ] {
            assert_eq!(
                (decimal(left) / decimal(right)).to_wide_string(),
                wide(expected),
                "{left}/{right}"
            );
        }
        // Exercise low-word borrow/carry boundaries in the source binary search.
        for value in [u32::MAX as u64, u64::from(u32::MAX) + 1, u64::MAX] {
            let number = CfgasDecimal::from_u64(value);
            assert_eq!(
                (number / CfgasDecimal::from_u32(1)).to_wide_string(),
                number.to_wide_string()
            );
            assert_eq!((number / number).to_wide_string(), wide("1"));
        }
    }
}
