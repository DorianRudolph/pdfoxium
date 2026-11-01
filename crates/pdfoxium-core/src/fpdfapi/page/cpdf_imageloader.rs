// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Image start/continuation and retained plane detachment from
//! `core/fpdfapi/page/cpdf_imageloader.{h,cpp}`.

use super::cpdf_image::{CpdfImageBitmap, CpdfImageLoadContext};
use super::cpdf_imageobject::CpdfImageObject;
use super::cpdf_pageimagecache::CpdfPageImageCache;
use super::cpdf_transferfunc::CpdfTransferFunc;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
#[cfg(test)]
use crate::fxge::dib::cstretchengine::StretchSource;
use std::rc::Rc;

#[derive(Default)]
pub struct CpdfImageLoader<'a> {
    bitmap: Option<Rc<CpdfImageBitmap>>,
    mask: Option<Rc<CpdfImageBitmap>>,
    cached: bool,
    matte_color: u32,
    cache: Option<&'a CpdfPageImageCache>,
    image_object: Option<&'a CpdfImageObject>,
}

impl<'a> CpdfImageLoader<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` when the caller must continue an incremental decode.
    pub fn start(
        &mut self,
        image: &'a CpdfImageObject,
        cache: Option<&'a CpdfPageImageCache>,
        resources: Option<&ObjectRef>,
    ) -> bool {
        let page_data = super::cpdf_docpagedata::CpdfDocPageData::new();
        self.start_with_context(
            image,
            cache,
            CpdfImageLoadContext::new(&page_data, resources),
        )
    }

    pub fn start_with_context(
        &mut self,
        image: &'a CpdfImageObject,
        cache: Option<&'a CpdfPageImageCache>,
        context: CpdfImageLoadContext<'_>,
    ) -> bool {
        self.cache = cache;
        self.image_object = Some(image);
        let Some(source) = image.image() else {
            self.bitmap = None;
            self.mask = None;
            self.matte_color = 0;
            self.cached = cache.is_some();
            return false;
        };
        let should_continue = cache.map_or_else(
            || source.start_load_dib_base(context),
            |cache| cache.start_get_cached_bitmap(source.clone(), context),
        );
        if !should_continue {
            self.finish();
        }
        should_continue
    }

    pub fn continue_load(
        &mut self,
        pause: Option<&mut dyn crate::fxcrt::pauseindicator_iface::PauseIndicatorIface>,
    ) -> bool {
        let should_continue = if let Some(cache) = self.cache {
            cache.continue_load(pause)
        } else {
            self.image_object
                .and_then(CpdfImageObject::image)
                .is_some_and(|image| image.continue_load(pause))
        };
        if !should_continue {
            self.finish();
        }
        should_continue
    }

    fn finish(&mut self) {
        self.cached = self.cache.is_some();
        if let Some(cache) = self.cache {
            self.bitmap = cache.detach_current_bitmap();
            self.mask = cache.detach_current_mask();
            self.matte_color = cache.current_matte_color();
        } else if let Some(image) = self.image_object.and_then(CpdfImageObject::image) {
            self.bitmap = image.detach_bitmap();
            self.mask = image.detach_mask();
            self.matte_color = image.matte_color();
        }
    }

    pub fn translate_image(&mut self, transfer: Rc<CpdfTransferFunc>) -> Option<&CpdfImageBitmap> {
        debug_assert!(!transfer.identity());
        let translated = transfer.translate_image(Rc::clone(self.bitmap.as_ref()?))?;
        self.bitmap = Some(Rc::new(CpdfImageBitmap::Transfer(translated)));
        if self.cached {
            if let Some(mask) = &self.mask {
                self.mask = mask
                    .realize()
                    .map(|mask| Rc::new(CpdfImageBitmap::Bitmap(mask)));
            }
        }
        self.cached = false;
        self.bitmap.as_deref()
    }

    pub fn bitmap_ref(&self) -> Option<Rc<CpdfImageBitmap>> {
        self.bitmap.as_ref().map(Rc::clone)
    }

    pub fn bitmap(&self) -> Option<&CpdfImageBitmap> {
        self.bitmap.as_deref()
    }

    pub fn mask_ref(&self) -> Option<Rc<CpdfImageBitmap>> {
        self.mask.as_ref().map(Rc::clone)
    }
    pub fn mask(&self) -> Option<&CpdfImageBitmap> {
        self.mask.as_deref()
    }

    pub fn matte_color(&self) -> u32 {
        self.matte_color
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_image::CpdfImage;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::{
        CpdfIndirectObjectHolder, HolderRef,
    };
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

    fn masked_image(mask_width: i32) -> (CpdfImageObject, HolderRef) {
        let objects = CpdfIndirectObjectHolder::new();
        let mut mask = CpdfDictionary::new();
        for (key, value) in [
            (b"Width".as_slice(), mask_width),
            (b"Height", 1),
            (b"BitsPerComponent", 8),
        ] {
            mask.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        mask.set_new_name_for(b"ColorSpace", b"DeviceGray");
        let mut matte = CpdfArray::new();
        for _ in 0..3 {
            matte.append_new(CpdfScalarObject::number_from_signed(1));
        }
        mask.set_new_for(b"Matte", matte);
        let mask = retain_object(CpdfStream::from_data_and_dict(
            vec![128],
            retain_object(mask),
        ));
        let mut image = CpdfDictionary::new();
        for (key, value) in [
            (b"Width".as_slice(), 2),
            (b"Height", 1),
            (b"BitsPerComponent", 8),
        ] {
            image.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        image.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        objects.borrow_mut().add_indirect_object(mask.clone());
        image.set_new_for(
            b"SMask",
            CpdfReference::new(&objects, mask.borrow().obj_num()),
        );
        let image = Rc::new(
            CpdfImage::from_stream(
                retain_object(CpdfStream::from_data_and_dict(
                    vec![255, 127, 127, 127, 127, 255],
                    retain_object(image),
                )),
                false,
            )
            .unwrap(),
        );
        let mut object = CpdfImageObject::new(0);
        object.set_image(Some(image));
        (object, objects)
    }

    #[test]
    fn cache_keeps_independent_planes_and_transfer_detaches_its_mask_once() {
        let (object, _objects) = masked_image(1);
        let cache = CpdfPageImageCache::new();
        let original = cache.get_or_decode_data(&object, None).unwrap();
        assert_eq!(
            (
                original.bitmap.width(),
                original.mask.as_ref().unwrap().width()
            ),
            (2, 1)
        );
        assert_eq!(original.matte_color, 0x00ffffff);
        assert_eq!(
            original.bitmap.format(),
            crate::fxge::dib::fx_dib::FxdibFormat::Bgr
        );
        assert_eq!(
            &original.bitmap.scanline(0)[..6],
            &[127, 127, 255, 255, 127, 127]
        );
        let independent = object.get_independent_bitmap().unwrap();
        assert_eq!(&independent.buffer()[..6], &[127, 127, 255, 255, 127, 127]);
        let mut loader = CpdfImageLoader::new();
        assert!(!loader.start(&object, Some(&cache), None));
        assert!(Rc::ptr_eq(
            &loader.mask_ref().unwrap(),
            original.mask.as_ref().unwrap()
        ));
        let inverted = std::array::from_fn(|i| 255 - i as u8);
        let transfer = Rc::new(CpdfTransferFunc::new(false, inverted, inverted, inverted));
        loader.translate_image(Rc::clone(&transfer)).unwrap();
        let detached = loader.mask_ref().unwrap();
        assert!(!Rc::ptr_eq(&detached, original.mask.as_ref().unwrap()));
        loader.translate_image(Rc::clone(&transfer)).unwrap();
        assert!(Rc::ptr_eq(&detached, &loader.mask_ref().unwrap()));
        assert_eq!(
            original.bitmap.format(),
            crate::fxge::dib::fx_dib::FxdibFormat::Bgr
        );
        assert_eq!(
            &original.bitmap.scanline(0)[..6],
            &[127, 127, 255, 255, 127, 127]
        );
    }

    #[test]
    fn a_failed_separate_mask_does_not_discard_the_color_image() {
        let (object, _objects) = masked_image(0);
        let mut loader = CpdfImageLoader::new();
        assert!(!loader.start(&object, None, None));
        assert!(loader.bitmap().is_some());
        assert!(loader.mask().is_none());
    }

    #[test]
    fn pending_load_finishes_through_image_or_cache_and_cache_hits_are_ready() {
        let objects = CpdfIndirectObjectHolder::new();
        let globals = retain_object(CpdfStream::from_data_and_dict(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/jbig2_globals.bin"
            ))
            .to_vec(),
            retain_object(CpdfDictionary::new()),
        ));
        objects.borrow_mut().add_indirect_object(globals.clone());
        let mut params = CpdfDictionary::new();
        params.set_new_for(
            b"JBIG2Globals",
            CpdfReference::new(&objects, globals.borrow().obj_num()),
        );
        let mut dict = CpdfDictionary::new();
        for (key, value) in [(b"Width".as_slice(), 1152), (b"Height", 720)] {
            dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        dict.set_new_name_for(b"Filter", b"JBIG2Decode");
        dict.set_new_for(b"DecodeParms", params);
        let image = Rc::new(
            CpdfImage::from_stream(
                retain_object(CpdfStream::from_data_and_dict(
                    include_bytes!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/tests/fixtures/jbig2_page.bin"
                    ))
                    .to_vec(),
                    retain_object(dict),
                )),
                false,
            )
            .unwrap(),
        );
        let mut object = CpdfImageObject::new(0);
        object.set_image(Some(image.clone()));
        let cache = CpdfPageImageCache::new();
        for cache in [None, Some(&cache)] {
            let mut loader = CpdfImageLoader::new();
            assert!(loader.start(&object, cache, None));
            assert!(loader.bitmap().is_none());
            assert!(!loader.continue_load(None));
            let bitmap = loader.bitmap_ref().unwrap();
            assert_eq!((bitmap.width(), bitmap.height()), (1152, 720));
            assert!(image.detach_bitmap().is_none());
            if let Some(cache) = cache {
                assert!(matches!(bitmap.as_ref(), CpdfImageBitmap::Bitmap(_)));
                assert_eq!(cache.time_count(), 1);
                assert!(!loader.start(&object, Some(cache), None));
                assert!(Rc::ptr_eq(&bitmap, &loader.bitmap_ref().unwrap()));
                assert_eq!(cache.time_count(), 2);
            } else {
                assert!(matches!(bitmap.as_ref(), CpdfImageBitmap::Dib(_)));
            }
        }
    }
}

#[path = "cpdf_imageloader_owned.rs"]
mod owned;
