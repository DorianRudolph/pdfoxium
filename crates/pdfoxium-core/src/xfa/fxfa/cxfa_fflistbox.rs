// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_fflistbox.{h,cpp}`. The abstract FFDropDown has no additional state;
//! its item virtuals are the real FfWidget choice operations implemented here.
use super::{
    cxfa_eventparam::{CxfaEventParam, XfaEventType},
    cxfa_fffield::CxfaFfField,
    cxfa_ffwidget::{CxfaFfWidget, FfResult, FfWidget},
    fxfa_basic::XfaAttributeValue as V,
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::cxfa_node::CxfaNode,
};
use crate::{
    fpdfdoc::cpdf_formfield::FormFieldType,
    fxcrt::fx_coordinates::{CfxMatrix, CfxRectF},
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fwl::{
            cfwl_event::{EventData, FwlEvent},
            cfwl_listbox::*,
            cfwl_message::FwlMessage,
            cfwl_widget::{FwlWidget, Properties, WidgetAdapter, WidgetRef, WidgetStyle},
            ifwl_widgetdelegate::WidgetDelegate,
        },
    },
};
use std::{
    cell::RefCell,
    ops::Deref,
    rc::{Rc, Weak},
};
pub struct CxfaFfListBox {
    field: CxfaFfField,
    weak_self: Weak<Self>,
    control: RefCell<Option<Rc<CfwlListBox>>>,
    old_delegate: RefCell<Option<Rc<dyn WidgetDelegate>>>,
}
impl CxfaFfListBox {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        Rc::new_cyclic(|weak| Self {
            field: CxfaFfField::new_base(node),
            weak_self: weak.clone(),
            control: RefCell::new(None),
            old_delegate: RefCell::new(None),
        })
    }
    pub fn control(&self) -> Option<Rc<CfwlListBox>> {
        self.control.borrow().clone()
    }
    pub fn pre_finalize(&self) {
        if let Some(control) = self.control() {
            if let Some(app) = control.base().app() {
                let widget: WidgetRef = control;
                app.note_driver().unregister_event_target(&widget);
            }
        }
    }
    fn alignment(&self) -> u32 {
        self.field.base.node().get_para_if_exists().map_or(0, |p| {
            match p.para_get_horizontal_align() {
                V::Center => FWL_STYLEEXT_LTB_CENTER_ALIGN,
                V::Right => FWL_STYLEEXT_LTB_RIGHT_ALIGN,
                _ => FWL_STYLEEXT_LTB_LEFT_ALIGN,
            }
        })
    }
    pub fn on_select_changed(&self) {
        if let Some(host) = self.field.base.doc_view() {
            let mut parameter = CxfaEventParam::new(XfaEventType::Change);
            parameter.prev_text = self.field.base.node().get_raw_value();
            host.process_event(self.field.base.node(), V::Change, &mut parameter);
        }
    }
    pub fn set_item_state(&self, index: i32, selected: bool) {
        if let Some(control) = self.control() {
            if let Some(item) = control.get_item(index) {
                control.set_sel_item(Some(&item), selected);
            }
            control.update();
            self.field.base.invalidate_rect();
        }
    }
}
impl Drop for CxfaFfListBox {
    fn drop(&mut self) {
        self.pre_finalize();
    }
}
impl Deref for CxfaFfListBox {
    type Target = CxfaFfField;
    fn deref(&self) -> &Self::Target {
        &self.field
    }
}
impl LayoutItemWidget for CxfaFfListBox {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.field.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfListBox {
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
impl FfWidget for CxfaFfListBox {
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
        let control = CfwlListBox::new(&app, Properties::default(), None);
        control
            .base()
            .set_styles(WidgetStyle::VSCROLL | WidgetStyle::NO_BACKGROUND);
        self.old_delegate.replace(control.base().get_delegate());
        let widget: WidgetRef = control.clone();
        let owner: Rc<dyn FfWidget> = self.weak_self.upgrade().expect("live FF listbox");
        self.field.attach_normal_widget(&owner, widget.clone());
        self.control.replace(Some(control.clone()));
        app.note_driver()
            .register_event_target(&widget, Some(&widget));
        {
            let _lock = control.base().scoped_update_lock();
            let labels = self.field.base.node().get_choice_list_items(false);
            let values = self.field.base.node().get_choice_list_items(true);
            for label in labels.iter().take(values.len()) {
                control.add_string(label);
            }
            let mut style = FWL_STYLEEXT_LTB_SHOW_SCROLL_BAR_FOCUS | self.alignment();
            if self.field.base.node().is_choice_list_multi_select() {
                style |= FWL_STYLEEXT_LTB_MULTI_SELECTION;
            }
            control.modify_style_exts(style, u32::MAX);
            for index in self.field.base.node().get_selected_items() {
                if let Some(item) = control.get_item(index) {
                    control.set_sel_item(Some(&item), true);
                }
            }
        }
        self.field.load_widget()
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
        let Some(control) = self.control() else {
            return false;
        };
        let indices = (0..control.count_sel_items())
            .map(|i| control.get_sel_index(i))
            .collect::<Vec<_>>();
        self.field
            .base
            .node()
            .set_selected_items(&indices, true, true)
    }
    fn is_data_changed(&self) -> bool {
        let Some(control) = self.control() else {
            return false;
        };
        let selected = self.field.base.node().get_selected_items();
        selected.len() != control.count_sel_items() as usize
            || selected
                .iter()
                .any(|&i| control.get_item(i).is_none_or(|item| !item.is_selected()))
    }
    fn update_fwl_data(&self) {
        let Some(control) = self.control() else {
            return;
        };
        let selected = self.field.base.node().get_selected_items();
        control.clear_selection();
        // Native GetSelItem(index) mistakes an item index for a selection
        // ordinal. Resolve actual items so noncontiguous selections restore.
        for index in selected {
            if let Some(item) = control.get_item(index) {
                control.set_sel_item(Some(&item), true);
            }
        }
        control.update();
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
            control.delete_all();
        } else {
            control.remove_at(index);
        }
        control.update();
        self.field.base.invalidate_rect();
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
        if matches!(event.data, EventData::SelectChanged { .. }) {
            self.on_select_changed();
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
        FormFieldType::XfaListBox
    }
}

#[cfg(test)]
#[path = "cxfa_ffchoice_tests.rs"]
mod tests;
