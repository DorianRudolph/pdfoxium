// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Page attributes and geometry from `core/fpdfapi/page/cpdf_page.{h,cpp}`
//! and the non-XFA portion of `core/fpdfapi/page/ipdf_page.h`.
//! Synchronous render contexts borrow the page for the duration of the call;
//! retained public progressive rendering and interactive page views are deferred.

use std::collections::HashSet;
use std::rc::Rc;

use super::cpdf_contentparser::{ContentStage, CpdfContentParser};
use super::cpdf_docpagedata::CpdfDocPageData;
use super::cpdf_pageimagecache::CpdfPageImageCache;
use super::cpdf_pageobjectholder::{CpdfPageObjectHolder, ParseState};
use super::cpdf_streamcontentparser::CpdfStreamContentParser;
use super::cpdf_transparency::CpdfTransparency;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::{get_dict_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF, CfxSizeF, FxRect};

pub struct CpdfPage {
    dict: ObjectRef,
    holder: CpdfPageObjectHolder,
    page_data: Rc<CpdfDocPageData>,
    image_cache: CpdfPageImageCache,
    page_size: CfxSizeF,
    page_matrix: CfxMatrix,
    bbox: CfxFloatRect,
}

impl CpdfPage {
    pub fn from_document(document: &mut CpdfDocument, index: usize) -> Option<Self> {
        let dict = document.page_dictionary(index)?;
        Self::new_with_page_data(dict, document.page_data_ref())
    }

    pub fn new(dict: ObjectRef) -> Option<Self> {
        Self::new_with_page_data(dict, Rc::new(CpdfDocPageData::new()))
    }

    pub(crate) fn new_with_page_data(
        dict: ObjectRef,
        page_data: Rc<CpdfDocPageData>,
    ) -> Option<Self> {
        if !Self::is_valid_page_dict_loose(&dict) {
            return None;
        }
        let missing_type = {
            let borrowed = dict.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .key_exist(b"Type")
                == false
        };
        if missing_type {
            let mut borrowed = dict.borrow_mut();
            borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()?
                .set_new_name_for(b"Type", b"Page");
        }
        let mut page = Self {
            holder: CpdfPageObjectHolder::new(dict.clone(), None, None),
            dict,
            image_cache: CpdfPageImageCache::with_page_data(&page_data),
            page_data,
            page_size: CfxSizeF::new(100.0, 100.0),
            page_matrix: CfxMatrix::default(),
            bbox: CfxFloatRect::default(),
        };
        page.holder.set_is_page(true);
        page.holder.bind_page_data(&page.page_data);
        let resources = page
            .page_attr(b"Resources")
            .and_then(|object| get_dict_object(&object));
        page.holder.set_resources(resources.clone());
        page.holder.set_page_resources(resources);
        page.update_dimensions();
        page.holder.mutable_transparency().set_isolated();
        page.holder.load_transparency_info();
        Some(page)
    }

    pub fn is_valid_page_dict(dict: &ObjectRef) -> bool {
        Self::is_valid_page_dict_loose(dict)
            && dict
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .is_some_and(|dict| dict.key_exist(b"Type"))
    }

    pub fn is_valid_page_dict_loose(dict: &ObjectRef) -> bool {
        let borrowed = dict.borrow();
        let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
            return false;
        };
        let Some(kind) = dict.get_direct_object_for(b"Type") else {
            return !dict.key_exist(b"Type");
        };
        let borrowed = kind.borrow();
        borrowed.object_type() == ObjectType::Name
            && crate::fxcrt::bytestring::eq_cstr(&borrowed.get_string(), b"Page")
    }

    pub fn dict(&self) -> ObjectRef {
        self.dict.clone()
    }
    pub fn holder(&self) -> &CpdfPageObjectHolder {
        &self.holder
    }
    pub fn mutable_holder(&mut self) -> &mut CpdfPageObjectHolder {
        &mut self.holder
    }
    pub fn page_data(&self) -> &CpdfDocPageData {
        &self.page_data
    }
    pub fn image_cache(&self) -> &CpdfPageImageCache {
        &self.image_cache
    }
    pub(crate) fn page_data_handle(&self) -> &Rc<CpdfDocPageData> {
        &self.page_data
    }
    pub fn page_data_ref(&self) -> Rc<CpdfDocPageData> {
        Rc::clone(&self.page_data)
    }
    /// Drain static page content parsing, as CPDF_Page::ParseContent does.
    pub fn parse_path_content(&mut self) {
        if self.holder.parse_state() != ParseState::NotParsed {
            return;
        }
        let mut content = {
            let borrowed = self.dict.borrow();
            let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
                return;
            };
            CpdfContentParser::from_page_dict(dict)
        };
        if content.stage() == ContentStage::Complete {
            self.holder.start_parse();
            self.holder.finish_parse(Default::default());
            return;
        }
        let mut parser = CpdfStreamContentParser::with_page_data(
            None,
            CfxMatrix::default(),
            Rc::clone(&self.page_data),
        );
        // ContentParser::Parse initializes page colors. Form parsers retain
        // missing colors so RenderStatus can resolve their initial state.
        parser.cur_states_mut().mutable_color_state().set_default();
        content.parse_all(&mut self.holder, &mut parser);
    }
    pub fn width(&self) -> f32 {
        self.page_size.width
    }
    pub fn height(&self) -> f32 {
        self.page_size.height
    }
    pub fn page_size(&self) -> CfxSizeF {
        self.page_size
    }
    pub fn page_matrix(&self) -> CfxMatrix {
        self.page_matrix
    }
    pub fn bbox(&self) -> CfxFloatRect {
        self.bbox
    }
    pub fn transparency(&self) -> CpdfTransparency {
        self.holder.transparency()
    }

    pub fn page_attr(&self, name: &[u8]) -> Option<ObjectRef> {
        let mut visited = HashSet::new();
        let mut current = Some(self.dict.clone());
        while let Some(dict) = current {
            let identity = Rc::as_ptr(&dict) as *const () as usize;
            if !visited.insert(identity) {
                break;
            }
            let borrowed = dict.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
            if let Some(attr) = dict.get_direct_object_for(name) {
                return Some(attr);
            }
            current = dict.get_dict_for(b"Parent");
        }
        None
    }

    pub fn box_for(&self, name: &[u8]) -> CfxFloatRect {
        let Some(array) = self.page_attr(name) else {
            return CfxFloatRect::default();
        };
        let borrowed = array.borrow();
        let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
            return CfxFloatRect::default();
        };
        let mut rect = array.get_rect();
        rect.normalize();
        rect
    }

    pub fn rotation(&self) -> i32 {
        let rotation = self
            .page_attr(b"Rotate")
            .map_or(0, |value| value.borrow().get_integer() / 90 % 4);
        if rotation < 0 {
            rotation + 4
        } else {
            rotation
        }
    }

    pub fn annots_array(&self) -> Option<ObjectRef> {
        let borrowed = self.dict.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_array_for(b"Annots")
    }

    pub fn get_or_create_annots_array(&mut self) -> ObjectRef {
        self.dict
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .expect("page dictionary")
            .get_or_create_array_for(b"Annots")
    }

    pub fn get_mutable_annots_array(&mut self) -> Option<ObjectRef> {
        self.annots_array()
    }

    pub fn display_matrix_for_rect(&self, rect: FxRect, rotation: i32) -> CfxMatrix {
        self.display_matrix_for_float_rect(CfxFloatRect::from_fx_rect(rect), rotation)
    }

    pub fn display_matrix_for_float_rect(&self, rect: CfxFloatRect, rotation: i32) -> CfxMatrix {
        if self.page_size.width == 0.0 || self.page_size.height == 0.0 {
            return CfxMatrix::default();
        }
        let (x0, y0, x1, y1, x2, y2) = match rotation % 4 {
            0 => (
                rect.left,
                rect.top,
                rect.left,
                rect.bottom,
                rect.right,
                rect.top,
            ),
            1 => (
                rect.left,
                rect.bottom,
                rect.right,
                rect.bottom,
                rect.left,
                rect.top,
            ),
            2 => (
                rect.right,
                rect.bottom,
                rect.right,
                rect.top,
                rect.left,
                rect.bottom,
            ),
            3 => (
                rect.right,
                rect.top,
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
            ),
            _ => return CfxMatrix::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
        };
        let matrix = CfxMatrix::new(
            (x2 - x0) / self.page_size.width,
            (y2 - y0) / self.page_size.width,
            (x1 - x0) / self.page_size.height,
            (y1 - y0) / self.page_size.height,
            x0,
            y0,
        );
        self.page_matrix * matrix
    }

    pub fn display_matrix(&self) -> CfxMatrix {
        self.display_matrix_for_float_rect(
            CfxFloatRect::new(0.0, 0.0, self.width(), self.height()),
            0,
        )
    }

    pub fn device_to_page(&self, rect: FxRect, rotation: i32, point: CfxPointF) -> CfxPointF {
        self.display_matrix_for_rect(rect, rotation)
            .get_inverse()
            .transform(point)
    }

    pub fn page_to_device(&self, rect: FxRect, rotation: i32, point: CfxPointF) -> CfxPointF {
        self.display_matrix_for_rect(rect, rotation)
            .transform(point)
    }

    pub fn update_dimensions(&mut self) {
        let mut media_box = self.box_for(b"MediaBox");
        if media_box.is_empty() {
            media_box = CfxFloatRect::new(0.0, 0.0, 612.0, 792.0);
        }
        self.bbox = self.box_for(b"CropBox");
        if self.bbox.is_empty() {
            self.bbox = media_box;
        } else {
            self.bbox.intersect(media_box);
        }
        self.page_size = CfxSizeF::new(self.bbox.width(), self.bbox.height());
        match self.rotation() {
            0 => {
                self.page_matrix =
                    CfxMatrix::new(1.0, 0.0, 0.0, 1.0, -self.bbox.left, -self.bbox.bottom)
            }
            1 => {
                std::mem::swap(&mut self.page_size.width, &mut self.page_size.height);
                self.page_matrix =
                    CfxMatrix::new(0.0, -1.0, 1.0, 0.0, -self.bbox.bottom, self.bbox.right);
            }
            2 => {
                self.page_matrix =
                    CfxMatrix::new(-1.0, 0.0, 0.0, -1.0, self.bbox.right, self.bbox.top)
            }
            3 => {
                std::mem::swap(&mut self.page_size.width, &mut self.page_size.height);
                self.page_matrix =
                    CfxMatrix::new(0.0, 1.0, -1.0, 0.0, self.bbox.top, -self.bbox.left);
            }
            _ => unreachable!(),
        }
        self.holder.set_bbox(self.bbox);
    }

    pub fn resources(&self) -> Option<ObjectRef> {
        get_dict_object(&self.page_attr(b"Resources")?)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::CpdfPage;
    use crate::fpdfapi::page::cpdf_pageobjectholder::ParseState;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
    use crate::fpdfapi::parser::cpdf_parser::ParserError;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
    use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, FxRect};

    #[test]
    fn page_type_comparison_uses_native_c_string_boundary() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Type", b"Page\0tail");
        let dict = retain_object(dict);
        assert!(CpdfPage::is_valid_page_dict(&dict));
        assert!(CpdfPage::new(Rc::clone(&dict)).is_some());
        assert_eq!(
            dict.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_name_for(b"Type"),
            b"Page\0tail"
        );
    }

    #[test]
    fn image_cache_belongs_to_page_even_when_document_data_is_shared() {
        use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
        use crate::fpdfapi::page::cpdf_image::CpdfImage;
        use crate::fpdfapi::page::cpdf_imageobject::CpdfImageObject;
        use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
        let data = Rc::new(CpdfDocPageData::new());
        let page = || {
            CpdfPage::new_with_page_data(retain_object(CpdfDictionary::new()), Rc::clone(&data))
                .unwrap()
        };
        let first = page();
        let second = page();
        let mut dict = CpdfDictionary::new();
        for (key, value) in [
            (b"Width".as_slice(), 1),
            (b"Height", 1),
            (b"BitsPerComponent", 8),
        ] {
            dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        let image = Rc::new(
            CpdfImage::from_stream(
                retain_object(CpdfStream::from_data_and_dict(
                    vec![128],
                    retain_object(dict),
                )),
                false,
            )
            .unwrap(),
        );
        let mut object = CpdfImageObject::new(0);
        object.set_image(Some(image));
        let first_pixels = first.image_cache().get_or_decode(&object, None).unwrap();
        let second_pixels = second.image_cache().get_or_decode(&object, None).unwrap();
        assert!(!Rc::ptr_eq(&first_pixels, &second_pixels));
        let first_weak = Rc::downgrade(&first_pixels);
        drop(first_pixels);
        drop(first);
        assert!(first_weak.upgrade().is_none());
        assert!(Rc::ptr_eq(
            &second_pixels,
            &second.image_cache().get_or_decode(&object, None).unwrap()
        ));
    }

    #[test]
    fn annotation_array_creation_retains_mutable_page_entry() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Type", b"Page");
        let mut page = CpdfPage::new(retain_object(dict)).unwrap();
        assert!(page.get_mutable_annots_array().is_none());
        let annots = page.get_or_create_annots_array();
        annots
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append_new(CpdfDictionary::new());
        assert!(Rc::ptr_eq(
            &annots,
            &page.get_mutable_annots_array().unwrap()
        ));
        assert_eq!(
            page.annots_array()
                .unwrap()
                .borrow()
                .as_any()
                .downcast_ref::<CpdfArray>()
                .unwrap()
                .size(),
            1
        );
    }

    #[test]
    fn inherited_media_box_and_device_conversion() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/hello_world_2_pages.pdf"
        ));
        let mut document = CpdfDocument::new();
        assert_eq!(
            document.load_doc(
                Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec())),
                b""
            ),
            ParserError::Success
        );
        let page = CpdfPage::from_document(&mut document, 0).unwrap();
        assert_eq!((page.width(), page.height()), (200.0, 200.0));
        assert_eq!(page.rotation(), 0);
        let rect = FxRect::new(0, 0, 200, 200);
        assert_eq!(
            page.page_to_device(rect, 0, CfxPointF::new(0.0, 0.0)),
            CfxPointF::new(0.0, 200.0)
        );
        let point = CfxPointF::new(42.0, 71.0);
        assert_eq!(
            page.device_to_page(rect, 0, page.page_to_device(rect, 0, point)),
            point
        );
    }

    #[test]
    fn parses_path_content_from_page_stream_reference() {
        let objects = CpdfIndirectObjectHolder::new();
        let stream = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"1 2 3 4 re f");
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Type", b"Page");
        dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, stream.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(dict)).unwrap();
        page.parse_path_content();
        assert_eq!(page.holder().parse_state(), ParseState::Parsed);
        assert_eq!(page.holder().page_object_count(), 1);
        assert_eq!(page.page_data().parsed_page_count(), 1);
        page.parse_path_content();
        assert_eq!(page.holder().page_object_count(), 1);
        assert_eq!(page.page_data().parsed_page_count(), 1);
    }

    #[test]
    fn loads_transparency_group_from_pdfium_fixture() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/tagged_alt_text.pdf"
        ));
        let mut document = CpdfDocument::new();
        assert_eq!(
            document.load_doc(
                Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec())),
                b""
            ),
            ParserError::Success
        );
        let page = CpdfPage::from_document(&mut document, 0).unwrap();
        assert!(page.transparency().is_group());
        assert!(page.transparency().is_isolated());
        assert_eq!((page.width(), page.height()), (612.0, 792.0));
    }

    #[test]
    fn rotation_and_crop_intersection_follow_inherited_attributes() {
        let parent: ObjectRef = retain_object(CpdfDictionary::new());
        let page: ObjectRef = retain_object(CpdfDictionary::new());
        let mut media = CpdfArray::new();
        for n in [0, 0, 300, 400] {
            media.append_new(CpdfScalarObject::number_from_signed(n));
        }
        let mut crop = CpdfArray::new();
        for n in [100, 50, 400, 250] {
            crop.append_new(CpdfScalarObject::number_from_signed(n));
        }
        {
            let mut borrowed = parent.borrow_mut();
            let dict = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            dict.set_new_for(b"MediaBox", media);
            dict.set_new_for(b"Rotate", CpdfScalarObject::number_from_signed(90));
        }
        {
            let mut borrowed = page.borrow_mut();
            let dict = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            dict.set_new_name_for(b"Type", b"Page");
            dict.set_new_for(b"CropBox", crop);
            dict.set_for(b"Parent", Some(parent));
        }
        let page = CpdfPage::new(page).unwrap();
        assert_eq!((page.width(), page.height()), (200.0, 200.0));
        assert_eq!(page.rotation(), 1);
        assert_eq!(page.bbox().right, 300.0);
        assert_eq!(
            page.page_matrix(),
            CfxMatrix::new(0.0, -1.0, 1.0, 0.0, -50.0, 300.0)
        );
        assert_eq!(
            page.display_matrix_for_rect(FxRect::new(0, 0, 200, 200), -1),
            CfxMatrix::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
        );
    }
}

#[cfg(test)]
#[path = "cpdf_page_unittest.rs"]
mod upstream_tests;
