// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2025 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! testing/png_codec/png_codec_unittest.cpp through the actual CLI channel
//! adapter, engine bitmap conversion, and published image PNG export/load path.
//! No native PNG wrapper or unused production format flags are introduced.
use super::rgba_image_from_bgra;
use image::{DynamicImage, ImageFormat, RgbaImage};
use pdfoxium_core::fxge::dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat};
use std::io::Cursor;

fn encode(image: RgbaImage) -> Vec<u8> {
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut png, ImageFormat::Png)
        .unwrap();
    png.into_inner()
}
fn encode_bitmap_pixels(input: &[u8], format: FxdibFormat) -> Vec<u8> {
    let mut bitmap = CfxDibitmap::create(1, 1, format, 0).unwrap();
    bitmap.scanline_mut(0).unwrap()[..input.len()].copy_from_slice(input);
    // Actual engine conversion supplies opaque alpha for BGRx/BGR/gray;
    // BGRA preserves alpha. The CLI always exports its resulting BGRA plane.
    assert!(bitmap.convert_format(FxdibFormat::Bgra));
    encode(rgba_image_from_bgra(1, 1, bitmap.buffer().to_vec()).unwrap())
}
fn decode_1x1_png_to_rgba(input: &[u8]) -> Vec<u8> {
    let image = image::load_from_memory(input).unwrap().into_rgba8();
    assert_eq!(image.height(), 1);
    assert_eq!(image.width(), 1);
    image.into_raw()
}
#[test]
fn encode_bgra_and_discard_transparency() {
    let bgrx_input = [1, 2, 3, 4];
    let png = encode_bitmap_pixels(&bgrx_input, FxdibFormat::Bgrx);
    assert!(!png.is_empty());
    let rgba_output = decode_1x1_png_to_rgba(&png);
    assert_eq!(rgba_output.len(), 4);
    assert_eq!(rgba_output[0], bgrx_input[2]);
    assert_eq!(rgba_output[1], bgrx_input[1]);
    assert_eq!(rgba_output[2], bgrx_input[0]);
    assert_eq!(rgba_output[3], 255);
}
#[test]
fn encode_bgra_and_preserve_transparency() {
    let bgra_input = [1, 2, 3, 4];
    let png = encode_bitmap_pixels(&bgra_input, FxdibFormat::Bgra);
    assert!(!png.is_empty());
    let rgba_output = decode_1x1_png_to_rgba(&png);
    assert_eq!(rgba_output.len(), 4);
    assert_eq!(rgba_output[0], bgra_input[2]);
    assert_eq!(rgba_output[1], bgra_input[1]);
    assert_eq!(rgba_output[2], bgra_input[0]);
    assert_eq!(rgba_output[3], bgra_input[3]);
}
#[test]
fn encode_bgr() {
    let bgr_input = [1, 2, 3];
    let png = encode_bitmap_pixels(&bgr_input, FxdibFormat::Bgr);
    assert!(!png.is_empty());
    let rgba_output = decode_1x1_png_to_rgba(&png);
    assert_eq!(rgba_output.len(), 4);
    assert_eq!(rgba_output[0], bgr_input[2]);
    assert_eq!(rgba_output[1], bgr_input[1]);
    assert_eq!(rgba_output[2], bgr_input[0]);
    assert_eq!(rgba_output[3], 255);
}
#[test]
fn encode_rgba() {
    let rgba_input = [1, 2, 3, 4];
    let png = encode(RgbaImage::from_raw(1, 1, rgba_input.to_vec()).unwrap());
    assert!(!png.is_empty());
    let rgba_output = decode_1x1_png_to_rgba(&png);
    assert_eq!(rgba_output.len(), 4);
    assert_eq!(rgba_output[0], rgba_input[0]);
    assert_eq!(rgba_output[1], rgba_input[1]);
    assert_eq!(rgba_output[2], rgba_input[2]);
    assert_eq!(rgba_output[3], rgba_input[3]);
}
#[test]
fn encode_gray() {
    let grayscale_input = [123];
    let png = encode_bitmap_pixels(&grayscale_input, FxdibFormat::EightBppRgb);
    assert!(!png.is_empty());
    let rgba_output = decode_1x1_png_to_rgba(&png);
    assert_eq!(rgba_output.len(), 4);
    assert_eq!(rgba_output[0], grayscale_input[0]);
    assert_eq!(rgba_output[1], grayscale_input[0]);
    assert_eq!(rgba_output[2], grayscale_input[0]);
    assert_eq!(rgba_output[3], 255);
}
