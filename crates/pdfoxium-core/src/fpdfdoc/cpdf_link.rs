// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Link data queries from `core/fpdfdoc/cpdf_link.{h,cpp}`.
use super::{cpdf_action::CpdfAction, cpdf_dest::CpdfDest};
use crate::fpdfapi::parser::{
    cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument, cpdf_object::ObjectRef,
};
use crate::fxcrt::fx_coordinates::CfxFloatRect;
#[derive(Clone, Default)]
pub struct CpdfLink {
    dict: Option<ObjectRef>,
}
impl CpdfLink {
    pub fn new(dict: Option<ObjectRef>) -> Self {
        Self { dict }
    }
    pub fn dict(&self) -> Option<ObjectRef> {
        self.dict.clone()
    }
    pub fn rect(&self) -> CfxFloatRect {
        self.dict
            .as_ref()
            .map_or_else(CfxFloatRect::default, |object| {
                object
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()
                    .map_or_else(CfxFloatRect::default, |dict| dict.get_rect_for(b"Rect"))
            })
    }
    pub fn dest(&self, document: &CpdfDocument) -> CpdfDest {
        let value = self.dict.as_ref().and_then(|object| {
            object
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_direct_object_for(b"Dest")
        });
        CpdfDest::create(document, value)
    }
    pub fn action(&self) -> CpdfAction {
        CpdfAction::new(self.dict.as_ref().and_then(|object| {
            object
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_dict_for(b"A")
        }))
    }
}
