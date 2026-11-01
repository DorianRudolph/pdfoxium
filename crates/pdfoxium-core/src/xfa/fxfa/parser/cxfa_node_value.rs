// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! CJX_Object content dispatch and CXFA_Node value/UI model preparation.
//! Binding propagation uses the real model edges; no FWL widget is fabricated.
//! Script dependency tracking belongs to the executing runtime.
use super::super::cxfa_localevalue::XfaValuePicture;
use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum XfaFfWidgetType {
    #[default]
    None = 0,
    Barcode,
    Button,
    CheckButton,
    ChoiceList,
    DateTimeEdit,
    ImageEdit,
    NumericEdit,
    PasswordEdit,
    Signature,
    TextEdit,
    Arc,
    Line,
    Rectangle,
    Text,
    Image,
    Subform,
    ExclGroup,
}
enum Preparation {
    Ui,
    Content,
}
struct PreparationGuard {
    node: CxfaNode,
    kind: Preparation,
}
impl Drop for PreparationGuard {
    fn drop(&mut self) {
        if self.node.is_alive() {
            self.node.0.with_value_mut(|data| match self.kind {
                Preparation::Ui => data.preparing_ui = false,
                Preparation::Content => data.preparing_content = false,
            });
        }
    }
}

impl CxfaNode {
    pub fn is_null(&self) -> bool {
        self.0.with_value(|data| data.is_null)
    }
    pub fn set_is_null(&self, value: bool) {
        self.0.with_value_mut(|data| data.is_null = value);
    }
    pub fn pre_null(&self) -> bool {
        self.0.with_value(|data| data.pre_null)
    }
    pub fn set_pre_null(&self, value: bool) {
        self.0.with_value_mut(|data| data.pre_null = value);
    }
    pub fn is_widget_ready(&self) -> bool {
        self.0.with_value(|data| data.widget_ready)
    }
    pub fn set_widget_ready(&self) {
        self.0.with_value_mut(|data| data.widget_ready = true);
    }
    pub fn get_form_value_if_exists(&self) -> Option<Self> {
        self.get_property(0, XfaElement::Value).0
    }
    pub fn get_default_value_if_exists(&self) -> Option<Self> {
        self.get_template_node_if_exists()?
            .get_property(0, XfaElement::Value)
            .0
    }
    pub fn get_raw_value(&self) -> Vec<u32> {
        self.get_content()
    }

    pub fn sync_value(&self, value: &[u32], notify: bool) -> bool {
        self.sync_value_with_script(value, notify, false)
    }
    pub(crate) fn sync_value_with_script(
        &self,
        value: &[u32],
        notify: bool,
        script_modify: bool,
    ) -> bool {
        let _document = self.document();
        let Some(container) = self.get_container_node() else {
            return self.set_content_with_script(value, value, notify, true, script_modify);
        };
        let formatted = container.get_format_data_value(value);
        self.set_content_with_script(value, &formatted, notify, true, script_modify)
    }

    /// Model preparation from CreateChildUIAndValueNodesIfNeeded. Recompute the
    /// small metadata dispatch after XML edits instead of retaining a stale UI
    /// cache. The actual platform/widget-ready flag is set only by its owner.
    fn prepare_ui_and_value(&self) -> Option<(XfaFfWidgetType, Self)> {
        let element = self.get_element_type();
        if !matches!(element, XfaElement::Field | XfaElement::Draw) {
            return None;
        }
        if self.0.with_value_mut(|data| {
            if data.preparing_ui {
                true
            } else {
                data.preparing_ui = true;
                false
            }
        }) {
            return None;
        }
        let _guard = PreparationGuard {
            node: self.clone(),
            kind: Preparation::Ui,
        };
        let _document = self.document();
        let ui = self.get_or_create_property(0, XfaElement::Ui)?;
        let mut child = ui.first_child();
        let mut ui_child = None;
        while let Some(node) = child {
            child = node.next_sibling();
            if ui.ui_is_a_one_of_child(&node) {
                ui_child = Some(node);
                break;
            }
        }
        let value = self.get_or_create_property(0, XfaElement::Value)?;
        let value_child = value.first_child();
        let (expected, draw_type) = match value_child.as_ref().map(Self::get_element_type) {
            Some(XfaElement::Boolean) => (XfaElement::CheckButton, XfaFfWidgetType::None),
            Some(XfaElement::Integer | XfaElement::Decimal | XfaElement::Float) => {
                (XfaElement::NumericEdit, XfaFfWidgetType::None)
            }
            Some(XfaElement::ExData | XfaElement::Text) => {
                (XfaElement::TextEdit, XfaFfWidgetType::Text)
            }
            Some(XfaElement::Date | XfaElement::Time | XfaElement::DateTime) => {
                (XfaElement::DateTimeEdit, XfaFfWidgetType::None)
            }
            Some(XfaElement::Image) => (XfaElement::ImageEdit, XfaFfWidgetType::Image),
            Some(XfaElement::Arc) => (XfaElement::DefaultUi, XfaFfWidgetType::Arc),
            Some(XfaElement::Line) => (XfaElement::DefaultUi, XfaFfWidgetType::Line),
            Some(XfaElement::Rectangle) => (XfaElement::DefaultUi, XfaFfWidgetType::Rectangle),
            _ => (XfaElement::Unknown, XfaFfWidgetType::None),
        };
        if ui_child.is_none() {
            ui_child = ui.get_or_create_property(
                0,
                if expected == XfaElement::Unknown {
                    XfaElement::TextEdit
                } else {
                    expected
                },
            );
        }
        let ui_child = ui_child?;
        if value.first_child().is_none() {
            let mut value_type = ui_child.get_value_node_type();
            if ui_child.get_element_type() == XfaElement::CheckButton {
                if let Some(item) = self
                    .get_child(0, XfaElement::Items, false)
                    .and_then(|items| items.get_child(0, XfaElement::Unknown, false))
                {
                    value_type = item.get_element_type();
                }
            }
            value.get_or_create_property(0, value_type)?;
        }
        let widget_type = if element == XfaElement::Draw {
            match ui_child.get_element_type() {
                XfaElement::TextEdit => XfaFfWidgetType::Text,
                XfaElement::ImageEdit => XfaFfWidgetType::Image,
                _ if draw_type != XfaFfWidgetType::None => draw_type,
                _ => XfaFfWidgetType::Text,
            }
        } else {
            ui_default_widget_type(ui_child.get_element_type())
        };
        Some((widget_type, ui_child))
    }
    pub fn get_ui_child_node(&self) -> Option<Self> {
        if !matches!(
            self.get_element_type(),
            XfaElement::Field | XfaElement::Draw
        ) {
            return None;
        }
        self.prepare_ui_and_value().map(|(_, child)| child)
    }
    pub fn get_ff_widget_type(&self) -> XfaFfWidgetType {
        match self.get_element_type() {
            XfaElement::Subform => XfaFfWidgetType::Subform,
            XfaElement::ExclGroup => XfaFfWidgetType::ExclGroup,
            _ => self
                .prepare_ui_and_value()
                .map_or(XfaFfWidgetType::None, |(kind, _)| kind),
        }
    }
    pub fn is_choice_list_multi_select(&self) -> bool {
        self.get_ui_child_node().is_some_and(|child| {
            child.get_enum(XfaAttribute::Open) == XfaAttributeValue::MultiSelect
        })
    }

    pub fn get_container_node(&self) -> Option<Self> {
        if self.get_packet_type() != XfaPacketType::Form
            || self.get_element_type() == XfaElement::ExclGroup
        {
            return None;
        }
        let parent = self.parent();
        if parent
            .as_ref()
            .is_some_and(|parent| parent.get_element_type() == XfaElement::ExclGroup)
        {
            return None;
        }
        if self.get_element_type() == XfaElement::Field {
            if self.is_choice_list_multi_select() {
                return None;
            }
            if !self
                .get_picture_content(XfaValuePicture::DataBind)
                .is_empty()
            {
                return Some(self.clone());
            }
            let data = self.get_bind_data()?;
            for field in data.get_bind_items_copy() {
                if !field.has_removed_children()
                    && field.is_widget_ready()
                    && !field
                        .get_picture_content(XfaValuePicture::DataBind)
                        .is_empty()
                {
                    return Some(field);
                }
            }
            return None;
        }
        let value = parent
            .as_ref()
            .filter(|node| node.get_element_type() == XfaElement::Value)
            .cloned()
            .or_else(|| {
                parent
                    .and_then(|parent| parent.parent())
                    .filter(|node| node.get_element_type() == XfaElement::Value)
            })?;
        value.parent()?.get_container_node()
    }

    fn content_raw_node(&self, reading: bool) -> Option<Self> {
        if let Some(child) = self.first_child() {
            return Some(child);
        }
        if self.0.with_value_mut(|data| {
            if data.preparing_content {
                true
            } else {
                data.preparing_content = true;
                false
            }
        }) {
            return None;
        }
        let _guard = PreparationGuard {
            node: self.clone(),
            kind: Preparation::Content,
        };
        let content_type = if self.get_element_type() == XfaElement::ExData {
            self.get_cdata(XfaAttribute::ContentType)
        } else {
            Vec::new()
        };
        let element = if reading && content_type == wide("text/html") {
            XfaElement::SharpxHTML
        } else if content_type == wide("text/xml") {
            XfaElement::Sharpxml
        } else {
            XfaElement::Sharptext
        };
        let child = self.create_same_packet_node(element)?;
        // A notification can have supplied a raw child while preparing ours.
        if let Some(existing) = self.first_child() {
            return Some(existing);
        }
        if self.insert_child_and_notify(-1, &child) {
            Some(child)
        } else {
            None
        }
    }

    fn content_leaf(&self) -> Option<Self> {
        let mut node = self.clone();
        loop {
            match node.get_object_type() {
                XfaObjectType::ContainerNode
                    if node.get_element_type() != XfaElement::ExclGroup =>
                {
                    node = node.get_form_value_if_exists()?.first_child()?
                }
                XfaObjectType::ContentNode => node = node.first_child()?,
                XfaObjectType::NodeC | XfaObjectType::NodeV | XfaObjectType::TextNode => {
                    return Some(node)
                }
                _ if matches!(
                    node.get_element_type(),
                    XfaElement::DataValue | XfaElement::ExclGroup
                ) =>
                {
                    return Some(node)
                }
                _ => return None,
            }
        }
    }
    fn content_matches(&self, value: &[u32]) -> bool {
        self.is_alive()&&self.content_leaf().is_some_and(|leaf|leaf.0.with_value(|data|matches!(data.values.get(&XfaAttribute::Value),Some(StoredValue::Text(current)) if current.as_ref()==value)))
    }

    /// The compatibility entry point writes local content. Call SyncValue or
    /// the explicit sync-data entry point to propagate through binding edges.
    pub fn set_content(&self, value: &[u32], xml_value: &[u32], notify: bool) -> bool {
        self.set_content_with_sync(value, xml_value, notify, false)
    }
    pub fn set_content_with_sync(
        &self,
        value: &[u32],
        xml_value: &[u32],
        notify: bool,
        sync_data: bool,
    ) -> bool {
        self.set_content_with_script(value, xml_value, notify, sync_data, false)
    }
    pub(crate) fn set_content_with_script(
        &self,
        value: &[u32],
        xml_value: &[u32],
        notify: bool,
        sync_data: bool,
        script_modify: bool,
    ) -> bool {
        if !self.is_alive() {
            return false;
        }
        let _document = self.document();
        let _store = self.retain_store();
        match self.get_object_type() {
            XfaObjectType::ContainerNode if self.get_element_type() != XfaElement::ExclGroup => {
                // Source callers prepare UI/value state before entering CJX;
                // the model-only entry point makes that prerequisite explicit.
                if matches!(
                    self.get_element_type(),
                    XfaElement::Field | XfaElement::Draw
                ) && self
                    .get_form_value_if_exists()
                    .and_then(|value| value.first_child())
                    .is_none()
                {
                    if self.get_ui_child_node().is_none() {
                        return false;
                    }
                }
                let Some(property) = self.get_or_create_property(0, XfaElement::Value) else {
                    return false;
                };
                let Some(child) = property.first_child() else {
                    return false;
                };
                if self.is_field_multi_list_box() {
                    child.set_cdata(XfaAttribute::ContentType, &wide("text/xml"));
                    if !child.set_content_with_script(value, value, notify, false, script_modify) {
                        return false;
                    }
                    if sync_data {
                        if let Some(data) = self.get_bind_data() {
                            return self.sync_multi_select_data(
                                &data,
                                value,
                                notify,
                                script_modify,
                            );
                        }
                    }
                    return true;
                }
                if !child.set_content_with_script(value, value, notify, false, script_modify) {
                    return false;
                }
                if sync_data {
                    if let Some(data) = self.get_bind_data() {
                        if !data.set_content_with_script(
                            value,
                            xml_value,
                            notify,
                            false,
                            script_modify,
                        ) || !self.content_matches(value)
                        {
                            return false;
                        }
                        return self.sync_bound_peers(&data, value, notify, true, script_modify);
                    }
                }
                true
            }
            XfaObjectType::ContentNode => {
                if self.get_element_type() == XfaElement::ExData
                    && self.get_cdata(XfaAttribute::ContentType) == wide("text/html")
                {
                    self.set_cdata(XfaAttribute::ContentType, &[]);
                }
                let Some(child) = self.content_raw_node(false) else {
                    return false;
                };
                child.set_content_with_script(value, xml_value, notify, sync_data, script_modify)
            }
            XfaObjectType::NodeC | XfaObjectType::NodeV | XfaObjectType::TextNode => {
                let bind = if sync_data
                    && self.get_object_type() == XfaObjectType::NodeV
                    && self.get_packet_type() == XfaPacketType::Form
                {
                    self.parent()
                        .and_then(|parent| parent.parent())
                        .filter(|parent| parent.get_element_type() == XfaElement::Value)
                        .and_then(|value| value.parent())
                        .filter(Self::is_container_node)
                        .and_then(|container| container.get_bind_data())
                } else {
                    None
                };
                if let Some(data) = bind.as_ref() {
                    if !data.set_content_with_script(value, xml_value, notify, false, script_modify)
                    {
                        return false;
                    }
                }
                self.set_attribute_value_with_script(value, xml_value, notify, script_modify);
                if !self.content_matches(value) {
                    return false;
                }
                if let Some(data) = bind {
                    return self.sync_bound_peers(&data, value, notify, false, script_modify);
                }
                true
            }
            _ if matches!(
                self.get_element_type(),
                XfaElement::DataValue | XfaElement::ExclGroup
            ) =>
            {
                self.set_attribute_value_with_script(value, xml_value, notify, script_modify);
                if !self.content_matches(value) {
                    return false;
                }
                if sync_data {
                    let data = if self.get_element_type() == XfaElement::DataValue {
                        Some(self.clone())
                    } else {
                        self.get_bind_data()
                    };
                    if let Some(data) = data {
                        if data != *self
                            && !data.set_content_with_script(
                                value,
                                xml_value,
                                notify,
                                false,
                                script_modify,
                            )
                        {
                            return false;
                        }
                        return self.sync_bound_peers(&data, value, notify, false, script_modify);
                    }
                }
                true
            }
            _ => false,
        }
    }
    fn sync_bound_peers(
        &self,
        data: &Self,
        value: &[u32],
        notify: bool,
        skip_self: bool,
        script_modify: bool,
    ) -> bool {
        for peer in data.get_bind_items_copy() {
            if !data.is_alive() || !self.content_matches(value) || !data.content_matches(value) {
                return false;
            }
            if peer.is_alive()
                && (!skip_self || peer != *self)
                && peer.get_bind_data().as_ref() == Some(data)
            {
                // Ordinary bound-container propagation is script-modified in
                // native SetContent even when the initiating change is not.
                if !peer.set_content_with_script(
                    value,
                    value,
                    notify,
                    false,
                    script_modify || skip_self,
                ) {
                    return false;
                }
            }
        }
        true
    }
    fn sync_multi_select_data(
        &self,
        data: &Self,
        value: &[u32],
        notify: bool,
        script_modify: bool,
    ) -> bool {
        let parts: Vec<&[u32]> = value.split(|&unit| unit == u32::from(b'\n')).collect();
        let mut values = data.get_node_list_for_type(XfaElement::DataValue);
        let mut tries = 0;
        while values.len() != parts.len() {
            tries += 1;
            if tries > 4 || !self.content_matches(value) {
                return false;
            }
            if values.len() < parts.len() {
                for _ in 0..parts.len() - values.len() {
                    let Some(node) = data.create_same_packet_node(XfaElement::DataValue) else {
                        return false;
                    };
                    node.set_cdata(XfaAttribute::Name, &wide("value"));
                    if data
                        .document()
                        .and_then(|document| document.xml_document())
                        .is_some()
                        && node.create_xml_mapping_node().is_none()
                    {
                        return false;
                    }
                    if !data.insert_child_and_notify(-1, &node) {
                        return false;
                    }
                    if !self.content_matches(value) {
                        return false;
                    }
                }
            } else {
                for node in values.iter().take(values.len() - parts.len()) {
                    if !data.remove_child_and_notify(node, true) {
                        return false;
                    }
                    if !self.content_matches(value) {
                        return false;
                    }
                }
            }
            values = data.get_node_list_for_type(XfaElement::DataValue);
        }
        for (node, part) in values.iter().zip(parts) {
            node.set_attribute_value(part, part, false);
        }
        for peer in data.get_bind_items_copy() {
            if !self.content_matches(value) {
                return false;
            }
            if peer.is_alive()
                && peer != *self
                && peer.get_bind_data().as_ref() == Some(data)
                && !peer.set_content_with_script(value, value, notify, false, script_modify)
            {
                return false;
            }
        }
        true
    }

    pub fn try_content(&self) -> Option<Vec<u32>> {
        let _document = self.document();
        let mut node = self.clone();
        loop {
            match node.get_object_type() {
                XfaObjectType::ContainerNode
                    if node.get_element_type() != XfaElement::ExclGroup =>
                {
                    let child = node.get_form_value_if_exists()?.first_child()?;
                    if node.is_field_multi_list_box() {
                        child.set_cdata(XfaAttribute::ContentType, &wide("text/xml"));
                    }
                    node = child;
                }
                XfaObjectType::ContentNode => node = node.content_raw_node(true)?,
                XfaObjectType::NodeC | XfaObjectType::NodeV | XfaObjectType::TextNode => {
                    return node.try_cdata(XfaAttribute::Value, false)
                }
                _ if matches!(
                    node.get_element_type(),
                    XfaElement::DataValue | XfaElement::ExclGroup
                ) =>
                {
                    return node.try_cdata(XfaAttribute::Value, false)
                }
                _ => return None,
            }
        }
    }
    pub fn get_content(&self) -> Vec<u32> {
        self.try_content().unwrap_or_default()
    }

    /// Remove an explicit typed override, exposing its template/default again.
    pub fn clear_attribute(&self, attribute: XfaAttribute, notify: bool) -> bool {
        if !self.is_alive() {
            return false;
        }
        let _document = self.document();
        let removed = self
            .0
            .with_value_mut(|data| data.values.remove(&attribute).is_some());
        if !removed {
            return false;
        }
        if attribute == XfaAttribute::Name {
            self.update_name_hash();
        }
        self.notify_attribute(attribute, notify);
        if !self.is_alive()
            || self.has_local_attribute(attribute)
            || !self.is_need_saving_xml_node()
        {
            return true;
        }
        if attribute == XfaAttribute::Value {
            self.set_to_xml(&self.get_cdata(attribute));
        } else if !matches!(
            attribute,
            XfaAttribute::QualifiedName | XfaAttribute::BindingNode
        ) && !(attribute == XfaAttribute::Name
            && matches!(
                self.get_element_type(),
                XfaElement::DataValue | XfaElement::DataGroup
            ))
        {
            if let Some(xml) = self
                .get_xml_mapping_node()
                .filter(|xml| xml.node_type() == XmlNodeType::Element)
            {
                let mut name = if attribute == XfaAttribute::ContentType {
                    wide("xfa:")
                } else {
                    Vec::new()
                };
                name.extend(wide(attribute_to_name(attribute)));
                xml.remove_attribute(&name);
            }
        }
        true
    }
}

fn ui_default_widget_type(element: XfaElement) -> XfaFfWidgetType {
    match element {
        XfaElement::Barcode => XfaFfWidgetType::Barcode,
        XfaElement::Button => XfaFfWidgetType::Button,
        XfaElement::CheckButton => XfaFfWidgetType::CheckButton,
        XfaElement::ChoiceList => XfaFfWidgetType::ChoiceList,
        XfaElement::DateTimeEdit => XfaFfWidgetType::DateTimeEdit,
        XfaElement::ImageEdit => XfaFfWidgetType::ImageEdit,
        XfaElement::NumericEdit => XfaFfWidgetType::NumericEdit,
        XfaElement::PasswordEdit => XfaFfWidgetType::PasswordEdit,
        XfaElement::Signature => XfaFfWidgetType::Signature,
        XfaElement::TextEdit | XfaElement::DefaultUi => XfaFfWidgetType::TextEdit,
        _ => XfaFfWidgetType::None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::cxfa_document::XfaDocumentNotify;
    use super::*;
    use crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument;
    use std::cell::{Cell, RefCell};

    fn document() -> Rc<CxfaDocument> {
        let document = CxfaDocument::new();
        document.set_xml_document(Rc::new(RefCell::new(CfxXmlDocument::new())));
        document
    }
    fn node(
        document: &Rc<CxfaDocument>,
        packet: XfaPacketType,
        element: XfaElement,
        name: &str,
    ) -> CxfaNode {
        let node = document.create_node(packet, element).unwrap();
        node.set_cdata(XfaAttribute::Name, &wide(name));
        node
    }
    fn append(parent: &CxfaNode, child: &CxfaNode) {
        assert!(parent.insert_child_and_notify(-1, child));
    }
    fn field(document: &Rc<CxfaDocument>, name: &str) -> CxfaNode {
        node(document, XfaPacketType::Form, XfaElement::Field, name)
    }
    fn data(document: &Rc<CxfaDocument>, name: &str, kind: XfaElement) -> CxfaNode {
        let node = node(document, XfaPacketType::Datasets, kind, name);
        node.create_xml_mapping_node().unwrap();
        node
    }
    fn bind(form: &CxfaNode, data: &CxfaNode) {
        form.set_binding_node(Some(data));
        data.add_bind_item(form);
    }
    fn choices(document: &Rc<CxfaDocument>, name: &str) -> CxfaNode {
        let field = field(document, name);
        let ui = field.get_or_create_property(0, XfaElement::Ui).unwrap();
        let choice = ui
            .get_or_create_property(0, XfaElement::ChoiceList)
            .unwrap();
        choice.set_enum(XfaAttribute::Open, XfaAttributeValue::MultiSelect, false);
        field
    }

    #[test]
    fn scalar_content_sync_updates_shared_forms_data_and_real_xml_without_feedback() {
        let doc = document();
        let first = field(&doc, "first");
        let second = field(&doc, "second");
        let data = data(&doc, "shared", XfaElement::DataValue);
        bind(&first, &data);
        bind(&second, &data);
        assert!(first.set_content_with_sync(&wide("raw"), &wide("serialized"), false, true));
        assert_eq!(first.get_raw_value(), wide("raw"));
        assert_eq!(data.get_raw_value(), wide("raw"));
        assert_eq!(second.get_raw_value(), wide("raw"));
        assert_eq!(
            data.get_xml_mapping_node().unwrap().text_data(),
            wide("serialized")
        );
        assert!(data.sync_value(&wide("from data"), false));
        for form in [&first, &second] {
            assert_eq!(form.get_raw_value(), wide("from data"));
        }
        assert!(first.set_content(&wide("local"), &wide("local"), false));
        assert_eq!(data.get_raw_value(), wide("from data"));
        assert_eq!(second.get_raw_value(), wide("from data"));
        let raw = first
            .get_form_value_if_exists()
            .unwrap()
            .first_child()
            .unwrap()
            .first_child()
            .unwrap();
        assert!(raw.is_node_v());
        assert!(raw.sync_value(&wide("from leaf"), false));
        assert_eq!(first.get_raw_value(), wide("from leaf"));
        assert_eq!(data.get_raw_value(), wide("from leaf"));
        assert_eq!(second.get_raw_value(), wide("from leaf"));
    }

    #[test]
    fn ui_preparation_uses_one_of_types_existing_values_and_checkbutton_items() {
        let doc = document();
        for (ui_type, value_type, widget) in [
            (
                XfaElement::TextEdit,
                XfaElement::Text,
                XfaFfWidgetType::TextEdit,
            ),
            (
                XfaElement::NumericEdit,
                XfaElement::Float,
                XfaFfWidgetType::NumericEdit,
            ),
            (
                XfaElement::DateTimeEdit,
                XfaElement::DateTime,
                XfaFfWidgetType::DateTimeEdit,
            ),
            (
                XfaElement::ImageEdit,
                XfaElement::Image,
                XfaFfWidgetType::ImageEdit,
            ),
            (
                XfaElement::PasswordEdit,
                XfaElement::Text,
                XfaFfWidgetType::PasswordEdit,
            ),
        ] {
            let field = field(&doc, "typed");
            let ui = field.get_or_create_property(0, XfaElement::Ui).unwrap();
            let extras = ui.get_or_create_property(0, XfaElement::Extras).unwrap();
            let control = ui.get_or_create_property(0, ui_type).unwrap();
            assert_eq!(field.get_ui_child_node(), Some(control));
            assert_eq!(ui.first_child(), Some(extras));
            assert_eq!(
                field
                    .get_form_value_if_exists()
                    .unwrap()
                    .first_child()
                    .unwrap()
                    .get_element_type(),
                value_type
            );
            assert_eq!(field.get_ff_widget_type(), widget);
            assert!(!field.is_widget_ready());
        }
        let checkbox = field(&doc, "checkbox");
        let ui = checkbox.get_or_create_property(0, XfaElement::Ui).unwrap();
        ui.get_or_create_property(0, XfaElement::CheckButton)
            .unwrap();
        let items = checkbox
            .get_or_create_property(0, XfaElement::Items)
            .unwrap();
        let item = node(&doc, XfaPacketType::Form, XfaElement::Boolean, "");
        append(&items, &item);
        checkbox.get_ui_child_node().unwrap();
        assert_eq!(
            checkbox
                .get_form_value_if_exists()
                .unwrap()
                .first_child()
                .unwrap()
                .get_element_type(),
            XfaElement::Boolean
        );
        let numeric = field(&doc, "number");
        let value = numeric
            .get_or_create_property(0, XfaElement::Value)
            .unwrap();
        value
            .get_or_create_property(0, XfaElement::Integer)
            .unwrap();
        assert_eq!(
            numeric.get_ui_child_node().unwrap().get_element_type(),
            XfaElement::NumericEdit
        );
        assert_eq!(numeric.get_ff_widget_type(), XfaFfWidgetType::NumericEdit);
        let draw = node(&doc, XfaPacketType::Form, XfaElement::Draw, "arc");
        let value = draw.get_or_create_property(0, XfaElement::Value).unwrap();
        value.get_or_create_property(0, XfaElement::Arc).unwrap();
        assert_eq!(draw.get_ff_widget_type(), XfaFfWidgetType::Arc);
    }

    #[test]
    fn multiple_selection_resizes_data_value_children_and_preserves_xml_structure() {
        let doc = document();
        let first = choices(&doc, "first");
        let second = choices(&doc, "second");
        let data = data(&doc, "choices", XfaElement::DataGroup);
        bind(&first, &data);
        bind(&second, &data);
        assert!(first.sync_value(&wide("red\nblue"), false));
        let values = data.get_node_list_for_type(XfaElement::DataValue);
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].get_raw_value(), wide("red"));
        assert_eq!(values[1].get_raw_value(), wide("blue"));
        assert_eq!(second.get_raw_value(), wide("red\nblue"));
        assert_eq!(
            data.get_xml_mapping_node().unwrap().text_data(),
            Vec::<u32>::new()
        );
        for value in &values {
            assert_eq!(
                value.get_xml_mapping_node().unwrap().parent(),
                data.get_xml_mapping_node()
            );
        }
        let form_value = first
            .get_form_value_if_exists()
            .unwrap()
            .first_child()
            .unwrap();
        assert_eq!(form_value.get_element_type(), XfaElement::ExData);
        assert_eq!(
            form_value.get_cdata(XfaAttribute::ContentType),
            wide("text/xml")
        );
        assert_eq!(
            form_value.first_child().unwrap().get_element_type(),
            XfaElement::Sharpxml
        );
        assert!(first.sync_value(&wide("green"), false));
        assert_eq!(data.count_children(XfaElement::DataValue, false), 1);
        assert_eq!(data.first_child().unwrap().get_raw_value(), wide("green"));
        assert_eq!(values[0].get_xml_mapping_node().unwrap().parent(), None);
        assert_eq!(second.get_raw_value(), wide("green"));
        // fxcrt::Split preserves the final empty segment, including empty input.
        assert!(first.sync_value(&[], false));
        assert_eq!(data.count_children(XfaElement::DataValue, false), 1);
        assert!(data.first_child().unwrap().get_raw_value().is_empty());
        assert!(second.get_raw_value().is_empty());
    }

    struct ReentrantNotify {
        field: RefCell<Option<CxfaNode>>,
        nested: Cell<bool>,
        ready_reads: Cell<usize>,
    }
    impl XfaDocumentNotify for ReentrantNotify {
        fn on_child_added(&self, _parent: &CxfaNode, _child: &CxfaNode) {}
        fn on_child_removed(&self, _parent: &CxfaNode, _child: &CxfaNode) {}
        fn on_node_ready(&self, node: &CxfaNode) {
            if node.get_element_type() == XfaElement::Sharptext {
                let field = self.field.borrow().clone();
                if let Some(field) = field {
                    self.ready_reads.set(self.ready_reads.get() + 1);
                    field.get_raw_value();
                }
            }
        }
        fn on_attribute_changed(&self, node: &CxfaNode, attribute: XfaAttribute) {
            if attribute == XfaAttribute::Value
                && node.get_cdata(attribute) == wide("outer")
                && !self.nested.replace(true)
            {
                let field = self.field.borrow().clone().unwrap();
                assert!(field.sync_value(&wide("inner"), true));
            }
        }
    }
    #[test]
    fn reentrant_value_notification_preserves_newer_content_and_ui_ready_reads_are_safe() {
        let notify = Rc::new(ReentrantNotify {
            field: RefCell::new(None),
            nested: Cell::new(false),
            ready_reads: Cell::new(0),
        });
        let doc = CxfaDocument::with_notify(Some(notify.clone()));
        doc.set_xml_document(Rc::new(RefCell::new(CfxXmlDocument::new())));
        let first = field(&doc, "first");
        let second = field(&doc, "second");
        let data = data(&doc, "shared", XfaElement::DataValue);
        bind(&first, &data);
        bind(&second, &data);
        *notify.field.borrow_mut() = Some(first.clone());
        assert!(!first.sync_value(&wide("outer"), true));
        assert!(notify.nested.get());
        assert!(notify.ready_reads.get() > 0);
        assert_eq!(first.get_raw_value(), wide("inner"));
        assert_eq!(second.get_raw_value(), wide("inner"));
        assert_eq!(data.get_raw_value(), wide("inner"));
        assert_eq!(
            data.get_xml_mapping_node().unwrap().text_data(),
            wide("inner")
        );
    }

    #[test]
    fn explicit_null_state_and_attribute_removal_keep_template_defaults_and_mapping() {
        let doc = document();
        let field = field(&doc, "field");
        assert!(field.is_null());
        assert!(field.pre_null());
        field.set_pre_null(field.is_null());
        field.set_is_null(false);
        assert!(field.sync_value(&wide("value"), false));
        assert!(!field.is_null());
        assert!(field.pre_null());
        field.set_pre_null(field.is_null());
        field.set_is_null(true);
        assert!(field.sync_value(&[], false));
        assert!(field.is_null());
        assert!(!field.pre_null());
        let template = node(&doc, XfaPacketType::Template, XfaElement::Field, "template");
        template.set_cdata(XfaAttribute::Locale, &wide("fr_FR"));
        field.set_template_node(Some(&template));
        field.set_cdata(XfaAttribute::Locale, &wide("de_DE"));
        assert!(field.clear_attribute(XfaAttribute::Locale, false));
        assert_eq!(field.get_cdata(XfaAttribute::Locale), wide("fr_FR"));
        assert!(!field.clear_attribute(XfaAttribute::Locale, false));
        field.set_enum(XfaAttribute::Presence, XfaAttributeValue::Hidden, false);
        assert!(field.clear_attribute(XfaAttribute::Presence, false));
        assert_eq!(
            field.get_enum(XfaAttribute::Presence),
            XfaAttributeValue::Visible
        );
        assert!(field.clear_attribute(XfaAttribute::Name, false));
        assert_eq!(field.get_name_hash(), hash_name(&wide("template")));
        let data = data(&doc, "value", XfaElement::DataValue);
        data.set_cdata(XfaAttribute::ContentType, &wide("text/plain"));
        data.set_cdata(XfaAttribute::Value, &wide("text"));
        assert!(data
            .get_xml_mapping_node()
            .unwrap()
            .has_attribute(&wide("xfa:contentType")));
        assert!(data.clear_attribute(XfaAttribute::ContentType, false));
        assert!(!data
            .get_xml_mapping_node()
            .unwrap()
            .has_attribute(&wide("xfa:contentType")));
        assert!(data.clear_attribute(XfaAttribute::Value, false));
        assert!(data.get_xml_mapping_node().unwrap().text_data().is_empty());
    }

    #[test]
    fn exdata_content_read_write_and_buffer_sync_use_actual_raw_nodes() {
        let doc = document();
        for (content_type, raw_type) in [
            ("text/html", XfaElement::SharpxHTML),
            ("text/xml", XfaElement::Sharpxml),
            ("text/plain", XfaElement::Sharptext),
        ] {
            let exdata = node(&doc, XfaPacketType::Form, XfaElement::ExData, "");
            exdata.set_cdata(XfaAttribute::ContentType, &wide(content_type));
            assert_eq!(exdata.try_content(), None);
            assert_eq!(exdata.first_child().unwrap().get_element_type(), raw_type);
            assert!(exdata.set_content(&wide("plain"), &wide("plain"), false));
            assert_eq!(exdata.get_content(), wide("plain"));
            if content_type == "text/html" {
                assert!(exdata.get_cdata(XfaAttribute::ContentType).is_empty());
            }
        }
        let source = field(&doc, "source");
        let destination = field(&doc, "destination");
        let data = data(&doc, "shared", XfaElement::DataValue);
        bind(&destination, &data);
        assert!(source.sync_value(&wide("source"), false));
        assert!(destination.set_content(&wide("destination"), &wide("destination"), false));
        assert!(source.sync_buffer_map_data(&destination));
        assert_eq!(data.get_raw_value(), wide("destination"));
    }
}
