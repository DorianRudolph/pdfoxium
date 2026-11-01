// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_shadingobject.{h,cpp}`.

use std::rc::Rc;

use super::cpdf_pageobject::{CpdfPageObject, PageObject, PageObjectType};
use super::cpdf_shadingpattern::CpdfShadingPattern;
use crate::fxcrt::fx_coordinates::CfxMatrix;

pub struct CpdfShadingObject {
    base: CpdfPageObject,
    shading: Rc<CpdfShadingPattern>,
    matrix: CfxMatrix,
}

impl CpdfShadingObject {
    pub fn new(content_stream: i32, shading: Rc<CpdfShadingPattern>, matrix: CfxMatrix) -> Self {
        Self {
            base: CpdfPageObject::new(content_stream),
            shading,
            matrix,
        }
    }
    pub fn pattern(&self) -> &CpdfShadingPattern {
        &self.shading
    }
    pub fn matrix(&self) -> CfxMatrix {
        self.matrix
    }
    pub fn calc_bounding_box(&mut self) {
        if self.base.clip_path().has_ref() {
            self.base.set_rect(self.base.clip_path().clip_box());
        }
    }
}

impl PageObject for CpdfShadingObject {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn object_type(&self) -> PageObjectType {
        PageObjectType::Shading
    }
    fn transform(&mut self, matrix: CfxMatrix) {
        let has_clip = self.base.clip_path().has_ref();
        if has_clip {
            self.base.mutable_clip_path().transform(matrix);
        }
        self.matrix.concat(matrix);
        if has_clip {
            self.calc_bounding_box();
        } else {
            self.base
                .set_rect(matrix.transform_float_rect(self.base.rect()));
        }
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
    use crate::fpdfapi::page::cpdf_path::CpdfPath;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fxcrt::fx_coordinates::CfxFloatRect;
    use crate::fxge::cfx_fillrenderoptions::FillType;

    #[test]
    fn transform_updates_clipped_bounds() {
        let shading = Rc::new(
            CpdfShadingPattern::new(
                retain_object(CpdfDictionary::new()),
                true,
                CfxMatrix::default(),
            )
            .unwrap(),
        );
        let mut object = CpdfShadingObject::new(0, shading, CfxMatrix::default());
        let mut path = CpdfPath::new();
        path.append_rect(1.0, 2.0, 3.0, 4.0);
        object
            .base_mut()
            .mutable_clip_path()
            .append_path(path, FillType::Winding);
        object.calc_bounding_box();
        object.transform(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 5.0, 6.0));
        assert_eq!(object.base().rect(), CfxFloatRect::new(6.0, 8.0, 8.0, 10.0));
        assert!(object.base().is_dirty());
    }
}
