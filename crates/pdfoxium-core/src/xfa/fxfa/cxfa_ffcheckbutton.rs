// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_ffcheckbutton.{h,cpp}`. Real FWL check state commits through model
//! binding and real document-view change/click/validation/calculation services.
use super::{
    cxfa_eventparam::{CxfaEventParam, XfaEventType},
    cxfa_fffield::{CxfaFfField, ShapeOption},
    cxfa_ffwidget::{rect_without_margin, CxfaFfWidget, FfResult, FfWidget, HighlightOption},
    fxfa_basic::XfaAttributeValue as V,
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::{cxfa_node::CxfaNode, cxfa_node_check::XfaCheckState},
};
use crate::{
    fpdfdoc::cpdf_formfield::FormFieldType,
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fwl::{
            cfwl_checkbox::*,
            cfwl_event::{EventData, FwlEvent},
            cfwl_message::{FwlMessage, MouseCommand},
            cfwl_widget::{FwlWidget, WidgetAdapter, WidgetRef, WidgetState},
            ifwl_widgetdelegate::WidgetDelegate,
        },
    },
};
use std::{
    cell::{Cell, RefCell},
    ops::Deref,
    rc::{Rc, Weak},
};
pub struct CxfaFfCheckButton {
    field: CxfaFfField,
    button: CxfaNode,
    weak_self: Weak<Self>,
    control: RefCell<Option<Rc<CfwlCheckBox>>>,
    old_delegate: RefCell<Option<Rc<dyn WidgetDelegate>>>,
    check_box_rect: Cell<CfxRectF>,
}
impl CxfaFfCheckButton {
    pub fn new(node: &CxfaNode, button: &CxfaNode) -> Rc<Self> {
        Rc::new_cyclic(|weak| Self {
            field: CxfaFfField::new_base(node),
            button: button.clone(),
            weak_self: weak.clone(),
            control: RefCell::new(None),
            old_delegate: RefCell::new(None),
            check_box_rect: Cell::new(CfxRectF::default()),
        })
    }
    pub fn control(&self) -> Option<Rc<CfwlCheckBox>> {
        self.control.borrow().clone()
    }
    pub fn check_box_rect(&self) -> CfxRectF {
        self.check_box_rect.get()
    }
    fn fwl_state_to_xfa_state(&self) -> XfaCheckState {
        let Some(control) = self.control() else {
            return XfaCheckState::Off;
        };
        let state = control.base().get_states();
        if state.contains(WidgetState::CHECKBOX_CHECKED) {
            XfaCheckState::On
        } else if state.contains(WidgetState::CHECKBOX_NEUTRAL) {
            XfaCheckState::Neutral
        } else {
            XfaCheckState::Off
        }
    }
    pub fn set_fwl_check_state(&self, state: XfaCheckState) {
        if let Some(control) = self.control() {
            // Native setters can leave both states set (and Off retains a
            // prior Neutral). Each model state instead replaces the whole mask.
            control
                .base()
                .clear_states(WidgetState::CHECKBOX_CHECK_MASK);
            match state {
                XfaCheckState::On => control.base().set_states(WidgetState::CHECKBOX_CHECKED),
                XfaCheckState::Neutral => control.base().set_states(WidgetState::CHECKBOX_NEUTRAL),
                _ => {}
            }
        }
    }
    fn cap_left_right_placement(&self, margin: Option<&CxfaNode>) {
        let mut caption = self.field.caption_rect();
        rect_without_margin(&mut caption, margin);
        if caption.height < 0. {
            caption.top += caption.height;
        }
        if caption.width < 0. {
            caption.left += caption.width;
            caption.width = -caption.width;
        }
        self.field.set_caption_rect(caption);
    }
    fn add_ui_margin(&self, placement: V) {
        let margin = self.field.base.node().get_ui_margin();
        let mut ui = self.field.ui_rect();
        ui.top -= margin.top / 2. - margin.height / 2.;
        let horizontal = margin.left + margin.width;
        let vertical = margin.top + margin.height;
        if ui.width < horizontal {
            if matches!(placement, V::Right | V::Left) {
                ui.left -= horizontal - ui.width;
            } else {
                ui.left -= 2. * (horizontal - ui.width);
            }
            ui.width += 2. * (horizontal - ui.width);
        }
        if ui.height < vertical {
            if placement == V::Right {
                ui.left -= vertical - ui.height;
            }
            ui.top -= vertical - ui.height;
            ui.height += 2. * (vertical - ui.height);
        }
        self.field.set_ui_rect(ui);
    }
}
impl Deref for CxfaFfCheckButton {
    type Target = CxfaFfField;
    fn deref(&self) -> &Self::Target {
        &self.field
    }
}
impl LayoutItemWidget for CxfaFfCheckButton {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.field.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfCheckButton {
    fn get_rotate_matrix(&self) -> CfxMatrix {
        self.field.base.get_rotate_matrix()
    }
    fn display_caret(&self, visible: bool, anchor: Option<CfxRectF>) {
        if let Some(host) = self.field.base.doc_view() {
            host.display_caret(self, visible, anchor);
        }
    }
    fn get_border_color_and_thickness(&self) -> (u32, f32) {
        self.field.base.get_border_color_and_thickness()
    }
}
impl FfWidget for CxfaFfCheckButton {
    fn widget(&self) -> &CxfaFfWidget {
        &self.field.base
    }
    fn as_field(&self) -> Option<&CxfaFfField> {
        Some(&self.field)
    }
    fn load_widget(&self) -> FfResult<bool> {
        if self.control().is_some() {
            return Ok(false);
        }
        let host = self.field.base.require_doc_view()?;
        let app = host.fwl_app();
        let control = CfwlCheckBox::new(&app);
        self.old_delegate.replace(control.base().get_delegate());
        let widget: WidgetRef = control.clone();
        let owner: Rc<dyn FfWidget> = self.weak_self.upgrade().expect("live FF checkbox");
        self.field.attach_normal_widget(&owner, widget.clone());
        self.control.replace(Some(control.clone()));
        app.note_driver()
            .register_event_target(&widget, Some(&widget));
        if self.field.base.node().is_radio_button() {
            control
                .base()
                .modify_style_exts(STYLEEXT_CKB_RADIO_BUTTON, u32::MAX);
        }
        {
            let _lock = control.base().scoped_update_lock();
            self.update_widget_property();
            self.set_fwl_check_state(self.field.base.node().get_check_state());
        }
        host.load_caption(self.field.base.node())?;
        self.perform_layout()?;
        Ok(true)
    }
    fn update_widget_property(&self) {
        let Some(control) = self.control() else {
            return;
        };
        control.set_box_size(self.field.base.node().get_check_button_size());
        let mut style = match self.button.check_button_get_mark() {
            V::Check => STYLEEXT_CKB_SIGN_SHAPE_CHECK,
            V::Circle => STYLEEXT_CKB_SIGN_SHAPE_CIRCLE,
            V::Cross => STYLEEXT_CKB_SIGN_SHAPE_CROSS,
            V::Diamond => STYLEEXT_CKB_SIGN_SHAPE_DIAMOND,
            V::Square => STYLEEXT_CKB_SIGN_SHAPE_SQUARE,
            V::Star => STYLEEXT_CKB_SIGN_SHAPE_STAR,
            _ => {
                if self.button.check_button_is_round() {
                    STYLEEXT_CKB_SIGN_SHAPE_CIRCLE
                } else {
                    STYLEEXT_CKB_SIGN_SHAPE_CROSS
                }
            }
        };
        if self.button.check_button_is_allow_neutral() {
            style |= STYLEEXT_CKB_3_STATE;
        }
        control
            .base()
            .modify_style_exts(style, STYLEEXT_CKB_SIGN_SHAPE_MASK | STYLEEXT_CKB_3_STATE);
    }
    fn perform_layout(&self) -> FfResult<()> {
        self.field.base.perform_layout();
        let node = self.field.base.node();
        let size = node.get_check_button_size();
        let mut rect = self.field.base.get_rect_without_rotate();
        rect_without_margin(&mut rect, node.get_margin_if_exists().as_ref());
        let caption = node.get_caption_if_exists();
        let mut placement = V::Unknown;
        let mut reserve = 0.;
        if let Some(caption) = caption.as_ref().filter(|c| c.caption_is_visible()) {
            self.field.set_caption_rect(rect);
            placement = caption.caption_get_placement_type();
            reserve = caption.caption_get_reserve();
            if reserve <= 0. {
                reserve = if matches!(placement, V::Top | V::Bottom) {
                    rect.height - size
                } else {
                    rect.width - size
                };
            }
        }
        let para = node.get_para_if_exists();
        let mut horizontal = para
            .as_ref()
            .map_or(V::Left, CxfaNode::para_get_horizontal_align);
        let vertical = para
            .as_ref()
            .map_or(V::Top, CxfaNode::para_get_vertical_align);
        self.field.set_ui_rect(rect);
        let margin = caption
            .as_ref()
            .and_then(CxfaNode::caption_get_margin_if_exists);
        let mut cap = self.field.caption_rect();
        let mut ui = rect;
        match placement {
            V::Left => {
                cap.width = reserve;
                self.field.set_caption_rect(cap);
                self.cap_left_right_placement(margin.as_ref());
                ui.width -= reserve;
                ui.left += reserve;
            }
            V::Top => {
                cap.height = reserve;
                rect_without_margin(&mut cap, margin.as_ref());
                self.field.set_caption_rect(cap);
                ui.height -= reserve;
                ui.top += reserve;
            }
            V::Right => {
                cap.left = cap.right() - reserve;
                cap.width = reserve;
                self.field.set_caption_rect(cap);
                self.cap_left_right_placement(margin.as_ref());
                ui.width -= reserve;
            }
            V::Bottom => {
                cap.top = cap.bottom() - reserve;
                cap.height = reserve;
                rect_without_margin(&mut cap, margin.as_ref());
                self.field.set_caption_rect(cap);
                ui.height -= reserve;
            }
            V::Inline => {}
            _ => horizontal = V::Right,
        }
        match horizontal {
            V::Center => ui.left += (ui.width - size) / 2.,
            V::Right => ui.left = ui.right() - size,
            _ => {}
        }
        match vertical {
            V::Middle => ui.top += (ui.height - size) / 2.,
            V::Bottom => ui.top = ui.bottom() - size,
            _ => {}
        }
        ui.width = size;
        ui.height = size;
        self.field.set_ui_rect(ui);
        self.add_ui_margin(placement);
        self.check_box_rect.set(self.field.ui_rect());
        let border_margin = node.get_ui_border().and_then(|b| b.get_margin_if_exists());
        let mut ui = self.field.ui_rect();
        rect_without_margin(&mut ui, border_margin.as_ref());
        ui.normalize();
        self.field.set_ui_rect(ui);
        self.field.layout_caption()?;
        self.field.set_fwl_rect();
        if let Some(control) = self.control() {
            control.update();
        }
        Ok(())
    }
    fn render_widget(
        &self,
        g: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
        highlight: HighlightOption,
    ) -> FfResult<bool> {
        if !self.field.base.has_visible_status() {
            return Ok(true);
        }
        let mut rotate = self.get_rotate_matrix();
        rotate.concat(matrix);
        let round = self.button.check_button_is_round();
        let mut success = self.field.base.render_widget(g, rotate, highlight)?;
        success &= self.field.base.draw_border(
            g,
            self.field.base.node().get_ui_border().as_ref(),
            self.field.ui_rect(),
            rotate,
            round,
        );
        success &= self.field.render_caption(g, rotate)?;
        self.field.draw_highlight(
            g,
            rotate,
            highlight,
            if round {
                ShapeOption::Round
            } else {
                ShapeOption::Square
            },
        );
        if let Some(control) = self.control() {
            let rect = self.check_box_rect.get();
            let mut translated = CfxMatrix::new(1., 0., 0., 1., rect.left, rect.top);
            translated.concat(rotate);
            let widget: WidgetRef = control;
            self.field
                .base
                .require_doc_view()?
                .fwl_app()
                .widget_mgr()
                .on_draw_widget(&widget, g, translated);
        }
        Ok(success)
    }
    fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        let Some(control) = self.control() else {
            return false;
        };
        if !self.field.base.is_button_down() {
            return false;
        }
        self.field.base.set_button_down(false);
        let widget: WidgetRef = control;
        self.field
            .send_message_to_fwl_widget(&mut FwlMessage::mouse(
                Some(widget),
                MouseCommand::LeftButtonUp,
                flags,
                self.field.fwl_to_client(point),
            ))
    }
    fn commit_data(&self) -> bool {
        self.field
            .base
            .node()
            .set_check_state(self.fwl_state_to_xfa_state())
    }
    fn is_data_changed(&self) -> bool {
        self.field.base.node().get_check_state() != self.fwl_state_to_xfa_state()
    }
    fn update_fwl_data(&self) {
        if let Some(control) = self.control() {
            self.set_fwl_check_state(self.field.base.node().get_check_state());
            control.update();
        }
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        let old = self.old_delegate.borrow().clone();
        if let Some(old) = old {
            old.on_process_message(message);
        } else if let Some(control) = self.control() {
            control.on_process_message(message);
        }
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        self.field.on_process_event(event);
        if matches!(event.data, EventData::CheckStateChanged) {
            let node = self.field.base.node();
            let mut parameter = CxfaEventParam::new(XfaEventType::Change);
            parameter.prev_text = node.get_raw_value();
            let group = node.get_excl_group_if_exists();
            if let Some(host) = self.field.base.doc_view() {
                if self.field.process_committed_data(self).unwrap_or(false) {
                    if let Some(group) = &group {
                        host.add_validate_node(group);
                        host.add_calculate_node(group);
                        host.process_event(group, V::Change, &mut parameter);
                    }
                    host.process_event(node, V::Change, &mut parameter);
                } else {
                    self.set_fwl_check_state(node.get_check_state());
                }
                if let Some(group) = &group {
                    host.process_event(group, V::Click, &mut parameter);
                }
                host.process_event(node, V::Click, &mut parameter);
            } else {
                self.set_fwl_check_state(node.get_check_state());
            }
        }
        let old = self.old_delegate.borrow().clone();
        if let Some(old) = old {
            old.on_process_event(event);
        } else if let Some(control) = self.control() {
            control.on_process_event(event);
        }
    }
    fn on_draw_widget(&self, g: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        let old = self.old_delegate.borrow().clone();
        if let Some(old) = old {
            old.on_draw_widget(g, matrix);
        } else if let Some(control) = self.control() {
            control.draw_widget(g, matrix);
        }
    }
    fn get_form_field_type(&self) -> FormFieldType {
        FormFieldType::XfaCheckBox
    }
}
