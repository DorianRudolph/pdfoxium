// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! The owned node factory, packet lookups, flags and parsing helpers from
//! `cxfa_document.{h,cpp}`. Prototype merging is in `cxfa_document_merge`.
//! Model data merge is in `cxfa_document_datamerge`. Script pseudo-models,
//! layout and full document-host services remain open.
use super::{
    cxfa_localemgr::CxfaLocaleMgr,
    cxfa_node::{CxfaNode, CxfaNodeData},
    cxfa_nodeowner::CxfaNodeOwner,
};
use crate::{
    fxcrt::{fx_system::wtoi, tree_node::TreeNodeOwner, xml::cfx_xmldocument::CfxXmlDocument},
    xfa::fxfa::fxfa_basic::{XfaAttribute, XfaElement, XfaPacketType},
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

pub const XFA_VERSION_UNKNOWN: i32 = 0;
pub const XFA_VERSION_200: i32 = 200;
pub const XFA_VERSION_202: i32 = 202;
pub const XFA_VERSION_204: i32 = 204;
pub const XFA_VERSION_205: i32 = 205;
pub const XFA_VERSION_206: i32 = 206;
pub const XFA_VERSION_207: i32 = 207;
pub const XFA_VERSION_208: i32 = 208;
pub const XFA_VERSION_300: i32 = 300;
pub const XFA_VERSION_301: i32 = 301;
pub const XFA_VERSION_303: i32 = 303;
pub const XFA_VERSION_306: i32 = 306;
pub const XFA_VERSION_DEFAULT: i32 = XFA_VERSION_303;
pub const XFA_VERSION_MIN: i32 = 200;
pub const XFA_VERSION_MAX: i32 = 400;
pub type XmlDocumentRef = Rc<RefCell<CfxXmlDocument>>;

/// Optional observation of core mutations. The concrete FFNotify/widget and
/// script services are later consumers of this synchronous boundary.
pub trait XfaDocumentNotify {
    fn on_attribute_changing(&self, _node: &CxfaNode, _attribute: XfaAttribute) {}
    fn on_widget_list_item_added(&self, _node: &CxfaNode, _label: &[u32], _index: i32) {}
    fn on_widget_list_item_removed(&self, _node: &CxfaNode, _index: i32) {}
    fn on_child_added(&self, parent: &CxfaNode, child: &CxfaNode);
    fn on_child_removed(&self, parent: &CxfaNode, child: &CxfaNode);
    fn on_attribute_changed(&self, node: &CxfaNode, attribute: XfaAttribute);
    fn on_attribute_changed_by_script(&self, node: &CxfaNode, attribute: XfaAttribute) {
        self.on_attribute_changed(node, attribute);
    }
    fn on_node_ready(&self, node: &CxfaNode);
}

/// Retained by documents and native-core lists. Payloads point weakly back to
/// this store and document, so retaining a list never creates an ownership cycle.
/// XML owners are retained with node allocations, including detached mappings.
pub(crate) struct CxfaNodeStore {
    nodes: RefCell<Vec<TreeNodeOwner<CxfaNodeData>>>,
    xml_documents: RefCell<Vec<XmlDocumentRef>>,
    current_xml: RefCell<Option<XmlDocumentRef>>,
}
impl CxfaNodeStore {
    fn new() -> Self {
        Self {
            nodes: RefCell::new(Vec::new()),
            xml_documents: RefCell::new(Vec::new()),
            current_xml: RefCell::new(None),
        }
    }
}

pub struct CxfaDocument {
    store: Rc<CxfaNodeStore>,
    layout_processor: RefCell<
        Option<std::rc::Weak<crate::xfa::fxfa::layout::cxfa_layoutprocessor::CxfaLayoutProcessor>>,
    >,
    node_owner: CxfaNodeOwner,
    root: RefCell<Option<CxfaNode>>,
    notify: RefCell<Option<Rc<dyn XfaDocumentNotify>>>,
    global_binding: RefCell<BTreeMap<u32, CxfaNode>>,
    pending_nodes: RefCell<Vec<CxfaNode>>,
    cur_version_mode: Cell<i32>,
    strict_scoping: Cell<bool>,
    scripting: Cell<bool>,
    interactive: Cell<Option<bool>>,
    locale_mgr: RefCell<Option<Rc<CxfaLocaleMgr>>>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UseHrefParts<'a> {
    pub uri: &'a [u32],
    pub id: &'a [u32],
    pub som: &'a [u32],
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UseParts<'a> {
    pub id: &'a [u32],
    pub som: &'a [u32],
}
impl CxfaDocument {
    pub fn set_layout_processor(
        &self,
        processor: Option<
            std::rc::Weak<crate::xfa::fxfa::layout::cxfa_layoutprocessor::CxfaLayoutProcessor>,
        >,
    ) {
        *self.layout_processor.borrow_mut() = processor;
    }
    /// Clear document-level layout/locale services. The actual FFDoc owner
    /// releases the Boa runtime and its pseudo-model bindings in PreFinalize.
    pub fn clear_layout_data(&self) {
        self.set_layout_processor(None);
        self.set_locale_mgr(None);
    }
    pub fn get_layout_processor(
        &self,
    ) -> Option<Rc<crate::xfa::fxfa::layout::cxfa_layoutprocessor::CxfaLayoutProcessor>> {
        self.layout_processor
            .borrow()
            .as_ref()
            .and_then(std::rc::Weak::upgrade)
    }

    pub fn new() -> Rc<Self> {
        Self::with_notify(None)
    }
    pub fn with_notify(notify: Option<Rc<dyn XfaDocumentNotify>>) -> Rc<Self> {
        Rc::new(Self {
            store: Rc::new(CxfaNodeStore::new()),
            layout_processor: RefCell::new(None),
            node_owner: CxfaNodeOwner::new(),
            root: RefCell::new(None),
            notify: RefCell::new(notify),
            global_binding: RefCell::new(BTreeMap::new()),
            pending_nodes: RefCell::new(Vec::new()),
            cur_version_mode: Cell::new(XFA_VERSION_DEFAULT),
            strict_scoping: Cell::new(false),
            scripting: Cell::new(false),
            interactive: Cell::new(None),
            locale_mgr: RefCell::new(None),
        })
    }
    pub(crate) fn node_store(&self) -> Rc<CxfaNodeStore> {
        self.store.clone()
    }
    pub fn get_node_owner(&self) -> &CxfaNodeOwner {
        &self.node_owner
    }
    /// The embedding host supplies its language and existing timezone service.
    /// Locale-set nodes use the same store as the parsed document.
    pub fn configure_locales(&self, default_name: &[u32], timezone: Rc<dyn Fn() -> i32>) {
        let locale_set = self.get_root().and_then(|root| {
            root.get_first_child_by_class(crate::xfa::fxfa::fxfa_basic::XfaElement::LocaleSet)
        });
        self.set_locale_mgr(Some(Rc::new(CxfaLocaleMgr::with_locale_set(
            locale_set,
            default_name,
            timezone,
        ))));
    }
    pub fn set_locale_mgr(&self, manager: Option<Rc<CxfaLocaleMgr>>) {
        let old = self.locale_mgr.replace(manager);
        drop(old);
    }
    pub fn get_locale_mgr(&self) -> Option<Rc<CxfaLocaleMgr>> {
        self.locale_mgr.borrow().clone()
    }
    pub fn notify(&self) -> Option<Rc<dyn XfaDocumentNotify>> {
        self.notify.borrow().clone()
    }
    pub fn set_notify(&self, notify: Option<Rc<dyn XfaDocumentNotify>>) {
        let old = self.notify.replace(notify);
        drop(old);
    }
    pub fn create_node(
        self: &Rc<Self>,
        packet: XfaPacketType,
        element: XfaElement,
    ) -> Option<CxfaNode> {
        let (owner, node) = CxfaNode::allocate(self, packet, element)?;
        self.store.nodes.borrow_mut().push(owner);
        Some(node)
    }
    pub fn get_root(&self) -> Option<CxfaNode> {
        self.root.borrow().clone()
    }
    pub fn set_root(&self, node: Option<CxfaNode>) -> bool {
        if let Some(node) = node.as_ref() {
            if !node.is_alive()
                || node
                    .retain_store()
                    .is_none_or(|store| !Rc::ptr_eq(&store, &self.store))
            {
                return false;
            }
        }
        let old = self.root.replace(node);
        drop(old);
        self.interactive.set(None);
        true
    }
    pub fn xml_document(&self) -> Option<XmlDocumentRef> {
        self.store.current_xml.borrow().clone()
    }
    pub fn set_xml_document(&self, xml: XmlDocumentRef) {
        {
            let mut owners = self.store.xml_documents.borrow_mut();
            if !owners.iter().any(|old| Rc::ptr_eq(old, &xml)) {
                owners.push(xml.clone());
            }
        }
        let old = self.store.current_xml.replace(Some(xml));
        drop(old);
    }
    pub fn is_strict_scoping(&self) -> bool {
        self.strict_scoping.get()
    }
    pub fn set_is_strict_scoping(&self) {
        self.strict_scoping.set(true);
    }
    pub fn is_scripting(&self) -> bool {
        self.scripting.get()
    }
    pub fn set_is_scripting(&self) {
        self.scripting.set(true);
    }
    pub fn get_cur_version_mode(&self) -> i32 {
        self.cur_version_mode.get()
    }
    pub fn recognize_xfa_version_number(&self, namespace: &[u32]) -> i32 {
        let version = Self::parse_xfa_version(namespace);
        if version != XFA_VERSION_UNKNOWN {
            self.cur_version_mode.set(version);
        }
        version
    }
    pub fn parse_xfa_version(namespace: &[u32]) -> i32 {
        const PREFIX: &str = "http://www.xfa.org/schema/xfa-template/";
        if namespace.len() <= PREFIX.len()
            || !namespace
                .iter()
                .zip(PREFIX.bytes())
                .all(|(&a, b)| a == u32::from(b))
        {
            return XFA_VERSION_UNKNOWN;
        }
        let suffix = &namespace[PREFIX.len()..];
        let Some(dot) = suffix.iter().position(|&c| c == u32::from('.')) else {
            return XFA_VERSION_UNKNOWN;
        };
        // Reject narrowing overflow rather than wrapping to an unrelated version.
        let (Ok(major), Ok(minor)) = (
            i8::try_from(wtoi(&suffix[..dot])),
            i8::try_from(wtoi(&suffix[dot + 1..])),
        ) else {
            return XFA_VERSION_UNKNOWN;
        };
        let version = i32::from(major) * 100 + i32::from(minor);
        if (XFA_VERSION_MIN..=XFA_VERSION_MAX).contains(&version) {
            version
        } else {
            XFA_VERSION_UNKNOWN
        }
    }
    pub fn parse_use_href(value: &[u32]) -> UseHrefParts<'_> {
        let empty = &value[..0];
        let Some(sharp) = value.iter().position(|&c| c == u32::from('#')) else {
            return UseHrefParts {
                uri: value,
                id: empty,
                som: empty,
            };
        };
        let uri = &value[..sharp];
        let fragment = &value[sharp + 1..];
        if fragment.starts_with(&[115, 111, 109, 40]) && fragment.last() == Some(&41) {
            UseHrefParts {
                uri,
                id: empty,
                som: &fragment[4..fragment.len() - 1],
            }
        } else {
            UseHrefParts {
                uri,
                id: fragment,
                som: empty,
            }
        }
    }
    pub fn parse_use(value: &[u32]) -> UseParts<'_> {
        if value.first() == Some(&35) {
            UseParts {
                id: &value[1..],
                som: &value[..0],
            }
        } else {
            UseParts {
                id: &value[..0],
                som: value,
            }
        }
    }
    /// Node-only view of GetXFAObject; script pseudo-model creation is separate
    /// pending runtime work, and is deliberately not exposed by this API.
    pub fn get_xfa_node(&self, name_hash: u32) -> Option<CxfaNode> {
        const DATA: u32 = 0xbde9abda;
        const DATASETS: u32 = 0x99b95079;
        const RECORD: u32 = 0x5779d65f;
        if name_hash == DATA {
            let datasets = self.get_xfa_node(DATASETS)?;
            let namespace = datasets.try_namespace()?;
            let mut current = datasets.first_child();
            while let Some(node) = current {
                if node.get_element_type() == XfaElement::DataGroup
                    && node.get_name_hash() == DATA
                    && node.try_namespace().as_ref() == Some(&namespace)
                {
                    return Some(node);
                }
                current = node.next_sibling();
            }
            return None;
        }
        if name_hash == RECORD {
            return Self::find_child(&self.get_xfa_node(DATA)?, XfaElement::DataGroup);
        }
        let root = self.get_root()?;
        let mut current = root.first_child();
        while let Some(node) = current {
            if node.get_name_hash() == name_hash {
                return Some(node);
            }
            current = node.next_sibling();
        }
        None
    }
    fn find_child(parent: &CxfaNode, element: XfaElement) -> Option<CxfaNode> {
        let mut current = parent.first_child();
        while let Some(node) = current {
            if node.get_element_type() == element {
                return Some(node);
            }
            current = node.next_sibling();
        }
        None
    }
    pub fn is_interactive(&self) -> bool {
        if let Some(interactive) = self.interactive.get() {
            return interactive;
        }
        let Some(config) = self.get_xfa_node(0x4e1e39b6) else {
            return false;
        };
        let Some(present) = Self::find_child(&config, XfaElement::Present) else {
            return false;
        };
        let Some(pdf) = Self::find_child(&present, XfaElement::Pdf) else {
            return false;
        };
        let Some(interactive) = Self::find_child(&pdf, XfaElement::Interactive) else {
            return false;
        };
        let value = interactive.get_content() == [u32::from('1')];
        self.interactive.set(Some(value));
        value
    }
    pub fn get_node_by_id(&self, root: Option<&CxfaNode>, id: &[u32]) -> Option<CxfaNode> {
        if id.is_empty() {
            return None;
        }
        let root = root?.clone();
        let mut current = Some(root.clone());
        while let Some(node) = current {
            if node.get_cdata(XfaAttribute::Id) == id {
                return Some(node);
            }
            if let Some(child) = node.first_child() {
                current = Some(child);
                continue;
            }
            let mut cursor = node;
            current = loop {
                if cursor == root {
                    break None;
                }
                if let Some(next) = cursor.next_sibling() {
                    break Some(next);
                }
                let Some(parent) = cursor.parent() else {
                    break None;
                };
                cursor = parent;
            };
        }
        None
    }
    pub fn get_global_binding(&self, name_hash: u32) -> Option<CxfaNode> {
        self.global_binding
            .borrow()
            .get(&name_hash)
            .filter(|n| n.is_alive())
            .cloned()
    }
    pub fn register_global_binding(&self, name_hash: u32, node: Option<CxfaNode>) {
        let old = if let Some(node) = node {
            self.global_binding.borrow_mut().insert(name_hash, node)
        } else {
            self.global_binding.borrow_mut().remove(&name_hash)
        };
        drop(old);
    }
    pub fn get_pending_nodes_count(&self) -> usize {
        self.pending_nodes.borrow().len()
    }
    pub fn clear_global_bindings(&self) {
        let old = self.global_binding.take();
        drop(old);
    }
    pub fn get_pending_node_at_index(&self, index: usize) -> Option<CxfaNode> {
        self.pending_nodes.borrow().get(index).cloned()
    }
    pub fn append_pending_node(&self, node: CxfaNode) {
        self.pending_nodes.borrow_mut().push(node);
    }
    pub fn clear_pending_nodes(&self) {
        let old = self.pending_nodes.take();
        drop(old);
    }
}

#[cfg(test)]
#[path = "cxfa_document_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "cxfa_document_rebinding_tests.rs"]
mod rebinding_tests;
