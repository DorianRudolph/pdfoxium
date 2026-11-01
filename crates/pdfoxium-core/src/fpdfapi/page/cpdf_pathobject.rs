// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_pathobject.{h,cpp}`.

use super::cpdf_pageobject::{CpdfPageObject, PageObject, PageObjectType};
use super::cpdf_path::CpdfPath;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use crate::fxge::cfx_fillrenderoptions::FillType;

pub struct CpdfPathObject {
    base: CpdfPageObject,
    stroke: bool,
    fill_type: FillType,
    path: CpdfPath,
    matrix: CfxMatrix,
}

impl Default for CpdfPathObject {
    fn default() -> Self {
        Self::new(CpdfPageObject::NO_CONTENT_STREAM)
    }
}

impl CpdfPathObject {
    pub fn new(content_stream: i32) -> Self {
        Self {
            base: CpdfPageObject::new(content_stream),
            stroke: false,
            fill_type: FillType::NoFill,
            path: CpdfPath::default(),
            matrix: CfxMatrix::default(),
        }
    }
    pub fn stroke(&self) -> bool {
        self.stroke
    }
    pub fn set_stroke(&mut self, value: bool) {
        self.stroke = value;
    }
    pub fn has_no_filltype(&self) -> bool {
        self.fill_type == FillType::NoFill
    }
    pub fn has_winding_filltype(&self) -> bool {
        self.fill_type == FillType::Winding
    }
    pub fn has_alternate_filltype(&self) -> bool {
        self.fill_type == FillType::EvenOdd
    }
    pub fn set_no_filltype(&mut self) {
        self.fill_type = FillType::NoFill;
    }
    pub fn set_winding_filltype(&mut self) {
        self.fill_type = FillType::Winding;
    }
    pub fn set_alternate_filltype(&mut self) {
        self.fill_type = FillType::EvenOdd;
    }
    pub fn filltype(&self) -> FillType {
        self.fill_type
    }
    pub fn set_filltype(&mut self, value: FillType) {
        self.fill_type = value;
    }
    pub fn path(&self) -> &CpdfPath {
        &self.path
    }
    pub fn mutable_path(&mut self) -> &mut CpdfPath {
        &mut self.path
    }
    pub fn matrix(&self) -> CfxMatrix {
        self.matrix
    }
    pub fn set_path_matrix(&mut self, matrix: CfxMatrix) {
        self.matrix = matrix;
        self.calc_bounding_box();
    }
    pub fn calc_bounding_box(&mut self) {
        #[cfg(feature = "render-profile")]
        let _profile = crate::render_profile::Scope::new("path bounding boxes");
        if !self.path.has_ref() {
            return;
        }
        let width = self.base.graph_state().get_line_width();
        let mut rect: CfxFloatRect = if self.stroke && width != 0.0 {
            self.path
                .bounding_box_for_stroke_path(width, self.base.graph_state().get_miter_limit())
        } else {
            self.path.bounding_box()
        };
        rect = self.matrix.transform_float_rect(rect);
        if width == 0.0 && self.stroke {
            rect.inflate_xy(0.5, 0.5);
        }
        self.base.set_rect(rect);
    }
}

impl PageObject for CpdfPathObject {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn object_type(&self) -> PageObjectType {
        PageObjectType::Path
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

    #[test]
    fn path_transform_updates_bounds_and_dirty_state() {
        let mut object = CpdfPathObject::default();
        object.mutable_path().append_rect(1.0, 2.0, 3.0, 4.0);
        object.calc_bounding_box();
        assert_eq!(object.base().rect(), CfxFloatRect::new(1.0, 2.0, 3.0, 4.0));
        assert!(!object.base().is_dirty());
        object.transform(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 5.0, 6.0));
        assert_eq!(object.base().rect(), CfxFloatRect::new(6.0, 8.0, 8.0, 10.0));
        assert!(object.base().is_dirty());
    }

    #[test]
    fn zero_width_stroke_inflates_after_transform() {
        let mut object = CpdfPathObject::default();
        object.mutable_path().append_rect(0.0, 0.0, 2.0, 2.0);
        object.base_mut().mutable_graph_state().set_line_width(0.0);
        object.set_stroke(true);
        object.set_path_matrix(CfxMatrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0));
        assert_eq!(
            object.base().rect(),
            CfxFloatRect::new(-0.5, -0.5, 4.5, 4.5)
        );
    }
}
