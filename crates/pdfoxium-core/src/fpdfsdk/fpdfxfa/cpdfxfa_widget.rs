// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cpdfxfa_widget.{h,cpp}`. The SDK annotation retains the real FF widget;
//! its page/annotation links remain weak. Actual FFWidgetHandler owns the
//! rotation, focus, event-update and capability-gating semantics.
use crate::{
    fpdfdoc::cpdf_annot::Subtype,
    fpdfsdk::{
        cpdfsdk_annot::{CpdfSdkAnnot, SdkInputEvent},
        cpdfsdk_pageview::CpdfSdkPageView,
        fpdf_fwlevent::*,
    },
    fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF, CfxVector},
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fwl::{fwl_widgetdef::*, fwl_widgethit::FwlWidgetHit},
        fxfa::{
            cxfa_ffdocview::CxfaFfDocView,
            cxfa_ffwidget::{FfResult, FfWidget, FfWidgetError, FocusOption},
            cxfa_ffwidgethandler::CxfaFfWidgetHandler,
            layout::cxfa_layoutitem::LayoutItemWidget,
            parser::cxfa_node::XfaFfWidgetType,
        },
    },
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
pub struct CpdfXfaWidget {
    widget: Rc<dyn LayoutItemWidget>,
    page_view: Weak<CpdfSdkPageView>,
    annot: RefCell<Weak<CpdfSdkAnnot>>,
}
impl CpdfXfaWidget {
    pub fn new(widget: Rc<dyn LayoutItemWidget>, page_view: &Rc<CpdfSdkPageView>) -> Rc<Self> {
        assert!(
            widget.as_ff_widget().is_some(),
            "XFA annotation requires an FF widget"
        );
        Rc::new(Self {
            widget,
            page_view: Rc::downgrade(page_view),
            annot: RefCell::new(Weak::new()),
        })
    }
    pub fn ff_widget(&self) -> &Rc<dyn LayoutItemWidget> {
        &self.widget
    }
    pub fn set_annotation(&self, annot: &Rc<CpdfSdkAnnot>) {
        *self.annot.borrow_mut() = Rc::downgrade(annot);
    }
    pub fn bind_annotation(&self, annot: &Rc<CpdfSdkAnnot>) {
        self.set_annotation(annot);
    }
    pub fn annotation(&self) -> Option<Rc<CpdfSdkAnnot>> {
        self.annot
            .borrow()
            .upgrade()
            .filter(|annot| annot.is_alive())
    }
    pub fn page_view(&self) -> Option<Rc<CpdfSdkPageView>> {
        self.page_view.upgrade().filter(|view| view.is_alive())
    }
    pub fn is_alive(&self) -> bool {
        self.annotation().is_some()
            && self.widget().widget().layout_item().is_some()
            && self.doc_view().is_some()
    }
    fn widget(&self) -> &dyn FfWidget {
        self.widget.as_ff_widget().expect("validated FF owner")
    }
    pub fn doc_view(&self) -> Option<Rc<CxfaFfDocView>> {
        let item = self.widget().widget().page_view()?;
        let page = item.page_view()?;
        page.as_ff_page_view()?.doc_view()
    }
    pub fn subtype(&self) -> Subtype {
        Subtype::XfaWidget
    }
    pub fn rect(&self) -> CfxFloatRect {
        self.widget()
            .widget()
            .layout_item()
            .map_or_else(CfxFloatRect::default, |item| {
                item.get_absolute_rect().to_float_rect()
            })
    }
    pub fn view_bbox(&self) -> CfxFloatRect {
        let widget = self.widget();
        let focus = if widget.widget().node().get_ff_widget_type() == XfaFfWidgetType::Signature {
            FocusOption::DrawFocus
        } else {
            FocusOption::DoNotDrawFocus
        };
        let mut rect = widget.get_bbox(focus).to_float_rect();
        rect.inflate_xy(1., 1.);
        rect
    }
    pub fn on_draw(
        &self,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
    ) -> FfResult<bool> {
        if !self.is_alive() {
            return Ok(false);
        }
        let highlighted = self
            .page_view()
            .and_then(|page| page.environment())
            .and_then(|env| env.get_focus_annot())
            .is_none_or(|annot| {
                self.annotation()
                    .is_none_or(|this| !Rc::ptr_eq(&this, &annot))
            });
        let view = self.doc_view().ok_or(FfWidgetError::DetachedDocumentView)?;
        CxfaFfWidgetHandler::new(Rc::downgrade(&view)).render_widget(
            self.widget(),
            graphics,
            matrix,
            highlighted,
        )
    }
    pub fn do_hit_test(&self, point: CfxPointF) -> bool {
        if !self.is_alive() {
            return false;
        }
        self.handler()
            .is_some_and(|handler| handler.hit_test(self.widget(), point) != FwlWidgetHit::Unknown)
    }
    fn handler(&self) -> Option<CxfaFfWidgetHandler> {
        Some(CxfaFfWidgetHandler::new(Rc::downgrade(&self.doc_view()?)))
    }

    pub fn on_changed_focus(&self) -> bool {
        let Some(view) = self.doc_view() else {
            return false;
        };
        if view.set_focus(Some(self.widget.clone())).unwrap_or(false) {
            return false;
        }
        view.focus_widget()
            .is_none_or(|owner| !Rc::ptr_eq(&owner, &self.widget))
    }
    pub fn dispatch(&self, event: SdkInputEvent) -> bool {
        if !self.is_alive() {
            return false;
        }
        let Some(view) = self.doc_view() else {
            return false;
        };
        let widget = self.widget();
        let handler = CxfaFfWidgetHandler::new(Rc::downgrade(&view));
        match event {
            SdkInputEvent::MouseEnter(_) => handler.on_mouse_enter(widget),
            SdkInputEvent::MouseExit(_) => handler.on_mouse_exit(widget),
            SdkInputEvent::LButtonDown(flags, point) => handler
                .on_button_down_checked(&self.widget, get_key_flags(flags), point, false, || {
                    self.is_alive()
                })
                .unwrap_or(false),
            SdkInputEvent::RButtonDown(flags, point) => handler
                .on_button_down_checked(&self.widget, get_key_flags(flags), point, true, || {
                    self.is_alive()
                })
                .unwrap_or(false),
            SdkInputEvent::LButtonUp(flags, point) => {
                handler.on_l_button_up(widget, get_key_flags(flags), point)
            }
            SdkInputEvent::LButtonDblClk(flags, point) => {
                handler.on_l_button_dbl_clk(widget, get_key_flags(flags), point)
            }
            SdkInputEvent::MouseMove(flags, point) => {
                handler.on_mouse_move(widget, get_key_flags(flags), point)
            }
            SdkInputEvent::MouseWheel(flags, point, delta) => handler.on_mouse_wheel(
                widget,
                get_key_flags(flags),
                point,
                CfxVector::new(delta.x as i32, delta.y as i32),
            ),
            SdkInputEvent::RButtonUp(flags, point) => {
                handler.on_r_button_up(widget, get_key_flags(flags), point)
            }
            SdkInputEvent::Char(ch, flags) => handler.on_char(widget, ch, get_key_flags(flags)),
            SdkInputEvent::KeyDown(key, flags) => {
                handler.on_key_down(widget, key as u32, get_key_flags(flags))
            }
            SdkInputEvent::SetFocus(_) => true,
            SdkInputEvent::KillFocus(_) => {
                let _ = view.set_focus(None);
                true
            }
        }
    }
    pub fn can_undo(&self) -> bool {
        self.is_alive()
            && self
                .handler()
                .is_some_and(|handler| handler.can_undo(self.widget()))
    }
    pub fn can_redo(&self) -> bool {
        self.is_alive()
            && self
                .handler()
                .is_some_and(|handler| handler.can_redo(self.widget()))
    }
    pub fn undo(&self) -> bool {
        self.is_alive()
            && self
                .handler()
                .is_some_and(|handler| handler.undo(self.widget()))
    }
    pub fn redo(&self) -> bool {
        self.is_alive()
            && self
                .handler()
                .is_some_and(|handler| handler.redo(self.widget()))
    }
    pub fn get_text(&self) -> Vec<u32> {
        if !self.is_alive() {
            return Vec::new();
        }
        self.handler()
            .map_or_else(Vec::new, |handler| handler.get_text(self.widget()))
    }
    pub fn get_selected_text(&self) -> Vec<u32> {
        if !self.is_alive() {
            return Vec::new();
        }
        self.handler()
            .map_or_else(Vec::new, |handler| handler.get_selected_text(self.widget()))
    }
    pub fn replace_selection(&self, text: &[u32]) {
        if self.is_alive() {
            if let Some(handler) = self.handler() {
                handler.paste_text(self.widget(), text);
            }
        }
    }
    pub fn replace_and_keep_selection(&self, text: &[u32]) {
        self.replace_selection(text);
    }
    pub fn select_all_text(&self) -> bool {
        self.is_alive()
            && self
                .handler()
                .is_some_and(|handler| handler.select_all_text(self.widget()))
    }
    // Native CPDFXFA_Widget does not implement selection-by-index (only PDF choice
    // widgets expose those SDK operations).
    pub fn set_index_selected(&self, _index: i32, _selected: bool) -> bool {
        false
    }
    pub fn is_index_selected(&self, _index: i32) -> bool {
        false
    }
}
pub fn get_key_flags(input: u32) -> u32 {
    let mapping = [
        (FWL_EVENTFLAG_ControlKey, KEY_FLAG_CTRL),
        (FWL_EVENTFLAG_MetaKey, KEY_FLAG_COMMAND),
        (FWL_EVENTFLAG_LeftButtonDown, KEY_FLAG_LBUTTON),
        (FWL_EVENTFLAG_MiddleButtonDown, KEY_FLAG_MBUTTON),
        (FWL_EVENTFLAG_RightButtonDown, KEY_FLAG_RBUTTON),
        (FWL_EVENTFLAG_ShiftKey, KEY_FLAG_SHIFT),
        (FWL_EVENTFLAG_AltKey, KEY_FLAG_ALT),
    ];
    mapping.into_iter().fold(0, |flags, (source, target)| {
        if input & source != 0 {
            flags | target
        } else {
            flags
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sdk_key_flags_use_xfa_bit_positions_and_ignore_non_modifiers() {
        let table = [
            (FWL_EVENTFLAG_ControlKey, KEY_FLAG_CTRL),
            (FWL_EVENTFLAG_MetaKey, KEY_FLAG_COMMAND),
            (FWL_EVENTFLAG_LeftButtonDown, KEY_FLAG_LBUTTON),
            (FWL_EVENTFLAG_MiddleButtonDown, KEY_FLAG_MBUTTON),
            (FWL_EVENTFLAG_RightButtonDown, KEY_FLAG_RBUTTON),
            (FWL_EVENTFLAG_ShiftKey, KEY_FLAG_SHIFT),
            (FWL_EVENTFLAG_AltKey, KEY_FLAG_ALT),
        ];
        for (source, target) in table {
            assert_eq!(get_key_flags(source), target);
        }
        assert_eq!(get_key_flags(u32::MAX), 127);
        assert_eq!(
            get_key_flags(FWL_EVENTFLAG_AutoRepeat | FWL_EVENTFLAG_KeyPad),
            0
        );
    }
}
