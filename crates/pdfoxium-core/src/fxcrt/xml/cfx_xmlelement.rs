// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxcrt/xml/cfx_xmlelement.{h,cpp}`.
use super::cfx_xmldocument::CfxXmlDocument;
use super::cfx_xmlnode::{XmlNodeData, XmlNodeRef, XmlNodeType};
use super::xml_string::{XmlString, XmlStringKey, XmlStringView};
use crate::fxcrt::fx_stream::WriteStream;
use crate::fxcrt::fx_string::{encode_entities, utf8_encode};
use std::collections::BTreeMap;

pub(crate) struct CfxXmlElement {
    pub name: XmlString,
    pub attrs: BTreeMap<XmlString, XmlString>,
}

impl XmlNodeRef {
    pub fn name(&self) -> XmlString {
        self.with(|data| match data {
            XmlNodeData::Element(elem) => elem.name.clone(),
            XmlNodeData::Instruction(instruction) => instruction.name.clone(),
            _ => panic!("XML node has no name"),
        })
    }
    pub fn with_attributes<R>(&self, f: impl FnOnce(&BTreeMap<XmlString, XmlString>) -> R) -> R {
        self.with(|data| match data {
            XmlNodeData::Element(elem) => f(&elem.attrs),
            _ => panic!("XML node is not an element"),
        })
    }
    pub fn attribute_count(&self) -> usize {
        self.with_attributes(BTreeMap::len)
    }
    pub fn has_attribute(&self, name: &[u32]) -> bool {
        self.with_attributes(|attrs| {
            attrs.contains_key::<dyn XmlStringKey + '_>(&XmlStringView(name))
        })
    }
    pub fn get_attribute(&self, name: &[u32]) -> XmlString {
        self.with_attributes(|attrs| {
            attrs
                .get::<dyn XmlStringKey + '_>(&XmlStringView(name))
                .cloned()
                .unwrap_or_else(|| XmlString::default())
        })
    }
    pub fn set_attribute(&self, name: &[u32], value: &[u32]) {
        self.with_mut(|data| match data {
            XmlNodeData::Element(elem) => {
                elem.attrs
                    .insert(XmlString::from(name), XmlString::from(value));
            }
            _ => panic!("XML node is not an element"),
        });
    }
    pub fn set_attribute_owned(&self, name: Vec<u32>, value: Vec<u32>) {
        self.with_mut(|data| match data {
            XmlNodeData::Element(elem) => {
                elem.attrs
                    .insert(XmlString::from(name), XmlString::from(value));
            }
            _ => panic!("XML node is not an element"),
        });
    }
    pub fn remove_attribute(&self, name: &[u32]) {
        self.with_mut(|data| match data {
            XmlNodeData::Element(elem) => {
                elem.attrs
                    .remove::<dyn XmlStringKey + '_>(&XmlStringView(name));
            }
            _ => panic!("XML node is not an element"),
        });
    }
    pub fn first_child_named(&self, name: &[u32]) -> Option<Self> {
        self.nth_child_named(name, 0)
    }
    pub fn nth_child_named(&self, name: &[u32], mut index: usize) -> Option<Self> {
        let mut child = self.first_child();
        while let Some(node) = child {
            if node.node_type() == XmlNodeType::Element && node.name().as_ref() == name {
                if index == 0 {
                    return Some(node);
                }
                index -= 1;
            }
            child = node.next_sibling();
        }
        None
    }
    pub fn local_tag_name(&self) -> XmlString {
        let name = self.name();
        if let Some(pos) = name.iter().position(|&c| c == b':' as u32) {
            XmlString::from(&name[pos + 1..])
        } else {
            name
        }
    }
    pub fn namespace_prefix(&self) -> XmlString {
        let name = self.name();
        name.iter()
            .position(|&c| c == b':' as u32)
            .map_or_else(|| XmlString::default(), |pos| XmlString::from(&name[..pos]))
    }
    pub fn namespace_uri(&self) -> XmlString {
        let prefix = self.namespace_prefix();
        let mut attr: Vec<u32> = "xmlns".chars().map(u32::from).collect();
        if !prefix.is_empty() {
            attr.push(b':' as u32);
            attr.extend_from_slice(&prefix);
        }
        let mut node = Some(self.clone());
        while let Some(element) = node {
            if element.node_type() != XmlNodeType::Element {
                break;
            }
            if element.has_attribute(&attr) {
                return element.get_attribute(&attr);
            }
            node = element.parent();
        }
        XmlString::default()
    }
    pub fn text_data(&self) -> Vec<u32> {
        let mut buffer = Vec::new();
        let mut child = self.first_child();
        while let Some(node) = child {
            if node.is_text() {
                node.with_text(|text| buffer.extend_from_slice(text));
            }
            child = node.next_sibling();
        }
        buffer
    }
    pub(crate) fn clone_element(&self, doc: &mut CfxXmlDocument) -> Self {
        // This map copy is intentional upstream Clone behavior. WideString
        // names and values share backing storage, represented by Rc slices.
        let name = self.name();
        let attrs = self.with_attributes(Clone::clone);
        let node = doc.create_node(XmlNodeData::Element(CfxXmlElement { name, attrs }));
        // TODO(dsinclair): This clone is wrong. It doesn't clone all child
        // nodes just text nodes?
        let mut child = self.first_child();
        while let Some(source) = child {
            if source.node_type() == XmlNodeType::Text {
                node.append_last_child(&source.clone_into(doc));
            }
            child = source.next_sibling();
        }
        node
    }
    pub(crate) fn save_element(&self, stream: &mut dyn WriteStream) {
        let name = utf8_encode(&self.name());
        stream.write_string(b"<");
        stream.write_string(&name);
        // A live ordered cursor releases payload borrows before callbacks and
        // observes source-valid updates/insertions to later map entries. Safe
        // BTreeMap range lookup costs O(log n) per attribute instead of the
        // source iterator's amortized O(1) increment; text buffers are shared.
        let mut previous: Option<XmlString> = None;
        loop {
            let attribute = self.with_attributes(|attrs| {
                use std::ops::Bound::{Excluded, Unbounded};
                let entry = if let Some(previous) = &previous {
                    let key: &(dyn XmlStringKey + '_) = previous;
                    attrs
                        .range::<dyn XmlStringKey + '_, _>((Excluded(key), Unbounded))
                        .next()
                } else {
                    attrs.iter().next()
                };
                entry.map(|(name, value)| (name.clone(), value.clone()))
            });
            let Some((name, value)) = attribute else {
                break;
            };
            previous = Some(name.clone());
            let encoded_value = encode_entities(&value);
            let mut attribute = vec![b' ' as u32];
            attribute.extend_from_slice(&name);
            attribute.extend_from_slice(&[b'=' as u32, b'"' as u32]);
            attribute.extend_from_slice(&encoded_value);
            attribute.push(b'"' as u32);
            stream.write_string(&utf8_encode(&attribute));
        }
        // Formatting whitespace becomes real text on reload, particularly in
        // mixed content. Serialize only the text nodes present in the DOM.
        if self.first_child().is_none() {
            stream.write_string(b" />");
            return;
        }
        stream.write_string(b">");
        let mut child = self.first_child();
        while let Some(node) = child {
            node.save(stream);
            child = node.next_sibling();
        }
        stream.write_string(b"</");
        stream.write_string(&name);
        stream.write_string(b">");
    }
}
#[cfg(test)]
#[path = "cfx_xmlelement_unittest.rs"]
mod upstream_tests;
