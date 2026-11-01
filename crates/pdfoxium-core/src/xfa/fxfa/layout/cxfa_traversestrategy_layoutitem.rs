// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Complete `cxfa_traversestrategy_layoutitem.h`, plus the two filtered
//! strategies in `cxfa_viewlayoutprocessor.cpp`.
use super::{cxfa_layoutitem::CxfaLayoutItem, cxfa_viewlayoutitem::CxfaViewLayoutItem};
use crate::xfa::fxfa::{
    fxfa_basic::XfaElement,
    parser::cxfa_nodeiteratortemplate::{CxfaNodeIteratorTemplate, TraverseStrategy},
};
pub struct CxfaTraverseStrategyLayoutItem;
impl TraverseStrategy<CxfaLayoutItem> for CxfaTraverseStrategyLayoutItem {
    fn get_first_child(node: &CxfaLayoutItem) -> Option<CxfaLayoutItem> {
        node.first_child()
    }
    fn get_next_sibling(node: &CxfaLayoutItem) -> Option<CxfaLayoutItem> {
        node.next_sibling()
    }
    fn get_parent(node: &CxfaLayoutItem) -> Option<CxfaLayoutItem> {
        node.parent()
    }
}
pub type CxfaLayoutItemIterator =
    CxfaNodeIteratorTemplate<CxfaLayoutItem, CxfaTraverseStrategyLayoutItem>;
pub struct TraverseStrategyViewLayoutItem;
fn next_view(mut node: Option<CxfaLayoutItem>, page_sets_only: bool) -> Option<CxfaViewLayoutItem> {
    while let Some(item) = node {
        if let Some(view) = item.as_view_layout_item() {
            if !page_sets_only || view.form_node().get_element_type() == XfaElement::PageSet {
                return Some(view);
            }
        }
        node = item.next_sibling();
    }
    None
}
impl TraverseStrategy<CxfaViewLayoutItem> for TraverseStrategyViewLayoutItem {
    fn get_first_child(node: &CxfaViewLayoutItem) -> Option<CxfaViewLayoutItem> {
        next_view(node.first_child(), false)
    }
    fn get_next_sibling(node: &CxfaViewLayoutItem) -> Option<CxfaViewLayoutItem> {
        next_view(node.next_sibling(), false)
    }
    fn get_parent(node: &CxfaViewLayoutItem) -> Option<CxfaViewLayoutItem> {
        node.parent()?.as_view_layout_item()
    }
}
pub type ViewLayoutItemIterator =
    CxfaNodeIteratorTemplate<CxfaViewLayoutItem, TraverseStrategyViewLayoutItem>;
pub struct TraverseStrategyPageSet;
impl TraverseStrategy<CxfaViewLayoutItem> for TraverseStrategyPageSet {
    fn get_first_child(node: &CxfaViewLayoutItem) -> Option<CxfaViewLayoutItem> {
        if node.form_node().get_element_type() == XfaElement::PageSet {
            next_view(node.first_child(), true)
        } else {
            None
        }
    }
    fn get_next_sibling(node: &CxfaViewLayoutItem) -> Option<CxfaViewLayoutItem> {
        next_view(node.next_sibling(), true)
    }
    fn get_parent(node: &CxfaViewLayoutItem) -> Option<CxfaViewLayoutItem> {
        node.parent()?.as_view_layout_item()
    }
}
pub type PageSetIterator = CxfaNodeIteratorTemplate<CxfaViewLayoutItem, TraverseStrategyPageSet>;
