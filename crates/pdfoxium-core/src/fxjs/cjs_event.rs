// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fxjs/cjs_event.{h,cpp}` property behavior on the real ECMAScript engine.
//! Event channels are acquired before coercion, but interior borrows never span
//! JavaScript value conversion or runtime host callbacks.
use super::cjs_event_context::{CjsEventContext, Kind};
use super::cjs_runtime::CjsRuntime;
use super::js_resources::{js_format_error_string, js_get_string_from_id, JsMessage};
use boa_engine::{
    object::{FunctionObjectBuilder, ObjectInitializer},
    property::Attribute,
    Context, JsError, JsResult, JsString, JsValue, NativeFunction,
};
use std::rc::Rc;

type Callback = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;
fn error_details(property: &str, details: &[u16]) -> JsError {
    let text = js_format_error_string("event", Some(property), details);
    JsError::from_opaque(JsString::from(text.as_slice()).into())
}
fn error(property: &str, message: JsMessage) -> JsError {
    error_details(property, &js_get_string_from_id(message))
}
fn custom_error(property: &str, details: &str) -> JsError {
    error_details(property, &details.encode_utf16().collect::<Vec<_>>())
}
fn current(context: &Context, property: &str) -> JsResult<Rc<CjsEventContext>> {
    CjsRuntime::from_context(context)
        .and_then(|runtime| runtime.current_event_context())
        .filter(|event| event.is_valid())
        .ok_or_else(|| error(property, JsMessage::BadObjectError))
}
fn check_after_coercion(
    context: &Context,
    event: &Rc<CjsEventContext>,
    property: &str,
) -> JsResult<()> {
    let active = current(context, property)?;
    if Rc::ptr_eq(event, &active) {
        Ok(())
    } else {
        Err(error(property, JsMessage::BadObjectError))
    }
}
fn undefined(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::undefined())
}
fn string(text: &[u16]) -> JsValue {
    JsString::from(text).into()
}
fn get_change(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let channel = current(context, "change")?.change();
    let value = string(&channel.borrow());
    Ok(value)
}
fn set_change(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    // The source intentionally ignores non-string values without coercion.
    if let Some(value) = args.first().and_then(JsValue::as_string) {
        *current(context, "change")?.change().borrow_mut() = value.to_vec();
    }
    Ok(JsValue::undefined())
}
fn get_change_ex(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(string(&current(context, "changeEx")?.change_ex()))
}
fn get_commit_key(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(current(context, "commitKey")?.commit_key().into())
}
fn get_field_full(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let event = current(context, "fieldFull")?;
    if event.event_kind() != Kind::FieldKeystroke {
        return Err(custom_error("fieldFull", "unrecognized event"));
    }
    Ok(event.field_full().into())
}
fn get_key_down(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(current(context, "keyDown")?.key_down().into())
}
fn get_modifier(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(current(context, "modifier")?.modifier().into())
}
fn get_name(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(JsString::from(current(context, "name")?.name()).into())
}
fn get_rc(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(current(context, "rc")?.rc().get().into())
}
fn set_rc(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    current(context, "rc")?
        .rc()
        .set(args.first().is_some_and(JsValue::to_boolean));
    Ok(JsValue::undefined())
}
fn get_sel_end(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let event = current(context, "selEnd")?;
    Ok(if event.event_kind() == Kind::FieldKeystroke {
        event.sel_end().into()
    } else {
        JsValue::undefined()
    })
}
fn get_sel_start(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let event = current(context, "selStart")?;
    Ok(if event.event_kind() == Kind::FieldKeystroke {
        event.sel_start().into()
    } else {
        JsValue::undefined()
    })
}
fn set_sel_end(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let event = current(context, "selEnd")?;
    if event.event_kind() == Kind::FieldKeystroke {
        let value = args
            .first()
            .unwrap_or(&JsValue::undefined())
            .to_i32(context)?;
        check_after_coercion(context, &event, "selEnd")?;
        event.set_sel_end(value);
    }
    Ok(JsValue::undefined())
}
fn set_sel_start(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let event = current(context, "selStart")?;
    if event.event_kind() == Kind::FieldKeystroke {
        let value = args
            .first()
            .unwrap_or(&JsValue::undefined())
            .to_i32(context)?;
        check_after_coercion(context, &event, "selStart")?;
        event.set_sel_start(value);
    }
    Ok(JsValue::undefined())
}
fn get_shift(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(current(context, "shift")?.shift().into())
}
fn get_source(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let reference = current(context, "source")?
        .source_field()
        .map_err(|_| error("source", JsMessage::BadObjectError))?;
    CjsRuntime::field_object(context, reference).map(Into::into)
}
fn get_target(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let reference = current(context, "target")?
        .target_field()
        .map_err(|_| error("target", JsMessage::BadObjectError))?;
    CjsRuntime::field_object(context, reference).map(Into::into)
}
fn get_target_name(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(string(&current(context, "targetName")?.target_name()))
}
fn get_type(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(JsString::from(current(context, "type")?.event_type()).into())
}
fn get_value(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let event = current(context, "value")?;
    if event.event_type() != "Field" {
        return Err(custom_error("value", "Bad event type."));
    }
    let channel = event
        .value()
        .ok_or_else(|| error("value", JsMessage::BadObjectError))?;
    let value = string(&channel.borrow());
    Ok(value)
}
fn set_value(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let event = current(context, "value")?;
    if event.event_type() != "Field" {
        return Err(custom_error("value", "Bad event type."));
    }
    let channel = event
        .value()
        .ok_or_else(|| error("value", JsMessage::BadObjectError))?;
    let value = args
        .first()
        .ok_or_else(|| error("value", JsMessage::BadObjectError))?;
    if value.is_null_or_undefined() || value.is_boolean() {
        return Err(error("value", JsMessage::InvalidSetError));
    }
    let value = value.to_string(context)?;
    check_after_coercion(context, &event, "value")?;
    *channel.borrow_mut() = value.to_vec();
    Ok(JsValue::undefined())
}
fn get_will_commit(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(current(context, "willCommit")?.will_commit().into())
}
macro_rules! readonly_setter {
    ($callback:ident, $property:literal) => {
        fn $callback(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
            Err(error($property, JsMessage::NotSupportedError))
        }
    };
}
readonly_setter!(set_change_ex, "changeEx");
readonly_setter!(set_commit_key, "commitKey");
readonly_setter!(set_field_full, "fieldFull");
readonly_setter!(set_key_down, "keyDown");
readonly_setter!(set_modifier, "modifier");
readonly_setter!(set_name, "name");
readonly_setter!(set_shift, "shift");
readonly_setter!(set_source, "source");
readonly_setter!(set_target, "target");
readonly_setter!(set_target_name, "targetName");
readonly_setter!(set_type, "type");
readonly_setter!(set_will_commit, "willCommit");

pub fn register_event(context: &mut Context) -> JsResult<()> {
    const PROPERTIES: &[(&str, Callback, Callback)] = &[
        ("change", get_change, set_change),
        ("changeEx", get_change_ex, set_change_ex),
        ("commitKey", get_commit_key, set_commit_key),
        ("fieldFull", get_field_full, set_field_full),
        ("keyDown", get_key_down, set_key_down),
        ("modifier", get_modifier, set_modifier),
        ("name", get_name, set_name),
        ("rc", get_rc, set_rc),
        // These rich-text properties have explicit no-op Success methods in
        // the ordinary upstream implementation, rather than missing ports.
        ("richChange", undefined, undefined),
        ("richChangeEx", undefined, undefined),
        ("richValue", undefined, undefined),
        ("selEnd", get_sel_end, set_sel_end),
        ("selStart", get_sel_start, set_sel_start),
        ("shift", get_shift, set_shift),
        ("source", get_source, set_source),
        ("target", get_target, set_target),
        ("targetName", get_target_name, set_target_name),
        ("type", get_type, set_type),
        ("value", get_value, set_value),
        ("willCommit", get_will_commit, set_will_commit),
    ];
    let accessors: Vec<_> = PROPERTIES
        .iter()
        .map(|&(name, get, set)| {
            let get = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(get))
                .name(JsString::from(format!("get {name}")))
                .length(0)
                .constructor(false)
                .build();
            let set = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(set))
                .name(JsString::from(format!("set {name}")))
                .length(1)
                .constructor(false)
                .build();
            (name, get, set)
        })
        .collect();
    let mut object = ObjectInitializer::new(context);
    for (name, get, set) in accessors {
        object.accessor(JsString::from(name), Some(get), Some(set), Attribute::all());
    }
    let event = object.build();
    context.register_global_property(JsString::from("event"), event, Attribute::all())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::{cpdf_document::CpdfDocument, cpdf_parser::ParserError};
    use crate::fpdfdoc::cpdf_formfield::CpdfFormField;
    use crate::fpdfsdk::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment;
    use crate::fpdfsdk::formfiller::cffl_fieldaction::CfflFieldAction;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
    use crate::fxjs::cjs_event_context::FieldActionChannels;
    use std::cell::RefCell;
    fn fixture() -> (
        Rc<RefCell<CpdfDocument>>,
        Rc<CpdfSdkFormFillEnvironment>,
        Rc<CjsRuntime>,
        Rc<CpdfFormField>,
    ) {
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
        let env = CpdfSdkFormFillEnvironment::new(&document, None);
        let runtime = CjsRuntime::new(Rc::downgrade(&env)).unwrap();
        let field = env
            .interactive_form()
            .core_form()
            .get_field(0, &[])
            .unwrap();
        (document, env, runtime, field)
    }
    fn script(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }
    #[test]
    fn actual_event_properties_preserve_aliases_coercions_and_raw_utf16() {
        let (_document, _env, runtime, field) = fixture();
        let mut action = CfflFieldAction {
            b_modifier: true,
            b_key_down: true,
            b_will_commit: true,
            b_field_full: true,
            s_change: vec![65],
            s_change_ex: vec![66],
            s_value: "before".chars().map(u32::from).collect(),
            n_sel_start: 2,
            n_sel_end: 4,
            ..Default::default()
        };
        let channels = FieldActionChannels::from_action(&action);
        let event = runtime.new_event_context();
        event.on_field_keystroke(&field, &channels);
        event
            .run_script(&script(
                r#"
            if (event.name !== "Keystroke" || event.type !== "Field" ||
                event.commitKey !== 0 || !event.keyDown || !event.modifier ||
                event.shift || !event.fieldFull || !event.willCommit ||
                event.change !== "A" || event.changeEx !== "B" || !event.rc ||
                event.value !== "before" || event.selStart !== 2 || event.selEnd !== 4)
                throw "incorrect initial event properties";
            event.change = {toString(){throw "change must not coerce nonstrings"}};
            if(event.change !== "A") throw "change unexpectedly coerced";
            const marker = {};
            try {event.selEnd = {valueOf(){throw marker}}} catch(error) {
                if(error !== marker) throw "opaque coercion error lost";
            }
            event.value = {toString(){
                if(event.value !== "before") throw "value borrow survived coercion";
                event.selStart = {valueOf(){event.change = "during"; return 7.9}};
                return "\udc00\u0000\ud834\udd1e";
            }};
            event.selEnd = 4294967299;
            event.rc = 0;
            if(event.richChange !== undefined || event.richChangeEx !== undefined ||
                event.richValue !== undefined) throw "rich properties should have no value";
            event.richValue = {toString(){throw "rich setter must not coerce"}};
            if(event.change !== "during" || event.selStart !== 7 || event.selEnd !== 3 || event.rc)
                throw "event channels not updated";
            for(const property of ["changeEx","commitKey","fieldFull","keyDown",
                "modifier","name","shift","source","target","targetName","type","willCommit"]) {
                let rejected = false;
                try {event[property] = 1} catch(error) {
                    rejected = typeof error === "string" &&
                        error === "event." + property + ": Operation not supported.";
                }
                if(!rejected) throw "readonly setter accepted " + property;
            }
        "#,
            ))
            .unwrap();
        channels.write_back(&mut action);
        assert_eq!(action.s_value, vec![0xdc00, 0, 0x1d11e]);
        assert_eq!(
            action.s_change,
            "during".chars().map(u32::from).collect::<Vec<_>>()
        );
        assert_eq!(
            (action.n_sel_start, action.n_sel_end, action.b_rc),
            (7, 3, false)
        );
        assert!(!event.is_valid());
    }
    #[test]
    fn value_event_gates_and_selection_ignores_coercion_outside_keystroke() {
        let (_document, _env, runtime, field) = fixture();
        let action = CfflFieldAction {
            s_value: vec![65],
            ..Default::default()
        };
        let channels = FieldActionChannels::from_action(&action);
        let event = runtime.new_event_context();
        event.on_field_validate(&field, &channels);
        event
            .run_script(&script(
                r#"
            if(event.name !== "Validate" || event.commitKey !== -1 || event.willCommit)
                throw "incorrect validate setup";
            event.selStart = {valueOf(){throw "must not coerce outside keystroke"}};
            event.selEnd = {valueOf(){throw "must not coerce outside keystroke"}};
            if(event.selStart !== undefined || event.selEnd !== undefined)
                throw "selection must be undefined outside keystroke";
            let fullRejected = false;
            try {event.fieldFull} catch(error) {
                fullRejected = typeof error === "string" &&
                    error === "event.fieldFull: unrecognized event";
            }
            if(!fullRejected) throw "fieldFull accepted wrong event";
            for(const value of [null, undefined, false, true]) {
                let rejected = false;
                try {event.value = value} catch(error) {
                    rejected = typeof error === "string" &&
                        error === "event.value: Set not possible, invalid or unknown.";
                }
                if(!rejected) throw "invalid event value accepted";
            }
            event.value = 123;
            if(event.value !== "123") throw "number value did not convert";
        "#,
            ))
            .unwrap();
        assert_eq!(*channels.value.borrow(), script("123"));
        let event = runtime.new_event_context();
        event.on_doc_open(&script("source.js"));
        event
            .run_script(&script(
                r#"
            if(event.name !== "Open" || event.type !== "Doc" || event.targetName !== "source.js")
                throw "incorrect document setup";
            let rejected = false;
            try {event.value} catch(error) {
                rejected = typeof error === "string" && error === "event.value: Bad event type.";
            }
            if(!rejected) throw "document value unexpectedly present";
        "#,
            ))
            .unwrap();
    }
}
