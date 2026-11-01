// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Remaining pagination, generated-node merge and synchronization from
//! `cxfa_viewlayoutprocessor.cpp`. State borrows never span model/host calls.
use super::super::cxfa_contentlayoutprocessor::{LayoutError, LayoutResult};
use super::*;
use crate::{
    fxjs::xfa::{cfxjse_engine::CfxJseEngine, cfxjse_resolveprocessor::XfaResolveFlags},
    xfa::fxfa::{
        fxfa_basic::{XfaHashCode, XfaPacketType},
        parser::{
            cxfa_document::CxfaDocument,
            cxfa_document_datamerge::DataMergeError,
            cxfa_measurement::CxfaMeasurement,
            cxfa_node::XfaNodeFlag,
            cxfa_traversestrategy_xfacontainernode::CxfaContainerIterator,
            cxfa_traversestrategy_xfanode::CxfaNodeIterator,
            xfa_document_datamerger_imp::{
                data_merge_find_data_scope, data_merge_find_form_dom_instance,
                node_merge_clone_or_merge_container,
            },
        },
    },
};
#[derive(Debug)]
pub enum ViewLayoutError {
    MissingHost,
    ExpiredDocument,
    InvalidTree(&'static str),
    Script(boa_engine::JsError),
    Resolve(crate::fxjs::xfa::cfxjse_resolveprocessor::ResolveError),
    Merge(DataMergeError),
    Content(LayoutError),
}
impl From<boa_engine::JsError> for ViewLayoutError {
    fn from(value: boa_engine::JsError) -> Self {
        Self::Script(value)
    }
}
impl From<DataMergeError> for ViewLayoutError {
    fn from(value: DataMergeError) -> Self {
        Self::Merge(value)
    }
}
impl From<LayoutError> for ViewLayoutError {
    fn from(value: LayoutError) -> Self {
        Self::Content(value)
    }
}
pub type ViewLayoutResult<T> = Result<T, ViewLayoutError>;
#[derive(Clone, Debug, Default)]
pub struct BreakData {
    pub leader: Option<CxfaNode>,
    pub trailer: Option<CxfaNode>,
    pub create_page: bool,
}
#[derive(Clone, Debug, Default)]
pub struct OverflowData {
    pub leader: Option<CxfaNode>,
    pub trailer: Option<CxfaNode>,
}
fn children(node: &CxfaNode) -> impl Iterator<Item = CxfaNode> {
    let mut child = node.first_child();
    std::iter::from_fn(move || {
        let current = child.take()?;
        child = current.next_sibling();
        Some(current)
    })
}
fn occurrence(node: &CxfaNode, attribute: A) -> Option<i32> {
    node.get_first_child_by_class(E::Occur)?
        .try_integer(attribute, false)
}
fn generated(node: &CxfaNode) {
    node.set_flag(XfaNodeFlag::LayoutGeneratedNode);
    node.clear_flag(XfaNodeFlag::UnusedNode);
}
impl CxfaViewLayoutProcessor {
    /// Ordered root repetitions are owned explicitly because arena tree handles
    /// cannot attach an unparented root to a sibling without a parent owner.
    pub(super) fn root_layout_items(&self) -> Vec<CxfaViewLayoutItem> {
        let state = self.state.borrow();
        if state.roots.is_empty() {
            state.root.clone().into_iter().collect()
        } else {
            state.roots.clone()
        }
    }
    fn services(&self) -> ViewLayoutResult<Rc<dyn ViewLayoutHost>> {
        self.host
            .borrow()
            .clone()
            .ok_or(ViewLayoutError::MissingHost)
    }
    fn runtime(&self) -> ViewLayoutResult<Rc<CfxJseEngine>> {
        self.services()?.runtime()
    }
    fn document(&self) -> ViewLayoutResult<Rc<CxfaDocument>> {
        self.state
            .borrow()
            .page_set_node
            .clone()
            .and_then(|node| node.document())
            .ok_or(ViewLayoutError::ExpiredDocument)
    }
    fn select_record(&self, index: Option<usize>) {
        self.state.borrow_mut().current_record = index;
    }
    fn tail(&self) -> Option<usize> {
        self.state.borrow().records.len().checked_sub(1)
    }
    pub fn init_layout_page(&self, form: &CxfaNode) -> bool {
        self.prepare_layout();
        let Some(template) = form.get_template_node_if_exists() else {
            return false;
        };
        let Some(page_set) = template.get_or_create_property(0, E::PageSet) else {
            return false;
        };
        let root = if let Some(root) = self.get_root_layout_item() {
            root.remove_self_if_parented();
            root
        } else {
            self.store.create_view(&page_set, None)
        };
        page_set.set_layout_item(Some((*root).clone()));
        let relation = page_set.get_enum(A::Relation);
        {
            let mut state = self.state.borrow_mut();
            state.page_set_node = Some(page_set.clone());
            state.root = Some(root.clone());
            state.roots = vec![root.clone()];
            state.current_page_set = Some(root);
            if relation != V::Unknown {
                state.page_set_mode = relation;
            }
        }
        self.init_page_set_map();
        let mut count = 0;
        for page in children(&page_set).filter(|node| node.get_element_type() == E::PageArea) {
            count += 1;
            if page.get_first_child_by_class(E::ContentArea).is_some() {
                return true;
            }
        }
        if count > 0 {
            return false;
        }
        let Some(document) = template.document() else {
            return false;
        };
        let page = if let Some(page) = page_set.get_child(0, E::PageArea, false) {
            page
        } else {
            let Some(page) = document.create_node(page_set.get_packet_type(), E::PageArea) else {
                return false;
            };
            if !page_set.insert_child_and_notify(-1, &page) {
                return false;
            }
            page.set_initialized_flag_and_notify();
            page
        };
        if page.get_child(0, E::ContentArea, false).is_none() {
            let Some(content) = document.create_node(page.get_packet_type(), E::ContentArea) else {
                return false;
            };
            if !page.insert_child_and_notify(-1, &content) {
                return false;
            }
            content.set_initialized_flag_and_notify();
            for (attribute, value) in [(A::X, 0.25), (A::Y, 0.25), (A::W, 8.0), (A::H, 10.5)] {
                content.set_measure(attribute, CxfaMeasurement::new(value, XfaUnit::In), false);
            }
        }
        if page.get_child(0, E::Medium, false).is_none() {
            let Some(medium) = document.create_node(page.get_packet_type(), E::Medium) else {
                return false;
            };
            if !page.insert_child_and_notify(-1, &medium) {
                return false;
            }
            medium.set_initialized_flag_and_notify();
            medium.set_measure(A::Short, CxfaMeasurement::new(8.5, XfaUnit::In), false);
            medium.set_measure(A::Long, CxfaMeasurement::new(11.0, XfaUnit::In), false);
        }
        true
    }
    pub fn prepare_first_page(&self, root: &CxfaNode) -> ViewLayoutResult<bool> {
        let mut root = Some(root.clone());
        let mut break_node = None;
        while let Some(node) = root {
            break_node = children(&node).find(|child| {
                child.get_element_type() == E::BreakBefore
                    || (child.get_element_type() == E::Break
                        && child.get_enum(A::Before) != V::Auto)
            });
            if break_node.is_some() {
                break;
            }
            root = node.get_first_child_by_class(E::Subform);
            while root
                .as_ref()
                .is_some_and(|node| !node.presence_requires_space())
            {
                root = root.and_then(|node| node.get_next_same_class_sibling(E::Subform));
            }
        }
        if let Some(node) = break_node {
            if self.execute_break_before_or_after(&node, true)?.create_page {
                self.reset_to_first_view_record();
                return Ok(true);
            }
        }
        Ok(self.append_new_page(true))
    }
    fn append_new_page(&self, first: bool) -> bool {
        if self.state.borrow().current_record != self.tail() {
            return true;
        }
        if self
            .get_next_avail_page_area(None, None, false, false)
            .is_none()
        {
            return false;
        }
        if first && self.current_view_record().is_none() {
            self.reset_to_first_view_record();
        }
        !first || self.current_view_record().is_some()
    }
    pub fn submit_content_item(
        &self,
        item: Option<&CxfaContentLayoutItem>,
        status: LayoutResult,
    ) -> bool {
        if !self.submit_content_to_current(item) {
            return false;
        }
        if status != LayoutResult::Done {
            if status == LayoutResult::PageFullBreak
                && self.state.borrow().current_record == self.tail()
            {
                self.append_new_page(false);
            }
            self.select_record(self.tail());
            let Some(page) = self
                .current_view_record()
                .and_then(|record| record.page_area)
            else {
                return false;
            };
            self.state.borrow_mut().current_page_area = Some(page.form_node());
        }
        true
    }
    fn find_page_area(
        &self,
        page_set: Option<&CxfaNode>,
        start: Option<&CxfaNode>,
        target: Option<&CxfaNode>,
        content: Option<&CxfaNode>,
        new_page: bool,
        query: bool,
    ) -> bool {
        if page_set.is_none() && start.is_none() {
            return false;
        }
        if self.ordered() {
            self.find_ordered(page_set, start, target, content, new_page, query)
        } else {
            let preferred = if self.current_view_record().is_some() {
                V::Rest
            } else {
                V::First
            };
            self.find_simplex_duplex(page_set, start, target, content, new_page, query, preferred)
        }
    }
    fn find_ordered(
        &self,
        page_set: Option<&CxfaNode>,
        start: Option<&CxfaNode>,
        mut target: Option<&CxfaNode>,
        content: Option<&CxfaNode>,
        new_page: bool,
        query: bool,
    ) -> bool {
        let mut set_count = 0;
        if start.is_none() && !query {
            let Some(page_set) = page_set else {
                return false;
            };
            set_count = self
                .state
                .borrow()
                .page_set_map
                .get(page_set)
                .copied()
                .unwrap_or(0);
            let max = occurrence(page_set, A::Max).unwrap_or(-1);
            if max >= 0 && max <= set_count {
                return false;
            }
        }
        let mut current = if let Some(start) = start {
            start.next_sibling()
        } else {
            page_set.and_then(CxfaNode::first_child)
        };
        let mut found = false;
        while let Some(node) = current {
            current = node.next_sibling();
            match node.get_element_type() {
                E::PageArea => {
                    if target.is_none_or(|target| *target == node) {
                        let first = node.get_first_child_by_class(E::ContentArea);
                        if first.is_none() {
                            if target == Some(&node) {
                                self.create_min_page_record(Some(&node), true, false);
                                target = None;
                            }
                            continue;
                        }
                        if !query {
                            let Some(record) = self.create_view_record(&node, start.is_none())
                            else {
                                return false;
                            };
                            if self.add_page_area_layout_item(record, &node).is_none() {
                                return false;
                            }
                            self.add_content_area_layout_item(record, content.or(first.as_ref()));
                        }
                        let mut state = self.state.borrow_mut();
                        state.current_page_area = Some(node);
                        state.current_page_count = 1;
                        found = true;
                        break;
                    }
                    if !query {
                        self.create_min_page_record(Some(&node), false, false);
                    }
                }
                E::PageSet => {
                    if self.find_ordered(Some(&node), None, target, content, new_page, query) {
                        found = true;
                        break;
                    }
                    if !query {
                        self.create_min_page_set_record(&node, true);
                    }
                }
                _ => {}
            }
        }
        if start.is_none() && found && !query {
            if let Some(page_set) = page_set {
                self.state
                    .borrow_mut()
                    .page_set_map
                    .insert(page_set.clone(), set_count.saturating_add(1));
            }
        }
        found
    }
    fn find_simplex_duplex(
        &self,
        page_set: Option<&CxfaNode>,
        start: Option<&CxfaNode>,
        mut target: Option<&CxfaNode>,
        content: Option<&CxfaNode>,
        new_page: bool,
        query: bool,
        preferred: V,
    ) -> bool {
        let mut fallback = None;
        let mut selected = None;
        let mut current = if start.is_none_or(|start| start.get_element_type() == E::PageArea) {
            page_set.and_then(CxfaNode::first_child)
        } else {
            start.and_then(CxfaNode::next_sibling)
        };
        while let Some(node) = current {
            current = node.next_sibling();
            match node.get_element_type() {
                E::PageArea => {
                    if !self.match_page_area_odd_or_even(&node) {
                        continue;
                    }
                    let position = node.get_enum(A::PagePosition);
                    if preferred == V::Last {
                        if position != preferred {
                            continue;
                        }
                        if self.state.borrow().page_set_mode == V::SimplexPaginated
                            || node.get_enum(A::OddOrEven) == V::Any
                        {
                            selected = Some(node);
                            break;
                        }
                        let record = self.create_view_record_simple();
                        self.add_page_area_layout_item(record, &node);
                        self.add_content_area_layout_item(
                            record,
                            node.get_first_child_by_class(E::ContentArea).as_ref(),
                        );
                        return false;
                    }
                    if preferred == V::Only {
                        if position != preferred {
                            continue;
                        }
                        if self.state.borrow().page_set_mode != V::DuplexPaginated
                            || node.get_enum(A::OddOrEven) == V::Any
                        {
                            selected = Some(node);
                            break;
                        }
                        return false;
                    }
                    if target.is_none_or(|target| *target == node) {
                        if node.get_first_child_by_class(E::ContentArea).is_none() {
                            if target == Some(&node) {
                                let record = self.create_view_record_simple();
                                self.add_page_area_layout_item(record, &node);
                                target = None;
                            }
                            continue;
                        }
                        if (preferred == V::Rest && position == V::Any) || position == preferred {
                            selected = Some(node);
                            break;
                        }
                        if position == V::Any && fallback.is_none() {
                            fallback = Some(node);
                        }
                    } else if target.is_some_and(|target| !self.match_page_area_odd_or_even(target))
                    {
                        let record = self.create_view_record_simple();
                        self.add_page_area_layout_item(record, &node);
                        self.add_content_area_layout_item(
                            record,
                            node.get_first_child_by_class(E::ContentArea).as_ref(),
                        );
                    }
                }
                E::PageSet => {
                    if self.find_simplex_duplex(
                        Some(&node),
                        None,
                        target,
                        content,
                        new_page,
                        query,
                        preferred,
                    ) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        let Some(page) = selected.or(fallback) else {
            return false;
        };
        if !query {
            let record = self.create_view_record_simple();
            self.add_page_area_layout_item(record, &page);
            let first = page.get_first_child_by_class(E::ContentArea);
            self.add_content_area_layout_item(record, content.or(first.as_ref()));
        }
        self.state.borrow_mut().current_page_area = Some(page);
        true
    }
    pub fn get_next_avail_page_area(
        &self,
        target: Option<&CxfaNode>,
        content: Option<&CxfaNode>,
        new_page: bool,
        query: bool,
    ) -> Option<CxfaNode> {
        let current = self.state.borrow().current_page_area.clone();
        let Some(current) = current else {
            let root = self.state.borrow().page_set_node.clone();
            self.find_page_area(root.as_ref(), None, target, content, new_page, query);
            return self.state.borrow().current_page_area.clone();
        };
        if target.is_none_or(|target| *target == current) {
            if !new_page && self.get_next_content_area(content) {
                return Some(current);
            }
            if self.ordered() {
                let max = occurrence(&current, A::Max).unwrap_or(-1);
                if max < 0 || self.state.borrow().current_page_count < max {
                    if !query {
                        let record = self.create_view_record(&current, false)?;
                        self.add_page_area_layout_item(record, &current)?;
                        let first = current.get_first_child_by_class(E::ContentArea);
                        self.add_content_area_layout_item(record, content.or(first.as_ref()));
                    }
                    let count = self.state.borrow().current_page_count;
                    self.state.borrow_mut().current_page_count = count.saturating_add(1);
                    return Some(current);
                }
            }
        }
        if !query && self.ordered() {
            self.create_min_page_record(Some(&current), false, true);
        }
        if self.find_page_area(
            current.parent().as_ref(),
            Some(&current),
            target,
            content,
            new_page,
            query,
        ) {
            return self.state.borrow().current_page_area.clone();
        }
        let mut page_set = current.parent();
        while let Some(node) = page_set {
            if self.find_page_area(Some(&node), None, target, content, new_page, query) {
                return self.state.borrow().current_page_area.clone();
            }
            if !query && self.ordered() {
                self.create_min_page_set_record(&node, false);
            }
            if self.find_page_area(None, Some(&node), target, content, new_page, query) {
                return self.state.borrow().current_page_area.clone();
            }
            if self.state.borrow().page_set_node.as_ref() == Some(&node) {
                break;
            }
            page_set = node.parent();
        }
        None
    }
    fn create_min_page_record(&self, page: Option<&CxfaNode>, target: bool, last: bool) -> i32 {
        let Some(page) = page else {
            return 0;
        };
        let min = occurrence(page, A::Min);
        let mut minimum = min.unwrap_or(0);
        if min.is_none() && !target {
            return minimum;
        }
        let content = page.get_first_child_by_class(E::ContentArea);
        if minimum < 1 && target && content.is_none() {
            minimum = 1;
        }
        let start = if last {
            self.state.borrow().current_page_count
        } else {
            0
        };
        for _ in start..minimum {
            let record = self.create_view_record_simple();
            self.add_page_area_layout_item(record, page);
            self.add_content_area_layout_item(record, content.as_ref());
        }
        minimum
    }
    fn create_min_page_set_record(&self, page_set: &CxfaNode, all: bool) {
        let Some(mut count) = self.state.borrow().page_set_map.get(page_set).copied() else {
            return;
        };
        if all {
            count = 0;
        }
        let Some(min) = occurrence(page_set, A::Min) else {
            return;
        };
        if count >= min {
            return;
        }
        for _ in count..min {
            for child in children(page_set) {
                match child.get_element_type() {
                    E::PageArea => {
                        self.create_min_page_record(Some(&child), false, false);
                    }
                    E::PageSet => self.create_min_page_set_record(&child, true),
                    _ => {}
                }
            }
        }
        self.state
            .borrow_mut()
            .page_set_map
            .insert(page_set.clone(), min);
    }
    fn create_next_min_record(&self, node: &CxfaNode) {
        let mut current = node.next_sibling();
        while let Some(node) = current {
            current = node.next_sibling();
            match node.get_element_type() {
                E::PageArea => {
                    self.create_min_page_record(Some(&node), false, false);
                }
                E::PageSet => self.create_min_page_set_record(&node, true),
                _ => {}
            }
        }
    }
    fn process_last_page_set(&self) {
        let Some(page) = self.state.borrow().current_page_area.clone() else {
            return;
        };
        self.create_min_page_record(Some(&page), false, true);
        self.create_next_min_record(&page);
        let mut current = page.parent();
        while let Some(node) = current {
            self.create_min_page_set_record(&node, false);
            if self.state.borrow().page_set_node.as_ref() == Some(&node) {
                break;
            }
            self.create_next_min_record(&node);
            current = node.parent();
        }
    }
    pub fn get_next_avail_content_height(&self, height: f32) -> bool {
        let Some(record) = self.current_view_record() else {
            return false;
        };
        let Some(area) = record.content_area else {
            return false;
        };
        if let Some(next) = area.form_node().get_next_same_class_sibling(E::ContentArea) {
            return next.get_measure_in_unit(A::H, XfaUnit::Pt) > height;
        }
        let Some(page) = record.page_area.map(|page| page.form_node()) else {
            return false;
        };
        if occurrence(&page, A::Max)
            .is_some_and(|max| self.state.borrow().current_page_count == max)
        {
            let (previous, count, length) = {
                let state = self.state.borrow();
                (
                    state.current_page_area.clone(),
                    state.current_page_count,
                    state.records.len(),
                )
            };
            let next = self.get_next_avail_page_area(None, None, false, true);
            let added = {
                let mut state = self.state.borrow_mut();
                state.current_page_area = previous;
                state.current_page_count = count;
                state.records.split_off(length)
            };
            if let Some(previous) = length
                .checked_sub(1)
                .and_then(|index| self.state.borrow().records.get(index).cloned())
            {
                for record in added {
                    Self::remove_layout_record(&record, &previous);
                }
            }
            return next
                .and_then(|next| next.get_first_child_by_class(E::ContentArea))
                .is_some_and(|next| next.get_measure_in_unit(A::H, XfaUnit::Pt) > height);
        }
        page.get_first_child_by_class(E::ContentArea)
            .is_some_and(|content| {
                let next = content.get_measure_in_unit(A::H, XfaUnit::Pt);
                next < XFA_LAYOUT_PRECISION || next > height
            })
    }
    fn process_simplex_duplex_page_set(&self, item: &CxfaViewLayoutItem, simplex: bool) {
        let (count, last) = get_page_area_count_and_last_page_area_from_page_set(item);
        let Some(last) = last else {
            return;
        };
        if !self.find_simplex_duplex(
            Some(&item.form_node()),
            None,
            None,
            None,
            true,
            true,
            if count == 1 { V::Only } else { V::Last },
        ) && count == 1
            && !self.find_simplex_duplex(
                Some(&item.form_node()),
                None,
                None,
                None,
                true,
                true,
                V::Last,
            )
        {
            return;
        }
        let Some(node) = self.state.borrow().current_page_area.clone() else {
            return;
        };
        let choice = node.get_enum(A::PagePosition);
        if choice == V::Last
            && last.form_node().get_enum(A::PagePosition) == V::First
            && (simplex || node.get_enum(A::OddOrEven) != V::Odd)
        {
            let record = self.create_view_record_simple();
            self.add_page_area_layout_item(record, &node);
            return;
        }
        let used = get_heights_for_content_areas(&last);
        if content_areas_fit_in_page_areas(&node, &used) {
            last.set_form_node(&node);
            let mut content = node.first_child();
            let mut child = last.first_child();
            while let (Some(item), Some(node)) = (child.clone(), content.clone()) {
                if item.form_node().get_element_type() != E::ContentArea {
                    child = item.next_sibling();
                    continue;
                }
                if node.get_element_type() != E::ContentArea {
                    content = node.next_sibling();
                    continue;
                }
                item.set_form_node(&node);
                child = item.next_sibling();
                content = node.next_sibling();
            }
            return;
        }
        if choice == V::Last {
            let record = self.create_view_record_simple();
            self.add_page_area_layout_item(record, &node);
        }
    }
    pub fn finish_paginated_page_sets(&self) {
        for root in self.root_layout_items() {
            let mut iterator = ViewLayoutItemIterator::new(Some(root));
            let mut next = iterator.get_current();
            while let Some(item) = next {
                next = iterator.move_to_next();
                if item.form_node().get_element_type() != E::PageSet {
                    continue;
                }
                match item.form_node().get_enum(A::Relation) {
                    V::SimplexPaginated => self.process_simplex_duplex_page_set(&item, true),
                    V::DuplexPaginated => self.process_simplex_duplex_page_set(&item, false),
                    _ => self.process_last_page_set(),
                }
            }
        }
    }
}

impl CxfaViewLayoutProcessor {
    pub fn resolve_break_target(
        &self,
        root: Option<&CxfaNode>,
        new_expression: bool,
        target: &[u32],
    ) -> ViewLayoutResult<Option<CxfaNode>> {
        let Some(root) = root else {
            return Ok(None);
        };
        let Some(document) = root.document() else {
            return Err(ViewLayoutError::ExpiredDocument);
        };
        let space = |code: &u32| char::from_u32(*code).is_some_and(char::is_whitespace);
        let start = target
            .iter()
            .position(|code| !space(code))
            .unwrap_or(target.len());
        let end = target
            .iter()
            .rposition(|code| !space(code))
            .map_or(start, |index| index + 1);
        let target = &target[start..end];
        if target.is_empty() {
            return Ok(None);
        }
        // Try the full expression first, then each alternative. The native
        // splitter fails to advance on a space; each slice here makes progress.
        for expression in std::iter::once(target).chain(
            target
                .split(|&code| code == 32)
                .filter(|part| !part.is_empty()),
        ) {
            if expression[0] == 35 {
                let template = document.get_xfa_node(XfaHashCode::Template as u32);
                if let Some(node) = document.get_node_by_id(template.as_ref(), &expression[1..]) {
                    return Ok(Some(node));
                }
            } else if new_expression {
                let expression = if expression.starts_with(&[115, 111, 109, 40])
                    && expression.last() == Some(&41)
                {
                    &expression[4..expression.len() - 1]
                } else {
                    expression
                };
                let runtime = self.runtime()?;
                let resolved = runtime.resolve_objects(
                    Some(root),
                    expression,
                    XfaResolveFlags::CHILDREN
                        | XfaResolveFlags::PROPERTIES
                        | XfaResolveFlags::ATTRIBUTES
                        | XfaResolveFlags::SIBLINGS
                        | XfaResolveFlags::PARENT,
                );
                match resolved {
                    Ok(Some(result)) => {
                        if let Some(node) = result.objects.into_iter().next() {
                            return Ok(Some(node));
                        }
                    }
                    Ok(None) => {}
                    Err(crate::fxjs::xfa::cfxjse_resolveprocessor::ResolveError::MalformedExpression
                        | crate::fxjs::xfa::cfxjse_resolveprocessor::ResolveError::InvalidIndex) => {}
                    Err(error) => return Err(ViewLayoutError::Resolve(error)),
                }
            }
        }
        Ok(None)
    }
    fn run_break(&self, target_type: V, target: Option<&CxfaNode>, start_new: bool) -> bool {
        match target_type {
            V::ContentArea => {
                let target = target.filter(|node| node.get_element_type() == E::ContentArea);
                if self.should_get_next_page_area(target, start_new) {
                    let page = target.and_then(CxfaNode::parent);
                    self.get_next_avail_page_area(page.as_ref(), target, false, false)
                        .is_some()
                } else {
                    false
                }
            }
            V::PageArea => {
                let target = target.filter(|node| node.get_element_type() == E::PageArea);
                self.should_get_next_page_area(target, start_new)
                    && self
                        .get_next_avail_page_area(target, None, true, false)
                        .is_some()
            }
            // These are intentional empty native cases, not implemented parity
            // breaks. Page-selection OddOrEven matching is separate and active.
            V::PageOdd | V::PageEven | V::Auto => false,
            _ => false,
        }
    }
    fn execute_break_before_or_after(
        &self,
        node: &CxfaNode,
        before: bool,
    ) -> ViewLayoutResult<BreakData> {
        let mut result = BreakData::default();
        match node.get_element_type() {
            E::BreakBefore | E::BreakAfter => {
                let Some(form) = node.get_container_parent() else {
                    return Ok(result);
                };
                let container = form.get_template_node_if_exists();
                let start = node.get_integer(A::StartNew) != 0;
                if let Some(script) = node.get_first_child_by_class(E::Script) {
                    if !script.get_content().is_empty() {
                        let parent =
                            script
                                .get_container_parent()
                                .ok_or(ViewLayoutError::InvalidTree(
                                    "break script has no container",
                                ))?;
                        if !self.services()?.run_break_script(&script, &parent)? {
                            return Ok(result);
                        }
                    }
                }
                let root = self.state.borrow().page_set_node.clone();
                let target =
                    self.resolve_break_target(root.as_ref(), true, &node.get_cdata(A::Target))?;
                let trailer = node.get_cdata(A::Trailer);
                let leader = node.get_cdata(A::Leader);
                result.leader = self.resolve_break_target(container.as_ref(), true, &leader)?;
                result.trailer = self.resolve_break_target(container.as_ref(), true, &trailer)?;
                if self.run_break(node.get_enum(A::TargetType), target.as_ref(), start) {
                    result.create_page = true;
                    return Ok(result);
                }
                let first = {
                    let state = self.state.borrow();
                    !state.records.is_empty() && state.current_record == Some(0)
                };
                if first && node.get_element_type() == E::BreakBefore {
                    if let Some(parent) = form.get_container_parent() {
                        if parent.get_first_container_child().as_ref() == Some(&form)
                            && parent
                                .parent()
                                .is_some_and(|node| node.get_element_type() == E::Form)
                        {
                            result.create_page = true;
                        }
                    }
                }
            }
            E::Break => {
                let target_attr = if before {
                    A::BeforeTarget
                } else {
                    A::AfterTarget
                };
                let target_value = if before { A::Before } else { A::After };
                let root = self.state.borrow().page_set_node.clone();
                let target =
                    self.resolve_break_target(root.as_ref(), true, &node.get_cdata(target_attr))?;
                result.create_page = self.run_break(
                    node.get_enum(target_value),
                    target.as_ref(),
                    node.get_integer(A::StartNew) != 0,
                );
            }
            _ => {}
        }
        Ok(result)
    }
    fn copy_generated(
        &self,
        template: &CxfaNode,
        parent: &CxfaNode,
        scope: Option<&CxfaNode>,
    ) -> ViewLayoutResult<Option<CxfaNode>> {
        let document = template
            .document()
            .ok_or(ViewLayoutError::ExpiredDocument)?;
        let runtime = self.runtime()?;
        let node = document.data_merge_copy_container_with_resolver(
            runtime.resolver(),
            template,
            parent,
            scope,
            true,
            true,
            true,
        )?;
        if let Some(node) = &node {
            document.data_merge_update_binding_relations_with_resolver(runtime.resolver(), node)?;
            generated(node);
        }
        Ok(node)
    }
    fn process_break_before_or_after(
        &self,
        node: &CxfaNode,
        before: bool,
    ) -> ViewLayoutResult<Option<BreakData>> {
        let Some(form) = node.get_container_parent() else {
            return Ok(None);
        };
        if !form.presence_requires_space() {
            return Ok(None);
        }
        let mut result = self.execute_break_before_or_after(node, before)?;
        let parent = form.get_container_parent();
        let scope = parent
            .as_ref()
            .and_then(|parent| data_merge_find_data_scope(Some(parent)));
        if let Some(template) = result.leader.take() {
            if !template.is_container_node() {
                return Ok(None);
            }
            let Some(parent) = parent.as_ref() else {
                return Ok(None);
            };
            result.leader = self.copy_generated(&template, parent, scope.as_ref())?;
            if result.leader.is_none() {
                return Ok(None);
            }
        }
        if let Some(template) = result.trailer.take() {
            if !template.is_container_node() {
                return Ok(None);
            }
            let Some(parent) = parent.as_ref() else {
                return Ok(None);
            };
            result.trailer = self.copy_generated(&template, parent, scope.as_ref())?;
            if result.trailer.is_none() {
                return Ok(None);
            }
        }
        Ok(Some(result))
    }
    pub fn process_break_before(&self, node: &CxfaNode) -> ViewLayoutResult<Option<BreakData>> {
        self.process_break_before_or_after(node, true)
    }
    pub fn process_break_after(&self, node: &CxfaNode) -> ViewLayoutResult<Option<BreakData>> {
        self.process_break_before_or_after(node, false)
    }
    fn resolve_bookend(&self, node: &CxfaNode, leader: bool) -> ViewLayoutResult<Option<CxfaNode>> {
        let container = node
            .get_container_parent()
            .and_then(|node| node.get_template_node_if_exists());
        match node.get_element_type() {
            E::Break => self.resolve_break_target(
                container.as_ref(),
                false,
                &node.get_cdata(if leader {
                    A::BookendLeader
                } else {
                    A::BookendTrailer
                }),
            ),
            E::Bookend => self.resolve_break_target(
                container.as_ref(),
                true,
                &node.get_cdata(if leader { A::Leader } else { A::Trailer }),
            ),
            _ => Ok(None),
        }
    }
    fn process_bookend(&self, node: &CxfaNode, leader: bool) -> ViewLayoutResult<Option<CxfaNode>> {
        let Some(parent) = node.get_container_parent() else {
            return Ok(None);
        };
        let Some(template) = self.resolve_bookend(node, leader)? else {
            return Ok(None);
        };
        if !template.is_container_node() {
            return Ok(None);
        }
        let scope = data_merge_find_data_scope(Some(&parent));
        self.copy_generated(&template, &parent, scope.as_ref())
    }
    pub fn process_bookend_leader(&self, node: &CxfaNode) -> ViewLayoutResult<Option<CxfaNode>> {
        self.process_bookend(node, true)
    }
    pub fn process_bookend_trailer(&self, node: &CxfaNode) -> ViewLayoutResult<Option<CxfaNode>> {
        self.process_bookend(node, false)
    }
    fn break_overflow(
        &self,
        node: &CxfaNode,
        create_page: bool,
    ) -> ViewLayoutResult<Option<OverflowData>> {
        let (leader_attr, target_attr, trailer_attr) = match node.get_element_type() {
            E::Break => (A::OverflowLeader, A::OverflowTarget, A::OverflowTrailer),
            E::Overflow => (A::Leader, A::Target, A::Trailer),
            _ => return Ok(None),
        };
        let leader = node.get_cdata(leader_attr);
        let target = node.get_cdata(target_attr);
        let trailer = node.get_cdata(trailer_attr);
        if node.get_element_type() == E::Break
            && leader.is_empty()
            && target.is_empty()
            && trailer.is_empty()
        {
            return Ok(None);
        }
        if !target.is_empty() && create_page && !self.state.borrow().create_overflow_page {
            let root = self.state.borrow().page_set_node.clone();
            if let Some(target) = self.resolve_break_target(root.as_ref(), true, &target)? {
                self.state.borrow_mut().create_overflow_page = true;
                let target_type = match target.get_element_type() {
                    E::PageArea => Some(V::PageArea),
                    E::ContentArea => Some(V::ContentArea),
                    _ => None,
                };
                if let Some(target_type) = target_type {
                    self.run_break(target_type, Some(&target), true);
                }
            }
        }
        if create_page {
            return Ok(Some(OverflowData::default()));
        }
        let container = node
            .get_container_parent()
            .and_then(|node| node.get_template_node_if_exists());
        Ok(Some(OverflowData {
            leader: self.resolve_break_target(container.as_ref(), true, &leader)?,
            trailer: self.resolve_break_target(container.as_ref(), true, &trailer)?,
        }))
    }
    pub fn process_overflow(
        &self,
        node: Option<&CxfaNode>,
        create_page: bool,
    ) -> ViewLayoutResult<Option<OverflowData>> {
        let Some(form) = node else {
            return Ok(None);
        };
        let direct = matches!(form.get_element_type(), E::Overflow | E::Break);
        let mut current = if direct {
            Some(form.clone())
        } else {
            form.first_child()
        };
        while let Some(node) = current {
            current = if direct { None } else { node.next_sibling() };
            let Some(templates) = self.break_overflow(&node, create_page)? else {
                continue;
            };
            let parent = if direct {
                node.parent()
            } else {
                Some(form.clone())
            };
            let Some(parent) = parent else {
                return Ok(None);
            };
            let scope = data_merge_find_data_scope(Some(&parent));
            let mut result = OverflowData::default();
            if let Some(template) = templates.leader {
                result.leader = self.copy_generated(&template, &parent, scope.as_ref())?;
                if result.leader.is_none() {
                    return Ok(None);
                }
            }
            if let Some(template) = templates.trailer.filter(CxfaNode::is_container_node) {
                result.trailer = self.copy_generated(&template, &parent, scope.as_ref())?;
                if result.trailer.is_none() {
                    return Ok(None);
                }
            }
            return Ok(Some(result));
        }
        Ok(None)
    }
    pub fn query_overflow(&self, node: &CxfaNode) -> Option<CxfaNode> {
        super::query_overflow(node)
    }
}

impl CxfaViewLayoutProcessor {
    fn notify_remove(&self, item: &CxfaLayoutItem) {
        if let Some(notify) = self.store.notify() {
            notify.on_layout_item_removing(item);
        }
        item.retire_from_layout();
    }
    fn sync_remove_layout(&self, root: &CxfaLayoutItem) {
        let mut stack = vec![(root.clone(), false)];
        while let Some((item, visited)) = stack.pop() {
            if visited {
                self.notify_remove(&item);
            } else {
                stack.push((item.clone(), true));
                let mut children = item.children();
                children.reverse();
                for child in children {
                    stack.push((child, false));
                }
            }
        }
    }
    fn save_layout_item_children(&self, root: &CxfaLayoutItem) {
        let mut stack = root.children();
        stack.reverse();
        while let Some(item) = stack.pop() {
            if item.is_content_layout_item() {
                let node = item.form_node();
                if node.has_removed_children() {
                    self.sync_remove_layout(&item);
                    continue;
                }
                if node.is_layout_generated_node() {
                    node.set_node_and_descendants_unused();
                }
            }
            let mut children = item.children();
            children.reverse();
            stack.extend(children);
        }
    }
    fn prepare_layout(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.current_page_set = None;
            state.page_set_mode = V::OrderedOccurrence;
            state.available_pages = 0;
        }
        self.clear_data();
        let mut roots = self.root_layout_items();
        let Some(root) = roots.first().cloned() else {
            return;
        };
        if root.form_node().get_packet_type() == XfaPacketType::Form {
            let root_node = root.form_node();
            if let Some(document) = root_node.document() {
                document.clear_pending_nodes();
                let mut page_set = Some(root_node.clone());
                if root_node.has_removed_children() {
                    release_layout_item(&root);
                    let mut state = self.state.borrow_mut();
                    state.root = None;
                    state.roots.clear();
                    state.pages.clear();
                    roots.clear();
                    page_set = None;
                }
                while let Some(node) = page_set {
                    page_set = node.get_next_same_class_sibling(E::PageSet);
                    if let Some(parent) = node.parent() {
                        parent.remove_child_and_notify(&node, false);
                    }
                    document.append_pending_node(node);
                }
            }
        }
        for root in roots {
            self.save_layout_item_children(&root);
            root.remove_self_if_parented();
        }
        let mut state = self.state.borrow_mut();
        state.root = None;
        state.roots.clear();
    }
    fn remove_existing_page_form(&self, existing: &CxfaNode) {
        let mut iterator = CxfaContainerIterator::new(Some(existing.clone()));
        let mut current = iterator.get_current();
        while let Some(node) = current {
            current = iterator.move_to_next();
            if node.get_element_type() != E::ContentArea {
                if let Some(item) = node.get_layout_item() {
                    self.notify_remove(&item);
                }
            }
        }
        if let Some(parent) = existing.parent() {
            parent.remove_child_and_notify(existing, true);
        }
    }
    pub fn merge_page_set_contents(&self) -> ViewLayoutResult<()> {
        let document = self.document()?;
        let runtime = self.runtime()?;
        document.set_pending_nodes_unused_and_unbound();
        for (pending_index, root) in self.root_layout_items().into_iter().enumerate() {
            if root.form_node().get_element_type() != E::PageSet {
                return Err(ViewLayoutError::InvalidTree(
                    "root view item is not a pageSet",
                ));
            }
            let old_root = root.form_node();
            let pending = if let Some(pending) = document.get_pending_node_at_index(pending_index) {
                pending
            } else if old_root.get_packet_type() == XfaPacketType::Template {
                old_root
                    .clone_template_to_form(false)
                    .ok_or(ViewLayoutError::InvalidTree("cannot clone pageSet to form"))?
            } else {
                old_root.clone()
            };
            if old_root.get_layout_item().as_ref() == Some(&*root) {
                old_root.set_layout_item(None);
            }
            root.set_form_node(&pending);
            pending.clear_flag(XfaNodeFlag::UnusedNode);
            let mut iterator = ViewLayoutItemIterator::new(Some(root.clone()));
            let mut next = iterator.move_to_next();
            while let Some(item) = next {
                next = iterator.move_to_next();
                let node = item.form_node();
                if node.get_packet_type() != XfaPacketType::Template {
                    continue;
                }
                let Some(parent) = item.parent().map(|item| item.form_node()) else {
                    return Err(ViewLayoutError::InvalidTree("view item has no form parent"));
                };
                match node.get_element_type() {
                    E::PageSet => {
                        let merged = node_merge_clone_or_merge_container(
                            &document, &parent, &node, true, None,
                        )
                        .ok_or(ViewLayoutError::InvalidTree("cannot merge nested pageSet"))?;
                        if merged != node {
                            node.set_layout_item(None);
                            item.set_form_node(&merged);
                        }
                    }
                    E::PageArea => {
                        let mut form_layout = Some((*item).clone());
                        for level in 0..3 {
                            form_layout = form_layout.and_then(|item| item.first_child());
                            if level == 2 {
                                while form_layout
                                    .as_ref()
                                    .is_some_and(|item| !item.form_node().presence_requires_space())
                                {
                                    form_layout = form_layout.and_then(|item| item.next_sibling());
                                }
                            }
                            if form_layout.is_none() {
                                break;
                            }
                        }
                        if let Some(form_layout) = form_layout {
                            let new_subform = form_layout.form_node();
                            if item.old_subform().is_some_and(|old| old != new_subform) {
                                if let Some(existing) = data_merge_find_form_dom_instance(
                                    &document,
                                    node.get_element_type(),
                                    node.get_name_hash(),
                                    &parent,
                                ) {
                                    self.remove_existing_page_form(&existing);
                                }
                            }
                            item.set_old_subform(Some(&new_subform));
                        }
                        let record = document.get_xfa_node(XfaHashCode::Record as u32);
                        let merged = document
                            .data_merge_copy_container_with_resolver(
                                runtime.resolver(),
                                &node,
                                &parent,
                                record.as_ref(),
                                true,
                                true,
                                true,
                            )?
                            .ok_or(ViewLayoutError::InvalidTree("cannot merge pageArea"))?;
                        if merged != node {
                            node.set_layout_item(None);
                            item.set_form_node(&merged);
                        }
                    }
                    E::ContentArea => {
                        for child in children(&parent) {
                            if child.get_template_node_if_exists().as_ref() == Some(&node) {
                                item.set_form_node(&child);
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
            if pending.parent().is_none() {
                if let Some(subform) = document
                    .get_xfa_node(XfaHashCode::Form as u32)
                    .and_then(|form| form.get_first_child_by_class(E::Subform))
                {
                    subform.insert_child_and_notify(-1, &pending);
                }
            }
            document
                .data_merge_update_binding_relations_with_resolver(runtime.resolver(), &pending)?;
            pending.set_initialized_flag_and_notify();
        }
        let mut page_set = self.get_root_layout_item().map(|item| item.form_node());
        while let Some(root) = page_set {
            page_set = root.get_next_same_class_sibling(E::PageSet);
            let mut iterator = CxfaNodeIterator::new(Some(root));
            let mut current = iterator.get_current();
            while let Some(node) = current {
                if node.is_unused_node() {
                    if node.is_container_node() {
                        if matches!(node.get_element_type(), E::PageArea | E::PageSet) {
                            let mut containers = CxfaContainerIterator::new(Some(node.clone()));
                            let mut child = containers.move_to_next();
                            while let Some(node) = child {
                                child = containers.move_to_next();
                                if let Some(layout) = node.get_layout_item() {
                                    self.notify_remove(&layout);
                                }
                            }
                        } else if node.get_element_type() != E::ContentArea {
                            if let Some(layout) = node.get_layout_item() {
                                self.notify_remove(&layout);
                            }
                        }
                        current = iterator.skip_children_and_move_to_next();
                        if let Some(parent) = node.parent() {
                            parent.remove_child_and_notify(&node, true);
                        }
                    } else {
                        node.clear_flag(XfaNodeFlag::UnusedNode);
                        node.set_initialized_flag_and_notify();
                        current = iterator.move_to_next();
                    }
                } else {
                    node.set_initialized_flag_and_notify();
                    current = iterator.move_to_next();
                }
            }
        }
        Ok(())
    }
    pub fn layout_page_set_contents(&self) -> ViewLayoutResult<()> {
        let host = self.services()?;
        for root in self.root_layout_items() {
            let mut iterator = ViewLayoutItemIterator::new(Some(root));
            let mut current = iterator.get_current();
            while let Some(item) = current {
                current = iterator.move_to_next();
                if item.form_node().get_element_type() == E::PageArea {
                    host.layout_page_area(&item)?;
                }
            }
        }
        Ok(())
    }
    pub fn sync_layout_data(&self) -> ViewLayoutResult<()> {
        self.merge_page_set_contents()?;
        self.layout_page_set_contents()?;
        self.sync_layout_item_status();
        Ok(())
    }
}
#[cfg(test)]
#[path = "cxfa_viewlayoutprocessor_pages_tests.rs"]
mod tests;
