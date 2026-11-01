// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/dib/blend_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::blend;
use crate::fxge::dib::fx_dib::BlendMode;

#[test]
fn Normal() {
    assert_eq!(0, blend(BlendMode::Normal, 0, 0));
    assert_eq!(0, blend(BlendMode::Normal, 99, 0));
    assert_eq!(0, blend(BlendMode::Normal, 255, 0));
    assert_eq!(99, blend(BlendMode::Normal, 0, 99));
    assert_eq!(99, blend(BlendMode::Normal, 99, 99));
    assert_eq!(99, blend(BlendMode::Normal, 255, 99));
    assert_eq!(255, blend(BlendMode::Normal, 0, 255));
    assert_eq!(255, blend(BlendMode::Normal, 99, 255));
    assert_eq!(255, blend(BlendMode::Normal, 255, 255));
}

#[test]
fn Multiply() {
    assert_eq!(0, blend(BlendMode::Multiply, 0, 0));
    assert_eq!(0, blend(BlendMode::Multiply, 99, 0));
    assert_eq!(0, blend(BlendMode::Multiply, 255, 0));
    assert_eq!(0, blend(BlendMode::Multiply, 0, 99));
    assert_eq!(38, blend(BlendMode::Multiply, 99, 99));
    assert_eq!(99, blend(BlendMode::Multiply, 255, 99));
    assert_eq!(0, blend(BlendMode::Multiply, 0, 255));
    assert_eq!(99, blend(BlendMode::Multiply, 99, 255));
    assert_eq!(255, blend(BlendMode::Multiply, 255, 255));
}

#[test]
fn Screen() {
    assert_eq!(0, blend(BlendMode::Screen, 0, 0));
    assert_eq!(99, blend(BlendMode::Screen, 99, 0));
    assert_eq!(255, blend(BlendMode::Screen, 255, 0));
    assert_eq!(99, blend(BlendMode::Screen, 0, 99));
    assert_eq!(160, blend(BlendMode::Screen, 99, 99));
    assert_eq!(255, blend(BlendMode::Screen, 255, 99));
    assert_eq!(255, blend(BlendMode::Screen, 0, 255));
    assert_eq!(255, blend(BlendMode::Screen, 99, 255));
    assert_eq!(255, blend(BlendMode::Screen, 255, 255));
}

#[test]
fn Overlay() {
    assert_eq!(0, blend(BlendMode::Overlay, 0, 0));
    assert_eq!(0, blend(BlendMode::Overlay, 99, 0));
    assert_eq!(143, blend(BlendMode::Overlay, 199, 0));
    assert_eq!(255, blend(BlendMode::Overlay, 255, 0));
    assert_eq!(0, blend(BlendMode::Overlay, 0, 99));
    assert_eq!(76, blend(BlendMode::Overlay, 99, 99));
    assert_eq!(187, blend(BlendMode::Overlay, 199, 99));
    assert_eq!(255, blend(BlendMode::Overlay, 255, 99));
    assert_eq!(0, blend(BlendMode::Overlay, 0, 199));
    assert_eq!(154, blend(BlendMode::Overlay, 99, 199));
    assert_eq!(231, blend(BlendMode::Overlay, 199, 199));
    assert_eq!(255, blend(BlendMode::Overlay, 255, 199));
    assert_eq!(0, blend(BlendMode::Overlay, 0, 255));
    assert_eq!(198, blend(BlendMode::Overlay, 99, 255));
    assert_eq!(255, blend(BlendMode::Overlay, 199, 255));
    assert_eq!(255, blend(BlendMode::Overlay, 255, 255));
}

#[test]
fn Darken() {
    assert_eq!(0, blend(BlendMode::Darken, 0, 0));
    assert_eq!(0, blend(BlendMode::Darken, 99, 0));
    assert_eq!(0, blend(BlendMode::Darken, 255, 0));
    assert_eq!(0, blend(BlendMode::Darken, 0, 99));
    assert_eq!(99, blend(BlendMode::Darken, 99, 99));
    assert_eq!(99, blend(BlendMode::Darken, 255, 99));
    assert_eq!(0, blend(BlendMode::Darken, 0, 255));
    assert_eq!(99, blend(BlendMode::Darken, 99, 255));
    assert_eq!(255, blend(BlendMode::Darken, 255, 255));
}

#[test]
fn Lighten() {
    assert_eq!(0, blend(BlendMode::Lighten, 0, 0));
    assert_eq!(99, blend(BlendMode::Lighten, 99, 0));
    assert_eq!(255, blend(BlendMode::Lighten, 255, 0));
    assert_eq!(99, blend(BlendMode::Lighten, 0, 99));
    assert_eq!(99, blend(BlendMode::Lighten, 99, 99));
    assert_eq!(255, blend(BlendMode::Lighten, 255, 99));
    assert_eq!(255, blend(BlendMode::Lighten, 0, 255));
    assert_eq!(255, blend(BlendMode::Lighten, 99, 255));
    assert_eq!(255, blend(BlendMode::Lighten, 255, 255));
}

#[test]
fn ColorDodge() {
    assert_eq!(0, blend(BlendMode::ColorDodge, 0, 0));
    assert_eq!(99, blend(BlendMode::ColorDodge, 99, 0));
    assert_eq!(255, blend(BlendMode::ColorDodge, 255, 0));
    assert_eq!(0, blend(BlendMode::ColorDodge, 0, 99));
    assert_eq!(161, blend(BlendMode::ColorDodge, 99, 99));
    assert_eq!(255, blend(BlendMode::ColorDodge, 255, 99));
    assert_eq!(255, blend(BlendMode::ColorDodge, 0, 255));
    assert_eq!(255, blend(BlendMode::ColorDodge, 99, 255));
    assert_eq!(255, blend(BlendMode::ColorDodge, 255, 255));
}

#[test]
fn ColorBurn() {
    assert_eq!(0, blend(BlendMode::ColorBurn, 0, 0));
    assert_eq!(0, blend(BlendMode::ColorBurn, 99, 0));
    assert_eq!(0, blend(BlendMode::ColorBurn, 255, 0));
    assert_eq!(0, blend(BlendMode::ColorBurn, 0, 99));
    assert_eq!(0, blend(BlendMode::ColorBurn, 99, 99));
    assert_eq!(255, blend(BlendMode::ColorBurn, 255, 99));
    assert_eq!(0, blend(BlendMode::ColorBurn, 0, 255));
    assert_eq!(99, blend(BlendMode::ColorBurn, 99, 255));
    assert_eq!(255, blend(BlendMode::ColorBurn, 255, 255));
}

#[test]
fn HardLight() {
    assert_eq!(0, blend(BlendMode::HardLight, 0, 0));
    assert_eq!(0, blend(BlendMode::HardLight, 99, 0));
    assert_eq!(0, blend(BlendMode::HardLight, 255, 0));
    assert_eq!(0, blend(BlendMode::HardLight, 0, 99));
    assert_eq!(76, blend(BlendMode::HardLight, 99, 99));
    assert_eq!(198, blend(BlendMode::HardLight, 255, 99));
    assert_eq!(143, blend(BlendMode::HardLight, 0, 199));
    assert_eq!(187, blend(BlendMode::HardLight, 99, 199));
    assert_eq!(255, blend(BlendMode::HardLight, 255, 199));
    assert_eq!(255, blend(BlendMode::HardLight, 0, 255));
    assert_eq!(255, blend(BlendMode::HardLight, 99, 255));
    assert_eq!(255, blend(BlendMode::HardLight, 255, 255));
}

#[test]
fn SoftLight() {
    assert_eq!(0, blend(BlendMode::SoftLight, 0, 0));
    assert_eq!(39, blend(BlendMode::SoftLight, 99, 0));
    assert_eq!(255, blend(BlendMode::SoftLight, 255, 0));
    assert_eq!(0, blend(BlendMode::SoftLight, 0, 99));
    assert_eq!(86, blend(BlendMode::SoftLight, 99, 99));
    assert_eq!(255, blend(BlendMode::SoftLight, 255, 99));
    assert_eq!(0, blend(BlendMode::SoftLight, 0, 199));
    assert_eq!(81, blend(BlendMode::SoftLight, 47, 199));
    assert_eq!(132, blend(BlendMode::SoftLight, 99, 199));
    assert_eq!(255, blend(BlendMode::SoftLight, 255, 199));
    assert_eq!(0, blend(BlendMode::SoftLight, 0, 255));
    assert_eq!(109, blend(BlendMode::SoftLight, 47, 255));
    assert_eq!(159, blend(BlendMode::SoftLight, 99, 255));
    assert_eq!(255, blend(BlendMode::SoftLight, 255, 255));
}

#[test]
fn Difference() {
    assert_eq!(0, blend(BlendMode::Difference, 0, 0));
    assert_eq!(99, blend(BlendMode::Difference, 99, 0));
    assert_eq!(255, blend(BlendMode::Difference, 255, 0));
    assert_eq!(99, blend(BlendMode::Difference, 0, 99));
    assert_eq!(0, blend(BlendMode::Difference, 99, 99));
    assert_eq!(156, blend(BlendMode::Difference, 255, 99));
    assert_eq!(255, blend(BlendMode::Difference, 0, 255));
    assert_eq!(156, blend(BlendMode::Difference, 99, 255));
    assert_eq!(0, blend(BlendMode::Difference, 255, 255));
}

#[test]
fn Exclusion() {
    assert_eq!(0, blend(BlendMode::Exclusion, 0, 0));
    assert_eq!(99, blend(BlendMode::Exclusion, 99, 0));
    assert_eq!(255, blend(BlendMode::Exclusion, 255, 0));
    assert_eq!(99, blend(BlendMode::Exclusion, 0, 99));
    assert_eq!(122, blend(BlendMode::Exclusion, 99, 99));
    assert_eq!(156, blend(BlendMode::Exclusion, 255, 99));
    assert_eq!(255, blend(BlendMode::Exclusion, 0, 255));
    assert_eq!(156, blend(BlendMode::Exclusion, 99, 255));
    assert_eq!(0, blend(BlendMode::Exclusion, 255, 255));
}
