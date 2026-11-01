// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Included privately by cxfa_contentlayoutprocessor after the coordinated
// integration checkpoint. All state is owned, view edges are scoped borrows.
use super::cxfa_viewlayoutprocessor::{CxfaViewLayoutProcessor, ViewLayoutError};
use crate::xfa::fxfa::parser::{
    cxfa_node::XfaNodeFlag, cxfa_nodeiteratortemplate::CxfaNodeIteratorTemplate,
    cxfa_traversestrategy_xfanode::CxfaTraverseStrategyXfaNode,
};
use std::collections::VecDeque;
#[cfg(test)]
#[path = "cxfa_contentlayoutprocessor_pages_tests.rs"]
mod pagination_tests;

struct PaginationState {
    pre_process_result: LayoutResult,
    break_pending: bool,
    use_inherited: bool,
    keep_break_finish: bool,
    is_process_keep: bool,
    has_avail_height: bool,
    used_size: f32,
    width_limit: f32,
    current_child: Option<CxfaNode>,
    keep_head: Option<CxfaNode>,
    keep_tail: Option<CxfaNode>,
    child_preprocessor: Option<Box<CxfaContentLayoutProcessor>>,
    keep_items: Vec<CxfaContentLayoutItem>,
    pending_nodes: VecDeque<CxfaNode>,
    detached_fragments: VecDeque<CxfaContentLayoutItem>,
}
impl Default for PaginationState {
    fn default() -> Self {
        Self {
            pre_process_result: LayoutResult::Done,
            break_pending: true,
            use_inherited: false,
            keep_break_finish: false,
            is_process_keep: false,
            has_avail_height: true,
            used_size: 0.,
            width_limit: 0.,
            current_child: None,
            keep_head: None,
            keep_tail: None,
            child_preprocessor: None,
            keep_items: Vec::new(),
            pending_nodes: VecDeque::new(),
            detached_fragments: VecDeque::new(),
        }
    }
}
#[derive(Clone, Default)]
struct LayoutContext<'a> {
    column_width: Option<f32>,
    columns: Option<&'a [f32]>,
    overflow_processor: Option<CxfaNode>,
    overflow_node: Option<CxfaNode>,
}
struct FlowRow {
    groups: [Vec<CxfaContentLayoutItem>; 3],
    align: usize,
    y: f32,
    height: f32,
    available_width: f32,
    added: bool,
    force_end_page: bool,
}
impl FlowRow {
    fn new(flow: V, y: f32, width: f32) -> Self {
        Self {
            groups: Default::default(),
            align: if flow == V::Rl_tb { 2 } else { 0 },
            y,
            height: 0.,
            available_width: width,
            added: false,
            force_end_page: false,
        }
    }
}
impl From<ViewLayoutError> for LayoutError {
    fn from(error: ViewLayoutError) -> Self {
        Self::View(Box::new(error))
    }
}
impl CxfaContentLayoutProcessor {
    #[allow(clippy::too_many_arguments)]
    fn do_layout_flowed_container(
        &mut self,
        view: Option<&CxfaViewLayoutProcessor>,
        use_break_control: bool,
        flow: V,
        height_limit: f32,
        mut real_height: f32,
        context: &mut LayoutContext<'_>,
        root_force_tb: bool,
    ) -> Result<LayoutResult, LayoutError> {
        // An intact flowed child can finish layout before its parent discovers
        // that it does not fit the current content area. Keep that completed
        // item while the parent retries it on the next area. Re-running the
        // completed layout would subtract used_size from its specified height
        // a second time and collapse the retained item to zero height.
        if self.stage == LayoutStage::Done
            && self.layout_item.is_some()
            && self.pages.pending_nodes.is_empty()
        {
            return Ok(LayoutResult::Done);
        }
        self.pages.has_avail_height = true;
        if let Some(child) = self.pages.child_preprocessor.as_mut() {
            child.pages.pre_process_result = LayoutResult::Done;
        }
        let node = self.form_node.clone();
        let mut specified = calculate_container_specified_size(&node);
        self.adjust_container_specified_size(context, &mut specified);
        let inset = get_margin_inset(node.get_first_child_by_class(E::Margin).as_ref());
        let width_limit = if specified.width_auto {
            f32::MAX
        } else {
            specified.size.width - inset.left - inset.right
        };
        let available_height = height_limit - inset.top - inset.bottom;
        if available_height < 0. {
            self.pages.has_avail_height = false;
        }
        real_height -= inset.top + inset.bottom;
        let mut last = self.find_last_content_layout_item(flow);
        let mut calculated = if self.layout_item.is_some() {
            self.calculate_layout_item_size(last.as_ref())
        } else {
            CfxSizeF::default()
        };
        let mut y = last
            .as_ref()
            .map_or(calculated.height, |item| item.position().y);
        y += self.insert_keep_layout_items()?;
        if self.stage == LayoutStage::None {
            self.goto_next_container_node_simple();
        }
        y += self.insert_pending_items(&node)?;
        if self.stage == LayoutStage::Container
            && self
                .pages
                .child_preprocessor
                .as_ref()
                .is_some_and(|processor| exist_container_keep(Some(&processor.form_node), false))
        {
            self.pages.keep_head = self.pages.current_child.clone();
            self.pages.is_process_keep = true;
            self.stage = LayoutStage::Keep;
        }
        let (mut force_end_page, mut break_done, mut manual_break) = (false, false, false);
        while self.stage != LayoutStage::Done {
            let mut row = FlowRow::new(flow, y, width_limit);
            self.pages.width_limit = width_limit;
            if let Some(last_item) = last.take() {
                let mut next = Some((*last_item).clone());
                while let Some(item) = next {
                    next = item.next_sibling();
                    let Some(content) = item.as_content_layout_item() else {
                        continue;
                    };
                    if content.next_sibling().is_none() {
                        if let Some(processor) = self
                            .pages
                            .child_preprocessor
                            .as_mut()
                            .filter(|processor| processor.form_node == content.form_node())
                        {
                            if let Some(current) = processor
                                .layout_item
                                .as_ref()
                                .filter(|current| **current != content)
                            {
                                content.insert_after(current);
                            }
                            processor.layout_item = Some(content);
                            break;
                        }
                    }
                    let align = h_align(content.form_node().get_enum(A::HAlign));
                    row.groups[align].push(content.clone());
                    row.align = if flow == V::Lr_tb {
                        row.align.max(align)
                    } else {
                        row.align.min(align)
                    };
                    if content.form_node().presence_requires_space() {
                        row.height = row.height.max(content.size().height);
                        row.available_width -= content.size().width;
                    }
                }
                let mut next = Some(last_item);
                while let Some(content) = next {
                    next = content
                        .next_sibling()
                        .and_then(|item| item.as_content_layout_item());
                    content.remove_self_if_parented();
                }
            }
            while let Some(current) = self.pages.current_child.clone() {
                row.added = false;
                row.y += self.insert_pending_items(&node)?;
                let mut suspend: Option<Box<CxfaContentLayoutProcessor>> = None;
                let mut suspend_row = false;
                match self.stage {
                    LayoutStage::Keep | LayoutStage::None => {}
                    LayoutStage::BreakBefore => {
                        if self.layout_item.is_some() {
                            for item in &self.pages.keep_items {
                                item.remove_self_if_parented();
                                calculated.height -= item.size().height;
                            }
                        }
                        if use_break_control {
                            if let Some(view) = view {
                                if let Some(data) = view.process_break_before(&current)? {
                                    if data.create_page && node.get_element_type() != E::Form {
                                        if self
                                            .judge_leader_or_trailer_for_occur(data.leader.as_ref())
                                        {
                                            self.add_pending_node(data.leader.unwrap(), true);
                                        }
                                        if self.judge_leader_or_trailer_for_occur(
                                            data.trailer.as_ref(),
                                        ) {
                                            let trailer = data.trailer.unwrap();
                                            if node.parent().is_some_and(|parent| {
                                                parent.get_element_type() == E::Form
                                            }) && self.layout_item.is_none()
                                            {
                                                self.add_pending_node(trailer, true);
                                            } else {
                                                let mut processor = self.child_processor(&trailer);
                                                self.insert_flowed_item(
                                                    view.into(),
                                                    &mut processor,
                                                    specified,
                                                    flow,
                                                    false,
                                                    f32::MAX,
                                                    f32::MAX,
                                                    width_limit,
                                                    &mut row,
                                                    context,
                                                    false,
                                                )?;
                                            }
                                        }
                                        self.goto_next_container_node_simple();
                                        row.force_end_page = true;
                                        manual_break = true;
                                        suspend_row = true;
                                    }
                                }
                            }
                        }
                    }
                    LayoutStage::BreakAfter => {
                        if use_break_control {
                            if let Some(view) = view {
                                if let Some(data) = view.process_break_after(&current)? {
                                    if node.get_element_type() != E::Form {
                                        let create_page = data.create_page;
                                        if self.judge_leader_or_trailer_for_occur(
                                            data.trailer.as_ref(),
                                        ) {
                                            let mut processor =
                                                self.child_processor(&data.trailer.unwrap());
                                            self.insert_flowed_item(
                                                Some(view),
                                                &mut processor,
                                                specified,
                                                flow,
                                                false,
                                                f32::MAX,
                                                f32::MAX,
                                                width_limit,
                                                &mut row,
                                                context,
                                                false,
                                            )?;
                                        }
                                        if !create_page {
                                            if self.judge_leader_or_trailer_for_occur(
                                                data.leader.as_ref(),
                                            ) {
                                                self.calculate_row_child_position(
                                                    &row.groups,
                                                    flow,
                                                    specified,
                                                    &mut calculated,
                                                    &mut row.y,
                                                    row.height,
                                                    width_limit,
                                                    false,
                                                );
                                                // A completed row is consumed. Clearing all align groups
                                                // avoids re-inserting centered/right items from that row.
                                                row.groups = Default::default();
                                                row.height = 0.;
                                                row.available_width = width_limit;
                                                let mut processor =
                                                    self.child_processor(&data.leader.unwrap());
                                                self.insert_flowed_item(
                                                    Some(view),
                                                    &mut processor,
                                                    specified,
                                                    flow,
                                                    false,
                                                    f32::MAX,
                                                    f32::MAX,
                                                    width_limit,
                                                    &mut row,
                                                    context,
                                                    false,
                                                )?;
                                            }
                                        } else if self
                                            .judge_leader_or_trailer_for_occur(data.leader.as_ref())
                                        {
                                            self.add_pending_node(data.leader.unwrap(), true);
                                        }
                                        self.goto_next_container_node_simple();
                                        if create_page {
                                            row.force_end_page = true;
                                            manual_break = true;
                                            if self.stage == LayoutStage::Done {
                                                break_done = true;
                                            }
                                        }
                                        suspend_row = true;
                                    }
                                }
                            }
                        }
                    }
                    LayoutStage::BookendLeader | LayoutStage::BookendTrailer => {
                        let mut processor =
                            if let Some(processor) = self.pages.child_preprocessor.take() {
                                Some(processor)
                            } else if let Some(view) = view {
                                let generated = if self.stage == LayoutStage::BookendLeader {
                                    view.process_bookend_leader(&current)?
                                } else {
                                    view.process_bookend_trailer(&current)?
                                };
                                generated.map(|node| Box::new(self.child_processor(&node)))
                            } else {
                                None
                            };
                        if let Some(mut processor) = processor.take() {
                            let result = self.insert_flowed_item(
                                view,
                                &mut processor,
                                specified,
                                flow,
                                use_break_control,
                                available_height,
                                real_height,
                                width_limit,
                                &mut row,
                                context,
                                false,
                            )?;
                            if result != LayoutResult::Done {
                                suspend = Some(processor);
                                suspend_row = true;
                            }
                        }
                    }
                    LayoutStage::Container => {
                        if current.get_element_type() != E::Variables && current.is_container_node()
                        {
                            if row.y >= height_limit + XFA_LAYOUT_PRECISION
                                && current.presence_requires_space()
                            {
                                row.force_end_page = true;
                                suspend_row = true;
                            } else {
                                let new_row = self.pages.child_preprocessor.is_some();
                                let mut processor =
                                    self.pages.child_preprocessor.take().unwrap_or_else(|| {
                                        Box::new(self.child_processor(&current))
                                    });
                                processor.insert_pending_items(&current)?;
                                let result = self.insert_flowed_item(
                                    view,
                                    &mut processor,
                                    specified,
                                    flow,
                                    use_break_control,
                                    available_height,
                                    real_height,
                                    width_limit,
                                    &mut row,
                                    context,
                                    new_row,
                                )?;
                                match result {
                                    LayoutResult::ManualBreak => {
                                        manual_break = true;
                                        row.force_end_page = true;
                                        suspend = Some(processor);
                                        suspend_row = true;
                                    }
                                    LayoutResult::PageFullBreak => {
                                        row.force_end_page = true;
                                        suspend = Some(processor);
                                        suspend_row = true;
                                    }
                                    LayoutResult::RowFullBreak => {
                                        suspend = Some(processor);
                                        suspend_row = true;
                                    }
                                    LayoutResult::Done => {
                                        row.y += processor.insert_pending_items(&current)?;
                                    }
                                }
                            }
                        }
                    }
                    LayoutStage::Done => {}
                }
                if suspend_row {
                    if let Some(processor) = suspend {
                        self.pages.child_preprocessor = Some(processor);
                    }
                    break;
                }
                self.goto_next_container_node_simple();
                if row.added && flow == V::Tb {
                    break;
                }
            }
            self.calculate_row_child_position(
                &row.groups,
                flow,
                specified,
                &mut calculated,
                &mut row.y,
                row.height,
                width_limit,
                root_force_tb,
            );
            self.pages.width_limit = row.available_width;
            y = row.y;
            force_end_page = row.force_end_page;
            if force_end_page {
                break;
            }
        }
        let done =
            self.stage == LayoutStage::Done && self.pages.pending_nodes.is_empty() && !break_done;
        let mut size =
            calculate_container_component_size_from_content_size(&node, specified, calculated);
        if size.height >= XFA_LAYOUT_PRECISION || self.layout_item.is_some() || done {
            if self.layout_item.is_none() {
                self.layout_item = Some(self.create_content_layout_item(&node));
            }
            size.height = size.height.max(0.);
            self.layout_item.as_ref().unwrap().set_size(size);
            if force_end_page {
                self.pages.used_size = 0.;
            } else {
                self.pages.used_size += size.height;
            }
        }
        Ok(if done {
            LayoutResult::Done
        } else if manual_break {
            LayoutResult::ManualBreak
        } else {
            LayoutResult::PageFullBreak
        })
    }
    fn judge_put_next_page(
        &self,
        view: Option<&CxfaViewLayoutProcessor>,
        child_height: f32,
        keep: &mut Vec<CxfaContentLayoutItem>,
    ) -> bool {
        let Some(parent) = self.layout_item.as_ref() else {
            return false;
        };
        let mut height = 0.;
        for child in content_children(parent) {
            if exist_container_keep(Some(&child.form_node()), false) {
                height += child.size().height;
                keep.push(child);
            } else {
                keep.clear();
                height = 0.;
            }
        }
        view.is_some_and(|view| view.get_next_avail_content_height(height + child_height))
    }
    fn process_keep_for_split(
        &mut self,
        view: Option<&CxfaViewLayoutProcessor>,
        processor: &mut CxfaContentLayoutProcessor,
        result: LayoutResult,
        align: usize,
        row: &mut FlowRow,
    ) -> Option<LayoutResult> {
        let current = self.pages.current_child.as_ref()?;
        if layout_node_intact(current) == V::None && processor.pages.has_avail_height {
            return None;
        }
        if !exist_container_keep(Some(current), true) {
            return None;
        }
        let size = processor.layout_item.as_ref()?.size();
        let mut keep = Vec::new();
        if self.judge_put_next_page(view, size.height, &mut keep) {
            self.pages.keep_items.clear();
            for item in keep {
                item.remove_self_if_parented();
                row.y -= item.size().height;
                self.pages.keep_items.push(item);
            }
            row.added = true;
            row.force_end_page = true;
            return Some(LayoutResult::PageFullBreak);
        }
        if let Some(item) = processor.extract_layout_item() {
            row.groups[align].push(item);
        }
        row.added = true;
        row.available_width -= size.width;
        row.height = row.height.max(size.height);
        Some(result)
    }
    fn finish_overflow_for_fitting(
        &mut self,
        child_size: &mut CfxSizeF,
        leader: Option<&CxfaNode>,
        trailer: Option<&CxfaNode>,
        trailer_item: Option<&CxfaContentLayoutItem>,
        form: Option<&CxfaNode>,
        added_trailer_height: bool,
    ) -> Result<(), LayoutError> {
        if self.pages.use_inherited {
            self.add_trailer_before_split(child_size.height, trailer_item, false)?;
            if self.judge_leader_or_trailer_for_occur(leader) {
                self.add_pending_node(leader.unwrap().clone(), false);
            }
            self.pages.use_inherited = false;
        } else {
            if added_trailer_height {
                if let Some(item) = trailer_item {
                    child_size.height -= item.size().height;
                }
            }
            self.process_unused_overflow(leader, trailer, trailer_item, form);
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn insert_flowed_item(
        &mut self,
        view: Option<&CxfaViewLayoutProcessor>,
        processor: &mut CxfaContentLayoutProcessor,
        specified: SpecifiedSize,
        flow: V,
        use_break_control: bool,
        mut available_height: f32,
        real_height: f32,
        width_limit: f32,
        row: &mut FlowRow,
        context: &mut LayoutContext<'_>,
        new_row: bool,
    ) -> Result<LayoutResult, LayoutError> {
        let child_node = processor.form_node.clone();
        let takes_space = child_node.presence_requires_space();
        let align = if specified.width_auto {
            0
        } else {
            h_align(
                self.pages
                    .current_child
                    .as_ref()
                    .unwrap_or(&child_node)
                    .get_enum(A::HAlign),
            )
        };
        if flow != V::Rl_tb && align < row.align || flow == V::Rl_tb && align > row.align {
            return Ok(LayoutResult::RowFullBreak);
        }
        row.align = align;
        let own_split = layout_node_intact(&child_node) == V::None;
        let use_real_height = takes_space
            && specified.height_auto
            && own_split
            && child_node
                .parent()
                .is_some_and(|parent| layout_node_intact(&parent) == V::None);
        let mut transfer_height = takes_space;
        if transfer_height && !own_split && matches!(get_layout(&child_node).0, V::Lr_tb | V::Rl_tb)
        {
            transfer_height = false;
        }
        let mut inherited = false;
        let mut own_context;
        let context =
            if let Some(overflow) = view.and_then(|view| view.query_overflow(&self.form_node)) {
                own_context = LayoutContext {
                    overflow_node: Some(overflow),
                    overflow_processor: Some(self.form_node.clone()),
                    ..Default::default()
                };
                &mut own_context
            } else {
                context
            };
        let result = if !new_row || processor.pages.pre_process_result == LayoutResult::Done {
            let result = processor.do_layout_internal(
                view,
                takes_space && use_break_control,
                if use_real_height {
                    real_height - row.y
                } else {
                    f32::MAX
                },
                if transfer_height {
                    real_height - row.y
                } else {
                    f32::MAX
                },
                context,
            )?;
            processor.pages.pre_process_result = result;
            result
        } else {
            let result = processor.pages.pre_process_result;
            processor.pages.pre_process_result = LayoutResult::Done;
            result
        };
        let Some(item) = processor.layout_item.as_ref() else {
            return Ok(result);
        };
        let mut child_size = item.size();
        if use_real_height && real_height < XFA_LAYOUT_PRECISION {
            available_height = f32::MAX;
        }
        if takes_space
            && child_size.width > row.available_width + XFA_LAYOUT_PRECISION
            && width_limit - row.available_width > XFA_LAYOUT_PRECISION
        {
            return Ok(LayoutResult::RowFullBreak);
        }
        let (mut leader, mut trailer, mut form, mut trailer_item) = (None, None, None, None);
        let mut added_trailer_height = false;
        if let Some(view) = view {
            if layout_node_intact(&child_node) == V::None {
                form = view.query_overflow(&child_node);
                if form.is_none() && context.overflow_processor.is_some() {
                    form = context.overflow_node.clone();
                    inherited = true;
                }
                if let Some(data) = view.process_overflow(form.as_ref(), false)? {
                    leader = data.leader;
                    trailer = data.trailer;
                    if processor.judge_leader_or_trailer_for_occur(trailer.as_ref()) {
                        if let Some(node) = trailer.as_ref() {
                            let mut trailer_processor = self.child_processor(node);
                            trailer_processor.do_layout_internal(
                                None,
                                false,
                                f32::MAX,
                                f32::MAX,
                                &mut LayoutContext::default(),
                            )?;
                            trailer_item = trailer_processor.extract_layout_item();
                        }
                        added_trailer_height = if inherited {
                            self.is_add_new_row_for_trailer(trailer_item.as_ref())
                        } else {
                            processor.is_add_new_row_for_trailer(trailer_item.as_ref())
                        };
                        if added_trailer_height {
                            child_size.height += trailer_item.as_ref().unwrap().size().height;
                        }
                    }
                }
            }
        }
        if !takes_space
            || row.y + child_size.height <= available_height + XFA_LAYOUT_PRECISION
            || !specified.height_auto
                && self.pages.used_size + available_height + XFA_LAYOUT_PRECISION
                    >= specified.size.height
        {
            if !takes_space || result == LayoutResult::Done {
                processor.finish_overflow_for_fitting(
                    &mut child_size,
                    leader.as_ref(),
                    trailer.as_ref(),
                    trailer_item.as_ref(),
                    form.as_ref(),
                    added_trailer_height,
                )?;
                let item = processor.extract_layout_item();
                if exist_container_keep(Some(&child_node), false)
                    && layout_node_intact(&child_node) == V::None
                {
                    if let Some(item) = item.as_ref() {
                        self.pages.keep_items.push(item.clone());
                    }
                } else {
                    self.pages.keep_items.clear();
                }
                if let Some(item) = item {
                    row.groups[align].push(item);
                }
                row.added = true;
                if takes_space {
                    row.available_width -= child_size.width;
                    row.height = row.height.max(child_size.height);
                }
                return Ok(LayoutResult::Done);
            }
            if result == LayoutResult::PageFullBreak {
                processor.finish_overflow_for_fitting(
                    &mut child_size,
                    leader.as_ref(),
                    trailer.as_ref(),
                    trailer_item.as_ref(),
                    form.as_ref(),
                    added_trailer_height,
                )?;
            }
            if let Some(item) = processor.extract_layout_item() {
                row.groups[align].push(item);
            }
            row.added = true;
            row.available_width -= child_size.width;
            row.height = row.height.max(child_size.height);
            return Ok(result);
        }
        if let Some(result) = self.process_keep_for_split(view, processor, result, align, row) {
            return Ok(result);
        }
        row.force_end_page = true;
        let split = processor.find_split_pos(available_height - row.y)?;
        if split > XFA_LAYOUT_PRECISION {
            let layout = child_node.get_enum(A::Layout);
            if layout == V::Tb && result == LayoutResult::Done {
                processor.process_unused_overflow(
                    leader.as_ref(),
                    trailer.as_ref(),
                    trailer_item.as_ref(),
                    form.as_ref(),
                );
                if let Some(item) = processor.extract_layout_item() {
                    row.groups[align].push(item);
                }
                row.added = true;
                if takes_space {
                    row.available_width -= child_size.width;
                    row.height = row.height.max(child_size.height);
                }
                return Ok(LayoutResult::PageFullBreak);
            }
            if let Some(view) = view {
                if !processor.pages.use_inherited && result != LayoutResult::PageFullBreak {
                    view.process_overflow(form.as_ref(), true)?;
                }
            }
            if trailer_item.is_some() && added_trailer_height {
                processor.add_trailer_before_split(split, trailer_item.as_ref(), inherited)?;
            } else {
                processor.split_layout_item(split)?;
            }
            if inherited {
                processor.process_unused_overflow(
                    leader.as_ref(),
                    trailer.as_ref(),
                    trailer_item.as_ref(),
                    form.as_ref(),
                );
                self.pages.use_inherited = true;
            } else {
                let generated_only = processor
                    .layout_item
                    .as_ref()
                    .and_then(|item| item.first_child())
                    .is_some_and(|child| {
                        child.next_sibling().is_none()
                            && child.form_node().is_layout_generated_node()
                    });
                if generated_only {
                    processor.process_unused_overflow(
                        leader.as_ref(),
                        trailer.as_ref(),
                        trailer_item.as_ref(),
                        form.as_ref(),
                    );
                } else if processor.judge_leader_or_trailer_for_occur(leader.as_ref()) {
                    processor.add_pending_node(leader.as_ref().unwrap().clone(), false);
                }
            }
            if processor.has_next_fragment() {
                child_size = processor.layout_item.as_ref().unwrap().size();
                if let Some(item) = processor.extract_layout_item() {
                    row.groups[align].push(item);
                }
                row.added = true;
                if takes_space {
                    row.available_width -= child_size.width;
                    row.height = row.height.max(child_size.height);
                }
            }
            return Ok(LayoutResult::PageFullBreak);
        }
        if row.y <= XFA_LAYOUT_PRECISION {
            child_size = processor.layout_item.as_ref().unwrap().size();
            if view.is_some_and(|view| view.get_next_avail_content_height(child_size.height)) {
                if form.is_none() {
                    form = context.overflow_processor.clone();
                }
                if let Some(view) = view {
                    view.process_overflow(form.as_ref(), true)?;
                }
                if inherited {
                    processor.process_unused_overflow(
                        leader.as_ref(),
                        trailer.as_ref(),
                        trailer_item.as_ref(),
                        form.as_ref(),
                    );
                    self.pages.use_inherited = true;
                }
                return Ok(LayoutResult::PageFullBreak);
            }
            if let Some(item) = processor.extract_layout_item() {
                row.groups[align].push(item);
            }
            row.added = true;
            if takes_space {
                row.available_width -= child_size.width;
                row.height = row.height.max(child_size.height);
            }
            if result == LayoutResult::Done {
                row.force_end_page = false;
            }
            return Ok(result);
        }
        if layout_node_intact(&child_node) == V::None && child_node.get_enum(A::Layout) == V::Tb {
            if let Some(view) = view {
                if let Some(data) = view.process_overflow(form.as_ref(), true)? {
                    leader = data.leader;
                }
            }
            if trailer_item.is_some() {
                processor.add_trailer_before_split(split, trailer_item.as_ref(), false)?;
            }
            if processor.judge_leader_or_trailer_for_occur(leader.as_ref()) {
                processor.add_pending_node(leader.unwrap(), false);
            }
            return Ok(LayoutResult::PageFullBreak);
        }
        if result != LayoutResult::Done {
            return Ok(LayoutResult::PageFullBreak);
        }
        if form.is_none() {
            form = context.overflow_processor.clone();
        }
        if let Some(view) = view {
            view.process_overflow(form.as_ref(), true)?;
        }
        if inherited {
            processor.process_unused_overflow(
                leader.as_ref(),
                trailer.as_ref(),
                trailer_item.as_ref(),
                form.as_ref(),
            );
            self.pages.use_inherited = true;
        }
        Ok(LayoutResult::PageFullBreak)
    }
}

/// The native node GetIntact/GetIntactFromKeep decisions, including explicit
/// older-template semantics. Widget split metrics remain actual host work.
fn layout_node_intact(node: &CxfaNode) -> V {
    let layout = node.try_enum(A::Layout, true).unwrap_or(V::Position);
    let version = node.document().map_or(0, |doc| doc.get_cur_version_mode());
    if let Some(keep) = node.get_first_child_by_class(E::Keep) {
        if let Some(intact) = keep.try_enum(A::Intact, false) {
            if intact != V::None || layout != V::Row || version >= 208 {
                return intact;
            }
            if let Some(previous) = node
                .get_prev_container_sibling()
                .filter(|node| node.get_enum(A::Layout) == V::Row)
            {
                let previous_keep = keep.try_enum(A::Previous, false);
                let next_keep = previous
                    .get_first_child_by_class(E::Keep)
                    .and_then(|keep| keep.try_enum(A::Next, false));
                if [previous_keep, next_keep]
                    .into_iter()
                    .flatten()
                    .any(|value| matches!(value, V::ContentArea | V::PageArea))
                {
                    return V::ContentArea;
                }
            }
            return intact;
        }
    }
    match node.get_element_type() {
        E::Subform => {
            if matches!(layout, V::Position | V::Row) {
                V::ContentArea
            } else {
                V::None
            }
        }
        E::Field => {
            let Some(parent) = node.parent() else {
                return V::ContentArea;
            };
            if parent.get_element_type() == E::PageArea || layout_node_intact(&parent) != V::None {
                return V::ContentArea;
            }
            let parent_layout = parent.try_enum(A::Layout, true).unwrap_or(V::Position);
            if matches!(parent_layout, V::Position | V::Row | V::Table) {
                return V::None;
            }
            if parent_layout == V::Tb && version < 208 && node.try_measure(A::H, false).is_some() {
                V::ContentArea
            } else {
                V::None
            }
        }
        E::Draw => V::ContentArea,
        _ => V::None,
    }
}
impl CxfaNode {
    pub fn get_intact(&self) -> V {
        layout_node_intact(self)
    }
}
impl CxfaContentLayoutProcessor {
    fn do_layout_positioned_internal(
        &mut self,
        view: Option<&CxfaViewLayoutProcessor>,
        context: &mut LayoutContext<'_>,
    ) -> Result<(), LayoutError> {
        if self.layout_item.is_some() {
            return Ok(());
        }
        let node = self.form_node.clone();
        let item = self.create_content_layout_item(&node);
        self.layout_item = Some(item.clone());
        let ignore_xy = node.try_enum(A::Layout, true).unwrap_or(V::Position) != V::Position;
        let specified = calculate_container_specified_size(&node);
        let (mut visible, mut hidden) = (CfxSizeF::default(), CfxSizeF::default());
        let mut column = 0usize;
        if self.pages.current_child.is_none() {
            self.goto_next_container_node_simple();
        }
        while let Some(child) = self.pages.current_child.clone() {
            if self.stage == LayoutStage::Container && child.get_element_type() != E::Variables {
                let mut processor = self.child_processor(&child);
                if let Some(columns) = context.columns {
                    let span = child.get_integer(A::ColSpan);
                    if span
                        <= i32::try_from(columns.len().saturating_sub(column)).unwrap_or(i32::MAX)
                    {
                        let span = if span == -1 {
                            columns.len().saturating_sub(column)
                        } else {
                            span.max(0) as usize
                        };
                        let width = columns[column..column + span].iter().sum::<f32>();
                        context.column_width = (width != 0.).then_some(width);
                        column += span;
                    }
                }
                processor.do_layout_internal(view, false, f32::MAX, f32::MAX, context)?;
                if let Some(child_item) = processor.extract_layout_item() {
                    let size = child_item.size();
                    let position = if ignore_xy {
                        CfxPointF::default()
                    } else {
                        calculate_positioned_container_pos(&child, size)
                    };
                    child_item.set_position(position);
                    if child.presence_requires_space() {
                        if specified.width_auto {
                            visible.width = visible.width.max(position.x + size.width);
                        }
                        if specified.height_auto {
                            visible.height = visible.height.max(position.y + size.height);
                        }
                    } else if child.get_element_type() != E::Subform {
                        if specified.width_auto {
                            hidden.width = hidden.width.max(position.x + size.width);
                        }
                        if specified.height_auto {
                            hidden.height = hidden.height.max(position.y + size.height);
                        }
                    }
                    item.append_last_child(&child_item);
                }
            }
            self.goto_next_container_node_simple();
        }
        if node
            .document()
            .is_some_and(|doc| doc.get_cur_version_mode() < 207)
        {
            if visible.width == 0. {
                visible.width = hidden.width;
            }
            if visible.height == 0. {
                visible.height = hidden.height;
            }
        }
        item.set_size(calculate_container_component_size_from_content_size(
            &node, specified, visible,
        ));
        Ok(())
    }
    fn do_layout_table_internal(
        &mut self,
        view: Option<&CxfaViewLayoutProcessor>,
        layout_node: &CxfaNode,
    ) -> Result<(), LayoutError> {
        if self.layout_item.is_some() {
            return Ok(());
        }
        let node = self.form_node.clone();
        let item = self.create_content_layout_item(&node);
        self.layout_item = Some(item.clone());
        let specified = calculate_container_specified_size(&node);
        let inset = get_margin_inset(node.get_first_child_by_class(E::Margin).as_ref());
        let limit = if specified.width_auto {
            f32::MAX
        } else {
            specified.size.width - inset.left - inset.right
        };
        let columns: Vec<f32> = layout_node
            .get_cdata(A::ColumnWidths)
            .split(|&ch| ch == u32::from(b' '))
            .filter(|part| !part.is_empty())
            .map(|part| CxfaMeasurement::from_string(part).to_unit(XfaUnit::Pt))
            .collect();
        let specified_count = columns.len();
        let mut context = LayoutContext {
            columns: (specified_count > 0).then_some(columns.as_slice()),
            ..Default::default()
        };
        if self.pages.current_child.is_none() {
            self.goto_next_container_node_simple();
        }
        while let Some(child) = self.pages.current_child.clone() {
            context.column_width = None;
            if self.stage == LayoutStage::Container {
                let mut processor = self.child_processor(&child);
                processor.do_layout_internal(view, false, f32::MAX, f32::MAX, &mut context)?;
                if let Some(child) = processor.extract_layout_item() {
                    item.append_last_child(&child);
                }
            }
            self.goto_next_container_node_simple();
        }
        self.specified_column_widths = columns;
        self.finish_table_rows(item, node, specified, limit, specified_count)
    }
    fn do_layout_page_area_internal(&self, page: &CxfaViewLayoutItem) -> Result<(), LayoutError> {
        let node = page.form_node();
        let mut walker = self.child_processor(&node);
        let (mut stage, mut action) = (LayoutStage::None, None);
        let mut previous = None;
        loop {
            (stage, action) = walker.goto_next_container_node(stage, &node, action);
            let Some(child) = action.as_ref() else {
                break;
            };
            if stage != LayoutStage::Container || child.get_element_type() == E::Variables {
                continue;
            }
            let mut processor = self.child_processor(child);
            processor.do_layout_internal(
                None,
                false,
                f32::MAX,
                f32::MAX,
                &mut LayoutContext::default(),
            )?;
            if let Some(item) = processor.extract_layout_item() {
                item.set_position(calculate_positioned_container_pos(child, item.size()));
                page.insert_after(&item, previous.as_ref());
                previous = Some((*item).clone());
            }
        }
        let draws: Vec<_> = page
            .children()
            .into_iter()
            .filter(|item| {
                item.is_content_layout_item() && item.form_node().get_element_type() == E::Draw
            })
            .collect();
        let mut previous = None;
        for item in draws {
            page.insert_after(&item, previous.as_ref());
            previous = Some(item);
        }
        Ok(())
    }
}
fn find_break_node(mut node: Option<CxfaNode>, before: bool) -> Option<CxfaNode> {
    while let Some(current) = node {
        let element = current.get_element_type();
        if element
            == if before {
                E::BreakBefore
            } else {
                E::BreakAfter
            }
            || element == E::Break
                && current.get_enum(if before { A::Before } else { A::After }) != V::Auto
        {
            return Some(current);
        }
        node = current.next_sibling();
    }
    None
}
fn find_bookend(parent: &CxfaNode, current: Option<&CxfaNode>) -> Option<CxfaNode> {
    let mut next = current.map_or_else(|| parent.first_child(), CxfaNode::next_sibling);
    while let Some(node) = next {
        if matches!(node.get_element_type(), E::Bookend | E::Break) {
            return Some(node);
        }
        next = node.next_sibling();
    }
    None
}
impl CxfaContentLayoutProcessor {
    pub fn do_layout(
        &mut self,
        view: &CxfaViewLayoutProcessor,
        use_break_control: bool,
        height_limit: f32,
        real_height: f32,
    ) -> Result<LayoutResult, LayoutError> {
        self.do_layout_internal(
            Some(view),
            use_break_control,
            height_limit,
            real_height,
            &mut LayoutContext::default(),
        )
    }
    fn do_layout_internal(
        &mut self,
        view: Option<&CxfaViewLayoutProcessor>,
        use_break_control: bool,
        height_limit: f32,
        real_height: f32,
        context: &mut LayoutContext<'_>,
    ) -> Result<LayoutResult, LayoutError> {
        match self.form_node.get_element_type() {
            E::Subform | E::Area | E::ExclGroup | E::SubformSet => {
                let layout_node = get_subform_set_parent(&self.form_node);
                let (layout, force_tb) = get_layout(&layout_node);
                match layout {
                    V::Tb | V::Lr_tb | V::Rl_tb => {
                        return self.do_layout_flowed_container(
                            view,
                            use_break_control,
                            layout,
                            height_limit,
                            real_height,
                            context,
                            force_tb,
                        )
                    }
                    V::Table => self.do_layout_table_internal(view, &layout_node)?,
                    _ => self.do_layout_positioned_internal(view, context)?,
                }
                self.stage = LayoutStage::Done;
                Ok(LayoutResult::Done)
            }
            E::Draw | E::Field => {
                self.do_layout_field()?;
                self.stage = LayoutStage::Done;
                Ok(LayoutResult::Done)
            }
            _ => Ok(LayoutResult::Done),
        }
    }
    fn do_layout_field(&mut self) -> Result<(), LayoutError> {
        if self.layout_item.is_some() {
            return Ok(());
        }
        let node = self.form_node.clone();
        let host = self
            .host
            .clone()
            .ok_or(LayoutError::MissingFieldMeasurementHost)?;
        let item = self.create_content_layout_item(&node);
        let mut size = host.start_field_draw_layout(&node, CfxSizeF::new(-1., -1.))?;
        if matches!(map_rotation(node.get_integer(A::Rotate)), 90 | 270) {
            std::mem::swap(&mut size.width, &mut size.height);
        }
        item.set_size(size);
        self.layout_item = Some(item);
        Ok(())
    }
    fn process_keep_nodes_end(&mut self) {
        self.pages.keep_break_finish = true;
        self.pages.keep_head = None;
        self.pages.keep_tail = None;
        self.pages.is_process_keep = false;
    }
    fn delete_layout_generated_node(&self, node: &CxfaNode) {
        let mut retired = Vec::new();
        let mut iter = CxfaNodeIteratorTemplate::<CxfaNode, CxfaTraverseStrategyXfaNode>::new(
            Some(node.clone()),
        );
        let mut current = iter.get_current();
        while let Some(node) = current {
            current = iter.move_to_next();
            let mut item = node
                .get_layout_item()
                .and_then(|item| item.as_content_layout_item());
            while let Some(content) = item {
                item = content.next();
                if let Some(notify) = self.store.notify() {
                    notify.on_layout_item_removing(&content);
                }
                retired.push(content);
            }
        }
        // Native removal notifications run while the old tree is intact.
        // Our arena retains allocations, so explicitly retire every fragment
        // afterwards; stale overflow decorations must not remain paintable
        // or consume height on subsequent layout passes.
        for content in retired.into_iter().rev() {
            content.retire_from_layout();
        }
        if let Some(parent) = node.parent() {
            parent.remove_child_and_notify(node, true);
        }
    }
    fn process_keep_nodes_for_check_next(
        &mut self,
        action: &mut Option<CxfaNode>,
        next: &mut CxfaNode,
        last_keep: &mut bool,
    ) -> Option<LayoutStage> {
        let can_split = layout_node_intact(next) == V::None;
        let next_keep = exist_container_keep(Some(next), false);
        if next_keep && !can_split {
            if !self.pages.is_process_keep && !self.pages.keep_break_finish {
                self.pages.keep_head = Some(next.clone());
                self.pages.is_process_keep = true;
            }
            return None;
        }
        if !self.pages.is_process_keep || self.pages.keep_head.is_none() {
            if self.pages.keep_break_finish {
                *last_keep = true;
            }
            self.pages.keep_break_finish = false;
            return None;
        }
        self.pages.keep_tail = Some(next.clone());
        if self.pages.keep_break_finish {
            *next = self.pages.keep_head.clone().unwrap();
            self.process_keep_nodes_end();
            return None;
        }
        if let Some(node) = find_break_node(next.first_child(), true) {
            *action = Some(node);
            return Some(LayoutStage::BreakBefore);
        }
        *next = self.pages.keep_head.clone().unwrap();
        self.process_keep_nodes_end();
        None
    }
    fn handle_break_before(
        &mut self,
        container: Option<&CxfaNode>,
        action: &mut Option<CxfaNode>,
    ) -> Option<LayoutStage> {
        let current = action.clone()?;
        if !self.pages.keep_break_finish {
            if let Some(node) = find_break_node(current.next_sibling(), true) {
                *action = Some(node);
                return Some(LayoutStage::BreakBefore);
            }
        }
        if self.pages.is_process_keep {
            let container = container?;
            if self.pages.keep_tail.as_ref() == Some(container) {
                *action = self.pages.keep_head.clone();
                self.process_keep_nodes_end();
                return Some(LayoutStage::Container);
            }
            if let Some(node) = find_break_node(container.first_child(), false) {
                *action = Some(node);
                return Some(LayoutStage::BreakAfter);
            }
            return None;
        }
        *action = container.cloned();
        Some(LayoutStage::Container)
    }
    fn handle_break_after(
        &self,
        container: Option<&CxfaNode>,
        action: &mut Option<CxfaNode>,
    ) -> Option<LayoutStage> {
        let start = if let Some(current) = action.as_ref() {
            current.next_sibling()
        } else {
            if self.pages.keep_break_finish {
                return None;
            }
            container?.first_child()
        };
        let node = find_break_node(start, false)?;
        *action = Some(node);
        Some(LayoutStage::BreakAfter)
    }
    fn handle_check_next_child_container(
        &mut self,
        parent: &CxfaNode,
        container: Option<&CxfaNode>,
        action: &mut Option<CxfaNode>,
    ) -> Option<LayoutStage> {
        let mut next = container.map_or_else(
            || parent.get_first_container_child(),
            CxfaNode::get_next_container_sibling,
        );
        while next
            .as_ref()
            .is_some_and(CxfaNode::is_layout_generated_node)
        {
            let generated = next.take().unwrap();
            next = generated.get_next_container_sibling();
            if generated.is_unused_node() {
                self.delete_layout_generated_node(&generated);
            }
        }
        let mut next = next?;
        let mut last_keep = false;
        if let Some(stage) =
            self.process_keep_nodes_for_check_next(action, &mut next, &mut last_keep)
        {
            return Some(stage);
        }
        if !self.pages.keep_break_finish && !last_keep {
            if let Some(node) = find_break_node(next.first_child(), true) {
                *action = Some(node);
                return Some(LayoutStage::BreakBefore);
            }
        }
        *action = Some(next);
        Some(if self.pages.is_process_keep {
            LayoutStage::Keep
        } else {
            LayoutStage::Container
        })
    }
    fn goto_next_container_node(
        &mut self,
        stage: LayoutStage,
        parent: &CxfaNode,
        mut action: Option<CxfaNode>,
    ) -> (LayoutStage, Option<CxfaNode>) {
        let container = match stage {
            LayoutStage::BreakBefore | LayoutStage::BreakAfter => {
                action.as_ref().and_then(CxfaNode::parent)
            }
            LayoutStage::Keep | LayoutStage::Container => action.clone(),
            _ => None,
        };
        let found = match stage {
            LayoutStage::Keep => {
                if self.pages.keep_break_finish {
                    None
                } else {
                    find_break_node(container.as_ref().and_then(CxfaNode::first_child), false).map(
                        |node| {
                            action = Some(node);
                            LayoutStage::BreakAfter
                        },
                    )
                }
            }
            LayoutStage::None | LayoutStage::BookendLeader => {
                if stage == LayoutStage::None {
                    action = None;
                }
                if let Some(node) = find_bookend(parent, action.as_ref()) {
                    action = Some(node);
                    Some(LayoutStage::BookendLeader)
                } else {
                    action = None;
                    self.handle_break_before(container.as_ref(), &mut action)
                }
            }
            LayoutStage::BreakBefore => self.handle_break_before(container.as_ref(), &mut action),
            LayoutStage::Container | LayoutStage::BreakAfter => {
                if stage == LayoutStage::Container {
                    action = None;
                }
                self.handle_break_after(container.as_ref(), &mut action)
            }
            LayoutStage::BookendTrailer => {
                return if let Some(node) = find_bookend(parent, action.as_ref()) {
                    (LayoutStage::BookendTrailer, Some(node))
                } else {
                    (LayoutStage::Done, None)
                }
            }
            _ => return (LayoutStage::Done, None),
        };
        if let Some(stage) = found {
            return (stage, action);
        }
        if let Some(stage) =
            self.handle_check_next_child_container(parent, container.as_ref(), &mut action)
        {
            return (stage, action);
        }
        if let Some(node) = find_bookend(parent, None) {
            (LayoutStage::BookendTrailer, Some(node))
        } else {
            (LayoutStage::Done, None)
        }
    }
    fn goto_next_container_node_simple(&mut self) {
        let parent = self.form_node.clone();
        let current = self.pages.current_child.take();
        let (stage, action) = self.goto_next_container_node(self.stage, &parent, current);
        self.stage = stage;
        self.pages.current_child = action;
    }
    fn adjust_container_specified_size(
        &self,
        context: &LayoutContext<'_>,
        specified: &mut SpecifiedSize,
    ) {
        if let Some(width) = context.column_width {
            specified.size.width = width;
            specified.width_auto = false;
        }
        if specified.height_auto {
            return;
        }
        specified.size.height -= self.pages.used_size;
        if self
            .form_node
            .parent()
            .is_some_and(|parent| get_layout(&parent).0 == V::Row)
            && self
                .form_node
                .get_first_container_child()
                .and_then(|child| child.get_next_container_sibling())
                .is_some()
        {
            specified.size.height = 0.;
            specified.height_auto = true;
        }
    }
    pub fn find_split_pos(&self, mut proposed: f32) -> Result<f32, LayoutError> {
        let Some(item) = self.layout_item.as_ref() else {
            return Ok(proposed);
        };
        let calculate_margin = self
            .form_node
            .try_enum(A::Layout, true)
            .unwrap_or(V::Position)
            != V::Position;
        while proposed > XFA_LAYOUT_PRECISION {
            let old = proposed;
            let mut app_change = false;
            if !self.find_layout_item_split_pos(
                item,
                0.,
                &mut proposed,
                &mut app_change,
                calculate_margin,
            )? {
                break;
            }
            if proposed >= old {
                break;
            }
        }
        Ok(proposed)
    }
    fn find_layout_item_split_pos(
        &self,
        item: &CxfaContentLayoutItem,
        offset: f32,
        proposed: &mut f32,
        app_change: &mut bool,
        calculate_margin: bool,
    ) -> Result<bool, LayoutError> {
        if *proposed <= offset + XFA_LAYOUT_PRECISION
            || *proposed > offset + item.size().height - XFA_LAYOUT_PRECISION
        {
            return Ok(false);
        }
        match layout_node_intact(&item.form_node()) {
            V::None => {
                let inset = if calculate_margin {
                    get_margin_inset(
                        item.form_node()
                            .get_first_child_by_class(E::Margin)
                            .as_ref(),
                    )
                } else {
                    MarginInset::default()
                };
                let mut any_changed = false;
                loop {
                    let mut changed = false;
                    if matches!(item.form_node().get_element_type(), E::Field | E::Draw) {
                        let host = self
                            .host
                            .as_ref()
                            .ok_or(LayoutError::MissingFieldMeasurementHost)?;
                        if let Some(relative) = host.find_split_pos(
                            &item.form_node(),
                            item.get_index(),
                            *proposed - offset,
                        )? {
                            if !relative.is_finite()
                                || relative < 0.
                                || offset + relative > *proposed + XFA_LAYOUT_PRECISION
                            {
                                return Err(LayoutError::InvalidSplitPosition);
                            }
                            if offset + relative < *proposed {
                                *proposed = offset + relative;
                                *app_change = true;
                                any_changed = true;
                                changed = true;
                                if *proposed <= offset + XFA_LAYOUT_PRECISION {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                    let mut relative = *proposed - inset.bottom;
                    for child in content_children(item) {
                        let child_offset = offset + inset.top + child.position().y;
                        let mut child_changed = false;
                        if self.find_layout_item_split_pos(
                            &child,
                            child_offset,
                            &mut relative,
                            &mut child_changed,
                            calculate_margin,
                        )? {
                            *proposed = if relative - child_offset < XFA_LAYOUT_PRECISION
                                && child_changed
                            {
                                relative - inset.top
                            } else {
                                relative + inset.bottom
                            };
                            any_changed = true;
                            changed = true;
                            if *proposed <= offset + XFA_LAYOUT_PRECISION {
                                return Ok(true);
                            }
                            break;
                        }
                    }
                    if !changed {
                        break;
                    }
                }
                Ok(any_changed)
            }
            V::ContentArea | V::PageArea => {
                *proposed = offset;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    pub fn split_layout_item(&mut self, split: f32) -> Result<(), LayoutError> {
        let item = self
            .layout_item
            .clone()
            .ok_or(LayoutError::InvalidLayoutTree)?;
        self.split_layout_item_internal(&item, None, split)
    }
    fn split_layout_item_internal(
        &mut self,
        item: &CxfaContentLayoutItem,
        second_parent: Option<&CxfaContentLayoutItem>,
        split: f32,
    ) -> Result<(), LayoutError> {
        if !split.is_finite() || split < 0. || split > item.size().height + XFA_LAYOUT_PRECISION {
            return Err(LayoutError::InvalidSplitPosition);
        }
        let calculate_margin = self
            .form_node
            .try_enum(A::Layout, true)
            .unwrap_or(V::Position)
            != V::Position;
        let inset = if calculate_margin {
            get_margin_inset(
                item.form_node()
                    .get_first_child_by_class(E::Margin)
                    .as_ref(),
            )
        } else {
            MarginInset::default()
        };
        let has_children = item.first_child().is_some();
        let second = if let Some(child) = self
            .pages
            .child_preprocessor
            .as_mut()
            .filter(|child| child.form_node == item.form_node())
        {
            child.create_content_layout_item(&item.form_node())
        } else {
            self.create_content_layout_item(&item.form_node())
        };
        second.set_position(CfxPointF::new(item.position().x, 0.));
        second.set_size(CfxSizeF::new(
            item.size().width,
            item.size().height - split + if has_children { inset.top } else { 0. },
        ));
        item.set_size(CfxSizeF::new(item.size().width, split));
        let mut orphan = false;
        if let Some(parent) = second_parent {
            parent.append_last_child(&second);
            if inset.top > 0. && has_children {
                let mut current = Some(parent.clone());
                while let Some(parent) = current {
                    let size = parent.size();
                    parent.set_size(CfxSizeF::new(size.width, size.height + inset.top));
                    current = parent
                        .parent()
                        .and_then(|item| item.as_content_layout_item());
                }
            }
        } else if let Some(parent) = item.parent() {
            parent.insert_after(&second, Some(item));
        } else {
            orphan = true;
        }
        let mut children = Vec::new();
        while let Some(child) = item
            .first_child()
            .and_then(|item| item.as_content_layout_item())
        {
            child.remove_self_if_parented();
            children.push(child);
        }
        let (mut keep_height, mut add_margin) = (0f32, 0f32);
        let mut keep_items: Vec<CxfaContentLayoutItem> = Vec::new();
        for child in children {
            if split <= inset.top + child.position().y + inset.bottom + XFA_LAYOUT_PRECISION {
                if !exist_container_keep(Some(&child.form_node()), true) {
                    let pos = child.position();
                    child.set_position(CfxPointF::new(
                        pos.x,
                        pos.y - split + inset.bottom + keep_height + add_margin,
                    ));
                    second.append_last_child(&child);
                    continue;
                }
                if keep_height < XFA_LAYOUT_PRECISION {
                    for previous in &keep_items {
                        previous.remove_self_if_parented();
                        let mut position = previous.position();
                        position.y = (position.y - split).max(0.);
                        if position.y + previous.size().height > keep_height {
                            position.y = keep_height;
                            keep_height += previous.size().height;
                            let size = second.size();
                            second.set_size(CfxSizeF::new(
                                size.width,
                                size.height + previous.size().height,
                            ));
                            if let Some(parent) = second_parent {
                                let size = parent.size();
                                parent.set_size(CfxSizeF::new(
                                    size.width,
                                    size.height + previous.size().height,
                                ));
                            }
                        }
                        previous.set_position(position);
                        second.append_last_child(previous);
                    }
                }
                let position = child.position();
                child.set_position(CfxPointF::new(
                    position.x,
                    position.y - split + keep_height + add_margin,
                ));
                second.append_last_child(&child);
                continue;
            }
            if split + XFA_LAYOUT_PRECISION
                >= inset.top + inset.bottom + child.position().y + child.size().height
            {
                item.append_last_child(&child);
                if exist_container_keep(Some(&child.form_node()), false) {
                    keep_items.push(child);
                } else {
                    keep_items.clear();
                }
                continue;
            }
            let old_height = second.size().height;
            self.split_layout_item_internal(
                &child,
                Some(&second),
                split - inset.top - inset.bottom - child.position().y,
            )?;
            add_margin = second.size().height - old_height;
            item.append_last_child(&child);
        }
        if orphan {
            // Detached roots cannot be siblings in TreeNode. Keep the produced
            // root fragment in processor order instead of nesting it as native
            // code does, which mixes the next page into the current subtree.
            self.pages.detached_fragments.push_front(second);
        }
        Ok(())
    }
    fn has_next_fragment(&self) -> bool {
        !self.pages.detached_fragments.is_empty()
            || self
                .layout_item
                .as_ref()
                .is_some_and(|item| item.next_sibling().is_some())
    }
    fn is_add_new_row_for_trailer(&self, item: Option<&CxfaContentLayoutItem>) -> bool {
        item.is_some_and(|item| {
            self.form_node.get_enum(A::Layout) == V::Tb
                || self.pages.width_limit <= item.size().width
        })
    }
    fn update_pending_item_layout(&self, item: &CxfaContentLayoutItem) -> Result<(), LayoutError> {
        if matches!(item.form_node().get_enum(A::Layout), V::Row | V::Rl_row) {
            relocate_table_row_cells(item, &self.specified_column_widths, self.host.as_deref())?;
        }
        Ok(())
    }
    fn add_leader_after_split(
        &mut self,
        leader: &CxfaContentLayoutItem,
    ) -> Result<(), LayoutError> {
        self.update_pending_item_layout(leader)?;
        let item = self
            .layout_item
            .clone()
            .ok_or(LayoutError::InvalidLayoutTree)?;
        let inset = get_margin_inset(self.form_node.get_first_child_by_class(E::Margin).as_ref());
        let height = leader.size().height;
        for child in content_children(&item) {
            let position = child.position();
            child.set_position(CfxPointF::new(position.x, position.y + height));
        }
        let x = match leader.form_node().get_enum(A::HAlign) {
            V::Right => item.size().width - inset.right - leader.size().width,
            V::Center => (item.size().width - inset.left - inset.right - leader.size().width) / 2.,
            _ => inset.left,
        };
        leader.set_position(CfxPointF::new(x, 0.));
        let size = item.size();
        item.set_size(CfxSizeF::new(size.width, size.height + height));
        item.append_last_child(leader);
        Ok(())
    }
    fn add_trailer_before_split(
        &mut self,
        split: f32,
        trailer: Option<&CxfaContentLayoutItem>,
        inherited: bool,
    ) -> Result<(), LayoutError> {
        let Some(trailer) = trailer else {
            return Ok(());
        };
        let height = trailer.size().height;
        if inherited {
            let next = if split - height > XFA_LAYOUT_PRECISION {
                self.find_split_pos(split - height)?
            } else {
                0.
            };
            if next > XFA_LAYOUT_PRECISION {
                self.split_layout_item(next)?;
            }
            return Ok(());
        }
        self.update_pending_item_layout(trailer)?;
        let inset = get_margin_inset(self.form_node.get_first_child_by_class(E::Margin).as_ref());
        if !self.is_add_new_row_for_trailer(Some(trailer)) {
            let item = self
                .layout_item
                .clone()
                .ok_or(LayoutError::InvalidLayoutTree)?;
            trailer.set_position(CfxPointF::new(self.last_row_width, self.last_row_y));
            let size = item.size();
            item.set_size(CfxSizeF::new(
                size.width + trailer.size().width,
                size.height,
            ));
            item.append_last_child(trailer);
            return Ok(());
        }
        let next = if split - height > XFA_LAYOUT_PRECISION {
            self.find_split_pos(split - height)?
        } else {
            0.
        };
        if next > XFA_LAYOUT_PRECISION {
            self.split_layout_item(next)?;
        }
        let y = if next > XFA_LAYOUT_PRECISION {
            next
        } else {
            split
        } - inset.top
            - inset.bottom;
        let item = self
            .layout_item
            .clone()
            .ok_or(LayoutError::InvalidLayoutTree)?;
        let x = match trailer.form_node().get_enum(A::HAlign) {
            V::Right => item.size().width - inset.right - trailer.size().width,
            V::Center => (item.size().width - inset.left - inset.right - trailer.size().width) / 2.,
            _ => inset.left,
        };
        trailer.set_position(CfxPointF::new(x, y));
        let size = item.size();
        item.set_size(CfxSizeF::new(size.width, size.height + height));
        item.append_last_child(trailer);
        Ok(())
    }
    fn add_pending_node(&mut self, node: CxfaNode, break_pending: bool) {
        self.pages.pending_nodes.push_back(node);
        self.pages.break_pending = break_pending;
    }
    fn insert_pending_items(&mut self, node: &CxfaNode) -> Result<f32, LayoutError> {
        if self.pages.pending_nodes.is_empty() {
            return Ok(0.);
        }
        if self.layout_item.is_none() {
            let item = self.create_content_layout_item(node);
            item.set_size(CfxSizeF::default());
            self.layout_item = Some(item);
        }
        let mut height = 0.;
        while let Some(node) = self.pages.pending_nodes.pop_front() {
            let mut processor = self.child_processor(&node);
            processor.do_layout_internal(
                None,
                false,
                f32::MAX,
                f32::MAX,
                &mut LayoutContext::default(),
            )?;
            if let Some(item) = processor.extract_layout_item() {
                self.add_leader_after_split(&item)?;
                if self.pages.break_pending {
                    height += item.size().height;
                }
            }
        }
        Ok(height)
    }
    fn insert_keep_layout_items(&mut self) -> Result<f32, LayoutError> {
        if self.pages.keep_items.is_empty() {
            return Ok(0.);
        }
        if self.layout_item.is_none() {
            let node = self.form_node.clone();
            let item = self.create_content_layout_item(&node);
            item.set_size(CfxSizeF::default());
            self.layout_item = Some(item);
        }
        let items = std::mem::take(&mut self.pages.keep_items);
        let mut height = 0.;
        for item in items.iter().rev() {
            self.add_leader_after_split(item)?;
            height += item.size().height;
        }
        Ok(height)
    }
    fn process_unused_binds(node: Option<&CxfaNode>) {
        let Some(node) = node else {
            return;
        };
        let mut iterator = CxfaNodeIteratorTemplate::<CxfaNode, CxfaTraverseStrategyXfaNode>::new(
            Some(node.clone()),
        );
        let mut current = iterator.move_to_next();
        while let Some(node) = current {
            current = iterator.move_to_next();
            if node.is_container_node() {
                if let Some(data) = node.get_bind_data() {
                    data.remove_bind_item(&node);
                    node.set_binding_node(None);
                }
            }
            node.set_flag(XfaNodeFlag::UnusedNode);
        }
    }
    fn process_unused_overflow(
        &self,
        leader: Option<&CxfaNode>,
        trailer: Option<&CxfaNode>,
        trailer_item: Option<&CxfaContentLayoutItem>,
        form: Option<&CxfaNode>,
    ) {
        Self::process_unused_binds(leader);
        Self::process_unused_binds(trailer);
        let Some(form) = form else {
            return;
        };
        let form = if matches!(form.get_element_type(), E::Overflow | E::Break) {
            form.parent()
        } else {
            Some(form.clone())
        };
        if let Some(form) = form {
            for node in [leader, trailer].into_iter().flatten() {
                if node.parent().as_ref() == Some(&form) {
                    form.remove_child_and_notify(node, true);
                }
            }
        }
        if let Some(item) = trailer_item {
            super::cxfa_layoutitem::release_layout_item(item);
        }
    }
    fn find_last_content_layout_item(&self, flow: V) -> Option<CxfaContentLayoutItem> {
        if self.stage == LayoutStage::Done || flow == V::Tb {
            return None;
        }
        let mut last = self
            .layout_item
            .as_ref()?
            .first_child()?
            .as_content_layout_item()?;
        let mut next = Some((*last).clone());
        while let Some(item) = next {
            next = item.next_sibling();
            if let Some(content) = item.as_content_layout_item() {
                if content.position().y != last.position().y {
                    last = content;
                }
            }
        }
        Some(last)
    }
    fn calculate_layout_item_size(&self, last: Option<&CxfaContentLayoutItem>) -> CfxSizeF {
        let mut size = CfxSizeF::default();
        if let Some(item) = self.layout_item.as_ref() {
            for child in item.children() {
                if Some(&child) == last.map(|item| &**item) {
                    break;
                }
                let Some(content) = child.as_content_layout_item() else {
                    continue;
                };
                if !content.form_node().presence_requires_space() {
                    continue;
                }
                let position = content.position();
                size.width = size.width.max(position.x + content.size().width);
                size.height = size.height.max(position.y + content.size().height);
            }
        }
        size
    }
}

impl CxfaContentLayoutProcessor {
    fn finish_table_rows(
        &mut self,
        item: CxfaContentLayoutItem,
        node: CxfaNode,
        specified: SpecifiedSize,
        limit: f32,
        specified_count: usize,
    ) -> Result<(), LayoutError> {
        let mut rows: Vec<(Option<CxfaContentLayoutItem>, i32, f32)> = content_children(&item)
            .filter(|row| {
                row.form_node().get_element_type() == E::Subform
                    && row.form_node().presence_requires_space()
                    && matches!(row.form_node().get_enum(A::Layout), V::Row | V::Rl_row)
            })
            .filter_map(|row| {
                let first = row.first_child()?.as_content_layout_item()?;
                let span = first.form_node().get_integer(A::ColSpan);
                let width = first.size().width;
                Some((Some(first), span, width))
            })
            .collect();
        let mut column = 0usize;
        loop {
            let (mut more, mut auto) = (false, false);
            for (cell, span, width) in &mut rows {
                while cell
                    .as_ref()
                    .is_some_and(|cell| *span <= 0 || !cell.form_node().presence_requires_space())
                {
                    let old = cell.take().unwrap();
                    *cell = if *span < 0 && old.form_node().presence_requires_space() {
                        None
                    } else {
                        old.next_sibling()
                            .and_then(|next| next.as_content_layout_item())
                    };
                    *span = cell
                        .as_ref()
                        .map_or(0, |cell| cell.form_node().get_integer(A::ColSpan));
                    *width = cell.as_ref().map_or(0., |cell| cell.size().width);
                }
                if cell.is_none() {
                    continue;
                }
                more = true;
                if *span != 1 {
                    continue;
                }
                if column >= specified_count && self.specified_column_widths.len() <= column {
                    self.specified_column_widths.resize(column + 1, 0.);
                }
                if self.specified_column_widths[column] < XFA_LAYOUT_PRECISION {
                    auto = true;
                }
                if auto {
                    self.specified_column_widths[column] =
                        self.specified_column_widths[column].max(*width);
                }
            }
            if !more {
                break;
            }
            let width = self
                .specified_column_widths
                .get(column)
                .copied()
                .unwrap_or(0.);
            for (cell, span, remaining) in &mut rows {
                if cell.is_some() {
                    *span -= 1;
                    *remaining -= width;
                }
            }
            column += 1;
        }
        let (mut y, mut width) = (0f32, 0f32);
        for row in content_children(&item) {
            let row_node = row.form_node();
            if !row_node.presence_requires_space() {
                continue;
            }
            if row_node.get_element_type() == E::Subform
                && matches!(row_node.get_enum(A::Layout), V::Row | V::Rl_row)
            {
                relocate_table_row_cells(
                    &row,
                    &self.specified_column_widths,
                    self.host.as_deref(),
                )?;
            }
            let x = if specified.width_auto {
                0.
            } else {
                match row_node.get_enum(A::HAlign) {
                    V::Center => (limit - row.size().width) / 2.,
                    V::Right => limit - row.size().width,
                    _ => 0.,
                }
            };
            row.set_position(CfxPointF::new(x, y));
            if specified.width_auto {
                let supplied = x + row.size().width;
                width = width.max(if limit < f32::MAX {
                    supplied.max(limit)
                } else {
                    supplied
                });
            }
            y += row.size().height;
        }
        item.set_size(calculate_container_component_size_from_content_size(
            &node,
            specified,
            CfxSizeF::new(width, if specified.height_auto { y } else { 0. }),
        ));
        Ok(())
    }
}
