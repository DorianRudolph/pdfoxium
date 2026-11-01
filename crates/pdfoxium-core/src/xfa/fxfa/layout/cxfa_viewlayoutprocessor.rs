// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! View records, page reuse, content-area switching and widget synchronization
//! from `cxfa_viewlayoutprocessor.{h,cpp}`. Pagination and generated-node
//! merging are in the private pages module and call actual runtime/layout hosts.
use super::{
    cxfa_contentlayoutitem::CxfaContentLayoutItem,
    cxfa_contentlayoutprocessor::XFA_LAYOUT_PRECISION,
    cxfa_layoutitem::{release_layout_item, CxfaLayoutItem, CxfaLayoutStore, LayoutItemPageView},
    cxfa_traversestrategy_layoutitem::{CxfaLayoutItemIterator, ViewLayoutItemIterator},
    cxfa_viewlayoutitem::CxfaViewLayoutItem,
};
use crate::xfa::fxfa::{
    fxfa::XfaWidgetStatus as S,
    fxfa_basic::{XfaAttribute as A, XfaAttributeValue as V, XfaElement as E, XfaUnit},
    parser::cxfa_node::CxfaNode,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub trait ViewLayoutHost {
    fn create_page_view(&self, node: &CxfaNode) -> Option<Rc<dyn LayoutItemPageView>>;
    fn runtime(&self) -> ViewLayoutResult<Rc<crate::fxjs::xfa::cfxjse_engine::CfxJseEngine>>;
    fn run_break_script(&self, script: &CxfaNode, container: &CxfaNode) -> ViewLayoutResult<bool>;
    fn layout_page_area(&self, page: &CxfaViewLayoutItem) -> ViewLayoutResult<()>;
}
#[derive(Clone, Debug, Default)]
pub struct CxfaViewRecord {
    pub page_set: Option<CxfaViewLayoutItem>,
    pub page_area: Option<CxfaViewLayoutItem>,
    pub content_area: Option<CxfaViewLayoutItem>,
}
struct ViewState {
    page_set_node: Option<CxfaNode>,
    current_page_area: Option<CxfaNode>,
    root: Option<CxfaViewLayoutItem>,
    roots: Vec<CxfaViewLayoutItem>,
    current_page_set: Option<CxfaViewLayoutItem>,
    page_set_mode: V,
    records: Vec<CxfaViewRecord>,
    current_record: Option<usize>,
    pages: Vec<CxfaViewLayoutItem>,
    available_pages: usize,
    current_page_count: i32,
    create_overflow_page: bool,
    page_set_map: HashMap<CxfaNode, i32>,
}
impl Default for ViewState {
    fn default() -> Self {
        Self {
            page_set_node: None,
            current_page_area: None,
            root: None,
            roots: Vec::new(),
            current_page_set: None,
            page_set_mode: V::OrderedOccurrence,
            records: Vec::new(),
            current_record: None,
            pages: Vec::new(),
            available_pages: 0,
            current_page_count: 0,
            create_overflow_page: false,
            page_set_map: HashMap::new(),
        }
    }
}
pub struct CxfaViewLayoutProcessor {
    store: Rc<CxfaLayoutStore>,
    host: RefCell<Option<Rc<dyn ViewLayoutHost>>>,
    state: RefCell<ViewState>,
}
impl CxfaViewLayoutProcessor {
    pub fn new(store: Rc<CxfaLayoutStore>) -> Self {
        Self {
            store,
            host: RefCell::new(None),
            state: RefCell::new(ViewState::default()),
        }
    }
    pub fn set_host(&self, host: Option<Rc<dyn ViewLayoutHost>>) {
        *self.host.borrow_mut() = host;
    }
    pub fn get_page_count(&self) -> i32 {
        self.state
            .borrow()
            .pages
            .len()
            .try_into()
            .unwrap_or(i32::MAX)
    }
    pub fn get_page(&self, index: i32) -> Option<CxfaViewLayoutItem> {
        self.state
            .borrow()
            .pages
            .get(usize::try_from(index).ok()?)
            .cloned()
    }
    pub fn get_page_index(&self, page: &CxfaViewLayoutItem) -> i32 {
        self.state
            .borrow()
            .pages
            .iter()
            .position(|item| item == page)
            .and_then(|index| index.try_into().ok())
            .unwrap_or(-1)
    }
    pub fn get_root_layout_item(&self) -> Option<CxfaViewLayoutItem> {
        self.state.borrow().root.clone()
    }
    pub fn current_view_record(&self) -> Option<CxfaViewRecord> {
        let state = self.state.borrow();
        state.records.get(state.current_record?).cloned()
    }
    pub fn reset_to_first_view_record(&self) {
        let mut state = self.state.borrow_mut();
        state.current_record = (!state.records.is_empty()).then_some(0);
    }
    pub fn set_current_view_record(&self, index: usize) -> bool {
        let mut state = self.state.borrow_mut();
        if index >= state.records.len() {
            return false;
        }
        state.current_record = Some(index);
        true
    }
    pub fn record_count(&self) -> usize {
        self.state.borrow().records.len()
    }
    /// Initial PageSet graph setup from InitLayoutPage. The caller supplies the
    /// real template PageSet; this does not claim to merge/recycle a form DOM.
    pub fn initialize_page_set(&self, node: &CxfaNode) -> bool {
        if node.get_element_type() != E::PageSet {
            return false;
        }
        self.clear_data();
        let item = self.store.create_view(node, None);
        node.set_layout_item(Some((*item).clone()));
        let relation = node.get_enum(A::Relation);
        {
            let mut state = self.state.borrow_mut();
            state.page_set_node = Some(node.clone());
            state.root = Some(item.clone());
            state.roots = vec![item.clone()];
            state.current_page_set = Some(item);
            state.available_pages = 0;
            state.page_set_mode = if relation == V::Unknown {
                V::OrderedOccurrence
            } else {
                relation
            };
        }
        self.init_page_set_map();
        true
    }
    fn ordered(&self) -> bool {
        self.state.borrow().page_set_mode == V::OrderedOccurrence
    }
    fn append_record(&self, record: CxfaViewRecord) -> usize {
        let mut state = self.state.borrow_mut();
        let index = state.records.len();
        state.records.push(record);
        index
    }
    pub fn create_view_record_simple(&self) -> usize {
        let record = self
            .current_view_record()
            .unwrap_or_else(|| CxfaViewRecord {
                page_set: self.get_root_layout_item(),
                ..Default::default()
            });
        self.append_record(record)
    }
    pub fn create_view_record(&self, page: &CxfaNode, create_new: bool) -> Option<usize> {
        let page_set = page.parent()?;
        let current = self.current_view_record();
        let root = self.get_root_layout_item()?;
        if current.is_none() {
            let item = if Some(&page_set) == self.state.borrow().page_set_node.as_ref() {
                root
            } else {
                let item = self.store.create_view(&page_set, None);
                page_set.set_layout_item(Some((*item).clone()));
                root.append_last_child(&item);
                item
            };
            return Some(self.append_record(CxfaViewRecord {
                page_set: Some(item),
                ..Default::default()
            }));
        }
        if !self.ordered() {
            return Some(self.append_record(current.unwrap()));
        }
        if !create_new {
            let item = if Some(&page_set) == self.state.borrow().page_set_node.as_ref() {
                self.state.borrow().current_page_set.clone()
            } else {
                page_set
                    .get_layout_item()
                    .and_then(|item| item.as_view_layout_item())
                    .or_else(|| self.state.borrow().current_page_set.clone())
            };
            return Some(self.append_record(CxfaViewRecord {
                page_set: item,
                ..Default::default()
            }));
        }
        let current = current.unwrap();
        let parent = if current
            .page_set
            .as_ref()
            .is_some_and(|item| item.form_node() == page_set)
        {
            current
                .page_set?
                .parent()
                .and_then(|item| item.as_view_layout_item())
        } else {
            page_set
                .parent()
                .and_then(|node| node.get_layout_item())
                .and_then(|item| item.as_view_layout_item())
        };
        let item = self.store.create_view(&page_set, None);
        page_set.set_layout_item(Some((*item).clone()));
        if let Some(parent) = parent {
            parent.append_last_child(&item);
        } else {
            let mut state = self.state.borrow_mut();
            if state.roots.is_empty() {
                state.roots.push(root);
            }
            state.roots.push(item.clone());
            state.current_page_set = Some(item.clone());
        }
        Some(self.append_record(CxfaViewRecord {
            page_set: Some(item),
            ..Default::default()
        }))
    }
    pub fn add_page_area_layout_item(
        &self,
        record: usize,
        node: &CxfaNode,
    ) -> Option<CxfaViewLayoutItem> {
        let page_set = self.state.borrow().records.get(record)?.page_set.clone()?;
        let existing = {
            let state = self.state.borrow();
            state.pages.get(state.available_pages).cloned()
        };
        let page = if let Some(page) = existing {
            page.set_form_node(node);
            page
        } else {
            let host = self.host.borrow().clone();
            let page_view = host.and_then(|host| host.create_page_view(node));
            let item = self.store.create_view(node, page_view);
            self.state.borrow_mut().pages.push(item.clone());
            if let Some(notify) = self.store.notify() {
                notify.on_page_view_removed(&item);
            }
            item
        };
        {
            self.state.borrow_mut().available_pages += 1;
        }
        page_set.append_last_child(&page);
        let mut state = self.state.borrow_mut();
        let entry = state.records.get_mut(record)?;
        entry.page_area = Some(page.clone());
        entry.content_area = None;
        Some(page)
    }
    pub fn add_content_area_layout_item(
        &self,
        record: usize,
        node: Option<&CxfaNode>,
    ) -> Option<CxfaViewLayoutItem> {
        let area = if let Some(node) = node {
            let page = self.state.borrow().records.get(record)?.page_area.clone()?;
            let item = self.store.create_view(node, None);
            page.append_last_child(&item);
            Some(item)
        } else {
            None
        };
        self.state
            .borrow_mut()
            .records
            .get_mut(record)?
            .content_area = area.clone();
        area
    }
    pub fn remove_layout_record(new: &CxfaViewRecord, previous: &CxfaViewRecord) {
        let item = if new.page_set != previous.page_set {
            new.page_set.as_ref()
        } else if new.page_area != previous.page_area {
            new.page_area.as_ref()
        } else if new.content_area != previous.content_area {
            new.content_area.as_ref()
        } else {
            None
        };
        if let Some(item) = item {
            item.remove_self_if_parented();
        }
    }
    pub fn get_avail_height(&self) -> f32 {
        let Some(record) = self.current_view_record() else {
            return 0.;
        };
        let Some(area) = record.content_area else {
            return 0.;
        };
        let height = area.form_node().get_measure_in_unit(A::H, XfaUnit::Pt);
        if height >= XFA_LAYOUT_PRECISION {
            return height;
        }
        if self.state.borrow().current_record == Some(0) {
            0.
        } else {
            f32::MAX
        }
    }
    /// Content submission for a completed fragment. Page-full submission must
    /// first supply the next real view record; automatic page search is pending.
    pub fn submit_content_to_current(&self, item: Option<&CxfaContentLayoutItem>) -> bool {
        if let Some(item) = item {
            let Some(record) = self.current_view_record() else {
                return true;
            };
            let Some(area) = record.content_area else {
                return false;
            };
            area.append_last_child(item);
            self.state.borrow_mut().create_overflow_page = false;
        }
        true
    }
    pub fn get_next_content_area(&self, target: Option<&CxfaNode>) -> bool {
        let Some(current) = self.current_view_record() else {
            return false;
        };
        let Some(area) = current.content_area else {
            return false;
        };
        let node = area.form_node();
        let target = if let Some(target) = target {
            if Some(target.parent())
                != current
                    .page_area
                    .as_ref()
                    .map(|page| Some(page.form_node()))
            {
                return false;
            }
            let Some(page) = current.page_area.as_ref() else {
                return false;
            };
            for child in page.children() {
                if child.form_node() == *target {
                    if child.first_child().is_some() {
                        return false;
                    }
                    if *target == node {
                        return false;
                    }
                    let Some(view) = child.as_view_layout_item() else {
                        continue;
                    };
                    let index = self.create_view_record_simple();
                    self.state.borrow_mut().records[index].content_area = Some(view);
                    return true;
                }
            }
            target.clone()
        } else {
            let Some(next) = node.get_next_same_class_sibling(E::ContentArea) else {
                return false;
            };
            next
        };
        let record = self.create_view_record_simple();
        self.add_content_area_layout_item(record, Some(&target));
        true
    }
    pub fn match_page_area_odd_or_even(&self, page: &CxfaNode) -> bool {
        if self.state.borrow().page_set_mode != V::DuplexPaginated {
            return true;
        }
        let value = page.try_enum(A::OddOrEven, true);
        if value.is_none() || value == Some(V::Any) {
            return true;
        }
        // Existing pages are retained across relayout. Their cache length is
        // not the ordinal of the next page being selected in this pass.
        // Native GetPageCount() incorrectly uses that retained length here.
        let parity = self.state.borrow().available_pages % 2;
        if value == Some(V::Odd) {
            parity == 0
        } else {
            parity == 1
        }
    }
    pub fn should_get_next_page_area(&self, target: Option<&CxfaNode>, start_new: bool) -> bool {
        start_new
            || target.is_none()
            || self
                .current_view_record()
                .and_then(|record| record.page_area)
                .is_none_or(|page| Some(&page.form_node()) != target)
    }
    fn init_page_set_map(&self) {
        if !self.ordered() {
            return;
        }
        let Some(root) = self.state.borrow().page_set_node.clone() else {
            return;
        };
        let mut stack = vec![root];
        let mut entries = Vec::new();
        while let Some(node) = stack.pop() {
            if node.get_element_type() == E::PageSet
                && node.get_enum(A::Relation) == V::OrderedOccurrence
            {
                entries.push(node.clone());
            }
            let mut child = node.first_child();
            while let Some(node) = child {
                child = node.next_sibling();
                stack.push(node);
            }
        }
        let mut state = self.state.borrow_mut();
        for node in entries {
            state.page_set_map.insert(node, 0);
        }
    }
    pub fn clear_data(&self) {
        let mut state = self.state.borrow_mut();
        if state.page_set_node.is_none() {
            return;
        }
        state.records.clear();
        state.current_record = None;
        state.current_page_area = None;
        state.current_page_count = 0;
        state.create_overflow_page = false;
        state.page_set_map.clear();
    }
    pub fn pre_finalize(&self) {
        self.clear_data();
        for root in self.root_layout_items() {
            release_layout_item(&root);
        }
        let mut state = self.state.borrow_mut();
        state.root = None;
        state.roots.clear();
        state.pages.clear();
    }
    /// Final notification phase of SyncLayoutData, after the actual caller has
    /// merged PageSet nodes and laid out master-page content.
    pub fn sync_layout_item_status(&self) {
        let mut page_index = -1;
        for item in self.root_layout_items() {
            let mut views = ViewLayoutItemIterator::new(Some(item));
            let mut view = views.get_current();
            while let Some(page) = view {
                view = views.move_to_next();
                if page.form_node().get_element_type() != E::PageArea {
                    continue;
                }
                page_index += 1;
                let mut items = CxfaLayoutItemIterator::new(Some((*page).clone()));
                let mut child = items.get_current();
                while let Some(item) = child {
                    if let Some(content) = item.as_content_layout_item() {
                        let visible = content
                            .form_node()
                            .try_enum(A::Presence, true)
                            .unwrap_or(V::Visible)
                            == V::Visible;
                        let relevant =
                            get_relevant(&content.form_node(), S::kViewable | S::kPrintable);
                        self.sync_container(&content, relevant, visible, page_index);
                        child = items.skip_children_and_move_to_next();
                    } else {
                        child = items.move_to_next();
                    }
                }
            }
        }
        let removed = {
            let mut state = self.state.borrow_mut();
            let available = state.available_pages;
            state.pages.split_off(available)
        };
        if let Some(notify) = self.store.notify() {
            for page in removed.iter().rev() {
                notify.on_page_view_removed(page);
            }
        }
        self.clear_data();
    }
    fn sync_container(
        &self,
        item: &CxfaContentLayoutItem,
        parent_relevant: S,
        visible: bool,
        page: i32,
    ) {
        let (mut own_visible, mut status, mut relevant) = (false, S::default(), S::default());
        if visible {
            own_visible = item
                .form_node()
                .try_enum(A::Presence, true)
                .unwrap_or(V::Visible)
                == V::Visible;
            relevant = get_relevant(&item.form_node(), parent_relevant);
            status = relevant;
            if own_visible {
                status |= S::kVisible;
            }
        }
        if let Some(notify) = self.store.notify() {
            notify.on_layout_item_added(item, page, status);
        }
        for child in item.children() {
            if let Some(content) = child.as_content_layout_item() {
                self.sync_container(&content, relevant, own_visible, page);
            }
        }
    }
}
pub fn get_relevant(node: &CxfaNode, parent: S) -> S {
    let mut relevant = S::kViewable | S::kPrintable;
    let text = node.get_cdata(A::Relevant);
    if text.iter().copied().eq("+print".chars().map(u32::from))
        || text.iter().copied().eq("print".chars().map(u32::from))
    {
        relevant &= !S::kViewable;
    } else if text.iter().copied().eq("-print".chars().map(u32::from)) {
        relevant &= !S::kPrintable;
    }
    if !parent.intersects(S::kViewable) && relevant != S::kViewable {
        relevant &= !S::kViewable;
    }
    if !parent.intersects(S::kPrintable) && relevant != S::kPrintable {
        relevant &= !S::kPrintable;
    }
    relevant
}
pub fn query_overflow(node: &CxfaNode) -> Option<CxfaNode> {
    let mut child = node.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        match node.get_element_type() {
            E::Break => {
                return [A::OverflowLeader, A::OverflowTarget, A::OverflowTrailer]
                    .into_iter()
                    .any(|attr| !node.get_cdata(attr).is_empty())
                    .then_some(node)
            }
            E::Overflow => return Some(node),
            _ => {}
        }
    }
    None
}
pub fn calculate_layout_item_height(item: &CxfaLayoutItem) -> f32 {
    item.children()
        .into_iter()
        .filter_map(|item| item.as_content_layout_item())
        .map(|item| item.size().height)
        .sum()
}
pub fn get_heights_for_content_areas(item: &CxfaLayoutItem) -> Vec<f32> {
    item.children()
        .into_iter()
        .filter(|item| item.form_node().get_element_type() == E::ContentArea)
        .map(|item| calculate_layout_item_height(&item))
        .collect()
}
pub fn get_page_area_count_and_last_page_area_from_page_set(
    item: &CxfaViewLayoutItem,
) -> (usize, Option<CxfaLayoutItem>) {
    let (mut count, mut last) = (0, None);
    for child in item.children() {
        if child.form_node().get_element_type() == E::PageArea {
            count += 1;
            last = Some(child);
        }
    }
    (count, last)
}
pub fn content_areas_fit_in_page_areas(node: &CxfaNode, used: &[f32]) -> bool {
    let mut index = 0;
    let mut child = node.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        if node.get_element_type() != E::ContentArea {
            continue;
        }
        let Some(height) = used.get(index) else {
            return false;
        };
        if *height > node.get_measure_in_unit(A::H, XfaUnit::Pt) + XFA_LAYOUT_PRECISION {
            return false;
        }
        index += 1;
    }
    true
}

#[path = "cxfa_viewlayoutprocessor_pages.rs"]
mod pages;
pub use pages::{BreakData, OverflowData, ViewLayoutError, ViewLayoutResult};
