// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Annotation appearance parsing from `core/fpdfapi/page/cpdf_annotcontext.{h,cpp}`.
//! The C ABI retains the non-owning page association; engine calls borrow it.
use super::cpdf_form::CpdfForm;
use super::cpdf_page::CpdfPage;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

pub struct CpdfAnnotContext {
    dict: ObjectRef,
    form: Option<CpdfForm>,
}
impl CpdfAnnotContext {
    pub fn new(dict: ObjectRef) -> Self {
        Self { dict, form: None }
    }
    pub fn dict(&self) -> &ObjectRef {
        &self.dict
    }
    pub fn form(&self) -> Option<&CpdfForm> {
        self.form.as_ref()
    }
    pub fn form_mut(&mut self) -> Option<&mut CpdfForm> {
        self.form.as_mut()
    }
    pub fn set_form(&mut self, page: &CpdfPage, stream: ObjectRef) -> bool {
        let dict = {
            let borrowed = stream.borrow();
            let Some(stream) = borrowed.as_any().downcast_ref::<CpdfStream>() else {
                return false;
            };
            stream.get_dict()
        };
        let inverse = dict
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .expect("appearance stream dictionary")
            .get_matrix_for(b"Matrix")
            .get_inverse();
        let Some(mut form) = CpdfForm::new(stream, page.resources(), None) else {
            return false;
        };
        // Annotations parse their contents with identity rather than the AP's
        // matrix. Cancel it through the parent matrix without editing the PDF.
        form.parse_content(
            None,
            Some(inverse),
            page.page_data_ref(),
            Rc::new(RefCell::new(HashSet::new())),
            0,
        );
        self.form = Some(form);
        true
    }
}
