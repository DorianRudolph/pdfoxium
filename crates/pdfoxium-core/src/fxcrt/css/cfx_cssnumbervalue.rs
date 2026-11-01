// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Scaled numbers from `cfx_cssnumbervalue.{h,cpp}`.
//! This stored value replaces the C++ numeric subclass without a virtual cast.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CfxCssNumberUnit {
    Number,
    Percent,
    EMS,
    EXS,
    Pixels,
    CentiMeters,
    MilliMeters,
    Inches,
    Points,
    Picas,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CfxCssNumber {
    pub unit: CfxCssNumberUnit,
    pub value: f32,
}
impl CfxCssNumber {
    pub fn new(unit: CfxCssNumberUnit, value: f32) -> Self {
        Self {
            unit,
            value: if unit == CfxCssNumberUnit::Number && value.abs() < 0.001 {
                0.0
            } else {
                value
            },
        }
    }
    pub fn apply(&self, percent_base: f32) -> f32 {
        use CfxCssNumberUnit::*;
        match self.unit {
            Pixels | Number => self.value * 72.0 / 96.0,
            EMS | EXS => self.value * percent_base,
            Percent => self.value * percent_base / 100.0,
            CentiMeters => self.value * 28.3464,
            MilliMeters => self.value * 2.8346,
            Inches => self.value * 72.0,
            // A pica is twelve points. Source division reverses the units.
            Picas => self.value * 12.0,
            Points => self.value,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn number_normalization_and_all_source_unit_conversions() {
        use CfxCssNumberUnit::*;
        for value in [-0.0009, 0.0, 0.0009] {
            assert_eq!(CfxCssNumber::new(Number, value).value, 0.0);
            assert_eq!(CfxCssNumber::new(Points, value).value, value);
        }
        assert_eq!(CfxCssNumber::new(Number, 0.001).value, 0.001);
        for (unit, value, base, expected) in [
            (Number, 96.0, 10.0, 72.0),
            (Pixels, 96.0, 10.0, 72.0),
            (EMS, 2.0, 12.0, 24.0),
            (EXS, 2.0, 12.0, 24.0),
            (Percent, 25.0, 12.0, 3.0),
            (CentiMeters, 1.0, 0.0, 28.3464),
            (MilliMeters, 1.0, 0.0, 2.8346),
            (Inches, 1.0, 0.0, 72.0),
            (Points, 12.0, 0.0, 12.0),
            (Picas, 6.0, 0.0, 72.0),
        ] {
            assert_eq!(
                CfxCssNumber::new(unit, value).apply(base),
                expected,
                "{unit:?}"
            );
        }
    }
}
