// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_formobject.{h,cpp}`.

use super::cpdf_form::CpdfForm;
use super::cpdf_pageobject::{CpdfPageObject, PageObject, PageObjectType};
use crate::fxcrt::fx_coordinates::CfxMatrix;

pub struct CpdfFormObject {
    base: CpdfPageObject,
    form: Box<CpdfForm>,
    form_matrix: CfxMatrix,
}

impl CpdfFormObject {
    pub fn new(content_stream: i32, form: CpdfForm, matrix: CfxMatrix) -> Self {
        Self {
            base: CpdfPageObject::new(content_stream),
            form: Box::new(form),
            form_matrix: matrix,
        }
    }
    pub fn form(&self) -> &CpdfForm {
        &self.form
    }
    pub fn form_mut(&mut self) -> &mut CpdfForm {
        &mut self.form
    }
    pub fn form_matrix(&self) -> CfxMatrix {
        self.form_matrix
    }
    pub fn calc_bounding_box(&mut self) {
        self.base.set_rect(
            self.form_matrix
                .transform_float_rect(self.form.calc_bounding_box()),
        );
    }
    pub fn set_form_matrix(&mut self, matrix: CfxMatrix) {
        self.form_matrix = matrix;
        self.calc_bounding_box();
    }
}

impl PageObject for CpdfFormObject {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn object_type(&self) -> PageObjectType {
        PageObjectType::Form
    }
    fn transform(&mut self, matrix: CfxMatrix) {
        self.form_matrix.concat(matrix);
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
