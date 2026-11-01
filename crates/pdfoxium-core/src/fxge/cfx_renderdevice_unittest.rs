// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Selected bitmap/AGG cases from `core/fxge/cfx_renderdevice_unittest.cpp`.
//! CFX_RenderDevice's sole selected driver is borrowed directly in Rust;
//! keep the bitmap owner alive for that borrow instead of an owning wrapper.
#![allow(non_snake_case)]
use super::*;

fn CreateDeviceForBitmap(width: i32, height: i32) -> Option<CfxDibitmap> {
    CfxDibitmap::create(width, height, FxdibFormat::Bgra, 0)
}

#[test]
fn GetClipBoxDefault() {
    let bitmap = CreateDeviceForBitmap(16, 16);
    assert!(bitmap.is_some());
    let mut bitmap = bitmap.unwrap();
    let device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
    assert_eq!(FxRect::new(0, 0, 16, 16), device.clip_box());
}

#[test]
fn GetClipBoxPathFill() {
    // Matrix that transposes and translates by 1 unit on each axis.
    let object_to_device = CfxMatrix::new(0.0, 1.0, 1.0, 0.0, 1.0, -1.0);
    // Fill type cannot be none.
    let fill_options = CfxFillRenderOptions::new(FillType::EvenOdd);
    let bitmap = CreateDeviceForBitmap(16, 16);
    assert!(bitmap.is_some());
    let mut bitmap = bitmap.unwrap();
    let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
    let mut path = CfxPath::new();
    path.append_rect(2.0, 4.0, 14.0, 12.0);
    assert!(device.set_clip_path_fill(&path, Some(object_to_device), fill_options));
    assert_eq!(FxRect::new(5, 1, 13, 13), device.clip_box());
}

#[test]
fn GetClipBoxPathStroke() {
    // Matrix that transposes and translates by 1 unit on each axis.
    let object_to_device = CfxMatrix::new(0.0, 1.0, 1.0, 0.0, 1.0, -1.0);
    // Default line width is 1.
    let graphics_state = CfxGraphStateData::default();
    let bitmap = CreateDeviceForBitmap(16, 16);
    assert!(bitmap.is_some());
    let mut bitmap = bitmap.unwrap();
    let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
    let mut path = CfxPath::new();
    path.append_rect(2.0, 4.0, 14.0, 12.0);
    assert!(device.set_clip_path_stroke(&path, Some(object_to_device), &graphics_state));
    assert_eq!(FxRect::new(4, 0, 14, 14), device.clip_box());
}

#[test]
fn GetClipBoxRect() {
    let bitmap = CreateDeviceForBitmap(16, 16);
    assert!(bitmap.is_some());
    let mut bitmap = bitmap.unwrap();
    let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
    assert!(device.set_clip_rect(FxRect::new(2, 4, 14, 12)));
    assert_eq!(FxRect::new(2, 4, 14, 12), device.clip_box());
}

#[test]
fn GetClipBoxEmpty() {
    let bitmap = CreateDeviceForBitmap(16, 16);
    assert!(bitmap.is_some());
    let mut bitmap = bitmap.unwrap();
    let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
    assert!(device.set_clip_rect(FxRect::new(2, 8, 14, 8)));
    assert!(device.clip_box().is_empty());
}
