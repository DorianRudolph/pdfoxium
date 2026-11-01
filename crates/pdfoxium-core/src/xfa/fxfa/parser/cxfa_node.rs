// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! The parser foundation of `cxfa_node.{h,cpp}`, object metadata from
//! `cxfa_object`, and typed local attribute/content storage from `CJX_Object`.
//! Document-owned allocations and weak tree/mapping handles replace GC edges.
//! Clone/template ancestry, binding/value synchronization and UI/value model
//! preparation are connected; script execution and widget/layout rendering
//! remain separate consumers of these operations.
use super::{
    cxfa_document::{CxfaDocument, CxfaNodeStore},
    cxfa_measurement::CxfaMeasurement,
    node_schema::{
        schema_for, AttributeData, AttributeDefault, NodeSchema, PropertyData, XfaObjectType,
    },
    xfa_basic_data::{
        attribute_to_name, attribute_value_to_name, element_to_name, get_attribute_by_name,
        get_attribute_value_by_name,
    },
};
use crate::{
    fxcrt::{
        fx_string::string_to_float_wide,
        tree_node::{TreeNodeOwner, TreeNodeRef},
        xml::cfx_xmlnode::{XmlNodeRef, XmlNodeType},
    },
    xfa::fxfa::fxfa_basic::{
        XfaAttribute, XfaAttributeType, XfaAttributeValue, XfaElement, XfaHashCode, XfaPacketType,
        XfaUnit,
    },
};
use std::{
    collections::BTreeMap,
    rc::{Rc, Weak},
};

pub const XFA_PROPERTY_ONE_OF: u8 = 1;
pub const XFA_PROPERTY_DEFAULT_ONE_OF: u8 = 2;
pub const XFA_NODE_FILTER_CHILDREN: u8 = 1;
pub const XFA_NODE_FILTER_PROPERTIES: u8 = 2;
pub const XFA_NODE_FILTER_ONE_OF_PROPERTY: u8 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum XfaNodeFlag {
    None = 0,
    Initialized = 1,
    HasRemovedChildren = 2,
    NeedsInitApp = 4,
    BindFormItems = 8,
    UserInteractive = 16,
    UnusedNode = 32,
    LayoutGeneratedNode = 64,
}

/// Source WideString name/class hashing, independent of platform wchar size.
pub use super::xfa_basic_data::hash_name;
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}

use crate::fxjs::xfa::cfxjse_mapmodule::{CfxJseMapModule, MapValue as StoredValue};
pub(crate) struct CxfaNodeData {
    document: Weak<CxfaDocument>,
    store: Weak<CxfaNodeStore>,
    packet: XfaPacketType,
    schema: &'static NodeSchema,
    name_hash: u32,
    flags: u8,
    notifying_ready: bool,
    preparing_ui: bool,
    preparing_content: bool,
    is_null: bool,
    pre_null: bool,
    widget_ready: bool,
    layout_item: Option<crate::xfa::fxfa::layout::cxfa_layoutitem::CxfaLayoutItem>,
    xml_node: Option<XmlNodeRef>,
    aux_node: Option<CxfaNode>,
    binding_nodes: Vec<CxfaNode>,
    calc_data: Option<Vec<CxfaNode>>,
    calc_recursion_count: usize,
    image_data: Option<Box<image_data::NodeImageData>>,
    values: CfxJseMapModule<XfaAttribute>,
    custom_values: BTreeMap<Vec<u32>, Rc<[u32]>>,
}
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CxfaNode(pub(crate) TreeNodeRef<CxfaNodeData>);

impl CxfaNode {
    pub fn get_layout_item(
        &self,
    ) -> Option<crate::xfa::fxfa::layout::cxfa_layoutitem::CxfaLayoutItem> {
        self.0
            .with_value(|data| data.layout_item.clone())
            .filter(|item| item.is_alive())
    }
    pub fn set_layout_item(
        &self,
        item: Option<crate::xfa::fxfa::layout::cxfa_layoutitem::CxfaLayoutItem>,
    ) {
        self.0.with_value_mut(|data| data.layout_item = item);
    }

    pub(crate) fn allocate(
        document: &Rc<CxfaDocument>,
        packet: XfaPacketType,
        element: XfaElement,
    ) -> Option<(TreeNodeOwner<CxfaNodeData>, Self)> {
        let schema = schema_for(element)?;
        if !schema.supports_packet(packet) {
            return None;
        }
        Some(Self::allocate_with_schema(document, packet, schema))
    }
    fn allocate_with_schema(
        document: &Rc<CxfaDocument>,
        packet: XfaPacketType,
        schema: &'static NodeSchema,
    ) -> (TreeNodeOwner<CxfaNodeData>, Self) {
        let owner = TreeNodeOwner::new(CxfaNodeData {
            document: Rc::downgrade(document),
            store: Rc::downgrade(&document.node_store()),
            packet,
            schema,
            name_hash: 0,
            flags: 0,
            notifying_ready: false,
            preparing_ui: false,
            preparing_content: false,
            is_null: true,
            pre_null: true,
            widget_ready: false,
            layout_item: None,
            xml_node: None,
            aux_node: None,
            binding_nodes: Vec::new(),
            calc_data: None,
            calc_recursion_count: 0,
            image_data: None,
            values: CfxJseMapModule::new(),
            custom_values: BTreeMap::new(),
        });
        let node = Self(owner.node_ref());
        (owner, node)
    }
    pub fn is_alive(&self) -> bool {
        self.0.is_alive()
    }
    pub(crate) fn retain_store(&self) -> Option<Rc<CxfaNodeStore>> {
        if !self.is_alive() {
            return None;
        }
        self.0.with_value(|data| data.store.upgrade())
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self == other
    }
    pub fn document(&self) -> Option<Rc<CxfaDocument>> {
        if !self.is_alive() {
            return None;
        }
        self.0.with_value(|data| data.document.upgrade())
    }
    pub fn get_document(&self) -> Option<Rc<CxfaDocument>> {
        self.document()
    }
    pub fn get_element_type(&self) -> XfaElement {
        self.0.with_value(|data| data.schema.element)
    }
    pub fn get_packet_type(&self) -> XfaPacketType {
        self.0.with_value(|data| data.packet)
    }
    pub fn get_object_type(&self) -> XfaObjectType {
        self.0.with_value(|data| data.schema.object_type)
    }
    pub fn get_class_name(&self) -> &'static str {
        element_to_name(self.get_element_type())
    }
    pub fn get_class_hash_code(&self) -> u32 {
        self.get_class_name()
            .bytes()
            .fold(0u32, |h, c| h.wrapping_mul(1313).wrapping_add(u32::from(c)))
    }
    pub fn get_name_hash(&self) -> u32 {
        self.0.with_value(|data| data.name_hash)
    }
    pub fn is_unnamed(&self) -> bool {
        self.get_name_hash() == 0
    }
    pub fn is_node(&self) -> bool {
        matches!(
            self.get_object_type(),
            XfaObjectType::Node
                | XfaObjectType::NodeC
                | XfaObjectType::NodeV
                | XfaObjectType::ModelNode
                | XfaObjectType::TextNode
                | XfaObjectType::ContainerNode
                | XfaObjectType::ContentNode
        ) || self.get_element_type() == XfaElement::Delta
    }
    pub fn is_content_node(&self) -> bool {
        self.get_object_type() == XfaObjectType::ContentNode
    }
    pub fn is_container_node(&self) -> bool {
        self.get_object_type() == XfaObjectType::ContainerNode
    }
    pub fn is_model_node(&self) -> bool {
        self.get_object_type() == XfaObjectType::ModelNode
    }
    pub fn is_node_v(&self) -> bool {
        self.get_object_type() == XfaObjectType::NodeV
    }
    pub fn is_form_container(&self) -> bool {
        self.get_packet_type() == XfaPacketType::Form && self.is_container_node()
    }
    pub fn has_created_ui_widget(&self) -> bool {
        matches!(
            self.get_element_type(),
            XfaElement::Field | XfaElement::Draw | XfaElement::Subform | XfaElement::ExclGroup
        )
    }
    pub fn parent(&self) -> Option<Self> {
        self.is_alive().then(|| self.0.parent()).flatten().map(Self)
    }
    pub fn first_child(&self) -> Option<Self> {
        self.is_alive()
            .then(|| self.0.first_child())
            .flatten()
            .map(Self)
    }
    pub fn last_child(&self) -> Option<Self> {
        self.is_alive()
            .then(|| self.0.last_child())
            .flatten()
            .map(Self)
    }
    pub fn next_sibling(&self) -> Option<Self> {
        self.is_alive()
            .then(|| self.0.next_sibling())
            .flatten()
            .map(Self)
    }
    pub fn prev_sibling(&self) -> Option<Self> {
        self.is_alive()
            .then(|| self.0.prev_sibling())
            .flatten()
            .map(Self)
    }
    pub fn nth_child(&self, index: i32) -> Option<Self> {
        self.is_alive()
            .then(|| self.0.nth_child(index))
            .flatten()
            .map(Self)
    }
    fn schema(&self) -> &'static NodeSchema {
        self.0.with_value(|data| data.schema)
    }
    pub fn get_property_data(&self, element: XfaElement) -> Option<&'static PropertyData> {
        self.schema().property(element)
    }
    pub fn has_property(&self, element: XfaElement) -> bool {
        self.get_property_data(element).is_some()
    }
    pub fn has_property_flag(&self, element: XfaElement, flag: u8) -> bool {
        self.get_property_data(element)
            .is_some_and(|property| property.flags & flag != 0)
    }
    pub fn property_occurrence_count(&self, element: XfaElement) -> u8 {
        self.get_property_data(element)
            .map_or(0, |property| property.occurrence_count)
    }
    pub fn get_first_property_with_flag(&self, flag: u8) -> Option<XfaElement> {
        self.schema()
            .properties
            .iter()
            .find(|property| property.flags & flag != 0)
            .map(|property| property.element)
    }
    pub fn get_property(&self, index: i32, element: XfaElement) -> (Option<Self>, i32) {
        if index < 0 || index >= i32::from(self.property_occurrence_count(element)) {
            return (None, 0);
        }
        let mut count = 0;
        let mut child = self.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if node.get_element_type() == element {
                count += 1;
                if count > index {
                    return (Some(node), count);
                }
            }
        }
        (None, count)
    }
    pub fn get_or_create_property(&self, index: i32, element: XfaElement) -> Option<Self> {
        if index < 0 || index >= i32::from(self.property_occurrence_count(element)) {
            return None;
        }
        let (node, mut count) = self.get_property(index, element);
        if node.is_some() {
            return node;
        }
        if self.has_property_flag(element, XFA_PROPERTY_ONE_OF) {
            let mut child = self.first_child();
            while let Some(node) = child {
                child = node.next_sibling();
                if self.has_property_flag(node.get_element_type(), XFA_PROPERTY_ONE_OF) {
                    return None;
                }
            }
        }
        let document = self.document()?;
        let mut result = None;
        while count <= index {
            let node = document.create_node(self.get_packet_type(), element)?;
            if !self.insert_child_and_notify(-1, &node) {
                return None;
            }
            node.set_initialized_flag_and_notify();
            result = Some(node);
            count += 1;
        }
        result
    }
    pub fn get_attribute_data(&self, attribute: XfaAttribute) -> Option<&'static AttributeData> {
        self.schema().attribute(attribute)
    }
    /// Schema support, distinct from an explicitly assigned CJX value.
    pub fn has_attribute(&self, attribute: XfaAttribute) -> bool {
        self.get_attribute_data(attribute).is_some()
    }
    pub fn has_local_attribute(&self, attribute: XfaAttribute) -> bool {
        self.0
            .with_value(|data| data.values.contains_key(&attribute))
    }
    pub fn get_attribute(&self, index: usize) -> XfaAttribute {
        self.schema()
            .attributes
            .get(index)
            .map_or(XfaAttribute::Unknown, |data| data.attribute)
    }
    pub fn get_attribute_type(&self, attribute: XfaAttribute) -> XfaAttributeType {
        self.get_attribute_data(attribute)
            .map_or(XfaAttributeType::CData, |data| data.attribute_type)
    }
    pub fn is_valid_in_packet(&self, packet: XfaPacketType) -> bool {
        self.schema().supports_packet(packet)
    }
    pub fn count_children(&self, element: XfaElement, only_child: bool) -> usize {
        let mut count = 0;
        let mut child = self.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if (element == XfaElement::Unknown || node.get_element_type() == element)
                && (!only_child || !self.has_property(node.get_element_type()))
            {
                count += 1;
            }
        }
        count
    }
    pub fn get_child(&self, index: usize, element: XfaElement, only_child: bool) -> Option<Self> {
        let mut count = 0;
        let mut child = self.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if (element == XfaElement::Unknown || node.get_element_type() == element)
                && (!only_child || !self.has_property(node.get_element_type()))
            {
                if count == index {
                    return Some(node);
                }
                count += 1;
            }
        }
        None
    }
    pub fn is_ancestor_of(&self, other: &Self) -> bool {
        let mut node = Some(other.clone());
        while let Some(current) = node {
            if self == &current {
                return true;
            }
            node = current.parent();
        }
        false
    }
    pub fn insert_child_and_notify(&self, index: i32, child: &Self) -> bool {
        self.insert_child_before_and_notify(child, self.nth_child(index).as_ref())
    }
    pub fn insert_child_before_and_notify(&self, child: &Self, before: Option<&Self>) -> bool {
        let Some(document) = self.document() else {
            return false;
        };
        if !child.is_alive()
            || child.parent().is_some()
            || child.is_ancestor_of(self)
            || !child
                .document()
                .is_some_and(|other| Rc::ptr_eq(&document, &other))
            || before.is_some_and(|node| node.parent().as_ref() != Some(self))
        {
            return false;
        }
        child.clear_flag(XfaNodeFlag::HasRemovedChildren);
        self.0.insert_before(&child.0, before.map(|node| &node.0));
        if let Some(notify) = document.notify() {
            notify.on_child_added(self, child);
        }
        // A host may have moved the child while handling OnChildAdded.
        if child.parent().as_ref() == Some(self) && self.is_need_saving_xml_node() {
            if let (Some(parent_xml), Some(child_xml)) =
                (self.get_xml_mapping_node(), child.get_xml_mapping_node())
            {
                if parent_xml != child_xml && child_xml.parent().is_none() {
                    let anchor = before
                        .filter(|node| node.parent().as_ref() == Some(self))
                        .and_then(Self::get_xml_mapping_node)
                        .filter(|xml| xml.parent().as_ref() == Some(&parent_xml));
                    parent_xml.insert_before(&child_xml, anchor.as_ref());
                }
            }
        }
        true
    }
    pub fn remove_child(&self, child: &Self) -> bool {
        self.remove_child_and_notify(child, true)
    }
    pub fn remove_child_and_notify(&self, child: &Self, notify: bool) -> bool {
        if !self.is_alive() || !child.is_alive() || child.parent().as_ref() != Some(self) {
            return false;
        }
        let document = self.document();
        child.set_flag(XfaNodeFlag::HasRemovedChildren);
        self.0.remove_child(&child.0);
        if notify {
            if let Some(callback) = document.as_ref().and_then(|document| document.notify()) {
                callback.on_child_removed(self, child);
            }
        }
        if child.parent().is_some() || !self.is_need_saving_xml_node() {
            return true;
        }
        let (Some(parent_xml), Some(child_xml)) =
            (self.get_xml_mapping_node(), child.get_xml_mapping_node())
        else {
            return true;
        };
        if !child.is_attribute_in_xml() {
            if child_xml.parent().as_ref() == Some(&parent_xml) {
                parent_xml.remove_child(&child_xml);
            }
            return true;
        }
        if child_xml.node_type() == XmlNodeType::Element {
            child_xml.remove_attribute(&child.get_cdata(XfaAttribute::QualifiedName));
        }
        // Detached metadata becomes an ordinary, detached data element.
        // Use the child's own value (the source accidentally reads its parent).
        if let Some(xml_document) = document.and_then(|document| document.xml_document()) {
            let name = child.get_cdata(XfaAttribute::Name);
            if !name.is_empty() {
                let mut xml_document = xml_document.borrow_mut();
                let xml = xml_document.create_element(&name);
                let value = child.get_cdata(XfaAttribute::Value);
                if !value.is_empty() {
                    xml.append_last_child(&xml_document.create_text(&value));
                }
                child.set_xml_mapping_node(xml);
                child.set_enum(XfaAttribute::Contains, XfaAttributeValue::Unknown, false);
            }
        }
        true
    }
    pub fn get_first_child_by_name(&self, name: &[u32]) -> Option<Self> {
        self.get_first_child_by_name_hash(hash_name(name))
    }
    pub fn get_first_child_by_name_hash(&self, hash: u32) -> Option<Self> {
        let mut child = self.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if node.get_name_hash() == hash {
                return Some(node);
            }
        }
        None
    }
    pub fn get_first_child_by_class(&self, element: XfaElement) -> Option<Self> {
        let mut child = self.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if node.get_element_type() == element {
                return Some(node);
            }
        }
        None
    }
    pub fn get_next_same_name_sibling(&self, hash: u32) -> Option<Self> {
        let mut sibling = self.next_sibling();
        while let Some(node) = sibling {
            sibling = node.next_sibling();
            if node.get_name_hash() == hash {
                return Some(node);
            }
        }
        None
    }
    pub fn get_next_same_class_sibling(&self, element: XfaElement) -> Option<Self> {
        let mut sibling = self.next_sibling();
        while let Some(node) = sibling {
            sibling = node.next_sibling();
            if node.get_element_type() == element {
                return Some(node);
            }
        }
        None
    }
    pub fn get_next_container_sibling(&self) -> Option<Self> {
        let mut sibling = self.next_sibling();
        while let Some(node) = sibling {
            sibling = node.next_sibling();
            if node.is_container_node() {
                return Some(node);
            }
        }
        None
    }
    pub fn get_prev_container_sibling(&self) -> Option<Self> {
        let mut sibling = self.prev_sibling();
        while let Some(node) = sibling {
            sibling = node.prev_sibling();
            if node.is_container_node() {
                return Some(node);
            }
        }
        None
    }
    pub fn get_first_container_child(&self) -> Option<Self> {
        let mut child = self.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if node.is_container_node() {
                return Some(node);
            }
        }
        None
    }
    pub fn get_container_parent(&self) -> Option<Self> {
        let mut parent = self.parent();
        while let Some(node) = parent {
            parent = node.parent();
            if node.is_container_node() {
                return Some(node);
            }
        }
        None
    }
    pub fn is_property(&self) -> bool {
        self.parent()
            .is_some_and(|parent| parent.has_property(self.get_element_type()))
    }
    pub fn get_node_list_for_type(&self, element: XfaElement) -> Vec<Self> {
        let mut nodes = Vec::new();
        let mut child = self.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if node.get_element_type() == element {
                nodes.push(node);
            }
        }
        nodes
    }
    pub fn get_node_list_with_filter(&self, filter: u8) -> Vec<Self> {
        if filter == 0 {
            return Vec::new();
        }
        let children = filter & XFA_NODE_FILTER_CHILDREN != 0;
        let properties = filter & XFA_NODE_FILTER_PROPERTIES != 0;
        let one_of = filter & XFA_NODE_FILTER_ONE_OF_PROPERTY != 0;
        let mut nodes = Vec::new();
        let mut child = self.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            let element = node.get_element_type();
            let include = if children && properties && !one_of {
                true
            } else if self.has_property(element) {
                properties
                    || (one_of && self.has_property_flag(element, XFA_PROPERTY_ONE_OF))
                    || (children && matches!(element, XfaElement::Variables | XfaElement::PageSet))
            } else {
                children
            };
            if include {
                nodes.push(node);
            }
        }
        if one_of && nodes.is_empty() {
            if let Some(element) = self.get_first_property_with_flag(XFA_PROPERTY_DEFAULT_ONE_OF) {
                if let Some(document) = self.document() {
                    if let Some(node) = document.create_node(self.get_packet_type(), element) {
                        if self.insert_child_and_notify(-1, &node) {
                            node.set_initialized_flag_and_notify();
                            nodes.push(node);
                        }
                    }
                }
            }
        }
        nodes
    }
    pub fn has_flag(&self, flag: XfaNodeFlag) -> bool {
        let mut node = Some(self.clone());
        while let Some(current) = node {
            if !current.is_alive() {
                return false;
            }
            if current.0.with_value(|data| data.flags & (flag as u8) != 0) {
                return true;
            }
            if flag != XfaNodeFlag::HasRemovedChildren {
                return false;
            }
            node = current.parent();
        }
        false
    }
    pub fn set_flag(&self, flag: XfaNodeFlag) {
        self.0.with_value_mut(|data| data.flags |= flag as u8);
    }
    pub fn clear_flag(&self, flag: XfaNodeFlag) {
        self.0.with_value_mut(|data| data.flags &= !(flag as u8));
    }
    pub fn is_initialized(&self) -> bool {
        self.has_flag(XfaNodeFlag::Initialized)
    }
    pub fn is_user_interactive(&self) -> bool {
        self.has_flag(XfaNodeFlag::UserInteractive)
    }
    pub fn is_unused_node(&self) -> bool {
        self.has_flag(XfaNodeFlag::UnusedNode)
    }
    pub fn is_layout_generated_node(&self) -> bool {
        self.has_flag(XfaNodeFlag::LayoutGeneratedNode)
    }
    pub fn has_removed_children(&self) -> bool {
        self.has_flag(XfaNodeFlag::HasRemovedChildren)
    }
    pub fn set_initialized_flag(&self) {
        self.set_flag(XfaNodeFlag::Initialized);
    }
    pub fn set_initialized_flag_and_notify(&self) {
        if !self.is_alive() || self.is_initialized() {
            return;
        }
        if self.0.with_value_mut(|data| {
            if data.notifying_ready {
                true
            } else {
                data.notifying_ready = true;
                false
            }
        }) {
            return;
        }
        // OnNodeReady observes the uninitialized node, as in the source. The
        // guard prevents recursive notification without holding a tree borrow.
        let document = self.document();
        if let Some(notify) = document.as_ref().and_then(|document| document.notify()) {
            notify.on_node_ready(self);
        }
        if self.is_alive() {
            self.0.with_value_mut(|data| {
                data.notifying_ready = false;
                data.flags |= XfaNodeFlag::Initialized as u8;
            });
        }
    }
    pub fn set_node_and_descendants_unused(&self) {
        let mut current = Some(self.clone());
        while let Some(node) = current {
            node.set_flag(XfaNodeFlag::UnusedNode);
            if let Some(child) = node.first_child() {
                current = Some(child);
                continue;
            }
            let mut ancestor = node;
            loop {
                if ancestor == *self {
                    return;
                }
                if let Some(next) = ancestor.next_sibling() {
                    current = Some(next);
                    break;
                }
                let Some(parent) = ancestor.parent() else {
                    return;
                };
                ancestor = parent;
            }
        }
    }
    pub fn set_xml_mapping_node(&self, node: XmlNodeRef) {
        self.0.with_value_mut(|data| data.xml_node = Some(node));
    }
    pub fn clear_xml_mapping_node(&self) {
        self.0.with_value_mut(|data| data.xml_node = None);
    }
    pub fn get_xml_mapping_node(&self) -> Option<XmlNodeRef> {
        if !self.is_alive() {
            return None;
        }
        self.0
            .with_value(|data| data.xml_node.clone())
            .filter(XmlNodeRef::is_alive)
    }
    pub fn create_xml_mapping_node(&self) -> Option<XmlNodeRef> {
        if let Some(node) = self.get_xml_mapping_node() {
            return Some(node);
        }
        let name = self.get_cdata(XfaAttribute::Name);
        if name.is_empty() {
            return None;
        }
        let document = self.document()?.xml_document()?;
        let node = document.borrow_mut().create_element(&name);
        self.set_xml_mapping_node(node.clone());
        Some(node)
    }
    pub fn is_need_saving_xml_node(&self) -> bool {
        self.is_alive()
            && self.get_xml_mapping_node().is_some()
            && (self.get_packet_type() == XfaPacketType::Datasets
                || self.get_element_type() == XfaElement::Xfa)
    }
    pub fn is_attribute_in_xml(&self) -> bool {
        self.get_enum(XfaAttribute::Contains) == XfaAttributeValue::MetaData
    }
    pub fn update_name_hash(&self) {
        let hash = hash_name(&self.get_cdata(XfaAttribute::Name));
        self.0.with_value_mut(|data| data.name_hash = hash);
    }
    pub fn get_template_node_if_exists(&self) -> Option<Self> {
        self.0
            .with_value(|data| data.aux_node.clone())
            .filter(Self::is_alive)
    }
    pub fn set_template_node(&self, node: Option<&Self>) {
        self.0.with_value_mut(|data| data.aux_node = node.cloned());
    }
    pub fn get_model_node(&self) -> Option<Self> {
        let document = self.document()?;
        let hash = match self.get_packet_type() {
            XfaPacketType::Xdp => return document.get_root(),
            XfaPacketType::Config => XfaHashCode::Config,
            XfaPacketType::Template => XfaHashCode::Template,
            XfaPacketType::Form => XfaHashCode::Form,
            XfaPacketType::Datasets => XfaHashCode::Datasets,
            XfaPacketType::LocaleSet => XfaHashCode::LocaleSet,
            XfaPacketType::ConnectionSet => XfaHashCode::ConnectionSet,
            XfaPacketType::SourceSet => XfaHashCode::SourceSet,
            XfaPacketType::Xdc => XfaHashCode::Xdc,
            _ => return Some(self.clone()),
        };
        document.get_xfa_node(hash as u32)
    }
    pub fn try_namespace(&self) -> Option<Vec<u32>> {
        if !self.is_model_node()
            && self.get_element_type() != XfaElement::Packet
            && self.get_packet_type() != XfaPacketType::Datasets
        {
            let model = self.get_model_node()?;
            return if model == *self {
                None
            } else {
                model.try_namespace()
            };
        }
        let xml = self.get_xml_mapping_node()?;
        if xml.node_type() != XmlNodeType::Element {
            return None;
        }
        if self.get_packet_type() == XfaPacketType::Datasets
            && self.get_element_type() == XfaElement::DataValue
            && self.is_attribute_in_xml()
        {
            let qualified = self.get_cdata(XfaAttribute::QualifiedName);
            // XML attribute names do not inherit the default namespace. Resolve
            // their prefix, rather than the source's entire qualified name.
            let Some(colon) = qualified.iter().position(|&unit| unit == u32::from(b':')) else {
                return Some(Vec::new());
            };
            let mut name = wide("xmlns:");
            name.extend_from_slice(&qualified[..colon]);
            let mut element = Some(xml);
            while let Some(node) = element {
                element = node.parent();
                if node.node_type() == XmlNodeType::Element && node.has_attribute(&name) {
                    return Some(node.get_attribute(&name).to_vec());
                }
            }
            return None;
        }
        Some(xml.namespace_uri().to_vec())
    }
    pub fn get_default_boolean(&self, attribute: XfaAttribute) -> Option<bool> {
        let data = self.get_attribute_data(attribute)?;
        match data.default_value {
            AttributeDefault::Boolean(value) => Some(value),
            _ => None,
        }
    }
    pub fn get_default_integer(&self, attribute: XfaAttribute) -> Option<i32> {
        let data = self.get_attribute_data(attribute)?;
        match data.default_value {
            AttributeDefault::Integer(value) => Some(value),
            _ => None,
        }
    }
    pub fn get_default_enum(&self, attribute: XfaAttribute) -> Option<XfaAttributeValue> {
        let data = self.get_attribute_data(attribute)?;
        match data.default_value {
            AttributeDefault::Enum(value) => Some(value),
            _ => None,
        }
    }
    pub fn get_default_measurement(&self, attribute: XfaAttribute) -> Option<CxfaMeasurement> {
        let data = self.get_attribute_data(attribute)?;
        match data.default_value {
            AttributeDefault::Measure(value) => Some(CxfaMeasurement::from_string(&wide(value))),
            _ => None,
        }
    }
    pub fn get_default_cdata(&self, attribute: XfaAttribute) -> Option<Vec<u32>> {
        let data = self.get_attribute_data(attribute)?;
        if data.attribute_type != XfaAttributeType::CData {
            return None;
        }
        match data.default_value {
            AttributeDefault::Text(value) => Some(wide(value)),
            AttributeDefault::None => Some(Vec::new()),
            _ => None,
        }
    }
    // Brent's cycle detection keeps template lookup linear with constant
    // auxiliary storage, without cloning maps or allocating a visited set.
    fn following_chain<R>(&self, mut lookup: impl FnMut(&CxfaNodeData) -> Option<R>) -> Option<R> {
        let mut node = self.clone();
        let mut anchor = node.clone();
        let mut power = 1usize;
        let mut length = 0usize;
        loop {
            if !node.is_alive() {
                return None;
            }
            if let Some(value) = node.0.with_value(&mut lookup) {
                return Some(value);
            }
            if node.get_packet_type() == XfaPacketType::Datasets {
                return None;
            }
            let next = node.get_template_node_if_exists()?;
            length += 1;
            if next == anchor {
                return None;
            }
            if length == power {
                anchor = next.clone();
                power = power.saturating_mul(2);
                length = 0;
            }
            node = next;
        }
    }
    fn scalar(&self, attribute: XfaAttribute) -> Option<i32> {
        let element = self.get_element_type();
        self.following_chain(|data| {
            match data
                .values
                .get(&attribute)
                .filter(|_| data.schema.element == element)
            {
                Some(StoredValue::Scalar(value)) => Some(*value),
                _ => None,
            }
        })
    }
    pub fn try_boolean(&self, attribute: XfaAttribute, use_default: bool) -> Option<bool> {
        self.scalar(attribute).map(|value| value != 0).or_else(|| {
            if use_default {
                self.get_default_boolean(attribute)
            } else {
                None
            }
        })
    }
    pub fn get_boolean(&self, attribute: XfaAttribute) -> bool {
        self.try_boolean(attribute, true).unwrap_or(false)
    }
    pub fn try_integer(&self, attribute: XfaAttribute, use_default: bool) -> Option<i32> {
        self.scalar(attribute).or_else(|| {
            if use_default {
                self.get_default_integer(attribute)
            } else {
                None
            }
        })
    }
    pub fn get_integer(&self, attribute: XfaAttribute) -> i32 {
        self.try_integer(attribute, true).unwrap_or(0)
    }
    pub fn try_enum(
        &self,
        attribute: XfaAttribute,
        use_default: bool,
    ) -> Option<XfaAttributeValue> {
        self.scalar(attribute)
            .and_then(XfaAttributeValue::from_i32)
            .or_else(|| {
                if use_default {
                    self.get_default_enum(attribute)
                } else {
                    None
                }
            })
    }
    pub fn get_enum(&self, attribute: XfaAttribute) -> XfaAttributeValue {
        self.try_enum(attribute, true)
            .unwrap_or(XfaAttributeValue::Unknown)
    }
    pub fn try_measure(
        &self,
        attribute: XfaAttribute,
        use_default: bool,
    ) -> Option<CxfaMeasurement> {
        let element = self.get_element_type();
        self.following_chain(|data| {
            match data
                .values
                .get(&attribute)
                .filter(|_| data.schema.element == element)
            {
                Some(StoredValue::Measure(value)) => Some(*value),
                _ => None,
            }
        })
        .or_else(|| {
            if use_default {
                self.get_default_measurement(attribute)
            } else {
                None
            }
        })
    }
    pub fn get_measure(&self, attribute: XfaAttribute) -> CxfaMeasurement {
        self.try_measure(attribute, true).unwrap_or_default()
    }
    pub fn get_measure_in_unit(&self, attribute: XfaAttribute, unit: XfaUnit) -> f32 {
        self.get_measure(attribute).to_unit(unit)
    }
    pub fn try_measure_as_float(&self, attribute: XfaAttribute) -> Option<f32> {
        self.try_measure(attribute, false)
            .map(|value| value.to_unit(XfaUnit::Pt))
    }
    pub fn try_cdata(&self, attribute: XfaAttribute, use_default: bool) -> Option<Vec<u32>> {
        let element = self.get_element_type();
        self.following_chain(|data| {
            match data
                .values
                .get(&attribute)
                .filter(|_| data.schema.element == element)
            {
                Some(StoredValue::Text(value)) => Some(value.clone()),
                _ => None,
            }
        })
        .map(|value| value.to_vec())
        .or_else(|| {
            if use_default {
                self.get_default_cdata(attribute)
            } else {
                None
            }
        })
    }
    pub fn get_cdata(&self, attribute: XfaAttribute) -> Vec<u32> {
        self.try_cdata(attribute, true).unwrap_or_default()
    }
    fn notify_attribute(&self, attribute: XfaAttribute, notify: bool) {
        if notify && self.is_initialized() {
            // Keep the document's arena alive until the observer returns,
            // including observers which release the host's last document Rc.
            if let Some(document) = self.document() {
                if let Some(callback) = document.notify() {
                    callback.on_attribute_changed(self, attribute);
                }
            }
        }
    }
    fn store(
        &self,
        attribute: XfaAttribute,
        value: StoredValue,
        notify: bool,
        always_notify: bool,
    ) -> bool {
        self.store_with_script(attribute, value, notify, always_notify, false)
    }
    fn store_with_script(
        &self,
        attribute: XfaAttribute,
        value: StoredValue,
        notify: bool,
        always_notify: bool,
        script_modify: bool,
    ) -> bool {
        // Retain the arena and release every interior borrow before observers.
        let _document = self.document();
        if !always_notify
            && self
                .0
                .with_value(|data| data.values.get(&attribute) == Some(&value))
        {
            return false;
        }
        if notify && self.is_initialized() {
            if let Some(callback) = self.document().and_then(|document| document.notify()) {
                callback.on_attribute_changing(self, attribute);
            }
        }
        if !self.is_alive() {
            return false;
        }
        self.0.with_value_mut(|data| {
            if attribute == XfaAttribute::Name {
                if let StoredValue::Text(text) = &value {
                    data.name_hash = hash_name(text);
                }
            }
            data.values.insert(attribute, value);
        });
        if script_modify && notify && self.is_initialized() {
            if let Some(callback) = self.document().and_then(|document| document.notify()) {
                callback.on_attribute_changed_by_script(self, attribute);
            }
        } else {
            self.notify_attribute(attribute, notify);
        }
        true
    }
    fn write_scalar_xml(&self, attribute: XfaAttribute, value: &[u32]) {
        if self.is_need_saving_xml_node() {
            if let Some(xml) = self
                .get_xml_mapping_node()
                .filter(|xml| xml.node_type() == XmlNodeType::Element)
            {
                xml.set_attribute(&wide(attribute_to_name(attribute)), value);
            }
        }
    }
    pub fn set_boolean(&self, attribute: XfaAttribute, value: bool, notify: bool) {
        let stored = StoredValue::Scalar(i32::from(value));
        self.store(attribute, stored.clone(), notify, false);
        if self.is_alive()
            && self
                .0
                .with_value(|data| data.values.get(&attribute) == Some(&stored))
        {
            self.write_scalar_xml(attribute, &wide(if value { "1" } else { "0" }));
        }
    }
    pub fn set_integer(&self, attribute: XfaAttribute, value: i32, notify: bool) {
        let stored = StoredValue::Scalar(value);
        self.store(attribute, stored.clone(), notify, false);
        if self.is_alive()
            && self
                .0
                .with_value(|data| data.values.get(&attribute) == Some(&stored))
        {
            self.write_scalar_xml(attribute, &wide(&value.to_string()));
        }
    }
    pub fn set_enum(&self, attribute: XfaAttribute, value: XfaAttributeValue, notify: bool) {
        let stored = StoredValue::Scalar(value as i32);
        self.store(attribute, stored.clone(), notify, false);
        if self.is_alive()
            && self
                .0
                .with_value(|data| data.values.get(&attribute) == Some(&stored))
        {
            self.write_scalar_xml(attribute, &wide(attribute_value_to_name(value)));
        }
    }
    pub fn set_measure(&self, attribute: XfaAttribute, value: CxfaMeasurement, notify: bool) {
        self.store(attribute, StoredValue::Measure(value), notify, true);
    }
    pub fn set_cdata(&self, attribute: XfaAttribute, value: &[u32]) {
        self.set_cdata_and_notify(attribute, value, false);
    }
    pub fn set_cdata_and_notify(&self, attribute: XfaAttribute, value: &[u32], notify: bool) {
        let value: Rc<[u32]> = Rc::from(value);
        let stored = StoredValue::Text(value.clone());
        // NameHash is updated with the value, before the changed callback.
        self.store(attribute, stored.clone(), notify, false);
        if !self.is_alive()
            || !self
                .0
                .with_value(|data| data.values.get(&attribute) == Some(&stored))
            || !self.is_need_saving_xml_node()
            || matches!(
                attribute,
                XfaAttribute::QualifiedName | XfaAttribute::BindingNode
            )
        {
            return;
        }
        if attribute == XfaAttribute::Name
            && matches!(
                self.get_element_type(),
                XfaElement::DataValue | XfaElement::DataGroup
            )
        {
            return;
        }
        if attribute == XfaAttribute::Value {
            self.set_to_xml(&value);
            return;
        }
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
            xml.set_attribute(&name, &value);
        }
    }
    pub fn set_attribute_by_enum(
        &self,
        attribute: XfaAttribute,
        value: &[u32],
        notify: bool,
    ) -> bool {
        match self.get_attribute_type(attribute) {
            XfaAttributeType::CData => self.set_cdata_and_notify(attribute, value, notify),
            XfaAttributeType::Boolean => {
                self.set_boolean(attribute, value != [u32::from(b'0')], notify)
            }
            XfaAttributeType::Integer => self.set_integer(
                attribute,
                string_to_float_wide(value).round() as i32,
                notify,
            ),
            XfaAttributeType::Measure => {
                self.set_measure(attribute, CxfaMeasurement::from_string(value), notify)
            }
            XfaAttributeType::Enum => {
                let Some(value) =
                    get_attribute_value_by_name(value).or_else(|| self.get_default_enum(attribute))
                else {
                    return false;
                };
                self.set_enum(attribute, value, notify);
            }
        }
        true
    }
    pub fn set_attribute(&self, attribute: XfaAttribute, value: &[u32], notify: bool) -> bool {
        self.set_attribute_by_enum(attribute, value, notify)
    }
    pub fn try_attribute(&self, attribute: XfaAttribute, use_default: bool) -> Option<Vec<u32>> {
        match self.get_attribute_type(attribute) {
            XfaAttributeType::CData => self.try_cdata(attribute, use_default),
            XfaAttributeType::Boolean => self
                .try_boolean(attribute, use_default)
                .map(|value| wide(if value { "1" } else { "0" })),
            XfaAttributeType::Integer => self
                .try_integer(attribute, use_default)
                .map(|value| wide(&value.to_string())),
            XfaAttributeType::Measure => self
                .try_measure(attribute, use_default)
                .map(|value| value.to_string()),
            XfaAttributeType::Enum => self
                .try_enum(attribute, use_default)
                .map(|value| wide(attribute_value_to_name(value))),
        }
    }
    pub fn get_attribute_by_enum(&self, attribute: XfaAttribute) -> Vec<u32> {
        self.try_attribute(attribute, true).unwrap_or_default()
    }
    pub fn set_attribute_by_string(&self, name: &[u32], value: &[u32]) {
        if let Some(info) = get_attribute_by_name(name) {
            self.set_attribute_by_enum(info.attribute, value, true);
        } else {
            self.0.with_value_mut(|data| {
                data.custom_values.insert(name.to_vec(), Rc::from(value));
            });
        }
    }
    pub fn get_attribute_by_string(&self, name: &[u32]) -> Vec<u32> {
        if let Some(info) = get_attribute_by_name(name) {
            self.get_attribute_by_enum(info.attribute)
        } else {
            self.following_chain(|data| data.custom_values.get(name).cloned())
                .map_or_else(Vec::new, |value| value.to_vec())
        }
    }
    /// CJX RemoveAttribute removes only a custom-name entry.
    pub fn remove_attribute(&self, name: &[u32]) {
        self.0.with_value_mut(|data| {
            data.custom_values.remove(name);
        });
    }
    pub fn set_attribute_value(&self, value: &[u32], xml_value: &[u32], notify: bool) {
        self.set_attribute_value_with_script(value, xml_value, notify, false);
    }
    pub(crate) fn set_attribute_value_with_script(
        &self,
        value: &[u32],
        xml_value: &[u32],
        notify: bool,
        script_modify: bool,
    ) {
        let value: Rc<[u32]> = Rc::from(value);
        let stored = StoredValue::Text(value);
        if self.store_with_script(
            XfaAttribute::Value,
            stored.clone(),
            notify,
            false,
            script_modify,
        ) && self.is_alive()
            && self
                .0
                .with_value(|data| data.values.get(&XfaAttribute::Value) == Some(&stored))
            && self.is_need_saving_xml_node()
        {
            self.set_to_xml(xml_value);
        }
    }
    pub fn set_to_xml(&self, value: &[u32]) -> bool {
        let Some(xml) = self.get_xml_mapping_node() else {
            return false;
        };
        match xml.node_type() {
            XmlNodeType::Element => {
                if self.is_attribute_in_xml() {
                    xml.set_attribute(&self.get_cdata(XfaAttribute::QualifiedName), value);
                    return true;
                }
                let Some(document) = self.document().and_then(|document| document.xml_document())
                else {
                    return false;
                };
                let mut preserve_children = false;
                if self.get_packet_type() == XfaPacketType::Datasets {
                    let mut child = self.first_child();
                    while let Some(node) = child {
                        child = node.next_sibling();
                        if node.has_bind_items() {
                            preserve_children = true;
                            break;
                        }
                    }
                }
                if !preserve_children {
                    xml.remove_all_children();
                }
                let text = document.borrow_mut().create_text(value);
                xml.append_last_child(&text);
                true
            }
            XmlNodeType::Text | XmlNodeType::CharData => {
                xml.set_text(value);
                true
            }
            XmlNodeType::Instruction => false,
        }
    }
    pub fn presence_requires_space(&self) -> bool {
        matches!(
            self.try_enum(XfaAttribute::Presence, true)
                .unwrap_or(XfaAttributeValue::Visible),
            XfaAttributeValue::Visible | XfaAttributeValue::Invisible
        )
    }
}

#[path = "cxfa_node_merge.rs"]
mod merge;

#[path = "cxfa_node_value.rs"]
mod value;
pub use value::XfaFfWidgetType;

#[cfg(test)]
#[path = "cxfa_node_unittest.rs"]
mod upstream_tests;

#[path = "cxfa_node_calculation.rs"]
mod calculation;

#[path = "cxfa_node_image.rs"]
mod image_data;
