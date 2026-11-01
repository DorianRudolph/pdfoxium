// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Push-button formfiller from `fpdfsdk/formfiller/cffl_pushbutton.{h,cpp}`.
use super::cffl_button::CfflButton;
use super::cffl_formfield::{CfflFormField, FormField};
use super::cffl_interactiveformfiller::CfflInteractiveFormFiller;
use super::cffl_perwindowdata::CfflPerWindowData;
use crate::fpdfsdk::cpdfsdk_pageview::CpdfSdkPageView;
use crate::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use crate::fpdfsdk::pwl::cpwl_special_button::CpwlPushButton;
use crate::fpdfsdk::pwl::cpwl_wnd::{CreateParams, Window};
use crate::fpdfsdk::pwl::pwl_draw::PwlDraw;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF};
use std::any::Any;
use std::rc::Rc;

pub struct CfflPushButton {
    button: CfflButton,
}
impl CfflPushButton {
    pub fn new(filler: &Rc<CfflInteractiveFormFiller>, widget: &Rc<CpdfSdkWidget>) -> Rc<Self> {
        let field = Rc::new(Self {
            button: CfflButton::new(filler, widget),
        });
        let owner: Rc<dyn FormField> = field.clone();
        field.base().bind_owner(&owner);
        field
    }
}
impl FormField for CfflPushButton {
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
        let window = CpwlPushButton::new(cp, Some(data));
        window.wnd.realize();
        (self.base().is_alive() && window.wnd.is_alive()).then_some(window as Rc<dyn Window>)
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
    fn on_l_button_up(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        self.button.on_l_button_up(view, widget, flags, point)
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
