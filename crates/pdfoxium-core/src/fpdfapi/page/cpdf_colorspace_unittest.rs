// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/page/cpdf_colorspace_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::{CpdfCalGray, CpdfCalRgb};
use crate::fpdfapi::page::cpdf_colorspace::CpdfColorSpace;

#[test]
fn CPDFCalGrayTest_TranslateImageLine() {
    // AllocateColorSpace("CalG") constructs this unloaded type. Rust's value
    // construction guarantees the native ASSERT_TRUE(pCal) condition.
    let p_cal = CpdfCalGray::default();
    const kSrc: [u8; 12] = [255, 0, 0, 0, 255, 0, 0, 0, 255, 128, 128, 128];
    let mut dst = [0xbd; 12];
    // bTransMask only applies to CMYK colorspaces.
    p_cal.translate_image_line(&mut dst, &kSrc, 4, 4, 1, false);
    assert_eq!(dst, [255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn CPDFCalRGBTest_TranslateImageLine() {
    // AllocateColorSpace("CalR") constructs this unloaded type.
    let p_cal = CpdfCalRgb::default();
    const kSrc: [u8; 12] = [255, 0, 0, 0, 255, 0, 0, 0, 255, 128, 128, 128];
    let mut dst = [0xbd; 12];
    p_cal.translate_image_line(&mut dst, &kSrc, 4, 4, 1, false);
    assert_eq!(dst, [0, 0, 255, 0, 255, 0, 255, 0, 0, 128, 128, 128]);
}
