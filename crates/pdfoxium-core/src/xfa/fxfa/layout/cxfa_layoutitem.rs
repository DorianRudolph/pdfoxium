// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_layoutitem.{h,cpp}`. The allocation arena owns items; every tree,
//! fragment and node-to-layout edge is weak. An item retains the parser arena,
//! preserving its form node without retaining the document or its processor.
use super::{
    cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutprocessor::CxfaLayoutProcessor,
    cxfa_viewlayoutitem::CxfaViewLayoutItem,
};
use crate::{
    fxcrt::{
        fx_coordinates::{CfxPointF, CfxSizeF},
        tree_node::{TreeNodeOwner, TreeNodeRef},
    },
    xfa::fxfa::{
        fxfa::XfaWidgetStatus,
        fxfa_basic::XfaElement,
        parser::{cxfa_document::CxfaNodeStore, cxfa_node::CxfaNode},
    },
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// Actual FF widget/page-view adapters can retain the supplied weak item handle.
pub trait LayoutItemWidget {
    fn as_ff_widget(&self) -> Option<&dyn crate::xfa::fxfa::cxfa_ffwidget::FfWidget> {
        None
    }
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>);
}
pub trait LayoutItemPageView {
    fn as_ff_page_view(&self) -> Option<&crate::xfa::fxfa::cxfa_ffpageview::CxfaFfPageView> {
        None
    }
    fn set_layout_item(&self, item: Option<CxfaViewLayoutItem>);
}
/// Notification edges are explicit: the model notification interface does not
/// impersonate FF layout callbacks. Calls occur with no item or arena borrowed.
pub trait LayoutItemNotify {
    fn on_layout_item_removing(&self, item: &CxfaLayoutItem);
    fn on_page_view_removed(&self, page: &CxfaViewLayoutItem);
    fn on_layout_item_added(
        &self,
        item: &CxfaContentLayoutItem,
        page_index: i32,
        status: XfaWidgetStatus,
    );
}

pub(crate) enum LayoutItemKind {
    Content {
        position: CfxPointF,
        size: CfxSizeF,
        status: XfaWidgetStatus,
        previous: Option<CxfaContentLayoutItem>,
        next: Option<CxfaContentLayoutItem>,
        widget: Option<Rc<dyn LayoutItemWidget>>,
    },
    View {
        old_subform: Option<CxfaNode>,
        old_store: Option<Rc<CxfaNodeStore>>,
        page_view: Option<Rc<dyn LayoutItemPageView>>,
    },
}
pub(crate) struct LayoutItemData {
    pub(crate) form_node: CxfaNode,
    node_store: Rc<CxfaNodeStore>,
    pub(crate) kind: LayoutItemKind,
}
pub struct CxfaLayoutStore {
    items: RefCell<Vec<TreeNodeOwner<LayoutItemData>>>,
    processor: RefCell<Weak<CxfaLayoutProcessor>>,
    notify: RefCell<Option<Rc<dyn LayoutItemNotify>>>,
}
impl Default for CxfaLayoutStore {
    fn default() -> Self {
        Self {
            items: RefCell::new(Vec::new()),
            processor: RefCell::new(Weak::new()),
            notify: RefCell::new(None),
        }
    }
}
impl CxfaLayoutStore {
    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }
    pub(crate) fn set_processor(&self, processor: Weak<CxfaLayoutProcessor>) {
        *self.processor.borrow_mut() = processor;
    }
    pub fn processor(&self) -> Option<Rc<CxfaLayoutProcessor>> {
        self.processor.borrow().upgrade()
    }
    pub fn set_notify(&self, notify: Option<Rc<dyn LayoutItemNotify>>) {
        *self.notify.borrow_mut() = notify;
    }
    pub fn notify(&self) -> Option<Rc<dyn LayoutItemNotify>> {
        self.notify.borrow().clone()
    }
    fn allocate(self: &Rc<Self>, node: &CxfaNode, kind: LayoutItemKind) -> CxfaLayoutItem {
        let node_store = node.retain_store().expect("layout form node has expired");
        let owner = TreeNodeOwner::new(LayoutItemData {
            form_node: node.clone(),
            node_store,
            kind,
        });
        let item = CxfaLayoutItem {
            node: owner.node_ref(),
            store: Rc::downgrade(self),
        };
        self.items.borrow_mut().push(owner);
        item
    }
    pub fn create_content(
        self: &Rc<Self>,
        node: &CxfaNode,
        widget: Option<Rc<dyn LayoutItemWidget>>,
    ) -> CxfaContentLayoutItem {
        let item = CxfaContentLayoutItem(self.allocate(
            node,
            LayoutItemKind::Content {
                position: CfxPointF::default(),
                size: CfxSizeF::default(),
                status: XfaWidgetStatus::default(),
                previous: None,
                next: None,
                widget: widget.clone(),
            },
        ));
        if let Some(widget) = widget {
            widget.set_layout_item(Some(item.clone()));
        }
        item
    }
    pub fn create_view(
        self: &Rc<Self>,
        node: &CxfaNode,
        page_view: Option<Rc<dyn LayoutItemPageView>>,
    ) -> CxfaViewLayoutItem {
        let item = CxfaViewLayoutItem(self.allocate(
            node,
            LayoutItemKind::View {
                old_subform: None,
                old_store: None,
                page_view: page_view.clone(),
            },
        ));
        if let Some(page_view) = page_view {
            page_view.set_layout_item(Some(item.clone()));
        }
        item
    }
}
impl Drop for CxfaLayoutStore {
    fn drop(&mut self) {
        // Source PreFinalize clears only an exact matching node back-reference.
        // Keep every allocation live until all back-references have been checked.
        for owner in self.items.get_mut().iter() {
            let item = owner.node_ref();
            let node = item.with_value(|data| data.form_node.clone());
            if node
                .get_layout_item()
                .is_some_and(|attached| attached.node == item)
            {
                node.set_layout_item(None);
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct CxfaLayoutItem {
    pub(crate) node: TreeNodeRef<LayoutItemData>,
    store: Weak<CxfaLayoutStore>,
}
impl PartialEq for CxfaLayoutItem {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node
    }
}
impl Eq for CxfaLayoutItem {}
impl std::hash::Hash for CxfaLayoutItem {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.node.hash(state);
    }
}
impl CxfaLayoutItem {
    pub fn is_alive(&self) -> bool {
        self.node.is_alive()
    }
    pub fn store(&self) -> Option<Rc<CxfaLayoutStore>> {
        self.store.upgrade()
    }
    fn wrap(&self, node: TreeNodeRef<LayoutItemData>) -> Self {
        Self {
            node,
            store: self.store.clone(),
        }
    }
    pub fn form_node(&self) -> CxfaNode {
        self.node.with_value(|data| data.form_node.clone())
    }
    pub fn set_form_node(&self, node: &CxfaNode) {
        let store = node.retain_store().expect("layout form node has expired");
        self.node.with_value_mut(|data| {
            data.form_node = node.clone();
            data.node_store = store;
        });
    }
    pub fn is_content_layout_item(&self) -> bool {
        self.node
            .with_value(|data| matches!(data.kind, LayoutItemKind::Content { .. }))
    }
    pub fn is_view_layout_item(&self) -> bool {
        !self.is_content_layout_item()
    }
    pub fn as_content_layout_item(&self) -> Option<CxfaContentLayoutItem> {
        self.is_content_layout_item()
            .then(|| CxfaContentLayoutItem(self.clone()))
    }
    pub fn as_view_layout_item(&self) -> Option<CxfaViewLayoutItem> {
        self.is_view_layout_item()
            .then(|| CxfaViewLayoutItem(self.clone()))
    }
    pub fn parent(&self) -> Option<Self> {
        self.node.parent().map(|node| self.wrap(node))
    }
    pub fn first_child(&self) -> Option<Self> {
        self.node.first_child().map(|node| self.wrap(node))
    }
    pub fn last_child(&self) -> Option<Self> {
        self.node.last_child().map(|node| self.wrap(node))
    }
    pub fn next_sibling(&self) -> Option<Self> {
        self.node.next_sibling().map(|node| self.wrap(node))
    }
    pub fn prev_sibling(&self) -> Option<Self> {
        self.node.prev_sibling().map(|node| self.wrap(node))
    }
    pub fn children(&self) -> Vec<Self> {
        let mut result = Vec::new();
        let mut current = self.first_child();
        while let Some(item) = current {
            current = item.next_sibling();
            result.push(item);
        }
        result
    }
    fn check_child(&self, child: &Self) {
        assert!(
            Weak::ptr_eq(&self.store, &child.store),
            "layout items belong to different arenas"
        );
        let mut ancestor = Some(self.clone());
        while let Some(item) = ancestor {
            assert_ne!(&item, child, "layout tree cycle");
            ancestor = item.parent();
        }
    }
    pub fn append_first_child(&self, child: &Self) {
        self.check_child(child);
        self.node.append_first_child(&child.node);
    }
    pub fn append_last_child(&self, child: &Self) {
        self.check_child(child);
        self.node.append_last_child(&child.node);
    }
    pub fn insert_before(&self, child: &Self, other: Option<&Self>) {
        self.check_child(child);
        self.node
            .insert_before(&child.node, other.map(|item| &item.node));
    }
    pub fn insert_after(&self, child: &Self, other: Option<&Self>) {
        self.check_child(child);
        self.node
            .insert_after(&child.node, other.map(|item| &item.node));
    }
    pub fn remove_child(&self, child: &Self) {
        self.node.remove_child(&child.node);
    }
    pub fn remove_self_if_parented(&self) {
        self.node.remove_self_if_parented();
    }
    pub fn get_page(&self) -> Option<CxfaViewLayoutItem> {
        let mut current = Some(self.clone());
        while let Some(item) = current {
            if item.form_node().get_element_type() == XfaElement::PageArea {
                return item.as_view_layout_item();
            }
            current = item.parent();
        }
        None
    }
    /// Permanently retire an item, rather than detach it for layout reuse.
    /// The arena keeps retired items alive, so their fragment links must be
    /// removed explicitly before the next layout or widget scroll calculation.
    pub(crate) fn retire_from_layout(&self) {
        if let Some(content) = self.as_content_layout_item() {
            let node = self.form_node();
            if node.get_layout_item().as_ref() == Some(self) {
                node.set_layout_item(content.next().map(|next| (*next).clone()));
            }
            content.remove_from_fragment_chain();
        } else {
            self.pre_finalize();
        }
        self.remove_self_if_parented();
    }
    pub fn pre_finalize(&self) {
        let node = self.form_node();
        if node.get_layout_item().as_ref() == Some(self) {
            node.set_layout_item(None);
        }
    }
}
/// Native postorder removal, preserving callback ordering and taking the next
/// sibling before calling out. The arena, not a detached tree edge, owns items.
pub fn release_layout_item(item: &CxfaLayoutItem) {
    let _store = item.store();
    let mut child = item.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        release_layout_item(&current);
    }
    if let Some(notify) = item.store().and_then(|store| store.notify()) {
        notify.on_layout_item_removing(item);
        if item.form_node().get_element_type() == XfaElement::PageArea {
            if let Some(page) = item.as_view_layout_item() {
                notify.on_page_view_removed(&page);
            }
        }
    }
    item.retire_from_layout();
}
