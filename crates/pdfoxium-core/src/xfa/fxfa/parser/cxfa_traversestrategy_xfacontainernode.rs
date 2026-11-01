// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_traversestrategy_xfacontainernode.h`.
use super::{
    cxfa_node::CxfaNode,
    cxfa_nodeiteratortemplate::{CxfaNodeIteratorTemplate, TraverseStrategy},
};
pub struct CxfaTraverseStrategyXfaContainerNode;
impl TraverseStrategy<CxfaNode> for CxfaTraverseStrategyXfaContainerNode {
    fn get_first_child(node: &CxfaNode) -> Option<CxfaNode> {
        node.get_first_container_child()
    }
    fn get_next_sibling(node: &CxfaNode) -> Option<CxfaNode> {
        node.get_next_container_sibling()
    }
    fn get_parent(node: &CxfaNode) -> Option<CxfaNode> {
        node.get_container_parent()
    }
}
pub type CxfaContainerIterator =
    CxfaNodeIteratorTemplate<CxfaNode, CxfaTraverseStrategyXfaContainerNode>;
