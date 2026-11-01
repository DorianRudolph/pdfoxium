// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpdf_dest.{h,cpp}` destination parsing and view parameters.
use super::cpdf_nametree::CpdfNameTree;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
const MODES: [&[u8]; 9] = [
    b"Unknown", b"XYZ", b"Fit", b"FitH", b"FitV", b"FitR", b"FitB", b"FitBH", b"FitBV",
];
const PARAMS: [usize; 9] = [0, 3, 0, 1, 1, 4, 0, 1, 1];
#[derive(Clone, Default)]
pub struct CpdfDest {
    array: Option<ObjectRef>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DestXyz {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub zoom: Option<f32>,
}
impl CpdfDest {
    pub fn new(array: Option<ObjectRef>) -> Self {
        Self {
            array: array.filter(|v| v.borrow().object_type() == ObjectType::Array),
        }
    }
    pub fn create(document: &CpdfDocument, dest: Option<ObjectRef>) -> Self {
        let Some(dest) = dest else {
            return Self::default();
        };
        let kind = dest.borrow().object_type();
        if matches!(kind, ObjectType::String | ObjectType::Name) {
            Self::new(CpdfNameTree::lookup_named_dest(
                document,
                &dest.borrow().get_string(),
            ))
        } else {
            Self::new(Some(dest))
        }
    }
    pub fn array(&self) -> Option<ObjectRef> {
        self.array.clone()
    }
    fn entry(&self, index: usize) -> Option<ObjectRef> {
        let value = self
            .array
            .as_ref()?
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()?
            .get_object_at(index)?;
        get_direct_object(&value)
    }
    fn size(&self) -> usize {
        self.array.as_ref().map_or(0, |a| {
            a.borrow()
                .as_any()
                .downcast_ref::<CpdfArray>()
                .unwrap()
                .size()
        })
    }
    pub fn dest_page_index(&self, document: &mut CpdfDocument) -> i32 {
        let Some(page) = self.entry(0) else {
            return -1;
        };
        let (kind, number, index) = {
            let p = page.borrow();
            (p.object_type(), p.obj_num(), p.get_integer())
        };
        match kind {
            ObjectType::Number => index,
            ObjectType::Dictionary => document
                .page_index(number)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
            _ => -1,
        }
    }
    pub fn scroll_position_array(&self) -> Vec<f32> {
        (2..self.size()).map(|i| self.param(i - 2)).collect()
    }
    pub fn zoom_mode(&self) -> i32 {
        let Some(mode) = self.entry(1) else {
            return 0;
        };
        let mode = mode.borrow().get_string();
        MODES
            .iter()
            .position(|&name| name == mode && name != b"Unknown")
            .map_or(0, |i| i as i32)
    }
    pub fn num_params(&self) -> usize {
        self.size()
            .saturating_sub(2)
            .min(PARAMS[self.zoom_mode() as usize])
    }
    pub fn param(&self, index: usize) -> f32 {
        index
            .checked_add(2)
            .and_then(|i| self.entry(i))
            .map_or(0.0, |v| v.borrow().get_number())
    }
    pub fn xyz(&self) -> Option<DestXyz> {
        if self.size() < 5 {
            return None;
        }
        let name = self.entry(1)?;
        if name.borrow().object_type() != ObjectType::Name || name.borrow().get_string() != b"XYZ" {
            return None;
        }
        let number = |index| {
            self.entry(index)
                .filter(|v| v.borrow().object_type() == ObjectType::Number)
                .map(|v| v.borrow().get_number())
        };
        Some(DestXyz {
            x: number(2),
            y: number(3),
            zoom: number(4).filter(|&zoom| zoom != 0.0),
        })
    }
}
#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    #[test]
    fn GetXYZ() {
        let mut array = CpdfArray::new();
        array.append_new(CpdfScalarObject::number_from_signed(0));
        array.append_new_name(b"XYZ");
        array.append_new(CpdfScalarObject::number_from_signed(4));
        let array = retain_object(array);
        assert_eq!(CpdfDest::default().xyz(), None);
        let dest = CpdfDest::new(Some(array.clone()));
        assert_eq!(dest.xyz(), None);
        {
            let mut borrowed = array.borrow_mut();
            let a = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
            a.append_new(CpdfScalarObject::number_from_signed(5));
            a.append_new(CpdfScalarObject::number_from_signed(6));
        }
        assert_eq!(
            dest.xyz(),
            Some(DestXyz {
                x: Some(4.0),
                y: Some(5.0),
                zoom: Some(6.0)
            })
        );
        array
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .set_new_at(4, CpdfScalarObject::number_from_signed(0));
        assert_eq!(dest.xyz().unwrap().zoom, None);
        for i in 2..5 {
            array
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfArray>()
                .unwrap()
                .set_new_at(i, CpdfScalarObject::null());
        }
        assert_eq!(
            dest.xyz(),
            Some(DestXyz {
                x: None,
                y: None,
                zoom: None
            })
        );
    }
}
