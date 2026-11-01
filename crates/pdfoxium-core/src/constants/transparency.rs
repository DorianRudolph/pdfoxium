// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
//! Translation of `upstream/pdfium/constants/transparency.h`.
#![allow(non_upper_case_globals)]

// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
// ISO 32000-1:2008 spec, table 136.
// Standard separable blend modes.
pub const kNormal: &str = "Normal";
pub const kMultiply: &str = "Multiply";
pub const kScreen: &str = "Screen";
pub const kOverlay: &str = "Overlay";
pub const kDarken: &str = "Darken";
pub const kLighten: &str = "Lighten";
pub const kColorDodge: &str = "ColorDodge";
pub const kColorBurn: &str = "ColorBurn";
pub const kHardLight: &str = "HardLight";
pub const kSoftLight: &str = "SoftLight";
pub const kDifference: &str = "Difference";
pub const kExclusion: &str = "Exclusion";
// ISO 32000-1:2008 spec, table 137.
// Standard nonseparable blend modes.
pub const kHue: &str = "Hue";
pub const kSaturation: &str = "Saturation";
pub const kColor: &str = "Color";
pub const kLuminosity: &str = "Luminosity";
// ISO 32000-1:2008 spec, table 144.
// Entries in a soft-mask dictionary.
pub const kSoftMaskSubType: &str = "S";
pub const kAlpha: &str = "Alpha";
pub const kG: &str = "G";
pub const kBC: &str = "BC";
pub const kTR: &str = "TR";
// ISO 32000-1:2008 spec, table 147.
// Additional entries specific to a transparency group attributes dictionary.
pub const kGroupSubType: &str = "S";
pub const kTransparency: &str = "Transparency";
pub const kCS: &str = "CS";
pub const kI: &str = "I";
