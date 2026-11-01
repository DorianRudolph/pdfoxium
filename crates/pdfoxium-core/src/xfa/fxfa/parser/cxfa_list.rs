// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Native list interface from `cxfa_list.{h,cpp}`. Executable list wrappers
//! connect through `fxjs::xfa::cjx_list` and the document Boa context.
use super::{cxfa_document::CxfaDocument, cxfa_node::CxfaNode, node_schema::XfaObjectType};
use crate::xfa::fxfa::fxfa_basic::XfaElement;
use std::rc::Rc;

pub trait CxfaList {
    fn document(&self) -> Option<Rc<CxfaDocument>>;
    fn get_length(&self) -> usize;
    fn append(&self, node: &CxfaNode) -> bool;
    fn insert(&self, node: &CxfaNode, before: Option<&CxfaNode>) -> bool;
    fn remove(&self, node: &CxfaNode);
    fn item(&self, index: usize) -> Option<CxfaNode>;
    fn get_object_type(&self) -> XfaObjectType {
        XfaObjectType::TreeList
    }
    fn get_element_type(&self) -> XfaElement {
        XfaElement::TreeList
    }
}
