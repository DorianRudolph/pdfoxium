// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxcrt/xml/cfx_xmlnode.{h,cpp}`. The document owns nodes, and this
//! handle and every tree link are weak, like the source's unowned pointers.

use super::cfx_xmldocument::CfxXmlDocument;
use super::cfx_xmlelement::CfxXmlElement;
use super::cfx_xmlinstruction::CfxXmlInstruction;
use super::xml_string::XmlString;
use crate::fxcrt::fx_stream::WriteStream;
use crate::fxcrt::tree_node::TreeNodeRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum XmlNodeType {
    Instruction = 0,
    Element,
    Text,
    CharData,
}

pub(crate) enum XmlNodeData {
    Instruction(CfxXmlInstruction),
    Element(CfxXmlElement),
    Text(XmlString),
    CharData(XmlString),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct XmlNodeRef(pub(crate) TreeNodeRef<XmlNodeData>);

impl XmlNodeRef {
    pub fn is_alive(&self) -> bool {
        self.0.is_alive()
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self == other
    }
    pub(crate) fn with<R>(&self, f: impl FnOnce(&XmlNodeData) -> R) -> R {
        self.0.with_value(f)
    }
    pub(crate) fn with_mut<R>(&self, f: impl FnOnce(&mut XmlNodeData) -> R) -> R {
        self.0.with_value_mut(f)
    }
    pub fn node_type(&self) -> XmlNodeType {
        self.with(|data| match data {
            XmlNodeData::Instruction(_) => XmlNodeType::Instruction,
            XmlNodeData::Element(_) => XmlNodeType::Element,
            XmlNodeData::Text(_) => XmlNodeType::Text,
            XmlNodeData::CharData(_) => XmlNodeType::CharData,
        })
    }
    pub fn parent(&self) -> Option<Self> {
        self.0.parent().map(Self)
    }
    pub fn first_child(&self) -> Option<Self> {
        self.0.first_child().map(Self)
    }
    pub fn last_child(&self) -> Option<Self> {
        self.0.last_child().map(Self)
    }
    pub fn next_sibling(&self) -> Option<Self> {
        self.0.next_sibling().map(Self)
    }
    pub fn prev_sibling(&self) -> Option<Self> {
        self.0.prev_sibling().map(Self)
    }
    pub fn nth_child(&self, index: i32) -> Option<Self> {
        self.0.nth_child(index).map(Self)
    }
    pub fn has_child(&self, child: &Self) -> bool {
        self.0.has_child(&child.0)
    }
    pub fn append_first_child(&self, child: &Self) {
        self.0.append_first_child(&child.0);
    }
    pub fn append_last_child(&self, child: &Self) {
        self.0.append_last_child(&child.0);
    }
    pub fn insert_before(&self, child: &Self, other: Option<&Self>) {
        self.0.insert_before(&child.0, other.map(|n| &n.0));
    }
    pub fn insert_after(&self, child: &Self, other: Option<&Self>) {
        self.0.insert_after(&child.0, other.map(|n| &n.0));
    }
    pub fn remove_child(&self, child: &Self) {
        self.0.remove_child(&child.0);
    }
    pub fn remove_all_children(&self) {
        self.0.remove_all_children();
    }
    pub fn remove_self_if_parented(&self) {
        self.0.remove_self_if_parented();
    }
    pub fn insert_child_node(&self, child: &Self, index: i32) {
        self.insert_before(child, self.nth_child(index).as_ref());
    }
    pub fn root(&self) -> Self {
        let mut parent = self.clone();
        while let Some(next) = parent.parent() {
            parent = next;
        }
        parent
    }
    pub fn clone_into(&self, doc: &mut CfxXmlDocument) -> Self {
        match self.node_type() {
            XmlNodeType::Element => self.clone_element(doc),
            XmlNodeType::Instruction => self.clone_instruction(doc),
            XmlNodeType::Text => doc.create_node(XmlNodeData::Text(self.text())),
            XmlNodeType::CharData => doc.create_node(XmlNodeData::CharData(self.text())),
        }
    }
    pub fn save(&self, stream: &mut dyn WriteStream) {
        match self.node_type() {
            XmlNodeType::Element => self.save_element(stream),
            XmlNodeType::Instruction => self.save_instruction(stream),
            XmlNodeType::Text => self.save_text(stream),
            XmlNodeType::CharData => self.save_chardata(stream),
        }
    }
}

#[cfg(test)]
#[path = "cfx_xmlnode_unittest.rs"]
mod upstream_tests;
