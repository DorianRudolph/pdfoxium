// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2024 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/dib/cfx_scanlinecompositor_unittest.cpp`.
//! BGRA/BGR struct tables are contiguous channel bytes in the same field order.
#![allow(non_snake_case, non_upper_case_globals)]
use super::*;

#[rustfmt::skip]
const kDestScan: [u8; 32] = [
    255, 100, 0, 0,
    255, 100, 0, 0,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 100,
    255, 100, 0, 100,
    255, 100, 0, 200,
    255, 100, 0, 200,
];
#[rustfmt::skip]
const kSrcScan1: [u8; 32] = [
    255, 100, 0, 0,
    255, 100, 0, 255,
    255, 100, 0, 0,
    255, 100, 0, 255,
    255, 100, 0, 100,
    255, 100, 0, 200,
    255, 100, 0, 100,
    255, 100, 0, 200,
];
#[rustfmt::skip]
const kSrcScan2: [u8; 32] = [
    100, 0, 255, 0,
    100, 0, 255, 255,
    100, 0, 255, 0,
    100, 0, 255, 255,
    100, 0, 255, 100,
    100, 0, 255, 200,
    100, 0, 255, 100,
    100, 0, 255, 200,
];
#[rustfmt::skip]
const kSrcScan3: [u8; 32] = [
    0, 255, 100, 0,
    0, 255, 100, 255,
    0, 255, 100, 0,
    0, 255, 100, 255,
    0, 255, 100, 100,
    0, 255, 100, 200,
    0, 255, 100, 100,
    0, 255, 100, 200,
];

fn RunTest(compositor: &CfxScanlineCompositor, src_span: &[u8], expectations: &[u8]) {
    let mut dest_scan = kDestScan;
    let width = dest_scan.len() / 4;
    compositor.composite_rgb_bitmap_line(&mut dest_scan, src_span, width, None);
    assert_eq!(dest_scan.as_slice(), expectations);
}

#[test]
fn CompositeRgbBitmapLineBgraNormal() {
    let compositor = CfxScanlineCompositor::new(
        FxdibFormat::Bgra,
        FxdibFormat::Bgra,
        &[],
        0,
        BlendMode::Normal,
        false,
    );
    assert!(compositor.is_some());
    let compositor = compositor.unwrap();

    #[rustfmt::skip]
const kExpectations1: [u8; 32] = [
    255, 100, 0, 0,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 161,
    255, 100, 0, 222,
    255, 100, 0, 222,
    255, 100, 0, 244,
];
    RunTest(&compositor, &kSrcScan1, &kExpectations1);
    #[rustfmt::skip]
const kExpectations2: [u8; 32] = [
    100, 0, 255, 0,
    100, 0, 255, 255,
    255, 100, 0, 255,
    100, 0, 255, 255,
    158, 38, 158, 161,
    115, 10, 229, 222,
    185, 55, 114, 222,
    127, 18, 209, 244,
];
    RunTest(&compositor, &kSrcScan2, &kExpectations2);
    #[rustfmt::skip]
const kExpectations3: [u8; 32] = [
    0, 255, 100, 0,
    0, 255, 100, 255,
    255, 100, 0, 255,
    0, 255, 100, 255,
    97, 196, 61, 161,
    26, 239, 89, 222,
    141, 169, 44, 222,
    46, 227, 81, 244,
];
    RunTest(&compositor, &kSrcScan3, &kExpectations3);
}

#[test]
fn CompositeRgbBitmapLineBgraScreen() {
    let compositor = CfxScanlineCompositor::new(
        FxdibFormat::Bgra,
        FxdibFormat::Bgra,
        &[],
        0,
        BlendMode::Screen,
        false,
    );
    assert!(compositor.is_some());
    let compositor = compositor.unwrap();

    #[rustfmt::skip]
const kExpectations1: [u8; 32] = [
    255, 100, 0, 0,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 161, 0, 255,
    255, 114, 0, 161,
    255, 120, 0, 222,
    255, 121, 0, 222,
    255, 138, 0, 244,
];
    RunTest(&compositor, &kSrcScan1, &kExpectations1);
    #[rustfmt::skip]
const kExpectations2: [u8; 32] = [
    100, 0, 255, 0,
    100, 0, 255, 255,
    255, 100, 0, 255,
    255, 100, 255, 255,
    196, 62, 158, 161,
    169, 45, 229, 222,
    239, 90, 114, 222,
    227, 81, 209, 244,
];
    RunTest(&compositor, &kSrcScan2, &kExpectations2);
    #[rustfmt::skip]
const kExpectations3: [u8; 32] = [
    0, 255, 100, 0,
    0, 255, 100, 255,
    255, 100, 0, 255,
    255, 255, 100, 255,
    158, 196, 61, 161,
    115, 239, 89, 222,
    230, 169, 44, 222,
    209, 227, 81, 244,
];
    RunTest(&compositor, &kSrcScan3, &kExpectations3);
}

#[test]
fn CompositeRgbBitmapLineBgraDarken() {
    let compositor = CfxScanlineCompositor::new(
        FxdibFormat::Bgra,
        FxdibFormat::Bgra,
        &[],
        0,
        BlendMode::Darken,
        false,
    );
    assert!(compositor.is_some());
    let compositor = compositor.unwrap();

    #[rustfmt::skip]
const kExpectations1: [u8; 32] = [
    255, 100, 0, 0,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 161,
    255, 100, 0, 222,
    255, 100, 0, 222,
    255, 100, 0, 244,
];
    RunTest(&compositor, &kSrcScan1, &kExpectations1);
    #[rustfmt::skip]
const kExpectations2: [u8; 32] = [
    100, 0, 255, 0,
    100, 0, 255, 255,
    255, 100, 0, 255,
    100, 0, 0, 255,
    158, 38, 96, 161,
    115, 10, 139, 222,
    185, 55, 24, 222,
    127, 18, 45, 244,
];
    RunTest(&compositor, &kSrcScan2, &kExpectations2);
    #[rustfmt::skip]
const kExpectations3: [u8; 32] = [
    0, 255, 100, 0,
    0, 255, 100, 255,
    255, 100, 0, 255,
    0, 100, 0, 255,
    97, 158, 37, 161,
    26, 184, 53, 222,
    141, 114, 9, 222,
    46, 127, 17, 244,
];
    RunTest(&compositor, &kSrcScan3, &kExpectations3);
}

#[test]
fn CompositeRgbBitmapLineBgraSoftLight() {
    let compositor = CfxScanlineCompositor::new(
        FxdibFormat::Bgra,
        FxdibFormat::Bgra,
        &[],
        0,
        BlendMode::SoftLight,
        false,
    );
    assert!(compositor.is_some());
    let compositor = compositor.unwrap();

    #[rustfmt::skip]
const kExpectations1: [u8; 32] = [
    255, 100, 0, 0,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 87, 0, 255,
    255, 96, 0, 161,
    255, 94, 0, 222,
    255, 95, 0, 222,
    255, 90, 0, 244,
];
    RunTest(&compositor, &kSrcScan1, &kExpectations1);
    #[rustfmt::skip]
const kExpectations2: [u8; 32] = [
    100, 0, 255, 0,
    100, 0, 255, 255,
    255, 100, 0, 255,
    255, 40, 0, 255,
    196, 47, 96, 161,
    169, 23, 139, 222,
    239, 69, 24, 222,
    227, 43, 45, 244,
];
    RunTest(&compositor, &kSrcScan2, &kExpectations2);
    #[rustfmt::skip]
const kExpectations3: [u8; 32] = [
    0, 255, 100, 0,
    0, 255, 100, 255,
    255, 100, 0, 255,
    255, 160, 0, 255,
    158, 172, 37, 161,
    115, 205, 53, 222,
    230, 135, 9, 222,
    209, 165, 17, 244,
];
    RunTest(&compositor, &kSrcScan3, &kExpectations3);
}

#[test]
fn CompositeRgbBitmapLineBgraHue() {
    let compositor = CfxScanlineCompositor::new(
        FxdibFormat::Bgra,
        FxdibFormat::Bgra,
        &[],
        0,
        BlendMode::Hue,
        false,
    );
    assert!(compositor.is_some());
    let compositor = compositor.unwrap();

    #[rustfmt::skip]
const kExpectations1: [u8; 32] = [
    255, 100, 0, 0,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 161,
    255, 100, 0, 222,
    255, 100, 0, 222,
    255, 100, 0, 244,
];
    RunTest(&compositor, &kSrcScan1, &kExpectations1);
    #[rustfmt::skip]
const kExpectations2: [u8; 32] = [
    100, 0, 255, 0,
    100, 0, 255, 255,
    255, 100, 0, 255,
    100, 0, 255, 255,
    158, 38, 158, 161,
    115, 10, 229, 222,
    185, 55, 114, 222,
    127, 18, 209, 244,
];
    RunTest(&compositor, &kSrcScan2, &kExpectations2);
    #[rustfmt::skip]
const kExpectations3: [u8; 32] = [
    0, 255, 100, 0,
    0, 255, 100, 255,
    255, 100, 0, 255,
    0, 123, 49, 255,
    97, 163, 49, 161,
    26, 192, 71, 222,
    141, 122, 26, 222,
    46, 141, 49, 244,
];
    RunTest(&compositor, &kSrcScan3, &kExpectations3);
}

#[test]
fn CompositeRgbBitmapLineBgraSaturation() {
    let compositor = CfxScanlineCompositor::new(
        FxdibFormat::Bgra,
        FxdibFormat::Bgra,
        &[],
        0,
        BlendMode::Saturation,
        false,
    );
    assert!(compositor.is_some());
    let compositor = compositor.unwrap();

    #[rustfmt::skip]
const kExpectations1: [u8; 32] = [
    255, 100, 0, 0,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 161,
    255, 100, 0, 222,
    255, 100, 0, 222,
    255, 100, 0, 244,
];
    RunTest(&compositor, &kSrcScan1, &kExpectations1);
    #[rustfmt::skip]
const kExpectations2: [u8; 32] = [
    100, 0, 255, 0,
    100, 0, 255, 255,
    255, 100, 0, 255,
    255, 100, 0, 255,
    196, 62, 96, 161,
    169, 45, 139, 222,
    239, 90, 24, 222,
    227, 81, 45, 244,
];
    RunTest(&compositor, &kSrcScan2, &kExpectations2);
    #[rustfmt::skip]
const kExpectations3: [u8; 32] = [
    0, 255, 100, 0,
    0, 255, 100, 255,
    255, 100, 0, 255,
    255, 100, 0, 255,
    158, 158, 37, 161,
    115, 184, 53, 222,
    230, 114, 9, 222,
    209, 127, 17, 244,
];
    RunTest(&compositor, &kSrcScan3, &kExpectations3);
}

#[test]
fn CompositeRgbBitmapLineBgraColor() {
    let compositor = CfxScanlineCompositor::new(
        FxdibFormat::Bgra,
        FxdibFormat::Bgra,
        &[],
        0,
        BlendMode::Color,
        false,
    );
    assert!(compositor.is_some());
    let compositor = compositor.unwrap();

    #[rustfmt::skip]
const kExpectations1: [u8; 32] = [
    255, 100, 0, 0,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 161,
    255, 100, 0, 222,
    255, 100, 0, 222,
    255, 100, 0, 244,
];
    RunTest(&compositor, &kSrcScan1, &kExpectations1);
    #[rustfmt::skip]
const kExpectations2: [u8; 32] = [
    100, 0, 255, 0,
    100, 0, 255, 255,
    255, 100, 0, 255,
    100, 0, 255, 255,
    158, 38, 158, 161,
    115, 10, 229, 222,
    185, 55, 114, 222,
    127, 18, 209, 244,
];
    RunTest(&compositor, &kSrcScan2, &kExpectations2);
    #[rustfmt::skip]
const kExpectations3: [u8; 32] = [
    0, 255, 100, 0,
    0, 255, 100, 255,
    255, 100, 0, 255,
    0, 123, 49, 255,
    97, 163, 49, 161,
    26, 192, 71, 222,
    141, 122, 26, 222,
    46, 141, 49, 244,
];
    RunTest(&compositor, &kSrcScan3, &kExpectations3);
}

#[test]
fn CompositeRgbBitmapLineBgraLuminosity() {
    let compositor = CfxScanlineCompositor::new(
        FxdibFormat::Bgra,
        FxdibFormat::Bgra,
        &[],
        0,
        BlendMode::Luminosity,
        false,
    );
    assert!(compositor.is_some());
    let compositor = compositor.unwrap();

    #[rustfmt::skip]
const kExpectations1: [u8; 32] = [
    255, 100, 0, 0,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 255,
    255, 100, 0, 161,
    255, 100, 0, 222,
    255, 100, 0, 222,
    255, 100, 0, 244,
];
    RunTest(&compositor, &kSrcScan1, &kExpectations1);
    #[rustfmt::skip]
const kExpectations2: [u8; 32] = [
    100, 0, 255, 0,
    100, 0, 255, 255,
    255, 100, 0, 255,
    255, 100, 0, 255,
    196, 62, 96, 161,
    169, 45, 139, 222,
    239, 90, 24, 222,
    227, 81, 45, 244,
];
    RunTest(&compositor, &kSrcScan2, &kExpectations2);
    #[rustfmt::skip]
const kExpectations3: [u8; 32] = [
    0, 255, 100, 0,
    0, 255, 100, 255,
    255, 100, 0, 255,
    255, 185, 142, 255,
    158, 178, 71, 161,
    115, 214, 104, 222,
    230, 144, 59, 222,
    209, 181, 108, 244,
];
    RunTest(&compositor, &kSrcScan3, &kExpectations3);
}

#[test]
fn CompositeRgbBitmapLineBgrToBgraNormal() {
    #[rustfmt::skip]
const kBgrSrc: [u8; 15] = [
    10, 20, 30,
    40, 50, 60,
    70, 80, 90,
    100, 110, 120,
    130, 140, 150,
];

    {
        let compositor = CfxScanlineCompositor::new(
            FxdibFormat::Bgra,
            FxdibFormat::Bgr,
            &[],
            0,
            BlendMode::Normal,
            false,
        );
        assert!(compositor.is_some());
        let compositor = compositor.unwrap();
        let mut dest_scan = [0u8; 5 * 4];
        let width = dest_scan.len() / 4;
        compositor.composite_rgb_bitmap_line(&mut dest_scan, &kBgrSrc, width, None);
        #[rustfmt::skip]
const kExpectedBgra: [u8; 20] = [
    10, 20, 30, 255,
    40, 50, 60, 255,
    70, 80, 90, 255,
    100, 110, 120, 255,
    130, 140, 150, 255,
];
        assert_eq!(dest_scan, kExpectedBgra);
    }

    {
        let compositor = CfxScanlineCompositor::new(
            FxdibFormat::Bgra,
            FxdibFormat::Bgr,
            &[],
            0,
            BlendMode::Normal,
            true,
        );
        assert!(compositor.is_some());
        let compositor = compositor.unwrap();
        let mut dest_scan = [0u8; 5 * 4];
        let width = dest_scan.len() / 4;
        compositor.composite_rgb_bitmap_line(&mut dest_scan, &kBgrSrc, width, None);
        #[rustfmt::skip]
const kExpectedRgba: [u8; 20] = [
    30, 20, 10, 255,
    60, 50, 40, 255,
    90, 80, 70, 255,
    120, 110, 100, 255,
    150, 140, 130, 255,
];
        assert_eq!(dest_scan, kExpectedRgba);
    }
}
