// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Model-facing `cjx_subform.{h,cpp}` and its optional FF script service.
use super::{
    cfxjse_engine::{node_data, CfxJseEngine},
    cjx_node::exec_single_event_by_name,
};
use crate::{
    fxcrt::fx_string::utf16_encode,
    fxjs::{
        cfxjs_engine::CfxJsEngine, cjs_event_context::wide_from_utf16, cjs_field::binding_error,
        js_resources::JsMessage,
    },
    xfa::fxfa::{
        cxfa_eventparam::XfaEventType,
        fxfa::XfaEventError,
        fxfa_basic::{XfaAttribute, XfaElement},
        parser::cxfa_node::CxfaNode,
    },
};
use boa_engine::{
    native_function::NativeFunctionPointer, object::FunctionObjectBuilder,
    property::PropertyDescriptor, Context, JsObject, JsResult, JsString, JsValue,
};

/// Supplied by an actual document/FF host. The model runtime installs no
/// substitute event dispatcher or validation scheduler.
pub trait XfaWidgetScriptHost {
    fn exec_deep_first(
        &self,
        node: &CxfaNode,
        event: XfaEventType,
        form_ready: bool,
        recursive: bool,
    ) -> JsResult<XfaEventError>;
    fn add_calc_validate(&self, node: &CxfaNode) -> JsResult<()>;
}
pub(super) fn parameter_error(class: &str, method: &str) -> boa_engine::JsError {
    binding_error(class, method, JsMessage::ParamError)
}
pub(super) fn readonly(class: &str, property: &str) -> boa_engine::JsError {
    binding_error(class, property, JsMessage::ReadOnlyError)
}
pub(super) fn to_wide(value: &JsValue, context: &mut Context) -> JsResult<Vec<u32>> {
    Ok(wide_from_utf16(&value.to_string(context)?.to_vec()))
}
pub(super) fn string(value: &[u32]) -> JsValue {
    JsString::from(utf16_encode(value).as_slice()).into()
}
pub(super) fn define(
    object: &JsObject,
    context: &mut Context,
    methods: &[(&str, usize, NativeFunctionPointer)],
) -> JsResult<()> {
    let class = if let Some(kind) = super::cjx_pseudomodel::binding_kind(object) {
        super::cjx_bindings::MethodClass::Pseudo(kind)
    } else {
        super::cjx_bindings::MethodClass::Element(
            node_data(&object.clone().into(), context)?
                .node
                .get_element_type(),
        )
    };
    for &(name, length, callback) in methods {
        let function = FunctionObjectBuilder::new(
            context.realm(),
            super::cjx_bindings::native_method(callback, class),
        )
        .name(JsString::from(name))
        .length(length)
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
    Ok(())
}
pub(super) fn exec_deep(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
    event: XfaEventType,
    recursive: bool,
) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if !args.is_empty() {
        return Err(parameter_error(binding.node.get_class_name(), "execEvent"));
    }
    let runtime = binding.runtime()?;
    let Some(host) = runtime.widget_script_host() else {
        return Ok(if event == XfaEventType::Validate {
            false.into()
        } else {
            JsValue::undefined()
        });
    };
    let result = CfxJsEngine::with_host_callback(context, || {
        host.exec_deep_first(&binding.node, event, false, recursive)
    })?;
    Ok(if event == XfaEventType::Validate {
        (result != XfaEventError::kError).into()
    } else {
        JsValue::undefined()
    })
}
pub(crate) fn define_subform_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    define(
        object,
        context,
        &[
            ("execCalculate", 0, exec_calculate),
            ("execEvent", 1, exec_event),
            ("execInitialize", 0, exec_initialize),
            ("execValidate", 0, exec_validate),
        ],
    )
}
fn exec_calculate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exec_deep(this, args, context, XfaEventType::Calculate, true)
}
fn exec_initialize(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exec_deep(this, args, context, XfaEventType::Initialize, true)
}
fn exec_validate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exec_deep(this, args, context, XfaEventType::Validate, true)
}
fn exec_event(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() != 1 {
        return Err(parameter_error("Subform", "execEvent"));
    }
    let name = to_wide(&args[0], context)?;
    let runtime = binding.runtime()?;
    exec_single_event_by_name(&runtime, &binding.node, &name, XfaElement::Subform, context)?;
    Ok(JsValue::undefined())
}
pub(crate) fn read_property(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    callback: &str,
    context: &mut Context,
) -> JsResult<JsValue> {
    match callback {
        "CJX_Subform::locale" => Ok(string(&node.get_locale_name().unwrap_or_default())),
        "CJX_Subform::instanceManager" => {
            let name = node.get_cdata(XfaAttribute::Name);
            let mut sibling = node.prev_sibling();
            while let Some(previous) = sibling {
                if previous.get_element_type() == XfaElement::InstanceManager {
                    return if previous
                        .get_cdata(XfaAttribute::Name)
                        .strip_prefix(&[95])
                        .is_some_and(|suffix| suffix == name)
                    {
                        Ok(runtime.get_or_create_binding(&previous, context)?.into())
                    } else {
                        Ok(JsValue::null())
                    };
                }
                sibling = previous.prev_sibling();
            }
            Ok(JsValue::null())
        }
        _ => Err(readonly("Subform", callback)),
    }
}
pub(crate) fn write_property(
    _runtime: &CfxJseEngine,
    node: &CxfaNode,
    callback: &str,
    value: &JsValue,
    context: &mut Context,
) -> JsResult<()> {
    if callback != "CJX_Subform::locale" {
        return Err(readonly("Subform", callback));
    }
    let value = to_wide(value, context)?;
    CfxJsEngine::with_host_callback(context, || {
        node.set_cdata_and_notify(XfaAttribute::Locale, &value, true)
    });
    Ok(())
}
