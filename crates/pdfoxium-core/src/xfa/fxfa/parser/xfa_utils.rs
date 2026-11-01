// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Portable helpers and data/form serialization from `xfa/fxfa/parser/xfa_utils.{h,cpp}`.
use super::{
    cxfa_document::{XFA_VERSION_DEFAULT, XFA_VERSION_UNKNOWN},
    cxfa_localevalue::{CxfaLocaleValue, ValueType},
    cxfa_node::CxfaNode,
    node_schema::XfaObjectType,
    xfa_basic_data::attribute_to_name,
};
use crate::fxcrt::xml::cfx_xmlnode::{XmlNodeRef, XmlNodeType};
use crate::xfa::fxfa::{
    fxfa::XfaEventError,
    fxfa_basic::{XfaElement, XfaPacketType},
};
use crate::{
    fxcrt::{
        cfx_memorystream::CfxMemoryStream,
        fx_stream::WriteStream,
        fx_string::{utf8_decode, utf8_encode},
    },
    xfa::{
        fgas::crt::locale_mgr_iface::LocaleMgrIface,
        fxfa::fxfa_basic::{XfaAttribute, XfaHashCode},
    },
};
use std::rc::Rc;

// The native declaration lives in xfa_utils.h; its implementation belongs to
// cxfa_document_builder.cpp and stays with that source port.
pub use super::cxfa_document_builder::recognize_rich_text;

pub fn get_locale_value_type(element: XfaElement) -> ValueType {
    match element {
        XfaElement::Decimal => ValueType::Decimal,
        XfaElement::Float => ValueType::Float,
        XfaElement::Date => ValueType::Date,
        XfaElement::Time => ValueType::Time,
        XfaElement::DateTime => ValueType::DateTime,
        XfaElement::Boolean => ValueType::Boolean,
        XfaElement::Integer => ValueType::Integer,
        XfaElement::Text => ValueType::Text,
        _ => ValueType::Null,
    }
}

pub fn resolve_namespace_qualifier(node: &XmlNodeRef, qualifier: &[u32]) -> Option<Vec<u32>> {
    let fake_root = node.root();
    let mut attribute: Vec<u32> = "xmlns".chars().map(u32::from).collect();
    if !qualifier.is_empty() {
        attribute.push(u32::from(b':'));
        attribute.extend_from_slice(qualifier);
    }
    let mut current = Some(node.clone());
    while let Some(parent) = current {
        if parent == fake_root {
            break;
        }
        if parent.node_type() == XmlNodeType::Element && parent.has_attribute(&attribute) {
            return Some(parent.get_attribute(&attribute).to_vec());
        }
        current = parent.parent();
    }
    qualifier.is_empty().then(Vec::new)
}

pub fn map_rotation(rotation: i32) -> i32 {
    let rotation = rotation % 360;
    if rotation < 0 {
        rotation + 360
    } else {
        rotation
    }
}

pub fn event_error_accumulate(accumulated: &mut XfaEventError, new: XfaEventError) {
    if *accumulated == XfaEventError::kNotExist || new == XfaEventError::kError {
        *accumulated = new;
    }
}

fn wide_ascii(value: &str) -> Vec<u32> {
    value.bytes().map(u32::from).collect()
}
fn wide_eq_ascii(value: &[u32], ascii: &str) -> bool {
    value.len() == ascii.len()
        && value
            .iter()
            .zip(ascii.bytes())
            .all(|(&w, b)| w == u32::from(b))
}
fn append_ascii(output: &mut Vec<u32>, value: &str) {
    output.extend(value.bytes().map(u32::from));
}
fn is_xml_valid_char(ch: u32) -> bool {
    matches!(ch,0x09|0x0a|0x0d|0x20..=0xd7ff|0xe000..=0xfffd|0x10000..=0x10ffff)
}
fn export_encode_attribute(value: &[u32]) -> Vec<u32> {
    let mut output = Vec::with_capacity(value.len());
    for &ch in value {
        match ch {
            38 => append_ascii(&mut output, "&amp;"),
            60 => append_ascii(&mut output, "&lt;"),
            62 => append_ascii(&mut output, "&gt;"),
            39 => append_ascii(&mut output, "&apos;"),
            34 => append_ascii(&mut output, "&quot;"),
            _ => output.push(ch),
        }
    }
    output
}
fn export_encode_content(value: &[u32]) -> Vec<u32> {
    let mut output = Vec::with_capacity(value.len());
    for (index, &ch) in value.iter().enumerate() {
        // XML 1.0 permits supplementary scalars. The native wchar32 helper
        // accidentally drops them by checking only the BMP ranges.
        if !is_xml_valid_char(ch) {
            continue;
        }
        match ch {
            38 => append_ascii(&mut output, "&amp;"),
            60 => append_ascii(&mut output, "&lt;"),
            62 => append_ascii(&mut output, "&gt;"),
            39 => append_ascii(&mut output, "&apos;"),
            34 => append_ascii(&mut output, "&quot;"),
            32 if index == 0 || value[index - 1] == 32 => append_ascii(&mut output, "&#x20;"),
            _ => output.push(ch),
        }
    }
    output
}
fn attribute_save_in_data_model(node: &CxfaNode, attribute: XfaAttribute) -> bool {
    if node.get_element_type() != XfaElement::Image || attribute != XfaAttribute::Href {
        return false;
    }
    let Some(value) = node
        .parent()
        .filter(|parent| parent.get_element_type() == XfaElement::Value)
    else {
        return false;
    };
    value
        .parent()
        .is_some_and(|field| field.get_bind_data().is_some())
}
fn content_node_needs_export(node: &CxfaNode) -> bool {
    if node.try_content().is_none() {
        return false;
    }
    let Some(parent) = node
        .parent()
        .filter(|parent| parent.get_element_type() == XfaElement::Value)
    else {
        return true;
    };
    let Some(container) = parent.parent().filter(CxfaNode::is_container_node) else {
        return true;
    };
    if container.get_bind_data().is_none() {
        return false;
    }
    !container
        .get_ui_child_node()
        .is_some_and(|ui| ui.get_element_type() == XfaElement::PasswordEdit)
}
fn save_attribute(
    node: &CxfaNode,
    attribute: XfaAttribute,
    name: &str,
    prototype: bool,
) -> Vec<u32> {
    if !prototype && !node.has_local_attribute(attribute) {
        return Vec::new();
    }
    let Some(value) = node.try_attribute(attribute, false) else {
        return Vec::new();
    };
    let mut output = wide_ascii(" ");
    append_ascii(&mut output, name);
    append_ascii(&mut output, "=\"");
    output.extend(export_encode_attribute(&value));
    output.push(34);
    output
}
const MAX_EXPORT_DEPTH: usize = 1024;
fn regenerate_form_changed(node: &CxfaNode, mut save_xml: bool, depth: usize) -> Option<Vec<u32>> {
    if depth > MAX_EXPORT_DEPTH || !node.is_alive() {
        return None;
    }
    let mut attributes = Vec::new();
    for i in 0.. {
        let attribute = node.get_attribute(i);
        if attribute == XfaAttribute::Unknown {
            break;
        }
        if attribute == XfaAttribute::Name
            || (!save_xml && attribute_save_in_data_model(node, attribute))
        {
            continue;
        }
        attributes.extend(save_attribute(
            node,
            attribute,
            attribute_to_name(attribute),
            save_xml,
        ));
    }
    let mut children = Vec::new();
    match node.get_object_type() {
        // Validate and Locale use ContentNode for Config text, but their
        // Template/Form/LocaleSet instances contain schema properties. Match
        // the builder's distinction instead of silently dropping Message,
        // Picture, Script and locale children during regeneration.
        XfaObjectType::ContentNode
            if !matches!(
                node.get_element_type(),
                XfaElement::Validate | XfaElement::Locale
            ) || node.get_packet_type() == XfaPacketType::Config =>
        {
            if save_xml || content_node_needs_export(node) {
                let mut raw = node.first_child();
                while let Some(current) = raw.as_ref() {
                    if matches!(
                        current.get_element_type(),
                        XfaElement::SharpxHTML | XfaElement::Sharptext | XfaElement::Sharpxml
                    ) {
                        break;
                    }
                    raw = current.next_sibling();
                }
                if let Some(raw) = raw {
                    let content_type = node.try_attribute(XfaAttribute::ContentType, false);
                    if raw.get_element_type() == XfaElement::SharpxHTML
                        && content_type
                            .as_deref()
                            .is_some_and(|value| wide_eq_ascii(value, "text/html"))
                    {
                        if let Some(rich) = node
                            .get_xml_mapping_node()
                            .and_then(|xml| xml.first_child())
                        {
                            let mut memory = CfxMemoryStream::new();
                            rich.save(&mut memory);
                            children.extend(utf8_decode(memory.get_span()));
                        }
                    } else if raw.get_element_type() == XfaElement::Sharpxml
                        && content_type
                            .as_deref()
                            .is_some_and(|value| wide_eq_ascii(value, "text/xml"))
                    {
                        if let Some(value) = raw
                            .try_attribute(XfaAttribute::Value, false)
                            .filter(|value| !value.is_empty())
                        {
                            let mut body = node
                                .parent()
                                .and_then(|value| value.parent())
                                .map_or_else(Vec::new, |field| field.get_cdata(XfaAttribute::Name));
                            if body.is_empty() {
                                body = wide_ascii("ListBox1");
                            }
                            if !body.iter().enumerate().all(|(index, &ch)| {
                                crate::fxcrt::xml::cfx_xmlparser::CfxXmlParser::is_xml_name_char(
                                    ch,
                                    index == 0,
                                )
                            }) {
                                return None;
                            }
                            children.push(60);
                            children.extend_from_slice(&body);
                            append_ascii(&mut children, " xmlns=\"\">\n");
                            for text in value.split(|&ch| ch == 10) {
                                append_ascii(&mut children, "<value>");
                                children.extend(export_encode_content(text));
                                append_ascii(&mut children, "</value>\n");
                            }
                            append_ascii(&mut children, "</");
                            children.extend_from_slice(&body);
                            append_ascii(&mut children, ">\n");
                        }
                    } else {
                        children.extend(export_encode_content(&raw.get_cdata(XfaAttribute::Value)));
                    }
                }
            }
        }
        XfaObjectType::TextNode | XfaObjectType::NodeC | XfaObjectType::NodeV => {
            children.extend(export_encode_content(&node.get_cdata(XfaAttribute::Value)));
        }
        _ => {
            if node.get_element_type() == XfaElement::Items
                && node.get_template_node_if_exists().is_none_or(|template| {
                    template.count_children(XfaElement::Unknown, false)
                        != node.count_children(XfaElement::Unknown, false)
                })
            {
                save_xml = true;
            }
            let mut child = node.first_child();
            while let Some(current) = child {
                children.extend(regenerate_form_changed(&current, save_xml, depth + 1)?);
                child = current.next_sibling();
            }
            if !save_xml && !children.is_empty() && node.get_element_type() == XfaElement::Items {
                children.clear();
                save_xml = true;
                let mut child = node.first_child();
                while let Some(current) = child {
                    children.extend(regenerate_form_changed(&current, save_xml, depth + 1)?);
                    child = current.next_sibling();
                }
            }
        }
    }
    let mut output = Vec::new();
    if !children.is_empty()
        || !attributes.is_empty()
        || node.has_local_attribute(XfaAttribute::Name)
    {
        let element = node.get_class_name();
        output.push(60);
        append_ascii(&mut output, element);
        output.extend(save_attribute(node, XfaAttribute::Name, "name", true));
        output.extend(attributes);
        if children.is_empty() {
            append_ascii(&mut output, "/>\n");
        } else {
            // Formatting whitespace in a scalar value becomes part of that
            // value after reload. Keep source formatting only for child nodes.
            if matches!(
                node.get_object_type(),
                XfaObjectType::ContentNode
                    | XfaObjectType::TextNode
                    | XfaObjectType::NodeC
                    | XfaObjectType::NodeV
            ) {
                output.push(62);
            } else {
                append_ascii(&mut output, ">\n");
            }
            output.extend(children);
            append_ascii(&mut output, "</");
            append_ascii(&mut output, element);
            append_ascii(&mut output, ">\n");
        }
    }
    Some(output)
}
fn regenerate_form_container(
    node: &CxfaNode,
    stream: &mut dyn WriteStream,
    save_xml: bool,
    depth: usize,
) -> bool {
    if depth > MAX_EXPORT_DEPTH || !node.is_alive() {
        return false;
    }
    if matches!(
        node.get_element_type(),
        XfaElement::Field | XfaElement::Draw
    ) || !node.is_container_node()
    {
        return regenerate_form_changed(node, save_xml, depth)
            .is_some_and(|value| value.is_empty() || stream.write_string(&utf8_encode(&value)));
    }
    let element = node.get_class_name();
    if !stream.write_string(b"<") || !stream.write_string(element.as_bytes()) {
        return false;
    }
    let mut attributes = save_attribute(node, XfaAttribute::Name, "name", true);
    for i in 0.. {
        let attribute = node.get_attribute(i);
        if attribute == XfaAttribute::Unknown {
            break;
        }
        if attribute != XfaAttribute::Name {
            attributes.extend(save_attribute(
                node,
                attribute,
                attribute_to_name(attribute),
                false,
            ));
        }
    }
    if !attributes.is_empty() && !stream.write_string(&utf8_encode(&attributes)) {
        return false;
    }
    let mut child = node.first_child();
    if child.is_none() {
        return stream.write_string(b" />\n");
    }
    if !stream.write_string(b">\n") {
        return false;
    }
    while let Some(current) = child {
        if !regenerate_form_container(&current, stream, save_xml, depth + 1) {
            return false;
        }
        child = current.next_sibling();
    }
    stream.write_string(b"</")
        && stream.write_string(element.as_bytes())
        && stream.write_string(b">\n")
}
fn recognize_xfa_version_number(template: &CxfaNode) -> Option<Vec<u32>> {
    let namespace = template.try_namespace()?;
    let document = template.document()?;
    let mut version = document.recognize_xfa_version_number(&namespace);
    if version == XFA_VERSION_UNKNOWN {
        version = XFA_VERSION_DEFAULT;
    }
    Some(wide_ascii(&format!("{}.{}", version / 100, version % 100)))
}

pub fn get_locale_value(node: &CxfaNode) -> CxfaLocaleValue {
    let Some(value) = node
        .get_child(0, XfaElement::Value, false)
        .and_then(|value| value.first_child())
    else {
        return CxfaLocaleValue::default();
    };
    let manager = node
        .document()
        .and_then(|document| document.get_locale_mgr())
        .map(|manager| manager as Rc<dyn LocaleMgrIface>);
    CxfaLocaleValue::with_value(
        get_locale_value_type(value.get_element_type()),
        &node.get_content(),
        manager,
    )
}
pub fn field_is_multi_list_box(node: Option<&CxfaNode>) -> bool {
    node.is_some_and(CxfaNode::is_field_multi_list_box)
}

/// Mark empty data groups while preserving XML-backed value nodes. A postorder
/// stack implements the source recursion without depending on call-stack depth.
pub fn deal_with_data_group_node(node: Option<&CxfaNode>) -> bool {
    let Some(node) = node else {
        return true;
    };
    let mut stack = vec![(node.clone(), false)];
    while let Some((node, visited)) = stack.pop() {
        if !node.is_alive() {
            return false;
        }
        if node.get_element_type() == XfaElement::DataValue {
            continue;
        }
        if !visited {
            stack.push((node.clone(), true));
            let mut child = node.first_child();
            while let Some(current) = child {
                child = current.next_sibling();
                stack.push((current, false));
            }
            continue;
        }
        if node.get_element_type() != XfaElement::DataGroup {
            continue;
        }
        let Some(xml) = node
            .get_xml_mapping_node()
            .filter(|xml| xml.node_type() == XmlNodeType::Element)
        else {
            return false;
        };
        if node.first_child().is_some() {
            xml.remove_attribute(&wide_ascii("xfa:dataNode"));
        } else {
            xml.set_attribute(&wide_ascii("xfa:dataNode"), &wide_ascii("dataGroup"));
        }
    }
    true
}
pub fn regenerate_form_file(node: &CxfaNode, stream: &mut dyn WriteStream, save_xml: bool) -> bool {
    let _store = node.retain_store();
    if !node.is_alive() {
        return false;
    }
    if !node.is_model_node() {
        return regenerate_form_container(node, stream, save_xml, 0);
    }
    if !stream.write_string(b"<form xmlns=\"http://www.xfa.org/schema/xfa-form/") {
        return false;
    }
    let version = node
        .document()
        .and_then(|document| document.get_xfa_node(XfaHashCode::Template as u32))
        .and_then(|template| recognize_xfa_version_number(&template))
        .unwrap_or_else(|| wide_ascii("2.8"));
    if !stream.write_string(&utf8_encode(&version)) || !stream.write_string(b"/\">\n") {
        return false;
    }
    let mut child = node.first_child();
    while let Some(current) = child {
        if !regenerate_form_container(&current, stream, false, 0) {
            return false;
        }
        child = current.next_sibling();
    }
    stream.write_string(b"</form>\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    #[test]
    fn xfa_map_rotation() {
        for (input, expected) in [
            (-1000000, 80),
            (-361, 359),
            (-360, 0),
            (-359, 1),
            (-91, 269),
            (-90, 270),
            (-89, 271),
            (-1, 359),
            (0, 0),
            (1, 1),
            (89, 89),
            (90, 90),
            (91, 91),
            (359, 359),
            (360, 0),
            (361, 1),
            (100000, 280),
        ] {
            assert_eq!(map_rotation(input), expected);
        }
    }
    #[test]
    fn namespace_ancestors_shadowing_default_and_document_root() {
        let mut document = CfxXmlDocument::new();
        let root = document.root();
        root.set_attribute(&wide("xmlns:fake"), &wide("ignored"));
        let parent = document.create_element(&wide("parent"));
        let child = document.create_element(&wide("child"));
        root.append_last_child(&parent);
        parent.append_last_child(&child);
        parent.set_attribute(&wide("xmlns:p"), &wide("outer"));
        parent.set_attribute(&wide("xmlns"), &wide("default"));
        assert_eq!(
            resolve_namespace_qualifier(&child, &wide("p")),
            Some(wide("outer"))
        );
        assert_eq!(
            resolve_namespace_qualifier(&child, &[]),
            Some(wide("default"))
        );
        child.set_attribute(&wide("xmlns:p"), &wide("inner"));
        child.set_attribute(&wide("xmlns"), &[]);
        assert_eq!(
            resolve_namespace_qualifier(&child, &wide("p")),
            Some(wide("inner"))
        );
        assert_eq!(resolve_namespace_qualifier(&child, &[]), Some(Vec::new()));
        assert_eq!(resolve_namespace_qualifier(&child, &wide("missing")), None);
        assert_eq!(resolve_namespace_qualifier(&child, &wide("fake")), None);
        assert_eq!(resolve_namespace_qualifier(&root, &[]), Some(Vec::new()));
        assert_eq!(resolve_namespace_qualifier(&root, &wide("fake")), None);
    }
    #[test]
    fn locale_types_and_event_error_precedence() {
        for (element, expected) in [
            (XfaElement::Decimal, ValueType::Decimal),
            (XfaElement::Float, ValueType::Float),
            (XfaElement::Date, ValueType::Date),
            (XfaElement::Time, ValueType::Time),
            (XfaElement::DateTime, ValueType::DateTime),
            (XfaElement::Boolean, ValueType::Boolean),
            (XfaElement::Integer, ValueType::Integer),
            (XfaElement::Text, ValueType::Text),
            (XfaElement::Unknown, ValueType::Null),
        ] {
            assert_eq!(get_locale_value_type(element), expected);
        }
        let errors = [
            XfaEventError::kError,
            XfaEventError::kNotExist,
            XfaEventError::kSuccess,
            XfaEventError::kDisabled,
        ];
        for previous in errors {
            for new in errors {
                let mut result = previous;
                event_error_accumulate(&mut result, new);
                let expected = match (previous, new) {
                    (_, XfaEventError::kError) => XfaEventError::kError,
                    (XfaEventError::kNotExist, value) => value,
                    (value, _) => value,
                };
                assert_eq!(result, expected);
            }
        }
    }
}
