// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! ResetBitmapForImage and the native cross-document Start guard.
use super::*;
use crate::{
    fpdfapi::{
        page::cpdf_docpagedata::CpdfDocPageData,
        parser::{
            cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument,
            cpdf_indirect_object_holder::CpdfIndirectObjectHolder, cpdf_object::retain_object,
            cpdf_reference::CpdfReference, cpdf_scalar_object::CpdfScalarObject,
            cpdf_stream::CpdfStream,
        },
    },
    fpdfsdk::fpdf_editimg::{new_image_object, set_bitmap},
    fxge::dib::{cfx_dibitmap::CfxDibitmap, cstretchengine::StretchSource, fx_dib::FxdibFormat},
};

fn gray_stream(value: u8) -> ObjectRef {
    let mut dict = CpdfDictionary::new();
    for (key, value) in [
        (b"Width".as_slice(), 1),
        (b"Height", 1),
        (b"BitsPerComponent", 8),
    ] {
        dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
    }
    dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
    retain_object(CpdfStream::from_data_and_dict(
        vec![value],
        retain_object(dict),
    ))
}

#[test]
fn reset_invalidates_only_cached_color_preserving_entry_mask_age_and_active_planes() {
    let objects = CpdfIndirectObjectHolder::new();
    let mask = gray_stream(128);
    let mask_number = objects.borrow_mut().add_indirect_object(mask);
    let stream = gray_stream(64);
    stream
        .borrow()
        .as_any()
        .downcast_ref::<CpdfStream>()
        .unwrap()
        .get_dict()
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(b"SMask", CpdfReference::new(&objects, mask_number));
    let image = Rc::new(CpdfImage::from_stream(stream.clone(), false).unwrap());
    let cache = CpdfPageImageCache::new();
    let data = CpdfDocPageData::new();
    let mut context = CpdfImageLoadContext::new(&data, None);
    context.load_mask = true;
    assert!(!cache.start_get_cached_bitmap(image.clone(), context));
    let entry = cache.current_entry().unwrap();
    let color = entry.borrow().bitmap.clone().unwrap();
    let mask = entry.borrow().mask.clone().unwrap();
    let age = entry.borrow().time;
    let requests = cache.time_count();
    assert_eq!(cache.cache_size.get(), entry.borrow().estimated_size());
    cache.reset_bitmap_for_image(&image);
    assert_eq!(cache.entries.borrow().len(), 1);
    assert!(Rc::ptr_eq(&entry, &cache.current_entry().unwrap()));
    assert!(entry.borrow().bitmap.is_none());
    assert!(Rc::ptr_eq(entry.borrow().mask.as_ref().unwrap(), &mask));
    assert_eq!(entry.borrow().time, age);
    assert_eq!(cache.time_count(), requests);
    assert_eq!(
        cache.cache_size.get(),
        mask.estimated_image_memory_burden() as u32
    );
    assert!(Rc::ptr_eq(&cache.detach_current_bitmap().unwrap(), &color));
    assert!(Rc::ptr_eq(&cache.detach_current_mask().unwrap(), &mask));
    stream
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfStream>()
        .unwrap()
        .set_data(&[192]);
    assert!(!cache.start_get_cached_bitmap(image, context));
    assert!(Rc::ptr_eq(&entry, &cache.current_entry().unwrap()));
    let replacement = cache.detach_current_bitmap().unwrap();
    assert!(!Rc::ptr_eq(&replacement, &color));
    assert_eq!(color.scanline(0)[0], 64);
    assert_eq!(replacement.scanline(0)[0], 192);
}

#[test]
fn real_document_owners_reject_foreign_images_without_leaking_prior_planes_or_cycles() {
    let mut first_doc = CpdfDocument::new();
    let mut second_doc = CpdfDocument::new();
    first_doc.create_new_doc();
    second_doc.create_new_doc();
    let first_data = first_doc.page_data_ref();
    let second_data = second_doc.page_data_ref();
    let mut first = new_image_object(&first_data);
    let mut second = new_image_object(&second_data);
    let mut pixels = CfxDibitmap::create(1, 1, FxdibFormat::Bgr, 0).unwrap();
    pixels.clear(0xff123456);
    assert!(set_bitmap(&mut first, &pixels));
    pixels.clear(0xffabcdef);
    assert!(set_bitmap(&mut second, &pixels));
    let first_cache = CpdfPageImageCache::with_page_data(&first_data);
    let second_cache = CpdfPageImageCache::with_page_data(&second_data);
    let original = first_cache
        .get_or_decode_data_with_context(&first, CpdfImageLoadContext::new(&first_data, None))
        .unwrap();
    assert!(!second_cache.start_get_cached_bitmap(
        second.image().unwrap(),
        CpdfImageLoadContext::new(&second_data, None)
    ));
    assert!(second_cache
        .current_entry()
        .unwrap()
        .borrow()
        .current_bitmap
        .is_some());
    let requests = second_cache.time_count();
    assert!(!second_cache.start_get_cached_bitmap(
        first.image().unwrap(),
        CpdfImageLoadContext::new(&second_data, None)
    ));
    assert!(second_cache.detach_current_bitmap().is_none());
    assert!(second_cache.detach_current_mask().is_none());
    assert_eq!(second_cache.time_count(), requests);
    assert_eq!(second_cache.entries.borrow().len(), 1);
    assert!(second_cache
        .get_or_decode_data_with_context(&first, CpdfImageLoadContext::new(&second_data, None))
        .is_none());
    let again = first_cache
        .get_or_decode_data_with_context(&first, CpdfImageLoadContext::new(&first_data, None))
        .unwrap();
    assert!(Rc::ptr_eq(&original.bitmap, &again.bitmap));
    assert_eq!(&again.bitmap.scanline(0)[..3], &[0x56, 0x34, 0x12]);
    let owner = first
        .image()
        .unwrap()
        .document_owner_identity()
        .unwrap()
        .clone();
    drop(first_doc);
    assert!(owner.upgrade().is_none());
    // Retained planes and image/cache weak identities do not retain the PDF
    // document/object graph; existing decoded pixels remain independently owned.
    assert_eq!(&original.bitmap.scanline(0)[..3], &[0x56, 0x34, 0x12]);
}

#[test]
fn replaced_streams_remain_owned_until_cache_eviction_and_never_alias_old_pixels() {
    let mut document = CpdfDocument::new();
    document.create_new_doc();
    let data = document.page_data_ref();
    let cache = CpdfPageImageCache::with_page_data(&data);
    let mut object = new_image_object(&data);
    let mut pixels = CfxDibitmap::create(3, 2, FxdibFormat::Bgr, 0).unwrap();
    pixels.clear(0xff123456);
    assert!(set_bitmap(&mut object, &pixels));
    let image = object.image().unwrap();
    let context = CpdfImageLoadContext::new(&data, None);
    let original = cache
        .get_or_decode_data_with_context(&object, context)
        .unwrap();
    let original_row: [u8; 9] = [0x56, 0x34, 0x12, 0x56, 0x34, 0x12, 0x56, 0x34, 0x12];
    assert_eq!(&original.bitmap.scanline(0)[..9], &original_row);
    let mut replaced_sources = Vec::new();
    for (index, color) in [0xff0000ff, 0xff00ff00, 0xffff0000, 0xffabcdef]
        .into_iter()
        .enumerate()
    {
        // Match replacements with no supplied page and with an explicitly
        // invalidated loaded page. Weak observers do not keep old keys alive.
        if index != 0 {
            cache.reset_bitmap_for_image(&image);
        }
        replaced_sources.push(Rc::downgrade(&image.stream()));
        pixels.clear(color);
        assert!(set_bitmap(&mut object, &pixels));
        assert!(
            replaced_sources
                .iter()
                .all(|source| source.upgrade().is_some()),
            "the native retained map key keeps every replaced source identity alive"
        );
        let current = cache
            .get_or_decode_data_with_context(&object, context)
            .unwrap();
        let expected = [
            (color & 255) as u8,
            (color >> 8 & 255) as u8,
            (color >> 16 & 255) as u8,
        ];
        for row in 0..2 {
            for actual in current.bitmap.scanline(row)[..9].chunks_exact(3) {
                assert_eq!(
                    actual, &expected,
                    "replacement {index} must decode its own color"
                );
            }
        }
        assert!(!Rc::ptr_eq(&original.bitmap, &current.bitmap));
        assert_eq!(&original.bitmap.scanline(0)[..9], &original_row);
    }
    cache.cache_optimization(0);
    assert!(replaced_sources
        .iter()
        .all(|source| source.upgrade().is_none()));
    assert!(cache.entries.borrow().is_empty());
    // A retained decoded bitmap survives source-key eviction independently.
    assert_eq!(&original.bitmap.scanline(0)[..9], &original_row);
    let reloaded = cache
        .get_or_decode_data_with_context(&object, context)
        .unwrap();
    assert_eq!(&reloaded.bitmap.scanline(0)[..3], &[0xef, 0xcd, 0xab]);
}
