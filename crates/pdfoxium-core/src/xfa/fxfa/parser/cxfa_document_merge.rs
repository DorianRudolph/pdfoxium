// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Prototype merging and pending binding cleanup from `cxfa_document.cpp`.
//! Data-driven container instantiation and layout remain separate open work.
use super::{
    cxfa_document::CxfaDocument,
    cxfa_node::{hash_name, CxfaNode, XfaNodeFlag},
    cxfa_traversestrategy_xfanode::CxfaNodeIterator,
};
use crate::{
    fxjs::xfa::cfxjse_resolveprocessor::{CfxJseResolveProcessor, ResolveError, XfaResolveFlags},
    xfa::fxfa::fxfa_basic::{XfaAttribute, XfaElement, XfaHashCode, XfaPacketType},
};
use std::{collections::BTreeMap, fmt, rc::Rc};

#[derive(Debug)]
pub enum ProtoMergeError {
    Resolve(ResolveError),
    InvalidPrototype,
    AllocationFailed,
    TreeMutation,
}
impl fmt::Display for ProtoMergeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resolve(error) => write!(f, "prototype resolution: {error}"),
            Self::InvalidPrototype => f.write_str("invalid or recursive XFA prototype"),
            Self::AllocationFailed => f.write_str("cannot clone XFA prototype"),
            Self::TreeMutation => f.write_str("prototype tree changed during notification"),
        }
    }
}
impl std::error::Error for ProtoMergeError {}
impl From<ResolveError> for ProtoMergeError {
    fn from(error: ResolveError) -> Self {
        Self::Resolve(error)
    }
}
fn children(node: &CxfaNode) -> Vec<CxfaNode> {
    let mut result = Vec::new();
    let mut child = node.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        result.push(current);
    }
    result
}
fn contains(root: &CxfaNode, node: &CxfaNode) -> bool {
    let mut current = Some(node.clone());
    while let Some(item) = current {
        if item == *root {
            return true;
        }
        current = item.parent();
    }
    false
}
fn mark_unused(root: &CxfaNode, unused: bool) {
    let mut iterator = CxfaNodeIterator::new(Some(root.clone()));
    while let Some(node) = iterator.get_current() {
        if unused {
            node.set_flag(XfaNodeFlag::UnusedNode);
        } else {
            node.clear_flag(XfaNodeFlag::UnusedNode);
        }
        iterator.move_to_next();
    }
}
struct UnusedReset(CxfaNode);
impl Drop for UnusedReset {
    fn drop(&mut self) {
        mark_unused(&self.0, false);
    }
}
/// Native MergeNode / MergeNodeRecurse, with an explicit DFS stack. Unmatched
/// local children remain; matching children inherit attributes from prototypes.
fn merge_node(destination: &CxfaNode, prototype: &CxfaNode) -> Result<(), ProtoMergeError> {
    // A prototype containing its destination would clone the destination back
    // into itself. Reject it before the native recursive algorithm can expand.
    if contains(prototype, destination) {
        return Err(ProtoMergeError::InvalidPrototype);
    }
    mark_unused(destination, true);
    let _reset = UnusedReset(destination.clone());
    destination.set_template_node(Some(prototype));
    let mut stack: Vec<_> = children(prototype)
        .into_iter()
        .rev()
        .map(|child| (destination.clone(), child))
        .collect();
    while let Some((parent, source)) = stack.pop() {
        let mut existing = None;
        let mut child = parent.first_child();
        while let Some(candidate) = child {
            child = candidate.next_sibling();
            if candidate.get_element_type() == source.get_element_type()
                && candidate.get_name_hash() == source.get_name_hash()
                && candidate.has_flag(XfaNodeFlag::UnusedNode)
            {
                candidate.clear_flag(XfaNodeFlag::UnusedNode);
                existing = Some(candidate);
                break;
            }
        }
        if let Some(existing) = existing {
            existing.set_template_node(Some(&source));
            stack.extend(
                children(&source)
                    .into_iter()
                    .rev()
                    .map(|child| (existing.clone(), child)),
            );
        } else {
            let new_node = source
                .clone_node(true)
                .ok_or(ProtoMergeError::AllocationFailed)?;
            new_node.set_template_node(Some(&source));
            if !parent.insert_child_and_notify(-1, &new_node) {
                return Err(ProtoMergeError::TreeMutation);
            }
        }
    }
    Ok(())
}
impl CxfaDocument {
    /// Native CloneOrMergeInstanceManager. Detached instances are returned in
    /// source order for the caller's subsequent container merge.
    pub fn clone_or_merge_instance_manager(
        self: &Rc<Self>,
        form_parent: &CxfaNode,
        template: &CxfaNode,
        subforms: &mut Vec<CxfaNode>,
    ) -> Option<CxfaNode> {
        use super::xfa_document_datamerger_imp::data_merge_find_form_dom_instance;
        if form_parent.get_packet_type() != XfaPacketType::Form
            || template.get_packet_type() != XfaPacketType::Template
            || form_parent
                .document()
                .is_none_or(|document| !Rc::ptr_eq(&document, self))
            || template
                .document()
                .is_none_or(|document| !Rc::ptr_eq(&document, self))
        {
            return None;
        }
        let mut name = vec![u32::from('_')];
        name.extend(template.get_cdata(XfaAttribute::Name));
        if let Some(existing) = data_merge_find_form_dom_instance(
            self,
            XfaElement::InstanceManager,
            hash_name(&name),
            form_parent,
        ) {
            let mut child = existing.next_sibling();
            while let Some(node) = child {
                let element = node.get_element_type();
                if element == XfaElement::InstanceManager {
                    break;
                }
                child = node.next_sibling();
                if !matches!(element, XfaElement::Subform | XfaElement::SubformSet) {
                    continue;
                }
                if node.get_name_hash() != template.get_name_hash() {
                    break;
                }
                if !form_parent.remove_child_and_notify(&node, true) {
                    return None;
                }
                subforms.push(node);
            }
            if !form_parent.remove_child_and_notify(&existing, true)
                || !form_parent.insert_child_and_notify(-1, &existing)
            {
                return None;
            }
            existing.clear_flag(XfaNodeFlag::UnusedNode);
            existing.set_template_node(Some(template));
            Some(existing)
        } else {
            let node = self.create_node(XfaPacketType::Form, XfaElement::InstanceManager)?;
            node.set_cdata(XfaAttribute::Name, &name);
            if !form_parent.insert_child_and_notify(-1, &node) {
                return None;
            }
            node.set_template_node(Some(template));
            Some(node)
        }
    }
    pub fn do_proto_merge(self: &Rc<Self>) -> Result<(), ProtoMergeError> {
        self.do_proto_merge_with_resolver(&CfxJseResolveProcessor::new(self.clone()))
    }
    pub fn do_proto_merge_with_resolver(
        &self,
        resolver: &CfxJseResolveProcessor,
    ) -> Result<(), ProtoMergeError> {
        let Some(template) = self.get_xfa_node(XfaHashCode::Template as u32) else {
            return Ok(());
        };
        let mut ids = BTreeMap::new();
        let mut use_nodes = Vec::new();
        let mut iterator = CxfaNodeIterator::new(Some(template));
        while let Some(node) = iterator.get_current() {
            let id = node.get_cdata(XfaAttribute::Id);
            if !id.is_empty() {
                ids.insert(hash_name(&id), node.clone());
            }
            if !node.get_cdata(XfaAttribute::Use).is_empty()
                || !node.get_cdata(XfaAttribute::Usehref).is_empty()
            {
                use_nodes.push(node);
            }
            iterator.move_to_next();
        }
        // Native uses a set of allocation addresses. Document order avoids an
        // allocator-dependent order while preserving the same lookup/merge work.
        for destination in use_nodes {
            let href = destination.get_cdata(XfaAttribute::Usehref);
            let value;
            let (id, som) = if !href.is_empty() {
                let parts = Self::parse_use_href(&href);
                if !parts.uri.is_empty() && parts.uri != [u32::from('.')] {
                    continue;
                }
                (parts.id, parts.som)
            } else {
                value = destination.get_cdata(XfaAttribute::Use);
                let parts = Self::parse_use(&value);
                (parts.id, parts.som)
            };
            let prototype = if !som.is_empty() {
                resolver
                    .resolve_objects(
                        Some(&destination),
                        som,
                        XfaResolveFlags::CHILDREN
                            | XfaResolveFlags::ATTRIBUTES
                            | XfaResolveFlags::PROPERTIES
                            | XfaResolveFlags::PARENT
                            | XfaResolveFlags::SIBLINGS,
                    )?
                    .and_then(|result| result.objects.into_iter().next())
            } else if !id.is_empty() {
                ids.get(&hash_name(id)).cloned()
            } else {
                None
            };
            if let Some(prototype) = prototype {
                if prototype.get_packet_type() != XfaPacketType::Template
                    || prototype
                        .document()
                        .is_none_or(|document| !std::ptr::eq(document.as_ref(), self))
                {
                    return Err(ProtoMergeError::InvalidPrototype);
                }
                merge_node(&destination, &prototype)?;
            }
        }
        Ok(())
    }
    pub fn get_not_bind_node(&self, nodes: &[CxfaNode]) -> Option<CxfaNode> {
        nodes
            .iter()
            .find(|node| node.is_alive() && !node.has_bind_item())
            .cloned()
    }
    pub fn set_pending_nodes_unused_and_unbound(&self) {
        // Snapshot weak handles so notification/reentry never holds the cache.
        let pending: Vec<_> = (0..self.get_pending_nodes_count())
            .filter_map(|index| self.get_pending_node_at_index(index))
            .collect();
        for root in pending {
            if !root.is_alive() {
                continue;
            }
            let mut iterator = CxfaNodeIterator::new(Some(root));
            while let Some(node) = iterator.get_current() {
                if node.is_container_node() {
                    if let Some(data) = node.get_bind_data() {
                        data.remove_bind_item(&node);
                        node.set_binding_node(None);
                    }
                }
                node.set_flag(XfaNodeFlag::UnusedNode);
                iterator.move_to_next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(
        document: &Rc<CxfaDocument>,
        packet: XfaPacketType,
        element: XfaElement,
        name: &str,
    ) -> CxfaNode {
        let node = document.create_node(packet, element).unwrap();
        node.set_cdata(
            XfaAttribute::Name,
            &name.chars().map(u32::from).collect::<Vec<_>>(),
        );
        node
    }
    #[test]
    fn instance_manager_reuses_ordered_subforms_and_stops_at_other_name() {
        let document = CxfaDocument::new();
        let template = node(
            &document,
            XfaPacketType::Template,
            XfaElement::Subform,
            "row",
        );
        let form = node(&document, XfaPacketType::Form, XfaElement::Form, "form");
        let mut instances = Vec::new();
        let manager = document
            .clone_or_merge_instance_manager(&form, &template, &mut instances)
            .unwrap();
        let first = template.clone_template_to_form(false).unwrap();
        let second = template.clone_template_to_form(false).unwrap();
        let other = node(&document, XfaPacketType::Form, XfaElement::Subform, "other");
        let after = template.clone_template_to_form(false).unwrap();
        for child in [&first, &second, &other, &after] {
            assert!(form.insert_child_and_notify(-1, child));
        }
        manager.set_flag(XfaNodeFlag::UnusedNode);
        let reused = document
            .clone_or_merge_instance_manager(&form, &template, &mut instances)
            .unwrap();
        assert_eq!(reused, manager);
        assert_eq!(instances, [first.clone(), second.clone()]);
        assert!(first.parent().is_none());
        assert!(second.parent().is_none());
        assert_eq!(form.first_child(), Some(other));
        assert_eq!(form.last_child(), Some(manager.clone()));
        assert_eq!(after.next_sibling(), Some(manager.clone()));
        assert!(!manager.is_unused_node());
        assert_eq!(manager.get_template_node_if_exists(), Some(template));
    }
    #[test]
    fn pending_cleanup_removes_reciprocal_bindings_and_marks_descendants() {
        let document = CxfaDocument::new();
        let form = node(&document, XfaPacketType::Form, XfaElement::Subform, "row");
        let field = node(&document, XfaPacketType::Form, XfaElement::Field, "value");
        let data = node(
            &document,
            XfaPacketType::Datasets,
            XfaElement::DataValue,
            "value",
        );
        assert!(form.insert_child_and_notify(-1, &field));
        field.set_binding_node(Some(&data));
        data.add_bind_item(&field);
        document.append_pending_node(form.clone());
        document.set_pending_nodes_unused_and_unbound();
        assert!(form.is_unused_node());
        assert!(field.is_unused_node());
        assert!(field.get_bind_data().is_none());
        assert!(!data.has_bind_item());
        assert_eq!(document.get_not_bind_node(&[data.clone()]), Some(data));
        assert_eq!(document.get_pending_node_at_index(0), Some(form));
    }
    #[test]
    fn prototype_containing_destination_rejects_before_mutation() {
        let document = CxfaDocument::new();
        let ancestor = node(
            &document,
            XfaPacketType::Template,
            XfaElement::Subform,
            "ancestor",
        );
        let destination = node(
            &document,
            XfaPacketType::Template,
            XfaElement::Field,
            "value",
        );
        assert!(ancestor.insert_child_and_notify(-1, &destination));
        assert!(matches!(
            merge_node(&destination, &ancestor),
            Err(ProtoMergeError::InvalidPrototype)
        ));
        assert!(destination.get_template_node_if_exists().is_none());
        assert!(!destination.is_unused_node());
    }
}
