// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/page/cpdf_devicecs_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CpdfDeviceCs;
use crate::fpdfapi::page::cpdf_colorspace::{ColorSpaceFamily, CpdfColorSpace};
use crate::test_support::assert_float_eq;

#[test]
fn GetRGBFromGray() {
    let device_gray = CpdfDeviceCs::new(ColorSpaceFamily::DeviceGray);

    // Test normal values. For gray, only first value from buf should be used.
    let mut buf = [0.43, 0.11, 0.34];
    let mut maybe_rgb = device_gray.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(0.43, maybe_rgb.unwrap().red);
    assert_float_eq(0.43, maybe_rgb.unwrap().green);
    assert_float_eq(0.43, maybe_rgb.unwrap().blue);

    buf[0] = 0.872;
    maybe_rgb = device_gray.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(0.872, maybe_rgb.unwrap().red);
    assert_float_eq(0.872, maybe_rgb.unwrap().green);
    assert_float_eq(0.872, maybe_rgb.unwrap().blue);

    // Test boundary values
    buf[0] = 0.0;
    maybe_rgb = device_gray.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(0.0, maybe_rgb.unwrap().red);
    assert_float_eq(0.0, maybe_rgb.unwrap().green);
    assert_float_eq(0.0, maybe_rgb.unwrap().blue);

    buf[0] = 1.0;
    maybe_rgb = device_gray.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(1.0, maybe_rgb.unwrap().red);
    assert_float_eq(1.0, maybe_rgb.unwrap().green);
    assert_float_eq(1.0, maybe_rgb.unwrap().blue);

    // Test out of range values
    buf[0] = -0.01;
    maybe_rgb = device_gray.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(0.0, maybe_rgb.unwrap().red);
    assert_float_eq(0.0, maybe_rgb.unwrap().green);
    assert_float_eq(0.0, maybe_rgb.unwrap().blue);

    buf[0] = 12.5;
    maybe_rgb = device_gray.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(1.0, maybe_rgb.unwrap().red);
    assert_float_eq(1.0, maybe_rgb.unwrap().green);
    assert_float_eq(1.0, maybe_rgb.unwrap().blue);
}

#[test]
fn GetRGBFromRGB() {
    let device_rgb = CpdfDeviceCs::new(ColorSpaceFamily::DeviceRgb);

    // Test normal values
    let mut buf = [0.13, 1.0, 0.652];
    let mut maybe_rgb = device_rgb.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(0.13, maybe_rgb.unwrap().red);
    assert_float_eq(1.0, maybe_rgb.unwrap().green);
    assert_float_eq(0.652, maybe_rgb.unwrap().blue);

    buf[0] = 0.0;
    buf[1] = 0.52;
    buf[2] = 0.78;
    maybe_rgb = device_rgb.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(0.0, maybe_rgb.unwrap().red);
    assert_float_eq(0.52, maybe_rgb.unwrap().green);
    assert_float_eq(0.78, maybe_rgb.unwrap().blue);

    // Test out of range values
    buf[0] = -10.5;
    buf[1] = 100.0;
    maybe_rgb = device_rgb.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(0.0, maybe_rgb.unwrap().red);
    assert_float_eq(1.0, maybe_rgb.unwrap().green);
    assert_float_eq(0.78, maybe_rgb.unwrap().blue);
}

#[test]
fn GetRGBFromCMYK() {
    let device_cmyk = CpdfDeviceCs::new(ColorSpaceFamily::DeviceCmyk);

    // Test normal values
    let mut buf = [0.6, 0.5, 0.3, 0.9];
    let mut maybe_rgb = device_cmyk.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(0.0627451, maybe_rgb.unwrap().red);
    assert_float_eq(0.0627451, maybe_rgb.unwrap().green);
    assert_float_eq(0.10588236, maybe_rgb.unwrap().blue);

    buf[0] = 0.15;
    buf[2] = 0.0;
    maybe_rgb = device_cmyk.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(0.2, maybe_rgb.unwrap().red);
    assert_float_eq(0.0862745, maybe_rgb.unwrap().green);
    assert_float_eq(0.16470589, maybe_rgb.unwrap().blue);

    buf[2] = 1.0;
    buf[3] = 0.0;
    maybe_rgb = device_cmyk.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(0.85098046, maybe_rgb.unwrap().red);
    assert_float_eq(0.552941, maybe_rgb.unwrap().green);
    assert_float_eq(0.15686275, maybe_rgb.unwrap().blue);

    // Test out of range values
    buf[2] = 1.5;
    buf[3] = -0.6;
    maybe_rgb = device_cmyk.get_rgb(&buf);
    assert!(maybe_rgb.is_some());
    assert_float_eq(0.85098046, maybe_rgb.unwrap().red);
    assert_float_eq(0.552941, maybe_rgb.unwrap().green);
    assert_float_eq(0.15686275, maybe_rgb.unwrap().blue);
}
