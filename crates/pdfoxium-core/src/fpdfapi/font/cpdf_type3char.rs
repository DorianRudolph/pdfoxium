// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/font/cpdf_type3char.{h,cpp}`. The form interface is supplied
//! by the font and page rendering layers.

use std::cell::RefCell;
use std::rc::Rc;

use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, FxRect};
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;

pub type BitmapRef = Rc<RefCell<CfxDibitmap>>;

pub trait Type3Form: std::any::Any {
    fn parse_content_for_type3_char(&mut self, glyph: &mut CpdfType3Char);
    fn has_page_objects(&self) -> bool;
    fn calc_bounding_box(&self) -> CfxFloatRect;
    fn get_bitmap_and_matrix_from_sole_image_of_form(&self) -> Option<(BitmapRef, CfxMatrix)>;
}

pub struct CpdfType3Char {
    form: Option<Rc<dyn Type3Form>>,
    bitmap: Option<BitmapRef>,
    colored: bool,
    width: i32,
    image_matrix: CfxMatrix,
    bbox: FxRect,
}

impl Default for CpdfType3Char {
    fn default() -> Self {
        Self {
            form: None,
            bitmap: None,
            colored: false,
            width: 0,
            image_matrix: CfxMatrix::default(),
            bbox: FxRect::default(),
        }
    }
}

impl CpdfType3Char {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text_unit_to_glyph_unit(value: f32) -> f32 {
        value * 1000.0
    }

    pub fn text_unit_rect_to_glyph_unit_rect(rect: &mut CfxFloatRect) {
        rect.scale(1000.0);
    }

    pub fn load_bitmap_from_sole_image_of_form(&mut self) -> bool {
        if self.bitmap.is_some() || self.form.is_none() {
            return true;
        }
        if self.colored {
            return false;
        }
        let Some((bitmap, matrix)) = self
            .form
            .as_ref()
            .and_then(|form| form.get_bitmap_and_matrix_from_sole_image_of_form())
        else {
            return false;
        };
        self.bitmap = Some(bitmap);
        self.image_matrix = matrix;
        self.form = None;
        true
    }

    pub fn initialize_from_stream_data(&mut self, colored: bool, data: [f32; 6]) {
        self.colored = colored;
        self.width = round_pdfium(Self::text_unit_to_glyph_unit(data[0]));
        self.bbox.left = round_pdfium(Self::text_unit_to_glyph_unit(data[2]));
        self.bbox.bottom = round_pdfium(Self::text_unit_to_glyph_unit(data[3]));
        self.bbox.right = round_pdfium(Self::text_unit_to_glyph_unit(data[4]));
        self.bbox.top = round_pdfium(Self::text_unit_to_glyph_unit(data[5]));
    }

    pub fn transform(&mut self, form: &dyn Type3Form, matrix: CfxMatrix) {
        self.width = (self.width as f32 * matrix.get_x_unit() + 0.5) as i32;
        let mut rect = if self.bbox.right <= self.bbox.left || self.bbox.bottom >= self.bbox.top {
            let mut rect = form.calc_bounding_box();
            Self::text_unit_rect_to_glyph_unit_rect(&mut rect);
            rect
        } else {
            CfxFloatRect::from_fx_rect(self.bbox)
        };
        rect = matrix.transform_float_rect(rect);
        self.bbox = rect.to_rounded_fx_rect();
    }

    pub fn will_be_destroyed(&mut self) {
        self.form = None;
    }
    pub fn set_form(&mut self, form: Box<dyn Type3Form>) {
        self.form = Some(Rc::from(form));
    }
    pub fn form(&self) -> Option<&dyn Type3Form> {
        self.form.as_deref()
    }
    pub fn retained_form(&self) -> Option<Rc<dyn Type3Form>> {
        self.form.as_ref().map(Rc::clone)
    }
    pub fn get_bitmap(&self) -> Option<BitmapRef> {
        self.bitmap.clone()
    }
    pub fn colored(&self) -> bool {
        self.colored
    }
    pub fn width(&self) -> i32 {
        self.width
    }
    pub fn matrix(&self) -> CfxMatrix {
        self.image_matrix
    }
    pub fn bbox(&self) -> FxRect {
        self.bbox
    }
}

pub(super) fn round_pdfium(value: f32) -> i32 {
    if value.is_nan() {
        0
    } else {
        value.round() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    struct ImageForm(BitmapRef);

    impl Type3Form for ImageForm {
        fn parse_content_for_type3_char(&mut self, _glyph: &mut CpdfType3Char) {}
        fn has_page_objects(&self) -> bool {
            true
        }
        fn calc_bounding_box(&self) -> CfxFloatRect {
            CfxFloatRect::new(0.0, 0.0, 1.0, 1.0)
        }
        fn get_bitmap_and_matrix_from_sole_image_of_form(&self) -> Option<(BitmapRef, CfxMatrix)> {
            Some((self.0.clone(), CfxMatrix::default()))
        }
    }

    #[test]
    fn type3_char_uses_form_image_and_releases_form() {
        let bitmap = Rc::new(RefCell::new(
            CfxDibitmap::create(1, 1, FxdibFormat::EightBppMask, 0).unwrap(),
        ));
        let mut glyph = CpdfType3Char::new();
        glyph.initialize_from_stream_data(false, [0.5, 0.0, 0.0, 0.0, 1.0, 1.0]);
        assert_eq!(glyph.width(), 500);
        assert_eq!(glyph.bbox(), FxRect::new(0, 1000, 1000, 0));
        glyph.set_form(Box::new(ImageForm(bitmap.clone())));
        assert!(glyph.load_bitmap_from_sole_image_of_form());
        assert!(glyph.form().is_none());
        assert!(Rc::ptr_eq(&glyph.get_bitmap().unwrap(), &bitmap));
        assert!(glyph.load_bitmap_from_sole_image_of_form());
    }

    #[test]
    fn colored_type3_char_keeps_form_and_uses_form_bounds() {
        let bitmap = Rc::new(RefCell::new(
            CfxDibitmap::create(1, 1, FxdibFormat::EightBppMask, 0).unwrap(),
        ));
        let mut glyph = CpdfType3Char::new();
        glyph.initialize_from_stream_data(true, [0.5, 0.0, 0.0, 0.0, 0.0, 0.0]);
        glyph.set_form(Box::new(ImageForm(bitmap.clone())));
        assert!(!glyph.load_bitmap_from_sole_image_of_form());
        let matrix = CfxMatrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0);
        glyph.transform(&ImageForm(bitmap), matrix);
        assert_eq!(glyph.width(), 1000);
        assert_eq!(glyph.bbox(), FxRect::new(0, 2000, 2000, 0));
        glyph.will_be_destroyed();
        assert!(glyph.form().is_none());
    }
}
