// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Node creation state from `fxjs/xfa/cfxjse_nodehelper.{h,cpp}`.
use super::cfxjse_resolveprocessor::{ResolveError, ResolveResultType};
use crate::{
    fxcrt::fx_system::wtoi,
    xfa::fxfa::{
        fxfa_basic::{XfaAttribute, XfaElement, XfaHashCode},
        parser::{
            cxfa_document::CxfaDocument, cxfa_node::CxfaNode, xfa_basic_data::get_element_by_name,
        },
    },
};

pub struct CfxJseNodeHelper {
    pub last_create_type: XfaElement,
    pub create_flag: ResolveResultType,
    pub create_count: usize,
    pub cur_all_start: Option<usize>,
    pub create_parent: Option<CxfaNode>,
    pub all_start_parent: Option<CxfaNode>,
}
impl Default for CfxJseNodeHelper {
    fn default() -> Self {
        Self {
            last_create_type: XfaElement::DataValue,
            create_flag: ResolveResultType::CreateNodeOne,
            create_count: 0,
            cur_all_start: None,
            create_parent: None,
            all_start_parent: None,
        }
    }
}
impl CfxJseNodeHelper {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn create_node_for_condition(&mut self, condition: &[u32]) -> bool {
        if condition.is_empty() {
            self.create_flag = ResolveResultType::CreateNodeOne;
            return false;
        }
        if condition[0] != 91 {
            return false;
        }
        let mut i = 1;
        while i < condition.len() {
            if condition[i] == 42 {
                self.create_flag = ResolveResultType::CreateNodeAll;
                self.create_count = 1;
                return true;
            }
            if condition[i] != 32 {
                break;
            }
            i += 1;
        }
        self.create_flag = ResolveResultType::CreateNodeOne;
        let end = condition.len().saturating_sub(1);
        let Some(digits) = condition.get(i..end) else {
            return false;
        };
        let count = wtoi(digits);
        if count < 0 {
            return false;
        }
        self.create_count = count as usize;
        true
    }
    pub fn create_node(
        &mut self,
        name: &[u32],
        condition: &[u32],
        last_node: bool,
        document: &CxfaDocument,
    ) -> Result<bool, ResolveError> {
        if self.create_parent.is_none() {
            return Ok(false);
        }
        let mut name = name;
        if name.first() == Some(&33) {
            name = &name[1..];
            self.create_parent = document.get_xfa_node(XfaHashCode::Datasets as u32);
        }
        let is_class = name.first() == Some(&35);
        if is_class {
            name = &name[1..];
        }
        if name.is_empty() {
            return Ok(false);
        }
        if self.create_count == 0 {
            self.create_node_for_condition(condition);
        }
        let element = if is_class {
            get_element_by_name(name)
        } else if last_node {
            self.last_create_type
        } else {
            XfaElement::DataGroup
        };
        if element == XfaElement::Unknown {
            return Ok(false);
        }
        let Some(parent) = self.create_parent.clone() else {
            return Ok(false);
        };
        let mut result = false;
        for i in 0..self.create_count {
            let Some(node) = parent.create_same_packet_node(element) else {
                continue;
            };
            if !is_class {
                node.set_attribute_by_enum(XfaAttribute::Name, name, false);
                if node.create_xml_mapping_node().is_none() {
                    return Err(ResolveError::MissingXmlDocument);
                }
            }
            parent.insert_child_and_notify(-1, &node);
            if i == self.create_count - 1 {
                self.create_parent = Some(node);
            }
            result = true;
        }
        if !result {
            self.create_parent = None;
        }
        Ok(result)
    }
    pub fn set_create_node_type(&mut self, reference: Option<&CxfaNode>) {
        let Some(node) = reference else {
            return;
        };
        match node.get_element_type() {
            XfaElement::Subform => self.last_create_type = XfaElement::DataGroup,
            XfaElement::Field => {
                self.last_create_type = if node.is_field_multi_list_box() {
                    XfaElement::DataGroup
                } else {
                    XfaElement::DataValue
                }
            }
            XfaElement::ExclGroup => self.last_create_type = XfaElement::DataValue,
            _ => {}
        }
    }
}
