// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Icon scaling and alignment from `core/fpdfdoc/cpdf_iconfit.{h,cpp}`.
use super::cpdf_formfield::{array_at, array_len, dict_bytes, dict_get, typed};
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF, CfxSizeF, CfxVectorF};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleMethod {
    Always,
    Bigger,
    Smaller,
    Never,
}
#[derive(Clone)]
pub struct CpdfIconFit {
    dict: Option<ObjectRef>,
}
impl CpdfIconFit {
    pub fn new(dict: Option<ObjectRef>) -> Self {
        Self { dict }
    }
    pub fn scale_method(&self) -> ScaleMethod {
        match self.dict.as_ref().map(|d| dict_bytes(d, b"SW")).as_deref() {
            Some(b"B") => ScaleMethod::Bigger,
            Some(b"S") => ScaleMethod::Smaller,
            Some(b"N") => ScaleMethod::Never,
            _ => ScaleMethod::Always,
        }
    }
    pub fn is_proportional_scale(&self) -> bool {
        self.dict
            .as_ref()
            .is_none_or(|d| dict_bytes(d, b"S") != b"A")
    }
    fn alignment(&self, default: f32) -> CfxPointF {
        let array = self
            .dict
            .as_ref()
            .and_then(|d| typed(dict_get(d, b"A"), ObjectType::Array));
        let mut result = CfxPointF::new(default, default);
        if let Some(array) = array {
            if array_len(&array) > 0 {
                result.x = array_at(&array, 0).map_or(0., |o| o.borrow().get_number());
            }
            if array_len(&array) > 1 {
                result.y = array_at(&array, 1).map_or(0., |o| o.borrow().get_number());
            }
        }
        result
    }
    pub fn icon_bottom_left_position(&self) -> CfxPointF {
        self.alignment(0.5)
    }
    pub fn fitting_bounds(&self) -> bool {
        self.dict
            .as_ref()
            .and_then(|d| dict_get(d, b"FB"))
            .is_some_and(|o| {
                o.borrow().object_type() == ObjectType::Boolean && o.borrow().get_integer() != 0
            })
    }
    pub fn scale(&self, image: CfxSizeF, plate: CfxFloatRect) -> CfxVectorF {
        let mut x = 1.;
        let mut y = 1.;
        let pw = plate.width();
        let ph = plate.height();
        match self.scale_method() {
            ScaleMethod::Always => {
                x = pw / image.width.max(1.);
                y = ph / image.height.max(1.);
            }
            ScaleMethod::Bigger => {
                if pw < image.width {
                    x = pw / image.width.max(1.);
                }
                if ph < image.height {
                    y = ph / image.height.max(1.);
                }
            }
            ScaleMethod::Smaller => {
                if pw > image.width {
                    x = pw / image.width.max(1.);
                }
                if ph > image.height {
                    y = ph / image.height.max(1.);
                }
            }
            ScaleMethod::Never => (),
        }
        if self.is_proportional_scale() {
            x = x.min(y);
            y = x;
        }
        CfxVectorF::new(x, y)
    }
    pub fn image_offset(
        &self,
        image: CfxSizeF,
        scale: CfxVectorF,
        plate: CfxFloatRect,
    ) -> CfxVectorF {
        let position = self.alignment(0.);
        CfxVectorF::new(
            (plate.width() - image.width * scale.x) * position.x,
            (plate.height() - image.height * scale.y) * position.y,
        )
    }
}
