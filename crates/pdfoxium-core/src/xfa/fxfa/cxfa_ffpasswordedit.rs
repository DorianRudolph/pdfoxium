// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Complete `cxfa_ffpasswordedit.{h,cpp}`; actual FWL password alias/rendering.
use super::{
    cxfa_fffield::CxfaFfField,
    cxfa_fftextedit::CxfaFfTextEdit,
    cxfa_ffwidget::{CxfaFfWidget, FfResult, FfWidget},
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::{cxfa_document::CxfaNodeStore, cxfa_node::CxfaNode},
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxRectF},
    xfa::fwl::{cfwl_edit::*, cfwl_widget::WidgetAdapter},
};
use std::rc::Rc;
pub struct CxfaFfPasswordEdit {
    pub text_edit: CxfaFfTextEdit,
    password_node: CxfaNode,
    _password_store: Rc<CxfaNodeStore>,
}
impl CxfaFfPasswordEdit {
    pub fn new(node: &CxfaNode, password_node: &CxfaNode) -> Rc<Self> {
        let widget = Rc::new(Self {
            text_edit: CxfaFfTextEdit::new_base(node),
            password_node: password_node.clone(),
            _password_store: password_node
                .retain_store()
                .expect("password node has expired"),
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
            | FWL_STYLEEXT_EDT_PASSWORD
            | field.update_ui_property();
        if let Some(alias) = self.password_node.password_edit_get_password_char().first() {
            edit.set_alias_char(*alias);
        }
        if !node.is_horizontal_scroll_policy_off() {
            extended |= FWL_STYLEEXT_EDT_AUTO_HSCROLL;
        }
        if !node.is_open_access()
            || field
                .base
                .doc_view()
                .is_some_and(|host| !host.is_interactive())
        {
            extended |= FWL_STYLEEXT_EDT_READ_ONLY;
        }
        extended |= self.text_edit.get_alignment();
        widget.modify_style_exts(extended, u32::MAX);
    }
}
impl LayoutItemWidget for CxfaFfPasswordEdit {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.text_edit.field.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfPasswordEdit {
    fn get_rotate_matrix(&self) -> CfxMatrix {
        self.widget().get_rotate_matrix()
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
impl FfWidget for CxfaFfPasswordEdit {
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
}
