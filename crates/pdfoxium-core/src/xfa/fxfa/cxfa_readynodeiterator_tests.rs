// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived preorder/readiness/SkipTree and arena-ownership cases.
use super::*;
use crate::xfa::fxfa::{
    fxfa_basic::{XfaElement as E, XfaPacketType as P},
    parser::cxfa_document::CxfaDocument,
};
fn node(document: &Rc<CxfaDocument>, parent: Option<&CxfaNode>, kind: E, ready: bool) -> CxfaNode {
    let node = document.create_node(P::Form, kind).unwrap();
    if ready {
        node.set_widget_ready();
    }
    if let Some(parent) = parent {
        assert!(parent.insert_child_and_notify(-1, &node));
    }
    node
}
#[test]
fn unready_containers_are_filtered_without_hiding_ready_descendants() {
    let document = CxfaDocument::new();
    let root = node(&document, None, E::Subform, false);
    let unready = node(&document, Some(&root), E::Subform, false);
    let first = node(&document, Some(&unready), E::Field, true);
    let _noncontainer = node(&document, Some(&root), E::Ui, true);
    let second = node(&document, Some(&root), E::Field, true);
    let mut iterator = CxfaReadyNodeIterator::new(Some(root));
    assert_eq!(iterator.move_to_next(), Some(first));
    assert_eq!(iterator.move_to_next(), Some(second));
    assert!(iterator.move_to_next().is_none());
    assert!(iterator.move_to_next().is_none());
}
#[test]
fn skip_tree_returns_the_next_ready_sibling_without_skipping_it_twice() {
    let document = CxfaDocument::new();
    let root = node(&document, None, E::Subform, true);
    let group = node(&document, Some(&root), E::ExclGroup, true);
    node(&document, Some(&group), E::Field, true);
    let unready = node(&document, Some(&root), E::Subform, false);
    let nested = node(&document, Some(&unready), E::Field, true);
    let last = node(&document, Some(&root), E::Field, true);
    let mut iterator = CxfaReadyNodeIterator::new(Some(root));
    assert!(iterator.move_to_next().is_some());
    assert_eq!(iterator.move_to_next(), Some(group));
    iterator.skip_tree();
    assert_eq!(iterator.move_to_next(), Some(nested));
    assert_eq!(iterator.move_to_next(), Some(last));
    iterator.skip_tree();
    assert!(iterator.move_to_next().is_none());
    let mut empty = CxfaReadyNodeIterator::new(None);
    empty.skip_tree();
    assert!(empty.move_to_next().is_none());
}
#[test]
fn readiness_is_observed_when_visited_and_iterator_retains_only_the_arena() {
    let document = CxfaDocument::new();
    let root = node(&document, None, E::Subform, true);
    let next = node(&document, Some(&root), E::Field, false);
    let mut iterator = CxfaReadyNodeIterator::new(Some(root.clone()));
    assert_eq!(iterator.move_to_next(), Some(root));
    next.set_widget_ready();
    let weak = Rc::downgrade(&document);
    drop(document);
    assert!(weak.upgrade().is_none());
    assert_eq!(iterator.move_to_next(), Some(next.clone()));
    assert!(next.is_alive());
    drop(iterator);
    assert!(!next.is_alive());
}
