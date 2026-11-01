// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Window attachment and age tracking from `cffl_perwindowdata.{h,cpp}`.
use super::cffl_formfield::FormField;
use crate::fpdfsdk::cpdfsdk_pageview::CpdfSdkPageView;
use crate::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use crate::fpdfsdk::pwl::ipwl_fillernotify::PerWindowData;
use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
pub struct CfflPerWindowData {
    widget: Weak<CpdfSdkWidget>,
    page_view: Weak<CpdfSdkPageView>,
    form_field: RefCell<Option<Weak<dyn FormField>>>,
    appearance_age: u32,
    value_age: u32,
}
impl CfflPerWindowData {
    pub fn new(
        widget: &Rc<CpdfSdkWidget>,
        page_view: &Rc<CpdfSdkPageView>,
        appearance_age: u32,
        value_age: u32,
    ) -> Self {
        Self {
            widget: Rc::downgrade(widget),
            page_view: Rc::downgrade(page_view),
            form_field: RefCell::new(None),
            appearance_age,
            value_age,
        }
    }
    pub fn widget(&self) -> Option<Rc<CpdfSdkWidget>> {
        self.widget.upgrade().filter(|widget| widget.is_alive())
    }
    pub fn page_view(&self) -> Option<Rc<CpdfSdkPageView>> {
        self.page_view.upgrade().filter(|view| view.is_alive())
    }
    pub fn set_form_field(&self, field: &Rc<dyn FormField>) {
        *self.form_field.borrow_mut() = Some(Rc::downgrade(field));
    }
    pub fn form_field(&self) -> Option<Rc<dyn FormField>> {
        self.form_field
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .filter(|field| field.base().is_alive())
    }
    pub fn appearance_age_equals(&self, age: u32) -> bool {
        age == self.appearance_age
    }
    pub fn value_age(&self) -> u32 {
        self.value_age
    }
    pub(crate) fn copy(&self) -> Self {
        Self {
            widget: self.widget.clone(),
            page_view: self.page_view.clone(),
            form_field: RefCell::new(self.form_field.borrow().clone()),
            appearance_age: self.appearance_age,
            value_age: self.value_age,
        }
    }
}
impl PerWindowData for CfflPerWindowData {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_data(&self) -> Box<dyn PerWindowData> {
        Box::new(self.copy())
    }
}
