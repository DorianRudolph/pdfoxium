// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Model data-merge algorithms from `cxfa_document.cpp`. Actual value binding
//! uses the shared typed value/picture helpers; SOM uses the node resolver.
use super::{
    cxfa_data_binding::{
        create_data_binding, form_value_node_match_none_create_child, DataBindingError,
    },
    cxfa_document::CxfaDocument,
    cxfa_node::{CxfaNode, XfaNodeFlag},
    cxfa_nodeiteratortemplate::{CxfaNodeIteratorTemplate, TraverseStrategy},
    cxfa_traversestrategy_xfacontainernode::CxfaContainerIterator,
    cxfa_traversestrategy_xfanode::CxfaNodeIterator,
    xfa_basic_data::hash_name,
    xfa_document_datamerger_imp::{
        data_merge_find_data_scope, data_merge_find_form_dom_instance,
        data_merge_need_generate_form, node_merge_clone_or_merge_container,
    },
};
use crate::{
    fxcrt::xml::cfx_xmlnode::XmlNodeRef,
    fxjs::xfa::cfxjse_resolveprocessor::{
        CfxJseResolveProcessor, ResolveError, ResolveResultType, XfaResolveFlags,
    },
    xfa::fxfa::fxfa_basic::{
        XfaAttribute, XfaAttributeValue, XfaElement, XfaHashCode, XfaPacketType,
    },
};
use std::{cell::Cell, fmt, rc::Rc};

#[derive(Debug)]
pub enum DataMergeError {
    Resolve(ResolveError),
    Binding(DataBindingError),
    MissingRoot,
    MissingXmlDocument,
    InvalidTree(&'static str),
    NodeCreation(XfaElement),
}
impl fmt::Display for DataMergeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resolve(error) => fmt::Display::fmt(error, f),
            Self::Binding(error) => write!(f, "data binding failed: {error:?}"),
            Self::MissingRoot => f.write_str("data merge requires an XFA root"),
            Self::MissingXmlDocument => {
                f.write_str("data merge requires the retained XML document")
            }
            Self::InvalidTree(message) => f.write_str(message),
            Self::NodeCreation(element) => write!(f, "cannot create XFA {element:?} node"),
        }
    }
}
impl std::error::Error for DataMergeError {}
impl From<ResolveError> for DataMergeError {
    fn from(error: ResolveError) -> Self {
        Self::Resolve(error)
    }
}
impl From<DataBindingError> for DataMergeError {
    fn from(error: DataBindingError) -> Self {
        Self::Binding(error)
    }
}
/// DoDataRemerge's real final layout invalidation. Model-only callers use the
/// explicitly named do_data_remerge_model instead of a fabricated layout host.
pub trait DataMergeLayoutHost {
    fn set_force_relayout(&self);
}
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn ascii(value: &[u32], text: &str) -> bool {
    value.iter().copied().eq(text.bytes().map(u32::from))
}
fn children(node: &CxfaNode) -> impl Iterator<Item = CxfaNode> {
    let root = node.clone();
    let mut previous: Option<CxfaNode> = None;
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
struct DdGroupStrategy;
impl TraverseStrategy<CxfaNode> for DdGroupStrategy {
    fn get_first_child(node: &CxfaNode) -> Option<CxfaNode> {
        node.get_first_child_by_name_hash(XfaHashCode::Group as u32)
    }
    fn get_next_sibling(node: &CxfaNode) -> Option<CxfaNode> {
        node.get_next_same_name_sibling(XfaHashCode::Group as u32)
    }
    fn get_parent(node: &CxfaNode) -> Option<CxfaNode> {
        node.parent()
    }
}
type DdGroupIterator = CxfaNodeIteratorTemplate<CxfaNode, DdGroupStrategy>;
struct RecurseRecord {
    template: CxfaNode,
    data: CxfaNode,
}
fn sort_recurse_records(records: &mut Vec<RecurseRecord>, scope: Option<&CxfaNode>, choice: bool) {
    let Some(scope) = scope else {
        return;
    };
    let mut result = Vec::new();
    for child in children(scope) {
        if let Some(index) = records.iter().position(|record| record.data == child) {
            result.push(records.remove(index));
            if choice {
                break;
            }
        }
    }
    if result.is_empty() {
        return;
    }
    if !choice {
        result.append(records);
    }
    *records = result;
}
fn scope_match_global_binding(
    scope: Option<&CxfaNode>,
    name_hash: u32,
    match_type: XfaElement,
    up_level: bool,
) -> Option<CxfaNode> {
    let mut scope = scope.cloned();
    let mut last_scope = None;
    while let Some(current) = scope {
        if current.get_packet_type() != XfaPacketType::Datasets {
            break;
        }
        // Search this scope's direct matches before its descendant data groups.
        // An explicit stack retains native DFS order for arbitrarily deep data.
        let mut stack = vec![(current.clone(), last_scope.clone())];
        while let Some((group, skip)) = stack.pop() {
            let mut child = group.get_first_child_by_name_hash(name_hash);
            while let Some(node) = child {
                child = node.get_next_same_name_sibling(name_hash);
                if skip.as_ref() != Some(&node)
                    && (match_type == XfaElement::DataModel
                        || node.get_element_type() == match_type)
                    && !node.has_bind_item()
                {
                    return Some(node);
                }
            }
            let groups: Vec<_> = children(&group)
                .filter(|node| node.get_element_type() == XfaElement::DataGroup)
                .collect();
            stack.extend(groups.into_iter().rev().map(|node| (node, None)));
        }
        if !up_level {
            break;
        }
        last_scope = Some(current.clone());
        scope = current.parent();
    }
    None
}
fn find_once_data_node(
    name: &[u32],
    scope: Option<&CxfaNode>,
    match_type: XfaElement,
) -> Option<CxfaNode> {
    if name.is_empty() {
        return None;
    }
    let hash = hash_name(name);
    let mut last_scope = None;
    let mut scope = scope.cloned();
    while let Some(current) = scope {
        if current.get_packet_type() != XfaPacketType::Datasets {
            break;
        }
        let mut child = current.get_first_child_by_name_hash(hash);
        while let Some(node) = child {
            child = node.get_next_same_name_sibling(hash);
            if last_scope.as_ref() != Some(&node)
                && !node.has_bind_item()
                && (match_type == XfaElement::DataModel || node.get_element_type() == match_type)
            {
                return Some(node);
            }
        }
        last_scope = Some(current.clone());
        scope = current.parent();
    }
    None
}
#[derive(Default)]
struct MatchResult {
    node: Option<CxfaNode>,
    self_match: bool,
    bind_match: Option<XfaAttributeValue>,
}
// CopyContainer_SubformSet has a substantial native-style recursive frame.
// Bound each operation before nested template/model data can exhaust the Rust
// call stack. Independent/reentrant operations have their own counter.
const MAX_MERGE_DEPTH: usize = 128;
struct MergeDepthGuard<'a> {
    depth: &'a Cell<usize>,
    previous: usize,
}
impl Drop for MergeDepthGuard<'_> {
    fn drop(&mut self) {
        self.depth.set(self.previous);
    }
}
struct DataMerger<'a> {
    document: &'a Rc<CxfaDocument>,
    resolver: &'a CfxJseResolveProcessor,
    recursion_depth: Cell<usize>,
}
impl<'a> DataMerger<'a> {
    fn new(document: &'a Rc<CxfaDocument>, resolver: &'a CfxJseResolveProcessor) -> Self {
        Self {
            document,
            resolver,
            recursion_depth: Cell::new(0),
        }
    }
    fn enter_recursive_merge(&self) -> Result<MergeDepthGuard<'_>, DataMergeError> {
        let previous = self.recursion_depth.get();
        if previous >= MAX_MERGE_DEPTH {
            return Err(DataMergeError::InvalidTree(
                "XFA data merge nesting exceeds the supported stack depth",
            ));
        }
        self.recursion_depth.set(previous + 1);
        Ok(MergeDepthGuard {
            depth: &self.recursion_depth,
            previous,
        })
    }
    fn new_node(
        &self,
        packet: XfaPacketType,
        element: XfaElement,
    ) -> Result<CxfaNode, DataMergeError> {
        self.document
            .create_node(packet, element)
            .ok_or(DataMergeError::NodeCreation(element))
    }
    fn clone_container(
        &self,
        parent: &CxfaNode,
        template: &CxfaNode,
        recursive: bool,
        search: Option<&mut Vec<CxfaNode>>,
    ) -> Result<CxfaNode, DataMergeError> {
        node_merge_clone_or_merge_container(self.document, parent, template, recursive, search)
            .ok_or(DataMergeError::NodeCreation(template.get_element_type()))
    }
    fn xml_element(&self, name: &[u32]) -> Result<XmlNodeRef, DataMergeError> {
        let xml = self
            .document
            .xml_document()
            .ok_or(DataMergeError::MissingXmlDocument)?;
        let node = xml.borrow_mut().create_element(name);
        Ok(node)
    }
    fn find_global_data_node(
        &self,
        name: &[u32],
        scope: Option<&CxfaNode>,
        match_type: XfaElement,
    ) -> Option<CxfaNode> {
        if name.is_empty() {
            return None;
        }
        let hash = hash_name(name);
        if let Some(node) = self.document.get_global_binding(hash) {
            return Some(node);
        }
        let node = scope_match_global_binding(scope, hash, match_type, true)?;
        self.document
            .register_global_binding(hash, Some(node.clone()));
        Some(node)
    }
    fn find_data_ref_node(
        &self,
        reference: &[u32],
        scope: Option<&CxfaNode>,
        template: &CxfaNode,
        force_bind: bool,
        up_level: bool,
    ) -> Result<Option<CxfaNode>, DataMergeError> {
        let mut flags = XfaResolveFlags::CHILDREN | XfaResolveFlags::BIND_NEW;
        if up_level || !ascii(reference, "name") {
            flags |= XfaResolveFlags::PARENT | XfaResolveFlags::SIBLINGS;
        }
        let Some(result) = self.resolver.resolve_objects_with_bind_node(
            scope,
            reference,
            flags,
            Some(template),
        )?
        else {
            return Ok(None);
        };
        if matches!(
            result.result_type,
            ResolveResultType::CreateNodeAll | ResolveResultType::CreateNodeMidAll
        ) || result.objects.len() > 1
        {
            return Ok(result
                .objects
                .into_iter()
                .find(|node| !node.has_bind_item()));
        }
        if result.result_type == ResolveResultType::CreateNodeOne {
            return Ok(result
                .objects
                .into_iter()
                .next()
                .filter(|node| force_bind || !node.has_bind_item()));
        }
        Ok(None)
    }
    fn find_matching_data_node(
        &self,
        template: &CxfaNode,
        scope: Option<&CxfaNode>,
        accessed: &mut bool,
        force_bind: bool,
        iterator: &mut CxfaContainerIterator,
        up_level: bool,
    ) -> Result<MatchResult, DataMergeError> {
        let mut result = MatchResult::default();
        while let Some(current) = iterator.get_current() {
            let match_type = match current.get_element_type() {
                XfaElement::Subform => XfaElement::DataGroup,
                XfaElement::Field => {
                    if current.is_field_multi_list_box() {
                        XfaElement::DataGroup
                    } else {
                        XfaElement::DataValue
                    }
                }
                XfaElement::ExclGroup => XfaElement::DataValue,
                _ => {
                    iterator.move_to_next();
                    continue;
                }
            };
            if current
                .get_first_child_by_class(XfaElement::Occur)
                .is_some_and(|occur| occur.occur_get_occur_info().1 == 0)
            {
                iterator.move_to_next();
                continue;
            }
            let bind = current.get_first_child_by_class(XfaElement::Bind);
            let bind_match = bind.as_ref().map_or(XfaAttributeValue::Once, |bind| {
                bind.get_enum(XfaAttribute::Match)
            });
            result.bind_match = Some(bind_match);
            match bind_match {
                XfaAttributeValue::None => {
                    iterator.move_to_next();
                    continue;
                }
                XfaAttributeValue::Global => {
                    *accessed = true;
                    if !force_bind {
                        iterator.move_to_next();
                        continue;
                    }
                    // The multi-list check belongs to the field, not its Bind
                    // property (the native call accidentally passes that property).
                    if match_type == XfaElement::DataValue
                        || (match_type == XfaElement::DataGroup
                            && current.is_field_multi_list_box())
                    {
                        result.node = self.find_global_data_node(
                            &current.get_cdata(XfaAttribute::Name),
                            scope,
                            match_type,
                        );
                    } else {
                        result.node = find_once_data_node(
                            &current.get_cdata(XfaAttribute::Name),
                            scope,
                            match_type,
                        );
                    }
                    if result.node.is_none() {
                        iterator.move_to_next();
                        continue;
                    }
                }
                XfaAttributeValue::Once => {
                    *accessed = true;
                    result.node = find_once_data_node(
                        &current.get_cdata(XfaAttribute::Name),
                        scope,
                        match_type,
                    );
                    if result.node.is_none() {
                        iterator.move_to_next();
                        continue;
                    }
                }
                XfaAttributeValue::DataRef => {
                    *accessed = true;
                    let reference = bind
                        .as_ref()
                        .map_or_else(Vec::new, |bind| bind.get_cdata(XfaAttribute::Ref));
                    result.node = self
                        .find_data_ref_node(&reference, scope, template, force_bind, up_level)?
                        .filter(|node| node.get_element_type() == match_type);
                    if result.node.is_none() {
                        iterator.skip_children_and_move_to_next();
                        continue;
                    }
                }
                _ => {}
            }
            result.self_match = current == *template && result.node.is_some();
            break;
        }
        Ok(result)
    }
    fn maybe_create_data_node(
        &self,
        parent: Option<&CxfaNode>,
        element: XfaElement,
        name: &[u32],
    ) -> Result<Option<CxfaNode>, DataMergeError> {
        let Some(parent) = parent else {
            return Ok(None);
        };
        let description = parent.get_data_description_node();
        let mut matched_description = None;
        if let Some(description) = description {
            let mut iterator = DdGroupIterator::new(Some(description.clone()));
            while let Some(group) = iterator.get_current() {
                iterator.move_to_next();
                if group != description
                    && (group.get_element_type() != XfaElement::DataGroup
                        || !group
                            .try_namespace()
                            .is_some_and(|ns| ascii(&ns, "http://ns.adobe.com/data-description/")))
                {
                    continue;
                }
                let Some(node) = group.get_first_child_by_name(name) else {
                    continue;
                };
                if node.get_element_type() != element {
                    break;
                }
                matched_description = Some(node);
                break;
            }
            if matched_description.is_none() {
                return Ok(None);
            }
        }
        let node = self.new_node(XfaPacketType::Datasets, element)?;
        node.set_cdata(XfaAttribute::Name, name);
        if matched_description.as_ref().is_some_and(|description| {
            element == XfaElement::DataValue
                && description.get_enum(XfaAttribute::Contains) == XfaAttributeValue::MetaData
        }) {
            // Metadata maps to an attribute of the parent XML element. Native
            // creation gives it a separate element and no qualified name,
            // producing an empty XML attribute name when its value is written.
            node.set_cdata(XfaAttribute::QualifiedName, name);
            node.set_enum(XfaAttribute::Contains, XfaAttributeValue::MetaData, false);
            node.set_xml_mapping_node(
                parent
                    .get_xml_mapping_node()
                    .ok_or(DataMergeError::MissingXmlDocument)?,
            );
        } else {
            node.create_xml_mapping_node()
                .ok_or(DataMergeError::MissingXmlDocument)?;
        }
        if !parent.insert_child_and_notify(-1, &node) {
            return Err(DataMergeError::InvalidTree("cannot insert a dataset node"));
        }
        if let Some(description) = matched_description {
            node.set_data_description_node(Some(&description));
        }
        node.set_initialized_flag();
        Ok(Some(node))
    }
    fn clone_or_merge_instance_manager(
        &self,
        parent: &CxfaNode,
        template: &CxfaNode,
        subforms: &mut Vec<CxfaNode>,
    ) -> Result<CxfaNode, DataMergeError> {
        let mut name = vec![95];
        name.extend(template.get_cdata(XfaAttribute::Name));
        if let Some(existing) = data_merge_find_form_dom_instance(
            self.document,
            XfaElement::InstanceManager,
            hash_name(&name),
            parent,
        ) {
            let mut current = existing.next_sibling();
            while let Some(node) = current {
                let element = node.get_element_type();
                if element == XfaElement::InstanceManager {
                    break;
                }
                if !matches!(element, XfaElement::Subform | XfaElement::SubformSet) {
                    current = node.next_sibling();
                    continue;
                }
                if node.get_name_hash() != template.get_name_hash() {
                    break;
                }
                current = node.next_sibling();
                parent.remove_child_and_notify(&node, true);
                subforms.push(node);
            }
            parent.remove_child_and_notify(&existing, true);
            parent.insert_child_and_notify(-1, &existing);
            existing.clear_flag(XfaNodeFlag::UnusedNode);
            existing.set_template_node(Some(template));
            return Ok(existing);
        }
        let node = self.new_node(XfaPacketType::Form, XfaElement::InstanceManager)?;
        node.set_cdata(XfaAttribute::Name, &name);
        parent.insert_child_and_notify(-1, &node);
        node.set_template_node(Some(template));
        Ok(node)
    }
    fn copy_container(
        &self,
        template: &CxfaNode,
        parent: &CxfaNode,
        scope: Option<&CxfaNode>,
        one_instance: bool,
        data_merge: bool,
        up_level: bool,
    ) -> Result<Option<CxfaNode>, DataMergeError> {
        let _depth = self.enter_recursive_merge()?;
        match template.get_element_type() {
            XfaElement::Area
            | XfaElement::PageArea
            | XfaElement::Subform
            | XfaElement::SubformSet => {
                self.copy_subform_set(template, parent, scope, one_instance, data_merge)
            }
            XfaElement::ContentArea
            | XfaElement::Draw
            | XfaElement::ExclGroup
            | XfaElement::Field => self
                .copy_field(template, parent, scope, data_merge, up_level)
                .map(Some),
            XfaElement::PageSet | XfaElement::Variables => Ok(None),
            _ => Err(DataMergeError::InvalidTree(
                "data merge requires a supported container node",
            )),
        }
    }
    fn copy_field(
        &self,
        template: &CxfaNode,
        parent: &CxfaNode,
        scope: Option<&CxfaNode>,
        data_merge: bool,
        up_level: bool,
    ) -> Result<CxfaNode, DataMergeError> {
        let field = self.clone_container(parent, template, false, None)?;
        for child in children(template) {
            if data_merge_need_generate_form(&child, true) {
                self.clone_container(&field, &child, true, None)?;
            } else if template.get_element_type() == XfaElement::ExclGroup
                && child.is_container_node()
                && child.get_element_type() == XfaElement::Field
            {
                self.copy_field(&child, &field, None, false, true)?;
            }
        }
        if data_merge {
            let mut accessed = false;
            let mut iterator = CxfaContainerIterator::new(Some(template.clone()));
            if let Some(data) = self
                .find_matching_data_node(
                    template,
                    scope,
                    &mut accessed,
                    true,
                    &mut iterator,
                    up_level,
                )?
                .node
            {
                create_data_binding(&field, &data, true)?;
            }
        } else {
            form_value_node_match_none_create_child(&field)?;
        }
        Ok(field)
    }
    fn copy_children(
        &self,
        template: &CxfaNode,
        parent: &CxfaNode,
        scope: Option<&CxfaNode>,
        data_merge: bool,
        up_level: bool,
        use_instance_manager: bool,
    ) -> Result<(), DataMergeError> {
        for child in children(template) {
            if data_merge_need_generate_form(&child, use_instance_manager) {
                self.clone_container(parent, &child, true, None)?;
            } else if child.is_container_node() {
                self.copy_container(&child, parent, scope, false, data_merge, up_level)?;
            }
        }
        Ok(())
    }
    fn copy_subform_set(
        &self,
        template: &CxfaNode,
        parent: &CxfaNode,
        scope: Option<&CxfaNode>,
        mut one_instance: bool,
        data_merge: bool,
    ) -> Result<Option<CxfaNode>, DataMergeError> {
        let element = template.get_element_type();
        let mut occur = None;
        let mut first_instance = None;
        let use_instance_manager = parent.get_element_type() != XfaElement::Area;
        let mut instance_manager = None;
        let mut subforms = Vec::new();
        let mut use_search = false;
        if !one_instance && matches!(element, XfaElement::SubformSet | XfaElement::Subform) {
            if use_instance_manager {
                instance_manager =
                    Some(self.clone_or_merge_instance_manager(parent, template, &mut subforms)?);
            }
            if let Some(template_occur) = template.get_first_child_by_class(XfaElement::Occur) {
                occur = Some(if let Some(manager) = &instance_manager {
                    self.clone_container(manager, &template_occur, false, None)?
                } else {
                    template_occur
                });
            } else if let Some(manager) = &instance_manager {
                occur = manager.get_first_child_by_class(XfaElement::Occur);
                if let Some(occur) = &occur {
                    occur.clear_flag(XfaNodeFlag::UnusedNode);
                }
            }
            if let Some(manager) = &instance_manager {
                manager.set_initialized_flag_and_notify();
                use_search = true;
                if parent.get_element_type() == XfaElement::PageArea {
                    one_instance = true;
                    if subforms.is_empty() {
                        use_search = false;
                    }
                } else if template.get_name_hash() == 0 && subforms.is_empty() {
                    use_search = false;
                }
            }
        }
        let (min, max, init) = if !one_instance {
            occur
                .as_ref()
                .map_or((1, 1, 1), CxfaNode::occur_get_occur_info)
        } else {
            (1, 1, 1)
        };
        let relation = if element == XfaElement::SubformSet {
            template.get_enum(XfaAttribute::Relation)
        } else {
            XfaAttributeValue::Ordered
        };
        let mut repeat = 0i32;
        let mut parent_bind_match = XfaAttributeValue::None;
        if data_merge {
            let mut iterator = CxfaContainerIterator::new(Some(template.clone()));
            let mut accessed = false;
            if matches!(element, XfaElement::SubformSet | XfaElement::Area) {
                iterator.move_to_next();
            } else {
                // The source stores both a node array and a node->data map.
                // Ordered pairs retain the same operation order without a second index.
                let mut instances = Vec::new();
                while max < 0 || repeat < max {
                    let matched = self.find_matching_data_node(
                        template,
                        scope,
                        &mut accessed,
                        false,
                        &mut iterator,
                        true,
                    )?;
                    let Some(data) = matched.node else {
                        break;
                    };
                    if iterator.get_current().as_ref() != Some(template) {
                        break;
                    }
                    parent_bind_match = matched.bind_match.unwrap_or(XfaAttributeValue::None);
                    let subform = self.clone_container(
                        parent,
                        template,
                        false,
                        if use_search {
                            Some(&mut subforms)
                        } else {
                            None
                        },
                    )?;
                    if first_instance.is_none() {
                        first_instance = Some(subform.clone());
                    }
                    create_data_binding(&subform, &data, true)?;
                    instances.push((subform, data));
                    repeat += 1;
                }
                for (subform, data) in instances {
                    self.copy_children(
                        template,
                        &subform,
                        Some(&data),
                        true,
                        false,
                        use_instance_manager,
                    )?;
                }
            }
            while max < 0 || repeat < max {
                let matched = self.find_matching_data_node(
                    template,
                    scope,
                    &mut accessed,
                    false,
                    &mut iterator,
                    true,
                )?;
                if matched.node.is_none() {
                    break;
                }
                if matched.bind_match == Some(XfaAttributeValue::DataRef)
                    && parent_bind_match == XfaAttributeValue::DataRef
                {
                    break;
                }
                let instance = self.clone_container(
                    parent,
                    template,
                    false,
                    if use_search {
                        Some(&mut subforms)
                    } else {
                        None
                    },
                )?;
                if first_instance.is_none() {
                    first_instance = Some(instance.clone());
                }
                if matches!(
                    relation,
                    XfaAttributeValue::Choice | XfaAttributeValue::Unordered
                ) {
                    let mut matches = Vec::new();
                    let mut unmatched = Vec::new();
                    for child in children(template) {
                        if data_merge_need_generate_form(&child, use_instance_manager) {
                            self.clone_container(&instance, &child, true, None)?;
                        } else if child.is_container_node() {
                            let mut child_iterator =
                                CxfaContainerIterator::new(Some(child.clone()));
                            let matched = self.find_matching_data_node(
                                &child,
                                scope,
                                &mut accessed,
                                false,
                                &mut child_iterator,
                                true,
                            )?;
                            if let Some(data) = matched.node {
                                let record = RecurseRecord {
                                    template: child,
                                    data,
                                };
                                if matched.self_match {
                                    matches.insert(0, record);
                                } else {
                                    matches.push(record);
                                }
                            } else {
                                unmatched.push(child);
                            }
                        }
                    }
                    match relation {
                        XfaAttributeValue::Choice => {
                            sort_recurse_records(&mut matches, scope, true);
                            let first = matches.first().ok_or(DataMergeError::InvalidTree(
                                "choice container lost its matching data child",
                            ))?;
                            self.copy_container(
                                &first.template,
                                &instance,
                                scope,
                                false,
                                true,
                                true,
                            )?;
                        }
                        XfaAttributeValue::Unordered => {
                            sort_recurse_records(&mut matches, scope, false);
                            for record in matches {
                                self.copy_container(
                                    &record.template,
                                    &instance,
                                    scope,
                                    false,
                                    true,
                                    true,
                                )?;
                            }
                            for child in unmatched {
                                self.copy_container(&child, &instance, scope, false, true, true)?;
                            }
                        }
                        _ => {}
                    }
                } else {
                    self.copy_children(
                        template,
                        &instance,
                        scope,
                        true,
                        true,
                        use_instance_manager,
                    )?;
                }
                repeat += 1;
            }
            if repeat == 0 && !accessed {
                let mut limit = max;
                if instance_manager.is_some() && template.get_name_hash() == 0 {
                    limit = i32::try_from(subforms.len())
                        .map_err(|_| DataMergeError::InvalidTree("too many form instances"))?;
                    if limit < min {
                        limit = init;
                    }
                }
                while limit < 0 || repeat < limit {
                    if instance_manager.is_some() {
                        if use_search && subforms.is_empty() {
                            if template.get_name_hash() != 0 {
                                break;
                            }
                            use_search = false;
                        }
                    } else if data_merge_find_form_dom_instance(
                        self.document,
                        element,
                        template.get_name_hash(),
                        parent,
                    )
                    .is_none()
                    {
                        break;
                    }
                    let instance = self.clone_container(
                        parent,
                        template,
                        false,
                        if use_search {
                            Some(&mut subforms)
                        } else {
                            None
                        },
                    )?;
                    if first_instance.is_none() {
                        first_instance = Some(instance.clone());
                    }
                    if relation == XfaAttributeValue::Choice {
                        // A reused unbound choice has the same default-branch
                        // rule as an initially created choice below. Copying
                        // every template container here revives inactive
                        // siblings when restoring a saved form packet.
                        let mut found = false;
                        for child in children(template) {
                            if data_merge_need_generate_form(&child, use_instance_manager) {
                                self.clone_container(&instance, &child, true, None)?;
                            } else if child.is_container_node() && !found {
                                self.copy_container(&child, &instance, scope, false, true, true)?;
                                found = true;
                            }
                        }
                    } else {
                        self.copy_children(
                            template,
                            &instance,
                            scope,
                            true,
                            true,
                            use_instance_manager,
                        )?;
                    }
                    repeat += 1;
                }
            }
        }
        let minimal_limit = if repeat == 0 { init } else { min };
        while repeat < minimal_limit {
            let instance = self.clone_container(
                parent,
                template,
                false,
                if use_search {
                    Some(&mut subforms)
                } else {
                    None
                },
            )?;
            if first_instance.is_none() {
                first_instance = Some(instance.clone());
            }
            let mut found = false;
            for child in children(template) {
                if child.get_packet_type() != XfaPacketType::Template {
                    continue;
                }
                if data_merge_need_generate_form(&child, use_instance_manager) {
                    self.clone_container(&instance, &child, true, None)?;
                } else if child.is_container_node() {
                    if found && relation == XfaAttributeValue::Choice {
                        continue;
                    }
                    self.copy_container(&child, &instance, scope, false, data_merge, true)?;
                    found = true;
                }
            }
            repeat += 1;
        }
        Ok(first_instance)
    }
    fn update_binding_relations(
        &self,
        form: &CxfaNode,
        scope: Option<&CxfaNode>,
        data_ref: bool,
        mut parent_data_ref: bool,
    ) -> Result<(), DataMergeError> {
        let _depth = self.enter_recursive_merge()?;
        let element = form.get_element_type();
        let mut data = form.get_bind_data();
        let mut match_ref = true;
        if matches!(
            element,
            XfaElement::Subform | XfaElement::ExclGroup | XfaElement::Field
        ) {
            let template = form.get_template_node_if_exists();
            let bind = template
                .as_ref()
                .and_then(|node| node.get_first_child_by_class(XfaElement::Bind));
            let match_type = bind.as_ref().map_or(XfaAttributeValue::Once, |bind| {
                bind.get_enum(XfaAttribute::Match)
            });
            match match_type {
                XfaAttributeValue::None => {
                    if !data_ref || parent_data_ref {
                        form_value_node_match_none_create_child(form)?;
                    }
                }
                XfaAttributeValue::Once => {
                    if !data_ref || parent_data_ref {
                        if data.is_none() {
                            if form.get_name_hash() != 0
                                && form.get_enum(XfaAttribute::Scope) != XfaAttributeValue::None
                            {
                                let data_type = if element == XfaElement::Subform
                                    || form.is_field_multi_list_box()
                                {
                                    XfaElement::DataGroup
                                } else {
                                    XfaElement::DataValue
                                };
                                data = self.maybe_create_data_node(
                                    scope,
                                    data_type,
                                    &form.get_cdata(XfaAttribute::Name),
                                )?;
                                if let Some(data) = &data {
                                    create_data_binding(form, data, false)?;
                                }
                            }
                            if data.is_none() {
                                form_value_node_match_none_create_child(form)?;
                            }
                        } else if let (Some(data_node), Some(scope)) = (data.as_ref(), scope) {
                            if data_node.parent().as_ref() != Some(scope) {
                                if let Some(parent) = data_node.parent() {
                                    parent.remove_child_and_notify(data_node, true);
                                }
                                if !scope.insert_child_and_notify(-1, data_node) {
                                    return Err(DataMergeError::InvalidTree(
                                        "cannot move a bound dataset into its scope",
                                    ));
                                }
                            }
                        }
                    }
                }
                XfaAttributeValue::Global => {
                    if !data_ref || parent_data_ref {
                        let hash = form.get_name_hash();
                        if hash != 0 && data.is_none() {
                            data = self.document.get_global_binding(hash);
                            if let Some(data) = &data {
                                create_data_binding(form, data, true)?;
                            } else {
                                let data_type = if element == XfaElement::Subform
                                    || form.is_field_multi_list_box()
                                {
                                    XfaElement::DataGroup
                                } else {
                                    XfaElement::DataValue
                                };
                                let record = self.document.get_xfa_node(XfaHashCode::Record as u32);
                                data = self.maybe_create_data_node(
                                    record.as_ref(),
                                    data_type,
                                    &form.get_cdata(XfaAttribute::Name),
                                )?;
                                if let Some(data) = &data {
                                    create_data_binding(form, data, false)?;
                                    self.document
                                        .register_global_binding(hash, Some(data.clone()));
                                }
                            }
                        }
                        if data.is_none() {
                            form_value_node_match_none_create_child(form)?;
                        }
                    }
                }
                XfaAttributeValue::DataRef => {
                    match_ref = data_ref;
                    parent_data_ref = true;
                    if data.is_none() && data_ref {
                        let reference = bind
                            .as_ref()
                            .map_or_else(Vec::new, |bind| bind.get_cdata(XfaAttribute::Ref));
                        let result = self.resolver.resolve_objects_with_bind_node(
                            scope,
                            &reference,
                            XfaResolveFlags::CHILDREN | XfaResolveFlags::CREATE_NODE,
                            template.as_ref(),
                        )?;
                        data = result
                            .as_ref()
                            .and_then(|result| result.objects.first().cloned());
                        if let Some(data) = &data {
                            create_data_binding(
                                form,
                                data,
                                result.is_some_and(|result| {
                                    result.result_type == ResolveResultType::ExistNodes
                                }),
                            )?;
                        } else {
                            form_value_node_match_none_create_child(form)?;
                        }
                    }
                }
                _ => {}
            }
        }
        if match_ref
            && matches!(
                element,
                XfaElement::Subform
                    | XfaElement::SubformSet
                    | XfaElement::Area
                    | XfaElement::PageArea
                    | XfaElement::PageSet
            )
        {
            for child in children(form) {
                if child.is_container_node() && !child.is_unused_node() {
                    self.update_binding_relations(
                        &child,
                        data.as_ref().or(scope),
                        data_ref,
                        parent_data_ref,
                    )?;
                }
            }
        }
        Ok(())
    }
    fn update_data_relation(&self, data: &CxfaNode, description: &CxfaNode) {
        // Source recursion is a DFS over actual matching data-description pairs.
        let mut stack = vec![(data.clone(), description.clone())];
        while let Some((data, description)) = stack.pop() {
            let mut matched_children = Vec::new();
            for child in children(&data) {
                let hash = child.get_name_hash();
                if hash == 0 {
                    continue;
                }
                let mut iterator = DdGroupIterator::new(Some(description.clone()));
                while let Some(group) = iterator.get_current() {
                    iterator.move_to_next();
                    if group != description
                        && (group.get_element_type() != XfaElement::DataGroup
                            || !group.try_namespace().is_some_and(|ns| {
                                ascii(&ns, "http://ns.adobe.com/data-description/")
                            }))
                    {
                        continue;
                    }
                    let Some(node) = group.get_first_child_by_name_hash(hash) else {
                        continue;
                    };
                    if node.get_element_type() != child.get_element_type() {
                        break;
                    }
                    child.set_data_description_node(Some(&node));
                    matched_children.push((child.clone(), node));
                    break;
                }
            }
            stack.extend(matched_children.into_iter().rev());
        }
    }
    fn do_data_merge(&self) -> Result<(), DataMergeError> {
        let root = self
            .document
            .get_root()
            .ok_or(DataMergeError::MissingRoot)?;
        let datasets =
            if let Some(datasets) = self.document.get_xfa_node(XfaHashCode::Datasets as u32) {
                datasets
            } else {
                let xml = self.xml_element(&wide("xfa:datasets"))?;
                xml.set_attribute(
                    &wide("xmlns:xfa"),
                    &wide("http://www.xfa.org/schema/xfa-data/1.0/"),
                );
                let datasets = self.new_node(XfaPacketType::Datasets, XfaElement::DataModel)?;
                datasets.set_cdata(XfaAttribute::Name, &wide("datasets"));
                let root_xml = root
                    .get_xml_mapping_node()
                    .ok_or(DataMergeError::MissingXmlDocument)?;
                root_xml.append_last_child(&xml);
                root.insert_child_and_notify(-1, &datasets);
                datasets.set_xml_mapping_node(xml);
                datasets
            };
        let mut data_root = None;
        let mut description_root = None;
        let datasets_uri = datasets.try_namespace().unwrap_or_default();
        for child in children(&datasets) {
            if child.get_element_type() != XfaElement::DataGroup {
                continue;
            }
            if description_root.is_none()
                && child.get_name_hash() == XfaHashCode::DataDescription as u32
            {
                if child
                    .try_namespace()
                    .is_some_and(|ns| ascii(&ns, "http://ns.adobe.com/data-description/"))
                {
                    description_root = Some(child.clone());
                }
            } else if data_root.is_none() && child.get_name_hash() == XfaHashCode::Data as u32 {
                if child.try_namespace().is_some_and(|ns| ns == datasets_uri) {
                    data_root = Some(child);
                }
            }
            if data_root.is_some() && description_root.is_some() {
                break;
            }
        }
        let data_root = if let Some(data) = data_root {
            data
        } else {
            let data = self.new_node(XfaPacketType::Datasets, XfaElement::DataGroup)?;
            data.set_cdata(XfaAttribute::Name, &wide("data"));
            let xml = self.xml_element(&wide("xfa:data"))?;
            // A datasets packet may declare its namespace as the default rather
            // than with the xfa prefix. Keep the newly introduced prefix bound.
            let namespace = datasets
                .get_xml_mapping_node()
                .map(|xml| xml.namespace_uri().to_vec())
                .unwrap_or_default();
            if !namespace.is_empty() {
                xml.set_attribute(&wide("xmlns:xfa"), &namespace);
            }
            data.set_xml_mapping_node(xml);
            datasets.insert_child_and_notify(-1, &data);
            data
        };
        let mut top_level = data_root.get_first_child_by_class(XfaElement::DataGroup);
        let Some(template_root) = root.get_first_child_by_class(XfaElement::Template) else {
            return Ok(());
        };
        let name_hash = top_level.as_ref().map_or(0, CxfaNode::get_name_hash);
        let chosen = if name_hash != 0 {
            template_root
                .get_first_child_by_name_hash(name_hash)
                .filter(|node| node.get_element_type() == XfaElement::Subform)
        } else {
            None
        }
        .or_else(|| template_root.get_first_child_by_class(XfaElement::Subform));
        let Some(chosen) = chosen else {
            return Ok(());
        };
        let (form_root, empty_form) =
            if let Some(form) = root.get_first_child_by_class(XfaElement::Form) {
                let mut iterator = CxfaNodeIterator::new(Some(form.clone()));
                let mut current = iterator.move_to_next();
                while let Some(node) = current {
                    node.set_flag(XfaNodeFlag::UnusedNode);
                    current = iterator.move_to_next();
                }
                (form, false)
            } else {
                let form = self.new_node(XfaPacketType::Form, XfaElement::Form)?;
                form.set_cdata(XfaAttribute::Name, &wide("form"));
                root.insert_child_and_notify(-1, &form);
                (form, true)
            };
        let form = self.clone_container(&form_root, &chosen, false, None)?;
        if top_level.is_none() {
            let mut name = form.get_cdata(XfaAttribute::Name);
            if name.is_empty() {
                name = wide("form");
            }
            let data = self.new_node(XfaPacketType::Datasets, XfaElement::DataGroup)?;
            data.set_cdata(XfaAttribute::Name, &name);
            data.set_xml_mapping_node(self.xml_element(&name)?);
            data_root.insert_child_before_and_notify(&data, data_root.first_child().as_ref());
            top_level = Some(data);
        }
        let data = top_level.ok_or(DataMergeError::InvalidTree(
            "data merge lost its top-level record",
        ))?;
        create_data_binding(&form, &data, true)?;
        self.copy_children(&chosen, &form, Some(&data), true, true, true)?;
        if let Some(description) = description_root {
            self.update_data_relation(&data_root, &description);
        }
        self.update_bindings(&form)?;
        let mut page_set = form.get_first_child_by_class(XfaElement::PageSet);
        while let Some(node) = page_set {
            self.document.append_pending_node(node.clone());
            page_set = node.get_next_same_class_sibling(XfaElement::PageSet);
            form.remove_child_and_notify(&node, true);
        }
        if empty_form {
            return Ok(());
        }
        let mut iterator = CxfaNodeIterator::new(Some(form_root));
        let mut current = iterator.move_to_next();
        while let Some(node) = current {
            if node.is_unused_node() {
                if node.is_container_node()
                    || node.get_element_type() == XfaElement::InstanceManager
                {
                    current = iterator.skip_children_and_move_to_next();
                    if let Some(parent) = node.parent() {
                        parent.remove_child_and_notify(&node, true);
                    }
                } else {
                    node.clear_flag(XfaNodeFlag::UnusedNode);
                    node.set_initialized_flag_and_notify();
                    current = iterator.move_to_next();
                }
            } else {
                node.set_initialized_flag_and_notify();
                current = iterator.move_to_next();
            }
        }
        Ok(())
    }
    fn update_bindings(&self, form: &CxfaNode) -> Result<(), DataMergeError> {
        let Some(scope) = data_merge_find_data_scope(form.parent().as_ref()) else {
            return Ok(());
        };
        self.update_binding_relations(form, Some(&scope), false, false)?;
        self.update_binding_relations(form, Some(&scope), true, false)
    }
}
fn clear_subtree_bindings(root: &CxfaNode) {
    let mut iterator = CxfaNodeIterator::new(Some(root.clone()));
    let mut current = iterator.get_current();
    while let Some(node) = current {
        if let Some(data) = node.get_bind_data() {
            data.remove_bind_item(&node);
            node.set_binding_node(None);
        }
        current = iterator.move_to_next();
    }
}
impl CxfaDocument {
    pub fn do_data_merge(self: &Rc<Self>) -> Result<(), DataMergeError> {
        let resolver = CfxJseResolveProcessor::new(self.clone());
        self.do_data_merge_with_resolver(&resolver)
    }
    pub fn do_data_merge_with_resolver(
        self: &Rc<Self>,
        resolver: &CfxJseResolveProcessor,
    ) -> Result<(), DataMergeError> {
        if !Rc::ptr_eq(self, resolver.document()) {
            return Err(DataMergeError::InvalidTree(
                "data merge resolver belongs to another document",
            ));
        }
        DataMerger::new(self, resolver).do_data_merge()
    }
    pub fn data_merge_copy_container(
        self: &Rc<Self>,
        template: &CxfaNode,
        parent: &CxfaNode,
        scope: Option<&CxfaNode>,
        one_instance: bool,
        data_merge: bool,
        up_level: bool,
    ) -> Result<Option<CxfaNode>, DataMergeError> {
        let resolver = CfxJseResolveProcessor::new(self.clone());
        self.data_merge_copy_container_with_resolver(
            &resolver,
            template,
            parent,
            scope,
            one_instance,
            data_merge,
            up_level,
        )
    }
    pub fn data_merge_copy_container_with_resolver(
        self: &Rc<Self>,
        resolver: &CfxJseResolveProcessor,
        template: &CxfaNode,
        parent: &CxfaNode,
        scope: Option<&CxfaNode>,
        one_instance: bool,
        data_merge: bool,
        up_level: bool,
    ) -> Result<Option<CxfaNode>, DataMergeError> {
        if !Rc::ptr_eq(self, resolver.document()) {
            return Err(DataMergeError::InvalidTree(
                "data merge resolver belongs to another document",
            ));
        }
        DataMerger::new(self, resolver).copy_container(
            template,
            parent,
            scope,
            one_instance,
            data_merge,
            up_level,
        )
    }
    pub fn data_merge_update_binding_relations(
        self: &Rc<Self>,
        form: &CxfaNode,
    ) -> Result<(), DataMergeError> {
        let resolver = CfxJseResolveProcessor::new(self.clone());
        self.data_merge_update_binding_relations_with_resolver(&resolver, form)
    }
    pub fn data_merge_update_binding_relations_with_resolver(
        self: &Rc<Self>,
        resolver: &CfxJseResolveProcessor,
        form: &CxfaNode,
    ) -> Result<(), DataMergeError> {
        if !Rc::ptr_eq(self, resolver.document()) {
            return Err(DataMergeError::InvalidTree(
                "data merge resolver belongs to another document",
            ));
        }
        DataMerger::new(self, resolver).update_bindings(form)
    }
    pub fn do_data_remerge_model(self: &Rc<Self>) -> Result<(), DataMergeError> {
        let resolver = CfxJseResolveProcessor::new(self.clone());
        self.do_data_remerge_model_with_resolver(&resolver)
    }
    pub fn do_data_remerge_model_with_resolver(
        self: &Rc<Self>,
        resolver: &CfxJseResolveProcessor,
    ) -> Result<(), DataMergeError> {
        if !Rc::ptr_eq(self, resolver.document()) {
            return Err(DataMergeError::InvalidTree(
                "data merge resolver belongs to another document",
            ));
        }
        if let Some(form) = self.get_xfa_node(XfaHashCode::Form as u32) {
            // Detaching a form leaves the document's node allocations alive.
            // Remove its binding pairs before matching replacement instances,
            // so stale detached forms cannot consume the same datasets forever.
            clear_subtree_bindings(&form);
            while let Some(child) = form.first_child() {
                form.remove_child_and_notify(&child, true);
            }
            form.set_binding_node(None);
        }
        self.clear_global_bindings();
        self.do_data_merge_with_resolver(resolver)
    }
    pub fn do_data_remerge(
        self: &Rc<Self>,
        layout: &dyn DataMergeLayoutHost,
    ) -> Result<(), DataMergeError> {
        let resolver = CfxJseResolveProcessor::new(self.clone());
        self.do_data_remerge_with_resolver(&resolver, layout)
    }
    pub fn do_data_remerge_with_resolver(
        self: &Rc<Self>,
        resolver: &CfxJseResolveProcessor,
        layout: &dyn DataMergeLayoutHost,
    ) -> Result<(), DataMergeError> {
        self.do_data_remerge_model_with_resolver(resolver)?;
        layout.set_force_relayout();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::cxfa_document_builder::CxfaDocumentBuilder;
    use super::*;
    use crate::fxcrt::{
        cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser,
    };
    use std::cell::{Cell, RefCell};
    fn parsed(input: &str) -> Rc<CxfaDocument> {
        let mut bytes = input.as_bytes().to_vec();
        bytes.push(0);
        let xml = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
            .parse()
            .unwrap();
        let document = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(document.clone());
        assert!(builder.build_document(Rc::new(RefCell::new(xml)), XfaPacketType::Xdp));
        document.set_root(builder.get_root_node());
        document
    }
    fn fixture(template: &str, record: &str) -> Rc<CxfaDocument> {
        parsed(&format!("<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'><template xmlns='http://www.xfa.org/schema/xfa-template/2.8/'><subform name='record'>{template}</subform></template><xfa:datasets xmlns:xfa='http://www.xfa.org/schema/xfa-data/1.0/'><xfa:data><record>{record}</record></xfa:data></xfa:datasets></xdp:xdp>"))
    }
    fn form(document: &CxfaDocument) -> CxfaNode {
        document
            .get_xfa_node(XfaHashCode::Form as u32)
            .unwrap()
            .get_first_child_by_class(XfaElement::Subform)
            .unwrap()
    }
    fn record(document: &CxfaDocument) -> CxfaNode {
        document.get_xfa_node(XfaHashCode::Record as u32).unwrap()
    }
    fn named(parent: &CxfaNode, name: &str) -> CxfaNode {
        parent.get_first_child_by_name(&wide(name)).unwrap()
    }
    fn fields(parent: &CxfaNode) -> Vec<CxfaNode> {
        children(parent)
            .filter(|node| node.get_element_type() == XfaElement::Field)
            .collect()
    }
    #[test]
    fn once_bindings_consume_distinct_values_and_keep_identity() {
        let document=fixture("<field name='item'><ui><textEdit/></ui></field><field name='item'><ui><textEdit/></ui></field>","<item>first</item><item>second</item>");
        let original = record(&document).get_node_list_for_type(XfaElement::DataValue);
        document.do_data_merge().unwrap();
        let fields = fields(&form(&document));
        assert_eq!(fields.len(), 2);
        for (field, data) in fields.iter().zip(&original) {
            assert_eq!(field.get_bind_data(), Some(data.clone()));
            assert_eq!(data.get_bind_items_copy(), [field.clone()]);
        }
        assert_eq!(fields[0].get_content(), wide("first"));
        assert_eq!(fields[1].get_content(), wide("second"));
        assert_eq!(
            record(&document).get_node_list_for_type(XfaElement::DataValue),
            original
        );
    }
    #[test]
    fn repeat_instances_follow_records_and_occur_limits() {
        let template="<subform name='row'><occur min='0' max='-1' initial='0'/><field name='value'><ui><textEdit/></ui></field></subform>";
        let document=fixture(template,"<row><value>one</value></row><row><value>two</value></row><row><value>three</value></row>");
        document.do_data_merge().unwrap();
        let root = form(&document);
        let rows: Vec<_> = children(&root)
            .filter(|node| {
                node.get_element_type() == XfaElement::Subform
                    && node.get_name_hash() == hash_name(&wide("row"))
            })
            .collect();
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter()
                .map(|row| named(row, "value").get_content())
                .collect::<Vec<_>>(),
            [wide("one"), wide("two"), wide("three")]
        );
        let manager = named(&root, "_row");
        assert_eq!(manager.get_element_type(), XfaElement::InstanceManager);
        assert_eq!(
            manager.first_child().unwrap().occur_get_occur_info(),
            (0, -1, 0)
        );
        for row in rows {
            assert_eq!(row.get_instance_mgr_of_subform(), Some(manager.clone()));
            assert!(row.get_bind_data().is_some());
        }
        let limited=fixture(&template.replace("max='-1'","max='2'"),"<row><value>one</value></row><row><value>two</value></row><row><value>three</value></row>");
        limited.do_data_merge().unwrap();
        assert_eq!(
            children(&form(&limited))
                .filter(|node| node.get_element_type() == XfaElement::Subform)
                .count(),
            2
        );
    }
    #[test]
    fn initial_instances_and_none_binding_preserve_template_defaults() {
        let document=fixture("<subform name='row'><occur min='2' max='4' initial='3'/><field name='value'><ui><textEdit/></ui><value><text>default</text></value></field></subform><field name='local'><bind match='none'/><ui><textEdit/></ui><value><text>local</text></value></field><subform name='scoped' scope='none'><field name='scopedValue'><ui><textEdit/></ui><value><text>scoped</text></value></field></subform>","");
        document.do_data_merge().unwrap();
        let root = form(&document);
        let rows: Vec<_> = children(&root)
            .filter(|node| {
                node.get_element_type() == XfaElement::Subform
                    && node.get_name_hash() == hash_name(&wide("row"))
            })
            .collect();
        assert_eq!(rows.len(), 3);
        for row in rows {
            assert_eq!(named(&row, "value").get_content(), wide("default"));
            assert_eq!(
                named(&row.get_bind_data().unwrap(), "value").get_content(),
                wide("default")
            );
        }
        assert_eq!(named(&root, "local").get_content(), wide("local"));
        assert!(named(&root, "local").get_bind_data().is_none());
        assert!(record(&document)
            .get_first_child_by_name(&wide("local"))
            .is_none());
        assert!(named(&root, "scoped").get_bind_data().is_none());
        let scoped = named(&root, "scoped");
        assert_eq!(named(&scoped, "scopedValue").get_content(), wide("scoped"));
        assert_eq!(
            named(&record(&document), "scopedValue").get_content(),
            wide("scoped")
        );
        assert!(record(&document)
            .get_first_child_by_name(&wide("scoped"))
            .is_none());
    }
    #[test]
    fn global_and_data_ref_bindings_share_values_and_create_missing_paths() {
        let document=fixture("<field name='shared'><bind match='global'/><ui><textEdit/></ui></field><field name='shared'><bind match='global'/><ui><textEdit/></ui></field><field name='mirror'><bind match='dataRef' ref='$record.shared'/><ui><textEdit/></ui></field><field name='created'><bind match='dataRef' ref='nested.value'/><ui><textEdit/></ui><value><text>seed</text></value></field>","<shared>shared value</shared>");
        document.do_data_merge().unwrap();
        let root = form(&document);
        let shared = named(&record(&document), "shared");
        for field in fields(&root).into_iter().take(3) {
            assert_eq!(field.get_bind_data(), Some(shared.clone()));
            assert_eq!(field.get_content(), wide("shared value"));
        }
        assert_eq!(shared.get_bind_items_copy().len(), 3);
        assert_eq!(
            document.get_global_binding(hash_name(&wide("shared"))),
            Some(shared)
        );
        let created = named(&root, "created");
        let nested = named(&record(&document), "nested");
        let value = named(&nested, "value");
        assert_eq!(nested.get_element_type(), XfaElement::DataGroup);
        assert_eq!(created.get_bind_data(), Some(value.clone()));
        assert_eq!(value.get_content(), wide("seed"));
    }
    #[test]
    fn choice_and_unordered_sets_follow_dataset_order() {
        let children="<field name='a'><ui><textEdit/></ui></field><field name='b'><ui><textEdit/></ui></field><field name='c'><ui><textEdit/></ui><value><text>fallback</text></value></field>";
        for (relation, names) in [("choice", vec!["b"]), ("unordered", vec!["b", "a", "c"])] {
            let document = fixture(
                &format!("<subformSet name='set' relation='{relation}'>{children}</subformSet>"),
                "<b>B</b><a>A</a>",
            );
            document.do_data_merge().unwrap();
            let set = named(&form(&document), "set");
            assert_eq!(
                fields(&set)
                    .iter()
                    .map(|field| field.get_cdata(XfaAttribute::Name))
                    .collect::<Vec<_>>(),
                names.iter().map(|name| wide(name)).collect::<Vec<_>>()
            );
            assert_eq!(named(&set, "b").get_content(), wide("B"));
            if relation == "unordered" {
                assert_eq!(named(&set, "a").get_content(), wide("A"));
                assert_eq!(named(&set, "c").get_content(), wide("fallback"));
            }
        }
    }
    #[test]
    fn reused_unbound_sets_preserve_their_relation_and_local_form_values() {
        for (relation, names) in [("choice", vec!["a"]), ("unordered", vec!["a", "b"])] {
            let document = fixture(
                &format!(
                    "<subformSet name='set' relation='{relation}'>\
                     <field name='a'><bind match='none'/><ui><textEdit/></ui>\
                     <value><text>A</text></value></field>\
                     <field name='b'><bind match='none'/><ui><textEdit/></ui>\
                     <value><text>B</text></value></field></subformSet>"
                ),
                "<anchor/>",
            );
            for pass in 0..3 {
                document.do_data_merge().unwrap();
                let set = named(&form(&document), "set");
                assert_eq!(
                    fields(&set)
                        .iter()
                        .map(|field| field.get_cdata(XfaAttribute::Name))
                        .collect::<Vec<_>>(),
                    names.iter().map(|name| wide(name)).collect::<Vec<_>>()
                );
                let first = named(&set, "a");
                assert_eq!(
                    first.get_content(),
                    wide(if pass == 0 { "A" } else { "Edited" })
                );
                assert!(first.set_content(&wide("Edited"), &wide("Edited"), false));
                assert!(first.get_bind_data().is_none());
            }
        }
    }
    #[test]
    fn reused_data_driven_choices_can_switch_to_the_remaining_matching_record() {
        let document = fixture(
            "<subformSet name='set' relation='choice'>\
             <field name='a'><ui><textEdit/></ui></field>\
             <field name='b'><ui><textEdit/></ui></field></subformSet>",
            "<b>B</b><a>A</a>",
        );
        document.do_data_merge().unwrap();
        let set = named(&form(&document), "set");
        assert_eq!(fields(&set).len(), 1);
        assert_eq!(named(&set, "b").get_content(), wide("B"));
        let data = record(&document);
        let previous = named(&data, "b");
        let remaining = named(&data, "a");
        assert!(data.remove_child_and_notify(&previous, true));
        document.do_data_merge().unwrap();
        let set = named(&form(&document), "set");
        assert_eq!(fields(&set).len(), 1);
        let selected = named(&set, "a");
        assert_eq!(selected.get_content(), wide("A"));
        assert_eq!(selected.get_bind_data(), Some(remaining));
        assert!(set.get_first_child_by_name(&wide("b")).is_none());
    }
    #[test]
    fn data_descriptions_limit_new_nodes_and_bind_existing_descriptors() {
        let input="<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'><template><subform name='record'><field name='allowed'><ui><textEdit/></ui><value><text>yes</text></value></field><field name='blocked'><ui><textEdit/></ui><value><text>local</text></value></field><field name='meta'><ui><textEdit/></ui><value><text>attribute</text></value></field></subform></template><xfa:datasets xmlns:xfa='http://www.xfa.org/schema/xfa-data/1.0/'><xfa:data><record xfa:dataNode='dataGroup'/></xfa:data><dd:dataDescription xmlns:dd='http://ns.adobe.com/data-description/'><record meta=''><allowed/></record></dd:dataDescription></xfa:datasets></xdp:xdp>";
        let document = parsed(input);
        document.do_data_merge().unwrap();
        let data = record(&document);
        let allowed = named(&data, "allowed");
        assert_eq!(allowed.get_content(), wide("yes"));
        assert!(allowed.get_data_description_node().is_some());
        assert!(data.get_data_description_node().is_some());
        let metadata = named(&data, "meta");
        assert_eq!(
            metadata.get_enum(XfaAttribute::Contains),
            XfaAttributeValue::MetaData
        );
        assert_eq!(
            metadata.get_cdata(XfaAttribute::QualifiedName),
            wide("meta")
        );
        assert_eq!(metadata.get_xml_mapping_node(), data.get_xml_mapping_node());
        assert_eq!(
            data.get_xml_mapping_node()
                .unwrap()
                .get_attribute(&wide("meta"))
                .as_ref(),
            wide("attribute")
        );
        assert!(data.get_first_child_by_name(&wide("blocked")).is_none());
        assert!(named(&form(&document), "blocked").get_bind_data().is_none());
        assert_eq!(
            named(&form(&document), "blocked").get_content(),
            wide("local")
        );
    }
    #[test]
    fn missing_packets_default_namespace_and_pending_page_sets() {
        let input="<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'><template><subform name='record'><pageSet/><field name='value'><ui><textEdit/></ui><value><text>seed</text></value></field></subform></template></xdp:xdp>";
        let document = parsed(input);
        document.do_data_merge().unwrap();
        assert_eq!(
            named(&record(&document), "value").get_content(),
            wide("seed")
        );
        // Native DataMerge_CopyContainer explicitly skips template PageSet
        // containers. The pending queue extracts PageSets already in a saved
        // Form DOM; materializing new template page sets belongs to layout.
        assert_eq!(document.get_pending_nodes_count(), 0);
        assert!(form(&document)
            .get_first_child_by_class(XfaElement::PageSet)
            .is_none());
        let saved = input.replace(
            "</xdp:xdp>",
            "<form><subform name='record'><pageSet/></subform></form></xdp:xdp>",
        );
        let document = parsed(&saved);
        let saved_page_set = form(&document)
            .get_first_child_by_class(XfaElement::PageSet)
            .unwrap();
        document.do_data_merge().unwrap();
        assert_eq!(document.get_pending_nodes_count(), 1);
        let page_set = document.get_pending_node_at_index(0).unwrap();
        assert_eq!(page_set, saved_page_set);
        assert_eq!(page_set.get_element_type(), XfaElement::PageSet);
        assert!(page_set.parent().is_none());
        assert!(form(&document)
            .get_first_child_by_class(XfaElement::PageSet)
            .is_none());
        let document=parsed("<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'><template><subform name='record'><field name='value'><ui><textEdit/></ui></field></subform></template><datasets xmlns='http://www.xfa.org/schema/xfa-data/1.0/'/></xdp:xdp>");
        document.do_data_merge().unwrap();
        assert!(document.get_xfa_node(XfaHashCode::Data as u32).is_some());
        assert!(document.get_xfa_node(XfaHashCode::Record as u32).is_some());
    }
    #[test]
    fn remerge_unbinds_old_forms_preserves_data_and_calls_real_layout_host() {
        struct Layout(Cell<usize>);
        impl DataMergeLayoutHost for Layout {
            fn set_force_relayout(&self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let document = fixture(
            "<field name='value'><ui><textEdit/></ui></field>",
            "<value>kept</value>",
        );
        document.do_data_merge().unwrap();
        let old = named(&form(&document), "value");
        let data = old.get_bind_data().unwrap();
        let layout = Layout(Cell::new(0));
        document.do_data_remerge(&layout).unwrap();
        let new = named(&form(&document), "value");
        assert_ne!(old, new);
        assert!(old.get_bind_data().is_none());
        assert_eq!(new.get_bind_data(), Some(data.clone()));
        assert_eq!(data.get_bind_items_copy(), [new.clone()]);
        assert_eq!(new.get_content(), wide("kept"));
        assert_eq!(layout.0.get(), 1);
        document.do_data_remerge_model().unwrap();
        assert_eq!(layout.0.get(), 1);
        assert_eq!(named(&form(&document), "value").get_content(), wide("kept"));
    }
    #[test]
    fn excessive_merge_depth_errors_and_restores_the_session() {
        let document = CxfaDocument::new();
        let template = document
            .create_node(XfaPacketType::Template, XfaElement::Subform)
            .unwrap();
        let mut parent = template.clone();
        for _ in 0..MAX_MERGE_DEPTH + 2 {
            let child = document
                .create_node(XfaPacketType::Template, XfaElement::Subform)
                .unwrap();
            assert!(parent.insert_child_and_notify(-1, &child));
            parent = child;
        }
        let form = document
            .create_node(XfaPacketType::Form, XfaElement::Form)
            .unwrap();
        let resolver = CfxJseResolveProcessor::new(document.clone());
        let merger = DataMerger::new(&document, &resolver);
        assert!(matches!(
            merger.copy_container(&template, &form, None, false, false, true),
            Err(DataMergeError::InvalidTree(_))
        ));
        assert_eq!(merger.recursion_depth.get(), 0);
        let shallow = document
            .create_node(XfaPacketType::Template, XfaElement::Subform)
            .unwrap();
        assert!(merger
            .copy_container(&shallow, &form, None, false, false, true)
            .unwrap()
            .is_some());
        assert_eq!(merger.recursion_depth.get(), 0);
        // Exercise the second recursive entrypoint's same scoped error path.
        merger.recursion_depth.set(MAX_MERGE_DEPTH);
        assert!(matches!(
            merger.update_binding_relations(&form, None, false, false),
            Err(DataMergeError::InvalidTree(_))
        ));
        assert_eq!(merger.recursion_depth.get(), MAX_MERGE_DEPTH);
    }
}
