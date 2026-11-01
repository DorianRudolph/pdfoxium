// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Applicable cases from `testing/utils/pixel_diff_util_unittest.cpp`, exercised
//! through the comparison CLI's actual exact RGBA difference operation.
#![allow(non_snake_case)]

use super::difference;
use image::{Rgba, RgbaImage};

fn max_pixel_per_channel_delta(a: u32, b: u32) -> u8 {
    // Native packed pixels have three color bytes followed by alpha. Their
    // color-channel order does not affect the maximum. The CLI displays each
    // RGB delta with an alpha-delta floor, so its largest displayed channel is
    // exactly the largest of all four native per-channel deltas.
    let a = RgbaImage::from_pixel(1, 1, Rgba(a.to_le_bytes()));
    let b = RgbaImage::from_pixel(1, 1, Rgba(b.to_le_bytes()));
    let (diff, _, _) = difference(&a, &b);
    *diff.as_raw().iter().max().unwrap()
}

#[test]
fn MaxPixelPerChannelDeltaExactMatch() {
    assert_eq!(0, max_pixel_per_channel_delta(0x00000000, 0x00000000));
    assert_eq!(0, max_pixel_per_channel_delta(0xffffffff, 0xffffffff));
    assert_eq!(0, max_pixel_per_channel_delta(0x12345678, 0x12345678));
}

#[test]
fn MaxPixelPerChannelDeltaSingleChannel() {
    assert_eq!(5, max_pixel_per_channel_delta(0x00000005, 0x00000000));
    assert_eq!(10, max_pixel_per_channel_delta(0x00000a00, 0x00000000));
    assert_eq!(15, max_pixel_per_channel_delta(0x000f0000, 0x00000000));
    assert_eq!(20, max_pixel_per_channel_delta(0x14000000, 0x00000000));
    assert_eq!(255, max_pixel_per_channel_delta(0x000000ff, 0x00000000));
}

#[test]
fn MaxPixelPerChannelDeltaMultipleChannels() {
    assert_eq!(8, max_pixel_per_channel_delta(0x01020304, 0x09020304));
    assert_eq!(7, max_pixel_per_channel_delta(0x01020304, 0x01090304));
    assert_eq!(6, max_pixel_per_channel_delta(0x01020304, 0x01020904));
    assert_eq!(5, max_pixel_per_channel_delta(0x01020304, 0x01020309));
}

#[test]
fn CalculatePixelsDifferentExactMatch() {
    let image = RgbaImage::from_pixel(8, 8, Rgba(0xffffffffu32.to_le_bytes()));
    let (_, changed, _) = difference(&image, &image);
    assert_eq!(0, changed);
}
