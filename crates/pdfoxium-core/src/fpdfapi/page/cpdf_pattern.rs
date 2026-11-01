// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Pattern identity and matrices from `core/fpdfapi/page/cpdf_pattern.{h,cpp}`.
//! Tiling form and shading content loading follow with the page object path.

use super::cpdf_shadingpattern::{CpdfShadingPattern, ShadingData};
use std::cell::{OnceCell, RefCell};
use std::rc::{Rc, Weak};

use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_dict_object, ObjectRef};
use crate::fxcrt::fx_coordinates::CfxMatrix;

pub type PatternRef = Rc<CpdfPattern>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternKind {
    Tiling { colored: bool },
    Shading { shading_object: bool },
}

pub struct CpdfPattern {
    pub(crate) shading_data: OnceCell<ShadingData>,
    pub(crate) shading_wrapper: RefCell<Weak<CpdfShadingPattern>>,
    kind: PatternKind,
    pattern_object: ObjectRef,
    parent_matrix: CfxMatrix,
    pattern_to_form: CfxMatrix,
}

impl CpdfPattern {
    pub fn new_tiling(object: ObjectRef, parent_matrix: CfxMatrix) -> Option<Self> {
        let dict = get_dict_object(&object)?;
        let borrowed = dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        Some(Self {
            shading_data: OnceCell::new(),
            shading_wrapper: RefCell::new(Weak::new()),
            kind: PatternKind::Tiling {
                colored: dict.get_integer_for(b"PaintType", 0) == 1,
            },
            pattern_to_form: dict.get_matrix_for(b"Matrix") * parent_matrix,
            parent_matrix,
            pattern_object: object,
        })
    }

    pub fn new_shading(
        object: ObjectRef,
        shading_object: bool,
        parent_matrix: CfxMatrix,
    ) -> Option<Self> {
        let pattern_to_form = if shading_object {
            CfxMatrix::default()
        } else {
            let dict = get_dict_object(&object)?;
            let borrowed = dict.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_matrix_for(b"Matrix")
                * parent_matrix
        };
        Some(Self {
            shading_data: OnceCell::new(),
            shading_wrapper: RefCell::new(Weak::new()),
            kind: PatternKind::Shading { shading_object },
            pattern_object: object,
            parent_matrix,
            pattern_to_form,
        })
    }

    pub fn kind(&self) -> PatternKind {
        self.kind
    }
    pub fn colored_tiling(&self) -> bool {
        matches!(self.kind, PatternKind::Tiling { colored: true })
    }
    pub fn pattern_object(&self) -> &ObjectRef {
        &self.pattern_object
    }
    pub fn parent_matrix(&self) -> CfxMatrix {
        self.parent_matrix
    }
    pub fn pattern_to_form(&self) -> CfxMatrix {
        self.pattern_to_form
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn tiling_and_shading_patterns_compose_parent_matrix() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"PaintType", CpdfScalarObject::number_from_signed(1));
        let mut values = CpdfArray::new();
        for value in [2, 0, 0, 3, 4, 5] {
            values.append_new(CpdfScalarObject::number_from_signed(value));
        }
        dict.set_new_for(b"Matrix", values);
        let object = retain_object(dict);
        let parent = CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 7.0, 11.0);
        let expected = CfxMatrix::new(2.0, 0.0, 0.0, 3.0, 11.0, 16.0);
        let tiling = CpdfPattern::new_tiling(object.clone(), parent).unwrap();
        assert_eq!(tiling.kind(), PatternKind::Tiling { colored: true });
        assert_eq!(tiling.pattern_to_form(), expected);
        let shading = CpdfPattern::new_shading(object.clone(), false, parent).unwrap();
        assert_eq!(
            shading.kind(),
            PatternKind::Shading {
                shading_object: false
            }
        );
        assert_eq!(shading.pattern_to_form(), expected);
        let direct = CpdfPattern::new_shading(object, true, parent).unwrap();
        assert_eq!(direct.pattern_to_form(), CfxMatrix::default());
    }
}
