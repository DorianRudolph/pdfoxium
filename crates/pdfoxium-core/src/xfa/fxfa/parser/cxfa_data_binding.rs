// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_document.cpp` CreateDataBinding/FormValueNode helpers, and
//! `cxfa_node.cpp` data picture/normalization methods. Dispatch uses the
//! selected model UI element, without creating host widgets. Multi-selection
//! import joins item values with newlines and omits a synthetic trailing empty
//! selection, correcting the native concatenation/trailing-newline defects.
use super::{
    cxfa_localevalue::{CxfaLocaleValue, ValueType, XfaValuePicture},
    cxfa_node::CxfaNode,
    node_schema::XfaObjectType,
    xfa_utils::{get_locale_value, get_locale_value_type},
};
use crate::{
    fxcrt::xml::cfx_xmlnode::{XmlNodeRef, XmlNodeType},
    xfa::{
        fgas::crt::{locale_iface::DateTimeSubcategory, locale_mgr_iface::LocaleMgrIface},
        fxfa::fxfa_basic::{XfaAttribute, XfaElement, XfaPacketType},
    },
};
use std::rc::Rc;
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataBindingError {
    InvalidFormNode,
    InvalidDataNode,
    DocumentMismatch,
    MissingValueNode,
    MissingUiNode,
    MissingXmlMapping,
    NodeCreationFailed,
    MutationFailed,
}
fn xml_element(node: &CxfaNode) -> Result<XmlNodeRef, DataBindingError> {
    node.get_xml_mapping_node()
        .filter(|node| node.node_type() == XmlNodeType::Element)
        .ok_or(DataBindingError::MissingXmlMapping)
}

pub fn form_value_node_match_none_create_child(form: &CxfaNode) -> Result<(), DataBindingError> {
    if !form.is_alive()
        || form.get_packet_type() != XfaPacketType::Form
        || !matches!(
            form.get_element_type(),
            XfaElement::Field | XfaElement::Draw | XfaElement::Subform | XfaElement::ExclGroup
        )
    {
        return Err(DataBindingError::InvalidFormNode);
    }
    // ExclGroup/Subform have no UI child; their native GetUIChildNode only sets
    // the widget discriminator. There is no host-widget state to materialize.
    if matches!(
        form.get_element_type(),
        XfaElement::Field | XfaElement::Draw
    ) {
        form.get_ui_child_node()
            .ok_or(DataBindingError::MissingUiNode)?;
    }
    Ok(())
}
pub fn form_value_node_create_child(value: &CxfaNode, kind: XfaElement) -> Option<CxfaNode> {
    value.first_child().or_else(|| {
        (kind != XfaElement::Unknown)
            .then(|| value.get_or_create_property(0, kind))
            .flatten()
    })
}
pub fn form_value_node_set_child_content(
    value: Option<&CxfaNode>,
    content: &[u32],
    kind: XfaElement,
) -> bool {
    let Some(value) = value else {
        return false;
    };
    if value.get_packet_type() != XfaPacketType::Form {
        return false;
    }
    let Some(child) = form_value_node_create_child(value, kind) else {
        return false;
    };
    match child.get_object_type() {
        XfaObjectType::ContentNode => {
            let raw = if let Some(raw) = child.first_child() {
                raw
            } else {
                let content_type = child.try_attribute(XfaAttribute::ContentType, false);
                let raw_kind = if child.get_element_type() == XfaElement::ExData
                    && content_type.as_deref() == Some(wide("text/html").as_slice())
                {
                    XfaElement::SharpxHTML
                } else if child.get_element_type() == XfaElement::ExData
                    && content_type.as_deref() == Some(wide("text/xml").as_slice())
                {
                    XfaElement::Sharpxml
                } else {
                    XfaElement::Sharptext
                };
                let Some(raw) = child.create_same_packet_node(raw_kind) else {
                    return false;
                };
                if !child.insert_child_before_and_notify(&raw, None) {
                    return false;
                }
                raw
            };
            raw.set_cdata(XfaAttribute::Value, content);
            true
        }
        XfaObjectType::NodeC | XfaObjectType::TextNode | XfaObjectType::NodeV => {
            child.set_cdata(XfaAttribute::Value, content);
            true
        }
        _ => false,
    }
}
fn set_form_value(
    value: Option<&CxfaNode>,
    content: &[u32],
    kind: XfaElement,
) -> Result<(), DataBindingError> {
    if form_value_node_set_child_content(value, content, kind) {
        Ok(())
    } else {
        Err(DataBindingError::MutationFailed)
    }
}

pub fn create_data_binding(
    form: &CxfaNode,
    data: &CxfaNode,
    data_to_form: bool,
) -> Result<(), DataBindingError> {
    if !form.is_alive() || form.get_packet_type() != XfaPacketType::Form {
        return Err(DataBindingError::InvalidFormNode);
    }
    if !data.is_alive()
        || data.get_packet_type() != XfaPacketType::Datasets
        || !matches!(
            data.get_element_type(),
            XfaElement::DataValue | XfaElement::DataGroup
        )
    {
        return Err(DataBindingError::InvalidDataNode);
    }
    let Some(document) = form.document() else {
        return Err(DataBindingError::InvalidFormNode);
    };
    if !data
        .document()
        .is_some_and(|other| Rc::ptr_eq(&document, &other))
    {
        return Err(DataBindingError::DocumentMismatch);
    }
    // Keep the inverse edge correct when a caller explicitly rebinds a form.
    if let Some(old) = form.get_bind_data() {
        if old != *data {
            old.remove_bind_item(form);
        }
    }
    form.set_binding_node(Some(data));
    data.add_bind_item(form);
    let element = form.get_element_type();
    if !matches!(element, XfaElement::Field | XfaElement::ExclGroup) {
        return Ok(());
    }
    let default_value = form.get_or_create_property(0, XfaElement::Value);
    let ui = if element == XfaElement::ExclGroup {
        XfaElement::ExclGroup
    } else {
        form.get_ui_child_node()
            .ok_or(DataBindingError::MissingUiNode)?
            .get_element_type()
    };
    if !data_to_form {
        return bind_form_to_data(form, data, default_value.as_ref(), ui);
    }
    let xml_value = data.get_content();
    let mut normalized = form.get_normalize_data_value(&xml_value);
    // A structured selection is represented by child data values. Writing a
    // flat group value here would detach those XML children before importing
    // them, so preserve the structure and its mappings instead.
    if !(ui == XfaElement::ChoiceList
        && form.is_choice_list_multi_select()
        && data.get_element_type() == XfaElement::DataGroup)
    {
        data.set_attribute_value(&normalized, &xml_value, false);
    }
    match ui {
        XfaElement::ImageEdit => {
            set_form_value(default_value.as_ref(), &normalized, XfaElement::Image)?;
            if let Some(image) = default_value
                .as_ref()
                .and_then(CxfaNode::value_get_image_if_exists)
            {
                let xml = xml_element(data)?;
                let content_type = xml.get_attribute(&wide("xfa:contentType"));
                if !content_type.is_empty() {
                    data.set_cdata(XfaAttribute::ContentType, &content_type);
                    image.set_cdata(XfaAttribute::ContentType, &content_type);
                }
                let href = xml.get_attribute(&wide("href"));
                if !href.is_empty() {
                    image.set_cdata(XfaAttribute::Href, &href);
                }
            }
        }
        XfaElement::ChoiceList => {
            if form.is_choice_list_multi_select() {
                let mut count = 0;
                let mut child = data.first_child();
                while let Some(item) = child {
                    if count > 0 {
                        normalized.push(u32::from(b'\n'));
                    } else {
                        normalized.clear();
                    }
                    normalized.extend(item.get_content());
                    count += 1;
                    child = item.next_sibling();
                }
                if count > 0 {
                    if let Some(ex_data) = default_value
                        .as_ref()
                        .and_then(CxfaNode::value_get_ex_data_if_exists)
                    {
                        ex_data.set_cdata(
                            XfaAttribute::ContentType,
                            &wide(if count == 1 { "text/plain" } else { "text/xml" }),
                        );
                    }
                }
                set_form_value(default_value.as_ref(), &normalized, XfaElement::ExData)?;
            } else {
                set_form_value(default_value.as_ref(), &normalized, XfaElement::Text)?;
            }
        }
        XfaElement::ExclGroup => {
            if !form.set_selected_member_by_value(&normalized, false, false) {
                return Err(DataBindingError::MutationFailed);
            }
        }
        XfaElement::DateTimeEdit => {
            set_form_value(default_value.as_ref(), &normalized, XfaElement::DateTime)?
        }
        XfaElement::NumericEdit => {
            if form
                .get_picture_content(XfaValuePicture::DataBind)
                .is_empty()
            {
                normalized = form.normalize_num_str(&normalized);
            }
            set_form_value(default_value.as_ref(), &normalized, XfaElement::Float)?;
        }
        _ => set_form_value(default_value.as_ref(), &normalized, XfaElement::Text)?,
    }
    Ok(())
}
fn bind_form_to_data(
    form: &CxfaNode,
    data: &CxfaNode,
    value: Option<&CxfaNode>,
    ui: XfaElement,
) -> Result<(), DataBindingError> {
    match ui {
        XfaElement::ImageEdit => {
            let image = value.and_then(CxfaNode::value_get_image_if_exists);
            let content = image.as_ref().map_or_else(Vec::new, CxfaNode::get_content);
            let content_type = image
                .as_ref()
                .map_or_else(Vec::new, |node| node.get_cdata(XfaAttribute::ContentType));
            let href = image
                .as_ref()
                .map_or_else(Vec::new, |node| node.get_cdata(XfaAttribute::Href));
            let xml = xml_element(data)?;
            data.set_attribute_value(&content, &form.get_format_data_value(&content), false);
            data.set_cdata(XfaAttribute::ContentType, &content_type);
            if !href.is_empty() {
                xml.set_attribute(&wide("href"), &href);
            }
        }
        XfaElement::ChoiceList if form.is_choice_list_multi_select() => {
            let items = form.get_selected_items_value();
            if items.is_empty() {
                xml_element(data)?.set_attribute(&wide("xfa:dataNode"), &wide("dataGroup"));
            } else {
                for text in items {
                    let node = data
                        .create_same_packet_node(XfaElement::DataValue)
                        .ok_or(DataBindingError::NodeCreationFailed)?;
                    node.set_cdata(XfaAttribute::Name, &wide("value"));
                    node.create_xml_mapping_node()
                        .ok_or(DataBindingError::MissingXmlMapping)?;
                    if !data.insert_child_before_and_notify(&node, None) {
                        return Err(DataBindingError::MutationFailed);
                    }
                    node.set_cdata(XfaAttribute::Value, &text);
                }
            }
        }
        XfaElement::ExclGroup => {
            let mut checked = None;
            let mut child = form.first_child();
            while let Some(member) = child {
                if member.get_element_type() == XfaElement::Field {
                    if let Some(value) = member.get_child(0, XfaElement::Value, false) {
                        let content = value.value_get_child_value_content();
                        if !content.is_empty() {
                            if let Some(item) = member
                                .get_child(0, XfaElement::Items, false)
                                .and_then(|items| items.first_child())
                            {
                                let on = item.get_content();
                                if on == content {
                                    data.set_attribute_value(&content, &content, false);
                                    form.set_cdata(XfaAttribute::Value, &on);
                                    checked = Some(member.clone());
                                    break;
                                }
                            }
                        }
                    }
                }
                child = member.next_sibling();
            }
            let Some(checked) = checked else {
                return Ok(());
            };
            let mut child = form.first_child();
            while let Some(member) = child {
                if member != checked && member.get_element_type() == XfaElement::Field {
                    let value = member.get_or_create_property(0, XfaElement::Value);
                    let off = member
                        .get_child(0, XfaElement::Items, false)
                        .and_then(|items| items.first_child())
                        .and_then(|item| item.next_sibling())
                        .map_or_else(Vec::new, |item| item.get_content());
                    set_form_value(value.as_ref(), &off, XfaElement::Text)?;
                }
                child = member.next_sibling();
            }
        }
        XfaElement::NumericEdit => {
            let content = value.map_or_else(Vec::new, CxfaNode::value_get_child_value_content);
            if !content.is_empty() {
                let content = form.normalize_num_str(&content);
                data.set_attribute_value(&content, &form.get_format_data_value(&content), false);
                let value = form.get_or_create_property(0, XfaElement::Value);
                set_form_value(value.as_ref(), &content, XfaElement::Float)?;
            }
        }
        _ => {
            let content = value.map_or_else(Vec::new, CxfaNode::value_get_child_value_content);
            if !content.is_empty() {
                data.set_attribute_value(&content, &form.get_format_data_value(&content), false);
            }
        }
    }
    Ok(())
}

fn split_date_time(value: &[u32]) -> Option<(&[u32], &[u32])> {
    let index = value
        .iter()
        .position(|&ch| ch == u32::from(b'T'))
        .or_else(|| value.iter().position(|&ch| ch == u32::from(b' ')))?;
    let (date, time) = (&value[..index], &value[index + 1..]);
    let has_digit = |part: &[u32]| part.is_empty() || part.iter().any(|ch| (48..=57).contains(ch));
    (has_digit(date) && has_digit(time)).then_some((date, time))
}
impl CxfaNode {
    pub fn get_frac_digits(&self) -> i32 {
        self.get_child(0, XfaElement::Value, false)
            .and_then(|value| value.get_child(0, XfaElement::Decimal, false))
            .and_then(|decimal| decimal.try_integer(XfaAttribute::FracDigits, true))
            .unwrap_or(-1)
    }
    pub fn get_lead_digits(&self) -> i32 {
        self.get_child(0, XfaElement::Value, false)
            .and_then(|value| value.get_child(0, XfaElement::Decimal, false))
            .and_then(|decimal| decimal.try_integer(XfaAttribute::LeadDigits, true))
            .unwrap_or(-1)
    }
    pub fn normalize_num_str(&self, value: &[u32]) -> Vec<u32> {
        if value.is_empty() {
            return Vec::new();
        }
        let start = value
            .iter()
            .position(|&ch| ch != u32::from(b'0'))
            .unwrap_or(value.len());
        let mut value = value[start..].to_vec();
        if !value.is_empty() && value.contains(&u32::from(b'.')) && self.get_frac_digits() != -1 {
            while value.last() == Some(&u32::from(b'0')) {
                value.pop();
            }
            while value.last() == Some(&u32::from(b'.')) {
                value.pop();
            }
        }
        if value.is_empty() || value.first() == Some(&u32::from(b'.')) {
            value.insert(0, u32::from(b'0'));
        }
        value
    }
    pub fn numeric_limit(&self, value: &[u32]) -> Vec<u32> {
        let mut lead = self.get_lead_digits();
        let fraction = self.get_frac_digits();
        if (lead == -1 && fraction == -1) || value.is_empty() {
            return value.to_vec();
        }
        let mut output = Vec::with_capacity(value.len());
        let mut position = 0;
        if value.first() == Some(&u32::from(b'-')) {
            output.push(u32::from(b'-'));
            position = 1;
        }
        let mut leading_count = 0i64;
        let mut fractional_count = -1i64;
        for &unit in &value[position..] {
            if (48..=57).contains(&unit) {
                if lead >= 0 {
                    leading_count += 1;
                    if leading_count > i64::from(lead) {
                        return wide("0");
                    }
                } else if fractional_count >= 0 {
                    fractional_count += 1;
                    // -1 is an unlimited fraction, not a request to truncate
                    // at the decimal point as in the native signed comparison.
                    if fraction >= 0 && fractional_count > i64::from(fraction) {
                        let mut decimal =
                            crate::xfa::fgas::crt::cfgas_decimal::CfgasDecimal::from_wide_string(
                                value,
                            );
                        decimal.set_scale(u8::try_from(fraction).unwrap_or(u8::MAX));
                        return decimal.to_wide_string();
                    }
                }
            } else if unit == u32::from(b'.') {
                fractional_count = 0;
                lead = -1;
            }
            output.push(unit);
        }
        output
    }
    pub fn get_picture_content(&self, picture: XfaValuePicture) -> Vec<u32> {
        if picture == XfaValuePicture::Raw {
            return Vec::new();
        }
        let widget = get_locale_value(self);
        let explicit = match picture {
            XfaValuePicture::Display => self.get_child(0, XfaElement::Format, false),
            XfaValuePicture::Edit => self.get_child(0, XfaElement::Ui, false),
            XfaValuePicture::DataBind => {
                return self
                    .get_child(0, XfaElement::Bind, false)
                    .map_or_else(Vec::new, |bind| bind.bind_get_picture())
            }
            XfaValuePicture::Raw => return Vec::new(),
        };
        if let Some(content) = explicit
            .and_then(|parent| parent.get_child(0, XfaElement::Picture, false))
            .and_then(|picture| picture.try_content())
        {
            return content;
        }
        let Some(locale) = self.get_locale() else {
            return Vec::new();
        };
        let category = if picture == XfaValuePicture::Display {
            DateTimeSubcategory::Medium
        } else {
            DateTimeSubcategory::Short
        };
        match widget.get_type() {
            ValueType::Date => locale.get_date_pattern(category),
            ValueType::Time => locale.get_time_pattern(category),
            ValueType::DateTime => {
                let mut result = locale.get_date_pattern(category);
                result.push(u32::from(b'T'));
                result.extend(locale.get_time_pattern(category));
                result
            }
            _ => Vec::new(),
        }
    }
    pub fn get_normalize_data_value(&self, value: &[u32]) -> Vec<u32> {
        if value.is_empty() {
            return Vec::new();
        }
        let picture = self.get_picture_content(XfaValuePicture::DataBind);
        if picture.is_empty() {
            return value.to_vec();
        }
        let manager = self
            .document()
            .and_then(|doc| doc.get_locale_mgr())
            .map(|manager| manager as Rc<dyn LocaleMgrIface>);
        let locale = self.get_locale();
        let widget = get_locale_value(self);
        let mut matched_picture = Vec::new();
        if widget.validate_value(value, &picture, locale.clone(), Some(&mut matched_picture)) {
            CxfaLocaleValue::with_pattern(
                widget.get_type(),
                value,
                &matched_picture,
                locale,
                manager,
            )
            .get_value()
            .to_vec()
        } else {
            value.to_vec()
        }
    }
    pub fn get_format_data_value(&self, value: &[u32]) -> Vec<u32> {
        if value.is_empty() {
            return Vec::new();
        }
        let picture = self.get_picture_content(XfaValuePicture::DataBind);
        if picture.is_empty() {
            return value.to_vec();
        }
        let Some(locale) = self.get_locale() else {
            return value.to_vec();
        };
        let Some(kind) = self
            .get_child(0, XfaElement::Value, false)
            .and_then(|value| value.first_child())
            .map(|child| get_locale_value_type(child.get_element_type()))
        else {
            return value.to_vec();
        };
        let manager = self
            .document()
            .and_then(|doc| doc.get_locale_mgr())
            .map(|manager| manager as Rc<dyn LocaleMgrIface>);
        let widget = CxfaLocaleValue::with_value(kind, value, manager.clone());
        let mut formatted = value.to_vec();
        if matches!(kind, ValueType::Date | ValueType::Time) {
            if let Some((date, time)) = split_date_time(value) {
                let part = if kind == ValueType::Date { date } else { time };
                let widget = CxfaLocaleValue::with_value(kind, part, manager);
                if widget.format_patterns(
                    &mut formatted,
                    &picture,
                    Some(locale.clone()),
                    XfaValuePicture::DataBind,
                ) {
                    return formatted;
                }
            }
        }
        widget.format_patterns(
            &mut formatted,
            &picture,
            Some(locale),
            XfaValuePicture::DataBind,
        );
        formatted
    }
}

#[cfg(test)]
mod tests {
    use super::super::{cxfa_document::CxfaDocument, cxfa_localemgr::CxfaLocaleMgr};
    use super::*;
    use crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument;
    use std::cell::RefCell;
    fn node(doc: &Rc<CxfaDocument>, packet: XfaPacketType, element: XfaElement) -> CxfaNode {
        doc.create_node(packet, element).unwrap()
    }
    fn add(parent: &CxfaNode, child: &CxfaNode) {
        assert!(parent.insert_child_and_notify(-1, child));
    }
    fn fixture() -> (Rc<CxfaDocument>, CxfaNode) {
        let doc = CxfaDocument::new();
        doc.set_xml_document(Rc::new(RefCell::new(CfxXmlDocument::new())));
        doc.set_locale_mgr(Some(Rc::new(CxfaLocaleMgr::new(
            &wide("en_US"),
            Rc::new(|| 0),
        ))));
        let root = node(&doc, XfaPacketType::Xdp, XfaElement::Xfa);
        let form = node(&doc, XfaPacketType::Form, XfaElement::Form);
        form.set_cdata(XfaAttribute::Name, &wide("form"));
        let top = node(&doc, XfaPacketType::Form, XfaElement::Subform);
        top.set_cdata(XfaAttribute::Locale, &wide("en_US"));
        add(&root, &form);
        add(&form, &top);
        assert!(doc.set_root(Some(root)));
        (doc, top)
    }
    fn field(doc: &Rc<CxfaDocument>, parent: &CxfaNode, kind: XfaElement) -> CxfaNode {
        let field = node(doc, XfaPacketType::Form, XfaElement::Field);
        let ui = node(doc, XfaPacketType::Form, XfaElement::Ui);
        let child = node(doc, XfaPacketType::Form, kind);
        add(parent, &field);
        add(&field, &ui);
        add(&ui, &child);
        field.get_ui_child_node().unwrap();
        field
    }
    fn data(doc: &Rc<CxfaDocument>, kind: XfaElement, name: &str, value: &str) -> CxfaNode {
        let data = node(doc, XfaPacketType::Datasets, kind);
        data.set_cdata(XfaAttribute::Name, &wide(name));
        data.create_xml_mapping_node().unwrap();
        if kind == XfaElement::DataValue {
            data.set_attribute_value(&wide(value), &wide(value), false);
        }
        data
    }
    fn picture(doc: &Rc<CxfaDocument>, field: &CxfaNode, value: &str) {
        let bind = field.get_or_create_property(0, XfaElement::Bind).unwrap();
        let picture = node(doc, XfaPacketType::Form, XfaElement::Picture);
        add(&bind, &picture);
        assert!(picture.set_content(&wide(value), &wide(value), false));
    }
    #[test]
    fn text_binding_both_directions_rebinding_and_validation() {
        let (doc, top) = fixture();
        let field = field(&doc, &top, XfaElement::TextEdit);
        let incoming = data(&doc, XfaElement::DataValue, "incoming", "hello");
        assert_eq!(create_data_binding(&field, &incoming, true), Ok(()));
        assert_eq!(field.get_content(), wide("hello"));
        assert_eq!(field.get_bind_data(), Some(incoming.clone()));
        assert!(incoming.get_bind_items_copy().contains(&field));
        assert!(field.set_content(&wide("outgoing"), &wide("outgoing"), false));
        let outgoing = data(&doc, XfaElement::DataValue, "outgoing", "");
        assert_eq!(create_data_binding(&field, &outgoing, false), Ok(()));
        assert_eq!(outgoing.get_content(), wide("outgoing"));
        assert_eq!(
            outgoing.get_xml_mapping_node().unwrap().text_data(),
            wide("outgoing")
        );
        assert!(!incoming.get_bind_items_copy().contains(&field));
        assert!(outgoing.get_bind_items_copy().contains(&field));
        let wrong = node(&doc, XfaPacketType::Template, XfaElement::Field);
        assert_eq!(
            create_data_binding(&wrong, &incoming, true),
            Err(DataBindingError::InvalidFormNode)
        );
        let foreign = CxfaDocument::new();
        let foreign_data = node(&foreign, XfaPacketType::Datasets, XfaElement::DataValue);
        assert_eq!(
            create_data_binding(&field, &foreign_data, true),
            Err(DataBindingError::DocumentMismatch)
        );
        assert_eq!(
            create_data_binding(&field, &wrong, true),
            Err(DataBindingError::InvalidDataNode)
        );
        let subform = node(&doc, XfaPacketType::Form, XfaElement::Subform);
        let scope = data(&doc, XfaElement::DataGroup, "scope", "");
        assert_eq!(create_data_binding(&subform, &scope, true), Ok(()));
        assert_eq!(subform.get_bind_data(), Some(scope));
        assert!(subform.first_child().is_none());
    }
    #[test]
    fn numeric_pictures_keep_canonical_raw_value_and_formatted_xml() {
        let (doc, top) = fixture();
        let field = field(&doc, &top, XfaElement::NumericEdit);
        picture(&doc, &field, "num{zz9.99}");
        let incoming = data(&doc, XfaElement::DataValue, "amount", "12.30");
        assert_eq!(create_data_binding(&field, &incoming, true), Ok(()));
        assert_eq!(field.get_content(), wide("12.30"));
        assert_eq!(incoming.get_content(), wide("12.30"));
        assert!(field.set_content(&wide("7.5"), &wide("7.5"), false));
        let outgoing = data(&doc, XfaElement::DataValue, "outgoing", "");
        assert_eq!(create_data_binding(&field, &outgoing, false), Ok(()));
        assert_eq!(outgoing.get_content(), wide("7.5"));
        assert_eq!(
            outgoing.get_xml_mapping_node().unwrap().text_data(),
            wide("7.50")
        );
        assert_eq!(
            field.get_normalize_data_value(&wide("invalid")),
            wide("invalid")
        );
        assert_eq!(
            field.get_format_data_value(&wide("invalid")),
            wide("invalid")
        );
    }
    #[test]
    fn decimal_normalization_uses_decimal_digit_limits_and_preserves_unlimited_fraction() {
        let (doc, top) = fixture();
        let field = field(&doc, &top, XfaElement::NumericEdit);
        assert_eq!(field.get_frac_digits(), -1);
        assert_eq!(
            field.normalize_num_str(&wide("00012.3400")),
            wide("12.3400")
        );
        let value = field.get_child(0, XfaElement::Value, false).unwrap();
        let previous = value.first_child().unwrap();
        assert!(value.remove_child_and_notify(&previous, false));
        let decimal = node(&doc, XfaPacketType::Form, XfaElement::Decimal);
        add(&value, &decimal);
        decimal.set_integer(XfaAttribute::FracDigits, 2, false);
        decimal.set_integer(XfaAttribute::LeadDigits, 4, false);
        assert_eq!(field.get_frac_digits(), 2);
        assert_eq!(field.get_lead_digits(), 4);
        for (input, expected) in [
            ("", ""),
            ("000", "0"),
            ("000.000", "0"),
            (".1200", "0.12"),
            ("00012.3400", "12.34"),
            ("-00012.3400", "-00012.34"),
        ] {
            assert_eq!(field.normalize_num_str(&wide(input)), wide(expected));
        }
        let incoming = data(&doc, XfaElement::DataValue, "number", "0001.200");
        assert_eq!(create_data_binding(&field, &incoming, true), Ok(()));
        assert_eq!(field.get_content(), wide("1.2"));
    }
    #[test]
    fn datetime_picture_import_and_export_use_existing_locale_values() {
        let (doc, top) = fixture();
        let field = field(&doc, &top, XfaElement::DateTimeEdit);
        picture(&doc, &field, "date{YYYY-MM-DD} time{HH:MM:SS}");
        let incoming = data(
            &doc,
            XfaElement::DataValue,
            "datetime",
            "2024-03-12 13:14:15",
        );
        assert_eq!(create_data_binding(&field, &incoming, true), Ok(()));
        assert_eq!(field.get_content(), wide("2024-03-12T13:14:15"));
        assert_eq!(
            field.get_format_data_value(&wide("2024-03-12T13:14:15")),
            wide("2024-03-12 13:14:15")
        );
        let value = field.get_child(0, XfaElement::Value, false).unwrap();
        let old = value.first_child().unwrap();
        value.remove_child_and_notify(&old, false);
        let date = node(&doc, XfaPacketType::Form, XfaElement::Date);
        add(&value, &date);
        picture(&doc, &field, "date{YYYY/MM/DD}");
        // Bind's first picture remains the one selected by source lookup.
        assert!(!field
            .get_picture_content(XfaValuePicture::DataBind)
            .is_empty());
    }
    #[test]
    fn image_binding_copies_content_type_and_href_in_both_directions() {
        let (doc, top) = fixture();
        let image_field = field(&doc, &top, XfaElement::ImageEdit);
        let incoming = data(&doc, XfaElement::DataValue, "image", "base64");
        let xml = incoming.get_xml_mapping_node().unwrap();
        xml.set_attribute(&wide("xfa:contentType"), &wide("image/png"));
        xml.set_attribute(&wide("href"), &wide("image.png"));
        assert_eq!(create_data_binding(&image_field, &incoming, true), Ok(()));
        let image = image_field
            .get_child(0, XfaElement::Value, false)
            .unwrap()
            .value_get_image_if_exists()
            .unwrap();
        assert_eq!(image.get_content(), wide("base64"));
        assert_eq!(
            image.get_cdata(XfaAttribute::ContentType),
            wide("image/png")
        );
        assert_eq!(image.get_cdata(XfaAttribute::Href), wide("image.png"));
        let outgoing = data(&doc, XfaElement::DataValue, "out", "");
        assert_eq!(create_data_binding(&image_field, &outgoing, false), Ok(()));
        assert_eq!(outgoing.get_content(), wide("base64"));
        let xml = outgoing.get_xml_mapping_node().unwrap();
        assert_eq!(
            xml.get_attribute(&wide("xfa:contentType")).as_ref(),
            wide("image/png")
        );
        assert_eq!(xml.get_attribute(&wide("href")).as_ref(), wide("image.png"));
        let absent = node(&doc, XfaPacketType::Datasets, XfaElement::DataValue);
        assert_eq!(
            create_data_binding(&image_field, &absent, false),
            Err(DataBindingError::MissingXmlMapping)
        );
    }
    #[test]
    fn multiselect_roundtrip_preserves_values_empty_selection_and_xml_children() {
        let (doc, top) = fixture();
        let field = field(&doc, &top, XfaElement::ChoiceList);
        let choice = field.get_ui_child_node().unwrap();
        choice.set_enum(
            XfaAttribute::Open,
            super::super::super::fxfa_basic::XfaAttributeValue::MultiSelect,
            false,
        );
        // Rebuild the value as ExData, matching a choiceList configured before its
        // first UI initialization rather than the fixture's initial single choice.
        let value = field.get_child(0, XfaElement::Value, false).unwrap();
        let old = value.first_child().unwrap();
        value.remove_child_and_notify(&old, false);
        let ex_data = node(&doc, XfaPacketType::Form, XfaElement::ExData);
        add(&value, &ex_data);
        let incoming = data(&doc, XfaElement::DataGroup, "selection", "");
        let first = data(&doc, XfaElement::DataValue, "value", "alpha");
        let second = data(&doc, XfaElement::DataValue, "value", "beta");
        add(&incoming, &first);
        add(&incoming, &second);
        let xml = incoming.get_xml_mapping_node().unwrap();
        let first_xml = first.get_xml_mapping_node().unwrap();
        let second_xml = second.get_xml_mapping_node().unwrap();
        assert_eq!(create_data_binding(&field, &incoming, true), Ok(()));
        assert_eq!(
            field.get_selected_items_value(),
            [wide("alpha"), wide("beta")]
        );
        assert_eq!(xml.first_child(), Some(first_xml.clone()));
        assert_eq!(first_xml.next_sibling(), Some(second_xml));
        let outgoing = data(&doc, XfaElement::DataGroup, "out", "");
        assert_eq!(create_data_binding(&field, &outgoing, false), Ok(()));
        assert_eq!(outgoing.count_children(XfaElement::Unknown, false), 2);
        assert_eq!(outgoing.first_child().unwrap().get_content(), wide("alpha"));
        assert_eq!(outgoing.last_child().unwrap().get_content(), wide("beta"));
        assert!(field.set_content(&[], &[], false));
        let empty = data(&doc, XfaElement::DataGroup, "empty", "");
        assert_eq!(create_data_binding(&field, &empty, false), Ok(()));
        assert!(empty.first_child().is_none());
        assert_eq!(
            empty
                .get_xml_mapping_node()
                .unwrap()
                .get_attribute(&wide("xfa:dataNode"))
                .as_ref(),
            wide("dataGroup")
        );
        let single = data(&doc, XfaElement::DataGroup, "single", "");
        let only = data(&doc, XfaElement::DataValue, "value", "one");
        add(&single, &only);
        assert_eq!(create_data_binding(&field, &single, true), Ok(()));
        assert_eq!(field.get_selected_items_value(), [wide("one")]);
    }
    #[test]
    fn exclusive_group_binding_selects_members_and_clears_other_checks() {
        let (doc, top) = fixture();
        let group = node(&doc, XfaPacketType::Form, XfaElement::ExclGroup);
        add(&top, &group);
        let member = |on: &str| {
            let member = field(&doc, &group, XfaElement::CheckButton);
            let items = node(&doc, XfaPacketType::Form, XfaElement::Items);
            add(&member, &items);
            for text in [on, "off"] {
                let item = node(&doc, XfaPacketType::Form, XfaElement::Text);
                add(&items, &item);
                assert!(item.set_content(&wide(text), &wide(text), false));
            }
            member
        };
        let first = member("first");
        let second = member("second");
        let incoming = data(&doc, XfaElement::DataValue, "choice", "second");
        assert_eq!(create_data_binding(&group, &incoming, true), Ok(()));
        assert_eq!(first.get_content(), wide("off"));
        assert_eq!(second.get_content(), wide("second"));
        assert_eq!(group.get_content(), wide("second"));
        assert!(first.set_content(&wide("first"), &wide("first"), false));
        let outgoing = data(&doc, XfaElement::DataValue, "out", "");
        assert_eq!(create_data_binding(&group, &outgoing, false), Ok(()));
        assert_eq!(outgoing.get_content(), wide("first"));
        assert_eq!(group.get_content(), wide("first"));
        assert_eq!(second.get_content(), wide("off"));
        assert!(group.set_selected_member_by_value(&wide("missing"), false, false));
        assert!(group.get_content().is_empty());
        assert_eq!(first.get_content(), wide("off"));
        assert_eq!(second.get_content(), wide("off"));
    }
    #[test]
    fn numeric_limit_uses_decimal_rounding_and_enforces_leading_digits() {
        let (doc, top) = fixture();
        let field = field(&doc, &top, XfaElement::NumericEdit);
        assert_eq!(field.numeric_limit(&wide("00012.3456")), wide("00012.3456"));
        let value = field.get_child(0, XfaElement::Value, false).unwrap();
        let previous = value.first_child().unwrap();
        assert!(value.remove_child_and_notify(&previous, false));
        let decimal = node(&doc, XfaPacketType::Form, XfaElement::Decimal);
        add(&value, &decimal);
        decimal.set_integer(XfaAttribute::LeadDigits, 3, false);
        decimal.set_integer(XfaAttribute::FracDigits, 2, false);
        for (input, expected) in [
            ("123.456", "123.46"),
            ("-123.456", "-123.46"),
            ("1234.5", "0"),
            ("-1234.5", "0"),
            ("12.3", "12.3"),
            ("", ""),
            (".125", "0.13"),
        ] {
            assert_eq!(field.numeric_limit(&wide(input)), wide(expected));
        }
        decimal.set_integer(XfaAttribute::FracDigits, 0, false);
        assert_eq!(field.numeric_limit(&wide("12.7")), wide("13"));
        decimal.set_integer(XfaAttribute::FracDigits, -1, false);
        assert_eq!(field.numeric_limit(&wide("123.4567")), wide("123.4567"));
        assert_eq!(field.numeric_limit(&wide("1234.5")), wide("0"));
    }
    #[test]
    fn form_value_helpers_preserve_existing_child_and_select_exdata_raw_kind() {
        let (doc, _) = fixture();
        assert!(!form_value_node_set_child_content(
            None,
            &wide("ignored"),
            XfaElement::Text
        ));
        for (content_type, raw_kind) in [
            ("", XfaElement::Sharptext),
            ("text/html", XfaElement::SharpxHTML),
            ("text/xml", XfaElement::Sharpxml),
        ] {
            let value = node(&doc, XfaPacketType::Form, XfaElement::Value);
            assert!(form_value_node_create_child(&value, XfaElement::Unknown).is_none());
            let ex_data = node(&doc, XfaPacketType::Form, XfaElement::ExData);
            ex_data.set_cdata(XfaAttribute::ContentType, &wide(content_type));
            add(&value, &ex_data);
            assert!(form_value_node_set_child_content(
                Some(&value),
                &wide("payload"),
                XfaElement::Float
            ));
            assert_eq!(value.first_child(), Some(ex_data.clone()));
            assert_eq!(ex_data.first_child().unwrap().get_element_type(), raw_kind);
            assert_eq!(ex_data.get_content(), wide("payload"));
        }
    }
}
