// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Shared text filler/font lifetime from `cffl_textobject.{h,cpp}`.
use super::cffl_formfield::CfflFormField;
use super::cffl_interactiveformfiller::CfflInteractiveFormFiller;
use crate::fpdfdoc::cpdf_bafontmap::CpdfBaFontMap;
use crate::fpdfsdk::cpdfsdk_pageview::CpdfSdkPageView;
use crate::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use crate::fpdfsdk::pwl::cpwl_wnd::Window;
use std::cell::RefCell;
use std::rc::Rc;
pub struct CfflTextObject {
    base: CfflFormField,
    font_map: RefCell<Option<Rc<CpdfBaFontMap>>>,
}
impl CfflTextObject {
    pub fn new(filler: &Rc<CfflInteractiveFormFiller>, widget: &Rc<CpdfSdkWidget>) -> Self {
        Self {
            base: CfflFormField::new(filler, widget),
            font_map: RefCell::new(None),
        }
    }
    pub fn base(&self) -> &CfflFormField {
        &self.base
    }
    pub fn get_or_create_font_map(&self) -> Option<Rc<CpdfBaFontMap>> {
        if let Some(map) = self.font_map.borrow().clone() {
            return Some(map);
        }
        let widget = self.base.widget()?;
        let document = widget.document()?;
        let map = Rc::new(CpdfBaFontMap::new(&document, widget.dict(), b"N"));
        if !self.base.is_alive() {
            return None;
        }
        *self.font_map.borrow_mut() = Some(map.clone());
        Some(map)
    }
    pub fn reset_pwl_window(&self, view: &Rc<CpdfSdkPageView>) -> Option<Rc<dyn Window>> {
        self.base.destroy_pwl_window(view);
        let window = self.base.create_or_update_pwl_window(view)?;
        self.base.widget()?.update_field();
        (self.base.is_alive() && window.wnd().is_alive()).then_some(window)
    }
    pub fn restore_pwl_window(&self, view: &Rc<CpdfSdkPageView>) -> Option<Rc<dyn Window>> {
        let owner = self.base.owner()?;
        owner.save_pwl_window_state(view);
        self.base.destroy_pwl_window(view);
        owner.recreate_pwl_window_from_saved_state(view);
        let window = self.base.pwl_window(view)?;
        self.base.widget()?.update_field();
        (self.base.is_alive() && window.wnd().is_alive()).then_some(window)
    }
}
impl Drop for CfflTextObject {
    fn drop(&mut self) {
        self.base.destroy_windows();
    }
}
