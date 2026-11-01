// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fpdfsdk/formfiller/cffl_textfield.{h,cpp}`. The concrete text-field adapter
//! owns the common text-object base and retains saved text across window reset.
use super::cffl_fieldaction::CfflFieldAction;
use super::cffl_formfield::{CfflFormField, FormField};
use super::cffl_interactiveformfiller::CfflInteractiveFormFiller;
use super::cffl_perwindowdata::CfflPerWindowData;
use super::cffl_textobject::CfflTextObject;
use crate::fpdfdoc::cpdf_aaction::AActionType;
use crate::fpdfsdk::cpdfsdk_pageview::CpdfSdkPageView;
use crate::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use crate::fpdfsdk::pwl::cpwl_edit::CpwlEdit;
use crate::fpdfsdk::pwl::cpwl_wnd::*;
use crate::fxcrt::fx_codepage::FxCharset;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Default)]
struct TextFieldState {
    start: i32,
    end: i32,
    value: Rc<[u32]>,
}
pub struct CfflTextField {
    text_object: CfflTextObject,
    state: RefCell<TextFieldState>,
}
impl Drop for CfflTextField {
    fn drop(&mut self) {
        self.text_object.base().destroy_windows();
    }
}
impl CfflTextField {
    pub fn new(filler: &Rc<CfflInteractiveFormFiller>, widget: &Rc<CpdfSdkWidget>) -> Rc<Self> {
        let field = Rc::new(Self {
            text_object: CfflTextObject::new(filler, widget),
            state: RefCell::new(TextFieldState::default()),
        });
        let owner: Rc<dyn FormField> = field.clone();
        field.base().bind_owner(&owner);
        field
    }
    fn with_edit<R>(
        &self,
        view: &Rc<CpdfSdkPageView>,
        create: bool,
        operation: impl FnOnce(&CpwlEdit) -> R,
    ) -> Option<R> {
        let window = if create {
            self.base().create_or_update_pwl_window(view)
        } else {
            self.base().pwl_window(view)
        }?;
        if !window.wnd().is_alive() {
            return None;
        }
        Some(operation(window.as_any().downcast_ref::<CpwlEdit>()?))
    }
    fn configure_edit(&self, edit: &CpwlEdit, widget: &CpdfSdkWidget) -> bool {
        let max = widget.max_len();
        let value = widget.value();
        if !edit.wnd().is_alive() || !self.base().is_alive() {
            return false;
        }
        if max > 0 {
            if edit.wnd().has_flag(EDIT_CHAR_ARRAY) {
                edit.set_char_array(max);
                edit.set_align_format_vertical_center();
            } else {
                edit.set_limit_char(max);
            }
        }
        if !edit.wnd().is_alive() {
            return false;
        }
        edit.set_text_direction(widget.text_direction());
        edit.set_text(&value);
        edit.wnd().is_alive()
    }
}
impl FormField for CfflTextField {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn base(&self) -> &CfflFormField {
        self.text_object.base()
    }
    fn get_create_param(&self) -> CreateParams {
        let mut cp = self.base().get_create_param();
        if let Some(widget) = self.base().widget() {
            apply_text_flags(&mut cp, widget.field_flags(), widget.alignment());
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
        if let Some(owner) = self.base().owner() {
            data.set_form_field(&owner);
        }
        let edit = CpwlEdit::new(cp, Some(data));
        edit.wnd().realize();
        let widget = self.base().widget()?;
        self.configure_edit(&edit, &widget)
            .then_some(edit as Rc<dyn Window>)
    }
    fn on_char(&self, widget: &Rc<CpdfSdkWidget>, character: u32, flags: u32) -> bool {
        match character {
            13 if widget.field_flags() & (1 << 12) == 0 => {
                let Some(view) = self.base().current_page_view() else {
                    return false;
                };
                self.base().set_valid(!self.base().is_valid());
                self.base().invalidate_rect(widget.rect().get_outer_rect());
                if !self.base().is_alive() {
                    return false;
                }
                if self.base().is_valid() {
                    if let Some(window) = self.base().create_or_update_pwl_window(&view) {
                        window.set_focus();
                    }
                } else {
                    if !self.base().commit_data(&view, flags) {
                        return false;
                    }
                    self.base().destroy_pwl_window(&view);
                    return true;
                }
            }
            27 => {
                let Some(view) = self.base().current_page_view() else {
                    return false;
                };
                self.base().escape_filler(&view, true);
                return true;
            }
            _ => {}
        }
        self.base().on_char(widget, character, flags)
    }
    fn is_field_full(&self, view: &Rc<CpdfSdkPageView>) -> bool {
        self.with_edit(view, false, CpwlEdit::is_text_full)
            .unwrap_or(false)
    }
    fn is_data_changed(&self, view: &Rc<CpdfSdkPageView>) -> bool {
        let Some(widget) = self.base().widget() else {
            return false;
        };
        self.with_edit(view, false, |edit| edit.text() != widget.value())
            .unwrap_or(false)
    }
    fn save_data(&self, view: &Rc<CpdfSdkPageView>) {
        let Some(widget) = self.base().widget() else {
            return;
        };
        // Keep the source's value access before reading the live edit: lazy
        // document access can reenter the host and invalidate the control.
        let _ = widget.value();
        let Some(value) = self.with_edit(view, false, CpwlEdit::text) else {
            return;
        };
        if !self.base().is_alive() || !widget.is_alive() {
            return;
        }
        widget.set_value(&value);
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
    }
    fn get_action_data(
        &self,
        view: &Rc<CpdfSdkPageView>,
        kind: AActionType,
        action: &mut CfflFieldAction,
    ) {
        match kind {
            AActionType::KeyStroke => {
                self.with_edit(view, false, |edit| {
                    action.b_field_full = edit.is_text_full();
                    action.s_value = edit.text();
                    if action.b_field_full {
                        action.s_change.clear();
                        action.s_change_ex.clear();
                    }
                });
            }
            AActionType::Validate => {
                self.with_edit(view, false, |edit| action.s_value = edit.text());
            }
            AActionType::GetFocus | AActionType::LoseFocus => {
                if let Some(widget) = self.base().widget() {
                    action.s_value = widget.value();
                }
            }
            _ => {}
        }
    }
    fn set_action_data(
        &self,
        view: &Rc<CpdfSdkPageView>,
        kind: AActionType,
        action: &CfflFieldAction,
    ) {
        if kind == AActionType::KeyStroke {
            self.with_edit(view, false, |edit| {
                edit.set_focus();
                if edit.wnd().is_alive() {
                    edit.set_selection(action.n_sel_start, action.n_sel_end);
                }
                if edit.wnd().is_alive() {
                    edit.replace_selection(&action.s_change);
                }
            });
        }
    }
    fn save_pwl_window_state(&self, view: &Rc<CpdfSdkPageView>) {
        if let Some(state) = self.with_edit(view, false, |edit| {
            let (start, end) = edit.selection();
            TextFieldState {
                start,
                end,
                value: Rc::from(edit.text()),
            }
        }) {
            *self.state.borrow_mut() = state;
        }
    }
    fn recreate_pwl_window_from_saved_state(&self, view: &Rc<CpdfSdkPageView>) {
        let state = self.state.borrow().clone();
        self.with_edit(view, true, |edit| {
            edit.set_text(&state.value);
            if edit.wnd().is_alive() {
                edit.set_selection(state.start, state.end);
            }
        });
    }
    fn reset_pwl_window(&self, view: &Rc<CpdfSdkPageView>) -> Option<Rc<dyn Window>> {
        self.text_object.reset_pwl_window(view)
    }
    fn restore_pwl_window(&self, view: &Rc<CpdfSdkPageView>) -> Option<Rc<dyn Window>> {
        self.text_object.restore_pwl_window(view)
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
}
fn apply_text_flags(cp: &mut CreateParams, flags: u32, alignment: i32) {
    if flags & (1 << 13) != 0 {
        cp.flags |= EDIT_PASSWORD;
    }
    if flags & (1 << 12) != 0 {
        cp.flags |= EDIT_MULTILINE | EDIT_AUTO_RETURN | EDIT_TOP;
        if flags & (1 << 23) == 0 {
            cp.flags |= WINDOW_VSCROLL | EDIT_AUTO_SCROLL;
        }
    } else {
        cp.flags |= EDIT_CENTER;
        if flags & (1 << 23) == 0 {
            cp.flags |= EDIT_AUTO_SCROLL;
        }
    }
    if flags & (1 << 24) != 0 {
        cp.flags |= EDIT_CHAR_ARRAY;
    }
    if flags & (1 << 25) != 0 {
        cp.flags |= EDIT_RICH;
    }
    cp.flags |= EDIT_UNDO
        | match alignment {
            1 => EDIT_MIDDLE,
            2 => EDIT_RIGHT,
            _ => EDIT_LEFT,
        };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::{
        cpdf_array::CpdfArray, cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument,
        cpdf_scalar_object::CpdfScalarObject,
    };
    use crate::fpdfsdk::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment;
    use crate::fxcrt::fx_coordinates::CfxFloatRect;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn fixture(
        flags: u32,
        max: i32,
        value: &str,
    ) -> (
        Rc<RefCell<CpdfDocument>>,
        Rc<CpdfSdkFormFillEnvironment>,
        Rc<CpdfSdkWidget>,
        Rc<CfflTextField>,
    ) {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        let mut widget = CpdfDictionary::new();
        widget.set_new_name_for(b"Subtype", b"Widget");
        widget.set_new_name_for(b"FT", b"Tx");
        widget.set_new_string_for(b"T", b"text");
        widget.set_new_for(b"Ff", CpdfScalarObject::number_from_signed(flags as i32));
        widget.set_new_for(b"MaxLen", CpdfScalarObject::number_from_signed(max));
        widget.set_new_string_for(b"V", value.as_bytes());
        widget.set_new_string_for(b"DA", b"/Helv 12 Tf 0 g");
        widget.set_rect_for(b"Rect", CfxFloatRect::new(0.0, 0.0, 80.0, 20.0));
        let mut fields = CpdfArray::new();
        fields.append_new(widget);
        let root = document.borrow().root().unwrap();
        let acro = root
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .get_or_create_dict_for(b"AcroForm");
        acro.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Fields", fields);
        let env = CpdfSdkFormFillEnvironment::new(&document, None);
        let field = env
            .interactive_form()
            .core_form()
            .get_field(0, &[])
            .unwrap();
        let widget = Rc::new(CpdfSdkWidget::new(&document, field.get_control(0).unwrap()));
        let filler = env.interactive_form_filler();
        let text = CfflTextField::new(&filler, &widget);
        (document, env, widget, text)
    }
    #[test]
    fn source_text_field_flags_preserve_multiline_scroll_password_and_alignment() {
        let mut cp = CreateParams::default();
        apply_text_flags(&mut cp, 0, 0);
        assert_eq!(
            cp.flags,
            EDIT_CENTER | EDIT_AUTO_SCROLL | EDIT_UNDO | EDIT_LEFT
        );
        let mut cp = CreateParams::default();
        apply_text_flags(&mut cp, (1 << 12) | (1 << 13) | (1 << 24) | (1 << 25), 1);
        assert_eq!(
            cp.flags,
            EDIT_PASSWORD
                | EDIT_MULTILINE
                | EDIT_AUTO_RETURN
                | EDIT_TOP
                | WINDOW_VSCROLL
                | EDIT_AUTO_SCROLL
                | EDIT_CHAR_ARRAY
                | EDIT_RICH
                | EDIT_UNDO
                | EDIT_MIDDLE
        );
        let mut cp = CreateParams::default();
        apply_text_flags(&mut cp, (1 << 12) | (1 << 23), 2);
        assert_eq!(
            cp.flags,
            EDIT_MULTILINE | EDIT_AUTO_RETURN | EDIT_TOP | EDIT_UNDO | EDIT_RIGHT
        );
        let mut cp = CreateParams::default();
        apply_text_flags(&mut cp, 1 << 23, -1);
        assert_eq!(cp.flags, EDIT_CENTER | EDIT_UNDO | EDIT_LEFT);
    }
    #[test]
    fn text_field_initializes_limit_and_comb_windows_from_the_real_field() {
        for flags in [0, 1 << 24] {
            let (_doc, _env, widget, field) = fixture(flags, 4, "AB");
            let edit = CpwlEdit::new(field.get_create_param(), None);
            edit.wnd().realize();
            assert!(field.configure_edit(&edit, &widget));
            assert_eq!(edit.text(), wide("AB"));
            edit.set_selection(2, 2);
            edit.replace_selection(&wide("CDEFG"));
            assert_eq!(edit.text(), wide("ABCD"));
            assert!(edit.is_text_full());
            edit.select_all_text();
            assert_eq!(edit.selected_text(), wide("ABCD"));
            edit.replace_selection(&wide("Z"));
            assert_eq!(edit.text(), wide("Z"));
            assert!(!edit.is_text_full());
            assert!(edit.undo());
            assert_eq!(edit.text(), wide("ABCD"));
            edit.wnd().destroy();
        }
    }
}
