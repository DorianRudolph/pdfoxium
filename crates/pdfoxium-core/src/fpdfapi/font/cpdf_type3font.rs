// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/font/cpdf_type3font.{h,cpp}`. The page layer supplies a form factory.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use super::cpdf_font::CpdfFontBase;
use super::cpdf_simplefont::CpdfSimpleFont;
use super::cpdf_tounicodemap::CpdfToUnicodeMap;
use super::cpdf_truetypefont::adobe_char_name;
use super::cpdf_type3char::{round_pdfium, CpdfType3Char, Type3Form};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{ObjectRef, PdfObject};
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, FxRect};

const MAX_TYPE3_FORM_LEVEL: i32 = 4;

pub trait Type3FormFactory {
    fn create_form(
        &self,
        resources: Option<ObjectRef>,
        stream: ObjectRef,
    ) -> Option<Box<dyn Type3Form>>;
}

struct LoadingDepth<'a>(&'a Cell<i32>);

impl Drop for LoadingDepth<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

pub struct CpdfType3Font {
    pub base_font_name: Vec<u8>,
    font_dict: Option<Weak<RefCell<dyn PdfObject>>>,
    to_unicode: OnceCell<Option<CpdfToUnicodeMap>>,
    pub simple: CpdfSimpleFont,
    pub font_base: RefCell<CpdfFontBase>,
    pub font_matrix: CfxMatrix,
    form_factory: Rc<dyn Type3FormFactory>,
    char_procs: Option<ObjectRef>,
    page_resources: RefCell<Option<ObjectRef>>,
    font_resources: Option<ObjectRef>,
    cache: RefCell<HashMap<u32, Rc<RefCell<CpdfType3Char>>>>,
    char_loading_depth: Cell<i32>,
    char_width: [i32; 256],
}

impl CpdfType3Font {
    pub fn new(form_factory: Rc<dyn Type3FormFactory>) -> Self {
        Self {
            base_font_name: Vec::new(),
            font_dict: None,
            to_unicode: OnceCell::new(),
            simple: CpdfSimpleFont::default(),
            font_base: RefCell::new(CpdfFontBase::default()),
            font_matrix: CfxMatrix::default(),
            form_factory,
            char_procs: None,
            page_resources: RefCell::new(None),
            font_resources: None,
            cache: RefCell::new(HashMap::new()),
            char_loading_depth: Cell::new(0),
            char_width: [0; 256],
        }
    }

    pub fn load(&mut self, font_dict: &CpdfDictionary, base_font_name: &[u8]) {
        self.base_font_name = base_font_name.to_vec();
        self.font_resources = font_dict.get_dict_for(b"Resources");
        let mut xscale = 1.0;
        let mut yscale = 1.0;
        if let Some(matrix) = font_dict.get_array_for(b"FontMatrix") {
            let borrowed = matrix.borrow();
            if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
                self.font_matrix = array.get_matrix();
                xscale = self.font_matrix.a;
                yscale = self.font_matrix.d;
            }
        }
        if let Some(bbox) = font_dict.get_array_for(b"FontBBox") {
            let borrowed = bbox.borrow();
            if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
                let mut rect = CfxFloatRect::new(
                    array.get_float_at(0) * xscale,
                    array.get_float_at(1) * yscale,
                    array.get_float_at(2) * xscale,
                    array.get_float_at(3) * yscale,
                );
                CpdfType3Char::text_unit_rect_to_glyph_unit_rect(&mut rect);
                self.font_base.get_mut().font_bbox = rect.to_fx_rect();
            }
        }
        let start = font_dict.get_integer_for(b"FirstChar", 0);
        if let Ok(start) = usize::try_from(start) {
            if start < self.char_width.len() {
                if let Some(widths) = font_dict.get_array_for(b"Widths") {
                    let borrowed = widths.borrow();
                    if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
                        let count = array.size().min(256).min(256 - start);
                        for index in 0..count {
                            self.char_width[start + index] =
                                round_pdfium(CpdfType3Char::text_unit_to_glyph_unit(
                                    array.get_float_at(index) * xscale,
                                ));
                        }
                    }
                }
            }
        }
        self.char_procs = font_dict.get_dict_for(b"CharProcs");
        if font_dict.get_direct_object_for(b"Encoding").is_some() {
            self.simple.load_pdf_encoding(
                font_dict,
                base_font_name,
                self.font_base.get_mut().flags,
                false,
                false,
            );
        }
    }

    pub fn set_page_resources(&self, resources: Option<ObjectRef>) {
        *self.page_resources.borrow_mut() = resources;
    }

    pub fn set_font_dict(&mut self, dict: &ObjectRef) {
        self.font_dict = Some(Rc::downgrade(dict));
    }

    pub fn font_dict(&self) -> Option<ObjectRef> {
        self.font_dict.as_ref()?.upgrade()
    }

    fn to_unicode(&self) -> Option<&CpdfToUnicodeMap> {
        self.to_unicode
            .get_or_init(|| {
                let dict = self.font_dict.as_ref()?.upgrade()?;
                let borrowed = dict.borrow();
                super::cpdf_font::load_to_unicode_map(
                    borrowed.as_any().downcast_ref::<CpdfDictionary>()?,
                )
            })
            .as_ref()
    }

    pub fn char_code_from_unicode(&self, unicode: u32) -> u32 {
        self.simple
            .char_code_from_unicode(unicode, self.to_unicode())
    }

    pub fn unicode_from_char_code(&self, charcode: u32) -> Vec<u32> {
        self.simple
            .unicode_from_char_code(charcode, self.to_unicode())
    }

    pub fn will_be_destroyed(&self) {
        for glyph in self.cache.borrow().values() {
            glyph.borrow_mut().will_be_destroyed();
        }
    }

    pub fn load_char(&self, charcode: u32) -> Option<Rc<RefCell<CpdfType3Char>>> {
        if self.char_loading_depth.get() >= MAX_TYPE3_FORM_LEVEL {
            return None;
        }
        if let Some(glyph) = self.cache.borrow().get(&charcode) {
            return Some(Rc::clone(glyph));
        }
        let name = adobe_char_name(
            self.simple.base_encoding(),
            self.simple.char_names(),
            u8::try_from(charcode).ok()?,
        )?;
        let procs = self.char_procs.as_ref()?;
        let stream = {
            let borrowed = procs.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
            dict.get_stream_for(name)?
        };
        let resources = self
            .font_resources
            .clone()
            .or_else(|| self.page_resources.borrow().clone());
        let mut form = self.form_factory.create_form(resources, stream)?;
        let mut glyph = CpdfType3Char::new();
        {
            self.char_loading_depth
                .set(self.char_loading_depth.get() + 1);
            let _guard = LoadingDepth(&self.char_loading_depth);
            form.parse_content_for_type3_char(&mut glyph);
        }
        if let Some(cached) = self.cache.borrow().get(&charcode) {
            return Some(Rc::clone(cached));
        }
        glyph.transform(form.as_ref(), self.font_matrix);
        if form.has_page_objects() {
            glyph.set_form(form);
        }
        let glyph = Rc::new(RefCell::new(glyph));
        self.cache.borrow_mut().insert(charcode, Rc::clone(&glyph));
        Some(glyph)
    }

    pub fn get_char_width(&self, charcode: u32) -> i32 {
        let index = usize::try_from(charcode)
            .ok()
            .filter(|&code| code < 256)
            .unwrap_or(0);
        if self.char_width[index] != 0 {
            return self.char_width[index];
        }
        self.load_char(index as u32)
            .map_or(0, |glyph| glyph.borrow().width())
    }

    pub fn get_char_bbox(&self, charcode: u32) -> FxRect {
        self.load_char(charcode)
            .map_or_else(FxRect::default, |glyph| glyph.borrow().bbox())
    }

    pub fn check_type3_font_metrics(&self) {
        // The no-face branch of CPDF_Font::CheckFontMetrics(). Release each
        // state borrow before LoadChar(), which can parse text using this font.
        if self.font_base.borrow().font_bbox == FxRect::default() {
            let mut first = true;
            for code in 0..256 {
                let rect = self.get_char_bbox(code);
                if rect.left == rect.right {
                    continue;
                }
                let mut base = self.font_base.borrow_mut();
                if first {
                    base.font_bbox = rect;
                    first = false;
                } else {
                    base.font_bbox.left = base.font_bbox.left.min(rect.left);
                    base.font_bbox.top = base.font_bbox.top.max(rect.top);
                    base.font_bbox.right = base.font_bbox.right.max(rect.right);
                    base.font_bbox.bottom = base.font_bbox.bottom.min(rect.bottom);
                }
            }
        }
        let missing_ascent_descent = {
            let base = self.font_base.borrow();
            base.ascent == 0 && base.descent == 0
        };
        if missing_ascent_descent {
            let upper = self.get_char_bbox(u32::from(b'A'));
            {
                let mut base = self.font_base.borrow_mut();
                base.ascent = if upper.bottom == upper.top {
                    base.font_bbox.top
                } else {
                    upper.top
                };
            }
            let lower = self.get_char_bbox(u32::from(b'g'));
            let mut base = self.font_base.borrow_mut();
            base.descent = if lower.bottom == lower.top {
                base.font_bbox.bottom
            } else {
                lower.bottom
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::make_reference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    struct Form {
        parsed: Rc<Cell<u32>>,
    }

    impl Type3Form for Form {
        fn parse_content_for_type3_char(&mut self, glyph: &mut CpdfType3Char) {
            self.parsed.set(self.parsed.get() + 1);
            glyph.initialize_from_stream_data(false, [0.5, 0.0, 0.0, 0.0, 1.0, 1.0]);
        }
        fn has_page_objects(&self) -> bool {
            true
        }
        fn calc_bounding_box(&self) -> CfxFloatRect {
            CfxFloatRect::new(0.0, 0.0, 1.0, 1.0)
        }
        fn get_bitmap_and_matrix_from_sole_image_of_form(
            &self,
        ) -> Option<(super::super::cpdf_type3char::BitmapRef, CfxMatrix)> {
            None
        }
    }

    struct Factory(Rc<Cell<u32>>);

    impl Type3FormFactory for Factory {
        fn create_form(
            &self,
            _resources: Option<ObjectRef>,
            _stream: ObjectRef,
        ) -> Option<Box<dyn Type3Form>> {
            Some(Box::new(Form {
                parsed: Rc::clone(&self.0),
            }))
        }
    }

    #[test]
    fn width_table_and_char_cache_follow_type3_load() {
        let parsed = Rc::new(Cell::new(0));
        let mut font = CpdfType3Font::new(Rc::new(Factory(Rc::clone(&parsed))));
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
        dict.set_rect_for(b"FontBBox", CfxFloatRect::new(0.0, 0.0, 1.0, 1.0));
        let widths = dict.set_new_array_for(b"Widths");
        widths
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append_new(CpdfScalarObject::number_from_float(0.4));
        let procs = dict.set_new_dictionary_for(b"CharProcs");
        let holder = CpdfIndirectObjectHolder::new();
        let stream = holder
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"500 0 d0");
        procs
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"A", Some(make_reference(&stream, &holder)));
        let encoding = dict.set_new_dictionary_for(b"Encoding");
        let diffs = encoding
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_array_for(b"Differences");
        let mut borrowed = diffs.borrow_mut();
        let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        array.append_new(CpdfScalarObject::number_from_signed(65));
        array.append_new_name(b"A");
        drop(borrowed);

        font.load(&dict, b"T3");
        assert_eq!(
            font.font_base.borrow().font_bbox,
            FxRect::new(0, 1000, 1000, 0)
        );
        assert_eq!(font.get_char_width(65), 400);
        assert_eq!(parsed.get(), 0);
        let first = font.load_char(65).unwrap();
        let second = font.load_char(65).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        assert_eq!(parsed.get(), 1);
        assert_eq!(font.get_char_bbox(65), FxRect::new(0, 1000, 1000, 0));
        font.char_loading_depth.set(MAX_TYPE3_FORM_LEVEL);
        assert!(font.load_char(65).is_none());
        font.char_loading_depth.set(0);
        font.will_be_destroyed();
        assert!(first.borrow().form().is_none());
    }
}
