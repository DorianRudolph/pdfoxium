// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/page/cpdf_contentmarkitem.{h,cpp}`.

use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParamType {
    None,
    PropertiesDict,
    DirectDict,
}

pub struct CpdfContentMarkItem {
    param_type: ParamType,
    mark_name: Vec<u8>,
    property_name: Vec<u8>,
    properties_holder: Option<ObjectRef>,
    direct_dict: Option<ObjectRef>,
}

impl CpdfContentMarkItem {
    pub fn new(name: &[u8]) -> Self {
        Self {
            param_type: ParamType::None,
            mark_name: name.to_vec(),
            property_name: Vec::new(),
            properties_holder: None,
            direct_dict: None,
        }
    }
    pub fn name(&self) -> &[u8] {
        &self.mark_name
    }
    pub fn param_type(&self) -> ParamType {
        self.param_type
    }
    pub fn property_name(&self) -> &[u8] {
        &self.property_name
    }

    #[inline]
    pub fn param(&self) -> Option<ObjectRef> {
        match self.param_type {
            ParamType::None => None,
            ParamType::DirectDict => self.direct_dict.clone(),
            ParamType::PropertiesDict => {
                let holder = self.properties_holder.as_ref()?;
                let borrowed = holder.borrow();
                borrowed.as_dictionary()?.get_dict_for(&self.property_name)
            }
        }
    }

    /// Resolve an SDK mark's property dictionary without retaining the mark's
    /// RefCell borrow across lazy indirect-object parsing and input callbacks.
    pub fn param_from_ref(item: &Rc<RefCell<Self>>) -> Option<ObjectRef> {
        let (holder, property_name) = {
            let item = item.borrow();
            match item.param_type {
                ParamType::None => return None,
                ParamType::DirectDict => return item.direct_dict.clone(),
                ParamType::PropertiesDict => {
                    (item.properties_holder.clone()?, item.property_name.clone())
                }
            }
        };
        let property = {
            let borrowed = holder.borrow();
            borrowed.as_dictionary()?.get_object_for(&property_name)
        }?;
        crate::fpdfapi::parser::cpdf_object::get_dict_object(&property)
    }

    pub fn set_direct_dict(&mut self, dict: ObjectRef) {
        self.param_type = ParamType::DirectDict;
        self.direct_dict = Some(dict);
    }

    pub fn set_properties_holder(&mut self, holder: ObjectRef, property_name: &[u8]) {
        self.param_type = ParamType::PropertiesDict;
        self.properties_holder = Some(holder);
        self.property_name = property_name.to_vec();
    }
}
