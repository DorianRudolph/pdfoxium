// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Combo field filler from `fpdfsdk/formfiller/cffl_combobox.{h,cpp}`.
//! Script execution and XFA are supplied by their runtime owners, not here.
use super::cffl_fieldaction::CfflFieldAction;
use super::cffl_formfield::{CfflFormField, FormField};
use super::cffl_interactiveformfiller::CfflInteractiveFormFiller;
use super::cffl_perwindowdata::CfflPerWindowData;
use super::cffl_textobject::CfflTextObject;
use crate::fpdfdoc::cpdf_aaction::AActionType;
use crate::fpdfsdk::cpdfsdk_pageview::CpdfSdkPageView;
use crate::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use crate::fpdfsdk::pwl::cpwl_combo_box::CpwlComboBox;
use crate::fpdfsdk::pwl::cpwl_edit::CpwlEdit;
use crate::fpdfsdk::pwl::cpwl_wnd::{CreateParams, Window, COMBOBOX_ALLOW_CUSTOM_TEXT};
use crate::fxcrt::fx_codepage::FxCharset;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

const CHOICE_EDIT: u32 = 1 << 18;
#[derive(Default, Clone)]
struct ComboBoxState {
    index: i32,
    start: i32,
    end: i32,
    value: Vec<u32>,
}

pub struct CfflComboBox {
    text_object: CfflTextObject,
    state: RefCell<ComboBoxState>,
}
impl CfflComboBox {
    pub fn new(filler: &Rc<CfflInteractiveFormFiller>, widget: &Rc<CpdfSdkWidget>) -> Rc<Self> {
        let field = Rc::new(Self {
            text_object: CfflTextObject::new(filler, widget),
            state: RefCell::new(ComboBoxState::default()),
        });
        let owner: Rc<dyn FormField> = field.clone();
        field.base().bind_owner(&owner);
        field
    }
    fn with_combo<R>(
        &self,
        view: &Rc<CpdfSdkPageView>,
        f: impl FnOnce(&CpwlComboBox) -> R,
    ) -> Option<R> {
        let window = self.base().pwl_window(view)?;
        let combo = window.as_any().downcast_ref::<CpwlComboBox>()?;
        if !self.base().is_alive() || !combo.wnd().is_alive() {
            return None;
        }
        Some(f(combo))
    }
    fn with_created_combo<R>(
        &self,
        view: &Rc<CpdfSdkPageView>,
        f: impl FnOnce(&CpwlComboBox) -> R,
    ) -> Option<R> {
        let window = self.base().create_or_update_pwl_window(view)?;
        let combo = window.as_any().downcast_ref::<CpwlComboBox>()?;
        if !self.base().is_alive() || !combo.wnd().is_alive() {
            return None;
        }
        Some(f(combo))
    }
    fn select_export_text(&self) -> Vec<u32> {
        let Some(widget) = self.base().widget() else {
            return Vec::new();
        };
        let index = self
            .base()
            .current_page_view()
            .and_then(|view| self.with_combo(&view, CpwlComboBox::selected_index))
            .unwrap_or(-1);
        widget.select_export_text(index)
    }
}
impl FormField for CfflComboBox {
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
            .is_some_and(|widget| widget.field_flags() & CHOICE_EDIT != 0)
        {
            cp.flags |= COMBOBOX_ALLOW_CUSTOM_TEXT;
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
        let window = CpwlComboBox::new(cp, Some(data));
        window.wnd().realize();
        if !self.base().is_alive() || !window.wnd().is_alive() {
            return None;
        }
        let selected = widget.selected_index(0);
        let text = if selected < 0 {
            widget.value()
        } else {
            widget.option_label(selected)
        };
        for index in 0..widget.count_options() {
            window.add_string(&widget.option_label(index as i32));
            if !self.base().is_alive() || !window.wnd().is_alive() || !widget.is_alive() {
                return None;
            }
        }
        window.set_text_direction(widget.text_direction());
        window.set_select(selected);
        if !self.base().is_alive() || !window.wnd().is_alive() || !widget.is_alive() {
            return None;
        }
        window.set_text(&text);
        (self.base().is_alive() && window.wnd().is_alive()).then_some(window as Rc<dyn Window>)
    }
    fn is_field_full(&self, view: &Rc<CpdfSdkPageView>) -> bool {
        self.with_combo(view, |combo| {
            combo.edit().is_some_and(|edit| edit.is_text_full())
        })
        .unwrap_or(false)
    }
    fn is_data_changed(&self, view: &Rc<CpdfSdkPageView>) -> bool {
        let Some(widget) = self.base().widget() else {
            return false;
        };
        self.with_combo(view, |window| {
            let selected = window.selected_index();
            if widget.field_flags() & CHOICE_EDIT == 0 || selected >= 0 {
                selected != widget.selected_index(0)
            } else {
                window.text() != widget.value()
            }
        })
        .unwrap_or(false)
    }
    fn save_data(&self, view: &Rc<CpdfSdkPageView>) {
        let Some(widget) = self.base().widget() else {
            return;
        };
        let Some((text, selected)) =
            self.with_combo(view, |window| (window.text(), window.selected_index()))
        else {
            return;
        };
        let set_value = widget.field_flags() & CHOICE_EDIT != 0
            && (selected < 0 || text != widget.option_label(selected));
        if set_value {
            widget.set_value(&text);
        } else {
            widget.set_option_selection(selected);
        }
        if !widget.is_alive() || !self.base().is_alive() {
            return;
        }
        widget.reset_field_appearance();
        if !widget.is_alive() || !self.base().is_alive() {
            return;
        }
        widget.update_field();
        if widget.is_alive() && self.base().is_alive() {
            self.base().set_change_mark();
        }
    }
    fn get_action_data(
        &self,
        view: &Rc<CpdfSdkPageView>,
        kind: AActionType,
        data: &mut CfflFieldAction,
    ) {
        match kind {
            AActionType::KeyStroke => {
                self.with_combo(view, |combo| {
                    if let Some(edit) = combo.edit() {
                        data.b_field_full = edit.is_text_full();
                        (data.n_sel_start, data.n_sel_end) = edit.selection();
                        data.s_value = edit.text();
                        data.s_change_ex = self.select_export_text();
                        if data.b_field_full {
                            data.s_change.clear();
                            data.s_change_ex.clear();
                        }
                    }
                });
            }
            AActionType::Validate => {
                self.with_combo(view, |combo| {
                    if let Some(edit) = combo.edit() {
                        data.s_value = edit.text();
                    }
                });
            }
            AActionType::LoseFocus | AActionType::GetFocus => {
                if let Some(widget) = self.base().widget() {
                    data.s_value = widget.value();
                }
            }
            _ => {}
        }
    }
    fn set_action_data(
        &self,
        view: &Rc<CpdfSdkPageView>,
        kind: AActionType,
        data: &CfflFieldAction,
    ) {
        if kind == AActionType::KeyStroke {
            self.with_combo(view, |combo| {
                if let Some(edit) = combo.edit() {
                    edit.set_selection(data.n_sel_start, data.n_sel_end);
                    if edit.wnd().is_alive() {
                        edit.replace_selection(&data.s_change);
                    }
                }
            });
        }
    }
    fn save_pwl_window_state(&self, view: &Rc<CpdfSdkPageView>) {
        let Some(state) = self
            .with_combo(view, |combo| {
                let index = combo.selected_index();
                let edit = combo.edit()?;
                let (start, end) = edit.selection();
                Some(ComboBoxState {
                    index,
                    start,
                    end,
                    value: edit.text(),
                })
            })
            .flatten()
        else {
            return;
        };
        *self.state.borrow_mut() = state;
    }
    fn recreate_pwl_window_from_saved_state(&self, view: &Rc<CpdfSdkPageView>) {
        let state = self.state.borrow().clone();
        self.with_created_combo(view, |combo| {
            if state.index >= 0 {
                combo.set_select(state.index);
                return;
            }
            if let Some(edit) = combo.edit() {
                edit.set_text(&state.value);
                if edit.wnd().is_alive() {
                    edit.set_selection(state.start, state.end);
                }
            }
        });
    }
    fn set_index_selected(&self, index: i32, selected: bool) -> bool {
        if !self.base().is_valid() || !selected {
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
        self.with_combo(&view, |combo| combo.set_select(index))
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
        self.with_combo(&view, |combo| combo.selected_index() == index)
            .unwrap_or(false)
    }
    fn on_set_focus_for_edit(&self, window: &dyn Window) {
        let Some(edit) = window.as_any().downcast_ref::<CpwlEdit>() else {
            return;
        };
        edit.set_char_set(FxCharset::ChineseSimplified);
        edit.set_ready_to_input();
        if let Some(filler) = self.base().filler() {
            filler.on_set_field_input_focus(&edit.text());
        }
    }
    fn reset_pwl_window(&self, view: &Rc<CpdfSdkPageView>) -> Option<Rc<dyn Window>> {
        self.text_object.reset_pwl_window(view)
    }
    fn restore_pwl_window(&self, view: &Rc<CpdfSdkPageView>) -> Option<Rc<dyn Window>> {
        self.text_object.restore_pwl_window(view)
    }
}
impl Drop for CfflComboBox {
    fn drop(&mut self) {
        self.base().destroy_windows();
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::fpdfapi::page::cpdf_page::CpdfPage;
    use crate::fpdfapi::parser::{cpdf_document::CpdfDocument, cpdf_parser::ParserError};
    use crate::fpdfsdk::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment;
    use crate::fpdfsdk::cpdfsdk_pageview::SdkPage;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
    pub(crate) fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    pub(crate) struct Fixture {
        pub(crate) _document: Rc<RefCell<CpdfDocument>>,
        pub(crate) _page_model: CpdfPage,
        pub(crate) _page: Rc<SdkPage>,
        pub(crate) environment: Rc<CpdfSdkFormFillEnvironment>,
        pub(crate) view: Rc<CpdfSdkPageView>,
        pub(crate) widget: Rc<CpdfSdkWidget>,
    }
    impl Fixture {
        pub(crate) fn load(bytes: &[u8], name: &str) -> Self {
            let document = Rc::new(RefCell::new(CpdfDocument::new()));
            assert_eq!(
                document.borrow_mut().load_doc(
                    Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec())),
                    b""
                ),
                ParserError::Success
            );
            let page_model = CpdfPage::from_document(&mut document.borrow_mut(), 0).unwrap();
            let page = SdkPage::new(&document, &page_model);
            let environment = CpdfSdkFormFillEnvironment::new(&document, None);
            let view = environment.get_or_create_page_view(&page).unwrap();
            view.set_valid(true);
            let wanted = wide(name);
            let widget = view
                .annot_list()
                .into_iter()
                .filter_map(|annot| annot.widget())
                .find(|widget| {
                    widget
                        .field()
                        .is_some_and(|field| field.full_name() == wanted)
                })
                .unwrap();
            Self {
                _document: document,
                _page_model: page_model,
                _page: page,
                environment,
                view,
                widget,
            }
        }
        pub(crate) fn filler(&self) -> Rc<CfflInteractiveFormFiller> {
            self.environment.get_interactive_form_filler()
        }
    }
    pub(crate) const COMBO_FORM: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/combobox_form.pdf"
    ));
    pub(crate) const LIST_FORM: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/listbox_form.pdf"
    ));
}

#[cfg(test)]
mod tests {
    use super::test_support::{wide, Fixture, COMBO_FORM};
    use super::*;
    #[test]
    fn source_get_action_data_preserves_value_and_export_text() {
        // Complete CFFLComboBoxEmbedderTest.GetActionData assertions with the
        // same fixture and normal preselected field, through the safe owners.
        let fixture = Fixture::load(COMBO_FORM, "Combo1");
        let field = CfflComboBox::new(&fixture.filler(), &fixture.widget);
        field.base().set_focus_for_annot(&fixture.widget, 0);
        for (kind, change_ex) in [
            (AActionType::KeyStroke, "Banana"),
            (AActionType::Validate, ""),
            (AActionType::GetFocus, ""),
        ] {
            let mut data = CfflFieldAction::default();
            field.get_action_data(&fixture.view, kind, &mut data);
            assert_eq!(data.s_value, wide("Banana"));
            assert_eq!(data.s_change_ex, wide(change_ex));
        }
    }
    #[test]
    fn source_set_action_data_replaces_only_selected_text() {
        // CFFLComboBoxEmbedderTest.SetActionData uses programmatic replacement
        // on the ordinary combo's read-only edit child, as the source does.
        let fixture = Fixture::load(COMBO_FORM, "Combo1");
        let field = CfflComboBox::new(&fixture.filler(), &fixture.widget);
        field.base().set_focus_for_annot(&fixture.widget, 0);
        let input = CfflFieldAction {
            n_sel_start: 2,
            n_sel_end: 4,
            s_change: wide("Hamster"),
            ..Default::default()
        };
        field.set_action_data(&fixture.view, AActionType::KeyStroke, &input);
        let mut output = CfflFieldAction::default();
        field.get_action_data(&fixture.view, AActionType::KeyStroke, &mut output);
        assert_eq!(output.s_value, wide("BaHamsterna"));
    }
    #[test]
    fn custom_text_and_option_exports_save_through_the_widget() {
        let fixture = Fixture::load(COMBO_FORM, "Combo_Editable");
        let field = CfflComboBox::new(&fixture.filler(), &fixture.widget);
        field.base().set_focus_for_annot(&fixture.widget, 0);
        assert!(!field.set_index_selected(-1, true));
        assert!(!field.set_index_selected(99, true));
        assert!(!field.set_index_selected(0, false));
        field
            .with_combo(&fixture.view, |combo| {
                combo.set_text(&wide("Custom choice"))
            })
            .unwrap();
        assert!(field.is_data_changed(&fixture.view));
        field.save_data(&fixture.view);
        assert_eq!(fixture.widget.value(), wide("Custom choice"));
        assert!(field.set_index_selected(1, true));
        assert!(field.is_index_selected(1));
        field.save_data(&fixture.view);
        assert_eq!(fixture.widget.value(), wide("bar"));
        assert!(fixture.environment.change_mark());
    }
}
