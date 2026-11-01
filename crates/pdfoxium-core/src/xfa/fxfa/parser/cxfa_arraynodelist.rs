// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Native snapshot list from `cxfa_arraynodelist.{h,cpp}`. Each source Member
//! retains its node arena without retaining the document or copying a tree.
//! The inherited script wrapper is supplied by `fxjs::xfa::cjx_list`.
use super::{
    cxfa_document::{CxfaDocument, CxfaNodeStore},
    cxfa_list::CxfaList,
    cxfa_node::CxfaNode,
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
struct Member {
    node: CxfaNode,
    _store: Rc<CxfaNodeStore>,
}
impl Member {
    fn new(node: &CxfaNode) -> Option<Self> {
        Some(Self {
            node: node.clone(),
            _store: node.retain_store()?,
        })
    }
}
pub struct CxfaArrayNodeList {
    document: Weak<CxfaDocument>,
    array: RefCell<Vec<Member>>,
}
impl CxfaArrayNodeList {
    pub fn new(document: &Rc<CxfaDocument>) -> Rc<Self> {
        Rc::new(Self {
            document: Rc::downgrade(document),
            array: RefCell::new(Vec::new()),
        })
    }
    pub fn set_array_node_list(&self, nodes: &[CxfaNode]) -> bool {
        // The source intentionally leaves an existing snapshot unchanged on empty
        // input, rather than clearing it.
        if nodes.is_empty() {
            return true;
        }
        let Some(members) = nodes.iter().map(Member::new).collect::<Option<Vec<_>>>() else {
            return false;
        };
        let old = std::mem::replace(&mut *self.array.borrow_mut(), members);
        drop(old);
        true
    }
}
impl CxfaList for CxfaArrayNodeList {
    fn document(&self) -> Option<Rc<CxfaDocument>> {
        self.document.upgrade()
    }
    fn get_length(&self) -> usize {
        self.array.borrow().len()
    }
    fn append(&self, node: &CxfaNode) -> bool {
        let Some(member) = Member::new(node) else {
            return false;
        };
        self.array.borrow_mut().push(member);
        true
    }
    fn insert(&self, node: &CxfaNode, before: Option<&CxfaNode>) -> bool {
        let Some(member) = Member::new(node) else {
            return false;
        };
        let mut array = self.array.borrow_mut();
        let index = match before {
            Some(before) => {
                let Some(index) = array.iter().position(|value| &value.node == before) else {
                    return false;
                };
                index
            }
            None => array.len(),
        };
        array.insert(index, member);
        true
    }
    fn remove(&self, node: &CxfaNode) {
        let removed = {
            let mut array = self.array.borrow_mut();
            array
                .iter()
                .position(|value| &value.node == node)
                .map(|index| array.remove(index))
        };
        drop(removed);
    }
    fn item(&self, index: usize) -> Option<CxfaNode> {
        self.array
            .borrow()
            .get(index)
            .map(|member| member.node.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xfa::fxfa::{
        fxfa_basic::{XfaElement, XfaPacketType},
        parser::cxfa_treelist::CxfaTreeList,
    };
    fn node(doc: &Rc<CxfaDocument>, element: XfaElement) -> CxfaNode {
        doc.create_node(XfaPacketType::Template, element).unwrap()
    }
    #[test]
    fn snapshot_insertion_removes_first_duplicate_and_empty_set_preserves_items() {
        let doc = CxfaDocument::new();
        let first = node(&doc, XfaElement::Subform);
        let second = node(&doc, XfaElement::Subform);
        let missing = node(&doc, XfaElement::Subform);
        let list = CxfaArrayNodeList::new(&doc);
        assert!(list.append(&first));
        assert!(list.append(&second));
        assert!(list.insert(&first, Some(&second)));
        assert_eq!(list.get_length(), 3);
        assert!(!list.insert(&missing, Some(&missing)));
        assert_eq!(list.get_length(), 3);
        assert!(list.set_array_node_list(&[]));
        assert_eq!(list.get_length(), 3);
        list.remove(&first);
        assert_eq!(list.item(0), Some(first.clone()));
        assert_eq!(list.item(1), Some(second.clone()));
        assert!(list.item(2).is_none());
        list.remove(&missing);
        assert_eq!(list.get_length(), 2);
        assert!(list.set_array_node_list(std::slice::from_ref(&second)));
        assert_eq!(list.get_length(), 1);
        assert_eq!(list.item(0), Some(second));
        // Default unnamed nodes have the source empty-name hash.
        assert_eq!(list.named_item(&[]), list.item(0));
    }
    #[test]
    fn cross_document_members_retain_nested_nodes_after_documents_drop() {
        let doc = CxfaDocument::new();
        let other = CxfaDocument::new();
        let root = node(&doc, XfaElement::Subform);
        let child = node(&doc, XfaElement::Subform);
        let grandchild = node(&doc, XfaElement::Field);
        assert!(root.insert_child_and_notify(-1, &child));
        assert!(child.insert_child_and_notify(-1, &grandchild));
        let foreign = node(&other, XfaElement::Subform);
        let list = CxfaArrayNodeList::new(&doc);
        assert!(list.append(&root));
        assert!(list.append(&foreign));
        drop(doc);
        drop(other);
        assert!(list.document().is_none());
        assert_eq!(list.item(0), Some(root.clone()));
        assert_eq!(root.first_child(), Some(child.clone()));
        assert_eq!(child.first_child(), Some(grandchild.clone()));
        assert_eq!(list.item(1), Some(foreign.clone()));
        list.remove(&root);
        assert!(!root.is_alive());
        assert!(!child.is_alive());
        assert!(!grandchild.is_alive());
        assert!(foreign.is_alive());
        list.remove(&foreign);
        assert!(!foreign.is_alive());
    }
    #[test]
    fn persisted_lists_do_not_retain_the_document() {
        let doc = CxfaDocument::new();
        let weak_document = Rc::downgrade(&doc);
        let root = node(&doc, XfaElement::Subform);
        let list = CxfaArrayNodeList::new(&doc);
        let weak_list = Rc::downgrade(&list);
        assert!(list.append(&root));
        doc.get_node_owner().persist_list(list.clone());
        assert_eq!(doc.get_node_owner().persisted_list_count(), 1);
        drop(list);
        assert!(weak_list.upgrade().is_some());
        drop(doc);
        assert!(weak_document.upgrade().is_none());
        assert!(weak_list.upgrade().is_none());
        assert!(!root.is_alive());
    }
    #[test]
    fn named_lookup_is_case_sensitive_and_returns_first_matching_name_hash() {
        use crate::xfa::fxfa::fxfa_basic::XfaAttribute;
        let doc = CxfaDocument::new();
        let first = node(&doc, XfaElement::Subform);
        let second = node(&doc, XfaElement::Subform);
        let name: Vec<u32> = "item".chars().map(u32::from).collect();
        first.set_cdata(XfaAttribute::Name, &name);
        second.set_cdata(XfaAttribute::Name, &name);
        let list = CxfaArrayNodeList::new(&doc);
        assert!(list.append(&first));
        assert!(list.append(&second));
        assert_eq!(list.named_item(&name), Some(first.clone()));
        assert!(list
            .named_item(&"Item".chars().map(u32::from).collect::<Vec<_>>())
            .is_none());
        list.remove(&first);
        assert_eq!(list.named_item(&name), Some(second.clone()));
        // Native NamedItem compares hashes, not complete names.
        second.set_cdata(XfaAttribute::Name, &[1, 0]);
        assert_eq!(list.named_item(&[0, 1313]), Some(second));
    }
}
