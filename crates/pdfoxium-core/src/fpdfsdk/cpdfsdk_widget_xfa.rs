// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Foreground AcroForm/XFA branches from `cpdfsdk_widget.cpp`. Lookup uses the
//! existing form SOM resolver and FF widget owner, and executes on the same SDK
//! VM in the actual XFA realm. No second field/model or script engine is made.
use super::{
    cpdfsdk_pageview::CpdfSdkPageView, cpdfsdk_widget::CpdfSdkWidget,
    formfiller::cffl_fieldaction::CfflFieldAction,
};
use crate::{
    fpdfdoc::{
        cpdf_aaction::AActionType, cpdf_annot::AppearanceMode, cpdf_formfield::FormFieldType,
    },
    xfa::fxfa::{
        cxfa_eventparam::{CxfaEventParam, XfaEventType},
        cxfa_ffdocview::CxfaFfDocView,
        cxfa_ffwidgethandler::CxfaFfWidgetHandler,
        layout::cxfa_layoutitem::LayoutItemWidget,
        parser::{cxfa_localevalue::XfaValuePicture, cxfa_node_check::XfaCheckState},
    },
};
use std::rc::Rc;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum XfaAActionType {
    Click,
    Full,
    PreOpen,
    PostOpen,
}
impl XfaAActionType {
    fn event(self) -> XfaEventType {
        match self {
            Self::Click => XfaEventType::Click,
            Self::Full => XfaEventType::Full,
            Self::PreOpen => XfaEventType::PreOpen,
            Self::PostOpen => XfaEventType::PostOpen,
        }
    }
}
pub fn xfa_event_for_action(kind: AActionType, will_commit: bool) -> XfaEventType {
    match kind {
        AActionType::CursorEnter => XfaEventType::MouseEnter,
        AActionType::CursorExit => XfaEventType::MouseExit,
        AActionType::ButtonDown => XfaEventType::MouseDown,
        AActionType::ButtonUp => XfaEventType::MouseUp,
        AActionType::GetFocus => XfaEventType::Enter,
        AActionType::LoseFocus => XfaEventType::Exit,
        AActionType::KeyStroke if !will_commit => XfaEventType::Change,
        AActionType::Validate => XfaEventType::Validate,
        _ => XfaEventType::Unknown,
    }
}
fn event_data(kind: XfaEventType, data: &CfflFieldAction) -> CxfaEventParam {
    let mut event = CxfaEventParam::new(kind);
    event.change = data.s_change.clone();
    event.commit_key = 0;
    event.shift = data.b_shift;
    event.sel_start = data.n_sel_start;
    event.sel_end = data.n_sel_end;
    event.full_text = data.s_value.clone();
    event.key_down = data.b_key_down;
    event.modifier = data.b_modifier;
    event.prev_text = data.s_value.clone();
    event
}
impl CpdfSdkWidget {
    pub(crate) fn foreground_xfa_view(&self) -> Option<Rc<CxfaFfDocView>> {
        let context = self.env()?.xfa_context()?;
        if !context.contains_extension_foreground_form() {
            return None;
        }
        context.doc_view()
    }
    pub fn get_mix_xfa_widget(&self) -> Option<Rc<dyn LayoutItemWidget>> {
        let view = self.foreground_xfa_view()?;
        let field = self.field()?;
        let name = if self.field_type() == FormFieldType::RadioButton {
            let name = self.annot().annot_name();
            if name.is_empty() {
                field.full_name()
            } else {
                name
            }
        } else {
            field.full_name()
        };
        if name.is_empty() {
            return None;
        }
        view.get_widget_by_name(&name, None)
    }
    pub fn get_group_mix_xfa_widget(&self) -> Option<Rc<dyn LayoutItemWidget>> {
        let view = self.foreground_xfa_view()?;
        let name = self.field()?.full_name();
        if name.is_empty() {
            return None;
        }
        view.get_widget_by_name(&name, None)
    }
    pub fn has_xfa_action(&self, kind: XfaAActionType) -> bool {
        let Some(owner) = self.get_mix_xfa_widget() else {
            return false;
        };
        let Some(widget) = owner.as_ff_widget() else {
            return false;
        };
        let Some(view) = self.foreground_xfa_view() else {
            return false;
        };
        let handler = CxfaFfWidgetHandler::new(Rc::downgrade(&view));
        let event = kind.event();
        if matches!(event, XfaEventType::Click | XfaEventType::Change)
            && self.field_type() == FormFieldType::RadioButton
        {
            if let Some(group) = self.get_group_mix_xfa_widget() {
                if group
                    .as_ff_widget()
                    .is_some_and(|widget| widget.widget().has_event_under_handler(event, &handler))
                {
                    return true;
                }
            }
        }
        widget.widget().has_event_under_handler(event, &handler)
    }
    pub fn on_xfa_action(
        &self,
        kind: XfaAActionType,
        data: &mut CfflFieldAction,
        _view: &Rc<CpdfSdkPageView>,
    ) -> bool {
        let Some(view) = self.foreground_xfa_view() else {
            return false;
        };
        let Some(owner) = self.get_mix_xfa_widget() else {
            return false;
        };
        let Some(widget) = owner.as_ff_widget() else {
            return false;
        };
        let handler = CxfaFfWidgetHandler::new(Rc::downgrade(&view));
        let mut event = event_data(kind.event(), data);
        if matches!(event.event_type, XfaEventType::Click | XfaEventType::Change)
            && self.field_type() == FormFieldType::RadioButton
        {
            if let Some(group) = self.get_group_mix_xfa_widget() {
                if let Some(group) = group.as_ff_widget() {
                    if !group
                        .widget()
                        .process_event_under_handler(&mut event, &handler)
                    {
                        return false;
                    }
                    if !self.is_alive() {
                        return false;
                    }
                }
            }
        }
        let result = widget
            .widget()
            .process_event_under_handler(&mut event, &handler);
        let _ = view.update_doc_view();
        result
    }
    pub fn handle_xfa_action(&self, kind: AActionType, data: &mut CfflFieldAction) -> bool {
        let event = xfa_event_for_action(kind, data.b_will_commit);
        if event == XfaEventType::Unknown {
            return false;
        }
        let Some(view) = self.foreground_xfa_view() else {
            return false;
        };
        let Some(owner) = self.get_mix_xfa_widget() else {
            return false;
        };
        let Some(widget) = owner.as_ff_widget() else {
            return false;
        };
        let handler = CxfaFfWidgetHandler::new(Rc::downgrade(&view));
        let mut event = event_data(event, data);
        let result = widget
            .widget()
            .process_event_under_handler(&mut event, &handler);
        let _ = view.update_doc_view();
        result
    }
    pub fn synchronize(&self, synchronize_else: bool) {
        let Some(owner) = self.get_mix_xfa_widget() else {
            return;
        };
        let Some(widget) = owner.as_ff_widget() else {
            return;
        };
        let node = widget.widget().node();
        if !node.is_widget_ready() {
            return;
        }
        let Some(field) = self.field() else {
            return;
        };
        match self.field_type() {
            FormFieldType::CheckBox | FormFieldType::RadioButton => {
                node.set_check_state(if self.control().is_checked() {
                    XfaCheckState::On
                } else {
                    XfaCheckState::Off
                });
            }
            FormFieldType::TextField => {
                node.set_value(XfaValuePicture::Edit, &field.value());
            }
            FormFieldType::ComboBox | FormFieldType::ListBox => {
                node.clear_all_selections();
                for index in 0..field.count_selected_items() {
                    let index = field.selected_index(index as i32);
                    if index >= 0 && (index as usize) < node.count_choice_list_items(false) {
                        node.set_item_state(index, true, false);
                    }
                }
                if self.field_type() == FormFieldType::ComboBox {
                    node.set_value(XfaValuePicture::Edit, &field.value());
                }
            }
            _ => {}
        }
        if synchronize_else && self.is_alive() {
            if let Some(view) = self.foreground_xfa_view() {
                view.process_value_changed(node);
            }
        }
    }
    pub fn reset_xfa_appearance(&self, _value_changed: bool) {
        match self.field_type() {
            FormFieldType::TextField | FormFieldType::ComboBox => {
                let formatted = self.on_format();
                if self.is_alive() {
                    self.reset_appearance(formatted.as_deref(), true);
                }
            }
            _ => {
                self.reset_appearance(None, false);
            }
        }
    }
    pub(crate) fn xfa_selected_index(&self, index: i32) -> Option<i32> {
        let owner = self.get_mix_xfa_widget()?;
        let node = owner.as_ff_widget()?.widget().node();
        if !node.is_widget_ready() || index < 0 || index as usize >= node.count_selected_items() {
            return None;
        }
        Some(node.get_selected_item(index))
    }
    pub(crate) fn xfa_value(&self) -> Option<Vec<u32>> {
        let owner = self.get_mix_xfa_widget()?;
        let node = owner.as_ff_widget()?.widget().node();
        node.is_widget_ready()
            .then(|| node.get_value(XfaValuePicture::Display))
    }
    pub(crate) fn xfa_option_selected(&self, index: i32) -> Option<bool> {
        let owner = self.get_mix_xfa_widget()?;
        let node = owner.as_ff_widget()?.widget().node();
        if !node.is_widget_ready() {
            return None;
        }
        Some(
            index >= 0
                && (index as usize) < node.count_choice_list_items(false)
                && node.get_item_state(index),
        )
    }
    pub(crate) fn xfa_checked(&self) -> Option<bool> {
        let owner = self.get_mix_xfa_widget()?;
        let node = owner.as_ff_widget()?.widget().node();
        node.is_widget_ready()
            .then(|| node.get_check_state() == XfaCheckState::On)
    }
    pub(crate) fn finish_xfa_check(&self) {
        if !self.is_widget_appearance_valid(AppearanceMode::Normal) {
            self.reset_xfa_appearance(true);
        }
        if self.is_alive() {
            self.synchronize(true);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_pdf_actions_map_only_supported_xfa_events() {
        for (pdf, xfa) in [
            (AActionType::CursorEnter, XfaEventType::MouseEnter),
            (AActionType::CursorExit, XfaEventType::MouseExit),
            (AActionType::ButtonDown, XfaEventType::MouseDown),
            (AActionType::ButtonUp, XfaEventType::MouseUp),
            (AActionType::GetFocus, XfaEventType::Enter),
            (AActionType::LoseFocus, XfaEventType::Exit),
            (AActionType::KeyStroke, XfaEventType::Change),
            (AActionType::Validate, XfaEventType::Validate),
        ] {
            assert_eq!(xfa_event_for_action(pdf, false), xfa);
        }
        assert_eq!(
            xfa_event_for_action(AActionType::KeyStroke, true),
            XfaEventType::Unknown
        );
        for kind in [
            AActionType::Format,
            AActionType::Calculate,
            AActionType::OpenPage,
            AActionType::ClosePage,
            AActionType::PageOpen,
            AActionType::PageClose,
            AActionType::PageVisible,
            AActionType::PageInvisible,
            AActionType::DocumentOpen,
            AActionType::CloseDocument,
            AActionType::SaveDocument,
            AActionType::DocumentSaved,
            AActionType::PrintDocument,
            AActionType::DocumentPrinted,
        ] {
            assert_eq!(xfa_event_for_action(kind, false), XfaEventType::Unknown);
        }
    }
}

#[cfg(test)]
#[path = "cpdfsdk_widget_xfa_tests.rs"]
mod integration_tests;
