// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/cfx_color_unittest.cpp`.

#![allow(non_snake_case, non_upper_case_globals)]

use super::*;
use crate::test_support::assert_float_eq;

#[test]
fn OperatorMinusUnusedComponents() {
    // Construct a Gray color. Unused components (2, 3, 4) should be 0.0f32.
    let c1 = CfxColor::make_gray(0.2f32);
    assert_eq!(c1.color_type, ColorType::Gray);
    assert_float_eq(c1.color1, 0.2f32);
    assert_float_eq(c1.color2, 0.0f32);
    assert_float_eq(c1.color3, 0.0f32);
    assert_float_eq(c1.color4, 0.0f32);

    // Subtract a negative value (effectively adding).
    // This should only affect the active component (fColor1).
    let c2 = c1 - (-0.5f32);

    assert_eq!(c2.color_type, ColorType::Gray);
    assert_float_eq(c2.color1, 0.7f32);

    // These failed before the fix because they became 0.5f32.
    assert_float_eq(c2.color2, 0.0f32);
    assert_float_eq(c2.color3, 0.0f32);
    assert_float_eq(c2.color4, 0.0f32);

    // Construct a Gray color with the expected value directly.
    let c3 = CfxColor::make_gray(0.7f32);

    // Should be equal.
    assert_eq!(c2, c3);

    // Construct a Gray color with non-zero unused components.
    let mut c4 = CfxColor::make_gray(0.7f32);
    c4.color2 = 0.5f32;
    c4.color3 = 0.5f32;
    c4.color4 = 0.5f32;

    // Equality should ignore fColor2, fColor3, and fColor4.
    assert_eq!(c3, c4);
}

#[test]
fn OperatorSlashUnusedComponents() {
    // Construct a Gray color. Unused components (2, 3, 4) should be 0.0f32.
    let c1 = CfxColor::make_gray(0.8f32);

    // Divide.
    let c2 = c1 / 2.0f32;

    assert_eq!(c2.color_type, ColorType::Gray);
    assert_float_eq(c2.color1, 0.4f32);
    assert_float_eq(c2.color2, 0.0f32);
    assert_float_eq(c2.color3, 0.0f32);
    assert_float_eq(c2.color4, 0.0f32);

    let c3 = CfxColor::make_gray(0.4f32);
    assert_eq!(c2, c3);
}
