// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Complete cases from the same-named pinned C++ source.
#![allow(non_snake_case)]
use super::*;

fn assert_panics(action: impl FnOnce()) {
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(action)).is_err());
}

#[test]
fn SelfAppendFirstChild() {
    let _pNode = TreeNodeOwner::new(());
    let pNode = _pNode.node_ref();
    assert_panics(|| {
        pNode.append_first_child(&pNode);
    });
}

#[test]
fn SelfAppendLastChild() {
    let _pNode = TreeNodeOwner::new(());
    let pNode = _pNode.node_ref();
    assert_panics(|| {
        pNode.append_last_child(&pNode);
    });
}

#[test]
fn SelfInsertBeforeOther() {
    let _pNode = TreeNodeOwner::new(());
    let pNode = _pNode.node_ref();
    let _pOther = TreeNodeOwner::new(());
    let pOther = _pOther.node_ref();
    pNode.append_first_child(&pOther);
    assert_panics(|| {
        pNode.insert_before(&pNode, Some(&pOther));
    });
}

#[test]
fn InsertOtherBeforeSelf() {
    let _pNode = TreeNodeOwner::new(());
    let pNode = _pNode.node_ref();
    let _pOther = TreeNodeOwner::new(());
    let pOther = _pOther.node_ref();
    pNode.append_first_child(&pOther);
    assert_panics(|| {
        pNode.insert_before(&pOther, Some(&pNode));
    });
}

#[test]
fn SelfInsertAfterOther() {
    let _pNode = TreeNodeOwner::new(());
    let pNode = _pNode.node_ref();
    let _pOther = TreeNodeOwner::new(());
    let pOther = _pOther.node_ref();
    pNode.append_first_child(&pOther);
    assert_panics(|| {
        pNode.insert_before(&pNode, Some(&pOther));
    });
}

#[test]
fn InsertOtherAfterSelf() {
    let _pNode = TreeNodeOwner::new(());
    let pNode = _pNode.node_ref();
    let _pOther = TreeNodeOwner::new(());
    let pOther = _pOther.node_ref();
    pNode.append_first_child(&pOther);
    assert_panics(|| {
        pNode.insert_before(&pOther, Some(&pNode));
    });
}

#[test]
fn RemoveParentless() {
    let _pNode = TreeNodeOwner::new(());
    let pNode = _pNode.node_ref();
    assert_panics(|| {
        pNode.parent().unwrap().remove_child(&pNode);
    });
}

#[test]
fn RemoveFromWrongParent() {
    let _pGoodParent = TreeNodeOwner::new(());
    let pGoodParent = _pGoodParent.node_ref();
    let _pBadParent = TreeNodeOwner::new(());
    let pBadParent = _pBadParent.node_ref();
    let _pNode = TreeNodeOwner::new(());
    let pNode = _pNode.node_ref();
    pGoodParent.append_first_child(&pNode);
    assert_panics(|| {
        pBadParent.remove_child(&pNode);
    });
}

#[test]
fn SafeRemove() {
    let _pParent = TreeNodeOwner::new(());
    let pParent = _pParent.node_ref();
    let _pChild = TreeNodeOwner::new(());
    let pChild = _pChild.node_ref();
    pParent.append_first_child(&pChild);
    pChild.remove_self_if_parented();
    assert!(pParent.first_child().is_none());
    assert!(pChild.parent().is_none());
}

#[test]
fn SafeRemoveParentless() {
    let _pNode = TreeNodeOwner::new(());
    let pNode = _pNode.node_ref();
    pNode.remove_self_if_parented();
    assert!(pNode.parent().is_none());
}

#[test]
fn RemoveAllChildren() {
    let _pParent = TreeNodeOwner::new(());
    let pParent = _pParent.node_ref();
    pParent.remove_all_children();
    assert!(pParent.first_child().is_none());
    let _p0 = TreeNodeOwner::new(());
    let p0 = _p0.node_ref();
    let _p1 = TreeNodeOwner::new(());
    let p1 = _p1.node_ref();
    let _p2 = TreeNodeOwner::new(());
    let p2 = _p2.node_ref();
    let _p3 = TreeNodeOwner::new(());
    let p3 = _p3.node_ref();
    pParent.append_last_child(&p0);
    pParent.append_last_child(&p1);
    pParent.append_last_child(&p2);
    pParent.append_last_child(&p3);
    pParent.remove_all_children();
    assert!(pParent.first_child().is_none());
}

#[test]
fn NthChild() {
    let _pParent = TreeNodeOwner::new(());
    let pParent = _pParent.node_ref();
    assert!(pParent.nth_child(-1).is_none());
    assert!(pParent.nth_child(0).is_none());
    let _p0 = TreeNodeOwner::new(());
    let p0 = _p0.node_ref();
    let _p1 = TreeNodeOwner::new(());
    let p1 = _p1.node_ref();
    let _p2 = TreeNodeOwner::new(());
    let p2 = _p2.node_ref();
    let _p3 = TreeNodeOwner::new(());
    let p3 = _p3.node_ref();
    pParent.append_last_child(&p0);
    pParent.append_last_child(&p1);
    pParent.append_last_child(&p2);
    pParent.append_last_child(&p3);
    assert!(pParent.nth_child(-1).is_none());
    assert_eq!(Some(p0.clone()), pParent.nth_child(0));
    assert_eq!(Some(p1.clone()), pParent.nth_child(1));
    assert_eq!(Some(p2.clone()), pParent.nth_child(2));
    assert_eq!(Some(p3.clone()), pParent.nth_child(3));
    assert!(pParent.nth_child(4).is_none());
    pParent.remove_all_children();
}

#[test]
fn AppendFirstChild() {
    let _parent = TreeNodeOwner::new(());
    let parent = _parent.node_ref();
    let _child0 = TreeNodeOwner::new(());
    let child0 = _child0.node_ref();
    let _child1 = TreeNodeOwner::new(());
    let child1 = _child1.node_ref();
    parent.append_first_child(&child0);
    assert_eq!(Some(child0.clone()), parent.first_child());
    parent.append_first_child(&child1);
    assert_eq!(Some(child1.clone()), parent.first_child());
    assert_eq!(Some(child1.clone()), parent.nth_child(0));
    assert_eq!(Some(child0.clone()), parent.nth_child(1));
}

#[test]
fn RemoveChild() {
    let _parent = TreeNodeOwner::new(());
    let parent = _parent.node_ref();
    let _child0 = TreeNodeOwner::new(());
    let child0 = _child0.node_ref();
    let _child1 = TreeNodeOwner::new(());
    let child1 = _child1.node_ref();
    parent.append_first_child(&child0);
    parent.append_last_child(&child1);
    assert_eq!(Some(child0.clone()), parent.first_child());
    assert_eq!(Some(child1.clone()), parent.last_child());
    parent.remove_child(&child0);
    assert_eq!(Some(child1.clone()), parent.first_child());
    assert_eq!(Some(child1.clone()), parent.last_child());
    parent.remove_child(&child1);
    assert!(parent.first_child().is_none());
    assert!(parent.last_child().is_none());
    parent.append_first_child(&child0);
    parent.append_last_child(&child1);
    assert_eq!(Some(child0.clone()), parent.first_child());
    assert_eq!(Some(child1.clone()), parent.last_child());
    parent.remove_child(&child1);
    assert_eq!(Some(child0.clone()), parent.first_child());
    assert_eq!(Some(child0.clone()), parent.last_child());
    parent.remove_child(&child0);
    assert!(parent.first_child().is_none());
    assert!(parent.last_child().is_none());
}
