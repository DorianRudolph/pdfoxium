// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2022 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fxcrt/fx_string_wrappers.h`: Rust String replaces the allocator
//! aliases. This helper supplies the default stream's six-significant-digit
//! float formatting used by annotation appearance generation.

use std::fmt::{self, Write};

pub fn write_default_float(stream: &mut impl Write, value: f32) -> fmt::Result {
    if value.is_nan() {
        return stream.write_str(if value.is_sign_negative() {
            "-nan"
        } else {
            "nan"
        });
    }
    if value.is_infinite() {
        return stream.write_str(if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        });
    }
    // Round first: rounding may change the exponent and therefore the choice
    // between fixed and scientific notation in defaultfloat.
    let scientific = format!("{:.5e}", f64::from(value));
    let (mantissa, exponent) = scientific.split_once('e').expect("exponent");
    let exponent: i32 = exponent.parse().expect("numeric exponent");
    if !(-4..6).contains(&exponent) {
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        return write!(stream, "{mantissa}e{exponent:+03}");
    }
    let (sign, mantissa) = mantissa
        .strip_prefix('-')
        .map_or(("", mantissa), |rest| ("-", rest));
    let digits: String = mantissa.chars().filter(|&ch| ch != '.').collect();
    let decimal_pos = exponent + 1;
    let mut fixed = String::new();
    fixed.push_str(sign);
    if decimal_pos <= 0 {
        fixed.push_str("0.");
        for _ in 0..-decimal_pos {
            fixed.push('0');
        }
        fixed.push_str(&digits);
    } else {
        let position = decimal_pos as usize;
        fixed.push_str(&digits[..position]);
        if position < digits.len() {
            fixed.push('.');
            fixed.push_str(&digits[position..]);
        }
    }
    if fixed.contains('.') {
        stream.write_str(fixed.trim_end_matches('0').trim_end_matches('.'))
    } else {
        stream.write_str(&fixed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_float_precision_exponent_boundaries_and_signed_zero() {
        for (value, expected) in [
            (1.2345678, "1.23457"),
            (123456.0, "123456"),
            (1_000_000.0, "1e+06"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (999999.9, "1e+06"),
            (0.0, "0"),
            (-0.0, "-0"),
        ] {
            let mut result = String::new();
            write_default_float(&mut result, value).unwrap();
            assert_eq!(result, expected);
        }
    }
}

#[cfg(test)]
#[path = "fx_string_wrappers_unittest.rs"]
mod upstream_tests;
