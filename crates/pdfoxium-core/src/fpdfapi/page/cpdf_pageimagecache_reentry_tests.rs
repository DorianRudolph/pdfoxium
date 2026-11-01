// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Lazy resource resolution may synchronously render another cached image.
//! The outer request must regain its own cursor before its caller detaches.

use super::*;
use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::parser::{
    cpdf_dictionary::CpdfDictionary, cpdf_name::CpdfName, cpdf_object::retain_object,
    cpdf_reference::CpdfReference, cpdf_scalar_object::CpdfScalarObject, cpdf_stream::CpdfStream,
};
use crate::fxcrt::fx_coordinates::CfxSize;
use crate::fxge::dib::cstretchengine::StretchSource;

fn gray_image(sample: u8) -> CpdfImageObject {
    let mut dictionary = CpdfDictionary::new();
    for (name, value) in [
        (b"Width".as_slice(), 1),
        (b"Height".as_slice(), 1),
        (b"BitsPerComponent".as_slice(), 8),
    ] {
        dictionary.set_new_for(name, CpdfScalarObject::number_from_signed(value));
    }
    dictionary.set_new_name_for(b"ColorSpace", b"DeviceGray");
    let stream = retain_object(CpdfStream::from_data_and_dict(
        vec![sample],
        retain_object(dictionary),
    ));
    let mut object = CpdfImageObject::new(0);
    object.set_image(Some(Rc::new(
        CpdfImage::from_stream(stream, false).unwrap(),
    )));
    object
}

fn check_resolution_reentry(evict_outer: bool) {
    let cache = Rc::new(CpdfPageImageCache::new());
    let data = Rc::new(CpdfDocPageData::new());
    let outer = gray_image(128);
    let inner = Rc::new(gray_image(64));
    let mut context = CpdfImageLoadContext::new(&data, None);
    context.max_size = CfxSize::new(2, 2);
    let original = cache
        .get_or_decode_data_with_context(&outer, context)
        .unwrap()
        .bitmap;
    assert_eq!((original.width(), original.height()), (1, 1));
    assert_eq!(original.scanline(0)[0], 128);

    // The existing bitmap is smaller than this request. Its only possible hit
    // is the extra full-resolution path, which must resolve the lazy default.
    let stream = outer.image().unwrap().stream();
    let key = Rc::as_ptr(&stream) as *const () as usize;
    let entry = cache.entries.borrow().get(&key).cloned().unwrap();
    let holder = CpdfIndirectObjectHolder::new();
    let calls = Rc::new(Cell::new(0));
    holder.borrow_mut().set_parser(Some(Rc::new({
        let cache = Rc::clone(&cache);
        let data = Rc::clone(&data);
        let inner = Rc::clone(&inner);
        let calls = Rc::clone(&calls);
        let stream = Rc::clone(&stream);
        move |number| {
            assert_eq!(number, 99);
            calls.set(calls.get() + 1);
            // Reentry must happen with neither the map, outer entry, nor
            // cursor borrowed by the outer resource-resolution call.
            assert!(cache.entries.try_borrow_mut().is_ok());
            assert!(entry.try_borrow_mut().is_ok());
            assert!(cache.current.try_borrow_mut().is_ok());
            if evict_outer {
                cache.clear_image_cache_entry(&stream);
            }
            let mut nested_context = CpdfImageLoadContext::new(&data, None);
            nested_context.max_size = CfxSize::new(2, 2);
            let nested = cache
                .get_or_decode_data_with_context(&inner, nested_context)
                .unwrap();
            assert_eq!(nested.bitmap.scanline(0)[0], 64);
            // The nested caller consumed its plane; leaving this cursor
            // selected would make the outer caller receive None.
            assert!(cache.detach_current_bitmap().is_none());
            Some(retain_object(CpdfName::new(None, b"DeviceGray")))
        }
    })));
    let mut color_spaces = CpdfDictionary::new();
    color_spaces.set_new_for(b"DefaultGray", CpdfReference::new(&holder, 99));
    let mut resources = CpdfDictionary::new();
    resources.set_new_for(b"ColorSpace", color_spaces);
    let resources = retain_object(resources);
    context.page_resources = Some(&resources);

    let result = cache
        .get_or_decode_data_with_context(&outer, context)
        .expect("outer request must detach its own image after nested decoding");
    assert_eq!(calls.get(), 1);
    assert_eq!(result.bitmap.scanline(0)[0], 128);
    assert!(result.mask.is_none());
    assert_eq!(Rc::ptr_eq(&original, &result.bitmap), !evict_outer);
    assert_eq!(cache.entries.borrow().len(), 2);
    assert!(cache.entries.borrow().contains_key(&key));
    let accounted: u32 = cache
        .entries
        .borrow()
        .values()
        .map(|entry| entry.borrow().estimated_size())
        .sum();
    assert_eq!(cache.cache_size.get(), accounted);
}

#[test]
fn full_resolution_resource_callback_restores_outer_cursor_after_nested_decode() {
    check_resolution_reentry(false);
}

#[test]
fn evicted_full_resolution_candidate_is_decoded_again_after_resource_callback() {
    check_resolution_reentry(true);
}

#[test]
fn first_decode_restores_cursor_and_accounts_retained_outer_entry_after_reentry() {
    let cache = Rc::new(CpdfPageImageCache::new());
    let data = Rc::new(CpdfDocPageData::new());
    let outer = gray_image(128);
    let inner = Rc::new(gray_image(64));
    let holder = CpdfIndirectObjectHolder::new();
    holder.borrow_mut().set_parser(Some(Rc::new({
        let cache = Rc::clone(&cache);
        let data = Rc::clone(&data);
        let inner = Rc::clone(&inner);
        move |number| {
            assert_eq!(number, 99);
            assert!(cache.entries.try_borrow_mut().is_ok());
            assert!(cache.current.try_borrow_mut().is_ok());
            let nested = cache
                .get_or_decode_data_with_context(&inner, CpdfImageLoadContext::new(&data, None))
                .unwrap();
            assert_eq!(nested.bitmap.scanline(0)[0], 64);
            Some(retain_object(CpdfName::new(None, b"DeviceGray")))
        }
    })));
    let mut spaces = CpdfDictionary::new();
    spaces.set_new_for(b"DefaultGray", CpdfReference::new(&holder, 99));
    let mut resources = CpdfDictionary::new();
    resources.set_new_for(b"ColorSpace", spaces);
    let resources = retain_object(resources);
    let context = CpdfImageLoadContext::new(&data, Some(&resources));
    let decoded = cache
        .get_or_decode_data_with_context(&outer, context)
        .expect("the outer decode must select its own result after a nested image read");
    assert_eq!(decoded.bitmap.scanline(0)[0], 128);
    assert_eq!(cache.entries.borrow().len(), 2);
    let accounted: u32 = cache
        .entries
        .borrow()
        .values()
        .map(|entry| entry.borrow().estimated_size())
        .sum();
    assert_eq!(cache.cache_size.get(), accounted);
}
