// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Geometry, image ownership, bitmap realization, and cache purging from
//! `core/fpdfapi/page/cpdf_imageobject.{h,cpp}`.

use std::rc::{Rc, Weak};

use super::cpdf_docpagedata::CpdfDocPageData;
use super::cpdf_image::CpdfImage;
use super::cpdf_pageobject::{CpdfPageObject, PageObject, PageObjectType};
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;

pub struct CpdfImageObject {
    base: CpdfPageObject,
    matrix: CfxMatrix,
    image: Option<Rc<CpdfImage>>,
    page_data: Option<Weak<CpdfDocPageData>>,
}

impl Drop for CpdfImageObject {
    fn drop(&mut self) {
        self.maybe_purge_cache();
    }
}

impl Default for CpdfImageObject {
    fn default() -> Self {
        Self::new(CpdfPageObject::NO_CONTENT_STREAM)
    }
}

impl CpdfImageObject {
    pub fn new(content_stream: i32) -> Self {
        Self {
            base: CpdfPageObject::new(content_stream),
            matrix: CfxMatrix::default(),
            image: None,
            page_data: None,
        }
    }
    pub fn set_page_data(&mut self, page_data: Weak<CpdfDocPageData>) {
        self.page_data = Some(page_data);
    }
    pub fn page_data_ref(&self) -> Option<Rc<CpdfDocPageData>> {
        self.page_data.as_ref()?.upgrade()
    }
    fn maybe_purge_cache(&mut self) {
        let Some(image) = self.image.take() else {
            return;
        };
        if image.is_going_to_be_destroyed() {
            return;
        }
        let obj_num = image.stream().borrow().obj_num();
        drop(image);
        if let Some(page_data) = self.page_data.as_ref().and_then(Weak::upgrade) {
            page_data.maybe_purge_image(obj_num);
        }
    }
    pub fn calc_bounding_box(&mut self) {
        let unit = CfxFloatRect::new(0.0, 0.0, 1.0, 1.0);
        self.base.set_original_rect(unit);
        self.base.set_rect(self.matrix.transform_float_rect(unit));
    }
    pub fn set_image(&mut self, image: Option<Rc<CpdfImage>>) {
        self.maybe_purge_cache();
        self.image = image;
    }
    pub fn image(&self) -> Option<Rc<CpdfImage>> {
        self.image.clone()
    }
    pub fn get_independent_bitmap(&self) -> Option<CfxDibitmap> {
        self.get_independent_bitmap_with_resources(None)
    }
    pub fn get_independent_bitmap_with_resources(
        &self,
        resources: Option<&ObjectRef>,
    ) -> Option<CfxDibitmap> {
        self.image.as_ref()?.decode_without_mask(resources)
    }
    pub fn set_initial_image_matrix(&mut self, matrix: CfxMatrix) {
        self.base.initialize_original_matrix(matrix);
        self.set_image_matrix(matrix);
    }
    pub fn set_image_matrix(&mut self, matrix: CfxMatrix) {
        self.matrix = matrix;
        self.calc_bounding_box();
    }
    pub fn matrix(&self) -> CfxMatrix {
        self.matrix
    }
}

impl PageObject for CpdfImageObject {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn object_type(&self) -> PageObjectType {
        PageObjectType::Image
    }
    fn transform(&mut self, matrix: CfxMatrix) {
        self.matrix.concat(matrix);
        self.calc_bounding_box();
        self.base.set_dirty(true);
    }
    fn base(&self) -> &CpdfPageObject {
        &self.base
    }
    fn base_mut(&mut self) -> &mut CpdfPageObject {
        &mut self.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

    #[test]
    fn unit_square_geometry_tracks_matrix_and_original_matrix() {
        let mut object = CpdfImageObject::default();
        object.set_initial_image_matrix(CfxMatrix::new(4.0, 0.0, 0.0, 2.0, 3.0, 5.0));
        assert_eq!(object.base().rect(), CfxFloatRect::new(3.0, 5.0, 7.0, 7.0));
        assert_eq!(
            object.base().original_rect(),
            CfxFloatRect::new(0.0, 0.0, 1.0, 1.0)
        );
        assert_eq!(object.base().original_matrix(), object.matrix());
        object.transform(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 2.0, 1.0));
        assert_eq!(object.base().rect(), CfxFloatRect::new(5.0, 6.0, 9.0, 8.0));
        assert!(object.base().is_dirty());
    }

    #[test]
    fn realized_image_bitmap_is_independent_of_later_reads() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        let stream = retain_object(CpdfStream::from_data_and_dict(
            vec![255, 0, 0],
            retain_object(dict),
        ));
        let mut object = CpdfImageObject::default();
        object.set_image(Some(Rc::new(
            CpdfImage::from_stream(stream, false).unwrap(),
        )));
        let mut first = object.get_independent_bitmap().unwrap();
        first.buffer_mut()[0] = 255;
        let second = object.get_independent_bitmap().unwrap();
        assert_eq!(&second.buffer()[..3], &[0, 0, 255]);
    }
}
