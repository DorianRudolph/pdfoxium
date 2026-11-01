// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Ordinary `fxjs/cjs_field.{h,cpp}` bindings, including values, choices,
//! appearance properties, buttons, notifications and per-field delay queues.
//! Foreground XFA synchronization follows the SDK field notification path.
use super::cfxjs_engine::CfxJsEngine;
use super::cjs_delaydata::{CjsDelayData, CjsDelayValue};
use super::cjs_event_context::{wide_from_utf16, FieldReference};
use super::js_resources::{js_format_error_string, js_get_string_from_id, JsMessage};
use crate::fpdfdoc::cpdf_formcontrol::CpdfFormControl;
use crate::fpdfdoc::cpdf_formcontrol::HighlightingMode;
use crate::fpdfdoc::cpdf_formfield::{CpdfFormField, FormFieldType, NotificationOption};
use crate::fpdfdoc::cpdf_iconfit::ScaleMethod;
use crate::fpdfsdk::cpdfsdk_baannot::BorderStyle;
use crate::fpdfsdk::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment;
use crate::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use crate::fxcrt::fx_coordinates::CfxFloatRect;
use crate::fxcrt::fx_string::{utf16_encode, utf8_encode};
use crate::fxge::cfx_color::{CfxColor, ColorType};
use boa_engine::{
    js_string,
    native_function::NativeFunctionPointer,
    object::{builtins::JsArray, FunctionObjectBuilder, JsData, JsObject, ObjectInitializer},
    property::PropertyDescriptor,
    Context, JsError, JsResult, JsString, JsValue, NativeFunction,
};
use boa_gc::{Finalize, Trace};
use std::cell::Cell;
use std::rc::{Rc, Weak};

#[derive(Clone, Trace, Finalize)]
struct FieldData {
    // Neither weak SDK observers nor native UTF-16 text contain Boa GC edges.
    #[unsafe_ignore_trace]
    environment: Weak<CpdfSdkFormFillEnvironment>,
    #[unsafe_ignore_trace]
    name: Rc<[u16]>,
    control_index: i32,
    can_set: bool,
    #[unsafe_ignore_trace]
    delay: Rc<Cell<bool>>,
}
impl JsData for FieldData {}

pub(super) fn binding_error(class: &str, property: &str, message: JsMessage) -> JsError {
    let text = js_format_error_string(class, Some(property), &js_get_string_from_id(message));
    // PDFium throws the formatted UTF-16 string itself, rather than an Error
    // object. Preserve the catch value's type and exact code units.
    JsError::from_opaque(JsString::from(text.as_slice()).into())
}
fn error(property: &str, message: JsMessage) -> JsError {
    binding_error("Field", property, message)
}
fn data(this: &JsValue, property: &str) -> JsResult<FieldData> {
    // Drop the native-data borrow before executing any JavaScript coercions.
    this.as_object()
        .and_then(|object| object.downcast_ref::<FieldData>().map(|data| data.clone()))
        .ok_or_else(|| error(property, JsMessage::BadObjectError))
}
fn field_callback(callback: NativeFunctionPointer, property: bool) -> NativeFunction {
    // js_define resolves native property callbacks through info.Holder(), but
    // methods through info.This(). Only a fn pointer and bool are captured;
    // Boa's safe Copy closure constructor contains no hidden GC references.
    NativeFunction::from_copy_closure(move |this, arguments, context| {
        let mut object = this.as_object();
        while let Some(holder) = object {
            if holder.downcast_ref::<FieldData>().is_some() {
                return callback(&holder.into(), arguments, context);
            }
            if !property {
                break;
            }
            object = holder.prototype();
        }
        Ok(JsValue::undefined())
    })
}
impl FieldData {
    fn environment(&self, property: &str) -> JsResult<Rc<CpdfSdkFormFillEnvironment>> {
        self.environment
            .upgrade()
            .filter(|environment| environment.is_alive())
            .ok_or_else(|| error(property, JsMessage::BadObjectError))
    }
    fn first_field(&self, property: &str) -> JsResult<Rc<CpdfFormField>> {
        self.environment(property)?
            .interactive_form()
            .core_form()
            .get_field(0, &wide_from_utf16(&self.name))
            .ok_or_else(|| error(property, JsMessage::BadObjectError))
    }
    fn fields(&self, property: &str) -> JsResult<Vec<Rc<CpdfFormField>>> {
        let form = self.environment(property)?.interactive_form().core_form();
        let name = wide_from_utf16(&self.name);
        Ok((0..form.count_fields(&name))
            .filter_map(|index| form.get_field(index, &name))
            .collect())
    }
    fn check_can_set(&self, property: &str) -> JsResult<()> {
        if self.can_set {
            Ok(())
        } else {
            Err(error(property, JsMessage::ReadOnlyError))
        }
    }
    fn smart_control(
        &self,
        field: &CpdfFormField,
        property: &str,
    ) -> JsResult<Rc<CpdfFormControl>> {
        let index = self.control_index.max(0) as usize;
        field
            .get_control(index)
            .ok_or_else(|| error(property, JsMessage::BadObjectError))
    }
    fn widget(&self, property: &str, context: &mut Context) -> JsResult<Rc<CpdfSdkWidget>> {
        let field = self.first_field(property)?;
        let control = self.smart_control(&field, property)?;
        let environment = self.environment(property)?;
        // A missing map entry can query the embedder's page service. That
        // service is allowed to synchronously reenter or detach the document.
        CfxJsEngine::with_host_callback(context, || {
            environment.interactive_form().get_widget(&control)
        })
        .filter(|widget| environment.is_alive() && widget.is_alive())
        .ok_or_else(|| error(property, JsMessage::BadObjectError))
    }
}

/// The source accepts a numeric prefix for nonzero control indices. Zero must
/// be exactly `0`, with optional trailing spaces. Overflow is rejected safely.
fn parse_field_name(name: &[u16]) -> Option<(&[u16], i32)> {
    let dot = name.iter().rposition(|&unit| unit == u16::from(b'.'))?;
    let suffix = &name[dot + 1..];
    let mut position = 0;
    while suffix
        .get(position)
        .is_some_and(|unit| matches!(*unit, 9..=13 | 32))
    {
        position += 1;
    }
    let negative = suffix.get(position) == Some(&u16::from(b'-'));
    if negative || suffix.get(position) == Some(&u16::from(b'+')) {
        position += 1;
    }
    let mut value = 0i64;
    while let Some(&unit @ 48..=57) = suffix.get(position) {
        value = value.checked_mul(10)?.checked_add(i64::from(unit - 48))?;
        position += 1;
    }
    let index = i32::try_from(if negative { -value } else { value }).ok()?;
    if index == 0 {
        let end = suffix
            .iter()
            .rposition(|&unit| unit != 32)
            .map_or(0, |i| i + 1);
        if suffix[..end] != [48] {
            return None;
        }
    }
    Some((&name[..dot], index))
}
fn normalize_name(name: &[u16]) -> Vec<u16> {
    let mut result = Vec::with_capacity(name.len());
    let mut position = 0;
    while position < name.len() {
        result.push(name[position]);
        position += if name.get(position..position + 2) == Some(&[46, 46]) {
            2
        } else {
            1
        };
    }
    result
}

pub fn field_object(context: &mut Context, reference: FieldReference) -> JsResult<JsObject> {
    let environment = reference
        .environment
        .upgrade()
        .filter(|environment| environment.is_alive())
        .ok_or_else(|| error("", JsMessage::BadObjectError))?;
    let name = normalize_name(&reference.name);
    let (name, control_index) = if environment
        .interactive_form()
        .core_form()
        .count_fields(&wide_from_utf16(&name))
        == 0
    {
        let (name, index) =
            parse_field_name(&name).ok_or_else(|| error("", JsMessage::BadObjectError))?;
        (Rc::from(name), index)
    } else {
        (Rc::from(name), -1)
    };
    let data = FieldData {
        environment: reference.environment,
        name,
        control_index,
        can_set: environment.has_permissions(0x100 | 0x20 | 0x8),
        delay: Rc::new(Cell::new(false)),
    };
    let object = ObjectInitializer::with_native_data(data, context).build();
    let properties: &[(&str, NativeFunctionPointer, NativeFunctionPointer)] = &[
        ("name", get_name, set_name),
        ("value", get_value, set_value),
        ("valueAsString", get_value_as_string, set_value_as_string),
        (
            "currentValueIndices",
            get_current_value_indices,
            set_current_value_indices,
        ),
        ("numItems", get_num_items, set_num_items),
        (
            "multipleSelection",
            get_multiple_selection,
            set_multiple_selection,
        ),
        ("readonly", get_readonly, set_readonly),
        ("required", get_required, set_required),
        ("multiline", get_multiline, set_multiline),
        ("password", get_password, set_password),
        ("doNotScroll", get_do_not_scroll, set_do_not_scroll),
        (
            "doNotSpellCheck",
            get_do_not_spell_check,
            set_do_not_spell_check,
        ),
        ("comb", get_comb, set_comb),
        (
            "commitOnSelChange",
            get_commit_on_sel_change,
            set_commit_on_sel_change,
        ),
        ("charLimit", get_char_limit, set_char_limit),
        ("defaultValue", get_default_value, set_default_value),
        ("type", get_type, set_type),
        ("exportValues", get_export_values, set_export_values),
        ("textColor", get_text_color, set_text_color),
        ("alignment", get_alignment, set_alignment),
        ("borderStyle", get_border_style, set_border_style),
        ("buttonAlignX", get_button_align_x, set_button_align_x),
        ("buttonAlignY", get_button_align_y, set_button_align_y),
        (
            "buttonFitBounds",
            get_button_fit_bounds,
            set_button_fit_bounds,
        ),
        ("buttonPosition", get_button_position, set_button_position),
        ("buttonScaleHow", get_button_scale_how, set_button_scale_how),
        (
            "buttonScaleWhen",
            get_button_scale_when,
            set_button_scale_when,
        ),
        ("calcOrderIndex", get_calc_order_index, set_calc_order_index),
        ("defaultStyle", get_default_style, set_default_style),
        ("delay", get_delay, set_delay),
        ("display", get_display, set_display),
        ("doc", get_doc, set_doc),
        ("editable", get_editable, set_editable),
        ("fileSelect", get_file_select, set_file_select),
        ("fillColor", get_fill_color, set_fill_color),
        ("hidden", get_hidden, set_hidden),
        ("highlight", get_highlight, set_highlight),
        ("lineWidth", get_line_width, set_line_width),
        ("page", get_page, set_page),
        ("print", get_print, set_print),
        ("radiosInUnison", get_radios_in_unison, set_radios_in_unison),
        ("rect", get_rect, set_rect),
        ("richText", get_rich_text, set_rich_text),
        ("richValue", get_rich_value, set_rich_value),
        ("rotation", get_rotation, set_rotation),
        ("source", get_source, set_source),
        ("strokeColor", get_stroke_color, set_stroke_color),
        ("style", get_style, set_style),
        ("submitName", get_submit_name, set_submit_name),
        ("textFont", get_text_font, set_text_font),
        ("textSize", get_text_size, set_text_size),
        ("userName", get_user_name, set_user_name),
    ];
    for &(name, getter, setter) in properties {
        let getter = FunctionObjectBuilder::new(context.realm(), field_callback(getter, true))
            .name(JsString::from(format!("get {name}")))
            .length(0)
            .constructor(false)
            .build();
        let setter = FunctionObjectBuilder::new(context.realm(), field_callback(setter, true))
            .name(JsString::from(format!("set {name}")))
            .length(1)
            .constructor(false)
            .build();
        object.define_property_or_throw(
            JsString::from(name),
            PropertyDescriptor::builder()
                .get(getter)
                .set(setter)
                .enumerable(true)
                .configurable(true),
            context,
        )?;
    }
    let methods: &[(&str, NativeFunctionPointer)] = &[
        ("getItemAt", get_item_at),
        ("insertItemAt", insert_item_at),
        ("deleteItemAt", delete_item_at),
        ("setItems", set_items),
        ("clearItems", clear_items),
        ("getArray", get_array),
        ("checkThisBox", check_this_box),
        ("defaultIsChecked", default_is_checked),
        ("isBoxChecked", is_box_checked),
        ("isDefaultChecked", is_default_checked),
        ("buttonGetCaption", button_get_caption),
        ("buttonSetCaption", button_set_caption),
        ("buttonImportIcon", button_import_icon),
        ("buttonSetIcon", button_set_icon),
        ("buttonGetIcon", button_get_icon),
        ("browseForFileToSubmit", browse_for_file_to_submit),
        ("getLock", get_lock),
        ("setAction", set_action),
        ("setFocus", set_focus),
        ("setLock", set_lock),
        ("signatureGetModifications", signature_get_modifications),
        ("signatureGetSeedValue", signature_get_seed_value),
        ("signatureInfo", signature_info),
        ("signatureSetSeedValue", signature_set_seed_value),
        ("signatureSign", signature_sign),
        ("signatureValidate", signature_validate),
    ];
    for &(name, callback) in methods {
        let function = FunctionObjectBuilder::new(context.realm(), field_callback(callback, false))
            .name(JsString::from(name))
            .length(0)
            .constructor(false)
            .build();
        object.define_property_or_throw(
            JsString::from(name),
            PropertyDescriptor::builder()
                .value(function)
                .writable(false)
                .enumerable(true)
                .configurable(true),
            context,
        )?;
    }
    Ok(object)
}

fn string(value: &[u32]) -> JsValue {
    JsString::from(utf16_encode(value).as_slice()).into()
}
/// PDFium's numeric field convention, including empty-string/NaN handling and
/// swallowed conversion exceptions. The value may invoke JS during ToNumber.
pub fn maybe_coerce_to_number(value: JsValue, context: &mut Context) -> JsValue {
    let allow_nan = if let Some(text) = value.as_string() {
        if text.is_empty() {
            return value;
        }
        text == js_string!("NaN")
    } else {
        false
    };
    match value.to_number(context) {
        Ok(number) if !number.is_nan() || allow_nan => number.into(),
        _ => value,
    }
}
fn get_name(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "name")?;
    data.first_field("name")?;
    Ok(JsString::from(data.name.as_ref()).into())
}
fn set_name(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(error("name", JsMessage::NotSupportedError))
}
fn get_value(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let field = data(this, "value")?.first_field("value")?;
    let value = match field.field_type() {
        FormFieldType::PushButton => return Err(error("value", JsMessage::ObjectTypeError)),
        FormFieldType::ListBox if field.count_selected_items() > 1 => {
            let values = (0..field.count_selected_items()).map(|i| {
                let index = field.selected_index(i as i32);
                let value = field.option_value(index);
                string(&if value.is_empty() {
                    field.option_label(index)
                } else {
                    value
                })
            });
            JsArray::from_iter(values, context).into()
        }
        FormFieldType::CheckBox | FormFieldType::RadioButton => field
            .controls()
            .iter()
            .find(|control| control.is_checked())
            .map_or_else(
                || js_string!("Off").into(),
                |control| string(&control.export_value()),
            ),
        _ => string(&field.value()),
    };
    Ok(maybe_coerce_to_number(value, context))
}
fn set_value(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "value")?;
    if !data.can_set {
        return Err(error("value", JsMessage::ReadOnlyError));
    }
    let undefined = JsValue::undefined();
    let value = args.first().unwrap_or(&undefined);
    let mut values = Vec::new();
    if let Some(object) = value.as_object().filter(|object| object.is_array()) {
        let array = JsArray::from_object(object.clone())?;
        let mut index = 0u32;
        while u64::from(index) < array.length(context)? {
            values.push(wide_from_utf16(
                &array.get(index, context)?.to_string(context)?.to_vec(),
            ));
            index += 1;
        }
    } else {
        values.push(wide_from_utf16(&value.to_string(context)?.to_vec()));
    }
    apply_or_delay(&data, "value", CjsDelayValue::Value(values), context)
}
fn get_value_as_string(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let field = data(this, "valueAsString")?.first_field("valueAsString")?;
    let value = match field.field_type() {
        FormFieldType::PushButton => {
            return Err(error("valueAsString", JsMessage::ObjectTypeError))
        }
        FormFieldType::CheckBox => {
            let control = field
                .get_control(0)
                .ok_or_else(|| error("valueAsString", JsMessage::BadObjectError))?;
            return Ok(if control.is_checked() {
                js_string!("Yes")
            } else {
                js_string!("Off")
            }
            .into());
        }
        FormFieldType::RadioButton if field.field_flags() & (1 << 25) == 0 => {
            return Ok(field
                .controls()
                .iter()
                .find(|control| control.is_checked())
                .map_or_else(
                    || js_string!("Off").into(),
                    |control| string(&control.export_value()),
                ));
        }
        FormFieldType::ListBox if field.count_selected_items() > 1 => Vec::new(),
        _ => field.value(),
    };
    Ok(string(&value))
}
fn set_value_as_string(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(error("valueAsString", JsMessage::NotSupportedError))
}

fn is_choice(field: &CpdfFormField) -> bool {
    matches!(
        field.field_type(),
        FormFieldType::ComboBox | FormFieldType::ListBox
    )
}
fn is_check_or_radio(field: &CpdfFormField) -> bool {
    matches!(
        field.field_type(),
        FormFieldType::CheckBox | FormFieldType::RadioButton
    )
}
fn check_type(_field: &CpdfFormField, property: &str, allowed: bool) -> JsResult<()> {
    if allowed {
        Ok(())
    } else {
        Err(error(property, JsMessage::ObjectTypeError))
    }
}
fn argument(args: &[JsValue], index: usize) -> JsValue {
    args.get(index).cloned().unwrap_or_else(JsValue::undefined)
}

// These setters intentionally perform only permission checks in the pinned
// source. Keep that boundary explicit rather than inventing field mutations.
macro_rules! permission_only_setter {
    ($($function:ident => $property:literal),* $(,)?) => {$(
        fn $function(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
            data(this, $property)?.check_can_set($property)?;
            Ok(JsValue::undefined())
        }
    )*};
}
permission_only_setter! {
    set_char_limit => "charLimit", set_comb => "comb",
    set_commit_on_sel_change => "commitOnSelChange", set_default_value => "defaultValue",
    set_do_not_scroll => "doNotScroll", set_do_not_spell_check => "doNotSpellCheck",
    set_multiline => "multiline", set_multiple_selection => "multipleSelection",
    set_password => "password",
}
macro_rules! flag_getter {
    ($($function:ident => ($property:literal, $flag:expr, $allowed:expr)),* $(,)?) => {$(
        fn $function(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
            let field = data(this, $property)?.first_field($property)?;
            check_type(&field, $property, ($allowed)(&field))?;
            Ok((field.field_flags() & $flag != 0).into())
        }
    )*};
}
flag_getter! {
    get_comb => ("comb", 1 << 24, |field: &CpdfFormField| field.field_type() == FormFieldType::TextField),
    get_commit_on_sel_change => ("commitOnSelChange", 1 << 26, is_choice),
    get_do_not_scroll => ("doNotScroll", 1 << 23, |field: &CpdfFormField| field.field_type() == FormFieldType::TextField),
    get_do_not_spell_check => ("doNotSpellCheck", 1 << 22, |field: &CpdfFormField| matches!(field.field_type(), FormFieldType::TextField | FormFieldType::ComboBox)),
    get_multiline => ("multiline", 1 << 12, |field: &CpdfFormField| field.field_type() == FormFieldType::TextField),
    get_multiple_selection => ("multipleSelection", 1 << 21, |field: &CpdfFormField| field.field_type() == FormFieldType::ListBox),
    get_password => ("password", 1 << 13, |field: &CpdfFormField| field.field_type() == FormFieldType::TextField),
    get_readonly => ("readonly", 1, |_: &CpdfFormField| true),
    get_required => ("required", 2, |field: &CpdfFormField| field.field_type() != FormFieldType::PushButton),
}
fn get_char_limit(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let field = data(this, "charLimit")?.first_field("charLimit")?;
    check_type(
        &field,
        "charLimit",
        field.field_type() == FormFieldType::TextField,
    )?;
    Ok(field.max_len().into())
}
fn get_default_value(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let field = data(this, "defaultValue")?.first_field("defaultValue")?;
    check_type(
        &field,
        "defaultValue",
        !matches!(
            field.field_type(),
            FormFieldType::PushButton | FormFieldType::Signature
        ),
    )?;
    Ok(string(&field.default_value()))
}
fn set_required(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "required")?;
    data.first_field("required")?;
    data.check_can_set("required")?;
    Ok(JsValue::undefined())
}
fn set_readonly(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "readonly")?;
    let field = data.first_field("readonly")?;
    data.check_can_set("readonly")?;
    let readonly = argument(args, 0).to_boolean();
    let flags = field.field_flags();
    let new_flags = if readonly { flags | 1 } else { flags & !1 };
    if flags != new_flags {
        CfxJsEngine::with_host_callback(context, || field.set_field_flags(new_flags));
    }
    Ok(JsValue::undefined())
}
fn get_type(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let field = data(this, "type")?.first_field("type")?;
    Ok(JsString::from(match field.field_type() {
        FormFieldType::PushButton => "button",
        FormFieldType::CheckBox => "checkbox",
        FormFieldType::RadioButton => "radiobutton",
        FormFieldType::ComboBox => "combobox",
        FormFieldType::ListBox => "listbox",
        FormFieldType::TextField => "text",
        FormFieldType::Signature => "signature",
        _ => "unknown",
    })
    .into())
}
fn set_type(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(error("type", JsMessage::NotSupportedError))
}
fn get_num_items(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let field = data(this, "numItems")?.first_field("numItems")?;
    check_type(&field, "numItems", is_choice(&field))?;
    Ok((field.count_options() as f64).into())
}
fn set_num_items(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(error("numItems", JsMessage::NotSupportedError))
}
fn get_current_value_indices(
    this: &JsValue,
    _: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let field = data(this, "currentValueIndices")?.first_field("currentValueIndices")?;
    check_type(&field, "currentValueIndices", is_choice(&field))?;
    match field.count_selected_items() {
        0 => Ok((-1).into()),
        1 => Ok(field.selected_index(0).into()),
        count => Ok(JsArray::from_iter(
            (0..count).map(|index| field.selected_index(index as i32).into()),
            context,
        )
        .into()),
    }
}
fn set_current_value_indices(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let data = data(this, "currentValueIndices")?;
    data.check_can_set("currentValueIndices")?;
    let value = argument(args, 0);
    let mut indices = Vec::new();
    if value.is_number() {
        indices.push(value.to_i32(context)? as u32);
    } else if let Some(array) = value.as_object().filter(JsObject::is_array) {
        let array = JsArray::from_object(array)?;
        let mut index = 0u32;
        while u64::from(index) < array.length(context)? {
            indices.push(array.get(index, context)?.to_i32(context)? as u32);
            index += 1;
        }
    }
    apply_or_delay(
        &data,
        "currentValueIndices",
        CjsDelayValue::CurrentValueIndices(indices),
        context,
    )
}
fn get_export_values(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "exportValues")?;
    let field = data.first_field("exportValues")?;
    check_type(&field, "exportValues", is_check_or_radio(&field))?;
    let controls = if data.control_index < 0 {
        field.controls()
    } else {
        if data.control_index as usize >= field.count_controls() {
            return Err(error("exportValues", JsMessage::ValueError));
        }
        vec![field
            .get_control(data.control_index as usize)
            .ok_or_else(|| error("exportValues", JsMessage::BadObjectError))?]
    };
    Ok(JsArray::from_iter(
        controls
            .iter()
            .map(|control| string(&control.export_value())),
        context,
    )
    .into())
}
fn set_export_values(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "exportValues")?;
    let field = data.first_field("exportValues")?;
    check_type(&field, "exportValues", is_check_or_radio(&field))?;
    data.check_can_set("exportValues")?;
    if !argument(args, 0)
        .as_object()
        .is_some_and(|object| object.is_array())
    {
        return Err(error("exportValues", JsMessage::BadObjectError));
    }
    // The pinned setter validates only; it does not rewrite export states.
    Ok(JsValue::undefined())
}
fn get_text_color(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "textColor")?;
    let field = data.first_field("textColor")?;
    let control = data.smart_control(&field, "textColor")?;
    let color = control
        .default_appearance()
        .color_argb()
        .filter(|color| color.color_type != ColorType::Transparent)
        .map_or_else(CfxColor::default, |color| {
            CfxColor::make_rgb_bytes(
                ((color.argb >> 16) & 255) as i32,
                ((color.argb >> 8) & 255) as i32,
                (color.argb & 255) as i32,
            )
        });
    Ok(super::cjs_color::convert_pwl_color_to_array(context, color))
}
fn set_text_color(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    data(this, "textColor")?.check_can_set("textColor")?;
    if !argument(args, 0)
        .as_object()
        .is_some_and(|object| object.is_array())
    {
        return Err(error("textColor", JsMessage::BadObjectError));
    }
    // Pinned source validates only; no SetTextColor/DA mutation exists here.
    Ok(JsValue::undefined())
}

fn get_item_at(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let mut index = if args.is_empty() {
        -1
    } else {
        args[0].to_i32(context)?
    };
    let export = args.get(1).is_none_or(JsValue::to_boolean);
    let field = data(this, "getItemAt")?.first_field("getItemAt")?;
    check_type(&field, "getItemAt", is_choice(&field))?;
    if index == -1 || i64::from(index) > field.count_options() as i64 {
        index = i32::try_from(field.count_options())
            .unwrap_or(i32::MAX)
            .saturating_sub(1);
    }
    let value = if export {
        field.option_value(index)
    } else {
        Vec::new()
    };
    Ok(string(&if value.is_empty() {
        field.option_label(index)
    } else {
        value
    }))
}
fn get_array(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "getArray")?;
    let fields = data.fields("getArray")?;
    if fields.is_empty() {
        return Err(error("getArray", JsMessage::BadObjectError));
    }
    let mut names = fields
        .iter()
        .map(|field| field.full_name())
        .collect::<Vec<_>>();
    names.sort();
    let mut objects = Vec::with_capacity(names.len());
    for name in names {
        objects.push(
            field_object(
                context,
                FieldReference {
                    environment: data.environment.clone(),
                    name: utf16_encode(&name).into(),
                },
            )?
            .into(),
        );
    }
    Ok(JsArray::from_iter(objects, context).into())
}
fn check_this_box(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.is_empty() {
        return Err(error("checkThisBox", JsMessage::ParamError));
    }
    let data = data(this, "checkThisBox")?;
    data.check_can_set("checkThisBox")?;
    let index = args[0].to_i32(context)?;
    let checked = args.get(1).is_none_or(JsValue::to_boolean);
    let environment = data.environment("checkThisBox")?;
    let field = data.first_field("checkThisBox")?;
    check_type(&field, "checkThisBox", is_check_or_radio(&field))?;
    if index < 0 || index as usize >= field.count_controls() {
        return Err(error("checkThisBox", JsMessage::ValueError));
    }
    CfxJsEngine::with_host_callback(context, || {
        field.check_control(index as usize, checked, NotificationOption::Notify);
        if environment.is_alive() {
            update_form_field(&environment, &field, true);
        }
    });
    Ok(JsValue::undefined())
}
fn is_box_checked(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let index = if args.is_empty() {
        -1
    } else {
        args[0].to_i32(context)?
    };
    let field = data(this, "isBoxChecked")?.first_field("isBoxChecked")?;
    if index < 0 || index as usize >= field.count_controls() {
        return Err(error("isBoxChecked", JsMessage::ValueError));
    }
    Ok((is_check_or_radio(&field)
        && field
            .get_control(index as usize)
            .is_some_and(|control| control.is_checked()))
    .into())
}
fn is_default_checked(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let index = if args.is_empty() {
        -1
    } else {
        args[0].to_i32(context)?
    };
    let field = data(this, "isDefaultChecked")?.first_field("isDefaultChecked")?;
    if index < 0 || index as usize >= field.count_controls() {
        return Err(error("isDefaultChecked", JsMessage::ValueError));
    }
    Ok((is_check_or_radio(&field)
        && field
            .get_control(index as usize)
            .is_some_and(|control| control.is_default_checked()))
    .into())
}
fn default_is_checked(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let data = data(this, "defaultIsChecked")?;
    data.check_can_set("defaultIsChecked")?;
    if args.is_empty() {
        return Err(error("defaultIsChecked", JsMessage::ParamError));
    }
    data.first_field("defaultIsChecked")?;
    let index = args[0].to_i32(context)?;
    // Reacquire after coercion instead of retaining the source's unobserved
    // pointer across a valueOf callback that may close the document.
    let field = data.first_field("defaultIsChecked")?;
    if index < 0 || index as usize >= field.count_controls() {
        return Err(error("defaultIsChecked", JsMessage::ValueError));
    }
    // The pinned method returns the field-kind check without changing /DV.
    Ok(is_check_or_radio(&field).into())
}
fn button_get_caption(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let face = if args.is_empty() {
        0
    } else {
        args[0].to_i32(context)?
    };
    let data = data(this, "buttonGetCaption")?;
    let field = data.first_field("buttonGetCaption")?;
    check_type(
        &field,
        "buttonGetCaption",
        field.field_type() == FormFieldType::PushButton,
    )?;
    let control = data.smart_control(&field, "buttonGetCaption")?;
    let caption = match face {
        0 => control.normal_caption(),
        1 => control.down_caption(),
        2 => control.rollover_caption(),
        _ => return Err(error("buttonGetCaption", JsMessage::ValueError)),
    };
    Ok(string(&caption))
}

// These methods are explicitly unconditional success in the pinned source;
// they deliberately neither coerce parameters nor modify option/icon data.
macro_rules! source_noop_method {
    ($($function:ident),* $(,)?) => {$(
        fn $function(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> { Ok(JsValue::undefined()) }
    )*};
}
source_noop_method!(
    insert_item_at,
    delete_item_at,
    set_items,
    clear_items,
    button_import_icon
);
fn button_set_caption(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(error("buttonSetCaption", JsMessage::NotSupportedError))
}
fn button_set_icon(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(error("buttonSetIcon", JsMessage::NotSupportedError))
}

fn apply_or_delay(
    data: &FieldData,
    property: &str,
    value: CjsDelayValue,
    context: &mut Context,
) -> JsResult<JsValue> {
    let environment = data.environment(property)?;
    let change = CjsDelayData {
        field_name: data.name.clone(),
        control_index: data.control_index,
        value,
    };
    if data.delay.get() {
        super::cjs_document::add_delay_data(context, change)?;
    } else {
        CfxJsEngine::with_host_callback(context, || do_delay(&environment, change));
    }
    Ok(JsValue::undefined())
}

/// Dispatch a source FIELD_PROP after the caller extracts its queue and lends
/// the active context back to the runtime. No queue borrow spans callbacks.
pub fn do_delay(environment: &CpdfSdkFormFillEnvironment, change: CjsDelayData) {
    if !environment.is_alive() {
        return;
    }
    let name = wide_from_utf16(&change.field_name);
    let index = change.control_index;
    match change.value {
        CjsDelayValue::BorderStyle(value) => {
            let style = match value.as_slice() {
                b"solid" => BorderStyle::Solid,
                b"dashed" => BorderStyle::Dash,
                b"beveled" => BorderStyle::Beveled,
                b"inset" => BorderStyle::Inset,
                b"underline" => BorderStyle::Underline,
                _ => return,
            };
            modify_widgets(environment, &name, index, true, |widget| {
                if widget.border_style() == style {
                    return false;
                }
                widget.annot().set_border_style(style);
                true
            });
        }
        CjsDelayValue::CurrentValueIndices(indices) => {
            set_current_value_indices_native(environment, &name, &indices)
        }
        CjsDelayValue::Display(value) => set_display_native(environment, &name, index, value),
        CjsDelayValue::Hidden(value) => {
            set_display_native(environment, &name, index, i32::from(value))
        }
        CjsDelayValue::LineWidth(value) => {
            modify_widgets(environment, &name, index, true, |widget| {
                if widget.border_width() == value {
                    return false;
                }
                widget.annot().set_border_width(value);
                true
            });
        }
        CjsDelayValue::Rect(value) => {
            modify_widgets(environment, &name, index, true, |widget| {
                let Some(view) = widget.page_view() else {
                    return false;
                };
                let mut rect = value;
                rect.intersect(view.bbox());
                if rect.is_empty() || rect == widget.rect() {
                    return false;
                }
                widget.set_rect(rect);
                true
            });
        }
        CjsDelayValue::Value(values) => set_field_value(environment, &name, index, &values),
    }
}
fn fields_for_name(
    environment: &CpdfSdkFormFillEnvironment,
    name: &[u32],
) -> Vec<Rc<CpdfFormField>> {
    let form = environment.interactive_form().core_form();
    (0..form.count_fields(name))
        .filter_map(|index| form.get_field(index, name))
        .collect()
}
/// Shared traversal of the source setters. Field-wide changes update all
/// controls before notifying; an indexed change updates only that control.
fn modify_widgets(
    environment: &CpdfSdkFormFillEnvironment,
    name: &[u32],
    index: i32,
    reset: bool,
    mut operation: impl FnMut(&CpdfSdkWidget) -> bool,
) -> bool {
    let form = environment.interactive_form();
    for field in fields_for_name(environment, name) {
        if !environment.is_alive() {
            return true;
        }
        if index < 0 {
            let mut changed = false;
            for control in field.controls() {
                if !environment.is_alive() {
                    return true;
                }
                if let Some(widget) = form.get_widget(&control) {
                    if !environment.is_alive() || !widget.is_alive() {
                        return true;
                    }
                    changed |= operation(&widget);
                }
            }
            if changed && environment.is_alive() {
                update_form_field(environment, &field, reset);
            }
        } else {
            if index as usize >= field.count_controls() {
                return false;
            }
            if let Some(control) = field.get_control(index as usize) {
                if let Some(widget) = form.get_widget(&control) {
                    if !environment.is_alive() || !widget.is_alive() {
                        return true;
                    }
                    if operation(&widget) {
                        update_form_control(environment, &control, reset);
                    }
                }
            }
        }
    }
    true
}
fn set_display_native(
    environment: &CpdfSdkFormFillEnvironment,
    name: &[u32],
    index: i32,
    display: i32,
) {
    modify_widgets(environment, name, index, false, |widget| {
        let old = widget.annot().flags();
        // Invisible=1, Hidden=2, Print=4, NoView=32.
        let flags = match display {
            0 => (old & !(1 | 2 | 32)) | 4,
            1 => (old & !(1 | 32)) | 2 | 4,
            2 => old & !(1 | 2 | 4 | 32),
            3 => (old & !(1 | 2)) | 4 | 32,
            _ => old,
        };
        if flags == old {
            return false;
        }
        widget.annot().set_flags(flags);
        true
    });
}
fn set_current_value_indices_native(
    environment: &CpdfSdkFormFillEnvironment,
    name: &[u32],
    indices: &[u32],
) {
    for field in fields_for_name(environment, name) {
        if !environment.is_alive() {
            return;
        }
        if !is_choice(&field) {
            continue;
        }
        let flags = field.field_flags();
        field.clear_selection(NotificationOption::Notify);
        for (position, &index) in indices.iter().enumerate() {
            if !environment.is_alive() {
                return;
            }
            if position != 0 && flags & (1 << 21) == 0 {
                break;
            }
            if (index as usize) < field.count_options() && !field.is_item_selected(index as i32) {
                field.set_item_selection(index as i32, NotificationOption::DoNotNotify);
            }
        }
        if environment.is_alive() {
            update_form_field(environment, &field, true);
        }
    }
}
fn update_form_control(
    environment: &CpdfSdkFormFillEnvironment,
    control: &CpdfFormControl,
    reset: bool,
) {
    if let Some(widget) = environment.interactive_form().get_widget(control) {
        if !environment.is_alive() || !widget.is_alive() {
            return;
        }
        if reset {
            let value = if matches!(
                widget.field_type(),
                FormFieldType::ComboBox | FormFieldType::TextField
            ) {
                widget.on_format()
            } else {
                None
            };
            if !environment.is_alive() || !widget.is_alive() {
                return;
            }
            widget.reset_appearance(value.as_deref(), false);
            if !environment.is_alive() || !widget.is_alive() {
                return;
            }
        }
        if let Some(annotation) = widget.annotation() {
            environment.update_all_views(&annotation);
        }
    }
    if environment.is_alive() {
        environment.set_change_mark();
    }
}

permission_only_setter! {
    set_alignment => "alignment", set_button_align_x => "buttonAlignX", set_button_align_y => "buttonAlignY",
    set_button_fit_bounds => "buttonFitBounds", set_button_scale_how => "buttonScaleHow",
    set_button_scale_when => "buttonScaleWhen", set_calc_order_index => "calcOrderIndex",
    set_editable => "editable", set_highlight => "highlight", set_rich_text => "richText",
    set_rotation => "rotation", set_style => "style", set_text_size => "textSize", set_user_name => "userName",
}
flag_getter! {
    get_editable => ("editable", 1 << 18, |field: &CpdfFormField| field.field_type() == FormFieldType::ComboBox),
    get_file_select => ("fileSelect", 1 << 20, |field: &CpdfFormField| field.field_type() == FormFieldType::TextField),
    get_radios_in_unison => ("radiosInUnison", 1 << 25, |field: &CpdfFormField| field.field_type() == FormFieldType::RadioButton),
    get_rich_text => ("richText", 1 << 25, |field: &CpdfFormField| field.field_type() == FormFieldType::TextField),
}
macro_rules! unsupported_binding {
    ($($function:ident => $property:literal),* $(,)?) => {$(
        fn $function(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> { Err(error($property, JsMessage::NotSupportedError)) }
    )*};
}
unsupported_binding! {
    get_default_style => "defaultStyle", set_default_style => "defaultStyle", set_doc => "doc",
    get_lock => "getLock", set_lock => "setLock", signature_get_modifications => "signatureGetModifications",
    signature_get_seed_value => "signatureGetSeedValue", signature_info => "signatureInfo",
    signature_set_seed_value => "signatureSetSeedValue", signature_sign => "signatureSign", signature_validate => "signatureValidate",
}
source_noop_method!(
    get_rich_value,
    set_rich_value,
    get_source,
    set_source,
    get_submit_name,
    set_submit_name,
    set_action
);

fn get_delay(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(data(this, "delay")?.delay.get().into())
}
fn set_delay(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "delay")?;
    data.check_can_set("delay")?;
    let delay = argument(args, 0).to_boolean();
    data.delay.set(delay);
    if !delay {
        super::cjs_document::do_field_delay(context, &data.name, data.control_index)?;
    }
    Ok(JsValue::undefined())
}
fn get_doc(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    data(this, "doc")?.environment("doc")?;
    Ok(context.global_object().into())
}
fn get_alignment(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "alignment")?;
    let field = data.first_field("alignment")?;
    check_type(
        &field,
        "alignment",
        field.field_type() == FormFieldType::TextField,
    )?;
    let control = data.smart_control(&field, "alignment")?;
    Ok(JsString::from(match control.control_alignment() {
        0 => "left",
        1 => "center",
        2 => "right",
        _ => "",
    })
    .into())
}
macro_rules! button_getter {
    ($($function:ident => ($property:literal, $value:expr)),* $(,)?) => {$(
        fn $function(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
            let data = data(this, $property)?;
            let field = data.first_field($property)?;
            check_type(&field, $property, field.field_type() == FormFieldType::PushButton)?;
            let control = data.smart_control(&field, $property)?;
            Ok(($value)(&control).into())
        }
    )*};
}
button_getter! {
    get_button_align_x => ("buttonAlignX", |control: &CpdfFormControl| control.icon_fit().icon_bottom_left_position().x as i32),
    get_button_align_y => ("buttonAlignY", |control: &CpdfFormControl| control.icon_fit().icon_bottom_left_position().y as i32),
    get_button_fit_bounds => ("buttonFitBounds", |control: &CpdfFormControl| control.icon_fit().fitting_bounds()),
    get_button_position => ("buttonPosition", |control: &CpdfFormControl| control.text_position() as i32),
    get_button_scale_how => ("buttonScaleHow", |control: &CpdfFormControl| !control.icon_fit().is_proportional_scale()),
    get_button_scale_when => ("buttonScaleWhen", |control: &CpdfFormControl| match control.icon_fit().scale_method() { ScaleMethod::Always => 0, ScaleMethod::Bigger => 1, ScaleMethod::Smaller => 2, ScaleMethod::Never => 3 }),
}
fn set_button_position(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    if !data(this, "buttonPosition")?.can_set {
        return Err(error("buttonPosition", JsMessage::BadObjectError));
    }
    Ok(JsValue::undefined())
}
fn get_calc_order_index(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "calcOrderIndex")?;
    let field = data.first_field("calcOrderIndex")?;
    check_type(
        &field,
        "calcOrderIndex",
        matches!(
            field.field_type(),
            FormFieldType::TextField | FormFieldType::ComboBox
        ),
    )?;
    Ok(data
        .environment("calcOrderIndex")?
        .interactive_form()
        .core_form()
        .find_field_in_calculation_order(&field)
        .into())
}
fn get_border_style(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let widget = data(this, "borderStyle")?.widget("borderStyle", context)?;
    Ok(JsString::from(match widget.border_style() {
        BorderStyle::Solid => "solid",
        BorderStyle::Dash => "dashed",
        BorderStyle::Beveled => "beveled",
        BorderStyle::Inset => "inset",
        BorderStyle::Underline => "underline",
    })
    .into())
}
fn set_border_style(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "borderStyle")?;
    data.check_can_set("borderStyle")?;
    let value = utf8_encode(&wide_from_utf16(
        &argument(args, 0).to_string(context)?.to_vec(),
    ));
    apply_or_delay(
        &data,
        "borderStyle",
        CjsDelayValue::BorderStyle(value),
        context,
    )
}
fn get_display(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let flags = data(this, "display")?
        .widget("display", context)?
        .annot()
        .flags();
    Ok(if flags & (1 | 2) != 0 {
        1
    } else if flags & 4 != 0 {
        if flags & 32 != 0 {
            3
        } else {
            0
        }
    } else {
        2
    }
    .into())
}
fn set_display(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "display")?;
    data.check_can_set("display")?;
    let value = argument(args, 0).to_i32(context)?;
    apply_or_delay(&data, "display", CjsDelayValue::Display(value), context)
}
fn get_hidden(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok((data(this, "hidden")?
        .widget("hidden", context)?
        .annot()
        .flags()
        & (1 | 2)
        != 0)
        .into())
}
fn set_hidden(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "hidden")?;
    data.check_can_set("hidden")?;
    apply_or_delay(
        &data,
        "hidden",
        CjsDelayValue::Hidden(argument(args, 0).to_boolean()),
        context,
    )
}
fn get_line_width(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "lineWidth")?;
    let field = data.first_field("lineWidth")?;
    // Native resolves this control but then accidentally reads control zero.
    // Keep the property paired with the control that its setter modifies.
    let control = data.smart_control(&field, "lineWidth")?;
    let environment = data.environment("lineWidth")?;
    let widget = CfxJsEngine::with_host_callback(context, || {
        environment.interactive_form().get_widget(&control)
    })
    .filter(|widget| environment.is_alive() && widget.is_alive())
    .ok_or_else(|| error("lineWidth", JsMessage::BadObjectError))?;
    Ok(widget.border_width().into())
}
fn set_line_width(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "lineWidth")?;
    data.check_can_set("lineWidth")?;
    let value = argument(args, 0).to_i32(context)?;
    apply_or_delay(&data, "lineWidth", CjsDelayValue::LineWidth(value), context)
}
fn get_rect(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let rect = data(this, "rect")?.widget("rect", context)?.rect();
    Ok(JsArray::from_iter(
        [rect.left, rect.top, rect.right, rect.bottom].map(|value| JsValue::from(value as i32)),
        context,
    )
    .into())
}
fn set_rect(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "rect")?;
    data.check_can_set("rect")?;
    let array = argument(args, 0)
        .as_object()
        .filter(JsObject::is_array)
        .ok_or_else(|| error("rect", JsMessage::ValueError))?;
    let array = JsArray::from_object(array)?;
    if array.length(context)? < 4 {
        return Err(error("rect", JsMessage::ValueError));
    }
    let mut coordinates = [0f32; 4];
    for (index, coordinate) in coordinates.iter_mut().enumerate() {
        *coordinate = array.get(index as u32, context)?.to_i32(context)? as f32;
    }
    let rect = CfxFloatRect::new(
        coordinates[0],
        coordinates[1],
        coordinates[2],
        coordinates[3],
    );
    apply_or_delay(&data, "rect", CjsDelayValue::Rect(rect), context)
}
fn get_print(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok((data(this, "print")?
        .widget("print", context)?
        .annot()
        .flags()
        & 4
        != 0)
        .into())
}
fn set_print(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "print")?;
    data.first_field("print")?;
    data.check_can_set("print")?;
    let value = argument(args, 0).to_boolean();
    let environment = data.environment("print")?;
    let name = wide_from_utf16(&data.name);
    let valid = CfxJsEngine::with_host_callback(context, || {
        modify_widgets(&environment, &name, data.control_index, false, |widget| {
            let old = widget.annot().flags();
            let flags = if value { old | 4 } else { old & !4 };
            if old == flags {
                return false;
            }
            widget.annot().set_flags(flags);
            true
        })
    });
    if !valid {
        return Err(error("print", JsMessage::ValueError));
    }
    Ok(JsValue::undefined())
}
fn get_page(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "page")?;
    let field = data.first_field("page")?;
    let environment = data.environment("page")?;
    let widgets = CfxJsEngine::with_host_callback(context, || {
        environment.interactive_form().widgets_for_field(&field)
    });
    if !environment.is_alive() {
        return Err(error("page", JsMessage::BadObjectError));
    }
    if widgets.is_empty() {
        return Ok((-1).into());
    }
    let mut pages = Vec::with_capacity(widgets.len());
    for widget in widgets {
        let view = widget
            .page_view()
            .filter(|view| view.is_alive())
            .ok_or_else(|| error("page", JsMessage::BadObjectError))?;
        pages.push(view.page_index().into());
    }
    Ok(JsArray::from_iter(pages, context).into())
}
fn set_page(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(error("page", JsMessage::ReadOnlyError))
}
fn get_rotation(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "rotation")?;
    let field = data.first_field("rotation")?;
    Ok(data.smart_control(&field, "rotation")?.rotation().into())
}
fn get_fill_color(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "fillColor")?;
    let field = data.first_field("fillColor")?;
    Ok(super::cjs_color::convert_pwl_color_to_array(
        context,
        data.smart_control(&field, "fillColor")?
            .original_background_color(),
    ))
}
fn get_stroke_color(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "strokeColor")?;
    let field = data.first_field("strokeColor")?;
    Ok(super::cjs_color::convert_pwl_color_to_array(
        context,
        data.smart_control(&field, "strokeColor")?
            .original_border_color(),
    ))
}
fn set_fill_color(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "fillColor")?;
    data.first_field("fillColor")?;
    data.check_can_set("fillColor")?;
    if !argument(args, 0)
        .as_object()
        .is_some_and(|object| object.is_array())
    {
        return Err(error("fillColor", JsMessage::BadObjectError));
    }
    Ok(JsValue::undefined())
}
fn set_stroke_color(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "strokeColor")?;
    data.check_can_set("strokeColor")?;
    if !argument(args, 0)
        .as_object()
        .is_some_and(|object| object.is_array())
    {
        return Err(error("strokeColor", JsMessage::BadObjectError));
    }
    Ok(JsValue::undefined())
}
fn set_file_select(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "fileSelect")?;
    let field = data.first_field("fileSelect")?;
    check_type(
        &field,
        "fileSelect",
        field.field_type() == FormFieldType::TextField,
    )?;
    data.check_can_set("fileSelect")?;
    Ok(JsValue::undefined())
}
fn set_radios_in_unison(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "radiosInUnison")?;
    data.first_field("radiosInUnison")?;
    data.check_can_set("radiosInUnison")?;
    Ok(JsValue::undefined())
}
fn get_highlight(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "highlight")?;
    let field = data.first_field("highlight")?;
    check_type(
        &field,
        "highlight",
        field.field_type() == FormFieldType::PushButton,
    )?;
    let control = data.smart_control(&field, "highlight")?;
    Ok(JsString::from(match control.highlighting_mode() {
        HighlightingMode::None => "none",
        HighlightingMode::Invert => "invert",
        HighlightingMode::Outline => "outline",
        HighlightingMode::Push => "push",
        HighlightingMode::Toggle => "toggle",
    })
    .into())
}
fn get_style(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "style")?;
    let field = data.first_field("style")?;
    check_type(&field, "style", is_check_or_radio(&field))?;
    let control = data.smart_control(&field, "style")?;
    let caption = control.normal_caption();
    let selector =
        caption
            .first()
            .copied()
            .unwrap_or(if field.field_type() == FormFieldType::RadioButton {
                u32::from('l')
            } else {
                u32::from('4')
            });
    Ok(JsString::from(match selector {
        108 => "circle",
        56 => "cross",
        117 => "diamond",
        110 => "square",
        72 => "star",
        _ => "check",
    })
    .into())
}
fn get_text_font(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "textFont")?;
    let field = data.first_field("textFont")?;
    let control = data.smart_control(&field, "textFont")?;
    check_type(
        &field,
        "textFont",
        matches!(
            field.field_type(),
            FormFieldType::PushButton
                | FormFieldType::ComboBox
                | FormFieldType::ListBox
                | FormFieldType::TextField
        ),
    )?;
    Ok(string(&control.default_control_font_name().ok_or_else(
        || error("textFont", JsMessage::BadObjectError),
    )?))
}
fn set_text_font(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    data(this, "textFont")?.check_can_set("textFont")?;
    if argument(args, 0).to_string(context)?.is_empty() {
        return Err(error("textFont", JsMessage::ValueError));
    }
    Ok(JsValue::undefined())
}
fn get_text_size(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "textSize")?;
    let field = data.first_field("textSize")?;
    Ok((data
        .smart_control(&field, "textSize")?
        .default_appearance()
        .font_size_or_zero() as i32)
        .into())
}
fn get_user_name(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(string(
        &data(this, "userName")?
            .first_field("userName")?
            .alternate_name(),
    ))
}
fn button_get_icon(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if let Some(face) = args.first() {
        if !(0..=2).contains(&face.to_i32(context)?) {
            return Err(error("buttonGetIcon", JsMessage::ValueError));
        }
    }
    let data = data(this, "buttonGetIcon")?;
    let field = data.first_field("buttonGetIcon")?;
    check_type(
        &field,
        "buttonGetIcon",
        field.field_type() == FormFieldType::PushButton,
    )?;
    data.smart_control(&field, "buttonGetIcon")?;
    Ok(super::cjs_icon::icon_object(context, &[]).into())
}
fn browse_for_file_to_submit(
    this: &JsValue,
    _: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let data = data(this, "browseForFileToSubmit")?;
    let field = data.first_field("browseForFileToSubmit")?;
    check_type(
        &field,
        "browseForFileToSubmit",
        field.field_type() == FormFieldType::TextField && field.field_flags() & (1 << 20) != 0,
    )?;
    let environment = data.environment("browseForFileToSubmit")?;
    CfxJsEngine::with_host_callback(context, || {
        let value = environment.js_field_browse();
        if environment.is_alive() && !value.is_empty() {
            field.set_value(&value, NotificationOption::DoNotNotify);
            if environment.is_alive() {
                update_form_field(&environment, &field, true);
            }
        }
    });
    Ok(JsValue::undefined())
}
fn set_focus(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "setFocus")?;
    let field = data.first_field("setFocus")?;
    if field.count_controls() == 0 {
        return Err(error("setFocus", JsMessage::BadObjectError));
    }
    let environment = data.environment("setFocus")?;
    let current: Option<i32> = if field.count_controls() > 1 {
        Some(
            CfxJsEngine::with_host_callback(context, || environment.current_page_index())
                .ok_or_else(|| error("setFocus", JsMessage::BadObjectError))?,
        )
    } else {
        None
    };
    if !environment.is_alive() {
        return Err(error("setFocus", JsMessage::BadObjectError));
    }
    CfxJsEngine::with_host_callback(context, || {
        let form = environment.interactive_form();
        let widget = field
            .controls()
            .into_iter()
            .filter_map(|control| form.get_widget(&control))
            .find(|widget| {
                current.is_none_or(|index| {
                    widget
                        .page_view()
                        .is_some_and(|view| view.page_index() == index)
                })
            });
        if let Some(annotation) = widget.and_then(|widget| widget.annotation()) {
            if environment.is_alive() {
                environment.set_focus_annot(&annotation);
            }
        }
    });
    Ok(JsValue::undefined())
}

fn set_field_value(
    environment: &CpdfSdkFormFillEnvironment,
    name: &[u32],
    _control_index: i32,
    values: &[Vec<u32>],
) {
    let Some(first) = values.first() else {
        return;
    };
    let form = environment.interactive_form().core_form();
    let fields = (0..form.count_fields(name))
        .filter_map(|index| form.get_field(index, name))
        .collect::<Vec<_>>();
    for field in fields {
        if !environment.is_alive() {
            return;
        }
        if field.full_name() != name {
            continue;
        }
        match field.field_type() {
            FormFieldType::TextField
            | FormFieldType::ComboBox
            | FormFieldType::CheckBox
            | FormFieldType::RadioButton => {
                if field.value() != *first {
                    field.set_value(first, NotificationOption::Notify);
                    if environment.is_alive() {
                        update_form_field(environment, &field, false);
                    }
                }
            }
            FormFieldType::ListBox => {
                let modified = values
                    .iter()
                    .any(|value| !field.is_item_selected(field.find_option(value)))
                    || (0..field.count_selected_items()).any(|position| {
                        let selected = field.selected_index(position as i32);
                        !values
                            .iter()
                            .any(|value| field.find_option(value) == selected)
                    });
                // Also remove old selections absent from the new value array.
                // The source checks additions only, leaving stale selections
                // when the replacement is a proper subset.
                if modified {
                    field.clear_selection(NotificationOption::Notify);
                    for value in values {
                        if !environment.is_alive() {
                            return;
                        }
                        let index = field.find_option(value);
                        if !field.is_item_selected(index) {
                            field.set_item_selection(index, NotificationOption::Notify);
                        }
                    }
                    if environment.is_alive() {
                        update_form_field(environment, &field, false);
                    }
                }
            }
            _ => {}
        }
    }
}
fn update_form_field(
    environment: &CpdfSdkFormFillEnvironment,
    field: &CpdfFormField,
    reset_appearance: bool,
) {
    if !environment.is_alive() {
        return;
    }
    let form = environment.interactive_form();
    if reset_appearance {
        for widget in form.widgets_for_field(field) {
            if !environment.is_alive() {
                return;
            }
            if !widget.is_alive() {
                continue;
            }
            let value = if matches!(
                field.field_type(),
                FormFieldType::TextField | FormFieldType::ComboBox
            ) {
                widget.on_format()
            } else {
                None
            };
            if environment.is_alive() && widget.is_alive() {
                widget.reset_appearance(value.as_deref(), false);
            }
        }
    }
    // Formatting can remove widgets, so query the list again before notifying.
    for widget in form.widgets_for_field(field) {
        if !environment.is_alive() {
            return;
        }
        if widget.is_alive() {
            if let Some(annotation) = widget.annotation() {
                environment.update_all_views(&annotation);
            }
        }
    }
    if environment.is_alive() {
        environment.set_change_mark();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::{
        cpdf_array::CpdfArray, cpdf_dictionary::CpdfDictionary,
        cpdf_scalar_object::CpdfScalarObject, cpdf_string::CpdfString,
    };
    use crate::fpdfapi::parser::{cpdf_document::CpdfDocument, cpdf_parser::ParserError};
    use crate::fpdfdoc::cpdf_formfield::dict_mut;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
    use boa_engine::property::Attribute;
    use std::cell::RefCell;

    #[test]
    fn real_script_choice_button_and_signature_values() {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        let mut fields = CpdfArray::new();
        for (name, kind, flags) in [
            ("list", b"Ch".as_slice(), 1 << 21),
            ("combo", b"Ch", 1 << 17),
            ("check", b"Btn", 0),
            ("radio", b"Btn", 1 << 15),
            ("push", b"Btn", 1 << 16),
            ("signature", b"Sig", 0),
        ] {
            let mut field = CpdfDictionary::new();
            field.set_new_name_for(b"Type", b"Annot");
            field.set_new_name_for(b"Subtype", b"Widget");
            field.set_new_name_for(b"FT", kind);
            field.set_new_for(b"T", CpdfString::from_bytes(None, name.as_bytes()));
            field.set_new_for(b"Ff", CpdfScalarObject::number_from_signed(flags));
            field.set_new_for(b"DA", CpdfString::from_bytes(None, b"0.1 0.2 0.3 rg"));
            if kind == b"Ch" {
                field.set_new_for(b"DV", CpdfString::from_bytes(None, b"20"));
                let mut options = CpdfArray::new();
                for (export, label) in [(b"10".as_slice(), b"Ten".as_slice()), (b"20", b"Twenty")] {
                    let mut option = CpdfArray::new();
                    option.append_new_string(export);
                    option.append_new_string(label);
                    options.append_new(option);
                }
                field.set_new_for(b"Opt", options);
            }
            if kind == b"Btn" {
                let mut normal = CpdfDictionary::new();
                normal.set_new_for(b"Selected", CpdfDictionary::new());
                normal.set_new_for(b"Off", CpdfDictionary::new());
                let mut appearance = CpdfDictionary::new();
                appearance.set_new_for(b"N", normal);
                field.set_new_for(b"AP", appearance);
                field.set_new_name_for(b"AS", b"Off");
                field.set_new_name_for(b"DV", b"Selected");
                let mut captions = CpdfDictionary::new();
                captions.set_new_for(b"CA", CpdfString::from_bytes(None, b"Normal"));
                captions.set_new_for(b"AC", CpdfString::from_bytes(None, b"Down"));
                captions.set_new_for(b"RC", CpdfString::from_bytes(None, b"Rollover"));
                captions.set_new_for(b"TP", CpdfScalarObject::number_from_signed(2));
                let mut fit = CpdfDictionary::new();
                fit.set_new_name_for(b"SW", b"B");
                fit.set_new_name_for(b"S", b"A");
                fit.set_new_for(b"FB", CpdfScalarObject::boolean(true));
                let mut alignment = CpdfArray::new();
                alignment.append_new(CpdfScalarObject::number_from_signed(1));
                alignment.append_new(CpdfScalarObject::number_from_signed(0));
                fit.set_new_for(b"A", alignment);
                captions.set_new_for(b"IF", fit);
                field.set_new_for(b"MK", captions);
            }
            fields.append_new(field);
        }
        let mut form = CpdfDictionary::new();
        form.set_new_for(b"Fields", fields);
        dict_mut(&document.borrow().root().unwrap(), |root| {
            root.set_new_for(b"AcroForm", form)
        });
        let environment = CpdfSdkFormFillEnvironment::new(&document, None);
        let runtime = environment.get_or_create_runtime().unwrap();
        runtime.engine().eval(&r#"
            function check(condition) { if (!condition) throw Error('choice assertion'); }
            function errorOf(callback) { try { callback(); } catch (e) { return e; } throw Error('missing exception'); }
            let list = getField('list');
            check(list.type === 'listbox' && list.multipleSelection === true);
            check(list.numItems === 2 && list.defaultValue === '20' && list.commitOnSelChange === false);
            check(list.currentValueIndices === -1);
            check(list.getItemAt(0) === '10' && list.getItemAt(0, false) === 'Ten');
            check(list.getItemAt() === '20' && list.getItemAt(100) === '20');
            check(list.getItemAt(2) === '' && list.getItemAt(-2) === '');
            check(errorOf(() => list.numItems = 5) === 'Field.numItems: Operation not supported.');
            check(errorOf(() => list.type = 'text') === 'Field.type: Operation not supported.');
            check(errorOf(() => list.charLimit) === 'Field.charLimit: Object is of the wrong type.');
            let dangerous = { valueOf() { throw Error('must not coerce'); }, toString() { throw Error('must not coerce'); } };
            list.multipleSelection = dangerous;
            list.commitOnSelChange = dangerous;
            list.defaultValue = dangerous;
            list.required = dangerous;
            check(list.multipleSelection && !list.commitOnSelChange && !list.required && list.defaultValue === '20');
            list.setItems(dangerous); list.insertItemAt(dangerous); list.deleteItemAt(dangerous); list.clearItems();
            check(list.numItems === 2);
            list.currentValueIndices = [1, 0, 1, -1, 999];
            check(JSON.stringify(list.currentValueIndices) === '[0,1]');
            list.currentValueIndices = [{ valueOf() { list.currentValueIndices = 1; return 0; } }];
            check(list.currentValueIndices === 0 && list.value === 10);
            list.currentValueIndices = 'unrecognized input clears selection';
            check(list.currentValueIndices === -1);
            list.value = ['10', '20'];
            check(Array.isArray(list.value));
            check(list.value.length === 2 && list.value[0] === '10' && list.value[1] === '20');
            check(list.valueAsString === '');
            list.value = ['10'];
            check(list.currentValueIndices === 0 && list.value === 10 && list.valueAsString === '10');
            let combo = getField('combo');
            check(combo.type === 'combobox');
            combo.currentValueIndices = [1, 0];
            check(combo.currentValueIndices === 1 && combo.value === 20);
            combo.value = '20';
            check(combo.value === 20 && combo.valueAsString === '20');
            let checkbox = getField('check');
            check(checkbox.type === 'checkbox');
            check(checkbox.exportValues.length === 1 && checkbox.exportValues[0] === 'Selected');
            checkbox.exportValues = [dangerous];
            check(checkbox.exportValues[0] === 'Selected');
            check(errorOf(() => checkbox.exportValues = 3) === 'Field.exportValues: Object no longer exists.');
            check(checkbox.isDefaultChecked(0) && !checkbox.isBoxChecked(0));
            check(checkbox.defaultIsChecked(0) === true);
            check(errorOf(() => checkbox.isBoxChecked()) === 'Field.isBoxChecked: Incorrect parameter value.');
            check(errorOf(() => checkbox.checkThisBox()) === 'Field.checkThisBox: Incorrect number of parameters passed to function.');
            checkbox.checkThisBox({ valueOf() { return 0; } });
            check(checkbox.isBoxChecked(0));
            checkbox.checkThisBox(0, false);
            check(!checkbox.isBoxChecked(0));
            check(checkbox.value === 'Off' && checkbox.valueAsString === 'Off');
            checkbox.value = 'Selected';
            check(checkbox.value === 'Selected' && checkbox.valueAsString === 'Yes');
            checkbox.value = 'Off';
            check(checkbox.value === 'Off' && checkbox.valueAsString === 'Off');
            let radio = getField('radio');
            check(radio.type === 'radiobutton');
            radio.value = 'Selected';
            check(radio.value === 'Selected' && radio.valueAsString === 'Selected');
            let signature = getField('signature');
            check(signature.type === 'signature');
            signature.value = 'ignored';
            check(signature.value === '' && signature.valueAsString === '');
            for (let property of ['value', 'valueAsString']) {
                let threw = false;
                try { getField('push')[property]; } catch (e) { threw = /wrong type/.test(String(e)); }
                check(threw);
            }
            let push = getField('push');
            check(push.type === 'button');
            check(push.doc === globalThis && push.page === -1);
            check(push.buttonAlignX === 1 && push.buttonAlignY === 0 && push.buttonFitBounds);
            check(push.buttonPosition === 2 && push.buttonScaleHow === true && push.buttonScaleWhen === 1);
            check(push.highlight === 'invert' && push.rotation === 0 && push.textSize === 0);
            check(push.buttonGetIcon().name === '' && push.buttonGetIcon(2).name === '');
            check(errorOf(() => push.buttonGetIcon(-1)) === 'Field.buttonGetIcon: Incorrect parameter value.');
            check(errorOf(() => push.defaultStyle) === 'Field.defaultStyle: Operation not supported.');
            check(errorOf(() => push.getLock()) === 'Field.getLock: Operation not supported.');
            check(errorOf(() => push.signatureValidate()) === 'Field.signatureValidate: Operation not supported.');
            check(push.richValue === undefined && push.source === undefined && push.submitName === undefined);
            push.richValue = dangerous; push.source = dangerous; push.submitName = dangerous;
            check(push.richValue === undefined && push.source === undefined && push.submitName === undefined);
            check(errorOf(() => push.page = 0) === 'Field.page: Cannot assign to readonly property.');
            for (let property of ['buttonAlignX','buttonAlignY','buttonFitBounds','buttonPosition','buttonScaleHow','buttonScaleWhen','highlight','rotation','textSize']) {
                let original = push[property]; push[property] = dangerous; check(push[property] === original);
            }
            check(push.buttonGetCaption() === 'Normal' && push.buttonGetCaption(1) === 'Down' && push.buttonGetCaption(2) === 'Rollover');
            check(errorOf(() => push.buttonGetCaption(3)) === 'Field.buttonGetCaption: Incorrect parameter value.');
            check(errorOf(() => push.buttonSetCaption('new')) === 'Field.buttonSetCaption: Operation not supported.');
            check(errorOf(() => push.buttonSetIcon(null)) === 'Field.buttonSetIcon: Operation not supported.');
            check(push.buttonImportIcon(dangerous) === undefined);
            check(errorOf(() => push.required) === 'Field.required: Object is of the wrong type.');
            check(errorOf(() => signature.defaultValue) === 'Field.defaultValue: Object is of the wrong type.');
            check(errorOf(() => list.checkThisBox(0)) === 'Field.checkThisBox: Object is of the wrong type.');
            let colors = checkbox.textColor;
            check(colors[0] === 'RGB' && Math.abs(colors[1] - 26/255) < 1e-6 && Math.abs(colors[2] - 51/255) < 1e-6);
            checkbox.textColor = ['RGB', dangerous];
            check(JSON.stringify(checkbox.textColor) === JSON.stringify(colors));
            check(errorOf(() => checkbox.textColor = 'red') === 'Field.textColor: Object no longer exists.');
            check(checkbox.getArray().length === 1 && checkbox.getArray()[0].name === 'check');
            let method = Object.getOwnPropertyDescriptor(checkbox, 'checkThisBox');
            check(!method.writable && method.enumerable && method.configurable);
        "#.encode_utf16().collect::<Vec<_>>()).unwrap();
        assert!(environment.change_mark());
    }

    #[test]
    fn real_script_field_reads_writes_arrays_errors_and_reentrant_coercion() {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        assert_eq!(
            document.borrow_mut().load_doc(
                Rc::new(CfxReadOnlyContainerStream::new(
                    include_bytes!("../../../../upstream/pdfium/testing/resources/text_form.pdf")
                        .to_vec()
                )),
                b""
            ),
            ParserError::Success
        );
        let environment = CpdfSdkFormFillEnvironment::new(&document, None);
        let field = environment
            .interactive_form()
            .core_form()
            .get_field(0, &[])
            .unwrap();
        let runtime = environment.get_or_create_runtime().unwrap();
        runtime.engine().with_context(|context| {
            context.register_global_property(js_string!("fieldName"), JsString::from(utf16_encode(&field.full_name()).as_slice()), Attribute::all())?;
            context.eval(boa_engine::Source::from_bytes(r#"
                function check(condition) { if (!condition) throw Error('field assertion'); }
                for (let method of ['getField', 'gotoNamedDest']) {
                    let descriptor = Object.getOwnPropertyDescriptor(globalThis, method);
                    check(descriptor.writable === false && descriptor.enumerable === true && descriptor.configurable === true);
                    let original = globalThis[method];
                    globalThis[method] = 'ignored';
                    check(globalThis[method] === original);
                    check(original.length === 1);
                }
                let f = getField(fieldName);
                check(f.name === fieldName);
                let inherited = Object.create(f);
                inherited.value = 'inherited property holder';
                check(f.valueAsString === 'inherited property holder' && inherited.valueAsString === 'inherited property holder');
                check(inherited.getArray() === undefined && f.getArray.call({}) === undefined);
                check(f.type === 'text' && typeof f.charLimit === 'number' && typeof f.defaultValue === 'string');
                let noCoercion = { valueOf() { throw Error('unexpected conversion'); }, toString() { throw Error('unexpected conversion'); } };
                for (let property of ['comb', 'multiline', 'password', 'doNotScroll', 'doNotSpellCheck', 'charLimit', 'defaultValue']) {
                    let original = f[property];
                    f[property] = noCoercion;
                    check(f[property] === original);
                }
                let originalReadonly = f.readonly;
                f.readonly = true;
                check(f.readonly === true);
                f.value = 'permitted by attach-time document permissions';
                check(f.valueAsString === 'permitted by attach-time document permissions');
                f.readonly = originalReadonly;
                check(getField('__missing__') === undefined);
                f.value = '0012';
                check(f.value === 12 && f.valueAsString === '0012');
                f.value = ['42', 'ignored'];
                check(f.value === 42 && f.valueAsString === '42');
                f.value = [];
                check(f.value === 42);
                f.value = { toString() { f.value = 'inner'; return 'outer'; } };
                check(f.valueAsString === 'outer');
                let a = ['initial'];
                Object.defineProperty(a, 0, { get() { a.push('added'); return 'first'; } });
                f.value = a;
                check(f.valueAsString === 'first');
                f.value = { toString() { return '\ud800'; } };
                check(f.valueAsString.charCodeAt(0) === 0xd800);
                let threw = false;
                try { f.valueAsString = 'unsupported'; } catch (e) { threw = e === 'Field.valueAsString: Operation not supported.'; }
                check(threw);
                threw = false;
                try { f.name = 'unsupported'; } catch (e) { threw = e === 'Field.name: Operation not supported.'; }
                check(threw);
                threw = false;
                try { getField(); } catch (e) { threw = e === 'Document.getField: Incorrect number of parameters passed to function.'; }
                check(threw);
                threw = false;
                try { f.value = { toString() { throw Error('coercion'); } }; } catch (e) { threw = /coercion/.test(String(e)); }
                check(threw && f.valueAsString.charCodeAt(0) === 0xd800);
            "#)).map(|_| ())
        }).unwrap();
        assert!(environment.is_alive());
        // Field objects observe document lifetime weakly even when the JS
        // object and native runtime are still reachable.
        drop(document);
        runtime.engine().eval(&r#"
            let closed = false;
            try { f.currentValueIndices; } catch (e) { closed = e === 'Field.currentValueIndices: Object no longer exists.'; }
            check(closed);
            closed = false;
            try { f.textColor; } catch (e) { closed = e === 'Field.textColor: Object no longer exists.'; }
            check(closed);
        "#.encode_utf16().collect::<Vec<_>>()).unwrap();
    }
    #[test]
    fn line_width_reads_the_selected_control_and_tracks_indexed_mutation() {
        use crate::fpdfapi::page::cpdf_page::CpdfPage;
        use crate::fpdfapi::parser::{cpdf_object::retain_object, cpdf_reference::CpdfReference};
        use crate::fpdfsdk::cpdfsdk_pageview::SdkPage;
        use crate::fxcrt::fx_coordinates::CfxFloatRect;

        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        let holder = document.borrow().holder();
        let mut field = CpdfDictionary::new();
        field.set_new_name_for(b"FT", b"Tx");
        field.set_new_for(b"T", CpdfString::from_bytes(None, b"input"));
        let field = retain_object(field);
        let field_number = holder.borrow_mut().add_indirect_object(field.clone());
        let mut children = CpdfArray::new();
        let mut annotations = CpdfArray::new();
        for (index, width) in [1, 3].into_iter().enumerate() {
            let mut widget = CpdfDictionary::new();
            widget.set_new_name_for(b"Subtype", b"Widget");
            widget.set_new_for(b"Parent", CpdfReference::new(&holder, field_number));
            widget.set_rect_for(
                b"Rect",
                CfxFloatRect::new(
                    10.,
                    10. + index as f32 * 40.,
                    100.,
                    40. + index as f32 * 40.,
                ),
            );
            let mut border = CpdfDictionary::new();
            border.set_new_for(b"W", CpdfScalarObject::number_from_signed(width));
            widget.set_new_for(b"BS", border);
            let widget = retain_object(widget);
            let widget_number = holder.borrow_mut().add_indirect_object(widget);
            children.append_new(CpdfReference::new(&holder, widget_number));
            annotations.append_new(CpdfReference::new(&holder, widget_number));
        }
        dict_mut(&field, |dict| dict.set_new_for(b"Kids", children));
        dict_mut(&document.borrow().root().unwrap(), |root| {
            let mut fields = CpdfArray::new();
            fields.append_new(CpdfReference::new(&holder, field_number));
            let mut form = CpdfDictionary::new();
            form.set_new_for(b"Fields", fields);
            root.set_new_for(b"AcroForm", form);
        });
        let page_dict = document.borrow_mut().create_new_page(0).unwrap();
        dict_mut(&page_dict, |dict| {
            dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0., 0., 200., 200.));
            dict.set_new_for(b"Annots", annotations);
        });
        let environment = CpdfSdkFormFillEnvironment::new(&document, None);
        let page = CpdfPage::from_document(&mut document.borrow_mut(), 0).unwrap();
        let sdk_page = SdkPage::new(&document, &page);
        environment.get_or_create_page_view(&sdk_page).unwrap();
        let runtime = environment.get_or_create_runtime().unwrap();
        runtime
            .engine()
            .eval(
                &r#"
            function check(v){if(!v)throw Error('indexed width assertion')}
            let first=getField('input..0'), second=getField('input..1');
            check(first.lineWidth===1 && second.lineWidth===3);
            second.lineWidth=4;
            check(first.lineWidth===1 && second.lineWidth===4);
            first.lineWidth=2;
            check(first.lineWidth===2 && second.lineWidth===4);
        "#
                .encode_utf16()
                .collect::<Vec<_>>(),
            )
            .unwrap();
    }

    #[test]
    fn field_name_control_suffix_and_normalization() {
        for (input, expected) in [
            ("a.0", Some(("a", 0))),
            ("a.0 ", Some(("a", 0))),
            ("a. 0", None),
            ("a.0x", None),
            ("a.12x", Some(("a", 12))),
            ("a.-2", Some(("a", -2))),
            ("a", None),
            ("a.", None),
            ("a.2147483648", None),
        ] {
            let name = input.encode_utf16().collect::<Vec<_>>();
            let actual = parse_field_name(&name)
                .map(|(name, index)| (String::from_utf16(name).unwrap(), index));
            assert_eq!(
                actual,
                expected.map(|(name, index)| (name.to_owned(), index)),
                "{input}"
            );
        }
        assert_eq!(
            normalize_name(&"a....b".encode_utf16().collect::<Vec<_>>()),
            "a..b".encode_utf16().collect::<Vec<_>>()
        );
    }
    #[test]
    fn numeric_coercion_preserves_empty_nan_and_failed_objects() {
        let context = &mut Context::default();
        for (text, expected) in [("", None), ("12", Some(12.0)), ("text", None)] {
            let value = JsValue::from(JsString::from(text));
            let result = maybe_coerce_to_number(value.clone(), context);
            if let Some(expected) = expected {
                assert_eq!(result.as_number(), Some(expected));
            } else {
                assert_eq!(result, value);
            }
        }
        assert!(maybe_coerce_to_number(js_string!("NaN").into(), context)
            .as_number()
            .unwrap()
            .is_nan());
        let value = context
            .eval(boa_engine::Source::from_bytes(
                "({ valueOf() { throw Error('conversion'); } })",
            ))
            .unwrap();
        assert_eq!(maybe_coerce_to_number(value.clone(), context), value);
    }
}
