// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Model-facing `cjx_exclgroup.{h,cpp}`. Default/raw values share CJX_Object.
use super::{
    cfxjse_engine::{node_data, CfxJseEngine},
    cjx_node::exec_single_event_by_name,
    cjx_subform::{define, exec_deep, parameter_error, readonly, to_wide},
};
use crate::xfa::fxfa::{
    cxfa_eventparam::XfaEventType, fxfa_basic::XfaElement, parser::cxfa_node::CxfaNode,
};
use boa_engine::{Context, JsObject, JsResult, JsValue};
pub(crate) fn define_exclgroup_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    define(
        object,
        context,
        &[
            ("execCalculate", 0, exec_calculate),
            ("execEvent", 1, exec_event),
            ("execInitialize", 0, exec_initialize),
            ("execValidate", 0, exec_validate),
            ("selectedMember", 0, selected_member),
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
        return Err(parameter_error("ExclGroup", "execEvent"));
    }
    let name = to_wide(&args[0], context)?;
    let runtime = binding.runtime()?;
    exec_single_event_by_name(
        &runtime,
        &binding.node,
        &name,
        XfaElement::ExclGroup,
        context,
    )?;
    Ok(JsValue::undefined())
}
fn selected_member(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    // The native entrypoint accepts no arguments; its setter branch is dead.
    if !args.is_empty() {
        return Err(parameter_error("ExclGroup", "selectedMember"));
    }
    if !binding.node.is_widget_ready() {
        return Ok(JsValue::null());
    }
    match binding.node.get_selected_member() {
        Some(node) => Ok(binding
            .runtime()?
            .get_or_create_binding(&node, context)?
            .into()),
        None => Ok(JsValue::null()),
    }
}
pub(crate) fn read_property(
    _runtime: &CfxJseEngine,
    _node: &CxfaNode,
    callback: &str,
    _context: &mut Context,
) -> JsResult<JsValue> {
    match callback {
        // Both source getters intentionally leave the value undefined.
        "CJX_ExclGroup::transient" | "CJX_ExclGroup::errorText" => Ok(JsValue::undefined()),
        _ => Err(readonly("ExclGroup", callback)),
    }
}
pub(crate) fn write_property(
    _runtime: &CfxJseEngine,
    _node: &CxfaNode,
    callback: &str,
    _value: &JsValue,
    _context: &mut Context,
) -> JsResult<()> {
    if callback == "CJX_ExclGroup::transient" {
        Ok(())
    } else {
        Err(readonly("ExclGroup", callback))
    }
}
