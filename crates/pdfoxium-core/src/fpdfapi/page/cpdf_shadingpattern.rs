// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_shadingpattern.{h,cpp}`.

use super::cpdf_colorspace::{ColorSpaceFamily, ColorSpaceRef};
use super::cpdf_docpagedata::CpdfDocPageData;
use super::cpdf_function::CpdfFunction;
use super::cpdf_pattern::{CpdfPattern, PatternKind, PatternRef};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_dict_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fxcrt::fx_coordinates::CfxMatrix;
use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(i32)]
pub enum ShadingType {
    #[default]
    Invalid = 0,
    FunctionBased = 1,
    Axial = 2,
    Radial = 3,
    FreeFormGouraudTriangleMesh = 4,
    LatticeFormGouraudTriangleMesh = 5,
    CoonsPatchMesh = 6,
    TensorProductPatchMesh = 7,
}

impl ShadingType {
    pub fn from_pdf_integer(value: i32) -> Self {
        match value {
            1 => Self::FunctionBased,
            2 => Self::Axial,
            3 => Self::Radial,
            4 => Self::FreeFormGouraudTriangleMesh,
            5 => Self::LatticeFormGouraudTriangleMesh,
            6 => Self::CoonsPatchMesh,
            7 => Self::TensorProductPatchMesh,
            _ => Self::Invalid,
        }
    }
    pub fn is_mesh(self) -> bool {
        matches!(
            self,
            Self::FreeFormGouraudTriangleMesh
                | Self::LatticeFormGouraudTriangleMesh
                | Self::CoonsPatchMesh
                | Self::TensorProductPatchMesh
        )
    }
}

#[derive(Default)]
pub(crate) struct ShadingData {
    shading_type: Cell<ShadingType>,
    color_space: RefCell<Option<ColorSpaceRef>>,
    functions: RefCell<Vec<Option<CpdfFunction>>>,
}

pub struct CpdfShadingPattern {
    base: PatternRef,
}

impl CpdfShadingPattern {
    pub fn new(object: ObjectRef, shading: bool, parent_matrix: CfxMatrix) -> Option<Self> {
        Self::from_pattern(Rc::new(CpdfPattern::new_shading(
            object,
            shading,
            parent_matrix,
        )?))
    }
    pub fn from_pattern(base: PatternRef) -> Option<Self> {
        matches!(base.kind(), PatternKind::Shading { .. }).then_some(Self { base })
    }
    pub(crate) fn retained(base: PatternRef) -> Option<Rc<Self>> {
        if let Some(pattern) = base.shading_wrapper.borrow().upgrade() {
            return Some(pattern);
        }
        let pattern = Rc::new(Self::from_pattern(Rc::clone(&base))?);
        *base.shading_wrapper.borrow_mut() = Rc::downgrade(&pattern);
        Some(pattern)
    }
    fn data(&self) -> &ShadingData {
        self.base.shading_data.get_or_init(ShadingData::default)
    }
    pub fn pattern(&self) -> &CpdfPattern {
        &self.base
    }
    pub fn shading_type(&self) -> ShadingType {
        self.data().shading_type.get()
    }
    pub fn is_mesh_shading(&self) -> bool {
        self.shading_type().is_mesh()
    }
    pub fn is_shading_object(&self) -> bool {
        matches!(
            self.base.kind(),
            PatternKind::Shading {
                shading_object: true
            }
        )
    }
    pub fn color_space(&self) -> Option<ColorSpaceRef> {
        self.data().color_space.borrow().clone()
    }
    pub fn functions(&self) -> Ref<'_, [Option<CpdfFunction>]> {
        Ref::map(self.data().functions.borrow(), Vec::as_slice)
    }
    pub fn shading_object(&self) -> Option<ObjectRef> {
        if self.is_shading_object() {
            return Some(self.base.pattern_object().clone());
        }
        let dict = get_dict_object(self.base.pattern_object())?;
        let borrowed = dict.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_direct_object_for(b"Shading")
    }
    pub fn load(&self, page_data: &CpdfDocPageData) -> bool {
        if self.shading_type() != ShadingType::Invalid {
            return true;
        }
        let Some(shading_object) = self.shading_object() else {
            return false;
        };
        let Some(dict_object) = get_dict_object(&shading_object) else {
            return false;
        };
        let borrowed = dict_object.borrow();
        let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
            return false;
        };
        let mut functions = self.data().functions.borrow_mut();
        functions.clear();
        if let Some(function) = dict.get_direct_object_for(b"Function") {
            let function_borrowed = function.borrow();
            if let Some(array) = function_borrowed.as_any().downcast_ref::<CpdfArray>() {
                *functions = (0..array.size().min(4))
                    .map(|index| {
                        array
                            .get_direct_object_at(index)
                            .and_then(|object| CpdfFunction::load(&object))
                    })
                    .collect();
            } else {
                drop(function_borrowed);
                functions.push(CpdfFunction::load(&function));
            }
        }
        drop(functions);
        let Some(cs_object) = dict.get_direct_object_for(b"ColorSpace") else {
            return false;
        };
        *self.data().color_space.borrow_mut() = page_data.get_color_space(&cs_object, None);
        let Some(color_space) = self.color_space() else {
            return false;
        };
        if color_space.family() == ColorSpaceFamily::Pattern {
            return false;
        }
        self.data().shading_type.set(ShadingType::from_pdf_integer(
            dict.get_integer_for(b"ShadingType", 0),
        ));
        self.validate()
    }
    fn validate(&self) -> bool {
        let Some(cs) = self.color_space() else {
            return false;
        };
        let shading_type = self.shading_type();
        if shading_type == ShadingType::Invalid {
            return false;
        }
        if shading_type.is_mesh()
            && self
                .shading_object()
                .is_none_or(|o| o.borrow().object_type() != ObjectType::Stream)
        {
            return false;
        }
        if cs.family() == ColorSpaceFamily::Indexed
            && (!shading_type.is_mesh() || !self.functions().is_empty())
        {
            return false;
        }
        let components = cs.component_count();
        match shading_type {
            ShadingType::FunctionBased => {
                self.validate_functions(1, 2, components)
                    || self.validate_functions(components, 2, 1)
            }
            ShadingType::Axial | ShadingType::Radial => {
                self.validate_functions(1, 1, components)
                    || self.validate_functions(components, 1, 1)
            }
            _ => {
                self.functions().is_empty()
                    || self.validate_functions(1, 1, components)
                    || self.validate_functions(components, 1, 1)
            }
        }
    }
    fn validate_functions(&self, expected_count: u32, inputs: u32, outputs: u32) -> bool {
        let functions = self.functions();
        if functions.len() != expected_count as usize {
            return false;
        }
        let mut total_outputs = 0_u32;
        for function in functions.iter() {
            let Some(function) = function else {
                return false;
            };
            if function.input_count() != inputs || function.output_count() != outputs {
                return false;
            }
            let Some(total) = total_outputs.checked_add(function.output_count()) else {
                return false;
            };
            total_outputs = total;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    fn float_array(values: &[f32]) -> CpdfArray {
        let mut array = CpdfArray::new();
        for &value in values {
            array.append_new(CpdfScalarObject::number_from_float(value));
        }
        array
    }

    #[test]
    fn rejects_mesh_without_stream() {
        let data = CpdfDocPageData::new();
        let mut mesh = CpdfDictionary::new();
        mesh.set_new_for(b"ShadingType", CpdfScalarObject::number_from_signed(4));
        mesh.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        let pattern =
            CpdfShadingPattern::new(retain_object(mesh), true, CfxMatrix::default()).unwrap();
        assert!(!pattern.load(&data));
        assert_eq!(
            pattern.shading_type(),
            ShadingType::FreeFormGouraudTriangleMesh
        );
    }

    #[test]
    fn loads_axial_shading_with_matching_function() {
        let data = CpdfDocPageData::new();
        let mut function = CpdfDictionary::new();
        function.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
        function.set_new_for(b"Domain", float_array(&[0.0, 1.0]));
        function.set_new_for(b"C0", float_array(&[0.0]));
        function.set_new_for(b"C1", float_array(&[1.0]));
        function.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
        let mut shading = CpdfDictionary::new();
        shading.set_new_for(b"ShadingType", CpdfScalarObject::number_from_signed(2));
        shading.set_new_name_for(b"ColorSpace", b"DeviceGray");
        shading.set_new_for(b"Function", function);
        let pattern =
            CpdfShadingPattern::new(retain_object(shading), true, CfxMatrix::default()).unwrap();
        assert!(pattern.load(&data));
        assert_eq!(pattern.shading_type(), ShadingType::Axial);
        assert_eq!(pattern.functions().len(), 1);
        assert!(pattern.load(&data));
    }
}
