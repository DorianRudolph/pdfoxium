// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fxcrt/tree_node.h`. Owners replace the external allocation, while
//! every tree edge and public node handle is nonowning. Payload access is
//! scoped so no RefCell borrow escapes an ownership-preserving operation.

use std::cell::RefCell;
use std::fmt;
use std::rc::{Rc, Weak};

struct TreeNode<T> {
    value: T,
    parent: Option<TreeNodeRef<T>>,
    first_child: Option<TreeNodeRef<T>>,
    last_child: Option<TreeNodeRef<T>>,
    next_sibling: Option<TreeNodeRef<T>>,
    prev_sibling: Option<TreeNodeRef<T>>,
}

pub struct TreeNodeOwner<T>(Rc<RefCell<TreeNode<T>>>);
pub struct TreeNodeRef<T>(Weak<RefCell<TreeNode<T>>>);

impl<T> Clone for TreeNodeRef<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> PartialEq for TreeNodeRef<T> {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}
impl<T> Eq for TreeNodeRef<T> {}
impl<T> std::hash::Hash for TreeNodeRef<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Weak allocation identity is stable even after its owner expires.
        std::hash::Hash::hash(&self.0.as_ptr(), state);
    }
}
impl<T> fmt::Debug for TreeNodeRef<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("TreeNodeRef")
            .field(&self.0.as_ptr())
            .finish()
    }
}
impl<T> TreeNodeOwner<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(RefCell::new(TreeNode {
            value,
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            prev_sibling: None,
        })))
    }
    pub fn node_ref(&self) -> TreeNodeRef<T> {
        TreeNodeRef(Rc::downgrade(&self.0))
    }
}
impl<T> TreeNodeRef<T> {
    pub fn is_alive(&self) -> bool {
        self.0.strong_count() != 0
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self == other
    }
    fn with_node<R>(&self, f: impl FnOnce(&TreeNode<T>) -> R) -> R {
        let node = self.0.upgrade().expect("tree node owner has expired");
        let result = f(&node.borrow());
        result
    }
    fn with_node_mut<R>(&self, f: impl FnOnce(&mut TreeNode<T>) -> R) -> R {
        let node = self.0.upgrade().expect("tree node owner has expired");
        let result = f(&mut node.borrow_mut());
        result
    }
    pub fn with_value<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.with_node(|node| f(&node.value))
    }
    pub fn with_value_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        self.with_node_mut(|node| f(&mut node.value))
    }
    pub fn parent(&self) -> Option<Self> {
        self.with_node(|n| n.parent.clone())
    }
    pub fn first_child(&self) -> Option<Self> {
        self.with_node(|n| n.first_child.clone())
    }
    pub fn last_child(&self) -> Option<Self> {
        self.with_node(|n| n.last_child.clone())
    }
    pub fn next_sibling(&self) -> Option<Self> {
        self.with_node(|n| n.next_sibling.clone())
    }
    pub fn prev_sibling(&self) -> Option<Self> {
        self.with_node(|n| n.prev_sibling.clone())
    }
    pub fn has_child(&self, child: &Self) -> bool {
        child != self && child.parent().as_ref() == Some(self)
    }
    pub fn nth_child(&self, mut n: i32) -> Option<Self> {
        if n < 0 {
            return None;
        }
        let mut result = self.first_child();
        while n > 0 {
            let child = result?;
            result = child.next_sibling();
            n -= 1;
        }
        result
    }
    fn become_parent(&self, child: &Self) {
        assert_ne!(child, self, "tree node self-insertion");
        if let Some(parent) = child.parent() {
            parent.remove_child(child);
        }
        child.with_node_mut(|n| n.parent = Some(self.clone()));
        assert!(child.next_sibling().is_none());
        assert!(child.prev_sibling().is_none());
    }
    pub fn append_first_child(&self, child: &Self) {
        self.become_parent(child);
        if let Some(first) = self.first_child() {
            assert!(self.last_child().is_some());
            first.with_node_mut(|n| n.prev_sibling = Some(child.clone()));
            child.with_node_mut(|n| n.next_sibling = Some(first));
            self.with_node_mut(|n| n.first_child = Some(child.clone()));
        } else {
            assert!(self.last_child().is_none());
            self.with_node_mut(|n| {
                n.first_child = Some(child.clone());
                n.last_child = Some(child.clone());
            });
        }
    }
    pub fn append_last_child(&self, child: &Self) {
        self.become_parent(child);
        if let Some(last) = self.last_child() {
            assert!(self.first_child().is_some());
            last.with_node_mut(|n| n.next_sibling = Some(child.clone()));
            child.with_node_mut(|n| n.prev_sibling = Some(last));
            self.with_node_mut(|n| n.last_child = Some(child.clone()));
        } else {
            assert!(self.first_child().is_none());
            self.with_node_mut(|n| {
                n.first_child = Some(child.clone());
                n.last_child = Some(child.clone());
            });
        }
    }
    pub fn insert_before(&self, child: &Self, other: Option<&Self>) {
        let Some(other) = other else {
            self.append_last_child(child);
            return;
        };
        self.become_parent(child);
        assert!(self.has_child(other), "insertion anchor is not a child");
        let previous = other.prev_sibling();
        child.with_node_mut(|n| {
            n.next_sibling = Some(other.clone());
            n.prev_sibling = previous.clone();
        });
        if self.first_child().as_ref() == Some(other) {
            assert!(previous.is_none());
            self.with_node_mut(|n| n.first_child = Some(child.clone()));
        } else {
            previous
                .unwrap()
                .with_node_mut(|n| n.next_sibling = Some(child.clone()));
        }
        other.with_node_mut(|n| n.prev_sibling = Some(child.clone()));
    }
    pub fn insert_after(&self, child: &Self, other: Option<&Self>) {
        let Some(other) = other else {
            self.append_first_child(child);
            return;
        };
        self.become_parent(child);
        assert!(self.has_child(other), "insertion anchor is not a child");
        let next = other.next_sibling();
        child.with_node_mut(|n| {
            n.next_sibling = next.clone();
            n.prev_sibling = Some(other.clone());
        });
        if self.last_child().as_ref() == Some(other) {
            assert!(next.is_none());
            self.with_node_mut(|n| n.last_child = Some(child.clone()));
        } else {
            next.unwrap()
                .with_node_mut(|n| n.prev_sibling = Some(child.clone()));
        }
        other.with_node_mut(|n| n.next_sibling = Some(child.clone()));
    }
    pub fn remove_child(&self, child: &Self) {
        assert!(self.has_child(child), "node is not a child");
        let next = child.next_sibling();
        let previous = child.prev_sibling();
        if self.last_child().as_ref() == Some(child) {
            assert!(next.is_none());
            self.with_node_mut(|n| n.last_child = previous.clone());
        } else {
            next.as_ref()
                .unwrap()
                .with_node_mut(|n| n.prev_sibling = previous.clone());
        }
        if self.first_child().as_ref() == Some(child) {
            assert!(previous.is_none());
            self.with_node_mut(|n| n.first_child = next.clone());
        } else {
            previous.unwrap().with_node_mut(|n| n.next_sibling = next);
        }
        child.with_node_mut(|n| {
            n.parent = None;
            n.prev_sibling = None;
            n.next_sibling = None;
        });
    }
    pub fn remove_all_children(&self) {
        while let Some(child) = self.first_child() {
            self.remove_child(&child);
        }
    }
    pub fn remove_self_if_parented(&self) {
        if let Some(parent) = self.parent() {
            parent.remove_child(self);
        }
    }
}

#[cfg(test)]
#[path = "tree_node_unittest.rs"]
mod upstream_tests;
