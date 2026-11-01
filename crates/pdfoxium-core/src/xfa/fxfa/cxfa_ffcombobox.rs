// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_ffcombobox.{h,cpp}`. FFDropDown's item virtuals use the actual FfWidget
//! choice hooks; no second control or parallel item collection is maintained.
use super::{
    cxfa_eventparam::{CxfaEventParam, XfaEventType},
    cxfa_fffield::CxfaFfField,
    cxfa_ffwidget::{CxfaFfWidget, FfResult, FfWidget, FocusOption},
    fxfa_basic::XfaAttributeValue as V,
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::{cxfa_localevalue::XfaValuePicture, cxfa_node::CxfaNode},
};
use crate::{
    fpdfdoc::cpdf_formfield::FormFieldType,
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fwl::{
            cfwl_combobox::*,
            cfwl_edit::{FWL_STYLEEXT_EDT_AUTO_HSCROLL, FWL_STYLEEXT_EDT_READ_ONLY},
            cfwl_event::{EventData, FwlEvent},
            cfwl_message::{FwlMessage, MouseCommand},
            cfwl_widget::{FwlWidget, WidgetAdapter, WidgetRef},
            ifwl_widgetdelegate::WidgetDelegate,
        },
    },
};
use std::{
    cell::RefCell,
    ops::Deref,
    rc::{Rc, Weak},
};
pub struct CxfaFfComboBox {
    field: CxfaFfField,
    weak_self: Weak<Self>,
    control: RefCell<Option<Rc<CfwlComboBox>>>,
    old_delegate: RefCell<Option<Rc<dyn WidgetDelegate>>>,
    new_value: RefCell<Rc<[u32]>>,
}
impl CxfaFfComboBox {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        Rc::new_cyclic(|weak| Self {
            field: CxfaFfField::new_base(node),
            weak_self: weak.clone(),
            control: RefCell::new(None),
            old_delegate: RefCell::new(None),
            new_value: RefCell::new(Rc::from([])),
        })
    }
    pub fn control(&self) -> Option<Rc<CfwlComboBox>> {
        self.control.borrow().clone()
    }
    pub fn get_current_text(&self) -> Vec<u32> {
        let Some(control) = self.control() else {
            return Vec::new();
        };
        let text = control.get_edit_text();
        let selected = control.get_cur_sel();
        if selected >= 0 && control.get_text_by_index(selected) == text {
            self.field
                .base
                .node()
                .get_choice_list_item(selected, true)
                .unwrap_or_default()
        } else {
            text
        }
    }
    fn alignment(&self) -> u32 {
        let Some(para) = self.field.base.node().get_para_if_exists() else {
            return 0;
        };
        let horizontal = match para.para_get_horizontal_align() {
            V::Center => STYLEEXT_CMB_EDIT_HCENTER | STYLEEXT_CMB_LIST_ITEM_CENTER_ALIGN,
            V::Justify => STYLEEXT_CMB_EDIT_JUSTIFIED,
            _ => 0,
        };
        horizontal
            | match para.para_get_vertical_align() {
                V::Middle => STYLEEXT_CMB_EDIT_VCENTER,
                V::Bottom => STYLEEXT_CMB_EDIT_VFAR,
                _ => STYLEEXT_CMB_EDIT_VNEAR,
            }
    }
    fn fwl_event_sel_change(&self, parameter: &mut CxfaEventParam) {
        let Some(control) = self.control() else {
            return;
        };
        parameter.event_type = XfaEventType::Change;
        parameter.prev_text = control.get_edit_text();
        if let Some(host) = self.field.base.doc_view() {
            host.process_event(self.field.base.node(), V::Change, parameter);
        }
    }
    pub fn on_text_changed(&self, changed: &[u32]) {
        let mut event = CxfaEventParam::new(XfaEventType::Unknown);
        event.prev_text = self.field.base.node().get_raw_value();
        event.change = changed.to_vec();
        self.fwl_event_sel_change(&mut event);
    }
    pub fn on_select_changed(&self, left_button_up: bool) {
        let mut event = CxfaEventParam::new(XfaEventType::Unknown);
        event.prev_text = self.field.base.node().get_raw_value();
        self.fwl_event_sel_change(&mut event);
        if self.field.base.node().is_choice_list_commit_on_select() && left_button_up {
            if let Some(host) = self.field.base.doc_view() {
                let _ = host.set_focus_node(None);
            }
        }
    }
    pub fn on_pre_open(&self) {
        if let Some(host) = self.field.base.doc_view() {
            host.process_event(
                self.field.base.node(),
                V::PreOpen,
                &mut CxfaEventParam::new(XfaEventType::PreOpen),
            );
        }
    }
    pub fn on_post_open(&self) {
        if let Some(host) = self.field.base.doc_view() {
            host.process_event(
                self.field.base.node(),
                V::PostOpen,
                &mut CxfaEventParam::new(XfaEventType::PostOpen),
            );
        }
    }
    pub fn set_item_state(&self, index: i32, selected: bool) {
        if let Some(control) = self.control() {
            control.set_cur_sel(if selected { index } else { -1 });
            control.update();
            self.field.base.invalidate_rect();
        }
    }
}
impl Deref for CxfaFfComboBox {
    type Target = CxfaFfField;
    fn deref(&self) -> &Self::Target {
        &self.field
    }
}
impl LayoutItemWidget for CxfaFfComboBox {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.field.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfComboBox {
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
impl FfWidget for CxfaFfComboBox {
    fn widget(&self) -> &CxfaFfWidget {
        &self.field.base
    }
    fn as_field(&self) -> Option<&CxfaFfField> {
        Some(&self.field)
    }
    fn get_bbox(&self, focus: FocusOption) -> CfxRectF {
        if focus == FocusOption::DrawFocus {
            CfxRectF::default()
        } else {
            self.field.base.get_bbox(FocusOption::DoNotDrawFocus)
        }
    }
    fn accepts_focus_on_button_down(
        &self,
        _flags: u32,
        point: CfxPointF,
        _command: MouseCommand,
    ) -> bool {
        self.field.base.node().is_open_access()
            && self
                .field
                .base
                .doc_view()
                .is_some_and(|h| h.is_interactive())
            && self
                .control()
                .is_some_and(|c| c.get_bbox().contains_point(point))
    }
    fn load_widget(&self) -> FfResult<bool> {
        if self.control().is_some() {
            return Ok(false);
        }
        let host = self.field.base.require_doc_view()?;
        let app = host.fwl_app();
        let control = CfwlComboBox::new(&app);
        self.old_delegate.replace(control.base().get_delegate());
        let widget: WidgetRef = control.clone();
        let owner: Rc<dyn FfWidget> = self.weak_self.upgrade().expect("live FF combobox");
        self.field.attach_normal_widget(&owner, widget.clone());
        self.control.replace(Some(control.clone()));
        app.note_driver()
            .register_event_target(&widget, Some(&widget));
        {
            let _lock = control.base().scoped_update_lock();
            // SetCurSel populates the child edit only after dropdown style is
            // installed. Apply the node's UI properties first so a freshly
            // loaded/recreated widget cannot later commit an empty display
            // over its already selected model value.
            self.update_widget_property();
            for label in self.field.base.node().get_choice_list_items(false) {
                control.add_string(&label);
            }
            if let Some(&selected) = self.field.base.node().get_selected_items().first() {
                control.set_cur_sel(selected);
            } else {
                control.set_edit_text(&self.field.base.node().get_raw_value());
            }
        }
        self.field.load_widget()
    }
    fn update_widget_property(&self) {
        let Some(control) = self.control() else {
            return;
        };
        let node = self.field.base.node();
        let mut style = self.field.update_ui_property();
        let mut edit = FWL_STYLEEXT_EDT_READ_ONLY;
        if node.is_choice_list_allow_text_entry() {
            edit &= !FWL_STYLEEXT_EDT_READ_ONLY;
            style |= STYLEEXT_CMB_DROP_DOWN;
        }
        if !node.is_open_access()
            || self
                .field
                .base
                .doc_view()
                .is_some_and(|h| !h.is_interactive())
        {
            edit |= FWL_STYLEEXT_EDT_READ_ONLY;
            style |= STYLEEXT_CMB_READ_ONLY;
        }
        style |= self.alignment();
        control.modify_style_exts(style, u32::MAX);
        if !node.is_horizontal_scroll_policy_off() {
            edit |= FWL_STYLEEXT_EDT_AUTO_HSCROLL;
        }
        control.edit_modify_style_exts(edit, u32::MAX);
    }
    fn on_r_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        if !self.field.on_r_button_up(flags, point) {
            return false;
        }
        if let Some(host) = self.field.base.doc_view() {
            host.popup_menu(self, point);
        }
        true
    }
    fn on_kill_focus(&self, new: Option<&dyn FfWidget>) -> FfResult<bool> {
        if !self.field.process_committed_data(self)? {
            self.update_fwl_data();
        }
        // A null successor is a real focus-clear operation. Native rejects
        // it before its base handler, preventing commit-on-select from clearing
        // focus through the document view.
        self.field.on_kill_focus(new)
    }
    fn commit_data(&self) -> bool {
        let value = self.new_value.borrow().clone();
        self.field
            .base
            .node()
            .set_value(XfaValuePicture::Raw, &value)
    }
    fn is_data_changed(&self) -> bool {
        let value = self.get_current_text();
        if self.field.base.node().get_raw_value() == value {
            return false;
        }
        self.new_value.replace(Rc::from(value));
        true
    }
    fn update_fwl_data(&self) {
        let Some(control) = self.control() else {
            return;
        };
        if let Some(&selected) = self.field.base.node().get_selected_items().first() {
            control.set_cur_sel(selected);
        } else {
            control.set_cur_sel(-1);
            control.set_edit_text(&self.field.base.node().get_raw_value());
        }
        control.update();
    }
    fn can_undo(&self) -> bool {
        self.field.base.node().is_choice_list_allow_text_entry()
            && self.control().is_some_and(|c| c.edit_can_undo())
    }
    fn can_redo(&self) -> bool {
        self.field.base.node().is_choice_list_allow_text_entry()
            && self.control().is_some_and(|c| c.edit_can_redo())
    }
    fn can_copy(&self) -> bool {
        self.control().is_some_and(|c| c.edit_can_copy())
    }
    fn can_cut(&self) -> bool {
        self.field.base.node().is_open_access()
            && self.field.base.node().is_choice_list_allow_text_entry()
            && self.control().is_some_and(|c| c.edit_can_cut())
    }
    fn can_paste(&self) -> bool {
        self.field.base.node().is_choice_list_allow_text_entry()
            && self.field.base.node().is_open_access()
    }
    fn can_select_all(&self) -> bool {
        self.control().is_some_and(|c| c.edit_can_select_all())
    }
    fn undo(&self) -> bool {
        self.field.base.node().is_choice_list_allow_text_entry()
            && self.control().is_some_and(|c| c.edit_undo())
    }
    fn redo(&self) -> bool {
        self.field.base.node().is_choice_list_allow_text_entry()
            && self.control().is_some_and(|c| c.edit_redo())
    }
    fn copy(&self) -> Option<Vec<u32>> {
        self.control()?.edit_copy()
    }
    fn cut(&self) -> Option<Vec<u32>> {
        if !self.field.base.node().is_choice_list_allow_text_entry() {
            return None;
        }
        self.control()?.edit_cut()
    }
    fn paste(&self, text: &[u32]) -> bool {
        self.field.base.node().is_choice_list_allow_text_entry()
            && self.control().is_some_and(|c| c.edit_paste(text))
    }
    fn select_all(&self) {
        if let Some(control) = self.control() {
            control.edit_select_all();
        }
    }
    fn delete(&self) {
        if let Some(control) = self.control() {
            control.edit_delete();
        }
    }
    fn de_select(&self) {
        if let Some(control) = self.control() {
            control.edit_de_select();
        }
    }
    fn get_text(&self) -> Vec<u32> {
        self.get_current_text()
    }
    fn insert_choice_item(&self, label: &[u32], _index: i32) -> bool {
        let Some(control) = self.control() else {
            return false;
        };
        control.add_string(label);
        control.update();
        self.field.base.invalidate_rect();
        true
    }
    fn delete_choice_item(&self, index: i32) -> bool {
        let Some(control) = self.control() else {
            return false;
        };
        if index < 0 {
            control.remove_all();
        } else {
            control.remove_at(index);
        }
        control.update();
        self.field.base.invalidate_rect();
        true
    }
    fn open_drop_down_list(&self) -> bool {
        let Some(control) = self.control() else {
            return false;
        };
        control.show_drop_down_list();
        true
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
        match &event.data {
            EventData::SelectChanged { left_button_up, .. } => {
                self.on_select_changed(*left_button_up)
            }
            EventData::EditChanged => self.on_text_changed(&[]),
            EventData::PreDropDown => self.on_pre_open(),
            EventData::PostDropDown => self.on_post_open(),
            _ => {}
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
        FormFieldType::XfaComboBox
    }
}
