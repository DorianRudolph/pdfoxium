// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_readynodeiterator.{h,cpp}`: ready-node preorder filtering and subtree
//! skipping. The retained arena keeps detached node identities valid.
use super::parser::{
    cxfa_document::CxfaNodeStore, cxfa_node::CxfaNode,
    cxfa_traversestrategy_xfacontainernode::CxfaContainerIterator,
};
use std::rc::Rc;
pub struct CxfaReadyNodeIterator {
    iterator: CxfaContainerIterator,
    current: Option<CxfaNode>,
    _store: Option<Rc<CxfaNodeStore>>,
}
impl CxfaReadyNodeIterator {
    pub fn new(root: Option<CxfaNode>) -> Self {
        let store = root.as_ref().and_then(CxfaNode::retain_store);
        Self {
            iterator: CxfaContainerIterator::new(root),
            current: None,
            _store: store,
        }
    }
    pub fn move_to_next(&mut self) -> Option<CxfaNode> {
        let mut node = if self.current.is_some() {
            self.iterator.move_to_next()
        } else {
            self.iterator.get_current()
        };
        while let Some(item) = node {
            self.current = item.is_widget_ready().then_some(item);
            if self.current.is_some() {
                return self.current.clone();
            }
            node = self.iterator.move_to_next();
        }
        None
    }
    pub fn skip_tree(&mut self) {
        self.iterator.skip_children_and_move_to_next();
        self.current = None;
    }
}
#[cfg(test)]
#[path = "cxfa_readynodeiterator_tests.rs"]
mod tests;
