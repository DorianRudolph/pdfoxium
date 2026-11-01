// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Packet and data loading from `xfa/fxfa/parser/cxfa_document_builder.{h,cpp}`.
use super::{
    cxfa_document::CxfaDocument,
    cxfa_node::CxfaNode,
    node_schema::XfaObjectType,
    xfa_basic_data::{
        get_attribute_by_name, get_element_by_name, get_packet_by_index, get_packet_by_name,
        XfaPacketMatch, XfaPacketSupport,
    },
    xfa_utils::resolve_namespace_qualifier,
};
use crate::fxcrt::xml::{
    cfx_xmldocument::CfxXmlDocument,
    cfx_xmlnode::{XmlNodeRef, XmlNodeType},
    xml_string::XmlString,
};
use crate::xfa::fxfa::fxfa_basic::{XfaAttribute, XfaAttributeValue, XfaElement, XfaPacketType};
use std::{cell::RefCell, rc::Rc};

const MAX_EXECUTE_RECURSION: usize = 1000;
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn ascii(text: &[u32], value: &str) -> bool {
    text.iter().copied().eq(value.bytes().map(u32::from))
}
fn children(node: &XmlNodeRef) -> impl Iterator<Item = XmlNodeRef> {
    let root = node.clone();
    let mut previous: Option<XmlNodeRef> = None;
    let mut started = false;
    std::iter::from_fn(move || {
        let current = if started {
            previous.as_ref()?.next_sibling()
        } else {
            started = true;
            root.first_child()
        };
        previous = current.clone();
        current
    })
}
fn attributes(node: &XmlNodeRef) -> Vec<(XmlString, XmlString)> {
    node.with_attributes(|attrs| {
        attrs
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    })
}
fn match_node_name(node: &XmlNodeRef, name: &str, uri: Option<&str>, mode: XfaPacketMatch) -> bool {
    if node.node_type() != XmlNodeType::Element || !ascii(&node.local_tag_name(), name) {
        return false;
    }
    if mode == XfaPacketMatch::NoMatch {
        return true;
    }
    let namespace = node.namespace_uri();
    let uri = uri.unwrap_or("");
    if mode == XfaPacketMatch::PrefixMatch {
        namespace.len() >= uri.len() && ascii(&namespace[..uri.len()], uri)
    } else {
        ascii(&namespace, uri)
    }
}
fn match_packet(node: &XmlNodeRef, packet: XfaPacketType) -> bool {
    let info = get_packet_by_index(packet);
    match_node_name(node, info.name, info.uri, info.match_type)
}
fn attribute_local_name(name: &[u32]) -> (&[u32], &[u32]) {
    name.iter()
        .position(|&ch| ch == 58)
        .map_or((name, &[]), |pos| (&name[pos + 1..], &name[..pos]))
}
fn resolve_attribute(element: &XmlNodeRef, name: &[u32]) -> Option<(Vec<u32>, Vec<u32>)> {
    let (local, prefix) = attribute_local_name(name);
    if ascii(local, "xmlns") || ascii(prefix, "xmlns") || ascii(prefix, "xml") {
        return None;
    }
    Some((
        local.to_vec(),
        resolve_namespace_qualifier(element, prefix)?,
    ))
}
fn find_attribute_with_ns(element: &XmlNodeRef, local: &str, uri: &str) -> Option<Vec<u32>> {
    for (name, value) in attributes(element) {
        let (attr, prefix) = attribute_local_name(&name);
        if !ascii(attr, local) {
            continue;
        }
        if resolve_namespace_qualifier(element, prefix)
            .is_some_and(|namespace| ascii(&namespace, uri))
        {
            return Some(value.to_vec());
        }
    }
    None
}
fn get_datasets_from_xdp(node: &XmlNodeRef) -> Option<XmlNodeRef> {
    if match_packet(node, XfaPacketType::Datasets) {
        return Some(node.clone());
    }
    if !match_packet(node, XfaPacketType::Xdp) {
        return None;
    }
    children(node).find(|child| match_packet(child, XfaPacketType::Datasets))
}
fn all_whitespace(text: &[u32]) -> bool {
    text.iter().all(|ch| matches!(ch, 32 | 9 | 13 | 10))
}
fn convert_xml_to_plain_text(root: &XmlNodeRef, output: &mut Vec<u32>) {
    for child in children(root) {
        match child.node_type() {
            XmlNodeType::Element => {
                output.extend(child.text_data());
                output.push(10);
            }
            XmlNodeType::Text | XmlNodeType::CharData => {
                let text = child.text();
                if !all_whitespace(&text) {
                    *output = text.to_vec();
                }
            }
            XmlNodeType::Instruction => {}
        }
    }
}
pub fn recognize_rich_text(node: Option<&XmlNodeRef>) -> bool {
    node.is_some_and(|node| {
        node.node_type() == XmlNodeType::Element
            && ascii(&node.namespace_uri(), "http://www.w3.org/1999/xhtml")
    })
}
fn plain_text_from_rich_text(root: &XmlNodeRef) -> Vec<u32> {
    // Each source recursive call starts with an empty local string. Only br
    // inserts a line break; p/div's tests of that empty string add nothing.
    // An explicit DFS stack preserves this behavior without growing the call stack.
    let mut output = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(node) = stack.pop() {
        match node.node_type() {
            XmlNodeType::Element => {
                let tag = node.local_tag_name();
                let hash = crate::fxcrt::css::cfx_cssdata::hash_code_lowered_wide(&tag);
                if hash == 0x0001f714 {
                    output.push(10);
                }
            }
            XmlNodeType::Text | XmlNodeType::CharData => output.extend_from_slice(&node.text()),
            XmlNodeType::Instruction => {}
        }
        let mut child = node.last_child();
        while let Some(current) = child {
            child = current.prev_sibling();
            stack.push(current);
        }
    }
    output
}

pub struct CxfaDocumentBuilder {
    node_factory: Rc<CxfaDocument>,
    root_node: Option<CxfaNode>,
    xml_doc: Option<Rc<RefCell<CfxXmlDocument>>>,
    execute_recursion_depth: usize,
}
impl CxfaDocumentBuilder {
    pub fn new(node_factory: Rc<CxfaDocument>) -> Self {
        Self {
            node_factory,
            root_node: None,
            xml_doc: None,
            execute_recursion_depth: 0,
        }
    }
    pub fn build(&mut self, xml: Rc<RefCell<CfxXmlDocument>>) -> Option<XmlNodeRef> {
        let (root, instruction) = {
            let mut document = xml.borrow_mut();
            (document.root(), document.create_instruction(&wide("xml")))
        };
        self.node_factory.set_xml_document(xml.clone());
        self.xml_doc = Some(xml);
        root.insert_child_node(&instruction, 0);
        children(&root).find(|child| child.node_type() == XmlNodeType::Element)
    }
    pub fn build_document(
        &mut self,
        xml: Rc<RefCell<CfxXmlDocument>>,
        packet: XfaPacketType,
    ) -> bool {
        let Some(root) = self.build(xml) else {
            self.root_node = None;
            return false;
        };
        self.root_node = self.parse_as_xdp_packet(&root, packet);
        self.root_node.is_some()
    }
    pub fn get_root_node(&self) -> Option<CxfaNode> {
        self.root_node.clone()
    }
    pub fn construct_xfa_node(&mut self, node: &CxfaNode, xml: &XmlNodeRef) {
        let packet = node.get_packet_type();
        if packet == XfaPacketType::Datasets {
            if node.get_element_type() == XfaElement::DataValue {
                for child in children(xml) {
                    if child.node_type() == XmlNodeType::Instruction {
                        continue;
                    }
                    if child.node_type() == XmlNodeType::Element {
                        let Some(xfa_child) = self
                            .node_factory
                            .create_node(XfaPacketType::Datasets, XfaElement::DataValue)
                        else {
                            return;
                        };
                        xfa_child.set_cdata(XfaAttribute::Name, &child.local_tag_name());
                        let value = plain_text_from_rich_text(&child);
                        if !value.is_empty() {
                            xfa_child.set_cdata(XfaAttribute::Value, &value);
                        }
                        node.insert_child_and_notify(-1, &xfa_child);
                        xfa_child.set_xml_mapping_node(child);
                        xfa_child.set_initialized_flag();
                        break;
                    }
                }
                self.root_node = Some(node.clone());
            } else {
                self.root_node = self.data_loader(node, xml);
            }
        } else if node.is_content_node() {
            self.parse_content_node(node, xml, packet);
            self.root_node = Some(node.clone());
        } else {
            self.root_node = self.normal_loader(node, xml, packet, true);
        }
    }
    fn parse_as_xdp_packet(&mut self, xml: &XmlNodeRef, packet: XfaPacketType) -> Option<CxfaNode> {
        match packet {
            XfaPacketType::Xdp => self.parse_xdp(xml),
            XfaPacketType::Config => self.parse_normal_packet(xml, packet, XfaElement::Config),
            XfaPacketType::Template => self.parse_template(xml),
            XfaPacketType::Form => self.parse_form(xml),
            XfaPacketType::Datasets => self.parse_data(xml),
            XfaPacketType::Xdc => {
                if !match_packet(xml, packet) {
                    return None;
                }
                let node = self.node_factory.create_node(packet, XfaElement::Xdc)?;
                node.set_cdata(XfaAttribute::Name, &wide(get_packet_by_index(packet).name));
                node.set_xml_mapping_node(xml.clone());
                Some(node)
            }
            XfaPacketType::LocaleSet => {
                self.parse_normal_packet(xml, packet, XfaElement::LocaleSet)
            }
            XfaPacketType::ConnectionSet => {
                self.parse_normal_packet(xml, packet, XfaElement::ConnectionSet)
            }
            XfaPacketType::SourceSet => {
                self.parse_normal_packet(xml, packet, XfaElement::SourceSet)
            }
            _ => self.parse_user(xml),
        }
    }
    fn parse_xdp(&mut self, xml: &XmlNodeRef) -> Option<CxfaNode> {
        if !match_packet(xml, XfaPacketType::Xdp) {
            return None;
        }
        let root = self
            .node_factory
            .create_node(XfaPacketType::Xdp, XfaElement::Xfa)?;
        self.root_node = Some(root.clone());
        root.set_cdata(XfaAttribute::Name, &wide("xfa"));
        for (name, value) in attributes(xml) {
            if ascii(&name, "uuid") {
                root.set_cdata(XfaAttribute::Uuid, &value);
            } else if ascii(&name, "timeStamp") {
                root.set_cdata(XfaAttribute::TimeStamp, &value);
            }
        }
        let mut config_xml = None;
        for child in children(xml) {
            if !match_packet(&child, XfaPacketType::Config) {
                continue;
            }
            if root
                .get_first_child_by_name(&wide(get_packet_by_index(XfaPacketType::Config).name))
                .is_some()
            {
                return None;
            }
            config_xml = Some(child.clone());
            if let Some(config) = self.parse_as_xdp_packet(&child, XfaPacketType::Config) {
                root.insert_child_and_notify(-1, &config);
            }
        }
        let mut datasets_xml = None;
        let mut form_xml = None;
        let mut template_xml = None;
        for child in children(xml) {
            if child.node_type() != XmlNodeType::Element || config_xml.as_ref() == Some(&child) {
                continue;
            }
            let mut packet_info = get_packet_by_name(&child.local_tag_name());
            if let Some(info) = packet_info {
                if info.uri.is_some()
                    && !match_node_name(&child, info.name, info.uri, info.match_type)
                {
                    packet_info = None;
                }
            }
            let packet = packet_info.map_or(XfaPacketType::User, |info| info.packet_type);
            match packet {
                XfaPacketType::Xdp => continue,
                XfaPacketType::Datasets => {
                    if datasets_xml.is_some() {
                        return None;
                    }
                    datasets_xml = Some(child);
                }
                XfaPacketType::Form => {
                    if form_xml.is_some() {
                        return None;
                    }
                    form_xml = Some(child);
                }
                XfaPacketType::Template => {
                    if template_xml.is_some() {
                        return None;
                    }
                    if let Some(node) = self.parse_template(&child) {
                        template_xml = Some(child);
                        root.insert_child_and_notify(-1, &node);
                    }
                }
                _ => {
                    if let Some(node) = self.parse_as_xdp_packet(&child, packet) {
                        if let Some(info) = packet_info {
                            if info.support == XfaPacketSupport::SupportOne
                                && root.get_first_child_by_name(&wide(info.name)).is_some()
                            {
                                return None;
                            }
                        }
                        root.insert_child_and_notify(-1, &node);
                    }
                }
            }
        }
        template_xml?;
        if let Some(xml) = datasets_xml {
            if let Some(node) = self.parse_as_xdp_packet(&xml, XfaPacketType::Datasets) {
                root.insert_child_and_notify(-1, &node);
            }
        }
        if let Some(xml) = form_xml {
            if let Some(node) = self.parse_as_xdp_packet(&xml, XfaPacketType::Form) {
                root.insert_child_and_notify(-1, &node);
            }
        }
        root.set_xml_mapping_node(xml.clone());
        Some(root)
    }
    fn parse_normal_packet(
        &mut self,
        xml: &XmlNodeRef,
        packet: XfaPacketType,
        element: XfaElement,
    ) -> Option<CxfaNode> {
        if !match_packet(xml, packet) {
            return None;
        }
        let node = self.node_factory.create_node(packet, element)?;
        node.set_cdata(XfaAttribute::Name, &wide(get_packet_by_index(packet).name));
        self.normal_loader(&node, xml, packet, true)?;
        node.set_xml_mapping_node(xml.clone());
        Some(node)
    }
    fn parse_template(&mut self, xml: &XmlNodeRef) -> Option<CxfaNode> {
        if !match_packet(xml, XfaPacketType::Template) {
            return None;
        }
        let node = self
            .node_factory
            .create_node(XfaPacketType::Template, XfaElement::Template)?;
        node.set_cdata(
            XfaAttribute::Name,
            &wide(get_packet_by_index(XfaPacketType::Template).name),
        );
        let mut namespace = xml.namespace_uri().to_vec();
        if namespace.is_empty() {
            namespace = xml.get_attribute(&wide("xmlns:xfa")).to_vec();
        }
        self.node_factory.recognize_xfa_version_number(&namespace);
        self.normal_loader(&node, xml, XfaPacketType::Template, true)?;
        node.set_xml_mapping_node(xml.clone());
        Some(node)
    }
    fn parse_form(&mut self, xml: &XmlNodeRef) -> Option<CxfaNode> {
        if !match_packet(xml, XfaPacketType::Form) {
            return None;
        }
        let node = self
            .node_factory
            .create_node(XfaPacketType::Form, XfaElement::Form)?;
        node.set_cdata(
            XfaAttribute::Name,
            &wide(get_packet_by_index(XfaPacketType::Form).name),
        );
        let chosen = self
            .root_node
            .as_ref()
            .and_then(|root| root.get_first_child_by_class(XfaElement::Template))
            .and_then(|template| template.get_first_child_by_class(XfaElement::Subform));
        let use_attribute = chosen.is_none_or(|chosen| {
            chosen.get_enum(XfaAttribute::RestoreState) == XfaAttributeValue::Auto
        });
        self.normal_loader(&node, xml, XfaPacketType::Form, use_attribute)?;
        node.set_xml_mapping_node(xml.clone());
        Some(node)
    }
    fn parse_data(&mut self, xml: &XmlNodeRef) -> Option<CxfaNode> {
        if let Some(datasets) = get_datasets_from_xdp(xml) {
            let node = self
                .node_factory
                .create_node(XfaPacketType::Datasets, XfaElement::DataModel)?;
            node.set_cdata(
                XfaAttribute::Name,
                &wide(get_packet_by_index(XfaPacketType::Datasets).name),
            );
            self.data_loader(&node, &datasets)?;
            node.set_xml_mapping_node(datasets);
            return Some(node);
        }
        if xml.node_type() != XmlNodeType::Element {
            return None;
        }
        let info = get_packet_by_index(XfaPacketType::Datasets);
        let data = if match_node_name(xml, "data", info.uri, info.match_type) {
            xml.remove_attribute(&wide("xmlns:xfa"));
            xml.clone()
        } else {
            let data = self
                .xml_doc
                .as_ref()?
                .borrow_mut()
                .create_element(&wide("xfa:data"));
            xml.remove_self_if_parented();
            xml.remove_attribute(&wide("xmlns:xfa"));
            data.append_last_child(xml);
            data
        };
        let node = self
            .node_factory
            .create_node(XfaPacketType::Datasets, XfaElement::DataGroup)?;
        node.set_cdata(XfaAttribute::Name, &data.local_tag_name());
        self.data_loader(&node, &data)?;
        node.set_xml_mapping_node(data);
        Some(node)
    }
    fn parse_user(&self, xml: &XmlNodeRef) -> Option<CxfaNode> {
        if xml.node_type() != XmlNodeType::Element {
            return None;
        }
        let node = self
            .node_factory
            .create_node(XfaPacketType::Xdp, XfaElement::Packet)?;
        node.set_cdata(XfaAttribute::Name, &xml.local_tag_name());
        node.set_xml_mapping_node(xml.clone());
        Some(node)
    }
    fn data_loader(&mut self, node: &CxfaNode, xml: &XmlNodeRef) -> Option<CxfaNode> {
        self.parse_data_group(node, xml, XfaPacketType::Datasets, 0)?;
        Some(node.clone())
    }
    fn normal_loader(
        &mut self,
        node: &CxfaNode,
        xml: &XmlNodeRef,
        packet: XfaPacketType,
        use_attribute: bool,
    ) -> Option<CxfaNode> {
        if self.execute_recursion_depth > MAX_EXECUTE_RECURSION {
            return None;
        }
        self.execute_recursion_depth += 1;
        let result = self.normal_loader_children(node, xml, packet, use_attribute);
        self.execute_recursion_depth -= 1;
        result.map(|()| node.clone())
    }
    fn normal_loader_children(
        &mut self,
        node: &CxfaNode,
        xml: &XmlNodeRef,
        packet: XfaPacketType,
        use_attribute: bool,
    ) -> Option<()> {
        let mut one_of_found = false;
        for child in children(xml) {
            match child.node_type() {
                XmlNodeType::Element => {
                    let tag = child.local_tag_name();
                    let element = get_element_by_name(&tag);
                    if element == XfaElement::Unknown {
                        continue;
                    }
                    if node.has_property_flag(element, 1) || node.has_property_flag(element, 2) {
                        if one_of_found {
                            continue;
                        }
                        one_of_found = true;
                    }
                    let xfa_child = self.node_factory.create_node(packet, element)?;
                    if packet == XfaPacketType::Config {
                        xfa_child.set_attribute_by_enum(XfaAttribute::Name, &tag, false);
                    }
                    let mut need_value = true;
                    for (name, value) in attributes(&child) {
                        let (local, _) = attribute_local_name(&name);
                        if ascii(local, "nil") && ascii(&value, "true") {
                            need_value = false;
                        }
                        let Some(info) = get_attribute_by_name(local) else {
                            continue;
                        };
                        if !use_attribute
                            && !matches!(info.attribute, XfaAttribute::Name | XfaAttribute::Save)
                        {
                            continue;
                        }
                        xfa_child.set_attribute_by_enum(info.attribute, &value, false);
                    }
                    node.insert_child_and_notify(-1, &xfa_child);
                    if matches!(element, XfaElement::Validate | XfaElement::Locale) {
                        if packet == XfaPacketType::Config {
                            self.parse_content_node(&xfa_child, &child, packet);
                        } else {
                            self.normal_loader(&xfa_child, &child, packet, use_attribute);
                        }
                        continue;
                    }
                    match xfa_child.get_object_type() {
                        XfaObjectType::ContentNode
                        | XfaObjectType::TextNode
                        | XfaObjectType::NodeC
                        | XfaObjectType::NodeV => {
                            if need_value {
                                self.parse_content_node(&xfa_child, &child, packet);
                            }
                        }
                        _ => {
                            self.normal_loader(&xfa_child, &child, packet, use_attribute);
                        }
                    }
                }
                XmlNodeType::Instruction => self.parse_instruction(&child),
                _ => {}
            }
        }
        Some(())
    }
    fn parse_content_node(&self, node: &CxfaNode, xml: &XmlNodeRef, packet: XfaPacketType) {
        let mut element = XfaElement::Sharptext;
        if node.get_element_type() == XfaElement::ExData {
            let content = node.get_cdata(XfaAttribute::ContentType);
            if ascii(&content, "text/html") {
                element = XfaElement::SharpxHTML;
            } else if ascii(&content, "text/xml") {
                element = XfaElement::Sharpxml;
            }
        }
        if element == XfaElement::SharpxHTML {
            node.set_xml_mapping_node(xml.clone());
        }
        let mut value = Vec::new();
        for child in children(xml) {
            if child.node_type() == XmlNodeType::Instruction {
                continue;
            }
            if element == XfaElement::SharpxHTML {
                if child.node_type() != XmlNodeType::Element {
                    break;
                }
                if recognize_rich_text(Some(&child)) {
                    value.extend(plain_text_from_rich_text(&child));
                }
            } else if element == XfaElement::Sharpxml {
                if child.node_type() != XmlNodeType::Element {
                    break;
                }
                convert_xml_to_plain_text(&child, &mut value);
            } else {
                if child.node_type() == XmlNodeType::Element {
                    break;
                }
                if child.is_text() {
                    value = child.text().to_vec();
                }
            }
            break;
        }
        if value.is_empty() {
            return;
        }
        if node.is_content_node() {
            if let Some(raw) = self.node_factory.create_node(packet, element) {
                raw.set_cdata(XfaAttribute::Value, &value);
                node.insert_child_and_notify(-1, &raw);
            }
        } else {
            node.set_cdata(XfaAttribute::Value, &value);
        }
    }
    fn parse_data_group(
        &mut self,
        node: &CxfaNode,
        xml: &XmlNodeRef,
        packet: XfaPacketType,
        depth: usize,
    ) -> Option<()> {
        // The normal loader already bounds recursion upstream. Apply the same
        // bound to adversarially nested datasets rather than risking stack exhaustion.
        if depth > MAX_EXECUTE_RECURSION {
            return None;
        }
        for child in children(xml) {
            match child.node_type() {
                XmlNodeType::Element => {
                    let namespace = child.namespace_uri();
                    if reserved_data_namespace(&namespace, false) {
                        continue;
                    }
                    let mut element = XfaElement::DataModel;
                    if let Some(kind) = find_attribute_with_ns(
                        &child,
                        "dataNode",
                        "http://www.xfa.org/schema/xfa-data/1.0/",
                    ) {
                        if ascii(&kind, "dataGroup") {
                            element = XfaElement::DataGroup;
                        } else if ascii(&kind, "dataValue") {
                            element = XfaElement::DataValue;
                        }
                    }
                    if element == XfaElement::DataModel {
                        if find_attribute_with_ns(
                            &child,
                            "contentType",
                            "http://www.xfa.org/schema/xfa-data/1.0/",
                        )
                        .is_some_and(|content| !content.is_empty())
                        {
                            element = XfaElement::DataValue;
                        }
                    }
                    if element == XfaElement::DataModel {
                        if children(&child).any(|item| {
                            item.node_type() == XmlNodeType::Element
                                && !recognize_rich_text(Some(&item))
                        }) {
                            element = XfaElement::DataGroup;
                        }
                    }
                    if element == XfaElement::DataModel {
                        element = XfaElement::DataValue;
                    }
                    let xfa_child = self
                        .node_factory
                        .create_node(XfaPacketType::Datasets, element)?;
                    xfa_child.set_cdata(XfaAttribute::Name, &child.local_tag_name());
                    let mut need_value = true;
                    for (name, value) in attributes(&child) {
                        let Some((local, namespace)) = resolve_attribute(&child, &name) else {
                            continue;
                        };
                        if ascii(&local, "nil") && ascii(&value, "true") {
                            need_value = false;
                            continue;
                        }
                        if reserved_data_namespace(&namespace, true) {
                            continue;
                        }
                        let metadata = self
                            .node_factory
                            .create_node(XfaPacketType::Datasets, XfaElement::DataValue)?;
                        metadata.set_cdata(XfaAttribute::Name, &local);
                        metadata.set_cdata(XfaAttribute::QualifiedName, &name);
                        metadata.set_cdata(XfaAttribute::Value, &value);
                        metadata.set_enum(
                            XfaAttribute::Contains,
                            XfaAttributeValue::MetaData,
                            false,
                        );
                        xfa_child.insert_child_and_notify(-1, &metadata);
                        metadata.set_xml_mapping_node(child.clone());
                        metadata.set_initialized_flag();
                    }
                    if !need_value {
                        child.remove_attribute(&wide("xsi:nil"));
                    }
                    node.insert_child_and_notify(-1, &xfa_child);
                    if element == XfaElement::DataGroup {
                        self.parse_data_group(&xfa_child, &child, packet, depth + 1)?;
                    } else if need_value {
                        self.parse_data_value(
                            &xfa_child,
                            &child,
                            XfaPacketType::Datasets,
                            depth + 1,
                        )?;
                    }
                    xfa_child.set_xml_mapping_node(child);
                    xfa_child.set_initialized_flag();
                }
                XmlNodeType::CharData | XmlNodeType::Text => {
                    let text = child.text();
                    if all_whitespace(&text) {
                        continue;
                    }
                    let xfa_child = self
                        .node_factory
                        .create_node(XfaPacketType::Datasets, XfaElement::DataValue)?;
                    xfa_child.set_cdata(XfaAttribute::Value, &text);
                    node.insert_child_and_notify(-1, &xfa_child);
                    xfa_child.set_xml_mapping_node(child);
                    xfa_child.set_initialized_flag();
                }
                XmlNodeType::Instruction => {}
            }
        }
        Some(())
    }
    fn append_data_text(
        &self,
        node: &CxfaNode,
        xml: &XmlNodeRef,
        packet: XfaPacketType,
        value: &[u32],
    ) -> Option<()> {
        let child = self
            .node_factory
            .create_node(packet, XfaElement::DataValue)?;
        child.set_cdata(XfaAttribute::Name, &[]);
        child.set_cdata(XfaAttribute::Value, value);
        node.insert_child_and_notify(-1, &child);
        child.set_xml_mapping_node(xml.clone());
        child.set_initialized_flag();
        Some(())
    }
    fn parse_data_value(
        &mut self,
        node: &CxfaNode,
        xml: &XmlNodeRef,
        packet: XfaPacketType,
        depth: usize,
    ) -> Option<()> {
        if depth > MAX_EXECUTE_RECURSION {
            return None;
        }
        let mut value = Vec::new();
        let mut current_value = Vec::new();
        let mut compound = false;
        let mut current_xml = None;
        for child in children(xml) {
            if child.node_type() == XmlNodeType::Instruction {
                continue;
            }
            if child.is_text() {
                if current_xml.is_none() {
                    current_xml = Some(child.clone());
                }
                current_value.extend_from_slice(&child.text());
                continue;
            }
            if recognize_rich_text(Some(&child)) {
                if current_xml.is_none() {
                    current_xml = Some(child.clone());
                }
                current_value.extend(plain_text_from_rich_text(&child));
                continue;
            }
            compound = true;
            if let Some(xml) = current_xml.take() {
                if !current_value.is_empty() {
                    self.append_data_text(node, &xml, packet, &current_value)?;
                    value.extend_from_slice(&current_value);
                    current_value.clear();
                }
            }
            let xfa_child = self
                .node_factory
                .create_node(packet, XfaElement::DataValue)?;
            xfa_child.set_cdata(XfaAttribute::Name, &child.local_tag_name());
            self.parse_data_value(&xfa_child, &child, packet, depth + 1)?;
            node.insert_child_and_notify(-1, &xfa_child);
            xfa_child.set_xml_mapping_node(child);
            xfa_child.set_initialized_flag();
            value.extend(xfa_child.get_cdata(XfaAttribute::Value));
        }
        if let Some(xml) = current_xml {
            if !current_value.is_empty() {
                if compound {
                    self.append_data_text(node, &xml, packet, &current_value)?;
                }
                value.extend(current_value);
            }
        }
        node.set_cdata(XfaAttribute::Value, &value);
        Some(())
    }
    fn parse_instruction(&self, instruction: &XmlNodeRef) {
        let data = instruction.target_data();
        if instruction.is_original_xfa_version() {
            if let (Some(version), Some(script)) = (data.get(0), data.get(1)) {
                if self.node_factory.recognize_xfa_version_number(&version) != 0
                    && ascii(&script, "v2.7-scripting:1")
                {
                    self.node_factory.set_is_scripting();
                }
            }
        } else if instruction.is_acrobat() {
            if let (Some(language), Some(scope)) = (data.get(0), data.get(1)) {
                if ascii(&language, "JavaScript") && ascii(&scope, "strictScoping") {
                    self.node_factory.set_is_strict_scoping();
                }
            }
        }
    }
}
fn reserved_data_namespace(namespace: &[u32], include_data: bool) -> bool {
    ascii(namespace, "http://www.xfa.com/schema/xfa-package/")
        || ascii(namespace, "http://www.xfa.org/schema/xfa-package/")
        || ascii(namespace, "http://www.w3.org/2001/XMLSchema-instance")
        || (include_data && ascii(namespace, "http://www.xfa.org/schema/xfa-data/1.0/"))
}

#[cfg(test)]
#[path = "cxfa_document_builder_unittest.rs"]
mod upstream_tests;
