// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cjx_form.{h,cpp}`: real model remerge, persisted native formNodes lists,
//! and native FF event ordering through the existing optional host services.
use super::{
    cfxjse_engine::node_data,
    cjx_list::new_list_binding,
    cjx_subform::{define, exec_deep, parameter_error, string, to_wide},
};
use crate::{
    fxjs::{cfxjs_engine::CfxJsEngine, cjs_field::binding_error, js_resources::JsMessage},
    xfa::fxfa::{
        cxfa_eventparam::XfaEventType,
        fxfa_basic::XfaAttribute,
        parser::{cxfa_arraynodelist::CxfaArrayNodeList, cxfa_node::CxfaNode},
    },
};
use boa_engine::{Context, JsNativeError, JsObject, JsResult, JsValue};
pub(crate) fn define_form_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    define(
        object,
        context,
        &[
            ("execCalculate", 0, exec_calculate),
            ("execInitialize", 0, exec_initialize),
            ("execValidate", 0, exec_validate),
            ("formNodes", 1, form_nodes),
            ("recalculate", 1, recalculate),
            ("remerge", 0, remerge),
        ],
    )
}
fn form_nodes(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() != 1 {
        return Err(parameter_error("Form", "formNodes"));
    }
    let runtime = binding.runtime()?;
    if runtime.node_from_value(&args[0]).is_none() {
        return Err(binding_error("Form", "formNodes", JsMessage::ValueError));
    }
    // The native function validates the supplied data node, then deliberately
    // returns a newly persisted empty list rather than deriving binding items.
    let list = CxfaArrayNodeList::new(runtime.document());
    runtime
        .document()
        .get_node_owner()
        .persist_list(list.clone());
    new_list_binding(&runtime, list, context).map(Into::into)
}
pub(super) fn remerge(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if !args.is_empty() {
        return Err(parameter_error(binding.node.get_class_name(), "remerge"));
    }
    let runtime = binding.runtime()?;
    // Full native DoDataRemerge includes relayout. Do not silently substitute
    // the explicitly separate model-only operation when no FF service exists.
    let layout = runtime.data_merge_layout_host().ok_or_else(|| {
        JsNativeError::typ().with_message("XFA remerge requires the document layout service")
    })?;
    CfxJsEngine::with_host_callback(context, || {
        runtime
            .document()
            .do_data_remerge_with_resolver(runtime.resolver(), layout.as_ref())
    })
    .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
    Ok(JsValue::undefined())
}
fn exec_initialize(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exec_deep(this, args, context, XfaEventType::Initialize, true)
}
fn exec_calculate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exec_deep(this, args, context, XfaEventType::Calculate, true)
}
fn exec_validate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exec_deep(this, args, context, XfaEventType::Validate, true)
}
fn recalculate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    let runtime = binding.runtime()?;
    // The active event is inspected before even validating the parameter count.
    if runtime.current_event().is_some_and(|event| {
        matches!(
            event.borrow().event_type,
            XfaEventType::Calculate | XfaEventType::InitCalculate
        )
    }) {
        return Ok(JsValue::undefined());
    }
    if args.len() != 1 {
        return Err(parameter_error("Form", "recalculate"));
    }
    let Some(host) = runtime.widget_script_host() else {
        return Ok(JsValue::undefined());
    };
    if args[0].to_i32(context)? != 0 {
        return Ok(JsValue::undefined());
    }
    // Hold no model/event/host cell borrow while any real event service runs.
    CfxJsEngine::with_host_callback(context, || {
        host.exec_deep_first(&binding.node, XfaEventType::Calculate, false, true)?;
        host.exec_deep_first(&binding.node, XfaEventType::Validate, false, true)?;
        host.exec_deep_first(&binding.node, XfaEventType::Ready, true, true)?;
        Ok::<(), boa_engine::JsError>(())
    })?;
    Ok(JsValue::undefined())
}
pub(crate) fn read_checksum(node: &CxfaNode) -> JsValue {
    string(
        &node
            .try_attribute(XfaAttribute::Checksum, false)
            .unwrap_or_default(),
    )
}
pub(crate) fn write_checksum(
    node: &CxfaNode,
    value: &JsValue,
    context: &mut Context,
) -> JsResult<()> {
    let value = to_wide(value, context)?;
    CfxJsEngine::with_host_callback(context, || {
        node.set_attribute_by_enum(XfaAttribute::Checksum, &value, false)
    });
    Ok(())
}
#[cfg(test)]
#[path = "cjx_model_wrappers_tests.rs"]
mod tests;
