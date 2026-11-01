// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_nodeiteratortemplate.h`: preorder traversal within an immutable root
//! boundary. Node values are identity handles; cloning a handle never clones
//! a subtree. The owner must keep the traversed nodes alive.
use std::marker::PhantomData;

pub trait TraverseStrategy<N> {
    fn get_first_child(node: &N) -> Option<N>;
    fn get_next_sibling(node: &N) -> Option<N>;
    fn get_parent(node: &N) -> Option<N>;
}
pub struct CxfaNodeIteratorTemplate<N, S> {
    root: Option<N>,
    current: Option<N>,
    strategy: PhantomData<S>,
}
impl<N: Clone + Eq, S: TraverseStrategy<N>> CxfaNodeIteratorTemplate<N, S> {
    pub fn new(root: Option<N>) -> Self {
        Self {
            current: root.clone(),
            root,
            strategy: PhantomData,
        }
    }
    pub fn get_root(&self) -> Option<N> {
        self.root.clone()
    }
    pub fn get_current(&self) -> Option<N> {
        self.current.clone()
    }
    pub fn reset(&mut self) {
        self.current = self.root.clone();
    }
    pub fn set_current(&mut self, node: Option<N>) -> bool {
        if !self.root_reachable_from_node(node.as_ref()) {
            self.current = None;
            return false;
        }
        self.current = node;
        true
    }
    pub fn move_to_prev(&mut self) -> Option<N> {
        let root = self.root.clone()?;
        let Some(current) = self.current.clone() else {
            let last = self.last_descendant(root);
            self.current = Some(last.clone());
            return Some(last);
        };
        if let Some(sibling) = self.previous_sibling_within_subtree(&current) {
            let last = self.last_descendant(sibling);
            self.current = Some(last.clone());
            return Some(last);
        }
        let parent = self.parent_within_subtree(&current);
        if parent.is_some() {
            self.current = parent.clone();
        }
        parent
    }
    pub fn move_to_next(&mut self) -> Option<N> {
        self.root.as_ref()?;
        let current = self.current.as_ref()?;
        if let Some(child) = S::get_first_child(current) {
            self.current = Some(child.clone());
            return Some(child);
        }
        self.skip_children_and_move_to_next()
    }
    pub fn skip_children_and_move_to_next(&mut self) -> Option<N> {
        self.root.as_ref()?;
        let mut node = self.current.clone();
        while let Some(current) = node {
            if let Some(sibling) = self.next_sibling_within_subtree(&current) {
                self.current = Some(sibling.clone());
                return Some(sibling);
            }
            node = self.parent_within_subtree(&current);
        }
        self.current = None;
        None
    }
    fn root_reachable_from_node(&self, node: Option<&N>) -> bool {
        let mut node = node.cloned();
        while let Some(current) = node {
            if self.root.as_ref() == Some(&current) {
                return true;
            }
            node = S::get_parent(&current);
        }
        false
    }
    fn parent_within_subtree(&self, node: &N) -> Option<N> {
        if self.root.as_ref() == Some(node) {
            None
        } else {
            S::get_parent(node)
        }
    }
    fn next_sibling_within_subtree(&self, node: &N) -> Option<N> {
        if self.root.as_ref() == Some(node) {
            None
        } else {
            S::get_next_sibling(node)
        }
    }
    fn previous_sibling_within_subtree(&self, node: &N) -> Option<N> {
        let parent = self.parent_within_subtree(node)?;
        let mut current = S::get_first_child(&parent);
        let mut previous = None;
        while let Some(candidate) = current {
            if &candidate == node {
                return previous;
            }
            previous = Some(candidate.clone());
            current = S::get_next_sibling(&candidate);
        }
        None
    }
    fn last_child(&self, node: &N) -> Option<N> {
        let mut child = S::get_first_child(node);
        let mut previous = None;
        while let Some(current) = child {
            child = self.next_sibling_within_subtree(&current);
            previous = Some(current);
        }
        previous
    }
    fn last_descendant(&self, mut node: N) -> N {
        while let Some(child) = self.last_child(&node) {
            node = child;
        }
        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::tree_node::{TreeNodeOwner, TreeNodeRef};
    type Node = TreeNodeRef<()>;
    struct Strategy;
    impl TraverseStrategy<Node> for Strategy {
        fn get_first_child(node: &Node) -> Option<Node> {
            node.first_child()
        }
        fn get_next_sibling(node: &Node) -> Option<Node> {
            node.next_sibling()
        }
        fn get_parent(node: &Node) -> Option<Node> {
            node.parent()
        }
    }
    type Iterator = CxfaNodeIteratorTemplate<Node, Strategy>;
    struct Fixture {
        _owners: Vec<TreeNodeOwner<()>>,
        nodes: Vec<Node>,
    }
    impl Fixture {
        fn new() -> Self {
            let owners: Vec<_> = (0..10).map(|_| TreeNodeOwner::new(())).collect();
            let nodes: Vec<_> = owners.iter().map(TreeNodeOwner::node_ref).collect();
            for (child, parent) in [
                (1, 0),
                (2, 0),
                (3, 2),
                (4, 3),
                (5, 4),
                (6, 3),
                (7, 2),
                (8, 7),
                (9, 2),
            ] {
                nodes[parent].append_last_child(&nodes[child]);
            }
            Self {
                _owners: owners,
                nodes,
            }
        }
        fn node(&self, index: usize) -> Option<Node> {
            Some(self.nodes[index].clone())
        }
    }
    #[test]
    fn empty() {
        let mut iter = Iterator::new(None);
        assert!(iter.get_root().is_none());
        assert!(iter.get_current().is_none());
        assert!(iter.move_to_next().is_none());
        assert!(iter.move_to_prev().is_none());
        assert!(iter.skip_children_and_move_to_next().is_none());
    }
    #[test]
    fn root() {
        let fixture = Fixture::new();
        let iter = Iterator::new(fixture.node(0));
        assert_eq!(fixture.node(0), iter.get_root());
        assert_eq!(fixture.node(0), iter.get_current());
    }
    #[test]
    fn current() {
        let fixture = Fixture::new();
        let mut iter = Iterator::new(fixture.node(0));
        assert!(iter.set_current(fixture.node(1)));
        assert_eq!(fixture.node(0), iter.get_root());
        assert_eq!(fixture.node(1), iter.get_current());
    }
    #[test]
    fn current_outside_root_disallowed() {
        let fixture = Fixture::new();
        let mut iter = Iterator::new(fixture.node(1));
        assert!(!iter.set_current(fixture.node(0)));
        assert_eq!(fixture.node(1), iter.get_root());
        assert!(iter.get_current().is_none());
    }
    #[test]
    fn current_null() {
        let fixture = Fixture::new();
        let mut iter = Iterator::new(fixture.node(0));
        assert_eq!(fixture.node(1), iter.move_to_next());
        assert!(!iter.set_current(None));
        assert!(iter.get_current().is_none());
        assert!(iter.move_to_next().is_none());
        assert!(iter.get_current().is_none());
    }
    #[test]
    fn move_to_prev() {
        let fixture = Fixture::new();
        let mut iter = Iterator::new(fixture.node(0));
        assert!(iter.set_current(fixture.node(9)));
        assert_eq!(fixture.node(8), iter.move_to_prev());
        assert_eq!(fixture.node(8), iter.get_current());
        assert_eq!(fixture.node(7), iter.move_to_prev());
        assert_eq!(fixture.node(7), iter.get_current());
        assert_eq!(fixture.node(6), iter.move_to_prev());
        assert_eq!(fixture.node(6), iter.get_current());
        assert_eq!(fixture.node(5), iter.move_to_prev());
        assert_eq!(fixture.node(5), iter.get_current());
        assert_eq!(fixture.node(4), iter.move_to_prev());
        assert_eq!(fixture.node(4), iter.get_current());
        assert_eq!(fixture.node(3), iter.move_to_prev());
        assert_eq!(fixture.node(3), iter.get_current());
        assert_eq!(fixture.node(2), iter.move_to_prev());
        assert_eq!(fixture.node(2), iter.get_current());
        assert_eq!(fixture.node(1), iter.move_to_prev());
        assert_eq!(fixture.node(1), iter.get_current());
        assert_eq!(fixture.node(0), iter.move_to_prev());
        assert_eq!(fixture.node(0), iter.get_current());
        assert!(iter.move_to_prev().is_none());
        assert_eq!(fixture.node(0), iter.get_current());
        assert!(iter.move_to_prev().is_none());
        assert_eq!(fixture.node(0), iter.get_current());
    }
    #[test]
    fn move_to_next() {
        let fixture = Fixture::new();
        let mut iter = Iterator::new(fixture.node(0));
        assert!(iter.set_current(fixture.node(2)));
        assert_eq!(fixture.node(3), iter.move_to_next());
        assert_eq!(fixture.node(3), iter.get_current());
        assert_eq!(fixture.node(4), iter.move_to_next());
        assert_eq!(fixture.node(4), iter.get_current());
        assert_eq!(fixture.node(5), iter.move_to_next());
        assert_eq!(fixture.node(5), iter.get_current());
        assert_eq!(fixture.node(6), iter.move_to_next());
        assert_eq!(fixture.node(6), iter.get_current());
        assert_eq!(fixture.node(7), iter.move_to_next());
        assert_eq!(fixture.node(7), iter.get_current());
        assert_eq!(fixture.node(8), iter.move_to_next());
        assert_eq!(fixture.node(8), iter.get_current());
        assert_eq!(fixture.node(9), iter.move_to_next());
        assert_eq!(fixture.node(9), iter.get_current());
        assert!(iter.move_to_next().is_none());
        assert!(iter.get_current().is_none());
        assert!(iter.move_to_next().is_none());
        assert!(iter.get_current().is_none());
    }
    #[test]
    fn skip_children_and_move_to_next() {
        let fixture = Fixture::new();
        let mut iter = Iterator::new(fixture.node(0));
        assert!(iter.set_current(fixture.node(3)));
        assert_eq!(fixture.node(7), iter.skip_children_and_move_to_next());
        assert_eq!(fixture.node(9), iter.skip_children_and_move_to_next());
        assert!(iter.skip_children_and_move_to_next().is_none());
    }
    #[test]
    fn back_and_forth() {
        let fixture = Fixture::new();
        let mut iter = Iterator::new(fixture.node(0));
        assert_eq!(fixture.node(1), iter.move_to_next());
        assert_eq!(fixture.node(2), iter.move_to_next());
        assert_eq!(fixture.node(3), iter.move_to_next());
        assert_eq!(fixture.node(4), iter.move_to_next());
        assert_eq!(fixture.node(5), iter.move_to_next());
        assert_eq!(fixture.node(4), iter.move_to_prev());
        assert_eq!(fixture.node(3), iter.move_to_prev());
        assert_eq!(fixture.node(2), iter.move_to_prev());
        assert_eq!(fixture.node(1), iter.move_to_prev());
    }
    #[test]
    fn next_from_before_the_beginning() {
        let fixture = Fixture::new();
        let mut iter = Iterator::new(fixture.node(0));
        assert!(iter.move_to_prev().is_none());
        assert_eq!(fixture.node(0), iter.get_current());
        assert_eq!(fixture.node(1), iter.move_to_next());
    }
    #[test]
    fn prev_from_after_the_end() {
        let fixture = Fixture::new();
        let mut iter = Iterator::new(fixture.node(0));
        assert!(iter.set_current(fixture.node(9)));
        assert!(iter.move_to_next().is_none());
        assert_eq!(fixture.node(9), iter.move_to_prev());
    }
    #[test]
    fn child_as_root_prev() {
        let fixture = Fixture::new();
        let mut iter = Iterator::new(fixture.node(3));
        assert!(iter.move_to_prev().is_none());
        assert!(iter.set_current(fixture.node(4)));
        assert_eq!(fixture.node(3), iter.move_to_prev());
        assert!(iter.move_to_prev().is_none());
    }
    #[test]
    fn child_as_root_next() {
        let fixture = Fixture::new();
        let mut iter = Iterator::new(fixture.node(3));
        assert!(iter.set_current(fixture.node(4)));
        assert_eq!(fixture.node(5), iter.move_to_next());
        assert_eq!(fixture.node(6), iter.move_to_next());
        assert!(iter.move_to_next().is_none());
    }
}
