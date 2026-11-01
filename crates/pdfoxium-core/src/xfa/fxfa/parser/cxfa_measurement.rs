// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Direct `xfa/fxfa/parser/cxfa_measurement.{h,cpp}`.
use crate::fxcrt::fx_extension::wcstof;
use crate::xfa::fxfa::fxfa_basic::XfaUnit;

const PT_TO_INCH: f32 = 72.0;
const PT_TO_CM: f32 = PT_TO_INCH / 2.54;
const PT_TO_MM: f32 = PT_TO_CM / 10.0;
const PT_TO_MP: f32 = 0.001;
const PT_TO_PC: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CxfaMeasurement {
    value: f32,
    unit: XfaUnit,
}
impl Default for CxfaMeasurement {
    fn default() -> Self {
        Self::new(-1.0, XfaUnit::Unknown)
    }
}
impl CxfaMeasurement {
    pub const fn new(value: f32, unit: XfaUnit) -> Self {
        Self { value, unit }
    }
    pub fn from_string(text: &[u32]) -> Self {
        let text = text.strip_prefix(&[u32::from(b'=')]).unwrap_or(text);
        if text.is_empty() {
            return Self::new(0.0, XfaUnit::Unknown);
        }
        let (value, used) = wcstof(text);
        Self::new(
            if value.is_finite() { value } else { 0.0 },
            Self::get_unit_from_string(&text[used..]),
        )
    }
    pub fn set(&mut self, value: f32, unit: XfaUnit) {
        self.value = value;
        self.unit = unit;
    }
    pub const fn get_unit(&self) -> XfaUnit {
        self.unit
    }
    pub const fn get_value(&self) -> f32 {
        self.value
    }
    pub fn get_unit_from_string(text: &[u32]) -> XfaUnit {
        match text {
            [109, 109] => XfaUnit::Mm,
            [112, 116] => XfaUnit::Pt,
            [105, 110] => XfaUnit::In,
            [99, 109] => XfaUnit::Cm,
            [112, 99] => XfaUnit::Pc,
            [109, 112] => XfaUnit::Mp,
            [101, 109] => XfaUnit::Em,
            [37] => XfaUnit::Percent,
            _ => XfaUnit::Unknown,
        }
    }
    pub fn to_string(&self) -> Vec<u32> {
        let mut result = format_measurement_value(self.value);
        result.push_str(match self.unit {
            XfaUnit::Mm => "mm",
            XfaUnit::Pt => "pt",
            XfaUnit::In => "in",
            XfaUnit::Cm => "cm",
            XfaUnit::Mp => "mp",
            XfaUnit::Pc => "pc",
            XfaUnit::Em => "em",
            XfaUnit::Percent => "%",
            XfaUnit::Unknown => "",
        });
        result.chars().map(u32::from).collect()
    }
    pub fn to_unit(&self, unit: XfaUnit) -> f32 {
        if self.unit == unit {
            return self.value;
        }
        // Keep both source operations, with f32 rounding at each step.
        let mut value = self.value;
        match self.unit {
            XfaUnit::Pt => (),
            XfaUnit::Mm => value *= PT_TO_MM,
            XfaUnit::In => value *= PT_TO_INCH,
            XfaUnit::Cm => value *= PT_TO_CM,
            XfaUnit::Mp => value *= PT_TO_MP,
            XfaUnit::Pc => value *= PT_TO_PC,
            _ => return 0.0,
        }
        match unit {
            XfaUnit::Pt => value,
            XfaUnit::Mm => value / PT_TO_MM,
            XfaUnit::In => value / PT_TO_INCH,
            XfaUnit::Cm => value / PT_TO_CM,
            XfaUnit::Mp => value / PT_TO_MP,
            XfaUnit::Pc => value / PT_TO_PC,
            // Source treats these destinations as unreachable. An unsupported
            // conversion has the same zero result as an unsupported source.
            _ => 0.0,
        }
    }
}

// WideString::Format("%.8g", value): f32 is promoted to double, rounded to
// eight significant digits, and uses scientific notation outside [-4, 8).
// As with fxcrt::fx_string_wrappers, Rust performs the actual decimal rounding.
fn format_measurement_value(value: f32) -> String {
    if !value.is_finite() {
        return match (value.is_nan(), value.is_sign_negative()) {
            (true, false) => "nan",
            (true, true) => "-nan",
            (false, false) => "inf",
            (false, true) => "-inf",
        }
        .to_owned();
    }
    let scientific = format!("{:.7e}", f64::from(value));
    let (mantissa, exponent) = scientific.split_once('e').expect("exponent");
    let exponent: i32 = exponent.parse().expect("numeric exponent");
    if !(-4..8).contains(&exponent) {
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        return format!("{mantissa}e{exponent:+03}");
    }
    let (sign, mantissa) = mantissa
        .strip_prefix('-')
        .map_or(("", mantissa), |rest| ("-", rest));
    let digits: String = mantissa.chars().filter(|&ch| ch != '.').collect();
    let decimal_pos = exponent + 1;
    let mut result = String::from(sign);
    if decimal_pos <= 0 {
        result.push_str("0.");
        for _ in 0..-decimal_pos {
            result.push('0');
        }
        result.push_str(&digits);
    } else {
        let position = decimal_pos as usize;
        result.push_str(&digits[..position]);
        if position < digits.len() {
            result.push('.');
            result.push_str(&digits[position..]);
        }
    }
    if result.contains('.') {
        result.truncate(result.trim_end_matches('0').trim_end_matches('.').len());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::assert_float_eq;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    #[test]
    fn to_string() {
        let mut measurement = CxfaMeasurement::default();
        for (value, unit, expected) in [
            (0.1, XfaUnit::Percent, "0.1%"),
            (1.0, XfaUnit::Em, "1em"),
            (1.1, XfaUnit::Pt, "1.1pt"),
            (1.0000001, XfaUnit::In, "1.0000001in"),
            (1234.0, XfaUnit::Pc, "1234pc"),
            (987654321.0123456789_f32, XfaUnit::Cm, "9.8765434e+08cm"),
            (0.0, XfaUnit::Mm, "0mm"),
            (-2.0, XfaUnit::Mp, "-2mp"),
        ] {
            measurement.set(value, unit);
            assert_eq!(measurement.to_string(), wide(expected));
        }
    }
    #[test]
    fn get_unit_from_string() {
        for (text, unit) in [
            ("%", XfaUnit::Percent),
            ("em", XfaUnit::Em),
            ("pt", XfaUnit::Pt),
            ("in", XfaUnit::In),
            ("pc", XfaUnit::Pc),
            ("cm", XfaUnit::Cm),
            ("mm", XfaUnit::Mm),
            ("mp", XfaUnit::Mp),
        ] {
            assert_eq!(CxfaMeasurement::get_unit_from_string(&wide(text)), unit);
        }
        for text in ["", "foo", "!", "CM", "Cm", "cM"] {
            assert_eq!(
                CxfaMeasurement::get_unit_from_string(&wide(text)),
                XfaUnit::Unknown
            );
        }
    }
    fn check_measurement(text: &str, unit: XfaUnit, value: f32) {
        let measurement = CxfaMeasurement::from_string(&wide(text));
        assert_eq!(measurement.get_unit(), unit);
        assert_float_eq(value, measurement.get_value());
    }
    #[test]
    fn equals_prefix() {
        check_measurement("=5", XfaUnit::Unknown, 5.0);
        check_measurement("=5mm", XfaUnit::Mm, 5.0);
    }
    #[test]
    fn no_prefix() {
        check_measurement("5", XfaUnit::Unknown, 5.0);
        check_measurement("5mm", XfaUnit::Mm, 5.0);
    }
    #[test]
    fn invalid_values() {
        check_measurement("", XfaUnit::Unknown, 0.0);
        check_measurement("=", XfaUnit::Unknown, 0.0);
    }
    #[test]
    fn unit_conversions_and_numeric_boundaries() {
        assert_eq!(
            CxfaMeasurement::default(),
            CxfaMeasurement::new(-1.0, XfaUnit::Unknown)
        );
        for (value, unit) in [
            (72.0, XfaUnit::Pt),
            (1.0, XfaUnit::In),
            (2.54, XfaUnit::Cm),
            (25.4, XfaUnit::Mm),
            (72000.0, XfaUnit::Mp),
            (6.0, XfaUnit::Pc),
        ] {
            let measurement = CxfaMeasurement::new(value, unit);
            assert_float_eq(72.0, measurement.to_unit(XfaUnit::Pt));
            assert_float_eq(value, CxfaMeasurement::new(72.0, XfaUnit::Pt).to_unit(unit));
        }
        for unit in [XfaUnit::Unknown, XfaUnit::Em, XfaUnit::Percent] {
            let measurement = CxfaMeasurement::new(5.0, unit);
            assert_eq!(measurement.to_unit(unit), 5.0);
            assert_eq!(measurement.to_unit(XfaUnit::Pt), 0.0);
            assert_eq!(CxfaMeasurement::new(5.0, XfaUnit::Pt).to_unit(unit), 0.0);
        }
        for text in ["infmm", "-infinitymm", "nan(payload)mm", "1e100mm"] {
            check_measurement(text, XfaUnit::Mm, 0.0);
        }
        check_measurement("= +1.25e2cm", XfaUnit::Cm, 125.0);
        check_measurement("5 mm", XfaUnit::Unknown, 5.0);
        check_measurement("mm", XfaUnit::Mm, 0.0);
        for (value, expected) in [
            (1.0e-5, "9.9999997e-06"),
            (1.0e-4, "9.9999997e-05"),
            (1.0e8, "1e+08"),
            (-0.0, "-0"),
        ] {
            assert_eq!(
                CxfaMeasurement::new(value, XfaUnit::Unknown).to_string(),
                wide(expected)
            );
        }
    }
}
