// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! The paginated layout coordinator from `cxfa_layoutprocessor.{h,cpp}`.
//! The owned root processor is parked outside its slot during callbacks;
//! recursive layout requests fail explicitly rather than borrowing it twice.
use super::{
    cxfa_contentlayoutprocessor::{
        ContentLayoutHost, CxfaContentLayoutProcessor, LayoutError, LayoutResult,
    },
    cxfa_layoutitem::{CxfaLayoutItem, CxfaLayoutStore},
    cxfa_viewlayoutitem::CxfaViewLayoutItem,
    cxfa_viewlayoutprocessor::{CxfaViewLayoutProcessor, ViewLayoutError},
};
use crate::{
    fxcrt::fx_coordinates::CfxPointF,
    xfa::fxfa::{
        fxfa_basic::{XfaAttribute, XfaElement, XfaHashCode, XfaUnit},
        parser::{cxfa_document::CxfaDocument, cxfa_node::CxfaNode},
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

#[derive(Debug)]
pub enum LayoutProcessorError {
    Reentrant,
    ExpiredDocument,
    ProgressOverflow,
    Content(LayoutError),
    View(ViewLayoutError),
}
impl From<LayoutError> for LayoutProcessorError {
    fn from(value: LayoutError) -> Self {
        Self::Content(value)
    }
}
impl From<ViewLayoutError> for LayoutProcessorError {
    fn from(value: ViewLayoutError) -> Self {
        Self::View(value)
    }
}
struct ActiveLayout<'a>(&'a Cell<bool>);
impl Drop for ActiveLayout<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
/// Restores the processor on both ordinary errors and callback unwinding.
struct RootContentLease<'a> {
    slot: &'a RefCell<Option<Box<CxfaContentLayoutProcessor>>>,
    content: Option<Box<CxfaContentLayoutProcessor>>,
}
impl Drop for RootContentLease<'_> {
    fn drop(&mut self) {
        *self.slot.borrow_mut() = self.content.take();
    }
}

pub struct CxfaLayoutProcessor {
    document: Weak<CxfaDocument>,
    store: Rc<CxfaLayoutStore>,
    view: CxfaViewLayoutProcessor,
    content_host: RefCell<Option<Rc<dyn ContentLayoutHost>>>,
    content: RefCell<Option<Box<CxfaContentLayoutProcessor>>>,
    progress_counter: Cell<u32>,
    active: Cell<bool>,
    has_changed_containers: Cell<bool>,
    need_layout: Cell<bool>,
}
impl CxfaLayoutProcessor {
    pub fn new(document: &Rc<CxfaDocument>) -> Rc<Self> {
        let store = CxfaLayoutStore::new();
        let processor = Rc::new(Self {
            document: Rc::downgrade(document),
            view: CxfaViewLayoutProcessor::new(store.clone()),
            store: store.clone(),
            content_host: RefCell::new(None),
            content: RefCell::new(None),
            progress_counter: Cell::new(0),
            active: Cell::new(false),
            has_changed_containers: Cell::new(false),
            need_layout: Cell::new(true),
        });
        store.set_processor(Rc::downgrade(&processor));
        document.set_layout_processor(Some(Rc::downgrade(&processor)));
        processor
    }
    pub fn from_document(document: &CxfaDocument) -> Option<Rc<Self>> {
        document.get_layout_processor()
    }
    pub fn document(&self) -> Option<Rc<CxfaDocument>> {
        self.document.upgrade()
    }
    pub fn store(&self) -> &Rc<CxfaLayoutStore> {
        &self.store
    }
    pub fn get_layout_page_mgr(&self) -> &CxfaViewLayoutProcessor {
        &self.view
    }
    pub fn set_content_host(&self, host: Option<Rc<dyn ContentLayoutHost>>) {
        let old = self.content_host.replace(host);
        drop(old);
    }
    fn enter(&self) -> Result<ActiveLayout<'_>, LayoutProcessorError> {
        if self.active.replace(true) {
            return Err(LayoutProcessorError::Reentrant);
        }
        Ok(ActiveLayout(&self.active))
    }
    pub fn set_force_relayout(&self) {
        self.need_layout.set(true);
    }
    pub fn set_has_changed_container(&self) {
        self.has_changed_containers.set(true);
    }
    pub fn need_layout(&self) -> bool {
        self.need_layout.get() || self.has_changed_containers.get()
    }
    pub fn start_layout(&self) -> Result<i32, LayoutProcessorError> {
        let _active = self.enter()?;
        if self.need_layout() {
            self.restart_layout_internal()
        } else {
            Ok(100)
        }
    }
    pub fn restart_layout(&self) -> Result<i32, LayoutProcessorError> {
        let _active = self.enter()?;
        self.restart_layout_internal()
    }
    fn restart_layout_internal(&self) -> Result<i32, LayoutProcessorError> {
        let old = self.content.replace(None);
        drop(old);
        self.progress_counter.set(0);
        let document = self
            .document()
            .ok_or(LayoutProcessorError::ExpiredDocument)?;
        let Some(form) = document.get_xfa_node(XfaHashCode::Form as u32) else {
            return Ok(-1);
        };
        let Some(root) = form.get_first_child_by_class(XfaElement::Subform) else {
            return Ok(-1);
        };
        if !self.view.init_layout_page(&root) || !self.view.prepare_first_page(&root)? {
            return Ok(-1);
        }
        let host = self.content_host.borrow().clone();
        let processor = Box::new(CxfaContentLayoutProcessor::new(
            &root,
            self.store.clone(),
            host,
        ));
        *self.content.borrow_mut() = Some(processor);
        self.progress_counter.set(1);
        Ok(0)
    }
    pub fn do_layout(&self) -> Result<i32, LayoutProcessorError> {
        let _active = self.enter()?;
        self.do_layout_internal()
    }
    fn do_layout_internal(&self) -> Result<i32, LayoutProcessorError> {
        if self.progress_counter.get() < 1 {
            return Ok(-1);
        }
        // Keep the document and node arena alive until all synchronous callbacks finish.
        let _document = self
            .document()
            .ok_or(LayoutProcessorError::ExpiredDocument)?;
        let mut lease = RootContentLease {
            slot: &self.content,
            content: self.content.borrow_mut().take(),
        };
        let Some(content) = lease.content.as_mut() else {
            return Ok(-1);
        };
        let root = content.form_node();
        let position = CfxPointF::new(
            root.get_measure_in_unit(XfaAttribute::X, XfaUnit::Pt),
            root.get_measure_in_unit(XfaAttribute::Y, XfaUnit::Pt),
        );
        loop {
            let height = self.view.get_avail_height();
            let status = content.do_layout(&self.view, true, height, height)?;
            if status != LayoutResult::Done {
                self.progress_counter.set(
                    self.progress_counter
                        .get()
                        .checked_add(1)
                        .ok_or(LayoutProcessorError::ProgressOverflow)?,
                );
            }
            let item = content.extract_layout_item();
            if let Some(item) = &item {
                item.set_position(position);
            }
            if !self.view.submit_content_item(item.as_ref(), status) {
                return Ok(-1);
            }
            if status == LayoutResult::Done {
                break;
            }
        }
        self.view.finish_paginated_page_sets();
        self.view.sync_layout_data()?;
        self.has_changed_containers.set(false);
        self.need_layout.set(false);
        // The native synchronous loop always reaches its complete progress ratio.
        Ok(100)
    }
    pub fn increment_layout(&self) -> Result<bool, LayoutProcessorError> {
        let _active = self.enter()?;
        if self.need_layout.get() {
            if self.restart_layout_internal()? < 0 {
                return Ok(false);
            }
            Ok(self.do_layout_internal()? == 100)
        } else {
            Ok(!self.has_changed_containers.get())
        }
    }
    /// Safe scoped equivalent of GetRootContentLayoutProcessor. Neither the
    /// controller slot nor a RefCell guard is borrowed while the client runs.
    pub fn with_root_content_layout_processor<R>(
        &self,
        operation: impl FnOnce(Option<&CxfaContentLayoutProcessor>) -> R,
    ) -> Result<R, LayoutProcessorError> {
        let _active = self.enter()?;
        let lease = RootContentLease {
            slot: &self.content,
            content: self.content.borrow_mut().take(),
        };
        Ok(operation(lease.content.as_deref()))
    }
    pub fn count_pages(&self) -> i32 {
        self.view.get_page_count()
    }
    pub fn get_page(&self, index: i32) -> Option<CxfaViewLayoutItem> {
        self.view.get_page(index)
    }
    pub fn get_page_index(&self, page: &CxfaViewLayoutItem) -> i32 {
        self.view.get_page_index(page)
    }
    pub fn get_layout_item(&self, node: &CxfaNode) -> Option<CxfaLayoutItem> {
        node.get_layout_item()
    }
}
#[cfg(test)]
#[path = "cxfa_layoutprocessor_tests.rs"]
mod tests;
