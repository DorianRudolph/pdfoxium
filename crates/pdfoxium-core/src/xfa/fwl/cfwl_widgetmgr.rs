// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Ordered FWL widget ownership/tree. All draw, delegate and host calls occur
//! after the tree borrow is released, including removal/reparenting callbacks.
use super::{
    cfwl_app::CfwlApp,
    cfwl_message::FwlMessage,
    cfwl_widget::{dispatch_draw_to, FwlType, WidgetId, WidgetRef, WidgetState},
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
};

pub trait WidgetMgrAdapter {
    fn repaint_widget(&self, widget: &WidgetRef);
    fn get_popup_pos(
        &self,
        widget: &WidgetRef,
        min_height: f32,
        max_height: f32,
        anchor: CfxRectF,
        popup_rect: &mut CfxRectF,
    ) -> bool;
}
struct Item {
    widget: Option<WidgetRef>,
    parent: Option<WidgetId>,
    children: Vec<WidgetId>,
}
pub struct CfwlWidgetMgr {
    adapter: Rc<dyn WidgetMgrAdapter>,
    app: Weak<CfwlApp>,
    items: RefCell<BTreeMap<WidgetId, Item>>,
}
impl CfwlWidgetMgr {
    pub fn new(adapter: Rc<dyn WidgetMgrAdapter>, app: Weak<CfwlApp>) -> Self {
        Self {
            adapter,
            app,
            items: RefCell::new(BTreeMap::from([(
                0,
                Item {
                    widget: None,
                    parent: None,
                    children: Vec::new(),
                },
            )])),
        }
    }
    pub fn get_widget(&self, id: WidgetId) -> Option<WidgetRef> {
        self.items.borrow().get(&id)?.widget.clone()
    }
    pub fn get_parent_widget(&self, id: WidgetId) -> Option<WidgetRef> {
        let items = self.items.borrow();
        let parent = items.get(&id)?.parent?;
        items.get(&parent)?.widget.clone()
    }
    fn sibling(&self, id: WidgetId, next: bool) -> Option<WidgetRef> {
        let items = self.items.borrow();
        let parent = items.get(&id)?.parent?;
        let children = &items.get(&parent)?.children;
        let index = children.iter().position(|&child| child == id)?;
        let index = if next {
            index.checked_add(1)?
        } else {
            index.checked_sub(1)?
        };
        items.get(children.get(index)?)?.widget.clone()
    }
    pub fn get_prior_sibling_widget(&self, id: WidgetId) -> Option<WidgetRef> {
        self.sibling(id, false)
    }
    pub fn get_next_sibling_widget(&self, id: WidgetId) -> Option<WidgetRef> {
        self.sibling(id, true)
    }
    pub fn get_first_child_widget(&self, parent: Option<WidgetId>) -> Option<WidgetRef> {
        let items = self.items.borrow();
        let child = items.get(&parent.unwrap_or(0))?.children.first()?;
        items.get(child)?.widget.clone()
    }
    pub fn get_last_child_widget(&self, parent: Option<WidgetId>) -> Option<WidgetRef> {
        let items = self.items.borrow();
        let child = items.get(&parent.unwrap_or(0))?.children.last()?;
        items.get(child)?.widget.clone()
    }
    pub fn children(&self, parent: Option<WidgetId>) -> Vec<WidgetRef> {
        let items = self.items.borrow();
        let Some(item) = items.get(&parent.unwrap_or(0)) else {
            return Vec::new();
        };
        item.children
            .iter()
            .filter_map(|id| items.get(id)?.widget.clone())
            .collect()
    }
    pub fn descendants(&self, parent: WidgetId) -> Vec<WidgetRef> {
        let items = self.items.borrow();
        let mut result = Vec::new();
        let mut stack = items
            .get(&parent)
            .map(|item| item.children.iter().rev().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        while let Some(id) = stack.pop() {
            if let Some(item) = items.get(&id) {
                if let Some(widget) = &item.widget {
                    result.push(widget.clone());
                }
                stack.extend(item.children.iter().rev().copied());
            }
        }
        result
    }
    pub fn insert_widget(&self, parent: Option<WidgetId>, child: WidgetRef) -> bool {
        let parent = parent.unwrap_or(0);
        let id = child.base().id();
        if child.base().is_destroyed() || parent == id {
            return false;
        }
        if let Some(app) = self.app.upgrade() {
            if !child
                .base()
                .app()
                .is_some_and(|owner| Rc::ptr_eq(&owner, &app))
            {
                return false;
            }
        } else {
            return false;
        }
        let mut items = self.items.borrow_mut();
        if !items.contains_key(&parent) {
            return false;
        }
        let mut ancestor = Some(parent);
        while let Some(current) = ancestor {
            if current == id {
                return false;
            }
            ancestor = items.get(&current).and_then(|item| item.parent);
        }
        if let Some(previous) = items.get(&id).and_then(|item| item.parent) {
            if let Some(item) = items.get_mut(&previous) {
                item.children.retain(|&child| child != id);
            }
        }
        let item = items.entry(id).or_insert_with(|| Item {
            widget: Some(child),
            parent: None,
            children: Vec::new(),
        });
        item.parent = Some(parent);
        items.get_mut(&parent).unwrap().children.push(id);
        true
    }
    pub fn remove_widget(&self, id: WidgetId) {
        if id == 0 {
            return;
        }
        // Remove links before invoking destructors/callbacks. A reentrant host
        // sees the final tree and can remove/reparent unrelated widgets safely.
        let removed = {
            let mut items = self.items.borrow_mut();
            let Some(item) = items.get(&id) else {
                return;
            };
            let parent = item.parent;
            if let Some(parent) = parent {
                if let Some(item) = items.get_mut(&parent) {
                    item.children.retain(|&child| child != id);
                }
            }
            let mut pending = vec![id];
            let mut removed = Vec::new();
            while let Some(next) = pending.pop() {
                if let Some(item) = items.remove(&next) {
                    pending.extend(item.children);
                    if let Some(widget) = item.widget {
                        removed.push(widget);
                    }
                }
            }
            removed
        };
        for widget in removed.iter().rev() {
            if let Some(app) = self.app.upgrade() {
                app.note_driver().notify_target_destroy(widget);
            }
            if !widget.base().is_finalized() {
                widget.pre_finalize();
            }
        }
    }
    pub fn remove_all_widgets(&self) {
        let roots = self.children(None);
        for widget in roots {
            self.remove_widget(widget.base().id());
        }
    }
    pub fn repaint_widget(&self, widget: &WidgetRef, _rect: CfxRectF) {
        let mut native = widget.clone();
        while let Some(outer) = native.base().outer() {
            native = outer;
        }
        self.adapter.repaint_widget(&native);
    }
    pub fn get_adapter_popup_pos(
        &self,
        widget: &WidgetRef,
        min_height: f32,
        max_height: f32,
        anchor: CfxRectF,
        popup_rect: &mut CfxRectF,
    ) -> bool {
        self.adapter
            .get_popup_pos(widget, min_height, max_height, anchor, popup_rect)
    }
    pub fn get_widget_at_point(&self, parent: &WidgetRef, point: CfxPointF) -> Option<WidgetRef> {
        if parent.base().is_destroyed() {
            return None;
        }
        // Recursive input is already parent-local after subtracting each
        // child rectangle. Applying the ancestor matrix again double-counts
        // offsets below the second nesting level in the source.
        for child in self.children(Some(parent.base().id())).into_iter().rev() {
            if !child.base().is_visible() {
                continue;
            }
            let rect = child.get_widget_rect();
            if rect.contains_point(point) {
                return self.get_widget_at_point(
                    &child,
                    CfxPointF::new(point.x - rect.left, point.y - rect.top),
                );
            }
        }
        Some(parent.clone())
    }
    pub fn get_default_button(&self, parent: &WidgetRef) -> Option<WidgetRef> {
        if parent.get_class_id() == FwlType::PushButton
            && parent
                .base()
                .get_states()
                .intersects(WidgetState::PUSHBUTTON_DEFAULT)
        {
            return Some(parent.clone());
        }
        for child in self.children(Some(parent.base().id())) {
            if let Some(button) = self.get_default_button(&child) {
                return Some(button);
            }
        }
        None
    }
    pub fn on_process_message_to_form(&self, message: &mut FwlMessage) {
        if message.destination.is_some() {
            if let Some(app) = self.app.upgrade() {
                app.note_driver().process_message(message);
            }
        }
    }
    pub fn on_draw_widget(
        &self,
        widget: &WidgetRef,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
    ) {
        dispatch_draw_to(widget, graphics, matrix);
        if !graphics.get_clip_rect().is_empty() && !widget.base().is_destroyed() {
            self.draw_children(widget, graphics, matrix);
        }
    }
    fn draw_children(
        &self,
        parent: &WidgetRef,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
    ) {
        // Snapshot only small identities/owners; a child draw may mutate the
        // entire tree, so recheck each relationship before drawing it.
        for child in self.children(Some(parent.base().id())) {
            if !child.base().is_visible()
                || child.base().is_destroyed()
                || self
                    .get_parent_widget(child.base().id())
                    .is_none_or(|current| current.base().id() != parent.base().id())
            {
                continue;
            }
            let rect = child.get_widget_rect();
            if rect.is_empty() {
                continue;
            }
            let mut child_matrix = matrix;
            child_matrix.translate_prepend(rect.left, rect.top);
            if !parent.draws_child(child.base().id()) {
                dispatch_draw_to(&child, graphics, child_matrix);
            }
            if !child.base().is_destroyed() {
                self.draw_children(&child, graphics, child_matrix);
            }
        }
    }
}

#[cfg(test)]
#[path = "cfwl_core_tests.rs"]
mod tests;
