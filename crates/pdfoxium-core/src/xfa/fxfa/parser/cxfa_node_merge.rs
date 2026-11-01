// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Clone, binding edges and SOM ancestry sections of `cxfa_node.cpp`, plus
//! `CJX_Object::MergeAllData`. These are model operations; executing script,
//! synchronizing bound values and constructing widgets remain separate work.
use super::super::xfa_basic_data::get_element_by_name;
use super::*;

impl CxfaNode {
    /// Copy explicit local storage, as CJX_Object::MergeAllData does for Clone.
    /// Typed keys include the source element; this connected operation requires
    /// matching element classes instead of misinterpreting another class's keys.
    /// Values inherited through a template are not materialized here.
    pub fn merge_all_data(&self, destination: &Self) -> bool {
        if !self.is_alive()
            || !destination.is_alive()
            || self.get_element_type() != destination.get_element_type()
        {
            return false;
        }
        if self == destination {
            return true;
        }
        // This intentional value copy is the source operation, not a temporary
        // snapshot for borrowing. Text values retain their shared WideString
        // backing and no callback executes while either payload is borrowed.
        self.0.with_value(|source| {
            destination.0.with_value_mut(|target| {
                target.values.merge_data_from(&source.values);
                target.custom_values.extend(
                    source
                        .custom_values
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone())),
                );
            })
        });
        true
    }

    /// Source Clone(bool); ordinary Rust Clone only copies the weak handle.
    pub fn clone_node(&self, recursive: bool) -> Option<Self> {
        self.clone_subtree(recursive, false)
    }

    // The native recursive calls are represented by explicit DFS frames, so
    // deeply nested parser trees do not consume the Rust call stack. Ready is
    // still delivered in postorder, before insertion into the cloned parent.
    fn clone_subtree(&self, recursive: bool, template_to_form: bool) -> Option<Self> {
        struct Frame {
            source: CxfaNode,
            destination: CxfaNode,
            child: Option<CxfaNode>,
        }
        let document = self.document()?;
        let root = self.prepare_clone(&document, template_to_form)?;
        let mut frames = vec![Frame {
            source: self.clone(),
            destination: root,
            child: if recursive { self.first_child() } else { None },
        }];
        loop {
            if let Some(child) = frames.last()?.child.clone() {
                if template_to_form && child.get_packet_type() != XfaPacketType::Template {
                    frames.last_mut()?.child = child.next_sibling();
                    continue;
                }
                let destination = child.prepare_clone(&document, template_to_form)?;
                let next = child.first_child();
                frames.push(Frame {
                    source: child,
                    destination,
                    child: next,
                });
                continue;
            }
            let finished = frames.pop()?;
            finished.destination.set_initialized_flag_and_notify();
            if !template_to_form {
                finished.destination.set_binding_node(None);
            }
            let Some(parent) = frames.last() else {
                return Some(finished.destination);
            };
            if !parent
                .destination
                .insert_child_and_notify(-1, &finished.destination)
            {
                return None;
            }
            // Advance only after callbacks, preserving the source's observation
            // of changes to the original child's sibling links.
            frames.last_mut()?.child = finished.source.next_sibling();
        }
    }

    fn prepare_clone(&self, document: &Rc<CxfaDocument>, template_to_form: bool) -> Option<Self> {
        if template_to_form {
            if self.get_packet_type() != XfaPacketType::Template {
                return None;
            }
            let cloned = document.create_node(XfaPacketType::Form, self.get_element_type())?;
            cloned.set_template_node(Some(self));
            cloned.update_name_hash();
            if let Some(xml) = self.get_xml_mapping_node() {
                cloned.set_xml_mapping_node(xml);
            }
            return Some(cloned);
        }
        let cloned = document.create_node(self.get_packet_type(), self.get_element_type())?;
        self.merge_all_data(&cloned);
        cloned.update_name_hash();
        if self.is_need_saving_xml_node() {
            let xml_document = document.xml_document()?;
            let cloned_xml = if self.is_attribute_in_xml() {
                let name = self
                    .try_attribute(XfaAttribute::Name, false)
                    .unwrap_or_default();
                if name.is_empty() {
                    return None;
                }
                let value = self.get_cdata(XfaAttribute::Value);
                let mut xml_document = xml_document.borrow_mut();
                let xml = xml_document.create_element(&name);
                if !value.is_empty() {
                    xml.append_last_child(&xml_document.create_text(&value));
                }
                // This happens before mapping the detached element, just as in
                // the source; it must not write a "contains" XML attribute.
                cloned.set_enum(XfaAttribute::Contains, XfaAttributeValue::Unknown, false);
                xml
            } else {
                self.get_xml_mapping_node()?
                    .clone_into(&mut xml_document.borrow_mut())
            };
            cloned.set_xml_mapping_node(cloned_xml);
        }
        Some(cloned)
    }

    pub fn create_same_packet_node(&self, element: XfaElement) -> Option<Self> {
        let node = self
            .document()?
            .create_node(self.get_packet_type(), element)?;
        node.set_initialized_flag_and_notify();
        Some(node)
    }

    pub fn clone_template_to_form(&self, recursive: bool) -> Option<Self> {
        if !self.is_alive() || self.get_packet_type() != XfaPacketType::Template {
            return None;
        }
        self.clone_subtree(recursive, true)
    }

    pub fn get_binding_node(&self) -> Option<Self> {
        if !self.is_alive() {
            return None;
        }
        self.0.with_value(|data| {
            data.binding_nodes
                .iter()
                .find(|node| node.is_alive())
                .cloned()
        })
    }
    pub fn get_bind_data(&self) -> Option<Self> {
        if self.is_alive() && self.get_packet_type() == XfaPacketType::Form {
            self.get_binding_node()
        } else {
            None
        }
    }
    pub fn set_binding_node(&self, node: Option<&Self>) {
        self.0.with_value_mut(|data| {
            data.binding_nodes.clear();
            if let Some(node) = node.filter(|node| node.is_alive()) {
                data.binding_nodes.push(node.clone());
            }
            // Keep the discriminator consistent when replacing a multi-item
            // binding with one item or clearing it; the source leaves it stale.
            data.flags &= !(XfaNodeFlag::BindFormItems as u8);
        });
    }
    pub fn binds_form_items(&self) -> bool {
        self.has_flag(XfaNodeFlag::BindFormItems)
    }
    pub fn has_bind_items(&self) -> bool {
        self.is_alive()
            && self
                .0
                .with_value(|data| data.binding_nodes.iter().any(Self::is_alive))
    }
    pub fn get_bind_items_copy(&self) -> Vec<Self> {
        if !self.is_alive() {
            return Vec::new();
        }
        self.0.with_value(|data| {
            data.binding_nodes
                .iter()
                .filter(|node| node.is_alive())
                .cloned()
                .collect()
        })
    }
    pub fn add_bind_item(&self, node: &Self) {
        if !self.is_alive() || !node.is_alive() {
            return;
        }
        self.0.with_value_mut(|data| {
            data.binding_nodes.retain(Self::is_alive);
            if !data.binding_nodes.contains(node) {
                data.binding_nodes.push(node.clone());
            }
            if data.binding_nodes.len() > 1 {
                data.flags |= XfaNodeFlag::BindFormItems as u8;
            } else {
                data.flags &= !(XfaNodeFlag::BindFormItems as u8);
            }
        });
    }
    /// Returns whether a binding remains, including when the requested item
    /// was absent, matching RemoveBindItem's single/multiple-item transitions.
    pub fn remove_bind_item(&self, node: &Self) -> bool {
        if !self.is_alive() {
            return false;
        }
        self.0.with_value_mut(|data| {
            data.binding_nodes
                .retain(|bound| bound.is_alive() && bound != node);
            if data.binding_nodes.len() > 1 {
                data.flags |= XfaNodeFlag::BindFormItems as u8;
            } else {
                data.flags &= !(XfaNodeFlag::BindFormItems as u8);
            }
            !data.binding_nodes.is_empty()
        })
    }
    pub fn has_bind_item(&self) -> bool {
        self.is_alive()
            && self.get_packet_type() == XfaPacketType::Datasets
            && self.get_binding_node().is_some()
    }

    /// CJX_Object::MoveBufferMapData: pair children in source order, transfer
    /// same-class CalcData, then synchronize NodeV content in postorder.
    pub fn sync_buffer_map_data(&self, destination: &Self) -> bool {
        struct Frame {
            source: CxfaNode,
            destination: CxfaNode,
            source_child: Option<CxfaNode>,
            destination_child: Option<CxfaNode>,
        }
        fn frame(source: &CxfaNode, destination: &CxfaNode) -> Frame {
            Frame {
                source: source.clone(),
                destination: destination.clone(),
                source_child: source.first_child(),
                destination_child: destination.first_child(),
            }
        }
        if !self.is_alive() || !destination.is_alive() {
            return false;
        }
        let _source_document = self.document();
        let _destination_document = destination.document();
        let mut frames = vec![frame(self, destination)];
        loop {
            let current = frames.last().unwrap();
            if let (Some(source), Some(destination)) =
                (&current.source_child, &current.destination_child)
            {
                let child = frame(source, destination);
                frames.push(child);
                continue;
            }
            let finished = frames.pop().unwrap();
            if finished.source.get_element_type() == finished.destination.get_element_type()
                && !finished
                    .destination
                    .take_calculation_data_from(&finished.source)
            {
                return false;
            }
            if finished.destination.is_node_v() {
                let value = finished.destination.get_content();
                if !finished.destination.sync_value(&value, true) {
                    return false;
                }
            }
            let Some(parent) = frames.last_mut() else {
                return true;
            };
            parent.source_child = finished.source.next_sibling();
            parent.destination_child = finished.destination.next_sibling();
        }
    }
    pub fn get_data_description_node(&self) -> Option<Self> {
        if self.get_packet_type() == XfaPacketType::Datasets {
            self.get_template_node_if_exists()
        } else {
            None
        }
    }
    pub fn set_data_description_node(&self, node: Option<&Self>) -> bool {
        if self.get_packet_type() != XfaPacketType::Datasets {
            return false;
        }
        self.set_template_node(node);
        true
    }

    pub fn is_transparent(&self) -> bool {
        matches!(
            self.get_element_type(),
            XfaElement::SubformSet | XfaElement::Area | XfaElement::Proto
        ) || (self.is_unnamed() && self.is_container_node())
    }
    pub fn get_transparent_parent(&self) -> Option<Self> {
        let mut parent = self.parent();
        while let Some(node) = parent {
            if node.get_element_type() == XfaElement::Variables
                || (node.get_element_type() != XfaElement::SubformSet && !node.is_unnamed())
            {
                return Some(node);
            }
            parent = node.parent();
        }
        None
    }
    pub fn get_one_child_named(&self, name: &[u32]) -> Option<Self> {
        find_first_sibling_named(self, hash_name(name))
    }
    pub fn get_one_child_of_class(&self, name: &[u32]) -> Option<Self> {
        let element = get_element_by_name(name);
        if element == XfaElement::Unknown {
            return None;
        }
        find_first_sibling_of_class(self, element)
    }
    pub fn get_siblings(&self, is_class_name: bool) -> Vec<Self> {
        let Some(mut parent) = self.parent() else {
            return Vec::new();
        };
        if !parent.has_property(self.get_element_type()) {
            let Some(transparent_parent) = self.get_transparent_parent() else {
                return Vec::new();
            };
            parent = transparent_parent;
        }
        let hash = if is_class_name {
            self.get_class_hash_code()
        } else {
            self.get_name_hash()
        };
        let mut siblings = Vec::new();
        traverse_properties_or_siblings(&parent, hash, &mut siblings, is_class_name);
        siblings
    }
    pub fn get_index(&self, is_property: bool, is_class_index: bool) -> usize {
        let Some(mut parent) = self.parent() else {
            return 0;
        };
        if !is_property {
            let Some(transparent_parent) = self.get_transparent_parent() else {
                return 0;
            };
            parent = transparent_parent;
        }
        let hash = if is_class_index {
            self.get_class_hash_code()
        } else {
            self.get_name_hash()
        };
        let mut siblings = Vec::new();
        traverse_properties_or_siblings(&parent, hash, &mut siblings, is_class_index);
        siblings.iter().position(|node| node == self).unwrap_or(0)
    }
    pub fn get_index_by_name(&self) -> usize {
        self.get_index(self.is_property(), false)
    }
    pub fn get_index_by_class_name(&self) -> usize {
        self.get_index(self.is_property(), true)
    }
    pub fn get_name_expression(&self) -> Vec<u32> {
        // Accumulate segments then emit them once, avoiding repeated copies of
        // a growing expression while retaining native segment/index semantics.
        let mut segments = Vec::new();
        let mut node = Some(self.clone());
        while let Some(current) = node {
            segments.push(name_expression_single_path(&current));
            node = current.parent();
        }
        let length = segments
            .iter()
            .map(Vec::len)
            .sum::<usize>()
            .saturating_add(segments.len().saturating_sub(1));
        let mut result = Vec::with_capacity(length);
        for (index, segment) in segments.iter().rev().enumerate() {
            if index != 0 {
                result.push(u32::from(b'.'));
            }
            result.extend_from_slice(segment);
        }
        result
    }
    pub fn get_instance_mgr_of_subform(&self) -> Option<Self> {
        if self.get_packet_type() != XfaPacketType::Form {
            return None;
        }
        let parent = self.parent()?;
        if parent.get_element_type() == XfaElement::Area {
            return None;
        }
        let mut sibling = self.prev_sibling();
        while let Some(node) = sibling {
            let element = node.get_element_type();
            if matches!(element, XfaElement::Subform | XfaElement::SubformSet)
                && node.get_name_hash() != self.get_name_hash()
            {
                break;
            }
            if element == XfaElement::InstanceManager {
                let name = self.get_cdata(XfaAttribute::Name);
                let manager_name = node.get_cdata(XfaAttribute::Name);
                return manager_name
                    .strip_prefix(&[u32::from(b'_')])
                    .filter(|suffix| *suffix == name)
                    .map(|_| node);
            }
            sibling = node.prev_sibling();
        }
        None
    }
    pub fn get_occur_if_exists(&self) -> Option<Self> {
        self.get_first_child_by_class(XfaElement::Occur)
    }
}

fn find_first_sibling_named(parent: &CxfaNode, hash: u32) -> Option<CxfaNode> {
    find_first_descendant(parent, |child| child.get_name_hash() == hash)
}
fn find_first_sibling_of_class(parent: &CxfaNode, element: XfaElement) -> Option<CxfaNode> {
    find_first_descendant(parent, |child| child.get_element_type() == element)
}
fn find_first_descendant(
    parent: &CxfaNode,
    matches: impl Fn(&CxfaNode) -> bool,
) -> Option<CxfaNode> {
    struct Frame {
        parent: CxfaNode,
        nodes: std::vec::IntoIter<CxfaNode>,
        children: bool,
    }
    fn frame(parent: &CxfaNode) -> Frame {
        Frame {
            parent: parent.clone(),
            nodes: parent
                .get_node_list_with_filter(XFA_NODE_FILTER_PROPERTIES)
                .into_iter(),
            children: false,
        }
    }
    let mut frames = vec![frame(parent)];
    while let Some(current) = frames.last_mut() {
        if let Some(child) = current.nodes.next() {
            if matches(&child) {
                return Some(child);
            }
            frames.push(frame(&child));
        } else if !current.children {
            let mut children = current
                .parent
                .get_node_list_with_filter(XFA_NODE_FILTER_CHILDREN);
            // Variables/PageSet are also exposed as children, but their whole
            // subtrees were searched in the property phase already. Avoid
            // repeating that work (exponential for nested PageSet properties).
            children.retain(|child| {
                !current.parent.has_property(child.get_element_type())
                    || !matches!(
                        child.get_element_type(),
                        XfaElement::Variables | XfaElement::PageSet
                    )
            });
            current.nodes = children.into_iter();
            current.children = true;
        } else {
            frames.pop();
        }
    }
    None
}
fn traverse_siblings(
    parent: &CxfaNode,
    hash: u32,
    siblings: &mut Vec<CxfaNode>,
    is_class_name: bool,
) {
    let mut frames = vec![parent
        .get_node_list_with_filter(XFA_NODE_FILTER_CHILDREN)
        .into_iter()];
    while let Some(current) = frames.last_mut() {
        let Some(child) = current.next() else {
            frames.pop();
            continue;
        };
        if child.get_element_type() == XfaElement::Variables {
            continue;
        }
        if (if is_class_name {
            child.get_class_hash_code()
        } else {
            child.get_name_hash()
        }) == hash
        {
            siblings.push(child.clone());
        }
        if child.is_transparent() && child.get_element_type() != XfaElement::PageSet {
            frames.push(
                child
                    .get_node_list_with_filter(XFA_NODE_FILTER_CHILDREN)
                    .into_iter(),
            );
        }
    }
}
fn traverse_properties_or_siblings(
    parent: &CxfaNode,
    hash: u32,
    siblings: &mut Vec<CxfaNode>,
    is_class_name: bool,
) {
    for child in parent.get_node_list_with_filter(XFA_NODE_FILTER_PROPERTIES) {
        if is_class_name {
            if child.get_class_hash_code() == hash {
                siblings.push(child.clone());
            }
        } else if child.get_name_hash() == hash
            && !matches!(
                child.get_element_type(),
                XfaElement::PageSet | XfaElement::Extras | XfaElement::Items
            )
        {
            siblings.push(child.clone());
        }
        if child.is_unnamed() && child.get_element_type() == XfaElement::PageSet {
            traverse_siblings(&child, hash, siblings, is_class_name);
        }
    }
    if siblings.is_empty() {
        traverse_siblings(parent, hash, siblings, is_class_name);
    }
}
fn name_expression_single_path(node: &CxfaNode) -> Vec<u32> {
    let is_property = node.is_property();
    let is_class_index =
        node.is_unnamed() || (is_property && node.get_element_type() != XfaElement::PageSet);
    let mut result = Vec::new();
    if is_class_index {
        result.push(u32::from(b'#'));
        result.extend(wide(node.get_class_name()));
    } else {
        for unit in node.get_cdata(XfaAttribute::Name) {
            if unit == u32::from(b'.') {
                result.push(u32::from(b'\\'));
            }
            result.push(unit);
        }
    }
    result.push(u32::from(b'['));
    result.extend(wide(
        &node.get_index(is_property, is_class_index).to_string(),
    ));
    result.push(u32::from(b']'));
    result
}

#[cfg(test)]
mod tests {
    use super::super::super::cxfa_document::XfaDocumentNotify;
    use super::*;
    use crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument;
    use std::cell::RefCell;

    fn node(
        doc: &Rc<CxfaDocument>,
        packet: XfaPacketType,
        element: XfaElement,
        name: &str,
    ) -> CxfaNode {
        let node = doc.create_node(packet, element).unwrap();
        node.set_cdata(XfaAttribute::Name, &wide(name));
        node
    }
    fn append(parent: &CxfaNode, child: &CxfaNode) {
        assert!(parent.insert_child_and_notify(-1, child));
    }

    #[test]
    fn clone_copies_only_explicit_typed_and_custom_storage_and_resets_bindings() {
        let doc = CxfaDocument::new();
        let source = node(&doc, XfaPacketType::Form, XfaElement::Field, "source");
        let prototype = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Field,
            "prototype",
        );
        prototype.set_cdata(XfaAttribute::Locale, &wide("de_DE"));
        source.set_template_node(Some(&prototype));
        source.set_boolean(XfaAttribute::IsNull, true, false);
        source.set_integer(XfaAttribute::ColSpan, 3, false);
        source.set_enum(XfaAttribute::Presence, XfaAttributeValue::Invisible, false);
        source.set_measure(
            XfaAttribute::H,
            CxfaMeasurement::new(2., XfaUnit::In),
            false,
        );
        source.set_attribute_by_string(&wide("custom-key"), &wide("custom-value"));
        source.set_flag(XfaNodeFlag::UnusedNode);
        source.set_flag(XfaNodeFlag::UserInteractive);
        let data = node(&doc, XfaPacketType::Datasets, XfaElement::DataValue, "data");
        source.set_binding_node(Some(&data));
        let ui = node(&doc, XfaPacketType::Form, XfaElement::Ui, "");
        let edit = node(&doc, XfaPacketType::Form, XfaElement::TextEdit, "");
        append(&source, &ui);
        append(&ui, &edit);
        let shallow = source.clone_node(false).unwrap();
        assert_ne!(source, shallow);
        assert!(shallow.parent().is_none());
        assert!(shallow.first_child().is_none());
        assert!(shallow.is_initialized());
        assert!(!shallow.is_unused_node());
        assert!(!shallow.is_user_interactive());
        assert!(shallow.get_bind_data().is_none());
        assert!(shallow.get_template_node_if_exists().is_none());
        assert_eq!(shallow.get_name_hash(), hash_name(&wide("source")));
        assert!(shallow.get_boolean(XfaAttribute::IsNull));
        assert_eq!(shallow.get_integer(XfaAttribute::ColSpan), 3);
        assert_eq!(
            shallow.get_enum(XfaAttribute::Presence),
            XfaAttributeValue::Invisible
        );
        assert_eq!(
            shallow.get_measure_in_unit(XfaAttribute::H, XfaUnit::Pt),
            144.
        );
        assert_eq!(
            shallow.get_attribute_by_string(&wide("custom-key")),
            wide("custom-value")
        );
        assert!(shallow.get_cdata(XfaAttribute::Locale).is_empty());
        let deep = source.clone_node(true).unwrap();
        let cloned_ui = deep.first_child().unwrap();
        assert_ne!(cloned_ui, ui);
        assert_eq!(cloned_ui.get_element_type(), XfaElement::Ui);
        assert_eq!(
            cloned_ui.first_child().unwrap().get_element_type(),
            XfaElement::TextEdit
        );
        assert!(cloned_ui.is_initialized());
        source.set_cdata(XfaAttribute::Name, &wide("changed"));
        assert_eq!(deep.get_cdata(XfaAttribute::Name), wide("source"));
        let destination = node(&doc, XfaPacketType::Form, XfaElement::Field, "old");
        destination.set_cdata(XfaAttribute::Id, &wide("retained"));
        assert!(source.merge_all_data(&destination));
        assert_eq!(destination.get_cdata(XfaAttribute::Name), wide("changed"));
        assert_eq!(destination.get_cdata(XfaAttribute::Id), wide("retained"));
        assert!(destination.merge_all_data(&destination));
        assert!(!source.merge_all_data(&ui));
    }

    #[test]
    fn template_clone_inherits_attributes_shares_mapping_and_skips_non_template_children() {
        let doc = CxfaDocument::new();
        let xml = Rc::new(RefCell::new(CfxXmlDocument::new()));
        doc.set_xml_document(xml.clone());
        let template = node(&doc, XfaPacketType::Template, XfaElement::Subform, "root");
        let mapping = xml.borrow_mut().create_element(&wide("subform"));
        template.set_xml_mapping_node(mapping.clone());
        template.set_cdata(XfaAttribute::Locale, &wide("en_US"));
        let field = node(&doc, XfaPacketType::Template, XfaElement::Field, "entry");
        append(&template, &field);
        let other = node(&doc, XfaPacketType::Form, XfaElement::Field, "other");
        append(&template, &other);
        let shallow = template.clone_template_to_form(false).unwrap();
        assert!(shallow.first_child().is_none());
        assert_eq!(shallow.get_packet_type(), XfaPacketType::Form);
        assert_eq!(
            shallow.get_template_node_if_exists(),
            Some(template.clone())
        );
        assert_eq!(shallow.get_xml_mapping_node(), Some(mapping));
        assert_eq!(shallow.get_cdata(XfaAttribute::Name), wide("root"));
        assert!(!shallow.has_local_attribute(XfaAttribute::Name));
        assert!(shallow.is_initialized());
        let form = template.clone_template_to_form(true).unwrap();
        assert_eq!(form.count_children(XfaElement::Unknown, false), 1);
        let form_field = form.first_child().unwrap();
        assert_eq!(form_field.get_packet_type(), XfaPacketType::Form);
        assert_eq!(form_field.get_template_node_if_exists(), Some(field));
        assert_eq!(form_field.get_name_hash(), hash_name(&wide("entry")));
        template.set_cdata(XfaAttribute::Locale, &wide("fr_FR"));
        assert_eq!(form.get_cdata(XfaAttribute::Locale), wide("fr_FR"));
        form.set_cdata(XfaAttribute::Locale, &wide("de_DE"));
        assert_eq!(template.get_cdata(XfaAttribute::Locale), wide("fr_FR"));
        assert_eq!(form.get_cdata(XfaAttribute::Locale), wide("de_DE"));
        assert!(form.clone_template_to_form(true).is_none());
        let created = template.create_same_packet_node(XfaElement::Field).unwrap();
        assert_eq!(created.get_packet_type(), XfaPacketType::Template);
        assert!(created.is_initialized());
        assert!(template
            .create_same_packet_node(XfaElement::DataValue)
            .is_none());
    }

    #[test]
    fn data_clone_copies_xml_and_metadata_becomes_an_independent_element() {
        let doc = CxfaDocument::new();
        let xml = Rc::new(RefCell::new(CfxXmlDocument::new()));
        doc.set_xml_document(xml);
        let group = node(
            &doc,
            XfaPacketType::Datasets,
            XfaElement::DataGroup,
            "group",
        );
        let group_xml = group.create_xml_mapping_node().unwrap();
        let data = node(
            &doc,
            XfaPacketType::Datasets,
            XfaElement::DataValue,
            "child",
        );
        let data_xml = data.create_xml_mapping_node().unwrap();
        data.set_cdata(XfaAttribute::Value, &wide("original"));
        append(&group, &data);
        let cloned = group.clone_node(true).unwrap();
        let cloned_xml = cloned.get_xml_mapping_node().unwrap();
        let cloned_data = cloned.first_child().unwrap();
        let cloned_data_xml = cloned_data.get_xml_mapping_node().unwrap();
        assert_ne!(cloned_xml, group_xml);
        assert_ne!(cloned_data_xml, data_xml);
        assert_eq!(cloned_data_xml.parent(), Some(cloned_xml));
        assert_eq!(cloned_data_xml.text_data(), wide("original"));
        cloned_data.set_cdata(XfaAttribute::Value, &wide("independent"));
        assert_eq!(data_xml.text_data(), wide("original"));
        let metadata = node(&doc, XfaPacketType::Datasets, XfaElement::DataValue, "meta");
        metadata.set_cdata(XfaAttribute::QualifiedName, &wide("p:meta"));
        metadata.set_cdata(XfaAttribute::Value, &wide("attribute-value"));
        metadata.set_enum(XfaAttribute::Contains, XfaAttributeValue::MetaData, false);
        append(&group, &metadata);
        metadata.set_xml_mapping_node(group_xml.clone());
        let cloned_metadata = metadata.clone_node(true).unwrap();
        let mapped = cloned_metadata.get_xml_mapping_node().unwrap();
        assert_eq!(mapped.name().as_ref(), wide("meta"));
        assert_eq!(mapped.text_data(), wide("attribute-value"));
        assert!(mapped.parent().is_none());
        assert!(!mapped.has_attribute(&wide("contains")));
        assert!(!cloned_metadata.is_attribute_in_xml());
        assert!(metadata.is_attribute_in_xml());
        assert_eq!(metadata.get_xml_mapping_node(), Some(group_xml));
    }

    #[test]
    fn binding_single_multiple_duplicate_and_removal_transitions_preserve_bound_xml() {
        let doc = CxfaDocument::new();
        let xml = Rc::new(RefCell::new(CfxXmlDocument::new()));
        doc.set_xml_document(xml);
        let data = node(&doc, XfaPacketType::Datasets, XfaElement::DataValue, "data");
        let first = node(&doc, XfaPacketType::Form, XfaElement::Field, "first");
        let second = node(&doc, XfaPacketType::Form, XfaElement::Field, "second");
        let absent = node(&doc, XfaPacketType::Form, XfaElement::Field, "absent");
        assert!(!data.has_bind_item());
        first.set_binding_node(Some(&data));
        data.add_bind_item(&first);
        assert_eq!(first.get_bind_data(), Some(data.clone()));
        assert!(data.has_bind_item());
        assert!(data.has_bind_items());
        assert!(!data.binds_form_items());
        data.add_bind_item(&first);
        assert_eq!(data.get_bind_items_copy(), vec![first.clone()]);
        data.add_bind_item(&second);
        assert!(data.binds_form_items());
        let snapshot = data.get_bind_items_copy();
        assert_eq!(snapshot, vec![first.clone(), second.clone()]);
        assert!(data.remove_bind_item(&absent));
        assert!(data.remove_bind_item(&first));
        assert!(!data.binds_form_items());
        assert_eq!(data.get_bind_items_copy(), vec![second.clone()]);
        assert_eq!(snapshot.len(), 2);
        assert!(!data.remove_bind_item(&second));
        assert!(!data.has_bind_items());
        assert!(!data.remove_bind_item(&absent));
        data.add_bind_item(&first);
        data.add_bind_item(&second);
        data.set_binding_node(Some(&first));
        assert_eq!(data.get_bind_items_copy(), vec![first.clone()]);
        assert!(!data.binds_form_items());
        data.set_binding_node(None);
        assert!(!data.has_bind_items());
        assert!(!data.binds_form_items());
        let group = node(
            &doc,
            XfaPacketType::Datasets,
            XfaElement::DataGroup,
            "group",
        );
        let group_xml = group.create_xml_mapping_node().unwrap();
        let data_xml = data.create_xml_mapping_node().unwrap();
        append(&group, &data);
        data.add_bind_item(&first);
        assert!(group.set_to_xml(&wide("tail")));
        assert_eq!(group_xml.first_child(), Some(data_xml.clone()));
        assert_eq!(group_xml.text_data(), wide("tail"));
        data.remove_bind_item(&first);
        assert!(group.set_to_xml(&wide("replacement")));
        assert_eq!(data_xml.parent(), None);
        assert_eq!(group_xml.text_data(), wide("replacement"));
        assert!(data.set_data_description_node(Some(&group)));
        assert_eq!(data.get_data_description_node(), Some(group));
        assert!(!first.set_data_description_node(Some(&data)));
        assert_eq!(first.get_data_description_node(), None);
    }

    #[test]
    fn som_lookup_prefers_properties_and_indexes_through_transparent_siblings() {
        let doc = CxfaDocument::new();
        let root = node(&doc, XfaPacketType::Form, XfaElement::Field, "root");
        let direct = node(&doc, XfaPacketType::Form, XfaElement::Field, "target");
        append(&root, &direct);
        let ui = node(&doc, XfaPacketType::Form, XfaElement::Ui, "");
        append(&root, &ui);
        let property_child = node(&doc, XfaPacketType::Form, XfaElement::TextEdit, "target");
        append(&ui, &property_child);
        let direct_edit = node(&doc, XfaPacketType::Form, XfaElement::TextEdit, "target");
        append(&root, &direct_edit);
        assert_eq!(
            root.get_one_child_named(&wide("target")),
            Some(property_child.clone())
        );
        assert_eq!(
            root.get_one_child_of_class(&wide("textEdit")),
            Some(property_child)
        );
        assert_eq!(root.get_first_child_by_name(&wide("target")), Some(direct));
        assert!(root
            .get_one_child_of_class(&wide("invalid-class"))
            .is_none());
        let outer = node(&doc, XfaPacketType::Form, XfaElement::Subform, "outer");
        let first = node(&doc, XfaPacketType::Form, XfaElement::Field, "item");
        append(&outer, &first);
        let transparent = node(&doc, XfaPacketType::Form, XfaElement::Subform, "");
        append(&outer, &transparent);
        let second = node(&doc, XfaPacketType::Form, XfaElement::Field, "item");
        append(&transparent, &second);
        let set = node(&doc, XfaPacketType::Form, XfaElement::SubformSet, "set");
        append(&outer, &set);
        let third = node(&doc, XfaPacketType::Form, XfaElement::Field, "item");
        append(&set, &third);
        let variables = node(&doc, XfaPacketType::Form, XfaElement::Variables, "vars");
        append(&outer, &variables);
        let excluded = node(&doc, XfaPacketType::Form, XfaElement::Field, "item");
        append(&variables, &excluded);
        assert!(transparent.is_transparent());
        assert!(set.is_transparent());
        assert_eq!(second.get_transparent_parent(), Some(outer.clone()));
        assert_eq!(third.get_transparent_parent(), Some(outer));
        assert_eq!(excluded.get_transparent_parent(), Some(variables));
        assert_eq!(
            second.get_siblings(false),
            vec![first.clone(), second.clone(), third.clone()]
        );
        assert_eq!(first.get_index_by_name(), 0);
        assert_eq!(second.get_index_by_name(), 1);
        assert_eq!(third.get_index_by_name(), 2);
        assert_eq!(third.get_index_by_class_name(), 2);
    }

    #[test]
    fn som_expression_escapes_dots_and_instance_lookup_observes_name_barriers() {
        let doc = CxfaDocument::new();
        let root = node(&doc, XfaPacketType::Form, XfaElement::Subform, "root");
        let field = node(&doc, XfaPacketType::Form, XfaElement::Field, "a.b");
        append(&root, &field);
        let other = node(&doc, XfaPacketType::Form, XfaElement::Field, "a.b");
        append(&root, &other);
        let ui = node(&doc, XfaPacketType::Form, XfaElement::Ui, "");
        append(&other, &ui);
        assert_eq!(ui.get_name_expression(), wide("root[0].a\\.b[1].#ui[0]"));
        let manager = node(
            &doc,
            XfaPacketType::Form,
            XfaElement::InstanceManager,
            "_item",
        );
        append(&root, &manager);
        let first = node(&doc, XfaPacketType::Form, XfaElement::Subform, "item");
        append(&root, &first);
        let second = node(&doc, XfaPacketType::Form, XfaElement::Subform, "item");
        append(&root, &second);
        assert_eq!(first.get_instance_mgr_of_subform(), Some(manager.clone()));
        assert_eq!(second.get_instance_mgr_of_subform(), Some(manager.clone()));
        let occur = manager.create_same_packet_node(XfaElement::Occur).unwrap();
        append(&manager, &occur);
        assert_eq!(manager.get_occur_if_exists(), Some(occur));
        let barrier = node(&doc, XfaPacketType::Form, XfaElement::Subform, "different");
        append(&root, &barrier);
        let last = node(&doc, XfaPacketType::Form, XfaElement::Subform, "item");
        append(&root, &last);
        assert!(last.get_instance_mgr_of_subform().is_none());
        let area = node(&doc, XfaPacketType::Form, XfaElement::Area, "area");
        let area_manager = node(
            &doc,
            XfaPacketType::Form,
            XfaElement::InstanceManager,
            "_item",
        );
        append(&area, &area_manager);
        let area_child = node(&doc, XfaPacketType::Form, XfaElement::Subform, "item");
        append(&area, &area_child);
        assert!(area_child.get_instance_mgr_of_subform().is_none());
        assert!(manager.clone_template_to_form(true).is_none());
    }

    struct Notify {
        events: RefCell<Vec<(char, Vec<u32>)>>,
    }
    impl XfaDocumentNotify for Notify {
        fn on_child_added(&self, _parent: &CxfaNode, child: &CxfaNode) {
            self.events
                .borrow_mut()
                .push(('A', child.get_cdata(XfaAttribute::Name)));
            assert!(child.is_alive());
        }
        fn on_child_removed(&self, _parent: &CxfaNode, _child: &CxfaNode) {}
        fn on_attribute_changed(&self, node: &CxfaNode, attr: XfaAttribute) {
            assert!(node.has_local_attribute(attr));
        }
        fn on_node_ready(&self, node: &CxfaNode) {
            assert!(!node.is_initialized());
            node.set_initialized_flag_and_notify();
            self.events
                .borrow_mut()
                .push(('R', node.get_cdata(XfaAttribute::Name)));
        }
    }
    #[test]
    fn cloning_delivers_ready_postorder_and_allows_reentrant_node_observation() {
        let notify = Rc::new(Notify {
            events: RefCell::new(Vec::new()),
        });
        let doc = CxfaDocument::with_notify(Some(notify.clone()));
        let source = node(&doc, XfaPacketType::Template, XfaElement::Subform, "parent");
        let child = node(&doc, XfaPacketType::Template, XfaElement::Field, "child");
        append(&source, &child);
        notify.events.borrow_mut().clear();
        let cloned = source.clone_node(true).unwrap();
        assert_eq!(
            *notify.events.borrow(),
            vec![
                ('R', wide("child")),
                ('A', wide("child")),
                ('R', wide("parent"))
            ]
        );
        assert!(cloned.is_initialized());
        assert!(cloned.first_child().unwrap().is_initialized());
        notify.events.borrow_mut().clear();
        source.clone_template_to_form(true).unwrap();
        assert_eq!(
            *notify.events.borrow(),
            vec![
                ('R', wide("child")),
                ('A', wide("child")),
                ('R', wide("parent"))
            ]
        );
    }

    #[test]
    fn thousand_level_clone_and_som_search_use_explicit_traversal_stacks() {
        let doc = CxfaDocument::new();
        let root = node(&doc, XfaPacketType::Template, XfaElement::Subform, "root");
        let mut parent = root.clone();
        for _ in 0..1000 {
            let child = node(&doc, XfaPacketType::Template, XfaElement::Subform, "");
            append(&parent, &child);
            parent = child;
        }
        parent.set_cdata(XfaAttribute::Name, &wide("needle"));
        assert_eq!(
            root.get_one_child_named(&wide("needle")),
            Some(parent.clone())
        );
        assert!(root.get_one_child_named(&wide("absent")).is_none());
        assert_eq!(parent.get_index_by_name(), 0);
        for cloned in [
            root.clone_node(true).unwrap(),
            root.clone_template_to_form(true).unwrap(),
        ] {
            let mut current = cloned;
            for _ in 0..1000 {
                assert!(current.is_initialized());
                current = current.first_child().unwrap();
            }
            assert_eq!(current.get_cdata(XfaAttribute::Name), wide("needle"));
            assert!(current.is_initialized());
            assert!(current.first_child().is_none());
        }
    }
}
