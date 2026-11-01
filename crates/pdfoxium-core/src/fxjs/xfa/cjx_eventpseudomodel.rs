// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cjx_eventpseudomodel.{h,cpp}` over the engine's actual scoped event record.
//! The weak native holder also represents CScript_EventPseudoModel's Object /
//! EventPseudoModel / document facade, without allocating a fictitious XFA node.
use super::cfxjse_engine::CfxJseEngine;
use crate::{
    fxcrt::fx_string::utf16_encode,
    fxjs::{cfxjs_engine::CfxJsEngine, cjs_event_context::wide_from_utf16},
    xfa::fxfa::{
        cxfa_eventparam::{CxfaEventParam, XfaEventType},
        parser::cxfa_node::CxfaNode,
    },
};
use boa_engine::{
    js_string,
    object::{FunctionObjectBuilder, ObjectInitializer},
    property::PropertyDescriptor,
    Context, JsData, JsNativeError, JsObject, JsResult, JsString, JsValue, NativeFunction,
};
use boa_gc::{Finalize, Trace};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// Optional actual widget-event service. A model-change observer is not this
/// service. Implementations release event borrows before synchronously reentering
/// the engine; emit calls them through its existing context callback lease.
pub trait XfaEventHost {
    fn handle_widget_event(
        &self,
        target: Option<&CxfaNode>,
        event: Rc<RefCell<CxfaEventParam>>,
    ) -> JsResult<()>;
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum XfaEvent {
    Change = 0,
    CommitKey,
    FullText,
    Keydown,
    Modifier,
    NewContentType,
    NewText,
    PreviousContentType,
    PreviousText,
    Reenter,
    SelectionEnd,
    SelectionStart,
    Shift,
    SoapFaultCode,
    SoapFaultString,
    Target,
    CancelAction,
}
#[derive(Trace, Finalize, JsData)]
struct EventBinding {
    // No Boa values are hidden in the Rust runtime weak link.
    #[unsafe_ignore_trace]
    runtime: Weak<CfxJseEngine>,
}
fn runtime(this: &JsValue, inherited: bool) -> JsResult<Rc<CfxJseEngine>> {
    let mut object = this.as_object();
    while let Some(candidate) = object {
        let link = candidate
            .downcast_ref::<EventBinding>()
            .map(|data| data.runtime.clone());
        if let Some(link) = link {
            return link.upgrade().ok_or_else(|| {
                JsNativeError::reference()
                    .with_message("XFA event script context has expired")
                    .into()
            });
        }
        if !inherited {
            break;
        }
        object = candidate.prototype();
    }
    Err(JsNativeError::typ()
        .with_message("Invalid XFA event receiver")
        .into())
}
pub(crate) fn event_object(
    runtime: &Rc<CfxJseEngine>,
    context: &mut Context,
) -> JsResult<JsObject> {
    let object = ObjectInitializer::with_native_data(
        EventBinding {
            runtime: Rc::downgrade(runtime),
        },
        context,
    )
    .build();
    let fields = &[
        ("cancelAction", XfaEvent::CancelAction),
        ("change", XfaEvent::Change),
        ("commitKey", XfaEvent::CommitKey),
        ("fullText", XfaEvent::FullText),
        ("keyDown", XfaEvent::Keydown),
        ("modifier", XfaEvent::Modifier),
        ("newContentType", XfaEvent::NewContentType),
        ("newText", XfaEvent::NewText),
        ("prevContentType", XfaEvent::PreviousContentType),
        ("prevText", XfaEvent::PreviousText),
        ("reenter", XfaEvent::Reenter),
        ("selEnd", XfaEvent::SelectionEnd),
        ("selStart", XfaEvent::SelectionStart),
        ("shift", XfaEvent::Shift),
        ("soapFaultCode", XfaEvent::SoapFaultCode),
        ("soapFaultString", XfaEvent::SoapFaultString),
        ("target", XfaEvent::Target),
    ];
    for &(name, field) in fields {
        // Captures contain only the Copy field tag and setter bit, never a GC
        // object or a borrowed Context/event record.
        let getter = NativeFunction::from_copy_closure(move |this, _, context| {
            property(this, &[], context, field, false)
        });
        let setter = NativeFunction::from_copy_closure(move |this, args, context| {
            property(this, args, context, field, true)
        });
        let getter = FunctionObjectBuilder::new(context.realm(), getter)
            .name(JsString::from(format!("get {name}")))
            .length(0)
            .constructor(false)
            .build();
        let setter = FunctionObjectBuilder::new(context.realm(), setter)
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
    for (name, callback) in [
        (
            "emit",
            emit as boa_engine::native_function::NativeFunctionPointer,
        ),
        ("reset", reset),
    ] {
        let function =
            FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(callback))
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
    let getter =
        FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(class_name))
            .name(js_string!("get className"))
            .constructor(false)
            .build();
    let setter =
        FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(set_class_name))
            .name(js_string!("set className"))
            .constructor(false)
            .build();
    object.define_property_or_throw(
        js_string!("className"),
        PropertyDescriptor::builder()
            .get(getter)
            .set(setter)
            .enumerable(true)
            .configurable(true),
        context,
    )?;
    crate::fxjs::boa_adapters::finish_class(&object, context)?;
    Ok(object)
}
fn class_name(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    runtime(this, true)?;
    Ok(js_string!("eventPseudoModel").into())
}
fn set_class_name(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    runtime(this, true)?;
    Err(JsNativeError::typ()
        .with_message("Invalid property set operation.")
        .into())
}
fn emit(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let runtime = runtime(this, false)?;
    let Some(event) = runtime.current_event() else {
        return Ok(JsValue::undefined());
    };
    let Some(host) = runtime.event_host() else {
        return Ok(JsValue::undefined());
    };
    let target = runtime.event_target();
    CfxJsEngine::with_host_callback(context, || host.handle_widget_event(target.as_ref(), event))?;
    Ok(JsValue::undefined())
}
fn reset(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let runtime = runtime(this, false)?;
    if let Some(event) = runtime.current_event() {
        *event.borrow_mut() = CxfaEventParam::new(XfaEventType::Unknown);
    }
    Ok(JsValue::undefined())
}
fn selection_end(event: &mut CxfaEventParam) {
    let length = event.prev_text.len().min(i32::MAX as usize) as i32;
    event.sel_end = event.sel_end.clamp(0, length);
    event.sel_start = event.sel_start.min(event.sel_end);
}
fn selection_start(event: &mut CxfaEventParam) {
    let length = event.prev_text.len().min(i32::MAX as usize) as i32;
    event.sel_start = event.sel_start.clamp(0, length);
    event.sel_end = event.sel_end.max(event.sel_start);
}
enum EventValue {
    Undefined,
    String(Vec<u32>),
    Integer(i32),
    Boolean(bool),
}
fn property(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
    field: XfaEvent,
    setting: bool,
) -> JsResult<JsValue> {
    let runtime = runtime(this, true)?;
    // The native contract deliberately ignores setters for all other fields,
    // including newText and target, without coercing the supplied value.
    if setting
        && !matches!(
            field,
            XfaEvent::CancelAction
                | XfaEvent::Change
                | XfaEvent::SelectionStart
                | XfaEvent::SelectionEnd
        )
    {
        return Ok(JsValue::undefined());
    }
    let Some(event) = runtime.current_event() else {
        return Ok(JsValue::undefined());
    };
    if setting {
        let value = args.first().cloned().unwrap_or_default();
        match field {
            XfaEvent::CancelAction => {
                let value = value.to_boolean();
                event.borrow_mut().cancel_action = value;
            }
            XfaEvent::Change => {
                // Retain the actual outer event cell, but no borrow, while
                // coercion runs script or enters a nested event scope.
                let value = wide_from_utf16(&value.to_string(context)?.to_vec());
                event.borrow_mut().change = value;
            }
            XfaEvent::SelectionEnd => {
                let value = value.to_i32(context)?;
                let mut event = event.borrow_mut();
                event.sel_end = value;
                selection_end(&mut event);
            }
            XfaEvent::SelectionStart => {
                let value = value.to_i32(context)?;
                let mut event = event.borrow_mut();
                event.sel_start = value;
                selection_start(&mut event);
            }
            _ => unreachable!("only four native writable event properties"),
        }
        return Ok(JsValue::undefined());
    }
    let value = match field {
        XfaEvent::SelectionEnd => {
            let mut event = event.borrow_mut();
            let result = event.sel_end;
            selection_end(&mut event);
            EventValue::Integer(result)
        }
        XfaEvent::SelectionStart => {
            let mut event = event.borrow_mut();
            let result = event.sel_start;
            selection_start(&mut event);
            EventValue::Integer(result)
        }
        _ => {
            let event = event.borrow();
            match field {
                XfaEvent::CancelAction => EventValue::Boolean(event.cancel_action),
                XfaEvent::Change => EventValue::String(event.change.clone()),
                XfaEvent::CommitKey => EventValue::Integer(event.commit_key),
                XfaEvent::FullText => EventValue::String(event.full_text.clone()),
                XfaEvent::Keydown => EventValue::Boolean(event.key_down),
                XfaEvent::Modifier => EventValue::Boolean(event.modifier),
                XfaEvent::NewContentType => EventValue::String(event.new_content_type.clone()),
                XfaEvent::NewText => EventValue::String(event.get_new_text()),
                XfaEvent::PreviousContentType => {
                    EventValue::String(event.prev_content_type.clone())
                }
                XfaEvent::PreviousText => EventValue::String(event.prev_text.clone()),
                XfaEvent::Reenter => EventValue::Boolean(event.reenter),
                XfaEvent::Shift => EventValue::Boolean(event.shift),
                XfaEvent::SoapFaultCode => EventValue::String(event.soap_fault_code.clone()),
                XfaEvent::SoapFaultString => EventValue::String(event.soap_fault_string.clone()),
                // This native callback has an empty Target switch case. It
                // does not expose a fabricated node or form-fill widget.
                XfaEvent::Target => EventValue::Undefined,
                XfaEvent::SelectionEnd | XfaEvent::SelectionStart => unreachable!(),
            }
        }
    };
    // Release every event borrow before allocating a Boa string/value.
    Ok(match value {
        EventValue::Undefined => JsValue::undefined(),
        EventValue::String(value) => JsString::from(utf16_encode(&value).as_slice()).into(),
        EventValue::Integer(value) => value.into(),
        EventValue::Boolean(value) => value.into(),
    })
}

#[cfg(test)]
#[path = "cjx_eventpseudomodel_tests.rs"]
mod tests;
