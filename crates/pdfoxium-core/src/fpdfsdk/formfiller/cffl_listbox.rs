// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! List field filler from `fpdfsdk/formfiller/cffl_listbox.{h,cpp}`.
use super::cffl_fieldaction::CfflFieldAction;
use super::cffl_formfield::{CfflFormField, FormField};
use super::cffl_interactiveformfiller::CfflInteractiveFormFiller;
use super::cffl_perwindowdata::CfflPerWindowData;
use super::cffl_textobject::CfflTextObject;
use crate::fpdfdoc::cpdf_aaction::AActionType;
use crate::fpdfsdk::cpdfsdk_pageview::CpdfSdkPageView;
use crate::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use crate::fpdfsdk::pwl::cpwl_list_box::CpwlListBox;
use crate::fpdfsdk::pwl::cpwl_wnd::{
    CreateParams, Window, LISTBOX_MULTIPLE_SEL, WINDOW_AUTO_FONT_SIZE, WINDOW_VSCROLL,
};
use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

const CHOICE_MULTI_SELECT: u32 = 1 << 21;
pub struct CfflListBox {
    text_object: CfflTextObject,
    original_selections: RefCell<BTreeSet<i32>>,
    state: RefCell<Vec<i32>>,
}
impl CfflListBox {
    pub fn new(filler: &Rc<CfflInteractiveFormFiller>, widget: &Rc<CpdfSdkWidget>) -> Rc<Self> {
        let field = Rc::new(Self {
            text_object: CfflTextObject::new(filler, widget),
            original_selections: RefCell::new(BTreeSet::new()),
            state: RefCell::new(Vec::new()),
        });
        let owner: Rc<dyn FormField> = field.clone();
        field.base().bind_owner(&owner);
        field
    }
    fn with_list<R>(
        &self,
        view: &Rc<CpdfSdkPageView>,
        f: impl FnOnce(&CpwlListBox) -> R,
    ) -> Option<R> {
        let window = self.base().pwl_window(view)?;
        let list = window.as_any().downcast_ref::<CpwlListBox>()?;
        if !self.base().is_alive() || !list.wnd().is_alive() {
            return None;
        }
        Some(f(list))
    }
    fn with_created_list<R>(
        &self,
        view: &Rc<CpdfSdkPageView>,
        f: impl FnOnce(&CpwlListBox) -> R,
    ) -> Option<R> {
        let window = self.base().create_or_update_pwl_window(view)?;
        let list = window.as_any().downcast_ref::<CpwlListBox>()?;
        if !self.base().is_alive() || !list.wnd().is_alive() {
            return None;
        }
        Some(f(list))
    }
}
impl FormField for CfflListBox {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn base(&self) -> &CfflFormField {
        self.text_object.base()
    }
    fn get_create_param(&self) -> CreateParams {
        let mut cp = self.base().get_create_param();
        if self
            .base()
            .widget()
            .is_some_and(|widget| widget.field_flags() & CHOICE_MULTI_SELECT != 0)
        {
            cp.flags |= LISTBOX_MULTIPLE_SEL;
        }
        cp.flags |= WINDOW_VSCROLL;
        if cp.flags & WINDOW_AUTO_FONT_SIZE != 0 {
            cp.font_size = 12.0;
        }
        cp.font_map = self
            .text_object
            .get_or_create_font_map()
            .map(|map| map as Rc<dyn crate::fpdfdoc::ipvt_fontmap::IpvtFontMap>);
        cp
    }
    fn new_pwl_window(
        &self,
        cp: CreateParams,
        data: Box<CfflPerWindowData>,
    ) -> Option<Rc<dyn Window>> {
        let owner = self.base().owner()?;
        data.set_form_field(&owner);
        let widget = self.base().widget()?;
        let window = CpwlListBox::new(cp, Some(data));
        window.wnd().realize();
        if !self.base().is_alive() || !window.wnd().is_alive() {
            return None;
        }
        for index in 0..widget.count_options() {
            window.add_string(&widget.option_label(index as i32));
            if !self.base().is_alive() || !window.wnd().is_alive() || !widget.is_alive() {
                return None;
            }
        }
        window.set_text_direction(widget.text_direction());
        if window.is_multiple_sel() {
            let mut original = BTreeSet::new();
            let mut set_caret = false;
            for index in 0..widget.count_options() {
                let index = index as i32;
                if widget.is_option_selected(index) {
                    if !set_caret {
                        window.set_caret(index);
                        set_caret = true;
                    }
                    if !self.base().is_alive() || !window.wnd().is_alive() || !widget.is_alive() {
                        return None;
                    }
                    window.select(index);
                    original.insert(index);
                    if !self.base().is_alive() || !window.wnd().is_alive() || !widget.is_alive() {
                        return None;
                    }
                }
            }
            *self.original_selections.borrow_mut() = original;
        } else {
            for index in 0..widget.count_options() {
                if widget.is_option_selected(index as i32) {
                    window.select(index as i32);
                    break;
                }
            }
        }
        if !self.base().is_alive() || !window.wnd().is_alive() || !widget.is_alive() {
            return None;
        }
        window.set_top_visible_index(widget.top_visible_index());
        (self.base().is_alive() && window.wnd().is_alive()).then_some(window as Rc<dyn Window>)
    }
    fn is_data_changed(&self, view: &Rc<CpdfSdkPageView>) -> bool {
        let Some(widget) = self.base().widget() else {
            return false;
        };
        self.with_list(view, |window| {
            if widget.field_flags() & CHOICE_MULTI_SELECT != 0 {
                let original = self.original_selections.borrow();
                let mut count = 0;
                for index in 0..window.count() {
                    if window.is_item_selected(index) {
                        if !original.contains(&index) {
                            return true;
                        }
                        count += 1;
                    }
                }
                count != original.len()
            } else {
                window.cur_sel() != widget.selected_index(0)
            }
        })
        .unwrap_or(false)
    }
    fn save_data(&self, view: &Rc<CpdfSdkPageView>) {
        let Some(widget) = self.base().widget() else {
            return;
        };
        self.with_list(view, |window| {
            let top = window.top_visible_index();
            if !self.base().is_alive() || !window.wnd().is_alive() || !widget.is_alive() {
                return;
            }
            widget.clear_selection();
            if !self.base().is_alive() || !window.wnd().is_alive() || !widget.is_alive() {
                return;
            }
            if widget.field_flags() & CHOICE_MULTI_SELECT != 0 {
                for index in 0..window.count() {
                    if window.is_item_selected(index) {
                        widget.set_option_selection(index);
                        if !self.base().is_alive() || !window.wnd().is_alive() || !widget.is_alive()
                        {
                            return;
                        }
                    }
                }
            } else {
                widget.set_option_selection(window.cur_sel());
            }
            if !self.base().is_alive() || !window.wnd().is_alive() || !widget.is_alive() {
                return;
            }
            widget.set_top_visible_index(top);
            if !self.base().is_alive() || !widget.is_alive() {
                return;
            }
            widget.reset_field_appearance();
            if !self.base().is_alive() || !widget.is_alive() {
                return;
            }
            widget.update_field();
            if self.base().is_alive() && widget.is_alive() {
                self.base().set_change_mark();
            }
        });
    }
    fn get_action_data(
        &self,
        view: &Rc<CpdfSdkPageView>,
        kind: AActionType,
        data: &mut CfflFieldAction,
    ) {
        let Some(widget) = self.base().widget() else {
            return;
        };
        match kind {
            AActionType::Validate => {
                if widget.field_flags() & CHOICE_MULTI_SELECT != 0 {
                    data.s_value.clear();
                } else {
                    self.with_list(view, |window| {
                        let selected = window.cur_sel();
                        if selected >= 0 {
                            data.s_value = widget.option_label(selected);
                        }
                    });
                }
            }
            AActionType::LoseFocus | AActionType::GetFocus => {
                if widget.field_flags() & CHOICE_MULTI_SELECT != 0 {
                    data.s_value.clear();
                } else {
                    let selected = widget.selected_index(0);
                    if selected >= 0 {
                        data.s_value = widget.option_label(selected);
                    }
                }
            }
            _ => {}
        }
    }
    fn save_pwl_window_state(&self, view: &Rc<CpdfSdkPageView>) {
        let Some(selected) = self.with_list(view, |window| {
            (0..window.count())
                .filter(|&index| window.is_item_selected(index))
                .collect::<Vec<_>>()
        }) else {
            return;
        };
        // Replace rather than append: restoring a second time must not replay
        // stale selections accumulated by an earlier save (upstream omission).
        *self.state.borrow_mut() = selected;
    }
    fn recreate_pwl_window_from_saved_state(&self, view: &Rc<CpdfSdkPageView>) {
        let selected = self.state.borrow().clone();
        self.with_created_list(view, |window| {
            // NewPWLWindow initializes the document's current selection. Saved
            // editing state must replace it, including user deselections.
            for index in 0..window.count() {
                window.deselect(index);
                if !self.base().is_alive() || !window.wnd().is_alive() {
                    return;
                }
            }
            for index in selected {
                window.select(index);
                if !self.base().is_alive() || !window.wnd().is_alive() {
                    break;
                }
            }
        });
    }
    fn set_index_selected(&self, index: i32, selected: bool) -> bool {
        if !self.base().is_valid() {
            return false;
        }
        let Some(widget) = self.base().widget() else {
            return false;
        };
        if index < 0 || index as usize >= widget.count_options() {
            return false;
        }
        let Some(view) = self.base().current_page_view() else {
            return false;
        };
        self.with_list(&view, |window| {
            if selected {
                window.select(index);
            } else {
                window.deselect(index);
            }
            if !window.wnd().is_alive() {
                return false;
            }
            window.set_caret(index);
            window.wnd().is_alive()
        })
        .unwrap_or(false)
    }
    fn is_index_selected(&self, index: i32) -> bool {
        if !self.base().is_valid() {
            return false;
        }
        let Some(widget) = self.base().widget() else {
            return false;
        };
        if index < 0 || index as usize >= widget.count_options() {
            return false;
        }
        let Some(view) = self.base().current_page_view() else {
            return false;
        };
        self.with_list(&view, |window| window.is_item_selected(index))
            .unwrap_or(false)
    }
    fn reset_pwl_window(&self, view: &Rc<CpdfSdkPageView>) -> Option<Rc<dyn Window>> {
        self.text_object.reset_pwl_window(view)
    }
    fn restore_pwl_window(&self, view: &Rc<CpdfSdkPageView>) -> Option<Rc<dyn Window>> {
        self.text_object.restore_pwl_window(view)
    }
}

#[cfg(test)]
mod tests {
    use super::super::cffl_combobox::test_support::{wide, Fixture, LIST_FORM};
    use super::*;
    #[test]
    fn saved_multiselection_replaces_document_and_previous_saved_selections() {
        let fixture = Fixture::load(LIST_FORM, "Listbox_MultiSelectMultipleIndices");
        let field = CfflListBox::new(&fixture.filler(), &fixture.widget);
        field.base().set_focus_for_annot(&fixture.widget, 0);
        assert!(field.is_index_selected(1));
        assert!(field.is_index_selected(3));
        assert!(!field.is_data_changed(&fixture.view));
        assert!(field.set_index_selected(1, false));
        assert!(field.set_index_selected(4, true));
        assert!(field.is_data_changed(&fixture.view));
        field.save_pwl_window_state(&fixture.view);
        field.base().destroy_pwl_window(&fixture.view);
        field.recreate_pwl_window_from_saved_state(&fixture.view);
        assert!(!field.is_index_selected(1));
        assert!(field.is_index_selected(3));
        assert!(field.is_index_selected(4));
        assert!(field.set_index_selected(3, false));
        field.save_pwl_window_state(&fixture.view);
        field.base().destroy_pwl_window(&fixture.view);
        field.recreate_pwl_window_from_saved_state(&fixture.view);
        assert!(!field.is_index_selected(1));
        assert!(!field.is_index_selected(3));
        assert!(field.is_index_selected(4));
        field.save_data(&fixture.view);
        assert!(!fixture.widget.is_option_selected(1));
        assert!(!fixture.widget.is_option_selected(3));
        assert!(fixture.widget.is_option_selected(4));
        assert!(fixture.environment.change_mark());
        let mut data = CfflFieldAction {
            s_value: wide("must clear"),
            ..Default::default()
        };
        field.get_action_data(&fixture.view, AActionType::Validate, &mut data);
        assert!(data.s_value.is_empty());
    }
    #[test]
    fn single_selection_action_value_is_label_while_saved_value_is_export() {
        let fixture = Fixture::load(LIST_FORM, "Listbox_SingleSelect");
        let field = CfflListBox::new(&fixture.filler(), &fixture.widget);
        field.base().set_focus_for_annot(&fixture.widget, 0);
        assert!(!field.set_index_selected(-1, true));
        assert!(!field.set_index_selected(3, true));
        assert!(field.set_index_selected(2, true));
        let mut data = CfflFieldAction::default();
        field.get_action_data(&fixture.view, AActionType::Validate, &mut data);
        assert_eq!(data.s_value, wide("Qux"));
        field.save_data(&fixture.view);
        assert_eq!(fixture.widget.value(), wide("qux"));
        let mut data = CfflFieldAction::default();
        field.get_action_data(&fixture.view, AActionType::GetFocus, &mut data);
        assert_eq!(data.s_value, wide("Qux"));
    }
}
