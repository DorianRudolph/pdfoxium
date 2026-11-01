// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Shared mouse/highlight behavior from `fpdfsdk/formfiller/cffl_button.{h,cpp}`.
//! This is an abstract composed base: concrete buttons supply their PWL window.
use super::cffl_formfield::CfflFormField;
use super::cffl_interactiveformfiller::CfflInteractiveFormFiller;
use crate::fpdfdoc::cpdf_annot::AppearanceMode;
use crate::fpdfsdk::cpdfsdk_pageview::CpdfSdkPageView;
use crate::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use crate::fpdfsdk::pwl::pwl_draw::PwlDraw;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF};
use std::cell::Cell;
use std::rc::Rc;

pub struct CfflButton {
    base: CfflFormField,
    mouse_in: Cell<bool>,
    mouse_down: Cell<bool>,
}
impl CfflButton {
    pub fn new(filler: &Rc<CfflInteractiveFormFiller>, widget: &Rc<CpdfSdkWidget>) -> Self {
        Self {
            base: CfflFormField::new(filler, widget),
            mouse_in: Cell::new(false),
            mouse_down: Cell::new(false),
        }
    }
    pub fn base(&self) -> &CfflFormField {
        &self.base
    }
    pub fn on_mouse_enter(&self, view: &Rc<CpdfSdkPageView>) {
        self.mouse_in.set(true);
        self.base.invalidate_rect(self.base.get_view_bbox(view));
    }
    pub fn on_mouse_exit(&self, view: &Rc<CpdfSdkPageView>) {
        self.mouse_in.set(false);
        self.base.invalidate_rect(self.base.get_view_bbox(view));
        self.base.cancel_timer();
    }
    pub fn on_l_button_down(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        _flags: u32,
        point: CfxPointF,
    ) -> bool {
        if !widget.rect().contains_point(point) {
            return false;
        }
        self.mouse_down.set(true);
        self.base.set_valid(true);
        self.base.invalidate_rect(self.base.get_view_bbox(view));
        true
    }
    pub fn on_l_button_up(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        _flags: u32,
        point: CfxPointF,
    ) -> bool {
        if !widget.rect().contains_point(point) {
            return false;
        }
        self.mouse_down.set(false);
        self.base.invalidate_rect(self.base.get_view_bbox(view));
        true
    }
    pub fn on_mouse_move(
        &self,
        _view: &Rc<CpdfSdkPageView>,
        _flags: u32,
        _point: CfxPointF,
    ) -> bool {
        true
    }
    pub fn on_draw(
        &self,
        _view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        draw: &mut dyn PwlDraw,
        matrix: CfxMatrix,
    ) {
        let requested = requested_mode(
            widget.is_push_highlighted(),
            self.mouse_down.get(),
            self.mouse_in.get(),
        );
        let mode = if requested == AppearanceMode::Normal
            || widget.is_widget_appearance_valid(requested)
        {
            requested
        } else {
            AppearanceMode::Normal
        };
        widget.draw_appearance(draw, matrix, mode);
    }
    pub fn on_draw_deactive(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        draw: &mut dyn PwlDraw,
        matrix: CfxMatrix,
    ) {
        self.on_draw(view, widget, draw, matrix);
    }
}
fn requested_mode(push_highlighted: bool, mouse_down: bool, mouse_in: bool) -> AppearanceMode {
    if !push_highlighted {
        AppearanceMode::Normal
    } else if mouse_down {
        AppearanceMode::Down
    } else if mouse_in {
        AppearanceMode::Rollover
    } else {
        AppearanceMode::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_button_highlight_order_prefers_pressed_over_hover() {
        for down in [false, true] {
            for inside in [false, true] {
                assert_eq!(requested_mode(false, down, inside), AppearanceMode::Normal);
            }
        }
        assert_eq!(requested_mode(true, false, false), AppearanceMode::Normal);
        assert_eq!(requested_mode(true, false, true), AppearanceMode::Rollover);
        assert_eq!(requested_mode(true, true, false), AppearanceMode::Down);
        assert_eq!(requested_mode(true, true, true), AppearanceMode::Down);
    }
}
