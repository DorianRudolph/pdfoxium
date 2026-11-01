// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Native live child list from `cxfa_attachnodelist.{h,cpp}`. Root arena
//! retention replaces Member tracing; `fxjs::xfa::cjx_list` supplies the wrapper.
use super::{
    cxfa_document::{CxfaDocument, CxfaNodeStore},
    cxfa_list::CxfaList,
    cxfa_node::CxfaNode,
};
use crate::xfa::fxfa::fxfa_basic::XfaElement;
use std::rc::{Rc, Weak};
pub struct CxfaAttachNodeList {
    document: Weak<CxfaDocument>,
    attach_node: CxfaNode,
    _store: Rc<CxfaNodeStore>,
}
impl CxfaAttachNodeList {
    pub fn new(document: &Rc<CxfaDocument>, attach_node: CxfaNode) -> Option<Rc<Self>> {
        let store = attach_node.retain_store()?;
        Some(Rc::new(Self {
            document: Rc::downgrade(document),
            attach_node,
            _store: store,
        }))
    }
    fn only_child(&self) -> bool {
        self.attach_node.get_element_type() == XfaElement::Subform
    }
    fn can_attach(&self, node: &CxfaNode) -> bool {
        if !node.is_alive() || node.is_ancestor_of(&self.attach_node) {
            return false;
        }
        let Some(document) = self.attach_node.document() else {
            return false;
        };
        node.document()
            .is_some_and(|other| Rc::ptr_eq(&document, &other))
    }
}
impl CxfaList for CxfaAttachNodeList {
    fn document(&self) -> Option<Rc<CxfaDocument>> {
        self.document.upgrade()
    }
    fn get_length(&self) -> usize {
        self.attach_node
            .count_children(XfaElement::Unknown, self.only_child())
    }
    fn append(&self, node: &CxfaNode) -> bool {
        if !self.can_attach(node) {
            return false;
        }
        if let Some(parent) = node.parent() {
            if !parent.remove_child_and_notify(node, true) {
                return false;
            }
        }
        self.attach_node.insert_child_before_and_notify(node, None)
    }
    fn insert(&self, node: &CxfaNode, before: Option<&CxfaNode>) -> bool {
        if !self.can_attach(node) {
            return false;
        }
        if let Some(before) = before {
            if !before.is_alive() || before.parent().as_ref() != Some(&self.attach_node) {
                return false;
            }
            // Inserting an existing child before itself is already satisfied. Detaching
            // it first would invalidate the anchor and trigger the native CHECK.
            if before == node {
                return true;
            }
        }
        if let Some(parent) = node.parent() {
            if !parent.remove_child_and_notify(node, true) {
                return false;
            }
        }
        self.attach_node
            .insert_child_before_and_notify(node, before)
    }
    fn remove(&self, node: &CxfaNode) {
        if node.is_alive() {
            self.attach_node.remove_child_and_notify(node, true);
        }
    }
    fn item(&self, index: usize) -> Option<CxfaNode> {
        self.attach_node
            .get_child(index, XfaElement::Unknown, self.only_child())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xfa::fxfa::fxfa_basic::XfaPacketType;
    fn node(doc: &Rc<CxfaDocument>, element: XfaElement) -> CxfaNode {
        doc.create_node(XfaPacketType::Template, element).unwrap()
    }
    #[test]
    fn live_child_list_filters_subform_properties_and_reparents_in_order() {
        let doc = CxfaDocument::new();
        let root = node(&doc, XfaElement::Subform);
        let property = node(&doc, XfaElement::Desc);
        let first = node(&doc, XfaElement::Field);
        let second = node(&doc, XfaElement::Draw);
        let old_parent = node(&doc, XfaElement::Subform);
        assert!(root.insert_child_and_notify(-1, &property));
        assert!(root.insert_child_and_notify(-1, &first));
        assert!(old_parent.insert_child_and_notify(-1, &second));
        let list = CxfaAttachNodeList::new(&doc, root.clone()).unwrap();
        assert_eq!(list.get_length(), 1);
        assert_eq!(list.item(0), Some(first.clone()));
        assert!(list.item(1).is_none());
        assert!(list.append(&second));
        assert!(old_parent.first_child().is_none());
        assert_eq!(list.get_length(), 2);
        assert_eq!(list.item(1), Some(second.clone()));
        assert!(list.insert(&second, Some(&first)));
        assert_eq!(list.item(0), Some(second.clone()));
        assert_eq!(list.item(1), Some(first.clone()));
        assert!(list.insert(&first, Some(&first)));
        assert_eq!(list.item(1), Some(first.clone()));
        assert!(!list.append(&root));
        assert!(!list.insert(&first, Some(&old_parent)));
        assert_eq!(first.parent(), Some(root.clone()));
        list.remove(&second);
        assert!(second.parent().is_none());
        assert_eq!(list.get_length(), 1);
        // The view is dynamic and its root store also retains nodes added later.
        let added = node(&doc, XfaElement::Subform);
        assert!(root.insert_child_and_notify(-1, &added));
        drop(doc);
        assert!(list.document().is_none());
        assert_eq!(list.get_length(), 2);
        assert_eq!(list.item(1), Some(added.clone()));
        assert_eq!(root.first_child(), Some(property));
        drop(list);
        assert!(!root.is_alive());
        assert!(!added.is_alive());
    }
    #[test]
    fn non_subform_lists_include_property_nodes_and_reject_cross_document_moves() {
        let doc = CxfaDocument::new();
        let other = CxfaDocument::new();
        let root = node(&doc, XfaElement::Field);
        let value = node(&doc, XfaElement::Value);
        let caption = node(&doc, XfaElement::Caption);
        assert!(root.insert_child_and_notify(-1, &value));
        assert!(root.insert_child_and_notify(-1, &caption));
        let list = CxfaAttachNodeList::new(&doc, root).unwrap();
        assert_eq!(list.get_length(), 2);
        assert_eq!(list.item(0), Some(value));
        assert_eq!(list.item(1), Some(caption));
        let foreign_parent = node(&other, XfaElement::Subform);
        let foreign = node(&other, XfaElement::Field);
        assert!(foreign_parent.insert_child_and_notify(-1, &foreign));
        assert!(!list.append(&foreign));
        assert_eq!(foreign.parent(), Some(foreign_parent));
    }
    #[test]
    fn notification_reentry_can_query_and_remove_the_just_inserted_child() {
        use crate::xfa::fxfa::{
            fxfa_basic::XfaAttribute, parser::cxfa_document::XfaDocumentNotify,
        };
        use std::cell::{Cell, RefCell};
        struct Observer {
            list: RefCell<Weak<CxfaAttachNodeList>>,
            entered: Cell<bool>,
            removed: Cell<usize>,
        }
        impl XfaDocumentNotify for Observer {
            fn on_child_added(&self, _parent: &CxfaNode, child: &CxfaNode) {
                let list = self.list.borrow().upgrade();
                if let Some(list) = list {
                    assert_eq!(list.get_length(), 1);
                    if !self.entered.replace(true) {
                        list.remove(child);
                        assert_eq!(list.get_length(), 0);
                    }
                }
            }
            fn on_child_removed(&self, _parent: &CxfaNode, _child: &CxfaNode) {
                self.removed.set(self.removed.get() + 1);
                if let Some(list) = self.list.borrow().upgrade() {
                    assert_eq!(list.get_length(), 0);
                }
            }
            fn on_attribute_changed(&self, _node: &CxfaNode, _attribute: XfaAttribute) {}
            fn on_node_ready(&self, _node: &CxfaNode) {}
        }
        let observer = Rc::new(Observer {
            list: RefCell::new(Weak::new()),
            entered: Cell::new(false),
            removed: Cell::new(0),
        });
        let doc = CxfaDocument::with_notify(Some(observer.clone()));
        let root = node(&doc, XfaElement::Subform);
        let child = node(&doc, XfaElement::Field);
        let list = CxfaAttachNodeList::new(&doc, root).unwrap();
        *observer.list.borrow_mut() = Rc::downgrade(&list);
        assert!(list.append(&child));
        assert!(observer.entered.get());
        assert_eq!(observer.removed.get(), 1);
        assert_eq!(list.get_length(), 0);
        assert!(child.parent().is_none());
    }
}
