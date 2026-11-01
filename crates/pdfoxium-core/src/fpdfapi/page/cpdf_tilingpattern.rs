// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_tilingpattern.{h,cpp}`.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::cpdf_allstates::CpdfAllStates;
use super::cpdf_docpagedata::CpdfDocPageData;
use super::cpdf_form::CpdfForm;
use super::cpdf_pageobject::PageObject;
use super::cpdf_pattern::CpdfPattern;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_dict_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};

pub struct CpdfTilingPattern {
    base: CpdfPattern,
    colored: bool,
    bbox: CfxFloatRect,
    x_step: f32,
    y_step: f32,
}

impl CpdfTilingPattern {
    pub fn new(object: ObjectRef, parent_matrix: CfxMatrix) -> Option<Self> {
        let base = CpdfPattern::new_tiling(object, parent_matrix)?;
        let colored = base.colored_tiling();
        Some(Self {
            base,
            colored,
            bbox: CfxFloatRect::default(),
            x_step: 0.0,
            y_step: 0.0,
        })
    }
    pub fn pattern(&self) -> &CpdfPattern {
        &self.base
    }
    pub fn colored(&self) -> bool {
        self.colored
    }
    pub fn bbox(&self) -> CfxFloatRect {
        self.bbox
    }
    pub fn x_step(&self) -> f32 {
        self.x_step
    }
    pub fn y_step(&self) -> f32 {
        self.y_step
    }

    pub fn load(
        &mut self,
        page_object: &dyn PageObject,
        page_data: Rc<CpdfDocPageData>,
    ) -> Option<CpdfForm> {
        self.load_with_fill_alpha(page_object, page_data, None)
    }

    /// A stroke pattern paints its cell with the caller's stroke opacity.
    /// Keep the original load path for callers without that adaptation.
    pub(crate) fn load_with_fill_alpha(
        &mut self,
        page_object: &dyn PageObject,
        page_data: Rc<CpdfDocPageData>,
        fill_alpha: Option<f32>,
    ) -> Option<CpdfForm> {
        let dict = get_dict_object(self.base.pattern_object())?;
        let borrowed = dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        self.colored = dict.get_integer_for(b"PaintType", 0) == 1;
        self.x_step = dict.get_float_for(b"XStep").abs();
        self.y_step = dict.get_float_for(b"YStep").abs();
        let bbox = dict.get_rect_for(b"BBox");
        if !self
            .base
            .pattern_object()
            .borrow()
            .as_any()
            .is::<CpdfStream>()
        {
            return None;
        }
        let mut form = CpdfForm::new(self.base.pattern_object().clone(), None, None)?;
        let mut states = CpdfAllStates::new();
        states.mutable_color_state().emplace();
        states.mutable_graph_state().emplace();
        states.mutable_text_state().emplace();
        *states.mutable_general_state() = page_object.base().general_state().clone();
        if let Some(alpha) = fill_alpha {
            states.mutable_general_state().set_fill_alpha(alpha);
        }
        form.parse_content(
            Some(states),
            Some(self.base.parent_matrix()),
            page_data,
            Rc::new(RefCell::new(HashSet::new())),
            0,
        );
        self.bbox = bbox;
        Some(form)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    fn numbers(values: &[f32]) -> CpdfArray {
        let mut array = CpdfArray::new();
        for &value in values {
            array.append_new(CpdfScalarObject::number_from_float(value));
        }
        array
    }

    #[test]
    fn parses_tile_form_with_parent_matrix_and_absolute_steps() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"PaintType", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"XStep", CpdfScalarObject::number_from_float(-6.0));
        dict.set_new_for(b"YStep", CpdfScalarObject::number_from_float(7.0));
        dict.set_new_for(b"BBox", numbers(&[0.0, 0.0, 5.0, 5.0]));
        dict.set_new_for(b"Matrix", numbers(&[2.0, 0.0, 0.0, 2.0, 0.0, 0.0]));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            b"1 1 2 3 re f".to_vec(),
            retain_object(dict),
        ));
        let mut pattern =
            CpdfTilingPattern::new(stream, CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 10.0, 20.0)).unwrap();
        let form = pattern
            .load(&CpdfPathObject::default(), Rc::new(CpdfDocPageData::new()))
            .unwrap();
        assert!(pattern.colored());
        assert_eq!((pattern.x_step(), pattern.y_step()), (6.0, 7.0));
        assert_eq!(pattern.bbox(), CfxFloatRect::new(0.0, 0.0, 5.0, 5.0));
        assert_eq!(form.holder().page_object_count(), 1);
        assert_eq!(
            form.calc_bounding_box(),
            CfxFloatRect::new(12.0, 22.0, 16.0, 28.0)
        );
        let mut caller = CpdfPathObject::default();
        caller
            .base_mut()
            .mutable_general_state()
            .set_fill_alpha(0.25);
        caller
            .base_mut()
            .mutable_general_state()
            .set_stroke_alpha(0.75);
        // Reuse one pattern with different inherited opacity; every parsed
        // cell owns its state and the override must not mutate the caller.
        let data = Rc::new(CpdfDocPageData::new());
        for override_alpha in [Some(0.75), None, Some(0.5), None] {
            let cell = pattern
                .load_with_fill_alpha(&caller, data.clone(), override_alpha)
                .unwrap();
            let first = cell.holder().iter().next().unwrap();
            assert_eq!(
                first.base().general_state().get_fill_alpha(),
                override_alpha.unwrap_or(0.25),
            );
            assert_eq!(caller.base().general_state().get_fill_alpha(), 0.25);
            assert_eq!(caller.base().general_state().get_stroke_alpha(), 0.75);
        }
    }
}
