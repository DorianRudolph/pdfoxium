// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! FWL host ownership. The app owns its managers and registered widgets;
//! widget/manager back-references are weak, replacing source GC cycles.
use super::{
    cfwl_notedriver::CfwlNoteDriver,
    cfwl_widget::{WidgetId, WidgetRef},
    cfwl_widgetmgr::{CfwlWidgetMgr, WidgetMgrAdapter},
    ifwl_themeprovider::ThemeProvider,
};
use crate::fxcrt::cfx_timer::TimerHandler;
use std::{cell::Cell, rc::Rc};

pub trait AppAdapter {
    fn widget_mgr_adapter(&self) -> Rc<dyn WidgetMgrAdapter>;
    fn timer_handler(&self) -> Option<Rc<dyn TimerHandler>>;
    fn theme_provider(&self) -> Rc<dyn ThemeProvider>;
}
pub struct CfwlApp {
    widget_mgr: CfwlWidgetMgr,
    note_driver: CfwlNoteDriver,
    adapter: Rc<dyn AppAdapter>,
    next_widget_id: Cell<WidgetId>,
}
impl CfwlApp {
    pub fn new(adapter: Rc<dyn AppAdapter>) -> Rc<Self> {
        Rc::new_cyclic(|app| Self {
            widget_mgr: CfwlWidgetMgr::new(adapter.widget_mgr_adapter(), app.clone()),
            note_driver: CfwlNoteDriver::new(app.clone()),
            adapter,
            next_widget_id: Cell::new(1),
        })
    }
    pub fn widget_mgr(&self) -> &CfwlWidgetMgr {
        &self.widget_mgr
    }
    pub fn note_driver(&self) -> &CfwlNoteDriver {
        &self.note_driver
    }
    pub fn theme_provider(&self) -> Rc<dyn ThemeProvider> {
        self.adapter.theme_provider()
    }
    pub fn timer_handler(&self) -> Option<Rc<dyn TimerHandler>> {
        self.adapter.timer_handler()
    }
    pub fn widget_mgr_adapter(&self) -> Rc<dyn WidgetMgrAdapter> {
        self.adapter.widget_mgr_adapter()
    }
    pub fn register_widget(&self, widget: WidgetRef) {
        let parent = widget.base().outer_id();
        self.widget_mgr.insert_widget(parent, widget);
    }
    pub fn remove_widget(&self, id: WidgetId) {
        self.widget_mgr.remove_widget(id);
    }
    pub(crate) fn allocate_widget_id(&self) -> WidgetId {
        let id = self.next_widget_id.get();
        self.next_widget_id
            .set(id.checked_add(1).expect("FWL widget identity exhausted"));
        id
    }
}

impl Drop for CfwlApp {
    fn drop(&mut self) {
        self.widget_mgr.remove_all_widgets();
    }
}
