// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use super::image_decode_size;
use crate::fpdfapi::page::{
    cpdf_docpagedata::CpdfDocPageData,
    cpdf_image::{CpdfImage, CpdfImageLoadContext},
    cpdf_imageobject::CpdfImageObject,
    cpdf_pageimagecache::CpdfPageImageCache,
};
use crate::fpdfapi::parser::{
    cpdf_dictionary::CpdfDictionary, cpdf_object::retain_object,
    cpdf_scalar_object::CpdfScalarObject, cpdf_stream::CpdfStream,
};
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxSize};
use std::rc::Rc;

fn image(width: i32, height: i32, bytes: Vec<u8>, filter: &[u8]) -> CpdfImageObject {
    let mut dict = CpdfDictionary::new();
    dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(width));
    dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(height));
    dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
    dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
    dict.set_new_name_for(b"Filter", filter);
    let stream = retain_object(CpdfStream::from_data_and_dict(bytes, retain_object(dict)));
    let mut object = CpdfImageObject::new(0);
    object.set_image(Some(Rc::new(CpdfImage::from_stream(stream, true).unwrap())));
    object
}

fn jpeg() -> CpdfImageObject {
    let bytes = libjpeg_turbo_rs::Encoder::new(
        &vec![128; 256 * 128],
        256,
        128,
        libjpeg_turbo_rs::PixelFormat::Grayscale,
    )
    .quality(90)
    .encode()
    .unwrap();
    image(256, 128, bytes, b"DCTDecode")
}

#[test]
fn drawn_edges_are_source_axes_not_the_rotated_device_bounds() {
    assert_eq!(
        image_decode_size(CfxMatrix::new(0.0, 120.0, -50.0, 0.0, 50.0, 0.0)),
        CfxSize::new(120, 50)
    );
    assert_eq!(
        image_decode_size(CfxMatrix::new(600.0, 800.0, -300.0, 400.0, 0.0, 0.0)),
        CfxSize::new(1000, 500)
    );
    assert_eq!(
        image_decode_size(CfxMatrix::new(-120.25, 0.0, 0.0, -50.5, 1e30, -1e30)),
        CfxSize::new(121, 51)
    );
}

#[test]
fn fractional_edge_above_scale_boundary_rounds_up_without_f32_loss() {
    assert_eq!(
        image_decode_size(CfxMatrix::new(512.0, 0.125, 0.0, 512.0, 0.0, 0.0)),
        CfxSize::new(513, 512)
    );
}

#[test]
fn invalid_or_degenerate_drawn_sizes_disable_reduction() {
    for matrix in [
        CfxMatrix::new(0.0, 0.0, 0.0, 20.0, 0.0, 0.0),
        CfxMatrix::new(20.0, 20.0, 10.0, 10.0, 0.0, 0.0),
        CfxMatrix::new(f32::NAN, 0.0, 0.0, 20.0, 0.0, 0.0),
        CfxMatrix::new(f32::INFINITY, 0.0, 0.0, 20.0, 0.0, 0.0),
        CfxMatrix::new(f32::MAX, 0.0, 0.0, f32::MAX, 0.0, 0.0),
    ] {
        assert_eq!(image_decode_size(matrix), CfxSize::new(0, 0));
    }
}

#[test]
fn small_device_does_not_reduce_a_zoomed_image_and_rotation_can_reduce() {
    let mut object = jpeg();
    let data = CpdfDocPageData::new();
    // A 64x64 tile does not enter the request: the same unit-square image is
    // drawn 512x256 on that device, and needs the full 256x128 source.
    object.set_image_matrix(CfxMatrix::new(512.0, 0.0, 0.0, 256.0, -100.0, -100.0));
    let mut context = CpdfImageLoadContext::new(&data, None);
    context.max_size = image_decode_size(object.matrix());
    let full = CpdfPageImageCache::new()
        .get_or_decode_data_with_context(&object, context)
        .unwrap();
    assert_eq!((full.bitmap.width(), full.bitmap.height()), (256, 128));
    // At a quarter-turn the source x/y demands remain 120x50, rather than
    // the 50x120 destination bounds. Uniform half-size decode is sufficient.
    object.set_image_matrix(CfxMatrix::new(0.0, 120.0, -50.0, 0.0, 50.0, 0.0));
    context.max_size = image_decode_size(object.matrix());
    let reduced = CpdfPageImageCache::new()
        .get_or_decode_data_with_context(&object, context)
        .unwrap();
    assert_eq!((reduced.bitmap.width(), reduced.bitmap.height()), (128, 64));
}

#[test]
fn drawn_size_growth_reloads_reduced_planes_and_keeps_full_source_shortcut() {
    let object = jpeg();
    let data = CpdfDocPageData::new();
    let cache = CpdfPageImageCache::new();
    let mut context = CpdfImageLoadContext::new(&data, None);
    context.max_size = image_decode_size(CfxMatrix::new(64.0, 0.0, 0.0, 32.0, 0.0, 0.0));
    let small = cache
        .get_or_decode_data_with_context(&object, context)
        .unwrap()
        .bitmap;
    assert_eq!((small.width(), small.height()), (64, 32));
    context.max_size = image_decode_size(CfxMatrix::new(128.0, 0.0, 0.0, 64.0, 0.0, 0.0));
    let medium = cache
        .get_or_decode_data_with_context(&object, context)
        .unwrap()
        .bitmap;
    assert!(!Rc::ptr_eq(&small, &medium));
    assert_eq!((medium.width(), medium.height()), (128, 64));
    assert!(Rc::ptr_eq(
        &medium,
        &cache
            .get_or_decode_data_with_context(&object, context)
            .unwrap()
            .bitmap
    ));
    context.max_size = CfxSize::new(512, 256);
    let full = cache
        .get_or_decode_data_with_context(&object, context)
        .unwrap()
        .bitmap;
    assert_eq!((full.width(), full.height()), (256, 128));
    context.max_size = CfxSize::new(1024, 512);
    assert!(Rc::ptr_eq(
        &full,
        &cache
            .get_or_decode_data_with_context(&object, context)
            .unwrap()
            .bitmap
    ));
}

#[test]
fn tiny_jpx_draw_uses_available_detail_instead_of_disappearing() {
    let mut bytes = crate::fxcodec::jpx::empty_packet_codestream(1, 1);
    for offset in [8, 12, 24, 28] {
        bytes[offset..offset + 4].copy_from_slice(&32u32.to_be_bytes());
    }
    let object = image(32, 32, bytes, b"JPXDecode");
    let data = CpdfDocPageData::new();
    let cache = CpdfPageImageCache::new();
    let mut context = CpdfImageLoadContext::new(&data, None);
    context.max_size = image_decode_size(CfxMatrix::default());
    // Five levels are requested, but this valid codestream has none to skip.
    let decoded = cache
        .get_or_decode_data_with_context(&object, context)
        .unwrap();
    assert_eq!((decoded.bitmap.width(), decoded.bitmap.height()), (32, 32));
    // Repeating the original request reuses its successful fallback plane.
    assert!(Rc::ptr_eq(
        &decoded.bitmap,
        &cache
            .get_or_decode_data_with_context(&object, context)
            .unwrap()
            .bitmap
    ));
    // Cache identity records the requested level, before JPX fallback. A new
    // request must establish its own decode proof even when both ultimately
    // use level zero; their available pixels must remain identical.
    context.max_size = CfxSize::new(64, 64);
    let full = cache
        .get_or_decode_data_with_context(&object, context)
        .unwrap();
    assert!(!Rc::ptr_eq(&decoded.bitmap, &full.bitmap));
    assert_eq!((full.bitmap.width(), full.bitmap.height()), (32, 32));
    assert_eq!(
        decoded.bitmap.realize().unwrap().buffer(),
        full.bitmap.realize().unwrap().buffer()
    );
    assert!(Rc::ptr_eq(
        &full.bitmap,
        &cache
            .get_or_decode_data_with_context(&object, context)
            .unwrap()
            .bitmap
    ));
}
