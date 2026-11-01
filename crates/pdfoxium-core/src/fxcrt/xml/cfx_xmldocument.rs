// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fxcrt/xml/cfx_xmldocument.{h,cpp}`.

use super::cfx_xmlelement::CfxXmlElement;
use super::cfx_xmlinstruction::CfxXmlInstruction;
use super::cfx_xmlnode::{XmlNodeData, XmlNodeRef};
use super::xml_string::XmlString;
use crate::fxcrt::tree_node::TreeNodeOwner;
use std::collections::BTreeMap;

pub struct CfxXmlDocument {
    nodes: Vec<TreeNodeOwner<XmlNodeData>>,
    root: XmlNodeRef,
}
impl Default for CfxXmlDocument {
    fn default() -> Self {
        Self::new()
    }
}
impl CfxXmlDocument {
    pub fn new() -> Self {
        let root = TreeNodeOwner::new(XmlNodeData::Element(CfxXmlElement {
            name: XmlString::from(vec![114, 111, 111, 116]),
            attrs: BTreeMap::new(),
        }));
        let root_ref = XmlNodeRef(root.node_ref());
        Self {
            nodes: vec![root],
            root: root_ref,
        }
    }
    pub fn root(&self) -> XmlNodeRef {
        self.root.clone()
    }
    pub(crate) fn create_node(&mut self, data: XmlNodeData) -> XmlNodeRef {
        let node = TreeNodeOwner::new(data);
        let node_ref = XmlNodeRef(node.node_ref());
        self.nodes.push(node);
        node_ref
    }
    pub fn create_element(&mut self, name: &[u32]) -> XmlNodeRef {
        debug_assert!(!name.is_empty());
        self.create_node(XmlNodeData::Element(CfxXmlElement {
            name: XmlString::from(name),
            attrs: BTreeMap::new(),
        }))
    }
    pub fn create_element_owned(&mut self, name: Vec<u32>) -> XmlNodeRef {
        debug_assert!(!name.is_empty());
        self.create_node(XmlNodeData::Element(CfxXmlElement {
            name: XmlString::from(name),
            attrs: BTreeMap::new(),
        }))
    }
    pub fn create_text_owned(&mut self, text: Vec<u32>) -> XmlNodeRef {
        self.create_node(XmlNodeData::Text(XmlString::from(text)))
    }
    pub fn create_chardata_owned(&mut self, text: Vec<u32>) -> XmlNodeRef {
        self.create_node(XmlNodeData::CharData(XmlString::from(text)))
    }
    pub fn create_instruction_owned(&mut self, name: Vec<u32>) -> XmlNodeRef {
        self.create_node(XmlNodeData::Instruction(CfxXmlInstruction {
            name: XmlString::from(name),
            target_data: Vec::new(),
        }))
    }
    pub fn create_text(&mut self, text: &[u32]) -> XmlNodeRef {
        self.create_node(XmlNodeData::Text(XmlString::from(text)))
    }
    pub fn create_chardata(&mut self, text: &[u32]) -> XmlNodeRef {
        self.create_node(XmlNodeData::CharData(XmlString::from(text)))
    }
    pub fn create_instruction(&mut self, name: &[u32]) -> XmlNodeRef {
        self.create_node(XmlNodeData::Instruction(CfxXmlInstruction {
            name: XmlString::from(name),
            target_data: Vec::new(),
        }))
    }
    /// Move every allocation, including detached nodes and the source root,
    /// while preserving nonowning handles and the source's root handle.
    pub fn append_nodes_from(&mut self, other: &mut Self) {
        self.nodes.reserve(other.nodes.len());
        self.nodes.append(&mut other.nodes);
    }
}

#[cfg(test)]
#[path = "cfx_xmldocument_unittest.rs"]
mod upstream_tests;
