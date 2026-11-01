// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fxcrt/stream_write_utils.{h,cpp}`. Rust's shortest
//! round-trip float formatter stands in for PDFium's Dragonbox conversion.

use std::fmt::{self, Write};

use super::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};

pub fn float_to_decimal(mut value: f32) -> String {
    if value.is_nan() || value == 0.0 {
        return "0".to_owned();
    }
    if value == f32::INFINITY {
        value = f32::MAX;
    } else if value == f32::NEG_INFINITY {
        value = f32::MIN;
    }
    let decimal = value.to_string();
    if let Some(rest) = decimal.strip_prefix("-0.") {
        format!("-.{rest}")
    } else if let Some(rest) = decimal.strip_prefix("0.") {
        format!(".{rest}")
    } else {
        decimal
    }
}

pub fn write_float<W: Write>(stream: &mut W, value: f32) -> fmt::Result {
    stream.write_str(&float_to_decimal(value))
}

pub fn write_matrix<W: Write>(stream: &mut W, matrix: CfxMatrix) -> fmt::Result {
    write_float(stream, matrix.a)?;
    stream.write_char(' ')?;
    write_float(stream, matrix.b)?;
    stream.write_char(' ')?;
    write_float(stream, matrix.c)?;
    stream.write_char(' ')?;
    write_float(stream, matrix.d)?;
    stream.write_char(' ')?;
    write_float(stream, matrix.e)?;
    stream.write_char(' ')?;
    write_float(stream, matrix.f)
}

pub fn write_point<W: Write>(stream: &mut W, point: CfxPointF) -> fmt::Result {
    write_float(stream, point.x)?;
    stream.write_char(' ')?;
    write_float(stream, point.y)
}

pub fn write_rect<W: Write>(stream: &mut W, rect: CfxFloatRect) -> fmt::Result {
    write_float(stream, rect.left)?;
    stream.write_char(' ')?;
    write_float(stream, rect.bottom)?;
    stream.write_char(' ')?;
    write_float(stream, rect.width())?;
    stream.write_char(' ')?;
    write_float(stream, rect.height())
}

#[cfg(test)]
mod tests {
    use super::{float_to_decimal, write_matrix, write_point, write_rect};
    use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};

    #[test]
    fn pinned_float_format_examples() {
        let cases = [
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-1.0, "-1"),
            (0.5, ".5"),
            (-0.5, "-.5"),
            (0.00125, ".00125"),
            (123.45, "123.45"),
            (-7.5, "-7.5"),
            (38.895285, "38.895287"),
            (-77.037232, "-77.03723"),
            (f32::MAX, "340282350000000000000000000000000000000"),
            (
                f32::MIN_POSITIVE,
                ".000000000000000000000000000000000000011754944",
            ),
            (f32::INFINITY, "340282350000000000000000000000000000000"),
            (f32::NAN, "0"),
        ];
        for (value, expected) in cases {
            assert_eq!(float_to_decimal(value), expected, "{value:?}");
        }
        assert_eq!(
            float_to_decimal(-f32::MAX),
            "-340282350000000000000000000000000000000"
        );
        assert_eq!(
            float_to_decimal(-f32::MIN_POSITIVE),
            "-.000000000000000000000000000000000000011754944"
        );
    }

    #[test]
    fn geometry_writers_follow_pdfium_order() {
        let mut output = String::new();
        write_matrix(&mut output, CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 10.5, 20.25)).unwrap();
        assert_eq!(output, "1 0 0 1 10.5 20.25");
        output.clear();
        write_point(&mut output, CfxPointF::new(1.0, 2.5)).unwrap();
        assert_eq!(output, "1 2.5");
        output.clear();
        write_rect(&mut output, CfxFloatRect::new(1.0, 2.0, 10.0, 20.0)).unwrap();
        assert_eq!(output, "1 2 9 18");
    }
}

#[cfg(test)]
#[path = "stream_write_utils_unittest.rs"]
mod upstream_tests;
