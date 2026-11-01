// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Full-resolution reuse is a decode-input contract, not a target-size shortcut.
//! These tests load actual streams and compare cache results with fresh decodes.

use super::*;
use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::parser::{
    cpdf_array::CpdfArray,
    cpdf_dictionary::CpdfDictionary,
    cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef},
    cpdf_name::CpdfName,
    cpdf_object::retain_object,
    cpdf_reference::CpdfReference,
    cpdf_scalar_object::CpdfScalarObject,
    cpdf_stream::CpdfStream,
};
use crate::fxcrt::fx_coordinates::CfxSize;
use crate::fxge::dib::{
    cfx_dibbase::convert_to, cstretchengine::StretchSource, fx_dib::FxdibFormat,
};

struct Fixture {
    object: CpdfImageObject,
    // Indirect profile/mask references retain their real holder for decoding.
    _objects: HolderRef,
}

fn dictionary(width: i32, height: i32, bits: i32, space: ObjectRef) -> CpdfDictionary {
    let mut dictionary = CpdfDictionary::new();
    for (key, value) in [
        (b"Width".as_slice(), width),
        (b"Height", height),
        (b"BitsPerComponent", bits),
    ] {
        dictionary.set_new_for(key, CpdfScalarObject::number_from_signed(value));
    }
    dictionary.set_for(b"ColorSpace", Some(space));
    dictionary
}

fn name(value: &[u8]) -> ObjectRef {
    retain_object(CpdfName::new(None, value))
}

fn icc_rgb(objects: &HolderRef) -> ObjectRef {
    let mut dictionary = CpdfDictionary::new();
    dictionary.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
    let stream = retain_object(CpdfStream::from_data_and_dict(
        moxcms::ColorProfile::new_adobe_rgb().encode().unwrap(),
        retain_object(dictionary),
    ));
    let number = objects.borrow_mut().add_indirect_object(stream);
    let mut array = CpdfArray::new();
    array.append_new_name(b"ICCBased");
    array.append_new(CpdfReference::new(objects, number));
    retain_object(array)
}

fn fixture(bytes: Vec<u8>, dictionary: CpdfDictionary, objects: HolderRef) -> Fixture {
    let stream = retain_object(CpdfStream::from_data_and_dict(
        bytes,
        retain_object(dictionary),
    ));
    let mut object = CpdfImageObject::new(0);
    object.set_image(Some(Rc::new(CpdfImage::from_stream(stream, true).unwrap())));
    Fixture {
        object,
        _objects: objects,
    }
}

fn icc_jpeg() -> Fixture {
    let objects = CpdfIndirectObjectHolder::new();
    let mut dictionary = dictionary(120, 120, 8, icc_rgb(&objects));
    dictionary.set_new_name_for(b"Filter", b"DCTDecode");
    fixture(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/mona_lisa.jpg"
        ))
        .to_vec(),
        dictionary,
        objects,
    )
}

fn rgb(bitmap: &CpdfImageBitmap) -> Vec<u8> {
    let bitmap = if bitmap.format() == FxdibFormat::Bgr {
        bitmap.realize()
    } else {
        convert_to(bitmap, FxdibFormat::Bgr)
    }
    .unwrap();
    (0..bitmap.height())
        .flat_map(|y| bitmap.scanline(y).unwrap()[..bitmap.width() as usize * 3].to_vec())
        .collect()
}

fn load(
    cache: &CpdfPageImageCache,
    image: &Fixture,
    context: CpdfImageLoadContext<'_>,
) -> CpdfImageData {
    cache
        .get_or_decode_data_with_context(&image.object, context)
        .unwrap()
}

#[test]
fn full_icc_jpeg_reuses_pixels_for_larger_and_unbounded_requests_without_retention_growth() {
    let image = icc_jpeg();
    let page_data = CpdfDocPageData::new();
    let mut context = CpdfImageLoadContext::new(&page_data, None);
    context.max_size = CfxSize::new(1200, 1600);
    let cache = CpdfPageImageCache::new();
    let original = load(&cache, &image, context);
    assert_eq!(
        (original.bitmap.width(), original.bitmap.height()),
        (120, 120)
    );
    assert!(matches!(
        original.bitmap.as_ref(),
        CpdfImageBitmap::Bitmap(_)
    ));
    let expected = rgb(&load(&CpdfPageImageCache::new(), &image, context).bitmap);
    assert!(expected.iter().any(|&byte| byte != 0));
    let retained = cache.cache_size.get();
    for size in [
        CfxSize::new(2400, 3200),
        CfxSize::new(0, 0),
        CfxSize::new(1200, 1600),
    ] {
        context.max_size = size;
        let repeated = load(&cache, &image, context);
        assert!(Rc::ptr_eq(&original.bitmap, &repeated.bitmap));
        assert_eq!(rgb(&repeated.bitmap), expected);
        assert_eq!(cache.cache_size.get(), retained);
    }
}

#[test]
fn changed_decode_flags_reload_full_size_planes_and_preserve_fresh_decode_colors() {
    for change in 0..3 {
        let objects = CpdfIndirectObjectHolder::new();
        let (bits, bytes) = if change == 0 {
            (1, vec![0x80])
        } else {
            (8, vec![64, 127, 193, 30])
        };
        let image = fixture(bytes, dictionary(1, 1, bits, name(b"DeviceCMYK")), objects);
        let page_data = CpdfDocPageData::new();
        let mut context = CpdfImageLoadContext::new(&page_data, None);
        context.max_size = CfxSize::new(100, 100);
        if change == 1 {
            context.load_mask = true;
        }
        if change == 2 {
            context.group_family = ColorSpaceFamily::DeviceCmyk;
        }
        let cache = CpdfPageImageCache::new();
        let first = load(&cache, &image, context);
        match change {
            0 => context.std_cs = true,
            1 => context.group_family = ColorSpaceFamily::DeviceCmyk,
            2 => context.load_mask = true,
            _ => unreachable!(),
        }
        context.max_size = CfxSize::new(200, 200);
        let changed = load(&cache, &image, context);
        assert!(
            !Rc::ptr_eq(&first.bitmap, &changed.bitmap),
            "condition {change}"
        );
        assert_eq!(
            rgb(&changed.bitmap),
            rgb(&load(&CpdfPageImageCache::new(), &image, context).bitmap)
        );
        assert_ne!(
            rgb(&first.bitmap),
            rgb(&changed.bitmap),
            "condition {change}"
        );
        context.max_size = CfxSize::new(400, 400);
        let repeated = load(&cache, &image, context);
        if change == 0 {
            // Standard CMYK palette loading mutates the shared conversion
            // state during decoding. The conservative epoch proof excludes
            // this case rather than assuming no callback changed mid-decode.
            assert!(!Rc::ptr_eq(&changed.bitmap, &repeated.bitmap));
        } else {
            assert!(Rc::ptr_eq(&changed.bitmap, &repeated.bitmap));
        }
        assert_eq!(
            rgb(&repeated.bitmap),
            rgb(&load(&CpdfPageImageCache::new(), &image, context).bitmap)
        );
    }
}

#[test]
fn balanced_stock_cmyk_conversion_changes_reload_the_full_size_plane_once() {
    use crate::fpdfapi::page::cpdf_colorspace::{cmyk_conversion_revision, get_stock_cs_for_name};

    let image = fixture(
        vec![64, 127, 193, 30],
        dictionary(1, 1, 8, name(b"DeviceCMYK")),
        CpdfIndirectObjectHolder::new(),
    );
    let page_data = CpdfDocPageData::new();
    let mut context = CpdfImageLoadContext::new(&page_data, None);
    context.max_size = CfxSize::new(100, 100);
    assert!(!context.std_cs);
    let cache = CpdfPageImageCache::new();
    let original = load(&cache, &image, context);
    assert!(matches!(
        original.bitmap.as_ref(),
        CpdfImageBitmap::Bitmap(_)
    ));
    let pixels = rgb(&original.bitmap);
    let stock = get_stock_cs_for_name(b"DeviceCMYK").unwrap();
    assert!(!stock.data().is_std_conversion_enabled());
    let before = cmyk_conversion_revision().unwrap();
    stock.enable_std_conversion(true);
    assert!(stock.data().is_std_conversion_enabled());
    stock.enable_std_conversion(false);
    assert!(!stock.data().is_std_conversion_enabled());
    assert!(cmyk_conversion_revision().unwrap() > before);

    // The final Boolean state is unchanged, but the conversion epoch is not.
    // Retain the old plane so pointer inequality cannot be address recycling.
    context.max_size = CfxSize::new(200, 200);
    let reloaded = load(&cache, &image, context);
    assert!(!Rc::ptr_eq(&original.bitmap, &reloaded.bitmap));
    assert_eq!(rgb(&reloaded.bitmap), pixels);
    assert_eq!(
        rgb(&reloaded.bitmap),
        rgb(&load(&CpdfPageImageCache::new(), &image, context).bitmap)
    );
    let stable = cmyk_conversion_revision();
    context.max_size = CfxSize::new(400, 400);
    let repeated = load(&cache, &image, context);
    assert!(Rc::ptr_eq(&reloaded.bitmap, &repeated.bitmap));
    assert_eq!(cmyk_conversion_revision(), stable);
    assert_eq!(rgb(&repeated.bitmap), pixels);
}

#[test]
fn named_color_changes_in_both_resource_scopes_and_in_place_do_not_reuse_old_pixels() {
    for (form_scope, request) in [(false, 1), (true, 1), (false, 100), (true, 100)] {
        let objects = CpdfIndirectObjectHolder::new();
        let image = fixture(
            vec![16, 128, 240],
            dictionary(1, 1, 8, name(b"Color0")),
            objects,
        );
        let mut spaces = CpdfDictionary::new();
        spaces.set_new_name_for(b"Color0", b"DeviceRGB");
        let mut resources = CpdfDictionary::new();
        let spaces = resources.set_new_for(b"ColorSpace", spaces);
        let resources = retain_object(resources);
        let page_data = CpdfDocPageData::new();
        let mut context = CpdfImageLoadContext::new(&page_data, None);
        if form_scope {
            context.form_resources = Some(&resources);
        } else {
            context.page_resources = Some(&resources);
        }
        context.max_size = CfxSize::new(request, request);
        let cache = CpdfPageImageCache::new();
        let first = load(&cache, &image, context);
        assert_eq!(rgb(&first.bitmap), [240, 128, 16]);
        // Resource identity stays the same; its resolved decoding space changes.
        spaces
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_name_for(b"Color0", b"DeviceGray");
        context.max_size = CfxSize::new(request, request);
        let changed = load(&cache, &image, context);
        assert!(!Rc::ptr_eq(&first.bitmap, &changed.bitmap));
        assert_eq!(rgb(&changed.bitmap), [16; 3]);
        assert_eq!(
            rgb(&changed.bitmap),
            rgb(&load(&CpdfPageImageCache::new(), &image, context).bitmap)
        );
        context.max_size = CfxSize::new(request, request);
        assert!(Rc::ptr_eq(
            &changed.bitmap,
            &load(&cache, &image, context).bitmap
        ));
    }
}

#[test]
fn full_external_soft_masks_use_their_own_dimensions_and_keep_matte_and_planes() {
    let objects = CpdfIndirectObjectHolder::new();
    let mut mask_dict = dictionary(2, 2, 8, name(b"DeviceGray"));
    let mut matte = CpdfArray::new();
    for value in [0, 0, 0] {
        matte.append_new(CpdfScalarObject::number_from_signed(value));
    }
    mask_dict.set_new_for(b"Matte", matte);
    let mask = retain_object(CpdfStream::from_data_and_dict(
        vec![0, 64, 128, 255],
        retain_object(mask_dict),
    ));
    let number = objects.borrow_mut().add_indirect_object(mask);
    let mut dictionary = dictionary(4, 3, 8, icc_rgb(&objects));
    dictionary.set_new_for(b"SMask", CpdfReference::new(&objects, number));
    let image = fixture(vec![20, 100, 200].repeat(12), dictionary, objects);
    let page_data = CpdfDocPageData::new();
    let mut context = CpdfImageLoadContext::new(&page_data, None);
    context.max_size = CfxSize::new(100, 100);
    let cache = CpdfPageImageCache::new();
    let first = load(&cache, &image, context);
    assert_eq!(first.matte_color, 0);
    let mask = first.mask.as_ref().unwrap();
    assert_eq!((mask.width(), mask.height()), (2, 2));
    assert_eq!(&mask.scanline(0)[..2], &[0, 64]);
    assert_eq!(&mask.scanline(1)[..2], &[128, 255]);
    context.max_size = CfxSize::new(200, 200);
    let next = load(&cache, &image, context);
    assert!(Rc::ptr_eq(&first.bitmap, &next.bitmap));
    assert!(Rc::ptr_eq(mask, next.mask.as_ref().unwrap()));
    assert_eq!(first.matte_color, next.matte_color);
}

#[test]
fn absent_failed_soft_mask_never_proves_the_complete_full_size_decode() {
    let objects = CpdfIndirectObjectHolder::new();
    let bad_mask = retain_object(CpdfStream::from_data_and_dict(
        vec![128],
        retain_object(dictionary(0, 1, 8, name(b"DeviceGray"))),
    ));
    let number = objects.borrow_mut().add_indirect_object(bad_mask);
    let mut dictionary = dictionary(1, 1, 8, icc_rgb(&objects));
    dictionary.set_new_for(b"SMask", CpdfReference::new(&objects, number));
    let image = fixture(vec![20, 100, 200], dictionary, objects);
    let page_data = CpdfDocPageData::new();
    let mut context = CpdfImageLoadContext::new(&page_data, None);
    context.max_size = CfxSize::new(100, 100);
    let cache = CpdfPageImageCache::new();
    let first = load(&cache, &image, context);
    assert!(first.mask.is_none());
    context.max_size = CfxSize::new(200, 200);
    let next = load(&cache, &image, context);
    assert!(next.mask.is_none());
    assert!(!Rc::ptr_eq(&first.bitmap, &next.bitmap));
    assert_eq!(rgb(&first.bitmap), rgb(&next.bitmap));
}

#[test]
fn reduced_jpeg_and_jpx_are_redecoded_when_more_detail_is_requested() {
    let jpeg = icc_jpeg();
    let objects = CpdfIndirectObjectHolder::new();
    let mut dictionary = dictionary(16, 16, 8, name(b"DeviceGray"));
    dictionary.set_new_name_for(b"Filter", b"JPXDecode");
    let jpx = fixture(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/gray.jp2"
        ))
        .to_vec(),
        dictionary,
        objects,
    );
    for (image, small, large, full) in [(jpeg, 30, 60, 120), (jpx, 4, 8, 16)] {
        let page_data = CpdfDocPageData::new();
        let cache = CpdfPageImageCache::new();
        let mut context = CpdfImageLoadContext::new(&page_data, None);
        context.max_size = CfxSize::new(small, small);
        let reduced = load(&cache, &image, context);
        assert_eq!(
            (reduced.bitmap.width(), reduced.bitmap.height()),
            (small, small)
        );
        context.max_size = CfxSize::new(large, large);
        let larger = load(&cache, &image, context);
        assert!(!Rc::ptr_eq(&reduced.bitmap, &larger.bitmap));
        assert_eq!(
            (larger.bitmap.width(), larger.bitmap.height()),
            (large, large)
        );
        assert_eq!(
            rgb(&larger.bitmap),
            rgb(&load(&CpdfPageImageCache::new(), &image, context).bitmap)
        );
        context.max_size = CfxSize::new(0, 0);
        let complete = load(&cache, &image, context);
        assert_eq!(
            (complete.bitmap.width(), complete.bitmap.height()),
            (full, full)
        );
        context.max_size = CfxSize::new(full * 4, full * 4);
        assert!(Rc::ptr_eq(
            &complete.bitmap,
            &load(&cache, &image, context).bitmap
        ));
    }
}

#[test]
fn jpeg_fallback_reduction_matching_declared_dimensions_still_reloads_more_detail() {
    let bytes = libjpeg_turbo_rs::Encoder::new(
        &vec![128; 16 * 16],
        16,
        16,
        libjpeg_turbo_rs::PixelFormat::Grayscale,
    )
    .quality(90)
    .encode()
    .unwrap();
    let objects = CpdfIndirectObjectHolder::new();
    // One actual component forces the native header fallback from the declared
    // three-component ICC space. The fallback retains the initial size hint.
    let mut dictionary = dictionary(8, 8, 8, icc_rgb(&objects));
    dictionary.set_new_name_for(b"Filter", b"DCTDecode");
    let image = fixture(bytes, dictionary, objects);
    let page_data = CpdfDocPageData::new();
    let cache = CpdfPageImageCache::new();
    let mut context = CpdfImageLoadContext::new(&page_data, None);
    context.max_size = CfxSize::new(4, 4);
    let small = load(&cache, &image, context);
    assert_eq!((small.bitmap.width(), small.bitmap.height()), (8, 8));
    assert_eq!(image.object.image().unwrap().pixel_width(), 8);
    context.max_size = CfxSize::new(32, 32);
    let large = load(&cache, &image, context);
    assert!(!Rc::ptr_eq(&small.bitmap, &large.bitmap));
    assert_eq!((large.bitmap.width(), large.bitmap.height()), (16, 16));
    assert_eq!(
        rgb(&large.bitmap),
        rgb(&load(&CpdfPageImageCache::new(), &image, context).bitmap)
    );
}

#[test]
fn huge_lazy_icc_planes_do_not_gain_full_resolution_reuse_or_realize_sixty_megabytes() {
    let objects = CpdfIndirectObjectHolder::new();
    let image = fixture(
        vec![20, 100, 200],
        dictionary(100_000, 200, 8, icc_rgb(&objects)),
        objects,
    );
    let page_data = CpdfDocPageData::new();
    let mut context = CpdfImageLoadContext::new(&page_data, None);
    context.max_size = CfxSize::new(200_000, 400);
    let cache = CpdfPageImageCache::new();
    let first = load(&cache, &image, context);
    assert_eq!(
        (first.bitmap.width(), first.bitmap.height()),
        (100_000, 200)
    );
    assert_eq!(
        first.bitmap.pitch() * first.bitmap.height() as u32,
        60_000_000
    );
    assert!(matches!(first.bitmap.as_ref(), CpdfImageBitmap::Dib(_)));
    assert_eq!(first.bitmap.estimated_image_memory_burden(), 0);
    context.max_size = CfxSize::new(300_000, 600);
    let second = load(&cache, &image, context);
    assert!(matches!(second.bitmap.as_ref(), CpdfImageBitmap::Dib(_)));
    assert!(!Rc::ptr_eq(&first.bitmap, &second.bitmap));
    assert_eq!(second.bitmap.estimated_image_memory_burden(), 0);
    assert_eq!(cache.cache_size.get(), 0);
}

#[test]
fn huge_lazy_device_rgb_retains_the_existing_full_size_reuse() {
    // A truncated raw stream avoids a 60 MB fixture; no scanlines need to be
    // realized to exercise the existing lazy DeviceRGB cache representation.
    let image = fixture(
        vec![20, 100, 200],
        dictionary(100_000, 200, 8, name(b"DeviceRGB")),
        CpdfIndirectObjectHolder::new(),
    );
    let page_data = CpdfDocPageData::new();
    let cache = CpdfPageImageCache::new();
    let mut context = CpdfImageLoadContext::new(&page_data, None);
    context.max_size = CfxSize::new(200_000, 400);
    let first = load(&cache, &image, context);
    assert!(matches!(first.bitmap.as_ref(), CpdfImageBitmap::Dib(_)));
    let retained = cache.cache_size.get();
    context.max_size = CfxSize::new(300_000, 600);
    let second = load(&cache, &image, context);
    assert!(Rc::ptr_eq(&first.bitmap, &second.bitmap));
    assert_eq!(cache.cache_size.get(), retained);
}

#[test]
fn unresolved_mask_references_never_establish_extra_full_size_reuse() {
    for key in [b"SMask".as_slice(), b"Mask".as_slice()] {
        let objects = CpdfIndirectObjectHolder::new();
        let mut dict = dictionary(4, 3, 8, name(b"DeviceRGB"));
        dict.set_new_for(key, CpdfReference::new(&objects, 99));
        let image = fixture(vec![20, 100, 200].repeat(12), dict, objects);
        let page_data = CpdfDocPageData::new();
        let cache = CpdfPageImageCache::new();
        let mut context = CpdfImageLoadContext::new(&page_data, None);
        context.load_mask = true;
        context.max_size = CfxSize::new(8, 6);
        let first = load(&cache, &image, context);
        assert!(first.mask.is_none());
        context.max_size = CfxSize::new(16, 12);
        let second = load(&cache, &image, context);
        assert!(second.mask.is_none());
        assert!(!Rc::ptr_eq(&first.bitmap, &second.bitmap));
        assert_eq!(
            rgb(&second.bitmap),
            rgb(&load(&CpdfPageImageCache::new(), &image, context).bitmap)
        );
        // No hit may claim unchanged complete inputs for an unresolved mask,
        // even when the decoded parent satisfies the small drawn-size request.
        context.max_size = CfxSize::new(4, 3);
        assert!(!Rc::ptr_eq(
            &second.bitmap,
            &load(&cache, &image, context).bitmap
        ));
    }
}

#[test]
fn reduced_cache_planes_still_match_decode_conditions_on_size_valid_hits() {
    let image = icc_jpeg();
    let data = CpdfDocPageData::new();
    let mut context = CpdfImageLoadContext::new(&data, None);
    context.max_size = CfxSize::new(30, 30);
    let cache = CpdfPageImageCache::new();
    let first = load(&cache, &image, context);
    assert_eq!((first.bitmap.width(), first.bitmap.height()), (30, 30));
    context.std_cs = true;
    let second = load(&cache, &image, context);
    assert!(!Rc::ptr_eq(&first.bitmap, &second.bitmap));
    assert_eq!(
        rgb(&second.bitmap),
        rgb(&load(&CpdfPageImageCache::new(), &image, context).bitmap)
    );
    assert!(Rc::ptr_eq(
        &second.bitmap,
        &load(&cache, &image, context).bitmap
    ));
}

#[test]
fn failed_unbounded_reload_cannot_make_an_old_reduced_plane_unconditional() {
    let image = icc_jpeg();
    let dictionary = image.object.image().unwrap().dict();
    let space = dictionary
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .unwrap()
        .get_object_for(b"ColorSpace")
        .unwrap();
    dictionary
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_name_for(b"ColorSpace", b"Photo");
    let mut spaces = CpdfDictionary::new();
    spaces.set_for(b"Photo", Some(space));
    let mut resources = CpdfDictionary::new();
    resources.set_new_for(b"ColorSpace", spaces);
    let resources = retain_object(resources);
    let data = CpdfDocPageData::new();
    let cache = CpdfPageImageCache::new();
    let mut context = CpdfImageLoadContext::new(&data, Some(&resources));
    context.max_size = CfxSize::new(30, 30);
    let small = load(&cache, &image, context);
    assert_eq!((small.bitmap.width(), small.bitmap.height()), (30, 30));
    let retained = cache.cache_size.get();
    context.page_resources = None;
    context.max_size = CfxSize::new(0, 0);
    assert!(cache
        .get_or_decode_data_with_context(&image.object, context)
        .is_none());
    assert_eq!(cache.cache_size.get(), retained);
    context.page_resources = Some(&resources);
    context.max_size = CfxSize::new(60, 60);
    let grown = load(&cache, &image, context);
    assert!(!Rc::ptr_eq(&small.bitmap, &grown.bitmap));
    assert_eq!((grown.bitmap.width(), grown.bitmap.height()), (60, 60));
    assert_eq!(
        rgb(&grown.bitmap),
        rgb(&load(&CpdfPageImageCache::new(), &image, context).bitmap)
    );
}

#[test]
fn jpeg_and_jpx_decode_pixels_are_independent_of_zoom_history() {
    let jpeg = icc_jpeg();
    let objects = CpdfIndirectObjectHolder::new();
    let mut dictionary = dictionary(16, 16, 8, name(b"DeviceGray"));
    dictionary.set_new_name_for(b"Filter", b"JPXDecode");
    let jpx = fixture(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/gray.jp2"
        ))
        .to_vec(),
        dictionary,
        objects,
    );
    for (image, small, large, full) in [(jpeg, 30, 60, 120), (jpx, 4, 8, 16)] {
        let page_data = CpdfDocPageData::new();
        for requests in [
            [full * 4, small, large, 1, small, 0],
            [1, large, small, full * 4, small, 0],
        ] {
            let cache = CpdfPageImageCache::new();
            for size in requests {
                let mut context = CpdfImageLoadContext::new(&page_data, None);
                context.max_size = CfxSize::new(size, size);
                let fresh = load(&CpdfPageImageCache::new(), &image, context);
                let historical = load(&cache, &image, context);
                assert_eq!(
                    (historical.bitmap.width(), historical.bitmap.height()),
                    (fresh.bitmap.width(), fresh.bitmap.height()),
                    "size {size} after {requests:?}"
                );
                assert_eq!(rgb(&historical.bitmap), rgb(&fresh.bitmap));
                assert_eq!(
                    cache.cache_size.get(),
                    cache
                        .entries
                        .borrow()
                        .values()
                        .map(|entry| entry.borrow().estimated_size())
                        .sum::<u32>()
                );
                assert!(Rc::ptr_eq(
                    &historical.bitmap,
                    &load(&cache, &image, context).bitmap
                ));
            }
            cache.cache_optimization(0);
            assert!(cache.entries.borrow().is_empty());
            assert_eq!(cache.cache_size.get(), 0);
        }
        // A different extent in the same reduction bucket still reuses decoding.
        let cache = CpdfPageImageCache::new();
        let mut context = CpdfImageLoadContext::new(&page_data, None);
        context.max_size = CfxSize::new(small, small);
        let first = load(&cache, &image, context);
        context.max_size = CfxSize::new(small - 1, small - 1);
        assert!(Rc::ptr_eq(
            &first.bitmap,
            &load(&cache, &image, context).bitmap
        ));
    }
}
