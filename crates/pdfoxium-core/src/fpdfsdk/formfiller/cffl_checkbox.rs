// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Checkbox formfiller from `fpdfsdk/formfiller/cffl_checkbox.{h,cpp}`.
use super::cffl_button::CfflButton;
use super::cffl_formfield::{CfflFormField, FormField};
use super::cffl_interactiveformfiller::CfflInteractiveFormFiller;
use super::cffl_perwindowdata::CfflPerWindowData;
use crate::fpdfsdk::cpdfsdk_pageview::CpdfSdkPageView;
use crate::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use crate::fpdfsdk::fpdf_fwlevent::{FWL_VKEY_Return, FWL_VKEY_Space};
use crate::fpdfsdk::pwl::cpwl_special_button::CpwlCheckBox;
use crate::fpdfsdk::pwl::cpwl_wnd::{CreateParams, Window};
use crate::fpdfsdk::pwl::pwl_draw::PwlDraw;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF};
use std::any::Any;
use std::rc::Rc;

pub struct CfflCheckBox {
    button: CfflButton,
}
impl CfflCheckBox {
    pub fn new(filler: &Rc<CfflInteractiveFormFiller>, widget: &Rc<CpdfSdkWidget>) -> Rc<Self> {
        let field = Rc::new(Self {
            button: CfflButton::new(filler, widget),
        });
        let owner: Rc<dyn FormField> = field.clone();
        field.base().bind_owner(&owner);
        field
    }
    fn with_check<R>(
        &self,
        view: &Rc<CpdfSdkPageView>,
        create: bool,
        operation: impl FnOnce(&CpwlCheckBox) -> R,
    ) -> Option<R> {
        let window = if create {
            self.base().create_or_update_pwl_window(view)
        } else {
            self.base().pwl_window(view)
        }?;
        if !window.wnd().is_alive() {
            return None;
        }
        Some(operation(window.as_any().downcast_ref::<CpwlCheckBox>()?))
    }
}
impl FormField for CfflCheckBox {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn base(&self) -> &CfflFormField {
        self.button.base()
    }
    fn new_pwl_window(
        &self,
        cp: CreateParams,
        data: Box<CfflPerWindowData>,
    ) -> Option<Rc<dyn Window>> {
        let window = CpwlCheckBox::new(cp, Some(data));
        window.wnd.realize();
        let checked = self.base().widget()?.is_checked();
        if !self.base().is_alive() || !window.wnd.is_alive() {
            return None;
        }
        window.set_check(checked);
        Some(window)
    }
    fn on_key_down(&self, key: i32, flags: u32) -> bool {
        if key == FWL_VKEY_Return || key == FWL_VKEY_Space {
            true
        } else {
            self.base().on_key_down(key, flags)
        }
    }
    fn on_char(&self, widget: &Rc<CpdfSdkWidget>, character: u32, flags: u32) -> bool {
        if !matches!(character, 13 | 32) {
            return self.base().on_char(widget, character, flags);
        }
        let Some(view) = widget.page_view() else {
            return false;
        };
        let Some(filler) = self.base().filler() else {
            return false;
        };
        if filler.on_button_up(widget, &view, flags)
            || !widget.is_alive()
            || !self.base().is_alive()
        {
            return true;
        }
        self.base().on_char(widget, character, flags);
        if !self.base().is_alive() || !widget.is_alive() {
            return true;
        }
        self.with_check(&view, true, |window| {
            if !window.wnd.is_read_only() {
                let checked = widget.is_checked();
                if window.wnd.is_alive() {
                    window.set_check(!checked);
                }
            }
        });
        self.base().commit_data(&view, flags)
    }
    fn on_l_button_up(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        self.button.on_l_button_up(view, widget, flags, point);
        if !self.base().is_valid() {
            return true;
        }
        self.with_check(view, true, |window| {
            let checked = widget.is_checked();
            if window.wnd.is_alive() {
                window.set_check(!checked);
            }
        });
        self.base().is_alive() && self.base().commit_data(view, flags)
    }
    fn is_data_changed(&self, view: &Rc<CpdfSdkPageView>) -> bool {
        let Some(widget) = self.base().widget() else {
            return false;
        };
        self.with_check(view, false, |window| {
            window.is_checked() != widget.is_checked()
        })
        .unwrap_or(false)
    }
    fn save_data(&self, view: &Rc<CpdfSdkPageView>) {
        let Some(checked) = self.with_check(view, false, CpwlCheckBox::is_checked) else {
            return;
        };
        let Some(widget) = self.base().widget() else {
            return;
        };
        widget.set_check(checked);
        if !widget.is_alive() || !self.base().is_alive() {
            return;
        }
        widget.update_field();
        if widget.is_alive() && self.base().is_alive() {
            self.base().set_change_mark();
        }
    }
    fn on_mouse_enter(&self, view: &Rc<CpdfSdkPageView>) {
        self.button.on_mouse_enter(view)
    }
    fn on_mouse_exit(&self, view: &Rc<CpdfSdkPageView>) {
        self.button.on_mouse_exit(view)
    }
    fn on_l_button_down(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        self.button.on_l_button_down(view, widget, flags, point)
    }
    fn on_mouse_move(&self, view: &Rc<CpdfSdkPageView>, flags: u32, point: CfxPointF) -> bool {
        self.button.on_mouse_move(view, flags, point)
    }
    fn on_draw(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        draw: &mut dyn PwlDraw,
        matrix: CfxMatrix,
    ) {
        self.button.on_draw(view, widget, draw, matrix)
    }
    fn on_draw_deactive(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        draw: &mut dyn PwlDraw,
        matrix: CfxMatrix,
    ) {
        self.button.on_draw_deactive(view, widget, draw, matrix)
    }
}
