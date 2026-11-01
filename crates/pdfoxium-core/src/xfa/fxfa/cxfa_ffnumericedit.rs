// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Complete `cxfa_ffnumericedit.{h,cpp}` on the existing locale/numeric engine.
use super::{
    cxfa_fffield::CxfaFfField,
    cxfa_fftextedit::CxfaFfTextEdit,
    cxfa_ffwidget::{CxfaFfWidget, FfResult, FfWidget},
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::{cxfa_localevalue::XfaValuePicture, cxfa_node::CxfaNode, xfa_utils::get_locale_value},
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxRectF},
    xfa::fwl::{
        cfwl_edit::*,
        cfwl_event::{EventData, FwlEvent},
        cfwl_widget::WidgetAdapter,
    },
};
use std::rc::Rc;
pub struct CxfaFfNumericEdit {
    pub text_edit: CxfaFfTextEdit,
}
impl CxfaFfNumericEdit {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        let widget = Rc::new(Self {
            text_edit: CxfaFfTextEdit::new_base(node),
        });
        let owner: Rc<dyn FfWidget> = widget.clone();
        widget.text_edit.bind_owner(&owner);
        widget
    }
    pub fn update_widget_property(&self) {
        let field = &self.text_edit.field;
        let Some(widget) = field.normal_widget() else {
            return;
        };
        let Some(edit) = widget.as_edit() else {
            return;
        };
        let node = field.base.node();
        let mut extended = FWL_STYLEEXT_EDT_SHOW_SCROLLBAR_FOCUS
            | FWL_STYLEEXT_EDT_OUTER_SCROLLBAR
            | FWL_STYLEEXT_EDT_VALIDATE
            | FWL_STYLEEXT_EDT_NUMBER
            | field.update_ui_property();
        if !node.is_horizontal_scroll_policy_off() {
            extended |= FWL_STYLEEXT_EDT_AUTO_HSCROLL;
        }
        if let Some(count) = node.get_number_of_cells().filter(|count| *count > 0) {
            extended |= FWL_STYLEEXT_EDT_COMB_TEXT;
            edit.set_limit(count);
        }
        extended |= self.text_edit.get_alignment();
        if !node.is_open_access()
            || field
                .base
                .doc_view()
                .is_some_and(|host| !host.is_interactive())
        {
            extended |= FWL_STYLEEXT_EDT_READ_ONLY;
        }
        widget.modify_style_exts(extended, u32::MAX);
    }
    pub fn on_validate(&self, text: &[u32]) -> bool {
        let node = self.text_edit.field.base.node();
        if !node.get_picture_content(XfaValuePicture::Edit).is_empty() {
            return true;
        }
        let value = get_locale_value(node);
        let mut format = Vec::new();
        if !value.get_numeric_format(&mut format, node.get_lead_digits(), node.get_frac_digits()) {
            return false;
        }
        value.validate_numeric_temp(text, &format, node.get_locale().as_deref())
    }
    pub fn on_process_event(&self, event: &mut FwlEvent) {
        if let EventData::Validate { insert, validate } = &mut event.data {
            *validate = self.on_validate(insert);
        } else {
            self.text_edit.on_process_event(event);
        }
    }
}
impl LayoutItemWidget for CxfaFfNumericEdit {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.text_edit.field.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfNumericEdit {
    fn get_rotate_matrix(&self) -> CfxMatrix {
        self.text_edit.field.base.get_rotate_matrix()
    }
    fn display_caret(&self, visible: bool, anchor: Option<CfxRectF>) {
        if let Some(host) = self.widget().doc_view() {
            host.display_caret(self, visible, anchor);
        }
    }
    fn get_border_color_and_thickness(&self) -> (u32, f32) {
        self.widget().get_border_color_and_thickness()
    }
}
impl FfWidget for CxfaFfNumericEdit {
    fn widget(&self) -> &CxfaFfWidget {
        &self.text_edit.field.base
    }
    fn as_field(&self) -> Option<&CxfaFfField> {
        Some(&self.text_edit.field)
    }
    fn as_text_edit(&self) -> Option<&CxfaFfTextEdit> {
        Some(&self.text_edit)
    }
    fn load_widget(&self) -> FfResult<bool> {
        self.text_edit.load_widget_with_text_first(true)
    }
    fn update_widget_property(&self) {
        self.update_widget_property();
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        self.on_process_event(event);
    }
}
