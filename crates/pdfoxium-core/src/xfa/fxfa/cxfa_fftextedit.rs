// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_fftextedit.{h,cpp}`: actual editable FWL control, scoped XFA change
//! cancellation, picture switching and model commit. The former native
//! delegate is retained; an absent explicit delegate means the real FWL
//! control's own virtual implementation, not an empty callback substitute.
use super::{
    cxfa_eventparam::{CxfaEventParam, XfaEventType},
    cxfa_fffield::CxfaFfField,
    cxfa_ffwidget::{CxfaFfWidget, FfResult, FfWidget, FfWidgetError},
    fxfa::{AlertButton, AlertIcon, XfaWidgetStatus as S},
    fxfa_basic::{XfaAttributeValue as V, XfaElement as E},
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::{
        cxfa_localevalue::XfaValuePicture,
        cxfa_node::{CxfaNode, XfaFfWidgetType as Kind},
    },
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fwl::{
            cfwl_edit::*,
            cfwl_event::{EventData, FwlEvent},
            cfwl_message::{FwlMessage, MouseCommand},
            cfwl_widget::{Properties, WidgetAdapter, WidgetRef, WidgetStyle},
            ifwl_widgetdelegate::WidgetDelegate,
        },
    },
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
pub struct CxfaFfTextEdit {
    pub field: CxfaFfField,
    owner: RefCell<Option<Weak<dyn FfWidget>>>,
    old_delegate: RefCell<Option<Rc<dyn WidgetDelegate>>>,
}
impl std::ops::Deref for CxfaFfTextEdit {
    type Target = CxfaFfField;
    fn deref(&self) -> &Self::Target {
        &self.field
    }
}
impl CxfaFfTextEdit {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        let value = Rc::new(Self::new_base(node));
        let owner: Rc<dyn FfWidget> = value.clone();
        value.bind_owner(&owner);
        value
    }
    pub fn new_base(node: &CxfaNode) -> Self {
        Self {
            field: CxfaFfField::new_base(node),
            owner: RefCell::new(None),
            old_delegate: RefCell::new(None),
        }
    }
    pub fn bind_owner(&self, owner: &Rc<dyn FfWidget>) {
        *self.owner.borrow_mut() = Some(Rc::downgrade(owner));
    }
    pub fn owner(&self) -> Option<Rc<dyn FfWidget>> {
        self.owner.borrow().as_ref()?.upgrade()
    }
    pub fn attach_edit_widget(&self, owner: &Rc<dyn FfWidget>, widget: WidgetRef) {
        self.bind_owner(owner);
        *self.old_delegate.borrow_mut() = widget.base().get_delegate();
        if let Some(app) = widget.base().app() {
            app.note_driver()
                .register_event_target(&widget, Some(&widget));
        }
        self.field.attach_normal_widget(owner, widget);
    }
    pub fn pre_finalize(&self) {
        if let Some(widget) = self.field.normal_widget() {
            if let Some(app) = widget.base().app() {
                app.note_driver().unregister_event_target(&widget);
            }
        }
    }
    pub fn load_widget(&self) -> FfResult<bool> {
        self.load_widget_with_text_first(false)
    }
    pub fn load_widget_with_text_first(&self, text_first: bool) -> FfResult<bool> {
        let host = self.field.base.require_doc_view()?;
        let owner = self.owner().ok_or(FfWidgetError::DetachedDocumentView)?;
        let edit = CfwlEdit::new(&host.fwl_app(), Properties::default(), None);
        let widget: WidgetRef = edit.clone();
        self.attach_edit_widget(&owner, widget.clone());
        {
            let _lock = widget.base().scoped_update_lock();
            if text_first {
                edit.set_text(&self.field.base.node().get_value(XfaValuePicture::Display));
            }
            owner.update_widget_property();
            if !text_first {
                edit.set_text(&self.field.base.node().get_value(XfaValuePicture::Display));
            }
        }
        self.field
            .load_widget_with_layout(|| owner.perform_layout())
    }
    pub fn update_widget_property(&self) {
        let Some(widget) = self.field.normal_widget() else {
            return;
        };
        let Some(edit) = widget.as_edit() else {
            return;
        };
        let node = self.field.base.node();
        let mut styles = WidgetStyle::NONE;
        let mut extended = FWL_STYLEEXT_EDT_SHOW_SCROLLBAR_FOCUS
            | FWL_STYLEEXT_EDT_OUTER_SCROLLBAR
            | self.field.update_ui_property();
        if node.is_multi_line() {
            extended |= FWL_STYLEEXT_EDT_MULTI_LINE | FWL_STYLEEXT_EDT_WANT_RETURN;
            if !node.is_vertical_scroll_policy_off() {
                styles |= WidgetStyle::VSCROLL;
                extended |= FWL_STYLEEXT_EDT_AUTO_VSCROLL;
            }
        } else if !node.is_horizontal_scroll_policy_off() {
            extended |= FWL_STYLEEXT_EDT_AUTO_HSCROLL;
        }
        if !node.is_open_access()
            || self
                .field
                .base
                .doc_view()
                .is_some_and(|host| !host.is_interactive())
        {
            extended |= FWL_STYLEEXT_EDT_READ_ONLY | FWL_STYLEEXT_EDT_MULTI_LINE;
        }
        let (kind, mut max) = node.get_max_chars();
        if kind == E::ExData {
            max = 0;
        }
        match node.get_number_of_cells() {
            None => edit.set_limit(max),
            Some(0) => {
                extended |= FWL_STYLEEXT_EDT_COMB_TEXT;
                edit.set_limit(if max > 0 { max } else { 1 });
            }
            Some(count) => {
                extended |= FWL_STYLEEXT_EDT_COMB_TEXT;
                edit.set_limit(count);
            }
        }
        extended |= self.get_alignment();
        widget.base().set_styles(styles);
        widget.modify_style_exts(extended, u32::MAX);
    }
    pub fn accepts_focus_on_button_down(
        &self,
        _flags: u32,
        point: CfxPointF,
        command: MouseCommand,
    ) -> bool {
        if command == MouseCommand::RightButtonDown && !self.field.base.node().is_open_access() {
            return false;
        }
        self.field.pt_in_active_rect(point)
    }
    fn prepare_button_down(&self) {
        if !self.field.base.is_focused() {
            self.field.base.modify_status(S::kFocused, S::kNone);
            if let Some(owner) = self.owner() {
                owner.update_fwl_data();
            } else {
                self.update_fwl_data();
            }
            self.field.base.invalidate_rect();
        }
        self.field.base.set_button_down(true);
    }
    pub fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.prepare_button_down();
        let mut message = FwlMessage::mouse(
            self.field.normal_widget(),
            MouseCommand::LeftButtonDown,
            flags,
            self.field.fwl_to_client(point),
        );
        self.field.send_message_to_fwl_widget(&mut message)
    }
    pub fn on_r_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.prepare_button_down();
        let mut message = FwlMessage::mouse(
            None,
            MouseCommand::RightButtonDown,
            flags,
            self.field.fwl_to_client(point),
        );
        self.field.send_message_to_fwl_widget(&mut message)
    }
    pub fn on_r_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        if !self.field.on_r_button_up(flags, point) {
            return false;
        }
        let Some(host) = self.field.base.doc_view() else {
            return false;
        };
        let Some(owner) = self.owner() else {
            return false;
        };
        host.popup_menu(owner.as_ref(), point);
        true
    }
    pub fn on_set_focus(&self, old: Option<&dyn FfWidget>) -> FfResult<bool> {
        self.field
            .base
            .modify_status(S::kNone, S::kTextEditValueChanged);
        if !self.field.base.is_focused() {
            self.field.base.modify_status(S::kFocused, S::kNone);
            if let Some(owner) = self.owner() {
                owner.update_fwl_data();
            } else {
                self.update_fwl_data();
            }
            self.field.base.invalidate_rect();
        }
        if !self.field.base.on_set_focus(old)? {
            return Ok(false);
        }
        Ok(self
            .field
            .send_message_to_fwl_widget(&mut FwlMessage::set_focus(self.field.normal_widget())))
    }
    pub fn on_kill_focus(&self, new: Option<&dyn FfWidget>) -> FfResult<bool> {
        self.field
            .send_message_to_fwl_widget(&mut FwlMessage::kill_focus(self.field.normal_widget()));
        self.field.base.modify_status(S::kNone, S::kFocused);
        self.field.set_edit_scroll_offset();
        if let Some(owner) = self.owner() {
            self.field.process_committed_data(owner.as_ref())?;
        }
        if let Some(owner) = self.owner() {
            owner.update_fwl_data();
        } else {
            self.update_fwl_data();
        }
        self.field.base.invalidate_rect();
        if !self.field.base.on_kill_focus(new)? {
            return Ok(false);
        }
        self.field
            .base
            .modify_status(S::kNone, S::kTextEditValueChanged);
        Ok(true)
    }
    pub fn commit_data(&self) -> bool {
        let Some(widget) = self.field.normal_widget() else {
            return false;
        };
        let Some(edit) = widget.as_edit() else {
            return false;
        };
        let text = edit.get_text();
        let node = self.field.base.node();
        if node.set_value(XfaValuePicture::Edit, &text) {
            if let Some(host) = self.field.base.doc_view() {
                let owner = self.owner();
                host.update_ui_display(node, owner.as_deref());
            }
            true
        } else {
            self.validate_number_field(&text);
            false
        }
    }
    fn validate_number_field(&self, text: &[u32]) {
        if self.field.base.node().get_ff_widget_type() != Kind::NumericEdit {
            return;
        }
        let Some(host) = self.field.base.doc_view() else {
            return;
        };
        let mut message = text.to_vec();
        message.extend(" can not contain ".chars().map(u32::from));
        message.extend(self.field.base.node().get_name_expression());
        host.message_box(
            &message,
            &host.app_title(),
            AlertIcon::kError,
            AlertButton::kOK,
        );
    }
    pub fn is_data_changed(&self) -> bool {
        self.field
            .base
            .layout_item()
            .is_some_and(|item| item.test_status_bits(S::kTextEditValueChanged))
    }
    pub fn get_alignment(&self) -> u32 {
        let Some(para) = self.field.base.node().get_para_if_exists() else {
            return 0;
        };
        let horizontal = match para.para_get_horizontal_align() {
            V::Center => FWL_STYLEEXT_EDT_HCENTER,
            V::Justify => FWL_STYLEEXT_EDT_JUSTIFIED,
            V::Right => FWL_STYLEEXT_EDT_HFAR,
            _ => 0,
        };
        horizontal
            | match para.para_get_vertical_align() {
                V::Middle => FWL_STYLEEXT_EDT_VCENTER,
                V::Bottom => FWL_STYLEEXT_EDT_VFAR,
                _ => 0,
            }
    }
    pub fn update_fwl_data(&self) {
        let Some(widget) = self.field.normal_widget() else {
            return;
        };
        let Some(edit) = widget.as_edit() else {
            return;
        };
        let node = self.field.base.node();
        let picture = if self.field.base.is_focused() {
            XfaValuePicture::Edit
        } else {
            XfaValuePicture::Display
        };
        let mut update = false;
        let kind = node.get_ff_widget_type();
        if kind == Kind::TextEdit && node.get_number_of_cells().is_none() {
            let (element, mut max) = node.get_max_chars();
            if element == E::ExData && picture != XfaValuePicture::Edit {
                max = 0;
            }
            if edit.get_limit() != max {
                edit.set_limit(max);
                update = true;
            }
        } else if kind == Kind::Barcode {
            let length = if picture == XfaValuePicture::Edit {
                node.get_ui_child_node()
                    .and_then(|barcode| barcode.barcode_get_data_length())
                    .unwrap_or(0)
            } else {
                0
            };
            edit.set_limit(length);
            update = true;
        }
        let text = node.get_value(picture);
        if text != edit.get_text() || (picture == XfaValuePicture::Edit && update) {
            edit.set_text_skip_notify(&text);
            update = true;
        }
        if update {
            widget.update();
        }
    }
    pub fn on_text_will_change(&self, event: &mut FwlEvent) {
        let EventData::TextWillChange {
            change_text,
            previous_text,
            selection_start,
            selection_end,
            ..
        } = &event.data
        else {
            return;
        };
        self.field
            .base
            .modify_status(S::kTextEditValueChanged, S::kNone);
        let mut parameter = CxfaEventParam::new(XfaEventType::Change);
        parameter.change = change_text.clone();
        parameter.prev_text = previous_text.clone();
        parameter.sel_start = (*selection_start).try_into().unwrap_or(i32::MAX);
        parameter.sel_end = (*selection_end).try_into().unwrap_or(i32::MAX);
        if let Some(host) = self.field.base.doc_view() {
            host.process_event(self.field.base.node(), V::Change, &mut parameter);
        }
        if let EventData::TextWillChange {
            change_text,
            selection_start,
            selection_end,
            cancelled,
            ..
        } = &mut event.data
        {
            *cancelled = parameter.cancel_action;
            *change_text = parameter.change;
            *selection_start = parameter.sel_start.max(0) as usize;
            *selection_end = parameter.sel_end.max(0) as usize;
        }
    }
    pub fn on_text_full(&self) {
        if let Some(host) = self.field.base.doc_view() {
            host.process_event(
                self.field.base.node(),
                V::Full,
                &mut CxfaEventParam::new(XfaEventType::Full),
            );
        }
    }
    pub fn on_process_message(&self, message: &mut FwlMessage) {
        let old = self.old_delegate.borrow().clone();
        if let Some(old) = old {
            old.on_process_message(message);
        } else if let Some(widget) = self.field.normal_widget() {
            widget.on_process_message(message);
        }
    }
    pub fn on_process_event(&self, event: &mut FwlEvent) {
        self.field.on_process_event(event);
        match event.data {
            EventData::TextWillChange { .. } => self.on_text_will_change(event),
            EventData::TextFull => self.on_text_full(),
            _ => {}
        }
        let old = self.old_delegate.borrow().clone();
        if let Some(old) = old {
            old.on_process_event(event);
        } else if let Some(widget) = self.field.normal_widget() {
            widget.on_process_event(event);
        }
    }
    pub fn on_draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        let old = self.old_delegate.borrow().clone();
        if let Some(old) = old {
            old.on_draw_widget(graphics, matrix);
        } else if let Some(widget) = self.field.normal_widget() {
            widget.draw_widget(graphics, matrix);
        }
    }
    pub fn can_undo(&self) -> bool {
        self.field
            .normal_widget()
            .is_some_and(|widget| widget.as_edit().is_some_and(CfwlEdit::can_undo))
    }
    pub fn can_redo(&self) -> bool {
        self.field
            .normal_widget()
            .is_some_and(|widget| widget.as_edit().is_some_and(CfwlEdit::can_redo))
    }
    pub fn can_copy(&self) -> bool {
        self.field
            .normal_widget()
            .is_some_and(|widget| widget.as_edit().is_some_and(CfwlEdit::has_selection))
    }
    pub fn can_cut(&self) -> bool {
        self.field.normal_widget().is_some_and(|widget| {
            widget.base().get_style_exts() & FWL_STYLEEXT_EDT_READ_ONLY == 0
                && widget.as_edit().is_some_and(CfwlEdit::has_selection)
        })
    }
    pub fn can_paste(&self) -> bool {
        self.field
            .normal_widget()
            .is_some_and(|widget| widget.base().get_style_exts() & FWL_STYLEEXT_EDT_READ_ONLY == 0)
    }
    pub fn can_select_all(&self) -> bool {
        self.field.normal_widget().is_some_and(|widget| {
            widget
                .as_edit()
                .is_some_and(|edit| edit.get_text_length() > 0)
        })
    }
    pub fn undo(&self) -> bool {
        self.field
            .normal_widget()
            .is_some_and(|widget| widget.as_edit().is_some_and(CfwlEdit::undo))
    }
    pub fn redo(&self) -> bool {
        self.field
            .normal_widget()
            .is_some_and(|widget| widget.as_edit().is_some_and(CfwlEdit::redo))
    }
    pub fn copy(&self) -> Option<Vec<u32>> {
        self.field.normal_widget()?.as_edit()?.copy()
    }
    pub fn cut(&self) -> Option<Vec<u32>> {
        self.field.normal_widget()?.as_edit()?.cut()
    }
    pub fn paste(&self, text: &[u32]) -> bool {
        self.field
            .normal_widget()
            .is_some_and(|widget| widget.as_edit().is_some_and(|edit| edit.paste(text)))
    }
    pub fn select_all(&self) {
        if let Some(widget) = self.field.normal_widget() {
            if let Some(edit) = widget.as_edit() {
                edit.select_all();
            }
        }
    }
    pub fn delete(&self) {
        if let Some(widget) = self.field.normal_widget() {
            if let Some(edit) = widget.as_edit() {
                edit.clear_text();
            }
        }
    }
    pub fn de_select(&self) {
        if let Some(widget) = self.field.normal_widget() {
            if let Some(edit) = widget.as_edit() {
                edit.clear_selection();
            }
        }
    }
    pub fn get_text(&self) -> Vec<u32> {
        self.field
            .normal_widget()
            .and_then(|widget| widget.as_edit().map(CfwlEdit::get_text))
            .unwrap_or_default()
    }
}
impl Drop for CxfaFfTextEdit {
    fn drop(&mut self) {
        self.pre_finalize();
    }
}
impl LayoutItemWidget for CxfaFfTextEdit {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.field.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfTextEdit {
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
impl FfWidget for CxfaFfTextEdit {
    fn widget(&self) -> &CxfaFfWidget {
        &self.field.base
    }
    fn as_field(&self) -> Option<&CxfaFfField> {
        Some(&self.field)
    }
    fn as_text_edit(&self) -> Option<&CxfaFfTextEdit> {
        Some(self)
    }
}
