// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Complete cases from the same-named pinned C++ source.
#![allow(non_snake_case)]
use super::XmlNodeRef;
use crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument;
use crate::fxcrt::xml::test_support::{assert_panics, wide};

fn children_string(parent: &XmlNodeRef) -> Vec<u32> {
    let mut result = Vec::new();
    let mut child = parent.first_child();
    while let Some(node) = child {
        result.extend_from_slice(&node.name());
        child = node.next_sibling();
    }
    result
}
fn reverse_children_string(parent: &XmlNodeRef) -> Vec<u32> {
    let mut result = Vec::new();
    let mut child = parent.last_child();
    while let Some(node) = child {
        let mut name = node.name().to_vec();
        name.extend(result);
        result = name;
        child = node.prev_sibling();
    }
    result
}

#[test]
fn GetParent() {
    let mut fixture = CfxXmlDocument::new();
    let node1 = fixture.create_element(&wide("node"));
    let node2 = fixture.create_element(&wide("node2"));
    let node3 = fixture.create_element(&wide("node3"));
    node1.append_last_child(&node2);
    node2.append_last_child(&node3);
    assert!(node1.parent().is_none());
    assert_eq!(Some(node1.clone()), node2.parent());
    assert_eq!(Some(node2.clone()), node3.parent());
}

#[test]
fn GetRoot() {
    let mut fixture = CfxXmlDocument::new();
    let node1 = fixture.create_element(&wide("node"));
    let node2 = fixture.create_element(&wide("node2"));
    let node3 = fixture.create_element(&wide("node3"));
    node1.append_last_child(&node2);
    node2.append_last_child(&node3);
    assert_eq!(node1.clone(), node1.root());
    assert_eq!(node1.clone(), node2.root());
    assert_eq!(node1.clone(), node3.root());
}

#[test]
fn GetChildren() {
    let mut fixture = CfxXmlDocument::new();
    let node1 = fixture.create_element(&wide("node"));
    let node2 = fixture.create_element(&wide("node2"));
    let node3 = fixture.create_element(&wide("node3"));
    let node4 = fixture.create_element(&wide("node4"));
    node1.append_last_child(&node2);
    node1.append_last_child(&node4);
    node2.append_last_child(&node3);
    assert_eq!(Some(node2.clone()), node1.first_child());
    assert_eq!(Some(node4.clone()), node2.next_sibling());
    assert_eq!(Some(node3.clone()), node2.first_child());
    assert!(node3.next_sibling().is_none());
    assert!(node3.first_child().is_none());
    assert!(node4.next_sibling().is_none());
    assert!(node4.first_child().is_none());
}

#[test]
fn DeleteChildren() {
    let mut fixture = CfxXmlDocument::new();
    let node1 = fixture.create_element(&wide("node"));
    let node2 = fixture.create_element(&wide("node2"));
    let node3 = fixture.create_element(&wide("node3"));
    let node4 = fixture.create_element(&wide("node4"));
    node1.append_last_child(&node2);
    node1.append_last_child(&node4);
    node2.append_last_child(&node3);
    node1.remove_all_children();
    assert!(node1.first_child().is_none());
    assert!(node2.parent().is_none());
    assert!(node4.parent().is_none());
    // node2 and node4 should no longer be siblings.
    assert!(node2.next_sibling().is_none());
    assert!(node4.prev_sibling().is_none());
    // Deleting children doesn't change deleted substructure
    assert_eq!(Some(node3.clone()), node2.first_child());
    assert!(node3.parent() == Some(node2.clone()));
}

#[test]
fn AddingChildren() {
    let mut fixture = CfxXmlDocument::new();
    let parent = fixture.create_element(&wide("Root"));
    let nodeA = fixture.create_element(&wide("A"));
    let nodeB = fixture.create_element(&wide("B"));
    parent.append_last_child(&nodeA);
    parent.append_last_child(&nodeB);
    assert_eq!(wide("AB").as_slice(), children_string(&parent).as_slice());
    assert_eq!(
        wide("AB").as_slice(),
        reverse_children_string(&parent).as_slice()
    );
    assert_eq!(Some(parent.clone()), nodeA.parent());
    assert_eq!(Some(parent.clone()), nodeB.parent());
    assert_eq!(Some(nodeA.clone()), parent.first_child());
    assert_eq!(Some(nodeB.clone()), nodeA.next_sibling());
    assert!(nodeB.next_sibling().is_none());
    // Insert to negative appends last child.
    let nodeC = fixture.create_element(&wide("C"));
    parent.insert_child_node(&nodeC, -1);
    assert_eq!(wide("ABC").as_slice(), children_string(&parent).as_slice());
    assert_eq!(
        wide("ABC").as_slice(),
        reverse_children_string(&parent).as_slice()
    );
    assert_eq!(Some(parent.clone()), nodeC.parent());
    assert_eq!(Some(nodeC.clone()), nodeB.next_sibling());
    assert!(nodeC.next_sibling().is_none());
    // Insertion occurs before a zero based index.
    let nodeD = fixture.create_element(&wide("D"));
    parent.insert_child_node(&nodeD, 1);
    assert_eq!(wide("ADBC").as_slice(), children_string(&parent).as_slice());
    assert_eq!(
        wide("ADBC").as_slice(),
        reverse_children_string(&parent).as_slice()
    );
    // Insert to 0 appends first child.
    let nodeE = fixture.create_element(&wide("E"));
    parent.insert_child_node(&nodeE, 0);
    assert_eq!(
        wide("EADBC").as_slice(),
        children_string(&parent).as_slice()
    );
    assert_eq!(
        wide("EADBC").as_slice(),
        reverse_children_string(&parent).as_slice()
    );
    // Insert to out-of-bounds index appends last child.
    let nodeF = fixture.create_element(&wide("F"));
    parent.insert_child_node(&nodeF, 10);
    assert_eq!(
        wide("EADBCF").as_slice(),
        children_string(&parent).as_slice()
    );
    assert_eq!(
        wide("EADBCF").as_slice(),
        reverse_children_string(&parent).as_slice()
    );
}

#[test]
fn RemovingMiddleChild() {
    let mut fixture = CfxXmlDocument::new();
    let node1 = fixture.create_element(&wide("node1"));
    let node2 = fixture.create_element(&wide("node2"));
    let node3 = fixture.create_element(&wide("node3"));
    let node4 = fixture.create_element(&wide("node4"));
    node1.append_last_child(&node2);
    node1.append_last_child(&node3);
    node1.append_last_child(&node4);
    assert_eq!(
        wide("node2node3node4").as_slice(),
        children_string(&node1).as_slice()
    );
    assert_eq!(
        wide("node2node3node4").as_slice(),
        reverse_children_string(&node1).as_slice()
    );
    assert_eq!(Some(node2.clone()), node1.first_child());
    assert_eq!(Some(node3.clone()), node2.next_sibling());
    assert_eq!(Some(node4.clone()), node3.next_sibling());
    assert!(node4.next_sibling().is_none());
    node1.remove_child(&node3);
    assert_eq!(
        wide("node2node4").as_slice(),
        children_string(&node1).as_slice()
    );
    assert_eq!(
        wide("node2node4").as_slice(),
        reverse_children_string(&node1).as_slice()
    );
    assert!(node3.parent().is_none());
    assert!(node3.next_sibling().is_none());
    assert!(node3.prev_sibling().is_none());
    assert_eq!(Some(node2.clone()), node1.first_child());
    assert_eq!(Some(node4.clone()), node2.next_sibling());
    assert_eq!(Some(node2.clone()), node4.prev_sibling());
    assert!(node4.next_sibling().is_none());
}

#[test]
fn RemovingFirstChild() {
    let mut fixture = CfxXmlDocument::new();
    let node1 = fixture.create_element(&wide("node1"));
    let node2 = fixture.create_element(&wide("node2"));
    let node3 = fixture.create_element(&wide("node3"));
    let node4 = fixture.create_element(&wide("node4"));
    node1.append_last_child(&node2);
    node1.append_last_child(&node3);
    node1.append_last_child(&node4);
    assert_eq!(
        wide("node2node3node4").as_slice(),
        children_string(&node1).as_slice()
    );
    assert_eq!(
        wide("node2node3node4").as_slice(),
        reverse_children_string(&node1).as_slice()
    );
    assert_eq!(Some(node2.clone()), node1.first_child());
    assert_eq!(Some(node3.clone()), node2.next_sibling());
    assert_eq!(Some(node4.clone()), node3.next_sibling());
    assert!(node4.next_sibling().is_none());
    node1.remove_child(&node2);
    assert_eq!(
        wide("node3node4").as_slice(),
        children_string(&node1).as_slice()
    );
    assert_eq!(
        wide("node3node4").as_slice(),
        reverse_children_string(&node1).as_slice()
    );
    assert!(node2.parent().is_none());
    assert!(node2.next_sibling().is_none());
    assert!(node2.prev_sibling().is_none());
    assert_eq!(Some(node3.clone()), node1.first_child());
    assert!(node3.prev_sibling().is_none());
    assert_eq!(Some(node4.clone()), node3.next_sibling());
    assert!(node4.next_sibling().is_none());
}

#[test]
fn RemovingLastChild() {
    let mut fixture = CfxXmlDocument::new();
    let node1 = fixture.create_element(&wide("node1"));
    let node2 = fixture.create_element(&wide("node2"));
    let node3 = fixture.create_element(&wide("node3"));
    let node4 = fixture.create_element(&wide("node4"));
    node1.append_last_child(&node2);
    node1.append_last_child(&node3);
    node1.append_last_child(&node4);
    assert_eq!(
        wide("node2node3node4").as_slice(),
        children_string(&node1).as_slice()
    );
    assert_eq!(
        wide("node2node3node4").as_slice(),
        reverse_children_string(&node1).as_slice()
    );
    assert_eq!(Some(node2.clone()), node1.first_child());
    assert_eq!(Some(node3.clone()), node2.next_sibling());
    assert_eq!(Some(node4.clone()), node3.next_sibling());
    assert!(node4.next_sibling().is_none());
    node1.remove_child(&node4);
    assert_eq!(
        wide("node2node3").as_slice(),
        children_string(&node1).as_slice()
    );
    assert_eq!(
        wide("node2node3").as_slice(),
        reverse_children_string(&node1).as_slice()
    );
    assert!(node4.parent().is_none());
    assert!(node4.next_sibling().is_none());
    assert!(node4.prev_sibling().is_none());
    assert_eq!(Some(node2.clone()), node1.first_child());
    assert_eq!(Some(node3.clone()), node2.next_sibling());
    assert!(node3.next_sibling().is_none());
}

#[test]
fn RemovingOnlyChild() {
    let mut fixture = CfxXmlDocument::new();
    let node1 = fixture.create_element(&wide("node1"));
    let node2 = fixture.create_element(&wide("node2"));
    node1.append_last_child(&node2);
    assert_eq!(Some(node2.clone()), node1.first_child());
    assert!(node2.next_sibling().is_none());
    node1.remove_child(&node2);
    assert!(node2.parent().is_none());
    assert!(node1.first_child().is_none());
    assert!(node2.next_sibling().is_none());
    assert!(node2.prev_sibling().is_none());
}

#[test]
fn RemoveMissingChild() {
    let mut fixture = CfxXmlDocument::new();
    let node1 = fixture.create_element(&wide("node1"));
    let node2 = fixture.create_element(&wide("node2"));
    let node3 = fixture.create_element(&wide("node3"));
    node1.append_last_child(&node2);
    assert_panics(|| {
        node1.remove_child(&node3);
    });
}
