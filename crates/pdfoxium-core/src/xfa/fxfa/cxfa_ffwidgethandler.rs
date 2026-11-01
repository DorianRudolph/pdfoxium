// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_ffwidgethandler.{h,cpp}`. The view backedge is weak; input clients
//! retain the actual layout-widget owner across focus/script callbacks.
use super::{
    cxfa_eventparam::{CxfaEventParam, XfaEventType},
    cxfa_ffdocview::{CxfaFfDocView, UpdateScope, EVENT_ACTIVITY},
    cxfa_ffwidget::{FfResult, FfWidget, FfWidgetError, FfWidgetHandler, HighlightOption},
    fxfa::{XfaEventError, XfaWidgetStatus},
    fxfa_basic::{XfaAttributeValue, XfaElement},
    layout::cxfa_layoutitem::LayoutItemWidget,
    parser::cxfa_node::CxfaNode,
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxVector},
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fwl::{cfwl_message::MouseCommand, fwl_widgethit::FwlWidgetHit},
    },
};
use std::rc::{Rc, Weak};

pub struct CxfaFfWidgetHandler {
    doc_view: Weak<CxfaFfDocView>,
}
impl CxfaFfWidgetHandler {
    pub fn new(doc_view: Weak<CxfaFfDocView>) -> Self {
        Self { doc_view }
    }
    pub fn on_mouse_enter(&self, widget: &dyn FfWidget) -> bool {
        let Some(view) = self.doc_view.upgrade() else {
            return false;
        };
        let _scope = UpdateScope::new(&view);
        widget.on_mouse_enter()
    }
    pub fn on_mouse_exit(&self, widget: &dyn FfWidget) -> bool {
        let Some(view) = self.doc_view.upgrade() else {
            return false;
        };
        let _scope = UpdateScope::new(&view);
        widget.on_mouse_exit()
    }
    pub fn on_l_button_down(
        &self,
        owner: &Rc<dyn LayoutItemWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> FfResult<bool> {
        self.on_button_down_checked(owner, flags, point, false, || true)
    }
    pub fn on_r_button_down(
        &self,
        owner: &Rc<dyn LayoutItemWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> FfResult<bool> {
        self.on_button_down_checked(owner, flags, point, true, || true)
    }
    /// SDK annotation liveness is separate from the FF widget owner. Check
    /// that observed SDK identity after focus callbacks and before dispatch.
    pub(crate) fn on_button_down_checked(
        &self,
        owner: &Rc<dyn LayoutItemWidget>,
        flags: u32,
        point: CfxPointF,
        right: bool,
        is_alive: impl FnOnce() -> bool,
    ) -> FfResult<bool> {
        let view = self
            .doc_view
            .upgrade()
            .ok_or(FfWidgetError::DetachedDocumentView)?;
        if right {
            self.button_down(
                &view,
                owner,
                flags,
                point,
                MouseCommand::RightButtonDown,
                is_alive,
            )
        } else {
            let _scope = UpdateScope::new(&view);
            self.button_down(
                &view,
                owner,
                flags,
                point,
                MouseCommand::LeftButtonDown,
                is_alive,
            )
        }
    }
    fn button_down(
        &self,
        view: &CxfaFfDocView,
        owner: &Rc<dyn LayoutItemWidget>,
        flags: u32,
        point: CfxPointF,
        command: MouseCommand,
        is_alive: impl FnOnce() -> bool,
    ) -> FfResult<bool> {
        let Some(widget) = owner.as_ff_widget() else {
            return Ok(false);
        };
        if !widget.accepts_focus_on_button_down(
            flags,
            widget.widget().rotate_to_normal(point),
            command,
        ) {
            return Ok(false);
        }
        if view.set_focus(Some(owner.clone()))? {
            if let Some(doc) = view.doc() {
                doc.set_focus_widget(Some(owner.clone()));
            }
        }
        // Focus can run JavaScript and remove the widget or close its document.
        // Retaining its owner prevents a dangling object, but must not dispatch
        // the button to a layout item removed during those callbacks.
        if !is_alive()
            || view.doc().is_none()
            || !widget.widget().node().is_alive()
            || widget.widget().layout_item().is_none()
        {
            return Ok(false);
        }
        let normal = widget.widget().rotate_to_normal(point);
        Ok(if command == MouseCommand::RightButtonDown {
            widget.on_r_button_down(flags, normal)
        } else {
            widget.on_l_button_down(flags, normal)
        })
    }
    pub fn on_l_button_up(&self, widget: &dyn FfWidget, flags: u32, point: CfxPointF) -> bool {
        let Some(view) = self.doc_view.upgrade() else {
            return false;
        };
        let _scope = UpdateScope::new(&view);
        view.set_layout_event();
        widget.on_l_button_up(flags, widget.widget().rotate_to_normal(point))
    }
    pub fn on_l_button_dbl_clk(&self, widget: &dyn FfWidget, flags: u32, point: CfxPointF) -> bool {
        widget.on_l_button_dbl_clk(flags, widget.widget().rotate_to_normal(point))
    }
    pub fn on_mouse_move(&self, widget: &dyn FfWidget, flags: u32, point: CfxPointF) -> bool {
        widget.on_mouse_move(flags, widget.widget().rotate_to_normal(point))
    }
    pub fn on_mouse_wheel(
        &self,
        widget: &dyn FfWidget,
        flags: u32,
        point: CfxPointF,
        delta: CfxVector,
    ) -> bool {
        widget.on_mouse_wheel(flags, widget.widget().rotate_to_normal(point), delta)
    }
    pub fn on_r_button_up(&self, widget: &dyn FfWidget, flags: u32, point: CfxPointF) -> bool {
        widget.on_r_button_up(flags, widget.widget().rotate_to_normal(point))
    }
    pub fn on_r_button_dbl_clk(&self, widget: &dyn FfWidget, flags: u32, point: CfxPointF) -> bool {
        widget.on_r_button_dbl_clk(flags, widget.widget().rotate_to_normal(point))
    }
    pub fn on_key_down(&self, widget: &dyn FfWidget, key: u32, flags: u32) -> bool {
        let result = widget.on_key_down(key, flags);
        if let Some(view) = self.doc_view.upgrade() {
            let _ = view.update_doc_view();
        }
        result
    }
    pub fn on_char(&self, widget: &dyn FfWidget, ch: u32, flags: u32) -> bool {
        widget.on_char(ch, flags)
    }
    pub fn get_text(&self, widget: &dyn FfWidget) -> Vec<u32> {
        widget.get_text()
    }
    pub fn get_selected_text(&self, widget: &dyn FfWidget) -> Vec<u32> {
        if widget.can_copy() {
            widget.copy().unwrap_or_default()
        } else {
            Vec::new()
        }
    }
    pub fn paste_text(&self, widget: &dyn FfWidget, text: &[u32]) {
        if widget.can_paste() {
            widget.paste(text);
        }
    }
    pub fn select_all_text(&self, widget: &dyn FfWidget) -> bool {
        if !widget.can_select_all() {
            return false;
        }
        widget.select_all();
        true
    }
    pub fn can_undo(&self, widget: &dyn FfWidget) -> bool {
        widget.can_undo()
    }
    pub fn can_redo(&self, widget: &dyn FfWidget) -> bool {
        widget.can_redo()
    }
    pub fn undo(&self, widget: &dyn FfWidget) -> bool {
        widget.undo()
    }
    pub fn redo(&self, widget: &dyn FfWidget) -> bool {
        widget.redo()
    }
    pub fn hit_test(&self, widget: &dyn FfWidget, point: CfxPointF) -> FwlWidgetHit {
        if !widget
            .widget()
            .layout_item()
            .is_some_and(|item| item.test_status_bits(XfaWidgetStatus::kVisible))
        {
            return FwlWidgetHit::Unknown;
        }
        widget.hit_test(widget.widget().rotate_to_normal(point))
    }
    pub fn render_widget(
        &self,
        widget: &dyn FfWidget,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
        highlight: bool,
    ) -> FfResult<bool> {
        widget.render_widget(
            graphics,
            matrix,
            if highlight {
                HighlightOption::Highlight
            } else {
                HighlightOption::NoHighlight
            },
        )
    }
    pub fn has_event(&self, node: Option<&CxfaNode>, kind: XfaEventType) -> bool {
        let Some(node) =
            node.filter(|node| node.is_alive() && node.get_element_type() != XfaElement::Draw)
        else {
            return false;
        };
        if kind == XfaEventType::Unknown {
            return false;
        }
        match kind {
            XfaEventType::Calculate => {
                node.get_child(0, XfaElement::Calculate, false)
                    .is_some_and(|calculate| {
                        calculate.get_child(0, XfaElement::Script, false).is_some()
                    })
            }
            XfaEventType::Validate => node
                .get_child(0, XfaElement::Validate, false)
                .is_some_and(|validate| validate.get_child(0, XfaElement::Script, false).is_some()),
            _ => !node
                .events_by_activity(
                    EVENT_ACTIVITY
                        .get(kind as usize)
                        .copied()
                        .unwrap_or(XfaAttributeValue::Unknown),
                    false,
                )
                .is_empty(),
        }
    }
    pub fn process_event(
        &self,
        node: Option<&CxfaNode>,
        event: Option<&mut CxfaEventParam>,
    ) -> XfaEventError {
        let Some(event) = event.filter(|event| event.event_type != XfaEventType::Unknown) else {
            return XfaEventError::kNotExist;
        };
        let Some(node) =
            node.filter(|node| node.is_alive() && node.get_element_type() != XfaElement::Draw)
        else {
            return XfaEventError::kNotExist;
        };
        let Some(view) = self.doc_view.upgrade() else {
            return XfaEventError::kNotExist;
        };
        match event.event_type {
            XfaEventType::Calculate => node.process_calculate(&view),
            XfaEventType::Validate => {
                if view.doc().is_some_and(|doc| doc.is_validations_enabled()) {
                    node.process_validate(&view, 0)
                } else {
                    XfaEventError::kDisabled
                }
            }
            XfaEventType::InitCalculate => {
                let Some(calculate) = node.get_child(0, XfaElement::Calculate, false) else {
                    return XfaEventError::kNotExist;
                };
                if node.is_user_interactive() {
                    return XfaEventError::kDisabled;
                }
                node.execute_script(
                    &view,
                    calculate.get_child(0, XfaElement::Script, false).as_ref(),
                    event,
                )
            }
            kind => node.process_event(
                &view,
                EVENT_ACTIVITY
                    .get(kind as usize)
                    .copied()
                    .unwrap_or(XfaAttributeValue::Unknown),
                event,
            ),
        }
    }
}
impl FfWidgetHandler for CxfaFfWidgetHandler {
    fn has_event(&self, node: &CxfaNode, kind: XfaEventType) -> bool {
        self.has_event(Some(node), kind)
    }
    fn process_event(&self, node: &CxfaNode, event: &mut CxfaEventParam) -> XfaEventError {
        self.process_event(Some(node), Some(event))
    }
}
#[cfg(test)]
#[path = "cxfa_ffwidgethandler_tests.rs"]
mod tests;
