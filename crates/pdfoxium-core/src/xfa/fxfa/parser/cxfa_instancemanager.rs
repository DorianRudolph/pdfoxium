// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Instance-manager constructor behavior from `cxfa_instancemanager.{h,cpp}`
//! uses the shared source schema (Form-only/Name/Occur). This module ports the
//! connected CXFA_Node GetCount/GetItemIfExists/CreateInstanceIfPossible,
//! InsertItem/RemoveItem and ReorderDataNodes algorithms. Hosts own FF init,
//! index events and layout scheduling; these model operations do not fake them.
use super::{
    cxfa_document_datamerge::DataMergeError, cxfa_node::CxfaNode,
    cxfa_traversestrategy_xfacontainernode::CxfaContainerIterator,
};
use crate::{
    fxjs::xfa::cfxjse_resolveprocessor::CfxJseResolveProcessor,
    xfa::fxfa::fxfa_basic::{XfaAttribute, XfaElement, XfaHashCode, XfaPacketType},
};
use std::collections::{HashMap, HashSet};

fn instance_nodes(manager: &CxfaNode) -> impl Iterator<Item = CxfaNode> {
    let valid = manager.is_alive() && manager.get_element_type() == XfaElement::InstanceManager;
    let name = if valid {
        manager.get_cdata(XfaAttribute::Name)
    } else {
        Vec::new()
    };
    let mut current = if valid { manager.next_sibling() } else { None };
    let mut hash = None;
    std::iter::from_fn(move || loop {
        let node = current.take()?;
        current = node.next_sibling();
        let kind = node.get_element_type();
        if kind == XfaElement::InstanceManager {
            current = None;
            return None;
        }
        if !matches!(kind, XfaElement::Subform | XfaElement::SubformSet) {
            continue;
        }
        if hash.is_none() {
            if name.first() != Some(&u32::from(b'_'))
                || name[1..] != node.get_cdata(XfaAttribute::Name)
            {
                current = None;
                return None;
            }
            hash = Some(node.get_name_hash());
        }
        if hash != Some(node.get_name_hash()) {
            current = None;
            return None;
        }
        return Some(node);
    })
}
fn bound_data_nodes(root: Option<&CxfaNode>) -> HashSet<CxfaNode> {
    let mut nodes = HashSet::new();
    let mut iterator = CxfaContainerIterator::new(root.cloned());
    while let Some(node) = iterator.get_current() {
        if let Some(data) = node.get_bind_data() {
            nodes.insert(data);
        }
        iterator.move_to_next();
    }
    nodes
}
fn sorted_nodes(parent: &CxfaNode, set: &HashSet<CxfaNode>) -> Vec<CxfaNode> {
    let mut result = Vec::with_capacity(set.len());
    let mut child = parent.first_child();
    while let Some(node) = child {
        if set.contains(&node) {
            result.push(node.clone());
        }
        child = node.next_sibling();
    }
    result
}
pub fn reorder_data_nodes(
    first: &HashSet<CxfaNode>,
    second: &HashSet<CxfaNode>,
    insert_before: bool,
) -> bool {
    type Pair = (HashSet<CxfaNode>, HashSet<CxfaNode>);
    let mut groups: HashMap<(CxfaNode, u32), Pair> = HashMap::new();
    for node in first {
        if let Some(parent) = node.parent() {
            let hash = node.get_name_hash();
            if hash != 0 {
                groups
                    .entry((parent, hash))
                    .or_default()
                    .0
                    .insert(node.clone());
            }
        }
    }
    for node in second {
        if let Some(parent) = node.parent() {
            let hash = node.get_name_hash();
            if hash != 0 {
                let pair = groups.entry((parent, hash)).or_default();
                if !pair.0.remove(node) {
                    pair.1.insert(node.clone());
                }
            }
        }
    }
    for ((parent, _), (first, second)) in groups {
        if first.is_empty() || second.is_empty() {
            continue;
        }
        let moving = sorted_nodes(&parent, &first);
        let anchors = sorted_nodes(&parent, &second);
        let mut anchor = if insert_before {
            anchors.first().cloned()
        } else {
            anchors.last().and_then(CxfaNode::next_sibling)
        };
        // Already-adjacent moving nodes are not valid insertion anchors after
        // removal. Skip them to keep a no-op reorder safe and stable.
        if !insert_before {
            while let Some(node) = anchor.as_ref().filter(|node| first.contains(*node)) {
                anchor = node.next_sibling();
            }
        }
        let Some(anchor) = anchor else {
            // An absent successor means append after the last anchor, not no work.
            if !insert_before && !anchors.is_empty() {
                for node in moving {
                    if node.parent().as_ref() != Some(&parent)
                        || !parent.remove_child_and_notify(&node, true)
                        || !parent.insert_child_before_and_notify(&node, None)
                    {
                        return false;
                    }
                }
                continue;
            }
            return false;
        };
        for node in moving {
            if node.parent().as_ref() != Some(&parent)
                || !parent.remove_child_and_notify(&node, true)
                || !parent.insert_child_before_and_notify(&node, Some(&anchor))
            {
                return false;
            }
        }
    }
    true
}
impl CxfaNode {
    pub fn get_instance_index(&self) -> i32 {
        let mut count = 0i32;
        let mut sibling = self.prev_sibling();
        while let Some(node) = sibling {
            if !matches!(
                node.get_element_type(),
                XfaElement::Subform | XfaElement::SubformSet
            ) {
                break;
            }
            count = count.saturating_add(1);
            sibling = node.prev_sibling();
        }
        count
    }
    pub fn get_count(&self) -> i32 {
        i32::try_from(instance_nodes(self).count()).unwrap_or(i32::MAX)
    }
    pub fn get_item_if_exists(&self, index: i32) -> Option<Self> {
        usize::try_from(index)
            .ok()
            .and_then(|index| instance_nodes(self).nth(index))
    }
    pub fn insert_item(
        &self,
        new_instance: &Self,
        position: i32,
        count: i32,
        move_data: bool,
    ) -> bool {
        if !self.is_alive()
            || self.get_element_type() != XfaElement::InstanceManager
            || !new_instance.is_alive()
            || new_instance.get_packet_type() != XfaPacketType::Form
            || !matches!(
                new_instance.get_element_type(),
                XfaElement::Subform | XfaElement::SubformSet
            )
        {
            return false;
        }
        let _store = self.retain_store();
        let Some(parent) = self.parent() else {
            return false;
        };
        let count = if count < 0 { self.get_count() } else { count };
        let position = if position < 0 { count } else { position };
        if position > count || position < 0 || count != self.get_count() {
            return false;
        }
        let predecessor = if position == count && count > 0 {
            self.get_item_if_exists(count - 1)
        } else {
            None
        };
        let anchor = if position == count {
            if let Some(previous) = &predecessor {
                previous.next_sibling()
            } else {
                self.next_sibling()
            }
        } else {
            self.get_item_if_exists(position)
        };
        if position < count && anchor.is_none() {
            return false;
        }
        if !parent.insert_child_before_and_notify(new_instance, anchor.as_ref()) {
            return false;
        }
        if !move_data {
            return true;
        }
        let moving = bound_data_nodes(Some(new_instance));
        // For append, order data after the preceding managed instance. The source
        // looks at the following sibling, leaving moves-to-end data out of order.
        let reference = if position == count {
            predecessor.as_ref()
        } else {
            anchor.as_ref()
        };
        reorder_data_nodes(&moving, &bound_data_nodes(reference), position != count)
    }
    pub fn remove_item(&self, instance: &Self, remove_data_binding: bool) -> bool {
        if !self.is_alive() || self.get_element_type() != XfaElement::InstanceManager {
            return false;
        }
        let _store = self.retain_store();
        let Some(parent) = self.parent() else {
            return false;
        };
        if !parent.remove_child_and_notify(instance, true) {
            return false;
        }
        if !remove_data_binding {
            return true;
        }
        let mut iterator = CxfaContainerIterator::new(Some(instance.clone()));
        while let Some(form) = iterator.get_current() {
            if let Some(data) = form.get_bind_data() {
                if !data.remove_bind_item(&form) {
                    if let Some(parent) = data.parent() {
                        parent.remove_child_and_notify(&data, true);
                    }
                }
                form.set_binding_node(None);
            }
            iterator.move_to_next();
        }
        true
    }
    pub fn create_instance_if_possible(
        &self,
        data_merge: bool,
    ) -> Result<Option<Self>, DataMergeError> {
        let Some(document) = self.document() else {
            return Ok(None);
        };
        let resolver = CfxJseResolveProcessor::new(document);
        self.create_instance_if_possible_with_resolver(data_merge, &resolver)
    }
    pub fn create_instance_if_possible_with_resolver(
        &self,
        data_merge: bool,
        resolver: &CfxJseResolveProcessor,
    ) -> Result<Option<Self>, DataMergeError> {
        if !self.is_alive() || self.get_element_type() != XfaElement::InstanceManager {
            return Ok(None);
        }
        let Some(document) = self.document() else {
            return Ok(None);
        };
        let Some(template) = self.get_template_node_if_exists() else {
            return Ok(None);
        };
        let Some(parent) = self.parent() else {
            return Ok(None);
        };
        let mut scope = None;
        let mut bound = Some(parent.clone());
        while let Some(node) = bound {
            if !node.is_container_node() {
                break;
            }
            if let Some(data) = node.get_bind_data() {
                scope = Some(data);
                break;
            }
            bound = node.parent();
        }
        let scope = scope
            .or_else(|| document.get_xfa_node(XfaHashCode::Record as u32))
            .ok_or(DataMergeError::InvalidTree(
                "instance creation requires a bound data scope or record",
            ))?;
        let instance = document.data_merge_copy_container_with_resolver(
            resolver,
            &template,
            &parent,
            Some(&scope),
            true,
            data_merge,
            true,
        )?;
        if let Some(instance) = &instance {
            document.data_merge_update_binding_relations_with_resolver(resolver, instance)?;
            if !parent.remove_child_and_notify(instance, true) {
                return Err(DataMergeError::InvalidTree(
                    "new instance was moved during initialization",
                ));
            }
        }
        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder};
    use super::*;
    use crate::fxcrt::{
        cfx_read_only_span_stream::CfxReadOnlySpanStream,
        xml::{cfx_xmldocument::CfxXmlDocument, cfx_xmlparser::CfxXmlParser},
    };
    use std::{cell::RefCell, rc::Rc};
    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    fn node(
        doc: &Rc<CxfaDocument>,
        packet: XfaPacketType,
        kind: XfaElement,
        name: &str,
    ) -> CxfaNode {
        let node = doc.create_node(packet, kind).unwrap();
        node.set_cdata(XfaAttribute::Name, &wide(name));
        node
    }
    fn add(parent: &CxfaNode, child: &CxfaNode) {
        assert!(parent.insert_child_and_notify(-1, child));
    }
    pub(super) fn parsed_manager(records: &str) -> (Rc<CxfaDocument>, CxfaNode) {
        let input=format!("<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'><template xmlns='http://www.xfa.org/schema/xfa-template/3.3/'><subform name='record'><subform name='row'><occur min='0' max='4' initial='0'/><field name='value'><ui><textEdit/></ui></field></subform></subform></template><xfa:datasets xmlns:xfa='http://www.xfa.org/schema/xfa-data/1.0/'><xfa:data><record>{records}</record></xfa:data></xfa:datasets></xdp:xdp>");
        let mut bytes = input.into_bytes();
        bytes.push(0);
        let xml = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
            .parse()
            .unwrap();
        let doc = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(doc.clone());
        assert!(builder.build_document(Rc::new(RefCell::new(xml)), XfaPacketType::Xdp));
        assert!(doc.set_root(builder.get_root_node()));
        doc.do_data_merge().unwrap();
        let manager = doc
            .get_xfa_node(XfaHashCode::Form as u32)
            .unwrap()
            .get_first_child_by_class(XfaElement::Subform)
            .unwrap()
            .get_first_child_by_name(&wide("_row"))
            .unwrap();
        (doc, manager)
    }
    #[test]
    fn count_and_lookup_skip_noninstances_and_stop_at_name_or_manager_boundaries() {
        let doc = CxfaDocument::new();
        let parent = node(&doc, XfaPacketType::Form, XfaElement::Subform, "parent");
        let manager = node(
            &doc,
            XfaPacketType::Form,
            XfaElement::InstanceManager,
            "_row",
        );
        add(&parent, &manager);
        assert_eq!(manager.get_count(), 0);
        assert!(manager.get_item_if_exists(-1).is_none());
        let before = node(&doc, XfaPacketType::Form, XfaElement::Draw, "decoration");
        let first = node(&doc, XfaPacketType::Form, XfaElement::Subform, "row");
        let middle = node(&doc, XfaPacketType::Form, XfaElement::Draw, "gap");
        let second = node(&doc, XfaPacketType::Form, XfaElement::SubformSet, "row");
        let stop = node(&doc, XfaPacketType::Form, XfaElement::Subform, "other");
        let hidden = node(&doc, XfaPacketType::Form, XfaElement::Subform, "row");
        for child in [&before, &first, &middle, &second, &stop, &hidden] {
            add(&parent, child);
        }
        assert_eq!(manager.get_count(), 2);
        assert_eq!(manager.get_item_if_exists(0), Some(first.clone()));
        assert_eq!(manager.get_item_if_exists(1), Some(second));
        assert!(manager.get_item_if_exists(2).is_none());
        manager.set_cdata(XfaAttribute::Name, &wide("row"));
        assert_eq!(manager.get_count(), 0);
        manager.set_cdata(XfaAttribute::Name, &wide("_row"));
        let barrier = node(
            &doc,
            XfaPacketType::Form,
            XfaElement::InstanceManager,
            "_other",
        );
        assert!(parent.insert_child_before_and_notify(&barrier, Some(&first)));
        assert_eq!(manager.get_count(), 0);
    }
    #[test]
    fn zero_instance_insertion_and_negative_lookup_are_safe() {
        let doc = CxfaDocument::new();
        let parent = node(&doc, XfaPacketType::Form, XfaElement::Subform, "parent");
        let manager = node(
            &doc,
            XfaPacketType::Form,
            XfaElement::InstanceManager,
            "_row",
        );
        let boundary = node(&doc, XfaPacketType::Form, XfaElement::Subform, "other");
        add(&parent, &manager);
        add(&parent, &boundary);
        let instance = node(&doc, XfaPacketType::Form, XfaElement::Subform, "row");
        assert!(manager.insert_item(&instance, 0, 0, true));
        assert_eq!(manager.get_count(), 1);
        assert_eq!(instance.next_sibling(), Some(boundary));
        assert!(manager.get_item_if_exists(-1).is_none());
        assert!(!manager.insert_item(&instance, 0, 1, false));
    }
    #[test]
    fn moving_to_end_reorders_bound_data_and_its_xml_mapping() {
        let(doc,manager)=parsed_manager("<row><value>one</value></row><row><value>two</value></row><row><value>three</value></row>");
        let first = manager.get_item_if_exists(0).unwrap();
        let second = manager.get_item_if_exists(1).unwrap();
        let third = manager.get_item_if_exists(2).unwrap();
        let first_data = first.get_bind_data().unwrap();
        let second_data = second.get_bind_data().unwrap();
        let third_data = third.get_bind_data().unwrap();
        let record = doc.get_xfa_node(XfaHashCode::Record as u32).unwrap();
        assert!(manager.remove_item(&first, false));
        assert!(manager.insert_item(&first, 2, 2, true));
        assert_eq!(manager.get_item_if_exists(0), Some(second.clone()));
        assert_eq!(manager.get_item_if_exists(2), Some(first.clone()));
        assert_eq!(record.first_child(), Some(second_data.clone()));
        assert_eq!(third_data.next_sibling(), Some(first_data.clone()));
        assert_eq!(
            third_data.get_xml_mapping_node().unwrap().next_sibling(),
            first_data.get_xml_mapping_node()
        );
        // Already adjacent data must not be removed then inserted before itself.
        assert!(reorder_data_nodes(
            &HashSet::from([first_data.clone()]),
            &HashSet::from([third_data]),
            false
        ));
        assert_eq!(record.last_child(), Some(first_data));
        assert_eq!(second.get_instance_index(), 0);
        assert_eq!(third.get_instance_index(), 1);
        assert_eq!(first.get_instance_index(), 2);
    }
    #[test]
    fn removal_unbinds_descendants_and_preserves_shared_dataset_values() {
        let (doc, manager) =
            parsed_manager("<row><value>one</value></row><row><value>two</value></row>");
        let first = manager.get_item_if_exists(0).unwrap();
        let second = manager.get_item_if_exists(1).unwrap();
        let first_data = first.get_bind_data().unwrap();
        let field = first.get_first_child_by_name(&wide("value")).unwrap();
        let value = field.get_bind_data().unwrap();
        let shared = node(&doc, XfaPacketType::Form, XfaElement::Field, "shared");
        shared.set_binding_node(Some(&value));
        value.add_bind_item(&shared);
        assert!(manager.remove_item(&first, true));
        assert!(first.parent().is_none());
        assert!(first.get_bind_data().is_none());
        assert!(field.get_bind_data().is_none());
        assert!(first_data.parent().is_none());
        assert!(value.has_bind_items());
        assert_eq!(value.get_bind_items_copy(), [shared]);
        assert_eq!(manager.get_item_if_exists(0), Some(second));
    }
    #[test]
    fn creation_clones_and_merges_real_template_bindings_then_detaches_for_insertion() {
        let (_doc, manager) = parsed_manager("<row><value>one</value></row>");
        let first = manager.get_item_if_exists(0).unwrap();
        let created = manager.create_instance_if_possible(true).unwrap().unwrap();
        assert!(created.parent().is_none());
        assert_eq!(manager.get_count(), 1);
        assert_eq!(
            created.get_template_node_if_exists(),
            manager.get_template_node_if_exists()
        );
        assert!(created.get_bind_data().is_some());
        assert!(created
            .get_first_child_by_name(&wide("value"))
            .unwrap()
            .get_bind_data()
            .is_some());
        assert!(manager.insert_item(&created, 1, 1, false));
        assert_eq!(manager.get_count(), 2);
        assert_eq!(manager.get_item_if_exists(0), Some(first));
        assert_eq!(manager.get_item_if_exists(1), Some(created));
    }
    #[test]
    fn data_reordering_groups_parents_names_and_excludes_shared_nodes() {
        let doc = CxfaDocument::new();
        doc.set_xml_document(Rc::new(RefCell::new(CfxXmlDocument::new())));
        let parent = node(
            &doc,
            XfaPacketType::Datasets,
            XfaElement::DataGroup,
            "record",
        );
        let a = node(&doc, XfaPacketType::Datasets, XfaElement::DataValue, "row");
        let b = node(&doc, XfaPacketType::Datasets, XfaElement::DataValue, "row");
        let c = node(
            &doc,
            XfaPacketType::Datasets,
            XfaElement::DataValue,
            "other",
        );
        let d = node(&doc, XfaPacketType::Datasets, XfaElement::DataValue, "row");
        for child in [&a, &b, &c, &d] {
            add(&parent, child);
        }
        assert!(reorder_data_nodes(
            &HashSet::from([d.clone(), c.clone()]),
            &HashSet::from([a.clone(), c.clone()]),
            true
        ));
        assert_eq!(parent.first_child(), Some(d.clone()));
        assert_eq!(d.next_sibling(), Some(a));
        assert_eq!(b.next_sibling(), Some(c));
    }
}
