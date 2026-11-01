// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Page-owned bitmap caching and continuation from
//! `core/fpdfapi/page/cpdf_pageimagecache.{h,cpp}`.

#[cfg(test)]
use super::cpdf_colorspace::ColorSpaceFamily;
use super::cpdf_dib::{CpdfDib, ImageCacheInputs, LoadState};
use super::cpdf_image::{CpdfImage, CpdfImageBitmap, CpdfImageData, CpdfImageLoadContext};
use super::cpdf_imageobject::CpdfImageObject;
use crate::fpdfapi::parser::{
    cpdf_indirect_object_holder::CpdfIndirectObjectHolder, cpdf_object::ObjectRef,
};
use crate::fxcrt::pauseindicator_iface::PauseIndicatorIface;
use crate::fxge::dib::cstretchengine::StretchSource;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

type EntryRef = Rc<RefCell<Entry>>;

struct Entry {
    // Native's map key is RetainPtr<const CPDF_Stream>. The image can replace
    // its stream after editing; retain this exact cached identity so its
    // address cannot be reused while an old decoded plane remains in the map.
    source_stream: ObjectRef,
    image: Rc<CpdfImage>,
    bitmap: Option<Rc<CpdfImageBitmap>>,
    mask: Option<Rc<CpdfImageBitmap>>,
    current_bitmap: Option<Rc<CpdfImageBitmap>>,
    current_mask: Option<Rc<CpdfImageBitmap>>,
    pending: Option<Box<CpdfDib>>,
    matte_color: u32,
    max_size_set: bool,
    decode_inputs: Option<ImageCacheInputs>,
    full_size_bitmap: bool,
    accounted_size: u32,
    time: u32,
}

impl Entry {
    fn new(image: Rc<CpdfImage>) -> Self {
        Self {
            source_stream: image.stream(),
            image,
            bitmap: None,
            mask: None,
            current_bitmap: None,
            current_mask: None,
            pending: None,
            matte_color: 0,
            max_size_set: false,
            decode_inputs: None,
            full_size_bitmap: false,
            accounted_size: 0,
            time: 0,
        }
    }
    fn estimated_size(&self) -> u32 {
        let bitmap = self
            .bitmap
            .as_ref()
            .map_or(0, |bitmap| bitmap.estimated_image_memory_burden() as u32);
        bitmap.wrapping_add(
            self.mask
                .as_ref()
                .map_or(0, |mask| mask.estimated_image_memory_burden() as u32),
        )
    }
    fn is_cache_valid(&self, size: crate::fxcrt::fx_coordinates::CfxSize) -> bool {
        if !self.max_size_set {
            return true;
        }
        if size.width == 0 && size.height == 0 {
            return false;
        }
        self.bitmap
            .as_ref()
            .is_some_and(|bitmap| bitmap.width() >= size.width && bitmap.height() >= size.height)
    }
}

// MaybeOwned<Entry>: a new entry belongs to the current request until it is
// inserted into the map. Current references to map entries are non-owning.
enum CurrentEntry {
    Owned(EntryRef),
    Cached(Weak<RefCell<Entry>>),
}
impl CurrentEntry {
    fn get(&self) -> Option<EntryRef> {
        match self {
            Self::Owned(entry) => Some(Rc::clone(entry)),
            Self::Cached(entry) => entry.upgrade(),
        }
    }
}

#[derive(Default)]
pub struct CpdfPageImageCache {
    document_owner: Option<Weak<RefCell<CpdfIndirectObjectHolder>>>,
    entries: RefCell<HashMap<usize, EntryRef>>,
    current: RefCell<Option<CurrentEntry>>,
    time_count: Cell<u32>,
    cache_size: Cell<u32>,
}

impl CpdfPageImageCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_page_data(page_data: &super::cpdf_docpagedata::CpdfDocPageData) -> Self {
        Self {
            document_owner: page_data.document_owner_identity(),
            ..Self::default()
        }
    }
    fn accepts_image(&self, image: &CpdfImage) -> bool {
        match (
            self.document_owner.as_ref(),
            image.document_owner_identity(),
        ) {
            (Some(page), Some(image)) => Weak::ptr_eq(page, image),
            (None, None) => true,
            _ => false,
        }
    }
    /// ResetBitmapForImage keeps the cache entry, age, mask and current planes.
    /// Only the cached color plane is invalidated; active renders can retain
    /// their original plane while the next request decodes the edited stream.
    pub fn reset_bitmap_for_image(&self, image: &CpdfImage) {
        if !self.accepts_image(image) {
            return;
        }
        let key = Rc::as_ptr(&image.stream()) as *const () as usize;
        let Some(entry) = self.entries.borrow().get(&key).cloned() else {
            return;
        };
        let mut entry = entry.borrow_mut();
        entry.bitmap = None;
        entry.decode_inputs = None;
        entry.full_size_bitmap = false;
        self.update_accounting(&mut entry);
    }

    fn current_entry(&self) -> Option<EntryRef> {
        self.current.borrow().as_ref().and_then(CurrentEntry::get)
    }

    pub fn start_get_cached_bitmap(
        &self,
        image: Rc<CpdfImage>,
        context: CpdfImageLoadContext<'_>,
    ) -> bool {
        // Cross-document images may be supplied by the embedding API. Never
        // decode them in this page's resource scope or expose a prior request's
        // current planes after rejecting the new request.
        if !self.accepts_image(&image) {
            self.current.borrow_mut().take();
            return false;
        }
        let key = Rc::as_ptr(&image.stream()) as *const () as usize;
        let found = self.entries.borrow().get(&key).cloned();
        let entry = if let Some(entry) = found {
            *self.current.borrow_mut() = Some(CurrentEntry::Cached(Rc::downgrade(&entry)));
            entry
        } else {
            let entry = Rc::new(RefCell::new(Entry::new(image)));
            *self.current.borrow_mut() = Some(CurrentEntry::Owned(Rc::clone(&entry)));
            entry
        };
        // Drawn-size requests are often smaller than the source plane. Every
        // size-valid hit must still match the actual successful decode inputs;
        // otherwise changing resource/group scopes could reuse wrong colors.
        // The proof also includes JPEG/JPX's requested reduction level: reusing
        // higher-detail pixels after zooming out changes the resampling result.
        let candidate = {
            let value = entry.borrow();
            value.bitmap.is_some()
                && value.decode_inputs.is_some()
                && (value.is_cache_valid(context.max_size) || value.full_size_bitmap)
        };
        let hit = if candidate {
            let image = Rc::clone(&entry.borrow().image);
            let inputs = ImageCacheInputs::for_request(&image, context);
            // Resolution may reenter and render/evict another image. Restore
            // this cursor and recheck both size and input proof afterwards.
            let retained = self
                .entries
                .borrow()
                .get(&key)
                .is_some_and(|cached| Rc::ptr_eq(cached, &entry));
            *self.current.borrow_mut() = Some(if retained {
                CurrentEntry::Cached(Rc::downgrade(&entry))
            } else {
                CurrentEntry::Owned(Rc::clone(&entry))
            });
            let value = entry.borrow();
            retained
                && Rc::ptr_eq(&value.source_stream, &image.stream())
                && value.bitmap.is_some()
                && (value.is_cache_valid(context.max_size) || value.full_size_bitmap)
                && inputs.as_ref().is_some_and(|inputs| {
                    value
                        .decode_inputs
                        .as_ref()
                        .is_some_and(|cached| cached.matches(inputs))
                })
        } else {
            false
        };
        {
            let mut value = entry.borrow_mut();
            if hit {
                value.current_bitmap = value.bitmap.as_ref().map(Rc::clone);
                value.current_mask = value.mask.as_ref().map(Rc::clone);
                drop(value);
                self.complete_request(&entry, false);
                return false;
            }
            value.current_bitmap = None;
            value.pending = None;
        }
        let image = Rc::clone(&entry.borrow().image);
        // Stream/codec callbacks run without map, entry or current-state borrows.
        let prepared = image.start_dib(context, true);
        self.restore_request_cursor(&entry);
        match prepared {
            Some((source, LoadState::Continue)) => {
                entry.borrow_mut().pending = Some(source);
                return true;
            }
            Some((source, LoadState::Success)) => self.finish_decoded(&entry, source),
            _ => {}
        }
        // Entry::StartGetCachedBitmap reports kFail after synchronous loading,
        // even when its decoded bitmap was successfully cached.
        self.complete_request(&entry, true);
        false
    }

    pub fn continue_load(&self, pause: Option<&mut dyn PauseIndicatorIface>) -> bool {
        let Some(entry) = self.current_entry() else {
            return false;
        };
        let source = entry.borrow_mut().pending.take();
        let Some(mut source) = source else {
            return false;
        };
        let state = source.continue_load(pause);
        self.restore_request_cursor(&entry);
        match state {
            LoadState::Continue => {
                entry.borrow_mut().pending = Some(source);
                return true;
            }
            LoadState::Success => self.finish_decoded(&entry, source),
            LoadState::Fail => {}
        }
        self.complete_request(&entry, true);
        false
    }

    fn finish_decoded(&self, entry: &EntryRef, mut source: Box<CpdfDib>) {
        let inputs = source.cache_inputs();
        let max_size_set = source.cache_size_was_set();
        let full_detail = source.has_full_source_detail();
        let lazy_device = source.has_unmasked_device_color_space();
        let matte_color = source.matte_color();
        let mask = source.detach_mask().map(Rc::new);
        let time = self.time_count.get();
        let realize =
            super::image_memory::image_memory_limits().realizes(source.pitch(), source.height());
        if !realize {
            source.retain_scanline_history();
        }
        let bitmap = Rc::new(CpdfImageBitmap::Dib(source));
        let bitmap = if realize {
            bitmap.make_cached()
        } else {
            Some(bitmap)
        };
        // None means keep the old cached mask; Some(None) means its realization
        // failed and clears that mask, matching ContinueGetCachedBitmap().
        let mask = mask.map(|mask| mask.make_cached());
        let mut entry = entry.borrow_mut();
        let complete_planes = bitmap.is_some()
            && match &mask {
                Some(mask) => mask.is_some(),
                None => entry.mask.is_none(),
            };
        entry.decode_inputs =
            inputs.filter(|inputs| complete_planes && inputs.conversion_is_unchanged());
        // Keep the landed full-source optimization, including its old lazy
        // DeviceRGB/Gray exception. Other lazy planes need a size-valid hit.
        entry.full_size_bitmap = full_detail
            && entry.decode_inputs.is_some()
            && (matches!(bitmap.as_deref(), Some(CpdfImageBitmap::Bitmap(_))) || lazy_device);
        entry.max_size_set = max_size_set;
        entry.matte_color = matte_color;
        entry.time = time;
        entry.bitmap = bitmap;
        if let Some(mask) = mask {
            entry.mask = mask;
        }
        entry.current_bitmap = entry.bitmap.as_ref().map(Rc::clone);
        entry.current_mask = entry.mask.as_ref().map(Rc::clone);
    }

    fn restore_request_cursor(&self, entry: &EntryRef) {
        let key = Rc::as_ptr(&entry.borrow().source_stream) as *const () as usize;
        let retained = self
            .entries
            .borrow()
            .get(&key)
            .is_some_and(|cached| Rc::ptr_eq(cached, entry));
        *self.current.borrow_mut() = Some(if retained {
            CurrentEntry::Cached(Rc::downgrade(entry))
        } else {
            CurrentEntry::Owned(Rc::clone(entry))
        });
    }

    fn complete_request(&self, entry: &EntryRef, add_size: bool) {
        // Decoder and lazy bitmap callbacks can select/evict another entry.
        // Restore this request before publishing its planes or accounting them.
        self.restore_request_cursor(entry);
        self.time_count.set(self.time_count.get().wrapping_add(1));
        let owned = matches!(self.current.borrow().as_ref(), Some(CurrentEntry::Owned(current)) if Rc::ptr_eq(current, entry));
        if owned {
            let key = Rc::as_ptr(&entry.borrow().source_stream) as *const () as usize;
            if let Some(replaced) = self.entries.borrow_mut().insert(key, Rc::clone(entry)) {
                let mut replaced = replaced.borrow_mut();
                self.cache_size
                    .set(self.cache_size.get().wrapping_sub(replaced.accounted_size));
                replaced.accounted_size = 0;
            }
            *self.current.borrow_mut() = Some(CurrentEntry::Cached(Rc::downgrade(entry)));
        }
        if add_size {
            self.update_accounting(&mut entry.borrow_mut());
        }
    }

    fn update_accounting(&self, entry: &mut Entry) {
        let size = entry.estimated_size();
        // Reloads replace the retained plane rather than adding another one.
        // Failed loads keep their previous plane and must not count it twice.
        self.cache_size.set(
            self.cache_size
                .get()
                .wrapping_sub(entry.accounted_size)
                .wrapping_add(size),
        );
        entry.accounted_size = size;
    }

    pub fn detach_current_bitmap(&self) -> Option<Rc<CpdfImageBitmap>> {
        self.current_entry()?.borrow_mut().current_bitmap.take()
    }
    pub fn detach_current_mask(&self) -> Option<Rc<CpdfImageBitmap>> {
        self.current_entry()?.borrow_mut().current_mask.take()
    }
    pub fn current_matte_color(&self) -> u32 {
        self.current_entry()
            .map_or(0, |entry| entry.borrow().matte_color)
    }

    pub fn get_or_decode(
        &self,
        object: &CpdfImageObject,
        resources: Option<&ObjectRef>,
    ) -> Option<Rc<CpdfImageBitmap>> {
        self.get_or_decode_data(object, resources)
            .map(|data| data.bitmap)
    }
    pub fn get_or_decode_data(
        &self,
        object: &CpdfImageObject,
        resources: Option<&ObjectRef>,
    ) -> Option<CpdfImageData> {
        let page_data = super::cpdf_docpagedata::CpdfDocPageData::new();
        self.get_or_decode_data_with_context(
            object,
            CpdfImageLoadContext::new(&page_data, resources),
        )
    }
    pub fn get_or_decode_data_with_context(
        &self,
        object: &CpdfImageObject,
        context: CpdfImageLoadContext<'_>,
    ) -> Option<CpdfImageData> {
        if self.start_get_cached_bitmap(object.image()?, context) {
            while self.continue_load(None) {}
        }
        let bitmap = self.detach_current_bitmap();
        let mask = self.detach_current_mask();
        let matte_color = self.current_matte_color();
        Some(CpdfImageData {
            bitmap: bitmap?,
            mask,
            matte_color,
        })
    }

    fn clear_current_if(&self, entry: &EntryRef) {
        if self
            .current_entry()
            .is_some_and(|current| Rc::ptr_eq(&current, entry))
        {
            self.current.borrow_mut().take();
        }
    }
    pub fn clear_image_cache_entry(&self, stream: &ObjectRef) {
        let key = Rc::as_ptr(stream) as *const () as usize;
        if let Some(entry) = self.entries.borrow_mut().remove(&key) {
            let mut value = entry.borrow_mut();
            self.cache_size
                .set(self.cache_size.get().wrapping_sub(value.accounted_size));
            value.accounted_size = 0;
            drop(value);
            self.clear_current_if(&entry);
        }
    }
    pub fn cache_optimization(&self, limit_bytes: i32) {
        let limit = limit_bytes as u32;
        if self.cache_size.get() <= limit {
            return;
        }
        let mut entries = self.entries.borrow_mut();
        let mut by_age: Vec<_> = entries
            .iter()
            .map(|(&key, entry)| (key, entry.borrow().time))
            .collect();
        by_age.sort_unstable_by_key(|(_, time)| *time);
        let count = by_age.len();
        if self.time_count.get() == u32::MAX {
            for (time, (key, _)) in by_age.iter().enumerate() {
                entries.get(key).unwrap().borrow_mut().time = time as u32;
            }
            self.time_count.set(count as u32);
        }
        let mut index = 0;
        while index < count && (index + 15 < count || self.cache_size.get() > limit) {
            let entry = entries.remove(&by_age[index].0).unwrap();
            let mut value = entry.borrow_mut();
            self.cache_size
                .set(self.cache_size.get().wrapping_sub(value.accounted_size));
            value.accounted_size = 0;
            drop(value);
            self.clear_current_if(&entry);
            index += 1;
        }
    }
    pub fn time_count(&self) -> u32 {
        self.time_count.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_image::CpdfImage;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

    fn image(sample: u8) -> CpdfImageObject {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        let image = Rc::new(
            CpdfImage::from_stream(
                retain_object(CpdfStream::from_data_and_dict(
                    vec![sample],
                    retain_object(dict),
                )),
                false,
            )
            .unwrap(),
        );
        let mut object = CpdfImageObject::new(0);
        object.set_image(Some(image));
        object
    }

    #[test]
    fn lazy_flate_row_history_is_included_in_cache_accounting() {
        use super::super::image_memory::{
            image_memory_limits, set_image_memory_limits, ImageMemoryLimits,
        };
        struct Restore(ImageMemoryLimits);
        impl Drop for Restore {
            fn drop(&mut self) {
                set_image_memory_limits(self.0);
            }
        }
        let _restore = Restore(image_memory_limits());
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(32));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(8));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        dict.set_new_name_for(b"Filter", b"FlateDecode");
        let mut object = CpdfImageObject::new(0);
        object.set_image(Some(Rc::new(
            CpdfImage::from_stream(
                retain_object(CpdfStream::from_data_and_dict(
                    crate::fxcodec::flate::encode(&[128; 256]),
                    retain_object(dict),
                )),
                false,
            )
            .unwrap(),
        )));
        set_image_memory_limits(ImageMemoryLimits {
            cached_image_bytes: 0,
        });
        let cache = CpdfPageImageCache::new();
        let lazy = cache.get_or_decode_data(&object, None).unwrap().bitmap;
        assert!(matches!(lazy.as_ref(), CpdfImageBitmap::Dib(_)));
        assert_eq!(lazy.estimated_image_memory_burden(), 64);
        assert_eq!(cache.cache_size.get(), 64);
        assert_eq!(lazy.scanline(0).as_ref(), &[128; 32]);
        cache.cache_optimization(0);
        assert_eq!(cache.cache_size.get(), 0);
        assert!(cache.entries.borrow().is_empty());
        set_image_memory_limits(ImageMemoryLimits::UNLIMITED);
        let realized = CpdfPageImageCache::new()
            .get_or_decode_data(&object, None)
            .unwrap()
            .bitmap;
        assert!(matches!(realized.as_ref(), CpdfImageBitmap::Bitmap(_)));
        assert_eq!(
            realized.estimated_image_memory_burden(),
            256 + crate::fxge::dib::cfx_dibbase::estimated_image_memory_burden(realized.format())
        );
        assert_eq!(realized.scanline(0).as_ref(), &[128; 32]);
    }

    #[test]
    fn image_limits_control_new_realization_without_changing_pixels_or_existing_entries() {
        use super::super::image_memory::{
            image_memory_limits, set_image_memory_limits, ImageMemoryLimits,
        };
        use crate::fxge::dib::cstretchengine::StretchSource;
        struct Restore(ImageMemoryLimits);
        impl Drop for Restore {
            fn drop(&mut self) {
                set_image_memory_limits(self.0);
            }
        }
        let _restore = Restore(image_memory_limits());
        let image = image(128);
        let cache = CpdfPageImageCache::new();
        set_image_memory_limits(ImageMemoryLimits::UNLIMITED);
        let realized = cache.get_or_decode_data(&image, None).unwrap().bitmap;
        assert!(matches!(realized.as_ref(), CpdfImageBitmap::Bitmap(_)));
        let expected = realized.scanline(0).to_vec();
        set_image_memory_limits(ImageMemoryLimits {
            cached_image_bytes: 0,
        });
        let retained = cache.get_or_decode_data(&image, None).unwrap().bitmap;
        assert!(Rc::ptr_eq(&realized, &retained));
        let lazy_cache = CpdfPageImageCache::new();
        let lazy = lazy_cache.get_or_decode_data(&image, None).unwrap().bitmap;
        assert!(matches!(lazy.as_ref(), CpdfImageBitmap::Dib(_)));
        // Lazy rows omit the realized bitmap's alignment padding.
        assert_eq!(lazy.scanline(0).as_ref(), &expected[..1]);
        // Changing policy does not convert an already cached lazy image either.
        set_image_memory_limits(ImageMemoryLimits::UNLIMITED);
        assert!(Rc::ptr_eq(
            &lazy,
            &lazy_cache.get_or_decode_data(&image, None).unwrap().bitmap
        ));
        let refreshed = CpdfPageImageCache::new()
            .get_or_decode_data(&image, None)
            .unwrap()
            .bitmap;
        assert!(matches!(refreshed.as_ref(), CpdfImageBitmap::Bitmap(_)));
        assert_eq!(refreshed.scanline(0).as_ref(), expected.as_slice());
    }

    #[test]
    fn reuses_bitmap_and_evicts_oldest_entry() {
        let first = image(10);
        let second = image(20);
        let cache = CpdfPageImageCache::new();
        let original = cache.get_or_decode(&first, None).unwrap();
        assert!(Rc::ptr_eq(
            &original,
            &cache.get_or_decode(&first, None).unwrap()
        ));
        let later = cache.get_or_decode(&second, None).unwrap();
        cache.cache_optimization(later.estimated_image_memory_burden() as i32);
        assert!(Rc::ptr_eq(
            &later,
            &cache.get_or_decode(&second, None).unwrap()
        ));
        assert!(!Rc::ptr_eq(
            &original,
            &cache.get_or_decode(&first, None).unwrap()
        ));
    }
    #[test]
    fn hits_preserve_age_and_optimization_caps_entries_and_renumbers_rollover() {
        let cache = CpdfPageImageCache::new();
        let images: Vec<_> = (0..17).map(image).collect();
        let handles: Vec<_> = images
            .iter()
            .map(|image| cache.get_or_decode(image, None).unwrap())
            .collect();
        // A hit of the oldest entry changes the request counter, not its age.
        cache.get_or_decode(&images[0], None).unwrap();
        let size = cache.cache_size.get();
        cache.cache_optimization(-1);
        assert_eq!(cache.entries.borrow().len(), 17);
        cache.time_count.set(u32::MAX);
        // This budget needs only one removal, but PDFium retains at most 15
        // entries whenever an over-budget optimization starts.
        cache.cache_optimization((size - 1) as i32);
        assert_eq!(cache.entries.borrow().len(), 15);
        assert_eq!(cache.time_count(), 17);
        assert_eq!(Rc::strong_count(&handles[0]), 1);
        assert_eq!(Rc::strong_count(&handles[1]), 1);
        assert!(Rc::ptr_eq(
            &handles[2],
            &cache.get_or_decode(&images[2], None).unwrap()
        ));
    }
    #[test]
    fn huge_images_keep_pdf_scanlines_while_small_images_and_masks_are_realized() {
        use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
        use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
        use crate::fxge::dib::cstretchengine::StretchSource;
        let objects = CpdfIndirectObjectHolder::new();
        let small = image(128);
        let mask = small.image().unwrap().stream();
        objects.borrow_mut().add_indirect_object(Rc::clone(&mask));
        let mut dict = CpdfDictionary::new();
        for (key, value) in [
            (b"Width".as_slice(), 100_000),
            (b"Height", 200),
            (b"BitsPerComponent", 8),
        ] {
            dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        dict.set_new_for(
            b"SMask",
            CpdfReference::new(&objects, mask.borrow().obj_num()),
        );
        // Exactly kHugeImageSize in the destination format: the input may be
        // truncated, so this checks retention without a 60 MB allocation.
        let stream = retain_object(CpdfStream::from_data_and_dict(
            vec![1, 2, 3],
            retain_object(dict),
        ));
        let weak_stream = Rc::downgrade(&stream);
        let mut object = CpdfImageObject::new(0);
        object.set_image(Some(Rc::new(
            CpdfImage::from_stream(stream, false).unwrap(),
        )));
        let cache = CpdfPageImageCache::new();
        let data = cache.get_or_decode_data(&object, None).unwrap();
        assert!(matches!(data.bitmap.as_ref(), CpdfImageBitmap::Dib(_)));
        assert!(matches!(
            data.mask.as_deref(),
            Some(CpdfImageBitmap::Bitmap(_))
        ));
        assert_eq!(data.bitmap.estimated_image_memory_burden(), 0);
        drop(object);
        drop(cache);
        assert!(weak_stream.upgrade().is_some());
        assert_eq!(&data.bitmap.scanline(0)[..6], &[3, 2, 1, 0, 0, 0]);
        drop(data);
        assert!(weak_stream.upgrade().is_none());
    }
    fn jpeg_image() -> CpdfImageObject {
        let mut dict = CpdfDictionary::new();
        for (key, value) in [
            (b"Width".as_slice(), 120),
            (b"Height", 120),
            (b"BitsPerComponent", 8),
        ] {
            dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        dict.set_new_name_for(b"Filter", b"DCTDecode");
        let image = Rc::new(
            CpdfImage::from_stream(
                retain_object(CpdfStream::from_data_and_dict(
                    include_bytes!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../upstream/pdfium/testing/resources/mona_lisa.jpg"
                    ))
                    .to_vec(),
                    retain_object(dict),
                )),
                false,
            )
            .unwrap(),
        );
        let mut object = CpdfImageObject::new(0);
        object.set_image(Some(image));
        object
    }

    #[test]
    fn jpeg_cache_reloads_for_larger_or_unbounded_requests_and_reuses_full_resolution() {
        use crate::fxcrt::fx_coordinates::CfxSize;
        let object = jpeg_image();
        let page_data = super::super::cpdf_docpagedata::CpdfDocPageData::new();
        let mut context = CpdfImageLoadContext::new(&page_data, None);
        let cache = CpdfPageImageCache::new();
        context.max_size = CfxSize::new(30, 30);
        let small = cache
            .get_or_decode_data_with_context(&object, context)
            .unwrap()
            .bitmap;
        assert_eq!((small.width(), small.height()), (30, 30));
        context.max_size = CfxSize::new(29, 29);
        assert!(Rc::ptr_eq(
            &small,
            &cache
                .get_or_decode_data_with_context(&object, context)
                .unwrap()
                .bitmap
        ));
        context.max_size = CfxSize::new(60, 60);
        let larger = cache
            .get_or_decode_data_with_context(&object, context)
            .unwrap()
            .bitmap;
        assert_eq!((larger.width(), larger.height()), (60, 60));
        assert!(!Rc::ptr_eq(&small, &larger));
        assert_eq!(
            cache.cache_size.get(),
            larger.estimated_image_memory_burden() as u32
        );
        context.max_size = CfxSize::new(0, 0);
        let full = cache
            .get_or_decode_data_with_context(&object, context)
            .unwrap()
            .bitmap;
        assert_eq!((full.width(), full.height()), (120, 120));
        context.max_size = CfxSize::new(240, 240);
        assert!(Rc::ptr_eq(
            &full,
            &cache
                .get_or_decode_data_with_context(&object, context)
                .unwrap()
                .bitmap
        ));
    }

    #[test]
    fn full_device_jpeg_is_reused_for_page_sized_requests_without_growing_retention() {
        use crate::fxcrt::fx_coordinates::CfxSize;
        let object = jpeg_image();
        let page_data = super::super::cpdf_docpagedata::CpdfDocPageData::new();
        let mut context = CpdfImageLoadContext::new(&page_data, None);
        context.max_size = CfxSize::new(1200, 1600);
        let cache = CpdfPageImageCache::new();
        let original = cache
            .get_or_decode_data_with_context(&object, context)
            .unwrap()
            .bitmap;
        assert_eq!((original.width(), original.height()), (120, 120));
        let retained = cache.cache_size.get();
        for request in 0..1960 {
            context.max_size = match request % 3 {
                0 => CfxSize::new(1200, 1600),
                1 => CfxSize::new(2400, 3200),
                _ => CfxSize::new(0, 0),
            };
            let next = cache
                .get_or_decode_data_with_context(&object, context)
                .unwrap();
            assert!(Rc::ptr_eq(&original, &next.bitmap));
            assert!(next.mask.is_none());
        }
        assert_eq!(cache.entries.borrow().len(), 1);
        assert_eq!(cache.cache_size.get(), retained);
        cache.cache_optimization(0);
        assert!(cache.entries.borrow().is_empty());
        assert_eq!(cache.cache_size.get(), 0);
    }

    #[test]
    fn full_size_masked_device_images_reuse_both_planes() {
        use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
        use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
        use crate::fxcrt::fx_coordinates::CfxSize;
        use crate::fxge::dib::cstretchengine::StretchSource;
        for mask_key in [b"SMask".as_slice(), b"Mask".as_slice()] {
            let object = image(128);
            let mask = image(64);
            let objects = CpdfIndirectObjectHolder::new();
            let stream = mask.image().unwrap().stream();
            objects.borrow_mut().add_indirect_object(Rc::clone(&stream));
            object
                .image()
                .unwrap()
                .dict()
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_for(
                    mask_key,
                    CpdfReference::new(&objects, stream.borrow().obj_num()),
                );
            let page_data = super::super::cpdf_docpagedata::CpdfDocPageData::new();
            let mut context = CpdfImageLoadContext::new(&page_data, None);
            context.load_mask = true;
            context.max_size = CfxSize::new(2, 2);
            let cache = CpdfPageImageCache::new();
            let original = cache
                .get_or_decode_data_with_context(&object, context)
                .unwrap();
            assert!(original.mask.is_some());
            context.max_size = CfxSize::new(4, 4);
            let next = cache
                .get_or_decode_data_with_context(&object, context)
                .unwrap();
            assert!(Rc::ptr_eq(&original.bitmap, &next.bitmap));
            assert!(next.mask.is_some());
            assert_eq!(next.bitmap.scanline(0)[0], original.bitmap.scanline(0)[0]);
            assert_eq!(
                next.mask.unwrap().scanline(0)[0],
                original.mask.unwrap().scanline(0)[0]
            );
        }
    }

    #[test]
    fn unchanged_device_defaults_reuse_full_size_in_each_resource_scope() {
        use crate::fxcrt::fx_coordinates::CfxSize;
        let object = image(128);
        let mut spaces = CpdfDictionary::new();
        spaces.set_new_name_for(b"DefaultGray", b"DeviceGray");
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"ColorSpace", spaces);
        let resources = retain_object(resources);
        let page_data = super::super::cpdf_docpagedata::CpdfDocPageData::new();
        for form_scope in [false, true] {
            let cache = CpdfPageImageCache::new();
            let mut context = CpdfImageLoadContext::new(&page_data, None);
            if form_scope {
                context.form_resources = Some(&resources);
            } else {
                context.page_resources = Some(&resources);
            }
            context.max_size = CfxSize::new(2, 2);
            let first = cache
                .get_or_decode_data_with_context(&object, context)
                .unwrap();
            context.max_size = CfxSize::new(4, 4);
            let second = cache
                .get_or_decode_data_with_context(&object, context)
                .unwrap();
            assert!(Rc::ptr_eq(&first.bitmap, &second.bitmap));
        }
    }

    #[test]
    fn failed_resolution_reload_preserves_old_cache_for_smaller_requests() {
        use crate::fxcrt::fx_coordinates::CfxSize;
        let object = image(128);
        let image = object.image().unwrap();
        image
            .dict()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_name_for(b"ColorSpace", b"NamedGray");
        let mut spaces = CpdfDictionary::new();
        spaces.set_new_name_for(b"NamedGray", b"DeviceGray");
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"ColorSpace", spaces);
        let resources = retain_object(resources);
        let page_data = super::super::cpdf_docpagedata::CpdfDocPageData::new();
        let mut context = CpdfImageLoadContext::new(&page_data, Some(&resources));
        context.max_size = CfxSize::new(1, 1);
        let cache = CpdfPageImageCache::new();
        let original = cache
            .get_or_decode_data_with_context(&object, context)
            .unwrap()
            .bitmap;
        context.max_size = CfxSize::new(2, 2);
        context.page_resources = None;
        assert!(cache
            .get_or_decode_data_with_context(&object, context)
            .is_none());
        context.max_size = CfxSize::new(1, 1);
        context.page_resources = Some(&resources);
        assert!(Rc::ptr_eq(
            &original,
            &cache
                .get_or_decode_data_with_context(&object, context)
                .unwrap()
                .bitmap
        ));
    }

    #[test]
    fn pending_entry_is_owned_until_failure_and_eviction_clears_the_cursor() {
        let object = image(0);
        let image = object.image().unwrap();
        // A short nonempty stream is accepted by native DecodeSequential.
        // Use a complete symbol header with missing required data instead.
        image
            .stream()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfStream>()
            .unwrap()
            .set_data(&[0; 11]);
        image
            .dict()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_name_for(b"Filter", b"JBIG2Decode");
        let cache = CpdfPageImageCache::new();
        let page_data = super::super::cpdf_docpagedata::CpdfDocPageData::new();
        assert!(cache
            .start_get_cached_bitmap(image.clone(), CpdfImageLoadContext::new(&page_data, None)));
        assert!(cache.entries.borrow().is_empty());
        assert_eq!(cache.time_count(), 0);
        assert!(matches!(
            cache.current.borrow().as_ref(),
            Some(CurrentEntry::Owned(_))
        ));
        struct Pause;
        impl PauseIndicatorIface for Pause {
            fn need_to_pause_now(&mut self) -> bool {
                true
            }
        }
        assert!(cache.continue_load(Some(&mut Pause)));
        assert_eq!(cache.time_count(), 0);
        assert!(!cache.continue_load(None));
        assert_eq!(cache.time_count(), 1);
        assert_eq!(cache.entries.borrow().len(), 1);
        assert!(cache.detach_current_bitmap().is_none());
        let entry = Rc::downgrade(&cache.current_entry().unwrap());
        cache.clear_image_cache_entry(&image.stream());
        assert!(cache.current.borrow().is_none());
        assert!(entry.upgrade().is_none());
    }
}

#[cfg(test)]
#[path = "cpdf_pageimagecache_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
#[path = "cpdf_pageimagecache_edit_tests.rs"]
mod edit_tests;

#[cfg(test)]
#[path = "cpdf_pageimagecache_context_tests.rs"]
mod context_tests;

#[cfg(test)]
#[path = "cpdf_pageimagecache_reentry_tests.rs"]
mod reentry_tests;
