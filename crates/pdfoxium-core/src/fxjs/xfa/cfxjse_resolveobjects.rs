// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Actual CXFA_Object results for GetXFAObject and ResolveObjects. Parser-node
//! consumers use an explicit node projection; pseudo-models never allocate DOM
//! nodes, XML mappings, or model attributes.
use super::*;
use std::rc::Weak;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum PseudoModel {
    DataWindow,
    Event,
    Host,
    Log,
    Signature,
    Layout,
}
impl PseudoModel {
    pub fn name(self) -> &'static str {
        match self {
            Self::DataWindow => "dataWindow",
            Self::Event => "event",
            Self::Host => "host",
            Self::Log => "log",
            Self::Signature => "signature",
            Self::Layout => "layout",
        }
    }
    pub fn element(self) -> XfaElement {
        match self {
            Self::DataWindow => XfaElement::DataWindow,
            Self::Event => XfaElement::EventPseudoModel,
            Self::Host => XfaElement::HostPseudoModel,
            Self::Log => XfaElement::LogPseudoModel,
            Self::Signature => XfaElement::SignaturePseudoModel,
            Self::Layout => XfaElement::LayoutPseudoModel,
        }
    }
    pub fn from_hash(hash: u32) -> Option<Self> {
        match hash {
            h if h == XfaHashCode::DataWindow as u32 => Some(Self::DataWindow),
            h if h == XfaHashCode::Event as u32 => Some(Self::Event),
            h if h == XfaHashCode::Host as u32 => Some(Self::Host),
            h if h == XfaHashCode::Log as u32 => Some(Self::Log),
            h if h == XfaHashCode::Signature as u32 => Some(Self::Signature),
            h if h == XfaHashCode::Layout as u32 => Some(Self::Layout),
            _ => None,
        }
    }
}
#[derive(Clone, Debug)]
pub struct PseudoObject {
    kind: PseudoModel,
    document: Weak<CxfaDocument>,
}
impl PseudoObject {
    pub fn new(document: &Rc<CxfaDocument>, kind: PseudoModel) -> Self {
        Self {
            kind,
            document: Rc::downgrade(document),
        }
    }
    pub fn kind(&self) -> PseudoModel {
        self.kind
    }
    pub fn document(&self) -> Option<Rc<CxfaDocument>> {
        self.document.upgrade()
    }
}
impl PartialEq for PseudoObject {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && Weak::ptr_eq(&self.document, &other.document)
    }
}
impl Eq for PseudoObject {}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolvedObject {
    Node(CxfaNode),
    Pseudo(PseudoObject),
}
impl From<CxfaNode> for ResolvedObject {
    fn from(node: CxfaNode) -> Self {
        Self::Node(node)
    }
}
impl ResolvedObject {
    pub fn as_node(&self) -> Option<&CxfaNode> {
        if let Self::Node(node) = self {
            Some(node)
        } else {
            None
        }
    }
    pub fn into_node(self) -> Option<CxfaNode> {
        if let Self::Node(node) = self {
            Some(node)
        } else {
            None
        }
    }
    pub fn element(&self) -> XfaElement {
        match self {
            Self::Node(node) => node.get_element_type(),
            Self::Pseudo(object) => object.kind.element(),
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct ObjectResolveResult {
    pub result_type: ResolveResultType,
    pub script_attribute: Option<ScriptAttributeDescriptor>,
    pub objects: Vec<ResolvedObject>,
}
impl From<ResolveResult> for ObjectResolveResult {
    fn from(result: ResolveResult) -> Self {
        Self {
            result_type: result.result_type,
            script_attribute: result.script_attribute,
            objects: result
                .objects
                .into_iter()
                .map(ResolvedObject::Node)
                .collect(),
        }
    }
}
impl ObjectResolveResult {
    pub fn into_node_result(self) -> Option<ResolveResult> {
        let objects: Vec<_> = self
            .objects
            .into_iter()
            .filter_map(ResolvedObject::into_node)
            .collect();
        (!objects.is_empty()).then_some(ResolveResult {
            result_type: self.result_type,
            script_attribute: self.script_attribute,
            objects,
        })
    }
}
impl CfxJseResolveProcessor {
    pub fn check_object_reference(&self, object: &ResolvedObject) -> Result<(), ResolveError> {
        match object {
            ResolvedObject::Node(node) => self.check_reference(node),
            ResolvedObject::Pseudo(object) => {
                if object
                    .document()
                    .is_some_and(|document| Rc::ptr_eq(&document, &self.document))
                {
                    Ok(())
                } else {
                    Err(ResolveError::InvalidReference)
                }
            }
        }
    }
    pub fn resolve_object_values(
        &self,
        reference: Option<&CxfaNode>,
        expression: &[u32],
        styles: XfaResolveFlags,
    ) -> Result<Option<ObjectResolveResult>, ResolveError> {
        let reference = reference.cloned().map(ResolvedObject::Node);
        self.resolve_object_values_from(reference.as_ref(), expression, styles)
    }
    pub fn resolve_object_values_from(
        &self,
        reference: Option<&ResolvedObject>,
        expression: &[u32],
        styles: XfaResolveFlags,
    ) -> Result<Option<ObjectResolveResult>, ResolveError> {
        if expression.is_empty() {
            return Ok(None);
        }
        if let Some(reference) = reference {
            self.check_object_reference(reference)?;
        }
        let Some(reference) = reference
            .cloned()
            .or_else(|| self.document.get_root().map(ResolvedObject::Node))
        else {
            return Ok(None);
        };
        let mut session = ResolveSession {
            processor: self,
            node_helper: CfxJseNodeHelper::new(),
            up_objects: Vec::new(),
            cur_start: 0,
        };
        if styles.intersects(XfaResolveFlags::PARENT | XfaResolveFlags::SIBLINGS) {
            if let Some(node) = reference.as_node() {
                session.up_objects.push(node.clone());
            }
        }
        session.resolve_object_values(reference, expression, styles)
    }
}
impl ResolveSession<'_> {
    fn resolve_object(
        &mut self,
        current: &ResolvedObject,
        rnd: &mut NodeData,
    ) -> Result<Option<ObjectResolveResult>, ResolveError> {
        self.processor.check_object_reference(current)?;
        if let ResolvedObject::Pseudo(_) = current {
            if !rnd.styles.contains(XfaResolveFlags::ATTRIBUTES) {
                return Ok(None);
            }
            return Ok(
                find_script_attribute_descriptor(current.element(), &rnd.name).map(|attribute| {
                    ObjectResolveResult {
                        result_type: ResolveResultType::Attribute,
                        script_attribute: Some(attribute),
                        objects: vec![current.clone()],
                    }
                }),
            );
        }
        rnd.cur_object = current.as_node().cloned();
        if !rnd.styles.contains(XfaResolveFlags::ANY_CHILD) {
            if rnd.hash_name == XfaHashCode::This as u32 && rnd.level == 0 {
                let host = self
                    .processor
                    .host
                    .as_ref()
                    .ok_or(ResolveError::Unsupported("active script this object"))?;
                let object = host.this_object()?;
                if let Some(object) = &object {
                    self.processor.check_object_reference(object)?;
                }
                return Ok(object.map(|object| ObjectResolveResult {
                    objects: vec![object],
                    ..Default::default()
                }));
            }
            let pseudo = if rnd.name.first() == Some(&36) && rnd.level == 0 {
                PseudoModel::from_hash(hash_name(&rnd.name[1..]))
            } else if current.element() == XfaElement::Xfa && rnd.name.first() != Some(&36) {
                PseudoModel::from_hash(rnd.hash_name)
            } else {
                None
            };
            if let Some(kind) = pseudo {
                if rnd.styles.contains(XfaResolveFlags::CREATE_NODE) {
                    return Err(ResolveError::InvalidReference);
                }
                let object =
                    ResolvedObject::Pseudo(PseudoObject::new(&self.processor.document, kind));
                return self.filter_object_condition(object, rnd);
            }
        }
        Ok(self.resolve(rnd)?.then(|| rnd.result.clone().into()))
    }
    fn filter_object_condition(
        &self,
        object: ResolvedObject,
        rnd: &NodeData,
    ) -> Result<Option<ObjectResolveResult>, ResolveError> {
        let condition = trim(&rnd.condition);
        let mut keep = true;
        if condition.first() == Some(&91) {
            let end = condition.len().saturating_sub(1);
            let digits = trim(
                condition
                    .get(1..end)
                    .ok_or(ResolveError::MalformedExpression)?,
            );
            if digits.first() != Some(&42) {
                let relative = digits.first().is_some_and(|ch| matches!(ch, 43 | 45));
                let mut index = i64::from(wtoi(digits));
                if relative {
                    index += i64::try_from(self.current_index())
                        .map_err(|_| ResolveError::InvalidIndex)?;
                }
                keep = index == 0;
            }
        } else if condition.len() >= 3
            && condition.first() == Some(&46)
            && matches!(condition[1], 91 | 40)
        {
            let language = if condition[1] == 91 {
                SomScriptLanguage::Formcalc
            } else {
                SomScriptLanguage::Javascript
            };
            let host = self
                .processor
                .host
                .as_ref()
                .ok_or(ResolveError::Unsupported("predicate execution"))?;
            keep = host.predicate_object(language, &condition[2..condition.len() - 1], &object)?;
        }
        Ok(keep.then(|| ObjectResolveResult {
            objects: vec![object],
            ..Default::default()
        }))
    }
}

impl ResolveSession<'_> {
    pub(super) fn resolve_object_values(
        &mut self,
        reference: ResolvedObject,
        expression: &[u32],
        mut styles: XfaResolveFlags,
    ) -> Result<Option<ObjectResolveResult>, ResolveError> {
        let mut next_create = false;
        let mut start = 0;
        let mut level = 0;
        let mut found = vec![reference];
        let mut rnd = NodeData::default();
        loop {
            let count = found.len();
            rnd.styles = styles;
            self.cur_start = start;
            let next = CfxJseResolveProcessor::get_filter(expression, start, &mut rnd)?;
            let Some(next) = next else {
                if styles.contains(XfaResolveFlags::CREATE_NODE) && !next_create {
                    if let Some(all_start) = self.node_helper.cur_all_start {
                        if let Some(unbound) = found
                            .iter()
                            .find(|object| {
                                object.as_node().is_some_and(|node| !node.has_bind_item())
                            })
                            .cloned()
                        {
                            found = vec![unbound];
                            break;
                        }
                        styles |= XfaResolveFlags::BIND;
                        let Some(parent) = self.node_helper.all_start_parent.clone() else {
                            return Ok(None);
                        };
                        found = vec![ResolvedObject::Node(parent)];
                        start = all_start;
                        continue;
                    }
                    if let Some(node) = found.first().cloned() {
                        found = vec![node];
                    }
                }
                break;
            };
            start = next;
            if next_create {
                if self.node_helper.create_node(
                    &rnd.name,
                    &rnd.condition,
                    start == expression.len(),
                    &self.processor.document,
                )? {
                    continue;
                }
                break;
            }
            let mut returned = Vec::new();
            let mut i = 0;
            while i < count {
                let data_bind = styles
                    .intersects(XfaResolveFlags::BIND | XfaResolveFlags::CREATE_NODE)
                    && count > 1;
                if data_bind {
                    let mut bind = NodeData::default();
                    CfxJseResolveProcessor::get_filter(expression, start, &mut bind)?;
                    i = self.index_for_data_bind(&bind.condition, count);
                }
                let current = found[i].clone();
                i += 1;
                rnd.cur_object = current.as_node().cloned();
                rnd.level = level;
                rnd.result = ResolveResult::default();
                let Some(mut object_result) = self.resolve_object(&current, &mut rnd)? else {
                    // Bind selects exactly one candidate. Re-selecting candidate
                    // zero on a miss would otherwise repeat forever upstream.
                    if data_bind {
                        break;
                    }
                    continue;
                };
                rnd.result.result_type = object_result.result_type;
                rnd.result.script_attribute = object_result.script_attribute;
                if object_result.result_type == ResolveResultType::Attribute
                    && start < expression.len()
                {
                    let descriptor = rnd
                        .result
                        .script_attribute
                        .ok_or(ResolveError::Unsupported("script attribute descriptor"))?;
                    let Some(host) = &self.processor.host else {
                        return Err(ResolveError::Unsupported(
                            "intermediate object-valued attribute",
                        ));
                    };
                    let owner = object_result
                        .objects
                        .first()
                        .cloned()
                        .ok_or(ResolveError::InvalidReference)?;
                    let value = host.attribute_object(&owner, descriptor)?;
                    object_result.objects.clear();
                    if let Some(object) = value {
                        self.processor.check_object_reference(&object)?;
                        object_result.objects.push(object);
                    }
                    // The attribute was dereferenced; only a terminal attribute
                    // result retains its attribute discriminator.
                    object_result.result_type = ResolveResultType::Nodes;
                    rnd.result.result_type = ResolveResultType::Nodes;
                }
                self.up_objects.pop();
                returned.append(&mut object_result.objects);
                if data_bind {
                    break;
                }
            }
            found = returned;
            if found.is_empty() {
                if styles.contains(XfaResolveFlags::CREATE_NODE) {
                    next_create = true;
                    if self.node_helper.create_parent.is_none() {
                        self.node_helper.create_parent = rnd.cur_object.clone();
                        self.node_helper.create_count = 1;
                    }
                    if self.node_helper.create_node(
                        &rnd.name,
                        &rnd.condition,
                        start == expression.len(),
                        &self.processor.document,
                    )? {
                        continue;
                    }
                }
                break;
            }
            if level == 0 {
                styles.remove(XfaResolveFlags::PARENT | XfaResolveFlags::SIBLINGS);
            }
            level += 1;
        }
        let mut result = ObjectResolveResult {
            result_type: rnd.result.result_type,
            script_attribute: rnd.result.script_attribute,
            objects: if next_create { Vec::new() } else { found },
        };
        if !next_create && result.result_type == ResolveResultType::Attribute {
            return Ok(Some(result));
        }
        if styles.intersects(
            XfaResolveFlags::CREATE_NODE | XfaResolveFlags::BIND | XfaResolveFlags::BIND_NEW,
        ) {
            if let Some(parent) = self.node_helper.create_parent.clone() {
                result.objects.push(ResolvedObject::Node(parent));
            } else {
                self.node_helper.create_node_for_condition(&rnd.condition);
            }
            result.result_type = self.node_helper.create_flag;
            if result.result_type == ResolveResultType::CreateNodeOne
                && self.node_helper.cur_all_start.is_some()
            {
                result.result_type = ResolveResultType::CreateNodeMidAll;
            }
            if !next_create && styles.contains(XfaResolveFlags::CREATE_NODE) {
                result.result_type = ResolveResultType::ExistNodes;
            }
        }
        Ok((!result.objects.is_empty()).then_some(result))
    }
}
