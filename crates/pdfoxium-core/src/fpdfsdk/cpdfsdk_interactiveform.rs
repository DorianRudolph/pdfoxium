// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fpdfsdk/cpdfsdk_interactiveform.{h,cpp}`: ordinary SDK form notification,
//! widget lookup, appearance updates, form actions, FDF submission and highlights.
//! Validation, calculation and formatting execute through the document Rust
//! JavaScript runtime. Foreground XFA shares its bound model values.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use super::cpdfsdk_formfillenvironment::{ActionError, CpdfSdkFormFillEnvironment};
use super::cpdfsdk_widget::CpdfSdkWidget;
use super::formfiller::cffl_fieldaction::CfflFieldAction;
use crate::fpdfapi::parser::cfdf_document::CfdfDocument;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::fpdf_parser_decode::pdf_decode_text;
use crate::fpdfdoc::cpdf_aaction::AActionType;
use crate::fpdfdoc::cpdf_action::CpdfAction;
use crate::fpdfdoc::cpdf_formcontrol::CpdfFormControl;
#[cfg(feature = "js")]
use crate::fpdfdoc::cpdf_formfield::NotificationOption;
use crate::fpdfdoc::cpdf_formfield::{dict_bytes, dict_get, typed, CpdfFormField, FormFieldType};
use crate::fpdfdoc::cpdf_interactiveform::{CpdfInteractiveForm, FormNotifier};
use crate::fxcrt::fx_codepage::{wide_char_to_multi_byte, FxCodePage};
#[cfg(feature = "js")]
use crate::fxcrt::fx_string::utf16_encode;

#[cfg(feature = "xfa")]
const FIELD_TYPE_COUNT: usize = 16;
#[cfg(not(feature = "xfa"))]
const FIELD_TYPE_COUNT: usize = 8;
const WHITE_BGR: u32 = 0x00ff_ffff;

fn combo_or_text(kind: FormFieldType) -> bool {
    matches!(kind, FormFieldType::ComboBox | FormFieldType::TextField)
}
fn control_id(control: &CpdfFormControl) -> usize {
    std::ptr::from_ref(control) as usize
}
fn def_ansi(value: &[u32]) -> Vec<u8> {
    let count = wide_char_to_multi_byte(FxCodePage::DefAnsi, value, &mut []);
    let mut bytes = vec![0; count];
    wide_char_to_multi_byte(FxCodePage::DefAnsi, value, &mut bytes);
    bytes
}
fn url_component(value: &[u8], output: &mut Vec<u8>) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in value {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                output.push(byte)
            }
            b' ' => output.push(b'+'),
            _ => output.extend_from_slice(&[
                b'%',
                HEX[(byte >> 4) as usize],
                HEX[(byte & 15) as usize],
            ]),
        }
    }
}
/// The FDF traversal and portable default-ANSI conversion follow the source.
/// Escape query components rather than reproducing its unescaped `&`/`=` bug.
pub fn fdf_to_url_encoded_data(buffer: Vec<u8>) -> Vec<u8> {
    let Some(document) = CfdfDocument::parse_memory(&buffer) else {
        return buffer;
    };
    let Some(fields) = document
        .root()
        .and_then(|root| typed(dict_get(&root, b"FDF"), ObjectType::Dictionary))
        .and_then(|main| typed(dict_get(&main, b"Fields"), ObjectType::Array))
    else {
        return Vec::new();
    };
    let fields: Vec<ObjectRef> = {
        let borrowed = fields.borrow();
        let array = borrowed
            .as_any()
            .downcast_ref::<CpdfArray>()
            .expect("FDF fields");
        (0..array.size())
            .filter_map(|index| array.get_dict_at(index))
            .collect()
    };
    let mut output = Vec::new();
    for field in fields {
        if !output.is_empty() {
            output.push(b'&');
        }
        let name = dict_get(&field, b"T")
            .map_or_else(Vec::new, |object| object.borrow().get_unicode_text());
        let value = pdf_decode_text(&dict_bytes(&field, b"V"));
        url_component(&def_ansi(&name), &mut output);
        output.push(b'=');
        url_component(&def_ansi(&value), &mut output);
    }
    output
}

pub struct CpdfSdkInteractiveForm {
    environment: Weak<CpdfSdkFormFillEnvironment>,
    form: Rc<CpdfInteractiveForm>,
    widgets: RefCell<HashMap<usize, Weak<CpdfSdkWidget>>>,
    calculate: Cell<bool>,
    #[cfg(feature = "xfa")]
    xfa_calculate: Cell<bool>,
    #[cfg(feature = "xfa")]
    xfa_validations: Cell<bool>,
    #[cfg(feature = "js")]
    busy: Cell<bool>,
    highlight_alpha: Cell<u8>,
    highlight_colors: Cell<[u32; FIELD_TYPE_COUNT]>,
    needs_highlight: Cell<[bool; FIELD_TYPE_COUNT]>,
}
impl CpdfSdkInteractiveForm {
    pub fn new(
        environment: Weak<CpdfSdkFormFillEnvironment>,
        document: &Rc<RefCell<CpdfDocument>>,
    ) -> Rc<Self> {
        let result = Rc::new(Self {
            environment,
            form: CpdfInteractiveForm::new(document),
            widgets: RefCell::new(HashMap::new()),
            calculate: Cell::new(true),
            #[cfg(feature = "xfa")]
            xfa_calculate: Cell::new(true),
            #[cfg(feature = "xfa")]
            xfa_validations: Cell::new(true),
            #[cfg(feature = "js")]
            busy: Cell::new(false),
            highlight_alpha: Cell::new(0),
            highlight_colors: Cell::new([WHITE_BGR; FIELD_TYPE_COUNT]),
            needs_highlight: Cell::new([false; FIELD_TYPE_COUNT]),
        });
        let notifier: Rc<dyn FormNotifier> = result.clone();
        result.form.set_notifier_iface(Some(&notifier));
        result
    }
    pub fn core_form(&self) -> Rc<CpdfInteractiveForm> {
        self.form.clone()
    }
    pub fn interactive_form(&self) -> Rc<CpdfInteractiveForm> {
        self.core_form()
    }
    pub fn add_map(&self, control: &Rc<CpdfFormControl>, widget: &Rc<CpdfSdkWidget>) {
        self.widgets
            .borrow_mut()
            .insert(control_id(control), Rc::downgrade(widget));
    }
    pub fn remove_map(&self, control: &CpdfFormControl) {
        self.widgets.borrow_mut().remove(&control_id(control));
    }
    pub fn get_widget(&self, control: &CpdfFormControl) -> Option<Rc<CpdfSdkWidget>> {
        let observed = self.widgets.borrow().get(&control_id(control)).cloned();
        if let Some(widget) = observed
            .and_then(|widget| widget.upgrade())
            .filter(|widget| widget.is_alive())
        {
            return Some(widget);
        }
        let environment = self.environment.upgrade()?;
        let document = environment.document()?;
        let control_dict = control.widget_dict();
        if let Some(page_dict) = typed(dict_get(&control_dict, b"P"), ObjectType::Dictionary) {
            let number = page_dict.borrow().obj_num();
            let index = document.borrow_mut().page_index(number);
            if let Some(page) =
                index.and_then(|index| environment.get_page_view_at_index(index as i32))
            {
                if let Some(widget) = page.widget_by_dict(&control_dict) {
                    return Some(widget);
                }
            }
        }
        let index = Self::page_index_by_annot_dict(&document, &control_dict)?;
        environment
            .get_page_view_at_index(index)?
            .widget_by_dict(&control_dict)
    }
    pub fn widgets_for_field(&self, field: &CpdfFormField) -> Vec<Rc<CpdfSdkWidget>> {
        field
            .controls()
            .iter()
            .filter_map(|control| self.get_widget(control))
            .collect()
    }
    pub fn widgets_by_name(&self, name: &[u32]) -> Vec<Rc<CpdfSdkWidget>> {
        let mut widgets = Vec::new();
        for index in 0..self.form.count_fields(name) {
            if let Some(field) = self.form.get_field(index, name) {
                widgets.extend(self.widgets_for_field(&field));
            }
        }
        widgets
    }
    fn page_index_by_annot_dict(
        document: &Rc<RefCell<CpdfDocument>>,
        annot: &ObjectRef,
    ) -> Option<i32> {
        let count = document.borrow().page_count();
        for index in 0..count {
            let Some(page) = document.borrow_mut().page_dictionary(index) else {
                continue;
            };
            let Some(annots) = typed(dict_get(&page, b"Annots"), ObjectType::Array) else {
                continue;
            };
            let entries = {
                let borrowed = annots.borrow();
                let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
                (0..array.size())
                    .filter_map(|index| array.get_object_at(index))
                    .collect::<Vec<_>>()
            };
            if entries
                .iter()
                .filter_map(get_direct_object)
                .any(|entry| Rc::ptr_eq(&entry, annot))
            {
                return Some(index as i32);
            }
        }
        None
    }
    #[cfg(feature = "xfa")]
    pub fn is_xfa_calculate_enabled(&self) -> bool {
        self.xfa_calculate.get()
    }
    #[cfg(feature = "xfa")]
    pub fn xfa_enable_calculate(&self, enabled: bool) {
        self.xfa_calculate.set(enabled);
    }
    #[cfg(feature = "xfa")]
    pub fn is_xfa_validations_enabled(&self) -> bool {
        self.xfa_validations.get()
    }
    #[cfg(feature = "xfa")]
    pub fn xfa_set_validations_enabled(&self, enabled: bool) {
        self.xfa_validations.set(enabled);
    }
    #[cfg(feature = "xfa")]
    pub fn synchronize_field(&self, field: &CpdfFormField) {
        for control in field.controls() {
            if let Some(widget) = self.get_widget(&control) {
                widget.synchronize(false);
            }
        }
    }
    pub fn enable_calculate(&self, enabled: bool) {
        self.calculate.set(enabled);
    }
    pub fn is_calculate_enabled(&self) -> bool {
        self.calculate.get()
    }
    pub fn on_validate(&self, field: &CpdfFormField, value: &[u32]) -> Result<bool, ActionError> {
        let actions = field.additional_action();
        if !actions.action_exist(AActionType::Validate) {
            return Ok(true);
        }
        let action = actions.action(AActionType::Validate);
        if !action.has_dict() {
            return Ok(true);
        }
        // BeforeValueChange uses a separate action buffer: changes to
        // event.value do not replace the proposed value in this source path.
        let mut data = CfflFieldAction {
            s_value: value.to_vec(),
            ..Default::default()
        };
        if action.javascript().is_empty() {
            return Ok(true);
        }
        self.environment
            .upgrade()
            .ok_or(ActionError::Detached)?
            .do_action_field_javascript(&action, AActionType::Validate, field, &mut data)?;
        Ok(data.b_rc)
    }
    pub fn on_calculate(&self, trigger: Option<&CpdfFormField>) -> Result<(), ActionError> {
        #[cfg(not(feature = "js"))]
        {
            let _ = trigger;
            Ok(())
        }
        #[cfg(feature = "js")]
        {
            if let Some(environment) = self.environment.upgrade() {
                if !environment.is_alive() {
                    return Err(ActionError::Detached);
                }
                if !environment.is_javascript_enabled() {
                    return Ok(());
                }
            }
            if self.busy.get() {
                return Ok(());
            }
            self.busy.set(true);
            struct Restore<'a>(&'a Cell<bool>);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    self.0.set(false);
                }
            }
            let _restore = Restore(&self.busy);
            if !self.is_calculate_enabled() {
                return Ok(());
            }
            let mut first_error = None;
            let count = self.form.count_fields_in_calculation_order();
            for index in 0..count {
                let Some(field) = self.form.field_in_calculation_order(index as i32) else {
                    continue;
                };
                if !combo_or_text(field.field_type()) {
                    continue;
                }
                let actions = field.additional_action();
                if !actions.action_exist(AActionType::Calculate) {
                    continue;
                }
                let action = actions.action(AActionType::Calculate);
                if !action.has_dict() {
                    continue;
                }
                let script = action.javascript();
                if script.is_empty() {
                    continue;
                }
                let environment = self.environment.upgrade().ok_or(ActionError::Detached)?;
                let runtime = environment.get_or_create_runtime()?;
                let old_value = field.value();
                let mut value = old_value.clone();
                let mut rc = true;
                let result = runtime.run_calculate_script(
                    trigger,
                    &field,
                    &mut value,
                    &mut rc,
                    &utf16_encode(&script),
                );
                if !environment.is_alive() {
                    return Err(ActionError::Detached);
                }
                if let Err(error) = result {
                    // Native calculation proceeds through CO after an exception.
                    // Keep its first diagnostic while retaining that operation order.
                    first_error.get_or_insert(environment.record_js_error(error));
                    continue;
                }
                if rc && value != old_value {
                    field.set_value(&value, NotificationOption::Notify);
                    if let Some(error) = environment.action_error() {
                        first_error.get_or_insert(error);
                    }
                }
            }
            first_error.map_or(Ok(()), Err)
        }
    }
    pub fn on_format(&self, field: &CpdfFormField) -> Result<Option<Vec<u32>>, ActionError> {
        #[cfg(not(feature = "js"))]
        {
            let _ = field;
            Ok(None)
        }
        #[cfg(feature = "js")]
        {
            let actions = field.additional_action();
            if !actions.action_exist(AActionType::Format) {
                return Ok(None);
            }
            let action = actions.action(AActionType::Format);
            if !action.has_dict() {
                return Ok(None);
            }
            let script = action.javascript();
            if script.is_empty() {
                return Ok(None);
            }
            let environment = self.environment.upgrade().ok_or(ActionError::Detached)?;
            if !environment.is_alive() {
                return Err(ActionError::Detached);
            }
            if !environment.is_javascript_enabled() {
                return Ok(None);
            }
            let mut value = field.value();
            if field.field_type() == FormFieldType::ComboBox && field.count_selected_items() > 0 {
                let index = field.selected_index(0);
                if index >= 0 {
                    value = field.option_label(index);
                }
            }
            environment
                .get_or_create_runtime()?
                .run_format_script(field, &mut value, &utf16_encode(&script))
                .map_err(|error| environment.record_js_error(error))?;
            if !environment.is_alive() {
                return Err(ActionError::Detached);
            }
            Ok(Some(value))
        }
    }
    pub fn reset_field_appearance(&self, field: &CpdfFormField, value: Option<&[u32]>) {
        for widget in self.widgets_for_field(field) {
            if widget.is_alive() {
                widget.reset_appearance(value, true);
            }
        }
    }
    pub fn update_field(&self, field: &CpdfFormField) {
        let Some(environment) = self.environment.upgrade() else {
            return;
        };
        for widget in self.widgets_for_field(field) {
            if widget.is_alive() {
                environment.invalidate_widget(&widget);
            }
        }
    }
    pub fn do_action_hide(&self, action: &CpdfAction) -> bool {
        let fields = self.fields_from_objects(&action.all_fields());
        let hide = action.hide_status();
        let mut changed = false;
        for field in fields {
            for widget in self.widgets_for_field(&field) {
                if !widget.is_alive() {
                    continue;
                }
                let mut flags = widget.annot().flags() & !(1 | 32);
                if hide {
                    flags |= 2;
                } else {
                    flags &= !2;
                }
                widget.annot().set_flags(flags);
                if let Some(environment) = self.environment.upgrade() {
                    environment.invalidate_widget(&widget);
                }
                changed = true;
            }
        }
        changed
    }
    pub fn do_action_reset_form(&self, action: &CpdfAction) {
        if !action.has_fields() {
            self.form.reset_form();
            return;
        }
        let fields = self.fields_from_objects(&action.all_fields());
        self.form
            .reset_form_fields(&fields, action.flags() & 1 == 0);
    }
    pub fn fields_from_objects(&self, objects: &[ObjectRef]) -> Vec<Rc<CpdfFormField>> {
        objects
            .iter()
            .filter_map(|object| {
                let object = object.borrow();
                if object.object_type() != ObjectType::String {
                    return None;
                }
                self.form.get_field(0, &object.get_unicode_text())
            })
            .collect()
    }
    pub fn do_action_submit_form(&self, action: &CpdfAction) -> Result<bool, ActionError> {
        let destination = action.file_path();
        if destination.is_empty() {
            return Ok(false);
        }
        if action.has_fields() {
            let fields = self.fields_from_objects(&action.all_fields());
            if !fields.is_empty() {
                let include = action.flags() & 1 == 0;
                if !self.form.check_required_fields(Some(&fields), include) {
                    return Ok(false);
                }
                return self.submit_fields(&destination, &fields, include, false);
            }
        }
        if !self.form.check_required_fields(None, true) {
            return Ok(false);
        }
        self.submit_form(&destination)
    }
    fn file_path(&self) -> Vec<u32> {
        self.environment
            .upgrade()
            .map_or_else(Vec::new, |environment| environment.file_path())
    }
    pub fn export_fields_to_fdf_text_buf(
        &self,
        fields: &[Rc<CpdfFormField>],
        include: bool,
    ) -> Vec<u8> {
        self.form
            .export_fields_to_fdf(&self.file_path(), fields, include)
            .write_to_string()
    }
    pub fn export_form_to_fdf_text_buf(&self) -> Vec<u8> {
        self.form.export_to_fdf(&self.file_path()).write_to_string()
    }
    pub fn submit_fields(
        &self,
        destination: &[u32],
        fields: &[Rc<CpdfFormField>],
        include: bool,
        url_encoded: bool,
    ) -> Result<bool, ActionError> {
        let mut data = self.export_fields_to_fdf_text_buf(fields, include);
        if data.is_empty() {
            return Ok(false);
        }
        if url_encoded {
            data = fdf_to_url_encoded_data(data);
            if data.is_empty() {
                return Ok(false);
            }
        }
        self.environment
            .upgrade()
            .ok_or(ActionError::Detached)?
            .submit_form(&data, destination)
    }
    pub fn submit_form(&self, destination: &[u32]) -> Result<bool, ActionError> {
        if destination.is_empty() {
            return Ok(false);
        }
        let data = self.export_form_to_fdf_text_buf();
        if data.is_empty() {
            return Ok(false);
        }
        self.environment
            .upgrade()
            .ok_or(ActionError::Detached)?
            .submit_form(&data, destination)
    }
    pub fn is_need_highlight(&self, kind: FormFieldType) -> bool {
        if kind == FormFieldType::Unknown {
            return false;
        }
        let enabled = self.needs_highlight.get();
        #[cfg(feature = "xfa")]
        if kind as usize >= FormFieldType::Xfa as usize && !enabled[kind as usize] {
            return enabled[FormFieldType::Xfa as usize];
        }
        enabled[kind as usize]
    }
    pub fn remove_all_highlights(&self) {
        self.highlight_colors.set([WHITE_BGR; FIELD_TYPE_COUNT]);
        self.needs_highlight.set([false; FIELD_TYPE_COUNT]);
    }
    pub fn set_highlight_alpha(&self, alpha: u8) {
        self.highlight_alpha.set(alpha);
    }
    pub fn highlight_alpha(&self) -> u8 {
        self.highlight_alpha.get()
    }
    pub fn set_highlight_color(&self, color: u32, kind: FormFieldType) {
        if kind == FormFieldType::Unknown {
            return;
        }
        let mut colors = self.highlight_colors.get();
        colors[kind as usize] = color;
        self.highlight_colors.set(colors);
        let mut enabled = self.needs_highlight.get();
        enabled[kind as usize] = true;
        self.needs_highlight.set(enabled);
    }
    pub fn set_all_highlight_colors(&self, color: u32) {
        self.highlight_colors.set([color; FIELD_TYPE_COUNT]);
        self.needs_highlight.set([true; FIELD_TYPE_COUNT]);
    }
    pub fn highlight_color(&self, kind: FormFieldType) -> u32 {
        if kind == FormFieldType::Unknown {
            return WHITE_BGR;
        }
        #[cfg(feature = "xfa")]
        let enabled = self.needs_highlight.get();
        let colors = self.highlight_colors.get();
        #[cfg(feature = "xfa")]
        if kind as usize >= FormFieldType::Xfa as usize
            && !enabled[kind as usize]
            && enabled[FormFieldType::Xfa as usize]
        {
            return colors[FormFieldType::Xfa as usize];
        }
        colors[kind as usize]
    }
    fn record_error(&self, error: ActionError) {
        if let Some(environment) = self.environment.upgrade() {
            environment.record_action_error(error);
        }
    }
}
impl FormNotifier for CpdfSdkInteractiveForm {
    fn before_value_change(&self, field: &CpdfFormField, value: &[u32]) -> bool {
        if !combo_or_text(field.field_type()) {
            return true;
        }
        match self.on_validate(field, value) {
            Ok(result) => result,
            Err(error) => {
                self.record_error(error);
                false
            }
        }
    }
    fn after_value_change(&self, field: &CpdfFormField) {
        #[cfg(feature = "xfa")]
        self.synchronize_field(field);
        if !combo_or_text(field.field_type()) {
            return;
        }
        if let Err(error) = self.on_calculate(Some(field)) {
            self.record_error(error);
        }
        let value = match self.on_format(field) {
            Ok(value) => value,
            Err(error) => {
                self.record_error(error);
                None
            }
        };
        self.reset_field_appearance(field, value.as_deref());
        self.update_field(field);
    }
    fn before_selection_change(&self, field: &CpdfFormField, value: &[u32]) -> bool {
        if field.field_type() != FormFieldType::ListBox {
            return true;
        }
        match self.on_validate(field, value) {
            Ok(result) => result,
            Err(error) => {
                self.record_error(error);
                false
            }
        }
    }
    fn after_selection_change(&self, field: &CpdfFormField) {
        if field.field_type() != FormFieldType::ListBox {
            return;
        }
        if let Err(error) = self.on_calculate(Some(field)) {
            self.record_error(error);
        }
        self.reset_field_appearance(field, None);
        self.update_field(field);
    }
    fn after_checked_status_change(&self, field: &CpdfFormField) {
        if !matches!(
            field.field_type(),
            FormFieldType::CheckBox | FormFieldType::RadioButton
        ) {
            return;
        }
        if let Err(error) = self.on_calculate(Some(field)) {
            self.record_error(error);
        }
        self.update_field(field);
    }
    fn after_form_reset(&self, _form: &CpdfInteractiveForm) {
        if let Err(error) = self.on_calculate(None) {
            self.record_error(error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_string::CpdfString;
    use crate::fpdfdoc::cpdf_formfield::{dict_mut, NotificationOption};

    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn fixture(
        name: &str,
        value: &str,
    ) -> (
        Rc<RefCell<CpdfDocument>>,
        Rc<CpdfSdkInteractiveForm>,
        Rc<CpdfFormField>,
    ) {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        let document = Rc::new(RefCell::new(document));
        let mut widget = CpdfDictionary::new();
        widget.set_new_name_for(b"Subtype", b"Widget");
        widget.set_new_name_for(b"FT", b"Tx");
        widget.set_new_for(b"T", CpdfString::from_unicode(None, &wide(name)));
        widget.set_new_for(b"V", CpdfString::from_unicode(None, &wide(value)));
        widget.set_new_for(b"DV", CpdfString::from_unicode(None, &wide("default")));
        let mut fields = CpdfArray::new();
        fields.append_new(widget);
        let root = document.borrow().root().unwrap();
        dict_mut(&root, |root| {
            let mut form = CpdfDictionary::new();
            form.set_new_for(b"Fields", fields);
            root.set_new_for(b"AcroForm", form);
        });
        let form = CpdfSdkInteractiveForm::new(Weak::new(), &document);
        let field = form.form.get_field(0, &[]).unwrap();
        (document, form, field)
    }

    #[test]
    fn reset_and_field_filtering_preserve_real_form_values_and_fdf() {
        let (_document, form, field) = fixture("text", "original");
        let name = retain_object(CpdfString::from_unicode(None, &wide("text")));
        let ignored = retain_object(CpdfScalarObject::number_from_signed(7));
        let fields = form.fields_from_objects(&[ignored, name.clone()]);
        assert_eq!(fields.len(), 1);
        assert!(Rc::ptr_eq(&fields[0], &field));
        let mut reset = CpdfDictionary::new();
        reset.set_new_name_for(b"S", b"ResetForm");
        let mut selected = CpdfArray::new();
        selected.append(name);
        reset.set_new_for(b"Fields", selected);
        let action = CpdfAction::new(Some(retain_object(reset)));
        assert!(field.set_value(&wide("edited"), NotificationOption::Notify));
        form.do_action_reset_form(&action);
        assert_eq!(field.value(), wide("default"));
        let fdf = CfdfDocument::parse_memory(&form.export_form_to_fdf_text_buf()).unwrap();
        let main = typed(
            dict_get(&fdf.root().unwrap(), b"FDF"),
            ObjectType::Dictionary,
        )
        .unwrap();
        let entries = typed(dict_get(&main, b"Fields"), ObjectType::Array).unwrap();
        let entry = entries
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .get_dict_at(0)
            .unwrap();
        assert_eq!(
            dict_get(&entry, b"T").unwrap().borrow().get_unicode_text(),
            wide("text")
        );
        assert_eq!(
            dict_get(&entry, b"V").unwrap().borrow().get_unicode_text(),
            wide("default")
        );
        assert!(matches!(
            form.submit_form(&wide("https://example.invalid/")),
            Err(ActionError::Detached)
        ));
    }

    #[test]
    fn form_url_components_escape_delimiters_and_invalid_fdf_is_preserved() {
        let (_document, form, _) = fixture("a &+=", "é\0 b&c=d+");
        assert_eq!(
            fdf_to_url_encoded_data(form.export_form_to_fdf_text_buf()),
            b"a+%26%2B%3D=%E9%00+b%26c%3Dd%2B"
        );
        assert_eq!(
            fdf_to_url_encoded_data(b"not an FDF document".to_vec()),
            b"not an FDF document"
        );
        assert!(
            fdf_to_url_encoded_data(CfdfDocument::create_new_doc().write_to_string()).is_empty()
        );
    }

    #[cfg(feature = "js")]
    fn script(field: &CpdfFormField, event: &[u8], body: &str) {
        let dict = field.field_dict();
        let actions = typed(dict_get(&dict, b"AA"), ObjectType::Dictionary).unwrap_or_else(|| {
            let actions = retain_object(CpdfDictionary::new());
            dict_mut(&dict, |dict| {
                dict.set_for(b"AA", Some(actions.clone()));
            });
            actions
        });
        let mut action = CpdfDictionary::new();
        action.set_new_name_for(b"S", b"JavaScript");
        action.set_new_for(b"JS", CpdfString::from_unicode(None, &wide(body)));
        dict_mut(&actions, |actions| {
            actions.set_new_for(event, action);
        });
    }
    #[cfg(feature = "js")]
    fn runtime_fixture() -> (
        Rc<RefCell<CpdfDocument>>,
        Rc<CpdfSdkFormFillEnvironment>,
        Rc<CpdfSdkInteractiveForm>,
        Rc<CpdfFormField>,
    ) {
        let (document, _, _) = fixture("text", "original");
        let env = CpdfSdkFormFillEnvironment::new(&document, None);
        let form = env.interactive_form();
        let field = form.form.get_field(0, &[]).unwrap();
        (document, env, form, field)
    }
    #[test]
    #[cfg(feature = "js")]
    fn validation_format_and_calculation_execute_real_scripts_and_restore_guards() {
        let (_document, env, form, field) = runtime_fixture();
        assert!(form.on_validate(&field, &wide("new")).unwrap());
        assert!(form.on_format(&field).unwrap().is_none());
        script(&field, b"V", "event.rc = event.value !== 'blocked';");
        assert!(env
            .run_input_event(|| form.on_validate(&field, &wide("new")))
            .unwrap());
        assert!(!env
            .run_input_event(|| form.on_validate(&field, &wide("blocked")))
            .unwrap());
        assert!(
            !env.run_input_event(|| field.set_value(&wide("blocked"), NotificationOption::Notify))
        );
        assert_eq!(field.value(), wide("original"));
        assert!(env.action_error().is_none());
        script(&field, b"F", "event.value = event.value.toUpperCase();");
        assert_eq!(
            env.run_input_event(|| form.on_format(&field)).unwrap(),
            Some(wide("ORIGINAL"))
        );
        assert_eq!(field.value(), wide("original"));
        let mut order = CpdfArray::new();
        order.append(field.field_dict());
        dict_mut(&form.form.form_dict().unwrap(), |dict| {
            dict.set_new_for(b"CO", order);
        });
        script(&field, b"C", "throw new Error('calculation failure');");
        for _ in 0..2 {
            assert_eq!(
                env.run_input_event(|| form.on_calculate(None)),
                Err(ActionError::JavaScriptException)
            );
            assert!(
                String::from_utf16_lossy(&env.last_js_error().unwrap().message)
                    .contains("calculation failure")
            );
        }
        form.enable_calculate(false);
        assert!(env.run_input_event(|| form.on_calculate(None)).is_ok());
        form.enable_calculate(true);
        script(&field, b"C", "event.value = 'calculated';");
        assert!(env.run_input_event(|| form.on_calculate(None)).is_ok());
        assert_eq!(field.value(), wide("calculated"));
        script(&field, b"C", "event.rc = false; event.value = 'vetoed';");
        assert!(env.run_input_event(|| form.on_calculate(None)).is_ok());
        assert_eq!(field.value(), wide("calculated"));
        script(&field, b"V", "throw new Error('validation failure');");
        assert!(!env.run_input_event(|| field.set_value(&wide("new"), NotificationOption::Notify)));
        assert_eq!(field.value(), wide("calculated"));
        assert_eq!(env.action_error(), Some(ActionError::JavaScriptException));
    }
    #[test]
    #[cfg(feature = "js")]
    fn explicitly_disabled_javascript_skips_validation_calculation_and_format() {
        struct DisabledHost;
        impl super::super::cpdfsdk_formfillenvironment::FormFillHost for DisabledHost {
            fn javascript_enabled(&self) -> bool {
                false
            }
        }
        let (document, _old_form, _) = fixture("text", "original");
        let host: Rc<dyn super::super::cpdfsdk_formfillenvironment::FormFillHost> =
            Rc::new(DisabledHost);
        let env = CpdfSdkFormFillEnvironment::new(&document, Some(&host));
        let form = env.interactive_form();
        let field = form.form.get_field(0, &[]).unwrap();
        for event in [b"V", b"C", b"F"] {
            script(&field, event, "throw new Error('disabled script');");
        }
        let mut order = CpdfArray::new();
        order.append(field.field_dict());
        dict_mut(&form.form.form_dict().unwrap(), |dict| {
            dict.set_new_for(b"CO", order);
        });
        assert!(form.on_validate(&field, &wide("changed")).unwrap());
        assert_eq!(form.on_calculate(None), Ok(()));
        assert_eq!(form.on_format(&field), Ok(None));
        assert!(field.set_value(&wide("changed"), NotificationOption::Notify));
        assert_eq!(field.value(), wide("changed"));
        assert_eq!(env.action_error(), None);
        assert!(env.last_js_error().is_none());
    }
    #[test]
    #[cfg(feature = "js")]
    fn calculation_preserves_order_and_continues_after_an_exception() {
        let (document, _old_form, old_a) = fixture("a", "oldA");
        script(&old_a, b"C", "throw new Error('first calculation');");
        let mut second = CpdfDictionary::new();
        second.set_new_name_for(b"FT", b"Tx");
        second.set_new_name_for(b"Subtype", b"Widget");
        second.set_new_for(b"T", CpdfString::from_unicode(None, &wide("b")));
        second.set_new_for(b"V", CpdfString::from_unicode(None, &wide("oldB")));
        let second = retain_object(second);
        let root = document.borrow().root().unwrap();
        let acro = typed(dict_get(&root, b"AcroForm"), ObjectType::Dictionary).unwrap();
        let fields = typed(dict_get(&acro, b"Fields"), ObjectType::Array).unwrap();
        fields
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append(second.clone());
        let mut order = CpdfArray::new();
        order.append(old_a.field_dict());
        order.append(second);
        dict_mut(&acro, |form| {
            form.set_new_for(b"CO", order);
        });
        let env = CpdfSdkFormFillEnvironment::new(&document, None);
        let form = env.interactive_form();
        let a = form.form.get_field(0, &wide("a")).unwrap();
        let b = form.form.get_field(0, &wide("b")).unwrap();
        script(&b, b"C", "event.value = this.getField('a').value + 'B';");
        assert_eq!(
            env.run_input_event(|| form.on_calculate(None)),
            Err(ActionError::JavaScriptException)
        );
        assert_eq!(a.value(), wide("oldA"));
        assert_eq!(b.value(), wide("oldAB"));
        script(&a, b"C", "event.value = 'A';");
        assert!(env.run_input_event(|| form.on_calculate(None)).is_ok());
        assert_eq!(a.value(), wide("A"));
        assert_eq!(b.value(), wide("AB"));
    }
    #[test]
    fn highlight_defaults_overrides_and_widget_map_are_nonowning() {
        let (document, form, field) = fixture("text", "original");
        assert!(!form.is_need_highlight(FormFieldType::TextField));
        assert_eq!(form.highlight_alpha(), 0);
        assert_eq!(form.highlight_color(FormFieldType::Unknown), WHITE_BGR);
        form.set_highlight_color(0x123456, FormFieldType::TextField);
        form.set_highlight_alpha(80);
        assert_eq!(form.highlight_alpha(), 80);
        assert!(form.is_need_highlight(FormFieldType::TextField));
        assert_eq!(form.highlight_color(FormFieldType::TextField), 0x123456);
        form.set_highlight_color(0x223344, FormFieldType::Unknown);
        assert!(!form.is_need_highlight(FormFieldType::Unknown));
        #[cfg(feature = "xfa")]
        {
            form.set_highlight_color(0x556677, FormFieldType::Xfa);
            assert!(form.is_need_highlight(FormFieldType::XfaTextField));
            assert_eq!(form.highlight_color(FormFieldType::XfaTextField), 0x556677);
            form.set_highlight_color(0x778899, FormFieldType::XfaTextField);
            assert_eq!(form.highlight_color(FormFieldType::XfaTextField), 0x778899);
        }
        form.remove_all_highlights();
        assert!(!form.is_need_highlight(FormFieldType::TextField));
        form.set_all_highlight_colors(0x111111);
        assert!(form.is_need_highlight(FormFieldType::CheckBox));
        assert!(!form.is_need_highlight(FormFieldType::Unknown));
        // Every supported field kind, including the highest index, addresses
        // its own highlight slot. Disabled XFA enum values are rejected by
        // the shared integer boundary before indexing these 8/16-entry tables.
        for value in 1..16 {
            let Some(kind) = crate::fpdfdoc::cpdf_formfield::int_to_form_field_type(value) else {
                continue;
            };
            form.remove_all_highlights();
            assert!(!form.is_need_highlight(kind));
            assert_eq!(form.highlight_color(kind), WHITE_BGR);
            let color = 0x102030 + value as u32;
            form.set_highlight_color(color, kind);
            assert!(form.is_need_highlight(kind));
            assert_eq!(form.highlight_color(kind), color);
            form.remove_all_highlights();
            assert!(!form.is_need_highlight(kind));
            assert_eq!(form.highlight_color(kind), WHITE_BGR);
        }
        let control = field.get_control(0).unwrap();
        let widget = Rc::new(CpdfSdkWidget::new(&document, control.clone()));
        form.add_map(&control, &widget);
        assert!(Rc::ptr_eq(&form.get_widget(&control).unwrap(), &widget));
        form.remove_map(&control);
        assert!(form.get_widget(&control).is_none());
        form.add_map(&control, &widget);
        drop(widget);
        assert!(form.get_widget(&control).is_none());
    }
}
