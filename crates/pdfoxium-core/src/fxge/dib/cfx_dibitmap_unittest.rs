// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/dib/cfx_dibitmap_unittest.cpp` for AGG.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CfxDibitmap;
use crate::fxge::dib::fx_dib::FxdibFormat;

#[test]
fn Create() {
    let mut pBitmap = CfxDibitmap::default();
    assert!(!pBitmap.create_in_place(400, 300, FxdibFormat::Invalid, 0));
    let mut pBitmap = CfxDibitmap::default();
    assert!(pBitmap.create_in_place(400, 300, FxdibFormat::OneBppRgb, 0));
}

#[test]
fn CalculatePitchAndSizeGood() {
    for (format, provided_pitch, expected_pitch, expected_size) in [
        (FxdibFormat::Bgra, 0, 400, 80000),
        (FxdibFormat::EightBppRgb, 0, 100, 20000),
        (FxdibFormat::Bgra, 400, 400, 80000),
        (FxdibFormat::Bgra, 455, 455, 91000),
    ] {
        let result = CfxDibitmap::calculate_pitch_and_size(100, 200, format, provided_pitch);
        assert!(result.is_some());
        let (pitch, size) = result.unwrap();
        assert_eq!(pitch, expected_pitch);
        assert_eq!(size, expected_size);
    }
}

#[test]
fn CalculatePitchAndSizeBad() {
    let kBadDimensions = [
        (0, 0),
        (-1, -1),
        (-1, 0),
        (0, -1),
        (0, 200),
        (100, 0),
        (-1, 200),
        (100, -1),
    ];
    for (width, height) in kBadDimensions {
        assert!(
            CfxDibitmap::calculate_pitch_and_size(width, height, FxdibFormat::Bgra, 0).is_none()
        );
        assert!(
            CfxDibitmap::calculate_pitch_and_size(width, height, FxdibFormat::Bgra, 1).is_none()
        );
    }
    assert!(CfxDibitmap::calculate_pitch_and_size(100, 200, FxdibFormat::Invalid, 0).is_none());
    assert!(CfxDibitmap::calculate_pitch_and_size(100, 200, FxdibFormat::Invalid, 800).is_none());
    assert!(CfxDibitmap::calculate_pitch_and_size(101, 200, FxdibFormat::Bgra, 400).is_none());
    assert!(CfxDibitmap::calculate_pitch_and_size(1073747000, 1, FxdibFormat::Bgra, 0).is_none());
    assert!(CfxDibitmap::calculate_pitch_and_size(1048576, 1024, FxdibFormat::Bgra, 0).is_none());
    assert!(
        CfxDibitmap::calculate_pitch_and_size(4194304, 1024, FxdibFormat::EightBppRgb, 0).is_none()
    );
    // Native passes an unsigned literal to int width; preserve its narrowing.
    assert!(CfxDibitmap::calculate_pitch_and_size(
        2147484000u32 as i32,
        1,
        FxdibFormat::Bgra,
        2147484000
    )
    .is_none());
    assert!(
        CfxDibitmap::calculate_pitch_and_size(1048576, 1024, FxdibFormat::Bgra, 4194304).is_none()
    );
    assert!(CfxDibitmap::calculate_pitch_and_size(
        4194304,
        1024,
        FxdibFormat::EightBppRgb,
        4194304
    )
    .is_none());
}

#[test]
fn CalculatePitchAndSizeBoundary() {
    let result = CfxDibitmap::calculate_pitch_and_size(536870911, 4, FxdibFormat::EightBppRgb, 0);
    assert!(result.is_some());
    let (pitch, size) = result.unwrap();
    assert_eq!(pitch, 536870912);
    assert_eq!(size, 2147483648);
    assert!(
        CfxDibitmap::calculate_pitch_and_size(536870912, 4, FxdibFormat::EightBppRgb, 0).is_none()
    );
    let result = CfxDibitmap::calculate_pitch_and_size(68174084, 63, FxdibFormat::EightBppRgb, 0);
    assert!(result.is_some());
    let (pitch, size) = result.unwrap();
    assert_eq!(pitch, 68174084);
    assert_eq!(size, 4294967292);
    assert!(
        CfxDibitmap::calculate_pitch_and_size(68174085, 63, FxdibFormat::EightBppRgb, 0).is_none()
    );
}

#[test]
fn GetScanlineAsWith24Bpp() {
    let mut bitmap = CfxDibitmap::default();
    assert!(bitmap.create_in_place(3, 3, FxdibFormat::Bgr, 0));
    assert_eq!(bitmap.width(), 3);
    assert_eq!(bitmap.pitch(), 12);
    assert_eq!(bitmap.buffer().len(), 36);
    assert_eq!(bitmap.scanline(0).unwrap().len(), 12);
    assert_eq!(bitmap.scanline_pixels::<3>(0).unwrap().len(), 3);
    assert_eq!(bitmap.buffer_mut().len(), 36);
    assert_eq!(bitmap.scanline_mut(0).unwrap().len(), 12);
    assert_eq!(bitmap.scanline_pixels_mut::<3>(0).unwrap().len(), 3);
}
