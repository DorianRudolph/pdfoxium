// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_ffdatetimeedit.{h,cpp}` using the real composite date picker,
//! existing locale picture formatter and inherited FF text-edit event handling.
use super::{
    cxfa_eventparam::{CxfaEventParam, XfaEventType},
    cxfa_fffield::CxfaFfField,
    cxfa_fftextedit::CxfaFfTextEdit,
    cxfa_ffwidget::{CxfaFfWidget, FfResult, FfWidget, FocusOption},
    fxfa::XfaWidgetStatus as S,
    fxfa_basic::{XfaAttributeValue as V, XfaElement as E},
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::{
        cxfa_localevalue::{CxfaLocaleValue, ValueType, XfaValuePicture},
        cxfa_node::CxfaNode,
        xfa_utils::get_locale_value,
    },
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::{
        fgas::crt::{locale_mgr_iface::LocaleMgrIface, picture_datetime::PictureDateTime},
        fwl::{
            cfwl_datetimepicker::*,
            cfwl_edit::{
                FWL_STYLEEXT_EDT_AUTO_HSCROLL, FWL_STYLEEXT_EDT_COMB_TEXT,
                FWL_STYLEEXT_EDT_READ_ONLY,
            },
            cfwl_event::{EventData, FwlEvent},
            cfwl_message::MouseCommand,
            cfwl_widget::{FwlWidget, WidgetAdapter, WidgetRef},
        },
    },
};
use std::{cell::RefCell, ops::Deref, rc::Rc};
pub struct CxfaFfDateTimeEdit {
    text_edit: CxfaFfTextEdit,
    control: RefCell<Option<Rc<CfwlDateTimePicker>>>,
}
impl CxfaFfDateTimeEdit {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        let value = Rc::new(Self {
            text_edit: CxfaFfTextEdit::new_base(node),
            control: RefCell::new(None),
        });
        let owner: Rc<dyn FfWidget> = value.clone();
        value.text_edit.bind_owner(&owner);
        value
    }
    pub fn control(&self) -> Option<Rc<CfwlDateTimePicker>> {
        self.control.borrow().clone()
    }
    fn alignment(&self) -> u32 {
        let Some(para) = self.text_edit.field.base.node().get_para_if_exists() else {
            return 0;
        };
        let horizontal = match para.para_get_horizontal_align() {
            V::Center => FWL_STYLEEXT_DTP_EDIT_H_CENTER,
            V::Justify => FWL_STYLEEXT_DTP_EDIT_JUSTIFIED,
            V::Right => FWL_STYLEEXT_DTP_EDIT_H_FAR,
            _ => FWL_STYLEEXT_DTP_EDIT_H_NEAR,
        };
        horizontal
            | match para.para_get_vertical_align() {
                V::Middle => FWL_STYLEEXT_DTP_EDIT_V_CENTER,
                V::Bottom => FWL_STYLEEXT_DTP_EDIT_V_FAR,
                _ => FWL_STYLEEXT_DTP_EDIT_V_NEAR,
            }
    }
    fn update_selected_date(&self, control: &CfwlDateTimePicker, require_valid: bool) {
        let value = get_locale_value(self.text_edit.field.base.node());
        if require_valid && !value.is_valid() {
            return;
        }
        let date = value.get_date();
        if date.is_set() {
            control.set_cur_sel(date.year, i32::from(date.month), i32::from(date.day));
        }
    }
    pub fn on_select_changed(&self, year: i32, month: i32, day: i32) -> bool {
        let (Ok(month), Ok(day)) = (u8::try_from(month), u8::try_from(day)) else {
            return false;
        };
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return false;
        }
        let Some(control) = self.control() else {
            return false;
        };
        let node = self.text_edit.field.base.node();
        let Some(document) = node.document() else {
            return false;
        };
        let Some(host) = self.text_edit.field.base.doc_view() else {
            return false;
        };
        let manager = document
            .get_locale_mgr()
            .map(|mgr| mgr as Rc<dyn LocaleMgrIface>);
        let mut value = CxfaLocaleValue::with_type(ValueType::Date, manager);
        value.set_date(&PictureDateTime::new(year, month, day, 0, 0, 0, 0));
        let mut text = Vec::new();
        value.format_patterns(
            &mut text,
            &node.get_picture_content(XfaValuePicture::Edit),
            node.get_locale(),
            XfaValuePicture::Edit,
        );
        // Keep the old model value across focus loss, which can commit the new
        // date before the Change callback receives its prevText.
        let previous = node.get_raw_value();
        control.set_edit_text(&text);
        control.update();
        if host.set_focus_node(None).is_err() {
            return false;
        }
        let mut event = CxfaEventParam::new(XfaEventType::Change);
        event.prev_text = previous;
        host.process_event(node, V::Change, &mut event);
        true
    }
    fn is_read_only(&self) -> bool {
        self.control().is_none_or(|control| {
            control.edit().base().get_style_exts() & FWL_STYLEEXT_EDT_READ_ONLY != 0
        })
    }
}
impl Deref for CxfaFfDateTimeEdit {
    type Target = CxfaFfTextEdit;
    fn deref(&self) -> &Self::Target {
        &self.text_edit
    }
}
impl LayoutItemWidget for CxfaFfDateTimeEdit {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.text_edit.field.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfDateTimeEdit {
    fn get_rotate_matrix(&self) -> CfxMatrix {
        self.text_edit.field.base.get_rotate_matrix()
    }
    fn display_caret(&self, visible: bool, anchor: Option<CfxRectF>) {
        if let Some(host) = self.text_edit.field.base.doc_view() {
            host.display_caret(self, visible, anchor);
        }
    }
    fn get_border_color_and_thickness(&self) -> (u32, f32) {
        self.text_edit.field.base.get_border_color_and_thickness()
    }
}
impl FfWidget for CxfaFfDateTimeEdit {
    fn widget(&self) -> &CxfaFfWidget {
        &self.text_edit.field.base
    }
    fn as_field(&self) -> Option<&CxfaFfField> {
        Some(&self.text_edit.field)
    }
    fn as_text_edit(&self) -> Option<&CxfaFfTextEdit> {
        Some(&self.text_edit)
    }
    fn get_bbox(&self, focus: FocusOption) -> CfxRectF {
        if focus == FocusOption::DrawFocus {
            CfxRectF::default()
        } else {
            self.text_edit
                .field
                .base
                .get_bbox(FocusOption::DoNotDrawFocus)
        }
    }
    fn accepts_focus_on_button_down(
        &self,
        _flags: u32,
        point: CfxPointF,
        command: MouseCommand,
    ) -> bool {
        if command == MouseCommand::RightButtonDown
            && !self.text_edit.field.base.node().is_open_access()
        {
            return false;
        }
        self.control()
            .is_some_and(|control| control.get_bbox().contains_point(point))
    }
    fn load_widget(&self) -> FfResult<bool> {
        if self.control().is_some() {
            return Ok(false);
        }
        let host = self.text_edit.field.base.require_doc_view()?;
        let app = host.fwl_app();
        let control = CfwlDateTimePicker::new(&app);
        let owner = self.text_edit.owner().expect("live FF date/time edit");
        let widget: WidgetRef = control.clone();
        self.text_edit.attach_edit_widget(&owner, widget);
        self.control.replace(Some(control.clone()));
        {
            let _lock = control.base().scoped_update_lock();
            let text = self
                .text_edit
                .field
                .base
                .node()
                .get_value(XfaValuePicture::Display);
            control.set_edit_text(&text);
            if !text.is_empty()
                && self
                    .text_edit
                    .field
                    .base
                    .node()
                    .get_form_value_if_exists()
                    .is_some_and(|v| v.value_get_child_value_class_id() == E::Date)
            {
                self.update_selected_date(&control, false);
            }
            self.update_widget_property();
        }
        self.text_edit
            .field
            .load_widget_with_layout(|| self.perform_layout())
    }
    fn update_widget_property(&self) {
        let Some(control) = self.control() else {
            return;
        };
        let node = self.text_edit.field.base.node();
        control.modify_style_exts(
            FWL_STYLEEXT_DTP_SHORT_DATE_FORMAT
                | self.text_edit.field.update_ui_property()
                | self.alignment(),
            u32::MAX,
        );
        let mut style = 0;
        if let Some(cells) = node.get_number_of_cells().filter(|&n| n > 0) {
            style |= FWL_STYLEEXT_EDT_COMB_TEXT;
            control.set_edit_limit(cells);
        }
        if !node.is_open_access()
            || self
                .text_edit
                .field
                .base
                .doc_view()
                .is_some_and(|host| !host.is_interactive())
        {
            style |= FWL_STYLEEXT_EDT_READ_ONLY;
        }
        if !node.is_horizontal_scroll_policy_off() {
            style |= FWL_STYLEEXT_EDT_AUTO_HSCROLL;
        }
        control.modify_edit_style_exts(style, u32::MAX);
    }
    fn commit_data(&self) -> bool {
        let Some(control) = self.control() else {
            return false;
        };
        let node = self.text_edit.field.base.node();
        if !node.set_value(XfaValuePicture::Edit, &control.get_edit_text()) {
            return false;
        }
        if let Some(host) = self.text_edit.field.base.doc_view() {
            host.update_ui_display(node, Some(self));
        }
        true
    }
    fn update_fwl_data(&self) {
        let Some(control) = self.control() else {
            return;
        };
        let focused = self.text_edit.field.base.is_focused();
        let text = self.text_edit.field.base.node().get_value(if focused {
            XfaValuePicture::Edit
        } else {
            XfaValuePicture::Display
        });
        control.set_edit_text(&text);
        if focused && !text.is_empty() {
            self.update_selected_date(&control, true);
        }
        control.update();
    }
    fn is_data_changed(&self) -> bool {
        if self
            .text_edit
            .field
            .base
            .layout_item()
            .is_some_and(|item| item.test_status_bits(S::kTextEditValueChanged))
        {
            return true;
        }
        self.control().is_some_and(|c| {
            self.text_edit
                .field
                .base
                .node()
                .get_value(XfaValuePicture::Edit)
                != c.get_edit_text()
        })
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        if let EventData::SelectChanged {
            year, month, day, ..
        } = event.data
        {
            self.on_select_changed(year, month, day);
            return;
        }
        self.text_edit.on_process_event(event);
    }
    fn can_undo(&self) -> bool {
        self.control().is_some_and(|c| c.can_undo())
    }
    fn can_redo(&self) -> bool {
        self.control().is_some_and(|c| c.can_redo())
    }
    fn can_copy(&self) -> bool {
        self.control().is_some_and(|c| c.has_selection())
    }
    fn can_cut(&self) -> bool {
        !self.is_read_only() && self.can_copy()
    }
    fn can_paste(&self) -> bool {
        !self.is_read_only()
    }
    fn can_select_all(&self) -> bool {
        self.control().is_some_and(|c| c.get_edit_text_length() > 0)
    }
    fn undo(&self) -> bool {
        self.control().is_some_and(|c| c.undo())
    }
    fn redo(&self) -> bool {
        self.control().is_some_and(|c| c.redo())
    }
    fn copy(&self) -> Option<Vec<u32>> {
        self.control()?.copy()
    }
    fn cut(&self) -> Option<Vec<u32>> {
        self.control()?.cut()
    }
    fn paste(&self, text: &[u32]) -> bool {
        self.control().is_some_and(|c| c.paste(text))
    }
    fn select_all(&self) {
        if let Some(control) = self.control() {
            control.select_all();
        }
    }
    fn delete(&self) {
        if let Some(control) = self.control() {
            control.clear_text();
        }
    }
    fn de_select(&self) {
        if let Some(control) = self.control() {
            control.clear_selection();
        }
    }
    fn get_text(&self) -> Vec<u32> {
        self.control().map_or_else(Vec::new, |c| c.get_edit_text())
    }
}
