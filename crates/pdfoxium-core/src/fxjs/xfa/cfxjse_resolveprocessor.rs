// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! SOM resolution from `cfxjse_resolveprocessor.{h,cpp}` and the connected
//! `CFXJSE_Engine::ResolveObjects[WithBindNode]` driver. Actual parser nodes and
//! non-node pseudo-models retain distinct types; node-only callers project the
//! result explicitly. Predicates and CJX access use the real script host.
use super::cfxjse_nodehelper::CfxJseNodeHelper;
use crate::{
    fxcrt::{fx_extension::is_wide_space, fx_system::wtoi},
    xfa::fxfa::{
        fxfa_basic::{XfaElement, XfaHashCode},
        parser::{
            cxfa_document::CxfaDocument,
            cxfa_node::CxfaNode,
            xfa_basic_data::{
                find_script_attribute_descriptor, get_element_by_name, hash_name,
                ScriptAttributeDescriptor,
            },
        },
    },
};
use std::{
    fmt,
    ops::{BitOr, BitOrAssign},
    rc::Rc,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct XfaResolveFlags(pub u16);
impl XfaResolveFlags {
    pub const CHILDREN: Self = Self(1 << 0);
    pub const TAG_NAME: Self = Self(1 << 1);
    pub const ATTRIBUTES: Self = Self(1 << 2);
    pub const PROPERTIES: Self = Self(1 << 3);
    pub const SIBLINGS: Self = Self(1 << 5);
    pub const PARENT: Self = Self(1 << 6);
    pub const ANY_CHILD: Self = Self(1 << 7);
    pub const ALL: Self = Self(1 << 8);
    pub const CREATE_NODE: Self = Self(1 << 10);
    pub const BIND: Self = Self(1 << 11);
    pub const BIND_NEW: Self = Self(1 << 12);
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}
impl BitOr for XfaResolveFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
impl BitOrAssign for XfaResolveFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResolveResultType {
    #[default]
    Nodes,
    Attribute,
    CreateNodeOne,
    CreateNodeAll,
    CreateNodeMidAll,
    ExistNodes,
}
#[derive(Clone, Debug, Default)]
pub struct ResolveResult {
    pub result_type: ResolveResultType,
    pub script_attribute: Option<ScriptAttributeDescriptor>,
    pub objects: Vec<CxfaNode>,
}
#[derive(Clone, Debug)]
pub enum ResolveError {
    MalformedExpression,
    InvalidReference,
    InvalidIndex,
    MissingXmlDocument,
    Unsupported(&'static str),
    Script(boa_engine::JsError),
}
impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedExpression => f.write_str("malformed SOM expression"),
            Self::InvalidReference => {
                f.write_str("SOM reference is expired or belongs to another document")
            }
            Self::InvalidIndex => f.write_str("SOM index cannot create a negative number of nodes"),
            Self::MissingXmlDocument => {
                f.write_str("SOM data creation requires the document's XML owner")
            }
            Self::Unsupported(feature) => write!(f, "SOM requires a script host for {feature}"),
            Self::Script(error) => fmt::Display::fmt(error, f),
        }
    }
}
impl std::error::Error for ResolveError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SomScriptLanguage {
    Formcalc,
    Javascript,
}
/// A real engine adapter runs these operations through CfxJsEngine's existing
/// context/host-callback lease. No DOM, document or resolver borrow is held here.
/// Return predicate script failures as Ok(false), as RunScript does upstream;
/// errors represent an unavailable service or an adapter/runtime failure.
/// Existing node-only hosts can implement the node callbacks; actual engines
/// also implement typed object access without fabricating parser nodes.
pub trait SomScriptHost {
    fn this_object(&self) -> Result<Option<ResolvedObject>, ResolveError> {
        self.this_node().map(|node| node.map(ResolvedObject::Node))
    }
    fn attribute_object(
        &self,
        object: &ResolvedObject,
        attribute: ScriptAttributeDescriptor,
    ) -> Result<Option<ResolvedObject>, ResolveError> {
        match object {
            ResolvedObject::Node(node) => self
                .attribute_node(node, attribute)
                .map(|node| node.map(ResolvedObject::Node)),
            ResolvedObject::Pseudo(_) => {
                Err(ResolveError::Unsupported("non-node attribute access"))
            }
        }
    }
    fn predicate_object(
        &self,
        language: SomScriptLanguage,
        script: &[u32],
        object: &ResolvedObject,
    ) -> Result<bool, ResolveError> {
        match object {
            ResolvedObject::Node(node) => self.predicate(language, script, node),
            ResolvedObject::Pseudo(_) => {
                Err(ResolveError::Unsupported("non-node predicate execution"))
            }
        }
    }
    fn this_node(&self) -> Result<Option<CxfaNode>, ResolveError>;
    fn attribute_node(
        &self,
        node: &CxfaNode,
        attribute: ScriptAttributeDescriptor,
    ) -> Result<Option<CxfaNode>, ResolveError>;
    fn predicate(
        &self,
        language: SomScriptLanguage,
        script: &[u32],
        node: &CxfaNode,
    ) -> Result<bool, ResolveError>;
}
#[derive(Clone, Debug)]
pub struct NodeData {
    pub cur_object: Option<CxfaNode>,
    pub name: Vec<u32>,
    pub condition: Vec<u32>,
    pub hash_name: u32,
    pub level: usize,
    pub styles: XfaResolveFlags,
    pub result: ResolveResult,
}
impl Default for NodeData {
    fn default() -> Self {
        Self {
            cur_object: None,
            name: Vec::new(),
            condition: Vec::new(),
            hash_name: 0,
            level: 0,
            styles: XfaResolveFlags::CHILDREN,
            result: ResolveResult::default(),
        }
    }
}
fn trim(text: &[u32]) -> &[u32] {
    let start = text
        .iter()
        .position(|&ch| !is_wide_space(ch))
        .unwrap_or(text.len());
    let end = text
        .iter()
        .rposition(|&ch| !is_wide_space(ch))
        .map_or(start, |i| i + 1);
    &text[start..end]
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

pub struct CfxJseResolveProcessor {
    document: Rc<CxfaDocument>,
    host: Option<Rc<dyn SomScriptHost>>,
}
impl CfxJseResolveProcessor {
    pub fn new(document: Rc<CxfaDocument>) -> Self {
        Self {
            document,
            host: None,
        }
    }
    pub fn with_host(document: Rc<CxfaDocument>, host: Rc<dyn SomScriptHost>) -> Self {
        Self {
            document,
            host: Some(host),
        }
    }
    pub fn document(&self) -> &Rc<CxfaDocument> {
        &self.document
    }
    pub fn resolve_objects(
        &self,
        reference: Option<&CxfaNode>,
        expression: &[u32],
        styles: XfaResolveFlags,
    ) -> Result<Option<ResolveResult>, ResolveError> {
        self.resolve_objects_with_bind_node(reference, expression, styles, None)
    }
    pub fn resolve_objects_with_bind_node(
        &self,
        reference: Option<&CxfaNode>,
        expression: &[u32],
        styles: XfaResolveFlags,
        bind_node: Option<&CxfaNode>,
    ) -> Result<Option<ResolveResult>, ResolveError> {
        if expression.is_empty() {
            return Ok(None);
        }
        if let Some(reference) = reference {
            self.check_reference(reference)?;
        }
        if let Some(bind_node) = bind_node {
            self.check_reference(bind_node)?;
        }
        let reference = reference.cloned().or_else(|| self.document.get_root());
        let Some(reference) = reference else {
            return Ok(None);
        };
        let mut session = ResolveSession {
            processor: self,
            node_helper: CfxJseNodeHelper::new(),
            up_objects: Vec::new(),
            cur_start: 0,
        };
        if styles.intersects(XfaResolveFlags::PARENT | XfaResolveFlags::SIBLINGS) {
            session.up_objects.push(reference.clone());
        }
        if styles.contains(XfaResolveFlags::CREATE_NODE) {
            session.node_helper.set_create_node_type(bind_node);
        }
        session.resolve_objects(reference, expression, styles)
    }
    fn check_reference(&self, node: &CxfaNode) -> Result<(), ResolveError> {
        if !node.is_alive()
            || node
                .document()
                .is_none_or(|doc| !Rc::ptr_eq(&doc, &self.document))
        {
            Err(ResolveError::InvalidReference)
        } else {
            Ok(())
        }
    }
    fn checked_host_node(&self, node: Option<CxfaNode>) -> Result<Option<CxfaNode>, ResolveError> {
        if let Some(node) = &node {
            self.check_reference(node)?;
        }
        Ok(node)
    }
    fn named_node(&self, name: &[u32]) -> Result<Option<CxfaNode>, ResolveError> {
        let hash = hash_name(name);
        Ok(self.document.get_xfa_node(hash))
    }
    /// Parse one native SOM filter. None means end-of-expression; malformed
    /// delimiter nesting is an explicit error rather than a partially resolved path.
    pub fn get_filter(
        expression: &[u32],
        start: usize,
        rnd: &mut NodeData,
    ) -> Result<Option<usize>, ResolveError> {
        if start >= expression.len() {
            return Ok(None);
        }
        rnd.name.clear();
        rnd.condition.clear();
        let mut cursor = start;
        let mut stack = Vec::new();
        let mut kind = -1i8;
        let mut previous = 0;
        let mut is_condition = false;
        while cursor < expression.len() {
            let current = expression[cursor];
            cursor += 1;
            if current == 46 {
                if rnd.name.is_empty() {
                    rnd.styles |= XfaResolveFlags::ANY_CHILD;
                    continue;
                }
                if previous == 92 {
                    if let Some(last) = rnd.name.last_mut() {
                        *last = 46;
                    }
                    previous = 46;
                    continue;
                }
                let lookahead = expression.get(cursor).copied().unwrap_or(0);
                if lookahead != 91 && lookahead != 40 && kind < 0 {
                    break;
                }
            }
            if current == 91
                || current == 40
                || (current == 46
                    && expression
                        .get(cursor)
                        .is_some_and(|ch| matches!(ch, 91 | 40)))
            {
                is_condition = true;
            }
            if is_condition {
                rnd.condition.push(current);
            } else {
                rnd.name.push(current);
            }
            if (kind == 0 && current == 93)
                || (kind == 1 && current == 41)
                || (kind == 2 && current == 34)
            {
                kind = stack.pop().unwrap_or(-1);
            } else if current == 91 {
                stack.push(kind);
                kind = 0;
            } else if current == 40 {
                stack.push(kind);
                kind = 1;
            } else if current == 34 {
                stack.push(kind);
                kind = 2;
            }
            previous = current;
        }
        if !stack.is_empty() {
            return Err(ResolveError::MalformedExpression);
        }
        rnd.name = trim(&rnd.name).to_vec();
        rnd.condition = trim(&rnd.condition).to_vec();
        rnd.hash_name = hash_name(&rnd.name);
        Ok(Some(cursor))
    }
}
struct ResolveSession<'a> {
    processor: &'a CfxJseResolveProcessor,
    node_helper: CfxJseNodeHelper,
    up_objects: Vec<CxfaNode>,
    cur_start: usize,
}
impl ResolveSession<'_> {
    fn resolve_objects(
        &mut self,
        reference: CxfaNode,
        expression: &[u32],
        styles: XfaResolveFlags,
    ) -> Result<Option<ResolveResult>, ResolveError> {
        Ok(self
            .resolve_object_values(ResolvedObject::Node(reference), expression, styles)?
            .and_then(ObjectResolveResult::into_node_result))
    }
    fn index_for_data_bind(&mut self, next_condition: &[u32], count: usize) -> usize {
        if self.node_helper.create_node_for_condition(next_condition)
            && self.node_helper.last_create_type == XfaElement::DataGroup
        {
            0
        } else {
            count - 1
        }
    }
    fn resolve(&mut self, rnd: &mut NodeData) -> Result<bool, ResolveError> {
        let Some(current) = rnd.cur_object.clone() else {
            return Ok(false);
        };
        self.processor.check_reference(&current)?;
        if rnd.styles.contains(XfaResolveFlags::ANY_CHILD) {
            return self.resolve_any_child(rnd);
        }
        match rnd.name.first().copied().unwrap_or(0) {
            36 => return self.resolve_dollar(rnd),
            33 => return self.resolve_exclamatory(rnd),
            35 => return self.resolve_number_sign(rnd),
            42 => return Ok(self.resolve_asterisk(rnd)),
            46 => return self.resolve_any_child(rnd),
            _ => {}
        }
        if rnd.hash_name == XfaHashCode::This as u32 && rnd.level == 0 {
            let Some(host) = &self.processor.host else {
                return Err(ResolveError::Unsupported("active script this object"));
            };
            if let Some(node) = self.processor.checked_host_node(host.this_node()?)? {
                rnd.result.objects.push(node);
            }
            return Ok(!rnd.result.objects.is_empty());
        }
        if current.get_element_type() == XfaElement::Xfa {
            if let Some(node) = self.processor.named_node(&rnd.name)? {
                rnd.result.objects.push(node);
            } else if rnd.hash_name == XfaHashCode::Xfa as u32 {
                rnd.result.objects.push(current.clone());
            } else if rnd.styles.contains(XfaResolveFlags::ATTRIBUTES)
                && Self::resolve_attribute(&current, &mut rnd.result, &rnd.name)
            {
                return Ok(true);
            }
            if !rnd.result.objects.is_empty() {
                self.filter_condition(rnd)?;
            }
            return Ok(!rnd.result.objects.is_empty());
        }
        if !self.resolve_normal(rnd)? && rnd.hash_name == XfaHashCode::Xfa as u32 {
            if let Some(root) = self.processor.document.get_root() {
                rnd.result.objects.push(root);
            }
        }
        Ok(!rnd.result.objects.is_empty())
    }
    fn resolve_any_child(&mut self, rnd: &mut NodeData) -> Result<bool, ResolveError> {
        let Some(parent) = rnd.cur_object.clone() else {
            return Ok(false);
        };
        let class_name = rnd.name.first() == Some(&35);
        let child = if class_name {
            parent.get_one_child_of_class(&rnd.name[1..])
        } else {
            parent.get_one_child_named(&rnd.name)
        };
        let Some(child) = child else {
            return Ok(false);
        };
        if rnd.condition.is_empty() {
            rnd.result.objects.push(child);
            return Ok(true);
        }
        rnd.result.objects.extend(child.get_siblings(class_name));
        self.filter_condition(rnd)?;
        Ok(!rnd.result.objects.is_empty())
    }
    fn resolve_dollar(&mut self, rnd: &mut NodeData) -> Result<bool, ResolveError> {
        if rnd.name.len() == 1 {
            if let Some(node) = rnd.cur_object.clone() {
                rnd.result.objects.push(node);
            }
            return Ok(true);
        }
        if rnd.level > 0 {
            return Ok(false);
        }
        let name = &rnd.name[1..];
        if hash_name(name) == XfaHashCode::Xfa as u32 {
            if let Some(root) = self.processor.document.get_root() {
                rnd.result.objects.push(root);
            }
        } else if let Some(node) = self.processor.named_node(name)? {
            rnd.result.objects.push(node);
        }
        if !rnd.result.objects.is_empty() {
            self.filter_condition(rnd)?;
        }
        Ok(!rnd.result.objects.is_empty())
    }
    fn resolve_exclamatory(&mut self, rnd: &mut NodeData) -> Result<bool, ResolveError> {
        if rnd.level > 0 {
            return Ok(false);
        }
        let Some(datasets) = self
            .processor
            .document
            .get_xfa_node(XfaHashCode::Datasets as u32)
        else {
            return Ok(false);
        };
        let mut find = NodeData {
            cur_object: Some(datasets),
            name: rnd.name[1..].to_vec(),
            condition: rnd.condition.clone(),
            level: rnd.level + 1,
            styles: XfaResolveFlags::CHILDREN,
            ..NodeData::default()
        };
        find.hash_name = hash_name(&find.name);
        self.resolve(&mut find)?;
        rnd.result.objects.append(&mut find.result.objects);
        Ok(!rnd.result.objects.is_empty())
    }
    fn resolve_number_sign(&mut self, rnd: &mut NodeData) -> Result<bool, ResolveError> {
        let Some(current) = rnd.cur_object.clone() else {
            return Ok(false);
        };
        let name = &rnd.name[1..];
        if Self::resolve_attribute(&current, &mut rnd.result, name) {
            return Ok(true);
        }
        let mut styles = rnd.styles | XfaResolveFlags::TAG_NAME;
        styles.remove(XfaResolveFlags::ATTRIBUTES);
        let mut find = NodeData {
            cur_object: Some(current.clone()),
            name: name.to_vec(),
            condition: rnd.condition.clone(),
            hash_name: hash_name(name),
            level: rnd.level + 1,
            styles,
            ..NodeData::default()
        };
        self.resolve_normal(&mut find)?;
        if find.result.objects.is_empty() {
            return Ok(false);
        }
        if rnd.condition.is_empty() && find.result.objects.contains(&current) {
            rnd.result.objects.push(current);
        } else {
            rnd.result.objects.append(&mut find.result.objects);
        }
        Ok(!rnd.result.objects.is_empty())
    }
    fn resolve_attribute(current: &CxfaNode, result: &mut ResolveResult, name: &[u32]) -> bool {
        let Some(info) = find_script_attribute_descriptor(current.get_element_type(), name) else {
            return false;
        };
        result.result_type = ResolveResultType::Attribute;
        result.script_attribute = Some(info);
        result.objects.push(current.clone());
        true
    }
    fn resolve_asterisk(&self, rnd: &mut NodeData) -> bool {
        if let Some(node) = &rnd.cur_object {
            rnd.result.objects.extend(node.get_node_list_with_filter(3));
        }
        !rnd.result.objects.is_empty()
    }
    fn child_styles(parent: XfaResolveFlags) -> XfaResolveFlags {
        let mut styles = XfaResolveFlags::CHILDREN | XfaResolveFlags::ALL;
        if parent.contains(XfaResolveFlags::TAG_NAME) {
            styles |= XfaResolveFlags::TAG_NAME;
        }
        styles
    }
    fn widen_transparent_siblings(rnd: &mut NodeData, transparent: bool) {
        if !transparent {
            return;
        }
        if let Some(first) = rnd.result.objects.first().cloned() {
            let siblings = first.get_siblings(rnd.styles.contains(XfaResolveFlags::TAG_NAME));
            if siblings.len() > rnd.result.objects.len() {
                rnd.result.objects = siblings;
                rnd.result.objects[0] = first;
            }
        }
    }
    fn resolve_normal(&mut self, rnd: &mut NodeData) -> Result<bool, ResolveError> {
        if rnd.level > 32 {
            return Ok(false);
        }
        let Some(current) = rnd.cur_object.clone() else {
            return Ok(false);
        };
        let initial_count = rnd.result.objects.len();
        let styles = rnd.styles;
        let hash = rnd.hash_name;
        let mut find = NodeData {
            name: rnd.name.clone(),
            condition: rnd.condition.clone(),
            level: rnd.level + 1,
            hash_name: hash,
            ..NodeData::default()
        };
        let mut child_nodes = Vec::new();
        let mut properties = Vec::new();
        let mut variables = None;
        let mut page_set = None;
        for child in children(&current) {
            match child.get_element_type() {
                XfaElement::Variables => {
                    variables = Some(child);
                    continue;
                }
                XfaElement::PageSet => {
                    page_set = Some(child);
                    continue;
                }
                _ => {}
            }
            if current.has_property(child.get_element_type()) {
                properties.push(child);
            } else {
                child_nodes.push(child);
            }
        }
        if styles.contains(XfaResolveFlags::PROPERTIES) {
            if let Some(variables) = variables {
                if variables.get_class_hash_code() == hash {
                    rnd.result.objects.push(variables);
                } else {
                    find.cur_object = Some(variables);
                    find.styles = Self::child_styles(styles);
                    let condition = std::mem::take(&mut find.condition);
                    self.resolve_normal(&mut find)?;
                    find.condition = condition;
                    rnd.result.objects.append(&mut find.result.objects);
                }
                if rnd.result.objects.len() > initial_count {
                    self.filter_condition(rnd)?;
                    return Ok(!rnd.result.objects.is_empty());
                }
            }
        }
        if styles.contains(XfaResolveFlags::CHILDREN) {
            if styles.contains(XfaResolveFlags::PROPERTIES) {
                if let Some(page_set) = page_set {
                    child_nodes.push(page_set);
                }
            }
            for child in child_nodes {
                let child_hash = if styles.contains(XfaResolveFlags::TAG_NAME) {
                    child.get_class_hash_code()
                } else {
                    child.get_name_hash()
                };
                if child_hash == hash {
                    rnd.result.objects.push(child.clone());
                }
                if child.get_element_type() != XfaElement::PageSet && child.is_transparent() {
                    find.styles = Self::child_styles(styles);
                    find.cur_object = Some(child);
                    let condition = std::mem::take(&mut find.condition);
                    self.resolve_normal(&mut find)?;
                    find.condition = condition;
                    rnd.result.objects.append(&mut find.result.objects);
                }
            }
            if rnd.result.objects.len() > initial_count {
                if !styles.contains(XfaResolveFlags::ALL) {
                    Self::widen_transparent_siblings(rnd, current.is_transparent());
                }
                self.filter_condition(rnd)?;
                return Ok(!rnd.result.objects.is_empty());
            }
        }
        if styles.contains(XfaResolveFlags::ATTRIBUTES)
            && Self::resolve_attribute(&current, &mut rnd.result, &rnd.name)
        {
            return Ok(true);
        }
        if styles.contains(XfaResolveFlags::PROPERTIES) {
            for property in properties {
                if property.is_unnamed() {
                    if property.get_class_hash_code() == hash {
                        rnd.result.objects.push(property);
                    }
                    continue;
                }
                if property.get_name_hash() == hash
                    && !matches!(
                        property.get_element_type(),
                        XfaElement::Extras | XfaElement::Items
                    )
                {
                    rnd.result.objects.push(property);
                }
            }
            if rnd.result.objects.len() > initial_count {
                self.filter_condition(rnd)?;
                return Ok(!rnd.result.objects.is_empty());
            }
            let property = if current.get_element_type() == XfaElement::Subform
                && hash == XfaHashCode::Occur as u32
            {
                current
                    .get_instance_mgr_of_subform()
                    .and_then(|manager| manager.get_or_create_property(0, XfaElement::Occur))
            } else {
                let element = get_element_by_name(&rnd.name);
                if element == XfaElement::PageSet {
                    current.get_property(0, element).0
                } else if element != XfaElement::Unknown {
                    current.get_or_create_property(0, element)
                } else {
                    None
                }
            };
            if let Some(property) = property {
                rnd.result.objects.push(property);
                return Ok(true);
            }
        }
        let Some(parent) = current.parent() else {
            if current.get_class_hash_code() == hash {
                rnd.result.objects.push(current);
                self.filter_condition(rnd)?;
                return Ok(!rnd.result.objects.is_empty());
            }
            return Ok(false);
        };
        if styles.contains(XfaResolveFlags::SIBLINGS) {
            let mut sub_styles = XfaResolveFlags::CHILDREN | XfaResolveFlags::PROPERTIES;
            if styles.contains(XfaResolveFlags::TAG_NAME) {
                sub_styles |= XfaResolveFlags::TAG_NAME;
            }
            if styles.contains(XfaResolveFlags::ALL) {
                sub_styles |= XfaResolveFlags::ALL;
            }
            find.styles = sub_styles;
            for child in children(&parent) {
                if child == current {
                    if styles.contains(XfaResolveFlags::TAG_NAME) {
                        if current.get_class_hash_code() == hash {
                            rnd.result.objects.push(current.clone());
                        }
                    } else if child.get_name_hash() == hash {
                        rnd.result.objects.push(current.clone());
                        if rnd.level == 0 && rnd.condition.is_empty() {
                            rnd.result.objects.clear();
                            rnd.result.objects.push(current.clone());
                            return Ok(true);
                        }
                    }
                    continue;
                }
                let child_hash = if styles.contains(XfaResolveFlags::TAG_NAME) {
                    child.get_class_hash_code()
                } else {
                    child.get_name_hash()
                };
                if child_hash == hash {
                    rnd.result.objects.push(child.clone());
                }
                let inner = if parent.has_property(child.get_element_type()) {
                    matches!(
                        child.get_element_type(),
                        XfaElement::Variables | XfaElement::PageSet
                    )
                } else {
                    child.is_transparent()
                };
                if inner {
                    find.cur_object = Some(child);
                    let condition = std::mem::take(&mut find.condition);
                    let original = find.styles;
                    find.styles |= XfaResolveFlags::ALL;
                    self.resolve_normal(&mut find)?;
                    find.styles = original;
                    find.condition = condition;
                    rnd.result.objects.append(&mut find.result.objects);
                }
            }
            if rnd.result.objects.len() > initial_count {
                Self::widen_transparent_siblings(rnd, parent.is_transparent());
                self.filter_condition(rnd)?;
                return Ok(!rnd.result.objects.is_empty());
            }
        }
        if styles.contains(XfaResolveFlags::PARENT) {
            let mut sub_styles =
                XfaResolveFlags::SIBLINGS | XfaResolveFlags::PARENT | XfaResolveFlags::PROPERTIES;
            if styles.contains(XfaResolveFlags::TAG_NAME) {
                sub_styles |= XfaResolveFlags::TAG_NAME;
            }
            if styles.contains(XfaResolveFlags::ALL) {
                sub_styles |= XfaResolveFlags::ALL;
            }
            self.up_objects.push(parent.clone());
            find.styles = sub_styles;
            find.cur_object = Some(parent);
            self.resolve_normal(&mut find)?;
            rnd.result.objects.append(&mut find.result.objects);
            if rnd.result.objects.len() > initial_count {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn current_index(&self) -> usize {
        self.up_objects.last().map_or(0, |node| {
            let property = node.is_property();
            let class_index =
                node.is_unnamed() || (property && node.get_element_type() != XfaElement::PageSet);
            node.get_index(property, class_index)
        })
    }
    fn condition_array(
        &mut self,
        current_index: usize,
        rnd: &mut NodeData,
    ) -> Result<(), ResolveError> {
        let condition = &rnd.condition;
        let len = condition.len();
        let mut relative = false;
        let mut all = false;
        let mut i = 1;
        while i < len {
            let ch = condition[i];
            if ch == 32 {
                i += 1;
                continue;
            }
            if ch == 43 || ch == 45 {
                relative = true;
            } else if ch == 42 {
                all = true;
            }
            break;
        }
        if all {
            if rnd.styles.contains(XfaResolveFlags::CREATE_NODE) {
                if rnd.styles.contains(XfaResolveFlags::BIND) {
                    self.node_helper.create_parent = rnd.cur_object.clone();
                    self.node_helper.create_count = 1;
                    rnd.result.objects.clear();
                    self.node_helper.cur_all_start = None;
                    self.node_helper.all_start_parent = None;
                } else if self.node_helper.cur_all_start.is_none() {
                    self.node_helper.cur_all_start = Some(self.cur_start);
                    self.node_helper.all_start_parent = rnd.cur_object.clone();
                }
            } else if rnd.styles.contains(XfaResolveFlags::BIND_NEW)
                && self.node_helper.cur_all_start.is_none()
            {
                self.node_helper.cur_all_start = Some(self.cur_start);
            }
            return Ok(());
        }
        let end = len.saturating_sub(1);
        let Some(digits) = condition.get(i..end) else {
            return Err(ResolveError::MalformedExpression);
        };
        let mut index = i64::from(wtoi(digits));
        if relative {
            index = index
                .checked_add(i64::try_from(current_index).map_err(|_| ResolveError::InvalidIndex)?)
                .ok_or(ResolveError::InvalidIndex)?;
        }
        let count = rnd.result.objects.len();
        if index < 0 || usize::try_from(index).map_or(true, |index| index >= count) {
            if rnd.styles.contains(XfaResolveFlags::CREATE_NODE) {
                if index < 0 {
                    return Err(ResolveError::InvalidIndex);
                }
                self.node_helper.create_parent = rnd.cur_object.clone();
                self.node_helper.create_count = usize::try_from(index)
                    .map_err(|_| ResolveError::InvalidIndex)?
                    .checked_sub(count)
                    .and_then(|missing| missing.checked_add(1))
                    .ok_or(ResolveError::InvalidIndex)?;
            }
            rnd.result.objects.clear();
        } else {
            let node = rnd.result.objects[index as usize].clone();
            rnd.result.objects = vec![node];
        }
        Ok(())
    }
    fn filter_condition(&mut self, rnd: &mut NodeData) -> Result<(), ResolveError> {
        let current_index = self.current_index();
        rnd.condition = trim(&rnd.condition).to_vec();
        if rnd.condition.is_empty() {
            if rnd.styles.contains(XfaResolveFlags::ALL) || rnd.result.objects.len() == 1 {
                return Ok(());
            }
            let count = rnd.result.objects.len();
            if count <= current_index {
                if rnd.styles.contains(XfaResolveFlags::CREATE_NODE) {
                    self.node_helper.create_parent = rnd.cur_object.clone();
                    self.node_helper.create_count = current_index - count + 1;
                }
                rnd.result.objects.clear();
                return Ok(());
            }
            let node = rnd.result.objects[current_index].clone();
            rnd.result.objects = vec![node];
            return Ok(());
        }
        match rnd.condition[0] {
            91 => self.condition_array(current_index, rnd),
            46 if rnd.condition.get(1).is_some_and(|ch| matches!(ch, 91 | 40)) => {
                self.predicate_filter(rnd)
            }
            _ => Ok(()),
        }
    }
    fn predicate_filter(&self, rnd: &mut NodeData) -> Result<(), ResolveError> {
        let condition = &rnd.condition;
        let language = if condition.starts_with(&[46, 91]) && condition.last() == Some(&93) {
            SomScriptLanguage::Formcalc
        } else if condition.starts_with(&[46, 40]) && condition.last() == Some(&41) {
            SomScriptLanguage::Javascript
        } else {
            return Ok(());
        };
        let Some(host) = &self.processor.host else {
            return Err(ResolveError::Unsupported("predicate execution"));
        };
        let script = &condition[2..condition.len() - 1];
        for i in (0..rnd.result.objects.len()).rev() {
            let node = rnd.result.objects[i].clone();
            self.processor.check_reference(&node)?;
            if !host.predicate(language, script, &node)? {
                rnd.result.objects.remove(i);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fxcrt::xml::cfx_xmldocument::CfxXmlDocument,
        fxcrt::{
            cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser,
        },
        fxjs::{
            cfxjs_engine::CfxJsEngine,
            xfa::cfxjse_formcalc_context::{register_formcalc, translate},
        },
        xfa::fxfa::{
            fxfa_basic::{XfaAttribute, XfaAttributeValue, XfaPacketType},
            parser::cxfa_document_builder::CxfaDocumentBuilder,
        },
    };
    use boa_engine::{js_string, Context, JsValue, Source};
    use std::{cell::RefCell, rc::Weak};
    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    fn utf16(value: &[u32]) -> Vec<u16> {
        value
            .iter()
            .flat_map(|&ch| {
                char::from_u32(ch)
                    .unwrap()
                    .encode_utf16(&mut [0; 2])
                    .to_vec()
            })
            .collect()
    }
    fn node(
        doc: &Rc<CxfaDocument>,
        packet: XfaPacketType,
        element: XfaElement,
        name: &str,
        parent: Option<&CxfaNode>,
    ) -> CxfaNode {
        let node = doc.create_node(packet, element).unwrap();
        node.set_cdata(XfaAttribute::Name, &wide(name));
        if let Some(parent) = parent {
            assert!(parent.insert_child_and_notify(-1, &node));
        }
        node
    }
    fn tree() -> (Rc<CxfaDocument>, CxfaNode, CxfaNode) {
        let doc = CxfaDocument::new();
        let root = node(&doc, XfaPacketType::Xdp, XfaElement::Xfa, "xfa", None);
        doc.set_root(Some(root.clone()));
        let template = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Template,
            "template",
            Some(&root),
        );
        let form = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Subform,
            "form",
            Some(&template),
        );
        (doc, template, form)
    }
    const ORDINARY: XfaResolveFlags = XfaResolveFlags(
        XfaResolveFlags::CHILDREN.0
            | XfaResolveFlags::ATTRIBUTES.0
            | XfaResolveFlags::PROPERTIES.0
            | XfaResolveFlags::SIBLINGS.0
            | XfaResolveFlags::PARENT.0,
    );
    fn found(
        resolver: &CfxJseResolveProcessor,
        reference: &CxfaNode,
        expr: &str,
        flags: XfaResolveFlags,
    ) -> Vec<CxfaNode> {
        resolver
            .resolve_objects(Some(reference), &wide(expr), flags)
            .unwrap()
            .map_or_else(Vec::new, |r| r.objects)
    }

    #[test]
    fn filter_segments_conditions_escapes_and_errors() {
        let expression = wide(" form.a\\.b[1].field.(this.name == \"a.b\") ");
        let mut data = NodeData::default();
        let first = CfxJseResolveProcessor::get_filter(&expression, 0, &mut data)
            .unwrap()
            .unwrap();
        assert_eq!(data.name, wide("form"));
        let second = CfxJseResolveProcessor::get_filter(&expression, first, &mut data)
            .unwrap()
            .unwrap();
        assert_eq!(data.name, wide("a.b"));
        assert_eq!(data.condition, wide("[1]"));
        let end = CfxJseResolveProcessor::get_filter(&expression, second, &mut data)
            .unwrap()
            .unwrap();
        assert_eq!(data.name, wide("field"));
        assert_eq!(data.condition, wide(".(this.name == \"a.b\")"));
        assert_eq!(end, expression.len());
        assert!(
            CfxJseResolveProcessor::get_filter(&expression, end, &mut data)
                .unwrap()
                .is_none()
        );
        CfxJseResolveProcessor::get_filter(&wide("..field[*]"), 0, &mut data).unwrap();
        assert!(data.styles.contains(XfaResolveFlags::ANY_CHILD));
        assert_eq!(data.name, wide("field"));
        for invalid in ["a[0", "a.(true", "a.[1"] {
            assert!(matches!(
                CfxJseResolveProcessor::get_filter(&wide(invalid), 0, &mut NodeData::default()),
                Err(ResolveError::MalformedExpression)
            ));
        }
    }
    #[test]
    fn names_classes_wildcards_indices_and_ancestors() {
        let (doc, template, form) = tree();
        let a = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Field,
            "a",
            Some(&form),
        );
        let b = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Field,
            "a",
            Some(&form),
        );
        let dotted = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Field,
            "a.b",
            Some(&form),
        );
        let resolver = CfxJseResolveProcessor::new(doc);
        assert_eq!(
            found(&resolver, &form, "a", XfaResolveFlags::CHILDREN),
            [a.clone()]
        );
        assert_eq!(
            found(&resolver, &form, "a[*]", XfaResolveFlags::CHILDREN),
            [a.clone(), b.clone()]
        );
        assert_eq!(
            found(&resolver, &form, "a[1]", XfaResolveFlags::CHILDREN),
            [b.clone()]
        );
        assert!(found(&resolver, &form, "a[-1]", XfaResolveFlags::CHILDREN).is_empty());
        assert_eq!(found(&resolver, &b, "a[-1]", ORDINARY), [a.clone()]);
        assert_eq!(found(&resolver, &b, "a[+0]", ORDINARY), [b.clone()]);
        assert_eq!(found(&resolver, &b, "a", ORDINARY), [b.clone()]);
        assert_eq!(
            found(&resolver, &form, "#field[*]", XfaResolveFlags::CHILDREN),
            [a.clone(), b.clone(), dotted.clone()]
        );
        assert_eq!(
            found(&resolver, &form, "a\\.b", XfaResolveFlags::CHILDREN),
            [dotted.clone()]
        );
        assert_eq!(
            found(&resolver, &form, "*", XfaResolveFlags::CHILDREN),
            [a, b.clone(), dotted]
        );
        assert_eq!(found(&resolver, &b, "form", ORDINARY), [form.clone()]);
        assert_eq!(
            found(&resolver, &form, "$template.form.a[1]", ORDINARY),
            [b]
        );
        assert_eq!(found(&resolver, &form, "$", ORDINARY), [form]);
        assert_eq!(
            found(&resolver, &template, "xfa", ORDINARY),
            [resolver.document.get_root().unwrap()]
        );
    }
    #[test]
    fn transparent_descendants_variables_and_properties() {
        let (doc, _, form) = tree();
        let transparent = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Subform,
            "",
            Some(&form),
        );
        let field = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Field,
            "hidden",
            Some(&transparent),
        );
        let child = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Subform,
            "named",
            Some(&form),
        );
        let deep = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Field,
            "deep",
            Some(&child),
        );
        let variables = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Variables,
            "",
            Some(&form),
        );
        let variable = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Text,
            "constant",
            Some(&variables),
        );
        let resolver = CfxJseResolveProcessor::new(doc);
        assert_eq!(found(&resolver, &form, "hidden", ORDINARY), [field]);
        assert!(found(&resolver, &form, "deep", XfaResolveFlags::CHILDREN).is_empty());
        assert_eq!(
            found(&resolver, &form, "..deep", XfaResolveFlags::CHILDREN),
            [deep]
        );
        assert_eq!(
            found(&resolver, &form, "constant", XfaResolveFlags::PROPERTIES),
            [variable]
        );
        assert_eq!(
            found(&resolver, &form, "variables", XfaResolveFlags::PROPERTIES),
            [variables]
        );
        let border = found(&resolver, &child, "border", XfaResolveFlags::PROPERTIES);
        assert_eq!(border.len(), 1);
        assert_eq!(border[0].get_element_type(), XfaElement::Border);
        assert!(found(&resolver, &child, "pageSet", XfaResolveFlags::PROPERTIES).is_empty());
    }
    #[test]
    fn dataset_prefix_creation_and_binding() {
        let (doc, _, form) = tree();
        doc.set_xml_document(Rc::new(RefCell::new(CfxXmlDocument::new())));
        let datasets = node(
            &doc,
            XfaPacketType::Datasets,
            XfaElement::DataModel,
            "datasets",
            Some(&doc.get_root().unwrap()),
        );
        let data = node(
            &doc,
            XfaPacketType::Datasets,
            XfaElement::DataGroup,
            "data",
            Some(&datasets),
        );
        data.create_xml_mapping_node().unwrap();
        let a = node(
            &doc,
            XfaPacketType::Datasets,
            XfaElement::DataValue,
            "a",
            Some(&data),
        );
        a.create_xml_mapping_node().unwrap();
        let resolver = CfxJseResolveProcessor::new(doc.clone());
        assert_eq!(found(&resolver, &form, "!data.a", ORDINARY), [a.clone()]);
        let created = resolver
            .resolve_objects(
                Some(&data),
                &wide("a[2]"),
                XfaResolveFlags::CHILDREN | XfaResolveFlags::CREATE_NODE,
            )
            .unwrap()
            .unwrap();
        assert_eq!(created.result_type, ResolveResultType::CreateNodeOne);
        assert_eq!(data.get_node_list_for_type(XfaElement::DataValue).len(), 3);
        assert_eq!(created.objects[0].get_name_hash(), hash_name(&wide("a")));
        let existing = resolver
            .resolve_objects(
                Some(&data),
                &wide("a[1]"),
                XfaResolveFlags::CHILDREN | XfaResolveFlags::CREATE_NODE,
            )
            .unwrap()
            .unwrap();
        assert_eq!(existing.result_type, ResolveResultType::ExistNodes);
        assert!(matches!(
            resolver.resolve_objects(
                Some(&data),
                &wide("a[-1]"),
                XfaResolveFlags::CHILDREN | XfaResolveFlags::CREATE_NODE
            ),
            Err(ResolveError::InvalidIndex)
        ));
        let values = found(&resolver, &data, "a[*]", XfaResolveFlags::CHILDREN);
        for value in &values {
            value.add_bind_item(&form);
        }
        let bound = resolver
            .resolve_objects(
                Some(&data),
                &wide("a[*]"),
                XfaResolveFlags::CHILDREN | XfaResolveFlags::CREATE_NODE,
            )
            .unwrap()
            .unwrap();
        assert_eq!(bound.result_type, ResolveResultType::CreateNodeOne);
        assert_eq!(data.get_node_list_for_type(XfaElement::DataValue).len(), 4);
        let all = resolver
            .resolve_objects(
                Some(&data),
                &wide("a[*]"),
                XfaResolveFlags::CHILDREN | XfaResolveFlags::BIND_NEW,
            )
            .unwrap()
            .unwrap();
        assert_eq!(all.result_type, ResolveResultType::CreateNodeAll);
        assert_eq!(all.objects.len(), 4);
        let group_bind = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Subform,
            "group",
            None,
        );
        let nested = resolver
            .resolve_objects_with_bind_node(
                Some(&data),
                &wide("group.child"),
                XfaResolveFlags::CHILDREN | XfaResolveFlags::CREATE_NODE,
                Some(&group_bind),
            )
            .unwrap()
            .unwrap();
        assert_eq!(nested.objects[0].get_element_type(), XfaElement::DataGroup);
        assert_eq!(
            nested.objects[0]
                .parent()
                .unwrap()
                .get_cdata(XfaAttribute::Name),
            wide("group")
        );
    }
    struct BoaHost {
        engine: Rc<CfxJsEngine>,
        this: CxfaNode,
        attribute: CxfaNode,
        visits: RefCell<Vec<CxfaNode>>,
        resolver: RefCell<Weak<CfxJseResolveProcessor>>,
    }
    impl SomScriptHost for BoaHost {
        fn this_node(&self) -> Result<Option<CxfaNode>, ResolveError> {
            Ok(Some(self.this.clone()))
        }
        fn attribute_node(
            &self,
            _: &CxfaNode,
            _: ScriptAttributeDescriptor,
        ) -> Result<Option<CxfaNode>, ResolveError> {
            Ok(Some(self.attribute.clone()))
        }
        fn predicate(
            &self,
            language: SomScriptLanguage,
            script: &[u32],
            node: &CxfaNode,
        ) -> Result<bool, ResolveError> {
            self.visits.borrow_mut().push(node.clone());
            // Re-enter the same resolver while its outer result/filter is active.
            // Its stacks and creation helper belong to this call, not a RefCell.
            let resolver = self.resolver.borrow().upgrade();
            if let Some(resolver) = resolver {
                let nested = resolver
                    .resolve_objects(Some(node), &wide("$"), XfaResolveFlags::CHILDREN)?
                    .unwrap();
                assert_eq!(nested.objects, [node.clone()]);
            }
            let javascript = if language == SomScriptLanguage::Formcalc {
                let Some(translated) = translate(script) else {
                    return Ok(false);
                };
                translated
            } else {
                script.to_vec()
            };
            // Execute through the existing Boa context owner; the resolver owns
            // no alternate runtime or date/time implementation.
            let result = self.engine.with_context(|context| {
                context.global_object().set(
                    js_string!("nodeIndex"),
                    JsValue::new(node.get_index(false, true) as i32),
                    true,
                    context,
                )?;
                let result = context.eval(Source::from_utf16(&utf16(&javascript)))?;
                Ok(result.to_boolean())
            });
            // Native RunScript status failure removes this candidate.
            Ok(result.unwrap_or(false))
        }
    }
    #[test]
    fn predicates_real_boa_formcalc_attributes_and_reentry() {
        let (doc, _, form) = tree();
        let a = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Field,
            "winner",
            Some(&form),
        );
        let b = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Field,
            "second",
            Some(&form),
        );
        let c = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Field,
            "third",
            Some(&form),
        );
        let mut context = Context::default();
        register_formcalc(&mut context).unwrap();
        let host = Rc::new(BoaHost {
            engine: CfxJsEngine::new(context),
            this: a.clone(),
            attribute: form.clone(),
            visits: RefCell::new(Vec::new()),
            resolver: RefCell::new(Weak::new()),
        });
        let resolver = Rc::new(CfxJseResolveProcessor::with_host(doc, host.clone()));
        *host.resolver.borrow_mut() = Rc::downgrade(&resolver);
        assert_eq!(
            found(
                &resolver,
                &form,
                "#field.(nodeIndex == 1)",
                XfaResolveFlags::CHILDREN
            ),
            [b.clone()]
        );
        assert_eq!(*host.visits.borrow(), [c.clone(), b.clone(), a.clone()]);
        assert_eq!(
            found(&resolver, &form, "#field.[1]", XfaResolveFlags::CHILDREN),
            [a.clone(), b.clone(), c]
        );
        assert!(found(&resolver, &form, "#field.[0]", XfaResolveFlags::CHILDREN).is_empty());
        assert!(found(
            &resolver,
            &form,
            "#field.(throw new Error('predicate'))",
            XfaResolveFlags::CHILDREN
        )
        .is_empty());
        assert_eq!(found(&resolver, &form, "this", ORDINARY), [a.clone()]);
        let attribute = resolver
            .resolve_objects(Some(&b), &wide("parent"), XfaResolveFlags::ATTRIBUTES)
            .unwrap()
            .unwrap();
        assert_eq!(attribute.result_type, ResolveResultType::Attribute);
        assert_eq!(attribute.objects, [b.clone()]);
        assert!(attribute.script_attribute.is_some());
        assert_eq!(found(&resolver, &b, "parent.winner", ORDINARY), [a]);
    }
    #[test]
    fn required_hosts_malformed_and_foreign_nodes_are_explicit_errors() {
        let (doc, _, form) = tree();
        let field = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Field,
            "a",
            Some(&form),
        );
        let resolver = CfxJseResolveProcessor::new(doc);
        for (reference, expression) in [
            (&form, "#field.(true)"),
            (&field, "parent.a"),
            (&form, "this"),
        ] {
            assert!(
                matches!(
                    resolver.resolve_objects(Some(reference), &wide(expression), ORDINARY),
                    Err(ResolveError::Unsupported(_))
                ),
                "{expression}"
            );
        }
        assert!(matches!(
            resolver.resolve_objects(Some(&form), &wide("a["), ORDINARY),
            Err(ResolveError::MalformedExpression)
        ));
        let other = CxfaDocument::new();
        let foreign = node(
            &other,
            XfaPacketType::Template,
            XfaElement::Subform,
            "foreign",
            None,
        );
        assert!(matches!(
            resolver.resolve_objects(Some(&foreign), &wide("$"), ORDINARY),
            Err(ResolveError::InvalidReference)
        ));
        assert!(resolver
            .resolve_objects(Some(&form), &[], ORDINARY)
            .unwrap()
            .is_none());
    }
    #[test]
    fn opaque_script_attribute_exception_keeps_object_identity() {
        struct ThrowingHost {
            engine: Rc<CfxJsEngine>,
        }
        impl SomScriptHost for ThrowingHost {
            fn this_node(&self) -> Result<Option<CxfaNode>, ResolveError> {
                Err(ResolveError::Unsupported("active script this object"))
            }
            fn attribute_node(
                &self,
                _: &CxfaNode,
                _: ScriptAttributeDescriptor,
            ) -> Result<Option<CxfaNode>, ResolveError> {
                self.engine
                    .eval(&"throw globalThis.marker".encode_utf16().collect::<Vec<_>>())
                    .map(|_| None)
                    .map_err(ResolveError::Script)
            }
            fn predicate(
                &self,
                _: SomScriptLanguage,
                _: &[u32],
                _: &CxfaNode,
            ) -> Result<bool, ResolveError> {
                Err(ResolveError::Unsupported("predicate execution"))
            }
        }
        let (doc, _, form) = tree();
        let engine = CfxJsEngine::new(Context::default());
        let marker = engine
            .eval(
                &"globalThis.marker = {identity: 17}"
                    .encode_utf16()
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let resolver = CfxJseResolveProcessor::with_host(
            doc,
            Rc::new(ThrowingHost {
                engine: engine.clone(),
            }),
        );
        let error = resolver
            .resolve_objects(
                Some(&form),
                &wide("parent.any"),
                XfaResolveFlags::ATTRIBUTES | XfaResolveFlags::CHILDREN,
            )
            .unwrap_err();
        let ResolveError::Script(error) = error else {
            panic!("expected an opaque script exception");
        };
        assert!(engine
            .with_context(|context| Ok(error.into_opaque(context)? == marker))
            .unwrap());
        assert_eq!(
            found(&resolver, &form, "$", XfaResolveFlags::CHILDREN),
            [form]
        );
        assert_eq!(
            engine
                .eval(&"1+1".encode_utf16().collect::<Vec<_>>())
                .unwrap()
                .as_number(),
            Some(2.0)
        );
    }
    #[test]
    fn parsed_template_som_uses_existing_builder_nodes() {
        let input=b"<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'><template><subform name='form'><field name='original' id='base'/><field name='copy'/></subform></template></xdp:xdp>";
        let mut bytes = input.to_vec();
        bytes.push(0);
        let xml = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
            .parse()
            .unwrap();
        let doc = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(doc.clone());
        assert!(builder.build_document(Rc::new(RefCell::new(xml)), XfaPacketType::Xdp));
        doc.set_root(builder.get_root_node());
        let resolver = CfxJseResolveProcessor::new(doc.clone());
        let template = doc.get_xfa_node(XfaHashCode::Template as u32).unwrap();
        let form = template.first_child().unwrap();
        let copy = form.last_child().unwrap();
        let original = found(&resolver, &copy, "$template.form.original", ORDINARY);
        assert_eq!(original, [form.first_child().unwrap()]);
        assert_eq!(original[0].get_cdata(XfaAttribute::Id), wide("base"));
    }
    #[test]
    fn helper_condition_counts_class_creation_and_field_choice_type() {
        let mut helper = CfxJseNodeHelper::new();
        assert!(!helper.create_node_for_condition(&[]));
        assert!(helper.create_node_for_condition(&wide("[ * ]")));
        assert_eq!(helper.create_flag, ResolveResultType::CreateNodeAll);
        assert_eq!(helper.create_count, 1);
        assert!(helper.create_node_for_condition(&wide("[3]")));
        assert_eq!(helper.create_count, 3);
        assert_eq!(helper.create_flag, ResolveResultType::CreateNodeOne);
        assert!(!helper.create_node_for_condition(&wide("[-1]")));
        let (doc, _, form) = tree();
        helper.create_parent = Some(form.clone());
        helper.create_count = 1;
        assert!(helper
            .create_node(&wide("#field"), &[], true, &doc)
            .unwrap());
        assert_eq!(
            helper.create_parent.as_ref().unwrap().get_element_type(),
            XfaElement::Field
        );
        let field = helper.create_parent.clone().unwrap();
        let ui = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::Ui,
            "",
            Some(&field),
        );
        let choice = node(
            &doc,
            XfaPacketType::Template,
            XfaElement::ChoiceList,
            "",
            Some(&ui),
        );
        choice.set_enum(XfaAttribute::Open, XfaAttributeValue::MultiSelect, false);
        helper.set_create_node_type(Some(&field));
        assert_eq!(helper.last_create_type, XfaElement::DataGroup);
        choice.set_enum(XfaAttribute::Open, XfaAttributeValue::UserControl, false);
        helper.set_create_node_type(Some(&field));
        assert_eq!(helper.last_create_type, XfaElement::DataValue);
        helper.set_create_node_type(Some(&form));
        assert_eq!(helper.last_create_type, XfaElement::DataGroup);
    }
}

#[path = "cfxjse_resolveobjects.rs"]
mod objects;
pub use objects::{ObjectResolveResult, PseudoModel, PseudoObject, ResolvedObject};
