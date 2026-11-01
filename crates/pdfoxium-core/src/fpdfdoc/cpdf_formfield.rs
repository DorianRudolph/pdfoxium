// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Field inheritance, values and selection from `core/fpdfdoc/cpdf_formfield.{h,cpp}`.
//! Forms own fields and controls; weak backedges permit notifications to reenter.
use super::cpdf_aaction::CpdfAAction;
use super::cpdf_formcontrol::CpdfFormControl;
use super::cpdf_interactiveform::CpdfInteractiveForm;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_name::CpdfName;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fpdfapi::parser::fpdf_parser_decode::pdf_encode_text;
use std::cell::Cell;
use std::collections::{BTreeMap, HashSet};
use std::rc::{Rc, Weak};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationOption {
    DoNotNotify,
    Notify,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldType {
    Unknown,
    PushButton,
    RadioButton,
    CheckBox,
    Text,
    RichText,
    File,
    ListBox,
    ComboBox,
    Sign,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FormFieldType {
    Unknown = 0,
    PushButton = 1,
    CheckBox = 2,
    RadioButton = 3,
    ComboBox = 4,
    ListBox = 5,
    TextField = 6,
    Signature = 7,
    #[cfg(feature = "xfa")]
    Xfa = 8,
    #[cfg(feature = "xfa")]
    XfaCheckBox = 9,
    #[cfg(feature = "xfa")]
    XfaComboBox = 10,
    #[cfg(feature = "xfa")]
    XfaImageField = 11,
    #[cfg(feature = "xfa")]
    XfaListBox = 12,
    #[cfg(feature = "xfa")]
    XfaPushButton = 13,
    #[cfg(feature = "xfa")]
    XfaSignature = 14,
    #[cfg(feature = "xfa")]
    XfaTextField = 15,
}
pub fn int_to_form_field_type(value: i32) -> Option<FormFieldType> {
    Some(match value {
        0 => FormFieldType::Unknown,
        1 => FormFieldType::PushButton,
        2 => FormFieldType::CheckBox,
        3 => FormFieldType::RadioButton,
        4 => FormFieldType::ComboBox,
        5 => FormFieldType::ListBox,
        6 => FormFieldType::TextField,
        7 => FormFieldType::Signature,
        #[cfg(feature = "xfa")]
        8 => FormFieldType::Xfa,
        #[cfg(feature = "xfa")]
        9 => FormFieldType::XfaCheckBox,
        #[cfg(feature = "xfa")]
        10 => FormFieldType::XfaComboBox,
        #[cfg(feature = "xfa")]
        11 => FormFieldType::XfaImageField,
        #[cfg(feature = "xfa")]
        12 => FormFieldType::XfaListBox,
        #[cfg(feature = "xfa")]
        13 => FormFieldType::XfaPushButton,
        #[cfg(feature = "xfa")]
        14 => FormFieldType::XfaSignature,
        #[cfg(feature = "xfa")]
        15 => FormFieldType::XfaTextField,
        _ => return None,
    })
}
pub(crate) fn dict_get(dict: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    let raw = dict
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(key)?;
    get_direct_object(&raw)
}
pub(crate) fn dict_has(dict: &ObjectRef, key: &[u8]) -> bool {
    dict.borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .is_some_and(|d| d.key_exist(key))
}
pub(crate) fn typed(object: Option<ObjectRef>, kind: ObjectType) -> Option<ObjectRef> {
    if kind == ObjectType::Dictionary {
        object.and_then(|o| crate::fpdfapi::parser::cpdf_object::get_dict_object(&o))
    } else {
        object.filter(|o| o.borrow().object_type() == kind)
    }
}
pub(crate) fn dict_bytes(dict: &ObjectRef, key: &[u8]) -> Vec<u8> {
    dict_get(dict, key).map_or_else(Vec::new, |o| o.borrow().get_string())
}
pub(crate) fn dict_unicode(dict: &ObjectRef, key: &[u8]) -> Vec<u32> {
    dict_get(dict, key).map_or_else(Vec::new, |o| o.borrow().get_unicode_text())
}
pub(crate) fn dict_int(dict: &ObjectRef, key: &[u8], default: i32) -> i32 {
    dict_get(dict, key).map_or(default, |o| o.borrow().get_integer())
}
pub(crate) fn dict_mut<R>(dict: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
    let mut borrowed = dict.borrow_mut();
    f(borrowed
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .expect("field dictionary"))
}
pub(crate) fn array_len(array: &ObjectRef) -> usize {
    array
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .map_or(0, CpdfArray::size)
}
pub(crate) fn array_at(array: &ObjectRef, index: usize) -> Option<ObjectRef> {
    let raw = array
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()?
        .get_object_at(index)?;
    get_direct_object(&raw)
}
pub fn field_attr(dict: &ObjectRef, name: &[u8]) -> Option<ObjectRef> {
    let mut current = Rc::clone(dict);
    for _ in 0..=32 {
        if let Some(value) = dict_get(&current, name) {
            return Some(value);
        }
        current = typed(dict_get(&current, b"Parent"), ObjectType::Dictionary)?;
    }
    None
}
// Preserve the Phase 1 adapter; new callers use field_attr() to release all
// dictionary borrows before lazy indirect-object parsing callbacks.
pub fn get_field_attr_for_dict(dict: Option<&CpdfDictionary>, name: &[u8]) -> Option<ObjectRef> {
    let dict = dict?;
    if let Some(value) = dict.get_direct_object_for(name) {
        return Some(value);
    }
    let mut parent = dict.get_dict_for(b"Parent")?;
    for _ in 1..=32 {
        if let Some(value) = dict_get(&parent, name) {
            return Some(value);
        }
        parent = typed(dict_get(&parent, b"Parent"), ObjectType::Dictionary)?;
    }
    None
}
pub fn get_full_name_for_dict<'a>(dict: impl Into<Option<&'a ObjectRef>>) -> Vec<u32> {
    let mut current = dict.into().cloned();
    let mut visited = HashSet::new();
    let mut full = Vec::new();
    while let Some(dict) = current {
        if !visited.insert(Rc::as_ptr(&dict) as *const () as usize) {
            break;
        }
        let mut short = dict_unicode(&dict, b"T");
        if !short.is_empty() {
            if !full.is_empty() {
                short.push(u32::from(b'.'));
                short.extend(full);
            }
            full = short;
        }
        current = typed(dict_get(&dict, b"Parent"), ObjectType::Dictionary);
    }
    full
}

pub struct CpdfFormField {
    field_type: FieldType,
    required: bool,
    no_export: bool,
    multi_select: bool,
    unison: bool,
    use_selected_indices: Cell<bool>,
    form: Weak<CpdfInteractiveForm>,
    dict: ObjectRef,
}
impl CpdfFormField {
    pub(crate) fn new(form: Weak<CpdfInteractiveForm>, dict: ObjectRef) -> Rc<Self> {
        let flags = field_attr(&dict, b"Ff").map_or(0, |o| o.borrow().get_integer() as u32);
        let name = field_attr(&dict, b"FT").map_or_else(Vec::new, |o| o.borrow().get_string());
        let ty = match name.as_slice() {
            b"Btn" if flags & (1 << 15) != 0 => FieldType::RadioButton,
            b"Btn" if flags & (1 << 16) != 0 => FieldType::PushButton,
            b"Btn" => FieldType::CheckBox,
            b"Tx" if flags & (1 << 20) != 0 => FieldType::File,
            b"Tx" if flags & (1 << 25) != 0 => FieldType::RichText,
            b"Tx" => FieldType::Text,
            b"Ch" if flags & (1 << 17) != 0 => FieldType::ComboBox,
            b"Ch" => FieldType::ListBox,
            b"Sig" => FieldType::Sign,
            _ => FieldType::Unknown,
        };
        let result = Rc::new(Self {
            field_type: ty,
            required: flags & 2 != 0,
            no_export: flags & 4 != 0,
            multi_select: ty == FieldType::ListBox && flags & (1 << 21) != 0,
            unison: ty == FieldType::CheckBox
                || ty == FieldType::RadioButton && flags & (1 << 25) != 0,
            use_selected_indices: Cell::new(false),
            form,
            dict,
        });
        if result.is_choice() {
            result
                .use_selected_indices
                .set(result.use_selected_indices_object());
        }
        result
    }
    pub fn full_name(&self) -> Vec<u32> {
        get_full_name_for_dict(&self.dict)
    }
    pub fn get_type(&self) -> FieldType {
        self.field_type
    }
    pub fn field_type(&self) -> FormFieldType {
        match self.field_type {
            FieldType::PushButton => FormFieldType::PushButton,
            FieldType::CheckBox => FormFieldType::CheckBox,
            FieldType::RadioButton => FormFieldType::RadioButton,
            FieldType::ComboBox => FormFieldType::ComboBox,
            FieldType::ListBox => FormFieldType::ListBox,
            FieldType::Text | FieldType::RichText | FieldType::File => FormFieldType::TextField,
            FieldType::Sign => FormFieldType::Signature,
            _ => FormFieldType::Unknown,
        }
    }
    pub fn field_dict(&self) -> ObjectRef {
        Rc::clone(&self.dict)
    }
    pub fn get_field_attr(&self, name: &[u8]) -> Option<ObjectRef> {
        field_attr(&self.dict, name)
    }
    pub fn field_flags(&self) -> u32 {
        self.get_field_attr(b"Ff")
            .map_or(0, |o| o.borrow().get_integer() as u32)
    }
    pub fn set_field_flags(&self, flags: u32) {
        dict_mut(&self.dict, |d| {
            d.set_new_for(b"Ff", CpdfScalarObject::number_from_signed(flags as i32));
        });
    }
    pub fn is_required(&self) -> bool {
        self.required
    }
    pub fn is_no_export(&self) -> bool {
        self.no_export
    }
    pub fn additional_action(&self) -> CpdfAAction {
        CpdfAAction::new(typed(self.get_field_attr(b"AA"), ObjectType::Dictionary))
    }
    pub fn alternate_name(&self) -> Vec<u32> {
        self.attr_unicode(b"TU")
    }
    pub fn mapping_name(&self) -> Vec<u32> {
        self.attr_unicode(b"TM")
    }
    fn attr_unicode(&self, key: &[u8]) -> Vec<u32> {
        self.get_field_attr(key)
            .map_or_else(Vec::new, |o| o.borrow().get_unicode_text())
    }
    pub fn controls(&self) -> Vec<Rc<CpdfFormControl>> {
        self.form
            .upgrade()
            .map_or_else(Vec::new, |f| f.controls_for_field(self))
    }
    pub fn count_controls(&self) -> usize {
        self.controls().len()
    }
    pub fn get_control(&self, index: usize) -> Option<Rc<CpdfFormControl>> {
        self.controls().get(index).cloned()
    }
    pub fn control_index(&self, control: &CpdfFormControl) -> i32 {
        self.controls()
            .iter()
            .position(|c| std::ptr::eq(c.as_ref(), control))
            .map_or(-1, |i| i as i32)
    }
    fn is_choice(&self) -> bool {
        matches!(self.field_type, FieldType::ComboBox | FieldType::ListBox)
    }
    fn is_check(&self) -> bool {
        matches!(
            self.field_type,
            FieldType::CheckBox | FieldType::RadioButton
        )
    }
    fn value_object(&self) -> Option<ObjectRef> {
        self.get_field_attr(b"V")
    }
    fn default_object(&self) -> Option<ObjectRef> {
        self.get_field_attr(b"DV")
    }
    fn indices_object(&self) -> Option<ObjectRef> {
        self.get_field_attr(b"I")
    }
    fn value_or_indices(&self) -> Option<ObjectRef> {
        self.value_object().or_else(|| self.indices_object())
    }
    fn get_value(&self, default: bool) -> Vec<u32> {
        if self.is_check() {
            return self.check_value(default);
        }
        let value = if default {
            self.default_object()
        } else {
            self.value_object().or_else(|| {
                if self.field_type != FieldType::Text {
                    self.default_object()
                } else {
                    None
                }
            })
        };
        let Some(value) = value else {
            return Vec::new();
        };
        let kind = value.borrow().object_type();
        match kind {
            ObjectType::String | ObjectType::Stream => value.borrow().get_unicode_text(),
            ObjectType::Array => {
                array_at(&value, 0).map_or_else(Vec::new, |o| o.borrow().get_unicode_text())
            }
            _ => Vec::new(),
        }
    }
    pub fn value(&self) -> Vec<u32> {
        self.get_value(false)
    }
    pub fn default_value(&self) -> Vec<u32> {
        self.get_value(true)
    }
    fn before_value(&self, value: &[u32]) -> bool {
        self.form
            .upgrade()
            .is_none_or(|f| f.notify_before_value_change(self, value))
    }
    fn after_value(&self) {
        if let Some(f) = self.form.upgrade() {
            f.notify_after_value_change(self);
        }
    }
    fn before_selection(&self, value: &[u32]) -> bool {
        self.form
            .upgrade()
            .is_none_or(|f| f.notify_before_selection_change(self, value))
    }
    fn after_selection(&self) {
        if let Some(f) = self.form.upgrade() {
            f.notify_after_selection_change(self);
        }
    }
    fn after_checked(&self) {
        if let Some(f) = self.form.upgrade() {
            f.notify_after_checked_status_change(self);
        }
    }
    fn choice_before(&self, value: &[u32]) -> bool {
        match self.field_type {
            FieldType::ComboBox => self.before_value(value),
            FieldType::ListBox => self.before_selection(value),
            _ => true,
        }
    }
    fn choice_after(&self) {
        match self.field_type {
            FieldType::ComboBox => self.after_value(),
            FieldType::ListBox => self.after_selection(),
            _ => (),
        }
    }
    pub fn reset_field(&self) {
        match self.field_type {
            FieldType::CheckBox | FieldType::RadioButton => {
                for (i, c) in self.controls().into_iter().enumerate() {
                    self.check_control(i, c.is_default_checked(), NotificationOption::DoNotNotify);
                }
                self.after_checked();
            }
            FieldType::ComboBox | FieldType::ListBox => {
                self.clear_selection(NotificationOption::DoNotNotify);
                let index = self.default_selected_item();
                let value = if index >= 0 {
                    self.option_label(index)
                } else {
                    Vec::new()
                };
                if !self.choice_before(&value) {
                    return;
                }
                self.set_item_selection(index, NotificationOption::DoNotNotify);
                self.choice_after();
            }
            _ => {
                let dv = self
                    .default_object()
                    .map_or_else(Vec::new, |o| o.borrow().get_unicode_text());
                let value = self
                    .value_object()
                    .map_or_else(Vec::new, |o| o.borrow().get_unicode_text());
                let has_rv = self.get_field_attr(b"RV").is_some();
                if !has_rv && dv == value {
                    return;
                }
                if !self.before_value(&dv) {
                    return;
                }
                if let Some(default) = self.default_object() {
                    let Some(clone) = crate::fpdfapi::parser::cpdf_object::clone_non_cyclic(
                        &default,
                        false,
                        &HashSet::new(),
                    ) else {
                        return;
                    };
                    dict_mut(&self.dict, |d| d.set_for(b"V", Some(clone)));
                    if has_rv {
                        let clone = crate::fpdfapi::parser::cpdf_object::clone_non_cyclic(
                            &default,
                            false,
                            &HashSet::new(),
                        );
                        dict_mut(&self.dict, |d| d.set_for(b"RV", clone));
                    }
                } else {
                    dict_mut(&self.dict, |d| {
                        d.remove_for(b"V");
                        d.remove_for(b"RV");
                    });
                }
                self.after_value();
            }
        }
    }
    pub fn set_value(&self, value: &[u32], notify: NotificationOption) -> bool {
        self.set_value_with_default(value, false, notify)
    }
    pub fn set_default_value(&self, value: &[u32], notify: NotificationOption) -> bool {
        self.set_value_with_default(value, true, notify)
    }
    fn set_value_with_default(
        &self,
        value: &[u32],
        default: bool,
        notify: NotificationOption,
    ) -> bool {
        if self.is_check() {
            return self.set_check_value(value, default, notify);
        }
        match self.field_type {
            FieldType::File | FieldType::RichText | FieldType::Text | FieldType::ComboBox => {
                if notify == NotificationOption::Notify && !self.before_value(value) {
                    return false;
                }
                let key = if default {
                    b"DV".as_slice()
                } else {
                    b"V".as_slice()
                };
                dict_mut(&self.dict, |d| {
                    d.set_new_for(key, CpdfString::from_unicode(None, value));
                });
                let index = if self.field_type == FieldType::ComboBox {
                    self.find_option(value)
                } else {
                    -1
                };
                if index < 0 {
                    if self.field_type == FieldType::RichText && !default {
                        dict_mut(&self.dict, |d| {
                            d.set_new_for(b"RV", CpdfString::from_unicode(None, value));
                        });
                    }
                    dict_mut(&self.dict, |d| {
                        d.remove_for(b"I");
                    });
                } else if !default {
                    self.clear_selection(NotificationOption::DoNotNotify);
                    self.set_item_selection(index, NotificationOption::DoNotNotify);
                }
                if notify == NotificationOption::Notify {
                    self.after_value();
                }
            }
            FieldType::ListBox => {
                let index = self.find_option(value);
                if index < 0 || default && index == self.default_selected_item() {
                    return false;
                }
                if notify == NotificationOption::Notify && !self.before_selection(value) {
                    return false;
                }
                if !default {
                    self.clear_selection(NotificationOption::DoNotNotify);
                    self.set_item_selection(index, NotificationOption::DoNotNotify);
                }
                if notify == NotificationOption::Notify {
                    self.after_selection();
                }
            }
            _ => (),
        }
        true
    }
    pub fn max_len(&self) -> i32 {
        if let Some(v) = self.get_field_attr(b"MaxLen") {
            return v.borrow().get_integer();
        }
        for c in self.controls() {
            let d = c.widget_dict();
            if dict_has(&d, b"MaxLen") {
                return dict_int(&d, b"MaxLen", 0);
            }
        }
        0
    }
    pub fn count_selected_items(&self) -> usize {
        let Some(v) = self.value_or_indices() else {
            return 0;
        };
        let b = v.borrow();
        match b.object_type() {
            ObjectType::String | ObjectType::Number => usize::from(!b.get_string().is_empty()),
            ObjectType::Array => b.as_any().downcast_ref::<CpdfArray>().unwrap().size(),
            _ => 0,
        }
    }
    pub fn selected_index(&self, index: i32) -> i32 {
        let Some(value) = self.value_or_indices() else {
            return -1;
        };
        let kind = value.borrow().object_type();
        if kind == ObjectType::Number {
            return value.borrow().get_integer();
        }
        let selected = if kind == ObjectType::String {
            if index != 0 {
                return -1;
            }
            value.borrow().get_unicode_text()
        } else {
            if kind != ObjectType::Array || index < 0 {
                return -1;
            }
            array_at(&value, index as usize)
                .map_or_else(Vec::new, |o| o.borrow().get_unicode_text())
        };
        if index >= 0 && (index as usize) < self.count_selected_options() {
            let i = self.selected_option_index(index);
            if self.option_value(i) == selected {
                return i;
            }
        }
        self.find_option(&selected)
    }
    pub fn clear_selection(&self, notify: NotificationOption) -> bool {
        if notify == NotificationOption::Notify {
            let i = self.selected_index(0);
            let value = if i >= 0 {
                self.option_label(i)
            } else {
                Vec::new()
            };
            if !self.choice_before(&value) {
                return false;
            }
        }
        dict_mut(&self.dict, |d| {
            d.remove_for(b"V");
            d.remove_for(b"I");
        });
        if notify == NotificationOption::Notify {
            self.choice_after();
        }
        true
    }
    pub fn is_item_selected(&self, index: i32) -> bool {
        if !self.is_choice() || index < 0 || index as usize >= self.count_options() {
            return false;
        }
        if self.use_selected_indices.get() {
            self.is_selected_index(index)
        } else {
            self.is_selected_option(&self.option_value(index))
        }
    }
    pub fn set_item_selection(&self, index: i32, notify: NotificationOption) {
        if !self.is_choice() || index < 0 || index as usize >= self.count_options() {
            return;
        }
        let value = self.option_value(index);
        if notify == NotificationOption::Notify && !self.choice_before(&value) {
            return;
        }
        if self.field_type != FieldType::ListBox {
            let mut indices = CpdfArray::new();
            indices.append_new(CpdfScalarObject::number_from_signed(index));
            dict_mut(&self.dict, |d| {
                d.set_new_for(b"V", CpdfString::from_unicode(None, &value));
                d.set_new_for(b"I", indices);
            });
        } else {
            // Snapshot previous selected values before replacing /V. This
            // preserves existing selections even if stale /I was unusable.
            let values = if self.multi_select {
                (0..self.count_options())
                    .filter(|&i| i == index as usize || self.is_item_selected(i as i32))
                    .map(|i| self.option_value(i as i32))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            self.select_option(index);
            if self.multi_select {
                let mut values_array = CpdfArray::new();
                for value in values {
                    values_array.append_new(CpdfString::from_unicode(None, &value));
                }
                dict_mut(&self.dict, |d| {
                    d.set_new_for(b"V", values_array);
                });
            } else {
                dict_mut(&self.dict, |d| {
                    d.set_new_for(b"V", CpdfString::from_unicode(None, &value));
                });
            }
        }
        if !self.use_selected_indices.get() {
            self.use_selected_indices
                .set(self.use_selected_indices_object());
        }
        if notify == NotificationOption::Notify {
            self.choice_after();
        }
    }
    pub fn default_selected_item(&self) -> i32 {
        let value = self
            .default_object()
            .map_or_else(Vec::new, |o| o.borrow().get_unicode_text());
        if value.is_empty() {
            -1
        } else {
            self.find_option(&value)
        }
    }
    pub fn has_opt_field(&self) -> bool {
        self.is_check() || self.is_choice()
    }
    pub fn count_options(&self) -> usize {
        typed(self.get_field_attr(b"Opt"), ObjectType::Array).map_or(0, |o| array_len(&o))
    }
    fn option_text(&self, index: i32, sub_index: usize) -> Vec<u32> {
        if index < 0 {
            return Vec::new();
        }
        let Some(options) = typed(self.get_field_attr(b"Opt"), ObjectType::Array) else {
            return Vec::new();
        };
        let Some(mut option) = array_at(&options, index as usize) else {
            return Vec::new();
        };
        if option.borrow().object_type() == ObjectType::Array {
            let Some(sub) = array_at(&option, sub_index) else {
                return Vec::new();
            };
            option = sub;
        }
        if option.borrow().object_type() != ObjectType::String {
            return Vec::new();
        }
        let result = option.borrow().get_unicode_text();
        result
    }
    pub fn option_value(&self, index: i32) -> Vec<u32> {
        self.option_text(index, 0)
    }
    pub fn option_label(&self, index: i32) -> Vec<u32> {
        self.option_text(index, 1)
    }
    pub fn find_option(&self, value: &[u32]) -> i32 {
        (0..self.count_options())
            .find(|&i| self.option_value(i as i32) == value)
            .map_or(-1, |i| i as i32)
    }
    pub fn check_control(&self, index: usize, checked: bool, notify: NotificationOption) {
        if !self.is_check() {
            return;
        }
        let controls = self.controls();
        let Some(control) = controls.get(index) else {
            return;
        };
        if !checked && !control.is_checked() {
            return;
        }
        let export = control.export_value();
        for (i, c) in controls.iter().enumerate() {
            if self.unison {
                if c.export_value() == export && c.on_state_name() == control.on_state_name() {
                    c.check_control(checked);
                } else if checked {
                    c.check_control(false);
                }
            } else if i == index {
                c.check_control(checked);
            } else if checked {
                c.check_control(false);
            }
        }
        if typed(self.get_field_attr(b"Opt"), ObjectType::Array).is_none() {
            let encoded = pdf_encode_text(&export);
            if checked {
                dict_mut(&self.dict, |d| {
                    d.set_new_for(b"V", CpdfName::new(None, &encoded));
                });
            } else if self
                .value_object()
                .is_some_and(|o| o.borrow().get_string() == encoded)
            {
                dict_mut(&self.dict, |d| {
                    d.set_new_name_for(b"V", b"Off");
                });
            }
        } else if checked {
            dict_mut(&self.dict, |d| {
                d.set_new_name_for(b"V", index.to_string().as_bytes());
            });
        }
        if notify == NotificationOption::Notify {
            self.after_checked();
        }
    }
    pub fn check_value(&self, default: bool) -> Vec<u32> {
        for c in self.controls() {
            if if default {
                c.is_default_checked()
            } else {
                c.is_checked()
            } {
                return c.export_value();
            }
        }
        vec![79, 102, 102]
    }
    fn set_check_value(&self, value: &[u32], default: bool, notify: NotificationOption) -> bool {
        for (i, c) in self.controls().iter().enumerate() {
            let matches = c.export_value() == value;
            if !default {
                self.check_control(i, matches, NotificationOption::DoNotNotify);
            }
            if matches {
                break;
            }
        }
        if notify == NotificationOption::Notify {
            self.after_checked();
        }
        true
    }
    pub fn top_visible_index(&self) -> i32 {
        self.get_field_attr(b"TI")
            .map_or(0, |o| o.borrow().get_integer())
    }
    pub fn count_selected_options(&self) -> usize {
        typed(self.indices_object(), ObjectType::Array).map_or(0, |o| array_len(&o))
    }
    pub fn selected_option_index(&self, index: i32) -> i32 {
        if index < 0 {
            return 0;
        }
        let Some(array) = typed(self.indices_object(), ObjectType::Array) else {
            return -1;
        };
        array_at(&array, index as usize).map_or(-1, |o| o.borrow().get_integer())
    }
    pub fn is_selected_option(&self, value: &[u32]) -> bool {
        let Some(object) = self.value_object() else {
            return false;
        };
        if object.borrow().object_type() == ObjectType::Array {
            return (0..array_len(&object)).any(|i| {
                array_at(&object, i).is_some_and(|o| {
                    o.borrow().object_type() == ObjectType::String
                        && o.borrow().get_unicode_text() == value
                })
            });
        }
        let b = object.borrow();
        b.object_type() == ObjectType::String && b.get_unicode_text() == value
    }
    pub fn is_selected_index(&self, index: i32) -> bool {
        let Some(object) = self.indices_object() else {
            return false;
        };
        if object.borrow().object_type() == ObjectType::Array {
            return (0..array_len(&object)).any(|i| {
                array_at(&object, i).is_some_and(|o| {
                    o.borrow().object_type() == ObjectType::Number
                        && o.borrow().get_integer() == index
                })
            });
        }
        let b = object.borrow();
        b.object_type() == ObjectType::Number && b.get_integer() == index
    }
    pub fn select_option(&self, index: i32) {
        let array = typed(dict_get(&self.dict, b"I"), ObjectType::Array)
            .unwrap_or_else(|| dict_mut(&self.dict, |d| d.set_new_for(b"I", CpdfArray::new())));
        let mut position = array_len(&array);
        for i in 0..position {
            let found = array_at(&array, i).map_or(0, |o| o.borrow().get_integer());
            if found == index {
                return;
            }
            if found > index {
                position = i;
                break;
            }
        }
        array
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .insert_new_at(position, CpdfScalarObject::number_from_signed(index));
    }
    pub fn use_selected_indices_object(&self) -> bool {
        let Some(indices) = self.indices_object() else {
            return false;
        };
        let Some(value) = self.value_object() else {
            return true;
        };
        let kind = indices.borrow().object_type();
        let count = match kind {
            ObjectType::Array => array_len(&indices),
            ObjectType::Number => 1,
            _ => return false,
        };
        let mut values = BTreeMap::<Vec<u32>, usize>::new();
        let value_kind = value.borrow().object_type();
        if value_kind == ObjectType::Array {
            if array_len(&value) != count {
                return false;
            }
            for i in 0..count {
                if let Some(v) = array_at(&value, i) {
                    let b = v.borrow();
                    if b.object_type() == ObjectType::String {
                        *values.entry(b.get_unicode_text()).or_default() += 1;
                    }
                }
            }
        } else if value_kind == ObjectType::String {
            if count != 1 {
                return false;
            }
            *values.entry(value.borrow().get_unicode_text()).or_default() += 1;
        }
        if kind == ObjectType::Array {
            for i in 0..count {
                let Some(v) = array_at(&indices, i) else {
                    return false;
                };
                let b = v.borrow();
                if b.object_type() != ObjectType::Number {
                    return false;
                }
                let index = b.get_integer();
                drop(b);
                if index < 0 || index as usize >= self.count_options() {
                    return false;
                }
                let opt = self.option_value(index);
                let Some(count) = values.get_mut(&opt) else {
                    return false;
                };
                *count -= 1;
                if *count == 0 {
                    values.remove(&opt);
                }
            }
            values.is_empty()
        } else {
            let index = indices.borrow().get_integer();
            index >= 0
                && (index as usize) < self.count_options()
                && values.contains_key(&self.option_value(index))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    fn wide(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }
    #[test]
    fn form_field_type_conversion_tracks_the_compiled_native_range() {
        // CPDF_FormField::IntToFormFieldType accepts [0, kFormFieldTypeCount).
        // AcroForm field IDs stay ABI-stable regardless of optional XFA.
        for value in 0..8 {
            assert_eq!(
                int_to_form_field_type(value).map(|kind| kind as i32),
                Some(value)
            );
        }
        for value in 8..16 {
            #[cfg(feature = "xfa")]
            assert_eq!(
                int_to_form_field_type(value).map(|kind| kind as i32),
                Some(value)
            );
            #[cfg(not(feature = "xfa"))]
            assert_eq!(int_to_form_field_type(value), None);
        }
        for value in [i32::MIN, -2, -1, 16, 17, 255, i32::MAX] {
            assert_eq!(int_to_form_field_type(value), None);
        }
        assert_eq!(FormFieldType::Unknown as u8, 0);
        assert_eq!(FormFieldType::Signature as u8, 7);
        #[cfg(feature = "xfa")]
        assert_eq!(FormFieldType::XfaTextField as u8, 15);
    }

    #[test]
    fn get_full_name_for_dict_retains_source_cycles_and_empty_names() {
        // CPDFFormFieldTest.GetFullNameForDict, including every start node.
        assert!(get_full_name_for_dict(None).is_empty());
        let holder =
            crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder::new();
        let root = holder.borrow_mut().new_indirect_dictionary();
        dict_mut(&root, |d| {
            d.set_new_name_for(b"T", b"foo");
        });
        assert_eq!(get_full_name_for_dict(&root), wide("foo"));
        let d1 = holder.borrow_mut().new_indirect_dictionary();
        let n = d1.borrow().obj_num();
        dict_mut(&root, |d| {
            d.set_new_for(b"Parent", CpdfReference::new(&holder, n));
        });
        dict_mut(&d1, |d| {
            d.set_new_name_for(b"T", b"bar");
        });
        assert_eq!(get_full_name_for_dict(&root), wide("bar.foo"));
        let d2 = dict_mut(&d1, |d| d.set_new_for(b"Parent", CpdfDictionary::new()));
        assert_eq!(get_full_name_for_dict(&root), wide("bar.foo"));
        let d3 = holder.borrow_mut().new_indirect_dictionary();
        let n = d3.borrow().obj_num();
        dict_mut(&d2, |d| {
            d.set_new_for(b"Parent", CpdfReference::new(&holder, n));
        });
        dict_mut(&d3, |d| {
            d.set_new_name_for(b"T", b"qux");
        });
        assert_eq!(get_full_name_for_dict(&root), wide("qux.bar.foo"));
        let n = root.borrow().obj_num();
        dict_mut(&d3, |d| {
            d.set_new_for(b"Parent", CpdfReference::new(&holder, n));
        });
        for (dict, name) in [
            (&root, "qux.bar.foo"),
            (&d1, "foo.qux.bar"),
            (&d2, "bar.foo.qux"),
            (&d3, "bar.foo.qux"),
        ] {
            assert_eq!(get_full_name_for_dict(dict), wide(name));
        }
    }
    #[derive(Clone, Copy)]
    enum Values {
        None,
        Text(&'static str),
        Texts(&'static [&'static str]),
        Number(i32),
        Numbers(&'static [i32]),
    }
    fn value(v: Values) -> Option<ObjectRef> {
        Some(match v {
            Values::None => return None,
            Values::Text(s) => retain_object(CpdfString::from_unicode(None, &wide(s))),
            Values::Number(i) => retain_object(CpdfScalarObject::number_from_signed(i)),
            Values::Texts(strings) => {
                let mut a = CpdfArray::new();
                for s in strings {
                    a.append_new(CpdfString::from_unicode(None, &wide(s)));
                }
                retain_object(a)
            }
            Values::Numbers(numbers) => {
                let mut a = CpdfArray::new();
                for &i in numbers {
                    a.append_new(CpdfScalarObject::number_from_signed(i));
                }
                retain_object(a)
            }
        })
    }
    #[test]
    fn is_item_selected_preserves_all_source_value_index_conflicts() {
        // Complete CPDFFormFieldTest.IsItemSelected inputs and assertions.
        use Values::{None as Missing, Number, Numbers, Text, Texts};
        let cases: &[(Values, Values, bool, &[i32], &[i32])] = &[
            (Missing, Missing, false, &[], &[-1, 5]),
            (Text("Gamma"), Missing, false, &[2], &[-1, 5]),
            (Text("Omega"), Missing, false, &[], &[-1, 5]),
            (Texts(&["Beta"]), Missing, false, &[1], &[-1, 5]),
            (Texts(&["Omega"]), Missing, false, &[], &[-1, 5]),
            (
                Texts(&["Beta", "Epsilon"]),
                Missing,
                false,
                &[1, 4],
                &[-1, 5],
            ),
            (
                Texts(&["Beta", "Epsilon", "Omega"]),
                Missing,
                false,
                &[1, 4],
                &[-1, 5],
            ),
            (Missing, Number(3), true, &[3], &[-1, 5]),
            (Missing, Number(26), true, &[], &[-1, 5, 26]),
            (Missing, Numbers(&[0]), true, &[0], &[-1, 5]),
            (Missing, Numbers(&[0, 2, 3]), true, &[0, 2, 3], &[-1, 5]),
            (
                Missing,
                Numbers(&[0, 2, 3, -5, 12, 42]),
                true,
                &[0, 2, 3],
                &[-5, -1, 5, 12, 42],
            ),
            (
                Texts(&["Beta", "Epsilon"]),
                Numbers(&[0, 2, 3]),
                false,
                &[1, 4],
                &[-1, 5],
            ),
            (
                Texts(&["Alpha", "Epsilon"]),
                Numbers(&[2, 3]),
                false,
                &[0, 4],
                &[-1, 5],
            ),
            (
                Texts(&["Beta", "Epsilon", "Omega"]),
                Numbers(&[1, 4]),
                false,
                &[1, 4],
                &[-1, 5],
            ),
            (
                Texts(&["Beta", "Epsilon"]),
                Numbers(&[1, 4, 26]),
                false,
                &[1, 4],
                &[-1, 5, 26],
            ),
            (
                Texts(&["Beta", "Epsilon", "Omega"]),
                Numbers(&[0, 2, 3, 26]),
                false,
                &[1, 4],
                &[-1, 5, 26],
            ),
            (Text("Gamma"), Number(4), false, &[2], &[-1, 5]),
        ];
        for (case, &(v, i, use_i, expected, excluded)) in cases.iter().enumerate() {
            let mut d = CpdfDictionary::new();
            d.set_new_name_for(b"Type", b"Annot");
            d.set_new_name_for(b"Subtype", b"Widget");
            d.set_new_name_for(b"FT", b"Ch");
            d.set_new_for(b"Ff", CpdfScalarObject::number_from_signed(1 << 21));
            let mut options = CpdfArray::new();
            for s in ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"] {
                options.append_new(CpdfString::from_unicode(None, &wide(s)));
            }
            d.set_new_for(b"Opt", options);
            d.set_for(b"V", value(v));
            d.set_for(b"I", value(i));
            let field = CpdfFormField::new(Weak::new(), retain_object(d));
            assert_eq!(field.use_selected_indices_object(), use_i, "case {case}");
            for i in 0..field.count_options() {
                assert_eq!(
                    field.is_item_selected(i as i32),
                    expected.contains(&(i as i32)),
                    "case {case}, option {i}"
                );
            }
            for &i in excluded {
                assert!(!field.is_item_selected(i), "case {case}, excluded {i}");
            }
        }
    }
}
