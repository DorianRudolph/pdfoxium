// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/stream_write_utils_unittest.cpp`.

#![allow(non_snake_case, clippy::excessive_precision)]

use super::*;

fn WriteFloatToString(value: f32) -> String {
    let mut stream = String::new();
    write_float(&mut stream, value).unwrap();
    stream
}

#[test]
fn WriteFloat() {
    assert_eq!("0", WriteFloatToString(0.0f32));
    assert_eq!("0", WriteFloatToString(-0.0f32));
    assert_eq!("1", WriteFloatToString(1.0f32));
    assert_eq!("-1", WriteFloatToString(-1.0f32));
    assert_eq!(".5", WriteFloatToString(0.5f32));
    assert_eq!("-.5", WriteFloatToString(-0.5f32));
    assert_eq!(".00125", WriteFloatToString(0.00125f32));
    assert_eq!("123.45", WriteFloatToString(123.45f32));
    assert_eq!("-7.5", WriteFloatToString(-7.5f32));
    assert_eq!("38.895287", WriteFloatToString(38.895285f32));
    assert_eq!("-77.03723", WriteFloatToString(-77.037232f32));
    assert_eq!(
        "340282350000000000000000000000000000000",
        WriteFloatToString(f32::MAX)
    );
    assert_eq!(
        "-340282350000000000000000000000000000000",
        WriteFloatToString(-f32::MAX)
    );
    assert_eq!(
        ".000000000000000000000000000000000000011754944",
        WriteFloatToString(f32::MIN_POSITIVE)
    );
    assert_eq!(
        "-.000000000000000000000000000000000000011754944",
        WriteFloatToString(-f32::MIN_POSITIVE)
    );
    assert_eq!(
        "340282350000000000000000000000000000000",
        WriteFloatToString(f32::INFINITY)
    );
    assert_eq!(
        "-340282350000000000000000000000000000000",
        WriteFloatToString(-f32::INFINITY)
    );
    assert_eq!("0", WriteFloatToString(f32::NAN));
}

#[test]
fn WriteMatrix() {
    let mut stream = String::new();
    write_matrix(&mut stream, CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 10.5, 20.25)).unwrap();
    assert_eq!("1 0 0 1 10.5 20.25", stream);
}

#[test]
fn WritePoint() {
    let mut stream = String::new();
    write_point(&mut stream, CfxPointF::new(1.0, 2.5)).unwrap();
    assert_eq!("1 2.5", stream);
}

#[test]
fn WriteRect() {
    let mut stream = String::new();
    write_rect(&mut stream, CfxFloatRect::new(1.0, 2.0, 10.0, 20.0)).unwrap();
    assert_eq!("1 2 9 18", stream);
}
